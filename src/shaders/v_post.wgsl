// SPDX-License-Identifier: MIT OR Apache-2.0
// 3D view: post processing, a line by line port of src/post.rs (the CPU renderer) so both views agree.
// Pixel coordinates are counted from the top left, like the CPU code.
struct PU {
    size: vec4<f32>,    // width, height, near, far
    cam: vec4<f32>,     // tan_half, aspect, centre distance, bypass
    s0: vec4<f32>,      // ssao, ssao radius, dof, focus
    s1: vec4<f32>,      // bloom, bloom threshold, vignette, contrast
    s2: vec4<f32>,      // saturation, tone map, bloom levels, 0
    wb: vec4<f32>,
    bw: array<vec4<f32>, 2>,
}
@group(0) @binding(0) var<uniform> u: PU;
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var depth_tex: texture_2d<f32>;
@group(0) @binding(3) var bright_tex: texture_2d<f32>;
@group(0) @binding(4) var lin_samp: sampler;

const PI: f32 = 3.14159265;
const INF: f32 = 1e30;

var<private> W: i32;
var<private> H: i32;

struct VOut {
    @builtin(position) clip: vec4<f32>,
}
@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VOut {
    let p = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    var o: VOut;
    o.clip = vec4<f32>(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
    return o;
}

fn dec_z(e: vec4<f32>) -> f32 {
    let z = dot(e, vec4<f32>(1.0, 1.0 / 255.0, 1.0 / 65025.0, 1.0 / 16581375.0));
    if (z > 0.9999999) {
        return 1.0;
    }
    return z;
}
fn lin_depth(d: f32) -> f32 {
    if (d >= 1.0) {
        return INF;
    }
    let zn = d * 2.0 - 1.0;
    return 2.0 * u.size.z * u.size.w / (u.size.w + u.size.z - zn * (u.size.w - u.size.z));
}
fn z_at(x0: i32, y0: i32) -> f32 {
    let x = clamp(x0, 0, W - 1);
    let y = clamp(y0, 0, H - 1);
    return lin_depth(dec_z(textureLoad(depth_tex, vec2<i32>(x, y), 0)));
}
fn src_at(x0: i32, y0: i32) -> vec3<f32> {
    return textureLoad(src_tex, vec2<i32>(x0, y0), 0).rgb;
}
fn ign(x: f32, y: f32) -> f32 {
    return fract(52.982919 * fract(x * 0.06711056 + y * 0.00583715));
}
fn view_pos(x: f32, y: f32, z: f32) -> vec3<f32> {
    let nx = (x + 0.5) / f32(W) * 2.0 - 1.0;
    let ny = 1.0 - (y + 0.5) / f32(H) * 2.0;
    return vec3<f32>(nx * u.cam.y * u.cam.x * z, ny * u.cam.x * z, z);
}

fn ssao_factor(x: i32, y: i32) -> f32 {
    let z = z_at(x, y);
    if (z > 1e29) {
        return 1.0;
    }
    let radius = u.s0.y;
    let p = view_pos(f32(x), f32(y), z);
    let zxp = z_at(x + 1, y);
    let zxm = z_at(x - 1, y);
    let zyp = z_at(x, y + 1);
    let zym = z_at(x, y - 1);
    var zx: f32;
    var sx: f32;
    var zy: f32;
    var sy: f32;
    if (abs(zxp - z) < abs(zxm - z)) {
        zx = zxp;
        sx = 1.0;
    } else {
        zx = zxm;
        sx = -1.0;
    }
    if (abs(zyp - z) < abs(zym - z)) {
        zy = zyp;
        sy = 1.0;
    } else {
        zy = zym;
        sy = -1.0;
    }
    var zxx = z;
    if (zx < 1e29) {
        zxx = zx;
    }
    var zyy = z;
    if (zy < 1e29) {
        zyy = zy;
    }
    let px = view_pos(f32(x) + sx, f32(y), zxx);
    let py = view_pos(f32(x), f32(y) + sy, zyy);
    var n = normalize(cross(px - p, py - p));
    if (dot(n, p) > 0.0) {
        n = -n;
    }
    var up = vec3<f32>(1.0, 0.0, 0.0);
    if (abs(n.y) < 0.999) {
        up = vec3<f32>(0.0, 1.0, 0.0);
    }
    let t = normalize(cross(up, n));
    let b = cross(n, t);
    let rot = ign(f32(x), f32(y)) * 2.0 * PI;
    var occ = 0.0;
    for (var i = 0; i < 14; i = i + 1) {
        let f = (f32(i) + 0.5) / 14.0;
        let a = f * 7.0 * PI + rot;
        let hz = max(1.0 - f * 0.95, 0.05);
        let hr = sqrt(max(1.0 - hz * hz, 0.0));
        let v = vec3<f32>(cos(a) * hr, sin(a) * hr, hz);
        let scale = radius * (0.2 + 0.8 * f * f);
        let sp = p + (t * v.x + b * v.y + n * v.z) * scale;
        if (sp.z <= 0.05) {
            continue;
        }
        let sxs = ((sp.x / (sp.z * u.cam.y * u.cam.x)) * 0.5 + 0.5) * f32(W) - 0.5;
        let sys = (0.5 - sp.y / (sp.z * u.cam.x) * 0.5) * f32(H) - 0.5;
        let sz = z_at(i32(floor(sxs + 0.5)), i32(floor(sys + 0.5)));
        if (sz < 1e29 && sz < sp.z - 0.01 * radius) {
            let r = clamp(radius / max(abs(p.z - sz), 1e-4), 0.0, 1.0);
            occ = occ + r * r * (3.0 - 2.0 * r);
        }
    }
    return 1.0 - occ / 14.0;
}

fn coc(z: f32) -> f32 {
    let max_r = 0.014 * f32(H);
    if (z > 1e29) {
        return max_r;
    }
    var focus = u.cam.z;
    if (u.s0.w > 0.0) {
        focus = u.s0.w;
    }
    return min(u.s0.z * abs(z - focus) / max(z, 0.05) * 0.045 * f32(H), max_r);
}

fn aces1(x: f32) -> f32 {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}
fn aces(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(aces1(c.x), aces1(c.y), aces1(c.z));
}
fn hable1(x: f32) -> f32 {
    let A = 0.15;
    let B = 0.50;
    let C = 0.10;
    let D = 0.20;
    let E = 0.02;
    let F = 0.30;
    return ((x * (A * x + C * B) + D * E) / (x * (A * x + B) + D * F)) - E / F;
}
fn tonemap(c0: vec3<f32>) -> vec3<f32> {
    let c = max(c0, vec3<f32>(0.0));
    let tone = i32(round(u.s2.y));
    if (tone == 4) {
        return min(c, vec3<f32>(1.0));
    }
    if (tone == 3) {
        return c / (1.0 + c);
    }
    if (tone == 1) {
        return aces(c);
    }
    if (tone == 2) {
        let w = 1.0 / hable1(11.2);
        return clamp(vec3<f32>(hable1(2.0 * c.x), hable1(2.0 * c.y), hable1(2.0 * c.z)) * w, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    let l = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    let k = aces1(l) / max(l, 1e-4);
    let hue = min(c * k, vec3<f32>(1.0));
    let a = aces(c);
    return mix(hue, a, smoothstep(2.0, 8.0, l) * 0.7);
}

fn setup() {
    W = i32(u.size.x);
    H = i32(u.size.y);
}

@fragment
fn fs_ssao(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    setup();
    let P = vec2<i32>(frag.xy);
    let c = textureLoad(src_tex, P, 0).rgb;
    let k = 1.0 - u.s0.x * (1.0 - ssao_factor(P.x, P.y));
    return vec4<f32>(c * k, 1.0);
}

@fragment
fn fs_dof(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    setup();
    let P = vec2<i32>(frag.xy);
    let x = P.x;
    let y = P.y;
    let c0 = textureLoad(src_tex, P, 0).rgb;
    let r = coc(z_at(x, y));
    if (r < 0.5) {
        return vec4<f32>(c0, 1.0);
    }
    let rot = ign(f32(x), f32(y)) * 2.0 * PI;
    var acc = vec3<f32>(0.0);
    var ws = 0.0;
    for (var k = 0; k < 28; k = k + 1) {
        let f = (f32(k) + 0.5) / 28.0;
        let a = f32(k) * 2.399963 + rot;
        let rr = sqrt(f) * r;
        let sx = i32(clamp(floor(f32(x) + cos(a) * rr + 0.5), 0.0, f32(W - 1)));
        let sy = i32(clamp(floor(f32(y) + sin(a) * rr + 0.5), 0.0, f32(H - 1)));
        var wgt = 0.0;
        if (coc(z_at(sx, sy)) + 0.5 >= rr) {
            wgt = 1.0;
        }
        acc = acc + src_at(sx, sy) * wgt;
        ws = ws + wgt;
    }
    if (ws < 1.0) {
        return vec4<f32>(c0, 1.0);
    }
    return vec4<f32>(acc / ws, 1.0);
}

@fragment
fn fs_bright(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let P = vec2<i32>(frag.xy);
    let c = textureLoad(src_tex, P, 0).rgb;
    let l = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    let thr = u.s1.y;
    let k = smoothstep(thr, thr + 0.5 + thr * 0.5, l);
    return vec4<f32>(c * k, 1.0);
}

@fragment
fn fs_final(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    setup();
    let P = vec2<i32>(frag.xy);
    let x = P.x;
    let y = P.y;
    let uv = frag.xy / u.size.xy;
    var c = textureLoad(src_tex, P, 0).rgb;
    if (u.cam.w > 0.5) {
        return vec4<f32>(c, 1.0);
    }
    if (u.s1.x > 0.0) {
        var b = vec3<f32>(0.0);
        let levels = i32(round(u.s2.z));
        for (var k = 0; k < 8; k = k + 1) {
            if (k >= levels) {
                break;
            }
            b = b + textureSampleLevel(bright_tex, lin_samp, uv, f32(k)).rgb * u.bw[k / 4][k % 4];
        }
        c = c + b * u.s1.x;
    }
    c = c * u.wb.xyz;
    let contrast = u.s1.w;
    if (abs(contrast - 1.0) > 1e-4) {
        c = 0.18 * pow(max(c, vec3<f32>(0.0)) / 0.18, vec3<f32>(contrast));
    }
    let sat = u.s2.x;
    if (abs(sat - 1.0) > 1e-4) {
        let l = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
        c = vec3<f32>(l) + (c - vec3<f32>(l)) * sat;
    }
    let vig = u.s1.z;
    if (vig > 0.0) {
        let nx = (f32(x) + 0.5) / f32(W) * 2.0 - 1.0;
        let ny = (f32(y) + 0.5) / f32(H) * 2.0 - 1.0;
        let r = sqrt(nx * nx * clamp(u.cam.y, 0.5, 1.5) + ny * ny) / 1.5;
        c = c * (1.0 - vig * smoothstep(0.35, 1.0, r));
    }
    return vec4<f32>(pow(clamp(tonemap(c), vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / 2.2)), 1.0);
}
