// Shared WGSL functions for Octofact hyperbolic rendering.
// Concatenated as a prefix to each per-pass shader (tile, belt, machine, item).

// --- Complex arithmetic ---

fn cmul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}

fn cconj(a: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(a.x, -a.y);
}

fn cdiv(num: vec2<f32>, den: vec2<f32>) -> vec2<f32> {
    let d = dot(den, den);
    return vec2<f32>(
        (num.x * den.x + num.y * den.y) / d,
        (num.y * den.x - num.x * den.y) / d
    );
}

// --- Mobius transforms ---

// Forward Mobius: T(z) = (a*z + b) / (conj(b)*z + conj(a))
fn apply_mobius(z: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let num = cmul(a, z) + b;
    let den = cmul(cconj(b), z) + cconj(a);
    return cdiv(num, den);
}

// Inverse Mobius: T^-1(w) = (conj(a)*w - b) / (a - conj(b)*w)
fn apply_inverse_mobius(w: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let num = cmul(cconj(a), w) - b;
    let den = a - cmul(cconj(b), w);
    return cdiv(num, den);
}

// --- Coordinate conversions ---

// Poincare disk -> gentle bowl embedding (Y-up).
// X,Z from disk coords, Y rises gently with distance from center.
// `h` is the bowl height parameter (asymptotic max Y as r -> inf).
fn disk_to_bowl_h(z: vec2<f32>, h: f32) -> vec3<f32> {
    let r2 = dot(z, z);
    let y = h * r2 / (1.0 + r2);
    return vec3<f32>(z.x, y, z.y);
}

// Poincare disk -> Klein disk: K = 2P / (1 + |P|^2)
fn poincare_to_klein(p: vec2<f32>) -> vec2<f32> {
    let r2 = dot(p, p);
    return 2.0 * p / (1.0 + r2);
}

// Klein disk -> Poincare disk: P = K / (1 + sqrt(1 - |K|^2))
fn klein_to_poincare(k: vec2<f32>) -> vec2<f32> {
    let r2 = dot(k, k);
    return k / (1.0 + sqrt(max(1.0 - r2, 0.0)));
}

// --- Color utilities ---

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

// Rainbow palette cycling with BFS depth.
fn eldritch_palette(depth: f32, cycle: f32) -> vec3<f32> {
    let t = (depth % cycle) / cycle;
    let hue = fract(t);
    return hsv2rgb(hue, 0.85, 0.85);
}

// --- SDF shape primitives ---
// All primitives expect p centered at origin; pattern functions should do p = uv - 0.5.

// Distance to circle boundary.
fn sdf_circle(p: vec2<f32>, r: f32) -> f32 {
    return length(p) - r;
}

// Distance to axis-aligned rectangle boundary.
fn sdf_box(p: vec2<f32>, half_size: vec2<f32>) -> f32 {
    let d = abs(p) - half_size;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

// Distance to line segment from a to b.
fn sdf_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

// Distance to circular arc centered at origin with radius r,
// spanning from angle_start to angle_start + angle_span (radians).
fn sdf_arc(p: vec2<f32>, r: f32, angle_start: f32, angle_span: f32) -> f32 {
    // Rotate p so arc is centered on +x axis
    let mid = angle_start + angle_span * 0.5;
    let cs = cos(-mid);
    let sn = sin(-mid);
    let q = vec2<f32>(cs * p.x - sn * p.y, sn * p.x + cs * p.y);
    // Half-angle of the span
    let ha = angle_span * 0.5;
    // Angle of rotated point
    let a = atan2(q.y, q.x);
    if abs(a) < ha {
        // Closest point is on the arc
        return abs(length(q) - r);
    }
    // Closest point is one of the arc endpoints
    let e1 = vec2<f32>(cos(ha), sin(ha)) * r;
    let e2 = vec2<f32>(cos(ha), -sin(ha)) * r;
    return min(length(q - e1), length(q - e2));
}

// --- SDF operations ---

// Smooth minimum (soft blend) of two distance fields.
fn sdf_smooth_union(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) - k * h * (1.0 - h);
}

// Turn any SDF into a ring/outline of given thickness.
fn sdf_annular(d: f32, thickness: f32) -> f32 {
    return abs(d) - thickness;
}

// --- SDF rendering helpers ---

// Anti-aliased fill: 1.0 inside, 0.0 outside, smooth transition at boundary.
fn sdf_fill(d: f32) -> f32 {
    let fw = fwidth(d);
    return 1.0 - smoothstep(-fw, fw, d);
}

// Anti-aliased stroke of given width centered on the SDF boundary.
fn sdf_stroke(d: f32, width: f32) -> f32 {
    let fw = fwidth(d);
    let half_w = width * 0.5;
    return 1.0 - smoothstep(-fw, fw, abs(d) - half_w);
}

// 2D rotation matrix.
fn rot2(angle: f32) -> mat2x2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return mat2x2<f32>(c, s, -s, c);
}

// --- Procedural noise ---

// Pseudo-random hash: vec2 -> f32 in [0,1].
fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + vec3<f32>(dot(p3, vec3<f32>(p3.y + 33.33, p3.z + 33.33, p3.x + 33.33)));
    return fract((p3.x + p3.y) * p3.z);
}

// Smooth value noise: bilinear interpolation of hashed grid corners.
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    // Smooth interpolation curve
    let u = f * f * (3.0 - 2.0 * f);
    // Four grid corners
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// --- 3D SDF primitives ---

fn sdf3_sphere(p: vec3<f32>, r: f32) -> f32 {
    return length(p) - r;
}

fn sdf3_box(p: vec3<f32>, b: vec3<f32>) -> f32 {
    let q = abs(p) - b;
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn sdf3_round_box(p: vec3<f32>, b: vec3<f32>, r: f32) -> f32 {
    return sdf3_box(p, b) - r;
}

fn sdf3_torus(p: vec3<f32>, R: f32, r: f32) -> f32 {
    let q = vec2<f32>(length(p.xz) - R, p.y);
    return length(q) - r;
}

fn sdf3_octahedron(p: vec3<f32>, s: f32) -> f32 {
    let q = abs(p);
    return (q.x + q.y + q.z - s) * 0.57735027;
}

fn sdf3_cylinder(p: vec3<f32>, h: f32, r: f32) -> f32 {
    let d = vec2<f32>(length(p.xz) - r, abs(p.y) - h);
    return min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0)));
}

fn sdf3_capsule(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>, r: f32) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h) - r;
}

// Twist around Y axis — returns modified position.
fn sdf3_twist_y(p: vec3<f32>, k: f32) -> vec3<f32> {
    let c = cos(k * p.y);
    let s = sin(k * p.y);
    let q = vec2<f32>(c * p.x - s * p.z, s * p.x + c * p.z);
    return vec3<f32>(q.x, p.y, q.y);
}

// --- 3D rotation matrices ---

fn rot3_x(a: f32) -> mat3x3<f32> {
    let c = cos(a);
    let s = sin(a);
    return mat3x3<f32>(
        1.0, 0.0, 0.0,
        0.0, c,   s,
        0.0, -s,  c
    );
}

fn rot3_y(a: f32) -> mat3x3<f32> {
    let c = cos(a);
    let s = sin(a);
    return mat3x3<f32>(
        c,   0.0, -s,
        0.0, 1.0, 0.0,
        s,   0.0, c
    );
}

fn rot3_z(a: f32) -> mat3x3<f32> {
    let c = cos(a);
    let s = sin(a);
    return mat3x3<f32>(
        c,   s,   0.0,
        -s,  c,   0.0,
        0.0, 0.0, 1.0
    );
}
