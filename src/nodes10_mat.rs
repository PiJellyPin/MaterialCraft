// SPDX-License-Identifier: MIT OR Apache-2.0
//! Library graphs implemented natively (see `nodes10`): material filters (blends, switches, transforms,
//! adjustments, crops, weathering), mask generators, PBR utilities and renders, atlas tools and MaterialCraft's own
//! versions of the library's material presets.
use crate::nodes10::{curvature_of, dec_n, enc_n, g, grey, is_blank, lerp, lerp4, level_contrast, noise, rnd, smooth, to_lin, to_srgb, Cx};
use crate::ops;
use crate::tex::{lum, Tex};
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

/// The channels a material carries, in the order the library's material nodes list them.
pub const CH: [&str; 12] = ["diffuse", "basecolor", "normal", "specular", "emissive", "glossiness", "roughness", "metallic", "specularlevel", "height", "ambientocclusion", "opacity"];

fn ch_default(ch: &str) -> [f32; 4] {
    match ch {
        "diffuse" | "basecolor" => grey(0.5),
        "normal" => [0.5, 0.5, 1.0, 1.0],
        "specular" => grey(0.22),
        "emissive" | "metallic" => grey(0.0),
        "glossiness" | "roughness" | "specularlevel" | "height" => grey(0.5),
        _ => grey(1.0),
    }
}

fn is_color_ch(ch: &str) -> bool {
    matches!(ch, "diffuse" | "basecolor" | "normal" | "specular" | "emissive")
}

/// A channel name of the library (any spelling) as one of ours.
pub fn canon(name: &str) -> Option<&'static str> {
    let mut n = name.to_ascii_lowercase().replace('_', "");
    while n.ends_with(|c: char| c.is_ascii_digit()) {
        n.pop();
    }
    Some(match n.as_str() {
        "diffuse" => "diffuse",
        "basecolor" => "basecolor",
        "normal" | "normalhighpoly" => "normal",
        "specular" => "specular",
        "emissive" => "emissive",
        "glossiness" => "glossiness",
        "roughness" => "roughness",
        "metallic" => "metallic",
        "specularlevel" => "specularlevel",
        "height" => "height",
        "ambientocclusion" | "ambientoolusion" | "ao" => "ambientocclusion",
        "opacity" => "opacity",
        _ => return None,
    })
}

/// A material: channel -> picture (absent channels read their default).
pub struct Mat {
    pub ch: HashMap<&'static str, Tex>,
    pub w: usize,
    pub h: usize,
}

impl Mat {
    pub fn new(w: usize, h: usize) -> Mat {
        Mat { ch: HashMap::new(), w, h }
    }
    pub fn has(&self, ch: &str) -> bool {
        self.ch.contains_key(ch)
    }
    pub fn get(&self, ch: &'static str) -> Tex {
        self.ch.get(ch).cloned().unwrap_or_else(|| Tex::solid(self.w, self.h, ch_default(ch)))
    }
    pub fn at(&self, ch: &'static str, u: f32, v: f32) -> [f32; 4] {
        match self.ch.get(ch) {
            Some(t) => t.sample(u, v),
            None => ch_default(ch),
        }
    }
    /// Read the channels from inputs named after them (`names` lists the input for each channel of CH).
    pub fn read(cx: &Cx, names: &[&str; 12]) -> Mat {
        let mut m = Mat::new(cx.s, cx.hs);
        for (i, n) in names.iter().enumerate() {
            if n.is_empty() {
                continue;
            }
            if let Some(t) = cx.img(n) {
                m.ch.insert(CH[i], t.clone());
            }
        }
        m
    }
}

/// Input names `prefix + channel` for every channel.
fn names_with(prefix: &str, ao: &str, spec_level: &str) -> [String; 12] {
    CH.map(|c| match c {
        "ambientocclusion" => format!("{prefix}{ao}"),
        "specularlevel" => format!("{prefix}{spec_level}"),
        _ => format!("{prefix}{c}"),
    })
}

fn read_named(cx: &Cx, names: &[String; 12]) -> Mat {
    let refs: [&str; 12] = std::array::from_fn(|i| names[i].as_str());
    Mat::read(cx, &refs)
}

/// The library's outputs, filled from a material (other outputs come from `extra`).
fn emit(cx: &Cx, f: &dyn Fn(&'static str) -> Tex, extra: &dyn Fn(&str) -> Option<Tex>) -> Tex {
    let outs: Vec<&'static str> = if cx.outs.is_empty() { cx.lib.outputs.clone() } else { cx.outs.to_vec() };
    let all: Vec<Option<Tex>> = outs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            if !cx.wants(i) {
                return None;
            }
            match extra(o) {
                Some(t) => Some(t),
                None => Some(match canon(o) {
                    Some(c) => f(c),
                    None => Tex::solid(cx.s, cx.hs, grey(0.0)),
                }),
            }
        })
        .collect();
    cx.pick(all)
}

/// Is channel `ch` switched on by the node's per-channel toggles (named `<prefix><channel>`)?
fn channel_on(cx: &Cx, prefix: &str, ch: &str) -> bool {
    let ids = match ch {
        "ambientocclusion" => vec![format!("{prefix}ambientocclusion"), format!("{prefix}ambient_occlusion")],
        "specularlevel" => vec![format!("{prefix}specularlevel"), format!("{prefix}specular_level")],
        c => vec![format!("{prefix}{c}")],
    };
    for id in ids {
        if cx.lib.input(&id).is_some() {
            return cx.b(&id);
        }
    }
    true
}

fn blend_mode(a: [f32; 4], b: [f32; 4], mode: i32, k: f32) -> [f32; 4] {
    let mut o = [0.0f32; 4];
    for c in 0..3 {
        let (x, y) = (a[c], b[c]);
        let r = match mode {
            1 => x + y,
            2 => x - y,
            3 => x * y,
            4 => x + y - 0.5,
            5 => x.max(y),
            6 => x.min(y),
            9 => {
                if x < 0.5 {
                    2.0 * x * y
                } else {
                    1.0 - 2.0 * (1.0 - x) * (1.0 - y)
                }
            }
            10 => 1.0 - (1.0 - x) * (1.0 - y),
            _ => y,
        };
        o[c] = lerp(x, r, k);
    }
    o[3] = a[3];
    o
}

