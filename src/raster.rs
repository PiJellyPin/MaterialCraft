// SPDX-License-Identifier: MIT OR Apache-2.0
//! The software renderer of the 3D view: rasterised primary visibility, then per-pixel PBR shading with the light rig,
//! shadow rays, image based lighting, channel isolation and the post-processing chain. The OpenGL viewport follows the
//! same maths (`gpu3d.rs`), the path tracer (`pt.rs`) reuses the material sampling.
use crate::bvh::{Bvh, Filter};
use crate::hdri::{self, Env};
use crate::meshio::Tri;
use crate::objects::{self, SceneMesh};
use crate::post::{self, PostCtx};
use crate::render3d::*;
use crate::scene::{self, Background, Channel, Material};
use crate::tex::{lum, Tex};
use crate::v3::*;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

pub(crate) fn lin(c: f32) -> f32 {
    c.max(0.0).powf(2.2)
}

/// The environment as the scene sees it: rotated and scaled.
pub struct EnvView<'a> {
    pub env: &'a Env,
    cs: f32,
    sn: f32,
    /// brightness of the background image
    pub bg_gain: f32,
    /// brightness of the lighting (background gain times the view's ambient multiplier)
    pub gain: f32,
}

impl<'a> EnvView<'a> {
    pub fn new(env: &'a Env, v: &View) -> EnvView<'a> {
        let (sn, cs) = v.env.rotation.sin_cos();
        let bg = v.env.exposure.exp2();
        EnvView { env, cs, sn, bg_gain: bg, gain: bg * v.ambient }
    }
    /// world direction -> direction in the environment image
    pub fn to_env(&self, d: V) -> V {
        [d[0] * self.cs - d[2] * self.sn, d[1], d[0] * self.sn + d[2] * self.cs]
    }
    pub fn from_env(&self, d: V) -> V {
        [d[0] * self.cs + d[2] * self.sn, d[1], -d[0] * self.sn + d[2] * self.cs]
    }
    pub fn irradiance(&self, n: V) -> V {
        mul(self.env.irradiance(self.to_env(n)), self.gain)
    }
    pub fn specular(&self, d: V, rough: f32) -> V {
        mul(self.env.specular(self.to_env(d), rough), self.gain)
    }
    pub fn background(&self, d: V, blur: f32) -> V {
        let lod = blur.clamp(0.0, 1.0) * (self.env.n_levels() - 1) as f32;
        mul(self.env.lod(self.to_env(d), lod), self.bg_gain)
    }
}

/// Resolve the environment of a render: the one in `Maps`, else the view's own settings (preset or file).
pub fn env_of(m: &Maps, v: &View) -> Arc<Env> {
    let _ = m;
    hdri::load_env(&v.env.preset, v.env.file.as_deref()).unwrap_or_else(|_| hdri::load_env("studio", None).expect("built-in environment"))
}

// ------------------------------------------------------------------------------------------------ surface
/// Everything the shaders need to know about a point of the scene.
pub(crate) struct Surface {
    pub p: V,
    /// geometric normal facing the viewer
    pub ng: V,
    /// shading normal (after the normal map), facing the viewer
    pub n: V,
    pub albedo: V,
    pub rough: f32,
    pub metal: f32,
    pub ao: f32,
    pub emissive: V,
    pub uv: [f32; 2],
    pub base_raw: V,
    pub nm_raw: V,
    pub height: f32,
    pub emis_raw: V,
}

struct PSurf {
    uv: [f32; 2],
    su: f32,
    sv: f32,
}

/// Outward relief mapping. The height field is a slab of thickness `depth` that sits on the surface: height `level`
/// is flush with the polygon, brighter pixels rise out of it. The view ray enters the slab at its top and is marched
/// down until it meets the height field. Returns (uv, fraction 0..1 of how deep below the top the hit is).
fn parallax(hm: &Tex, s: &PSurf, vt: V, depth: f32, level: f32) -> ([f32; 2], f32) {
    let vz = vt[2].max(0.2);
    let du = -vt[0] / vz / s.su;
    let dv = vt[1] / vz / s.sv; // uv v runs opposite to the bitangent
    let top = (1.0 - level.clamp(0.0, 1.0)) * depth;
    let at = |d: f32| -> (f32, [f32; 2]) {
        let uv = [s.uv[0] + du * (d - top), s.uv[1] + dv * (d - top)];
        ((1.0 - lum(hm.sample(uv[0], uv[1]))) * depth - d, uv)
    };
    let steps = 48;
    let mut prev_d = 0.0f32;
    if at(0.0).0 <= 0.0 {
        return (at(0.0).1, 0.0);
    }
    for i in 1..=steps {
        let d = depth * i as f32 / steps as f32;
        let (f, _) = at(d);
        if f <= 0.0 {
            let (mut lo, mut hi) = (prev_d, d);
            for _ in 0..5 {
                let mid = 0.5 * (lo + hi);
                if at(mid).0 > 0.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let dh = 0.5 * (lo + hi);
            return (at(dh).1, (dh / depth).clamp(0.0, 1.0));
        }
        prev_d = d;
    }
    (at(depth).1, 1.0)
}

/// Does the material carry an opacity (an opacity map, or a base colour with some alpha below one)?
pub(crate) fn has_opacity(m: &Maps) -> bool {
    m.opacity.is_some() || m.base.px.iter().any(|p| p[3] < 0.999)
}

/// Opacity of the material at a point of a triangle: the opacity map, else the base colour's alpha (fixed materials
/// are opaque).
pub(crate) fn opacity(m: &Maps, sm: &SceneMesh, tri: usize, b1: f32, b2: f32) -> f32 {
    if !matches!(sm.objects[sm.mesh.object_of(tri)].material, Material::Graph) {
        return 1.0;
    }
    let t = sm.mesh.tris[tri];
    let (i0, i1, i2) = (t[0] as usize, t[1] as usize, t[2] as usize);
    let b0 = 1.0 - b1 - b2;
    let uv = &sm.mesh.uv;
    let (u, v) = (b0 * uv[i0][0] + b1 * uv[i1][0] + b2 * uv[i2][0], b0 * uv[i0][1] + b1 * uv[i1][1] + b2 * uv[i2][1]);
    let a = match m.opacity {
        Some(o) => lum(o.sample(u, v)),
        None => m.base.sample(u, v)[3],
    };
    if a.is_nan() {
        1.0
    } else {
        a.clamp(0.0, 1.0)
    }
}

/// Ordered dither threshold of pixel (x, y) (4 x 4 Bayer, as the GPU viewer): partial opacity becomes a screen-door
/// pattern.
pub(crate) fn bayer(x: usize, y: usize) -> f32 {
    const M: [f32; 16] = [0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0];
    (M[(y % 4) * 4 + x % 4] + 0.5) / 16.0
}

/// Sample the material at a point of a triangle. `vv` points from the surface towards the viewer.
pub(crate) fn surface(m: &Maps, v: &View, sm: &SceneMesh, tri: usize, b1: f32, b2: f32, vv: V, use_parallax: bool) -> Surface {
    let mesh = &sm.mesh;
    let t = mesh.tris[tri];
    let (i0, i1, i2) = (t[0] as usize, t[1] as usize, t[2] as usize);
    let b0 = 1.0 - b1 - b2;
    let mix3 = |a: V, b: V, c: V| add(add(mul(a, b0), mul(b, b1)), mul(c, b2));
    let p = mix3(mesh.pos[i0], mesh.pos[i1], mesh.pos[i2]);
    let n = norm(mix3(mesh.nrm[i0], mesh.nrm[i1], mesh.nrm[i2]));
    let t0 = mix3(mesh.tan[i0], mesh.tan[i1], mesh.tan[i2]);
    let tg = norm(sub(t0, mul(n, dot(t0, n))));
    let bt = cross(n, tg);
    let uv = [b0 * mesh.uv[i0][0] + b1 * mesh.uv[i1][0] + b2 * mesh.uv[i2][0], b0 * mesh.uv[i0][1] + b1 * mesh.uv[i1][1] + b2 * mesh.uv[i2][1]];
    let su = (b0 * mesh.su[i0] + b1 * mesh.su[i1] + b2 * mesh.su[i2]).max(1e-4);
    let sv = (b0 * mesh.sv[i0] + b1 * mesh.sv[i1] + b2 * mesh.sv[i2]).max(1e-4);
    // triangle winding is outward, so the true face normal tells us whether we look at the back of the surface
    let fnrm = norm(cross(sub(mesh.pos[i1], mesh.pos[i0]), sub(mesh.pos[i2], mesh.pos[i0])));
    let back = dot(fnrm, vv) < 0.0;
    let flip = if back { -1.0 } else { 1.0 };
    let ng = mul(fnrm, flip);
    let obj = &sm.objects[mesh.object_of(tri)];
    match obj.material {
        Material::Fixed(c, r, mt) => Surface {
            p,
            ng,
            n: mul(n, flip),
            albedo: c,
            rough: r.clamp(0.04, 1.0),
            metal: mt,
            ao: 1.0,
            emissive: [0.0; 3],
            uv,
            base_raw: [c[0].powf(1.0 / 2.2), c[1].powf(1.0 / 2.2), c[2].powf(1.0 / 2.2)],
            nm_raw: [0.5, 0.5, 1.0],
            height: 0.5,
            emis_raw: [0.0; 3],
        },
        Material::Graph => {
            let mut uv = uv;
            let mut cavity = 1.0;
            if let (Some(hm), true, true) = (m.height, use_parallax && v.relief == RELIEF_PARALLAX, v.height_scale > 0.0) {
                let vt = [dot(vv, tg), dot(vv, bt), dot(vv, n) * flip];
                let (nuv, d) = parallax(hm, &PSurf { uv, su, sv }, vt, v.height_scale, v.height_level);
                uv = nuv;
                cavity = 1.0 - 0.45 * d;
            }
            let base = m.base.sample(uv[0], uv[1]);
            let rough = lum(m.rough.sample(uv[0], uv[1])).clamp(0.04, 1.0);
            let metal = lum(m.metal.sample(uv[0], uv[1])).clamp(0.0, 1.0);
            let ao = lum(m.ao.sample(uv[0], uv[1])).clamp(0.0, 1.0) * cavity;
            let nm = m.normal.sample(uv[0], uv[1]);
            let amt = v.normal_amt.clamp(0.0, 1.0);
            let mut tn = [(nm[0] * 2.0 - 1.0) * amt, (nm[1] * 2.0 - 1.0) * amt, (nm[2] * 2.0 - 1.0).max(0.05)];
            let hv = m.height.map(|hm| lum(hm.sample(uv[0], uv[1])));
            if let (Some(hm), true) = (m.height, v.relief == RELIEF_BUMP && v.bump > 0.0) {
                let e = 1.0 / hm.w.max(1) as f32;
                let hs = |du: f32, dv: f32| lum(hm.sample(uv[0] + du, uv[1] + dv));
                let (dhu, dhv) = ((hs(e, 0.0) - hs(-e, 0.0)) / (2.0 * e), (hs(0.0, e) - hs(0.0, -e)) / (2.0 * e));
                let k = v.bump * 0.12;
                tn[0] -= k * dhu / su;
                tn[1] += k * dhv / sv;
            }
            let tn = norm(tn);
            let ns = norm(add(add(mul(tg, tn[0]), mul(bt, tn[1])), mul(n, tn[2])));
            let emis = m.emissive.map(|e| e.sample(uv[0], uv[1]));
            Surface {
                p,
                ng,
                n: mul(ns, flip),
                albedo: [lin(base[0]), lin(base[1]), lin(base[2])],
                rough,
                metal,
                ao,
                emissive: emis.map(|e| [lin(e[0]), lin(e[1]), lin(e[2])]).unwrap_or([0.0; 3]),
                uv,
                base_raw: [base[0], base[1], base[2]],
                nm_raw: [nm[0], nm[1], nm[2]],
                height: hv.unwrap_or(0.5),
                emis_raw: emis.map(|e| [e[0], e[1], e[2]]).unwrap_or([0.0; 3]),
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------ shading
/// Analytic fit of the split-sum environment BRDF (scale, bias) by Karis.
pub(crate) fn env_brdf(ndv: f32, rough: f32) -> (f32, f32) {
    let c0 = [-1.0f32, -0.0275, -0.572, 0.022];
    let c1 = [1.0f32, 0.0425, 1.04, -0.04];
    let r = [rough * c0[0] + c1[0], rough * c0[1] + c1[1], rough * c0[2] + c1[2], rough * c0[3] + c1[3]];
    let a004 = (r[0] * r[0]).min((-9.28 * ndv).exp2()) * r[0] + r[1];
    (-1.04 * a004 + r[2], 1.04 * a004 + r[3])
}

/// Specular occlusion from the ambient occlusion (Lagarde).
pub(crate) fn spec_occlusion(ndv: f32, ao: f32, rough: f32) -> f32 {
    ((ndv + ao).powf((-16.0 * rough - 1.0).exp2()) - 1.0 + ao).clamp(0.0, 1.0)
}

/// Radiance reflected towards `vv` from a light arriving from `l` with irradiance `e` (already includes the cosine).
pub(crate) fn direct(s: &Surface, vv: V, l: V, e: V) -> V {
    let n = s.n;
    let ndl = dot(n, l).max(0.0);
    if ndl <= 0.0 {
        return [0.0; 3];
    }
    let ndv = dot(n, vv).max(1e-3);
    let hv = norm(add(l, vv));
    let (ndh, vdh) = (dot(n, hv).max(0.0), dot(vv, hv).max(0.0));
    let a = s.rough * s.rough;
    let a2 = a * a;
    let d = a2 / (PI * (ndh * ndh * (a2 - 1.0) + 1.0).powi(2)).max(1e-20);
    let k = (s.rough + 1.0).powi(2) / 8.0;
    let g = (ndl / (ndl * (1.0 - k) + k)) * (ndv / (ndv * (1.0 - k) + k));
    let fr = (1.0 - vdh).powi(5);
    let mut o = [0.0; 3];
    for c in 0..3 {
        let f0 = 0.04 * (1.0 - s.metal) + s.albedo[c] * s.metal;
        let f = f0 + (1.0 - f0) * fr;
        let spec = d * g * f / (4.0 * ndl.max(1e-3) * ndv);
        let diff = s.albedo[c] * (1.0 - s.metal) * (1.0 - f) / PI;
        o[c] = (diff + spec) * e[c] * ndl;
    }
    o
}

/// Light from the environment: diffuse irradiance and prefiltered reflection.
pub(crate) fn ambient(s: &Surface, vv: V, ev: &EnvView) -> V {
    let ndv = dot(s.n, vv).max(1e-3);
    let refl = norm(sub(mul(s.n, 2.0 * dot(s.n, vv)), vv));
    let irr = ev.irradiance(s.n);
    let spec_l = ev.specular(refl, s.rough);
    let (sc, bi) = env_brdf(ndv, s.rough);
    let so = spec_occlusion(ndv, s.ao, s.rough);
    let mut o = [0.0; 3];
    for c in 0..3 {
        let f0 = 0.04 * (1.0 - s.metal) + s.albedo[c] * s.metal;
        let diff = s.albedo[c] * (1.0 - s.metal) * irr[c] * s.ao * (1.0 - f0 * 0.0);
        let spec = spec_l[c] * (f0 * sc + bi) * so;
        o[c] = diff + spec;
    }
    o
}

/// Shadow geometry: the scene's triangles that are actually drawn.
pub(crate) fn shadow_bvh(sm: &SceneMesh, eye: Option<V>) -> Bvh {
    scene_bvh(sm, eye).0
}

/// A BVH over the triangles of the scene that are drawn, and for every BVH triangle its index in the scene mesh.
pub(crate) fn scene_bvh(sm: &SceneMesh, eye: Option<V>) -> (Bvh, Vec<u32>) {
    let m = &sm.mesh;
    let mut tris = Vec::with_capacity(m.tris.len());
    let mut ids = Vec::with_capacity(m.tris.len());
    for (ti, t) in m.tris.iter().enumerate() {
        let (i0, i1, i2) = (t[0] as usize, t[1] as usize, t[2] as usize);
        if let Some(eye) = eye {
            if m.cull[m.object_of(ti)] {
                let nn = add(add(m.base_nrm[i0], m.base_nrm[i1]), m.base_nrm[i2]);
                if dot(nn, sub(eye, m.pos[i0])) <= 0.0 {
                    continue;
                }
            }
        }
        ids.push(ti as u32);
        tris.push(Tri {
            p: [m.pos[i0], m.pos[i1], m.pos[i2]],
            n: [m.nrm[i0], m.nrm[i1], m.nrm[i2]],
            uv: [[0.0; 2]; 3],
            uv1: [[0.0; 2]; 3],
            col: [[1.0; 3]; 3],
            object: m.object_of(ti) as u32,
            material: 0,
            group: 0,
            smooth: 0,
        });
    }
    (Bvh::build(&tris), ids)
}

/// Fraction (0..1) of a light that reaches `p`. `soft` is the light's softness; `seed` decorrelates the pixels.
pub(crate) fn visibility(bvh: &Bvh, p: V, ng: V, dir: V, dist: f32, soft: f32, seed: f32) -> f32 {
    let o = add(p, mul(ng, 0.003));
    let tmax = if dist.is_finite() { (dist - 0.01).max(0.01) } else { 1e30 };
    if soft < 0.01 {
        return if bvh.any(o, dir, 1e-4, tmax, &Filter::ALL) { 0.0 } else { 1.0 };
    }
    let (t, b) = hdri::basis(dir);
    let n = 10;
    let mut vis = 0.0;
    for i in 0..n {
        let r = ((i as f32 + 0.5) / n as f32).sqrt() * soft * 0.18;
        let a = i as f32 * 2.399_963 + seed * TAU;
        let d = norm(add(dir, add(mul(t, r * a.cos()), mul(b, r * a.sin()))));
        if dot(d, ng) <= 0.0 {
            continue;
        }
        if !bvh.any(o, d, 1e-4, tmax, &Filter::ALL) {
            vis += 1.0;
        }
    }
    vis / n as f32
}

/// Gradient behind the model (linear radiance, tone maps to about 0.12 .. 0.2 on screen).
pub(crate) fn gradient_bg(py: f32) -> V {
    let g = 0.044 + 0.026 * (1.0 - py);
    [g, g, g * 1.05]
}

/// Radiance behind the model in direction `dir` (screen row `py`, 0 at the top, for the gradient).
pub(crate) fn background(v: &View, ev: &EnvView, dir: V, py: f32) -> V {
    match v.env.background {
        Background::Gradient => gradient_bg(py),
        Background::Color => [lin(v.env.bg_color[0]), lin(v.env.bg_color[1]), lin(v.env.bg_color[2])],
        Background::Environment => mul(ev.background(dir, v.env.blur), v.exposure),
    }
}

/// Colour shown by a channel view, display referred.
fn channel_color(s: &Surface, ch: Channel) -> V {
    let g = |x: f32| [x, x, x];
    match ch {
        Channel::BaseColor => s.base_raw,
        Channel::Roughness => g(s.rough),
        Channel::Metallic => g(s.metal),
        Channel::Normal => s.nm_raw,
        Channel::Ao => g(s.ao),
        Channel::Height => g(s.height),
        Channel::Emissive => s.emis_raw,
        Channel::WorldNormal => [s.n[0] * 0.5 + 0.5, s.n[1] * 0.5 + 0.5, s.n[2] * 0.5 + 0.5],
        Channel::Uv => {
            let (fu, fv) = (s.uv[0] - s.uv[0].floor(), s.uv[1] - s.uv[1].floor());
            let chk = if ((s.uv[0] * 8.0).floor() as i64 + (s.uv[1] * 8.0).floor() as i64) & 1 == 0 { 1.0 } else { 0.8 };
            [fu * chk, fv * chk, 0.2 * chk]
        }
        Channel::Lit => [0.0; 3],
    }
}

// ------------------------------------------------------------------------------------------------ render
/// A rendered frame before post-processing.
pub struct Frame {
    /// exposed linear radiance (for channel views: display colours)
    pub hdr: Tex,
    /// view-space depth per pixel, infinity for the background
    pub depth: Vec<f32>,
    pub ctx: PostCtx,
    pub(crate) frags: Vec<Frag>,
}

fn camera(v: &View, w: usize, h: usize) -> Cam {
    let c = v.cam();
    Cam { eye: c.eye, fwd: c.fwd, right: c.right, up: c.up, tf: c.tan_half, aspect: w as f32 / h as f32, w, h }
}

pub fn render_frame(m: &Maps, v: &View, w: usize, h: usize) -> (Frame, SceneMesh) {
    let (w, h) = (w.max(1), h.max(1));
    let cam = camera(v, w, h);
    let cb = v.cam();
    let sm = objects::build_scene(v, if v.relief == RELIEF_DISPLACE { m.height } else { None });
    // transparent texels are not drawn (see `opacity`)
    let cut = |tri: u32, b1: f32, b2: f32, x: usize, y: usize| opacity(m, &sm, tri as usize, b1, b2) >= bayer(x, y);
    let frags = rasterize(&sm.mesh, &cam, if has_opacity(m) { Some(&cut) } else { None });
    let env_arc;
    let env: &Env = match m.env {
        Some(e) => e,
        None => {
            env_arc = env_of(m, v);
            &env_arc
        }
    };
    let ev = EnvView::new(env, v);
    let cf = v.cam_frame();
    let lit = v.channel == Channel::Lit;
    let want_shadows = lit && v.lights.iter().any(|l| l.enabled && l.shadows && l.intensity > 0.0);
    let bvh = if want_shadows { Some(shadow_bvh(&sm, Some(cam.eye))) } else { None };
    let lights: Vec<&scene::Light> = if lit { v.lights.iter().filter(|l| l.enabled && l.intensity > 0.0).collect() } else { vec![] };
    let (tf, aspect) = (cam.tf, cam.aspect);
    let mut depth = vec![f32::INFINITY; w * h];
    for (i, f) in frags.iter().enumerate() {
        if f.tri != NO_TRI {
            depth[i] = f.z;
        }
    }
    let hdr = Tex::from_fn(w, h, |px, py| {
        let (ix, iy) = (((px * w as f32) as usize).min(w - 1), ((py * h as f32) as usize).min(h - 1));
        let x = (px * 2.0 - 1.0) * aspect * tf;
        let y = (1.0 - py * 2.0) * tf;
        let dir = norm(add(add(cam.fwd, mul(cam.right, x)), mul(cam.up, y)));
        let fr = frags[iy * w + ix];
        if fr.tri == NO_TRI {
            let c = if !lit {
                // channel views keep the neutral backdrop
                let g = gradient_bg(py);
                [g[0].powf(1.0 / 2.2) * 1.0, g[1].powf(1.0 / 2.2), g[2].powf(1.0 / 2.2)]
            } else {
                background(v, &ev, dir, py)
            };
            return [c[0], c[1], c[2], 1.0];
        }
        let vv = norm(mul(dir, -1.0));
        let s = surface(m, v, &sm, fr.tri as usize, fr.b1, fr.b2, vv, true);
        if !lit {
            let c = channel_color(&s, v.channel);
            return [c[0], c[1], c[2], 1.0];
        }
        let mut out = ambient(&s, vv, &ev);
        let seed = post::ign(ix as f32, iy as f32);
        for l in &lights {
            let (ld, e, dist) = scene::light_at(l, &cf, s.p);
            if dot(s.n, ld) <= 0.0 || (e[0] + e[1] + e[2]) <= 0.0 {
                continue;
            }
            let vis = match (&bvh, l.shadows) {
                (Some(b), true) => visibility(b, s.p, s.ng, ld, dist, l.softness, seed),
                _ => 1.0,
            };
            if vis <= 0.0 {
                continue;
            }
            let c = direct(&s, vv, ld, mul(e, vis));
            out = add(out, c);
        }
        out = add(out, s.emissive);
        out = mul(out, v.exposure);
        [out[0], out[1], out[2], 1.0]
    });
    let ctx = PostCtx { tan_half: tf, aspect, centre_dist: cb.dist };
    (Frame { hdr, depth, ctx, frags }, sm)
}

/// Software render of the view: display-referred RGB.
pub fn render(m: &Maps, v: &View, w: usize, h: usize) -> Tex {
    let (fr, _sm) = render_frame(m, v, w, h);
    let (w, h) = (fr.hdr.w, fr.hdr.h);
    let mut out = if v.channel == Channel::Lit {
        post::apply(&fr.hdr, &fr.depth, &fr.ctx, &v.post)
    } else {
        fr.hdr.clone()
    };
    if v.wireframe {
        for iy in 0..h {
            for ix in 0..w {
                let f = fr.frags[iy * w + ix];
                if f.tri == NO_TRI {
                    continue;
                }
                let cov = wire_coverage(&fr.frags, w, h, ix, iy, f);
                if cov > 0.0 {
                    let line = [1.0, 0.62, 0.12];
                    let p = &mut out.px[iy * w + ix];
                    for c in 0..3 {
                        p[c] = p[c] * (1.0 - 0.85 * cov) + line[c] * 0.85 * cov;
                    }
                }
            }
        }
    }
    out
}
