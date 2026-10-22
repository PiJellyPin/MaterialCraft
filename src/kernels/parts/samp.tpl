fn wr$S(a: i32, m: i32) -> i32 {
    let q = i32(floor(f32(a) / f32(m)));
    return a - q * m;
}
fn tx$S(x: i32, y: i32) -> vec4<f32> {
    return $B[u32(wr$S(y, i32(IH()))) * IW() + u32(wr$S(x, i32(IW())))];
}
fn sp$S(uu: f32, vv: f32) -> vec4<f32> {
    let x = uu * f32(IW()) - 0.5;
    let y = vv * f32(IH()) - 0.5;
    let x0 = floor(x);
    let y0 = floor(y);
    let fx = x - x0;
    let fy = y - y0;
    let xa = i32(x0);
    let ya = i32(y0);
    let a = tx$S(xa, ya);
    let b = tx$S(xa + 1, ya);
    let c = tx$S(xa, ya + 1);
    let d = tx$S(xa + 1, ya + 1);
    let top = a + (b - a) * fx;
    let bot = c + (d - c) * fx;
    return top + (bot - top) * fy;
}
fn cw$S(t: f32) -> vec4<f32> {
    return vec4<f32>(
        0.5 * (-t * t * t + 2.0 * t * t - t),
        0.5 * (3.0 * t * t * t - 5.0 * t * t + 2.0),
        0.5 * (-3.0 * t * t * t + 4.0 * t * t + t),
        0.5 * (t * t * t - t * t));
}
fn cu$S(uu: f32, vv: f32) -> vec4<f32> {
    let x = uu * f32(IW()) - 0.5;
    let y = vv * f32(IH()) - 0.5;
    let x0 = floor(x);
    let y0 = floor(y);
    let wx = cw$S(x - x0);
    let wy = cw$S(y - y0);
    let xi = i32(x0);
    let yi = i32(y0);
    var o = vec4<f32>(0.0);
    for (var j = 0; j < 4; j = j + 1) {
        for (var i = 0; i < 4; i = i + 1) {
            o = o + tx$S(xi + i - 1, yi + j - 1) * (wx[i] * wy[j]);
        }
    }
    let n0 = tx$S(xi, yi);
    let n1 = tx$S(xi + 1, yi);
    let n2 = tx$S(xi, yi + 1);
    let n3 = tx$S(xi + 1, yi + 1);
    let lo = min(min(n0, n1), min(n2, n3));
    let hi = max(max(n0, n1), max(n2, n3));
    return clamp(o, lo, hi);
}
