use std::sync::Arc;
use winit::window::Window;

use crate::hyperbolic::poincare::{Complex, Mobius};
use crate::hyperbolic::tiling::TilingState;
use crate::render::instances::{BeltInstance, InstanceBuffer, ItemInstance, MachineInstance, TileInstance};
use crate::render::pipeline::{BeltPipeline, Globals, ItemPipeline, MachinePipeline, RenderState, TilePipeline, MAX_TILES};
use crate::ui::icons::IconAtlas;
use crate::ui::integration::EguiIntegration;
use crate::ui::style::apply_octofact_style;

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

/// Owns all GPU rendering state: device, pipelines, egui integration, etc.
/// Created once on window open, used each frame for drawing.
pub struct RenderEngine {
    pub gpu: GpuState,
    pub render: RenderState,
    pub tile_pipeline: TilePipeline,
    pub tile_instances: InstanceBuffer<TileInstance>,
    pub belt_pipeline: BeltPipeline,
    pub belt_instances: InstanceBuffer<BeltInstance>,
    pub machine_pipeline: MachinePipeline,
    pub machine_instances: InstanceBuffer<MachineInstance>,
    pub item_pipeline: ItemPipeline,
    pub item_instances: InstanceBuffer<ItemInstance>,
    pub tiling: TilingState,
    pub extra_elevation: std::collections::HashMap<usize, f32>,
    pub egui: EguiIntegration,
    pub icon_atlas: IconAtlas,
}

impl RenderEngine {
    pub fn new(
        window: Arc<Window>,
        cfg: crate::hyperbolic::poincare::TilingConfig,
        vertices: &[crate::render::mesh::Vertex],
        indices: &[u16],
    ) -> Self {
        let gpu = GpuState::new(window.clone());

        let render = RenderState::new(
            &gpu.device,
            &gpu.queue,
            gpu.config.format,
            gpu.config.width,
            gpu.config.height,
            vertices,
            indices,
        );

        let mut tiling = TilingState::new(cfg);
        tiling.ensure_coverage(Complex::ZERO, 3);

        let tile_pipeline = TilePipeline::new(&gpu.device, gpu.config.format);
        let tile_instances = InstanceBuffer::new(&gpu.device, "tile instances", 256);

        let globals_layout = tile_pipeline.pipeline.get_bind_group_layout(0);
        let belt_pipeline = BeltPipeline::new(&gpu.device, gpu.config.format, &globals_layout);
        let belt_instances = InstanceBuffer::new(&gpu.device, "belt instances", 256);

        let machine_pipeline = MachinePipeline::new(&gpu.device, gpu.config.format, &globals_layout);
        let machine_instances = InstanceBuffer::new(&gpu.device, "machine instances", 64);

        let item_pipeline = ItemPipeline::new(&gpu.device, gpu.config.format, &globals_layout);
        let item_instances = InstanceBuffer::new(&gpu.device, "item instances", 256);

        let egui = EguiIntegration::new(&gpu.device, gpu.config.format, window);
        apply_octofact_style(&egui.ctx);
        let icon_atlas = IconAtlas::generate(&egui.ctx);

        Self {
            gpu,
            render,
            tile_pipeline,
            tile_instances,
            belt_pipeline,
            belt_instances,
            machine_pipeline,
            machine_instances,
            item_pipeline,
            item_instances,
            tiling,
            extra_elevation: std::collections::HashMap::new(),
            egui,
            icon_atlas,
        }
    }

    /// Compute visible tiles (culled by disk distance) with composed Mobius transforms.
    pub fn visible_tiles(&self, inv_view: &Mobius) -> Vec<(usize, Mobius)> {
        self.tiling
            .tiles
            .iter()
            .enumerate()
            .filter_map(|(i, tile)| {
                let combined = inv_view.compose(&tile.transform);
                let center = combined.apply(Complex::ZERO);
                if center.abs() < 0.98 {
                    Some((i, combined))
                } else {
                    None
                }
            })
            .take(MAX_TILES)
            .collect()
    }

    /// Build tile instance buffer and upload globals for instanced rendering.
    pub fn build_tile_instances(
        &mut self,
        visible: &[(usize, Mobius)],
        view_proj: &glam::Mat4,
        grid_enabled: bool,
        klein_half_side: f32,
    ) {
        // Build instance data
        self.tile_instances.clear();
        for &(tile_idx, combined) in visible {
            let tile = &self.tiling.tiles[tile_idx];
            let elevation = self.extra_elevation.get(&tile_idx).copied().unwrap_or(0.0);
            self.tile_instances.push(TileInstance {
                mobius_a: [combined.a.re as f32, combined.a.im as f32],
                mobius_b: [combined.b.re as f32, combined.b.im as f32],
                depth: tile.depth as f32,
                elevation,
            });
        }
        self.tile_instances.upload(&self.gpu.device, &self.gpu.queue);

        // Upload global uniforms
        let globals = Globals {
            view_proj: view_proj.to_cols_array_2d(),
            grid_params: [
                if grid_enabled { 1.0 } else { 0.0 },
                64.0,
                0.03,
                klein_half_side,
            ],
            color_cycle: 13.0,
            _pad: [0.0; 3],
        };
        self.tile_pipeline.upload_globals(&self.gpu.queue, &globals);
    }

