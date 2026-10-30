// SPDX-License-Identifier: MIT OR Apache-2.0
//! Library graphs implemented natively (see `nodes10`): the older mesh-based material adjustments (each paints a
//! colour or an image over its input through a mask made from baked maps), the older weathering graphs, the layer
//! composite, the material transforms and the conversions to the specular / glossiness workflow.
//!
//! The library facts name most inputs of the older graphs only `Input_N`, and the public documentation covers their
//! modern mask generator successors, not these graphs. Which baked map each input is taken to be is written next to
//! the code that reads it; the masks follow the documented behaviour of those successors (level, contrast, curvature,
//! occlusion, world-space normal), the algorithms are MaterialCraft's own.
use crate::nodes10::{curvature_of, dec_n, enc_n, g, grey, is_blank, lerp, lerp4, level_contrast, noise, smooth, to_lin, to_srgb, Cx};
use crate::nodes10_mat::{emit, Mat};
use crate::ops;
use crate::tex::{lum, Tex};
use std::f32::consts::TAU;

pub fn compute(cx: &Cx) -> Option<Tex> {
    Some(match cx.kind {
        "x_directionnal_leaks" | "x_edge_blur" | "x_edge_dirt" | "x_edge_spatter" | "x_occlusion" | "x_occlusion_noise" | "x_random_dirt" | "x_selective_dirt" | "x_sun_bleach" => adjustment(cx),
        "x_metal_weathering" | "x_wood_weathering" => surface_weathering(cx),
        "x_moss_weahtering" => moss(cx),
        "x_material_composite" => composite(cx),
        "x_material_transforms" => transforms(cx),
        "x_basecolor_metallic_roughness_to_diffuse_specular_glossiness" | "x_diffuse_metallic_roughness_to_diffuse_specular_glossiness" => to_specular_glossiness(cx),
        _ => return None,
    })
}

/// A connected baked map (a uniform black fill counts as not connected, see [`is_blank`]).
fn baked<'a>(cx: &Cx<'a>, id: &str) -> Option<&'a Tex> {
    cx.img(id).filter(|t| !is_blank(t))
}

fn convex(c: f32) -> f32 {
    ((c - 0.5) * 2.0).clamp(0.0, 1.0)
}

fn concave(c: f32) -> f32 {
    ((0.5 - c) * 2.0).clamp(0.0, 1.0)
}

/// Fractal noise from `2^lo` to `2^hi` cells across: each level weighs `roughness` times the one before.
fn fractal(u: f32, v: f32, (lo, hi): (u32, u32), roughness: f32, seed: u32) -> f32 {
    let (mut sum, mut total, mut amp) = (0.0f32, 0.0f32, 1.0f32);
    for level in lo..=hi {
        sum += noise(u, v, 1 << level, 1, seed.wrapping_add(level * 97)) * amp;
        total += amp;
        amp *= roughness;
    }
    // many levels gather around the middle: spread them over the range again
    let spread = (1.0 + 0.25 * (hi - lo) as f32).min(2.5);
    ((sum / total.max(1e-6) - 0.5) * spread + 0.5).clamp(0.0, 1.0)
}

/// The levels of the `noise_minlevel` / `noise_maxlevel` settings; levels finer than the picture can show are left out.
fn levels(cx: &Cx) -> (u32, u32) {
    let (a, b) = (cx.i("noise_minlevel").clamp(1, 12) as u32, cx.i("noise_maxlevel").clamp(1, 12) as u32);
    let finest = (cx.s.min(cx.hs).max(4) as f32).log2().floor() as u32 - 1;
    (a.min(b).min(finest), a.max(b).min(finest))
}

