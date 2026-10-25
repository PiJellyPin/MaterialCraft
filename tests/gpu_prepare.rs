// SPDX-License-Identifier: MIT OR Apache-2.0
//! The 3D view's maps packed on the device (`prepare_dev`) equal the ones the host code makes (`prepare`), and the
//! deferred results of an evaluation can be read back later. Needs the `gui` feature and an adapter (a software one is
//! accepted here); skips otherwise.
#![cfg(feature = "gui")]
use materialcraft::graph::{Graph, NodeCache};
use materialcraft::gpu3d::{prepare, prepare_dev, read_packed, DevInputs, GpuMaps, MapInputs};
use materialcraft::{gpu, noise, ops, tex::Tex};
use std::sync::Arc;

static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn setup() -> Option<std::sync::MutexGuard<'static, ()>> {
    let g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("MC_GPU", "force");
    gpu::set_enabled(true);
    gpu::set_min_pixels(0);
    if gpu::adapter_name().is_some() { Some(g) } else { None }
}

fn colour(size: usize, seed: u32) -> Tex {
    Tex::from_fn(size, size, |u, v| [noise::perlin(u, v, 3, 3, seed), noise::perlin(u, v, 4, 4, seed + 1), noise::perlin(u, v, 5, 5, seed + 2), 1.0])
}

fn close_bytes(a: &[u8], b: &[u8], what: &str) {
    assert_eq!(a.len(), b.len(), "{what}: length");
    // the host and the device may round a value exactly on a boundary differently: never by more than one step
    let bad = a.iter().zip(b).filter(|(x, y)| (**x as i32 - **y as i32).abs() > 1).count();
    assert_eq!(bad, 0, "{what}: {bad} bytes differ by more than one step");
    let off = a.iter().zip(b).filter(|(x, y)| x != y).count();
    assert!(off * 500 < a.len(), "{what}: {off} of {} bytes differ", a.len());
}

fn same(a: &GpuMaps, b: &GpuMaps) {
    assert_eq!(a.size, b.size);
    close_bytes(&a.base, &b.base, "base");
    close_bytes(&a.normal, &b.normal, "normal");
    close_bytes(&a.rma, &b.rma, "rma");
    assert_eq!(a.emissive.is_some(), b.emissive.is_some());
    if let (Some(x), Some(y)) = (&a.emissive, &b.emissive) {
        close_bytes(x, y, "emissive");
    }
    assert_eq!(a.height.is_some(), b.height.is_some());
    if let (Some(x), Some(y)) = (&a.height, &b.height) {
        assert_eq!((x.0, x.1), (y.0, y.1));
        let worst = x.2.iter().zip(y.2.iter()).map(|(p, q)| (p - q).abs()).fold(0.0f32, f32::max);
        assert!(worst < 2e-3, "height differs by {worst}");
        let (hx, hy) = (a.height_half.as_ref().unwrap(), b.height_half.as_ref().unwrap());
        assert_eq!(hx.len(), hy.len());
        let off = hx.iter().zip(hy).filter(|(p, q)| (**p as i32 - **q as i32).abs() > 1).count();
        assert_eq!(off, 0, "half floats differ by more than one step");
    }
}

#[test]
fn device_maps_equal_host_maps() {
    let Some(_guard) = setup() else { return };
    let n = 48;
    let (base, normal, rough, metal, ao, emis) = (colour(n, 1), colour(n, 4), colour(n, 7), colour(n, 9), colour(n, 11), colour(n, 13));
    let height = Tex::gray_fn(40, 56, |u, v| noise::perlin(u, v, 4, 4, 3) * 3.0 - 1.0);
    let up = |t: &Tex| gpu::upload(t).unwrap();
    let (gb, gn, gr, gm, ga, ge, gh) = (up(&base), up(&normal), up(&rough), up(&metal), up(&ao), up(&emis), up(&height));
    // everything present
    let host = prepare(&MapInputs { base: Some(&base), normal: Some(&normal), rough: Some(&rough), metal: Some(&metal), ao: Some(&ao), emissive: Some(&emis), height: Some(&height) });
    let dev = prepare_dev(&DevInputs { base: Some(&gb), normal: Some(&gn), flip_normal_g: false, rough: Some(&gr), rough_invert: false, metal: Some(&gm), ao: Some(&ga), emissive: Some(&ge), height: Some(&gh) }).expect("device maps");
    same(&host, &read_packed(&dev).expect("packed on device"));
    // a DirectX normal map is flipped and glossiness becomes roughness
    let flipped = ops::flip_green(&normal);
    let inv = ops::invert(&rough);
    let host = prepare(&MapInputs { base: Some(&base), normal: Some(&flipped), rough: Some(&inv), metal: None, ao: None, emissive: None, height: None });
    let dev = prepare_dev(&DevInputs { base: Some(&gb), normal: Some(&gn), flip_normal_g: true, rough: Some(&gr), rough_invert: true, metal: None, ao: None, emissive: None, height: None }).expect("device maps");
    same(&host, &read_packed(&dev).expect("packed on device"));
    // missing maps take the same defaults
    let host = prepare(&MapInputs { base: None, normal: None, rough: None, metal: Some(&metal), ao: None, emissive: None, height: Some(&height) });
    let dev = prepare_dev(&DevInputs { base: None, normal: None, flip_normal_g: false, rough: None, rough_invert: false, metal: Some(&gm), ao: None, emissive: None, height: Some(&gh) }).expect("device maps");
    same(&host, &read_packed(&dev).expect("packed on device"));
    // nothing but a height map
    let host = prepare(&MapInputs { base: None, normal: None, rough: None, metal: None, ao: None, emissive: None, height: Some(&height) });
    let dev = prepare_dev(&DevInputs { base: None, normal: None, flip_normal_g: false, rough: None, rough_invert: false, metal: None, ao: None, emissive: None, height: Some(&gh) }).expect("device maps");
    same(&host, &read_packed(&dev).expect("packed on device"));
    // maps of different sizes are the host code's job (it resamples them)
    let small = gpu::upload(&colour(24, 2)).unwrap();
    assert!(prepare_dev(&DevInputs { base: Some(&gb), normal: Some(&small), flip_normal_g: false, rough: None, rough_invert: false, metal: None, ao: None, emissive: None, height: None }).is_none());
}

