// SPDX-License-Identifier: MIT OR Apache-2.0
//! Graph nodes that run on device-resident pictures.
//!
//! `eval_res` asks `run` before it computes a node on the CPU. When the node has a GPU kernel, its settings are inside
//! what the kernel supports and the work is worth sending to the device, the kernel reads the inputs where they are
//! (uploading a CPU-only input once) and the result stays in GPU memory: the next GPU node reads it from there and only
//! a CPU node, a thumbnail or the 3D view ever brings the pixels back. The settings are read exactly as the CPU code in
//! `graph.rs` and `nodes2/3/5.rs` reads them, and `tests/gpu_graph.rs` compares every node here with that CPU code.
//!
//! `None` always means "run the CPU code" (nothing has been changed), so a graph gives the same pictures with or
//! without a usable adapter, apart from float rounding.

use super::{Ctx, Lazy, Node, Rc};
use crate::gpu::{self, opa, opb, opc, GTex, Kern, Point, WarpKind};
use crate::noise;
use std::collections::BTreeMap;
use std::f32::consts::TAU;

type Ins = BTreeMap<String, Rc<Lazy>>;

/// Kinds this module can run (the quick test before anything else is looked at).
fn supported(kind: &str) -> bool {
    matches!(
        kind,
        "perlin"
            | "fbm"
            | "clouds"
            | "directional_noise"
            | "blur"
            | "directional_blur"
            | "anisotropic_blur"
            | "levels"
            | "invert"
            | "warp"
            | "directional_warp"
            | "vector_warp"
            | "slope_blur"
            | "blend"
            | "normal"
            | "radial_blur"
            | "bent_normal"
            | "bilateral_blur"
            | "ao"
            | "preset_noise"
            | "threshold"
            | "clamp"
            | "pow"
            | "to_gray"
            | "quantize"
            | "tint"
            | "histogram_scan"
            | "channel"
            | "histogram_shift"
            | "x_rt_shadows"
            | "x_shadows"
            | "hsl"
            | "adjust"
            | "contrast_luminosity"
            | "srgb_to_linear"
            | "linear_to_srgb"
            | "premultiply"
            | "grayscale_conversion"
            | "channel_mixer"
            | "color_to_mask"
            | "replace_color"
            | "chrominance_extract"
            | "normal_invert"
            | "normal_normalize"
            | "histogram_range"
            | "alpha_merge"
            | "merge"
            | "channel_shuffle"
            | "normal_vector_rotation"
            | "normal_combine"
            | "normal_blend"
            | "alpha_split"
            | "rgba_split"
            | "auto_levels"
            | "height_combine"
            | "sharpen"
            | "highpass"
            | "curvature"
            | "convex_concave"
            | "glow"
            | "colorize"
            | "switch"
            | "white_noise"
            | "gaussian_noise"
            | "plasma"
            | "stripes"
            | "weave"
            | "shape"
            | "star"
            | "gradient_axial"
            | "gradient_circular"
            | "gradient"
            | "circle"
            | "rect"
            | "polygon"
            | "checker"
            | "bricks"
            | "gradient_2pt"
            | "voronoi"
            | "voronoi_fractal"
            | "cells"
            | "bnw_spots"
            | "crystal"
            | "waveform"
            | "gaussian_spots"
            | "fractal_sum"
            | "triangle_grid"
            | "weave_generator"
            | "starburst"
            | "curve"
            | "distance"
            | "bevel"
            | "shape_stroke"
            | "shape_glow"
            | "blur_hq"
            | "tile_sampler"
            | "tile_generator"
            | "splatter"
            | "splatter_circular"
            | "metal_rough_to_spec_gloss"
            | "spec_gloss_to_metal_rough"
            | "material_blend"
            | "leak_streaks"
            | "settled_dust"
            | "make_it_tile_mirror"
            | "height_blend"
            | "make_it_tile_photo"
            | "edge_wear"
            | "dirt"
            | "dust"
            | "emboss"
            | "edge_detect"
            | "sobel"
            | "swirl"
            | "transform"
            | "mirror"
            | "mosaic"
            | "symmetry"
            | "skew"
            | "non_uniform_directional_warp"
            | "multi_directional_warp"
            | "non_uniform_blur"
            | "normal_facing"
            | "height_to_normal_world"
            | "normal_sobel"
            | "normal_transform"
            | "height_normal_blend"
            | "min_max"
            | "frequency_merge"
            | "frequency_split"
            | "multi_switch"
            | "polar_convert"
            | "symmetry_slice"
            | "quad_transform"
            | "trapezoid_transform"
            | "clone_patch"
            | "gradient_reflected"
            | "shape_drop_shadow"
            | "sdf_mask"
            | "sdf_op"
            | "sdf_form"
            | "sdf_shape"
            | "arc_pavement"
            | "herringbone"
            | "anisotropic_noise"
            | "microscope_noise"
            | "moisture_noise"
            | "gradient_3d"
            | "volume_mask"
            | "fbm_3d"
            | "worley_3d"
            | "wire_mesh"
            | "paving"
            | "perlin_3d"
            | "liquid_noise"
            | "caustics"
            | "summed_area_table"
            | "water_filter"
            | "season_filter"
            | "snow_filter"
            | "mesh_weathering"
            | "safe_transform"
            | "uber_emboss"
            | "cross_section"
            | "fxaa"
            | "halftone"
            | "output"
            | "dot"
            | "color"
            | "noise_upscale"
            | "auto_crop"
            | "histogram_equalize"
            | "lookup_table"
            | "morphology"
            | "flood_fill_gradient"
            | "flood_fill_color"
            | "flood_fill_random"
            | "flood_fill_to_bbox_size"
            | "flood_fill_to_position"
            | "flood_fill_to_index"
            | "flood_fill"
    )
}

