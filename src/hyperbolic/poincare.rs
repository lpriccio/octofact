use std::f64::consts::PI;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// Complex number with f64 precision for hyperbolic geometry.
#[derive(Clone, Copy, Debug)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}

impl Complex {
    pub const ZERO: Complex = Complex { re: 0.0, im: 0.0 };
    pub const ONE: Complex = Complex { re: 1.0, im: 0.0 };

    pub fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    pub fn from_polar(r: f64, theta: f64) -> Self {
        Self {
            re: r * theta.cos(),
            im: r * theta.sin(),
        }
    }

    pub fn conj(self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
        }
    }

    pub fn norm_sq(self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    pub fn abs(self) -> f64 {
        self.norm_sq().sqrt()
    }
}

impl Add for Complex {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.re + rhs.re, self.im + rhs.im)
    }
}

impl Sub for Complex {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.re - rhs.re, self.im - rhs.im)
    }
}

impl Mul for Complex {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self::new(
            self.re * rhs.re - self.im * rhs.im,
            self.re * rhs.im + self.im * rhs.re,
        )
    }
}

impl Div for Complex {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        let d = rhs.norm_sq();
        Self::new(
            (self.re * rhs.re + self.im * rhs.im) / d,
            (self.im * rhs.re - self.re * rhs.im) / d,
        )
    }
}

impl Neg for Complex {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.re, -self.im)
    }
}

impl Mul<f64> for Complex {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self::new(self.re * rhs, self.im * rhs)
    }
}

/// Mobius transformation in SU(1,1) form: T(z) = (a*z + b) / (conj(b)*z + conj(a))
/// Invariant: |a|^2 - |b|^2 = 1
#[derive(Clone, Copy, Debug)]
pub struct Mobius {
    pub a: Complex,
    pub b: Complex,
}

impl Mobius {
    pub fn identity() -> Self {
        Self {
            a: Complex::ONE,
            b: Complex::ZERO,
        }
    }

    /// Apply this transform to a point on the Poincare disk.
    pub fn apply(&self, z: Complex) -> Complex {
        let num = self.a * z + self.b;
        let den = self.b.conj() * z + self.a.conj();
        num / den
    }

    /// Compose two Mobius transforms (self after other): self.compose(other) = self(other(z)).
    /// Result is normalized to maintain SU(1,1).
    pub fn compose(&self, other: &Mobius) -> Mobius {
        // SU(1,1) matrix multiplication:
        // [a1 b1] [a2 b2]   [a1*a2+b1*conj(b2)  a1*b2+b1*conj(a2)]
        // [b1* a1*] [b2* a2*] = [(a1*a2+b1*conj(b2))*  ...]
        let new_a = self.a * other.a + self.b * other.b.conj();
        let new_b = self.a * other.b + self.b * other.a.conj();
        Mobius {
            a: new_a,
            b: new_b,
        }
        .normalized()
    }

    /// Inverse: {conj(a), -b}
    pub fn inverse(&self) -> Mobius {
        Mobius {
            a: self.a.conj(),
            b: -self.b,
        }
    }

    /// Normalize to maintain |a|^2 - |b|^2 = 1.
    pub fn normalized(self) -> Mobius {
        let det = self.a.norm_sq() - self.b.norm_sq();
        let scale = 1.0 / det.sqrt();
        Mobius {
            a: self.a * scale,
            b: self.b * scale,
        }
    }
}

/// Returns the 8 vertices of the canonical {8,3} octagon on the Poincare disk.
/// Vertices at r_disk * exp(i * (pi/8 + k*pi/4)) for k = 0..7.
pub fn canonical_octagon() -> [Complex; 8] {
    let r_disk = octagon_disk_radius();
    let mut verts = [Complex::ZERO; 8];
    for (k, vert) in verts.iter_mut().enumerate() {
        let angle = PI / 8.0 + k as f64 * PI / 4.0;
        *vert = Complex::from_polar(r_disk, angle);
    }
    verts
}

