// SPDX-License-Identifier: MIT OR Apache-2.0
//! Random Tiles 2: a patchwork of rectangles of random widths (and, if asked, heights and stacked halves) whose
//! corners can be pulled in, slanted, rotated and rounded, with an even gap between the tiles and a bevel whose width
//! does not depend on the size of the tile. Everything is our own construction; the layout repeats with the image.
use crate::noise_styles::unit;
use std::f32::consts::TAU;

/// The settings of one Random Tiles 2 node.
pub struct Tiles {
    pub nx: i32,
    pub ny: i32,
    /// random width of the tiles of a row / random height of the rows (0 = all alike), each 0..1
    pub size_x: f32,
    pub size_y: f32,
    /// per-side gap, as a share of the average cell
    pub gap: f32,
    pub position: f32,
    pub radius: f32,
    pub scale: f32,
    pub scale_random: f32,
    pub rotation: f32,
    pub rotation_random: f32,
    pub corners: f32,
    pub slant: [f32; 2],
    /// bevel width in image units for edges across x and across y, and how round the profile is
    pub bevel: [f32; 2],
    pub curve: f32,
    pub mask: Option<(f32, f32, bool)>,
    pub seed: u32,
    /// one pixel in image units
    pub px: f32,
    rows: Vec<f32>,
    cols: Vec<Vec<f32>>,
    avg: f32,
}

#[derive(Clone, Copy)]
struct Rect {
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
    id: (i32, i32, i32),
}

/// Boundaries of `n` runs of random length summing to 1 (`k` is the chance's key, `amount` 0..1).
fn bounds(n: i32, amount: f32, key: i32, seed: u32) -> Vec<f32> {
    let w: Vec<f32> = (0..n).map(|i| 1.0 + 0.6 * amount * (2.0 * unit(i, key, seed) - 1.0)).collect();
    let sum: f32 = w.iter().sum();
    let mut acc = 0.0;
    let mut out = vec![0.0];
    for x in &w {
        acc += x / sum;
        out.push(acc);
    }
    if let Some(l) = out.last_mut() {
        *l = 1.0;
    }
    out
}

impl Tiles {
    #[allow(clippy::too_many_arguments)]
    pub fn new(nx: i32, ny: i32, size_x: f32, size_y: f32, seed: u32, px: f32) -> Tiles {
        let rows = bounds(ny, size_y, 9001, seed.wrapping_add(7));
        let cols = (0..ny).map(|j| bounds(nx, size_x, j, seed.wrapping_add(11))).collect();
        Tiles {
            nx,
            ny,
            size_x,
            size_y,
            gap: 0.0,
            position: 0.0,
            radius: 0.0,
            scale: 1.0,
            scale_random: 0.0,
            rotation: 0.0,
            rotation_random: 0.0,
            corners: 0.0,
            slant: [0.0, 0.0],
            bevel: [0.02, 0.02],
            curve: 0.0,
            mask: None,
            seed,
            px,
            rows,
            cols,
            avg: 1.0 / ((nx * ny) as f32).sqrt(),
        }
    }

    /// Average cell size in image units: gaps and bevels are measured against it so every tile gets the same.
    pub fn average(&self) -> f32 {
        self.avg
    }

    fn cell_at(&self, x: f32, y: f32) -> Rect {
        let (x, y) = (x.rem_euclid(1.0), y.rem_euclid(1.0));
        let j = self.rows.partition_point(|b| *b <= y).saturating_sub(1).min(self.ny as usize - 1);
        let c = &self.cols[j];
        let i = c.partition_point(|b| *b <= x).saturating_sub(1).min(self.nx as usize - 1);
        let (y0, y1) = (self.rows[j], self.rows[j + 1]);
        let mut r = Rect { x0: c[i], x1: c[i + 1], y0, y1, id: (i as i32, j as i32, 0) };
        // a tile is sometimes two stacked halves, when the rows are allowed to differ
        if self.size_y > 0.0 && unit(i as i32, j as i32, self.seed.wrapping_add(21)) < 0.6 * self.size_y {
            let cut = y0 + (y1 - y0) * (0.35 + 0.3 * unit(i as i32, j as i32, self.seed.wrapping_add(22)));
            if y < cut {
                r.y1 = cut;
            } else {
                r.y0 = cut;
                r.id.2 = 1;
            }
        }
        r
    }