/// The older adjustments: `Input_2` under a colour or an image, through a mask.
fn adjustment(cx: &Cx) -> Tex {
    let (s, hs, rs, seed, k) = (cx.s, cx.hs, cx.rs(), cx.seed, cx.kind);
    let base = cx.img_or_ref("Input_2", grey(0.5));
    if !cx.b("enable") {
        return base.into_owned();
    }
    // the inputs each graph adds to `Input_2`
    let (image, mask_in) = match k {
        "x_directionnal_leaks" => (cx.img("Input").filter(|_| cx.b("dirt_map_input")), cx.img("Input_3")),
        "x_edge_blur" | "x_edge_spatter" => (cx.img("Input_4").filter(|_| cx.b("image_input")), cx.img("Input_3")),
        "x_edge_dirt" => (cx.img("Input").filter(|_| cx.b("image_input")), cx.img("Input_3")),
        "x_selective_dirt" => (cx.img("Input").filter(|_| cx.b("image_input")), cx.img("Input_4")),
        // its only other input: the dirt's pattern when `image_input` is on (below), else a mask over the generated one
        "x_random_dirt" => (None, cx.img("Input_3").filter(|_| !cx.b("image_input"))),
        _ => (None, cx.img("Input_3")),
    };
    // `Input_1` is the curvature of the edge graphs and the occlusion of the others
    let baked1 = baked(cx, "Input_1");
    let (flat, open) = (Tex::solid(s, hs, grey(0.5)), Tex::solid(s, hs, grey(1.0)));
    let (curv, ao) = (baked1.unwrap_or(&flat), baked1.unwrap_or(&open));
    let contrast = cx.f(if cx.lib.input("Contrast").is_some_and(|i| i.id == "Contrast") { "Contrast" } else { "contrast" });
    let level = cx.f("level");
    let lc = |x: f32| level_contrast(x, level, contrast);
    let shape: Tex = match k {
        "x_edge_blur" => {
            let c = ops::blur(curv, cx.f("blur_radius") * 2.0 * rs);
            Tex::from_fn(s, hs, |u, v| grey(lc(convex(g(&c, u, v)))))
        }
        // dirt gathers in the concave edges; the selective graph keeps to the convex ones, as its successor's page says
        "x_edge_dirt" | "x_selective_dirt" => {
            let pick: fn(f32) -> f32 = if k == "x_edge_dirt" { concave } else { convex };
            let amount = cx.f("amount");
            Tex::from_fn(s, hs, |u, v| {
                let breakup = lerp(1.0, noise(u, v, 8, 4, seed.wrapping_add(29)) * 2.0, amount);
                grey(lc((pick(g(curv, u, v)) * breakup).clamp(0.0, 1.0)))
            })
        }
        // specks that thin out with the distance from the edges
        "x_edge_spatter" => {
            let c = ops::blur(curv, cx.f("radius") * 2.0 * rs);
            Tex::from_fn(s, hs, |u, v| {
                let e = (convex(g(&c, u, v)) * 4.0).min(1.0);
                grey(lc(smooth(1.0 - e * 0.7, 1.05 - e * 0.7, noise(u, v, 48, 2, seed.wrapping_add(37)))))
            })
        }
        "x_occlusion" => {
            let inv = cx.b("invert_ao");
            Tex::from_fn(s, hs, |u, v| {
                let a = g(ao, u, v);
                grey(lc(if inv { a } else { 1.0 - a }))
            })
        }
        "x_occlusion_noise" => {
            let (inv, ao_level, ao_contrast, r, lv) = (cx.b("invert_ao"), cx.f("ao_level"), cx.f("ao_contrast"), cx.f("noise_roughness"), levels(cx));
            Tex::from_fn(s, hs, |u, v| {
                let a = g(ao, u, v);
                let occ = level_contrast(if inv { a } else { 1.0 - a }, ao_level, ao_contrast);
                // the noise breaks the occlusion up around its own value, it does not dim it
                grey(lc((occ * fractal(u, v, lv, r, seed) * 2.0).min(1.0)))
            })
        }
        "x_random_dirt" => {
            let (r, lv, warp) = (cx.f("noise_roughness"), levels(cx), cx.f("noise_distortion") * 0.25);
            let pattern = cx.img("Input_3").filter(|_| cx.b("image_input"));
            Tex::from_fn(s, hs, |u, v| {
                let (wu, wv) = (u + (noise(u, v, 3, 2, seed.wrapping_add(1)) - 0.5) * warp, v + (noise(u, v, 3, 2, seed.wrapping_add(2)) - 0.5) * warp);
                grey(lc(pattern.map(|t| g(t, wu, wv)).unwrap_or_else(|| fractal(wu, wv, lv, r, seed))))
            })
        }
        "x_sun_bleach" => {
            // `Input` is the world-space normal
            let nws = cx.img("Input");
            let a = ops::blur(ao, cx.f("ao_radius") * 16.0 * rs);
            Tex::from_fn(s, hs, |u, v| {
                let n = nws.map(|t| dec_n(t.sample(u, v), false)).unwrap_or([0.0, 0.5, 0.8]);
                grey(lc((n[1] * 0.7 + n[2] * 0.3).max(0.0) * g(&a, u, v)))
            })
        }
        _ => leaks(cx, baked1, &lc),
    };
    let (color, opacity) = (cx.col("color"), cx.f("opacity").clamp(0.0, 1.0));
    Tex::from_fn(s, hs, |u, v| {
        let b = base.sample(u, v);
        let p = image.map(|t| t.sample(u, v)).unwrap_or(color);
        let k = shape.sample(u, v)[0] * opacity * mask_in.map(|t| g(t, u, v)).unwrap_or(1.0);
        [lerp(b[0], p[0], k), lerp(b[1], p[1], k), lerp(b[2], p[2], k), b[3]]
    })
}

