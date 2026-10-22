// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tenth set of nodes: MaterialCraft's own implementations of the standard library graphs that `.sbs` files use and
//! that have no dedicated node elsewhere (mask generators, material filters, spline and path tools, panorama and
//! HDR tools, 3D-texture tools, PBR utilities and material presets).
//!
//! Each of these nodes is named after the library graph it stands for (`x_<graph id>`). Its interface comes from the
//! library interface facts (`sbs_library.txt`): the image inputs keep the graph's input names, the outputs keep the
//! graph's output names and every parameter keeps the graph's parameter name (vector parameters are stored one lane
//! per number, `name_0`, `name_1` ...). So an imported node carries all its settings over unchanged and the editor,
//! the command interface and the renderer agree on what each setting is. How the original graphs are built is never
//! used; the algorithms here are MaterialCraft's own (see `nodes10_*.rs`).
use crate::graph::{Error, Node, Pin, Spec};
use crate::params::{PK, PD};
use crate::sbs_lib::{LGraph, LTy};
use crate::tex::{lum, Tex};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc as Rc;
use std::sync::OnceLock;

/// The library graphs implemented here: our node kind, library package, library graph id.
pub const LIBS: &[(&str, &str, &str)] = &[
    ("x_2dtransform_path", "paths_tools", "2dtransform_path"),
    ("x_3d_linear_gradient", "3d_gradient", "3d_linear_gradient"),
    ("x_3d_planar_projection", "3d_planar_projection", "3d_planar_projection"),
    ("x_3d_planar_projection_color", "3d_planar_projection", "3d_planar_projection_color"),
    ("x_3d_texture_offset_color", "3d_texture_transform", "3d_texture_offset_color"),
    ("x_3d_texture_offset_grayscale", "3d_texture_transform", "3d_texture_offset_grayscale"),
    ("x_3d_texture_position", "3d_texture_position", "3d_texture_position"),
    ("x_3d_texture_sdf", "3d_texture_jump_flood", "3d_texture_sdf"),
    ("x_3d_texture_surface_render", "3d_texture_render", "3d_texture_surface_render"),
    ("x_3d_texture_volume_render_light", "3d_texture_render", "3d_texture_volume_render_light"),
    ("x_3d_volume_mask", "3d_volume_mask", "3d_volume_mask"),
    ("x_acescg_to_linear_srgb", "color_space_conversion", "acescg_to_linear_srgb"),
    ("x_acescg_to_srgb", "color_space_conversion", "acescg_to_srgb"),
    ("x_anisotropic_kuwahara_color", "anisotropic_kuwahara", "anisotropic_kuwahara_color"),
    ("x_anisotropic_kuwahara_grayscale", "anisotropic_kuwahara", "anisotropic_kuwahara_grayscale"),
    ("x_ao_cancel", "ao_cancel", "ao_cancel"),
    ("x_apply_color_palette", "quantize_color", "apply_color_palette"),
    ("x_atlas_scatter", "atlas_scatter", "atlas_scatter"),
    ("x_atlas_spliter", "atlas_spliter", "atlas_spliter"),
    ("x_basecolor_metallic_roughness_converter", "pbr_converter", "basecolor_metallic_roughness_converter"),
    ("x_bitmap2material_2_light", "bitmaptomaterial_2_light", "Bitmap2Material_2_Light"),
    ("x_blackbody", "blackbody", "blackbody"),
    ("x_bricks_001", "bricks_001", "bricks_001"),
    ("x_bricks_005", "bricks_005", "bricks_005"),
    ("x_cardboard_001", "cardboard_001", "cardboard_001"),
    ("x_clone", "clone", "clone"),
    ("x_color_equalizer", "color_equalizer", "color_equalizer"),
    ("x_color_temperature_adjustment", "color_temperature_adjustment", "color_temperature_adjustment"),
    ("x_concrete_002", "concrete_002", "concrete_002"),
    ("x_concrete_006", "concrete_006", "concrete_006"),
    ("x_concrete_010", "concrete_010", "concrete_010"),
    ("x_concrete_011", "concrete_011", "concrete_011"),
    ("x_concrete_044", "concrete_044", "concrete_044"),
    ("x_concrete_070", "concrete_070", "concrete_070"),
    ("x_cracks_weathering", "cracks_weathering_material", "cracks_weathering"),
    ("x_create_color_palette_16colors", "quantize_color", "create_color_palette_16colors"),
    ("x_crop", "crop", "crop"),
    ("x_crop_grayscale", "crop", "crop_grayscale"),
    ("x_cube_3d", "pattern_cube_3d", "cube_3d"),
    ("x_cube_3d_gbuffers", "pattern_cube_3d", "cube_3d_gbuffers"),
    ("x_directional_distance", "directional_distance", "directional_distance"),
    ("x_exposure", "exposure", "exposure"),
    ("x_extend_shape_color", "extend_shape", "extend_shape_color"),
    ("x_extend_shape_grayscale", "extend_shape", "extend_shape_grayscale"),
    ("x_fabric_002", "fabric_002", "fabric_002"),
    ("x_fabric_009", "fabric_009", "fabric_009"),
    ("x_fabric_025", "fabric_025", "fabric_025"),
    ("x_fabric_weathering", "fabric_weathering_material", "fabric_weathering"),
    ("x_floodfill_mapper_color", "floodfill_mapper", "floodfill_mapper_color"),
    ("x_floodfill_mapper_grayscale", "floodfill_mapper", "floodfill_mapper_grayscale"),
    ("x_get_size_color", "value_get_size", "get_size_color"),
    ("x_get_size_grayscale", "value_get_size", "get_size_grayscale"),
    ("x_gravel", "gravel", "gravel"),
    ("x_hald_clut", "hald_clut", "hald_clut"),
    ("x_hdr_exposure_preview", "hdr_exposure_preview", "hdr_exposure_preview"),
    ("x_hdr_merge_2", "hdr_merge", "hdr_merge_2"),
    ("x_hdr_range_viewer", "hdr_range_viewer", "hdr_range_viewer"),
    ("x_hdr_range_viewer_grayscale", "hdr_range_viewer", "hdr_range_viewer_grayscale"),
    ("x_height_extrude", "height_extrude", "height_extrude"),
    ("x_height_map_frequencies_mapper", "height_map_frequencies_mapper", "height_map_frequencies_mapper"),
    ("x_histogram_compute", "histogram_tools", "histogram_compute"),
    ("x_histogram_render", "histogram_tools", "histogram_render"),
    ("x_id_to_mask_grayscale", "id_to_mask", "id_to_mask_grayscale"),
    ("x_leather_weathering", "leather_weathering_material", "leather_weathering"),
    ("x_lighting_cancel_hf", "lighting_cancel_hf", "lighting_cancel_hf"),
    ("x_lighting_cancel_lf", "lighting_cancel_lf", "lighting_cancel_lf"),
    ("x_linear_srgb_to_acescg", "color_space_conversion", "linear_srgb_to_acescg"),
    ("x_mask_to_paths_2", "paths_tools", "mask_to_paths_2"),
    ("x_material_adjustment_blend", "material_adjustment_blend", "material_adjustment_blend"),
    ("x_material_clone_patch", "clone_patch", "material_clone_patch"),
    ("x_material_color_blend", "material_color_blend", "material_color_blend"),
    ("x_material_crop", "crop", "material_crop"),
    ("x_material_height_blend", "material_height_blend", "material_height_blend"),
    ("x_material_mesh_data_blender", "material_mesh_data_blender", "material_mesh_data_blender"),
    ("x_material_selector", "material_selector", "material_selector"),
    ("x_material_switch", "material_switch", "material_switch"),
    ("x_material_transform", "material_transform", "material_transform"),
    ("x_median_filter_color", "median_filter", "median_filter_color"),
    ("x_median_filter_grayscale", "median_filter", "median_filter_grayscale"),
    ("x_mesh_data_combiner", "mesh_data_combiner", "mesh_data_combiner"),
    ("x_metal_002", "metal_002", "metal_002"),
    ("x_metal_003", "metal_003", "metal_003"),
    ("x_metal_006", "metal_006", "metal_006"),
    ("x_metal_plate_001", "metal_plate_001", "metal_plate_001"),
    ("x_metal_plate_004", "metal_plate_004", "metal_plate_004"),
    ("x_mg_bottom_to_top", "mg_bottom_to_top", "mg_bottom_to_top"),
    ("x_mg_cloth_wear", "mg_cloth_wear", "mg_cloth_wear"),
    ("x_mg_dirt", "mg_dirt", "mg_dirt"),
    ("x_mg_dripping_rust", "mg_dripping_rust", "mg_dripping_rust"),
    ("x_mg_dust", "mg_dust", "mg_dust"),
    ("x_mg_edge_blur", "mg_edge_blur", "mg_edge_blur"),
    ("x_mg_edge_damages", "mg_edge_damages", "mg_edge_damages"),
    ("x_mg_edge_dirt", "mg_edge_dirt", "mg_edge_dirt"),
    ("x_mg_edge_notch", "mg_edge_notch", "mg_edge_notch"),
    ("x_mg_edge_select", "mg_edge_select", "mg_edge_select"),
    ("x_mg_edge_speckle", "mg_edge_speckle", "mg_edge_speckle"),
    ("x_mg_edge_wear", "mg_edge_wear", "mg_edge_wear"),
    ("x_mg_fiber_glass_edge_wear", "mg_fiber_glass_edge_wear", "mg_fiber_glass_edge_wear"),
    ("x_mg_grease", "mg_grease", "mg_grease"),
    ("x_mg_ground_dirt", "mg_ground_dirt", "mg_ground_dirt"),
    ("x_mg_leaks", "mg_leaks", "mg_leaks"),
    ("x_mg_leather_wear", "mg_leather_wear", "mg_leather_wear"),
    ("x_mg_light", "mg_light", "mg_light"),
    ("x_mg_mask_builder_2", "mg_mask_builder", "mg_mask_builder_2"),
    ("x_mg_metal_edge_wear", "mg_metal_edge_wear", "mg_metal_edge_wear"),
    ("x_mg_paint_wear", "mg_paint_wear", "mg_paint_wear"),
    ("x_mg_selective_dirt", "mg_selective_dirt", "mg_selective_dirt"),
    ("x_mg_sun_bleach", "mg_sun_bleach", "mg_sun_bleach"),
    ("x_mg_surface_brush", "mg_surface_brush", "mg_surface_brush"),
    ("x_mlv_color", "mean_of_least_variance", "mlv_color"),
    ("x_mlv_grayscale", "mean_of_least_variance", "mlv_grayscale"),
    ("x_modify_color_palette", "quantize_color", "modify_color_palette"),
    ("x_moss_weathering", "moss_weathering_material", "moss_weathering"),
    ("x_multi_clone_patch", "clone_patch", "multi_clone_patch"),
    ("x_multi_clone_patch_grayscale", "clone_patch", "multi_clone_patch_grayscale"),
    ("x_multi_color_equalizer", "color_equalizer", "multi_color_equalizer"),
    ("x_multi_crop", "crop", "multi_crop"),
    ("x_multi_crop_grayscale", "crop", "multi_crop_grayscale"),
    ("x_multi_material_blend", "multi_material_blend", "multi_material_blend"),
    ("x_multiangle_to_albedo", "multiangle_to_albedo", "multiangle_to_albedo"),
    ("x_multiangle_to_normal", "multiangle_to_normal", "multiangle_to_normal"),
    ("x_nadir_extract", "panorama_nadir_extract", "nadir_extract"),
    ("x_non_square_transform", "non_square_transform", "non_square_transform"),
    ("x_non_square_transform_greyscale", "non_square_transform", "non_square_transform_greyscale"),
    ("x_non_uniform_rotation_color", "non_uniform_rotation", "non_uniform_rotation_color"),
    ("x_non_uniform_rotation_grayscale", "non_uniform_rotation", "non_uniform_rotation_grayscale"),
    ("x_normal_intensity", "normal_intensity", "normal_intensity"),
    ("x_normal_uncombine", "normal_uncombine", "normal_uncombine"),
    ("x_old_painted_planks", "old_painted_planks", "old_painted_planks"),
    ("x_painted_metal_weathering", "painted_metal_weathering_material", "painted_metal_weathering"),
    ("x_panorama_3d_position", "panorama_position", "panorama_3d_position"),
    ("x_panorama_gradient_2_points", "panorama_gradients", "panorama_gradient_2_points"),
    ("x_panorama_gradient_linear", "panorama_gradients", "panorama_gradient_linear"),
    ("x_panorama_line_light", "panorama_shape", "panorama_line_light"),
    ("x_panorama_nadir_patch", "panorama_nadir_patch", "panorama_nadir_patch"),
    ("x_panorama_physical_sun_sky", "panorama_physical_sun_sky", "panorama_physical_sun_sky"),
    ("x_panorama_plane_light", "panorama_shape", "panorama_plane_light"),
    ("x_panorama_rotation", "panorama_rotation", "panorama_rotation"),
    ("x_panorama_shape", "panorama_shape", "panorama_shape"),
    ("x_panorama_shape_2", "panorama_shape", "panorama_shape_2"),
    ("x_panorama_sphere_light", "panorama_shape", "panorama_sphere_light"),
    ("x_panorama_straighten_horizon", "panorama_straighten_horizon", "panorama_straighten_horizon"),
    ("x_paper_005", "paper_005", "paper_005"),
    ("x_path_to_spline", "paths_tools", "path_to_spline"),
    ("x_paths_polygon", "paths_tools", "paths_polygon"),
    ("x_paths_trace", "paths_tools", "paths_trace"),
    ("x_paths_warp", "paths_tools", "paths_warp"),
    ("x_pbr_albedo_safe_color", "pbr_albedo_safe_color", "pbr_albedo_safe_color"),
    ("x_pbr_base_material", "pbr_base_material", "pbr_base_material"),
    ("x_pbr_basecolor_metallic_validate_2", "pbr_basecolor_metallic_validate", "pbr_basecolor_metallic_validate_2"),
    ("x_pbr_dielectric_f0", "pbr_dielectric_F0", "pbr_dielectric_F0"),
    ("x_pbr_metal_reflectance", "pbr_metal_reflectance", "pbr_metal_reflectance"),
    ("x_pbr_render", "pbr_render2", "PBR_render"),
    ("x_pbr_render_mapping_color", "pbr_render2", "PBR_render_mapping_color"),
    ("x_pbr_render_mapping_grayscale", "pbr_render2", "PBR_render_mapping_grayscale"),
    ("x_point_list", "spline_tools", "point_list"),
    ("x_quad_transform_path", "paths_tools", "quad_transform_path"),
    ("x_rock_weathering", "rock_weathering_material", "rock_weathering"),
    ("x_rough_concrete_with_lines", "rough_concrete_with_lines", "rough_concrete_with_lines"),
    ("x_rt_irradiance", "rt_irradiance", "rt_irradiance"),
    ("x_rt_shadows", "rt_shadows_v2", "rt_shadows"),
    ("x_rust_weathering", "rust_weathering_material", "rust_weathering"),
    ("x_scatter_on_spline_color", "spline_tools", "scatter_on_spline_color"),
    ("x_scatter_on_spline_grayscale", "spline_tools", "scatter_on_spline_grayscale"),
    ("x_scatter_splines_on_splines", "spline_tools", "scatter_splines_on_splines"),
    ("x_scratches_generator_normal", "pattern_scratches_generator", "scratches_generator_normal"),
    ("x_select_path", "paths_tools", "select_path"),
    ("x_shadows", "shadows", "shadows"),
    ("x_shape_splatter", "shape_splatter", "shape_splatter"),
    ("x_shape_splatter_blend", "shape_splatter", "shape_splatter_blend"),
    ("x_shape_splatter_blend_color", "shape_splatter", "shape_splatter_blend_color"),
    ("x_shape_splatter_data_extract", "shape_splatter", "shape_splatter_data_extract"),
    ("x_shape_splatter_to_mask", "shape_splatter", "shape_splatter_to_mask"),
    ("x_smart_auto_tile", "smart_auto_tile", "smart_auto_tile"),
    ("x_snow_cover", "me_snow_cover", "snow_cover"),
    ("x_spline_2dtransform", "spline_tools", "spline_2dtransform"),
    ("x_spline_append", "spline_tools", "spline_append"),
    ("x_spline_bridge_2_splines", "spline_tools", "spline_bridge_2_splines"),
    ("x_spline_bridge_list_v2", "spline_tools", "spline_bridge_list_v2"),
    ("x_spline_bridge_mapper_color", "spline_tools", "spline_bridge_mapper_color"),
    ("x_spline_bridge_mapper_grayscale", "spline_tools", "spline_bridge_mapper_grayscale"),
    ("x_spline_circle", "spline_tools", "spline_circle"),
    ("x_spline_cubic", "spline_tools", "spline_cubic"),
    ("x_spline_fill", "spline_tools", "spline_fill"),
    ("x_spline_flow_mapper", "spline_tools", "spline_flow_mapper"),
    ("x_spline_mapper_color", "spline_tools", "spline_mapper_color"),
    ("x_spline_mapper_grayscale", "spline_tools", "spline_mapper_grayscale"),
    ("x_spline_merge_list", "spline_tools", "spline_merge_list"),
    ("x_spline_poly_quadratic", "spline_tools", "spline_poly_quadratic"),
    ("x_spline_quadratic", "spline_tools", "spline_quadratic"),
    ("x_spline_render", "spline_tools", "spline_render"),
    ("x_spline_sample_height", "spline_tools", "spline_sample_height"),
    ("x_spline_sample_thickness", "spline_tools", "spline_sample_thickness"),
    ("x_spline_select", "spline_tools", "spline_select"),
    ("x_spline_warp", "spline_tools", "spline_warp"),
    ("x_srgb_to_acescg", "color_space_conversion", "srgb_to_acescg"),
    ("x_tiles_002", "tiles_002", "tiles_002"),
    ("x_tiles_008", "tiles_008", "tiles_008"),
    ("x_tiles_011", "tiles_011", "tiles_011"),
    ("x_tiles_025", "tiles_025", "tiles_025"),
    ("x_triplanar_3", "triplanar", "triplanar_3"),
    ("x_triplanar_grayscale_3", "triplanar", "triplanar_grayscale_3"),
    ("x_uv_mapper_color", "uv_mapper", "uv_mapper_color"),
    ("x_uv_mapper_grayscale", "uv_mapper", "uv_mapper_grayscale"),
    ("x_vertex_processor", "paths_tools", "vertex_processor"),
    ("x_vertex_processor_simple", "paths_tools", "vertex_processor_simple"),
    ("x_water_level", "me_water_level", "water_level"),
    ("x_wood_american_cherry", "wood_american_cherry", "wood_american_cherry"),
];

