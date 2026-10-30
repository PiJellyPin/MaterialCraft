// SPDX-License-Identifier: MIT OR Apache-2.0
//! Adapters from standard library graphs (as used by `.sbs` files) to MaterialCraft's own nodes.
//!
//! A file only stores the parameters someone changed. The adapter sees every parameter of the library graph: the
//! file's value when there is one, the library default otherwise (from [`crate::sbs_lib`]), and translates them into
//! the parameters of our node, so an imported graph starts from the same settings it was made with. Each adapter
//! also says which of our inputs and outputs the graph's image inputs and outputs map to.
//!
//! Only interface facts are used: names, types, defaults and ranges of the inputs. How the original graphs are built
//! is never needed; the adapters translate values into our own nodes' terms.

use crate::sbs::{FuncGraph, PValue, Param, Val};
use crate::sbs_import::{Bind, Conv, Level};
use crate::sbs_lib::LGraph;

/// Base parameters every node carries; they are handled by the importer, not by adapters.
pub(crate) const BASE_PARAMS: &[&str] = &["outputsize", "format", "pixelsize", "pixelratio", "tiling", "quality"];

pub(crate) struct Ad<'a> {
    pub kind: &'static str,
    pub lib: Option<&'static LGraph>,
    params: &'a [Param],
    pub c: Conv,
    used: Vec<String>,
    outs: Vec<(String, String)>,
    imgs: Vec<(String, String)>,
}

impl<'a> Ad<'a> {
    pub fn new(kind: &'static str, id: &'a str, lib: Option<&'static LGraph>, params: &'a [Param]) -> Ad<'a> {
        let mut c = Conv::new(kind, Level::Approx);
        c.notes.push((Level::Approx, format!("library node '{id}' uses MaterialCraft's own {} node", crate::graph::label(kind))));
        Ad { kind, lib, params, c, used: Vec::new(), outs: Vec::new(), imgs: Vec::new() }
    }

    fn param(&self, sd: &str) -> Option<&'a Param> {
        // exact name first; a different spelling only when it is not one of the node's base parameters
        // (a graph input "Tiling" is not the base parameter "tiling")
        self.params.iter().find(|p| p.name == sd).or_else(|| self.params.iter().find(|p| p.name.eq_ignore_ascii_case(sd) && !BASE_PARAMS.contains(&p.name.as_str())))
    }

    /// The library graph declares input `sd`.
    pub fn decl(&self, sd: &str) -> bool {
        self.lib.and_then(|l| l.input(sd)).is_some()
    }

    /// Library default of `sd`, lane `i` (0 when the library does not declare it).
    pub fn def(&self, sd: &str, i: usize) -> f32 {
        self.lib.and_then(|l| l.input(sd)).map(|x| x.lane(i)).unwrap_or(0.0)
    }

    fn lanes_of(&self, sd: &str) -> usize {
        self.lib.and_then(|l| l.input(sd)).map(|x| x.ty.lanes()).unwrap_or(1).max(1)
    }

    /// Value of `sd`, lane `i`: the file's constant, else the library default.
    pub fn v(&self, sd: &str, i: usize) -> f32 {
        match self.param(sd).map(|p| &p.value) {
            Some(PValue::Const(v)) => lane_of(v, i, self.def(sd, i)),
            _ => self.def(sd, i),
        }
    }
    pub fn dynamic(&self, sd: &str) -> Option<&'a FuncGraph> {
        match self.param(sd).map(|p| &p.value) {
            Some(PValue::Dynamic(f)) => Some(f),
            _ => None,
        }
    }

    fn mark(&mut self, sd: &str) {
        if !self.used.iter().any(|u| u.eq_ignore_ascii_case(sd)) {
            self.used.push(sd.to_string());
        }
    }

    /// Set one of our parameters to a fixed value.
    pub fn set(&mut self, mc: &str, v: f32) {
        self.c.num(mc, v);
    }

    /// `mc = sd[lane] * scale + offset`; a function-driven value is carried as a function.
    pub fn lin(&mut self, sd: &str, lane: usize, mc: &str, scale: f32, offset: f32) {
        self.mark(sd);
        let x = self.v(sd, lane);
        self.c.num(mc, x * scale + offset);
        if let Some(f) = self.dynamic(sd) {
            let lane = if self.lanes_of(sd) > 1 { Some(lane) } else { None };
            self.c.binds.push(Bind { param: mc.to_string(), func: f.clone(), lane, scale, offset });
        }
    }
    /// Same value, same units.
    pub fn same(&mut self, sd: &str, mc: &str) {
        self.lin(sd, 0, mc, 1.0, 0.0);
    }
    /// The lanes of a vector into several of our parameters (empty names are skipped).
    pub fn vec(&mut self, sd: &str, mcs: &[&str]) {
        for (i, mc) in mcs.iter().enumerate() {
            if !mc.is_empty() {
                self.lin(sd, i, mc, 1.0, 0.0);
            }
        }
        self.mark(sd);
    }
    /// `mc = f(sd)`; a function-driven value keeps the result for its default and is reported.
    pub fn map(&mut self, sd: &str, mc: &str, f: impl Fn(f32) -> f32) {
        let x = self.take(sd);
        self.c.num(mc, f(x));
    }
    /// Read `sd` for a computation (marks it used; a function-driven value is reported and its default used).
    pub fn take(&mut self, sd: &str) -> f32 {
        self.take_lane(sd, 0)
    }
    pub fn take_lane(&mut self, sd: &str, lane: usize) -> f32 {
        self.mark(sd);
        if self.dynamic(sd).is_some() {
            let msg = format!("'{sd}' is driven by a function here; its default value is used");
            if !self.c.notes.iter().any(|(_, t)| *t == msg) {
                self.c.approx(msg);
            }
        }
        self.v(sd, lane)
    }
    pub fn take_i(&mut self, sd: &str) -> i32 {
        self.take(sd).round() as i32
    }
    pub fn take_b(&mut self, sd: &str) -> bool {
        self.take(sd) > 0.5
    }
    /// Parameters that need no counterpart (handled elsewhere, or with no visible effect here).
    pub fn ignore(&mut self, sds: &[&str]) {
        for s in sds {
            self.mark(s);
        }
    }
    /// The library graph's image input `sd` feeds our input `mc` (an empty `mc`: no counterpart, left unconnected).
    pub fn slot(&mut self, sd: &str, mc: &str) {
        self.imgs.push((sd.to_string(), mc.to_string()));
    }
    /// The library graph's output `sd` is our output `mc`.
    pub fn out(&mut self, sd: &str, mc: &str) {
        self.outs.push((sd.to_string(), mc.to_string()));
    }
    pub fn note(&mut self, text: impl Into<String>) {
        self.c.approx(text);
    }

    /// Wire inputs, report what was not carried over, and hand back the node with its output map.
    pub fn finish(mut self) -> (Conv, Vec<(String, String)>) {
        let kind = self.kind;
        let pnames: Vec<&'static str> = crate::params::pdefs(kind).iter().map(|d| d.name).collect();
        // seed
        if let Some(p) = self.param("randomseed") {
            if pnames.contains(&"seed") && !self.c.node.nums.contains_key("seed") {
                if let PValue::Const(v) = &p.value {
                    self.c.num("seed", v.num().max(0.0));
                }
            }
            self.mark("randomseed");
        }
        // parameters set in the file that were not used
        let mut skipped = Vec::new();
        for p in self.params {
            let n = p.name.as_str();
            if n.starts_with('$') || BASE_PARAMS.contains(&n) || self.used.iter().any(|u| u.eq_ignore_ascii_case(n)) {
                continue;
            }
            let at_default = match (&p.value, self.lib.and_then(|l| l.input(n))) {
                (PValue::Const(v), Some(li)) => (0..li.ty.lanes().max(1)).all(|i| (lane_of(v, i, li.lane(i)) - li.lane(i)).abs() < 1e-4),
                _ => false,
            };
            if !at_default {
                skipped.push(n.to_string());
            }
        }
        if !skipped.is_empty() {
            self.c.approx(format!("parameters not carried over: {}", skipped.join(", ")));
        }
        // image inputs
        let spec = crate::graph::spec(kind);
        let mut ours: Vec<String> = spec.map(|s| s.req.iter().chain(s.opt.iter()).map(|x| x.to_string()).collect()).unwrap_or_default();
        if ours.is_empty() && kind != "color" {
            ours.push("in".into());
        }
        let mut slots: Vec<(String, String)> = self.imgs.clone();
        let taken = |slots: &Vec<(String, String)>, mc: &str| slots.iter().any(|(_, m)| m == mc);
        let lib_imgs: Vec<&'static str> = self.lib.map(|l| l.images().map(|i| i.id).collect()).unwrap_or_default();
        let mapped = |slots: &Vec<(String, String)>, sd: &str| slots.iter().any(|(s, _)| s.eq_ignore_ascii_case(sd));
        // same name
        for sd in &lib_imgs {
            if !mapped(&slots, sd) {
                if let Some(mc) = ours.iter().find(|m| m.eq_ignore_ascii_case(sd)) {
                    if !taken(&slots, mc) {
                        slots.push((sd.to_string(), mc.clone()));
                    }
                }
            }
        }
        // inputs whose usual library names mean one of ours
        for (sd, mc) in SLOT_ALIASES {
            if ours.iter().any(|m| m == mc) && !taken(&slots, mc) {
                if let Some(li) = lib_imgs.iter().find(|x| x.eq_ignore_ascii_case(sd) && !mapped(&slots, x)) {
                    slots.push((li.to_string(), mc.to_string()));
                } else if self.lib.is_none() {
                    slots.push((sd.to_string(), mc.to_string()));
                }
            }
        }
        // the usual names of a main input go to our first input
        if let Some(first) = ours.first().cloned() {
            if !taken(&slots, &first) {
                if let Some(sd) = lib_imgs.iter().find(|sd| !mapped(&slots, sd) && MAIN_INPUTS.iter().any(|m| m.eq_ignore_ascii_case(sd))) {
                    slots.push((sd.to_string(), first.clone()));
                }
            }
        }
        // the rest by order
        for sd in &lib_imgs {
            if mapped(&slots, sd) {
                continue;
            }
            if let Some(mc) = ours.iter().find(|m| !taken(&slots, m)).cloned() {
                slots.push((sd.to_string(), mc));
            }
        }
        if self.lib.is_none() {
            // no interface facts: same names and the usual main-input names
            for s in &ours {
                slots.push((s.clone(), s.clone()));
            }
            if let Some(first) = ours.first() {
                for m in MAIN_INPUTS {
                    slots.push((m.to_string(), first.clone()));
                }
            }
        }
        self.c.order = ours.iter().filter(|m| !taken(&slots, m)).cloned().collect();
        // inputs declared as having no counterpart stay unconnected
        self.c.slots.extend(slots.into_iter().filter(|(_, m)| !m.is_empty()));
        // outputs: declared, then same name
        let mut outs = self.outs.clone();
        let our_outs = crate::graph::node_outputs(kind);
        if let Some(l) = self.lib {
            for o in &l.outputs {
                if !outs.iter().any(|(s, _)| s.eq_ignore_ascii_case(o)) {
                    if let Some(m) = our_outs.iter().find(|m| m.eq_ignore_ascii_case(o)) {
                        outs.push((o.to_string(), m.to_string()));
                    }
                }
            }
        }
        (self.c, outs)
    }
}

fn lane_of(v: &Val, i: usize, def: f32) -> f32 {
    if i < v.len() {
        v.at(i)
    } else if v.len() == 1 && i < 3 {
        // a grayscale value given for a colour fills the colour lanes
        v.num()
    } else {
        def
    }
}

/// Library input names that mean one of our named inputs.
const SLOT_ALIASES: &[(&str, &str)] = &[
    ("mask", "mask"),
    ("mask_input", "mask"),
    ("background", "background"),
    ("background_input", "bg"),
    ("pattern_input", "pattern"),
    ("pattern_input_1", "pattern"),
    ("pattern_input_2", "pattern2"),
    ("pattern_input_3", "pattern3"),
    ("pattern_input_4", "pattern4"),
    ("pattern_input_5", "pattern5"),
    ("pattern_input_6", "pattern6"),
    ("intensity_input", "intensity"),
    ("vector_map", "vector"),
    ("slope", "slope"),
    ("slope_input", "slope"),
    ("warp", "warp"),
];

/// Library input names that mean "the main input".
pub(crate) const MAIN_INPUTS: &[&str] = &["input", "input1", "input_1", "source", "img_input", "input_grayscale", "grayscale_input", "input_color", "image", "image_input", "height", "height_input", "in", "Input", "Source"];

/// An adapter: library graph identifiers (optionally `package/graph`) -> our node kind and the translation.
pub(crate) struct Adapter {
    pub ids: &'static [&'static str],
    pub kind: &'static str,
    pub f: fn(&mut Ad),
}

/// Parameters with the same name and meaning carry over unchanged.
pub(crate) fn generic(ad: &mut Ad) {
    let pnames: Vec<&'static str> = crate::params::pdefs(ad.kind).iter().map(|d| d.name).collect();
    let values: Vec<&'static str> = ad.lib.map(|l| l.values().map(|i| i.id).collect()).unwrap_or_default();
    if ad.lib.is_some() {
        for sd in values {
            if let Some(mc) = pnames.iter().find(|p| p.eq_ignore_ascii_case(sd)) {
                ad.same(sd, mc);
            }
        }
    } else {
        let names: Vec<String> = ad.params.iter().map(|p| p.name.clone()).collect();
        for sd in names {
            if let Some(mc) = pnames.iter().find(|p| p.eq_ignore_ascii_case(&sd)) {
                ad.same(&sd, mc);
            }
        }
    }
}

/// The adapter for library graph `id` of `package`, if one is written.
pub(crate) fn adapter(package: &str, id: &str) -> Option<&'static Adapter> {
    let full = format!("{package}/{id}");
    ADAPTERS
        .iter()
        .find(|a| a.ids.iter().any(|x| x.eq_ignore_ascii_case(&full)))
        .or_else(|| ADAPTERS.iter().find(|a| a.ids.iter().any(|x| x.eq_ignore_ascii_case(id))))
}

// ------------------------------------------------------------------------------------------------ noises

/// Shared part of the Preset Noise adapters: scale, disorder, roughness and the extras of some presets.
fn preset(ad: &mut Ad, i: usize) {
    ad.set("preset", i as f32);
    // the looks are fitted at the graph's default scale: ours counts multiples of it
    if ad.lib.and_then(|l| l.input("scale")).is_some() {
        let d = ad.def("scale", 0).max(1e-3);
        ad.lin("scale", 0, "scale", 1.0 / d, 0.0);
    }
    // the older graphs take their disorder on a 0..100 scale
    if ad.lib.and_then(|l| l.input("Disorder")).is_some() {
        ad.lin("Disorder", 0, "disorder", 0.01, 0.0);
    } else if ad.lib.and_then(|l| l.input("disorder")).is_some() {
        ad.same("disorder", "disorder");
    }
    for r in ["roughness", "Roughness"] {
        if ad.lib.and_then(|l| l.input(r)).is_some() {
            ad.lin(r, 0, "roughness", 1.0, 0.0);
        }
    }
    // overall intensity of the summed octaves: our closest control is the contrast around mid grey
    for g in ["GlobalOpacity", "global_opacity"] {
        if let Some(li) = ad.lib.and_then(|l| l.input(g)) {
            let d = li.lane(0).max(1e-3);
            let x = ad.take(g);
            if (x - d).abs() > 1e-4 {
                ad.set("contrast", (x / d).clamp(0.0, 4.0));
            }
        }
    }
    ad.ignore(&["non_square_expansion"]);
    extras(ad);
}

/// Library parameters of the noise and grunge graphs that move one of the extra layers of Preset Noise. Each entry:
/// (library names, our parameter, full-scale change when the library slider goes over its whole range). Values are
/// taken relative to the library default, because the looks were fitted at the defaults.
const EXTRAS: &[(&[&str], &str, f32)] = &[
    (&["warp_intensity", "base_warp_intensity", "distortion", "micro_distortion", "splines_distortion", "splashes_distortion"], "warp", 2.0),
    (&["angle_random", "leak_angle_random", "rotation_random", "rotation_var"], "angle_random", 1.0),
    (&["sharpen", "sharpen_intensity"], "sharpen", 2.0),
    (&["scratches_opacity", "scratches_intensity", "scratches_amount", "scratch_quantity", "double_scratch", "scratch_dirtiness"], "scratches", 1.0),
    (&["dirt_specks_opacity", "dust_intensity", "dust", "spots_opacity", "scratch_spots_amount", "dirt", "details", "density", "density_contrast", "dust_spread", "splash_dirt_ratio"], "specks", 1.0),
    (&["streaks_intensity", "drip_length", "drip_crispness", "leak_intensity", "leak_scale", "leak_crispness", "shape_contrast", "scratch_length", "pattern_lenght", "flow"], "streaks", 1.0),
    (&["grain_intensity", "noise_intensity", "base_noisiness", "crispness"], "grain", 1.0),
    (&["cracks"], "cracks", 1.0),
    (&["brush_pattern"], "brush", 1.0),
];

