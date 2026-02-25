//! GPU instance buffer types for instanced rendering (Phase 6).
//!
//! Each struct is `#[repr(C)]` + `bytemuck::Pod` and provides a
//! `desc()` returning `VertexBufferLayout` with `step_mode: Instance`.
//! Shader locations start at 5 to avoid colliding with the per-vertex
//! attributes (locations 0–2).

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
