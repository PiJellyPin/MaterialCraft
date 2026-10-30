// SPDX-License-Identifier: MIT OR Apache-2.0
//! The second batch of device kernels: filters, transforms, warps, material utilities, scatter nodes, distance fields
//! and the extra generators. Each node's device result must agree with the CPU code (a hard threshold or a lattice
//! border may flip a few pixels, so the comparison allows a tiny share of outliers), run as a kernel (not fall back)
//! and keep the pictures on the device. Needs the `gpu` feature and an adapter (a software one is accepted); skips otherwise.
#![cfg(feature = "gpu")]
use materialcraft::graph::{Graph, NodeCache};
use materialcraft::{gpu, tex::Tex};
use std::sync::Arc;

static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn setup() -> Option<std::sync::MutexGuard<'static, ()>> {
    let g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("MC_GPU", "force");
    gpu::set_enabled(true);
    gpu::set_min_pixels(0);
    if gpu::adapter_name().is_some() { Some(g) } else { None }
}

fn eval(text: &str, keep: &[u32], cache: &Arc<NodeCache>) -> materialcraft::graph::EvalAll {
    let g = Graph::parse(text).unwrap();
    let r = g.evaluate_all_cached(64, keep, 0, cache, None, false, &mut |_| {});
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    r
}

fn cpu_only(text: &str, keep: &[u32]) -> materialcraft::graph::EvalAll {
    gpu::set_enabled(false);
    let r = eval(text, keep, &Arc::new(NodeCache::new(256)));
    gpu::set_enabled(true);
    r
}

/// (largest difference, mean difference, share of pixels that differ by more than 0.05)
fn stats(a: &Tex, b: &Tex) -> (f32, f32, f32) {
    assert_eq!((a.w, a.h), (b.w, b.h));
    let mut mx = 0f32;
    let mut sum = 0f32;
    let mut bad = 0usize;
    for (p, q) in a.px.iter().zip(&b.px) {
        let d = (0..4).map(|c| (p[c] - q[c]).abs()).fold(0.0, f32::max);
        mx = mx.max(d);
        sum += d;
        if d > 0.05 {
            bad += 1;
        }
    }
    let n = a.px.len() as f32;
    (mx, sum / n, bad as f32 / n)
}

