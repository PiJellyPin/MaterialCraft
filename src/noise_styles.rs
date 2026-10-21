// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fractal noises built from one basis and a table of octave weights, with a final tone curve.
//!
//! Many of the library noises that `.sbs` files use differ mostly in *which* scales carry the detail and in the
//! spread of their values. A style records that: the basis (smooth value noise, gradient noise or soft spots), the
//! weight of every octave (octave `k` repeats `2^k` times across the image at scale 1) and five target percentiles
//! the result is mapped to. The weights and percentiles were tuned by comparing our own renders with reference
//! pictures; the code that builds the noise is our own.

use crate::tex::Tex;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Basis {
    /// random values on a lattice, cubic B-spline interpolation (soft, no overshoot)
    Value,
    /// gradient (Perlin) noise
    Gradient,
    /// one soft spot of random height per lattice cell (sparse convolution)
    Spots,
    /// absolute value of gradient noise, inverted (sharp creases)
    Ridged,
}

#[derive(Clone, Debug)]
pub struct Style {
    pub basis: Basis,
    /// amplitude of octave k (period 2^k across the image at scale 1); later octaves continue with `tail`
    pub weights: Vec<f32>,
    /// ratio between successive octaves past the end of `weights`
    pub tail: f32,
    /// extra factor for octaves finer than a quarter / half of the image size (the finest detail fades out)
    pub taper: [f32; 2],
    /// horizontal period = vertical period / stretch (1 = isotropic); > 1 gives horizontal streaks
    pub stretch: f32,
    /// output percentiles 1, 10, 50, 90, 99
    pub tone: [f32; 5],
    /// vertical trend added before the tone curve (dark at the top, light at the bottom)
    pub ramp: f32,
}

impl Style {
    pub fn weight(&self, k: usize) -> f32 {
        match self.weights.get(k) {
            Some(w) => *w,
            None => self.weights.last().copied().unwrap_or(0.0) * self.tail.powi((k + 1 - self.weights.len()) as i32),
        }
    }
}

#[inline(always)]
fn mix(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    h
}

/// A well mixed hash of a lattice point (no patterns along rows or columns).
#[inline(always)]
pub fn hash(x: i32, y: i32, seed: u32) -> u32 {
    mix((x as u32).wrapping_add(mix((y as u32).wrapping_add(mix(seed.wrapping_add(0x9e37_79b9))))))
}

/// A random value in 0..1 for a lattice point.
#[inline]
pub fn unit(x: i32, y: i32, seed: u32) -> f32 {
    (hash(x, y, seed) >> 8) as f32 / 16_777_216.0
}

#[inline]
fn lattice(ix: i32, iy: i32, p: i32, q: i32, seed: u32) -> f32 {
    let x = ix.rem_euclid(p);
    let y = iy.rem_euclid(q);
    (hash(x, y, seed) >> 8) as f32 / 8_388_608.0 - 1.0
}

fn grad_dirs() -> &'static [[f32; 2]; 256] {
    static T: std::sync::OnceLock<[[f32; 2]; 256]> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut t = [[0.0f32; 2]; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let a = (i as f32 + 0.5) / 256.0 * std::f32::consts::TAU;
            *e = [a.cos(), a.sin()];
        }
        t
    })
}

/// One octave of tileable gradient noise in about -1..1, periods `p` x `q`.
pub fn gradient(u: f32, v: f32, p: i32, q: i32, seed: u32) -> f32 {
    let (p, q) = (p.max(1), q.max(1));
    let x = u * p as f32;
    let y = v * q as f32;
    let (fx, fy) = (x.floor(), y.floor());
    let (ix, iy) = (fx as i32, fy as i32);
    let (tx, ty) = (x - fx, y - fy);
    let dirs = grad_dirs();
    let g = |cx: i32, cy: i32, dx: f32, dy: f32| {
        let d = dirs[(hash(cx.rem_euclid(p), cy.rem_euclid(q), seed) >> 24) as usize];
        d[0] * dx + d[1] * dy
    };
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (sx, sy) = (fade(tx), fade(ty));
    let a = g(ix, iy, tx, ty);
    let b = g(ix + 1, iy, tx - 1.0, ty);
    let c = g(ix, iy + 1, tx, ty - 1.0);
    let d = g(ix + 1, iy + 1, tx - 1.0, ty - 1.0);
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    (top + (bot - top) * sy) * 1.4
}

#[inline]
fn bspline(t: f32) -> [f32; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    let s = 1.0 - t;
    [s * s * s / 6.0, (3.0 * t3 - 6.0 * t2 + 4.0) / 6.0, (-3.0 * t3 + 3.0 * t2 + 3.0 * t + 1.0) / 6.0, t3 / 6.0]
}

/// One octave of smooth value noise in -1..1, periods `p` x `q`.
pub fn value(u: f32, v: f32, p: i32, q: i32, seed: u32) -> f32 {
    let x = u * p as f32;
    let y = v * q as f32;
    let (fx, fy) = (x.floor(), y.floor());
    let (ix, iy) = (fx as i32, fy as i32);
    let wx = bspline(x - fx);
    let wy = bspline(y - fy);
    let mut acc = 0.0;
    for (j, wyj) in wy.iter().enumerate() {
        let mut row = 0.0;
        for (i, wxi) in wx.iter().enumerate() {
            row += wxi * lattice(ix + i as i32 - 1, iy + j as i32 - 1, p, q, seed);
        }
        acc += wyj * row;
    }
    // the B-spline averages neighbours: restore the spread of a single lattice value
    acc * 1.8
}

/// One octave of soft spots in about -1..1.
pub fn spots(u: f32, v: f32, p: i32, q: i32, seed: u32) -> f32 {
    let x = u * p as f32;
    let y = v * q as f32;
    let (fx, fy) = (x.floor(), y.floor());
    let (ix, iy) = (fx as i32, fy as i32);
    let mut acc = 0.0;
    for j in -1..=1 {
        for i in -1..=1 {
            let (cx, cy) = (ix + i, iy + j);
            let (wx, wy) = (cx.rem_euclid(p), cy.rem_euclid(q));
            let h = hash(wx, wy, seed);
            let jx = (h & 0x3ff) as f32 / 1023.0;
            let jy = ((h >> 10) & 0x3ff) as f32 / 1023.0;
            let amp = ((h >> 20) & 0xfff) as f32 / 2047.5 - 1.0;
            let dx = x - (cx as f32 + jx);
            let dy = y - (cy as f32 + jy);
            acc += amp * (-(dx * dx + dy * dy) * 4.0).exp();
        }
    }
    acc * 1.6
}

