// Instanced machine shader: positions a scaled quad on the tile surface per machine.
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
    @location(7) grid_pos: vec2<f32>,   // grid cell coords
    @location(8) machine_type: f32,     // 0-7 (see machine_size/machine_color)
    @location(9) progress: f32,         // 0.0-1.0 working, -1.0 idle, -2.0 no power
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) machine_type: f32,
    @location(2) progress: f32,
    @location(3) disk_r: f32,
};

// Machine footprint in grid cells: (width, height).
fn machine_size(mt: u32) -> vec2<f32> {
    switch mt {
        case 5u: { return vec2<f32>(1.0, 1.0); }  // Source
        case 6u: { return vec2<f32>(1.0, 1.0); }  // Quadrupole
        case 0u: { return vec2<f32>(2.0, 2.0); }  // Composer
        case 7u: { return vec2<f32>(2.0, 2.0); }  // Dynamo
        default: { return vec2<f32>(3.0, 2.0); }   // Inverter, Embedder, Quotient, Transformer
    }
}

// Machine type color (matching icon_params in items.rs).
fn machine_color(mt: u32) -> vec3<f32> {
    switch mt {
        case 0u: { return vec3<f32>(0.4, 0.6, 0.8); }   // Composer: blue
        case 1u: { return vec3<f32>(0.8, 0.4, 0.4); }   // Inverter: red
        case 2u: { return vec3<f32>(0.5, 0.8, 0.5); }   // Embedder: green
        case 3u: { return vec3<f32>(0.7, 0.5, 0.3); }   // Quotient: brown
        case 4u: { return vec3<f32>(0.6, 0.3, 0.8); }   // Transformer: purple
        case 5u: { return vec3<f32>(0.4, 0.9, 0.3); }   // Source: lime
        case 6u: { return vec3<f32>(0.9, 0.8, 0.2); }   // Quadrupole: gold
        case 7u: { return vec3<f32>(1.0, 0.9, 0.3); }   // Dynamo: bright gold
        default: { return vec3<f32>(0.5, 0.5, 0.5); }
    }
}

@vertex
fn vs_machine(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;

    let divisions = globals.grid_params.y;  // 64.0
    let khs = globals.grid_params.w;        // klein_half_side
    let cell_size = 2.0 * khs / divisions;

    let mt = u32(inst.machine_type + 0.5);
    let size = machine_size(mt);

    // Scale unit quad to machine footprint (slightly inset)
    let inset = 0.92;
    let scaled = vert.local_pos * size * cell_size * inset;
    let center = inst.grid_pos / divisions * 2.0 * khs;
    let klein = scaled + center;

    // Klein -> Poincare -> Mobius -> bowl -> clip
    let poincare = klein_to_poincare(klein);
    let disk = apply_mobius(poincare, inst.mobius_a, inst.mobius_b);
    var world = disk_to_bowl(disk);
    world.y += 0.003;  // lift above tile + belt surface

    out.clip_position = globals.view_proj * vec4<f32>(world, 1.0);
    out.uv = vert.uv;
    out.machine_type = inst.machine_type;
    out.progress = inst.progress;
    out.disk_r = length(disk);

    return out;
}

@fragment
fn fs_machine(in: VertexOutput) -> @location(0) vec4<f32> {
    // Distance fade near disk boundary
    let fade = 1.0 - smoothstep(0.85, 0.95, in.disk_r);
    if fade < 0.01 { discard; }

    let mt = u32(in.machine_type + 0.5);
    let base_color = machine_color(mt);

    // Edge bevel: darken at edges for 3D raised look
    let ex = smoothstep(0.0, 0.08, min(in.uv.x, 1.0 - in.uv.x));
    let ey = smoothstep(0.0, 0.08, min(in.uv.y, 1.0 - in.uv.y));
    let edge = ex * ey;

    var color = mix(vec3<f32>(0.05, 0.05, 0.05), base_color, edge);

    // Highlight top-left, shadow bottom-right (bevel)
    let highlight = (1.0 - in.uv.x) * (1.0 - in.uv.y);
    let shadow = in.uv.x * in.uv.y;
    color += vec3<f32>(0.1) * smoothstep(0.3, 0.8, highlight) * edge;
    color -= vec3<f32>(0.08) * smoothstep(0.3, 0.8, shadow) * edge;

    // State-based pulsing glow
    if in.progress >= 0.0 {
        // Working: pulse glow — one full sine cycle per craft
        let pulse = 0.75 + 0.25 * sin(in.progress * 6.2832);
        color *= pulse;
    } else if in.progress > -1.5 {
        // Idle (progress ~ -1.0): static dim
        color *= 0.45;
    } else {
        // No power (progress ~ -2.0): dark and desaturated
        let grey = dot(color, vec3<f32>(0.299, 0.587, 0.114));
        color = mix(vec3<f32>(grey), color, 0.2) * 0.25;
    }

    return vec4<f32>(color * fade, 1.0);
}
