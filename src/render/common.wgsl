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
fn disk_to_bowl(z: vec2<f32>) -> vec3<f32> {
    let r2 = dot(z, z);
    let y = 0.4 * r2 / (1.0 + r2);
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
