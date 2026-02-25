use crate::game::input::{GameAction, InputState};
use crate::hyperbolic::poincare::{Complex, Mobius};
use crate::hyperbolic::tiling::TilingState;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraMode {
    TopDown,
    FirstPerson,
}

pub struct Camera {
    pub tile: usize,
    pub local: Mobius,
    pub heading: f64,
    pub height: f32,
    pub mode: CameraMode,
}

/// Snapshot for interpolation between sim ticks.
#[derive(Clone, Copy)]
pub struct CameraSnapshot {
    pub tile: usize,
    pub local: Mobius,
    pub heading: f64,
    pub height: f32,
    pub mode: CameraMode,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            tile: 0,
            local: Mobius::identity(),
            heading: 0.0,
            height: 2.0,
            mode: CameraMode::TopDown,
        }
    }

    pub fn is_first_person(&self) -> bool {
        self.mode == CameraMode::FirstPerson
    }

    pub fn toggle_mode(&mut self) {
        match self.mode {
            CameraMode::TopDown => {
                self.mode = CameraMode::FirstPerson;
                self.height = 0.05;
            }
            CameraMode::FirstPerson => {
                self.mode = CameraMode::TopDown;
                self.height = 2.0;
            }
        }
    }

    pub fn snapshot(&self) -> CameraSnapshot {
        CameraSnapshot {
            tile: self.tile,
            local: self.local,
            heading: self.heading,
            height: self.height,
            mode: self.mode,
        }
    }

    pub fn build_view_proj(&self, aspect: f32) -> glam::Mat4 {
        match self.mode {
            CameraMode::FirstPerson => {
                let eye = glam::Vec3::new(0.0, self.height, 0.0);
                let h = self.heading as f32;
                let look_dist = 0.5_f32;
                let target = glam::Vec3::new(
                    -h.sin() * look_dist,
                    0.02,
                    -h.cos() * look_dist,
                );
                let view = glam::Mat4::look_at_rh(eye, target, glam::Vec3::Y);
                let proj = glam::Mat4::perspective_rh(90.0_f32.to_radians(), aspect, 0.005, 100.0);
                proj * view
            }
            CameraMode::TopDown => {
                let eye = glam::Vec3::new(0.0, self.height, 0.0);
                let center = glam::Vec3::ZERO;
                let up = glam::Vec3::new(0.0, 0.0, -1.0);
                let view = glam::Mat4::look_at_rh(eye, center, up);
                let proj = glam::Mat4::perspective_rh(60.0_f32.to_radians(), aspect, 0.1, 100.0);
                proj * view
            }
        }
    }

    pub fn process_movement(
        &mut self,
        input: &InputState,
        tiling: &mut TilingState,
        ui_open: bool,
        dt: f64,
    ) {
        if ui_open {
            return;
        }

        let move_speed = 0.8 * dt;
        let mut dx = 0.0_f64;
        let mut dy = 0.0_f64;

        match self.mode {
            CameraMode::FirstPerson => {
                let rotate_speed = 1.8 * dt;
                if input.is_active(GameAction::StrafeLeft) {
                    self.heading += rotate_speed;
                }
                if input.is_active(GameAction::StrafeRight) {
                    self.heading -= rotate_speed;
                }
                let mut forward = 0.0_f64;
                if input.is_active(GameAction::MoveForward) {
                    forward += move_speed;
                }
                if input.is_active(GameAction::MoveBackward) {
                    forward -= move_speed;
                }
                if forward != 0.0 {
                    dx = -self.heading.sin() * forward;
                    dy = -self.heading.cos() * forward;
                }
            }
            CameraMode::TopDown => {
                if input.is_active(GameAction::MoveForward) {
                    dy -= move_speed;
                }
                if input.is_active(GameAction::MoveBackward) {
                    dy += move_speed;
                }
                if input.is_active(GameAction::StrafeLeft) {
                    dx -= move_speed;
                }
                if input.is_active(GameAction::StrafeRight) {
                    dx += move_speed;
                }
            }
        }

        if input.is_active(GameAction::CameraUp) {
            self.height = (self.height + 2.0 * dt as f32).min(20.0);
        }
        if input.is_active(GameAction::CameraDown) {
            let min_height = if self.is_first_person() { 0.02 } else { 1.5 };
            self.height = (self.height - 2.0 * dt as f32).max(min_height);
        }

        if dx != 0.0 || dy != 0.0 {
            let dist = (dx * dx + dy * dy).sqrt();
            let half_d = dist / 2.0;
            let a_val = half_d.cosh();
            let b_mag = half_d.sinh();
            let theta = dy.atan2(dx);
            let translation = Mobius {
                a: Complex::new(a_val, 0.0),
                b: Complex::from_polar(b_mag, theta),
            };
            self.local = self.local.compose(&translation);

            let camera_pos = self.local.apply(Complex::ZERO);
            let dist_to_origin = camera_pos.abs();

            let cam_parity = tiling.tiles[self.tile].parity as usize;
            let xforms = &tiling.neighbor_xforms[cam_parity];
            let mut best_dir: Option<usize> = None;
            let mut best_dist = dist_to_origin;
            for (dir, xform) in xforms.iter().enumerate() {
                let neighbor_center = xform.apply(Complex::ZERO);
                let d = (camera_pos - neighbor_center).abs();
                if d < best_dist {
                    best_dist = d;
                    best_dir = Some(dir);
                }
            }

            if let Some(dir) = best_dir {
                let current_tile_xform = tiling.tiles[self.tile].transform;
                let neighbor_abs = current_tile_xform.compose(&xforms[dir]);
                let neighbor_center = neighbor_abs.apply(Complex::ZERO);

                if let Some(new_tile_idx) = tiling.find_tile_near(neighbor_center) {
                    let new_tile_xform = tiling.tiles[new_tile_idx].transform;
                    let inv_new = new_tile_xform.inverse();
                    self.local = inv_new.compose(&current_tile_xform.compose(&self.local));
                    self.tile = new_tile_idx;
                    tiling.recenter_on(new_tile_idx);
                }
            }
        }

        let cam_pos = self.local.apply(Complex::ZERO);
        tiling.ensure_coverage(cam_pos, 3);
    }

    pub fn unproject_to_disk(&self, sx: f64, sy: f64, width: f32, height: f32) -> Option<Complex> {
        let aspect = width / height;
        let view_proj = self.build_view_proj(aspect);
        let inv_vp = view_proj.inverse();

        let ndc_x = 2.0 * (sx as f32 / width) - 1.0;
        let ndc_y = 1.0 - 2.0 * (sy as f32 / height);

        let near = inv_vp * glam::Vec4::new(ndc_x, ndc_y, -1.0, 1.0);
        let far = inv_vp * glam::Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
        let near = glam::Vec3::new(near.x / near.w, near.y / near.w, near.z / near.w);
        let far = glam::Vec3::new(far.x / far.w, far.y / far.w, far.z / far.w);

        let dir = far - near;
        if dir.y.abs() < 1e-8 {
            return None;
        }

        // Iteratively intersect ray with bowl surface y = 0.4*r^2/(1+r^2)
        let mut target_y = 0.0_f32;
        for _ in 0..5 {
            let t = (target_y - near.y) / dir.y;
            if t < 0.0 {
                return None;
            }
            let hit = near + dir * t;
            let r2 = hit.x * hit.x + hit.z * hit.z;
            target_y = 0.4 * r2 / (1.0 + r2);
        }
        let t = (target_y - near.y) / dir.y;
        if t < 0.0 {
            return None;
        }
        let hit = near + dir * t;
        Some(Complex::new(hit.x as f64, hit.z as f64))
    }
}

