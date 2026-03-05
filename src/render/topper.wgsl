// Instanced topper shader: ray-marched 3D shapes above machine bases.
// Prepended by common.wgsl at load time.
//
// Vertex mesh: subdivided cube [-0.5, 0.5]³ bounding volume.
// Fragment shader ray marches per-machine-type 3D SDFs.

struct Globals {
    view_proj: mat4x4<f32>,
    grid_params: vec4<f32>,  // (enabled, divisions, line_width, klein_half_side)
    color_cycle: f32,
    time: f32,               // elapsed seconds since startup
    embed_param: f32,
    embed_type: f32,
    camera_world: vec4<f32>, // .xyz = camera eye position in bowl space
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct VertexInput {
    @location(0) cube_pos: vec3<f32>,  // [-0.5, 0.5]³
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
    @location(1) box_pos: vec3<f32>,          // [0,1]³ within bounding cube
    @location(2) @interpolate(flat) machine_type: f32,
    @location(3) @interpolate(flat) progress: f32,
    @location(4) disk_r: f32,
    @location(5) @interpolate(flat) power_sat: f32,
    @location(6) @interpolate(flat) facing: f32,
};

// Machine footprint in grid cells (canonical, facing North).
fn topper_machine_size_canonical(mt: u32) -> vec2<f32> {
    switch mt {
        case 5u: { return vec2<f32>(1.0, 1.0); }  // Source
        case 6u: { return vec2<f32>(1.0, 1.0); }  // Quadrupole
        case 8u: { return vec2<f32>(1.0, 1.0); }  // Splitter
        case 0u: { return vec2<f32>(2.0, 2.0); }  // Composer
        case 7u: { return vec2<f32>(2.0, 2.0); }  // Dynamo
        case 9u: { return vec2<f32>(2.0, 2.0); }  // Storage
        default: { return vec2<f32>(3.0, 3.0); }   // 3×3 machines
    }
}

fn topper_machine_size(mt: u32, facing: u32) -> vec2<f32> {
    let s = topper_machine_size_canonical(mt);
    if facing == 1u || facing == 3u {
        return vec2<f32>(s.y, s.x);
    }
    return s;
}

// Machine base height (must match machine.wgsl machine_height).
fn topper_base_height(mt: u32) -> f32 {
    switch mt {
        case 6u: { return 0.005; }
        case 5u: { return 0.008; }
        case 8u: { return 0.008; }
        default: { return 0.010; }
    }
}

// Topper bounding box height above the machine base.
fn topper_height(mt: u32) -> f32 {
    switch mt {
        case 5u: { return 0.012; }  // Source (1×1)
        case 6u: { return 0.012; }  // Quadrupole (1×1)
        case 8u: { return 0.012; }  // Splitter (1×1)
        case 0u: { return 0.015; }  // Composer (2×2)
        case 7u: { return 0.015; }  // Dynamo (2×2)
        case 9u: { return 0.015; }  // Storage (2×2)
        default: { return 0.018; }  // 3×3 machines
    }
}

// Per-machine-type fresnel rim glow parameters.
// Returns vec2(intensity, power_exponent).
// Round shapes get stronger, wider glow; angular shapes get subtle, tight glow.
fn topper_fresnel_params(mt: u32) -> vec2<f32> {
    switch mt {
        case 0u: { return vec2<f32>(0.20, 2.5); }  // Composer: round tori — strong, wide
        case 1u: { return vec2<f32>(0.10, 4.0); }  // Inverter: angular octahedron — subtle
        case 2u: { return vec2<f32>(0.18, 3.0); }  // Embedder: mixed curves — moderate
        case 3u: { return vec2<f32>(0.12, 3.5); }  // Quotient: pointy stella — restrained
        case 4u: { return vec2<f32>(0.22, 2.5); }  // Transformer: round twist — strong
        case 5u: { return vec2<f32>(0.25, 2.0); }  // Source: smooth sphere — strongest
        case 6u: { return vec2<f32>(0.15, 3.0); }  // Quadrupole: diamond — standard
        case 7u: { return vec2<f32>(0.18, 3.0); }  // Dynamo: cylinder+fins — moderate
        case 8u: { return vec2<f32>(0.08, 4.0); }  // Splitter: angular prism — subtle
        case 9u: { return vec2<f32>(0.10, 4.0); }  // Storage: boxy cubes — subtle
        default: { return vec2<f32>(0.15, 3.0); }
    }
}

// Machine type color (matching machine.wgsl).
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
        default: { return vec3<f32>(0.5, 0.5, 0.5); }
    }
}