/// Names that only turn something on or off, or that tune a layer we do not model separately; they are read so the
/// import does not report them as lost values, and the note says they act through the extra layers.
const EXTRAS_QUIET: &[&str] = &[
    "invert_dirt", "invert_scratches", "main_grunge_intensity", "base_grunge_intensity", "large_variation_intensity", "ridge_detail_opacity",
    "base_grunge_contrast", "scratch_width", "scratch_blur", "scratch_masking", "scratch_spots_tiling", "tile_scratch", "tile_spots", "tile_grunge",
    "spots_tiling", "divisions", "waves", "emboss_intensity", "emboss_angle", "position",
];

fn extras(ad: &mut Ad) {
    let Some(lib) = ad.lib else { return };
    let mut touched = false;
    for (names, mc, full) in EXTRAS {
        let mut delta = 0.0f32;
        let mut any = false;
        for li in lib.values() {
            if names.iter().any(|n| n.eq_ignore_ascii_case(li.id)) {
                let span = li.range.map(|(a, b)| (b - a).abs()).filter(|s| *s > 1e-6).unwrap_or(1.0);
                let x = ad.take(li.id);
                delta += (x - li.lane(0)) * full / span;
                any = true;
            }
        }
        if any && delta.abs() > 1e-6 {
            let cur = ad.c.node.nums.get(*mc).copied().unwrap_or(0.0);
            ad.set(mc, cur + delta);
            touched = true;
        }
    }
    for li in lib.values() {
        if EXTRAS_QUIET.iter().any(|n| n.eq_ignore_ascii_case(li.id)) {
            ad.ignore(&[li.id]);
        }
    }
    if touched {
        ad.note("the extra grunge controls change layers of our own (warp, scratches, specks, streaks, grain, sharpen), not the original's exact recipe");
    }
}

/// Grunge presets: balance shifts the values, contrast is relative to the graph's default.
fn grunge(ad: &mut Ad, i: usize) {
    preset(ad, i);
    if ad.lib.and_then(|l| l.input("balance")).is_some() {
        ad.same("balance", "balance");
    }
    for c in ["contrast", "Contrast"] {
        if let Some(li) = ad.lib.and_then(|l| l.input(c)) {
            let d = li.lane(0);
            let x = ad.take(c);
            if (x - d).abs() > 1e-4 {
                ad.set("contrast", (1.0 + (x - d) * 2.0).clamp(0.0, 4.0));
            }
        }
    }
    if ad.lib.and_then(|l| l.input("invert")).is_some() {
        ad.same("invert", "invert");
    }
}

/// Directional presets: the angle turns the streaks (in quarter turns; finer angles are reported).
fn directional(ad: &mut Ad, i: usize) {
    preset(ad, i);
    if ad.lib.and_then(|l| l.input("angle")).is_some() {
        let a = ad.take("angle");
        let q = (a * 4.0).round() / 4.0;
        if (a - q).abs() > 0.01 {
            ad.note(format!("the streaks are turned by {:.0} degrees here instead of {:.0}", q * 360.0, a * 360.0));
        }
        ad.set("angle", q);
    }
}

macro_rules! grunges {
    ($($f:ident = $i:expr;)*) => {
        $(fn $f(ad: &mut Ad) { grunge(ad, $i) })*
    };
}

macro_rules! directionals {
    ($($f:ident = $i:expr;)*) => {
        $(fn $f(ad: &mut Ad) { directional(ad, $i) })*
    };
}

grunges! {
    g_map_001 = 30; g_map_002 = 31; g_map_003 = 32; g_map_004 = 33; g_map_005 = 34; g_map_006 = 35; g_map_007 = 36; g_map_008 = 37;
    g_map_009 = 38; g_map_010 = 39; g_map_011 = 40; g_map_012 = 41; g_map_013 = 42; g_map_014 = 43; g_map_015 = 44;
    g_concrete = 45; g_damas = 46; g_galvanic_large = 47; g_galvanic_small = 48; g_leaks = 49; g_leaky_paint = 50; g_rough_dirty = 51;
    g_rust_fine = 52; g_scratches_dirty = 53; g_scratches_fine = 54; g_scratches_rough = 55; g_shavings = 56; g_splashes_dusty = 57;
    g_spots = 58; g_spots_dirty = 59;
}

directionals! {
    d_noise_1 = 60; d_noise_2 = 61; d_noise_3 = 62; d_noise_4 = 63; d_old_noise = 64; d_vertical = 65; d_scratches = 66;
    d_wood_1 = 67; d_wood_2 = 68; d_crystal_2 = 69; d_old_crystal_2 = 70; d_old_horizontal_scratch = 77;
}

macro_rules! presets {
    ($($f:ident = $i:expr;)*) => {
        $(fn $f(ad: &mut Ad) { preset(ad, $i) })*
    };
}

presets! {
    p_clouds_1 = 0; p_clouds_2 = 1; p_clouds_3 = 2; p_bnw_1 = 3; p_bnw_2 = 4; p_bnw_3 = 5;
    p_fsum_base = 6; p_fsum_1 = 7; p_fsum_2 = 8; p_fsum_3 = 9; p_fsum_4 = 10; p_moisture = 11;
    p_dirt_1 = 12; p_dirt_2 = 13; p_dirt_3 = 14; p_dirt_4 = 15; p_dirt_5 = 16;
    p_old_clouds_1 = 17; p_old_clouds_2 = 18; p_old_clouds_3 = 19; p_old_bnw_1 = 20; p_old_bnw_2 = 21; p_old_bnw_3 = 22;
    p_old_fsum_base = 23; p_old_fsum_1 = 24; p_old_fsum_2 = 25; p_old_fsum_3 = 26; p_old_fsum_4 = 27;
    p_old_noise_1 = 28; p_old_noise_2 = 29;
    p_gauss_spots_1 = 71; p_gauss_spots_2 = 72; p_old_few_spots = 73; p_old_spots = 74; p_dirt_gradient = 75; p_old_dirt_gradient = 76;
    p_white_stains = 79; p_old_creased = 80; p_creased = 81; p_old_fluid = 82; p_fluid = 83; p_herb = 84; p_horizontal_lines = 85;
    p_old_liquid = 86; p_liquid = 87; p_old_messy_1 = 88; p_messy_1 = 89; p_sponge_stains = 90; p_paper_path = 91;
}

/// Noise Zoom: the distance between its features is only matched at its default.
fn p_noise_zoom(ad: &mut Ad) {
    preset(ad, 78);
    if (ad.take("Distance") - 14.0).abs() > 0.5 {
        ad.note("the feature distance is only matched at its default (14)");
    }
}

/// Shared part of the Noise Patterns adapters.
fn pattern(ad: &mut Ad, i: usize) {
    ad.set("preset", i as f32);
    if ad.lib.and_then(|l| l.input("scale")).is_some() {
        ad.same("scale", "scale");
    }
    // the older cell graphs give the cell count as half the lattice and their randomness on 0..100
    if ad.lib.and_then(|l| l.input("Distance")).is_some() {
        ad.lin("Distance", 0, "scale", 2.0, 0.0);
    }
    if ad.lib.and_then(|l| l.input("Disorder")).is_some() && i >= 8 && i <= 11 {
        ad.lin("Disorder", 0, "jitter", 0.02, 0.0);
    } else if ad.lib.and_then(|l| l.input("disorder")).is_some() {
        ad.same("disorder", "disorder");
    }
    for (sd, mc) in [("edge_width", "edge_width"), ("hardness", "hardness"), ("invert", "invert"), ("color_source", "color_source"), ("pseudo_random_seed", "pseudo_random_seed")] {
        if ad.lib.and_then(|l| l.input(sd)).is_some() {
            ad.same(sd, mc);
        }
    }
    ad.slot("input", "input");
    ad.ignore(&["non_square_expansion"]);
    // the older crystal graph has no scale: about 14 cells across
    if i == 12 {
        ad.set("scale", 14.0);
    }
}

macro_rules! patterns {
    ($($f:ident = $i:expr;)*) => {
        $(fn $f(ad: &mut Ad) { pattern(ad, $i) })*
    };
}

patterns! {
    n_cells_1 = 0; n_cells_2 = 1; n_cells_3 = 2; n_cells_4 = 3; n_crystal_1 = 4; n_gaussian = 5; n_perlin = 6; n_plasma = 7;
    n_old_cells_1 = 8; n_old_cells_2 = 9; n_old_cells_3 = 10; n_old_cells_4 = 11; n_old_crystal_1 = 12; n_old_plasma = 13;
}

// ------------------------------------------------------------------------------------------------ patterns

/// Checker 1: "Tiling" counts pairs of squares; the top left square is white.
fn checker_1(ad: &mut Ad) {
    ad.map("Tiling", "tiles", |t| t.round().max(1.0) * 2.0);
    ad.set("white_first", 1.0);
    ad.same("Rotate_45_Degrees", "rotate45");
    ad.ignore(&["non_square_expansion"]);
}

/// Checker 2: 32 squares per tile, the white ones with a random luminance.
fn checker_2(ad: &mut Ad) {
    ad.map("Tiling", "tiles", |t| t.round().max(1.0) * 32.0);
    ad.set("white_first", 1.0);
    ad.same("Luminance_Variation", "lum_var");
    if ad.take("Disorder") > 0.0 {
        ad.note("the squares' disorder is not reproduced");
    }
}

/// Stripes: "Shift" slants them (whole stripes over the height); a stripe is centred in its period.
fn stripes(ad: &mut Ad) {
    ad.same("Stripes", "count");
    ad.set("angle", 0.0);
    ad.set("tile", 1.0);
    let w = ad.take("Width").clamp(0.0, 1.0);
    // filtering softens the edges over about a twelfth of a stripe period
    let filtered = ad.take_b("Filtering");
    let soft = (ad.take("Softness").clamp(0.0, 1.0) * 0.5).max(if filtered { 0.08 } else { 0.001 });
    ad.set("width", w);
    ad.set("softness", soft);
    ad.set("phase", 0.5 - (w + soft) * 0.5);
    ad.same("Shift", "shift");
    if ad.take_i("Align") != 0 {
        ad.note("the stripes' alignment option is not reproduced");
    }
    ad.ignore(&["non_square_expansion"]);
}

/// Turn a point around the centre by quarter turns (clockwise on screen).
fn quarter(p: [f32; 2], q: i32) -> [f32; 2] {
    let (mut x, mut y) = (p[0] - 0.5, p[1] - 0.5);
    for _ in 0..q.rem_euclid(4) {
        (x, y) = (-y, x);
    }
    [x + 0.5, y + 0.5]
}

fn points(ad: &mut Ad, p1: [f32; 2], p2: [f32; 2]) {
    ad.set("p1_x", p1[0]);
    ad.set("p1_y", p1[1]);
    ad.set("p2_x", p2[0]);
    ad.set("p2_y", p2[1]);
}

/// Linear gradients 1 to 3: vertical, turned by quarter turns.
fn linear_gradient(ad: &mut Ad, which: u8) {
    let q = ad.take_i("rotation");
    ad.same("Tiling", "tiling");
    let (p1, p2) = if which == 1 { ([0.5, 1.0], [0.5, 0.0]) } else { ([0.5, 0.0], [0.5, 1.0]) };
    points(ad, quarter(p1, q), quarter(p2, q));
    match which {
        1 => ad.set("mode", 0.0),
        2 => {
            ad.set("mode", 1.0);
            ad.set("profile", 1.0);
        }
        _ => {
            ad.set("mode", 1.0);
            ad.same("position", "peak");
        }
    }
}
fn linear_1(ad: &mut Ad) {
    linear_gradient(ad, 1)
}
fn linear_2(ad: &mut Ad) {
    linear_gradient(ad, 2)
}
fn linear_3(ad: &mut Ad) {
    linear_gradient(ad, 3)
}

/// Axial gradient: 0 at both points, 1 half way.
fn axial(ad: &mut Ad) {
    ad.set("mode", 1.0);
    ad.vec("point_1", &["p1_x", "p1_y"]);
    ad.vec("point_2", &["p2_x", "p2_y"]);
    ad.ignore(&["non_square_expansion"]);
}

/// Reflected axial gradient: 1 at point 1, 0 at point 2 and at its mirror image.
fn axial_reflected(ad: &mut Ad) {
    let p1 = [ad.take_lane("point_1", 0), ad.take_lane("point_1", 1)];
    let p2 = [ad.take_lane("point_2", 0), ad.take_lane("point_2", 1)];
    ad.set("mode", 1.0);
    points(ad, [2.0 * p1[0] - p2[0], 2.0 * p1[1] - p2[1]], p2);
    ad.note("the reflected axial gradient is not yet checked against a reference render");
    ad.ignore(&["non_square_expansion"]);
}

fn circular(ad: &mut Ad) {
    ad.set("mode", 2.0);
    ad.vec("point_1", &["p1_x", "p1_y"]);
    ad.vec("point_2", &["p2_x", "p2_y"]);
    ad.same("angle", "angle");
    ad.ignore(&["non_square_expansion"]);
}

fn radial(ad: &mut Ad) {
    ad.set("mode", 3.0);
    ad.vec("point_1", &["p1_x", "p1_y"]);
    ad.vec("point_2", &["p2_x", "p2_y"]);
    let k = ad.take_i("shape");
    if k > 1 {
        ad.note(format!("radial gradient shape {k} is shown round"));
    }
    ad.set("radial_shape", if k == 1 { 1.0 } else { 0.0 });
    ad.ignore(&["non_square_expansion"]);
}

/// Polygon 1: flat white, a corner pointing up, as large as the tile.
fn polygon_1(ad: &mut Ad) {
    ad.same("Sides", "sides");
    ad.lin("Scale", 0, "radius", 0.5, 0.0);
    ad.lin("Rotation", 0, "rotation", 1.0, 0.75);
    ad.set("softness", 0.0);
    if ad.take_b("Gradient") {
        ad.set("gradient", 1.0);
    }
    ad.same("GradientInvert", "gradient_invert");
    ad.same("Tiling", "tiling");
    if ad.take("Explode") > 0.0 {
        ad.note("the exploded triangles are not reproduced");
    }
    if (ad.take_lane("TriangleSize", 0) - 1.0).abs() > 1e-4 || (ad.take_lane("TriangleSize", 1) - 1.0).abs() > 1e-4 {
        ad.note("the triangle size is not reproduced");
    }
    if ad.decl("AutoScale") && !ad.take_b("AutoScale") {
        ad.note("automatic scaling is always on here");
    }
    ad.ignore(&["non_square_expansion", "non_square_tiling"]);
}

/// Polygon 2: a pyramid gradient from the centre to the edges, a corner pointing left.
fn polygon_2(ad: &mut Ad) {
    ad.same("Sides", "sides");
    ad.lin("Scale", 0, "radius", 0.5, 0.0);
    ad.lin("Rotation", 0, "rotation", 1.0, 0.5);
    ad.set("softness", 0.0);
    ad.same("Curve", "curve");
    ad.same("Gradient", "gradient");
    ad.same("InvertGradient", "gradient_invert");
    if ad.decl("Tiling") {
        ad.same("Tiling", "tiling");
    }
    ad.ignore(&["autoscale", "non_square_expansion"]);
}

/// Shape: pattern number -> our shape; size from the overall size and the X / Y sizes.
fn shape_common(ad: &mut Ad, newer: bool) {
    let pk = if ad.decl("Pattern") { "Pattern" } else { "pattern" };
    let p = ad.take_i(pk);
    // 1 is each graph's default (checked against renders: a square for the older graph, a dome for the newer ones);
    // 2..15 draw the pattern list the scatter nodes share (our shapes from 100 on), 0 draws nothing
    let k: u32 = match (newer, p) {
        (false, 1) => 0,
        (true, 1) => 6,
        (_, 2..=15) => 100 + p as u32,
        _ => {
            ad.set("size_x", 0.0);
            ad.set("size_y", 0.0);
            ad.set("shape", 0.0);
            ad.note(format!("shape pattern {p} draws nothing"));
            ad.ignore(&["Size", "scale", "Size_xy", "size_xy", "Angle", "angle", "Tiling", "Pattern_Specific", "pattern_specific", "Rotation", "non_square_expansion", "non_square_tiling", "non_square"]);
            return;
        }
    };
    if k >= 100 {
        ad.note(format!("shape pattern {p} is drawn with the scatter nodes' '{}' pattern (not yet checked against a reference render)", crate::scatter::PATTERN_NAMES[p as usize]));
    }
    ad.set("shape", k as f32);
    let sk = if ad.decl("Size") { "Size" } else { "scale" };
    let xy = if ad.decl("Size_xy") { "Size_xy" } else { "size_xy" };
    let sz = ad.take(sk);
    let (sx, sy) = (ad.take_lane(xy, 0), ad.take_lane(xy, 1));
    ad.set("size_x", sz * sx);
    ad.set("size_y", sz * sy);
    let ak = if ad.decl("Angle") { "Angle" } else { "angle" };
    ad.same(ak, "rotation");
    ad.set("softness", 0.0);
    if ad.decl("Tiling") {
        ad.same("Tiling", "tiling");
    }
    let psk = if ad.decl("Pattern_Specific") { "Pattern_Specific" } else { "pattern_specific" };
    let spec = ad.take(psk);
    if k >= 100 {
        ad.set("pattern_specific", spec);
    } else if spec > 0.0 {
        ad.note("the pattern specific setting is not reproduced");
    }
    if ad.decl("Rotation") && ad.take_b("Rotation") {
        ad.note("the shape's rotation option is not reproduced");
    }
    ad.ignore(&["non_square_expansion", "non_square_tiling", "non_square"]);
}
fn shape_old(ad: &mut Ad) {
    shape_common(ad, false)
}
fn shape_new(ad: &mut Ad) {
    shape_common(ad, true)
}

