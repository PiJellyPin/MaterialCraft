// SPDX-License-Identifier: MIT OR Apache-2.0
//! Image-processing operators used by graph nodes. All are wrap-around (tileable).
use crate::tex::{lum, Tex};
use std::f32::consts::{PI, TAU};

fn sm(a: f32, b: f32, x: f32) -> f32 {
    let d = b - a;
    let t = if d.abs() < 1e-9 { if x >= b { 1.0 } else { 0.0 } } else { ((x - a) / d).clamp(0.0, 1.0) };
    t * t * (3.0 - 2.0 * t)
}

/// Shortest signed distance on a wrapping unit axis: maps any offset into [-0.5, 0.5], so shapes centred
/// anywhere on a tile continue seamlessly across its borders.
pub fn wrapd(d: f32) -> f32 {
    d - d.round()
}

/// Evaluate a centred shape at the wrapped offset (x, y). When the shape is larger than half a tile (`big`), the
/// neighbouring copies overlap this tile, so the union of the 3x3 copies is taken instead of only the nearest one.
pub fn tiled_max<F: Fn(f32, f32) -> f32>(big: bool, x: f32, y: f32, f: F) -> f32 {
    if !big {
        return f(x, y);
    }
    let mut m = f32::MIN;
    for dy in -1..=1 {
        for dx in -1..=1 {
            m = m.max(f(x + dx as f32, y + dy as f32));
        }
    }
    m
}

/// Make any texture tile: cross-fades it with a copy shifted by half a tile, with weights that hand over completely
/// to the shifted copy at the borders. Used when an operation cannot be periodic by itself.
pub fn seamless_blend(t: &Tex) -> Tex {
    let (w, h) = (t.w, t.h);
    let wt = |i: usize, n: usize| {
        let x = (i as f32 + 0.5) / n as f32;
        let d = (2.0 * x - 1.0).abs(); // 0 centre .. 1 border
        let k = 1.0 - d;
        k * k * (3.0 - 2.0 * k)
    };
    let mut o = t.clone();
    for y in 0..h {
        let wy = wt(y, h);
        for x in 0..w {
            let wx = wt(x, w);
            let (x2, y2) = ((x + w / 2) % w, (y + h / 2) % h);
            let a = t.px[y * w + x];
            let b = t.px[y * w + x2];
            let c = t.px[y2 * w + x];
            let d = t.px[y2 * w + x2];
            let mut r = [0.0f32; 4];
            for i in 0..4 {
                r[i] = a[i] * wx * wy + b[i] * (1.0 - wx) * wy + c[i] * wx * (1.0 - wy) + d[i] * (1.0 - wx) * (1.0 - wy);
            }
            o.px[y * w + x] = r;
        }
    }
    o
}

// ---------- blur / sharpen ----------
/// Widths (odd) of three successive box blurs whose combination approximates a gaussian of the given sigma.
pub(crate) fn gauss_boxes(sigma: f32) -> [i64; 3] {
    let n = 3.0f32;
    let w_ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i64;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m_ideal = (12.0 * sigma * sigma - n * (wl * wl) as f32 - 4.0 * n * wl as f32 - 3.0 * n) / (-4.0 * wl as f32 - 4.0);
    let m = m_ideal.round() as i64;
    [if 0 < m { wl } else { wu }, if 1 < m { wl } else { wu }, if 2 < m { wl } else { wu }]
}

/// One wrapping box blur along the rows of `src` (width w, height h), radius r, in parallel over rows.
fn box_rows(src: &[[f32; 4]], w: usize, h: usize, r: i64) -> Vec<[f32; 4]> {
    let mut dst = vec![[0.0f32; 4]; w * h];
    let rows_per = crate::pool::rows_per_piece(w, h);
    let inv = 1.0 / (2 * r + 1) as f32;
    let my = crate::tex::my_job();
    crate::pool::for_chunks(&mut dst, rows_per.max(1) * w, &|ci, chunk| {
        for (ri, out) in chunk.chunks_mut(w).enumerate() {
            if crate::tex::stale(my) {
                return;
            }
            let row = &src[(ci * rows_per + ri) * w..(ci * rows_per + ri + 1) * w];
            let wi = w as i64;
            let mut acc = [0.0f32; 4];
            for k in -r..=r {
                let p = row[k.rem_euclid(wi) as usize];
                for c in 0..4 {
                    acc[c] += p[c];
                }
            }
            let (mut ia, mut is) = ((r + 1).rem_euclid(wi), (-r).rem_euclid(wi));
            for x in 0..w {
                out[x] = [acc[0] * inv, acc[1] * inv, acc[2] * inv, acc[3] * inv];
                let (add, sub) = (row[ia as usize], row[is as usize]);
                for c in 0..4 {
                    acc[c] += add[c] - sub[c];
                }
                ia += 1;
                if ia == wi {
                    ia = 0;
                }
                is += 1;
                if is == wi {
                    is = 0;
                }
            }
        }
    });
    dst
}

/// Single-channel version of `box_rows` (grayscale images need a quarter of the memory traffic).
fn box_rows1(src: &[f32], w: usize, h: usize, r: i64) -> Vec<f32> {
    let mut dst = vec![0.0f32; w * h];
    let rows_per = crate::pool::rows_per_piece(w, h);
    let inv = 1.0 / (2 * r + 1) as f32;
    let my = crate::tex::my_job();
    crate::pool::for_chunks(&mut dst, rows_per.max(1) * w, &|ci, chunk| {
        for (ri, out) in chunk.chunks_mut(w).enumerate() {
            if crate::tex::stale(my) {
                return;
            }
            let row = &src[(ci * rows_per + ri) * w..(ci * rows_per + ri + 1) * w];
            let wi = w as i64;
            let mut acc = 0.0f32;
            for k in -r..=r {
                acc += row[k.rem_euclid(wi) as usize];
            }
            let (mut ia, mut is) = ((r + 1).rem_euclid(wi), (-r).rem_euclid(wi));
            for x in 0..w {
                out[x] = acc * inv;
                acc += row[ia as usize] - row[is as usize];
                ia += 1;
                if ia == wi {
                    ia = 0;
                }
                is += 1;
                if is == wi {
                    is = 0;
                }
            }
        }
    });
    dst
}

/// Vertical wrapping box blur of a single-channel image without transposing it: every piece owns a band of columns and
/// slides a running sum down the rows, reading whole row segments (cache friendly).
fn box_cols1(src: &[f32], w: usize, h: usize, r: i64) -> Vec<f32> {
    let mut dst = vec![0.0f32; w * h];
    let n = crate::pool::threads();
    let band = w.div_ceil(n).max(1);
    let inv = 1.0 / (2 * r + 1) as f32;
    let my = crate::tex::my_job();
    // each piece writes its own columns: split the destination into column bands through raw row pointers
    struct Ptr(*mut f32);
    unsafe impl Send for Ptr {}
    unsafe impl Sync for Ptr {}
    let base = Ptr(dst.as_mut_ptr());
    let basep = &base;
    crate::pool::for_each(w.div_ceil(band), &|bi| {
        let x0 = bi * band;
        let x1 = (x0 + band).min(w);
        let hi = h as i64;
        let mut acc = vec![0.0f32; x1 - x0];
        for k in -r..=r {
            let row = &src[(k.rem_euclid(hi) as usize) * w + x0..(k.rem_euclid(hi) as usize) * w + x1];
            for (a, v) in acc.iter_mut().zip(row) {
                *a += *v;
            }
        }
        for y in 0..h {
            if y % 32 == 0 && crate::tex::stale(my) {
                return;
            }
            // SAFETY: the bands are disjoint column ranges, so no two pieces write the same element, and `dst` outlives
            // the loop (`for_each` returns only when every piece is done)
            let out = unsafe { std::slice::from_raw_parts_mut(basep.0.add(y * w + x0), x1 - x0) };
            let ia = ((y as i64 + r + 1).rem_euclid(hi)) as usize;
            let is = ((y as i64 - r).rem_euclid(hi)) as usize;
            let (add, sub) = (&src[ia * w + x0..ia * w + x1], &src[is * w + x0..is * w + x1]);
            for i in 0..acc.len() {
                out[i] = acc[i] * inv;
                acc[i] += add[i] - sub[i];
            }
        }
    });
    dst
}

fn transpose(src: &[[f32; 4]], w: usize, h: usize) -> Vec<[f32; 4]> {
    let mut dst = vec![[0.0f32; 4]; w * h];
    // rows of the transposed image (= columns of the source)
    let rows_per = crate::pool::rows_per_piece(h, w);
    crate::pool::for_chunks(&mut dst, rows_per.max(1) * h, &|ci, chunk| {
        for (ri, out) in chunk.chunks_mut(h).enumerate() {
            let x = ci * rows_per + ri;
            for (y, o) in out.iter_mut().enumerate() {
                *o = src[y * w + x];
            }
        }
    });
    dst
}

/// Gaussian blur with wrap-around (tileable) edges. Large radii use three box passes (cost independent of the radius,
/// within a few percent of a true gaussian); small radii use the exact kernel.
pub fn blur(t: &Tex, sigma: f32) -> Tex {
    if sigma < 0.1 {
        return t.clone();
    }
    let (w, h) = (t.w, t.h);
    if sigma >= 2.5 && t.px.iter().all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 1.0) {
        // grayscale: blur one channel
        let mut cur: Vec<f32> = t.px.iter().map(|p| p[0]).collect();
        for bw in gauss_boxes(sigma) {
            let r = (bw / 2).max(0);
            cur = box_rows1(&cur, w, h, r.min((w as i64 - 1) / 2));
            cur = box_cols1(&cur, w, h, r.min((h as i64 - 1) / 2));
        }
        return Tex { w, h, px: cur.into_iter().map(|v| [v, v, v, 1.0]).collect() };
    }
    if sigma >= 2.5 {
        let mut cur = t.px.clone();
        for bw in gauss_boxes(sigma) {
            let r = (bw / 2).max(0);
            // a window wider than the image would wrap more than once; the result is then simply the mean
            cur = box_rows(&cur, w, h, r.min((w as i64 - 1) / 2));
            cur = transpose(&cur, w, h);
            cur = box_rows(&cur, h, w, r.min((h as i64 - 1) / 2));
            cur = transpose(&cur, h, w);
        }
        return Tex { w, h, px: cur };
    }
    let r = ((sigma * 3.0).ceil() as i64).clamp(1, 64);
    let mut k: Vec<f32> = (-r..=r).map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp()).collect();
    let s: f32 = k.iter().sum();
    k.iter_mut().for_each(|x| *x /= s);
    // exact kernel, rows then columns, on all cores; the taps are added in the same order as a plain loop would
    let wrap = |i: i64, n: usize| if i >= 0 && (i as usize) < n { i as usize } else { i.rem_euclid(n as i64) as usize };
    let mut tmp = vec![[0.0f32; 4]; w * h];
    crate::tex::par_rows(&mut tmp, w, |start, piece| {
        for (ry, out) in piece.chunks_mut(w).enumerate() {
            let y = start / w + ry;
            let row = &t.px[y * w..(y + 1) * w];
            for (x, o) in out.iter_mut().enumerate() {
                let mut a = [0.0f32; 4];
                for (i, kv) in k.iter().enumerate() {
                    let p = row[wrap(x as i64 + i as i64 - r, w)];
                    for c in 0..4 {
                        a[c] += p[c] * kv;
                    }
                }
                *o = a;
            }
        }
    });
    let mut out = vec![[0.0f32; 4]; w * h];
    crate::tex::par_rows(&mut out, w, |start, piece| {
        for (ry, orow) in piece.chunks_mut(w).enumerate() {
            let y = start / w + ry;
            for (x, o) in orow.iter_mut().enumerate() {
                let mut a = [0.0f32; 4];
                for (i, kv) in k.iter().enumerate() {
                    let p = tmp[wrap(y as i64 + i as i64 - r, h) * w + x];
                    for c in 0..4 {
                        a[c] += p[c] * kv;
                    }
                }
                *o = a;
            }
        }
    });
    Tex { w, h, px: out }
}

pub fn directional_blur(t: &Tex, angle: f32, length: f32) -> Tex {
    let (s, c) = (angle * TAU).sin_cos();
    let n = 24;
    Tex::from_fn(t.w, t.h, |u, v| {
        let mut a = [0.0f32; 4];
        for i in 0..n {
            let f = (i as f32 / (n - 1) as f32 - 0.5) * length;
            let p = t.sample(u + c * f, v + s * f);
            for k in 0..4 {
                a[k] += p[k] / n as f32;
            }
        }
        a
    })
}

pub fn sharpen(t: &Tex, radius: f32, amount: f32) -> Tex {
    let b = blur(t, radius);
    let mut o = t.clone();
    for (i, p) in o.px.iter_mut().enumerate() {
        for c in 0..3 {
            // no clamp: values above 1 (HDR) and the small undershoot next to edges are kept
            p[c] = t.px[i][c] + (t.px[i][c] - b.px[i][c]) * amount;
        }
    }
    o
}

// ---------- tonal ----------
pub fn levels(t: &Tex, in_lo: f32, in_hi: f32, gamma: f32, out_lo: f32, out_hi: f32) -> Tex {
    let d = if in_hi >= in_lo { (in_hi - in_lo).max(1e-6) } else { (in_hi - in_lo).min(-1e-6) };
    let g = 1.0 / gamma.max(1e-3);
    let unit_gamma = (g - 1.0).abs() < 1e-6;
    let lv = |x: f32| {
        let v = ((x - in_lo) / d).clamp(0.0, 1.0);
        out_lo + (out_hi - out_lo) * if unit_gamma { v } else { v.powf(g) }
    };
    t.map(|p| {
        if p[0] == p[1] && p[1] == p[2] {
            let v = lv(p[0]);
            return [v, v, v, p[3]];
        }
        [lv(p[0]), lv(p[1]), lv(p[2]), p[3]]
    })
}