pub fn compute(cx: &Cx) -> Option<Tex> {
    let (s, hs) = (cx.s, cx.hs);
    let rs = cx.rs();
    if crate::nodes10::preset_material(cx.kind) {
        return Some(preset(cx));
    }
    if cx.kind.starts_with("x_mg_") {
        return Some(mask_generator(cx));
    }
    if cx.kind.ends_with("_weathering") {
        return Some(weathering(cx));
    }
    Some(match cx.kind {
        "x_material_height_blend" | "x_material_switch" => {
            let m1 = read_named(cx, &names_with("material1_", "ambientocclusion", "specularlevel"));
            let m2 = read_named(cx, &names_with("material2_", "ambientocclusion", "specularlevel"));
            if cx.kind == "x_material_switch" {
                let second = cx.b("Switch");
                emit(cx, &|c| if channel_on(cx, "", c) && second { m2.get(c) } else { m1.get(c) }, &|_| None)
            } else {
                let (off, con, opacity) = (cx.f("height_offset"), cx.f("contrast").clamp(0.0, 1.0), cx.f("opacity").clamp(0.0, 1.0));
                let (h1, h2) = (m1.get("height"), m2.get("height"));
                // material 2 shows where it stands higher than material 1 (moved by the offset)
                let w = Tex::from_fn(s, hs, |u, v| {
                    let d = g(&h2, u, v) - g(&h1, u, v) + (off - 0.5) * 2.0;
                    let width = (1.0 - con) * 0.5 + 0.005;
                    grey(smooth(-width, width, d) * opacity)
                });
                let mix = |c: &'static str| {
                    let (a, b) = (m1.get(c), m2.get(c));
                    if !channel_on(cx, "channel_", c) {
                        return a;
                    }
                    Tex::from_fn(s, hs, |u, v| {
                        let k = w.sample(u, v)[0];
                        if c == "normal" {
                            let (na, nb) = (dec_n(a.sample(u, v), false), dec_n(b.sample(u, v), false));
                            enc_n([lerp(na[0], nb[0], k), lerp(na[1], nb[1], k), lerp(na[2], nb[2], k)], false)
                        } else if c == "height" {
                            grey(g(&a, u, v).max(g(&b, u, v) * k + g(&a, u, v) * (1.0 - k)))
                        } else {
                            lerp4(a.sample(u, v), b.sample(u, v), k)
                        }
                    })
                };
                emit(cx, &mix, &|o| if o.eq_ignore_ascii_case("height_mask") { Some(w.clone()) } else { None })
            }
        }
        "x_material_transform" => {
            let names = ["Input_12", "Input_14", "Input_11", "Input_15", "Input_20", "Input_18", "Input_13", "Input_16", "Input_21", "Input_10", "Input_19", "Input_17"];
            let m = Mat::read(cx, &names);
            let mat = cx.v4("matrix22");
            let off = cx.v2("offset");
            let det = mat[0] * mat[3] - mat[1] * mat[2];
            let det = if det.abs() < 1e-6 { 1e-6 } else { det };
            let ang = mat[2].atan2(mat[0]);
            let rot_n = cx.b("normal_scale_and_skew_transformation") || ang.abs() > 1e-4;
            let xf = |t: &Tex, normal: bool| Tex::from_fn(s, hs, |u, v| {
                let (x, y) = (u - 0.5 - off[0], v - 0.5 - off[1]);
                let (su, sv) = ((mat[3] * x - mat[1] * y) / det + 0.5, (-mat[2] * x + mat[0] * y) / det + 0.5);
                let p = t.sample(su, sv);
                if normal && rot_n {
                    let n = dec_n(p, false);
                    let (sa, ca) = ang.sin_cos();
                    enc_n([n[0] * ca + n[1] * sa, -n[0] * sa + n[1] * ca, n[2]], false)
                } else {
                    p
                }
            });
            emit(cx, &|c| if channel_on(cx, "", c) { xf(&m.get(c), c == "normal") } else { m.get(c) }, &|_| None)
        }
        "x_material_adjustment_blend" | "x_material_color_blend" => {
            let m = Mat::read(cx, &["diffuse", "basecolor", "normal", "specular", "emissive", "glossiness", "roughness", "metallic", "specularlevel", "height", "ambientocclusion", "opacity"]);
            let mask_in = cx.img("Input_21");
            let col_src = cx.img("Input_20");
            let color_mask = cx.b("color_mask");
            let (fuzz, target, pad) = (cx.f("fuzziness").max(0.01), cx.col("color"), cx.f("padding"));
            let mask = Tex::from_fn(s, hs, |u, v| {
                let mut k = mask_in.map(|t| g(t, u, v)).unwrap_or(1.0);
                if color_mask {
                    if let Some(c) = col_src {
                        let p = c.sample(u, v);
                        let d = ((p[0] - target[0]).powi(2) + (p[1] - target[1]).powi(2) + (p[2] - target[2]).powi(2)).sqrt();
                        k *= 1.0 - smooth(fuzz * (0.5 + pad), fuzz * (1.0 + pad), d);
                    }
                }
                grey(k.clamp(0.0, 1.0))
            });
            let adjust = cx.kind == "x_material_adjustment_blend";
            let prefix = |c: &str| -> String {
                match c {
                    "ambientocclusion" => "ambient_occlusion".into(),
                    o => o.to_string(),
                }
            };
            let f = |c: &'static str| -> Tex {
                let t = m.get(c);
                if !channel_on(cx, "channel_", c) {
                    return t;
                }
                let p = prefix(c);
                let opacity = cx.f(&format!("{p}_opacity")).clamp(0.0, 1.0);
                let bmode = if cx.lib.input(&format!("{p}_blending_mode")).is_some() { cx.i(&format!("{p}_blending_mode")) } else { cx.i(&format!("{p}_blendingmode")) };
                if c == "normal" {
                    if adjust {
                        let k = cx.f("normal_intensity");
                        return Tex::from_fn(s, hs, |u, v| {
                            let n = dec_n(t.sample(u, v), false);
                            let mk = mask.sample(u, v)[0];
                            let kk = lerp(1.0, k, mk);
                            enc_n([n[0] * kk, n[1] * kk, n[2]], false)
                        });
                    }
                    return t;
                }
                if adjust {
                    let (con, lumi) = (cx.f(&format!("{p}_contrast")), cx.f(&format!("{p}_luminosity")));
                    let (hue, sat) = if is_color_ch(c) { (cx.f(&format!("{p}_hue")) - 0.5, cx.f(&format!("{p}_saturation")) - 0.5) } else { (0.0, 0.0) };
                    Tex::from_fn(s, hs, |u, v| {
                        let a = t.sample(u, v);
                        let mut b = a;
                        if hue.abs() > 1e-4 || sat.abs() > 1e-4 {
                            let (h, sa, va) = ops::rgb_to_hsv(a[0], a[1], a[2]);
                            let (r, gg, bb) = ops::hsv_to_rgb((h + hue).rem_euclid(1.0), (sa * (1.0 + sat * 2.0)).clamp(0.0, 1.0), va);
                            b = [r, gg, bb, a[3]];
                        }
                        for ch in 0..3 {
                            b[ch] = ((b[ch] - 0.5) * (1.0 + con * 2.0).max(0.0) + 0.5 + lumi).clamp(0.0, 1.0);
                        }
                        blend_mode(a, b, 0, opacity * mask.sample(u, v)[0])
                    })
                } else {
                    let col = cx.col(&format!("{p}_color"));
                    Tex::from_fn(s, hs, |u, v| {
                        let a = t.sample(u, v);
                        let mode = match bmode {
                            1 => 3,
                            2 => 1,
                            3 => 2,
                            4 => 5,
                            5 => 6,
                            6 => 9,
                            7 => 10,
                            _ => 0,
                        };
                        blend_mode(a, col, mode, opacity * mask.sample(u, v)[0])
                    })
                }
            };
            emit(cx, &f, &|_| None)
        }
        "x_multi_material_blend" => {
            // the library lists 16 blocks of 12 channel inputs (material 16 first), then the ID map
            let imgs: Vec<&'static str> = cx.lib.images().map(|i| i.id).collect();
            let count = cx.i("Materials").clamp(1, 16) as usize;
            let idmap = imgs.last().and_then(|id| cx.img(id));
            let mut mats: Vec<Mat> = Vec::new();
            for k in 1..=16usize {
                let block = 16 - k;
                let mut m = Mat::new(s, hs);
                for (ci, c) in CH.iter().enumerate() {
                    if let Some(t) = imgs.get(block * 12 + ci).and_then(|id| cx.img(id)) {
                        m.ch.insert(c, t.clone());
                    }
                }
                mats.push(m);
            }
            let opacity = cx.f("Opacity").clamp(0.0, 1.0);
            let colors: Vec<([f32; 4], f32, f32)> = (1..=16).map(|k| if k == 1 { ([0.0; 4], 0.0, 0.0) } else { (cx.col(&format!("Material_{k}_Color")), cx.f(&format!("Material_{k}_Fuzziness")).max(0.005), cx.f(&format!("Material_{k}_Padding"))) }).collect();
            // weight of material k at a pixel: material 1 is the base, others replace it where the ID map has their colour
            let weights = |u: f32, v: f32| -> Vec<f32> {
                let mut w = vec![0.0f32; count];
                let mut rest = 1.0f32;
                if let Some(idm) = idmap {
                    let p = idm.sample(u, v);
                    for k in (1..count).rev() {
                        let (c, fz, pad) = colors[k];
                        let d = ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt();
                        let a = (1.0 - smooth(fz * pad, fz * (1.0 + pad) + 1e-4, d)) * opacity;
                        w[k] = a * rest;
                        rest *= 1.0 - a;
                    }
                }
                w[0] = rest;
                w
            };
            let f = |c: &'static str| -> Tex {
                Tex::from_fn(s, hs, |u, v| {
                    let w = weights(u, v);
                    let mut o = [0.0f32; 4];
                    for (k, wk) in w.iter().enumerate() {
                        if *wk <= 0.0 {
                            continue;
                        }
                        let p = mats[k].at(c, u, v);
                        for ch in 0..4 {
                            o[ch] += p[ch] * wk;
                        }
                    }
                    o
                })
            };
            emit(cx, &f, &|_| None)
        }
        "x_material_selector" => {
            let src = cx.img_or_ref("Input", grey(0.0));
            let n = cx.i("materials").clamp(1, 16);
            let (fuzz, pad) = (cx.f("fuzziness").max(0.01), cx.f("padding"));
            let sel: Vec<[f32; 4]> = (1..=n).filter(|k| cx.b(&format!("material_{k}_enable"))).map(|k| cx.col(&format!("material_{k}_color"))).collect();
            src.map(|p| {
                let mut m = 0.0f32;
                for c in &sel {
                    let d = ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt();
                    m = m.max(1.0 - smooth(fuzz * pad, fuzz * (1.0 + pad) + 1e-4, d));
                }
                grey(m)
            })
        }
        "x_material_mesh_data_blender" => {
            let m = Mat::read(cx, &["diffuse", "basecolor", "normal_2", "specular", "", "glossiness", "roughness", "metallic", "", "Input_10", "ambient_occlusion_2", ""]);
            let ao = cx.img("ambient_occlusion");
            let curv = cx.img("curvature");
            let mesh_h = cx.img("height");
            let mesh_n = cx.img("normal");
            let vc = cx.img("vertex_color");
            let sharp = |u: f32, v: f32| curv.map(|t| ((g(t, u, v) - 0.5) * 2.0).max(0.0)).unwrap_or(0.0);
            let aov = |u: f32, v: f32| ao.map(|t| g(t, u, v)).unwrap_or(1.0);
            let f = |c: &'static str| -> Tex {
                let t = m.get(c);
                match c {
                    "diffuse" | "basecolor" => {
                        let (ka, ks, kv) = if c == "diffuse" { (cx.f("diffuse_ao"), cx.f("diffuse_sharp_edges"), cx.f("diffuse_color_from_vertex_color")) } else { (cx.f("basecolor_ao"), cx.f("basecolor_sharp_edges"), cx.f("basecolor_from_vertex_color")) };
                        Tex::from_fn(s, hs, |u, v| {
                            let mut p = t.sample(u, v);
                            if let Some(vct) = vc {
                                p = lerp4(p, vct.sample(u, v), kv);
                            }
                            let k = lerp(1.0, aov(u, v), ka) * (1.0 + sharp(u, v) * ks);
                            [p[0] * k, p[1] * k, p[2] * k, p[3]]
                        })
                    }
                    "normal" => {
                        let ki = cx.f("normal_material_intensity");
                        match mesh_n {
                            None => t,
                            Some(mn) => Tex::from_fn(s, hs, |u, v| {
                                let (a, b) = (dec_n(mn.sample(u, v), false), dec_n(t.sample(u, v), false));
                                // the material's detail rides on the mesh normal (whiteout blend)
                                enc_n([a[0] + b[0] * ki, a[1] + b[1] * ki, a[2]], false)
                            }),
                        }
                    }
                    "roughness" => {
                        let k = cx.f("roughness_bright_sharp_edges");
                        Tex::from_fn(s, hs, |u, v| grey((g(&t, u, v) + sharp(u, v) * k).clamp(0.0, 1.0)))
                    }
                    "glossiness" => {
                        let k = cx.f("glossiness_dark_sharp_edges");
                        Tex::from_fn(s, hs, |u, v| grey((g(&t, u, v) - sharp(u, v) * k).clamp(0.0, 1.0)))
                    }
                    "metallic" => {
                        let k = cx.f("metallic_bright_sharp_edges");
                        Tex::from_fn(s, hs, |u, v| grey((g(&t, u, v) + sharp(u, v) * k).clamp(0.0, 1.0)))
                    }
                    "specular" => {
                        let (ka, ks) = (cx.f("specular_ao"), cx.f("specular_bright_sharp_edges"));
                        Tex::from_fn(s, hs, |u, v| {
                            let p = t.sample(u, v);
                            let k = lerp(1.0, aov(u, v), ka) + sharp(u, v) * ks;
                            [p[0] * k, p[1] * k, p[2] * k, p[3]]
                        })
                    }
                    "height" => {
                        let k = cx.f("height_material_intensity");
                        let mode = cx.i("height_material_blending_type");
                        match mesh_h {
                            None => t,
                            Some(mh) => Tex::from_fn(s, hs, |u, v| {
                                let (a, b) = (g(mh, u, v), g(&t, u, v));
                                grey(if mode == 1 { a.max(b * k) } else { (a + (b - 0.5) * k).clamp(0.0, 1.0) })
                            }),
                        }
                    }
                    "ambientocclusion" => {
                        let k = cx.f("ao_materiel_intensity");
                        Tex::from_fn(s, hs, |u, v| grey(aov(u, v) * lerp(1.0, g(&t, u, v), k)))
                    }
                    _ => t,
                }
            };
            emit(cx, &f, &|_| None)
        }
        "x_mesh_data_combiner" => {
            let outs: Vec<&'static str> = cx.outs.to_vec();
            let all: Vec<Option<Tex>> = outs
                .iter()
                .enumerate()
                .map(|(i, o)| {
                    if !cx.wants(i) {
                        return None;
                    }
                    let on = cx.lib.input(&format!("{o}_1")).map(|_| cx.b(&format!("{o}_1"))).unwrap_or(true);
                    let def = match *o {
                        "normal" | "normalWS" => [0.5, 0.5, 1.0, 1.0],
                        "ambient_occlusion" => grey(1.0),
                        "curvature" | "height" | "position_grayscale" | "thickness" => grey(0.5),
                        _ => [0.0, 0.0, 0.0, 1.0],
                    };
                    Some(if on { cx.img_or(o, def) } else { Tex::solid(s, hs, def) })
                })
                .collect();
            cx.pick(all)
        }
        "x_snow_cover" => {
            let m = Mat::read(cx, &["diffuse", "basecolor", "normal", "specular", "", "glossiness", "roughness", "metallic", "", "height", "ambient_occlusion", ""]);
            let mask = cx.img("mask");
            let (fresh, melted, buildup, smoothness, flakes) = (cx.f("fresh_snow"), cx.f("melted_snow"), cx.f("buildup"), cx.f("smoothness"), cx.f("flakes_intensity"));
            let h = m.get("height");
            let n = m.get("normal");
            let seed = cx.seed;
            // snow settles on what faces up (the normal's green toward the viewer is "up" on a wall) and fills the low parts
            let snow = Tex::from_fn(s, hs, |u, v| {
                let nn = dec_n(n.sample(u, v), false);
                let up = nn[2] * 0.6 + (-nn[1]).max(0.0) * 0.4;
                let hh = g(&h, u, v);
                let cover = up * 0.6 + (1.0 - hh) * buildup * 0.8 + noise(u, v, 8, 3, seed.wrapping_add(71)) * 0.4;
                let level = 1.0 - buildup;
                let k = smooth(level - 0.15 + melted * 0.3, level + 0.15 + melted * 0.3, cover) * mask.map(|t| g(t, u, v)).unwrap_or(1.0);
                grey(k)
            });
            let f = |c: &'static str| -> Tex {
                let t = m.get(c);
                Tex::from_fn(s, hs, |u, v| {
                    let k = snow.sample(u, v)[0];
                    let p = t.sample(u, v);
                    let sparkle = rnd((u * s as f32) as i32, (v * hs as f32) as i32, seed.wrapping_add(5)) * flakes;
                    match c {
                        "diffuse" | "basecolor" => {
                            let wv = lerp(0.82, 0.95, fresh) + sparkle * 0.05;
                            lerp4(p, [wv, wv * 0.99, wv * 1.0, 1.0], k)
                        }
                        "roughness" => grey(lerp(p[0], lerp(0.75, 0.35, smoothness) - sparkle * 0.2, k)),
                        "glossiness" => grey(lerp(p[0], lerp(0.25, 0.65, smoothness) + sparkle * 0.2, k)),
                        "metallic" => grey(p[0] * (1.0 - k)),
                        "specular" => lerp4(p, grey(0.22), k),
                        "height" => grey((p[0] + k * 0.25 * buildup).min(1.0)),
                        "normal" => {
                            let nn = dec_n(p, false);
                            enc_n([nn[0] * (1.0 - k * 0.8), nn[1] * (1.0 - k * 0.8), nn[2]], false)
                        }
                        "ambientocclusion" => grey(lerp(p[0], 1.0, k * 0.7)),
                        _ => p,
                    }
                })
            };
            emit(cx, &f, &|_| None)
        }
        "x_water_level" => {
            let m = Mat::read(cx, &["diffuse", "basecolor", "normal", "specular", "", "glossiness", "roughness", "metallic", "", "height", "ambient_occlusion", ""]);
            let mask = cx.img("mask");
            let (level, dark, edge, edge_d) = (cx.f("water_level"), cx.f("water_darkness"), cx.f("edges_wetness"), cx.f("edges_wetness_distance").max(0.001));
            let (blur_amt, blur_op) = (cx.f("depth_blur_amount"), cx.f("depth_blur_opacity"));
            let (sludge, sludge_depth, sludge_op) = (cx.col("sludge_color"), cx.f("sludge_depth"), cx.f("sludge_opacity"));
            let (frost, frost_i, frost_c) = (cx.f("frost"), cx.f("frost_intensity"), cx.f("frost_cracks"));
            let h = m.get("height");
            let seed = cx.seed;
            let water = Tex::from_fn(s, hs, |u, v| {
                let d = level - g(&h, u, v);
                grey(smooth(-0.005, 0.005, d) * mask.map(|t| g(t, u, v)).unwrap_or(1.0))
            });
            let wet = Tex::from_fn(s, hs, |u, v| {
                let d = level - g(&h, u, v);
                grey((smooth(-edge_d, 0.0, d) * edge).max(water.sample(u, v)[0]))
            });
            let depth = Tex::from_fn(s, hs, |u, v| grey(((level - g(&h, u, v)) / sludge_depth.max(0.01)).clamp(0.0, 1.0)));
            let base_blur = ops::blur(&m.get("basecolor"), blur_amt * 4.0 * rs);
            let f = |c: &'static str| -> Tex {
                let t = m.get(c);
                Tex::from_fn(s, hs, |u, v| {
                    let (w, we, dp) = (water.sample(u, v)[0], wet.sample(u, v)[0], depth.sample(u, v)[0]);
                    let p = t.sample(u, v);
                    let crack = if frost > 0.0 { crate::noise::voronoi(u, v, 12, 12, seed.wrapping_add(9), 1.0) } else { (1.0, 1.0, 0.0) };
                    let frost_k = frost * w * (1.0 - smooth(0.0, 0.05, crack.1 - crack.0) * frost_c);
                    match c {
                        "diffuse" | "basecolor" => {
                            let under = lerp4(p, base_blur.sample(u, v), w * blur_op);
                            let darker = [under[0] * (1.0 - dark * 0.6 * we), under[1] * (1.0 - dark * 0.6 * we), under[2] * (1.0 - dark * 0.5 * we), under[3]];
                            let mud = lerp4(darker, sludge, dp * sludge_op * w);
                            lerp4(mud, grey(0.85), frost_k * frost_i)
                        }
                        "roughness" => grey(lerp(lerp(p[0], 0.02, w), 0.35, frost_k)),
                        "glossiness" => grey(lerp(lerp(p[0], 0.98, w), 0.65, frost_k)),
                        "normal" => {
                            let nn = dec_n(p, false);
                            enc_n([nn[0] * (1.0 - w), nn[1] * (1.0 - w), nn[2]], false)
                        }
                        "height" => grey(p[0].max(level * w)),
                        _ => p,
                    }
                })
            };
            emit(cx, &f, &|o| if o.eq_ignore_ascii_case("mask") { Some(water.clone()) } else { None })
        }
        "x_material_crop" => {
            let m = Mat::read(cx, &["diffuse", "basecolor", "normal", "specular", "emissive", "glossiness", "roughness", "metallic", "specular_level", "height", "ambient_occlusion", "opacity"]);
            let mat = cx.v4("matrix");
            let off = cx.v2("offset");
            let det = mat[0] * mat[3] - mat[1] * mat[2];
            let det = if det.abs() < 1e-6 { 1e-6 } else { det };
            let crop = |t: &Tex| Tex::from_fn(s, hs, |u, v| t.sample((mat[3] * u - mat[1] * v) / det + off[0], (-mat[2] * u + mat[0] * v) / det + off[1]));
            let show = cx.b("show_crop_area");
            let extra = |o: &str| -> Option<Tex> {
                match o {
                    "area" => {
                        let b = m.get("basecolor");
                        Some(Tex::from_fn(s, hs, |u, v| {
                            let p = b.sample(u, v);
                            let (x, y) = (u - off[0], v - off[1]);
                            let (cu, cv) = (mat[0] * x + mat[1] * y, mat[2] * x + mat[3] * y);
                            if !show || (0.0..=1.0).contains(&cu) && (0.0..=1.0).contains(&cv) {
                                p
                            } else {
                                [p[0] * 0.35, p[1] * 0.35, p[2] * 0.35, p[3]]
                            }
                        }))
                    }
                    "mask" | "anisotropy_level" | "anisotropy_angle" => cx.img(o).map(|t| crop(t)).or(Some(Tex::solid(s, hs, grey(0.0)))),
                    _ => None,
                }
            };
            emit(cx, &|c| crop(&m.get(c)), &extra)
        }
        "x_material_clone_patch" => {
            let m = Mat::read(cx, &["diffuse", "basecolor", "normal", "specular", "emissive", "glossiness", "roughness", "metallic", "specular_level", "height", "ambient_occlusion", "opacity"]);
            let pt = PatchArea::new(cx);
            let extra = |o: &str| -> Option<Tex> {
                match o {
                    "areas_basecolor" => Some(pt.show(&pt.apply(&m.get("basecolor"), s, hs), s, hs)),
                    "areas_normal" => Some(pt.show(&pt.apply(&m.get("normal"), s, hs), s, hs)),
                    "areas_height" => Some(pt.show(&pt.apply(&m.get("height"), s, hs), s, hs)),
                    "mask" | "anisotropy_level" | "anisotropy_angle" => cx.img(o).map(|t| pt.apply(t, s, hs)).or(Some(Tex::solid(s, hs, grey(0.0)))),
                    _ => None,
                }
            };
            emit(cx, &|c| pt.apply(&m.get(c), s, hs), &extra)
        }
        // ---------------------------------------------------------------- PBR utilities
        "x_pbr_albedo_safe_color" => {
            let b = cx.img_or_ref("basecolor_diffuse", grey(0.5));
            let metal = cx.img("metallic");
            let tol = cx.f("tolerance").clamp(0.0, 1.0);
            let metal_flow = cx.i("pbr_workflow") == 0;
            // non-metals: sRGB 30..240 (50..243 strict); metals: 180..255 brightness
            let (lo, hi) = (lerp(50.0, 30.0, tol) / 255.0, lerp(243.0, 250.0, tol) / 255.0);
            Tex::from_fn(s, hs, |u, v| {
                let p = b.sample(u, v);
                let m = if metal_flow { metal.map(|t| g(t, u, v)).unwrap_or(0.0) } else { 0.0 };
                let l = lum(p).max(1e-4);
                let (a, z) = if m > 0.5 { (180.0 / 255.0, 1.0) } else { (lo, hi) };
                let k = if l < a { a / l } else if l > z { z / l } else { 1.0 };
                [(p[0] * k).clamp(0.0, 1.0), (p[1] * k).clamp(0.0, 1.0), (p[2] * k).clamp(0.0, 1.0), p[3]]
            })
        }
        "x_pbr_basecolor_metallic_validate_2" => {
            let b = cx.img_or_ref("baseColor", grey(0.5));
            let metal = cx.img("metallic");
            let overlay = cx.b("overlay_maps");
            let (ar, mr) = (cx.i("albedo_range"), cx.i("metal_reflectance_range"));
            let (lo, hi) = if ar == 0 { (30.0 / 255.0, 240.0 / 255.0) } else { (50.0 / 255.0, 243.0 / 255.0) };
            let mlo = if mr == 0 { 0.5 } else { 0.7 };
            Tex::from_fn(s, hs, |u, v| {
                let p = b.sample(u, v);
                let m = metal.map(|t| g(t, u, v)).unwrap_or(0.0);
                let l = lum(p);
                let bad = if m > 0.5 { l < mlo } else { l < lo || l > hi } || (m > 0.05 && m < 0.95);
                let flag = if bad { [1.0, 0.0, 0.0, 1.0] } else { [0.0, 1.0, 0.0, 1.0] };
                if overlay {
                    lerp4(p, flag, if bad { 0.6 } else { 0.0 })
                } else {
                    flag
                }
            })
        }
        "x_basecolor_metallic_roughness_converter" => {
            let b = cx.img_or_ref("basecolor", grey(0.5));
            let n = cx.img_or_ref("normal", [0.5, 0.5, 1.0, 1.0]);
            let r = cx.img_or_ref("roughness", grey(0.5));
            let m = cx.img_or_ref("metallic", grey(0.0));
            let sl = if cx.b("use_specularlevel_input") { cx.img("specularlevel") } else { None };
            let f0 = |u: f32, v: f32| -> [f32; 3] {
                let base = b.sample(u, v);
                let mm = g(&m, u, v);
                let d = sl.map(|t| 0.08 * g(t, u, v)).unwrap_or(0.04);
                [lerp(d, to_lin(base[0]), mm), lerp(d, to_lin(base[1]), mm), lerp(d, to_lin(base[2]), mm)]
            };
            let outs = cx.outs.to_vec();
            let all: Vec<Option<Tex>> = outs
                .iter()
                .enumerate()
                .map(|(i, o)| {
                    if !cx.wants(i) {
                        return None;
                    }
                    Some(match *o {
                        "diffuse" => Tex::from_fn(s, hs, |u, v| {
                            let p = b.sample(u, v);
                            let k = 1.0 - g(&m, u, v);
                            [p[0] * k, p[1] * k, p[2] * k, p[3]]
                        }),
                        "normal" => n.clone().into_owned(),
                        "specular" | "speuclar_face_color" => Tex::from_fn(s, hs, |u, v| {
                            let f = f0(u, v);
                            [to_srgb(f[0]), to_srgb(f[1]), to_srgb(f[2]), 1.0]
                        }),
                        "reflection" => Tex::from_fn(s, hs, |u, v| {
                            let f = f0(u, v);
                            grey(to_srgb((f[0] + f[1] + f[2]) / 3.0))
                        }),
                        "glossiness" => r.map(|p| grey(1.0 - p[0])),
                        "roughness" => r.clone().into_owned(),
                        "ior" => Tex::from_fn(s, hs, |u, v| {
                            let f = f0(u, v);
                            let sq = ((f[0] + f[1] + f[2]) / 3.0).clamp(0.0, 0.99).sqrt();
                            grey(((1.0 + sq) / (1.0 - sq)) / 4.0)
                        }),
                        _ => Tex::from_fn(s, hs, |u, v| {
                            let f = f0(u, v);
                            grey((f[0] + f[1] + f[2]) / 3.0)
                        }),
                    })
                })
                .collect();
            cx.pick(all)
        }
        "x_pbr_dielectric_f0" => {
            let f0 = match cx.i("specular_values") {
                1 => lerp(0.04, 0.08, cx.f("specular_range_glass")),
                2 => {
                    let ior = cx.f("ior").max(1.0);
                    ((ior - 1.0) / (ior + 1.0)).powi(2)
                }
                _ => lerp(0.02, 0.05, cx.f("specular_range")),
            };
            Tex::solid(s, hs, grey(to_srgb(f0)))
        }
        "x_pbr_metal_reflectance" => {
            // linear F0 of common metals (sRGB-encoded on output)
            const METALS: [[f32; 3]; 12] = [
                [0.562, 0.565, 0.578],
                [0.972, 0.960, 0.915],
                [0.913, 0.922, 0.924],
                [1.000, 0.766, 0.336],
                [0.955, 0.638, 0.538],
                [0.550, 0.556, 0.554],
                [0.660, 0.609, 0.526],
                [0.542, 0.497, 0.449],
                [0.662, 0.655, 0.634],
                [0.672, 0.637, 0.585],
                [0.664, 0.824, 0.850],
                [0.910, 0.778, 0.423],
            ];
            let c = METALS[cx.i("metal_colors").clamp(0, METALS.len() as i32 - 1) as usize];
            Tex::solid(s, hs, [to_srgb(c[0]), to_srgb(c[1]), to_srgb(c[2]), 1.0])
        }
        "x_pbr_base_material" => pbr_base_material(cx),
        "x_bitmap2material_2_light" => bitmap2material(cx),
        "x_pbr_render" => pbr_render(cx),
        "x_atlas_spliter" | "x_atlas_scatter" => atlas(cx),
        "x_smart_auto_tile" => {
            let b = cx.img_or_ref("baseColor", grey(0.5));
            let n = cx.img_or_ref("normal", [0.5, 0.5, 1.0, 1.0]);
            let h = cx.img_or_ref("height", grey(0.5));
            let mat = cx.v4("crop_matrix");
            let off = cx.v2("crop_offset");
            let det = mat[0] * mat[3] - mat[1] * mat[2];
            let det = if det.abs() < 1e-6 { 1e-6 } else { det };
            let crop = |t: &Tex| Tex::from_fn(s, hs, |u, v| t.sample((mat[3] * u - mat[1] * v) / det + off[0], (-mat[2] * u + mat[0] * v) / det + off[1]));
            let patches = (cx.i("grid_resolution").clamp(1, 11) as f32 * 0.8).max(2.0);
            let soft = (cx.f("edge_smoothness") + cx.f("edge_blur") * 0.5).max(0.05);
            let seed = cx.seed.wrapping_add(31);
            let tile = |t: &Tex| ops::make_tile_patch(&crop(t), patches, soft, seed);
            let coords = Tex::from_fn(s, hs, |u, v| [u, v, 0.0, 1.0]);
            let pat = tile(&coords);
            let pattern = Tex::from_fn(s, hs, |u, v| grey(rnd((pat.sample(u, v)[0] * 64.0) as i32, (pat.sample(u, v)[1] * 64.0) as i32, 3)));
            let outs: Vec<Option<Tex>> = vec![
                if cx.wants(0) { Some(tile(&b)) } else { None },
                if cx.wants(1) { Some(tile(&n)) } else { None },
                if cx.wants(2) { Some(tile(&h)) } else { None },
                if cx.wants(3) { Some(pattern) } else { None },
            ];
            cx.pick(outs)
        }
        _ => return None,
    })
}

