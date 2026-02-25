// Instanced belt shader: positions a small quad on the tile surface per belt segment.
// Prepended by common.wgsl at load time.

struct Globals {
    view_proj: mat4x4<f32>,
    grid_params: vec4<f32>,  // (enabled, divisions, line_width, klein_half_side)
    color_cycle: f32,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct VertexInput {
    @location(0) local_pos: vec2<f32>,  // unit quad: -0.5 to 0.5
    @location(1) uv: vec2<f32>,         // 0 to 1
};

struct InstanceInput {
    @location(5) mobius_a: vec2<f32>,
    @location(6) mobius_b: vec2<f32>,
    @location(7) grid_pos: vec2<f32>,   // grid cell coords (e.g. -32..+31)
    @location(8) direction: f32,        // 0=N, 1=E, 2=S, 3=W
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) direction: f32,
    @location(2) disk_r: f32,
};

@vertex
fn vs_belt(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;

    let divisions = globals.grid_params.y;  // 64.0
    let khs = globals.grid_params.w;        // klein_half_side
    let cell_size = 2.0 * khs / divisions;

    // Scale unit quad to grid cell size (slightly inset to show gaps between cells)
    let inset = 0.92;
    let klein = vert.local_pos * cell_size * inset + inst.grid_pos / divisions * 2.0 * khs;

    // Klein -> Poincare -> Mobius -> bowl -> clip
    let poincare = klein_to_poincare(klein);
    let disk = apply_mobius(poincare, inst.mobius_a, inst.mobius_b);
    var world = disk_to_bowl(disk);
    world.y += 0.002;  // lift above tile surface to prevent z-fighting

    out.clip_position = globals.view_proj * vec4<f32>(world, 1.0);
    out.uv = vert.uv;
    out.direction = inst.direction;
    out.disk_r = length(disk);

    return out;
}

@fragment
fn fs_belt(in: VertexOutput) -> @location(0) vec4<f32> {
    // Distance fade near disk boundary
    let fade = 1.0 - smoothstep(0.85, 0.95, in.disk_r);
    if fade < 0.01 { discard; }

    // Base belt color: metallic grey
    var color = vec3<f32>(0.55, 0.55, 0.57);

    // Edge bevel: darken at quad edges for a 3D raised look
    let ex = smoothstep(0.0, 0.12, min(in.uv.x, 1.0 - in.uv.x));
    let ey = smoothstep(0.0, 0.12, min(in.uv.y, 1.0 - in.uv.y));
    let edge = ex * ey;
    color = mix(vec3<f32>(0.08, 0.08, 0.08), color, edge);

    // Highlight top-left edge, shadow bottom-right (bevel)
    let highlight = (1.0 - in.uv.x) * (1.0 - in.uv.y);
    let shadow = in.uv.x * in.uv.y;
    color += vec3<f32>(0.08) * smoothstep(0.3, 0.8, highlight) * edge;
    color -= vec3<f32>(0.06) * smoothstep(0.3, 0.8, shadow) * edge;

    // Direction arrow
    let dir = u32(in.direction + 0.5);
    let c = in.uv - 0.5;  // centered: -0.5 to 0.5

    // Direction vectors in grid space
    var dv = vec2<f32>(0.0, 0.0);
    switch dir {
        case 0u: { dv = vec2<f32>(0.0, -1.0); }  // North
        case 1u: { dv = vec2<f32>(1.0, 0.0); }    // East
        case 2u: { dv = vec2<f32>(0.0, 1.0); }    // South
        case 3u: { dv = vec2<f32>(-1.0, 0.0); }   // West
        default: {}
    }

    // Project onto direction axis and perpendicular
    let along = c.x * dv.x + c.y * dv.y;
    let perp = abs(c.x * dv.y - c.y * dv.x);

    // Arrow triangle: tip at along=0.3, base at along=-0.05, half-width 0.17 at base
    let arrow_tip = 0.3;
    let arrow_base = -0.05;
    let arrow_width = 0.17;
    let t = (arrow_tip - along) / (arrow_tip - arrow_base);
    let in_arrow = along > arrow_base && along < arrow_tip && perp < arrow_width * t;

    if in_arrow {
        color = mix(color, vec3<f32>(0.12, 0.12, 0.12), 0.65);
    }

    return vec4<f32>(color * fade, 1.0);
}
