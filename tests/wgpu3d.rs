// SPDX-License-Identifier: MIT OR Apache-2.0
//! The wgpu 3D viewer: it draws, it agrees roughly with the software renderer, a camera move does not repack the
//! maps and an unchanged view is not drawn again. Needs an adapter (a software one is accepted); skips otherwise.
#![cfg(feature = "gpu")]
use materialcraft::gpu3d::{self, frame, MapInputs};
use materialcraft::render3d::{self, Maps, View};
use materialcraft::{gpu, noise, tex::Tex};

fn colour(size: usize, seed: u32) -> Tex {
    Tex::from_fn(size, size, |u, v| [0.3 + 0.5 * noise::perlin(u, v, 3, 3, seed), 0.2 + 0.4 * noise::perlin(u, v, 4, 4, seed + 1), 0.2, 1.0])
}

#[test]
fn viewer_draws_and_caches() {
    std::env::set_var("MC_GPU", "force");
    gpu::set_enabled(true);
    gpu::set_min_pixels(0);
    if gpu::adapter_name().is_none() {
        return;
    }
    let n = 64;
    let base = colour(n, 1);
    let normal = Tex::solid(n, n, [0.5, 0.5, 1.0, 1.0]);
    let rough = Tex::solid(n, n, [0.5, 0.5, 0.5, 1.0]);
    let metal = Tex::solid(n, n, [0.0, 0.0, 0.0, 1.0]);
    let ao = Tex::solid(n, n, [1.0; 4]);
    let height = Tex::gray_fn(n, n, |u, v| noise::perlin(u, v, 4, 4, 5));
    let maps = gpu3d::prepare(&MapInputs { base: Some(&base), normal: Some(&normal), rough: Some(&rough), metal: Some(&metal), ao: Some(&ao), emissive: None, height: Some(&height) });
    let shared = gpu3d::new_shared();
    shared.lock().unwrap().set_maps(maps);
    let mut view = View { shape: 2, relief: 0, yaw: 0.35, pitch: 0.5, dist: 3.2, ..View::default() };
    let (w, h) = (160, 120);
    let a = frame(&shared, w, h, &view).unwrap_or_else(|| panic!("viewer failed: {:?}", shared.lock().unwrap().error));
    assert_eq!((a.w, a.h, a.rgba.len()), (w, h, w * h * 4));
    assert!(a.rgba.chunks(4).any(|p| p[0] > 20 || p[1] > 20 || p[2] > 20), "picture is black");
    let packs = shared.lock().unwrap().pack_count;
    // identical view: same picture, nothing drawn
    let b = frame(&shared, w, h, &view).unwrap();
    assert_eq!(a.seq, b.seq);
    // camera move: new picture, maps not packed again
    view.yaw += 0.3;
    let c = frame(&shared, w, h, &view).unwrap();
    assert_ne!(a.seq, c.seq);
    assert_ne!(a.rgba, c.rgba);
    assert_eq!(shared.lock().unwrap().pack_count, packs, "camera move repacked the maps");
    // agreement with the software renderer: loose, the lighting models are not identical
    view.yaw = 0.35;
    let g = frame(&shared, w, h, &view).unwrap();
    let m = Maps { base: &base, normal: &normal, rough: &rough, metal: &metal, ao: &ao, emissive: None, height: Some(&height), env: None };
    let cpu = render3d::render(&m, &view, w, h);
    let (mut sum, mut cnt) = (0.0f64, 0usize);
    for (p, q) in g.rgba.chunks(4).zip(cpu.px.iter()) {
        for k in 0..3 {
            sum += (p[k] as f64 / 255.0 - q[k] as f64).abs();
            cnt += 1;
        }
    }
    let mean = sum / cnt as f64;
    assert!(mean < 0.25, "mean abs difference to the software renderer {mean}");
    // wireframe draws too
    view.wireframe = true;
    assert!(frame(&shared, w, h, &view).is_some());
    assert!(shared.lock().unwrap().error.is_none());
}
