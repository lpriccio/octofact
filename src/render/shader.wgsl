// Octofact shader: Mobius transform + gentle bowl embedding in vertex,
// eldritch palette + lighting in fragment.

struct Uniforms {
    view_proj: mat4x4<f32>,
    mobius_a: vec4<f32>,
    mobius_b: vec4<f32>,
    disk_params: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> u: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) depth_val: f32,
    @location(3) world_pos: vec3<f32>,
    @location(4) disk_r: f32,
};

// Complex multiply
fn cmul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}

// Complex conjugate
fn cconj(a: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(a.x, -a.y);
}

// Complex divide
fn cdiv(num: vec2<f32>, den: vec2<f32>) -> vec2<f32> {
    let d = dot(den, den);
    return vec2<f32>(
        (num.x * den.x + num.y * den.y) / d,
        (num.y * den.x - num.x * den.y) / d
    );
}

// Poincare disk -> gentle bowl (Y-up)
// Uses disk coordinates directly for X,Z with a mild upward curvature for lighting.
// This avoids the explosive stretching of the true hyperboloid near the boundary.
fn disk_to_bowl(z: vec2<f32>) -> vec3<f32> {
    let r2 = dot(z, z);
    // Gentle bowl: Y rises slowly with distance from center.
    // At r=0, Y=0. At r=0.95, Y≈0.36. Much gentler than hyperboloid.
    let y = 0.4 * r2 / (1.0 + r2);
    return vec3<f32>(z.x, y, z.y);
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let z = vec2<f32>(in.position.x, in.position.z);
    let vert_type = in.uv.x; // 0 = top face, 1 = side wall top, -1 = side wall bottom

    // Mobius transform: T(z) = (a*z + b) / (conj(b)*z + conj(a))
    let a = u.mobius_a.xy;
    let b = u.mobius_b.xy;
    let num = cmul(a, z) + b;
    let den = cmul(cconj(b), z) + cconj(a);
    let w = cdiv(num, den);

    let disk_r = length(w);
    var world: vec3<f32>;
    var normal: vec3<f32>;

    if vert_type > 0.5 {
        // Side wall top: bowl + elevation, horizontal outward normal
        world = disk_to_bowl(w);
        world.y += u.disk_params.y;
        let outward = normalize(vec2<f32>(w.x, w.y));
        normal = normalize(vec3<f32>(outward.x, 0.0, outward.y));
    } else if vert_type < -0.5 {
        // Side wall bottom: bowl only (no elevation), horizontal outward normal
        world = disk_to_bowl(w);
        let outward = normalize(vec2<f32>(w.x, w.y));
        normal = normalize(vec3<f32>(outward.x, 0.0, outward.y));
    } else {
        // Top face: bowl + elevation, normal via finite differences
        world = disk_to_bowl(w);
        world.y += u.disk_params.y;

        let eps = 0.001;
        let z_dx = z + vec2<f32>(eps, 0.0);
        let z_dy = z + vec2<f32>(0.0, eps);
        let num_dx = cmul(a, z_dx) + b;
        let den_dx = cmul(cconj(b), z_dx) + cconj(a);
        let w_dx = cdiv(num_dx, den_dx);
        let num_dy = cmul(a, z_dy) + b;
        let den_dy = cmul(cconj(b), z_dy) + cconj(a);
        let w_dy = cdiv(num_dy, den_dy);
        var world_dx = disk_to_bowl(w_dx);
        world_dx.y += u.disk_params.y;
        var world_dy = disk_to_bowl(w_dy);
        world_dy.y += u.disk_params.y;
        let tangent_x = world_dx - world;
        let tangent_y = world_dy - world;
        normal = normalize(cross(tangent_x, tangent_y));
    }

    out.clip_position = u.view_proj * vec4<f32>(world, 1.0);
    out.clip_position.z += u.disk_params.z;
    out.world_normal = normal;
    out.uv = in.uv;
    out.depth_val = u.disk_params.x;
    out.world_pos = world;
    out.disk_r = disk_r;

    return out;
}

// HSV to RGB conversion
fn hsv2rgb(h: f32, s: f32, v: f32) -> vec3<f32> {
    let c = v * s;
    let hp = h * 6.0;
    let x = c * (1.0 - abs(hp % 2.0 - 1.0));
    var rgb: vec3<f32>;
    if hp < 1.0 {
        rgb = vec3<f32>(c, x, 0.0);
    } else if hp < 2.0 {
        rgb = vec3<f32>(x, c, 0.0);
    } else if hp < 3.0 {
        rgb = vec3<f32>(0.0, c, x);
    } else if hp < 4.0 {
        rgb = vec3<f32>(0.0, x, c);
    } else if hp < 5.0 {
        rgb = vec3<f32>(x, 0.0, c);
    } else {
        rgb = vec3<f32>(c, 0.0, x);
    }
    let m = v - c;
    return rgb + vec3<f32>(m, m, m);
}

// Rainbow palette: 8 saturated colors cycling with depth
fn eldritch_palette(depth: f32) -> vec3<f32> {
    let cycle = u.disk_params.w;
    let t = (depth % cycle) / cycle;
    let hue = fract(t);
    return hsv2rgb(hue, 0.85, 0.85);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let base_color = eldritch_palette(in.depth_val);

    // Diffuse lighting from above
    let light_dir = normalize(vec3<f32>(0.3, 1.0, -0.2));
    let n = normalize(in.world_normal);
    let ndotl = max(dot(n, light_dir), 0.0);
    let ambient = 0.35;
    let diffuse = 0.65 * ndotl;
    let lighting = ambient + diffuse;

    // Edge darkening: uv.y = 0 at center, 1 at edge
    let edge = in.uv.y;
    let edge_factor = 1.0 - 0.45 * edge * edge;

    // Distance fade near disk boundary
    let fade = 1.0 - smoothstep(0.88, 0.99, in.disk_r);

    let color = base_color * lighting * edge_factor * fade;

    // Subtle rim highlight at tile edges for separation
    let rim = smoothstep(0.75, 1.0, edge) * 0.06;
    let final_color = color + vec3<f32>(rim * 0.4, rim * 0.3, rim * 0.7);

    return vec4<f32>(final_color, 1.0);
}
