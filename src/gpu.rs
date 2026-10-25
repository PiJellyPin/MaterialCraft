// SPDX-License-Identifier: MIT OR Apache-2.0
//! GPU compute engine (optional `gpu` feature, built on wgpu compute shaders).
//!
//! The CPU code in `ops.rs` / `noise.rs` is the reference. A node is only sent to the GPU when this module has a
//! kernel for it, the parameters are inside what the kernel supports, the texture is big enough to be worth the
//! upload and read-back, and a usable adapter exists. In every other case the caller gets `None` and runs the CPU
//! code, so results never depend on whether a GPU is present (apart from float rounding, which the parity tests
//! bound). Without the feature every function here returns `None`.
//!
//! Environment: `MC_GPU=0` turns the engine off, `MC_GPU=force` also accepts software adapters (used by the tests),
//! `MC_GPU_MIN` sets the smallest pixel count that is sent to the GPU (default 262144 = 512 x 512).

use crate::tex::Tex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(true);
/// Set when the driver reported an error that cannot be traced to one call; the engine stays off from then on.
static BROKEN: AtomicBool = AtomicBool::new(false);
static MIN_PIXELS: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Turn GPU evaluation on or off at run time (the editor's switch).
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Smallest texture (in pixels) worth sending to the GPU. 0 sends everything (tests).
pub fn set_min_pixels(n: usize) {
    MIN_PIXELS.store(n, Ordering::Relaxed);
}

fn min_pixels() -> usize {
    let v = MIN_PIXELS.load(Ordering::Relaxed);
    if v != usize::MAX {
        return v;
    }
    std::env::var("MC_GPU_MIN").ok().and_then(|s| s.parse().ok()).unwrap_or(512 * 512)
}

/// True when evaluation may use the GPU right now (part of the node cache key: the two paths differ by rounding).
pub fn active() -> bool {
    ENABLED.load(Ordering::Relaxed) && !BROKEN.load(Ordering::Relaxed) && imp::global().is_some()
}

/// Name of the adapter in use, if any.
pub fn adapter_name() -> Option<String> {
    imp::global().map(|g| g.name())
}

fn usable(t_w: usize, t_h: usize) -> bool {
    ENABLED.load(Ordering::Relaxed) && !BROKEN.load(Ordering::Relaxed) && t_w * t_h >= min_pixels() && imp::global().is_some()
}

/// Perlin (`fractal == false`) or fractal noise on the GPU. Returns a grayscale texture, or `None` for the CPU path.
#[allow(clippy::too_many_arguments)]
pub fn noise(w: usize, h: usize, fractal: bool, px: i32, py: i32, octaves: u32, gain: f32, seed: u32, mode: u32) -> Option<Tex> {
    if !usable(w, h) {
        return None;
    }
    imp::global()?.noise(w, h, fractal, px, py, octaves, gain, seed, mode)
}

/// Gaussian blur with wrap-around edges (the three-box approximation used for sigma >= 2.5).
pub fn blur(t: &Tex, sigma: f32) -> Option<Tex> {
    if !usable(t.w, t.h) || sigma < 2.5 {
        return None;
    }
    imp::global()?.blur(t, sigma)
}

/// Normal map from a height map (3x3 Scharr, wrap-around edges).
pub fn normal_from_height(t: &Tex, strength: f32, directx: bool) -> Option<Tex> {
    if !usable(t.w, t.h) {
        return None;
    }
    imp::global()?.normal(t, strength, directx)
}

/// Levels (input range, gamma, output range) on the GPU.
pub fn levels(t: &Tex, in_lo: f32, in_hi: f32, gamma: f32, out_lo: f32, out_hi: f32) -> Option<Tex> {
    if !usable(t.w, t.h) {
        return None;
    }
    imp::global()?.levels(t, in_lo, in_hi, gamma, out_lo, out_hi)
}

/// Directional blur (24 taps along a line, wrap-around, bilinear).
pub fn directional_blur(t: &Tex, angle: f32, length: f32) -> Option<Tex> {
    if !usable(t.w, t.h) {
        return None;
    }
    imp::global()?.directional_blur(t, angle, length)
}

/// Blend of `fg` over `bg` for the colour blend modes (not Dissolve), with the "ignore alpha" and "use source alpha"
/// alpha modes and no cropping; every input must have the same size. `None` leaves the other cases to the CPU.
pub fn blend(fg: &Tex, bg: &Tex, mask: Option<&Tex>, mode: u32, opacity: f32, alpha_mode: u32) -> Option<Tex> {
    if !usable(bg.w, bg.h) || mode == 19 || alpha_mode > 3 {
        return None;
    }
    if (fg.w, fg.h) != (bg.w, bg.h) || mask.map(|m| (m.w, m.h) != (bg.w, bg.h)).unwrap_or(false) {
        return None;
    }
    imp::global()?.blend(fg, bg, mask, mode, opacity, alpha_mode)
}

/// Spin (`mode` 0, `amt` = arc in radians) or zoom (`mode` 1, `amt` = amount) blur around the centre (`cx`, `cy`).
#[allow(clippy::too_many_arguments)]
pub fn radial_blur(t: &Tex, mode: u32, taps: u32, cx: f32, cy: f32, amt: f32, rad: f32) -> Option<Tex> {
    if !usable(t.w, t.h) {
        return None;
    }
    imp::global()?.radial_blur(t, mode, taps, cx, cy, amt, rad)
}

/// Horizon-based bent normal (`want_ao` false) or occlusion (`want_ao` true) of a height map.
#[allow(clippy::too_many_arguments)]
pub fn bent_normal(t: &Tex, dirs: u32, steps: u32, rad: f32, relief: f32, directx: bool, want_ao: bool) -> Option<Tex> {
    if !usable(t.w, t.h) {
        return None;
    }
    imp::global()?.bent_normal(t, dirs, steps, rad, relief, directx, want_ao)
}

/// Edge-preserving blur of `t` guided by `guide` (same size).
pub fn bilateral_blur(t: &Tex, guide: &Tex, rad: i32, range: f32) -> Option<Tex> {
    if !usable(t.w, t.h) || (guide.w, guide.h) != (t.w, t.h) || rad < 1 {
        return None;
    }
    imp::global()?.bilateral_blur(t, guide, rad, range)
}

/// Blur of `t` along the gradient of `slope` (same size); mode 0 average, 1 minimum, 2 maximum.
pub fn slope_blur(t: &Tex, slope: &Tex, samples: u32, intensity: f32, mode: u32) -> Option<Tex> {
    if !usable(t.w, t.h) || (slope.w, slope.h) != (t.w, t.h) {
        return None;
    }
    imp::global()?.slope_blur(t, slope, samples, intensity, mode)
}

/// Iterated extrusion: `steps` passes of `cur = max(m, sample(cur, p - (dx, dy)) - drop)` starting from the luminance of
/// `t`; returns the final plane as a grey texture.
pub fn extrude(t: &Tex, steps: u32, dx: f32, dy: f32, drop: f32) -> Option<Tex> {
    if !usable(t.w, t.h) || steps == 0 {
        return None;
    }
    imp::global()?.extrude(t, steps, dx, dy, drop)
}

/// How a warp reads its displacement from the map `w`.
#[derive(Clone, Copy)]
pub enum WarpKind {
    /// gradient of the map's luminance (central differences) times the intensity
    Slope,
    /// `(lum - offset) * intensity` along the direction (`c`, `s`)
    Directional { c: f32, s: f32, offset: f32 },
    /// red and green as a vector; `gs` flips the green axis
    Vector { gs: f32 },
}

/// Displace `t` by the map `w` (same size). `filtering`: 0 bilinear, 1 nearest, 2 bicubic.
pub fn warp(t: &Tex, w: &Tex, kind: WarpKind, intensity: f32, filtering: u32) -> Option<Tex> {
    if !usable(t.w, t.h) || (w.w, w.h) != (t.w, t.h) {
        return None;
    }
    imp::global()?.warp(t, w, kind, intensity, filtering)
}

/// Streak noise (see the directional noise node): `taps` samples along the direction (`ca`, `sa`), streak length `len`.
#[allow(clippy::too_many_arguments)]
pub fn directional_noise(w: usize, h: usize, p1: i32, taps: u32, ca: f32, sa: f32, len: f32, gain: f32, seed: u32) -> Option<Tex> {
    if !usable(w, h) {
        return None;
    }
    imp::global()?.dir_noise(w, h, p1, taps, ca, sa, len, gain, seed)
}

// ---------------------------------------------------------------------------------------------------------------
// Device-resident pictures. A `GTex` lives in GPU memory; the `*_g` functions read and write them without a trip
// through the CPU, so a chain of GPU nodes only touches host memory when something needs the pixels (a CPU node,
// the 3D view, a file export). `None` always means "use the CPU code".
// ---------------------------------------------------------------------------------------------------------------

/// A picture held in GPU memory (cheap to clone; the memory is freed with the last clone).
#[derive(Clone)]
pub struct GTex(std::sync::Arc<imp::GBuf>);

impl GTex {
    pub fn w(&self) -> usize {
        self.0.w
    }
    pub fn h(&self) -> usize {
        self.0.h
    }
    pub fn bytes(&self) -> usize {
        self.0.w * self.0.h * 16
    }
}

/// How many times pictures were read back from the device (each call is one wait, however many pictures).
pub fn download_count() -> usize {
    imp::downloads()
}

/// How many kernel launches ran on the device (each is one command submission).
pub fn submit_count() -> usize {
    imp::submits()
}

/// Device memory currently held by pictures, in bytes.
pub fn resident_bytes() -> u64 {
    imp::live_bytes()
}

/// True when pictures of this size can live on the device right now (switch on, adapter present, fits one buffer).
pub fn can_hold(w: usize, h: usize) -> bool {
    ENABLED.load(Ordering::Relaxed) && !BROKEN.load(Ordering::Relaxed) && imp::global().map(|g| g.fits(w, h)).unwrap_or(false)
}

/// True when a node of this size is big enough to be worth sending to the device on its own (see `MC_GPU_MIN`).
pub fn worth_it(w: usize, h: usize) -> bool {
    usable(w, h)
}

/// Copies a picture to the device.
pub fn upload(t: &Tex) -> Option<GTex> {
    if !ENABLED.load(Ordering::Relaxed) || BROKEN.load(Ordering::Relaxed) {
        return None;
    }
    imp::global()?.upload(t).map(|g| GTex(std::sync::Arc::new(g)))
}

/// Reads pictures back with one submission and one wait.
pub fn download(ts: &[&GTex]) -> Option<Vec<Tex>> {
    let refs: Vec<&imp::GBuf> = ts.iter().map(|t| &*t.0).collect();
    imp::global()?.download(&refs)
}

/// The 8-bit thumbnail (`size` x `size`, RGBA) of a device picture, fitted with dark bars like the CPU one.
pub fn thumb_g(t: &GTex, size: usize) -> Option<Vec<u8>> {
    imp::global()?.thumb_g(&t.0, size)
}

fn wrap(g: Option<imp::GBuf>) -> Option<GTex> {
    g.map(|g| GTex(std::sync::Arc::new(g)))
}

/// Solid colour picture on the device.
pub fn solid_g(w: usize, h: usize, rgba: [f32; 4]) -> Option<GTex> {
    wrap(imp::global()?.solid_g(w, h, rgba))
}

/// Invert the colour channels (alpha is kept).
pub fn invert_g(t: &GTex) -> Option<GTex> {
    wrap(imp::global()?.pointwise_g(&t.0, 0, 0.0, 0.0, 0.0, 0.0))
}

/// `((c - 0.5) * contrast + 0.5)` clamped, then optionally inverted (the generators' Contrast / Invert settings).
pub fn contrast_invert_g(t: &GTex, contrast: f32, invert: bool) -> Option<GTex> {
    wrap(imp::global()?.pointwise_g(&t.0, 1, contrast, invert as u32 as f32, 0.0, 0.0))
}

/// Kernel families that run through `kernel_g` (their WGSL lives in `src/kernels`, built by `tools/gen_kernels.py`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kern {
    /// colour, channel, normal, mask and combine operations (`op` picks one)
    OpsA,
    /// gradient map with up to 32 stops
    Colorize,
    /// pattern and noise generators (no inputs; `n` is the seed, parameter 24.. hold Disorder, Contrast and Invert)
    Gen,
    /// the second set of generators
    Gen2,
    /// neighbourhood filters, transforms and warps (`op` picks one)
    OpsB,
    /// material utilities and mask recipes (`op` picks one)
    OpsC,
    /// effect filters (cross section, emboss, safe transform, weathering, snow, season, water, summed area table)
    OpsD,
    /// finishes the instance records of a tile grid with the map inputs
    TileFin,
    /// draws pattern instances (tile grids, splatter, circular splatter)
    Scatter,
    /// signed distance by jump flooding (distance, bevel, shape stroke / glow)
    Jfa,
    /// resamples a picture to another size (box average for whole reductions, bilinear otherwise)
    Resize,
}

#[allow(dead_code)]
impl Kern {
    pub(crate) const MINMAX_SRC: &'static str = include_str!("kernels/parts/minmax.wgsl");

    fn name(self) -> &'static str {
        match self {
            Kern::OpsA => "k_ops_a",
            Kern::Colorize => "k_colorize",
            Kern::Gen => "k_gen",
            Kern::Gen2 => "k_gen2",
            Kern::OpsB => "k_ops_b",
            Kern::OpsC => "k_ops_c",
            Kern::OpsD => "k_ops_d",
            Kern::TileFin => "k_tile_fin",
            Kern::Scatter => "k_scatter",
            Kern::Jfa => "k_jfa",
            Kern::Resize => "k_resize",
        }
    }
    fn src(self) -> &'static str {
        match self {
            Kern::OpsA => include_str!("kernels/ops_a.wgsl"),
            Kern::Colorize => include_str!("kernels/colorize.wgsl"),
            Kern::Gen => include_str!("kernels/gen.wgsl"),
            Kern::Gen2 => include_str!("kernels/gen2.wgsl"),
            Kern::OpsB => include_str!("kernels/ops_b.wgsl"),
            Kern::OpsC => include_str!("kernels/ops_c.wgsl"),
            Kern::OpsD => include_str!("kernels/ops_d.wgsl"),
            Kern::TileFin => include_str!("kernels/tile_fin.wgsl"),
            Kern::Scatter => include_str!("kernels/scatter.wgsl"),
            Kern::Jfa => include_str!("kernels/jfa.wgsl"),
            Kern::Resize => include_str!("kernels/resize.wgsl"),
        }
    }
    /// pictures bound to the kernel
    fn n_inputs(self) -> usize {
        match self {
            Kern::OpsA | Kern::Colorize | Kern::OpsB | Kern::OpsC | Kern::OpsD | Kern::Scatter | Kern::Jfa => 4,
            Kern::TileFin => 7,
            Kern::Resize => 1,
            Kern::Gen | Kern::Gen2 => 0,
        }
    }
    /// how many of the leading inputs must have the result's size
    fn strict(self) -> usize {
        match self {
            Kern::OpsA | Kern::Colorize | Kern::OpsB | Kern::OpsC | Kern::OpsD | Kern::Jfa => 3,
            Kern::Gen | Kern::Gen2 | Kern::TileFin | Kern::Scatter | Kern::Resize => 0,
        }
    }
    /// 16-byte groups of parameters in the uniform block
    fn p_vec4(self) -> usize {
        match self {
            Kern::OpsA | Kern::Colorize | Kern::OpsB | Kern::OpsC | Kern::OpsD | Kern::Jfa | Kern::Resize => 4,
            Kern::Gen | Kern::Gen2 | Kern::TileFin | Kern::Scatter => 8,
        }
    }
    /// 16-byte groups of table values after the parameters
    fn x_vec4(self) -> usize {
        match self {
            Kern::Colorize => 64,
            _ => 0,
        }
    }
}

/// Operation numbers of the `Kern::OpsA` kernel (`src/kernels/parts/ops_a.wgsl`).
pub mod opa {
    pub const HSL: u32 = 0;
    pub const ADJUST: u32 = 1;
    pub const CONTRAST_LUM: u32 = 2;
    pub const SRGB_TO_LIN: u32 = 3;
    pub const LIN_TO_SRGB: u32 = 4;
    pub const PREMUL: u32 = 5;
    pub const GRAYSCALE: u32 = 6;
    pub const MIXER: u32 = 7;
    pub const COLOR_TO_MASK: u32 = 8;
    pub const REPLACE: u32 = 9;
    pub const CHROMA: u32 = 10;
    pub const NORMAL_INVERT: u32 = 11;
    pub const NORMAL_NORM: u32 = 12;
    pub const HIST_RANGE: u32 = 13;
    pub const ALPHA_MERGE: u32 = 14;
    pub const MERGE: u32 = 15;
    pub const SHUFFLE: u32 = 16;
    pub const NORMAL_ROT: u32 = 17;
    pub const NORMAL_COMBINE: u32 = 18;
    pub const NORMAL_BLEND: u32 = 19;
    pub const ALPHA_SPLIT: u32 = 20;
    pub const AUTO_LEVELS: u32 = 21;
    pub const LEVELS_AUTO: u32 = 22;
    pub const HEIGHT_COMBINE: u32 = 23;
    pub const AGAINST_BLUR: u32 = 24;
    pub const FORMAT: u32 = 25;
}

/// Operation numbers of the `Kern::OpsB` kernel (`src/kernels/parts/ops_b.wgsl`).
pub mod opb {
    pub const EMBOSS: u32 = 0;
    pub const EMBOSS_LIT: u32 = 1;
    pub const EDGE: u32 = 2;
    pub const SWIRL: u32 = 3;
    pub const TRANSFORM: u32 = 4;
    pub const MIRROR: u32 = 5;
    pub const SEAM: u32 = 6;
    pub const SOBEL: u32 = 7;
    pub const POLAR: u32 = 8;
    pub const SYM_SLICE: u32 = 9;
    pub const HOMOGRAPHY: u32 = 10;
    pub const MIN_MAX: u32 = 11;
    pub const FREQ_MERGE: u32 = 12;
    pub const FREQ_HIGH: u32 = 13;
    pub const CLONE_PATCH: u32 = 14;
    pub const SYMMETRY: u32 = 16;
    pub const SKEW: u32 = 17;
    pub const MOSAIC: u32 = 18;
    pub const NU_DIR_WARP: u32 = 19;
    pub const MULTI_DIR_WARP: u32 = 20;
    pub const NU_BLUR_ACC: u32 = 21;
    pub const NORMAL_FACING: u32 = 22;
    pub const H2N_WORLD: u32 = 23;
    pub const NORMAL_SOBEL: u32 = 24;
    pub const NORMAL_ROT: u32 = 25;
    pub const HEIGHT_NORMAL_BLEND: u32 = 26;
    pub const SHIFT: u32 = 27;
    pub const DROP_SHADOW: u32 = 28;
}

