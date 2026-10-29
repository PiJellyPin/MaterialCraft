// SPDX-License-Identifier: MIT OR Apache-2.0
//! Ninth set of nodes: generators whose looks were matched to the noises and patterns `.sbs` files use, so imported
//! graphs keep their appearance (Preset Noise and the nodes that follow). Every result tiles and works on
//! non-square graphs. `compute` returns Ok(None) for kinds it does not know.
use crate::graph::{Error, Node};
use crate::noise;
use crate::noise_styles;
use crate::ops;
use crate::tex::Tex;
use std::collections::BTreeMap;
use std::sync::Arc as Rc;

pub fn compute(kind: &str, _id: u32, n: &Node, ins: &BTreeMap<String, Rc<Tex>>, s: usize, hs: usize, seed: u32) -> Result<Option<Tex>, Error> {
    let _ins = ins;
    let f = |k: &str, d: f32| n.nums.get(k).copied().filter(|v| v.is_finite()).unwrap_or(d);
    let (con, inv) = (f("contrast", 1.0).clamp(0.0, 16.0), f("invert", 0.0) > 0.5);
    let fin = |t: Tex| -> Tex {
        if (con - 1.0).abs() > 1e-4 || inv {
            ops::contrast_invert(&t, con, inv)
        } else {
            t
        }
    };
    let dz = noise::Disorder::new(seed, f("disorder", 0.0), f("disorder_speed", 1.0), f("anisotropy", 0.0), f("anisotropy_angle", 0.0));
    let warp = move |u: f32, v: f32| dz.warp(u, v);
    let out = match kind {
        "preset_noise" => {
            let turns = (f("angle", 0.0) * 4.0).round() as i32;
            let st = noise_styles::preset_style(f("preset", 0.0).max(0.0) as usize, f("roughness", 1.0).clamp(0.0, 4.0), turns);
            let scale = f("scale", 1.0).round().clamp(1.0, 256.0);
            // extra warps: the domain is displaced by low-frequency noise (Warp intensity) and, for streaky looks,
            // turned locally (Angle random)
            let (wp, ar) = (f("warp", 0.0).clamp(0.0, 4.0), f("angle_random", 0.0).clamp(0.0, 1.0));
            let warp2 = move |u: f32, v: f32| {
                let (mut a, mut b) = warp(u, v);
                if wp > 0.0 {
                    a += wp * 0.04 * noise_styles::gradient(u, v, 6, 6, seed.wrapping_add(301));
                    b += wp * 0.04 * noise_styles::gradient(u, v, 6, 6, seed.wrapping_add(302));
                }
                if ar > 0.0 {
                    a += ar * 0.10 * noise_styles::gradient(u, v, 3, 3, seed.wrapping_add(311));
                    b += ar * 0.10 * noise_styles::gradient(u, v, 3, 3, seed.wrapping_add(312));
                }
                (a, b)
            };
            let creased = matches!(f("preset", 0.0) as i32, 80 | 81);
            let mut t = if creased { noise_styles::creased(s, hs, scale, seed, wp, &warp2) } else { noise_styles::render(&st, s, hs, scale, seed, &warp2) };
            // brush pattern: a second look with another seed is mixed in
            let brush = f("brush", 0.0).clamp(0.0, 1.0);
            if brush > 1e-3 {
                let t2 = noise_styles::render(&st, s, hs, scale, seed.wrapping_add(401), &warp2);
                for (p, q) in t.px.iter_mut().zip(&t2.px) {
                    let g = p[0] + (q[0] - p[0]) * brush * 0.5;
                    *p = [g, g, g, 1.0];
                }
            }
            // overlays of other looks, each blended in with its own amount (these stand in for the extra layers the
            // library grunges mix into their base)
            let overlay = |t: &mut Tex, preset: usize, amount: f32, scale: f32, off: u32, mode: u8| {
                if amount.abs() <= 1e-3 {
                    return;
                }
                // a negative amount takes the layer away (darkens where it is bright)
                let (amount, mode) = if amount < 0.0 { (-amount, 1u8) } else { (amount, mode) };
                let o = noise_styles::render(&noise_styles::preset_style(preset, 1.0, 0), s, hs, scale, seed.wrapping_add(off), &|u, v| (u, v));
                let a = amount.clamp(0.0, 1.0);
                for (p, q) in t.px.iter_mut().zip(&o.px) {
                    let g = match mode {
                        // brighten where the overlay is bright
                        0 => p[0].max(q[0] * a + p[0] * (1.0 - a)),
                        // darken where the overlay is dark
                        1 => p[0].min(q[0] * a + p[0] * (1.0 - a)),
                        // multiply
                        _ => p[0] * (1.0 - a + a * q[0]),
                    };
                    *p = [g, g, g, 1.0];
                }
            };
            // scratches: thin straight lines at random angles, wrapped so the result still tiles (like the library
            // grunges' scratch layer), brightening for a positive amount and darkening for a negative one
            let sc = f("scratches", 0.0).clamp(-1.0, 1.0);
            if sc.abs() > 1e-3 {
                let n = (150.0 * sc.abs()) as usize + 1;
                let sf = s as f32;
                for i in 0..n {
                    let r = |k: i32| noise_styles::unit(i as i32, k, seed.wrapping_add(501));
                    let (x0, y0) = (r(1) * sf, r(2) * sf);
                    let ang = r(3) * std::f32::consts::TAU;
                    let len = (0.04 + 0.22 * r(4) * r(4)) * sf;
                    let strength = 0.25 + 0.5 * r(5);
                    // most scratches are darker than the surface, some lighter (negative amounts invert this)
                    let dark = (r(6) < 0.7) == (sc > 0.0);
                    let steps = (len * 2.0).ceil() as usize;
                    for k in 0..=steps {
                        let d = len * k as f32 / steps.max(1) as f32;
                        let fade = 1.0 - (2.0 * k as f32 / steps.max(1) as f32 - 1.0).abs().powi(2);
                        let (x, y) = (x0 + ang.cos() * d, y0 + ang.sin() * d);
                        // the line is about s/600 pixels wide so it looks the same at every resolution
                        let half = (sf / 1200.0).max(0.5);
                        let rr = half.ceil() as i32 + 1;
                        let (cx, cy) = (x.floor() as i32, y.floor() as i32);
                        for dy in -rr..=rr {
                            for dx in -rr..=rr {
                                let (qx, qy) = ((cx + dx) as f32 + 0.5 - x, (cy + dy) as f32 + 0.5 - y);
                                let w = (1.0 - ((qx * qx + qy * qy).sqrt() - half) / 1.0).clamp(0.0, 1.0);
                                if w <= 0.0 {
                                    continue;
                                }
                                let (px, py) = ((cx + dx).rem_euclid(s as i32) as usize, (cy + dy).rem_euclid(hs as i32) as usize);
                                let q = &mut t.px[py * s + px];
                                let amt = (w * fade * strength * 0.22).min(1.0);
                                let g = if !dark { q[0] + (1.0 - q[0]) * amt } else { q[0] * (1.0 - amt) };
                                *q = [g, g, g, 1.0];
                            }
                        }
                    }
                }
            }
            overlay(&mut t, 74, f("specks", 0.0), (scale * 2.0).min(256.0), 502, 0);
            overlay(&mut t, 60, f("streaks", 0.0), scale, 503, 2);
            let cr = f("cracks", 0.0).clamp(0.0, 1.0);
            if cr > 1e-3 {
                let px = (scale * 6.0).clamp(2.0, 96.0) as i32;
                for y in 0..t.h {
                    for x in 0..t.w {
                        let (u, v) = ((x as f32 + 0.5) / t.w as f32, (y as f32 + 0.5) / t.h as f32);
                        let (f1, f2, _) = noise::voronoi(u, v, px, px, seed.wrapping_add(601), 1.0);
                        let e = 1.0 - ((f2 - f1) / 0.06).clamp(0.0, 1.0);
                        let p = &mut t.px[y * t.w + x];
                        let g = p[0] * (1.0 - cr * e);
                        *p = [g, g, g, 1.0];
                    }
                }
            }
            let grain = f("grain", 0.0).clamp(-1.0, 1.0);
            if grain < -1e-3 {
                // negative grain smooths the fine noise away
                let soft = ops::blur(&t, 2.0);
                for (p, q) in t.px.iter_mut().zip(soft.px.iter()) {
                    let g = p[0] + (q[0] - p[0]) * (-grain);
                    *p = [g, g, g, 1.0];
                }
            } else if grain > 1e-3 {
                for (i, p) in t.px.iter_mut().enumerate() {
                    let r = noise_styles::unit((i % s) as i32, (i / s) as i32, seed.wrapping_add(701)) - 0.5;
                    let g = (p[0] + r * grain * 0.6).clamp(0.0, 1.0);
                    *p = [g, g, g, 1.0];
                }
            }
            let bal = f("balance", 0.5) - 0.5;
            if bal.abs() > 1e-4 {
                for p in t.px.iter_mut() {
                    let g = (p[0] + bal).clamp(0.0, 1.0);
                    *p = [g, g, g, 1.0];
                }
            }
            let sh = f("sharpen", 0.0).clamp(-4.0, 4.0);
            if sh < -1e-3 {
                t = ops::blur(&t, 1.5 * (-sh).min(2.0));
            } else if sh > 1e-3 {
                t = ops::sharpen(&t, 1.5, sh);
                for p in t.px.iter_mut() {
                    let g = p[0].clamp(0.0, 1.0);
                    *p = [g, g, g, 1.0];
                }
            } else if sh < -1e-3 {
                for p in t.px.iter_mut() {
                    *p = [p[0], p[0], p[0], 1.0];
                }
            }
            fin(t)
        }
        "gradient_2pt" => {
            let mode = f("mode", 1.0).round() as i32;
            let (p1, p2) = ([f("p1_x", 0.5), f("p1_y", 0.0)], [f("p2_x", 0.5), f("p2_y", 1.0)]);
            let til = f("tiling", 1.0).round().max(1.0);
            let square = f("radial_shape", 0.0) > 0.5;
            let peak = f("peak", 0.5).clamp(0.0, 1.0);
            let profile = f("profile", 0.0).round() as i32;
            let a_off = f("angle", 0.0);
            let (dx, dy) = (p2[0] - p1[0], p2[1] - p1[1]);
            let len2 = (dx * dx + dy * dy).max(1e-8);
            let len = len2.sqrt();
            fin(Tex::gray_fn(s, hs, move |u, v| {
                let (x, y) = (u - p1[0], v - p1[1]);
                let t = match mode {
                    // along the line from point 1 (0) to point 2 (1)
                    0 | 1 => {
                        let t = (x * dx + y * dy) / len2;
                        let t = if til > 1.0 { (t * til).rem_euclid(1.0) } else { t.clamp(0.0, 1.0) };
                        if mode == 0 {
                            t
                        } else if t <= peak {
                            // 0 at both points, 1 at the peak between them
                            if peak > 0.0 { t / peak } else { 1.0 }
                        } else if peak < 1.0 {
                            (1.0 - t) / (1.0 - peak)
                        } else {
                            1.0
                        }
                    }
                    // angle around point 1, starting towards point 2, counter-clockwise on screen
                    2 => {
                        let a0 = (-dy).atan2(dx) + a_off * std::f32::consts::TAU;
                        let a = (-y).atan2(x);
                        1.0 - ((a - a0) / std::f32::consts::TAU).rem_euclid(1.0)
                    }
                    // distance from point 1, 1 at the centre and 0 at point 2's distance
                    _ => {
                        let d = if square { x.abs().max(y.abs()) } else { (x * x + y * y).sqrt() };
                        1.0 - d / len.max(1e-6)
                    }
                };
                let t = t.clamp(0.0, 1.0);
                match profile {
                    1 => 1.0 - (1.0 - t) * (1.0 - t),
                    2 => t * t,
                    3 => t * t * (3.0 - 2.0 * t),
                    _ => t,
                }
            }))
        }
        "weave_preset" => {
            let preset = f("preset", 0.0).round().clamp(0.0, (WEAVE_PRESETS.len() - 1) as f32) as usize;
            let til = f("tiling", 1.0).round().clamp(1.0, 64.0);
            let r45 = f("rotate45", 0.0) > 0.5;
            let scale = f("scale", 8.0).round().clamp(1.0, 64.0);
            let dis = f("disorder", 0.0).clamp(0.0, 1.0);
            let px = til * if r45 { std::f32::consts::SQRT_2 } else { 1.0 } / s as f32;
            let warp_cells = (4.0 * til) as i32;
            fin(Tex::gray_fn(s, hs, move |u, v| {
                // turned by 45 degrees: the diagonals become the axes and the result still tiles
                let (u, v) = if r45 { (u + v, v - u) } else { (u, v) };
                let (mut u, mut v) = (u * til, v * til);
                if dis > 0.0 {
                    let (a, b) = (u / til, v / til);
                    u += dis * 0.06 * noise_styles::gradient(a, b, warp_cells, warp_cells, seed);
                    v += dis * 0.06 * noise_styles::gradient(a, b, warp_cells, warp_cells, seed.wrapping_add(5));
                }
                weave_value(preset, u.rem_euclid(1.0), v.rem_euclid(1.0), scale, px).clamp(0.0, 1.0)
            }))
        }
        "tile_preset" => {
            let preset = f("preset", 0.0).round().clamp(0.0, (TILE_PRESETS.len() - 1) as f32) as usize;
            let tp = TileParams {
                tiling: f("tiling", 1.0).round().clamp(1.0, 64.0),
                interstice: f("interstice", 0.1).clamp(0.0, 1.0),
                smooth: f("edge_smoothness", 0.0).clamp(0.0, 1.0),
                round: f("roundness", 0.1).clamp(0.0, 1.0),
                lum_var: f("lum_var", 0.0).clamp(0.0, 1.0),
                bricks: [f("bricks_x", 4.0).round().clamp(1.0, 256.0), f("bricks_y", 8.0).round().clamp(1.0, 256.0)],
                bevel: [f("bevel_x", 0.5).clamp(0.0, 1.0), f("bevel_y", 0.5).clamp(0.0, 1.0)],
                gap: [f("gap_x", 0.0).clamp(0.0, 1.0), f("gap_y", 0.0).clamp(0.0, 1.0)],
                offset: f("offset", 0.5),
                variant: f("variant", 0.0).round() as i32,
                size_random: f("size_random", 0.5).clamp(0.0, 1.0),
                px: 1.0 / s as f32,
                seed,
            };
            fin(Tex::gray_fn(s, hs, move |u, v| tile_value(preset, u, v, &tp).clamp(0.0, 1.0)))
        }
        "noise_3d" => {
            let q = Noise3 {
                kind: f("type", 0.0).round() as i32,
                scale: f("scale", 8.0).max(0.0),
                size: [f("size_x", 1.0), f("size_y", 1.0), f("size_z", 1.0)],
                offset: [f("offset_x", 0.0), f("offset_y", 0.0), f("offset_z", 0.0)],
                distortion: f("distortion", 0.0).clamp(0.0, 1.0),
                distortion_scale: f("distortion_scale", 1.0).clamp(0.0, 4.0),
                baseline: f("baseline", 0.5),
                contrast: f("noise_contrast", 0.0).clamp(0.0, 1.0),
                absolute: f("absolute", 0.0) > 0.5,
                levels: [f("min_level", 1.0).round().clamp(0.0, 12.0) as i32, f("max_level", 7.0).round().clamp(0.0, 12.0) as i32],
                roughness: f("roughness", 0.5).clamp(0.0, 2.0),
                lacunarity: f("lacunarity", 2.0).clamp(1.0, 4.0),
                opacity: f("opacity", 1.0).clamp(0.0, 2.0),
                jitter: f("jitter", 1.0).clamp(0.0, 1.0),
                style: f("style", 0.0).round() as i32,
                edge: f("edge_thickness", 0.1).clamp(0.0, 0.5),
                seed,
            };
            let pos = ins.get("position").cloned();
            fin(Tex::gray_fn(s, hs, move |u, v| {
                // without a position map the noise is read on the texture plane, tiling with the scale
                let (p, tile) = match &pos {
                    Some(t) => {
                        let c = t.sample(u, v);
                        ([c[0], c[1], c[2]], false)
                    }
                    None => ([u, v, 0.0], true),
                };
                q.at(p, tile).clamp(0.0, 1.0)
            }))
        }
        "noise_pattern" => {
            let preset = f("preset", 0.0).max(0.0) as usize;
            let lat = f("scale", 32.0).clamp(1.0, 512.0);
            fin(noise_pattern(preset, lat, s, hs, seed, &warp, n, _ins))
        }
        _ => return crate::nodes10::compute(kind, _id, n, ins, s, hs, seed),
    };
    Ok(Some(out))
}