/// Streaks running from the edges along `angle`: the sources are smeared over their length by halving steps, which
/// read across the picture's borders like every other sample, so the streaks wrap around.
fn leaks(cx: &Cx, curv: Option<&Tex>, lc: &(dyn Fn(f32) -> f32 + Sync)) -> Tex {
    let (s, hs, seed) = (cx.s, cx.hs, cx.seed);
    // `Input_5` occlusion, `Input_6` position, `Input_4` world-space normal
    let (ao, pos, nws) = (baked(cx, "Input_5"), baked(cx, "Input_6"), baked(cx, "Input_4"));
    let amount = cx.f("amount");
    let mut cur = Tex::from_fn(s, hs, |u, v| {
        let c = curv.map(|t| convex(g(t, u, v))).unwrap_or(0.0);
        let cav = ao.map(|t| 1.0 - g(t, u, v)).unwrap_or(0.0);
        let src = if curv.is_none() && ao.is_none() { 0.6 } else { (c + cav * 0.5).min(1.0) };
        let high = pos.map(|t| smooth(0.0, 0.5, t.sample(u, v)[1])).unwrap_or(1.0);
        let gaps = lerp(1.0, smooth(0.45, 0.6, noise(u, v, 64, 2, seed.wrapping_add(61))), amount);
        grey(lc(src * gaps) * high)
    });
    let a = cx.f("angle") * TAU;
    let dir = [a.cos(), -a.sin()];
    let length = cx.f("lenght").max(0.0) * 0.05;
    let bias = cx.f("bias").clamp(0.0, 1.0);
    let finest = 0.75 / s.max(hs) as f32;
    let mut step = length * 0.5;
    while step > finest {
        // every streak tapers off; the bias makes it end sooner
        let fade = (-(1.0 + bias * 5.0) * step / length.max(1e-6)).exp();
        let prev = cur.clone();
        cur = Tex::from_fn(s, hs, |u, v| grey(prev.sample(u, v)[0].max(prev.sample(u - dir[0] * step, v - dir[1] * step)[0] * fade)));
        step *= 0.5;
    }
    match nws {
        // nothing runs down a surface that faces up
        Some(t) => Tex::from_fn(s, hs, |u, v| grey(cur.sample(u, v)[0] * (1.0 - smooth(0.6, 0.95, dec_n(t.sample(u, v), false)[1])))),
        None => cur,
    }
}

