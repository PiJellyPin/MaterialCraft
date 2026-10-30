// SPDX-License-Identifier: MIT OR Apache-2.0
//! Node graph: text format, validation, subgraphs, packages and evaluation.
use crate::expr;
use crate::noise;
use crate::ops;
use crate::png;
use crate::grunge;
use crate::scatter;
use crate::tex::{lum, Tex};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex};
use std::sync::Arc as Rc;

#[path = "graph_gpu.rs"]
mod graph_gpu;

#[derive(Debug)]
pub enum Error {
    Parse(usize, String),
    UnknownNode(u32, String),
    UnknownSlot(u32, String),
    MissingInput(u32, String),
    MissingNode(u32, u32),
    Cycle(u32),
    BadParam(u32, String),
    NoOutputs,
    BadSize(usize),
    Io(String),
    /// the editor started a newer evaluation; this one stopped early
    Cancelled,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Parse(l, m) => write!(f, "line {l}: {m}"),
            Error::UnknownNode(id, k) => write!(f, "node {id}: unknown node type '{k}'"),
            Error::UnknownSlot(id, s) => write!(f, "node {id}: unknown input slot '{s}'"),
            Error::MissingInput(id, s) => write!(f, "node {id}: required input '{s}' is not connected"),
            Error::MissingNode(id, r) => write!(f, "node {id}: refers to missing node {r}"),
            Error::Cycle(id) => write!(f, "cycle detected at node {id}"),
            Error::BadParam(id, m) => write!(f, "node {id}: {m}"),
            Error::NoOutputs => write!(f, "graph has no outputs"),
            Error::BadSize(s) => write!(f, "invalid resolution {s} (use 1..=8192)"),
            Error::Io(m) => write!(f, "io error: {m}"),
            Error::Cancelled => write!(f, "evaluation cancelled"),
        }
    }
}
impl std::error::Error for Error {}

pub struct Spec {
    pub kind: &'static str,
    pub req: &'static [&'static str],
    pub opt: &'static [&'static str],
    pub doc: &'static str,
}

macro_rules! s {
    ($k:expr, [$($r:expr),*], [$($o:expr),*], $d:expr) => {
        Spec { kind: $k, req: &[$($r),*], opt: &[$($o),*], doc: $d }
    };
}

pub const SPECS: &[Spec] = &[
    s!("color", [], [], "constant colour: r g b a"),
    s!("white_noise", [], [], "per-pixel random: seed distribution(0 uniform,1 gaussian,2 triangle)"),
    s!("gaussian_noise", [], [], "gaussian-distributed per-pixel noise: seed scale contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y"),
    s!("perlin", [], [], "perlin noise: scale scale_x scale_y seed disorder disorder_speed tile_x tile_y"),
    s!("fbm", [], [], "fractal noise/clouds: scale octaves gain seed mode(0 fbm,1 ridged,2 turbulence) disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y"),
    s!("voronoi", [], [], "cellular: scale jitter seed mode(0 f1,1 cell id,2 edge,3 f2,4 f2-f1,5 f1*f2,6 f1/f2) aspect offset_x offset_y distortion distortion_scale distance_mode(0 euclid,1 manhattan,2 chebyshev,3 minkowski) minkowski edge_thickness rounded distance_scale color_seed_mode color_seed invert"),
    s!("voronoi_fractal", [], [], "layered voronoi: scale jitter seed mode aspect offset_x offset_y distortion distortion_scale distance_mode minkowski edge_thickness rounded distance_scale min_level max_level roughness lacunarity opacity blend(0 add,1 max,2 min) color_seed_mode color_seed invert"),
    s!("gradient", [], [], "gradient: mode(0 linear,1 radial) angle"),
    s!("circle", [], [], "radius softness"),
    s!("rect", [], [], "width height softness"),
    s!("polygon", [], [], "sides radius softness rotation gradient curve gradient_invert tiling"),
    s!("checker", [], [], "tiles white_first rotate45 lum_var seed"),
    s!("bricks", [], [], "rows cols mortar bevel offset lum_var seed"),
    s!("fx_map", [], ["in", "in2", "in3", "in4", "in5", "in6", "in7", "in8", "background"], "run a pattern program: program=\"name\" depth seed blend (0 max 1 add 2 over 3 min 4 multiply) color bg bg_r bg_g bg_b roughness (strength per quad-tree level) opacity; in..in8 are the image patterns, background the starting image"),
    s!("curve", ["in"], [], "tone curves: points (master), points_r points_g points_b points_a as \"x:y,x:y\" lists; a point may add a slope (x:y:s), two slopes (x:y:sin:sout) or Bezier handles at absolute positions (x:y:lx:ly:rx:ry)"),
    s!("svg", [], [], "vector image: file=\"art.svg\" fit (0 contain 1 stretch 2 cover) scale x y"),
    s!("text", [], [], "text: text=\"...\" font=\"face.ttf\" (optional) or font_family=\"Arial\" (an installed family) size x y align (0 left 1 centre 2 right) spacing line_spacing rotation value wrap(width as a fraction of the image, 0 off) use_color color_r color_g color_b"),
    s!("channel_shuffle", ["in"], ["in2"], "rebuild channels: src_r src_g src_b src_a (0-3 channels of in, 4-7 channels of in2, 8 zero, 9 one), inv_r inv_g inv_b inv_a"),
    s!("input_value", [], [], "graph parameter read by functions: name, type (0 float, 1 int, 2 bool, 3 float2, 4 float3, 5 float4, 6 int2, 7 int3, 8 int4, 9 string), v / x y z w, min max, label"),
    s!("pixel_processor", [], ["in", "in2", "in3", "in4", "in5", "in6", "in7", "in8", "in9", "in10", "in11", "in12", "in13", "in14", "in15", "in16"], "per-pixel expr / expr_g / expr_b / expr_a (string params), or fn=\"function\" to run a function graph per pixel"),
    s!("value_processor", [], ["in", "in2", "in3", "in4", "in5", "in6", "in7", "in8", "in9", "in10", "in11", "in12", "in13", "in14", "in15", "in16"], "run fn=\"function\" once; the result (float, float3 or float4) fills the image. The function can read image statistics"),
    s!("sdf_shape", [], [], "2D signed distance shape: shape(0 circle,1 box,2 polygon,3 ring) size aspect sides rotation round x y range"),
    s!("sdf_op", ["a", "b"], [], "combine SDFs: op(0 union,1 subtract,2 intersect) smooth range"),
    s!("sdf_mask", ["in"], [], "SDF -> mask: offset softness"),
    s!("sdf_form", [], [], "procedural 3D SDF form as height: shape(0 sphere,1 box,2 torus,3 cylinder) size repeat round scatter seed height"),
    s!("bitmap", [], [], "flip_x flip_y gray; imported bitmap (PNG, JPEG, TIFF, EXR, HDR, WebP, DDS, TGA, BMP): file=\"image.png\""),
    s!("baked_map", [], [], "map baked from meshes and stored in the package: set=\"name\" map=\"normal\" flip_y invert gray; see the mesh baker"),
    s!("mesh_weathering", ["curvature"], ["ao", "normal", "noise"], "wear, dirt and dust masks from baked maps: wear dirt dust up_angle(degrees) up_spread breakup scale contrast invert seed"),
    s!("input", [], [], "subgraph input: idx"),
    Spec { kind: "subgraph", req: &[], opt: &SUB_INPUTS, doc: "instance of a reusable subgraph: graph=\"name\"; in0..in15 feed the graph's inputs (idx 0..15)" },
    s!("shape", [], [], "procedural shape: shape(0 square,1 disc,2 paraboloid,3 gaussian,4 diamond,5 pyramid,6 half sphere,7 cone; 100+n pattern n of the scatter list) size_x size_y rotation pos_x pos_y tiling softness pattern_specific"),
    s!("tile_sampler", [], ["pattern", "pattern2", "pattern3", "pattern4", "pattern5", "pattern6", "mask", "scale_map", "rotation_map", "displacement_map", "color_map", "distribution_map", "background"], "grid of pattern instances: x_amount y_amount size aspect size_mode scale scale_random size_random rotation rotation_random pattern_rotation pattern_rotation_random symmetry_random position_random offset offset_mode offset_random global_offset_x global_offset_y color color_random lum_by_number lum_by_scale mask_threshold mask_invert mask_random mask_checker invert_mask blending opacity background reverse_order pattern_type pattern_specific pattern_distribution"),
    s!("tile_generator", [], ["pattern", "pattern2", "pattern3", "pattern4", "pattern5", "pattern6", "mask", "scale_map", "rotation_map", "displacement_map", "color_map", "distribution_map", "background"], "brick-style tiles: x_amount y_amount size aspect size_mode scale scale_random size_random rotation rotation_random pattern_rotation pattern_rotation_random symmetry_random position_random offset offset_mode offset_random global_offset_x global_offset_y color color_random lum_by_number lum_by_scale mask_checker mask_horizontal mask_vertical mask_random invert_mask blending opacity background reverse_order pattern_type pattern_specific pattern_distribution"),
    s!("splatter", [], ["pattern", "pattern2", "pattern3", "pattern4", "pattern5", "pattern6"], "random scatter of a pattern: count grid_number scale scale_var aspect rotation rot_var pattern_rotation lum_var disorder pan_x pan_y symmetry_random gain output_min output_max background blending opacity pattern_type pattern_specific pattern_distribution"),
    s!("splatter_circular", [], ["pattern", "pattern2", "pattern3", "pattern4", "pattern5", "pattern6"], "instances on one or more circles: number amount_random rings radius radius_random ring_radius_multiplier angle_random spiral_factor spread size size_random scale_by_number scale_by_ring rotation rotation_random center_orientation ring_rotation ring_rotation_random position_random color luminance_random lum_by_number lum_by_ring mask_random background blending opacity pattern_type pattern_specific pattern_distribution"),
    s!("clouds", [], [], "soft cloud noise: scale seed contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y"),
    s!("bnw_spots", [], [], "black and white spots: scale seed contrast invert variant disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y"),
    s!("crystal", [], [], "faceted crystal cells: scale disorder seed contrast invert variant"),
    s!("directional_noise", [], [], "streak noise, seamless at any angle: scale stretch angle seed contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y"),
    s!("grunge", [], [], "grunge masks: variant(0 concrete,1 scratches,2 spots,3 smudges) balance large_variation base_noisiness dirt_specks_opacity invert_dirt scratches_opacity sharpen quantity scratch_width scratch_length details coverage coverage_contrast disorder disorder_speed seed contrast invert"),
    s!("plasma", [], [], "smooth plasma waves"),
    s!("stripes", [], [], "stripes: count angle softness width tile(1 = snap to seamless angle, 0 = exact) shift phase"),
    s!("weave", [], [], "woven threads: count width"),
    s!("star", [], [], "star shape: points inner size softness rotation straight"),
    s!("gradient_axial", [], [], "symmetric gradient: angle"),
    s!("gradient_circular", [], [], "angular gradient: offset"),
    s!("switch", [], ["a", "b", "c", "d"], "pass one of up to four inputs: index"),
    s!("flood_fill", ["in"], [], "label connected regions of a mask"),
    s!("flood_fill_random", ["in"], [], "random grayscale per flood-fill region"),
    s!("flood_fill_color", ["in"], [], "random color per flood-fill region"),
    s!("flood_fill_to_index", ["in"], [], "region number of each flood-fill region as a gray level"),
    s!("flood_fill_to_position", ["in"], [], "centre of each flood-fill region (outputs: position, x, y)"),
    s!("flood_fill_to_bbox_size", ["in"], [], "bounding-box size of each flood-fill region (outputs: size, width, height)"),
    s!("flood_fill_gradient", ["in"], [], "gradient across each flood-fill region"),
    s!("vector_warp", ["in", "vector"], [], "displace by a vector map: intensity directx"),
    s!("slope_blur", ["in", "slope"], [], "blur along a slope map: samples intensity mode"),
    s!("blur_hq", ["in"], [], "high quality blur: intensity quality"),
    s!("histogram_range", ["in"], [], "select a range of values: position range contrast invert"),
    s!("bevel", ["in"], [], "bevel a shape (outputs: height, normal): distance (negative grows outwards) smoothing threshold use_as_normal normal_intensity corner(0 round,1 angular) directx"),
    s!("distance", ["in"], [], "distance field of a mask, edges placed between pixels: distance threshold mode(0 inside,1 the format's Distance: 1 on the shape fading outside,2 both)"),
    s!("color_match", ["in"], ["reference", "src_color", "target_color"], "recolour: method(0 statistics transfer from reference,1 colour swap) strength source_mode(0 average,1 colour,2 input) src_r src_g src_b target_mode(0 colour,1 input) target_r target_g target_b custom_variation variation_hue variation_chroma variation_luma use_mask mask_hue_range mask_chroma_range mask_luma_range mask_smoothness"),
    s!("curvature_smooth", ["in"], [], "smooth curvature from a normal map"),
    s!("height_blend", ["bg", "fg"], ["mask"], "blend two height maps: opacity contrast output"),
    s!("make_it_tile_photo", ["in"], [], "make a photo tile: width width_v sharp sharp_v warp warp_v seed"),
    s!("make_it_tile_patch", ["in"], [], "make an image tile with random patches: mode(0 cells,1 stamped) patches softness octave mask_size mask_precision mask_warp size_x size_y disorder size_var rotation rotation_var bg_r bg_g bg_b bg_a color_var lum_var seed"),
    s!("clamp", ["in"], [], "clamp values: low high apply_alpha"),
    s!("highpass", ["in"], [], "high-pass filter: radius"),
    s!("swirl", ["in"], [], "swirl distortion: amount radius"),
    s!("edge_wear", ["in"], ["noise"], "edge wear mask from a height map: amount width contrast"),
    s!("dirt", ["in"], ["noise"], "dirt mask from an ambient occlusion map: amount contrast"),
    s!("dust", ["in"], ["noise"], "dust mask from a normal map: amount contrast"),
    s!("output", [], ["in"], "graph output: usage identifier"),
    s!("dot", ["in"], [], "re-route a link"),
    s!("blur", ["in"], [], "gaussian blur: radius"),
    s!("directional_blur", ["in"], [], "angle length"),
    s!("sharpen", ["in"], [], "radius amount"),
    s!("levels", ["in"], [], "low high gamma out_low out_high"),
    s!("adjust", ["in"], [], "brightness contrast"),
    s!("invert", ["in"], [], "invert colours: invert"),
    s!("threshold", ["in"], [], "binary threshold: threshold softness mode(0 greater,1 greater or equal,2 lower,3 lower or equal)"),
    s!("histogram_scan", ["in"], ["position_map", "contrast_map"], "position contrast invert; position_map / contrast_map (grayscale) give the position and contrast per pixel while use_position_map / use_contrast_map are on"),
    s!("quantize", ["in"], [], "steps"),
    s!("to_gray", ["in"], [], "luminance"),
    s!("channel", ["in"], [], "ch(0 r,1 g,2 b,3 a)"),
    s!("merge", [], ["r", "g", "b", "a"], "combine channels"),
    s!("tint", ["in"], [], "multiply by colour: r g b"),
    s!("colorize", ["in"], [], "gradient map: interp(0 linear,1 smooth,2 step,3 smoother) smoothness addressing(0 clamp,1 repeat) use_alpha  stops p0 r0 g0 b0 a0 ... p15 r15 g15 b15 a15"),
    s!("hsl", ["in"], [], "hue sat val"),
    s!("transform", ["in"], [], "offset_x offset_y scale_x scale_y rotation(turns)"),
    s!("mirror", ["in"], [], "mirror an image: axis(0 x,1 y,2 both,3 corner) offset_x offset_y invert_x invert_y corner(0 top left,1 top right,2 bottom left,3 bottom right)"),
    s!("warp", ["in", "warp"], [], "slope warp: intensity filtering(0 bilinear,1 nearest,2 bicubic)"),
    s!("directional_warp", ["in", "warp"], [], "warp in one direction: angle intensity offset filtering(0 bilinear,1 nearest,2 bicubic)"),
    s!("blend", ["fg", "bg"], ["mask"], "blend two images: mode(0 copy,1 add,2 sub,3 mul,4 screen,5 overlay,6 max,7 min,8 diff,9 softlight,20 add sub,21 switch) opacity alpha_mode(0 source alpha,1 ignore alpha,2 straight,3 premultiplied) crop_x0 crop_y0 crop_x1 crop_y1"),
    s!("normal", ["in"], [], "height->normal (OpenGL +Y): strength directx alpha_from_input"),
    s!("normal_blend", ["a", "b"], ["mask"], "blend normal maps (a = background, b = foreground): opacity"),
    s!("ao", ["in"], [], "ambient occlusion: mode(0 blur,1 horizon) radius intensity hbao_radius hbao_intensity quality(0 4,1 8,2 16 samples) use_world_units height_depth surface_size height_depth_cm"),
    s!("quantize_color", ["in"], [], "reduce to few colours (k-means): color_amount contour_smoothing dithering dithering_pattern(0 noise,1 bayer) ignore_alpha distance_color_space(0 lab,1 rgb)"),
    s!("curvature", ["in"], [], "curvature of a normal map: mode(0 normal map,1 height map) intensity directx radius"),
    s!("edge_detect", ["in"], [], "edge lines: mode(0 lines,1 sobel) edge_width edge_roundness invert tolerance intensity"),
    s!("emboss", ["in"], ["height"], "mode (0 relief of the input, 1 light the input with the height map) angle intensity hl_r hl_g hl_b (highlight colour) sh_r sh_g sh_b (shadow colour) gloss"),
    s!("non_uniform_blur", ["in", "mask"], [], "blur by a mask: intensity"),
    s!("radial_blur", ["in"], [], "spin blur around a centre: mode(0 spin,1 zoom) samples angle x y amount radius"),
    s!("anisotropic_blur", ["in"], [], "blur stretched in one direction: intensity anisotropy angle"),
    s!("symmetry", ["in"], [], "mirror the image around a line: axis(0 x,1 y,2 both,3 diagonal,4 anti-diagonal,5 eight-fold,6 both diagonals) offset flip transfer(0 copy,1 add,2 sub,3 mul,4 add sub,5 max,6 min)"),
    s!("skew", ["in"], [], "shear that keeps tiling: skew_x skew_y (whole numbers) align(0 centre,1 top left,2 bottom right)"),
    s!("auto_levels", ["in"], [], "stretch values to the full range: per_channel"),
    s!("channel_mixer", ["in"], [], "mix colour channels: rr rg rb gr gg gb br bg bb mono"),
    s!("alpha_merge", ["rgb"], ["alpha"], "put a grayscale image into the alpha channel"),
    s!("color_to_mask", ["in"], [], "mask of pixels close to a colour: r g b range softness"),
    s!("replace_color", ["in"], [], "replace one colour with another: src_r src_g src_b dst_r dst_g dst_b range softness"),
    s!("pow", ["in"], [], "raise values to a power: exponent"),
    s!("histogram_shift", ["in"], [], "shift all values: position"),
    s!("grayscale_conversion", ["in"], [], "weighted grayscale: r g b"),
    s!("cells", [], ["color"], "cellular noise: scale jitter seed variant(0 cells1,1 cells2,2 cells3,3 cells4) luminance_random color_source(0 random,1 pseudorandom,2 image) pseudorandom_seed edge_width hardness contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y"),
    s!("fractal_sum", [], [], "sum of noise octaves: scale min_level max_level roughness seed mode(0 base,1 turbulence,2 creased) contrast invert disorder disorder_speed"),
    s!("waveform", [], [], "tileable wave pattern: count angle warp warp_scale seed shape(0 sine,1 triangle,2 saw,3 square) style(0 waves,1 sound bars) samples wave_number size_min size_max noise intensity contrast invert disorder disorder_speed"),
    s!("gaussian_spots", [], [], "soft gaussian spots: scale size size_variation seed contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y"),
    s!("normal_invert", ["in"], [], "flip normal map channels: invert_x invert_y invert_z invert_a"),
    s!("normal_normalize", ["in"], [], "renormalise a normal map"),
    s!("normal_vector_rotation", ["in"], ["rotation"], "rotate normal vectors: angle directx"),
    s!("normal_to_height", ["in"], [], "integrate a normal map into a height map: directx relief_balance opacity"),
    s!("sobel", ["in"], [], "sobel edge magnitude: intensity"),
    s!("glow", ["in"], [], "glow around bright areas: width intensity clear glow_r glow_g glow_b"),
    s!("mosaic", ["in"], ["mosaic_map"], "mosaic of the map regions (or a pixel grid without a map): count intensity"),
    s!("non_uniform_directional_warp", ["in", "intensity"], ["angle"], "warp along a direction chosen per pixel: intensity angle"),
    s!("multi_directional_warp", ["in", "warp"], [], "warp in several directions at once: intensity directions(0 one,1 two,2 four) angle mode(0 average,1 max,2 min,3 chain)"),
    s!("normal_combine", ["base", "detail"], [], "combine two normal maps: technique(0 whiteout,1 channel mixer,2 detail oriented)"),
    s!("triangle_grid", [], [], "grid of triangles: style(0 squares,1 jittered mesh) count border variation x_amount y_amount random_position_multiplier quincunx_offset_x quincunx_offset_y triangulation(0 fixed,1 alternate,2 random) output(0 smooth,1 flat,2 edge distance) height_random thin_lines distance_to_edge_thickness thin_line_color thin_line_background seed contrast invert"),
    s!("grunge_map", [], [], "grunge map 1-24: map balance contrast brush_pattern cracks disorder coverage seed invert"),
    s!("rgba_split", ["in"], [], "split into R, G, B and A grayscale outputs"),
    s!("alpha_split", ["in"], [], "split into an RGB output and an alpha output"),
    s!("weave_generator", [], [], "woven threads with gaps (outputs: height, mask_1, mask_2, opacity): tile_x tile_y weave shape gap_x gap_y style(0 plain,1 twill) contrast invert"),
    s!("starburst", [], [], "star-like rays: count rays sharpness size rotation rotation_random style(0 wedges,1 light rays) blur randomness size_min size_variation seed contrast invert"),
    s!("gradient_reflected", [], [], "gradient that goes 0-1-0: count angle contrast invert"),
    s!("shape_stroke", ["in"], [], "outline of a shape: width offset softness threshold opacity color_r color_g color_b"),
    s!("shape_glow", ["in"], [], "glow around a shape: width softness intensity threshold opacity spread color_r color_g color_b"),
    s!("chrominance_extract", ["in"], [], "colour without brightness"),
    s!("shape_drop_shadow", ["in"], [], "drop shadow of a mask: angle distance blur opacity spread"),
    s!("contrast_luminosity", ["in"], [], "contrast around mid-gray and brightness shift: contrast luminosity"),
    s!("srgb_to_linear", ["in"], [], "decode the sRGB curve (colour channels only)"),
    s!("linear_to_srgb", ["in"], [], "encode the sRGB curve (colour channels only)"),
    s!("premultiply", ["in"], [], "alpha premultiplication: mode(0 straight to premultiplied,1 premultiplied to straight)"),
    s!("histogram_equalize", ["in"], [], "spread values evenly over 0..1: amount per_channel"),
    s!("min_max", ["a", "b"], [], "per-channel minimum or maximum of two images: mode(0 min,1 max)"),
    s!("morphology", ["in"], [], "grow or shrink bright areas: mode(0 dilate,1 erode,2 open,3 close) radius"),
    s!("multi_switch", [], ["a", "b", "c", "d", "e", "f", "g", "h"], "pass one of up to eight inputs: index"),
    s!("color_lut", ["in"], [], "colour grade through a lookup table file (.cube, .3dl, or a strip / Hald image): file=\"grade.cube\" intensity"),
    s!("lookup_table", ["in", "lut"], [], "recolour through a lookup image: mode(0 per channel,1 by luminance) row (the line read) vertical (read a column) repeat"),
    s!("frequency_split", ["in"], [], "split into a blurred low band and a high band centred on gray (outputs: low, high): radius"),
    s!("frequency_merge", ["low", "high"], [], "recombine the bands of frequency_split: gain"),
    s!("auto_crop", ["in"], [], "stretch the non-black area to fill the image: threshold margin keep_aspect"),
    s!("polar_convert", ["in"], [], "mode(0 cartesian to polar,1 polar to cartesian) radius rotation tile"),
    s!("clone_patch", ["in"], [], "copy a patch from one place to another: src_x src_y dst_x dst_y width height feather rotation opacity"),
    s!("noise_upscale", ["in"], [], "smoothly enlarge a coarse image: cells mode(0 bicubic,1 bilinear,2 nearest)"),
    s!("symmetry_slice", ["in"], [], "kaleidoscope of one slice around the centre: slices mirror rotation tile"),
    s!("quad_transform", ["in"], [], "move the four corners: tl_x tl_y tr_x tr_y br_x br_y bl_x bl_y tile"),
    s!("trapezoid_transform", ["in"], [], "perspective taper: top bottom skew tile"),
    s!("bent_normal", ["in"], [], "bent normal of a height map (outputs: normal, ao): radius relief directions steps directx"),
    s!("normal_facing", ["in"], [], "mask of surfaces that face a direction: angle tilt sharpness"),
    s!("height_normal_blend", ["normal", "height"], [], "add the relief of a height map to a normal map: strength mode(0 reoriented,1 whiteout,2 partial derivative) opacity directx"),
    s!("height_to_normal_world", ["in"], [], "normal map from real-world sizes: size height directx"),
    s!("normal_transform", ["in"], [], "transform a normal map and turn its vectors with it: offset_x offset_y scale_x scale_y rotation rotate_vectors directx"),
    s!("normal_sobel", ["in"], [], "edges or curvature of a normal map: mode(0 edges,1 curvature) intensity directx"),
    s!("metal_rough_to_spec_gloss", ["base_color"], ["metallic", "roughness"], "convert a metal / roughness material (outputs: diffuse, specular, glossiness)"),
    s!("spec_gloss_to_metal_rough", ["diffuse", "specular"], ["glossiness"], "convert a specular / glossiness material (outputs: base_color, metallic, roughness)"),
    s!("material_blend", [], ["bg_base", "bg_normal", "bg_height", "bg_rough", "bg_metal", "bg_ao", "fg_base", "fg_normal", "fg_height", "fg_rough", "fg_metal", "fg_ao", "mask"], "blend two complete materials (outputs: base_color, normal, height, roughness, metallic, ao): opacity height_influence sharpness"),
    s!("height_combine", ["a", "b"], ["mask"], "combine two height maps: mode(0 max,1 min,2 add,3 difference) amount"),
    s!("convex_concave", ["in"], [], "ridge and cavity masks of a height map (outputs: convex, concave): radius gain"),
    s!("leak_streaks", ["in"], ["noise"], "streaks that run on from a mask like drips: length angle intensity breakup"),
    s!("settled_dust", ["in"], ["noise"], "dust that settles on flat upward surfaces and in cavities: amount slope cavity relief radius"),
    s!("palette_extract", ["in"], [], "strip of the dominant colours of an image: count"),
    s!("palette_apply", ["in", "palette"], [], "snap colours to the nearest palette colour: count amount"),
    s!("make_it_tile_mirror", ["in"], [], "make an image tile by mirroring: mode(0 both axes,1 horizontal,2 vertical)"),
    s!("blue_noise", [], [], "evenly spread grain with little low-frequency content: scale seed tightness contrast invert"),
    s!("fibers", [], [], "random curved hair-like strands: count length thickness angle angle_variation curl taper brightness_variation blend(0 keep the brighter,1 add up) gain soft seed contrast invert"),
    s!("scratches", [], [], "random thin scratches: count length thickness angle angle_variation curl taper brightness_variation blend(0 keep the brighter,1 add up) gain soft seed contrast invert"),
    s!("caustics", [], [], "light patterns of water: scale octaves strength sharpness seed contrast invert"),
    s!("liquid_noise", [], [], "swirling warped noise: scale octaves warp mode(0 liquid,1 marble) bands seed contrast invert"),
    s!("perlin_3d", [], [], "slice of 3D noise that loops in depth: scale depth octaves gain seed contrast invert"),
    s!("paving", [], [], "irregular paving stones (outputs: height, cell_id, gaps): scale jitter gap bevel height_variation dome seed contrast invert"),
    s!("wire_mesh", [], [], "woven or chain-link wire: count_x count_y wire_width weave_depth mode(0 square,1 diamond) contrast invert"),
    s!("path_stroke", [], [], "line along a spline (dashes, taper, profile 0 flat,1 round,2 bevel): file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit width softness profile taper_start taper_end dash_length dash_gap contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse"),
    s!("path_fill", [], [], "inside of closed splines: file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit even_odd softness contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse"),
    s!("path_distance", [], [], "distance to a spline: mode(0 falloff from the line,1 signed,2 inward gradient,3 outward gradient) range file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse"),
    s!("path_border", [], [], "border or bevel along closed splines (outputs: height, mask): width side(0 inside,1 outside,2 both) profile(0 linear,1 round,2 smooth) flat file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse"),
    s!("path_scatter", [], ["in", "mask"], "copies of a pattern along a spline: spacing scale scale_variation jitter_along jitter_across rotation(0 follow,1 fixed,2 random) angle angle_variation density opacity_variation blend(0 max,1 add) offset seed file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse"),
    s!("path_track", [], [], "railway track along a spline (outputs: height, rails, sleepers): gauge rail_width sleeper_spacing sleeper_length sleeper_width rail_height sleeper_height file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse"),
    s!("path_cable", [], [], "twisted cable along a spline (outputs: height, mask, strand): width strands strand_size twist file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse"),
    s!("path_splatter", [], ["in", "mask"], "several rows of copies of a pattern along a spline, thinned out by a mask: rows row_spacing spacing scale scale_variation jitter_along jitter_across rotation(0 follow,1 fixed,2 random) angle angle_variation density opacity_variation blend(0 max,1 add) offset seed file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse"),
    s!("path_warp", ["in"], [], "push, slide or swirl a picture near a spline: width amount mode(0 push away,1 slide along,2 swirl) file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse"),
    s!("path_sample", [], [], "maps from a spline (outputs: position, width, height, side): width file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse contrast invert"),
    s!("path_seam", [], [], "stitched seam along a spline (outputs: height, stitches): stitch_length stitch_gap stitch_width groove_width groove_depth stitch_height file preset(0 line,1 diagonal,2 wave,3 circle,4 spiral,5 zigzag,6 figure eight) amplitude frequency path(-1 all) fit contrast invert file2 file3 file4 path_count path_scale path_rotate path_x path_y path_mirror(0 none,1 horizontal,2 vertical,3 both) path_reverse"),
    s!("halftone", ["in"], [], "dots whose size follows brightness: cells mode(0 grid,1 diagonal) softness gain"),
    s!("reaction_diffusion", ["in"], [], "growing spots and stripes seeded by the input: resolution iterations feed kill spots seed"),
    s!("herringbone", [], [], "herringbone pavement: scale gap bevel variation mode(0 height,1 brick id,2 orientation) seed contrast invert"),
    s!("tile_random2", [], [], "patchwork of random tiles with even gaps, rounded or pulled-in corners, slant and a bevel that does not depend on tile size: amount_x amount_y random_size random_size_x random_size_y interstice random_position round_corners scale scale_random rotation rotation_random corners_random slant slant_x slant_y bevel bevel_per_axis bevel_x bevel_y bevel_curve mask_random_invert mask_random_start mask_random_end seed contrast invert"),
    s!("arc_pavement", [], [], "fan-shaped arc pavement: scale rings stones gap bevel variation mode(0 height,1 stone id) seed contrast invert"),
    s!("anisotropic_noise", [], [], "noise stretched along any direction, still tiling: scale stretch angle octaves gain style(0 fractal streaks,1 value grid) x_amount y_amount smoothness rotate seed contrast invert"),
    s!("fur_noise", [], [], "fur-like hairs: count length thickness angle angle_variation curl taper root_shade brightness_variation flow flow_mode(0 noise field,1 waves) flow_scale waves seed contrast invert"),
    s!("moisture_noise", [], [], "damp patches with drips: scale coverage softness drips drip_length seed contrast invert"),
    s!("microscope_noise", [], [], "cells seen through a microscope: scale jitter membrane nucleus grain mode(0 combined,1 membrane,2 nuclei,3 cell id) style(0 tissue,1 floating cells) density cell_size focus warp brightness seed contrast invert"),
    s!("worley_3d", [], [], "cellular noise in 3D, one slice per z: scale z jitter mode(0 f1,1 f2,2 f2-f1,3 cell id) seed contrast invert"),
    s!("fbm_3d", [], [], "fractal noise in 3D, one slice per z: scale z octaves gain mode(0 fbm,1 ridged,2 turbulence) seed contrast invert"),
    s!("volume_mask", [], [], "slice through a repeated 3D shape as a mask: shape(0 sphere,1 box,2 cylinder,3 torus,4 cone) size repeat z roundness softness contrast invert"),
    s!("gradient_3d", [], [], "distance gradient of a repeated 3D shape slice: shape size repeat z roundness range contrast invert"),
    s!("shape_extrude", ["in"], [], "extrude shapes along a direction (outputs: height, walls): length angle top falloff contrast invert"),
    s!("shape_mapper", ["in"], ["pattern"], "put a pattern into every flood-filled region: scale rotation_random flip_random keep_aspect seed contrast invert"),
    s!("cross_section", ["in"], [], "plot of the values along one row: position height thickness fill contrast invert"),
    s!("uber_emboss", ["height"], ["mask", "gloss"], "emboss with gloss (outputs: emboss, highlight, shadow): intensity angle elevation gloss highlight gain"),
    s!("diffusion", ["in"], ["mask"], "fill unknown pixels from the known ones (alpha or mask): smoothing gray"),
    s!("summed_area_table", ["in"], [], "running sums of the image: per_channel normalize"),
    s!("vector_morph", ["in", "vector"], [], "move pixels along a vector map and fill the holes: amount directx"),
    s!("safe_transform", ["in"], [], "transform that stays seamless: mode(0 tile-safe,1 free) angle scale_x scale_y offset_x offset_y filter(0 nearest,1 bilinear,2 bicubic) edge(0 repeat,1 mirror,2 clamp,3 empty) rotate_after_scale"),
    s!("snow_filter", ["height"], ["mask"], "snow on upward-facing surfaces (outputs: height, mask): coverage slope softness breakup height_bias depth steepness scale seed"),
    s!("season_filter", ["in"], ["mask"], "recolour foliage for a season (0 spring,1 summer,2 autumn,3 winter): season strength variation foliage_hue foliage_range scale seed"),
    s!("water_filter", ["height"], ["base"], "water in the low parts (outputs: color, mask, height, roughness): level softness tint_r tint_g tint_b murkiness wet_edge ripples ripple_scale dry_roughness wet_roughness seed"),
    s!("fxaa", ["in"], [], "smooth jagged edges: strength threshold"),
    s!("bilateral_blur", ["in"], ["guide"], "blur that keeps edges: radius range"),
    s!("noise_pattern", [], ["input"], "cell, crystal, gaussian, perlin and plasma noises matched to common library noises: preset scale jitter edge_width hardness color_source(0 random,1 pseudorandom,2 input image) pseudo_random_seed seed contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y"),
    s!("noise_3d", [], ["position"], "noise read at 3D positions (a position map, or the texture plane): type(0 perlin,1 perlin fractal,2 ridged fractal,3 simplex,4 worley,5 voronoi,6 voronoi fractal) scale size_x size_y size_z offset_x offset_y offset_z distortion distortion_scale baseline noise_contrast absolute min_level max_level roughness lacunarity opacity jitter style(0 f1,1 cell value,2 edges,3 f2,4 f2-f1) edge_thickness seed contrast invert"),
    s!("tile_preset", [], [], "bricks, cells, spots, meshes, parquet and random tiles: preset tiling interstice edge_smoothness roundness lum_var bricks_x bricks_y bevel_x bevel_y gap_x gap_y offset variant size_random seed contrast invert"),
    s!("weave_preset", [], [], "rope, fibre and weave patterns: preset tiling rotate45 scale disorder seed contrast invert"),
    s!("gradient_2pt", [], [], "gradient between two points: mode(0 linear,1 axial,2 circular,3 radial) p1_x p1_y p2_x p2_y tiling radial_shape peak profile(0 linear,1 ease out,2 ease in,3 smooth) angle invert"),
    s!("preset_noise", [], [], "fractal noise presets matched to common library noises: preset scale roughness angle balance warp angle_random brush scratches specks streaks cracks grain sharpen seed contrast invert disorder disorder_speed anisotropy anisotropy_angle tile_x tile_y"),
];

