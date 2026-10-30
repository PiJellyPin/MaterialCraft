// SPDX-License-Identifier: MIT OR Apache-2.0
//! Library graphs implemented natively (see `nodes10`): image filters and utilities - colour spaces, HDR tools,
//! edge-preserving filters, crop / clone / equalise tools, mapping and projection, palettes, lighting estimates.
use crate::nodes10::{dec_n, enc_n, g, grey, is_blank, lerp, lerp4, norm3, rnd, smooth, to_lin, to_srgb, Cx};
use crate::ops;
use crate::tex::{lum, Tex};
use std::f32::consts::{PI, TAU};

const TO_AP1: [[f32; 3]; 3] = [[0.613_097_4, 0.339_523_1, 0.047_379_5], [0.070_193_7, 0.916_353_9, 0.013_452_4], [0.020_615_6, 0.109_569_8, 0.869_814_6]];
const FROM_AP1: [[f32; 3]; 3] = [[1.705_051, -0.621_792_1, -0.083_258_9], [-0.130_256_4, 1.140_804_7, -0.010_548_3], [-0.024_003_4, -0.128_969, 1.152_972_4]];

fn mat3(m: &[[f32; 3]; 3], c: [f32; 3]) -> [f32; 3] {
    [m[0][0] * c[0] + m[0][1] * c[1] + m[0][2] * c[2], m[1][0] * c[0] + m[1][1] * c[1] + m[1][2] * c[2], m[2][0] * c[0] + m[2][1] * c[1] + m[2][2] * c[2]]
}

/// Input `id` (black when not connected).
fn inp<'a>(cx: &Cx<'a>, id: &str) -> std::borrow::Cow<'a, Tex> {
    cx.img_or_ref(id, [0.0, 0.0, 0.0, 1.0])
}

/// Bilinear sampling without wrapping, including at non-power-of-two image boundaries.
fn sample_clamped(t: &Tex, u: f32, v: f32) -> [f32; 4] {
    let x = (u * t.w as f32 - 0.5).clamp(0.0, (t.w - 1) as f32);
    let y = (v * t.h as f32 - 0.5).clamp(0.0, (t.h - 1) as f32);
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(t.w - 1), (y0 + 1).min(t.h - 1));
    let top = lerp4(t.px[y0 * t.w + x0], t.px[y0 * t.w + x1], x - x0 as f32);
    let bot = lerp4(t.px[y1 * t.w + x0], t.px[y1 * t.w + x1], x - x0 as f32);
    lerp4(top, bot, y - y0 as f32)
}

fn rot(u: f32, v: f32, cu: f32, cv: f32, turns: f32, aspect: f32) -> (f32, f32) {
    let (s, c) = (turns * TAU).sin_cos();
    let (x, y) = ((u - cu) * aspect, v - cv);
    ((x * c - y * s) / aspect + cu, x * s + y * c + cv)
}

/// Pixels of a square window (radius `r`) around (x, y), wrapped.
fn window(t: &Tex, x: i64, y: i64, r: i64, f: &mut impl FnMut([f32; 4], i64, i64)) {
    for dy in -r..=r {
        for dx in -r..=r {
            f(t.at(x + dx, y + dy), dx, dy);
        }
    }
}

