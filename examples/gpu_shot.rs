// SPDX-License-Identifier: MIT OR Apache-2.0
//! GPU viewer diagnostic: renders a material with the wgpu viewer and writes a screenshot. No window is needed, only a
//! graphics adapter (MC_GPU=force also accepts a software one).
//! usage: cargo run --release --features gui --example gpu_shot -- out.png [shape 0|1|2] [relief 0..3] [width height] [wire] [rough=0.3 metal=0 color=r,g,b]
//!        [set=key=value]... (any View setting, e.g. set=env.preset=sunset set=post.bloom=0.4 set=rig=sun set=scene=floor)
//!        [cpu=cpu.png] also render the same view with the software renderer and print how far the two pictures are apart
//!        [graph=package.sbs] [size=1024] load an SBS material instead of the procedural brick material
use materialcraft::gpu3d::{self, MapInputs};
use materialcraft::render3d::View;
use materialcraft::tex::Tex;
use std::sync::Arc;

fn bricks(n: usize) -> (Tex, Tex, Tex, Tex, Tex) {
    let height = |u: f32, v: f32| -> f32 {
        let row = (v * 8.0).floor();
        let x = (u * 4.0 + (row as i32 % 2) as f32 * 0.5).rem_euclid(1.0);
        let y = (v * 8.0).rem_euclid(1.0);
        let d = (x.min(1.0 - x) * 2.0).min(y.min(1.0 - y) * 4.0);
        let s = ((d - 0.05) / 0.2).clamp(0.0, 1.0);
        let m = s * s * (3.0 - 2.0 * s);
        let grain = ((u * 190.0).sin() * (v * 230.0).cos() + (u * 63.0 + v * 41.0).sin()) * 0.03;
        (0.15 + 0.85 * m + grain * m).clamp(0.0, 1.0)
    };
    let h = Tex::gray_fn(n, n, |u, v| height(u, v));
    let e = 1.0 / n as f32;
    let normal = Tex::from_fn(n, n, |u, v| {
        let dx = (height(u + e, v) - height(u - e, v)) / (2.0 * e);
        let dy = (height(u, v + e) - height(u, v - e)) / (2.0 * e);
        let k = 0.006;
        let nx = -dx * k;
        let ny = dy * k;
        let l = (nx * nx + ny * ny + 1.0).sqrt();
        [nx / l * 0.5 + 0.5, ny / l * 0.5 + 0.5, 1.0 / l * 0.5 + 0.5, 1.0]
    });
    let base = Tex::from_fn(n, n, |u, v| {
        let hh = height(u, v);
        let t = ((hh - 0.15) / 0.85).clamp(0.0, 1.0);
        let k = ((u * 37.0).floor() * 7.0 + (v * 8.0).floor() * 13.0).sin() * 0.06;
        [0.20 + t * (0.62 + k), 0.12 + t * (0.28 + k * 0.5), 0.10 + t * (0.22 + k * 0.4), 1.0]
    });
    let rough = Tex::gray_fn(n, n, |u, v| 0.95 - 0.3 * height(u, v));
    let ao = Tex::gray_fn(n, n, |u, v| 0.35 + 0.65 * height(u, v));
    (base, normal, rough, ao, h)
}

struct TexSet {
    base: Tex,
    normal: Tex,
    rough: Tex,
    metal: Tex,
    ao: Tex,
    emissive: Option<Tex>,
    height: Option<Tex>,
}

