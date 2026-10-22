// SPDX-License-Identifier: MIT OR Apache-2.0
//! The editable parameters of every node type: label, default, range, choices and panel group. The editor draws its
//! Properties panel from these tables and the command interface (`session`) describes them to scripts and AI tools,
//! so both always agree.
#[derive(Clone, Copy, Debug)]
pub enum PK {
    F { def: f32, lo: f32, hi: f32 },
    I { def: i32, lo: i32, hi: i32 },
    E { def: i32, opts: &'static [&'static str] },
    B { def: bool },
}

#[derive(Clone, Copy, Debug)]
pub struct PD {
    pub name: &'static str,
    pub label: &'static str,
    pub k: PK,
}

pub fn pf(name: &'static str, label: &'static str, def: f32, lo: f32, hi: f32) -> PD {
    PD { name, label, k: PK::F { def, lo, hi } }
}
pub fn pi(name: &'static str, label: &'static str, def: i32, lo: i32, hi: i32) -> PD {
    PD { name, label, k: PK::I { def, lo, hi } }
}
pub fn pe(name: &'static str, label: &'static str, def: i32, opts: &'static [&'static str]) -> PD {
    PD { name, label, k: PK::E { def, opts } }
}
pub fn pb(name: &'static str, label: &'static str, def: bool) -> PD {
    PD { name, label, k: PK::B { def } }
}

pub const BLEND_MODES: &[&str] = &["Copy", "Add", "Subtract", "Multiply", "Screen", "Overlay", "Max", "Min", "Difference", "Soft light", "Color dodge", "Color burn", "Linear burn", "Hard light", "Exclusion", "Divide", "Linear light", "Color", "Luminosity", "Dissolve"];
pub const SHAPES: &[&str] = &["Square", "Disc", "Paraboloid", "Gaussian", "Diamond", "Pyramid", "Half sphere", "Cone"];

pub const CH_SRC: &[&str] = &["Input red", "Input green", "Input blue", "Input alpha", "Second red", "Second green", "Second blue", "Second alpha", "Zero", "One"];
pub const ROT90: &[&str] = &["0", "90", "180", "270"];
pub const SYMM: &[&str] = &["None", "Horizontal", "Vertical", "Both"];
pub const TILE_BLEND: &[&str] = &["Max", "Add", "Alpha blend", "Add / Sub"];
pub const SIZE_MODES: &[&str] = &["Normal (fill cell)", "Keep ratio", "Absolute"];
pub const OFFSET_MODES: &[&str] = &["None", "Odd rows", "Even rows", "Odd columns", "Even columns"];
pub const CHECKER: &[&str] = &["Off", "Checker A", "Checker B"];
pub const DISTRIB: &[&str] = &["Random", "By pattern number", "By ring number (circular)"];

/// Pattern selection shared by every scatter node.
fn pattern_block() -> Vec<PD> {
    vec![
        pe("pattern_type", "Pattern", 0, crate::scatter::PATTERN_NAMES),
        pf("pattern_specific", "Pattern specific", 0.5, 0.0, 1.0),
        pe("pattern_distribution", "Pattern input distribution", 0, DISTRIB),
    ]
}

/// Properties shared by Tile Sampler and Tile Generator (everything except the X / Y amounts).
fn tile_common(gen: bool) -> Vec<PD> {
    let mut v = vec![
        pf("size", "Size", if gen { 0.92 } else { 0.8 }, 0.05, 3.0),
        pf("aspect", "Size Y / X", 1.0, 0.1, 4.0),
        pe("size_mode", "Size mode", 0, SIZE_MODES),
        pf("scale", "Scale", 1.0, 0.0, 10.0),
        pf("scale_random", "Scale random", 0.0, 0.0, 1.0),
        pi("scale_seed", "Scale random seed", 0, 0, 9999),
        pf("size_random", "Size random", 0.0, 0.0, 1.0),
        pf("rotation", "Rotation", 0.0, 0.0, 1.0),
        pf("rotation_random", "Rotation random", 0.0, 0.0, 1.0),
        pe("pattern_rotation", "Pattern rotation", 0, ROT90),
        pf("pattern_rotation_random", "Random quarter turns of the pattern", 0.0, 0.0, 1.0),
        pe("symmetry_random", "Symmetry random", 0, SYMM),
        pf("offset", if gen { "Row offset" } else { "Offset" }, 0.5, 0.0, 1.0),
        pe("offset_mode", "Offset type", if gen { 1 } else { 0 }, OFFSET_MODES),
        pf("offset_random", "Offset random", 0.0, 0.0, 1.0),
        pi("offset_seed", "Offset random seed", 0, 0, 9999),
        pf("global_offset_x", "Global offset X", 0.0, -1.0, 1.0),
        pf("global_offset_y", "Global offset Y", 0.0, -1.0, 1.0),
        pf("position_random", "Position random", 0.0, 0.0, 1.0),
        pf("position_random_x", "Position random X", 1.0, 0.0, 10.0),
        pf("position_random_y", "Position random Y", 1.0, 0.0, 10.0),
        pf("color", "Color", 1.0, 0.0, 1.0),
        pf("color_random", "Color random", 0.0, 0.0, 1.0),
        pf("lum_by_number", "Luminance by number", 0.0, 0.0, 1.0),
        pf("lum_by_scale", "Luminance by scale", 0.0, 0.0, 1.0),
        pf("mask_threshold", "Mask map threshold", 0.5, 0.0, 1.0),
        pb("mask_invert", "Mask map invert", false),
        pf("mask_random", "Random mask", 0.0, 0.0, 1.0),
        pe("mask_checker", "Checker mask", 0, CHECKER),
        pb("mask_horizontal", "Horizontal mask (every other column)", false),
        pb("mask_vertical", "Vertical mask (every other row)", false),
        pb("invert_mask", "Invert mask", false),
        pf("scale_map_multiplier", "Scale map multiplier", 1.0, 0.0, 2.0),
        pf("rotation_map_multiplier", "Rotation map multiplier", 1.0, 0.0, 4.0),
        pf("displacement_intensity", "Displacement map intensity", 0.5, 0.0, 4.0),
        pf("displacement_angle", "Displacement angle", 0.0, 0.0, 1.0),
        pf("color_map_multiplier", "Color map multiplier", 1.0, 0.0, 2.0),
        pf("pattern_specific_random", "Pattern specific random", 0.0, 0.0, 1.0),
        pe("blending", "Blending mode", 0, TILE_BLEND),
        pf("opacity", "Global opacity", 1.0, 0.0, 1.0),
        pf("background", "Background color", 0.0, 0.0, 1.0),
        pb("reverse_order", "Reverse rendering order", false),
    ];
    v.extend(pattern_block());
    v.push(p_seed());
    v
}

/// Category and sub category of a parameter in the Properties panel (empty sub = directly under the category).
pub fn param_group(kind: &str, name: &str) -> (&'static str, &'static str) {
    let scatter = matches!(kind, "tile_sampler" | "tile_generator" | "splatter" | "splatter_circular");
    if scatter {
        return match name {
            "pattern_type" | "pattern_specific" | "pattern_specific_random" | "pattern_distribution" | "pattern_rotation" | "pattern_rotation_random" | "symmetry_random" => ("Pattern", ""),
            "x_amount" | "y_amount" | "count" | "grid_number" | "number" | "rings" => ("Layout", "Amount"),
            "amount_random" | "amount_random_min" | "ring_radius_multiplier" => ("Layout", "Amount"),
            "offset" | "offset_mode" | "offset_random" | "offset_seed" | "global_offset_x" | "global_offset_y" | "pan_x" | "pan_y" => ("Layout", "Offset"),
            "position_random" | "position_random_x" | "position_random_y" | "disorder" | "radius_random" | "angle_random" => ("Layout", "Randomness"),
            "radius" | "spiral_factor" | "spread" => ("Layout", "Circle"),
            "size" | "aspect" | "size_mode" | "scale" | "scale_var" => ("Size", "Size"),
            "scale_random" | "scale_seed" | "size_random" | "scale_by_number" | "invert_number" | "scale_by_ring" | "invert_ring" => ("Size", "Variation"),
            "rotation" | "rotation_random" | "rot_var" | "center_orientation" => ("Rotation", ""),
            "ring_rotation" | "ring_rotation_random" | "ring_rotation_offset" => ("Rotation", "Rings"),
            "color" | "color_random" | "lum_var" | "luminance_random" | "lum_by_number" | "lum_by_scale" | "lum_by_ring" => ("Color", "Luminance"),
            "blending" | "opacity" | "background" | "gain" | "output_min" | "output_max" | "reverse_order" => ("Color", "Blending and output"),
            "mask_threshold" | "mask_invert" => ("Mask", "Mask map"),
            "mask_random" | "mask_checker" | "mask_horizontal" | "mask_vertical" | "invert_mask" => ("Mask", "Procedural mask"),
            "scale_map_multiplier" | "rotation_map_multiplier" | "displacement_intensity" | "displacement_angle" | "color_map_multiplier" => ("Map inputs", ""),
            "seed" => ("Pattern", "Random seed"),
            _ => ("Other", ""),
        };
    }
    let n = name;
    if n.starts_with("seed") || n.ends_with("seed") {
        return ("Random", "");
    }
    if n.starts_with("disorder") || n.starts_with("anisotropy") || n == "speed" {
        return ("Disorder", "");
    }
    if n.starts_with("tile_") {
        return ("Tiling offset", "");
    }
    if n.contains("random") || n.contains("var") || n.contains("jitter") {
        return ("Variation", "");
    }
    if n.contains("opacity") || n.contains("blend") || n.contains("background") || n.contains("contrast") || n.contains("invert") || n.contains("clamp") || n.contains("gain") || n.contains("output") || n.contains("levels") {
        return ("Output", "");
    }
    if n.contains("angle") || n.contains("rotat") || n.contains("offset") || n.starts_with("pan") || n.contains("zoom") || n.contains("symm") || n.contains("mirror") || n.contains("skew") || n.contains("axis") {
        return ("Transform", "");
    }
    ("Main", "")
}

fn p_seed() -> PD {
    pi("seed", "Random seed", 0, 0, 9999)
}
fn p_con() -> PD {
    pf("contrast", "Contrast", 1.0, 0.0, 4.0)
}
fn p_inv() -> PD {
    pb("invert", "Invert", false)
}
fn p_dis() -> PD {
    pf("disorder", "Disorder", 0.0, 0.0, 1.0)
}

/// Editable parameters of each node type (defaults match the engine).
pub fn pdefs(kind: &str) -> Vec<PD> {
    let mut v = pdefs_base(kind);
    // shared noise block: Disorder, Disorder speed, Disorder anisotropy (+ angle), Tile offset
    let (aniso, tile, speed, dis) = match kind {
        "gaussian_noise" | "clouds" | "bnw_spots" | "gaussian_spots" | "directional_noise" | "cells" | "fbm" | "preset_noise" | "noise_pattern" => (true, true, true, true),
        "perlin" => (false, true, true, true),
        "fractal_sum" | "waveform" => (false, false, true, true),
        _ => (false, false, false, false),
    };
    if dis || speed {
        let mut extra: Vec<PD> = Vec::new();
        if !v.iter().any(|p| p.name == "disorder") {
            extra.push(p_dis());
        }
        extra.push(pf("disorder_speed", "Disorder speed", 1.0, 0.0, 4.0));
        if aniso {
            extra.push(pf("anisotropy", "Disorder anisotropy", 0.0, 0.0, 1.0));
            extra.push(pf("anisotropy_angle", "Disorder anisotropy angle", 0.0, 0.0, 1.0));
        }
        if tile {
            extra.push(pf("tile_x", "Tile offset X", 0.0, -16.0, 16.0));
            extra.push(pf("tile_y", "Tile offset Y", 0.0, -16.0, 16.0));
        }
        // after the first of scale/count/etc., before seed/contrast/invert
        let at = v.iter().position(|p| matches!(p.name, "seed" | "contrast" | "invert")).unwrap_or(v.len());
        // keep an existing Disorder where it already is: move the rest in front of the seed
        for (i, e) in extra.into_iter().enumerate() {
            v.insert(at + i, e);
        }
    }
    if kind.starts_with("path_") {
        // every path node can use several splines and place them: merged files are chosen in the panel above
        let at = v.iter().position(|p| p.name == "path").map(|i| i + 1).unwrap_or(v.len());
        let extra = vec![
            pi("path_count", "Paths from the chosen one (0 = rest)", 1, 0, 63),
            pf("path_scale", "Path scale", 1.0, 0.05, 8.0),
            pf("path_rotate", "Path rotation (turns)", 0.0, -1.0, 1.0),
            pf("path_x", "Path offset X", 0.0, -1.0, 1.0),
            pf("path_y", "Path offset Y", 0.0, -1.0, 1.0),
            pe("path_mirror", "Path mirror", 0, &["None", "Horizontal", "Vertical", "Both"]),
            pb("path_reverse", "Reverse path direction", false),
        ];
        for (i, e) in extra.into_iter().enumerate() {
            v.insert(at + i, e);
        }
    }
    if kind == "white_noise" {
        v.insert(0, pe("distribution", "Noise distribution", 0, &["Uniform", "Gaussian", "Triangle"]));
    }
    v
}

