use crate::hyperbolic::poincare::Complex;

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

/// Build a subdivided octagon mesh from 8 Poincare disk vertices.
/// Uses concentric ring subdivision: 4 rings, 3 segments per octagon side.
///
/// Vertices stored in Poincare disk coords as [x, 0, z] — the shader handles
/// Mobius transform + hyperboloid embedding.
///
/// `uv.x` encodes fractional depth (set per-tile at draw time via uniform),
/// `uv.y` encodes distance from center (0 at center, 1 at edge) for edge darkening.
pub fn build_octagon_mesh(vertices: &[Complex; 8]) -> (Vec<Vertex>, Vec<u16>) {
    let num_rings = 8u32;
    let segs_per_side = 4u32;
    let num_sides = 8u32;

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
                let edge_point = lerp_complex(v0, v1, t_seg);
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
