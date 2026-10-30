// SPDX-License-Identifier: MIT OR Apache-2.0
//! Library graphs implemented natively (see `nodes10`): Tile Random, drawn by laying out random cells and stamping
//! one tile in each.
use crate::nodes10::{lerp, rnd, Cx};
use crate::scatter::builtin;
use crate::tex::{lum, Tex};
use std::f32::consts::TAU;

/// One cell of the layout, in parts of the picture.
#[derive(Clone, Copy)]
struct Cell {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// A row of `n` random sizes adding up to 1: `amount` 0 is even, 1 varies them by up to 90%.
fn sizes(n: i32, amount: f32, row: i32, seed: u32) -> Vec<f32> {
    let w: Vec<f32> = (0..n).map(|i| 1.0 + 0.9 * amount * (2.0 * rnd(i, row, seed) - 1.0)).collect();
    let total: f32 = w.iter().sum();
    w.iter().map(|x| x / total).collect()
}

/// Rows of random height, each with its own random widths.
fn rows(nx: i32, ny: i32, rx: f32, ry: f32, seed: u32) -> Vec<Cell> {
    let mut out = Vec::with_capacity((nx * ny) as usize);
    let mut y = 0.0;
    for (j, h) in sizes(ny, ry, -1, seed.wrapping_add(1)).into_iter().enumerate() {
        let mut x = 0.0;
        for w in sizes(nx, rx, j as i32, seed.wrapping_add(2)) {
            out.push(Cell { x, y, w, h });
            x += w;
        }
        y += h;
    }
    out
}

/// One pass of splitting: mode 1 cuts the longer side, 2 the width, 3 the height, 4 either at a random place. A cell
/// is cut when that side is over `threshold` of the longest side of any cell.
fn split(cells: Vec<Cell>, mode: i32, threshold: f32, pass: i32, seed: u32) -> Vec<Cell> {
    let longest = cells.iter().map(|c| c.w.max(c.h)).fold(0.0, f32::max);
    let mut out = Vec::with_capacity(cells.len() * 2);
    for (i, c) in cells.into_iter().enumerate() {
        let r = |k: i32| rnd(i as i32 + pass * 7919, k, seed.wrapping_add(3));
        let across = match mode {
            1 => c.w >= c.h,
            2 => true,
            3 => false,
            _ => r(1) < 0.5,
        };
        let side = if across { c.w } else { c.h };
        if side <= threshold * longest {
            out.push(c);
            continue;
        }
        let at = if mode == 4 { 0.3 + 0.4 * r(2) } else { 0.5 };
        if across {
            out.push(Cell { w: c.w * at, ..c });
            out.push(Cell { x: c.x + c.w * at, w: c.w * (1.0 - at), ..c });
        } else {
            out.push(Cell { h: c.h * at, ..c });
            out.push(Cell { y: c.y + c.h * at, h: c.h * (1.0 - at), ..c });
        }
    }
    out
}

/// Which of the tile's size measures tints it: 1 area, 2 width, 3 height, as a part of the largest.
fn size_value(mode: i32, c: &Cell, most: [f32; 3]) -> f32 {
    match mode {
        1 => c.w * c.h / most[0].max(1e-6),
        2 => c.w / most[1].max(1e-6),
        _ => c.h / most[2].max(1e-6),
    }
}

fn tile_random(cx: &Cx, colour: bool) -> Tex {
    let (s, hs, seed) = (cx.s, cx.hs, cx.seed);
    let (nx, ny) = (cx.i("x_amount").clamp(1, 64), cx.i("y_amount").clamp(1, 64));
    let mut cells = rows(nx, ny, cx.f("size_random_x").clamp(0.0, 1.0), cx.f("size_random_y").clamp(0.0, 1.0), seed);
    let mode = cx.i("split_mode");
    if mode > 0 {
        let threshold = cx.f("split_threshold").clamp(0.0, 1.0);
        for pass in 0..cx.i("split_multiplier").clamp(0, 10) {
            if cells.len() < 1 << 16 {
                cells = split(cells, mode, threshold, pass, seed);
            }
        }
    }
    let sides = || cells.iter().map(|c| c.w.min(c.h));
    let reference = if cx.i("interstice_mode") == 0 { sides().fold(1.0, f32::min) } else { sides().fold(0.0, f32::max) };
    let gap = cx.f("interstice_amount").clamp(0.0, 1.0) * reference;
    let most = [cells.iter().map(|c| c.w * c.h).fold(0.0, f32::max), cells.iter().map(|c| c.w).fold(0.0, f32::max), cells.iter().map(|c| c.h).fold(0.0, f32::max)];

    let pattern = cx.i("pattern");
    let image = if pattern == 1 { cx.img("pattern_input") } else { None };
    let (spec, spec_r) = (cx.f("pattern_specific"), cx.f("pattern_specific_random"));
    let (quarter, free_turn) = (cx.i("pattern_rotation"), cx.f("pattern_rotation_random"));
    let (flip, flip_mode) = (cx.f("pattern_symmetry_random"), cx.i("pattern_symmetry_random_mode"));
    let (scale, scale_r) = (cx.f("shape_scale").clamp(0.0, 1.0), cx.f("shape_scale_random").clamp(0.0, 1.0));
    let (rot, rot_r) = (cx.f("shape_rotation"), cx.f("shape_rotation_random"));
    let keep_apart = cx.b("shape_rotation_constraint");
    let (off, off_r) = (cx.f("position_offset"), cx.f("position_offset_random"));
    let (pos_r, touching) = (cx.f("position_random").clamp(0.0, 1.0), cx.b("position_random_constraints"));
    let (mask_r, mask_inv) = (cx.f("mask_random").clamp(0.0, 1.0), cx.b("mask_invert"));
    let (par, par_k) = (cx.i("pattern_color_parametrisation"), cx.f("pattern_color_parametrisation_intensity"));
    let par_affect = if colour { cx.i("color_parametrization_affect") } else { 0 };
    let blending = cx.i("blending_mode");
    let color = cx.col("pattern_color");
    // the grayscale graph has one amount, for the three channels together
    let color_r = if colour { cx.v4("pattern_color_random") } else { [cx.f("pattern_color_random"), cx.f("pattern_color_random"), cx.f("pattern_color_random"), 0.0] };

    let back = cx.img(if colour { "background_input_1" } else { "background_input" });
    let bg = cx.col("background_color");
    let mut out = match back {
        Some(t) => Tex::from_fn(s, hs, |u, v| t.sample(u, v)),
        None => Tex::solid(s, hs, bg),
    };
    if pattern <= 0 {
        return out;
    }

    for (i, c) in cells.iter().enumerate() {
        let r = |k: i32| rnd(i as i32, k, seed.wrapping_add(5));
        // the mask hides that many tiles; inverted, it hides the others
        if mask_r > 0.0 && (r(1) < mask_r) != mask_inv {
            continue;
        }
        let k = scale * (1.0 - scale_r * r(2));
        let (mut hx, mut hy) = (0.5 * (c.w - gap).max(0.0) * k, 0.5 * (c.h - gap).max(0.0) * k);
        let turn = rot + rot_r * (r(3) - 0.5);
        let (sn, cs) = (turn * TAU).sin_cos();
        if keep_apart && turn != 0.0 {
            // shrunk until the turned tile fits where it stood
            let (bx, by) = (cs.abs() * hx + sn.abs() * hy, sn.abs() * hx + cs.abs() * hy);
            let fit = (hx / bx.max(1e-6)).min(hy / by.max(1e-6)).min(1.0);
            hx *= fit;
            hy *= fit;
        }
        if hx * s as f32 <= 0.0 || hy * hs as f32 <= 0.0 {
            continue;
        }
        if touching {
            let fit = 1.0 / (1.0 + pos_r);
            hx *= fit;
            hy *= fit;
        }
        // with the constraint a tile stays inside its cell, without it it may leave by half a cell
        let (rx, ry) = if touching { ((0.5 * c.w - hx).max(0.0), (0.5 * c.h - hy).max(0.0)) } else { (0.5 * c.w, 0.5 * c.h) };
        let px = c.x + 0.5 * c.w + off * c.w + off_r * c.w * (r(4) - 0.5) + pos_r * rx * (2.0 * r(5) - 1.0);
        let py = c.y + 0.5 * c.h + pos_r * ry * (2.0 * r(6) - 1.0);

        // the pattern turns inside the tile: quarter turns, then a free one
        let quarters = match quarter {
            1..=3 => quarter,
            4 => 2 * (r(7) < 0.5) as i32,
            5 => 1 + 2 * (r(7) < 0.5) as i32,
            _ => 0,
        };
        let (pn, pc) = ((quarters as f32 * 0.25 + free_turn * r(8)) * TAU).sin_cos();
        let mirror = r(9) < flip;
        let (mx, my) = (mirror && flip_mode != 2 && (flip_mode == 1 || r(10) < 0.5 || r(11) < 0.5), mirror && flip_mode != 1 && (flip_mode == 2 || r(11) >= 0.5));
        let tile_spec = (spec + spec_r * (r(12) - 0.5)).clamp(0.0, 1.0);

        let mut tint = [0.0f32; 4];
        for ch in 0..4 {
            let draw = if colour { r(20 + ch as i32) } else { r(20) };
            tint[ch] = color[ch] * (1.0 - color_r[ch].clamp(0.0, 1.0) * draw);
        }
        if (1..=3).contains(&par) {
            let by_size = lerp(1.0, size_value(par, c, most), par_k.clamp(0.0, 1.0));
            for (ch, t) in tint.iter_mut().enumerate() {
                if par_affect == 0 || (par_affect == 1 && ch < 3) || (par_affect == 2 && ch == 3) {
                    *t *= by_size;
                }
            }
        }

        // the box the turned tile covers, one pixel larger for the soft edge
        let (ex, ey) = (cs.abs() * hx + sn.abs() * hy, sn.abs() * hx + cs.abs() * hy);
        let (w, h) = (s as f32, hs as f32);
        let (x0, x1) = (((px - ex) * w).floor() as i64 - 1, ((px + ex) * w).ceil() as i64 + 1);
        let (y0, y1) = (((py - ey) * h).floor() as i64 - 1, ((py + ey) * h).ceil() as i64 + 1);
        for y in y0..y1.min(y0 + hs as i64) {
            for x in x0..x1.min(x0 + s as i64) {
                let (du, dv) = ((x as f32 + 0.5) / w - px, (y as f32 + 0.5) / h - py);
                let (lx, ly) = (cs * du + sn * dv, -sn * du + cs * dv);
                let (nx, ny) = (lx / hx, ly / hy);
                let edge = ((1.0 - nx.abs()) * hx * w).min((1.0 - ny.abs()) * hy * h) + 0.5;
                if edge <= 0.0 {
                    continue;
                }
                // tile space, 0..1, then into the pattern
                let (ax, ay) = (((nx * pc + ny * pn) * if mx { -1.0 } else { 1.0 }), ((ny * pc - nx * pn) * if my { -1.0 } else { 1.0 }));
                let (lu, lv) = ((ax + 1.0) * 0.5, (ay + 1.0) * 0.5);
                let (rgb, mut cover) = match image {
                    Some(t) => {
                        let p = t.sample(lu.clamp(0.0, 1.0), lv.clamp(0.0, 1.0));
                        if colour { ([p[0], p[1], p[2]], p[3]) } else { ([1.0; 3], lum(p)) }
                    }
                    None if pattern == 1 => ([1.0; 3], 1.0),
                    None => ([1.0; 3], builtin((pattern - 1) as u32, lu, lv, tile_spec)),
                };
                cover *= edge.min(1.0);
                let (ix, iy) = (x.rem_euclid(s as i64) as usize, y.rem_euclid(hs as i64) as usize);
                let d = &mut out.px[iy * s + ix];
                if colour {
                    cover *= tint[3];
                    for ch in 0..3 {
                        let v = rgb[ch] * tint[ch];
                        d[ch] = match blending {
                            1 => d[ch].max(v * cover),
                            2 => lerp(d[ch], v, cover),
                            _ => d[ch] + v * cover,
                        };
                    }
                    d[3] = d[3].max(cover);
                } else {
                    let v = tint[0] * cover;
                    let g = if blending == 1 { d[0].max(v) } else { d[0] + v };
                    *d = [g, g, g, 1.0];
                }
            }
        }
    }
    for p in out.px.iter_mut() {
        for ch in p.iter_mut() {
            *ch = ch.clamp(0.0, 1.0);
        }
    }
    out
}

pub fn compute(cx: &Cx) -> Option<Tex> {
    Some(match cx.kind {
        "x_tile_random" => tile_random(cx, false),
        "x_tile_random_color" => tile_random(cx, true),
        _ => return None,
    })
}