impl CameraSnapshot {
    pub fn build_view_proj(&self, aspect: f32) -> glam::Mat4 {
        match self.mode {
            CameraMode::FirstPerson => {
                let eye = glam::Vec3::new(0.0, self.height, 0.0);
                let h = self.heading as f32;
                let look_dist = 0.5_f32;
                let target = glam::Vec3::new(
                    -h.sin() * look_dist,
                    0.02,
                    -h.cos() * look_dist,
                );
                let view = glam::Mat4::look_at_rh(eye, target, glam::Vec3::Y);
                let proj = glam::Mat4::perspective_rh(90.0_f32.to_radians(), aspect, 0.005, 100.0);
                proj * view
            }
            CameraMode::TopDown => {
                let eye = glam::Vec3::new(0.0, self.height, 0.0);
                let center = glam::Vec3::ZERO;
                let up = glam::Vec3::new(0.0, 0.0, -1.0);
                let view = glam::Mat4::look_at_rh(eye, center, up);
                let proj = glam::Mat4::perspective_rh(60.0_f32.to_radians(), aspect, 0.1, 100.0);
                proj * view
            }
        }
    }

    /// Linearly interpolate between two snapshots for smooth rendering.
    /// Uses `self` as the "previous" state and `next` as the "current" state.
    pub fn lerp(&self, next: &CameraSnapshot, alpha: f64) -> CameraSnapshot {
        // If tiles differ, the Mobius transforms are in different coordinate frames.
        // Interpolation would produce garbage — just use the latest snapshot.
        if self.tile != next.tile {
            return *next;
        }

        let tile = next.tile;
        let mode = next.mode;

        // Interpolate continuous values
        let height = self.height + (next.height - self.height) * alpha as f32;
        let heading = self.heading + (next.heading - self.heading) * alpha;

        // Interpolate Mobius: lerp a and b components, then renormalize
        let a = Complex::new(
            self.local.a.re + (next.local.a.re - self.local.a.re) * alpha,
            self.local.a.im + (next.local.a.im - self.local.a.im) * alpha,
        );
        let b = Complex::new(
            self.local.b.re + (next.local.b.re - self.local.b.re) * alpha,
            self.local.b.im + (next.local.b.im - self.local.b.im) * alpha,
        );
        // Renormalize: |a|^2 - |b|^2 = 1
        let norm = (a.norm_sq() - b.norm_sq()).abs().sqrt();
        let local = if norm > 1e-10 {
            Mobius {
                a: Complex::new(a.re / norm, a.im / norm),
                b: Complex::new(b.re / norm, b.im / norm),
            }
        } else {
            next.local
        };

        CameraSnapshot {
            tile,
            local,
            heading,
            height,
            mode,
        }
    }
}