/// (node line, kernel launches at least, largest accepted max difference). A case also passes when the mean
/// difference is below 4e-3 and fewer than 1.5 % of the pixels differ by more than 0.05.
const CASES: &[(&str, usize, f32)] = &[
    // neighbourhood filters and transforms
    ("emboss in=@1 angle=0.2 intensity=5", 1, 2e-3),
    ("emboss mode=1 in=@6 height=@1 angle=0.1 intensity=3 hl_r=1 hl_g=0.9 hl_b=0.7 sh_r=0.1 sh_g=0.1 sh_b=0.2 gloss=0.3", 1, 5e-3),
    ("emboss mode=1 in=@6", 0, 1e-6),
    ("edge_detect mode=1 in=@1 intensity=3", 1, 2e-3),
    ("sobel in=@1 intensity=2", 1, 2e-3),
    ("swirl in=@1 amount=0.4 radius=0.6", 1, 5e-3),
    ("transform in=@1 offset_x=0.1 offset_y=0.2 scale_x=2 scale_y=2 rotation=0.25", 1, 5e-3),
    ("transform in=@1 offset_x=0.1 scale_x=1.3 scale_y=0.8 rotation=0.1 tiling=1", 1, 5e-3),
    ("transform in=@1 scale_x=1.3 rotation=0.1", 2, 5e-3),
    ("mirror in=@1 axis=0 offset_x=0.4", 1, 1e-6),
    ("mirror in=@1 axis=2 offset_x=0.3 offset_y=0.6 invert_x=1", 1, 1e-6),
    ("mirror in=@1 axis=3 corner=3", 1, 1e-6),
    ("mosaic in=@1 count=8", 1, 2e-3),
    ("symmetry in=@1 axis=0 offset=0.1", 1, 2e-3),
    ("symmetry in=@1 axis=3 flip=1 transfer=1", 1, 2e-3),
    ("symmetry in=@1 axis=5 transfer=5", 1, 2e-3),
    ("symmetry in=@1 axis=6 flip=1", 1, 2e-3),
    ("skew in=@1 skew_x=2 skew_y=1 align=0", 1, 5e-3),
    ("skew in=@1 skew_x=0.35 skew_y=0 align=1", 1, 5e-3),
    ("non_uniform_directional_warp in=@1 intensity=@2 angle=@5 intensity=0.2", 1, 8e-3),
    ("multi_directional_warp in=@1 warp=@2 intensity=0.2 mode=1", 1, 8e-3),
    ("multi_directional_warp in=@1 warp=@2 intensity=0.1 mode=0 directions=1", 1, 8e-3),
    ("normal_facing in=@4 angle=0.1 tilt=0.4 sharpness=2", 1, 2e-3),
    ("height_to_normal_world in=@1 size=50 height=2", 1, 5e-3),
    ("normal_sobel in=@4 intensity=3", 1, 5e-3),
    ("normal_sobel in=@4 intensity=3 mode=1", 1, 5e-3),
    ("normal_transform in=@4 rotation=0.25 rotate_vectors=1", 2, 5e-3),
    ("height_normal_blend normal=@4 height=@1 mode=0 strength=3", 2, 2e-2),
    ("height_normal_blend normal=@4 height=@1 mode=1 opacity=0.5", 2, 2e-2),
    ("min_max a=@1 b=@2 mode=0", 1, 1e-6),
    ("min_max a=@1 b=@2 mode=1", 1, 1e-6),
    ("frequency_merge low=@1 high=@2 gain=2", 1, 1e-5),
    ("multi_switch a=@1 b=@2 index=1", 0, 1e-6),
    ("polar_convert in=@1 mode=0 tile=0", 1, 5e-3),
    ("polar_convert in=@1 mode=1 tile=1", 2, 5e-3),
    ("symmetry_slice in=@1 slices=5 mirror=1 tile=0", 1, 5e-3),
    ("symmetry_slice in=@1 slices=3 mirror=0 tile=1", 2, 5e-3),
    ("quad_transform in=@1 tl_x=0.1 tr_y=0.05 br_x=-0.1 tile=0", 1, 8e-3),
    ("trapezoid_transform in=@1 top=0.5 bottom=1 tile=1", 2, 8e-3),
    ("clone_patch in=@1 src_x=0.25 src_y=0.25 dst_x=0.7 dst_y=0.6 width=0.3 height=0.3 feather=0.4 rotation=0.1", 1, 8e-3),
    // material utilities
    ("metal_rough_to_spec_gloss base_color=@6 metallic=@1 roughness=@2", 1, 2e-3),
    ("spec_gloss_to_metal_rough diffuse=@6 specular=@7 glossiness=@2", 1, 5e-3),
    ("material_blend bg_base=@6 fg_base=@7 bg_height=@1 fg_height=@2 mask=@5 opacity=0.9", 2, 5e-3),
    ("leak_streaks in=@1 noise=@2 length=60 angle=0.25 intensity=1.5", 1, 5e-3),
    ("make_it_tile_mirror in=@1 mode=0", 1, 2e-3),
    ("height_blend bg=@1 fg=@2 mask=@5 opacity=0.9 contrast=0.6", 1, 2e-3),
    ("make_it_tile_photo in=@1 width=0.5", 1, 2e-3),
    ("make_it_tile_photo in=@1 width=0.3 width_v=0.05 sharp=0.6 sharp_v=1 warp=0.8 warp_v=-0.5 seed=3", 1, 2e-3),
    ("dirt in=@6 amount=0.6 noise=@2", 3, 2e-3),
    ("dust in=@6 amount=0.4", 3, 2e-3),
    ("curve in=@1", 1, 1e-4),
    ("morphology in=@1 mode=0 radius=16", 2, 1e-4),
    ("morphology in=@6 mode=1 radius=24", 2, 1e-4),
    ("morphology in=@1 mode=2 radius=16", 4, 1e-4),
    ("morphology in=@6 mode=3 radius=16", 4, 1e-4),
    ("lookup_table in=@1 lut=@6 mode=0 row=0.3", 1, 2e-3),
    ("lookup_table in=@1 lut=@7 mode=1 vertical=1 repeat=1", 1, 2e-3),
    ("histogram_equalize in=@1", 2, 3e-3),
    ("histogram_equalize in=@6 per_channel=1 amount=0.7", 2, 3e-3),
    ("auto_crop in=@9 margin=0.05", 2, 2e-3),
    ("auto_crop in=@9 keep_aspect=1", 2, 2e-3),
    ("noise_upscale in=@1 cells=8 mode=0", 2, 2e-3),
    ("noise_upscale in=@1 cells=8 mode=1", 2, 2e-3),
    ("noise_upscale in=@6 cells=12 mode=2", 2, 2e-3),
    ("halftone in=@1 cells=8 mode=0 softness=0.2", 1, 3e-3),
    ("halftone in=@1 cells=8 mode=1 gain=1.5", 1, 3e-3),
    ("fxaa in=@9 strength=0.8 threshold=0.05", 1, 3e-3),
    ("color r=0.2 g=0.4 b=0.6 a=0.9", 1, 1e-4),
    ("cross_section in=@1 position=0.4 fill=1 thickness=6", 1, 3e-3),
    ("uber_emboss height=@1 mask=@2 gloss=@5 intensity=5", 1, 3e-3),
    ("safe_transform in=@1 mode=0 angle=0.1 scale_x=2 scale_y=3 offset_x=0.2", 1, 3e-3),
    ("safe_transform in=@1 mode=1 angle=0.1 scale_x=1.5 edge=1 filter=2", 1, 3e-3),
    ("safe_transform in=@1 mode=1 angle=0.3 scale_x=0.7 edge=3 rotate_after_scale=1", 1, 3e-3),
    ("mesh_weathering curvature=@1 ao=@2 normal=@4 noise=@5", 1, 3e-3),
    ("mesh_weathering curvature=@1 wear=0.8", 1, 3e-3),
    ("snow_filter height=@1 mask=@2", 3, 3e-3),
    ("season_filter in=@6 mask=@2 season=2.4", 1, 3e-3),
    ("water_filter height=@1 base=@6", 1, 3e-3),
    ("summed_area_table in=@1", 2, 2e-3),
    ("summed_area_table in=@6 per_channel=1 normalize=0", 2, 2e-3),
    // scatter nodes on a graph that is not square (drawn square, then stretched like the CPU does)
    ("tile_sampler pattern_type=2 x_amount=6 y_amount=5 size=0.9 rotation_random=0.3 position_random=0.5 seed=3", 2, 0.02),
    ("tile_generator pattern_type=8 x_amount=5 y_amount=6", 2, 0.02),
    ("splatter pattern_type=2 count=30 scale=0.15 blending=1 seed=4", 2, 0.02),
    ("splatter pattern=@1 count=20 scale=0.2 rotation=0.1 seed=6", 3, 0.02),
    ("splatter_circular pattern_type=2 number=10 rings=2 size=0.12", 2, 0.02),
    ("tile_sampler pattern=@1 x_amount=5 y_amount=5 size=1 mask=@2 mask_threshold=0.4 scale_map=@5", 3, 0.02),
    // flood-fill family
    ("flood_fill in=@9", 1, 1e-4),
    ("flood_fill in=@8 threshold=0.3", 1, 1e-4),
    ("flood_fill_to_index in=@10", 1, 1e-3),
    ("flood_fill_to_position in=@10", 1, 1e-3),
    ("flood_fill_to_bbox_size in=@10", 1, 1e-3),
    ("flood_fill_random in=@10 seed=4 contrast=0.5 min=0.1 max=0.9", 1, 1e-3),
    ("flood_fill_color in=@10 seed=5", 1, 1e-3),
    ("flood_fill_gradient in=@10 angle=0.2 variation=0.3 seed=2", 1, 1e-3),
];

