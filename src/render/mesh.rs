use crate::hyperbolic::poincare::{geodesic_lerp, Complex};

/// 32-byte vertex: position (12), normal (12), uv (8).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl Vertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // normal
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // uv
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

/// Build a subdivided polygon mesh from Poincare disk vertices.
/// Uses concentric ring subdivision: 8 rings, 4 segments per side.
///
/// Vertices stored in Poincare disk coords as [x, 0, z] — the shader handles
/// Mobius transform + hyperboloid embedding.
///
/// `uv.x` encodes fractional depth (set per-tile at draw time via uniform),
/// `uv.y` encodes distance from center (0 at center, 1 at edge) for edge darkening.
pub fn build_polygon_mesh(vertices: &[Complex]) -> (Vec<Vertex>, Vec<u16>) {
    let num_rings = 8u32;
    let segs_per_side = 4u32;
    let num_sides = vertices.len() as u32;

    let center = Complex::ZERO;
    let mut verts = Vec::new();
    let mut indices = Vec::new();

    // Center vertex
    verts.push(Vertex {
        position: [0.0, 0.0, 0.0],
        normal: [0.0, 1.0, 0.0],
        uv: [0.0, 0.0],
    });

    // Generate ring vertices
    for ring in 1..=num_rings {
        let t_ring = ring as f64 / num_rings as f64;
        for side in 0..num_sides {
            let v0 = vertices[side as usize];
            let v1 = vertices[((side + 1) % num_sides) as usize];
            for seg in 0..segs_per_side {
                let t_seg = seg as f64 / segs_per_side as f64;
                // Interpolate along the edge at this ring's fraction
                let edge_point = geodesic_lerp(v0, v1, t_seg);
                let point = lerp_complex(center, edge_point, t_ring);

                verts.push(Vertex {
                    position: [point.re as f32, 0.0, point.im as f32],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, t_ring as f32],
                });
            }
        }
    }

    let verts_per_ring = (num_sides * segs_per_side) as u16;

    // Triangles from center to first ring
    for i in 0..verts_per_ring {
        let next = (i + 1) % verts_per_ring;
        indices.push(0); // center
        indices.push(1 + i);
        indices.push(1 + next);
    }

    // Triangles between consecutive rings
    for ring in 0..(num_rings - 1) {
        let ring_start = 1 + ring as u16 * verts_per_ring;
        let next_ring_start = ring_start + verts_per_ring;
        for i in 0..verts_per_ring {
            let next = (i + 1) % verts_per_ring;
            // Two triangles per quad
            indices.push(ring_start + i);
            indices.push(next_ring_start + i);
            indices.push(next_ring_start + next);

            indices.push(ring_start + i);
            indices.push(next_ring_start + next);
            indices.push(ring_start + next);
        }
    }

    // Side wall geometry: duplicate outermost ring for top and bottom of prism walls.
    // uv.x flags: 0.0 = top face, 1.0 = side wall top, -1.0 = side wall bottom.
    let outer_ring_start = 1 + (num_rings - 1) as u16 * verts_per_ring;
    let wall_top_start = verts.len() as u16;

    // Side wall top vertices (same positions as outermost ring)
    for i in 0..verts_per_ring {
        let src = &verts[(outer_ring_start + i) as usize];
        verts.push(Vertex {
            position: src.position,
            normal: [0.0, 0.0, 0.0], // shader computes actual normal
            uv: [1.0, 1.0],          // uv.x=1.0 flags side wall top, uv.y=1.0 for edge darkening
        });
    }

    let wall_bot_start = verts.len() as u16;

    // Side wall bottom vertices (same positions, shader places at bowl height without elevation)
    for i in 0..verts_per_ring {
        let src = &verts[(outer_ring_start + i) as usize];
        verts.push(Vertex {
            position: src.position,
            normal: [0.0, 0.0, 0.0],
            uv: [-1.0, 1.0],         // uv.x=-1.0 flags side wall bottom
        });
    }

    // Side wall quads: connect adjacent (top_i, top_next, bot_next, bot_i)
    for i in 0..verts_per_ring {
        let next = (i + 1) % verts_per_ring;
        let ti = wall_top_start + i;
        let tn = wall_top_start + next;
        let bi = wall_bot_start + i;
        let bn = wall_bot_start + next;

        // Two triangles per quad, wound CCW when viewed from outside
        indices.push(ti);
        indices.push(bi);
        indices.push(bn);

        indices.push(ti);
        indices.push(bn);
        indices.push(tn);
    }

    (verts, indices)
}

