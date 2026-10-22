// SPDX-License-Identifier: MIT OR Apache-2.0
// Lowest point of the displaced object: the floor of a multi-object scene is put under it. One workgroup scans the
// bottom vertices of the mesh, sampling the height map where it lives (on the device).
struct LU {
    n: u32,
    scale: f32,
    level: f32,
    ymin0: f32,
}
@group(0) @binding(0) var<uniform> u: LU;
@group(0) @binding(1) var<storage, read> verts: array<vec4<f32>>; // y, u, v, dy * fade
@group(0) @binding(2) var height_tex: texture_2d<f32>;
@group(0) @binding(3) var samp: sampler;
@group(0) @binding(4) var<storage, read_write> out: array<f32>;

var<workgroup> part: array<f32, 256>;

@compute @workgroup_size(256, 1, 1)
fn main(@builtin(local_invocation_index) li: u32) {
    var m = u.ymin0;
    var i = li;
    loop {
        if (i >= u.n) {
            break;
        }
        let v = verts[i];
        let h = textureSampleLevel(height_tex, samp, v.yz, 0.0).r;
        m = min(m, v.x + v.w * (h - u.level) * u.scale);
        i = i + 256u;
    }
    part[li] = m;
    workgroupBarrier();
    var s = 128u;
    loop {
        if (s == 0u) {
            break;
        }
        if (li < s) {
            part[li] = min(part[li], part[li + s]);
        }
        workgroupBarrier();
        s = s / 2u;
    }
    if (li == 0u) {
        out[0] = part[0];
    }
}
