// Ghost preview shader: translucent placement preview.
// Prepended by common.wgsl at load time.
//
// Reuses MachineInstance layout and subdivided box mesh.
// Vertex shader identical to vs_machine; fragment shader simplified
// with alpha blending and gentle pulse animation.

struct Globals {
    view_proj: mat4x4<f32>,
    grid_params: vec4<f32>,
    color_cycle: f32,
    time: f32,
    bowl_height: f32,
    _pad: f32,
    camera_world: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct VertexInput {
    @location(0) local_pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct InstanceInput {
    @location(5) mobius_a: vec2<f32>,
    @location(6) mobius_b: vec2<f32>,
    @location(7) grid_pos: vec2<f32>,
    @location(8) machine_type: f32,
    @location(9) progress: f32,
    @location(10) power_sat: f32,
    @location(11) facing: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) machine_type: f32,
    @location(2) disk_r: f32,
    @location(3) world_normal: vec3<f32>,
    @location(4) @interpolate(flat) progress: f32,
};

// Machine footprint in grid cells: (width, height), canonical (facing North).
fn machine_size_canonical(mt: u32) -> vec2<f32> {
    switch mt {
        case 5u: { return vec2<f32>(1.0, 1.0); }  // Source
        case 6u: { return vec2<f32>(1.0, 1.0); }  // Quadrupole
        case 8u: { return vec2<f32>(1.0, 1.0); }  // Splitter
        case 10u: { return vec2<f32>(1.0, 1.0); }  // Belt ghost
        case 0u: { return vec2<f32>(2.0, 2.0); }  // Composer
        case 7u: { return vec2<f32>(2.0, 2.0); }  // Dynamo
        case 9u: { return vec2<f32>(2.0, 2.0); }  // Storage
        default: { return vec2<f32>(3.0, 3.0); }   // Inverter, Embedder, Quotient, Transformer
    }
}

fn machine_size(mt: u32, facing: u32) -> vec2<f32> {
    let s = machine_size_canonical(mt);
    if facing == 1u || facing == 3u {
        return vec2<f32>(s.y, s.x);
    }
    return s;
}

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
        case 8u: { return vec3<f32>(0.3, 0.8, 0.7); }   // Splitter: teal
        case 9u: { return vec3<f32>(0.8, 0.6, 0.3); }   // Storage: amber
        case 10u: { return vec3<f32>(0.55, 0.55, 0.57); } // Belt ghost: grey
        default: { return vec3<f32>(0.5, 0.5, 0.5); }
    }
}

fn machine_height(mt: u32) -> f32 {
    switch mt {
        case 10u: { return 0.005; }  // Belt ghost: low
        case 6u: { return 0.005; }   // Quadrupole: short relay
        case 5u: { return 0.008; }   // Source: medium
        case 8u: { return 0.008; }   // Splitter: medium
        default: { return 0.010; }   // All production machines: tall
    }
}

@vertex
fn vs_ghost(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;

    let divisions = globals.grid_params.y;
    let khs = globals.grid_params.w;
    let cell_size = 2.0 * khs / divisions;

    let mt = u32(inst.machine_type + 0.5);
    let facing = u32(inst.facing + 0.5);
    let size = machine_size(mt, facing);
    let height = machine_height(mt);

    let inset = 0.92;
    let scaled = vert.local_pos * size * cell_size * inset;
    let footprint_offset = (size - 1.0) * 0.5;
    let center = (inst.grid_pos + footprint_offset) / divisions * 2.0 * khs;
    let klein = scaled + center;

    let poincare = klein_to_poincare(klein);
    let disk = apply_mobius(poincare, inst.mobius_a, inst.mobius_b);
    var world = disk_to_bowl_h(disk, globals.bowl_height);

    var normal: vec3<f32>;

    if vert.uv.y > 2.5 {
        let outward = normalize(disk);
        normal = normalize(vec3<f32>(outward.x, 0.0, outward.y));
    } else if vert.uv.y > 1.5 {
        world.y += height;
        let outward = normalize(disk);
        normal = normalize(vec3<f32>(outward.x, 0.0, outward.y));
    } else {
        world.y += height;
        let eps = 0.002;
        let k_dx = klein + vec2<f32>(eps, 0.0);
        let k_dy = klein + vec2<f32>(0.0, eps);
        let p_dx = klein_to_poincare(k_dx);
        let p_dy = klein_to_poincare(k_dy);
        let d_dx = apply_mobius(p_dx, inst.mobius_a, inst.mobius_b);
        let d_dy = apply_mobius(p_dy, inst.mobius_a, inst.mobius_b);
        let w_dx = disk_to_bowl_h(d_dx, globals.bowl_height);
        let w_dy = disk_to_bowl_h(d_dy, globals.bowl_height);
        normal = normalize(cross(w_dx - world, w_dy - world));
    }

    out.clip_position = globals.view_proj * vec4<f32>(world, 1.0);
    out.uv = vert.uv;
    out.machine_type = inst.machine_type;
    out.disk_r = length(disk);
    out.world_normal = normal;
    out.progress = inst.progress;

    return out;
}

@fragment
fn fs_ghost(in: VertexOutput) -> @location(0) vec4<f32> {
    // Distance fade near disk boundary
    let fade = 1.0 - smoothstep(0.85, 0.95, in.disk_r);
    if fade < 0.01 { discard; }

    let mt = u32(in.machine_type + 0.5);
    // Blocked ghost sentinel: progress <= -2.5 → red tint
    let blocked = in.progress < -2.5;
    var base_color = machine_color(mt);
    if blocked {
        base_color = vec3<f32>(0.85, 0.15, 0.15);
    }

    // Diffuse lighting
    let light_dir = normalize(vec3<f32>(0.3, 1.0, -0.2));
    let n = normalize(in.world_normal);
    let ndotl = max(dot(n, light_dir), 0.0);

    // Side wall rendering
    if in.uv.y > 1.5 {
        let wall_v = in.uv.y - 2.0;
        let side_base = base_color * 0.5;
        let side_lit = side_base * (0.35 + 0.65 * ndotl);
        let grad = 1.0 - wall_v * 0.4;
        let side_color = side_lit * grad;
        let pulse = 0.85 + 0.15 * sin(globals.time * 3.0);
        return vec4<f32>(side_color * fade * pulse, 0.4);
    }

    // Top face
    let ambient = 0.4;
    let diffuse = 0.6 * ndotl;
    let lighting = ambient + diffuse;

    // Edge bevel
    let ex = smoothstep(0.0, 0.1, min(in.uv.x, 1.0 - in.uv.x));
    let ey = smoothstep(0.0, 0.1, min(in.uv.y, 1.0 - in.uv.y));
    let edge = ex * ey;

    var color = mix(vec3<f32>(0.04, 0.04, 0.04), base_color, edge);

    // Bevel highlight/shadow
    let highlight = (1.0 - in.uv.x) * (1.0 - in.uv.y);
    let shadow = in.uv.x * in.uv.y;
    color += vec3<f32>(0.15) * smoothstep(0.3, 0.8, highlight) * edge;
    color -= vec3<f32>(0.12) * smoothstep(0.3, 0.8, shadow) * edge;

    color *= lighting;

    // Gentle pulse animation
    let pulse = 0.85 + 0.15 * sin(globals.time * 3.0);
    color *= pulse;

    return vec4<f32>(color * fade, 0.4);
}
