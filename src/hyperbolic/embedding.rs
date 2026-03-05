use super::poincare::Complex;

/// Embed a Poincare disk point into 3D space (Y-up).
/// `embed_type`: 0.0 = paraboloid, 1.0 = sphere.
/// `embed_param`: height (paraboloid) or theta_max latitude (sphere).
/// Matches the shader's `disk_embed` function.
///
/// Returns [X, Y, Z] where Y is "up".
pub fn disk_embed(z: Complex, embed_type: f32, embed_param: f32) -> [f32; 3] {
    if embed_type < 0.5 {
        // Paraboloid
        let r2 = z.norm_sq() as f32;
        let y = embed_param * r2 / (1.0 + r2);
        [z.re as f32, y, z.im as f32]
    } else {
        // Sphere
        let r = z.abs() as f32;
        let theta_max = embed_param.abs().max(0.001);
        let big_r = 1.0 / theta_max;
        let theta = theta_max * r;
        if r < 0.0001 {
            return [0.0, 0.0, 0.0];
        }
        let st = theta.sin();
        let ct = theta.cos();
        let scale = big_r * st / r;
        [z.re as f32 * scale, big_r * (1.0 - ct), z.im as f32 * scale]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_origin_maps_to_center() {
        let [x, y, z] = disk_embed(Complex::ZERO, 0.0, 0.4);
        assert!((x - 0.0).abs() < 1e-6);
        assert!((y - 0.0).abs() < 1e-6);
        assert!((z - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_paraboloid_bounded() {
        let test_points = [
            Complex::new(0.0, 0.0),
            Complex::new(0.3, 0.0),
            Complex::new(0.5, 0.0),
            Complex::new(0.8, 0.0),
            Complex::new(0.95, 0.0),
        ];
        for p in &test_points {
            let [_x, y, _z] = disk_embed(*p, 0.0, 0.4);
            assert!(
                y < 0.5,
                "paraboloid Y too large at r={}: Y={y}",
                p.abs()
            );
        }
    }

    #[test]
    fn test_y_increases_with_distance() {
        let near = disk_embed(Complex::new(0.1, 0.0), 0.0, 0.4);
        let far = disk_embed(Complex::new(0.5, 0.0), 0.0, 0.4);
        assert!(far[1] > near[1], "Y should increase with distance from origin");
    }

    #[test]
    fn test_paraboloid_xz_match_disk_coords() {
        let z = Complex::new(0.3, -0.4);
        let [x, _y, zz] = disk_embed(z, 0.0, 0.4);
        assert!((x as f64 - z.re).abs() < 1e-6);
        assert!((zz as f64 - z.im).abs() < 1e-6);
    }

    #[test]
    fn test_sphere_origin() {
        let [x, y, z] = disk_embed(Complex::ZERO, 1.0, 0.2);
        assert!(x.abs() < 1e-6);
        assert!(y.abs() < 1e-6);
        assert!(z.abs() < 1e-6);
    }

    #[test]
    fn test_sphere_near_center_matches_disk() {
        // Near center, sphere coords should approximate disk coords
        let z = Complex::new(0.01, 0.02);
        let [x, _y, zz] = disk_embed(z, 1.0, 0.2);
        assert!((x as f64 - z.re).abs() < 0.01, "x={x} vs re={}", z.re);
        assert!((zz as f64 - z.im).abs() < 0.01, "z={zz} vs im={}", z.im);
    }

    #[test]
    fn test_sphere_y_increases_with_distance() {
        let near = disk_embed(Complex::new(0.1, 0.0), 1.0, 0.5);
        let far = disk_embed(Complex::new(0.5, 0.0), 1.0, 0.5);
        assert!(far[1] > near[1], "Sphere Y should increase with distance from origin");
    }
}
