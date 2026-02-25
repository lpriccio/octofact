// Instanced tile shader: per-instance Mobius from vertex buffer slot 1.
// Prepended by common.wgsl at load time.

struct Globals {
    view_proj: mat4x4<f32>,
    grid_params: vec4<f32>,  // (enabled, divisions, line_width, klein_half_side)
    color_cycle: f32,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct InstanceInput {
    @location(5) mobius_a: vec2<f32>,
    @location(6) mobius_b: vec2<f32>,
    @location(7) depth: f32,
    @location(8) elevation: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) depth_val: f32,
    @location(3) world_pos: vec3<f32>,
    @location(4) disk_r: f32,
    @location(5) mobius_a: vec2<f32>,
    @location(6) mobius_b: vec2<f32>,
};

@vertex
fn vs_tile(
    vert: VertexInput,
    inst: InstanceInput,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let z = vec2<f32>(vert.position.x, vert.position.z);
    let vert_type = vert.uv.x;

    // Mobius transform from instance data
    let w = apply_mobius(z, inst.mobius_a, inst.mobius_b);
    let disk_r = length(w);

    var world: vec3<f32>;
    var normal: vec3<f32>;

    if vert_type > 0.5 {
        // Side wall top: bowl + elevation
        world = disk_to_bowl(w);
        world.y += inst.elevation;
        let outward = normalize(w);
        normal = normalize(vec3<f32>(outward.x, 0.0, outward.y));
    } else if vert_type < -0.5 {
        // Side wall bottom: bowl only (no elevation)
        world = disk_to_bowl(w);
        let outward = normalize(w);
        normal = normalize(vec3<f32>(outward.x, 0.0, outward.y));
    } else {
        // Top face: bowl + elevation, normal via finite differences
        world = disk_to_bowl(w);
        world.y += inst.elevation;

        let eps = 0.001;
        let w_dx = apply_mobius(z + vec2<f32>(eps, 0.0), inst.mobius_a, inst.mobius_b);
        let w_dy = apply_mobius(z + vec2<f32>(0.0, eps), inst.mobius_a, inst.mobius_b);
        var world_dx = disk_to_bowl(w_dx);
        world_dx.y += inst.elevation;
        var world_dy = disk_to_bowl(w_dy);
        world_dy.y += inst.elevation;
        normal = normalize(cross(world_dx - world, world_dy - world));
    }

    out.clip_position = globals.view_proj * vec4<f32>(world, 1.0);
    // Small z-bias per instance to avoid z-fighting between coplanar tiles
    out.clip_position.z += f32(instance_index) * 1e-6;
    out.world_normal = normal;
    out.uv = vert.uv;
    out.depth_val = inst.depth;
    out.world_pos = world;
    out.disk_r = disk_r;
    out.mobius_a = inst.mobius_a;
    out.mobius_b = inst.mobius_b;

    return out;
}

@fragment
fn fs_tile(in: VertexOutput) -> @location(0) vec4<f32> {
    let base_color = eldritch_palette(in.depth_val, globals.color_cycle);

    // Diffuse lighting from above
    let light_dir = normalize(vec3<f32>(0.3, 1.0, -0.2));
    let n = normalize(in.world_normal);
    let ndotl = max(dot(n, light_dir), 0.0);
    let ambient = 0.35;
    let diffuse = 0.65 * ndotl;
    let lighting = ambient + diffuse;

    // Edge darkening
    let edge = in.uv.y;
    let edge_factor = 1.0 - 0.45 * edge * edge;

    // Distance fade near disk boundary
    let fade = 1.0 - smoothstep(0.88, 0.99, in.disk_r);

    let color = base_color * lighting * edge_factor * fade;

    // Rim highlight
    let rim = smoothstep(0.75, 1.0, edge) * 0.06;
    var final_color = color + vec3<f32>(rim * 0.4, rim * 0.3, rim * 0.7);

    // Grid overlay (top face only, {4,n} square cells)
    if globals.grid_params.x > 0.5 && in.uv.x > -0.5 && in.uv.x < 0.5 {
        // Inverse Mobius to get local Poincare position
        let w = vec2<f32>(in.world_pos.x, in.world_pos.z);
        let local_p = apply_inverse_mobius(w, in.mobius_a, in.mobius_b);

        // Poincare -> Klein for straight grid lines
        let local_k = poincare_to_klein(local_p);

        // Normalize so edges map to +/-0.5
        let norm = local_k / (2.0 * globals.grid_params.w);

        // Grid at N divisions
        let gp = norm * globals.grid_params.y;
        let fx = fract(gp.x);
        let fy = fract(gp.y);
        let nearest = min(min(fx, 1.0 - fx), min(fy, 1.0 - fy));
        let lw = globals.grid_params.z;
        let t = smoothstep(0.0, lw, nearest);
        final_color = mix(vec3<f32>(0.06, 0.06, 0.10), final_color, t);
    }

    return vec4<f32>(final_color, 1.0);
}
