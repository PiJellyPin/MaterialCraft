// SPDX-License-Identifier: MIT OR Apache-2.0
// 3D view: shadow map pass (depth only, same displacement as the scene pass).
struct DU {
    vp: mat4x4<f32>,
    p: vec4<f32>,                // disp, level, vtx_lod, 0
    obj_a: array<vec4<f32>, 8>,
}
@group(0) @binding(0) var<uniform> u: DU;
@group(0) @binding(1) var maps_samp: sampler;
@group(0) @binding(2) var height_tex: texture_2d<f32>;

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
