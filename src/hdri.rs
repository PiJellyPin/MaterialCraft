// SPDX-License-Identifier: MIT OR Apache-2.0
//! High dynamic range environments for the 3D viewer and the path tracer.
//!
//! * readers for Radiance `.hdr` (RGBE, flat and run-length) and OpenEXR through the `exr` crate,
//!   plus standard bitmap formats for low dynamic range images
//! * [`Env`]: an equirectangular environment with a spherical-harmonics irradiance (diffuse lighting) and a chain of
//!   GGX-prefiltered levels (specular reflections), the usual "split sum" image based lighting set-up
//! * procedural presets (studio, clear sky, sunset, overcast, night, neutral grey, white furnace)
//!
//! Direction to image mapping (shared with the GLSL shader): `u = 0.5 + atan2(d.x, -d.z) / tau`, `v = acos(d.y) / pi`.
//! The middle of the image looks down the -Z axis and the top row is straight up (+Y).
use crate::tex::Tex;
use crate::v3::*;
use std::f32::consts::{PI, TAU};

// ------------------------------------------------------------------------------------------------ file readers
/// Radiance RGBE (`.hdr` / `.pic`). Returns linear RGB in a `Tex` (alpha 1).
pub fn decode_hdr(b: &[u8]) -> Result<Tex, String> {
    if b.len() < 10 || !(b.starts_with(b"#?RADIANCE") || b.starts_with(b"#?RGBE")) {
        return Err("not a Radiance .hdr file".into());
    }
    // header lines up to the first empty line, then the resolution line
    let mut pos = 0usize;
    let line = |pos: &mut usize| -> Option<String> {
        if *pos >= b.len() {
            return None;
        }
        let start = *pos;
        while *pos < b.len() && b[*pos] != b'\n' {
            *pos += 1;
        }
        let s = String::from_utf8_lossy(&b[start..*pos]).trim().to_string();
        *pos += 1;
        Some(s)
    };
    let mut format_ok = true;
    loop {
        let l = line(&mut pos).ok_or("truncated header")?;
        if l.is_empty() {
            break;
        }
        if let Some(f) = l.strip_prefix("FORMAT=") {
            format_ok = f.trim() == "32-bit_rle_rgbe";
        }
    }
    if !format_ok {
        return Err("only 32-bit_rle_rgbe .hdr files are supported".into());
    }
    let res = line(&mut pos).ok_or("missing resolution line")?;
    let t: Vec<&str> = res.split_whitespace().collect();
    if t.len() != 4 {
        return Err(format!("bad resolution line '{res}'"));
    }
    let (ys, h, xs, w) = (t[0], t[1].parse::<usize>().map_err(|_| "bad height")?, t[2], t[3].parse::<usize>().map_err(|_| "bad width")?);
    if w == 0 || h == 0 || w > 32768 || h > 32768 || w * h > 1 << 28 {
        return Err("unsupported .hdr size".into());
    }
    let flip_y = ys == "+Y";
    let flip_x = xs == "-X";
    let mut px = vec![[0.0f32, 0.0, 0.0, 1.0]; w * h];
    let mut row = vec![[0u8; 4]; w];
    let rgbe = |c: [u8; 4]| -> [f32; 4] {
        if c[3] == 0 {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            let f = 2f32.powi(c[3] as i32 - 136);
            [c[0] as f32 * f, c[1] as f32 * f, c[2] as f32 * f, 1.0]
        }
    };
    for y in 0..h {
        if pos + 4 > b.len() {
            return Err("truncated pixel data".into());
        }
        let adaptive = w >= 8 && w < 32768 && b[pos] == 2 && b[pos + 1] == 2 && (b[pos + 2] & 0x80) == 0 && (((b[pos + 2] as usize) << 8) | b[pos + 3] as usize) == w;
        if adaptive {
            pos += 4;
            for ch in 0..4 {
                let mut x = 0;
                while x < w {
                    let n = *b.get(pos).ok_or("truncated run")? as usize;
                    pos += 1;
                    if n > 128 {
                        let cnt = n - 128;
                        let v = *b.get(pos).ok_or("truncated run")?;
                        pos += 1;
                        if x + cnt > w {
                            return Err("bad run length".into());
                        }
                        for _ in 0..cnt {
                            row[x][ch] = v;
                            x += 1;
                        }
                    } else {
                        if n == 0 || x + n > w || pos + n > b.len() {
                            return Err("bad literal run".into());
                        }
                        for _ in 0..n {
                            row[x][ch] = b[pos];
                            pos += 1;
                            x += 1;
                        }
                    }
                }
            }
        } else {
            // flat pixels (the old run-length form is not produced any more and is rejected)
            if pos + 4 * w > b.len() {
                return Err("truncated pixel data".into());
            }
            for x in 0..w {
                row[x] = [b[pos], b[pos + 1], b[pos + 2], b[pos + 3]];
                pos += 4;
            }
        }
        let dy = if flip_y { h - 1 - y } else { y };
        for x in 0..w {
            let dx = if flip_x { w - 1 - x } else { x };
            px[dy * w + dx] = rgbe(row[x]);
        }
    }
    Ok(Tex { w, h, px })
}

