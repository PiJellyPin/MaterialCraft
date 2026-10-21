// SPDX-License-Identifier: MIT OR Apache-2.0
//! The review sheet: one picture of a material for checking it at a glance (the lit 3D preview, the output maps, the
//! node graph with a thumbnail per node, an optional reference picture next to the preview) and numbers that say how
//! it differs from the reference (brightness, colour, contrast, fine detail, feature size), so a person or an AI
//! assistant can compare and adjust without guessing.

use crate::json::Json;
use crate::tex::Tex;

/// A picture to draw on, in display values (0..1).
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[f32; 4]>,
}

pub const BG: [f32; 4] = [0.11, 0.115, 0.125, 1.0];
pub const PANEL: [f32; 4] = [0.16, 0.165, 0.18, 1.0];
pub const TEXT: [f32; 4] = [0.9, 0.9, 0.92, 1.0];
pub const DIM: [f32; 4] = [0.6, 0.62, 0.66, 1.0];
pub const ACCENT: [f32; 4] = [0.95, 0.62, 0.25, 1.0];

impl Canvas {
    pub fn new(w: usize, h: usize, bg: [f32; 4]) -> Canvas {
        Canvas { w, h, px: vec![bg; w * h] }
    }

    pub fn into_tex(self) -> Tex {
        Tex { w: self.w, h: self.h, px: self.px }
    }

    fn put(&mut self, x: i64, y: i64, c: [f32; 4]) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            let i = y as usize * self.w + x as usize;
            let a = c[3].clamp(0.0, 1.0);
            let d = &mut self.px[i];
            for k in 0..3 {
                d[k] = d[k] * (1.0 - a) + c[k] * a;
            }
            d[3] = 1.0;
        }
    }

    pub fn rect(&mut self, x: i64, y: i64, w: i64, h: i64, c: [f32; 4]) {
        for yy in y.max(0)..(y + h).min(self.h as i64) {
            for xx in x.max(0)..(x + w).min(self.w as i64) {
                self.put(xx, yy, c);
            }
        }
    }

    pub fn frame(&mut self, x: i64, y: i64, w: i64, h: i64, c: [f32; 4], t: i64) {
        self.rect(x, y, w, t, c);
        self.rect(x, y + h - t, w, t, c);
        self.rect(x, y, t, h, c);
        self.rect(x + w - t, y, t, h, c);
    }

    /// Draw `t` scaled to `w` x `h` at (x, y) (bilinear when shrinking or growing).
    pub fn blit(&mut self, t: &Tex, x: i64, y: i64, w: usize, h: usize) {
        if t.w == 0 || t.h == 0 || w == 0 || h == 0 {
            return;
        }
        for yy in 0..h {
            for xx in 0..w {
                let u = (xx as f32 + 0.5) / w as f32;
                let v = (yy as f32 + 0.5) / h as f32;
                let fx = (u * t.w as f32 - 0.5).clamp(0.0, (t.w - 1) as f32);
                let fy = (v * t.h as f32 - 0.5).clamp(0.0, (t.h - 1) as f32);
                let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
                let (x1, y1) = ((x0 + 1).min(t.w - 1), (y0 + 1).min(t.h - 1));
                let (ax, ay) = (fx - x0 as f32, fy - y0 as f32);
                let p = |xx: usize, yy: usize| t.px[yy * t.w + xx];
                let (a, b, c, d) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
                let mut o = [0.0; 4];
                for k in 0..4 {
                    o[k] = (a[k] * (1.0 - ax) + b[k] * ax) * (1.0 - ay) + (c[k] * (1.0 - ax) + d[k] * ax) * ay;
                }
                // transparency over a dark checker
                let chk = if ((xx / 8) + (yy / 8)) % 2 == 0 { 0.32 } else { 0.24 };
                let al = o[3].clamp(0.0, 1.0);
                self.put(x + xx as i64, y + yy as i64, [o[0] * al + chk * (1.0 - al), o[1] * al + chk * (1.0 - al), o[2] * al + chk * (1.0 - al), 1.0]);
            }
        }
    }

    /// A line `thick` pixels wide.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: [f32; 4], thick: f32) {
        let n = ((x1 - x0).abs().max((y1 - y0).abs()) as usize).max(1);
        let r = (thick * 0.5).max(0.5);
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let (x, y) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
            let ri = r.ceil() as i64;
            for dy in -ri..=ri {
                for dx in -ri..=ri {
                    if ((dx * dx + dy * dy) as f32) <= r * r + 0.25 {
                        self.put(x.round() as i64 + dx, y.round() as i64 + dy, c);
                    }
                }
            }
        }
    }

    /// A curve from an output (left) to an input (right), like the graph editor draws links.
    pub fn link(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: [f32; 4]) {
        let dx = ((x1 - x0).abs() * 0.5).max(30.0);
        let pt = |t: f32| {
            let u = 1.0 - t;
            let (ax, bx, cx, ex) = (x0, x0 + dx, x1 - dx, x1);
            let (ay, by, cy, ey) = (y0, y0, y1, y1);
            (u * u * u * ax + 3.0 * u * u * t * bx + 3.0 * u * t * t * cx + t * t * t * ex, u * u * u * ay + 3.0 * u * u * t * by + 3.0 * u * t * t * cy + t * t * t * ey)
        };
        let steps = 24;
        let mut prev = pt(0.0);
        for i in 1..=steps {
            let p = pt(i as f32 / steps as f32);
            self.line(prev.0, prev.1, p.0, p.1, c, 1.5);
            prev = p;
        }
    }

    /// Text in the built-in 5 x 10 pixel face, `scale` pixels per font pixel. Returns the width drawn.
    pub fn text(&mut self, x: i64, y: i64, s: &str, scale: i64, c: [f32; 4]) -> i64 {
        let mut pen = x;
        for ch in s.chars() {
            let code = ch as u32;
            let code = if (32..127).contains(&code) { code } else { '?' as u32 };
            let rows = &crate::font::GLYPHS[(code - 32) as usize];
            for (r, bits) in rows.iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        self.rect(pen + col * scale, y + r as i64 * scale, scale, scale, c);
                    }
                }
            }
            pen += 6 * scale;
        }
        pen - x
    }
}