/// The metal and wood graphs: damage from the baked maps and from settings of the same meaning in both.
fn surface_weathering(cx: &Cx) -> Tex {
    let (s, hs, rs, seed) = (cx.s, cx.hs, cx.rs(), cx.seed);
    let wood = cx.kind == "x_wood_weathering";
    let m = Mat::read(cx, &["Diffuse", "", "Normal_High_Poly", "Specular", "", "", "", "", "", "Height", "", ""]);
    let h = m.get("height");
    let curv = baked(cx, "Curvature").cloned().unwrap_or_else(|| curvature_of(&h, 3.0 * rs));
    let ao_in = baked(cx, "AO");
    let ao = ao_in.cloned().unwrap_or_else(|| ops::ambient_occlusion(&h, 6.0 * rs, 1.0));
    let nws = cx.img("Normal_Wold_Space");
    let (dust, dirt, wear, edges) = (cx.f("Dust"), cx.f("Dirtiness"), cx.f("Edges_Wearing"), cx.f("Edges_Details"));
    let (scr_scale, scr_warp) = (cx.i("Sharp_Edges_Scratches_Scale").max(1) * 2, cx.f("Sharp_Edges_Scratches_Warp_Intensity") * 0.2);
    // the layer each graph peels off, and what it uncovers
    let (peel, crk_scale, crk_warp, desat, bright) = if wood {
        (cx.f("Varnish_Peeling"), cx.i("Cracks_Scale").max(1), cx.f("Cracks_Warp_Intensity") * 0.2, cx.f("Used_Wood_Desaturation"), cx.f("Used_Wood_Brightness"))
    } else {
        (cx.f("Paint_Peeling"), cx.i("Paint_Cracks_Scale").max(1), cx.f("Paint_Cracks_Warp_Intensity") * 0.2, cx.f("Raw_Metal_Desaturation"), cx.f("Raw_Metal_Brightness"))
    };
    let (age, threshold) = if wood { (cx.f("Age"), cx.f("Age_Threshlod")) } else { (0.0, 0.5) };
    let (rust, rust_peel, verdigris) = if wood { (0.0, 0.0, false) } else { (cx.f("Rust"), cx.f("Rust_Peeling"), cx.i("Rust_Verdigris") != 0) };
    let warped = |u: f32, v: f32, k: f32, salt: u32| (u + (noise(u, v, 5, 2, seed.wrapping_add(salt)) - 0.5) * k, v + (noise(u, v, 5, 2, seed.wrapping_add(salt + 1)) - 0.5) * k);
    // what lies in a cavity
    let cavity = |u: f32, v: f32| (1.0 - g(&ao, u, v)) * 0.8 + concave(g(&curv, u, v)) * 0.6;
    // exposed surface, cracks, rust, age
    let layers = Tex::from_fn(s, hs, |u, v| {
        let (e, cav) = (convex(g(&curv, u, v)), cavity(u, v));
        let (su, sv) = warped(u, v, scr_warp, 3);
        let scratch = smooth(0.82, 0.97, 1.0 - (noise(su, sv, scr_scale, 3, seed.wrapping_add(5)) * 2.0 - 1.0).abs());
        let detail = (noise(u, v, 32, 3, seed.wrapping_add(7)) - 0.5) * edges * 1.2;
        let worn = (smooth(0.25, 0.55, e + detail) + scratch * smooth(0.02, 0.2, e)).min(1.0) * wear;
        let (cu, cv) = warped(u, v, crk_warp, 9);
        let vr = crate::noise::voronoi(cu, cv, crk_scale * 2, crk_scale * 2, seed.wrapping_add(11), 1.0);
        let line = 1.0 - smooth(0.0, 0.06, vr.1 - vr.0);
        let n = noise(cu, cv, crk_scale, 4, seed.wrapping_add(13));
        let t = 1.0 - peel;
        let peeled = smooth(t, t + 0.08, n * 0.8 + line * 0.15 + e * 0.2) * smooth(0.0, 0.1, peel);
        let aged = smooth(threshold - 0.12, threshold + 0.12, noise(u, v, 6, 4, seed.wrapping_add(17)) * 0.75 + cav * 0.35) * age;
        let cracks = line * if wood { aged } else { peeled.max(smooth(0.0, 0.3, peel) * 0.5) };
        let tr = 1.0 - rust;
        let rusted = smooth(tr, tr + 0.12, noise(u, v, 10, 5, seed.wrapping_add(19)) * 0.7 + cav * 0.5) * smooth(0.0, 0.1, rust);
        [(worn + peeled).min(1.0), cracks, rusted, aged]
    });
    // dirt, dust, the flakes of rust that came off
    let grime = Tex::from_fn(s, hs, |u, v| {
        let top = nws.map(|t| dec_n(t.sample(u, v), false)[1].max(0.0)).unwrap_or(0.5);
        let vr = crate::noise::voronoi(u, v, 20, 20, seed.wrapping_add(23), 1.0);
        let flake = if vr.2.fract() < rust_peel { 1.0 } else { 0.0 };
        [(cavity(u, v) * 1.2 + noise(u, v, 12, 4, seed.wrapping_add(29)) * 0.3).min(1.0) * dirt, (top * 0.8 + noise(u, v, 16, 4, seed.wrapping_add(31)) * 0.4).min(1.0) * dust, flake, 1.0]
    });
    let rust_col: [f32; 3] = if verdigris { [0.24, 0.52, 0.44] } else { [0.42, 0.20, 0.08] };
    let pre = cx.f("Pre_Lighting");
    let (normal_k, ao_k) = (cx.f("Substance_Normal_Intensity") * 0.04, cx.f("Substance_AO_Intensity"));
    // how far the damage sinks into or stands out of the surface
    let relief = || {
        Tex::from_fn(s, hs, |u, v| {
            let (a, b) = (layers.sample(u, v), grime.sample(u, v));
            grey((0.5 - a[0] * 0.1 - a[1] * 0.35 + a[2] * (0.15 - 0.3 * b[2]) + b[0] * 0.03 + b[1] * 0.03).clamp(0.0, 1.0))
        })
    };
    let f = |c: &'static str| -> Tex {
        let t = m.get(c);
        match c {
            "normal" => {
                let bump = ops::normal_from_height(&relief(), normal_k, false);
                Tex::from_fn(s, hs, |u, v| {
                    let (a, b) = (dec_n(t.sample(u, v), false), dec_n(bump.sample(u, v), false));
                    enc_n([a[0] + b[0], a[1] + b[1], a[2]], false)
                })
            }
            "ambientocclusion" => {
                let cavities = ops::ambient_occlusion(&relief(), 8.0 * rs, 3.0);
                Tex::from_fn(s, hs, |u, v| grey(ao_in.map(|a| g(a, u, v)).unwrap_or(1.0) * lerp(1.0, g(&cavities, u, v), ao_k)))
            }
            "specular" => Tex::from_fn(s, hs, |u, v| {
                let (a, b, p) = (layers.sample(u, v), grime.sample(u, v), t.sample(u, v));
                // bare metal shines, bare wood does not
                let bare = if wood { lerp4(p, grey(0.12), a[0] * 0.6) } else { lerp4(p, grey(lerp(0.5, 0.95, bright)), a[0]) };
                let mut o = lerp4(bare, grey(0.2), (a[2] + b[0] * 0.7 + b[1] * 0.7).min(1.0));
                o[3] = p[3];
                o
            }),
            "diffuse" => Tex::from_fn(s, hs, |u, v| {
                let (a, b, p) = (layers.sample(u, v), grime.sample(u, v), t.sample(u, v));
                // bare wood is paler than varnished wood
                let (l, k) = (lum(p), bright * 2.0 + if wood { 0.4 } else { 0.0 });
                let used = [lerp(p[0], l, desat) * k, lerp(p[1], l, desat) * k, lerp(p[2], l, desat) * k, p[3]];
                let exposed = lerp4(p, used, (a[0] + a[3]).min(1.0));
                let flaked = 0.8 + 0.4 * b[2];
                let rusted = [rust_col[0] * flaked, rust_col[1] * flaked, rust_col[2] * flaked, p[3]];
                let mut o = lerp4(exposed, rusted, a[2] * (1.0 - b[2] * 0.8));
                for ch in 0..3 {
                    o[ch] *= 1.0 - a[1] * 0.8;
                }
                o = lerp4(o, [0.16, 0.13, 0.10, p[3]], b[0] * 0.85);
                o = lerp4(o, [0.62, 0.58, 0.52, p[3]], b[1] * 0.8);
                let lit = lerp(1.0, g(&ao, u, v) * (1.0 + convex(g(&curv, u, v)) * 0.5), pre);
                [(o[0] * lit).clamp(0.0, 1.0), (o[1] * lit).clamp(0.0, 1.0), (o[2] * lit).clamp(0.0, 1.0), p[3]]
            }),
            _ => t,
        }
    };
    emit(cx, &f, &|_| None)
}