    /// Execute the main wgpu render pass (tiles) and egui render pass, then submit.
    /// `egui_output` should be the result of `egui.end_frame()`.
    pub fn draw_and_submit(
        &mut self,
        egui_output: &egui::FullOutput,
    ) -> Result<wgpu::SurfaceTexture, wgpu::SurfaceError> {
        let output = self.gpu.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.gpu.config.width, self.gpu.config.height],
            pixels_per_point: self.gpu.window.scale_factor() as f32,
        };

        let mut encoder = self.gpu.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("render encoder"),
            },
        );

        // Prepare egui (tessellate, update textures/buffers)
        let paint_jobs = self.egui.prepare(
            &self.gpu.device,
            &self.gpu.queue,
            &mut encoder,
            &screen,
            egui_output,
        );

        // Main render pass: draw tiles (instanced)
        let tile_count = self.tile_instances.count();
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
                    view: &self.render.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if tile_count > 0 {
                pass.set_pipeline(&self.tile_pipeline.pipeline);
                pass.set_bind_group(0, &self.tile_pipeline.globals_bind_group, &[]);
                pass.set_vertex_buffer(0, self.render.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, self.tile_instances.slice());
                pass.set_index_buffer(
                    self.render.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                pass.draw_indexed(0..self.render.num_indices, 0, 0..tile_count);
            }

            // Draw belt segments (instanced)
            let belt_count = self.belt_instances.count();
            if belt_count > 0 {
                pass.set_pipeline(&self.belt_pipeline.pipeline);
                pass.set_bind_group(0, &self.tile_pipeline.globals_bind_group, &[]);
                pass.set_vertex_buffer(0, self.belt_pipeline.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, self.belt_instances.slice());
                pass.set_index_buffer(
                    self.belt_pipeline.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                pass.draw_indexed(0..self.belt_pipeline.num_indices, 0, 0..belt_count);
            }

            // Draw machines (instanced)
            let machine_count = self.machine_instances.count();
            if machine_count > 0 {
                pass.set_pipeline(&self.machine_pipeline.pipeline);
                pass.set_bind_group(0, &self.tile_pipeline.globals_bind_group, &[]);
                pass.set_vertex_buffer(0, self.machine_pipeline.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, self.machine_instances.slice());
                pass.set_index_buffer(
                    self.machine_pipeline.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                pass.draw_indexed(0..self.machine_pipeline.num_indices, 0, 0..machine_count);
            }

            // Draw items on belts (instanced)
            let item_count = self.item_instances.count();
            if item_count > 0 {
                pass.set_pipeline(&self.item_pipeline.pipeline);
                pass.set_bind_group(0, &self.tile_pipeline.globals_bind_group, &[]);
                pass.set_vertex_buffer(0, self.item_pipeline.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, self.item_instances.slice());
                pass.set_index_buffer(
                    self.item_pipeline.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                pass.draw_indexed(0..self.item_pipeline.num_indices, 0, 0..item_count);
            }
        }

        // Egui render pass
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
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
            self.egui.renderer.render(&mut pass, &paint_jobs, &screen);
        }

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        self.egui.cleanup(egui_output);
        Ok(output)
    }

    pub fn width(&self) -> f32 {
        self.gpu.config.width as f32
    }

    pub fn height(&self) -> f32 {
        self.gpu.config.height as f32
    }

    pub fn scale_factor(&self) -> f32 {
        self.gpu.window.scale_factor() as f32
    }
}

/// Project a 3D world position to screen coordinates.
/// Returns None if behind the camera or outside the viewport.
pub fn project_to_screen(
    world_pos: glam::Vec3,
    view_proj: &glam::Mat4,
    width: f32,
    height: f32,
) -> Option<(f32, f32)> {
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

/// Like project_to_screen but allows points slightly outside the viewport
/// (for partially visible geometry). Coordinates are clamped to 3x NDC
/// to prevent degenerate polygons when clip.w is near zero.
pub fn project_to_screen_unclamped(
    world_pos: glam::Vec3,
    view_proj: &glam::Mat4,
    width: f32,
    height: f32,
) -> Option<(f32, f32)> {
    let clip = *view_proj * glam::Vec4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);
    if clip.w <= 0.01 {
        return None;
    }
    let ndc_x = (clip.x / clip.w).clamp(-3.0, 3.0);
    let ndc_y = (clip.y / clip.w).clamp(-3.0, 3.0);
    let screen_x = (ndc_x + 1.0) * 0.5 * width;
    let screen_y = (1.0 - ndc_y) * 0.5 * height;
    Some((screen_x, screen_y))
}