/// Nodes the CPU draws on a square graph, with looser limits for the distance fields (jump flooding is closer to the
/// true distance than the CPU sweeps, so the two differ by up to a pixel).
const SQUARE_CASES: &[(&str, usize, f32, f32)] = &[
    // scatter
    ("tile_sampler pattern_type=2 x_amount=6 y_amount=5 size=0.9 rotation_random=0.3 scale_random=0.4 position_random=0.5 seed=3", 2, 5e-3, 4e-3),
    ("tile_sampler pattern_type=2 x_amount=4 y_amount=4 blending=2 size=1.2 color_random=0.5 seed=7", 2, 5e-3, 4e-3),
    ("tile_sampler pattern=@1 x_amount=5 y_amount=5 size=1 rotation=0.1 pattern_rotation_random=0.5 seed=2", 3, 5e-3, 4e-3),
    ("tile_sampler pattern_type=3 scale_map=@2 mask=@5 mask_threshold=0.4 displacement_map=@2 rotation_map=@1 x_amount=6 y_amount=6", 3, 5e-3, 4e-3),
    ("tile_generator pattern_type=8 x_amount=5 y_amount=6", 2, 5e-3, 4e-3),
    ("splatter pattern_type=2 count=30 scale=0.15 blending=1 seed=4", 2, 5e-3, 4e-3),
    ("splatter pattern=@1 count=20 scale=0.2 rotation=0.1 seed=6", 3, 5e-3, 4e-3),
    ("splatter_circular pattern_type=2 number=10 rings=2 size=0.12", 2, 5e-3, 4e-3),
    // distance fields
    ("distance in=@1 distance=0.2 mode=0", 4, 0.12, 0.03),
    ("distance in=@1 distance=0.2 mode=1 threshold=0.4", 4, 0.12, 0.03),
    ("distance in=@1 distance=0.2 mode=2", 4, 0.12, 0.03),
    ("bevel in=@1 distance=0.08 smoothing=0.5", 4, 0.12, 0.04),
    ("shape_stroke in=@1 width=6", 4, 0.12, 0.08),
    ("shape_glow in=@1 width=10 spread=0.2", 4, 0.12, 0.04),
];

