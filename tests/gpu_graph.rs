// SPDX-License-Identifier: MIT OR Apache-2.0
//! Device-resident graph evaluation: every node that has a GPU kernel gives the CPU result, chains of them never read
//! back to the host between nodes, and the editor's node cache keeps the device pictures between edits. Needs the
//! `gpu` feature and an adapter (a software one is accepted here); skips otherwise.
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

fn max_diff(a: &Tex, b: &Tex) -> f32 {
    assert_eq!((a.w, a.h), (b.w, b.h));
    a.px.iter().zip(&b.px).flat_map(|(p, q)| (0..4).map(move |c| (p[c] - q[c]).abs())).fold(0.0, f32::max)
}

/// The editor's evaluation of `text` at size 64, keeping the nodes in `keep`.
fn editor(text: &str, keep: &[u32], cache: &Arc<NodeCache>, full: bool) -> materialcraft::graph::EvalAll {
    let g = Graph::parse(text).unwrap();
    let r = g.evaluate_all_cached(64, keep, 0, cache, None, full, &mut |_| {});
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    r
}

fn cpu_only(text: &str, keep: &[u32]) -> materialcraft::graph::EvalAll {
    gpu::set_enabled(false);
    let cache = Arc::new(NodeCache::new(256));
    let r = editor(text, keep, &cache, false);
    gpu::set_enabled(true);
    r
}