pub fn compute(cx: &Cx) -> Option<Tex> {
    let (s, hs) = (cx.s, cx.hs);
    let rs = cx.rs();
    let aspect = s as f32 / hs as f32;
    Some(match cx.kind {
        // ---------------------------------------------------------------- colour spaces
        "x_acescg_to_linear_srgb" | "x_acescg_to_srgb" | "x_linear_srgb_to_acescg" | "x_srgb_to_acescg" => {
            let t = inp(cx, "input");
            let k = cx.kind;
            t.map(|p| {
                let c = [p[0], p[1], p[2]];
                let o = match k {
                    "x_acescg_to_linear_srgb" => mat3(&FROM_AP1, c),
                    "x_acescg_to_srgb" => {
                        let l = mat3(&FROM_AP1, c);
                        [to_srgb(l[0]), to_srgb(l[1]), to_srgb(l[2])]
                    }
                    "x_linear_srgb_to_acescg" => mat3(&TO_AP1, c),
                    _ => mat3(&TO_AP1, [to_lin(c[0]), to_lin(c[1]), to_lin(c[2])]),
                };
                [o[0], o[1], o[2], p[3]]
            })
        }
        "x_color_temperature_adjustment" => {
            let t = inp(cx, "input");
            let (temp, tint, srgb) = (cx.f("temp").clamp(-1.0, 1.0), cx.f("magreen").clamp(-1.0, 1.0), cx.i("color_space") == 0);
            // warmer (temp > 0) raises red and lowers blue; magenta/green moves green against red and blue
            let gain = [(1.0 + 0.35 * temp) * (1.0 + 0.12 * tint), 1.0 - 0.25 * tint, (1.0 - 0.35 * temp) * (1.0 + 0.12 * tint)];
            let norm = 0.2126 * gain[0] + 0.7152 * gain[1] + 0.0722 * gain[2];
            t.map(|p| {
                let mut o = [0.0f32; 4];
                for c in 0..3 {
                    let lin = if srgb { to_lin(p[c].clamp(0.0, 1.0)) } else { p[c] };
                    let v = lin * gain[c] / norm.max(1e-4);
                    o[c] = if srgb { to_srgb(v) } else { v };
                }
                o[3] = p[3];
                o
            })
        }
        "x_exposure" => {
            let k = 2f32.powf(cx.f("exposure").clamp(-16.0, 16.0));
            inp(cx, "input").map(|p| [p[0] * k, p[1] * k, p[2] * k, p[3]])
        }
        "x_hdr_exposure_preview" => {
            let t = inp(cx, "input");
            let (lo, hi) = (cx.f("min_exposure"), cx.f("max_exposure"));
            let (lo, hi) = (lo.min(hi), lo.max(hi));
            let strips = ((hi - lo).round() as i32 + 1).clamp(1, 16);
            Tex::from_fn(s, hs, |u, v| {
                let k = ((u * strips as f32) as i32).clamp(0, strips - 1);
                let e = if strips == 1 { lo } else { lo + (hi - lo) * k as f32 / (strips - 1) as f32 };
                let p = t.sample(u, v);
                let m = 2f32.powf(e);
                // the strip border is drawn dark so the bands read apart
                let edge = ((u * strips as f32).fract() < 0.004 * strips as f32) as i32 as f32;
                let c = |x: f32| to_srgb((x * m).clamp(0.0, 1.0)) * (1.0 - 0.6 * edge);
                [c(p[0]), c(p[1]), c(p[2]), 1.0]
            })
        }
        "x_hdr_range_viewer" | "x_hdr_range_viewer_grayscale" => {
            let t = inp(cx, "input");
            let (lo, hi) = (cx.f("range_min").min(0.0), cx.f("range_max").max(1.0));
            t.map(|p| {
                let v = if cx.kind.ends_with("grayscale") { p[0] } else { lum(p) };
                if v < 0.0 {
                    // below zero: blue, brighter the further below
                    let k = (v / lo.min(-1e-4)).clamp(0.0, 1.0);
                    [0.0, 0.2 * k, 0.4 + 0.6 * k, 1.0]
                } else if v > 1.0 {
                    // above one: red to yellow up to the range's top
                    let k = ((v - 1.0) / (hi - 1.0).max(1e-4)).clamp(0.0, 1.0);
                    [1.0, k, 0.0, 1.0]
                } else {
                    grey(v)
                }
            })
        }
        "x_blackbody" => {
            let c = crate::nodes10::blackbody(cx.f("temperature"));
            Tex::solid(s, hs, [c[0], c[1], c[2], 1.0])
        }
        "x_hdr_merge_2" => {
            let n = cx.i("inputs").clamp(2, 16) as usize;
            let delta = cx.f("input_exposure_delta").max(0.0);
            let inputs: Vec<Option<&Tex>> = (1..=n).map(|i| cx.img(&format!("input_{i}"))).collect();
            let auto = cx.b("output_auto_exposure");
            let off = 2f32.powf(cx.f("output_exposure_offset"));
            // inputs go from the darkest exposure (1) to the brightest; each is brought back to the scene's radiance
            // and weighted by how well exposed it is (a hat centred on mid grey)
            let mut t = Tex::from_fn(s, hs, |u, v| {
                let mut acc = [0.0f32; 3];
                let mut wsum = 0.0f32;
                for (i, img) in inputs.iter().enumerate() {
                    let Some(img) = img else { continue };
                    let p = img.sample(u, v);
                    let scale = 2f32.powf(-(i as f32) * delta * 2.0);
                    for c in 0..3 {
                        let x = p[c].clamp(0.0, 1.0);
                        let w = (1.0 - (2.0 * x - 1.0).abs()).max(0.02);
                        acc[c] += to_lin(x) * scale * w;
                        if c == 1 {
                            wsum += w;
                        }
                    }
                }
                let k = 1.0 / wsum.max(1e-4);
                [acc[0] * k, acc[1] * k, acc[2] * k, 1.0]
            });
            let mean = t.px.iter().map(|p| lum(*p)).sum::<f32>() / t.px.len().max(1) as f32;
            let m = if auto && mean > 1e-6 { 0.18 / mean * off } else { off };
            for p in t.px.iter_mut() {
                for c in 0..3 {
                    p[c] *= m;
                }
            }
            t
        }
        "x_get_size_color" | "x_get_size_grayscale" => {
            let t = cx.img("input");
            let (w, h) = t.map(|t| (t.w, t.h)).unwrap_or((s, hs));
            Tex::solid(s, hs, [w as f32, h as f32, 0.0, 1.0])
        }
        "x_hald_clut" => {
            let t = inp(cx, "input");
            let by_alpha = cx.b("lut_intensity_by_input_alpha");
            match cx.img("lut").filter(|l| !is_blank(l)) {
                None => t.into_owned(),
                Some(lut) => {
                    // a Hald image of level L is L^3 pixels square and holds an L^2 x L^2 x L^2 colour cube
                    let side = lut.w.min(lut.h).max(1);
                    let level = ((side as f32).cbrt().round() as usize).max(2);
                    let n = level * level;
                    let fetch = |r: usize, gg: usize, b: usize| {
                        let i = r + gg * n + b * n * n;
                        lut.at((i % side) as i64, (i / side) as i64)
                    };
                    t.map(|p| {
                        let q = [p[0].clamp(0.0, 1.0) * (n - 1) as f32, p[1].clamp(0.0, 1.0) * (n - 1) as f32, p[2].clamp(0.0, 1.0) * (n - 1) as f32];
                        let i0 = [q[0] as usize, q[1] as usize, q[2] as usize];
                        let i1 = [(i0[0] + 1).min(n - 1), (i0[1] + 1).min(n - 1), (i0[2] + 1).min(n - 1)];
                        let f = [q[0] - i0[0] as f32, q[1] - i0[1] as f32, q[2] - i0[2] as f32];
                        let mut o = [0.0f32; 4];
                        for corner in 0..8 {
                            let (a, b, c) = (corner & 1, (corner >> 1) & 1, (corner >> 2) & 1);
                            let w = (if a == 1 { f[0] } else { 1.0 - f[0] }) * (if b == 1 { f[1] } else { 1.0 - f[1] }) * (if c == 1 { f[2] } else { 1.0 - f[2] });
                            let s = fetch(if a == 1 { i1[0] } else { i0[0] }, if b == 1 { i1[1] } else { i0[1] }, if c == 1 { i1[2] } else { i0[2] });
                            for k in 0..3 {
                                o[k] += s[k] * w;
                            }
                        }
                        let k = if by_alpha { p[3].clamp(0.0, 1.0) } else { 1.0 };
                        [lerp(p[0], o[0], k), lerp(p[1], o[1], k), lerp(p[2], o[2], k), p[3]]
                    })
                }
            }
        }
        // ---------------------------------------------------------------- histogram and IDs
        "x_histogram_compute" | "x_histogram_render" => {
            let t = inp(cx, "input");
            let bins = 1usize << cx.i("histogram_resolution").clamp(2, 10);
            let mut hist = vec![0f32; bins];
            for p in &t.px {
                let b = ((p[0].clamp(0.0, 1.0)) * (bins - 1) as f32).round() as usize;
                hist[b.min(bins - 1)] += 1.0;
            }
            let total = t.px.len().max(1) as f32;
            let mut cdf = vec![0f32; bins];
            let mut run = 0.0;
            for (i, h) in hist.iter().enumerate() {
                run += h / total;
                cdf[i] = run;
            }
            let peak = hist.iter().cloned().fold(0.0f32, f32::max).max(1.0);
            if cx.kind == "x_histogram_compute" {
                let hist_img = Tex::from_fn(s, hs, |u, _| grey(hist[((u * bins as f32) as usize).min(bins - 1)] / peak));
                let cdf_img = Tex::from_fn(s, hs, |u, _| grey(cdf[((u * bins as f32) as usize).min(bins - 1)]));
                cx.pick(vec![Some(hist_img), Some(cdf_img)])
            } else {
                let scale = if cx.b("automatic_scale") { 1.0 / peak } else { cx.f("scale").max(1e-4) * 8.0 / total * bins as f32 };
                let flip = cx.b("flip_y_axis");
                let cumulative = cx.i("sampling") == 1;
                Tex::from_fn(s, hs, |u, v| {
                    let b = ((u * bins as f32) as usize).min(bins - 1);
                    let height = if cumulative { cdf[b] } else { (hist[b] * scale).min(1.0) };
                    let y = if flip { 1.0 - v } else { v };
                    grey(if y <= height { 1.0 } else { 0.0 })
                })
            }
        }
        "x_id_to_mask_grayscale" => {
            let t = inp(cx, "id");
            let (mode, id) = (cx.i("selection_mode"), cx.i("id_integer"));
            let (a, b) = (cx.lane("id_range", 0).round() as i32, cx.lane("id_range", 1).round() as i32);
            t.map(|p| {
                let k = (p[0] * 255.0).round() as i32;
                let hit = if mode == 0 { k == id } else { k >= a.min(b) && k <= a.max(b) };
                grey(hit as i32 as f32)
            })
        }
        // ---------------------------------------------------------------- lighting cancellation
        "x_lighting_cancel_hf" | "x_lighting_cancel_lf" => {
            let t = inp(cx, "Source");
            let (radius, amount) = if cx.kind.ends_with("hf") { (cx.f("Radius").max(0.0) * 2.0 * rs, cx.f("Intensity").clamp(0.0, 1.0)) } else { (cx.f("Cancellation_Radius").max(0.0) * 2.0 * rs, 1.0) };
            let l = crate::nodes10::gray_of(&t);
            let low = ops::blur(&l, radius.max(0.5));
            let mean = l.px.iter().map(|p| p[0]).sum::<f32>() / l.px.len().max(1) as f32;
            let mut o = t.clone().into_owned();
            for (i, p) in o.px.iter_mut().enumerate() {
                // the light falling on the surface is what the blurred brightness says; dividing it out leaves the
                // surface's own colour at the image's average brightness
                let k = (mean / low.px[i][0].max(1e-3)).clamp(0.0, 8.0);
                let k = lerp(1.0, k, amount);
                for c in 0..3 {
                    p[c] = (p[c] * k).max(0.0);
                }
            }
            o
        }
        // ---------------------------------------------------------------- edge-preserving filters
        "x_median_filter_color" | "x_median_filter_grayscale" => {
            let t = inp(cx, "input");
            let r = [1i64, 2, 3, 4][cx.i("kernel_size").clamp(0, 3) as usize];
            let keep_alpha = !cx.b("affect_alpha");
            let mut o = t.clone().into_owned();
            let w = t.w;
            crate::tex::par_rows(&mut o.px, w, |start, rows| {
                let mut buf: Vec<(f32, [f32; 4])> = Vec::with_capacity(((2 * r + 1) * (2 * r + 1)) as usize);
                for (j, dst) in rows.iter_mut().enumerate() {
                    let i = start + j;
                    let (x, y) = ((i % w) as i64, (i / w) as i64);
                    buf.clear();
                    window(&t, x, y, r, &mut |p, _, _| buf.push((lum(p), p)));
                    buf.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    let m = buf[buf.len() / 2].1;
                    let a = if keep_alpha { t.px[i][3] } else { m[3] };
                    *dst = [m[0], m[1], m[2], a];
                }
            });
            o
        }
        "x_mlv_color" | "x_mlv_grayscale" => {
            let mut t = inp(cx, "input").into_owned();
            let r = (cx.f("intensity").max(0.0) * 3.0 * rs).round().clamp(1.0, 24.0) as i64;
            let iters = cx.i("iterations").clamp(1, 8);
            let gauss = cx.b("gaussian");
            let smoothness = cx.f("smoothness").clamp(0.0, 1.0);
            for _ in 0..iters {
                let src = t.clone();
                let w = src.w;
                crate::tex::par_rows(&mut t.px, w, |start, rows| {
                    for (j, dst) in rows.iter_mut().enumerate() {
                        let i = start + j;
                        let (x, y) = ((i % w) as i64, (i / w) as i64);
                        // the four quadrant windows around the pixel; the calmest one gives the result
                        let mut best = (f32::MAX, [0.0f32; 4]);
                        let mut all = Vec::with_capacity(4);
                        for (qx, qy) in [(-1i64, -1i64), (1, -1), (-1, 1), (1, 1)] {
                            let (mut sum, mut sum2, mut wsum) = ([0.0f32; 4], 0.0f32, 0.0f32);
                            for dy in 0..=r {
                                for dx in 0..=r {
                                    let p = src.at(x + dx * qx, y + dy * qy);
                                    let wt = if gauss { (-((dx * dx + dy * dy) as f32) / (2.0 * (r as f32 * 0.6).powi(2))).exp() } else { 1.0 };
                                    let l = lum(p);
                                    for c in 0..4 {
                                        sum[c] += p[c] * wt;
                                    }
                                    sum2 += l * l * wt;
                                    wsum += wt;
                                }
                            }
                            let mean = [sum[0] / wsum, sum[1] / wsum, sum[2] / wsum, sum[3] / wsum];
                            let ml = lum(mean);
                            let var = (sum2 / wsum - ml * ml).max(0.0);
                            all.push((var, mean));
                            if var < best.0 {
                                best = (var, mean);
                            }
                        }
                        // smoothness mixes in the other windows by how calm they are
                        let o = if smoothness > 1e-3 {
                            let mut acc = [0.0f32; 4];
                            let mut ws = 0.0;
                            for (var, m) in &all {
                                let wv = 1.0 / (1.0 + (var - best.0) / (smoothness * 0.01 + 1e-6));
                                for c in 0..4 {
                                    acc[c] += m[c] * wv;
                                }
                                ws += wv;
                            }
                            [acc[0] / ws, acc[1] / ws, acc[2] / ws, acc[3] / ws]
                        } else {
                            best.1
                        };
                        *dst = o;
                    }
                });
            }
            t
        }
        "x_anisotropic_kuwahara_color" | "x_anisotropic_kuwahara_grayscale" => kuwahara(cx),
        // ---------------------------------------------------------------- shape tools
        "x_directional_distance" => {
            let t = inp(cx, "input");
            let dmap = cx.img("distance_map");
            let amap = cx.img("angle_map");
            let maxd = cx.f("max_distance").clamp(0.0, 1.0);
            let (dmul, amul, aoff) = (cx.f("distance_map_multiplier"), cx.f("angle_map_multiplier"), cx.f("angle_map_offset"));
            let angle = cx.f("angle");
            let steps = (maxd * s as f32).ceil().clamp(1.0, 2048.0) as i32;
            let mode = cx.i("output");
            let mut uvs = Tex::solid(s, hs, [0.0; 4]);
            let mut dist = Tex::solid(s, hs, grey(1.0));
            let w = s;
            let tt = &t;
            let results: Vec<([f32; 4], [f32; 4])> = crate::tex::par_map(s * hs, |i| {
                let (x, y) = (i % w, i / w);
                let (u, v) = ((x as f32 + 0.5) / s as f32, (y as f32 + 0.5) / hs as f32);
                let md = maxd * dmap.map(|m| lerp(1.0, g(m, u, v), dmul)).unwrap_or(1.0);
                let a = angle + amap.map(|m| (g(m, u, v) - aoff) * amul).unwrap_or(0.0);
                let (sn, cs) = (a * TAU).sin_cos();
                let (dx, dy) = (cs / s as f32, -sn / hs as f32);
                let n = ((md * s as f32).ceil() as i32).min(steps).max(1);
                for k in 0..=n {
                    let (pu, pv) = (u + dx * k as f32, v + dy * k as f32);
                    if g(tt, pu, pv) > 0.5 {
                        let d = k as f32 / n as f32;
                        let d = if mode == 1 { 1.0 - d } else { d };
                        return (grey(d), [crate::noise::wrap01(pu), crate::noise::wrap01(pv), 0.0, 1.0]);
                    }
                }
                (grey(if mode == 1 { 0.0 } else { 1.0 }), [u, v, 0.0, 0.0])
            });
            for (i, (d, uv)) in results.into_iter().enumerate() {
                dist.px[i] = d;
                uvs.px[i] = uv;
            }
            cx.pick(vec![Some(dist), Some(uvs)])
        }
        "x_extend_shape_color" | "x_extend_shape_grayscale" => {
            let t = inp(cx, "input");
            let (a, b) = (cx.v2("start_position"), cx.v2("end_position"));
            let ang = cx.f("extension_angle") * TAU;
            let mut d = [b[0] - a[0], b[1] - a[1]];
            let l = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-5);
            d = [d[0] / l, d[1] / l];
            let (sn, cs) = ang.sin_cos();
            d = [d[0] * cs - d[1] * sn, d[0] * sn + d[1] * cs];
            let ext = cx.f("extension_distance").clamp(0.0, 1.0);
            let pos = cx.f("extension_position");
            let gray = cx.kind.ends_with("grayscale");
            let (lo, hi) = if gray { (cx.f("start_luminance_offset"), cx.f("end_luminance_offset")) } else { (0.0, 0.0) };
            let ignore_black = gray && cx.b("luminance_offset_ignores_black_pixels");
            let mode = cx.i("mode");
            Tex::from_fn(s, hs, |u, v| {
                // the line through the end position, across the direction, is the edge that gets pulled outward
                let e = [b[0] + d[0] * pos * l, b[1] + d[1] * pos * l];
                let t_along = (u - e[0]) * d[0] + (v - e[1]) * d[1];
                if t_along <= 0.0 {
                    return t.sample(u, v);
                }
                if t_along > ext && mode == 0 {
                    return t.sample(u - d[0] * ext, v - d[1] * ext);
                }
                let k = t_along.min(ext);
                let mut p = t.sample(u - d[0] * k, v - d[1] * k);
                if gray && !(ignore_black && p[0] <= 0.0) {
                    let off = lerp(lo, hi, (k / ext.max(1e-5)).clamp(0.0, 1.0));
                    for c in 0..3 {
                        p[c] = (p[c] + off).clamp(0.0, 1.0);
                    }
                }
                p
            })
        }
        "x_floodfill_mapper_color" | "x_floodfill_mapper_grayscale" => floodfill_mapper(cx),
        // ---------------------------------------------------------------- lighting estimates
        "x_rt_shadows" | "x_shadows" => {
            let h = inp(cx, if cx.kind == "x_rt_shadows" { "input" } else { "Input" });
            let sh = shadow_params(cx, s, hs);
            let (dir, elev, len, steps, height_scale, atten, light_k, opacity, soft, invert) = (sh.dir, sh.elev, sh.len, sh.steps as usize, sh.height_scale, sh.atten, sh.light_k, sh.opacity, sh.soft, sh.invert);
            let hh = &h;
            let res = crate::tex::par_map(s * hs, |i| {
                let (u, v) = (((i % s) as f32 + 0.5) / s as f32, ((i / s) as f32 + 0.5) / hs as f32);
                let h0 = g(hh, u, v) * height_scale;
                let mut lit = 0.0;
                let taps = sh.taps.clamp(1, 8) as i32;
                for k in 0..taps {
                    // jittered directions inside the light's size give soft shadow edges
                    let j = (rnd(i as i32, k, 91) - 0.5) * soft * 6.0;
                    let (sj, cj) = j.sin_cos();
                    let d = [dir[0] * cj - dir[1] * sj, dir[0] * sj + dir[1] * cj];
                    let off = rnd(i as i32, k, 97);
                    let mut shade = 0.0f32;
                    for st in 0..steps {
                        let t = (st as f32 + off) / steps as f32 * len;
                        if t <= 0.5 / s.max(hs) as f32 {
                            continue;
                        }
                        let ray = h0 + t * elev;
                        if g(hh, u + d[0] * t, v + d[1] * t) * height_scale > ray + 1e-4 {
                            shade = 1.0 - atten * (t / len.max(1e-6));
                            break;
                        }
                    }
                    lit += 1.0 - shade.max(0.0);
                }
                let lit = lit / taps as f32;
                let v = light_k * (1.0 - opacity * (1.0 - lit));
                if invert {
                    1.0 - v
                } else {
                    v
                }
            });
            Tex { w: s, h: hs, px: res.into_iter().map(grey).collect() }
        }
        "x_rt_irradiance" => {
            let base = cx.img_or_ref("basecolor", grey(0.5));
            let metal = cx.img_or_ref("metallic", grey(0.0));
            let h = cx.img_or_ref("height", grey(0.0));
            let em = cx.img("emissive");
            let env = cx.img("environment");
            let hs_k = cx.f("height_scale");
            let exposure = 2f32.powf(cx.f("environment_exposure"));
            let ei = cx.f("emissive_intensity");
            let ao = ops::ambient_occlusion(&h, 8.0 * rs, (hs_k * 10.0).clamp(0.0, 4.0));
            let sky = env.map(ops::mean_rgb).unwrap_or([0.85, 0.9, 1.0]);
            let glow = em.map(|e| ops::blur(e, 6.0 * rs));
            let raw = Tex::from_fn(s, hs, |u, v| {
                let a = g(&ao, u, v);
                let mut c = [sky[0] * a * exposure, sky[1] * a * exposure, sky[2] * a * exposure];
                if let Some(gl) = &glow {
                    let q = gl.sample(u, v);
                    for k in 0..3 {
                        c[k] += q[k] * ei * 0.1;
                    }
                }
                [c[0], c[1], c[2], 1.0]
            });
            let lit = Tex::from_fn(s, hs, |u, v| {
                let b = base.sample(u, v);
                let m = g(&metal, u, v);
                let r = raw.sample(u, v);
                let mut o = [0.0f32; 4];
                for k in 0..3 {
                    o[k] = b[k] * (1.0 - m) * r[k] + em.map(|e| e.sample(u, v)[k] * ei * 0.1).unwrap_or(0.0);
                }
                o[3] = 1.0;
                o
            });
            cx.pick(vec![Some(lit), Some(raw)])
        }
        // ---------------------------------------------------------------- normals
        "x_normal_intensity" => {
            let k = 4f32.powf(cx.f("intensity").clamp(-1.0, 1.0));
            inp(cx, "input").map(|p| {
                let n = [(p[0] * 2.0 - 1.0) * k, (p[1] * 2.0 - 1.0) * k, (p[2] * 2.0 - 1.0).max(1e-3)];
                let n = norm3(n);
                [n[0] * 0.5 + 0.5, n[1] * 0.5 + 0.5, n[2] * 0.5 + 0.5, p[3]]
            })
        }
        "x_normal_uncombine" => {
            let n = cx.img_or_ref("normal", [0.5, 0.5, 1.0, 1.0]);
            let h = inp(cx, "height");
            let dx = cx.i("normal_format") == 0;
            let (lo, hi) = (cx.lane("intensity_range", 0), cx.lane("intensity_range", 1));
            // the height's own normal at unit strength; the strength that best explains the combined normal is the
            // least-squares fit of the slopes
            let unit = ops::normal_from_height(&h, 1.0, dx);
            let (mut num, mut den) = (0.0f64, 0.0f64);
            for (a, b) in n.px.iter().zip(&unit.px) {
                let (na, nb) = (dec_n(*a, false), dec_n(*b, false));
                let (sa, sb) = ([na[0] / na[2].max(0.05), na[1] / na[2].max(0.05)], [nb[0] / nb[2].max(0.05), nb[1] / nb[2].max(0.05)]);
                num += (sa[0] * sb[0] + sa[1] * sb[1]) as f64;
                den += (sb[0] * sb[0] + sb[1] * sb[1]) as f64;
            }
            let k = if den > 1e-9 { (num / den) as f32 } else { 0.0 };
            let k = k.clamp(lo.min(hi), lo.max(hi));
            let detail = Tex {
                w: n.w,
                h: n.h,
                px: n
                    .px
                    .iter()
                    .zip(&unit.px)
                    .map(|(a, b)| {
                        let (na, nb) = (dec_n(*a, false), dec_n(*b, false));
                        let sx = na[0] / na[2].max(0.05) - k * nb[0] / nb[2].max(0.05);
                        let sy = na[1] / na[2].max(0.05) - k * nb[1] / nb[2].max(0.05);
                        enc_n([sx, sy, 1.0], false)
                    })
                    .collect(),
            };
            let span = (hi - lo).abs().max(1e-4);
            cx.pick(vec![Some(detail), Some(Tex::solid(s, hs, grey(((k - lo.min(hi)) / span).clamp(0.0, 1.0))))])
        }
        // ---------------------------------------------------------------- transforms
        "x_clone" => {
            let src = inp(cx, "Source");
            let mask = cx.img("Mask").map(|m| ops::blur(m, cx.f("Blur_Mask").max(0.0) * 2.0 * rs));
            let off = cx.v2("Offset");
            Tex::from_fn(s, hs, |u, v| {
                let a = src.sample(u, v);
                let k = mask.as_ref().map(|m| g(m, u, v).clamp(0.0, 1.0)).unwrap_or(0.0);
                if k <= 0.0 {
                    return a;
                }
                lerp4(a, src.sample(u - off[0], v - off[1]), k)
            })
        }
        "x_non_square_transform" | "x_non_square_transform_greyscale" => {
            let t = inp(cx, "input");
            let tile = [cx.lane("tile", 0).round().max(1.0), cx.lane("tile", 1).round().max(1.0)];
            let off = cx.v2("offset");
            let r = cx.f("rotation");
            let safe = cx.b("tile_safe_rotation");
            let r = if safe { (r * 4.0).round() / 4.0 } else { r };
            let wrap = cx.i("tile_mode") == 0;
            let bg = cx.col("background_color");
            Tex::from_fn(s, hs, |u, v| {
                let (ru, rv) = rot(u, v, 0.5, 0.5, -r, aspect);
                let (su, sv) = ((ru - off[0]) * tile[0], (rv - off[1]) * tile[1]);
                if !wrap && !(0.0..1.0).contains(&(ru - off[0])) || !wrap && !(0.0..1.0).contains(&(rv - off[1])) {
                    return bg;
                }
                t.sample(su, sv)
            })
        }
        "x_non_uniform_rotation_color" | "x_non_uniform_rotation_grayscale" => {
            let t = inp(cx, "input");
            let rmap = cx.img("rotation_map");
            let pmap = cx.img("rotation_pivot_map");
            let (mul, off) = (cx.f("rotation_angle_multiplier"), cx.f("rotation_angle_offset"));
            let use_map = cx.b("use_pivot_position_map");
            let piv = cx.v2("pivot_position");
            let bg = cx.col("background_color");
            let wraps = false;
            Tex::from_fn(s, hs, |u, v| {
                let a = rmap.map(|m| g(m, u, v)).unwrap_or(0.0) * mul + off;
                let (cu, cv) = match (use_map, pmap) {
                    (true, Some(p)) => {
                        let q = p.sample(u, v);
                        (q[0], q[1])
                    }
                    _ => (piv[0], piv[1]),
                };
                let (su, sv) = rot(u, v, cu, cv, -a, aspect);
                if !wraps && (!(0.0..=1.0).contains(&su) || !(0.0..=1.0).contains(&sv)) {
                    return bg;
                }
                t.sample(su, sv)
            })
        }
        "x_crop" | "x_crop_grayscale" | "x_multi_crop" | "x_multi_crop_grayscale" => {
            let m = cx.v4("matrix");
            let off = cx.v2("offset");
            let nearest = cx.i("filtering") == 1; // 0 bilinear, 1 nearest (as in Transformation 2D)
            let multi = cx.kind.starts_with("x_multi");
            let bg = cx.col("background");
            let show = cx.b("show_crop_area");
            let first = if multi { "input_1" } else { "input" };
            let crop = |t: &Tex| Tex::from_fn(s, hs, |u, v| {
                // the matrix zooms in around the offset (the crop area's top-left corner)
                let det = m[0] * m[3] - m[1] * m[2];
                let det = if det.abs() < 1e-6 { 1e-6 } else { det };
                let su = (m[3] * u - m[1] * v) / det + off[0];
                let sv = (-m[2] * u + m[0] * v) / det + off[1];
                if !(0.0..=1.0).contains(&su) || !(0.0..=1.0).contains(&sv) {
                    return bg;
                }
                if nearest {
                    // Crop does not tile: coordinates on the far edge belong to the last texel.
                    t.at(((su * t.w as f32).floor() as i64).min(t.w as i64 - 1), ((sv * t.h as f32).floor() as i64).min(t.h as i64 - 1))
                } else {
                    sample_clamped(t, su, sv)
                }
            });
            let area = |t: &Tex| Tex::from_fn(s, hs, |u, v| {
                let p = t.sample(u, v);
                if !show {
                    return p;
                }
                // the crop area keeps its brightness, the rest is dimmed
                let det = m[0] * m[3] - m[1] * m[2];
                let (x, y) = (u - off[0], v - off[1]);
                let (cu, cv) = ((m[0] * x + m[1] * y), (m[2] * x + m[3] * y));
                let inside = (0.0..=1.0).contains(&cu) && (0.0..=1.0).contains(&cv) && det.abs() > 1e-6;
                if inside {
                    p
                } else {
                    [p[0] * 0.35, p[1] * 0.35, p[2] * 0.35, p[3]]
                }
            });
            if multi {
                let n = cx.i("input_count").clamp(1, 8) as usize;
                let mut outs: Vec<Option<Tex>> = (1..=8).map(|i| if i <= n && cx.wants(i - 1) { Some(crop(&inp(cx, &format!("input_{i}")))) } else { None }).collect();
                outs.push(if cx.wants(8) { Some(area(&inp(cx, first))) } else { None });
                cx.pick(outs)
            } else {
                let src = inp(cx, first);
                // Keep the original texels for Nearest, but retain the executor's area-average
                // downsampling for Bilinear and the crop-area preview. Fit at most once.
                let fitted = if ((!nearest && cx.wants(0)) || cx.wants(1)) && (src.w != s || src.h != hs) {
                    std::borrow::Cow::Owned(ops::resize_to(&src, s, hs))
                } else {
                    std::borrow::Cow::Borrowed(src.as_ref())
                };
                let c = if cx.wants(0) { Some(crop(if nearest { &src } else { &fitted })) } else { None };
                let a = if cx.wants(1) { Some(area(&fitted)) } else { None };
                cx.pick(vec![c, a])
            }
        }
        "x_multi_clone_patch" | "x_multi_clone_patch_grayscale" => {
            let n = cx.i("input_count").clamp(1, 8) as usize;
            let pt = Patch::new(cx);
            let first = inp(cx, "input_1");
            let mut outs: Vec<Option<Tex>> = (1..=8).map(|i| if i <= n && cx.wants(i - 1) { Some(pt.apply(&inp(cx, &format!("input_{i}")), s, hs)) } else { None }).collect();
            outs.push(if cx.wants(8) { Some(pt.areas(&first, s, hs)) } else { None });
            cx.pick(outs)
        }
        "x_color_equalizer" | "x_multi_color_equalizer" => {
            let multi = cx.kind.starts_with("x_multi");
            let radius = cx.f("radius").max(0.0) * 2.0 * rs;
            let balance = cx.f("balance").clamp(0.0, 1.0);
            let target = cx.col("color");
            let (hr, cr, lr) = (cx.f("mask_hue_range") / 360.0, cx.f("mask_chroma_range"), cx.f("mask_luma_range"));
            let user_mask = cx.img("mask_input");
            let mask_source = cx.i("mask_source");
            let mask_blur = cx.f("mask_blur") * 2.0 * rs;
            let first = if multi { "input_1" } else { "input" };
            let src = inp(cx, first);
            // the mask: pixels whose colour lies near the chosen colour (or the given mask)
            let mut mask = match (mask_source, user_mask) {
                (1, Some(m)) => crate::nodes10::gray_of(m),
                _ => {
                    let (th, tc, tl) = hcl(target);
                    src.map(|p| {
                        let (h, c, l) = hcl(p);
                        let dh = ((h - th + 0.5).rem_euclid(1.0) - 0.5).abs();
                        let k = (1.0 - dh / hr.max(1e-3)).clamp(0.0, 1.0) * (1.0 - (c - tc).abs() / cr.max(1e-3)).clamp(0.0, 1.0) * (1.0 - (l - tl).abs() / lr.max(1e-3)).clamp(0.0, 1.0);
                        grey(if cx.b("custom_color_variation") { k } else { 1.0 })
                    })
                }
            };
            if mask_blur > 0.1 {
                mask = ops::blur(&mask, mask_blur);
            }
            let eq = |t: &Tex| {
                let low = ops::blur(t, radius.max(0.5));
                let mean = ops::mean_rgb(t);
                let mut o = t.clone();
                for (i, p) in o.px.iter_mut().enumerate() {
                    let k = balance * mask.px[i][0];
                    for c in 0..3 {
                        p[c] = (p[c] + (mean[c] - low.px[i][c]) * k * 2.0).max(0.0);
                    }
                }
                o
            };
            if multi {
                let n = cx.i("input_count").clamp(1, 8) as usize;
                let outs: Vec<Option<Tex>> = (1..=8).map(|i| if i <= n && cx.wants(i - 1) { Some(eq(&inp(cx, &format!("input_{i}")))) } else { None }).collect();
                cx.pick(outs)
            } else {
                cx.pick(vec![Some(eq(&src)), Some(mask)])
            }
        }
        // ---------------------------------------------------------------- photometric
        "x_multiangle_to_albedo" => {
            let n = cx.i("samples_amount").clamp(2, 8) as usize;
            let ins: Vec<&Tex> = (1..=n).filter_map(|i| cx.img(&format!("input_{i}"))).collect();
            if ins.is_empty() {
                Tex::solid(s, hs, grey(0.0))
            } else {
                // the median of the lit samples drops highlights and shadows that only some light directions make
                Tex::from_fn(s, hs, |u, v| {
                    let mut samples = [[0.0; 4]; 8];
                    let v4 = &mut samples[..ins.len()];
                    for (p, t) in v4.iter_mut().zip(&ins) {
                        *p = t.sample(u, v);
                    }
                    // At most eight elements: stable insertion sort needs no heap allocation.
                    v4.sort_by(|a, b| lum(*a).partial_cmp(&lum(*b)).unwrap_or(std::cmp::Ordering::Equal));
                    let m = v4.len() / 2;
                    if v4.len() % 2 == 0 {
                        lerp4(v4[m - 1], v4[m], 0.5)
                    } else {
                        v4[m]
                    }
                })
            }
        }
        "x_multiangle_to_normal" => {
            let n = cx.i("samples_amount").clamp(2, 8) as usize;
            let first = cx.f("first_sample_light_angle");
            let step_mode = cx.i("next_sample_light_angle");
            let intensity = cx.f("intensity").clamp(0.0, 1.0) * 4.0;
            let dx = cx.i("normal_format") == 0;
            let ins: Vec<(f32, f32, &Tex)> = (1..=n)
                .filter_map(|i| {
                    let step = if step_mode == 0 { 1.0 / n as f32 } else { -1.0 / n as f32 };
                    cx.img(&format!("input_{i}")).map(|t| {
                        let a = (first + step * (i - 1) as f32) * TAU;
                        (a.cos(), a.sin(), t)
                    })
                })
                .collect();
            Tex::from_fn(s, hs, |u, v| {
                // each sample is lit from its own direction: brighter where the surface leans toward that light
                let (mut gx, mut gy, mut tot) = (0.0f32, 0.0f32, 0.0f32);
                for (cos, sin, t) in &ins {
                    let l = g(t, u, v);
                    gx += l * cos;
                    gy += l * sin;
                    tot += l;
                }
                let k = intensity / tot.max(1e-4);
                enc_n([gx * k, gy * k, 1.0], dx)
            })
        }
        // ---------------------------------------------------------------- mapping and projection
        "x_uv_mapper_color" | "x_uv_mapper_grayscale" | "x_pbr_render_mapping_color" | "x_pbr_render_mapping_grayscale" => {
            let (tex_id, bg) = if cx.kind.starts_with("x_pbr") { ("texture", cx.col("background_color")) } else { ("input", if cx.kind.ends_with("color") { cx.col("background_color") } else { grey(0.0) }) };
            let t = inp(cx, tex_id);
            let uv = cx.img("uv");
            match uv {
                None => Tex::solid(s, hs, bg),
                Some(uvm) => Tex::from_fn(s, hs, |u, v| {
                    let q = uvm.sample(u, v);
                    if q[3] <= 0.0 {
                        return bg;
                    }
                    t.sample(q[0], q[1])
                }),
            }
        }
        "x_triplanar" | "x_triplanar_2" | "x_triplanar_3" | "x_triplanar_grayscale" | "x_triplanar_grayscale_2" | "x_triplanar_grayscale_3" => {
            let pos = cx.img("position");
            let nrm = cx.img("ws_normal");
            let single = cx.i("image_inputs") == 0;
            // the older graphs have one image input, `input`, and no per-axis images
            let (tx, ty, tz) = (inp(cx, if cx.kind.ends_with("_3") { "input_x" } else { "input" }), cx.img("input_y"), cx.img("input_z"));
            let tiling = cx.f("texture_tiling").max(0.0);
            let contrast = cx.f("blending_contrast").clamp(0.001, 1.0);
            let rots = [cx.f("rotation_x"), cx.f("rotation_y"), cx.f("rotation_z")];
            let offs = [cx.v2("offset_x"), cx.v2("offset_y"), cx.v2("offset_z")];
            let fix = cx.b("fix_mirrored_projection");
            // 0 all axes, 1..3 that axis alone
            let only = cx.i("projection").clamp(0, 3) as usize;
            let advanced = cx.i("blending_mode") == 1;
            Tex::from_fn(s, hs, |u, v| {
                let p = pos.map(|t| {
                    let q = t.sample(u, v);
                    [q[0], q[1], q[2]]
                });
                let p = p.unwrap_or([u, 1.0 - v, 0.5]);
                let n = nrm.map(|t| dec_n(t.sample(u, v), false)).unwrap_or([0.0, 0.0, 1.0]);
                let pw = 1.0 + contrast * 15.0;
                let mut w = [n[0].abs().powf(pw), n[1].abs().powf(pw), n[2].abs().powf(pw)];
                if only > 0 {
                    w = [0.0; 3];
                    w[only - 1] = 1.0;
                }
                let ws = (w[0] + w[1] + w[2]).max(1e-6);
                let planes = [(p[2], p[1]), (p[0], p[2]), (p[0], p[1])];
                let (mut samples, mut mix) = ([[0.0f32; 4]; 3], [0.0f32; 3]);
                for axis in 0..3 {
                    if w[axis] <= 1e-6 {
                        continue;
                    }
                    let (mut a, b) = planes[axis];
                    if fix && n[axis] < 0.0 {
                        a = -a;
                    }
                    let (a, b) = rot(a * tiling, b * tiling, 0.0, 0.0, rots[axis], 1.0);
                    let (a, b) = (a + offs[axis][0], b + offs[axis][1]);
                    let src = if single {
                        &tx
                    } else {
                        match axis {
                            0 => &tx,
                            1 => ty.unwrap_or(&tx),
                            _ => tz.unwrap_or(&tx),
                        }
                    };
                    samples[axis] = src.sample(a, 1.0 - b);
                    mix[axis] = w[axis] / ws;
                    if advanced {
                        mix[axis] *= 1.0 + lum(samples[axis]).max(0.0);
                    }
                }
                if advanced {
                    // the axis with the strongest mix of facing and picture value wins; `band` is the width of the
                    // transition around it, narrower with more contrast
                    let top = mix[0].max(mix[1]).max(mix[2]);
                    let band = 0.02 + 0.5 * (1.0 - contrast);
                    let mut total = 0.0;
                    for m in &mut mix {
                        *m = if *m > 0.0 { (*m - top + band).max(0.0) } else { 0.0 };
                        total += *m;
                    }
                    mix = mix.map(|m| m / f32::max(total, 1e-6));
                }
                let mut o = [0.0f32; 4];
                for axis in 0..3 {
                    for c in 0..4 {
                        o[c] += samples[axis][c] * mix[axis];
                    }
                }
                o
            })
        }
        "x_3d_planar_projection" | "x_3d_planar_projection_color" => {
            let pos = cx.img("position");
            let nrm = cx.img("wsnormal");
            let tex = inp(cx, "texture");
            let uv_mode = cx.i("projection_input") == 0;
            let target = if uv_mode { pos.map(|p| { let q = p.sample(cx.lane("target_uv_position", 0), cx.lane("target_uv_position", 1)); [q[0], q[1], q[2]] }).unwrap_or([0.5, 0.5, 0.5]) } else { cx.v3("target_position") };
            let tn = cx.v3("target_normal");
            let dir = norm3([tn[0] * 2.0 - 1.0, tn[1] * 2.0 - 1.0, tn[2] * 2.0 - 1.0]);
            let up = if dir[1].abs() > 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
            let ax = norm3(crate::nodes10::cross3(up, dir));
            let ay = crate::nodes10::cross3(dir, ax);
            let scale = cx.f("scale").max(1e-3);
            let size = cx.v2("size");
            let rotation = cx.f("rotation");
            let (maxd, dfade, nth, nfade) = (cx.f("max_depth"), cx.f("depth_fade"), cx.f("normal_threshold"), cx.f("norma_fade"));
            Tex::from_fn(s, hs, |u, v| {
                let p = pos.map(|t| { let q = t.sample(u, v); [q[0], q[1], q[2]] }).unwrap_or([u, 1.0 - v, 0.5]);
                let n = nrm.map(|t| dec_n(t.sample(u, v), false)).unwrap_or([0.0, 0.0, 1.0]);
                let d = crate::nodes10::sub3(p, target);
                let (a, b) = (crate::nodes10::dot3(d, ax), crate::nodes10::dot3(d, ay));
                let (a, b) = rot(a, b, 0.0, 0.0, rotation, 1.0);
                let (pu, pv) = (a / (scale * size[0].max(1e-3)) + 0.5, -b / (scale * size[1].max(1e-3)) + 0.5);
                if !(0.0..=1.0).contains(&pu) || !(0.0..=1.0).contains(&pv) {
                    return [0.0, 0.0, 0.0, 0.0];
                }
                let depth = crate::nodes10::dot3(d, dir).abs();
                let fd = 1.0 - smooth(maxd * (1.0 - dfade), maxd.max(1e-4), depth);
                let facing = crate::nodes10::dot3(n, dir);
                let fnrm = smooth(nth - nfade * 0.5 - 1e-3, nth + nfade * 0.5, facing);
                let k = fd * fnrm;
                let q = tex.sample(pu, pv);
                [q[0] * k, q[1] * k, q[2] * k, q[3] * k]
            })
        }
        "x_3d_linear_gradient" => {
            let pos = cx.img("position");
            let at = |uv: [f32; 2]| pos.map(|p| { let q = p.sample(uv[0], uv[1]); [q[0], q[1], q[2]] }).unwrap_or([uv[0], 1.0 - uv[1], 0.5]);
            let (a, b) = if cx.i("points_position_mode") == 0 { (at(cx.v2("point_1_uv")), at(cx.v2("point_2_uv"))) } else { (cx.v3("point_1_world"), cx.v3("point_2_world")) };
            let d = crate::nodes10::sub3(b, a);
            let l2 = crate::nodes10::dot3(d, d).max(1e-8);
            let con = cx.f("contrast").clamp(0.0, 1.0);
            Tex::from_fn(s, hs, |u, v| {
                let p = pos.map(|t| { let q = t.sample(u, v); [q[0], q[1], q[2]] }).unwrap_or([u, 1.0 - v, 0.5]);
                let t = (crate::nodes10::dot3(crate::nodes10::sub3(p, a), d) / l2).clamp(0.0, 1.0);
                let t = smooth(0.5 * con, 1.0 - 0.5 * con, t);
                grey(t)
            })
        }
        "x_3d_volume_mask" => {
            let pos = cx.img("position");
            let centre = if cx.i("position_input") == 0 {
                let uv = cx.v2("position_uv");
                pos.map(|p| { let q = p.sample(uv[0], uv[1]); [q[0], q[1], q[2]] }).unwrap_or([uv[0], 1.0 - uv[1], 0.5])
            } else {
                cx.v3("position_worldspace")
            };
            let size = cx.v3("size");
            let sc = cx.f("scale").max(1e-4);
            let half = [size[0] * sc * 0.5, size[1] * sc * 0.5, size[2] * sc * 0.5];
            let r = cx.v3("rotation");
            let shape = cx.i("shape");
            let feather = cx.f("feather_width").clamp(0.0, 1.0);
            let (sx, cxr) = (r[0] * TAU).sin_cos();
            let (sy, cy) = (r[1] * TAU).sin_cos();
            let (sz, cz) = (r[2] * TAU).sin_cos();
            Tex::from_fn(s, hs, |u, v| {
                let p = pos.map(|t| { let q = t.sample(u, v); [q[0], q[1], q[2]] }).unwrap_or([u, 1.0 - v, 0.5]);
                let mut d = crate::nodes10::sub3(p, centre);
                // undo the volume's rotation (z, y, x)
                d = [d[0] * cz + d[1] * sz, -d[0] * sz + d[1] * cz, d[2]];
                d = [d[0] * cy - d[2] * sy, d[1], d[0] * sy + d[2] * cy];
                d = [d[0], d[1] * cxr + d[2] * sx, -d[1] * sx + d[2] * cxr];
                let q = [d[0] / half[0].max(1e-5), d[1] / half[1].max(1e-5), d[2] / half[2].max(1e-5)];
                // a normalised distance: 1 at the volume's surface
                let k = match shape {
                    1 => (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt(),
                    2 => (q[0] * q[0] + q[2] * q[2]).sqrt().max(q[1].abs()),
                    _ => q[0].abs().max(q[1].abs()).max(q[2].abs()),
                };
                grey(1.0 - smooth(1.0 - feather, 1.0 + 1e-4, k))
            })
        }
        // ---------------------------------------------------------------- PBR utilities
        "x_ao_cancel" => {
            let c = inp(cx, "color");
            let ao = cx.img_or_ref("AO", grey(1.0));
            let (amount, sat) = (cx.f("ao_cancellation").clamp(0.0, 1.0), cx.f("sat").clamp(0.0, 1.0));
            Tex::from_fn(s, hs, |u, v| {
                let p = c.sample(u, v);
                let a = g(&ao, u, v).clamp(0.05, 1.0);
                // dividing by the occlusion brings the darkened crevices back up
                let k = lerp(1.0, 1.0 / a, amount);
                let l = lum(p);
                let mut o = [p[0] * k, p[1] * k, p[2] * k, p[3]];
                // occluded colour also loses saturation; give some back where the cancellation acted
                let ls = lum(o);
                let boost = 1.0 + sat * (k - 1.0).max(0.0) * 0.5;
                for ch in 0..3 {
                    o[ch] = (ls + (o[ch] - ls) * boost).max(0.0);
                }
                let _ = l;
                o
            })
        }
        "x_apply_color_palette" => {
            let id = inp(cx, "id");
            let pal = cx.img_or_ref("palette", grey(1.0));
            // the palette is a row of colour swatches; the id picks one (id 0..1 across the row)
            id.map(|p| {
                let k = p[0].clamp(0.0, 1.0);
                pal.sample(k * (1.0 - 1.0 / pal.w as f32) + 0.5 / pal.w as f32, 0.5)
            })
        }
        "x_create_color_palette_16colors" => {
            let n = cx.i("color_amount").clamp(1, 16) as usize;
            let inpal = cx.img("palette");
            let base = cx.i("palette_color_amount").max(0) as usize;
            let cols: Vec<[f32; 4]> = (1..=n).map(|i| cx.col(&format!("color_{i}"))).collect();
            // our palette: one horizontal band per colour, prefixed by the colours of an input palette
            let mut all: Vec<[f32; 4]> = Vec::new();
            if let Some(p) = inpal {
                let k = base.clamp(0, 64);
                for i in 0..k {
                    all.push(p.sample((i as f32 + 0.5) / k.max(1) as f32, 0.5));
                }
            }
            all.extend(cols);
            let m = all.len().max(1);
            let pal = Tex::from_fn(s, hs, |u, _| all[((u * m as f32) as usize).min(m - 1)]);
            cx.pick(vec![Some(pal), Some(Tex::solid(s, hs, grey(m as f32 / 255.0)))])
        }
        "x_modify_color_palette" => {
            let id = inp(cx, "id");
            let pal = cx.img_or_ref("palette", grey(1.0));
            let mode = cx.i("color_selection_mode");
            let sel_idx = cx.i("color_index").max(0) as f32;
            let pick = cx.v2("color_position");
            let spread = cx.f("color_selection_spread");
            let opacity = cx.f("opacity").clamp(0.0, 1.0);
            let over = cx.col("override_color");
            let hsl = cx.v3("hsl");
            let blend_mode = cx.i("mode");
            let pw = pal.w.max(1) as f32;
            let target = if mode == 1 { id.sample(pick[0], pick[1])[0] } else { (sel_idx + 0.5) / pw };
            let new_pal = Tex::from_fn(s, hs, |u, v| {
                let c = pal.sample(u, v);
                let d = (u - target).abs();
                let k = (1.0 - smooth(spread * 0.5, spread * 0.5 + 1.0 / pw, d)) * opacity;
                if k <= 0.0 {
                    return c;
                }
                let m = if blend_mode == 0 {
                    over
                } else {
                    let (h, sat, val) = ops::rgb_to_hsv(c[0], c[1], c[2]);
                    let (r, gg, b) = ops::hsv_to_rgb((h + hsl[0]).rem_euclid(1.0), (sat + hsl[1]).clamp(0.0, 1.0), (val + hsl[2]).clamp(0.0, 1.0));
                    [r, gg, b, c[3]]
                };
                lerp4(c, m, k)
            });
            let out = id.map(|p| new_pal.sample(p[0].clamp(0.0, 1.0) * (1.0 - 1.0 / pw) + 0.5 / pw, 0.5));
            cx.pick(vec![Some(out), Some(new_pal)])
        }
        // ---------------------------------------------------------------- height tools
        "x_height_extrude" => {
            let h = inp(cx, "height");
            let view = cx.v2("view_angle");
            let sc = cx.f("scale").max(1e-3);
            let hk = cx.f("height_scale");
            let voff = cx.f("voffset");
            let terrain = cx.b("terrain_mode");
            let inv_dy = cx.b("inversedy");
            let ni = cx.f("normal_intensity") / 128.0;
            // an oblique view: the camera looks down at `view[1]` (0 flat .. 1 straight down) and is turned by
            // `view[0]`; every column is drawn from the far end so near surfaces cover far ones
            let tilt = (view[1].clamp(0.05, 1.0) * PI * 0.5).sin();
            let lift = (1.0 - tilt * tilt).sqrt();
            let turn = (view[0] - 0.5) * TAU;
            let (st, ct) = turn.sin_cos();
            let nrm = ops::normal_from_height(&h, 2.0 * ni.max(0.0), false);
            let w = s;
            let mut depth = vec![0.0f32; w * hs];
            let mut norm_px = vec![[0.5f32, 0.5, 1.0, 1.0]; w * hs];
            let rows = hs * 2;
            for x in 0..w {
                let mut top = hs as f32;
                for k in (0..rows).rev() {
                    // walk from near (bottom) to far (top) in ground coordinates
                    let gv = k as f32 / rows as f32;
                    let gu = (x as f32 + 0.5) / w as f32;
                    let (bu, bv) = ((gu - 0.5) / sc, (gv - 0.5) / sc);
                    let (wu, wv) = (bu * ct - bv * st + 0.5, bu * st + bv * ct + 0.5);
                    if terrain && !(0.0..=1.0).contains(&wu) || terrain && !(0.0..=1.0).contains(&wv) {
                        continue;
                    }
                    let hv = g(&h, wu, wv);
                    let y = (gv * tilt - hv * hk * lift * 0.5 + voff + (1.0 - tilt) * 0.5) * hs as f32;
                    let yi = y.round() as i64;
                    if (y as f32) < top {
                        let from = yi.max(0) as usize;
                        let to = (top as usize).min(hs);
                        for yy in from..to {
                            let i = yy * w + x;
                            depth[i] = if inv_dy { 1.0 - gv } else { gv };
                            norm_px[i] = nrm.sample(wu, wv);
                        }
                        top = y.max(0.0);
                    }
                }
            }
            let d = Tex { w, h: hs, px: depth.into_iter().map(grey).collect() };
            let n = Tex { w, h: hs, px: norm_px };
            cx.pick(vec![Some(d), Some(n)])
        }
        "x_height_map_frequencies_mapper" => {
            let h = inp(cx, "Height");
            let relief = cx.f("Relief").max(0.0) * rs;
            let low = ops::blur(&h, relief.max(0.5));
            let high = Tex { w: h.w, h: h.h, px: h.px.iter().zip(&low.px).map(|(a, b)| grey((a[0] - b[0] + 0.5).clamp(0.0, 1.0))).collect() };
            cx.pick(vec![Some(low), Some(high)])
        }
        _ => return None,
    })
}