#[inline]
fn basis_at(b: Basis, u: f32, v: f32, p: i32, q: i32, seed: u32) -> f32 {
    match b {
        Basis::Value => value(u, v, p, q, seed),
        Basis::Gradient => gradient(u, v, p, q, seed),
        Basis::Spots => spots(u, v, p, q, seed),
        Basis::Ridged => 1.0 - 2.0 * gradient(u, v, p, q, seed).abs(),
    }
}

/// The octaves of a style for an image `w` wide, at `scale` (multiplies every period): (period x, period y, weight).
pub fn octaves(style: &Style, w: usize, h: usize, scale: f32) -> Vec<(i32, i32, f32)> {
    let lim = w.max(h).max(2) as f32;
    let mut out = Vec::new();
    for k in 0..24usize {
        let q = ((1u32 << k) as f32 * scale).round().max(1.0);
        if q > lim {
            break;
        }
        let mut wt = style.weight(k);
        if q > lim * 0.5 {
            wt *= style.taper[1];
        } else if q > lim * 0.25 {
            wt *= style.taper[0];
        }
        if wt <= 0.0 {
            continue;
        }
        let p = (q / style.stretch.max(1e-3)).round().max(1.0);
        if p > lim {
            continue;
        }
        let ar = h as f32 / w as f32;
        let q = if (ar - 1.0).abs() > 1e-3 { (q * ar).round().max(1.0) } else { q };
        out.push((p as i32, q as i32, wt));
    }
    out
}

/// Raw (un-toned) noise of a style.
pub fn raw(style: &Style, w: usize, h: usize, scale: f32, seed: u32, warp: &(dyn Fn(f32, f32) -> (f32, f32) + Sync)) -> Tex {
    let oct = octaves(style, w, h, scale);
    let basis = style.basis;
    let ramp = style.ramp * 0.35 * oct.iter().map(|o| o.2).sum::<f32>();
    Tex::gray_fn(w, h, move |u, v| {
        let trend = ramp * (v * 2.0 - 1.0);
        let (u, v) = warp(u, v);
        let mut acc = 0.0;
        for (k, &(p, q, wt)) in oct.iter().enumerate() {
            acc += wt * basis_at(basis, u, v, p, q, seed.wrapping_add(k as u32 * 1013));
        }
        acc + trend
    })
}

/// Percentiles 1, 10, 50, 90, 99 (and the extremes) of the first channel.
pub fn percentiles(t: &Tex) -> [f32; 7] {
    let n = t.px.len();
    let step = (n / 65536).max(1);
    let mut s: Vec<f32> = t.px.iter().step_by(step).map(|p| p[0]).collect();
    s.sort_by(|a, b| a.total_cmp(b));
    if s.is_empty() {
        return [0.0; 7];
    }
    let at = |q: f32| s[((q * (s.len() - 1) as f32).round() as usize).min(s.len() - 1)];
    [s[0], at(0.01), at(0.1), at(0.5), at(0.9), at(0.99), s[s.len() - 1]]
}

/// Map the values of `t` so its percentiles land on `target` (piecewise linear between them, extended past the ends
/// with the slope of the outer pieces), clamped to 0..1.
pub fn tone(t: &mut Tex, target: &[f32; 5]) {
    let p = percentiles(t);
    let src = [p[1], p[2], p[3], p[4], p[5]];
    let mut xs: Vec<f32> = Vec::with_capacity(5);
    let mut ys: Vec<f32> = Vec::with_capacity(5);
    for i in 0..5 {
        if xs.last().map(|&l| src[i] > l + 1e-7).unwrap_or(true) {
            xs.push(src[i]);
            ys.push(target[i]);
        }
    }
    if xs.len() < 2 {
        let v = target[2];
        for px in t.px.iter_mut() {
            *px = [v, v, v, 1.0];
        }
        return;
    }
    let n = xs.len();
    let map = |x: f32| -> f32 {
        let i = if x <= xs[0] {
            0
        } else if x >= xs[n - 1] {
            n - 2
        } else {
            xs.iter().position(|&a| a > x).unwrap_or(n - 1).saturating_sub(1).min(n - 2)
        };
        let (x0, x1, y0, y1) = (xs[i], xs[i + 1], ys[i], ys[i + 1]);
        (y0 + (x - x0) * (y1 - y0) / (x1 - x0)).clamp(0.0, 1.0)
    };
    crate::tex::par_rows(&mut t.px, t.w, |_, rows| {
        for px in rows.iter_mut() {
            let g = map(px[0]);
            *px = [g, g, g, 1.0];
        }
    });
}

/// A styled noise: octaves, then the tone curve.
pub fn render(style: &Style, w: usize, h: usize, scale: f32, seed: u32, warp: &(dyn Fn(f32, f32) -> (f32, f32) + Sync)) -> Tex {
    let mut t = raw(style, w, h, scale, seed, warp);
    tone(&mut t, &style.tone);
    t
}

/// A preset: one fitted style under a display name.
pub struct P {
    pub name: &'static str,
    pub basis: Basis,
    pub w: &'static [f32],
    pub tail: f32,
    pub taper: [f32; 2],
    /// < 1: vertical streaks, > 1: horizontal streaks
    pub stretch: f32,
    pub tone: [f32; 5],
}

