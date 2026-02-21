use super::poincare::Complex;

/// Convert a point on the Poincare disk to gentle bowl coordinates (Y-up).
/// Uses disk coordinates directly for X,Z with mild upward curvature.
/// Matches the shader's disk_to_bowl function.
///
/// Returns [X, Y, Z] where Y is "up".
pub fn disk_to_bowl(z: Complex) -> [f32; 3] {
    let r2 = z.norm_sq();
    let y = 0.4 * r2 / (1.0 + r2);
    [z.re as f32, y as f32, z.im as f32]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_origin_maps_to_center() {
        let [x, y, z] = disk_to_bowl(Complex::ZERO);
        assert!((x - 0.0).abs() < 1e-6);
        assert!((y - 0.0).abs() < 1e-6);
        assert!((z - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_bowl_bounded() {
        // Bowl Y should stay bounded even near disk boundary
        let test_points = [
            Complex::new(0.0, 0.0),
            Complex::new(0.3, 0.0),
            Complex::new(0.5, 0.0),
            Complex::new(0.8, 0.0),
            Complex::new(0.95, 0.0),
        ];
        for p in &test_points {
            let [_x, y, _z] = disk_to_bowl(*p);
            assert!(
                y < 0.5,
                "bowl Y too large at r={}: Y={y}",
                p.abs()
            );
        }
    }

    #[test]
    fn test_y_increases_with_distance() {
        let near = disk_to_bowl(Complex::new(0.1, 0.0));
        let far = disk_to_bowl(Complex::new(0.5, 0.0));
        assert!(far[1] > near[1], "Y should increase with distance from origin");
    }

    #[test]
    fn test_xz_match_disk_coords() {
        let z = Complex::new(0.3, -0.4);
        let [x, _y, zz] = disk_to_bowl(z);
        assert!((x as f64 - z.re).abs() < 1e-6);
        assert!((zz as f64 - z.im).abs() < 1e-6);
    }
}
