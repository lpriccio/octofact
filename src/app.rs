use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

use crate::hyperbolic::poincare::{canonical_octagon, Complex, Mobius};
use crate::hyperbolic::tiling::{format_address, TilingState};
use crate::render::mesh::build_octagon_mesh;
use crate::render::pipeline::{RenderState, Uniforms};

fn project_to_screen(world_pos: glam::Vec3, view_proj: &glam::Mat4, width: f32, height: f32) -> Option<(f32, f32)> {
    let clip = *view_proj * glam::Vec4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let ndc = glam::Vec3::new(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w);
    if ndc.x < -1.0 || ndc.x > 1.0 || ndc.y < -1.0 || ndc.y > 1.0 {
        return None;
    }
    let screen_x = (ndc.x + 1.0) * 0.5 * width;
    let screen_y = (1.0 - ndc.y) * 0.5 * height;
    Some((screen_x, screen_y))
}

pub struct GpuState {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub window: Arc<Window>,
}

impl GpuState {
    pub fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("request adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("octofact device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            },
        ))
        .expect("request device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        Self {
            surface,
            device,
            queue,
            config,
            window,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }
}

struct RunningState {
    gpu: GpuState,
    render: RenderState,
    tiling: TilingState,
    extra_elevation: std::collections::HashMap<usize, f32>,
}

pub struct App {
    running: Option<RunningState>,
    camera_height: f32,
    camera_tile: usize,
    camera_local: Mobius,
    first_person: bool,
    heading: f64,
    keys_held: std::collections::HashSet<winit::keyboard::KeyCode>,
    last_frame: Option<std::time::Instant>,
    cursor_pos: Option<winit::dpi::PhysicalPosition<f64>>,
}

impl App {
    pub fn new() -> Self {
        Self {
            running: None,
            camera_height: 2.0,
            camera_tile: 0,
            camera_local: Mobius::identity(),
            first_person: false,
            heading: 0.0,
            keys_held: std::collections::HashSet::new(),
            last_frame: None,
            cursor_pos: None,
        }
    }

    fn build_view_proj(&self, aspect: f32) -> glam::Mat4 {
        if self.first_person {
            // Camera sits at the bowl center; tiles are rendered in camera-relative
            // space via inv_view, so the world slides under us as we move.
            let eye = glam::Vec3::new(0.0, self.camera_height, 0.0);

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
        } else {
            let eye = glam::Vec3::new(0.0, self.camera_height, 0.0);
            let center = glam::Vec3::ZERO;
            let up = glam::Vec3::new(0.0, 0.0, -1.0);
            let view = glam::Mat4::look_at_rh(eye, center, up);
            let proj = glam::Mat4::perspective_rh(60.0_f32.to_radians(), aspect, 0.1, 100.0);
            proj * view
        }
    }