/// The noise presets, in the order of the Preset Noise node's list (the order is part of the file format: files
/// store the index).
pub const PRESETS: &[P] = &[
    P { name: "Clouds 1", basis: Basis::Gradient, w: &[0.000, 0.035, 0.767, 0.749, 0.947, 1.000, 0.822, 0.647], tail: 0.805, taper: [0.715, 0.286], stretch: 1.0, tone: [0.3223, 0.4833, 0.6418, 0.7708, 0.8541] },
    P { name: "Clouds 2", basis: Basis::Gradient, w: &[0.000, 0.221, 1.000, 0.862, 0.471, 0.281, 0.167, 0.095], tail: 0.582, taper: [0.775, 0.467], stretch: 1.0, tone: [0.3840, 0.5301, 0.6932, 0.8328, 0.9101] },
    P { name: "Clouds 3", basis: Basis::Gradient, w: &[0.603, 0.000, 0.962, 1.000, 0.680, 0.615, 0.437, 0.238], tail: 0.622, taper: [0.000, 2.132], stretch: 1.0, tone: [0.1449, 0.2473, 0.4052, 0.5999, 0.7667] },
    P { name: "BnW Spots 1", basis: Basis::Gradient, w: &[0.000, 0.516, 1.000, 0.657, 0.621, 0.620, 0.627, 0.587], tail: 0.950, taper: [0.832, 0.313], stretch: 1.0, tone: [0.0600, 0.1670, 0.3519, 0.5795, 0.7973] },
    P { name: "BnW Spots 2", basis: Basis::Gradient, w: &[1.000, 0.000, 0.726, 0.509, 0.464, 0.464, 0.318, 0.587], tail: 0.950, taper: [0.000, 1.033], stretch: 1.0, tone: [0.1088, 0.2632, 0.4432, 0.6188, 0.7756] },
    P { name: "BnW Spots 3", basis: Basis::Gradient, w: &[1.000, 0.000, 0.683, 0.534, 0.481, 0.465, 0.286, 0.283], tail: 0.781, taper: [0.000, 1.476], stretch: 1.0, tone: [0.2238, 0.3392, 0.4850, 0.6281, 0.7524] },
    P { name: "Fractal Sum Base", basis: Basis::Gradient, w: &[1.000, 0.000, 0.307, 0.198, 0.157, 0.152, 0.136, 0.185], tail: 0.950, taper: [0.000, 1.070], stretch: 1.0, tone: [0.4018, 0.4376, 0.4895, 0.5547, 0.6014] },
    P { name: "Fractal Sum 1", basis: Basis::Gradient, w: &[1.000, 0.000, 0.332, 0.232, 0.205, 0.220, 0.204, 0.315], tail: 0.950, taper: [0.000, 1.145], stretch: 1.0, tone: [0.1522, 0.2379, 0.3758, 0.5530, 0.7086] },
    P { name: "Fractal Sum 2", basis: Basis::Gradient, w: &[1.000, 0.000, 0.333, 0.221, 0.202, 0.218, 0.202, 0.336], tail: 0.950, taper: [0.000, 1.397], stretch: 1.0, tone: [0.0206, 0.1147, 0.2709, 0.4814, 0.6881] },
    P { name: "Fractal Sum 3", basis: Basis::Gradient, w: &[0.154, 0.000, 0.078, 0.344, 0.420, 1.000, 0.659, 0.274], tail: 0.523, taper: [0.000, 2.138], stretch: 1.0, tone: [0.2165, 0.3301, 0.4956, 0.6657, 0.8008] },
    P { name: "Fractal Sum 4", basis: Basis::Gradient, w: &[0.000, 0.063, 0.137, 0.474, 1.000, 0.791, 0.240, 0.058], tail: 0.300, taper: [3.250, 0.000], stretch: 1.0, tone: [0.2596, 0.4025, 0.6035, 0.7704, 0.8934] },
    P { name: "Moisture", basis: Basis::Gradient, w: &[1.000, 0.000, 0.737, 0.396, 0.229, 0.216, 0.159, 0.128], tail: 0.772, taper: [0.000, 1.206], stretch: 1.0, tone: [0.1731, 0.3475, 0.5266, 0.7023, 0.8307] },
    P { name: "Dirt 1", basis: Basis::Gradient, w: &[0.503, 0.000, 0.396, 0.325, 0.066, 0.300, 0.273, 1.000], tail: 0.950, taper: [0.000, 1.007], stretch: 1.0, tone: [0.0000, 0.0000, 0.1242, 0.3648, 0.6363] },
    P { name: "Dirt 2", basis: Basis::Gradient, w: &[0.450, 0.000, 0.313, 0.269, 0.000, 0.324, 0.170, 1.000], tail: 0.950, taper: [0.000, 0.837], stretch: 1.0, tone: [0.0000, 0.0000, 0.0000, 0.0000, 0.3993] },
    P { name: "Dirt 3", basis: Basis::Gradient, w: &[0.008, 0.000, 0.000, 0.000, 0.147, 0.275, 0.611, 1.000], tail: 0.950, taper: [0.867, 0.763], stretch: 1.0, tone: [0.0000, 0.0084, 0.1111, 0.2808, 0.4586] },
    P { name: "Dirt 4", basis: Basis::Gradient, w: &[0.000, 0.030, 0.392, 0.491, 0.501, 0.638, 0.634, 1.000], tail: 0.950, taper: [0.723, 0.000], stretch: 1.0, tone: [0.0000, 0.0000, 0.2242, 0.5161, 0.7475] },
    P { name: "Dirt 5", basis: Basis::Gradient, w: &[0.026, 0.000, 0.044, 0.071, 0.139, 0.272, 0.538, 1.000], tail: 0.950, taper: [0.000, 0.867], stretch: 1.0, tone: [0.0000, 0.0000, 0.0553, 0.2570, 0.4685] },
    P { name: "Clouds 1 (legacy)", basis: Basis::Gradient, w: &[0.000, 0.091, 0.690, 0.873, 0.992, 1.000, 0.699, 0.496], tail: 0.704, taper: [0.674, 0.000], stretch: 1.0, tone: [0.1381, 0.3392, 0.5561, 0.7415, 0.8589] },
    P { name: "Clouds 2 (legacy)", basis: Basis::Gradient, w: &[0.000, 0.350, 1.000, 0.700, 0.408, 0.232, 0.117, 0.067], tail: 0.536, taper: [0.846, 0.554], stretch: 1.0, tone: [0.2246, 0.4711, 0.7091, 0.8637, 0.9561] },
    P { name: "Clouds 3 (legacy)", basis: Basis::Gradient, w: &[0.999, 0.000, 0.863, 1.000, 0.929, 0.733, 0.432, 0.264], tail: 0.600, taper: [0.000, 1.756], stretch: 1.0, tone: [0.1427, 0.2564, 0.4160, 0.6206, 0.7860] },
    P { name: "BnW Spots 1 (legacy)", basis: Basis::Gradient, w: &[0.000, 0.438, 1.000, 0.586, 0.755, 0.700, 0.595, 0.604], tail: 0.929, taper: [1.107, 0.395], stretch: 1.0, tone: [0.1185, 0.2231, 0.3838, 0.5784, 0.7673] },
    P { name: "BnW Spots 2 (legacy)", basis: Basis::Gradient, w: &[0.000, 0.332, 1.000, 0.909, 0.753, 0.755, 0.578, 0.887], tail: 0.950, taper: [0.567, 0.945], stretch: 1.0, tone: [0.0662, 0.2240, 0.3997, 0.5708, 0.7263] },
    P { name: "BnW Spots 3 (legacy)", basis: Basis::Gradient, w: &[0.000, 0.345, 1.000, 0.828, 0.717, 0.717, 0.504, 0.474], tail: 0.813, taper: [0.858, 1.267], stretch: 1.0, tone: [0.1763, 0.2998, 0.4500, 0.5964, 0.7232] },
    P { name: "Fractal Sum Base (legacy)", basis: Basis::Gradient, w: &[1.000, 0.000, 0.367, 0.198, 0.192, 0.190, 0.176, 0.219], tail: 0.950, taper: [0.000, 1.092], stretch: 1.0, tone: [0.4276, 0.4639, 0.5135, 0.5625, 0.6004] },
    P { name: "Fractal Sum 1 (legacy)", basis: Basis::Gradient, w: &[1.000, 0.000, 0.404, 0.238, 0.257, 0.279, 0.269, 0.375], tail: 0.950, taper: [0.000, 1.186], stretch: 1.0, tone: [0.2010, 0.2907, 0.4242, 0.5781, 0.7135] },
    P { name: "Fractal Sum 2 (legacy)", basis: Basis::Gradient, w: &[1.000, 0.000, 0.419, 0.238, 0.267, 0.288, 0.274, 0.391], tail: 0.950, taper: [0.000, 1.203], stretch: 1.0, tone: [0.0057, 0.1297, 0.3270, 0.5648, 0.7885] },
    P { name: "Fractal Sum 3 (legacy)", basis: Basis::Gradient, w: &[0.189, 0.000, 0.000, 0.273, 0.469, 1.000, 0.667, 0.287], tail: 0.535, taper: [0.000, 1.989], stretch: 1.0, tone: [0.2290, 0.3374, 0.4907, 0.6581, 0.7938] },
    P { name: "Fractal Sum 4 (legacy)", basis: Basis::Gradient, w: &[0.000, 0.075, 0.404, 0.409, 1.000, 0.841, 0.285, 0.083], tail: 0.314, taper: [3.303, 0.000], stretch: 1.0, tone: [0.3134, 0.4547, 0.6320, 0.7919, 0.9105] },
    P { name: "Soft Noise 1 (legacy)", basis: Basis::Gradient, w: &[1.000, 0.000, 0.821, 0.275, 0.000, 0.000, 0.000, 0.000], tail: 0.500, taper: [0.000, 0.000], stretch: 1.0, tone: [0.0370, 0.1905, 0.4505, 0.7158, 0.8983] },
    P { name: "Soft Noise 2 (legacy)", basis: Basis::Gradient, w: &[0.000, 1.000, 0.664, 0.000, 0.000, 0.000, 0.000, 0.000], tail: 0.500, taper: [0.000, 0.000], stretch: 1.0, tone: [0.0294, 0.1136, 0.3691, 0.7226, 0.9682] },
    P { name: "Grunge Map 001", basis: Basis::Gradient, w: &[1.000, 0.000, 0.889, 0.707, 0.611, 0.520, 0.439, 0.367], tail: 0.840, taper: [0.000, 1.057], stretch: 1.000, tone: [0.0615, 0.4716, 0.8585, 1.0000, 1.0000] },
    P { name: "Grunge Map 002", basis: Basis::Gradient, w: &[0.750, 0.825, 1.000, 0.793, 0.687, 0.596, 0.486, 0.164], tail: 0.525, taper: [4.510, 0.000], stretch: 0.250, tone: [0.0000, 0.1137, 0.4842, 0.9461, 1.0000] },
    P { name: "Grunge Map 003", basis: Basis::Gradient, w: &[0.331, 0.659, 1.000, 0.492, 0.426, 0.479, 0.294, 0.135], tail: 0.531, taper: [2.419, 0.000], stretch: 0.250, tone: [0.0000, 0.0757, 0.5998, 1.0000, 1.0000] },
    P { name: "Grunge Map 004", basis: Basis::Gradient, w: &[0.000, 0.311, 0.575, 1.000, 0.856, 0.583, 0.374, 0.265], tail: 0.674, taper: [0.180, 1.320], stretch: 1.000, tone: [0.0000, 0.0000, 0.0000, 0.6301, 1.0000] },
    P { name: "Grunge Map 005", basis: Basis::Gradient, w: &[0.000, 0.000, 1.000, 1.000, 0.461, 0.277, 0.000, 0.083], tail: 0.547, taper: [0.000, 0.000], stretch: 0.125, tone: [0.0000, 0.2502, 0.7291, 1.0000, 1.0000] },
    P { name: "Grunge Map 006", basis: Basis::Gradient, w: &[0.000, 1.000, 0.000, 0.159, 0.107, 0.060, 0.032, 0.064], tail: 0.950, taper: [0.736, 0.122], stretch: 2.000, tone: [0.0000, 0.0626, 0.8920, 1.0000, 1.0000] },
    P { name: "Grunge Map 007", basis: Basis::Gradient, w: &[1.000, 0.000, 0.702, 0.602, 0.403, 0.326, 0.186, 0.211], tail: 0.805, taper: [0.000, 1.356], stretch: 1.000, tone: [0.0000, 0.0000, 0.4890, 0.9498, 1.0000] },
    P { name: "Grunge Map 008", basis: Basis::Gradient, w: &[0.000, 0.519, 1.000, 0.960, 0.651, 0.595, 0.421, 0.455], tail: 0.875, taper: [0.155, 0.943], stretch: 1.000, tone: [0.0000, 0.2223, 0.7718, 1.0000, 1.0000] },
    P { name: "Grunge Map 009", basis: Basis::Gradient, w: &[1.000, 0.000, 0.846, 0.500, 0.367, 0.351, 0.299, 0.326], tail: 0.950, taper: [0.000, 1.272], stretch: 1.000, tone: [0.0000, 0.0208, 0.7609, 1.0000, 1.0000] },
    P { name: "Grunge Map 010", basis: Basis::Gradient, w: &[1.000, 0.000, 0.798, 0.638, 0.605, 0.500, 0.720, 0.826], tail: 0.950, taper: [0.000, 0.916], stretch: 1.000, tone: [0.0000, 0.1016, 0.7090, 1.0000, 1.0000] },
    P { name: "Grunge Map 011", basis: Basis::Gradient, w: &[1.000, 0.000, 0.701, 0.719, 0.572, 0.653, 0.694, 0.755], tail: 0.950, taper: [0.000, 0.996], stretch: 1.000, tone: [0.0000, 0.0000, 0.4233, 0.9541, 1.0000] },
    P { name: "Grunge Map 012", basis: Basis::Gradient, w: &[0.746, 0.000, 0.548, 0.808, 0.697, 0.844, 0.779, 1.000], tail: 0.950, taper: [0.000, 0.944], stretch: 1.000, tone: [0.0000, 0.6730, 1.0000, 1.0000, 1.0000] },
    P { name: "Grunge Map 013", basis: Basis::Gradient, w: &[1.000, 0.000, 0.873, 0.480, 0.209, 0.381, 0.180, 0.201], tail: 0.726, taper: [0.000, 1.998], stretch: 1.000, tone: [0.0000, 0.1122, 0.4269, 0.7579, 1.0000] },
    P { name: "Grunge Map 014", basis: Basis::Gradient, w: &[1.000, 0.000, 0.418, 0.281, 0.180, 0.240, 0.251, 0.192], tail: 0.893, taper: [0.000, 0.907], stretch: 1.000, tone: [0.0000, 0.0000, 0.3645, 0.7197, 0.9674] },
    P { name: "Grunge Map 015", basis: Basis::Gradient, w: &[1.000, 0.000, 0.268, 0.213, 0.111, 0.109, 0.096, 0.127], tail: 0.950, taper: [0.000, 1.281], stretch: 1.000, tone: [0.0000, 0.0712, 0.2989, 0.6688, 0.9180] },
    P { name: "Grunge Concrete", basis: Basis::Gradient, w: &[1.000, 0.000, 0.787, 0.473, 0.304, 0.301, 0.299, 0.346], tail: 0.950, taper: [0.000, 2.184], stretch: 1.000, tone: [0.1572, 0.2676, 0.3747, 0.4736, 0.5538] },
    P { name: "Grunge Damas", basis: Basis::Gradient, w: &[0.000, 0.195, 0.000, 0.000, 0.657, 1.000, 0.821, 0.850], tail: 0.922, taper: [0.533, 0.308], stretch: 2.000, tone: [0.0614, 0.1801, 0.7338, 0.9878, 0.9987] },
    P { name: "Grunge Galvanic Large", basis: Basis::Gradient, w: &[1.000, 0.000, 0.707, 0.664, 0.400, 0.389, 0.292, 0.346], tail: 0.942, taper: [0.000, 1.428], stretch: 1.000, tone: [0.1803, 0.3050, 0.4803, 0.7241, 0.9900] },
    P { name: "Grunge Galvanic Small", basis: Basis::Gradient, w: &[0.000, 0.032, 0.563, 0.506, 0.773, 0.985, 1.000, 0.904], tail: 0.950, taper: [0.678, 0.267], stretch: 1.000, tone: [0.1227, 0.2493, 0.4648, 0.7268, 0.8901] },
    P { name: "Grunge Leaks", basis: Basis::Gradient, w: &[0.363, 0.765, 1.000, 0.768, 0.451, 0.334, 0.236, 0.180], tail: 0.735, taper: [0.000, 0.740], stretch: 0.500, tone: [0.0239, 0.0720, 0.2912, 0.6932, 0.9141] },
    P { name: "Grunge Leaky Paint", basis: Basis::Gradient, w: &[0.512, 0.467, 1.000, 0.940, 0.668, 0.572, 0.494, 0.269], tail: 0.685, taper: [0.487, 0.562], stretch: 0.500, tone: [0.1157, 0.2228, 0.3645, 0.4933, 0.6134] },
    P { name: "Grunge Rough Dirty", basis: Basis::Gradient, w: &[1.000, 0.000, 0.485, 0.433, 0.370, 0.380, 0.349, 0.460], tail: 0.950, taper: [0.000, 1.362], stretch: 1.000, tone: [0.3603, 0.4452, 0.5461, 0.6356, 0.6968] },
    P { name: "Grunge Rust Fine", basis: Basis::Gradient, w: &[1.000, 0.000, 0.911, 0.629, 0.594, 0.536, 0.416, 0.418], tail: 0.883, taper: [0.000, 1.610], stretch: 1.000, tone: [0.2311, 0.3840, 0.5726, 0.7593, 0.8805] },
    P { name: "Grunge Scratches Dirty", basis: Basis::Gradient, w: &[1.000, 0.000, 0.753, 0.485, 0.362, 0.362, 0.358, 0.730], tail: 0.950, taper: [0.000, 1.243], stretch: 1.000, tone: [0.3819, 0.4260, 0.4700, 0.5166, 0.5659] },
    P { name: "Grunge Scratches Fine", basis: Basis::Gradient, w: &[0.109, 0.000, 0.076, 0.124, 0.108, 0.234, 0.206, 0.541], tail: 0.950, taper: [0.000, 2.047], stretch: 1.000, tone: [0.4450, 0.4816, 0.5018, 0.5107, 0.6140] },
    P { name: "Grunge Scratches Rough", basis: Basis::Gradient, w: &[0.330, 0.000, 0.343, 0.227, 0.476, 0.760, 1.000, 0.717], tail: 0.950, taper: [0.000, 0.857], stretch: 1.000, tone: [0.0000, 0.0000, 0.0007, 0.1257, 0.4855] },
    P { name: "Grunge Shavings", basis: Basis::Gradient, w: &[0.000, 0.078, 0.197, 0.224, 0.310, 0.638, 1.000, 0.849], tail: 0.950, taper: [0.406, 0.000], stretch: 1.000, tone: [0.0000, 0.0000, 0.0010, 0.0732, 0.3598] },
    P { name: "Grunge Splashes Dusty", basis: Basis::Gradient, w: &[0.000, 0.247, 0.677, 0.683, 0.840, 1.000, 0.900, 0.429], tail: 0.655, taper: [0.456, 1.173], stretch: 1.000, tone: [0.0280, 0.0372, 0.0509, 0.1177, 0.6572] },
    P { name: "Grunge Spots", basis: Basis::Gradient, w: &[0.000, 0.049, 0.196, 0.210, 0.351, 0.419, 1.000, 0.951], tail: 0.950, taper: [0.091, 0.245], stretch: 1.000, tone: [0.0000, 0.0000, 0.0000, 0.0000, 0.8340] },
    P { name: "Grunge Spots Dirty", basis: Basis::Gradient, w: &[0.000, 0.000, 0.638, 0.723, 0.873, 1.000, 0.801, 0.758], tail: 0.871, taper: [0.450, 0.350], stretch: 1.000, tone: [0.0000, 0.0000, 0.0000, 0.0000, 0.6740] },
    P { name: "Directional Noise 1", basis: Basis::Gradient, w: &[0.000, 0.717, 0.195, 0.782, 1.000, 0.468, 0.470, 0.580], tail: 0.950, taper: [0.604, 0.702], stretch: 4.000, tone: [0.1065, 0.2028, 0.3537, 0.5406, 0.6969] },
    P { name: "Directional Noise 2", basis: Basis::Gradient, w: &[1.000, 0.000, 0.000, 0.477, 0.839, 0.799, 0.598, 0.810], tail: 0.950, taper: [0.558, 0.981], stretch: 4.000, tone: [0.0809, 0.1827, 0.3242, 0.4880, 0.6390] },
    P { name: "Directional Noise 3", basis: Basis::Gradient, w: &[1.000, 0.000, 0.340, 0.345, 0.000, 0.260, 0.142, 0.238], tail: 0.950, taper: [0.000, 1.020], stretch: 8.000, tone: [0.2682, 0.3851, 0.5304, 0.6833, 0.8329] },
    P { name: "Directional Noise 4", basis: Basis::Gradient, w: &[0.261, 0.000, 0.000, 0.593, 0.407, 1.000, 0.590, 0.279], tail: 0.528, taper: [1.085, 0.081], stretch: 8.000, tone: [0.2686, 0.3892, 0.5055, 0.6225, 0.7535] },
    P { name: "Directional Noise (legacy)", basis: Basis::Gradient, w: &[1.000, 0.000, 0.000, 0.106, 0.359, 0.300, 0.325, 0.318], tail: 0.950, taper: [0.984, 0.471], stretch: 4.000, tone: [0.0830, 0.1584, 0.2852, 0.4462, 0.6057] },
    P { name: "Vertical Noise", basis: Basis::Gradient, w: &[0.444, 0.208, 1.000, 0.843, 0.381, 0.171, 0.008, 0.000], tail: 0.500, taper: [0.000, 0.000], stretch: 0.125, tone: [0.2151, 0.3820, 0.5051, 0.6293, 0.7659] },
    P { name: "Directional Scratches", basis: Basis::Gradient, w: &[0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.085, 1.000], tail: 0.500, taper: [0.229, 2.929], stretch: 8.000, tone: [0.0830, 0.2162, 0.4309, 0.6755, 0.8491] },
    P { name: "Wood Fibers 1", basis: Basis::Gradient, w: &[1.000, 0.158, 0.378, 0.436, 0.396, 0.388, 0.260, 0.346], tail: 0.945, taper: [0.000, 0.368], stretch: 0.125, tone: [0.0838, 0.1760, 0.3291, 0.5159, 0.6950] },
    P { name: "Wood Fibers 2", basis: Basis::Gradient, w: &[1.000, 0.579, 0.861, 0.766, 0.841, 0.651, 0.643, 0.414], tail: 0.797, taper: [0.000, 0.122], stretch: 0.125, tone: [0.3353, 0.4467, 0.5780, 0.7107, 0.8182] },
    P { name: "Crystal 2", basis: Basis::Gradient, w: &[0.448, 0.415, 1.000, 0.533, 0.236, 0.068, 0.000, 0.002], tail: 0.300, taper: [0.000, 0.000], stretch: 0.250, tone: [0.2602, 0.3832, 0.4988, 0.6131, 0.7474] },
    P { name: "Crystal 2 (legacy)", basis: Basis::Gradient, w: &[0.000, 0.736, 1.000, 0.543, 0.260, 0.072, 0.019, 0.000], tail: 0.500, taper: [0.000, 0.000], stretch: 0.250, tone: [0.1113, 0.2709, 0.4002, 0.5559, 0.7490] },
    P { name: "Gaussian Spots 1", basis: Basis::Gradient, w: &[0.176, 0.000, 0.341, 0.250, 0.862, 1.000, 0.000, 0.000], tail: 0.500, taper: [0.000, 0.000], stretch: 1.000, tone: [0.0000, 0.0007, 0.0836, 0.3221, 0.5937] },
    P { name: "Gaussian Spots 2", basis: Basis::Gradient, w: &[0.000, 0.000, 0.000, 0.100, 0.191, 0.375, 0.849, 1.000], tail: 0.950, taper: [0.608, 0.000], stretch: 1.000, tone: [0.0000, 0.0007, 0.0526, 0.2497, 0.4577] },
    P { name: "Few Spots (legacy)", basis: Basis::Gradient, w: &[0.730, 0.000, 0.325, 0.651, 0.887, 1.000, 0.000, 0.000], tail: 0.500, taper: [0.000, 0.000], stretch: 1.000, tone: [0.0000, 0.0000, 0.0776, 0.3394, 0.6202] },
    P { name: "Spots (legacy)", basis: Basis::Gradient, w: &[0.000, 0.032, 0.065, 0.114, 0.188, 0.374, 0.872, 1.000], tail: 0.950, taper: [0.616, 0.000], stretch: 1.000, tone: [0.0000, 0.0007, 0.0516, 0.2494, 0.4555] },
    P { name: "Dirt Gradient", basis: Basis::Gradient, w: &[0.000, 1.000, 0.718, 0.364, 0.301, 0.209, 0.187, 0.151], tail: 0.850, taper: [0.990, 0.000], stretch: 1.000, tone: [0.0032, 0.0207, 0.2236, 0.6074, 0.7775] },
    P { name: "Dirt Gradient (legacy)", basis: Basis::Gradient, w: &[0.000, 1.000, 0.426, 0.404, 0.257, 0.217, 0.167, 0.137], tail: 0.794, taper: [1.070, 0.000], stretch: 1.000, tone: [0.0004, 0.0061, 0.1797, 0.5927, 0.7649] },
    P { name: "Horizontal Scratch (legacy)", basis: Basis::Gradient, w: &[0.000, 0.000, 0.000, 0.000, 0.000, 0.035, 0.255, 0.693], tail: 0.950, taper: [0.804, 1.598], stretch: 1.000, tone: [0.0000, 0.0234, 0.1787, 0.4463, 0.6467] },
    P { name: "Noise Zoom (legacy)", basis: Basis::Gradient, w: &[0.000, 0.000, 0.000, 0.808, 1.000, 0.000, 0.000, 0.000], tail: 0.500, taper: [0.000, 0.000], stretch: 1.000, tone: [0.0855, 0.1758, 0.3281, 0.5146, 0.7136] },
    P { name: "White Stains (legacy)", basis: Basis::Spots, w: &[0.000, 0.000, 0.011, 0.000, 0.000, 0.000, 0.000, 0.252], tail: 0.500, taper: [6.727, 15.873], stretch: 1.000, tone: [0.0000, 0.0000, 0.1113, 0.3228, 0.5540] },
    P { name: "Creased (legacy)", basis: Basis::Gradient, w: &[0.000, 0.121, 0.447, 1.000, 0.487, 0.201, 0.026, 0.000], tail: 0.500, taper: [0.000, 0.000], stretch: 1.000, tone: [0.1438, 0.3483, 0.4883, 0.6010, 0.7840] },
    P { name: "Creased", basis: Basis::Gradient, w: &[0.000, 0.308, 0.435, 1.000, 0.722, 0.290, 0.057, 0.000], tail: 0.500, taper: [0.000, 0.000], stretch: 1.000, tone: [0.2115, 0.3838, 0.5019, 0.6201, 0.7172] },
    P { name: "Fluid (legacy)", basis: Basis::Gradient, w: &[0.185, 0.066, 0.183, 0.535, 1.000, 0.530, 0.287, 0.150], tail: 0.532, taper: [0.000, 0.000], stretch: 0.500, tone: [0.1144, 0.3860, 0.7144, 0.8638, 0.8912] },
    P { name: "Fluid", basis: Basis::Gradient, w: &[0.000, 0.000, 0.000, 0.312, 1.000, 0.581, 0.289, 0.194], tail: 0.578, taper: [0.000, 4.841], stretch: 0.500, tone: [0.1507, 0.3843, 0.6997, 0.8735, 0.9101] },
    P { name: "Herb (legacy)", basis: Basis::Gradient, w: &[0.000, 0.029, 0.201, 0.197, 0.423, 0.507, 0.720, 1.000], tail: 0.950, taper: [0.801, 0.481], stretch: 1.000, tone: [0.0171, 0.1383, 0.3003, 0.4825, 0.6581] },
    P { name: "Horizontal Lines (legacy)", basis: Basis::Gradient, w: &[0.033, 0.000, 0.000, 0.057, 0.082, 0.233, 0.289, 0.380], tail: 0.950, taper: [2.771, 0.000], stretch: 1.000, tone: [0.0318, 0.1349, 0.3055, 0.5144, 0.7351] },
    P { name: "Liquid (legacy)", basis: Basis::Gradient, w: &[0.000, 0.017, 0.055, 0.367, 0.508, 0.510, 1.000, 0.849], tail: 0.950, taper: [0.478, 0.000], stretch: 1.000, tone: [0.1923, 0.3068, 0.4513, 0.6071, 0.7337] },
    P { name: "Liquid", basis: Basis::Gradient, w: &[0.000, 0.019, 0.093, 0.436, 0.465, 0.825, 1.000, 0.667], tail: 0.899, taper: [0.131, 0.000], stretch: 1.000, tone: [0.1558, 0.2695, 0.4293, 0.5913, 0.7274] },
    P { name: "Messy Fibers 1 (legacy)", basis: Basis::Gradient, w: &[0.000, 0.000, 0.182, 0.375, 0.419, 0.000, 0.000, 1.000], tail: 0.500, taper: [1.811, 0.000], stretch: 1.000, tone: [0.0935, 0.3430, 0.6885, 0.9428, 1.0000] },
    P { name: "Messy Fibers 1", basis: Basis::Gradient, w: &[0.454, 0.000, 0.107, 0.514, 0.405, 0.000, 0.000, 0.193], tail: 0.500, taper: [10.362, 0.000], stretch: 1.000, tone: [0.1601, 0.3779, 0.6820, 0.9032, 0.9788] },
    P { name: "Sponge Stains (legacy)", basis: Basis::Gradient, w: &[0.000, 0.054, 0.044, 0.545, 0.588, 0.411, 0.203, 0.000], tail: 0.500, taper: [0.000, 0.000], stretch: 1.000, tone: [0.2088, 0.3591, 0.5542, 0.7463, 0.8909] },
    P { name: "Paper Path (legacy)", basis: Basis::Gradient, w: &[0.004, 0.000, 0.035, 0.015, 0.000, 0.336, 1.000, 0.294], tail: 0.935, taper: [0.000, 1.056], stretch: 1.000, tone: [0.0000, 0.0000, 0.9315, 1.0000, 1.0000] },
];