pub fn spec(kind: &str) -> Option<&'static Spec> {
    if kind.starts_with("sbs:") {
        return crate::sbs_defs::spec(kind);
    }
    SPECS.iter().chain(crate::func::FUNC_SPECS.iter()).chain(crate::fxmap::FXM_SPECS.iter()).find(|s| s.kind == kind).or_else(|| crate::nodes10::spec(kind))
}

pub fn category(kind: &str) -> &'static str {
    match kind {
        k if k.starts_with("x_") => crate::nodes10::category(k),
        "color" | "white_noise" | "gaussian_noise" | "perlin" | "fbm" | "clouds" | "voronoi" | "voronoi_fractal" | "bnw_spots" | "crystal" | "directional_noise" | "grunge" | "plasma"
        | "gradient" | "gradient_axial" | "gradient_circular" | "circle" | "rect" | "polygon" | "shape" | "star" | "checker" | "bricks" | "stripes" | "weave"
        | "tile_sampler" | "tile_generator" | "splatter" | "splatter_circular" | "pixel_processor" | "value_processor" | "fx_map" | "text" | "svg" | "cells" | "fractal_sum" | "waveform" | "gaussian_spots" | "triangle_grid" | "starburst" | "gradient_reflected" | "weave_generator" | "grunge_map" | "blue_noise" | "fibers" | "scratches" | "caustics" | "liquid_noise" | "perlin_3d" | "paving" | "wire_mesh" | "path_stroke" | "path_fill" | "path_distance" | "path_border" | "path_scatter" | "path_track" | "path_cable" | "path_seam" | "path_splatter" | "path_sample" | "herringbone" | "arc_pavement" | "anisotropic_noise" | "fur_noise" | "moisture_noise" | "microscope_noise" | "worley_3d" | "fbm_3d" | "volume_mask" | "gradient_3d" | "preset_noise" | "noise_pattern" | "gradient_2pt" | "weave_preset" | "tile_preset" | "tile_random2" | "noise_3d" => "Generators",
        "sdf_shape" | "sdf_op" | "sdf_mask" | "sdf_form" => "SDF",
        "bitmap" | "baked_map" | "input" | "input_value" | "subgraph" | "output" | "dot" => "Utility",
        k if k.starts_with("f_") => "Function",
        k if k.starts_with("fxm_") => "Pattern Program",
        "normal" | "normal_blend" | "normal_combine" | "normal_invert" | "normal_normalize" | "normal_vector_rotation" | "normal_to_height" | "ao" | "curvature" | "curvature_smooth" | "height_blend" | "edge_wear" | "dirt" | "dust" | "bent_normal" | "normal_facing" | "height_normal_blend" | "height_to_normal_world" | "normal_transform" | "normal_sobel" | "metal_rough_to_spec_gloss" | "spec_gloss_to_metal_rough" | "material_blend" | "height_combine" | "convex_concave" | "leak_streaks" | "settled_dust" | "snow_filter" | "season_filter" | "water_filter" | "mesh_weathering" => "Material",
        _ => "Filters",
    }
}

/// Friendly display name for a node kind.
pub fn label(kind: &str) -> String {
    if let Some(l) = crate::nodes10::label(kind) {
        return l;
    }
    let named = match kind {
        "fbm" => "Fractal Noise",
        "clouds" => "Clouds",
        "bnw_spots" => "BnW Spots",
        "merge" => "RGBA Merge",
        "colorize" => "Gradient Map",
        "voronoi_fractal" => "Voronoi Fractal",
        "color" => "Uniform Color",
        "hsl" => "HSL",
        "ao" => "Ambient Occlusion",
        "transform" => "Transform 2D",
        "blur_hq" => "Blur HQ",
        "make_it_tile_photo" => "Make It Tile Photo",
        "make_it_tile_patch" => "Make It Tile Patch",
        "sdf_shape" => "SDF Shape",
        "sdf_op" => "SDF Combine",
        "sdf_mask" => "SDF Mask",
        "sdf_form" => "SDF 3D Form",
        "pixel_processor" => "Pixel Processor",
        "value_processor" => "Value Processor",
        "svg" => "SVG",
        "path_stroke" => "Path Stroke",
        "path_fill" => "Path Fill",
        "path_distance" => "Path Distance",
        "path_border" => "Path Border",
        "path_scatter" => "Path Scatter",
        "path_track" => "Path Track",
        "path_cable" => "Path Cable",
        "path_seam" => "Path Seam",
        "path_splatter" => "Path Splatter",
        "path_warp" => "Path Warp",
        "path_sample" => "Path Sample",
        "baked_map" => "Baked Map",
        "mesh_weathering" => "Mesh Weathering",
        "herringbone" => "Herringbone",
        "arc_pavement" => "Arc Pavement",
        "tile_random2" => "Random Tiles 2",
        "anisotropic_noise" => "Anisotropic Noise",
        "fur_noise" => "Fur Noise",
        "moisture_noise" => "Moisture Noise",
        "microscope_noise" => "Microscope Noise",
        "worley_3d" => "Worley Noise 3D",
        "fbm_3d" => "Fractal Noise 3D",
        "volume_mask" => "Volume Mask",
        "gradient_3d" => "Gradient 3D",
        "shape_extrude" => "Shape Extrude",
        "shape_mapper" => "Shape Mapper",
        "cross_section" => "Cross Section",
        "uber_emboss" => "Uber Emboss",
        "summed_area_table" => "Summed Area Table",
        "vector_morph" => "Vector Morph",
        "safe_transform" => "Safe Transform",
        "snow_filter" => "Snow",
        "season_filter" => "Season",
        "water_filter" => "Water",
        "text" => "Text",
        "fx_map" => "Pattern Program Map",
        "input_value" => "Input Value",
        "normal" => "Normal From Height",
        "normal_blend" => "Normal Blend",
        "rgba_split" => "RGBA Split",
        "pow" => "Pow",
        "sobel" => "Sobel",
        "srgb_to_linear" => "sRGB To Linear",
        "linear_to_srgb" => "Linear To sRGB",
        "contrast_luminosity" => "Contrast / Luminosity",
        "clone_patch" => "Clone Patch",
        "perlin_3d" => "Perlin Noise 3D",
        "gradient_2pt" => "Gradient (Two Points)",
        "weave_preset" => "Weave Patterns",
        "tile_preset" => "Tile Patterns",
        "noise_3d" => "3D Noise (Position)",
        "noise_pattern" => "Noise Patterns",
        "fxaa" => "FXAA",
        "metal_rough_to_spec_gloss" => "Metal/Rough To Spec/Gloss",
        "spec_gloss_to_metal_rough" => "Spec/Gloss To Metal/Rough",
        _ => "",
    };
    if !named.is_empty() {
        return named.to_string();
    }
    let fnamed = match kind {
        "f_vec2" => "Float2",
        "f_vec3" => "Float3",
        "f_vec4" => "Float4",
        "f_ivec2" => "Integer2",
        "f_ivec3" => "Integer3",
        "f_ivec4" => "Integer4",
        "f_int" => "Integer",
        "f_bool" => "Boolean",
        "f_graph_input" => "Graph Input",
        "f_sample_gray" => "Sample Grayscale",
        "f_sample_color" => "Sample Color",
        "f_lerp" => "Lerp",
        "f_atan2" => "Atan2",
        "f_eq" => "Equal",
        "f_ne" => "Not Equal",
        "f_lt" => "Less Than",
        "f_le" => "Less Or Equal",
        "f_gt" => "Greater Than",
        "f_ge" => "Greater Or Equal",
        "f_pos" => "Position",
        "f_out" => "Function Output",
        "fxm_out" => "Program Output",
        "fxm_quad" => "Quad Cell",
        "fxm_select" => "Select Branch",
        _ => "",
    };
    if !fnamed.is_empty() {
        return fnamed.to_string();
    }
    let kind = kind.strip_prefix("fxm_").or_else(|| kind.strip_prefix("f_")).unwrap_or(kind);
    kind.split('_').map(|w| { let mut c = w.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() }).collect::<Vec<_>>().join(" ")
}

/// Output usages in the order they are listed on a graph node.
pub const OUTPUT_USAGES: [&str; 12] = ["basecolor", "normal", "roughness", "metallic", "height", "ambient_occlusion", "opacity", "emissive", "diffuse", "specular", "glossiness", "out"];

pub fn usage_label(usage: &str) -> String {
    match usage {
        "basecolor" => "Base Color",
        "normal" => "Normal",
        "roughness" => "Roughness",
        "metallic" => "Metallic",
        "height" => "Height",
        "ambient_occlusion" => "Ambient Occlusion",
        "opacity" => "Opacity",
        "emissive" => "Emissive",
        "diffuse" => "Diffuse",
        "specular" => "Specular",
        "glossiness" => "Glossiness",
        "out" => "Output",
        other => return label(other),
    }
    .to_string()
}

/// Value an unconnected Output node produces.
pub fn output_default(usage: &str) -> [f32; 4] {
    match usage {
        "normal" => [0.5, 0.5, 1.0, 1.0],
        "metallic" | "emissive" => [0.0, 0.0, 0.0, 1.0],
        "ambient_occlusion" | "opacity" => [1.0; 4],
        "specular" => [0.04, 0.04, 0.04, 1.0],
        _ => [0.5, 0.5, 0.5, 1.0],
    }
}