/// Text cut to `max` characters (with "..").
pub fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(2)).collect();
        format!("{t}..")
    }
}

// ------------------------------------------------------------------------------------------------ measures

fn lum(p: &[f32; 4]) -> f32 {
    0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
}

/// In-place radix-2 FFT of `re` + i `im` (length a power of two).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -std::f32::consts::TAU / len as f32;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (s, c) = (ang * k as f32).sin_cos();
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

/// Numbers that describe how a picture looks, independent of where its features are.
#[derive(Debug, Clone)]
pub struct Look {
    /// mean colour (display values)
    pub mean: [f32; 3],
    /// brightness spread (standard deviation of the luminance)
    pub contrast: f32,
    /// mean step between neighbouring pixels at 256 px (fine detail / noise)
    pub detail: f32,
    /// how many times the main features repeat across the picture (from the power spectrum; a rough estimate)
    pub features_across: f32,
    /// luminance histogram, 16 bins summing to 1
    pub hist: [f32; 16],
}

/// Measure `t` (a picture of any size). `window`: fade the borders before measuring the feature size (for pictures
/// that do not tile, such as photos).
pub fn look(t: &Tex, window: bool) -> Look {
    let n = 128usize;
    let s = crate::ops::resize_to(t, n, n);
    let d = crate::ops::resize_to(t, 256, 256);
    let l: Vec<f32> = s.px.iter().map(lum).collect();
    let count = l.len().max(1) as f32;
    let mut mean = [0.0f32; 3];
    for p in &s.px {
        for k in 0..3 {
            mean[k] += p[k] / count;
        }
    }
    let ml = l.iter().sum::<f32>() / count;
    let contrast = (l.iter().map(|v| (v - ml) * (v - ml)).sum::<f32>() / count).sqrt();
    let dl: Vec<f32> = d.px.iter().map(lum).collect();
    let mut steps = 0.0f32;
    for y in 0..256 {
        for x in 0..256 {
            let v = dl[y * 256 + x];
            steps += (v - dl[y * 256 + (x + 1) % 256]).abs() + (v - dl[((y + 1) % 256) * 256 + x]).abs();
        }
    }
    let detail = steps / (2.0 * 256.0 * 256.0);
    let mut hist = [0.0f32; 16];
    for v in &l {
        hist[((v.clamp(0.0, 0.9999)) * 16.0) as usize] += 1.0 / count;
    }
    // power spectrum of the luminance, averaged over rings; the main feature size is where r * P(r) peaks
    let mut re: Vec<f32> = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let w = if window {
                let hx = 0.5 - 0.5 * (std::f32::consts::TAU * (x as f32 + 0.5) / n as f32).cos();
                let hy = 0.5 - 0.5 * (std::f32::consts::TAU * (y as f32 + 0.5) / n as f32).cos();
                hx * hy
            } else {
                1.0
            };
            re.push((l[y * n + x] - ml) * w);
        }
    }
    let mut im = vec![0.0f32; n * n];
    for y in 0..n {
        fft(&mut re[y * n..(y + 1) * n], &mut im[y * n..(y + 1) * n]);
    }
    let (mut cr, mut ci) = (vec![0.0f32; n], vec![0.0f32; n]);
    for x in 0..n {
        for y in 0..n {
            cr[y] = re[y * n + x];
            ci[y] = im[y * n + x];
        }
        fft(&mut cr, &mut ci);
        for y in 0..n {
            re[y * n + x] = cr[y];
            im[y * n + x] = ci[y];
        }
    }
    let half = n / 2;
    let mut ring = vec![0.0f32; half];
    let mut cnt = vec![0.0f32; half];
    for y in 0..n {
        for x in 0..n {
            let fx = if x < half { x as f32 } else { x as f32 - n as f32 };
            let fy = if y < half { y as f32 } else { y as f32 - n as f32 };
            let r = (fx * fx + fy * fy).sqrt().round() as usize;
            if r >= 1 && r < half {
                let i = y * n + x;
                ring[r] += re[i] * re[i] + im[i] * im[i];
                cnt[r] += 1.0;
            }
        }
    }
    let mut best = (1usize, 0.0f32);
    for r in 1..(half * 3 / 4) {
        let p = if cnt[r] > 0.0 { ring[r] / cnt[r] } else { 0.0 };
        let score = p * r as f32;
        if score > best.1 {
            best = (r, score);
        }
    }
    Look { mean, contrast, detail, features_across: best.0 as f32, hist }
}