/// Source and target areas of the material clone patch (same rules as the image clone patch).
struct PatchArea {
    src_c: [f32; 2],
    src_m: [f32; 4],
    tgt_c: [f32; 2],
    tgt_m: [f32; 4],
    shape: i32,
    soft: f32,
    show: bool,
}

impl PatchArea {
    fn new(cx: &Cx) -> PatchArea {
        let so = cx.v2("src_offset");
        let to = cx.v2("tgt_offset");
        PatchArea {
            src_c: [0.5 + so[0] * 0.5, 0.5 + so[1] * 0.5],
            src_m: cx.v4("src_matrix"),
            tgt_c: [0.5 + to[0] * 0.5, 0.5 + to[1] * 0.5],
            tgt_m: cx.v4("tgt_matrix"),
            shape: cx.i("shape"),
            soft: (cx.f("edge_blur") + cx.f("edge_smoothness")) * 0.05,
            show: cx.b("show_patch_areas"),
        }
    }
    fn local(&self, m: [f32; 4], c: [f32; 2], u: f32, v: f32) -> ([f32; 2], f32) {
        let (x, y) = (ops::wrapd(u - c[0]), ops::wrapd(v - c[1]));
        let l = [(m[0] * x + m[1] * y) * 2.0, (m[2] * x + m[3] * y) * 2.0];
        let d = match self.shape {
            0 => l[0].abs().max(l[1].abs()),
            1 => l[0].abs() + l[1].abs(),
            _ => (l[0] * l[0] + l[1] * l[1]).sqrt(),
        };
        (l, 1.0 - smooth(1.0 - self.soft - 0.02, 1.0, d))
    }
    fn apply(&self, t: &Tex, s: usize, hs: usize) -> Tex {
        let m = self.src_m;
        let det = m[0] * m[3] - m[1] * m[2];
        let det = if det.abs() < 1e-6 { 1e-6 } else { det };
        Tex::from_fn(s, hs, |u, v| {
            let base = t.sample(u, v);
            let (l, k) = self.local(self.tgt_m, self.tgt_c, u, v);
            if k <= 0.0 {
                return base;
            }
            let (x, y) = (l[0] * 0.5, l[1] * 0.5);
            lerp4(base, t.sample((m[3] * x - m[1] * y) / det + self.src_c[0], (-m[2] * x + m[0] * y) / det + self.src_c[1]), k)
        })
    }
    fn show(&self, t: &Tex, s: usize, hs: usize) -> Tex {
        if !self.show {
            return t.clone();
        }
        Tex::from_fn(s, hs, |u, v| {
            let p = t.sample(u, v);
            let (_, kt) = self.local(self.tgt_m, self.tgt_c, u, v);
            let (_, ks) = self.local(self.src_m, self.src_c, u, v);
            let edge = |k: f32| (k > 0.05 && k < 0.6) as i32 as f32;
            let (et, es) = (edge(kt), edge(ks));
            [lerp(lerp(p[0], 1.0, es), 0.0, et), lerp(lerp(p[1], 0.0, es), 1.0, et), lerp(p[2], 0.0, es.max(et)), 1.0]
        })
    }
}

// ------------------------------------------------------------------------------------------------ mask generators

/// The mesh maps a mask generator reads, each optional.
struct Maps<'a> {
    ao: Option<&'a Tex>,
    curv: Option<&'a Tex>,
    pos: Option<&'a Tex>,
    nws: Option<&'a Tex>,
    thick: Option<&'a Tex>,
    var: Option<&'a Tex>,
    grunge: Option<&'a Tex>,
    mask: Option<&'a Tex>,
}