/// Operation numbers of the `Kern::OpsC` kernel (`src/kernels/parts/ops_c.wgsl`).
pub mod opc {
    pub const MAT_WEIGHT: u32 = 0;
    pub const LERP_COLOR: u32 = 1;
    pub const LERP_NORMAL: u32 = 2;
    pub const LERP_GREY: u32 = 3;
    pub const METAL_TO_SPEC: u32 = 4;
    pub const SPEC_TO_METAL: u32 = 5;
    pub const LEAK: u32 = 6;
    pub const DUST: u32 = 7;
    pub const TILE_MIRROR: u32 = 8;
    pub const HEIGHT_BLEND: u32 = 9;
    pub const TILE_PHOTO: u32 = 10;
    pub const MUL_LUM: u32 = 11;
    pub const PACK4: u32 = 12;
    pub const CURVE: u32 = 13;
}

/// One launch of a kernel family on device pictures (see `Kern`). `None` = not possible here, use the CPU code.
#[allow(clippy::too_many_arguments)]
pub fn kernel_g(k: Kern, op: u32, n: u32, p: &[f32], x: &[f32], ins: &[&GTex], size: (usize, usize)) -> Option<GTex> {
    let refs: Vec<&imp::GBuf> = ins.iter().map(|t| &*t.0).collect();
    wrap(imp::global()?.kernel_g(k, op, n, p, x, &refs, size))
}

/// Minimum and maximum of the colour channels and of the luminance, as a 2 x 1 picture (minimum first).
pub fn minmax_g(t: &GTex) -> Option<GTex> {
    wrap(imp::global()?.minmax_g(&t.0))
}

/// Rounds a device picture to an explicit output format (1 = 8-bit, 2 = 16-bit integer, 3 = half float).
pub fn format_g(t: &GTex, fmt: u32) -> Option<GTex> {
    kernel_g(Kern::OpsA, opa::FORMAT, 0, &[fmt as f32], &[], &[t], (t.w(), t.h()))
}

/// Resamples a device picture to `w` x `h` (the host's `ops::resize_to`).
pub fn resize_g(t: &GTex, w: usize, h: usize) -> Option<GTex> {
    if (t.w(), t.h()) == (w, h) {
        return Some(t.clone());
    }
    kernel_g(Kern::Resize, 0, 0, &[t.w() as f32, t.h() as f32], &[], &[t], (w, h))
}

/// Flood-fill family on the device: connected regions (4-neighbours, wrapping) of `lum > thr` (`mode` 0) or of touching
/// pixels with equal region id (`mode` 1). `out`: 11 flood fill result, 12 region index, 13 region centre, 14 size.
pub fn flood_g(t: &GTex, mode: u32, thr: f32, out: u32) -> Option<GTex> {
    wrap(imp::global()?.flood_g(&t.0, mode, thr, out))
}

/// Whole-picture statistics on the device: `op` 0 equalising curves (1024 x 1), 1 bounding box of bright pixels (1 x 1).
pub fn stats_g(t: &GTex, op: u32, thr: f32) -> Option<GTex> {
    wrap(imp::global()?.stats_g(&t.0, op, thr))
}

/// The same launch on host pictures (whole-picture API for tests).
#[allow(clippy::too_many_arguments)]
pub fn kernel(k: Kern, op: u32, n: u32, p: &[f32], x: &[f32], ins: &[&Tex], size: (usize, usize)) -> Option<Tex> {
    if !usable(size.0, size.1) {
        return None;
    }
    let ups: Vec<GTex> = ins.iter().map(|t| upload(t)).collect::<Option<Vec<_>>>()?;
    let refs: Vec<&GTex> = ups.iter().collect();
    let out = kernel_g(k, op, n, p, x, &refs, size)?;
    download(&[&out]).and_then(|mut v| v.pop())
}

/// Per-pixel operations on the device (see the kernel for the meaning of `a`..`d` per `op`).
#[derive(Clone, Copy, Debug)]
pub enum Point {
    /// grey value of the luminance against a level: level, softness, mode (0 >, 1 >=, 2 <, 3 <=)
    Threshold(f32, f32, u32),
    /// clamp the colour (and the alpha if asked)
    Clamp(f32, f32, bool),
    Pow(f32),
    ToGray,
    /// `steps - 1`
    Quantize(f32),
    Tint(f32, f32, f32),
    /// low edge and width of the ramp, inverted or not
    HistogramScan(f32, f32, bool),
    /// one channel (0..3) as grey
    Channel(u32),
    HistogramShift(f32),
}

/// A per-pixel operation on a picture that is on the device.
pub fn point_g(t: &GTex, op: Point) -> Option<GTex> {
    let (code, a, b, c) = match op {
        Point::Threshold(l, s, m) => (3, l, s, m as f32),
        Point::Clamp(lo, hi, alpha) => (4, lo, hi, alpha as u32 as f32),
        Point::Pow(e) => (5, e, 0.0, 0.0),
        Point::ToGray => (6, 0.0, 0.0, 0.0),
        Point::Quantize(s) => (7, s, 0.0, 0.0),
        Point::Tint(r, g, b) => (8, r, g, b),
        Point::HistogramScan(lo, w, inv) => (9, lo, w, inv as u32 as f32),
        Point::Channel(c) => (10, c as f32, 0.0, 0.0),
        Point::HistogramShift(d) => (11, d, 0.0, 0.0),
    };
    wrap(imp::global()?.pointwise_g(&t.0, code, a, b, c, 0.0))
}

/// The same on a host picture (whole-picture API for tests).
pub fn point(t: &Tex, op: Point) -> Option<Tex> {
    if !usable(t.w, t.h) {
        return None;
    }
    let g = upload(t)?;
    download(&[&point_g(&g, op)?]).and_then(|mut v| v.pop())
}

#[allow(clippy::too_many_arguments)]
pub fn noise_g(w: usize, h: usize, fractal: bool, px: i32, py: i32, octaves: u32, gain: f32, seed: u32, mode: u32) -> Option<GTex> {
    wrap(imp::global()?.noise_g(w, h, fractal, px, py, octaves, gain, seed, mode))
}

#[allow(clippy::too_many_arguments)]
pub fn directional_noise_g(w: usize, h: usize, p1: i32, taps: u32, ca: f32, sa: f32, len: f32, gain: f32, seed: u32) -> Option<GTex> {
    wrap(imp::global()?.dir_noise_g(w, h, p1, taps, ca, sa, len, gain, seed))
}

/// Gaussian blur (three box passes); `None` for sigma below 2.5, which the CPU does exactly.
pub fn blur_g(t: &GTex, sigma: f32) -> Option<GTex> {
    if sigma < 2.5 {
        return None;
    }
    wrap(imp::global()?.blur_g(&t.0, sigma))
}

pub fn normal_g(t: &GTex, strength: f32, directx: bool) -> Option<GTex> {
    wrap(imp::global()?.normal_g(&t.0, strength, directx))
}

pub fn levels_g(t: &GTex, in_lo: f32, in_hi: f32, gamma: f32, out_lo: f32, out_hi: f32) -> Option<GTex> {
    wrap(imp::global()?.levels_g(&t.0, in_lo, in_hi, gamma, out_lo, out_hi))
}

pub fn directional_blur_g(t: &GTex, angle: f32, length: f32) -> Option<GTex> {
    wrap(imp::global()?.directional_blur_g(&t.0, angle, length))
}

/// Same restrictions as `blend`: all inputs one size, not Dissolve, alpha mode 0..=3.
pub fn blend_g(fg: &GTex, bg: &GTex, mask: Option<&GTex>, mode: u32, opacity: f32, alpha_mode: u32) -> Option<GTex> {
    if mode == 19 || alpha_mode > 3 || (fg.w(), fg.h()) != (bg.w(), bg.h()) || mask.map(|m| (m.w(), m.h()) != (bg.w(), bg.h())).unwrap_or(false) {
        return None;
    }
    wrap(imp::global()?.blend_g(&fg.0, &bg.0, mask.map(|m| &*m.0), mode, opacity, alpha_mode))
}

#[allow(clippy::too_many_arguments)]
pub fn radial_blur_g(t: &GTex, mode: u32, taps: u32, cx: f32, cy: f32, amt: f32, rad: f32) -> Option<GTex> {
    wrap(imp::global()?.radial_blur_g(&t.0, mode, taps, cx, cy, amt, rad))
}

#[allow(clippy::too_many_arguments)]
pub fn bent_normal_g(t: &GTex, dirs: u32, steps: u32, rad: f32, relief: f32, directx: bool, want_ao: bool) -> Option<GTex> {
    wrap(imp::global()?.bent_normal_g(&t.0, dirs, steps, rad, relief, directx, want_ao))
}

pub fn bilateral_blur_g(t: &GTex, guide: &GTex, rad: i32, range: f32) -> Option<GTex> {
    if (guide.w(), guide.h()) != (t.w(), t.h()) || rad < 1 {
        return None;
    }
    wrap(imp::global()?.bilateral_blur_g(&t.0, &guide.0, rad, range))
}

pub fn slope_blur_g(t: &GTex, slope: &GTex, samples: u32, intensity: f32, mode: u32) -> Option<GTex> {
    if (slope.w(), slope.h()) != (t.w(), t.h()) {
        return None;
    }
    wrap(imp::global()?.slope_blur_g(&t.0, &slope.0, samples, intensity, mode))
}

pub fn warp_g(t: &GTex, w: &GTex, kind: WarpKind, intensity: f32, filtering: u32) -> Option<GTex> {
    if (w.w(), w.h()) != (t.w(), t.h()) {
        return None;
    }
    wrap(imp::global()?.warp_g(&t.0, &w.0, kind, intensity, filtering))
}

pub fn extrude_g(t: &GTex, steps: u32, dx: f32, dy: f32, drop: f32) -> Option<GTex> {
    wrap(imp::global()?.extrude_g(&t.0, steps, dx, dy, drop))
}


/// Horizon-based ambient occlusion (the AO node's HBAO mode) of the luminance of `t`, on the device. `radius_uv`,
/// `scale`, `dirs` and `intensity` mean what they do for `ops::hbao`.
pub fn hbao_g(t: &GTex, radius_uv: f32, scale: f32, dirs: u32, intensity: f32) -> Option<GTex> {
    let dirs = dirs.clamp(2, 64);
    let table = crate::ops::hbao_tap_table(t.w(), t.h(), radius_uv, dirs);
    wrap(imp::global()?.hbao_g(&t.0, &table, dirs, crate::ops::HBAO_STEPS as u32, scale, intensity))
}

/// The same on a host picture (whole-picture API for tests and callers without a device chain).
pub fn hbao(t: &Tex, radius_uv: f32, scale: f32, dirs: u32, intensity: f32) -> Option<Tex> {
    if !usable(t.w, t.h) {
        return None;
    }
    let dirs = dirs.clamp(2, 64);
    let table = crate::ops::hbao_tap_table(t.w, t.h, radius_uv, dirs);
    imp::global()?.hbao(t, &table, dirs, crate::ops::HBAO_STEPS as u32, scale, intensity)
}

/// Packed map bytes waiting on the device (see `pack_rgb8_g`).
pub struct Packed(imp::PBuf);

/// RGB of a picture as RGBA8 (alpha 255); `flip_g` flips the green channel (`1 - g`).
pub fn pack_rgb8_g(t: &GTex, flip_g: bool) -> Option<Packed> {
    imp::global()?.pack_rgb8(&t.0, flip_g).map(Packed)
}

/// Roughness, metallic and occlusion (luminance of each, or its default when absent) as RGBA8.
pub fn pack_rma_g(r: Option<&GTex>, m: Option<&GTex>, a: Option<&GTex>, dflt: [f32; 3]) -> Option<Packed> {
    imp::global()?.pack_rma(r.map(|t| &*t.0), m.map(|t| &*t.0), a.map(|t| &*t.0), dflt).map(Packed)
}

/// Luminance as 16-bit floats, two per word, low half first (little-endian `u16` values in the bytes).
pub fn pack_half_g(t: &GTex) -> Option<Packed> {
    imp::global()?.pack_half(&t.0).map(Packed)
}

/// Maps the first channel of `t` so its percentiles land on `target` (as `noise_styles::tone`), then optionally adds
/// `balance` and clamps; on the device.
pub fn tone_g(t: &GTex, target: &[f32; 5], balance: Option<f32>) -> Option<GTex> {
    wrap(imp::global()?.tone_g(&t.0, target, balance))
}

/// The same on a host picture.
pub fn tone(t: &Tex, target: &[f32; 5], balance: Option<f32>) -> Option<Tex> {
    if !usable(t.w, t.h) {
        return None;
    }
    imp::global()?.tone(t, target, balance)
}

/// Raw octave sum of a styled noise on the device: basis (0 value, 1 gradient, 2 spots, 3 ridged), octaves
/// (period x, period y, weight, seed), vertical trend, Warp and Angle random amounts and the node's seed.
#[allow(clippy::too_many_arguments)]
pub fn styled_raw_g(w: usize, h: usize, basis: u32, oct: &[(i32, i32, f32, u32)], ramp: f32, warp: f32, angle_random: f32, seed: u32) -> Option<GTex> {
    wrap(imp::global()?.styled_raw_g(w, h, basis, oct, ramp, warp, angle_random, seed))
}

/// Settings of the Shadows nodes (all derived from the node's parameters by `nodes10_img`, in its units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowParams {
    /// ray-march steps per ray
    pub steps: u32,
    /// jittered rays per pixel (soft edges)
    pub taps: u32,
    pub height_scale: f32,
    /// how much shadows fade with the distance to what casts them
    pub atten: f32,
    pub light_k: f32,
    pub opacity: f32,
    /// angle jitter of the light's size
    pub soft: f32,
    /// ray length in texture units
    pub len: f32,
    /// the light's elevation
    pub elev: f32,
    pub invert: bool,
    /// unit direction towards the light
    pub dir: [f32; 2],
}

/// Height shadows of the luminance of `t`, on the device.
pub fn shadows_g(t: &GTex, sh: &ShadowParams) -> Option<GTex> {
    wrap(imp::global()?.shadows_g(&t.0, sh))
}

/// The same on a host picture.
pub fn shadows(t: &Tex, sh: &ShadowParams) -> Option<Tex> {
    if !usable(t.w, t.h) {
        return None;
    }
    imp::global()?.shadows(t, sh)
}

/// A picture for the 2D view, ready on the device: RGBA8 and the value range (see [`download_view`]).
pub struct ViewPack(Packed, Packed);

pub fn pack_view_g(t: &GTex) -> Option<ViewPack> {
    imp::global()?.pack_view(&t.0).map(|(a, b)| ViewPack(Packed(a), Packed(b)))
}

/// Reads the picture of [`pack_view_g`] back: RGBA8 bytes and (smallest, largest) colour value.
pub fn download_view(v: &ViewPack) -> Option<(Vec<u8>, (f32, f32))> {
    let mut r = download_packed(&[&v.0, &v.1])?;
    let range = r.pop()?;
    let px = r.pop()?;
    if range.len() < 8 {
        return None;
    }
    let un = |k: u32| f32::from_bits(if k >> 31 == 1 { k & 0x7fff_ffff } else { !k });
    let k = |o: usize| u32::from_le_bytes([range[o], range[o + 1], range[o + 2], range[o + 3]]);
    Some((px, (un(k(0)), un(k(4)))))
}

/// Reads packed buffers back with one submission and one wait.
pub fn download_packed(bs: &[&Packed]) -> Option<Vec<Vec<u8>>> {
    let refs: Vec<&imp::PBuf> = bs.iter().map(|b| &b.0).collect();
    imp::global()?.download_bytes(&refs)
}

/// A mesh uploaded for ray tracing on the GPU (the baker's scene): the BVH words from `Bvh::export`, one allow-mask per
/// low-poly object (bit set = the high-poly object may be hit) and the back-face switch for primary rays.
pub struct SceneData {
    pub nodes: Vec<[u32; 4]>,
    pub tris: Vec<[u32; 4]>,
    /// `words` u32 per mask; empty when no object has a mask
    pub masks: Vec<u32>,
    pub words: u32,
    pub cull_back: bool,
}

/// Parameters of the hemisphere kernels.
#[derive(Clone, Copy)]
pub struct HemiParams {
    /// rays per sample
    pub rays: u32,
    pub dist: f32,
    /// ambient occlusion distance falloff exponent (0 = none)
    pub falloff: f32,
    /// false: ambient occlusion and bent normal (occlusion sum, direction sum); true: thickness (distance sum)
    pub thickness: bool,
    /// first of two hash seeds (the second is `seed + 1`); sub-sample index is added by the kernel
    pub seed: u32,
}

/// A scene on the GPU. `None` from `BakeScene::new` means the baker should use the CPU.
pub struct BakeScene(imp::Scene);

impl BakeScene {
    pub fn new(d: &SceneData) -> Option<BakeScene> {
        if !ENABLED.load(Ordering::Relaxed) {
            return None;
        }
        imp::global()?.scene(d).map(BakeScene)
    }
    pub fn adapter() -> Option<String> {
        if ENABLED.load(Ordering::Relaxed) { adapter_name() } else { None }
    }
    /// Closest hits. A ray is three `[u32; 4]`: `o.xyz, tmin` / `d.xyz, tmax` / `mask index (u32::MAX = any), 0, 0, 0`
    /// (floats as bits). A hit is `[t bits, triangle, u bits, v bits]`; a miss has t < 0.
    pub fn trace(&self, rays: &[[u32; 4]]) -> Option<Vec<[u32; 4]>> {
        imp::global()?.trace(&self.0, rays)
    }
    /// Cosine-weighted hemisphere rays per sample. A sample is two `[u32; 4]`: `origin.xyz, x` / `normal.xyz, y | sub << 16`.
    /// The result per sample is `[sum, dir.x, dir.y, dir.z]` as floats (occlusion) or `[sum, 0, 0, 0]` (thickness).
    pub fn hemi(&self, samples: &[[u32; 4]], p: HemiParams) -> Option<Vec<[f32; 4]>> {
        imp::global()?.hemi(&self.0, samples, p)
    }
}

