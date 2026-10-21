// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fifth set of nodes: more noises (blue, caustics, liquid, 3D), strand patterns (fibres, scratches), paving and wire
//! mesh patterns, halftone, and image effects (reaction diffusion, anti-aliasing, bilateral blur). `compute` returns
//! Ok(None) for kinds it does not know. Everything tiles and works on non-square graphs; pixel sizes are given for a
//! 512 px texture and scaled by `size / 512`.
use crate::graph::{Error, Node};
use crate::noise::{self, Fbm};
use crate::ops;
use crate::tex::{lum, par_rows, Tex};
use std::collections::BTreeMap;
use std::f32::consts::TAU;
use std::sync::Arc as Rc;

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let d = b - a;
    let t = if d.abs() < 1e-9 { if x >= b { 1.0 } else { 0.0 } } else { ((x - a) / d).clamp(0.0, 1.0) };
    t * t * (3.0 - 2.0 * t)
}

fn grey(v: f32) -> [f32; 4] {
    [v, v, v, 1.0]
}

fn fract(x: f32) -> f32 {
    x - x.floor()
}

/// Lattice periods for a noise of `scale` cells across the width, so cells stay square on a non-square graph.
fn periods(scale: f32, w: usize, h: usize) -> (i32, i32) {
    let px = scale.round().clamp(1.0, 256.0) as i32;
    (px, ((px as f32 * h as f32 / w as f32).round() as i32).max(1))
}

// ---------------------------------------------------------------- 3D gradient noise
fn grad3(ix: i32, iy: i32, iz: i32, seed: u32) -> [f32; 3] {
    const G: [[f32; 3]; 12] = [
        [1.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0],
        [1.0, -1.0, 0.0],
        [-1.0, -1.0, 0.0],
        [1.0, 0.0, 1.0],
        [-1.0, 0.0, 1.0],
        [1.0, 0.0, -1.0],
        [-1.0, 0.0, -1.0],
        [0.0, 1.0, 1.0],
        [0.0, -1.0, 1.0],
        [0.0, 1.0, -1.0],
        [0.0, -1.0, -1.0],
    ];
    let h = noise::hash2(ix.wrapping_add(iz.wrapping_mul(7919)), iy.wrapping_add(iz.wrapping_mul(104_729)), seed.wrapping_add((iz as u32).wrapping_mul(0x9e37_79b1)));
    G[((h >> 8) % 12) as usize]
}

fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Tileable 3D gradient noise in about -1..1; `x`,`y`,`z` are lattice coordinates with periods `p`.
fn perlin3(x: f32, y: f32, z: f32, p: [i32; 3], seed: u32) -> f32 {
    let (fx, fy, fz) = (x.floor(), y.floor(), z.floor());
    let (ix, iy, iz) = (fx as i32, fy as i32, fz as i32);
    let (dx, dy, dz) = (x - fx, y - fy, z - fz);
    let (u, v, w) = (fade(dx), fade(dy), fade(dz));
    let corner = |ox: i32, oy: i32, oz: i32| -> f32 {
        let g = grad3((ix + ox).rem_euclid(p[0]), (iy + oy).rem_euclid(p[1]), (iz + oz).rem_euclid(p[2]), seed);
        g[0] * (dx - ox as f32) + g[1] * (dy - oy as f32) + g[2] * (dz - oz as f32)
    };
    let x00 = lerp(corner(0, 0, 0), corner(1, 0, 0), u);
    let x10 = lerp(corner(0, 1, 0), corner(1, 1, 0), u);
    let x01 = lerp(corner(0, 0, 1), corner(1, 0, 1), u);
    let x11 = lerp(corner(0, 1, 1), corner(1, 1, 1), u);
    lerp(lerp(x00, x10, v), lerp(x01, x11, v), w)
}

// ---------------------------------------------------------------- strands (fibres and scratches)
struct Strands {
    count: usize,
    length: f32,
    thick: f32,
    angle: f32,
    angle_var: f32,
    curl: f32,
    taper: f32,
    vary: f32,
    seed: u32,
    /// overlapping strands add up (scaled by `gain`) instead of keeping the brighter one
    add: bool,
    gain: f32,
    /// soft strands: a bell-shaped profile across instead of a hard edge
    soft: bool,
}

