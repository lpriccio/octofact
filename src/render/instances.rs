//! GPU instance buffer types for instanced rendering (Phase 6).
//!
//! Each struct is `#[repr(C)]` + `bytemuck::Pod` and provides a
//! `desc()` returning `VertexBufferLayout` with `step_mode: Instance`.
//! Shader locations start at 5 to avoid colliding with the per-vertex
//! attributes (locations 0–2).
//!
//! [`InstanceBuffer<T>`] manages a CPU staging vec + GPU buffer with
//! grow-on-demand upload for any `Pod` instance type.

/// Generic instance buffer: CPU staging vec + GPU vertex buffer.
///
/// Usage each frame:
/// 1. `clear()`
/// 2. `push()` instances
/// 3. `upload(device, queue)` — writes staging data to GPU, growing buffer if needed
/// 4. Bind with `set_vertex_buffer(slot, buf.slice())`
/// 5. Draw with `0..buf.count()` instances
pub struct InstanceBuffer<T: bytemuck::Pod> {
    staging: Vec<T>,
    buffer: wgpu::Buffer,
    /// Current GPU buffer capacity in number of elements.
    capacity: usize,
    label: &'static str,
}

impl<T: bytemuck::Pod> InstanceBuffer<T> {
    /// Create a new instance buffer with the given initial capacity (in elements).
    pub fn new(device: &wgpu::Device, label: &'static str, initial_capacity: usize) -> Self {
        let capacity = initial_capacity.max(1);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (capacity * std::mem::size_of::<T>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            staging: Vec::with_capacity(capacity),
            buffer,
            capacity,
            label,
        }
    }

    /// Clear the staging buffer for the next frame.
    pub fn clear(&mut self) {
        self.staging.clear();
    }

    /// Push an instance into the staging buffer.
    pub fn push(&mut self, instance: T) {
        self.staging.push(instance);
    }

    /// Number of instances staged for this frame.
    pub fn count(&self) -> u32 {
        self.staging.len() as u32
    }

    /// Upload staging data to the GPU buffer, growing the buffer if needed.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.staging.is_empty() {
            return;
        }
        // Grow GPU buffer if staging exceeds capacity.
        if self.staging.len() > self.capacity {
            self.capacity = self.staging.len().next_power_of_two();
            self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: (self.capacity * std::mem::size_of::<T>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.staging));
    }

    /// Return the buffer slice covering all uploaded instances.
    pub fn slice(&self) -> wgpu::BufferSlice<'_> {
        let size = self.staging.len() * std::mem::size_of::<T>();
        self.buffer.slice(..size as u64)
    }
}

/// Per-tile instance data. Carries the composed Mobius transform
/// (camera × tile) so the vertex shader can position each tile in
/// a single instanced draw call.
///
/// 24 bytes (6 floats). Shader locations 5–8.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TileInstance {
    /// Mobius numerator coefficient (complex: re, im).
    pub mobius_a: [f32; 2],
    /// Mobius denominator coefficient (complex: re, im).
    pub mobius_b: [f32; 2],
    /// Tile depth in BFS tree (drives palette color cycling).
    pub depth: f32,
    /// Extra elevation offset for this tile.
    pub elevation: f32,
}

impl TileInstance {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TileInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // mobius_a
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // mobius_b
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // depth
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32,
                },
                // elevation
                wgpu::VertexAttribute {
                    offset: 20,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}

/// Per-belt-segment instance data. Positions a small rectangle on
/// the tile surface at a specific grid cell, with direction for
/// arrow animation.
///
/// 28 bytes (7 floats). Shader locations 5–10.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BeltInstance {
    /// Mobius numerator coefficient for the parent tile.
    pub mobius_a: [f32; 2],
    /// Mobius denominator coefficient for the parent tile.
    pub mobius_b: [f32; 2],
    /// Grid cell position within the tile (0..63, 0..63), packed as floats.
    pub grid_pos: [f32; 2],
    /// Belt direction encoded as float: 0=North, 1=East, 2=South, 3=West.
    pub direction: f32,
}

impl BeltInstance {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<BeltInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // mobius_a
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // mobius_b
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // grid_pos
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // direction
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}

/// Per-machine instance data. Positions a machine visual on the tile
/// surface at a grid cell, with type and crafting state for the shader.
///
/// 32 bytes (8 floats). Shader locations 5–11.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MachineInstance {
    /// Mobius numerator coefficient for the parent tile.
    pub mobius_a: [f32; 2],
    /// Mobius denominator coefficient for the parent tile.
    pub mobius_b: [f32; 2],
    /// Grid cell position within the tile (0..63, 0..63).
    pub grid_pos: [f32; 2],
    /// Machine type: 0=Composer, 1=Inverter, 2=Embedder, 3=Quotient,
    /// 4=Transformer, 5=Source.
    pub machine_type: f32,
    /// Crafting progress 0.0–1.0, or negative for special states
    /// (-1.0 = idle, -2.0 = no power).
    pub progress: f32,
}

impl MachineInstance {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<MachineInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // mobius_a
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // mobius_b
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // grid_pos
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // machine_type
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32,
                },
                // progress
                wgpu::VertexAttribute {
                    offset: 28,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}

/// Per-item instance data. Positions a billboard/sprite on a belt
/// at a specific fractional position along the transport line.
///
/// 28 bytes (7 floats). Shader locations 5–10.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ItemInstance {
    /// Mobius numerator coefficient for the parent tile.
    pub mobius_a: [f32; 2],
    /// Mobius denominator coefficient for the parent tile.
    pub mobius_b: [f32; 2],
    /// Position in Klein-model coordinates relative to tile center.
    /// Pre-computed from grid cell + fractional offset along belt.
    pub klein_pos: [f32; 2],
    /// Item type index for color/texture lookup in the shader.
    pub item_type: f32,
}

impl ItemInstance {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ItemInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // mobius_a
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // mobius_b
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // klein_pos
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // item_type
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}