/// OpenEXR import, shared with bitmap loading.
pub fn decode_exr(bytes: &[u8]) -> Result<Tex, String> {
    crate::exr::decode_exr(bytes)
}

/// Load an environment image: `.hdr`, `.exr`, or a PNG (treated as sRGB and converted to linear).
pub fn load_image(path: &std::path::Path) -> Result<Tex, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    load_bytes(&path.to_string_lossy(), &bytes)
}

pub fn load_bytes(name: &str, bytes: &[u8]) -> Result<Tex, String> {
    let lower = name.to_lowercase();
    if bytes.starts_with(b"#?") || lower.ends_with(".hdr") || lower.ends_with(".pic") {
        decode_hdr(bytes)
    } else if bytes.starts_with(&[0x76, 0x2f, 0x31, 0x01]) || lower.ends_with(".exr") {
        decode_exr(bytes)
    } else if crate::imgio::sniff(name, bytes).is_some() {
        let d = crate::imgio::decode_full(name, bytes)?;
        let mut t = d.tex;
        if !d.float {
            for p in t.px.iter_mut() {
                for c in p.iter_mut().take(3) {
                    *c = c.max(0.0).powf(2.2);
                }
            }
        }
        Ok(t)
    } else {
        Err(format!("{name}: unsupported environment format (use .hdr, .exr, .png, .jpg, .tif, .webp, .dds)"))
    }
}

// ------------------------------------------------------------------------------------------------ environment
#[derive(Clone, Debug)]
pub struct Level {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[f32; 3]>,
}

impl Level {
    /// Bilinear lookup: wraps horizontally, clamps vertically.
    fn at(&self, u: f32, v: f32) -> V {
        let x = u * self.w as f32 - 0.5;
        let y = (v * self.h as f32 - 0.5).clamp(0.0, self.h as f32 - 1.0);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let w = self.w as i64;
        let xa = (x0 as i64).rem_euclid(w) as usize;
        let xb = ((x0 as i64 + 1).rem_euclid(w)) as usize;
        let ya = y0 as usize;
        let yb = (ya + 1).min(self.h - 1);
        let p = |x: usize, y: usize| self.px[y * self.w + x];
        let (a, b, c, d) = (p(xa, ya), p(xb, ya), p(xa, yb), p(xb, yb));
        let mut o = [0.0; 3];
        for k in 0..3 {
            let t = a[k] + (b[k] - a[k]) * fx;
            let bt = c[k] + (d[k] - c[k]) * fx;
            o[k] = t + (bt - t) * fy;
        }
        o
    }
}

/// Image coordinates of a direction (see the module notes).
pub fn dir_to_uv(d: V) -> (f32, f32) {
    let u = 0.5 + d[0].atan2(-d[2]) / TAU;
    (u - u.floor(), d[1].clamp(-1.0, 1.0).acos() / PI)
}

