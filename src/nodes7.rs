// SPDX-License-Identifier: MIT OR Apache-2.0
//! Seventh set of nodes: pavements (herringbone, arc), noises (fur, moisture, microscope, anisotropic, 3D), 3D volume
//! masks and gradients, extruded and mapped shapes, and the effects / transform / material filters that finished the
//! node library (cross section, uber emboss, diffusion, summed area table, vector morph, safe transform, snow,
//! season, water). Everything is our own construction. Patterns tile seamlessly and work on non-square graphs; pixel
//! sizes are given for a 512 px texture and scaled by `size / 512`. `compute` returns Ok(None) for unknown kinds and
//! `outputs` selects one of several outputs with the hidden `_out` number.
use crate::graph::{Error, Node};
use crate::noise::{self, Fbm};
use crate::ops;
use crate::tex::{lum, par_rows, Tex};
use std::collections::BTreeMap;
use std::f32::consts::{PI, TAU};
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

fn fract(x: f32) -> f32 {
    x - x.floor()
}

fn periods(scale: f32, w: usize, h: usize) -> (i32, i32) {
    let px = scale.round().clamp(1.0, 256.0) as i32;
    (px, ((px as f32 * h as f32 / w as f32).round() as i32).max(1))
}

/// Cell counts for a pattern with `n` cells across the width, both multiples of `m` (so the pattern period divides
/// the texture), keeping cells close to square on a non-square graph.
fn cells_for(n: f32, w: usize, h: usize, m: i32) -> (i32, i32) {
    let nx = ((n / m as f32).round().max(1.0) as i32 * m).clamp(m, 512);
    let ny = (((nx as f32 * h as f32 / w as f32) / m as f32).round().max(1.0) as i32 * m).clamp(m, 2048);
    (nx, ny)
}

fn rh(a: i32, b: i32, c: u32) -> f32 {
    noise::rnd(a, b, c)
}

// ====================================================================== herringbone
/// Owner of each cell of the 4 x 4 herringbone period: (is horizontal, base cell x, base cell y) of its brick.
fn herring_table() -> [[(bool, i32, i32); 4]; 4] {
    let mut t = [[(false, -1, -1); 4]; 4];
    for a in 0..4i32 {
        for b in 0..4i32 {
            let (sx, sy) = ((3 * a - b).rem_euclid(4), (a + b).rem_euclid(4));
            // a vertical brick (sx, sy)-(sx, sy+1), then the horizontal one (sx+1, sy+1)-(sx+2, sy+1)
            let cells = [(false, sx, sy, sx, sy), (false, sx, sy + 1, sx, sy), (true, sx + 1, sy + 1, sx + 1, sy + 1), (true, sx + 2, sy + 1, sx + 1, sy + 1)];
            for (hor, cx, cy, bx, by) in cells {
                t[cy.rem_euclid(4) as usize][cx.rem_euclid(4) as usize] = (hor, bx.rem_euclid(4), by.rem_euclid(4));
            }
        }
    }
    t
}

/// Brick under the point (x, y) given in cell units: (horizontal, base cell, distance to the brick border in cells).
fn herring_brick(tab: &[[(bool, i32, i32); 4]; 4], x: f32, y: f32) -> (bool, i32, i32, f32) {
    let (ci, cj) = (x.floor() as i32, y.floor() as i32);
    let (hor, bx, by) = tab[cj.rem_euclid(4) as usize][ci.rem_euclid(4) as usize];
    let dx = (ci - bx).rem_euclid(4);
    let dy = (cj - by).rem_euclid(4);
    let (ax, ay) = (ci - dx, cj - dy);
    let (w, h) = if hor { (2.0, 1.0) } else { (1.0, 2.0) };
    let d = (x - ax as f32).min(ax as f32 + w - x).min(y - ay as f32).min(ay as f32 + h - y);
    (hor, ax, ay, d.max(0.0))
}

// ====================================================================== arc pavement
struct Arc {
    nx: i32,
    ny: i32,
    rings: i32,
    stones: f32,
}

/// Stone under (x, y) in cell units (cell = width of one fan): (ring, stone id, distance to the stone border in cells).
fn arc_stone(a: &Arc, x: f32, y: f32) -> (i32, i32, i32, i32, f32) {
    let r0 = 0.5f32;
    let jrow = (y * 2.0).floor() as i32;
    let mut best: Option<(f32, i32, i32, f32, f32)> = None;
    for j in (jrow - 1)..=(jrow + 2) {
        let off = 0.5 * (j.rem_euclid(2)) as f32;
        let i0 = (x - off).round() as i32;
        for i in (i0 - 1)..=(i0 + 1) {
            let (cx, cy) = (i as f32 + off, j as f32 * 0.5);
            let (dx, dy) = (x - cx, y - cy);
            let d = (dx * dx + dy * dy).sqrt();
            if d <= r0 && best.map(|b| cy > b.0).unwrap_or(true) {
                best = Some((cy, i, j, dx, dy));
            }
        }
    }
    let Some((_, i, j, dx, dy)) = best else { return (0, 0, 0, 0, 0.0) };
    let r = (dx * dx + dy * dy).sqrt();
    let k = a.rings as f32;
    let ring = ((r / r0 * k).floor() as i32).clamp(0, a.rings - 1);
    let (r_in, r_out) = (ring as f32 * r0 / k, (ring + 1) as f32 * r0 / k);
    let n = ((a.stones * 2.0 * (ring + 1) as f32 / k).round() as i32).max(3);
    let ang = (dy.atan2(dx) + PI) / TAU + 0.5 * (ring & 1) as f32 / n as f32;
    let f = fract(ang * n as f32);
    let arc_d = r * f.min(1.0 - f) * TAU / n as f32;
    let sector = ((ang * n as f32).floor() as i32).rem_euclid(n);
    let d = (r - r_in).min(r_out - r).min(arc_d);
    (i.rem_euclid(a.nx), j.rem_euclid(a.ny), ring, sector, d.max(0.0))
}