/// Hue (0..1), chroma and luma of a colour.
fn hcl(p: [f32; 4]) -> (f32, f32, f32) {
    let (h, sat, val) = ops::rgb_to_hsv(p[0], p[1], p[2]);
    (h, sat * val, lum(p))
}

/// Generalised Kuwahara: eight sectors around the pixel, stretched along the local edge direction; each sector's
/// mean is weighted by how calm it is.
fn kuwahara(cx: &Cx) -> Tex {
    let t = cx.img_or_ref("input", [0.0, 0.0, 0.0, 1.0]);
    let (s, hs) = (t.w, t.h);
    let rs = cx.rs();
    let radius_map = cx.img("radius_map");
    let r0 = (cx.f("radius").max(0.0) * 3.0 * rs).clamp(1.0, 24.0);
    let q = 2.0 + cx.f("sharpness").clamp(0.25, 1.0) * 14.0;
    let alpha = cx.f("anisotropy").clamp(0.0, 1.0);
    let overlap = cx.f("smoothness").clamp(0.0, 1.0);
    let angle_bias = cx.f("anisotropy_angle") * TAU;
    let angle_map = cx.img("anisotropy_angle_map");
    let amul = cx.f("anisotropy_angle_map_multiplier");
    let dir_in = if cx.b("use_input_direction_map") { cx.img("direction_map") } else { None };
    let keep_alpha = cx.kind.ends_with("grayscale") || !cx.b("ignore_alpha");
    // structure tensor of the brightness, smoothed
    let gray = crate::nodes10::gray_of(&t);
    let (w, h) = (s as i64, hs as i64);
    let mut tensor = Tex::solid(s, hs, [0.0; 4]);
    for y in 0..h {
        for x in 0..w {
            let gx = gray.at(x + 1, y)[0] - gray.at(x - 1, y)[0];
            let gy = gray.at(x, y + 1)[0] - gray.at(x, y - 1)[0];
            tensor.px[(y * w + x) as usize] = [gx * gx, gx * gy, gy * gy, 1.0];
        }
    }
    let tensor = ops::blur(&tensor, (cx.f("tensor_smoothness").clamp(0.0, 1.0) * 6.0 * rs).max(0.6));
    let mut dirs = Tex::solid(s, hs, [0.5, 0.5, 0.0, 1.0]);
    for (i, p) in tensor.px.iter().enumerate() {
        let (e, f, gg) = (p[0], p[1], p[2]);
        let disc = ((e - gg) * (e - gg) + 4.0 * f * f).sqrt();
        let l1 = (e + gg + disc) * 0.5;
        let l2 = (e + gg - disc) * 0.5;
        let (vx, vy) = (l1 - e, -f);
        let len = (vx * vx + vy * vy).sqrt();
        let (tx, ty) = if len > 1e-8 { (vx / len, vy / len) } else { (1.0, 0.0) };
        let aniso = if l1 + l2 > 1e-8 { (l1 - l2) / (l1 + l2) } else { 0.0 };
        dirs.px[i] = [tx * 0.5 + 0.5, ty * 0.5 + 0.5, aniso, 1.0];
    }
    if let Some(d) = dir_in {
        dirs = d.clone();
    }
    let sectors = 8;
    let mut out = t.clone().into_owned();
    let tt = &t;
    let dd = &dirs;
    let res = crate::tex::par_map(s * hs, |i| {
        let (x, y) = ((i % s) as i64, (i / s) as i64);
        let (u, v) = ((x as f32 + 0.5) / s as f32, (y as f32 + 0.5) / hs as f32);
        let d = dd.px[i];
        let mut phi = (d[1] * 2.0 - 1.0).atan2(d[0] * 2.0 - 1.0) + angle_bias;
        if let Some(am) = angle_map {
            phi += g(am, u, v) * amul * TAU;
        }
        let a = d[2] * alpha;
        let r = r0 * radius_map.map(|m| g(m, u, v).max(0.05)).unwrap_or(1.0);
        let (sa, ca) = phi.sin_cos();
        // an ellipse: long along the edge, short across it
        let (ra, rb) = (r * (1.0 + a), r / (1.0 + a));
        let ri = r.ceil() as i64 * 2;
        let mut m = [[0.0f32; 4]; 8];
        let mut m2 = [0.0f32; 8];
        let mut wsum = [0.0f32; 8];
        for dy in -ri..=ri {
            for dx in -ri..=ri {
                let (fx, fy) = (dx as f32, dy as f32);
                let (lx, ly) = ((fx * ca + fy * sa) / ra.max(0.5), (-fx * sa + fy * ca) / rb.max(0.5));
                let rr = lx * lx + ly * ly;
                if rr > 1.0 {
                    continue;
                }
                let p = tt.at(x + dx, y + dy);
                let l = lum(p);
                let ang = ly.atan2(lx).rem_euclid(TAU);
                for k in 0..sectors {
                    let c = (k as f32 + 0.5) * TAU / sectors as f32;
                    let da = ((ang - c + PI).rem_euclid(TAU) - PI).abs();
                    let wa = (1.0 - da / (PI / sectors as f32 * (1.0 + overlap * 2.0))).clamp(0.0, 1.0);
                    if wa <= 0.0 {
                        continue;
                    }
                    let wt = wa * (-rr * 2.0).exp();
                    for ch in 0..4 {
                        m[k][ch] += p[ch] * wt;
                    }
                    m2[k] += l * l * wt;
                    wsum[k] += wt;
                }
            }
        }
        let mut acc = [0.0f32; 4];
        let mut tot = 0.0;
        for k in 0..sectors {
            if wsum[k] <= 0.0 {
                continue;
            }
            let mean = [m[k][0] / wsum[k], m[k][1] / wsum[k], m[k][2] / wsum[k], m[k][3] / wsum[k]];
            let ml = lum(mean);
            let var = (m2[k] / wsum[k] - ml * ml).max(0.0);
            let wv = 1.0 / (1.0 + (var * 255.0).powf(q * 0.5));
            for ch in 0..4 {
                acc[ch] += mean[ch] * wv;
            }
            tot += wv;
        }
        if tot <= 0.0 {
            tt.px[i]
        } else {
            let a = if keep_alpha { tt.px[i][3] } else { acc[3] / tot };
            [acc[0] / tot, acc[1] / tot, acc[2] / tot, a]
        }
    });
    out.px = res;
    cx.pick(vec![Some(out), Some(dirs)])
}