/// Disk radius of the {8,3} octagon (circumradius: center to vertex).
/// cosh(chi) = cot(pi/p) * cot(pi/q) = cot(pi/8) * cot(pi/3), r = tanh(chi/2)
pub fn octagon_disk_radius() -> f64 {
    let cosh_chi = 1.0 / (PI / 8.0).tan() * 1.0 / (PI / 3.0).tan();
    let chi = cosh_chi.acosh();
    (chi / 2.0).tanh()
}

/// Hyperbolic distance between two points on the Poincare disk.
/// d(z1,z2) = 2 * atanh(|z1-z2| / |1 - conj(z1)*z2|)
pub fn poincare_distance(z1: Complex, z2: Complex) -> f64 {
    let num = (z1 - z2).abs();
    let den = (Complex::ONE - z1.conj() * z2).abs();
    if den < 1e-15 {
        return f64::MAX;
    }
    let ratio = (num / den).min(0.99999); // clamp for numerical safety
    2.0 * ratio.atanh()
}

/// Center-to-center hyperbolic distance D = 2*psi for adjacent tiles in {8,3}.
/// Inradius: cosh(psi) = cos(pi/q) / sin(pi/p) = cos(pi/3) / sin(pi/8).
fn half_edge_distance() -> f64 {
    let cosh_psi = (PI / 3.0).cos() / (PI / 8.0).sin();
    let cosh_d = 2.0 * cosh_psi * cosh_psi - 1.0;
    cosh_d.acosh()
}

/// Center-to-center hyperbolic distance between adjacent tiles in {8,3}.
pub fn center_to_center_distance() -> f64 {
    half_edge_distance()
}