fn lerp_complex(a: Complex, b: Complex, t: f64) -> Complex {
    Complex::new(a.re + (b.re - a.re) * t, a.im + (b.im - a.im) * t)
}

/// 16-byte 2D quad vertex: position (8) + uv (8).
/// Used for belt, machine, and item quads rendered on the tile surface.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct QuadVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
}

impl QuadVertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<QuadVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

/// Build a unit quad mesh centered at origin ([-0.5, 0.5] in both axes).
/// Returns 4 vertices + 6 indices (two triangles).
pub fn build_quad_mesh() -> (Vec<QuadVertex>, Vec<u16>) {
    let verts = vec![
        QuadVertex { pos: [-0.5, -0.5], uv: [0.0, 0.0] },
        QuadVertex { pos: [ 0.5, -0.5], uv: [1.0, 0.0] },
        QuadVertex { pos: [ 0.5,  0.5], uv: [1.0, 1.0] },
        QuadVertex { pos: [-0.5,  0.5], uv: [0.0, 1.0] },
    ];
    let indices = vec![0, 1, 2, 0, 2, 3];
    (verts, indices)
}

/// Build a box mesh: top face + 4 side walls extending down to the tile surface.
/// Top face uses uv.y in [0, 1]. Side walls use uv.y in [2, 3]:
///   uv.y = 2.0 → side wall top edge (lifted)
///   uv.y = 3.0 → side wall bottom edge (tile surface)
/// The shader checks uv.y > 1.5 to detect side wall fragments.
pub fn build_box_mesh() -> (Vec<QuadVertex>, Vec<u16>) {
    build_subdivided_box_mesh(1)
}

/// Build a subdivided box mesh for multi-cell machines.
/// The top face is an `n × n` grid so intermediate vertices get properly
/// transformed through Klein → Poincaré → Möbius → bowl in the shader,
/// preventing large flat-quad distortion on the curved surface.
/// Side walls are subdivided `n` times along each edge.
pub fn build_subdivided_box_mesh(n: u32) -> (Vec<QuadVertex>, Vec<u16>) {
    let mut verts = Vec::new();
    let mut indices = Vec::new();
    let nf = n as f32;

    // Top face: (n+1)² vertices, 2·n² triangles
    for j in 0..=n {
        for i in 0..=n {
            let u = i as f32 / nf;
            let v = j as f32 / nf;
            verts.push(QuadVertex {
                pos: [-0.5 + u, -0.5 + v],
                uv: [u, v],
            });
        }
    }
    let stride = n + 1;
    for j in 0..n {
        for i in 0..n {
            let tl = (j * stride + i) as u16;
            let tr = tl + 1;
            let bl = tl + stride as u16;
            let br = bl + 1;
            indices.extend_from_slice(&[tl, tr, br, tl, br, bl]);
        }
    }

    // 4 side walls, each subdivided n times along the edge.
    // Corners of the unit quad in CCW order.
    let corners: [[f32; 2]; 4] = [
        [-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5],
    ];
    for side in 0..4usize {
        let next = (side + 1) % 4;
        let c0 = corners[side];
        let c1 = corners[next];
        let base = verts.len() as u16;

        // n+1 pairs of (top, bottom) vertices along the edge
        for k in 0..=n {
            let t = k as f32 / nf;
            let px = c0[0] + (c1[0] - c0[0]) * t;
            let py = c0[1] + (c1[1] - c0[1]) * t;
            // Top edge (lifted by shader)
            verts.push(QuadVertex { pos: [px, py], uv: [t, 2.0] });
            // Bottom edge (at tile surface)
            verts.push(QuadVertex { pos: [px, py], uv: [t, 3.0] });
        }

        // n quads along the wall
        for k in 0..n {
            let k2 = k * 2;
            let tl = base + k2 as u16;       // top-left
            let bl = tl + 1;                  // bottom-left
            let tr = tl + 2;                  // top-right
            let br = tl + 3;                  // bottom-right
            indices.extend_from_slice(&[tl, tr, br, tl, br, bl]);
        }
    }

    (verts, indices)
}