/// Direction of the centre of image coordinates (u, v).
pub fn uv_to_dir(u: f32, v: f32) -> V {
    let (theta, phi) = ((u - 0.5) * TAU, v * PI);
    let s = phi.sin();
    [s * theta.sin(), phi.cos(), -s * theta.cos()]
}

#[derive(Clone, Debug)]
pub struct Env {
    pub name: String,
    /// level 0 is the environment itself; level k is convolved with a GGX lobe of roughness k / (levels - 1)
    pub levels: Vec<Level>,
    /// diffuse lighting: second order spherical harmonics, already multiplied by the cosine-lobe weights and divided by
    /// pi, so `irradiance(n) = sum(coef[i] * basis_i(n))` is the cosine-weighted mean radiance around `n`
    pub sh: [V; 9],
    /// brightest value in the image (for exposure hints)
    pub peak: f32,
    /// mean luminance
    pub mean: f32,
}

fn lum3(c: V) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn halve(l: &Level) -> Level {
    let (w, h) = ((l.w / 2).max(1), (l.h / 2).max(1));
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let mut a = [0.0; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = l.px[((y * 2 + dy).min(l.h - 1)) * l.w + (x * 2 + dx).min(l.w - 1)];
                for k in 0..3 {
                    a[k] += p[k] * 0.25;
                }
            }
            px.push(a);
        }
    }
    Level { w, h, px }
}

fn radical_inverse(mut i: u32) -> f32 {
    i = (i << 16) | (i >> 16);
    i = ((i & 0x55555555) << 1) | ((i & 0xAAAAAAAA) >> 1);
    i = ((i & 0x33333333) << 2) | ((i & 0xCCCCCCCC) >> 2);
    i = ((i & 0x0F0F0F0F) << 4) | ((i & 0xF0F0F0F0) >> 4);
    i = ((i & 0x00FF00FF) << 8) | ((i & 0xFF00FF00) >> 8);
    i as f32 * 2.328_306_4e-10
}

/// Orthonormal basis around a unit vector.
pub fn basis(n: V) -> (V, V) {
    let up = if n[1].abs() < 0.999 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let t = norm(cross(up, n));
    (t, cross(n, t))
}

/// SH basis (real, order 2) in the usual order.
pub fn sh_basis(d: V) -> [f32; 9] {
    let (x, y, z) = (d[0], d[1], d[2]);
    [0.282095, 0.488603 * y, 0.488603 * z, 0.488603 * x, 1.092548 * x * y, 1.092548 * y * z, 0.315392 * (3.0 * z * z - 1.0), 1.092548 * x * z, 0.546274 * (x * x - y * y)]
}