/// Small graph used for library thumbnails and the node smoke test. None = no preview graph.
pub fn demo_graph_for(kind: &str) -> Option<String> {
    if matches!(kind, "input" | "subgraph" | "bitmap" | "baked_map" | "output" | "dot") {
        return None;
    }
    let sp = spec(kind)?;
    match kind {
        "fx_map" => return Some("size 64\nsubgraph dots\nmeta kind \"fxmap\"\nnode 1 fxm_marker\nnode 2 fxm_iterate body=@1 grid_x=4 grid_y=4\nnode 3 fxm_out in=@2\nend\nnode 1 fx_map program=\"dots\"\noutput o 1\n".into()),
        "value_processor" => return Some("size 64\nsubgraph avg\nmeta kind \"function\"\nnode 1 f_image_avg\nnode 2 f_out in=@1\nend\nnode 100 fbm scale=4 seed=2\nnode 1 value_processor fn=\"avg\" in=@100\noutput o 1\n".into()),
        "color_lut" => return Some("size 64\nresource grade.cube VElUTEUgImRlbW8iCkxVVF8zRF9TSVpFIDIKMCAwIDAKMSAwIDAKMCAxIDAKMSAxIDAKMCAwIDEKMSAwIDEKMCAxIDEKMSAxIDEK\nnode 2 color r=0.8 g=0.4 b=0.1\nnode 3 color_lut in=@2 file=\"grade.cube\"\noutput o 3\n".into()),
        "svg" => return Some("size 64\nresource art.svg PHN2ZyB2aWV3Qm94PSIwIDAgMTAgMTAiPjxjaXJjbGUgY3g9IjUiIGN5PSI1IiByPSI0IiBmaWxsPSIjZmZmIi8+PC9zdmc+\nnode 1 svg file=\"art.svg\"\noutput o 1\n".into()),
        _ => {}
    }
    let mut t = String::from(
        "size 64\nnode 100 fbm scale=4 octaves=4 seed=2\nnode 101 gradient angle=0.1\nnode 102 sdf_shape shape=0 size=0.3\nnode 103 sdf_shape shape=1 size=0.25 x=0.15\nnode 104 voronoi scale=5 mode=2 seed=3\nnode 105 threshold in=@104 threshold=0.2\nnode 106 flood_fill in=@105\nnode 107 normal in=@100 strength=3\nnode 108 colorize in=@100 p0=0 r0=1 g0=0.2 b0=0.1 p1=1 r1=0.1 g1=0.4 b1=1\n",
    )
    .replace("\\n", "\n");
    let (a, b) = match kind {
        "sdf_op" | "sdf_mask" => (102, 103),
        "flood_fill" => (105, 105),
        k if k.starts_with("flood_fill_") || k == "shape_mapper" => (106, 106),
        "dust" | "curvature_smooth" | "normal_combine" | "normal_invert" | "normal_normalize" | "normal_vector_rotation" | "normal_to_height" | "normal_facing" | "normal_transform" | "normal_sobel" => (107, 107),
        "spec_gloss_to_metal_rough" => (108, 100),
        "palette_apply" => (108, 108),
        "height_normal_blend" => (107, 100),
        "vector_warp" => (100, 107),
        "chrominance_extract" => (108, 108),
        _ => (100, 101),
    };
    let mut line = format!("node 1 {kind}");
    match kind {
        "pixel_processor" => line += " expr=\"sin(u*18.849556)*0.5+0.5\"",
        "text" => line += " text=\"Ab\" size=0.5",
        "input_value" => line += " name=\"level\" type=0 v=0.5",
        "colorize" => line += " p0=0 r0=0.1 g0=0.1 b0=0.35 p1=1 r1=1 g1=0.8 b1=0.3",
        "merge" => line += " r=@100 g=@101 b=@100",
        "alpha_merge" => line += " alpha=@101",
        "color_to_mask" => line += " r=0.5 g=0.5 b=0.5 range=0.12 softness=0.2",
        "replace_color" => line += " src_r=0.5 src_g=0.5 src_b=0.5 range=0.2",
        "switch" => line += " a=@100 b=@101 index=1",
        "multi_switch" => line += " a=@100 b=@101 index=1",
        "material_blend" => line += " bg_base=@108 fg_base=@100 bg_height=@100 fg_height=@101 mask=@101 bg_normal=@107",
        _ => {}
    }
    for (i, slot) in sp.req.iter().enumerate() {
        line += &format!(" {slot}=@{}", if i % 2 == 0 { a } else { b });
    }
    t += &line;
    t += "\noutput o 1\n";
    Some(t)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Node {
    pub kind: String,
    pub nums: BTreeMap<String, f32>,
    pub strs: BTreeMap<String, String>,
    pub ins: BTreeMap<String, u32>,
    /// For links coming from a graph (subgraph) node: which of its outputs the slot reads. Absent = default output.
    pub in_out: BTreeMap<String, String>,
    pub pos: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct Comment {
    pub text: String,
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub color: u8,
}

#[derive(Debug, Clone)]
pub struct Exposed {
    pub label: String,
    pub node: u32,
    pub param: String,
    pub min: f32,
    pub max: f32,
}

/// Version of the line text of a graph (`Graph::parse` / `Graph::to_text`, the form tests and tools write graphs in).
/// Text without a `format` line is version 1; newer text is refused with a clear message instead of being
/// half-understood.
pub const FORMAT_VERSION: u32 = 2;

#[derive(Debug, Clone)]
pub struct Graph {
    /// format version of the line text it was parsed from (1 when it had no `format` line)
    pub format: u32,
    pub size: usize,
    /// pixel height at the nominal size; 0 means square (same as `size`)
    pub height: usize,
    pub nodes: BTreeMap<u32, Node>,
    pub outputs: Vec<(String, u32)>,
    /// `output name id.pin` in a file: which named output of the node feeds the graph output (legacy output lines)
    pub output_pins: BTreeMap<String, String>,
    pub subgraphs: BTreeMap<String, Graph>,
    pub resources: BTreeMap<String, Arc<Vec<u8>>>,
    pub exposed: Vec<Exposed>,
    pub comments: Vec<Comment>,
    pub seed_offset: u32,
    pub meta: BTreeMap<String, String>,
    /// graphs of an `.sbs` package edited as it is (see `sbs_native`): where the graph lives in the file
    pub sd: Option<Box<crate::sbs_native::SdGraph>>,
}

impl Default for Graph {
    fn default() -> Self {
        Graph {
            format: FORMAT_VERSION,
            size: 512,
            height: 0,
            nodes: BTreeMap::new(),
            outputs: Vec::new(),
            output_pins: BTreeMap::new(),
            subgraphs: BTreeMap::new(),
            resources: BTreeMap::new(),
            exposed: Vec::new(),
            comments: Vec::new(),
            seed_offset: 0,
            meta: BTreeMap::new(),
            sd: None,
        }
    }
}

fn split_tokens(line: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut had_q = false;
    for c in line.chars() {
        match c {
            '"' => {
                in_q = !in_q;
                had_q = true;
            }
            c if c.is_whitespace() && !in_q => {
                if !cur.is_empty() || had_q {
                    out.push(std::mem::take(&mut cur));
                    had_q = false;
                }
            }
            c => cur.push(c),
        }
    }
    if in_q {
        return Err("unterminated quote".into());
    }
    if !cur.is_empty() || had_q {
        out.push(cur);
    }
    Ok(out)
}

// ---------- base64 (for embedded resources) ----------
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(d: &[u8]) -> String {
    let mut s = String::with_capacity(d.len() * 4 / 3 + 4);
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(B64[(n >> 18) as usize & 63] as char);
        s.push(B64[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    s
}

pub fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for ch in s.bytes() {
        if ch == b'=' {
            break;
        }
        let v = B64.iter().position(|&b| b == ch).ok_or_else(|| "invalid base64".to_string())? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

pub const THUMB: usize = 96;

pub struct EvalAll {
    pub thumbs: HashMap<u32, Vec<u8>>,
    pub errors: HashMap<u32, String>,
    pub kept: HashMap<u32, Arc<Tex>>,
    /// results asked for with `defer` in `evaluate_all_cached_ex`: still wherever they were made, read on demand
    pub pending: HashMap<u32, Pending>,
    /// output size of every evaluated node (differs from the graph size when a node has its own Output Size)
    pub sizes: HashMap<u32, usize>,
}

struct Ctx<'a> {
    size: usize,
    /// pixel height (equals `size` for a square graph)
    height: usize,
    /// the graph's own (nominal) size: absolute node sizes are scaled by size / nominal so quick previews stay proportional
    nominal: usize,
    lib: &'a BTreeMap<String, Graph>,
    res: &'a BTreeMap<String, Arc<Vec<u8>>>,
    so: u32,
    depth: usize,
    /// values of the Input Value nodes of the graph being evaluated (read by function graphs)
    gin: &'a BTreeMap<String, crate::func::Value>,
}

/// Most image inputs a graph used as a node takes.
pub const MAX_SUB_INPUTS: usize = 16;
/// Input slots of a graph node, in order of the graph's inputs (`idx`).
pub const SUB_INPUTS: [&str; MAX_SUB_INPUTS] = ["in0", "in1", "in2", "in3", "in4", "in5", "in6", "in7", "in8", "in9", "in10", "in11", "in12", "in13", "in14", "in15"];

/// Image-pattern inputs of the pattern program map, in order.
/// Image inputs of the Pixel and Value Processor (the format allows any number; 16 here).
pub const PROCESSOR_INPUTS: [&str; 16] = ["in", "in2", "in3", "in4", "in5", "in6", "in7", "in8", "in9", "in10", "in11", "in12", "in13", "in14", "in15", "in16"];
const FX_MAP_IMAGES: [&str; 8] = ["in", "in2", "in3", "in4", "in5", "in6", "in7", "in8"];

/// Node id used for the helper nodes of [`Graph::preview_host`] (far above any id an author would reach).
pub const PREVIEW_ID: u32 = 4_000_000_001;

impl Graph {
    /// The graph's normal maps are made in the DirectX convention (green pointing down): viewers flip the green
    /// channel of the normal output to light them correctly. Set with `meta normal_format "directx"`.
    pub fn normal_directx(&self) -> bool {
        if self.sd.is_some() {
            return crate::sbs_native::normal_directx(self);
        }
        self.meta.get("normal_format").map(|v| v.eq_ignore_ascii_case("directx")).unwrap_or(false)
    }

    /// The normal map as a viewer needs it (OpenGL convention): flipped when the graph makes DirectX normals.
    pub fn normal_for_view(&self, t: &Tex) -> Option<Tex> {
        self.normal_directx().then(|| crate::ops::flip_green(t))
    }

    /// "texture", "function" or "fxmap".
    pub fn graph_kind(&self) -> &str {
        match self.meta.get("kind").map(String::as_str) {
            Some("function") => "function",
            Some("fxmap") => "fxmap",
            _ => "texture",
        }
    }

    /// Function graphs and pattern programs are not textures. To look at one, wrap it in a texture graph that
    /// runs it: a pixel processor for a function, a pattern map for a program. Returns the wrapper and the id of the
    /// node that shows the result (`None` for ordinary graphs).
    pub fn preview_host(&self) -> Option<(Graph, u32)> {
        let kind = self.graph_kind();
        if kind == "texture" {
            return None;
        }
        if self.sd.is_some() {
            return crate::sbs_native::preview_host(self);
        }
        let mut host = Graph::default();
        host.size = self.size;
        host.height = self.height;
        host.seed_offset = self.seed_offset;
        host.resources = self.resources.clone();
        host.subgraphs = self.subgraphs.clone();
        let mut me = self.clone();
        me.subgraphs.clear();
        me.resources.clear();
        host.subgraphs.insert("__self".into(), me);
        let mut n = Node::default();
        if kind == "function" {
            n.kind = "pixel_processor".into();
            n.strs.insert("fn".into(), "__self".into());
        } else {
            n.kind = "fx_map".into();
            n.strs.insert("program".into(), "__self".into());
            n.nums.insert("color".into(), 0.0);
        }
        host.nodes.insert(PREVIEW_ID, n);
        let mut o = Node::default();
        o.kind = "output".into();
        o.strs.insert("usage".into(), "basecolor".into());
        o.ins.insert("in".into(), PREVIEW_ID);
        host.nodes.insert(PREVIEW_ID + 1, o);
        Some((host, PREVIEW_ID))
    }

    pub fn parse(text: &str) -> Result<Graph, Error> {
        let g = Graph::parse_unchecked(text)?;
        g.validate()?;
        Ok(g)
    }

    /// Parse without validating connections (used by the editor for work-in-progress graphs).
    pub fn parse_unchecked(text: &str) -> Result<Graph, Error> {
        let mut g = Graph::default();
        g.format = 1;
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let ln = i + 1;
            let raw = lines[i];
            i += 1;
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix("subgraph ") {
                let mut body = String::new();
                let mut closed = false;
                while i < lines.len() {
                    let l = lines[i];
                    i += 1;
                    if l.trim() == "end" {
                        closed = true;
                        break;
                    }
                    body.push_str(l);
                    body.push('\n');
                }
                if !closed {
                    return Err(Error::Parse(ln, "subgraph without matching 'end'".into()));
                }
                let sub = Graph::parse_unchecked(&body).map_err(|e| Error::Parse(ln, format!("in subgraph '{}': {e}", name.trim())))?;
                g.subgraphs.insert(name.trim().to_string(), sub);
                continue;
            }
            if let Some(rest) = line.strip_prefix("comment ") {
                let t: Vec<&str> = rest.split_whitespace().collect();
                if t.len() < 5 {
                    return Err(Error::Parse(ln, "usage: comment x y w h color text...".into()));
                }
                let f = |k: usize| t[k].parse::<f32>().map_err(|_| Error::Parse(ln, "bad comment number".into()));
                g.comments.push(Comment { text: t[5..].join(" "), pos: [f(0)?, f(1)?], size: [f(2)?.max(40.0), f(3)?.max(30.0)], color: t[4].parse().unwrap_or(0) });
                continue;
            }
            let t = split_tokens(line).map_err(|m| Error::Parse(ln, m))?;
            match t[0].as_str() {
                "format" => {
                    let v = t.get(1).and_then(|s| s.parse::<u32>().ok()).ok_or_else(|| Error::Parse(ln, "format needs an integer".into()))?;
                    if v > FORMAT_VERSION {
                        return Err(Error::Parse(ln, format!("this file uses format {v}, but this version of MaterialCraft reads up to format {FORMAT_VERSION}; please update")));
                    }
                    g.format = v;
                }
                "size" => {
                    g.size = t.get(1).and_then(|s| s.parse::<usize>().ok()).ok_or_else(|| Error::Parse(ln, "size needs an integer".into()))?;
                    g.height = match t.get(2) {
                        Some(h) => h.parse::<usize>().map_err(|_| Error::Parse(ln, "size height needs an integer".into()))?,
                        None => 0,
                    };
                }
                "seed_offset" => {
                    g.seed_offset = t.get(1).and_then(|s| s.parse::<u32>().ok()).ok_or_else(|| Error::Parse(ln, "seed_offset needs an integer".into()))?;
                }
                "meta" => {
                    if t.len() != 3 {
                        return Err(Error::Parse(ln, "usage: meta <key> \"value\"".into()));
                    }
                    g.meta.insert(t[1].clone(), t[2].clone());
                }
                "resource" => {
                    if t.len() != 3 {
                        return Err(Error::Parse(ln, "usage: resource <name> <base64>".into()));
                    }
                    g.resources.insert(t[1].clone(), Arc::new(b64_decode(&t[2]).map_err(|m| Error::Parse(ln, m))?));
                }
                "expose" => {
                    if t.len() != 6 {
                        return Err(Error::Parse(ln, "usage: expose <label> <node> <param> <min> <max>".into()));
                    }
                    let p = |s: &str| s.parse::<f32>().map_err(|_| Error::Parse(ln, "bad number".into()));
                    g.exposed.push(Exposed {
                        label: t[1].clone(),
                        node: t[2].parse().map_err(|_| Error::Parse(ln, "bad node id".into()))?,
                        param: t[3].clone(),
                        min: p(&t[4])?,
                        max: p(&t[5])?,
                    });
                }
                "output" => {
                    if t.len() != 3 {
                        return Err(Error::Parse(ln, "usage: output <name> <node-id>".into()));
                    }
                    let (idtxt, pin) = match t[2].split_once('.') {
                        Some((a, b)) => (a, Some(b.to_string())),
                        None => (t[2].as_str(), None),
                    };
                    let id = idtxt.parse::<u32>().map_err(|_| Error::Parse(ln, "bad node id".into()))?;
                    g.outputs.push((t[1].clone(), id));
                    if let Some(pin) = pin {
                        g.output_pins.insert(t[1].clone(), pin);
                    }
                }
                "node" => {
                    if t.len() < 3 {
                        return Err(Error::Parse(ln, "usage: node <id> <type> [key=value ...]".into()));
                    }
                    let id = t[1].parse::<u32>().map_err(|_| Error::Parse(ln, "bad node id".into()))?;
                    if g.nodes.contains_key(&id) {
                        return Err(Error::Parse(ln, format!("duplicate node id {id}")));
                    }
                    let mut n = Node { kind: t[2].clone(), ..Default::default() };
                    for kv in &t[3..] {
                        let (k, v) = kv.split_once('=').ok_or_else(|| Error::Parse(ln, format!("expected key=value, got '{kv}'")))?;
                        if let Some(r) = v.strip_prefix('@') {
                            let (idpart, outpart) = match r.split_once('.') {
                                Some((a, b)) => (a, Some(b)),
                                None => (r, None),
                            };
                            n.ins.insert(k.to_string(), idpart.parse::<u32>().map_err(|_| Error::Parse(ln, format!("bad reference '{v}'")))?);
                            if let Some(o) = outpart.filter(|o| !o.is_empty()) {
                                n.in_out.insert(k.to_string(), o.to_string());
                            }
                        } else if let Ok(x) = v.parse::<f32>() {
                            match k {
                                "_x" => n.pos[0] = x,
                                "_y" => n.pos[1] = x,
                                _ => {
                                    n.nums.insert(k.to_string(), x);
                                }
                            }
                        } else {
                            n.strs.insert(k.to_string(), v.to_string());
                        }
                    }
                    g.nodes.insert(id, n);
                }
                other => return Err(Error::Parse(ln, format!("unknown directive '{other}'"))),
            }
        }
        // `output name id.pin` needs a real Output node to carry the pin
        if !g.output_pins.is_empty() {
            g.migrate_outputs();
        }
        Ok(g)
    }

    /// All outputs: (usage, identifier, node id). Includes legacy `output` lines.
    pub fn output_nodes(&self) -> Vec<(String, String, u32)> {
        let mut v = Vec::new();
        for (id, n) in &self.nodes {
            if n.kind == "sbs:output" {
                // a graph output of an .sbs package: named and used as the graph declares it
                let uid = n.strs.get("#output").cloned().unwrap_or_default();
                let sd = self.sd.as_ref();
                let ident = sd.and_then(|s| s.output_identifier(&uid)).unwrap_or_else(|| "output".into());
                let usage = sd.and_then(|s| s.output_usage(&uid)).map(|u| crate::sbs_import::usage(&u)).unwrap_or_else(|| crate::sbs_import::usage(&ident));
                v.push((usage.to_string(), crate::sbs_import::san(&ident), *id));
                continue;
            }
            if n.kind == "output" {
                let usage = n.strs.get("usage").cloned().unwrap_or_else(|| "basecolor".into());
                let ident = n.strs.get("identifier").filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| usage.clone());
                v.push((usage, ident, *id));
            }
        }
        for (name, id) in &self.outputs {
            v.push((name.clone(), name.clone(), *id));
        }
        v
    }

    /// Names of the outputs in their canonical order (base color, normal, roughness, ...).
    pub fn output_idents(&self) -> Vec<String> {
        let mut v: Vec<(usize, String)> = self.output_nodes().into_iter().map(|(u, i, _)| (OUTPUT_USAGES.iter().position(|x| *x == u).unwrap_or(99), i)).collect();
        v.sort();
        v.dedup();
        v.into_iter().map(|x| x.1).collect()
    }

    /// Convert legacy `output` assignments into real Output nodes.
    pub fn migrate_outputs(&mut self) {
        let legacy = std::mem::take(&mut self.outputs);
        let pins = std::mem::take(&mut self.output_pins);
        let mut next = self.nodes.keys().max().copied().unwrap_or(0) + 1;
        for (name, id) in legacy {
            let pos = self.nodes.get(&id).map(|n| [n.pos[0] + 150.0, n.pos[1]]).unwrap_or([0.0, 0.0]);
            let mut node = Node { kind: "output".into(), pos, ..Default::default() };
            node.strs.insert("usage".into(), name.clone());
            node.ins.insert("in".into(), id);
            if let Some(pin) = pins.get(&name) {
                node.in_out.insert("in".into(), pin.clone());
            }
            self.nodes.insert(next, node);
            next += 1;
        }
    }

    /// Precision every node of this graph is quantised to unless it sets its own Output Format:
    /// 0 = 32-bit float (default, nothing is rounded), 1 = 8 bit, 2 = 16 bit, 3 = 16-bit float.
    pub fn default_format(&self) -> u32 {
        self.meta.get("ofmt").and_then(|s| s.parse::<u32>().ok()).filter(|v| *v <= 3).unwrap_or(0)
    }

    /// true when width and height are the same
    pub fn is_square(&self) -> bool {
        self.height == 0 || self.height == self.size
    }

    /// Height in pixels of an evaluation whose width is `size` (keeps the graph's aspect ratio).
    pub fn height_for(&self, size: usize) -> usize {
        if self.is_square() {
            size
        } else {
            ((size as f64 * self.height as f64 / self.size.max(1) as f64).round() as usize).clamp(1, 8192)
        }
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.size == 0 || self.size > 8192 {
            return Err(Error::BadSize(self.size));
        }
        if self.sd.is_some() {
            // an .sbs graph: unconnected inputs read black there, so only what stops rendering counts
            if self.output_nodes().is_empty() {
                return Err(Error::NoOutputs);
            }
            return self.check_cycles();
        }
        if self.height > 8192 {
            return Err(Error::BadSize(self.height));
        }
        if self.output_nodes().is_empty() {
            return Err(Error::NoOutputs);
        }
        self.validate_nodes()?;
        for (_, name, id) in &self.output_nodes() {
            if !self.nodes.contains_key(id) {
                return Err(Error::MissingNode(0, *id));
            }
            if name.is_empty() || name.contains(['/', '\\', '.']) {
                return Err(Error::Parse(0, format!("invalid output name '{name}'")));
            }
        }
        self.check_cycles()
    }

    fn validate_nodes(&self) -> Result<(), Error> {
        for (id, n) in &self.nodes {
            let sp = spec(&n.kind).ok_or_else(|| Error::UnknownNode(*id, n.kind.clone()))?;
            for r in sp.req {
                if !n.ins.contains_key(*r) {
                    return Err(Error::MissingInput(*id, r.to_string()));
                }
            }
            for (slot, rid) in &n.ins {
                if !sp.req.contains(&slot.as_str()) && !sp.opt.contains(&slot.as_str()) {
                    return Err(Error::UnknownSlot(*id, slot.clone()));
                }
                if !self.nodes.contains_key(rid) {
                    return Err(Error::MissingNode(*id, *rid));
                }
                if let (Some(o), Some(src)) = (n.in_out.get(slot), self.nodes.get(rid)) {
                    let outs = node_outputs(&src.kind);
                    if !outs.is_empty() && !outs.contains(&o.as_str()) {
                        return Err(Error::BadParam(*id, format!("{slot}: node {rid} ({}) has no output '{o}' (outputs: {})", src.kind, outs.join(", "))));
                    }
                }
            }
            if n.kind == "pixel_processor" {
                if !n.strs.contains_key("expr") && !n.strs.contains_key("fn") {
                    return Err(Error::BadParam(*id, "pixel_processor needs expr=\"...\" or fn=\"function\"".into()));
                }
                for (k, v) in &n.strs {
                    if k.starts_with("expr") {
                        expr::parse(v).map_err(|m| Error::BadParam(*id, format!("{k}: {m}")))?;
                    }
                }
            }
            if n.kind == "subgraph" {
                match n.strs.get("graph") {
                    Some(name) if self.subgraphs.contains_key(name) => {}
                    Some(name) => return Err(Error::BadParam(*id, format!("unknown subgraph '{name}'"))),
                    None => return Err(Error::BadParam(*id, "subgraph node needs graph=\"name\"".into())),
                }
            }
            if n.kind == "baked_map" {
                let set = n.strs.get("set").map(|v| v.trim()).unwrap_or("");
                if set.is_empty() || set.contains(['/', '\\']) {
                    return Err(Error::BadParam(*id, "baked_map needs set=\"name\" (a bake set of the package)".into()));
                }
                match n.strs.get("map").and_then(|m| crate::bake::MapKind::from_id(m)) {
                    Some(_) => {}
                    None => return Err(Error::BadParam(*id, "baked_map needs map=\"normal\" (one of the baked map names)".into())),
                }
            }
            if n.kind == "bitmap" && !n.strs.contains_key("file") {
                return Err(Error::BadParam(*id, "bitmap needs file=\"path.png\"".into()));
            }
            if n.kind == "color_lut" && n.strs.get("file").map(|f| f.is_empty()).unwrap_or(true) {
                return Err(Error::BadParam(*id, "color_lut needs file=\"grade.cube\"".into()));
            }
        }
        Ok(())
    }

    /// Cycle detection (iterative DFS, 0=unseen 1=active 2=done).
    pub fn check_cycles(&self) -> Result<(), Error> {
        let mut state: HashMap<u32, u8> = HashMap::new();
        for &start in self.nodes.keys() {
            if state.get(&start).copied().unwrap_or(0) != 0 {
                continue;
            }
            let mut stack: Vec<(u32, Vec<u32>)> = vec![(start, self.nodes[&start].ins.values().copied().collect())];
            state.insert(start, 1);
            while let Some((id, deps)) = stack.last_mut() {
                if let Some(d) = deps.pop() {
                    if !self.nodes.contains_key(&d) {
                        continue;
                    }
                    match state.get(&d).copied().unwrap_or(0) {
                        1 => return Err(Error::Cycle(d)),
                        0 => {
                            state.insert(d, 1);
                            let nd: Vec<u32> = self.nodes[&d].ins.values().copied().collect();
                            stack.push((d, nd));
                        }
                        _ => {}
                    }
                } else {
                    state.insert(*id, 2);
                    stack.pop();
                }
            }
        }
        Ok(())
    }

    pub fn to_text(&self) -> String {
        let mut s = format!("format {FORMAT_VERSION}\n");
        s.push_str(&if self.is_square() { format!("size {}\n", self.size) } else { format!("size {} {}\n", self.size, self.height) });
        if self.seed_offset != 0 {
            s.push_str(&format!("seed_offset {}\n", self.seed_offset));
        }
        for (k, v) in &self.meta {
            if !v.is_empty() {
                s.push_str(&format!("meta {k} \"{}\"\n", v.replace('"', "").replace('#', "").replace('\n', " ")));
            }
        }
        for (k, v) in &self.resources {
            s.push_str(&format!("resource {k} {}\n", b64_encode(v)));
        }
        for (name, sub) in &self.subgraphs {
            s.push_str(&format!("subgraph {name}\n{}end\n", sub.to_text()));
        }
        for (id, n) in &self.nodes {
            s.push_str(&format!("node {id} {}", n.kind));
            if n.pos != [0.0, 0.0] {
                s.push_str(&format!(" _x={} _y={}", n.pos[0], n.pos[1]));
            }
            for (k, v) in &n.nums {
                s.push_str(&format!(" {k}={v}"));
            }
            for (k, v) in &n.strs {
                s.push_str(&format!(" {k}=\"{}\"", v.replace('"', "").replace('#', "")));
            }
            for (k, v) in &n.ins {
                match n.in_out.get(k).filter(|o| !o.is_empty() && !o.contains(|c: char| c.is_whitespace() || matches!(c, '"' | '.' | '#' | '='))) {
                    Some(o) => s.push_str(&format!(" {k}=@{v}.{o}")),
                    None => s.push_str(&format!(" {k}=@{v}")),
                }
            }
            s.push('\n');
        }
        for (name, id) in &self.outputs {
            s.push_str(&format!("output {name} {id}\n"));
        }
        for e in &self.exposed {
            s.push_str(&format!("expose \"{}\" {} {} {} {}\n", e.label.replace('"', ""), e.node, e.param, e.min, e.max));
        }
        for c in &self.comments {
            s.push_str(&format!("comment {} {} {} {} {} {}\n", c.pos[0], c.pos[1], c.size[0], c.size[1], c.color, c.text.replace('#', "").replace('\n', " ")));
        }
        s
    }

    /// Apply exposed-parameter overrides by label.
    pub fn apply_params(&mut self, p: &BTreeMap<String, f32>) -> Result<(), String> {
        for (label, v) in p {
            let e = self.exposed.iter().find(|e| &e.label == label).cloned().ok_or_else(|| format!("no exposed parameter '{label}'"))?;
            let n = self.nodes.get_mut(&e.node).ok_or_else(|| format!("exposed node {} missing", e.node))?;
            n.nums.insert(e.param.clone(), v.clamp(e.min.min(e.max), e.min.max(e.max)));
        }
        Ok(())
    }

    /// Import a whole (already made) graph as a reusable subgraph named `name`, so it can be placed as a node in this
    /// graph. Its own subgraphs are merged under a `name__` prefix and its embedded resources are carried over.
    pub fn import_graph(&mut self, mut other: Graph, name: &str) -> Result<(), String> {
        if self.subgraphs.contains_key(name) {
            return Err(format!("subgraph '{name}' already exists"));
        }
        if other.nodes.is_empty() {
            return Err("the graph is empty".into());
        }
        if other.output_nodes().is_empty() {
            return Err("the graph has no Output node to use as the node's result".into());
        }
        other.check_cycles().map_err(|e| e.to_string())?;
        let mut inner = std::mem::take(&mut other.subgraphs);
        let renamed: Vec<String> = inner.keys().cloned().collect();
        for g in std::iter::once(&mut other).chain(inner.values_mut()) {
            for n in g.nodes.values_mut() {
                if n.kind == "subgraph" {
                    if let Some(r) = n.strs.get_mut("graph") {
                        if renamed.contains(r) {
                            *r = format!("{name}__{r}");
                        }
                    }
                }
            }
        }
        for (k, g) in inner {
            self.subgraphs.insert(format!("{name}__{k}"), g);
        }
        for (k, v) in std::mem::take(&mut other.resources) {
            self.resources.entry(k).or_insert(v);
        }
        self.subgraphs.insert(name.to_string(), other);
        Ok(())
    }

    /// Replace the selected nodes with a subgraph instance. Returns the new node id.
    pub fn extract_subgraph(&mut self, sel: &BTreeSet<u32>, name: &str) -> Result<u32, String> {
        let sel: BTreeSet<u32> = sel.iter().copied().filter(|i| self.nodes.contains_key(i)).collect();
        if sel.is_empty() {
            return Err("select at least one node".into());
        }
        if self.subgraphs.contains_key(name) {
            return Err(format!("subgraph '{name}' already exists"));
        }
        if sel.iter().any(|i| self.nodes[i].kind == "output") {
            return Err("output nodes cannot be placed inside a subgraph".into());
        }
        let mut sub = Graph { size: self.size, height: self.height, ..Default::default() };
        let mut map: HashMap<u32, u32> = HashMap::new();
        let mut next = 1u32;
        for id in &sel {
            map.insert(*id, next);
            next += 1;
        }
        let mut ext: Vec<u32> = Vec::new();
        let mut input_ids: HashMap<u32, u32> = HashMap::new();
        for id in &sel {
            let n = &self.nodes[id];
            let mut nn = n.clone();
            nn.ins.clear();
            for (slot, src) in &n.ins {
                if let Some(m) = map.get(src) {
                    nn.ins.insert(slot.clone(), *m);
                } else {
                    let iid = if let Some(x) = input_ids.get(src) {
                        *x
                    } else {
                        if ext.len() >= MAX_SUB_INPUTS {
                            return Err(format!("a subgraph supports at most {MAX_SUB_INPUTS} external inputs"));
                        }
                        let idx = ext.len();
                        ext.push(*src);
                        let iid = next;
                        next += 1;
                        let mut inode = Node { kind: "input".into(), ..Default::default() };
                        inode.nums.insert("idx".into(), idx as f32);
                        inode.pos = [n.pos[0] - 150.0, n.pos[1]];
                        sub.nodes.insert(iid, inode);
                        input_ids.insert(*src, iid);
                        iid
                    };
                    nn.ins.insert(slot.clone(), iid);
                }
            }
            sub.nodes.insert(map[id], nn);
        }
        let mut outs: Vec<u32> = Vec::new();
        for (oid, on) in &self.nodes {
            if sel.contains(oid) {
                continue;
            }
            for src in on.ins.values() {
                if sel.contains(src) && !outs.contains(src) {
                    outs.push(*src);
                }
            }
        }
        for (_, s) in &self.outputs {
            if sel.contains(s) && !outs.contains(s) {
                outs.push(*s);
            }
        }
        if outs.len() > 1 {
            return Err(format!("the selection has {} outputs used elsewhere; a subgraph supports exactly one", outs.len()));
        }
        let out_id = match outs.first() {
            Some(x) => *x,
            None => {
                let consumed: BTreeSet<u32> = sel.iter().flat_map(|i| self.nodes[i].ins.values().copied()).collect();
                *sel.iter().rev().find(|i| !consumed.contains(i)).unwrap_or_else(|| sel.iter().next_back().unwrap_or(&0))
            }
        };
        {
            let onode_id = sub.nodes.keys().max().copied().unwrap_or(0) + 1;
            let opos = sub.nodes.get(&map[&out_id]).map(|n| [n.pos[0] + 150.0, n.pos[1]]).unwrap_or([0.0, 0.0]);
            let mut o = Node { kind: "output".into(), pos: opos, ..Default::default() };
            o.strs.insert("usage".into(), "out".into());
            o.ins.insert("in".into(), map[&out_id]);
            sub.nodes.insert(onode_id, o);
        }
        let (mut cx, mut cy) = (0.0, 0.0);
        for id in &sel {
            cx += self.nodes[id].pos[0];
            cy += self.nodes[id].pos[1];
        }
        let new_id = self.nodes.keys().max().copied().unwrap_or(0) + 1;
        for id in &sel {
            self.nodes.remove(id);
        }
        let mut inst = Node { kind: "subgraph".into(), pos: [cx / sel.len() as f32, cy / sel.len() as f32], ..Default::default() };
        inst.strs.insert("graph".into(), name.to_string());
        for (i, e) in ext.iter().enumerate() {
            inst.ins.insert(format!("in{i}"), *e);
        }
        self.nodes.insert(new_id, inst);
        for n in self.nodes.values_mut() {
            for v in n.ins.values_mut() {
                if *v == out_id {
                    *v = new_id;
                }
            }
        }
        self.outputs.retain(|(_, s)| !sel.contains(s) || *s == out_id);
        for (_, s) in self.outputs.iter_mut() {
            if *s == out_id {
                *s = new_id;
            }
        }
        self.subgraphs.insert(name.to_string(), sub);
        Ok(new_id)
    }

    // ---------------- evaluation ----------------

    /// An `.sbs` graph as MaterialCraft nodes (see `sbs_native::lower`); `None` for ordinary graphs.
    fn lowered(&self) -> Result<Option<Graph>, Error> {
        match &self.sd {
            Some(sd) if sd.role == crate::sbs_native::Role::Graph => crate::sbs_native::lower(self).map(|l| Some(l.graph)).map_err(|e| Error::Parse(0, e)),
            _ => Ok(None),
        }
    }
    pub fn evaluate(&self, size: Option<usize>) -> Result<BTreeMap<String, Tex>, Error> {
        if let Some(low) = self.lowered()? {
            return low.evaluate(size.or(Some(self.size)));
        }
        self.validate()?;
        let size = size.unwrap_or(self.size);
        if size == 0 || size > 8192 {
            return Err(Error::BadSize(size));
        }
        let gin = crate::func::graph_inputs(self);
        let ctx = Ctx { size, height: self.height_for(size), nominal: self.size, lib: &self.subgraphs, res: &self.resources, so: self.seed_offset, depth: 0, gin: &gin };
        let outs = self.output_nodes();
        let keep: Vec<u32> = outs.iter().map(|o| o.2).collect();
        let mut cache = Cache::for_graph(self, &keep, false);
        let mut res = BTreeMap::new();
        clear_siblings();
        for (_, name, id) in &outs {
            let t = eval_node(self, *id, &ctx, &[], &mut cache);
            let t = match t {
                Ok(t) => t,
                Err(e) => {
                    clear_siblings();
                    return Err(e);
                }
            };
            res.insert(name.clone(), t);
        }
        clear_siblings();
        // hand the textures over without copying when no other output shares them
        cache.map.clear();
        Ok(res.into_iter().map(|(k, v)| (k, Rc::try_unwrap(v).unwrap_or_else(|rc| (*rc).clone()))).collect())
    }

    /// Evaluate a single node (editor preview). Ignores unrelated incomplete nodes.
    pub fn evaluate_node(&self, id: u32, size: usize) -> Result<Tex, Error> {
        if let Some(low) = self.lowered()? {
            return low.evaluate_node(id, size);
        }
        if size == 0 || size > 8192 {
            return Err(Error::BadSize(size));
        }
        self.check_cycles()?;
        let gin = crate::func::graph_inputs(self);
        let ctx = Ctx { size, height: self.height_for(size), nominal: self.size, lib: &self.subgraphs, res: &self.resources, so: self.seed_offset, depth: 0, gin: &gin };
        let mut cache = Cache::for_graph(self, &[id], false);
        clear_siblings();
        let t = eval_node(self, id, &ctx, &[], &mut cache);
        clear_siblings();
        let t = t?;
        cache.map.clear();
        Ok(Rc::try_unwrap(t).unwrap_or_else(|rc| (*rc).clone()))
    }

    /// Evaluate every node once (shared cache). Produces thumbnails, per-node errors and
    /// full-resolution textures for the ids in `keep`. Result is Send, so it can run on a worker thread.
    /// Like `evaluate_all`, but as editor job `job`: once `tex::CURRENT_JOB` moves on to a newer id this run stops
    /// within a few rows and its (partial) result must be thrown away.
    pub fn evaluate_all_cancellable(&self, size: usize, keep: &[u32], job: u64) -> EvalAll {
        crate::tex::set_job(job);
        let r = self.evaluate_all(size, keep);
        crate::tex::set_job(0);
        r
    }

    /// Editor evaluation: reuses every node result whose settings and inputs are unchanged (`cache` outlives the call),
    /// evaluates `first` (the node shown in the 2D view) before anything else and hands it to `on_first` at once.
    /// `full`: evaluate every node and take thumbnails (the idle pass). When false, only the nodes that feed `first`
    /// and `keep` are evaluated and no thumbnails are taken — the cheap pass run while a value is being dragged, so a
    /// large graph's off-screen branches and disconnected nodes cost nothing until the input settles.
    pub fn evaluate_all_cached(&self, size: usize, keep: &[u32], job: u64, cache: &Arc<NodeCache>, first: Option<u32>, full: bool, on_first: &mut dyn FnMut(&Pending)) -> EvalAll {
        self.evaluate_all_cached_ex(size, keep, &[], job, cache, first, full, on_first)
    }

    /// `evaluate_all_cached` where the nodes in `defer` (a subset of `keep`) are not read back to the host: they come
    /// back in `EvalAll::pending`, and whoever needs their pixels (or their device pictures) asks then. The editor
    /// defers the material outputs so the 3D view can take them packed straight from the device.
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_all_cached_ex(&self, size: usize, keep: &[u32], defer: &[u32], job: u64, cache: &Arc<NodeCache>, first: Option<u32>, full: bool, on_first: &mut dyn FnMut(&Pending)) -> EvalAll {
        crate::tex::set_job(job);
        let r = self.evaluate_all_impl(size, keep, defer, Some(cache.clone()), first, full, on_first);
        crate::tex::set_job(0);
        r
    }

    pub fn evaluate_all(&self, size: usize, keep: &[u32]) -> EvalAll {
        self.evaluate_all_impl(size, keep, &[], None, None, true, &mut |_| {})
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_all_impl(&self, size: usize, keep: &[u32], defer: &[u32], persist: Option<Arc<NodeCache>>, first: Option<u32>, full: bool, on_first: &mut dyn FnMut(&Pending)) -> EvalAll {
        let mut r = EvalAll { thumbs: HashMap::new(), errors: HashMap::new(), kept: HashMap::new(), pending: HashMap::new(), sizes: HashMap::new() };
        match self.lowered() {
            Ok(Some(low)) => return low.evaluate_all_impl(size, keep, defer, persist, first, full, on_first),
            Ok(None) => {}
            Err(e) => {
                r.errors.insert(0, e.to_string());
                return r;
            }
        }
        if size == 0 || size > 8192 {
            r.errors.insert(0, Error::BadSize(size).to_string());
            return r;
        }
        if let Err(e) = self.check_cycles() {
            r.errors.insert(0, e.to_string());
            return r;
        }
        let gin = crate::func::graph_inputs(self);
        let ctx = Ctx { size, height: self.height_for(size), nominal: self.size, lib: &self.subgraphs, res: &self.resources, so: self.seed_offset, depth: 0, gin: &gin };
        // thumbnails are taken the moment a node finishes, so intermediate textures can be freed as soon as every
        // node that reads them is done (a big graph at 4K would not fit in memory otherwise)
        let mut cache = Cache::for_graph(self, keep, full);
        if let Some(p) = persist {
            cache.hashes = hash_graph(self, &ctx);
            cache.persist = Some(p);
        }
        // the 2D-view node first, then the outputs (3D view), then everything else (thumbnails on the full pass only)
        let mut order: Vec<u32> = Vec::new();
        order.extend(first.filter(|f| self.nodes.contains_key(f)));
        order.extend(keep.iter().copied().filter(|k| self.nodes.contains_key(k)));
        if full {
            order.extend(self.nodes.keys().copied());
        }
        let mut seen = HashSet::new();
        order.retain(|i| seen.insert(*i));
        clear_siblings();
        let mut kept: Vec<(u32, Rc<Lazy>)> = Vec::new();
        for id in order {
            if Some(id) != first && !keep.contains(&id) {
                // already evaluated (or taken from the cache) as another node's input during this pass
                if cache.sizes.contains_key(&id) {
                    continue;
                }
                // unchanged since an earlier pass: only its card is shown, so take the thumbnail without the pixels
                if let (Some(p), Some(h)) = (&cache.persist, cache.hashes.get(&id)) {
                    if let Some((th, sz)) = p.peek(*h) {
                        // no thumbnail yet and a pass that shows the cards: evaluate (a cache hit) to take it
                        if !th.is_empty() || cache.thumbs.is_none() {
                            cache.sizes.insert(id, sz);
                            if let Some(tm) = cache.thumbs.as_mut() {
                                tm.insert(id, th);
                            }
                            continue;
                        }
                    }
                }
            }
            match eval_res(self, id, None, &ctx, &[], &mut cache) {
                Ok(l) => {
                    if Some(id) == first {
                        // a picture that is on the device stays there: the viewer packs it there (`Pending::view`)
                        if l.gpu.get().is_some() {
                            on_first(&Pending(l.clone()));
                        } else {
                            match l.tex() {
                                Ok(_) => on_first(&Pending(l.clone())),
                                Err(e) => {
                                    r.errors.insert(id, e.to_string());
                                }
                            }
                        }
                    }
                    if keep.contains(&id) {
                        kept.push((id, l));
                    }
                }
                Err(Error::Cancelled) => break,
                Err(e) => {
                    r.errors.insert(id, e.to_string());
                }
            }
        }
        // the outputs the editor shows: every one still on the device comes back in one batch (the deferred ones stay)
        let (later, kept): (Vec<_>, Vec<_>) = kept.into_iter().partition(|(id, _)| defer.contains(id));
        for (id, l) in later {
            r.pending.insert(id, Pending(l));
        }
        read_back(&kept);
        for (id, l) in kept {
            match l.tex() {
                Ok(t) => {
                    r.kept.insert(id, t);
                }
                Err(e) => {
                    r.errors.insert(id, e.to_string());
                }
            }
        }
        clear_siblings();
        r.thumbs = cache.thumbs.take().unwrap_or_default();
        r.sizes = std::mem::take(&mut cache.sizes);
        if let Some(p) = &cache.persist {
            p.trim();
        }
        r
    }
}

/// Node results kept between editor evaluations, found by a hash of everything a result depends on.
/// Least recently used entries are dropped when the memory budget is exceeded.
pub struct NodeCache {
    inner: Mutex<NcInner>,
    budget: usize,
    gbudget: usize,
    hits: std::sync::atomic::AtomicUsize,
    misses: std::sync::atomic::AtomicUsize,
}

struct NcInner {
    map: HashMap<u64, NcEntry>,
    /// host memory held by the entries' pixels
    bytes: usize,
    /// device memory held by the entries' pictures
    gbytes: usize,
    tick: u64,
}

struct NcEntry {
    /// the pixels on the host (absent for a result that only exists on the device)
    data: Option<NcData>,
    /// the same picture in device memory, when a GPU node made it or a GPU node had to read it
    gpu: Option<crate::gpu::GTex>,
    thumb: Vec<u8>,
    size: usize,
    stamp: u64,
    /// the full texture handed out last time: while anyone still holds it (the editor keeps the outputs and the 2D
    /// view), a hit returns it without expanding the one-channel copy again or reading the device picture back
    hot: std::sync::Weak<Tex>,
}

/// Grayscale textures (most of a typical graph) are stored with one channel: a quarter of the memory.
#[derive(Clone)]
enum NcData {
    Gray(usize, usize, Arc<Vec<f32>>),
    Rgba(Arc<Tex>),
}

impl NcData {
    fn pack(t: &Arc<Tex>) -> NcData {
        if t.px.iter().all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 1.0) {
            NcData::Gray(t.w, t.h, Arc::new(t.px.iter().map(|p| p[0]).collect()))
        } else {
            NcData::Rgba(t.clone())
        }
    }
    fn unpack(&self) -> Arc<Tex> {
        match self {
            NcData::Gray(w, h, v) => {
                let mut px = vec![[0.0f32, 0.0, 0.0, 1.0]; v.len()];
                let per = 1 << 16;
                crate::pool::for_chunks(&mut px, per, &|ci, dst| {
                    for (d, s) in dst.iter_mut().zip(&v[ci * per..]) {
                        *d = [*s, *s, *s, 1.0];
                    }
                });
                Arc::new(Tex { w: *w, h: *h, px })
            }
            NcData::Rgba(t) => t.clone(),
        }
    }
    fn bytes(&self) -> usize {
        match self {
            NcData::Gray(_, _, v) => v.len() * 4,
            NcData::Rgba(t) => t.px.len() * 16,
        }
    }
    fn dims(&self) -> (usize, usize) {
        match self {
            NcData::Gray(w, h, _) => (*w, *h),
            NcData::Rgba(t) => (t.w, t.h),
        }
    }
}

/// What a cache hit hands out: whichever forms of the picture exist, nothing converted yet.
struct NcHit {
    hot: Option<Arc<Tex>>,
    data: Option<NcData>,
    gpu: Option<crate::gpu::GTex>,
    thumb: Vec<u8>,
    size: usize,
}

impl NodeCache {
    pub fn new(budget_mb: usize) -> NodeCache {
        let gmb = std::env::var("MC_GPU_CACHE_MB").ok().and_then(|s| s.parse().ok()).unwrap_or(1536usize);
        NodeCache {
            inner: Mutex::new(NcInner { map: HashMap::new(), bytes: 0, gbytes: 0, tick: 0 }),
            budget: budget_mb.max(16) << 20,
            gbudget: gmb.max(16) << 20,
            hits: Default::default(),
            misses: Default::default(),
        }
    }

    /// Budget from the MC_CACHE_MB environment variable (default 3 GB); device pictures have their own budget
    /// (MC_GPU_CACHE_MB, default 1.5 GB).
    pub fn with_default_budget() -> NodeCache {
        let mb = std::env::var("MC_CACHE_MB").ok().and_then(|s| s.parse().ok()).unwrap_or(3072);
        NodeCache::new(mb)
    }

    fn get(&self, h: u64) -> Option<NcHit> {
        let mut g = self.inner.lock().ok()?;
        g.tick += 1;
        let tick = g.tick;
        use std::sync::atomic::Ordering::Relaxed;
        match g.map.get_mut(&h) {
            Some(e) => {
                e.stamp = tick;
                self.hits.fetch_add(1, Relaxed);
                Some(NcHit { hot: e.hot.upgrade(), data: e.data.clone(), gpu: e.gpu.clone(), thumb: e.thumb.clone(), size: e.size })
            }
            None => {
                self.misses.fetch_add(1, Relaxed);
                None
            }
        }
    }

    fn contains(&self, h: u64) -> bool {
        self.inner.lock().map(|g| g.map.contains_key(&h)).unwrap_or(false)
    }

    /// Thumbnail and size of a cached result, without touching its pixels (for nodes a pass only shows as a card).
    fn peek(&self, h: u64) -> Option<(Vec<u8>, usize)> {
        let mut g = self.inner.lock().ok()?;
        g.tick += 1;
        let tick = g.tick;
        let e = g.map.get_mut(&h)?;
        e.stamp = tick;
        self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Some((e.thumb.clone(), e.size))
    }

    fn put(&self, h: u64, tex: Arc<Tex>, thumb: Vec<u8>, size: usize) {
        if let Ok(mut g) = self.inner.lock() {
            g.tick += 1;
            let tick = g.tick;
            let data = NcData::pack(&tex);
            let bytes = data.bytes();
            let hot = Arc::downgrade(&tex);
            // the same hash is the same picture: a device copy made earlier stays valid
            let gpu = match g.map.remove(&h) {
                Some(old) => {
                    g.bytes -= old.data.as_ref().map_or(0, |d| d.bytes());
                    old.gpu
                }
                None => None,
            };
            g.map.insert(h, NcEntry { data: Some(data), gpu, thumb, size, stamp: tick, hot });
            g.bytes += bytes;
        }
    }

    /// Stores a result that only exists in device memory.
    fn put_gpu(&self, h: u64, pic: crate::gpu::GTex, thumb: Vec<u8>, size: usize) {
        if let Ok(mut g) = self.inner.lock() {
            g.tick += 1;
            let tick = g.tick;
            let (data, hot) = match g.map.remove(&h) {
                Some(old) => {
                    g.gbytes -= old.gpu.as_ref().map_or(0, |x| x.bytes());
                    (old.data, old.hot)
                }
                None => (None, std::sync::Weak::new()),
            };
            g.gbytes += pic.bytes();
            g.map.insert(h, NcEntry { data, gpu: Some(pic), thumb, size, stamp: tick, hot });
        }
    }

    /// Remembers the device copy of an entry that so far only had pixels on the host.
    fn attach_gpu(&self, h: u64, pic: crate::gpu::GTex) {
        if let Ok(mut g) = self.inner.lock() {
            let add = match g.map.get_mut(&h) {
                Some(e) if e.gpu.is_none() => {
                    let b = pic.bytes();
                    e.gpu = Some(pic);
                    b
                }
                _ => 0,
            };
            g.gbytes += add;
        }
    }

    fn set_thumb(&self, h: u64, thumb: Vec<u8>) {
        if let Ok(mut g) = self.inner.lock() {
            if let Some(e) = g.map.get_mut(&h) {
                e.thumb = thumb;
            }
        }
    }

    /// Remembers the texture just read back or expanded, so the next hit finds it while anyone still holds it.
    fn note_hot(&self, h: u64, t: &Arc<Tex>) {
        if let Ok(mut g) = self.inner.lock() {
            if let Some(e) = g.map.get_mut(&h) {
                e.hot = Arc::downgrade(t);
            }
        }
    }

    fn trim(&self) {
        if let Ok(mut g) = self.inner.lock() {
            if g.bytes <= self.budget && g.gbytes <= self.gbudget {
                return;
            }
            // least recently used first, sorted once (not one full scan per evicted entry)
            let mut order: Vec<(u64, u64)> = g.map.iter().map(|(k, e)| (e.stamp, *k)).collect();
            order.sort_unstable();
            for (_, k) in order {
                let (over_host, over_dev) = (g.bytes > self.budget, g.gbytes > self.gbudget);
                if (!over_host && !over_dev) || g.map.len() <= 1 {
                    break;
                }
                // only the kind of memory that is over budget is given back
                let (db, dg, empty) = match g.map.get_mut(&k) {
                    Some(e) => {
                        let db = if over_host { e.data.take().map_or(0, |d| d.bytes()) } else { 0 };
                        let dg = if over_dev { e.gpu.take().map_or(0, |x| x.bytes()) } else { 0 };
                        (db, dg, e.data.is_none() && e.gpu.is_none())
                    }
                    None => continue,
                };
                g.bytes -= db;
                g.gbytes -= dg;
                if empty {
                    g.map.remove(&k);
                }
            }
        }
    }

    /// (reused, recomputed) node counts since the cache was created.
    pub fn stats(&self) -> (usize, usize) {
        use std::sync::atomic::Ordering::Relaxed;
        (self.hits.load(Relaxed), self.misses.load(Relaxed))
    }

    pub fn clear(&self) {
        if let Ok(mut g) = self.inner.lock() {
            g.map.clear();
            g.bytes = 0;
            g.gbytes = 0;
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.map.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn bytes(&self) -> usize {
        self.inner.lock().map(|g| g.bytes).unwrap_or(0)
    }

    /// Device memory held by cached pictures.
    pub fn gpu_bytes(&self) -> usize {
        self.inner.lock().map(|g| g.gbytes).unwrap_or(0)
    }
}

struct Fnv(u64);

impl Fnv {
    fn new() -> Fnv {
        Fnv(0xcbf29ce484222325)
    }
    fn bytes(&mut self, b: &[u8]) {
        for x in b {
            self.0 = (self.0 ^ *x as u64).wrapping_mul(0x100000001b3);
        }
    }
    fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        self.u64(s.len() as u64);
        self.bytes(s.as_bytes());
    }
}

/// Hash of every node (and named subgraph output) of the graph: kind, settings, evaluation context and the hashes of
/// its inputs. Two nodes with the same hash always produce the same texture.
fn hash_graph(g: &Graph, ctx: &Ctx) -> HashMap<u32, u64> {
    let mut env = Fnv::new();
    env.u64(crate::gpu::active() as u64);
    env.u64(g.default_format() as u64);
    for (k, sub) in ctx.lib {
        env.str(k);
        env.str(&sub.to_text());
    }
    for (k, v) in ctx.res {
        env.str(k);
        env.u64(v.len() as u64);
        let mut f = Fnv::new();
        if v.len() > (1 << 20) {
            // big resources (baked maps) are fingerprinted from 4096 evenly spread 64-byte samples, not byte by byte
            let step = v.len() / 4096;
            let mut at = 0;
            while at < v.len() {
                f.bytes(&v[at..(at + 64).min(v.len())]);
                at += step.max(64);
            }
        } else {
            f.bytes(v);
        }
        env.u64(f.0);
    }
    for (k, v) in ctx.gin {
        env.str(k);
        env.str(&v.show());
    }
    let mut memo: HashMap<u32, u64> = HashMap::new();
    fn go(g: &Graph, id: u32, out: Option<&str>, ctx: &Ctx, env: u64, memo: &mut HashMap<u32, u64>, depth: u32) -> u64 {
        let key = cache_key(g, id, out);
        if let Some(h) = memo.get(&key) {
            return *h;
        }
        let Some(n) = g.nodes.get(&id) else { return 0 };
        let mut h = Fnv::new();
        h.u64(env);
        h.u64(ctx.size as u64);
        h.u64(ctx.height as u64);
        h.u64(ctx.nominal as u64);
        h.u64(ctx.so as u64);
        h.str(&n.kind);
        if key != id {
            h.str(out.unwrap_or(""));
        }
        for (k, v) in &n.nums {
            h.str(k);
            h.u64(v.to_bits() as u64);
        }
        for (k, v) in &n.strs {
            h.str(k);
            h.str(v);
        }
        if n.kind.starts_with("path_") {
            // an embedded spline can be edited under the same name: its content is part of the identity
            for key in ["file", "file2", "file3", "file4"] {
                if let Some(b) = n.strs.get(key).and_then(|f| ctx.res.get(f)) {
                    h.u64(b.len() as u64);
                    for ch in b.chunks(8) {
                        h.u64(ch.iter().fold(0u64, |a, &x| a.wrapping_mul(257).wrapping_add(x as u64)));
                    }
                }
            }
        }
        if matches!(n.kind.as_str(), "bitmap" | "svg" | "text" | "color_lut") || n.kind.starts_with("path_") {
            // a file on disk can change under us: its size and modification time are part of the identity
            for f in [n.strs.get("file"), n.strs.get("file2"), n.strs.get("file3"), n.strs.get("file4"), n.strs.get("font")].into_iter().flatten() {
                if !ctx.res.contains_key(f) {
                    if let Some(m) = crate::resources::resolve(f).and_then(|p| std::fs::metadata(p).ok()) {
                        h.u64(m.len());
                        h.u64(m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos() as u64).unwrap_or(0));
                    }
                }
            }
        }
        if depth < 512 {
            for (slot, rid) in &n.ins {
                let want = n.in_out.get(slot).map(|s| s.as_str());
                h.str(slot);
                h.str(want.unwrap_or(""));
                h.u64(go(g, *rid, want, ctx, env, memo, depth + 1));
            }
        }
        memo.insert(key, h.0);
        h.0
    }
    for (id, n) in &g.nodes {
        go(g, *id, None, ctx, env.0, &mut memo, 0);
        for (slot, rid) in &n.ins {
            if let Some(w) = n.in_out.get(slot) {
                go(g, *rid, Some(w), ctx, env.0, &mut memo, 0);
            }
        }
    }
    memo
}

fn eval_node(g: &Graph, id: u32, ctx: &Ctx, ext: &[Option<Rc<Lazy>>], cache: &mut Cache) -> Result<Rc<Tex>, Error> {
    eval_node_out(g, id, None, ctx, ext, cache)
}

/// Cache key for one named output of a graph node (plain node ids keep their own key).
fn out_key(id: u32, out: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in out.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    0x8000_0000 | ((h & 0x7fff) << 16) | (id & 0xffff)
}

fn cancelled() -> bool {
    crate::tex::job_stale()
}

/// MC_PROF set: every node prints how long it took.
fn profiling() -> bool {
    static P: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *P.get_or_init(|| std::env::var_os("MC_PROF").is_some())
}

thread_local! {
    /// outputs a multi-output node computed together with the one it was asked for, waiting for their own request in
    /// this pass: (identity of the node run, output index, texture, the inputs it was computed from)
    static SIBLINGS: std::cell::RefCell<Vec<(u64, usize, Tex, Vec<Rc<Tex>>)>> = const { std::cell::RefCell::new(Vec::new()) };
    /// where node code puts those extra outputs while it runs
    static OUTBOX: std::cell::RefCell<Vec<(usize, Tex)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// For node code that produces several outputs in one go (see the hidden `_want` setting): hands over output `out`,
/// which the request for that output collects instead of running the node again.
pub fn emit_sibling(out: usize, t: Tex) {
    OUTBOX.with(|o| o.borrow_mut().push((out, t)));
}

/// Forgets every waiting sibling output (start and end of an evaluation).
fn clear_siblings() {
    SIBLINGS.with(|s| s.borrow_mut().clear());
    OUTBOX.with(|o| o.borrow_mut().clear());
}

/// Identity of one run of a node: everything its result depends on except which output is wanted. The inputs are
/// identified by their texture; the stash keeps those textures alive, so an address is never reused meanwhile.
fn sibling_key(id: u32, n: &Node, ins: &BTreeMap<String, Rc<Tex>>, ctx: &Ctx) -> u64 {
    let mut h = Fnv::new();
    h.u64(id as u64);
    h.str(&n.kind);
    for (k, v) in &n.nums {
        if k != "_out" && k != "_want" {
            h.str(k);
            h.u64(v.to_bits() as u64);
        }
    }
    for (k, v) in &n.strs {
        h.str(k);
        h.str(v);
    }
    for v in [ctx.size, ctx.height, ctx.nominal, ctx.so as usize, ctx.depth, ctx.lib as *const _ as usize, ctx.res as *const _ as usize, ctx.gin as *const _ as usize] {
        h.u64(v as u64);
    }
    for (k, t) in ins {
        h.str(k);
        h.u64(Rc::as_ptr(t) as usize as u64);
    }
    h.0
}

/// Runs a node for output `idx`, sharing one run between all the outputs this pass reads when the node supports it.
fn compute_output(id: u32, n: &Node, idx: usize, ins: &BTreeMap<String, Rc<Tex>>, ctx: &Ctx, want: u32) -> Result<Tex, Error> {
    let key = sibling_key(id, n, ins, ctx);
    let ready = SIBLINGS.with(|s| {
        let mut s = s.borrow_mut();
        s.iter().position(|e| e.0 == key && e.1 == idx).map(|i| s.swap_remove(i).2)
    });
    if let Some(t) = ready {
        return Ok(t);
    }
    let want = want | (1u32 << idx.min(31));
    let n2;
    let n = if idx > 0 || want != 1u32 << idx {
        let mut c = n.clone();
        if idx > 0 {
            c.nums.insert("_out".into(), idx as f32);
        }
        if want != 1u32 << idx {
            c.nums.insert("_want".into(), want as f32);
        }
        n2 = c;
        &n2
    } else {
        n
    };
    let mark = OUTBOX.with(|o| o.borrow().len());
    let r = compute(id, n, ins, ctx);
    let extra: Vec<(usize, Tex)> = OUTBOX.with(|o| {
        let mut o = o.borrow_mut();
        let keep = mark.min(o.len());
        o.split_off(keep)
    });
    if r.is_ok() && !extra.is_empty() {
        let held: Vec<Rc<Tex>> = ins.values().cloned().collect();
        SIBLINGS.with(|s| {
            let mut s = s.borrow_mut();
            for (o, t) in extra {
                if o != idx {
                    s.push((key, o, t, held.clone()));
                }
            }
        });
    }
    r
}

/// Output Format of a node's result: its own setting, else the graph default (0 = 32-bit float, untouched).
fn node_format(g: &Graph, n: &Node) -> u32 {
    match n.nums.get("ofmt").map(|v| v.round() as u32) {
        Some(v @ 1..=3) => v,
        Some(4) => 0,
        _ => g.default_format(),
    }
}

/// Output size of a node, the Output Size setting: "Relative to input" (default; falls back to the parent size when
/// nothing is connected), "Relative to parent", or "Absolute" (a power of two). The relative modifier moves the size
/// up or down in powers of two (-12 .. +12), so a heavy node can run at half or quarter resolution.
/// Output size of a node from the sizes of its inputs (a device-only input has no pixels to measure).
fn node_size_d(n: &Node, ins: &BTreeMap<String, (usize, usize)>, ctx: &Ctx) -> (usize, usize) {
    let get = |k: &str, d: f32| n.nums.get(k).copied().unwrap_or(d);
    let mode = get("osize_mode", 0.0).round() as i32;
    let rel = get("osize_rel", 0.0).round().clamp(-12.0, 12.0) as i32;
    let (bw, bh) = match mode {
        2 => {
            let e = get("osize_abs", 9.0).round().clamp(4.0, 13.0) as u32;
            let w = (((1usize << e) as f64 * ctx.size as f64 / ctx.nominal.max(1) as f64).round() as usize).max(1);
            // the height keeps the graph's aspect ratio unless the node sets its own height exponent
            let h = match n.nums.get("osize_abs_h") {
                Some(eh) => (((1usize << (eh.round().clamp(4.0, 13.0) as u32)) as f64 * ctx.size as f64 / ctx.nominal.max(1) as f64).round() as usize).max(1),
                None => ((w as f64 * ctx.height as f64 / ctx.size.max(1) as f64).round() as usize).max(1),
            };
            (w, h)
        }
        1 => (ctx.size, ctx.height),
        _ => ins.get("in").or_else(|| ins.get("bg")).or_else(|| ins.values().next()).copied().unwrap_or((ctx.size, ctx.height)),
    };
    let shift = |b: usize| -> usize {
        if mode != 2 && rel != 0 {
            let v = if rel > 0 { b << rel } else { (b >> (-rel)).max(1) };
            v.clamp(16.min(b), 8192)
        } else {
            b
        }
    };
    (shift(bw).clamp(1, 8192), shift(bh).clamp(1, 8192))
}

fn eval_node_out(g: &Graph, id: u32, out: Option<&str>, ctx: &Ctx, ext: &[Option<Rc<Lazy>>], cache: &mut Cache) -> Result<Rc<Tex>, Error> {
    eval_res(g, id, out, ctx, ext, cache)?.tex()
}

/// One node result. It exists on the host (`cpu`), in device memory (`gpu`), or both; the other form is made the first
/// time something asks for it. A chain of GPU nodes therefore never touches host memory, and a CPU node only pays for
/// the read-back of the inputs it really reads.
pub(crate) struct Lazy {
    w: usize,
    h: usize,
    cpu: std::sync::OnceLock<Rc<Tex>>,
    gpu: std::sync::OnceLock<crate::gpu::GTex>,
    /// the cache's compact copy, expanded on demand
    packed: Option<NcData>,
    /// cache key and cache, so a form made later is remembered for the next evaluation
    hash: Option<u64>,
    persist: Option<Arc<NodeCache>>,
}

impl Lazy {
    fn from_cpu(t: Rc<Tex>, hash: Option<u64>, persist: Option<Arc<NodeCache>>) -> Rc<Lazy> {
        let (w, h) = (t.w, t.h);
        let cpu = std::sync::OnceLock::new();
        let _ = cpu.set(t);
        Rc::new(Lazy { w, h, cpu, gpu: std::sync::OnceLock::new(), packed: None, hash, persist })
    }

    fn from_gpu(g: crate::gpu::GTex, hash: Option<u64>, persist: Option<Arc<NodeCache>>) -> Rc<Lazy> {
        let (w, h) = (g.w(), g.h());
        let gpu = std::sync::OnceLock::new();
        let _ = gpu.set(g);
        Rc::new(Lazy { w, h, cpu: std::sync::OnceLock::new(), gpu, packed: None, hash, persist })
    }

    fn from_hit(hit: NcHit, hash: u64, persist: &Arc<NodeCache>) -> Option<Rc<Lazy>> {
        let (w, h) = match (&hit.hot, &hit.data, &hit.gpu) {
            (Some(t), _, _) => (t.w, t.h),
            (_, Some(d), _) => d.dims(),
            (_, _, Some(g)) => (g.w(), g.h()),
            _ => return None,
        };
        let cpu = std::sync::OnceLock::new();
        if let Some(t) = hit.hot {
            let _ = cpu.set(t);
        }
        let gpu = std::sync::OnceLock::new();
        if let Some(g) = hit.gpu {
            let _ = gpu.set(g);
        }
        Some(Rc::new(Lazy { w, h, cpu, gpu, packed: hit.data, hash: Some(hash), persist: Some(persist.clone()) }))
    }

    /// True when the picture is already in device memory (reading it there costs nothing).
    fn on_device(&self) -> bool {
        self.gpu.get().is_some()
    }

    /// The pixels on the host; reads the device picture back (or expands the cached copy) the first time.
    fn tex(&self) -> Result<Rc<Tex>, Error> {
        if let Some(t) = self.cpu.get() {
            return Ok(t.clone());
        }
        let t = if let Some(d) = &self.packed {
            d.unpack()
        } else if let Some(g) = self.gpu.get() {
            match crate::gpu::download(&[g]).and_then(|mut v| v.pop()) {
                Some(t) => Rc::new(t),
                None => {
                    // the device failed: later passes recompute on the CPU (the cache key includes this switch)
                    crate::gpu::set_enabled(false);
                    return Err(Error::Io("GPU read-back failed; GPU evaluation was switched off".into()));
                }
            }
        } else {
            return Err(Error::Io("node result is empty".into()));
        };
        if let (Some(h), Some(p)) = (self.hash, &self.persist) {
            p.note_hot(h, &t);
        }
        let _ = self.cpu.set(t);
        self.cpu.get().cloned().ok_or_else(|| Error::Io("node result is empty".into()))
    }

    /// The picture in device memory; uploads the host copy the first time (and remembers it in the cache).
    fn dev(&self) -> Option<crate::gpu::GTex> {
        if let Some(g) = self.gpu.get() {
            return Some(g.clone());
        }
        let t = self.tex().ok()?;
        let g = crate::gpu::upload(&t)?;
        if let (Some(h), Some(p)) = (self.hash, &self.persist) {
            p.attach_gpu(h, g.clone());
        }
        let _ = self.gpu.set(g);
        self.gpu.get().cloned()
    }

    fn thumb(&self) -> Vec<u8> {
        if let Some(t) = self.cpu.get() {
            return thumbnail(t);
        }
        if let Some(g) = self.gpu.get() {
            if let Some(b) = crate::gpu::thumb_g(g, THUMB) {
                return b;
            }
        }
        match self.tex() {
            Ok(t) => thumbnail(&t),
            Err(_) => Vec::new(),
        }
    }
}

/// A node result handed out unread (see `Graph::evaluate_all_cached_ex`).
#[derive(Clone)]
pub struct Pending(Rc<Lazy>);

impl Pending {
    /// Width and height.
    pub fn dims(&self) -> (usize, usize) {
        (self.0.w, self.0.h)
    }

    /// The picture in device memory, only if it is there already (nothing is uploaded).
    pub fn device(&self) -> Option<crate::gpu::GTex> {
        self.0.gpu.get().cloned()
    }

    /// The picture for the 2D view made on the device: RGBA8 bytes and the (smallest, largest) colour value. `None`
    /// when the picture is not on the device (or the device fails): use [`Pending::tex`] then.
    pub fn view(&self) -> Option<(Vec<u8>, (f32, f32))> {
        let g = self.device()?;
        let v = crate::gpu::pack_view_g(&g)?;
        crate::gpu::download_view(&v)
    }

    /// The pixels on the host; reads the device picture back the first time.
    pub fn tex(&self) -> Result<Arc<Tex>, String> {
        self.0.tex().map_err(|e| e.to_string())
    }
}

/// Reads every device-only picture in `items` back with one submission and one wait (a failure leaves the single
/// `tex()` calls to report it).
fn read_back(items: &[(u32, Rc<Lazy>)]) {
    let need: Vec<&Rc<Lazy>> = items.iter().map(|(_, l)| l).filter(|l| l.cpu.get().is_none() && l.packed.is_none() && l.gpu.get().is_some()).collect();
    if need.len() < 2 {
        return;
    }
    let gs: Vec<&crate::gpu::GTex> = need.iter().filter_map(|l| l.gpu.get()).collect();
    if let Some(ts) = crate::gpu::download(&gs) {
        for (l, t) in need.iter().zip(ts) {
            let t = Rc::new(t);
            if let (Some(h), Some(p)) = (l.hash, &l.persist) {
                p.note_hot(h, &t);
            }
            let _ = l.cpu.set(t);
        }
    }
}

fn eval_res(g: &Graph, id: u32, out: Option<&str>, ctx: &Ctx, ext: &[Option<Rc<Lazy>>], cache: &mut Cache) -> Result<Rc<Lazy>, Error> {
    let key = cache_key(g, id, out);
    let out = if key != id { out } else { None };
    if let Some(t) = cache.map.get(&key) {
        return Ok(t.clone());
    }
    if cancelled() {
        return Err(Error::Cancelled);
    }
    let phash = cache.hashes.get(&key).copied();
    if let (Some(p), Some(h)) = (&cache.persist, phash) {
        if let Some(hit) = p.get(h) {
            let (mut th, sz) = (hit.thumb.clone(), hit.size);
            if let Some(t) = Lazy::from_hit(hit, h, p) {
                // a device picture made during a drag has no card thumbnail yet: take it when a pass shows the cards
                if key == id && th.is_empty() && cache.thumbs.is_some() {
                    th = t.thumb();
                    p.set_thumb(h, th.clone());
                }
                cache.map.insert(key, t.clone());
                if key == id {
                    cache.sizes.insert(id, sz);
                    if let Some(tm) = cache.thumbs.as_mut() {
                        tm.insert(id, th);
                    }
                }
                return Ok(t);
            }
        }
    }
    let n = g.nodes.get(&id).ok_or(Error::MissingNode(0, id))?;
    let bound;
    let n = if n.strs.keys().any(|k| k.starts_with("fn.")) {
        bound = bind_functions(id, n, ctx)?;
        &bound
    } else {
        n
    };
    let mut ins_l: BTreeMap<String, Rc<Lazy>> = BTreeMap::new();
    for (slot, rid) in &n.ins {
        let want = n.in_out.get(slot).map(|s| s.as_str());
        ins_l.insert(slot.clone(), eval_res(g, *rid, want, ctx, ext, cache)?);
    }
    // the node's own output size; inputs of another size are resampled to it
    let dims: BTreeMap<String, (usize, usize)> = ins_l.iter().map(|(k, t)| (k.clone(), (t.w, t.h))).collect();
    let sz = node_size_d(n, &dims, ctx);
    let node_ctx;
    let ctx = if sz.0 != ctx.size || sz.1 != ctx.height {
        node_ctx = Ctx { size: sz.0, height: sz.1, nominal: ctx.nominal, lib: ctx.lib, res: ctx.res, so: ctx.so, depth: ctx.depth, gin: ctx.gin };
        &node_ctx
    } else {
        ctx
    };
    // an explicit per-node Output Format (8 / 16 bit integer, 16 bit float) quantises the result; 32 bit float is untouched
    // (a node's own value 4 forces 32-bit float even when the graph rounds everything else)
    let fmt = node_format(g, n);
    // Output and Dot nodes pass their input on: the same texture is shared instead of copied whenever the result would
    // be identical (same size, and no quantisation the input has not already had)
    // a disabled node (Shift+D in the editor) hands its first input on unchanged
    let disabled = n.nums.get("_disabled").copied().unwrap_or(0.0) > 0.5 && !matches!(n.kind.as_str(), "input" | "output" | "dot" | "subgraph");
    let through = if disabled && ins_l.values().next().is_some() {
        ins_l.get("in").or_else(|| ins_l.values().next()).cloned()
    } else if matches!(n.kind.as_str(), "output" | "dot") && out.is_none() {
        ins_l.get("in").filter(|t| (t.w, t.h) == sz).filter(|_| {
            fmt == 0 || (!n.in_out.contains_key("in") && n.ins.get("in").and_then(|r| g.nodes.get(r)).map(|m| m.kind != "subgraph" && node_format(g, m) == fmt).unwrap_or(false))
        })
        .cloned()
    } else {
        None
    };
    let outs = node_outputs(&n.kind);
    let rc = match through {
        Some(t) => t,
        None => {
            // GPU nodes work on device pictures and leave the result there
            let t0 = std::time::Instant::now();
            // instancing nodes pass the pictures on as they are (on the device when they live there)
            let special = match n.kind.as_str() {
                "input" => Some(input_lazy(n, ext, sz)?),
                "subgraph" => {
                    let fitted: BTreeMap<String, Rc<Lazy>> = ins_l.iter().map(|(k, l)| lazy_at(l, sz).map(|r| (k.clone(), r))).collect::<Result<_, _>>()?;
                    Some(run_sub(id, n, &fitted, ctx, out)?)
                }
                _ => None,
            };
            let on_dev = if special.is_none() {
                let idx = out.and_then(|o| outs.iter().position(|x| *x == o)).unwrap_or(0);
                graph_gpu::run(n, &ins_l, sz, ctx, idx)
            } else {
                None
            };
            if let Some(l) = special {
                if profiling() {
                    eprintln!("  {:>5.0} ms  node {} {} ({})", t0.elapsed().as_secs_f64() * 1000.0, id, n.kind, if l.on_device() { "gpu" } else { "cpu" });
                }
                if cancelled() {
                    return Err(Error::Cancelled);
                }
                lazy_format(l, fmt, phash, cache.persist.clone())?
            } else if let Some(gt) = on_dev {
                if profiling() {
                    eprintln!("  {:>5.0} ms  node {} {} (gpu)", t0.elapsed().as_secs_f64() * 1000.0, id, n.kind);
                }
                if cancelled() {
                    return Err(Error::Cancelled);
                }
                lazy_format(Lazy::from_gpu(gt, None, None), fmt, phash, cache.persist.clone())?
            } else {
                let mut ins: BTreeMap<String, Rc<Tex>> = BTreeMap::new();
                for (slot, l) in &ins_l {
                    ins.insert(slot.clone(), l.tex()?);
                }
                if ins.values().any(|t| t.w != sz.0 || t.h != sz.1) {
                    for t in ins.values_mut() {
                        if t.w != sz.0 || t.h != sz.1 {
                            *t = Rc::new(ops::resize_to(t, sz.0, sz.1));
                        }
                    }
                }
                let t = match n.kind.as_str() {
                    _ => {
                        // a secondary output of a multi-output node runs the same code with the output index set; outputs
                        // that this pass also reads (and has not got yet) may be produced by the same run
                        let idx = out.and_then(|o| outs.iter().position(|x| *x == o)).unwrap_or(0);
                        let r = if outs.len() > 1 {
                            let mut want = cache.wants.get(&id).copied().unwrap_or(1);
                            for (b, name) in outs.iter().enumerate() {
                                if b == idx || want & (1 << b) == 0 {
                                    continue;
                                }
                                let k = cache_key(g, id, Some(name));
                                let known = cache.map.contains_key(&k) || matches!((&cache.persist, cache.hashes.get(&k)), (Some(p), Some(h)) if p.contains(*h));
                                if known {
                                    want &= !(1 << b);
                                }
                            }
                            compute_output(id, n, idx, &ins, ctx, want)?
                        } else {
                            compute(id, n, &ins, ctx)?
                        };
                        if profiling() {
                            eprintln!("  {:>5.0} ms  node {} {}", t0.elapsed().as_secs_f64() * 1000.0, id, n.kind);
                        }
                        r
                    }
                };
                // a superseded evaluation can leave a half-finished texture behind: never keep or show it
                if cancelled() {
                    return Err(Error::Cancelled);
                }
                let mut t = if t.w != sz.0 || t.h != sz.1 { ops::resize_to(&t, sz.0, sz.1) } else { t };
                if fmt != 0 {
                    ops::apply_format(&mut t, fmt);
                }
                Lazy::from_cpu(Rc::new(t), phash, cache.persist.clone())
            }
        }
    };
    cache.map.insert(key, rc.clone());
    if key == id {
        cache.sizes.insert(id, sz.0);
    }
    // a result that only exists on the device gets its thumbnail when a pass shows the cards (it would cost a read-back
    // per node during a drag); one that is on the host gets it now, as before
    let thumb = if key == id && (cache.thumbs.is_some() || (cache.persist.is_some() && rc.cpu.get().is_some())) { rc.thumb() } else { Vec::new() };
    if let (Some(p), Some(h)) = (&cache.persist, phash) {
        match rc.cpu.get() {
            Some(t) => p.put(h, t.clone(), thumb.clone(), sz.0),
            None => {
                if let Some(gt) = rc.gpu.get() {
                    p.put_gpu(h, gt.clone(), thumb.clone(), sz.0);
                }
            }
        }
    }
    if key == id {
        if let Some(th) = cache.thumbs.as_mut() {
            th.insert(id, thumb);
        }
    }
    // free inputs nobody else is waiting for
    for (slot, rid) in &n.ins {
        let k = cache_key(g, *rid, n.in_out.get(slot).map(|s| s.as_str()));
        if let Some(c) = cache.left.get_mut(&k) {
            *c = c.saturating_sub(1);
            if *c == 0 && !cache.keep.contains(&k) {
                cache.map.remove(&k);
            }
        }
    }
    Ok(rc)
}

/// Textures of finished nodes, plus what is needed to free them early.
struct Cache {
    map: HashMap<u32, Rc<Lazy>>,
    /// how many not-yet-evaluated links still read each cached entry; an entry is freed when it reaches zero
    left: HashMap<u32, u32>,
    keep: HashSet<u32>,
    /// thumbnails taken while evaluating (editor preview)
    thumbs: Option<HashMap<u32, Vec<u8>>>,
    sizes: HashMap<u32, usize>,
    /// results that outlive one evaluation (editor): a node whose settings and inputs did not change is not recomputed
    persist: Option<Arc<NodeCache>>,
    hashes: HashMap<u32, u64>,
    /// outputs of each multi-output node that this pass reads (bit i = output i; bit 0, the default, always set)
    wants: HashMap<u32, u32>,
}

impl Cache {
    fn plain() -> Cache {
        Cache { map: HashMap::new(), left: HashMap::new(), keep: HashSet::new(), thumbs: None, sizes: HashMap::new(), persist: None, hashes: HashMap::new(), wants: HashMap::new() }
    }

    fn for_graph(g: &Graph, keep: &[u32], thumbs: bool) -> Cache {
        let mut c = Cache::plain();
        // which outputs of every node are read: a node computes each of them separately (and reads its inputs again
        // for each), so its inputs must stay until all of them are done
        let mut named: HashMap<u32, HashSet<&str>> = HashMap::new();
        for n in g.nodes.values() {
            for (slot, rid) in &n.ins {
                let Some(o) = n.in_out.get(slot) else { continue };
                if cache_key(g, *rid, Some(o)) != *rid {
                    named.entry(*rid).or_default().insert(o.as_str());
                }
                if let Some(i) = g.nodes.get(rid).and_then(|m| node_outputs(&m.kind).iter().position(|x| x == o)) {
                    *c.wants.entry(*rid).or_insert(1) |= 1u32 << i.min(31);
                }
            }
        }
        for (id, n) in &g.nodes {
            let times = 1 + named.get(id).map_or(0, |s| s.len() as u32);
            for (slot, rid) in &n.ins {
                *c.left.entry(cache_key(g, *rid, n.in_out.get(slot).map(|s| s.as_str()))).or_insert(0) += times;
            }
        }
        c.keep = keep.iter().copied().collect();
        if thumbs {
            c.thumbs = Some(HashMap::new());
        }
        c
    }
}

/// Named outputs of a built-in node (empty = one plain output). The first name is the default output: a link that
/// names no output, or names the first one, reads it. Extra outputs are computed on demand by running the node's
/// own code with a hidden `_out` index (see `eval_node_out`), and cached separately.
pub fn node_outputs(kind: &str) -> &'static [&'static str] {
    match kind {
        "color_match" => &["output", "mask"],
        "quantize_color" => &["output", "id", "palette"],
        "weave_generator" => &["height", "mask_1", "mask_2", "opacity"],
        "rgba_split" => &["r", "g", "b", "a"],
        "alpha_split" => &["rgb", "alpha"],
        "voronoi" => &["output", "cell_id", "edge", "distance"],
        "cells" => &["output", "cell_id", "distance", "edge"],
        "splatter" => &["output", "id", "center", "size", "luminance"],
        "flood_fill_to_position" => &["position", "x", "y"],
        "bevel" => &["height", "normal"],
        "flood_fill_to_bbox_size" => &["size", "width", "height"],
        "frequency_split" => &["low", "high"],
        "bent_normal" => &["normal", "ao"],
        "metal_rough_to_spec_gloss" => &["diffuse", "specular", "glossiness"],
        "spec_gloss_to_metal_rough" => &["base_color", "metallic", "roughness"],
        "material_blend" => &["base_color", "normal", "height", "roughness", "metallic", "ao"],
        "convex_concave" => &["convex", "concave"],
        "curvature_smooth" => &["output", "convexity", "concavity"],
        "paving" => &["height", "cell_id", "gaps"],
        "path_border" => &["height", "mask"],
        "path_track" => &["height", "rails", "sleepers"],
        "path_cable" => &["height", "mask", "strand"],
        "path_seam" => &["height", "stitches"],
        "path_sample" => &["position", "width", "height", "side"],
        "shape_extrude" => &["height", "walls"],
        "uber_emboss" => &["emboss", "highlight", "shadow"],
        "snow_filter" => &["height", "mask"],
        "water_filter" => &["color", "mask", "height", "roughness"],
        _ => crate::nodes10::outputs(kind),
    }
}

/// What travels through a pin. Grayscale pins carry one value per pixel, colour pins carry RGB(A). Some tools keep
/// the two kinds apart; here every texture is stored as RGBA, so the type is what the editor shows and what
/// `output_pin` / `pin_types` report (a grayscale value fed to a colour input simply becomes a gray colour).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pin {
    Gray,
    Color,
    /// value pins (carried by function graphs and value nodes, not by textures)
    Float,
    Float2,
    Float3,
    Float4,
    Int,
    Int2,
    Int3,
    Int4,
    Bool,
    Text,
}

impl Pin {
    /// Carries a texture (grayscale or colour).
    pub fn is_image(self) -> bool {
        matches!(self, Pin::Gray | Pin::Color)
    }

    /// Number of components of a numeric value pin (1 for scalars, 0 for images, bool and text).
    pub fn lanes(self) -> usize {
        match self {
            Pin::Float | Pin::Int => 1,
            Pin::Float2 | Pin::Int2 => 2,
            Pin::Float3 | Pin::Int3 => 3,
            Pin::Float4 | Pin::Int4 => 4,
            _ => 0,
        }
    }

    /// Can an output of type `from` feed an input of this type without an explicit cast?
    /// Images convert freely between gray and colour; numeric values widen from integer to float and from bool to
    /// integer or float with the same number of components; nothing else converts implicitly (a float needs a cast
    /// to become an integer, an image never turns into a value or back).
    pub fn accepts(self, from: Pin) -> bool {
        if self == from {
            return true;
        }
        if self.is_image() || from.is_image() {
            return self.is_image() && from.is_image();
        }
        match (self, from) {
            (Pin::Float, Pin::Int) | (Pin::Float2, Pin::Int2) | (Pin::Float3, Pin::Int3) | (Pin::Float4, Pin::Int4) => true,
            (Pin::Int, Pin::Bool) | (Pin::Float, Pin::Bool) => true,
            _ => false,
        }
    }

    /// Short name for tooltips and file messages.
    pub fn name(self) -> &'static str {
        match self {
            Pin::Gray => "grayscale",
            Pin::Color => "color",
            Pin::Float => "float",
            Pin::Float2 => "float2",
            Pin::Float3 => "float3",
            Pin::Float4 => "float4",
            Pin::Int => "integer",
            Pin::Int2 => "integer2",
            Pin::Int3 => "integer3",
            Pin::Int4 => "integer4",
            Pin::Bool => "boolean",
            Pin::Text => "string",
        }
    }
}

