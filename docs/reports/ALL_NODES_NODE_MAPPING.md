# all_nodes.sbs complete node mapping

This is the detailed inventory for the [compatibility report](ALL_NODES_COMPATIBILITY.md). All UIDs below belong to `Substance_graph`. Every node in the package, including the 237 nodes of the output-size function `Substance_graph/1584843868/outputsize`, has a native editor definition.

This inventory predates the replacement of 34 HDRI bitmap nodes with four bundled panoramas. See the fixture-update note in the compatibility report; the current fixture has 536 compositing nodes.

The tables contain all 566 compositing nodes once: 207 unconverted, 300 approximate, and 59 classified exact by the importer. `Texture` means finite pixel data was returned at 32×32; it does not prove visual fidelity. `Placeholder` means a gray fallback, even though evaluation returned a texture.

“Definition” describes native editor interface coverage, not renderer support. Behavior warnings and render errors are kept beside each node. Rows without a specific warning say “Native implementation of the library graph” (an `x_` kind) or “Own renderer implementation” (a library graph run by one of MaterialCraft's general nodes).

The 34 environment-map bitmap rows in the exact table rendered from stand-in files with the fixture's names in `assets/panorama/`; with the real maps there they resolve the same way, and without them they report the missing file. The one render error is the fixture's temporary bitmap, a fixture limitation.

## Formerly unconverted library nodes (207)

Each of these rendered a gray placeholder in the previous run. Each now maps to a native implementation of its library graph (renderer kinds starting with `x_`); their full rows are in the approximate table below, marked “was a placeholder”.

| Node UID | Native kind | Renderer kind | 32×32 result |
| --- | --- | --- | --- |
| `1584687228` | `sbs:lib:3d_gradient/3d_linear_gradient` | `x_3d_linear_gradient` | Texture |
| `1584687230` | `sbs:lib:3d_volume_mask/3d_volume_mask` | `x_3d_volume_mask` | Texture |
| `1584687244` | `sbs:lib:pattern_cube_3d/cube_3d` | `x_cube_3d` | Texture |
| `1584687246` | `sbs:lib:pattern_cube_3d/cube_3d_gbuffers` | `x_cube_3d_gbuffers` | Texture |
| `1584687273` | `sbs:lib:height_extrude/height_extrude` | `x_height_extrude` | Texture |
| `1584687281` | `sbs:lib:panorama_shape/panorama_shape` | `x_panorama_shape` | Texture |
| `1584687289` | `sbs:lib:pattern_scratches_generator/scratches_generator_normal` | `x_scratches_generator_normal` | Texture |
| `1584687298` | `sbs:lib:shape_splatter/shape_splatter` | `x_shape_splatter` | Texture |
| `1584687302` | `sbs:lib:shape_splatter/shape_splatter_blend` | `x_shape_splatter_blend` | Texture |
| `1584687304` | `sbs:lib:shape_splatter/shape_splatter_blend_color` | `x_shape_splatter_blend_color` | Texture |
| `1584687306` | `sbs:lib:shape_splatter/shape_splatter_data_extract` | `x_shape_splatter_data_extract` | Texture |
| `1584687314` | `sbs:lib:shape_splatter/shape_splatter_to_mask` | `x_shape_splatter_to_mask` | Texture |
| `1584687442` | `sbs:lib:color_space_conversion/acescg_to_linear_srgb` | `x_acescg_to_linear_srgb` | Texture |
| `1584687453` | `sbs:lib:color_space_conversion/acescg_to_srgb` | `x_acescg_to_srgb` | Texture |
| `1584688012` | `sbs:lib:quantize_color/apply_color_palette` | `x_apply_color_palette` | Texture |
| `1584689070` | `sbs:lib:quantize_color/create_color_palette_16colors` | `x_create_color_palette_16colors` | Texture |
| `1584689076` | `sbs:lib:value_get_size/get_size_color` | `x_get_size_color` | Texture |
| `1584689080` | `sbs:lib:value_get_size/get_size_grayscale` | `x_get_size_grayscale` | Texture |
| `1584689156` | `sbs:lib:hald_clut/hald_clut` | `x_hald_clut` | Texture |
| `1584689201` | `sbs:lib:hdr_range_viewer/hdr_range_viewer` | `x_hdr_range_viewer` | Texture |
| `1584689231` | `sbs:lib:hdr_range_viewer/hdr_range_viewer_grayscale` | `x_hdr_range_viewer_grayscale` | Texture |
| `1584689244` | `sbs:lib:height_map_frequencies_mapper/height_map_frequencies_mapper` | `x_height_map_frequencies_mapper` | Texture |
| `1584689495` | `sbs:lib:histogram_tools/histogram_compute` | `x_histogram_compute` | Texture |
| `1584689829` | `sbs:lib:histogram_tools/histogram_render` | `x_histogram_render` | Texture |
| `1584689939` | `sbs:lib:id_to_mask/id_to_mask_grayscale` | `x_id_to_mask_grayscale` | Texture |
| `1584689959` | `sbs:lib:lighting_cancel_hf/lighting_cancel_hf` | `x_lighting_cancel_hf` | Texture |
| `1584689993` | `sbs:lib:lighting_cancel_lf/lighting_cancel_lf` | `x_lighting_cancel_lf` | Texture |
| `1584690003` | `sbs:lib:color_space_conversion/linear_srgb_to_acescg` | `x_linear_srgb_to_acescg` | Texture |
| `1584690774` | `sbs:lib:quantize_color/modify_color_palette` | `x_modify_color_palette` | Texture |
| `1584693755` | `sbs:lib:color_space_conversion/srgb_to_acescg` | `x_srgb_to_acescg` | Texture |
| `1584700340` | `sbs:lib:median_filter/median_filter_color` | `x_median_filter_color` | Texture |
| `1584702386` | `sbs:lib:median_filter/median_filter_grayscale` | `x_median_filter_grayscale` | Texture |
| `1584703098` | `sbs:lib:mean_of_least_variance/mlv_color` | `x_mlv_color` | Texture |
| `1584703483` | `sbs:lib:mean_of_least_variance/mlv_grayscale` | `x_mlv_grayscale` | Texture |
| `1584740945` | `sbs:lib:3d_texture_position/3d_texture_position` | `x_3d_texture_position` | Texture |
| `1584748834` | `sbs:lib:3d_texture_jump_flood/3d_texture_sdf` | `x_3d_texture_sdf` | Texture |
| `1584749868` | `sbs:lib:3d_texture_render/3d_texture_surface_render` | `x_3d_texture_surface_render` | Texture |
| `1584750340` | `sbs:lib:3d_texture_render/3d_texture_volume_render_light` | `x_3d_texture_volume_render_light` | Texture |
| `1584751690` | `sbs:lib:anisotropic_kuwahara/anisotropic_kuwahara_color` | `x_anisotropic_kuwahara_color` | Texture |
| `1584752930` | `sbs:lib:anisotropic_kuwahara/anisotropic_kuwahara_grayscale` | `x_anisotropic_kuwahara_grayscale` | Texture |
| `1584756535` | `sbs:lib:directional_distance/directional_distance` | `x_directional_distance` | Texture |
| `1584756796` | `sbs:lib:extend_shape/extend_shape_color` | `x_extend_shape_color` | Texture |
| `1584757009` | `sbs:lib:extend_shape/extend_shape_grayscale` | `x_extend_shape_grayscale` | Texture |
| `1584758518` | `sbs:lib:floodfill_mapper/floodfill_mapper_color` | `x_floodfill_mapper_color` | Texture |
| `1584758945` | `sbs:lib:floodfill_mapper/floodfill_mapper_grayscale` | `x_floodfill_mapper_grayscale` | Texture |
| `1584773929` | `sbs:lib:rt_irradiance/rt_irradiance` | `x_rt_irradiance` | Texture |
| `1584774678` | `sbs:lib:rt_shadows_v2/rt_shadows` | `x_rt_shadows` | Texture |
| `1584774692` | `sbs:lib:shadows/shadows` | `x_shadows` | Texture |
| `1584776155` | `sbs:lib:normal_intensity/normal_intensity` | `x_normal_intensity` | Texture |
| `1584778294` | `sbs:lib:normal_uncombine/normal_uncombine` | `x_normal_uncombine` | Texture |
| `1584779828` | `sbs:lib:3d_texture_transform/3d_texture_offset_color` | `x_3d_texture_offset_color` | Texture |
| `1584779882` | `sbs:lib:3d_texture_transform/3d_texture_offset_grayscale` | `x_3d_texture_offset_grayscale` | Texture |
| `1584780179` | `sbs:lib:clone/clone` | `x_clone` | Texture |
| `1584780499` | `sbs:lib:non_square_transform/non_square_transform` | `x_non_square_transform` | Texture |
| `1584780600` | `sbs:lib:non_square_transform/non_square_transform_greyscale` | `x_non_square_transform_greyscale` | Texture |
| `1584780678` | `sbs:lib:non_uniform_rotation/non_uniform_rotation_color` | `x_non_uniform_rotation_color` | Texture |
| `1584780755` | `sbs:lib:non_uniform_rotation/non_uniform_rotation_grayscale` | `x_non_uniform_rotation_grayscale` | Texture |
| `1584784186` | `sbs:lib:bitmaptomaterial_2_light/Bitmap2Material_2_Light` | `x_bitmap2material_2_light` | Texture |
| `1584784367` | `sbs:lib:material_height_blend/material_height_blend` | `x_material_height_blend` | Texture |
| `1584784735` | `sbs:lib:me_snow_cover/snow_cover` | `x_snow_cover` | Texture |
| `1584785554` | `sbs:lib:me_water_level/water_level` | `x_water_level` | Texture |
| `1584785611` | `sbs:lib:material_transform/material_transform` | `x_material_transform` | Texture |
| `1584785791` | `sbs:lib:material_adjustment_blend/material_adjustment_blend` | `x_material_adjustment_blend` | Texture |
| `1584786041` | `sbs:lib:material_color_blend/material_color_blend` | `x_material_color_blend` | Texture |
| `1584786148` | `sbs:lib:material_switch/material_switch` | `x_material_switch` | Texture |
| `1584786933` | `sbs:lib:multi_material_blend/multi_material_blend` | `x_multi_material_blend` | Texture |
| `1584787058` | `sbs:lib:pbr_converter/basecolor_metallic_roughness_converter` | `x_basecolor_metallic_roughness_converter` | Texture |
| `1584787096` | `sbs:lib:pbr_albedo_safe_color/pbr_albedo_safe_color` | `x_pbr_albedo_safe_color` | Texture |
| `1584787453` | `sbs:lib:pbr_base_material/pbr_base_material` | `x_pbr_base_material` | Texture |
| `1584787540` | `sbs:lib:pbr_basecolor_metallic_validate/pbr_basecolor_metallic_validate_2` | `x_pbr_basecolor_metallic_validate_2` | Texture |
| `1584787624` | `sbs:lib:pbr_dielectric_F0/pbr_dielectric_F0` | `x_pbr_dielectric_f0` | Texture |
| `1584787673` | `sbs:lib:pbr_metal_reflectance/pbr_metal_reflectance` | `x_pbr_metal_reflectance` | Texture |
| `1584796648` | `sbs:lib:pbr_render2/PBR_render` | `x_pbr_render` | Texture |
| `1584796667` | `sbs:lib:pbr_render2/PBR_render_mapping_color` | `x_pbr_render_mapping_color` | Texture |
| `1584796684` | `sbs:lib:pbr_render2/PBR_render_mapping_grayscale` | `x_pbr_render_mapping_grayscale` | Texture |
| `1584796821` | `sbs:lib:ao_cancel/ao_cancel` | `x_ao_cancel` | Texture |
| `1584799214` | `sbs:lib:atlas_scatter/atlas_scatter` | `x_atlas_scatter` | Texture |
| `1584799684` | `sbs:lib:atlas_spliter/atlas_spliter` | `x_atlas_spliter` | Texture |
| `1584802007` | `sbs:lib:color_equalizer/color_equalizer` | `x_color_equalizer` | Texture |
| `1584802178` | `sbs:lib:crop/crop` | `x_crop` | Texture |
| `1584802301` | `sbs:lib:crop/crop_grayscale` | `x_crop_grayscale` | Texture |
| `1584802714` | `sbs:lib:clone_patch/material_clone_patch` | `x_material_clone_patch` | Texture |
| `1584802772` | `sbs:lib:crop/material_crop` | `x_material_crop` | Texture |
| `1584802806` | `sbs:lib:clone_patch/multi_clone_patch` | `x_multi_clone_patch` | Texture |
| `1584802833` | `sbs:lib:clone_patch/multi_clone_patch_grayscale` | `x_multi_clone_patch_grayscale` | Texture |
| `1584802867` | `sbs:lib:color_equalizer/multi_color_equalizer` | `x_multi_color_equalizer` | Texture |
| `1584802893` | `sbs:lib:crop/multi_crop` | `x_multi_crop` | Texture |
| `1584802920` | `sbs:lib:crop/multi_crop_grayscale` | `x_multi_crop_grayscale` | Texture |
| `1584803173` | `sbs:lib:multiangle_to_albedo/multiangle_to_albedo` | `x_multiangle_to_albedo` | Texture |
| `1584803920` | `sbs:lib:multiangle_to_normal/multiangle_to_normal` | `x_multiangle_to_normal` | Texture |
| `1584804064` | `sbs:lib:smart_auto_tile/smart_auto_tile` | `x_smart_auto_tile` | Texture |
| `1584804124` | `sbs:lib:mg_bottom_to_top/mg_bottom_to_top` | `x_mg_bottom_to_top` | Texture |
| `1584804143` | `sbs:lib:mg_cloth_wear/mg_cloth_wear` | `x_mg_cloth_wear` | Texture |
| `1584805295` | `sbs:lib:mg_dirt/mg_dirt` | `x_mg_dirt` | Texture |
| `1584805525` | `sbs:lib:mg_dripping_rust/mg_dripping_rust` | `x_mg_dripping_rust` | Texture |
| `1584806798` | `sbs:lib:mg_dust/mg_dust` | `x_mg_dust` | Texture |
| `1584806811` | `sbs:lib:mg_edge_blur/mg_edge_blur` | `x_mg_edge_blur` | Texture |
| `1584806952` | `sbs:lib:mg_edge_damages/mg_edge_damages` | `x_mg_edge_damages` | Texture |
| `1584807037` | `sbs:lib:mg_edge_dirt/mg_edge_dirt` | `x_mg_edge_dirt` | Texture |
| `1584807214` | `sbs:lib:mg_edge_notch/mg_edge_notch` | `x_mg_edge_notch` | Texture |
| `1584807237` | `sbs:lib:mg_edge_select/mg_edge_select` | `x_mg_edge_select` | Texture |
| `1584807261` | `sbs:lib:mg_edge_speckle/mg_edge_speckle` | `x_mg_edge_speckle` | Texture |
| `1584807336` | `sbs:lib:mg_edge_wear/mg_edge_wear` | `x_mg_edge_wear` | Texture |
| `1584807384` | `sbs:lib:mg_fiber_glass_edge_wear/mg_fiber_glass_edge_wear` | `x_mg_fiber_glass_edge_wear` | Texture |
| `1584807962` | `sbs:lib:mg_grease/mg_grease` | `x_mg_grease` | Texture |
| `1584808041` | `sbs:lib:mg_ground_dirt/mg_ground_dirt` | `x_mg_ground_dirt` | Texture |
| `1584808089` | `sbs:lib:mg_leaks/mg_leaks` | `x_mg_leaks` | Texture |
| `1584808118` | `sbs:lib:mg_leather_wear/mg_leather_wear` | `x_mg_leather_wear` | Texture |
| `1584808202` | `sbs:lib:mg_light/mg_light` | `x_mg_light` | Texture |
| `1584808863` | `sbs:lib:mg_mask_builder/mg_mask_builder_2` | `x_mg_mask_builder_2` | Texture |
| `1584808913` | `sbs:lib:mg_metal_edge_wear/mg_metal_edge_wear` | `x_mg_metal_edge_wear` | Texture |
| `1584808940` | `sbs:lib:mg_paint_wear/mg_paint_wear` | `x_mg_paint_wear` | Texture |
| `1584809008` | `sbs:lib:mg_selective_dirt/mg_selective_dirt` | `x_mg_selective_dirt` | Texture |
| `1584809026` | `sbs:lib:mg_sun_bleach/mg_sun_bleach` | `x_mg_sun_bleach` | Texture |
| `1584809068` | `sbs:lib:mg_surface_brush/mg_surface_brush` | `x_mg_surface_brush` | Texture |
| `1584809651` | `sbs:lib:cracks_weathering_material/cracks_weathering` | `x_cracks_weathering` | Texture |
| `1584810302` | `sbs:lib:fabric_weathering_material/fabric_weathering` | `x_fabric_weathering` | Texture |
| `1584810677` | `sbs:lib:leather_weathering_material/leather_weathering` | `x_leather_weathering` | Texture |
| `1584811994` | `sbs:lib:moss_weathering_material/moss_weathering` | `x_moss_weathering` | Texture |
| `1584812347` | `sbs:lib:painted_metal_weathering_material/painted_metal_weathering` | `x_painted_metal_weathering` | Texture |
| `1584812755` | `sbs:lib:rock_weathering_material/rock_weathering` | `x_rock_weathering` | Texture |
| `1584812976` | `sbs:lib:rust_weathering_material/rust_weathering` | `x_rust_weathering` | Texture |
| `1584813989` | `sbs:lib:3d_planar_projection/3d_planar_projection` | `x_3d_planar_projection` | Texture |
| `1584814192` | `sbs:lib:3d_planar_projection/3d_planar_projection_color` | `x_3d_planar_projection_color` | Texture |
| `1584814314` | `sbs:lib:material_mesh_data_blender/material_mesh_data_blender` | `x_material_mesh_data_blender` | Texture |
| `1584814437` | `sbs:lib:material_selector/material_selector` | `x_material_selector` | Texture |
| `1584814451` | `sbs:lib:mesh_data_combiner/mesh_data_combiner` | `x_mesh_data_combiner` | Texture |
| `1584814910` | `sbs:lib:triplanar/triplanar_3` | `x_triplanar_3` | Texture |
| `1584815344` | `sbs:lib:triplanar/triplanar_grayscale_3` | `x_triplanar_grayscale_3` | Texture |
| `1584815812` | `sbs:lib:paths_tools/path_to_spline` | `x_path_to_spline` | Texture |
| `1584818731` | `sbs:lib:spline_tools/point_list` | `x_point_list` | Texture |
| `1584819739` | `sbs:lib:spline_tools/scatter_on_spline_color` | `x_scatter_on_spline_color` | Texture |
| `1584820440` | `sbs:lib:spline_tools/scatter_on_spline_grayscale` | `x_scatter_on_spline_grayscale` | Texture |
| `1584821455` | `sbs:lib:spline_tools/scatter_splines_on_splines` | `x_scatter_splines_on_splines` | Texture |
| `1584821547` | `sbs:lib:spline_tools/spline_2dtransform` | `x_spline_2dtransform` | Texture |
| `1584821776` | `sbs:lib:spline_tools/spline_append` | `x_spline_append` | Texture |
| `1584822105` | `sbs:lib:spline_tools/spline_bridge_2_splines` | `x_spline_bridge_2_splines` | Texture |
| `1584822922` | `sbs:lib:spline_tools/spline_bridge_list_v2` | `x_spline_bridge_list_v2` | Texture |
| `1584823413` | `sbs:lib:spline_tools/spline_bridge_mapper_color` | `x_spline_bridge_mapper_color` | Texture |
| `1584823451` | `sbs:lib:spline_tools/spline_bridge_mapper_grayscale` | `x_spline_bridge_mapper_grayscale` | Texture |
| `1584823997` | `sbs:lib:spline_tools/spline_circle` | `x_spline_circle` | Texture |
| `1584824472` | `sbs:lib:spline_tools/spline_cubic` | `x_spline_cubic` | Texture |
| `1584824654` | `sbs:lib:spline_tools/spline_fill` | `x_spline_fill` | Texture |
| `1584825300` | `sbs:lib:spline_tools/spline_flow_mapper` | `x_spline_flow_mapper` | Texture |
| `1584825783` | `sbs:lib:spline_tools/spline_mapper_color` | `x_spline_mapper_color` | Texture |
| `1584825809` | `sbs:lib:spline_tools/spline_mapper_grayscale` | `x_spline_mapper_grayscale` | Texture |
| `1584826243` | `sbs:lib:spline_tools/spline_merge_list` | `x_spline_merge_list` | Texture |
| `1584826940` | `sbs:lib:spline_tools/spline_poly_quadratic` | `x_spline_poly_quadratic` | Texture |
| `1584827367` | `sbs:lib:spline_tools/spline_quadratic` | `x_spline_quadratic` | Texture |
| `1584827372` | `sbs:lib:spline_tools/spline_render` | `x_spline_render` | Texture |
| `1584827453` | `sbs:lib:spline_tools/spline_sample_height` | `x_spline_sample_height` | Texture |
| `1584827473` | `sbs:lib:spline_tools/spline_sample_thickness` | `x_spline_sample_thickness` | Texture |
| `1584827603` | `sbs:lib:spline_tools/spline_select` | `x_spline_select` | Texture |
| `1584827728` | `sbs:lib:spline_tools/spline_warp` | `x_spline_warp` | Texture |
| `1584827733` | `sbs:lib:uv_mapper/uv_mapper_color` | `x_uv_mapper_color` | Texture |
| `1584827735` | `sbs:lib:uv_mapper/uv_mapper_grayscale` | `x_uv_mapper_grayscale` | Texture |
| `1584827819` | `sbs:lib:paths_tools/2dtransform_path` | `x_2dtransform_path` | Texture |
| `1584827821` | `sbs:lib:paths_tools/mask_to_paths_2` | `x_mask_to_paths_2` | Texture |
| `1584827824` | `sbs:lib:paths_tools/path_to_spline` | `x_path_to_spline` | Texture |
| `1584828003` | `sbs:lib:paths_tools/paths_polygon` | `x_paths_polygon` | Texture |
| `1584828005` | `sbs:lib:paths_tools/paths_trace` | `x_paths_trace` | Texture |
| `1584828155` | `sbs:lib:paths_tools/paths_warp` | `x_paths_warp` | Texture |
| `1584828170` | `sbs:lib:paths_tools/quad_transform_path` | `x_quad_transform_path` | Texture |
| `1584828216` | `sbs:lib:paths_tools/select_path` | `x_select_path` | Texture |
| `1584828350` | `sbs:lib:paths_tools/vertex_processor` | `x_vertex_processor` | Texture |
| `1584828352` | `sbs:lib:paths_tools/vertex_processor_simple` | `x_vertex_processor_simple` | Texture |
| `1584828559` | `sbs:lib:blackbody/blackbody` | `x_blackbody` | Texture |
| `1584828622` | `sbs:lib:color_temperature_adjustment/color_temperature_adjustment` | `x_color_temperature_adjustment` | Texture |
| `1584828635` | `sbs:lib:exposure/exposure` | `x_exposure` | Texture |
| `1584828682` | `sbs:lib:hdr_exposure_preview/hdr_exposure_preview` | `x_hdr_exposure_preview` | Texture |
| `1584829257` | `sbs:lib:hdr_merge/hdr_merge_2` | `x_hdr_merge_2` | Texture |
| `1584829292` | `sbs:lib:panorama_nadir_extract/nadir_extract` | `x_nadir_extract` | Texture |
| `1584829310` | `sbs:lib:panorama_position/panorama_3d_position` | `x_panorama_3d_position` | Texture |
| `1584829381` | `sbs:lib:panorama_gradients/panorama_gradient_2_points` | `x_panorama_gradient_2_points` | Texture |
| `1584829404` | `sbs:lib:panorama_gradients/panorama_gradient_linear` | `x_panorama_gradient_linear` | Texture |
| `1584830014` | `sbs:lib:panorama_shape/panorama_line_light` | `x_panorama_line_light` | Texture |
| `1584830391` | `sbs:lib:panorama_nadir_patch/panorama_nadir_patch` | `x_panorama_nadir_patch` | Texture |
| `1584830983` | `sbs:lib:panorama_physical_sun_sky/panorama_physical_sun_sky` | `x_panorama_physical_sun_sky` | Texture |
| `1584831441` | `sbs:lib:panorama_shape/panorama_plane_light` | `x_panorama_plane_light` | Texture |
| `1584831549` | `sbs:lib:panorama_rotation/panorama_rotation` | `x_panorama_rotation` | Texture |
| `1584831847` | `sbs:lib:panorama_shape/panorama_shape_2` | `x_panorama_shape_2` | Texture |
| `1584832052` | `sbs:lib:panorama_shape/panorama_sphere_light` | `x_panorama_sphere_light` | Texture |
| `1584832250` | `sbs:lib:panorama_straighten_horizon/panorama_straighten_horizon` | `x_panorama_straighten_horizon` | Texture |
| `1584834378` | `sbs:lib:cardboard_001/cardboard_001` | `x_cardboard_001` | Texture |
| `1584835031` | `sbs:lib:concrete_002/concrete_002` | `x_concrete_002` | Texture |
| `1584835321` | `sbs:lib:concrete_006/concrete_006` | `x_concrete_006` | Texture |
| `1584835647` | `sbs:lib:concrete_010/concrete_010` | `x_concrete_010` | Texture |
| `1584835948` | `sbs:lib:concrete_011/concrete_011` | `x_concrete_011` | Texture |
| `1584836158` | `sbs:lib:concrete_044/concrete_044` | `x_concrete_044` | Texture |
| `1584836287` | `sbs:lib:concrete_070/concrete_070` | `x_concrete_070` | Texture |
| `1584836996` | `sbs:lib:fabric_002/fabric_002` | `x_fabric_002` | Texture |
| `1584837273` | `sbs:lib:fabric_009/fabric_009` | `x_fabric_009` | Texture |
| `1584838663` | `sbs:lib:fabric_025/fabric_025` | `x_fabric_025` | Texture |
| `1584839008` | `sbs:lib:gravel/gravel` | `x_gravel` | Texture |
| `1584839491` | `sbs:lib:metal_002/metal_002` | `x_metal_002` | Texture |
| `1584840061` | `sbs:lib:metal_003/metal_003` | `x_metal_003` | Texture |
| `1584840263` | `sbs:lib:metal_006/metal_006` | `x_metal_006` | Texture |
| `1584840449` | `sbs:lib:metal_plate_001/metal_plate_001` | `x_metal_plate_001` | Texture |
| `1584840983` | `sbs:lib:metal_plate_004/metal_plate_004` | `x_metal_plate_004` | Texture |
| `1584841298` | `sbs:lib:old_painted_planks/old_painted_planks` | `x_old_painted_planks` | Texture |
| `1584841700` | `sbs:lib:paper_005/paper_005` | `x_paper_005` | Texture |
| `1584841869` | `sbs:lib:rough_concrete_with_lines/rough_concrete_with_lines` | `x_rough_concrete_with_lines` | Texture |
| `1584842120` | `sbs:lib:tiles_002/tiles_002` | `x_tiles_002` | Texture |
| `1584842397` | `sbs:lib:tiles_008/tiles_008` | `x_tiles_008` | Texture |
| `1584842671` | `sbs:lib:tiles_011/tiles_011` | `x_tiles_011` | Texture |
| `1584842975` | `sbs:lib:tiles_025/tiles_025` | `x_tiles_025` | Texture |
| `1584843264` | `sbs:lib:wood_american_cherry/wood_american_cherry` | `x_wood_american_cherry` | Texture |

## Approximate mappings (505)

| Node UID | Native kind | Renderer kind | Definition | 32×32 result | Limitations / errors |
| --- | --- | --- | --- | --- | --- |
| `1359211383` | `sbs:normal` | `normal` | Present | Texture | values are converted to the units of MaterialCraft's Normal From Height node; check the result |
| `1584632887` | `sbs:blur` | `blur` | Present | Texture | blur strength is converted (intensity / 1.5 as a Gaussian deviation); compare with a reference render |
| `1584632925` | `sbs:dirmotionblur` | `directional_blur` | Present | Texture | blur length is converted from pixels at 256 |
| `1584632931` | `sbs:directionalwarp` | `directional_warp` | Present | Texture | values are converted to the units of MaterialCraft's Directional Warp node; check the result |
| `1584632937` | `sbs:distance` | `distance` | Present | Texture | values are converted to the units of MaterialCraft's Distance node; check the result |
| `1584632943` | `sbs:emboss` | `emboss` | Present | Texture | the emboss lighting follows Substance Designer's model (highlight and shadow colours on the height map's slopes); its strength is not yet matched against a reference render |
| `1584632949` | `sbs:fxmaps` | `fx_map` | Present | Texture | drawn by MaterialCraft's quad-tree engine: the layout follows the file, random numbers differ |
| `1584632980` | `sbs:hsl` | `hsl` | Present | Texture | saturation and lightness are converted to multipliers |
| `1584633042` | `sbs:normal` | `normal` | Present | Texture | values are converted to the units of MaterialCraft's Normal From Height node; check the result |
| `1584633054` | `sbs:pixelprocessor` | `pixel_processor` | Present | Texture | the per-pixel function is a constant |
| `1584633061` | `sbs:sharpen` | `sharpen` | Present | Texture | values are converted to the units of MaterialCraft's Sharpen node; check the result |
| `1584633070` | `sbs:svg` | `svg` | Present | Texture | values are converted to the units of MaterialCraft's SVG node; check the result |
| `1584633100` | `sbs:valueprocessor` | `value_processor` | Present | Texture | the value processor has no function; it gives 0, like an empty one in the file's own editor |
| `1584633107` | `sbs:warp` | `warp` | Present | Texture | values are converted to the units of MaterialCraft's Warp node; check the result |
| `1584687025` | `sbs:lib:3d_perlin_noise/3D_perlin_noise` | `noise_3d` | Present | Texture | without a position map the noise is read on the texture plane |
| `1584687027` | `sbs:lib:3d_perlin_noise/3D_perlin_noise_fractal` | `noise_3d` | Present | Texture | without a position map the noise is read on the texture plane |
| `1584687029` | `sbs:lib:3d_perlin_noise/3D_ridged_noise_fractal` | `noise_3d` | Present | Texture | without a position map the noise is read on the texture plane |
| `1584687031` | `sbs:lib:3d_simplex_noise/3d_simplex_noise` | `noise_3d` | Present | Texture | without a position map the noise is read on the texture plane |
| `1584687033` | `sbs:lib:noise_voronoi/3d_voronoi` | `noise_3d` | Present | Texture | without a position map the noise is read on the texture plane |
| `1584687035` | `sbs:lib:noise_voronoi/3d_voronoi_fractal` | `noise_3d` | Present | Texture | without a position map the noise is read on the texture plane |
| `1584687037` | `sbs:lib:3d_worley_noise/3D_worley_noise` | `noise_3d` | Present | Texture | without a position map the noise is read on the texture plane |
| `1584687039` | `sbs:lib:noise_anisotropic_noise/anisotropic_noise` | `anisotropic_noise` | Present | Texture | Own renderer implementation. |
| `1584687041` | `sbs:lib:blue_noise/blue_noise_fast` | `blue_noise` | Present | Texture | Own renderer implementation. |
| `1584687043` | `sbs:lib:noise_bnw_spots_1/bnw_spots_1` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687045` | `sbs:lib:noise_bnw_spots_2/bnw_spots_2` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687047` | `sbs:lib:noise_bnw_spots_3/bnw_spots_3` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687049` | `sbs:lib:noise_cells_1/cells_1` | `noise_pattern` | Present | Texture | Own renderer implementation. |
| `1584687051` | `sbs:lib:noise_cells_2/cells_2` | `noise_pattern` | Present | Texture | Own renderer implementation. |
| `1584687053` | `sbs:lib:noise_cells_3/cells_3` | `noise_pattern` | Present | Texture | Own renderer implementation. |
| `1584687055` | `sbs:lib:noise_cells_4/cells_4` | `noise_pattern` | Present | Texture | Own renderer implementation. |
| `1584687057` | `sbs:lib:noise_clouds_1/clouds_1` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687059` | `sbs:lib:noise_clouds_2/clouds_2` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687061` | `sbs:lib:noise_clouds_3/clouds_3` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687063` | `sbs:lib:noise_creased/creased` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687065` | `sbs:lib:noise_crystal_1/crystal_1` | `noise_pattern` | Present | Texture | Own renderer implementation. |
| `1584687067` | `sbs:lib:noise_crystal_2/crystal_2` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687069` | `sbs:lib:noise_directional_noise_1/directional_noise_1` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687071` | `sbs:lib:noise_directional_noise_2/directional_noise_2` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687073` | `sbs:lib:noise_directional_noise_3/directional_noise_3` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687075` | `sbs:lib:noise_directional_noise_4/directional_noise_4` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687077` | `sbs:lib:noise_directional_scratches/directional_scratches` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687079` | `sbs:lib:noise_dirt_1/dirt_1` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687081` | `sbs:lib:noise_dirt_2/dirt_2` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687083` | `sbs:lib:noise_dirt_3/dirt_3` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687085` | `sbs:lib:noise_dirt_4/dirt_4` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687087` | `sbs:lib:noise_dirt_5/dirt_5` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687089` | `sbs:lib:noise_dirt_gradient/dirt_gradient` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687091` | `sbs:lib:noise_fluid/fluid` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687093` | `sbs:lib:noise_fractal_sum_1/fractal_sum_1` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687095` | `sbs:lib:noise_fractal_sum_2/fractal_sum_2` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687097` | `sbs:lib:noise_fractal_sum_3/fractal_sum_3` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687099` | `sbs:lib:noise_fractal_sum_4/fractal_sum_4` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687101` | `sbs:lib:noise_fractal_sum_base/fractal_sum_base_2` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687103` | `sbs:lib:noise_fur_1/fur_1` | `fur_noise` | Present | Texture | fur is drawn with MaterialCraft's own hairs; the look is similar, not identical |
| `1584687105` | `sbs:lib:noise_fur_2/fur_2` | `fur_noise` | Present | Texture | fur is drawn with MaterialCraft's own hairs; the look is similar, not identical |
| `1584687107` | `sbs:lib:noise_fur_3/fur_3` | `fur_noise` | Present | Texture | fur is drawn with MaterialCraft's own hairs; the look is similar, not identical |
| `1584687109` | `sbs:lib:noise_gaussian_noise/gaussian_noise` | `noise_pattern` | Present | Texture | Own renderer implementation. |
| `1584687111` | `sbs:lib:noise_gaussian_spots_1/gaussian_spots_1` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687113` | `sbs:lib:noise_gaussian_spots_2/gaussian_spots_2` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687115` | `sbs:lib:grunge_concrete/grunge_concrete` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687117` | `sbs:lib:grunge_damas/grunge_damas` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687119` | `sbs:lib:grunge_galvanic_large/grunge_galvanic_large` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687121` | `sbs:lib:grunge_galvanic_small/grunge_galvanic_small` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687123` | `sbs:lib:grunge_leaks/grunge_leaks` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687125` | `sbs:lib:grunge_leaky_paint/grunge_leaky_paint` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687127` | `sbs:lib:noise_grunge_map_001/grunge_map_001` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687129` | `sbs:lib:noise_grunge_map_002/grunge_map_002` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687131` | `sbs:lib:noise_grunge_map_003/grunge_map_003` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687133` | `sbs:lib:noise_grunge_map_004/grunge_map_004` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687135` | `sbs:lib:noise_grunge_map_005/grunge_map_005` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687137` | `sbs:lib:noise_grunge_map_006/grunge_map_006` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687139` | `sbs:lib:noise_grunge_map_007/grunge_map_007` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687141` | `sbs:lib:noise_grunge_map_008/grunge_map_008` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687143` | `sbs:lib:noise_grunge_map_009/grunge_map_009` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687145` | `sbs:lib:noise_grunge_map_010/grunge_map_010` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687147` | `sbs:lib:noise_grunge_map_011/grunge_map_011` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687149` | `sbs:lib:noise_grunge_map_012/grunge_map_012` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687151` | `sbs:lib:noise_grunge_map_013/grunge_map_013` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687153` | `sbs:lib:noise_grunge_map_014/grunge_map_014` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687155` | `sbs:lib:noise_grunge_map_015/grunge_map_015` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687157` | `sbs:lib:grunge_rough_dirty/grunge_rough_dirty` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687159` | `sbs:lib:grunge_rust_fine/grunge_rust_fine` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687161` | `sbs:lib:grunge_scratches_dirty/grunge_scratches_dirty` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687163` | `sbs:lib:grunge_scratches_fine/grunge_scratches_fine` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687165` | `sbs:lib:grunge_scratches_rough/grunge_scratches_rough` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687167` | `sbs:lib:grunge_shavings/grunge_shavings` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687169` | `sbs:lib:grunge_splashes_dusty/grunge_splashes_dusty` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687171` | `sbs:lib:grunge_spots/grunge_spots` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687173` | `sbs:lib:grunge_spots_dirty/grunge_spots_dirty` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687175` | `sbs:lib:noise_liquid/liquid` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687177` | `sbs:lib:noise_messy_fibers_1/messy_fibers_1` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687179` | `sbs:lib:noise_messy_fibers_2/messy_fibers_2` | `scratches` | Present | Texture | Own renderer implementation. |
| `1584687181` | `sbs:lib:noise_messy_fibers_3/messy_fibers_3` | `scratches` | Present | Texture | Own renderer implementation. |
| `1584687183` | `sbs:lib:noise_microscope_view/microscope_view` | `microscope_noise` | Present | Texture | Own renderer implementation. |
| `1584687185` | `sbs:lib:noise_moisture_noise/moisture_noise` | `preset_noise` | Present | Texture | Own renderer implementation. |
| `1584687187` | `sbs:lib:noise_perlin_noise/perlin_noise` | `noise_pattern` | Present | Texture | Own renderer implementation. |
| `1584687189` | `sbs:lib:noise_plasma/plasma` | `noise_pattern` | Present | Texture | Own renderer implementation. |
| `1584687191` | `sbs:lib:rt_caustics/rt_caustics_color` | `caustics` | Present | Texture | the caustics are MaterialCraft's own pattern, not traced through the input surface |
| `1584687193` | `sbs:lib:rt_caustics/rt_caustics_grayscale` | `caustics` | Present | Texture | the caustics are MaterialCraft's own pattern, not traced through the input surface |
| `1584687195` | `sbs:lib:noise_voronoi/voronoi` | `voronoi` | Present | Texture | Own renderer implementation. |
| `1584687197` | `sbs:lib:noise_voronoi/voronoi_fractal` | `voronoi_fractal` | Present | Texture | Own renderer implementation. |
| `1584687199` | `sbs:lib:waveform_1/waveform_1` | `waveform` | Present | Texture | Own renderer implementation. |
| `1584687201` | `sbs:lib:white_noise/white_noise_fast` | `white_noise` | Present | Texture | Own renderer implementation. |
| `1584687228` | `sbs:lib:3d_gradient/3d_linear_gradient` | `x_3d_linear_gradient` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584687230` | `sbs:lib:3d_volume_mask/3d_volume_mask` | `x_3d_volume_mask` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584687232` | `sbs:lib:pattern_alveolus/alveolus` | `tile_preset` | Present | Texture | Own renderer implementation. |
| `1584687234` | `sbs:lib:pattern_arc_pavement/arc_pavement` | `arc_pavement` | Present | Texture | Own renderer implementation. |
| `1584687236` | `sbs:lib:pattern_brick_1/brick_1` | `tile_preset` | Present | Texture | Own renderer implementation. |
| `1584687238` | `sbs:lib:pattern_brick_2/brick_2` | `tile_preset` | Present | Texture | Own renderer implementation. |
| `1584687240` | `sbs:lib:pattern_brick_generator/brick_generator` | `tile_preset` | Present | Texture | Own renderer implementation. |
| `1584687242` | `sbs:lib:pattern_checker_1/checker_1` | `checker` | Present | Texture | Own renderer implementation. |
| `1584687244` | `sbs:lib:pattern_cube_3d/cube_3d` | `x_cube_3d` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584687246` | `sbs:lib:pattern_cube_3d/cube_3d_gbuffers` | `x_cube_3d_gbuffers` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584687251` | `sbs:lib:pattern_fibers_1/fibers_1` | `weave_preset` | Present | Texture | Own renderer implementation. |
| `1584687253` | `sbs:lib:pattern_fibers_2/fibers_2` | `weave_preset` | Present | Texture | Own renderer implementation. |
| `1584687255` | `sbs:lib:pattern_gaussian_1/gaussian_1` | `tile_preset` | Present | Texture | Own renderer implementation. |
| `1584687257` | `sbs:lib:pattern_gaussian_2/gaussian_2` | `tile_preset` | Present | Texture | Own renderer implementation. |
| `1584687259` | `sbs:lib:gradient_axial/gradient_axial` | `gradient_2pt` | Present | Texture | Own renderer implementation. |
| `1584687261` | `sbs:lib:gradient_axial/gradient_axial_reflected` | `gradient_2pt` | Present | Texture | the reflected axial gradient is not yet checked against a reference render |
| `1584687263` | `sbs:lib:gradient_circular/gradient_circular` | `gradient_2pt` | Present | Texture | Own renderer implementation. |
| `1584687265` | `sbs:lib:gradient_linear_1/gradient_linear_1` | `gradient_2pt` | Present | Texture | Own renderer implementation. |
| `1584687267` | `sbs:lib:gradient_linear_2/gradient_linear_2` | `gradient_2pt` | Present | Texture | Own renderer implementation. |
| `1584687269` | `sbs:lib:gradient_linear_3/gradient_linear_3` | `gradient_2pt` | Present | Texture | Own renderer implementation. |
| `1584687271` | `sbs:lib:gradient_radial/gradient_radial` | `gradient_2pt` | Present | Texture | Own renderer implementation. |
| `1584687273` | `sbs:lib:height_extrude/height_extrude` | `x_height_extrude` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584687276` | `sbs:lib:pattern_mesh_1/mesh_1` | `tile_preset` | Present | Texture | Own renderer implementation. |
| `1584687279` | `sbs:lib:pattern_mesh_2/mesh_2` | `tile_preset` | Present | Texture | Own renderer implementation. |
| `1584687281` | `sbs:lib:panorama_shape/panorama_shape` | `x_panorama_shape` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584687283` | `sbs:lib:pattern_polygon_1/polygon_1` | `polygon` | Present | Texture | Own renderer implementation. |
| `1584687285` | `sbs:lib:pattern_polygon_2/polygon_2` | `polygon` | Present | Texture | Own renderer implementation. |
| `1584687287` | `sbs:lib:pattern_scratches_generator/scratches_generator` | `scratches` | Present | Texture | the scratches are drawn with MaterialCraft's own strokes |
| `1584687289` | `sbs:lib:pattern_scratches_generator/scratches_generator_normal` | `x_scratches_generator_normal` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584687292` | `sbs:lib:pattern_shape/shape` | `shape` | Present | Texture | Own renderer implementation. |
| `1584687294` | `sbs:lib:shape_extrude/shape_extrude_2` | `shape_extrude` | Present | Texture | Own renderer implementation. |
| `1584687296` | `sbs:lib:shape_mapper/shape_mapper` | `shape_mapper` | Present | Texture | Own renderer implementation. |
| `1584687298` | `sbs:lib:shape_splatter/shape_splatter` | `x_shape_splatter` (was a placeholder) | Present | Texture | splatter data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' splatter images |
| `1584687302` | `sbs:lib:shape_splatter/shape_splatter_blend` | `x_shape_splatter_blend` (was a placeholder) | Present | Texture | splatter data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' splatter images |
| `1584687304` | `sbs:lib:shape_splatter/shape_splatter_blend_color` | `x_shape_splatter_blend_color` (was a placeholder) | Present | Texture | splatter data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' splatter images |
| `1584687306` | `sbs:lib:shape_splatter/shape_splatter_data_extract` | `x_shape_splatter_data_extract` (was a placeholder) | Present | Texture | splatter data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' splatter images |
| `1584687314` | `sbs:lib:shape_splatter/shape_splatter_to_mask` | `x_shape_splatter_to_mask` (was a placeholder) | Present | Texture | splatter data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' splatter images |
| `1584687316` | `sbs:lib:splatter/splatter` | `splatter` | Present | Texture | Own renderer implementation. |
| `1584687318` | `sbs:lib:pattern_splatter_circular/splatter_circular` | `splatter_circular` | Present | Texture | Own renderer implementation. |
| `1584687320` | `sbs:lib:pattern_splatter_circular/splatter_circular_color` | `splatter_circular` | Present | Texture | Own renderer implementation. |
| `1584687322` | `sbs:lib:splatter_color/splatter_color` | `splatter` | Present | Texture | Own renderer implementation. |
| `1584687324` | `sbs:lib:pattern_star/star` | `star` | Present | Texture | the star is not yet checked against a reference render |
| `1584687326` | `sbs:lib:starburst/starburst` | `starburst` | Present | Texture | Own renderer implementation. |
| `1584687328` | `sbs:lib:pattern_stripes/stripes` | `stripes` | Present | Texture | Own renderer implementation. |
| `1584687330` | `sbs:lib:pattern_tile_generator/tile_generator` | `tile_generator` | Present | Texture | Own renderer implementation. |
| `1584687332` | `sbs:lib:pattern_tile_generator/tile_generator_color` | `tile_generator` | Present | Texture | Own renderer implementation. |
| `1584687334` | `sbs:lib:pattern_tile_random/tile_random` | `tile_preset` | Present | Texture | random tiles follow the same rules with their own random choices |
| `1584687336` | `sbs:lib:pattern_tile_random_2/tile_random_2` | `tile_random2` | Present | Texture | Own renderer implementation. |
| `1584687339` | `sbs:lib:pattern_tile_random/tile_random_color` | `tile_preset` | Present | Texture | random tiles follow the same rules with their own random choices |
| `1584687341` | `sbs:lib:pattern_tile_sampler/tile_sampler` | `tile_sampler` | Present | Texture | Own renderer implementation. |
| `1584687343` | `sbs:lib:pattern_tile_sampler/tile_sampler_color` | `tile_sampler` | Present | Texture | blending mode 2 is assumed to be our 'Add' mode |
| `1584687345` | `sbs:lib:pattern_triangle_grid/triangle_grid_color` | `triangle_grid` | Present | Texture | Own renderer implementation. |
| `1584687347` | `sbs:lib:pattern_triangle_grid/triangle_grid_grayscale` | `triangle_grid` | Present | Texture | Own renderer implementation. |
| `1584687349` | `sbs:lib:pattern_weave_1/weave_1` | `weave_preset` | Present | Texture | Own renderer implementation. |
| `1584687351` | `sbs:lib:pattern_weave_2/weave_2` | `weave_preset` | Present | Texture | Own renderer implementation. |
| `1584687353` | `sbs:lib:pattern_weave_generator/weave_generator` | `weave_generator` | Present | Texture | Own renderer implementation. |
| `1584687442` | `sbs:lib:color_space_conversion/acescg_to_linear_srgb` | `x_acescg_to_linear_srgb` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584687453` | `sbs:lib:color_space_conversion/acescg_to_srgb` | `x_acescg_to_srgb` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584688012` | `sbs:lib:quantize_color/apply_color_palette` | `x_apply_color_palette` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584688014` | `sbs:lib:auto_levels/auto_levels` | `auto_levels` | Present | Texture | Own renderer implementation. |
| `1584688654` | `sbs:lib:channel_mixer/channel_mixer` | `channel_mixer` | Present | Texture | Own renderer implementation. |
| `1584688662` | `sbs:lib:chrominance_extract/chrominance_extract` | `chrominance_extract` | Present | Texture | Own renderer implementation. |
| `1584688692` | `sbs:lib:clamp/clamp` | `clamp` | Present | Texture | Own renderer implementation. |
| `1584688699` | `sbs:lib:clamp/clamp_grayscale` | `clamp` | Present | Texture | Own renderer implementation. |
| `1584688832` | `sbs:lib:color_match/color_match` | `color_match` | Present | Texture | Own renderer implementation. |
| `1584688896` | `sbs:lib:color_to_mask/color_to_mask` | `color_to_mask` | Present | Texture | Own renderer implementation. |
| `1584688958` | `sbs:lib:contrast_luminosity/contrast_luminosity` | `contrast_luminosity` | Present | Texture | Own renderer implementation. |
| `1584688960` | `sbs:lib:contrast_luminosity/contrast_luminosity_grayscale` | `contrast_luminosity` | Present | Texture | Own renderer implementation. |
| `1584689070` | `sbs:lib:quantize_color/create_color_palette_16colors` | `x_create_color_palette_16colors` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584689076` | `sbs:lib:value_get_size/get_size_color` | `x_get_size_color` (was a placeholder) | Present | Texture | the size is written as pixel counts in the red and green channels |
| `1584689080` | `sbs:lib:value_get_size/get_size_grayscale` | `x_get_size_grayscale` (was a placeholder) | Present | Texture | the size is written as pixel counts in the red and green channels |
| `1584689106` | `sbs:lib:grayscale_conversion_advanced/grayscale_conversion_advanced` | `grayscale_conversion` | Present | Texture | Own renderer implementation. |
| `1584689156` | `sbs:lib:hald_clut/hald_clut` | `x_hald_clut` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584689201` | `sbs:lib:hdr_range_viewer/hdr_range_viewer` | `x_hdr_range_viewer` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584689231` | `sbs:lib:hdr_range_viewer/hdr_range_viewer_grayscale` | `x_hdr_range_viewer_grayscale` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584689244` | `sbs:lib:height_map_frequencies_mapper/height_map_frequencies_mapper` | `x_height_map_frequencies_mapper` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584689253` | `sbs:lib:highpass/highpass` | `highpass` | Present | Texture | Own renderer implementation. |
| `1584689260` | `sbs:lib:highpass/highpass_grayscale` | `highpass` | Present | Texture | Own renderer implementation. |
| `1584689495` | `sbs:lib:histogram_tools/histogram_compute` | `x_histogram_compute` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584689523` | `sbs:lib:histogram_tools/histogram_equalize` | `histogram_equalize` | Present | Texture | Own renderer implementation. |
| `1584689549` | `sbs:lib:histogram_range/histogram_range` | `histogram_range` | Present | Texture | Own renderer implementation. |
| `1584689829` | `sbs:lib:histogram_tools/histogram_render` | `x_histogram_render` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584689831` | `sbs:lib:histogram_scan/histogram_scan` | `histogram_scan` | Present | Texture | Own renderer implementation. |
| `1584689879` | `sbs:lib:histogram_scan_non_uniform/histogram_scan_non_uniform` | `histogram_scan` | Present | Texture | Own renderer implementation. |
| `1584689881` | `sbs:lib:histogram_select/histogram_select` | `histogram_range` | Present | Texture | Own renderer implementation. |
| `1584689911` | `sbs:lib:histogram_shift/histogram_shift` | `histogram_shift` | Present | Texture | Own renderer implementation. |
| `1584689939` | `sbs:lib:id_to_mask/id_to_mask_grayscale` | `x_id_to_mask_grayscale` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584689949` | `sbs:lib:invert/invert` | `invert` | Present | Texture | Own renderer implementation. |
| `1584689951` | `sbs:lib:invert/invert_grayscale` | `invert` | Present | Texture | Own renderer implementation. |
| `1584689959` | `sbs:lib:lighting_cancel_hf/lighting_cancel_hf` | `x_lighting_cancel_hf` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584689993` | `sbs:lib:lighting_cancel_lf/lighting_cancel_lf` | `x_lighting_cancel_lf` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584690003` | `sbs:lib:color_space_conversion/linear_srgb_to_acescg` | `x_linear_srgb_to_acescg` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584690005` | `sbs:lib:color_space_conversion/linear_srgb_to_srgb` | `linear_to_srgb` | Present | Texture | Own renderer implementation. |
| `1584690007` | `sbs:lib:color_space_conversion/linear_srgb_to_srgb_grayscale` | `linear_to_srgb` | Present | Texture | Own renderer implementation. |
| `1584690029` | `sbs:lib:luminance_highpass/luminance_highpass` | `highpass` | Present | Texture | Own renderer implementation. |
| `1584690585` | `sbs:lib:min_max/min_max` | `min_max` | Present | Texture | Own renderer implementation. |
| `1584690774` | `sbs:lib:quantize_color/modify_color_palette` | `x_modify_color_palette` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584690805` | `sbs:lib:pow/pow` | `pow` | Present | Texture | Own renderer implementation. |
| `1584690826` | `sbs:lib:pow/pow_grayscale` | `pow` | Present | Texture | Own renderer implementation. |
| `1584690884` | `sbs:lib:quantize/quantize` | `quantize` | Present | Texture | Own renderer implementation. |
| `1584693593` | `sbs:lib:quantize_color/quantize_color` | `quantize_color` | Present | Texture | Own renderer implementation. |
| `1584693679` | `sbs:lib:quantize_2/quantize_grayscale` | `quantize` | Present | Texture | Own renderer implementation. |
| `1584693711` | `sbs:lib:replace_color/replace_color` | `replace_color` | Present | Texture | Own renderer implementation. |
| `1584693744` | `sbs:lib:replace_color/replace_color_range` | `replace_color` | Present | Texture | Own renderer implementation. |
| `1584693755` | `sbs:lib:color_space_conversion/srgb_to_acescg` | `x_srgb_to_acescg` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584693765` | `sbs:lib:color_space_conversion/srgb_to_linear_srgb` | `srgb_to_linear` | Present | Texture | Own renderer implementation. |
| `1584693771` | `sbs:lib:color_space_conversion/srgb_to_linear_srgb_grayscale` | `srgb_to_linear` | Present | Texture | Own renderer implementation. |
| `1584693806` | `sbs:lib:threshold/threshold` | `threshold` | Present | Texture | Own renderer implementation. |
| `1584693879` | `sbs:lib:blend/color` | `color` | Present | Texture | Own renderer implementation. |
| `1584693909` | `sbs:lib:blend/color_burn` | `blend` | Present | Texture | Own renderer implementation. |
| `1584693947` | `sbs:lib:blend/color_dodge` | `blend` | Present | Texture | Own renderer implementation. |
| `1584693969` | `sbs:lib:blend/difference` | `blend` | Present | Texture | Own renderer implementation. |
| `1584693987` | `sbs:lib:blend/dissolve_2` | `blend` | Present | Texture | Own renderer implementation. |
| `1584694011` | `sbs:lib:blend/linear_burn` | `blend` | Present | Texture | Own renderer implementation. |
| `1584694028` | `sbs:lib:blend/luminosity` | `blend` | Present | Texture | Own renderer implementation. |
| `1584694164` | `sbs:lib:blend_switch/multi_switch` | `multi_switch` | Present | Texture | Own renderer implementation. |
| `1584694166` | `sbs:lib:blend_switch/multi_switch_grayscale` | `multi_switch` | Present | Texture | Own renderer implementation. |
| `1584694168` | `sbs:lib:blend_switch/switch` | `switch` | Present | Texture | Own renderer implementation. |
| `1584694170` | `sbs:lib:blend_switch/switch_grayscale` | `switch` | Present | Texture | Own renderer implementation. |
| `1584694260` | `sbs:lib:anisotropic_blur/anisotropic_blur` | `anisotropic_blur` | Present | Texture | Own renderer implementation. |
| `1584694317` | `sbs:lib:anisotropic_blur/anisotropic_blur_grayscale` | `anisotropic_blur` | Present | Texture | Own renderer implementation. |
| `1584694319` | `sbs:lib:blur_hq/blur_hq` | `blur_hq` | Present | Texture | Own renderer implementation. |
| `1584694321` | `sbs:lib:blur_hq/blur_hq_grayscale` | `blur_hq` | Present | Texture | Own renderer implementation. |
| `1584700340` | `sbs:lib:median_filter/median_filter_color` | `x_median_filter_color` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584702386` | `sbs:lib:median_filter/median_filter_grayscale` | `x_median_filter_grayscale` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584703098` | `sbs:lib:mean_of_least_variance/mlv_color` | `x_mlv_color` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584703483` | `sbs:lib:mean_of_least_variance/mlv_grayscale` | `x_mlv_grayscale` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584704474` | `sbs:lib:non_uniform_blur/non_uniform_blur` | `non_uniform_blur` | Present | Texture | Own renderer implementation. |
| `1584704476` | `sbs:lib:non_uniform_blur/non_uniform_blur_grayscale` | `non_uniform_blur` | Present | Texture | Own renderer implementation. |
| `1584704743` | `sbs:lib:radial_blur/radial_blur` | `radial_blur` | Present | Texture | Own renderer implementation. |
| `1584704745` | `sbs:lib:radial_blur/radial_blur_grayscale` | `radial_blur` | Present | Texture | Own renderer implementation. |
| `1584705376` | `sbs:lib:slope_blur/slope_blur` | `slope_blur` | Present | Texture | Own renderer implementation. |
| `1584706007` | `sbs:lib:slope_blur/slope_blur_grayscale_2` | `slope_blur` | Present | Texture | Own renderer implementation. |
| `1584706037` | `sbs:lib:pre_multiply_straight/premult_to_straight` | `premultiply` | Present | Texture | Own renderer implementation. |
| `1584706039` | `sbs:lib:rgb-a_merge/rgb-a_merge` | `alpha_merge` | Present | Texture | Own renderer implementation. |
| `1584706041` | `sbs:lib:rgb-a_split/rgb-a_split` | `alpha_split` | Present | Texture | Own renderer implementation. |
| `1584706044` | `sbs:lib:rgba_merge/rgba_merge` | `merge` | Present | Texture | Own renderer implementation. |
| `1584706055` | `sbs:lib:rgba_split/rgba_split` | `rgba_split` | Present | Texture | Own renderer implementation. |
| `1584706060` | `sbs:lib:pre_multiply_straight/straight_to_premult` | `premultiply` | Present | Texture | Own renderer implementation. |
| `1584740945` | `sbs:lib:3d_texture_position/3d_texture_position` | `x_3d_texture_position` (was a placeholder) | Present | Texture | 3D textures are slice atlases in MaterialCraft's own layout (grid of slices, first slice top left) |
| `1584748834` | `sbs:lib:3d_texture_jump_flood/3d_texture_sdf` | `x_3d_texture_sdf` (was a placeholder) | Present | Texture | 3D textures are slice atlases in MaterialCraft's own layout (grid of slices, first slice top left) |
| `1584749868` | `sbs:lib:3d_texture_render/3d_texture_surface_render` | `x_3d_texture_surface_render` (was a placeholder) | Present | Texture | 3D textures are slice atlases in MaterialCraft's own layout (grid of slices, first slice top left) |
| `1584750340` | `sbs:lib:3d_texture_render/3d_texture_volume_render_light` | `x_3d_texture_volume_render_light` (was a placeholder) | Present | Texture | 3D textures are slice atlases in MaterialCraft's own layout (grid of slices, first slice top left) |
| `1584751690` | `sbs:lib:anisotropic_kuwahara/anisotropic_kuwahara_color` | `x_anisotropic_kuwahara_color` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584752930` | `sbs:lib:anisotropic_kuwahara/anisotropic_kuwahara_grayscale` | `x_anisotropic_kuwahara_grayscale` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584753071` | `sbs:lib:bevel/bevel` | `bevel` | Present | Texture | Own renderer implementation. |
| `1584753935` | `sbs:lib:bevel_smooth/bevel_smooth` | `bevel` | Present | Texture | Own renderer implementation. |
| `1584754547` | `sbs:lib:cross_section/cross_section` | `cross_section` | Present | Texture | Own renderer implementation. |
| `1584754596` | `sbs:lib:curvature/curvature` | `curvature` | Present | Texture | Own renderer implementation. |
| `1584755262` | `sbs:lib:curvature_smooth_v2/curvature_smooth_v2` | `curvature_smooth` | Present | Texture | Own renderer implementation. |
| `1584755433` | `sbs:lib:curvature_sobel/curvature_sobel` | `normal_sobel` | Present | Texture | Own renderer implementation. |
| `1584755966` | `sbs:lib:diffusion/diffusion_color` | `diffusion` | Present | Texture | Own renderer implementation. |
| `1584756313` | `sbs:lib:diffusion/diffusion_grayscale` | `diffusion` | Present | Texture | Own renderer implementation. |
| `1584756535` | `sbs:lib:directional_distance/directional_distance` | `x_directional_distance` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584756579` | `sbs:lib:edge_detect/edge_detect` | `edge_detect` | Present | Texture | Own renderer implementation. |
| `1584756604` | `sbs:lib:emboss_with_gloss/emboss_with_gloss` | `emboss` | Present | Texture | the emboss lighting and gloss follow Substance Designer's model; their strength is not yet matched against a reference render |
| `1584756796` | `sbs:lib:extend_shape/extend_shape_color` | `x_extend_shape_color` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584757009` | `sbs:lib:extend_shape/extend_shape_grayscale` | `x_extend_shape_grayscale` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584757618` | `sbs:lib:flood_fill_2/flood_fill` | `flood_fill` | Present | Texture | Own renderer implementation. |
| `1584757638` | `sbs:lib:flood_fill_2/flood_fill_to_bbox_size` | `flood_fill_to_bbox_size` | Present | Texture | Own renderer implementation. |
| `1584757726` | `sbs:lib:flood_fill_2/flood_fill_to_color` | `flood_fill_color` | Present | Texture | Own renderer implementation. |
| `1584757829` | `sbs:lib:flood_fill_2/flood_fill_to_gradient_2` | `flood_fill_gradient` | Present | Texture | Own renderer implementation. |
| `1584757887` | `sbs:lib:flood_fill_2/flood_fill_to_grayscale` | `flood_fill_random` | Present | Texture | Own renderer implementation. |
| `1584757917` | `sbs:lib:flood_fill_2/flood_fill_to_position` | `flood_fill_to_position` | Present | Texture | Own renderer implementation. |
| `1584757957` | `sbs:lib:flood_fill_2/flood_fill_to_random_color` | `flood_fill_color` | Present | Texture | Own renderer implementation. |
| `1584757989` | `sbs:lib:flood_fill_2/flood_fill_to_random_grayscale` | `flood_fill_random` | Present | Texture | Own renderer implementation. |
| `1584758518` | `sbs:lib:floodfill_mapper/floodfill_mapper_color` | `x_floodfill_mapper_color` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584758945` | `sbs:lib:floodfill_mapper/floodfill_mapper_grayscale` | `x_floodfill_mapper_grayscale` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584759410` | `sbs:lib:floodfill_to_index/floodfill_to_index` | `flood_fill_to_index` | Present | Texture | Own renderer implementation. |
| `1584760317` | `sbs:lib:fxaa/fxaa_color` | `fxaa` | Present | Texture | Own renderer implementation. |
| `1584760319` | `sbs:lib:fxaa/fxaa_grayscale` | `fxaa` | Present | Texture | Own renderer implementation. |
| `1584760342` | `sbs:lib:glow/glow` | `glow` | Present | Texture | the glow's size and strength are converted approximately |
| `1584760349` | `sbs:lib:glow/glow_grayscale` | `glow` | Present | Texture | the glow's size and strength are converted approximately |
| `1584761889` | `sbs:lib:hbao_2/hbao` | `ao` | Present | Texture | Own renderer implementation. |
| `1584762115` | `sbs:lib:mosaic/mosaic` | `mosaic` | Present | Texture | Own renderer implementation. |
| `1584762340` | `sbs:lib:mosaic/mosaic_grayscale` | `mosaic` | Present | Texture | Own renderer implementation. |
| `1584762458` | `sbs:lib:multi_directional_warp/multi_directional_warp_color` | `multi_directional_warp` | Present | Texture | Own renderer implementation. |
| `1584762575` | `sbs:lib:multi_directional_warp/multi_directional_warp_grayscale` | `multi_directional_warp` | Present | Texture | Own renderer implementation. |
| `1584763581` | `sbs:lib:non_uniform_directional_warp/non_uniform_directional_warp_color` | `non_uniform_directional_warp` | Present | Texture | Own renderer implementation. |
| `1584764492` | `sbs:lib:non_uniform_directional_warp/non_uniform_directional_warp_grayscale` | `non_uniform_directional_warp` | Present | Texture | Own renderer implementation. |
| `1584764517` | `sbs:lib:reaction_diffusion/reaction_diffusion` | `reaction_diffusion` | Present | Texture | Own renderer implementation. |
| `1584765350` | `sbs:lib:rt_ao_v2/rt_ao` | `ao` | Present | Texture | Own renderer implementation. |
| `1584773929` | `sbs:lib:rt_irradiance/rt_irradiance` | `x_rt_irradiance` (was a placeholder) | Present | Texture | irradiance is estimated from the height, emissive and an analytic sky |
| `1584774678` | `sbs:lib:rt_shadows_v2/rt_shadows` | `x_rt_shadows` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584774692` | `sbs:lib:shadows/shadows` | `x_shadows` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584774745` | `sbs:lib:shape_shadow/shape_drop_shadow_color` | `shape_drop_shadow` | Present | Texture | Own renderer implementation. |
| `1584774755` | `sbs:lib:shape_shadow/shape_drop_shadow_grayscale` | `shape_drop_shadow` | Present | Texture | Own renderer implementation. |
| `1584774841` | `sbs:lib:shape_glow/shape_glow_color` | `shape_glow` | Present | Texture | Own renderer implementation. |
| `1584774851` | `sbs:lib:shape_glow/shape_glow_grayscale` | `shape_glow` | Present | Texture | Own renderer implementation. |
| `1584774923` | `sbs:lib:shape_stroke/shape_stroke_color` | `shape_stroke` | Present | Texture | Own renderer implementation. |
| `1584774933` | `sbs:lib:shape_stroke/shape_stroke_grayscale` | `shape_stroke` | Present | Texture | Own renderer implementation. |
| `1584775017` | `sbs:lib:summed_area_table/summed_area_table` | `summed_area_table` | Present | Texture | Own renderer implementation. |
| `1584775168` | `sbs:lib:swirl/swirl` | `swirl` | Present | Texture | Own renderer implementation. |
| `1584775283` | `sbs:lib:swirl/swirl_grayscale` | `swirl` | Present | Texture | Own renderer implementation. |
| `1584775384` | `sbs:lib:uber_emboss/uber_emboss` | `uber_emboss` | Present | Texture | Own renderer implementation. |
| `1584775522` | `sbs:lib:vector_morph/vector_morph` | `vector_morph` | Present | Texture | Own renderer implementation. |
| `1584775658` | `sbs:lib:vector_morph/vector_morph_grayscale` | `vector_morph` | Present | Texture | Own renderer implementation. |
| `1584775696` | `sbs:lib:vector_warp/vector_warp` | `vector_warp` | Present | Texture | Own renderer implementation. |
| `1584775734` | `sbs:lib:vector_warp/vector_warp_grayscale` | `vector_warp` | Present | Texture | Own renderer implementation. |
| `1584775766` | `sbs:lib:facing_normal/facing_normal` | `normal_facing` | Present | Texture | Own renderer implementation. |
| `1584775834` | `sbs:lib:height_normal_blend/height_normal_blend` | `height_normal_blend` | Present | Texture | Own renderer implementation. |
| `1584775945` | `sbs:lib:height_to_normal_world_units/height_to_normal_world_units_2` | `height_to_normal_world` | Present | Texture | Own renderer implementation. |
| `1584775971` | `sbs:lib:normal_blend/normal_blend` | `normal_blend` | Present | Texture | Own renderer implementation. |
| `1584776093` | `sbs:lib:normal_combine/normal_combine` | `normal_combine` | Present | Texture | Own renderer implementation. |
| `1584776155` | `sbs:lib:normal_intensity/normal_intensity` | `x_normal_intensity` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584776157` | `sbs:lib:normal_invert/normal_invert` | `normal_invert` | Present | Texture | Own renderer implementation. |
| `1584776159` | `sbs:lib:normal_normalise/normal_normalise` | `normal_normalize` | Present | Texture | Own renderer implementation. |
| `1584776271` | `sbs:lib:normal_sobel/normal_sobel_2` | `normal_sobel` | Present | Texture | Own renderer implementation. |
| `1584777044` | `sbs:lib:normal_to_height_2/normal_to_height` | `normal_to_height` | Present | Texture | Own renderer implementation. |
| `1584777678` | `sbs:lib:normal_to_height_hq/normal_to_height_hq` | `normal_to_height` | Present | Texture | Own renderer implementation. |
| `1584777758` | `sbs:lib:normal_transform/normal_transform` | `normal_transform` | Present | Texture | Own renderer implementation. |
| `1584778294` | `sbs:lib:normal_uncombine/normal_uncombine` | `x_normal_uncombine` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584778353` | `sbs:lib:normal_vector_rotation/normal_vector_rotation` | `normal_vector_rotation` | Present | Texture | Own renderer implementation. |
| `1584779200` | `sbs:lib:rt_bent_normal_v2/rt_bent_normal` | `bent_normal` | Present | Texture | Own renderer implementation. |
| `1584779386` | `sbs:lib:make_it_tile_patch/make_it_tile_patch` | `make_it_tile_patch` | Present | Texture | Own renderer implementation. |
| `1584779555` | `sbs:lib:make_it_tile_patch_grayscale/make_it_tile_patch_grayscale` | `make_it_tile_patch` | Present | Texture | Own renderer implementation. |
| `1584779687` | `sbs:lib:make_it_tile_photo/make_it_tile_photo` | `make_it_tile_photo` | Present | Texture | Own renderer implementation. |
| `1584779749` | `sbs:lib:make_it_tile_photo_grayscale/make_it_tile_photo_grayscale` | `make_it_tile_photo` | Present | Texture | Own renderer implementation. |
| `1584779828` | `sbs:lib:3d_texture_transform/3d_texture_offset_color` | `x_3d_texture_offset_color` (was a placeholder) | Present | Texture | 3D textures are slice atlases in MaterialCraft's own layout (grid of slices, first slice top left) |
| `1584779882` | `sbs:lib:3d_texture_transform/3d_texture_offset_grayscale` | `x_3d_texture_offset_grayscale` (was a placeholder) | Present | Texture | 3D textures are slice atlases in MaterialCraft's own layout (grid of slices, first slice top left) |
| `1584780032` | `sbs:lib:auto_crop/auto_crop_color` | `auto_crop` | Present | Texture | Own renderer implementation. |
| `1584780115` | `sbs:lib:auto_crop/auto_crop_grayscale` | `auto_crop` | Present | Texture | Own renderer implementation. |
| `1584780162` | `sbs:lib:cartesian_polar_transformation/cartesian_to_polar` | `polar_convert` | Present | Texture | Own renderer implementation. |
| `1584780170` | `sbs:lib:cartesian_polar_transformation/cartesian_to_polar_grayscale` | `polar_convert` | Present | Texture | Own renderer implementation. |
| `1584780179` | `sbs:lib:clone/clone` | `x_clone` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584780272` | `sbs:lib:mirror/mirror` | `mirror` | Present | Texture | Own renderer implementation. |
| `1584780364` | `sbs:lib:mirror/mirror_grayscale` | `mirror` | Present | Texture | Own renderer implementation. |
| `1584780377` | `sbs:lib:noise_upscale_1/noise_upscale_1` | `noise_upscale` | Present | Texture | Own renderer implementation. |
| `1584780394` | `sbs:lib:noise_upscale_2/noise_upscale_2` | `noise_upscale` | Present | Texture | Own renderer implementation. |
| `1584780396` | `sbs:lib:noise_upscale_3/noise_upscale_3` | `noise_upscale` | Present | Texture | Own renderer implementation. |
| `1584780499` | `sbs:lib:non_square_transform/non_square_transform` | `x_non_square_transform` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584780600` | `sbs:lib:non_square_transform/non_square_transform_greyscale` | `x_non_square_transform_greyscale` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584780678` | `sbs:lib:non_uniform_rotation/non_uniform_rotation_color` | `x_non_uniform_rotation_color` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584780755` | `sbs:lib:non_uniform_rotation/non_uniform_rotation_grayscale` | `x_non_uniform_rotation_grayscale` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584780761` | `sbs:lib:cartesian_polar_transformation/polar_to_cartesian` | `polar_convert` | Present | Texture | Own renderer implementation. |
| `1584780767` | `sbs:lib:cartesian_polar_transformation/polar_to_cartesian_grayscale` | `polar_convert` | Present | Texture | Own renderer implementation. |
| `1584781021` | `sbs:lib:quad_transform/quad_transform` | `quad_transform` | Present | Texture | Own renderer implementation. |
| `1584781136` | `sbs:lib:quad_transform/quad_transform_grayscale` | `quad_transform` | Present | Texture | Own renderer implementation. |
| `1584781227` | `sbs:lib:safe_transform/safe_transform` | `safe_transform` | Present | Texture | Own renderer implementation. |
| `1584781229` | `sbs:lib:safe_transform/safe_transform_grayscale` | `safe_transform` | Present | Texture | Own renderer implementation. |
| `1584781278` | `sbs:lib:skew/skew` | `skew` | Present | Texture | Own renderer implementation. |
| `1584781326` | `sbs:lib:skew/skew_grayscale` | `skew` | Present | Texture | Own renderer implementation. |
| `1584781414` | `sbs:lib:symmetry/symmetry` | `symmetry` | Present | Texture | Own renderer implementation. |
| `1584781536` | `sbs:lib:symmetry_slice/symmetry_slice` | `symmetry_slice` | Present | Texture | Own renderer implementation. |
| `1584781579` | `sbs:lib:trapezoid_transform/trapezoid_transform` | `trapezoid_transform` | Present | Texture | Own renderer implementation. |
| `1584781621` | `sbs:lib:trapezoid_transform/trapezoid_transform_grayscale` | `trapezoid_transform` | Present | Texture | Own renderer implementation. |
| `1584784186` | `sbs:lib:bitmaptomaterial_2_light/Bitmap2Material_2_Light` | `x_bitmap2material_2_light` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584784273` | `sbs:lib:height_blend/height_blend` | `height_blend` | Present | Texture | Own renderer implementation. |
| `1584784367` | `sbs:lib:material_height_blend/material_height_blend` | `x_material_height_blend` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584784735` | `sbs:lib:me_snow_cover/snow_cover` | `x_snow_cover` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584785554` | `sbs:lib:me_water_level/water_level` | `x_water_level` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584785611` | `sbs:lib:material_transform/material_transform` | `x_material_transform` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584785791` | `sbs:lib:material_adjustment_blend/material_adjustment_blend` | `x_material_adjustment_blend` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584785912` | `sbs:lib:material_blend/material_blend` | `material_blend` | Present | Texture | Own renderer implementation. |
| `1584786041` | `sbs:lib:material_color_blend/material_color_blend` | `x_material_color_blend` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584786148` | `sbs:lib:material_switch/material_switch` | `x_material_switch` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584786933` | `sbs:lib:multi_material_blend/multi_material_blend` | `x_multi_material_blend` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584787058` | `sbs:lib:pbr_converter/basecolor_metallic_roughness_converter` | `x_basecolor_metallic_roughness_converter` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584787096` | `sbs:lib:pbr_albedo_safe_color/pbr_albedo_safe_color` | `x_pbr_albedo_safe_color` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584787453` | `sbs:lib:pbr_base_material/pbr_base_material` | `x_pbr_base_material` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584787540` | `sbs:lib:pbr_basecolor_metallic_validate/pbr_basecolor_metallic_validate_2` | `x_pbr_basecolor_metallic_validate_2` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584787624` | `sbs:lib:pbr_dielectric_F0/pbr_dielectric_F0` | `x_pbr_dielectric_f0` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584787673` | `sbs:lib:pbr_metal_reflectance/pbr_metal_reflectance` | `x_pbr_metal_reflectance` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584796648` | `sbs:lib:pbr_render2/PBR_render` | `x_pbr_render` (was a placeholder) | Present | Texture | the render is MaterialCraft's own image-based shading of the material on the chosen shape |
| `1584796667` | `sbs:lib:pbr_render2/PBR_render_mapping_color` | `x_pbr_render_mapping_color` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584796684` | `sbs:lib:pbr_render2/PBR_render_mapping_grayscale` | `x_pbr_render_mapping_grayscale` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584796821` | `sbs:lib:ao_cancel/ao_cancel` | `x_ao_cancel` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584799214` | `sbs:lib:atlas_scatter/atlas_scatter` | `x_atlas_scatter` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584799684` | `sbs:lib:atlas_spliter/atlas_spliter` | `x_atlas_spliter` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584801517` | `sbs:lib:clone_patch/clone_patch` | `clone_patch` | Present | Texture | Own renderer implementation. |
| `1584801805` | `sbs:lib:clone_patch/clone_patch_grayscale` | `clone_patch` | Present | Texture | Own renderer implementation. |
| `1584802007` | `sbs:lib:color_equalizer/color_equalizer` | `x_color_equalizer` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584802178` | `sbs:lib:crop/crop` | `x_crop` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584802301` | `sbs:lib:crop/crop_grayscale` | `x_crop_grayscale` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584802714` | `sbs:lib:clone_patch/material_clone_patch` | `x_material_clone_patch` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584802772` | `sbs:lib:crop/material_crop` | `x_material_crop` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584802806` | `sbs:lib:clone_patch/multi_clone_patch` | `x_multi_clone_patch` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584802833` | `sbs:lib:clone_patch/multi_clone_patch_grayscale` | `x_multi_clone_patch_grayscale` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584802867` | `sbs:lib:color_equalizer/multi_color_equalizer` | `x_multi_color_equalizer` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584802893` | `sbs:lib:crop/multi_crop` | `x_multi_crop` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584802920` | `sbs:lib:crop/multi_crop_grayscale` | `x_multi_crop_grayscale` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584803173` | `sbs:lib:multiangle_to_albedo/multiangle_to_albedo` | `x_multiangle_to_albedo` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584803920` | `sbs:lib:multiangle_to_normal/multiangle_to_normal` | `x_multiangle_to_normal` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584804064` | `sbs:lib:smart_auto_tile/smart_auto_tile` | `x_smart_auto_tile` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584804124` | `sbs:lib:mg_bottom_to_top/mg_bottom_to_top` | `x_mg_bottom_to_top` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584804143` | `sbs:lib:mg_cloth_wear/mg_cloth_wear` | `x_mg_cloth_wear` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584805295` | `sbs:lib:mg_dirt/mg_dirt` | `x_mg_dirt` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584805525` | `sbs:lib:mg_dripping_rust/mg_dripping_rust` | `x_mg_dripping_rust` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584806798` | `sbs:lib:mg_dust/mg_dust` | `x_mg_dust` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584806811` | `sbs:lib:mg_edge_blur/mg_edge_blur` | `x_mg_edge_blur` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584806952` | `sbs:lib:mg_edge_damages/mg_edge_damages` | `x_mg_edge_damages` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584807037` | `sbs:lib:mg_edge_dirt/mg_edge_dirt` | `x_mg_edge_dirt` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584807214` | `sbs:lib:mg_edge_notch/mg_edge_notch` | `x_mg_edge_notch` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584807237` | `sbs:lib:mg_edge_select/mg_edge_select` | `x_mg_edge_select` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584807261` | `sbs:lib:mg_edge_speckle/mg_edge_speckle` | `x_mg_edge_speckle` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584807336` | `sbs:lib:mg_edge_wear/mg_edge_wear` | `x_mg_edge_wear` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584807384` | `sbs:lib:mg_fiber_glass_edge_wear/mg_fiber_glass_edge_wear` | `x_mg_fiber_glass_edge_wear` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584807962` | `sbs:lib:mg_grease/mg_grease` | `x_mg_grease` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584808041` | `sbs:lib:mg_ground_dirt/mg_ground_dirt` | `x_mg_ground_dirt` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584808089` | `sbs:lib:mg_leaks/mg_leaks` | `x_mg_leaks` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584808118` | `sbs:lib:mg_leather_wear/mg_leather_wear` | `x_mg_leather_wear` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584808202` | `sbs:lib:mg_light/mg_light` | `x_mg_light` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584808863` | `sbs:lib:mg_mask_builder/mg_mask_builder_2` | `x_mg_mask_builder_2` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584808913` | `sbs:lib:mg_metal_edge_wear/mg_metal_edge_wear` | `x_mg_metal_edge_wear` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584808940` | `sbs:lib:mg_paint_wear/mg_paint_wear` | `x_mg_paint_wear` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584809008` | `sbs:lib:mg_selective_dirt/mg_selective_dirt` | `x_mg_selective_dirt` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584809026` | `sbs:lib:mg_sun_bleach/mg_sun_bleach` | `x_mg_sun_bleach` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584809068` | `sbs:lib:mg_surface_brush/mg_surface_brush` | `x_mg_surface_brush` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584809651` | `sbs:lib:cracks_weathering_material/cracks_weathering` | `x_cracks_weathering` (was a placeholder) | Present | Texture | the weathering is MaterialCraft's own (masks from curvature, occlusion and noise) |
| `1584810302` | `sbs:lib:fabric_weathering_material/fabric_weathering` | `x_fabric_weathering` (was a placeholder) | Present | Texture | the weathering is MaterialCraft's own (masks from curvature, occlusion and noise) |
| `1584810677` | `sbs:lib:leather_weathering_material/leather_weathering` | `x_leather_weathering` (was a placeholder) | Present | Texture | the weathering is MaterialCraft's own (masks from curvature, occlusion and noise) |
| `1584811994` | `sbs:lib:moss_weathering_material/moss_weathering` | `x_moss_weathering` (was a placeholder) | Present | Texture | the weathering is MaterialCraft's own (masks from curvature, occlusion and noise) |
| `1584812347` | `sbs:lib:painted_metal_weathering_material/painted_metal_weathering` | `x_painted_metal_weathering` (was a placeholder) | Present | Texture | the weathering is MaterialCraft's own (masks from curvature, occlusion and noise) |
| `1584812755` | `sbs:lib:rock_weathering_material/rock_weathering` | `x_rock_weathering` (was a placeholder) | Present | Texture | the weathering is MaterialCraft's own (masks from curvature, occlusion and noise) |
| `1584812976` | `sbs:lib:rust_weathering_material/rust_weathering` | `x_rust_weathering` (was a placeholder) | Present | Texture | the weathering is MaterialCraft's own (masks from curvature, occlusion and noise) |
| `1584813989` | `sbs:lib:3d_planar_projection/3d_planar_projection` | `x_3d_planar_projection` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584814192` | `sbs:lib:3d_planar_projection/3d_planar_projection_color` | `x_3d_planar_projection_color` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584814314` | `sbs:lib:material_mesh_data_blender/material_mesh_data_blender` | `x_material_mesh_data_blender` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584814437` | `sbs:lib:material_selector/material_selector` | `x_material_selector` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584814451` | `sbs:lib:mesh_data_combiner/mesh_data_combiner` | `x_mesh_data_combiner` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584814910` | `sbs:lib:triplanar/triplanar_3` | `x_triplanar_3` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584815344` | `sbs:lib:triplanar/triplanar_grayscale_3` | `x_triplanar_grayscale_3` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584815812` | `sbs:lib:paths_tools/path_to_spline` | `x_path_to_spline` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584818731` | `sbs:lib:spline_tools/point_list` | `x_point_list` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584819739` | `sbs:lib:spline_tools/scatter_on_spline_color` | `x_scatter_on_spline_color` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584820440` | `sbs:lib:spline_tools/scatter_on_spline_grayscale` | `x_scatter_on_spline_grayscale` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584821455` | `sbs:lib:spline_tools/scatter_splines_on_splines` | `x_scatter_splines_on_splines` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584821547` | `sbs:lib:spline_tools/spline_2dtransform` | `x_spline_2dtransform` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584821776` | `sbs:lib:spline_tools/spline_append` | `x_spline_append` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584822105` | `sbs:lib:spline_tools/spline_bridge_2_splines` | `x_spline_bridge_2_splines` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584822922` | `sbs:lib:spline_tools/spline_bridge_list_v2` | `x_spline_bridge_list_v2` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584823413` | `sbs:lib:spline_tools/spline_bridge_mapper_color` | `x_spline_bridge_mapper_color` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584823451` | `sbs:lib:spline_tools/spline_bridge_mapper_grayscale` | `x_spline_bridge_mapper_grayscale` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584823997` | `sbs:lib:spline_tools/spline_circle` | `x_spline_circle` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584824472` | `sbs:lib:spline_tools/spline_cubic` | `x_spline_cubic` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584824654` | `sbs:lib:spline_tools/spline_fill` | `x_spline_fill` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584825300` | `sbs:lib:spline_tools/spline_flow_mapper` | `x_spline_flow_mapper` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584825783` | `sbs:lib:spline_tools/spline_mapper_color` | `x_spline_mapper_color` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584825809` | `sbs:lib:spline_tools/spline_mapper_grayscale` | `x_spline_mapper_grayscale` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584826243` | `sbs:lib:spline_tools/spline_merge_list` | `x_spline_merge_list` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584826940` | `sbs:lib:spline_tools/spline_poly_quadratic` | `x_spline_poly_quadratic` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584827367` | `sbs:lib:spline_tools/spline_quadratic` | `x_spline_quadratic` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584827372` | `sbs:lib:spline_tools/spline_render` | `x_spline_render` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584827453` | `sbs:lib:spline_tools/spline_sample_height` | `x_spline_sample_height` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584827473` | `sbs:lib:spline_tools/spline_sample_thickness` | `x_spline_sample_thickness` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584827603` | `sbs:lib:spline_tools/spline_select` | `x_spline_select` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584827728` | `sbs:lib:spline_tools/spline_warp` | `x_spline_warp` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584827733` | `sbs:lib:uv_mapper/uv_mapper_color` | `x_uv_mapper_color` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584827735` | `sbs:lib:uv_mapper/uv_mapper_grayscale` | `x_uv_mapper_grayscale` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584827819` | `sbs:lib:paths_tools/2dtransform_path` | `x_2dtransform_path` (was a placeholder) | Present | Texture | path data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' path images |
| `1584827821` | `sbs:lib:paths_tools/mask_to_paths_2` | `x_mask_to_paths_2` (was a placeholder) | Present | Texture | path data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' path images |
| `1584827824` | `sbs:lib:paths_tools/path_to_spline` | `x_path_to_spline` (was a placeholder) | Present | Texture | spline data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' spline images |
| `1584828003` | `sbs:lib:paths_tools/paths_polygon` | `x_paths_polygon` (was a placeholder) | Present | Texture | path data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' path images |
| `1584828005` | `sbs:lib:paths_tools/paths_trace` | `x_paths_trace` (was a placeholder) | Present | Texture | path data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' path images |
| `1584828155` | `sbs:lib:paths_tools/paths_warp` | `x_paths_warp` (was a placeholder) | Present | Texture | path data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' path images |
| `1584828170` | `sbs:lib:paths_tools/quad_transform_path` | `x_quad_transform_path` (was a placeholder) | Present | Texture | path data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' path images |
| `1584828216` | `sbs:lib:paths_tools/select_path` | `x_select_path` (was a placeholder) | Present | Texture | path data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' path images |
| `1584828350` | `sbs:lib:paths_tools/vertex_processor` | `x_vertex_processor` (was a placeholder) | Present | Texture | path data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' path images |
| `1584828352` | `sbs:lib:paths_tools/vertex_processor_simple` | `x_vertex_processor_simple` (was a placeholder) | Present | Texture | path data travels in MaterialCraft's own encoding: these nodes work together, not with other programs' path images |
| `1584828559` | `sbs:lib:blackbody/blackbody` | `x_blackbody` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584828622` | `sbs:lib:color_temperature_adjustment/color_temperature_adjustment` | `x_color_temperature_adjustment` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584828635` | `sbs:lib:exposure/exposure` | `x_exposure` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584828682` | `sbs:lib:hdr_exposure_preview/hdr_exposure_preview` | `x_hdr_exposure_preview` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584829257` | `sbs:lib:hdr_merge/hdr_merge_2` | `x_hdr_merge_2` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584829292` | `sbs:lib:panorama_nadir_extract/nadir_extract` | `x_nadir_extract` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584829310` | `sbs:lib:panorama_position/panorama_3d_position` | `x_panorama_3d_position` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584829381` | `sbs:lib:panorama_gradients/panorama_gradient_2_points` | `x_panorama_gradient_2_points` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584829404` | `sbs:lib:panorama_gradients/panorama_gradient_linear` | `x_panorama_gradient_linear` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584830014` | `sbs:lib:panorama_shape/panorama_line_light` | `x_panorama_line_light` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584830391` | `sbs:lib:panorama_nadir_patch/panorama_nadir_patch` | `x_panorama_nadir_patch` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584830983` | `sbs:lib:panorama_physical_sun_sky/panorama_physical_sun_sky` | `x_panorama_physical_sun_sky` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584831441` | `sbs:lib:panorama_shape/panorama_plane_light` | `x_panorama_plane_light` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584831549` | `sbs:lib:panorama_rotation/panorama_rotation` | `x_panorama_rotation` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584831847` | `sbs:lib:panorama_shape/panorama_shape_2` | `x_panorama_shape_2` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584832052` | `sbs:lib:panorama_shape/panorama_sphere_light` | `x_panorama_sphere_light` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584832250` | `sbs:lib:panorama_straighten_horizon/panorama_straighten_horizon` | `x_panorama_straighten_horizon` (was a placeholder) | Present | Texture | Native implementation of the library graph. |
| `1584833851` | `sbs:lib:bricks_001/bricks_001` | `x_bricks_001` | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584833861` | `sbs:lib:bricks_005/bricks_005` | `x_bricks_005` | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584834378` | `sbs:lib:cardboard_001/cardboard_001` | `x_cardboard_001` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584835031` | `sbs:lib:concrete_002/concrete_002` | `x_concrete_002` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584835321` | `sbs:lib:concrete_006/concrete_006` | `x_concrete_006` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584835647` | `sbs:lib:concrete_010/concrete_010` | `x_concrete_010` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584835948` | `sbs:lib:concrete_011/concrete_011` | `x_concrete_011` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584836158` | `sbs:lib:concrete_044/concrete_044` | `x_concrete_044` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584836287` | `sbs:lib:concrete_070/concrete_070` | `x_concrete_070` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584836996` | `sbs:lib:fabric_002/fabric_002` | `x_fabric_002` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584837273` | `sbs:lib:fabric_009/fabric_009` | `x_fabric_009` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584838663` | `sbs:lib:fabric_025/fabric_025` | `x_fabric_025` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584839008` | `sbs:lib:gravel/gravel` | `x_gravel` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584839491` | `sbs:lib:metal_002/metal_002` | `x_metal_002` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584840061` | `sbs:lib:metal_003/metal_003` | `x_metal_003` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584840263` | `sbs:lib:metal_006/metal_006` | `x_metal_006` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584840449` | `sbs:lib:metal_plate_001/metal_plate_001` | `x_metal_plate_001` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584840983` | `sbs:lib:metal_plate_004/metal_plate_004` | `x_metal_plate_004` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584841298` | `sbs:lib:old_painted_planks/old_painted_planks` | `x_old_painted_planks` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584841700` | `sbs:lib:paper_005/paper_005` | `x_paper_005` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584841869` | `sbs:lib:rough_concrete_with_lines/rough_concrete_with_lines` | `x_rough_concrete_with_lines` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584842120` | `sbs:lib:tiles_002/tiles_002` | `x_tiles_002` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584842397` | `sbs:lib:tiles_008/tiles_008` | `x_tiles_008` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584842671` | `sbs:lib:tiles_011/tiles_011` | `x_tiles_011` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584842975` | `sbs:lib:tiles_025/tiles_025` | `x_tiles_025` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |
| `1584843264` | `sbs:lib:wood_american_cherry/wood_american_cherry` | `x_wood_american_cherry` (was a placeholder) | Present | Texture | the material is MaterialCraft's own procedural version of this preset, not the library's |

## Exact mapping classification (61)

| Node UID | Native kind | Renderer kind | Definition | 32×32 result | Limitations / errors |
| --- | --- | --- | --- | --- | --- |
| `1213284337` | `sbs:output` | `output` | Present | Texture | No behavior warning reported. |
| `1213284339` | `sbs:output` | `output` | Present | Texture | No behavior warning reported. |
| `1213284341` | `sbs:output` | `output` | Present | Texture | No behavior warning reported. |
| `1213284343` | `sbs:output` | `output` | Present | Texture | No behavior warning reported. |
| `1279137030` | `sbs:output` | `output` | Present | Texture | No behavior warning reported. |
| `1359211355` | `sbs:uniform` | `color` | Present | Texture | No behavior warning reported. |
| `1359211391` | `sbs:uniform` | `color` | Present | Texture | No behavior warning reported. |
| `1359211407` | `sbs:uniform` | `color` | Present | Texture | No behavior warning reported. |
| `1359211415` | `sbs:uniform` | `color` | Present | Texture | No behavior warning reported. |
| `1359211719` | `sbs:uniform` | `color` | Present | Texture | No behavior warning reported. |
| `1359211720` | `sbs:output` | `output` | Present | Texture | No behavior warning reported. |
| `1584632881` | `sbs:blend` | `blend` | Present | Texture | No behavior warning reported. |
| `1584632893` | `sbs:shuffle` | `channel_shuffle` | Present | Texture | No behavior warning reported. |
| `1584632919` | `sbs:curve` | `curve` | Present | Texture | No behavior warning reported. |
| `1584632960` | `sbs:dyngradient` | `lookup_table` | Present | Texture | No behavior warning reported. |
| `1584632967` | `sbs:gradient` | `colorize` | Present | Texture | No behavior warning reported. |
| `1584632973` | `sbs:grayscaleconversion` | `grayscale_conversion` | Present | Texture | No behavior warning reported. |
| `1584632988` | `sbs:input` | `input` | Present | Texture | No behavior warning reported. |
| `1584633001` | `sbs:bitmap` | `bitmap` | Present | Error | No behavior warning reported.<br>Render error: node 1584633001: cannot read '../AppData/Local/Temp/Adobe/Adobe Substance 3D Designer/resources/XXXXXX-Untitled_Bitmap.png': file not found |
| `1584633018` | `sbs:input` | `input` | Present | Texture | No behavior warning reported. |
| `1584633027` | `sbs:input` | `input` | Present | Texture | No behavior warning reported. |
| `1584633036` | `sbs:levels` | `levels` | Present | Texture | No behavior warning reported. |
| `1584633048` | `sbs:output` | `output` | Present | Texture | No behavior warning reported. |
| `1584633077` | `sbs:text` | `text` | Present | Texture | No behavior warning reported. |
| `1584633084` | `sbs:transformation` | `transform` | Present | Texture | No behavior warning reported. |
| `1584633093` | `sbs:uniform` | `color` | Present | Texture | No behavior warning reported. |
| `1584832322` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832324` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832326` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832328` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832330` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832332` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832334` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832336` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832338` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832340` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832342` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832344` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832346` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832348` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832350` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832352` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832354` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832356` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832358` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832360` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832362` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832364` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832366` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832368` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832370` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832372` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832374` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832376` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832378` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832380` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832382` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832384` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832386` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584832388` | `sbs:bitmap` | `bitmap` | Present | Texture | No behavior warning reported. |
| `1584843868` | `sbs:blend` | `blend` | Present | Texture | the output size function has no result node; the default size is used, as in the file's own editor |