pub fn pdefs_base(kind: &str) -> Vec<PD> {
    match kind {
        "white_noise" => vec![p_seed()],
        "gaussian_noise" => vec![pi("scale", "Scale (grain)", 1, 1, 16), p_seed(), p_con(), p_inv()],
        "perlin" => vec![pi("scale", "Scale", 4, 1, 64), p_dis(), p_seed(), p_con(), p_inv()],
        "noise_pattern" => vec![
            pe("preset", "Preset", 0, crate::nodes9::PATTERN_PRESETS),
            pi("scale", "Scale", 32, 1, 256),
            pf("jitter", "Cell disorder", 1.0, 0.0, 1.0),
            pf("edge_width", "Edge width (Cells 2)", 2.0, 0.0, 4.0),
            pf("hardness", "Hardness (Cells 3)", 0.5, 0.0, 1.0),
            pe("color_source", "Colour source (Cells 4)", 0, &["Random", "Pseudorandom", "Input image"]),
            pi("pseudo_random_seed", "Pseudorandom seed (Cells 4)", 0, 0, 10),
            p_dis(),
            p_seed(),
            p_con(),
            p_inv(),
        ],
        "noise_3d" => vec![
            pe("type", "Noise", 0, &["Perlin", "Perlin fractal", "Ridged fractal", "Simplex", "Worley", "Voronoi", "Voronoi fractal"]),
            pf("scale", "Scale", 8.0, 0.0, 64.0),
            pf("size_x", "Size X", 1.0, 0.0, 2.0),
            pf("size_y", "Size Y", 1.0, 0.0, 2.0),
            pf("size_z", "Size Z", 1.0, 0.0, 2.0),
            pf("offset_x", "Offset X", 0.0, -1.0, 1.0),
            pf("offset_y", "Offset Y", 0.0, -1.0, 1.0),
            pf("offset_z", "Offset Z", 0.0, -1.0, 1.0),
            pf("distortion", "Distortion", 0.0, 0.0, 1.0),
            pf("distortion_scale", "Distortion scale", 1.0, 0.0, 2.0),
            pf("baseline", "Baseline (perlin)", 0.5, 0.0, 1.0),
            pf("noise_contrast", "Noise contrast (perlin)", 0.0, 0.0, 1.0),
            pb("absolute", "Absolute (perlin)", false),
            pi("min_level", "Min level (fractal)", 1, 0, 12),
            pi("max_level", "Max level (fractal)", 7, 0, 12),
            pf("roughness", "Roughness (fractal)", 0.5, 0.0, 2.0),
            pf("lacunarity", "Lacunarity (fractal)", 2.0, 1.0, 4.0),
            pf("opacity", "Global opacity (fractal)", 1.0, 0.0, 2.0),
            pf("jitter", "Disorder (cells)", 1.0, 0.0, 1.0),
            pe("style", "Style (cells)", 0, &["F1 distance", "Cell value", "Edges", "F2", "F2 - F1"]),
            pf("edge_thickness", "Edge thickness (cells)", 0.1, 0.0, 0.5),
            p_seed(),
            p_con(),
            p_inv(),
        ],
        "tile_preset" => vec![
            pe("preset", "Pattern", 0, crate::nodes9::TILE_PRESETS),
            pi("tiling", "Tiling", 1, 1, 16),
            pf("interstice", "Gap (bricks 1 and 2)", 0.1, 0.0, 1.0),
            pf("edge_smoothness", "Edge smoothness (bricks 1 and 2)", 0.0, 0.0, 1.0),
            pf("roundness", "Corner roundness (bricks 3)", 0.1, 0.0, 1.0),
            pf("lum_var", "Luminance variation", 0.0, 0.0, 1.0),
            pi("bricks_x", "Bricks across (generator)", 4, 1, 64),
            pi("bricks_y", "Brick rows (generator)", 8, 1, 64),
            pf("bevel_x", "Bevel along (generator)", 0.5, 0.0, 1.0),
            pf("bevel_y", "Bevel across (generator)", 0.5, 0.0, 1.0),
            pf("gap_x", "Gap along (generator)", 0.0, 0.0, 1.0),
            pf("gap_y", "Gap across (generator)", 0.0, 0.0, 1.0),
            pf("offset", "Row offset (generator)", 0.5, 0.0, 1.0),
            pe("variant", "Long rows (brick 2)", 0, &["Joint at the tile edge", "Joint in the middle"]),
            pf("size_random", "Size random (random tiles)", 0.5, 0.0, 1.0),
            p_seed(),
            p_con(),
            p_inv(),
        ],
        "weave_preset" => vec![
            pe("preset", "Pattern", 0, crate::nodes9::WEAVE_PRESETS),
            pi("tiling", "Tiling", 1, 1, 16),
            pb("rotate45", "Rotate 45 degrees", false),
            pi("scale", "Scale (fine knit)", 8, 1, 64),
            pf("disorder", "Disorder", 0.0, 0.0, 1.0),
            p_seed(),
            p_con(),
            p_inv(),
        ],
        "gradient_2pt" => vec![
            pe("mode", "Mode", 1, &["Linear", "Axial (0 at both points, 1 at the peak)", "Circular (angle)", "Radial (distance)"]),
            pf("p1_x", "Point 1 X", 0.5, 0.0, 1.0),
            pf("p1_y", "Point 1 Y", 0.0, 0.0, 1.0),
            pf("p2_x", "Point 2 X", 0.5, 0.0, 1.0),
            pf("p2_y", "Point 2 Y", 1.0, 0.0, 1.0),
            pi("tiling", "Tiling", 1, 1, 16),
            pe("radial_shape", "Radial shape", 0, &["Round", "Square"]),
            pf("peak", "Peak position (axial)", 0.5, 0.0, 1.0),
            pe("profile", "Profile", 0, &["Linear", "Ease out (quadratic)", "Ease in (quadratic)", "Smooth"]),
            pf("angle", "Angle offset (circular)", 0.0, 0.0, 1.0),
            p_inv(),
        ],
        "preset_noise" => vec![
            pe("preset", "Preset", 0, crate::noise_styles::PRESET_NAMES),
            pi("scale", "Scale", 1, 1, 64),
            pf("roughness", "Roughness", 1.0, 0.0, 2.0),
            pf("angle", "Direction (quarter turns)", 0.0, 0.0, 1.0),
            pf("balance", "Balance", 0.5, 0.0, 1.0),
            pf("warp", "Warp intensity", 0.0, -2.0, 4.0),
            pf("angle_random", "Angle random", 0.0, 0.0, 1.0),
            pf("brush", "Brush pattern", 0.0, 0.0, 1.0),
            pf("scratches", "Scratch layer", 0.0, -1.0, 1.0),
            pf("specks", "Specks layer", 0.0, -1.0, 1.0),
            pf("streaks", "Streaks layer", 0.0, -1.0, 1.0),
            pf("cracks", "Cracks layer", 0.0, 0.0, 1.0),
            pf("grain", "Grain", 0.0, -1.0, 1.0),
            pf("sharpen", "Sharpen", 0.0, -4.0, 4.0),
            p_dis(),
            p_seed(),
            p_con(),
            p_inv(),
        ],
        "fbm" => vec![pi("scale", "Scale", 4, 1, 64), pi("octaves", "Octaves", 5, 1, 10), pf("gain", "Gain", 0.5, 0.0, 1.0), p_dis(), p_seed(), pe("mode", "Mode", 0, &["fBm", "Ridged", "Turbulence"]), p_con(), p_inv()],
        "clouds" => vec![pi("scale", "Scale", 3, 1, 64), pi("octaves", "Octaves", 6, 1, 10), pf("gain", "Gain", 0.6, 0.0, 1.0), p_dis(), p_seed(), p_con(), p_inv()],
        "voronoi" | "voronoi_fractal" => {
            let mut v = vec![
                pi("scale", "Scale", 6, 1, 64),
                pf("aspect", "Size Y / X", 1.0, 0.25, 4.0),
                pf("offset_x", "Offset X", 0.0, 0.0, 1.0),
                pf("offset_y", "Offset Y", 0.0, 0.0, 1.0),
                pf("jitter", "Disorder", 1.0, 0.0, 1.0),
                pf("distortion", "Distortion intensity", 0.0, 0.0, 2.0),
                pi("distortion_scale", "Distortion scale", 3, 1, 32),
                pe("mode", "Style", 0, &["F1 (distance)", "Random color", "Edge", "F2", "F2 - F1", "F1 * F2", "F1 / F2"]),
                pf("edge_thickness", "Edge thickness", 0.25, 0.01, 1.0),
                pf("rounded", "Rounded curve", 0.0, 0.0, 1.0),
                pe("distance_mode", "Distance mode", 0, &["Euclidean", "Manhattan", "Chebyshev", "Minkowski"]),
                pf("minkowski", "Minkowski number", 2.0, 0.25, 16.0),
                pf("distance_scale", "Distance scale", 1.0, 0.0, 8.0),
                pe("color_seed_mode", "Random color seed mode", 0, &["Global (node seed)", "Manual"]),
                pi("color_seed", "Random color seed", 0, 0, 9999),
                pb("invert", "Invert", false),
            ];
            if kind == "voronoi_fractal" {
                v.extend([
                    pi("min_level", "Min level", 0, 0, 8),
                    pi("max_level", "Max level", 3, 0, 8),
                    pf("roughness", "Roughness (Add)", 0.5, 0.0, 1.5),
                    pf("lacunarity", "Lacunarity", 2.0, 1.0, 4.0),
                    pf("opacity", "Global opacity", 1.0, 0.0, 1.0),
                    pe("blend", "Blend mode", 0, &["Add", "Max", "Min"]),
                ]);
            }
            v.push(p_seed());
            v
        }
        "bnw_spots" => vec![pe("variant", "Type", 0, &["Spots 1", "Spots 2", "Spots 3"]), pi("scale", "Scale", 8, 1, 64), p_dis(), p_seed(), p_con(), p_inv()],
        "crystal" => vec![pe("variant", "Type", 0, &["Crystal 1", "Crystal 2"]), pi("scale", "Scale", 6, 1, 64), p_dis(), p_seed(), p_con(), p_inv()],
        "directional_noise" => vec![pf("scale", "Scale", 6.0, 1.0, 32.0), pf("stretch", "Stretch", 8.0, 1.0, 32.0), pf("angle", "Angle", 0.0, 0.0, 1.0), p_seed(), p_con(), p_inv()],
        "grunge_map" => vec![pi("map", "Grunge map", 1, 1, 24), pf("balance", "Balance", 0.5, 0.0, 1.0), pf("contrast", "Contrast", 0.5, 0.0, 1.0), pf("brush_pattern", "Brush pattern", 0.0, 0.0, 1.0), pf("cracks", "Cracks (maps 11, 13)", 0.5, 0.0, 1.0), pf("disorder", "Disorder", 0.0, 0.0, 1.0), pb("invert", "Invert", false), p_seed()],
        "grunge" => vec![
            pe("variant", "Type", 0, &["Concrete", "Scratches", "Spots", "Smudges"]),
            pf("balance", "Balance", 0.5, 0.0, 1.0),
            pf("large_variation", "Large variation (Concrete)", 0.55, 0.0, 1.0),
            pf("base_noisiness", "Base noisiness (Concrete)", 0.45, 0.0, 1.0),
            pf("dirt_specks_opacity", "Dirt specks opacity (Concrete)", 0.0, 0.0, 1.0),
            pb("invert_dirt", "Invert dirt (Concrete)", false),
            pf("scratches_opacity", "Scratches opacity (Concrete)", 0.0, 0.0, 1.0),
            pf("sharpen", "Sharpen (Concrete)", 0.0, 0.0, 1.0),
            pi("quantity", "Scratch quantity (Scratches)", 3, 1, 32),
            pf("scratch_width", "Scratch width (Scratches)", 0.4, 0.05, 0.95),
            pf("scratch_length", "Scratch length (Scratches)", 0.45, 0.0, 0.9),
            pi("details", "Details (Spots)", 22, 2, 128),
            pf("coverage", "Coverage (Spots)", 0.35, 0.0, 0.9),
            pf("coverage_contrast", "Coverage contrast (Spots)", 0.25, 0.02, 1.0),
            p_dis(),
            p_seed(),
            p_con(),
            p_inv(),
        ],
        "plasma" => vec![pi("scale", "Scale", 3, 1, 8), p_dis(), p_seed(), p_con(), p_inv()],
        "gradient" => vec![pe("mode", "Type", 0, &["Linear", "Radial"]), pf("angle", "Angle", 0.0, 0.0, 1.0)],
        "gradient_axial" => vec![pf("angle", "Angle", 0.0, 0.0, 1.0)],
        "gradient_circular" => vec![pf("offset", "Offset", 0.0, 0.0, 1.0)],
        "circle" => vec![pf("radius", "Radius", 0.4, 0.0, 0.7), pf("softness", "Edge softness", 0.01, 0.0, 0.5)],
        "rect" => vec![pf("width", "Width", 0.6, 0.0, 1.0), pf("height", "Height", 0.4, 0.0, 1.0), pf("softness", "Edge softness", 0.01, 0.0, 0.5)],
        "polygon" => vec![pi("sides", "Sides", 5, 3, 32), pf("radius", "Radius", 0.4, 0.0, 0.7), pf("softness", "Edge softness", 0.01, 0.0, 0.5), pf("rotation", "Rotation", 0.0, 0.0, 1.0), pf("gradient", "Gradient (centre to edge)", 0.0, 0.0, 1.0), pf("curve", "Gradient curve", 0.0, -1.0, 1.0), pb("gradient_invert", "Invert gradient", false), pi("tiling", "Tiling", 1, 1, 16)],
        "shape" => vec![
            pe("shape", "Shape", 1, SHAPES),
            pf("size_x", "Width", 0.6, 0.0, 1.5),
            pf("size_y", "Height", 0.6, 0.0, 1.5),
            pf("rotation", "Rotation", 0.0, 0.0, 1.0),
            pf("pos_x", "Position X", 0.0, -0.5, 0.5),
            pf("pos_y", "Position Y", 0.0, -0.5, 0.5),
            pi("tiling", "Tiling", 1, 1, 16),
            pf("softness", "Edge softness", 0.02, 0.0, 1.0),
            pf("pattern_specific", "Pattern specific (shapes from the pattern list)", 0.0, 0.0, 1.0),
        ],
        "star" => vec![pi("points", "Points", 5, 3, 16), pf("inner", "Inner radius", 0.45, 0.0, 1.0), pf("size", "Size", 0.4, 0.0, 0.7), pf("softness", "Edge softness", 0.01, 0.0, 0.5), pf("rotation", "Rotation", 0.0, 0.0, 1.0), pb("straight", "Straight edges", false)],
        "checker" => vec![
            pi("tiles", "Tiles", 8, 1, 64),
            pb("white_first", "Start with a white square", false),
            pb("rotate45", "Rotate 45 degrees", false),
            pf("lum_var", "Luminance variation (white squares)", 0.0, 0.0, 1.0),
            p_seed(),
        ],
        "bricks" => vec![
            pi("rows", "Rows", 8, 1, 32),
            pi("cols", "Columns", 4, 1, 32),
            pf("mortar", "Mortar", 0.03, 0.0, 0.2),
            pf("bevel", "Bevel", 0.05, 0.001, 0.5),
            pf("offset", "Row offset", 0.5, 0.0, 1.0),
            pf("lum_var", "Brick variation", 0.3, 0.0, 1.0),
            p_seed(),
        ],
        "stripes" => vec![pi("count", "Count", 8, 1, 64), pf("angle", "Angle", 0.0, 0.0, 1.0), pf("softness", "Edge softness", 0.1, 0.001, 0.5), pf("width", "Stripe width", 0.5, 0.0, 1.0), pe("tile", "Angle mode", 1, &["Exact angle (may not tile)", "Seamless (snap to tileable angle)"]), pi("shift", "Shift (stripes moved across the height; slants them)", 0, 0, 64), pf("phase", "Phase", 0.0, 0.0, 1.0)],
        "weave" => vec![pi("count", "Count", 6, 1, 32), pf("width", "Thread width", 0.8, 0.1, 1.0)],
        "tile_sampler" => {
            let mut v = vec![pi("x_amount", "X amount", 8, 1, 64), pi("y_amount", "Y amount", 8, 1, 64)];
            v.extend(tile_common(false));
            v
        }
        "tile_generator" => {
            let mut v = vec![pi("x_amount", "X amount", 6, 1, 64), pi("y_amount", "Y amount", 6, 1, 64)];
            v.extend(tile_common(true));
            v
        }
        "splatter" => {
            let mut v = vec![pi("count", "Count", 40, 0, 500), pi("grid_number", "Grid number (0 = random)", 0, 0, 32), pf("scale", "Size", 0.1, 0.01, 0.5), pf("scale_var", "Size random", 0.3, 0.0, 1.0), pf("aspect", "Pattern height / width", 1.0, 0.1, 4.0)];
            v.extend([
                pf("rotation", "Rotation", 0.0, 0.0, 1.0),
                pf("rot_var", "Rotation random", 1.0, 0.0, 1.0),
                pe("pattern_rotation", "Pattern rotation", 0, ROT90),
                pe("symmetry_random", "Symmetry random", 0, SYMM),
                pf("lum_var", "Color random", 0.0, 0.0, 1.0),
                pf("disorder", "Disorder (grid jitter)", 1.0, 0.0, 1.0),
                pf("pan_x", "Pan X", 0.0, -1.0, 1.0),
                pf("pan_y", "Pan Y", 0.0, -1.0, 1.0),
                pf("gain", "Gain", 1.0, 0.1, 4.0),
                pf("output_min", "Output min", 0.0, 0.0, 1.0),
                pf("output_max", "Output max", 1.0, 0.0, 1.0),
                pf("background", "Background", 0.0, 0.0, 1.0),
                pe("blending", "Blending mode", 0, TILE_BLEND),
                pf("opacity", "Global opacity", 1.0, 0.0, 1.0),
            ]);
            v.extend(pattern_block());
            v.push(p_seed());
            v
        }
        "splatter_circular" => {
            let mut v = vec![
                pi("number", "Pattern amount", 12, 1, 256),
                pf("amount_random", "Amount random", 0.0, 0.0, 1.0),
                pi("amount_random_min", "Amount random min", 1, 1, 256),
                pi("rings", "Ring amount", 1, 1, 10),
                pf("radius", "Radius", 0.3, 0.0, 0.7),
                pf("radius_random", "Radius random", 0.0, 0.0, 1.0),
                pf("ring_radius_multiplier", "Ring radius multiplier", 0.5, 0.0, 2.0),
                pf("angle_random", "Angle random", 0.0, 0.0, 1.0),
                pf("spiral_factor", "Spiral factor", 0.0, -2.0, 2.0),
                pf("spread", "Spread", 1.0, 0.0, 1.0),
                pf("size", "Size", 0.1, 0.005, 0.5),
                pf("size_random", "Size random", 0.0, 0.0, 1.0),
                pf("scale_by_number", "Scale by pattern number", 0.0, 0.0, 1.0),
                pb("invert_number", "Invert pattern number", false),
                pf("scale_by_ring", "Scale by ring number", 0.0, 0.0, 1.0),
                pb("invert_ring", "Invert ring number", false),
                pf("rotation", "Pattern rotation", 0.0, 0.0, 1.0),
                pf("rotation_random", "Pattern rotation random", 0.0, 0.0, 1.0),
                pf("center_orientation", "Center orientation", 0.0, 0.0, 1.0),
                pf("ring_rotation", "Ring rotation", 0.0, -1.0, 1.0),
                pf("ring_rotation_random", "Ring rotation random", 0.0, 0.0, 1.0),
                pf("ring_rotation_offset", "Ring rotation offset", 0.0, 0.0, 1.0),
                pf("position_random", "Position random", 0.0, 0.0, 1.0),
                pe("symmetry_random", "Symmetry random", 0, SYMM),
                pf("color", "Color", 1.0, 0.0, 1.0),
                pf("luminance_random", "Luminance random", 0.0, 0.0, 1.0),
                pf("lum_by_number", "Luminance by pattern number", 0.0, 0.0, 1.0),
                pf("lum_by_ring", "Luminance by ring number", 0.0, 0.0, 1.0),
                pf("mask_random", "Random mask", 0.0, 0.0, 1.0),
                pf("background", "Background", 0.0, 0.0, 1.0),
                pe("blending", "Blending mode", 0, TILE_BLEND),
                pf("opacity", "Global opacity", 1.0, 0.0, 1.0),
            ];
            v.extend(pattern_block());
            v.push(p_seed());
            v
        }
        "sdf_shape" => vec![
            pe("shape", "Shape", 0, &["Circle", "Box", "Polygon", "Ring"]),
            pf("size", "Size", 0.25, 0.0, 0.5),
            pf("aspect", "Aspect", 1.0, 0.1, 3.0),
            pi("sides", "Sides", 5, 3, 12),
            pf("rotation", "Rotation", 0.0, 0.0, 1.0),
            pf("round", "Corner rounding", 0.0, 0.0, 0.3),
            pf("x", "Position X", 0.0, -0.5, 0.5),
            pf("y", "Position Y", 0.0, -0.5, 0.5),
            pf("range", "Distance range", 0.25, 0.02, 1.0),
        ],
        "sdf_op" => vec![pe("op", "Operation", 0, &["Union", "Subtract", "Intersect"]), pf("smooth", "Smoothing", 0.0, 0.0, 0.3), pf("range", "Distance range", 0.25, 0.02, 1.0)],
        "sdf_mask" => vec![pf("offset", "Offset", 0.0, -0.5, 0.5), pf("softness", "Edge softness", 0.02, 0.0, 0.5)],
        "sdf_form" => vec![
            pe("shape", "Shape", 0, &["Sphere", "Box", "Torus", "Cylinder"]),
            pf("size", "Size", 0.3, 0.05, 0.5),
            pi("repeat", "Repeat", 3, 1, 16),
            pf("round", "Rounding", 0.05, 0.0, 0.3),
            pf("scatter", "Size variation", 0.3, 0.0, 1.0),
            pf("height", "Height", 1.5, 0.1, 4.0),
            p_seed(),
        ],
        "input" => vec![pi("idx", "Input index", 0, 0, 15), pb("gray", "Grayscale", false)],
        "switch" => vec![pi("index", "Index", 0, 0, 3)],
        "blur" => vec![pf("radius", "Intensity", 4.0, 0.0, 64.0)],
        "blur_hq" => vec![pf("intensity", "Intensity", 6.0, 0.0, 64.0), pi("quality", "Quality", 1, 1, 4)],
        "directional_blur" => vec![pf("angle", "Angle", 0.0, 0.0, 1.0), pf("length", "Length", 0.05, 0.0, 0.3)],
        "sharpen" => vec![pf("radius", "Radius", 2.0, 0.0, 32.0), pf("amount", "Amount", 1.0, 0.0, 5.0)],
        "levels" => vec![
            pb("auto", "Auto levels", false),
            pf("low", "Input low", 0.0, 0.0, 1.0),
            pf("high", "Input high", 1.0, 0.0, 1.0),
            pf("gamma", "Gamma", 1.0, 0.1, 4.0),
            pf("out_low", "Output low", 0.0, 0.0, 1.0),
            pf("out_high", "Output high", 1.0, 0.0, 1.0),
        ],
        "adjust" => vec![pf("brightness", "Brightness", 0.0, -1.0, 1.0), pf("contrast", "Contrast", 1.0, 0.0, 3.0)],
        "threshold" => vec![pf("threshold", "Threshold", 0.5, 0.0, 1.0), pe("mode", "Mode", 0, &["Greater", "Greater or equal", "Lower", "Lower or equal"]), pf("softness", "Edge softness", 0.0, 0.0, 1.0)],
        "invert" => vec![pb("invert", "Invert", true)],
        "histogram_scan" => vec![
            pf("position", "Position", 0.5, 0.0, 1.0),
            pf("contrast", "Contrast", 0.5, 0.0, 1.0),
            pb("invert", "Invert", false),
            pb("use_position_map", "Position from the position map (when connected)", true),
            pb("use_contrast_map", "Contrast from the contrast map (when connected)", true),
        ],
        "histogram_range" => vec![pf("position", "Position", 0.5, 0.0, 1.0), pf("range", "Range", 0.2, 0.0, 1.0), pf("contrast", "Contrast", 0.5, 0.0, 1.0), pb("invert", "Invert", false)],
        "quantize" => vec![pi("steps", "Steps", 4, 2, 32)],
        "channel" => vec![pe("ch", "Channel", 0, &["Red", "Green", "Blue", "Alpha"])],
        "tint" => vec![pf("r", "Red", 1.0, 0.0, 1.0), pf("g", "Green", 1.0, 0.0, 1.0), pf("b", "Blue", 1.0, 0.0, 1.0)],
        "colorize" => vec![pe("interp", "Interpolation", 0, &["Linear", "Smooth", "Step", "Smoother"]), pf("smoothness", "Smoothness", 1.0, 0.0, 1.0), pe("addressing", "Gradient addressing", 0, &["Clamp", "Repeat"]), pb("use_alpha", "Use gradient alpha", false)],
        "hsl" => vec![pf("hue", "Hue", 0.0, -1.0, 1.0), pf("sat", "Saturation", 1.0, 0.0, 2.0), pf("val", "Lightness", 1.0, 0.0, 2.0)],
        "transform" => vec![
            pf("offset_x", "Offset X", 0.0, -1.0, 1.0),
            pf("offset_y", "Offset Y", 0.0, -1.0, 1.0),
            pf("scale_x", "Scale X", 1.0, 0.1, 4.0),
            pf("scale_y", "Scale Y", 1.0, 0.1, 4.0),
            pf("rotation", "Rotation", 0.0, 0.0, 1.0),
            pe("tiling", "Tiling mode", 0, &["H and V tiling", "No tiling"]),
        ],
        "mirror" => vec![
            pe("axis", "Mode", 0, &["Mirror axis X", "Mirror axis Y", "Both axes", "Mirror corner"]),
            pf("offset_x", "Axis X offset", 0.5, 0.0, 1.0),
            pf("offset_y", "Axis Y offset", 0.5, 0.0, 1.0),
            pb("invert_x", "Invert axis X", false),
            pb("invert_y", "Invert axis Y", false),
            pe("corner", "Corner type", 0, &["Top left", "Top right", "Bottom left", "Bottom right"]),
        ],
        "warp" => vec![pf("intensity", "Intensity", 0.05, 0.0, 0.5), pe("filtering", "Input filtering", 2, &["Bilinear", "Nearest", "Bicubic"])],
        "directional_warp" => vec![pf("angle", "Warp angle", 0.0, 0.0, 1.0), pf("intensity", "Intensity", 0.1, 0.0, 0.5), pf("offset", "Intensity map offset", 0.5, 0.0, 1.0), pe("filtering", "Input filtering", 2, &["Bilinear", "Nearest", "Bicubic"])],
        "vector_warp" => vec![pf("intensity", "Intensity", 0.05, 0.0, 0.5), pe("directx", "Vector format", 0, &["OpenGL", "DirectX"])],
        "slope_blur" => vec![pi("samples", "Samples", 16, 1, 64), pf("intensity", "Intensity", 0.1, 0.0, 1.0), pe("mode", "Mode", 0, &["Blur", "Min", "Max"])],
        "swirl" => vec![pf("amount", "Amount", 0.3, -2.0, 2.0), pf("radius", "Radius", 0.5, 0.0, 1.0)],
        "highpass" => vec![pf("radius", "Radius", 6.0, 0.0, 64.0)],
        "clamp" => vec![pf("low", "Min", 0.0, 0.0, 1.0), pf("high", "Max", 1.0, 0.0, 1.0), pb("apply_alpha", "Apply to alpha", false)],
        "blend" => vec![
            pe("mode", "Blending mode", 0, BLEND_MODES),
            pf("opacity", "Opacity", 1.0, 0.0, 1.0),
            pe("alpha_mode", "Alpha blending", 1, &["Use source alpha", "Ignore alpha", "Straight alpha blending", "Premultiplied alpha blending"]),
            pf("crop_x0", "Cropping left", 0.0, 0.0, 1.0),
            pf("crop_y0", "Cropping top", 0.0, 0.0, 1.0),
            pf("crop_x1", "Cropping right", 1.0, 0.0, 1.0),
            pf("crop_y1", "Cropping bottom", 1.0, 0.0, 1.0),
        ],
        "normal" => vec![pf("strength", "Intensity", 2.0, 0.0, 10.0), pb("directx", "DirectX format", false), pb("alpha_from_input", "Fill alpha with input", false)],
        "normal_blend" => vec![pf("opacity", "Opacity", 1.0, 0.0, 1.0)],
        "normal_combine" => vec![pe("technique", "Technique", 2, &["Whiteout", "Channel mixer", "Detail oriented"])],
        "ao" => vec![
            pe("mode", "Method", 0, &["Blur (legacy)", "Horizon (HBAO)"]),
            pf("radius", "Spread (legacy)", 6.0, 0.0, 64.0),
            pf("intensity", "Height depth (legacy)", 2.0, 0.0, 10.0),
            pf("hbao_radius", "Radius", 1.0, 0.0, 1.0),
            pf("hbao_intensity", "Intensity", 1.0, 0.0, 2.0),
            pe("quality", "Quality", 1, &["4 samples", "8 samples", "16 samples"]),
            pb("use_world_units", "Use world units", false),
            pf("height_depth", "Height depth (screen)", 0.1, 0.0, 1.0),
            pf("surface_size", "Surface size (cm)", 300.0, 1.0, 1000.0),
            pf("height_depth_cm", "Height depth (cm)", 30.0, 0.0, 1000.0),
        ],
        "quantize_color" => vec![pi("color_amount", "Max. colour amount", 32, 1, 128), pf("contour_smoothing", "Contour smoothing", 0.0, 0.0, 8.0), pf("dithering", "Dithering", 0.0, 0.0, 1.0), pe("dithering_pattern", "Dithering pattern", 0, &["Noise", "Bayer"]), pb("ignore_alpha", "Ignore alpha", false), pe("distance_color_space", "Distance colour space", 0, &["Lab", "RGB"])],
        "curvature" => vec![pe("mode", "Input", 0, &["Normal map", "Height map (legacy)"]), pe("directx", "Normal format", 0, &["OpenGL", "DirectX"]), pf("intensity", "Intensity", 4.0, 0.0, 10.0), pf("radius", "Radius (height input)", 3.0, 0.0, 64.0)],
        "curvature_smooth" => vec![pf("smoothing", "Smoothing", 2.0, 0.0, 32.0), pf("intensity", "Intensity", 6.0, 0.0, 20.0), pb("directx", "DirectX normal input (green points down)", false)],
        "edge_detect" => vec![pe("mode", "Method", 0, &["Edge lines", "Sobel (legacy)"]), pf("edge_width", "Edge width", 2.0, 1.0, 16.0), pf("edge_roundness", "Edge roundness", 4.0, 0.0, 16.0), pb("invert", "Invert", false), pf("tolerance", "Tolerance", 0.0, 0.0, 1.0), pf("intensity", "Intensity (Sobel)", 2.0, 0.0, 10.0)],
        "emboss" => vec![
            pe("mode", "Mode", 0, &["Relief of the input", "Light the input with the height map"]),
            pf("angle", "Angle", 0.125, 0.0, 1.0),
            pf("intensity", "Intensity", 4.0, 0.0, 32.0),
            pf("hl_r", "Highlight R", 1.0, 0.0, 1.0),
            pf("hl_g", "Highlight G", 1.0, 0.0, 1.0),
            pf("hl_b", "Highlight B", 1.0, 0.0, 1.0),
            pf("sh_r", "Shadow R", 0.0, 0.0, 1.0),
            pf("sh_g", "Shadow G", 0.0, 0.0, 1.0),
            pf("sh_b", "Shadow B", 0.0, 0.0, 1.0),
            pf("gloss", "Gloss", 0.0, 0.0, 0.5),
        ],
        "flood_fill" => vec![pf("threshold", "Threshold", 0.5, 0.0, 1.0)],
        "flood_fill_random" => vec![pf("min", "Minimum", 0.0, 0.0, 1.0), pf("max", "Maximum", 1.0, 0.0, 1.0), pf("contrast", "Contrast (spread the values toward black and white)", 0.0, 0.0, 1.0), p_seed()],
        "flood_fill_color" => vec![p_seed()],
        "flood_fill_gradient" => vec![pf("angle", "Angle", 0.0, 0.0, 1.0), pf("variation", "Angle variation", 0.0, 0.0, 1.0), p_seed()],
        "bevel" => vec![
            pf("distance", "Distance", 0.05, 0.0, 0.5),
            pf("smoothing", "Smoothing", 0.5, 0.0, 1.0),
            pf("threshold", "Shape threshold", 0.5, 0.0, 1.0),
            pb("use_as_normal", "Output as normal", false),
            pf("normal_intensity", "Normal intensity", 4.0, 0.0, 50.0),
            pe("corner", "Corner type", 1, &["Round", "Angular"]),
            pe("directx", "Normal format", 0, &["OpenGL", "DirectX"]),
        ],
        "distance" => vec![pf("distance", "Distance range", 0.1, 0.0, 1.0), pf("threshold", "Shape threshold", 0.5, 0.0, 1.0), pe("mode", "Mode", 0, &["Inside", "Outside", "Both"])],
        "color_match" => vec![
            pe("method", "Method", 0, &["Statistics transfer", "Colour swap"]),
            pf("strength", "Strength", 1.0, 0.0, 1.0),
            pe("source_mode", "Source colour mode", 0, &["Average", "Colour", "Input"]),
            pf("src_r", "Source red", 0.5, 0.0, 1.0), pf("src_g", "Source green", 0.5, 0.0, 1.0), pf("src_b", "Source blue", 0.5, 0.0, 1.0),
            pe("target_mode", "Target colour mode", 0, &["Colour", "Input"]),
            pf("target_r", "Target red", 0.5, 0.0, 1.0), pf("target_g", "Target green", 0.5, 0.0, 1.0), pf("target_b", "Target blue", 0.5, 0.0, 1.0),
            pb("custom_variation", "Custom colour variation", false),
            pf("variation_hue", "Hue", 0.0, 0.0, 360.0), pf("variation_chroma", "Chrominance", 1.0, 0.0, 2.0), pf("variation_luma", "Luminance", 1.0, 0.0, 2.0),
            pb("use_mask", "Use mask", true),
            pf("mask_hue_range", "Mask hue range", 30.0, 0.0, 180.0), pf("mask_chroma_range", "Mask chroma range", 0.5, 0.0, 1.0), pf("mask_luma_range", "Mask luma range", 0.5, 0.0, 1.0),
            pf("mask_smoothness", "Mask smoothness", 0.3, 0.0, 2.0),
        ],
        "height_blend" => vec![pf("opacity", "Opacity", 1.0, 0.0, 1.0), pf("contrast", "Height transition", 0.5, 0.0, 2.0), pe("output", "Output", 0, &["Height", "Blend mask"])],
        "make_it_tile_photo" => vec![pf("width", "Mask size H", 0.6, 0.1, 1.0), pf("width_v", "Mask size V", 0.6, 0.1, 1.0)],
        "make_it_tile_patch" => vec![pi("patches", "Patches", 6, 2, 32), pf("softness", "Softness", 0.5, 0.0, 1.0), p_seed()],
        "edge_wear" => vec![pf("amount", "Amount", 0.5, 0.0, 1.0), pf("width", "Edge width", 2.0, 0.0, 16.0), pf("contrast", "Contrast", 1.0, 0.1, 4.0)],
        "dirt" | "dust" => vec![pf("amount", "Amount", 0.5, 0.0, 1.0), pf("contrast", "Contrast", 1.0, 0.1, 4.0)],
        "non_uniform_blur" => vec![pf("intensity", "Intensity", 8.0, 0.0, 64.0)],
        "radial_blur" => vec![pe("mode", "Blur type", 0, &["Spin", "Zoom (legacy)"]), pi("samples", "Samples", 64, 1, 128), pf("angle", "Angle", 0.125, 0.0, 0.5), pf("x", "Center X", 0.0, -0.5, 0.5), pf("y", "Center Y", 0.0, -0.5, 0.5), pf("amount", "Zoom amount", 0.1, 0.0, 1.0), pf("radius", "Zoom radius", 0.5, 0.01, 0.5)],
        "anisotropic_blur" => vec![pf("intensity", "Intensity", 8.0, 0.0, 64.0), pf("anisotropy", "Anisotropy", 0.8, 0.0, 1.0), pf("angle", "Angle", 0.0, 0.0, 1.0)],
        "symmetry" => vec![
            pe("axis", "Symmetry mode", 0, &["Mirror X", "Mirror Y", "Mirror X / Y", "Diagonal left", "Diagonal right", "Eight-fold"]),
            pe("transfer", "Transfer mode", 0, &["Copy", "Add", "Subtract", "Multiply", "Add sub", "Max", "Min"]),
            pf("offset", "Offset", 0.0, -0.5, 0.5),
            pb("flip", "Flip side", false),
        ],
        "skew" => vec![pi("skew_x", "Skew X", 1, -8, 8), pi("skew_y", "Skew Y", 0, -8, 8), pe("align", "Align", 1, &["Center", "Top left", "Bottom right"])],
        "auto_levels" => vec![pb("per_channel", "Per channel", false)],
        "channel_mixer" => vec![
            pf("rr", "Red from red", 1.0, -2.0, 2.0), pf("rg", "Red from green", 0.0, -2.0, 2.0), pf("rb", "Red from blue", 0.0, -2.0, 2.0),
            pf("gr", "Green from red", 0.0, -2.0, 2.0), pf("gg", "Green from green", 1.0, -2.0, 2.0), pf("gb", "Green from blue", 0.0, -2.0, 2.0),
            pf("br", "Blue from red", 0.0, -2.0, 2.0), pf("bg", "Blue from green", 0.0, -2.0, 2.0), pf("bb", "Blue from blue", 1.0, -2.0, 2.0),
            pb("mono", "Monochrome", false),
        ],
        "color_to_mask" => vec![pf("r", "Red", 1.0, 0.0, 1.0), pf("g", "Green", 0.0, 0.0, 1.0), pf("b", "Blue", 0.0, 0.0, 1.0), pf("range", "Range", 0.2, 0.0, 1.0), pf("softness", "Softness", 0.1, 0.0, 1.0)],
        "replace_color" => vec![
            pf("src_r", "Source red", 1.0, 0.0, 1.0), pf("src_g", "Source green", 0.0, 0.0, 1.0), pf("src_b", "Source blue", 0.0, 0.0, 1.0),
            pf("dst_r", "Target red", 0.0, 0.0, 1.0), pf("dst_g", "Target green", 0.0, 0.0, 1.0), pf("dst_b", "Target blue", 1.0, 0.0, 1.0),
            pf("range", "Range", 0.25, 0.0, 1.0), pf("softness", "Softness", 0.15, 0.0, 1.0),
        ],
        "pow" => vec![pf("exponent", "Exponent", 2.0, 0.0, 8.0)],
        "histogram_shift" => vec![pf("position", "Position", 0.5, 0.0, 1.0)],
        "grayscale_conversion" => vec![pf("r", "Red weight", 0.3, 0.0, 1.0), pf("g", "Green weight", 0.59, 0.0, 1.0), pf("b", "Blue weight", 0.11, 0.0, 1.0)],
        "cells" => vec![pe("variant", "Type", 0, &["Cells 1", "Cells 2", "Cells 3", "Cells 4"]), pi("scale", "Scale", 8, 1, 64), pf("jitter", "Randomness", 1.0, 0.0, 1.0), pf("luminance_random", "Luminance random (Type 1)", 0.0, 0.0, 1.0), pe("color_source", "Color source (Type 2)", 0, &["Random", "Pseudorandom", "Image input"]), pi("pseudorandom_seed", "Pseudorandom seed", 0, 0, 9999), pf("edge_width", "Edge width (Type 3)", 0.25, 0.01, 1.0), pf("hardness", "Hardness (Type 4)", 2.0, 0.1, 16.0), p_seed(), p_con(), p_inv()],
        "fractal_sum" => vec![
            pe("mode", "Type", 0, &["Base", "Turbulence", "Creased"]), pi("scale", "Scale", 2, 1, 32), pi("min_level", "Min level", 0, 0, 8), pi("max_level", "Max level", 6, 0, 8),
            pf("roughness", "Roughness", 0.5, 0.0, 1.5), p_seed(), p_con(), p_inv(),
        ],
        "waveform" => vec![
            pe("shape", "Waveform", 0, &["Sine", "Triangle", "Saw", "Square"]), pi("count", "Count", 8, 1, 64), pf("angle", "Angle", 0.0, 0.0, 1.0),
            pf("warp", "Warp", 0.3, 0.0, 4.0), pi("warp_scale", "Warp scale", 4, 1, 32),
            pe("style", "Style", 0, &["Waves", "Sound bars"]),
            pi("samples", "Bars (sound bars)", 256, 1, 1024), pi("wave_number", "Waves (sound bars)", 2, 0, 64), pf("size_min", "Smallest bar (sound bars)", 0.0, 0.0, 1.0), pf("size_max", "Largest bar (sound bars)", 1.0, 0.0, 1.0), pf("noise", "Height noise (sound bars)", 0.5, 0.0, 1.0), pf("intensity", "Brightness (sound bars)", 1.0, 0.0, 4.0),
            p_seed(), p_con(), p_inv(),
        ],
        "gaussian_spots" => vec![pi("scale", "Scale", 8, 1, 64), pf("size", "Size", 0.5, 0.05, 1.0), pf("size_variation", "Size variation", 0.5, 0.0, 1.0), p_seed(), p_con(), p_inv()],
        "normal_invert" => vec![pb("invert_x", "Invert red (X)", false), pb("invert_y", "Invert green (Y)", true), pb("invert_z", "Invert blue (Z)", false), pb("invert_a", "Invert alpha", false)],
        "normal_vector_rotation" => vec![pf("angle", "Rotation angle", 0.0, 0.0, 1.0), pe("directx", "Normal format", 0, &["OpenGL", "DirectX"])],
        "normal_to_height" => vec![pf("relief_balance", "Relief balance", 0.5, 0.0, 1.0), pe("directx", "Normal format", 0, &["OpenGL", "DirectX"]), pf("opacity", "Global opacity", 1.0, 0.0, 1.0)],
        "non_uniform_directional_warp" => vec![pf("intensity", "Intensity", 0.1, -1.0, 1.0), pf("angle", "Angle", 0.0, 0.0, 1.0)],
        "multi_directional_warp" => vec![pf("intensity", "Intensity", 0.1, -1.0, 1.0), pe("directions", "Directions", 2, &["1", "2", "4"]), pf("angle", "Warp angle", 0.0, 0.0, 1.0), pe("mode", "Mode", 0, &["Average", "Max", "Min", "Chain"])],
        "triangle_grid" => vec![
            pe("style", "Style", 0, &["Squares (legacy)", "Jittered mesh"]),
            pi("count", "Count (legacy)", 8, 1, 64),
            pf("border", "Border (legacy)", 0.06, 0.0, 0.5),
            pf("variation", "Variation (legacy)", 1.0, 0.0, 1.0),
            pi("x_amount", "X amount", 16, 1, 64),
            pi("y_amount", "Y amount", 16, 1, 64),
            pf("random_position_multiplier", "Random position", 1.0, 0.0, 2.0),
            pf("quincunx_offset_x", "Quincunx offset X", 0.0, 0.0, 1.0),
            pf("quincunx_offset_y", "Quincunx offset Y", 0.0, 0.0, 1.0),
            pe("triangulation", "Triangulation", 0, &["Fixed", "Alternate", "Random"]),
            pe("output", "Output", 0, &["Smooth heights", "Flat triangles", "Edge distance"]),
            pf("height_random", "Height random", 1.0, 0.0, 1.0),
            pb("thin_lines", "Thin lines", false),
            pf("distance_to_edge_thickness", "Line thickness", 0.025, 0.0, 0.1),
            pf("thin_line_color", "Line colour", 1.0, 0.0, 1.0),
            pf("thin_line_background", "Line background", 0.0, 0.0, 1.0),
            p_seed(),
            p_con(),
            p_inv(),
        ],
        "weave_generator" => vec![pi("tile_x", "Tile X", 1, 1, 20), pi("tile_y", "Tile Y", 1, 1, 20), pi("weave", "Weave", 2, 1, 10), pf("shape", "Shape", 0.5, 0.0, 1.0), pf("gap_x", "Gap X", 0.1, 0.0, 0.9), pf("gap_y", "Gap Y", 0.1, 0.0, 0.9), pe("style", "Style", 0, &["Plain (checker)", "Twill"]), p_con(), p_inv()],
        "starburst" => vec![
            pi("count", "Count", 3, 1, 32), pi("rays", "Rays", 8, 2, 512), pf("sharpness", "Sharpness", 2.0, 0.1, 16.0), pf("size", "Size", 1.0, 0.05, 2.0),
            pf("rotation", "Rotation", 0.0, 0.0, 1.0), pf("rotation_random", "Rotation random", 0.0, 0.0, 1.0),
            pe("style", "Style", 0, &["Wedges", "Light rays"]),
            pf("blur", "Ray blur (light rays)", 2.0, 0.0, 32.0),
            pf("randomness", "Ray angle random (light rays)", 0.0, 0.0, 1.0),
            pf("size_min", "Shortest ray (light rays)", 0.05, 0.0, 1.0),
            pf("size_variation", "Ray length random (light rays)", 0.0, 0.0, 1.0),
            p_seed(), p_con(), p_inv(),
        ],
        "gradient_reflected" => vec![pi("count", "Count", 1, 1, 16), pf("angle", "Angle", 0.0, 0.0, 1.0), p_con(), p_inv()],
        "shape_stroke" => vec![pf("width", "Width", 8.0, 0.5, 256.0), pf("offset", "Offset", 0.0, -1.0, 1.0), pf("softness", "Softness", 0.5, 0.0, 1.0), pf("threshold", "Shape threshold", 0.5, 0.0, 1.0), pf("opacity", "Opacity", 1.0, 0.0, 1.0), pf("color_r", "Outline color R", 1.0, 0.0, 1.0), pf("color_g", "Outline color G", 1.0, 0.0, 1.0), pf("color_b", "Outline color B", 1.0, 0.0, 1.0)],
        "shape_glow" => vec![pf("width", "Width", 16.0, 0.5, 256.0), pf("softness", "Softness", 0.5, 0.0, 1.0), pf("intensity", "Intensity", 1.0, 0.0, 4.0), pf("threshold", "Shape threshold", 0.5, 0.0, 1.0), pf("spread", "Spread", 0.0, 0.0, 1.0), pf("opacity", "Opacity", 1.0, 0.0, 1.0), pf("color_r", "Glow color R", 1.0, 0.0, 1.0), pf("color_g", "Glow color G", 1.0, 0.0, 1.0), pf("color_b", "Glow color B", 1.0, 0.0, 1.0)],
        "f_float" | "f_int" => vec![pf("v", "Value", 0.0, -1000.0, 1000.0)],
        "f_bool" => vec![pb("v", "Value", false)],
        "f_vec2" | "f_ivec2" => vec![pf("x", "X", 0.0, -1000.0, 1000.0), pf("y", "Y", 0.0, -1000.0, 1000.0)],
        "f_vec3" | "f_ivec3" => vec![pf("x", "X", 0.0, -1000.0, 1000.0), pf("y", "Y", 0.0, -1000.0, 1000.0), pf("z", "Z", 0.0, -1000.0, 1000.0)],
        "f_vec4" | "f_ivec4" => vec![pf("x", "X", 0.0, -1000.0, 1000.0), pf("y", "Y", 0.0, -1000.0, 1000.0), pf("z", "Z", 0.0, -1000.0, 1000.0), pf("w", "W", 0.0, -1000.0, 1000.0)],
        "f_random" => vec![pi("seed", "Seed", 0, 0, 9999)],
        "f_input" | "f_graph_input" | "input_value" => {
            let mut v = vec![pe("type", "Type", 0, &["Float", "Integer", "Boolean", "Float2", "Float3", "Float4", "Integer2", "Integer3", "Integer4", "String"]), pf("v", "Default", 0.0, -1000.0, 1000.0), pf("x", "Default X", 0.0, -1000.0, 1000.0), pf("y", "Default Y", 0.0, -1000.0, 1000.0), pf("z", "Default Z", 0.0, -1000.0, 1000.0), pf("w", "Default W", 0.0, -1000.0, 1000.0)];
            if kind == "input_value" {
                v.push(pf("min", "Minimum", 0.0, -1000.0, 1000.0));
                v.push(pf("max", "Maximum", 1.0, -1000.0, 1000.0));
            }
            v
        }
        "pixel_processor" | "value_processor" => vec![pi("seed", "Seed (function)", 0, 0, 9999)],
        "fx_map" => vec![
            pi("depth", "Recursion depth", 12, 0, 32), pi("seed", "Random seed", 0, 0, 9999),
            pe("blend", "Blend", 0, &["Max", "Add", "Over", "Min", "Multiply"]), pb("color", "Colour", false),
            pf("bg", "Background", 0.0, 0.0, 1.0), pf("bg_r", "Background R", 0.0, 0.0, 1.0), pf("bg_g", "Background G", 0.0, 0.0, 1.0), pf("bg_b", "Background B", 0.0, 0.0, 1.0),
            pf("roughness", "Strength per level (quad cells)", 1.0, 0.0, 1.0), pf("opacity", "Opacity (quad cells)", 1.0, 0.0, 1.0),
        ],
        "fxm_quad" => vec![
            pe("pattern", "Pattern", 0, crate::fxmap::QUAD_PATTERNS), pi("image", "Image input (0 = first)", 0, 0, 7),
            pf("value", "Value (grayscale)", 1.0, 0.0, 1.0), pf("r", "Colour R", 1.0, 0.0, 1.0), pf("g", "Colour G", 1.0, 0.0, 1.0), pf("b", "Colour B", 1.0, 0.0, 1.0), pf("a", "Opacity (colour)", 1.0, 0.0, 1.0),
            pf("off_x", "Pattern offset X", 0.0, -1.0, 1.0), pf("off_y", "Pattern offset Y", 0.0, -1.0, 1.0), pf("size_x", "Pattern size X", 1.0, 0.0, 4.0), pf("size_y", "Pattern size Y", 1.0, 0.0, 4.0),
            pf("rotation", "Pattern rotation", 0.0, 0.0, 1.0), pf("variation", "Pattern variation", 0.0, 0.0, 1.0), pe("blend", "Blending", 0, crate::fxmap::QUAD_BLENDS),
            pf("branch_x", "Branch offset X", 0.0, -1.0, 1.0), pf("branch_y", "Branch offset Y", 0.0, -1.0, 1.0),
            pi("seed", "Random seed", 0, 0, 9999), pb("inherit", "Inherit random", true), pe("filtering", "Image filtering", 0, crate::fxmap::QUAD_FILTERS), pb("premul", "Image is premultiplied", false),
        ],
        "fxm_repeat" => vec![pi("count", "Repeats", 1, 0, 1000), pi("seed", "Random seed", 0, 0, 9999), pb("inherit", "Inherit random", true)],
        "fxm_select" => vec![pb("select", "Take branch B", false), pi("seed", "Random seed", 0, 0, 9999), pb("inherit", "Inherit random", true)],
        "fxm_marker" => vec![
            pe("shape", "Shape", 0, &["Circle", "Square", "Diamond", "Image"]), pi("src", "Image input", 0, 0, 3),
            pf("x", "Offset X", 0.0, -1.0, 1.0), pf("y", "Offset Y", 0.0, -1.0, 1.0), pf("w", "Width", 1.0, 0.0, 4.0), pf("h", "Height", 1.0, 0.0, 4.0),
            pf("rot", "Rotation (deg)", 0.0, -360.0, 360.0), pf("value", "Value", 1.0, 0.0, 4.0), pf("softness", "Softness", 0.02, 0.0, 1.0),
            pf("r", "Colour R", 1.0, 0.0, 2.0), pf("g", "Colour G", 1.0, 0.0, 2.0), pf("b", "Colour B", 1.0, 0.0, 2.0),
            pf("rnd_pos", "Random position", 0.0, 0.0, 1.0), pf("rnd_scale", "Random scale", 0.0, 0.0, 1.0), pf("rnd_rot", "Random rotation (deg)", 0.0, 0.0, 180.0), pf("rnd_val", "Random value", 0.0, 0.0, 1.0), pf("rnd_color", "Random colour", 0.0, 0.0, 1.0),
        ],
        "fxm_quadrant" => vec![
            pf("scale", "Child scale", 0.5, 0.0, 2.0), pf("off_x", "Offset X", 0.0, -1.0, 1.0), pf("off_y", "Offset Y", 0.0, -1.0, 1.0),
            pf("p0", "Chance 1", 1.0, 0.0, 1.0), pf("p1", "Chance 2", 1.0, 0.0, 1.0), pf("p2", "Chance 3", 1.0, 0.0, 1.0), pf("p3", "Chance 4", 1.0, 0.0, 1.0),
            pf("v0", "Value 1", 1.0, 0.0, 4.0), pf("v1", "Value 2", 1.0, 0.0, 4.0), pf("v2", "Value 3", 1.0, 0.0, 4.0), pf("v3", "Value 4", 1.0, 0.0, 4.0),
            pf("r0", "Rotation 1 (deg)", 0.0, -360.0, 360.0), pf("r1", "Rotation 2 (deg)", 0.0, -360.0, 360.0), pf("r2", "Rotation 3 (deg)", 0.0, -360.0, 360.0), pf("r3", "Rotation 4 (deg)", 0.0, -360.0, 360.0),
            pf("rnd_pos", "Random position", 0.0, 0.0, 1.0), pf("rnd_scale", "Random scale", 0.0, 0.0, 1.0), pf("rnd_rot", "Random rotation (deg)", 0.0, 0.0, 180.0), pf("rnd_val", "Random value", 0.0, 0.0, 1.0),
        ],
        "fxm_iterate" => vec![
            pi("count", "Count", 4, 0, 1000), pi("grid_x", "Grid X (0 = off)", 0, 0, 64), pi("grid_y", "Grid Y", 0, 0, 64), pf("stagger", "Stagger odd rows", 0.0, 0.0, 1.0),
            pf("dx", "Step X", 0.0, -2.0, 2.0), pf("dy", "Step Y", 0.0, -2.0, 2.0), pf("dscale", "Step scale", 1.0, 0.0, 2.0), pf("drot", "Step rotation (deg)", 0.0, -360.0, 360.0), pf("dval", "Step value", 1.0, 0.0, 2.0),
            pf("rnd_x", "Random X", 0.0, 0.0, 1.0), pf("rnd_y", "Random Y", 0.0, 0.0, 1.0), pf("rnd_scale", "Random scale", 0.0, 0.0, 1.0), pf("rnd_rot", "Random rotation (deg)", 0.0, 0.0, 180.0), pf("rnd_val", "Random value", 0.0, 0.0, 1.0),
        ],
        "fxm_switch" => vec![pe("mode", "Mode", 0, &["By index", "Random"]), pi("index", "Index", 0, 0, 3), pf("w0", "Weight A", 1.0, 0.0, 10.0), pf("w1", "Weight B", 1.0, 0.0, 10.0), pf("w2", "Weight C", 1.0, 0.0, 10.0), pf("w3", "Weight D", 1.0, 0.0, 10.0)],
        "svg" => vec![pe("fit", "Fit", 0, &["Contain", "Stretch", "Cover"]), pf("scale", "Scale", 1.0, 0.05, 8.0), pf("x", "Offset X", 0.0, -1.0, 1.0), pf("y", "Offset Y", 0.0, -1.0, 1.0)],
        "text" => vec![
            pf("size", "Size", 0.2, 0.02, 1.5), pf("x", "Offset X", 0.0, -1.0, 1.0), pf("y", "Offset Y", 0.0, -1.0, 1.0), pe("align", "Align", 1, &["Left", "Centre", "Right"]),
            pf("spacing", "Letter spacing", 0.0, -0.3, 1.0), pf("line_spacing", "Line spacing", 1.0, 0.3, 3.0), pf("rotation", "Rotation (deg)", 0.0, -360.0, 360.0), pf("value", "Value", 1.0, 0.0, 1.0),
            pf("wrap", "Wrap width (0 = off)", 0.0, 0.0, 2.0), pb("use_color", "Coloured text", false), pf("color_r", "Colour R", 1.0, 0.0, 1.0), pf("color_g", "Colour G", 1.0, 0.0, 1.0), pf("color_b", "Colour B", 1.0, 0.0, 1.0),
        ],
        "channel_shuffle" => vec![
            pe("src_r", "Red from", 0, CH_SRC), pe("src_g", "Green from", 1, CH_SRC), pe("src_b", "Blue from", 2, CH_SRC), pe("src_a", "Alpha from", 3, CH_SRC),
            pb("inv_r", "Invert red", false), pb("inv_g", "Invert green", false), pb("inv_b", "Invert blue", false), pb("inv_a", "Invert alpha", false),
        ],
        "baked_map" => vec![pb("flip_y", "Flip green (DirectX / OpenGL)", false), pb("invert", "Invert", false), pb("gray", "Convert to grayscale", false)],
        "mesh_weathering" => vec![pf("wear", "Edge wear", 0.5, 0.0, 1.0), pf("dirt", "Cavity dirt", 0.5, 0.0, 1.0), pf("dust", "Dust on top", 0.5, 0.0, 1.0), pf("up_angle", "Up direction tilt (degrees)", 0.0, -180.0, 180.0), pf("up_spread", "Dust spread (degrees)", 60.0, 1.0, 179.0), pf("breakup", "Breakup", 0.5, 0.0, 1.0), pi("scale", "Breakup scale", 8, 1, 64), p_seed(), p_con(), p_inv()],
        "color_lut" => vec![pf("intensity", "Intensity", 1.0, 0.0, 1.0)],
        "bitmap" => vec![pb("flip_x", "Flip horizontally", false), pb("flip_y", "Flip vertically", false), pb("gray", "Convert to grayscale", false)],
        "sobel" => vec![pf("intensity", "Intensity", 1.0, 0.0, 16.0)],
        "glow" => vec![pf("width", "Glow size", 8.0, 0.0, 128.0), pf("intensity", "Glow amount", 1.0, 0.0, 8.0), pf("clear", "Clear amount", 0.0, 0.0, 1.0), pf("glow_r", "Glow color R", 1.0, 0.0, 1.0), pf("glow_g", "Glow color G", 1.0, 0.0, 1.0), pf("glow_b", "Glow color B", 1.0, 0.0, 1.0)],
        "mosaic" => vec![pf("intensity", "Intensity (with map)", 1.0, 0.0, 1.0), pi("count", "Cells per side (no map)", 16, 1, 128)],
        "contrast_luminosity" => vec![pf("contrast", "Contrast", 1.0, -4.0, 4.0), pf("luminosity", "Luminosity", 0.0, -1.0, 1.0)],
        "premultiply" => vec![pe("mode", "Direction", 0, &["Straight to premultiplied", "Premultiplied to straight"])],
        "histogram_equalize" => vec![pf("amount", "Amount", 1.0, 0.0, 1.0), pb("per_channel", "Each colour channel separately", false)],
        "min_max" => vec![pe("mode", "Operation", 1, &["Minimum", "Maximum"])],
        "morphology" => vec![pe("mode", "Operation", 0, &["Dilate (grow bright)", "Erode (shrink bright)", "Open", "Close"]), pf("radius", "Radius (px at 512)", 4.0, 0.0, 128.0)],
        "multi_switch" => vec![pi("index", "Input (0 = a ... 7 = h)", 0, 0, 7)],
        "lookup_table" => vec![
            pe("mode", "Lookup by", 0, &["Each channel", "Luminance"]), pf("row", "Line of the lookup image", 0.5, 0.0, 1.0), pb("vertical", "Read a column (not a row)", false),
            pb("repeat", "Repeat past the ends", false),
        ],
        "frequency_split" => vec![pf("radius", "Split radius (px at 512)", 8.0, 0.0, 128.0)],
        "frequency_merge" => vec![pf("gain", "Detail gain", 1.0, 0.0, 8.0)],
        "auto_crop" => vec![pf("threshold", "Threshold", 0.001, 0.0, 1.0), pf("margin", "Margin", 0.0, 0.0, 0.4), pb("keep_aspect", "Keep aspect ratio", false)],
        "polar_convert" => vec![pe("mode", "Direction", 0, &["Cartesian to polar", "Polar to cartesian"]), pf("radius", "Radius", 1.0, 0.05, 2.0), pf("rotation", "Rotation", 0.0, 0.0, 1.0), pb("tile", "Make tileable", true)],
        "clone_patch" => vec![
            pf("src_x", "Source X", 0.25, 0.0, 1.0), pf("src_y", "Source Y", 0.25, 0.0, 1.0), pf("dst_x", "Target X", 0.75, 0.0, 1.0), pf("dst_y", "Target Y", 0.75, 0.0, 1.0),
            pf("width", "Width", 0.3, 0.0, 1.0), pf("height", "Height", 0.3, 0.0, 1.0), pf("feather", "Feather", 0.25, 0.0, 1.0), pf("rotation", "Rotation", 0.0, 0.0, 1.0), pf("opacity", "Opacity", 1.0, 0.0, 1.0),
        ],
        "noise_upscale" => vec![pi("cells", "Cells across", 16, 2, 512), pe("mode", "Filtering", 0, &["Bicubic", "Bilinear", "Nearest"])],
        "symmetry_slice" => vec![pi("slices", "Slices", 6, 1, 64), pb("mirror", "Mirror alternate slices", true), pf("rotation", "Rotation", 0.0, 0.0, 1.0), pb("tile", "Make tileable", true)],
        "quad_transform" => vec![
            pf("tl_x", "Top-left X", 0.0, -0.45, 0.45), pf("tl_y", "Top-left Y", 0.0, -0.45, 0.45), pf("tr_x", "Top-right X", 0.0, -0.45, 0.45), pf("tr_y", "Top-right Y", 0.0, -0.45, 0.45),
            pf("br_x", "Bottom-right X", 0.0, -0.45, 0.45), pf("br_y", "Bottom-right Y", 0.0, -0.45, 0.45), pf("bl_x", "Bottom-left X", 0.0, -0.45, 0.45), pf("bl_y", "Bottom-left Y", 0.0, -0.45, 0.45),
            pb("tile", "Make tileable", true),
        ],
        "trapezoid_transform" => vec![pf("top", "Top width", 0.6, 0.05, 2.0), pf("bottom", "Bottom width", 1.0, 0.05, 2.0), pf("skew", "Skew", 0.0, -0.5, 0.5), pb("tile", "Make tileable", true)],
        "bent_normal" => vec![pf("radius", "Radius (px at 512)", 16.0, 1.0, 256.0), pf("relief", "Relief", 0.1, 0.0, 2.0), pi("directions", "Directions", 16, 4, 32), pi("steps", "Steps", 8, 2, 16), pb("directx", "DirectX normal (Y down)", false)],
        "normal_facing" => vec![pf("angle", "Direction", 0.0, 0.0, 1.0), pf("tilt", "Tilt from facing up", 0.0, 0.0, 1.0), pf("sharpness", "Sharpness", 1.0, 0.05, 16.0)],
        "height_normal_blend" => vec![pf("strength", "Height strength", 2.0, 0.0, 16.0), pe("mode", "Blend method", 0, &["Reoriented", "Whiteout", "Partial derivative"]), pf("opacity", "Opacity", 1.0, 0.0, 1.0), pb("directx", "DirectX normal (Y down)", false)],
        "height_to_normal_world" => vec![pf("size", "Texture width (cm)", 100.0, 0.1, 10000.0), pf("height", "Height of white (cm)", 1.0, 0.0, 1000.0), pb("directx", "DirectX normal (Y down)", false)],
        "normal_transform" => vec![
            pf("offset_x", "Offset X", 0.0, -1.0, 1.0), pf("offset_y", "Offset Y", 0.0, -1.0, 1.0), pf("scale_x", "Scale X", 1.0, 0.1, 8.0), pf("scale_y", "Scale Y", 1.0, 0.1, 8.0),
            pf("rotation", "Rotation", 0.0, 0.0, 1.0), pb("rotate_vectors", "Turn the normals with the image", true), pb("directx", "DirectX normal (Y down)", false),
        ],
        "normal_sobel" => vec![pe("mode", "Result", 0, &["Edges", "Curvature"]), pf("intensity", "Intensity", 4.0, 0.0, 64.0), pb("directx", "DirectX normal (Y down)", false)],
        "material_blend" => vec![pf("opacity", "Opacity", 1.0, 0.0, 1.0), pf("height_influence", "Height influence", 1.0, 0.0, 8.0), pf("sharpness", "Edge sharpness", 1.0, 0.1, 32.0)],
        "height_combine" => vec![pe("mode", "Operation", 0, &["Maximum", "Minimum", "Add", "Difference"]), pf("amount", "Amount (add)", 1.0, 0.0, 8.0)],
        "convex_concave" => vec![pf("radius", "Radius (px at 512)", 4.0, 0.0, 128.0), pf("gain", "Gain", 4.0, 0.0, 64.0)],
        "leak_streaks" => vec![pf("length", "Length (px at 512)", 120.0, 0.0, 1024.0), pf("angle", "Direction", 0.25, 0.0, 1.0), pf("intensity", "Intensity", 1.0, 0.0, 8.0), pf("breakup", "Breakup by noise", 0.5, 0.0, 1.0)],
        "settled_dust" => vec![pf("amount", "Amount", 0.6, 0.0, 1.0), pf("slope", "Slope tolerance", 0.3, 0.0, 1.0), pf("cavity", "Cavity", 0.5, 0.0, 1.0), pf("relief", "Relief", 3.0, 0.0, 32.0), pf("radius", "Cavity radius (px at 512)", 8.0, 0.0, 128.0)],
        "palette_extract" => vec![pi("count", "Colours", 5, 2, 16)],
        "palette_apply" => vec![pi("count", "Colours in palette", 5, 2, 16), pf("amount", "Amount", 1.0, 0.0, 1.0)],
        "make_it_tile_mirror" => vec![pe("mode", "Mirror", 0, &["Both axes", "Horizontal", "Vertical"])],
        "blue_noise" => vec![pi("scale", "Cells across", 256, 8, 2048), pf("tightness", "Smoothing", 1.5, 0.3, 6.0), p_seed(), p_con(), p_inv()],
        "fibers" | "scratches" => {
            let fib = kind == "fibers";
            vec![
                pi("count", "Count", if fib { 600 } else { 80 }, 0, 5000), pf("length", "Length (px at 512)", if fib { 90.0 } else { 220.0 }, 1.0, 1024.0), pf("thickness", "Thickness (px at 512)", if fib { 1.4 } else { 1.0 }, 0.1, 32.0),
                pf("angle", "Direction", 0.0, 0.0, 1.0), pf("angle_variation", "Direction variation", if fib { 0.5 } else { 0.2 }, 0.0, 1.0), pf("curl", "Curl", if fib { 0.6 } else { 0.0 }, -8.0, 8.0),
                pf("taper", "Tapered tips", if fib { 0.4 } else { 1.0 }, 0.0, 1.0), pf("brightness_variation", "Brightness variation", 0.5, 0.0, 1.0),
                pe("blend", "Overlaps", 0, &["Keep the brighter", "Add up"]), pf("gain", "Brightness (adding)", 1.0, 0.0, 4.0), pb("soft", "Soft strands", false),
                p_seed(), p_con(), p_inv(),
            ]
        }
        "caustics" => vec![pi("scale", "Scale", 4, 1, 32), pi("octaves", "Octaves", 3, 1, 6), pf("strength", "Strength", 1.0, 0.0, 8.0), pf("sharpness", "Sharpness", 1.0, 0.05, 8.0), p_seed(), p_con(), p_inv()],
        "liquid_noise" => vec![pi("scale", "Scale", 3, 1, 32), pi("octaves", "Octaves", 4, 1, 8), pf("warp", "Warp", 0.35, 0.0, 2.0), pe("mode", "Style", 0, &["Liquid", "Marble"]), pf("bands", "Marble bands", 6.0, 1.0, 32.0), p_seed(), p_con(), p_inv()],
        "perlin_3d" => vec![pi("scale", "Scale", 4, 1, 64), pf("depth", "Depth (loops at 1)", 0.0, 0.0, 1.0), pi("octaves", "Octaves", 4, 1, 8), pf("gain", "Gain", 0.5, 0.0, 1.0), p_seed(), p_con(), p_inv()],
        "paving" => vec![pi("scale", "Stones across", 6, 1, 64), pf("jitter", "Irregularity", 0.5, 0.0, 1.0), pf("gap", "Gap width", 0.04, 0.0, 0.4), pf("bevel", "Bevel", 0.08, 0.001, 0.5), pf("height_variation", "Height variation", 0.3, 0.0, 1.0), pf("dome", "Dome", 0.3, 0.0, 1.0), p_seed(), p_con(), p_inv()],
        "path_stroke" => vec![pe("preset", "Built-in shape", 2, &["Horizontal line", "Diagonal line", "Wave", "Circle", "Spiral", "Zigzag", "Figure eight"]), pf("amplitude", "Wave height", 0.12, 0.0, 0.5), pf("frequency", "Wave count", 2.0, 0.25, 32.0), pi("path", "Path (-1 = all)", -1, -1, 63), pf("width", "Width (px at 512)", 12.0, 0.1, 256.0), pf("softness", "Softness", 0.0, 0.0, 1.0), pe("profile", "Profile", 0, &["Flat", "Round", "Bevel"]), pf("taper_start", "Taper start", 0.0, 0.0, 1.0), pf("taper_end", "Taper end", 0.0, 0.0, 1.0), pf("dash_length", "Dash length (0 = solid)", 0.0, 0.0, 512.0), pf("dash_gap", "Dash gap", 8.0, 0.0, 512.0), pb("fit", "Fit dashes to path length", true), p_con(), p_inv()],
        "path_fill" => vec![pe("preset", "Built-in shape", 3, &["Horizontal line", "Diagonal line", "Wave", "Circle", "Spiral", "Zigzag", "Figure eight"]), pf("amplitude", "Wave height", 0.12, 0.0, 0.5), pf("frequency", "Wave count", 2.0, 0.25, 32.0), pi("path", "Path (-1 = all)", -1, -1, 63), pb("even_odd", "Even-odd fill", false), pf("softness", "Softness", 0.0, 0.0, 1.0), p_con(), p_inv()],
        "path_distance" => vec![pe("preset", "Built-in shape", 2, &["Horizontal line", "Diagonal line", "Wave", "Circle", "Spiral", "Zigzag", "Figure eight"]), pf("amplitude", "Wave height", 0.12, 0.0, 0.5), pf("frequency", "Wave count", 2.0, 0.25, 32.0), pi("path", "Path (-1 = all)", -1, -1, 63), pe("mode", "Mode", 0, &["Falloff from the line", "Signed", "Inward gradient", "Outward gradient"]), pf("range", "Range (px at 512)", 64.0, 1.0, 1024.0), p_con(), p_inv()],
        "path_border" => vec![pe("preset", "Built-in shape", 3, &["Horizontal line", "Diagonal line", "Wave", "Circle", "Spiral", "Zigzag", "Figure eight"]), pf("amplitude", "Wave height", 0.12, 0.0, 0.5), pf("frequency", "Wave count", 2.0, 0.25, 32.0), pi("path", "Path (-1 = all)", -1, -1, 63), pf("width", "Width (px at 512)", 32.0, 0.5, 512.0), pe("side", "Side", 0, &["Inside", "Outside", "Both"]), pe("profile", "Profile", 0, &["Linear", "Round", "Smooth"]), pf("flat", "Flat part", 0.0, 0.0, 0.95), p_con(), p_inv()],
        "path_scatter" => vec![pe("preset", "Built-in shape", 2, &["Horizontal line", "Diagonal line", "Wave", "Circle", "Spiral", "Zigzag", "Figure eight"]), pf("amplitude", "Wave height", 0.12, 0.0, 0.5), pf("frequency", "Wave count", 2.0, 0.25, 32.0), pi("path", "Path (-1 = all)", -1, -1, 63), pf("spacing", "Spacing (px at 512)", 24.0, 1.0, 512.0), pf("scale", "Size (px at 512)", 24.0, 1.0, 512.0), pf("scale_variation", "Size variation", 0.2, 0.0, 1.0), pf("jitter_along", "Jitter along", 0.0, 0.0, 1.0), pf("jitter_across", "Jitter across (px)", 0.0, 0.0, 256.0), pe("rotation", "Rotation", 0, &["Follow the path", "Fixed", "Random"]), pf("angle", "Angle", 0.0, 0.0, 1.0), pf("angle_variation", "Angle variation", 0.0, 0.0, 1.0), pf("density", "Density", 1.0, 0.0, 1.0), pf("opacity_variation", "Opacity variation", 0.0, 0.0, 1.0), pe("blend", "Blend", 0, &["Max", "Add"]), pf("offset", "Start offset (px)", 0.0, 0.0, 512.0), pb("fit", "Fit spacing to path length", true), p_seed(), p_con(), p_inv()],
        "path_track" => vec![pe("preset", "Built-in shape", 2, &["Horizontal line", "Diagonal line", "Wave", "Circle", "Spiral", "Zigzag", "Figure eight"]), pf("amplitude", "Wave height", 0.12, 0.0, 0.5), pf("frequency", "Wave count", 2.0, 0.25, 32.0), pi("path", "Path (-1 = all)", -1, -1, 63), pf("gauge", "Rail distance (px at 512)", 60.0, 2.0, 256.0), pf("rail_width", "Rail width", 8.0, 0.5, 64.0), pf("sleeper_spacing", "Sleeper spacing", 28.0, 2.0, 256.0), pf("sleeper_length", "Sleeper length", 110.0, 2.0, 512.0), pf("sleeper_width", "Sleeper width", 14.0, 1.0, 128.0), pf("rail_height", "Rail height", 0.9, 0.0, 1.0), pf("sleeper_height", "Sleeper height", 0.55, 0.0, 1.0), pb("fit", "Fit sleepers to path length", true), p_con(), p_inv()],
        "path_cable" => vec![pe("preset", "Built-in shape", 2, &["Horizontal line", "Diagonal line", "Wave", "Circle", "Spiral", "Zigzag", "Figure eight"]), pf("amplitude", "Wave height", 0.12, 0.0, 0.5), pf("frequency", "Wave count", 2.0, 0.25, 32.0), pi("path", "Path (-1 = all)", -1, -1, 63), pf("width", "Width (px at 512)", 40.0, 1.0, 256.0), pi("strands", "Strands", 3, 2, 8), pf("strand_size", "Strand size", 0.5, 0.15, 0.95), pf("twist", "Twists per 512 px", 3.0, 0.0, 32.0), p_con(), p_inv()],
        "path_splatter" => vec![pe("preset", "Built-in shape", 2, &["Horizontal line", "Diagonal line", "Wave", "Circle", "Spiral", "Zigzag", "Figure eight"]), pf("amplitude", "Wave height", 0.12, 0.0, 0.5), pf("frequency", "Wave count", 2.0, 0.25, 32.0), pi("path", "Path (-1 = all)", -1, -1, 63), pi("rows", "Rows", 3, 1, 32), pf("row_spacing", "Row spacing (px at 512)", 24.0, 0.0, 512.0), pf("spacing", "Spacing (px at 512)", 24.0, 1.0, 512.0), pf("scale", "Size (px at 512)", 24.0, 1.0, 512.0), pf("scale_variation", "Size variation", 0.2, 0.0, 1.0), pf("jitter_along", "Jitter along", 0.0, 0.0, 1.0), pf("jitter_across", "Jitter across (px)", 0.0, 0.0, 256.0), pe("rotation", "Rotation", 0, &["Follow the path", "Fixed", "Random"]), pf("angle", "Angle", 0.0, 0.0, 1.0), pf("angle_variation", "Angle variation", 0.0, 0.0, 1.0), pf("density", "Density", 1.0, 0.0, 1.0), pf("opacity_variation", "Opacity variation", 0.0, 0.0, 1.0), pe("blend", "Blend", 0, &["Max", "Add"]), pf("offset", "Start offset (px)", 0.0, 0.0, 512.0), pb("fit", "Fit spacing to path length", true), p_seed(), p_con(), p_inv()],
        "path_warp" => vec![pe("preset", "Built-in shape", 2, &["Horizontal line", "Diagonal line", "Wave", "Circle", "Spiral", "Zigzag", "Figure eight"]), pf("amplitude", "Wave height", 0.12, 0.0, 0.5), pf("frequency", "Wave count", 2.0, 0.25, 32.0), pi("path", "Path (-1 = all)", -1, -1, 63), pf("width", "Reach (px at 512)", 80.0, 1.0, 512.0), pf("amount", "Amount (px at 512)", 24.0, -256.0, 256.0), pe("mode", "Mode", 0, &["Push away", "Slide along", "Swirl"])],
        "path_sample" => vec![pe("preset", "Built-in shape", 2, &["Horizontal line", "Diagonal line", "Wave", "Circle", "Spiral", "Zigzag", "Figure eight"]), pf("amplitude", "Wave height", 0.12, 0.0, 0.5), pf("frequency", "Wave count", 2.0, 0.25, 32.0), pi("path", "Path (-1 = all)", -1, -1, 63), pf("width", "Width (px at 512)", 40.0, 0.5, 256.0), p_con(), p_inv()],
        "path_seam" => vec![pe("preset", "Built-in shape", 2, &["Horizontal line", "Diagonal line", "Wave", "Circle", "Spiral", "Zigzag", "Figure eight"]), pf("amplitude", "Wave height", 0.12, 0.0, 0.5), pf("frequency", "Wave count", 2.0, 0.25, 32.0), pi("path", "Path (-1 = all)", -1, -1, 63), pf("stitch_length", "Stitch length (px at 512)", 18.0, 1.0, 256.0), pf("stitch_gap", "Stitch gap", 10.0, 0.0, 256.0), pf("stitch_width", "Stitch width", 5.0, 0.5, 64.0), pf("groove_width", "Groove width", 12.0, 0.0, 128.0), pf("groove_depth", "Groove depth", 0.5, 0.0, 1.0), pf("stitch_height", "Stitch height", 0.7, 0.0, 1.0), pb("fit", "Fit stitches to path length", true), p_con(), p_inv()],
        "herringbone" => vec![pi("scale", "Bricks across (multiple of 4)", 8, 4, 64), pf("gap", "Gap (px at 512)", 3.0, 0.0, 32.0), pf("bevel", "Bevel (px at 512)", 6.0, 0.5, 64.0), pf("variation", "Height variation", 0.3, 0.0, 1.0), pe("mode", "Output", 0, &["Height", "Brick id", "Orientation"]), p_seed(), p_con(), p_inv()],
        "tile_random2" => vec![
            pi("amount_x", "Tiles across", 10, 2, 64), pi("amount_y", "Rows", 10, 2, 64),
            pf("random_size", "Random size", 1.0, 0.0, 1.0), pf("random_size_x", "Random width", 1.0, 0.0, 1.0), pf("random_size_y", "Random height (0 = straight rows)", 1.0, 0.0, 1.0),
            pf("interstice", "Gap between tiles", 0.0, 0.0, 1.0), pf("random_position", "Random position", 0.0, 0.0, 1.0), pf("round_corners", "Corner radius", 0.0, 0.0, 1.0),
            pf("scale", "Tile scale", 1.0, 0.0, 1.0), pf("scale_random", "Random scale", 0.0, 0.0, 1.0), pf("rotation", "Tile rotation (turns, up to 0.25)", 0.0, 0.0, 0.25), pf("rotation_random", "Random rotation", 0.0, 0.0, 1.0),
            pf("corners_random", "Random corner positions", 0.0, 0.0, 1.0), pf("slant", "Random slant", 0.0, 0.0, 1.0), pf("slant_x", "Slant along x", 1.0, 0.0, 1.0), pf("slant_y", "Slant along y", 1.0, 0.0, 1.0),
            pf("bevel", "Bevel width", 0.1, 0.0, 1.0), pb("bevel_per_axis", "Bevel width per axis", false), pf("bevel_x", "Bevel along x", 1.0, 0.0, 1.0), pf("bevel_y", "Bevel along y", 1.0, 0.0, 1.0), pf("bevel_curve", "Rounded bevel", 0.0, 0.0, 1.0),
            pb("mask_random_invert", "Invert random mask", false), pf("mask_random_start", "Random mask start", 0.0, 0.0, 1.0), pf("mask_random_end", "Random mask end (0 = keep all)", 0.0, 0.0, 1.0),
            p_seed(), p_con(), p_inv(),
        ],
        "arc_pavement" => vec![pi("scale", "Fans across", 6, 2, 64), pi("rings", "Stone rows per fan", 4, 1, 12), pf("stones", "Stones on the outer row", 9.0, 2.0, 64.0), pf("gap", "Gap (px at 512)", 2.0, 0.0, 32.0), pf("bevel", "Bevel (px at 512)", 4.0, 0.5, 64.0), pf("variation", "Height variation", 0.3, 0.0, 1.0), pe("mode", "Output", 0, &["Height", "Stone id"]), p_seed(), p_con(), p_inv()],
        "anisotropic_noise" => vec![
            pi("scale", "Cells across", 8, 1, 128), pf("stretch", "Stretch", 6.0, 1.0, 64.0), pf("angle", "Direction", 0.0, 0.0, 1.0), pi("octaves", "Octaves", 3, 1, 8), pf("gain", "Gain", 0.5, 0.1, 1.0),
            pe("style", "Style", 0, &["Fractal streaks", "Value grid (rows of values)"]),
            pi("x_amount", "Values per row (value grid)", 4, 1, 512), pi("y_amount", "Rows (value grid)", 256, 1, 512), pf("smoothness", "Smoothness (value grid)", 1.0, 0.0, 1.0), pf("smoothness_interpolation", "Smoothness interpolation (value grid)", 0.5, 0.0, 1.0), pb("rows_by_resolution", "Rows follow the resolution (value grid)", false), pb("rotate", "Turn 90 degrees (value grid)", false),
            p_seed(), p_con(), p_inv(),
        ],
        "fur_noise" => vec![pi("count", "Hairs", 2500, 1, 20000), pf("length", "Length (px at 512)", 70.0, 2.0, 512.0), pf("thickness", "Thickness (px at 512)", 2.2, 0.4, 32.0), pf("angle", "Direction", 0.25, 0.0, 1.0), pf("angle_variation", "Direction variation", 0.25, 0.0, 1.0), pf("curl", "Curl", 0.8, -8.0, 8.0), pf("taper", "Tapered tips", 0.8, 0.0, 1.0), pf("root_shade", "Root darkness", 0.25, 0.0, 1.0), pf("brightness_variation", "Brightness variation", 0.35, 0.0, 1.0), pf("flow", "Flow", 0.0, 0.0, 1.0), pe("flow_mode", "Flow kind", 0, &["Noise field", "Waves across"]), pi("flow_scale", "Flow scale", 2, 1, 32), pi("waves", "Waves", 2, 0, 16), p_seed(), p_con(), p_inv()],
        "moisture_noise" => vec![pi("scale", "Patch scale", 4, 1, 32), pf("coverage", "Coverage", 0.5, 0.0, 1.0), pf("softness", "Edge softness", 0.15, 0.01, 0.5), pf("drips", "Drips", 0.7, 0.0, 1.0), pf("drip_length", "Drip length (px at 512)", 90.0, 1.0, 512.0), p_seed(), p_con(), p_inv()],
        "microscope_noise" => vec![pi("scale", "Cells across", 10, 1, 64), pf("jitter", "Irregularity", 0.8, 0.0, 1.0), pf("membrane", "Membrane width", 0.06, 0.005, 0.5), pf("nucleus", "Nucleus size", 0.28, 0.02, 0.8), pf("grain", "Cytoplasm grain", 0.25, 0.0, 1.0), pe("mode", "Output", 0, &["Combined", "Membranes", "Nuclei", "Cell id"]), pe("style", "Style", 0, &["Tissue", "Floating cells"]), pi("density", "Cells per side / 3 (floating)", 14, 2, 64), pf("cell_size", "Cell size (floating)", 0.014, 0.002, 0.2), pf("focus", "Share in focus (floating)", 0.07, 0.0, 1.0), pf("warp", "Warp (floating)", 0.5, 0.0, 4.0), pf("brightness", "Brightness (floating)", 0.45, 0.0, 8.0), p_seed(), p_con(), p_inv()],
        "worley_3d" => vec![pi("scale", "Cells across", 6, 1, 64), pf("z", "Slice (z)", 0.0, 0.0, 8.0), pf("jitter", "Irregularity", 1.0, 0.0, 1.0), pe("mode", "Output", 0, &["Nearest", "Second nearest", "Edges (f2 - f1)", "Cell id"]), p_seed(), p_con(), p_inv()],
        "fbm_3d" => vec![pi("scale", "Scale", 4, 1, 64), pf("z", "Slice (z)", 0.0, 0.0, 8.0), pi("octaves", "Octaves", 5, 1, 10), pf("gain", "Gain", 0.5, 0.1, 0.95), pe("mode", "Style", 0, &["Fractal", "Ridged", "Turbulence"]), p_seed(), p_con(), p_inv()],
        "volume_mask" => vec![pe("shape", "Shape", 0, &["Sphere", "Box", "Cylinder", "Torus", "Cone"]), pf("size", "Size", 0.35, 0.01, 0.5), pi("repeat", "Repeat", 1, 1, 64), pf("z", "Slice height", 0.5, 0.0, 1.0), pf("roundness", "Corner roundness", 0.0, 0.0, 0.3), pf("softness", "Edge softness", 0.02, 0.0, 0.5), p_con(), p_inv()],
        "gradient_3d" => vec![pe("shape", "Shape", 0, &["Sphere", "Box", "Cylinder", "Torus", "Cone"]), pf("size", "Size", 0.35, 0.01, 0.5), pi("repeat", "Repeat", 1, 1, 64), pf("z", "Slice height", 0.5, 0.0, 1.0), pf("roundness", "Corner roundness", 0.0, 0.0, 0.3), pf("range", "Gradient range", 0.5, 0.01, 2.0), p_con(), p_inv()],
        "shape_extrude" => vec![pf("length", "Length (px at 512)", 60.0, 1.0, 512.0), pf("angle", "Direction", 0.125, 0.0, 1.0), pf("top", "Top height", 1.0, 0.0, 1.0), pf("falloff", "Wall falloff", 1.0, 0.1, 8.0), p_con(), p_inv()],
        "shape_mapper" => vec![pf("scale", "Pattern size", 1.0, 0.1, 4.0), pf("rotation_random", "Random quarter turns", 0.0, 0.0, 1.0), pf("flip_random", "Random flips", 0.0, 0.0, 1.0), pb("keep_aspect", "Keep square", false), p_seed(), p_con(), p_inv()],
        "cross_section" => vec![pf("position", "Row", 0.5, 0.0, 1.0), pf("height", "Height scale", 1.0, 0.0, 8.0), pf("thickness", "Line width (px at 512)", 3.0, 0.5, 32.0), pb("fill", "Fill under the line", true), p_con(), p_inv()],
        "uber_emboss" => vec![pf("intensity", "Intensity", 3.0, 0.0, 64.0), pf("angle", "Light direction", 0.125, 0.0, 1.0), pf("elevation", "Light height", 0.45, 0.05, 1.0), pf("gloss", "Gloss", 0.5, 0.0, 1.0), pf("highlight", "Highlight strength", 1.0, 0.0, 4.0), pf("gain", "Gain", 1.0, 0.0, 8.0)],
        "diffusion" => vec![pf("smoothing", "Smoothing (px at 512)", 0.0, 0.0, 64.0), pb("gray", "Grayscale result", false)],
        "summed_area_table" => vec![pb("per_channel", "Per colour channel", false), pb("normalize", "Normalize to 0-1", true)],
        "vector_morph" => vec![pf("amount", "Amount (px at 512)", 40.0, 0.0, 512.0), pb("directx", "Flip Y of the vectors", false)],
        "safe_transform" => vec![pe("mode", "Mode", 0, &["Tile-safe (integer)", "Free"]), pf("angle", "Rotation", 0.0, 0.0, 1.0), pf("scale_x", "Scale X", 1.0, 0.01, 32.0), pf("scale_y", "Scale Y", 1.0, 0.01, 32.0), pf("offset_x", "Offset X", 0.0, -1.0, 1.0), pf("offset_y", "Offset Y", 0.0, -1.0, 1.0), pe("filter", "Filtering", 1, &["Nearest", "Bilinear", "Bicubic"]), pe("edge", "Edges (free mode)", 0, &["Repeat", "Mirror", "Clamp", "Empty"]), pb("rotate_after_scale", "Rotate after scaling", false)],
        "snow_filter" => vec![pf("coverage", "Coverage", 0.55, 0.0, 1.0), pf("slope", "Slope that holds snow", 0.5, 0.0, 1.0), pf("steepness", "Slope sensitivity", 6.0, 0.1, 64.0), pf("softness", "Edge softness", 0.2, 0.01, 1.0), pf("breakup", "Breakup", 0.4, 0.0, 1.0), pf("height_bias", "Prefer high places", 0.3, 0.0, 1.0), pf("depth", "Snow depth", 0.15, 0.0, 1.0), pi("scale", "Breakup scale", 6, 1, 64), p_seed()],
        "season_filter" => vec![pf("season", "Season (0 spring, 1 summer, 2 autumn, 3 winter)", 2.0, 0.0, 4.0), pf("strength", "Strength", 1.0, 0.0, 1.0), pf("variation", "Leaf variation", 0.4, 0.0, 1.0), pf("foliage_hue", "Foliage hue", 0.28, 0.0, 1.0), pf("foliage_range", "Hue range", 0.14, 0.02, 0.5), pi("scale", "Variation scale", 8, 1, 64), p_seed()],
        "water_filter" => vec![pf("level", "Water level", 0.35, 0.0, 1.0), pf("softness", "Shore softness", 0.03, 0.001, 0.5), pf("tint_r", "Water red", 0.1, 0.0, 1.0), pf("tint_g", "Water green", 0.25, 0.0, 1.0), pf("tint_b", "Water blue", 0.3, 0.0, 1.0), pf("murkiness", "Murkiness", 0.5, 0.0, 1.0), pf("wet_edge", "Wet ground width", 0.08, 0.0, 0.5), pf("ripples", "Ripples", 0.3, 0.0, 1.0), pi("ripple_scale", "Ripple scale", 24, 1, 128), pf("dry_roughness", "Dry roughness", 0.7, 0.0, 1.0), pf("wet_roughness", "Wet roughness", 0.05, 0.0, 1.0), p_seed()],
        "wire_mesh" => vec![pi("count_x", "Wires across", 8, 1, 128), pi("count_y", "Wires down", 8, 1, 128), pf("wire_width", "Wire width", 0.3, 0.02, 0.95), pf("weave_depth", "Weave depth", 0.6, 0.0, 1.0), pe("mode", "Layout", 0, &["Square weave", "Diamond (chain-link)"]), p_con(), p_inv()],
        "halftone" => vec![pi("cells", "Dots across", 16, 2, 256), pe("mode", "Layout", 0, &["Grid", "Diagonal"]), pf("softness", "Softness", 0.1, 0.0, 1.0), pf("gain", "Brightness gain", 1.0, 0.0, 4.0)],
        "reaction_diffusion" => vec![pi("resolution", "Simulation size", 160, 32, 512), pi("iterations", "Iterations", 600, 1, 6000), pf("feed", "Feed", 0.037, 0.0, 0.1), pf("kill", "Kill", 0.06, 0.0, 0.1), pf("spots", "Random seeds", 0.01, 0.0, 1.0), p_seed()],
        "fxaa" => vec![pf("strength", "Strength", 1.0, 0.0, 1.0), pf("threshold", "Edge threshold", 0.05, 0.0, 1.0)],
        "bilateral_blur" => vec![pf("radius", "Radius (px at 512)", 4.0, 0.0, 32.0), pf("range", "Colour tolerance", 0.1, 0.005, 2.0)],
        "shape_drop_shadow" => vec![pf("angle", "Angle", 0.125, 0.0, 1.0), pf("distance", "Distance", 0.03, 0.0, 0.5), pf("blur", "Blur", 6.0, 0.0, 128.0), pf("opacity", "Opacity", 0.8, 0.0, 1.0), pf("spread", "Spread", 0.0, 0.0, 1.0)],
        k if k.starts_with("x_") => crate::nodes10::pdefs(k),
        _ => vec![],
    }
}

/// Starting values a node gets when it is created in the editor (sensible starting points for a new node).
/// They are written into the node, so graphs saved earlier (which rely on the engine's own fallbacks) keep
/// rendering exactly as before.
pub fn new_node_defaults(kind: &str) -> &'static [(&'static str, f32)] {
    match kind {
        "polygon" => &[("sides", 6.0)],
        "triangle_grid" => &[("style", 1.0)],
        "color_match" => &[("method", 1.0)],
        "ao" => &[("mode", 1.0)],
        "tile_sampler" => &[("x_amount", 16.0), ("y_amount", 16.0)],
        "tile_generator" => &[("x_amount", 10.0), ("y_amount", 10.0)],
        "splatter_circular" => &[("number", 10.0), ("radius", 0.25), ("center_orientation", 1.0)],
        "splatter" => &[("grid_number", 4.0)],
        "weave_generator" => &[("style", 1.0)],
        "histogram_range" => &[("range", 0.5)],
        "pow" => &[("exponent", 4.0)],
        "shape_drop_shadow" => &[("angle", 0.25), ("distance", 0.02), ("opacity", 0.5)],
        _ => &[],
    }
}
