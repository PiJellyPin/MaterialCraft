// SPDX-License-Identifier: MIT OR Apache-2.0
// 3D view: background (gradient, colour or the environment image).
struct BU {
    cam_r: vec4<f32>,
    cam_u: vec4<f32>,
    cam_f: vec4<f32>,
    col: vec4<f32>,      // linear background colour
    p: vec4<f32>,        // cos rot, sin rot, gain, lod
    q: vec4<f32>,        // tan_half, aspect, 0, 0
    m: vec4<u32>,        // mode (0 gradient, 1 colour, 2 environment), channel
}
@group(0) @binding(0) var<uniform> u: BU;
@group(0) @binding(1) var env_tex: texture_2d<f32>;
@group(0) @binding(2) var env_samp: sampler;

const PI: f32 = 3.14159265;

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) vp: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VOut {
    let p = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    var o: VOut;
    o.vp = p * 2.0 - 1.0;
    o.clip = vec4<f32>(p * 2.0 - 1.0, 1.0, 1.0);
    return o;
}

fn shade(in: VOut) -> vec4<f32> {
    let t = in.vp.y * 0.5 + 0.5; // 1 at the top
    var grad = vec3<f32>(0.044 + 0.026 * t);
    grad.z = grad.z * 1.05;
    if (u.m.y != 0u) {
        return vec4<f32>(pow(grad, vec3<f32>(1.0 / 2.2)), 1.0);
    }
    var c: vec3<f32>;
    if (u.m.x == 0u) {
        c = grad;
    } else if (u.m.x == 1u) {
        c = u.col.xyz;
    } else {
        let d = normalize(u.cam_f.xyz + u.cam_r.xyz * (in.vp.x * u.q.y * u.q.x) + u.cam_u.xyz * (in.vp.y * u.q.x));
        let e = vec3<f32>(d.x * u.p.x - d.z * u.p.y, d.y, d.x * u.p.y + d.z * u.p.x);
        let uv = vec2<f32>(0.5 + atan2(e.x, -e.z) / (2.0 * PI), acos(clamp(e.y, -1.0, 1.0)) / PI);
        c = textureSampleLevel(env_tex, env_samp, uv, u.p.w).rgb * u.p.z;
    }
    return vec4<f32>(min(c, vec3<f32>(60000.0)), 1.0);
}

// depth stored as 8-bit channels (float colour targets are not available on every adapter)
fn enc_z(z0: f32) -> vec4<f32> {
    let z = clamp(z0, 0.0, 0.99999994);
    var e = fract(z * vec4<f32>(1.0, 255.0, 65025.0, 16581375.0));
    e = e - e.yzww * vec4<f32>(1.0 / 255.0, 1.0 / 255.0, 1.0 / 255.0, 0.0);
    return e;
}

struct FOut {
    @location(0) c: vec4<f32>,
    @location(1) z: vec4<f32>,
}

@fragment
fn fs_main(in: VOut) -> FOut {
    var o: FOut;
    o.c = shade(in);
    o.z = enc_z(1.0);
    return o;
}