fn compare(path: &str, set: &TexSet, view: &View, gl: &[[f32; 4]], w: usize, h: usize) {
    use materialcraft::render3d::{self, Maps};
    let maps = Maps { base: &set.base, normal: &set.normal, rough: &set.rough, metal: &set.metal, ao: &set.ao, emissive: set.emissive.as_ref(), height: set.height.as_ref(), env: None };
    let t0 = std::time::Instant::now();
    let img = render3d::render(&maps, view, w, h);
    let dt = t0.elapsed().as_secs_f32();
    std::fs::write(path, materialcraft::png::encode(w, h, &img.px, 8)).unwrap();
    let (mut sum, mut worst, mut n) = (0.0f64, 0.0f32, 0usize);
    let (mut mean_gl, mut mean_cpu) = (0.0f64, 0.0f64);
    for (a, b) in gl.iter().zip(img.px.iter()) {
        for c in 0..3 {
            let d = (a[c] - b[c]).abs();
            sum += d as f64;
            worst = worst.max(d);
            mean_gl += a[c] as f64;
            mean_cpu += b[c] as f64;
            n += 1;
        }
    }
    println!("cpu render {w}x{h} in {dt:.2}s -> {path} | mean abs difference {:.4} (gl mean {:.4}, cpu mean {:.4}), worst {:.3}", sum / n as f64, mean_gl / n as f64, mean_cpu / n as f64, worst);
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let out = a.get(1).cloned().unwrap_or_else(|| "gpu_shot.png".into());
    let shape = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(2u32);
    let relief = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(3u32);
    let (w, h) = (a.get(4).and_then(|s| s.parse().ok()).unwrap_or(900.0f32), a.get(5).and_then(|s| s.parse().ok()).unwrap_or(700.0f32));
    let wire = a.iter().any(|s| s == "wire");
    let arg = |k: &str| a.iter().find_map(|s| s.strip_prefix(k).map(String::from));
    let gsize: usize = arg("size=").and_then(|s| s.parse().ok()).unwrap_or(1024);
    let gout;
    let set: TexSet = if let Some(path) = arg("graph=") {
        if !path.to_ascii_lowercase().ends_with(".sbs") {
            eprintln!("{path}: graph= expects a Substance package (.sbs)");
            std::process::exit(1);
        }
        let (g, warnings) = materialcraft::sbs_native::open(std::path::Path::new(&path)).unwrap_or_else(|e| {
            eprintln!("{path}: {e}");
            std::process::exit(1);
        });
        for warning in warnings {
            eprintln!("{path}: warning: {warning}");
        }
        gout = g.evaluate(Some(gsize)).unwrap_or_else(|e| {
            eprintln!("{path}: render failed: {e}");
            std::process::exit(1);
        });
        let get = |k: &str| gout.get(k).cloned();
        TexSet {
            base: get("basecolor").unwrap_or_else(|| Tex::solid(4, 4, [0.5, 0.5, 0.5, 1.0])),
            normal: get("normal").unwrap_or_else(|| Tex::solid(4, 4, [0.5, 0.5, 1.0, 1.0])),
            rough: get("roughness").unwrap_or_else(|| Tex::solid(4, 4, [0.5, 0.5, 0.5, 1.0])),
            metal: get("metallic").unwrap_or_else(|| Tex::solid(4, 4, [0.0, 0.0, 0.0, 1.0])),
            ao: get("ambient_occlusion").unwrap_or_else(|| Tex::solid(4, 4, [1.0; 4])),
            emissive: get("emissive"),
            height: get("height"),
        }
    } else if let Some(r) = arg("rough=") {
        // uniform material test: rough=0.3 metal=0 color=0.8,0.2,0.2
        let r: f32 = r.parse().unwrap_or(0.5);
        let m: f32 = arg("metal=").and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let c: Vec<f32> = arg("color=").map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
        let c = if c.len() == 3 { [c[0], c[1], c[2], 1.0] } else { [0.8, 0.2, 0.2, 1.0] };
        TexSet { base: Tex::solid(4, 4, c), normal: Tex::solid(4, 4, [0.5, 0.5, 1.0, 1.0]), rough: Tex::solid(4, 4, [r, r, r, 1.0]), metal: Tex::solid(4, 4, [m, m, m, 1.0]), ao: Tex::solid(4, 4, [1.0; 4]), emissive: None, height: None }
    } else {
        let (base, normal, rough, ao, height) = bricks(1024);
        TexSet { base, normal, rough, metal: Tex::solid(4, 4, [0.0, 0.0, 0.0, 1.0]), ao, emissive: None, height: Some(height) }
    };
    let maps = gpu3d::prepare(&MapInputs { base: Some(&set.base), normal: Some(&set.normal), rough: Some(&set.rough), metal: Some(&set.metal), ao: Some(&set.ao), emissive: set.emissive.as_ref(), height: set.height.as_ref() });
    let gpu = gpu3d::new_shared();
    gpu.lock().unwrap().set_maps(maps);
    let mut view = View { shape, relief, wireframe: wire, yaw: 0.35, pitch: 0.5, dist: if shape == 2 { 3.2 } else { 3.4 }, height_scale: 0.08, ..View::default() };
    for kv in a.iter().filter_map(|s| s.strip_prefix("set=")) {
        let (k, v) = kv.split_once('=').expect("set=key=value");
        if let Err(e) = view.set(k, v) {
            eprintln!("bad setting {kv}: {e}");
            std::process::exit(1);
        }
    }
    if let Some(p) = arg("mesh=") {
        let cm = materialcraft::objects::CustomMesh::load(std::path::Path::new(&p)).expect("mesh");
        view.custom = Some(Arc::new(cm));
        view.shape = materialcraft::render3d::SHAPE_CUSTOM;
    }
    let cpu = arg("cpu=").map(|p| (p, set));
    let (w, h) = (w as usize, h as usize);
    let Some(f) = gpu3d::frame(&gpu, w, h, &view) else {
        eprintln!("GPU ERROR: {}", gpu.lock().unwrap().error.clone().unwrap_or_default());
        std::process::exit(2);
    };
    let px: Vec<[f32; 4]> = f.rgba.chunks(4).map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]).collect();
    std::fs::write(&out, materialcraft::png::encode(f.w, f.h, &px, 8)).unwrap();
    println!("renderer: {} | {}x{} written to {}", gpu.lock().unwrap().renderer, f.w, f.h, out);
    if let Some((p, set)) = &cpu {
        compare(p, set, &view, &px, f.w, f.h);
    }
}