/// Draw `count` thin curved strands with wrap-around; lengths and thickness are in pixels. Overlaps keep the brighter
/// one, or add up (each strand counted once per pixel).
fn strands(w: usize, h: usize, p: &Strands) -> Tex {
    let mut buf = vec![0.0f32; w * h];
    // for adding: which strand last wrote a pixel and how much it put there
    let mut owner = if p.add { vec![u32::MAX; w * h] } else { Vec::new() };
    let mut own = if p.add { vec![0.0f32; w * h] } else { Vec::new() };
    let r = |i: usize, k: u32| noise::rnd(i as i32, k as i32, p.seed.wrapping_add(0x51ed));
    for i in 0..p.count {
        let (mut x, mut y) = (r(i, 1) * w as f32, r(i, 2) * h as f32);
        let mut a = (p.angle + (r(i, 3) - 0.5) * 2.0 * p.angle_var) * TAU;
        let len = p.length * (0.5 + r(i, 4));
        let bright = 1.0 - p.vary * r(i, 5);
        let bend = if r(i, 6) < 0.5 { -p.curl } else { p.curl } * (0.5 + r(i, 7));
        let step = 0.7f32;
        let n = ((len / step).ceil() as usize).clamp(1, 4096);
        for k in 0..n {
            let t = (k as f32 + 0.5) / n as f32;
            // thickness profile: 1 in the middle, thinning to the tips as `taper` grows
            let prof = (1.0 - (2.0 * t - 1.0).abs().powf(1.0 + 2.0 * (1.0 - p.taper.clamp(0.0, 1.0)) * 4.0)).max(0.0);
            let rad = (p.thick * 0.5 * lerp(1.0, prof, p.taper.clamp(0.0, 1.0))).max(0.0);
            let reach = if p.soft { (rad * 2.0).ceil() as i32 + 1 } else { rad.ceil() as i32 + 1 };
            let (cx, cy) = (x.floor() as i32, y.floor() as i32);
            for oy in -reach..=reach {
                for ox in -reach..=reach {
                    let (px, py) = (cx + ox, cy + oy);
                    let d = ((px as f32 + 0.5 - x).powi(2) + (py as f32 + 0.5 - y).powi(2)).sqrt();
                    let cover = if p.soft { (-2.0 * (d / rad.max(0.5)).powi(2)).exp() } else { (rad + 0.5 - d).clamp(0.0, 1.0) } * bright;
                    if cover > 0.0 {
                        let idx = (py.rem_euclid(h as i32) as usize) * w + px.rem_euclid(w as i32) as usize;
                        if p.add {
                            let c = cover * p.gain;
                            if owner[idx] != i as u32 {
                                owner[idx] = i as u32;
                                own[idx] = c;
                                buf[idx] += c;
                            } else if c > own[idx] {
                                buf[idx] += c - own[idx];
                                own[idx] = c;
                            }
                        } else if cover > buf[idx] {
                            buf[idx] = cover;
                        }
                    }
                }
            }
            x += a.cos() * step;
            y += a.sin() * step;
            a += bend * step / len.max(1.0);
            x = x.rem_euclid(w as f32);
            y = y.rem_euclid(h as f32);
        }
    }
    Tex { w, h, px: buf.into_iter().map(|v| grey(v.min(1.0))).collect() }
}