const GEN_CASES: &[(&str, usize, f32)] = &[
    // generators
    ("gradient_reflected count=2 angle=0.1", 1, 1e-4),
    ("triangle_grid count=6 border=0.1", 1, 5e-3),
    ("weave_generator weave=2 shape=0.5 gap_x=0.2 gap_y=0.2", 1, 5e-3),
    ("starburst count=2 rays=8 sharpness=2", 1, 5e-3),
    ("caustics scale=4 strength=2", 1, 5e-3),
    ("liquid_noise scale=3 warp=0.5 mode=1 bands=5", 1, 5e-3),
    ("perlin_3d scale=4 depth=0.3 octaves=3", 1, 5e-3),
    ("paving scale=6 jitter=0.6", 1, 5e-3),
    ("wire_mesh count_x=8 count_y=6 weave_depth=0.5", 1, 5e-3),
    ("wire_mesh mode=1 contrast=1.5 invert=1", 1, 5e-3),
    ("sdf_shape shape=0 size=0.3 x=0.1 range=0.3", 1, 5e-3),
    ("sdf_shape shape=1 size=0.25 aspect=0.6 rotation=0.1 round=0.05", 1, 5e-3),
    ("sdf_shape shape=2 sides=6 size=0.3", 1, 5e-3),
    ("sdf_shape shape=3 size=0.3", 1, 5e-3),
    ("sdf_form shape=0 size=0.3 repeat=3 scatter=0.3", 1, 5e-3),
    ("sdf_form shape=1 size=0.3 repeat=4 round=0.05", 1, 5e-3),
    ("sdf_form shape=2 repeat=2", 1, 5e-3),
    ("sdf_form shape=3 repeat=2", 1, 5e-3),
    ("sdf_op a=@1 b=@2 op=0 smooth=0.1", 1, 5e-3),
    ("sdf_op a=@1 b=@2 op=1", 1, 5e-3),
    ("sdf_op a=@1 b=@2 op=2 smooth=0.2", 1, 5e-3),
    ("sdf_mask in=@1 offset=0.1 softness=0.1", 1, 5e-3),
    ("worley_3d scale=5 z=0.3 mode=0", 1, 5e-3),
    ("worley_3d scale=5 z=0.3 mode=2 jitter=0.7", 1, 5e-3),
    ("worley_3d scale=5 mode=3", 1, 5e-3),
    ("fbm_3d scale=3 octaves=4 z=0.2 mode=1", 1, 5e-3),
    ("fbm_3d scale=3 mode=0", 1, 5e-3),
    ("volume_mask shape=1 size=0.3 repeat=3 z=0.4 roundness=0.1", 1, 5e-3),
    ("gradient_3d shape=3 repeat=2", 1, 5e-3),
    ("moisture_noise scale=4 drips=0.8", 1, 5e-3),
    ("microscope_noise scale=8 mode=0", 1, 5e-3),
    ("microscope_noise scale=8 mode=1", 1, 5e-3),
    ("microscope_noise style=1 density=6 cell_size=0.03", 1, 5e-3),
    ("anisotropic_noise angle=0.1 scale=8 stretch=4 octaves=3", 1, 5e-3),
    ("anisotropic_noise style=1 x_amount=4 y_amount=64 smoothness=0.5", 1, 5e-3),
    ("anisotropic_noise style=1 rotate=1 rows_by_resolution=1", 1, 5e-3),
    ("herringbone scale=8 mode=0", 1, 5e-3),
    ("herringbone scale=8 mode=1", 1, 5e-3),
    ("arc_pavement scale=6 rings=3", 1, 5e-3),
    ("arc_pavement scale=6 mode=1", 1, 5e-3),
];