/// Everything the rest of MaterialCraft needs to know about one of these nodes, built once from the library facts.
pub struct Info {
    pub kind: &'static str,
    pub lib: &'static LGraph,
    pub spec: Spec,
    /// named outputs (empty for a single output)
    pub outs: &'static [&'static str],
    pub label: &'static str,
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn table() -> &'static HashMap<&'static str, Info> {
    static T: OnceLock<HashMap<&'static str, Info>> = OnceLock::new();
    T.get_or_init(|| {
        let mut m = HashMap::new();
        for &(kind, pkg, id) in LIBS {
            let Some(lib) = crate::sbs_lib::find(Some(pkg), id) else { continue };
            let imgs: Vec<&'static str> = lib.images().map(|i| i.id).collect();
            let opt: &'static [&'static str] = Box::leak(imgs.into_boxed_slice());
            let outs: &'static [&'static str] = if lib.outputs.len() > 1 { Box::leak(lib.outputs.clone().into_boxed_slice()) } else { &[] };
            let label = leak(title(id));
            let doc = leak(format!("MaterialCraft's own implementation of the library graph {pkg}/{id}; parameters keep the graph's names"));
            m.insert(kind, Info { kind, lib, spec: Spec { kind, req: &[], opt, doc }, outs, label });
        }
        m
    })
}