impl Look {
    pub fn json(&self) -> Json {
        let r3 = |v: f32| (v * 1000.0).round() / 1000.0;
        Json::obj()
            .with("mean_color", Json::Arr(self.mean.iter().map(|v| Json::from(r3(*v))).collect()))
            .with("brightness", r3(0.2126 * self.mean[0] + 0.7152 * self.mean[1] + 0.0722 * self.mean[2]))
            .with("contrast", r3(self.contrast))
            .with("fine_detail", r3(self.detail))
            .with("features_across", self.features_across)
    }
}

/// Hue (degrees) and colourfulness of a mean colour, in a simple opponent space.
fn hue_sat(c: [f32; 3]) -> (f32, f32) {
    let a = c[0] - c[1];
    let b = (c[0] + c[1]) * 0.5 - c[2];
    (b.atan2(a).to_degrees().rem_euclid(360.0), (a * a + b * b).sqrt())
}

fn hue_name(h: f32) -> &'static str {
    match h {
        h if !(30.0..330.0).contains(&h) => "red",
        h if h < 75.0 => "orange / yellow",
        h if h < 120.0 => "yellow / green",
        h if h < 200.0 => "green",
        h if h < 260.0 => "cyan / blue",
        _ => "blue / purple",
    }
}

/// How `ours` differs from `reference`: a similarity score (0..1, 1 = the same overall look) and hints in words.
pub fn compare(ours: &Look, reference: &Look) -> (f32, Json, Vec<String>) {
    let mut hints = Vec::new();
    let lb = |m: [f32; 3]| 0.2126 * m[0] + 0.7152 * m[1] + 0.0722 * m[2];
    let (bo, br) = (lb(ours.mean), lb(reference.mean));
    if (bo - br).abs() > 0.04 {
        hints.push(format!("ours is {} than the reference (brightness {:.2} vs {:.2})", if bo > br { "brighter" } else { "darker" }, bo, br));
    }
    let (ho, so) = hue_sat(ours.mean);
    let (hr, sr) = hue_sat(reference.mean);
    if sr > 0.04 || so > 0.04 {
        let dh = ((ho - hr + 540.0).rem_euclid(360.0)) - 180.0;
        if dh.abs() > 15.0 && sr > 0.04 && so > 0.04 {
            hints.push(format!("the colour is off: ours leans {} where the reference leans {} (hue {:.0} vs {:.0} degrees)", hue_name(ho), hue_name(hr), ho, hr));
        }
        if so > sr * 1.35 + 0.01 {
            hints.push(format!("ours is more saturated (colourfulness {so:.2} vs {sr:.2})"));
        } else if so * 1.35 + 0.01 < sr {
            hints.push(format!("ours is less saturated (colourfulness {so:.2} vs {sr:.2})"));
        }
    }
    let cr = ours.contrast / reference.contrast.max(1e-4);
    if cr < 0.75 {
        hints.push(format!("ours has less contrast ({:.2} vs {:.2}): widen the value range (Levels, Histogram Scan) or strengthen the height / colour variation", ours.contrast, reference.contrast));
    } else if cr > 1.33 {
        hints.push(format!("ours has more contrast ({:.2} vs {:.2})", ours.contrast, reference.contrast));
    }
    let dr = ours.detail / reference.detail.max(1e-4);
    if dr < 0.7 {
        hints.push(format!("ours is smoother, with less fine detail ({:.3} vs {:.3}): add small-scale noise or grain", ours.detail, reference.detail));
    } else if dr > 1.4 {
        hints.push(format!("ours has more fine detail or noise ({:.3} vs {:.3}): blur it or lower the high-frequency noise", ours.detail, reference.detail));
    }
    let fr = ours.features_across / reference.features_across.max(1.0);
    if fr > 1.35 {
        hints.push(format!("ours has smaller, more numerous main features (about {} across vs {}): use fewer tiles or a larger scale", ours.features_across, reference.features_across));
    } else if fr < 0.74 {
        hints.push(format!("ours has larger main features (about {} across vs {}): use more tiles or a smaller scale", ours.features_across, reference.features_across));
    }
    let hist_sim: f32 = ours.hist.iter().zip(reference.hist.iter()).map(|(a, b)| a.min(*b)).sum();
    let color_dist = ((ours.mean[0] - reference.mean[0]).powi(2) + (ours.mean[1] - reference.mean[1]).powi(2) + (ours.mean[2] - reference.mean[2]).powi(2)).sqrt();
    let score = (hist_sim * 0.4 + (1.0 - color_dist.min(1.0)) * 0.3 + (1.0 - (cr.ln().abs() / 1.5).min(1.0)) * 0.15 + (1.0 - (fr.ln().abs() / 1.5).min(1.0)) * 0.15).clamp(0.0, 1.0);
    let r3 = |v: f32| (v * 1000.0).round() / 1000.0;
    let j = Json::obj()
        .with("similarity", r3(score))
        .with("histogram_overlap", r3(hist_sim))
        .with("mean_color_distance", r3(color_dist))
        .with("contrast_ratio", r3(cr))
        .with("detail_ratio", r3(dr))
        .with("feature_size_ratio", r3(1.0 / fr.max(1e-3)))
        .with("ours", ours.json())
        .with("reference", reference.json());
    if hints.is_empty() {
        hints.push("no large differences in brightness, colour, contrast, detail or feature size; compare the pictures for shapes and patterns".into());
    }
    (score, j, hints)
}