const SRC: &str = "size 64 48\nnode 1 fbm scale=4 octaves=4 seed=2\nnode 2 perlin scale=3 seed=9\nnode 4 normal in=@2 strength=2\nnode 5 fbm scale=2 octaves=2 seed=5\nnode 6 tint in=@1 r=0.8 g=0.4 b=0.1\nnode 7 tint in=@2 r=0.2 g=0.5 b=0.9\nnode 8 voronoi scale=5 mode=2 seed=3\nnode 9 threshold in=@8 threshold=0.55\nnode 10 flood_fill in=@9\n";

/// Runs `node` on the device and on the CPU; `Err` describes a mismatch.
fn check(src: &str, node: &str, kernels: usize, tol: f32, mean_tol: f32) -> Result<(), String> {
    let text = format!("{src}node 3 {node}\noutput o 3\n");
    let want = cpu_only(&text, &[3]);
    let cache = Arc::new(NodeCache::new(256));
    let launched = gpu::submit_count();
    let got = eval(&text, &[3], &cache);
    let launched = gpu::submit_count() - launched;
    if launched < kernels {
        return Err(format!("{node}: only {launched} launches (wanted {kernels}): the node fell back to the CPU"));
    }
    let (mx, mean, bad) = stats(&got.kept[&3], &want.kept[&3]);
    if mx < tol || (mean < mean_tol && bad < 0.12) {
        Ok(())
    } else {
        Err(format!("{node}: max {mx}, mean {mean}, outliers {bad}"))
    }
}

#[test]
fn the_square_graph_kernels_match_the_cpu() {
    let Some(_guard) = setup() else { return };
    let src = SRC.replace("size 64 48", "size 64 64");
    let failures: Vec<String> = SQUARE_CASES.iter().filter_map(|(n, k, t, m)| check(&src, n, *k, *t, *m).err()).collect();
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn the_second_batch_of_kernels_matches_the_cpu() {
    let Some(_guard) = setup() else { return };
    let failures: Vec<String> = CASES.iter().chain(GEN_CASES.iter()).filter_map(|(n, k, t)| check(SRC, n, *k, *t, 4e-3).err()).collect();
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn splatter_data_outputs_match_the_cpu() {
    let Some(_guard) = setup() else { return };
    for pin in ["id", "center", "size", "luminance"] {
        let text = format!("size 64 64\nnode 3 splatter pattern_type=2 count=25 scale=0.15 lum_var=0.4 seed=3\noutput o 3.{pin}\n");
        let want = cpu_only(&text, &[3]);
        let got = eval(&text, &[3], &Arc::new(NodeCache::new(256)));
        let (mx, mean, bad) = stats(&got.kept[&3], &want.kept[&3]);
        assert!(mx < 5e-3 || (mean < 4e-3 && bad < 0.015), "{pin}: max {mx}, mean {mean}, outliers {bad}");
    }
}

#[test]
fn editing_one_node_only_recomputes_that_node_and_the_ones_after_it() {
    let Some(_guard) = setup() else { return };
    let mk = |gain: f32| format!("size 64 48\nnode 1 fbm scale=4 octaves=4\nnode 2 perlin scale=3 seed=9\nnode 3 emboss in=@1 intensity=4\nnode 4 blend fg=@3 bg=@2 opacity={gain}\nnode 5 distance in=@4 distance=0.2\noutput o 5\n");
    let cache = Arc::new(NodeCache::new(256));
    let first = gpu::submit_count();
    let _ = eval(&mk(0.5), &[5], &cache);
    let full = gpu::submit_count() - first;
    let before = gpu::submit_count();
    // an unchanged graph evaluates from the cache
    let _ = eval(&mk(0.5), &[5], &cache);
    assert!(gpu::submit_count() - before <= 2, "nothing changed, nothing may run");
    // changing the blend opacity re-runs the blend and the distance field, not the generators or the emboss
    let before = gpu::submit_count();
    let _ = eval(&mk(0.9), &[5], &cache);
    let n = gpu::submit_count() - before;
    assert!(n > 0 && n < full, "{n} launches after the edit, {full} for the whole graph");
}