fn title(id: &str) -> String {
    let words: Vec<String> = id
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| match w.to_ascii_lowercase().as_str() {
            "mg" => "MG".to_string(),
            "pbr" => "PBR".to_string(),
            "hdr" => "HDR".to_string(),
            "id" => "ID".to_string(),
            "uv" => "UV".to_string(),
            "ao" => "AO".to_string(),
            "lf" => "LF".to_string(),
            "hf" => "HF".to_string(),
            "mlv" => "MLV".to_string(),
            "2d" | "2dtransform" => w.to_ascii_uppercase().replace("TRANSFORM", " Transform"),
            "3d" => "3D".to_string(),
            _ => {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            }
        })
        .collect();
    words.join(" ")
}

pub fn info(kind: &str) -> Option<&'static Info> {
    if !kind.starts_with("x_") {
        return None;
    }
    table().get(kind)
}

/// Our node kind for library graph `id` of `package`.
pub fn kind_for(package: &str, id: &str) -> Option<&'static str> {
    LIBS.iter().find(|(_, p, g)| p.eq_ignore_ascii_case(package) && g.eq_ignore_ascii_case(id)).or_else(|| LIBS.iter().find(|(_, _, g)| g.eq_ignore_ascii_case(id))).map(|(k, _, _)| *k).filter(|k| info(k).is_some())
}

