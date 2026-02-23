use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

use crate::game::config::GameConfig;
use crate::game::input::{GameAction, InputState};
use crate::game::inventory::Inventory;
use crate::game::recipes::RecipeIndex;
use crate::hyperbolic::poincare::{canonical_polygon, polygon_disk_radius, Complex, Mobius, TilingConfig};
use crate::hyperbolic::tiling::{format_address, TilingState};
use crate::render::mesh::build_polygon_mesh;
use crate::render::pipeline::{RenderState, Uniforms};
use crate::ui::icons::IconAtlas;
use crate::ui::integration::EguiIntegration;
use crate::ui::style::apply_octofact_style;

struct ClickResult {
    tile_idx: usize,
    grid_xy: (i32, i32),
    local_disk: Complex,
}

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

fn render_egui_pass(
    encoder: &mut wgpu::CommandEncoder,
    renderer: &egui_wgpu::Renderer,
    view: &wgpu::TextureView,
    paint_jobs: &[egui::ClippedPrimitive],
    screen: &egui_wgpu::ScreenDescriptor,
) {
    let mut pass = encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        })
        .forget_lifetime();
    renderer.render(&mut pass, paint_jobs, screen);
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
    egui: EguiIntegration,
    icon_atlas: IconAtlas,
}

pub struct App {
    cfg: TilingConfig,
    running: Option<RunningState>,
    camera_height: f32,
    camera_tile: usize,
    camera_local: Mobius,
    first_person: bool,
    heading: f64,
    input_state: InputState,
    config: GameConfig,
    inventory: Inventory,
    recipes: RecipeIndex,
    last_frame: Option<std::time::Instant>,
    cursor_pos: Option<winit::dpi::PhysicalPosition<f64>>,
    settings_open: bool,
    inventory_open: bool,
    rebinding: Option<GameAction>,
    grid_enabled: bool,
    klein_half_side: f64,
    flash_screen_pos: Option<(f32, f32)>,
    flash_timer: f32,
}

impl App {
    pub fn new(cfg: TilingConfig) -> Self {
        let config = GameConfig::load();
        let input_state = InputState::new(config.key_bindings.clone());
        Self {
            cfg,
            running: None,
            camera_height: 2.0,
            camera_tile: 0,
            camera_local: Mobius::identity(),
            first_person: false,
            heading: 0.0,
            input_state,
            config,
            inventory: Inventory::starting_inventory(),
            recipes: RecipeIndex::new(),
            last_frame: None,
            cursor_pos: None,
            settings_open: false,
            inventory_open: false,
            rebinding: None,
            grid_enabled: false,
            klein_half_side: {
                // For {4,n} squares: Klein half-side = r_klein / sqrt(2)
                // where r_klein = 2*r_poincare / (1 + r_poincare^2)
                let r_p = polygon_disk_radius(&cfg);
                let r_k = 2.0 * r_p / (1.0 + r_p * r_p);
                r_k / std::f64::consts::SQRT_2
            },
            flash_screen_pos: None,
            flash_timer: 0.0,
        }
    }