impl<'a> Maps<'a> {
    fn of(cx: &Cx<'a>) -> Maps<'a> {
        let first = |ids: &[&str]| ids.iter().find_map(|i| cx.img(i));
        Maps {
            ao: first(&["ambient_occlusion", "ambientOcclusion"]),
            curv: first(&["curvature"]),
            pos: first(&["position"]),
            nws: first(&["normalWS", "normalws", "world_space_normals", "world_space_normal"]),
            thick: first(&["thickness"]),
            var: first(&["variation_mask", "noise", "Input"]),
            grunge: first(&["input", "grunge"]),
            mask: first(&["mask", "mask_1"]),
        }
    }
    fn ao(&self, u: f32, v: f32) -> f32 {
        self.ao.map(|t| g(t, u, v)).unwrap_or(1.0)
    }
    /// Convexity 0..1 (0.5 flat).
    fn curv(&self, u: f32, v: f32) -> f32 {
        self.curv.map(|t| g(t, u, v)).unwrap_or(0.5)
    }
    /// Height in the object, 0 (bottom) .. 1 (top): the position map's green (or gray) value.
    fn up(&self, u: f32, v: f32) -> f32 {
        self.pos
            .map(|t| {
                let p = t.sample(u, v);
                if (p[0] - p[1]).abs() < 1e-6 && (p[1] - p[2]).abs() < 1e-6 {
                    p[0]
                } else {
                    p[1]
                }
            })
            .unwrap_or(1.0 - v)
    }
    /// How much the surface faces up, 0..1.
    fn top(&self, u: f32, v: f32) -> f32 {
        self.nws.map(|t| (t.sample(u, v)[1] * 2.0 - 1.0).max(0.0)).unwrap_or(0.5)
    }
    fn mask(&self, u: f32, v: f32) -> f32 {
        self.mask.map(|t| g(t, u, v)).unwrap_or(1.0)
    }
}

fn mask_generator(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let m = Maps::of(cx);
    let seed = cx.seed;
    let k = cx.kind;
    let level = if cx.lib.input("level").is_some() { cx.f("level") } else if cx.lib.input("wear_level").is_some() { cx.f("wear_level") } else if cx.lib.input("dirt_level").is_some() { cx.f("dirt_level") } else { 0.5 };
    let contrast = if cx.lib.input("contrast").is_some() {
        cx.f("contrast")
    } else if cx.lib.input("Contrast").is_some() {
        cx.f("Contrast")
    } else if cx.lib.input("wear_contrast").is_some() {
        cx.f("wear_contrast")
    } else if cx.lib.input("dirt_contrast").is_some() {
        cx.f("dirt_contrast")
    } else {
        0.5
    };
    // edge maps: convex and concave parts of the curvature, optionally softened
    let soft_curv = |radius: f32| -> Option<Tex> { m.curv.map(|c| if radius > 0.1 { ops::blur(c, radius * cx.rs()) } else { c.clone() }) };
    let grunge_scale = if cx.lib.input("grunge_scale").is_some() { cx.i("grunge_scale").max(1) } else { 1 };
    let grunge = |u: f32, v: f32, cells: i32, salt: u32| -> f32 {
        if cx.lib.input("use_custom_grunge").is_some() && cx.b("use_custom_grunge") {
            if let Some(t) = m.grunge {
                return g(t, u, v);
            }
        }
        noise(u, v, cells * grunge_scale, 5, seed.wrapping_add(salt))
    };
    let variation = |u: f32, v: f32, salt: u32| -> f32 {
        if cx.lib.input("override_variation_mask").is_some() && cx.b("override_variation_mask") {
            if let Some(t) = m.var {
                return g(t, u, v);
            }
        }
        if cx.lib.input("override_noise").is_some() && cx.b("override_noise") {
            if let Some(t) = m.var {
                return g(t, u, v);
            }
        }
        noise(u, v, 6, 4, seed.wrapping_add(salt))
    };
    let lc = |x: f32| level_contrast(x, level, contrast);
    let blurred = soft_curv(match k {
        "x_mg_edge_blur" => cx.f("blur_radius") * 2.0,
        "x_mg_metal_edge_wear" | "x_mg_fiber_glass_edge_wear" => cx.f("edges_smoothness"),
        "x_mg_edge_notch" => cx.f("radius"),
        "x_mg_paint_wear" => cx.f("radius") * 2.0,
        _ => 0.0,
    });
    let convex = |u: f32, v: f32| blurred.as_ref().map(|c| ((g(c, u, v) - 0.5) * 2.0).clamp(0.0, 1.0)).unwrap_or(0.0);
    let concave = |u: f32, v: f32| blurred.as_ref().map(|c| ((0.5 - g(c, u, v)) * 2.0).clamp(0.0, 1.0)).unwrap_or(0.0);
    let streaks = |u: f32, v: f32, len: f32, salt: u32| -> f32 {
        // vertical streaks: noise stretched downward
        let col = noise(u * 1.0, v * 0.08 / len.max(0.1), 24, 3, seed.wrapping_add(salt));
        smooth(0.45, 0.75, col)
    };
    match k {
        "x_mg_light" => {
            let ha = cx.f("horizontal_angle") * TAU;
            let va = cx.f("vertical_angle") * TAU;
            let l = [ha.cos() * va.cos(), va.sin(), ha.sin() * va.cos()];
            let gloss = cx.f("highlight_glossiness").clamp(0.0, 0.999);
            let hl = cx.f("highlight_level");
            return Tex::from_fn(s, hs, |u, v| {
                let n = m.nws.map(|t| dec_n(t.sample(u, v), false)).unwrap_or([0.0, 1.0, 0.0]);
                let d = (n[0] * l[0] + n[1] * l[1] + n[2] * l[2]).max(0.0);
                let spec = d.powf(1.0 + gloss * 200.0) * hl;
                grey((d + spec).min(1.0))
            });
        }
        "x_mg_mask_builder_2" => {
            let inv = cx.b("invert");
            let (ao_i, ao_r, ao_c, ao_noise) = (cx.f("ao_intensity"), cx.f("ao_range"), cx.f("ao_contrast"), cx.f("ao_noisiness"));
            let (cg, cvx, ccv, cboost, cnoise) = (cx.f("curv_global_intensity"), cx.f("curvature_convex_intensity"), cx.f("curvature_concave_intensity"), cx.f("curv_boost"), cx.f("curvature_noisiness"));
            let (pi_, pr, pc) = (cx.f("position_intensity"), cx.f("position_range"), cx.f("position_contrast"));
            let (wsi, top_i, bottom_i) = (cx.f("wsnormal_intensity"), cx.f("world_space_normal_top_intensity"), cx.f("world_space_normal_bottom_intensity"));
            let (gi, scratch_i, scatter_i) = (cx.f("grunge_intensity"), cx.f("scratches_intensity"), cx.f("scatter_intensity"));
            let n_scr = cx.i("scratches_amount").clamp(0, 4096) as usize;
            let mut scratches = vec![0.0f32; s * hs];
            if scratch_i.abs() > 1e-3 && n_scr > 0 {
                draw_scratches(&mut scratches, s, hs, n_scr.min(1500), cx.f("scratches_scale").max(0.05) * 0.15, seed.wrapping_add(17));
            }
            let res: Vec<[f32; 4]> = crate::tex::par_map(s * hs, |i| {
                let (u, v) = (((i % s) as f32 + 0.5) / s as f32, ((i / s) as f32 + 0.5) / hs as f32);
                let ao = 1.0 - m.ao(u, v);
                let ao = smooth(1.0 - ao_r, 1.0 - ao_r + (1.0 - ao_c) * 0.5 + 0.01, ao + (noise(u, v, 8, 3, seed) - 0.5) * ao_noise) * ao_i;
                let c = m.curv(u, v);
                let cv = ((c - 0.5) * 2.0).max(0.0) * cvx + ((0.5 - c) * 2.0).max(0.0) * ccv;
                let cv = (cv * (1.0 + cboost * 2.0) + (noise(u, v, 10, 3, seed.wrapping_add(3)) - 0.5) * cnoise).clamp(-1.0, 1.0) * cg;
                let p = smooth(1.0 - pr - (1.0 - pc) * 0.3, 1.0 - pr + (1.0 - pc) * 0.3 + 0.01, m.up(u, v)) * pi_;
                let nn = m.nws.map(|t| dec_n(t.sample(u, v), false)).unwrap_or([0.0, 0.0, 1.0]);
                let ws = (nn[1].max(0.0) * top_i + (-nn[1]).max(0.0) * bottom_i) * wsi.max(top_i.abs().max(bottom_i.abs()).signum());
                let gr = (grunge(u, v, 4, 11) - 0.5) * 2.0 * gi;
                let sc = scratches[i] * scratch_i;
                let sp = if scatter_i.abs() > 1e-3 { smooth(0.6, 0.7, noise(u, v, cx.i("scatter_scale").max(1), 1, seed.wrapping_add(23))) * scatter_i } else { 0.0 };
                let x = (ao + cv + p + ws + gr + sc + sp).clamp(0.0, 1.0);
                let x = lc(x) * m.mask(u, v);
                grey(if inv { 1.0 - x } else { x })
            });
            return Tex { w: s, h: hs, px: res };
        }
        _ => {}
    }
    let res: Vec<[f32; 4]> = crate::tex::par_map(s * hs, |i| {
        let (u, v) = (((i % s) as f32 + 0.5) / s as f32, ((i / s) as f32 + 0.5) / hs as f32);
        let x = match k {
            "x_mg_bottom_to_top" => {
                let rv = cx.f("roughness_variation");
                let n = m.var.map(|t| g(t, u, v)).unwrap_or_else(|| noise(u, v, 6, 4, seed));
                lc(1.0 - m.up(u, v) + (n - 0.5) * rv)
            }
            "x_mg_cloth_wear" => {
                let hard = cx.f("hard_edges_amount");
                let soft = cx.f("wear_softness");
                let h = cx.img("height").map(|t| g(t, u, v)).unwrap_or(0.5);
                let c = convex(u, v) * (1.0 + hard * 2.0) + (h - 0.5) * 0.5;
                smooth(0.5 - soft * 0.1, 0.5 + soft * 0.1 + 0.05, c + (noise(u, v, 32, 3, seed) - 0.5) * 0.3)
            }
            "x_mg_dirt" => {
                let edges = cx.f("edges_masking");
                let ga = cx.f("grunge_amount");
                let cav = (1.0 - m.ao(u, v)) * 0.8 + concave(u, v) * 0.5;
                let x = cav - convex(u, v) * edges + (grunge(u, v, 6, 5) - 0.5) * ga;
                lc(x.clamp(0.0, 1.0))
            }
            "x_mg_dripping_rust" => {
                let (spread, rcon, sm, drips) = (cx.f("rust_spreading"), cx.f("rust_contrast"), cx.f("spreading_smoothness"), cx.f("drips_intensity"));
                let base = (1.0 - m.ao(u, v)) * 0.6 + concave(u, v) * 0.6 + (noise(u, v, 8, 4, seed) - 0.5) * (0.3 + sm * 0.3);
                let r = level_contrast(base, spread, rcon);
                (r + streaks(u, v, 1.0 + cx.f("drips_smoothness") * 4.0, 7) * drips * (1.0 - m.up(u, v) * 0.5)).clamp(0.0, 1.0)
            }
            "x_mg_dust" => {
                let occ = cx.f("occlusion_amount");
                let nop = cx.f("noise_opacity");
                let x = m.top(u, v) * lerp(1.0, m.ao(u, v), occ) + (variation(u, v, 13) - 0.5) * nop;
                lc(x)
            }
            "x_mg_edge_blur" | "x_mg_edge_select" => {
                if k == "x_mg_edge_select" {
                    let c = m.curv(u, v);
                    let (cw, cs, ci) = (cx.f("convex_edges_width"), cx.f("convex_softness"), cx.f("convex_intensity"));
                    let (vw, vs, vi) = (cx.f("concave_edges_width"), cx.f("concave_softness"), cx.f("concave_intensity"));
                    let cvx = smooth(0.5 + (1.0 - cw) * 0.45 - cs * 0.2, 0.5 + (1.0 - cw) * 0.45 + cs * 0.2 + 0.01, c) * ci;
                    let ccv = (1.0 - smooth(0.5 - (1.0 - vw) * 0.45 - vs * 0.2 - 0.01, 0.5 - (1.0 - vw) * 0.45 + vs * 0.2, c)) * vi;
                    lc(cvx.max(ccv))
                } else {
                    lc(convex(u, v))
                }
            }
            "x_mg_edge_damages" => {
                let d = cx.f("damages_intensity");
                let n = noise(u, v, 16, 5, seed.wrapping_add(19));
                lc((convex(u, v) * (1.0 + d * 2.0) - (1.0 - n) * d * 1.5).clamp(0.0, 1.0)) * m.ao(u, v).sqrt()
            }
            "x_mg_edge_dirt" | "x_mg_selective_dirt" => {
                let var = cx.f("variation");
                lc((concave(u, v) + (variation(u, v, 29) - 0.5) * var).clamp(0.0, 1.0))
            }
            "x_mg_edge_notch" => {
                let n = smooth(0.35, 0.65, noise(u, v, 24, 4, seed.wrapping_add(31)));
                lc(convex(u, v) * n)
            }
            "x_mg_edge_speckle" => {
                let sel = cx.f("edge_selection");
                let var = cx.f("variation");
                let sp = smooth(0.55, 0.7, noise(u, v, 48, 2, seed.wrapping_add(37)));
                let e = smooth(1.0 - sel - 0.1, 1.0 - sel + 0.1, convex(u, v));
                lc((e * sp + (variation(u, v, 41) - 0.5) * var * e).clamp(0.0, 1.0))
            }
            "x_mg_edge_wear" => {
                let (th, wd, dis) = (cx.f("threshold"), cx.f("edges_width"), cx.f("disorder"));
                let c = convex(u, v) + (noise(u, v, 20, 4, seed.wrapping_add(43)) - 0.5) * dis;
                lc(smooth((1.0 - th) * 0.9 - wd * 0.5, (1.0 - th) * 0.9 + wd * 0.5 + 0.01, c))
            }
            "x_mg_fiber_glass_edge_wear" | "x_mg_metal_edge_wear" | "x_mg_leather_wear" => {
                let (ga, aom, cw) = (cx.f("grunge_amount"), cx.f("ao_masking"), cx.f("curvature_weight"));
                let e = convex(u, v) * (0.5 + cw * 1.5);
                let x = e + (grunge(u, v, 10, 47) - 0.5) * ga * 1.5 - (1.0 - m.ao(u, v)) * aom;
                lc(x.clamp(0.0, 1.0))
            }
            "x_mg_grease" => {
                let th = cx.f("thickness_threshold");
                let t = m.thick.map(|t| g(t, u, v)).unwrap_or(0.5);
                let x = (1.0 - smooth(th - 0.2, th + 0.2, t)) * 0.8 + (variation(u, v, 53) - 0.5) * 0.4;
                lc(x.clamp(0.0, 1.0))
            }
            "x_mg_ground_dirt" => {
                let dh = cx.f("dirt_height");
                let x = 1.0 - smooth(dh * 0.5, dh + 0.05, m.up(u, v)) + (noise(u, v, 6, 4, seed.wrapping_add(59)) - 0.5) * 0.3;
                lc(x.clamp(0.0, 1.0))
            }
            "x_mg_leaks" => {
                let var = cx.f("variation");
                let len = cx.f("lenght");
                let src = (1.0 - m.ao(u, v)) * 0.7 + concave(u, v) * 0.5;
                let st = streaks(u, v, len.max(0.1), 61);
                lc((src * 0.5 + st * (0.4 + var * 0.4) * (0.4 + src)).clamp(0.0, 1.0))
            }
            "x_mg_paint_wear" => {
                let (occ, var) = (cx.f("occlusion"), cx.f("variation"));
                let x = convex(u, v) * 1.2 + (variation(u, v, 67) - 0.5) * var - (1.0 - m.ao(u, v)) * occ;
                lc(x.clamp(0.0, 1.0))
            }
            "x_mg_sun_bleach" => {
                let occ = cx.f("occlusion");
                let nn = m.nws.map(|t| dec_n(t.sample(u, v), false)).unwrap_or([0.0, 0.5, 0.8]);
                let sun = (nn[1] * 0.7 + nn[2] * 0.3).max(0.0);
                lc(sun * lerp(1.0, m.ao(u, v), occ))
            }
            "x_mg_surface_brush" => {
                let len = cx.f("scratches_lenght").max(0.1);
                let axis = cx.i("occlude_axis");
                let (ax_i, occ, sharp) = (cx.f("occlude_axis_intensity"), cx.f("occlusion"), cx.f("sharpen_intensity"));
                // brush strokes: noise stretched along one axis
                let n = if axis == 1 { noise(u * 0.06 / len, v, 64, 3, seed.wrapping_add(71)) } else { noise(u, v * 0.06 / len, 64, 3, seed.wrapping_add(71)) };
                let n = ((n - 0.5) * (1.0 + sharp * 4.0) + 0.5).clamp(0.0, 1.0);
                let nn = m.nws.map(|t| dec_n(t.sample(u, v), false)).unwrap_or([0.0, 0.0, 1.0]);
                let facing = [nn[0].abs(), nn[1].abs(), nn[2].abs()][axis.clamp(0, 2) as usize];
                lc(n * lerp(1.0, m.ao(u, v), occ) * lerp(1.0, 1.0 - facing, ax_i))
            }
            _ => 0.0,
        };
        grey((x * m.mask(u, v)).clamp(0.0, 1.0))
    });
    Tex { w: s, h: hs, px: res }
}

/// Thin random strokes (bright) into a gray buffer.
fn draw_scratches(buf: &mut [f32], s: usize, hs: usize, n: usize, len: f32, seed: u32) {
    for i in 0..n {
        let r = |k: i32| rnd(i as i32, k, seed);
        let (x0, y0) = (r(1) * s as f32, r(2) * hs as f32);
        let a = r(3) * TAU;
        let l = len * s as f32 * (0.4 + r(4));
        let steps = (l * 1.5).ceil().max(2.0) as usize;
        let strength = 0.4 + 0.6 * r(5);
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            let (x, y) = (x0 + a.cos() * l * t, y0 + a.sin() * l * t);
            let (xi, yi) = ((x.floor() as i64).rem_euclid(s as i64) as usize, (y.floor() as i64).rem_euclid(hs as i64) as usize);
            let fade = 1.0 - (2.0 * t - 1.0).abs().powi(2);
            let p = &mut buf[yi * s + xi];
            *p = p.max(strength * fade);
        }
    }
}