pub fn adjust(t: &Tex, brightness: f32, contrast: f32) -> Tex {
    t.map(|p| {
        let mut o = p;
        for c in 0..3 {
            o[c] = (p[c] - 0.5) * contrast + 0.5 + brightness;
        }
        o
    })
}

/// Cyclic shift by a fraction of the tile (output pixel (x, y) reads input pixel (x + fx * w, y + fy * h)).
pub fn roll(t: &Tex, fx: f32, fy: f32) -> Tex {
    let (w, h) = (t.w, t.h);
    let sx = ((fx * w as f32).round() as i64).rem_euclid(w as i64) as usize;
    let sy = ((fy * h as f32).round() as i64).rem_euclid(h as i64) as usize;
    Tex { w, h, px: (0..w * h).map(|i| t.px[((i / w + sy) % h) * w + (i % w + sx) % w]).collect() }
}

/// Flip the green channel (a normal map between the DirectX and OpenGL conventions).
pub fn flip_green(t: &Tex) -> Tex {
    t.map(|p| [p[0], 1.0 - p[1], p[2], p[3]])
}

pub fn invert(t: &Tex) -> Tex {
    t.map(|p| [1.0 - p[0], 1.0 - p[1], 1.0 - p[2], p[3]])
}

pub fn threshold(t: &Tex, th: f32, soft: f32) -> Tex {
    t.map(|p| {
        let g = sm(th - soft * 0.5, th + soft * 0.5, lum(p));
        [g, g, g, 1.0]
    })
}

/// Threshold: mode 0 greater, 1 greater or equal, 2 lower, 3 lower or equal. With softness > 0 the
/// edge is smoothed and the comparison direction is all that remains of the mode.
pub fn threshold_mode(t: &Tex, th: f32, soft: f32, mode: u32) -> Tex {
    t.map(move |p| {
        let l = lum(p);
        let g = if soft > 1e-6 {
            let g = sm(th - soft * 0.5, th + soft * 0.5, l);
            if mode >= 2 { 1.0 - g } else { g }
        } else {
            let on = match mode {
                0 => l > th,
                1 => l >= th,
                2 => l < th,
                _ => l <= th,
            };
            if on { 1.0 } else { 0.0 }
        };
        [g, g, g, 1.0]
    })
}

pub fn histogram_scan(t: &Tex, pos: f32, width: f32) -> Tex {
    let lo = pos - width * 0.5;
    let w = width.max(1e-4);
    t.map(|p| {
        let g = ((lum(p) - lo) / w).clamp(0.0, 1.0);
        [g, g, g, 1.0]
    })
}

pub fn quantize(t: &Tex, steps: f32) -> Tex {
    let s = steps.max(2.0) - 1.0;
    t.map(|p| {
        let q = |v: f32| (v * s).round() / s;
        [q(p[0]), q(p[1]), q(p[2]), p[3]]
    })
}

pub fn to_gray(t: &Tex) -> Tex {
    t.map(|p| {
        let g = lum(p);
        [g, g, g, 1.0]
    })
}

pub fn channel(t: &Tex, ch: usize) -> Tex {
    let ch = ch.min(3);
    t.map(|p| [p[ch], p[ch], p[ch], 1.0])
}

pub fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = mx - mn;
    let h = if d < 1e-9 {
        0.0
    } else if mx == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    (h, if mx <= 0.0 { 0.0 } else { d / mx }, mx)
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let h6 = h.rem_euclid(1.0) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

pub fn hsl_adjust(t: &Tex, hue: f32, sat: f32, val: f32) -> Tex {
    t.map(|p| {
        let (h, s, v) = rgb_to_hsv(p[0], p[1], p[2]);
        let (r, g, b) = hsv_to_rgb(h + hue, (s * sat).clamp(0.0, 1.0), (v * val).max(0.0));
        [r, g, b, p[3]]
    })
}

/// Gradient map. stops: (position, rgb).
pub fn colorize(t: &Tex, stops: &[(f32, [f32; 3])], interp: u32) -> Tex {
    let mut st: Vec<(f32, [f32; 3])> = stops.to_vec();
    st.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    t.map(|p| {
        let g = lum(p).clamp(0.0, 1.0);
        if st.is_empty() {
            return [g, g, g, p[3]];
        }
        if g <= st[0].0 {
            let c = st[0].1;
            return [c[0], c[1], c[2], p[3]];
        }
        for i in 1..st.len() {
            if g <= st[i].0 {
                let (a, b) = (st[i - 1], st[i]);
                let d = (b.0 - a.0).max(1e-9);
                let mut f = (g - a.0) / d;
                if interp == 1 {
                    f = f * f * (3.0 - 2.0 * f);
                } else if interp == 2 {
                    f = 0.0;
                }
                return [
                    a.1[0] + (b.1[0] - a.1[0]) * f,
                    a.1[1] + (b.1[1] - a.1[1]) * f,
                    a.1[2] + (b.1[2] - a.1[2]) * f,
                    p[3],
                ];
            }
        }
        let c = st[st.len() - 1].1;
        [c[0], c[1], c[2], p[3]]
    })
}

// ---------- blending ----------
fn bm(mode: u32, f: f32, b: f32) -> f32 {
    match mode {
        0 => f,
        1 => f + b,
        2 => b - f,
        3 => f * b,
        4 => 1.0 - (1.0 - f) * (1.0 - b),
        5 => {
            if b < 0.5 {
                2.0 * f * b
            } else {
                1.0 - 2.0 * (1.0 - f) * (1.0 - b)
            }
        }
        6 => f.max(b),
        7 => f.min(b),
        8 => (f - b).abs(),
        9 => (1.0 - 2.0 * f) * b * b + 2.0 * f * b,
        10 => if f >= 0.999 { 1.0 } else { b / (1.0 - f) },
        11 => if f <= 0.001 { 0.0 } else { 1.0 - (1.0 - b) / f },
        12 => f + b - 1.0,
        13 => if f < 0.5 { 2.0 * f * b } else { 1.0 - 2.0 * (1.0 - f) * (1.0 - b) },
        14 => f + b - 2.0 * f * b,
        15 => if f <= 0.001 { b } else { b / f },
        16 => b + 2.0 * f - 1.0,
        20 => b + (f - 0.5) * 2.0,
        _ => f,
    }
}

/// Modes: 0 copy,1 add,2 sub,3 multiply,4 screen,5 overlay,6 max,7 min,8 difference,9 soft light,10 color dodge,11 color burn,12 linear burn,13 hard light,14 exclusion,15 divide,16 linear light,17 color,18 luminosity,19 dissolve,20 add sub,21 switch (copy).
pub fn blend(fg: &Tex, bg: &Tex, mask: Option<&Tex>, mode: u32, opacity: f32) -> Tex {
    blend_ex(fg, bg, mask, mode, opacity, 1, [0.0, 0.0, 1.0, 1.0])
}

/// Blend with alpha handling and cropping area. `alpha_mode`: 0 use source alpha, 1 ignore alpha, 2 straight alpha
/// blending, 3 premultiplied alpha blending. `crop` = [x0, y0, x1, y1] in 0..1; outside it only the background shows.
pub fn blend_ex(fg: &Tex, bg: &Tex, mask: Option<&Tex>, mode: u32, opacity: f32, alpha_mode: u32, crop: [f32; 4]) -> Tex {
    // every channel of every pixel is written below, so the background is read, not copied first
    let mut out = Tex { w: bg.w, h: bg.h, px: vec![[0.0f32; 4]; bg.px.len()] };
    let (ow, oh) = (out.w, out.h);
    let cropped = crop != [0.0, 0.0, 1.0, 1.0];
    crate::tex::par_rows(&mut out.px, ow, |start, chunk| {
      for (j, dst) in chunk.iter_mut().enumerate() {
        let i = start + j;
        let f = fg.px.get(i).copied().unwrap_or([0.0; 4]);
        let b = bg.px[i];
        let mut m = mask.and_then(|m| m.px.get(i)).map(|p| lum(*p)).unwrap_or(1.0) * opacity;
        if cropped {
            let (px, py) = (((i % ow) as f32 + 0.5) / ow as f32, ((i / ow) as f32 + 0.5) / oh as f32);
            if px < crop[0] || px > crop[2] || py < crop[1] || py > crop[3] {
                m = 0.0;
            }
        }
        // alpha: 1 leaves the background alpha alone, the others let the foreground alpha act as extra opacity
        let m_plain = m;
        match alpha_mode {
            0 => {
                dst[3] = b[3] + (f[3] - b[3]) * m_plain;
                m *= f[3];
            }
            2 | 3 => {
                dst[3] = f[3] * m_plain + b[3] * (1.0 - f[3] * m_plain);
                m *= f[3];
            }
            _ => dst[3] = b[3],
        }
        if (17..=19).contains(&mode) {
            // 17 color (hue+saturation of the foreground, brightness of the background), 18 luminosity, 19 dissolve
            let set_lum = |c: [f32; 4], l: f32| {
                let d = l - lum(c);
                [(c[0] + d).clamp(0.0, 1.0), (c[1] + d).clamp(0.0, 1.0), (c[2] + d).clamp(0.0, 1.0)]
            };
            let r = match mode {
                17 => set_lum(f, lum(b)),
                18 => set_lum(b, lum(f)),
                _ => {
                    let keep = crate::noise::rnd((i % out.w) as i32, (i / out.w) as i32, 4242) < m;
                    if keep { [f[0], f[1], f[2]] } else { [b[0], b[1], b[2]] }
                }
            };
            let m2 = if mode == 19 { 1.0 } else { m };
            for c in 0..3 {
                dst[c] = (b[c] + (r[c] - b[c]) * m2).clamp(0.0, 1.0);
            }
            continue;
        }
        for c in 0..3 {
            dst[c] = (b[c] + (bm(mode, f[c], b[c]).clamp(0.0, 1.0) - b[c]) * m).clamp(0.0, 1.0);
        }
      }
    });
    out
}

// ---------- geometry ----------
pub fn transform(t: &Tex, ox: f32, oy: f32, sx: f32, sy: f32, rot: f32, tile: bool) -> Tex {
    let (s, c) = (rot * TAU).sin_cos();
    let sx = sx.abs().max(1e-4);
    let sy = sy.abs().max(1e-4);
    // the resampling is periodic only if the matrix maps the tile lattice onto itself (whole numbers)
    let whole = [c / sx, s / sx, s / sy, c / sy].iter().all(|m| (m - m.round()).abs() < 1e-3);
    let out = Tex::from_fn(t.w, t.h, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;
        let rx = c * x + s * y;
        let ry = -s * x + c * y;
        let (su, sv) = (rx / sx + 0.5 - ox, ry / sy + 0.5 - oy);
        if !tile && !(0.0..1.0).contains(&su) || !tile && !(0.0..1.0).contains(&sv) {
            return [0.0, 0.0, 0.0, 1.0];
        }
        t.sample_cubic(su, sv)
    });
    if tile && !whole {
        seamless_blend(&out)
    } else {
        out
    }
}

pub fn mirror(t: &Tex, axis: u32) -> Tex {
    let (w, h) = (t.w as i64, t.h as i64);
    let mut o = t.clone();
    for y in 0..h {
        for x in 0..w {
            let sx = if axis == 0 || axis == 2 { x.min(w - 1 - x) } else { x };
            let sy = if axis == 1 || axis == 2 { y.min(h - 1 - y) } else { y };
            o.px[(y * w + x) as usize] = t.at(sx, sy);
        }
    }
    o
}

/// Mirror: `axis` 0 mirrors across a vertical axis, 1 across a horizontal axis, 2 both, 3 corner.
/// `ox`/`oy` are the axis positions (0..1); `inv_*` copy the other side; `corner` 0 top-left, 1 top-right, 2 bottom-left, 3 bottom-right.
pub fn mirror_ex(t: &Tex, axis: u32, ox: f32, oy: f32, inv_x: bool, inv_y: bool, corner: u32) -> Tex {
    let (w, h) = (t.w as i64, t.h as i64);
    // pixel on the source side stays, the other side reads the reflected pixel
    let fold = |x: i64, n: i64, o: f32, inv: bool| -> i64 {
        let ax = o.clamp(0.0, 1.0) * n as f32;
        let xc = x as f32 + 0.5;
        let source = if inv { xc >= ax } else { xc < ax };
        if source { x } else { ((2.0 * ax - xc - 0.5).round() as i64).rem_euclid(n) }
    };
    let (do_x, do_y, ix, iy, ox, oy) = match axis {
        0 => (true, false, inv_x, false, ox, 0.5),
        1 => (false, true, false, inv_y, 0.5, oy),
        2 => (true, true, inv_x, inv_y, ox, oy),
        _ => (true, true, corner == 1 || corner == 3, corner == 2 || corner == 3, 0.5, 0.5),
    };
    let mut o = t.clone();
    for y in 0..h {
        let sy = if do_y { fold(y, h, oy, iy) } else { y };
        for x in 0..w {
            let sx = if do_x { fold(x, w, ox, ix) } else { x };
            o.px[(y * w + x) as usize] = t.at(sx, sy);
        }
    }
    o
}

