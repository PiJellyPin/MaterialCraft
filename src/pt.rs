// SPDX-License-Identifier: MIT OR Apache-2.0
//! Progressive path tracer for the 3D view: unbiased global illumination with the same materials, lights and
//! environment as the raster views. Rays are cast against a BVH of the (displaced) scene; the light rig and the
//! environment are sampled directly (next event estimation, combined with the BSDF samples by multiple importance
//! sampling), the rest of the light arrives through GGX / cosine sampled bounces.
use crate::bvh::{Bvh, Filter};
use crate::hdri::{self, Env, EnvSampler};
use crate::objects::{self, SceneMesh};
use crate::post::{self, PostCtx};
use crate::raster::{self, direct, surface, EnvView, Surface};
use crate::render3d::{Maps, View, RELIEF_DISPLACE};
use crate::scene::{self, Channel};
use crate::tex::Tex;
use crate::v3::*;
use std::f32::consts::PI;
use std::sync::Arc;

/// Longest path (number of surface bounces).
pub const MAX_BOUNCES: u32 = 5;

struct Rng(u32);

impl Rng {
    fn new(a: u32, b: u32, c: u32) -> Rng {
        let mut r = Rng(a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B) ^ c.wrapping_mul(0xC2B2_AE35) ^ 0x1234_5678);
        r.next();
        r.next();
        r
    }
    fn next(&mut self) -> f32 {
        // PCG output function over an LCG state
        self.0 = self.0.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
        let w = ((self.0 >> ((self.0 >> 28) + 4)) ^ self.0).wrapping_mul(277_803_737);
        let r = (w >> 22) ^ w;
        (r >> 8) as f32 * (1.0 / 16_777_216.0)
    }
}

/// Everything that stays the same while samples accumulate.
pub struct PathTracer<'a> {
    maps: &'a Maps<'a>,
    view: View,
    sm: SceneMesh,
    bvh: Bvh,
    ids: Vec<u32>,
    /// the material has transparent texels (see `raster::opacity`)
    alpha: bool,
    env: Arc<Env>,
    sampler: EnvSampler,
    w: usize,
    h: usize,
}

/// Accumulated samples.
pub struct PtState {
    pub w: usize,
    pub h: usize,
    pub samples: u32,
    sum: Vec<[f32; 3]>,
    depth: Vec<f32>,
}

impl PtState {
    pub fn new(w: usize, h: usize) -> PtState {
        PtState { w, h, samples: 0, sum: vec![[0.0; 3]; w * h], depth: vec![f32::INFINITY; w * h] }
    }
}

/// Probability of choosing the specular lobe at a surface.
fn spec_prob(s: &Surface) -> f32 {
    (s.metal + (1.0 - s.metal) * 0.25).clamp(0.15, 0.9)
}

/// Density (per steradian) of sampling direction `l` with the lobe mix of [`sample_bsdf`].
fn bsdf_pdf(s: &Surface, vv: V, l: V) -> f32 {
    let ndl = dot(s.n, l);
    if ndl <= 0.0 {
        return 0.0;
    }
    let h = norm(add(l, vv));
    let (ndh, vdh) = (dot(s.n, h).max(0.0), dot(vv, h).max(1e-4));
    let a = s.rough * s.rough;
    let a2 = a * a;
    let d = a2 / (PI * (ndh * ndh * (a2 - 1.0) + 1.0).powi(2)).max(1e-20);
    let ps = spec_prob(s);
    ps * d * ndh / (4.0 * vdh) + (1.0 - ps) * ndl / PI
}