// ------------------------------------------------------------------------------------------------ weathering

fn weathering(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let rs = cx.rs();
    let seed = cx.seed;
    let k = cx.kind;
    // the input of each channel (the library's own input names)
    let names: [&str; 12] = match k {
        "x_cracks_weathering" => ["Input_9", "Input_13", "Input_6", "Input", "", "Input_8", "Input_11", "Input_12", "", "Input_14", "Input_2", ""],
        "x_fabric_weathering" | "x_leather_weathering" => ["Diffuse_1", "basecolor_1", "Input", "Specular_1", "", "Input_1", "Input_3", "Input_4", "", "Height_1", "Input_2", ""],
        "x_rock_weathering" => ["Diffuse", "basecolor_1", "Input", "Specular", "", "Input_1", "Input_3", "Input_4", "", "Height", "Input_2", ""],
        "x_moss_weathering" => ["Input_2", "Input_5", "Input", "Input_7", "", "Input_6", "Input_8", "Input_9", "", "Input_11", "Input_10", ""],
        "x_painted_metal_weathering" => ["Diffuse", "basecolor_1", "Normal_High_Poly_1", "Specular", "", "Input", "Input_1", "Input_2", "", "Height", "Input_3", ""],
        _ => ["diffuse_1", "basecolor_1", "normal_1", "specular_1", "", "glossiness_1", "roughness_1", "metallic_1", "", "height_1", "ambientOcclusion_1", ""],
    };
    let m = Mat::read(cx, &names);
    let mesh_ao = ["AO", "ambientOcclusion", "Input_4"].iter().find_map(|i| if k == "x_cracks_weathering" && *i == "Input_4" { None } else { cx.img(i) });
    let curv_in = ["Curvature", "curvature", "Input_4"].iter().find_map(|i| if *i == "Input_4" && k != "x_cracks_weathering" { None } else { cx.img(i) });
    let user_mask = if cx.lib.input("mask").is_some() && cx.b("mask") { ["Input_12", "mask_1", "Input_5"].iter().find_map(|i| cx.img(i)) } else { None };
    let h = m.get("height");
    let curv = match curv_in {
        Some(c) if !is_blank(c) => c.clone(),
        _ => curvature_of(&h, 3.0 * rs),
    };
    let ao = match mesh_ao {
        Some(a) if !is_blank(a) => a.clone(),
        _ => ops::ambient_occlusion(&h, 6.0 * rs, 1.0),
    };
    let fp = |id: &str, d: f32| if cx.lib.input(id).is_some() { cx.f(id) } else { d };
    // the effects each kind adds: dust and dirt in cavities, wear on edges, and its own pattern
    let (dust, dirt, wear) = (fp("Dust", fp("dust", 0.0)), fp("Dirtiness", fp("dirtiness", 0.0)), fp("Edges_Wearing", fp("edges_wearing", 0.0)));
    let age = fp("Age", fp("age", 0.0));
    let pattern = |u: f32, v: f32| -> f32 {
        match k {
            "x_cracks_weathering" => {
                let prop = fp("cracks_propagation", 1.0);
                let vr = crate::noise::voronoi(u, v, 10, 10, seed.wrapping_add(3), 1.0);
                let line = 1.0 - smooth(0.0, 0.035, vr.1 - vr.0);
                line * smooth(1.0 - prop, 1.0 - prop + 0.3, noise(u, v, 4, 3, seed.wrapping_add(4)))
            }
            "x_moss_weathering" => {
                let prop = fp("moss_propagation", 1.0);
                let n = noise(u, v, 8, 5, seed.wrapping_add(5));
                let up = 1.0 - g(&h, u, v);
                smooth(0.8 - prop * 0.3, 1.0 - prop * 0.3, n * 0.6 + up * 0.2 + (1.0 - g(&ao, u, v)) * 0.4)
            }
            "x_rust_weathering" => {
                let spread = fp("rust_spreading", 0.11);
                let n = noise(u, v, 10, 5, seed.wrapping_add(6));
                let cav = 1.0 - g(&ao, u, v) + ((0.5 - g(&curv, u, v)) * 2.0).max(0.0);
                smooth(1.0 - spread * 2.0, 1.0 - spread * 2.0 + 0.2, n * 0.6 + cav * 0.6)
            }
            "x_painted_metal_weathering" => {
                let peel = fp("paint_peeling", 0.0);
                let sc = fp("paint_cracks_scale", 4.0).round().max(1.0) as i32;
                let n = noise(u, v, sc * 2, 5, seed.wrapping_add(7));
                let e = ((g(&curv, u, v) - 0.5) * 2.0).max(0.0);
                smooth(1.0 - peel, 1.0 - peel + 0.15, n * 0.7 + e * 0.6)
            }
            "x_rock_weathering" => {
                let sc = fp("cracks_scale", 32.0).round().max(1.0) as i32;
                let ci = fp("cracks_intensity", 0.5);
                let vr = crate::noise::voronoi(u, v, sc / 2 + 1, sc / 2 + 1, seed.wrapping_add(8), 1.0);
                (1.0 - smooth(0.0, 0.03, vr.1 - vr.0)) * ci
            }
            "x_leather_weathering" => {
                let sc = fp("Cracks_Scale", 5.0).round().max(1.0) as i32;
                let vr = crate::noise::voronoi(u, v, sc * 3, sc * 3, seed.wrapping_add(9), 1.0);
                (1.0 - smooth(0.0, 0.05, vr.1 - vr.0)) * age
            }
            _ => smooth(0.6, 0.8, noise(u, v, 24, 4, seed.wrapping_add(10))) * fp("Age_Creases", 0.5) * age,
        }
    };
    let res_mask = Tex::from_fn(s, hs, |u, v| {
        let um = user_mask.map(|t| g(t, u, v)).unwrap_or(1.0);
        let e = ((g(&curv, u, v) - 0.5) * 2.0).max(0.0);
        let cav = 1.0 - g(&ao, u, v);
        let n = noise(u, v, 12, 4, seed.wrapping_add(11));
        // channels: r = edge wear, g = dirt/dust, b = the kind's own pattern
        [smooth(0.4, 0.7, e + (n - 0.5) * 0.4) * wear * um, ((cav * 0.8 + n * 0.3) * dirt + (1.0 - g(&h, u, v)) * dust * 0.6).min(1.0) * um, pattern(u, v) * um, 1.0]
    });
    let (desat, bright) = (fp("Old_Fabric_Desaturation", fp("Used_Leather_Desaturation", fp("used_rock_desaturation", 0.0))), fp("Old_Fabric_Brightness", fp("Used_Leather_Brightness", fp("used_rock_brightness", 0.5))));
    let moss_col = if k == "x_moss_weathering" { cx.col("Moss_Color") } else { [0.44, 0.45, 0.07, 1.0] };
    let rust_col = [0.45, 0.20, 0.08, 1.0];
    let raw_metal = if k == "x_painted_metal_weathering" { cx.col("raw_metal_color") } else { grey(0.5) };
    let (rmr, rmg) = if k == "x_painted_metal_weathering" { (cx.f("raw_metal_roughness_value"), cx.f("raw_metal_glossiness_value")) } else { (0.25, 0.75) };
    let intensity = |c: &str| -> f32 {
        let id = match c {
            "ambientocclusion" => "ambient_occlusion_intensity".to_string(),
            o => format!("{o}_intensity"),
        };
        if cx.lib.input(&id).is_some() {
            cx.f(&id)
        } else if c == "ambientocclusion" && cx.lib.input("ao_intensity").is_some() {
            cx.f("ao_intensity")
        } else {
            1.0
        }
    };
    let f = |c: &'static str| -> Tex {
        let t = m.get(c);
        let ki = intensity(c);
        Tex::from_fn(s, hs, |u, v| {
            let p = t.sample(u, v);
            let wm = res_mask.sample(u, v);
            let (we, wd, wp) = (wm[0] * ki, wm[1] * ki, wm[2] * ki);
            match c {
                "diffuse" | "basecolor" => {
                    let l = lum(p);
                    let aged = lerp4(p, grey(l), desat * age);
                    let aged = [aged[0] * lerp(1.0, bright * 2.0, age), aged[1] * lerp(1.0, bright * 2.0, age), aged[2] * lerp(1.0, bright * 2.0, age), p[3]];
                    let dirty = lerp4(aged, [0.18, 0.15, 0.11, 1.0], wd * 0.8);
                    let worn = lerp4(dirty, [(p[0] + 0.25).min(1.0), (p[1] + 0.25).min(1.0), (p[2] + 0.22).min(1.0), p[3]], we * 0.6);
                    let over = match k {
                        "x_moss_weathering" => moss_col,
                        "x_rust_weathering" => rust_col,
                        "x_painted_metal_weathering" => raw_metal,
                        _ => [p[0] * 0.35, p[1] * 0.33, p[2] * 0.3, 1.0],
                    };
                    lerp4(worn, over, wp)
                }
                "roughness" => grey((p[0] + wd * 0.3 - we * 0.15 + match k {
                    "x_moss_weathering" => wp * (0.85 - p[0]),
                    "x_rust_weathering" => wp * (0.9 - p[0]),
                    "x_painted_metal_weathering" => wp * (rmr - p[0]),
                    _ => wp * 0.2,
                })
                .clamp(0.0, 1.0)),
                "glossiness" => grey((p[0] - wd * 0.3 + we * 0.15 + match k {
                    "x_painted_metal_weathering" => wp * (rmg - p[0]),
                    _ => -wp * 0.3,
                })
                .clamp(0.0, 1.0)),
                "metallic" => grey(match k {
                    "x_painted_metal_weathering" => p[0].max(wp),
                    "x_rust_weathering" => p[0] * (1.0 - wp),
                    _ => p[0] * (1.0 - wd),
                }),
                "height" => grey((p[0] + match k {
                    "x_moss_weathering" => wp * 0.15,
                    "x_cracks_weathering" | "x_rock_weathering" | "x_leather_weathering" => -wp * fp("cracks_depth", 0.5) * 0.3,
                    _ => -we * 0.05,
                })
                .clamp(0.0, 1.0)),
                "normal" => {
                    let n = dec_n(p, false);
                    let d = wp * 0.6;
                    let gx = g(&res_mask, u + 1.0 / s as f32, v) - g(&res_mask, u - 1.0 / s as f32, v);
                    let gy = g(&res_mask, u, v + 1.0 / hs as f32) - g(&res_mask, u, v - 1.0 / hs as f32);
                    enc_n([n[0] - gx * d * 4.0, n[1] - gy * d * 4.0, n[2]], false)
                }
                "ambientocclusion" => grey(p[0] * (1.0 - wp * 0.3)),
                _ => p,
            }
        })
    };
    emit(cx, &f, &|_| None)
}

// ------------------------------------------------------------------------------------------------ PBR helpers