/// Sampling used by the warps: 0 bilinear, 1 nearest, 2 bicubic.
fn samp(t: &Tex, mode: u32, u: f32, v: f32) -> [f32; 4] {
    match mode {
        0 => t.sample(u, v),
        1 => t.at((u.rem_euclid(1.0) * t.w as f32) as i64, (v.rem_euclid(1.0) * t.h as f32) as i64),
        _ => t.sample_cubic(u, v),
    }
}

/// The slope of `w` is measured per 1/256 of the image, so the displacement is the same at every resolution.
pub fn warp_ex(t: &Tex, w: &Tex, intensity: f32, filtering: u32) -> Tex {
    let (kx, ky) = (w.w as f32 / 256.0, w.h as f32 / 256.0);
    Tex::from_fn(t.w, t.h, |u, v| {
        let x = (u * w.w as f32) as i64;
        let y = (v * w.h as f32) as i64;
        let dx = (w.gray_at(x + 1, y) - w.gray_at(x - 1, y)) * kx;
        let dy = (w.gray_at(x, y + 1) - w.gray_at(x, y - 1)) * ky;
        samp(t, filtering, u + dx * intensity, v + dy * intensity)
    })
}

/// `offset` is subtracted from the intensity map (0.5 = mid grey does not move the pixel).
pub fn directional_warp_ex(t: &Tex, w: &Tex, angle: f32, intensity: f32, offset: f32, filtering: u32) -> Tex {
    let (s, c) = (angle * TAU).sin_cos();
    Tex::from_fn(t.w, t.h, |u, v| {
        let d = (lum(w.sample(u, v)) - offset) * intensity;
        samp(t, filtering, u + c * d, v + s * d)
    })
}

/// Displace by the gradient of `w` (slope warp).
pub fn warp(t: &Tex, w: &Tex, intensity: f32) -> Tex {
    let (kx, ky) = (w.w as f32 / 256.0, w.h as f32 / 256.0);
    Tex::from_fn(t.w, t.h, |u, v| {
        let x = (u * w.w as f32) as i64;
        let y = (v * w.h as f32) as i64;
        let dx = (w.gray_at(x + 1, y) - w.gray_at(x - 1, y)) * kx;
        let dy = (w.gray_at(x, y + 1) - w.gray_at(x, y - 1)) * ky;
        t.sample_cubic(u + dx * intensity, v + dy * intensity)
    })
}

pub fn directional_warp(t: &Tex, w: &Tex, angle: f32, intensity: f32) -> Tex {
    let (s, c) = (angle * TAU).sin_cos();
    Tex::from_fn(t.w, t.h, |u, v| {
        let d = (lum(w.sample(u, v)) - 0.5) * intensity;
        t.sample_cubic(u + c * d, v + s * d)
    })
}

// ---------- surface analysis ----------
pub fn normal_from_height(h: &Tex, strength: f32, directx: bool) -> Tex {
    let k = strength * h.w as f32 * 0.05;
    let (w, hh) = (h.w, h.h);
    // luminance once per pixel instead of nine times
    let gray: Vec<f32> = h.px.iter().map(|p| lum(*p)).collect();
    let mut out = Tex { w, h: hh, px: vec![[0.0, 0.0, 1.0, 1.0]; w * hh] };
    crate::tex::par_rows(&mut out.px, w, |start, chunk| {
        for (j, dst) in chunk.iter_mut().enumerate() {
            let i = start + j;
            let (x, y) = (i % w, i / w);
            let (xm, xp) = (if x == 0 { w - 1 } else { x - 1 }, if x + 1 == w { 0 } else { x + 1 });
            let (ym, yp) = (if y == 0 { hh - 1 } else { y - 1 }, if y + 1 == hh { 0 } else { y + 1 });
            // 3x3 Scharr kernel: smoother and more rotation-invariant than a plain central difference
            let g = |xx: usize, yy: usize| gray[yy * w + xx];
            let dx = (3.0 * (g(xp, ym) - g(xm, ym)) + 10.0 * (g(xp, y) - g(xm, y)) + 3.0 * (g(xp, yp) - g(xm, yp))) / 32.0 * k;
            let mut dy = (3.0 * (g(xm, yp) - g(xm, ym)) + 10.0 * (g(x, yp) - g(x, ym)) + 3.0 * (g(xp, yp) - g(xp, ym))) / 32.0 * k;
            if directx {
                dy = -dy;
            }
            let (nx, ny, nz) = (-dx, dy, 1.0f32);
            let l = (nx * nx + ny * ny + nz * nz).sqrt();
            *dst = [nx / l * 0.5 + 0.5, ny / l * 0.5 + 0.5, nz / l * 0.5 + 0.5, 1.0];
        }
    });
    out
}

pub fn normal_blend(a: &Tex, b: &Tex) -> Tex {
    let mut o = a.clone();
    for i in 0..o.px.len() {
        let pa = a.px[i];
        let pb = b.px.get(i).copied().unwrap_or([0.5, 0.5, 1.0, 1.0]);
        let x = (pa[0] - 0.5) * 2.0 + (pb[0] - 0.5) * 2.0;
        let y = (pa[1] - 0.5) * 2.0 + (pb[1] - 0.5) * 2.0;
        let z = (pa[2] - 0.5) * 2.0 * (pb[2] - 0.5) * 2.0;
        let l = (x * x + y * y + z * z).sqrt().max(1e-6);
        o.px[i] = [x / l * 0.5 + 0.5, y / l * 0.5 + 0.5, z / l * 0.5 + 0.5, 1.0];
    }
    o
}

/// Normal Blend with Opacity and optional Mask: `a` is the background, `b` the foreground.
pub fn normal_blend_ex(a: &Tex, b: &Tex, opacity: f32, mask: Option<&Tex>) -> Tex {
    let full = normal_blend(a, b);
    let mut o = full.clone();
    for i in 0..o.px.len() {
        let k = (opacity * mask.and_then(|m| m.px.get(i)).map(|p| lum(*p)).unwrap_or(1.0)).clamp(0.0, 1.0);
        if k >= 1.0 {
            continue;
        }
        let (pa, pf) = (a.px[i], full.px[i]);
        let v: Vec<f32> = (0..3).map(|c| (pa[c] - 0.5) * 2.0 * (1.0 - k) + (pf[c] - 0.5) * 2.0 * k).collect();
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
        o.px[i] = [v[0] / l * 0.5 + 0.5, v[1] / l * 0.5 + 0.5, v[2] / l * 0.5 + 0.5, 1.0];
    }
    o
}

pub fn ambient_occlusion(h: &Tex, radius: f32, intensity: f32) -> Tex {
    let g = to_gray(h);
    let b = blur(&g, radius);
    gray_combine(&g, &b, |g, b| (1.0 - (b - g) * intensity).clamp(0.0, 1.0))
}

pub fn curvature(h: &Tex, radius: f32, intensity: f32) -> Tex {
    let g = to_gray(h);
    let b = blur(&g, radius);
    gray_combine(&g, &b, |g, b| (0.5 + (g - b) * intensity).clamp(0.0, 1.0))
}

/// Gray image from the first channels of two equally sized images, on all cores.
fn gray_combine(a: &Tex, b: &Tex, f: impl Fn(f32, f32) -> f32 + Sync) -> Tex {
    let mut px = vec![[0.0f32; 4]; a.px.len()];
    crate::tex::par_rows(&mut px, a.w.max(1), |start, piece| {
        for (j, o) in piece.iter_mut().enumerate() {
            let v = f(a.px[start + j][0], b.px[start + j][0]);
            *o = [v, v, v, 1.0];
        }
    });
    Tex { w: a.w, h: a.h, px }
}

pub fn edge_detect(t: &Tex, intensity: f32) -> Tex {
    Tex::from_fn(t.w, t.h, |u, v| {
        let x = (u * t.w as f32) as i64;
        let y = (v * t.h as f32) as i64;
        let gx = -t.gray_at(x - 1, y - 1) - 2.0 * t.gray_at(x - 1, y) - t.gray_at(x - 1, y + 1)
            + t.gray_at(x + 1, y - 1)
            + 2.0 * t.gray_at(x + 1, y)
            + t.gray_at(x + 1, y + 1);
        let gy = -t.gray_at(x - 1, y - 1) - 2.0 * t.gray_at(x, y - 1) - t.gray_at(x + 1, y - 1)
            + t.gray_at(x - 1, y + 1)
            + 2.0 * t.gray_at(x, y + 1)
            + t.gray_at(x + 1, y + 1);
        let m = ((gx * gx + gy * gy).sqrt() * intensity).clamp(0.0, 1.0);
        [m, m, m, 1.0]
    })
}

/// Separable grey-scale dilation (maximum filter) of radius `r` pixels, wrapping at the borders.
pub fn dilate(t: &Tex, r: i64) -> Tex {
    if r <= 0 {
        return t.clone();
    }
    let h = Tex::from_fn(t.w, t.h, |u, v| {
        let (x, y) = ((u * t.w as f32) as i64, (v * t.h as f32) as i64);
        let m = (-r..=r).map(|d| t.gray_at(x + d, y)).fold(0.0f32, f32::max);
        [m, m, m, 1.0]
    });
    Tex::from_fn(t.w, t.h, |u, v| {
        let (x, y) = ((u * t.w as f32) as i64, (v * t.h as f32) as i64);
        let m = (-r..=r).map(|d| h.gray_at(x, y + d)).fold(0.0f32, f32::max);
        [m, m, m, 1.0]
    })
}

/// Edge Detect: white lines of `width` pixels where the image changes by more than `tolerance`;
/// `roundness` (pixels) rounds the corners of the lines; `invert` flips the result.
pub fn edge_detect_ex(t: &Tex, width: f32, roundness: f32, invert: bool, tolerance: f32) -> Tex {
    let g = to_gray(t);
    // Sobel magnitude, 1 for a full black / white step
    let e = Tex::from_fn(t.w, t.h, |u, v| {
        let (x, y) = ((u * t.w as f32) as i64, (v * t.h as f32) as i64);
        let gx = -g.gray_at(x - 1, y - 1) - 2.0 * g.gray_at(x - 1, y) - g.gray_at(x - 1, y + 1) + g.gray_at(x + 1, y - 1) + 2.0 * g.gray_at(x + 1, y) + g.gray_at(x + 1, y + 1);
        let gy = -g.gray_at(x - 1, y - 1) - 2.0 * g.gray_at(x, y - 1) - g.gray_at(x + 1, y - 1) + g.gray_at(x - 1, y + 1) + 2.0 * g.gray_at(x, y + 1) + g.gray_at(x + 1, y + 1);
        let m = (gx * gx + gy * gy).sqrt() * 0.25;
        let k = tolerance.clamp(0.0, 0.999);
        // everything below the tolerance is no edge; a step of the tolerance size already counts as a full edge
        let o = if m <= k * 0.5 { 0.0 } else { ((m - k * 0.5) / (0.5 * (1.0 - k)).max(1e-3)).clamp(0.0, 1.0) };
        [o, o, o, 1.0]
    });
    let mut d = dilate(&e, ((width.max(1.0) - 1.0) * 0.5).round() as i64);
    if roundness > 0.5 {
        let b = blur(&d, roundness * 0.5);
        d = Tex::from_fn(t.w, t.h, |u, v| {
            let x = sm(0.25, 0.75, lum(b.sample(u, v)));
            [x, x, x, 1.0]
        });
    }
    if invert {
        for p in d.px.iter_mut() {
            let v = 1.0 - p[0];
            *p = [v, v, v, 1.0];
        }
    }
    d
}

/// Curvature of a normal map: the divergence of its XY components (convex bright, concave dark).
pub fn curvature_normal(n: &Tex, intensity: f32, directx: bool) -> Tex {
    let s = if directx { -1.0 } else { 1.0 };
    Tex::from_fn(n.w, n.h, |u, v| {
        let (x, y) = ((u * n.w as f32) as i64, (v * n.h as f32) as i64);
        let div = (n.at(x + 1, y)[0] - n.at(x - 1, y)[0]) - s * (n.at(x, y + 1)[1] - n.at(x, y - 1)[1]);
        let g = (0.5 + div * intensity).clamp(0.0, 1.0);
        [g, g, g, 1.0]
    })
}

/// Mosaic driven by a map: every region of equal map value takes the mean colour of the input inside it.
/// `intensity` blends between the input (0) and the mosaic (1).
pub fn mosaic_map(t: &Tex, map: &Tex, intensity: f32) -> Tex {
    use std::collections::HashMap;
    let key = |u: f32, v: f32| (lum(map.sample(u, v)) * 4095.0).round() as i32;
    let mut acc: HashMap<i32, ([f64; 4], f64)> = HashMap::new();
    for y in 0..t.h {
        for x in 0..t.w {
            let (u, v) = ((x as f32 + 0.5) / t.w as f32, (y as f32 + 0.5) / t.h as f32);
            let e = acc.entry(key(u, v)).or_insert(([0.0; 4], 0.0));
            let p = t.px[y * t.w + x];
            for c in 0..4 {
                e.0[c] += p[c] as f64;
            }
            e.1 += 1.0;
        }
    }
    let k = intensity.clamp(0.0, 1.0);
    Tex::from_fn(t.w, t.h, |u, v| {
        let src = t.sample(u, v);
        match acc.get(&key(u, v)) {
            Some((s, n)) if *n > 0.0 => {
                let mut o = [0.0f32; 4];
                for c in 0..4 {
                    o[c] = src[c] + ((s[c] / n) as f32 - src[c]) * k;
                }
                o
            }
            _ => src,
        }
    })
}