#[cfg(not(feature = "gpu"))]
mod imp {
    use crate::tex::Tex;
    pub struct Gpu;
    pub struct Scene;
    /// Never constructed without the `gpu` feature.
    pub struct GBuf {
        pub w: usize,
        pub h: usize,
    }
    #[allow(dead_code)]
    pub struct PBuf {
        pub bytes: usize,
    }
    pub fn live_bytes() -> u64 {
        0
    }
    pub fn downloads() -> usize {
        0
    }
    pub fn submits() -> usize {
        0
    }
    pub fn global() -> Option<&'static Gpu> {
        None
    }
    impl Gpu {
        pub fn scene(&self, _: &super::SceneData) -> Option<Scene> {
            None
        }
        pub fn trace(&self, _: &Scene, _: &[[u32; 4]]) -> Option<Vec<[u32; 4]>> {
            None
        }
        pub fn hemi(&self, _: &Scene, _: &[[u32; 4]], _: super::HemiParams) -> Option<Vec<[f32; 4]>> {
            None
        }
        pub fn name(&self) -> String {
            String::new()
        }
        pub fn fits(&self, _: usize, _: usize) -> bool {
            false
        }
        pub fn hbao_g(&self, _: &GBuf, _: &[[f32; 4]], _: u32, _: u32, _: f32, _: f32) -> Option<GBuf> {
            None
        }
        pub fn hbao(&self, _: &Tex, _: &[[f32; 4]], _: u32, _: u32, _: f32, _: f32) -> Option<Tex> {
            None
        }
        pub fn pack_rgb8(&self, _: &GBuf, _: bool) -> Option<PBuf> {
            None
        }
        pub fn pack_rma(&self, _: Option<&GBuf>, _: Option<&GBuf>, _: Option<&GBuf>, _: [f32; 3]) -> Option<PBuf> {
            None
        }
        pub fn pack_half(&self, _: &GBuf) -> Option<PBuf> {
            None
        }
        pub fn pack_view(&self, _: &GBuf) -> Option<(PBuf, PBuf)> {
            None
        }
        pub fn shadows_g(&self, _: &GBuf, _: &super::ShadowParams) -> Option<GBuf> {
            None
        }
        pub fn tone_g(&self, _: &GBuf, _: &[f32; 5], _: Option<f32>) -> Option<GBuf> {
            None
        }
        pub fn tone(&self, _: &Tex, _: &[f32; 5], _: Option<f32>) -> Option<Tex> {
            None
        }
        #[allow(clippy::too_many_arguments)]
        pub fn styled_raw_g(&self, _: usize, _: usize, _: u32, _: &[(i32, i32, f32, u32)], _: f32, _: f32, _: f32, _: u32) -> Option<GBuf> {
            None
        }
        pub fn shadows(&self, _: &Tex, _: &super::ShadowParams) -> Option<Tex> {
            None
        }
        pub fn download_bytes(&self, _: &[&PBuf]) -> Option<Vec<Vec<u8>>> {
            None
        }
        pub fn upload(&self, _: &Tex) -> Option<GBuf> {
            None
        }
        pub fn download(&self, _: &[&GBuf]) -> Option<Vec<Tex>> {
            None
        }
        pub fn thumb_g(&self, _: &GBuf, _: usize) -> Option<Vec<u8>> {
            None
        }
        pub fn solid_g(&self, _: usize, _: usize, _: [f32; 4]) -> Option<GBuf> {
            None
        }
        pub fn pointwise_g(&self, _: &GBuf, _: u32, _: f32, _: f32, _: f32, _: f32) -> Option<GBuf> {
            None
        }
        #[allow(clippy::too_many_arguments)]
        pub fn kernel_g(&self, _: super::Kern, _: u32, _: u32, _: &[f32], _: &[f32], _: &[&GBuf], _: (usize, usize)) -> Option<GBuf> {
            None
        }
        pub fn minmax_g(&self, _: &GBuf) -> Option<GBuf> {
            None
        }
        pub fn flood_g(&self, _: &GBuf, _: u32, _: f32, _: u32) -> Option<GBuf> {
            None
        }
        pub fn stats_g(&self, _: &GBuf, _: u32, _: f32) -> Option<GBuf> {
            None
        }
        #[allow(clippy::too_many_arguments)]
        pub fn noise_g(&self, _: usize, _: usize, _: bool, _: i32, _: i32, _: u32, _: f32, _: u32, _: u32) -> Option<GBuf> {
            None
        }
        #[allow(clippy::too_many_arguments)]
        pub fn dir_noise_g(&self, _: usize, _: usize, _: i32, _: u32, _: f32, _: f32, _: f32, _: f32, _: u32) -> Option<GBuf> {
            None
        }
        pub fn blur_g(&self, _: &GBuf, _: f32) -> Option<GBuf> {
            None
        }
        pub fn normal_g(&self, _: &GBuf, _: f32, _: bool) -> Option<GBuf> {
            None
        }
        pub fn levels_g(&self, _: &GBuf, _: f32, _: f32, _: f32, _: f32, _: f32) -> Option<GBuf> {
            None
        }
        pub fn directional_blur_g(&self, _: &GBuf, _: f32, _: f32) -> Option<GBuf> {
            None
        }
        pub fn blend_g(&self, _: &GBuf, _: &GBuf, _: Option<&GBuf>, _: u32, _: f32, _: u32) -> Option<GBuf> {
            None
        }
        #[allow(clippy::too_many_arguments)]
        pub fn radial_blur_g(&self, _: &GBuf, _: u32, _: u32, _: f32, _: f32, _: f32, _: f32) -> Option<GBuf> {
            None
        }
        #[allow(clippy::too_many_arguments)]
        pub fn bent_normal_g(&self, _: &GBuf, _: u32, _: u32, _: f32, _: f32, _: bool, _: bool) -> Option<GBuf> {
            None
        }
        pub fn bilateral_blur_g(&self, _: &GBuf, _: &GBuf, _: i32, _: f32) -> Option<GBuf> {
            None
        }
        pub fn warp_g(&self, _: &GBuf, _: &GBuf, _: super::WarpKind, _: f32, _: u32) -> Option<GBuf> {
            None
        }
        pub fn extrude_g(&self, _: &GBuf, _: u32, _: f32, _: f32, _: f32) -> Option<GBuf> {
            None
        }
        pub fn slope_blur_g(&self, _: &GBuf, _: &GBuf, _: u32, _: f32, _: u32) -> Option<GBuf> {
            None
        }
        #[allow(clippy::too_many_arguments)]
        pub fn noise(&self, _: usize, _: usize, _: bool, _: i32, _: i32, _: u32, _: f32, _: u32, _: u32) -> Option<Tex> {
            None
        }
        pub fn blur(&self, _: &Tex, _: f32) -> Option<Tex> {
            None
        }
        pub fn normal(&self, _: &Tex, _: f32, _: bool) -> Option<Tex> {
            None
        }
        pub fn levels(&self, _: &Tex, _: f32, _: f32, _: f32, _: f32, _: f32) -> Option<Tex> {
            None
        }
        pub fn directional_blur(&self, _: &Tex, _: f32, _: f32) -> Option<Tex> {
            None
        }
        pub fn blend(&self, _: &Tex, _: &Tex, _: Option<&Tex>, _: u32, _: f32, _: u32) -> Option<Tex> {
            None
        }
        pub fn radial_blur(&self, _: &Tex, _: u32, _: u32, _: f32, _: f32, _: f32, _: f32) -> Option<Tex> {
            None
        }
        pub fn bent_normal(&self, _: &Tex, _: u32, _: u32, _: f32, _: f32, _: bool, _: bool) -> Option<Tex> {
            None
        }
        pub fn bilateral_blur(&self, _: &Tex, _: &Tex, _: i32, _: f32) -> Option<Tex> {
            None
        }
        pub fn extrude(&self, _: &Tex, _: u32, _: f32, _: f32, _: f32) -> Option<Tex> {
            None
        }
        #[allow(clippy::too_many_arguments)]
        pub fn dir_noise(&self, _: usize, _: usize, _: i32, _: u32, _: f32, _: f32, _: f32, _: f32, _: u32) -> Option<Tex> {
            None
        }
        pub fn warp(&self, _: &Tex, _: &Tex, _: super::WarpKind, _: f32, _: u32) -> Option<Tex> {
            None
        }
        pub fn slope_blur(&self, _: &Tex, _: &Tex, _: u32, _: f32, _: u32) -> Option<Tex> {
            None
        }
    }
}