fn sample_bsdf(s: &Surface, vv: V, r: [f32; 3]) -> Option<V> {
    let (t, b) = hdri::basis(s.n);
    let to_world = |l: V| add(add(mul(t, l[0]), mul(b, l[1])), mul(s.n, l[2]));
    if r[2] < spec_prob(s) {
        // GGX half vector
        let a = s.rough * s.rough;
        let phi = 2.0 * PI * r[0];
        let cos_t = ((1.0 - r[1]) / (1.0 + (a * a - 1.0) * r[1])).max(0.0).sqrt();
        let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
        let h = to_world([sin_t * phi.cos(), sin_t * phi.sin(), cos_t]);
        let l = sub(mul(h, 2.0 * dot(vv, h)), vv);
        if dot(l, s.n) <= 0.0 {
            return None;
        }
        Some(norm(l))
    } else {
        let rad = r[0].sqrt();
        let phi = 2.0 * PI * r[1];
        let l = to_world([rad * phi.cos(), rad * phi.sin(), (1.0 - r[0]).max(0.0).sqrt()]);
        if dot(l, s.n) <= 0.0 {
            return None;
        }
        Some(norm(l))
    }
}

impl<'a> PathTracer<'a> {
    pub fn new(maps: &'a Maps<'a>, view: &View, w: usize, h: usize) -> PathTracer<'a> {
        let height = if view.relief == RELIEF_DISPLACE { maps.height } else { None };
        let sm = objects::build_scene(view, height);
        let cam = view.cam();
        let (bvh, ids) = raster::scene_bvh(&sm, Some(cam.eye));
        let env = match maps.env {
            Some(e) => Arc::new(e.clone()),
            None => raster::env_of(maps, view),
        };
        let sampler = env.sampler();
        let alpha = raster::has_opacity(maps);
        PathTracer { maps, view: view.clone(), sm, bvh, ids, alpha, env, sampler, w: w.max(1), h: h.max(1) }
    }

    /// Radiance arriving along one camera path.
    fn path(&self, ev: &EnvView, o: V, d: V, py: f32, rng: &mut Rng) -> (V, f32) {
        let v = &self.view;
        let cf = v.cam_frame();
        let lights: Vec<&scene::Light> = v.lights.iter().filter(|l| l.enabled && l.intensity > 0.0).collect();
        let mut radiance = [0.0f32; 3];
        let mut thr = [1.0f32; 3];
        let (mut o, mut d) = (o, d);
        let mut first_depth = f32::INFINITY;
        // for MIS of environment hits found by BSDF sampling
        let mut prev_pdf = 0.0f32;
        let mut prev_was_nee = false;
        for bounce in 0..=MAX_BOUNCES {
            // transparent texels let the ray through (with the probability of their transparency)
            let mut hit = self.bvh.closest(o, d, 1e-4, 1e30, &Filter::ALL);
            let mut passed = 0.0f32;
            for _ in 0..16 {
                match hit {
                    Some(h) if self.see_through(h.tri, h.u, h.v, rng) => {
                        passed += h.t;
                        o = add(o, mul(d, h.t));
                        hit = self.bvh.closest(o, d, 1e-4, 1e30, &Filter::ALL);
                    }
                    _ => break,
                }
            }
            let Some(mut hit) = hit else {
                let bg = if bounce == 0 {
                    raster::background(v, ev, d, py)
                } else {
                    let e = ev.specular(d, 0.0);
                    let mut c = e;
                    if prev_was_nee && prev_pdf > 0.0 {
                        // weight of the BSDF sample against the environment sampler
                        let pe = self.sampler.pdf(ev.to_env(d));
                        let wb = prev_pdf * prev_pdf / (prev_pdf * prev_pdf + pe * pe).max(1e-12);
                        c = mul(c, wb);
                    }
                    mul(c, v.exposure)
                };
                for k in 0..3 {
                    radiance[k] += thr[k] * bg[k];
                }
                break;
            };
            hit.t += passed;
            if bounce == 0 {
                first_depth = hit.t * dot(d, v.cam().fwd);
            }
            hit.t -= passed;
            let tri = self.ids[hit.tri as usize] as usize;
            let vv = mul(d, -1.0);
            let mut s = surface(self.maps, v, &self.sm, tri, hit.u, hit.v, vv, false);
            // the ambient occlusion map stands in for fine geometry the mesh does not have
            for k in 0..3 {
                s.albedo[k] *= s.ao;
            }
            s.ao = 1.0;
            for k in 0..3 {
                radiance[k] += thr[k] * s.emissive[k] * v.exposure;
            }
            let p = s.p;
            let o_shadow = add(p, mul(s.ng, 0.003));
            // ---- next event estimation: lights
            for l in &lights {
                let (ld, e, dist) = scene::light_at(l, &cf, p);
                if dot(s.n, ld) <= 0.0 || (e[0] + e[1] + e[2]) <= 0.0 {
                    continue;
                }
                let mut dir = ld;
                if l.softness > 0.01 {
                    let (t, b) = hdri::basis(ld);
                    let (r1, r2) = (rng.next().sqrt() * l.softness * 0.18, rng.next() * 2.0 * PI);
                    dir = norm(add(ld, add(mul(t, r1 * r2.cos()), mul(b, r1 * r2.sin()))));
                    if dot(dir, s.ng) <= 0.0 {
                        continue;
                    }
                }
                if l.shadows {
                    let tmax = if dist.is_finite() { (dist - 0.01).max(0.01) } else { 1e30 };
                    if self.blocked(o_shadow, dir, tmax, rng) {
                        continue;
                    }
                }
                let c = direct(&s, vv, dir, e);
                for k in 0..3 {
                    radiance[k] += thr[k] * c[k] * v.exposure;
                }
            }
            // ---- next event estimation: environment
            let (de, pdf_e) = self.sampler.sample(rng.next(), rng.next());
            let dw = ev.from_env(de);
            let nee_ok = pdf_e > 0.0 && dot(s.n, dw) > 0.0 && dot(s.ng, dw) > 0.0;
            if nee_ok && !self.blocked(o_shadow, dw, 1e30, rng) {
                let li = mul(self.env.background(de), ev.gain);
                let f = direct(&s, vv, dw, [1.0; 3]); // BSDF x cos
                let pb = bsdf_pdf(&s, vv, dw);
                let wl = pdf_e * pdf_e / (pdf_e * pdf_e + pb * pb).max(1e-12);
                for k in 0..3 {
                    radiance[k] += thr[k] * f[k] * li[k] / pdf_e * wl * v.exposure;
                }
            }
            // ---- continue the path
            let Some(nd) = sample_bsdf(&s, vv, [rng.next(), rng.next(), rng.next()]) else { break };
            let pdf = bsdf_pdf(&s, vv, nd);
            if pdf <= 1e-8 {
                break;
            }
            let f = direct(&s, vv, nd, [1.0; 3]);
            for k in 0..3 {
                thr[k] *= f[k] / pdf;
            }
            let tmax = thr[0].max(thr[1]).max(thr[2]);
            if !tmax.is_finite() {
                break;
            }
            // clamp rare fireflies
            if tmax > 8.0 {
                let k = 8.0 / tmax;
                for t in thr.iter_mut() {
                    *t *= k;
                }
            }
            if bounce >= 3 {
                let q = tmax.clamp(0.05, 0.95);
                if rng.next() > q {
                    break;
                }
                for t in thr.iter_mut() {
                    *t /= q;
                }
            }
            prev_pdf = pdf;
            prev_was_nee = true;
            o = o_shadow;
            d = nd;
        }
        (radiance, first_depth)
    }

    /// Does the ray pass a hit point of the BVH's triangle `tri` (transparent texels, by their transparency)?
    fn see_through(&self, tri: u32, b1: f32, b2: f32, rng: &mut Rng) -> bool {
        self.alpha && rng.next() >= raster::opacity(self.maps, &self.sm, self.ids[tri as usize] as usize, b1, b2)
    }

    /// Is the light along a shadow ray blocked before `tmax` (transparent texels let part of it through)?
    fn blocked(&self, o: V, d: V, tmax: f32, rng: &mut Rng) -> bool {
        if !self.alpha {
            return self.bvh.any(o, d, 1e-4, tmax, &Filter::ALL);
        }
        let (mut o, mut left) = (o, tmax);
        for _ in 0..16 {
            match self.bvh.closest(o, d, 1e-4, left, &Filter::ALL) {
                None => return false,
                Some(h) if self.see_through(h.tri, h.u, h.v, rng) => {
                    o = add(o, mul(d, h.t));
                    left -= h.t;
                }
                Some(_) => return true,
            }
        }
        true
    }

    /// Add `spp` samples to every pixel.
    pub fn render_pass(&self, st: &mut PtState, spp: u32) {
        let (w, h) = (self.w, self.h);
        assert!(st.w == w && st.h == h, "state and tracer sizes differ");
        let cam = self.view.cam();
        let aspect = w as f32 / h as f32;
        let ev = EnvView::new(&self.env, &self.view);
        let base_sample = st.samples;
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).max(1);
        let rows_per = (h + threads * 4 - 1) / (threads * 4);
        let rows_per = rows_per.max(1);
        let next_row = std::sync::atomic::AtomicUsize::new(0);
        let (sum_chunks, depth_chunks): (Vec<_>, Vec<_>) = (st.sum.chunks_mut(rows_per * w).collect::<Vec<_>>(), st.depth.chunks_mut(rows_per * w).collect::<Vec<_>>());
        let jobs: Vec<std::sync::Mutex<(usize, &mut [[f32; 3]], &mut [f32])>> = sum_chunks.into_iter().zip(depth_chunks).enumerate().map(|(i, (s, d))| std::sync::Mutex::new((i, s, d))).collect();
        let jobs = &jobs;
        let next = &next_row;
        let this = &*self;
        let evr = &ev;
        std::thread::scope(|sc| {
            for _ in 0..threads {
                sc.spawn(move || loop {
                    let j = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if j >= jobs.len() {
                        break;
                    }
                    let mut guard = jobs[j].lock().unwrap();
                    let t = &mut *guard;
                    let (ci, sum, depth) = (t.0, &mut *t.1, &mut *t.2);
                    let y0 = ci * rows_per;
                    for (i, (acc, dp)) in sum.iter_mut().zip(depth.iter_mut()).enumerate() {
                        let (x, y) = (i % w, y0 + i / w);
                        for s in 0..spp {
                            let mut rng = Rng::new(x as u32, y as u32, base_sample + s);
                            let (jx, jy) = (rng.next(), rng.next());
                            let nx = ((x as f32 + jx) / w as f32) * 2.0 - 1.0;
                            let ny = 1.0 - ((y as f32 + jy) / h as f32) * 2.0;
                            let dir = norm(add(add(cam.fwd, mul(cam.right, nx * aspect * cam.tan_half)), mul(cam.up, ny * cam.tan_half)));
                            let (c, z) = this.path(evr, cam.eye, dir, (y as f32 + jy) / h as f32, &mut rng);
                            let ok = c.iter().all(|x| x.is_finite());
                            if ok {
                                for k in 0..3 {
                                    acc[k] += c[k].min(1.0e4);
                                }
                            }
                            if s == 0 && base_sample == 0 {
                                *dp = z;
                            }
                        }
                    }
                });
            }
        });
        st.samples += spp;
    }

    /// The picture so far: averaged radiance through the post chain (display referred).
    pub fn resolve(&self, st: &PtState) -> Tex {
        let n = st.samples.max(1) as f32;
        let hdr = Tex { w: st.w, h: st.h, px: st.sum.iter().map(|c| [c[0] / n, c[1] / n, c[2] / n, 1.0]).collect() };
        let cam = self.view.cam();
        let ctx = PostCtx { tan_half: cam.tan_half, aspect: st.w as f32 / st.h as f32, centre_dist: cam.dist };
        post::apply(&hdr, &st.depth, &ctx, &self.view.post)
    }
}

/// Render `spp` samples per pixel in one go. Channel views are not path traced (they show the raw maps).
pub fn render(m: &Maps, v: &View, w: usize, h: usize, spp: u32) -> Tex {
    if v.channel != Channel::Lit {
        return raster::render(m, v, w, h);
    }
    let pt = PathTracer::new(m, v, w, h);
    let mut st = PtState::new(pt.w, pt.h);
    pt.render_pass(&mut st, spp.max(1));
    pt.resolve(&st)
}