/// What a point of the image knows about its cell (lattice units: one cell is 1 wide).
struct Cell {
    f1: f32,
    /// distance to the edge of its cell (to the nearest bisector)
    edge: f32,
    /// random value of its cell and of the cell across the nearest edge
    val: f32,
    val2: f32,
    /// offset from the cell's point (lattice units)
    dx: f32,
    dy: f32,
    /// position of the cell's point in uv
    pu: f32,
    pv: f32,
}

/// The points of every lattice cell, worked out once per image: the random offset of a cell's point and the hash that
/// gives the cell its value only depend on the (wrapped) cell, yet every pixel looks at the 25 or 49 cells around it.
struct CellGrid {
    p: i32,
    q: i32,
    /// how far the search reaches: 3 cells when points can stray far, else 2
    r: i32,
    /// per wrapped cell: offset of its point inside the cell, and the hash that stands for the cell's value
    tab: Vec<(f32, f32, u32)>,
    /// tells this grid's cached points from another one's
    id: u64,
}

impl CellGrid {
    /// `jitter` 1 lets points stray up to one and a half cells from their own cell, which gives cells of quite
    /// different sizes; 0 is a regular grid.
    fn new(p: i32, q: i32, seed: u32, jitter: f32) -> CellGrid {
        let amp = 3.0 * jitter;
        let tab = crate::tex::par_map((p * q) as usize, |i| {
            let (wx, wy) = (i as i32 % p, i as i32 / p);
            let h = noise_styles::hash(wx, wy, seed);
            let jx = ((h & 0xffff) as f32 / 65535.0 - 0.5) * amp + 0.5;
            let jy = (((h >> 16) & 0xffff) as f32 / 65535.0 - 0.5) * amp + 0.5;
            (jx, jy, noise_styles::hash(wx, wy, seed ^ 0x5bd1_e995))
        });
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        CellGrid { p, q, r: if jitter > 0.67 { 3 } else { 2 }, tab, id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) }
    }
}

