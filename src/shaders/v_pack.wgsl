// SPDX-License-Identifier: MIT OR Apache-2.0
// Packs graph outputs (RGBA f32 buffers on the device) into the 8-bit / 16-bit layouts the 3D view samples, with rows
// padded to 256 bytes so the result can be copied straight into a texture. No pixel leaves the device.
struct PK {
    w: u32,
    h: u32,
    stride: u32,     // u32 words per padded row
    mode: u32,       // 0 rgb8, 1 rough/metal/ao, 2 luminance as 16-bit floats, 3 rgb8 of a normal map
    flip_g: u32,
    has: u32,        // bit 0..2: source 0..2 present
    inv_r: u32,      // roughness is a glossiness map
    pad: u32,
    dflt: vec4<f32>, // defaults of roughness, metallic, occlusion
}
@group(0) @binding(0) var<uniform> u: PK;
@group(0) @binding(1) var<storage, read> s0: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> s1: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> s2: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> dst: array<u32>;

fn lum(p: vec4<f32>) -> f32 {
    return 0.2126 * p.x + 0.7152 * p.y + 0.0722 * p.z;
}
fn tex_lum_h(i: u32) -> f32 {
    let v = lum(s0[i]);
    if (v != v) {
        return 0.0;
    }
    return v;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.stride || id.y >= u.h) {
        return;
    }
    let o = id.y * u.stride + id.x;
    // keep the buffer bound: the guard is never true
    if (u.pad == 0xFFFFFFFFu) {
        dst[0] = arrayLength(&s1) + arrayLength(&s2) + arrayLength(&s0);
    }
    if (u.mode == 2u) {
        let x0 = id.x * 2u;
        var a = 0.0;
        var b = 0.0;
        if (x0 < u.w) {
            a = tex_lum_h(id.y * u.w + x0);
        }
        if (x0 + 1u < u.w) {
            b = tex_lum_h(id.y * u.w + x0 + 1u);
        }
        dst[o] = pack2x16float(vec2<f32>(a, b));
        return;
    }
    if (id.x >= u.w) {
        dst[o] = 0u;
        return;
    }
    let i = id.y * u.w + id.x;
    if (u.mode == 0u || u.mode == 3u) {
        var c = clamp(s0[i], vec4<f32>(0.0), vec4<f32>(1.0));
        if ((u.has & 1u) == 0u) {
            // no picture connected: mid grey, or a flat normal
            c = vec4<f32>(0.5, 0.5, select(0.5, 1.0, u.mode == 3u), 1.0);
        }
        if (c.x != c.x) { c.x = 0.0; }
        if (c.y != c.y) { c.y = 0.0; }
        if (c.z != c.z) { c.z = 0.0; }
        if (u.flip_g != 0u && (u.has & 1u) != 0u) {
            c.y = 1.0 - c.y;
        }
        dst[o] = pack4x8unorm(vec4<f32>(c.x, c.y, c.z, 1.0));
        return;
    }
    var r = u.dflt.x;
    var m = u.dflt.y;
    var a = u.dflt.z;
    if ((u.has & 1u) != 0u) {
        r = lum(s0[i]);
        if (u.inv_r != 0u) {
            r = 1.0 - r;
        }
    }
    if ((u.has & 2u) != 0u) {
        m = lum(s1[i]);
    }
    if ((u.has & 4u) != 0u) {
        a = lum(s2[i]);
    }
    r = clamp(r, 0.0, 1.0);
    m = clamp(m, 0.0, 1.0);
    a = clamp(a, 0.0, 1.0);
    if (r != r) { r = 0.0; }
    if (m != m) { m = 0.0; }
    if (a != a) { a = 0.0; }
    dst[o] = pack4x8unorm(vec4<f32>(r, m, a, 1.0));
}
