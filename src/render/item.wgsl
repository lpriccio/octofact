// Instanced item shader: renders small colored diamonds on belt surfaces.
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
    @location(7) klein_pos: vec2<f32>,  // pre-computed Klein-model position
    @location(8) item_type: f32,        // type index for color lookup
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) item_type: f32,
    @location(2) disk_r: f32,
};

@vertex
fn vs_item(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;

    let divisions = globals.grid_params.y;  // 64.0
    let khs = globals.grid_params.w;        // klein_half_side
    let cell_size = 2.0 * khs / divisions;

    // Scale unit quad to ~40% of cell size (items are small dots)
    let item_size = cell_size * 0.4;
    let klein = vert.local_pos * item_size + inst.klein_pos;

    // Klein -> Poincare -> Mobius -> bowl -> clip
    let poincare = klein_to_poincare(klein);
    let disk = apply_mobius(poincare, inst.mobius_a, inst.mobius_b);
    var world = disk_to_bowl(disk);
    world.y += 0.006;  // lift above belt surface (BELT_HEIGHT = 0.005)

    out.clip_position = globals.view_proj * vec4<f32>(world, 1.0);
    out.uv = vert.uv;
    out.item_type = inst.item_type;
    out.disk_r = length(disk);

    return out;
}

// Golden-ratio hue distribution for visually distinct item colors.
fn item_color(item_type: f32) -> vec3<f32> {
    let hue = fract(item_type * 0.618034);
    return hsv2rgb(hue, 0.85, 0.95);
}

@fragment
fn fs_item(in: VertexOutput) -> @location(0) vec4<f32> {
    // Distance fade near disk boundary
    let fade = 1.0 - smoothstep(0.85, 0.95, in.disk_r);
    if fade < 0.01 { discard; }

    // Diamond shape via L1 distance from center
    let c = in.uv - 0.5;
    let dist = abs(c.x) + abs(c.y);
    if dist > 0.45 { discard; }

    let color = item_color(in.item_type);

    // Smooth edge with inner glow
    let edge = smoothstep(0.45, 0.25, dist);
    let lit = color * (0.6 + 0.4 * edge);

    // Subtle highlight on top-left
    let highlight = (1.0 - in.uv.x) * (1.0 - in.uv.y);
    let final_color = lit + vec3<f32>(0.08) * smoothstep(0.3, 0.7, highlight) * edge;

    return vec4<f32>(final_color * fade, 1.0);
}
