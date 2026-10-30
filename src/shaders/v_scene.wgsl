// SPDX-License-Identifier: MIT OR Apache-2.0
// 3D view: scene pass. Vertex stage displaces along the height map, fragment stage is GGX PBR with normal mapping,
// parallax relief, light rig with shadow maps and image based lighting. Port of the former GLSL shaders.
struct SU {
    vp: mat4x4<f32>,
    eye: vec4<f32>,      // xyz eye, w exposure
    a: vec4<f32>,        // normal_amt, bump, depth, level
    b: vec4<f32>,        // texel, disp, vtx_lod, lod_scale
    c: vec4<u32>,        // mode, has_height, has_emis, self_shadow
    d: vec4<u32>,        // channel, n_lights, 0, 0
    env: vec4<f32>,      // cos rot, sin rot, gain, levels
    obj_a: array<vec4<f32>, 8>, // x: 0 graph / 1 fixed, y rough, z metal, w cull
    obj_c: array<vec4<f32>, 8>, // fixed base colour (linear)
    l_axis: array<vec4<f32>, 6>, // xyz direction towards the light, w distance
    l_info: array<vec4<f32>, 6>, // kind, cos outer, cos inner, softness
    l_col: array<vec4<f32>, 6>,  // colour x intensity, w shadow layer (-1 none)
    l_vp: array<mat4x4<f32>, 3>,
    sh: array<vec4<f32>, 9>,
}
@group(0) @binding(0) var<uniform> u: SU;
@group(0) @binding(1) var maps_samp: sampler;
@group(0) @binding(2) var env_samp: sampler;
@group(0) @binding(3) var base_tex: texture_2d<f32>;
@group(0) @binding(4) var normal_tex: texture_2d<f32>;
@group(0) @binding(5) var rma_tex: texture_2d<f32>;
@group(0) @binding(6) var emis_tex: texture_2d<f32>;
@group(0) @binding(7) var height_tex: texture_2d<f32>;
@group(0) @binding(8) var env_tex: texture_2d<f32>;
@group(0) @binding(9) var shadow_tex: texture_depth_2d_array;
@group(0) @binding(10) var shadow_samp: sampler_comparison;

const PI: f32 = 3.14159265;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) tan: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) misc: vec3<f32>,
    @location(5) disp: vec3<f32>,
    @location(6) obj: f32,
}
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) pos: vec3<f32>,
    @location(1) n: vec3<f32>,
    @location(2) t: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) scale: vec2<f32>,
    @location(5) @interpolate(flat) obj: u32,
}

@vertex
fn vs_main(v: VIn) -> VOut {
    let oi = u32(v.obj + 0.5);
    var p = v.pos;
    if (u.b.y != 0.0 && u.obj_a[oi].x < 0.5) {
        let h = textureSampleLevel(height_tex, maps_samp, v.uv, u.b.z).r;
        p = p + v.disp * (h - u.a.w) * u.b.y * v.misc.z;
    }
    var o: VOut;
    o.clip = u.vp * vec4<f32>(p, 1.0);
    o.pos = p;
    o.n = v.nrm;
    o.t = v.tan;
    o.uv = v.uv;
    o.scale = v.misc.xy;
    o.obj = oi;
    return o;
}

var<private> gdx: vec2<f32>;
var<private> gdy: vec2<f32>;

// ---------------------------------------------------------------- environment
fn env_in(d: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(d.x * u.env.x - d.z * u.env.y, d.y, d.x * u.env.y + d.z * u.env.x);
}
fn env_lod(d: vec3<f32>, lod: f32) -> vec3<f32> {
    let e = env_in(d);
    let uv = vec2<f32>(0.5 + atan2(e.x, -e.z) / (2.0 * PI), acos(clamp(e.y, -1.0, 1.0)) / PI);
    return textureSampleLevel(env_tex, env_samp, uv, lod).rgb;
}
fn env_spec(d: vec3<f32>, rough: f32) -> vec3<f32> {
    return env_lod(d, clamp(rough, 0.0, 1.0) * (u.env.w - 1.0)) * u.env.z;
}
fn env_irr(n: vec3<f32>) -> vec3<f32> {
    let d = env_in(n);
    let x = d.x;
    let y = d.y;
    let z = d.z;
    var b = array<f32, 9>(0.282095, 0.488603 * y, 0.488603 * z, 0.488603 * x, 1.092548 * x * y, 1.092548 * y * z, 0.315392 * (3.0 * z * z - 1.0), 1.092548 * x * z, 0.546274 * (x * x - y * y));
    var o = vec3<f32>(0.0);
    for (var i = 0; i < 9; i = i + 1) {
        o = o + u.sh[i].xyz * b[i];
    }
    return max(o, vec3<f32>(0.0)) * u.env.z;
}
fn env_brdf_ab(rough: f32, ndv: f32) -> vec2<f32> {
    let c0 = vec4<f32>(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let r = rough * c0 + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * ndv)) * r.x + r.y;
    return vec2<f32>(-1.04, 1.04) * a004 + r.zw;
}