// ====================================================================== lattice gradient noise on a skewed torus
/// Gradient noise whose lattice axes are the integer vector (a, b) and its normal, with `sx` cells along that
/// direction and `sy` across, so it stays seamless at any direction. `(s, t)` are the lattice coordinates.
struct Lattice {
    g1: (i64, i64),
    g2: (i64, i64),
    det: f64,
    seed: u32,
}

impl Lattice {
    fn new(a: i32, b: i32, sx: i32, sy: i32, seed: u32) -> Lattice {
        let g1 = ((a * sx) as i64, (-b * sy) as i64);
        let g2 = ((b * sx) as i64, (a * sy) as i64);
        let det = (g1.0 * g2.1 - g1.1 * g2.0) as f64;
        Lattice { g1, g2, det: if det == 0.0 { 1.0 } else { det }, seed }
    }
    fn reduce(&self, i: i64, j: i64) -> (i32, i32) {
        let al = ((i * self.g2.1 - j * self.g2.0) as f64 / self.det).floor() as i64;
        let be = ((-i * self.g1.1 + j * self.g1.0) as f64 / self.det).floor() as i64;
        ((i - al * self.g1.0 - be * self.g2.0) as i32, (j - al * self.g1.1 - be * self.g2.1) as i32)
    }
    fn grad(&self, i: i64, j: i64) -> (f32, f32) {
        let (ri, rj) = self.reduce(i, j);
        let a = noise::rnd(ri, rj, self.seed) * TAU;
        (a.cos(), a.sin())
    }
    fn sample(&self, s: f32, t: f32) -> f32 {
        let (i0, j0) = (s.floor() as i64, t.floor() as i64);
        let (fx, fy) = (s - i0 as f32, t - j0 as f32);
        let fade = |x: f32| x * x * x * (x * (x * 6.0 - 15.0) + 10.0);
        let dot = |di: i64, dj: i64| {
            let g = self.grad(i0 + di, j0 + dj);
            g.0 * (fx - di as f32) + g.1 * (fy - dj as f32)
        };
        let (u, v) = (fade(fx), fade(fy));
        let top = lerp(dot(0, 0), dot(1, 0), u);
        let bot = lerp(dot(0, 1), dot(1, 1), u);
        lerp(top, bot, v)
    }
}

/// Integer direction closest to `turns`, as (a, b) with |a|, |b| <= 5.
fn int_direction(turns: f32) -> (i32, i32) {
    let th = turns * TAU;
    let mut best = (f32::MIN, 1, 0);
    for a in -5..=5i32 {
        for b in -5..=5i32 {
            if a == 0 && b == 0 {
                continue;
            }
            let c = (a as f32 * th.cos() + b as f32 * th.sin()) / ((a * a + b * b) as f32).sqrt();
            if c > best.0 + 1e-6 || ((c - best.0).abs() <= 1e-6 && a * a + b * b < best.1 * best.1 + best.2 * best.2) {
                best = (c, a, b);
            }
        }
    }
    (best.1, best.2)
}

// ====================================================================== 3D noise (tiles in x and y, free in z)
fn grad3(ix: i32, iy: i32, iz: i32, seed: u32) -> [f32; 3] {
    const G: [[f32; 3]; 12] = [[1., 1., 0.], [-1., 1., 0.], [1., -1., 0.], [-1., -1., 0.], [1., 0., 1.], [-1., 0., 1.], [1., 0., -1.], [-1., 0., -1.], [0., 1., 1.], [0., -1., 1.], [0., 1., -1.], [0., -1., -1.]];
    let h = noise::hash2(ix.wrapping_add(iz.wrapping_mul(7919)), iy.wrapping_add(iz.wrapping_mul(104_729)), seed.wrapping_add((iz as u32).wrapping_mul(0x9e37_79b1)));
    G[((h >> 8) % 12) as usize]
}