// ---------------------------------------------------------------- reaction diffusion
fn reaction_diffusion(seed_map: &Tex, gw: usize, gh: usize, iters: usize, feed: f32, kill: f32, seed: u32, spots: f32) -> Vec<f32> {
    let n = gw * gh;
    let mut u = vec![1.0f32; n];
    let mut v = vec![0.0f32; n];
    for y in 0..gh {
        for x in 0..gw {
            let s = lum(seed_map.sample((x as f32 + 0.5) / gw as f32, (y as f32 + 0.5) / gh as f32));
            if s > 0.5 || noise::rnd(x as i32, y as i32, seed) < spots {
                v[y * gw + x] = 1.0;
                u[y * gw + x] = 0.5;
            }
        }
    }
    let (mut nu, mut nv) = (u.clone(), v.clone());
    let (du, dv) = (0.2097f32, 0.105f32);
    for _ in 0..iters {
        if crate::tex::job_stale() {
            break;
        }
        for y in 0..gh {
            let (ym, yp) = ((y + gh - 1) % gh, (y + 1) % gh);
            for x in 0..gw {
                let (xm, xp) = ((x + gw - 1) % gw, (x + 1) % gw);
                let i = y * gw + x;
                let lap = |a: &[f32]| 0.2 * (a[y * gw + xm] + a[y * gw + xp] + a[ym * gw + x] + a[yp * gw + x]) + 0.05 * (a[ym * gw + xm] + a[ym * gw + xp] + a[yp * gw + xm] + a[yp * gw + xp]) - a[i];
                let uvv = u[i] * v[i] * v[i];
                nu[i] = (u[i] + du * lap(&u) - uvv + feed * (1.0 - u[i])).clamp(0.0, 1.0);
                nv[i] = (v[i] + dv * lap(&v) + uvv - (kill + feed) * v[i]).clamp(0.0, 1.0);
            }
        }
        std::mem::swap(&mut u, &mut nu);
        std::mem::swap(&mut v, &mut nv);
    }
    v
}