/// The points around one lattice cell, kept between pixels: neighbouring pixels (almost) always lie in the same cell.
struct CellCache {
    grid: u64,
    ix: i32,
    iy: i32,
    n: usize,
    px: [f32; 49],
    py: [f32; 49],
    hv: [u32; 49],
}

thread_local! {
    static CELLS: std::cell::RefCell<CellCache> = const { std::cell::RefCell::new(CellCache { grid: 0, ix: 0, iy: 0, n: 0, px: [0.0; 49], py: [0.0; 49], hv: [0; 49] }) };
}

/// Tileable cellular noise at `p` x `q` cells.
fn cell_at(grid: &CellGrid, u: f32, v: f32) -> Cell {
    let (p, q) = (grid.p, grid.q);
    let (x, y) = (u * p as f32, v * q as f32);
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    CELLS.with(|cc| {
        let mut c = cc.borrow_mut();
        if c.grid != grid.id || c.ix != ix || c.iy != iy {
            let r = grid.r;
            let mut np = 0usize;
            for j in -r..=r {
                let cy = iy + j;
                let wy = if cy >= 0 && cy < q { cy } else { cy.rem_euclid(q) };
                let row = (wy * p) as usize;
                for i in -r..=r {
                    let cx = ix + i;
                    let wx = if cx >= 0 && cx < p { cx } else { cx.rem_euclid(p) };
                    let (jx, jy, hv) = grid.tab[row + wx as usize];
                    c.px[np] = cx as f32 + jx;
                    c.py[np] = cy as f32 + jy;
                    c.hv[np] = hv;
                    np += 1;
                }
            }
            c.grid = grid.id;
            c.ix = ix;
            c.iy = iy;
            c.n = np;
        }
        let np = c.n;
        let (px, py, hv) = (&c.px[..np], &c.py[..np], &c.hv[..np]);
        // squared distance to every point (one straight loop the compiler turns into SIMD code), then the nearest
        let mut dist = [0.0f32; 49];
        for k in 0..np {
            dist[k] = (x - px[k]) * (x - px[k]) + (y - py[k]) * (y - py[k]);
        }
        let (mut best, mut bi) = (f32::MAX, 0usize);
        // the second nearest as well: its edge is a good first guess for the pruning below
        let (mut second, mut si) = (f32::MAX, usize::MAX);
        for k in 0..np {
            let d = dist[k];
            if d < best {
                second = best;
                si = bi;
                best = d;
                bi = k;
            } else if d < second {
                second = d;
                si = k;
            }
        }
        let (bx, by, bh) = (px[bi], py[bi], hv[bi]);
        let (mut edge, mut h2, mut ik) = (f32::MAX, bh, usize::MAX);
        // the distance from the point to the bisector with candidate k (the original arithmetic)
        let edge_of = |k: usize| -> Option<f32> {
            let (ux, uy) = (px[k] - bx, py[k] - by);
            let l = (ux * ux + uy * uy).sqrt();
            if l < 1e-6 {
                return None;
            }
            let (mx, my) = ((px[k] + bx) * 0.5, (py[k] + by) * 0.5);
            Some(((mx - x) * ux + (my - y) * uy) / l)
        };
        if si != usize::MAX {
            if let Some(e) = edge_of(si) {
                edge = e;
                h2 = hv[si];
                ik = si;
            }
        }
        // The distance to the bisector with candidate k is at least (sqrt(d_k) - sqrt(d_best)) / 2, so a candidate whose
        // distance exceeds sqrt(d_best) + 2 * edge cannot win. That needs no square root per candidate: the limit is a
        // squared distance, and it shrinks as nearer edges are found (the margin is far above any rounding).
        let sb = best.sqrt();
        let limit = |edge: f32| {
            let r = sb + 2.0 * (edge + 2e-3);
            r * r
        };
        let mut thr = if edge < f32::MAX { limit(edge) } else { f32::MAX };
        for k in 0..np {
            if dist[k] > thr || k == bi || k == si {
                continue;
            }
            if let Some(e) = edge_of(k) {
                // the first of equally near candidates in lattice order wins, as when they are visited in that order
                if e < edge || (e == edge && k < ik) {
                    edge = e;
                    h2 = hv[k];
                    ik = k;
                    thr = limit(edge);
                }
            }
        }
        let unit = |h: u32| (h >> 8) as f32 / 16_777_216.0;
        Cell {
            f1: best.sqrt(),
            edge: edge.max(0.0),
            val: unit(bh),
            val2: unit(h2),
            dx: x - bx,
            dy: y - by,
            pu: bx / p as f32,
            pv: by / q as f32,
        }
    })
}