pub fn emboss(t: &Tex, angle: f32, intensity: f32) -> Tex {
    let (s, c) = (angle * TAU).sin_cos();
    Tex::from_fn(t.w, t.h, |u, v| {
        let x = (u * t.w as f32) as i64;
        let y = (v * t.h as f32) as i64;
        let dx = t.gray_at(x + 1, y) - t.gray_at(x - 1, y);
        let dy = t.gray_at(x, y + 1) - t.gray_at(x, y - 1);
        let g = (0.5 + (dx * c + dy * s) * intensity).clamp(0.0, 1.0);
        [g, g, g, 1.0]
    })
}

/// Emboss as Substance Designer's Emboss node does it: the source lit by the slopes of a height map.
/// Slopes facing the light add the highlight colour, slopes facing away take light out where the shadow colour
/// is dark; `gloss` (0 = off) adds a sharp specular highlight in the highlight colour.
pub fn emboss_lit(src: &Tex, height: Option<&Tex>, angle: f32, intensity: f32, hl: [f32; 3], sh: [f32; 3], gloss: f32) -> Tex {
    let Some(h) = height else { return src.clone() };
    let (s, c) = (angle * TAU).sin_cos();
    // light 45 degrees above the surface, towards the light angle
    let l = {
        let v = [c, s, 1.0];
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / n, v[1] / n, v[2] / n]
    };
    let hv = {
        let v = [l[0], l[1], l[2] + 1.0];
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / n, v[1] / n, v[2] / n]
    };
    let flat_spec = hv[2];
    let expo = if gloss > 1e-4 { (2.0 / (gloss * gloss)).clamp(2.0, 4096.0) } else { 0.0 };
    let (w, hh) = (src.w, src.h);
    Tex::from_fn(w, hh, |u, v| {
        let p = src.sample(u, v);
        let x = (u * h.w as f32) as i64;
        let y = (v * h.h as f32) as i64;
        let dx = h.gray_at(x + 1, y) - h.gray_at(x - 1, y);
        let dy = h.gray_at(x, y + 1) - h.gray_at(x, y - 1);
        let d = (dx * c + dy * s) * intensity;
        let (lit, shade) = (d.max(0.0), (-d).max(0.0));
        let mut spec = 0.0;
        if expo > 0.0 {
            let n = [-dx * intensity, -dy * intensity, 1.0];
            let nl = (n[0] * n[0] + n[1] * n[1] + 1.0).sqrt();
            let ndh = (n[0] * hv[0] + n[1] * hv[1] + n[2] * hv[2]) / nl;
            // only the part brighter than a flat surface's reflection, so flat areas stay as they are
            spec = ((ndh.max(0.0).powf(expo) - flat_spec.powf(expo)).max(0.0) * gloss * 2.0).min(1.0);
        }
        let ch = |k: usize| (p[k] + hl[k] * (lit + spec) - (1.0 - sh[k]) * shade).clamp(0.0, 1.0);
        [ch(0), ch(1), ch(2), p[3]]
    })
}

// ---------- shapes ----------
pub fn polygon_sdf(x: f32, y: f32, sides: f32, radius: f32, rot: f32) -> f32 {
    let n = sides.round().max(3.0);
    let a = y.atan2(x) - rot * TAU;
    let sector = TAU / n;
    let r = (x * x + y * y).sqrt();
    let local = a.rem_euclid(sector) - sector * 0.5;
    r * local.cos() - radius * (PI / n).cos()
}

pub fn bricks(u: f32, v: f32, rows: f32, cols: f32, mortar: f32, bevel: f32, offset: f32, var: f32, seed: u32) -> f32 {
    let rows = rows.round().max(1.0);
    let cols = cols.round().max(1.0);
    let row = (v * rows).floor();
    let shift = if (row as i64) % 2 != 0 { offset } else { 0.0 };
    let cx = u * cols + shift;
    let col = cx.floor();
    let fu = cx - col;
    let fv = v * rows - row;
    let e = fu.min(1.0 - fu).min(fv.min(1.0 - fv));
    let hgt = ((e - mortar) / bevel.max(1e-4)).clamp(0.0, 1.0);
    let id = crate::noise::rnd(col.rem_euclid(cols) as i32, row as i32, seed);
    hgt * (1.0 - var + var * id)
}

// ---------- scattering ----------
#[allow(clippy::too_many_arguments)]
pub fn splatter(
    w: usize,
    h: usize,
    pattern: Option<&Tex>,
    count: u32,
    scale: f32,
    scale_var: f32,
    rot_var: f32,
    lum_var: f32,
    seed: u32,
) -> Tex {
    use crate::noise::rnd;
    let inst: Vec<(f32, f32, f32, f32, f32)> = (0..count.min(2000))
        .map(|i| {
            let i = i as i32;
            (
                rnd(i, 1, seed),
                rnd(i, 2, seed),
                (scale * (1.0 + (rnd(i, 3, seed) - 0.5) * 2.0 * scale_var)).max(1e-3),
                (rnd(i, 4, seed) - 0.5) * rot_var * TAU,
                1.0 - lum_var * rnd(i, 5, seed),
            )
        })
        .collect();
    Tex::from_fn(w, h, |u, v| {
        let mut best = 0.0f32;
        for &(cx, cy, s, r, l) in &inst {
            let dx = (u - cx + 0.5).rem_euclid(1.0) - 0.5;
            let dy = (v - cy + 0.5).rem_euclid(1.0) - 0.5;
            let (sn, cs) = r.sin_cos();
            let lx = (cs * dx + sn * dy) / s + 0.5;
            let ly = (-sn * dx + cs * dy) / s + 0.5;
            if !(0.0..=1.0).contains(&lx) || !(0.0..=1.0).contains(&ly) {
                continue;
            }
            let val = match pattern {
                Some(p) => lum(p.sample(lx, ly)),
                None => {
                    let d = ((lx - 0.5).powi(2) + (ly - 0.5).powi(2)).sqrt() * 2.0;
                    1.0 - sm(0.8, 1.0, d)
                }
            };
            best = best.max(val * l);
        }
        [best, best, best, 1.0]
    })
}

// ---------- 3D-ish preview ----------
pub fn lit_preview(base: &Tex, normal: &Tex, rough: &Tex, metal: &Tex, ao: &Tex, light: [f32; 3]) -> Tex {
    let l = {
        let n = (light[0] * light[0] + light[1] * light[1] + light[2] * light[2]).sqrt().max(1e-6);
        [light[0] / n, light[1] / n, light[2] / n]
    };
    let hv = {
        let x = l[0];
        let y = l[1];
        let z = l[2] + 1.0;
        let n = (x * x + y * y + z * z).sqrt().max(1e-6);
        [x / n, y / n, z / n]
    };
    Tex::from_fn(base.w, base.h, |u, v| {
        let b = base.sample(u, v);
        let n = normal.sample(u, v);
        let (nx, ny, nz) = (n[0] * 2.0 - 1.0, n[1] * 2.0 - 1.0, n[2] * 2.0 - 1.0);
        let r = lum(rough.sample(u, v)).clamp(0.05, 1.0);
        let m = lum(metal.sample(u, v)).clamp(0.0, 1.0);
        let o = lum(ao.sample(u, v));
        let ndl = (nx * l[0] + ny * l[1] + nz * l[2]).max(0.0);
        let ndh = (nx * hv[0] + ny * hv[1] + nz * hv[2]).max(0.0);
        let shin = 2.0 / (r * r * r * r) - 2.0;
        let spec = ndh.powf(shin.clamp(1.0, 2000.0)) * (1.0 - r * 0.5);
        let mut out = [0.0; 4];
        for c in 0..3 {
            let diff = b[c] * (1.0 - m) * ndl;
            let sp = spec * (0.04 * (1.0 - m) + b[c] * m);
            out[c] = ((0.1 * b[c] + diff + sp) * o).clamp(0.0, 1.0);
        }
        out[3] = 1.0;
        out
    })
}