fn perlin3(x: f32, y: f32, z: f32, px: i32, py: i32, seed: u32) -> f32 {
    let (x0, y0, z0) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let (fx, fy, fz) = (x - x0 as f32, y - y0 as f32, z - z0 as f32);
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (u, v, w) = (fade(fx), fade(fy), fade(fz));
    let corner = |dx: i32, dy: i32, dz: i32| {
        let g = grad3((x0 + dx).rem_euclid(px), (y0 + dy).rem_euclid(py), z0 + dz, seed);
        g[0] * (fx - dx as f32) + g[1] * (fy - dy as f32) + g[2] * (fz - dz as f32)
    };
    let mut layer = [0.0f32; 2];
    for dz in 0..2 {
        let a = lerp(corner(0, 0, dz), corner(1, 0, dz), u);
        let b = lerp(corner(0, 1, dz), corner(1, 1, dz), u);
        layer[dz as usize] = lerp(a, b, v);
    }
    lerp(layer[0], layer[1], w)
}

/// Nearest and second nearest feature point distances in 3D (cells tile in x and y).
fn worley3(x: f32, y: f32, z: f32, px: i32, py: i32, seed: u32, jitter: f32) -> (f32, f32, f32) {
    let (ix, iy, iz) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let (mut f1, mut f2, mut id) = (f32::MAX, f32::MAX, 0.0);
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (cx, cy, cz) = (ix + dx, iy + dy, iz + dz);
                let (wx, wy) = (cx.rem_euclid(px), cy.rem_euclid(py));
                let s = seed.wrapping_add((cz as u32).wrapping_mul(0x9e37_79b1));
                let j = |k: u32| (rh(wx, wy, s.wrapping_add(k)) - 0.5) * jitter;
                let (fx, fy, fz) = (cx as f32 + 0.5 + j(1), cy as f32 + 0.5 + j(2), cz as f32 + 0.5 + j(3));
                let d = ((fx - x).powi(2) + (fy - y).powi(2) + (fz - z).powi(2)).sqrt();
                if d < f1 {
                    f2 = f1;
                    f1 = d;
                    id = rh(wx, wy, s.wrapping_add(9));
                } else if d < f2 {
                    f2 = d;
                }
            }
        }
    }
    (f1, f2, id)
}

fn sdf3(shape: u32, p: [f32; 3], size: f32, round: f32) -> f32 {
    let (x, y, z) = (p[0], p[1], p[2]);
    match shape {
        0 => (x * x + y * y + z * z).sqrt() - size,
        1 => {
            let q = [x.abs() - size + round, y.abs() - size + round, z.abs() - size + round];
            let o = (q[0].max(0.0).powi(2) + q[1].max(0.0).powi(2) + q[2].max(0.0).powi(2)).sqrt();
            o + q[0].max(q[1]).max(q[2]).min(0.0) - round
        }
        2 => {
            let (dr, dz) = ((x * x + y * y).sqrt() - size, z.abs() - size);
            dr.max(dz).min(0.0) + dr.max(0.0).hypot(dz.max(0.0))
        }
        3 => {
            let q = ((x * x + y * y).sqrt() - size, z);
            (q.0 * q.0 + q.1 * q.1).sqrt() - size * 0.4
        }
        _ => {
            // cone with its tip up
            let r = (x * x + y * y).sqrt();
            let k = ((z + size) / (2.0 * size)).clamp(0.0, 1.0);
            (r - size * (1.0 - k)).max(z.abs() - size)
        }
    }
}

// ====================================================================== splatting helper (fur)
fn splat_max(img: &mut [f32], w: usize, h: usize, x: f32, y: f32, r: f32, val: f32) {
    let rr = r.max(0.5) + 0.7;
    let (x0, x1) = ((x - rr).floor() as i64, (x + rr).ceil() as i64);
    let (y0, y1) = ((y - rr).floor() as i64, (y + rr).ceil() as i64);
    for yy in y0..=y1 {
        for xx in x0..=x1 {
            let d = (((xx as f32 + 0.5) - x).powi(2) + ((yy as f32 + 0.5) - y).powi(2)).sqrt();
            let a = (1.0 - (d - r.max(0.5) + 0.7) / 1.4).clamp(0.0, 1.0);
            if a > 0.0 {
                let k = (yy.rem_euclid(h as i64) as usize) * w + xx.rem_euclid(w as i64) as usize;
                img[k] = img[k].max(val * a);
            }
        }
    }
}

