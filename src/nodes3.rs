// SPDX-License-Identifier: MIT OR Apache-2.0
//! Third set of nodes: adjustments, transforms and normal-map tools. `compute` returns Ok(None) for kinds it does not
//! know. Every node keeps the result tileable (by construction, or by `ops::seamless_blend` when the operation cannot
//! be periodic) and works on non-square graphs; pixel sizes are given for a 512 px texture and scaled by `size / 512`.
use crate::graph::{Error, Node};
use crate::ops;
use crate::tex::{lum, par_rows, Tex};
use std::collections::{BTreeMap, VecDeque};
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::sync::Arc as Rc;

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let d = b - a;
    let t = if d.abs() < 1e-9 { if x >= b { 1.0 } else { 0.0 } } else { ((x - a) / d).clamp(0.0, 1.0) };
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn mix4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t), lerp(a[3], b[3], t)]
}

/// sRGB transfer curve, decoded. Negative values are treated as black; values above 1 (HDR) continue the curve.
pub fn srgb_to_linear(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

// ---------------------------------------------------------------- morphology (sliding min / max, wrapped)
/// Minimum or maximum over a window of `2r + 1` samples centred on each element of a wrapped row.
fn slide(row: &[f32], r: usize, max: bool) -> Vec<f32> {
    let n = row.len();
    if n == 0 || r == 0 {
        return row.to_vec();
    }
    let r = r.min((n - 1) / 2);
    let total = n + 2 * r;
    let at = |i: usize| row[(i + n - r % n) % n];
    let better = |a: f32, b: f32| if max { a >= b } else { a <= b };
    let mut out = Vec::with_capacity(n);
    let mut dq: VecDeque<usize> = VecDeque::new();
    for i in 0..total {
        let v = at(i);
        while let Some(&b) = dq.back() {
            if better(v, at(b)) {
                dq.pop_back();
            } else {
                break;
            }
        }
        dq.push_back(i);
        if i >= 2 * r {
            let lo = i - 2 * r;
            while let Some(&f) = dq.front() {
                if f < lo {
                    dq.pop_front();
                } else {
                    break;
                }
            }
            out.push(at(*dq.front().unwrap()));
        }
    }
    out
}

fn pass_h(t: &Tex, r: usize, max: bool) -> Tex {
    let w = t.w;
    let mut out = t.clone();
    par_rows(&mut out.px, w, |start, chunk| {
        for (ri, row) in chunk.chunks_mut(w).enumerate() {
            let y = start / w + ri;
            for c in 0..4 {
                let line: Vec<f32> = (0..w).map(|x| t.px[y * w + x][c]).collect();
                let res = slide(&line, r, max);
                for x in 0..w {
                    row[x][c] = res[x];
                }
            }
        }
    });
    out
}

fn transpose(t: &Tex) -> Tex {
    let mut px = vec![[0.0f32; 4]; t.px.len()];
    for y in 0..t.h {
        for x in 0..t.w {
            px[x * t.h + y] = t.px[y * t.w + x];
        }
    }
    Tex { w: t.h, h: t.w, px }
}

fn box_extreme(t: &Tex, rx: usize, ry: usize, max: bool) -> Tex {
    let a = pass_h(t, rx, max);
    transpose(&pass_h(&transpose(&a), ry, max))
}

// ---------------------------------------------------------------- histogram
const BINS: usize = 1024;

/// Equalising curve of a set of values in 0..1: returns BINS samples of (cdf - cdf_min) / (1 - cdf_min).
fn equalize_curve(vals: impl Iterator<Item = f32>) -> Option<Vec<f32>> {
    let mut hist = vec![0.0f64; BINS];
    let mut n = 0.0f64;
    for v in vals {
        let b = (v.clamp(0.0, 1.0) * (BINS - 1) as f32).round() as usize;
        hist[b] += 1.0;
        n += 1.0;
    }
    if n == 0.0 {
        return None;
    }
    let mut acc = 0.0;
    let mut cdf = vec![0.0f64; BINS];
    for i in 0..BINS {
        acc += hist[i];
        cdf[i] = acc / n;
    }
    let min = cdf.iter().copied().find(|c| *c > 0.0).unwrap_or(0.0);
    if 1.0 - min < 1e-9 {
        return None;
    }
    Some(cdf.iter().map(|c| (((*c - min) / (1.0 - min)).clamp(0.0, 1.0)) as f32).collect())
}

fn curve_at(c: &[f32], v: f32) -> f32 {
    let x = v.clamp(0.0, 1.0) * (c.len() - 1) as f32;
    let i = x.floor() as usize;
    let j = (i + 1).min(c.len() - 1);
    lerp(c[i], c[j], x - i as f32)
}

// ---------------------------------------------------------------- homography (quad transforms)
/// Projective map taking the four points `from` to the four points `to`: returns [a b c d e f g h] with
/// x' = (a x + b y + c) / (g x + h y + 1), y' = (d x + e y + f) / (g x + h y + 1). None when degenerate.
pub(crate) fn homography(from: [[f32; 2]; 4], to: [[f32; 2]; 4]) -> Option<[f32; 8]> {
    let mut m = [[0.0f64; 9]; 8];
    for i in 0..4 {
        let (x, y) = (from[i][0] as f64, from[i][1] as f64);
        let (u, v) = (to[i][0] as f64, to[i][1] as f64);
        m[2 * i] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
        m[2 * i + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
    }
    for col in 0..8 {
        let mut piv = col;
        for r in col + 1..8 {
            if m[r][col].abs() > m[piv][col].abs() {
                piv = r;
            }
        }
        if m[piv][col].abs() < 1e-10 {
            return None;
        }
        m.swap(col, piv);
        for r in 0..8 {
            if r != col {
                let k = m[r][col] / m[col][col];
                for c in col..9 {
                    m[r][c] -= k * m[col][c];
                }
            }
        }
    }
    let mut h = [0.0f32; 8];
    for i in 0..8 {
        h[i] = (m[i][8] / m[i][i]) as f32;
    }
    if h.iter().all(|v| v.is_finite()) {
        Some(h)
    } else {
        None
    }
}

/// Warp `t` so its corners (top-left, top-right, bottom-right, bottom-left, in uv) land on `quad`.
fn quad_warp(t: &Tex, quad: [[f32; 2]; 4], tile: bool) -> Tex {
    let unit = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    if quad.iter().zip(&unit).all(|(a, b)| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6) {
        return t.clone();
    }
    let Some(h) = homography(quad, unit) else { return t.clone() };
    let out = Tex::from_fn(t.w, t.h, |u, v| {
        let den = h[6] * u + h[7] * v + 1.0;
        if den.abs() < 1e-5 {
            return t.sample(u, v);
        }
        t.sample_cubic((h[0] * u + h[1] * v + h[2]) / den, (h[3] * u + h[4] * v + h[5]) / den)
    });
    if tile {
        ops::seamless_blend(&out)
    } else {
        out
    }
}

// ---------------------------------------------------------------- normal helpers
fn dec(p: [f32; 4]) -> [f32; 3] {
    [(p[0] - 0.5) * 2.0, (p[1] - 0.5) * 2.0, (p[2] - 0.5) * 2.0]
}

fn enc(v: [f32; 3], a: f32) -> [f32; 4] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l < 1e-8 {
        return [0.5, 0.5, 1.0, a];
    }
    [v[0] / l * 0.5 + 0.5, v[1] / l * 0.5 + 0.5, v[2] / l * 0.5 + 0.5, a]
}

fn gray_plane(t: &Tex) -> Vec<f32> {
    t.px.iter().map(|p| lum(*p)).collect()
}

/// Run `f(x, y)` for every pixel in parallel.
fn per_pixel<F: Fn(usize, usize) -> [f32; 4] + Sync>(w: usize, h: usize, f: F) -> Tex {
    let mut out = Tex { w, h, px: vec![[0.0, 0.0, 0.0, 1.0]; w * h] };
    par_rows(&mut out.px, w, |start, chunk| {
        for (j, dst) in chunk.iter_mut().enumerate() {
            let i = start + j;
            *dst = f(i % w, i / w);
        }
    });
    out
}

/// Run a node of this module. Ok(None) = not one of ours.
pub fn compute(kind: &str, id: u32, n: &Node, ins: &BTreeMap<String, Rc<Tex>>, s: usize, hs: usize, _seed: u32) -> Result<Option<Tex>, Error> {
    let f = |k: &str, d: f32| n.nums.get(k).copied().filter(|v| v.is_finite()).unwrap_or(d);
    let need = |slot: &str| -> Result<&Tex, Error> { ins.get(slot).map(|r| r.as_ref()).ok_or_else(|| Error::MissingInput(id, slot.to_string())) };
    let rs = s as f32 / 512.0;
    let out_idx = f("_out", 0.0).round() as usize;
    let out = match kind {
        // ------------------------------------------------ adjustments
        "contrast_luminosity" => {
            let (con, lumi) = (f("contrast", 1.0).clamp(-16.0, 16.0), f("luminosity", 0.0).clamp(-8.0, 8.0));
            need("in")?.map(|p| [(p[0] - 0.5) * con + 0.5 + lumi, (p[1] - 0.5) * con + 0.5 + lumi, (p[2] - 0.5) * con + 0.5 + lumi, p[3]])
        }
        "srgb_to_linear" => need("in")?.map(|p| [srgb_to_linear(p[0]), srgb_to_linear(p[1]), srgb_to_linear(p[2]), p[3]]),
        "linear_to_srgb" => need("in")?.map(|p| [linear_to_srgb(p[0]), linear_to_srgb(p[1]), linear_to_srgb(p[2]), p[3]]),
        "premultiply" => {
            if f("mode", 0.0) > 0.5 {
                need("in")?.map(|p| if p[3] > 1e-6 { [p[0] / p[3], p[1] / p[3], p[2] / p[3], p[3]] } else { [0.0, 0.0, 0.0, p[3]] })
            } else {
                need("in")?.map(|p| [p[0] * p[3], p[1] * p[3], p[2] * p[3], p[3]])
            }
        }
        "histogram_equalize" => {
            let t = need("in")?;
            let amount = f("amount", 1.0).clamp(0.0, 1.0);
            if f("per_channel", 0.0) > 0.5 {
                let curves: Vec<Option<Vec<f32>>> = (0..3).map(|c| equalize_curve(t.px.iter().map(|p| p[c]))).collect();
                t.map(|p| {
                    let mut o = p;
                    for c in 0..3 {
                        if let Some(cv) = &curves[c] {
                            o[c] = lerp(p[c], curve_at(cv, p[c]), amount);
                        }
                    }
                    o
                })
            } else {
                match equalize_curve(t.px.iter().map(|p| lum(*p))) {
                    Some(cv) => t.map(|p| {
                        let l = lum(p);
                        let d = (curve_at(&cv, l) - l) * amount;
                        [p[0] + d, p[1] + d, p[2] + d, p[3]]
                    }),
                    None => t.clone(),
                }
            }
        }
        "min_max" => {
            let (a, b) = (need("a")?, need("b")?);
            let max = f("mode", 1.0) > 0.5;
            Tex::from_fn(a.w, a.h, |u, v| {
                let (p, q) = (a.sample(u, v), b.sample(u, v));
                if max {
                    [p[0].max(q[0]), p[1].max(q[1]), p[2].max(q[2]), p[3].max(q[3])]
                } else {
                    [p[0].min(q[0]), p[1].min(q[1]), p[2].min(q[2]), p[3].min(q[3])]
                }
            })
        }
        "morphology" => {
            let t = need("in")?;
            let r = (f("radius", 4.0).clamp(0.0, 128.0) * rs).round() as usize;
            let (rx, ry) = (r, r);
            match f("mode", 0.0).round() as i32 {
                1 => box_extreme(t, rx, ry, false),
                2 => box_extreme(&box_extreme(t, rx, ry, false), rx, ry, true),
                3 => box_extreme(&box_extreme(t, rx, ry, true), rx, ry, false),
                _ => box_extreme(t, rx, ry, true),
            }
        }
        "multi_switch" => {
            const NAMES: [&str; 8] = ["a", "b", "c", "d", "e", "f", "g", "h"];
            let idx = (f("index", 0.0).round().clamp(0.0, 7.0)) as usize;
            let pick = ins.get(NAMES[idx]).or_else(|| NAMES.iter().find_map(|k| ins.get(*k)));
            match pick {
                Some(t) => (**t).clone(),
                None => Tex::solid(s, hs, [0.0, 0.0, 0.0, 1.0]),
            }
        }
        "lookup_table" => {
            let (t, lut) = (need("in")?, need("lut")?);
            let (lw, lh) = (lut.w, lut.h);
            // the line of the lookup image that is read: a row, or a column when `vertical`
            let vertical = f("vertical", 0.0) > 0.5;
            let repeat = f("repeat", 0.0) > 0.5;
            let (len, across) = if vertical { (lh, lw) } else { (lw, lh) };
            let line = ((f("row", 0.5).clamp(0.0, 1.0) * across as f32) as usize).min(across - 1);
            let px = |k: usize| if vertical { lut.px[k * lw + line] } else { lut.px[line * lw + k] };
            let at = |x: f32| -> [f32; 4] {
                let x = if repeat { x - x.floor() } else { x.clamp(0.0, 1.0) };
                let fx = x * (len - 1) as f32;
                let i = (fx.floor() as usize).min(len - 1);
                let j = (i + 1).min(len - 1);
                mix4(px(i), px(j), fx - i as f32)
            };
            if f("mode", 0.0) > 0.5 {
                t.map(|p| {
                    let c = at(lum(p));
                    [c[0], c[1], c[2], p[3]]
                })
            } else {
                t.map(|p| [at(p[0])[0], at(p[1])[1], at(p[2])[2], p[3]])
            }
        }
        "frequency_split" => {
            let t = need("in")?;
            let low = ops::blur(t, f("radius", 8.0).clamp(0.0, 128.0) * rs);
            if out_idx == 0 {
                low
            } else {
                let mut o = t.clone();
                par_rows(&mut o.px, t.w, |start, chunk| {
                    for (j, p) in chunk.iter_mut().enumerate() {
                        let l = low.px[start + j];
                        *p = [p[0] - l[0] + 0.5, p[1] - l[1] + 0.5, p[2] - l[2] + 0.5, p[3]];
                    }
                });
                o
            }
        }
        "frequency_merge" => {
            let (low, high) = (need("low")?, need("high")?);
            let gain = f("gain", 1.0).clamp(0.0, 16.0);
            Tex::from_fn(low.w, low.h, |u, v| {
                let (l, h) = (low.sample(u, v), high.sample(u, v));
                [l[0] + (h[0] - 0.5) * gain, l[1] + (h[1] - 0.5) * gain, l[2] + (h[2] - 0.5) * gain, l[3]]
            })
        }

        // ------------------------------------------------ transforms
        "auto_crop" => {
            let t = need("in")?;
            let thr = f("threshold", 0.001).clamp(0.0, 1.0);
            let (mut x0, mut y0, mut x1, mut y1) = (t.w, t.h, 0usize, 0usize);
            for y in 0..t.h {
                for x in 0..t.w {
                    let p = t.px[y * t.w + x];
                    if p[0].max(p[1]).max(p[2]) > thr {
                        x0 = x0.min(x);
                        y0 = y0.min(y);
                        x1 = x1.max(x + 1);
                        y1 = y1.max(y + 1);
                    }
                }
            }
            if x1 <= x0 || y1 <= y0 {
                t.clone()
            } else {
                let (mut u0, mut u1) = (x0 as f32 / t.w as f32, x1 as f32 / t.w as f32);
                let (mut v0, mut v1) = (y0 as f32 / t.h as f32, y1 as f32 / t.h as f32);
                let m = f("margin", 0.0).clamp(0.0, 0.4);
                u0 -= m;
                u1 += m;
                v0 -= m;
                v1 += m;
                if f("keep_aspect", 0.0) > 0.5 {
                    // grow the smaller side (in pixels) so the cropped area keeps the graph's aspect ratio
                    let (pw, ph) = ((u1 - u0) * t.w as f32, (v1 - v0) * t.h as f32);
                    let want = t.w as f32 / t.h as f32;
                    if pw / ph.max(1e-6) > want {
                        let grow = pw / want / t.h as f32 - (v1 - v0);
                        v0 -= grow * 0.5;
                        v1 += grow * 0.5;
                    } else {
                        let grow = ph * want / t.w as f32 - (u1 - u0);
                        u0 -= grow * 0.5;
                        u1 += grow * 0.5;
                    }
                }
                Tex::from_fn(t.w, t.h, |u, v| t.sample(lerp(u0, u1, u), lerp(v0, v1, v)))
            }
        }
        "polar_convert" => {
            let t = need("in")?;
            let (radius, rot) = (f("radius", 1.0).clamp(0.05, 2.0), f("rotation", 0.0));
            let out = if f("mode", 0.0) < 0.5 {
                // Cartesian -> polar: u is the angle, v the distance from the centre (mirrored about v = 0.5)
                Tex::from_fn(t.w, t.h, |u, v| {
                    let a = (u + rot) * TAU;
                    let r = 0.5 * radius * (1.0 - 2.0 * v).abs();
                    t.sample_cubic(0.5 + r * a.cos(), 0.5 + r * a.sin())
                })
            } else {
                // polar -> Cartesian
                Tex::from_fn(t.w, t.h, |u, v| {
                    let (dx, dy) = (u - 0.5, v - 0.5);
                    let rn = (dx * dx + dy * dy).sqrt() * 2.0 / radius;
                    let a = dy.atan2(dx) / TAU - rot;
                    t.sample_cubic(a, (1.0 - rn.min(1.0)) * 0.5)
                })
            };
            if f("tile", 1.0) > 0.5 {
                ops::seamless_blend(&out)
            } else {
                out
            }
        }
        "clone_patch" => {
            let t = need("in")?;
            let (sx, sy, dx, dy) = (f("src_x", 0.25), f("src_y", 0.25), f("dst_x", 0.75), f("dst_y", 0.75));
            let (pw, ph) = (f("width", 0.3).clamp(0.0, 1.0), f("height", 0.3).clamp(0.0, 1.0));
            let (feather, op) = (f("feather", 0.25).clamp(0.0, 1.0), f("opacity", 1.0).clamp(0.0, 1.0));
            let (sr, cr) = (f("rotation", 0.0) * TAU).sin_cos();
            Tex::from_fn(t.w, t.h, |u, v| {
                let (lx, ly) = (ops::wrapd(u - dx), ops::wrapd(v - dy));
                let p = t.sample(u, v);
                if lx.abs() >= pw * 0.5 || ly.abs() >= ph * 0.5 {
                    return p;
                }
                let e = ((pw * 0.5 - lx.abs()) / (pw * 0.5)).min((ph * 0.5 - ly.abs()) / (ph * 0.5));
                let wgt = smooth(0.0, feather.max(1e-4), e) * op;
                let q = t.sample(sx + cr * lx - sr * ly, sy + sr * lx + cr * ly);
                mix4(p, q, wgt)
            })
        }
        "noise_upscale" => {
            let t = need("in")?;
            let cx = f("cells", 16.0).round().clamp(2.0, 512.0) as usize;
            let cy = ((cx as f32 * t.h as f32 / t.w as f32).round() as usize).max(1);
            let small = Tex::from_fn(cx, cy, |u, v| {
                let mut a = [0.0f32; 4];
                for j in 0..4 {
                    for i in 0..4 {
                        let p = t.sample(u + ((i as f32 + 0.5) / 4.0 - 0.5) / cx as f32, v + ((j as f32 + 0.5) / 4.0 - 0.5) / cy as f32);
                        for c in 0..4 {
                            a[c] += p[c] / 16.0;
                        }
                    }
                }
                a
            });
            match f("mode", 0.0).round() as i32 {
                1 => Tex::from_fn(t.w, t.h, |u, v| small.sample(u, v)),
                2 => Tex::from_fn(t.w, t.h, |u, v| small.at((u * cx as f32).floor() as i64, (v * cy as f32).floor() as i64)),
                _ => Tex::from_fn(t.w, t.h, |u, v| small.sample_cubic(u, v)),
            }
        }
        "symmetry_slice" => {
            let t = need("in")?;
            let slices = f("slices", 6.0).round().clamp(1.0, 64.0);
            let (mirror, rot) = (f("mirror", 1.0) > 0.5, f("rotation", 0.0));
            let out = Tex::from_fn(t.w, t.h, |u, v| {
                let (dx, dy) = (u - 0.5, v - 0.5);
                let r = (dx * dx + dy * dy).sqrt();
                let a = (dy.atan2(dx) / TAU - rot).rem_euclid(1.0) * slices;
                let k = a.floor();
                let mut local = a - k;
                if mirror && (k as i64) % 2 != 0 {
                    local = 1.0 - local;
                }
                let ang = (local / slices + rot) * TAU;
                t.sample_cubic(0.5 + r * ang.cos(), 0.5 + r * ang.sin())
            });
            if f("tile", 1.0) > 0.5 {
                ops::seamless_blend(&out)
            } else {
                out
            }
        }
        "quad_transform" => {
            let t = need("in")?;
            let o = |k: &str| f(k, 0.0).clamp(-0.45, 0.45);
            let quad = [
                [o("tl_x"), o("tl_y")],
                [1.0 + o("tr_x"), o("tr_y")],
                [1.0 + o("br_x"), 1.0 + o("br_y")],
                [o("bl_x"), 1.0 + o("bl_y")],
            ];
            quad_warp(t, quad, f("tile", 1.0) > 0.5)
        }
        "trapezoid_transform" => {
            let t = need("in")?;
            let (top, bot) = (f("top", 0.6).clamp(0.05, 2.0), f("bottom", 1.0).clamp(0.05, 2.0));
            let skew = f("skew", 0.0).clamp(-0.5, 0.5);
            let quad = [[0.5 - top * 0.5 + skew, 0.0], [0.5 + top * 0.5 + skew, 0.0], [0.5 + bot * 0.5, 1.0], [0.5 - bot * 0.5, 1.0]];
            quad_warp(t, quad, f("tile", 1.0) > 0.5)
        }

        // ------------------------------------------------ normal tools
        "bent_normal" => {
            let t = need("in")?;
            let dirs = f("directions", 16.0).round().clamp(4.0, 32.0) as usize;
            let steps = f("steps", 8.0).round().clamp(2.0, 16.0) as usize;
            let rad = f("radius", 16.0).clamp(1.0, 256.0) / 512.0;
            let relief = f("relief", 0.1).clamp(0.0, 10.0);
            let directx = f("directx", 0.0) > 0.5;
            let want_ao = out_idx == 1;
            if let Some(g) = crate::gpu::bent_normal(t, dirs as u32, steps as u32, rad, relief, directx, want_ao) {
                return Ok(Some(g));
            }
            let g = Tex { w: t.w, h: t.h, px: gray_plane(t).into_iter().map(|v| [v, v, v, 1.0]).collect() };
            let aspect = t.w as f32 / t.h as f32;
            let cs: Vec<(f32, f32)> = (0..dirs).map(|i| ((i as f32 + 0.5) / dirs as f32 * TAU).sin_cos()).collect();
            per_pixel(t.w, t.h, |x, y| {
                let (u, v) = ((x as f32 + 0.5) / t.w as f32, (y as f32 + 0.5) / t.h as f32);
                let h0 = g.px[y * t.w + x][0];
                let mut acc = [0.0f32; 3];
                let mut ao = 0.0f32;
                for &(sn, cn) in &cs {
                    let mut tanmax = 0.0f32;
                    for k in 1..=steps {
                        let d = rad * k as f32 / steps as f32;
                        let hs = g.sample(u + cn * d, v + sn * d * aspect)[0];
                        tanmax = tanmax.max((hs - h0) * relief / d);
                    }
                    let hz = tanmax.atan();
                    let theta = (hz + FRAC_PI_2) * 0.5;
                    let vis = (FRAC_PI_2 - hz) / FRAC_PI_2;
                    let ysign = if directx { 1.0 } else { -1.0 };
                    acc[0] += vis * cn * theta.cos();
                    acc[1] += vis * ysign * sn * theta.cos();
                    acc[2] += vis * theta.sin();
                    ao += hz.cos() * hz.cos();
                }
                if want_ao {
                    let a = (ao / dirs as f32).clamp(0.0, 1.0);
                    [a, a, a, 1.0]
                } else {
                    enc(acc, 1.0)
                }
            })
        }
        "normal_facing" => {
            let t = need("in")?;
            let (az, tilt) = (f("angle", 0.0) * TAU, f("tilt", 0.0).clamp(0.0, 1.0) * FRAC_PI_2);
            let d = [tilt.sin() * az.cos(), tilt.sin() * az.sin(), tilt.cos()];
            let sharp = f("sharpness", 1.0).clamp(0.05, 64.0);
            t.map(|p| {
                let nv = dec(p);
                let l = (nv[0] * nv[0] + nv[1] * nv[1] + nv[2] * nv[2]).sqrt().max(1e-6);
                let dot = (nv[0] * d[0] + nv[1] * d[1] + nv[2] * d[2]) / l;
                let g = dot.max(0.0).powf(sharp);
                [g, g, g, 1.0]
            })
        }
        "height_normal_blend" => {
            let (nm, ht) = (need("normal")?, need("height")?);
            let directx = f("directx", 0.0) > 0.5;
            let detail = ops::normal_from_height(ht, f("strength", 2.0).clamp(0.0, 64.0), directx);
            let opacity = f("opacity", 1.0).clamp(0.0, 1.0);
            let mode = f("mode", 0.0).round() as i32;
            Tex::from_fn(nm.w, nm.h, |u, v| {
                let p = nm.sample(u, v);
                let (a, b) = (dec(p), dec(detail.sample(u, v)));
                let r = match mode {
                    // reoriented: rotate the detail normal onto the base normal
                    0 => {
                        let t = [a[0], a[1], a[2] + 1.0];
                        let uu = [-b[0], -b[1], b[2]];
                        let k = (t[0] * uu[0] + t[1] * uu[1] + t[2] * uu[2]) / t[2].max(1e-4);
                        [t[0] * k - uu[0], t[1] * k - uu[1], t[2] * k - uu[2]]
                    }
                    // whiteout
                    1 => [a[0] + b[0], a[1] + b[1], a[2] * b[2]],
                    // partial derivatives
                    _ => [a[0] / a[2].max(1e-4) + b[0] / b[2].max(1e-4), a[1] / a[2].max(1e-4) + b[1] / b[2].max(1e-4), 1.0],
                };
                let e = enc(r, p[3]);
                mix4(p, e, opacity)
            })
        }
        "height_to_normal_world" => {
            let t = need("in")?;
            let (size_cm, height_cm) = (f("size", 100.0).clamp(0.01, 1e6), f("height", 1.0).clamp(0.0, 1e6));
            let directx = f("directx", 0.0) > 0.5;
            let gray = gray_plane(t);
            let (w, h) = (t.w, t.h);
            let pitch = size_cm / w as f32;
            per_pixel(w, h, |x, y| {
                let (xm, xp) = ((x + w - 1) % w, (x + 1) % w);
                let (ym, yp) = ((y + h - 1) % h, (y + 1) % h);
                let dx = (gray[y * w + xp] - gray[y * w + xm]) * 0.5 * height_cm / pitch;
                let mut dy = (gray[yp * w + x] - gray[ym * w + x]) * 0.5 * height_cm / pitch;
                if directx {
                    dy = -dy;
                }
                enc([-dx, dy, 1.0], 1.0)
            })
        }
        "normal_transform" => {
            let t = need("in")?;
            let rot = f("rotation", 0.0);
            let moved = ops::transform(t, f("offset_x", 0.0), f("offset_y", 0.0), f("scale_x", 1.0), f("scale_y", 1.0), rot, true);
            if f("rotate_vectors", 1.0) > 0.5 {
                let (sn, cs) = (rot * TAU).sin_cos();
                let sgn = if f("directx", 0.0) > 0.5 { -1.0 } else { 1.0 };
                moved.map(|p| {
                    let (nx, ny) = ((p[0] - 0.5) * 2.0, (p[1] - 0.5) * 2.0 * sgn);
                    let (rx, ry) = (cs * nx + sn * ny, -sn * nx + cs * ny);
                    enc([rx, ry * sgn, (p[2] - 0.5) * 2.0], p[3])
                })
            } else {
                moved
            }
        }
        "normal_sobel" => {
            let t = need("in")?;
            let (w, h) = (t.w, t.h);
            let inten = f("intensity", 4.0).clamp(0.0, 256.0);
            let curv = f("mode", 0.0) > 0.5;
            let ysgn = if f("directx", 0.0) > 0.5 { 1.0 } else { -1.0 };
            let v: Vec<[f32; 3]> = t.px.iter().map(|p| dec(*p)).collect();
            per_pixel(w, h, |x, y| {
                let g = |dx: i64, dy: i64| v[((y as i64 + dy).rem_euclid(h as i64) as usize) * w + (x as i64 + dx).rem_euclid(w as i64) as usize];
                let mut gx = [0.0f32; 3];
                let mut gy = [0.0f32; 3];
                for c in 0..3 {
                    gx[c] = (g(1, -1)[c] - g(-1, -1)[c] + 2.0 * (g(1, 0)[c] - g(-1, 0)[c]) + g(1, 1)[c] - g(-1, 1)[c]) / 8.0;
                    gy[c] = (g(-1, 1)[c] - g(-1, -1)[c] + 2.0 * (g(0, 1)[c] - g(0, -1)[c]) + g(1, 1)[c] - g(1, -1)[c]) / 8.0;
                }
                if curv {
                    // divergence of the slope field: positive on ridges and bumps, negative in grooves
                    let c = (gx[0] + ysgn * gy[1]) * inten * 0.5 + 0.5;
                    [c, c, c, 1.0]
                } else {
                    let m = (0..3).map(|c| gx[c] * gx[c] + gy[c] * gy[c]).sum::<f32>().sqrt() * inten;
                    [m, m, m, 1.0]
                }
            })
        }
        _ => return Ok(None),
    };
    let _ = PI;
    Ok(Some(out))
}