/// The part of a picture that shows the subject, as fractions (x, y, w, h): what differs from the plain background
/// (taken from the picture's border). `None` when the whole picture is the subject or nothing stands out.
pub fn object_box(t: &Tex) -> Option<[f32; 4]> {
    if t.w < 16 || t.h < 16 {
        return None;
    }
    let b = (t.w.min(t.h) / 32).max(1);
    let mut border: Vec<[f32; 3]> = Vec::new();
    for y in 0..t.h {
        for x in 0..t.w {
            if x < b || y < b || x + b >= t.w || y + b >= t.h {
                let p = t.px[y * t.w + x];
                border.push([p[0], p[1], p[2]]);
            }
        }
    }
    let median = |k: usize| {
        let mut v: Vec<f32> = border.iter().map(|p| p[k]).collect();
        v.sort_by(|a, c| a.partial_cmp(c).unwrap_or(std::cmp::Ordering::Equal));
        v[v.len() / 2]
    };
    let bg = [median(0), median(1), median(2)];
    let dist = |p: &[f32; 4]| ((p[0] - bg[0]).powi(2) + (p[1] - bg[1]).powi(2) + (p[2] - bg[2]).powi(2)).sqrt();
    let spread = border.iter().map(|p| ((p[0] - bg[0]).powi(2) + (p[1] - bg[1]).powi(2) + (p[2] - bg[2]).powi(2)).sqrt()).sum::<f32>() / border.len().max(1) as f32;
    let thr = 0.06 + 3.0 * spread;
    let mut rows = vec![0usize; t.h];
    let mut cols = vec![0usize; t.w];
    for y in 0..t.h {
        for x in 0..t.w {
            if dist(&t.px[y * t.w + x]) > thr {
                rows[y] += 1;
                cols[x] += 1;
            }
        }
    }
    let first = |v: &[usize], need: usize| v.iter().position(|c| *c >= need);
    let last = |v: &[usize], need: usize| v.iter().rposition(|c| *c >= need);
    let (rn, cn) = ((t.w / 50).max(2), (t.h / 50).max(2));
    let (y0, y1, x0, x1) = (first(&rows, rn)?, last(&rows, rn)?, first(&cols, cn)?, last(&cols, cn)?);
    if x1 <= x0 + 8 || y1 <= y0 + 8 {
        return None;
    }
    let frac = ((x1 - x0) * (y1 - y0)) as f32 / (t.w * t.h) as f32;
    if !(0.05..=0.9).contains(&frac) {
        return None;
    }
    // a little inside, away from the object's edge and shading falloff
    let (w, h) = ((x1 - x0) as f32, (y1 - y0) as f32);
    Some([(x0 as f32 + w * 0.08) / t.w as f32, (y0 as f32 + h * 0.08) / t.h as f32, w * 0.84 / t.w as f32, h * 0.84 / t.h as f32])
}