impl Env {
    /// Build an environment from an equirectangular image of linear radiance. `max_width` caps the stored size (a power
    /// of two is chosen, at most 2048); the prefiltering cost grows with it.
    pub fn from_image(name: &str, src: &Tex, max_width: usize) -> Env {
        let mut base_w = 8usize;
        while base_w * 2 <= max_width.clamp(16, 2048) && base_w * 2 <= src.w.max(16) {
            base_w *= 2;
        }
        let base_h = base_w / 2;
        // resample the source to base_w x base_h (box filter when shrinking, bilinear otherwise); half-float safe
        let mut l0 = Level { w: src.w, h: src.h, px: src.px.iter().map(|p| [clean(p[0]), clean(p[1]), clean(p[2])]).collect() };
        while l0.w >= base_w * 2 && l0.h >= base_h * 2 {
            l0 = halve(&l0);
        }
        if l0.w != base_w || l0.h != base_h {
            let mut px = Vec::with_capacity(base_w * base_h);
            for y in 0..base_h {
                for x in 0..base_w {
                    px.push(l0.at((x as f32 + 0.5) / base_w as f32, (y as f32 + 0.5) / base_h as f32));
                }
            }
            l0 = Level { w: base_w, h: base_h, px };
        }
        let peak = l0.px.iter().map(|p| p[0].max(p[1]).max(p[2])).fold(0.0, f32::max);
        // solid-angle weighted mean luminance
        let mut mean = 0.0;
        let mut wsum = 0.0;
        for y in 0..l0.h {
            let wgt = ((y as f32 + 0.5) / l0.h as f32 * PI).sin();
            for x in 0..l0.w {
                mean += lum3(l0.px[y * l0.w + x]) * wgt;
                wsum += wgt;
            }
        }
        let mean = mean / wsum.max(1e-6);

        // plain mip chain of the source used as the input of the prefilter
        let mut mips = vec![l0.clone()];
        while mips.last().unwrap().w > 8 {
            let n = halve(mips.last().unwrap());
            mips.push(n);
        }
        let n_levels = mips.len();

        // spherical harmonics from a small version of the image
        let small = mips.iter().find(|m| m.w <= 64).unwrap_or_else(|| mips.last().unwrap());
        let mut coef = [[0.0f64; 3]; 9];
        let mut tot = 0.0f64;
        for y in 0..small.h {
            let v = (y as f32 + 0.5) / small.h as f32;
            let dw = (v * PI).sin() as f64;
            for x in 0..small.w {
                let d = uv_to_dir((x as f32 + 0.5) / small.w as f32, v);
                let b = sh_basis(d);
                let c = small.px[y * small.w + x];
                for i in 0..9 {
                    for k in 0..3 {
                        coef[i][k] += c[k] as f64 * b[i] as f64 * dw;
                    }
                }
                tot += dw;
            }
        }
        // integral over the sphere = sum * 4 pi / total weight; cosine lobe weights A_l / pi = 1, 2/3, 1/4
        let norm_k = 4.0 * std::f64::consts::PI / tot.max(1e-9);
        let a = [1.0, 2.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0, 0.25, 0.25, 0.25, 0.25, 0.25];
        let mut sh = [[0.0f32; 3]; 9];
        for i in 0..9 {
            for k in 0..3 {
                sh[i][k] = (coef[i][k] * norm_k * a[i]) as f32;
            }
        }

        // GGX prefiltered specular chain
        let mut levels = vec![l0];
        for k in 1..n_levels {
            let rough = k as f32 / (n_levels - 1) as f32;
            let (w, h) = (mips[k].w, mips[k].h);
            let samples = if k <= 2 { 96u32 } else { 64 };
            let alpha = (rough * rough).max(1e-3);
            let a2 = alpha * alpha;
            let mipref = &mips;
            let src_texel_sa = 4.0 * PI / (mips[0].w * mips[0].h) as f32;
            let px = crate::tex::par_map(w * h, |i| {
                let (x, y) = (i % w, i / w);
                let n = uv_to_dir((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
                let (t, bt) = basis(n);
                let mut acc = [0.0f32; 3];
                let mut wsum = 0.0f32;
                for s in 0..samples {
                    let (x1, x2) = ((s as f32 + 0.5) / samples as f32, radical_inverse(s));
                    let phi = TAU * x1;
                    let ct = ((1.0 - x2) / (1.0 + (a2 - 1.0) * x2)).max(0.0).sqrt();
                    let st = (1.0 - ct * ct).max(0.0).sqrt();
                    let hl = [st * phi.cos(), st * phi.sin(), ct];
                    let hv = add(add(mul(t, hl[0]), mul(bt, hl[1])), mul(n, hl[2]));
                    let l = sub(mul(hv, 2.0 * dot(n, hv)), n);
                    let ndl = dot(n, l);
                    if ndl <= 0.0 {
                        continue;
                    }
                    // pdf of the half vector with N = V: D(h) / 4
                    let dd = a2 / (PI * (ct * ct * (a2 - 1.0) + 1.0).powi(2)).max(1e-9);
                    let pdf = dd / 4.0;
                    let sa_sample = 1.0 / (samples as f32 * pdf).max(1e-9);
                    let lod = (0.5 * (sa_sample / src_texel_sa).max(1.0).log2() + 1.0).clamp(0.0, (mipref.len() - 1) as f32);
                    let (l0i, fr) = (lod.floor() as usize, lod - lod.floor());
                    let (u, v) = dir_to_uv(l);
                    let c0 = mipref[l0i].at(u, v);
                    let c = if fr > 0.0 && l0i + 1 < mipref.len() {
                        let c1 = mipref[l0i + 1].at(u, v);
                        [c0[0] + (c1[0] - c0[0]) * fr, c0[1] + (c1[1] - c0[1]) * fr, c0[2] + (c1[2] - c0[2]) * fr]
                    } else {
                        c0
                    };
                    for q in 0..3 {
                        acc[q] += c[q] * ndl;
                    }
                    wsum += ndl;
                }
                let inv = 1.0 / wsum.max(1e-6);
                [acc[0] * inv, acc[1] * inv, acc[2] * inv]
            });
            levels.push(Level { w, h, px });
        }
        Env { name: name.to_string(), levels, sh, peak, mean }
    }

    pub fn n_levels(&self) -> usize {
        self.levels.len()
    }

    /// Radiance seen in direction `d` at full resolution (the background).
    pub fn background(&self, d: V) -> V {
        let (u, v) = dir_to_uv(d);
        self.levels[0].at(u, v)
    }

    /// Radiance blurred as for a GGX lobe of perceptual roughness `rough` (0 sharp .. 1 fully rough) around `d`.
    pub fn specular(&self, d: V, rough: f32) -> V {
        let lod = rough.clamp(0.0, 1.0) * (self.levels.len() - 1) as f32;
        self.lod(d, lod)
    }

    pub fn lod(&self, d: V, lod: f32) -> V {
        let (u, v) = dir_to_uv(d);
        let i = (lod.floor() as usize).min(self.levels.len() - 1);
        let f = lod - i as f32;
        let a = self.levels[i].at(u, v);
        if f <= 0.0 || i + 1 >= self.levels.len() {
            return a;
        }
        let b = self.levels[i + 1].at(u, v);
        [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f]
    }

    /// Cosine-weighted mean radiance around the normal `n` (multiply by the albedo for diffuse light).
    pub fn irradiance(&self, n: V) -> V {
        let b = sh_basis(n);
        let mut o = [0.0f32; 3];
        for i in 0..9 {
            for k in 0..3 {
                o[k] += self.sh[i][k] * b[i];
            }
        }
        [o[0].max(0.0), o[1].max(0.0), o[2].max(0.0)]
    }

    /// Pick a direction with a probability proportional to its brightness, for next-event estimation. Returns the
    /// direction, the radiance and the probability density per unit solid angle.
    pub fn sampler(&self) -> EnvSampler {
        EnvSampler::new(self)
    }
}

fn clean(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 60000.0)
    } else {
        0.0
    }
}

