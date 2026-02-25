use crate::render::camera::CameraSnapshot;

/// Fixed simulation rate: 60 updates per second.
pub const SIM_DT: f64 = 1.0 / 60.0;

/// Maximum frame time to prevent spiral of death.
/// If a frame takes longer than this, we cap the accumulated time.
const MAX_FRAME_TIME: f64 = 0.25;

pub struct GameLoop {
    pub accumulator: f64,
    pub sim_tick: u64,
    pub last_frame: Option<std::time::Instant>,
    pub prev_camera: Option<CameraSnapshot>,
    pub curr_camera: Option<CameraSnapshot>,
    // FPS/UPS tracking
    fps_samples: Vec<f64>,
    ups_ticks: u32,
    ups_timer: f64,
    pub fps: f64,
    pub ups: f64,
}

impl GameLoop {
    pub fn new() -> Self {
        Self {
            accumulator: 0.0,
            sim_tick: 0,
            last_frame: None,
            prev_camera: None,
            curr_camera: None,
            fps_samples: Vec::with_capacity(60),
            ups_ticks: 0,
            ups_timer: 0.0,
            fps: 0.0,
            ups: 0.0,
        }
    }

    /// Call at the start of each frame. Returns the frame dt (capped) if we
    /// have a previous frame, or None on the very first frame.
    pub fn begin_frame(&mut self) -> Option<f64> {
        let now = std::time::Instant::now();
        let dt = if let Some(last) = self.last_frame {
            let raw_dt = now.duration_since(last).as_secs_f64();
            let dt = raw_dt.min(MAX_FRAME_TIME);

            // FPS tracking
            if raw_dt > 0.0 {
                self.fps_samples.push(raw_dt);
                if self.fps_samples.len() > 60 {
                    self.fps_samples.remove(0);
                }
                let avg: f64 = self.fps_samples.iter().sum::<f64>() / self.fps_samples.len() as f64;
                self.fps = 1.0 / avg;
            }

            Some(dt)
        } else {
            None
        };
        self.last_frame = Some(now);
        dt
    }

    /// Accumulate frame time and return how many sim ticks should run.
    pub fn accumulate(&mut self, frame_dt: f64) -> u32 {
        self.accumulator += frame_dt;

        // UPS tracking
        self.ups_timer += frame_dt;

        let mut ticks = 0u32;
        while self.accumulator >= SIM_DT {
            self.accumulator -= SIM_DT;
            self.sim_tick += 1;
            ticks += 1;

            self.ups_ticks += 1;
        }

        if self.ups_timer >= 1.0 {
            self.ups = self.ups_ticks as f64 / self.ups_timer;
            self.ups_ticks = 0;
            self.ups_timer = 0.0;
        }

        ticks
    }

    /// Alpha for interpolating between prev and curr camera snapshots.
    pub fn interpolation_alpha(&self) -> f64 {
        self.accumulator / SIM_DT
    }

    /// Store camera snapshots before and after sim ticks.
    pub fn save_prev_camera(&mut self, snap: CameraSnapshot) {
        self.prev_camera = Some(snap);
    }

    pub fn save_curr_camera(&mut self, snap: CameraSnapshot) {
        self.curr_camera = Some(snap);
    }

    /// Get an interpolated camera snapshot for rendering.
    pub fn interpolated_camera(&self) -> Option<CameraSnapshot> {
        match (self.prev_camera.as_ref(), self.curr_camera.as_ref()) {
            (Some(prev), Some(curr)) => {
                let alpha = self.interpolation_alpha();
                Some(prev.lerp(curr, alpha))
            }
            (None, Some(curr)) => Some(*curr),
            _ => None,
        }
    }
}