/// What a built-in node's input slot expects when it is not an image slot. Image slots (the default) take any
/// texture. Value-typed slots belong to the function-graph and value nodes.
pub fn input_value_type(kind: &str, slot: &str) -> Option<Pin> {
    let _ = (kind, slot);
    None
}

/// Connections whose types do not fit: `(node, slot, message)`. The editor refuses these when a link is dragged;
/// a file that contains one is still loadable, the node simply reports the error when it is evaluated.
pub fn connection_issues(g: &Graph) -> Vec<(u32, String, String)> {
    let pins = pin_types(g);
    let mut issues = Vec::new();
    for (id, n) in &g.nodes {
        for (slot, src) in &n.ins {
            let Some(sn) = g.nodes.get(src) else { continue };
            let out = n.in_out.get(slot).map(|s| s.as_str());
            let from = if !node_outputs(&sn.kind).is_empty() && out.is_some() {
                let upstream = sn.ins.values().any(|s| pins.get(s) == Some(&Pin::Color));
                output_pin(&sn.kind, out, upstream, sn)
            } else {
                pins.get(src).copied().unwrap_or(Pin::Gray)
            };
            let want = input_value_type(&n.kind, slot).unwrap_or(Pin::Gray);
            if !want.accepts(from) {
                issues.push((*id, slot.clone(), format!("{} input '{slot}' needs {} but node {src} gives {}", n.kind, want.name(), from.name())));
            }
        }
    }
    issues
}