#[cfg(feature = "gpu")]
mod imp {
    use crate::tex::Tex;
    use std::borrow::Cow;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};

    /// Bytes of picture memory currently held on the device by live `GBuf`s.
    static LIVE: AtomicU64 = AtomicU64::new(0);

    pub fn live_bytes() -> u64 {
        LIVE.load(Ordering::Relaxed)
    }

    /// Number of read-backs (`download` calls) since the program started.
    static DOWNLOADS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    pub fn downloads() -> usize {
        DOWNLOADS.load(Ordering::Relaxed)
    }

    /// Number of command submissions (one per kernel launch) since the program started.
    static SUBMITS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    pub fn submits() -> usize {
        SUBMITS.load(Ordering::Relaxed)
    }

    /// A picture in device memory: `w * h` RGBA f32 texels in a storage buffer.
    pub struct GBuf {
        pub buf: wgpu::Buffer,
        pub w: usize,
        pub h: usize,
    }

    impl Drop for GBuf {
        fn drop(&mut self) {
            LIVE.fetch_sub((self.w * self.h * 16) as u64, Ordering::Relaxed);
        }
    }

    pub struct Gpu {
        dev: wgpu::Device,
        queue: wgpu::Queue,
        info: wgpu::AdapterInfo,
        max_bytes: u64,
        /// most device memory the pictures of one evaluation may hold at once (MC_GPU_MAX_MB, default 3 GB): past it
        /// a node runs on the CPU instead of risking an out-of-memory error
        max_live: u64,
        pipes: Mutex<HashMap<&'static str, std::sync::Arc<wgpu::ComputePipeline>>>,
        grad: wgpu::Buffer,
    }

    pub fn global() -> Option<&'static Gpu> {
        static G: OnceLock<Option<Gpu>> = OnceLock::new();
        G.get_or_init(Gpu::create).as_ref()
    }

    /// 32-bit words of a uniform block.
    #[derive(Default)]
    struct Uni(Vec<u8>);
    impl Uni {
        fn u(mut self, v: u32) -> Uni {
            self.0.extend_from_slice(&v.to_le_bytes());
            self
        }
        fn i(mut self, v: i32) -> Uni {
            self.0.extend_from_slice(&v.to_le_bytes());
            self
        }
        fn f(mut self, v: f32) -> Uni {
            self.0.extend_from_slice(&v.to_le_bytes());
            self
        }
        fn done(mut self) -> Vec<u8> {
            while self.0.len() % 16 != 0 || self.0.is_empty() {
                self.0.push(0);
            }
            self.0
        }
    }

    fn px_bytes(p: &[[f32; 4]]) -> &[u8] {
        // SAFETY: [f32; 4] is plain old data with no padding; the slice covers exactly the same memory
        unsafe { std::slice::from_raw_parts(p.as_ptr() as *const u8, std::mem::size_of_val(p)) }
    }

    include!("gpu_k.rs");
    include!("gpu_f.rs");

    impl Gpu {
        fn create() -> Option<Gpu> {
            let mode = std::env::var("MC_GPU").unwrap_or_default().to_lowercase();
            if mode == "0" || mode == "off" {
                return None;
            }
            let force = mode == "force";
            let inst = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::all(), ..Default::default() });
            let mut best: Option<wgpu::Adapter> = None;
            let mut rank = 0;
            for a in inst.enumerate_adapters(wgpu::Backends::all()) {
                let info = a.get_info();
                let r = match info.device_type {
                    wgpu::DeviceType::DiscreteGpu => 4,
                    wgpu::DeviceType::IntegratedGpu => 3,
                    wgpu::DeviceType::VirtualGpu => 2,
                    _ if force => 1,
                    _ => 0, // software rendering is never faster than the CPU code
                };
                // compute shaders need storage buffers in the compute stage; skip adapters without them
                if r > rank && a.limits().max_storage_buffers_per_shader_stage >= 4 {
                    rank = r;
                    best = Some(a);
                }
            }
            let adapter = best?;
            let info = adapter.get_info();
            let lim = adapter.limits();
            let (dev, queue) = pollster::block_on(adapter.request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("materialcraft"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits {
                        max_storage_buffer_binding_size: lim.max_storage_buffer_binding_size,
                        max_buffer_size: lim.max_buffer_size,
                        // the baker kernels bind five storage buffers (mesh nodes, triangles, masks, input, output)
                        max_storage_buffers_per_shader_stage: lim.max_storage_buffers_per_shader_stage.min(8),
                        // the 3D view keeps the material maps as sampled textures up to 8K
                        max_texture_dimension_2d: lim.max_texture_dimension_2d.min(16384),
                        ..wgpu::Limits::downlevel_defaults()
                    },
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            ))
            .ok()?;
            let max_bytes = lim.max_storage_buffer_binding_size.min(lim.max_buffer_size as u32) as u64;
            // the same 4096-angle gradient table the CPU noise uses, so both paths pick identical gradients
            let tab: Vec<[f32; 2]> = (0..4096).map(|i| {
                let a = i as f32 / 4096.0 * std::f32::consts::TAU;
                [a.cos(), a.sin()]
            }).collect();
            let grad = dev.create_buffer(&wgpu::BufferDescriptor { label: Some("grad"), size: 4096 * 8, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            // SAFETY: [f32; 2] is plain old data
            let bytes = unsafe { std::slice::from_raw_parts(tab.as_ptr() as *const u8, tab.len() * 8) };
            queue.write_buffer(&grad, 0, bytes);
            let max_live = std::env::var("MC_GPU_MAX_MB").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(3072).max(64) << 20;
            // an error the driver reports later (out of memory, a lost device) must not abort the program: the GPU path
            // switches itself off and the CPU code takes over
            dev.on_uncaptured_error(Box::new(|e| {
                eprintln!("GPU error, switching GPU evaluation off: {e}");
                super::BROKEN.store(true, std::sync::atomic::Ordering::Relaxed);
            }));
            Some(Gpu { dev, queue, info, max_bytes, max_live, pipes: Mutex::new(HashMap::new()), grad })
        }

        pub fn name(&self) -> String {
            format!("{} ({:?})", self.info.name, self.info.backend)
        }

        fn pipeline(&self, name: &'static str, src: &str) -> std::sync::Arc<wgpu::ComputePipeline> {
            let mut m = self.pipes.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(p) = m.get(name) {
                return p.clone();
            }
            let module = self.dev.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(name), source: wgpu::ShaderSource::Wgsl(Cow::Owned(src.to_string())) });
            let p = std::sync::Arc::new(self.dev.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(name),
                layout: None,
                module: &module,
                entry_point: "main",
                compilation_options: Default::default(),
                cache: None,
            }));
            m.insert(name, p.clone());
            p
        }

        fn buf(&self, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
            self.dev.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage, mapped_at_creation: false })
        }

        pub fn fits(&self, w: usize, h: usize) -> bool {
            let bytes = (w * h * 16) as u64;
            // a kernel needs its result and sometimes a temporary next to the inputs already held
            bytes <= self.max_bytes && w <= 65535 * 8 && h <= 65535 * 8 && LIVE.load(Ordering::Relaxed) + 3 * bytes <= self.max_live
        }

        /// A texture-sized storage buffer that can be read, written and copied (stays on the device).
        fn gbuf(&self, w: usize, h: usize) -> GBuf {
            let bytes = (w * h * 16) as u64;
            LIVE.fetch_add(bytes, Ordering::Relaxed);
            GBuf { buf: self.buf(bytes, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST), w, h }
        }

        fn submit(&self, enc: wgpu::CommandEncoder) {
            SUBMITS.fetch_add(1, Ordering::Relaxed);
            self.queue.submit(Some(enc.finish()));
        }

        /// Copies a picture into device memory.
        pub fn upload(&self, t: &Tex) -> Option<GBuf> {
            if !self.fits(t.w, t.h) || t.px.len() != t.w * t.h {
                return None;
            }
            let g = self.gbuf(t.w, t.h);
            self.queue.write_buffer(&g.buf, 0, px_bytes(&t.px));
            Some(g)
        }

        /// Reads several device pictures back with one submission and one wait (non-finite values become 0).
        pub fn download(&self, ts: &[&GBuf]) -> Option<Vec<Tex>> {
            if ts.is_empty() {
                return Some(Vec::new());
            }
            DOWNLOADS.fetch_add(1, Ordering::Relaxed);
            let mut enc = self.dev.create_command_encoder(&Default::default());
            let stages: Vec<wgpu::Buffer> = ts
                .iter()
                .map(|g| {
                    let bytes = (g.w * g.h * 16) as u64;
                    let stage = self.buf(bytes, wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST);
                    enc.copy_buffer_to_buffer(&g.buf, 0, &stage, 0, bytes);
                    stage
                })
                .collect();
            self.submit(enc);
            let (tx, rx) = std::sync::mpsc::channel();
            for s in &stages {
                let tx = tx.clone();
                s.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                    let _ = tx.send(r);
                });
            }
            drop(tx);
            self.dev.poll(wgpu::Maintain::Wait);
            for _ in 0..stages.len() {
                rx.recv().ok()?.ok()?;
            }
            let mut out = Vec::with_capacity(ts.len());
            for (g, stage) in ts.iter().zip(&stages) {
                let data = stage.slice(..).get_mapped_range();
                let mut px = vec![[0.0f32; 4]; g.w * g.h];
                // SAFETY: the destination has exactly `data.len()` bytes of plain f32 data
                unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), px.as_mut_ptr() as *mut u8, data.len().min(px.len() * 16)) };
                drop(data);
                stage.unmap();
                for p in px.iter_mut() {
                    for c in p.iter_mut() {
                        if !c.is_finite() {
                            *c = 0.0;
                        }
                    }
                }
                out.push(Tex { w: g.w, h: g.h, px });
            }
            Some(out)
        }

        fn dispatch(&self, enc: &mut wgpu::CommandEncoder, pipe: &wgpu::ComputePipeline, entries: &[wgpu::BindGroupEntry], w: usize, h: usize) {
            let bg = self.dev.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &pipe.get_bind_group_layout(0), entries });
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(((w + 7) / 8) as u32, ((h + 7) / 8) as u32, 1);
        }

        fn uniform(&self, data: &[u8]) -> wgpu::Buffer {
            let b = self.buf(data.len() as u64, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST);
            self.queue.write_buffer(&b, 0, data);
            b
        }

        /// Uploads the inputs, runs `f` on the device copies and reads the one result back (the whole-picture API).
        fn roundtrip(&self, ins: &[&Tex], f: impl FnOnce(&[&GBuf]) -> Option<GBuf>) -> Option<Tex> {
            let ups: Vec<GBuf> = ins.iter().map(|t| self.upload(t)).collect::<Option<Vec<_>>>()?;
            let refs: Vec<&GBuf> = ups.iter().collect();
            let out = f(&refs)?;
            self.download(&[&out])?.pop()
        }

        #[allow(clippy::too_many_arguments)]
        pub fn noise_g(&self, w: usize, h: usize, fractal: bool, px: i32, py: i32, octaves: u32, gain: f32, seed: u32, mode: u32) -> Option<GBuf> {
            if !self.fits(w, h) {
                return None;
            }
            let pipe = self.pipeline("noise", NOISE_WGSL);
            let uni = self.uniform(&Uni::default().u(w as u32).u(h as u32).i(px.max(1)).i(py.max(1)).u(octaves).u(seed).u(mode).u(fractal as u32).f(gain).done());
            let out = self.gbuf(w, h);
            let mut enc = self.dev.create_command_encoder(&Default::default());
            self.dispatch(&mut enc, &pipe, &[
                wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.grad.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: out.buf.as_entire_binding() },
            ], w, h);
            self.submit(enc);
            Some(out)
        }

        #[allow(clippy::too_many_arguments)]
        pub fn noise(&self, w: usize, h: usize, fractal: bool, px: i32, py: i32, octaves: u32, gain: f32, seed: u32, mode: u32) -> Option<Tex> {
            let g = self.noise_g(w, h, fractal, px, py, octaves, gain, seed, mode)?;
            self.download(&[&g])?.pop()
        }

        #[allow(clippy::too_many_arguments)]
        pub fn dir_noise_g(&self, w: usize, h: usize, p1: i32, taps: u32, ca: f32, sa: f32, len: f32, gain: f32, seed: u32) -> Option<GBuf> {
            if !self.fits(w, h) {
                return None;
            }
            let helpers = NOISE_WGSL.split("@compute").next().unwrap_or("");
            let src = format!("{}{}", helpers, DIRNOISE_MAIN_WGSL);
            let pipe = self.pipeline("dirnoise", &src);
            let uni = self.uniform(&Uni::default().u(w as u32).u(h as u32).i(p1.max(1)).i(p1.max(1)).u(taps).u(seed).u(0).u(0).f(gain).f(ca).f(sa).f(len).done());
            let out = self.gbuf(w, h);
            let mut enc = self.dev.create_command_encoder(&Default::default());
            self.dispatch(&mut enc, &pipe, &[
                wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.grad.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: out.buf.as_entire_binding() },
            ], w, h);
            self.submit(enc);
            Some(out)
        }

        #[allow(clippy::too_many_arguments)]
        pub fn dir_noise(&self, w: usize, h: usize, p1: i32, taps: u32, ca: f32, sa: f32, len: f32, gain: f32, seed: u32) -> Option<Tex> {
            let g = self.dir_noise_g(w, h, p1, taps, ca, sa, len, gain, seed)?;
            self.download(&[&g])?.pop()
        }

        pub fn blur_g(&self, t: &GBuf, sigma: f32) -> Option<GBuf> {
            let (w, h) = (t.w, t.h);
            if !self.fits(w, h) {
                return None;
            }
            let pipe = self.pipeline("box", BOX_WGSL);
            let (a, b) = (self.gbuf(w, h), self.gbuf(w, h));
            let mut enc = self.dev.create_command_encoder(&Default::default());
            let mut unis = Vec::new();
            let mut cur: &GBuf = t;
            let mut next_a = true;
            let mut last_a = false;
            // three box blurs, each a horizontal then a vertical pass (same radii as the CPU path)
            for bw in crate::ops::gauss_boxes(sigma) {
                let r0 = (bw / 2).max(0);
                for dir in 0..2u32 {
                    let r = if dir == 0 { r0.min((w as i64 - 1) / 2) } else { r0.min((h as i64 - 1) / 2) };
                    let uni = self.uniform(&Uni::default().u(w as u32).u(h as u32).i(r as i32).u(dir).done());
                    let dst = if next_a { &a } else { &b };
                    self.dispatch(&mut enc, &pipe, &[
                        wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: cur.buf.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 2, resource: dst.buf.as_entire_binding() },
                    ], w, h);
                    unis.push(uni);
                    cur = dst;
                    last_a = next_a;
                    next_a = !next_a;
                }
            }
            if unis.is_empty() {
                return None;
            }
            self.submit(enc);
            Some(if last_a { a } else { b })
        }

        pub fn blur(&self, t: &Tex, sigma: f32) -> Option<Tex> {
            self.roundtrip(&[t], |g| self.blur_g(g[0], sigma))
        }

        pub fn normal_g(&self, t: &GBuf, strength: f32, directx: bool) -> Option<GBuf> {
            let (w, h) = (t.w, t.h);
            let k = strength * w as f32 * 0.05;
            let uni = Uni::default().u(w as u32).u(h as u32).f(k).u(directx as u32).done();
            self.map_pixels_g("normal", NORMAL_WGSL, uni, &[t])
        }

        pub fn normal(&self, t: &Tex, strength: f32, directx: bool) -> Option<Tex> {
            self.roundtrip(&[t], |g| self.normal_g(g[0], strength, directx))
        }
    }

    impl Gpu {
        /// Run `src` once per pixel over `inputs` (bindings 1..=n, storage read); the result stays on the device.
        fn map_pixels_g(&self, name: &'static str, src: &str, uni: Vec<u8>, inputs: &[&GBuf]) -> Option<GBuf> {
            let (w, h) = (inputs.first()?.w, inputs.first()?.h);
            if !self.fits(w, h) || inputs.iter().any(|t| (t.w, t.h) != (w, h)) {
                return None;
            }
            let pipe = self.pipeline(name, src);
            let ubuf = self.uniform(&uni);
            let out = self.gbuf(w, h);
            let mut entries = vec![wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() }];
            for (i, b) in inputs.iter().enumerate() {
                entries.push(wgpu::BindGroupEntry { binding: 1 + i as u32, resource: b.buf.as_entire_binding() });
            }
            entries.push(wgpu::BindGroupEntry { binding: 1 + inputs.len() as u32, resource: out.buf.as_entire_binding() });
            let mut enc = self.dev.create_command_encoder(&Default::default());
            self.dispatch(&mut enc, &pipe, &entries, w, h);
            self.submit(enc);
            Some(out)
        }

        pub fn levels_g(&self, t: &GBuf, in_lo: f32, in_hi: f32, gamma: f32, out_lo: f32, out_hi: f32) -> Option<GBuf> {
            let d = if in_hi >= in_lo { (in_hi - in_lo).max(1e-6) } else { (in_hi - in_lo).min(-1e-6) };
            let g = 1.0 / gamma.max(1e-3);
            let uni = Uni::default().u(t.w as u32).u(t.h as u32).f(in_lo).f(d).f(g).f(out_lo).f(out_hi).done();
            self.map_pixels_g("levels", LEVELS_WGSL, uni, &[t])
        }

        pub fn levels(&self, t: &Tex, in_lo: f32, in_hi: f32, gamma: f32, out_lo: f32, out_hi: f32) -> Option<Tex> {
            self.roundtrip(&[t], |g| self.levels_g(g[0], in_lo, in_hi, gamma, out_lo, out_hi))
        }

        pub fn directional_blur_g(&self, t: &GBuf, angle: f32, length: f32) -> Option<GBuf> {
            let (s, c) = (angle * std::f32::consts::TAU).sin_cos();
            let uni = Uni::default().u(t.w as u32).u(t.h as u32).f(c).f(s).f(length).done();
            self.map_pixels_g("dirblur", DIRBLUR_WGSL, uni, &[t])
        }

        pub fn directional_blur(&self, t: &Tex, angle: f32, length: f32) -> Option<Tex> {
            self.roundtrip(&[t], |g| self.directional_blur_g(g[0], angle, length))
        }

        pub fn blend_g(&self, fg: &GBuf, bg: &GBuf, mask: Option<&GBuf>, mode: u32, opacity: f32, alpha_mode: u32) -> Option<GBuf> {
            let uni = Uni::default().u(bg.w as u32).u(bg.h as u32).u(mode).u(alpha_mode).f(opacity).u(mask.is_some() as u32).done();
            self.map_pixels_g("blend", BLEND_WGSL, uni, &[fg, bg, mask.unwrap_or(fg)])
        }

        pub fn blend(&self, fg: &Tex, bg: &Tex, mask: Option<&Tex>, mode: u32, opacity: f32, alpha_mode: u32) -> Option<Tex> {
            let ins: Vec<&Tex> = match mask {
                Some(m) => vec![fg, bg, m],
                None => vec![fg, bg],
            };
            self.roundtrip(&ins, |g| self.blend_g(g[0], g[1], g.get(2).copied(), mode, opacity, alpha_mode))
        }

        #[allow(clippy::too_many_arguments)]
        pub fn radial_blur_g(&self, t: &GBuf, mode: u32, taps: u32, cx: f32, cy: f32, amt: f32, rad: f32) -> Option<GBuf> {
            let uni = Uni::default().u(t.w as u32).u(t.h as u32).u(mode).u(taps).f(cx).f(cy).f(amt).f(rad).done();
            let src = format!("{}{}", RADIAL_WGSL, samp_fn("samp", "src"));
            self.map_pixels_g("radial", &src, uni, &[t])
        }

        #[allow(clippy::too_many_arguments)]
        pub fn radial_blur(&self, t: &Tex, mode: u32, taps: u32, cx: f32, cy: f32, amt: f32, rad: f32) -> Option<Tex> {
            self.roundtrip(&[t], |g| self.radial_blur_g(g[0], mode, taps, cx, cy, amt, rad))
        }

        #[allow(clippy::too_many_arguments)]
        pub fn bent_normal_g(&self, t: &GBuf, dirs: u32, steps: u32, rad: f32, relief: f32, directx: bool, want_ao: bool) -> Option<GBuf> {
            let aspect = t.w as f32 / t.h as f32;
            let ysign = if directx { 1.0 } else { -1.0 };
            let uni = Uni::default().u(t.w as u32).u(t.h as u32).u(dirs).u(steps).f(rad).f(relief).f(ysign).u(want_ao as u32).f(aspect).done();
            let src = format!("{}{}", BENT_WGSL, samp_fn("samp", "src"));
            self.map_pixels_g("bent", &src, uni, &[t])
        }

        #[allow(clippy::too_many_arguments)]
        pub fn bent_normal(&self, t: &Tex, dirs: u32, steps: u32, rad: f32, relief: f32, directx: bool, want_ao: bool) -> Option<Tex> {
            self.roundtrip(&[t], |g| self.bent_normal_g(g[0], dirs, steps, rad, relief, directx, want_ao))
        }

        pub fn bilateral_blur_g(&self, t: &GBuf, guide: &GBuf, rad: i32, range: f32) -> Option<GBuf> {
            let ss = (rad as f32 * 0.5).max(0.5);
            let uni = Uni::default().u(t.w as u32).u(t.h as u32).i(rad).f(ss).f(range).done();
            self.map_pixels_g("bilateral", BILATERAL_WGSL, uni, &[t, guide])
        }

        pub fn bilateral_blur(&self, t: &Tex, guide: &Tex, rad: i32, range: f32) -> Option<Tex> {
            self.roundtrip(&[t, guide], |g| self.bilateral_blur_g(g[0], g[1], rad, range))
        }

        pub fn warp_g(&self, t: &GBuf, w: &GBuf, kind: super::WarpKind, intensity: f32, filtering: u32) -> Option<GBuf> {
            use super::WarpKind::*;
            let (k, a, b, c) = match kind {
                Slope => (0u32, 0.0, 0.0, 0.0),
                Directional { c, s, offset } => (1, c, s, offset),
                Vector { gs } => (2, gs, 0.0, 0.0),
            };
            let uni = Uni::default().u(t.w as u32).u(t.h as u32).u(k).u(filtering.min(2)).f(intensity).f(a).f(b).f(c).done();
            let src = format!("{}{}", WARP_WGSL, samp_fn("samp", "src"));
            self.map_pixels_g("warp", &src, uni, &[t, w])
        }

        pub fn warp(&self, t: &Tex, w: &Tex, kind: super::WarpKind, intensity: f32, filtering: u32) -> Option<Tex> {
            self.roundtrip(&[t, w], |g| self.warp_g(g[0], g[1], kind, intensity, filtering))
        }

        /// Many dependent passes on ping-pong buffers inside one submission (no readback between the passes).
        pub fn extrude_g(&self, t: &GBuf, steps: u32, dx: f32, dy: f32, drop: f32) -> Option<GBuf> {
            let (w, h) = (t.w, t.h);
            if steps == 0 || !self.fits(w, h) || !self.fits(w, h * 3) {
                return None;
            }
            let pipe = self.pipeline("extrude", EXTRUDE_WGSL);
            let ping = [self.gbuf(w, h), self.gbuf(w, h)];
            let mk = |first: u32| self.uniform(&Uni::default().u(w as u32).u(h as u32).u(first).f(dx).f(dy).f(drop).done());
            let (u_first, u_rest) = (mk(1), mk(0));
            let mut enc = self.dev.create_command_encoder(&Default::default());
            for i in 0..steps as usize {
                let (prev, out) = (&ping[(i + 1) % 2], &ping[i % 2]);
                let ub = if i == 0 { &u_first } else { &u_rest };
                self.dispatch(&mut enc, &pipe, &[
                    wgpu::BindGroupEntry { binding: 0, resource: ub.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: t.buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: prev.buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: out.buf.as_entire_binding() },
                ], w, h);
            }
            self.submit(enc);
            let [p0, p1] = ping;
            Some(if (steps as usize - 1) % 2 == 0 { p0 } else { p1 })
        }

        pub fn extrude(&self, t: &Tex, steps: u32, dx: f32, dy: f32, drop: f32) -> Option<Tex> {
            self.roundtrip(&[t], |g| self.extrude_g(g[0], steps, dx, dy, drop))
        }

        pub fn slope_blur_g(&self, t: &GBuf, slope: &GBuf, samples: u32, intensity: f32, mode: u32) -> Option<GBuf> {
            let gu = Uni::default().u(slope.w as u32).u(slope.h as u32).done();
            let gv = self.map_pixels_g("slope_grad", SLOPE_GRAD_WGSL, gu, &[slope])?;
            let n = samples.clamp(1, 64);
            let uni = Uni::default().u(t.w as u32).u(t.h as u32).u(n).u(mode).f(intensity / n as f32).f(slope.w as f32 * 0.5).done();
            let src = format!("{}{}{}", SLOPE_WGSL, samp_fn("samp_t", "src"), samp_fn("samp_g", "gv"));
            self.map_pixels_g("slope_blur", &src, uni, &[t, &gv])
        }

        pub fn slope_blur(&self, t: &Tex, slope: &Tex, samples: u32, intensity: f32, mode: u32) -> Option<Tex> {
            self.roundtrip(&[t, slope], |g| self.slope_blur_g(g[0], g[1], samples, intensity, mode))
        }

        /// Per-pixel colour operations that keep a chain on the device: 0 invert, 1 contrast then optional invert
        /// (`a` = contrast, `b` = invert flag), 2 solid fill (`a`..`d` = r, g, b, alpha; `t` only gives the size).
        pub fn pointwise_g(&self, t: &GBuf, op: u32, a: f32, b: f32, c: f32, d: f32) -> Option<GBuf> {
            let uni = Uni::default().u(t.w as u32).u(t.h as u32).u(op).u(0).f(a).f(b).f(c).f(d).done();
            self.map_pixels_g("pointwise", POINTWISE_WGSL, uni, &[t])
        }

        /// A new device picture filled with one colour.
        pub fn solid_g(&self, w: usize, h: usize, rgba: [f32; 4]) -> Option<GBuf> {
            if !self.fits(w, h) {
                return None;
            }
            let dummy = self.gbuf(w, h);
            self.pointwise_g(&dummy, 2, rgba[0], rgba[1], rgba[2], rgba[3])
        }

        /// The picture fitted into a `size` x `size` square with dark bars (what the node cards show), as RGBA8.
        pub fn thumb_g(&self, t: &GBuf, size: usize) -> Option<Vec<u8>> {
            if !self.fits(size, size) || !self.fits(t.w, t.h) {
                return None;
            }
            let (w, h) = (t.w.max(1) as f32, t.h.max(1) as f32);
            let k = (size as f32 / w).min(size as f32 / h);
            let (tw, th) = ((w * k).round().max(1.0), (h * k).round().max(1.0));
            let (ox, oy) = ((size as f32 - tw) * 0.5, (size as f32 - th) * 0.5);
            let src = format!("{}{}", THUMB_WGSL, samp_fn("samp", "src"));
            let pipe = self.pipeline("thumb", &src);
            let uni = self.uniform(&Uni::default().u(t.w as u32).u(t.h as u32).u(size as u32).u(0).f(tw).f(th).f(ox).f(oy).done());
            let out = self.gbuf(size, size);
            let mut enc = self.dev.create_command_encoder(&Default::default());
            self.dispatch(&mut enc, &pipe, &[
                wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: t.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: out.buf.as_entire_binding() },
            ], size, size);
            self.submit(enc);
            let px = self.download(&[&out])?.pop()?;
            let mut bytes = Vec::with_capacity(size * size * 4);
            for p in &px.px {
                if p[3] < 0.5 {
                    bytes.extend_from_slice(&[30, 30, 30, 255]);
                } else {
                    for c in &p[..3] {
                        bytes.push((c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
                    }
                    bytes.push(255);
                }
            }
            Some(bytes)
        }
    }

    impl Gpu {
        /// Horizon-based occlusion of the luminance of `t`; `table` is `ops::hbao_tap_table` (two `vec4` per tap).
        pub fn hbao_g(&self, t: &GBuf, table: &[[f32; 4]], dirs: u32, steps: u32, scale: f32, intensity: f32) -> Option<GBuf> {
            let (w, h) = (t.w, t.h);
            if !self.fits(w, h) || table.len() != (dirs * steps * 2) as usize || table.is_empty() {
                return None;
            }
            let pipe = self.pipeline("hbao", HBAO_WGSL);
            let uni = self.uniform(&Uni::default().u(w as u32).u(h as u32).u(dirs).u(steps).f(scale).f(intensity).f(0.0).f(0.0).done());
            let taps = self.buf((table.len() * 16) as u64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST);
            self.queue.write_buffer(&taps, 0, px_bytes(table));
            let out = self.gbuf(w, h);
            let mut enc = self.dev.create_command_encoder(&Default::default());
            self.dispatch(&mut enc, &pipe, &[
                wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: t.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: taps.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: out.buf.as_entire_binding() },
            ], w, h);
            self.submit(enc);
            Some(out)
        }

        pub fn hbao(&self, t: &Tex, table: &[[f32; 4]], dirs: u32, steps: u32, scale: f32, intensity: f32) -> Option<Tex> {
            self.roundtrip(&[t], |g| self.hbao_g(g[0], table, dirs, steps, scale, intensity))
        }
    }

    const HBAO_WGSL: &str = r#"