pub fn spec(kind: &str) -> Option<&'static Spec> {
    info(kind).map(|i| &i.spec)
}

pub fn outputs(kind: &str) -> &'static [&'static str] {
    info(kind).map(|i| i.outs).unwrap_or(&[])
}

pub fn label(kind: &str) -> Option<String> {
    info(kind).map(|i| i.label.to_string())
}

pub fn category(kind: &str) -> &'static str {
    let k = kind.strip_prefix("x_").unwrap_or(kind);
    if k.starts_with("mg_") || k.starts_with("material_") || k.ends_with("_weathering") || k.starts_with("pbr_") || k.starts_with("snow") || k.starts_with("water") || k == "mesh_data_combiner" || k == "multi_material_blend" || preset_material(kind) {
        "Material"
    } else if k.starts_with("spline") || k.contains("path") || k.starts_with("scatter_") || k == "point_list" || k.starts_with("shape_splatter") || k.starts_with("panorama") || k == "blackbody" || k.starts_with("cube_3d") || k == "3d_texture_position" || k == "scratches_generator_normal" {
        "Generators"
    } else {
        "Filters"
    }
}

/// The library's material presets (whole materials with no inputs).
pub fn preset_material(kind: &str) -> bool {
    matches!(
        kind,
        "x_bricks_001" | "x_bricks_005" | "x_cardboard_001" | "x_concrete_002" | "x_concrete_006" | "x_concrete_010" | "x_concrete_011" | "x_concrete_044" | "x_concrete_070" | "x_fabric_002" | "x_fabric_009"
            | "x_fabric_025" | "x_gravel" | "x_metal_002" | "x_metal_003" | "x_metal_006" | "x_metal_plate_001" | "x_metal_plate_004" | "x_old_painted_planks" | "x_paper_005" | "x_rough_concrete_with_lines"
            | "x_tiles_002" | "x_tiles_008" | "x_tiles_011" | "x_tiles_025" | "x_wood_american_cherry"
    )
}

