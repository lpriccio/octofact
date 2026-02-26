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
    @location(11) facing: f32,          // 0=North, 1=East, 2=South, 3=West
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) machine_type: f32,
    @location(2) progress: f32,
    @location(3) disk_r: f32,
    @location(4) world_normal: vec3<f32>,
    @location(5) power_sat: f32,
    @location(6) facing: f32,
};

// Machine footprint in grid cells: (width, height), canonical (facing North).
fn machine_size_canonical(mt: u32) -> vec2<f32> {
    switch mt {
        case 5u: { return vec2<f32>(1.0, 1.0); }  // Source
        case 6u: { return vec2<f32>(1.0, 1.0); }  // Quadrupole
        case 0u: { return vec2<f32>(2.0, 2.0); }  // Composer
        case 7u: { return vec2<f32>(2.0, 2.0); }  // Dynamo
        default: { return vec2<f32>(3.0, 2.0); }   // Inverter, Embedder, Quotient, Transformer
    }
}

// Machine footprint rotated by facing direction.
// 90° and 270° swap width and height.
fn machine_size(mt: u32, facing: u32) -> vec2<f32> {
    let s = machine_size_canonical(mt);
    if facing == 1u || facing == 3u {
        return vec2<f32>(s.y, s.x);
    }
    return s;
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

// --- Port indicator helpers ---
// Side encoding: 0=North, 1=East, 2=South, 3=West
// Kind: 0=input, 1=output

// Compute UV position of a port given its cell offset, rotated side, and machine size.
fn port_uv_pos(cell: vec2<f32>, rot_side: u32, size: vec2<f32>) -> vec2<f32> {
    let cx = (cell.x + 0.5) / size.x;
    let cy = (cell.y + 0.5) / size.y;
    switch rot_side {
        case 0u: { return vec2<f32>(cx, cell.y / size.y); }               // North: top of cell
        case 1u: { return vec2<f32>((cell.x + 1.0) / size.x, cy); }      // East: right of cell
        case 2u: { return vec2<f32>(cx, (cell.y + 1.0) / size.y); }      // South: bottom of cell
        default: { return vec2<f32>(cell.x / size.x, cy); }               // West: left of cell
    }
}

// Rotate a canonical cell offset by facing direction within canonical footprint (w, h).
fn rotate_cell(cell: vec2<f32>, facing: u32, canon_size: vec2<f32>) -> vec2<f32> {
    let w = canon_size.x;
    let h = canon_size.y;
    switch facing {
        case 1u: { return vec2<f32>(h - 1.0 - cell.y, cell.x); }          // East: 90° CW
        case 2u: { return vec2<f32>(w - 1.0 - cell.x, h - 1.0 - cell.y); } // South: 180°
        case 3u: { return vec2<f32>(cell.y, w - 1.0 - cell.x); }          // West: 270° CW
        default: { return cell; }                                            // North: identity
    }
}

// Check if UV is near a port and return (color, alpha).
// canon_side: canonical direction (0=N,1=E,2=S,3=W), facing: rotation steps CW from North
// cell: canonical cell offset (will be rotated internally)
// canon_size: canonical footprint (w, h) before rotation
fn check_port(uv: vec2<f32>, canon_size: vec2<f32>, cell: vec2<f32>, canon_side: u32, facing: u32, kind: u32) -> vec4<f32> {
    let rot_side = (canon_side + facing) % 4u;
    let rot_cell = rotate_cell(cell, facing, canon_size);
    // Rotated footprint size: swap w/h for 90° and 270°
    var size = canon_size;
    if facing == 1u || facing == 3u {
        size = vec2<f32>(canon_size.y, canon_size.x);
    }
    let pos = port_uv_pos(rot_cell, rot_side, size);
    // Scale delta by size so circles are round regardless of aspect ratio
    let delta = (uv - pos) * size;
    let dist = length(delta);
    let radius = 0.22;
    if dist > radius { return vec4<f32>(0.0); }

    let alpha = smoothstep(radius, radius * 0.4, dist);
    // Draw a triangle/arrow pointing in the port direction
    // Normalized direction from port center outward
    var arrow_dir: vec2<f32>;
    switch rot_side {
        case 0u: { arrow_dir = vec2<f32>(0.0, -1.0); }  // North: up
        case 1u: { arrow_dir = vec2<f32>(1.0, 0.0); }   // East: right
        case 2u: { arrow_dir = vec2<f32>(0.0, 1.0); }   // South: down
        default: { arrow_dir = vec2<f32>(-1.0, 0.0); }   // West: left
    }
    // For input ports, arrow points inward (toward machine center)
    if kind == 0u { arrow_dir = -arrow_dir; }

    // Triangle shape: strongest along arrow direction
    let along = dot(delta / radius, arrow_dir);
    let tri_shape = smoothstep(-0.3, 0.5, along);

    var port_color: vec3<f32>;
    if kind == 0u {
        port_color = vec3<f32>(0.3, 0.5, 1.0);  // Blue for input
    } else {
        port_color = vec3<f32>(1.0, 0.6, 0.2);  // Orange for output
    }
    return vec4<f32>(port_color, alpha * max(tri_shape, 0.5));
}

// Get port indicator overlay for a given machine type.
// Returns vec4(color.rgb, alpha) — alpha > 0 means a port indicator is here.
fn port_indicators(uv: vec2<f32>, mt: u32, facing: u32) -> vec4<f32> {
    let canon_size = machine_size_canonical(mt);
    var best = vec4<f32>(0.0);

    switch mt {
        case 0u: { // Composer (2×2): input South@(0,1), output North@(0,0)
            best = max(best, check_port(uv, canon_size, vec2<f32>(0.0, 1.0), 2u, facing, 0u));
            best = max(best, check_port(uv, canon_size, vec2<f32>(0.0, 0.0), 0u, facing, 1u));
        }
        case 1u: { // Inverter (3×2): input South@(1,1), output North@(1,0)
            best = max(best, check_port(uv, canon_size, vec2<f32>(1.0, 1.0), 2u, facing, 0u));
            best = max(best, check_port(uv, canon_size, vec2<f32>(1.0, 0.0), 0u, facing, 1u));
        }
        case 2u: { // Embedder (3×2): input0 South@(1,1), input1 West@(0,0), output North@(1,0)
            best = max(best, check_port(uv, canon_size, vec2<f32>(1.0, 1.0), 2u, facing, 0u));
            best = max(best, check_port(uv, canon_size, vec2<f32>(0.0, 0.0), 3u, facing, 0u));
            best = max(best, check_port(uv, canon_size, vec2<f32>(1.0, 0.0), 0u, facing, 1u));
        }
        case 3u: { // Quotient (3×2): input South@(1,1), output0 North@(1,0), output1 East@(2,0)
            best = max(best, check_port(uv, canon_size, vec2<f32>(1.0, 1.0), 2u, facing, 0u));
            best = max(best, check_port(uv, canon_size, vec2<f32>(1.0, 0.0), 0u, facing, 1u));
            best = max(best, check_port(uv, canon_size, vec2<f32>(2.0, 0.0), 1u, facing, 1u));
        }
        case 4u: { // Transformer (3×2): input0 South@(1,1), input1 West@(0,0), output North@(1,0)
            best = max(best, check_port(uv, canon_size, vec2<f32>(1.0, 1.0), 2u, facing, 0u));
            best = max(best, check_port(uv, canon_size, vec2<f32>(0.0, 0.0), 3u, facing, 0u));
            best = max(best, check_port(uv, canon_size, vec2<f32>(1.0, 0.0), 0u, facing, 1u));
        }
        case 5u: { // Source (1×1): output North@(0,0)
            best = max(best, check_port(uv, canon_size, vec2<f32>(0.0, 0.0), 0u, facing, 1u));
        }
        default: { } // Quadrupole, Dynamo: no ports
    }
    return best;
}

@vertex
fn vs_machine(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;

    let divisions = globals.grid_params.y;  // 64.0
    let khs = globals.grid_params.w;        // klein_half_side
    let cell_size = 2.0 * khs / divisions;

    let mt = u32(inst.machine_type + 0.5);
    let facing = u32(inst.facing + 0.5);
    let size = machine_size(mt, facing);
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
    out.facing = inst.facing;

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

    // Port indicators on top face
    let facing_u = u32(in.facing + 0.5);
    let port = port_indicators(in.uv, mt, facing_u);
    if port.w > 0.01 {
        color = mix(color, port.rgb, port.w * 0.85);
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