/// Names of the Noise Patterns presets (the order is stored in files).
pub const PATTERN_PRESETS: &[&str] = &["Cells 1", "Cells 2", "Cells 3", "Cells 4", "Crystal 1", "Gaussian Noise", "Perlin Noise", "Plasma", "Cells 1 (legacy)", "Cells 2 (legacy)", "Cells 3 (legacy)", "Cells 4 (legacy)", "Crystal 1 (legacy)", "Plasma (legacy)"];

/// Output percentiles 1/10/50/90/99 of each preset at its default settings.
const PATTERN_TONE: &[[f32; 5]] = &[
    [0.5147, 0.7246, 0.9078, 0.9853, 0.9986],
    [0.0, 0.0, 0.0, 1.0, 1.0],
    [0.0, 0.6827, 1.0, 1.0, 1.0],
    [0.0123, 0.1032, 0.4963, 0.9074, 0.9892],
    [0.0013, 0.0137, 0.0888, 0.2727, 0.4935],
    [0.0887, 0.1664, 0.3073, 0.4809, 0.6477],
    [0.1694, 0.3042, 0.5009, 0.6934, 0.8253],
    [0.0022, 0.0221, 0.1174, 0.2974, 0.4763],
    [0.4856, 0.7337, 0.9168, 0.9872, 0.9987],
    [0.0, 0.0, 1.0, 1.0, 1.0],
    [0.8157, 0.9668, 0.9953, 0.9974, 0.9979],
    [0.1059, 0.2157, 0.5844, 0.9393, 0.9843],
    [0.0015, 0.0159, 0.1004, 0.3300, 0.6408],
    [0.0058, 0.0506, 0.2448, 0.5745, 0.8660],
];

