const TAU: f32 = 6.28318530717959;
const PI: f32 = 3.14159265358979;
const HALF_PI: f32 = 1.5707963267949;
fn lum(p: vec4<f32>) -> f32 { return 0.2126 * p.x + 0.7152 * p.y + 0.0722 * p.z; }
fn smoothc(a: f32, b: f32, x: f32) -> f32 {
    let d = b - a;
    var t: f32;
    if (abs(d) < 1e-9) { t = select(0.0, 1.0, x >= b); } else { t = clamp((x - a) / d, 0.0, 1.0); }
    return t * t * (3.0 - 2.0 * t);
}
fn grey(v: f32) -> vec4<f32> { return vec4<f32>(v, v, v, 1.0); }
fn rem1(x: f32) -> f32 { return x - floor(x); }
fn rnd_away(x: f32) -> f32 { return sign(x) * floor(abs(x) + 0.5); }
fn wrapd(x: f32) -> f32 { return x - rnd_away(x); }
fn lerpf(a: f32, b: f32, t: f32) -> f32 { return a + (b - a) * t; }
fn lerp4(a: vec4<f32>, b: vec4<f32>, t: f32) -> vec4<f32> { return a + (b - a) * t; }
fn remi(a: i32, m: i32) -> i32 { let q = i32(floor(f32(a) / f32(m))); return a - q * m; }
fn rgb_to_hsv(c: vec3<f32>) -> vec3<f32> {
    let mx = max(c.x, max(c.y, c.z));
    let mn = min(c.x, min(c.y, c.z));
    let d = mx - mn;
    var h = 0.0;
    if (d >= 1e-9) {
        if (mx == c.x) {
            let q = (c.y - c.z) / d;
            h = q - 6.0 * floor(q / 6.0);
        } else if (mx == c.y) {
            h = (c.z - c.x) / d + 2.0;
        } else {
            h = (c.x - c.y) / d + 4.0;
        }
        h = h / 6.0;
    }
    var s = 0.0;
    if (mx > 0.0) { s = d / mx; }
    return vec3<f32>(h, s, mx);
}
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> vec3<f32> {
    let h6 = (h - floor(h)) * 6.0;
    let i = floor(h6);
    let f = h6 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let k = i32(i) % 6;
    if (k == 0) { return vec3<f32>(v, t, p); }
    if (k == 1) { return vec3<f32>(q, v, p); }
    if (k == 2) { return vec3<f32>(p, v, t); }
    if (k == 3) { return vec3<f32>(p, q, v); }
    if (k == 4) { return vec3<f32>(t, p, v); }
    return vec3<f32>(v, p, q);
}
fn s2l(c0: f32) -> f32 {
    let c = max(c0, 0.0);
    if (c <= 0.04045) { return c / 12.92; }
    return pow((c + 0.055) / 1.055, 2.4);
}
fn l2s(c0: f32) -> f32 {
    let c = max(c0, 0.0);
    if (c <= 0.0031308) { return c * 12.92; }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}