/// Names of the presets, for the node's list.
pub const PRESET_NAMES: &[&str] = &[
    "Clouds 1", "Clouds 2", "Clouds 3", "BnW Spots 1", "BnW Spots 2", "BnW Spots 3", "Fractal Sum Base", "Fractal Sum 1", "Fractal Sum 2", "Fractal Sum 3", "Fractal Sum 4", "Moisture", "Dirt 1", "Dirt 2", "Dirt 3", "Dirt 4", "Dirt 5",
    "Clouds 1 (legacy)", "Clouds 2 (legacy)", "Clouds 3 (legacy)", "BnW Spots 1 (legacy)", "BnW Spots 2 (legacy)", "BnW Spots 3 (legacy)", "Fractal Sum Base (legacy)", "Fractal Sum 1 (legacy)", "Fractal Sum 2 (legacy)", "Fractal Sum 3 (legacy)", "Fractal Sum 4 (legacy)", "Soft Noise 1 (legacy)", "Soft Noise 2 (legacy)",
    "Grunge Map 001", "Grunge Map 002", "Grunge Map 003", "Grunge Map 004", "Grunge Map 005", "Grunge Map 006", "Grunge Map 007", "Grunge Map 008", "Grunge Map 009", "Grunge Map 010", "Grunge Map 011", "Grunge Map 012", "Grunge Map 013", "Grunge Map 014", "Grunge Map 015", "Grunge Concrete", "Grunge Damas", "Grunge Galvanic Large", "Grunge Galvanic Small", "Grunge Leaks", "Grunge Leaky Paint", "Grunge Rough Dirty", "Grunge Rust Fine", "Grunge Scratches Dirty", "Grunge Scratches Fine", "Grunge Scratches Rough", "Grunge Shavings", "Grunge Splashes Dusty", "Grunge Spots", "Grunge Spots Dirty", "Directional Noise 1", "Directional Noise 2", "Directional Noise 3", "Directional Noise 4", "Directional Noise (legacy)", "Vertical Noise", "Directional Scratches", "Wood Fibers 1", "Wood Fibers 2", "Crystal 2", "Crystal 2 (legacy)", "Gaussian Spots 1", "Gaussian Spots 2", "Few Spots (legacy)", "Spots (legacy)", "Dirt Gradient", "Dirt Gradient (legacy)", "Horizontal Scratch (legacy)", "Noise Zoom (legacy)", "White Stains (legacy)", "Creased (legacy)", "Creased", "Fluid (legacy)", "Fluid", "Herb (legacy)", "Horizontal Lines (legacy)", "Liquid (legacy)", "Liquid", "Messy Fibers 1 (legacy)", "Messy Fibers 1", "Sponge Stains (legacy)", "Paper Path (legacy)",
];

