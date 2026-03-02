use std::f64::consts::PI;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// Configuration for a {p,q} hyperbolic tiling.
/// p = sides per polygon, q = polygons meeting at each vertex.
/// Requires (p-2)(q-2) > 4 for hyperbolicity.
#[derive(Clone, Copy, Debug)]
pub struct TilingConfig {
    pub p: u32,
    pub q: u32,
}

impl TilingConfig {
    pub fn new(p: u32, q: u32) -> Self {
        assert!(
            (p - 2) * (q - 2) > 4,
            "{{p,q}} = {{{p},{q}}} is not hyperbolic: (p-2)(q-2) = {} <= 4",
            (p - 2) * (q - 2)
        );
        Self { p, q }
    }

    pub fn vertex_angle_step(&self) -> f64 {
        2.0 * PI / self.p as f64
    }

    pub fn vertex_angle_offset(&self) -> f64 {
        PI / self.p as f64
    }
}

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
        let scale = 1.0 / det.abs().sqrt();
        Mobius {
            a: self.a * scale,
            b: self.b * scale,
        }
    }
}

/// Returns the p vertices of the canonical {p,q} polygon on the Poincare disk.
/// Vertices at r_disk * exp(i * (pi/p + k*2*pi/p)) for k = 0..p-1.
pub fn canonical_polygon(cfg: &TilingConfig) -> Vec<Complex> {
    let r_disk = polygon_disk_radius(cfg);
    (0..cfg.p)
        .map(|k| {
            let angle = cfg.vertex_angle_offset() + k as f64 * cfg.vertex_angle_step();
            Complex::from_polar(r_disk, angle)
        })
        .collect()
}