#[test]
fn deferred_outputs_come_back_when_asked() {
    let Some(_guard) = setup() else { return };
    let text = "size 64 64\nnode 1 fbm scale=4 octaves=4\nnode 2 blur in=@1 radius=40\nnode 3 normal in=@2 strength=2\nnode 4 invert in=@2\noutput normal 3\noutput height 4\n";
    let g = Graph::parse(text).unwrap();
    let cache = Arc::new(NodeCache::new(256));
    let (n3, n4) = (3u32, 4u32);
    let before = gpu::download_count();
    let r = g.evaluate_all_cached_ex(64, &[n3, n4], &[n3, n4], 0, &cache, None, false, &mut |_| {});
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert!(r.kept.is_empty() && r.pending.len() == 2);
    assert_eq!(gpu::download_count(), before, "a deferred output was read back");
    let p = &r.pending[&n3];
    assert!(p.device().is_some());
    let want = {
        gpu::set_enabled(false);
        let w = g.evaluate_all(64, &[n3]);
        gpu::set_enabled(true);
        w.kept[&n3].clone()
    };
    let got = p.tex().unwrap();
    let worst = got.px.iter().zip(&want.px).flat_map(|(a, b)| (0..4).map(move |c| (a[c] - b[c]).abs())).fold(0.0f32, f32::max);
    assert!(worst < 2e-2, "{worst}");
    assert_eq!(gpu::download_count(), before + 1);
    // the same pending result a second time costs nothing
    let again = p.tex().unwrap();
    assert!(Arc::ptr_eq(&got, &again));
    assert_eq!(gpu::download_count(), before + 1);
}

#[test]
fn the_2d_view_picture_is_made_on_the_device() {
    let Some(_guard) = setup() else { return };
    let text = "size 64 64\nnode 1 fbm scale=4 octaves=4\nnode 2 blur in=@1 radius=40\nnode 3 normal in=@2 strength=2\nnode 4 levels in=@2 in_low=0.2 in_high=0.7 out_low=-0.3 out_high=1.4\noutput normal 3\noutput height 4\n";
    let g = Graph::parse(text).unwrap();
    for id in [3u32, 4] {
        let cache = Arc::new(NodeCache::new(256));
        let mut first_calls = 0;
        let before = gpu::download_count();
        let r = g.evaluate_all_cached_ex(64, &[id], &[id], 0, &cache, Some(id), false, &mut |p| {
            first_calls += 1;
            assert!(p.device().is_some());
        });
        assert_eq!(first_calls, 1);
        assert!(r.errors.is_empty() && r.kept.is_empty(), "{:?}", r.errors);
        assert_eq!(gpu::download_count(), before, "the callback read the picture back");
        let p = &r.pending[&id];
        let (bytes, (lo, hi)) = p.view().expect("a device picture packs for the view");
        assert_eq!(bytes.len(), 64 * 64 * 4);
        // the host path: the pixels converted as the editor always did
        let t = p.tex().unwrap();
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        let want: Vec<u8> = t.px.iter().flat_map(|p| [q(p[0]), q(p[1]), q(p[2]), q(p[3])]).collect();
        close_bytes(&bytes, &want, "2d view bytes");
        let (wlo, whi) = t.px.iter().fold((f32::MAX, f32::MIN), |m, p| (m.0.min(p[0]).min(p[1]).min(p[2]), m.1.max(p[0]).max(p[1]).max(p[2])));
        assert!((lo - wlo).abs() < 1e-6 && (hi - whi).abs() < 1e-6, "range {lo}..{hi} vs {wlo}..{whi}");
    }
}