/// Runs node `n` (output `out_idx` for multi-output kinds) on the device, or returns `None` for the CPU code.
pub(super) fn run(n: &Node, ins: &Ins, sz: (usize, usize), ctx: &Ctx, out_idx: usize) -> Option<GTex> {
    let kind = n.kind.as_str();
    if !supported(kind) || !gpu::can_hold(sz.0, sz.1) {
        return None;
    }
    // an input of another size is resampled on the device first (as the CPU path does)
    let fit = |l: &Lazy| -> Option<GTex> { gpu::resize_g(&l.dev()?, sz.0, sz.1) };
    // big enough to be worth it on its own, or the work is already on the device (then the upload is free)
    if !(gpu::worth_it(sz.0, sz.1) || ins.values().any(|l| l.on_device())) {
        return None;
    }
    let f = |k: &str, d: f32| n.nums.get(k).copied().unwrap_or(d);
    // the CPU code rolls a result by the fractional tile offset afterwards; the kernels do not
    let (tx, ty) = (f("tile_x", 0.0), f("tile_y", 0.0));
    if tx - tx.floor() > 0.0 || ty - ty.floor() > 0.0 {
        return None;
    }
    let seed = (f("seed", 0.0).max(0.0) as u32).wrapping_add(ctx.so);
    let seed = if tx.floor() != 0.0 || ty.floor() != 0.0 { noise::tile_seed(seed, tx.floor() as i32, ty.floor() as i32) } else { seed };
    let (s, hs) = sz;
    let rs = s as f32 / 512.0;
    let dev = |slot: &str| -> Option<GTex> { fit(ins.get(slot)?) };
    // an optional input: `None` = something went wrong, `Some(None)` = not connected
    let odev = |slot: &str| -> Option<Option<GTex>> {
        match ins.get(slot) {
            Some(l) => Some(Some(fit(l)?)),
            None => Some(None),
        }
    };
    let call = |op: u32, p: &[f32], i: &[&GTex]| -> Option<GTex> { gpu::kernel_g(Kern::OpsA, op, 0, p, &[], i, sz) };
    // generators: parameters 0..24 are the node's own, then Disorder, Contrast (minus one) and Invert
    let gen_c = |op: u32, p: &[f32], con: f32| -> Option<GTex> {
        let mut v = [0f32; 32];
        v[..p.len()].copy_from_slice(p);
        v[24] = f("disorder", 0.0).max(0.0) * f("disorder_speed", 1.0).max(0.0) * 0.3;
        v[25] = f("anisotropy", 0.0).clamp(0.0, 1.0);
        let (sn, cs) = (f("anisotropy_angle", 0.0) * TAU).sin_cos();
        v[26] = cs;
        v[27] = sn;
        v[29] = con - 1.0;
        v[30] = (f("invert", 0.0) > 0.5) as u32 as f32;
        gpu::kernel_g(Kern::Gen, op, seed, &v, &[], &[], sz)
    };
    let gen = |op: u32, p: &[f32]| -> Option<GTex> { gen_c(op, p, f("contrast", 1.0)) };
    // the second generator kernel (no Disorder: those nodes have none)
    let gen2 = |op: u32, p: &[f32]| -> Option<GTex> {
        let mut v = [0f32; 32];
        v[..p.len()].copy_from_slice(p);
        v[29] = f("contrast", 1.0).clamp(0.0, 16.0) - 1.0;
        v[30] = (f("invert", 0.0) > 0.5) as u32 as f32;
        gpu::kernel_g(Kern::Gen2, op, seed, &v, &[], &[], sz)
    };
    let callb = |op: u32, p: &[f32], i: &[&GTex]| -> Option<GTex> { gpu::kernel_g(Kern::OpsB, op, 0, p, &[], i, sz) };
    // the periodic seam blend that tiling transforms apply to a result which is not periodic by itself
    let seam = |t: GTex| -> Option<GTex> { callb(opb::SEAM, &[], &[&t]) };
    let callc = |op: u32, p: &[f32], i: &[&GTex]| -> Option<GTex> { gpu::kernel_g(Kern::OpsC, op, 0, p, &[], i, sz) };
    // the same with a seed (the kernel's `n`)
    let callc_n = |op: u32, n: u32, p: &[f32], i: &[&GTex]| -> Option<GTex> { gpu::kernel_g(Kern::OpsC, op, n, p, &[], i, sz) };
    let calld = |op: u32, p: &[f32], i: &[&GTex]| -> Option<GTex> { gpu::kernel_g(Kern::OpsD, op, 0, p, &[], i, sz) };
    let callc_d = |op: u32, n: u32, p: &[f32], i: &[&GTex]| -> Option<GTex> { gpu::kernel_g(Kern::OpsD, op, n, p, &[], i, sz) };
    // noise periods of the effect nodes (same ratio as the graph)
    let periods = |scale: f32, w: usize, h: usize| -> (i32, i32) {
        let px = scale.round().clamp(1.0, 256.0) as i32;
        (px, ((px as f32 * h as f32 / w as f32).round() as i32).max(1))
    };
    // signed distance field of `lum > thr` by jump flooding: everything stays on the device
    let sdf = |t: &GTex, thr: f32| -> Option<GTex> {
        let mut st = gpu::kernel_g(Kern::Jfa, 0, 0, &[thr], &[], &[t], sz)?;
        let mut k = (sz.0.max(sz.1)).next_power_of_two() / 2;
        while k >= 1 {
            st = gpu::kernel_g(Kern::Jfa, 1, 0, &[k as f32], &[], &[&st], sz)?;
            k /= 2;
        }
        for k in [2.0f32, 1.0] {
            st = gpu::kernel_g(Kern::Jfa, 1, 0, &[k], &[], &[&st], sz)?;
        }
        Some(st)
    };
    let per256 = |k: &str, d: f32| f(k, d).round().clamp(1.0, 256.0);
    let px1 = 1.2 / s as f32;
    // generators: Contrast and Invert settings, applied after the noise
    let fin = |t: GTex| -> Option<GTex> {
        let (con, inv) = (f("contrast", 1.0), f("invert", 0.0) > 0.5);
        if (con - 1.0).abs() > 1e-4 || inv {
            gpu::contrast_invert_g(&t, con, inv)
        } else {
            Some(t)
        }
    };
    // Disorder warps the sample positions per pixel: the kernels do not
    let no_warp = f("disorder", 0.0) * f("disorder_speed", 1.0) <= 0.0;
    let per = |k: &str, d: f32| f(k, d).round().clamp(1.0, 4096.0) as i32;
    let ar = if f("square_cells", 0.0) > 0.5 { hs as f32 / s as f32 } else { 1.0 };
    let per_y = |k: &str, d: f32| ((per(k, d) as f32 * ar).round().max(1.0)) as i32;
    match kind {
        "perlin" => {
            let (px, py) = (per("scale_x", f("scale", 4.0)), per_y("scale_y", f("scale", 4.0)));
            if !no_warp {
                return gen(30, &[px as f32, py as f32]);
            }
            fin(gpu::noise_g(s, hs, false, px, py, 1, 0.5, seed, 0)?)
        }
        "fbm" | "clouds" => {
            let cl = kind == "clouds";
            let (px, py) = (per("scale_x", f("scale", if cl { 3.0 } else { 4.0 })), per_y("scale_y", f("scale", if cl { 3.0 } else { 4.0 })));
            let (oct, gain, mode) = (f("octaves", if cl { 6.0 } else { 5.0 }).max(1.0) as u32, f("gain", if cl { 0.6 } else { 0.5 }), f("mode", 0.0).max(0.0) as u32);
            if !no_warp {
                return gen(31, &[px as f32, py as f32, oct as f32, gain, mode as f32]);
            }
            fin(gpu::noise_g(s, hs, true, px, py, oct, gain, seed, mode)?)
        }
        "directional_noise" => {
            if !no_warp {
                return None;
            }
            let (sc, st, ang) = (f("scale", 6.0), f("stretch", 8.0).max(1.0), f("angle", 0.0));
            let p1 = sc.round().clamp(1.0, 256.0) as i32;
            let (sa, ca) = (ang * TAU).sin_cos();
            let len1 = (st / p1 as f32).min(1.0);
            let k = ((st * 2.5).ceil() as usize).clamp(8, 96).min(((4.0 * len1 * p1 as f32).ceil() as usize).max(8)) | 1;
            let gain = 1.6 * st.min(p1 as f32).powf(0.55);
            fin(gpu::directional_noise_g(s, hs, p1, k as u32, ca, sa, len1, gain, seed)?)
        }
        "blur" => gpu::blur_g(&dev("in")?, f("radius", 4.0) * rs),
        "directional_blur" => gpu::directional_blur_g(&dev("in")?, f("angle", 0.0), f("length", 0.05)),
        "anisotropic_blur" => {
            let (inten, an) = (f("intensity", 8.0).clamp(0.0, 64.0), f("anisotropy", 0.8).clamp(0.0, 1.0));
            let sg = inten * (1.0 - an) * rs;
            let a = gpu::blur_g(&dev("in")?, sg)?;
            gpu::directional_blur_g(&a, f("angle", 0.0), inten * an * 4.0 / 512.0)
        }
        "levels" => {
            if f("auto", 0.0) > 0.5 {
                // the range comes from the picture itself: found on the device, never read back
                let t = dev("in")?;
                let st = gpu::minmax_g(&t)?;
                return gpu::kernel_g(Kern::OpsA, opa::LEVELS_AUTO, 0, &[f("gamma", 1.0), f("out_low", 0.0), f("out_high", 1.0)], &[], &[&t, &t, &t, &st], sz);
            }
            gpu::levels_g(&dev("in")?, f("low", 0.0), f("high", 1.0), f("gamma", 1.0), f("out_low", 0.0), f("out_high", 1.0))
        }
        "invert" => {
            let t = dev("in")?;
            if f("invert", 1.0) > 0.5 {
                gpu::invert_g(&t)
            } else {
                Some(t)
            }
        }
        "warp" => {
            let (t, w) = (dev("in")?, dev("warp")?);
            // the kernel measures the slope per pixel: scaled to 1/256 of the image like the CPU path
            gpu::warp_g(&t, &w, WarpKind::Slope, f("intensity", 0.05) * w.w() as f32 / 256.0, f("filtering", 2.0).max(0.0) as u32)
        }
        "directional_warp" => {
            let (t, w) = (dev("in")?, dev("warp")?);
            let (s_, c_) = (f("angle", 0.0) * TAU).sin_cos();
            gpu::warp_g(&t, &w, WarpKind::Directional { c: c_, s: s_, offset: f("offset", 0.5) }, f("intensity", 0.1), f("filtering", 2.0).max(0.0) as u32)
        }
        "vector_warp" => {
            let (t, vm) = (dev("in")?, dev("vector")?);
            let gs = if f("directx", 0.0) > 0.5 { -1.0 } else { 1.0 };
            gpu::warp_g(&t, &vm, WarpKind::Vector { gs }, f("intensity", 0.05), 2)
        }
        "slope_blur" => gpu::slope_blur_g(&dev("in")?, &dev("slope")?, f("samples", 16.0) as u32, f("intensity", 0.1), f("mode", 0.0) as u32),
        "blend" => {
            let crop = [f("crop_x0", 0.0), f("crop_y0", 0.0), f("crop_x1", 1.0), f("crop_y1", 1.0)];
            if crop != [0.0, 0.0, 1.0, 1.0] {
                return None;
            }
            let (fg, bg) = (dev("fg")?, dev("bg")?);
            let mask = match ins.get("mask") {
                Some(m) => Some(fit(m)?),
                None => None,
            };
            gpu::blend_g(&fg, &bg, mask.as_ref(), f("mode", 0.0).max(0.0) as u32, f("opacity", 1.0), f("alpha_mode", 1.0).max(0.0) as u32)
        }
        "normal" => {
            if f("alpha_from_input", 0.0) > 0.5 {
                return None;
            }
            gpu::normal_g(&dev("in")?, f("strength", 2.0), f("directx", 0.0) > 0.5)
        }
        "radial_blur" => {
            let t = dev("in")?;
            let (cx, cy) = (0.5 + f("x", 0.0), 0.5 + f("y", 0.0));
            if f("mode", 0.0) > 0.5 {
                let (amount, rad) = (f("amount", 0.1).clamp(0.0, 1.0), f("radius", 0.5).clamp(0.01, 0.5));
                gpu::radial_blur_g(&t, 1, 24, cx, cy, amount, rad)
            } else {
                let taps = f("samples", 64.0).round().clamp(1.0, 128.0) as u32;
                gpu::radial_blur_g(&t, 0, taps, cx, cy, f("angle", 0.125).clamp(0.0, 0.5) * TAU, 0.0)
            }
        }
        "bent_normal" => {
            let dirs = f("directions", 16.0).round().clamp(4.0, 32.0) as u32;
            let steps = f("steps", 8.0).round().clamp(2.0, 16.0) as u32;
            let rad = f("radius", 16.0).clamp(1.0, 256.0) / 512.0;
            let relief = f("relief", 0.1).clamp(0.0, 10.0);
            gpu::bent_normal_g(&dev("in")?, dirs, steps, rad, relief, f("directx", 0.0) > 0.5, out_idx == 1)
        }
        "ao" => {
            if f("mode", 0.0) < 0.5 {
                // screen-space: the height against its own blur
                let g = gpu::point_g(&dev("in")?, Point::ToGray)?;
                let b = gpu::blur_g(&g, f("radius", 6.0) * rs)?;
                return call(opa::AGAINST_BLUR, &[1.0, f("intensity", 2.0)], &[&g, &b]);
            }
            let scale = if f("use_world_units", 0.0) > 0.5 { f("height_depth_cm", 30.0) / f("surface_size", 300.0).max(1.0) } else { f("height_depth", 0.1) };
            let dirs = [4u32, 8, 16][(f("quality", 1.0).round() as usize).min(2)];
            gpu::hbao_g(&dev("in")?, f("hbao_radius", 1.0).clamp(0.0, 1.0) * 0.25, scale, dirs, f("hbao_intensity", 1.0))
        }
        "threshold" => gpu::point_g(&dev("in")?, Point::Threshold(f("threshold", 0.5), f("softness", 0.0), f("mode", 0.0).max(0.0) as u32)),
        "clamp" => gpu::point_g(&dev("in")?, Point::Clamp(f("low", 0.0), f("high", 1.0), f("apply_alpha", 0.0) > 0.5)),
        "pow" => gpu::point_g(&dev("in")?, Point::Pow(f("exponent", 2.0).clamp(0.0, 32.0))),
        "to_gray" => gpu::point_g(&dev("in")?, Point::ToGray),
        "quantize" => gpu::point_g(&dev("in")?, Point::Quantize(f("steps", 4.0).max(2.0) - 1.0)),
        "tint" => gpu::point_g(&dev("in")?, Point::Tint(f("r", 1.0), f("g", 1.0), f("b", 1.0))),
        "channel" => gpu::point_g(&dev("in")?, Point::Channel((f("ch", 0.0).max(0.0) as u32).min(3))),
        "histogram_shift" => gpu::point_g(&dev("in")?, Point::HistogramShift((f("position", 0.5) - 0.5) * 2.0)),
        "histogram_scan" => {
            // with a position or contrast map the ramp changes across the picture: the CPU code does that
            if ins.contains_key("position_map") || ins.contains_key("contrast_map") {
                return None;
            }
            let width = (1.0 - f("contrast", 0.5).clamp(0.0, 1.0)).max(1e-4);
            gpu::point_g(&dev("in")?, Point::HistogramScan(f("position", 0.5) - width * 0.5, width.max(1e-4), f("invert", 0.0) > 0.5))
        }
        "preset_noise" => {
            // the plain look: octaves, tone curve, balance. The extra layers (brush, specks, streaks, scratches, cracks,
            // grain, sharpen) and Disorder run on the CPU.
            let extras = ["scratches", "specks", "streaks", "grain", "sharpen"].iter().any(|k| f(k, 0.0) != 0.0) || f("cracks", 0.0) > 1e-3 || f("brush", 0.0) > 1e-3;
            if !no_warp || extras || matches!(f("preset", 0.0) as i32, 80 | 81) {
                return None;
            }
            let turns = (f("angle", 0.0) * 4.0).round() as i32;
            let st = crate::noise_styles::preset_style(f("preset", 0.0).max(0.0) as usize, f("roughness", 1.0).clamp(0.0, 4.0), turns);
            let scale = f("scale", 1.0).round().clamp(1.0, 256.0);
            let oct = crate::noise_styles::octaves(&st, s, hs, scale);
            if oct.is_empty() {
                return None;
            }
            let ramp = st.ramp * 0.35 * oct.iter().map(|o| o.2).sum::<f32>();
            let basis = match st.basis {
                crate::noise_styles::Basis::Value => 0,
                crate::noise_styles::Basis::Gradient => 1,
                crate::noise_styles::Basis::Spots => 2,
                crate::noise_styles::Basis::Ridged => 3,
            };
            let table: Vec<(i32, i32, f32, u32)> = oct.iter().enumerate().map(|(k, &(p, q, wt))| (p, q, wt, seed.wrapping_add(k as u32 * 1013))).collect();
            let (wp, ar) = (f("warp", 0.0).clamp(0.0, 4.0), f("angle_random", 0.0).clamp(0.0, 1.0));
            let raw = gpu::styled_raw_g(s, hs, basis, &table, ramp, wp, ar, seed)?;
            let bal = f("balance", 0.5) - 0.5;
            let toned = gpu::tone_g(&raw, &st.tone, if bal.abs() > 1e-4 { Some(bal) } else { None })?;
            // the CPU code clamps Contrast the same way before applying it
            let (con, inv) = (f("contrast", 1.0).clamp(0.0, 16.0), f("invert", 0.0) > 0.5);
            if (con - 1.0).abs() > 1e-4 || inv {
                gpu::contrast_invert_g(&toned, con, inv)
            } else {
                Some(toned)
            }
        }
        "x_rt_shadows" | "x_shadows" => {
            let sh = crate::nodes10::with_params(kind, n, s, hs, seed, |cx| crate::nodes10_img::shadow_params(cx, s, hs))?;
            gpu::shadows_g(&dev(if kind == "x_rt_shadows" { "input" } else { "Input" })?, &sh)
        }
        "hsl" => call(opa::HSL, &[f("hue", 0.0), f("sat", 1.0), f("val", 1.0)], &[&dev("in")?]),
        "adjust" => call(opa::ADJUST, &[f("brightness", 0.0), f("contrast", 1.0)], &[&dev("in")?]),
        "contrast_luminosity" => call(opa::CONTRAST_LUM, &[f("contrast", 1.0).clamp(-16.0, 16.0), f("luminosity", 0.0).clamp(-8.0, 8.0)], &[&dev("in")?]),
        "srgb_to_linear" => call(opa::SRGB_TO_LIN, &[], &[&dev("in")?]),
        "linear_to_srgb" => call(opa::LIN_TO_SRGB, &[], &[&dev("in")?]),
        "premultiply" => call(opa::PREMUL, &[f("mode", 0.0)], &[&dev("in")?]),
        "grayscale_conversion" => call(opa::GRAYSCALE, &[f("r", 0.3), f("g", 0.59), f("b", 0.11)], &[&dev("in")?]),
        "channel_mixer" => call(
            opa::MIXER,
            &[f("rr", 1.0), f("rg", 0.0), f("rb", 0.0), f("gr", 0.0), f("gg", 1.0), f("gb", 0.0), f("br", 0.0), f("bg", 0.0), f("bb", 1.0), f("mono", 0.0)],
            &[&dev("in")?],
        ),
        "color_to_mask" => call(opa::COLOR_TO_MASK, &[f("r", 1.0), f("g", 0.0), f("b", 0.0), f("range", 0.2).max(0.0), f("softness", 0.1).max(0.0)], &[&dev("in")?]),
        "replace_color" => call(
            opa::REPLACE,
            &[f("src_r", 1.0), f("src_g", 0.0), f("src_b", 0.0), f("dst_r", 0.0), f("dst_g", 0.0), f("dst_b", 1.0), f("range", 0.25).max(0.0), f("softness", 0.15).max(0.0)],
            &[&dev("in")?],
        ),
        "chrominance_extract" => call(opa::CHROMA, &[], &[&dev("in")?]),
        "normal_invert" => call(opa::NORMAL_INVERT, &[f("invert_x", 0.0), f("invert_y", 1.0), f("invert_z", 0.0), f("invert_a", 0.0)], &[&dev("in")?]),
        "normal_normalize" => call(opa::NORMAL_NORM, &[], &[&dev("in")?]),
        "histogram_range" => call(opa::HIST_RANGE, &[f("position", 0.5), f("range", 0.2).max(1e-4), f("contrast", 0.5), f("invert", 0.0)], &[&dev("in")?]),
        "alpha_merge" => {
            let rgb = dev("rgb")?;
            match odev("alpha")? {
                Some(a) => call(opa::ALPHA_MERGE, &[1.0], &[&rgb, &a]),
                None => Some(rgb),
            }
        }
        "merge" => {
            let d: [Option<GTex>; 4] = [odev("r")?, odev("g")?, odev("b")?, odev("a")?];
            let Some(any) = d.iter().flatten().next().cloned() else {
                return gpu::solid_g(s, hs, [0.0, 0.0, 0.0, 1.0]);
            };
            let pick = |i: usize| d[i].clone().unwrap_or_else(|| any.clone());
            let flags = [d[0].is_some() as u32 as f32, d[1].is_some() as u32 as f32, d[2].is_some() as u32 as f32, d[3].is_some() as u32 as f32];
            call(opa::MERGE, &flags, &[&pick(0), &pick(1), &pick(2), &pick(3)])
        }
        "channel_shuffle" => {
            let a = dev("in")?;
            let b = odev("in2")?.unwrap_or_else(|| a.clone());
            let pick = |k: &str, d: f32| f(k, d).round().clamp(0.0, 9.0);
            let inv = |k: &str| if f(k, 0.0) > 0.5 { 1.0 } else { 0.0 };
            call(opa::SHUFFLE, &[pick("src_r", 0.0), pick("src_g", 1.0), pick("src_b", 2.0), pick("src_a", 3.0), inv("inv_r"), inv("inv_g"), inv("inv_b"), inv("inv_a")], &[&a, &b])
        }
        "normal_vector_rotation" => {
            let t = dev("in")?;
            let rot = odev("rotation")?;
            let gs = if f("directx", 0.0) > 0.5 { -1.0 } else { 1.0 };
            let r = rot.clone().unwrap_or_else(|| t.clone());
            call(opa::NORMAL_ROT, &[f("angle", 0.0) * TAU, gs, rot.is_some() as u32 as f32], &[&t, &r])
        }
        "normal_combine" => call(opa::NORMAL_COMBINE, &[f("technique", 2.0).round()], &[&dev("base")?, &dev("detail")?]),
        "normal_blend" => {
            let (a, b) = (dev("a")?, dev("b")?);
            let m = odev("mask")?;
            let mm = m.clone().unwrap_or_else(|| a.clone());
            call(opa::NORMAL_BLEND, &[f("opacity", 1.0), m.is_some() as u32 as f32], &[&a, &b, &mm])
        }
        "alpha_split" => {
            let t = dev("in")?;
            if out_idx >= 1 {
                gpu::point_g(&t, Point::Channel(3))
            } else {
                call(opa::ALPHA_SPLIT, &[], &[&t])
            }
        }
        "rgba_split" => gpu::point_g(&dev("in")?, Point::Channel((out_idx as u32).min(3))),
        "auto_levels" => {
            let t = dev("in")?;
            let st = gpu::minmax_g(&t)?;
            call(opa::AUTO_LEVELS, &[f("per_channel", 0.0)], &[&t, &t, &t, &st])
        }
        "height_combine" => {
            let (a, b) = (dev("a")?, dev("b")?);
            let m = odev("mask")?;
            let mm = m.clone().unwrap_or_else(|| a.clone());
            call(opa::HEIGHT_COMBINE, &[f("mode", 0.0).round(), f("amount", 1.0).clamp(0.0, 8.0), m.is_some() as u32 as f32], &[&a, &b, &mm])
        }
        "sharpen" => {
            let t = dev("in")?;
            let b = gpu::blur_g(&t, f("radius", 2.0) * rs)?;
            call(opa::AGAINST_BLUR, &[3.0, f("amount", 1.0)], &[&t, &b])
        }
        "highpass" => {
            let t = dev("in")?;
            let b = gpu::blur_g(&t, f("radius", 6.0) * rs)?;
            call(opa::AGAINST_BLUR, &[2.0], &[&t, &b])
        }
        "curvature" => {
            // the blur-based mode; the normal-based one runs on the CPU
            if f("mode", 0.0) <= 0.5 {
                return None;
            }
            let g = gpu::point_g(&dev("in")?, Point::ToGray)?;
            let b = gpu::blur_g(&g, f("radius", 3.0) * rs)?;
            call(opa::AGAINST_BLUR, &[0.0, f("intensity", 4.0)], &[&g, &b])
        }
        "convex_concave" => {
            let t = dev("in")?;
            let b = gpu::blur_g(&t, f("radius", 4.0).clamp(0.0, 128.0) * rs)?;
            call(opa::AGAINST_BLUR, &[4.0, f("gain", 4.0).clamp(0.0, 64.0), (out_idx != 0) as u32 as f32], &[&t, &b])
        }
        "glow" => {
            let t = dev("in")?;
            let b = gpu::blur_g(&t, f("width", 8.0).clamp(0.0, 128.0) * rs)?;
            call(opa::AGAINST_BLUR, &[5.0, f("clear", 0.0).clamp(0.0, 0.99), f("glow_r", 1.0), f("glow_g", 1.0), f("glow_b", 1.0), f("intensity", 1.0).clamp(0.0, 8.0)], &[&t, &b])
        }
        "colorize" => {
            let mut stops: Vec<(f32, [f32; 3], f32)> = Vec::new();
            for i in 0..16 {
                if let Some(p) = n.nums.get(&format!("p{i}")) {
                    let g = |c: &str| n.nums.get(&format!("{c}{i}")).copied().unwrap_or(0.0);
                    stops.push((*p, [g("r"), g("g"), g("b")], n.nums.get(&format!("a{i}")).copied().unwrap_or(1.0)));
                }
            }
            if stops.is_empty() {
                return None;
            }
            stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let mut x = Vec::with_capacity(stops.len() * 8);
            for (pos, c, a) in &stops {
                x.extend_from_slice(&[*pos, c[0], c[1], c[2], *a, 0.0, 0.0, 0.0]);
            }
            let p = [(f("interp", 0.0) as u32) as f32, f("smoothness", 1.0).clamp(0.0, 1.0), (f("addressing", 0.0) > 0.5) as u32 as f32, (f("use_alpha", 0.0) > 0.5) as u32 as f32];
            let t = dev("in")?;
            gpu::kernel_g(Kern::Colorize, 0, stops.len() as u32, &p, &x, &[&t, &t, &t, &t], sz)
        }
        "switch" => {
            // the chosen input as it is (nothing is copied); the first connected one when that slot is empty
            let order = ["a", "b", "c", "d"];
            let idx = f("index", 0.0).max(0.0) as usize;
            let slot = if ins.contains_key(order[idx.min(3)]) { Some(order[idx.min(3)]) } else { order.iter().copied().find(|k| ins.contains_key(*k)) };
            dev(slot?)
        }
        "white_noise" => gen(0, &[f("distribution", 0.0).round()]),
        "gaussian_noise" => gen(1, &[f("scale", 1.0).round().max(1.0)]),
        "plasma" => gen(2, &[f("scale", 3.0).round().max(1.0)]),
        "stripes" => {
            let (cnt, ang, soft, wd) = (f("count", 8.0).max(1.0), f("angle", 0.0), f("softness", 0.1).max(1e-3), f("width", 0.5));
            let (a, b) = if f("tile", 1.0) > 0.5 {
                let (a, b) = crate::ops::tileable_wave(cnt, ang);
                (a as f32, b as f32)
            } else {
                let (sa, ca) = (ang * TAU).sin_cos();
                (cnt.round() * ca, cnt.round() * sa)
            };
            gen(3, &[a, b - f("shift", 0.0).round(), f("phase", 0.0), soft, wd])
        }
        "weave" => gen(4, &[(f("count", 6.0) / 2.0).round().max(1.0) * 2.0, f("width", 0.8).clamp(0.1, 1.0)]),
        "shape" => {
            let k = f("shape", 1.0) as u32;
            if k >= 100 {
                return None;
            }
            gen(5, &[k as f32, f("size_x", 0.6), f("size_y", 0.6), f("rotation", 0.0), f("pos_x", 0.0), f("pos_y", 0.0), f("tiling", 1.0), f("softness", 0.02)])
        }
        "star" => gen(6, &[f("points", 5.0).round().max(3.0), f("inner", 0.45), f("size", 0.4), f("softness", 0.01).max(px1), f("rotation", 0.0), f("straight", 0.0)]),
        "gradient_axial" => {
            let (a, b) = crate::ops::tileable_wave(1.0, f("angle", 0.0));
            gen(7, &[a as f32, b as f32])
        }
        "gradient_circular" => gen(8, &[f("offset", 0.0)]),
        "gradient" => {
            let (a, b) = crate::ops::tileable_wave(1.0, f("angle", 0.0));
            gen(9, &[a as f32, b as f32, (f("mode", 0.0) as u32 == 1) as u32 as f32])
        }
        "circle" => gen(10, &[f("radius", 0.4), f("softness", 0.01).max(px1)]),
        "rect" => gen(11, &[f("width", 0.6), f("height", 0.4), f("softness", 0.01).max(px1)]),
        "polygon" => {
            let (sd, r, so, rot) = (f("sides", 5.0), f("radius", 0.4), f("softness", 0.01).max(px1), f("rotation", 0.0));
            let (grad, curve) = (f("gradient", 0.0).clamp(0.0, 1.0), f("curve", 0.0).clamp(-1.0, 1.0));
            let til = f("tiling", 1.0).round().clamp(1.0, 16.0);
            let apo = (r * (std::f32::consts::PI / sd.round().max(3.0)).cos()).max(1e-6);
            gen(12, &[sd, r, so, rot, grad, curve, f("gradient_invert", 0.0), til, apo])
        }
        "checker" => {
            let t = (f("tiles", 8.0) / 2.0).round().max(1.0) * 2.0;
            let r45 = f("rotate45", 0.0) > 0.5;
            let aa = (t / s as f32 * if r45 { std::f32::consts::SQRT_2 * 0.5 } else { 1.0 }).max(1e-6);
            gen(13, &[t, (f("white_first", 0.0) > 0.5) as u32 as f32, r45 as u32 as f32, f("lum_var", 0.0).clamp(0.0, 1.0), aa, t])
        }
        "bricks" => {
            let (r, c, m, b, o, lv) = (f("rows", 8.0), f("cols", 4.0), f("mortar", 0.03), f("bevel", 0.05), f("offset", 0.5), f("lum_var", 0.3));
            let b = b.max(1.5 * r.max(c) / s as f32);
            gen(14, &[r.round().max(1.0), c.round().max(1.0), m, b, o, lv])
        }
        "gradient_2pt" => gen_c(
            15,
            &[f("p1_x", 0.5), f("p1_y", 0.0), f("p2_x", 0.5), f("p2_y", 1.0), f("mode", 1.0).round(), f("tiling", 1.0).round().max(1.0), f("radial_shape", 0.0), f("peak", 0.5).clamp(0.0, 1.0), f("profile", 0.0).round(), f("angle", 0.0)],
            f("contrast", 1.0).clamp(0.0, 16.0),
        ),
        "voronoi" | "voronoi_fractal" => {
            let fractal = kind == "voronoi_fractal";
            let (p, j, mut style) = (per("scale", 6.0), f("jitter", 1.0).clamp(0.0, 1.0), f("mode", 0.0) as u32);
            if !fractal {
                style = match out_idx as i32 {
                    1 => 1,
                    2 => 2,
                    3 => 0,
                    _ => style,
                };
            }
            let aspect = (f("aspect", 1.0) * ar).clamp(0.05, 8.0);
            let metric = f("distance_mode", 0.0) as u32;
            let mp = f("minkowski", 2.0).clamp(0.25, 16.0);
            let id_seed = if f("color_seed_mode", 0.0) > 0.5 { f("color_seed", 0.0) as u32 } else { seed };
            let (lo, hi) = (f("min_level", 0.0).clamp(0.0, 8.0) as u32, f("max_level", 3.0).clamp(0.0, 8.0) as u32);
            let mut v = [0f32; 24];
            v[0] = p as f32;
            v[1] = j;
            v[2] = style as f32;
            v[3] = fractal as u32 as f32;
            v[4] = aspect;
            v[5] = f("offset_x", 0.0);
            v[6] = f("offset_y", 0.0);
            v[7] = f("distortion", 0.0).clamp(0.0, 2.0);
            v[8] = per("distortion_scale", 3.0) as f32;
            v[9] = metric as f32;
            v[10] = mp;
            v[11] = f("edge_thickness", 0.25);
            v[12] = f("rounded", 0.0);
            v[13] = f("distance_scale", 1.0).clamp(0.0, 8.0);
            v[14] = f32::from_bits(id_seed);
            v[15] = lo as f32;
            v[16] = hi.max(lo) as f32;
            v[17] = f("roughness", 0.5).clamp(0.0, 1.5);
            v[18] = f("lacunarity", 2.0).clamp(1.0, 4.0);
            v[19] = f("opacity", 1.0).clamp(0.0, 1.0);
            v[20] = f("blend", 0.0) as u32 as f32;
            gen(16, &v)
        }
        "cells" => {
            // colours from an image are sampled per cell: the CPU does that
            if f("color_source", 0.0) as u32 == 2 && ins.contains_key("color") {
                return None;
            }
            let p = f("scale", 8.0).round().clamp(1.0, 256.0);
            let py = if f("square_cells", 0.0) > 0.5 { (p * hs as f32 / s as f32).round().max(1.0) } else { p };
            gen(
                17,
                &[
                    f("variant", 0.0) as u32 as f32,
                    p,
                    py,
                    f("jitter", 1.0).clamp(0.0, 1.0),
                    f("luminance_random", 0.0).clamp(0.0, 1.0),
                    f("edge_width", 0.25).clamp(0.01, 1.0),
                    f("hardness", 2.0).clamp(0.1, 16.0),
                    f("color_source", 0.0) as u32 as f32,
                    f32::from_bits(f("pseudorandom_seed", 0.0) as u32),
                    out_idx as f32,
                ],
            )
        }
        "bnw_spots" => gen(18, &[f("variant", 0.0) as u32 as f32, per("scale", 8.0) as f32]),
        "crystal" => gen(19, &[f("variant", 0.0) as u32 as f32, per("scale", 6.0) as f32]),
        "waveform" => {
            if f("style", 0.0) > 0.5 {
                let samples = f("samples", 256.0).round().clamp(1.0, 2048.0);
                gen(
                    21,
                    &[
                        samples,
                        f("wave_number", 2.0).round().clamp(0.0, 64.0),
                        f("size_min", 0.0).clamp(0.0, 1.0),
                        f("size_max", 1.0).clamp(0.0, 1.0),
                        f("noise", 0.5).clamp(0.0, 1.0),
                        f("intensity", 1.0).clamp(0.0, 4.0),
                        1.0 / hs as f32,
                    ],
                )
            } else {
                let (a, b) = crate::ops::tileable_wave(f("count", 8.0), f("angle", 0.0));
                let w = f("warp_scale", 4.0).round().clamp(1.0, 256.0);
                gen(20, &[a as f32, b as f32, f("warp", 0.3).clamp(0.0, 4.0), w, f("shape", 0.0) as u32 as f32])
            }
        }
        "gaussian_spots" => gen(22, &[f("scale", 8.0).round().clamp(1.0, 256.0), f("size", 0.5).clamp(0.05, 1.0), f("size_variation", 0.5).clamp(0.0, 1.0)]),
        "fractal_sum" => gen(
            23,
            &[
                f("scale", 2.0).round().clamp(1.0, 256.0),
                f("min_level", 0.0).clamp(0.0, 10.0) as u32 as f32,
                f("max_level", 6.0).clamp(0.0, 10.0) as u32 as f32,
                f("roughness", 0.5).clamp(0.0, 1.5),
                f("mode", 0.0) as u32 as f32,
            ],
        ),
        "emboss" => {
            let t = dev("in")?;
            if f("mode", 0.0) > 0.5 {
                let h = match odev("height")? {
                    Some(h) => h,
                    None => return Some(t),
                };
                let p = [f("angle", 0.125), f("intensity", 4.0), f("gloss", 0.0).clamp(0.0, 1.0), 0.0, f("hl_r", 1.0), f("hl_g", 1.0), f("hl_b", 1.0), 0.0, f("sh_r", 0.0), f("sh_g", 0.0), f("sh_b", 0.0)];
                callb(opb::EMBOSS_LIT, &p, &[&t, &h])
            } else {
                callb(opb::EMBOSS, &[f("angle", 0.125), f("intensity", 4.0)], &[&t])
            }
        }
        "edge_detect" => {
            if f("mode", 0.0) <= 0.5 {
                return None;
            }
            callb(opb::EDGE, &[f("intensity", 2.0)], &[&dev("in")?])
        }
        "sobel" => callb(opb::SOBEL, &[f("intensity", 1.0).clamp(0.0, 64.0) * (s as f32 / 512.0).max(0.25)], &[&dev("in")?]),
        "swirl" => callb(opb::SWIRL, &[f("amount", 0.3), f("radius", 0.5)], &[&dev("in")?]),
        "transform" | "normal_transform" => {
            let t = dev("in")?;
            let normal = kind == "normal_transform";
            let (ox, oy, sx, sy, rot) = (f("offset_x", 0.0), f("offset_y", 0.0), f("scale_x", 1.0), f("scale_y", 1.0), f("rotation", 0.0));
            let tile = normal || f("tiling", 0.0) < 0.5;
            let (sn, cs) = (rot * TAU).sin_cos();
            let (ax, ay) = (sx.abs().max(1e-4), sy.abs().max(1e-4));
            let whole = [cs / ax, sn / ax, sn / ay, cs / ay].iter().all(|m| (m - m.round()).abs() < 1e-3);
            let mut r = callb(opb::TRANSFORM, &[ox, oy, sx, sy, rot, tile as u32 as f32], &[&t])?;
            if tile && !whole {
                r = seam(r)?;
            }
            if normal && f("rotate_vectors", 1.0) > 0.5 {
                let sgn = if f("directx", 0.0) > 0.5 { -1.0 } else { 1.0 };
                r = callb(opb::NORMAL_ROT, &[sn, cs, sgn], &[&r])?;
            }
            Some(r)
        }
        "mirror" => {
            let (axis, ox, oy) = (f("axis", 0.0) as u32, f("offset_x", 0.5), f("offset_y", 0.5));
            let (inv_x, inv_y, corner) = (f("invert_x", 0.0) > 0.5, f("invert_y", 0.0) > 0.5, f("corner", 0.0) as u32);
            let (dx, dy, ix, iy, ox, oy) = match axis {
                0 => (true, false, inv_x, false, ox, 0.5),
                1 => (false, true, false, inv_y, 0.5, oy),
                2 => (true, true, inv_x, inv_y, ox, oy),
                _ => (true, true, corner == 1 || corner == 3, corner == 2 || corner == 3, 0.5, 0.5),
            };
            let b = |v: bool| v as u32 as f32;
            callb(opb::MIRROR, &[b(dx), b(dy), b(ix), b(iy), ox, oy], &[&dev("in")?])
        }
        "mosaic" => {
            if ins.contains_key("mosaic_map") {
                return None;
            }
            callb(opb::MOSAIC, &[f("count", 16.0).round().clamp(1.0, 256.0)], &[&dev("in")?])
        }
        "symmetry" => {
            let sg = if f("flip", 0.0) > 0.5 { -1.0 } else { 1.0 };
            callb(opb::SYMMETRY, &[(f("axis", 0.0) as u32) as f32, f("offset", 0.0), sg, f("transfer", 0.0).round().max(0.0)], &[&dev("in")?])
        }
        "skew" => {
            let (kx, ky) = (f("skew_x", 1.0).clamp(-32.0, 32.0), f("skew_y", 0.0).clamp(-32.0, 32.0));
            let o = match f("align", 1.0).round() as u32 {
                0 => 0.5,
                2 => 1.0,
                _ => 0.0,
            };
            callb(opb::SKEW, &[kx, ky, o], &[&dev("in")?])
        }
        "non_uniform_directional_warp" => {
            let (t, m) = (dev("in")?, dev("intensity")?);
            let a = odev("angle")?;
            let am = a.as_ref().unwrap_or(&m);
            callb(opb::NU_DIR_WARP, &[f("intensity", 0.1).clamp(-2.0, 2.0), f("angle", 0.0) * TAU, a.is_some() as u32 as f32], &[&t, &m, am])
        }
        "multi_directional_warp" => {
            let mode = f("mode", 0.0).round() as u32;
            if mode == 3 {
                return None;
            }
            let dv = f("directions", 2.0).round().max(0.0) as usize;
            let cnt = if dv <= 2 { 2 * [1usize, 2, 4][dv] } else { dv.min(16) };
            callb(opb::MULTI_DIR_WARP, &[f("intensity", 0.1).clamp(-2.0, 2.0), f("angle", 0.0), cnt as f32, mode as f32], &[&dev("in")?, &dev("warp")?])
        }
        "non_uniform_blur" => {
            let (t, m) = (dev("in")?, dev("mask")?);
            let max = f("intensity", 8.0).clamp(0.0, 64.0) * rs;
            let mut acc: Option<GTex> = None;
            for i in 0..5u32 {
                let lv = if i == 0 { t.clone() } else { gpu::blur_g(&t, max * i as f32 / 4.0)? };
                let a = acc.as_ref().unwrap_or(&lv);
                acc = Some(callb(opb::NU_BLUR_ACC, &[i as f32], &[a, &lv, &m])?);
            }
            acc
        }
        "normal_facing" => {
            let (az, tilt) = (f("angle", 0.0) * TAU, f("tilt", 0.0).clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2);
            callb(opb::NORMAL_FACING, &[tilt.sin() * az.cos(), tilt.sin() * az.sin(), tilt.cos(), f("sharpness", 1.0).clamp(0.05, 64.0)], &[&dev("in")?])
        }
        "height_to_normal_world" => {
            let (size_cm, height_cm) = (f("size", 100.0).clamp(0.01, 1e6), f("height", 1.0).clamp(0.0, 1e6));
            let pitch = size_cm / s as f32;
            callb(opb::H2N_WORLD, &[height_cm / pitch, (f("directx", 0.0) > 0.5) as u32 as f32], &[&dev("in")?])
        }
        "normal_sobel" => {
            let ysgn = if f("directx", 0.0) > 0.5 { 1.0 } else { -1.0 };
            callb(opb::NORMAL_SOBEL, &[f("intensity", 4.0).clamp(0.0, 256.0), (f("mode", 0.0) > 0.5) as u32 as f32, ysgn], &[&dev("in")?])
        }
        "height_normal_blend" => {
            let (nm, ht) = (dev("normal")?, dev("height")?);
            let detail = gpu::normal_g(&ht, f("strength", 2.0).clamp(0.0, 64.0), f("directx", 0.0) > 0.5)?;
            callb(opb::HEIGHT_NORMAL_BLEND, &[f("mode", 0.0).round(), f("opacity", 1.0).clamp(0.0, 1.0)], &[&nm, &detail])
        }
        "min_max" => callb(opb::MIN_MAX, &[(f("mode", 1.0) > 0.5) as u32 as f32], &[&dev("a")?, &dev("b")?]),
        "frequency_merge" => callb(opb::FREQ_MERGE, &[f("gain", 1.0).clamp(0.0, 16.0)], &[&dev("low")?, &dev("high")?]),
        "frequency_split" => {
            let t = dev("in")?;
            let low = gpu::blur_g(&t, f("radius", 8.0).clamp(0.0, 128.0) * rs)?;
            if out_idx == 0 {
                Some(low)
            } else {
                callb(opb::FREQ_HIGH, &[], &[&t, &low])
            }
        }
        "multi_switch" => {
            const NAMES: [&str; 8] = ["a", "b", "c", "d", "e", "f", "g", "h"];
            let idx = (f("index", 0.0).round().clamp(0.0, 7.0)) as usize;
            let pick = ins.get(NAMES[idx]).or_else(|| NAMES.iter().find_map(|k| ins.get(*k)))?;
            fit(pick)
        }
        "polar_convert" => {
            let r = callb(opb::POLAR, &[(f("mode", 0.0) >= 0.5) as u32 as f32, f("radius", 1.0).clamp(0.05, 2.0), f("rotation", 0.0)], &[&dev("in")?])?;
            if f("tile", 1.0) > 0.5 {
                seam(r)
            } else {
                Some(r)
            }
        }
        "symmetry_slice" => {
            let r = callb(opb::SYM_SLICE, &[f("slices", 6.0).round().clamp(1.0, 64.0), (f("mirror", 1.0) > 0.5) as u32 as f32, f("rotation", 0.0)], &[&dev("in")?])?;
            if f("tile", 1.0) > 0.5 {
                seam(r)
            } else {
                Some(r)
            }
        }
        "quad_transform" | "trapezoid_transform" => {
            let t = dev("in")?;
            let quad = if kind == "quad_transform" {
                let o = |k: &str| f(k, 0.0).clamp(-0.45, 0.45);
                [[o("tl_x"), o("tl_y")], [1.0 + o("tr_x"), o("tr_y")], [1.0 + o("br_x"), 1.0 + o("br_y")], [o("bl_x"), 1.0 + o("bl_y")]]
            } else {
                let (top, bot) = (f("top", 0.6).clamp(0.05, 2.0), f("bottom", 1.0).clamp(0.05, 2.0));
                let skew = f("skew", 0.0).clamp(-0.5, 0.5);
                [[0.5 - top * 0.5 + skew, 0.0], [0.5 + top * 0.5 + skew, 0.0], [0.5 + bot * 0.5, 1.0], [0.5 - bot * 0.5, 1.0]]
            };
            let unit = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
            if quad.iter().zip(&unit).all(|(a, b): (&[f32; 2], &[f32; 2])| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6) {
                return Some(t);
            }
            let h = crate::nodes3::homography(quad, unit)?;
            let r = callb(opb::HOMOGRAPHY, &h, &[&t])?;
            if f("tile", 1.0) > 0.5 {
                seam(r)
            } else {
                Some(r)
            }
        }
        "clone_patch" => {
            let (sn, cs) = (f("rotation", 0.0) * TAU).sin_cos();
            let p = [f("src_x", 0.25), f("src_y", 0.25), f("dst_x", 0.75), f("dst_y", 0.75), f("width", 0.3).clamp(0.0, 1.0), f("height", 0.3).clamp(0.0, 1.0), f("feather", 0.25).clamp(0.0, 1.0), f("opacity", 1.0).clamp(0.0, 1.0), sn, cs];
            callb(opb::CLONE_PATCH, &p, &[&dev("in")?])
        }
        "gradient_reflected" => {
            let (a, b) = crate::ops::tileable_wave(f("count", 1.0), f("angle", 0.0));
            gen(24, &[a as f32, b as f32])
        }
        "shape_drop_shadow" => {
            let t = dev("in")?;
            let (ang, dist) = (f("angle", 0.125) * TAU, f("distance", 0.03).clamp(0.0, 0.5));
            let (sn, cs) = ang.sin_cos();
            let shifted = callb(opb::SHIFT, &[cs * dist, sn * dist], &[&t])?;
            let sh = gpu::blur_g(&shifted, f("blur", 6.0).clamp(0.0, 128.0) * rs)?;
            callb(opb::DROP_SHADOW, &[f("opacity", 0.8).clamp(0.0, 1.0), f("spread", 0.0).clamp(0.0, 1.0)], &[&t, &sh])
        }
        "metal_rough_to_spec_gloss" => {
            let base = dev("base_color")?;
            let (m, r) = (odev("metallic")?, odev("roughness")?);
            callc(opc::METAL_TO_SPEC, &[out_idx as f32, m.is_some() as u32 as f32, r.is_some() as u32 as f32], &[&base, m.as_ref().unwrap_or(&base), r.as_ref().unwrap_or(&base)])
        }
        "spec_gloss_to_metal_rough" => {
            let (d, sp) = (dev("diffuse")?, dev("specular")?);
            let g = odev("glossiness")?;
            callc(opc::SPEC_TO_METAL, &[out_idx as f32, g.is_some() as u32 as f32], &[&d, &sp, g.as_ref().unwrap_or(&d)])
        }
        "material_blend" => {
            const SLOTS: [&str; 13] = ["mask", "bg_base", "bg_normal", "bg_height", "bg_rough", "bg_metal", "bg_ao", "fg_base", "fg_normal", "fg_height", "fg_rough", "fg_metal", "fg_ao"];
            let any = fit(SLOTS.iter().find_map(|k| ins.get(*k))?)?;
            let g = |k: &str| odev(k);
            let (mask, bh, fh) = (g("mask")?, g("bg_height")?, g("fg_height")?);
            let (opacity, infl, sharp) = (f("opacity", 1.0).clamp(0.0, 1.0), f("height_influence", 1.0).clamp(0.0, 8.0), f("sharpness", 1.0).clamp(0.1, 32.0));
            let w = callc(
                opc::MAT_WEIGHT,
                &[mask.is_some() as u32 as f32, bh.is_some() as u32 as f32, fh.is_some() as u32 as f32, opacity, infl, sharp],
                &[mask.as_ref().unwrap_or(&any), bh.as_ref().unwrap_or(&any), fh.as_ref().unwrap_or(&any)],
            )?;
            let (a_slot, b_slot, op, da, db) = match out_idx {
                0 => ("bg_base", "fg_base", opc::LERP_COLOR, 0.5, 0.5),
                1 => ("bg_normal", "fg_normal", opc::LERP_NORMAL, 0.0, 0.0),
                2 => ("bg_height", "fg_height", opc::LERP_GREY, 0.5, 0.5),
                3 => ("bg_rough", "fg_rough", opc::LERP_GREY, 0.5, 0.5),
                4 => ("bg_metal", "fg_metal", opc::LERP_GREY, 0.0, 0.0),
                _ => ("bg_ao", "fg_ao", opc::LERP_GREY, 1.0, 1.0),
            };
            let (a, b) = (g(a_slot)?, g(b_slot)?);
            let p = [a.is_some() as u32 as f32, b.is_some() as u32 as f32, 0.0, 0.0, da, if op == opc::LERP_COLOR { 0.5 } else { db }, 0.5, 1.0, 0.5, 0.5, 0.5, 1.0];
            callc(op, &p, &[a.as_ref().unwrap_or(&w), b.as_ref().unwrap_or(&w), &w])
        }
        "leak_streaks" => {
            let t = dev("in")?;
            let nz = odev("noise")?;
            let len = (f("length", 120.0).clamp(0.0, 1024.0) * rs) / s.max(1) as f32;
            let (sn, cs) = (f("angle", 0.25) * TAU).sin_cos();
            let p = [len, sn, cs, f("intensity", 1.0).clamp(0.0, 8.0), f("breakup", 0.5).clamp(0.0, 1.0), nz.is_some() as u32 as f32, sz.0 as f32 / sz.1 as f32];
            callc(opc::LEAK, &p, &[&t, nz.as_ref().unwrap_or(&t)])
        }
        "settled_dust" => {
            let t = dev("in")?;
            let nz = odev("noise")?;
            let nm = gpu::normal_g(&t, f("relief", 3.0).clamp(0.0, 32.0), false)?;
            let low = gpu::blur_g(&t, f("radius", 8.0).clamp(0.0, 128.0) * rs)?;
            let p = [f("amount", 0.6).clamp(0.0, 1.0), f("slope", 0.3).clamp(0.0, 1.0), f("cavity", 0.5).clamp(0.0, 1.0), nz.is_some() as u32 as f32];
            callc(opc::DUST, &p, &[&t, &nm, &low, nz.as_ref().unwrap_or(&t)])
        }
        "make_it_tile_mirror" => callc(opc::TILE_MIRROR, &[f("mode", 0.0).round()], &[&dev("in")?]),
        "height_blend" => {
            let (bg, fg) = (dev("bg")?, dev("fg")?);
            let m = odev("mask")?;
            callc(opc::HEIGHT_BLEND, &[m.is_some() as u32 as f32, f("opacity", 1.0), f("contrast", 0.5), (f("output", 0.0) > 0.5) as u32 as f32], &[&bg, &fg, m.as_ref().unwrap_or(&bg)])
        }
        "make_it_tile_photo" => {
            let w = f("width", 0.6);
            let p = [w, f("width_v", w), f("sharp", 0.0), f("sharp_v", 0.0), f("warp", 0.0), f("warp_v", 0.0)];
            callc_n(opc::TILE_PHOTO, seed, &p, &[&dev("in")?])
        }
        "edge_wear" | "dirt" | "dust" => {
            let base = dev("in")?;
            let amount = f("amount", 0.5).clamp(0.0, 1.0);
            let soft = 0.25 / f("contrast", 1.0).max(0.1);
            let thr = 0.5 + (1.0 - amount) * 0.35;
            let m = match kind {
                "edge_wear" => {
                    let g = gpu::point_g(&base, Point::ToGray)?;
                    let b = gpu::blur_g(&g, f("width", 2.0) * rs)?;
                    let c = call(opa::AGAINST_BLUR, &[0.0, 6.0], &[&g, &b])?;
                    gpu::point_g(&c, Point::Threshold(thr, soft, 1))?
                }
                "dirt" => {
                    let inv = gpu::invert_g(&gpu::point_g(&base, Point::ToGray)?)?;
                    gpu::point_g(&inv, Point::Threshold(1.0 - amount * 0.6, soft, 1))?
                }
                _ => gpu::point_g(&gpu::point_g(&base, Point::Channel(1))?, Point::Threshold(0.95 - amount * 0.45, soft, 1))?,
            };
            match odev("noise")? {
                Some(nz) => callc(opc::MUL_LUM, &[], &[&m, &nz]),
                None => Some(m),
            }
        }
        "tile_sampler" | "tile_generator" | "splatter" | "splatter_circular" => {
            // the CPU code draws these on a square and stretches the result to a non-square graph: keep that look
            let sq = if sz.0 != sz.1 && (kind != "splatter" || out_idx == 0) { (s, s) } else { sz };
            let fit_sq = |l: &Lazy| -> Option<GTex> { gpu::resize_g(&l.dev()?, sq.0, sq.1) };
            const PAT_SLOTS: [&str; 6] = ["pattern", "pattern2", "pattern3", "pattern4", "pattern5", "pattern6"];
            let gen = kind == "tile_generator";
            let mut pats: Vec<GTex> = Vec::new();
            for k in PAT_SLOTS {
                if let Some(l) = ins.get(k) {
                    pats.push(fit_sq(l)?);
                }
            }
            let np = pats.len().max(1);
            let builtin = f("pattern_type", 0.0).max(0.0) as u32;
            let dist = f("pattern_distribution", 0.0).max(0.0) as u32;
            let bg = match ins.get("background") {
                Some(l) => Some(fit_sq(l)?),
                None => None,
            };
            let blend_i = |v: f32| match v as u32 {
                1 => 1.0,
                2 => 2.0,
                3 => 3.0,
                _ => 0.0,
            };
            // the instance records live on the device next to the pictures
            let rec = |v: Vec<[f32; 4]>| -> Option<GTex> {
                let v = if v.is_empty() { vec![[0.0; 4]; 3] } else { v };
                gpu::upload(&crate::tex::Tex { w: v.len(), h: 1, px: v })
            };
            let pack = |s: &[GTex]| -> Option<GTex> {
                let r: Vec<&GTex> = s.iter().collect();
                gpu::kernel_g(Kern::OpsC, opc::PACK4, 0, &[], &[], &r, sq)
            };
            let (inst_final, mode, ninst, outk, spec, use_spec, grid, rad, dedupe, dv, opacity, blend, reverse, background, gain, lo, hi);
            match kind {
                "splatter" => {
                    let sp = super::splatter_params(n, seed);
                    let flat = crate::scatter::splat_flat(np, dist, &sp);
                    ninst = flat.len() / 3;
                    inst_final = rec(flat)?;
                    mode = 1.0;
                    outk = out_idx.min(4) as f32;
                    spec = sp.pattern_specific;
                    use_spec = 0.0;
                    grid = (1, 1);
                    rad = 1;
                    dedupe = false;
                    dv = (1.0, 1.0);
                    opacity = sp.opacity;
                    blend = blend_i(f("blending", 0.0));
                    reverse = 0.0;
                    background = sp.background;
                    gain = sp.gain;
                    lo = sp.out_min;
                    hi = sp.out_max;
                }
                "splatter_circular" => {
                    let cp = super::circular_params(n, seed);
                    let flat = crate::scatter::circular_flat(np, if dist == 2 { 2 } else { dist }, &cp);
                    ninst = flat.len() / 3;
                    inst_final = rec(flat)?;
                    mode = 2.0;
                    outk = 0.0;
                    spec = cp.pattern_specific;
                    use_spec = 0.0;
                    grid = (1, 1);
                    rad = 1;
                    dedupe = false;
                    dv = (1.0, 1.0);
                    opacity = cp.opacity;
                    blend = blend_i(f("blending", 0.0));
                    reverse = 0.0;
                    background = cp.background;
                    gain = 1.0;
                    lo = 0.0;
                    hi = 1.0;
                }
                _ => {
                    let tp = super::tile_params(n, gen, ar, seed);
                    let slots = ["mask", "scale_map", "rotation_map", "displacement_map", "color_map", "distribution_map"];
                    let mut maps: Vec<Option<GTex>> = Vec::new();
                    for k in slots {
                        maps.push(odev(k)?);
                    }
                    let pre = crate::scatter::tile_pre(np, dist, maps[3].is_some(), &tp);
                    let pre_buf = rec(pre.inst.clone())?;
                    let nn = pre.inst.len() / 3;
                    let b = |o: &Option<GTex>| o.is_some() as u32 as f32;
                    let fp = [
                        b(&maps[0]), b(&maps[1]), b(&maps[2]), b(&maps[3]), b(&maps[4]), b(&maps[5]),
                        tp.thresh, tp.mask_invert as u32 as f32, tp.displacement_angle, tp.displacement_intensity,
                        pre.nx as f32, pre.ny as f32, tp.scale_map_mult, tp.rotation_map_mult, tp.color_map_mult, tp.aspect,
                        tp.lum_by_scale, pre.max_size, np as f32, 0.0, sz.0 as f32, sz.1 as f32,
                    ];
                    let mut fin_in: Vec<&GTex> = vec![&pre_buf];
                    for m in &maps {
                        fin_in.push(m.as_ref().unwrap_or(&pre_buf));
                    }
                    inst_final = gpu::kernel_g(Kern::TileFin, 0, 0, &fp, &[], &fin_in, (nn * 3, 1))?;
                    ninst = nn;
                    mode = 0.0;
                    outk = 0.0;
                    spec = tp.pattern_specific;
                    use_spec = 1.0;
                    grid = (pre.nx, pre.ny);
                    rad = pre.rad;
                    dedupe = pre.dedupe;
                    dv = (pre.dvx, pre.dvy);
                    opacity = tp.opacity;
                    blend = blend_i(f("blending", 0.0));
                    reverse = tp.reverse as u32 as f32;
                    background = tp.background;
                    gain = 1.0;
                    lo = 0.0;
                    hi = 1.0;
                }
            }
            let (pk0, pk1) = if pats.is_empty() {
                (inst_final.clone(), inst_final.clone())
            } else {
                let p0 = pack(&pats[..pats.len().min(4)])?;
                let p1 = if pats.len() > 4 { pack(&pats[4..])? } else { p0.clone() };
                (p0, p1)
            };
            let fallback = if kind == "splatter" { 99.0 } else if gen { 1.0 } else { 2.0 };
            let rp = [
                mode, grid.0 as f32, grid.1 as f32, dv.0, dv.1, rad as f32, opacity, blend,
                reverse, background, bg.is_some() as u32 as f32, np as f32, builtin as f32, fallback, ninst as f32, dedupe as u32 as f32,
                outk, spec, use_spec, gain, lo, hi, (!pats.is_empty()) as u32 as f32,
            ];
            let out = gpu::kernel_g(Kern::Scatter, 0, 0, &rp, &[], &[&pk0, &pk1, bg.as_ref().unwrap_or(&pk0), &inst_final], sq)?;
            gpu::resize_g(&out, sz.0, sz.1)
        }
        "distance" => {
            let st = sdf(&dev("in")?, f("threshold", 0.5))?;
            gpu::kernel_g(Kern::Jfa, 2, 0, &[f("distance", 0.1) * s as f32, f("mode", 0.0).max(0.0)], &[], &[&st], sz)
        }
        "bevel" => {
            let st = sdf(&dev("in")?, f("threshold", 0.5))?;
            let b = gpu::kernel_g(Kern::Jfa, 3, 0, &[f("distance", 0.05) * s as f32, f("smoothing", 0.5), (f("corner", 1.0) < 0.5) as u32 as f32], &[], &[&st], sz)?;
            if f("use_as_normal", 0.0) > 0.5 || out_idx >= 1 {
                gpu::normal_g(&b, f("normal_intensity", 4.0), f("directx", 0.0) > 0.5)
            } else {
                Some(b)
            }
        }
        "shape_stroke" | "shape_glow" => {
            let st = sdf(&dev("in")?, f("threshold", 0.5))?;
            let w = (f("width", 8.0).clamp(0.5, 256.0) * rs).max(2.0);
            let p = [
                (kind == "shape_stroke") as u32 as f32, w, f("offset", 0.0).clamp(-1.0, 1.0), f("softness", 0.5).clamp(0.0, 1.0), f("intensity", 1.0).clamp(0.0, 4.0),
                f("opacity", 1.0).clamp(0.0, 1.0), f("spread", 0.0).clamp(0.0, 1.0), f("color_r", 1.0), f("color_g", 1.0), f("color_b", 1.0),
            ];
            gpu::kernel_g(Kern::Jfa, 4, 0, &p, &[], &[&st], sz)
        }
        "blur_hq" => {
            let q = f("quality", 1.0).round().clamp(1.0, 4.0) as u32;
            let sigma = f("intensity", 6.0) * rs / (q as f32).sqrt();
            let mut t = dev("in")?;
            for _ in 0..q {
                t = gpu::blur_g(&t, sigma)?;
            }
            Some(t)
        }
        "curve" => {
            let get = |k: &str| -> Option<crate::curve::Curve> {
                match n.strs.get(k) {
                    Some(t) if !t.trim().is_empty() => crate::curve::Curve::parse(t).ok(),
                    _ => Some(crate::curve::Curve::identity()),
                }
            };
            let (m, r, g, b, a) = (get("points")?, get("points_r")?, get("points_g")?, get("points_b")?, get("points_a")?);
            let (lm, lr, lg, lb, la) = (m.lut(), r.lut(), g.lut(), b.lut(), a.lut());
            let nl = lm.len();
            let mut px: Vec<[f32; 4]> = (0..nl).map(|i| [lm[i], lr[i], lg[i], lb[i]]).collect();
            px.extend((0..nl).map(|i| [la[i], 0.0, 0.0, 0.0]));
            let table = gpu::upload(&crate::tex::Tex { w: 2 * nl, h: 1, px })?;
            let t = dev("in")?;
            callc(opc::CURVE, &[nl as f32], &[&t, &t, &t, &table])
        }
        "triangle_grid" => {
            if f("style", 0.0) >= 0.5 {
                return None;
            }
            gen(25, &[per256("count", 8.0), f("border", 0.06).clamp(0.0, 0.5), f("variation", 1.0).clamp(0.0, 1.0)])
        }
        "weave_generator" => {
            if f("style", 0.0) > 0.5 {
                return None;
            }
            let (tx, ty) = (per256("tile_x", 1.0).clamp(1.0, 20.0), per256("tile_y", 1.0).clamp(1.0, 20.0));
            let wv = per256("weave", 2.0).clamp(1.0, 10.0);
            let p = [tx * wv * 2.0, ty * wv * 2.0, f("shape", 0.5).clamp(0.0, 1.0), f("gap_x", 0.1).clamp(0.0, 0.9), f("gap_y", 0.1).clamp(0.0, 0.9), out_idx as f32];
            gen(26, &p)
        }
        "starburst" => {
            if f("style", 0.0) > 0.5 {
                return None;
            }
            gen(27, &[per256("count", 3.0), f("rays", 8.0).round().clamp(2.0, 512.0), f("sharpness", 2.0).clamp(0.1, 16.0), f("size", 1.0).clamp(0.05, 2.0), f("rotation", 0.0), f("rotation_random", 0.0).clamp(0.0, 1.0)])
        }
        "bilateral_blur" => {
            let t = dev("in")?;
            let rad = (f("radius", 4.0).clamp(0.0, 32.0) * rs).round().clamp(0.0, 12.0) as i64;
            let sr = f("range", 0.1).clamp(0.005, 2.0);
            if rad == 0 {
                return Some(t);
            }
            let guide = match ins.get("guide") {
                Some(g) => fit(g)?,
                None => t.clone(),
            };
            gpu::bilateral_blur_g(&t, &guide, rad as i32, sr)
        }
        "flood_fill" => gpu::flood_g(&dev("in")?, 0, f("threshold", 0.5), 11),
        "flood_fill_to_index" => gpu::flood_g(&dev("in")?, 1, 0.0, 12),
        "flood_fill_to_position" | "flood_fill_to_bbox_size" => {
            let g = gpu::flood_g(&dev("in")?, 1, 0.0, if kind == "flood_fill_to_bbox_size" { 14 } else { 13 })?;
            match out_idx {
                1 => gpu::point_g(&g, Point::Channel(0)),
                2 => gpu::point_g(&g, Point::Channel(1)),
                _ => Some(g),
            }
        }
        "flood_fill_random" => {
            let t = dev("in")?;
            callc_n(14, seed, &[f("min", 0.0), f("max", 1.0), 1.0 + f("contrast", 0.0) * 4.0], &[&t])
        }
        "flood_fill_color" => callc_n(15, seed, &[], &[&dev("in")?]),
        "flood_fill_gradient" => callc_n(16, seed, &[f("angle", 0.0), f("variation", 0.0)], &[&dev("in")?]),
        "morphology" => {
            let t = dev("in")?;
            let r = (f("radius", 4.0).clamp(0.0, 128.0) * rs).round();
            let ext = |t: &GTex, mx: bool| -> Option<GTex> {
                let a = callc(17, &[0.0, mx as u32 as f32, r], &[t])?;
                callc(17, &[1.0, mx as u32 as f32, r], &[&a])
            };
            match f("mode", 0.0).round() as i32 {
                1 => ext(&t, false),
                2 => {
                    let a = ext(&t, false)?;
                    ext(&a, true)
                }
                3 => {
                    let a = ext(&t, true)?;
                    ext(&a, false)
                }
                _ => ext(&t, true),
            }
        }
        "lookup_table" => {
            let (t, lut) = (dev("in")?, dev("lut")?);
            if (lut.w(), lut.h()) != sz {
                return None;
            }
            let vertical = f("vertical", 0.0) > 0.5;
            let (len, across) = if vertical { (sz.1, sz.0) } else { (sz.0, sz.1) };
            let line = ((f("row", 0.5).clamp(0.0, 1.0) * across as f32) as usize).min(across - 1);
            callc(18, &[(f("mode", 0.0) > 0.5) as u32 as f32, vertical as u32 as f32, (f("repeat", 0.0) > 0.5) as u32 as f32, line as f32, len as f32], &[&t, &lut])
        }
        "histogram_equalize" => {
            let t = dev("in")?;
            let cv = gpu::stats_g(&t, 0, 0.0)?;
            callc(19, &[f("amount", 1.0).clamp(0.0, 1.0), (f("per_channel", 0.0) > 0.5) as u32 as f32], &[&t, &t, &t, &cv])
        }
        "auto_crop" => {
            let t = dev("in")?;
            let bb = gpu::stats_g(&t, 1, f("threshold", 0.001).clamp(0.0, 1.0))?;
            callc(20, &[f("margin", 0.0).clamp(0.0, 0.4), (f("keep_aspect", 0.0) > 0.5) as u32 as f32], &[&t, &t, &t, &bb])
        }
        "noise_upscale" => {
            let t = dev("in")?;
            let cx = f("cells", 16.0).round().clamp(2.0, 512.0) as usize;
            let cy = ((cx as f32 * sz.1 as f32 / sz.0 as f32).round() as usize).max(1);
            let small = gpu::kernel_g(Kern::Resize, 1, 0, &[sz.0 as f32, sz.1 as f32], &[], &[&t], (cx, cy))?;
            let op = match f("mode", 0.0).round() as i32 {
                1 => 3,
                2 => 4,
                _ => 2,
            };
            gpu::kernel_g(Kern::Resize, op, 0, &[cx as f32, cy as f32], &[], &[&small], sz)
        }
        "color" => {
            if f("gray", 0.0) > 0.5 {
                let v = f("v", 0.5);
                gpu::solid_g(s, hs, [v, v, v, f("a", 1.0)])
            } else {
                gpu::solid_g(s, hs, [f("r", 0.5), f("g", 0.5), f("b", 0.5), f("a", 1.0)])
            }
        }
        "dot" => dev("in"),
        "output" => match dev("in") {
            Some(t) => Some(t),
            None => gpu::solid_g(s, hs, crate::graph::output_default(n.strs.get("usage").map(String::as_str).unwrap_or("basecolor"))),
        },
        "halftone" => {
            let t = dev("in")?;
            let nn = f("cells", 16.0).round().clamp(2.0, 256.0);
            let nyc = (nn * sz.1 as f32 / sz.0 as f32).round().max(1.0);
            callc(21, &[nn, nyc, (f("mode", 0.0) > 0.5) as u32 as f32, f("softness", 0.1).clamp(0.0, 1.0), f("gain", 1.0).clamp(0.0, 4.0)], &[&t])
        }
        "fxaa" => callc(22, &[f("strength", 1.0).clamp(0.0, 1.0), f("threshold", 0.05).clamp(0.0, 1.0)], &[&dev("in")?]),
        "cross_section" => {
            let t = dev("in")?;
            let row = ((f("position", 0.5).clamp(0.0, 1.0) * sz.1 as f32) as i64).min(sz.1 as i64 - 1);
            let thick = (f("thickness", 3.0) * rs).clamp(0.5, 64.0);
            let r = calld(0, &[row as f32, f("height", 1.0).clamp(0.0, 8.0), thick, (f("fill", 1.0) > 0.5) as u32 as f32, hs as f32 * 0.06], &[&t])?;
            fin(r)
        }
        "uber_emboss" => {
            let hm = dev("height")?;
            let (mask, gloss) = (odev("mask")?, odev("gloss")?);
            let (az, el) = (f("angle", 0.125) * TAU, f("elevation", 0.45).clamp(0.05, 1.0) * std::f32::consts::FRAC_PI_2);
            let l = [az.cos() * el.cos(), az.sin() * el.cos(), el.sin()];
            let hv = {
                let v = [l[0], l[1], l[2] + 1.0];
                let m = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                [v[0] / m, v[1] / m, v[2] / m]
            };
            calld(
                1,
                &[
                    f("intensity", 3.0).clamp(0.0, 64.0), l[0], l[1], l[2], hv[0], hv[1], hv[2], f("highlight", 1.0).clamp(0.0, 4.0), f("gain", 1.0).clamp(0.0, 8.0),
                    el.sin(), f("gloss", 0.5).clamp(0.0, 1.0), out_idx.min(2) as f32, mask.is_some() as u32 as f32, gloss.is_some() as u32 as f32, sz.0 as f32 / 512.0,
                ],
                &[&hm, mask.as_ref().unwrap_or(&hm), gloss.as_ref().unwrap_or(&hm)],
            )
        }
        "safe_transform" => {
            let t = dev("in")?;
            let mode = f("mode", 0.0).round() as i32;
            let angle = f("angle", 0.0);
            let (mut a, mut b) = (1.0f32, 0.0f32);
            let (sx, sy);
            if mode == 0 {
                let th = angle * TAU;
                let mut best = (f32::MIN, 1i32, 0i32);
                for ia in -4..=4i32 {
                    for ib in -4..=4i32 {
                        if ia == 0 && ib == 0 {
                            continue;
                        }
                        let c = (ia as f32 * th.cos() + ib as f32 * th.sin()) / ((ia * ia + ib * ib) as f32).sqrt();
                        if c > best.0 + 1e-6 || ((c - best.0).abs() <= 1e-6 && ia * ia + ib * ib < best.1 * best.1 + best.2 * best.2) {
                            best = (c, ia, ib);
                        }
                    }
                }
                a = best.1 as f32;
                b = best.2 as f32;
                sx = f("scale_x", 1.0).round().clamp(1.0, 32.0);
                sy = f("scale_y", 1.0).round().clamp(1.0, 32.0);
            } else {
                sx = f("scale_x", 1.0).clamp(0.01, 64.0);
                sy = f("scale_y", 1.0).clamp(0.01, 64.0);
            }
            let (sa, ca) = (angle * TAU).sin_cos();
            calld(
                2,
                &[
                    (mode != 0) as u32 as f32, f("filter", 1.0).round() as f32, f("offset_x", 0.0), f("offset_y", 0.0), sx, sy, a, b, f("edge", 0.0).round(),
                    (f("rotate_after_scale", 0.0) > 0.5) as u32 as f32, sa, ca,
                ],
                &[&t],
            )
        }
        "mesh_weathering" => {
            let curv = dev("curvature")?;
            let (ao, nrm, noise_in) = (odev("ao")?, odev("normal")?, odev("noise")?);
            let spread = f("up_spread", 60.0).clamp(1.0, 179.0).to_radians();
            let tilt = f("up_angle", 0.0).to_radians();
            let (px, py) = periods(f("scale", 8.0), s, hs);
            let r = callc_d(
                3,
                seed,
                &[
                    f("wear", 0.5).clamp(0.0, 1.0), f("dirt", 0.5).clamp(0.0, 1.0), f("dust", 0.5).clamp(0.0, 1.0), spread.cos(), tilt.sin(), tilt.cos(),
                    f("breakup", 0.5).clamp(0.0, 1.0), ao.is_some() as u32 as f32, nrm.is_some() as u32 as f32, noise_in.is_some() as u32 as f32, px as f32, py as f32,
                ],
                &[&curv, ao.as_ref().unwrap_or(&curv), nrm.as_ref().unwrap_or(&curv), noise_in.as_ref().unwrap_or(&curv)],
            )?;
            fin(r)
        }
        "snow_filter" => {
            let hm = dev("height")?;
            let mask = odev("mask")?;
            let (px, py) = periods(f("scale", 6.0), s, hs);
            let m = callc_d(
                4,
                seed,
                &[
                    f("coverage", 0.55).clamp(0.0, 1.0), f("slope", 0.5).clamp(0.0, 1.0), f("softness", 0.2).clamp(0.01, 1.0), f("breakup", 0.4).clamp(0.0, 1.0),
                    f("height_bias", 0.3).clamp(0.0, 1.0), f("steepness", 6.0).clamp(0.1, 64.0), mask.is_some() as u32 as f32, px as f32, py as f32, sz.0 as f32 / 512.0,
                ],
                &[&hm, mask.as_ref().unwrap_or(&hm)],
            )?;
            let blurred = gpu::blur_g(&m, (2.0 * rs).max(0.5))?;
            if out_idx == 1 {
                Some(blurred)
            } else {
                let sm2 = gpu::blur_g(&blurred, (6.0 * rs).max(1.0))?;
                calld(5, &[f("depth", 0.15).clamp(0.0, 1.0)], &[&hm, &blurred, &sm2])
            }
        }
        "season_filter" => {
            let t = dev("in")?;
            let mask = odev("mask")?;
            let season = f("season", 2.0).rem_euclid(4.0);
            let (px, py) = periods(f("scale", 8.0), s, hs);
            callc_d(
                6,
                seed,
                &[
                    mask.is_some() as u32 as f32, f("strength", 1.0).clamp(0.0, 1.0), f("variation", 0.4).clamp(0.0, 1.0), f("foliage_hue", 0.28), f("foliage_range", 0.14).clamp(0.02, 0.5),
                    season.floor(), season - season.floor(), px as f32, py as f32,
                ],
                &[&t, mask.as_ref().unwrap_or(&t)],
            )
        }
        "water_filter" => {
            let hm = dev("height")?;
            let base = odev("base")?;
            let (px, py) = periods(f("ripple_scale", 24.0), s, hs);
            callc_d(
                7,
                seed,
                &[
                    f("level", 0.35).clamp(0.0, 1.0), f("softness", 0.03).clamp(0.001, 0.5), f("tint_r", 0.1), f("tint_g", 0.25), f("tint_b", 0.3), f("murkiness", 0.5).clamp(0.0, 1.0),
                    f("wet_edge", 0.08).clamp(0.0, 0.5), f("ripples", 0.3).clamp(0.0, 1.0), f("dry_roughness", 0.7).clamp(0.0, 1.0), f("wet_roughness", 0.05).clamp(0.0, 1.0),
                    out_idx.min(3) as f32, base.is_some() as u32 as f32, px as f32, py as f32,
                ],
                &[&hm, base.as_ref().unwrap_or(&hm)],
            )
        }
        "summed_area_table" => {
            let t = dev("in")?;
            let rows = calld(8, &[(f("per_channel", 0.0) > 0.5) as u32 as f32], &[&t])?;
            calld(9, &[(f("normalize", 1.0) > 0.5) as u32 as f32], &[&rows])
        }
        "caustics" => {
            let (px, py) = periods(f("scale", 4.0), s, hs);
            let k = f("strength", 1.0).clamp(0.0, 16.0) / ((TAU * px as f32) * (TAU * px as f32)) * 2.0;
            gen2(0, &[px as f32, py as f32, f("octaves", 3.0).round().clamp(1.0, 6.0), k, 0.15 / f("sharpness", 1.0).clamp(0.05, 16.0)])
        }
        "liquid_noise" => {
            let (px, py) = periods(f("scale", 3.0), s, hs);
            gen2(1, &[px as f32, py as f32, f("octaves", 4.0).round().clamp(1.0, 8.0), f("warp", 0.35).clamp(0.0, 2.0), f("bands", 6.0).clamp(1.0, 32.0), (f("mode", 0.0) > 0.5) as u32 as f32])
        }
        "perlin_3d" => {
            let (px, py) = periods(f("scale", 4.0), s, hs);
            let pz = px.max(1);
            gen2(2, &[px as f32, py as f32, pz as f32, f("depth", 0.0) * pz as f32, f("octaves", 4.0).round().clamp(1.0, 8.0), f("gain", 0.5).clamp(0.0, 1.0)])
        }
        "paving" => {
            let (px, py) = periods(f("scale", 6.0), s, hs);
            gen2(3, &[px as f32, py as f32, f("jitter", 0.5).clamp(0.0, 1.0), f("gap", 0.04).clamp(0.0, 0.4), f("bevel", 0.08).clamp(0.001, 0.5), f("height_variation", 0.3).clamp(0.0, 1.0), f("dome", 0.3).clamp(0.0, 1.0), out_idx.min(2) as f32])
        }
        "wire_mesh" => gen2(4, &[f("count_x", 8.0).round().clamp(1.0, 128.0), f("count_y", 8.0).round().clamp(1.0, 128.0), f("wire_width", 0.3).clamp(0.02, 0.95), f("weave_depth", 0.6).clamp(0.0, 1.0), (f("mode", 0.0) > 0.5) as u32 as f32]),
        "worley_3d" => {
            let (px, py) = periods(f("scale", 6.0), s, hs);
            gen2(5, &[px as f32, py as f32, f("z", 0.0) * f("scale", 6.0), f("jitter", 1.0).clamp(0.0, 1.0), f("mode", 0.0).round()])
        }
        "fbm_3d" => {
            let (px, py) = periods(f("scale", 4.0), s, hs);
            gen2(6, &[px as f32, py as f32, f("octaves", 5.0).round().clamp(1.0, 10.0), f("gain", 0.5).clamp(0.1, 0.95), f("z", 0.0) * f("scale", 4.0), f("mode", 0.0).round()])
        }
        "volume_mask" | "gradient_3d" => {
            let rep = f("repeat", 1.0).round().clamp(1.0, 64.0);
            let repy = ((rep * hs as f32 / s as f32).round() as i32).max(1) as f32;
            gen2(
                7,
                &[
                    f("shape", 0.0).round(), f("size", 0.35).clamp(0.01, 0.5), rep, repy, f("z", 0.5) - 0.5, f("roundness", 0.0).clamp(0.0, 0.3), f("range", 0.5).clamp(0.01, 2.0),
                    f("softness", 0.02).clamp(0.0, 0.5), (kind == "volume_mask") as u32 as f32,
                ],
            )
        }
        "moisture_noise" => {
            let (px, py) = periods(f("scale", 4.0), s, hs);
            let dlen = (f("drip_length", 90.0) * rs).clamp(1.0, 1024.0);
            let (sx, sy) = periods(f("scale", 4.0) * 6.0, s, hs);
            gen2(8, &[px as f32, py as f32, f("coverage", 0.5).clamp(0.0, 1.0), f("softness", 0.15).clamp(0.01, 0.5), f("drips", 0.7).clamp(0.0, 1.0), dlen / 14.0, sx as f32, (sy / 6).max(1) as f32])
        }
        "microscope_noise" => {
            if f("style", 0.0) > 0.5 {
                let g = f("density", 14.0).round().clamp(2.0, 64.0);
                let size = f("cell_size", 0.014).clamp(0.002, 0.2);
                let reach = (size * 1.4 * 2.2 * g).ceil();
                if reach > 5.0 {
                    return None;
                }
                gen2(12, &[g, size, f("focus", 0.07).clamp(0.0, 1.0), f("warp", 0.5).clamp(0.0, 4.0) * 0.015, f("brightness", 0.45).clamp(0.0, 8.0), 1.0 / s as f32, reach])
            } else {
                let (px, py) = periods(f("scale", 10.0), s, hs);
                gen2(9, &[px as f32, py as f32, f("jitter", 0.8).clamp(0.0, 1.0), f("membrane", 0.06).clamp(0.005, 0.5), f("nucleus", 0.28).clamp(0.02, 0.8), f("grain", 0.25).clamp(0.0, 1.0), f("mode", 0.0).round()])
            }
        }
        "anisotropic_noise" => {
            if f("style", 0.0) > 0.5 {
                let nx = f("x_amount", 4.0).round().clamp(1.0, 512.0);
                let ny0 = f("y_amount", 256.0).round().clamp(1.0, 512.0);
                let turn = f("rotate", 0.0) > 0.5;
                let along = if turn { s } else { hs };
                let ny = if f("rows_by_resolution", 0.0) > 0.5 { ((ny0 * along as f32 / 256.0).round()).clamp(1.0, 8192.0) } else { ny0 };
                gen2(10, &[nx, ny, turn as u32 as f32, f("smoothness", 1.0).clamp(0.0, 1.0), f("smoothness_interpolation", 0.5).clamp(0.0, 1.0)])
            } else {
                let (a, b) = int_direction(f("angle", 0.0));
                let sy0 = f("scale", 8.0).round().clamp(1.0, 128.0) as i32;
                let sx0 = ((sy0 as f32 / f("stretch", 6.0).clamp(1.0, 64.0)).round() as i32).max(1);
                let oct = f("octaves", 3.0).round().clamp(1.0, 8.0) as i32;
                if (sx0 as i64) * (sy0 as i64) * (1i64 << (2 * (oct - 1))) > 10_000_000 {
                    return None;
                }
                gen2(11, &[a as f32, b as f32, sx0 as f32, sy0 as f32, oct as f32, f("gain", 0.5).clamp(0.1, 1.0)])
            }
        }
        "herringbone" => {
            let (nx, ny) = cells_for(f("scale", 8.0), s, hs, 4);
            let cell_px = (s as f32 / nx as f32).min(hs as f32 / ny as f32);
            let mut p = [0f32; 24];
            p[0] = nx as f32;
            p[1] = ny as f32;
            p[2] = (f("gap", 3.0) * rs).max(0.0) / cell_px;
            p[3] = (f("bevel", 6.0) * rs).max(0.5) / cell_px;
            p[4] = f("variation", 0.3).clamp(0.0, 1.0);
            p[5] = f("mode", 0.0).round();
            for (k, e) in herring_table().iter().flatten().enumerate() {
                p[8 + k] = (e.0 as i32 * 16 + e.1 * 4 + e.2) as f32;
            }
            gen2(13, &p)
        }
        "arc_pavement" => {
            let nx = f("scale", 6.0).round().clamp(2.0, 64.0) as i32;
            let ny = (((nx as f32 * hs as f32 / s as f32) * 2.0 / 2.0).round().max(1.0) as i32 * 2).clamp(2, 512);
            let rings = f("rings", 4.0).round().clamp(1.0, 12.0);
            let cell_px = s as f32 / nx as f32;
            let gap = (f("gap", 2.0) * rs).max(0.0) / cell_px;
            let half_row = 0.25 / rings;
            let bevel = ((f("bevel", 4.0) * rs).max(0.5) / cell_px).min((half_row - gap * 0.5) * 0.9).max(0.002);
            gen2(14, &[nx as f32, ny as f32, rings, f("stones", 9.0).clamp(2.0, 64.0), gap, bevel, f("variation", 0.3).clamp(0.0, 1.0), f("mode", 0.0).round()])
        }
        "sdf_shape" => gen2(15, &[f("shape", 0.0).floor(), f("size", 0.25), f("aspect", 1.0), f("sides", 5.0), f("rotation", 0.0), f("round", 0.0), f("x", 0.0), f("y", 0.0), f("range", 0.25)]),
        "sdf_form" => gen2(16, &[f("shape", 0.0).floor(), f("size", 0.3), f("repeat", 3.0).round().clamp(1.0, 32.0), f("round", 0.05), f("scatter", 0.3), f("height", 1.5)]),
        "sdf_op" => {
            let (a, b) = (dev("a")?, dev("b")?);
            calld(10, &[f("op", 0.0).floor(), f("smooth", 0.0), f("range", 0.25)], &[&a, &b])
        }
        "sdf_mask" => calld(11, &[f("offset", 0.0), f("softness", 0.02).max(1e-4)], &[&dev("in")?]),
        _ => None,
    }
}