// ---------- SDF (2D and procedural 3D) ----------
fn sd_box(x: f32, y: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let qx = x.abs() - hx + r;
    let qy = y.abs() - hy + r;
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

/// 2D signed distance (negative inside). shape: 0 circle, 1 box, 2 polygon, 3 ring.
#[allow(clippy::too_many_arguments)]
pub fn sdf2d(shape: u32, x: f32, y: f32, size: f32, aspect: f32, sides: f32, rot: f32, round: f32) -> f32 {
    let (s, c) = (rot * TAU).sin_cos();
    let (rx, ry) = (c * x + s * y, -s * x + c * y);
    match shape {
        0 => (x * x + y * y).sqrt() - size,
        1 => sd_box(rx, ry, size, size * aspect.max(0.01), round.min(size * 0.99)),
        2 => polygon_sdf(x, y, sides, size, rot),
        _ => ((x * x + y * y).sqrt() - size).abs() - (size * 0.2).max(0.005),
    }
}

pub fn sdf_to_value(d: f32, range: f32) -> f32 {
    (0.5 - d / (2.0 * range.max(1e-4))).clamp(0.0, 1.0)
}

pub fn value_to_sdf(v: f32, range: f32) -> f32 {
    (0.5 - v) * 2.0 * range.max(1e-4)
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    if k < 1e-5 {
        return a.min(b);
    }
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b * (1.0 - h) + a * h - k * h * (1.0 - h)
}

/// op: 0 union, 1 subtract (a - b), 2 intersect. k = smoothing radius.
pub fn sdf_combine(op: u32, da: f32, db: f32, k: f32) -> f32 {
    match op {
        0 => smin(da, db, k),
        1 => -smin(-da, db, k),
        _ => -smin(-da, -db, k),
    }
}

fn map3(shape: u32, x: f32, y: f32, z: f32, s: f32, r: f32) -> f32 {
    match shape {
        0 => (x * x + y * y + z * z).sqrt() - s,
        1 => {
            let q = [x.abs() - s + r, y.abs() - s + r, z.abs() - s * 0.8 + r];
            let o = (q[0].max(0.0).powi(2) + q[1].max(0.0).powi(2) + q[2].max(0.0).powi(2)).sqrt();
            o + q[0].max(q[1]).max(q[2]).min(0.0) - r
        }
        2 => {
            let a = ((x * x + y * y).sqrt() - s * 0.7, z);
            (a.0 * a.0 + a.1 * a.1).sqrt() - s * 0.3
        }
        _ => {
            let d = (((x * x + y * y).sqrt() - s), z.abs() - s);
            d.0.max(d.1).min(0.0) + (d.0.max(0.0).powi(2) + d.1.max(0.0).powi(2)).sqrt()
        }
    }
}

/// Height of a procedural 3D SDF form, ray-marched from the top. Domain-repeated and tileable.
#[allow(clippy::too_many_arguments)]
pub fn sdf_form(u: f32, v: f32, shape: u32, size: f32, repeat: f32, round: f32, scatter: f32, seed: u32, hscale: f32) -> f32 {
    let rep = repeat.round().clamp(1.0, 32.0);
    let (cu, cv) = (u * rep, v * rep);
    let (ci, cj) = (cu.floor() as i32, cv.floor() as i32);
    let (x, y) = (cu - ci as f32 - 0.5, cv - cj as f32 - 0.5);
    let (wi, wj) = (ci.rem_euclid(rep as i32), cj.rem_euclid(rep as i32));
    let r1 = crate::noise::rnd(wi, wj, seed);
    let s = (size * (1.0 - scatter * 0.6 * r1)).clamp(0.01, 0.5);
    let zc = match shape {
        0 | 3 => s,
        1 => s * 0.8,
        _ => s * 0.3,
    };
    let top = 2.0f32;
    let mut t = 0.0f32;
    for _ in 0..72 {
        let z = top - t;
        let d = map3(shape, x, y, z - zc, s, round.min(s * 0.9));
        if d < 0.0007 {
            return (z * hscale).clamp(0.0, 1.0);
        }
        t += d.max(0.003);
        if t > top {
            break;
        }
    }
    0.0
}

// =====================================================================================
// Additional nodes
// =====================================================================================
use crate::noise::{rnd, voronoi_ext};

pub fn contrast_invert(t: &Tex, contrast: f32, invert: bool) -> Tex {
    t.map(|p| {
        let mut o = p;
        for c in 0..3 {
            let v = ((p[c] - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
            o[c] = if invert { 1.0 - v } else { v };
        }
        o
    })
}

/// Procedural shape. kind: 0 square, 1 disc, 2 paraboloid, 3 gaussian, 4 diamond, 5 pyramid, 6 half sphere, 7 cone;
/// 100 + n draws pattern n of the scatter nodes' list (`scatter::PATTERN_NAMES`), with `spec` as its Pattern
/// Specific setting. Returns the value at uv (u, v).
#[allow(clippy::too_many_arguments)]
pub fn shape_value(kind: u32, u: f32, v: f32, px: f32, py: f32, sx: f32, sy: f32, rot: f32, tiling: f32, soft: f32, spec: f32) -> f32 {
    let t = tiling.round().clamp(1.0, 32.0);
    let x = ((u - 0.5 - px) * t + 0.5).rem_euclid(1.0) - 0.5;
    let y = ((v - 0.5 - py) * t + 0.5).rem_euclid(1.0) - 0.5;
    let (s, c) = (rot * TAU).sin_cos();
    let (rx, ry) = (c * x + s * y, -s * x + c * y);
    let nx = rx / (sx.max(1e-3) * 0.5);
    let ny = ry / (sy.max(1e-3) * 0.5);
    let r = (nx * nx + ny * ny).sqrt();
    let so = soft.clamp(0.0, 1.0).max(1e-3);
    if kind >= 100 {
        let (lx, ly) = (nx * 0.5 + 0.5, ny * 0.5 + 0.5);
        if !(0.0..=1.0).contains(&lx) || !(0.0..=1.0).contains(&ly) {
            return 0.0;
        }
        return crate::scatter::builtin(kind - 100, lx, ly, spec.clamp(0.0, 1.0));
    }
    match kind {
        0 => 1.0 - sm(1.0 - so, 1.0, nx.abs().max(ny.abs())),
        1 => 1.0 - sm(1.0 - so, 1.0, r),
        2 => (1.0 - r * r).clamp(0.0, 1.0),
        3 => (-(r * 1.8).powi(2)).exp(),
        4 => 1.0 - sm(1.0 - so, 1.0, nx.abs() + ny.abs()),
        5 => (1.0 - nx.abs().max(ny.abs())).clamp(0.0, 1.0),
        6 => (1.0 - r * r).max(0.0).sqrt(),
        _ => (1.0 - r).clamp(0.0, 1.0),
    }
}

pub struct TileP {
    pub nx: f32,
    pub ny: f32,
    pub size: f32,
    pub size_rand: f32,
    pub rot: f32,
    pub rot_rand: f32,
    pub pos_rand: f32,
    pub color: f32,
    pub color_rand: f32,
    pub offset: f32,
    pub seed: u32,
    pub thresh: f32,
    pub default_disc: bool,
    pub brick: bool,
}

/// Grid of pattern instances (Tile Sampler / Tile Generator).
pub fn tile_grid(w: usize, h: usize, pattern: Option<&Tex>, mask: Option<&Tex>, p: &TileP) -> Tex {
    let nx = p.nx.round().clamp(1.0, 64.0) as i32;
    let ny = p.ny.round().clamp(1.0, 64.0) as i32;
    let (fnx, fny) = (nx as f32, ny as f32);
    Tex::from_fn(w, h, |u, v| {
        let (ci, cj) = ((u * fnx).floor() as i32, (v * fny).floor() as i32);
        let mut best = 0.0f32;
        for dj in -1..=1 {
            for di in -1..=1 {
                let (i, j) = (ci + di, cj + dj);
                let (wi, wj) = (i.rem_euclid(nx), j.rem_euclid(ny));
                let shift = if p.brick && (wj & 1) == 1 { p.offset } else { 0.0 };
                let (r1, r2, r3, r4) = (rnd(wi, wj, p.seed), rnd(wi, wj, p.seed.wrapping_add(1)), rnd(wi, wj, p.seed.wrapping_add(2)), rnd(wi, wj, p.seed.wrapping_add(3)));
                let cx = (i as f32 + 0.5 + shift) / fnx + (r1 - 0.5) * p.pos_rand / fnx;
                let cy = (j as f32 + 0.5) / fny + (r2 - 0.5) * p.pos_rand / fny;
                if let Some(m) = mask {
                    if lum(m.sample(cx, cy)) < p.thresh {
                        continue;
                    }
                }
                let s = (p.size * (1.0 + (r3 - 0.5) * 2.0 * p.size_rand)).max(0.01);
                let ang = (p.rot + (r4 - 0.5) * 2.0 * p.rot_rand) * TAU;
                let col = p.color * (1.0 - p.color_rand * rnd(wi, wj, p.seed.wrapping_add(9)));
                let dx = (u - cx + 0.5).rem_euclid(1.0) - 0.5;
                let dy = (v - cy + 0.5).rem_euclid(1.0) - 0.5;
                let (sn, cs) = ang.sin_cos();
                let lx = (cs * dx + sn * dy) * fnx / s + 0.5;
                let ly = (-sn * dx + cs * dy) * fny / s + 0.5;
                if !(0.0..=1.0).contains(&lx) || !(0.0..=1.0).contains(&ly) {
                    continue;
                }
                let val = match pattern {
                    Some(pt) => lum(pt.sample(lx, ly)),
                    None if p.default_disc => 1.0 - sm(0.85, 1.0, (((lx - 0.5) * 2.0).powi(2) + ((ly - 0.5) * 2.0).powi(2)).sqrt()),
                    None => 1.0,
                };
                best = best.max(val * col);
            }
        }
        [best, best, best, 1.0]
    })
}

/// Instances placed on a circle.
#[allow(clippy::too_many_arguments)]
pub fn splatter_circular(w: usize, h: usize, pattern: Option<&Tex>, n: u32, radius: f32, size: f32, size_rand: f32, rot: f32, rot_rand: f32, pos_rand: f32, seed: u32) -> Tex {
    let n = n.clamp(1, 256);
    let inst: Vec<(f32, f32, f32, f32)> = (0..n)
        .map(|k| {
            let i = k as i32;
            let a = k as f32 / n as f32 * TAU;
            let cx = 0.5 + radius * a.cos() + (rnd(i, 1, seed) - 0.5) * pos_rand * 0.2;
            let cy = 0.5 + radius * a.sin() + (rnd(i, 2, seed) - 0.5) * pos_rand * 0.2;
            let s = (size * (1.0 + (rnd(i, 3, seed) - 0.5) * 2.0 * size_rand)).max(0.005);
            let r = (rot + (rnd(i, 4, seed) - 0.5) * 2.0 * rot_rand) * TAU;
            (cx, cy, s, r)
        })
        .collect();
    Tex::from_fn(w, h, |u, v| {
        let mut best = 0.0f32;
        for &(cx, cy, s, r) in &inst {
            let (dx, dy) = (u - cx, v - cy);
            let (sn, cs) = r.sin_cos();
            let (lx, ly) = ((cs * dx + sn * dy) / s + 0.5, (-sn * dx + cs * dy) / s + 0.5);
            if !(0.0..=1.0).contains(&lx) || !(0.0..=1.0).contains(&ly) {
                continue;
            }
            let val = match pattern {
                Some(p) => lum(p.sample(lx, ly)),
                None => 1.0 - sm(0.85, 1.0, (((lx - 0.5) * 2.0).powi(2) + ((ly - 0.5) * 2.0).powi(2)).sqrt()),
            };
            best = best.max(val);
        }
        [best, best, best, 1.0]
    })
}

// ---------------- flood fill family ----------------
pub fn flood_fill(t: &Tex, thresh: f32) -> Tex {
    let (w, h) = (t.w, t.h);
    let n = w * h;
    let mask: Vec<bool> = t.px.iter().map(|p| lum(*p) > thresh).collect();
    let mut label = vec![0u32; n];
    // position of every pixel relative to the region's first pixel, measured WITHOUT wrapping, so a region that
    // crosses a tile border still gets one continuous gradient
    let mut rel = vec![(0i32, 0i32); n];
    let mut bbox: Vec<[i32; 4]> = vec![[0; 4]];
    let mut stack: Vec<usize> = Vec::new();
    let mut count = 0u32;
    for s in 0..n {
        if !mask[s] || label[s] != 0 {
            continue;
        }
        count += 1;
        label[s] = count;
        stack.push(s);
        let mut bb = [0i32, 0, 0, 0];
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            let (rx, ry) = rel[i];
            bb = [bb[0].min(rx), bb[1].min(ry), bb[2].max(rx), bb[3].max(ry)];
            for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                let nx = (x as i64 + dx).rem_euclid(w as i64) as usize;
                let ny = (y as i64 + dy).rem_euclid(h as i64) as usize;
                let j = ny * w + nx;
                if mask[j] && label[j] == 0 {
                    label[j] = count;
                    rel[j] = (rx + dx as i32, ry + dy as i32);
                    stack.push(j);
                }
            }
        }
        bbox.push(bb);
    }
    let px = (0..n)
        .map(|i| {
            if label[i] == 0 {
                return [0.0, 0.0, 0.0, 1.0];
            }
            let b = bbox[label[i] as usize];
            let (rx, ry) = rel[i];
            let gx = ((rx - b[0]) as f32 + 0.5) / ((b[2] - b[0] + 1) as f32);
            let gy = ((ry - b[1]) as f32 + 0.5) / ((b[3] - b[1] + 1) as f32);
            [0.1 + 0.9 * rnd(label[i] as i32, 3, 17), gx, gy, 1.0]
        })
        .collect();
    Tex { w, h, px }
}

fn region_id(p: [f32; 4]) -> Option<i32> {
    if p[0] == 0.0 && p[1] == 0.0 && p[2] == 0.0 {
        None
    } else {
        Some((p[0] * 100_000.0) as i32)
    }
}

/// Per-region geometry of a flood-fill result (regions are 4-connected pixels with the same region id; they may
/// wrap around the tile border). Returns the region index of every pixel (0 = none) and, per region, the centre in
/// uv (wrapped into 0..1), the bounding-box size in uv, and the pixel count. Index 0 of the vector is the background.
pub fn flood_regions(t: &Tex) -> (Vec<u32>, Vec<([f32; 2], [f32; 2], u32)>) {
    let (w, h) = (t.w, t.h);
    let n = w * h;
    let ids: Vec<Option<i32>> = t.px.iter().map(|p| region_id(*p)).collect();
    let mut label = vec![0u32; n];
    let mut stats = vec![([0.0f32; 2], [0.0f32; 2], 0u32)];
    let mut stack = Vec::new();
    for s in 0..n {
        if ids[s].is_none() || label[s] != 0 {
            continue;
        }
        let k = stats.len() as u32;
        label[s] = k;
        stack.push((s, 0i32, 0i32));
        let (mut x0, mut x1, mut y0, mut y1, mut count) = (0i32, 0i32, 0i32, 0i32, 0u32);
        let (mut sx, mut sy) = (0i64, 0i64);
        while let Some((i, rx, ry)) = stack.pop() {
            count += 1;
            x0 = x0.min(rx);
            x1 = x1.max(rx);
            y0 = y0.min(ry);
            y1 = y1.max(ry);
            sx += rx as i64;
            sy += ry as i64;
            let (x, y) = (i % w, i / w);
            for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                let j = (y as i64 + dy).rem_euclid(h as i64) as usize * w + (x as i64 + dx).rem_euclid(w as i64) as usize;
                if label[j] == 0 && ids[j] == ids[s] {
                    label[j] = k;
                    stack.push((j, rx + dx as i32, ry + dy as i32));
                }
            }
        }
        // centre of the bounding box relative to the first pixel, moved back into the tile
        let _ = (sx, sy);
        let cx = ((s % w) as f32 + 0.5 + (x0 + x1) as f32 * 0.5).rem_euclid(w as f32) / w as f32;
        let cy = ((s / w) as f32 + 0.5 + (y0 + y1) as f32 * 0.5).rem_euclid(h as f32) / h as f32;
        stats.push(([cx, cy], [(x1 - x0 + 1) as f32 / w as f32, (y1 - y0 + 1) as f32 / h as f32], count));
    }
    (label, stats)
}

/// Flood Fill to Index: every region gets its running number as a gray level (index / region count).
pub fn flood_fill_to_index(t: &Tex) -> Tex {
    let (label, stats) = flood_regions(t);
    let div = (stats.len().max(2) - 1) as f32;
    Tex { w: t.w, h: t.h, px: label.iter().map(|&l| if l == 0 { [0.0, 0.0, 0.0, 1.0] } else { let v = l as f32 / div; [v, v, v, 1.0] }).collect() }
}

/// Flood Fill to Position (`size == false`) or to Bounding Box Size (`size == true`): the region's centre or extent
/// in uv as red/green.
pub fn flood_fill_to_geometry(t: &Tex, size: bool) -> Tex {
    let (label, stats) = flood_regions(t);
    Tex {
        w: t.w,
        h: t.h,
        px: label.iter().map(|&l| if l == 0 { [0.0, 0.0, 0.0, 1.0] } else { let s = &stats[l as usize]; let v = if size { s.1 } else { s.0 }; [v[0], v[1], 0.0, 1.0] }).collect(),
    }
}

pub fn flood_fill_random(t: &Tex, seed: u32, lo: f32, hi: f32, contrast: f32) -> Tex {
    let k = 1.0 + contrast * 4.0;
    t.map(|p| match region_id(p) {
        None => [0.0, 0.0, 0.0, 1.0],
        Some(id) => {
            let r = (0.5 + (rnd(id, 5, seed) - 0.5) * k).clamp(0.0, 1.0);
            let g = lo + (hi - lo) * r;
            [g, g, g, 1.0]
        }
    })
}

pub fn flood_fill_color(t: &Tex, seed: u32) -> Tex {
    t.map(|p| match region_id(p) {
        None => [0.0, 0.0, 0.0, 1.0],
        Some(id) => [0.2 + 0.8 * rnd(id, 6, seed), 0.2 + 0.8 * rnd(id, 7, seed), 0.2 + 0.8 * rnd(id, 8, seed), 1.0],
    })
}

pub fn flood_fill_gradient(t: &Tex, angle: f32, variation: f32, seed: u32) -> Tex {
    t.map(|p| match region_id(p) {
        None => [0.0, 0.0, 0.0, 1.0],
        Some(id) => {
            let a = angle * TAU + (rnd(id, 9, seed) - 0.5) * variation * TAU;
            let (s, c) = a.sin_cos();
            let g = (0.5 + ((p[1] - 0.5) * c + (p[2] - 0.5) * s) / (c.abs() + s.abs()).max(1e-3)).clamp(0.0, 1.0);
            [g, g, g, 1.0]
        }
    })
}