/// Moss over a material of four channels: `Input_2` diffuse, `Input` normal, `Input_6` glossiness or roughness and
/// `Input_3` specular or reflectance; `Input_1` is the height, `Input_4` the occlusion and `Input_5` the mask.
fn moss(cx: &Cx) -> Tex {
    let (s, hs, rs, seed) = (cx.s, cx.hs, cx.rs(), cx.seed);
    let m = Mat::read(cx, &["Input_2", "", "Input", "Input_3", "", "Input_6", "", "", "", "Input_1", "", ""]);
    let h = m.get("height");
    let ao = baked(cx, "Input_4").cloned().unwrap_or_else(|| ops::ambient_occlusion(&h, 6.0 * rs, 1.0));
    let mask = cx.img("Input_5").filter(|_| cx.i("use_mask") != 0);
    let t = 1.0 - cx.f("moss_propagation").clamp(0.0, 1.0);
    let cover = Tex::from_fn(s, hs, |u, v| {
        let score = noise(u, v, 8, 5, seed.wrapping_add(5)) * 0.7 + (1.0 - g(&h, u, v)) * 0.15 + (1.0 - g(&ao, u, v)) * 0.35;
        grey(smooth(t - 0.1, t + 0.1, score) * mask.map(|k| g(k, u, v)).unwrap_or(1.0))
    });
    let tufts = |u: f32, v: f32| noise(u, v, 48, 3, seed.wrapping_add(7));
    let col = cx.col("Moss_Color");
    let f = |c: &'static str| -> Tex {
        let t = m.get(c);
        match c {
            "diffuse" => Tex::from_fn(s, hs, |u, v| {
                let (p, k, shade) = (t.sample(u, v), cover.sample(u, v)[0], 0.6 + tufts(u, v) * 0.8);
                [lerp(p[0], col[0] * shade, k).clamp(0.0, 1.0), lerp(p[1], col[1] * shade, k).clamp(0.0, 1.0), lerp(p[2], col[2] * shade, k).clamp(0.0, 1.0), p[3]]
            }),
            "normal" => {
                let bump = Tex::from_fn(s, hs, |u, v| grey(cover.sample(u, v)[0] * (0.5 + tufts(u, v) * 0.5)));
                let b = ops::normal_from_height(&bump, 0.12, cx.i("normal_format") == 0);
                Tex::from_fn(s, hs, |u, v| {
                    let (x, y) = (dec_n(t.sample(u, v), false), dec_n(b.sample(u, v), false));
                    enc_n([x[0] + y[0], x[1] + y[1], x[2]], false)
                })
            }
            // moss is rough and does not shine: its value depends on what the channel holds
            "glossiness" => {
                let moss_v = if cx.i("roughness_type") == 0 { 0.15 } else { 0.85 };
                Tex::from_fn(s, hs, |u, v| lerp4(t.sample(u, v), grey(moss_v), cover.sample(u, v)[0]))
            }
            "specular" => {
                let moss_v = if cx.i("reflectance_type") == 0 { 0.22 } else { 0.0 };
                Tex::from_fn(s, hs, |u, v| lerp4(t.sample(u, v), grey(moss_v), cover.sample(u, v)[0]))
            }
            _ => t,
        }
    };
    emit(cx, &f, &|o| match o.to_ascii_lowercase().as_str() {
        "glossiness_roughness" => Some(f("glossiness")),
        "specular_reflectance" => Some(f("specular")),
        _ => None,
    })
}

