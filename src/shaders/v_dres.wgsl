// SPDX-License-Identifier: MIT OR Apache-2.0
// Resolves the multisampled scene (colour: the average of the samples, depth: the first sample) by hand, so the result
// does not depend on how an adapter resolves several attachments at once.
@group(0) @binding(0) var csrc: texture_multisampled_2d<f32>;
@group(0) @binding(1) var dsrc: texture_multisampled_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
}

struct FOut {
    @location(0) c: vec4<f32>,
    @location(1) z: vec4<f32>,
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> FOut {
    let p = vec2<i32>(frag.xy);
    var acc = vec4<f32>(0.0);
    for (var s = 0; s < 4; s = s + 1) {
        acc = acc + min(textureLoad(csrc, p, s), vec4<f32>(60000.0));
    }
    var o: FOut;
    o.c = acc * 0.25;
    o.z = textureLoad(dsrc, p, 0);
    return o;
}