// ---------------- distance / bevel ----------------
/// Signed distance in pixels from every pixel centre to the edge of the shape `lum > thr`: positive inside, negative
/// outside. The edge is placed between pixels by the grey levels (where the values cross `thr`), so slanted and curved
/// edges give smooth distances instead of stair steps; distances are Euclidean and wrap around the tile border.
/// Without any edge (all inside or all outside) every pixel gets +/-1e9.
pub fn signed_distance(t: &Tex, thr: f32) -> Vec<f32> {
    let (w, h) = (t.w, t.h);
    let n = w * h;
    let g: Vec<f32> = t.px.iter().map(|p| lum(*p)).collect();
    let inside: Vec<bool> = g.iter().map(|v| *v > thr).collect();
    let at = |x: i64, y: i64| y.rem_euclid(h as i64) as usize * w + x.rem_euclid(w as i64) as usize;
    // offset from each pixel centre to the nearest edge point found so far
    let far = [1e9f32, 1e9f32];
    let mut off = vec![far; n];
    let len = |o: [f32; 2]| o[0] * o[0] + o[1] * o[1];
    let mut any = false;
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let i = at(x, y);
            for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                let j = at(x + dx, y + dy);
                if inside[j] == inside[i] {
                    continue;
                }
                // where the values cross the threshold on the way to the neighbour
                let d = g[j] - g[i];
                let f = if d.abs() < 1e-6 { 0.5 } else { ((thr - g[i]) / d).clamp(0.0, 1.0) };
                let o = [dx as f32 * f, dy as f32 * f];
                if len(o) < len(off[i]) {
                    off[i] = o;
                }
                any = true;
            }
        }
    }
    if !any {
        return inside.iter().map(|&m| if m { 1e9 } else { -1e9 }).collect();
    }
    // propagate the nearest edge points (two sweeps each way, 8 neighbours)
    let fwd = [(-1i64, 0i64), (0, -1), (-1, -1), (1, -1)];
    let bwd = [(1i64, 0i64), (0, 1), (1, 1), (-1, 1)];
    for _ in 0..2 {
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                let i = at(x, y);
                for (dx, dy) in fwd {
                    let o = off[at(x + dx, y + dy)];
                    let c = [o[0] + dx as f32, o[1] + dy as f32];
                    if len(c) < len(off[i]) {
                        off[i] = c;
                    }
                }
            }
            for x in (0..w as i64).rev() {
                let i = at(x, y);
                let (dx, dy) = (1i64, 0i64);
                let o = off[at(x + dx, y + dy)];
                let c = [o[0] + dx as f32, o[1] + dy as f32];
                if len(c) < len(off[i]) {
                    off[i] = c;
                }
            }
        }
        for y in (0..h as i64).rev() {
            for x in (0..w as i64).rev() {
                let i = at(x, y);
                for (dx, dy) in bwd {
                    let o = off[at(x + dx, y + dy)];
                    let c = [o[0] + dx as f32, o[1] + dy as f32];
                    if len(c) < len(off[i]) {
                        off[i] = c;
                    }
                }
            }
            for x in 0..w as i64 {
                let i = at(x, y);
                let (dx, dy) = (-1i64, 0i64);
                let o = off[at(x + dx, y + dy)];
                let c = [o[0] + dx as f32, o[1] + dy as f32];
                if len(c) < len(off[i]) {
                    off[i] = c;
                }
            }
        }
    }
    off.iter().zip(&inside).map(|(o, &m)| if m { len(*o).sqrt() } else { -len(*o).sqrt() }).collect()
}

pub fn bevel(t: &Tex, dist_px: f32, smoothing: f32, thr: f32) -> Tex {
    bevel_ex(t, dist_px, smoothing, thr, false)
}

/// `round` = circular (quarter-circle) profile, otherwise the linear ramp of an angular bevel.
/// Bevel of the shape `lum > thr`: 0 outside, rising to 1 at `dist_px` inside. A negative distance grows the bevel
/// outwards instead (1 on the shape, falling to 0 at `-dist_px` outside it).
pub fn bevel_ex(t: &Tex, dist_px: f32, smoothing: f32, thr: f32, round: bool) -> Tex {
    let sd = signed_distance(t, thr);
    let outward = dist_px < 0.0;
    let reach = dist_px.abs().max(1.0);
    let px = sd
        .iter()
        .map(|d| {
            let tt = if outward {
                if *d > 0.0 {
                    1.0
                } else {
                    1.0 - (-d / reach).clamp(0.0, 1.0)
                }
            } else if *d <= 0.0 {
                return [0.0, 0.0, 0.0, 1.0];
            } else {
                (d / reach).clamp(0.0, 1.0)
            };
            let tt = if round { (1.0 - (1.0 - tt) * (1.0 - tt)).sqrt() } else { tt };
            let g = tt + (tt * tt * (3.0 - 2.0 * tt) - tt) * smoothing.clamp(0.0, 1.0);
            [g, g, g, 1.0]
        })
        .collect();
    Tex { w: t.w, h: t.h, px }
}

/// mode: 0 inside (0 at the edge, 1 at `range_px` inside), 1 the format's Distance (1 on the shape, falling to 0 at
/// `range_px` outside it), 2 both (0.5 = edge).
pub fn distance(t: &Tex, range_px: f32, thr: f32, mode: u32) -> Tex {
    let sd = signed_distance(t, thr);
    let r = range_px.max(1e-3);
    let px = sd
        .iter()
        .map(|d| {
            let g = match mode {
                0 => (d / r).clamp(0.0, 1.0),
                1 => 1.0 - (-d / r).clamp(0.0, 1.0),
                _ => (0.5 + d / r * 0.5).clamp(0.0, 1.0),
            };
            [g, g, g, 1.0]
        })
        .collect();
    Tex { w: t.w, h: t.h, px }
}

// ---------------- warps ----------------
pub fn vector_warp(t: &Tex, vecmap: &Tex, intensity: f32) -> Tex {
    Tex::from_fn(t.w, t.h, |u, v| {
        let m = vecmap.sample(u, v);
        t.sample_cubic(u + (m[0] - 0.5) * 2.0 * intensity, v + (m[1] - 0.5) * 2.0 * intensity)
    })
}

/// Vector Warp with a Vector Format option: `directx` reads the green channel as pointing down.
pub fn vector_warp_ex(t: &Tex, vecmap: &Tex, intensity: f32, directx: bool) -> Tex {
    let gs = if directx { -1.0 } else { 1.0 };
    Tex::from_fn(t.w, t.h, |u, v| {
        let m = vecmap.sample(u, v);
        t.sample_cubic(u + (m[0] - 0.5) * 2.0 * intensity, v + gs * (m[1] - 0.5) * 2.0 * intensity)
    })
}

/// mode: 0 blur (average), 1 min, 2 max.
pub fn slope_blur(t: &Tex, slope: &Tex, samples: u32, intensity: f32, mode: u32) -> Tex {
    let (sw, sh) = (slope.w, slope.h);
    let gray: Vec<f32> = slope.px.iter().map(|p| lum(*p)).collect();
    // slope direction (central differences) as a two-channel plane
    let mut gv = vec![[0.0f32; 2]; sw * sh];
    {
        let rows_per = crate::pool::rows_per_piece(sw, sh);
        let gray = &gray;
        let my = crate::tex::my_job();
        crate::pool::for_chunks(&mut gv, rows_per * sw, &|ci, chunk| {
            for (ry, row) in chunk.chunks_mut(sw).enumerate() {
                if crate::tex::stale(my) {
                    return;
                }
                let y = ci * rows_per + ry;
                let (ym, yp) = (if y == 0 { sh - 1 } else { y - 1 }, if y + 1 == sh { 0 } else { y + 1 });
                for (x, dst) in row.iter_mut().enumerate() {
                    let (xm, xp) = (if x == 0 { sw - 1 } else { x - 1 }, if x + 1 == sw { 0 } else { x + 1 });
                    *dst = [gray[y * sw + xp] - gray[y * sw + xm], gray[yp * sw + x] - gray[ym * sw + x]];
                }
            }
        });
    }
    let n = samples.clamp(1, 64);
    let step = intensity / n as f32;
    let gain = slope.w as f32 * 0.5;
    let nf = n as f32;
    // bilinear lookups on plain planes (wrapped)
    #[inline]
    fn bil<const C: usize>(d: &[[f32; C]], w: usize, h: usize, u: f32, v: f32) -> [f32; C] {
        let u = if u.is_finite() { u } else { 0.0 };
        let v = if v.is_finite() { v } else { 0.0 };
        let (x, y) = (u * w as f32 - 0.5, v * h as f32 - 0.5);
        let (x0, y0) = (crate::tex::ffloor(x), crate::tex::ffloor(y));
        let (fx, fy) = (x - x0, y - y0);
        let (wi, hi) = (w as i64, h as i64);
        let wrap = |i: i64, n: i64| if i >= 0 && i < n { i } else { i.rem_euclid(n) };
        let (xa, ya) = (wrap(x0 as i64, wi), wrap(y0 as i64, hi));
        let xb = if xa + 1 == wi { 0 } else { xa + 1 };
        let yb = if ya + 1 == hi { 0 } else { ya + 1 };
        let (r0, r1) = ((ya * wi) as usize, (yb * wi) as usize);
        let (a, b, c, e) = (d[r0 + xa as usize], d[r0 + xb as usize], d[r1 + xa as usize], d[r1 + xb as usize]);
        let mut o = [0.0; C];
        for i in 0..C {
            let top = a[i] + (b[i] - a[i]) * fx;
            let bot = c[i] + (e[i] - c[i]) * fx;
            o[i] = top + (bot - top) * fy;
        }
        o
    }
    let walk = |u: f32, v: f32, mut visit: &mut dyn FnMut(f32, f32)| {
        let (mut pu, mut pv) = (u, v);
        for _ in 0..n {
            visit(pu, pv);
            let gvv = bil::<2>(&gv, sw, sh, pu, pv);
            let l = (gvv[0] * gvv[0] + gvv[1] * gvv[1]).sqrt().max(1e-6);
            let k = (l * gain).min(1.0) * step / l;
            pu += gvv[0] * k;
            pv += gvv[1] * k;
        }
        let _ = &mut visit;
    };
    let is_gray = t.px.iter().all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 1.0);
    if is_gray {
        let plane: Vec<[f32; 1]> = t.px.iter().map(|p| [p[0]]).collect();
        return Tex::from_fn(t.w, t.h, |u, v| {
            let mut acc = if mode == 1 { 1.0f32 } else { 0.0 };
            walk(u, v, &mut |pu, pv| {
                let p = bil::<1>(&plane, t.w, t.h, pu, pv)[0];
                acc = match mode {
                    1 => acc.min(p),
                    2 => acc.max(p),
                    _ => acc + p / nf,
                };
            });
            [acc, acc, acc, 1.0]
        });
    }
    Tex::from_fn(t.w, t.h, |u, v| {
        let mut acc = if mode == 1 { [1.0f32; 4] } else { [0.0f32; 4] };
        walk(u, v, &mut |pu, pv| {
            let p = bil::<4>(&t.px, t.w, t.h, pu, pv);
            for c in 0..4 {
                acc[c] = match mode {
                    1 => acc[c].min(p[c]),
                    2 => acc[c].max(p[c]),
                    _ => acc[c] + p[c] / nf,
                };
            }
        });
        acc
    })
}

pub fn swirl(t: &Tex, amount: f32, radius: f32) -> Tex {
    let rad = radius.max(1e-3);
    Tex::from_fn(t.w, t.h, |u, v| {
        let (x, y) = (wrapd(u - 0.5), wrapd(v - 0.5));
        let r = (x * x + y * y).sqrt();
        let k = (1.0 - r / rad).clamp(0.0, 1.0);
        let a = amount * k * k * TAU;
        let (s, c) = a.sin_cos();
        t.sample(0.5 + c * x - s * y, 0.5 + s * x + c * y)
    })
}

pub fn highpass(t: &Tex, radius: f32) -> Tex {
    let b = blur(t, radius);
    let mut o = t.clone();
    for (i, p) in o.px.iter_mut().enumerate() {
        for c in 0..3 {
            p[c] = (t.px[i][c] - b.px[i][c] + 0.5).clamp(0.0, 1.0);
        }
    }
    o
}

// ---------------- colour / mask helpers ----------------
pub fn color_match(t: &Tex, reference: &Tex, strength: f32) -> Tex {
    let stats = |x: &Tex| -> ([f32; 3], [f32; 3]) {
        let n = x.px.len().max(1) as f32;
        let mut m = [0.0f32; 3];
        for p in &x.px {
            for c in 0..3 {
                m[c] += p[c] / n;
            }
        }
        let mut sd = [0.0f32; 3];
        for p in &x.px {
            for c in 0..3 {
                sd[c] += (p[c] - m[c]).powi(2) / n;
            }
        }
        (m, [sd[0].sqrt(), sd[1].sqrt(), sd[2].sqrt()])
    };
    let ((m1, s1), (m2, s2)) = (stats(t), stats(reference));
    t.map(|p| {
        let mut o = p;
        for c in 0..3 {
            let matched = (p[c] - m1[c]) * (s2[c] / s1[c].max(1e-4)) + m2[c];
            o[c] = (p[c] + (matched - p[c]) * strength.clamp(0.0, 1.0)).clamp(0.0, 1.0);
        }
        o
    })
}