struct U { w: u32, h: u32, dirs: u32, steps: u32, scale: f32, intensity: f32, p0: f32, p1: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> taps: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> dst: array<vec4<f32>>;
fn lum(p: vec4<f32>) -> f32 { return 0.2126 * p.x + 0.7152 * p.y + 0.0722 * p.z; }
fn hv(x: u32, y: u32) -> f32 { return lum(src[y * u.w + x]); }
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let h0 = hv(id.x, id.y);
    var occ = 0.0;
    for (var k = 0u; k < u.dirs; k = k + 1u) {
        var best = 0.0;
        for (var j = 0u; j < u.steps; j = j + 1u) {
            let i = (k * u.steps + j) * 2u;
            let t0 = taps[i];
            let t1 = taps[i + 1u];
            let ya = (id.y + u32(t0.y)) % u.h;
            var yb = ya + 1u;
            if (yb == u.h) { yb = 0u; }
            let xa = (id.x + u32(t0.x)) % u.w;
            var xb = xa + 1u;
            if (xb == u.w) { xb = 0u; }
            let a = hv(xa, ya);
            let b = hv(xb, ya);
            let c = hv(xa, yb);
            let d = hv(xb, yb);
            let top = a + (b - a) * t0.z;
            let bot = c + (d - c) * t0.z;
            let hs = top + (bot - top) * t0.w;
            let dz = (hs - h0) * u.scale;
            var q = sqrt(dz * dz + t1.x);
            if (!(q > 1e-9)) { q = 1e-9; }
            let v = dz / q * t1.y;
            if (v > best) { best = v; }
        }
        occ = occ + best;
    }
    let a = clamp(1.0 - occ / f32(u.dirs) * u.intensity, 0.0, 1.0);
    dst[id.y * u.w + id.x] = vec4<f32>(a, a, a, 1.0);
}
"#;

    impl Gpu {
        /// Ray-marched height shadows (the Shadows nodes): see `nodes10_img` for the CPU code this repeats.
        #[allow(clippy::too_many_arguments)]
        pub fn shadows_g(&self, t: &GBuf, sh: &super::ShadowParams) -> Option<GBuf> {
            let min_t = 0.5 / t.w.max(t.h) as f32;
            let uni = Uni::default()
                .u(t.w as u32).u(t.h as u32).u(sh.steps).u(sh.taps)
                .f(sh.height_scale).f(sh.atten).f(sh.light_k).f(sh.opacity)
                .f(sh.soft).f(sh.len).f(sh.elev).u(sh.invert as u32)
                .f(sh.dir[0]).f(sh.dir[1]).f(min_t).f(0.0)
                .done();
            let src = format!("{}{}", SHADOWS_WGSL, samp_fn("samp", "src"));
            self.map_pixels_g("shadows", &src, uni, &[t])
        }

        pub fn shadows(&self, t: &Tex, sh: &super::ShadowParams) -> Option<Tex> {
            self.roundtrip(&[t], |g| self.shadows_g(g[0], sh))
        }
    }

    const SHADOWS_WGSL: &str = r#"
struct U { w: u32, h: u32, steps: u32, taps: u32, hscale: f32, atten: f32, light_k: f32, opacity: f32, soft: f32, len: f32, elev: f32, invert: u32, dx: f32, dy: f32, min_t: f32, pad: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;
fn hash2(x: i32, y: i32, seed: u32) -> u32 {
    var h = (bitcast<u32>(x) * 0x27d4eb2du) ^ (bitcast<u32>(y) * 0x165667b1u) ^ (seed * 0x9e3779b9u);
    h = h ^ (h >> 15u);
    h = h * 0x85ebca6bu;
    h = h ^ (h >> 13u);
    h = h * 0xc2b2ae35u;
    h = h ^ (h >> 16u);
    return h;
}
fn rnd(x: i32, y: i32, seed: u32) -> f32 { return f32(hash2(x, y, seed) >> 8u) / 16777216.0; }
fn lum(p: vec4<f32>) -> f32 { return 0.2126 * p.x + 0.7152 * p.y + 0.0722 * p.z; }
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let idx = id.y * u.w + id.x;
    let uu = (f32(id.x) + 0.5) / f32(u.w);
    let vv = (f32(id.y) + 0.5) / f32(u.h);
    let h0 = lum(samp(uu, vv)) * u.hscale;
    var lit = 0.0;
    for (var k = 0u; k < u.taps; k = k + 1u) {
        let j = (rnd(i32(idx), i32(k), 91u) - 0.5) * u.soft * 6.0;
        let sj = sin(j);
        let cj = cos(j);
        let d0 = u.dx * cj - u.dy * sj;
        let d1 = u.dx * sj + u.dy * cj;
        let off = rnd(i32(idx), i32(k), 97u);
        var shade = 0.0;
        for (var st = 0u; st < u.steps; st = st + 1u) {
            let t = (f32(st) + off) / f32(u.steps) * u.len;
            if (t <= u.min_t) { continue; }
            let ray = h0 + t * u.elev;
            if (lum(samp(uu + d0 * t, vv + d1 * t)) * u.hscale > ray + 1e-4) {
                shade = 1.0 - u.atten * (t / max(u.len, 1e-6));
                break;
            }
        }
        lit = lit + (1.0 - max(shade, 0.0));
    }
    lit = lit / f32(u.taps);
    var v = u.light_k * (1.0 - u.opacity * (1.0 - lit));
    if (u.invert != 0u) { v = 1.0 - v; }
    dst[idx] = vec4<f32>(v, v, v, 1.0);
}
"#;

    impl Gpu {
        /// Like `dispatch` for kernels that work on a line of invocations (`groups` workgroups of 256).
        fn dispatch_line(&self, enc: &mut wgpu::CommandEncoder, pipe: &wgpu::ComputePipeline, entries: &[wgpu::BindGroupEntry], groups: u32) {
            let bg = self.dev.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &pipe.get_bind_group_layout(0), entries });
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(groups.max(1), 1, 1);
        }

        /// Maps the first channel of `t` so that its percentiles (found on a strided sample of at most 65536 values, as
        /// `noise_styles::tone` does) land on `target`, optionally adds `balance` and clamps. Everything stays on the
        /// device: the sample is gathered, sorted by one workgroup and read by the mapping kernel.
        pub fn tone_g(&self, t: &GBuf, target: &[f32; 5], balance: Option<f32>) -> Option<GBuf> {
            if !self.fits(t.w, t.h) {
                return None;
            }
            let n = t.w * t.h;
            let step = (n / 65536).max(1);
            let m = (n + step - 1) / step;
            let p = m.next_power_of_two().max(2);
            let at = |q: f32| (((q * (m - 1) as f32).round() as usize).min(m - 1)) as u32;
            let idx = [at(0.01), at(0.1), at(0.5), at(0.9), at(0.99)];
            let mut uni = Uni::default().u(n as u32).u(step as u32).u(m as u32).u(p as u32);
            for i in idx {
                uni = uni.u(i);
            }
            uni = uni.u(balance.is_some() as u32).f(balance.unwrap_or(0.0)).u(t.w as u32);
            for v in target {
                uni = uni.f(*v);
            }
            let uni = self.uniform(&uni.f(0.0).f(0.0).f(0.0).done());
            let sorted = self.buf((p * 4) as u64, wgpu::BufferUsages::STORAGE);
            let gather = self.pipeline("tone_gather", TONE_GATHER_WGSL);
            let sort = self.pipeline("tone_sort", TONE_SORT_WGSL);
            let map = self.pipeline("tone_map", TONE_MAP_WGSL);
            let out = self.gbuf(t.w, t.h);
            let mut enc = self.dev.create_command_encoder(&Default::default());
            self.dispatch_line(&mut enc, &gather, &[
                wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: t.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: sorted.as_entire_binding() },
            ], ((p + 255) / 256) as u32);
            self.dispatch_line(&mut enc, &sort, &[
                wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: sorted.as_entire_binding() },
            ], 1);
            self.dispatch(&mut enc, &map, &[
                wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: t.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: sorted.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: out.buf.as_entire_binding() },
            ], t.w, t.h);
            self.submit(enc);
            Some(out)
        }

        pub fn tone(&self, t: &Tex, target: &[f32; 5], balance: Option<f32>) -> Option<Tex> {
            self.roundtrip(&[t], |g| self.tone_g(g[0], target, balance))
        }

        /// The raw octave sum of a styled noise (`noise_styles::raw`, with the Warp and Angle random displacements of
        /// the Preset Noise node). `oct` holds (period x, period y, weight, seed) per octave.
        #[allow(clippy::too_many_arguments)]
        pub fn styled_raw_g(&self, w: usize, h: usize, basis: u32, oct: &[(i32, i32, f32, u32)], ramp: f32, wp: f32, ar: f32, seed: u32) -> Option<GBuf> {
            if !self.fits(w, h) || oct.is_empty() || oct.len() > 64 {
                return None;
            }
            let pipe = self.pipeline("styled_raw", STYLED_RAW_WGSL);
            let uni = self.uniform(&Uni::default().u(w as u32).u(h as u32).u(basis).u(oct.len() as u32).f(ramp).f(wp).f(ar).u(seed).done());
            let mut table: Vec<u8> = Vec::with_capacity(oct.len() * 16);
            for &(p, q, wt, sd) in oct {
                table.extend_from_slice(&(p as f32).to_le_bytes());
                table.extend_from_slice(&(q as f32).to_le_bytes());
                table.extend_from_slice(&wt.to_le_bytes());
                table.extend_from_slice(&sd.to_le_bytes());
            }
            let ob = self.buf(table.len() as u64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST);
            self.queue.write_buffer(&ob, 0, &table);
            let out = self.gbuf(w, h);
            let mut enc = self.dev.create_command_encoder(&Default::default());
            self.dispatch(&mut enc, &pipe, &[
                wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: ob.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: out.buf.as_entire_binding() },
            ], w, h);
            self.submit(enc);
            Some(out)
        }
    }

    const TONE_GATHER_WGSL: &str = r#"
struct U { n: u32, step: u32, m: u32, p: u32, i0: u32, i1: u32, i2: u32, i3: u32, i4: u32, bal_on: u32, bal: f32, w: u32, t0: f32, t1: f32, t2: f32, t3: f32, t4: f32, a: f32, b: f32, c: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> sorted: array<f32>;
@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= u.p) { return; }
    if (i < u.m) {
        let v = src[i * u.step].x;
        sorted[i] = select(v, 0.0, v != v);
    } else {
        sorted[i] = 3.0e38;
    }
}
"#;

    const TONE_SORT_WGSL: &str = r#"
struct U { n: u32, step: u32, m: u32, p: u32, i0: u32, i1: u32, i2: u32, i3: u32, i4: u32, bal_on: u32, bal: f32, w: u32, t0: f32, t1: f32, t2: f32, t3: f32, t4: f32, a: f32, b: f32, c: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(2) var<storage, read_write> sorted: array<f32>;
@compute @workgroup_size(256)
fn main(@builtin(local_invocation_index) li: u32) {
    let n = u.p;
    var k = 2u;
    loop {
        if (k > n) { break; }
        var j = k >> 1u;
        loop {
            if (j == 0u) { break; }
            for (var i = li; i < n; i = i + 256u) {
                let ixj = i ^ j;
                if (ixj > i) {
                    let a = sorted[i];
                    let b = sorted[ixj];
                    let asc = (i & k) == 0u;
                    if ((asc && a > b) || (!asc && a < b)) {
                        sorted[i] = b;
                        sorted[ixj] = a;
                    }
                }
            }
            storageBarrier();
            workgroupBarrier();
            j = j >> 1u;
        }
        k = k << 1u;
    }
}
"#;

    const TONE_MAP_WGSL: &str = r#"
struct U { n: u32, step: u32, m: u32, p: u32, i0: u32, i1: u32, i2: u32, i3: u32, i4: u32, bal_on: u32, bal: f32, w: u32, t0: f32, t1: f32, t2: f32, t3: f32, t4: f32, a: f32, b: f32, c: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> sorted: array<f32>;
@group(0) @binding(3) var<storage, read_write> dst: array<vec4<f32>>;
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let h = u.n / u.w;
    if (id.x >= u.w || id.y >= h) { return; }
    var s = array<f32, 5>(sorted[u.i0], sorted[u.i1], sorted[u.i2], sorted[u.i3], sorted[u.i4]);
    var t = array<f32, 5>(u.t0, u.t1, u.t2, u.t3, u.t4);
    var xs: array<f32, 5>;
    var ys: array<f32, 5>;
    var cnt = 0u;
    for (var i = 0u; i < 5u; i = i + 1u) {
        if (cnt == 0u || s[i] > xs[cnt - 1u] + 1e-7) {
            xs[cnt] = s[i];
            ys[cnt] = t[i];
            cnt = cnt + 1u;
        }
    }
    var g = u.t2;
    if (cnt >= 2u) {
        let x = src[id.y * u.w + id.x].x;
        var i = 0u;
        if (x <= xs[0]) {
            i = 0u;
        } else if (x >= xs[cnt - 1u]) {
            i = cnt - 2u;
        } else {
            var f = cnt - 1u;
            for (var a = 0u; a < cnt; a = a + 1u) {
                if (xs[a] > x) { f = a; break; }
            }
            i = min(select(f - 1u, 0u, f == 0u), cnt - 2u);
        }
        let x0 = xs[i];
        let x1 = xs[i + 1u];
        let y0 = ys[i];
        let y1 = ys[i + 1u];
        g = clamp(y0 + (x - x0) * (y1 - y0) / (x1 - x0), 0.0, 1.0);
        if (u.bal_on != 0u) { g = clamp(g + u.bal, 0.0, 1.0); }
    }
    dst[id.y * u.w + id.x] = vec4<f32>(g, g, g, 1.0);
}
"#;

    const STYLED_RAW_WGSL: &str = r#"
struct U { w: u32, h: u32, basis: u32, n: u32, ramp: f32, wp: f32, ar: f32, seed: u32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> oct: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;
fn mixh(x: u32) -> u32 {
    var h = x;
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return h;
}
fn hashp(x: i32, y: i32, seed: u32) -> u32 {
    return mixh(bitcast<u32>(x) + mixh(bitcast<u32>(y) + mixh(seed + 0x9e3779b9u)));
}
fn remi(a: i32, m: i32) -> i32 { return ((a % m) + m) % m; }
fn lattice(ix: i32, iy: i32, p: i32, q: i32, seed: u32) -> f32 {
    return f32(hashp(remi(ix, p), remi(iy, q), seed) >> 8u) / 8388608.0 - 1.0;
}
fn gdir(cx: i32, cy: i32, p: i32, q: i32, seed: u32, dx: f32, dy: f32) -> f32 {
    let idx = hashp(remi(cx, p), remi(cy, q), seed) >> 24u;
    let a = (f32(idx) + 0.5) / 256.0 * 6.28318530717959;
    return cos(a) * dx + sin(a) * dy;
}
fn fade(t: f32) -> f32 { return t * t * t * (t * (t * 6.0 - 15.0) + 10.0); }
fn gradient(uu: f32, vv: f32, p0: i32, q0: i32, seed: u32) -> f32 {
    let p = max(p0, 1);
    let q = max(q0, 1);
    let x = uu * f32(p);
    let y = vv * f32(q);
    let fx = floor(x);
    let fy = floor(y);
    let ix = i32(fx);
    let iy = i32(fy);
    let tx = x - fx;
    let ty = y - fy;
    let sx = fade(tx);
    let sy = fade(ty);
    let a = gdir(ix, iy, p, q, seed, tx, ty);
    let b = gdir(ix + 1, iy, p, q, seed, tx - 1.0, ty);
    let c = gdir(ix, iy + 1, p, q, seed, tx, ty - 1.0);
    let d = gdir(ix + 1, iy + 1, p, q, seed, tx - 1.0, ty - 1.0);
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    return (top + (bot - top) * sy) * 1.4;
}
fn bspline(t: f32) -> vec4<f32> {
    let t2 = t * t;
    let t3 = t2 * t;
    let s = 1.0 - t;
    return vec4<f32>(s * s * s / 6.0, (3.0 * t3 - 6.0 * t2 + 4.0) / 6.0, (-3.0 * t3 + 3.0 * t2 + 3.0 * t + 1.0) / 6.0, t3 / 6.0);
}
fn value(uu: f32, vv: f32, p: i32, q: i32, seed: u32) -> f32 {
    let x = uu * f32(p);
    let y = vv * f32(q);
    let fx = floor(x);
    let fy = floor(y);
    let ix = i32(fx);
    let iy = i32(fy);
    let wx = bspline(x - fx);
    let wy = bspline(y - fy);
    var acc = 0.0;
    for (var j = 0; j < 4; j = j + 1) {
        var row = 0.0;
        for (var i = 0; i < 4; i = i + 1) {
            row = row + wx[i] * lattice(ix + i - 1, iy + j - 1, p, q, seed);
        }
        acc = acc + wy[j] * row;
    }
    return acc * 1.8;
}
fn spots(uu: f32, vv: f32, p: i32, q: i32, seed: u32) -> f32 {
    let x = uu * f32(p);
    let y = vv * f32(q);
    let fx = floor(x);
    let fy = floor(y);
    let ix = i32(fx);
    let iy = i32(fy);
    var acc = 0.0;
    for (var j = -1; j <= 1; j = j + 1) {
        for (var i = -1; i <= 1; i = i + 1) {
            let cx = ix + i;
            let cy = iy + j;
            let h = hashp(remi(cx, p), remi(cy, q), seed);
            let jx = f32(h & 0x3ffu) / 1023.0;
            let jy = f32((h >> 10u) & 0x3ffu) / 1023.0;
            let amp = f32((h >> 20u) & 0xfffu) / 2047.5 - 1.0;
            let dx = x - (f32(cx) + jx);
            let dy = y - (f32(cy) + jy);
            acc = acc + amp * exp(-(dx * dx + dy * dy) * 4.0);
        }
    }
    return acc * 1.6;
}
fn basis_at(b: u32, uu: f32, vv: f32, p: i32, q: i32, seed: u32) -> f32 {
    if (b == 0u) { return value(uu, vv, p, q, seed); }
    if (b == 1u) { return gradient(uu, vv, p, q, seed); }
    if (b == 2u) { return spots(uu, vv, p, q, seed); }
    return 1.0 - 2.0 * abs(gradient(uu, vv, p, q, seed));
}
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let u0 = (f32(id.x) + 0.5) / f32(u.w);
    let v0 = (f32(id.y) + 0.5) / f32(u.h);
    let trend = u.ramp * (v0 * 2.0 - 1.0);
    var a = u0;
    var b = v0;
    if (u.wp > 0.0) {
        a = a + u.wp * 0.04 * gradient(u0, v0, 6, 6, u.seed + 301u);
        b = b + u.wp * 0.04 * gradient(u0, v0, 6, 6, u.seed + 302u);
    }
    if (u.ar > 0.0) {
        a = a + u.ar * 0.10 * gradient(u0, v0, 3, 3, u.seed + 311u);
        b = b + u.ar * 0.10 * gradient(u0, v0, 3, 3, u.seed + 312u);
    }
    var acc = 0.0;
    for (var k = 0u; k < u.n; k = k + 1u) {
        let o = oct[k];
        acc = acc + o.z * basis_at(u.basis, a, b, i32(o.x), i32(o.y), bitcast<u32>(o.w));
    }
    let g = acc + trend;
    dst[id.y * u.w + id.x] = vec4<f32>(g, g, g, 1.0);
}
"#;

