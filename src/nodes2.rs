// SPDX-License-Identifier: MIT OR Apache-2.0
//! Second set of nodes (batch 1). `compute` returns Ok(None) for kinds it does not know, so the
//! main evaluator can fall back to it. Every node here is tileable and independent of the output resolution:
//! pixel-sized parameters are given for a 512 px texture and scaled by `size / 512`.
use crate::graph::{Error, Node};
use crate::noise;
use crate::ops;
use crate::tex::{lum, Tex};
use std::collections::BTreeMap;
use std::f32::consts::TAU;
use std::sync::Arc as Rc;

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let d = b - a;
    let t = if d.abs() < 1e-9 { if x >= b { 1.0 } else { 0.0 } } else { ((x - a) / d).clamp(0.0, 1.0) };
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// ---------------------------------------------------------------- FFT (used by Normal To Height)
fn fft(a: &mut [[f32; 2]], inverse: bool) {
    let n = a.len();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            a.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = if inverse { TAU } else { -TAU } / len as f32;
        let (ws, wc) = ang.sin_cos();
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (u, v) = (a[start + k], a[start + k + len / 2]);
                let t = [v[0] * cr - v[1] * ci, v[0] * ci + v[1] * cr];
                a[start + k] = [u[0] + t[0], u[1] + t[1]];
                a[start + k + len / 2] = [u[0] - t[0], u[1] - t[1]];
                let nr = cr * wc - ci * ws;
                ci = cr * ws + ci * wc;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

fn transpose_c(a: &[[f32; 2]], n: usize) -> Vec<[f32; 2]> {
    let mut o = vec![[0.0; 2]; n * n];
    for y in 0..n {
        for x in 0..n {
            o[x * n + y] = a[y * n + x];
        }
    }
    o
}

fn fft2(a: &mut Vec<[f32; 2]>, n: usize, inverse: bool) {
    for r in a.chunks_mut(n) {
        fft(r, inverse);
    }
    let mut t = transpose_c(a, n);
    for r in t.chunks_mut(n) {
        fft(r, inverse);
    }
    *a = transpose_c(&t, n);
}

/// Integrate a tangent-space normal map back into a height map (periodic Poisson solve in the Fourier domain).
fn normal_to_height(nm: &Tex, directx: bool, relief: f32, opacity: f32) -> Tex {
    let n = nm.w.next_power_of_two().min(4096).max(2);
    let src = if nm.w == n && nm.h == n { nm.clone() } else { ops::resize(nm, n) };
    let mut gx = vec![[0.0f32; 2]; n * n];
    let mut gy = vec![[0.0f32; 2]; n * n];
    for i in 0..n * n {
        let p = src.px[i];
        let (x, y, z) = ((p[0] - 0.5) * 2.0, (p[1] - 0.5) * 2.0, ((p[2] - 0.5) * 2.0).max(0.05));
        gx[i][0] = -x / z;
        gy[i][0] = (if directx { -y } else { y }) / z;
    }
    fft2(&mut gx, n, false);
    fft2(&mut gy, n, false);
    let mut h = vec![[0.0f32; 2]; n * n];
    for ky in 0..n {
        let sy = (TAU * ky as f32 / n as f32).sin();
        for kx in 0..n {
            let sx = (TAU * kx as f32 / n as f32).sin();
            let d = sx * sx + sy * sy;
            if d < 1e-9 {
                continue;
            }
            let i = ky * n + kx;
            // h = -i (sx gx + sy gy) / (sx^2 + sy^2)
            let (re, im) = (sx * gx[i][0] + sy * gy[i][0], sx * gx[i][1] + sy * gy[i][1]);
            // relief balance: 0.5 = plain integration, above favours large shapes, below favours fine detail
            let wgt = if (relief - 0.5).abs() > 1e-4 { d.powf(0.5 - relief) } else { 1.0 };
            h[i] = [im / d * wgt, -re / d * wgt];
        }
    }
    fft2(&mut h, n, true);
    let vals: Vec<f32> = h.iter().map(|c| c[0] / (n * n) as f32).collect();
    let (mn, mx) = vals.iter().fold((f32::MAX, f32::MIN), |a, v| (a.0.min(*v), a.1.max(*v)));
    let k = if mx - mn > 1e-9 { 1.0 / (mx - mn) } else { 0.0 };
    let out = Tex { w: n, h: n, px: vals.iter().map(|v| { let g = if k > 0.0 { (v - mn) * k } else { 0.5 }; let g = 0.5 + (g - 0.5) * opacity; [g, g, g, 1.0] }).collect() };
    if nm.w == n && nm.h == n {
        out
    } else {
        ops::resize_to(&out, nm.w, nm.h)
    }
}

/// Run a node of this module. Ok(None) = not one of ours.
pub fn compute(kind: &str, id: u32, n: &Node, ins: &BTreeMap<String, Rc<Tex>>, s: usize, hs: usize, seed: u32) -> Result<Option<Tex>, Error> {
    let f = |k: &str, d: f32| n.nums.get(k).copied().unwrap_or(d);
    let need = |slot: &str| -> Result<&Tex, Error> { ins.get(slot).map(|r| r.as_ref()).ok_or_else(|| Error::MissingInput(id, slot.to_string())) };
    let opt = |slot: &str| -> Option<&Tex> { ins.get(slot).map(|r| r.as_ref()) };
    let rs = s as f32 / 512.0;
    let per = |k: &str, d: f32| f(k, d).round().clamp(1.0, 256.0) as i32;
    let (con, inv) = (f("contrast", 1.0), f("invert", 0.0) > 0.5);
    let dz = noise::Disorder::new(seed, f("disorder", 0.0), f("disorder_speed", 1.0), f("anisotropy", 0.0), f("anisotropy_angle", 0.0));
    let fin = |t: Tex| -> Tex {
        if (con - 1.0).abs() > 1e-4 || inv {
            ops::contrast_invert(&t, con, inv)
        } else {
            t
        }
    };
    let out = match kind {
        // ------------------------------------------------ blurs and warps
        "non_uniform_blur" => {
            let (t, m) = (need("in")?, need("mask")?);
            let max = f("intensity", 8.0).clamp(0.0, 64.0) * rs;
            let lv: Vec<Tex> = (0..=4).map(|i| if i == 0 { t.clone() } else { ops::blur(t, max * i as f32 / 4.0) }).collect();
            Tex::from_fn(t.w, t.h, |u, v| {
                let k = lum(m.sample(u, v)).clamp(0.0, 1.0) * 4.0;
                let i = (k.floor() as usize).min(3);
                let fr = k - i as f32;
                let (a, b) = (lv[i].sample(u, v), lv[i + 1].sample(u, v));
                [lerp(a[0], b[0], fr), lerp(a[1], b[1], fr), lerp(a[2], b[2], fr), lerp(a[3], b[3], fr)]
            })
        }
        "radial_blur" => {
            let t = need("in")?;
            let (cx, cy) = (0.5 + f("x", 0.0), 0.5 + f("y", 0.0));
            if f("mode", 0.0) > 0.5 {
                // legacy zoom blur
                let (amount, rad) = (f("amount", 0.1).clamp(0.0, 1.0), f("radius", 0.5).clamp(0.01, 0.5));
                let taps = 24;
                if let Some(g) = crate::gpu::radial_blur(t, 1, taps as u32, cx, cy, amount, rad) {
                    return Ok(Some(g));
                }
                Tex::from_fn(t.w, t.h, |u, v| {
                    let (dx, dy) = (ops::wrapd(u - cx), ops::wrapd(v - cy));
                    let r = (dx * dx + dy * dy).sqrt();
                    let amt = amount * smooth(0.0, 1.0, 1.0 - r / rad);
                    let mut a = [0.0f32; 4];
                    for k in 0..taps {
                        let sc = 1.0 - amt * (k as f32 / (taps - 1) as f32);
                        let p = t.sample(cx + dx * sc, cy + dy * sc);
                        for c in 0..4 {
                            a[c] += p[c] / taps as f32;
                        }
                    }
                    a
                })
            } else {
                // spin blur, `samples` taps along an arc of `angle` turns around the centre
                let n = f("samples", 64.0).round().clamp(1.0, 128.0) as usize;
                let arc = f("angle", 0.125).clamp(0.0, 0.5) * TAU;
                if let Some(g) = crate::gpu::radial_blur(t, 0, n as u32, cx, cy, arc, 0.0) {
                    return Ok(Some(g));
                }
                Tex::from_fn(t.w, t.h, |u, v| {
                    let (dx, dy) = (ops::wrapd(u - cx), ops::wrapd(v - cy));
                    // the blur fades out towards the image border so the result still tiles
                    let fade = ((0.5 - (dx * dx + dy * dy).sqrt()) / 0.15).clamp(0.0, 1.0);
                    let mut a = [0.0f32; 4];
                    for k in 0..n {
                        let th = if n > 1 { (k as f32 / (n - 1) as f32 - 0.5) * arc * fade } else { 0.0 };
                        let (sn, cs) = th.sin_cos();
                        let p = t.sample(cx + dx * cs - dy * sn, cy + dx * sn + dy * cs);
                        for c in 0..4 {
                            a[c] += p[c] / n as f32;
                        }
                    }
                    a
                })
            }
        }
        "anisotropic_blur" => {
            let t = need("in")?;
            let (inten, an) = (f("intensity", 8.0).clamp(0.0, 64.0), f("anisotropy", 0.8).clamp(0.0, 1.0));
            let sg = inten * (1.0 - an) * rs;
            let a = crate::gpu::blur(t, sg).unwrap_or_else(|| ops::blur(t, sg));
            let (ang, len) = (f("angle", 0.0), inten * an * 4.0 / 512.0);
            crate::gpu::directional_blur(&a, ang, len).unwrap_or_else(|| ops::directional_blur(&a, ang, len))
        }
        // ------------------------------------------------ transforms
        "symmetry" => {
            let t = need("in")?;
            let (axis, c) = (f("axis", 0.0) as u32, f("offset", 0.0));
            let sg = if f("flip", 0.0) > 0.5 { -1.0 } else { 1.0 };
            let tm = f("transfer", 0.0).round() as u32; // 0 copy, 1 add, 2 subtract, 3 multiply, 4 add sub, 5 max, 6 min
            let fold = move |p: f32| 0.5 + c + sg * ops::wrapd(p - 0.5 - c).abs();
            let diag = move |u: f32, v: f32| {
                // mirror across the line u = v
                let (x, y) = (ops::wrapd(u - v), u + v);
                let x = x.abs() * sg;
                ((y + x) * 0.5, (y - x) * 0.5)
            };
            let anti = move |u: f32, v: f32| {
                // mirror across the line u + v = 0 (mod 1)
                let (x, y) = (ops::wrapd(u + v), u - v);
                let x = x.abs() * sg;
                ((y + x) * 0.5, (x - y) * 0.5)
            };
            Tex::from_fn(t.w, t.h, |u, v| {
                let (a, b) = match axis {
                    0 => (fold(u), v),
                    1 => (u, fold(v)),
                    2 => (fold(u), fold(v)),
                    3 => diag(u, v),
                    4 => anti(u, v),
                    6 => {
                        // both diagonals
                        let (a, b) = diag(u, v);
                        anti(a, b)
                    }
                    _ => diag(fold(u), fold(v)), // 8-fold: both axes plus the diagonal
                };
                let m = t.sample(a, b);
                if tm == 0 {
                    return m;
                }
                let o = t.sample(u, v);
                let mut r = o;
                for k in 0..3 {
                    r[k] = match tm {
                        1 => o[k] + m[k],
                        2 => o[k] - m[k],
                        3 => o[k] * m[k],
                        4 => o[k] + (m[k] - 0.5) * 2.0,
                        5 => o[k].max(m[k]),
                        _ => o[k].min(m[k]),
                    }
                    .clamp(0.0, 1.0);
                }
                r
            })
        }
        "skew" => {
            let t = need("in")?;
            // whole numbers keep the lattice periodic: the skewed texture still tiles (an imported fraction does not)
            let (kx, ky) = (f("skew_x", 1.0).clamp(-32.0, 32.0), f("skew_y", 0.0).clamp(-32.0, 32.0));
            // align = the line the shear pivots around: 0 centre, 1 top left (default), 2 bottom right
            let o = match f("align", 1.0).round() as u32 { 0 => 0.5, 2 => 1.0, _ => 0.0 };
            Tex::from_fn(t.w, t.h, |u, v| t.sample_cubic(u + kx * (v - o), v + ky * (u - o)))
        }
        // ------------------------------------------------ adjustments
        "auto_levels" => {
            let t = need("in")?;
            let per_ch = f("per_channel", 0.0) > 0.5;
            let mut lo = [f32::MAX; 3];
            let mut hi = [f32::MIN; 3];
            for p in &t.px {
                for c in 0..3 {
                    lo[c] = lo[c].min(p[c]);
                    hi[c] = hi[c].max(p[c]);
                }
            }
            if !per_ch {
                let (l, h) = (lo[0].min(lo[1]).min(lo[2]), hi[0].max(hi[1]).max(hi[2]));
                lo = [l; 3];
                hi = [h; 3];
            }
            t.map(|p| {
                let mut o = p;
                for c in 0..3 {
                    o[c] = if hi[c] - lo[c] > 1e-6 { (p[c] - lo[c]) / (hi[c] - lo[c]) } else { p[c] };
                }
                o
            })
        }
        "channel_mixer" => {
            let t = need("in")?;
            let m = [[f("rr", 1.0), f("rg", 0.0), f("rb", 0.0)], [f("gr", 0.0), f("gg", 1.0), f("gb", 0.0)], [f("br", 0.0), f("bg", 0.0), f("bb", 1.0)]];
            let mono = f("mono", 0.0) > 0.5;
            t.map(|p| {
                let mut o = p;
                for c in 0..3 {
                    o[c] = (m[c][0] * p[0] + m[c][1] * p[1] + m[c][2] * p[2]).clamp(-8.0, 8.0);
                }
                if mono {
                    let g = lum(o);
                    o = [g, g, g, p[3]];
                }
                o
            })
        }
        "alpha_merge" => {
            let rgb = need("rgb")?;
            match opt("alpha") {
                Some(a) => Tex::from_fn(rgb.w, rgb.h, |u, v| {
                    let p = rgb.sample(u, v);
                    [p[0], p[1], p[2], lum(a.sample(u, v))]
                }),
                None => rgb.clone(),
            }
        }
        "color_to_mask" => {
            let t = need("in")?;
            let (cr, cg, cb) = (f("r", 1.0), f("g", 0.0), f("b", 0.0));
            let (range, soft) = (f("range", 0.2).max(0.0), f("softness", 0.1).max(0.0));
            t.map(|p| {
                let d = (((p[0] - cr).powi(2) + (p[1] - cg).powi(2) + (p[2] - cb).powi(2)) / 3.0).sqrt();
                let m = 1.0 - smooth(range, range + soft.max(1e-4), d);
                [m, m, m, 1.0]
            })
        }
        "replace_color" => {
            let t = need("in")?;
            let src = [f("src_r", 1.0), f("src_g", 0.0), f("src_b", 0.0)];
            let dst = [f("dst_r", 0.0), f("dst_g", 0.0), f("dst_b", 1.0)];
            let (range, soft) = (f("range", 0.25).max(0.0), f("softness", 0.15).max(0.0));
            t.map(|p| {
                let d = (((p[0] - src[0]).powi(2) + (p[1] - src[1]).powi(2) + (p[2] - src[2]).powi(2)) / 3.0).sqrt();
                let m = 1.0 - smooth(range, range + soft.max(1e-4), d);
                [p[0] + (dst[0] - src[0]) * m, p[1] + (dst[1] - src[1]) * m, p[2] + (dst[2] - src[2]) * m, p[3]]
            })
        }
        "pow" => {
            let t = need("in")?;
            let e = f("exponent", 2.0).clamp(0.0, 32.0);
            t.map(|p| [p[0].max(0.0).powf(e), p[1].max(0.0).powf(e), p[2].max(0.0).powf(e), p[3]])
        }
        "histogram_shift" => {
            let t = need("in")?;
            let d = (f("position", 0.5) - 0.5) * 2.0;
            t.map(|p| [(p[0] + d).clamp(0.0, 1.0), (p[1] + d).clamp(0.0, 1.0), (p[2] + d).clamp(0.0, 1.0), p[3]])
        }
        "grayscale_conversion" => {
            let t = need("in")?;
            let (wr, wg, wb) = (f("r", 0.3), f("g", 0.59), f("b", 0.11));
            t.map(|p| {
                let g = p[0] * wr + p[1] * wg + p[2] * wb;
                [g, g, g, p[3]]
            })
        }
        // ------------------------------------------------ generators
        "cells" => {
            let (var, p) = (f("variant", 0.0) as u32, per("scale", 8.0));
            let jitter = f("jitter", 1.0).clamp(0.0, 1.0);
            let (lum_rand, edge_w, hard) = (f("luminance_random", 0.0).clamp(0.0, 1.0), f("edge_width", 0.25).clamp(0.01, 1.0), f("hardness", 2.0).clamp(0.1, 16.0));
            let (src, pseed) = (f("color_source", 0.0) as u32, f("pseudorandom_seed", 0.0) as u32);
            let img = if src == 2 { opt("color") } else { None };
            let extra = f("_out", 0.0).round() as i32;
            let py = if f("square_cells", 0.0) > 0.5 { ((p as f32 * hs as f32 / s as f32).round().max(1.0)) as i32 } else { p };
            fin(Tex::gray_fn(s, hs, move |u, v| {
                let (u, v) = dz.warp(u, v);
                let vr = noise::voronoi_ext(u, v, p, py, seed, jitter);
                // cell value of Cells 4: random, pseudorandom (re-rolled with its own seed) or the input image at the cell centre
                let cell_val = || match (src, img) {
                    (2, Some(t)) => lum(t.sample(u + vr.dx / p as f32, v + vr.dy / p as f32)),
                    (1, _) => noise::rnd((((u + vr.dx / p as f32) * p as f32).floor()) as i32, (((v + vr.dy / p as f32) * p as f32).floor()) as i32, pseed.wrapping_add(4242)),
                    _ => vr.id1,
                };
                match extra {
                    1 => return vr.id1,
                    2 => return vr.f1,
                    3 => return smooth(0.0, edge_w, vr.f2 - vr.f1),
                    _ => {}
                }
                match var {
                    0 => (1.0 - vr.f1) * (1.0 - lum_rand * vr.id1),
                    1 => cell_val(),
                    2 => smooth(0.0, edge_w, vr.f2 - vr.f1),
                    _ => vr.id1 * (1.0 - vr.f1.powf(hard)),
                }
            }))
        }
        "fractal_sum" => {
            let (p, lo, hi) = (per("scale", 2.0), f("min_level", 0.0).clamp(0.0, 10.0) as u32, f("max_level", 6.0).clamp(0.0, 10.0) as u32);
            let rough = f("roughness", 0.5).clamp(0.0, 1.5);
            let mode = f("mode", 0.0) as u32;
            let hi = hi.max(lo);
            let grids: Vec<(noise::GradGrid, f32)> = (lo..=hi)
                .map(|o| {
                    let q = (p as i64 * (1i64 << o)).clamp(1, 1024) as i32;
                    (noise::GradGrid::new(q, q, seed.wrapping_add(o * 31)), rough.powi((o - lo) as i32))
                })
                .collect();
            let norm: f32 = grids.iter().map(|g| g.1).sum::<f32>().max(1e-6);
            fin(Tex::gray_fn(s, hs, move |u, v| {
                let (u, v) = dz.warp(u, v);
                let mut acc = 0.0;
                for (g, a) in &grids {
                    let x = g.sample(u, v);
                    acc += a * match mode {
                        1 => (x * 2.0 - 1.0).abs(),
                        2 => 1.0 - (x * 2.0 - 1.0).abs(),
                        _ => x,
                    };
                }
                (acc / norm).clamp(0.0, 1.0)
            }))
        }
        "waveform" if f("style", 0.0) > 0.5 => {
            // sound-wave bars: one soft vertical bar per sample, their heights following a bell per wave with
            // random variation
            let samples = f("samples", 256.0).round().clamp(1.0, 2048.0);
            let waves = f("wave_number", 2.0).round().clamp(0.0, 64.0);
            let (smin, smax) = (f("size_min", 0.0).clamp(0.0, 1.0), f("size_max", 1.0).clamp(0.0, 1.0));
            let noise_amt = f("noise", 0.5).clamp(0.0, 1.0);
            let peak = f("intensity", 1.0).clamp(0.0, 4.0);
            let ns = samples as i32;
            let px = 1.0 / hs as f32;
            fin(Tex::gray_fn(s, hs, move |u, v| {
                let i = ((u * samples).floor() as i32).rem_euclid(ns);
                let uc = (i as f32 + 0.5) / samples;
                let env = (std::f32::consts::PI * (uc * waves).fract()).sin().powi(2);
                let r = crate::noise_styles::unit(i, 0, seed);
                let half = 0.125 * (smin + (smax - smin) * env * (1.0 - noise_amt * r));
                if half <= 0.0 {
                    return 0.0;
                }
                let sigma = (half / 1.1774).max(px * 0.5);
                peak * (-0.5 * ((v - 0.5) / sigma).powi(2)).exp()
            }))
        }
        "waveform" => {
            let (a, b) = ops::tileable_wave(f("count", 8.0), f("angle", 0.0));
            let (warp, w) = (f("warp", 0.3).clamp(0.0, 4.0), per("warp_scale", 4.0));
            let shape = f("shape", 0.0) as u32;
            fin(Tex::gray_fn(s, hs, move |u, v| {
                let (u, v) = dz.warp(u, v);
                let ph = (a as f32 * u + b as f32 * v) + (noise::perlin(u, v, w, w, seed) - 0.5) * warp;
                let x = ph - ph.floor();
                match shape {
                    1 => 1.0 - (x * 2.0 - 1.0).abs(),
                    2 => x,
                    3 => if x < 0.5 { 1.0 } else { 0.0 },
                    _ => 0.5 + 0.5 * (x * TAU).sin(),
                }
            }))
        }
        "gaussian_spots" => {
            let p = per("scale", 8.0);
            let (size, var) = (f("size", 0.5).clamp(0.05, 1.0), f("size_variation", 0.5).clamp(0.0, 1.0));
            fin(Tex::gray_fn(s, hs, move |u, v| {
                let (u, v) = dz.warp(u, v);
                // spots() blends smoothly from 1 to 0 between the inner and outer radius, i.e. a gaussian-like bump
                noise::spots(u, v, p, p, seed, |id| {
                    let r = size * 0.55 * (1.0 - var + var * id);
                    (0.0, r.max(0.02))
                }, 0.0)
            }))
        }
        // ------------------------------------------------ normals and surface
        "normal_invert" => {
            let t = need("in")?;
            let (ix, iy, iz, ia) = (f("invert_x", 0.0) > 0.5, f("invert_y", 1.0) > 0.5, f("invert_z", 0.0) > 0.5, f("invert_a", 0.0) > 0.5);
            t.map(|p| [if ix { 1.0 - p[0] } else { p[0] }, if iy { 1.0 - p[1] } else { p[1] }, if iz { 1.0 - p[2] } else { p[2] }, if ia { 1.0 - p[3] } else { p[3] }])
        }
        "normal_normalize" => need("in")?.map(|p| {
            let (x, y, z) = ((p[0] - 0.5) * 2.0, (p[1] - 0.5) * 2.0, (p[2] - 0.5) * 2.0);
            let l = (x * x + y * y + z * z).sqrt().max(1e-6);
            [x / l * 0.5 + 0.5, y / l * 0.5 + 0.5, z / l * 0.5 + 0.5, p[3]]
        }),
        "normal_vector_rotation" => {
            let t = need("in")?;
            let (ang, gs) = (f("angle", 0.0) * TAU, if f("directx", 0.0) > 0.5 { -1.0f32 } else { 1.0 });
            let rot = opt("rotation");
            Tex::from_fn(t.w, t.h, |u, v| {
                let p = t.sample(u, v);
                // the optional rotation map scales the angle per pixel
                let a = ang * rot.map(|m| lum(m.sample(u, v))).unwrap_or(1.0);
                let (sn, cs) = a.sin_cos();
                let (x, y) = ((p[0] - 0.5) * 2.0, gs * (p[1] - 0.5) * 2.0);
                let (rx, ry) = (cs * x - sn * y, sn * x + cs * y);
                [rx * 0.5 + 0.5, gs * ry * 0.5 + 0.5, p[2], p[3]]
            })
        }
        "normal_to_height" => normal_to_height(need("in")?, f("directx", 0.0) > 0.5, f("relief_balance", 0.5).clamp(0.0, 1.0), f("opacity", 1.0).clamp(0.0, 1.0)),
        "sobel" => {
            let t = need("in")?;
            let k = f("intensity", 1.0).clamp(0.0, 64.0) * (t.w as f32 / 512.0).max(0.25);
            Tex::from_fn(t.w, t.h, |u, v| {
                let (x, y) = ((u * t.w as f32) as i64, (v * t.h as f32) as i64);
                let g = |i: i64, j: i64| t.gray_at(x + i, y + j);
                let gx = g(1, -1) + 2.0 * g(1, 0) + g(1, 1) - g(-1, -1) - 2.0 * g(-1, 0) - g(-1, 1);
                let gy = g(-1, 1) + 2.0 * g(0, 1) + g(1, 1) - g(-1, -1) - 2.0 * g(0, -1) - g(1, -1);
                let m = ((gx * gx + gy * gy).sqrt() * k * 0.25).clamp(0.0, 1.0);
                [m, m, m, 1.0]
            })
        }
        // ------------------------------------------------ effects
        "glow" => {
            let t = need("in")?;
            let (w, inten) = (f("width", 8.0).clamp(0.0, 128.0) * rs, f("intensity", 1.0).clamp(0.0, 8.0));
            let b = ops::blur(t, w);
            // clear amount cuts the faint tail of the glow off; the colour tints it (white = unchanged)
            let (clear, gc) = (f("clear", 0.0).clamp(0.0, 0.99), [f("glow_r", 1.0), f("glow_g", 1.0), f("glow_b", 1.0)]);
            Tex::from_fn(t.w, t.h, |u, v| {
                let (p, q) = (t.sample(u, v), b.sample(u, v));
                let cut = |x: f32| ((x - clear) / (1.0 - clear)).max(0.0);
                [(p[0] + cut(q[0]) * gc[0] * inten).clamp(0.0, 1.0), (p[1] + cut(q[1]) * gc[1] * inten).clamp(0.0, 1.0), (p[2] + cut(q[2]) * gc[2] * inten).clamp(0.0, 1.0), p[3]]
            })
        }
        "mosaic" => {
            let t = need("in")?;
            if let Some(m) = opt("mosaic_map") {
                ops::mosaic_map(t, m, f("intensity", 1.0))
            } else {
                // no map: a plain grid of `count` cells per side
                let c = per("count", 16.0) as f32;
                Tex::from_fn(t.w, t.h, |u, v| t.sample(((u * c).floor() + 0.5) / c, ((v * c).floor() + 0.5) / c))
            }
        }
        "shape_drop_shadow" => {
            let t = need("in")?;
            let (ang, dist) = (f("angle", 0.125) * TAU, f("distance", 0.03).clamp(0.0, 0.5));
            let (sn, cs) = ang.sin_cos();
            let shifted = Tex::from_fn(t.w, t.h, |u, v| t.sample(u - cs * dist, v - sn * dist));
            let sh = ops::blur(&shifted, f("blur", 6.0).clamp(0.0, 128.0) * rs);
            let op = f("opacity", 0.8).clamp(0.0, 1.0);
            let spread = f("spread", 0.0).clamp(0.0, 1.0);
            Tex::from_fn(t.w, t.h, |u, v| {
                // spread raises the blur cut-off so the shadow stays solid for longer
                let sv = (lum(sh.sample(u, v)) / (1.0 - spread * 0.99)).min(1.0);
                let m = lum(t.sample(u, v)).max(sv * op).clamp(0.0, 1.0);
                [m, m, m, 1.0]
            })
        }
        // ------------------------------------------------ batch 1b: warps, patterns, strokes
        "non_uniform_directional_warp" => {
            let (t, m) = (need("in")?, need("intensity")?);
            let (k, base) = (f("intensity", 0.1).clamp(-2.0, 2.0), f("angle", 0.0) * TAU);
            let am = opt("angle");
            Tex::from_fn(t.w, t.h, |u, v| {
                let a = base + am.map(|a| (lum(a.sample(u, v)) - 0.5) * TAU).unwrap_or(0.0);
                let d = (lum(m.sample(u, v)) - 0.5) * 2.0 * k;
                t.sample_cubic(u + a.cos() * d, v + a.sin() * d)
            })
        }
        "multi_directional_warp" => {
            let (t, m) = (need("in")?, need("warp")?);
            let (k, a0) = (f("intensity", 0.1).clamp(-2.0, 2.0), f("angle", 0.0));
            // Directions: 0 = 1 (the angle and its opposite), 1 = 2 (plus the perpendicular axis), 2 = 4 (plus the 45 degree
            // steps). Larger values from older graphs keep meaning "that many samples".
            let dv = f("directions", 2.0).round().max(0.0) as usize;
            let n = if dv <= 2 { 2 * [1usize, 2, 4][dv] } else { dv.min(16) };
            let mode = f("mode", 0.0).round() as u32; // 0 average, 1 max, 2 min, 3 chain
            if mode == 3 {
                // each direction warps the result of the previous one
                let mut cur = t.clone();
                for i in 0..n {
                    let a = (a0 + i as f32 / n as f32) * TAU;
                    let src = cur.clone();
                    cur = Tex::from_fn(t.w, t.h, |u, v| {
                        let d = (lum(m.sample(u, v)) - 0.5) * 2.0 * k / n as f32;
                        src.sample(u + a.cos() * d, v + a.sin() * d)
                    });
                }
                cur
            } else {
                Tex::from_fn(t.w, t.h, |u, v| {
                    let d = (lum(m.sample(u, v)) - 0.5) * 2.0 * k;
                    let mut acc = if mode == 1 { [f32::MIN; 4] } else if mode == 2 { [f32::MAX; 4] } else { [0.0f32; 4] };
                    for i in 0..n {
                        let a = (a0 + i as f32 / n as f32) * TAU;
                        let p = t.sample(u + a.cos() * d, v + a.sin() * d);
                        for c in 0..4 {
                            match mode {
                                1 => acc[c] = acc[c].max(p[c]),
                                2 => acc[c] = acc[c].min(p[c]),
                                _ => acc[c] += p[c] / n as f32,
                            }
                        }
                    }
                    acc
                })
            }
        }
        "normal_combine" => {
            // technique 0 whiteout, 1 channel mixer (partial derivatives added), 2 detail oriented (reoriented normal mapping:
            // the detail normal is rotated into the surface of the base normal)
            let (b, d) = (need("base")?, need("detail")?);
            let tech = f("technique", 2.0).round() as u32;
            Tex::from_fn(b.w, b.h, |u, v| {
                let (pb, pd) = (b.sample(u, v), d.sample(u, v));
                if tech < 2 {
                    let (bx, by, bz) = ((pb[0] - 0.5) * 2.0, (pb[1] - 0.5) * 2.0, ((pb[2] - 0.5) * 2.0).max(0.05));
                    let (dx, dy, dz) = ((pd[0] - 0.5) * 2.0, (pd[1] - 0.5) * 2.0, ((pd[2] - 0.5) * 2.0).max(0.05));
                    let r = if tech == 0 {
                        [bx + dx, by + dy, bz * dz]
                    } else {
                        [bx / bz + dx / dz, by / bz + dy / dz, 1.0]
                    };
                    let l = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt().max(1e-6);
                    return [r[0] / l * 0.5 + 0.5, r[1] / l * 0.5 + 0.5, r[2] / l * 0.5 + 0.5, 1.0];
                }
                let tt = [(pb[0] - 0.5) * 2.0, (pb[1] - 0.5) * 2.0, (pb[2] - 0.5) * 2.0 + 1.0];
                let uu = [-(pd[0] - 0.5) * 2.0, -(pd[1] - 0.5) * 2.0, (pd[2] - 0.5) * 2.0];
                let dot = tt[0] * uu[0] + tt[1] * uu[1] + tt[2] * uu[2];
                let tz = if tt[2].abs() < 1e-4 { 1e-4 } else { tt[2] };
                let r = [tt[0] * dot / tz - uu[0], tt[1] * dot / tz - uu[1], tt[2] * dot / tz - uu[2]];
                let l = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt().max(1e-6);
                [r[0] / l * 0.5 + 0.5, r[1] / l * 0.5 + 0.5, r[2] / l * 0.5 + 0.5, 1.0]
            })
        }
        "triangle_grid" if f("style", 0.0) >= 0.5 => {
            let (nx, ny) = (per("x_amount", 16.0).clamp(1, 64), per("y_amount", 16.0).clamp(1, 64));
            let amp = f("random_position_multiplier", 1.0).clamp(0.0, 2.0).min(1.0) * 0.32;
            let (qx, qy) = (f("quincunx_offset_x", 0.0).clamp(0.0, 1.0), f("quincunx_offset_y", 0.0).clamp(0.0, 1.0));
            let (hr, tmode, omode) = (f("height_random", 1.0).clamp(0.0, 1.0), f("triangulation", 0.0).round() as i32, f("output", 0.0).round() as i32);
            let (th, lc, lb) = (f("distance_to_edge_thickness", 0.025).clamp(0.0, 0.1), f("thin_line_color", 1.0), f("thin_line_background", 0.0));
            let lines = f("thin_lines", 0.0) > 0.5;
            let rot = f("rotation", 0.0).clamp(0.0, 1.0);
            let rd = f("rotation_disorder", 0.5).clamp(0.0, 1.0);
            fin(Tex::gray_fn(s, hs, move |u, v| {
                let (x, y) = (u * nx as f32, v * ny as f32);
                let (ci, cj) = (x.floor() as i32, y.floor() as i32);
                // jittered lattice point (i, j) in grid units; indices wrap so the pattern tiles
                let vert = |i: i32, j: i32| -> ([f32; 2], f32) {
                    let (wi, wj) = (i.rem_euclid(nx), j.rem_euclid(ny));
                    let jx = (noise::rnd(wi, wj, seed) - 0.5) * 2.0 * amp;
                    let jy = (noise::rnd(wi, wj, seed.wrapping_add(7919)) - 0.5) * 2.0 * amp;
                    let sh = if j.rem_euclid(2) == 1 { qx } else { 0.0 };
                    let sv = if i.rem_euclid(2) == 1 { qy } else { 0.0 };
                    let h = 0.5 + (noise::rnd(wi, wj, seed.wrapping_add(104729)) - 0.5) * hr;
                    ([i as f32 + jx + sh * 0.5, j as f32 + jy + sv * 0.5], h)
                };
                let mut best = (f32::MAX, 0.0f32, 0.0f32);
                for dj in -1..=1 {
                    for di in -1..=1 {
                        let (i, j) = (ci + di, cj + dj);
                        let (wi, wj) = (i.rem_euclid(nx), j.rem_euclid(ny));
                        let alt = match tmode { 1 => ((i + j) & 1) as i32, 2 => (noise::rnd(wi, wj, seed.wrapping_add(31)) > 0.5) as i32, _ => 0 };
                        let (a, ha) = vert(i, j);
                        let (b, hb) = vert(i + 1, j);
                        let (c, hc) = vert(i, j + 1);
                        let (d, hd) = vert(i + 1, j + 1);
                        let tris: [([[f32; 2]; 3], [f32; 3]); 2] = if alt == 0 {
                            [([a, b, d], [ha, hb, hd]), ([a, d, c], [ha, hd, hc])]
                        } else {
                            [([a, b, c], [ha, hb, hc]), ([b, d, c], [hb, hd, hc])]
                        };
                        for (t, (p, hs)) in tris.iter().enumerate() {
                            let det = (p[1][1] - p[2][1]) * (p[0][0] - p[2][0]) + (p[2][0] - p[1][0]) * (p[0][1] - p[2][1]);
                            if det.abs() < 1e-9 {
                                continue;
                            }
                            let l0 = ((p[1][1] - p[2][1]) * (x - p[2][0]) + (p[2][0] - p[1][0]) * (y - p[2][1])) / det;
                            let l1 = ((p[2][1] - p[0][1]) * (x - p[2][0]) + (p[0][0] - p[2][0]) * (y - p[2][1])) / det;
                            let l2 = 1.0 - l0 - l1;
                            let m = l0.min(l1).min(l2);
                            if m < -1e-5 {
                                continue;
                            }
                            // distance to the nearest triangle edge in grid units
                            let mut de = f32::MAX;
                            for k in 0..3 {
                                let (e0, e1) = (p[k], p[(k + 1) % 3]);
                                let (ex, ey) = (e1[0] - e0[0], e1[1] - e0[1]);
                                let len = (ex * ex + ey * ey).sqrt().max(1e-9);
                                de = de.min(((x - e0[0]) * ey - (y - e0[1]) * ex).abs() / len);
                            }
                            let tid = noise::rnd(wi, wj, seed.wrapping_add(555 + t as u32 * 13 + alt as u32 * 3));
                            let val = match omode {
                                1 => 0.5 + (tid - 0.5) * hr,
                                2 => (de * 2.5).min(1.0),
                                _ => l0 * hs[0] + l1 * hs[1] + l2 * hs[2],
                            };
                            let _ = (rot, rd);
                            let cand = (-m, val, de);
                            if cand.0 < best.0 {
                                best = cand;
                            }
                        }
                    }
                }
                let (val, de) = (best.1, best.2);
                if lines && th > 0.0 {
                    let w = th * 6.0;
                    let k = 1.0 - smooth(w * 0.5, w, de);
                    lerp(lerp(val, lb, 0.0), lc, k)
                } else {
                    val
                }
            }))
        }
        "triangle_grid" => {
            let (c, bw) = (per("count", 8.0), f("border", 0.06).clamp(0.0, 0.5));
            let var = f("variation", 1.0).clamp(0.0, 1.0);
            fin(Tex::gray_fn(s, hs, move |u, v| {
                let (x, y) = (u * c as f32, v * c as f32);
                let (ix, iy) = (x.floor(), y.floor());
                let (fx, fy) = (x - ix, y - iy);
                let tri = (fx + fy >= 1.0) as u32;
                let d = fx.min(fy).min(1.0 - fx).min(1.0 - fy).min((fx + fy - 1.0).abs() * 0.7071);
                let id = noise::rnd((ix as i32).rem_euclid(c), (iy as i32).rem_euclid(c), seed.wrapping_add(tri * 131));
                (1.0 - var + var * id) * smooth(0.0, bw.max(1e-4), d)
            }))
        }
        "weave_generator" if f("style", 0.0) > 0.5 => {
            // twill: every thread passes over `weave` cells and under as many, each row shifted by one cell
            let (tx, ty) = (per("tile_x", 1.0).clamp(1, 20), per("tile_y", 1.0).clamp(1, 20));
            let wv = per("weave", 2.0).clamp(1, 10);
            let period = 2 * wv;
            let (nx, ny) = ((tx * period) as f32, (ty * period) as f32);
            let shape = f("shape", 0.5).clamp(0.0, 1.0);
            let (gx, gy) = (f("gap_x", 0.1).clamp(0.0, 0.9) * 0.5, f("gap_y", 0.1).clamp(0.0, 0.9) * 0.5);
            let which = f("_out", 0.0).round() as i32;
            // height along a thread: 0 at the middle of its run over the others, 1 at the ends of that run, 2 at
            // the middle of its run under them
            let height = move |e: f32| -> f32 {
                if e <= 1.0 {
                    let flat = 1.0 - shape;
                    let k = if shape > 0.0 { ((e - flat) / shape).clamp(0.0, 1.0) } else { 0.0 };
                    1.0 - 0.4 * k * k
                } else {
                    0.55 - 0.05 * (e - 1.0)
                }
            };
            let wvf = wv as f32;
            let t = Tex::gray_fn(s, hs, move |u, v| {
                let (x, y) = (u * nx, v * ny);
                let (i, j) = (x.floor() as i32, y.floor() as i32);
                let (fx, fy) = (x - i as f32, y - j as f32);
                // a horizontal thread fills its row except a margin at the top and bottom, a vertical one its column
                let h_here = fy >= gy && fy < 1.0 - gy;
                let v_here = fx >= gx && fx < 1.0 - gx;
                let h_over = (i - j + 1).rem_euclid(period) < wv;
                // distance from the middle of each thread's run over the others, in runs
                let run = |pos: f32, start: i32| -> f32 {
                    let c = start as f32 + wvf * 0.5;
                    let d = (pos - c).rem_euclid(period as f32);
                    d.min(period as f32 - d) / (wvf * 0.5)
                };
                // the horizontal thread of row j is over from cell j - 1 on; the vertical one of column i from row i + 1 - period + 1
                let eh = run(x, j - 1);
                let ev = run(y, i + 1 - period + 1);
                let (show_h, show_v) = match (h_here, v_here) {
                    (true, true) => (h_over, !h_over),
                    (h, v) => (h, v),
                };
                match which {
                    1 => show_h as u32 as f32,
                    2 => show_v as u32 as f32,
                    3 => (h_here || v_here) as u32 as f32,
                    _ => {
                        if show_h {
                            height(eh)
                        } else if show_v {
                            height(ev)
                        } else {
                            0.0
                        }
                    }
                }
            });
            if which == 0 { fin(t) } else { t }
        }
        "weave_generator" => {
            let (tx, ty) = (per("tile_x", 1.0).clamp(1, 20), per("tile_y", 1.0).clamp(1, 20));
            let wv = per("weave", 2.0).clamp(1, 10);
            let (nx, ny) = ((tx * wv * 2) as f32, (ty * wv * 2) as f32);
            let shape = f("shape", 0.5).clamp(0.0, 1.0);
            let (gx, gy) = (f("gap_x", 0.1).clamp(0.0, 0.9), f("gap_y", 0.1).clamp(0.0, 0.9));
            let (nxi, nyi) = (nx as i32, ny as i32);
            let which = f("_out", 0.0).round() as i32;
            let t = Tex::gray_fn(s, hs, move |u, v| {
                let (x, y) = (u * nx, v * ny);
                let (i, j) = (x.floor() as i32, y.floor() as i32);
                let (fx, fy) = (x - i as f32, y - j as f32);
                let (i, j) = (i.rem_euclid(nxi), j.rem_euclid(nyi));
                let in_gx = fx < gx * 0.5 || fx > 1.0 - gx * 0.5;
                let in_gy = fy < gy * 0.5 || fy > 1.0 - gy * 0.5;
                let thread = !in_gx && !in_gy;
                let horizontal = (i + j) & 1 == 0;
                match which {
                    1 => return (thread && horizontal) as u32 as f32,
                    2 => return (thread && !horizontal) as u32 as f32,
                    3 => return thread as u32 as f32,
                    _ => {}
                }
                if in_gx && in_gy {
                    return 0.0;
                }
                if in_gx || in_gy {
                    return 0.5;
                }
                // a thread runs along the slot; it dips towards both ends where it passes under its neighbour
                let along = if horizontal { fx } else { fy };
                let e = (along - 0.5).abs() * 2.0;
                if shape <= 0.0 { 1.0 } else { 1.0 - 0.5 * smooth(1.0 - shape, 1.0, e) }
            });
            if which == 0 { fin(t) } else { t }
        }
        "starburst" if f("style", 0.0) > 0.5 => {
            // light rays: blurred lines from the centre whose brightness falls off along their length
            let (c, rays) = (per("count", 3.0), f("rays", 8.0).round().clamp(2.0, 512.0));
            let sig = (f("blur", 2.0).max(0.0) * 0.0045).max(0.6 / s as f32 * c as f32);
            let (smax, smin, svar) = (f("size", 1.0).max(0.0), f("size_min", 0.05).max(0.0), f("size_variation", 0.0).clamp(0.0, 1.0));
            let (rot, rnd_a) = (f("rotation", 0.0), f("randomness", 0.0).clamp(0.0, 1.0));
            let nr = rays as i32;
            let sp = TAU / rays;
            fin(Tex::gray_fn(s, hs, move |u, v| {
                let (x, y) = (u * c as f32, v * c as f32);
                let (dx, dy) = (x - x.floor() - 0.5, y - y.floor() - 0.5);
                let r = (dx * dx + dy * dy).sqrt();
                let th = dy.atan2(dx) - rot * TAU;
                let k0 = (th / sp).round() as i32;
                let mut keep = 1.0f32;
                for dk in -3..=3 {
                    let k = k0 + dk;
                    let kk = k.rem_euclid(nr);
                    let ang = k as f32 * sp + (crate::noise_styles::unit(kk, 11, seed) - 0.5) * sp * rnd_a;
                    let len = 0.35 * (smax + (smin - smax) * svar * crate::noise_styles::unit(kk, 23, seed));
                    let (sn, cs) = (th - ang).sin_cos();
                    if cs <= 0.0 || len <= 0.0 {
                        continue;
                    }
                    let d = r * sn.abs();
                    let i = (1.0 - r / len).clamp(0.0, 1.0);
                    let g = i * (-0.5 * (d / sig).powi(2)).exp();
                    keep *= 1.0 - g;
                }
                1.0 - keep
            }))
        }
        "starburst" => {
            let (c, rays) = (per("count", 3.0), f("rays", 8.0).round().clamp(2.0, 512.0));
            let (sharp, size, rot) = (f("sharpness", 2.0).clamp(0.1, 16.0), f("size", 1.0).clamp(0.05, 2.0), f("rotation", 0.0));
            let rr = f("rotation_random", 0.0).clamp(0.0, 1.0);
            fin(Tex::gray_fn(s, hs, move |u, v| {
                let (x, y) = (u * c as f32, v * c as f32);
                let (ix, iy) = (x.floor(), y.floor());
                let (dx, dy) = (x - ix - 0.5, y - iy - 0.5);
                let r = (dx * dx + dy * dy).sqrt();
                let off = rot + rr * noise::rnd((ix as i32).rem_euclid(c), (iy as i32).rem_euclid(c), seed);
                let a = (dy.atan2(dx) / TAU + off) * rays;
                let ray = 1.0 - (2.0 * (a - a.floor()) - 1.0).abs();
                ray.powf(sharp) * (1.0 - r * 2.0 / size).clamp(0.0, 1.0)
            }))
        }
        "gradient_reflected" => {
            let (a, b) = ops::tileable_wave(f("count", 1.0), f("angle", 0.0));
            fin(Tex::gray_fn(s, hs, move |u, v| {
                let ph = a as f32 * u + b as f32 * v;
                1.0 - (2.0 * (ph - ph.floor()) - 1.0).abs()
            }))
        }
        "shape_stroke" | "shape_glow" => {
            let t = need("in")?;
            let w = (f("width", 8.0).clamp(0.5, 256.0) * rs).max(2.0);
            let sdist = ops::signed_distance(t, f("threshold", 0.5));
            let mask: Vec<bool> = sdist.iter().map(|d| *d > 0.0).collect();
            let stroke = kind == "shape_stroke";
            let (off, soft, inten) = (f("offset", 0.0).clamp(-1.0, 1.0), f("softness", 0.5).clamp(0.0, 1.0), f("intensity", 1.0).clamp(0.0, 4.0));
            let (opacity, spread) = (f("opacity", 1.0).clamp(0.0, 1.0), f("spread", 0.0).clamp(0.0, 1.0));
            let col = [f("color_r", 1.0), f("color_g", 1.0), f("color_b", 1.0)];
            let px = (0..mask.len())
                .map(|i| {
                    // signed distance in pixels: positive outside the shape
                    let sd = -sdist[i].clamp(-1e8, 1e8);
                    let g = if stroke {
                        let c = off * w * 0.5;
                        1.0 - smooth(w * 0.5 * (1.0 - soft), w * 0.5 + 0.5, (sd - c).abs())
                    } else if mask[i] {
                        1.0
                    } else {
                        (1.0 - (sd / w).clamp(0.0, 1.0)).powf(1.0 + soft * 3.0) * inten
                    };
                    let g = if stroke { g } else { g / (1.0 - spread * 0.99) };
                    let g = g.clamp(0.0, 1.0) * opacity;
                    [g * col[0], g * col[1], g * col[2], 1.0]
                })
                .collect();
            Tex { w: t.w, h: t.h, px }
        }
        "chrominance_extract" => need("in")?.map(|p| {
            let l = lum(p).max(1e-3);
            [(p[0] / l * 0.5).clamp(0.0, 1.0), (p[1] / l * 0.5).clamp(0.0, 1.0), (p[2] / l * 0.5).clamp(0.0, 1.0), p[3]]
        }),
        _ => return Ok(None),
    };
    Ok(Some(out))
}
