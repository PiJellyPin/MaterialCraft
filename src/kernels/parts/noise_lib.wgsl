fn hash2(x: i32, y: i32, seed: u32) -> u32 {
    var h = (bitcast<u32>(x) * 0x27d4eb2du) ^ (bitcast<u32>(y) * 0x165667b1u) ^ (seed * 0x9e3779b9u);
    h = h ^ (h >> 15u);
    h = h * 0x85ebca6bu;
    h = h ^ (h >> 13u);
    h = h * 0xc2b2ae35u;
    h = h ^ (h >> 16u);
    return h;
}
fn rnd(x: i32, y: i32, seed: u32) -> f32 { return f32(hash2(x, y, seed) >> 8u) / 16777216.0; }
fn fadef(t: f32) -> f32 { return t * t * t * (t * (t * 6.0 - 15.0) + 10.0); }
fn gdotp(cx: i32, cy: i32, dx: f32, dy: f32, seed: u32) -> f32 {
    let a = f32(hash2(cx, cy, seed) >> 20u) / 4096.0 * 6.28318530717959;
    return cos(a) * dx + sin(a) * dy;
}
// tileable gradient noise with integer lattice periods px, py (the host's noise::perlin)
fn perlin(uu: f32, vv: f32, px0: i32, py0: i32, seed: u32) -> f32 {
    let px = max(px0, 1);
    let py = max(py0, 1);
    let x = uu * f32(px);
    let y = vv * f32(py);
    let fxl = floor(x);
    let fyl = floor(y);
    let fx = x - fxl;
    let fy = y - fyl;
    let x0 = remi(i32(fxl), px);
    let y0 = remi(i32(fyl), py);
    var x1 = x0 + 1;
    if (x1 == px) { x1 = 0; }
    var y1 = y0 + 1;
    if (y1 == py) { y1 = 0; }
    let n00 = gdotp(x0, y0, fx, fy, seed);
    let n10 = gdotp(x1, y0, fx - 1.0, fy, seed);
    let n01 = gdotp(x0, y1, fx, fy - 1.0, seed);
    let n11 = gdotp(x1, y1, fx - 1.0, fy - 1.0, seed);
    let sx = fadef(fx);
    let sy = fadef(fy);
    let n = mix(mix(n00, n10, sx), mix(n01, n11, sx), sy);
    return clamp(n * 0.70710678118 + 0.5, 0.0, 1.0);
}
struct Vor { f1: f32, f2: f32, id1: f32, id2: f32, dx: f32, dy: f32 };
fn vdist(dx: f32, dy: f32, metric: u32, mp: f32) -> f32 {
    let ax = abs(dx);
    let ay = abs(dy);
    if (metric == 1u) { return ax + ay; }
    if (metric == 2u) { return max(ax, ay); }
    if (metric == 3u) { return pow(pow(ax, mp) + pow(ay, mp), 1.0 / mp); }
    return sqrt(dx * dx + dy * dy);
}
// tileable Voronoi with the two nearest sites (noise::voronoi_gen); metric 0 euclid, 1 manhattan, 2 chebyshev, 3 minkowski
fn voronoi_gen(uu: f32, vv: f32, px0: i32, py0: i32, seed: u32, id_seed: u32, jitter: f32, metric: u32, mp: f32) -> Vor {
    let px = max(px0, 1);
    let py = max(py0, 1);
    let x = uu * f32(px);
    let y = vv * f32(py);
    let ix = i32(floor(x));
    let iy = i32(floor(y));
    var r = Vor(3.4e38, 3.4e38, 0.0, 0.0, 0.0, 0.0);
    for (var dy = -1; dy <= 1; dy = dy + 1) {
        for (var dx = -1; dx <= 1; dx = dx + 1) {
            let cx = ix + dx;
            let cy = iy + dy;
            let wx = remi(cx, px);
            let wy = remi(cy, py);
            let fx = f32(cx) + 0.5 + (rnd(wx, wy, seed) - 0.5) * jitter;
            let fy = f32(cy) + 0.5 + (rnd(wx, wy, seed + 77u) - 0.5) * jitter;
            let d = vdist(fx - x, fy - y, metric, mp);
            let id = rnd(wx, wy, id_seed + 999u);
            if (d < r.f1) {
                r.f2 = r.f1;
                r.id2 = r.id1;
                r.f1 = d;
                r.id1 = id;
                r.dx = fx - x;
                r.dy = fy - y;
            } else if (d < r.f2) {
                r.f2 = d;
                r.id2 = id;
            }
        }
    }
    let k = 0.70710678118;
    r.f1 = clamp(r.f1 * k, 0.0, 1.0);
    r.f2 = clamp(r.f2 * k, 0.0, 1.0);
    return r;
}
fn voronoi_style(r: Vor, style: u32, thick: f32, round: f32) -> f32 {
    if (style == 1u) { return r.id1; }
    if (style == 2u) {
        let e = clamp((r.f2 - r.f1) / max(thick, 1e-3), 0.0, 1.0);
        return e + (e * e * (3.0 - 2.0 * e) - e) * clamp(round, 0.0, 1.0);
    }
    if (style == 3u) { return r.f2; }
    if (style == 4u) { return clamp((r.f2 - r.f1) * 2.0, 0.0, 1.0); }
    if (style == 5u) { return clamp(r.f1 * r.f2 * 3.0, 0.0, 1.0); }
    if (style == 6u) { return clamp(r.f1 / max(r.f2, 1e-4), 0.0, 1.0); }
    return r.f1;
}
// soft round spots, one per cell (noise::spots); shape mode 0..2 are the BnW Spots variants, 3 Gaussian Spots
// (a = size, b = variation), 4 Grunge specks (0.1, 0.18 + 0.2 id), 5 Grunge spots (0.12, 0.2 + 0.3 id)
fn spot_shape(mode: u32, id: f32, a: f32, b: f32) -> vec2<f32> {
    if (mode == 0u) { let rad = 0.18 + 0.5 * id * id; return vec2<f32>(rad * 0.8, rad); }
    if (mode == 1u) { let rad = 0.3 + 0.55 * id; return vec2<f32>(rad * 0.8, rad); }
    if (mode == 2u) { let rad = 0.08 + 0.3 * id; return vec2<f32>(rad * 0.8, rad); }
    if (mode == 3u) { let r = a * 0.55 * (1.0 - b + b * id); return vec2<f32>(0.0, max(r, 0.02)); }
    if (mode == 4u) { return vec2<f32>(0.1, 0.18 + id * 0.2); }
    return vec2<f32>(0.12, 0.2 + id * 0.3);
}
fn spots(uu: f32, vv: f32, px0: i32, py0: i32, seed: u32, mode: u32, a: f32, b: f32, grain: f32) -> f32 {
    let px = max(px0, 1);
    let py = max(py0, 1);
    let x = uu * f32(px);
    let y = vv * f32(py);
    let ix = i32(floor(x));
    let iy = i32(floor(y));
    var best = 0.0;
    for (var dy = -1; dy <= 1; dy = dy + 1) {
        for (var dx = -1; dx <= 1; dx = dx + 1) {
            let cx = ix + dx;
            let cy = iy + dy;
            let wx = remi(cx, px);
            let wy = remi(cy, py);
            let fx = f32(cx) + 0.5 + (rnd(wx, wy, seed) - 0.5);
            let fy = f32(cy) + 0.5 + (rnd(wx, wy, seed + 77u) - 0.5);
            let d = max(sqrt((fx - x) * (fx - x) + (fy - y) * (fy - y)) * 0.70710678118 + grain, 0.0);
            let lh = spot_shape(mode, rnd(wx, wy, seed + 999u), a, b);
            let t = clamp((d - lh.x) / max(lh.y - lh.x, 1e-6), 0.0, 1.0);
            best = max(best, 1.0 - t * t * (3.0 - 2.0 * t));
        }
    }
    return best;
}
fn fbm_n(uu: f32, vv: f32, px: i32, py: i32, octaves: u32, gain: f32, seed: u32, mode: u32) -> f32 {
    var amp = 1.0;
    var sum = 0.0;
    var norm = 0.0;
    var a = uu;
    var b = vv;
    let oct = clamp(octaves, 1u, 12u);
    for (var o = 0u; o < oct; o = o + 1u) {
        let n = perlin(a, b, px, py, seed + o * 131u);
        var m = n;
        if (mode == 1u) { m = 1.0 - abs(2.0 * n - 1.0); } else if (mode == 2u) { m = abs(2.0 * n - 1.0); }
        sum = sum + m * amp;
        norm = norm + amp;
        amp = amp * gain;
        let na = 2.0 * a - b;
        let nb = a + 2.0 * b;
        a = na - floor(na);
        b = nb - floor(nb);
    }
    if (norm > 0.0) {
        var k = 1.35;
        if (mode == 0u) { k = 1.7; }
        return clamp(0.5 + (sum / norm - 0.5) * k, 0.0, 1.0);
    }
    return 0.0;
}
fn mixh(x: u32) -> u32 {
    var h = x;
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return h;
}
fn hashp(x: i32, y: i32, seed: u32) -> u32 {
    return mixh(bitcast<u32>(x) + mixh(bitcast<u32>(y) + mixh(seed + 0x9e3779b9u)));
}
fn unitp(x: i32, y: i32, seed: u32) -> f32 { return f32(hashp(x, y, seed) >> 8u) / 16777216.0; }