/// Starburst: blurred rays fading along their length.
fn starburst(ad: &mut Ad) {
    ad.set("style", 1.0);
    ad.same("Branches", "rays");
    ad.same("BlurAmount", "blur");
    ad.same("Randomness", "randomness");
    ad.same("SizeMin", "size_min");
    ad.same("SizeMax", "size");
    ad.same("SizeVariation", "size_variation");
    ad.same("Tiling", "count");
    ad.set("rotation", 0.0);
}

/// The pattern lists of the tile and splatter graphs: 0 none, 1 the image inputs, then the built-in shapes in our
/// order. Returns the graph's pattern number.
fn scatter_pattern(ad: &mut Ad, sd: &str) -> i32 {
    let p = ad.take_i(sd);
    match p {
        0 => {
            ad.set("pattern_type", 0.0);
            ad.set("opacity", 0.0);
        }
        1 => ad.set("pattern_type", 0.0),
        2..=16 => ad.set("pattern_type", (p - 1) as f32),
        _ => {
            ad.note(format!("pattern {p} has no equivalent here; a square is drawn"));
            ad.set("pattern_type", 1.0);
        }
    }
    p
}

/// Pattern specific: the bevel of a brick is measured differently here.
fn scatter_specific(ad: &mut Ad, sd: &str, pattern: i32) {
    let x = ad.take_lane(sd, 0);
    ad.set("pattern_specific", if pattern == 9 { ((x - 0.04) / 0.5).clamp(0.0, 1.0) } else { x });
}

/// Blending list of the tile and splatter graphs (not yet checked against renders with overlapping patterns).
fn scatter_blend(ad: &mut Ad, sd: &str, default: i32) {
    let b = ad.take_i(sd);
    let ours = match b {
        0 => 3.0,
        1 => 0.0,
        2 => 1.0,
        _ => 2.0,
    };
    ad.set("blending", ours);
    if b != default {
        ad.note(format!("blending mode {b} is assumed to be our '{}' mode", crate::params::TILE_BLEND[ours as usize]));
    }
}

/// A random position (per axis, in cells) as ours: overall amount 1, per axis amounts.
fn scatter_position_random(ad: &mut Ad, sd: &str) {
    let lanes = ad.lib.and_then(|l| l.input(sd)).map(|i| i.ty.lanes()).unwrap_or(1);
    let x = ad.take_lane(sd, 0);
    let y = if lanes > 1 { ad.take_lane(sd, 1) } else { x };
    ad.set("position_random", 1.0);
    ad.set("position_random_x", x);
    ad.set("position_random_y", y);
}

/// Tile Generator (current version).
fn tile_generator_new(ad: &mut Ad) {
    ad.same("x_amount", "x_amount");
    ad.same("y_amount", "y_amount");
    let p = scatter_pattern(ad, "pattern");
    scatter_specific(ad, "pattern_specific", p);
    ad.same("pattern_input_distribution", "pattern_distribution");
    ad.same("pattern_rotation", "pattern_rotation");
    ad.same("pattern_rotation_random", "pattern_rotation_random");
    if ad.take("pattern_symmetry_random") > 0.0 {
        let m = ad.take_i("pattern_symmetry_random_mode");
        ad.set("symmetry_random", (m.clamp(0, 2) + 1) as f32);
        ad.note("random mirroring is applied to every other pattern here");
    }
    ad.ignore(&["pattern_symmetry_random_mode", "pattern_quincunx_flip", "pattern_input_number", "image_input_filtering"]);
    let mode = ad.take_i("size_mode");
    let key = match mode {
        1 => "pattern_size_absolute",
        2 => "pattern_size_pixel",
        _ => "pattern_size",
    };
    if mode != 0 {
        ad.note(format!("size mode {mode} is approximated"));
    }
    ad.set("size_mode", if mode == 0 { 0.0 } else { 2.0 });
    let (sx, sy) = (ad.take_lane(key, 0), ad.take_lane(key, 1));
    ad.set("size", sx);
    ad.set("aspect", if sx > 1e-4 { sy / sx } else { 1.0 });
    ad.ignore(&["pattern_size", "pattern_size_absolute", "pattern_size_pixel"]);
    ad.lin("pattern_size_random", 0, "size_random", 1.0, 0.0);
    ad.same("scale", "scale");
    ad.same("scale_random", "scale_random");
    ad.same("scale_random_seed", "scale_seed");
    let off = ad.take("position_offset");
    let off_r = ad.take("position_offset_random");
    let vertical = ad.take_b("vertical_offset");
    ad.set("offset", off);
    ad.set("offset_random", off_r);
    ad.set("offset_mode", if off == 0.0 && off_r == 0.0 { 0.0 } else if vertical { 3.0 } else { 1.0 });
    ad.same("offset_random_seed", "offset_seed");
    scatter_position_random(ad, "position_random");
    ad.vec("global_offset", &["global_offset_x", "global_offset_y"]);
    ad.same("rotation", "rotation");
    ad.same("rotation_random", "rotation_random");
    ad.same("pattern_color", "color");
    ad.same("luminance_random", "color_random");
    ad.same("luminance_by_number", "lum_by_number");
    ad.same("luminance_by_scale", "lum_by_scale");
    ad.same("checker_mask", "mask_checker");
    ad.same("horizontal_mask", "mask_horizontal");
    ad.same("vertical_mask", "mask_vertical");
    ad.same("random_mask", "mask_random");
    ad.same("invert_mask", "invert_mask");
    scatter_blend(ad, "blending_mode", 0);
    ad.same("background_color", "background");
    ad.same("global_opacity", "opacity");
    ad.same("reverse_rendering_order", "reverse_order");
    let mid = (ad.take_lane("middle_size", 0), ad.take_lane("middle_size", 1));
    if (mid.0 - 0.5).abs() > 1e-4 || (mid.1 - 0.5).abs() > 1e-4 {
        ad.note("the middle size is not reproduced");
    }
    if (0..4).any(|i| ad.take_lane("interstice", i) > 0.0) {
        ad.note("the interstice is not reproduced");
    }
    for (i, mc) in ["pattern", "pattern2", "pattern3", "pattern4", "pattern5", "pattern6"].iter().enumerate() {
        ad.slot(&format!("pattern_input_{}", i + 1), mc);
    }
    ad.slot("background", "background");
    ad.ignore(&["non_square_expansion"]);
}

/// Tile Generator (older version).
fn tile_generator_old(ad: &mut Ad) {
    ad.same("Nb_X", "x_amount");
    ad.same("Nb_Y", "y_amount");
    let p = scatter_pattern(ad, "Pattern_Type");
    scatter_specific(ad, "Pattern_Specific", p);
    ad.set("size", 1.0);
    ad.same("Scale", "scale");
    ad.same("ScaleVariation", "scale_random");
    ad.same("ScaleVariation_Seed", "scale_seed");
    ad.same("LuminanceByScaleVariation", "lum_by_scale");
    let off = ad.take("Offset");
    let off_r = ad.take("Offset_Random");
    let vertical = ad.take_b("Vertical_Offset");
    if off > 1.0 {
        ad.note("row offsets beyond one tile are wrapped");
    }
    ad.set("offset", off.rem_euclid(1.0));
    ad.set("offset_random", off_r);
    ad.set("offset_mode", if off == 0.0 && off_r == 0.0 { 0.0 } else if vertical { 3.0 } else { 1.0 });
    ad.same("Offset_Random_Seed", "offset_seed");
    ad.vec("Global_Offset", &["global_offset_x", "global_offset_y"]);
    ad.same("LuminanceByNumber", "lum_by_number");
    ad.lin("Luminance_Variation-Contrast", 0, "color_random", 1.0, 0.0);
    ad.lin("Free_Rotation", 0, "rotation", 1.0 / 360.0, 0.0);
    ad.lin("Free_Rotation_Random", 0, "rotation_random", 1.0 / 360.0, 0.0);
    ad.same("Fixed_Rotation", "pattern_rotation");
    if ad.take("Fixed_Rotation_Random") > 0.0 {
        ad.note("random quarter turns of the pattern are not reproduced");
    }
    if ad.take_b("QuincunxFlip") {
        ad.note("the quincunx flip is not reproduced");
    }
    scatter_position_random(ad, "Position_Random");
    ad.same("CheckerMask", "mask_checker");
    ad.same("HorizontalMask", "mask_horizontal");
    ad.same("VerticalMask", "mask_vertical");
    ad.same("RandomMask", "mask_random");
    ad.same("InvertMask", "invert_mask");
    scatter_blend(ad, "BlendingMode", 0);
    ad.same("BackgroundColor", "background");
    ad.same("GlobalOpacity", "opacity");
    ad.same("reverse_rendering_order", "reverse_order");
    let mid = (ad.take_lane("Middle_Size", 0), ad.take_lane("Middle_Size", 1));
    if (mid.0 - 0.5).abs() > 1e-4 || (mid.1 - 0.5).abs() > 1e-4 {
        ad.note("the middle size is not reproduced");
    }
    if (0..4).any(|i| ad.take_lane("Interstice", i) > 0.0) {
        ad.note("the interstice is not reproduced");
    }
    ad.slot("Source", "pattern");
    ad.slot("background", "background");
    ad.ignore(&["image_input_filtering"]);
}

/// Tile Sampler (both versions; the older one has fewer settings).
fn tile_sampler(ad: &mut Ad) {
    ad.same("x_amount", "x_amount");
    ad.same("y_amount", "y_amount");
    let p = scatter_pattern(ad, "pattern");
    scatter_specific(ad, "pattern_specific", p);
    ad.same("pattern_specific_random", "pattern_specific_random");
    if ad.decl("pattern_input_distribution") {
        ad.same("pattern_input_distribution", "pattern_distribution");
    }
    ad.same("pattern_rotation", "pattern_rotation");
    ad.same("pattern_rotation_random", "pattern_rotation_random");
    if ad.take("pattern_symmetry_random") > 0.0 {
        let m = if ad.decl("pattern_symmetry_random_mode") { ad.take_i("pattern_symmetry_random_mode") } else { 0 };
        ad.set("symmetry_random", (m.clamp(0, 2) + 1) as f32);
        ad.note("random mirroring is applied to every other pattern here");
    }
    ad.ignore(&["pattern_symmetry_random_mode", "pattern_input_number", "pattern_input_filtering", "image_input_filtering"]);
    // size: the current version's default mode (1) sizes patterns by their cell, like the older version
    let mode = if ad.decl("size_mode") { ad.take_i("size_mode") } else { 1 };
    let key = match mode {
        0 => "size_absolute",
        2 => "size_pixel",
        _ => "size",
    };
    if mode != 1 {
        ad.note(format!("size mode {mode} is approximated"));
    }
    ad.set("size_mode", if mode == 1 { 0.0 } else { 2.0 });
    let (sx, sy) = (ad.take_lane(key, 0), ad.take_lane(key, 1));
    ad.set("size", sx);
    ad.set("aspect", if sx > 1e-4 { sy / sx } else { 1.0 });
    ad.ignore(&["size", "size_absolute", "size_pixel"]);
    ad.lin("size_random", 0, "size_random", 1.0, 0.0);
    ad.same("scale", "scale");
    ad.same("scale_random", "scale_random");
    ad.same("scale_map_multiplier", "scale_map_multiplier");
    if ad.take("scale_vector_map_multiplier") > 0.0 || ad.take("vector_map_displacement") > 0.0 || ad.take("rotation_vector_map_multiplier") > 0.0 {
        ad.note("the vector map settings are not reproduced");
    }
    ad.ignore(&["scale_parametrization_affect", "mask_map_sampling_technique", "color_parametrization_mode"]);
    let pr = ad.take("position_random");
    ad.set("position_random", pr);
    ad.set("position_random_x", 1.0);
    ad.set("position_random_y", 1.0);
    let off = ad.take("offset");
    let ot = ad.take_i("offset_type");
    ad.set("offset", off);
    ad.set("offset_mode", if off == 0.0 { 0.0 } else if ot == 1 { 3.0 } else { 1.0 });
    if ot > 1 {
        ad.note(format!("offset type {ot} is approximated"));
    }
    ad.vec("global_offset", &["global_offset_x", "global_offset_y"]);
    ad.same("displacement_map_intensity", "displacement_intensity");
    ad.same("displacement_angle", "displacement_angle");
    ad.same("rotation", "rotation");
    ad.same("rotation_random", "rotation_random");
    ad.same("rotation_map_multiplier", "rotation_map_multiplier");
    ad.same("mask_map_threshold", "mask_threshold");
    ad.same("mask_map_invert", "mask_invert");
    ad.same("mask_random", "mask_random");
    ad.same("mask_invert", "invert_mask");
    scatter_blend(ad, "blending_mode", 1);
    ad.same("color", "color");
    ad.same("color_random", "color_random");
    ad.same("color_parametrization_multiplier", "color_map_multiplier");
    ad.same("global_opacity", "opacity");
    ad.same("background_color", "background");
    ad.same("reverse_rendering_order", "reverse_order");
    ad.slot("pattern_input", "pattern");
    for i in 2..=6 {
        ad.slot(&format!("pattern_input_{i}"), &format!("pattern{i}"));
    }
    for (sd, mc) in [
        ("scale_map_input", "scale_map"),
        ("displacement_map_input", "displacement_map"),
        ("rotation_map_input", "rotation_map"),
        ("vector_map_input", ""),
        ("color_map_input", "color_map"),
        ("mask_map_input", "mask"),
        ("pattern_distribution_map_input", "distribution_map"),
        ("background_input", "background"),
    ] {
        ad.slot(sd, mc);
    }
    ad.ignore(&["non_square_expansion"]);
}

/// Splatter Circular (current version).
fn splatter_circular_new(ad: &mut Ad) {
    ad.same("pattern_amount", "number");
    ad.same("pattern_amount_random", "amount_random");
    ad.same("pattern_amount_random_min", "amount_random_min");
    ad.same("ring_amount", "rings");
    let p = scatter_pattern(ad, "pattern");
    scatter_specific(ad, "Pattern_Specific", p);
    ad.same("pattern_input_distribution", "pattern_distribution");
    if ad.take("pattern_symmetry_random") > 0.0 {
        let m = ad.take_i("pattern_symmetry_random_mode");
        ad.set("symmetry_random", (m.clamp(0, 2) + 1) as f32);
        ad.note("random mirroring is applied to every other pattern here");
    }
    ad.ignore(&["pattern_symmetry_random_mode", "pattern_input_number", "image_input_filtering", "pattern_rotation_pivot"]);
    ad.same("radius", "radius");
    ad.same("radius_random", "radius_random");
    ad.same("ring_radius_mutliplier", "ring_radius_multiplier");
    ad.same("angle_random", "angle_random");
    ad.same("spiral_factor", "spiral_factor");
    ad.same("spread", "spread");
    if ad.take("position_offset_direction") > 0.0 || ad.take_lane("global_offset", 0) != 0.0 || ad.take_lane("global_offset", 1) != 0.0 {
        ad.note("the position offsets are not reproduced");
    }
    if ad.take_b("connect_patterns") {
        ad.note("connected patterns are not reproduced");
    }
    ad.ignore(&["pattern_size_connected"]);
    let sc = ad.take("pattern_scale");
    let (sx, sy) = (ad.take_lane("pattern_size", 0), ad.take_lane("pattern_size", 1));
    ad.set("size", sx * sc);
    if (sx - sy).abs() > 1e-4 {
        ad.note("non-square pattern sizes are drawn square");
    }
    let r = ad.take_lane("pattern_size_random", 0).max(ad.take("pattern_scale_random"));
    ad.set("size_random", r);
    ad.same("pattern_scale_by_pattern_number", "scale_by_number");
    ad.same("pattern_scale_by_pattern_number_invert", "invert_number");
    ad.same("pattern_scale_by_ring_number", "scale_by_ring");
    ad.same("pattern_scale_by_ring_number_invert", "invert_ring");
    ad.same("pattern_rotation", "rotation");
    ad.same("pattern_rotation_random", "rotation_random");
    ad.same("center_orientation", "center_orientation");
    ad.same("ring_rotation", "ring_rotation");
    ad.same("ring_rotation_random", "ring_rotation_random");
    // the first pattern sits at the top of the ring
    ad.lin("ring_rotation_offset", 0, "ring_rotation_offset", 1.0, 0.75);
    ad.same("pattern_color", "color");
    ad.same("luminance_random", "luminance_random");
    ad.same("luminance_by_pattern_number", "lum_by_number");
    ad.same("luminance_by_ring_number", "lum_by_ring");
    if ad.take("luminance_by_scale") > 0.0 || ad.take_b("luminance_by_pattern_number_invert") || ad.take_b("luminance_by_ring_number_invert") {
        ad.note("some luminance settings are not reproduced");
    }
    ad.same("random_mask", "mask_random");
    ad.same("background_color", "background");
    scatter_blend(ad, "blending_mode", 1);
    ad.same("global_opacity", "opacity");
    for (i, mc) in ["pattern", "pattern2", "pattern3", "pattern4", "pattern5", "pattern6"].iter().enumerate() {
        ad.slot(&format!("pattern_input_{}", i + 1), mc);
    }
    ad.slot("background", "");
    ad.ignore(&["non_square_expansion"]);
}

