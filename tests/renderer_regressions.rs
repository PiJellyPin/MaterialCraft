// SPDX-License-Identifier: MIT OR Apache-2.0
use materialcraft::{graph::{Graph, Node}, nodes10, ops, png, tex::{lum, Tex}};
use std::collections::BTreeMap;
use std::sync::Arc;

#[test]
fn crop_downsampling_keeps_area_averages_and_nearest_keeps_source_texels() {
    // Bilinear alone aliases this signal to black; area downsampling must retain its 25% coverage.
    let src = Tex::from_fn(64, 64, |u, _| nodes10::grey(if (u * 64.0) as usize % 4 == 0 { 1.0 } else { 0.0 }));
    let png = Arc::new(png::encode(64, 64, &src.px, 8));
    let want = ops::resize_to(&src, 16, 16);
    assert!(want.px.iter().all(|p| p[0] == 0.25));
    for kind in ["x_crop", "x_crop_grayscale"] {
        let area = nodes10::info(kind).unwrap().outs[1];
        for filtering in [0, 1] {
            // Ask for either output alone and both together, exercising sibling reuse as well.
            for outputs in ["output crop 2\n".to_string(), format!("output area 2.{area}\n"), format!("output crop 2\noutput area 2.{area}\n")] {
                let mut g = Graph::parse(&format!("size 64\nnode 1 bitmap file=\"stripes.png\"\nnode 2 {kind} input=@1 osize_mode=2 osize_abs=4 matrix_0=1 matrix_3=1 filtering={filtering} show_crop_area=0\n{outputs}")).unwrap();
                g.resources.insert("stripes.png".into(), png.clone());
                let out = g.evaluate(None).unwrap();
                for (name, t) in out {
                    assert_eq!((t.w, t.h), (16, 16));
                    if name == "area" || filtering == 0 {
                        assert_eq!(t.px, want.px, "{kind}: {name}, filtering {filtering}");
                    } else {
                        assert!(t.px.iter().all(|p| src.px.contains(p)), "{kind}: Nearest must read original texels");
                        assert_ne!(t.px, want.px);
                    }
                }
            }
        }
    }
}

#[test]
fn crop_edges_clamp_and_outside_uses_background() {
    for kind in ["x_crop", "x_crop_grayscale", "x_multi_crop", "x_multi_crop_grayscale"] {
        for filtering in [0, 1] {
            for axis in [0, 1] {
                let src = Arc::new(Tex::from_fn(4, 4, |u, v| nodes10::grey(if axis == 0 { u } else { v })));
                let inputs = BTreeMap::from([(if kind.starts_with("x_multi") { "input_1" } else { "input" }.into(), src)]);
                for (offset, index, expected) in [(-0.125, 0, 0.125), (0.125, 3, 0.875), (-0.0625, 0, 0.125), (0.0625, 3, 0.875), (-0.25, 0, 0.0), (0.25, 3, 0.0)] {
                    let mut n = Node::default();
                    n.nums = BTreeMap::from([("matrix_0".into(), 1.0), ("matrix_3".into(), 1.0), ("filtering".into(), filtering as f32), (format!("offset_{axis}"), offset)]);
                    let t = nodes10::compute(kind, 1, &n, &inputs, 4, 4, 0).unwrap().unwrap();
                    let p = t.at(if axis == 0 { index } else { 1 }, if axis == 1 { index } else { 1 });
                    let want = if offset.abs() > 0.125 { nodes10::with_params(kind, &n, 4, 4, 0, |cx| cx.col("background")).unwrap() } else { nodes10::grey(expected) };
                    assert_eq!(p, want, "{kind}: filtering {filtering}, axis {axis}, offset {offset}");
                }
            }
        }
    }
    // A one-texel source has no neighbouring texel to blend or wrap to.
    let inputs = BTreeMap::from([("input".into(), Arc::new(Tex::solid(1, 1, [0.2, 0.4, 0.6, 0.8])))]);
    for filtering in [0.0, 1.0] {
        let n = Node { nums: BTreeMap::from([("matrix_0".into(), 1.0), ("matrix_3".into(), 1.0), ("offset_0".into(), 0.5), ("filtering".into(), filtering)]), ..Default::default() };
        assert_eq!(nodes10::compute("x_crop", 1, &n, &inputs, 1, 1, 0).unwrap().unwrap().px, inputs["input"].px);
    }
    // Floating-point UV rounding must never blend the opposite edge on odd-sized images.
    let src = Arc::new(Tex::from_fn(41, 29, |u, v| [u, v, 0.0, 1.0]));
    let inputs = BTreeMap::from([("input".into(), src.clone())]);
    let n = Node { nums: BTreeMap::from([("matrix_0".into(), 1.0), ("matrix_3".into(), 1.0), ("offset_0".into(), 0.25 / 41.0), ("offset_1".into(), 0.25 / 29.0)]), ..Default::default() };
    let t = nodes10::compute("x_crop", 1, &n, &inputs, 41, 29, 0).unwrap().unwrap();
    assert_eq!(t.px.last(), src.px.last());
}