/// Luminance importance sampling of the environment (a cumulative table over a 128 x 64 version of the image).
#[derive(Clone, Debug)]
pub struct EnvSampler {
    w: usize,
    h: usize,
    cdf: Vec<f32>,
    total: f32,
}

impl EnvSampler {
    fn new(env: &Env) -> EnvSampler {
        // box-filtered copy of the sharp level, about 256 wide
        let mut owned = env.levels[0].clone();
        while owned.w > 256 {
            owned = halve(&owned);
        }
        let lv = &owned;
        let (w, h) = (lv.w, lv.h);
        let mut cdf = Vec::with_capacity(w * h);
        let mut acc = 0.0f32;
        for y in 0..h {
            let s = ((y as f32 + 0.5) / h as f32 * PI).sin();
            for x in 0..w {
                acc += lum3(lv.px[y * w + x]).max(1e-6) * s;
                cdf.push(acc);
            }
        }
        EnvSampler { w, h, cdf, total: acc }
    }

    /// `r` uniform in [0, 1), `r2` uniform in [0, 1): direction and pdf (per steradian).
    pub fn sample(&self, r: f32, r2: f32) -> (V, f32) {
        let target = r * self.total;
        let idx = match self.cdf.binary_search_by(|c| c.partial_cmp(&target).unwrap_or(std::cmp::Ordering::Less)) {
            Ok(i) | Err(i) => i.min(self.cdf.len() - 1),
        };
        let (x, y) = (idx % self.w, idx / self.w);
        let (u, v) = ((x as f32 + r2.fract().abs()) / self.w as f32, (y as f32 + 0.5) / self.h as f32);
        let d = uv_to_dir(u, v);
        (d, self.pdf(d))
    }