/// Splatter Circular (older version).
fn splatter_circular_old(ad: &mut Ad) {
    ad.same("Number", "number");
    ad.same("Radius", "radius");
    ad.same("Radius_Random", "radius_random");
    let p = scatter_pattern(ad, "Pattern");
    scatter_specific(ad, "Pattern_Specific", p);
    let sc = ad.take("pattern_scale");
    let (sx, sy) = (ad.take_lane("PatternSize", 0), ad.take_lane("PatternSize", 1));
    ad.set("size", sx * sc);
    if (sx - sy).abs() > 1e-4 {
        ad.note("non-square pattern sizes are drawn square");
    }
    let r = ad.take("PatternSize_Random").max(ad.take("pattern_scale_random"));
    ad.set("size_random", r);
    ad.same("Position_Random", "position_random");
    ad.same("RandomMask", "mask_random");
    ad.same("Rotation", "rotation");
    ad.same("Rotation_Random", "rotation_random");
    ad.same("CenterOrientation", "center_orientation");
    ad.set("ring_rotation_offset", 0.75);
    scatter_blend(ad, "BlendingMode", 0);
    ad.same("Luminance_Random", "luminance_random");
    ad.ignore(&["image_input_filtering", "pattern_pivot", "Luminance_By_Scale"]);
    ad.slot("Source", "pattern");
    ad.slot("background", "");
}

/// Ropes and weaves: fixed patterns with a tiling, some with a 45 degree turn or a scale.
fn weave_common(ad: &mut Ad, preset: usize, tiling_mult: f32) {
    ad.set("preset", preset as f32);
    if ad.decl("Tiling") {
        ad.lin("Tiling", 0, "tiling", tiling_mult, 0.0);
    } else if tiling_mult != 1.0 {
        ad.set("tiling", tiling_mult);
    }
    if ad.decl("Rotate_45_Degrees") {
        ad.same("Rotate_45_Degrees", "rotate45");
    }
    if ad.decl("Rotation") && ad.take_b("Rotation") {
        ad.note("the pattern's rotation option is not reproduced");
    }
    ad.ignore(&["non_square_expansion"]);
}
fn rope(ad: &mut Ad) {
    weave_common(ad, 0, 1.0)
}
fn fibers_old(ad: &mut Ad) {
    weave_common(ad, 1, 2.0)
}
fn fibers_new(ad: &mut Ad) {
    weave_common(ad, 1, 1.0)
}
fn weave_1(ad: &mut Ad) {
    weave_common(ad, 2, 1.0)
}
fn weave_2_old(ad: &mut Ad) {
    // the older graph is turned by 45 degrees by default; that look is its own preset here
    let turned = ad.take_b("Rotate_45_Degrees");
    weave_common(ad, if turned { 3 } else { 4 }, 1.0);
    ad.set("rotate45", 0.0);
    if !turned {
        let t = ad.take("Tiling");
        ad.set("scale", 8.0 * t.max(1.0));
        ad.set("tiling", 1.0);
        ad.note("the knit without its 45 degree turn is approximated");
    }
    ad.lin("Disorder", 0, "disorder", 0.01, 0.0);
}
fn weave_2_new(ad: &mut Ad) {
    let turned = ad.take_b("rotate_45_degrees");
    weave_common(ad, if turned { 3 } else { 4 }, 1.0);
    ad.set("rotate45", 0.0);
    if turned {
        ad.note("the knit turned by 45 degrees is approximated");
    }
    ad.same("scale", "scale");
    ad.same("Disorder", "disorder");
}
fn weave_3(ad: &mut Ad) {
    weave_common(ad, 5, 1.0)
}
fn weave_4(ad: &mut Ad) {
    weave_common(ad, 6, 1.0)
}

/// Anisotropic Noise: rows of random values joined smoothly along each row.
fn anisotropic(ad: &mut Ad) {
    ad.set("style", 1.0);
    ad.same("X_Amount", "x_amount");
    ad.same("Y_Amount", "y_amount");
    // the older graph capitalises its settings
    let (sm, si) = if ad.decl("Smoothness") { ("Smoothness", "Smoothness_Interpolation") } else { ("smoothness", "smoothness_interpolation") };
    ad.same(sm, "smoothness");
    if ad.decl(si) {
        ad.same(si, "smoothness_interpolation");
    }
    ad.same("rotate", "rotate");
    let by_res = if ad.decl("Y_By_Resolution") { "Y_By_Resolution" } else { "y_amount_by_resolution" };
    if ad.take_b(by_res) {
        ad.set("rows_by_resolution", 1.0);
    }
    ad.ignore(&["non_square_expansion"]);
}

/// Weave Generator: a twill; the graph's outputs are ours by name.
fn weave_generator(ad: &mut Ad) {
    ad.set("style", 1.0);
    for k in ["tile_x", "tile_y", "weave", "shape"] {
        ad.same(k, k);
    }
    ad.vec("gap", &["gap_x", "gap_y"]);
    ad.out("Height", "height");
    ad.out("Opacity", "opacity");
    ad.out("Mask_1", "mask_1");
    ad.out("Mask_2", "mask_2");
    ad.ignore(&["non_square_expansion"]);
}

/// Sets fixed values (fitted looks of library graphs that have few or no settings of their own).
fn set_all(ad: &mut Ad, vals: &[(&str, f32)]) {
    for (k, v) in vals {
        ad.set(k, *v);
    }
}

/// Scratches: thin, mostly parallel strokes; the angles are given in degrees.
fn scratches_1(ad: &mut Ad) {
    set_all(ad, &[("count", 500.0), ("length", 45.0), ("thickness", 1.0), ("curl", 0.3), ("taper", 1.0), ("brightness_variation", 0.8), ("blend", 0.0)]);
    ad.lin("Rotation", 0, "angle", 1.0 / 360.0, 0.25);
    ad.lin("Rotation_Var", 0, "angle_variation", 1.0 / 360.0, 0.0);
    let d = ad.take("Disorder");
    ad.set("curl", 0.3 + d * 0.01);
    let (w, h) = (ad.take("Pattern_size_width"), ad.take("Pattern_size_height"));
    ad.set("length", 45.0 * w / 100.0);
    if (h - 100.0).abs() > 0.5 {
        ad.set("thickness", (h / 100.0).max(0.1));
    }
}

/// Thorns 1 to 3: dense short strokes in every direction that add up where they cross.
fn thorns(ad: &mut Ad, gain: f32) {
    set_all(ad, &[("count", 4557.0), ("length", 79.6), ("thickness", 0.6), ("angle", 0.0), ("angle_variation", 0.5), ("curl", 0.033), ("taper", 0.57), ("brightness_variation", 0.21), ("blend", 1.0), ("gain", gain)]);
    if ad.decl("Pattern_size_width") {
        let w = ad.take("Pattern_size_width");
        ad.set("length", 79.6 * w / 100.0);
        if (ad.take("Pattern_size_height") - 100.0).abs() > 0.5 {
            ad.note("the thorns' height setting is not reproduced");
        }
    }
    if ad.decl("Disorder") {
        let d = ad.take("Disorder");
        ad.set("curl", 0.033 + d * 0.01);
    }
}
fn thorns_1(ad: &mut Ad) {
    thorns(ad, 0.585)
}
fn thorns_2(ad: &mut Ad) {
    thorns(ad, 0.41)
}
fn thorns_3(ad: &mut Ad) {
    thorns(ad, 0.5)
}

/// Fur 1 to 4: hairs combed one way, swaying in waves or following a swirling field. Their settings scale the
/// number and length of the hairs and add disorder to their direction.
fn fur(ad: &mut Ad, which: u8) {
    let base: &[(&str, f32)] = match which {
        1 => &[("count", 1800.0), ("length", 110.0), ("thickness", 1.8), ("angle", 0.25), ("angle_variation", 0.05), ("curl", 0.25), ("taper", 0.5), ("root_shade", 0.4), ("brightness_variation", 0.7), ("flow", 0.05), ("flow_scale", 4.0)],
        2 => &[("count", 7000.0), ("length", 45.0), ("thickness", 1.0), ("angle", 0.0), ("angle_variation", 0.03), ("curl", 0.2), ("taper", 0.6), ("root_shade", 0.2), ("brightness_variation", 0.7), ("flow", 0.3), ("flow_scale", 2.0)],
        3 if ad.decl("scale") => &[("count", 3274.0), ("length", 60.0), ("thickness", 1.12), ("angle", 0.25), ("angle_variation", 0.054), ("curl", 0.075), ("taper", 0.4), ("root_shade", 0.51), ("brightness_variation", 0.69), ("flow", 0.3), ("flow_mode", 1.0), ("waves", 2.0)],
        3 => &[("count", 5673.0), ("length", 60.0), ("thickness", 0.83), ("angle", 0.25), ("angle_variation", 0.027), ("curl", 0.36), ("taper", 0.42), ("root_shade", 0.89), ("brightness_variation", 0.2), ("flow", 0.3), ("flow_mode", 1.0), ("waves", 2.0)],
        _ => &[("count", 4000.0), ("length", 110.0), ("thickness", 1.5), ("angle", 0.25), ("angle_variation", 0.05), ("curl", 0.5), ("taper", 0.6), ("root_shade", 0.6), ("brightness_variation", 0.3), ("flow", 0.5), ("flow_scale", 1.0)],
    };
    set_all(ad, base);
    let get = |k: &str| base.iter().find(|(n, _)| *n == k).map(|x| x.1).unwrap_or(0.0);
    if ad.decl("scale") {
        let sc = ad.take("scale").max(1.0) / if which == 1 { 2.0 } else { 1.0 };
        ad.set("count", (get("count") * sc * sc).min(20000.0));
        ad.set("length", get("length") / sc);
        ad.set("thickness", get("thickness") / sc.sqrt());
    }
    // the older graphs give the disorder on 0..100
    if ad.decl("Disorder") {
        let d = ad.take("Disorder");
        let percent = ad.lib.and_then(|l| l.input("Disorder")).and_then(|i| i.range).map(|r| r.1 > 1.0).unwrap_or(false);
        let d = if percent { d * 0.01 } else { d };
        ad.set("angle_variation", get("angle_variation") + d * 0.15);
    }
    if ad.decl("Density") {
        let d = ad.take("Density");
        ad.set("count", get("count") * d / 0.6);
    }
    if ad.decl("waves_amount") {
        ad.lin("waves_amount", 0, "waves", 1.0, 0.0);
    }
    for k in ["Waves_Scale", "Waves_Rotation"] {
        if ad.decl(k) && (ad.take(k) - ad.def(k, 0)).abs() > 1e-4 {
            ad.note("the waves' settings are not reproduced");
        }
    }
    ad.ignore(&["non_square_expansion"]);
    ad.note("fur is drawn with MaterialCraft's own hairs; the look is similar, not identical");
}
fn fur_1(ad: &mut Ad) {
    fur(ad, 1)
}
fn fur_2(ad: &mut Ad) {
    fur(ad, 2)
}
fn fur_3(ad: &mut Ad) {
    fur(ad, 3)
}
fn fur_4(ad: &mut Ad) {
    fur(ad, 4)
}

/// Waveform 1: soft bars like a sound wave; its brightness tops out at an eighth.
fn waveform_1(ad: &mut Ad) {
    ad.set("style", 1.0);
    ad.set("intensity", 0.125);
    ad.same("Samples", "samples");
    ad.same("SizeMin", "size_min");
    ad.same("SizeMax", "size_max");
    ad.same("WaveNumber", "wave_number");
    ad.same("Noise", "noise");
    let p = ad.take_i("Pattern");
    if p != 6 {
        ad.note(format!("bar pattern {p} is drawn as soft bars"));
    }
}

// ------------------------------------------------------------------------------------------------ filters

/// The library's normal format setting: 0 is DirectX, 1 OpenGL.
fn normal_format(ad: &mut Ad, sd: &str) {
    if ad.decl(sd) {
        let f = ad.take_i(sd);
        ad.set("directx", if f == 0 { 1.0 } else { 0.0 });
    }
}

/// Bevel: distance (negative grows the bevel outwards), corner type, normal output and its format.
fn bevel(ad: &mut Ad) {
    ad.same("distance", "distance");
    ad.lin("smoothing", 0, "smoothing", 0.2, 0.0);
    let m = ad.take_i("bevel_mode");
    ad.set("corner", if m == 1 { 1.0 } else { 0.0 });
    ad.same("normal_intensity", "normal_intensity");
    normal_format(ad, "normal_format");
    if ad.take_b("Use_Custom_Curve") {
        ad.note("the custom curve input is not reproduced");
    }
    ad.ignore(&["non_uniform_blur"]);
    ad.slot("input", "in");
    ad.slot("custom_curve", "");
    ad.out("height", "height");
    ad.out("normal", "normal");
}

/// Slope Blur: the mode numbers are blend modes of the format (5 max, 6 min, 7 the blur).
fn slope_blur(ad: &mut Ad) {
    ad.same("Samples", "samples");
    // the format's intensity counts pixels of a 256-pixel image (default 10); ours is a share of the image width
    ad.lin("Intensity", 0, "intensity", 1.0 / 256.0, 0.0);
    let m = ad.take_i("mode");
    ad.set("mode", match m {
        5 => 2.0,
        6 => 1.0,
        _ => 0.0,
    });
    if !matches!(m, 5..=7) {
        ad.note(format!("slope blur mode {m} is drawn as a blur"));
    }
    ad.slot("Source", "in");
    ad.slot("Effect", "slope");
}

fn clamp(ad: &mut Ad) {
    ad.same("min", "low");
    ad.same("max", "high");
    if ad.decl("apply_to_alpha") {
        ad.same("apply_to_alpha", "apply_alpha");
    }
    ad.slot("input", "in");
}

fn color_to_mask(ad: &mut Ad) {
    ad.vec("color", &["r", "g", "b"]);
    ad.same("mask_range", "range");
    ad.same("mask_softness", "softness");
    if ad.take_i("keying_type") != 0 {
        ad.note("the keying type is approximated by a colour distance");
    }
    ad.ignore(&["flatten_alpha"]);
    ad.slot("Input", "in");
}

fn curvature(ad: &mut Ad) {
    ad.set("mode", 0.0);
    ad.same("intensity", "intensity");
    normal_format(ad, "normal_format");
    if ad.take_b("invert_green") {
        let dx = ad.c.node.nums.get("directx").copied().unwrap_or(1.0);
        ad.set("directx", 1.0 - dx);
    }
    ad.slot("Input", "in");
}

fn glow(ad: &mut Ad) {
    // the glow size is given on 0..20; ours is in pixels at 512
    ad.lin("Glow_Size", 0, "width", 1.6, 0.0);
    ad.lin("Glow_Amount", 0, "intensity", 2.0, 0.0);
    ad.same("Clear_Amount", "clear");
    if ad.decl("Glow_Color") {
        ad.vec("Glow_Color", &["glow_r", "glow_g", "glow_b"]);
    }
    ad.slot("Source", "in");
    ad.note("the glow's size and strength are converted approximately");
}

fn histogram_scan(ad: &mut Ad) {
    let inv = ad.take_b("Invert_Position");
    let pos = ad.take("Position");
    ad.set("position", if inv { 1.0 - pos } else { pos });
    ad.set("invert", if inv { 1.0 } else { 0.0 });
    ad.same("Contrast", "contrast");
    ad.slot("Input_1", "in");
}

fn mirror(ad: &mut Ad) {
    let t = ad.take_i("mirror_type");
    ad.set("axis", match t {
        0 => 0.0,
        1 => 1.0,
        2 => 3.0,
        _ => 2.0,
    });
    ad.same("axis_x", "offset_x");
    ad.same("axis_y", "offset_y");
    ad.same("invert_x", "invert_x");
    ad.same("invert_y", "invert_y");
    ad.same("corner_type", "corner");
    ad.slot("input", "in");
    if t > 2 {
        ad.note(format!("mirror type {t} is approximated"));
    }
}

fn mosaic(ad: &mut Ad) {
    ad.same("Samples", "count");
    ad.same("Intensity", "intensity");
    ad.slot("Source", "in");
    ad.slot("Effect", "mosaic_map");
}

fn multi_directional_warp(ad: &mut Ad) {
    ad.lin("intensity", 0, "intensity", 0.01, 0.0);
    ad.same("warp_angle", "angle");
    ad.same("mode", "mode");
    let d = ad.take_i("directions");
    ad.set("directions", match d {
        1 => 0.0,
        2 => 1.0,
        _ => 2.0,
    });
    if !matches!(d, 1 | 2 | 4) {
        ad.note(format!("{d} warp directions are approximated by four"));
    }
    ad.slot("input", "in");
    ad.slot("intensity_input", "warp");
}

fn multi_switch(ad: &mut Ad) {
    ad.lin("input_selection", 0, "index", 1.0, -1.0);
    for (i, mc) in ["a", "b", "c", "d", "e", "f", "g", "h"].iter().enumerate() {
        ad.slot(&format!("input_{}", i + 1), mc);
    }
    for i in 9..=20 {
        ad.slot(&format!("input_{i}"), "");
    }
    if ad.take_i("input_selection") > 8 {
        ad.note("only the first eight inputs can be chosen here");
    }
    ad.ignore(&["input_number"]);
}