/// Nodes whose outputs carry data (spline points, splatter records, positions) rather than pictures: they are
/// computed and kept in 32-bit float so the data survives.
pub fn data_node(kind: &str) -> bool {
    let k = kind.strip_prefix("x_").unwrap_or(kind);
    k.starts_with("spline_") && !matches!(k, "spline_render" | "spline_fill" | "spline_flow_mapper" | "spline_mapper_color" | "spline_mapper_grayscale" | "spline_bridge_mapper_color" | "spline_bridge_mapper_grayscale")
        || matches!(k, "point_list" | "scatter_splines_on_splines" | "path_to_spline" | "2dtransform_path" | "mask_to_paths_2" | "paths_polygon" | "paths_warp" | "quad_transform_path" | "select_path" | "vertex_processor" | "vertex_processor_simple" | "shape_splatter" | "3d_texture_position" | "3d_texture_sdf" | "panorama_3d_position" | "cube_3d_gbuffers" | "get_size_color" | "get_size_grayscale" | "histogram_compute")
}

/// Type of output `out` (None: not one of these nodes).
pub fn pin(kind: &str, out: Option<&str>) -> Option<Pin> {
    let i = info(kind)?;
    let name = out.map(str::to_string).unwrap_or_else(|| i.lib.outputs.first().map(|s| s.to_string()).unwrap_or_default()).to_ascii_lowercase();
    let k = kind.to_ascii_lowercase();
    if k.ends_with("_grayscale") || k.ends_with("_greyscale") || k.starts_with("x_mg_") && k != "x_mg_light" || k == "x_material_selector" || k == "x_id_to_mask_grayscale" {
        if !matches!(name.as_str(), "uv" | "direction_map" | "spline_coords" | "spline_data" | "position" | "uv_shape_space" | "uv_texture_space" | "bounding_box_size") {
            return Some(Pin::Gray);
        }
    }
    let colour = [
        "diffuse", "basecolor", "normal", "specular", "emissive", "color", "position", "uv", "spline_coords", "spline_data", "point_list", "paths", "tracing", "direction_map", "splatter_data", "palette", "albedo", "raw_irradiance", "raw_specular", "normalws", "output", "clone", "areas", "area", "speuclar_face_color", "reflection", "input", "preview",
        "uv_shape_space", "uv_texture_space", "bounding_box_size", "lighting_cancel", "vertex_color", "direction",
    ];
    let gray_names = ["roughness", "metallic", "height", "glossiness", "ambient", "opacity", "mask", "depth", "thickness", "curvature", "shape_count", "pattern", "user1", "user2", "histogram", "cdf", "ior", "f0", "spline_amount", "point_number", "shape_index", "pattern_id", "shape_angle", "guessed_intensity", "specularlevel", "specular_level", "anisotropy", "displacement", "relief", "id", "palette_color_amount", "position_grayscale", "height_mask"];
    if gray_names.iter().any(|g| name.starts_with(g)) && !name.starts_with("height_mask_color") {
        if name == "id" && k.starts_with("x_spline") {
            return Some(Pin::Gray);
        }
        return Some(Pin::Gray);
    }
    if name == "preview" && (k.starts_with("x_spline") || k.starts_with("x_point_list") || k.starts_with("x_scatter_splines") || k.starts_with("x_mask_to_paths")) {
        return Some(Pin::Gray);
    }
    if colour.iter().any(|c| name.starts_with(c)) || name.starts_with("output") {
        // single outputs of grayscale tools stay grayscale
        if matches!(k.as_str(), "x_shadows" | "x_rt_shadows" | "x_3d_linear_gradient" | "x_3d_volume_mask" | "x_3d_texture_sdf" | "x_histogram_render" | "x_directional_distance" | "x_spline_render" | "x_spline_fill" | "x_spline_flow_mapper" | "x_get_size_grayscale" | "x_3d_texture_offset_grayscale") && name != "uv" {
            return Some(Pin::Gray);
        }
        return Some(Pin::Color);
    }
    Some(Pin::Color)
}