/// Output type of a built-in node. `any_color_in` says whether any connected input is a colour pin, `n` supplies
/// mode switches such as the Uniform Color node's grayscale flag.
pub fn output_pin(kind: &str, out: Option<&str>, any_color_in: bool, n: &Node) -> Pin {
    if let Some(p) = crate::nodes10::pin(kind, out) {
        return p;
    }
    let flag = |k: &str| n.nums.get(k).copied().unwrap_or(0.0) > 0.5;
    match kind {
        // always colour
        "colorize" | "flood_fill_color" => Pin::Color,
        "color_lut" | "tint" | "hsl" | "channel_mixer" | "merge" | "replace_color" | "chrominance_extract" | "normal" | "normal_blend" | "normal_combine"
        | "normal_invert" | "normal_normalize" | "normal_vector_rotation" | "alpha_merge" | "vector_warp_color" | "height_normal_blend" | "height_to_normal_world" | "normal_transform" => Pin::Color,
        "bent_normal" => match out {
            Some("ao") => Pin::Gray,
            _ => Pin::Color,
        },
        "metal_rough_to_spec_gloss" => match out {
            Some("glossiness") => Pin::Gray,
            _ => Pin::Color,
        },
        "spec_gloss_to_metal_rough" => match out {
            Some("metallic") | Some("roughness") => Pin::Gray,
            _ => Pin::Color,
        },
        "material_blend" => match out {
            Some("base_color") | Some("normal") | None => Pin::Color,
            _ => Pin::Gray,
        },
        "height_combine" | "convex_concave" | "leak_streaks" | "settled_dust" | "mesh_weathering" => Pin::Gray,
        "baked_map" => {
            if flag("gray") || !n.strs.get("map").and_then(|m| crate::bake::MapKind::from_id(m)).map(|m| m.is_colour()).unwrap_or(false) {
                Pin::Gray
            } else {
                Pin::Color
            }
        }
        "palette_extract" | "palette_apply" => Pin::Color,
        "text" if flag("use_color") => Pin::Color,
        "normal_facing" | "normal_sobel" => Pin::Gray,
        "color" | "bitmap" => {
            if flag("gray") {
                Pin::Gray
            } else {
                Pin::Color
            }
        }
        "water_filter" => match out {
            Some("color") | None => Pin::Color,
            _ => Pin::Gray,
        },
        "uber_emboss" | "shape_extrude" | "cross_section" | "snow_filter" => Pin::Gray,
        "color_match" => match out {
            Some("mask") => Pin::Gray,
            _ => Pin::Color,
        },
        "quantize_color" => match out {
            Some("id") => Pin::Gray,
            _ => Pin::Color,
        },
        "rgba_split" => Pin::Gray,
        "splatter" => match out {
            Some("center") | Some("size") => Pin::Color,
            Some("output") | None => if any_color_in { Pin::Color } else { Pin::Gray },
            _ => Pin::Gray,
        },
        "flood_fill_to_index" => Pin::Gray,
        "flood_fill_to_position" | "flood_fill_to_bbox_size" => match out {
            Some("x") | Some("y") | Some("width") | Some("height") => Pin::Gray,
            _ => Pin::Color,
        },
        "alpha_split" => match out {
            Some("alpha") => Pin::Gray,
            _ => Pin::Color,
        },
        // always grayscale
        "to_gray" | "grayscale_conversion" | "normal_to_height" | "ao" | "curvature" | "curvature_smooth" | "distance" | "histogram_range"
        | "histogram_scan" | "histogram_shift" | "threshold" | "flood_fill" | "flood_fill_random" | "flood_fill_gradient" | "edge_detect" | "color_to_mask" | "quantize"
        | "sobel" | "edge_wear" | "dirt" | "dust" | "bevel" | "channel" => Pin::Gray,
        // generators with no colour of their own
        "white_noise" | "gaussian_noise" | "perlin" | "fbm" | "clouds" | "voronoi" | "voronoi_fractal" | "bnw_spots" | "crystal" | "directional_noise"
        | "grunge" | "plasma" | "gradient" | "gradient_axial" | "gradient_circular" | "gradient_reflected" | "circle" | "rect" | "polygon" | "shape"
        | "star" | "starburst" | "checker" | "bricks" | "stripes" | "weave" | "cells" | "fractal_sum" | "waveform" | "gaussian_spots" | "triangle_grid"
        | "grunge_map" | "sdf_shape" | "sdf_op" | "sdf_mask" | "sdf_form" | "blue_noise" | "fibers" | "scratches" | "caustics" | "liquid_noise" | "perlin_3d" | "paving" | "wire_mesh" | "reaction_diffusion" | "path_stroke" | "path_fill" | "path_distance" | "path_border" | "path_scatter" | "path_track" | "path_cable" | "path_seam" | "path_splatter" | "path_sample" | "herringbone" | "arc_pavement" | "anisotropic_noise" | "fur_noise" | "moisture_noise" | "microscope_noise" | "worley_3d" | "fbm_3d" | "volume_mask" | "gradient_3d" | "preset_noise" | "noise_pattern" | "gradient_2pt" | "weave_preset" | "tile_preset" | "tile_random2" | "noise_3d" => Pin::Gray,
        "weave_generator" => Pin::Gray,
        // pixel processor and the scatter family produce colour only when colour comes in
        "pixel_processor" | "value_processor" | "fx_map" => Pin::Color,
        // everything else (blur, levels, blend, transform, warp, ...) keeps the type of what goes in
        _ => {
            if any_color_in {
                Pin::Color
            } else {
                Pin::Gray
            }
        }
    }
}

/// Type of every node's default output in a graph (nodes in a cycle or with unknown inputs count as gray).
pub fn pin_types(g: &Graph) -> HashMap<u32, Pin> {
    fn go(g: &Graph, id: u32, memo: &mut HashMap<u32, Pin>, depth: u32) -> Pin {
        if let Some(p) = memo.get(&id) {
            return *p;
        }
        let Some(n) = g.nodes.get(&id) else { return Pin::Gray };
        if depth > 512 {
            return Pin::Gray;
        }
        memo.insert(id, Pin::Gray); // breaks cycles
        let mut any_color = false;
        for (slot, src) in &n.ins {
            let want = n.in_out.get(slot).map(|s| s.as_str());
            let sn = g.nodes.get(src);
            let p = match (sn, want) {
                // a named output of a built-in multi-output node has its own type
                (Some(sn), Some(o)) if !node_outputs(&sn.kind).is_empty() => {
                    let upstream = sn.ins.values().any(|s| go(g, *s, memo, depth + 1) == Pin::Color);
                    output_pin(&sn.kind, Some(o), upstream, sn)
                }
                _ => go(g, *src, memo, depth + 1),
            };
            if p == Pin::Color {
                any_color = true;
            }
        }
        let p = match n.kind.as_str() {
            "input" => {
                if n.nums.get("gray").copied().unwrap_or(0.0) > 0.5 {
                    Pin::Gray
                } else {
                    Pin::Color
                }
            }
            // a graph node is treated as colour (its outputs are defined by the graph inside)
            "subgraph" => Pin::Color,
            "output" | "dot" => {
                if any_color {
                    Pin::Color
                } else {
                    Pin::Gray
                }
            }
            k => output_pin(k, None, any_color, n),
        };
        memo.insert(id, p);
        p
    }
    let mut memo = HashMap::new();
    for id in g.nodes.keys() {
        go(g, *id, &mut memo, 0);
    }
    memo
}

/// Cache key of an output: the node id, or a hash of (id, output name) for a named output of a graph node or of a
/// built-in multi-output node (its default output keeps the plain node id).
fn cache_key(g: &Graph, id: u32, out: Option<&str>) -> u32 {
    let Some(o) = out else { return id };
    match g.nodes.get(&id) {
        Some(n) if n.kind == "subgraph" => out_key(id, o),
        Some(n) => match node_outputs(&n.kind).iter().position(|x| *x == o) {
            Some(i) if i > 0 => out_key(id, o),
            _ => id,
        },
        None => id,
    }
}

