use super::instances::{BeltInstance, ItemInstance, MachineInstance, TileInstance};
use super::mesh::{QuadVertex, Vertex};

/// Global uniforms shared across all instanced draw calls.
/// Per-tile data (Mobius, depth, elevation) lives in instance buffers.
///
/// WGSL layout (96 bytes):
///   view_proj: mat4x4<f32>  (64)
///   grid_params: vec4<f32>  (16) — enabled, divisions, line_width, klein_half_side
///   color_cycle: f32        (4)
///   _pad: 12 bytes          (align to 16)
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4],
    pub grid_params: [f32; 4],
    pub color_cycle: f32,
    pub _pad: [f32; 3],
}

/// Max tiles we can draw per frame.
pub const MAX_TILES: usize = 1024;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Holds the shared tile mesh buffers, depth buffer, and misc render state.
pub struct RenderState {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
    pub depth_view: wgpu::TextureView,
    pub labels_enabled: bool,
}

impl RenderState {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, vertices: &[Vertex], indices: &[u16]) -> Self {
        use wgpu::util::DeviceExt;

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("index buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let depth_view = Self::create_depth_view(device, width, height);

        Self {
            vertex_buffer,
            index_buffer,
            num_indices: indices.len() as u32,
            depth_view,
            labels_enabled: false,
        }
    }

    fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth texture"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    pub fn resize_depth(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.depth_view = Self::create_depth_view(device, width, height);
    }
}

/// Instanced tile pipeline: single draw call for all visible tiles.
/// Uses Globals uniform (bind group 0) + per-instance TileInstance (vertex slot 1).
pub struct TilePipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub globals_buffer: wgpu::Buffer,
    pub globals_bind_group: wgpu::BindGroup,
}

impl TilePipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let common_src = include_str!("common.wgsl");
        let tile_src = include_str!("tile.wgsl");
        let full_src = format!("{}\n{}", common_src, tile_src);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tile instanced shader"),
            source: wgpu::ShaderSource::Wgsl(full_src.into()),
        });

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals uniform buffer"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("globals bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<Globals>() as u64,
                        ),
                    },
                    count: None,
                }],
            });

        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tile instanced pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("tile instanced pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_tile"),
                buffers: &[Vertex::desc(), TileInstance::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_tile"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            globals_buffer,
            globals_bind_group,
        }
    }

    /// Upload global uniforms for this frame.
    pub fn upload_globals(&self, queue: &wgpu::Queue, globals: &Globals) {
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(globals));
    }
}

/// Instanced belt pipeline: one draw call for all visible belt segments.
/// Uses the same Globals uniform bind group as TilePipeline.
pub struct BeltPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
}

impl BeltPipeline {
    /// Create the belt pipeline. `globals_layout` should come from
    /// `tile_pipeline.pipeline.get_bind_group_layout(0)` to share the same bind group.
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        globals_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        use wgpu::util::DeviceExt;

        let common_src = include_str!("common.wgsl");
        let belt_src = include_str!("belt.wgsl");
        let full_src = format!("{}\n{}", common_src, belt_src);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("belt instanced shader"),
            source: wgpu::ShaderSource::Wgsl(full_src.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("belt pipeline layout"),
            bind_group_layouts: &[globals_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("belt instanced pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_belt"),
                buffers: &[QuadVertex::desc(), BeltInstance::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_belt"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Build box mesh for belt segments (top face + side walls)
        let (quad_verts, quad_indices) = crate::render::mesh::build_box_mesh();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("belt vertex buffer"),
            contents: bytemuck::cast_slice(&quad_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("belt index buffer"),
            contents: bytemuck::cast_slice(&quad_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            num_indices: quad_indices.len() as u32,
        }
    }
}

/// Instanced machine pipeline: one draw call for all visible machines.
/// Uses the same Globals uniform bind group as TilePipeline.
pub struct MachinePipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
}

impl MachinePipeline {
    /// Create the machine pipeline. `globals_layout` should come from
    /// `tile_pipeline.pipeline.get_bind_group_layout(0)` to share the same bind group.
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        globals_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        use wgpu::util::DeviceExt;

        let common_src = include_str!("common.wgsl");
        let machine_src = include_str!("machine.wgsl");
        let full_src = format!("{}\n{}", common_src, machine_src);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("machine instanced shader"),
            source: wgpu::ShaderSource::Wgsl(full_src.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("machine pipeline layout"),
            bind_group_layouts: &[globals_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("machine instanced pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_machine"),
                buffers: &[QuadVertex::desc(), MachineInstance::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_machine"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Build subdivided box mesh for machines (top face + side walls).
        // n=8 gives enough vertices so multi-cell machines (up to 3×2 cells)
        // properly follow the curved hyperbolic surface.
        let (quad_verts, quad_indices) = crate::render::mesh::build_subdivided_box_mesh(8);

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("machine vertex buffer"),
            contents: bytemuck::cast_slice(&quad_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("machine index buffer"),
            contents: bytemuck::cast_slice(&quad_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            num_indices: quad_indices.len() as u32,
        }
    }
}

/// Instanced item pipeline: one draw call for all visible items on belts.
/// Uses the same Globals uniform bind group as TilePipeline.
pub struct ItemPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
}

impl ItemPipeline {
    /// Create the item pipeline. `globals_layout` should come from
    /// `tile_pipeline.pipeline.get_bind_group_layout(0)` to share the same bind group.
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        globals_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        use wgpu::util::DeviceExt;

        let common_src = include_str!("common.wgsl");
        let item_src = include_str!("item.wgsl");
        let full_src = format!("{}\n{}", common_src, item_src);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("item instanced shader"),
            source: wgpu::ShaderSource::Wgsl(full_src.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("item pipeline layout"),
            bind_group_layouts: &[globals_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("item instanced pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_item"),
                buffers: &[QuadVertex::desc(), ItemInstance::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_item"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let (quad_verts, quad_indices) = crate::render::mesh::build_quad_mesh();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("item vertex buffer"),
            contents: bytemuck::cast_slice(&quad_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("item index buffer"),
            contents: bytemuck::cast_slice(&quad_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            num_indices: quad_indices.len() as u32,
        }
    }
}
