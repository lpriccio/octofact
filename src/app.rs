use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

use crate::hyperbolic::poincare::{canonical_octagon, Complex, Mobius};
use crate::hyperbolic::tiling::{format_address, TilingState};
use crate::render::mesh::{build_octagon_mesh, Vertex};
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

pub struct LabelState {
    pub font_system: glyphon::FontSystem,
    pub swash_cache: glyphon::SwashCache,
    #[allow(dead_code)] // kept alive for glyphon shared resources
    pub cache: glyphon::Cache,
    pub atlas: glyphon::TextAtlas,
    pub renderer: glyphon::TextRenderer,
    pub viewport: glyphon::Viewport,
    pub enabled: bool,
}

impl LabelState {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let font_system = glyphon::FontSystem::new();
        let swash_cache = glyphon::SwashCache::new();
        let cache = glyphon::Cache::new(device);
        let mut atlas = glyphon::TextAtlas::new(device, queue, &cache, format);
        let renderer = glyphon::TextRenderer::new(
            &mut atlas,
            device,
            wgpu::MultisampleState::default(),
            None,
        );
        let viewport = glyphon::Viewport::new(device, &cache);

        Self {
            font_system,
            swash_cache,
            cache,
            atlas,
            renderer,
            viewport,
            enabled: false,
        }
    }
}

pub struct App {
    gpu: Option<GpuState>,
    render: Option<RenderState>,
    labels: Option<LabelState>,
    tiling: Option<TilingState>,
    mesh_verts: Vec<Vertex>,
    mesh_indices: Vec<u16>,
    camera_height: f32,
    view_mobius: Mobius,
    keys_held: std::collections::HashSet<winit::keyboard::KeyCode>,
    last_frame: Option<std::time::Instant>,
}

impl App {
    pub fn new() -> Self {
        let octagon = canonical_octagon();
        let (verts, indices) = build_octagon_mesh(&octagon);

        Self {
            gpu: None,
            render: None,
            labels: None,
            tiling: None,
            mesh_verts: verts,
            mesh_indices: indices,
            camera_height: 2.0,
            view_mobius: Mobius::identity(),
            keys_held: std::collections::HashSet::new(),
            last_frame: None,
        }
    }

    fn build_view_proj(&self, aspect: f32) -> glam::Mat4 {
        let eye = glam::Vec3::new(0.0, self.camera_height, 0.0);
        let center = glam::Vec3::new(0.0, 0.0, 0.0);
        let up = glam::Vec3::new(0.0, 0.0, -1.0);
        let view = glam::Mat4::look_at_rh(eye, center, up);
        let proj = glam::Mat4::perspective_rh(60.0_f32.to_radians(), aspect, 0.1, 100.0);
        proj * view
    }

    fn process_movement(&mut self, dt: f64) {
        use winit::keyboard::KeyCode;

        let move_speed = 0.8 * dt;
        let mut dx = 0.0_f64;
        let mut dy = 0.0_f64;

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
        if self.keys_held.contains(&KeyCode::KeyQ) {
            self.camera_height = (self.camera_height + 2.0 * dt as f32).min(20.0);
        }
        if self.keys_held.contains(&KeyCode::KeyE) {
            self.camera_height = (self.camera_height - 2.0 * dt as f32).max(1.5);
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
            self.view_mobius = translation.compose(&self.view_mobius);

            // Rebase check
            let origin_pos = self.view_mobius.apply(Complex::ZERO);
            if origin_pos.abs() > 0.5 {
                if let Some(tiling) = &mut self.tiling {
                    let rebase = self.view_mobius.inverse();
                    tiling.rebase(&rebase);
                    self.view_mobius = Mobius::identity();
                    tiling.expand(2);
                }
            }
        }
    }