@vertex
fn vs_topper(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;

    let divisions = globals.grid_params.y;  // 64.0
    let khs = globals.grid_params.w;        // klein_half_side
    let cell_size = 2.0 * khs / divisions;

    let mt = u32(inst.machine_type + 0.5);
    let facing = u32(inst.facing + 0.5);
    let size = topper_machine_size(mt, facing);
    let base_h = topper_base_height(mt);
    let top_h = topper_height(mt);

    // Map XZ: same as machine vertex shader
    let inset = 0.92;
    let xz = vert.cube_pos.xz * size * cell_size * inset;
    let footprint_offset = (size - 1.0) * 0.5;
    let center = (inst.grid_pos + footprint_offset) / divisions * 2.0 * khs;
    let klein = xz + center;

    // Klein -> Poincare -> Mobius -> bowl
    let poincare = klein_to_poincare(klein);
    let disk = apply_mobius(poincare, inst.mobius_a, inst.mobius_b);
    var world = disk_embed(disk, globals.embed_type, globals.embed_param);

    // Map Y: bottom of cube sits on machine base, top extends upward
    let y_frac = vert.cube_pos.y + 0.5;  // [0, 1]
    world.y += base_h + y_frac * top_h;

    out.clip_position = globals.view_proj * vec4<f32>(world, 1.0);
    out.world_pos = world;
    out.box_pos = vert.cube_pos + vec3<f32>(0.5);  // [0,1]³
    out.machine_type = inst.machine_type;
    out.progress = inst.progress;
    out.disk_r = length(disk);
    out.power_sat = inst.power_sat;
    out.facing = inst.facing;

    return out;
}

// --- Ray marching ---

const MAX_STEPS: i32 = 48;
const MIN_DIST: f32 = 0.002;
const MAX_DIST: f32 = 2.0;