    /// A byte buffer on the device (packed maps waiting to be read back).
    pub struct PBuf {
        pub buf: wgpu::Buffer,
        pub bytes: usize,
    }

    impl Gpu {
        fn pbuf(&self, bytes: usize) -> PBuf {
            let size = (bytes.max(4) + 3) / 4 * 4;
            PBuf { buf: self.buf(size as u64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST), bytes }
        }

        /// RGB of `t` as RGBA8 (alpha 255), green optionally flipped (`1 - g`).
        pub fn pack_rgb8(&self, t: &GBuf, flip_g: bool) -> Option<PBuf> {
            if !self.fits(t.w, t.h) {
                return None;
            }
            let pipe = self.pipeline("pack_rgb8", PACK_RGB8_WGSL);
            let uni = self.uniform(&Uni::default().u(t.w as u32).u(t.h as u32).u(flip_g as u32).u(0).done());
            let out = self.pbuf(t.w * t.h * 4);
            let mut enc = self.dev.create_command_encoder(&Default::default());
            self.dispatch(&mut enc, &pipe, &[
                wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: t.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: out.buf.as_entire_binding() },
            ], t.w, t.h);
            self.submit(enc);
            Some(out)
        }

        /// R = luminance of `r`, G = of `m`, B = of `a` as RGBA8 (alpha 255); a missing map gives its default value.
        pub fn pack_rma(&self, r: Option<&GBuf>, m: Option<&GBuf>, a: Option<&GBuf>, dflt: [f32; 3]) -> Option<PBuf> {
            let any = r.or(m).or(a)?;
            let (w, h) = (any.w, any.h);
            if !self.fits(w, h) || [r, m, a].iter().flatten().any(|t| (t.w, t.h) != (w, h)) {
                return None;
            }
            let pipe = self.pipeline("pack_rma", PACK_RMA_WGSL);
            let flags = r.is_some() as u32 | (m.is_some() as u32) << 1 | (a.is_some() as u32) << 2;
            let uni = self.uniform(&Uni::default().u(w as u32).u(h as u32).u(flags).u(0).f(dflt[0]).f(dflt[1]).f(dflt[2]).f(0.0).done());
            let out = self.pbuf(w * h * 4);
            let mut enc = self.dev.create_command_encoder(&Default::default());
            self.dispatch(&mut enc, &pipe, &[
                wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: r.unwrap_or(any).buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: m.unwrap_or(any).buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: a.unwrap_or(any).buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: out.buf.as_entire_binding() },
            ], w, h);
            self.submit(enc);
            Some(out)
        }

        /// Luminance of `t` as 16-bit floats, two per 32-bit word (low half first).
        pub fn pack_half(&self, t: &GBuf) -> Option<PBuf> {
            if !self.fits(t.w, t.h) {
                return None;
            }
            let pipe = self.pipeline("pack_half", PACK_HALF_WGSL);
            let uni = self.uniform(&Uni::default().u(t.w as u32).u(t.h as u32).u(0).u(0).done());
            let out = self.pbuf(t.w * t.h * 2);
            let mut enc = self.dev.create_command_encoder(&Default::default());
            self.dispatch(&mut enc, &pipe, &[
                wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: t.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: out.buf.as_entire_binding() },
            ], t.w, t.h);
            self.submit(enc);
            Some(out)
        }

        /// Reads several byte buffers back with one submission and one wait.
        pub fn download_bytes(&self, bs: &[&PBuf]) -> Option<Vec<Vec<u8>>> {
            if bs.is_empty() {
                return Some(Vec::new());
            }
            DOWNLOADS.fetch_add(1, Ordering::Relaxed);
            let mut enc = self.dev.create_command_encoder(&Default::default());
            let stages: Vec<wgpu::Buffer> = bs
                .iter()
                .map(|b| {
                    let size = (b.bytes.max(4) + 3) / 4 * 4;
                    let stage = self.buf(size as u64, wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST);
                    enc.copy_buffer_to_buffer(&b.buf, 0, &stage, 0, size as u64);
                    stage
                })
                .collect();
            self.submit(enc);
            let (tx, rx) = std::sync::mpsc::channel();
            for s in &stages {
                let tx = tx.clone();
                s.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                    let _ = tx.send(r);
                });
            }
            drop(tx);
            self.dev.poll(wgpu::Maintain::Wait);
            for _ in 0..stages.len() {
                rx.recv().ok()?.ok()?;
            }
            let mut out = Vec::with_capacity(bs.len());
            for (b, stage) in bs.iter().zip(&stages) {
                let data = stage.slice(..).get_mapped_range();
                out.push(data[..b.bytes.min(data.len())].to_vec());
                drop(data);
                stage.unmap();
            }
            Some(out)
        }
    }

    impl Gpu {
        /// `t` as RGBA8 for the 2D view (alpha kept, rounding as `qimg`) and the smallest and largest red, green or blue
        /// value, found on the device (two words: ordered-integer minimum and maximum).
        pub fn pack_view(&self, t: &GBuf) -> Option<(PBuf, PBuf)> {
            if !self.fits(t.w, t.h) {
                return None;
            }
            let pipe = self.pipeline("pack_view", PACK_VIEW_WGSL);
            let uni = self.uniform(&Uni::default().u(t.w as u32).u(t.h as u32).u(0).u(0).done());
            let out = self.pbuf(t.w * t.h * 4);
            let range = self.pbuf(8);
            self.queue.write_buffer(&range.buf, 0, &[0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0]);
            let mut enc = self.dev.create_command_encoder(&Default::default());
            self.dispatch(&mut enc, &pipe, &[
                wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: t.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: out.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: range.buf.as_entire_binding() },
            ], t.w, t.h);
            self.submit(enc);
            Some((out, range))
        }
    }

    const PACK_VIEW_WGSL: &str = r#"
struct U { w: u32, h: u32, p0: u32, p1: u32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<u32>;
@group(0) @binding(3) var<storage, read_write> rng: array<atomic<u32>, 2>;
var<workgroup> lo: atomic<u32>;
var<workgroup> hi: atomic<u32>;
fn b8(v: f32) -> u32 { return u32(clamp(v, 0.0, 1.0) * 255.0 + 0.5); }
fn key(f: f32) -> u32 {
    let b = bitcast<u32>(f);
    return select(b | 0x80000000u, ~b, (b >> 31u) == 1u);
}
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    if (li == 0u) {
        atomicStore(&lo, 0xffffffffu);
        atomicStore(&hi, 0u);
    }
    workgroupBarrier();
    if (id.x < u.w && id.y < u.h) {
        let i = id.y * u.w + id.x;
        let p = src[i];
        dst[i] = b8(p.x) | (b8(p.y) << 8u) | (b8(p.z) << 16u) | (b8(p.w) << 24u);
        let a = min(p.x, min(p.y, p.z));
        let b = max(p.x, max(p.y, p.z));
        atomicMin(&lo, key(a));
        atomicMax(&hi, key(b));
    }
    workgroupBarrier();
    if (li == 0u) {
        atomicMin(&rng[0], atomicLoad(&lo));
        atomicMax(&rng[1], atomicLoad(&hi));
    }
}
"#;

    const PACK_RGB8_WGSL: &str = r#"
struct U { w: u32, h: u32, flip: u32, pad: u32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<u32>;
fn b8(v: f32) -> u32 { return u32(clamp(v, 0.0, 1.0) * 255.0 + 0.5); }
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let i = id.y * u.w + id.x;
    let p = src[i];
    var g = p.y;
    if (u.flip != 0u) { g = 1.0 - g; }
    dst[i] = b8(p.x) | (b8(g) << 8u) | (b8(p.z) << 16u) | (255u << 24u);
}
"#;

    const PACK_RMA_WGSL: &str = r#"
struct U { w: u32, h: u32, flags: u32, pad: u32, dr: f32, dm: f32, da: f32, pad2: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> rs: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> ms: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> as_: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> dst: array<u32>;
fn b8(v: f32) -> u32 { return u32(clamp(v, 0.0, 1.0) * 255.0 + 0.5); }
fn lum(p: vec4<f32>) -> f32 { return 0.2126 * p.x + 0.7152 * p.y + 0.0722 * p.z; }
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let i = id.y * u.w + id.x;
    var r = u.dr;
    var m = u.dm;
    var a = u.da;
    if ((u.flags & 1u) != 0u) { r = lum(rs[i]); }
    if ((u.flags & 2u) != 0u) { m = lum(ms[i]); }
    if ((u.flags & 4u) != 0u) { a = lum(as_[i]); }
    dst[i] = b8(r) | (b8(m) << 8u) | (b8(a) << 16u) | (255u << 24u);
}
"#;

    const PACK_HALF_WGSL: &str = r#"
struct U { w: u32, h: u32, pad: u32, pad2: u32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<u32>;
fn lum(p: vec4<f32>) -> f32 { return 0.2126 * p.x + 0.7152 * p.y + 0.0722 * p.z; }
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let i = id.y * u.w + id.x;
    if ((i & 1u) != 0u) { return; }
    let n = u.w * u.h;
    let lo = lum(src[i]);
    var hi = 0.0;
    if (i + 1u < n) { hi = lum(src[i + 1u]); }
    dst[i / 2u] = pack2x16float(vec2<f32>(lo, hi));
}
"#;

    pub struct Scene {
        nodes: wgpu::Buffer,
        tris: wgpu::Buffer,
        masks: wgpu::Buffer,
        words: u32,
        cull: bool,
    }

    fn u32_bytes(p: &[[u32; 4]]) -> &[u8] {
        // SAFETY: [u32; 4] is plain old data with no padding
        unsafe { std::slice::from_raw_parts(p.as_ptr() as *const u8, std::mem::size_of_val(p)) }
    }

    impl Gpu {
        pub fn scene(&self, d: &super::SceneData) -> Option<Scene> {
            let (nb, tb) = ((d.nodes.len() * 16) as u64, (d.tris.len() * 16) as u64);
            if self.dev.limits().max_storage_buffers_per_shader_stage < 5 || nb == 0 || tb == 0 || nb > self.max_bytes || tb > self.max_bytes {
                return None;
            }
            let mk = |data: &[u8], size: u64| {
                let b = self.buf(size.max(16), wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST);
                self.queue.write_buffer(&b, 0, data);
                b
            };
            let nodes = mk(u32_bytes(&d.nodes), nb);
            let tris = mk(u32_bytes(&d.tris), tb);
            let mut mb: Vec<u8> = d.masks.iter().flat_map(|w| w.to_le_bytes()).collect();
            while mb.len() < 16 {
                mb.push(0);
            }
            let mlen = mb.len() as u64;
            let masks = mk(&mb, mlen);
            Some(Scene { nodes, tris, masks, words: d.words.max(1), cull: d.cull_back })
        }

        /// Run one ray kernel over `n` items in slices; `inp` words per item `iw`, `out` words per item 1 (a `vec4<u32>`).
        fn run_rays(&self, name: &'static str, src: &str, sc: &Scene, input: &[[u32; 4]], iw: usize, per_item_rays: usize, uni: impl Fn(u32) -> Vec<u8>) -> Option<Vec<[u32; 4]>> {
            let n = input.len() / iw;
            let pipe = self.pipeline(name, src);
            let slice = (4_000_000 / per_item_rays.max(1)).clamp(4096, 1 << 20);
            if (slice * iw * 16) as u64 > self.max_bytes {
                return None;
            }
            let inb = self.buf((slice * iw * 16) as u64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST);
            let outb = self.buf((slice * 16) as u64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC);
            let stage = self.buf((slice * 16) as u64, wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST);
            let mut result: Vec<[u32; 4]> = Vec::with_capacity(n);
            let mut at = 0;
            while at < n {
                let cnt = (n - at).min(slice);
                self.queue.write_buffer(&inb, 0, u32_bytes(&input[at * iw..(at + cnt) * iw]));
                let ub = self.uniform(&uni(cnt as u32));
                let bg = self.dev.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &pipe.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: ub.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: sc.nodes.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 2, resource: sc.tris.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 3, resource: sc.masks.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 4, resource: inb.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 5, resource: outb.as_entire_binding() },
                    ],
                });
                let mut enc = self.dev.create_command_encoder(&Default::default());
                {
                    let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
                    pass.set_pipeline(&pipe);
                    pass.set_bind_group(0, &bg, &[]);
                    pass.dispatch_workgroups(((cnt + 63) / 64) as u32, 1, 1);
                }
                enc.copy_buffer_to_buffer(&outb, 0, &stage, 0, (cnt * 16) as u64);
                self.queue.submit(Some(enc.finish()));
                let sl = stage.slice(..(cnt * 16) as u64);
                let (tx, rx) = std::sync::mpsc::channel();
                sl.map_async(wgpu::MapMode::Read, move |r| {
                    let _ = tx.send(r);
                });
                self.dev.poll(wgpu::Maintain::Wait);
                rx.recv().ok()?.ok()?;
                let data = sl.get_mapped_range();
                let start = result.len();
                result.resize(start + cnt, [0; 4]);
                // SAFETY: the destination holds exactly cnt * 16 bytes of plain data
                unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), result[start..].as_mut_ptr() as *mut u8, cnt * 16) };
                drop(data);
                stage.unmap();
                at += cnt;
            }
            Some(result)
        }

        pub fn trace(&self, sc: &Scene, rays: &[[u32; 4]]) -> Option<Vec<[u32; 4]>> {
            let cull = sc.cull as u32;
            let words = sc.words;
            self.run_rays("bake_trace", &format!("{RAY_COMMON_WGSL}{TRACE_MAIN_WGSL}"), sc, rays, 3, 1, move |n| Uni::default().u(n).u(words).u(cull).u(0).f(0.0).f(0.0).u(0).u(0).done())
        }

        pub fn hemi(&self, sc: &Scene, samples: &[[u32; 4]], p: super::HemiParams) -> Option<Vec<[f32; 4]>> {
            let words = sc.words;
            let out = self.run_rays("bake_hemi", &format!("{RAY_COMMON_WGSL}{HEMI_MAIN_WGSL}"), sc, samples, 2, p.rays as usize, move |n| {
                Uni::default().u(n).u(words).u(0).u(p.rays).f(p.dist).f(p.falloff).u(p.thickness as u32).u(p.seed).done()
            })?;
            Some(out.into_iter().map(|w| [f32::from_bits(w[0]), f32::from_bits(w[1]), f32::from_bits(w[2]), f32::from_bits(w[3])]).collect())
        }
    }

    /// BVH traversal shared by the baker kernels. Words are read as `u32` and converted with `bitcast`, so triangle and
    /// node data keep their exact bits.
    const RAY_COMMON_WGSL: &str = r#"
struct U { n: u32, words: u32, cull: u32, m: u32, dist: f32, falloff: f32, mode: u32, seed: u32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> nodes: array<vec4<u32>>;
@group(0) @binding(2) var<storage, read> tris: array<vec4<u32>>;
@group(0) @binding(3) var<storage, read> masks: array<u32>;
@group(0) @binding(4) var<storage, read> inp: array<vec4<u32>>;
@group(0) @binding(5) var<storage, read_write> outp: array<vec4<u32>>;

fn fv(x: vec3<u32>) -> vec3<f32> { return bitcast<vec3<f32>>(x); }

struct Hit { t: f32, tri: u32, u: f32, v: f32 };

fn slab(bmin: vec3<f32>, bmax: vec3<f32>, o: vec3<f32>, inv: vec3<f32>, tm: f32) -> f32 {
    let a = (bmin - o) * inv;
    let b = (bmax - o) * inv;
    let lo = min(a, b);
    let hi = max(a, b);
    let t0 = max(max(max(lo.x, lo.y), lo.z), 0.0);
    let t1 = min(min(min(hi.x, hi.y), hi.z), tm);
    if (t0 <= t1) { return t0; }
    return -1.0;
}

fn fixd(x: f32) -> f32 {
    if (abs(x) < 1e-12) {
        if (x < 0.0) { return -1e-12; }
        return 1e-12;
    }
    return x;
}

fn closest(o: vec3<f32>, d: vec3<f32>, tmin: f32, tmax: f32, flt: u32, cull: bool) -> Hit {
    var best: Hit;
    best.t = -1.0;
    best.tri = 0u;
    best.u = 0.0;
    best.v = 0.0;
    let inv = vec3<f32>(1.0 / fixd(d.x), 1.0 / fixd(d.y), 1.0 / fixd(d.z));
    var tm = tmax;
    let r0 = nodes[0];
    let r1 = nodes[1];
    if (slab(fv(r0.xyz), fv(r1.xyz), o, inv, tm) < 0.0) { return best; }
    var stack: array<u32, 64>;
    var sp = 1u;
    stack[0] = 0u;
    loop {
        if (sp == 0u) { break; }
        sp = sp - 1u;
        let ni = stack[sp];
        let n0 = nodes[2u * ni];
        let n1 = nodes[2u * ni + 1u];
        if (n1.w > 0u) {
            for (var k = 0u; k < n1.w; k = k + 1u) {
                let s = n0.w + k;
                let t0 = tris[3u * s];
                let t1 = tris[3u * s + 1u];
                let t2 = tris[3u * s + 2u];
                if (flt != 0xffffffffu) {
                    let obj = t1.w;
                    let word = masks[flt * u.words + (obj >> 5u)];
                    if (((word >> (obj & 31u)) & 1u) == 0u) { continue; }
                }
                let p0 = fv(t0.xyz);
                let e1 = fv(t1.xyz);
                let e2 = fv(t2.xyz);
                let pv = cross(d, e2);
                let det = dot(e1, pv);
                if (abs(det) < 1e-20 || (cull && det < 0.0)) { continue; }
                let idet = 1.0 / det;
                let tv = o - p0;
                let uu = dot(tv, pv) * idet;
                if (uu < -1e-6 || uu > 1.000001) { continue; }
                let q = cross(tv, e1);
                let vv = dot(d, q) * idet;
                if (vv < -1e-6 || uu + vv > 1.000001) { continue; }
                let t = dot(e2, q) * idet;
                if (t > tmin && t < tm) {
                    tm = t;
                    best.t = t;
                    best.tri = t0.w;
                    best.u = clamp(uu, 0.0, 1.0);
                    best.v = clamp(vv, 0.0, 1.0);
                }
            }
        } else {
            let l = n0.w;
            let a0 = nodes[2u * l];
            let a1 = nodes[2u * l + 1u];
            let b0 = nodes[2u * l + 2u];
            let b1 = nodes[2u * l + 3u];
            let ta = slab(fv(a0.xyz), fv(a1.xyz), o, inv, tm);
            let tb = slab(fv(b0.xyz), fv(b1.xyz), o, inv, tm);
            if (ta >= 0.0 && tb >= 0.0) {
                if (sp + 2u <= 64u) {
                    if (ta <= tb) {
                        stack[sp] = l + 1u;
                        stack[sp + 1u] = l;
                    } else {
                        stack[sp] = l;
                        stack[sp + 1u] = l + 1u;
                    }
                    sp = sp + 2u;
                }
            } else if (ta >= 0.0) {
                stack[sp] = l;
                sp = sp + 1u;
            } else if (tb >= 0.0) {
                stack[sp] = l + 1u;
                sp = sp + 1u;
            }
        }
    }
    return best;
}
"#;