/// The style of preset `i`, with `roughness` (1 = as fitted) changing the weight of the finer octaves and
/// `quarter_turns` turning the streaks of directional presets by 90 degree steps.
pub fn preset_style(i: usize, roughness: f32, quarter_turns: i32) -> Style {
    let p = &PRESETS[i.min(PRESETS.len() - 1)];
    let mut w = p.w.to_vec();
    let mut tail = p.tail;
    if (roughness - 1.0).abs() > 1e-4 {
        let r = roughness.max(0.0);
        // the finer octaves (past the strongest one) gain or lose weight by r per octave
        let peak = w.iter().enumerate().fold((0, 0.0f32), |a, (k, &x)| if x > a.1 { (k, x) } else { a }).0;
        for (k, x) in w.iter_mut().enumerate() {
            if k > peak {
                *x *= r.powi((k - peak) as i32);
            }
        }
        tail = (tail * r).min(1.5);
    }
    let stretch = if quarter_turns.rem_euclid(2) == 1 { 1.0 / p.stretch.max(1e-3) } else { p.stretch };
    let ramp = if p.name.starts_with("Dirt Gradient") { 1.5 } else { 0.0 };
    Style { basis: p.basis, weights: w, tail, taper: p.taper, stretch, tone: p.tone, ramp }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_and_names_agree() {
        assert_eq!(PRESETS.len(), PRESET_NAMES.len());
        for (p, n) in PRESETS.iter().zip(PRESET_NAMES) {
            assert_eq!(p.name, *n);
            assert!(p.tone.windows(2).all(|t| t[0] <= t[1]), "{}", p.name);
        }
    }

    #[test]
    fn styled_noise_tiles_and_follows_its_tone() {
        let st = preset_style(0, 1.0, 0);
        let t = render(&st, 128, 128, 1.0, 3, &|u, v| (u, v));
        let p = percentiles(&t);
        assert!((p[3] - st.tone[2]).abs() < 0.03, "median {}", p[3]);
        // across the left/right edge the values change no more than between any two neighbouring columns
        let (mut wrap, mut inside) = (0.0, 0.0);
        for y in 0..128 {
            wrap += (t.px[y * 128][0] - t.px[y * 128 + 127][0]).abs();
            inside += (t.px[y * 128 + 63][0] - t.px[y * 128 + 64][0]).abs();
        }
        assert!(wrap < inside * 1.5 + 0.5, "seam {wrap} vs {inside}");
        // non-square images work too
        let t = render(&st, 128, 64, 2.0, 3, &|u, v| (u, v));
        assert_eq!((t.w, t.h), (128, 64));
    }
}