// Per-machine-type scene SDF. p is in normalized [-0.5, 0.5]³ local space.
fn machine_scene(p: vec3<f32>, mt: u32, t: f32, progress: f32, facing: f32) -> f32 {
    // Apply facing rotation so shapes orient with the machine
    let fa = facing * 1.5707963;  // facing * PI/2
    let rp = rot3_y(fa) * p;

    switch mt {
        case 5u: {
            // Source: pulsing sphere
            let r = 0.25 + 0.03 * sin(t * 3.0);
            return sdf3_sphere(rp, r);
        }
        case 0u: {
            // Composer: two counter-rotating tori
            let p1 = rot3_y(t * 0.5) * (rp - vec3<f32>(0.0, 0.1, 0.0));
            let p2 = rot3_y(-t * 0.5) * (rp + vec3<f32>(0.0, 0.1, 0.0));
            let d1 = sdf3_torus(p1, 0.2, 0.05);
            let d2 = sdf3_torus(p2, 0.2, 0.05);
            return sdf_smooth_union(d1, d2, 0.05);
        }
        case 1u: {
            // Inverter: tumbling octahedron
            let tp = rot3_x(t * 0.3) * rot3_y(t * 0.5) * rp;
            return sdf3_octahedron(tp, 0.25);
        }
        case 2u: {
            // Embedder: rotating helix (spiral of capsules)
            let hp = rot3_y(t * 0.7) * rp;
            var d = 1e10;
            for (var i = 0; i < 6; i += 1) {
                let angle = f32(i) * 1.0472;  // 2*PI/6
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
            // Quotient: stella octangula (two interpenetrating tetrahedra via octahedra)
            let p1 = rot3_y(t * 0.2) * rp;
            let p2 = rot3_y(-t * 0.2) * rp;
            let d1 = sdf3_octahedron(p1, 0.22);
            let d2 = sdf3_octahedron(p2 * vec3<f32>(1.0, -1.0, 1.0), 0.22);
            return sdf_smooth_union(d1, d2, 0.02);
        }
        case 4u: {
            // Transformer: twisted torus
            let twist_k = sin(t) * 2.0;
            let tp = sdf3_twist_y(rp, twist_k);
            return sdf3_torus(tp, 0.22, 0.06);
        }
        case 6u: {
            // Quadrupole: elongated diamond (stretched octahedron)
            var pulse = 1.0;
            if progress >= 0.0 {
                pulse = 1.0 + 0.05 * sin(t * 4.0);
            }
            return sdf3_octahedron(rp * vec3<f32>(1.0, 0.6, 1.0) / pulse, 0.2);
        }
        case 7u: {
            // Dynamo: cylinder with 4 radial fins, spinning
            let dp = rot3_y(t * 2.0) * rp;
            var d = sdf3_cylinder(dp, 0.15, 0.12);
            // 4 fins
            for (var i = 0; i < 4; i += 1) {
                let angle = f32(i) * 1.5707963;
                let fp = rot3_y(angle) * dp;
                let fin = sdf3_box(fp - vec3<f32>(0.15, 0.0, 0.0), vec3<f32>(0.08, 0.12, 0.015));
                d = sdf_smooth_union(d, fin, 0.02);
            }
            return d;
        }
        case 8u: {
            // Splitter: triangular prism
            // Intersection of 3 half-spaces
            let a1 = rp.x * 0.866 + rp.z * 0.5;
            let a2 = -rp.x * 0.866 + rp.z * 0.5;
            let a3 = -rp.z;
            let d_prism = max(max(a1, a2), a3) - 0.2;
            let d_height = abs(rp.y) - 0.2;
            return max(d_prism, d_height);
        }
        case 9u: {
            // Storage: stacked cubes based on fill level
            let fill = clamp(progress, 0.0, 1.0);
            let n_cubes = 1u + u32(fill * 2.0);
            var d = 1e10;
            for (var i = 0u; i < n_cubes; i += 1u) {
                let y_off = -0.2 + f32(i) * 0.18;
                let cube_size = 0.15 - f32(i) * 0.02;
                d = min(d, sdf3_round_box(rp - vec3<f32>(0.0, y_off, 0.0), vec3<f32>(cube_size, 0.06, cube_size), 0.02));
            }
            return d;
        }
        default: {
            return sdf3_sphere(rp, 0.2);
        }
    }
}

// Ray march through the scene.
// ro: ray origin in [-0.5, 0.5]³ local space
// rd: ray direction (normalized in local space)
// Returns hit distance or -1.0 for miss.
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

// Compute normal via central differences.
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
fn fs_topper(in: VertexOutput) -> @location(0) vec4<f32> {
    // Distance fade near disk boundary
    let fade = 1.0 - smoothstep(0.85, 0.95, in.disk_r);
    if fade < 0.01 { discard; }

    let mt = u32(in.machine_type + 0.5);

    // Animation time: working machines animate, idle/unpowered freeze
    var anim_t = 0.0;
    if in.progress >= 0.0 {
        anim_t = globals.time;
    }

    let facing = in.facing;

    // Ray setup: entry point is the interpolated box position
    let entry = in.box_pos - vec3<f32>(0.5);  // back to [-0.5, 0.5]³

    // Ray direction: from camera to this world point
    let ray_dir = normalize(in.world_pos - globals.camera_world.xyz);

    // Transform ray direction into local box space
    // The box occupies world space with some scale — we need to approximate
    // the local-space ray direction. Since the box is small and roughly uniform,
    // using the world-space direction is a reasonable approximation.
    let local_rd = normalize(ray_dir);

    // March
    let hit_dist = march_machine(entry, local_rd, mt, anim_t, in.progress, facing);
    if hit_dist < 0.0 {
        discard;
    }

    let hit_pos = entry + local_rd * hit_dist;
    let normal = machine_normal(hit_pos, mt, anim_t, in.progress, facing);

    // Lighting (same light direction as machine shader)
    let light_dir = normalize(vec3<f32>(0.3, 1.0, -0.2));
    let ndotl = max(dot(normal, light_dir), 0.0);
    let ambient = 0.35;
    let diffuse = 0.65 * ndotl;

    // Fresnel rim glow (per-machine-type intensity and falloff)
    let view_dir = -local_rd;
    let fparams = topper_fresnel_params(mt);
    let fresnel = pow(1.0 - abs(dot(normal, view_dir)), fparams.y);

    // Base color (brighter than machine base)
    var base_color = topper_color(mt) * 1.3;

    // State-based effects
    if in.progress < -1.5 {
        // No power: desaturate + dim
        let grey = dot(base_color, vec3<f32>(0.299, 0.587, 0.114));
        base_color = mix(vec3<f32>(grey), base_color, 0.2) * 0.4;
    } else if in.progress < 0.0 && in.progress > -1.5 {
        // Idle: dim slightly
        base_color *= 0.7;
    }

    let color = base_color * (ambient + diffuse) + vec3<f32>(fresnel * fparams.x);

    return vec4<f32>(color * fade, 1.0);
}