// ---------------------------------------------------------------- lights
struct LightS {
    l: vec3<f32>,
    e: vec3<f32>,
}
fn light_at(i: u32, p: vec3<f32>) -> LightS {
    let ax = u.l_axis[i];
    let info = u.l_info[i];
    let c = u.l_col[i].xyz;
    var o: LightS;
    if (info.x < 0.5) {
        o.l = ax.xyz;
        o.e = c;
        return o;
    }
    let v = ax.xyz * ax.w - p;
    let d2 = max(dot(v, v), 1e-4);
    let d = sqrt(d2);
    o.l = v / d;
    var k = 1.0 / d2;
    if (info.x > 1.5) {
        let cd = dot(o.l, ax.xyz);
        let t = clamp((cd - info.y) / max(info.z - info.y, 1e-4), 0.0, 1.0);
        k = k * t * t * (3.0 - 2.0 * t);
    }
    o.e = c * k;
    return o;
}

fn ign(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715))));
}

fn shadow_at(li: u32, p: vec3<f32>, ng: vec3<f32>, soft: f32, frag: vec2<f32>) -> f32 {
    let layer = i32(round(u.l_col[li].w));
    if (layer < 0) {
        return 1.0;
    }
    let q = u.l_vp[layer] * vec4<f32>(p + ng * 0.012, 1.0);
    let ndc = q.xyz / q.w;
    let s = vec3<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5, ndc.z);
    if (s.x < 0.0 || s.x > 1.0 || s.y < 0.0 || s.y > 1.0 || s.z > 1.0) {
        return 1.0;
    }
    let dim = vec2<f32>(textureDimensions(shadow_tex));
    let rad = (1.5 + soft * 10.0) / dim.x;
    let rot = ign(frag) * 2.0 * PI;
    var lit = 0.0;
    for (var i = 0; i < 16; i = i + 1) {
        let f = (f32(i) + 0.5) / 16.0;
        let a = f32(i) * 2.399963 + rot;
        let o = vec2<f32>(cos(a), sin(a)) * sqrt(f) * rad;
        lit = lit + textureSampleCompareLevel(shadow_tex, shadow_samp, clamp(s.xy + o, vec2<f32>(0.0), vec2<f32>(1.0)), layer, s.z - 0.0015);
    }
    return lit / 16.0;
}

// ---------------------------------------------------------------- relief
fn h_at(uv: vec2<f32>) -> f32 {
    return textureSampleGrad(height_tex, maps_samp, uv, gdx, gdy).r;
}