    fn process_movement(&mut self, dt: f64) {
        use winit::keyboard::KeyCode;

        let move_speed = 0.8 * dt;
        let mut dx = 0.0_f64;
        let mut dy = 0.0_f64;

        if self.first_person {
            // First-person: A/D rotate heading, W/S move along heading
            let rotate_speed = 1.8 * dt;
            if self.keys_held.contains(&KeyCode::KeyA) {
                self.heading += rotate_speed;
            }
            if self.keys_held.contains(&KeyCode::KeyD) {
                self.heading -= rotate_speed;
            }
            let mut forward = 0.0_f64;
            if self.keys_held.contains(&KeyCode::KeyW) {
                forward += move_speed;
            }
            if self.keys_held.contains(&KeyCode::KeyS) {
                forward -= move_speed;
            }
            if forward != 0.0 {
                dx = -self.heading.sin() * forward;
                dy = -self.heading.cos() * forward;
            }
        } else {
            // Top-down: WASD maps directly to disk axes
            if self.keys_held.contains(&KeyCode::KeyW) {
                dy -= move_speed;
            }
            if self.keys_held.contains(&KeyCode::KeyS) {
                dy += move_speed;
            }
            if self.keys_held.contains(&KeyCode::KeyA) {
                dx -= move_speed;
            }
            if self.keys_held.contains(&KeyCode::KeyD) {
                dx += move_speed;
            }
        }
        if self.keys_held.contains(&KeyCode::KeyQ) {
            self.camera_height = (self.camera_height + 2.0 * dt as f32).min(20.0);
        }
        if self.keys_held.contains(&KeyCode::KeyE) {
            let min_height = if self.first_person { 0.02 } else { 1.5 };
            self.camera_height = (self.camera_height - 2.0 * dt as f32).max(min_height);
        }

        if dx != 0.0 || dy != 0.0 {
            let dist = (dx * dx + dy * dy).sqrt();
            // Proper hyperbolic translation: a = cosh(d/2), b = sinh(d/2) * e^(i*theta)
            let half_d = dist / 2.0;
            let a_val = half_d.cosh();
            let b_mag = half_d.sinh();
            let theta = dy.atan2(dx);
            let translation = Mobius {
                a: Complex::new(a_val, 0.0),
                b: Complex::from_polar(b_mag, theta),
            };
            // Right-compose: movement in camera's body frame, invariant under
            // cell frame changes from recenter_on.
            self.camera_local = self.camera_local.compose(&translation);

            // Cell transition: check if camera is closer to a neighbor center
            if let Some(running) = &mut self.running {
                let tiling = &mut running.tiling;
                let camera_pos = self.camera_local.apply(Complex::ZERO);
                let dist_to_origin = camera_pos.abs(); // hyperbolic dist approx for small values

                let mut best_dir: Option<usize> = None;
                let mut best_dist = dist_to_origin;
                for dir in 0..8usize {
                    let neighbor_center = tiling.neighbor_xforms[dir].apply(Complex::ZERO);
                    let d = (camera_pos - neighbor_center).abs();
                    // Use Euclidean distance in disk as proxy (fine near center)
                    if d < best_dist {
                        best_dist = d;
                        best_dir = Some(dir);
                    }
                }

                if let Some(dir) = best_dir {
                    // Find the tile at that neighbor position
                    let current_tile_xform = tiling.tiles[self.camera_tile].transform;
                    let neighbor_abs = current_tile_xform.compose(&tiling.neighbor_xforms[dir]);
                    let neighbor_center = neighbor_abs.apply(Complex::ZERO);

                    if let Some(new_tile_idx) = tiling.find_tile_near(neighbor_center) {
                        // Transition: adjust camera_local so world position is unchanged
                        let new_tile_xform = tiling.tiles[new_tile_idx].transform;
                        let inv_new = new_tile_xform.inverse();
                        // camera_local was relative to current tile's frame;
                        // new_local = inv(new_tile) . current_tile . old_local
                        self.camera_local =
                            inv_new.compose(&current_tile_xform.compose(&self.camera_local));
                        self.camera_tile = new_tile_idx;
                        tiling.recenter_on(new_tile_idx);
                    }
                }
            }
        }

        // Ensure 3 layers of tiles around camera position
        if let Some(running) = &mut self.running {
            let cam_pos = self.camera_local.apply(Complex::ZERO);
            running.tiling.ensure_coverage(cam_pos, 3);
        }
    }


    fn handle_click(&mut self, sx: f64, sy: f64, delta: f32) {
        let running = match self.running.as_ref() {
            Some(r) => r,
            None => return,
        };

        let width = running.gpu.config.width as f32;
        let height = running.gpu.config.height as f32;
        let aspect = width / height;
        let view_proj = self.build_view_proj(aspect);
        let inv_view = self.camera_local.inverse();

        let running = self.running.as_ref().unwrap();
        let click_sx = sx as f32;
        let click_sy = sy as f32;

        // Screen-space projection picking: project each tile center through the
        // exact same transform chain as rendering, then pick the closest to click.
        let mut best_idx: Option<usize> = None;
        let mut best_dist_sq = f32::MAX;
        for (i, tile) in running.tiling.tiles.iter().enumerate() {
            let combined = inv_view.compose(&tile.transform);
            let disk_center = combined.apply(Complex::ZERO);
            if disk_center.abs() > 0.98 {
                continue;
            }

            // Bowl coords (same as shader/render_frame)
            let bowl = crate::hyperbolic::embedding::disk_to_bowl(disk_center);

            // Elevation (replicate render_frame logic)
            let digit_sum: u32 = tile.address.iter().map(|&d| d as u32).sum();
            let base_elevation = if !tile.address.is_empty() && digit_sum % 10 == 9 { 0.04_f32 } else { 0.0 };
            let extra = running.extra_elevation.get(&i).copied().unwrap_or(0.0);
            let elevation = base_elevation + extra;

            let world_pos = glam::Vec3::new(bowl[0], bowl[1] + elevation, bowl[2]);

            if let Some((proj_sx, proj_sy)) = project_to_screen(world_pos, &view_proj, width, height) {
                let dx = proj_sx - click_sx;
                let dy = proj_sy - click_sy;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq < best_dist_sq {
                    best_dist_sq = dist_sq;
                    best_idx = Some(i);
                }
            }
        }

        if let Some(idx) = best_idx {
            *self.running.as_mut().unwrap().extra_elevation.entry(idx).or_insert(0.0) += delta;
        }
    }