    fn render_frame(&mut self) -> Result<(), wgpu::SurfaceError> {
        let gpu = self.gpu.as_ref().unwrap();
        let render = self.render.as_ref().unwrap();
        let tiling = self.tiling.as_ref().unwrap();

        let output = gpu.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let aspect = gpu.config.width as f32 / gpu.config.height as f32;
        let view_proj = self.build_view_proj(aspect);
        let width = gpu.config.width as f32;
        let height = gpu.config.height as f32;

        let tile_count = tiling.tiles.len().min(crate::render::pipeline::MAX_TILES);
        let inv_view = self.view_mobius.inverse();

        // Upload all tile uniforms
        for (i, tile) in tiling.tiles.iter().enumerate().take(tile_count) {
            let combined = inv_view.compose(&tile.transform);
            let uniforms = Uniforms {
                view_proj: view_proj.to_cols_array_2d(),
                mobius_a: [combined.a.re as f32, combined.a.im as f32, 0.0, 0.0],
                mobius_b: [combined.b.re as f32, combined.b.im as f32, 0.0, 0.0],
                disk_params: [tile.depth as f32, 0.0, 0.0, 0.0],
                ..Default::default()
            };
            render.write_tile_uniforms(&gpu.queue, i, &uniforms);
        }

        // Prepare label text if enabled
        let labels_enabled = self.labels.as_ref().is_some_and(|l| l.enabled);
        let mut label_buffers: Vec<glyphon::Buffer> = Vec::new();
        let mut label_positions: Vec<(f32, f32)> = Vec::new();

        if labels_enabled {
            let labels = self.labels.as_mut().unwrap();
            labels.viewport.update(
                &gpu.queue,
                glyphon::Resolution {
                    width: gpu.config.width,
                    height: gpu.config.height,
                },
            );

            let metrics = glyphon::Metrics::new(28.0, 32.0);

            for tile in tiling.tiles.iter().take(tile_count) {
                let combined = inv_view.compose(&tile.transform);
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
                    let mut buffer = glyphon::Buffer::new(&mut labels.font_system, metrics);
                    buffer.set_size(&mut labels.font_system, Some(100.0), Some(20.0));
                    buffer.set_text(
                        &mut labels.font_system,
                        &text,
                        &glyphon::cosmic_text::Attrs::new(),
                        glyphon::cosmic_text::Shaping::Advanced,
                        None,
                    );
                    buffer.shape_until_scroll(&mut labels.font_system, false);

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
                        right: gpu.config.width as i32,
                        bottom: gpu.config.height as i32,
                    },
                    default_color: glyphon::Color::rgb(255, 255, 255),
                    custom_glyphs: &[],
                })
                .collect();

            let _ = labels.renderer.prepare(
                &gpu.device,
                &gpu.queue,
                &mut labels.font_system,
                &mut labels.atlas,
                &labels.viewport,
                text_areas,
                &mut labels.swash_cache,
            );
        }

        let mut encoder = gpu
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
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Draw tiles
            pass.set_pipeline(&render.pipeline);
            pass.set_vertex_buffer(0, render.vertex_buffer.slice(..));
            pass.set_index_buffer(render.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

            for i in 0..tile_count {
                let offset = RenderState::dynamic_offset(i);
                pass.set_bind_group(0, &render.bind_group, &[offset]);
                pass.draw_indexed(0..render.num_indices, 0, 0..1);
            }

            // Draw labels overlay
            if labels_enabled {
                let labels = self.labels.as_ref().unwrap();
                let _ = labels.renderer.render(&labels.atlas, &labels.viewport, &mut pass);
            }
        }

        gpu.queue.submit(std::iter::once(encoder.finish()));

        // Trim atlas after present
        if labels_enabled {
            let labels = self.labels.as_mut().unwrap();
            labels.atlas.trim();
        }

        output.present();
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
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

        let render = RenderState::new(
            &gpu.device,
            gpu.config.format,
            &self.mesh_verts,
            &self.mesh_indices,
        );

        let labels = LabelState::new(&gpu.device, &gpu.queue, gpu.config.format);

        let mut tiling = TilingState::new();
        tiling.expand(4);

        self.gpu = Some(gpu);
        self.render = Some(render);
        self.labels = Some(labels);
        self.tiling = Some(tiling);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        use winit::keyboard::{KeyCode, PhysicalKey};

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(new_size.width, new_size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if event.state.is_pressed() {
                        self.keys_held.insert(code);
                        match code {
                            KeyCode::Escape => event_loop.exit(),
                            KeyCode::KeyL => {
                                if let Some(labels) = &mut self.labels {
                                    labels.enabled = !labels.enabled;
                                    log::info!("labels: {}", if labels.enabled { "ON" } else { "OFF" });
                                }
                            }
                            _ => {}
                        }
                    } else {
                        self.keys_held.remove(&code);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if self.gpu.is_some() && self.render.is_some() && self.tiling.is_some() {
                    match self.render_frame() {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => {
                            let gpu = self.gpu.as_ref().unwrap();
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

        if let Some(gpu) = &self.gpu {
            gpu.window.request_redraw();
        }
    }
}