fn pbr_base_material(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let seed = cx.seed;
    let use_in = |flag: &str, id: &str| if cx.b(flag) { cx.img(id) } else { None };
    let preset = cx.i("material_preset");
    // presets: 0 custom, then common metals (linear F0, sRGB-encoded)
    let metals: [[f32; 3]; 6] = [[0.972, 0.960, 0.915], [1.0, 0.766, 0.336], [0.955, 0.638, 0.538], [0.913, 0.922, 0.924], [0.562, 0.565, 0.578], [0.550, 0.556, 0.554]];
    let (base_col, metal_v) = if preset > 0 {
        let c = metals[((preset - 1) as usize).min(metals.len() - 1)];
        ([to_srgb(c[0]), to_srgb(c[1]), to_srgb(c[2]), 1.0], 1.0)
    } else {
        (cx.col("basecolor"), cx.f("metallic"))
    };
    let tiling = cx.i("grunge_tiling").max(1);
    let grunge_in = if cx.b("grunge_custom_input") { cx.img("grunge_input") } else { None };
    let grunge = |u: f32, v: f32| grunge_in.map(|t| g(t, u, v)).unwrap_or_else(|| noise(u, v, 4 * tiling, 6, seed.wrapping_add(5)));
    let (rough, rga, gloss, gga) = (cx.f("roughness"), cx.f("roughness_grunge_amount"), cx.f("glossiness"), cx.f("glossiness_grunge_amount"));
    let height_in = use_in("user_height", "height_input");
    let (hpos, hrange) = (cx.f("height_position"), cx.f("height_range"));
    let height = Tex::from_fn(s, hs, |u, v| grey(height_in.map(|t| (hpos - 0.5) + g(t, u, v) * hrange * 2.0 + (0.5 - hrange)).unwrap_or(hpos).clamp(0.0, 1.0)));
    let nfh = cx.f("normal_from_height_intensity");
    let f = |c: &'static str| -> Tex {
        match c {
            "diffuse" => match use_in("user_diffuse", "diffuse_input") {
                Some(t) => t.clone(),
                None => {
                    let d = cx.col("diffuse_color");
                    let k = 1.0 - metal_v;
                    Tex::solid(s, hs, [d[0] * k + base_col[0] * k * 0.0, d[1] * k, d[2] * k, 1.0])
                }
            },
            "basecolor" => use_in("user_basecolor", "basecolor_input").cloned().unwrap_or_else(|| Tex::solid(s, hs, base_col)),
            "normal" => match use_in("user_normal", "normal_input") {
                Some(t) => t.clone(),
                None => ops::normal_from_height(&height, nfh * 0.1, true),
            },
            "specular" => use_in("user_specular", "specular_input").cloned().unwrap_or_else(|| Tex::solid(s, hs, cx.col("specular"))),
            "glossiness" => match use_in("user_glossiness", "glossiness_input") {
                Some(t) => t.clone(),
                None => Tex::from_fn(s, hs, |u, v| grey((gloss + (grunge(u, v) - 0.5) * gga).clamp(0.0, 1.0))),
            },
            "roughness" => match use_in("user_roughness", "roughness_input") {
                Some(t) => t.clone(),
                None => Tex::from_fn(s, hs, |u, v| grey((rough + (grunge(u, v) - 0.5) * rga).clamp(0.0, 1.0))),
            },
            "metallic" => use_in("user_metallic", "metallic_input").cloned().unwrap_or_else(|| Tex::solid(s, hs, grey(metal_v))),
            "height" => height.clone(),
            "ambientocclusion" => use_in("user_ao", "ambientOcclusion_input").cloned().unwrap_or_else(|| Tex::solid(s, hs, grey(1.0))),
            _ => Tex::solid(s, hs, grey(1.0)),
        }
    };
    emit(cx, &f, &|_| None)
}

fn bitmap2material(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let rs = cx.rs();
    let src = cx.img_or_ref("bitmap_diffuse", grey(0.5));
    let metal_in = cx.img("metallic_1");
    let (hue, sat, bri, con) = (cx.f("Hue"), cx.f("Saturation"), cx.f("Brightness"), cx.f("Contrast"));
    let diffuse = src.map(|p| {
        let (h, sa, va) = ops::rgb_to_hsv(p[0], p[1], p[2]);
        let (r, gg, b) = ops::hsv_to_rgb((h + hue).rem_euclid(1.0), (sa * sat * 2.0).clamp(0.0, 1.0), (va + (bri - 0.5)).clamp(0.0, 1.0));
        let c = |x: f32| ((x - 0.5) * (1.0 + con) + 0.5).clamp(0.0, 1.0);
        [c(r), c(gg), c(b), p[3]]
    });
    // relief: a sum of band-passes of the brightness, weighted by the four-band equaliser (fine .. coarse)
    let gray = crate::nodes10::gray_of(&src);
    let eq = cx.v4("Relief_Graphic_Equalizer");
    let radii = [1.0f32, 4.0, 16.0, 64.0];
    let mut prev = gray.clone();
    let mut height = Tex::solid(s, hs, grey(0.5));
    for (b, r) in radii.iter().enumerate() {
        let low = ops::blur(&gray, r * rs);
        for (i, p) in height.px.iter_mut().enumerate() {
            let d = prev.px[i][0] - low.px[i][0];
            p[0] += d * eq[b] * 2.0;
        }
        prev = low;
    }
    let balance = cx.f("Depth_Balance");
    let inv = cx.b("Invert_Generated_Relief");
    for p in height.px.iter_mut() {
        let v = (p[0] + balance * 0.25).clamp(0.0, 1.0);
        let v = if inv { 1.0 - v } else { v };
        *p = grey(v);
    }
    let dx = cx.i("Output_Normal_Format") == 0;
    let normal = ops::normal_from_height(&height, cx.f("Normal_Strength") * 0.1, dx);
    let levels = |t: &Tex, id_in: &str, id_out: &str| -> Tex {
        let li = cx.v3(id_in);
        let lo = cx.v2(id_out);
        t.map(|p| {
            let x = ((p[0] - li[0]) / (li[2] - li[0]).max(1e-4)).clamp(0.0, 1.0);
            let gamma = ((li[1] - li[0]) / (li[2] - li[0]).max(1e-4)).clamp(0.01, 0.99);
            let x = x.powf((0.5f32).ln() / gamma.ln());
            grey(lerp(lo[0], lo[1], x))
        })
    };
    let f = |c: &'static str| -> Tex {
        match c {
            "diffuse" => diffuse.clone(),
            "normal" => normal.clone(),
            "specular" => levels(&gray, "Specular_Levels_In", "Specular_Levels_Out"),
            "glossiness" => levels(&gray, "glossiness_levels_in", "glossiness_levels_out"),
            "roughness" => levels(&gray, "roughness_levels_in", "roughness_levels_out"),
            "metallic" => metal_in.cloned().unwrap_or_else(|| Tex::solid(s, hs, grey(0.0))),
            "height" => height.clone(),
            "ambientocclusion" => {
                let ao = ops::ambient_occlusion(&height, cx.f("AO_Spread").max(0.05) * 12.0 * rs, 1.0 + cx.f("AO_Multiply") * 2.0);
                let lv = cx.v2("AO_Levels");
                ao.map(|p| grey(lerp(lv[0], lv[1], p[0])))
            }
            _ => Tex::solid(s, hs, grey(0.5)),
        }
    };
    emit(cx, &f, &|_| None)
}

/// Equirectangular environment direction -> radiance: the connected map, else a simple daylight sky.
fn sky(env: Option<&Tex>, d: [f32; 3], rot: f32) -> [f32; 3] {
    match env {
        Some(e) => {
            let u = (d[0].atan2(-d[2]) / TAU + 0.5 + rot).rem_euclid(1.0);
            let v = (d[1].clamp(-1.0, 1.0)).acos() / PI;
            let p = e.sample(u, v);
            [p[0], p[1], p[2]]
        }
        None => {
            let up = d[1];
            let zen = [0.35, 0.55, 0.95];
            let hor = [0.85, 0.9, 1.0];
            let ground = [0.25, 0.22, 0.2];
            let sun_d = crate::nodes10::norm3([0.5, 0.6, 0.6]);
            let sun = crate::nodes10::dot3(d, sun_d).max(0.0).powf(800.0) * 40.0;
            if up >= 0.0 {
                let t = up.powf(0.5);
                [lerp(hor[0], zen[0], t) + sun, lerp(hor[1], zen[1], t) + sun * 0.95, lerp(hor[2], zen[2], t) + sun * 0.85]
            } else {
                let t = (-up).powf(0.3).min(1.0);
                [lerp(hor[0] * 0.6, ground[0], t), lerp(hor[1] * 0.6, ground[1], t), lerp(hor[2] * 0.6, ground[2], t)]
            }
        }
    }
}

