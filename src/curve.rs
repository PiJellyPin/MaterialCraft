// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tone curves: a list of control points joined by smooth cubic segments, baked into a lookup table.
//!
//! Text form (used by the `curve` node): `x:y` points separated by commas, e.g. `0:0,0.5:0.7,1:1`.
//! A point may carry a slope (`x:y:s`, the same slope on both sides) or two (`x:y:sin:sout`),
//! or Bezier handles at absolute positions (`x:y:lx:ly:rx:ry`, the left then the right handle), as
//! Substance Designer's Curve node stores them; a segment with a handle at either end is a cubic Bezier.
//! Points without a slope get a smooth, overshoot-free slope from their neighbours.

use crate::tex::Tex;

#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    /// (x, y, slope in, slope out), sorted by x
    pub pts: Vec<(f32, f32, f32, f32)>,
    /// optional Bezier handles per point: (left, right) at absolute curve positions
    pub handles: Vec<Option<([f32; 2], [f32; 2])>>,
}

pub const LUT: usize = 1024;

impl Curve {
    pub fn identity() -> Curve {
        Curve::new(vec![(0.0, 0.0, None, None), (1.0, 1.0, None, None)])
    }

    /// Points with optional explicit slopes; computes the automatic ones.
    pub fn new(raw: Vec<(f32, f32, Option<f32>, Option<f32>)>) -> Curve {
        Curve::with_handles(raw.into_iter().map(|p| (p, None)).collect())
    }