/// The part `r` = (x, y, w, h) (fractions) of a picture.
pub fn crop_frac(t: &Tex, r: [f32; 4]) -> Tex {
    let x0 = ((r[0].clamp(0.0, 1.0)) * t.w as f32) as usize;
    let y0 = ((r[1].clamp(0.0, 1.0)) * t.h as f32) as usize;
    let w = ((r[2].clamp(0.01, 1.0) * t.w as f32) as usize).clamp(1, t.w - x0.min(t.w - 1));
    let h = ((r[3].clamp(0.01, 1.0) * t.h as f32) as usize).clamp(1, t.h - y0.min(t.h - 1));
    let (x0, y0) = (x0.min(t.w - 1), y0.min(t.h - 1));
    let (w, h) = (w.min(t.w - x0), h.min(t.h - y0));
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            px.push(t.px[(y0 + y) * t.w + x0 + x]);
        }
    }
    Tex { w, h, px }
}

/// The picture with its colour multiplied by `gain` (an exposure change in linear light).
pub fn exposure_gain(t: &Tex, gain: f32) -> Tex {
    Tex { w: t.w, h: t.h, px: t.px.iter().map(|p| [(p[0] * gain).min(1.0), (p[1] * gain).min(1.0), (p[2] * gain).min(1.0), p[3]]).collect() }
}

/// The middle part of a 3D preview (inside the object), for measuring the material without the background.
pub fn centre_crop(t: &Tex, frac: f32) -> Tex {
    let (w, h) = (((t.w as f32 * frac) as usize).max(1), ((t.h as f32 * frac) as usize).max(1));
    let (x0, y0) = ((t.w - w) / 2, (t.h - h) / 2);
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            px.push(t.px[(y0 + y) * t.w + x0 + x]);
        }
    }
    Tex { w, h, px }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stripes(n: usize, k: usize, lo: f32, hi: f32) -> Tex {
        Tex::from_fn(n, n, |u, _| {
            let v = if (u * k as f32).fract() < 0.5 { lo } else { hi };
            [v, v * 0.8, v * 0.6, 1.0]
        })
    }

    #[test]
    fn the_measures_see_size_contrast_and_brightness() {
        let a = look(&stripes(256, 8, 0.2, 0.8), false);
        assert!((a.features_across - 8.0).abs() <= 1.0, "{}", a.features_across);
        let b = look(&stripes(256, 4, 0.4, 0.6), false);
        assert!((b.features_across - 4.0).abs() <= 1.0, "{}", b.features_across);
        let (score_same, _, _) = compare(&a, &a);
        let (score_diff, _, hints) = compare(&b, &a);
        assert!(score_same > 0.95 && score_diff < score_same);
        assert!(hints.iter().any(|h| h.contains("less contrast")), "{hints:?}");
        assert!(hints.iter().any(|h| h.contains("larger main features")), "{hints:?}");
    }

    #[test]
    fn text_and_pictures_are_drawn() {
        let mut c = Canvas::new(120, 40, BG);
        let w = c.text(2, 2, "Base 1", 2, TEXT);
        assert_eq!(w, 6 * 2 * 6);
        c.blit(&stripes(32, 2, 0.0, 1.0), 80, 0, 40, 40);
        c.link(0.0, 30.0, 70.0, 10.0, ACCENT);
        let t = c.into_tex();
        assert!(t.px.iter().any(|p| p[0] > 0.85));
    }
}