fn switch(ad: &mut Ad) {
    let on = ad.take_b("switch");
    ad.set("index", if on { 0.0 } else { 1.0 });
    ad.slot("input_1", "a");
    ad.slot("input_2", "b");
}

fn non_uniform_directional_warp(ad: &mut Ad) {
    ad.lin("intensity", 0, "intensity", 0.01, 0.0);
    ad.same("warp_angle", "angle");
    ad.slot("input", "in");
    ad.slot("intensity_input", "intensity");
    ad.slot("warp_angle_input", "angle");
    if ad.take("trail_length") > 0.0 {
        ad.note("the warp trail is not reproduced");
    }
    ad.ignore(&["warp_angle_input_multiplier", "trail_mode", "trail_fade", "trail_curve"]);
}

fn normal_blend(ad: &mut Ad) {
    ad.same("Opacity", "opacity");
    ad.slot("NormalFG", "b");
    ad.slot("NormalBG", "a");
    let use_mask = ad.take_b("Use_Mask");
    ad.slot("Mask", if use_mask { "mask" } else { "" });
}

fn normal_invert(ad: &mut Ad) {
    ad.same("invert_red", "invert_x");
    ad.same("invert_green", "invert_y");
    ad.same("invert_blue", "invert_z");
    ad.same("invert_alpha", "invert_a");
    ad.slot("Normal", "in");
}

fn normal_to_height(ad: &mut Ad) {
    if ad.decl("Relief_Balance") {
        ad.lin("Relief_Balance", 0, "relief_balance", 1.0, 0.0);
    }
    normal_format(ad, "normal_format");
    if ad.decl("global_opacity") {
        ad.same("global_opacity", "opacity");
    }
    ad.slot("Input", "in");
    ad.slot("Normal", "in");
}

fn normal_vector_rotation(ad: &mut Ad) {
    ad.same("rotation_angle", "angle");
    normal_format(ad, "normal_format");
    ad.slot("Normal", "in");
    ad.slot("rotation_map", "rotation");
}

fn quad_transform(ad: &mut Ad) {
    ad.vec("p00", &["tl_x", "tl_y"]);
    ad.vec("p10", &["tr_x", "tr_y"]);
    ad.vec("p11", &["br_x", "br_y"]);
    ad.vec("p01", &["bl_x", "bl_y"]);
    ad.same("enable_tiling", "tile");
    ad.ignore(&["culling", "sampling", "background_color"]);
    ad.slot("input", "in");
}

fn quantize(ad: &mut Ad) {
    if ad.decl("Quantize_R") {
        let r = ad.take("Quantize_R");
        ad.set("steps", r);
        let differ = ["Quantize_G", "Quantize_B"].iter().any(|k| (ad.take(k) - r).abs() > 0.5);
        if differ {
            ad.note("one number of steps is used for every channel");
        }
        ad.ignore(&["Quantize_A"]);
    } else if ad.decl("Quantize") {
        ad.same("Quantize", "steps");
    } else {
        ad.same("steps", "steps");
        if ad.take("offset") != 0.0 || ad.take("slope") != 0.0 {
            ad.note("the steps' offset and slope are not reproduced");
        }
        ad.ignore(&["slope_curve"]);
        ad.slot("curve_input", "");
    }
    ad.slot("Input", "in");
    ad.slot("input", "in");
}

fn radial_blur(ad: &mut Ad) {
    ad.set("mode", 0.0);
    ad.same("samples", "samples");
    ad.same("angle", "angle");
    ad.vec("center_position", &["x", "y"]);
    ad.slot("input", "in");
}

fn replace_color(ad: &mut Ad) {
    ad.vec("SourceColor", &["src_r", "src_g", "src_b"]);
    ad.vec("TargetColor", &["dst_r", "dst_g", "dst_b"]);
    ad.slot("Input", "in");
}

fn trapezoid(ad: &mut Ad) {
    ad.same("top_stretch", "top");
    ad.same("bottom_stretch", "bottom");
    ad.ignore(&["sampling", "background_color"]);
    ad.slot("input", "in");
}

fn vector_warp(ad: &mut Ad) {
    ad.same("intensity", "intensity");
    normal_format(ad, "vector_format");
    ad.slot("input", "in");
    ad.slot("vector_map", "vector");
}

fn height_blend(ad: &mut Ad) {
    ad.same("opacity", "opacity");
    ad.same("contrast", "contrast");
    if (ad.take("height_position") - 0.5).abs() > 1e-4 {
        ad.note("the height position is not reproduced");
    }
    if ad.take_i("mode") != 0 {
        ad.note("the blend mode is approximated");
    }
    ad.slot("height_top", "fg");
    ad.slot("height_bottom", "bg");
    ad.slot("mask", "mask");
}

fn emboss_with_gloss(ad: &mut Ad) {
    // the source lit by the height map, with coloured highlights and shadows and a glossy specular highlight
    ad.set("mode", 1.0);
    ad.same("Intensity", "intensity");
    ad.same("LightAngle", "angle");
    ad.vec("HighlightColor", &["hl_r", "hl_g", "hl_b"]);
    ad.vec("ShadowColor", &["sh_r", "sh_g", "sh_b"]);
    ad.same("Gloss", "gloss");
    ad.slot("Source", "in");
    ad.slot("Height", "height");
    ad.note("the emboss lighting and gloss follow Substance Designer's model; their strength is not yet matched against a reference render");
}

fn non_uniform_blur(ad: &mut Ad) {
    ad.same("Intensity", "intensity");
    if ad.take("Anisotropy") > 0.0 || ad.take("Asymmetry") > 0.0 {
        ad.note("an anisotropic or asymmetric blur is approximated by a round one");
    }
    ad.ignore(&["Angle", "Samples", "Blades"]);
    ad.slot("Source", "in");
    ad.slot("Effect", "mask");
}

fn safe_transform(ad: &mut Ad) {
    let safe = ad.take_b("tile_safe_rotation");
    ad.set("mode", if safe { 0.0 } else { 1.0 });
    ad.lin("tile", 0, "scale_x", 1.0, 0.0);
    ad.lin("tile", 0, "scale_y", 1.0, 0.0);
    ad.same("rotation", "angle");
    ad.vec("offset", &["offset_x", "offset_y"]);
    if ad.take_i("symmetry") != 0 || ad.take_i("offset_mode") != 0 {
        ad.note("the symmetry and offset mode settings are not reproduced");
    }
    ad.ignore(&["background_color", "mipmapmode", "manualmiplevel"]);
    ad.slot("input", "in");
}

fn normal_combine(ad: &mut Ad) {
    ad.same("blend_quality", "technique");
    ad.slot("Input", "base");
    ad.slot("Input_1", "detail");
}

/// Microscope View: cells floating at different depths.
fn microscope(ad: &mut Ad) {
    ad.set("style", 1.0);
    if ad.lib.map_or(false, |l| l.inputs.iter().any(|i| i.id == "Warp_intensity")) {
        ad.lin("Warp_intensity", 0, "warp", 1.0 / 24.0, 0.0);
        // the number of cells follows the scale (the look was fitted at the library default)
        let d = ad.def("scale", 0).max(1e-3);
        let x = ad.take("scale");
        ad.set("density", (14.0 * x / d).round().clamp(2.0, 64.0));
        ad.set("cell_size", 0.014 * d / x.max(1e-3));
    } else {
        ad.same("warp_intensity", "warp");
        ad.set("density", 10.0);
        ad.set("cell_size", 0.02);
        if (ad.take("scale") - ad.def("scale", 0)).abs() > 0.5 {
            ad.note("the cell scale is only matched at its default");
        }
    }
    if ad.take("Disorder") > 0.0 {
        ad.note("the disorder is not reproduced");
    }
    ad.ignore(&["non_square_expansion"]);
}

/// Fractal Sum Base 2: the fractal sum base look; its octave range is only matched at its defaults.
fn p_fsum_base_2(ad: &mut Ad) {
    preset(ad, 6);
    if ad.take_i("MinLevel") != 1 || ad.take_i("MaxLevel") != 11 {
        ad.note("the octave range is only matched at its defaults");
    }
}

/// Star: a five-pointed star with straight edges, a point at the top.
fn star(ad: &mut Ad) {
    set_all(ad, &[("points", 5.0), ("inner", 0.382), ("straight", 1.0), ("rotation", 0.9), ("softness", 0.0)]);
    ad.lin("scale", 0, "size", 0.5, 0.0);
    ad.note("the star is not yet checked against a reference render");
    ad.ignore(&["non_square_expansion"]);
}

/// Fixed tile patterns with a tiling or scale.
fn tile_common(ad: &mut Ad, preset: usize, scale_key: &str) {
    ad.set("preset", preset as f32);
    if ad.decl("Tiling") {
        ad.same("Tiling", "tiling");
    } else if ad.decl(scale_key) {
        ad.same(scale_key, "tiling");
    }
    ad.ignore(&["non_square_expansion"]);
}
fn brick_1(ad: &mut Ad) {
    tile_common(ad, 0, "scale");
    ad.same("Interstice_Width", "interstice");
    ad.same("Edge_Smoothness", "edge_smoothness");
}
fn brick_2(ad: &mut Ad) {
    tile_common(ad, 1, "scale");
    // the current version puts the joint of its long rows in the middle
    if ad.decl("scale") {
        ad.set("variant", 1.0);
    }
    ad.same("Interstice_Width", "interstice");
    ad.same("Edge_Smoothness", "edge_smoothness");
}
fn brick_3(ad: &mut Ad) {
    tile_common(ad, 2, "");
    ad.same("Roundness", "roundness");
    ad.same("Lum_var", "lum_var");
    ad.same("Lum_seed", "seed");
    if ad.take("Disorder") > 0.0 {
        ad.note("the bricks' disorder is not reproduced");
    }
    ad.ignore(&["Random_position"]);
}
fn brick_generator(ad: &mut Ad) {
    ad.set("preset", 3.0);
    ad.vec("Bricks", &["bricks_x", "bricks_y"]);
    ad.vec("Bevel", &["bevel_x", "bevel_y"]);
    ad.vec("Gap", &["gap_x", "gap_y"]);
    ad.lin("Offset", 0, "offset", 1.0, 0.0);
    let changed = |ad: &mut Ad, k: &str, lanes: usize| (0..lanes).any(|i| (ad.take_lane(k, i) - ad.def(k, i)).abs() > 1e-4);
    if changed(ad, "Middle_Size", 2) || changed(ad, "Height", 4) || changed(ad, "Slope", 4) {
        ad.note("the bricks' middle size, height and slope settings are not reproduced");
    }
    ad.ignore(&["Keep_Ratio", "non_square_expansion"]);
}
fn alveolus(ad: &mut Ad) {
    tile_common(ad, 4, "");
    if ad.take_b("Gradient_Filled_Cells") || ad.take("Interstice_Width") > 0.0 {
        ad.note("the cells' fill and interstice settings are not reproduced");
    }
}
fn gaussian_1(ad: &mut Ad) {
    tile_common(ad, 5, "");
}
fn gaussian_2(ad: &mut Ad) {
    tile_common(ad, 6, "");
}
fn mesh_1(ad: &mut Ad) {
    tile_common(ad, 7, "");
    let r45 = if ad.decl("Rotate_45_Degrees") { ad.take_b("Rotate_45_Degrees") } else { false };
    if r45 {
        ad.note("the mesh turned by 45 degrees is not reproduced");
    }
}
fn mesh_2(ad: &mut Ad) {
    tile_common(ad, 8, "");
    if ad.decl("Rotation") && ad.take_b("Rotation") {
        ad.note("the mesh's rotation option is not reproduced");
    }
}
fn parquet(ad: &mut Ad) {
    tile_common(ad, 9, "");
    ad.same("Luminance_Variation", "lum_var");
}

/// Triangle Grid: our jittered-mesh style follows the same settings.
fn triangle_grid(ad: &mut Ad) {
    ad.set("style", 1.0);
    for k in ["x_amount", "y_amount", "random_position_multiplier", "quincunx_offset_x", "quincunx_offset_y", "triangulation", "height_random", "distance_to_edge_thickness"] {
        ad.same(k, k);
    }
    ad.same("color_output", "output");
    ad.lin("thin_line_color", 0, "thin_line_color", 1.0, 0.0);
    ad.lin("thin_line_background", 0, "thin_line_background", 1.0, 0.0);
    let rp = (ad.take_lane("random_position", 0), ad.take_lane("random_position", 1));
    if (rp.0 - 1.0).abs() > 1e-4 || (rp.1 - 1.0).abs() > 1e-4 {
        ad.note("separate random position amounts per axis are not reproduced");
    }
    if ad.take("rotation") != 0.0 || ad.take("vector_map_displacement") != 0.0 || ad.take("height_input_multiplier") != 0.0 {
        ad.note("rotation and map-driven settings of the triangle grid are not reproduced");
    }
    ad.ignore(&["rotation_disorder", "blend_mode", "thin_line_blend_mode", "distance_blend_mode", "color_source", "color_source_opacity", "random_color_seed_mode", "random_color_seed", "non_square_expansion"]);
    ad.slot("height", "");
    ad.slot("vector_map", "");
    ad.slot("color_input", "");
}

fn white_noise(_ad: &mut Ad) {}

/// Voronoi: our cellular node follows the same settings.
fn voronoi(ad: &mut Ad) {
    ad.same("scale", "scale");
    ad.lin("disorder", 0, "jitter", 1.0, 0.0);
    let (sx, sy) = (ad.take_lane("size", 0), ad.take_lane("size", 1));
    ad.set("aspect", if sx > 1e-4 { sy / sx } else { 1.0 });
    ad.vec("offset", &["offset_x", "offset_y"]);
    ad.same("distortion_intensity", "distortion");
    ad.lin("distortion_scale_multiplier", 0, "distortion_scale", 3.0, 0.0);
    ad.same("rounded_curve", "rounded");
    ad.same("distance_scale", "distance_scale");
    ad.same("distance_mode", "distance_mode");
    ad.same("minkowski_number", "minkowski");
    ad.same("style", "mode");
    ad.same("edge_thickness", "edge_thickness");
    ad.same("random_color_seed_mode", "color_seed_mode");
    ad.same("random_color_seed", "color_seed");
    ad.same("invert", "invert");
    ad.ignore(&["non_square_expansion"]);
}

/// Tile Random: rows of cells of random widths, split once at random.
fn tile_random(ad: &mut Ad) {
    ad.set("preset", 10.0);
    ad.same("x_amount", "bricks_x");
    ad.same("y_amount", "bricks_y");
    ad.same("size_random_x", "size_random");
    ad.same("interstice_amount", "interstice");
    ad.note("random tiles follow the same rules with their own random choices");
    ad.ignore(&["non_square_expansion"]);
}
/// Tile Random 2: a patchwork of random tiles (our own Random Tiles 2 node).
fn tile_random_2(ad: &mut Ad) {
    ad.same("amount_x", "amount_x");
    ad.same("amount_y", "amount_y");
    ad.same("random_size_multiplier", "random_size");
    ad.same("random_size_x", "random_size_x");
    ad.same("random_size_y", "random_size_y");
    ad.same("interstice_thickness", "interstice");
    ad.same("random_position_multiplier", "random_position");
    ad.same("round_corners_radius", "round_corners");
    ad.same("shape_scale_multiplier", "scale");
    ad.same("shape_scale_random_multiplier", "scale_random");
    ad.same("shape_rotation", "rotation");
    ad.same("shape_rotation_random", "rotation_random");
    ad.same("shape_corners_position_random", "corners_random");
    ad.same("random_slant_multiplier", "slant");
    ad.same("random_slant_x", "slant_x");
    ad.same("random_slant_y", "slant_y");
    ad.same("bevel_distance_multiplier", "bevel");
    ad.same("enable_per_axis_bevel_control", "bevel_per_axis");
    ad.same("bevel_distance_x", "bevel_x");
    ad.same("bevel_distance_y", "bevel_y");
    ad.same("bevel_rounded_curve", "bevel_curve");
    ad.same("mask_random_invert", "mask_random_invert");
    ad.same("mask_random_start", "mask_random_start");
    ad.same("mask_random_end", "mask_random_end");
    // the maps that steer size, slant, corners, bevel and masking per tile are not read
    ad.ignore(&["random_size_distribution", "random_size_input_map_multiplier", "round_corners_radius_input_map_multiplier", "random_slant_distribution", "random_slant_input_map_multiplier", "bevel_distance_mode", "bevel_distance_input_map_multiplier", "mask_by_cell_area_invert", "mask_by_cell_area_start", "mask_by_cell_area_end", "mask_map_input_invert", "mask_map_input_start", "mask_map_input_end", "non_square_expansion"]);
}

