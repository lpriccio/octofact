use std::collections::HashMap;
use crate::game::items::{IconParams, IconShape, ItemId};

pub struct IconAtlas {
    textures: HashMap<ItemId, egui::TextureHandle>,
}

impl IconAtlas {
    pub fn generate(ctx: &egui::Context) -> Self {
        let mut textures = HashMap::new();
        let size = 32;

        for &item in ItemId::all() {
            let params = item.icon_params();
            let pixels = rasterize_icon(&params, size, size);
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [size, size],
                &pixels.iter().flat_map(|c| [c.r(), c.g(), c.b(), c.a()]).collect::<Vec<_>>(),
            );
            let texture = ctx.load_texture(
                item.display_name(),
                image,
                egui::TextureOptions::NEAREST,
            );
            textures.insert(item, texture);
        }

        Self { textures }
    }

    pub fn get(&self, item: ItemId) -> Option<&egui::TextureHandle> {
        self.textures.get(&item)
    }
}

pub fn rasterize_icon(params: &IconParams, w: usize, h: usize) -> Vec<egui::Color32> {
    let mut pixels = vec![egui::Color32::TRANSPARENT; w * h];
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let radius = (w.min(h) as f32 / 2.0) - 1.5;

    for y in 0..h {
        for x in 0..w {
            let px = x as f32 + 0.5 - cx;
            let py = y as f32 + 0.5 - cy;
            let dist = sdf_shape(params.shape, px, py, radius);

            if dist < 0.0 {
                // Inside the shape
                let t = (-dist / radius).min(1.0);
                let r = lerp(params.secondary_color[0], params.primary_color[0], t);
                let g = lerp(params.secondary_color[1], params.primary_color[1], t);
                let b = lerp(params.secondary_color[2], params.primary_color[2], t);
                pixels[y * w + x] = egui::Color32::from_rgb(
                    (r * 255.0) as u8,
                    (g * 255.0) as u8,
                    (b * 255.0) as u8,
                );
            } else if dist < 1.5 {
                // Anti-aliased edge
                let alpha = 1.0 - dist / 1.5;
                let r = params.secondary_color[0];
                let g = params.secondary_color[1];
                let b = params.secondary_color[2];
                pixels[y * w + x] = egui::Color32::from_rgba_unmultiplied(
                    (r * 255.0) as u8,
                    (g * 255.0) as u8,
                    (b * 255.0) as u8,
                    (alpha * 255.0) as u8,
                );
            }
        }
    }

    pixels
}

fn sdf_shape(shape: IconShape, px: f32, py: f32, radius: f32) -> f32 {
    match shape {
        IconShape::Circle => {
            (px * px + py * py).sqrt() - radius
        }
        IconShape::Square => {
            let r = radius * 0.75;
            let dx = px.abs() - r;
            let dy = py.abs() - r;
            dx.max(dy)
        }
        IconShape::Triangle => {
            let r = radius * 0.85;
            // Equilateral triangle pointing up
            let k = 3.0_f32.sqrt();
            let px_abs = px.abs();
            let py_shifted = py + r * 0.35;
            let d1 = px_abs * k / 2.0 + py_shifted / 2.0 - r * 0.5;
            let d2 = -py_shifted - r * 0.35;
            d1.max(d2)
        }
        IconShape::Hexagon => {
            let r = radius * 0.85;
            let k = 3.0_f32.sqrt();
            let px_abs = px.abs();
            let py_abs = py.abs();
            (px_abs * k / 2.0 + py_abs / 2.0).max(py_abs) - r
        }
        IconShape::Diamond => {
            let r = radius * 0.85;
            (px.abs() + py.abs()) / 2.0_f32.sqrt() - r / 2.0_f32.sqrt()
        }
        IconShape::Octagon => {
            let r = radius * 0.8;
            let px_abs = px.abs();
            let py_abs = py.abs();
            let d = px_abs.max(py_abs).max((px_abs + py_abs) * std::f32::consts::FRAC_1_SQRT_2);
            d - r
        }
        IconShape::Star => {
            // 5-pointed star using polar SDF
            let angle = py.atan2(px);
            let dist = (px * px + py * py).sqrt();
            let r = radius * 0.85;
            // Star radius oscillates between outer and inner
            let n = 5.0_f32;
            let sector = (angle * n / std::f32::consts::TAU + 0.5).fract() - 0.5;
            let inner = r * 0.4;
            let star_r = r - (r - inner) * (sector.abs() * 2.0).min(1.0);
            dist - star_r
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rasterize_icon_dimensions() {
        let params = ItemId::Point.icon_params();
        let pixels = rasterize_icon(&params, 32, 32);
        assert_eq!(pixels.len(), 32 * 32);
    }

    #[test]
    fn test_rasterize_icon_not_blank() {
        let params = ItemId::Point.icon_params();
        let pixels = rasterize_icon(&params, 32, 32);
        let non_transparent = pixels.iter().filter(|p| p.a() > 0).count();
        assert!(non_transparent > 0, "Icon should have non-transparent pixels");
    }

    #[test]
    fn test_all_shapes_produce_pixels() {
        for &item in ItemId::all() {
            let params = item.icon_params();
            let pixels = rasterize_icon(&params, 32, 32);
            let non_transparent = pixels.iter().filter(|p| p.a() > 0).count();
            assert!(non_transparent > 0, "{:?} icon is blank", item);
        }
    }
}