#[test]
fn every_gpu_node_matches_the_cpu_and_runs_resident() {
    let Some(_guard) = setup() else { return };
    let src = "size 64 48\nnode 1 fbm scale=4 octaves=4 seed=2\nnode 2 perlin scale=3 seed=9\nnode 4 normal in=@2 strength=2\nnode 5 fbm scale=2 octaves=2 seed=5\n";
    let cases: &[(&str, f32, usize)] = &[
        ("node 3 perlin scale=5 seed=3", 3e-3, 1),
        ("node 3 perlin scale_x=6 scale_y=3 seed=3 square_cells=1 contrast=1.4 invert=1", 6e-3, 1),
        ("node 3 fbm scale=4 octaves=5 gain=0.6 mode=1 seed=8", 5e-3, 1),
        ("node 3 clouds scale=3 seed=4 contrast=0.7", 5e-3, 1),
        ("node 3 directional_noise scale=5 stretch=10 angle=0.2 seed=3", 6e-3, 1),
        ("node 3 blur in=@1 radius=40", 2e-3, 2),
        ("node 3 directional_blur in=@1 angle=0.15 length=0.08", 3e-3, 2),
        ("node 3 anisotropic_blur in=@1 intensity=60 anisotropy=0.2 angle=0.3", 6e-3, 2),
        ("node 3 levels in=@1 low=0.2 high=0.8 gamma=1.5 out_low=0.1 out_high=0.9", 2e-3, 2),
        ("node 3 invert in=@1", 1e-6, 2),
        ("node 3 invert in=@1 invert=0", 1e-6, 2),
        ("node 3 warp in=@1 warp=@2 intensity=0.08 filtering=2", 8e-3, 2),
        ("node 3 warp in=@1 warp=@2 intensity=0.08 filtering=0", 8e-3, 2),
        ("node 3 directional_warp in=@1 warp=@2 angle=0.2 intensity=0.1 offset=0.4", 8e-3, 2),
        ("node 3 vector_warp in=@1 vector=@4 intensity=0.05", 8e-3, 2),
        ("node 3 vector_warp in=@1 vector=@4 intensity=0.05 directx=1", 8e-3, 2),
        ("node 3 slope_blur in=@1 slope=@2 samples=12 intensity=0.1 mode=1", 6e-3, 2),
        ("node 3 blend fg=@1 bg=@2 mode=5 opacity=0.8", 4e-3, 2),
        ("node 3 blend fg=@1 bg=@2 mask=@5 mode=2 opacity=0.6 alpha_mode=0", 4e-3, 2),
        ("node 3 normal in=@1 strength=3", 2e-2, 2),
        ("node 3 normal in=@1 strength=3 directx=1", 2e-2, 2),
        ("node 3 radial_blur in=@1 mode=0 samples=32 angle=0.1 x=0.1 y=-0.05", 4e-3, 2),
        ("node 3 radial_blur in=@1 mode=1 amount=0.3 radius=0.4", 4e-3, 1),
        ("node 3 bent_normal in=@1 radius=12 relief=0.5 directions=12 steps=6", 2e-2, 2),
        ("node 3 bilateral_blur in=@1 radius=3 range=0.1", 4e-3, 2),
        ("node 3 bilateral_blur in=@1 guide=@2 radius=3 range=0.2", 4e-3, 2),
        ("node 3 bilateral_blur in=@1 radius=0", 1e-6, 1),
        ("node 3 threshold in=@1 threshold=0.45 softness=0", 1e-6, 2),
        ("node 3 threshold in=@1 threshold=0.45 softness=0.2 mode=2", 2e-3, 2),
        ("node 3 threshold in=@1 threshold=0.5 mode=1", 1e-6, 2),
        ("node 3 clamp in=@1 low=0.3 high=0.7", 1e-6, 2),
        ("node 3 clamp in=@1 low=0.8 high=0.2 apply_alpha=1", 1e-6, 2),
        ("node 3 pow in=@1 exponent=2.2", 2e-3, 2),
        ("node 3 pow in=@1 exponent=0", 1e-6, 2),
        ("node 3 to_gray in=@1", 1e-6, 2),
        ("node 3 quantize in=@1 steps=5", 1e-6, 2),
        ("node 3 tint in=@1 r=0.9 g=0.5 b=0.2", 1e-6, 2),
        ("node 3 channel in=@1 ch=2", 1e-6, 2),
        ("node 3 histogram_shift in=@1 position=0.7", 1e-6, 2),
        ("node 3 histogram_scan in=@1 position=0.4 contrast=0.8", 2e-3, 2),
        ("node 3 histogram_scan in=@1 position=0.6 contrast=0.3 invert=1", 2e-3, 2),
        ("node 3 preset_noise preset=0 scale=3 roughness=1", 1e-3, 3),
        ("node 3 preset_noise preset=7 scale=2 roughness=0.5 angle=0.25 balance=0.6 contrast=1.3", 2e-3, 3),
        ("node 3 preset_noise preset=20 scale=4 seed=5 invert=1", 1e-3, 3),
        ("node 3 preset_noise preset=40 scale=1 roughness=2 seed=9", 1e-3, 3),
        ("node 3 preset_noise preset=60 scale=5", 1e-3, 3),
        ("node 3 x_rt_shadows input=@1 height_scale=0.6 light_position_0=0.8 light_position_1=0.25 samples=4 shadows_opacity=0.9", 2e-2, 2),
        ("node 3 x_rt_shadows input=@1 height_scale=0.3 light_position_0=0.3 light_position_1=0.6 samples=1 light_scale=0.2 invert=1", 2e-2, 2),
        ("node 3 x_shadows Input=@1 shadow_distance=0.5 light_angle=0.2 Samples=2", 2e-2, 2),
        ("node 3 ao in=@1 mode=1 quality=1 hbao_radius=0.6 height_depth=0.2 hbao_intensity=1.2", 1e-3, 2),
        ("node 3 ao in=@1 mode=1 quality=2 hbao_radius=1 height_depth=0.05", 1e-3, 2),
        ("node 3 ao in=@1 mode=1 quality=0 hbao_radius=0", 1e-3, 2),
        ("node 3 ao in=@1 mode=1 quality=1 use_world_units=1 height_depth_cm=10 surface_size=100", 1e-3, 2),
    ];
    for (node, tol, kernels) in cases {
        let text = format!("{src}{node}\noutput o 3\n");
        let want = cpu_only(&text, &[3]);
        // the whole graph runs on the device; only the kept output comes back, in one read-back
        let cache = Arc::new(NodeCache::new(256));
        let (before, launched) = (gpu::download_count(), gpu::submit_count());
        let got = editor(&text, &[3], &cache, false);
        let (reads, launched) = (gpu::download_count() - before, gpu::submit_count() - launched);
        assert!(launched >= *kernels, "{node}: only {launched} kernel launches, the node fell back to the CPU");
        let d = max_diff(&got.kept[&3], &want.kept[&3]);
        assert!(d < *tol, "{node}: {d}");
        assert_eq!(reads, 1, "{node}: {reads} read-backs");
        assert!(cache.gpu_bytes() > 0, "{node}: nothing stayed on the device");
    }
    // Warp moves the sample positions: a few pixels at lattice borders may fall on the other side, the picture agrees
    for (preset, extra) in [(0, "warp=1"), (20, "warp=1"), (40, "warp=1"), (20, "angle_random=0.5 seed=5"), (7, "warp=0.5 angle_random=1")] {
        let text = format!("{src}node 3 preset_noise preset={preset} scale=3 {extra}\noutput o 3\n");
        let want = cpu_only(&text, &[3]);
        let got = editor(&text, &[3], &Arc::new(NodeCache::new(256)), false);
        let (a, b) = (&got.kept[&3], &want.kept[&3]);
        let mean = a.px.iter().zip(&b.px).map(|(p, q)| (p[0] - q[0]).abs()).sum::<f32>() / a.px.len() as f32;
        assert!(mean < 2e-3, "preset {preset} ({extra}): mean difference {mean}");
    }
    // the occlusion output of the bent normal node
    let text = format!("{src}node 3 bent_normal in=@1 radius=12 relief=0.5\noutput o 3.ao\n");
    let want = cpu_only(&text, &[3]);
    let got = editor(&text, &[3], &Arc::new(NodeCache::new(256)), false);
    assert!(max_diff(&got.kept[&3], &want.kept[&3]) < 1e-2);
}