/// Disk radius of the {p,q} polygon (circumradius: center to vertex).
/// cosh(chi) = cot(pi/p) * cot(pi/q), r = tanh(chi/2)
pub fn polygon_disk_radius(cfg: &TilingConfig) -> f64 {
    let cosh_chi = 1.0 / (PI / cfg.p as f64).tan() * 1.0 / (PI / cfg.q as f64).tan();
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

/// Center-to-center hyperbolic distance D = 2*psi for adjacent tiles in {p,q}.
/// Inradius: cosh(psi) = cos(pi/q) / sin(pi/p).
pub fn half_edge_distance(cfg: &TilingConfig) -> f64 {
    let cosh_psi = (PI / cfg.q as f64).cos() / (PI / cfg.p as f64).sin();
    let cosh_d = 2.0 * cosh_psi * cosh_psi - 1.0;
    cosh_d.acosh()
}

/// Center-to-center hyperbolic distance between adjacent tiles in {p,q}.
pub fn center_to_center_distance(cfg: &TilingConfig) -> f64 {
    half_edge_distance(cfg)
}

/// Returns two sets of p Mobius transforms for even/odd parity tiles.
/// For even p, both sets are identical (pure translations).
/// For odd p, each set includes a ±π/p rotation to align edges correctly.
///
/// `[0]` = transforms for even-parity tiles (origin, grandchildren, ...)
/// `[1]` = transforms for odd-parity tiles (children, great-grandchildren, ...)
pub fn neighbor_transforms(cfg: &TilingConfig) -> [Vec<Mobius>; 2] {
    let d = half_edge_distance(cfg);
    let cosh_half = (d / 2.0).cosh();
    let sinh_half = (d / 2.0).sinh();
    let step = cfg.vertex_angle_step();

    if cfg.p.is_multiple_of(2) {
        // Even p: pure translations, both parity sets identical
        let xforms: Vec<Mobius> = (0..cfg.p)
            .map(|k| {
                let angle = k as f64 * step;
                Mobius {
                    a: Complex::new(cosh_half, 0.0),
                    b: Complex::from_polar(sinh_half, angle),
                }
            })
            .collect();
        [xforms.clone(), xforms]
    } else {
        // Odd p: compose translation with rotation(±π/p).
        // T_k ∘ R(α): a = cosh(D/2) * e^(iα/2), b = sinh(D/2) * e^(i(θ_k - α/2))
        let alpha = PI / cfg.p as f64;
        let half_alpha = alpha / 2.0;

        let even_xforms: Vec<Mobius> = (0..cfg.p)
            .map(|k| {
                let theta_k = k as f64 * step;
                Mobius {
                    a: Complex::from_polar(cosh_half, half_alpha),
                    b: Complex::from_polar(sinh_half, theta_k - half_alpha),
                }
            })
            .collect();

        let odd_xforms: Vec<Mobius> = (0..cfg.p)
            .map(|k| {
                let theta_k = k as f64 * step;
                Mobius {
                    a: Complex::from_polar(cosh_half, -half_alpha),
                    b: Complex::from_polar(sinh_half, theta_k + half_alpha),
                }
            })
            .collect();

        [even_xforms, odd_xforms]
    }
}

/// Interpolate along the Poincaré disk geodesic between z1 and z2.
/// t=0 gives z1, t=1 gives z2. Maps z1 to origin (where geodesics are
/// straight lines), interpolates along the diameter, then maps back.
pub fn geodesic_lerp(z1: Complex, z2: Complex, t: f64) -> Complex {
    if (z1 - z2).norm_sq() < 1e-30 {
        return z1;
    }

    // M(z) = (z - z1) / (1 - conj(z1)*z) sends z1 to origin
    let w = (z2 - z1) / (Complex::ONE - z1.conj() * z2);
    let w_abs = w.abs();

    if w_abs < 1e-15 {
        return z1;
    }

    // Along the diameter through w: tanh(t * atanh(|w|)) in direction of w
    let r_t = (t * w_abs.atanh()).tanh();
    let w_t = w * (r_t / w_abs);

    // M^(-1)(z) = (z + z1) / (1 + conj(z1)*z)
    (w_t + z1) / (Complex::ONE + z1.conj() * w_t)
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
        let cfg = TilingConfig::new(8, 3);
        let r = polygon_disk_radius(&cfg);
        assert!(
            (r - 0.4056).abs() < 0.001,
            "octagon disk radius {r} should be ~0.4056"
        );
    }

    #[test]
    fn test_canonical_polygon_vertices_83() {
        let cfg = TilingConfig::new(8, 3);
        let verts = canonical_polygon(&cfg);
        let r = polygon_disk_radius(&cfg);
        assert_eq!(verts.len(), 8);
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
    fn test_canonical_polygon_vertices_37() {
        let cfg = TilingConfig::new(3, 7);
        let verts = canonical_polygon(&cfg);
        assert_eq!(verts.len(), 3);
        let r = polygon_disk_radius(&cfg);
        for (i, v) in verts.iter().enumerate() {
            assert!(
                (v.abs() - r).abs() < EPS,
                "vertex {i} radius {} != {r}",
                v.abs()
            );
        }
    }

    #[test]
    fn test_canonical_polygon_vertices_54() {
        let cfg = TilingConfig::new(5, 4);
        let verts = canonical_polygon(&cfg);
        assert_eq!(verts.len(), 5);
        let r = polygon_disk_radius(&cfg);
        for (i, v) in verts.iter().enumerate() {
            assert!(
                (v.abs() - r).abs() < EPS,
                "vertex {i} radius {} != {r}",
                v.abs()
            );
        }
    }

    #[test]
    #[should_panic(expected = "not hyperbolic")]
    fn test_non_hyperbolic_44_panics() {
        TilingConfig::new(4, 4);
    }

    #[test]
    fn test_neighbor_transforms_distinct() {
        let cfg = TilingConfig::new(8, 3);
        let [even_xforms, _] = neighbor_transforms(&cfg);
        let centers: Vec<Complex> = even_xforms.iter().map(|t| t.apply(Complex::ZERO)).collect();

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
        let cfg = TilingConfig::new(8, 3);
        let [even_xforms, _] = neighbor_transforms(&cfg);
        let c0 = even_xforms[0].apply(Complex::ZERO);
        let r = c0.abs();
        let hyp_dist = 2.0 * r.atanh();
        let cosh_psi = (PI / 3.0).cos() / (PI / 8.0).sin();
        let expected_cosh_d = 2.0 * cosh_psi * cosh_psi - 1.0;
        let expected_d = expected_cosh_d.acosh();
        assert!(
            (hyp_dist - expected_d).abs() < 1e-6,
            "neighbor distance {hyp_dist} != expected {expected_d}"
        );
    }

    #[test]
    fn test_geodesic_lerp_endpoints() {
        let z1 = Complex::new(0.2, 0.1);
        let z2 = Complex::new(-0.1, 0.3);
        assert!(complex_approx_eq(geodesic_lerp(z1, z2, 0.0), z1));
        assert!(complex_approx_eq(geodesic_lerp(z1, z2, 1.0), z2));
    }

    #[test]
    fn test_geodesic_lerp_degenerate() {
        let z = Complex::new(0.15, -0.2);
        assert!(complex_approx_eq(geodesic_lerp(z, z, 0.5), z));
    }

    #[test]
    fn test_geodesic_lerp_midpoint_closer_to_origin() {
        // Geodesic midpoint should be closer to origin than chord midpoint
        let z1 = Complex::new(0.3, 0.1);
        let z2 = Complex::new(-0.1, 0.35);
        let geo_mid = geodesic_lerp(z1, z2, 0.5);
        let chord_mid = Complex::new((z1.re + z2.re) / 2.0, (z1.im + z2.im) / 2.0);
        assert!(
            geo_mid.abs() < chord_mid.abs(),
            "geodesic midpoint {} should be closer to origin than chord midpoint {}",
            geo_mid.abs(),
            chord_mid.abs()
        );
    }

    #[test]
    fn test_geodesic_lerp_mobius_invariance() {
        let z1 = Complex::new(0.2, 0.1);
        let z2 = Complex::new(-0.1, 0.3);
        let m = Mobius {
            a: Complex::new(1.2, 0.3),
            b: Complex::new(0.1, 0.4),
        }
        .normalized();

        for &t in &[0.0, 0.25, 0.5, 0.75, 1.0] {
            let lhs = m.apply(geodesic_lerp(z1, z2, t));
            let rhs = geodesic_lerp(m.apply(z1), m.apply(z2), t);
            assert!(
                complex_approx_eq(lhs, rhs),
                "Möbius invariance failed at t={t}: ({}, {}) vs ({}, {})",
                lhs.re, lhs.im, rhs.re, rhs.im
            );
        }
    }

    #[test]
    fn test_geodesic_lerp_on_geodesic() {
        // All interpolated points should lie on the geodesic (equal hyperbolic distance ratios)
        let z1 = Complex::new(0.25, 0.1);
        let z2 = Complex::new(-0.15, 0.3);
        let total_d = poincare_distance(z1, z2);
        for i in 0..=4 {
            let t = i as f64 / 4.0;
            let p = geodesic_lerp(z1, z2, t);
            let d = poincare_distance(z1, p);
            assert!(
                (d - t * total_d).abs() < 1e-8,
                "at t={t}: d(z1,p)={d} != t*D={}",
                t * total_d
            );
        }
    }
}