#[test]
fn photometric_optimizations_preserve_reference_pixels() {
    for count in 2..=8 {
        for missing in [false, true] {
            let inputs: BTreeMap<String, Arc<Tex>> = (1..=count).filter(|i| !missing || i % 3 != 1).map(|i| {
                // Some equal-luminance samples have different alpha: sorting must remain stable.
                let t = Tex::from_fn(16, 12, |u, v| [u * (i / 2) as f32 / 4.0, v, 0.3, i as f32 / 8.0]);
                (format!("input_{i}"), Arc::new(t))
            }).collect();
            let mut n = Node { nums: BTreeMap::from([("samples_amount".into(), count as f32)]), ..Default::default() };
            let before: Vec<_> = inputs.values().map(|t| t.px.clone()).collect();
            let samples: Vec<_> = (1..=count).filter_map(|i| inputs.get(&format!("input_{i}"))).collect();
            let want = Tex::from_fn(16, 12, |u, v| {
                let mut colors: Vec<_> = samples.iter().map(|t| t.sample(u, v)).collect();
                colors.sort_by(|a, b| lum(*a).partial_cmp(&lum(*b)).unwrap());
                let m = colors.len() / 2;
                if colors.len() % 2 == 0 { nodes10::lerp4(colors[m - 1], colors[m], 0.5) } else { colors[m] }
            });
            let got = nodes10::compute("x_multiangle_to_albedo", 1, &n, &inputs, 16, 12, 0).unwrap().unwrap();
            assert_eq!(got.px, want.px, "albedo: {count} samples, missing={missing}");
            for direction in [0, 1] {
                for format in [0, 1] {
                    n.nums.extend([("first_sample_light_angle".into(), 0.13), ("next_sample_light_angle".into(), direction as f32), ("normal_format".into(), format as f32), ("intensity".into(), 0.7)]);
                    let want = Tex::from_fn(16, 12, |u, v| {
                        let (mut gx, mut gy, mut total) = (0.0f32, 0.0f32, 0.0f32);
                        for i in 1..=count {
                            if let Some(t) = inputs.get(&format!("input_{i}")) {
                                let step = if direction == 0 { 1.0 / count as f32 } else { -1.0 / count as f32 };
                                let a = (0.13 + step * (i - 1) as f32) * std::f32::consts::TAU;
                                let l = lum(t.sample(u, v));
                                gx += l * a.cos();
                                gy += l * a.sin();
                                total += l;
                            }
                        }
                        let k = 0.7 * 4.0 / total.max(1e-4);
                        nodes10::enc_n([gx * k, gy * k, 1.0], format == 0)
                    });
                    let got = nodes10::compute("x_multiangle_to_normal", 1, &n, &inputs, 16, 12, 0).unwrap().unwrap();
                    assert_eq!(got.px, want.px, "normal: {count} samples, missing={missing}, direction={direction}, format={format}");
                }
            }
            assert_eq!(before, inputs.values().map(|t| t.px.clone()).collect::<Vec<_>>());
        }
    }
    for (kind, pixel) in [("x_multiangle_to_albedo", nodes10::grey(0.0)), ("x_multiangle_to_normal", [0.5, 0.5, 1.0, 1.0])] {
        let t = nodes10::compute(kind, 1, &Node::default(), &BTreeMap::new(), 16, 12, 0).unwrap().unwrap();
        assert!(t.px.iter().all(|p| *p == pixel));
    }
}