/// Curvature from a tangent-space normal map (divergence of the normal), smoothed by `smooth_px`.
pub fn curvature_from_normal(n: &Tex, smooth_px: f32, intensity: f32) -> Tex {
    let c = Tex::from_fn(n.w, n.h, |u, v| {
        let (x, y) = ((u * n.w as f32) as i64, (v * n.h as f32) as i64);
        let div = (n.at(x + 1, y)[0] - n.at(x - 1, y)[0]) - (n.at(x, y + 1)[1] - n.at(x, y - 1)[1]);
        let g = (0.5 + div * intensity).clamp(0.0, 1.0);
        [g, g, g, 1.0]
    });
    blur(&c, smooth_px)
}

/// Returns (height, blend factor).
pub fn height_blend(bg: &Tex, fg: &Tex, mask: Option<&Tex>, opacity: f32, contrast: f32) -> (Tex, Tex) {
    let mut h = bg.clone();
    let mut f = bg.clone();
    for i in 0..h.px.len() {
        let m = mask.and_then(|m| m.px.get(i)).map(|p| lum(*p)).unwrap_or(1.0) * opacity;
        let (b, g) = (lum(bg.px[i]), lum(fg.px.get(i).copied().unwrap_or([0.0; 4])));
        let score = (g + (m - 0.5) * 2.0) - b;
        let k = (0.5 + score * contrast.max(0.05) * 2.0).clamp(0.0, 1.0) * sm(0.0, 0.08, m);
        let out = b + (g - b) * k;
        h.px[i] = [out, out, out, 1.0];
        f.px[i] = [k, k, k, 1.0];
    }
    (h, f)
}

pub fn make_tile_photo(t: &Tex, width: f32) -> Tex {
    make_tile_photo_ex(t, width, width)
}

/// Separate horizontal and vertical transition reach (Mask Size H / V).
pub fn make_tile_photo_ex(t: &Tex, width_h: f32, width_v: f32) -> Tex {
    let (wh, wv) = (width_h.clamp(0.1, 1.0), width_v.clamp(0.1, 1.0));
    let a = |x: f32, w: f32| sm(0.0, w, 1.0 - (2.0 * x - 1.0).abs());
    Tex::from_fn(t.w, t.h, |u, v| {
        let (au, av) = (a(u, wh), a(v, wv));
        let s = |du: f32, dv: f32| t.sample((u + du).rem_euclid(1.0), (v + dv).rem_euclid(1.0));
        let (c00, c10, c01, c11) = (s(0.0, 0.0), s(0.5, 0.0), s(0.0, 0.5), s(0.5, 0.5));
        let (w00, w10, w01, w11) = (au * av, (1.0 - au) * av, au * (1.0 - av), (1.0 - au) * (1.0 - av));
        let mut o = [0.0; 4];
        for c in 0..4 {
            o[c] = c00[c] * w00 + c10[c] * w10 + c01[c] * w01 + c11[c] * w11;
        }
        o
    })
}

pub fn make_tile_patch(t: &Tex, patches: f32, softness: f32, seed: u32) -> Tex {
    let p = patches.round().clamp(2.0, 32.0) as i32;
    let soft = softness.clamp(0.01, 1.0) * 0.3;
    Tex::from_fn(t.w, t.h, |u, v| {
        let vr = voronoi_ext(u, v, p, p, seed, 1.0);
        let off = |id: f32| ((id * 10_000.0) as i32, rnd((id * 10_000.0) as i32, 1, seed), rnd((id * 10_000.0) as i32, 2, seed));
        let (a, b) = (off(vr.id1), off(vr.id2));
        let (s1, s2) = (t.sample(u + a.1, v + a.2), t.sample(u + b.1, v + b.2));
        let k = 0.5 + 0.5 * sm(0.0, soft, vr.f2 - vr.f1);
        let mut o = [0.0; 4];
        for c in 0..4 {
            o[c] = s2[c] + (s1[c] - s2[c]) * k;
        }
        o
    })
}

/// Seamless stripe direction. A stripe pattern `frac(a*u + b*v)` repeats exactly across the unit tile
/// only when (a, b) are integers, so an arbitrary angle cannot be exact. This returns the integer wave
/// vector whose direction is closest to `angle_turns` while keeping the stripe count within about
/// -40%/+60% of `count` (angle error is weighted most; count error is penalised softly).
pub fn tileable_wave(count: f32, angle_turns: f32) -> (i32, i32) {
    let c = (count as f64).clamp(1.0, 8192.0);
    let th = (angle_turns as f64) * std::f64::consts::TAU;
    let (st, ct) = th.sin_cos();
    let (lo, hi) = ((c * 0.6).max(1.0), (c * 1.6).max(1.5));
    let horizontal = ct.abs() >= st.abs();
    let prim = if horizontal { ct.abs() } else { st.abs() };
    let m = (hi * prim).ceil() as i64 + 2;
    let mut best = (f64::MAX, (c.round() as i32).max(1), 0i32);
    for p in -m..=m {
        let sec = if horizontal { p as f64 * st / ct } else { p as f64 * ct / st };
        let base = sec.floor() as i64;
        for s in (base - 1)..=(base + 2) {
            let (a, b) = if horizontal { (p, s) } else { (s, p) };
            if a == 0 && b == 0 {
                continue;
            }
            let len = ((a * a + b * b) as f64).sqrt();
            if len < lo || len > hi {
                continue;
            }
            let d = (b as f64).atan2(a as f64) - th;
            let dth = d.sin().atan2(d.cos()).abs();
            let score = dth + 0.5 * (len / c).ln().abs();
            if score < best.0 {
                best = (score, a as i32, b as i32);
            }
        }
    }
    (best.1, best.2)
}

/// Resample a texture to `size` x `size` (wrapped). Shrinking by a whole factor averages the covered texels exactly
/// (no aliasing); anything else is bilinear.
pub fn resize(t: &Tex, size: usize) -> Tex {
    resize_to(t, size, size)
}

/// Resample to `w` x `h`: exact box averaging for whole-number reductions, bilinear (wrapped) otherwise.
pub fn resize_to(t: &Tex, w: usize, h: usize) -> Tex {
    let (w, h) = (w.max(1), h.max(1));
    if t.w == w && t.h == h {
        return t.clone();
    }
    if t.w >= w && t.h >= h && t.w % w == 0 && t.h % h == 0 {
        let (fx, fy) = (t.w / w, t.h / h);
        let inv = 1.0 / (fx * fy) as f32;
        return Tex::from_fn(w, h, |u, v| {
            let (x0, y0) = (((u * w as f32) as usize).min(w - 1) * fx, ((v * h as f32) as usize).min(h - 1) * fy);
            let mut a = [0.0f32; 4];
            for y in y0..y0 + fy {
                for x in x0..x0 + fx {
                    let p = t.px[y * t.w + x];
                    for c in 0..4 {
                        a[c] += p[c];
                    }
                }
            }
            [a[0] * inv, a[1] * inv, a[2] * inv, a[3] * inv]
        });
    }
    Tex::from_fn(w, h, |u, v| t.sample(u, v))
}

// ---------- gradient map with alpha, addressing and smoothness ----------
/// Gradient map. `stops` are (position, rgb, alpha). `interp`: 0 linear, 1 smooth (`smoothness` blends linear and
/// smoothstep), 2 step, 3 smoother. `repeat` wraps the input instead of clamping it; `use_alpha` writes the
/// gradient's alpha into the result (otherwise the input's alpha is kept).
pub fn colorize_ex(t: &Tex, stops: &[(f32, [f32; 3], f32)], interp: u32, smoothness: f32, repeat: bool, use_alpha: bool) -> Tex {
    let mut st: Vec<(f32, [f32; 3], f32)> = stops.to_vec();
    st.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let sm = smoothness.clamp(0.0, 1.0);
    t.map(|p| {
        let mut g = lum(p);
        g = if repeat { g.rem_euclid(1.0) } else { g.clamp(0.0, 1.0) };
        let out = |c: [f32; 3], a: f32| [c[0], c[1], c[2], if use_alpha { a } else { p[3] }];
        if st.is_empty() {
            return [g, g, g, p[3]];
        }
        if g <= st[0].0 {
            return out(st[0].1, st[0].2);
        }
        for i in 1..st.len() {
            if g <= st[i].0 {
                let (a, b) = (st[i - 1], st[i]);
                let mut f = (g - a.0) / (b.0 - a.0).max(1e-9);
                f = match interp {
                    1 => f + (f * f * (3.0 - 2.0 * f) - f) * sm,
                    2 => 0.0,
                    3 => f * f * f * (f * (f * 6.0 - 15.0) + 10.0),
                    _ => f,
                };
                let l = |x: f32, y: f32| x + (y - x) * f;
                return out([l(a.1[0], b.1[0]), l(a.1[1], b.1[1]), l(a.1[2], b.1[2])], l(a.2, b.2));
            }
        }
        let e = st[st.len() - 1];
        out(e.1, e.2)
    })
}

// ---------- colour match (colour swap in OKLCh) ----------
fn to_lin(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}
fn from_lin(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}
/// sRGB -> (L, C, h in turns)
pub fn rgb_to_lch(c: [f32; 3]) -> [f32; 3] {
    let (r, g, b) = (to_lin(c[0]), to_lin(c[1]), to_lin(c[2]));
    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    let ll = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
    let aa = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
    let bb = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;
    [ll, (aa * aa + bb * bb).sqrt(), (bb.atan2(aa) / std::f32::consts::TAU).rem_euclid(1.0)]
}
pub fn lch_to_rgb(c: [f32; 3]) -> [f32; 3] {
    let (aa, bb) = ((c[2] * std::f32::consts::TAU).cos() * c[1], (c[2] * std::f32::consts::TAU).sin() * c[1]);
    let l = (c[0] + 0.396_337_78 * aa + 0.215_803_76 * bb).powi(3);
    let m = (c[0] - 0.105_561_346 * aa - 0.063_854_17 * bb).powi(3);
    let s = (c[0] - 0.089_484_18 * aa - 1.291_485_5 * bb).powi(3);
    [
        from_lin(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s),
        from_lin(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_4 * s),
        from_lin(-0.004_196_086 * l - 0.703_418_6 * m + 1.707_614_7 * s),
    ]
}
pub fn mean_rgb(t: &Tex) -> [f32; 3] {
    let n = t.px.len().max(1) as f32;
    let mut m = [0.0f32; 3];
    for p in &t.px {
        for c in 0..3 {
            m[c] += p[c] / n;
        }
    }
    m
}

/// Parameters of the colour swap.
pub struct ColorSwap {
    pub source: [f32; 3],
    pub target: [f32; 3],
    pub hue_shift: f32,     // turns
    pub chroma_mul: f32,
    pub luma_mul: f32,
    pub use_mask: bool,
    pub hue_range: f32,     // turns
    pub chroma_range: f32,
    pub luma_range: f32,
    pub smooth: f32,
    pub strength: f32,
}