/// Blending modes of the composite, in the library's usual order: Copy, Add, Subtract, Multiply, Add/Sub, Max, Min.
fn blend(x: f32, y: f32, mode: i32) -> f32 {
    match mode {
        1 => x + y,
        2 => x - y,
        3 => x * y,
        4 => x + (y - 0.5) * 2.0,
        5 => x.max(y),
        6 => x.min(y),
        _ => y,
    }
}

/// Layer one over layer zero, channel by channel.
fn composite(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let layer = |n: &str| {
        let id = |c: &str| format!("Layer_{n}_{c}");
        let names = [id("Diffuse"), String::new(), format!("layer{n}Normal"), id("Specular"), id("Emissive"), String::new(), String::new(), String::new(), String::new(), id("Height"), id("Ambient"), id("Opacity")];
        let refs: [&str; 12] = std::array::from_fn(|i| names[i].as_str());
        Mat::read(cx, &refs)
    };
    let (one, zero) = (layer("One"), layer("Zero"));
    let comp = cx.img("Composite_Mask").filter(|_| cx.b("Use_Composite_Mask"));
    let use_opacity = cx.b("Use_Layer_One_Opacity");
    let mask = |u: f32, v: f32| comp.map(|t| g(t, u, v)).unwrap_or(1.0) * if use_opacity { one.at("opacity", u, v)[0] } else { 1.0 };
    let f = |c: &'static str| -> Tex {
        let (a, b) = (zero.get(c), one.get(c));
        let name = match c {
            "ambientocclusion" => "Ambient",
            "diffuse" => "Diffuse",
            "specular" => "Specular",
            "height" => "Height",
            "normal" => "Normal",
            "opacity" => "Opacity",
            _ => "Emissive",
        };
        let amount = cx.f(&format!("{name}_Blending_Amount")).clamp(0.0, 1.0);
        if c == "normal" {
            // 0 replaces the normal below, 1 adds layer one's detail to it
            let (method, strength) = (cx.i("Normal_Method"), cx.f("Normal_Intensity"));
            return Tex::from_fn(s, hs, |u, v| {
                let k = mask(u, v) * amount;
                let (x, y) = (dec_n(a.sample(u, v), false), dec_n(b.sample(u, v), false));
                let (dx, dy) = (y[0] * strength, y[1] * strength);
                enc_n(if method == 0 { [lerp(x[0], dx, k), lerp(x[1], dy, k), lerp(x[2], y[2], k)] } else { [x[0] + dx * k, x[1] + dy * k, x[2] * lerp(1.0, y[2], k)] }, false)
            });
        }
        let mode = cx.i(&format!("{name}_Blending_Mode"));
        let lift = if c == "height" { cx.f("Height_Balance") * 0.5 } else { 0.0 };
        let mut t = Tex::from_fn(s, hs, |u, v| {
            let (x, y, k) = (a.sample(u, v), b.sample(u, v), mask(u, v) * amount);
            let mut o = x;
            for ch in 0..3 {
                o[ch] = lerp(x[ch], blend(x[ch], y[ch] + lift, mode), k).clamp(0.0, 1.0);
            }
            o
        });
        if c == "height" && cx.b("Height_Auto_Level") {
            let (lo, hi) = t.px.iter().fold((f32::MAX, f32::MIN), |(l, h), p| (l.min(p[0]), h.max(p[0])));
            let range = (hi - lo).max(1e-5);
            t = t.map(|p| grey((p[0] - lo) / range));
        }
        t
    };
    emit(cx, &f, &|o| if o.eq_ignore_ascii_case("ambient") { Some(f("ambientocclusion")) } else { None })
}