struct PomR {
    uv: vec2<f32>,
    cav: f32,
}
fn pom(uv: vec2<f32>, vt: vec3<f32>, scale: vec2<f32>) -> PomR {
    let depth = u.a.z;
    let vz = max(vt.z, 0.2);
    let d = vec2<f32>(-vt.x / vz / scale.x, vt.y / vz / scale.y);
    let top = (1.0 - clamp(u.a.w, 0.0, 1.0)) * depth;
    var r: PomR;
    r.cav = 0.0;
    let f0 = (1.0 - h_at(uv + d * (0.0 - top))) * depth;
    if (f0 <= 0.0) {
        r.uv = uv + d * (0.0 - top);
        return r;
    }
    var prev_d = 0.0;
    for (var i = 1; i <= 64; i = i + 1) {
        let dd = depth * f32(i) / 64.0;
        let f = (1.0 - h_at(uv + d * (dd - top))) * depth - dd;
        if (f <= 0.0) {
            var lo = prev_d;
            var hi = dd;
            for (var k = 0; k < 6; k = k + 1) {
                let mid = 0.5 * (lo + hi);
                let fm = (1.0 - h_at(uv + d * (mid - top))) * depth - mid;
                if (fm > 0.0) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let dh = 0.5 * (lo + hi);
            r.cav = clamp(dh / depth, 0.0, 1.0);
            r.uv = uv + d * (dh - top);
            return r;
        }
        prev_d = dd;
    }
    r.cav = 1.0;
    r.uv = uv + d * (depth - top);
    return r;
}

fn hshadow(uv: vec2<f32>, lt: vec3<f32>, scale: vec2<f32>, frag: vec2<f32>) -> f32 {
    if (lt.z <= 0.02) {
        return 1.0;
    }
    let depth = u.a.z;
    let level = u.a.w;
    let h0 = (h_at(uv) - level) * depth + 0.01 * depth;
    let slope = lt.z / max(length(lt.xy), 1e-3);
    let hd = lt.xy / max(length(lt.xy), 1e-4);
    let max_s = ((1.0 - level) * depth - h0) / slope;
    if (max_s <= 0.0) {
        return 1.0;
    }
    var res = 1.0;
    let jit = ign(frag);
    for (var i = 1; i <= 32; i = i + 1) {
        let s = max_s * (f32(i) - 1.0 + jit) / 32.0;
        let u2 = uv + vec2<f32>(hd.x / scale.x, -hd.y / scale.y) * s;
        let hs = (h_at(u2) - level) * depth;
        let ray = h0 + s * slope;
        res = min(res, 1.0 - smoothstep(0.0, 0.10 * depth + 1e-5, hs - ray));
    }
    return mix(0.55, 1.0, res);
}

fn shade(in: VOut, front: bool) -> vec4<f32> {
    // derivatives first, while the control flow is still uniform
    let d_uv_x = dpdx(in.uv);
    let d_uv_y = dpdy(in.uv);
    gdx = d_uv_x * u.b.w;
    gdy = d_uv_y * u.b.w;
    let frag = in.clip.xy;
    let oa = u.obj_a[in.obj];
    let fixed_mat = oa.x > 0.5;
    let N = normalize(in.n);
    // shapes seen from the inside: walls that face away from the camera are not drawn
    if (oa.w > 0.5 && dot(N, u.eye.xyz - in.pos) < 0.0) {
        discard;
    }
    let T = normalize(in.t - N * dot(in.t, N));
    let B = cross(N, T);
    let V = normalize(u.eye.xyz - in.pos);
    let back = !front;
    var flip = 1.0;
    if (back) {
        flip = -1.0;
    }
    var uv = in.uv;
    var cav = 1.0;
    let has_h = u.c.y == 1u && u.a.z > 0.0 && !fixed_mat;
    let mode = u.c.x;

    var albedo: vec3<f32>;
    var rough: f32;
    var metal: f32;
    var ao: f32;
    var nm = vec3<f32>(0.5, 0.5, 1.0);
    var n: vec3<f32>;
    var emis = vec3<f32>(0.0);
    var base_raw: vec3<f32>;
    if (fixed_mat) {
        albedo = u.obj_c[in.obj].xyz;
        rough = clamp(oa.y, 0.04, 1.0);
        metal = oa.z;
        ao = 1.0;
        n = N * flip;
        base_raw = pow(albedo, vec3<f32>(1.0 / 2.2));
    } else {
        if (mode == 2u && has_h) {
            let vt = vec3<f32>(dot(V, T), dot(V, B), dot(V, N) * flip);
            let pr = pom(uv, vt, in.scale);
            uv = pr.uv;
            cav = 1.0 - 0.45 * pr.cav;
        }
        let bt = textureSampleGrad(base_tex, maps_samp, uv, gdx, gdy).rgb; // sRGB texture: linear here
        albedo = bt;
        base_raw = pow(max(bt, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
        let rma = textureSampleGrad(rma_tex, maps_samp, uv, gdx, gdy).rgb;
        rough = clamp(rma.r, 0.04, 1.0);
        metal = clamp(rma.g, 0.0, 1.0);
        ao = clamp(rma.b, 0.0, 1.0) * cav;
        nm = textureSampleGrad(normal_tex, maps_samp, uv, gdx, gdy).xyz;
        var tn = vec3<f32>((nm.xy * 2.0 - 1.0) * u.a.x, max(nm.z * 2.0 - 1.0, 0.05));
        if (mode == 1u && u.c.y == 1u && u.a.y > 0.0) {
            let e = u.b.x;
            let dhu = (h_at(uv + vec2<f32>(e, 0.0)) - h_at(uv - vec2<f32>(e, 0.0))) / (2.0 * e);
            let dhv = (h_at(uv + vec2<f32>(0.0, e)) - h_at(uv - vec2<f32>(0.0, e))) / (2.0 * e);
            let k = u.a.y * 0.12;
            tn.x = tn.x - k * dhu / in.scale.x;
            tn.y = tn.y + k * dhv / in.scale.y;
        }
        tn = normalize(tn);
        n = normalize(T * tn.x + B * tn.y + N * tn.z) * flip;
        if (u.c.z == 1u) {
            emis = textureSampleGrad(emis_tex, maps_samp, uv, gdx, gdy).rgb;
        }
    }

    // ------------------------------------------------ channel views (display referred)
    let channel = u.d.x;
    if (channel != 0u) {
        var c = vec3<f32>(0.0);
        if (channel == 1u) {
            c = base_raw;
        } else if (channel == 2u) {
            c = vec3<f32>(rough);
        } else if (channel == 3u) {
            c = vec3<f32>(metal);
        } else if (channel == 4u) {
            c = nm;
        } else if (channel == 5u) {
            c = vec3<f32>(ao);
        } else if (channel == 6u) {
            if (fixed_mat) {
                c = vec3<f32>(0.5);
            } else if (u.c.y == 1u) {
                c = vec3<f32>(textureSampleGrad(height_tex, maps_samp, uv, gdx, gdy).r);
            } else {
                c = vec3<f32>(0.5);
            }
        } else if (channel == 7u) {
            if (fixed_mat) {
                c = vec3<f32>(0.0);
            } else {
                c = pow(max(emis, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
            }
        } else if (channel == 8u) {
            c = n * 0.5 + 0.5;
        } else {
            let f = fract(uv);
            var chk = 0.8;
            if ((i32(floor(uv.x * 8.0)) + i32(floor(uv.y * 8.0))) % 2 == 0) {
                chk = 1.0;
            }
            c = vec3<f32>(f.x * chk, f.y * chk, 0.2 * chk);
        }
        return vec4<f32>(c, 1.0);
    }

    let ng = N * flip;
    let ndv = max(dot(n, V), 1e-3);
    let f0 = mix(vec3<f32>(0.04), albedo, metal);
    let a = rough * rough;
    let a2 = a * a;
    let kk = (rough + 1.0) * (rough + 1.0) / 8.0;

    // ------------------------------------------------ image based lighting
    let refl = reflect(-V, n);
    let ab = env_brdf_ab(rough, ndv);
    let spec_occ = clamp(pow(max(ndv + ao, 0.0), exp2(-16.0 * rough - 1.0)) - 1.0 + ao, 0.0, 1.0);
    var col = albedo * (1.0 - metal) * env_irr(n) * ao + env_spec(refl, rough) * (f0 * ab.x + ab.y) * spec_occ;

    // ------------------------------------------------ lights
    let nl = u.d.y;
    for (var i = 0u; i < nl; i = i + 1u) {
        let ls = light_at(i, in.pos);
        let L = ls.l;
        let E = ls.e;
        let ndl = max(dot(n, L), 0.0);
        if (ndl <= 0.0) {
            continue;
        }
        var vis = shadow_at(i, in.pos, ng, u.l_info[i].w, frag);
        if (u.c.w == 1u && i == 0u && has_h && mode == 2u && !back) {
            let lt = vec3<f32>(dot(L, T), dot(L, B), dot(L, N));
            vis = vis * hshadow(uv, lt, in.scale, frag);
        }
        if (vis <= 0.0) {
            continue;
        }
        let hv = normalize(L + V);
        let ndh = max(dot(n, hv), 0.0);
        let vdh = max(dot(V, hv), 0.0);
        let dd = ndh * ndh * (a2 - 1.0) + 1.0;
        let d = a2 / max(PI * dd * dd, 1e-20);
        let g = (ndl / (ndl * (1.0 - kk) + kk)) * (ndv / (ndv * (1.0 - kk) + kk));
        let om = max(1.0 - vdh, 0.0);
        let fr = om * om * om * om * om;
        let f = f0 + (1.0 - f0) * fr;
        let spec = d * g * f / (4.0 * max(ndl, 1e-3) * ndv);
        let diff = albedo * (1.0 - metal) * (1.0 - f) / PI;
        col = col + (diff + spec) * E * ndl * vis;
    }
    col = col + emis;
    col = col * u.eye.w;
    return vec4<f32>(min(col, vec3<f32>(60000.0)), 1.0);
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

// ordered dither threshold of a pixel (4 x 4 Bayer): partial opacity becomes a screen-door pattern, which needs no
// sorting and keeps depth, shadows and the post effects right
fn bayer(p: vec2<f32>) -> f32 {
    var m = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    let x = u32(p.x) % 4u;
    let y = u32(p.y) % 4u;
    return (m[y * 4u + x] + 0.5) / 16.0;
}

@fragment
fn fs_main(in: VOut, @builtin(front_facing) front: bool) -> FOut {
    var o: FOut;
    // opacity: the base colour's alpha (graph materials only)
    let dx = dpdx(in.uv) * u.b.w;
    let dy = dpdy(in.uv) * u.b.w;
    let alpha = textureSampleGrad(base_tex, maps_samp, in.uv, dx, dy).a;
    if (u.obj_a[in.obj].x < 0.5 && alpha < bayer(in.clip.xy)) {
        discard;
    }
    o.c = shade(in, front);
    o.z = enc_z(in.clip.z);
    return o;
}