    const TRACE_MAIN_WGSL: &str = r#"
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= u.n) { return; }
    let a = inp[3u * i];
    let b = inp[3u * i + 1u];
    let c = inp[3u * i + 2u];
    let h = closest(fv(a.xyz), fv(b.xyz), bitcast<f32>(a.w), bitcast<f32>(b.w), c.x, u.cull == 1u);
    outp[i] = vec4<u32>(bitcast<u32>(h.t), h.tri, bitcast<u32>(h.u), bitcast<u32>(h.v));
}
"#;

    const HEMI_MAIN_WGSL: &str = r#"
fn hash2(x: i32, y: i32, seed: u32) -> u32 {
    var h = (bitcast<u32>(x) * 0x27d4eb2du) ^ (bitcast<u32>(y) * 0x165667b1u) ^ (seed * 0x9e3779b9u);
    h = h ^ (h >> 15u);
    h = h * 0x85ebca6bu;
    h = h ^ (h >> 13u);
    h = h * 0xc2b2ae35u;
    h = h ^ (h >> 16u);
    return h;
}
fn rnd(x: i32, y: i32, seed: u32) -> f32 { return f32(hash2(x, y, seed) >> 8u) / 16777216.0; }

fn hemi_dir(n: vec3<f32>, a: f32, b: f32) -> vec3<f32> {
    var up = vec3<f32>(1.0, 0.0, 0.0);
    if (abs(n.y) < 0.99) { up = vec3<f32>(0.0, 1.0, 0.0); }
    let tg = normalize(cross(up, n));
    let bt = cross(n, tg);
    let phi = a * 6.2831855;
    let ct = sqrt(1.0 - b);
    let st = sqrt(b);
    return normalize(tg * (st * cos(phi)) + bt * (st * sin(phi)) + n * ct);
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= u.n) { return; }
    let a = inp[2u * i];
    let b = inp[2u * i + 1u];
    let org = fv(a.xyz);
    let nrm = fv(b.xyz);
    let px = i32(a.w);
    let py = i32(b.w & 0xffffu);
    let sub = b.w >> 16u;
    let r1 = rnd(px, py, u.seed + sub * 3u);
    let r2 = rnd(py, px, u.seed + 1u + sub * 3u);
    var occ = 0.0;
    var dsum = vec3<f32>(0.0);
    for (var k = 0u; k < u.m; k = k + 1u) {
        let ha = f32(k) / f32(u.m);
        let hb = f32(reverseBits(k)) * 2.3283064e-10;
        let dd = hemi_dir(nrm, fract(ha + r1), fract(hb + r2));
        let h = closest(org, dd, 1e-7, u.dist, 0xffffffffu, false);
        if (u.mode == 1u) {
            if (h.t >= 0.0) { occ = occ + clamp(h.t / u.dist, 0.0, 1.0); } else { occ = occ + 1.0; }
        } else {
            if (h.t >= 0.0) {
                var f = 1.0;
                if (u.falloff > 0.0) { f = pow(clamp(1.0 - h.t / u.dist, 0.0, 1.0), max(u.falloff, 0.01)); }
                occ = occ + f;
                dsum = dsum + dd * (1.0 - f);
            } else {
                dsum = dsum + dd;
            }
        }
    }
    outp[i] = vec4<u32>(bitcast<u32>(occ), bitcast<u32>(dsum.x), bitcast<u32>(dsum.y), bitcast<u32>(dsum.z));
}
"#;

    /// Bilinear wrapped lookup in the storage buffer `buf` (same maths as `Tex::sample`); needs `u.w` and `u.h`.
    fn samp_fn(name: &str, buf: &str) -> String {
        format!(
            r#"
fn wrapi_{name}(a: i32, m: i32) -> i32 {{
    let q = i32(floor(f32(a) / f32(m)));
    return a - q * m;
}}
fn texel_{name}(x: i32, y: i32) -> vec4<f32> {{
    return {buf}[u32(wrapi_{name}(y, i32(u.h))) * u.w + u32(wrapi_{name}(x, i32(u.w)))];
}}
fn {name}(uu: f32, vv: f32) -> vec4<f32> {{
    let x = uu * f32(u.w) - 0.5;
    let y = vv * f32(u.h) - 0.5;
    let x0 = floor(x);
    let y0 = floor(y);
    let fx = x - x0;
    let fy = y - y0;
    let xa = i32(x0);
    let ya = i32(y0);
    let a = texel_{name}(xa, ya);
    let b = texel_{name}(xa + 1, ya);
    let c = texel_{name}(xa, ya + 1);
    let d = texel_{name}(xa + 1, ya + 1);
    let top = a + (b - a) * fx;
    let bot = c + (d - c) * fx;
    return top + (bot - top) * fy;
}}
"#
        )
    }

    const RADIAL_WGSL: &str = r#"
struct U { w: u32, h: u32, mode: u32, n: u32, cx: f32, cy: f32, amt: f32, rad: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;
fn wd(d: f32) -> f32 { return d - sign(d) * floor(abs(d) + 0.5); }
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let uu = (f32(id.x) + 0.5) / f32(u.w);
    let vv = (f32(id.y) + 0.5) / f32(u.h);
    let dx = wd(uu - u.cx);
    let dy = wd(vv - u.cy);
    let r = sqrt(dx * dx + dy * dy);
    var acc = vec4<f32>(0.0);
    if (u.mode == 1u) {
        let t = clamp(1.0 - r / u.rad, 0.0, 1.0);
        let amt = u.amt * t * t * (3.0 - 2.0 * t);
        for (var k = 0u; k < 24u; k = k + 1u) {
            let sc = 1.0 - amt * (f32(k) / 23.0);
            acc = acc + samp(u.cx + dx * sc, u.cy + dy * sc) / 24.0;
        }
    } else {
        let fade = clamp((0.5 - r) / 0.15, 0.0, 1.0);
        for (var k = 0u; k < u.n; k = k + 1u) {
            var th = 0.0;
            if (u.n > 1u) { th = (f32(k) / f32(u.n - 1u) - 0.5) * u.amt * fade; }
            let sn = sin(th);
            let cs = cos(th);
            acc = acc + samp(u.cx + dx * cs - dy * sn, u.cy + dx * sn + dy * cs) / f32(u.n);
        }
    }
    dst[id.y * u.w + id.x] = acc;
}
"#;

    const BENT_WGSL: &str = r#"
struct U { w: u32, h: u32, dirs: u32, steps: u32, rad: f32, relief: f32, ysign: f32, want_ao: u32, aspect: f32, a: f32, b: f32, c: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;
fn lum(p: vec4<f32>) -> f32 { return 0.2126 * p.x + 0.7152 * p.y + 0.0722 * p.z; }
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let uu = (f32(id.x) + 0.5) / f32(u.w);
    let vv = (f32(id.y) + 0.5) / f32(u.h);
    let h0 = lum(src[id.y * u.w + id.x]);
    var acc = vec3<f32>(0.0);
    var ao = 0.0;
    let pi = 3.14159265358979;
    for (var i = 0u; i < u.dirs; i = i + 1u) {
        let ang = (f32(i) + 0.5) / f32(u.dirs) * 6.28318530717959;
        let sn = sin(ang);
        let cn = cos(ang);
        var tanmax = 0.0;
        for (var k = 1u; k <= u.steps; k = k + 1u) {
            let d = u.rad * f32(k) / f32(u.steps);
            let hs = lum(samp(uu + cn * d, vv + sn * d * u.aspect));
            tanmax = max(tanmax, (hs - h0) * u.relief / d);
        }
        let hz = atan(tanmax);
        let theta = (hz + pi * 0.5) * 0.5;
        let vis = (pi * 0.5 - hz) / (pi * 0.5);
        acc = acc + vec3<f32>(vis * cn * cos(theta), vis * u.ysign * sn * cos(theta), vis * sin(theta));
        ao = ao + cos(hz) * cos(hz);
    }
    var o = vec4<f32>(0.5, 0.5, 1.0, 1.0);
    if (u.want_ao != 0u) {
        let a = clamp(ao / f32(u.dirs), 0.0, 1.0);
        o = vec4<f32>(a, a, a, 1.0);
    } else {
        let l = length(acc);
        if (l >= 1e-8) {
            let n = acc / l * 0.5 + vec3<f32>(0.5);
            o = vec4<f32>(n.x, n.y, n.z, 1.0);
        }
    }
    dst[id.y * u.w + id.x] = o;
}
"#;

    const BILATERAL_WGSL: &str = r#"
struct U { w: u32, h: u32, rad: i32, ss: f32, sr: f32, a: f32, b: f32, c: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> guide: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> dst: array<vec4<f32>>;
fn wrapi(a: i32, m: i32) -> i32 {
    let q = i32(floor(f32(a) / f32(m)));
    return a - q * m;
}
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let x = i32(id.x);
    let y = i32(id.y);
    let c = guide[id.y * u.w + id.x];
    var acc = vec4<f32>(0.0);
    var wsum = 0.0;
    let inv_s = 1.0 / (2.0 * u.ss * u.ss);
    let inv_r = 1.0 / (2.0 * u.sr * u.sr);
    for (var dy = -u.rad; dy <= u.rad; dy = dy + 1) {
        let yy = u32(wrapi(y + dy, i32(u.h)));
        for (var dx = -u.rad; dx <= u.rad; dx = dx + 1) {
            let i = yy * u.w + u32(wrapi(x + dx, i32(u.w)));
            let gp = guide[i];
            let dc = (gp.x - c.x) * (gp.x - c.x) + (gp.y - c.y) * (gp.y - c.y) + (gp.z - c.z) * (gp.z - c.z);
            let wgt = exp(-f32(dx * dx + dy * dy) * inv_s - dc * inv_r);
            acc = acc + src[i] * wgt;
            wsum = wsum + wgt;
        }
    }
    dst[id.y * u.w + id.x] = acc / wsum;
}
"#;

    const SLOPE_GRAD_WGSL: &str = r#"
struct U { w: u32, h: u32, a: u32, b: u32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;
fn lum(p: vec4<f32>) -> f32 { return 0.2126 * p.x + 0.7152 * p.y + 0.0722 * p.z; }
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let x = id.x;
    let y = id.y;
    var xm = x - 1u; if (x == 0u) { xm = u.w - 1u; }
    var xp = x + 1u; if (x + 1u == u.w) { xp = 0u; }
    var ym = y - 1u; if (y == 0u) { ym = u.h - 1u; }
    var yp = y + 1u; if (y + 1u == u.h) { yp = 0u; }
    let gx = lum(src[y * u.w + xp]) - lum(src[y * u.w + xm]);
    let gy = lum(src[yp * u.w + x]) - lum(src[ym * u.w + x]);
    dst[y * u.w + x] = vec4<f32>(gx, gy, 0.0, 0.0);
}
"#;

    const SLOPE_WGSL: &str = r#"
struct U { w: u32, h: u32, n: u32, mode: u32, step: f32, gain: f32, a: f32, b: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> gv: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> dst: array<vec4<f32>>;
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    var pu = (f32(id.x) + 0.5) / f32(u.w);
    var pv = (f32(id.y) + 0.5) / f32(u.h);
    var acc = vec4<f32>(0.0);
    if (u.mode == 1u) { acc = vec4<f32>(1.0); }
    for (var k = 0u; k < u.n; k = k + 1u) {
        let p = samp_t(pu, pv);
        if (u.mode == 1u) { acc = min(acc, p); }
        else if (u.mode == 2u) { acc = max(acc, p); }
        else { acc = acc + p / f32(u.n); }
        let g = samp_g(pu, pv);
        let l = max(sqrt(g.x * g.x + g.y * g.y), 1e-6);
        let kk = min(l * u.gain, 1.0) * u.step / l;
        pu = pu + g.x * kk;
        pv = pv + g.y * kk;
    }
    dst[id.y * u.w + id.x] = acc;
}
"#;

    const EXTRUDE_WGSL: &str = r#"
struct U { w: u32, h: u32, first: u32, dx: f32, dy: f32, drop: f32, a: f32, b: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> prev: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> dst: array<vec4<f32>>;
fn lum(p: vec4<f32>) -> f32 { return 0.2126 * p.x + 0.7152 * p.y + 0.0722 * p.z; }
fn wrapi(a: i32, m: i32) -> i32 {
    let q = i32(floor(f32(a) / f32(m)));
    return a - q * m;
}
fn pv(x: i32, y: i32) -> f32 {
    let i = u32(wrapi(y, i32(u.h))) * u.w + u32(wrapi(x, i32(u.w)));
    if (u.first != 0u) { return lum(src[i]); }
    return prev[i].x;
}
fn psamp(uu: f32, vv: f32) -> f32 {
    let x = uu * f32(u.w) - 0.5;
    let y = vv * f32(u.h) - 0.5;
    let x0 = floor(x);
    let y0 = floor(y);
    let fx = x - x0;
    let fy = y - y0;
    let xa = i32(x0);
    let ya = i32(y0);
    let top = mix(pv(xa, ya), pv(xa + 1, ya), fx);
    let bot = mix(pv(xa, ya + 1), pv(xa + 1, ya + 1), fx);
    return mix(top, bot, fy);
}
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let uu = (f32(id.x) + 0.5) / f32(u.w);
    let vv = (f32(id.y) + 0.5) / f32(u.h);
    let back = psamp(uu - u.dx, vv - u.dy) - u.drop;
    let g = max(lum(src[id.y * u.w + id.x]), max(back, 0.0));
    dst[id.y * u.w + id.x] = vec4<f32>(g, g, g, 1.0);
}
"#;

    const WARP_WGSL: &str = r#"
struct U { w: u32, h: u32, kind: u32, filt: u32, intensity: f32, a: f32, b: f32, c: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> wmap: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> dst: array<vec4<f32>>;
fn lum(p: vec4<f32>) -> f32 { return 0.2126 * p.x + 0.7152 * p.y + 0.0722 * p.z; }
fn wl(x: i32, y: i32) -> f32 {
    let xx = u32(wrapi_samp(x, i32(u.w)));
    let yy = u32(wrapi_samp(y, i32(u.h)));
    return lum(wmap[yy * u.w + xx]);
}
fn cw(t: f32) -> vec4<f32> {
    return vec4<f32>(
        0.5 * (-t * t * t + 2.0 * t * t - t),
        0.5 * (3.0 * t * t * t - 5.0 * t * t + 2.0),
        0.5 * (-3.0 * t * t * t + 4.0 * t * t + t),
        0.5 * (t * t * t - t * t));
}
fn cubic(uu: f32, vv: f32) -> vec4<f32> {
    let x = uu * f32(u.w) - 0.5;
    let y = vv * f32(u.h) - 0.5;
    let x0 = floor(x);
    let y0 = floor(y);
    let wx = cw(x - x0);
    let wy = cw(y - y0);
    let xi = i32(x0);
    let yi = i32(y0);
    var o = vec4<f32>(0.0);
    for (var j = 0; j < 4; j = j + 1) {
        for (var i = 0; i < 4; i = i + 1) {
            o = o + texel_samp(xi + i - 1, yi + j - 1) * (wx[i] * wy[j]);
        }
    }
    let n0 = texel_samp(xi, yi);
    let n1 = texel_samp(xi + 1, yi);
    let n2 = texel_samp(xi, yi + 1);
    let n3 = texel_samp(xi + 1, yi + 1);
    let lo = min(min(n0, n1), min(n2, n3));
    let hi = max(max(n0, n1), max(n2, n3));
    return clamp(o, lo, hi);
}
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let uu = (f32(id.x) + 0.5) / f32(u.w);
    let vv = (f32(id.y) + 0.5) / f32(u.h);
    let x = i32(id.x);
    let y = i32(id.y);
    var d = vec2<f32>(0.0);
    if (u.kind == 0u) {
        d = vec2<f32>(wl(x + 1, y) - wl(x - 1, y), wl(x, y + 1) - wl(x, y - 1)) * u.intensity;
    } else if (u.kind == 1u) {
        let k = (lum(wmap[id.y * u.w + id.x]) - u.c) * u.intensity;
        d = vec2<f32>(u.a * k, u.b * k);
    } else {
        let m = wmap[id.y * u.w + id.x];
        d = vec2<f32>((m.x - 0.5) * 2.0 * u.intensity, u.a * (m.y - 0.5) * 2.0 * u.intensity);
    }
    let pu = uu + d.x;
    let pv = vv + d.y;
    var o = vec4<f32>(0.0);
    if (u.filt == 0u) {
        o = samp(pu, pv);
    } else if (u.filt == 1u) {
        let fu = pu - floor(pu);
        let fv = pv - floor(pv);
        o = texel_samp(i32(fu * f32(u.w)), i32(fv * f32(u.h)));
    } else {
        o = cubic(pu, pv);
    }
    dst[id.y * u.w + id.x] = o;
}
"#;

    const LEVELS_WGSL: &str = r#"
struct U { w: u32, h: u32, in_lo: f32, d: f32, g: f32, out_lo: f32, out_hi: f32, pad: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;
fn lv(x: f32) -> f32 {
    let v = clamp((x - u.in_lo) / u.d, 0.0, 1.0);
    var k = v;
    if (abs(u.g - 1.0) >= 1e-6) { k = pow(v, u.g); }
    return u.out_lo + (u.out_hi - u.out_lo) * k;
}
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let p = src[id.y * u.w + id.x];
    dst[id.y * u.w + id.x] = vec4<f32>(lv(p.x), lv(p.y), lv(p.z), p.w);
}
"#;

    const DIRBLUR_WGSL: &str = r#"