/// The clone-patch tools' source and target areas.
struct Patch {
    src_c: [f32; 2],
    src_m: [f32; 4],
    tgt_c: [f32; 2],
    tgt_m: [f32; 4],
    shape: i32,
    soft: f32,
    show: bool,
}

impl Patch {
    fn new(cx: &Cx) -> Patch {
        let so = cx.v2("src_offset");
        let to = cx.v2("tgt_offset");
        Patch {
            src_c: [0.5 + so[0] * 0.5, 0.5 + so[1] * 0.5],
            src_m: cx.v4("src_matrix"),
            tgt_c: [0.5 + to[0] * 0.5, 0.5 + to[1] * 0.5],
            tgt_m: cx.v4("tgt_matrix"),
            shape: cx.i("shape"),
            soft: (cx.f("edge_blur") + cx.f("edge_smoothness")) * 0.05,
            show: cx.b("show_patch_areas"),
        }
    }
    /// Where (u, v) lies in the target area (-1..1 inside) and how strongly it is patched.
    fn local(&self, m: [f32; 4], c: [f32; 2], u: f32, v: f32) -> ([f32; 2], f32) {
        let (x, y) = (crate::ops::wrapd(u - c[0]), crate::ops::wrapd(v - c[1]));
        let l = [(m[0] * x + m[1] * y) * 2.0, (m[2] * x + m[3] * y) * 2.0];
        let d = match self.shape {
            0 => l[0].abs().max(l[1].abs()),
            1 => l[0].abs() + l[1].abs(),
            _ => (l[0] * l[0] + l[1] * l[1]).sqrt(),
        };
        (l, 1.0 - smooth(1.0 - self.soft - 0.02, 1.0, d))
    }
    fn src_of(&self, l: [f32; 2]) -> (f32, f32) {
        let m = self.src_m;
        let det = m[0] * m[3] - m[1] * m[2];
        let det = if det.abs() < 1e-6 { 1e-6 } else { det };
        let (x, y) = (l[0] * 0.5, l[1] * 0.5);
        ((m[3] * x - m[1] * y) / det + self.src_c[0], (-m[2] * x + m[0] * y) / det + self.src_c[1])
    }
    fn apply(&self, t: &Tex, s: usize, hs: usize) -> Tex {
        Tex::from_fn(s, hs, |u, v| {
            let base = t.sample(u, v);
            let (l, k) = self.local(self.tgt_m, self.tgt_c, u, v);
            if k <= 0.0 {
                return base;
            }
            let (su, sv) = self.src_of(l);
            lerp4(base, t.sample(su, sv), k)
        })
    }
    fn areas(&self, t: &Tex, s: usize, hs: usize) -> Tex {
        let patched = self.apply(t, s, hs);
        if !self.show {
            return patched;
        }
        Tex::from_fn(s, hs, |u, v| {
            let p = patched.sample(u, v);
            let (_, kt) = self.local(self.tgt_m, self.tgt_c, u, v);
            let (_, ks) = self.local(self.src_m, self.src_c, u, v);
            // target outlined in green, source in red
            let edge = |k: f32| (k > 0.05 && k < 0.6) as i32 as f32;
            let (et, es) = (edge(kt), edge(ks));
            [lerp(lerp(p[0], 1.0, es), 0.0, et), lerp(lerp(p[1], 0.0, es), 1.0, et), lerp(p[2], 0.0, es.max(et)), 1.0]
        })
    }
}