/// Scale (in percent, 200 repeats the material twice), rotation (in turns) and offset, about the picture's centre.
fn transforms(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let m = Mat::read(cx, &["Diffuse_In", "", "", "Specular_In", "Emissive_In", "", "", "", "", "Height_In", "", "Opacity_In"]);
    let (scale, off) = (cx.v2("ScaleUV"), cx.v2("Offset"));
    let (sa, ca) = (cx.f("Rotation") * TAU).sin_cos();
    let f = |c: &'static str| -> Tex {
        let t = m.get(c);
        Tex::from_fn(s, hs, |u, v| {
            let (x, y) = (u - 0.5 - off[0], v - 0.5 - off[1]);
            t.sample((ca * x + sa * y) * scale[0] * 0.01 + 0.5, (-sa * x + ca * y) * scale[1] * 0.01 + 0.5)
        })
    };
    emit(cx, &f, &|_| None)
}

/// Metallic / roughness to specular / glossiness, as the Basecolor Metallic Roughness Converter node does: colours stay
/// sRGB-encoded and a dielectric reflects 4% (linear). A diffuse input is read the same way as a base colour: metals
/// keep their reflectance colour in it.
fn to_specular_glossiness(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let id = if cx.lib.input("basecolor").is_some() { "basecolor" } else { "diffuse" };
    let (b, r, m) = (cx.img_or_ref(id, grey(0.5)), cx.img_or_ref("roughness", grey(0.5)), cx.img_or_ref("metallic", grey(0.0)));
    let f = |c: &'static str| -> Tex {
        match c {
            "diffuse" => Tex::from_fn(s, hs, |u, v| {
                let (p, k) = (b.sample(u, v), 1.0 - g(&m, u, v));
                [p[0] * k, p[1] * k, p[2] * k, p[3]]
            }),
            "specular" => Tex::from_fn(s, hs, |u, v| {
                let (p, k) = (b.sample(u, v), g(&m, u, v));
                let f0 = |ch: usize| to_srgb(lerp(0.04, to_lin(p[ch]), k));
                [f0(0), f0(1), f0(2), 1.0]
            }),
            _ => Tex::from_fn(s, hs, |u, v| grey(1.0 - g(&r, u, v))),
        }
    };
    emit(cx, &f, &|_| None)
}