    /// Points with optional slopes and optional Bezier handles (left, right).
    pub fn with_handles(mut list: Vec<((f32, f32, Option<f32>, Option<f32>), Option<([f32; 2], [f32; 2])>)>) -> Curve {
        list.sort_by(|a, b| a.0 .0.partial_cmp(&b.0 .0).unwrap_or(std::cmp::Ordering::Equal));
        list.dedup_by(|a, b| (a.0 .0 - b.0 .0).abs() < 1e-6);
        let mut handles: Vec<Option<([f32; 2], [f32; 2])>> = list.iter().map(|p| p.1).collect();
        let mut raw: Vec<(f32, f32, Option<f32>, Option<f32>)> = list.into_iter().map(|p| p.0).collect();
        if raw.is_empty() {
            return Curve::identity();
        }
        if raw.len() == 1 {
            let p = raw[0];
            raw = vec![(0.0, p.1, None, None), (1.0, p.1, None, None)];
            handles = vec![None, None];
        }
        let n = raw.len();
        // secant slopes and Fritsch-Carlson monotone tangents
        let sec: Vec<f32> = (0..n - 1).map(|i| (raw[i + 1].1 - raw[i].1) / (raw[i + 1].0 - raw[i].0)).collect();
        let mut auto = vec![0.0f32; n];
        auto[0] = sec[0];
        auto[n - 1] = sec[n - 2];
        for i in 1..n - 1 {
            auto[i] = if sec[i - 1] * sec[i] <= 0.0 { 0.0 } else { (sec[i - 1] + sec[i]) * 0.5 };
        }
        for i in 0..n - 1 {
            if sec[i] == 0.0 {
                auto[i] = 0.0;
                auto[i + 1] = 0.0;
            } else {
                let (a, b) = (auto[i] / sec[i], auto[i + 1] / sec[i]);
                let s = a * a + b * b;
                if s > 9.0 {
                    let t = 3.0 / s.sqrt();
                    auto[i] = t * a * sec[i];
                    auto[i + 1] = t * b * sec[i];
                }
            }
        }
        let pts = raw
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let s_in = p.2.unwrap_or(auto[i]);
                let s_out = p.3.or(p.2).unwrap_or(auto[i]);
                (p.0.clamp(0.0, 1.0), p.1.clamp(0.0, 1.0), s_in, s_out)
            })
            .collect();
        Curve { pts, handles }
    }

    pub fn parse(text: &str) -> Result<Curve, String> {
        let mut raw = Vec::new();
        for item in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let parts: Vec<&str> = item.split(':').collect();
            let num = |s: &str| s.trim().parse::<f32>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("bad number '{s}' in curve point '{item}'"));
            match parts.len() {
                2 => raw.push(((num(parts[0])?, num(parts[1])?, None, None), None)),
                3 => raw.push(((num(parts[0])?, num(parts[1])?, Some(num(parts[2])?), None), None)),
                4 => raw.push(((num(parts[0])?, num(parts[1])?, Some(num(parts[2])?), Some(num(parts[3])?)), None)),
                6 => {
                    let v: Vec<f32> = parts.iter().map(|s| num(s)).collect::<Result<_, _>>()?;
                    raw.push(((v[0], v[1], None, None), Some(([v[2], v[3]], [v[4], v[5]]))));
                }
                _ => return Err(format!("curve point '{item}' must look like x:y")),
            }
        }
        if raw.is_empty() {
            return Err("a curve needs at least one point".into());
        }
        Ok(Curve::with_handles(raw))
    }

    pub fn to_text(&self) -> String {
        self.pts
            .iter()
            .zip(self.handles.iter())
            .map(|(p, h)| match h {
                Some((l, r)) => format!("{}:{}:{}:{}:{}:{}", p.0, p.1, l[0], l[1], r[0], r[1]),
                None => format!("{}:{}:{}:{}", p.0, p.1, p.2, p.3),
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn eval(&self, x: f32) -> f32 {
        let p = &self.pts;
        if x <= p[0].0 {
            return p[0].1;
        }
        let last = p[p.len() - 1];
        if x >= last.0 {
            return last.1;
        }
        let i = p.partition_point(|q| q.0 <= x) - 1;
        let (a, b) = (p[i], p[i + 1]);
        let h = b.0 - a.0;
        let (ha, hb) = (self.handles.get(i).copied().flatten(), self.handles.get(i + 1).copied().flatten());
        if ha.is_some() || hb.is_some() {
            // cubic Bezier through the handles (a slope stands in for a missing handle);
            // the handles' x stays inside the segment so the curve is a function of x
            let c1 = ha.map(|h| h.1).unwrap_or([a.0 + h / 3.0, a.1 + a.3 * h / 3.0]);
            let c2 = hb.map(|h| h.0).unwrap_or([b.0 - h / 3.0, b.1 - b.2 * h / 3.0]);
            let (x1, x2) = (c1[0].clamp(a.0, b.0), c2[0].clamp(a.0, b.0));
            let bz = |p0: f32, p1: f32, p2: f32, p3: f32, t: f32| {
                let u = 1.0 - t;
                u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3
            };
            // x(t) never decreases with the handles inside the segment: bisection finds t
            let (mut lo, mut hi) = (0.0f32, 1.0f32);
            for _ in 0..40 {
                let m = 0.5 * (lo + hi);
                if bz(a.0, x1, x2, b.0, m) < x {
                    lo = m;
                } else {
                    hi = m;
                }
            }
            return bz(a.1, c1[1], c2[1], b.1, 0.5 * (lo + hi)).clamp(0.0, 1.0);
        }
        let t = (x - a.0) / h;
        let (t2, t3) = (t * t, t * t * t);
        let y = (2.0 * t3 - 3.0 * t2 + 1.0) * a.1 + (t3 - 2.0 * t2 + t) * h * a.3 + (-2.0 * t3 + 3.0 * t2) * b.1 + (t3 - t2) * h * b.2;
        y.clamp(0.0, 1.0)
    }

    pub fn lut(&self) -> Vec<f32> {
        (0..LUT).map(|i| self.eval(i as f32 / (LUT - 1) as f32)).collect()
    }
}

fn lookup(lut: &[f32], x: f32) -> f32 {
    let f = x.clamp(0.0, 1.0) * (LUT - 1) as f32;
    let i = (f as usize).min(LUT - 2);
    let t = f - i as f32;
    lut[i] + (lut[i + 1] - lut[i]) * t
}

/// Apply the master curve to R, G and B, then the per-channel curves (r, g, b, a).
pub fn apply(t: &Tex, master: &Curve, ch: [&Curve; 4]) -> Tex {
    let m = master.lut();
    let l: Vec<Vec<f32>> = ch.iter().map(|c| c.lut()).collect();
    let mut out = t.clone();
    for p in out.px.iter_mut() {
        for k in 0..3 {
            p[k] = lookup(&l[k], lookup(&m, p[k]));
        }
        p[3] = lookup(&l[3], p[3]);
    }
    out
}