// ====================================================================== the node compute function
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
        // ------------------------------------------------ pavements
        "herringbone" => {
            let (nx, ny) = cells_for(f("scale", 8.0), s, hs, 4);
            let tab = herring_table();
            let cell_px = (s as f32 / nx as f32).min(hs as f32 / ny as f32);
            let gap = (f("gap", 3.0) * rs).max(0.0) / cell_px;
            let bevel = (f("bevel", 6.0) * rs).max(0.5) / cell_px;
            let var = f("variation", 0.3).clamp(0.0, 1.0);
            let mode = f("mode", 0.0).round() as i32;
            fin(Tex::gray_fn(s, hs, |u, v| {
                let (x, y) = (u * nx as f32, v * ny as f32);
                let (hor, bx, by, d) = herring_brick(&tab, x, y);
                let (wx, wy) = (bx.rem_euclid(nx), by.rem_euclid(ny));
                match mode {
                    1 => rh(wx, wy, seed),
                    2 => if hor { 1.0 } else { 0.0 },
                    _ => smooth(gap * 0.5, gap * 0.5 + bevel, d) * (1.0 - var * rh(wx, wy, seed)),
                }
            }))
        }
        "arc_pavement" => {
            let nx = f("scale", 6.0).round().clamp(2.0, 64.0) as i32;
            let ny = (((nx as f32 * hs as f32 / s as f32) * 2.0 / 2.0).round().max(1.0) as i32 * 2).clamp(2, 512);
            let a = Arc { nx, ny, rings: f("rings", 4.0).round().clamp(1.0, 12.0) as i32, stones: f("stones", 9.0).clamp(2.0, 64.0) };
            // rows are half a fan apart, so the texture height in fans is ny / 2 and the vertical scale differs from the width
            let cell_px = s as f32 / nx as f32;
            let gap = (f("gap", 2.0) * rs).max(0.0) / cell_px;
            // a stone row is only 0.5 / rings of a fan thick: keep the bevel inside the narrowest stone
            let half_row = 0.25 / a.rings as f32;
            let bevel = ((f("bevel", 4.0) * rs).max(0.5) / cell_px).min((half_row - gap * 0.5) * 0.9).max(0.002);
            let var = f("variation", 0.3).clamp(0.0, 1.0);
            let mode = f("mode", 0.0).round() as i32;
            fin(Tex::gray_fn(s, hs, |u, v| {
                let (x, y) = (u * nx as f32, v * ny as f32 * 0.5);
                let (i, j, ring, sector, d) = arc_stone(&a, x, y);
                let key = rh(i.wrapping_mul(31).wrapping_add(ring), j.wrapping_mul(17).wrapping_add(sector), seed);
                match mode {
                    1 => key,
                    _ => smooth(gap * 0.5, gap * 0.5 + bevel, d) * (1.0 - var * key),
                }
            }))
        }
        "tile_random2" => {
            let (nx, ny) = (f("amount_x", 10.0).round().clamp(2.0, 64.0) as i32, f("amount_y", 10.0).round().clamp(2.0, 64.0) as i32);
            let rsz = f("random_size", 1.0).clamp(0.0, 1.0);
            let mut t = crate::tile2::Tiles::new(nx, ny, rsz * f("random_size_x", 1.0).clamp(0.0, 1.0), rsz * f("random_size_y", 1.0).clamp(0.0, 1.0), seed, 1.0 / s as f32);
            let avg = t.average();
            t.gap = f("interstice", 0.0).clamp(0.0, 1.0) * 0.25;
            t.position = f("random_position", 0.0).clamp(0.0, 1.0);
            t.radius = f("round_corners", 0.0).clamp(0.0, 1.0);
            t.scale = f("scale", 1.0).clamp(0.0, 1.0);
            t.scale_random = f("scale_random", 0.0).clamp(0.0, 1.0);
            t.rotation = f("rotation", 0.0).clamp(0.0, 0.25);
            t.rotation_random = f("rotation_random", 0.0).clamp(0.0, 1.0);
            t.corners = f("corners_random", 0.0).clamp(0.0, 1.0);
            let sl = f("slant", 0.0).clamp(0.0, 1.0);
            t.slant = [sl * f("slant_x", 1.0).clamp(0.0, 1.0), sl * f("slant_y", 1.0).clamp(0.0, 1.0)];
            // the bevel is measured against the average cell, so every tile gets the same width
            let bw = f("bevel", 0.1).clamp(0.0, 1.0) * 0.5 * avg;
            t.bevel = if f("bevel_per_axis", 0.0) > 0.5 { [bw * f("bevel_x", 1.0).clamp(0.0, 1.0), bw * f("bevel_y", 1.0).clamp(0.0, 1.0)] } else { [bw, bw] };
            t.curve = f("bevel_curve", 0.0).clamp(0.0, 1.0);
            let (ms, me) = (f("mask_random_start", 0.0), f("mask_random_end", 0.0));
            if me > ms {
                t.mask = Some((ms, me, f("mask_random_invert", 0.0) > 0.5));
            }
            fin(Tex::gray_fn(s, hs, |u, v| t.at(u, v).clamp(0.0, 1.0)))
        }
        // ------------------------------------------------ noises
        "anisotropic_noise" if f("style", 0.0) > 0.5 => {
            // value grid: few random values along each row, many rows, smoothly joined along the row
            let nx = f("x_amount", 4.0).round().clamp(1.0, 512.0) as i32;
            let ny0 = f("y_amount", 256.0).round().clamp(1.0, 512.0) as i32;
            let turn = f("rotate", 0.0) > 0.5;
            // "by resolution": the row count is given for a 256 px image and grows with the resolution (one row per
            // pixel row at the default)
            let along = if turn { s } else { hs };
            let ny = if f("rows_by_resolution", 0.0) > 0.5 { ((ny0 as f32 * along as f32 / 256.0).round() as i32).clamp(1, 8192) } else { ny0 };
            // smoothness: how much of each step along the row is a transition (0 = hard steps, 1 = all of it);
            // the interpolation blends a straight ramp (0) with a smooth S curve (1)
            let sm = f("smoothness", 1.0).clamp(0.0, 1.0);
            let si = f("smoothness_interpolation", 0.5).clamp(0.0, 1.0);
            fin(Tex::gray_fn(s, hs, |u, v| {
                let (u, v) = if turn { (v, u) } else { (u, v) };
                let j = ((v * ny as f32).floor() as i32).rem_euclid(ny);
                // every row starts at its own random place so the joins do not line up
                let x = (u + crate::noise_styles::unit(j, 7, seed)) * nx as f32;
                let i = x.floor();
                let t = x - i;
                let i = i as i32;
                let a = crate::noise_styles::unit(i.rem_euclid(nx), j, seed);
                let b = crate::noise_styles::unit((i + 1).rem_euclid(nx), j, seed);
                let t = if sm < 1e-4 { if t < 0.5 { 0.0 } else { 1.0 } } else { ((t - 0.5) / sm + 0.5).clamp(0.0, 1.0) };
                let t = t + (t * t * (3.0 - 2.0 * t) - t) * si;
                a + (b - a) * t
            }))
        }
        "anisotropic_noise" => {
            let (a, b) = int_direction(f("angle", 0.0));
            let sy0 = f("scale", 8.0).round().clamp(1.0, 128.0) as i32;
            let sx0 = ((sy0 as f32 / f("stretch", 6.0).clamp(1.0, 64.0)).round() as i32).max(1);
            let oct = f("octaves", 3.0).round().clamp(1.0, 8.0) as i32;
            let gain = f("gain", 0.5).clamp(0.1, 1.0);
            let lats: Vec<(Lattice, i32, i32, f32)> = (0..oct).map(|o| {
                let (sx, sy) = (sx0 << o, sy0 << o);
                (Lattice::new(a, b, sx, sy, seed.wrapping_add(o as u32 * 101)), sx, sy, gain.powi(o))
            }).collect();
            let norm: f32 = lats.iter().map(|l| l.3).sum();
            fin(Tex::gray_fn(s, hs, |u, v| {
                let mut acc = 0.0;
                for (lat, sx, sy, amp) in &lats {
                    let (su, sv) = (a as f32 * u + b as f32 * v, -(b as f32) * u + a as f32 * v);
                    acc += lat.sample(su * *sx as f32, sv * *sy as f32) * amp;
                }
                (0.5 + 0.9 * acc / norm).clamp(0.0, 1.0)
            }))
        }
        "fur_noise" => {
            let count = f("count", 2500.0).round().clamp(1.0, 60000.0) as usize;
            let len = f("length", 70.0).clamp(2.0, 1024.0) * rs;
            let thick = (f("thickness", 2.2) * rs).clamp(0.4, 128.0);
            let base = f("angle", 0.25) * TAU;
            let var = f("angle_variation", 0.25).clamp(0.0, 1.0) * PI;
            let curl = f("curl", 0.8).clamp(-8.0, 8.0);
            let taper = f("taper", 0.8).clamp(0.0, 1.0);
            let root = f("root_shade", 0.25).clamp(0.0, 1.0);
            let bvar = f("brightness_variation", 0.35).clamp(0.0, 1.0);
            let step = (thick * 0.6).max(1.0);
            let nstep = ((len / step).ceil() as usize).clamp(2, 400);
            let mut img = vec![0.0f32; s * hs];
            let fl = noise::Fbm::new(4, ((4.0 * hs as f32 / s as f32).round() as i32).max(1), 2, 0.5, seed.wrapping_add(5), 0);
            // optional flow: hairs follow a smooth field (noise) or sway sideways in waves across the image
            let flow = f("flow", 0.0).clamp(0.0, 1.0);
            let waves_mode = f("flow_mode", 0.0) > 0.5;
            let waves = f("waves", 2.0).round().clamp(0.0, 16.0);
            let (fpx, fpy) = periods(f("flow_scale", 2.0), s, hs);
            let ff = Fbm::new(fpx, fpy, 2, 0.5, seed.wrapping_add(9), 0);
            let field = |x: f32, y: f32| -> f32 {
                if flow <= 0.0 {
                    0.0
                } else if waves_mode {
                    (TAU * waves * x / s as f32).sin() * std::f32::consts::FRAC_PI_2 * flow
                } else {
                    (ff.sample(x / s as f32, y / hs as f32) - 0.5) * TAU * flow
                }
            };
            for k in 0..count {
                let kk = k as i32;
                let (mut x, mut y) = (rh(kk, 1, seed) * s as f32, rh(kk, 2, seed) * hs as f32);
                let ang0 = base + (rh(kk, 3, seed) - 0.5) * 2.0 * var;
                let mut drift = 0.0f32;
                let bright = 1.0 - bvar * rh(kk, 4, seed);
                let bend = (rh(kk, 5, seed) - 0.5) * 2.0 * curl;
                for i in 0..nstep {
                    let t = i as f32 / (nstep - 1) as f32;
                    let r = thick * 0.5 * (1.0 - taper * t);
                    // the shade rises from the root to the tip; the field makes neighbouring hairs lean together
                    let val = bright * lerp(root, 1.0, t);
                    splat_max(&mut img, s, hs, x, y, r, val);
                    // without a flow, a faint field makes neighbouring hairs lean together
                    drift += bend * step / len.max(1.0) * 2.0 + if flow > 0.0 { 0.0 } else { (fl.sample(x / s as f32, y / hs as f32) - 0.5) * 0.05 };
                    let ang = ang0 + drift + field(x, y);
                    x += ang.cos() * step;
                    y += ang.sin() * step;
                }
            }
            fin(Tex { w: s, h: hs, px: img.into_iter().map(grey).collect() })
        }
        "moisture_noise" => {
            let (px, py) = periods(f("scale", 4.0), s, hs);
            let cover = f("coverage", 0.5).clamp(0.0, 1.0);
            let soft = f("softness", 0.15).clamp(0.01, 0.5);
            let drip = f("drips", 0.7).clamp(0.0, 1.0);
            let dlen = (f("drip_length", 90.0) * rs).clamp(1.0, 1024.0);
            let fb = Fbm::new(px, py, 4, 0.5, seed, 0);
            let patches = Tex::gray_fn(s, hs, |u, v| smooth(1.0 - cover - soft, 1.0 - cover + soft, fb.sample(u, v)));
            let (sx, sy) = periods(f("scale", 4.0) * 6.0, s, hs);
            let streak = Fbm::new(sx, (sy / 6).max(1), 3, 0.5, seed.wrapping_add(31), 0);
            let nd = 14usize;
            let step = dlen / nd as f32;
            fin(Tex::gray_fn(s, hs, |u, v| {
                let (x, y) = ((u * s as f32) as i64, (v * hs as f32) as i64);
                let base = patches.at(x, y)[0];
                // water runs down from the wet patches along the streak pattern
                let st = smooth(0.35, 0.75, streak.sample(u, v));
                let mut run = 0.0f32;
                for k in 1..=nd {
                    let fall = 1.0 - k as f32 / (nd + 1) as f32;
                    run = run.max(patches.at(x, y - (step * k as f32) as i64)[0] * fall);
                }
                base.max(run * st * drip)
            }))
        }
        "microscope_noise" if f("style", 0.0) > 0.5 => {
            // cells floating at different depths: rings that are sharp and bright in focus, soft and dim out of focus,
            // some of them dividing in two
            let g = f("density", 14.0).round().clamp(2.0, 64.0) as i32;
            let per = 3;
            let size = f("cell_size", 0.014).clamp(0.002, 0.2);
            let focus = f("focus", 0.07).clamp(0.0, 1.0);
            let warp = f("warp", 0.5).clamp(0.0, 4.0) * 0.015;
            let bright = f("brightness", 0.45).clamp(0.0, 8.0);
            let px = 1.0 / s as f32;
            let reach = ((size * 1.4 * 2.2) * g as f32).ceil() as i32;
            fin(Tex::gray_fn(s, hs, |u, v| {
                let (wu, wv) = if warp > 0.0 {
                    (u + warp * crate::noise_styles::gradient(u, v, 6, 6, seed.wrapping_add(3)), v + warp * crate::noise_styles::gradient(u, v, 6, 6, seed.wrapping_add(4)))
                } else {
                    (u, v)
                };
                let (gx, gy) = ((wu * g as f32).floor() as i32, (wv * g as f32).floor() as i32);
                let mut acc = 0.0f32;
                for dy in -reach..=reach {
                    for dx in -reach..=reach {
                        let (cx, cy) = (gx + dx, gy + dy);
                        let (hx, hy) = (cx.rem_euclid(g), cy.rem_euclid(g));
                        for k in 0..per {
                            let r = |n: i32| crate::noise_styles::unit(hx * per + k, hy * 97 + n, seed);
                            let (px0, py0) = ((cx as f32 + r(1)) / g as f32, (cy as f32 + r(2)) / g as f32);
                            let rad = size * (0.6 + 0.8 * r(3));
                            let stretch = 1.0 + 0.5 * r(4);
                            let (sn, cs) = (r(5) * std::f32::consts::TAU).sin_cos();
                            // most cells are out of focus; `focus` is the share that is sharp
                            let depth = if r(6) < focus { 0.0 } else { r(7) };
                            let blur = (0.08 + 0.9 * depth) * rad + px;
                            let amp = bright * (0.12 + 0.5 * (1.0 - depth).powi(3));
                            let dividing = r(8) < 0.25;
                            let (ex, ey) = (wu - px0, wv - py0);
                            let mut best = 0.0f32;
                            for part in 0..if dividing { 2 } else { 1 } {
                                let (ox, oy, rr) = if dividing {
                                    let o = if part == 0 { 0.55 } else { -0.55 } * rad;
                                    (o * cs, o * sn, rad * 0.75)
                                } else {
                                    (0.0, 0.0, rad)
                                };
                                let (qx, qy) = (ex - ox, ey - oy);
                                let a = (qx * cs + qy * sn) / stretch;
                                let b = -qx * sn + qy * cs;
                                let d = (a * a + b * b).sqrt();
                                let ring = (-((d - rr) / blur).powi(2)).exp();
                                let fill = 0.6 * (1.0 - smooth(rr - blur, rr + blur, d)) * depth.sqrt();
                                best = best.max(ring + fill);
                            }
                            acc += amp * best;
                        }
                    }
                }
                acc.min(1.0)
            }))
        }
        "microscope_noise" => {
            let (px, py) = periods(f("scale", 10.0), s, hs);
            let jitter = f("jitter", 0.8).clamp(0.0, 1.0);
            let wall = f("membrane", 0.06).clamp(0.005, 0.5);
            let nuc = f("nucleus", 0.28).clamp(0.02, 0.8);
            let grain = f("grain", 0.25).clamp(0.0, 1.0);
            let mode = f("mode", 0.0).round() as i32;
            let gfb = Fbm::new(px * 6, py * 6, 3, 0.55, seed.wrapping_add(7), 0);
            fin(Tex::gray_fn(s, hs, |u, v| {
                let r = noise::voronoi_ext(u, v, px, py, seed, jitter);
                let edge = (r.f2 - r.f1) * 1.4;
                let membrane = 1.0 - smooth(wall * 0.5, wall, edge);
                let size = nuc * (0.7 + 0.6 * r.id1);
                let nucleus = 1.0 - smooth(size * 0.65, size, r.f1);
                let cyto = 0.38 + (gfb.sample(u, v) - 0.5) * grain;
                match mode {
                    1 => membrane,
                    2 => nucleus,
                    3 => r.id1,
                    _ => (lerp(lerp(cyto, 0.12, nucleus), 0.85, membrane)).clamp(0.0, 1.0),
                }
            }))
        }
        "worley_3d" => {
            let (px, py) = periods(f("scale", 6.0), s, hs);
            let z = f("z", 0.0) * f("scale", 6.0);
            let jit = f("jitter", 1.0).clamp(0.0, 1.0);
            let mode = f("mode", 0.0).round() as i32;
            fin(Tex::gray_fn(s, hs, |u, v| {
                let (f1, f2, id) = worley3(u * px as f32, v * py as f32, z, px, py, seed, jit);
                match mode {
                    1 => (f2 * 0.7).clamp(0.0, 1.0),
                    2 => ((f2 - f1) * 1.4).clamp(0.0, 1.0),
                    3 => id,
                    _ => (f1 * 1.2).clamp(0.0, 1.0),
                }
            }))
        }
        "fbm_3d" => {
            let (px, py) = periods(f("scale", 4.0), s, hs);
            let oct = f("octaves", 5.0).round().clamp(1.0, 10.0) as i32;
            let gain = f("gain", 0.5).clamp(0.1, 0.95);
            let z = f("z", 0.0) * f("scale", 4.0);
            let mode = f("mode", 0.0).round() as i32;
            fin(Tex::gray_fn(s, hs, |u, v| {
                let (mut acc, mut amp, mut tot) = (0.0, 1.0, 0.0);
                for o in 0..oct {
                    let m = (1 << o) as f32;
                    let nv = perlin3(u * (px << o) as f32, v * (py << o) as f32, z * m, px << o, py << o, seed.wrapping_add(o as u32 * 57));
                    let t = match mode {
                        1 => 1.0 - (nv * 1.6).abs().min(1.0),
                        2 => (nv * 1.6).abs().min(1.0),
                        _ => 0.5 + 0.8 * nv,
                    };
                    acc += t * amp;
                    tot += amp;
                    amp *= gain;
                }
                (acc / tot).clamp(0.0, 1.0)
            }))
        }
        "volume_mask" | "gradient_3d" => {
            let shape = f("shape", 0.0).round() as u32;
            let size = f("size", 0.35).clamp(0.01, 0.5);
            let rep = f("repeat", 1.0).round().clamp(1.0, 64.0) as i32;
            let repy = ((rep as f32 * hs as f32 / s as f32).round() as i32).max(1);
            let z = f("z", 0.5) - 0.5;
            let round = f("roundness", 0.0).clamp(0.0, 0.3);
            let range = f("range", 0.5).clamp(0.01, 2.0);
            let soft = f("softness", 0.02).clamp(0.0, 0.5);
            let mask = kind == "volume_mask";
            fin(Tex::gray_fn(s, hs, |u, v| {
                let p = [fract(u * rep as f32) - 0.5, fract(v * repy as f32) - 0.5, z];
                let d = sdf3(shape, p, size, round);
                if mask {
                    smooth(-soft - 1e-4, soft + 1e-4, -d)
                } else {
                    (1.0 - d / range).clamp(0.0, 1.0)
                }
            }))
        }
        // ------------------------------------------------ shapes from shapes
        "shape_extrude" => {
            let src = need("in")?;
            let len = (f("length", 60.0) * rs).clamp(1.0, 2048.0);
            let th = f("angle", 0.125) * TAU;
            let top = f("top", 1.0).clamp(0.0, 1.0);
            let falloff = f("falloff", 1.0).clamp(0.1, 8.0);
            let steps = (len.ceil() as usize).clamp(1, 96);
            let stepl = len / steps as f32;
            let (dx, dy) = (th.cos() * stepl / s as f32, th.sin() * stepl / hs as f32);
            let m0: Vec<f32> = src.px.iter().map(|p| lum(*p)).collect();
            let drop = 1.0 / (steps as f32 + 1.0);
            let on_gpu = if (src.w, src.h) == (s, hs) { crate::gpu::extrude(src, steps as u32, dx, dy, drop) } else { None };
            let (mut cur, steps) = match on_gpu {
                Some(g) => (g, 0),
                None => (Tex { w: s, h: hs, px: m0.iter().map(|v| grey(*v)).collect() }, steps),
            };
            for _ in 0..steps {
                let prev = cur.clone();
                let mut nx = Tex { w: s, h: hs, px: vec![[0.0, 0.0, 0.0, 1.0]; s * hs] };
                let src_ref = &m0;
                par_rows(&mut nx.px, s, |start, chunk| {
                    for (j, dst) in chunk.iter_mut().enumerate() {
                        let (x, y) = ((start + j) % s, (start + j) / s);
                        let (u, v) = ((x as f32 + 0.5) / s as f32, (y as f32 + 0.5) / hs as f32);
                        let back = prev.sample(u - dx, v - dy)[0] - drop;
                        *dst = grey(src_ref[start + j].max(back.max(0.0)));
                    }
                });
                cur = nx;
            }
            // top face stays flat at `top`, the walls fall off from it along the extrusion
            let h: Vec<f32> = cur.px.iter().zip(&m0).map(|(c, m)| if *m > 0.5 { top } else { c[0].powf(falloff) * top }).collect();
            let walls: Vec<f32> = h.iter().zip(&m0).map(|(a, m)| if *m > 0.5 { 0.0 } else { *a }).collect();
            let pick = if out_idx == 1 { walls } else { h };
            fin(Tex { w: s, h: hs, px: pick.into_iter().map(grey).collect() })
        }
        "shape_mapper" => {
            let regions = need("in")?;
            let pat = opt("pattern");
            let (labels, stats) = ops::flood_regions(regions);
            let rot_rand = f("rotation_random", 0.0).clamp(0.0, 1.0);
            let flip = f("flip_random", 0.0).clamp(0.0, 1.0);
            let margin = f("scale", 1.0).clamp(0.1, 4.0);
            let keep = f("keep_aspect", 0.0) > 0.5;
            let mut out = Tex { w: s, h: hs, px: vec![[0.0, 0.0, 0.0, 1.0]; s * hs] };
            let (rw, rh_) = (regions.w, regions.h);
            let base_pat = |u: f32, v: f32| -> [f32; 4] {
                match pat {
                    Some(p) => p.sample(u, v),
                    None => {
                        let (x, y) = (u - 0.5, v - 0.5);
                        grey((1.0 - (x * x + y * y).sqrt() * 2.0).clamp(0.0, 1.0))
                    }
                }
            };
            par_rows(&mut out.px, s, |start, chunk| {
                for (j, dst) in chunk.iter_mut().enumerate() {
                    let (x, y) = ((start + j) % s, (start + j) / s);
                    let (u, v) = ((x as f32 + 0.5) / s as f32, (y as f32 + 0.5) / hs as f32);
                    let (lx, ly) = (((u * rw as f32) as usize).min(rw - 1), ((v * rh_ as f32) as usize).min(rh_ - 1));
                    let l = labels[ly * rw + lx] as usize;
                    if l == 0 || l >= stats.len() {
                        continue;
                    }
                    let (c, size, _) = stats[l];
                    let wrap = |d: f32| d - d.round();
                    let (mut px_, py_) = (wrap(u - c[0]), wrap(v - c[1]));
                    let (mut bw, mut bh) = (size[0].max(1e-4), size[1].max(1e-4));
                    if keep {
                        let m = bw.max(bh);
                        bw = m;
                        bh = m;
                    }
                    let r = rh(l as i32, 1, seed);
                    if r < flip {
                        px_ = -px_;
                    }
                    let ang = if rot_rand > 0.0 { (rh(l as i32, 2, seed) * 4.0).floor() * 0.25 * TAU * rot_rand } else { 0.0 };
                    let (sa, ca) = ang.sin_cos();
                    let (rx, ry) = (px_ * ca - py_ * sa, px_ * sa + py_ * ca);
                    let (pu, pv) = (rx / (bw * margin) + 0.5, ry / (bh * margin) + 0.5);
                    if (0.0..=1.0).contains(&pu) && (0.0..=1.0).contains(&pv) {
                        *dst = base_pat(pu, pv);
                    }
                }
            });
            fin(out)
        }
        _ => return crate::nodes8::compute(kind, id, n, ins, s, hs, seed),
    };
    Ok(Some(out))
}