/// Moves every colour near `source` towards `target` (hue rotated, chroma and luma scaled by the ratio between the two,
/// each times its variation) and returns (result, mask). The mask is how much each pixel was selected.
pub fn color_swap(t: &Tex, c: &ColorSwap) -> (Tex, Tex) {
    let (s, g) = (rgb_to_lch(c.source), rgb_to_lch(c.target));
    let dh = g[2] - s[2] + c.hue_shift;
    let cr = (g[1] / s[1].max(1e-3)).clamp(0.0, 8.0) * c.chroma_mul;
    let lr = (g[0] / s[0].max(1e-3)).clamp(0.0, 8.0).powf(1.0) * c.luma_mul;
    // the chroma ratio is meaningless for a grey source: fall back to adding the target's chroma
    let grey = s[1] < 0.01;
    let wrap = |d: f32| {
        let d = d.rem_euclid(1.0);
        d.min(1.0 - d)
    };
    let calc = |p: [f32; 4]| -> ([f32; 4], f32) {
        let q = rgb_to_lch([p[0], p[1], p[2]]);
        let w = if c.use_mask {
            let d = (wrap(q[2] - s[2]) / c.hue_range.max(1e-4)).max((q[1] - s[1]).abs() / c.chroma_range.max(1e-4)).max((q[0] - s[0]).abs() / c.luma_range.max(1e-4));
            1.0 - ((d - 1.0) / c.smooth.max(0.02)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let w = (w * c.strength).clamp(0.0, 1.0);
        let nc = if grey { q[1] + g[1] * c.chroma_mul } else { q[1] * cr };
        let nl = (q[0] * lr).clamp(0.0, 1.2);
        let res = lch_to_rgb([nl, nc.max(0.0), (q[2] + dh).rem_euclid(1.0)]);
        ([p[0] + (res[0] - p[0]) * w, p[1] + (res[1] - p[1]) * w, p[2] + (res[2] - p[2]) * w, p[3]], w)
    };
    (t.map(|p| calc(p).0), t.map(|p| {
        let w = calc(p).1;
        [w, w, w, 1.0]
    }))
}

// ---------- horizon based ambient occlusion ----------
/// Horizon-based AO of a height map. `radius_uv` is the search distance in image units (0..1), `height_scale` how tall
/// a height of 1.0 is in the same units (screen-relative: the "height depth"; world units: height cm / surface cm),
/// `dirs` the number of directions (4 / 8 / 16). Wraps at the borders, so tileable inputs give tileable output.
pub fn hbao(h: &Tex, radius_uv: f32, height_scale: f32, dirs: u32, intensity: f32) -> Tex {
    let g = to_gray(h);
    let (w, hh) = (g.w, g.h);
    // the height as one channel: every step samples a quarter of the memory (same values as sampling the texture)
    let p = crate::tex::Plane::of(&g, 0);
    drop(g);
    let dirs = dirs.clamp(2, 64) as usize;
    let steps = 14usize;
    // Every pixel looks at the same offsets, so the whole-pixel shift and the bilinear weights of each (direction,
    // step) are the same for all pixels: they are worked out once here, and a row of pixels is then handled with
    // straight, contiguous reads that the compiler turns into SIMD code.
    let mut taps: Vec<HbaoTap> = Vec::with_capacity(dirs * steps);
    for k in 0..dirs {
        let a = k as f32 / dirs as f32 * std::f32::consts::TAU;
        let (dx, dy) = (a.cos(), a.sin());
        for j in 1..=steps {
            let t = j as f32 / steps as f32;
            let d = radius_uv * t;
            let (ox, oy) = (dx * d * w as f32, dy * d * hh as f32);
            let (fxl, fyl) = (ox.floor(), oy.floor());
            taps.push(HbaoTap {
                ix: (fxl as i64).rem_euclid(w as i64) as usize,
                iy: (fyl as i64).rem_euclid(hh as i64) as usize,
                fx: ox - fxl,
                fy: oy - fyl,
                d2: d * d,
                falloff: 1.0 - t * t,
            });
        }
    }
    let rows = crate::tex::par_map(hh, |y| hbao_row(&p, &taps, dirs, steps, y, height_scale, intensity));
    Tex { w, h: hh, px: rows.concat() }
}

/// One (direction, step) of the horizon search: the whole-pixel offset (already wrapped into the image), the
/// fractional part that sets the bilinear weights, and the step's squared distance and falloff.
struct HbaoTap {
    ix: usize,
    iy: usize,
    fx: f32,
    fy: f32,
    d2: f32,
    falloff: f32,
}

/// Horizon of one run of pixels: `best[i]` takes the larger of itself and the slope seen through this tap. All the
/// slices have the same length; `a b / c d` are the four texels the bilinear read blends.
#[inline(always)]
fn hbao_run(best: &mut [f32], h0: &[f32], a: &[f32], b: &[f32], c: &[f32], d: &[f32], tap: &HbaoTap, scale: f32) {
    let n = best.len();
    let (h0, a, b, c, d) = (&h0[..n], &a[..n], &b[..n], &c[..n], &d[..n]);
    let (fx, fy, d2, fall) = (tap.fx, tap.fy, tap.d2, tap.falloff);
    for i in 0..n {
        let top = a[i] + (b[i] - a[i]) * fx;
        let bot = c[i] + (d[i] - c[i]) * fx;
        let hs = top + (bot - top) * fy;
        let dz = (hs - h0[i]) * scale;
        let q = (dz * dz + d2).sqrt();
        let q = if q > 1e-9 { q } else { 1e-9 };
        let v = dz / q * fall;
        best[i] = if v > best[i] { v } else { best[i] };
    }
}

/// The occlusion of row `y`, as RGBA pixels.
fn hbao_row(p: &crate::tex::Plane, taps: &[HbaoTap], dirs: usize, steps: usize, y: usize, scale: f32, intensity: f32) -> Vec<[f32; 4]> {
    #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
    {
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: the CPU reports AVX2; the function only reads and writes safe slices
            return unsafe { hbao_row_avx2(p, taps, dirs, steps, y, scale, intensity) };
        }
    }
    hbao_row_generic(p, taps, dirs, steps, y, scale, intensity)
}

#[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
#[target_feature(enable = "avx2")]
unsafe fn hbao_row_avx2(p: &crate::tex::Plane, taps: &[HbaoTap], dirs: usize, steps: usize, y: usize, scale: f32, intensity: f32) -> Vec<[f32; 4]> {
    hbao_row_generic(p, taps, dirs, steps, y, scale, intensity)
}

#[inline(always)]
fn hbao_row_generic(p: &crate::tex::Plane, taps: &[HbaoTap], dirs: usize, steps: usize, y: usize, scale: f32, intensity: f32) -> Vec<[f32; 4]> {
    let (w, hh) = (p.w, p.h);
    let h0 = &p.v[y * w..(y + 1) * w];
    let mut occ = vec![0.0f32; w];
    let mut best = vec![0.0f32; w];
    for k in 0..dirs {
        best.iter_mut().for_each(|b| *b = 0.0);
        for tap in &taps[k * steps..(k + 1) * steps] {
            let ya = (y + tap.iy) % hh;
            let yb = if ya + 1 == hh { 0 } else { ya + 1 };
            let (r0, r1) = (&p.v[ya * w..(ya + 1) * w], &p.v[yb * w..(yb + 1) * w]);
            // pixels x = 0 .. w - ix read columns ix .. w; the rest wrap round to columns 0 ..
            let first = w - tap.ix;
            // the last column of the first run has its right neighbour at column 0
            let n1 = first.saturating_sub(1);
            if n1 > 0 {
                let s = tap.ix;
                hbao_run(&mut best[..n1], &h0[..n1], &r0[s..s + n1], &r0[s + 1..s + 1 + n1], &r1[s..s + n1], &r1[s + 1..s + 1 + n1], tap, scale);
            }
            if first >= 1 {
                // x = first - 1: column w - 1 and its wrapped neighbour 0
                let x = first - 1;
                let c = w - 1;
                let (a, b, cc, d) = ([r0[c]], [r0[0]], [r1[c]], [r1[0]]);
                hbao_run(&mut best[x..x + 1], &h0[x..x + 1], &a, &b, &cc, &d, tap, scale);
            }
            if first < w {
                // x = first .. w: columns 0 .. ix, neighbours 1 ..= ix
                let n2 = w - first;
                hbao_run(&mut best[first..], &h0[first..], &r0[..n2], &r0[1..1 + n2], &r1[..n2], &r1[1..1 + n2], tap, scale);
            }
        }
        for (o, b) in occ.iter_mut().zip(&best) {
            *o += *b;
        }
    }
    occ.iter()
        .map(|o| {
            let a = (1.0 - o / dirs as f32 * intensity).clamp(0.0, 1.0);
            [a, a, a, 1.0]
        })
        .collect()
}

// ---------- colour quantisation (k-means) ----------
fn oklab(c: [f32; 3]) -> [f32; 3] {
    let l = rgb_to_lch(c);
    let a = (l[2] * std::f32::consts::TAU).cos() * l[1];
    let b = (l[2] * std::f32::consts::TAU).sin() * l[1];
    [l[0], a, b]
}

/// Reduces `t` to at most `k` colours chosen by k-means over the image's colour histogram. `smoothing` blurs the input
/// first (pixels) so contours between colour regions are smoother; `dither` (0..1) adds an ordered pattern
/// (`bayer`) or a hash noise before choosing the nearest colour; `lab` measures distance in OKLab instead of RGB.
pub fn quantize_color(t: &Tex, k: usize, smoothing: f32, dither: f32, bayer: bool, lab: bool, ignore_alpha: bool) -> Tex {
    quantize_color_full(t, k, smoothing, dither, bayer, lab, ignore_alpha).0
}

/// Like `quantize_color`, also returning the palette index of every pixel and the palette (RGB).
pub fn quantize_color_full(t: &Tex, k: usize, smoothing: f32, dither: f32, bayer: bool, lab: bool, ignore_alpha: bool) -> (Tex, Vec<u32>, Vec<[f32; 3]>) {
    let src = if smoothing > 0.0 { blur(t, smoothing) } else { t.clone() };
    let k = k.clamp(1, 128);
    let space = |p: [f32; 4]| if lab { oklab([p[0], p[1], p[2]]) } else { [p[0], p[1], p[2]] };
    // histogram with 24 bins per channel: [count, sum of the three channels in the working space]
    const B: usize = 24;
    let mut hist: std::collections::HashMap<usize, (f64, [f64; 3], [f64; 3])> = std::collections::HashMap::new();
    for p in &src.px {
        let key = ((p[0].clamp(0.0, 1.0) * (B as f32 - 1e-3)) as usize * B + (p[1].clamp(0.0, 1.0) * (B as f32 - 1e-3)) as usize) * B + (p[2].clamp(0.0, 1.0) * (B as f32 - 1e-3)) as usize;
        let e = hist.entry(key).or_insert((0.0, [0.0; 3], [0.0; 3]));
        let s = space(*p);
        e.0 += 1.0;
        for c in 0..3 {
            e.1[c] += s[c] as f64;
            e.2[c] += p[c] as f64;
        }
    }
    let mut bins: Vec<(usize, f64, [f32; 3], [f32; 3])> = hist
        .into_iter()
        .map(|(key, (n, s, r))| (key, n, [(s[0] / n) as f32, (s[1] / n) as f32, (s[2] / n) as f32], [(r[0] / n) as f32, (r[1] / n) as f32, (r[2] / n) as f32]))
        .collect();
    bins.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0))); // deterministic
    if bins.is_empty() {
        return (t.clone(), vec![0; t.px.len()], vec![[0.0; 3]]);
    }
    let d2 = |a: [f32; 3], b: [f32; 3]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);
    // farthest-point initialisation starting from the most common colour
    let mut cent: Vec<[f32; 3]> = vec![bins[0].2];
    while cent.len() < k.min(bins.len()) {
        let mut best = (0.0f32, 0usize);
        for (i, b) in bins.iter().enumerate() {
            let d = cent.iter().map(|c| d2(*c, b.2)).fold(f32::MAX, f32::min) * (b.1 as f32).sqrt();
            if d > best.0 {
                best = (d, i);
            }
        }
        if best.0 <= 0.0 {
            break;
        }
        cent.push(bins[best.1].2);
    }
    for _ in 0..10 {
        let mut acc = vec![([0.0f64; 3], 0.0f64); cent.len()];
        for b in &bins {
            let j = (0..cent.len()).min_by(|&x, &y| d2(cent[x], b.2).partial_cmp(&d2(cent[y], b.2)).unwrap_or(std::cmp::Ordering::Equal)).unwrap_or(0);
            for c in 0..3 {
                acc[j].0[c] += b.2[c] as f64 * b.1;
            }
            acc[j].1 += b.1;
        }
        for (j, a) in acc.iter().enumerate() {
            if a.1 > 0.0 {
                cent[j] = [(a.0[0] / a.1) as f32, (a.0[1] / a.1) as f32, (a.0[2] / a.1) as f32];
            }
        }
    }
    // palette in RGB: mean RGB of the pixels (bins) that fall to each centre
    let mut pal = vec![([0.0f64; 3], 0.0f64); cent.len()];
    for b in &bins {
        let j = (0..cent.len()).min_by(|&x, &y| d2(cent[x], b.2).partial_cmp(&d2(cent[y], b.2)).unwrap_or(std::cmp::Ordering::Equal)).unwrap_or(0);
        for c in 0..3 {
            pal[j].0[c] += b.3[c] as f64 * b.1;
        }
        pal[j].1 += b.1;
    }
    let pal: Vec<[f32; 3]> = pal.iter().zip(&cent).map(|(p, c)| if p.1 > 0.0 { [(p.0[0] / p.1) as f32, (p.0[1] / p.1) as f32, (p.0[2] / p.1) as f32] } else { *c }).collect();
    const BAYER: [[f32; 8]; 8] = [
        [0., 32., 8., 40., 2., 34., 10., 42.],
        [48., 16., 56., 24., 50., 18., 58., 26.],
        [12., 44., 4., 36., 14., 46., 6., 38.],
        [60., 28., 52., 20., 62., 30., 54., 22.],
        [3., 35., 11., 43., 1., 33., 9., 41.],
        [51., 19., 59., 27., 49., 17., 57., 25.],
        [15., 47., 7., 39., 13., 45., 5., 37.],
        [63., 31., 55., 23., 61., 29., 53., 21.],
    ];
    let spread = dither * 0.6 / (cent.len() as f32).cbrt();
    let w = src.w;
    let mut out = src.clone();
    let mut ids = vec![0u32; src.px.len()];
    for (i, p) in src.px.iter().enumerate() {
        let (x, y) = (i % w, i / w);
        let off = if dither > 0.0 {
            let n = if bayer { BAYER[y % 8][x % 8] / 64.0 } else { rnd(x as i32, y as i32, 4242) };
            (n - 0.5) * spread
        } else {
            0.0
        };
        let q = space([p[0] + off, p[1] + off, p[2] + off, p[3]]);
        let j = (0..cent.len()).min_by(|&a, &b| d2(cent[a], q).partial_cmp(&d2(cent[b], q)).unwrap_or(std::cmp::Ordering::Equal)).unwrap_or(0);
        out.px[i] = [pal[j][0], pal[j][1], pal[j][2], if ignore_alpha { 1.0 } else { p[3] }];
        ids[i] = j as u32;
    }
    (out, ids, pal)
}

/// Per-node output precision (the node Output Format): 1 = 8 bit integer, 2 = 16 bit integer (both clamp to
/// 0..1 and quantise), 3 = 16 bit float (rounds to half precision, keeps values above 1), anything else = 32 bit float
/// (untouched). Alpha is treated like the colour channels.
pub fn apply_format(t: &mut Tex, fmt: u32) {
    match fmt {
        1 | 2 => {
            let k = if fmt == 1 { 255.0 } else { 65535.0 };
            for p in t.px.iter_mut() {
                for c in p.iter_mut() {
                    *c = (c.clamp(0.0, 1.0) * k).round() / k;
                }
            }
        }
        3 => {
            for p in t.px.iter_mut() {
                for c in p.iter_mut() {
                    *c = crate::exr::half_to_f32(crate::exr::f32_to_half(*c));
                }
            }
        }
        _ => {}
    }
}