/// Cell counts for a pattern with `n` cells across the width, both multiples of `m` (the host's `nodes7::cells_for`).
fn cells_for(n: f32, w: usize, h: usize, m: i32) -> (i32, i32) {
    let nx = ((n / m as f32).round().max(1.0) as i32 * m).clamp(m, 512);
    let ny = (((nx as f32 * h as f32 / w as f32) / m as f32).round().max(1.0) as i32 * m).clamp(m, 2048);
    (nx, ny)
}

/// Owner of each cell of the 4 x 4 herringbone period: (is horizontal, base cell x, base cell y) of its brick.
fn herring_table() -> [[(bool, i32, i32); 4]; 4] {
    let mut t = [[(false, -1, -1); 4]; 4];
    for a in 0..4i32 {
        for b in 0..4i32 {
            let (sx, sy) = ((3 * a - b).rem_euclid(4), (a + b).rem_euclid(4));
            let cells = [(false, sx, sy, sx, sy), (false, sx, sy + 1, sx, sy), (true, sx + 1, sy + 1, sx + 1, sy + 1), (true, sx + 2, sy + 1, sx + 1, sy + 1)];
            for (hor, cx, cy, bx, by) in cells {
                t[cy.rem_euclid(4) as usize][cx.rem_euclid(4) as usize] = (hor, bx.rem_euclid(4), by.rem_euclid(4));
            }
        }
    }
    t
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