/// 3D noises read at a position map: our 3D Noise node, of the matching type.
fn noise_3d(ad: &mut Ad, kind: i32) {
    ad.set("type", kind as f32);
    ad.same("scale", "scale");
    if ad.decl("size") {
        ad.vec("size", &["size_x", "size_y", "size_z"]);
    }
    if ad.decl("offset") {
        ad.vec("offset", &["offset_x", "offset_y", "offset_z"]);
    }
    for (sd, mc) in [("distortion_intensity", "distortion"), ("distortion_scale_multiplier", "distortion_scale"), ("baseline", "baseline"), ("contrast", "noise_contrast"), ("absolute", "absolute"), ("min_level", "min_level"), ("max_level", "max_level"), ("roughness", "roughness"), ("lacunarity", "lacunarity"), ("global_opacity", "opacity"), ("disorder", "jitter"), ("style", "style"), ("edge_thickness", "edge_thickness"), ("invert", "invert")] {
        if ad.decl(sd) {
            ad.same(sd, mc);
        }
    }
    if ad.decl("border_width") {
        ad.same("border_width", "edge_thickness");
    }
    ad.slot("position", "position");
    ad.ignore(&["enable_tiling", "distance_scale", "distance_mode", "minkowski_number", "rounded_curve", "random_color_seed_mode", "random_color_seed", "blend_mode", "mode", "roundness"]);
    ad.note("without a position map the noise is read on the texture plane");
}
fn n3_perlin(ad: &mut Ad) {
    noise_3d(ad, 0)
}
fn n3_perlin_fractal(ad: &mut Ad) {
    noise_3d(ad, 1)
}
fn n3_ridged(ad: &mut Ad) {
    noise_3d(ad, 2)
}
fn n3_simplex(ad: &mut Ad) {
    noise_3d(ad, 3)
}
fn n3_worley(ad: &mut Ad) {
    noise_3d(ad, 4)
}
fn n3_voronoi(ad: &mut Ad) {
    noise_3d(ad, 5)
}
fn n3_voronoi_fractal(ad: &mut Ad) {
    noise_3d(ad, 6)
}

fn voronoi_fractal(ad: &mut Ad) {
    voronoi(ad);
    for (sd, mc) in [("min_level", "min_level"), ("max_level", "max_level"), ("roughness", "roughness"), ("lacunarity", "lacunarity"), ("global_opacity", "opacity"), ("blend_mode", "blend")] {
        ad.same(sd, mc);
    }
}

/// Strand looks fitted to the messy-fibre graphs; the current versions scale the count and size.
fn strands_scaled(ad: &mut Ad, base: &[(&str, f32)]) {
    set_all(ad, base);
    let get = |k: &str| base.iter().find(|(n, _)| *n == k).map(|x| x.1).unwrap_or(0.0);
    if ad.decl("scale") {
        let sc = ad.take("scale").max(1.0);
        ad.set("count", (get("count") * sc * sc).min(20000.0));
        ad.set("length", get("length") / sc);
        ad.set("thickness", get("thickness") / sc.sqrt());
    }
    if ad.decl("angle") {
        ad.same("angle", "angle");
    }
    if ad.decl("angle_random") {
        ad.same("angle_random", "angle_variation");
    }
    if ad.decl("Rotation_Var") {
        ad.lin("Rotation_Var", 0, "angle_variation", 1.0 / 720.0, 0.0);
    }
    if ad.decl("Rotation") {
        ad.lin("Rotation", 0, "angle", 1.0 / 360.0, 0.0);
    }
    let d = if ad.decl("Disorder") { ad.take("Disorder") * if ad.lib.and_then(|l| l.input("Disorder")).and_then(|i| i.range).map(|r| r.1 > 1.0).unwrap_or(false) { 0.01 } else { 1.0 } } else { 0.0 };
    if d > 0.0 {
        ad.set("curl", get("curl") + d);
    }
    ad.ignore(&["non_square_expansion", "color_random"]);
}
fn messy_lines(ad: &mut Ad) {
    let base = [("count", 357.0), ("length", 222.8), ("thickness", 7.56), ("angle", 0.0), ("angle_variation", 0.5), ("curl", 0.094), ("taper", 0.35), ("brightness_variation", 0.27), ("blend", 1.0), ("gain", 0.266), ("soft", 1.0)];
    strands_scaled(ad, &base);
    for k in ["Lines_Number", "links_number"] {
        if ad.decl(k) {
            let n = ad.take(k);
            let d = ad.def(k, 0).max(1.0);
            let sc = if ad.decl("scale") { ad.take("scale").max(1.0) } else { 1.0 };
            ad.set("count", (357.0 * n / d * sc * sc).min(20000.0));
        }
    }
}
fn messy_spikes(ad: &mut Ad) {
    let base: &[(&str, f32)] = if ad.decl("scale") {
        &[("count", 1818.0), ("length", 186.5), ("thickness", 2.2), ("angle", 0.0), ("angle_variation", 0.5), ("curl", 0.3), ("taper", 0.95), ("brightness_variation", 0.89), ("blend", 1.0), ("gain", 0.512)]
    } else {
        &[("count", 3996.0), ("length", 82.0), ("thickness", 1.2), ("angle", 0.0), ("angle_variation", 0.5), ("curl", 0.56), ("taper", 0.19), ("brightness_variation", 0.36), ("blend", 1.0), ("gain", 0.925)]
    };
    strands_scaled(ad, base);
}

/// Scratches Generator: strokes of random length, width and brightness.
fn scratches_generator(ad: &mut Ad) {
    set_all(ad, &[("thickness", 1.0), ("taper", 1.0), ("curl", 0.2), ("brightness_variation", 1.0), ("blend", 0.0)]);
    if ad.decl("spline_number") {
        ad.same("spline_number", "count");
        ad.lin("spline_scale", 0, "length", 512.0, 0.0);
        ad.lin("spline_rotation", 0, "angle", 1.0, 0.0);
        ad.lin("spline_rotation_random", 0, "angle_variation", 0.5, 0.0);
        ad.same("spline_width", "thickness");
        ad.same("luminance_random", "brightness_variation");
        ad.lin("spline_distortion", 0, "curl", 2.0, 0.2);
        ad.ignore(&["max_segments_per_spline", "spline_scale_random", "spline_distortion_random", "spline_distortion_frequency", "spline_width_random", "spline_position_random", "spline_width_in_pixel", "fade_mode", "fade_length", "non_square_expansion"]);
    } else {
        ad.same("scratches_amount", "count");
        ad.lin("scratches_scale", 0, "length", 512.0, 0.0);
        ad.set("angle_variation", 0.5);
        ad.ignore(&["mask_threshold"]);
        ad.slot("mask", "");
    }
    ad.note("the scratches are drawn with MaterialCraft's own strokes");
}

// ------------------------------------------------------------------------------------------------ filters (2)

/// The library's blend graphs (one per mode) are our Blend node in that mode.
fn lib_blend(ad: &mut Ad, mode: f32) {
    ad.set("mode", mode);
    ad.same("Opacity", "opacity");
    // with alpha blending the foreground's alpha decides where it shows
    let alpha = if ad.decl("Alpha_Blending") { ad.take_b("Alpha_Blending") } else { true };
    ad.set("alpha_mode", if alpha { 0.0 } else { 1.0 });
    ad.slot("Foreground", "fg");
    ad.slot("Background", "bg");
    ad.slot("Mask", "mask");
}
/// Blend: the general one, its blending mode numbered as on the built-in blend.
fn bl_blend(ad: &mut Ad) {
    let n = ad.take_i("Blending_Mode");
    let mode = crate::sbs_import::blend_mode(n).unwrap_or_else(|| {
        ad.note(format!("blending mode {n} is not known; Copy is used"));
        0.0
    });
    lib_blend(ad, mode)
}
fn bl_add(ad: &mut Ad) {
    lib_blend(ad, 1.0)
}
fn bl_add_sub(ad: &mut Ad) {
    lib_blend(ad, 20.0)
}
/// Color: hue and saturation of the foreground on the brightness of the background.
fn bl_color(ad: &mut Ad) {
    lib_blend(ad, 17.0)
}
fn bl_color_burn(ad: &mut Ad) {
    lib_blend(ad, 11.0)
}
fn bl_color_dodge(ad: &mut Ad) {
    lib_blend(ad, 10.0)
}
fn bl_copy(ad: &mut Ad) {
    lib_blend(ad, 0.0)
}
fn bl_difference(ad: &mut Ad) {
    lib_blend(ad, 8.0)
}
fn bl_dissolve(ad: &mut Ad) {
    lib_blend(ad, 19.0)
}
fn bl_exclusion(ad: &mut Ad) {
    lib_blend(ad, 14.0)
}
fn bl_hard_light(ad: &mut Ad) {
    lib_blend(ad, 13.0)
}
fn bl_linear_burn(ad: &mut Ad) {
    lib_blend(ad, 12.0)
}
fn bl_luminosity(ad: &mut Ad) {
    lib_blend(ad, 18.0)
}
fn bl_max(ad: &mut Ad) {
    lib_blend(ad, 6.0)
}
fn bl_min(ad: &mut Ad) {
    lib_blend(ad, 7.0)
}
fn bl_multiply(ad: &mut Ad) {
    lib_blend(ad, 3.0)
}
fn bl_overlay(ad: &mut Ad) {
    lib_blend(ad, 5.0)
}
fn bl_screen(ad: &mut Ad) {
    lib_blend(ad, 4.0)
}
fn bl_soft_light(ad: &mut Ad) {
    lib_blend(ad, 9.0)
}
fn bl_subtract(ad: &mut Ad) {
    lib_blend(ad, 2.0)
}

/// Blur HQ: the same reach as the Blur node; quality 0 / 1 are two and four passes here.
fn blur_hq(ad: &mut Ad) {
    ad.lin("Intensity", 0, "intensity", 2.0 / 3.0, 0.0);
    let q = ad.take("Quality");
    ad.set("quality", if q > 0.5 { 4.0 } else { 2.0 });
    ad.slot("Source", "in");
}

fn one_input(ad: &mut Ad) {
    ad.slot("input", "in");
}

fn curvature_sobel(ad: &mut Ad) {
    ad.set("mode", 1.0);
    ad.lin("intensity", 0, "intensity", 8.0, 0.0);
    normal_format(ad, "input_normal_type");
    ad.slot("normal", "in");
}

fn curvature_smooth_v2(ad: &mut Ad) {
    ad.lin("radius", 0, "smoothing", 2.0, 0.0);
    normal_format(ad, "normal_format");
    ad.slot("input", "in");
    ad.out("curvature", "output");
    ad.out("convexity", "convexity");
    ad.out("concavity", "concavity");
}

fn facing_normal(ad: &mut Ad) {
    ad.slot("Normal", "in");
}

fn flood_to_color(ad: &mut Ad) {
    if (0..3).any(|i| ad.take_lane("hsl_adjustement", i) != 0.0 || ad.take_lane("hsl_random", i) != 0.0) {
        ad.note("the colour adjustments per region are not reproduced");
    }
    ad.slot("input", "in");
    ad.slot("color_input", "");
}

fn flood_to_gray(ad: &mut Ad) {
    let adj = ad.take("luminance_adjustement");
    let rnd = ad.take("luminance_random");
    // a random grey per region, around the adjustment
    ad.set("min", (0.5 + adj - rnd.abs() * 0.5).clamp(0.0, 1.0));
    ad.set("max", (0.5 + adj + rnd.abs() * 0.5).clamp(0.0, 1.0));
    if rnd == 0.0 {
        ad.set("min", 0.0);
        ad.set("max", 1.0);
    }
    ad.slot("input", "in");
    ad.slot("grayscale_input", "");
}

/// Flood Fill to Gradient: one ramp per region, turned by the angle plus a random angle per region.
fn flood_to_gradient(ad: &mut Ad) {
    ad.same("angle", "angle");
    ad.same("angle_variation", "variation");
    if ad.take("multiply_by_bbox_size") != 0.0 {
        ad.note("scaling the ramps by the region size is not reproduced");
    }
    if ad.take("angle_image_input_multiplier") != 0.0 || ad.take("slope_image_input_multiplier") != 0.0 {
        ad.note("angle and slope images per region are not reproduced");
    }
    ad.ignore(&["multiply_by_slope_intensity", "flat_slope_color"]);
    ad.slot("input", "in");
    ad.slot("angle_input", "");
    ad.slot("slope_input", "");
}

fn floodfill_to_index(ad: &mut Ad) {
    ad.ignore(&["output", "ignore_shape_smaller_than", "show_floodfill_data_table"]);
    ad.slot("input", "in");
    ad.out("shape_index", "output");
}

fn hbao(ad: &mut Ad) {
    ad.set("mode", 1.0);
    ad.same("radius", "hbao_radius");
    ad.same("height_depth", "height_depth");
    ad.same("use_world_units", "use_world_units");
    ad.same("surface_size", "surface_size");
    ad.same("height_depth_cm", "height_depth_cm");
    if ad.decl("samples") {
        let n = ad.take_i("samples");
        ad.set("quality", if n <= 4 { 0.0 } else if n <= 8 { 1.0 } else { 2.0 });
    }
    ad.ignore(&["gpu_optim", "non_square"]);
    ad.slot("input", "in");
}

fn height_to_normal_world_units(ad: &mut Ad) {
    ad.same("surface_size", "size");
    ad.same("height_depth", "height");
    normal_format(ad, "normal_format");
    ad.ignore(&["sampling"]);
    ad.slot("input", "in");
}

fn histogram_scan_non_uniform(ad: &mut Ad) {
    ad.same("position", "position");
    ad.same("contrast", "contrast");
    // "custom" position / contrast: the slider; otherwise the connected map drives it per pixel
    let (cp, cc) = (ad.take_b("custom_position"), ad.take_b("custom_contrast"));
    ad.set("use_position_map", if cp { 0.0 } else { 1.0 });
    ad.set("use_contrast_map", if cc { 0.0 } else { 1.0 });
    ad.slot("input", "in");
    ad.slot("position_1", "position_map");
    ad.slot("contrast_1", "contrast_map");
}

fn histogram_select(ad: &mut Ad) {
    ad.same("position", "position");
    ad.same("range", "range");
    ad.same("constrast", "contrast");
    ad.slot("input", "in");
}

fn luminance_highpass(ad: &mut Ad) {
    ad.same("Radius", "radius");
    ad.slot("Source", "in");
}

fn normal_to_height_hq(ad: &mut Ad) {
    ad.same("relief_balance", "relief_balance");
    ad.same("height_intensity", "opacity");
    normal_format(ad, "normal_format");
    ad.ignore(&["height_normalize", "sample_quality"]);
    ad.slot("normal", "in");
}

fn premult_to_straight(ad: &mut Ad) {
    ad.set("mode", 1.0);
    ad.slot("input", "in");
}

fn straight_to_premult(ad: &mut Ad) {
    ad.set("mode", 0.0);
    ad.slot("input", "in");
}

fn replace_color_range(ad: &mut Ad) {
    ad.vec("SourceColor", &["src_r", "src_g", "src_b"]);
    ad.vec("TargetColor", &["dst_r", "dst_g", "dst_b"]);
    ad.same("SourceRange", "range");
    ad.lin("threshold", 0, "softness", 0.15, 0.0);
    ad.slot("Input", "in");
}

fn rgb_a_merge(ad: &mut Ad) {
    ad.slot("RGB", "rgb");
    ad.slot("A", "alpha");
}

fn rgb_a_split(ad: &mut Ad) {
    ad.slot("RGBA", "in");
    ad.out("RGB", "rgb");
    ad.out("A", "alpha");
}

fn rt_ao(ad: &mut Ad) {
    ad.set("mode", 1.0);
    ad.lin("max_distance", 0, "hbao_radius", 2.0, 0.0);
    ad.same("height_scale", "height_depth");
    if ad.take_b("use_physical_size") {
        ad.note("physical sizes are not used");
    }
    ad.ignore(&["physical_size", "distribution", "spread_angle"]);
    ad.slot("input", "in");
}

fn rt_bent_normal(ad: &mut Ad) {
    ad.lin("max_distance", 0, "radius", 128.0, 0.0);
    ad.same("height_scale", "relief");
    normal_format(ad, "normal_format");
    if ad.take_b("use_physical_size") {
        ad.note("physical sizes are not used");
    }
    ad.ignore(&["physical_size", "distribution", "spread_angle"]);
    ad.slot("input", "in");
    ad.out("output", "normal");
}

fn bevel_smooth(ad: &mut Ad) {
    ad.same("maximum_distance", "distance");
    ad.lin("mask_smoothness", 0, "smoothing", 0.25, 0.0);
    ad.same("mask_threshold", "threshold");
    if ad.take_i("output_mode") != 0 {
        ad.note("only the height output mode is reproduced");
    }
    ad.ignore(&["direction", "mask_offset", "distance_metric", "scale", "distance_map_multiplier"]);
    ad.slot("mask_input", "in");
    ad.slot("source_input", "");
    ad.slot("distance_map", "");
    ad.out("output", "output");
}

fn diffusion_uv(ad: &mut Ad) {
    ad.ignore(&["iterations"]);
    ad.slot("input", "in");
    ad.slot("mask", "mask");
}

fn normal_add(ad: &mut Ad) {
    ad.set("opacity", 1.0);
    ad.slot("Normal", "a");
    ad.slot("Normal_1", "b");
    ad.out("Normal", "output");
}

fn caustics_rt(ad: &mut Ad) {
    // a light pattern of our own: the surface it would come from is not traced
    ad.ignore(&["output_color_space", "photon_grid_size", "surface_height_scale", "surface_height_position", "surface_ior", "photon_size", "jittering", "light_position", "non_square_expansion"]);
    ad.slot("input", "");
    ad.note("the caustics are MaterialCraft's own pattern, not traced through the input surface");
}