    fn render_frame(&mut self) -> Result<(), wgpu::SurfaceError> {
        let aspect = {
            let gpu = &self.running.as_ref().unwrap().gpu;
            gpu.config.width as f32 / gpu.config.height as f32
        };
        let view_proj = self.build_view_proj(aspect);

        let running = self.running.as_mut().unwrap();

        let output = running.gpu.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let width = running.gpu.config.width as f32;
        let height = running.gpu.config.height as f32;

        let inv_view = self.camera_local.inverse();

        // Collect visible tile indices with cached Mobius composition
        let visible: Vec<(usize, Mobius)> = running.tiling
            .tiles
            .iter()
            .enumerate()
            .filter_map(|(i, tile)| {
                let combined = inv_view.compose(&tile.transform);
                let center = combined.apply(Complex::ZERO);
                if center.abs() < 0.98 { Some((i, combined)) } else { None }
            })
            .take(crate::render::pipeline::MAX_TILES)
            .collect();
        let tile_count = visible.len();

        // Upload uniforms only for visible tiles
        for (slot, &(tile_idx, combined)) in visible.iter().enumerate() {
            let tile = &running.tiling.tiles[tile_idx];
            let digit_sum: u32 = tile.address.iter().map(|&d| d as u32).sum();
            let base_elevation = if !tile.address.is_empty() && digit_sum % 10 == 9 { 0.04_f32 } else { 0.0 };
            let extra = running.extra_elevation.get(&tile_idx).copied().unwrap_or(0.0);
            let elevation = base_elevation + extra;
            let uniforms = Uniforms {
                view_proj: view_proj.to_cols_array_2d(),
                mobius_a: [combined.a.re as f32, combined.a.im as f32, 0.0, 0.0],
                mobius_b: [combined.b.re as f32, combined.b.im as f32, 0.0, 0.0],
                disk_params: [tile.depth as f32, elevation, slot as f32 * 1e-6, 0.0],
                ..Default::default()
            };
            running.render.write_tile_uniforms(&running.gpu.queue, slot, &uniforms);
        }

        // Prepare label text if enabled
        let labels_enabled = running.render.labels_enabled;
        let mut label_buffers: Vec<glyphon::Buffer> = Vec::new();
        let mut label_positions: Vec<(f32, f32)> = Vec::new();

        if labels_enabled {
            running.render.viewport.update(
                &running.gpu.queue,
                glyphon::Resolution {
                    width: running.gpu.config.width,
                    height: running.gpu.config.height,
                },
            );

            let metrics = glyphon::Metrics::new(28.0, 32.0);

            for &(tile_idx, combined) in &visible {
                let tile = &running.tiling.tiles[tile_idx];
                let disk_center = combined.apply(Complex::ZERO);

                // Skip tiles near the disk boundary to avoid atlas overflow
                if disk_center.abs() > 0.9 {
                    continue;
                }

                // Convert to hyperboloid for projection
                let hyp = crate::hyperbolic::embedding::disk_to_bowl(disk_center);
                let world_pos = glam::Vec3::new(hyp[0], hyp[1], hyp[2]);

                if let Some((sx, sy)) = project_to_screen(world_pos, &view_proj, width, height) {
                    let text = format_address(&tile.address);
                    let mut buffer = glyphon::Buffer::new(&mut running.render.font_system, metrics);
                    buffer.set_size(&mut running.render.font_system, Some(100.0), Some(20.0));
                    buffer.set_text(
                        &mut running.render.font_system,
                        &text,
                        &glyphon::cosmic_text::Attrs::new(),
                        glyphon::cosmic_text::Shaping::Advanced,
                        None,
                    );
                    buffer.shape_until_scroll(&mut running.render.font_system, false);

                    label_buffers.push(buffer);
                    label_positions.push((sx, sy));
                }
            }

            let text_areas: Vec<glyphon::TextArea> = label_buffers
                .iter()
                .zip(label_positions.iter())
                .map(|(buf, &(sx, sy))| glyphon::TextArea {
                    buffer: buf,
                    left: sx - 20.0,
                    top: sy - 8.0,
                    scale: 1.0,
                    bounds: glyphon::TextBounds {
                        left: 0,
                        top: 0,
                        right: running.gpu.config.width as i32,
                        bottom: running.gpu.config.height as i32,
                    },
                    default_color: glyphon::Color::rgb(255, 255, 255),
                    custom_glyphs: &[],
                })
                .collect();

            let _ = running.render.text_renderer.prepare(
                &running.gpu.device,
                &running.gpu.queue,
                &mut running.render.font_system,
                &mut running.render.atlas,
                &running.render.viewport,
                text_areas,
                &mut running.render.swash_cache,
            );
        }

        let mut encoder = running.gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.02,
                            g: 0.02,
                            b: 0.05,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &running.render.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Draw tiles
            pass.set_pipeline(&running.render.pipeline);
            pass.set_vertex_buffer(0, running.render.vertex_buffer.slice(..));
            pass.set_index_buffer(running.render.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

            for i in 0..tile_count {
                let offset = RenderState::dynamic_offset(i);
                pass.set_bind_group(0, &running.render.bind_group, &[offset]);
                pass.draw_indexed(0..running.render.num_indices, 0, 0..1);
            }

            // Draw labels overlay
            if labels_enabled {
                let _ = running.render.text_renderer.render(&running.render.atlas, &running.render.viewport, &mut pass);
            }
        }

        running.gpu.queue.submit(std::iter::once(encoder.finish()));

        // Trim atlas after present
        if labels_enabled {
            running.render.atlas.trim();
        }

        output.present();
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        let window_attrs = Window::default_attributes()
            .with_title("Octofact — {8,3} Hyperbolic Plane")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 800));
        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .expect("create window"),
        );
        let gpu = GpuState::new(window);

        let octagon = canonical_octagon();
        let (verts, indices) = build_octagon_mesh(&octagon);

        let render = RenderState::new(
            &gpu.device,
            &gpu.queue,
            gpu.config.format,
            gpu.config.width,
            gpu.config.height,
            &verts,
            &indices,
        );

        let mut tiling = TilingState::new();
        tiling.ensure_coverage(Complex::ZERO, 3);

        self.running = Some(RunningState {
            gpu,
            render,
            tiling,
            extra_elevation: std::collections::HashMap::new(),
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        use winit::keyboard::{KeyCode, PhysicalKey};

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if let Some(running) = &mut self.running {
                    running.gpu.resize(new_size.width, new_size.height);
                    running.render.resize_depth(&running.gpu.device, new_size.width, new_size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if event.state.is_pressed() {
                        self.keys_held.insert(code);
                        match code {
                            KeyCode::Escape => event_loop.exit(),
                            KeyCode::KeyL => {
                                if let Some(running) = &mut self.running {
                                    running.render.labels_enabled = !running.render.labels_enabled;
                                    log::info!("labels: {}", if running.render.labels_enabled { "ON" } else { "OFF" });
                                }
                            }
                            KeyCode::Backquote => {
                                self.first_person = !self.first_person;
                                self.camera_height = if self.first_person { 0.05 } else { 2.0 };
                                log::info!("view: {}", if self.first_person { "first-person" } else { "top-down" });
                            }
                            _ => {}
                        }
                    } else {
                        self.keys_held.remove(&code);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = Some(position);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if state == winit::event::ElementState::Pressed {
                    let delta = match button {
                        winit::event::MouseButton::Left => Some(0.04),
                        winit::event::MouseButton::Right => Some(-0.04),
                        _ => None,
                    };
                    if let (Some(delta), Some(pos)) = (delta, self.cursor_pos) {
                        self.handle_click(pos.x, pos.y, delta);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if self.running.is_some() {
                    match self.render_frame() {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => {
                            let gpu = &self.running.as_ref().unwrap().gpu;
                            gpu.surface.configure(&gpu.device, &gpu.config);
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                        Err(e) => log::error!("render error: {e:?}"),
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let now = std::time::Instant::now();
        if let Some(last) = self.last_frame {
            let dt = now.duration_since(last).as_secs_f64();
            self.process_movement(dt);
        }
        self.last_frame = Some(now);

        if let Some(running) = &self.running {
            running.gpu.window.request_redraw();
        }
    }
}