/// Parameters of one of these nodes, straight from the library facts.
pub fn pdefs(kind: &str) -> Vec<PD> {
    static CACHE: OnceLock<std::sync::Mutex<HashMap<String, Vec<PD>>>> = OnceLock::new();
    let Some(i) = info(kind) else { return Vec::new() };
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut g = cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(v) = g.get(kind) {
        return v.clone();
    }
    let mut v: Vec<PD> = Vec::new();
    for li in i.lib.values() {
        let lanes = li.ty.lanes().max(1);
        let (lo, hi) = li.range.unwrap_or(match li.ty {
            LTy::Int(_) => (0.0, 16.0),
            _ => (0.0, 1.0),
        });
        for lane in 0..lanes {
            let name = if lanes == 1 { li.id } else { leak(format!("{}_{lane}", li.id)) };
            let label = if lanes == 1 { leak(title(li.id)) } else { leak(format!("{} {}", title(li.id), ["X", "Y", "Z", "W"][lane.min(3)])) };
            let def = li.lane(lane);
            let k = match li.ty {
                LTy::Bool => PK::B { def: def > 0.5 },
                LTy::Int(_) => {
                    let lo = lo.min(def).floor() as i32;
                    let hi = hi.max(def).ceil().max(lo as f32 + 1.0) as i32;
                    PK::I { def: def.round() as i32, lo, hi }
                }
                _ => PK::F { def, lo: lo.min(def), hi: hi.max(def) },
            };
            v.push(PD { name, label, k });
        }
    }
    v.push(crate::params::pi("seed", "Random seed", 0, 0, 10000));
    g.insert(kind.to_string(), v.clone());
    v
}

/// Adapter: every parameter of the library graph is carried over under its own name (vector lanes as `name_N`).
pub(crate) fn adapt(ad: &mut crate::sbs_adapt::Ad) {
    let Some(lib) = ad.lib else { return };
    let ids: Vec<(&'static str, usize)> = lib.values().map(|li| (li.id, li.ty.lanes().max(1))).collect();
    for (id, lanes) in ids {
        if lanes == 1 {
            ad.lin(id, 0, id, 1.0, 0.0);
        } else {
            for lane in 0..lanes {
                let k = format!("{id}_{lane}");
                ad.lin(id, lane, &k, 1.0, 0.0);
            }
        }
    }
    if data_node(ad.kind) {
        ad.set("ofmt", 4.0);
    }
    if let Some(note) = note(ad.kind) {
        ad.note(note);
    }
}

/// What a user should know about how close an implementation is (shown with the import report).
fn note(kind: &str) -> Option<&'static str> {
    Some(match kind {
        k if preset_material(k) => "the material is MaterialCraft's own procedural version of this preset, not the library's",
        k if k.starts_with("x_spline") || k == "x_point_list" || k == "x_scatter_splines_on_splines" || k == "x_scatter_on_spline_color" || k == "x_scatter_on_spline_grayscale" || k == "x_path_to_spline" => {
            "spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images"
        }
        k if k.contains("path") || k.starts_with("x_vertex_processor") => "path data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' path images",
        k if k.starts_with("x_shape_splatter") => "splatter data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' splatter images",
        k if k.starts_with("x_3d_texture") => "3D textures are slice atlases in MaterialCraft's own layout (grid of slices, first slice top left)",
        k if k.ends_with("_weathering") => "the weathering is MaterialCraft's own (masks from curvature, occlusion and noise)",
        "x_pbr_render" => "the render is MaterialCraft's own image-based shading of the material on the chosen shape",
        "x_rt_irradiance" => "irradiance is estimated from the height, emissive and an analytic sky",
        "x_get_size_color" | "x_get_size_grayscale" => "the size is written as pixel counts in the red and green channels",
        _ => return None,
    })
}

