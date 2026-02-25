// Instanced machine shader: positions a box on the tile surface per machine.
// Prepended by common.wgsl at load time.
//
// Box mesh convention (from build_box_mesh):
//   uv.y in [0,1]  → top face (lifted above tile)
//   uv.y = 2.0     → side wall top edge (lifted)
//   uv.y = 3.0     → side wall bottom edge (at tile surface)

struct Globals {
    view_proj: mat4x4<f32>,
    grid_params: vec4<f32>,  // (enabled, divisions, line_width, klein_half_side)
    color_cycle: f32,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct VertexInput {
    @location(0) local_pos: vec2<f32>,  // unit quad: -0.5 to 0.5
    @location(1) uv: vec2<f32>,         // 0-1 for top face, 2-3 for side walls
};

struct InstanceInput {
    @location(5) mobius_a: vec2<f32>,
    @location(6) mobius_b: vec2<f32>,
    @location(7) grid_pos: vec2<f32>,   // grid cell coords
    @location(8) machine_type: f32,     // 0-7 (see machine_size/machine_color)
    @location(9) progress: f32,         // 0.0-1.0 working, -1.0 idle, -2.0 no power
    @location(10) power_sat: f32,       // 0.0-1.0 satisfaction, -1.0 not connected
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) machine_type: f32,
    @location(2) progress: f32,
    @location(3) disk_r: f32,
    @location(4) world_normal: vec3<f32>,
    @location(5) power_sat: f32,
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

// Machine height above tile surface (taller machines = more prominent).
fn machine_height(mt: u32) -> f32 {
    switch mt {
        case 6u: { return 0.005; }  // Quadrupole: short relay
        case 5u: { return 0.008; }  // Source: medium
        default: { return 0.010; }  // All production machines: tall
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
    let height = machine_height(mt);

    // Scale unit quad to machine footprint (slightly inset)
    let inset = 0.92;
    let scaled = vert.local_pos * size * cell_size * inset;
    // Offset center so multi-cell machines span from origin to origin+(size-1)
    let footprint_offset = (size - 1.0) * 0.5;
    let center = (inst.grid_pos + footprint_offset) / divisions * 2.0 * khs;
    let klein = scaled + center;

    // Klein -> Poincare -> Mobius -> bowl
    let poincare = klein_to_poincare(klein);
    let disk = apply_mobius(poincare, inst.mobius_a, inst.mobius_b);
    var world = disk_to_bowl(disk);

    var normal: vec3<f32>;

    if vert.uv.y > 2.5 {
        // Side wall bottom edge: at tile surface (no lift)
        let outward = normalize(disk);
        normal = normalize(vec3<f32>(outward.x, 0.0, outward.y));
    } else if vert.uv.y > 1.5 {
        // Side wall top edge: lifted
        world.y += height;
        let outward = normalize(disk);
        normal = normalize(vec3<f32>(outward.x, 0.0, outward.y));
    } else {
        // Top face: lifted, compute surface normal via finite differences
        world.y += height;
        let eps = 0.002;
        let k_dx = klein + vec2<f32>(eps, 0.0);
        let k_dy = klein + vec2<f32>(0.0, eps);
        let p_dx = klein_to_poincare(k_dx);
        let p_dy = klein_to_poincare(k_dy);
        let d_dx = apply_mobius(p_dx, inst.mobius_a, inst.mobius_b);
        let d_dy = apply_mobius(p_dy, inst.mobius_a, inst.mobius_b);
        let w_dx = disk_to_bowl(d_dx);
        let w_dy = disk_to_bowl(d_dy);
        normal = normalize(cross(w_dx - world, w_dy - world));
    }

    out.clip_position = globals.view_proj * vec4<f32>(world, 1.0);
    out.uv = vert.uv;
    out.machine_type = inst.machine_type;
    out.progress = inst.progress;
    out.disk_r = length(disk);
    out.world_normal = normal;
    out.power_sat = inst.power_sat;

    return out;
}

@fragment
fn fs_machine(in: VertexOutput) -> @location(0) vec4<f32> {
    // Distance fade near disk boundary
    let fade = 1.0 - smoothstep(0.85, 0.95, in.disk_r);
    if fade < 0.01 { discard; }

    let mt = u32(in.machine_type + 0.5);
    let base_color = machine_color(mt);

    // Diffuse lighting
    let light_dir = normalize(vec3<f32>(0.3, 1.0, -0.2));
    let n = normalize(in.world_normal);
    let ndotl = max(dot(n, light_dir), 0.0);

    // --- Side wall rendering ---
    if in.uv.y > 1.5 {
        let wall_v = in.uv.y - 2.0;  // 0 at top, 1 at bottom
        let side_base = base_color * 0.5;
        let side_lit = side_base * (0.35 + 0.65 * ndotl);
        // Gradient: darker toward bottom
        let grad = 1.0 - wall_v * 0.4;
        var side_color = side_lit * grad;

        // State dimming for side walls too
        if in.progress >= 0.0 {
            let pulse = 0.8 + 0.2 * sin(in.progress * 6.2832);
            side_color *= pulse;
        } else if in.progress > -1.5 {
            side_color *= 0.65;
        } else {
            let grey = dot(side_color, vec3<f32>(0.299, 0.587, 0.114));
            side_color = mix(vec3<f32>(grey), side_color, 0.2) * 0.3;
        }

        return vec4<f32>(side_color * fade, 1.0);
    }

    // --- Top face rendering ---
    let ambient = 0.4;
    let diffuse = 0.6 * ndotl;
    let lighting = ambient + diffuse;

    // Edge bevel: darken at edges for 3D raised look
    let ex = smoothstep(0.0, 0.1, min(in.uv.x, 1.0 - in.uv.x));
    let ey = smoothstep(0.0, 0.1, min(in.uv.y, 1.0 - in.uv.y));
    let edge = ex * ey;

    var color = mix(vec3<f32>(0.04, 0.04, 0.04), base_color, edge);

    // Highlight top-left, shadow bottom-right (bevel)
    let highlight = (1.0 - in.uv.x) * (1.0 - in.uv.y);
    let shadow = in.uv.x * in.uv.y;
    color += vec3<f32>(0.15) * smoothstep(0.3, 0.8, highlight) * edge;
    color -= vec3<f32>(0.12) * smoothstep(0.3, 0.8, shadow) * edge;

    // Apply lighting
    color *= lighting;

    // State-based pulsing glow
    if in.progress >= 0.0 {
        let pulse = 0.8 + 0.2 * sin(in.progress * 6.2832);
        color *= pulse;
    } else if in.progress > -1.5 {
        color *= 0.65;
    } else {
        let grey = dot(color, vec3<f32>(0.299, 0.587, 0.114));
        color = mix(vec3<f32>(grey), color, 0.2) * 0.3;
    }

    // Power satisfaction pip in bottom-right corner
    if in.power_sat >= 0.0 {
        let pip_center = vec2<f32>(0.85, 0.85);
        let pip_dist = length(in.uv - pip_center);
        let pip_radius = 0.08;
        if pip_dist < pip_radius {
            var pip_color: vec3<f32>;
            if in.power_sat >= 1.0 {
                pip_color = vec3<f32>(0.2, 0.8, 0.2);  // green: full power
            } else if in.power_sat >= 0.5 {
                let t = (in.power_sat - 0.5) * 2.0;
                pip_color = mix(vec3<f32>(0.9, 0.7, 0.2), vec3<f32>(0.2, 0.8, 0.2), t);
            } else {
                let t = in.power_sat * 2.0;
                pip_color = mix(vec3<f32>(0.8, 0.2, 0.2), vec3<f32>(0.9, 0.7, 0.2), t);
            }
            let pip_edge = smoothstep(pip_radius, pip_radius * 0.5, pip_dist);
            color = mix(color, pip_color, pip_edge);
        }
    }

    return vec4<f32>(color * fade, 1.0);
}
