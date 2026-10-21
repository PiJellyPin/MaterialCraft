// SPDX-License-Identifier: MIT OR Apache-2.0
//! Eighth set of nodes: effects, transforms and material filters that complete the node library: cross section,
//! uber emboss, diffusion (pull-push fill), summed area table, vector morph, safe transform, snow, season and water.
//! Everything tiles and works on non-square graphs; pixel sizes are given for a 512 px texture and scaled by
//! `size / 512`. `compute` returns Ok(None) for kinds it does not know and the hidden `_out` number selects one of
//! several outputs.
use crate::graph::{Error, Node};
use crate::noise::{self, Fbm};
use crate::ops;
use crate::tex::{lum, par_rows, Tex};
use std::collections::BTreeMap;
use std::f32::consts::TAU;
use std::sync::Arc as Rc;

fn grey(v: f32) -> [f32; 4] {
    [v, v, v, 1.0]
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let d = b - a;
    let t = if d.abs() < 1e-9 { if x >= b { 1.0 } else { 0.0 } } else { ((x - a) / d).clamp(0.0, 1.0) };
    t * t * (3.0 - 2.0 * t)
}

fn periods(scale: f32, w: usize, h: usize) -> (i32, i32) {
    let px = scale.round().clamp(1.0, 256.0) as i32;
    (px, ((px as f32 * h as f32 / w as f32).round() as i32).max(1))
}

/// Height gradient per pixel at the 512 reference size (central differences, wrapping).
fn gradient(h: &Tex, x: i64, y: i64, rs: f32) -> (f32, f32) {
    let g = |dx: i64, dy: i64| lum(h.at(x + dx, y + dy));
    (((g(1, 0) - g(-1, 0)) * 0.5) * rs, ((g(0, 1) - g(0, -1)) * 0.5) * rs)
}

/// Fill the unknown parts of an image from the known ones (pull-push over a mip chain). `known` is 1 where the
/// colour is valid. The result tiles, and known pixels are kept.
fn diffuse(color: &[[f32; 4]], known: &[f32], w: usize, h: usize) -> Vec<[f32; 4]> {
    struct Lvl {
        w: usize,
        h: usize,
        c: Vec<[f32; 3]>,
        wt: Vec<f32>,
    }
    let mut lv = vec![Lvl { w, h, c: color.iter().map(|p| [p[0], p[1], p[2]]).collect(), wt: known.to_vec() }];
    while lv.last().map(|l| l.w > 1 || l.h > 1).unwrap_or(false) {
        let p = lv.last().unwrap();
        let (nw, nh) = (((p.w + 1) / 2).max(1), ((p.h + 1) / 2).max(1));
        let mut c = vec![[0.0f32; 3]; nw * nh];
        let mut wt = vec![0.0f32; nw * nh];
        for y in 0..nh {
            for x in 0..nw {
                let (mut acc, mut tot) = ([0.0f32; 3], 0.0f32);
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (sx, sy) = ((2 * x + dx) % p.w, (2 * y + dy) % p.h);
                    let k = sy * p.w + sx;
                    for i in 0..3 {
                        acc[i] += p.c[k][i] * p.wt[k];
                    }
                    tot += p.wt[k];
                }
                let k = y * nw + x;
                if tot > 1e-6 {
                    c[k] = [acc[0] / tot, acc[1] / tot, acc[2] / tot];
                }
                wt[k] = (tot / 4.0).min(1.0);
            }
        }
        lv.push(Lvl { w: nw, h: nh, c, wt });
    }
    let top = lv.len() - 1;
    let mut cur: Vec<[f32; 3]> = lv[top].c.clone();
    for l in (0..top).rev() {
        let (lw, lh) = (lv[l].w, lv[l].h);
        let (uw, uh) = (lv[l + 1].w, lv[l + 1].h);
        let mut next = vec![[0.0f32; 3]; lw * lh];
        for y in 0..lh {
            for x in 0..lw {
                // bilinear lookup in the coarser level (wrapping)
                let (fx, fy) = ((x as f32 + 0.5) * uw as f32 / lw as f32 - 0.5, (y as f32 + 0.5) * uh as f32 / lh as f32 - 0.5);
                let (x0, y0) = (fx.floor(), fy.floor());
                let (tx, ty) = (fx - x0, fy - y0);
                let at = |xx: i64, yy: i64| cur[(yy.rem_euclid(uh as i64) as usize) * uw + xx.rem_euclid(uw as i64) as usize];
                let (a, b, c, d) = (at(x0 as i64, y0 as i64), at(x0 as i64 + 1, y0 as i64), at(x0 as i64, y0 as i64 + 1), at(x0 as i64 + 1, y0 as i64 + 1));
                let k = y * lw + x;
                let wv = lv[l].wt[k];
                for i in 0..3 {
                    let up = lerp(lerp(a[i], b[i], tx), lerp(c[i], d[i], tx), ty);
                    next[k][i] = lv[l].c[k][i] * wv + up * (1.0 - wv);
                }
            }
        }
        cur = next;
    }
    cur.into_iter().map(|c| [c[0], c[1], c[2], 1.0]).collect()
}