// ------------------------------------------------------------------------------------------------ evaluation

/// What a node implementation sees: its settings (by library name, library default when not set), its inputs and
/// the output size.
pub struct Cx<'a> {
    pub kind: &'a str,
    pub n: &'a Node,
    pub lib: &'static LGraph,
    pub ins: &'a BTreeMap<String, Rc<Tex>>,
    pub s: usize,
    pub hs: usize,
    pub seed: u32,
    /// which output is asked for (index into the library's outputs)
    pub out: usize,
    pub want: u32,
    pub outs: &'static [&'static str],
}

impl<'a> Cx<'a> {
    /// Lane `lane` of parameter `id`.
    pub fn lane(&self, id: &str, lane: usize) -> f32 {
        let li = self.lib.input(id);
        let lanes = li.map(|l| l.ty.lanes().max(1)).unwrap_or(1);
        let def = li.map(|l| l.lane(lane)).unwrap_or(0.0);
        let key = if lanes == 1 && lane == 0 { id.to_string() } else { format!("{}_{lane}", li.map(|l| l.id).unwrap_or(id)) };
        let exact = li.map(|l| l.id).unwrap_or(id);
        let v = if lanes == 1 { self.n.nums.get(exact).or_else(|| self.n.nums.get(id)) } else { self.n.nums.get(&key) };
        v.copied().filter(|v| v.is_finite()).unwrap_or(def)
    }
    pub fn f(&self, id: &str) -> f32 {
        self.lane(id, 0)
    }
    pub fn i(&self, id: &str) -> i32 {
        self.f(id).round() as i32
    }
    pub fn b(&self, id: &str) -> bool {
        self.f(id) > 0.5
    }
    pub fn v2(&self, id: &str) -> [f32; 2] {
        [self.lane(id, 0), self.lane(id, 1)]
    }
    pub fn v3(&self, id: &str) -> [f32; 3] {
        [self.lane(id, 0), self.lane(id, 1), self.lane(id, 2)]
    }
    pub fn v4(&self, id: &str) -> [f32; 4] {
        [self.lane(id, 0), self.lane(id, 1), self.lane(id, 2), self.lane(id, 3)]
    }
    /// A colour parameter (3 or 4 lanes; a 1-lane value fills the colour).
    pub fn col(&self, id: &str) -> [f32; 4] {
        let lanes = self.lib.input(id).map(|l| l.ty.lanes()).unwrap_or(1);
        match lanes {
            4 => self.v4(id),
            3 => {
                let c = self.v3(id);
                [c[0], c[1], c[2], 1.0]
            }
            _ => {
                let v = self.f(id);
                [v, v, v, 1.0]
            }
        }
    }
    /// Image input `id` when connected.
    pub fn img(&self, id: &str) -> Option<&'a Tex> {
        self.ins.get(id).or_else(|| self.ins.iter().find(|(k, _)| k.eq_ignore_ascii_case(id)).map(|(_, v)| v)).map(|r| r.as_ref())
    }
    /// Image input `id`, or a uniform picture of colour `c`.
    pub fn img_or(&self, id: &str, c: [f32; 4]) -> Tex {
        match self.img(id) {
            Some(t) => t.clone(),
            None => Tex::solid(self.s, self.hs, c),
        }
    }
    /// Pixel size relative to a 512 px image.
    pub fn rs(&self) -> f32 {
        self.s as f32 / 512.0
    }
    /// Index of the output named `name` (case does not matter).
    pub fn out_index(&self, name: &str) -> usize {
        self.outs.iter().position(|o| o.eq_ignore_ascii_case(name)).unwrap_or(0)
    }
    /// Is output `i` wanted by this pass (the asked-for one always is)?
    pub fn wants(&self, i: usize) -> bool {
        i == self.out || self.want & (1 << i.min(31)) != 0
    }
    /// Hands out the asked-for output of a node that computed several, keeping the others this pass reads.
    pub fn pick(&self, mut all: Vec<Option<Tex>>) -> Tex {
        let idx = self.out.min(all.len().saturating_sub(1));
        for (i, t) in all.iter_mut().enumerate() {
            if i != idx && self.want & (1 << i.min(31)) != 0 {
                if let Some(t) = t.take() {
                    crate::graph::emit_sibling(i, t);
                }
            }
        }
        all.get_mut(idx).and_then(|t| t.take()).unwrap_or_else(|| Tex::solid(self.s, self.hs, [0.0, 0.0, 0.0, 1.0]))
    }
}