/// Returns the 8 Mobius transforms that map the origin tile to each neighbor in {8,3}.
/// Uses the inradius to compute center-to-center distance D = 2*psi.
pub fn neighbor_transforms() -> [Mobius; 8] {
    let d = half_edge_distance();

    let a = (d / 2.0).cosh();
    let sinh_half_d = (d / 2.0).sinh();

    let a_c = Complex::new(a, 0.0);

    let mut transforms = [Mobius::identity(); 8];
    for (k, xform) in transforms.iter_mut().enumerate() {
        let angle = k as f64 * PI / 4.0;
        let b_c = Complex::from_polar(sinh_half_d, angle);
        *xform = Mobius { a: a_c, b: b_c };
    }
    transforms
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-10;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    fn complex_approx_eq(a: Complex, b: Complex) -> bool {
        (a - b).abs() < EPS
    }

    #[test]
    fn test_complex_basic() {
        let a = Complex::new(3.0, 4.0);
        assert!(approx_eq(a.abs(), 5.0));
        assert!(approx_eq(a.norm_sq(), 25.0));

        let b = a.conj();
        assert!(approx_eq(b.re, 3.0));
        assert!(approx_eq(b.im, -4.0));
    }

    #[test]
    fn test_complex_from_polar() {
        let z = Complex::from_polar(1.0, PI / 4.0);
        assert!(approx_eq(z.re, (PI / 4.0).cos()));
        assert!(approx_eq(z.im, (PI / 4.0).sin()));
    }

    #[test]
    fn test_complex_arithmetic() {
        let a = Complex::new(1.0, 2.0);
        let b = Complex::new(3.0, 4.0);

        let sum = a + b;
        assert!(approx_eq(sum.re, 4.0));
        assert!(approx_eq(sum.im, 6.0));

        let prod = a * b;
        // (1+2i)(3+4i) = 3+4i+6i+8i^2 = -5+10i
        assert!(approx_eq(prod.re, -5.0));
        assert!(approx_eq(prod.im, 10.0));

        let div = a / b;
        // (1+2i)/(3+4i) = (1+2i)(3-4i)/25 = (11+2i)/25
        assert!(approx_eq(div.re, 11.0 / 25.0));
        assert!(approx_eq(div.im, 2.0 / 25.0));
    }

    #[test]
    fn test_mobius_identity() {
        let id = Mobius::identity();
        let z = Complex::new(0.3, -0.2);
        let w = id.apply(z);
        assert!(complex_approx_eq(z, w));
    }

    #[test]
    fn test_mobius_inverse_roundtrip() {
        let m = Mobius {
            a: Complex::new(1.2, 0.3),
            b: Complex::new(0.1, 0.4),
        }
        .normalized();

        let inv = m.inverse();
        let z = Complex::new(0.1, 0.2);
        let w = m.apply(z);
        let z2 = inv.apply(w);
        assert!(complex_approx_eq(z, z2));
    }

    #[test]
    fn test_mobius_composition_associativity() {
        let a = Mobius {
            a: Complex::new(1.1, 0.2),
            b: Complex::new(0.1, 0.3),
        }
        .normalized();
        let b = Mobius {
            a: Complex::new(1.3, -0.1),
            b: Complex::new(0.2, 0.1),
        }
        .normalized();
        let c = Mobius {
            a: Complex::new(1.0, 0.4),
            b: Complex::new(0.15, -0.2),
        }
        .normalized();

        let z = Complex::new(0.1, 0.05);

        // (a . b) . c == a . (b . c)
        let ab_c = a.compose(&b).compose(&c);
        let a_bc = a.compose(&b.compose(&c));

        let r1 = ab_c.apply(z);
        let r2 = a_bc.apply(z);
        assert!(complex_approx_eq(r1, r2));
    }

    #[test]
    fn test_octagon_disk_radius() {
        let r = octagon_disk_radius();
        assert!(
            (r - 0.4056).abs() < 0.001,
            "octagon disk radius {r} should be ~0.4056"
        );
    }

    #[test]
    fn test_canonical_octagon_vertices() {
        let verts = canonical_octagon();
        let r = octagon_disk_radius();
        for (i, v) in verts.iter().enumerate() {
            assert!(
                (v.abs() - r).abs() < EPS,
                "vertex {i} radius {} != {r}",
                v.abs()
            );
        }
        // Check they're evenly spaced
        for i in 0..8 {
            let j = (i + 1) % 8;
            let d1 = (verts[i] - verts[j]).abs();
            let d2 = (verts[0] - verts[1]).abs();
            assert!(
                (d1 - d2).abs() < EPS,
                "edge {i}-{j} length {d1} != {d2}"
            );
        }
    }

    #[test]
    fn test_neighbor_transforms_distinct() {
        let transforms = neighbor_transforms();
        let centers: Vec<Complex> = transforms.iter().map(|t| t.apply(Complex::ZERO)).collect();

        // All 8 centers should be distinct
        for i in 0..8 {
            for j in (i + 1)..8 {
                let d = (centers[i] - centers[j]).abs();
                assert!(d > 0.01, "neighbor centers {i} and {j} too close: {d}");
            }
        }

        // All centers should be inside the unit disk
        for (i, c) in centers.iter().enumerate() {
            assert!(c.abs() < 1.0, "neighbor {i} center outside disk: {}", c.abs());
        }

        // All should be at the same distance from origin
        let d0 = centers[0].abs();
        for (i, c) in centers.iter().enumerate().skip(1) {
            assert!(
                (c.abs() - d0).abs() < EPS,
                "neighbor {i} distance {} != {d0}",
                c.abs()
            );
        }
    }

    #[test]
    fn test_neighbor_center_distance() {
        let transforms = neighbor_transforms();
        let c0 = transforms[0].apply(Complex::ZERO);
        // The center should be at the expected hyperbolic distance
        // In the Poincare disk, |c| = tanh(d/2) where d is the hyperbolic distance
        let r = c0.abs();
        let hyp_dist = 2.0 * r.atanh();
        // Expected: D = 2*psi, cosh(D) = 2*cosh(psi)^2 - 1
        // Inradius: cosh(psi) = cos(pi/q) / sin(pi/p) = cos(pi/3) / sin(pi/8)
        let cosh_psi = (PI / 3.0).cos() / (PI / 8.0).sin();
        let expected_cosh_d = 2.0 * cosh_psi * cosh_psi - 1.0;
        let expected_d = expected_cosh_d.acosh();
        assert!(
            (hyp_dist - expected_d).abs() < 1e-6,
            "neighbor distance {hyp_dist} != expected {expected_d}"
        );
    }
}