/// Flood Fill Mapper: every flood-filled region gets one of the pattern inputs, fitted to its bounding box.
fn floodfill_mapper(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let bbox = cx.img_or_ref("bbox", [0.0, 0.0, 0.0, 1.0]);
    let gray = cx.kind.ends_with("grayscale");
    let n = cx.i("pattern_input_number").clamp(1, 8) as usize;
    let pats: Vec<Option<&Tex>> = (1..=n).map(|i| if gray && i == 1 { cx.img("pattern_input") } else { cx.img(&format!("pattern_input_{i}")) }).collect();
    let (label, stats) = ops::flood_regions(&bbox);
    let seed = cx.seed;
    let dist_mode = cx.i("pattern_distribution_mode");
    let jitter = cx.f("pattern_distribution_jittering");
    let dist_map = cx.img("pattern_distribution_map");
    let scale = cx.f("scale");
    let scale_rand = cx.f("scale_random");
    let scale_map = cx.img("scale_map");
    let smul = cx.f("scale_map_multiplier");
    let rotation = cx.f("rotation");
    let rot_rand = cx.f("rotation_random");
    let rot_map = cx.img("rotation_map");
    let rmul = cx.f("rotation_map_multiplier");
    let off = cx.v2("position_offset");
    let off_rand = cx.v2("position_offset_random");
    let lum_map = cx.img("luminance_offset_map");
    let bgc = cx.col("background_color");
    let (lrange, lrange_rand, loff, loff_rand, lmul) = if gray {
        (cx.f("luminance_range"), cx.f("luminance_range_random"), cx.f("luminance_offset"), cx.f("luminance_offset_random"), cx.f("luminance_offset_map_multiplier"))
    } else {
        (1.0, 0.0, 0.0, 0.0, 0.0)
    };
    let (hsl_adj, hsl_rand, a_adj, a_rand) = if gray { ([0.0; 3], [0.0; 3], 0.0, 0.0) } else { (cx.v3("hsl_adjustement"), cx.v3("hsl_random"), cx.f("alpha_adjustment"), cx.f("alpha_random")) };
    let size = cx.v2("size");
    let w = bbox.w;
    let px: Vec<[f32; 4]> = (0..s * hs)
        .map(|i| {
            let (x, y) = (i % s, i / s);
            let bi = (y * bbox.h / hs) * w + (x * w / s);
            let l = label.get(bi).copied().unwrap_or(0);
            if l == 0 {
                return bgc;
            }
            let st = &stats[l as usize];
            let id = l as i32;
            let r = |k: i32| rnd(id, k, seed.wrapping_add(4001));
            let (cu, cv) = (st.0[0], st.0[1]);
            let pick = match dist_mode {
                0 => (r(1) * n as f32) as usize,
                1 => (id as usize).wrapping_sub(1) % n,
                _ => (dist_map.map(|m| g(m, cu, cv)).unwrap_or(0.0) * n as f32 + (r(2) - 0.5) * jitter * n as f32) as usize,
            }
            .min(n - 1);
            let Some(pat) = pats[pick] else { return bgc };
            let p = bbox.px[bi];
            // local coordinates inside the region's bounding box come with the flood fill
            let (mut lu, mut lv) = (p[1] - 0.5, p[2] - 0.5);
            let sc = (scale * (1.0 + (r(3) - 0.5) * 2.0 * scale_rand) * scale_map.map(|m| lerp(1.0, g(m, cu, cv), smul)).unwrap_or(1.0)).max(1e-3);
            let a = rotation + (r(4) - 0.5) * rot_rand + rot_map.map(|m| g(m, cu, cv) * rmul).unwrap_or(0.0);
            let (sa, ca) = (a * TAU).sin_cos();
            let (ru, rv) = (lu * ca + lv * sa, -lu * sa + lv * ca);
            lu = ru / (sc * size[0].max(1e-3)) + 0.5 - off[0] - (r(5) - 0.5) * off_rand[0];
            lv = rv / (sc * size[1].max(1e-3)) + 0.5 - off[1] - (r(6) - 0.5) * off_rand[1];
            if !(0.0..=1.0).contains(&lu) || !(0.0..=1.0).contains(&lv) {
                return bgc;
            }
            let mut c = pat.sample(lu, lv);
            if gray {
                let range = (lrange * (1.0 - r(7) * lrange_rand)).clamp(0.0, 1.0);
                let o = loff + (r(8) - 0.5) * 2.0 * loff_rand + lum_map.map(|m| (g(m, cu, cv) - 0.5) * lmul).unwrap_or(0.0);
                let gv = (c[0] * range + o).clamp(0.0, 1.0);
                c = [gv, gv, gv, 1.0];
            } else {
                let (h, sat, val) = ops::rgb_to_hsv(c[0], c[1], c[2]);
                let (rr, gg, bb) = ops::hsv_to_rgb(
                    (h + hsl_adj[0] * 0.5 + (r(9) - 0.5) * hsl_rand[0]).rem_euclid(1.0),
                    (sat + hsl_adj[1] + (r(10) - 0.5) * hsl_rand[1]).clamp(0.0, 1.0),
                    (val + hsl_adj[2] + (r(11) - 0.5) * hsl_rand[2]).clamp(0.0, 1.0),
                );
                c = [rr, gg, bb, (c[3] + a_adj + (r(12) - 0.5) * a_rand).clamp(0.0, 1.0)];
            }
            c
        })
        .collect();
    Tex { w: s, h: hs, px }
}