struct U { w: u32, h: u32, c: f32, s: f32, length: f32, a: f32, b: f32, e: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;
fn wrapi(a: i32, m: i32) -> i32 {
    // floor division through floats: signed remainders are not reliable on every driver
    let q = i32(floor(f32(a) / f32(m)));
    return a - q * m;
}
fn texel(x: i32, y: i32) -> vec4<f32> {
    return src[u32(wrapi(y, i32(u.h))) * u.w + u32(wrapi(x, i32(u.w)))];
}
fn samp(uu: f32, vv: f32) -> vec4<f32> {
    let x = uu * f32(u.w) - 0.5;
    let y = vv * f32(u.h) - 0.5;
    let x0 = floor(x);
    let y0 = floor(y);
    let fx = x - x0;
    let fy = y - y0;
    let xa = i32(x0);
    let ya = i32(y0);
    let a = texel(xa, ya);
    let b = texel(xa + 1, ya);
    let c = texel(xa, ya + 1);
    let d = texel(xa + 1, ya + 1);
    let top = a + (b - a) * fx;
    let bot = c + (d - c) * fx;
    return top + (bot - top) * fy;
}
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let uu = (f32(id.x) + 0.5) / f32(u.w);
    let vv = (f32(id.y) + 0.5) / f32(u.h);
    var acc = vec4<f32>(0.0);
    for (var i = 0; i < 24; i = i + 1) {
        let f = (f32(i) / 23.0 - 0.5) * u.length;
        acc = acc + samp(uu + u.c * f, vv + u.s * f) / 24.0;
    }
    dst[id.y * u.w + id.x] = acc;
}
"#;

    const BLEND_WGSL: &str = r#"
struct U { w: u32, h: u32, mode: u32, alpha_mode: u32, opacity: f32, has_mask: u32, a: u32, b: u32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> fg: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> bg: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> msk: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> dst: array<vec4<f32>>;
fn lum(c: vec4<f32>) -> f32 { return 0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z; }
fn bm(mode: u32, f: f32, b: f32) -> f32 {
    switch (mode) {
        case 0u: { return f; }
        case 1u: { return f + b; }
        case 2u: { return b - f; }
        case 3u: { return f * b; }
        case 4u: { return 1.0 - (1.0 - f) * (1.0 - b); }
        case 5u: {
            if (b < 0.5) { return 2.0 * f * b; }
            return 1.0 - 2.0 * (1.0 - f) * (1.0 - b);
        }
        case 6u: { return max(f, b); }
        case 7u: { return min(f, b); }
        case 8u: { return abs(f - b); }
        case 9u: { return (1.0 - 2.0 * f) * b * b + 2.0 * f * b; }
        case 10u: {
            if (f >= 0.999) { return 1.0; }
            return b / (1.0 - f);
        }
        case 11u: {
            if (f <= 0.001) { return 0.0; }
            return 1.0 - (1.0 - b) / f;
        }
        case 12u: { return f + b - 1.0; }
        case 13u: {
            if (f < 0.5) { return 2.0 * f * b; }
            return 1.0 - 2.0 * (1.0 - f) * (1.0 - b);
        }
        case 14u: { return f + b - 2.0 * f * b; }
        case 15u: {
            if (f <= 0.001) { return b; }
            return b / f;
        }
        case 16u: { return b + 2.0 * f - 1.0; }
        case 20u: { return b + (f - 0.5) * 2.0; }
        default: { return f; }
    }
}
fn set_lum(c: vec4<f32>, l: f32) -> vec3<f32> {
    let d = l - lum(c);
    return clamp(vec3<f32>(c.x + d, c.y + d, c.z + d), vec3<f32>(0.0), vec3<f32>(1.0));
}
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let i = id.y * u.w + id.x;
    let f = fg[i];
    let b = bg[i];
    var m = u.opacity;
    if (u.has_mask != 0u) { m = m * lum(msk[i]); }
    var out = b;
    if (u.alpha_mode == 0u) {
        out.w = b.w + (f.w - b.w) * m;
        m = m * f.w;
    } else if (u.alpha_mode >= 2u) {
        out.w = f.w * m + b.w * (1.0 - f.w * m);
        m = m * f.w;
    }
    if (u.mode == 17u || u.mode == 18u) {
        var r: vec3<f32>;
        if (u.mode == 17u) { r = set_lum(f, lum(b)); } else { r = set_lum(b, lum(f)); }
        let o = clamp(b.xyz + (r - b.xyz) * m, vec3<f32>(0.0), vec3<f32>(1.0));
        dst[i] = vec4<f32>(o, out.w);
        return;
    }
    let cr = clamp(b.x + (clamp(bm(u.mode, f.x, b.x), 0.0, 1.0) - b.x) * m, 0.0, 1.0);
    let cg = clamp(b.y + (clamp(bm(u.mode, f.y, b.y), 0.0, 1.0) - b.y) * m, 0.0, 1.0);
    let cb = clamp(b.z + (clamp(bm(u.mode, f.z, b.z), 0.0, 1.0) - b.z) * m, 0.0, 1.0);
    dst[i] = vec4<f32>(cr, cg, cb, out.w);
}
"#;

    const NOISE_WGSL: &str = r#"
struct U { w: u32, h: u32, px: i32, py: i32, octaves: u32, seed: u32, mode: u32, fractal: u32, gain: f32, a: f32, b: f32, c: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> grad: array<vec2<f32>>;
@group(0) @binding(2) var<storage, read_write> outp: array<vec4<f32>>;

fn hash2(x: i32, y: i32, seed: u32) -> u32 {
    var h = (bitcast<u32>(x) * 0x27d4eb2du) ^ (bitcast<u32>(y) * 0x165667b1u) ^ (seed * 0x9e3779b9u);
    h = h ^ (h >> 15u);
    h = h * 0x85ebca6bu;
    h = h ^ (h >> 13u);
    h = h * 0xc2b2ae35u;
    h = h ^ (h >> 16u);
    return h;
}
fn wrapi(a: i32, m: i32) -> i32 { return ((a % m) + m) % m; }
fn fade(t: f32) -> f32 { return t * t * t * (t * (t * 6.0 - 15.0) + 10.0); }
fn lerp(a: f32, b: f32, t: f32) -> f32 { return a + (b - a) * t; }
fn gdot(cx: i32, cy: i32, dx: f32, dy: f32, seed: u32, gx: i32, gy: i32) -> f32 {
    let c = grad[hash2(wrapi(cx, gx), wrapi(cy, gy), seed) >> 20u];
    return c.x * dx + c.y * dy;
}
fn perlin(uu: f32, vv: f32, seed: u32) -> f32 { return perlin_g(uu, vv, seed, u.px, u.py); }
fn perlin_g(uu: f32, vv: f32, seed: u32, gx: i32, gy: i32) -> f32 {
    let x = uu * f32(gx);
    let y = vv * f32(gy);
    let fxl = floor(x);
    let fyl = floor(y);
    let ix = i32(fxl);
    let iy = i32(fyl);
    let fx = x - fxl;
    let fy = y - fyl;
    let n00 = gdot(ix, iy, fx, fy, seed, gx, gy);
    let n10 = gdot(ix + 1, iy, fx - 1.0, fy, seed, gx, gy);
    let n01 = gdot(ix, iy + 1, fx, fy - 1.0, seed, gx, gy);
    let n11 = gdot(ix + 1, iy + 1, fx - 1.0, fy - 1.0, seed, gx, gy);
    let sx = fade(fx);
    let sy = fade(fy);
    let n = lerp(lerp(n00, n10, sx), lerp(n01, n11, sx), sy);
    return clamp(n * 0.70710678 + 0.5, 0.0, 1.0);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let uu = (f32(id.x) + 0.5) / f32(u.w);
    let vv = (f32(id.y) + 0.5) / f32(u.h);
    var g: f32;
    if (u.fractal == 0u) {
        g = perlin(uu, vv, u.seed);
    } else {
        var a = uu;
        var b = vv;
        var amp = 1.0;
        var sum = 0.0;
        var norm = 0.0;
        let oct = clamp(u.octaves, 1u, 12u);
        for (var o = 0u; o < oct; o = o + 1u) {
            let n = perlin(a, b, u.seed + o * 131u);
            var m = n;
            if (u.mode == 1u) { m = 1.0 - abs(2.0 * n - 1.0); }
            else if (u.mode == 2u) { m = abs(2.0 * n - 1.0); }
            sum = sum + m * amp;
            norm = norm + amp;
            amp = amp * u.gain;
            let na = 2.0 * a - b;
            let nb = a + 2.0 * b;
            a = na - floor(na);
            b = nb - floor(nb);
        }
        if (norm > 0.0) {
            let m = sum / norm;
            var k = 1.35;
            if (u.mode == 0u) { k = 1.7; }
            g = clamp(0.5 + (m - 0.5) * k, 0.0, 1.0);
        } else {
            g = 0.0;
        }
    }
    outp[id.y * u.w + id.x] = vec4<f32>(g, g, g, 1.0);
}
"#;

    /// Streak noise: tileable Perlin averaged along a direction (octaves = tap count, a/b/c = cos, sin, streak length).
    const DIRNOISE_MAIN_WGSL: &str = r#"
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let uu = (f32(id.x) + 0.5) / f32(u.w);
    let vv = (f32(id.y) + 0.5) / f32(u.h);
    let k = u.octaves;
    var a1 = 0.0;
    var a2 = 0.0;
    var wsum = 0.0;
    for (var i = 0u; i < k; i = i + 1u) {
        let t = ((f32(i) + 0.5) / f32(k)) * 2.0 - 1.0;
        let w = 1.0 - abs(t);
        let o1 = t * u.c * 0.5;
        let o2 = t * u.c * 0.25;
        a1 = a1 + w * perlin_g(uu + 1.0 + o1 * u.a, vv + 1.0 + o1 * u.b, u.seed, u.px, u.py);
        a2 = a2 + w * perlin_g(uu + 1.0 + o2 * u.a, vv + 1.0 + o2 * u.b, u.seed + 7u, u.px * 2, u.py * 2);
        wsum = wsum + w;
    }
    let m = (a1 * 0.65 + a2 * 0.35) / wsum;
    let g = clamp(0.5 + (m - 0.5) * u.gain, 0.0, 1.0);
    outp[id.y * u.w + id.x] = vec4<f32>(g, g, g, 1.0);
}
"#;

    const BOX_WGSL: &str = r#"
struct U { w: u32, h: u32, r: i32, dir: u32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;
fn wrapi(a: i32, m: i32) -> i32 {
    // the offset never exceeds half the size, so one correction is enough (and avoids signed remainders)
    if (a < 0) { return a + m; }
    if (a >= m) { return a - m; }
    return a;
}
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let x = i32(id.x);
    let y = i32(id.y);
    let w = i32(u.w);
    let h = i32(u.h);
    var acc = vec4<f32>(0.0);
    for (var k = -u.r; k <= u.r; k = k + 1) {
        if (u.dir == 0u) {
            acc = acc + src[u32(y * w + wrapi(x + k, w))];
        } else {
            acc = acc + src[u32(wrapi(y + k, h) * w + x)];
        }
    }
    dst[id.y * u.w + id.x] = acc * (1.0 / f32(2 * u.r + 1));
}
"#;

    const POINTWISE_WGSL: &str = r#"
struct U { w: u32, h: u32, op: u32, pad: u32, a: f32, b: f32, c: f32, d: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let i = id.y * u.w + id.x;
    let p = src[i];
    var o = p;
    if (u.op == 0u) {
        o = vec4<f32>(1.0 - p.x, 1.0 - p.y, 1.0 - p.z, p.w);
    } else if (u.op == 1u) {
        var v = clamp((p.xyz - vec3<f32>(0.5)) * u.a + vec3<f32>(0.5), vec3<f32>(0.0), vec3<f32>(1.0));
        if (u.b > 0.5) { v = vec3<f32>(1.0) - v; }
        o = vec4<f32>(v.x, v.y, v.z, p.w);
    } else if (u.op == 2u) {
        o = vec4<f32>(u.a, u.b, u.c, u.d);
    } else if (u.op == 3u) {
        // threshold: a level, b softness, c mode
        let l = 0.2126 * p.x + 0.7152 * p.y + 0.0722 * p.z;
        var g = 0.0;
        if (u.b > 1e-6) {
            let a0 = u.a - u.b * 0.5;
            let d = u.b;
            var t = 0.0;
            if (abs(d) < 1e-9) { t = select(0.0, 1.0, l >= u.a + u.b * 0.5); } else { t = clamp((l - a0) / d, 0.0, 1.0); }
            g = t * t * (3.0 - 2.0 * t);
            if (u.c >= 2.0) { g = 1.0 - g; }
        } else {
            var on = false;
            if (u.c < 0.5) { on = l > u.a; } else if (u.c < 1.5) { on = l >= u.a; } else if (u.c < 2.5) { on = l < u.a; } else { on = l <= u.a; }
            g = select(0.0, 1.0, on);
        }
        o = vec4<f32>(g, g, g, 1.0);
    } else if (u.op == 4u) {
        // clamp: a low, b high, c apply to alpha
        let lo = min(u.a, u.b);
        let hi = max(u.a, u.b);
        let c = clamp(p.xyz, vec3<f32>(lo), vec3<f32>(hi));
        o = vec4<f32>(c.x, c.y, c.z, select(p.w, clamp(p.w, lo, hi), u.c > 0.5));
    } else if (u.op == 5u) {
        // pow: a exponent
        var c = max(p.xyz, vec3<f32>(0.0));
        if (u.a == 0.0) { c = vec3<f32>(1.0); } else { c = pow(c, vec3<f32>(u.a)); }
        o = vec4<f32>(c.x, c.y, c.z, p.w);
    } else if (u.op == 6u) {
        let l = 0.2126 * p.x + 0.7152 * p.y + 0.0722 * p.z;
        o = vec4<f32>(l, l, l, 1.0);
    } else if (u.op == 7u) {
        // quantize: a steps - 1 (rounding half away from zero like the host)
        let q = p.xyz * u.a;
        let r = sign(q) * floor(abs(q) + 0.5) / u.a;
        o = vec4<f32>(r.x, r.y, r.z, p.w);
    } else if (u.op == 8u) {
        o = vec4<f32>(p.x * u.a, p.y * u.b, p.z * u.c, p.w);
    } else if (u.op == 9u) {
        // histogram scan: a low edge, b width, c invert
        let l = 0.2126 * p.x + 0.7152 * p.y + 0.0722 * p.z;
        var g = clamp((l - u.a) / u.b, 0.0, 1.0);
        if (u.c > 0.5) { g = 1.0 - g; }
        o = vec4<f32>(g, g, g, 1.0);
    } else if (u.op == 10u) {
        // one channel as grey: a channel index
        var v = p.x;
        if (u.a > 2.5) { v = p.w; } else if (u.a > 1.5) { v = p.z; } else if (u.a > 0.5) { v = p.y; }
        o = vec4<f32>(v, v, v, 1.0);
    } else {
        // histogram shift: a offset
        let c = clamp(p.xyz + vec3<f32>(u.a), vec3<f32>(0.0), vec3<f32>(1.0));
        o = vec4<f32>(c.x, c.y, c.z, p.w);
    }
    dst[i] = o;
}
"#;

    const THUMB_WGSL: &str = r#"
struct U { w: u32, h: u32, n: u32, pad: u32, tw: f32, th: f32, ox: f32, oy: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> outp: array<vec4<f32>>;
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.n || id.y >= u.n) { return; }
    let fx = f32(id.x) + 0.5 - u.ox;
    let fy = f32(id.y) + 0.5 - u.oy;
    var o = vec4<f32>(0.0);
    if (!(fx < 0.0 || fy < 0.0 || fx >= u.tw || fy >= u.th)) {
        let p = samp(fx / u.tw, fy / u.th);
        o = vec4<f32>(p.x, p.y, p.z, 1.0);
    }
    outp[id.y * u.n + id.x] = o;
}
"#;

    const NORMAL_WGSL: &str = r#"
struct U { w: u32, h: u32, k: f32, directx: u32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;
fn lum(p: vec4<f32>) -> f32 { return 0.2126 * p.x + 0.7152 * p.y + 0.0722 * p.z; }
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let x = id.x;
    let y = id.y;
    var xm = x - 1u; if (x == 0u) { xm = u.w - 1u; }
    var xp = x + 1u; if (x + 1u == u.w) { xp = 0u; }
    var ym = y - 1u; if (y == 0u) { ym = u.h - 1u; }
    var yp = y + 1u; if (y + 1u == u.h) { yp = 0u; }
    let g = array<f32, 9>(
        lum(src[ym * u.w + xm]), lum(src[ym * u.w + x]), lum(src[ym * u.w + xp]),
        lum(src[y * u.w + xm]),  lum(src[y * u.w + x]),  lum(src[y * u.w + xp]),
        lum(src[yp * u.w + xm]), lum(src[yp * u.w + x]), lum(src[yp * u.w + xp]));
    // g indices: row 0 = ym, row 1 = y, row 2 = yp; column 0 = xm, 1 = x, 2 = xp
    let dx = (3.0 * (g[2] - g[0]) + 10.0 * (g[5] - g[3]) + 3.0 * (g[8] - g[6])) / 32.0 * u.k;
    var dy = (3.0 * (g[6] - g[0]) + 10.0 * (g[7] - g[1]) + 3.0 * (g[8] - g[2])) / 32.0 * u.k;
    if (u.directx != 0u) { dy = -dy; }
    let n = vec3<f32>(-dx, dy, 1.0);
    let l = length(n);
    let o = n / l * 0.5 + vec3<f32>(0.5);
    dst[id.y * u.w + id.x] = vec4<f32>(o.x, o.y, o.z, 1.0);
}
"#;

    // ---- viewer hooks ----
    impl Gpu {
        /// The device and queue the graph kernels run on (the 3D view renders on the same ones).
        pub fn viewer_parts(&self) -> (&wgpu::Device, &wgpu::Queue) {
            (&self.dev, &self.queue)
        }
        pub fn viewer_limits(&self) -> wgpu::Limits {
            self.dev.limits()
        }
    }
}

// ---- viewer hooks ----
#[cfg(feature = "gpu")]
impl GTex {
    /// The storage buffer that holds the picture (RGBA f32 per texel, row by row).
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.0.buf
    }
    /// Identity of the picture in device memory: the same value while the same buffer is held (clones share it).
    pub fn id(&self) -> usize {
        std::sync::Arc::as_ptr(&self.0) as usize
    }
}

/// The device and queue of the compute engine (`None` without an adapter, or when the engine is switched off).
#[cfg(feature = "gpu")]
pub fn viewer_parts() -> Option<(&'static wgpu::Device, &'static wgpu::Queue)> {
    if !ENABLED.load(Ordering::Relaxed) || BROKEN.load(Ordering::Relaxed) {
        return None;
    }
    imp::global().map(|g| g.viewer_parts())
}

/// Limits of the compute engine's device.
#[cfg(feature = "gpu")]
pub fn viewer_limits() -> Option<wgpu::Limits> {
    imp::global().map(|g| g.viewer_limits())
}