    pub fn pdf(&self, d: V) -> f32 {
        let (u, v) = dir_to_uv(d);
        let (x, y) = (((u * self.w as f32) as usize).min(self.w - 1), ((v * self.h as f32) as usize).min(self.h - 1));
        let i = y * self.w + x;
        let p = self.cdf[i] - if i > 0 { self.cdf[i - 1] } else { 0.0 };
        let s = ((y as f32 + 0.5) / self.h as f32 * PI).sin().max(1e-6);
        // probability of the texel / its solid angle
        let sa = (2.0 * PI / self.w as f32) * (PI / self.h as f32) * s;
        p / self.total / sa
    }
}

// ------------------------------------------------------------------------------------------------ presets
pub const PRESETS: &[(&str, &str)] = &[
    ("studio", "Studio softboxes"),
    ("sky", "Clear sky"),
    ("sunset", "Sunset"),
    ("overcast", "Overcast"),
    ("night", "Night"),
    ("grey", "Neutral grey"),
    ("furnace", "White furnace (test)"),
];

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp3(a: V, b: V, t: f32) -> V {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Radiance of a built-in environment in direction `d`.
fn preset_radiance(name: &str, d: V) -> V {
    match name {
        "sky" => {
            let sun = norm([0.45, 0.62, -0.45]);
            let t = d[1].clamp(-1.0, 1.0);
            let sky = lerp3([0.78, 0.86, 1.0], [0.18, 0.38, 0.85], t.max(0.0).powf(0.5));
            let ground = lerp3([0.42, 0.40, 0.36], [0.20, 0.19, 0.17], (-t).max(0.0).powf(0.5));
            let mut c = if t >= 0.0 { sky } else { ground };
            let hz = (-(t / 0.06).powi(2)).exp();
            c = lerp3(c, [0.9, 0.92, 0.95], hz * 0.5);
            let cs = dot(d, sun);
            // sun disc (about 0.5 degree radius is too small for the 256 x 128 maps, so it is widened) and glow
            c = add(c, mul([1.0, 0.93, 0.8], 600.0 * smooth(0.99935, 0.9999, cs)));
            c = add(c, mul([1.0, 0.85, 0.6], 0.6 * cs.max(0.0).powi(48)));
            c
        }
        "sunset" => {
            let sun = norm([-0.6, 0.06, -0.78]);
            let t = d[1].clamp(-1.0, 1.0);
            let cs = dot(d, sun).max(0.0);
            let top = [0.10, 0.16, 0.38];
            let hor = [1.0, 0.45, 0.16];
            let sky = lerp3(hor, top, t.max(0.0).powf(0.45));
            let ground = lerp3([0.25, 0.14, 0.10], [0.05, 0.04, 0.04], (-t).max(0.0).powf(0.4));
            let mut c = if t >= 0.0 { sky } else { ground };
            c = add(c, mul([1.0, 0.55, 0.2], 1.4 * cs.powi(6)));
            c = add(c, mul([1.0, 0.8, 0.5], 300.0 * smooth(0.9992, 0.9999, dot(d, sun))));
            c
        }
        "overcast" => {
            let t = d[1].clamp(-1.0, 1.0);
            let k = 0.55 + 0.45 * t.max(0.0);
            let sky = [0.62 * k * 1.5, 0.66 * k * 1.5, 0.72 * k * 1.5];
            let ground = [0.22, 0.22, 0.21];
            if t >= 0.0 { lerp3(ground, sky, smooth(0.0, 0.1, t)) } else { ground }
        }
        "night" => {
            let t = d[1].clamp(-1.0, 1.0);
            let sky = lerp3([0.015, 0.02, 0.045], [0.004, 0.006, 0.02], t.max(0.0).powf(0.5));
            let ground = [0.006, 0.006, 0.008];
            let moon = norm([0.3, 0.55, -0.78]);
            let mut c = if t >= 0.0 { sky } else { ground };
            c = add(c, mul([0.7, 0.8, 1.0], 6.0 * smooth(0.9993, 0.9998, dot(d, moon))));
            c = add(c, mul([0.2, 0.28, 0.45], 0.08 * dot(d, moon).max(0.0).powi(24)));
            c
        }
        "grey" => [0.5, 0.5, 0.5],
        "furnace" => [1.0, 1.0, 1.0],
        _ => {
            // studio: a soft gradient room with a big key softbox, a tall side strip, a top panel and a horizon line
            let t = d[1];
            let sky = [0.30, 0.34, 0.42];
            let hor = [0.52, 0.52, 0.53];
            let ground = [0.20, 0.19, 0.18];
            let mut c = if t > 0.0 { lerp3(hor, sky, t.powf(0.6)) } else { lerp3(hor, ground, (-t).powf(0.5)) };
            let key = norm([0.5, 0.62, 0.6]);
            let kb = smooth(0.90, 0.93, dot(d, key));
            c = add(c, mul([1.0, 0.96, 0.90], 9.0 * kb));
            let side = norm([-key[2], 0.0, key[0]]);
            let hd = norm([d[0], 1e-5, d[2]]);
            let strip = smooth(0.93, 0.97, dot(hd, side)) * smooth(0.02, 0.12, t) * (1.0 - smooth(0.55, 0.75, t));
            c = add(c, mul([0.85, 0.92, 1.0], 3.0 * strip));
            c = add(c, mul([1.0, 1.0, 1.0], 2.0 * smooth(0.90, 0.97, t)));
            c = add(c, mul([0.9, 0.9, 0.9], 0.3 * (-(t / 0.035).powi(2)).exp()));
            c
        }
    }
}

/// Render a built-in environment to an equirectangular image of the given width (height = width / 2).
pub fn preset_image(name: &str, width: usize) -> Tex {
    let (w, h) = (width.max(16), (width / 2).max(8));
    Tex::from_fn(w, h, |u, v| {
        let c = preset_radiance(name, uv_to_dir(u, v));
        [c[0], c[1], c[2], 1.0]
    })
}

/// A built-in environment by name (see [`PRESETS`]); unknown names give the studio.
pub fn preset(name: &str) -> Env {
    let known = PRESETS.iter().any(|p| p.0 == name);
    let nm = if known { name } else { "studio" };
    // the sun and softboxes need some resolution to stay sharp; the prefilter makes the rest cheap
    Env::from_image(nm, &preset_image(nm, 512), 512)
}

/// A uniform environment of the given colour (tests, flat lighting).
pub fn uniform(color: V) -> Env {
    Env::from_image("uniform", &Tex::from_fn(64, 32, |_, _| [color[0], color[1], color[2], 1.0]), 64)
}

/// A loaded environment, shared and cached by name / path (the prefilter is the expensive part). `file` wins over
/// `preset`; a file that cannot be read is an error, an unknown preset name gives the studio.
pub fn load_env(preset_name: &str, file: Option<&str>) -> Result<std::sync::Arc<Env>, String> {
    use std::sync::{Arc, Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<Vec<(String, Arc<Env>)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(Vec::new()));
    let key = match file {
        Some(f) => {
            let stamp = std::fs::metadata(f).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
            format!("file:{f}:{stamp}")
        }
        None => format!("preset:{preset_name}"),
    };
    if let Some((_, e)) = cache.lock().unwrap().iter().find(|(k, _)| *k == key) {
        return Ok(e.clone());
    }
    let env = match file {
        Some(f) => {
            let img = load_image(std::path::Path::new(f))?;
            let name = std::path::Path::new(f).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            Env::from_image(&name, &img, 1024)
        }
        None => preset(preset_name),
    };
    let env = Arc::new(env);
    let mut c = cache.lock().unwrap();
    if c.len() >= 6 {
        c.remove(0);
    }
    c.push((key, env.clone()));
    Ok(env)
}