#[test]
fn a_chain_of_gpu_nodes_reads_back_once() {
    let Some(_guard) = setup() else { return };
    let text = concat!(
        "size 64 48\n",
        "node 1 fbm scale=5 octaves=4\nnode 2 levels in=@1 low=0.2 high=0.8 gamma=1.4\nnode 3 blur in=@2 radius=40\n",
        "node 4 directional_blur in=@3 angle=0.1 length=0.1\nnode 5 perlin scale=3 seed=4\n",
        "node 6 warp in=@4 warp=@5 intensity=0.05\nnode 7 blend fg=@5 bg=@6 mode=5 opacity=0.8\nnode 8 normal in=@7 strength=3\n",
        "node 9 output in=@8\noutput n 9\n",
    );
    let want = cpu_only(text, &[9]);
    let cache = Arc::new(NodeCache::new(256));
    let before = gpu::download_count();
    let got = editor(text, &[9], &cache, false);
    assert_eq!(gpu::download_count() - before, 1, "eight GPU nodes and an output node");
    assert!(max_diff(&got.kept[&9], &want.kept[&9]) < 3e-2);
}

#[test]
fn cpu_nodes_in_a_chain_only_read_what_they_use() {
    let Some(_guard) = setup() else { return };
    let text = "size 64 48\nnode 1 fbm scale=4 octaves=4\nnode 2 blur in=@1 radius=40\nnode 3 sharpen in=@2 radius=2 amount=1\nnode 4 levels in=@3 low=0.2 high=0.7\noutput o 4\n";
    let want = cpu_only(text, &[4]);
    let cache = Arc::new(NodeCache::new(256));
    let before = gpu::download_count();
    let got = editor(text, &[4], &cache, false);
    let reads = gpu::download_count() - before;
    assert!(max_diff(&got.kept[&4], &want.kept[&4]) < 6e-3);
    // blur -> sharpen (CPU) needs one read-back; levels after it is a plain upload and its result one more read-back
    assert!(reads <= 2, "{reads} read-backs");
}