    /// Height of the tile `r` at (x, y) (positions already moved next to the tile: no wrapping inside).
    fn tile(&self, r: &Rect, x: f32, y: f32) -> f32 {
        let (i, j, k) = r.id;
        let key = |n: u32| unit(i.wrapping_mul(31).wrapping_add(k), j, self.seed.wrapping_add(n));
        if let Some((a, b, invert)) = self.mask {
            // tiles whose random value falls inside the range are kept (outside it, when inverted)
            let m = key(90);
            if (m >= a && m <= b) == invert {
                return 0.0;
            }
        }
        let (w, h) = (r.x1 - r.x0, r.y1 - r.y0);
        let (cx, cy) = ((r.x0 + r.x1) * 0.5, (r.y0 + r.y1) * 0.5);
        let sc = (self.scale * (1.0 - self.scale_random * key(31))).clamp(0.05, 1.5);
        let (hw, hh) = (w * 0.5 * sc, h * 0.5 * sc);
        let (sx, sy) = (cx + self.position * (2.0 * key(32) - 1.0) * w * 0.25, cy + self.position * (2.0 * key(33) - 1.0) * h * 0.25);
        // the four corners, counter-clockwise from the lower left
        let mut p = [[sx - hw, sy - hh], [sx + hw, sy - hh], [sx + hw, sy + hh], [sx - hw, sy + hh]];
        // slant: the upper edge slides along x against the lower one, the right edge along y against the left
        let (a, b) = (self.slant[0] * (2.0 * key(34) - 1.0) * hh * 0.5, self.slant[1] * (2.0 * key(35) - 1.0) * hw * 0.5);
        p[0][0] -= a;
        p[1][0] -= a;
        p[2][0] += a;
        p[3][0] += a;
        p[0][1] -= b;
        p[3][1] -= b;
        p[1][1] += b;
        p[2][1] += b;
        // corners: each one pulled towards the middle and sideways along its edge
        if self.corners > 0.0 {
            for (n, c) in p.iter_mut().enumerate() {
                let t = self.corners * 0.55 * key(40 + n as u32);
                let s = self.corners * (2.0 * key(50 + n as u32) - 1.0) * 0.3;
                c[0] += (sx - c[0]) * t + s * (if n == 0 || n == 3 { 1.0 } else { -1.0 }) * hw * 0.5;
                c[1] += (sy - c[1]) * t + s * (if n < 2 { 1.0 } else { -1.0 }) * hh * 0.5;
            }
        }
        let rot = (self.rotation + self.rotation_random * (2.0 * key(36) - 1.0) * 0.25) * TAU;
        if rot.abs() > 1e-6 {
            let (sn, cs) = rot.sin_cos();
            for c in p.iter_mut() {
                let (dx, dy) = (c[0] - sx, c[1] - sy);
                *c = [sx + dx * cs - dy * sn, sy + dx * sn + dy * cs];
            }
        }
        // distance inside each edge, the nearest two decide corners
        let mut d = [0.0f32; 4];
        let mut horizontal = [false; 4];
        for n in 0..4 {
            let (a, b) = (p[n], p[(n + 1) % 4]);
            let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
            let l = (ex * ex + ey * ey).sqrt().max(1e-9);
            d[n] = (ex * (y - a[1]) - ey * (x - a[0])) / l;
            horizontal[n] = ex.abs() >= ey.abs();
        }
        let gap = self.gap * self.avg;
        let mut order = [0usize, 1, 2, 3];
        order.sort_by(|a, b| d[*a].partial_cmp(&d[*b]).unwrap_or(std::cmp::Ordering::Equal));
        let (n1, n2) = (order[0], order[1]);
        let mut dist = d[n1];
        let rad = (self.radius * 0.5 * (hw.min(hh) * 2.0)).max(0.0);
        if rad > 1e-6 && d[n1] < rad && d[n2] < rad && n1 % 2 != n2 % 2 {
            let (u, v) = (rad - d[n1], rad - d[n2]);
            dist = rad - (u * u + v * v).sqrt();
        }
        let dist = dist - gap;
        let bw = if horizontal[n1] { self.bevel[1] } else { self.bevel[0] }.max(self.px);
        let t = (dist / bw).clamp(0.0, 1.0);
        let round = (1.0 - (1.0 - t) * (1.0 - t)).max(0.0).sqrt();
        let h = t + (round - t) * self.curve.clamp(0.0, 1.0);
        h * (dist / self.px + 0.5).clamp(0.0, 1.0)
    }

    /// The height at (u, v).
    pub fn at(&self, u: f32, v: f32) -> f32 {
        // the cell holding the point and the ones just around it (corners and slants reach over the edge)
        let m = self.avg * 0.6;
        let mut best = 0.0f32;
        let mut seen: [((i32, i32, i32), i32, i32); 9] = [((-1, -1, -1), 0, 0); 9];
        let mut n = 0;
        for (ox, oy) in [(0.0, 0.0), (m, 0.0), (-m, 0.0), (0.0, m), (0.0, -m), (m, m), (-m, m), (m, -m), (-m, -m)] {
            let (wx, wy) = (u + ox, v + oy);
            let r = self.cell_at(wx, wy);
            // the point in the frame of the cell that was found (the layout repeats every 1.0)
            let (kx, ky) = ((wx.rem_euclid(1.0) - wx).round() as i32, (wy.rem_euclid(1.0) - wy).round() as i32);
            let key = (r.id, kx, ky);
            if seen[..n].contains(&key) {
                continue;
            }
            seen[n] = key;
            n += 1;
            best = best.max(self.tile(&r, u + kx as f32, v + ky as f32));
        }
        best
    }
}