pub fn compute(kind: &str, _id: u32, n: &Node, ins: &BTreeMap<String, Rc<Tex>>, s: usize, hs: usize, seed: u32) -> Result<Option<Tex>, Error> {
    let Some(info) = info(kind) else { return Ok(None) };
    let out = n.nums.get("_out").copied().unwrap_or(0.0).max(0.0).round() as usize;
    let want = n.nums.get("_want").copied().unwrap_or(0.0).max(0.0) as u32 | (1u32 << out.min(31));
    let cx = Cx { kind, n, lib: info.lib, ins, s: s.max(1), hs: hs.max(1), seed, out, want, outs: info.outs };
    let t = crate::nodes10_img::compute(&cx)
        .or_else(|| crate::nodes10_mat::compute(&cx))
        .or_else(|| crate::nodes10_spl::compute(&cx))
        .or_else(|| crate::nodes10_pano::compute(&cx));
    match t {
        Some(t) => Ok(Some(t)),
        None => Err(Error::BadParam(_id, format!("{kind}: not implemented"))),
    }
}

// ------------------------------------------------------------------------------------------------ shared helpers

pub fn grey(v: f32) -> [f32; 4] {
    [v, v, v, 1.0]
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn lerp4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t), lerp(a[3], b[3], t)]
}

pub fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let d = b - a;
    let t = if d.abs() < 1e-9 {
        if x >= b {
            1.0
        } else {
            0.0
        }
    } else {
        ((x - a) / d).clamp(0.0, 1.0)
    };
    t * t * (3.0 - 2.0 * t)
}

/// A value turned into a mask around `level` (0..1) with a `contrast` (0 soft .. 1 hard).
pub fn level_contrast(v: f32, level: f32, contrast: f32) -> f32 {
    let w = (1.0 - contrast.clamp(0.0, 1.0)) * 0.5 + 0.002;
    let t = 1.0 - level.clamp(0.0, 1.0);
    smooth(t - w, t + w, v)
}

/// Gray value of an image at (u, v).
pub fn g(t: &Tex, u: f32, v: f32) -> f32 {
    lum(t.sample(u, v))
}

/// Normal map pixel to a unit vector (OpenGL: green up; `directx` flips green).
pub fn dec_n(p: [f32; 4], directx: bool) -> [f32; 3] {
    let mut v = [p[0] * 2.0 - 1.0, p[1] * 2.0 - 1.0, p[2] * 2.0 - 1.0];
    if directx {
        v[1] = -v[1];
    }
    norm3(v)
}

pub fn enc_n(v: [f32; 3], directx: bool) -> [f32; 4] {
    let mut v = norm3(v);
    if directx {
        v[1] = -v[1];
    }
    [v[0] * 0.5 + 0.5, v[1] * 0.5 + 0.5, v[2] * 0.5 + 0.5, 1.0]
}

pub fn norm3(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l < 1e-9 {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / l, v[1] / l, v[2] / l]
    }
}

pub fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn mul3(a: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

/// A random number 0..1 for integer coordinates.
pub fn rnd(a: i32, b: i32, seed: u32) -> f32 {
    crate::noise::rnd(a, b, seed)
}

/// sRGB-encoded value to linear.
pub fn to_lin(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn to_srgb(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Colour of a black body at `kelvin`, linear RGB normalised so the brightest channel is 1.
pub fn blackbody(kelvin: f32) -> [f32; 3] {
    // a fit of the Planckian locus in sRGB (Tanner Helland's curves), then linearised
    let t = (kelvin.clamp(1000.0, 40000.0)) / 100.0;
    let r = if t <= 66.0 { 255.0 } else { 329.698_73 * (t - 60.0).powf(-0.133_204_76) };
    let gch = if t <= 66.0 { 99.470_8 * t.ln() - 161.119_57 } else { 288.122_17 * (t - 60.0).powf(-0.075_514_85) };
    let b = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_73 * (t - 10.0).ln() - 305.044_8
    };
    let c = [to_lin((r / 255.0).clamp(0.0, 1.0)), to_lin((gch / 255.0).clamp(0.0, 1.0)), to_lin((b / 255.0).clamp(0.0, 1.0))];
    let m = c[0].max(c[1]).max(c[2]).max(1e-6);
    [c[0] / m, c[1] / m, c[2] / m]
}

/// Height map to a normal map (`intensity` like the Normal node's, `directx` for the green-down format).
pub fn normal_of(h: &Tex, intensity: f32, directx: bool) -> Tex {
    crate::ops::normal_from_height(h, intensity, directx)
}

/// Convexity (>0.5) and concavity (<0.5) of a height map, centred on 0.5.
pub fn curvature_of(h: &Tex, radius_px: f32) -> Tex {
    crate::ops::curvature(h, radius_px.max(1.0), 1.0)
}

/// An image is "empty" when it is a uniform black fill (an unconnected input of an imported graph reads black).
pub fn is_blank(t: &Tex) -> bool {
    t.px.iter().step_by((t.px.len() / 97).max(1)).all(|p| p[0].abs() < 1e-6 && p[1].abs() < 1e-6 && p[2].abs() < 1e-6)
}

/// Grayscale copy of an image.
pub fn gray_of(t: &Tex) -> Tex {
    t.map(|p| grey(lum(p)))
}

/// Tileable value noise 0..1 with `cells` cells across (several octaves when `octaves` > 1).
pub fn noise(u: f32, v: f32, cells: i32, octaves: u32, seed: u32) -> f32 {
    if octaves <= 1 {
        crate::noise::perlin(u, v, cells.max(1), cells.max(1), seed)
    } else {
        crate::noise::fbm(u, v, cells.max(1), cells.max(1), octaves, 0.5, seed, 0)
    }
}