#[test]
fn the_cache_keeps_device_pictures_between_edits() {
    let Some(_guard) = setup() else { return };
    let mk = |low: f32| format!("size 64 48\nnode 1 fbm scale=4 octaves=4\nnode 2 blur in=@1 radius=40\nnode 3 levels in=@2 low={low} high=0.9\nnode 4 normal in=@3 strength=2\noutput o 4\n");
    let cache = Arc::new(NodeCache::new(256));
    let a = editor(&mk(0.2), &[4], &cache, false);
    let (h0, m0) = cache.stats();
    assert_eq!(h0, 0);
    // same graph again: the output is served from the cache, nothing is recomputed
    let b = editor(&mk(0.2), &[4], &cache, false);
    let (h1, m1) = cache.stats();
    assert_eq!(m1, m0, "an unchanged graph recomputed something");
    assert!(h1 > h0);
    assert_eq!(a.kept[&4].px, b.kept[&4].px);
    // drop the host copies: the next hit has to read the device picture back, still without recomputing
    drop(a);
    drop(b);
    let before = gpu::download_count();
    let c = editor(&mk(0.2), &[4], &cache, false);
    assert_eq!(cache.stats().1, m1);
    assert_eq!(gpu::download_count() - before, 1);
    drop(c);
    // an edit in the middle recomputes the edited node and what depends on it, not what is upstream
    let (_, m2) = cache.stats();
    let d = editor(&mk(0.35), &[4], &cache, false);
    let (_, m3) = cache.stats();
    assert_eq!(m3 - m2, 2, "levels and normal only");
    let want = cpu_only(&mk(0.35), &[4]);
    assert!(max_diff(&d.kept[&4], &want.kept[&4]) < 2e-2);
    assert!(cache.gpu_bytes() > 0);
}

#[test]
fn thumbnails_of_device_pictures_match_the_cpu_ones() {
    let Some(_guard) = setup() else { return };
    let text = "size 64 48\nnode 1 fbm scale=4 octaves=4\nnode 2 blur in=@1 radius=40\nnode 3 normal in=@2 strength=2\nnode 4 invert in=@3\noutput o 4\n";
    gpu::set_enabled(false);
    let off = editor(text, &[4], &Arc::new(NodeCache::new(256)), true);
    gpu::set_enabled(true);
    let before = gpu::download_count();
    let on = editor(text, &[4], &Arc::new(NodeCache::new(256)), true);
    assert!(gpu::download_count() > before);
    for id in 1..=4u32 {
        let (a, b) = (&on.thumbs[&id], &off.thumbs[&id]);
        assert_eq!(a.len(), b.len());
        let worst = a.iter().zip(b).map(|(x, y)| (*x as i32 - *y as i32).abs()).max().unwrap();
        assert!(worst <= 6, "node {id}: thumbnail differs by {worst}");
    }
}

#[test]
fn several_outputs_come_back_in_one_batch() {
    let Some(_guard) = setup() else { return };
    let text = "size 64 48\nnode 1 fbm scale=4 octaves=4\nnode 2 blur in=@1 radius=40\nnode 3 normal in=@2 strength=2\nnode 4 invert in=@2\nnode 5 levels in=@2 low=0.3 high=0.7\noutput a 3\noutput b 4\noutput c 5\n";
    let want = cpu_only(text, &[3, 4, 5]);
    let cache = Arc::new(NodeCache::new(256));
    let before = gpu::download_count();
    let got = editor(text, &[3, 4, 5], &cache, false);
    assert_eq!(gpu::download_count() - before, 1);
    for id in [3u32, 4, 5] {
        assert!(max_diff(&got.kept[&id], &want.kept[&id]) < 2e-2, "node {id}");
    }
}