fn pbr_render(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let base = cx.img_or_ref("basecolor", grey(0.5));
    let nmap = cx.img_or_ref("normal", [0.5, 0.5, 1.0, 1.0]);
    let rough = cx.img_or_ref("roughness", grey(0.5));
    let metal = cx.img_or_ref("metallic", grey(0.0));
    let sl = cx.img("specularlevel");
    let em = cx.img("emissive");
    let ao = cx.img("ambient_occlusion");
    let op = cx.img("opacity");
    let env = cx.img("environment_map");
    let bg_in = cx.img("background_input");
    let dx = cx.i("normal_format") == 0;
    let shape = cx.i("shape");
    let scale = cx.f("shape_scale").max(0.05);
    let rot = cx.f("envrionment_rotation_horizontal");
    let exposure = 2f32.powf(cx.f("camera_exposure"));
    let bg_mode = cx.i("background_mode");
    let bg_col = cx.col("background_color");
    let sphere_uv = cx.v2("sphere_uv_scale");
    let plane_uv = cx.v2("plane_uv_scale");
    let cyl_uv = cx.v2("cylinder_uv_scale");
    let uv_off = cx.v2("uv_offset");
    let tiling = cx.f("uv_tiling").max(0.01);
    let f0d = cx.f("dielectric_f0");
    let ao_k = cx.f("ambient_occlusion_intensity");
    let em_k = if cx.b("enable_emissive_lighting") { cx.f("emissive_intensity") } else { 0.0 };
    let srot = cx.f("shape_rotation") * TAU;
    let env_mean = env.map(ops::mean_rgb).unwrap_or([0.6, 0.65, 0.75]);
    let aspect = s as f32 / hs as f32;
    let tone = cx.i("tone_mapper");
    let vignette = cx.f("vignette_intensity");
    // per pixel: (lit colour, irradiance, specular, world normal, tangent normal, uv)
    let res: Vec<[[f32; 4]; 6]> = crate::tex::par_map(s * hs, |i| {
        let (u, v) = (((i % s) as f32 + 0.5) / s as f32, ((i / s) as f32 + 0.5) / hs as f32);
        let (x, y) = (((u - 0.5) * 2.0 * aspect) / scale, ((0.5 - v) * 2.0) / scale);
        // hit point on the shape, its normal, tangent frame and texture coordinates
        let hit: Option<([f32; 3], [f32; 3], [f32; 3], [f32; 2])> = match shape {
            1 => {
                if x.abs() > 1.0 || y.abs() > 1.0 {
                    None
                } else {
                    Some(([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [(x * 0.5 + 0.5) * plane_uv[0], (0.5 - y * 0.5) * plane_uv[1]]))
                }
            }
            2 => {
                let r = cx.f("cylinder_radius").max(0.05) * 2.0;
                let len = cx.f("cylinder_length").max(0.05) * 1.6;
                if x.abs() > r || y.abs() > len * 0.5 {
                    None
                } else {
                    let z = (r * r - x * x).max(0.0).sqrt();
                    let n = [x / r, 0.0, z / r];
                    let a = (x / r).asin() + srot;
                    Some((n, [n[2], 0.0, -n[0]], [0.0, 1.0, 0.0], [(a / TAU + 0.5) * cyl_uv[0], (0.5 - y / len) * cyl_uv[1]]))
                }
            }
            _ => {
                let r2 = x * x + y * y;
                if r2 > 1.0 {
                    None
                } else {
                    let z = (1.0 - r2).sqrt();
                    let n = [x, y, z];
                    let lon = x.atan2(z) + srot;
                    let lat = y.asin();
                    let t = crate::nodes10::norm3([lon.cos(), 0.0, -lon.sin()]);
                    let b = crate::nodes10::cross3(n, t);
                    Some((n, t, b, [(lon / TAU + 0.5) * sphere_uv[0], (0.5 - lat / PI) * sphere_uv[1]]))
                }
            }
        };
        let Some((n0, t, b, uv)) = hit else {
            let d = crate::nodes10::norm3([x * 0.4, y * 0.4, -1.0]);
            let bg = match (bg_mode, bg_in) {
                (2, Some(bi)) => bi.sample(u, v),
                (0, _) => bg_col,
                _ => {
                    let c = sky(env, [-d[0], d[1], -d[2]], rot);
                    [c[0], c[1], c[2], 1.0]
                }
            };
            return [bg, [0.0; 4], [0.0; 4], [0.5, 0.5, 1.0, 0.0], [0.5, 0.5, 1.0, 0.0], [0.0; 4]];
        };
        let (tu, tv) = (uv[0] * tiling + uv_off[0], uv[1] * tiling + uv_off[1]);
        let tn = dec_n(nmap.sample(tu, tv), dx);
        let n = crate::nodes10::norm3([t[0] * tn[0] + b[0] * tn[1] + n0[0] * tn[2], t[1] * tn[0] + b[1] * tn[1] + n0[1] * tn[2], t[2] * tn[0] + b[2] * tn[1] + n0[2] * tn[2]]);
        let bc = base.sample(tu, tv);
        let bl = [to_lin(bc[0]), to_lin(bc[1]), to_lin(bc[2])];
        let r = g(&rough, tu, tv).clamp(0.02, 1.0);
        let m = g(&metal, tu, tv).clamp(0.0, 1.0);
        let f0s = sl.map(|t| g(t, tu, tv) * 0.08).unwrap_or(f0d);
        let f0 = [lerp(f0s, bl[0], m), lerp(f0s, bl[1], m), lerp(f0s, bl[2], m)];
        let vdir = [0.0, 0.0, 1.0];
        let ndv = crate::nodes10::dot3(n, vdir).max(1e-3);
        let refl = crate::nodes10::sub3(crate::nodes10::mul3(n, 2.0 * ndv), vdir);
        let irr_up = sky(env, [n[0], n[1], -n[2]], rot);
        let irr = [lerp(irr_up[0], env_mean[0], 0.6), lerp(irr_up[1], env_mean[1], 0.6), lerp(irr_up[2], env_mean[2], 0.6)];
        let spec_env = sky(env, [refl[0], refl[1], -refl[2]], rot);
        let spec_env = [lerp(spec_env[0], env_mean[0], r * r), lerp(spec_env[1], env_mean[1], r * r), lerp(spec_env[2], env_mean[2], r * r)];
        let fres = (1.0 - ndv).powi(5);
        let a = ao.map(|t| lerp(1.0, g(t, tu, tv), ao_k.max(0.5))).unwrap_or(1.0);
        let e = em.map(|t| t.sample(tu, tv)).unwrap_or([0.0; 4]);
        let mut col = [0.0f32; 4];
        let mut spec = [0.0f32; 4];
        for c in 0..3 {
            let f = f0[c] + (1.0 - f0[c]) * fres * (1.0 - r);
            let sp = spec_env[c] * f;
            let d = bl[c] * (1.0 - m) * irr[c];
            spec[c] = sp;
            col[c] = ((d + sp) * a + to_lin(e[c]) * em_k) * exposure;
        }
        let alpha = op.map(|t| g(t, tu, tv)).unwrap_or(1.0);
        col[3] = alpha;
        spec[3] = 1.0;
        let wn = [n[0] * 0.5 + 0.5, n[1] * 0.5 + 0.5, n[2] * 0.5 + 0.5, 1.0];
        let tnp = enc_n(tn, false);
        [col, [irr[0], irr[1], irr[2], 1.0], spec, wn, tnp, [crate::noise::wrap01(tu), crate::noise::wrap01(tv), 0.0, 1.0]]
    });
    let mut outs: Vec<Vec<[f32; 4]>> = vec![Vec::with_capacity(s * hs); 6];
    for (i, px) in res.into_iter().enumerate() {
        let (u, v) = (((i % s) as f32 + 0.5) / s as f32, ((i / s) as f32 + 0.5) / hs as f32);
        let vig = 1.0 - vignette * 0.5 * (((u - 0.5) * 2.0).powi(2) + ((v - 0.5) * 2.0).powi(2)).min(1.0);
        let mut c = px[0];
        for k in 0..3 {
            let x = c[k] * vig;
            // tone mapping: 0 linear clamp, others a filmic curve
            let t = if tone == 0 { x.clamp(0.0, 1.0) } else { (x * (2.51 * x + 0.03) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0) };
            c[k] = to_srgb(t);
        }
        outs[0].push([c[0], c[1], c[2], 1.0]);
        for (k, o) in outs.iter_mut().enumerate().skip(1) {
            o.push(px[k]);
        }
    }
    let all: Vec<Option<Tex>> = outs.into_iter().map(|px| Some(Tex { w: s, h: hs, px })).collect();
    cx.pick(all)
}

/// Atlas Splitter and Atlas Scatter: the shapes of an atlas are found from its opacity (or height), then one is cut
/// out (splitter) or all are scattered on a grid (scatter).
fn atlas(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let seed = cx.seed;
    let m = Mat::read(cx, &["", "basecolor", "normal", "", "", "", "roughness", "metallic", "", "height", "", "opacity"]);
    let users = [cx.img("user1"), cx.img("user2")];
    let key = match cx.img("opacity").filter(|t| !is_blank(t)) {
        Some(o) => o.clone(),
        None => match cx.img("height").filter(|t| !is_blank(t)) {
            Some(h) => h.clone(),
            None => Tex::solid(s, hs, grey(1.0)),
        },
    };
    // shapes: connected parts of the key above a small threshold, as a flood-fill picture
    let mask = key.map(|p| grey(if p[0] > 0.02 { 1.0 } else { 0.0 }));
    let ff = ops::flood_fill(&mask, 0.5);
    let (label, stats) = ops::flood_regions(&ff);
    let min_size = if cx.kind == "x_atlas_spliter" { cx.f("ignore_shape_smaller_than") } else { cx.f("pattern_ignore_shape_smaller_than") };
    let shapes: Vec<usize> = (1..stats.len()).filter(|&k| stats[k].1[0].max(stats[k].1[1]) >= min_size * 0.25).collect();
    let count = shapes.len();
    let (lw, lh) = (mask.w, mask.h);
    // sample channel `t` of shape `k` at local (0..1) coordinates inside its bounding box: the bilinear mix of the
    // four nearest atlas pixels that belong to the shape, and how much of the mix they make up (smooth edges
    // when a shape is enlarged; neighbouring shapes and the background never bleed in)
    let fetch = |t: &Tex, k: usize, lu: f32, lv: f32| -> Option<([f32; 4], f32)> {
        let st = &stats[k];
        let (u, v) = (st.0[0] + (lu - 0.5) * st.1[0], st.0[1] + (lv - 0.5) * st.1[1]);
        let (fx, fy) = (crate::noise::wrap01(u) * lw as f32 - 0.5, crate::noise::wrap01(v) * lh as f32 - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (mut acc, mut cov) = ([0.0f32; 4], 0.0f32);
        for (dx, dy, w) in [(0i64, 0i64, (1.0 - tx) * (1.0 - ty)), (1, 0, tx * (1.0 - ty)), (0, 1, (1.0 - tx) * ty), (1, 1, tx * ty)] {
            let x = (x0 as i64 + dx).rem_euclid(lw as i64) as usize;
            let y = (y0 as i64 + dy).rem_euclid(lh as i64) as usize;
            if w <= 0.0 || label.get(y * lw + x).copied().unwrap_or(0) as usize != k {
                continue;
            }
            let p = t.sample((x as f32 + 0.5) / lw as f32, (y as f32 + 0.5) / lh as f32);
            for c in 0..4 {
                acc[c] += p[c] * w;
            }
            cov += w;
        }
        if cov <= 1e-4 {
            return None;
        }
        // at the border, the outline follows the key's own (interpolated) contour rather than the pixel steps
        let edge = if cov < 0.999 { ((key.sample(u, v)[0] - 0.02) / 0.03).clamp(0.0, 1.0) } else { 1.0 };
        if edge <= 0.0 {
            return None;
        }
        Some(([acc[0] / cov, acc[1] / cov, acc[2] / cov, acc[3] / cov], edge))
    };
    let chan: [(&str, &'static str); 6] = [("basecolor", "basecolor"), ("normal", "normal"), ("roughness", "roughness"), ("metallic", "metallic"), ("height", "height"), ("opacity", "opacity")];
    if cx.kind == "x_atlas_spliter" {
        let sel = (cx.i("shape_selection").max(1) as usize).min(count.max(1));
        let k = shapes.get(sel - 1).copied();
        let auto_scale = cx.b("auto_scale");
        let rot = cx.f("rotation");
        let bg = |o: &str| -> [f32; 4] {
            match o {
                "basecolor" => cx.col("basecolor_bg_color"),
                "normal" => cx.col("normal_bg_color"),
                "metallic" => grey(cx.f("metallic_bg_color")),
                "roughness" => grey(cx.f("roughness_bg_color")),
                "height" => grey(cx.f("height_bg_color")),
                "user1" => grey(cx.f("user1_bg_color")),
                "user2" => grey(cx.f("user2_bg_color")),
                _ => grey(0.0),
            }
        };
        let custom_bg = cx.b("custom_background_color");
        let cut = |o: &str, t: Option<Tex>| -> Tex {
            let fill = if custom_bg || o == "normal" { bg(o) } else { grey(0.0) };
            let (Some(k), Some(t)) = (k, t) else { return Tex::solid(s, hs, fill) };
            let st = &stats[k];
            let fit = if auto_scale { 1.0 } else { st.1[0].max(st.1[1]).max(1e-3) };
            Tex::from_fn(s, hs, |u, v| {
                let (a, b) = (u - 0.5, v - 0.5);
                let (sn, cs) = (rot * TAU).sin_cos();
                let (a, b) = (a * cs - b * sn, a * sn + b * cs);
                let (lu, lv) = (a / fit + 0.5, b / fit + 0.5);
                if !(0.0..=1.0).contains(&lu) || !(0.0..=1.0).contains(&lv) {
                    return fill;
                }
                match fetch(&t, k, lu, lv) {
                    Some((p, c)) => lerp4(fill, p, c),
                    None => fill,
                }
            })
        };
        let outs: Vec<&'static str> = cx.outs.to_vec();
        let all: Vec<Option<Tex>> = outs
            .iter()
            .enumerate()
            .map(|(i, o)| {
                if !cx.wants(i) {
                    return None;
                }
                Some(match *o {
                    "shape_count" => Tex::solid(s, hs, grey(count as f32 / 255.0)),
                    "user1" => cut(o, users[0].cloned()),
                    "user2" => cut(o, users[1].cloned()),
                    "opacity" => cut(o, Some(key.clone())),
                    other => {
                        let c = chan.iter().find(|(n, _)| *n == other).map(|(_, c)| *c).unwrap_or("basecolor");
                        cut(other, Some(m.get(c)))
                    }
                })
            })
            .collect();
        return cx.pick(all);
    }
    // scatter
    let (xa, ya) = (cx.i("x_amount").clamp(1, 64), cx.i("y_amount").clamp(1, 64));
    let (scale, scale_rand) = (cx.f("shape_scale"), cx.f("shape_scale_random"));
    let pos_rand = cx.f("position_random");
    let (srot, srot_rand) = (cx.f("shape_rotation"), cx.f("shape_rotation_random"));
    let (hscale, hoff, hscale_rand, hoff_rand) = (cx.f("height_scale"), cx.f("height_offset"), cx.f("height_scale_random"), cx.f("height_offset_random"));
    let mask_rand = cx.f("mask_random");
    let range = (cx.lane("pattern_range", 0).max(0.0) as usize, cx.lane("pattern_range", 1).max(0.0) as usize);
    let pool: Vec<usize> = if range.1 > 0 && range.1 >= range.0 { shapes.iter().copied().enumerate().filter(|(i, _)| *i >= range.0 && *i <= range.1).map(|(_, k)| k).collect() } else { shapes.clone() };
    let bgm = Mat::read(cx, &["", "background_basecolor", "background_normal", "", "", "", "background_roughness", "background_metallic", "", "background_height", "", ""]);
    let atlas_h = m.get("height");
    let atlas: HashMap<&'static str, Tex> = chan.iter().map(|(_, c)| (*c, m.get(c))).collect();
    // every pixel: the highest shape instance covering it (cell, shape, local uv, height)
    let hits: Vec<Option<(usize, f32, f32, f32, f32, f32)>> = crate::tex::par_map(s * hs, |i| {
        let (u, v) = (((i % s) as f32 + 0.5) / s as f32, ((i / s) as f32 + 0.5) / hs as f32);
        if pool.is_empty() {
            return None;
        }
        let (gx, gy) = (u * xa as f32, v * ya as f32);
        let mut best: Option<(usize, f32, f32, f32, f32, f32)> = None;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (cxi, cyi) = (gx.floor() as i32 + dx, gy.floor() as i32 + dy);
                let (wx, wy) = (cxi.rem_euclid(xa), cyi.rem_euclid(ya));
                let r = |k: i32| rnd(wx * 131 + wy, k, seed.wrapping_add(907));
                if r(9) < mask_rand {
                    continue;
                }
                let k = pool[((r(1) * pool.len() as f32) as usize).min(pool.len() - 1)];
                let centre = [(cxi as f32 + 0.5 + (r(2) - 0.5) * pos_rand) / xa as f32, (cyi as f32 + 0.5 + (r(3) - 0.5) * pos_rand) / ya as f32];
                let st = &stats[k];
                let sc = scale * (1.0 - r(4) * scale_rand) / (xa.max(ya) as f32) / st.1[0].max(st.1[1]).max(1e-3);
                let a = (srot + (r(5) - 0.5) * srot_rand) * TAU;
                let (du, dv) = (u - centre[0], v - centre[1]);
                let (du, dv) = (ops::wrapd(du), ops::wrapd(dv));
                let (ru, rv) = (du * a.cos() + dv * a.sin(), -du * a.sin() + dv * a.cos());
                let (lu, lv) = (ru / (sc * st.1[0].max(1e-4)) + 0.5, rv / (sc * st.1[1].max(1e-4)) + 0.5);
                if !(0.0..=1.0).contains(&lu) || !(0.0..=1.0).contains(&lv) {
                    continue;
                }
                let Some((hp, cov)) = fetch(&atlas_h, k, lu, lv) else { continue };
                let hv = hp[0] * hscale * (1.0 - r(6) * hscale_rand) + hoff + (r(7) - 0.5) * hoff_rand;
                if best.map(|b| hv > b.4).unwrap_or(true) {
                    best = Some((k, lu, lv, a, hv, cov));
                }
            }
        }
        best
    });
    let outs: Vec<&'static str> = cx.outs.to_vec();
    let all: Vec<Option<Tex>> = outs
        .iter()
        .enumerate()
        .map(|(oi, o)| {
            if !cx.wants(oi) {
                return None;
            }
            let px: Vec<[f32; 4]> = (0..s * hs)
                .map(|i| {
                    let (u, v) = (((i % s) as f32 + 0.5) / s as f32, ((i / s) as f32 + 0.5) / hs as f32);
                    let hit = hits[i];
                    match *o {
                        "shape_count" => grey(count as f32 / 255.0),
                        "mask" => grey(hit.map(|h| h.5).unwrap_or(0.0)),
                        "height" => {
                            let bh = bgm.at("height", u, v)[0];
                            grey(hit.map(|h| lerp(bh, h.4.max(bh), h.5)).unwrap_or(bh))
                        }
                        other => {
                            let c = chan.iter().find(|(n, _)| *n == other).map(|(_, c)| *c).unwrap_or("basecolor");
                            match hit {
                                Some((k, lu, lv, a, _, cov)) => {
                                    let bg = bgm.at(c, u, v);
                                    let p = atlas.get(c).and_then(|t| fetch(t, k, lu, lv)).map(|(p, _)| p).unwrap_or(bg);
                                    let p = if c == "normal" {
                                        let n = dec_n(p, false);
                                        let (sa, ca) = a.sin_cos();
                                        enc_n([n[0] * ca - n[1] * sa, n[0] * sa + n[1] * ca, n[2]], false)
                                    } else {
                                        p
                                    };
                                    lerp4(bg, p, cov)
                                }
                                None => bgm.at(c, u, v),
                            }
                        }
                    }
                })
                .collect();
            Some(Tex { w: s, h: hs, px })
        })
        .collect();
    cx.pick(all)
}

// ------------------------------------------------------------------------------------------------ material presets

/// MaterialCraft's own procedural versions of the library's material presets: height, colour and roughness made
/// from our generators, the other channels derived from them.
fn preset(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let rs = cx.rs();
    let seed = cx.seed.wrapping_add(1000);
    let k = cx.kind;
    let n2 = |u: f32, v: f32, c: i32, o: u32, salt: u32| noise(u, v, c, o, seed.wrapping_add(salt));
    // height, base colour (sRGB), roughness and metallic per pixel
    let sample = |u: f32, v: f32| -> (f32, [f32; 3], f32, f32) {
        match k {
            "x_bricks_001" | "x_bricks_005" => {
                let (rows, cols, base) = if k == "x_bricks_001" { (12.0, 4.0, [0.55, 0.24, 0.16]) } else { (16.0, 6.0, [0.72, 0.70, 0.66]) };
                let b = ops::bricks(u, v, rows, cols, 0.06, 0.05, 0.5, 0.25, seed);
                let grain = n2(u, v, 32, 4, 1);
                let id = rnd((u * cols) as i32, (v * rows) as i32, seed);
                let h = if b > 0.0 { 0.55 + b * 0.35 + (grain - 0.5) * 0.12 } else { 0.25 + grain * 0.1 };
                let mortar = [0.62, 0.6, 0.56];
                let c = if b > 0.0 { [base[0] * (0.8 + id * 0.4) * (0.85 + grain * 0.3), base[1] * (0.8 + id * 0.4) * (0.85 + grain * 0.3), base[2] * (0.8 + id * 0.3) * (0.85 + grain * 0.3)] } else { mortar };
                (h, c, if b > 0.0 { 0.8 } else { 0.92 }, 0.0)
            }
            "x_cardboard_001" | "x_paper_005" => {
                let fibers = n2(u * 4.0, v, 64, 3, 2) * 0.5 + n2(u, v, 128, 2, 3) * 0.5;
                let corr = if k == "x_cardboard_001" { ((v * 48.0 * TAU).sin() * 0.5 + 0.5) * 0.15 } else { 0.0 };
                let h = 0.5 + (fibers - 0.5) * 0.1 + corr;
                let c = if k == "x_cardboard_001" { [0.62 + fibers * 0.08, 0.47 + fibers * 0.06, 0.3 + fibers * 0.04] } else { [0.9 + fibers * 0.05, 0.89 + fibers * 0.05, 0.85 + fibers * 0.04] };
                (h, c, 0.85, 0.0)
            }
            "x_concrete_002" | "x_concrete_006" | "x_concrete_010" | "x_concrete_011" | "x_concrete_044" | "x_concrete_070" | "x_rough_concrete_with_lines" => {
                let (tone, rough, pores, agg) = match k {
                    "x_concrete_002" => (0.62, 0.8, 0.6, 0.2),
                    "x_concrete_006" => (0.55, 0.9, 0.3, 0.8),
                    "x_concrete_010" => (0.6, 0.85, 0.4, 0.4),
                    "x_concrete_011" => (0.5, 0.88, 0.5, 0.5),
                    "x_concrete_044" => (0.68, 0.55, 0.2, 0.3),
                    "x_concrete_070" => (0.38, 0.92, 0.6, 0.6),
                    _ => (0.58, 0.9, 0.4, 0.4),
                };
                let base = n2(u, v, 6, 6, 4);
                let fine = n2(u, v, 96, 3, 5);
                let vr = crate::noise::voronoi(u, v, 48, 48, seed.wrapping_add(6), 1.0);
                let aggregate = smooth(0.3, 0.0, vr.0) * agg;
                let pit = smooth(0.72, 0.8, n2(u, v, 128, 1, 7)) * pores;
                let mut h = 0.5 + (base - 0.5) * 0.2 + (fine - 0.5) * 0.1 + aggregate * 0.15 - pit * 0.3;
                if k == "x_rough_concrete_with_lines" || k == "x_concrete_010" {
                    // form-work lines / slab seams
                    let rows = if k == "x_concrete_010" { 4.0 } else { 8.0 };
                    let fv = (v * rows).fract();
                    let line = smooth(0.012, 0.0, fv.min(1.0 - fv));
                    let fu = (u * if k == "x_concrete_010" { 4.0 } else { 1.0 }).fract();
                    let seam = if k == "x_concrete_010" { smooth(0.012, 0.0, fu.min(1.0 - fu)) } else { 0.0 };
                    h -= line.max(seam) * 0.35;
                }
                let stain = n2(u, v, 4, 5, 8);
                let t = tone * (0.85 + base * 0.25) * (0.92 + fine * 0.12) * (1.0 - (stain - 0.5).max(0.0) * 0.25);
                (h.clamp(0.0, 1.0), [t, t * 0.99, t * 0.96], (rough + (fine - 0.5) * 0.1).clamp(0.0, 1.0), 0.0)
            }
            "x_fabric_002" | "x_fabric_009" | "x_fabric_025" => {
                let (threads, col) = match k {
                    "x_fabric_002" => (48.0, [0.78, 0.72, 0.6]),
                    "x_fabric_009" => (64.0, [0.18, 0.27, 0.45]),
                    _ => (32.0, [0.55, 0.12, 0.12]),
                };
                let (x, y) = (u * threads, v * threads);
                let (cxi, cyi) = (x.floor() as i32, y.floor() as i32);
                let over = (cxi + cyi).rem_euclid(2) == 0;
                let (fx, fy) = (x.fract(), y.fract());
                let warp = (fx * PI).sin();
                let weft = (fy * PI).sin();
                let h = if over { 0.5 + warp * 0.4 } else { 0.5 + weft * 0.4 };
                let fuzz = n2(u, v, 256, 2, 9);
                let shade = 0.8 + h * 0.3 + (fuzz - 0.5) * 0.1;
                (h, [col[0] * shade, col[1] * shade, col[2] * shade], 0.9, 0.0)
            }
            "x_gravel" => {
                let vr = crate::noise::voronoi_ext(u, v, 24, 24, seed.wrapping_add(10), 1.0);
                let dome = (1.0 - (vr.f1 * 2.2).min(1.0)).sqrt();
                let gap = smooth(0.0, 0.12, vr.f2 - vr.f1);
                let id = vr.id1;
                let h = dome * gap * (0.7 + id * 0.3);
                let t = 0.35 + id * 0.3;
                let tint = rnd((id * 10000.0) as i32, 1, seed);
                (h, [t * (0.95 + tint * 0.15), t * (0.92 + tint * 0.1), t * 0.85], 0.75 + (1.0 - gap) * 0.2, 0.0)
            }
            "x_metal_002" | "x_metal_003" | "x_metal_006" => {
                let (col, rough) = match k {
                    "x_metal_002" => ([0.78, 0.78, 0.79], 0.35),
                    "x_metal_003" => ([0.66, 0.66, 0.68], 0.45),
                    _ => ([0.72, 0.7, 0.68], 0.3),
                };
                let (h, r) = match k {
                    "x_metal_002" => {
                        // brushed: noise stretched along x
                        let b = n2(u * 0.02, v, 256, 3, 11);
                        (0.5 + (b - 0.5) * 0.08, rough + (b - 0.5) * 0.15)
                    }
                    "x_metal_003" => {
                        let vr = crate::noise::voronoi(u, v, 20, 20, seed.wrapping_add(12), 1.0);
                        let dent = (vr.0 * 1.8).min(1.0);
                        (0.4 + dent * 0.3, rough + (1.0 - dent) * 0.1)
                    }
                    _ => {
                        let mut buf = 0.0;
                        for i in 0..40 {
                            let rr = |kk: i32| rnd(i, kk, seed.wrapping_add(13));
                            let a = rr(1) * TAU;
                            let (ox, oy) = (rr(2), rr(3));
                            let d = ops::wrapd(u - ox) * a.sin() - ops::wrapd(v - oy) * a.cos();
                            let along = ops::wrapd(u - ox) * a.cos() + ops::wrapd(v - oy) * a.sin();
                            if along.abs() < 0.2 * rr(4) + 0.05 {
                                buf += smooth(0.0015, 0.0, d.abs()) * rr(5);
                            }
                        }
                        (0.5 - buf.min(1.0) * 0.1, rough + buf.min(1.0) * 0.2)
                    }
                };
                (h, col, r.clamp(0.0, 1.0), 1.0)
            }
            "x_metal_plate_001" | "x_metal_plate_004" => {
                if k == "x_metal_plate_004" {
                    // tread plate: raised diagonal lozenges
                    let (x, y) = (u * 12.0, v * 12.0);
                    let (cxi, cyi) = (x.floor(), y.floor());
                    let (fx, fy) = (x - cxi - 0.5, y - cyi - 0.5);
                    let alt = ((cxi + cyi) as i32).rem_euclid(2) == 0;
                    let (a, b) = if alt { (fx + fy, fx - fy) } else { (fx - fy, fx + fy) };
                    let loz = (1.0 - ((a / 0.55).powi(2) + (b / 0.12).powi(2))).max(0.0).sqrt();
                    let n = n2(u, v, 64, 3, 14);
                    (0.4 + loz * 0.4 + (n - 0.5) * 0.03, [0.6, 0.61, 0.62], 0.35 + (1.0 - loz) * 0.15, 1.0)
                } else {
                    let (x, y) = (u * 2.0, v * 2.0);
                    let (fx, fy) = (x.fract(), y.fract());
                    let seam = smooth(0.008, 0.0, fx.min(1.0 - fx).min(fy.min(1.0 - fy)));
                    let mut rivet = 0.0f32;
                    for (rx, ry) in [(0.06, 0.06), (0.94, 0.06), (0.06, 0.94), (0.94, 0.94), (0.5, 0.06), (0.5, 0.94), (0.06, 0.5), (0.94, 0.5)] {
                        let d = ((fx - rx).powi(2) + (fy - ry).powi(2)).sqrt();
                        rivet = rivet.max((1.0 - (d / 0.025).powi(2)).max(0.0).sqrt());
                    }
                    let n = n2(u, v, 48, 4, 15);
                    let id = rnd(x.floor() as i32, y.floor() as i32, seed);
                    (0.5 - seam * 0.4 + rivet * 0.3 + (n - 0.5) * 0.04, [0.55 + id * 0.08, 0.56 + id * 0.08, 0.58 + id * 0.08], 0.4 + (n - 0.5) * 0.2, 1.0)
                }
            }
            "x_old_painted_planks" | "x_wood_american_cherry" => {
                let planks = if k == "x_wood_american_cherry" { 5.0 } else { 6.0 };
                let x = u * planks;
                let id = x.floor();
                let fx = x - id;
                let off = rnd(id as i32, 1, seed) * 7.0;
                let grain_v = v + off;
                let warp = n2(u, grain_v, 8, 3, 16) * 0.6;
                let rings = ((fx * 6.0 + warp * 4.0 + n2(fx * 0.2 + id * 0.37, grain_v * 0.1, 4, 2, 17) * 3.0) * TAU).sin() * 0.5 + 0.5;
                let fibre = n2(u * 4.0, grain_v * 0.25, 128, 2, 18);
                let gap = smooth(0.01, 0.0, fx.min(1.0 - fx));
                let wood = [0.42 + rings * 0.12 + fibre * 0.05, 0.2 + rings * 0.07 + fibre * 0.03, 0.12 + rings * 0.04];
                let wood_h = 0.5 + (rings - 0.5) * 0.06 + (fibre - 0.5) * 0.04 - gap * 0.4;
                if k == "x_wood_american_cherry" {
                    (wood_h, wood, 0.45 + (fibre - 0.5) * 0.1, 0.0)
                } else {
                    let paint = smooth(0.42, 0.5, n2(u, v, 10, 6, 19) + (rings - 0.5) * 0.1);
                    let col = [0.3, 0.46, 0.42];
                    let worn = [wood[0] * 0.8, wood[1] * 0.8, wood[2] * 0.8];
                    let c = [lerp(worn[0], col[0], paint), lerp(worn[1], col[1], paint), lerp(worn[2], col[2], paint)];
                    (wood_h + paint * 0.04, c, lerp(0.8, 0.6, paint), 0.0)
                }
            }
            _ => {
                // tiles
                let (n, grout, col, hex) = match k {
                    "x_tiles_002" => (4.0, 0.03, [0.9, 0.9, 0.88], false),
                    "x_tiles_008" => (16.0, 0.08, [0.3, 0.5, 0.6], false),
                    "x_tiles_011" => (8.0, 0.06, [0.85, 0.83, 0.78], true),
                    _ => (6.0, 0.05, [0.66, 0.35, 0.22], false),
                };
                let (d, id) = if hex {
                    // pointy hexagons on a grid that tiles: n columns, 2n/sqrt(3) rounded rows of centres
                    let rows = ((n * 2.0 / 3f32.sqrt()).round() / 2.0).max(1.0) * 2.0;
                    let (x, y) = (u * n, v * rows * 0.75 * 2.0 / 1.5);
                    let mut best = (f32::MAX, f32::MAX, 0.0f32);
                    let r0 = y.floor() as i32;
                    for ry in r0 - 1..=r0 + 1 {
                        let shift = if ry.rem_euclid(2) == 1 { 0.5 } else { 0.0 };
                        let c0 = (x - shift).floor() as i32;
                        for cxi in c0 - 1..=c0 + 1 {
                            let (px, py) = (cxi as f32 + 0.5 + shift, ry as f32 + 0.5);
                            let (dx, dy) = (x - px, (y - py) * 0.866);
                            let dd = (dx * dx + dy * dy).sqrt();
                            if dd < best.0 {
                                best = (dd, best.0, rnd(cxi.rem_euclid(n as i32), ry.rem_euclid(rows as i32), seed));
                            } else if dd < best.1 {
                                best.1 = dd;
                            }
                        }
                    }
                    ((best.1 - best.0) * 0.5, best.2)
                } else {
                    let (x, y) = (u * n, v * n);
                    let (fx, fy) = (x.fract(), y.fract());
                    (fx.min(1.0 - fx).min(fy.min(1.0 - fy)), rnd(x.floor() as i32, y.floor() as i32, seed))
                };
                let tile = smooth(grout * 0.5, grout, d);
                let bevel = smooth(grout, grout * 2.5, d);
                let nn = n2(u, v, 64, 3, 20);
                let var = 0.92 + id * 0.12;
                let c = if tile > 0.5 { [col[0] * var, col[1] * var, col[2] * var] } else { [0.55 + nn * 0.1, 0.54 + nn * 0.1, 0.5 + nn * 0.08] };
                (0.3 + tile * 0.3 + bevel * 0.2 + (nn - 0.5) * 0.03, c, if tile > 0.5 { 0.2 + nn * 0.1 } else { 0.9 }, 0.0)
            }
        }
    };
    // one pass for every channel source, then derived maps
    let data: Vec<(f32, [f32; 3], f32, f32)> = crate::tex::par_map(s * hs, |i| {
        let (u, v) = (((i % s) as f32 + 0.5) / s as f32, ((i / s) as f32 + 0.5) / hs as f32);
        let (h, c, r, m) = sample(u, v);
        (h, c, r, m)
    });
    let height = Tex { w: s, h: hs, px: data.iter().map(|d| grey(d.0.clamp(0.0, 1.0))).collect() };
    let base = Tex { w: s, h: hs, px: data.iter().map(|d| [d.1[0].clamp(0.0, 1.0), d.1[1].clamp(0.0, 1.0), d.1[2].clamp(0.0, 1.0), 1.0]).collect() };
    let rough = Tex { w: s, h: hs, px: data.iter().map(|d| grey(d.2.clamp(0.0, 1.0))).collect() };
    let metal = Tex { w: s, h: hs, px: data.iter().map(|d| grey(d.3)).collect() };
    let f = |c: &'static str| -> Tex {
        match c {
            "basecolor" => base.clone(),
            "diffuse" => Tex { w: s, h: hs, px: base.px.iter().zip(&metal.px).map(|(b, m)| [b[0] * (1.0 - m[0]), b[1] * (1.0 - m[0]), b[2] * (1.0 - m[0]), 1.0]).collect() },
            "normal" => ops::normal_from_height(&height, 1.2, true),
            "specular" => Tex { w: s, h: hs, px: base.px.iter().zip(&metal.px).map(|(b, m)| [lerp(0.22, b[0], m[0]), lerp(0.22, b[1], m[0]), lerp(0.22, b[2], m[0]), 1.0]).collect() },
            "glossiness" => rough.map(|p| grey(1.0 - p[0])),
            "roughness" => rough.clone(),
            "metallic" => metal.clone(),
            "height" => height.clone(),
            "ambientocclusion" => ops::ambient_occlusion(&height, 6.0 * rs, 1.0),
            _ => Tex::solid(s, hs, grey(1.0)),
        }
    };
    emit(cx, &f, &|_| None)
}