/// Season anchors: (target hue for foliage or None, saturation x, value x, white mix, cold tint).
fn season_anchor(i: usize) -> (Option<f32>, f32, f32, f32, f32) {
    match i % 4 {
        0 => (Some(0.22), 1.0, 1.12, 0.0, 0.0),
        1 => (Some(0.30), 1.18, 0.92, 0.0, 0.0),
        2 => (Some(0.07), 1.05, 1.0, 0.0, 0.0),
        _ => (None, 0.2, 1.1, 0.35, 0.25),
    }
}

pub fn compute(kind: &str, id: u32, n: &Node, ins: &BTreeMap<String, Rc<Tex>>, s: usize, hs: usize, seed: u32) -> Result<Option<Tex>, Error> {
    let f = |k: &str, d: f32| n.nums.get(k).copied().filter(|v| v.is_finite()).unwrap_or(d);
    let need = |slot: &str| -> Result<&Tex, Error> { ins.get(slot).map(|r| r.as_ref()).ok_or_else(|| Error::MissingInput(id, slot.to_string())) };
    let opt = |slot: &str| -> Option<&Tex> { ins.get(slot).map(|r| r.as_ref()) };
    let rs = s as f32 / 512.0;
    let out_idx = f("_out", 0.0).round() as usize;
    let (con, inv) = (f("contrast", 1.0).clamp(0.0, 16.0), f("invert", 0.0) > 0.5);
    let fin = |t: Tex| -> Tex {
        if (con - 1.0).abs() > 1e-4 || inv {
            ops::contrast_invert(&t, con, inv)
        } else {
            t
        }
    };
    let out = match kind {
        "cross_section" => {
            let src = need("in")?;
            let row = ((f("position", 0.5).clamp(0.0, 1.0) * src.h as f32) as i64).min(src.h as i64 - 1);
            let gain = f("height", 1.0).clamp(0.0, 8.0);
            let thick = (f("thickness", 3.0) * rs).clamp(0.5, 64.0);
            let fill = f("fill", 1.0) > 0.5;
            let margin = hs as f32 * 0.06;
            let ys: Vec<f32> = (0..s).map(|x| {
                let u = (x as f32 + 0.5) / s as f32;
                let v = lum(src.sample(u, (row as f32 + 0.5) / src.h as f32));
                hs as f32 - margin - (v * gain).clamp(0.0, 1.0) * (hs as f32 - 2.0 * margin)
            }).collect();
            let mut out = Tex { w: s, h: hs, px: vec![[0.0, 0.0, 0.0, 1.0]; s * hs] };
            par_rows(&mut out.px, s, |start, chunk| {
                for (j, dst) in chunk.iter_mut().enumerate() {
                    let (x, y) = ((start + j) % s, ((start + j) / s) as f32 + 0.5);
                    let (a, b, c) = (ys[(x + s - 1) % s], ys[x], ys[(x + 1) % s]);
                    let (lo, hi) = (a.min(b).min(c), a.max(b).max(c));
                    let dy = if y < lo { lo - y } else if y > hi { y - hi } else { 0.0 };
                    let line = 1.0 - smooth(thick * 0.5, thick * 0.5 + 1.0, dy);
                    let area = if fill && y > b { 0.28 } else { 0.0 };
                    let q = (y - margin) / (hs as f32 - 2.0 * margin) * 4.0;
                    let grid = if (q - q.round()).abs() * (hs as f32 - 2.0 * margin) / 4.0 < 0.6 { 0.09 } else { 0.0 };
                    *dst = grey(line.max(area).max(grid));
                }
            });
            fin(out)
        }
        "uber_emboss" => {
            let hmap = need("height")?;
            let mask = opt("mask");
            let gloss_map = opt("gloss");
            let intensity = f("intensity", 3.0).clamp(0.0, 64.0);
            let (az, el) = (f("angle", 0.125) * TAU, f("elevation", 0.45).clamp(0.05, 1.0) * std::f32::consts::FRAC_PI_2);
            let l = [az.cos() * el.cos(), az.sin() * el.cos(), el.sin()];
            let hv = {
                let v = [l[0], l[1], l[2] + 1.0];
                let m = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                [v[0] / m, v[1] / m, v[2] / m]
            };
            let gloss0 = f("gloss", 0.5).clamp(0.0, 1.0);
            let spec = f("highlight", 1.0).clamp(0.0, 4.0);
            let gain = f("gain", 1.0).clamp(0.0, 8.0);
            let sin_e = el.sin();
            let mut out = Tex { w: s, h: hs, px: vec![[0.0, 0.0, 0.0, 1.0]; s * hs] };
            let (w, hh) = (hmap.w, hmap.h);
            par_rows(&mut out.px, s, |start, chunk| {
                for (j, dst) in chunk.iter_mut().enumerate() {
                    let (x, y) = ((start + j) % s, (start + j) / s);
                    let (u, v) = ((x as f32 + 0.5) / s as f32, (y as f32 + 0.5) / hs as f32);
                    let (px, py) = (((u * w as f32) as i64), ((v * hh as f32) as i64));
                    let (gx, gy) = gradient(hmap, px, py, w as f32 / 512.0);
                    let nn = [-gx * intensity * 8.0, -gy * intensity * 8.0, 1.0];
                    let m = (nn[0] * nn[0] + nn[1] * nn[1] + 1.0).sqrt();
                    let nv = [nn[0] / m, nn[1] / m, nn[2] / m];
                    let d = nv[0] * l[0] + nv[1] * l[1] + nv[2] * l[2];
                    let g = gloss_map.map(|t| lum(t.sample(u, v))).unwrap_or(gloss0);
                    let expo = lerp(4.0, 256.0, g * g);
                    let sp = (nv[0] * hv[0] + nv[1] * hv[1] + nv[2] * hv[2]).max(0.0).powf(expo) * spec * lerp(0.4, 1.0, g);
                    let mk = mask.map(|t| lum(t.sample(u, v))).unwrap_or(1.0);
                    let val = match out_idx {
                        1 => sp,
                        2 => (sin_e - d).max(0.0) * gain * 2.0,
                        _ => 0.5 + (d - sin_e) * gain * 0.5 / (1.0 - sin_e * 0.6),
                    };
                    let base = if out_idx == 0 { 0.5 } else { 0.0 };
                    *dst = grey(lerp(base, val, mk).clamp(0.0, 1.0));
                }
            });
            out
        }
        "diffusion" => {
            let src = need("in")?;
            let mask = opt("mask");
            let known: Vec<f32> = src.px.iter().enumerate().map(|(i, p)| match mask {
                Some(m) => if lum(m.px[i.min(m.px.len() - 1)]) > 0.5 { 1.0 } else { 0.0 },
                None => if p[3] > 0.5 { 1.0 } else { 0.0 },
            }).collect();
            let mut px = diffuse(&src.px, &known, src.w, src.h);
            let sm = f("smoothing", 0.0).clamp(0.0, 64.0) * rs;
            let mut t = Tex { w: src.w, h: src.h, px: std::mem::take(&mut px) };
            if sm > 0.1 {
                let b = ops::blur(&t, sm);
                for i in 0..t.px.len() {
                    let k = 1.0 - known[i];
                    for c in 0..3 {
                        t.px[i][c] = lerp(t.px[i][c], b.px[i][c], k);
                    }
                }
            }
            if f("gray", 0.0) > 0.5 {
                t = ops::to_gray(&t);
            }
            t
        }
        "summed_area_table" => {
            let src = need("in")?;
            let per = f("per_channel", 0.0) > 0.5;
            let (w, h) = (src.w, src.h);
            let mut sat = vec![[0.0f64; 3]; w * h];
            for y in 0..h {
                let mut row = [0.0f64; 3];
                for x in 0..w {
                    let p = src.px[y * w + x];
                    let v = if per { [p[0] as f64, p[1] as f64, p[2] as f64] } else { let l = lum(p) as f64; [l, l, l] };
                    for c in 0..3 {
                        row[c] += v[c];
                        sat[y * w + x][c] = row[c] + if y > 0 { sat[(y - 1) * w + x][c] } else { 0.0 };
                    }
                }
            }
            let tot = sat[w * h - 1];
            let norm = f("normalize", 1.0) > 0.5;
            Tex { w, h, px: sat.iter().map(|v| {
                let q = |c: usize| if norm { (v[c] / tot[c].max(1e-9)) as f32 } else { (v[c] / (w * h) as f64) as f32 };
                [q(0), q(1), q(2), 1.0]
            }).collect() }
        }
        "vector_morph" => {
            let src = need("in")?;
            let vecm = need("vector")?;
            let amount = f("amount", 40.0) * rs;
            let flip = f("directx", 0.0) > 0.5;
            let (w, h) = (src.w, src.h);
            let mut acc = vec![[0.0f32; 4]; w * h];
            let mut wt = vec![0.0f32; w * h];
            let dec = |u: f32, v: f32| {
                let p = vecm.sample(u, v);
                ((p[0] * 2.0 - 1.0), (p[1] * 2.0 - 1.0) * if flip { -1.0 } else { 1.0 })
            };
            for y in 0..h {
                for x in 0..w {
                    let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
                    let (vx, vy) = dec(u, v);
                    let (fx, fy) = (x as f32 + vx * amount * w as f32 / s as f32, y as f32 + vy * amount * h as f32 / hs as f32);
                    let (x0, y0) = (fx.floor(), fy.floor());
                    let (ax, ay) = (fx - x0, fy - y0);
                    let p = src.px[y * w + x];
                    for (dx, dy, wgt) in [(0i64, 0i64, (1.0 - ax) * (1.0 - ay)), (1, 0, ax * (1.0 - ay)), (0, 1, (1.0 - ax) * ay), (1, 1, ax * ay)] {
                        let k = ((y0 as i64 + dy).rem_euclid(h as i64) as usize) * w + (x0 as i64 + dx).rem_euclid(w as i64) as usize;
                        for c in 0..4 {
                            acc[k][c] += p[c] * wgt;
                        }
                        wt[k] += wgt;
                    }
                }
            }
            let px: Vec<[f32; 4]> = (0..w * h).map(|k| {
                if wt[k] > 0.05 {
                    let m = 1.0 / wt[k];
                    [acc[k][0] * m, acc[k][1] * m, acc[k][2] * m, acc[k][3] * m]
                } else {
                    // nothing moved here: look back along the vector instead of leaving a hole
                    let (u, v) = (((k % w) as f32 + 0.5) / w as f32, ((k / w) as f32 + 0.5) / h as f32);
                    let (vx, vy) = dec(u, v);
                    src.sample(u - vx * amount / s as f32, v - vy * amount / hs as f32)
                }
            }).collect();
            Tex { w, h, px }
        }
        "safe_transform" => {
            let src = need("in")?;
            let mode = f("mode", 0.0).round() as i32;
            let filt = f("filter", 1.0).round() as i32;
            let (offx, offy) = (f("offset_x", 0.0), f("offset_y", 0.0));
            let angle = f("angle", 0.0);
            let sample = |u: f32, v: f32| -> [f32; 4] {
                match filt {
                    0 => src.at((u * src.w as f32).floor() as i64, (v * src.h as f32).floor() as i64),
                    2 => src.sample_cubic(u, v),
                    _ => src.sample(u, v),
                }
            };
            if mode == 0 {
                // tile-safe: an integer matrix (rotation by an integer direction, integer repeats) maps the torus onto itself
                let (a, b) = {
                    let th = angle * TAU;
                    let mut best = (f32::MIN, 1i32, 0i32);
                    for a in -4..=4i32 {
                        for b in -4..=4i32 {
                            if a == 0 && b == 0 {
                                continue;
                            }
                            let c = (a as f32 * th.cos() + b as f32 * th.sin()) / ((a * a + b * b) as f32).sqrt();
                            if c > best.0 + 1e-6 || ((c - best.0).abs() <= 1e-6 && a * a + b * b < best.1 * best.1 + best.2 * best.2) {
                                best = (c, a, b);
                            }
                        }
                    }
                    (best.1 as f32, best.2 as f32)
                };
                let (sx, sy) = (f("scale_x", 1.0).round().clamp(1.0, 32.0), f("scale_y", 1.0).round().clamp(1.0, 32.0));
                Tex::from_fn(s, hs, |u, v| {
                    let (x, y) = (u * sx, v * sy);
                    sample(a * x - b * y + offx, b * x + a * y + offy)
                })
            } else {
                let edge = f("edge", 0.0).round() as i32;
                let (sx, sy) = (f("scale_x", 1.0).clamp(0.01, 64.0), f("scale_y", 1.0).clamp(0.01, 64.0));
                let after = f("rotate_after_scale", 0.0) > 0.5;
                let (sa, ca) = (angle * TAU).sin_cos();
                let (aw, ah) = (s as f32, hs as f32);
                Tex::from_fn(s, hs, |u, v| {
                    // work in pixel units so a rotation does not shear a non-square texture
                    let (mut x, mut y) = ((u - 0.5) * aw, (v - 0.5) * ah);
                    if after {
                        x /= sx;
                        y /= sy;
                        let (rx, ry) = (x * ca + y * sa, -x * sa + y * ca);
                        x = rx;
                        y = ry;
                    } else {
                        let (rx, ry) = (x * ca + y * sa, -x * sa + y * ca);
                        x = rx / sx;
                        y = ry / sy;
                    }
                    let (mut su, mut sv) = (x / aw + 0.5 - offx, y / ah + 0.5 - offy);
                    let mut alpha = 1.0;
                    match edge {
                        1 => {
                            let m = |t: f32| { let r = t.rem_euclid(2.0); if r > 1.0 { 2.0 - r } else { r } };
                            su = m(su);
                            sv = m(sv);
                        }
                        2 => {
                            su = su.clamp(0.0, 1.0);
                            sv = sv.clamp(0.0, 1.0);
                        }
                        3 => {
                            if !(0.0..1.0).contains(&su) || !(0.0..1.0).contains(&sv) {
                                alpha = 0.0;
                            }
                        }
                        _ => {}
                    }
                    let c = sample(su, sv);
                    [c[0] * alpha, c[1] * alpha, c[2] * alpha, c[3] * alpha]
                })
            }
        }
        "mesh_weathering" => {
            // wear on convex edges, dirt in cavities, dust on surfaces that face up: all from maps baked from a mesh
            let curv = need("curvature")?;
            let (ao, nrm, noise_in) = (opt("ao"), opt("normal"), opt("noise"));
            let (wear, dirt, dust) = (f("wear", 0.5).clamp(0.0, 1.0), f("dirt", 0.5).clamp(0.0, 1.0), f("dust", 0.5).clamp(0.0, 1.0));
            let spread = f("up_spread", 60.0).clamp(1.0, 179.0).to_radians();
            let tilt = f("up_angle", 0.0).to_radians();
            let up = [tilt.sin(), tilt.cos()];
            let breakup = f("breakup", 0.5).clamp(0.0, 1.0);
            let (px, py) = periods(f("scale", 8.0), s, hs);
            let fb = Fbm::new(px, py, 4, 0.5, seed, 0);
            let cos_lim = spread.cos();
            let mut m = Tex { w: s, h: hs, px: vec![[0.0, 0.0, 0.0, 1.0]; s * hs] };
            par_rows(&mut m.px, s, |start, chunk| {
                for (j, dst) in chunk.iter_mut().enumerate() {
                    let (x, y) = ((start + j) % s, (start + j) / s);
                    let (u, v) = ((x as f32 + 0.5) / s as f32, (y as f32 + 0.5) / hs as f32);
                    let c = lum(curv.sample(u, v)) - 0.5;
                    let nz = noise_in.map(|t| lum(t.sample(u, v))).unwrap_or_else(|| fb.sample(u, v));
                    let br = (nz - 0.5) * breakup;
                    let w = smooth(0.0, 0.25, c * 2.0 * (0.4 + wear * 1.6) + br * 0.6) * wear.min(1.0).sqrt();
                    let cav = (-c * 2.0).max(0.0);
                    let occl = ao.map(|t| 1.0 - lum(t.sample(u, v))).unwrap_or(0.0);
                    let d = smooth(0.05, 0.6, (cav + occl) * (0.5 + dirt) + br) * dirt.sqrt();
                    let du = nrm
                        .map(|t| {
                            let p = t.sample(u, v);
                            let (nx, ny, nz) = (p[0] * 2.0 - 1.0, p[1] * 2.0 - 1.0, p[2] * 2.0 - 1.0);
                            let l = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-6);
                            let facing = (nx * up[0] + ny * up[1]) / l;
                            smooth(cos_lim, 1.0, facing + br * 0.5) * dust.sqrt()
                        })
                        .unwrap_or(0.0);
                    *dst = grey(1.0 - (1.0 - w) * (1.0 - d) * (1.0 - du));
                }
            });
            fin(m)
        }
        "snow_filter" => {
            let hmap = need("height")?;
            let mask_in = opt("mask");
            let coverage = f("coverage", 0.55).clamp(0.0, 1.0);
            let slope_hold = f("slope", 0.5).clamp(0.0, 1.0);
            let soft = f("softness", 0.2).clamp(0.01, 1.0);
            let breakup = f("breakup", 0.4).clamp(0.0, 1.0);
            let height_bias = f("height_bias", 0.3).clamp(0.0, 1.0);
            let depth = f("depth", 0.15).clamp(0.0, 1.0);
            let (px, py) = periods(f("scale", 6.0), s, hs);
            let fb = Fbm::new(px, py, 4, 0.5, seed, 0);
            let steep = f("steepness", 6.0).clamp(0.1, 64.0);
            let mut m = Tex { w: s, h: hs, px: vec![[0.0, 0.0, 0.0, 1.0]; s * hs] };
            par_rows(&mut m.px, s, |start, chunk| {
                for (j, dst) in chunk.iter_mut().enumerate() {
                    let (x, y) = ((start + j) % s, (start + j) / s);
                    let (u, v) = ((x as f32 + 0.5) / s as f32, (y as f32 + 0.5) / hs as f32);
                    let (gx, gy) = gradient(hmap, (u * hmap.w as f32) as i64, (v * hmap.h as f32) as i64, hmap.w as f32 / 512.0);
                    let slope = (gx * gx + gy * gy).sqrt() * steep;
                    let up = 1.0 / (1.0 + slope * slope).sqrt();
                    let hv = lum(hmap.sample(u, v));
                    // surfaces tilted past the hold limit stay clean; coverage, height and noise move the limit around
                    let up_min = 1.0 - slope_hold * 0.8;
                    let k = up - up_min + (coverage - 0.5) * 0.6 + (hv - 0.5) * height_bias * 0.6 + (fb.sample(u, v) - 0.5) * breakup * 0.8;
                    let mk = mask_in.map(|t| lum(t.sample(u, v))).unwrap_or(1.0);
                    *dst = grey(smooth(0.0, soft, k) * mk);
                }
            });
            let blurred = ops::blur(&m, (2.0 * rs).max(0.5));
            if out_idx == 1 {
                blurred
            } else {
                let sm2 = ops::blur(&blurred, (6.0 * rs).max(1.0));
                Tex::gray_fn(s, hs, |u, v| {
                    let c = lum(blurred.sample(u, v));
                    (lum(hmap.sample(u, v)) + depth * (c * 0.7 + lum(sm2.sample(u, v)) * 0.3)).clamp(0.0, 1.0)
                })
            }
        }
        "season_filter" => {
            let src = need("in")?;
            let mask = opt("mask");
            let season = f("season", 2.0).rem_euclid(4.0);
            let strength = f("strength", 1.0).clamp(0.0, 1.0);
            let var = f("variation", 0.4).clamp(0.0, 1.0);
            let (fh, fr) = (f("foliage_hue", 0.28), f("foliage_range", 0.14).clamp(0.02, 0.5));
            let (px, py) = periods(f("scale", 8.0), s, hs);
            let fb = Fbm::new(px, py, 3, 0.5, seed, 0);
            let (i0, t) = (season.floor() as usize, season - season.floor());
            let (a, b) = (season_anchor(i0), season_anchor(i0 + 1));
            Tex::from_fn(s, hs, |u, v| {
                let p = src.sample(u, v);
                let (h0, sat, val) = ops::rgb_to_hsv(p[0], p[1], p[2]);
                let dh = { let d = (h0 - fh).abs(); d.min(1.0 - d) };
                let foliage = smooth(fr, fr * 0.4, dh) * smooth(0.12, 0.3, sat);
                let mk = mask.map(|m| lum(m.sample(u, v))).unwrap_or(1.0);
                let wgt = strength * mk;
                let nz = fb.sample(u, v) - 0.5;
                // hue target of the two nearest seasons, with a leaf-by-leaf shift in autumn
                let tgt = |an: (Option<f32>, f32, f32, f32, f32), idx: usize| -> (f32, f32, f32, f32, f32) {
                    let th = an.0.map(|t| if idx % 4 == 2 { (t + nz * var * 0.2).rem_euclid(1.0) } else { t }).unwrap_or(h0);
                    (th, an.1, an.2, an.3, an.4)
                };
                let (ta, tb) = (tgt(a, i0), tgt(b, i0 + 1));
                let hue_shift = |th: f32| { let d = th - h0; let d = d - d.round(); h0 + d * foliage };
                let hue = lerp(hue_shift(ta.0), hue_shift(tb.0), t);
                let sat_m = lerp(lerp(1.0, ta.1, foliage.max(if ta.3 > 0.0 { 1.0 } else { 0.0 })), lerp(1.0, tb.1, foliage.max(if tb.3 > 0.0 { 1.0 } else { 0.0 })), t);
                let val_m = lerp(ta.2, tb.2, t);
                let (r, g, bl) = ops::hsv_to_rgb(hue.rem_euclid(1.0), (sat * sat_m).clamp(0.0, 1.0), (val * val_m).clamp(0.0, 1.0));
                let white = lerp(ta.3, tb.3, t);
                let cold = lerp(ta.4, tb.4, t);
                let (r, g, bl) = (lerp(r, 0.95, white), lerp(g, 0.97, white), lerp(bl, 1.0, white));
                let (r, g, bl) = (lerp(r, r * 0.85, cold), lerp(g, g * 0.95, cold), lerp(bl, (bl * 1.1).min(1.0), cold));
                [lerp(p[0], r, wgt), lerp(p[1], g, wgt), lerp(p[2], bl, wgt), p[3]]
            })
        }
        "water_filter" => {
            let hmap = need("height")?;
            let base = opt("base");
            let level = f("level", 0.35).clamp(0.0, 1.0);
            let soft = f("softness", 0.03).clamp(0.001, 0.5);
            let tint = [f("tint_r", 0.1), f("tint_g", 0.25), f("tint_b", 0.3)];
            let murk = f("murkiness", 0.5).clamp(0.0, 1.0);
            let wet_edge = f("wet_edge", 0.08).clamp(0.0, 0.5);
            let ripples = f("ripples", 0.3).clamp(0.0, 1.0);
            let (px, py) = periods(f("ripple_scale", 24.0), s, hs);
            let fb = Fbm::new(px, py, 3, 0.5, seed, 0);
            let dry_rough = f("dry_roughness", 0.7).clamp(0.0, 1.0);
            let wet_rough = f("wet_roughness", 0.05).clamp(0.0, 1.0);
            let depth_view = Tex::gray_fn(s, hs, |u, v| lum(hmap.sample(u, v)));
            Tex::from_fn(s, hs, |u, v| {
                let hv = depth_view.sample(u, v)[0];
                let water = smooth(level + soft, level - soft, hv);
                let wet = smooth(level + soft + wet_edge, level - soft, hv);
                let deep = ((level - hv) / level.max(0.05)).clamp(0.0, 1.0);
                match out_idx {
                    1 => grey(water),
                    2 => {
                        let rip = (fb.sample(u, v) - 0.5) * ripples * 0.04;
                        grey(lerp(hv, level + rip, water))
                    }
                    3 => grey(lerp(dry_rough, wet_rough, water.max(wet * 0.5))),
                    _ => {
                        let bc = base.map(|b| b.sample(u, v)).unwrap_or([0.5, 0.5, 0.5, 1.0]);
                        // wet ground is darker; under water the colour fades into the tint with depth
                        let darkened = [bc[0] * (1.0 - 0.45 * wet), bc[1] * (1.0 - 0.45 * wet), bc[2] * (1.0 - 0.45 * wet)];
                        let k = water * (0.4 + 0.6 * murk * (0.4 + 0.6 * deep));
                        [lerp(darkened[0], tint[0], k), lerp(darkened[1], tint[1], k), lerp(darkened[2], tint[2], k), bc[3]]
                    }
                }
            })
        }
        _ => return crate::nodes9::compute(kind, id, n, ins, s, hs, seed),
    };
    // quiet an unused import when a build has no nodes using it
    let _ = noise::rnd;
    Ok(Some(out))
}