    fn build_view_proj(&self, aspect: f32) -> glam::Mat4 {
        if self.first_person {
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

    fn ui_is_open(&self) -> bool {
        self.settings_open || self.inventory_open
    }

    fn process_movement(&mut self, dt: f64) {
        // Don't process movement when UI is open
        if self.ui_is_open() {
            return;
        }

        let move_speed = 0.8 * dt;
        let mut dx = 0.0_f64;
        let mut dy = 0.0_f64;

        if self.first_person {
            let rotate_speed = 1.8 * dt;
            if self.input_state.is_active(GameAction::StrafeLeft) {
                self.heading += rotate_speed;
            }
            if self.input_state.is_active(GameAction::StrafeRight) {
                self.heading -= rotate_speed;
            }
            let mut forward = 0.0_f64;
            if self.input_state.is_active(GameAction::MoveForward) {
                forward += move_speed;
            }
            if self.input_state.is_active(GameAction::MoveBackward) {
                forward -= move_speed;
            }
            if forward != 0.0 {
                dx = -self.heading.sin() * forward;
                dy = -self.heading.cos() * forward;
            }
        } else {
            if self.input_state.is_active(GameAction::MoveForward) {
                dy -= move_speed;
            }
            if self.input_state.is_active(GameAction::MoveBackward) {
                dy += move_speed;
            }
            if self.input_state.is_active(GameAction::StrafeLeft) {
                dx -= move_speed;
            }
            if self.input_state.is_active(GameAction::StrafeRight) {
                dx += move_speed;
            }
        }
        if self.input_state.is_active(GameAction::CameraUp) {
            self.camera_height = (self.camera_height + 2.0 * dt as f32).min(20.0);
        }
        if self.input_state.is_active(GameAction::CameraDown) {
            let min_height = if self.first_person { 0.02 } else { 1.5 };
            self.camera_height = (self.camera_height - 2.0 * dt as f32).max(min_height);
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
            self.camera_local = self.camera_local.compose(&translation);

            if let Some(running) = &mut self.running {
                let tiling = &mut running.tiling;
                let camera_pos = self.camera_local.apply(Complex::ZERO);
                let dist_to_origin = camera_pos.abs();

                let cam_parity = tiling.tiles[self.camera_tile].parity as usize;
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
                    let current_tile_xform = tiling.tiles[self.camera_tile].transform;
                    let neighbor_abs = current_tile_xform.compose(&xforms[dir]);
                    let neighbor_center = neighbor_abs.apply(Complex::ZERO);

                    if let Some(new_tile_idx) = tiling.find_tile_near(neighbor_center) {
                        let new_tile_xform = tiling.tiles[new_tile_idx].transform;
                        let inv_new = new_tile_xform.inverse();
                        self.camera_local =
                            inv_new.compose(&current_tile_xform.compose(&self.camera_local));
                        self.camera_tile = new_tile_idx;
                        tiling.recenter_on(new_tile_idx);
                    }
                }
            }
        }

        if let Some(running) = &mut self.running {
            let cam_pos = self.camera_local.apply(Complex::ZERO);
            running.tiling.ensure_coverage(cam_pos, 3);
        }
    }

    fn unproject_to_disk(&self, sx: f64, sy: f64) -> Option<Complex> {
        let running = self.running.as_ref()?;
        let width = running.gpu.config.width as f32;
        let height = running.gpu.config.height as f32;
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
        let t = -near.y / dir.y;
        if t < 0.0 {
            return None;
        }
        let hit = near + dir * t;
        Some(Complex::new(hit.x as f64, hit.z as f64))
    }

    fn find_clicked_tile(&self, sx: f64, sy: f64) -> Option<ClickResult> {
        let running = self.running.as_ref()?;
        let width = running.gpu.config.width as f32;
        let height = running.gpu.config.height as f32;
        let aspect = width / height;
        let view_proj = self.build_view_proj(aspect);
        let inv_view = self.camera_local.inverse();
        let click_sx = sx as f32;
        let click_sy = sy as f32;

        let mut best_idx: Option<usize> = None;
        let mut best_dist_sq = f32::MAX;
        for (i, tile) in running.tiling.tiles.iter().enumerate() {
            let combined = inv_view.compose(&tile.transform);
            let disk_center = combined.apply(Complex::ZERO);
            if disk_center.abs() > 0.98 {
                continue;
            }

            let bowl = crate::hyperbolic::embedding::disk_to_bowl(disk_center);
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

        let tile_idx = best_idx?;

        // Compute grid-snapped local position
        let grid_scale = 32.0_f64;
        let click_disk = self.unproject_to_disk(sx, sy);
        let tile_xform = running.tiling.tiles[tile_idx].transform;
        let combined = inv_view.compose(&tile_xform);
        let inv_combined = combined.inverse();

        let (grid_xy, local_disk) = if let Some(disk_pos) = click_disk {
            let local = inv_combined.apply(disk_pos);
            let gx = (local.re * grid_scale).round() as i32;
            let gy = (local.im * grid_scale).round() as i32;
            let snapped = Complex::new(gx as f64 / grid_scale, gy as f64 / grid_scale);
            ((gx, gy), snapped)
        } else {
            ((0, 0), Complex::ZERO)
        };

        Some(ClickResult { tile_idx, grid_xy, local_disk })
    }

    fn handle_click(&mut self, sx: f64, sy: f64, delta: f32) {
        let result = match self.find_clicked_tile(sx, sy) {
            Some(r) => r,
            None => return,
        };

        if self.config.debug.log_clicks {
            let tile = &self.running.as_ref().unwrap().tiling.tiles[result.tile_idx];
            log::info!(
                "Click: tile[{}] addr={} grid=({},{}) local=({:.4},{:.4})",
                result.tile_idx,
                format_address(&tile.address),
                result.grid_xy.0,
                result.grid_xy.1,
                result.local_disk.re,
                result.local_disk.im,
            );

            // Set flash at click screen position
            let running = self.running.as_ref().unwrap();
            let scale = running.gpu.window.scale_factor() as f32;
            self.flash_screen_pos = Some((sx as f32 / scale, sy as f32 / scale));
            self.flash_timer = 0.4;
        }

        *self.running.as_mut().unwrap().extra_elevation.entry(result.tile_idx).or_insert(0.0) += delta;
    }

    fn render_frame(&mut self) -> Result<(), wgpu::SurfaceError> {
        let aspect = {
            let gpu = &self.running.as_ref().unwrap().gpu;
            gpu.config.width as f32 / gpu.config.height as f32
        };
        let view_proj = self.build_view_proj(aspect);

        let running = self.running.as_mut().unwrap();
        let window = running.gpu.window.clone();

        let output = running.gpu.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let width = running.gpu.config.width as f32;
        let height = running.gpu.config.height as f32;

        let inv_view = self.camera_local.inverse();

        let running = self.running.as_mut().unwrap();

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
                disk_params: [tile.depth as f32, elevation, slot as f32 * 1e-6, 13.0],
                grid_params: [
                    if self.grid_enabled { 1.0 } else { 0.0 },
                    64.0,  // 64 grid divisions per cell edge
                    0.03,  // line width in grid-cell units
                    self.klein_half_side as f32,
                ],
                ..Default::default()
            };
            running.render.write_tile_uniforms(&running.gpu.queue, slot, &uniforms);
        }

        // --- egui frame ---
        running.egui.begin_frame(&window);

        // Label overlay (replaces glyphon)
        if running.render.labels_enabled {
            let egui_ctx = running.egui.ctx.clone();
            let scale = window.scale_factor() as f32;
            let area = egui::Area::new(egui::Id::new("tile_labels"))
                .order(egui::Order::Background)
                .interactable(false);
            area.show(&egui_ctx, |ui| {
                for &(tile_idx, combined) in &visible {
                    let tile = &running.tiling.tiles[tile_idx];
                    let disk_center = combined.apply(Complex::ZERO);
                    if disk_center.abs() > 0.9 {
                        continue;
                    }
                    let digit_sum: u32 = tile.address.iter().map(|&d| d as u32).sum();
                    let base_elevation = if !tile.address.is_empty() && digit_sum % 10 == 9 { 0.04_f32 } else { 0.0 };
                    let extra = running.extra_elevation.get(&tile_idx).copied().unwrap_or(0.0);
                    let elevation = base_elevation + extra;
                    let hyp = crate::hyperbolic::embedding::disk_to_bowl(disk_center);
                    let world_pos = glam::Vec3::new(hyp[0], hyp[1] + elevation, hyp[2]);
                    if let Some((sx, sy)) = project_to_screen(world_pos, &view_proj, width, height) {
                        // Convert physical pixels to logical points for egui
                        let lx = sx / scale;
                        let ly = sy / scale;
                        let text = format_address(&tile.address);
                        ui.put(
                            egui::Rect::from_min_size(
                                egui::pos2(lx - 20.0, ly - 8.0),
                                egui::vec2(80.0, 20.0),
                            ),
                            egui::Label::new(
                                egui::RichText::new(text).color(egui::Color32::WHITE).size(14.0)
                            ),
                        );
                    }
                }
            });
        }

        // Settings menu
        crate::ui::settings::settings_menu(
            &running.egui.ctx.clone(),
            &mut self.settings_open,
            &mut self.config,
            &mut self.input_state,
            &mut self.rebinding,
        );

        // Inventory window
        crate::ui::inventory::inventory_window(
            &running.egui.ctx.clone(),
            &mut self.inventory_open,
            &self.inventory,
            &running.icon_atlas,
            &self.recipes,
        );

        // Debug click flash
        if self.flash_timer > 0.0 {
            if let Some((fx, fy)) = self.flash_screen_pos {
                let alpha = (self.flash_timer / 0.4).clamp(0.0, 1.0);
                let a = (alpha * 255.0) as u8;
                let egui_ctx = running.egui.ctx.clone();
                egui::Area::new(egui::Id::new("debug_flash"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(egui::pos2(fx, fy))
                    .interactable(false)
                    .show(&egui_ctx, |ui| {
                        let painter = ui.painter();
                        painter.circle_filled(
                            egui::pos2(fx, fy),
                            6.0,
                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, a),
                        );
                    });
            }
        }

        let full_output = running.egui.end_frame(&window);

        // --- Render ---
        let mut encoder = running.gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render encoder"),
            });

        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [running.gpu.config.width, running.gpu.config.height],
            pixels_per_point: window.scale_factor() as f32,
        };

        // Prepare egui (tessellate, update textures/buffers)
        let paint_jobs = running.egui.prepare(
            &running.gpu.device,
            &running.gpu.queue,
            &mut encoder,
            &screen,
            &full_output,
        );

        // Main render pass (tiles + egui overlay)
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
        }

        // Egui render pass (separate helper to unify encoder/renderer lifetimes)
        render_egui_pass(&mut encoder, &running.egui.renderer, &view, &paint_jobs, &screen);

        running.gpu.queue.submit(std::iter::once(encoder.finish()));
        running.egui.cleanup(&full_output);
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
            .with_title(format!("Octofact — {{{},{}}} Hyperbolic Plane", self.cfg.p, self.cfg.q))
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 800));
        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .expect("create window"),
        );
        let gpu = GpuState::new(window.clone());

        let polygon = canonical_polygon(&self.cfg);
        let (verts, indices) = build_polygon_mesh(&polygon);

        let render = RenderState::new(
            &gpu.device,
            &gpu.queue,
            gpu.config.format,
            gpu.config.width,
            gpu.config.height,
            &verts,
            &indices,
        );

        let mut tiling = TilingState::new(self.cfg);
        tiling.ensure_coverage(Complex::ZERO, 3);

        let egui = EguiIntegration::new(&gpu.device, gpu.config.format, window);
        apply_octofact_style(&egui.ctx);
        let icon_atlas = IconAtlas::generate(&egui.ctx);

        self.running = Some(RunningState {
            gpu,
            render,
            tiling,
            extra_elevation: std::collections::HashMap::new(),
            egui,
            icon_atlas,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        use winit::keyboard::PhysicalKey;

        // Handle game toggle keys BEFORE egui so Tab/Esc aren't consumed
        if let WindowEvent::KeyboardInput { ref event, .. } = event {
            if let PhysicalKey::Code(code) = event.physical_key {
                // Track shift state before rebinding check
                if code == winit::keyboard::KeyCode::ShiftLeft || code == winit::keyboard::KeyCode::ShiftRight {
                    self.input_state.shift_held = event.state.is_pressed();
                }

                // Handle rebinding mode
                if let Some(action) = self.rebinding {
                    if event.state.is_pressed() {
                        // Don't bind bare modifier keys
                        if code == winit::keyboard::KeyCode::ShiftLeft || code == winit::keyboard::KeyCode::ShiftRight {
                            return;
                        }
                        let bind = if self.input_state.shift_held {
                            crate::game::input::KeyBind::with_shift(code)
                        } else {
                            crate::game::input::KeyBind::new(code)
                        };
                        self.input_state.rebind(action, bind);
                        self.config.key_bindings.insert(action, bind);
                        self.config.save();
                        self.rebinding = None;
                    }
                    return;
                }

                self.input_state.on_key_event(code, event.state.is_pressed());

                // Handle toggle actions on press (before egui eats them)
                if event.state.is_pressed() {
                    if self.input_state.just_pressed(GameAction::OpenSettings) {
                        self.settings_open = !self.settings_open;
                    }
                    if self.input_state.just_pressed(GameAction::OpenInventory) {
                        self.inventory_open = !self.inventory_open;
                    }
                    if self.input_state.just_pressed(GameAction::ToggleLabels) {
                        if let Some(running) = &mut self.running {
                            running.render.labels_enabled = !running.render.labels_enabled;
                            log::info!("labels: {}", if running.render.labels_enabled { "ON" } else { "OFF" });
                        }
                    }
                    if self.input_state.just_pressed(GameAction::ToggleViewMode) {
                        self.first_person = !self.first_person;
                        self.camera_height = if self.first_person { 0.05 } else { 2.0 };
                        log::info!("view: {}", if self.first_person { "first-person" } else { "top-down" });
                    }
                    if self.input_state.just_pressed(GameAction::ToggleGrid) {
                        self.grid_enabled = !self.grid_enabled;
                        log::info!("grid: {}", if self.grid_enabled { "ON" } else { "OFF" });
                    }
                    if self.input_state.just_pressed(GameAction::RaiseTerrain) {
                        if let Some(pos) = self.cursor_pos {
                            self.handle_click(pos.x, pos.y, 0.04);
                        }
                    }
                    if self.input_state.just_pressed(GameAction::LowerTerrain) {
                        if let Some(pos) = self.cursor_pos {
                            self.handle_click(pos.x, pos.y, -0.04);
                        }
                    }
                }
            }
        }

        // Let egui handle events (for pointer, text input, etc.)
        if let Some(running) = &mut self.running {
            let consumed = running.egui.on_window_event(&running.gpu.window, &event);
            if consumed {
                return;
            }
        }

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
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = Some(position);
            }
            WindowEvent::MouseInput { .. } => {}
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
            if self.flash_timer > 0.0 {
                self.flash_timer = (self.flash_timer - dt as f32).max(0.0);
            }
        }
        self.last_frame = Some(now);
        self.input_state.end_frame();

        if let Some(running) = &self.running {
            running.gpu.window.request_redraw();
        }
    }
}
