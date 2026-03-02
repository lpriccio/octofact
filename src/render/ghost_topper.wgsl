// Ghost topper shader: translucent ray-marched 3D shapes for placement preview.
// Prepended by common.wgsl at load time.
//
// Same ray-marching as topper.wgsl but with alpha output and pulse animation.

struct Globals {
    view_proj: mat4x4<f32>,
    grid_params: vec4<f32>,
    color_cycle: f32,
    time: f32,
    _pad: vec2<f32>,
    camera_world: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct VertexInput {
    @location(0) cube_pos: vec3<f32>,
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
    @location(0) world_pos: vec3<f32>,
    @location(1) box_pos: vec3<f32>,
    @location(2) @interpolate(flat) machine_type: f32,
    @location(3) @interpolate(flat) progress: f32,
    @location(4) disk_r: f32,
    @location(5) @interpolate(flat) power_sat: f32,
    @location(6) @interpolate(flat) facing: f32,
};

fn topper_machine_size_canonical(mt: u32) -> vec2<f32> {
    switch mt {
        case 5u: { return vec2<f32>(1.0, 1.0); }
        case 6u: { return vec2<f32>(1.0, 1.0); }
        case 8u: { return vec2<f32>(1.0, 1.0); }
        case 10u: { return vec2<f32>(1.0, 1.0); }  // Belt ghost
        case 0u: { return vec2<f32>(2.0, 2.0); }
        case 7u: { return vec2<f32>(2.0, 2.0); }
        case 9u: { return vec2<f32>(2.0, 2.0); }
        default: { return vec2<f32>(3.0, 3.0); }
    }
}

fn topper_machine_size(mt: u32, facing: u32) -> vec2<f32> {
    let s = topper_machine_size_canonical(mt);
    if facing == 1u || facing == 3u {
        return vec2<f32>(s.y, s.x);
    }
    return s;
}

fn topper_base_height(mt: u32) -> f32 {
    switch mt {
        case 10u: { return 0.005; }
        case 6u: { return 0.005; }
        case 5u: { return 0.008; }
        case 8u: { return 0.008; }
        default: { return 0.010; }
    }
}

fn topper_height(mt: u32) -> f32 {
    switch mt {
        case 10u: { return 0.008; }  // Belt ghost: small
        case 5u: { return 0.012; }
        case 6u: { return 0.012; }
        case 8u: { return 0.012; }
        case 0u: { return 0.015; }
        case 7u: { return 0.015; }
        case 9u: { return 0.015; }
        default: { return 0.018; }
    }
}

fn topper_fresnel_params(mt: u32) -> vec2<f32> {
    switch mt {
        case 0u: { return vec2<f32>(0.20, 2.5); }
        case 1u: { return vec2<f32>(0.10, 4.0); }
        case 2u: { return vec2<f32>(0.18, 3.0); }
        case 3u: { return vec2<f32>(0.12, 3.5); }
        case 4u: { return vec2<f32>(0.22, 2.5); }
        case 5u: { return vec2<f32>(0.25, 2.0); }
        case 6u: { return vec2<f32>(0.15, 3.0); }
        case 7u: { return vec2<f32>(0.18, 3.0); }
        case 8u: { return vec2<f32>(0.08, 4.0); }
        case 9u: { return vec2<f32>(0.10, 4.0); }
        case 10u: { return vec2<f32>(0.15, 3.0); }
        default: { return vec2<f32>(0.15, 3.0); }
    }
}

fn topper_color(mt: u32) -> vec3<f32> {
    switch mt {
        case 0u: { return vec3<f32>(0.4, 0.6, 0.8); }
        case 1u: { return vec3<f32>(0.8, 0.4, 0.4); }
        case 2u: { return vec3<f32>(0.5, 0.8, 0.5); }
        case 3u: { return vec3<f32>(0.7, 0.5, 0.3); }
        case 4u: { return vec3<f32>(0.6, 0.3, 0.8); }
        case 5u: { return vec3<f32>(0.4, 0.9, 0.3); }
        case 6u: { return vec3<f32>(0.9, 0.8, 0.2); }
        case 7u: { return vec3<f32>(1.0, 0.9, 0.3); }
        case 8u: { return vec3<f32>(0.3, 0.8, 0.7); }
        case 9u: { return vec3<f32>(0.8, 0.6, 0.3); }
        case 10u: { return vec3<f32>(0.55, 0.55, 0.57); }
        default: { return vec3<f32>(0.5, 0.5, 0.5); }
    }
}

@vertex
fn vs_ghost_topper(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;

    let divisions = globals.grid_params.y;
    let khs = globals.grid_params.w;
    let cell_size = 2.0 * khs / divisions;

    let mt = u32(inst.machine_type + 0.5);
    let facing = u32(inst.facing + 0.5);
    let size = topper_machine_size(mt, facing);
    let base_h = topper_base_height(mt);
    let top_h = topper_height(mt);

    let inset = 0.92;
    let xz = vert.cube_pos.xz * size * cell_size * inset;
    let footprint_offset = (size - 1.0) * 0.5;
    let center = (inst.grid_pos + footprint_offset) / divisions * 2.0 * khs;
    let klein = xz + center;

    let poincare = klein_to_poincare(klein);
    let disk = apply_mobius(poincare, inst.mobius_a, inst.mobius_b);
    var world = disk_to_bowl(disk);

    let y_frac = vert.cube_pos.y + 0.5;
    world.y += base_h + y_frac * top_h;

    out.clip_position = globals.view_proj * vec4<f32>(world, 1.0);
    out.world_pos = world;
    out.box_pos = vert.cube_pos + vec3<f32>(0.5);
    out.machine_type = inst.machine_type;
    out.progress = inst.progress;
    out.disk_r = length(disk);
    out.power_sat = inst.power_sat;
    out.facing = inst.facing;

    return out;
}

// --- Ray marching (same as topper.wgsl) ---

const MAX_STEPS: i32 = 48;
const MIN_DIST: f32 = 0.002;
const MAX_DIST: f32 = 2.0;

fn machine_scene(p: vec3<f32>, mt: u32, t: f32, progress: f32, facing: f32) -> f32 {
    let fa = facing * 1.5707963;
    let rp = rot3_y(fa) * p;

    switch mt {
        case 5u: {
            let r = 0.25 + 0.03 * sin(t * 3.0);
            return sdf3_sphere(rp, r);
        }
        case 0u: {
            let p1 = rot3_y(t * 0.5) * (rp - vec3<f32>(0.0, 0.1, 0.0));
            let p2 = rot3_y(-t * 0.5) * (rp + vec3<f32>(0.0, 0.1, 0.0));
            let d1 = sdf3_torus(p1, 0.2, 0.05);
            let d2 = sdf3_torus(p2, 0.2, 0.05);
            return sdf_smooth_union(d1, d2, 0.05);
        }
        case 1u: {
            let tp = rot3_x(t * 0.3) * rot3_y(t * 0.5) * rp;
            return sdf3_octahedron(tp, 0.25);
        }
        case 2u: {
            let hp = rot3_y(t * 0.7) * rp;
            var d = 1e10;
            for (var i = 0; i < 6; i += 1) {
                let angle = f32(i) * 1.0472;
                let y0 = -0.3 + f32(i) * 0.1;
                let y1 = y0 + 0.1;
                let r_helix = 0.15;
                let a0 = vec3<f32>(cos(angle) * r_helix, y0, sin(angle) * r_helix);
                let a1 = vec3<f32>(cos(angle + 0.5) * r_helix, y1, sin(angle + 0.5) * r_helix);
                d = min(d, sdf3_capsule(hp, a0, a1, 0.04));
            }
            return d;
        }
        case 3u: {
            let p1 = rot3_y(t * 0.2) * rp;
            let p2 = rot3_y(-t * 0.2) * rp;
            let d1 = sdf3_octahedron(p1, 0.22);
            let d2 = sdf3_octahedron(p2 * vec3<f32>(1.0, -1.0, 1.0), 0.22);
            return sdf_smooth_union(d1, d2, 0.02);
        }
        case 4u: {
            let twist_k = sin(t) * 2.0;
            let tp = sdf3_twist_y(rp, twist_k);
            return sdf3_torus(tp, 0.22, 0.06);
        }
        case 6u: {
            return sdf3_octahedron(rp * vec3<f32>(1.0, 0.6, 1.0), 0.2);
        }
        case 7u: {
            let dp = rot3_y(t * 2.0) * rp;
            var d = sdf3_cylinder(dp, 0.15, 0.12);
            for (var i = 0; i < 4; i += 1) {
                let angle = f32(i) * 1.5707963;
                let fp = rot3_y(angle) * dp;
                let fin = sdf3_box(fp - vec3<f32>(0.15, 0.0, 0.0), vec3<f32>(0.08, 0.12, 0.015));
                d = sdf_smooth_union(d, fin, 0.02);
            }
            return d;
        }
        case 8u: {
            let a1 = rp.x * 0.866 + rp.z * 0.5;
            let a2 = -rp.x * 0.866 + rp.z * 0.5;
            let a3 = -rp.z;
            let d_prism = max(max(a1, a2), a3) - 0.2;
            let d_height = abs(rp.y) - 0.2;
            return max(d_prism, d_height);
        }
        case 9u: {
            // Storage ghost: show single cube (no fill level)
            return sdf3_round_box(rp, vec3<f32>(0.15, 0.06, 0.15), 0.02);
        }
        case 10u: {
            // Belt ghost: small sphere
            return sdf3_sphere(rp, 0.2);
        }
        default: {
            return sdf3_sphere(rp, 0.2);
        }
    }
}

fn march_machine(ro: vec3<f32>, rd: vec3<f32>, mt: u32, t: f32, progress: f32, facing: f32) -> f32 {
    var dist = 0.0;
    for (var i: i32 = 0; i < MAX_STEPS; i += 1) {
        let p = ro + rd * dist;
        let d = machine_scene(p, mt, t, progress, facing);
        if d < MIN_DIST {
            return dist;
        }
        dist += d;
        if dist > MAX_DIST {
            return -1.0;
        }
    }
    return -1.0;
}

fn machine_normal(p: vec3<f32>, mt: u32, t: f32, progress: f32, facing: f32) -> vec3<f32> {
    let eps = 0.001;
    let ex = vec3<f32>(eps, 0.0, 0.0);
    let ey = vec3<f32>(0.0, eps, 0.0);
    let ez = vec3<f32>(0.0, 0.0, eps);
    let d = machine_scene(p, mt, t, progress, facing);
    return normalize(vec3<f32>(
        machine_scene(p + ex, mt, t, progress, facing) - d,
        machine_scene(p + ey, mt, t, progress, facing) - d,
        machine_scene(p + ez, mt, t, progress, facing) - d,
    ));
}

@fragment
fn fs_ghost_topper(in: VertexOutput) -> @location(0) vec4<f32> {
    let fade = 1.0 - smoothstep(0.85, 0.95, in.disk_r);
    if fade < 0.01 { discard; }

    let mt = u32(in.machine_type + 0.5);

    // Ghost toppers always animate (they're previews, not idle)
    let anim_t = globals.time;
    let facing = in.facing;

    let entry = in.box_pos - vec3<f32>(0.5);
    let ray_dir = normalize(in.world_pos - globals.camera_world.xyz);
    let local_rd = normalize(ray_dir);

    let hit_dist = march_machine(entry, local_rd, mt, anim_t, in.progress, facing);
    if hit_dist < 0.0 {
        discard;
    }

    let hit_pos = entry + local_rd * hit_dist;
    let normal = machine_normal(hit_pos, mt, anim_t, in.progress, facing);

    let light_dir = normalize(vec3<f32>(0.3, 1.0, -0.2));
    let ndotl = max(dot(normal, light_dir), 0.0);
    let ambient = 0.35;
    let diffuse = 0.65 * ndotl;

    let view_dir = -local_rd;
    let fparams = topper_fresnel_params(mt);
    let fresnel = pow(1.0 - abs(dot(normal, view_dir)), fparams.y);

    let base_color = topper_color(mt) * 1.3;
    var color = base_color * (ambient + diffuse) + vec3<f32>(fresnel * fparams.x);

    // Ghost pulse
    let pulse = 0.85 + 0.15 * sin(globals.time * 3.0);
    color *= pulse;

    return vec4<f32>(color * fade, 0.4);
}