fn thumbnail(t: &Tex) -> Vec<u8> {
    // a non-square texture is fitted inside the square thumbnail with dark bars instead of being stretched
    let (w, h) = (t.w.max(1) as f32, t.h.max(1) as f32);
    let k = (THUMB as f32 / w).min(THUMB as f32 / h);
    let (tw, th) = ((w * k).round().max(1.0), (h * k).round().max(1.0));
    let (ox, oy) = ((THUMB as f32 - tw) * 0.5, (THUMB as f32 - th) * 0.5);
    let mut px = Vec::with_capacity(THUMB * THUMB * 4);
    for y in 0..THUMB {
        for x in 0..THUMB {
            let (fx, fy) = (x as f32 + 0.5 - ox, y as f32 + 0.5 - oy);
            if fx < 0.0 || fy < 0.0 || fx >= tw || fy >= th {
                px.extend_from_slice(&[30, 30, 30, 255]);
                continue;
            }
            let p = t.sample(fx / tw, fy / th);
            for c in &p[..3] {
                px.push((c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
            px.push(255);
        }
    }
    px
}

/// Function-driven parameters: `fn.<param>="name"` runs that function and stores the result in `<param>`;
/// `fa.<param>.<input>` numbers feed the function's inputs.
fn bind_functions(id: u32, n: &Node, ctx: &Ctx) -> Result<Node, Error> {
    use crate::func::{compile_named, FnEnv};
    let mut out = n.clone();
    for (k, fname) in &n.strs {
        let Some(param) = k.strip_prefix("fn.") else { continue };
        if fname.is_empty() {
            continue;
        }
        let prog = compile_named(ctx.lib, fname).map_err(|e| Error::BadParam(id, format!("{k}: {e}")))?;
        let mut env = FnEnv::new([ctx.size as f64, ctx.height as f64]);
        env.seed = ctx.so;
        env.graph_inputs = ctx.gin;
        let prefix = format!("fa.{param}.");
        for (ak, av) in &n.nums {
            if let Some(input) = ak.strip_prefix(&prefix) {
                let pin = prog.inputs.iter().find(|i| i.0 == input).map(|i| i.1).unwrap_or(crate::graph::Pin::Float);
                env.args.insert(input.to_string(), crate::func::coerce(&crate::func::Value::float(*av as f64), pin));
            }
        }
        let v = prog.run(&env, [0.5, 0.5]).map_err(|e| Error::BadParam(id, format!("{k}: {e}")))?;
        match v {
            crate::func::Value::Str(s) => {
                out.strs.insert(param.to_string(), s.to_string());
            }
            v => {
                out.nums.insert(param.to_string(), v.as_f32());
            }
        }
    }
    Ok(out)
}

/// Pixel processor / value processor driven by a function graph.
fn read_resource(id: u32, file: &str, ctx: &Ctx) -> Result<Vec<u8>, Error> {
    match ctx.res.get(file) {
        Some(b) => Ok(b.as_ref().clone()),
        None => crate::resources::read_file(file).map_err(|e| Error::BadParam(id, e)),
    }
}

fn run_function_image(id: u32, n: &Node, fname: &str, ins: &BTreeMap<String, Rc<Tex>>, ctx: &Ctx, once: bool) -> Result<Tex, Error> {
    use crate::func::{compile_named, FnEnv, Value};
    let prog = compile_named(ctx.lib, fname).map_err(|e| Error::BadParam(id, e))?;
    let (w, h) = (ctx.size, ctx.height);
    let mut env = FnEnv::new([w as f64, h as f64]);
    env.seed = (n.nums.get("seed").copied().unwrap_or(0.0).max(0.0) as u32).wrapping_add(ctx.so);
    env.graph_inputs = ctx.gin;
    for slot in PROCESSOR_INPUTS {
        if let Some(t) = ins.get(slot) {
            env.images.push((slot, t.as_ref()));
        }
    }
    for (ak, av) in &n.nums {
        if let Some(input) = ak.strip_prefix("fa.") {
            let pin = prog.inputs.iter().find(|i| i.0 == input).map(|i| i.1).unwrap_or(crate::graph::Pin::Float);
            env.args.insert(input.to_string(), crate::func::coerce(&Value::float(*av as f64), pin));
        }
    }
    if prog.uses_stats {
        env = env.with_stats();
    }
    let to_px = |v: &Value| -> [f32; 4] {
        match v {
            Value::Num(x) => match x.lanes {
                1 => [x.v[0] as f32, x.v[0] as f32, x.v[0] as f32, 1.0],
                2 => [x.v[0] as f32, x.v[1] as f32, 0.0, 1.0],
                3 => [x.v[0] as f32, x.v[1] as f32, x.v[2] as f32, 1.0],
                _ => [x.v[0] as f32, x.v[1] as f32, x.v[2] as f32, x.v[3] as f32],
            },
            Value::Bool(b) => {
                let f = *b as i32 as f32;
                [f, f, f, 1.0]
            }
            Value::Str(_) => [0.0, 0.0, 0.0, 1.0],
        }
    };
    // the first run reports a broken function as an error instead of a silent black image
    let first = prog.run(&env, [0.5, 0.5]).map_err(|e| Error::BadParam(id, format!("{fname}: {e}")))?;
    if matches!(first, Value::Str(_)) {
        return Err(Error::BadParam(id, format!("{fname}: the result must be a number, vector or boolean")));
    }
    if once {
        return Ok(Tex::solid(w, h, to_px(&first)));
    }
    Ok(Tex::from_fn(w, h, |u, v| match prog.run(&env, [u as f64, v as f64]) {
        Ok(r) => to_px(&r),
        Err(_) => [0.0, 0.0, 0.0, 1.0],
    }))
}

fn run_sub(id: u32, n: &Node, ins: &BTreeMap<String, Rc<Lazy>>, ctx: &Ctx, out: Option<&str>) -> Result<Rc<Lazy>, Error> {
    if ctx.depth >= 8 {
        return Err(Error::BadParam(id, "subgraph nesting too deep (recursive?)".into()));
    }
    let name = n.strs.get("graph").ok_or_else(|| Error::BadParam(id, "subgraph needs graph=\"name\"".into()))?;
    let mut sub = ctx.lib.get(name).cloned().ok_or_else(|| Error::BadParam(id, format!("unknown subgraph '{name}'")))?;
    sub.check_cycles()?;
    for (k, v) in &n.nums {
        if let Some((nid, param)) = k.split_once('.') {
            if let Ok(nid) = nid.parse::<u32>() {
                if let Some(inner) = sub.nodes.get_mut(&nid) {
                    inner.nums.insert(param.to_string(), *v);
                }
            }
        }
    }
    let ext: Vec<Option<Rc<Lazy>>> = SUB_INPUTS.iter().map(|k| ins.get(*k).cloned()).collect();
    let mut outs = sub.output_nodes();
    outs.sort_by_key(|o| OUTPUT_USAGES.iter().position(|x| *x == o.0).unwrap_or(99));
    let want = out.map(String::from).or_else(|| n.strs.get("output").cloned()).unwrap_or_default();
    let found = outs.iter().find(|o| !want.is_empty() && (o.0 == want || o.1 == want));
    if out.is_some() && found.is_none() {
        return Err(Error::BadParam(id, format!("graph '{name}' has no output '{want}'")));
    }
    let out_id = found
        .or_else(|| outs.iter().find(|o| o.0 == "out"))
        .or_else(|| outs.first())
        .map(|o| o.2)
        .ok_or_else(|| Error::BadParam(id, format!("subgraph '{name}' has no output")))?;
    let gin = crate::func::graph_inputs(&sub);
    let c2 = Ctx { size: ctx.size, height: ctx.height, nominal: ctx.nominal, lib: ctx.lib, res: ctx.res, so: ctx.so, depth: ctx.depth + 1, gin: &gin };
    let mut cache = Cache::for_graph(&sub, &[out_id], false);
    // the pictures stay where they are (on the device when the nodes inside ran there)
    let t = eval_res(&sub, out_id, None, &c2, &ext, &mut cache)?;
    cache.map.clear();
    Ok(t)
}

/// The picture at size `sz`: resampled on the device when it lives there, on the host otherwise.
fn lazy_at(l: &Rc<Lazy>, sz: (usize, usize)) -> Result<Rc<Lazy>, Error> {
    if (l.w, l.h) == sz {
        return Ok(l.clone());
    }
    if l.on_device() {
        if let Some(g) = l.dev().and_then(|g| crate::gpu::resize_g(&g, sz.0, sz.1)) {
            return Ok(Lazy::from_gpu(g, None, None));
        }
    }
    let t = l.tex()?;
    Ok(Lazy::from_cpu(Rc::new(ops::resize_to(&t, sz.0, sz.1)), None, None))
}

/// The result of an `input` node: the picture handed in by the instancing node (shared, not copied), as a grey level if asked.
fn input_lazy(n: &Node, ext: &[Option<Rc<Lazy>>], sz: (usize, usize)) -> Result<Rc<Lazy>, Error> {
    let idx = n.nums.get("idx").copied().unwrap_or(0.0).max(0.0) as usize;
    let gray = n.nums.get("gray").copied().unwrap_or(0.0) > 0.5;
    let l = match ext.get(idx).cloned().flatten() {
        Some(l) if !gray => l,
        Some(l) => {
            let dev = if l.on_device() { l.dev().and_then(|g| crate::gpu::point_g(&g, crate::gpu::Point::ToGray)) } else { None };
            match dev {
                Some(g) => Lazy::from_gpu(g, None, None),
                None => {
                    let t = l.tex()?;
                    Lazy::from_cpu(Rc::new(ops::to_gray(&t)), None, None)
                }
            }
        }
        None => match crate::gpu::solid_g(sz.0, sz.1, [0.0, 0.0, 0.0, 1.0]) {
            Some(g) if crate::gpu::worth_it(sz.0, sz.1) => Lazy::from_gpu(g, None, None),
            _ => Lazy::from_cpu(Rc::new(Tex::solid(sz.0, sz.1, [0.0, 0.0, 0.0, 1.0])), None, None),
        },
    };
    lazy_at(&l, sz)
}

/// Applies an explicit output format (8 / 16 bit integer, 16 bit float) to a result: on the device when it is there.
fn lazy_format(l: Rc<Lazy>, fmt: u32, hash: Option<u64>, persist: Option<Arc<NodeCache>>) -> Result<Rc<Lazy>, Error> {
    if fmt != 0 {
        if let Some(g) = l.gpu.get().and_then(|g| crate::gpu::format_g(g, fmt)) {
            return Ok(Lazy::from_gpu(g, hash, persist));
        }
        let mut t = (*l.tex()?).clone();
        ops::apply_format(&mut t, fmt);
        return Ok(Lazy::from_cpu(Rc::new(t), hash, persist));
    }
    match l.gpu.get() {
        Some(g) => Ok(Lazy::from_gpu(g.clone(), hash, persist)),
        None => Ok(Lazy::from_cpu(l.tex()?, hash, persist)),
    }
}

fn compute(id: u32, n: &Node, ins: &BTreeMap<String, Rc<Tex>>, ctx: &Ctx) -> Result<Tex, Error> {
    let f = |k: &str, d: f32| n.nums.get(k).copied().unwrap_or(d);
    let tex = |slot: &str| -> Result<&Tex, Error> {
        ins.get(slot).map(|r| r.as_ref()).ok_or_else(|| Error::MissingInput(id, slot.to_string()))
    };
    let seed = (f("seed", 0.0).max(0.0) as u32).wrapping_add(ctx.so);
    // Tile offset: the whole part picks another tile of the infinite noise plane, the fraction pans the result
    let (tx, ty) = (f("tile_x", 0.0), f("tile_y", 0.0));
    let seed = if tx.floor() != 0.0 || ty.floor() != 0.0 { noise::tile_seed(seed, tx.floor() as i32, ty.floor() as i32) } else { seed };
    let per = |k: &str, d: f32| f(k, d).round().clamp(1.0, 4096.0) as i32;
    let s = ctx.size;
    let hs = ctx.height;
    let rs = s as f32 / 512.0;
    // `square_cells` keeps lattice cells square in a non-square graph: the vertical count follows the aspect ratio
    let ar = if f("square_cells", 0.0) > 0.5 { hs as f32 / s as f32 } else { 1.0 };
    let per_y = |k: &str, d: f32| ((per(k, d) as f32 * ar).round().max(1.0)) as i32;
    // one pixel in uv units: hard edges are softened by at least this much, so they are anti-aliased at any resolution
    let px1 = 1.2 / s as f32;
    let (dis, con, inv) = (f("disorder", 0.0), f("contrast", 1.0), f("invert", 0.0) > 0.5);
    let dz = noise::Disorder::new(seed, dis, f("disorder_speed", 1.0), f("anisotropy", 0.0), f("anisotropy_angle", 0.0));
    let no_warp = dis * f("disorder_speed", 1.0) <= 0.0;
    let warp = move |u: f32, v: f32| -> (f32, f32) { dz.warp(u, v) };
    let fin = |t: Tex| -> Tex {
        if (con - 1.0).abs() > 1e-4 || inv {
            ops::contrast_invert(&t, con, inv)
        } else {
            t
        }
    };
    let out = match n.kind.as_str() {
        "color" => {
            if f("gray", 0.0) > 0.5 {
                let v = f("v", 0.5);
                Tex::solid(s, hs, [v, v, v, f("a", 1.0)])
            } else {
                Tex::solid(s, hs, [f("r", 0.5), f("g", 0.5), f("b", 0.5), f("a", 1.0)])
            }
        }
        "output" => match ins.get("in") {
            Some(t) => (**t).clone(),
            None => Tex::solid(s, hs, output_default(n.strs.get("usage").map(String::as_str).unwrap_or("basecolor"))),
        },
        "dot" => tex("in")?.clone(),
        "white_noise" => {
            // Noise distribution: 0 uniform, 1 gaussian (bell curve), 2 triangle
            let dist = f("distribution", 0.0).round() as u32;
            Tex::gray_fn(s, hs, move |u, v| {
                let (x, y) = ((u * s as f32) as i32, (v * s as f32) as i32);
                match dist {
                    1 => {
                        // mean of four uniforms: a bell curve around 0.5 (std about 0.14)
                        let sum: f32 = (0..4).map(|i| noise::rnd(x, y, seed.wrapping_add(i * 17))).sum();
                        sum / 4.0
                    }
                    2 => (noise::rnd(x, y, seed) + noise::rnd(x, y, seed.wrapping_add(17))) * 0.5,
                    _ => noise::rnd(x, y, seed),
                }
            })
        }
        "gaussian_noise" => {
            let k = f("scale", 1.0).round().max(1.0) as i32;
            fin(Tex::gray_fn(s, hs, move |u, v| {
                // displaced positions wrap around so the noise still tiles
                let (a, b) = warp(u, v);
                let (x, y) = (((a * s as f32).floor() as i32).rem_euclid(s as i32) / k, ((b * s as f32).floor() as i32).rem_euclid(s as i32) / k);
                let sum: f32 = (0..4).map(|i| noise::rnd(x, y, seed.wrapping_add(i * 17))).sum();
                ((sum / 4.0 - 0.5) * 3.46 + 0.5).clamp(0.0, 1.0)
            }))
        }
        "perlin" => {
            let (px, py) = (per("scale_x", f("scale", 4.0)), per_y("scale_y", f("scale", 4.0)));
            match if no_warp { crate::gpu::noise(s, hs, false, px, py, 1, 0.5, seed, 0) } else { None } {
                Some(t) => fin(t),
                None => fin(Tex::gray_fn(s, hs, move |u, v| {
                    let (a, b) = warp(u, v);
                    noise::perlin(a, b, px, py, seed)
                })),
            }
        }
        "fbm" | "clouds" => {
            let cl = n.kind == "clouds";
            let (px, py) = (per("scale_x", f("scale", if cl { 3.0 } else { 4.0 })), per_y("scale_y", f("scale", if cl { 3.0 } else { 4.0 })));
            let (oct, gain, mode) = (f("octaves", if cl { 6.0 } else { 5.0 }).max(1.0) as u32, f("gain", if cl { 0.6 } else { 0.5 }), f("mode", 0.0).max(0.0) as u32);
            match if no_warp { crate::gpu::noise(s, hs, true, px, py, oct, gain, seed, mode) } else { None } {
                Some(t) => fin(t),
                None => {
                    let kernel = noise::Fbm::new(px, py, oct, gain, seed, mode);
                    fin(Tex::gray_fn(s, hs, move |u, v| {
                        let (a, b) = warp(u, v);
                        kernel.sample(a, b)
                    }))
                }
            }
        }
        "bnw_spots" => {
            let (var, p) = (f("variant", 0.0) as u32, per("scale", 8.0));
            fin(Tex::gray_fn(s, hs, move |u, v| {
                let (a, b) = warp(u, v);
                let grain = if var >= 2 { (noise::fbm(u, v, p * 4, p * 4, 3, 0.5, seed.wrapping_add(5), 0) - 0.5) * 0.25 } else { 0.0 };
                noise::spots(a, b, p, p, seed, |id| {
                    let rad = match var {
                        0 => 0.18 + 0.5 * id * id,
                        1 => 0.3 + 0.55 * id,
                        _ => 0.08 + 0.3 * id,
                    };
                    (rad * 0.8, rad)
                }, grain)
            }))
        }
        "crystal" => {
            let (var, p) = (f("variant", 0.0) as u32, per("scale", 6.0));
            fin(Tex::gray_fn(s, hs, move |u, v| {
                let (a, b) = warp(u, v);
                let vr = noise::voronoi_ext(a, b, p, p, seed, 1.0);
                let l = (vr.dx * vr.dx + vr.dy * vr.dy).sqrt().max(1e-4);
                let facet = if var == 0 { 1.0 - vr.f1 } else { 0.5 + 0.5 * ((vr.dx / l) * 0.6 + (vr.dy / l) * 0.8) };
                let edge = smooth(0.0, 0.08, vr.f2 - vr.f1).max(0.2);
                (vr.id1 * 0.55 + facet * 0.45) * edge
            }))
        }
        "directional_noise" => {
            // Streaks = tileable Perlin noise averaged along the streak direction. Every sample is taken from a
            // periodic function at wrapped coordinates, so the result tiles seamlessly at ANY angle.
            let (sc, st, ang) = (f("scale", 6.0), f("stretch", 8.0).max(1.0), f("angle", 0.0));
            let p1 = sc.round().clamp(1.0, 256.0) as i32;
            let (sa, ca) = (ang * std::f32::consts::TAU).sin_cos();
            // a streak never needs to be longer than one tile (longer would wrap and average the noise away)
            let len1 = (st / p1 as f32).min(1.0);
            // 2.5 taps per unit of stretch, but never more than 4 per lattice cell crossed: a long streak over a coarse
            // lattice was sampled up to 50 times per cell of a smooth noise (4 per cell changes no pixel by more than
            // one 8-bit step)
            let k = ((st * 2.5).ceil() as usize).clamp(8, 96).min(((4.0 * len1 * p1 as f32).ceil() as usize).max(8)) | 1;
            let taps: Vec<(f32, f32)> = (0..k)
                .map(|i| {
                    let t = ((i as f32 + 0.5) / k as f32) * 2.0 - 1.0;
                    (t, 1.0 - t.abs())
                })
                .collect();
            let wsum: f32 = taps.iter().map(|t| t.1).sum();
            let gain = 1.6 * st.min(p1 as f32).powf(0.55); // restores the contrast lost by averaging (calibrated)
            let on_gpu = if no_warp { crate::gpu::directional_noise(s, hs, p1, k as u32, ca, sa, len1, gain, seed) } else { None };
            let (g1, g2) = (noise::GradGrid::new(p1, p1, seed), noise::GradGrid::new(p1 * 2, p1 * 2, seed.wrapping_add(7)));
            fin(on_gpu.unwrap_or_else(|| Tex::gray_fn(s, hs, move |u, v| {
                let (u, v) = warp(u, v);
                let (mut a1, mut a2) = (0.0f32, 0.0f32);
                for &(t, w) in &taps {
                    let (o1, o2) = (t * len1 * 0.5, t * len1 * 0.25);
                    a1 += w * g1.sample(u + o1 * ca, v + o1 * sa);
                    a2 += w * g2.sample(u + o2 * ca, v + o2 * sa);
                }
                let m = (a1 * 0.65 + a2 * 0.35) / wsum;
                (0.5 + (m - 0.5) * gain).clamp(0.0, 1.0)
            })))
        }
        "grunge" => {
            let var = f("variant", 0.0) as u32;
            let balance = f("balance", 0.5).clamp(0.0, 1.0);
            // concrete
            let (large, noisy) = (f("large_variation", 0.55), f("base_noisiness", 0.45));
            let (specks, scr_op, sharpen) = (f("dirt_specks_opacity", 0.0).clamp(0.0, 1.0), f("scratches_opacity", 0.0).clamp(0.0, 1.0), f("sharpen", 0.0).clamp(0.0, 1.0));
            let invert_dirt = f("invert_dirt", 0.0) > 0.5;
            // scratches
            let (quant, width, length) = (per("quantity", 3.0).min(32), f("scratch_width", 0.4).clamp(0.05, 0.95), f("scratch_length", 0.45).clamp(0.0, 0.9));
            // spots
            let (details, cover, cover_c) = (per("details", 22.0).min(128), f("coverage", 0.35).clamp(0.0, 0.9), f("coverage_contrast", 0.25).clamp(0.02, 1.0));
            fin(Tex::gray_fn(s, hs, move |u, v| {
                let (a, b) = warp(u, v);
                let val = match var {
                    0 => {
                        let n = noise::fbm(a, b, 8, 8, 6, 0.55, seed, 0) * large + noise::fbm(a, b, 40, 40, 3, 0.5, seed.wrapping_add(1), 0) * noisy;
                        let mut c = ((n - 0.35) * 2.2).clamp(0.0, 1.0);
                        if specks > 0.0 {
                            let sp = noise::spots(a, b, 64, 64, seed.wrapping_add(11), |id| (0.1, 0.18 + id * 0.2), 0.0);
                            c = if invert_dirt { c + (1.0 - c) * sp * specks } else { c * (1.0 - sp * specks) };
                        }
                        if scr_op > 0.0 {
                            let line = noise::perlin(a * 3.0 % 1.0, b, 3, 160, seed.wrapping_add(13));
                            c = (c + smooth(0.62, 0.7, line) * scr_op).clamp(0.0, 1.0);
                        }
                        if sharpen > 0.0 {
                            c += (smooth(0.0, 1.0, c) - c) * sharpen;
                        }
                        c
                    }
                    1 => {
                        let q = quant as f32;
                        let line = noise::perlin(a * q % 1.0, b, quant, 90, seed);
                        let m = noise::fbm(a, b, 5, 5, 4, 0.5, seed.wrapping_add(3), 0);
                        smooth(1.0 - width, 1.1 - width, line) * smooth(length, length + 0.15, m)
                    }
                    2 => {
                        let m = noise::fbm(a, b, 6, 6, 4, 0.5, seed.wrapping_add(4), 0);
                        noise::spots(a, b, details, details, seed, |id| (0.12, 0.2 + id * 0.3), 0.0) * smooth(cover, cover + cover_c, m)
                    }
                    _ => {
                        let n = noise::fbm(a, b, 3, 3, 5, 0.6, seed, 2);
                        smooth(0.2, 0.8, n)
                    }
                };
                (val + (balance - 0.5)).clamp(0.0, 1.0)
            }))
        }
        "grunge_map" => {
            let gp = grunge::GrungeP {
                map: per("map", 1.0).clamp(1, 24) as u32,
                seed,
                balance: f("balance", 0.5).clamp(0.0, 1.0),
                contrast: f("contrast", 0.5).clamp(0.0, 1.0),
                brush: f("brush_pattern", 0.0).clamp(0.0, 1.0),
                cracks: f("cracks", 0.5).clamp(0.0, 1.0),
                disorder: f("disorder", 0.0).clamp(0.0, 1.0),
            };
            let t0 = f("coverage", grunge::T0[(gp.map - 1) as usize]);
            let inv = f("invert", 0.0) > 0.5;
            // the map's noise lattices are built once for the whole image
            let kit = grunge::Kit::new(&gp);
            Tex::gray_fn(s, hs, move |u, v| {
                let x = grunge::grunge_map_with(&kit, &gp, u, v, t0);
                if inv { 1.0 - x } else { x }
            })
        }
        "plasma" => {
            let sc = f("scale", 3.0).round().max(1.0);
            fin(Tex::gray_fn(s, hs, move |u, v| {
                let (a, b) = warp(u, v);
                let t = std::f32::consts::TAU;
                let p = (t * (sc * a + 0.35 * (t * (sc + 1.0) * b).sin())).sin() + (t * (sc * b + 0.35 * (t * (sc + 2.0) * a).sin())).sin() + (t * ((sc + 1.0) * (a + b))).sin();
                (0.5 + p / 6.0).clamp(0.0, 1.0)
            }))
        }
        "stripes" => {
            let (cnt, ang, soft, wd) = (f("count", 8.0).max(1.0), f("angle", 0.0), f("softness", 0.1).max(1e-3), f("width", 0.5));
            // tile=1 (default): snap to the nearest seamlessly tileable direction (integer wave vector).
            // tile=0: exact angle and count, but the pattern only tiles when count*cos/sin are whole numbers.
            let (a, b) = if f("tile", 1.0) > 0.5 {
                let (a, b) = ops::tileable_wave(cnt, ang);
                (a as f32, b as f32)
            } else {
                let (sa, ca) = (ang * std::f32::consts::TAU).sin_cos();
                (cnt.round() * ca, cnt.round() * sa)
            };
            // shift: going down the image the stripes move right by a whole number of stripes over the height
            let b = b - f("shift", 0.0).round();
            let ph = f("phase", 0.0);
            Tex::gray_fn(s, hs, move |u, v| {
                let t = (u * a + v * b - ph).rem_euclid(1.0);
                smooth(0.0, soft, t) * (1.0 - smooth(wd, wd + soft, t))
            })
        }
        "weave" => {
            let (cnt, wd) = ((f("count", 6.0) / 2.0).round().max(1.0) * 2.0, f("width", 0.8).clamp(0.1, 1.0)); // even count: the over/under pattern wraps
            Tex::gray_fn(s, hs, move |u, v| {
                let (x, y) = (u * cnt, v * cnt);
                let (i, j) = (x.floor() as i32, y.floor() as i32);
                let (fx, fy) = (x - i as f32, y - j as f32);
                let hv = (1.0 - ((2.0 * fx - 1.0).abs() / wd).clamp(0.0, 1.0)).sqrt();
                let hh = (1.0 - ((2.0 * fy - 1.0).abs() / wd).clamp(0.0, 1.0)).sqrt();
                if (i + j) & 1 == 0 { hv.max(0.5 * hh) } else { hh.max(0.5 * hv) }
            })
        }
        "shape" => {
            let (k, sx, sy, rot, px, py, til, so) = (f("shape", 1.0) as u32, f("size_x", 0.6), f("size_y", 0.6), f("rotation", 0.0), f("pos_x", 0.0), f("pos_y", 0.0), f("tiling", 1.0), f("softness", 0.02));
            let spec = f("pattern_specific", 0.0);
            Tex::gray_fn(s, hs, move |u, v| ops::shape_value(k, u, v, px, py, sx, sy, rot, til, so, spec))
        }
        "star" => {
            let (pts, inner, size, so, rot) = (f("points", 5.0).round().max(3.0), f("inner", 0.45), f("size", 0.4), f("softness", 0.01).max(px1), f("rotation", 0.0));
            let straight = f("straight", 0.0) > 0.5;
            Tex::gray_fn(s, hs, move |u, v| {
                ops::tiled_max(size > 0.5, ops::wrapd(u - 0.5), ops::wrapd(v - 0.5), |x, y| {
                    let a = y.atan2(x) - rot * std::f32::consts::TAU + std::f32::consts::FRAC_PI_2;
                    let seg = std::f32::consts::TAU / pts;
                    let t = a.rem_euclid(seg) / seg;
                    if straight {
                        // distance to the straight edge from the outer point to the inner corner of this half sector
                        let r = (x * x + y * y).sqrt();
                        let h = (t - 0.5).abs() * seg; // angle from the outer point: 0 there, seg/2 at the inner corner
                        let (px, py) = (r * h.cos(), r * h.sin());
                        let (ox, oy) = (size, 0.0);
                        let (ix, iy) = (size * inner * (seg * 0.5).cos(), size * inner * (seg * 0.5).sin());
                        let (ex, ey) = (ix - ox, iy - oy);
                        let l = (ex * ex + ey * ey).sqrt().max(1e-6);
                        // positive outside (away from the centre)
                        let d = ((px - ox) * ey - (py - oy) * ex) / l;
                        let d = if (0.0 - ox) * ey - (0.0 - oy) * ex > 0.0 { -d } else { d };
                        return 1.0 - smooth(-so, 0.0, d);
                    }
                    let edge = size * inner + (size - size * inner) * (1.0 - (2.0 * t - 1.0).abs());
                    1.0 - smooth(-so, 0.0, (x * x + y * y).sqrt() - edge)
                })
            })
        }
        "gradient_axial" => {
            // direction snapped to a whole wave vector so the symmetric ramp repeats seamlessly
            let (a, b) = ops::tileable_wave(1.0, f("angle", 0.0));
            let (a, b) = (a as f32, b as f32);
            Tex::gray_fn(s, hs, move |u, v| 1.0 - (2.0 * ((u - 0.5) * a + (v - 0.5) * b + 0.5).rem_euclid(1.0) - 1.0).abs())
        }
        "gradient_circular" => {
            // the angular ramp fades to a flat value towards the tile border, so neighbouring tiles meet seamlessly
            let off = f("offset", 0.0);
            Tex::gray_fn(s, hs, move |u, v| {
                let (x, y) = (ops::wrapd(u - 0.5), ops::wrapd(v - 0.5));
                let a = (y.atan2(x) / std::f32::consts::TAU + 0.5 + off).rem_euclid(1.0);
                let w = 1.0 - smooth(0.3, 0.5, x.abs().max(y.abs()));
                0.5 + (a - 0.5) * w
            })
        }
        "voronoi" | "voronoi_fractal" => {
            let fractal = n.kind == "voronoi_fractal";
            let (p, j, mut style) = (per("scale", 6.0), f("jitter", 1.0).clamp(0.0, 1.0), f("mode", 0.0) as u32);
            if !fractal {
                // extra outputs show the same cells as an id map, an edge map and the plain distance
                style = match f("_out", 0.0).round() as i32 { 1 => 1, 2 => 2, 3 => 0, _ => style };
            }
            let aspect = (f("aspect", 1.0) * ar).clamp(0.05, 8.0);
            let (ox, oy) = (f("offset_x", 0.0), f("offset_y", 0.0));
            let (dist, dscale) = (f("distortion", 0.0).clamp(0.0, 2.0), per("distortion_scale", 3.0));
            let metric = noise::Metric::from_index(f("distance_mode", 0.0) as u32, f("minkowski", 2.0));
            let (thick, round, dsc) = (f("edge_thickness", 0.25), f("rounded", 0.0), f("distance_scale", 1.0).clamp(0.0, 8.0));
            let id_seed = if f("color_seed_mode", 0.0) > 0.5 { f("color_seed", 0.0) as u32 } else { seed };
            let (lo, hi) = (f("min_level", 0.0).clamp(0.0, 8.0) as u32, f("max_level", 3.0).clamp(0.0, 8.0) as u32);
            let (rough, lac, opac, blend) = (f("roughness", 0.5).clamp(0.0, 1.5), f("lacunarity", 2.0).clamp(1.0, 4.0), f("opacity", 1.0).clamp(0.0, 1.0), f("blend", 0.0) as u32);
            let hi = hi.max(lo);
            let one = move |u: f32, v: f32, px: i32, sd: u32| -> f32 {
                let py = ((px as f32 * aspect).round().max(1.0)) as i32;
                let (u, v) = (u + ox, v + oy);
                let (u, v) = if dist > 0.0 { (u + (noise::perlin(u, v, dscale, dscale, seed.wrapping_add(5)) - 0.5) * dist * 0.3, v + (noise::perlin(u, v, dscale, dscale, seed.wrapping_add(6)) - 0.5) * dist * 0.3) } else { (u, v) };
                let r = noise::voronoi_gen(u, v, px, py, sd, id_seed.wrapping_add(sd.wrapping_sub(seed)), j, metric);
                (noise::voronoi_style(&r, style, thick, round) * dsc).clamp(0.0, 1.0)
            };
            let levels: Vec<(i32, f32)> = if fractal { (lo..=hi).map(|l| (((p as f32) * lac.powi(l as i32)).round().clamp(1.0, 512.0) as i32, rough.powi((l - lo) as i32))).collect() } else { vec![(p, 1.0)] };
            let norm: f32 = levels.iter().map(|l| l.1).sum::<f32>().max(1e-6);
            fin(Tex::gray_fn(s, hs, move |u, v| {
                if !fractal {
                    return one(u, v, p, seed);
                }
                let mut acc: f32 = if blend == 2 { 1.0 } else { 0.0 };
                for (i, (px, amp)) in levels.iter().enumerate() {
                    let val = one(u, v, *px, seed.wrapping_add(i as u32 * 131));
                    match blend {
                        1 => acc = acc.max(val * if i == 0 { 1.0 } else { opac }),
                        2 => acc = acc.min(1.0 - (1.0 - val) * if i == 0 { 1.0 } else { opac }),
                        _ => acc += amp * val * if i == 0 { 1.0 } else { opac },
                    }
                }
                if blend == 0 { acc / norm } else { acc }.clamp(0.0, 1.0)
            }))
        }
        "gradient" => {
            let (mode, (sa, ca)) = (f("mode", 0.0) as u32, (f("angle", 0.0) * std::f32::consts::TAU).sin_cos());
            // linear mode: direction snapped to a whole wave vector and folded (0 -> 1 -> 0), so it repeats seamlessly
            let (wa, wb) = ops::tileable_wave(1.0, f("angle", 0.0));
            let (wa, wb) = (wa as f32, wb as f32);
            let _ = (sa, ca);
            Tex::gray_fn(s, hs, move |u, v| {
                if mode == 1 {
                    let (x, y) = (ops::wrapd(u - 0.5), ops::wrapd(v - 0.5));
                    ((x * x + y * y).sqrt() * 2.0).clamp(0.0, 1.0)
                } else {
                    1.0 - (2.0 * ((u - 0.5) * wa + (v - 0.5) * wb + 0.5).rem_euclid(1.0) - 1.0).abs()
                }
            })
        }
        "circle" => {
            let (r, so) = (f("radius", 0.4), f("softness", 0.01).max(px1));
            Tex::gray_fn(s, hs, move |u, v| ops::tiled_max(r > 0.5, ops::wrapd(u - 0.5), ops::wrapd(v - 0.5), |x, y| 1.0 - smooth(r - so, r, (x * x + y * y).sqrt())))
        }
        "rect" => {
            let (w, h, so) = (f("width", 0.6), f("height", 0.4), f("softness", 0.01).max(px1));
            Tex::gray_fn(s, hs, move |u, v| {
                let d = (ops::wrapd(u - 0.5).abs() - w * 0.5).max(ops::wrapd(v - 0.5).abs() - h * 0.5);
                1.0 - smooth(-so, 0.0, d)
            })
        }
        "polygon" => {
            let (sd, r, so, rot) = (f("sides", 5.0), f("radius", 0.4), f("softness", 0.01).max(px1), f("rotation", 0.0));
            let (grad, curve) = (f("gradient", 0.0).clamp(0.0, 1.0), f("curve", 0.0).clamp(-1.0, 1.0));
            let ginv = f("gradient_invert", 0.0) > 0.5;
            let til = f("tiling", 1.0).round().clamp(1.0, 16.0);
            let apo = (r * (std::f32::consts::PI / sd.round().max(3.0)).cos()).max(1e-6);
            Tex::gray_fn(s, hs, move |u, v| {
                let (u, v) = ((u * til).rem_euclid(1.0), (v * til).rem_euclid(1.0));
                ops::tiled_max(r > 0.5, ops::wrapd(u - 0.5), ops::wrapd(v - 0.5), |x, y| {
                    let d = ops::polygon_sdf(x, y, sd, r, rot);
                    let hard = 1.0 - smooth(-so, 0.0, d);
                    if grad <= 0.0 {
                        return hard;
                    }
                    // a ramp from the edge (0) to the centre (1), bent by the curve
                    let ramp = (-d / apo).clamp(0.0, 1.0);
                    let ramp = if ginv { 1.0 - ramp } else { ramp };
                    let ramp = if curve.abs() > 1e-4 { ramp.powf(2f32.powf(-curve * 2.0)) } else { ramp };
                    hard * (1.0 - grad + grad * ramp)
                })
            })
        }
        "checker" => {
            let t = (f("tiles", 8.0) / 2.0).round().max(1.0) * 2.0; // even count so the pattern wraps
            let (white_first, r45, lv) = (f("white_first", 0.0) > 0.5, f("rotate45", 0.0) > 0.5, f("lum_var", 0.0).clamp(0.0, 1.0));
            // anti-aliased: blend across a cell border over about one pixel
            let aa = (t / s as f32 * if r45 { std::f32::consts::SQRT_2 * 0.5 } else { 1.0 }).max(1e-6);
            Tex::gray_fn(s, hs, move |u, v| {
                // turned by 45 degrees: diamonds, still wrapping (a whole number of cells along both diagonals)
                let (x, y) = if r45 { ((u + v) * t * 0.5, (v - u) * t * 0.5) } else { (u * t, v * t) };
                let (fx, fy) = (x - x.floor(), y - y.floor());
                let (ix, iy) = (x.floor() as i64, y.floor() as i64);
                // signed distances (in cells) to the nearest vertical / horizontal border and which side we are on
                let sx = ((fx.min(1.0 - fx)) / aa).min(1.0) * 0.5 + 0.5; // 0.5 at the border .. 1 inside
                let sy = ((fy.min(1.0 - fy)) / aa).min(1.0) * 0.5 + 0.5;
                let base = (((ix + iy) & 1) ^ white_first as i64) as f32;
                // mix towards the average (0.5) at the border: at a border the neighbour cell has the opposite value
                let kx = 1.0 - sx; // 0 inside .. 0.5 at the border
                let ky = 1.0 - sy;
                let v1 = base + (1.0 - 2.0 * base) * kx; // moves towards the neighbour value across x
                let v2 = v1 + (1.0 - 2.0 * v1) * ky;
                if lv <= 0.0 {
                    return v2.clamp(0.0, 1.0);
                }
                // white squares get a random luminance; near a border the white neighbour's value is used
                let (wx, wy) = if base > 0.5 {
                    (ix, iy)
                } else if fx.min(1.0 - fx) < fy.min(1.0 - fy) {
                    (if fx < 0.5 { ix - 1 } else { ix + 1 }, iy)
                } else {
                    (ix, if fy < 0.5 { iy - 1 } else { iy + 1 })
                };
                let n = t as i64;
                let r = crate::noise_styles::unit(wx.rem_euclid(n) as i32, wy.rem_euclid(n) as i32, seed);
                (v2 * (1.0 - lv * r)).clamp(0.0, 1.0)
            })
        }
        "bricks" => {
            let (r, c, m, b, o, lv) = (f("rows", 8.0), f("cols", 4.0), f("mortar", 0.03), f("bevel", 0.05), f("offset", 0.5), f("lum_var", 0.3));
            let b = b.max(1.5 * r.max(c) / s as f32); // edge ramps are at least about a pixel wide (anti-aliased)
            Tex::gray_fn(s, hs, move |u, v| ops::bricks(u, v, r, c, m, b, o, lv, seed))
        }
        "tile_sampler" | "tile_generator" => {
            let gen = n.kind == "tile_generator";
            let p = tile_params(n, gen, ar, seed);
            let pats = scatter_patterns(ins, f("pattern_type", 0.0) as u32, if gen { 1 } else { 2 }, f("pattern_distribution", 0.0) as u32);
            let g = |k: &str| ins.get(k).map(|t| t.as_ref());
            let maps = scatter::TileMaps { mask: g("mask"), scale: g("scale_map"), rotation: g("rotation_map"), displacement: g("displacement_map"), color: g("color_map"), distribution: g("distribution_map"), background: g("background") };
            scatter::tile_grid(s, s, &pats, &maps, &p)
        }
        "splatter" => {
            let p = splatter_params(n, seed);
            let pats = scatter_patterns(ins, f("pattern_type", 0.0) as u32, 99, f("pattern_distribution", 0.0) as u32);
            match f("_out", 0.0).round() as i32 {
                1 => scatter::splatter_data(s, hs, &pats, &p, scatter::SplatData::Id),
                2 => scatter::splatter_data(s, hs, &pats, &p, scatter::SplatData::Center),
                3 => scatter::splatter_data(s, hs, &pats, &p, scatter::SplatData::Size),
                4 => scatter::splatter_data(s, hs, &pats, &p, scatter::SplatData::Luminance),
                _ => scatter::splatter(s, s, &pats, &p),
            }
        }
        "splatter_circular" => {
            let p = circular_params(n, seed);
            let pats = scatter_patterns(ins, f("pattern_type", 0.0) as u32, 2, f("pattern_distribution", 0.0) as u32);
            scatter::splatter_circular(s, s, &pats, &p)
        }
        "switch" => {
            let order = ["a", "b", "c", "d"];
            let idx = f("index", 0.0).max(0.0) as usize;
            match ins.get(order[idx.min(3)]) {
                Some(t) => (**t).clone(),
                None => order.iter().find_map(|k| ins.get(*k)).map(|t| (**t).clone()).unwrap_or_else(|| Tex::solid(s, hs, [0.0, 0.0, 0.0, 1.0])),
            }
        }
        "flood_fill" => ops::flood_fill(tex("in")?, f("threshold", 0.5)),
        "flood_fill_to_index" => ops::flood_fill_to_index(tex("in")?),
        "flood_fill_to_position" | "flood_fill_to_bbox_size" => {
            let g = ops::flood_fill_to_geometry(tex("in")?, n.kind == "flood_fill_to_bbox_size");
            match f("_out", 0.0).round() as i32 {
                1 => g.map(|p| [p[0], p[0], p[0], 1.0]),
                2 => g.map(|p| [p[1], p[1], p[1], 1.0]),
                _ => g,
            }
        }
        "flood_fill_random" => ops::flood_fill_random(tex("in")?, seed, f("min", 0.0), f("max", 1.0), f("contrast", 0.0)),
        "flood_fill_color" => ops::flood_fill_color(tex("in")?, seed),
        "flood_fill_gradient" => ops::flood_fill_gradient(tex("in")?, f("angle", 0.0), f("variation", 0.0), seed),
        "vector_warp" => {
            let (t, vm, inten, dx) = (tex("in")?, tex("vector")?, f("intensity", 0.05), f("directx", 0.0) > 0.5);
            let gs = if dx { -1.0 } else { 1.0 };
            crate::gpu::warp(t, vm, crate::gpu::WarpKind::Vector { gs }, inten, 2).unwrap_or_else(|| ops::vector_warp_ex(t, vm, inten, dx))
        }
        "slope_blur" => {
            let (t, sl) = (tex("in")?, tex("slope")?);
            let (n, inten, md) = (f("samples", 16.0) as u32, f("intensity", 0.1), f("mode", 0.0) as u32);
            crate::gpu::slope_blur(t, sl, n, inten, md).unwrap_or_else(|| ops::slope_blur(t, sl, n, inten, md))
        }
        "blur_hq" => {
            let q = f("quality", 1.0).round().clamp(1.0, 4.0) as u32;
            let mut t = tex("in")?.clone();
            let sigma = f("intensity", 6.0) * rs / (q as f32).sqrt();
            for _ in 0..q {
                t = ops::blur(&t, sigma);
            }
            t
        }
        "histogram_range" => {
            let (pos, range, con2, inv2) = (f("position", 0.5), f("range", 0.2).max(1e-4), f("contrast", 0.5), f("invert", 0.0) > 0.5);
            let soft = (1.0 - con2.clamp(0.0, 1.0)) * range * 0.5 + 1e-4;
            let (lo, hi) = (pos - range * 0.5, pos + range * 0.5);
            tex("in")?.map(move |p| {
                let l = lum(p);
                let g = smooth(lo - soft, lo + soft, l) * (1.0 - smooth(hi - soft, hi + soft, l));
                let g = if inv2 { 1.0 - g } else { g };
                [g, g, g, 1.0]
            })
        }
        "bevel" => {
            let b = ops::bevel_ex(tex("in")?, f("distance", 0.05) * s as f32, f("smoothing", 0.5), f("threshold", 0.5), f("corner", 1.0) < 0.5);
            if f("use_as_normal", 0.0) > 0.5 || f("_out", 0.0) >= 0.5 {
                ops::normal_from_height(&b, f("normal_intensity", 4.0), f("directx", 0.0) > 0.5)
            } else {
                b
            }
        }
        "distance" => ops::distance(tex("in")?, f("distance", 0.1) * s as f32, f("threshold", 0.5), f("mode", 0.0) as u32),
        "color_match" if f("method", 0.0) < 0.5 => {
            if f("_out", 0.0) >= 0.5 {
                Tex::solid(s, hs, [1.0; 4])
            } else {
                ops::color_match(tex("in")?, ins.get("reference").map(|r| r.as_ref()).unwrap_or(tex("in")?), f("strength", 1.0))
            }
        }
        "color_match" => {
            let src = tex("in")?;
            let rgb = |name: &str, d: f32| [f(&format!("{name}_r"), d), f(&format!("{name}_g"), d), f(&format!("{name}_b"), d)];
            let source = match f("source_mode", 0.0).round() as i32 {
                1 => rgb("src", 0.5),
                2 => ops::mean_rgb(tex("src_color")?),
                _ => ops::mean_rgb(src),
            };
            let target = if f("target_mode", 0.0) > 0.5 { ops::mean_rgb(tex("target_color")?) } else { rgb("target", 0.5) };
            let var = f("custom_variation", 0.0) > 0.5;
            let sw = ops::ColorSwap {
                source,
                target,
                hue_shift: if var { f("variation_hue", 0.0) / 360.0 } else { 0.0 },
                chroma_mul: if var { f("variation_chroma", 1.0).max(0.0) } else { 1.0 },
                luma_mul: if var { f("variation_luma", 1.0).max(0.0) } else { 1.0 },
                use_mask: f("use_mask", 1.0) > 0.5,
                hue_range: f("mask_hue_range", 30.0) / 360.0,
                chroma_range: f("mask_chroma_range", 0.5),
                luma_range: f("mask_luma_range", 0.5),
                smooth: f("mask_smoothness", 0.3),
                strength: f("strength", 1.0),
            };
            let (res, mask) = ops::color_swap(src, &sw);
            if f("_out", 0.0) >= 0.5 { mask } else { res }
        }
        "curvature_smooth" => {
            let src = tex("in")?;
            let flipped;
            let src = if f("directx", 0.0) > 0.5 {
                flipped = ops::flip_green(src);
                &flipped
            } else {
                src
            };
            let c = ops::curvature_from_normal(src, f("smoothing", 2.0) * rs, f("intensity", 6.0));
            // outputs: curvature (0.5 is flat), convexity (the positive half) and concavity (the negative half)
            match f("_out", 0.0).round() as i32 {
                1 => c.map(|p| {
                    let v = ((p[0] - 0.5) * 2.0).clamp(0.0, 1.0);
                    [v, v, v, 1.0]
                }),
                2 => c.map(|p| {
                    let v = ((0.5 - p[0]) * 2.0).clamp(0.0, 1.0);
                    [v, v, v, 1.0]
                }),
                _ => c,
            }
        }
        "height_blend" => {
            let (h, m) = ops::height_blend(tex("bg")?, tex("fg")?, ins.get("mask").map(|x| x.as_ref()), f("opacity", 1.0), f("contrast", 0.5));
            if f("output", 0.0) > 0.5 { m } else { h }
        }
        "make_it_tile_photo" => {
            let w = f("width", 0.6);
            let tp = ops::TilePhoto { width: [w, f("width_v", w)], sharp: [f("sharp", 0.0), f("sharp_v", 0.0)], warp: [f("warp", 0.0), f("warp_v", 0.0)], seed };
            ops::make_tile_photo_full(tex("in")?, tp)
        }
        "make_it_tile_patch" => {
            if f("mode", 0.0) > 0.5 {
                ops::make_tile_patch_stamped(tex("in")?, &tile_patch_params(&f, seed))
            } else {
                ops::make_tile_patch(tex("in")?, f("patches", 6.0), f("softness", 0.5), seed)
            }
        }
        "clamp" => {
            let (lo, hi) = (f("low", 0.0), f("high", 1.0));
            let ca = f("apply_alpha", 0.0) > 0.5;
            tex("in")?.map(move |p| [p[0].clamp(lo.min(hi), hi.max(lo)), p[1].clamp(lo.min(hi), hi.max(lo)), p[2].clamp(lo.min(hi), hi.max(lo)), if ca { p[3].clamp(lo.min(hi), hi.max(lo)) } else { p[3] }])
        }
        "highpass" => ops::highpass(tex("in")?, f("radius", 6.0) * rs),
        "swirl" => ops::swirl(tex("in")?, f("amount", 0.3), f("radius", 0.5)),
        "edge_wear" | "dirt" | "dust" => {
            let base = tex("in")?;
            let amount = f("amount", 0.5).clamp(0.0, 1.0);
            let soft = 0.25 / f("contrast", 1.0).max(0.1);
            let thr = 0.5 + (1.0 - amount) * 0.35;
            let mut m = match n.kind.as_str() {
                "edge_wear" => {
                    let c = ops::curvature(base, f("width", 2.0) * rs, 6.0);
                    ops::threshold(&c, thr, soft)
                }
                "dirt" => {
                    let inv = ops::invert(&ops::to_gray(base));
                    ops::threshold(&inv, 1.0 - amount * 0.6, soft)
                }
                _ => ops::threshold(&ops::channel(base, 1), 0.95 - amount * 0.45, soft),
            };
            if let Some(nz) = ins.get("noise") {
                for (i, p) in m.px.iter_mut().enumerate() {
                    let k = lum(nz.px[i]);
                    p[0] *= k;
                    p[1] *= k;
                    p[2] *= k;
                }
            }
            m
        }
        "fx_map" => {
            let name = n.strs.get("program").ok_or_else(|| Error::BadParam(id, "fx_map needs program=\"name\"".into()))?;
            let prog = ctx.lib.get(name).ok_or_else(|| Error::BadParam(id, format!("unknown pattern program '{name}'")))?;
            if !crate::fxmap::is_program(prog) {
                return Err(Error::BadParam(id, format!("'{name}' is not a pattern program")));
            }
            let pats: Vec<Option<&Tex>> = FX_MAP_IMAGES.iter().map(|k| ins.get(*k).map(|t| t.as_ref())).collect();
            let color = f("color", 0.0) > 0.5;
            let bg = f("bg", 0.0);
            let bgc = if color { [f("bg_r", bg), f("bg_g", bg), f("bg_b", bg)] } else { [bg, bg, bg] };
            let mut run = crate::fxmap::Run::new(f("depth", 12.0).clamp(0.0, 32.0) as u32, seed, f("blend", 0.0) as u32, color, &pats);
            run.lib = Some(ctx.lib);
            run.graph_inputs = Some(ctx.gin);
            run.roughness = f("roughness", 1.0).max(0.0);
            run.opacity = f("opacity", 1.0);
            run.background = ins.get("background").map(|t| t.as_ref());
            crate::fxmap::render(prog, s, hs, bgc, &run).map_err(|e| Error::BadParam(id, format!("{name}: {e}")))?
        }
        "curve" => {
            let get = |k: &str| -> Result<crate::curve::Curve, Error> {
                match n.strs.get(k) {
                    Some(t) if !t.trim().is_empty() => crate::curve::Curve::parse(t).map_err(|m| Error::BadParam(id, format!("{k}: {m}"))),
                    _ => Ok(crate::curve::Curve::identity()),
                }
            };
            let (m, r, g, b, a) = (get("points")?, get("points_r")?, get("points_g")?, get("points_b")?, get("points_a")?);
            crate::curve::apply(tex("in")?, &m, [&r, &g, &b, &a])
        }
        "color_lut" => {
            let file = n.strs.get("file").filter(|f| !f.is_empty()).ok_or_else(|| Error::BadParam(id, "color_lut needs file=\"grade.cube\"".into()))?;
            let bytes = read_resource(id, file, ctx)?;
            let lut = crate::lut::Lut::load(file, &bytes).map_err(|m| Error::BadParam(id, format!("'{file}': {m}")))?;
            let src = ins.get("in").ok_or_else(|| Error::MissingInput(id, "in".into()))?;
            lut.apply_tex(src, f("intensity", 1.0).clamp(0.0, 1.0))
        }
        "svg" => {
            let file = n.strs.get("file").ok_or_else(|| Error::BadParam(id, "svg needs file=\"art.svg\"".into()))?;
            let bytes = read_resource(id, file, ctx)?;
            let src = String::from_utf8_lossy(&bytes);
            crate::vector::render_svg(&src, s, hs, f("fit", 0.0) as u32, f("scale", 1.0).max(0.001), f("x", 0.0), f("y", 0.0)).map_err(|m| Error::BadParam(id, format!("'{file}': {m}")))?
        }
        "text" => {
            let font = match n.strs.get("font").filter(|f| !f.is_empty()) {
                Some(file) => crate::font::Font::load(&read_resource(id, file, ctx)?).map_err(|m| Error::BadParam(id, format!("'{file}': {m}")))?,
                // an installed family (as Substance Designer's Text node names it); the built-in face when it is not installed
                None => n
                    .strs
                    .get("font_family")
                    .filter(|f| !f.trim().is_empty())
                    .and_then(|fam| crate::font::system_font(fam))
                    .and_then(|p| std::fs::read(p).ok())
                    .and_then(|b| crate::font::Font::load(&b).ok())
                    .unwrap_or(crate::font::Font::Builtin),
            };
            let mut text = n.strs.get("text").cloned().unwrap_or_default();
            let size_px = f("size", 0.2).max(0.001) * hs as f32;
            let wrap = f("wrap", 0.0);
            if wrap > 0.001 {
                text = font.wrap(&text, wrap * s as f32 / size_px, f("spacing", 0.0));
            }
            let xf = crate::vector::Xf::rotate(f("rotation", 0.0).to_radians()).then(crate::vector::Xf::translate((0.5 + f("x", 0.0)) * s as f32, (0.5 + f("y", 0.0)) * hs as f32));
            let polys = font.layout(&text, f("size", 0.2).max(0.001) * hs as f32, f("align", 1.0) as u32, f("spacing", 0.0), f("line_spacing", 1.0), &xf);
            let mask = crate::vector::fill(&polys, s, hs, false);
            let v = f("value", 1.0);
            let col = if f("use_color", 0.0) > 0.5 { [f("color_r", 1.0), f("color_g", 1.0), f("color_b", 1.0)] } else { [1.0; 3] };
            let mut t = Tex::solid(s, hs, [0.0, 0.0, 0.0, 1.0]);
            for (p, c) in t.px.iter_mut().zip(&mask.a) {
                let g = c * v;
                *p = [g * col[0], g * col[1], g * col[2], 1.0];
            }
            t
        }
        "value_processor" => match n.strs.get("fn") {
            Some(fname) => return run_function_image(id, n, fname, ins, ctx, true),
            // no function: the value is the type's default (0), as an empty processor gives in the file's own editor
            None => Tex::solid(ctx.size, ctx.height, [0.0, 0.0, 0.0, 1.0]),
        },
        "input_value" => {
            let v = f("v", 0.0);
            Tex::solid(ctx.size, ctx.height, [v, v, v, 1.0])
        }
        "pixel_processor" => {
            if let Some(fname) = n.strs.get("fn") {
                return run_function_image(id, n, fname, ins, ctx, false);
            }
            let parse = |k: &str| -> Result<Option<expr::Ex>, Error> {
                match n.strs.get(k) {
                    Some(src) => expr::parse(src).map(Some).map_err(|m| Error::BadParam(id, format!("{k}: {m}"))),
                    None => Ok(None),
                }
            };
            let e0 = parse("expr")?.ok_or_else(|| Error::BadParam(id, "missing expr".into()))?;
            let (eg, eb, ea) = (parse("expr_g")?, parse("expr_b")?, parse("expr_a")?);
            let input = ins.get("in").map(|t| t.as_ref());
            Tex::from_fn(s, hs, |u, v| {
                let c = input.map(|t| t.sample(u, v)).unwrap_or([0.0, 0.0, 0.0, 1.0]);
                let env = expr::Env { vars: [u, v, u * s as f32, v * s as f32, s as f32, s as f32, c[0], c[1], c[2], lum(c)], tex: input };
                let r = expr::eval(&e0, &env);
                let g = eg.as_ref().map(|e| expr::eval(e, &env)).unwrap_or(r);
                let b = eb.as_ref().map(|e| expr::eval(e, &env)).unwrap_or(r);
                let a = ea.as_ref().map(|e| expr::eval(e, &env)).unwrap_or(1.0);
                [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0), a.clamp(0.0, 1.0)]
            })
        }
        "sdf_shape" => {
            let (shape, size, aspect, sides, rot, round) = (f("shape", 0.0) as u32, f("size", 0.25), f("aspect", 1.0), f("sides", 5.0), f("rotation", 0.0), f("round", 0.0));
            let (cx, cy, range) = (f("x", 0.0), f("y", 0.0), f("range", 0.25));
            Tex::gray_fn(s, hs, move |u, v| ops::sdf_to_value(ops::sdf2d(shape, ops::wrapd(u - 0.5 - cx), ops::wrapd(v - 0.5 - cy), size, aspect, sides, rot, round), range))
        }
        "sdf_op" => {
            let (op, k, range) = (f("op", 0.0) as u32, f("smooth", 0.0), f("range", 0.25));
            let (a, b) = (tex("a")?, tex("b")?);
            Tex::gray_fn(s, hs, move |u, v| {
                let (da, db) = (ops::value_to_sdf(lum(a.sample(u, v)), range), ops::value_to_sdf(lum(b.sample(u, v)), range));
                ops::sdf_to_value(ops::sdf_combine(op, da, db, k), range)
            })
        }
        "sdf_mask" => {
            let (off, soft) = (f("offset", 0.0), f("softness", 0.02).max(1e-4));
            let a = tex("in")?;
            Tex::gray_fn(s, hs, move |u, v| smooth(0.5 + off - soft * 0.5, 0.5 + off + soft * 0.5, lum(a.sample(u, v))))
        }
        "sdf_form" => {
            let (shape, size, rep, round, scatter, hgt) = (f("shape", 0.0) as u32, f("size", 0.3), f("repeat", 3.0), f("round", 0.05), f("scatter", 0.3), f("height", 1.5));
            Tex::gray_fn(s, hs, move |u, v| ops::sdf_form(u, v, shape, size, rep, round, scatter, seed, hgt))
        }
        "baked_map" => {
            let (set, map) = (n.strs.get("set").map(|v| v.trim()).unwrap_or(""), n.strs.get("map").map(|v| v.trim()).unwrap_or(""));
            let kind = crate::bake::MapKind::from_id(map).ok_or_else(|| Error::BadParam(id, format!("unknown baked map '{map}'")))?;
            let key = crate::bake::resource_key(set, kind);
            let bytes = ctx.res.get(&key).ok_or_else(|| Error::BadParam(id, format!("the package has no baked '{}' map for set '{set}': bake it with the mesh baker (Tools > Mesh map baker)", kind.id())))?;
            let mut t = png::decode(bytes).map_err(|e| Error::BadParam(id, format!("'{key}': {e}")))?;
            if t.w != s || t.h != hs {
                let src = t;
                t = Tex::from_fn(s, hs, |u, v| src.sample(u, v));
            }
            if f("flip_y", 0.0) > 0.5 && kind.is_colour() {
                for p in &mut t.px {
                    p[1] = 1.0 - p[1];
                }
            }
            if f("invert", 0.0) > 0.5 {
                for p in &mut t.px {
                    for k in 0..3 {
                        p[k] = 1.0 - p[k];
                    }
                }
            }
            if f("gray", 0.0) > 0.5 || !kind.is_colour() {
                t = ops::to_gray(&t);
            }
            t
        }
        "bitmap" => {
            let file = n.strs.get("file").ok_or_else(|| Error::BadParam(id, "bitmap needs file".into()))?;
            let bytes = match ctx.res.get(file) {
                Some(b) => b.clone(),
                // graphs written on Windows use backslashes; accept them on every platform
                None => Arc::new(crate::resources::read_file(file).map_err(|e| Error::BadParam(id, e))?),
            };
            let mut t = crate::imgio::decode(file, &bytes).map_err(|e| Error::BadParam(id, format!("'{file}': {e}")))?;
            if f("flip_x", 0.0) > 0.5 || f("flip_y", 0.0) > 0.5 {
                let (fx, fy) = (f("flip_x", 0.0) > 0.5, f("flip_y", 0.0) > 0.5);
                let src = t.clone();
                for y in 0..t.h {
                    for x in 0..t.w {
                        let (sx, sy) = (if fx { t.w - 1 - x } else { x }, if fy { t.h - 1 - y } else { y });
                        t.px[y * t.w + x] = src.px[sy * t.w + sx];
                    }
                }
            }
            if f("gray", 0.0) > 0.5 {
                t = ops::to_gray(&t);
            }
            t
        }
        "channel_shuffle" => {
            // each output channel picks a source: 0-3 channels of `in`, 4-7 channels of `in2`, 8 zero, 9 one
            let a = tex("in")?;
            let b = ins.get("in2").map(|t| t.as_ref()).unwrap_or(a);
            let pick = |k: &str, d: f32| f(k, d).round().clamp(0.0, 9.0) as usize;
            let src = [pick("src_r", 0.0), pick("src_g", 1.0), pick("src_b", 2.0), pick("src_a", 3.0)];
            let inv = [f("inv_r", 0.0) > 0.5, f("inv_g", 0.0) > 0.5, f("inv_b", 0.0) > 0.5, f("inv_a", 0.0) > 0.5];
            let mut out = a.clone();
            for (i, p) in out.px.iter_mut().enumerate() {
                for k in 0..4 {
                    let v = match src[k] {
                        0..=3 => a.px[i][src[k]],
                        4..=7 => b.px[i][src[k] - 4],
                        8 => 0.0,
                        _ => 1.0,
                    };
                    p[k] = if inv[k] { 1.0 - v } else { v };
                }
            }
            out
        }
        "blur" => {
            let (src, sg) = (tex("in")?, f("radius", 4.0) * rs);
            crate::gpu::blur(src, sg).unwrap_or_else(|| ops::blur(src, sg))
        }
        "directional_blur" => {
            let (src, a, l) = (tex("in")?, f("angle", 0.0), f("length", 0.05));
            crate::gpu::directional_blur(src, a, l).unwrap_or_else(|| ops::directional_blur(src, a, l))
        }
        "sharpen" => ops::sharpen(tex("in")?, f("radius", 2.0) * rs, f("amount", 1.0)),
        "levels" => {
            let t = tex("in")?;
            let (mut lo, mut hi) = (f("low", 0.0), f("high", 1.0));
            if f("auto", 0.0) > 0.5 {
                let (mn, mx) = t.px.iter().fold((1.0f32, 0.0f32), |a, p| (a.0.min(lum(*p)), a.1.max(lum(*p))));
                lo = mn;
                hi = mx.max(mn + 1e-4);
            }
            let (gm, ol, oh) = (f("gamma", 1.0), f("out_low", 0.0), f("out_high", 1.0));
            crate::gpu::levels(t, lo, hi, gm, ol, oh).unwrap_or_else(|| ops::levels(t, lo, hi, gm, ol, oh))
        }
        "adjust" => ops::adjust(tex("in")?, f("brightness", 0.0), f("contrast", 1.0)),
        "invert" => if f("invert", 1.0) > 0.5 { ops::invert(tex("in")?) } else { tex("in")?.clone() },
        "threshold" => ops::threshold_mode(tex("in")?, f("threshold", 0.5), f("softness", 0.0), f("mode", 0.0).max(0.0) as u32),
        "histogram_scan" => {
            let src = tex("in")?;
            let (pos, con) = (f("position", 0.5), f("contrast", 0.5));
            let pm = ins.get("position_map").filter(|_| f("use_position_map", 1.0) > 0.5);
            let cm = ins.get("contrast_map").filter(|_| f("use_contrast_map", 1.0) > 0.5);
            let t = if pm.is_none() && cm.is_none() {
                ops::histogram_scan(src, pos, (1.0 - con.clamp(0.0, 1.0)).max(1e-4))
            } else {
                // non-uniform: the position and contrast change across the image
                Tex::from_fn(src.w, src.h, |u, v| {
                    let p = pm.map(|m| m.sample(u, v)[0]).unwrap_or(pos);
                    let w = (1.0 - cm.map(|m| m.sample(u, v)[0]).unwrap_or(con).clamp(0.0, 1.0)).max(1e-4);
                    let g = ((lum(src.sample(u, v)) - (p - w * 0.5)) / w).clamp(0.0, 1.0);
                    [g, g, g, 1.0]
                })
            };
            if f("invert", 0.0) > 0.5 { ops::invert(&t) } else { t }
        }
        "quantize" => ops::quantize(tex("in")?, f("steps", 4.0)),
        "to_gray" => ops::to_gray(tex("in")?),
        "channel" => ops::channel(tex("in")?, f("ch", 0.0).max(0.0) as usize),
        "merge" => {
            let g = |slot: &str, ch: usize| ins.get(slot).map(|t| ops::channel(t, ch));
            // the alpha is the grey value of its input (its luminance), not that input's own alpha (1 for a grey map)
            let (r, gg, b, a) = (g("r", 0), g("g", 1), g("b", 2), ins.get("a").map(|t| ops::to_gray(t)));
            Tex::from_fn(s, hs, |u, v| {
                let q = |t: &Option<Tex>, d: f32| t.as_ref().map(|t| t.sample(u, v)[0]).unwrap_or(d);
                [q(&r, 0.0), q(&gg, 0.0), q(&b, 0.0), q(&a, 1.0)]
            })
        }
        "tint" => {
            let c = [f("r", 1.0), f("g", 1.0), f("b", 1.0)];
            tex("in")?.map(move |p| [p[0] * c[0], p[1] * c[1], p[2] * c[2], p[3]])
        }
        "colorize" => {
            let mut stops = Vec::new();
            for i in 0..16 {
                if let Some(p) = n.nums.get(&format!("p{i}")) {
                    let g = |c: &str| n.nums.get(&format!("{c}{i}")).copied().unwrap_or(0.0);
                    stops.push((*p, [g("r"), g("g"), g("b")], n.nums.get(&format!("a{i}")).copied().unwrap_or(1.0)));
                }
            }
            if stops.is_empty() {
                return Err(Error::BadParam(id, "colorize needs at least one stop (p0 r0 g0 b0)".into()));
            }
            ops::colorize_ex(tex("in")?, &stops, f("interp", 0.0) as u32, f("smoothness", 1.0), f("addressing", 0.0) > 0.5, f("use_alpha", 0.0) > 0.5)
        }
        "hsl" => ops::hsl_adjust(tex("in")?, f("hue", 0.0), f("sat", 1.0), f("val", 1.0)),
        "transform" => ops::transform(tex("in")?, f("offset_x", 0.0), f("offset_y", 0.0), f("scale_x", 1.0), f("scale_y", 1.0), f("rotation", 0.0), f("tiling", 0.0) < 0.5),
        "mirror" => ops::mirror_ex(tex("in")?, f("axis", 0.0) as u32, f("offset_x", 0.5), f("offset_y", 0.5), f("invert_x", 0.0) > 0.5, f("invert_y", 0.0) > 0.5, f("corner", 0.0) as u32),
        "warp" => {
            let (t, w, inten, fl) = (tex("in")?, tex("warp")?, f("intensity", 0.05), f("filtering", 2.0).max(0.0) as u32);
            // the GPU measures the slope per pixel: scaled to 1/256 of the image like the CPU path
            crate::gpu::warp(t, w, crate::gpu::WarpKind::Slope, inten * w.w as f32 / 256.0, fl).unwrap_or_else(|| ops::warp_ex(t, w, inten, fl))
        }
        "directional_warp" => {
            let (t, w, ang, inten, off, fl) = (tex("in")?, tex("warp")?, f("angle", 0.0), f("intensity", 0.1), f("offset", 0.5), f("filtering", 2.0).max(0.0) as u32);
            let (s, c) = (ang * std::f32::consts::TAU).sin_cos();
            crate::gpu::warp(t, w, crate::gpu::WarpKind::Directional { c, s, offset: off }, inten, fl).unwrap_or_else(|| ops::directional_warp_ex(t, w, ang, inten, off, fl))
        }
        "blend" => {
            let (fg, bg, mask) = (tex("fg")?, tex("bg")?, ins.get("mask").map(|m| m.as_ref()));
            let (mode, op, am) = (f("mode", 0.0).max(0.0) as u32, f("opacity", 1.0), f("alpha_mode", 1.0).max(0.0) as u32);
            let crop = [f("crop_x0", 0.0), f("crop_y0", 0.0), f("crop_x1", 1.0), f("crop_y1", 1.0)];
            let on_gpu = if crop == [0.0, 0.0, 1.0, 1.0] { crate::gpu::blend(fg, bg, mask, mode, op, am) } else { None };
            on_gpu.unwrap_or_else(|| ops::blend_ex(fg, bg, mask, mode, op, am, crop))
        }
        "normal" => {
            let (src, st, dx) = (tex("in")?, f("strength", 2.0), f("directx", 0.0) > 0.5);
            let n = crate::gpu::normal_from_height(src, st, dx).unwrap_or_else(|| ops::normal_from_height(src, st, dx));
            if f("alpha_from_input", 0.0) > 0.5 {
                // "Fill alpha with input": the height map goes into the alpha channel
                let src = tex("in")?;
                let mut n = n;
                for (o, i) in n.px.iter_mut().zip(&src.px) {
                    o[3] = crate::tex::lum(*i);
                }
                n
            } else {
                n
            }
        }
        "normal_blend" => ops::normal_blend_ex(tex("a")?, tex("b")?, f("opacity", 1.0), ins.get("mask").map(|m| m.as_ref())),
        "ao" if f("mode", 0.0) < 0.5 => ops::ambient_occlusion(tex("in")?, f("radius", 6.0) * rs, f("intensity", 2.0)),
        "ao" => {
            let scale = if f("use_world_units", 0.0) > 0.5 { f("height_depth_cm", 30.0) / f("surface_size", 300.0).max(1.0) } else { f("height_depth", 0.1) };
            let dirs = [4u32, 8, 16][(f("quality", 1.0).round() as usize).min(2)];
            ops::hbao(tex("in")?, f("hbao_radius", 1.0).clamp(0.0, 1.0) * 0.25, scale, dirs, f("hbao_intensity", 1.0))
        }
        "quantize_color" => {
            let (res, ids, pal) = ops::quantize_color_full(
                tex("in")?,
                per("color_amount", 32.0).clamp(1, 128) as usize,
                f("contour_smoothing", 0.0) * rs,
                f("dithering", 0.0).clamp(0.0, 1.0),
                f("dithering_pattern", 0.0) > 0.5,
                f("distance_color_space", 0.0) < 0.5,
                f("ignore_alpha", 0.0) > 0.5,
            );
            match f("_out", 0.0).round() as i32 {
                1 => {
                    // ID: every region's palette index as a gray level (index / (colours - 1))
                    let div = (pal.len().max(2) - 1) as f32;
                    let mut t = res.clone();
                    for (o, i) in t.px.iter_mut().zip(&ids) {
                        let v = *i as f32 / div;
                        *o = [v, v, v, 1.0];
                    }
                    t
                }
                2 => {
                    // Palette: one vertical band per colour, left to right in index order
                    let k = pal.len();
                    Tex::from_fn(s, hs, move |u, _| {
                        let c = pal[((u * k as f32) as usize).min(k - 1)];
                        [c[0], c[1], c[2], 1.0]
                    })
                }
                _ => res,
            }
        }
        "rgba_split" => {
            let c = (f("_out", 0.0).round() as usize).min(3);
            tex("in")?.map(|p| [p[c], p[c], p[c], 1.0])
        }
        "alpha_split" => {
            if f("_out", 0.0) >= 0.5 {
                tex("in")?.map(|p| [p[3], p[3], p[3], 1.0])
            } else {
                tex("in")?.map(|p| [p[0], p[1], p[2], 1.0])
            }
        }
        "curvature" => {
            if f("mode", 0.0) > 0.5 {
                ops::curvature(tex("in")?, f("radius", 3.0) * rs, f("intensity", 4.0))
            } else {
                ops::curvature_normal(tex("in")?, f("intensity", 4.0), f("directx", 0.0) > 0.5)
            }
        }
        "edge_detect" => {
            if f("mode", 0.0) > 0.5 {
                ops::edge_detect(tex("in")?, f("intensity", 2.0))
            } else {
                // widths are given in pixels at the 512 reference size and scale with the resolution
                ops::edge_detect_ex(tex("in")?, (f("edge_width", 2.0) * rs).max(1.0), f("edge_roundness", 4.0) * rs, f("invert", 0.0) > 0.5, f("tolerance", 0.0))
            }
        }
        "emboss" => {
            if f("mode", 0.0) > 0.5 {
                let (hl, sh) = ([f("hl_r", 1.0), f("hl_g", 1.0), f("hl_b", 1.0)], [f("sh_r", 0.0), f("sh_g", 0.0), f("sh_b", 0.0)]);
                ops::emboss_lit(tex("in")?, ins.get("height").map(|t| &**t), f("angle", 0.125), f("intensity", 4.0), hl, sh, f("gloss", 0.0).clamp(0.0, 1.0))
            } else {
                ops::emboss(tex("in")?, f("angle", 0.125), f("intensity", 4.0))
            }
        }
        other => match crate::nodes2::compute(other, id, n, ins, s, hs, seed)? {
            Some(t) => t,
            None => match crate::nodes3::compute(other, id, n, ins, s, hs, seed)? {
                Some(t) => t,
                None => match crate::nodes4::compute(other, id, n, ins, s, hs, seed)? {
                    Some(t) => t,
                    None => match crate::nodes5::compute(other, id, n, ins, s, hs, seed)? {
                        Some(t) => t,
                        None => match crate::nodes6::compute(other, id, n, ins, s, hs, seed, &|f| read_resource(id, f, ctx).map_err(|e| format!("{e}")))? {
                            Some(t) => t,
                            None => match crate::nodes7::compute(other, id, n, ins, s, hs, seed)? {
                                Some(t) => t,
                                None => return Err(Error::UnknownNode(id, other.to_string())),
                            },
                        },
                    },
                },
            },
        },
    };
    let out = if out.w != s || out.h != hs { Tex::from_fn(s, hs, |u, v| out.sample(u, v)) } else { out };
    let (fx, fy) = (tx - tx.floor(), ty - ty.floor());
    if fx > 0.0 || fy > 0.0 {
        return Ok(ops::roll(&out, fx, fy));
    }
    Ok(out)
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let d = b - a;
    let t = if d.abs() < 1e-9 { if x >= b { 1.0 } else { 0.0 } } else { ((x - a) / d).clamp(0.0, 1.0) };
    t * t * (3.0 - 2.0 * t)
}

pub const EXAMPLE: &str = r#"# Brick wall PBR material
size 2048
node 1 bricks rows=8 cols=4 mortar=0.04 bevel=0.10 lum_var=0.45 seed=3
node 2 fbm scale=8 octaves=5 seed=7
node 3 fbm scale=32 octaves=3 seed=9
node 4 blend fg=@2 bg=@1 mode=5 opacity=0.35
node 5 blend fg=@3 bg=@4 mode=1 opacity=0.06
node 6 normal in=@5 strength=3
node 7 ao in=@5 radius=6 intensity=2.5
node 8 colorize in=@5 p0=0 r0=0.10 g0=0.09 b0=0.08 p1=0.5 r1=0.55 g1=0.27 b1=0.19 p2=1 r2=0.78 g2=0.46 b2=0.34
node 9 blend fg=@7 bg=@8 mode=3 opacity=1
node 10 levels in=@3 low=0.2 high=0.9 out_low=0.55 out_high=0.95
node 11 color r=0 g=0 b=0
output basecolor 9
output height 5
output normal 6
output ambient_occlusion 7
output roughness 10
output metallic 11
expose "Brick count" 1 cols 1 12
expose "Mortar width" 1 mortar 0 0.15
expose "Normal strength" 6 strength 0 10
"#;

/// Parameters of a Tile Sampler / Tile Generator node (shared by the CPU and the device paths).
pub(crate) fn tile_params(n: &Node, gen: bool, ar: f32, seed: u32) -> scatter::TileP {
    let f = |k: &str, d: f32| n.nums.get(k).copied().unwrap_or(d);
    let mut p = scatter::TileP::legacy(gen);
    p.nx = f("x_amount", p.nx);
    p.ny = f("y_amount", p.ny);
    if ar != 1.0 {
        p.ny = (p.nx * ar).round().max(1.0);
    }
    p.size = f("size", p.size);
    p.aspect = f("aspect", 1.0);
    p.size_mode = f("size_mode", 0.0) as u32;
    p.scale = f("scale", 1.0);
    p.scale_rand = f("scale_random", 0.0);
    p.scale_seed = f("scale_seed", 0.0) as u32;
    p.size_rand = f("size_random", 0.0);
    p.rot = f("rotation", 0.0);
    p.rot_rand = f("rotation_random", 0.0);
    p.pattern_rot90 = f("pattern_rotation", 0.0) as u32;
    p.pattern_rot_rand = f("pattern_rotation_random", 0.0).clamp(0.0, 1.0);
    p.symmetry_random = f("symmetry_random", 0.0) as u32;
    p.pos_rand = f("position_random", 0.0);
    p.pos_rand_x = f("position_random_x", 1.0);
    p.pos_rand_y = f("position_random_y", 1.0);
    p.color = f("color", 1.0);
    p.color_rand = f("color_random", 0.0);
    p.lum_by_number = f("lum_by_number", 0.0);
    p.lum_by_scale = f("lum_by_scale", 0.0);
    p.offset = f("offset", 0.5);
    p.offset_mode = f("offset_mode", p.offset_mode as f32) as u32;
    p.offset_rand = f("offset_random", 0.0);
    p.offset_seed = f("offset_seed", 0.0) as u32;
    p.global_offset = [f("global_offset_x", 0.0), f("global_offset_y", 0.0)];
    p.seed = seed;
    p.thresh = f("mask_threshold", 0.5);
    p.mask_invert = f("mask_invert", 0.0) > 0.5;
    p.mask_random = f("mask_random", 0.0);
    p.mask_checker = f("mask_checker", 0.0) as u32;
    p.mask_h = f("mask_horizontal", 0.0) > 0.5;
    p.mask_v = f("mask_vertical", 0.0) > 0.5;
    p.invert_mask = f("invert_mask", 0.0) > 0.5;
    p.scale_map_mult = f("scale_map_multiplier", 1.0);
    p.rotation_map_mult = f("rotation_map_multiplier", 1.0);
    p.displacement_intensity = f("displacement_intensity", 0.5);
    p.displacement_angle = f("displacement_angle", 0.0);
    p.color_map_mult = f("color_map_multiplier", 1.0);
    p.pattern_specific = f("pattern_specific", 0.5);
    p.pattern_specific_rand = f("pattern_specific_random", 0.0);
    p.blend = scatter::Blend::from_index(f("blending", 0.0) as u32);
    p.opacity = f("opacity", 1.0);
    p.background = f("background", 0.0);
    p.reverse = f("reverse_order", 0.0) > 0.5;
    p
}

/// Parameters of a Splatter node.
pub(crate) fn splatter_params(n: &Node, seed: u32) -> scatter::SplatterP {
    let f = |k: &str, d: f32| n.nums.get(k).copied().unwrap_or(d);
    let mut p = scatter::SplatterP::legacy();
    p.count = f("count", 40.0).max(0.0) as u32;
    p.grid = f("grid_number", 0.0).max(0.0) as u32;
    p.scale = f("scale", 0.1);
    p.scale_var = f("scale_var", 0.3);
    p.aspect = f("aspect", 1.0);
    p.rot = f("rotation", 0.0);
    p.rot_var = f("rot_var", 1.0);
    p.pattern_rot90 = f("pattern_rotation", 0.0) as u32;
    p.lum_var = f("lum_var", 0.0);
    p.disorder = f("disorder", 1.0);
    p.pan = [f("pan_x", 0.0), f("pan_y", 0.0)];
    p.symmetry_random = f("symmetry_random", 0.0) as u32;
    p.pattern_specific = f("pattern_specific", 0.5);
    p.gain = f("gain", 1.0);
    p.out_min = f("output_min", 0.0);
    p.out_max = f("output_max", 1.0);
    p.background = f("background", 0.0);
    p.blend = scatter::Blend::from_index(f("blending", 0.0) as u32);
    p.opacity = f("opacity", 1.0);
    p.seed = seed;
    p
}

/// Parameters of a Make It Tile Patch node drawn by stamping.
pub(crate) fn tile_patch_params(f: &dyn Fn(&str, f32) -> f32, seed: u32) -> ops::TilePatch {
    ops::TilePatch {
        octave: f("octave", 3.0).round() as i32,
        mask_size: f("mask_size", 1.0),
        mask_precision: f("mask_precision", 0.5),
        mask_warp: f("mask_warp", 0.0),
        size: [f("size_x", 2.0), f("size_y", 2.0)],
        disorder: f("disorder", 0.0),
        size_var: f("size_var", 0.0),
        rotation: f("rotation", 0.0),
        rotation_var: f("rotation_var", 0.0),
        background: [f("bg_r", 0.0), f("bg_g", 0.0), f("bg_b", 0.0), f("bg_a", 0.0)],
        color_var: f("color_var", 0.0),
        lum_var: f("lum_var", 0.0),
        seed,
    }
}

/// Parameters of a Splatter Circular node.
pub(crate) fn circular_params(n: &Node, seed: u32) -> scatter::CircularP {
    let f = |k: &str, d: f32| n.nums.get(k).copied().unwrap_or(d);
    let mut p = scatter::CircularP::legacy();
    p.amount = f("number", 12.0).max(1.0) as u32;
    p.amount_rand = f("amount_random", 0.0);
    p.amount_rand_min = f("amount_random_min", 1.0).max(1.0) as u32;
    p.rings = f("rings", 1.0).max(1.0) as u32;
    p.radius = f("radius", 0.3);
    p.radius_rand = f("radius_random", 0.0);
    p.ring_mult = f("ring_radius_multiplier", 0.5);
    p.angle_rand = f("angle_random", 0.0);
    p.spiral = f("spiral_factor", 0.0);
    p.spread = f("spread", 1.0);
    p.size = f("size", 0.1);
    p.size_rand = f("size_random", 0.0);
    p.scale_by_number = f("scale_by_number", 0.0);
    p.invert_number = f("invert_number", 0.0) > 0.5;
    p.scale_by_ring = f("scale_by_ring", 0.0);
    p.invert_ring = f("invert_ring", 0.0) > 0.5;
    p.rot = f("rotation", 0.0);
    p.rot_rand = f("rotation_random", 0.0);
    p.center_orientation = f("center_orientation", 0.0);
    p.ring_rot = f("ring_rotation", 0.0);
    p.ring_rot_rand = f("ring_rotation_random", 0.0);
    p.ring_rot_offset = f("ring_rotation_offset", 0.0);
    p.pos_rand = f("position_random", 0.0);
    p.symmetry_random = f("symmetry_random", 0.0) as u32;
    p.pattern_specific = f("pattern_specific", 0.5);
    p.color = f("color", 1.0);
    p.lum_rand = f("luminance_random", 0.0);
    p.lum_by_number = f("lum_by_number", 0.0);
    p.lum_by_ring = f("lum_by_ring", 0.0);
    p.mask_random = f("mask_random", 0.0);
    p.background = f("background", 0.0);
    p.blend = scatter::Blend::from_index(f("blending", 0.0) as u32);
    p.opacity = f("opacity", 1.0);
    p.seed = seed;
    p
}

/// Collect the connected pattern inputs of a scatter node (`pattern`, `pattern2` .. `pattern6`).
fn scatter_patterns<'a>(ins: &'a BTreeMap<String, Rc<Tex>>, builtin: u32, fallback: u32, distribution: u32) -> scatter::Patterns<'a> {
    let mut inputs: Vec<&Tex> = Vec::new();
    for k in ["pattern", "pattern2", "pattern3", "pattern4", "pattern5", "pattern6"] {
        if let Some(t) = ins.get(k) {
            inputs.push(t.as_ref());
        }
    }
    scatter::Patterns { inputs, builtin, fallback, distribution }
}
