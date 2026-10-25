// SPDX-License-Identifier: MIT OR Apache-2.0
// 3D view: wireframe overlay, hidden lines removed with the resolved depth.
struct WU {
    vp: mat4x4<f32>,
    p: vec4<f32>,                // disp, level, vtx_lod, near
    q: vec4<f32>,                // far, 0, 0, 0
    obj_a: array<vec4<f32>, 8>,
}
@group(0) @binding(0) var<uniform> u: WU;
@group(0) @binding(1) var maps_samp: sampler;
@group(0) @binding(2) var height_tex: texture_2d<f32>;
@group(0) @binding(3) var depth_tex: texture_2d<f32>;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) tan: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) misc: vec3<f32>,
    @location(5) disp: vec3<f32>,
    @location(6) obj: f32,
}

@vertex
fn vs_main(v: VIn) -> @builtin(position) vec4<f32> {
    let oi = u32(v.obj + 0.5);
    var p = v.pos;
    if (u.p.x != 0.0 && u.obj_a[oi].x < 0.5) {
        let h = textureSampleLevel(height_tex, maps_samp, v.uv, u.p.z).r;
        p = p + v.disp * (h - u.p.y) * u.p.x * v.misc.z;
    }
    return u.vp * vec4<f32>(p, 1.0);
}

fn lin(d: f32) -> f32 {
    let zn = d * 2.0 - 1.0;
    return 2.0 * u.p.w * u.q.x / (u.q.x + u.p.w - zn * (u.q.x - u.p.w));
}

fn dec_z(e: vec4<f32>) -> f32 {
    let z = dot(e, vec4<f32>(1.0, 1.0 / 255.0, 1.0 / 65025.0, 1.0 / 16581375.0));
    if (z > 0.9999999) {
        return 1.0;
    }
    return z;
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let d = dec_z(textureLoad(depth_tex, vec2<i32>(frag.xy), 0));
    let zl = lin(frag.z);
    let dl = lin(d);
    if (d < 1.0 && zl > dl * 1.0015 + 0.0005) {
        discard;
    }
    return vec4<f32>(1.0, 0.62, 0.12, 0.85);
}