fn pavement(ad: &mut Ad) {
    ad.set("preset", 12.0);
    ad.same("Roudness", "roundness");
    ad.out("Pavement", "output");
}


pub(crate) const ADAPTERS: &[Adapter] = &[
    Adapter { ids: &["blend/add"], kind: "blend", f: bl_add },
    Adapter { ids: &["blend/add_sub"], kind: "blend", f: bl_add_sub },
    Adapter { ids: &["blend/blend"], kind: "blend", f: bl_blend },
    Adapter { ids: &["blend/color"], kind: "blend", f: bl_color },
    Adapter { ids: &["blend/color_burn"], kind: "blend", f: bl_color_burn },
    Adapter { ids: &["blend/color_dodge"], kind: "blend", f: bl_color_dodge },
    Adapter { ids: &["blend/copy"], kind: "blend", f: bl_copy },
    Adapter { ids: &["blend/difference"], kind: "blend", f: bl_difference },
    Adapter { ids: &["blend/dissolve", "blend/dissolve_2"], kind: "blend", f: bl_dissolve },
    Adapter { ids: &["blend/exclusion"], kind: "blend", f: bl_exclusion },
    Adapter { ids: &["blend/hard_light"], kind: "blend", f: bl_hard_light },
    Adapter { ids: &["blend/linear_burn"], kind: "blend", f: bl_linear_burn },
    Adapter { ids: &["blend/luminosity"], kind: "blend", f: bl_luminosity },
    Adapter { ids: &["blend/max"], kind: "blend", f: bl_max },
    Adapter { ids: &["blend/min"], kind: "blend", f: bl_min },
    Adapter { ids: &["blend/multiply"], kind: "blend", f: bl_multiply },
    Adapter { ids: &["blend/overlay"], kind: "blend", f: bl_overlay },
    Adapter { ids: &["blend/screen"], kind: "blend", f: bl_screen },
    Adapter { ids: &["blend/soft_light"], kind: "blend", f: bl_soft_light },
    Adapter { ids: &["blend/subtract"], kind: "blend", f: bl_subtract },
    Adapter {
        ids: &["color_space_conversion/srgb_to_linear_srgb", "color_space_conversion/srgb_to_linear_srgb_grayscale", "linear_srgb_conversion/convert_to_linear", "linear_srgb_conversion/convert_to_linear_grayscale"],
        kind: "srgb_to_linear",
        f: one_input,
    },
    Adapter {
        ids: &["color_space_conversion/linear_srgb_to_srgb", "color_space_conversion/linear_srgb_to_srgb_grayscale", "linear_srgb_conversion/convert_to_srgb", "linear_srgb_conversion/convert_to_srgb_grayscale"],
        kind: "linear_to_srgb",
        f: one_input,
    },
    Adapter { ids: &["curvature_sobel/curvature_sobel"], kind: "normal_sobel", f: curvature_sobel },
    Adapter { ids: &["blur_hq/blur_hq", "blur_hq/blur_hq_grayscale"], kind: "blur_hq", f: blur_hq },
    Adapter { ids: &["curvature_smooth_v2/curvature_smooth_v2"], kind: "curvature_smooth", f: curvature_smooth_v2 },
    Adapter { ids: &["facing_normal/facing_normal"], kind: "normal_facing", f: facing_normal },
    Adapter { ids: &["flood_fill_2/flood_fill_to_color"], kind: "flood_fill_color", f: flood_to_color },
    Adapter { ids: &["flood_fill_2/flood_fill_to_grayscale"], kind: "flood_fill_random", f: flood_to_gray },
    Adapter { ids: &["floodfill_to_index/floodfill_to_index"], kind: "flood_fill_to_index", f: floodfill_to_index },
    Adapter { ids: &["flood_fill_2/flood_fill_to_gradient", "flood_fill/flood_fill_to_gradient", "flood_fill_2/flood_fill_to_gradient_2"], kind: "flood_fill_gradient", f: flood_to_gradient },
    Adapter { ids: &["hbao/hbao", "hbao_2/hbao"], kind: "ao", f: hbao },
    Adapter { ids: &["height_to_normal_world_units/height_to_normal_world_units", "height_to_normal_world_units/height_to_normal_world_units_2"], kind: "height_to_normal_world", f: height_to_normal_world_units },
    Adapter { ids: &["histogram_scan_non_uniform/histogram_scan_non_uniform"], kind: "histogram_scan", f: histogram_scan_non_uniform },
    Adapter { ids: &["histogram_select/histogram_select"], kind: "histogram_range", f: histogram_select },
    Adapter { ids: &["luminance_highpass/luminance_highpass"], kind: "highpass", f: luminance_highpass },
    Adapter { ids: &["normal_to_height_hq/normal_to_height_hq"], kind: "normal_to_height", f: normal_to_height_hq },
    Adapter { ids: &["pre_multiply_straight/premult_to_straight"], kind: "premultiply", f: premult_to_straight },
    Adapter { ids: &["pre_multiply_straight/straight_to_premult"], kind: "premultiply", f: straight_to_premult },
    Adapter { ids: &["replace_color/replace_color_range"], kind: "replace_color", f: replace_color_range },
    Adapter { ids: &["rgb-a_merge/rgb-a_merge"], kind: "alpha_merge", f: rgb_a_merge },
    Adapter { ids: &["rgb-a_split/rgb-a_split"], kind: "alpha_split", f: rgb_a_split },
    Adapter { ids: &["rt_ao/rt_ao", "rt_ao_v2/rt_ao"], kind: "ao", f: rt_ao },
    Adapter { ids: &["rt_bent_normal/rt_bent_normal", "rt_bent_normal_v2/rt_bent_normal"], kind: "bent_normal", f: rt_bent_normal },
    Adapter { ids: &["bevel_smooth/bevel_smooth"], kind: "bevel", f: bevel_smooth },
    Adapter { ids: &["bevel/bevel"], kind: "bevel", f: bevel },
    Adapter { ids: &["slope_blur/slope_blur", "slope_blur/slope_blur_grayscale", "slope_blur/slope_blur_grayscale_2"], kind: "slope_blur", f: slope_blur },
    Adapter { ids: &["diffusion/diffusion_uv"], kind: "diffusion", f: diffusion_uv },
    Adapter { ids: &["normal_add/normal_add"], kind: "normal_blend", f: normal_add },
    Adapter { ids: &["rt_caustics/rt_caustics_grayscale", "rt_caustics/rt_caustics_color"], kind: "caustics", f: caustics_rt },
    Adapter { ids: &["pavement_1/pavement_1"], kind: "tile_preset", f: pavement },
    Adapter { ids: &["pattern_scratches_generator/scratches_generator", "scratches_generator/scratches_generator"], kind: "scratches", f: scratches_generator },
    Adapter { ids: &["messy_fibers_2/messy_fibers_2", "noise_messy_fibers_2/messy_fibers_2"], kind: "scratches", f: messy_lines },
    Adapter { ids: &["messy_fibers_3/messy_fibers_3", "noise_messy_fibers_3/messy_fibers_3"], kind: "scratches", f: messy_spikes },
    Adapter { ids: &["noise_voronoi/voronoi_fractal"], kind: "voronoi_fractal", f: voronoi_fractal },
    Adapter { ids: &["3d_perlin_noise/3D_perlin_noise"], kind: "noise_3d", f: n3_perlin },
    Adapter { ids: &["3d_perlin_noise/3D_perlin_noise_fractal"], kind: "noise_3d", f: n3_perlin_fractal },
    Adapter { ids: &["3d_perlin_noise/3D_ridged_noise_fractal"], kind: "noise_3d", f: n3_ridged },
    Adapter { ids: &["3d_simplex_noise/3d_simplex_noise"], kind: "noise_3d", f: n3_simplex },
    Adapter { ids: &["3d_worley_noise/3D_worley_noise"], kind: "noise_3d", f: n3_worley },
    Adapter { ids: &["noise_voronoi/3d_voronoi"], kind: "noise_3d", f: n3_voronoi },
    Adapter { ids: &["noise_voronoi/3d_voronoi_fractal"], kind: "noise_3d", f: n3_voronoi_fractal },
    Adapter { ids: &["tile_random/tile_random", "tile_random/tile_random_color", "pattern_tile_random/tile_random", "pattern_tile_random/tile_random_color"], kind: "tile_preset", f: tile_random },
    Adapter { ids: &["pattern_tile_random_2/tile_random_2"], kind: "tile_random2", f: tile_random_2 },
    Adapter { ids: &["creased/creased"], kind: "preset_noise", f: p_old_creased },
    Adapter { ids: &["noise_creased/creased"], kind: "preset_noise", f: p_creased },
    Adapter { ids: &["fluid/fluid"], kind: "preset_noise", f: p_old_fluid },
    Adapter { ids: &["noise_fluid/fluid"], kind: "preset_noise", f: p_fluid },
    Adapter { ids: &["herb_1/herb_1"], kind: "preset_noise", f: p_herb },
    Adapter { ids: &["horizontal_lines/horizontal_lines"], kind: "preset_noise", f: p_horizontal_lines },
    Adapter { ids: &["liquid/liquid"], kind: "preset_noise", f: p_old_liquid },
    Adapter { ids: &["noise_liquid/liquid"], kind: "preset_noise", f: p_liquid },
    Adapter { ids: &["messy_fibers_1/messy_fibers_1"], kind: "preset_noise", f: p_old_messy_1 },
    Adapter { ids: &["noise_messy_fibers_1/messy_fibers_1"], kind: "preset_noise", f: p_messy_1 },
    Adapter { ids: &["messy_fibers_4/messy_fibers_4"], kind: "preset_noise", f: p_sponge_stains },
    Adapter { ids: &["paper_path/paper_path"], kind: "preset_noise", f: p_paper_path },
    Adapter { ids: &["white_noise/white_noise", "white_noise/white_noise_fast"], kind: "white_noise", f: white_noise },
    Adapter { ids: &["noise_voronoi/voronoi"], kind: "voronoi", f: voronoi },
    Adapter { ids: &["pattern_triangle_grid/triangle_grid_base", "pattern_triangle_grid/triangle_grid_grayscale", "pattern_triangle_grid/triangle_grid_color"], kind: "triangle_grid", f: triangle_grid },
    Adapter { ids: &["brick_1/brick_1", "pattern_brick_1/brick_1"], kind: "tile_preset", f: brick_1 },
    Adapter { ids: &["brick_2/brick_2", "pattern_brick_2/brick_2"], kind: "tile_preset", f: brick_2 },
    Adapter { ids: &["brick_3/brick_3"], kind: "tile_preset", f: brick_3 },
    Adapter { ids: &["brick_generator/brick_generator", "pattern_brick_generator/brick_generator"], kind: "tile_preset", f: brick_generator },
    Adapter { ids: &["alveolus/alveolus", "pattern_alveolus/alveolus"], kind: "tile_preset", f: alveolus },
    Adapter { ids: &["gaussian_1/gaussian_1", "pattern_gaussian_1/gaussian_1"], kind: "tile_preset", f: gaussian_1 },
    Adapter { ids: &["gaussian_2/gaussian_2", "pattern_gaussian_2/gaussian_2"], kind: "tile_preset", f: gaussian_2 },
    Adapter { ids: &["mesh_1/mesh_1", "pattern_mesh_1/mesh_1"], kind: "tile_preset", f: mesh_1 },
    Adapter { ids: &["mesh_2/mesh_2", "pattern_mesh_2/mesh_2"], kind: "tile_preset", f: mesh_2 },
    Adapter { ids: &["parquet_1/parquet_1"], kind: "tile_preset", f: parquet },
    Adapter { ids: &["noise_fractal_sum_base/fractal_sum_base_2"], kind: "preset_noise", f: p_fsum_base_2 },
    Adapter { ids: &["pattern_star/star"], kind: "star", f: star },
    Adapter { ids: &["microscope_view/microscope_view", "noise_microscope_view/microscope_view"], kind: "microscope_noise", f: microscope },
    Adapter { ids: &["clamp/clamp", "clamp/clamp_grayscale"], kind: "clamp", f: clamp },
    Adapter { ids: &["color_to_mask/color_to_mask"], kind: "color_to_mask", f: color_to_mask },
    Adapter { ids: &["curvature/curvature"], kind: "curvature", f: curvature },
    Adapter { ids: &["glow/glow", "glow/glow_grayscale"], kind: "glow", f: glow },
    Adapter { ids: &["histogram_scan/histogram_scan"], kind: "histogram_scan", f: histogram_scan },
    Adapter { ids: &["mirror/mirror", "mirror/mirror_grayscale"], kind: "mirror", f: mirror },
    Adapter { ids: &["mosaic/mosaic", "mosaic/mosaic_grayscale"], kind: "mosaic", f: mosaic },
    Adapter { ids: &["multi_directional_warp/multi_directional_warp_color", "multi_directional_warp/multi_directional_warp_grayscale"], kind: "multi_directional_warp", f: multi_directional_warp },
    Adapter { ids: &["blend_switch/multi_switch", "blend_switch/multi_switch_grayscale"], kind: "multi_switch", f: multi_switch },
    Adapter { ids: &["blend_switch/switch", "blend_switch/switch_grayscale"], kind: "switch", f: switch },
    Adapter { ids: &["non_uniform_directional_warp/non_uniform_directional_warp_color", "non_uniform_directional_warp/non_uniform_directional_warp_grayscale"], kind: "non_uniform_directional_warp", f: non_uniform_directional_warp },
    Adapter { ids: &["normal_blend/normal_blend"], kind: "normal_blend", f: normal_blend },
    Adapter { ids: &["normal_invert/normal_invert"], kind: "normal_invert", f: normal_invert },
    Adapter { ids: &["normal_to_height/normal_to_height", "normal_to_height_2/normal_to_height"], kind: "normal_to_height", f: normal_to_height },
    Adapter { ids: &["normal_vector_rotation/normal_vector_rotation"], kind: "normal_vector_rotation", f: normal_vector_rotation },
    Adapter { ids: &["quad_transform/quad_transform", "quad_transform/quad_transform_grayscale"], kind: "quad_transform", f: quad_transform },
    Adapter { ids: &["quantize/quantize", "quantize/quantize_grayscale", "quantize_2/quantize_grayscale"], kind: "quantize", f: quantize },
    Adapter { ids: &["radial_blur/radial_blur", "radial_blur/radial_blur_grayscale"], kind: "radial_blur", f: radial_blur },
    Adapter { ids: &["replace_color/replace_color"], kind: "replace_color", f: replace_color },
    Adapter { ids: &["trapezoid_transform/trapezoid_transform", "trapezoid_transform/trapezoid_transform_grayscale"], kind: "trapezoid_transform", f: trapezoid },
    Adapter { ids: &["vector_warp/vector_warp", "vector_warp/vector_warp_grayscale"], kind: "vector_warp", f: vector_warp },
    Adapter { ids: &["height_blend/height_blend"], kind: "height_blend", f: height_blend },
    Adapter { ids: &["emboss_with_gloss/emboss_with_gloss"], kind: "emboss", f: emboss_with_gloss },
    Adapter { ids: &["non_uniform_blur/non_uniform_blur", "non_uniform_blur/non_uniform_blur_grayscale"], kind: "non_uniform_blur", f: non_uniform_blur },
    Adapter { ids: &["safe_transform/safe_transform", "safe_transform/safe_transform_grayscale"], kind: "safe_transform", f: safe_transform },
    Adapter { ids: &["normal_combine/normal_combine"], kind: "normal_combine", f: normal_combine },
    Adapter { ids: &["waveform_1/waveform_1"], kind: "waveform", f: waveform_1 },
    Adapter { ids: &["scratches_1/scratches_1"], kind: "scratches", f: scratches_1 },
    Adapter { ids: &["scratches_2/scratches_2", "me_water_level/scratches_2"], kind: "scratches", f: thorns_1 },
    Adapter { ids: &["scratches_3/scratches_3"], kind: "scratches", f: thorns_2 },
    Adapter { ids: &["scratches_4/scratches_4"], kind: "scratches", f: thorns_3 },
    Adapter { ids: &["fur_1/fur_1", "noise_fur_1/fur_1"], kind: "fur_noise", f: fur_1 },
    Adapter { ids: &["fur_2/fur_2", "noise_fur_2/fur_2"], kind: "fur_noise", f: fur_2 },
    Adapter { ids: &["fur_3/fur_3", "noise_fur_3/fur_3"], kind: "fur_noise", f: fur_3 },
    Adapter { ids: &["fur_4/fur_4"], kind: "fur_noise", f: fur_4 },
    Adapter { ids: &["weave_generator/weave_generator", "pattern_weave_generator/weave_generator"], kind: "weave_generator", f: weave_generator },
    Adapter { ids: &["anisotropic_noise/anisotropic_noise", "noise_anisotropic_noise/anisotropic_noise"], kind: "anisotropic_noise", f: anisotropic },
    Adapter { ids: &["perlin_noise_zoom/perlin_noise_zoom"], kind: "preset_noise", f: p_noise_zoom },
    Adapter { ids: &["dirt_6/dirt_6"], kind: "preset_noise", f: p_white_stains },
    Adapter { ids: &["fibers_1/fibers_1", "pattern_fibers_1/fibers_1"], kind: "weave_preset", f: rope },
    Adapter { ids: &["fibers_2/fibers_2"], kind: "weave_preset", f: fibers_old },
    Adapter { ids: &["pattern_fibers_2/fibers_2"], kind: "weave_preset", f: fibers_new },
    Adapter { ids: &["weave_1/weave_1", "pattern_weave_1/weave_1"], kind: "weave_preset", f: weave_1 },
    Adapter { ids: &["weave_2/weave_2"], kind: "weave_preset", f: weave_2_old },
    Adapter { ids: &["pattern_weave_2/weave_2"], kind: "weave_preset", f: weave_2_new },
    Adapter { ids: &["weave_3/weave_3"], kind: "weave_preset", f: weave_3 },
    Adapter { ids: &["weave_4/weave_4"], kind: "weave_preset", f: weave_4 },
    Adapter { ids: &["checker_1/checker_1", "pattern_checker_1/checker_1"], kind: "checker", f: checker_1 },
    Adapter { ids: &["checker_2/checker_2"], kind: "checker", f: checker_2 },
    Adapter { ids: &["stripes/stripes", "pattern_stripes/stripes"], kind: "stripes", f: stripes },
    Adapter { ids: &["gradient_linear_1/gradient_linear_1"], kind: "gradient_2pt", f: linear_1 },
    Adapter { ids: &["gradient_linear_2/gradient_linear_2"], kind: "gradient_2pt", f: linear_2 },
    Adapter { ids: &["gradient_linear_3/gradient_linear_3"], kind: "gradient_2pt", f: linear_3 },
    Adapter { ids: &["gradient_axial/gradient_axial"], kind: "gradient_2pt", f: axial },
    Adapter { ids: &["gradient_axial/gradient_axial_reflected"], kind: "gradient_2pt", f: axial_reflected },
    Adapter { ids: &["gradient_circular/gradient_circular"], kind: "gradient_2pt", f: circular },
    Adapter { ids: &["gradient_radial/gradient_radial"], kind: "gradient_2pt", f: radial },
    Adapter { ids: &["polygon_1/polygon_1", "pattern_polygon_1/polygon_1"], kind: "polygon", f: polygon_1 },
    Adapter { ids: &["polygon_2/polygon_2", "pattern_polygon_2/polygon_2"], kind: "polygon", f: polygon_2 },
    Adapter { ids: &["shape/shape"], kind: "shape", f: shape_old },
    Adapter { ids: &["pattern_shape/shape", "pattern_shape/shape_base", "shape/shape_base"], kind: "shape", f: shape_new },
    Adapter { ids: &["starburst/starburst"], kind: "starburst", f: starburst },
    Adapter { ids: &["pattern_tile_generator/tile_generator", "pattern_tile_generator/tile_generator_color"], kind: "tile_generator", f: tile_generator_new },
    Adapter { ids: &["tile_generator/tile_generator", "tile_generator_color/tile_generator_color"], kind: "tile_generator", f: tile_generator_old },
    Adapter {
        ids: &["pattern_tile_sampler/tile_sampler", "pattern_tile_sampler/tile_sampler_color", "tile_sampler/tile_sampler", "tile_sampler/tile_sampler_color"],
        kind: "tile_sampler",
        f: tile_sampler,
    },
    Adapter { ids: &["pattern_splatter_circular/splatter_circular", "pattern_splatter_circular/splatter_circular_color"], kind: "splatter_circular", f: splatter_circular_new },
    Adapter { ids: &["splatter_circular/splatter_circular", "splatter_circular/splatter_circular_color"], kind: "splatter_circular", f: splatter_circular_old },
    Adapter { ids: &["noise_cells_1/cells_1"], kind: "noise_pattern", f: n_cells_1 },
    Adapter { ids: &["noise_cells_2/cells_2"], kind: "noise_pattern", f: n_cells_2 },
    Adapter { ids: &["noise_cells_3/cells_3"], kind: "noise_pattern", f: n_cells_3 },
    Adapter { ids: &["noise_cells_4/cells_4"], kind: "noise_pattern", f: n_cells_4 },
    Adapter { ids: &["noise_crystal_1/crystal_1"], kind: "noise_pattern", f: n_crystal_1 },
    Adapter { ids: &["noise_gaussian_noise/gaussian_noise"], kind: "noise_pattern", f: n_gaussian },
    Adapter { ids: &["noise_perlin_noise/perlin_noise"], kind: "noise_pattern", f: n_perlin },
    Adapter { ids: &["noise_plasma/plasma"], kind: "noise_pattern", f: n_plasma },
    Adapter { ids: &["cells_1/cells_1"], kind: "noise_pattern", f: n_old_cells_1 },
    Adapter { ids: &["cells_2/cells_2"], kind: "noise_pattern", f: n_old_cells_2 },
    Adapter { ids: &["cells_3/cells_3"], kind: "noise_pattern", f: n_old_cells_3 },
    Adapter { ids: &["cells_4/cells_4"], kind: "noise_pattern", f: n_old_cells_4 },
    Adapter { ids: &["crystal_1/crystal_1"], kind: "noise_pattern", f: n_old_crystal_1 },
    Adapter { ids: &["plasma/plasma"], kind: "noise_pattern", f: n_old_plasma },
    Adapter { ids: &["noise_clouds_1/clouds_1"], kind: "preset_noise", f: p_clouds_1 },
    Adapter { ids: &["noise_clouds_2/clouds_2"], kind: "preset_noise", f: p_clouds_2 },
    Adapter { ids: &["noise_clouds_3/clouds_3"], kind: "preset_noise", f: p_clouds_3 },
    Adapter { ids: &["noise_bnw_spots_1/bnw_spots_1"], kind: "preset_noise", f: p_bnw_1 },
    Adapter { ids: &["noise_bnw_spots_2/bnw_spots_2"], kind: "preset_noise", f: p_bnw_2 },
    Adapter { ids: &["noise_bnw_spots_3/bnw_spots_3"], kind: "preset_noise", f: p_bnw_3 },
    Adapter { ids: &["noise_fractal_sum_base/fractal_sum_base"], kind: "preset_noise", f: p_fsum_base },
    Adapter { ids: &["noise_fractal_sum_1/fractal_sum_1"], kind: "preset_noise", f: p_fsum_1 },
    Adapter { ids: &["noise_fractal_sum_2/fractal_sum_2"], kind: "preset_noise", f: p_fsum_2 },
    Adapter { ids: &["noise_fractal_sum_3/fractal_sum_3"], kind: "preset_noise", f: p_fsum_3 },
    Adapter { ids: &["noise_fractal_sum_4/fractal_sum_4"], kind: "preset_noise", f: p_fsum_4 },
    Adapter { ids: &["noise_moisture_noise/moisture_noise", "moisture_noise/moisture_noise"], kind: "preset_noise", f: p_moisture },
    Adapter { ids: &["noise_dirt_1/dirt_1", "dirt_1/dirt_1"], kind: "preset_noise", f: p_dirt_1 },
    Adapter { ids: &["noise_dirt_2/dirt_2", "dirt_2/dirt_2"], kind: "preset_noise", f: p_dirt_2 },
    Adapter { ids: &["noise_dirt_3/dirt_3", "dirt_3/dirt_3"], kind: "preset_noise", f: p_dirt_3 },
    Adapter { ids: &["noise_dirt_4/dirt_4"], kind: "preset_noise", f: p_dirt_4 },
    Adapter { ids: &["noise_dirt_5/dirt_5", "dirt_5/dirt_5"], kind: "preset_noise", f: p_dirt_5 },
    Adapter { ids: &["clouds_1/clouds_1"], kind: "preset_noise", f: p_old_clouds_1 },
    Adapter { ids: &["clouds_2/clouds_2"], kind: "preset_noise", f: p_old_clouds_2 },
    Adapter { ids: &["clouds_3/clouds_3"], kind: "preset_noise", f: p_old_clouds_3 },
    Adapter { ids: &["bnw_spots_1/bnw_spots_1"], kind: "preset_noise", f: p_old_bnw_1 },
    Adapter { ids: &["bnw_spots_2/bnw_spots_2"], kind: "preset_noise", f: p_old_bnw_2 },
    Adapter { ids: &["bnw_spots_3/bnw_spots_3"], kind: "preset_noise", f: p_old_bnw_3 },
    Adapter { ids: &["fractal_sum_base/fractal_sum_base"], kind: "preset_noise", f: p_old_fsum_base },
    Adapter { ids: &["fractal_sum_1/fractal_sum_1"], kind: "preset_noise", f: p_old_fsum_1 },
    Adapter { ids: &["fractal_sum_2/fractal_sum_2"], kind: "preset_noise", f: p_old_fsum_2 },
    Adapter { ids: &["fractal_sum_3/fractal_sum_3"], kind: "preset_noise", f: p_old_fsum_3 },
    Adapter { ids: &["fractal_sum_4/fractal_sum_4"], kind: "preset_noise", f: p_old_fsum_4 },
    Adapter { ids: &["perlin_noise_1/perlin_noise_1"], kind: "preset_noise", f: p_old_noise_1 },
    Adapter { ids: &["perlin_noise_2/perlin_noise_2"], kind: "preset_noise", f: p_old_noise_2 },
    Adapter { ids: &["noise_grunge_map_001/grunge_map_001", "grunge_map_001/grunge_map_001"], kind: "preset_noise", f: g_map_001 },
    Adapter { ids: &["noise_grunge_map_002/grunge_map_002", "grunge_map_002/grunge_map_002"], kind: "preset_noise", f: g_map_002 },
    Adapter { ids: &["noise_grunge_map_003/grunge_map_003", "grunge_map_003/grunge_map_003"], kind: "preset_noise", f: g_map_003 },
    Adapter { ids: &["noise_grunge_map_004/grunge_map_004", "grunge_map_004/grunge_map_004"], kind: "preset_noise", f: g_map_004 },
    Adapter { ids: &["noise_grunge_map_005/grunge_map_005", "grunge_map_005/grunge_map_005"], kind: "preset_noise", f: g_map_005 },
    Adapter { ids: &["noise_grunge_map_006/grunge_map_006", "grunge_map_006/grunge_map_006"], kind: "preset_noise", f: g_map_006 },
    Adapter { ids: &["noise_grunge_map_007/grunge_map_007", "grunge_map_007/grunge_map_007"], kind: "preset_noise", f: g_map_007 },
    Adapter { ids: &["noise_grunge_map_008/grunge_map_008", "grunge_map_008/grunge_map_008"], kind: "preset_noise", f: g_map_008 },
    Adapter { ids: &["noise_grunge_map_009/grunge_map_009", "grunge_map_009/grunge_map_009"], kind: "preset_noise", f: g_map_009 },
    Adapter { ids: &["noise_grunge_map_010/grunge_map_010", "grunge_map_010/grunge_map_010"], kind: "preset_noise", f: g_map_010 },
    Adapter { ids: &["noise_grunge_map_011/grunge_map_011", "grunge_map_011/grunge_map_011"], kind: "preset_noise", f: g_map_011 },
    Adapter { ids: &["noise_grunge_map_012/grunge_map_012", "grunge_map_012/grunge_map_012"], kind: "preset_noise", f: g_map_012 },
    Adapter { ids: &["noise_grunge_map_013/grunge_map_013", "grunge_map_013/grunge_map_013"], kind: "preset_noise", f: g_map_013 },
    Adapter { ids: &["noise_grunge_map_014/grunge_map_014", "grunge_map_014/grunge_map_014"], kind: "preset_noise", f: g_map_014 },
    Adapter { ids: &["noise_grunge_map_015/grunge_map_015", "grunge_map_015/grunge_map_015"], kind: "preset_noise", f: g_map_015 },
    Adapter { ids: &["grunge_concrete/grunge_concrete"], kind: "preset_noise", f: g_concrete },
    Adapter { ids: &["grunge_damas/grunge_damas"], kind: "preset_noise", f: g_damas },
    Adapter { ids: &["grunge_galvanic_large/grunge_galvanic_large"], kind: "preset_noise", f: g_galvanic_large },
    Adapter { ids: &["grunge_galvanic_small/grunge_galvanic_small"], kind: "preset_noise", f: g_galvanic_small },
    Adapter { ids: &["grunge_leaks/grunge_leaks"], kind: "preset_noise", f: g_leaks },
    Adapter { ids: &["grunge_leaky_paint/grunge_leaky_paint"], kind: "preset_noise", f: g_leaky_paint },
    Adapter { ids: &["grunge_rough_dirty/grunge_rough_dirty"], kind: "preset_noise", f: g_rough_dirty },
    Adapter { ids: &["grunge_rust_fine/grunge_rust_fine"], kind: "preset_noise", f: g_rust_fine },
    Adapter { ids: &["grunge_scratches_dirty/grunge_scratches_dirty"], kind: "preset_noise", f: g_scratches_dirty },
    Adapter { ids: &["grunge_scratches_fine/grunge_scratches_fine"], kind: "preset_noise", f: g_scratches_fine },
    Adapter { ids: &["grunge_scratches_rough/grunge_scratches_rough"], kind: "preset_noise", f: g_scratches_rough },
    Adapter { ids: &["grunge_shavings/grunge_shavings"], kind: "preset_noise", f: g_shavings },
    Adapter { ids: &["grunge_splashes_dusty/grunge_splashes_dusty"], kind: "preset_noise", f: g_splashes_dusty },
    Adapter { ids: &["grunge_spots/grunge_spots"], kind: "preset_noise", f: g_spots },
    Adapter { ids: &["grunge_spots_dirty/grunge_spots_dirty"], kind: "preset_noise", f: g_spots_dirty },
    Adapter { ids: &["noise_directional_noise_1/directional_noise_1"], kind: "preset_noise", f: d_noise_1 },
    Adapter { ids: &["noise_directional_noise_2/directional_noise_2"], kind: "preset_noise", f: d_noise_2 },
    Adapter { ids: &["noise_directional_noise_3/directional_noise_3"], kind: "preset_noise", f: d_noise_3 },
    Adapter { ids: &["noise_directional_noise_4/directional_noise_4"], kind: "preset_noise", f: d_noise_4 },
    Adapter { ids: &["directionnal_noise/directionnal_noise"], kind: "preset_noise", f: d_old_noise },
    Adapter { ids: &["vertical_noise/vertical_noise"], kind: "preset_noise", f: d_vertical },
    Adapter { ids: &["noise_directional_scratches/directional_scratches"], kind: "preset_noise", f: d_scratches },
    Adapter { ids: &["wood_fibers_1/wood_fibers_1"], kind: "preset_noise", f: d_wood_1 },
    Adapter { ids: &["wood_fibers_2/wood_fibers_2"], kind: "preset_noise", f: d_wood_2 },
    Adapter { ids: &["noise_crystal_2/crystal_2"], kind: "preset_noise", f: d_crystal_2 },
    Adapter { ids: &["crystal_2/crystal_2"], kind: "preset_noise", f: d_old_crystal_2 },
    Adapter { ids: &["dirt_4/dirt_4"], kind: "preset_noise", f: d_old_horizontal_scratch },
    Adapter { ids: &["noise_gaussian_spots_1/gaussian_spots_1"], kind: "preset_noise", f: p_gauss_spots_1 },
    Adapter { ids: &["noise_gaussian_spots_2/gaussian_spots_2"], kind: "preset_noise", f: p_gauss_spots_2 },
    Adapter { ids: &["gaussian_spots_1/gaussian_spots_1"], kind: "preset_noise", f: p_old_few_spots },
    Adapter { ids: &["gaussian_spots_2/gaussian_spots_2"], kind: "preset_noise", f: p_old_spots },
    Adapter { ids: &["noise_dirt_gradient/dirt_gradient"], kind: "preset_noise", f: p_dirt_gradient },
    Adapter { ids: &["dirt_gradient/dirt_gradient"], kind: "preset_noise", f: p_old_dirt_gradient },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_targets_exist() {
        for a in ADAPTERS {
            assert!(crate::graph::spec(a.kind).is_some(), "{:?} -> {}", a.ids, a.kind);
            for id in a.ids {
                let (p, g) = id.split_once('/').map(|(p, g)| (Some(p), g)).unwrap_or((None, id));
                assert!(crate::sbs_lib::find(p, g).is_some(), "adapter for unknown library graph {id}");
            }
        }
    }
}

#[cfg(test)]
mod audit {
    use super::*;

    /// Lists, per library graph, the value inputs no adapter reads (run with `--nocapture`).
    #[test]
    fn unmapped_parameters() {
        let mut total = 0;
        let mut graphs = 0;
        for g in crate::sbs_lib::all() {
            let (kind, f): (&'static str, fn(&mut Ad)) = match adapter(g.package, g.id) {
                Some(a) => (a.kind, a.f),
                None => match crate::sbs_import::library_kind(g.id) {
                    Some(k) => (k, generic),
                    None => continue,
                },
            };
            let mut ad = Ad::new(kind, g.id, Some(g), &[]);
            f(&mut ad);
            let miss: Vec<&str> = g
                .values()
                .map(|i| i.id)
                .filter(|n| !n.starts_with('$') && !BASE_PARAMS.contains(n) && !ad.used.iter().any(|u| u.eq_ignore_ascii_case(n)))
                .collect();
            if !miss.is_empty() {
                graphs += 1;
                total += miss.len();
                println!("AUDIT {}/{} -> {kind}: {}", g.package, g.id, miss.join(", "));
            }
        }
        println!("AUDIT TOTAL {total} unmapped parameters in {graphs} graphs");
    }
}