#[allow(clippy::too_many_arguments)]
fn noise_pattern(preset: usize, lat: f32, s: usize, hs: usize, seed: u32, warp: &(dyn Fn(f32, f32) -> (f32, f32) + Sync), n: &Node, ins: &BTreeMap<String, Rc<Tex>>) -> Tex {
    let f = |k: &str, d: f32| n.nums.get(k).copied().filter(|v| v.is_finite()).unwrap_or(d);
    let preset = preset.min(PATTERN_PRESETS.len() - 1);
    let p = lat.round().max(1.0) as i32;
    let q = ((lat * hs as f32 / s as f32).round().max(1.0)) as i32;
    let px = 1.0 / (s as f32 / p as f32).max(1e-3); // one pixel in cell units
    let jitter = f("jitter", 1.0).clamp(0.0, 1.0);
    let legacy = preset >= 8;
    let base = match preset {
        8..=12 => preset - 8,
        13 => 7,
        x => x,
    };
    let smooth = |a: f32, b: f32, x: f32| {
        let t = ((x - a) / (b - a).max(1e-6)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    // only the cell presets look at the lattice points
    let grid = if base <= 4 { CellGrid::new(p, q, seed, jitter) } else { CellGrid::new(1, 1, seed, jitter) };
    let mut t = match base {
        // Cells 1: round bubbles that leave dark gaps where three cells meet
        0 => Tex::gray_fn(s, hs, |u, v| {
            let (u, v) = warp(u, v);
            let c = cell_at(&grid, u, v);
            (1.0 - (c.f1 / 1.15) * (c.f1 / 1.15)).max(0.0).sqrt()
        }),
        // Cells 2: white cells with dark borders of even width
        1 => {
            let w = if legacy { 0.098 } else { f("edge_width", 2.0).clamp(0.0, 8.0) * 0.0766 };
            Tex::gray_fn(s, hs, |u, v| {
                let (u, v) = warp(u, v);
                let c = cell_at(&grid, u, v);
                smooth(w - px * 0.5, w + px * 0.5, c.edge)
            })
        }
        // Cells 3: thin lines; each border is as dark as its two cells differ
        2 => {
            let hard = f("hardness", 0.5).clamp(0.0, 1.0);
            let w = px * (1.7 - 1.0 * hard);
            Tex::gray_fn(s, hs, |u, v| {
                let (u, v) = warp(u, v);
                let c = cell_at(&grid, u, v);
                let line = 1.0 - smooth(0.0, w, c.edge);
                let k = (c.val - c.val2).abs().powf(0.6);
                1.0 - line * (0.55 + 0.45 * k)
            })
        }
        // Cells 4: a flat random value per cell (or the colour of an image at the cell's point)
        3 => {
            let src = f("color_source", 0.0).round() as i32;
            let pseed = f("pseudo_random_seed", 0.0).max(0.0) as u32;
            let img = ins.get("input").cloned();
            Tex::gray_fn(s, hs, move |u, v| {
                let (u, v) = warp(u, v);
                let c = cell_at(&grid, u, v);
                match (src, &img) {
                    (2, Some(t)) => crate::tex::lum(t.sample(c.pu, c.pv)),
                    (1, _) => noise::rnd((c.pu * 4096.0) as i32, (c.pv * 4096.0) as i32, pseed.wrapping_add(4242)),
                    _ => c.val,
                }
            })
        }
        // Crystal 1: faceted pyramids (creases run from each point to the corners of its cell), each cell with
        // its own brightness, dark along the borders
        4 => {
            // each cell is a flat facet laid on a smooth noise (its value and slope at the cell's point)
            let (pn, qn) = (((lat / 2.0).round() as i32).max(1), ((lat / 2.0 * hs as f32 / s as f32).round() as i32).max(1));
            Tex::gray_fn(s, hs, |u, v| {
                let (u, v) = warp(u, v);
                let c = cell_at(&grid, u, v);
                let e = 0.25 / p as f32;
                let n0 = noise_styles::gradient(c.pu, c.pv, pn, qn, seed ^ 0x51);
                let gx = (noise_styles::gradient(c.pu + e, c.pv, pn, qn, seed ^ 0x51) - n0) / e;
                let gy = (noise_styles::gradient(c.pu, c.pv + e, pn, qn, seed ^ 0x51) - n0) / e;
                let plane = n0 + (gx * c.dx / p as f32 + gy * c.dy / q as f32) * 3.0;
                (plane * 0.5 + 0.5).max(0.0) * smooth(0.0, 0.12, c.edge)
            })
        }
        // Gaussian Noise: soft blobs, about two per lattice cell
        5 => {
            let (p2, q2) = (((p + 1) / 2).max(1), ((q + 1) / 2).max(1));
            Tex::gray_fn(s, hs, |u, v| {
                let (u, v) = warp(u, v);
                noise_styles::gradient(u, v, p2, q2, seed) + 0.3 * noise_styles::gradient(u, v, p, q, seed.wrapping_add(3))
            })
        }
        // Perlin Noise: its features measure a little more than one lattice cell
        6 => {
            let (p3, q3) = (((lat * 0.75).round() as i32).max(1), ((lat * 0.75 * hs as f32 / s as f32).round() as i32).max(1));
            Tex::gray_fn(s, hs, |u, v| {
                let (u, v) = warp(u, v);
                noise_styles::gradient(u, v, p3, q3, seed)
            })
        }
        // Plasma: dark creases where the noise crosses its middle
        _ => {
            let (p2, q2) = (((p + 1) / 2).max(1), ((q + 1) / 2).max(1));
            Tex::gray_fn(s, hs, |u, v| {
                let (u, v) = warp(u, v);
                let a = noise_styles::gradient(u, v, p2, q2, seed).abs();
                let b = noise_styles::gradient(u, v, p, q, seed.wrapping_add(7)).abs();
                a + 0.3 * b
            })
        }
    };
    noise_styles::tone(&mut t, &PATTERN_TONE[preset]);
    t
}

// ------------------------------------------------------------------------------------------------ weave patterns

/// Rope, fibre and weave patterns, each matched to the look of the library pattern of the same kind.
pub const WEAVE_PRESETS: &[&str] = &["Rope", "Fibers", "Basket weave", "Knit (diagonal)", "Knit (fine)", "Plain weave strips", "Pinwheel basket"];

/// A knit: one curved, shaded stitch per lattice point. Lengths are in lattice pixels (a stitch lattice of 32).
struct Knit {
    /// lattice vectors
    l: [[f32; 2]; 2],
    /// stitch centre offset, half length, half width, tilt from vertical, bend, profile power, side shading
    o: [f32; 2],
    along: f32,
    across: f32,
    tilt: f32,
    bend: f32,
    power: f32,
    side: f32,
    /// brightness of the stitches and of the background
    amp: f32,
    base: f32,
}

const KNIT_DIAGONAL: Knit = Knit { l: [[32.0, 32.0], [32.0, -32.0]], o: [10.768, 35.202], along: 47.113, across: 8.876, tilt: -0.3053, bend: -0.00059, power: 0.837, side: 0.923, amp: 0.641, base: 0.160 };
const KNIT_FINE: Knit = Knit { l: [[32.0, 0.0], [0.0, 32.0]], o: [14.163, 16.972], along: 33.271, across: 5.274, tilt: 0.4883, bend: -0.00321, power: 1.004, side: 1.118, amp: 0.655, base: 0.113 };

fn knit(k: &Knit, x: f32, y: f32) -> f32 {
    let det = k.l[0][0] * k.l[1][1] - k.l[1][0] * k.l[0][1];
    let (dx, dy) = (x - k.o[0], y - k.o[1]);
    // lattice coordinates of the point
    let c0 = ((dx * k.l[1][1] - dy * k.l[1][0]) / det).floor();
    let c1 = ((-dx * k.l[0][1] + dy * k.l[0][0]) / det).floor();
    let (st, ct) = k.tilt.sin_cos();
    let mut best = 0.0f32;
    for i in -2..=2 {
        for j in -2..=2 {
            let (a, b) = (c0 + i as f32, c1 + j as f32);
            let ex = dx - (a * k.l[0][0] + b * k.l[1][0]);
            let ey = dy - (a * k.l[0][1] + b * k.l[1][1]);
            let s = ex * st + ey * ct;
            let c = ex * ct - ey * st - k.bend * s * s;
            let r2 = (s / k.along).powi(2) + (c / k.across).powi(2);
            if r2 < 1.0 {
                let v = (1.0 - r2).powf(k.power) * (1.0 + k.side * c / k.across).clamp(0.0, 2.0);
                best = best.max(v);
            }
        }
    }
    k.amp * best + k.base
}

#[inline]
fn fract(x: f32) -> f32 {
    x - x.floor()
}

/// One pattern tile; `u`, `v` in 0..1, `px` the size of a pixel in tile units.
fn weave_value(preset: usize, u: f32, v: f32, scale: f32, px: f32) -> f32 {
    use std::f32::consts::PI;
    match preset {
        // a twisted rope across the tile: a soft round profile with slanted strands
        0 => {
            let w = v - 0.5;
            let tube = (-0.5 * (w / 0.1725).powi(2)).exp();
            let t = fract(4.0 * u - 1.6 * w);
            tube * (1.0 - 0.2 * (1.0 - (PI * t).sin()))
        }
        // two thinner ropes whose strands are deeper towards their edges
        1 => {
            let w = fract(2.0 * v + 0.5) - 0.5;
            let d = w.abs();
            let tube = (PI * d).cos().max(0.0).powf(1.05);
            let k = 0.28 + 0.72 * (2.0 * d).powi(4);
            let t = fract(4.0 * u - 0.8 * w);
            tube * (1.0 - k * (1.0 - (PI * t).sin()))
        }
        // basket weave: 2 x 2 blocks of five rounded threads, alternating direction
        2 => {
            let (bx, by) = ((2.0 * u).floor(), (2.0 * v).floor());
            let (bu, bv) = (fract(2.0 * u), fract(2.0 * v));
            let (across, along) = if bx == by { (bu, bv) } else { (bv, bu) };
            let a = fract(5.0 * across) - 0.494;
            let (hw, cap) = (0.4258, 0.111);
            let beyond = (cap - along).max(along - (1.0 - cap)).max(0.0);
            (1.0 - (a / hw).powi(2) - (beyond / cap).powi(2)).max(0.0)
        }
        // knits: rows of shaded, curved stitches
        3 | 4 => {
            let (k, unit) = if preset == 3 { (&KNIT_DIAGONAL, 512.0) } else { (&KNIT_FINE, 64.0 * scale) };
            knit(k, u * unit, v * unit)
        }
        // plain weave of two strips each way, each passing over and under, with a soft shadow where it goes under
        5 => {
            let du = ((fract(2.0 * u) - 0.5).abs() - 0.2) * 0.5;
            let dv = ((fract(2.0 * v) - 0.5).abs() - 0.2) * 0.5;
            let in_v = (0.5 - du / px).clamp(0.0, 1.0);
            let in_h = (0.5 - dv / px).clamp(0.0, 1.0);
            let vertical_over = ((2.0 * u).floor() + (2.0 * v).floor()) as i32 % 2 == 0;
            let shade = |d: f32| if d > 0.0 { 0.26 * (1.0 - d / 0.0254).clamp(0.0, 1.0) } else { 0.0 };
            let h = in_h * (1.0 - if vertical_over { shade(du) } else { 0.0 });
            let vv = in_v * (1.0 - if vertical_over { 0.0 } else { shade(dv) });
            h.max(vv)
        }
        // pinwheel basket: four wide strips that fade where they pass under their neighbours
        _ => {
            let (len, wid, ramp) = (0.625, 0.375, 0.125);
            let mut out = 0.0f32;
            for (horizontal, x0, y0) in [(true, -0.0625, 0.0625), (false, 0.5625, -0.0625), (true, 0.4375, 0.5625), (false, 0.0625, 0.4375)] {
                let (tx, ty) = (fract(u - x0), fract(v - y0));
                let (t, c) = if horizontal { (tx, ty) } else { (ty, tx) };
                if t < len && c < wid {
                    out = out.max((t / ramp).clamp(0.0, 1.0) * ((len - t) / ramp).clamp(0.0, 1.0));
                }
            }
            out
        }
    }
}

// ------------------------------------------------------------------------------------------------ tile patterns

/// Bricks, cells and spots, each matched to the look of the library pattern of the same kind.
pub const TILE_PRESETS: &[&str] = &["Brick 1 (running bond)", "Brick 2 (long and short rows)", "Bricks 3 (shaded)", "Brick generator", "Alveolus", "Gaussian spots 1", "Gaussian spots 2", "Mesh 1", "Mesh 2", "Parquet", "Random tiles (rows)", "Random tiles (blocks)", "Arched pavement"];

struct TileParams {
    tiling: f32,
    interstice: f32,
    smooth: f32,
    round: f32,
    lum_var: f32,
    bricks: [f32; 2],
    bevel: [f32; 2],
    gap: [f32; 2],
    offset: f32,
    variant: i32,
    size_random: f32,
    /// one pixel in tile units
    px: f32,
    seed: u32,
}

/// A white box with a soft or hard edge: `dx`, `dy` are the distances inside its edges (negative outside), `soft` the
/// edge width, `r` the corner radius.
fn boxed(dx: f32, dy: f32, soft: f32, r: f32, aa: f32) -> f32 {
    let d = if r > 0.0 && dx < r && dy < r {
        let (a, b) = (r - dx, r - dy);
        r - (a * a + b * b).sqrt()
    } else {
        dx.min(dy)
    };
    if soft > aa {
        (d / soft).clamp(0.0, 1.0)
    } else {
        (d / aa + 0.5).clamp(0.0, 1.0)
    }
}

/// Bricks in rows of height `h` (tile units): row `j` has bricks `len` long shifted by `shift` of a brick; gaps of `gap`
/// between them. Returns (inside distance x, inside distance y, brick column, row).
fn brick_cell(u: f32, v: f32, h: f32, len: f32, shift: f32, gap: [f32; 2]) -> (f32, f32, i32, i32) {
    let j = (v / h).floor();
    let fy = v - j * h;
    let x = u / len + shift;
    let i = x.floor();
    let fx = (x - i) * len;
    let dx = fx.min(len - fx) - gap[0] * 0.5;
    let dy = fy.min(h - fy) - gap[1] * 0.5;
    (dx, dy, i as i32, j as i32)
}

fn tile_value(preset: usize, u: f32, v: f32, p: &TileParams) -> f32 {
    let t = p.tiling;
    let (u, v) = (u * t, v * t);
    let aa = p.px * t;
    match preset {
        // running bond: two rows per tile, bricks twice as long as high, every other row shifted by half a brick
        0 => {
            let h = 0.5;
            let j = (v / h).floor() as i32;
            let g = p.interstice * h;
            let (dx, dy, _, _) = brick_cell(u, v, h, 2.0 * h, if j & 1 == 1 { 0.5 } else { 0.0 }, [g, g]);
            boxed(dx, dy, p.smooth * h * 0.5, 0.0, aa)
        }
        // long bricks and half-long bricks in alternate rows
        1 => {
            let h = 0.25;
            let j = (v / h).floor() as i32;
            let g = p.interstice * h;
            let (len, shift) = if j & 1 == 0 { (4.0 * h, if p.variant == 1 { 0.5 } else { 0.0 }) } else { (2.0 * h, 0.35) };
            let (dx, dy, _, _) = brick_cell(u, v, h, len, shift, [g, g]);
            boxed(dx, dy, p.smooth * h * 0.5, 0.0, aa)
        }
        // small bricks with rounded corners and random shades, many of them white
        2 => {
            let h = 1.0 / 16.0;
            let j = (v / h).floor() as i32;
            let (dx, dy, i, jj) = brick_cell(u, v, h, 2.0 * h, if j & 1 == 1 { 0.5 } else { 0.0 }, [0.09 * h, 0.09 * h]);
            let cols = (8.0 * t) as i32;
            let rows = (16.0 * t) as i32;
            let r = noise_styles::unit(i.rem_euclid(cols), jj.rem_euclid(rows), p.seed);
            let lum = (1.0 - 0.64 * p.lum_var + 1.56 * p.lum_var * r).min(1.0);
            boxed(dx, dy, 0.0, p.round * h, aa) * lum
        }
        // general bricks: counts, gaps, bevels and row offset
        3 => {
            let (w, h) = (1.0 / p.bricks[0], 1.0 / p.bricks[1]);
            let j = (v / h).floor() as i32;
            let (dx, dy, _, _) = brick_cell(u, v, h, w, if j & 1 == 1 { p.offset } else { 0.0 }, [p.gap[0] * w, p.gap[1] * h]);
            let base = w.min(h) * 0.5;
            let bx = if p.bevel[0] > 0.0 { (dx / (p.bevel[0] * base)).clamp(0.0, 1.0) } else { (dx / aa + 0.5).clamp(0.0, 1.0) };
            let by = if p.bevel[1] > 0.0 { (dy / (p.bevel[1] * base)).clamp(0.0, 1.0) } else { (dy / aa + 0.5).clamp(0.0, 1.0) };
            bx.min(by)
        }
        // alveolus: soft round cells on a staggered row of four
        4 => {
            let mut acc = 0.0;
            for k in 0..4 {
                let cx = 0.125 + 0.25 * k as f32;
                let cy = if k & 1 == 1 { 0.25 } else { 0.75 };
                let dx = fract(u - cx + 0.5) - 0.5;
                for oy in [-1.0, 0.0, 1.0] {
                    let dy = fract(v) - cy + oy;
                    let r2 = (dx * dx + dy * dy) / (0.3125 * 0.3125);
                    if r2 < 1.0 {
                        acc += (1.0 - r2) * (1.0 - r2);
                    }
                }
            }
            acc
        }
        // gaussian spots: on a diagonal lattice (two per tile) or a staggered one (four per tile)
        5 | 6 => {
            let (pts, sigma): (&[(f32, f32)], f32) = if preset == 5 { (&[(0.0, 0.0), (0.5, 0.5)], 0.150) } else { (&[(0.0, 0.25), (0.5, 0.25), (0.25, 0.75), (0.75, 0.75)], 0.075) };
            let mut acc = 0.0;
            for &(cx, cy) in pts {
                for ox in [-1.0, 0.0, 1.0] {
                    for oy in [-1.0, 0.0, 1.0] {
                        let (dx, dy) = (fract(u) - cx + ox, fract(v) - cy + oy);
                        acc += (-(dx * dx + dy * dy) / (2.0 * sigma * sigma)).exp();
                    }
                }
            }
            acc
        }
        // meshes: threads crossing, two across and two along
        7 | 8 => {
            let mut acc = 0.0f32;
            for &(cx, cy, vert) in &[(0.5f32, 0.5f32, false), (0.0, 0.0, false), (0.5, 0.0, true), (0.0, 0.5, true)] {
                for ox in [-1.0, 0.0, 1.0] {
                    for oy in [-1.0, 0.0, 1.0] {
                        let (mut dx, mut dy) = (fract(u) - cx + ox, fract(v) - cy + oy);
                        if vert {
                            std::mem::swap(&mut dx, &mut dy);
                        }
                        let m = if preset == 7 { &MESH1 } else { &MESH2 };
                        acc += m[4] * (-(dx.abs() / m[0]).powf(m[2]) - (dy.abs() / m[1]).powf(m[3])).exp();
                    }
                }
            }
            acc
        }
        // random tiles in rows: cells of random widths, each split once across or along at random
        10 => {
            let (nx, ny) = (p.bricks[0].max(1.0), p.bricks[1].max(1.0));
            let h = 1.0 / ny;
            let rows = (ny * t) as i32;
            let j = (v / h).floor() as i32;
            let fy = v / h - j as f32;
            let jj = j.rem_euclid(rows);
            let un = fract(u);
            // cell edges of this row: random widths, normalised to the tile
            let n = nx as i32;
            let wid = |k: i32| 1.0 + 0.9 * p.size_random * (2.0 * noise_styles::unit(k, jj, p.seed.wrapping_add(41)) - 1.0);
            let total: f32 = (0..n).map(wid).sum();
            let (mut x0, mut k) = (0.0f32, 0);
            let mut x1 = wid(0) / total;
            while un >= x1 && k < n - 1 {
                k += 1;
                x0 = x1;
                x1 += wid(k) / total;
            }
            let fx = (un - x0) / (x1 - x0).max(1e-6);
            let cw = (x1 - x0) / t;
            let ch = h / t;
            // split: across (two stacked) or along (two side by side) at a random place
            let r = noise_styles::unit(k, jj, p.seed.wrapping_add(43));
            let at = 0.3 + 0.4 * noise_styles::unit(k, jj, p.seed.wrapping_add(47));
            let (mut lx, mut ly, mut sx, mut sy) = (fx, fy, cw, ch);
            if r < 0.35 {
                if ly < at {
                    ly /= at;
                    sy *= at;
                } else {
                    ly = (ly - at) / (1.0 - at);
                    sy *= 1.0 - at;
                }
            } else if r < 0.7 {
                if lx < at {
                    lx /= at;
                    sx *= at;
                } else {
                    lx = (lx - at) / (1.0 - at);
                    sx *= 1.0 - at;
                }
            }
            let g = p.interstice * 0.06 / t;
            let dx = lx.min(1.0 - lx) * sx - g * 0.5;
            let dy = ly.min(1.0 - ly) * sy - g * 0.5;
            boxed(dx * t, dy * t, 0.0, 0.0, aa)
        }
        // arched pavement: rows of rounded stones along circular arcs (one arch per tile, so the rows meet in a point at the
        // tile edge); every stone is turned to follow its row
        12 => {
            let (cols, rows, rad, gap) = (16.0f32, 19.0f32, 0.9f32, 0.025f32);
            let arc = |x: f32| {
                let d = x - 0.5;
                rad - (rad * rad - d * d).max(0.0).sqrt()
            };
            let fu = fract(u);
            let ci = (fu * cols).floor();
            let mut best = 0.0f32;
            for di in -1..=1 {
                let uc = (ci + di as f32 + 0.5) / cols;
                let w = uc.rem_euclid(1.0);
                let lift = arc(w);
                let (sn, cs) = ((w - 0.5) / (rad * rad - (w - 0.5) * (w - 0.5)).max(1e-6).sqrt()).atan().sin_cos();
                let rj = ((v - lift) * rows).floor();
                for dj in -1..=1 {
                    let vc = (rj + dj as f32 + 0.5) / rows + lift;
                    let (du, dv) = (fu - uc, v - vc);
                    let lx = (du * cs + dv * sn) * cols;
                    let ly = (-du * sn + dv * cs) * rows;
                    best = best.max(boxed(0.5 - lx.abs() - gap, 0.5 - ly.abs() - gap, 0.1, 0.05 + 0.3 * p.round, aa * cols));
                }
            }
            best
        }
        // random tiles in blocks: a grid whose pairs of cells are joined at random into larger tiles, each tile
        // shrunk a little at random and bevelled
        11 => {
            let (nx, ny) = (p.bricks[0].max(2.0), p.bricks[1].max(2.0));
            let (bx, by) = ((u * nx * 0.5).floor() as i32, (v * ny * 0.5).floor() as i32);
            let (cols, rows) = ((nx * 0.5 * t) as i32, (ny * 0.5 * t) as i32);
            let (hx, hy) = (bx.rem_euclid(cols.max(1)), by.rem_euclid(rows.max(1)));
            let (fx, fy) = (fract(u * nx * 0.5), fract(v * ny * 0.5));
            // each 2 x 2 block is one tile, two halves (either way) or four quarters
            let kind = (noise_styles::unit(hx, hy, p.seed.wrapping_add(51)) * 4.0) as i32;
            let (qx, qy) = match kind {
                0 => (1, 1),
                1 => (2, 1),
                2 => (1, 2),
                _ => (2, 2),
            };
            let (ix, iy) = ((fx * qx as f32).floor(), (fy * qy as f32).floor());
            let (lx, ly) = (fract(fx * qx as f32), fract(fy * qy as f32));
            let key = noise_styles::unit(hx * 4 + ix as i32, hy * 4 + iy as i32, p.seed.wrapping_add(53));
            let shrink = 1.0 - 0.3 * p.size_random * key;
            let (sx, sy) = (2.0 / (nx * qx as f32) / t, 2.0 / (ny * qy as f32) / t);
            let (cx, cy) = ((lx - 0.5).abs() * 2.0, (ly - 0.5).abs() * 2.0);
            let dx = (shrink - cx) * 0.5 * sx;
            let dy = (shrink - cy) * 0.5 * sy;
            let bevel = p.bevel[0].max(0.0) * 0.4 * sx.min(sy) * t;
            boxed(dx * t, dy * t, bevel, 0.0, aa)
        }
        // parquet: eight rows of planks as long as the tile, each with one joint at a random place
        _ => {
            let rows = 8.0;
            let h = 1.0 / rows;
            let j = (v / h).floor() as i32;
            let r = noise_styles::unit(j.rem_euclid((rows * t) as i32), 3, p.seed);
            let line = 2.0 * aa;
            let (dx, dy, i, _) = brick_cell(u, v, h, 1.0, r, [line, line]);
            let rl = noise_styles::unit(i, j, p.seed.wrapping_add(9));
            boxed(dx, dy, 0.0, 0.0, aa) * (1.0 - p.lum_var * rl)
        }
    }
}

/// Fitted shapes of the two meshes (threads summed): sizes along and across, their exponents, brightness.
const MESH1: [f32; 5] = [0.2472, 0.0742, 2.3025, 1.6234, 0.9407];
const MESH2: [f32; 5] = [0.3077, 0.1705, 8.0072, 3.6528, 0.9763];

// ------------------------------------------------------------------------------------------------ 3D noises

/// Noise read at 3D positions (a position map, or the texture plane).
struct Noise3 {
    /// 0 perlin, 1 perlin fractal, 2 ridged fractal, 3 simplex-like, 4 worley, 5 voronoi, 6 voronoi fractal
    kind: i32,
    scale: f32,
    size: [f32; 3],
    offset: [f32; 3],
    distortion: f32,
    distortion_scale: f32,
    baseline: f32,
    contrast: f32,
    absolute: bool,
    levels: [i32; 2],
    roughness: f32,
    lacunarity: f32,
    opacity: f32,
    jitter: f32,
    style: i32,
    edge: f32,
    seed: u32,
}

#[inline]
fn hash3(x: i32, y: i32, z: i32, seed: u32) -> u32 {
    noise_styles::hash(x, y.wrapping_add(z.wrapping_mul(7919)), seed)
}

/// Gradient noise in 3D (about -1..1); `per` > 0 wraps x and y with that period.
fn grad3(x: f32, y: f32, z: f32, per: i32, seed: u32) -> f32 {
    let (fx, fy, fz) = (x.floor(), y.floor(), z.floor());
    let (ix, iy, iz) = (fx as i32, fy as i32, fz as i32);
    let (tx, ty, tz) = (x - fx, y - fy, z - fz);
    let wrap = |a: i32| if per > 0 { a.rem_euclid(per) } else { a };
    const G: [[f32; 3]; 12] = [[1.0, 1.0, 0.0], [-1.0, 1.0, 0.0], [1.0, -1.0, 0.0], [-1.0, -1.0, 0.0], [1.0, 0.0, 1.0], [-1.0, 0.0, 1.0], [1.0, 0.0, -1.0], [-1.0, 0.0, -1.0], [0.0, 1.0, 1.0], [0.0, -1.0, 1.0], [0.0, 1.0, -1.0], [0.0, -1.0, -1.0]];
    let g = |cx: i32, cy: i32, cz: i32, dx: f32, dy: f32, dz: f32| {
        let d = G[(hash3(wrap(cx), wrap(cy), cz, seed) % 12) as usize];
        d[0] * dx + d[1] * dy + d[2] * dz
    };
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (sx, sy, sz) = (fade(tx), fade(ty), fade(tz));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(g(ix, iy, iz, tx, ty, tz), g(ix + 1, iy, iz, tx - 1.0, ty, tz), sx);
    let x10 = lerp(g(ix, iy + 1, iz, tx, ty - 1.0, tz), g(ix + 1, iy + 1, iz, tx - 1.0, ty - 1.0, tz), sx);
    let x01 = lerp(g(ix, iy, iz + 1, tx, ty, tz - 1.0), g(ix + 1, iy, iz + 1, tx - 1.0, ty, tz - 1.0), sx);
    let x11 = lerp(g(ix, iy + 1, iz + 1, tx, ty - 1.0, tz - 1.0), g(ix + 1, iy + 1, iz + 1, tx - 1.0, ty - 1.0, tz - 1.0), sx);
    lerp(lerp(x00, x10, sy), lerp(x01, x11, sy), sz)
}

/// Nearest and second nearest feature points in 3D: (f1, f2, value of the nearest cell).
fn cells3(x: f32, y: f32, z: f32, per: i32, jitter: f32, seed: u32) -> (f32, f32, f32) {
    let (ix, iy, iz) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let wrap = |a: i32| if per > 0 { a.rem_euclid(per) } else { a };
    let (mut f1, mut f2, mut val) = (f32::MAX, f32::MAX, 0.0);
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (cx, cy, cz) = (ix + dx, iy + dy, iz + dz);
                let h = hash3(wrap(cx), wrap(cy), cz, seed);
                let r = |k: u32| ((h.rotate_left(k * 11) >> 8) as f32 / 16_777_216.0 - 0.5) * jitter + 0.5;
                let (px, py, pz) = (cx as f32 + r(0), cy as f32 + r(1), cz as f32 + r(2));
                let d = ((x - px).powi(2) + (y - py).powi(2) + (z - pz).powi(2)).sqrt();
                if d < f1 {
                    f2 = f1;
                    f1 = d;
                    val = (h >> 8) as f32 / 16_777_216.0;
                } else if d < f2 {
                    f2 = d;
                }
            }
        }
    }
    (f1, f2, val)
}

