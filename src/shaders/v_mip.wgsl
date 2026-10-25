// SPDX-License-Identifier: MIT OR Apache-2.0
// Mip chain by box filtering the previous level (one fullscreen pass per level, all on the device).
@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}
@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VOut {
    let p = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    var o: VOut;
    o.clip = vec4<f32>(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
    o.uv = p;
    return o;
}
@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(src_tex, samp, in.uv, 0.0);
}