/// The settings of the Shadows nodes (`x_rt_shadows`, and the older `x_shadows`) for a picture of `s` x `hs` pixels. The
/// CPU code above and the GPU kernel both run on exactly these values.
pub fn shadow_params(cx: &Cx, s: usize, hs: usize) -> crate::gpu::ShadowParams {
    let rt = cx.kind == "x_rt_shadows";
    let (height_scale, lp, opacity, samples, soft, maxlen, invert) = if rt {
        (cx.f("height_scale"), cx.v2("light_position"), cx.f("shadows_opacity"), cx.i("samples").clamp(1, 64), cx.f("light_scale") + 0.02 * cx.lane("light_size", 0), cx.f("max_shadow_length"), cx.b("invert"))
    } else {
        // older node: a light angle (turns, from the horizon) and a shadow distance
        let a = cx.f("light_angle") * TAU;
        (cx.f("shadow_distance") * 0.5, [0.5 + 0.4 * a.cos(), 0.5 - 0.4 * a.sin()], 1.0, cx.i("Samples").clamp(1, 16) * 4, cx.f("edges_softness") * 0.05, 1.0, false)
    };
    // shadows fade with the distance to what casts them; the light's intensity scales the lit result
    let (atten, light_k) = if rt { (cx.f("shadows_attenuation").clamp(0.0, 1.0), cx.f("light_intensity").max(0.0)) } else { (0.0, 1.0) };
    let dir = [lp[0] - 0.5, lp[1] - 0.5];
    let dl = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt().max(1e-4);
    let dir = [dir[0] / dl, dir[1] / dl];
    // the light's elevation: lights near the centre are high, at the border low
    let elev = (1.0 - (dl * 2.0).min(1.0)) * 1.2 + 0.08;
    let len = maxlen.clamp(0.0, 1.0) * 0.5;
    // about one step per pixel along the ray (bounded)
    let steps = ((len * s.max(hs) as f32).ceil() as usize).clamp(16, 192) as u32;
    crate::gpu::ShadowParams { steps, taps: samples.clamp(1, 8) as u32, height_scale, atten, light_k, opacity, soft, len, elev, invert, dir }
}