impl Noise3 {
    fn at(&self, p: [f32; 3], tile: bool) -> f32 {
        let sc = self.scale.max(1e-3);
        let mut q = [(p[0] + self.offset[0]) * sc * self.size[0], (p[1] + self.offset[1]) * sc * self.size[1], (p[2] + self.offset[2]) * sc * self.size[2]];
        // tiling on the texture plane needs a whole number of cells across, the same on both axes
        let per = if tile && (self.size[0] - self.size[1]).abs() < 1e-4 && self.offset[0] == 0.0 && self.offset[1] == 0.0 { (sc * self.size[0]).round().max(1.0) as i32 } else { 0 };
        if per > 0 {
            q = [p[0] * per as f32, p[1] * per as f32, q[2]];
        }
        if self.distortion > 0.0 {
            let ds = self.distortion_scale.max(0.05);
            let dp = if per > 0 { ((per as f32 * ds).round() as i32).max(1) } else { 0 };
            let a = grad3(q[0] * ds, q[1] * ds, q[2] * ds, dp, self.seed.wrapping_add(101));
            let b = grad3(q[0] * ds + 5.2, q[1] * ds + 1.3, q[2] * ds, dp, self.seed.wrapping_add(102));
            q[0] += a * self.distortion * 4.0;
            q[1] += b * self.distortion * 4.0;
        }
        let tone = |n: f32| {
            let n = if self.absolute { n.abs() * 2.0 - 1.0 } else { n };
            self.baseline + 0.5 * n * (1.0 + 4.0 * self.contrast)
        };
        match self.kind {
            0 | 3 => tone(grad3(q[0], q[1], q[2], per, self.seed) * 1.4),
            1 | 2 => {
                let (lo, hi) = (self.levels[0].min(self.levels[1]), self.levels[0].max(self.levels[1]));
                let (mut sum, mut norm, mut amp, mut fr) = (0.0f32, 0.0f32, 1.0f32, 1.0f32);
                for k in 0..=hi {
                    if k >= lo {
                        let pp = if per > 0 { (per as f32 * fr).round() as i32 } else { 0 };
                        let n = grad3(q[0] * fr, q[1] * fr, q[2] * fr, pp, self.seed.wrapping_add(k as u32 * 31)) * 1.4;
                        sum += amp * if self.kind == 2 { 1.0 - 2.0 * n.abs() } else { n };
                        norm += amp;
                    }
                    amp *= self.roughness;
                    fr *= self.lacunarity;
                }
                tone(self.opacity * sum / norm.max(1e-6))
            }
            _ => {
                let one = |q: [f32; 3], per: i32, seed: u32| {
                    let (f1, f2, val) = cells3(q[0], q[1], q[2], per, self.jitter, seed);
                    match self.style {
                        1 => val,
                        2 => (((f2 - f1) / self.edge.max(1e-3)).min(1.0)).clamp(0.0, 1.0),
                        3 => (f2 * 0.7).min(1.0),
                        4 => ((f2 - f1) * 1.4).min(1.0),
                        _ => (f1 * 1.15).min(1.0),
                    }
                };
                if self.kind == 6 {
                    let (lo, hi) = (self.levels[0].min(self.levels[1]), self.levels[0].max(self.levels[1]));
                    let (mut sum, mut norm, mut amp, mut fr) = (0.0f32, 0.0f32, 1.0f32, 1.0f32);
                    for k in 0..=hi {
                        if k >= lo {
                            let pp = if per > 0 { (per as f32 * fr).round() as i32 } else { 0 };
                            sum += amp * one([q[0] * fr, q[1] * fr, q[2] * fr], pp, self.seed.wrapping_add(k as u32 * 17));
                            norm += amp;
                        }
                        amp *= self.roughness;
                        fr *= self.lacunarity;
                    }
                    self.opacity * sum / norm.max(1e-6)
                } else {
                    one(q, per, self.seed)
                }
            }
        }
    }
}