/// Run a node of this module. Ok(None) = not one of ours.
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
        // ------------------------------------------------ noises
        "blue_noise" => {
            let cells = f("scale", 256.0).round().clamp(8.0, 2048.0) as usize;
            let cx = cells.min(s).max(8);
            let cy = ((cx as f32 * hs as f32 / s as f32).round() as usize).max(8);
            let white = Tex::gray_fn(cx, cy, |u, v| noise::rnd((u * cx as f32) as i32, (v * cy as f32) as i32, seed));
            let low = ops::blur(&white, f("tightness", 1.5).clamp(0.3, 6.0));
            let hp: Vec<f32> = white.px.iter().zip(&low.px).map(|(a, b)| a[0] - b[0]).collect();
            // rank transform: a flat histogram with the spectrum of the high-passed noise
            let mut order: Vec<usize> = (0..hp.len()).collect();
            order.sort_by(|a, b| hp[*a].partial_cmp(&hp[*b]).unwrap_or(std::cmp::Ordering::Equal));
            let mut rank = vec![0.0f32; hp.len()];
            for (r, i) in order.iter().enumerate() {
                rank[*i] = r as f32 / (hp.len().max(2) - 1) as f32;
            }
            let small = Tex { w: cx, h: cy, px: rank.into_iter().map(grey).collect() };
            fin(Tex::from_fn(s, hs, |u, v| small.at((u * cx as f32) as i64, (v * cy as f32) as i64)))
        }
        "fibers" | "scratches" => {
            let fib = kind == "fibers";
            let p = Strands {
                count: f("count", if fib { 600.0 } else { 80.0 }).round().clamp(0.0, 20000.0) as usize,
                length: f("length", if fib { 90.0 } else { 220.0 }).clamp(1.0, 2048.0) * rs,
                thick: f("thickness", if fib { 1.4 } else { 1.0 }).clamp(0.1, 64.0) * rs.max(0.5),
                angle: f("angle", 0.0),
                angle_var: f("angle_variation", if fib { 0.5 } else { 0.2 }).clamp(0.0, 1.0),
                curl: f("curl", if fib { 0.6 } else { 0.0 }).clamp(-8.0, 8.0),
                taper: f("taper", if fib { 0.4 } else { 1.0 }),
                vary: f("brightness_variation", 0.5).clamp(0.0, 1.0),
                seed,
                add: f("blend", 0.0) > 0.5,
                gain: f("gain", 1.0).clamp(0.0, 4.0),
                soft: f("soft", 0.0) > 0.5,
            };
            fin(strands(s, hs, &p))
        }
        "caustics" => {
            let sc = f("scale", 4.0);
            let (px, py) = periods(sc, s, hs);
            let fb = Fbm::new(px, py, f("octaves", 3.0).round().clamp(1.0, 6.0) as u32, 0.5, seed, 0);
            let h: Vec<f32> = Tex::gray_fn(s, hs, |u, v| fb.sample(u, v)).px.iter().map(|p| p[0]).collect();
            // light refracted by the surface: bright where the mapping p -> p + k grad(h) folds (determinant near zero)
            let k = f("strength", 1.0).clamp(0.0, 16.0) / ((TAU * px as f32) * (TAU * px as f32)) * 2.0;
            let soft = 0.15 / f("sharpness", 1.0).clamp(0.05, 16.0);
            let (w, hh) = (s as i64, hs as i64);
            let at = |x: i64, y: i64| h[(y.rem_euclid(hh) as usize) * s + x.rem_euclid(w) as usize];
            let fw = s as f32;
            let mut out = Tex { w: s, h: hs, px: vec![[0.0, 0.0, 0.0, 1.0]; s * hs] };
            par_rows(&mut out.px, s, |start, chunk| {
                for (j, dst) in chunk.iter_mut().enumerate() {
                    let (x, y) = ((start + j) as i64 % w, (start + j) as i64 / w);
                    let c = at(x, y);
                    // second derivatives per unit of uv (pixels are 1/w apart on both axes)
                    let hxx = (at(x + 1, y) - 2.0 * c + at(x - 1, y)) * fw * fw;
                    let hyy = (at(x, y + 1) - 2.0 * c + at(x, y - 1)) * fw * fw;
                    let hxy = (at(x + 1, y + 1) - at(x + 1, y - 1) - at(x - 1, y + 1) + at(x - 1, y - 1)) * 0.25 * fw * fw;
                    let det = (1.0 + k * hxx) * (1.0 + k * hyy) - k * k * hxy * hxy;
                    *dst = grey(soft / (det.abs() + soft));
                }
            });
            fin(out)
        }
        "liquid_noise" => {
            let sc = f("scale", 3.0);
            let (px, py) = periods(sc, s, hs);
            let oct = f("octaves", 4.0).round().clamp(1.0, 8.0) as u32;
            let sd = seed;
            let (a, b, c) = (Fbm::new(px, py, oct, 0.5, sd, 0), Fbm::new(px, py, oct, 0.5, sd.wrapping_add(17), 0), Fbm::new(px, py, oct, 0.5, sd.wrapping_add(31), 0));
            let warp = f("warp", 0.35).clamp(0.0, 2.0);
            let bands = f("bands", 6.0).clamp(1.0, 32.0);
            let marble = f("mode", 0.0) > 0.5;
            fin(Tex::gray_fn(s, hs, |u, v| {
                let (qx, qy) = (a.sample(u, v) - 0.5, b.sample(u, v) - 0.5);
                let (rx, ry) = (a.sample(u + warp * qx, v + warp * qy) - 0.5, b.sample(u + warp * qx + 0.17, v + warp * qy + 0.31) - 0.5);
                let val = c.sample(u + warp * rx * 2.0, v + warp * ry * 2.0);
                if marble {
                    0.5 + 0.5 * (TAU * (val * bands)).sin()
                } else {
                    val
                }
            }))
        }
        "perlin_3d" => {
            let sc = f("scale", 4.0);
            let (px, py) = periods(sc, s, hs);
            let pz = px.max(1);
            let z = f("depth", 0.0) * pz as f32;
            let oct = f("octaves", 4.0).round().clamp(1.0, 8.0) as u32;
            let gain = f("gain", 0.5).clamp(0.0, 1.0);
            let sd = seed;
            fin(Tex::gray_fn(s, hs, |u, v| {
                let (mut amp, mut sum, mut norm) = (1.0f32, 0.0f32, 0.0f32);
                for o in 0..oct {
                    let m = 1i32 << o;
                    let p = [px * m, py * m, pz * m];
                    sum += amp * perlin3(u * p[0] as f32, v * p[1] as f32, z * m as f32, p, sd.wrapping_add(o * 131));
                    norm += amp;
                    amp *= gain;
                }
                (0.5 + 0.5 * sum / norm * 1.2).clamp(0.0, 1.0)
            }))
        }

        // ------------------------------------------------ patterns
        "paving" => {
            let sc = f("scale", 6.0);
            let (px, py) = periods(sc, s, hs);
            let sd = seed;
            let (jitter, gap, bevel) = (f("jitter", 0.5).clamp(0.0, 1.0), f("gap", 0.04).clamp(0.0, 0.4), f("bevel", 0.08).clamp(0.001, 0.5));
            let (vary, dome) = (f("height_variation", 0.3).clamp(0.0, 1.0), f("dome", 0.3).clamp(0.0, 1.0));
            fin(Tex::gray_fn(s, hs, |u, v| {
                let r = noise::voronoi_ext(u, v, px, py, sd, jitter);
                let edge = (r.f2 - r.f1) * 0.5;
                match out_idx {
                    1 => r.id1,
                    2 => 1.0 - smooth(gap, gap + bevel * 0.25, edge),
                    _ => {
                        let top = smooth(gap, gap + bevel, edge);
                        let h = 1.0 - vary * r.id1;
                        // a gentle dome: stones are higher in the middle
                        top * h * (1.0 - dome * (1.0 - smooth(gap, gap + 0.3, edge)))
                    }
                }
            }))
        }
        "wire_mesh" => {
            let nx = f("count_x", 8.0).round().clamp(1.0, 128.0);
            let ny = f("count_y", 8.0).round().clamp(1.0, 128.0);
            let (width, depth) = (f("wire_width", 0.3).clamp(0.02, 0.95), f("weave_depth", 0.6).clamp(0.0, 1.0));
            let diamond = f("mode", 0.0) > 0.5;
            let round = |d: f32| -> f32 {
                // cross-section of a round wire: d is the distance from the wire centre as a fraction of its half width
                (1.0 - d * d).max(0.0).sqrt()
            };
            fin(Tex::gray_fn(s, hs, |u, v| {
                let (a, b, wa, wb) = if diamond {
                    (u * nx + v * ny, u * nx - v * ny, u * nx, v * ny)
                } else {
                    (u * nx, v * ny, v * ny, u * nx)
                };
                let (fa, fb) = (fract(a), fract(b));
                let (da, db) = ((fa - 0.5).abs() * 2.0 / width, (fb - 0.5).abs() * 2.0 / width);
                // over / under: each wire rises and dips as it passes the crossings of the other direction
                let wave_a = 0.5 + 0.5 * (TAU * (wa + 0.5 * (b.floor().rem_euclid(2.0)))).cos();
                let wave_b = 0.5 + 0.5 * (TAU * (wb + 0.5 * (a.floor().rem_euclid(2.0)) + 0.5)).cos();
                let ha = if da < 1.0 { round(da) * (1.0 - depth + depth * wave_a) } else { 0.0 };
                let hb = if db < 1.0 { round(db) * (1.0 - depth + depth * wave_b) } else { 0.0 };
                ha.max(hb)
            }))
        }
        "halftone" => {
            let t = need("in")?;
            let nn = f("cells", 16.0).round().clamp(2.0, 256.0);
            let nyc = (nn * t.h as f32 / t.w as f32).round().max(1.0);
            let diag = f("mode", 0.0) > 0.5;
            let (soft, gain) = (f("softness", 0.1).clamp(0.0, 1.0), f("gain", 1.0).clamp(0.0, 4.0));
            Tex::from_fn(t.w, t.h, |u, v| {
                let (x, y) = (u * nn, v * nyc);
                let (centre_uv, d) = if diag {
                    let (a, b) = (x + y, x - y);
                    let (ac, bc) = (a.floor() + 0.5, b.floor() + 0.5);
                    let (cx, cy) = ((ac + bc) * 0.5, (ac - bc) * 0.5);
                    ((cx / nn, cy / nyc), ((a - ac).powi(2) + (b - bc).powi(2)).sqrt())
                } else {
                    let (cx, cy) = (x.floor() + 0.5, y.floor() + 0.5);
                    ((cx / nn, cy / nyc), ((x - cx).powi(2) + (y - cy).powi(2)).sqrt())
                };
                let l = (lum(t.sample(centre_uv.0, centre_uv.1)) * gain).clamp(0.0, 1.0);
                let r = l.sqrt() * 0.56;
                grey(smooth(r + soft * 0.3 + 1e-3, (r - soft * 0.3).max(0.0), d) * if l <= 0.0 { 0.0 } else { 1.0 })
            })
        }

        // ------------------------------------------------ effects
        "reaction_diffusion" => {
            let t = need("in")?;
            let g = f("resolution", 160.0).round().clamp(32.0, 512.0) as usize;
            let gh = ((g as f32 * t.h as f32 / t.w as f32).round() as usize).max(8);
            let iters = (f("iterations", 600.0).round().clamp(1.0, 6000.0) as usize).min((150_000_000 / (g * gh)).max(1));
            let v = reaction_diffusion(t, g, gh, iters, f("feed", 0.037).clamp(0.0, 0.1), f("kill", 0.06).clamp(0.0, 0.1), seed, f("spots", 0.01).clamp(0.0, 1.0));
            let small = Tex { w: g, h: gh, px: v.into_iter().map(|x| grey((x * 2.5).clamp(0.0, 1.0))).collect() };
            Tex::from_fn(s, hs, |u, w| small.sample_cubic(u, w))
        }
        "fxaa" => {
            let t = need("in")?;
            let (strength, thr) = (f("strength", 1.0).clamp(0.0, 1.0), f("threshold", 0.05).clamp(0.0, 1.0));
            let l: Vec<f32> = t.px.iter().map(|p| lum(*p)).collect();
            let (w, h) = (t.w as i64, t.h as i64);
            let li = |x: i64, y: i64| l[(y.rem_euclid(h) as usize) * t.w + x.rem_euclid(w) as usize];
            let mut out = t.clone();
            par_rows(&mut out.px, t.w, |start, chunk| {
                for (j, dst) in chunk.iter_mut().enumerate() {
                    let (x, y) = ((start + j) as i64 % w, (start + j) as i64 / w);
                    let m = li(x, y);
                    let (n, so, e, we) = (li(x, y - 1), li(x, y + 1), li(x + 1, y), li(x - 1, y));
                    let hi = m.max(n).max(so).max(e).max(we);
                    let lo = m.min(n).min(so).min(e).min(we);
                    let range = hi - lo;
                    if range < thr.max(hi * 0.125) {
                        continue;
                    }
                    let avg = (n + so + e + we) * 0.25;
                    let blend = smooth(0.0, 1.0, ((avg - m).abs() / range).clamp(0.0, 1.0)).powi(2) * 0.75 * strength + smooth(0.0, 0.5, range) * 0.25 * strength;
                    let mut acc = [0.0f32; 4];
                    for dy in -1..=1i64 {
                        for dx in -1..=1i64 {
                            let p = t.at(x + dx, y + dy);
                            for c in 0..4 {
                                acc[c] += p[c] / 9.0;
                            }
                        }
                    }
                    *dst = [lerp(dst[0], acc[0], blend), lerp(dst[1], acc[1], blend), lerp(dst[2], acc[2], blend), lerp(dst[3], acc[3], blend)];
                }
            });
            out
        }
        "bilateral_blur" => {
            let t = need("in")?;
            let guide = opt("guide").unwrap_or(t);
            let rad = (f("radius", 4.0).clamp(0.0, 32.0) * rs).round().clamp(0.0, 12.0) as i64;
            let sr = f("range", 0.1).clamp(0.005, 2.0);
            if rad == 0 {
                t.clone()
            } else if let Some(g) = crate::gpu::bilateral_blur(t, guide, rad as i32, sr) {
                g
            } else {
                let ss = (rad as f32 * 0.5).max(0.5);
                Tex::from_fn(t.w, t.h, |u, v| {
                    let (x, y) = ((u * t.w as f32) as i64, (v * t.h as f32) as i64);
                    let c = guide.sample(u, v);
                    let (mut acc, mut wsum) = ([0.0f32; 4], 0.0f32);
                    for dy in -rad..=rad {
                        for dx in -rad..=rad {
                            let gp = guide.at(x + dx, y + dy);
                            let dc = (0..3).map(|k| (gp[k] - c[k]) * (gp[k] - c[k])).sum::<f32>();
                            let wgt = (-((dx * dx + dy * dy) as f32) / (2.0 * ss * ss) - dc / (2.0 * sr * sr)).exp();
                            let p = t.at(x + dx, y + dy);
                            for k in 0..4 {
                                acc[k] += p[k] * wgt;
                            }
                            wsum += wgt;
                        }
                    }
                    [acc[0] / wsum, acc[1] / wsum, acc[2] / wsum, acc[3] / wsum]
                })
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(out))
}
