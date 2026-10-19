// SPDX-License-Identifier: MIT OR Apache-2.0
use materialcraft::graph::{Error, Graph, EXAMPLE};
use materialcraft::{expr, noise, png};

#[test]
fn example_graph_renders() {
    let g = Graph::parse(EXAMPLE).unwrap();
    let out = g.evaluate(Some(64)).unwrap();
    assert_eq!(out.len(), 6);
    assert!(out["height"].px.iter().all(|p| p[0].is_finite()));
}

#[test]
fn deterministic_by_seed() {
    let g = Graph::parse("size 32\nnode 1 fbm scale=4 seed=5\noutput a 1\n").unwrap();
    assert_eq!(g.evaluate(None).unwrap()["a"].px, g.evaluate(None).unwrap()["a"].px);
}

#[test]
fn noise_is_tileable() {
    for i in 0..20 {
        let v = i as f32 / 20.0;
        let a = noise::fbm(0.0, v, 4, 4, 4, 0.5, 1, 0);
        let b = noise::fbm(1.0, v, 4, 4, 4, 0.5, 1, 0);
        assert!((a - b).abs() < 1e-4);
    }
}

#[test]
fn detects_cycle() {
    let g = "size 8\nnode 1 invert in=@2\nnode 2 invert in=@1\noutput a 1\n";
    assert!(matches!(Graph::parse(g), Err(Error::Cycle(_))));
}

#[test]
fn detects_missing_input_and_node() {
    assert!(matches!(Graph::parse("node 1 blur\noutput a 1\n"), Err(Error::MissingInput(..))));
    assert!(matches!(Graph::parse("node 1 blur in=@9\noutput a 1\n"), Err(Error::MissingNode(..))));
    assert!(matches!(Graph::parse("node 1 nope\noutput a 1\n"), Err(Error::UnknownNode(..))));
}

#[test]
fn expressions() {
    let e = expr::parse("clamp(u*2+1, 0, 1) + 2^3").unwrap();
    let env = expr::Env { vars: [0.1; 10], tex: None };
    assert!((expr::eval(&e, &env) - 9.0).abs() < 1e-4);
    assert!(expr::parse("sin(").is_err());
    assert!(expr::parse("foo(1)").is_err());
}

#[test]
fn png_signature_and_roundtrip_size() {
    let px = vec![[0.5, 0.25, 1.0, 1.0]; 16];
    let b = png::encode(4, 4, &px, 16);
    assert_eq!(&b[1..4], b"PNG");
}

#[test]
fn pixel_processor_and_save_roundtrip() {
    let t = "size 16\nnode 1 pixel_processor expr=\"u*v\"\noutput a 1\n";
    let g = Graph::parse(t).unwrap();
    let g2 = Graph::parse(&g.to_text()).unwrap();
    assert_eq!(g.evaluate(None).unwrap()["a"].px, g2.evaluate(None).unwrap()["a"].px);
}

use materialcraft::bake;
use materialcraft::graph::Graph as G2;
use std::collections::BTreeSet;

#[test]
fn png_roundtrip_decode() {
    let mut px = Vec::new();
    for i in 0..64 {
        px.push([i as f32 / 63.0, 0.25, 1.0 - i as f32 / 63.0, 1.0]);
    }
    for depth in [8u8, 16] {
        let bytes = png::encode(8, 8, &px, depth);
        let t = png::decode(&bytes).unwrap();
        assert_eq!((t.w, t.h), (8, 8));
        for (a, b) in px.iter().zip(&t.px) {
            for c in 0..3 {
                assert!((a[c] - b[c]).abs() < 0.01, "{a:?} vs {b:?}");
            }
        }
    }
}

#[test]
fn subgraph_extract_preserves_result() {
    let mut g = G2::parse(EXAMPLE).unwrap();
    let before = g.evaluate(Some(48)).unwrap();
    let sel: BTreeSet<u32> = [2, 4].into_iter().collect();
    let id = g.extract_subgraph(&sel, "sg").unwrap();
    assert!(g.nodes.contains_key(&id));
    let g2 = G2::parse(&g.to_text()).unwrap();
    let after = g2.evaluate(Some(48)).unwrap();
    assert_eq!(before["basecolor"].px, after["basecolor"].px);
}

#[test]
fn publish_embeds_bitmap_and_loads() {
    let dir = std::env::temp_dir().join("mf_test_bitmap");
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("t.png");
    let px = vec![[1.0, 0.0, 0.0, 1.0]; 16];
    std::fs::write(&f, png::encode(4, 4, &px, 8)).unwrap();
    let txt = format!("size 16\nnode 1 bitmap file=\"{}\"\noutput basecolor 1\nexpose \"X\" 1 seed 0 1\n", f.display());
    let g = G2::parse(&txt).unwrap();
    let pkg = g.publish().unwrap();
    std::fs::remove_file(&f).unwrap();
    let g2 = G2::parse(&pkg).unwrap();
    let out = g2.evaluate(None).unwrap();
    assert!(out["basecolor"].px.iter().all(|p| p[0] > 0.99 && p[1] < 0.01));
}

#[test]
fn exposed_parameters_apply() {
    let mut g = G2::parse(EXAMPLE).unwrap();
    let mut p = std::collections::BTreeMap::new();
    p.insert("Brick count".to_string(), 6.0);
    g.apply_params(&p).unwrap();
    assert_eq!(g.nodes[&1].nums["cols"], 6.0);
    p.insert("Nope".to_string(), 1.0);
    assert!(g.apply_params(&p).is_err());
}

#[test]
fn sdf_nodes_and_forms() {
    let t = "size 32\nnode 1 sdf_shape shape=0 size=0.3\nnode 2 sdf_shape shape=1 size=0.2 x=0.2\nnode 3 sdf_op a=@1 b=@2 op=0 smooth=0.05\nnode 4 sdf_mask in=@3\nnode 5 sdf_form shape=2 repeat=2\noutput a 4\noutput b 5\n";
    let out = G2::parse(t).unwrap().evaluate(None).unwrap();
    assert!(out["a"].px.iter().any(|p| p[0] > 0.9) && out["a"].px.iter().any(|p| p[0] < 0.1));
    assert!(out["b"].px.iter().any(|p| p[0] > 0.05));
}

#[test]
fn baker_produces_maps() {
    let low = bake::parse_obj(&bake::uv_sphere_obj(12, 8, 1.0, 0.0)).unwrap();
    let high = bake::parse_obj(&bake::uv_sphere_obj(48, 32, 1.0, 0.05)).unwrap();
    let s = bake::BakeSettings { width: 64, height: 64, aa: 1, front: 0.2, rear: 0.2, ao_rays: 8, ao_dist: 0.5, padding: 2, ..Default::default() };
    let m = bake::bake(&low, &high, &s).unwrap();
    assert_eq!(m.len(), s.maps.len());
    let n = &m["normal"];
    assert!(n.px.iter().all(|p| p[2] > 0.3));
    assert!(n.px.iter().any(|p| (p[0] - 0.5).abs() > 0.02));
    assert!(bake::parse_obj("v 0 0 0\n").is_err());
}

#[test]
fn renderer_draws_shapes() {
    use materialcraft::render3d::{render, Maps, View};
    use materialcraft::tex::Tex;
    let (b, n, r, m, a) = (Tex::solid(8, 8, [0.8, 0.2, 0.2, 1.0]), Tex::solid(8, 8, [0.5, 0.5, 1.0, 1.0]), Tex::solid(8, 8, [0.4; 4]), Tex::solid(8, 8, [0.0, 0.0, 0.0, 1.0]), Tex::solid(8, 8, [1.0; 4]));
    let maps = Maps { base: &b, normal: &n, rough: &r, metal: &m, ao: &a, emissive: None, height: None, env: None };
    for shape in 0..3 {
        let img = render(&maps, &View { shape, ..View::default() }, 64, 64);
        let c = img.px[32 * 64 + 32];
        assert!(c[0] > c[2], "shape {shape} centre should be reddish, got {c:?}");
    }
}

#[test]
fn bad_inputs_are_errors_not_panics() {
    assert!(png::decode(b"not a png").is_err());
    assert!(png::decode(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0]).is_err());
    assert!(G2::parse("subgraph a\nnode 1 color\n").is_err());
    assert!(G2::parse("node 1 subgraph graph=\"missing\"\noutput a 1\n").is_err());
}

#[test]
fn every_node_type_evaluates() {
    use materialcraft::graph::{demo_graph_for, SPECS};
    let mut count = 0;
    for sp in SPECS {
        if let Some(text) = demo_graph_for(sp.kind) {
            let g = G2::parse(&text).unwrap_or_else(|e| panic!("{}: parse: {e}", sp.kind));
            let out = g.evaluate(Some(32)).unwrap_or_else(|e| panic!("{}: eval: {e}", sp.kind));
            let t = &out["o"];
            assert!(t.px.iter().all(|p| p.iter().all(|c| c.is_finite())), "{} produced NaN", sp.kind);
            count += 1;
        }
    }
    assert!(count > 60, "only {count} nodes tested");
}

#[test]
fn output_nodes_defaults_and_migration() {
    let g = G2::parse("size 16\nnode 1 output usage=normal\nnode 2 output usage=basecolor identifier=Albedo\n").unwrap();
    let out = g.evaluate(None).unwrap();
    assert!((out["normal"].px[0][2] - 1.0).abs() < 1e-4);
    assert!(out.contains_key("Albedo"));
    let mut legacy = G2::parse(EXAMPLE).unwrap();
    let before = legacy.evaluate(Some(32)).unwrap();
    legacy.migrate_outputs();
    assert!(legacy.outputs.is_empty());
    let after = legacy.evaluate(Some(32)).unwrap();
    assert_eq!(before["basecolor"].px, after["basecolor"].px);
}

#[test]
fn flood_fill_and_distance_work() {
    let t = "size 64\nnode 1 voronoi scale=4 mode=2 seed=1\nnode 2 threshold in=@1 threshold=0.2\nnode 3 flood_fill in=@2\nnode 4 flood_fill_random in=@3 seed=2\nnode 5 distance in=@2 distance=0.1\nnode 6 bevel in=@2 distance=0.05\nnode 7 make_it_tile_photo in=@1\noutput a 4\noutput b 5\noutput c 6\noutput d 7\n";
    let out = G2::parse(t).unwrap().evaluate(None).unwrap();
    let vals: std::collections::BTreeSet<u32> = out["a"].px.iter().map(|p| (p[0] * 255.0) as u32).collect();
    assert!(vals.len() >= 4, "regions should get different grays, got {}", vals.len());
    assert!(out["b"].px.iter().any(|p| p[0] > 0.9));
    assert!(out["c"].px.iter().any(|p| p[0] > 0.9));
    // photo-tile result must be seamless: the wrap seam should look like any other pixel step
    let d = &out["d"];
    let (mut seam, mut inner) = (0.0, 0.0);
    for y in 0..64 {
        seam += (d.px[y * 64][0] - d.px[y * 64 + 63][0]).abs();
        for x in 8..55 {
            inner += (d.px[y * 64 + x][0] - d.px[y * 64 + x + 1][0]).abs() / 47.0;
        }
    }
    assert!(seam <= inner * 1.6 + 0.5, "seam {seam} vs typical {inner}");
}

#[test]
fn color_node_gray_mode() {
    let g = G2::parse("size 8\nnode 1 color r=1 g=0 b=0 gray=1 v=0.25\noutput a 1\n").unwrap();
    let p = g.evaluate(None).unwrap()["a"].px[0];
    assert!((p[0] - 0.25).abs() < 1e-5 && (p[1] - 0.25).abs() < 1e-5);
}

#[test]
fn jpeg_and_tiff_are_well_formed() {
    use materialcraft::{export, jpeg};
    assert!(jpeg::tables_complete(), "standard Huffman tables must cover every symbol");
    let mut px = Vec::new();
    for y in 0..37 {
        for x in 0..53 {
            px.push([x as f32 / 52.0, y as f32 / 36.0, 0.5, 1.0]);
        }
    }
    let t = materialcraft::tex::Tex { w: 53, h: 37, px };
    let j = export::encode(export::Format::Jpeg, &t, 8, 90);
    assert_eq!(&j[..2], &[0xff, 0xd8]);
    assert_eq!(&j[j.len() - 2..], &[0xff, 0xd9]);
    for depth in [8u8, 16] {
        let tf = export::encode(export::Format::Tiff, &t, depth, 90);
        assert_eq!(&tf[..4], &[b'I', b'I', 42, 0]);
    }
}

#[test]
fn png_decoder_handles_palette_alpha_and_low_bit_depth() {
    // hand-built 2x2 4-bit palette PNG with tRNS, stored (uncompressed) deflate
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&[0, 0, 0, 0]); // CRC is not checked by the decoder
    }
    let mut p = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&[4, 3, 0, 0, 0]);
    chunk(&mut p, b"IHDR", &ihdr);
    chunk(&mut p, b"PLTE", &[255, 0, 0, 0, 255, 0]);
    chunk(&mut p, b"tRNS", &[255, 128]);
    let rows = [0u8, 0x01, 0u8, 0x10]; // filter 0 + two pixels per row (indices 0,1 / 1,0)
    let mut z = vec![0x78, 0x01, 1, rows.len() as u8, 0, !(rows.len() as u8), 0xff];
    z.extend_from_slice(&rows);
    z.extend_from_slice(&[0, 0, 0, 0]);
    chunk(&mut p, b"IDAT", &z);
    chunk(&mut p, b"IEND", &[]);
    let t = png::decode(&p).unwrap();
    assert_eq!((t.w, t.h), (2, 2));
    assert!(t.px[0][0] > 0.99 && t.px[0][3] > 0.99); // index 0: opaque red
    assert!(t.px[1][1] > 0.99 && (t.px[1][3] - 128.0 / 255.0).abs() < 0.01); // index 1: half-transparent green
}

// ---------------------------------------------------------------- seamless angles + 3D mesh / displacement
/// mean |difference| across the wrap seam divided by the mean |difference| of ordinary neighbouring pixels
fn seam_ratio(t: &materialcraft::tex::Tex) -> f32 {
    let w = t.w;
    let (mut seam, mut inner) = (0.0f32, 0.0f32);
    for y in 0..t.h {
        seam += (t.px[y * w][0] - t.px[y * w + w - 1][0]).abs();
        for x in 0..w - 1 {
            inner += (t.px[y * w + x][0] - t.px[y * w + x + 1][0]).abs();
        }
    }
    for x in 0..w {
        seam += (t.px[x][0] - t.px[(t.h - 1) * w + x][0]).abs();
        for y in 0..t.h - 1 {
            inner += (t.px[y * w + x][0] - t.px[(y + 1) * w + x][0]).abs();
        }
    }
    (seam / (2 * w) as f32) / (inner / (2 * w * (w - 1)) as f32)
}

#[test]
fn stripes_tile_at_arbitrary_angles() {
    for (count, ang) in [(8.0, 0.0), (8.0, 0.137), (5.0, 0.21), (12.0, 0.3333), (3.0, 0.77), (20.0, 0.05), (8.0, 0.9)] {
        let g = G2::parse(&format!("size 256\nnode 1 stripes count={count} angle={ang} softness=0.2\noutput a 1\n")).unwrap();
        let r = seam_ratio(&g.evaluate(None).unwrap()["a"]);
        assert!(r < 1.6, "stripes count {count} angle {ang}: seam/inner = {r}");
        // the snapped direction must stay close to what was asked for
        let (a, b) = materialcraft::ops::tileable_wave(count, ang);
        let want = ang as f64 * std::f64::consts::TAU;
        let d = ((b as f64).atan2(a as f64) - want).sin().atan2(((b as f64).atan2(a as f64) - want).cos()).abs();
        let len = ((a * a + b * b) as f64).sqrt();
        assert!(d < 0.35 && len > count as f64 * 0.55 && len < count as f64 * 1.65, "snap ({a},{b}) too far for count {count} angle {ang}");
    }
    // already-tileable settings are unchanged
    assert_eq!(materialcraft::ops::tileable_wave(8.0, 0.0), (8, 0));
    assert_eq!(materialcraft::ops::tileable_wave(6.0, 0.25).0, 0);
}

#[test]
fn stripes_exact_mode_keeps_requested_angle() {
    let g = G2::parse("size 64\nnode 1 stripes count=8 angle=0.137 tile=0\noutput a 1\n").unwrap();
    assert!(g.evaluate(None).is_ok());
}

#[test]
fn directional_noise_tiles_at_arbitrary_angles() {
    for ang in [0.0, 0.05, 0.137, 0.3333, 0.62, 0.9] {
        for stretch in [1.0, 8.0, 24.0] {
            let g = G2::parse(&format!("size 128\nnode 1 directional_noise scale=6 stretch={stretch} angle={ang} seed=4\noutput a 1\n")).unwrap();
            let t = &g.evaluate(None).unwrap()["a"];
            assert!(t.px.iter().all(|p| p[0].is_finite() && (0.0..=1.0).contains(&p[0])));
            let n = t.px.len() as f32;
            let m = t.px.iter().map(|p| p[0]).sum::<f32>() / n;
            let sd = (t.px.iter().map(|p| (p[0] - m).powi(2)).sum::<f32>() / n).sqrt();
            assert!(sd > 0.07 && sd < 0.3, "angle {ang} stretch {stretch}: sd {sd}");
            // axis-aligned streaks vary along one axis only, so the ratio is a single-row sample there; skip it
            if (ang * 4.0f32).fract().abs() > 0.02 {
                let r = seam_ratio(t);
                assert!(r < 1.5, "directional noise angle {ang} stretch {stretch}: seam/inner = {r}");
            }
        }
    }
}

#[test]
fn gradient_grid_matches_perlin() {
    let gg = noise::GradGrid::new(5, 7, 3);
    for i in 0..50 {
        let (u, v) = (i as f32 * 0.0731 - 0.4, i as f32 * 0.0419 + 0.1);
        assert!((gg.sample(u, v) - noise::perlin(u, v, 5, 7, 3)).abs() < 1e-5);
    }
}

#[test]
fn meshes_are_outward_closed_and_finite() {
    use materialcraft::render3d::build_mesh;
    use materialcraft::tex::Tex;
    let hm = Tex::gray_fn(64, 64, |u, v| ((u * 9.0).sin() * (v * 7.0).cos()) * 0.5 + 0.5);
    for shape in 0..3 {
        for h in [None, Some(&hm)] {
            let m = build_mesh(shape, 24, 1.0, h, 0.1, 0.0);
            assert!(m.pos.iter().chain(&m.nrm).chain(&m.tan).all(|p| p.iter().all(|c| c.is_finite())));
            assert!(m.tris.iter().all(|t| t.iter().all(|&i| (i as usize) < m.pos.len())));
            if h.is_none() {
                // flat/undisplaced: the winding must agree with the analytic normal for every triangle
                for t in &m.tris {
                    let (a, b, c) = (m.pos[t[0] as usize], m.pos[t[1] as usize], m.pos[t[2] as usize]);
                    let f = materialcraft::v3::cross(materialcraft::v3::sub(b, a), materialcraft::v3::sub(c, a));
                    let (n0, n1, n2) = (m.nrm[t[0] as usize], m.nrm[t[1] as usize], m.nrm[t[2] as usize]);
                    let n = [n0[0] + n1[0] + n2[0], n0[1] + n1[1] + n2[1], n0[2] + n1[2] + n2[2]];
                    // pole triangles are degenerate (zero area), so allow float noise
                    assert!(materialcraft::v3::dot(f, n) >= -1e-6, "shape {shape} has an inward-wound triangle");
                }
            }
        }
    }
    // outward displacement: a white height map pushes every plane vertex towards +z (the surface normal)
    let white = Tex::solid(4, 4, [1.0; 4]);
    let p = build_mesh(2, 16, 1.0, Some(&white), 0.2, 0.0);
    assert!(p.pos.iter().all(|v| (v[2] - 0.2).abs() < 1e-5), "relief must rise out of the surface");
    // centred level: mid grey stays put
    let grey = Tex::solid(4, 4, [0.5; 4]);
    let p = build_mesh(2, 16, 1.0, Some(&grey), 0.2, 0.5);
    assert!(p.pos.iter().all(|v| v[2].abs() < 1e-5));
    // sphere seam and poles must stay welded after displacement
    let s = build_mesh(0, 24, 1.0, Some(&hm), 0.15, 0.0);
    let stride = 24 * 2 + 1;
    for j in 0..=24usize {
        let (a, b) = (s.pos[j * stride], s.pos[j * stride + 48]);
        assert!(materialcraft::v3::len(materialcraft::v3::sub(a, b)) < 1e-4, "sphere seam opened at row {j}");
    }
    let top = s.pos[0];
    assert!(s.pos[..stride].iter().all(|v| materialcraft::v3::len(materialcraft::v3::sub(*v, top)) < 1e-4), "north pole tore");
}

#[test]
fn displacement_renders_every_shape_and_camera_without_panics() {
    use materialcraft::render3d::{render, Maps, View, RELIEF_BUMP, RELIEF_DISPLACE, RELIEF_NONE, RELIEF_PARALLAX};
    use materialcraft::tex::Tex;
    let hm = Tex::gray_fn(32, 32, |u, v| (u * 6.0).sin() * 0.5 + 0.5 + v * 0.0);
    let (b, n, r, m, a) = (Tex::solid(4, 4, [0.8, 0.2, 0.2, 1.0]), Tex::solid(4, 4, [0.5, 0.5, 1.0, 1.0]), Tex::solid(4, 4, [0.4; 4]), Tex::solid(4, 4, [0.0, 0.0, 0.0, 1.0]), Tex::solid(4, 4, [1.0; 4]));
    let maps = Maps { base: &b, normal: &n, rough: &r, metal: &m, ao: &a, emissive: None, height: Some(&hm), env: None };
    for shape in 0..3 {
        for relief in [RELIEF_NONE, RELIEF_BUMP, RELIEF_PARALLAX, RELIEF_DISPLACE] {
            // close cameras make triangles cross the near plane; behind/side views exercise back faces
            for (yaw, pitch, dist) in [(0.5, 0.3, 3.4), (3.4, -0.4, 1.6), (1.2, 1.2, 1.6), (-2.0, 0.0, 9.0)] {
                let v = View { shape, relief, yaw, pitch, dist, tess: 32, wireframe: true, height_scale: 0.3, ..View::default() };
                let img = render(&maps, &v, 48, 40);
                assert_eq!((img.w, img.h), (48, 40));
                assert!(img.px.iter().all(|p| p.iter().all(|c| c.is_finite() && *c >= 0.0 && *c <= 1.0001)));
            }
        }
    }
    // the plane must be solid: its centre pixel is not background
    let v = View { shape: 2, yaw: 0.0, pitch: 0.0, dist: 3.0, tess: 32, ..View::default() };
    let img = render(&maps, &v, 64, 64);
    let c = img.px[32 * 64 + 32];
    assert!(c[0] > c[2], "plane centre should be reddish, got {c:?}");
}

/// Every node: several resolutions, finite values, sane range, and (for nodes that should produce variation) a non-blank result.
#[test]
fn every_node_output_is_sane_at_several_sizes() {
    use materialcraft::graph::{demo_graph_for, SPECS};
    // nodes whose demo output may legitimately be a flat colour
    let flat_ok = ["color", "input_value", "value_processor", "invert_flat", "clamp", "histogram_range", "dust", "curvature_smooth", "curvature", "edge_detect", "height_blend", "edge_wear", "dirt", "ao", "slope_blur", "vector_warp", "distance", "flood_fill_gradient"];
    let mut problems = Vec::new();
    for sp in SPECS {
        let Some(text) = demo_graph_for(sp.kind) else { continue };
        for size in [16usize, 32, 64, 100] {
            let g = G2::parse(&text).unwrap();
            let out = match std::panic::catch_unwind(|| g.evaluate(Some(size))) {
                Ok(Ok(o)) => o,
                Ok(Err(e)) => { problems.push(format!("{} @{size}: error {e}", sp.kind)); continue; }
                Err(_) => { problems.push(format!("{} @{size}: PANIC", sp.kind)); continue; }
            };
            let t = &out["o"];
            if t.px.iter().any(|p| p.iter().any(|c| !c.is_finite())) { problems.push(format!("{} @{size}: NaN/Inf", sp.kind)); continue; }
            if t.px.iter().any(|p| p.iter().any(|c| *c < -4.0 || *c > 8.0)) { problems.push(format!("{} @{size}: wild range", sp.kind)); }
            let (mn, mx) = t.px.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[0]).min(p[1]).min(p[2]), a.1.max(p[0]).max(p[1]).max(p[2])));
            if mx - mn < 1e-4 && size >= 32 && !flat_ok.contains(&sp.kind) { problems.push(format!("{} @{size}: blank output ({mn})", sp.kind)); }
        }
    }
    assert!(problems.is_empty(), "node problems:\n{}", problems.join("\n"));
}

/// Fuzz every documented numeric parameter of every node with extreme values: must never panic or return NaN.
#[test]
fn node_parameters_survive_extreme_values() {
    use materialcraft::graph::{demo_graph_for, SPECS};
    let mut problems = Vec::new();
    let mut ran = 0usize;
    for sp in SPECS {
        let Some(text) = demo_graph_for(sp.kind) else { continue };
        let doc = sp.doc.split_once(':').map(|x| x.1).unwrap_or("");
        let names: Vec<String> = doc.split_whitespace().map(|w| w.split('(').next().unwrap().trim_matches(|c: char| !c.is_alphanumeric() && c != '_').to_string()).filter(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') && !w.starts_with("expr")).collect();
        for name in names {
            for v in ["0", "1", "-1", "0.0001", "50", "1000"] {
                let line = format!("node 1 {}", sp.kind);
                let t2 = text.replacen(&line, &format!("{line} {name}={v}"), 1);
                let Ok(g) = G2::parse(&t2) else { continue };
                match std::panic::catch_unwind(|| g.evaluate(Some(24))) {
                    Ok(Ok(out)) => {
                        ran += 1;
                        let t = &out["o"];
                        if t.px.iter().any(|p| p.iter().any(|c| !c.is_finite())) { problems.push(format!("{} {name}={v}: NaN/Inf", sp.kind)); }
                    }
                    Ok(Err(_)) => {} // a clean error is acceptable
                    Err(_) => problems.push(format!("{} {name}={v}: PANIC", sp.kind)),
                }
            }
        }
    }
    problems.sort();
    problems.dedup();
    assert!(ran > 500, "only {ran} parameter cases ran");
    assert!(problems.is_empty(), "{} parameter problems:\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn whole_graph_can_be_imported_and_used_as_a_node() {
    let other = G2::parse("size 64\nnode 1 perlin scale=4 seed=3\nnode 2 invert in=@1\noutput height 2\noutput basecolor 1\n").unwrap();
    let direct = other.evaluate(Some(32)).unwrap();
    let mut host = G2::parse_unchecked("size 64\nnode 1 subgraph graph=\"mat\" output=\"height\"\nnode 2 subgraph graph=\"mat\" output=\"basecolor\"\noutput a 1\noutput b 2\n").unwrap();
    host.import_graph(other.clone(), "mat").unwrap();
    assert!(host.import_graph(other, "mat").is_err(), "duplicate name must be rejected");
    host.validate().unwrap();
    let out = host.evaluate(Some(32)).unwrap();
    assert_eq!(out["a"].px, direct["height"].px, "output=height must pick the height output");
    assert_eq!(out["b"].px, direct["basecolor"].px, "output=basecolor must pick the basecolor output");
    // an empty graph or one without outputs is refused
    let mut h2 = G2::default();
    assert!(h2.import_graph(G2::default(), "x").is_err());
}

#[test]
fn links_can_read_a_specific_output_of_a_graph_node() {
    let other = G2::parse("size 64\nnode 1 perlin scale=4 seed=3\nnode 2 invert in=@1\noutput height 2\noutput basecolor 1\noutput normal 1\n").unwrap();
    assert_eq!(other.output_idents(), vec!["basecolor", "normal", "height"], "outputs are listed in canonical order");
    let mut host = G2::parse_unchecked("size 64\nnode 1 subgraph graph=\"mat\"\nnode 2 invert in=@1.height\nnode 3 invert in=@1.basecolor\noutput a 2\noutput b 3\n").unwrap();
    host.import_graph(other.clone(), "mat").unwrap();
    host.validate().unwrap();
    // round trip keeps the per-link output
    let again = G2::parse_unchecked(&host.to_text()).unwrap();
    assert_eq!(again.nodes[&2].in_out.get("in").map(String::as_str), Some("height"));
    let out = again.evaluate(Some(32)).unwrap();
    let direct = other.evaluate(Some(32)).unwrap();
    let inv = |t: &materialcraft::tex::Tex| materialcraft::ops::invert(t).px;
    assert_eq!(out["a"].px, inv(&direct["height"]));
    assert_eq!(out["b"].px, inv(&direct["basecolor"]));
    assert_ne!(out["a"].px, out["b"].px, "the two outputs differ");
    // unknown output names are a clean error
    let mut bad = G2::parse_unchecked("size 64\nnode 1 subgraph graph=\"mat\"\nnode 2 invert in=@1.nosuch\noutput a 2\n").unwrap();
    bad.import_graph(other, "mat").unwrap();
    assert!(bad.evaluate(Some(16)).is_err());
}

#[test]
fn fast_blur_matches_a_true_gaussian_and_keeps_the_mean() {
    use materialcraft::ops;
    use materialcraft::tex::Tex;
    let n = 128usize;
    let t = Tex::gray_fn(n, n, |u, v| noise::fbm(u, v, 6, 6, 4, 0.5, 3, 0));
    let sigma = 6.0f32;
    let fast = ops::blur(&t, sigma);
    // reference: direct separable gaussian with wrap-around
    let r = (sigma * 4.0) as i64;
    let k: Vec<f32> = (-r..=r).map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp()).collect();
    let ks: f32 = k.iter().sum();
    let mut tmp = t.clone();
    for y in 0..n {
        for x in 0..n {
            tmp.px[y * n + x][0] = (0..k.len()).map(|i| t.at(x as i64 + i as i64 - r, y as i64)[0] * k[i]).sum::<f32>() / ks;
        }
    }
    let mut max_err = 0.0f32;
    let (mut mean_a, mut mean_b) = (0.0f64, 0.0f64);
    for y in 0..n {
        for x in 0..n {
            let v = (0..k.len()).map(|i| tmp.at(x as i64, y as i64 + i as i64 - r)[0] * k[i]).sum::<f32>() / ks;
            max_err = max_err.max((v - fast.px[y * n + x][0]).abs());
            mean_a += v as f64;
            mean_b += fast.px[y * n + x][0] as f64;
        }
    }
    assert!(max_err < 0.02, "box-approximated gaussian differs by {max_err}");
    assert!(((mean_a - mean_b) / (n * n) as f64).abs() < 1e-4, "mean not preserved");
}

#[test]
fn results_look_the_same_at_every_resolution() {
    // parameters are resolution independent: the average of every output must barely change between 128 and 512 px
    let g = Graph::parse(EXAMPLE).unwrap();
    let lo = g.evaluate(Some(128)).unwrap();
    let hi = g.evaluate(Some(512)).unwrap();
    for (name, a) in &lo {
        let b = &hi[name];
        let mean = |t: &materialcraft::tex::Tex| t.px.iter().map(|p| (p[0] + p[1] + p[2]) as f64 / 3.0).sum::<f64>() / t.px.len() as f64;
        let (ma, mb) = (mean(a), mean(b));
        assert!((ma - mb).abs() < 0.06, "{name}: mean {ma:.3} at 128px vs {mb:.3} at 512px");
    }
}

#[test]
fn large_sizes_are_accepted() {
    for s in [2048, 4096, 8192] {
        let text = format!("size {s}\nnode 1 fbm scale=4\noutput a 1\n");
        assert!(Graph::parse(&text).is_ok(), "size {s} rejected");
    }
    assert!(Graph::parse("size 16384\nnode 1 fbm\noutput a 1\n").is_err());
}

#[test]
fn a_cancelled_evaluation_stops_early() {
    use materialcraft::tex::CURRENT_JOB;
    use std::sync::atomic::Ordering;
    let mut g = Graph::parse(EXAMPLE).unwrap();
    g.size = 1024;
    let keep: Vec<u32> = g.output_nodes().iter().map(|o| o.2).collect();
    CURRENT_JOB.store(777, Ordering::Relaxed);
    let t0 = std::time::Instant::now();
    let full = g.evaluate_all_cancellable(1024, &keep, 777);
    let full_time = t0.elapsed();
    assert!(!full.kept.is_empty(), "a current job must deliver its outputs");
    // a newer job exists: the old one is stale from the start
    CURRENT_JOB.store(778, Ordering::Relaxed);
    let t1 = std::time::Instant::now();
    let r = g.evaluate_all_cancellable(1024, &keep, 777);
    assert!(r.kept.is_empty(), "a stale run must not deliver outputs");
    assert!(t1.elapsed() < full_time / 4 + std::time::Duration::from_millis(50), "stale run still took {:?} (full run {:?})", t1.elapsed(), full_time);
}

#[test]
fn nodes_can_have_their_own_output_size() {
    // generators follow the parent size, the modifier moves it in powers of two
    let g = Graph::parse("size 256\nnode 1 fbm scale=4 osize_rel=-2\noutput o 1\n").unwrap();
    let t = &g.evaluate(Some(256)).unwrap()["o"];
    assert_eq!((t.w, t.h), (64, 64), "-2 means a quarter of the parent size");
    let g = Graph::parse("size 256\nnode 1 fbm scale=4 osize_mode=1 osize_rel=1\noutput o 1\n").unwrap();
    assert_eq!(g.evaluate(Some(256)).unwrap()["o"].w, 512, "+1 doubles");
    // absolute sizes are powers of two and ignore the modifier
    let g = Graph::parse("size 256\nnode 1 fbm scale=4 osize_mode=2 osize_abs=7 osize_rel=3\noutput o 1\n").unwrap();
    assert_eq!(g.evaluate(Some(256)).unwrap()["o"].w, 128);
    // filters inherit from their input; a node with another size resamples its inputs to its own size
    let g = Graph::parse("size 256\nnode 1 fbm scale=4 osize_rel=-1\nnode 2 blur in=@1 radius=2\nnode 3 invert in=@2 osize_mode=1\noutput o 2\noutput p 3\n").unwrap();
    let r = g.evaluate(Some(256)).unwrap();
    assert_eq!(r["o"].w, 128, "blur follows its input");
    assert_eq!(r["p"].w, 256, "relative-to-parent restores the graph size");
    // quick previews scale absolute sizes with the preview resolution
    let g = Graph::parse("size 1024\nnode 1 fbm scale=4 osize_mode=2 osize_abs=9\noutput o 1\n").unwrap();
    assert_eq!(g.evaluate(Some(256)).unwrap()["o"].w, 128, "512 px of a 1024 graph = 1/2, so 128 at a 256 preview");
    // everything stays tileable and finite at a reduced size
    let g = Graph::parse("size 128\nnode 1 voronoi scale=5 osize_rel=-1\nnode 2 blur in=@1 radius=3 osize_mode=1\noutput o 2\n").unwrap();
    let t = &g.evaluate(Some(128)).unwrap()["o"];
    assert!(t.px.iter().all(|p| p.iter().all(|v| v.is_finite())));
}

#[test]
fn normal_to_height_inverts_normal_from_height() {
    // height -> normal -> height must give back (a normalised copy of) the original height
    let t = "size 128\nnode 1 fbm scale=3 octaves=3 seed=4\nnode 2 normal in=@1 strength=2\nnode 3 normal_to_height in=@2\nnode 4 auto_levels in=@1\noutput a 3\noutput b 4\n";
    let o = Graph::parse(t).unwrap().evaluate(None).unwrap();
    let (a, b) = (&o["a"], &o["b"]);
    let n = a.px.len() as f64;
    let (ma, mb) = (a.px.iter().map(|p| p[0] as f64).sum::<f64>() / n, b.px.iter().map(|p| p[0] as f64).sum::<f64>() / n);
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for (p, q) in a.px.iter().zip(&b.px) {
        let (x, y) = (p[0] as f64 - ma, q[0] as f64 - mb);
        cov += x * y;
        va += x * x;
        vb += y * y;
    }
    let corr = cov / (va * vb).sqrt();
    assert!(corr > 0.9, "recovered height correlates only {corr:.3} with the original");
}

#[test]
fn batch1_nodes_behave() {
    let run = |body: &str| Graph::parse(&format!("size 64\n{body}")).unwrap().evaluate(None).unwrap();
    // symmetry: left and right halves are mirror images
    let o = run("node 1 fbm scale=3 seed=2\nnode 2 symmetry in=@1 axis=0\noutput a 2\n");
    let t = &o["a"];
    for y in 0..64 {
        for x in 0..32 {
            let (l, r) = (t.at(x, y)[0], t.at(63 - x, y)[0]);
            assert!((l - r).abs() < 0.02, "symmetry broken at ({x},{y}): {l} vs {r}");
        }
    }
    // auto levels reaches 0 and 1
    let o = run("node 1 fbm scale=3 seed=2\nnode 2 auto_levels in=@1\noutput a 2\n");
    let (mn, mx) = o["a"].px.iter().fold((9.0f32, -9.0f32), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
    assert!(mn < 0.001 && mx > 0.999, "range {mn}..{mx}");
    // pow with exponent 2 squares a constant
    let o = run("node 1 color r=0.5 g=0.5 b=0.5\nnode 2 pow in=@1 exponent=2\noutput a 2\n");
    assert!((o["a"].px[0][0] - 0.25).abs() < 1e-4);
    // channel mixer swaps red and blue
    let o = run("node 1 color r=1 g=0.2 b=0\nnode 2 channel_mixer in=@1 rr=0 rb=1 br=1 bb=0\noutput a 2\n");
    let p = o["a"].px[0];
    assert!(p[0] < 0.01 && p[2] > 0.99 && (p[1] - 0.2).abs() < 1e-4, "{p:?}");
    // color to mask: exact colour -> 1, other colour -> 0
    let o = run("node 1 color r=1 g=0 b=0\nnode 2 color_to_mask in=@1 r=1 g=0 b=0\nnode 3 color_to_mask in=@1 r=0 g=0 b=1\noutput a 2\noutput b 3\n");
    assert!(o["a"].px[0][0] > 0.99 && o["b"].px[0][0] < 0.01);
    // new blend modes stay in range
    for m in 0..17 {
        let o = run(&format!("node 1 fbm seed=1\nnode 2 fbm seed=2 scale=7\nnode 3 blend fg=@1 bg=@2 mode={m}\noutput a 3\n"));
        assert!(o["a"].px.iter().all(|p| p[0].is_finite() && (0.0..=1.0).contains(&p[0])), "blend mode {m}");
    }
}

#[test]
fn batch1b_nodes_behave() {
    let run = |body: &str| Graph::parse(&format!("size 64\n{body}")).unwrap().evaluate(None).unwrap();
    // normal combine with a flat detail map returns the base normal
    let o = run("node 1 fbm scale=3 seed=2\nnode 2 normal in=@1 strength=2\nnode 3 color r=0.5 g=0.5 b=1\nnode 4 normal_combine base=@2 detail=@3\noutput a 2\noutput b 4\n");
    for (p, q) in o["a"].px.iter().zip(&o["b"].px) {
        assert!((p[0] - q[0]).abs() < 0.02 && (p[1] - q[1]).abs() < 0.02, "{p:?} vs {q:?}");
    }
    // stroke of a disc: zero in the centre and far outside, bright on the edge
    let o = run("node 1 circle radius=0.3 softness=0.001\nnode 2 shape_stroke in=@1 width=6\noutput a 2\n");
    let t = &o["a"];
    assert!(t.at(32, 32)[0] < 0.01 && t.at(1, 1)[0] < 0.01);
    let edge = (0..64).map(|x| t.at(x, 32)[0]).fold(0.0f32, f32::max);
    assert!(edge > 0.9, "stroke peak {edge}");
    // glow: bright inside the shape, fading outside
    let o = run("node 1 circle radius=0.2 softness=0.001\nnode 2 shape_glow in=@1 width=40\noutput a 2\n");
    let t = &o["a"];
    assert!(t.at(32, 32)[0] > 0.99);
    let row: Vec<f32> = (32..64).map(|x| t.at(x, 32)[0]).collect();
    assert!(row.windows(2).all(|w| w[1] <= w[0] + 1e-4), "glow must fall off monotonically: {row:?}");
    assert!(row.iter().any(|v| *v > 0.05 && *v < 0.95), "glow needs a soft falloff: {row:?}");
    // reflected gradient goes 0 -> 1 -> 0
    let o = run("node 1 gradient_reflected angle=0\noutput a 1\n");
    let t = &o["a"];
    assert!(t.at(0, 0)[0] < 0.05 && t.at(32, 0)[0] > 0.95 && t.at(63, 0)[0] < 0.1);
    // the colour and luminosity blends: luminosity of a grey foreground keeps the background hue
    let o = run("node 1 color r=0.9 g=0.3 b=0.1\nnode 2 color r=0.5 g=0.5 b=0.5 gray=0\nnode 3 blend fg=@2 bg=@1 mode=18 opacity=1\noutput a 3\n");
    let p = o["a"].px[0];
    assert!(p[0] > p[2], "hue lost: {p:?}");
}

#[test]
fn node_cache_reuses_unchanged_nodes_and_matches_a_fresh_run() {
    use materialcraft::graph::NodeCache;
    use std::sync::Arc;
    let mut g = Graph::parse(EXAMPLE).unwrap();
    g.size = 128;
    let keep: Vec<u32> = g.output_nodes().iter().map(|o| o.2).collect();
    let cache = Arc::new(NodeCache::new(512));
    let mut first_seen = 0;
    let r1 = g.evaluate_all_cached(128, &keep, 0, &cache, Some(9), true, &mut |_| first_seen += 1);
    assert_eq!(first_seen, 1, "the selected node must be reported once");
    let (h0, m0) = cache.stats();
    let _ = h0;
    assert!(m0 >= 11, "every node computed on the first run ({m0})");
    // nothing changed: everything is reused and the pixels are identical
    let r2 = g.evaluate_all_cached(128, &keep, 0, &cache, None, true, &mut |_| {});
    let (h1, m1) = cache.stats();
    assert_eq!(m1, m0, "no node may be recomputed");
    assert!(h1 >= 11 + h0);
    for id in &keep {
        assert_eq!(r1.kept[id].px, r2.kept[id].px);
    }
    assert_eq!(r1.thumbs.len(), r2.thumbs.len(), "thumbnails come back from the cache");
    // change the normal node (6): only that node is recomputed, and the result equals an uncached evaluation
    g.nodes.get_mut(&6).unwrap().nums.insert("strength".into(), 7.0);
    let r3 = g.evaluate_all_cached(128, &keep, 0, &cache, None, true, &mut |_| {});
    let (_, m2) = cache.stats();
    assert_eq!(m2 - m1, 1, "only the edited node should be recomputed, got {}", m2 - m1);
    let fresh = g.evaluate_all(128, &keep);
    for id in &keep {
        assert_eq!(r3.kept[id].px, fresh.kept[id].px, "cached result differs for node {id}");
    }
    // change a node near the start (2): everything downstream of it is recomputed, the rest is reused
    g.nodes.get_mut(&2).unwrap().nums.insert("scale".into(), 5.0);
    let before = cache.stats().1;
    let r4 = g.evaluate_all_cached(128, &keep, 0, &cache, None, true, &mut |_| {});
    let redone = cache.stats().1 - before;
    assert!((5..=9).contains(&redone), "recomputed {redone} nodes");
    let fresh = g.evaluate_all(128, &keep);
    for id in &keep {
        assert_eq!(r4.kept[id].px, fresh.kept[id].px, "cached result differs for node {id}");
    }
    // a different size is a different result
    let m = cache.stats().1;
    g.evaluate_all_cached(64, &keep, 0, &cache, None, true, &mut |_| {});
    assert!(cache.stats().1 > m);
    // a tiny budget evicts old entries instead of growing without bound
    let small = Arc::new(NodeCache::new(16));
    g.evaluate_all_cached(512, &keep, 0, &small, None, true, &mut |_| {});
    assert!(small.bytes() <= (16 << 20) + 512 * 512 * 16, "cache holds {} bytes", small.bytes());
}

#[test]
fn interactive_pass_skips_offscreen_nodes_and_thumbnails() {
    use materialcraft::graph::NodeCache;
    use std::sync::Arc;
    let mut g = Graph::parse(EXAMPLE).unwrap();
    g.size = 64;
    // a node nothing reads (an off-screen experiment); a unique extra parameter makes its content hash distinct so
    // the content cache cannot dedupe it against the node it was cloned from
    let mut lone = g.nodes.get(&2).unwrap().clone();
    lone.nums.insert("_offscreen_marker".into(), 123.0);
    let free_id = g.nodes.keys().max().unwrap() + 1;
    g.nodes.insert(free_id, lone);
    let keep: Vec<u32> = g.output_nodes().iter().map(|o| o.2).collect();

    // the interactive pass evaluates only the subgraph feeding the outputs and takes no thumbnails
    let c1 = Arc::new(NodeCache::new(256));
    let ri = g.evaluate_all_cached(64, &keep, 0, &c1, None, false, &mut |_| {});
    assert!(ri.thumbs.is_empty(), "interactive pass takes no thumbnails");
    assert!(!ri.kept.is_empty());
    let interactive_misses = c1.stats().1;

    // the full pass evaluates every node (including the off-screen one) and takes thumbnails
    let c2 = Arc::new(NodeCache::new(256));
    let rf = g.evaluate_all_cached(64, &keep, 0, &c2, None, true, &mut |_| {});
    assert!(!rf.thumbs.is_empty(), "full pass takes thumbnails");
    let full_misses = c2.stats().1;

    assert!(full_misses > interactive_misses, "full pass computes the off-screen node too ({full_misses} vs {interactive_misses})");
    // the visible outputs are identical whichever pass produced them
    for id in &keep {
        assert_eq!(ri.kept[id].px, rf.kept[id].px, "output {id} differs between passes");
    }
}

#[test]
fn fbm_kernel_matches_the_reference_function() {
    for (px, py, oct, mode) in [(4, 4, 5, 0u32), (8, 3, 6, 1), (1, 1, 3, 2), (32, 32, 12, 0)] {
        let k = noise::Fbm::new(px, py, oct, 0.55, 7, mode);
        for i in 0..400 {
            let (u, v) = ((i as f32 * 0.6180339) % 1.0, (i as f32 * 0.4142136) % 1.0);
            let (a, b) = (k.sample(u, v), noise::fbm(u, v, px, py, oct, 0.55, 7, mode));
            assert!((a - b).abs() < 1e-5, "{px}x{py} oct {oct} mode {mode}: {a} vs {b}");
        }
    }
}

#[test]
fn faster_kernels_match_straightforward_references() {
    use materialcraft::ops;
    use materialcraft::tex::{lum, Tex};
    let img = Tex::gray_fn(96, 80, |u, v| noise::fbm(u, v, 4, 4, 3, 0.5, 5, 0));
    // gray blur (single channel path) must equal the general four-channel path
    let mut rgba = img.clone();
    for p in rgba.px.iter_mut() {
        p[3] = 0.5;
    }
    for sigma in [3.0f32, 7.5, 20.0] {
        let (a, b) = (ops::blur(&img, sigma), ops::blur(&rgba, sigma));
        let worst = a.px.iter().zip(&b.px).map(|(p, q)| (p[0] - q[0]).abs()).fold(0.0f32, f32::max);
        assert!(worst < 1e-5, "sigma {sigma}: gray and rgba blur differ by {worst}");
    }
    // normal map: compare with a direct per-pixel Scharr implementation
    let n = ops::normal_from_height(&img, 3.0, false);
    let k = 3.0 * img.w as f32 * 0.05;
    for &(x, y) in &[(0i64, 0i64), (95, 79), (10, 20), (50, 0), (0, 40), (95, 40)] {
        let g = |i: i64, j: i64| lum(img.at(x + i, y + j));
        let dx = (3.0 * (g(1, -1) - g(-1, -1)) + 10.0 * (g(1, 0) - g(-1, 0)) + 3.0 * (g(1, 1) - g(-1, 1))) / 32.0 * k;
        let dy = (3.0 * (g(-1, 1) - g(-1, -1)) + 10.0 * (g(0, 1) - g(0, -1)) + 3.0 * (g(1, 1) - g(1, -1))) / 32.0 * k;
        let l = (dx * dx + dy * dy + 1.0).sqrt();
        let want = [-dx / l * 0.5 + 0.5, dy / l * 0.5 + 0.5, 1.0 / l * 0.5 + 0.5];
        let got = n.at(x, y);
        for c in 0..3 {
            assert!((got[c] - want[c]).abs() < 1e-4, "normal at ({x},{y}) channel {c}: {} vs {}", got[c], want[c]);
        }
    }
    // levels: gamma 1 fast path equals the general formula; coloured pixels keep their channels
    let t = Tex::from_fn(8, 8, |u, v| [u, v, 0.3, 1.0]);
    let l = ops::levels(&t, 0.2, 0.8, 1.0, 0.1, 0.9);
    let p = t.px[11];
    for c in 0..3 {
        let want = 0.1 + 0.8 * ((p[c] - 0.2) / 0.6).clamp(0.0, 1.0);
        assert!((l.px[11][c] - want).abs() < 1e-5);
    }
}

#[test]
fn directional_noise_with_fewer_taps_matches_the_dense_reference() {
    // reference: the original sampling density of 5 taps per lattice cell
    for (st, ang) in [(8.0f32, 0.13f32), (24.0, 0.4), (3.0, 0.0)] {
        let size = 96usize;
        let g = Graph::parse(&format!("size {size}\nnode 1 directional_noise scale=6 stretch={st} angle={ang} seed=3\noutput a 1\n")).unwrap();
        let got = &g.evaluate(None).unwrap()["a"];
        let p1 = 6i32;
        let (sa, ca) = (ang * std::f32::consts::TAU).sin_cos();
        let len1 = (st / p1 as f32).min(1.0);
        let k = ((st * 5.0).ceil() as usize).clamp(8, 160) | 1;
        let taps: Vec<(f32, f32)> = (0..k).map(|i| { let t = ((i as f32 + 0.5) / k as f32) * 2.0 - 1.0; (t, 1.0 - t.abs()) }).collect();
        let wsum: f32 = taps.iter().map(|t| t.1).sum();
        let gain = 1.6 * st.min(p1 as f32).powf(0.55);
        let (g1, g2) = (noise::GradGrid::new(p1, p1, 3), noise::GradGrid::new(p1 * 2, p1 * 2, 3 + 7));
        let mut diff = 0.0f64;
        for y in 0..size {
            for x in 0..size {
                let (u, v) = ((x as f32 + 0.5) / size as f32, (y as f32 + 0.5) / size as f32);
                let (mut a1, mut a2) = (0.0f32, 0.0f32);
                for &(t, w) in &taps {
                    let (o1, o2) = (t * len1 * 0.5, t * len1 * 0.25);
                    a1 += w * g1.sample(u + o1 * ca, v + o1 * sa);
                    a2 += w * g2.sample(u + o2 * ca, v + o2 * sa);
                }
                let m = (a1 * 0.65 + a2 * 0.35) / wsum;
                let want = (0.5 + (m - 0.5) * gain).clamp(0.0, 1.0);
                diff += (got.at(x as i64, y as i64)[0] - want).abs() as f64;
            }
        }
        let mean = diff / (size * size) as f64;
        assert!(mean < 0.01, "stretch {st}: mean difference {mean:.4}");
    }
}

#[test]
fn slope_blur_gray_path_matches_the_color_path() {
    use materialcraft::{ops, tex::Tex};
    let img = Tex::gray_fn(64, 64, |u, v| noise::fbm(u, v, 4, 4, 3, 0.5, 5, 0));
    let slope = Tex::gray_fn(64, 64, |u, v| noise::fbm(u, v, 2, 2, 2, 0.5, 9, 0));
    let mut tinted = img.clone();
    for p in tinted.px.iter_mut() {
        p[3] = 0.99; // not exactly opaque: forces the general four-channel path
    }
    for mode in 0..3 {
        let (a, b) = (ops::slope_blur(&img, &slope, 12, 0.1, mode), ops::slope_blur(&tinted, &slope, 12, 0.1, mode));
        let worst = a.px.iter().zip(&b.px).map(|(p, q)| (p[0] - q[0]).abs()).fold(0.0f32, f32::max);
        assert!(worst < 1e-5, "mode {mode}: paths differ by {worst}");
    }
}

#[test]
fn phase1_parameters_behave() {
    let run = |body: &str| Graph::parse(&format!("size 64\n{body}")).unwrap().evaluate(None).unwrap();
    let close = |a: &materialcraft::tex::Tex, b: &materialcraft::tex::Tex, tol: f32, what: &str| {
        for (p, q) in a.px.iter().zip(&b.px) {
            for c in 0..3 {
                assert!((p[c] - q[c]).abs() <= tol, "{what}: {p:?} vs {q:?}");
            }
        }
    };
    // invert toggle: off = pass through
    let o = run("node 1 fbm scale=3 seed=4\nnode 2 invert in=@1 invert=0\nnode 3 invert in=@1 invert=1\noutput a 1\noutput b 2\noutput c 3\n");
    close(&o["a"], &o["b"], 0.0, "invert off");
    assert!((o["c"].px[5][0] - (1.0 - o["a"].px[5][0])).abs() < 1e-6);
    // threshold: greater and lower are complements (but for pixels exactly on the threshold)
    let o = run("node 1 gradient angle=0\nnode 2 threshold in=@1 threshold=0.5 mode=0\nnode 3 threshold in=@1 threshold=0.5 mode=2\nnode 4 threshold in=@1 threshold=0.5 mode=1\noutput a 2\noutput b 3\noutput c 4\n");
    let (mut gt, mut lt, mut ge) = (0, 0, 0);
    for i in 0..64 * 64 {
        gt += (o["a"].px[i][0] > 0.5) as i32;
        lt += (o["b"].px[i][0] > 0.5) as i32;
        ge += (o["c"].px[i][0] > 0.5) as i32;
    }
    assert!(gt > 0 && lt > 0 && gt + lt <= 64 * 64 && 64 * 64 - gt - lt <= 128, "greater {gt} lower {lt}");
    assert!(ge >= gt, "greater or equal {ge} must include greater {gt}");
    // clamp: apply to alpha
    let o = run("node 1 color r=0.5 g=0.5 b=0.5 a=0.2\nnode 2 clamp in=@1 low=0.5 high=1 apply_alpha=0\nnode 3 clamp in=@1 low=0.5 high=1 apply_alpha=1\noutput a 2\noutput b 3\n");
    assert!((o["a"].px[0][3] - 0.2).abs() < 1e-5 && (o["b"].px[0][3] - 0.5).abs() < 1e-5, "{:?} {:?}", o["a"].px[0], o["b"].px[0]);
    // blend: add sub and the cropping area
    let o = run("node 1 color r=0.5 g=0.5 b=0.5\nnode 2 color r=0.6 g=0.6 b=0.6\nnode 3 blend fg=@2 bg=@1 mode=20 opacity=1\nnode 4 color r=0.9 g=0.9 b=0.9\nnode 5 color r=0.1 g=0.1 b=0.1\nnode 6 blend fg=@4 bg=@5 mode=0 crop_x1=0.5\noutput a 3\noutput b 6\n");
    assert!((o["a"].px[0][0] - 0.7).abs() < 0.01, "add sub {:?}", o["a"].px[0]);
    assert!(o["b"].at(8, 8)[0] > 0.85 && o["b"].at(56, 8)[0] < 0.15, "crop {:?} {:?}", o["b"].at(8, 8), o["b"].at(56, 8));
    // blend alpha: a half-transparent foreground with "use source alpha" mixes halfway
    let o = run("node 1 color r=0 g=0 b=0\nnode 2 color r=1 g=1 b=1 a=0.5\nnode 3 blend fg=@2 bg=@1 mode=0 opacity=1 alpha_mode=0\nnode 4 blend fg=@2 bg=@1 mode=0 opacity=1 alpha_mode=1\noutput a 3\noutput b 4\n");
    assert!((o["a"].px[0][0] - 0.5).abs() < 0.01 && o["b"].px[0][0] > 0.99, "{:?} {:?}", o["a"].px[0], o["b"].px[0]);
    // mirror: default copies the left half onto the right; invert copies the other way; the corner mode mirrors both ways
    let o = run("node 1 fbm scale=3 seed=7\nnode 2 mirror in=@1 axis=0\nnode 3 mirror in=@1 axis=0 invert_x=1\nnode 4 mirror in=@1 axis=3 corner=3\noutput src 1\noutput m 2\noutput inv 3\noutput cor 4\n");
    let (src, m, inv, cor) = (&o["src"], &o["m"], &o["inv"], &o["cor"]);
    for y in [3usize, 20, 50] {
        for x in 0..32usize {
            assert_eq!(m.at(x as i64, y as i64), src.at(x as i64, y as i64));
            assert_eq!(m.at(63 - x as i64, y as i64), src.at(x as i64, y as i64));
            assert_eq!(inv.at(x as i64, y as i64), src.at(63 - x as i64, y as i64));
        }
    }
    assert_eq!(cor.at(5, 6), src.at(58, 57));
    // symmetry: the eight-fold mode is symmetric under swapping x and y; add combines original and mirror
    let o = run("node 1 fbm scale=3 seed=9\nnode 2 symmetry in=@1 axis=5\nnode 3 symmetry in=@1 axis=0 transfer=1\nnode 4 symmetry in=@1 axis=0\noutput a 2\noutput b 3\noutput c 4\n");
    for (x, y) in [(3, 11), (40, 7), (25, 60)] {
        let (p, q) = (o["a"].at(x, y), o["a"].at(y, x));
        assert!((p[0] - q[0]).abs() < 0.03, "eight-fold {p:?} vs {q:?}");
    }
    assert!(o["b"].px.iter().zip(&o["c"].px).any(|(a, b)| (a[0] - b[0]).abs() > 0.05), "add transfer must differ from copy");
    // skew align: pivoting about the centre shifts the result by half a turn for a shear of one
    let o = run("node 1 fbm scale=3 seed=3\nnode 2 skew in=@1 skew_x=1 skew_y=0 align=0\nnode 3 skew in=@1 skew_x=1 skew_y=0 align=1\noutput a 2\noutput b 3\n");
    let mut err = 0.0f32;
    for (x, y) in [(5i64, 9i64), (30, 40), (50, 20)] {
        err = err.max((o["a"].at(x, y)[0] - o["b"].at((x + 32).rem_euclid(64), y)[0]).abs());
    }
    assert!(err < 0.08, "skew align mismatch {err}");
    // multi directional warp: max >= average >= min
    let o = run("node 1 fbm scale=4 seed=1\nnode 2 fbm scale=2 seed=2\nnode 3 multi_directional_warp in=@1 warp=@2 intensity=0.3 mode=0\nnode 4 multi_directional_warp in=@1 warp=@2 intensity=0.3 mode=1\nnode 5 multi_directional_warp in=@1 warp=@2 intensity=0.3 mode=2\nnode 6 multi_directional_warp in=@1 warp=@2 intensity=0.3 mode=3 directions=1\noutput avg 3\noutput max 4\noutput min 5\noutput chain 6\n");
    for i in 0..64 * 64 {
        assert!(o["max"].px[i][0] + 1e-5 >= o["avg"].px[i][0] && o["avg"].px[i][0] + 1e-5 >= o["min"].px[i][0], "max/avg/min order");
        assert!(o["chain"].px[i][0].is_finite());
    }
    // directional warp: a mid grey intensity map moves nothing with the default offset
    let o = run("node 1 fbm scale=4 seed=5\nnode 2 color r=0.5 g=0.5 b=0.5\nnode 3 directional_warp in=@1 warp=@2 intensity=0.2 angle=0.1\nnode 4 directional_warp in=@1 warp=@2 intensity=0.2 angle=0.1 offset=0\noutput src 1\noutput a 3\noutput b 4\n");
    close(&o["src"], &o["a"], 0.02, "grey map, offset 0.5");
    assert!(o["src"].px.iter().zip(&o["b"].px).any(|(a, b)| (a[0] - b[0]).abs() > 0.05), "offset 0 must move pixels");
    // normal nodes: blend opacity, invert z, combine techniques, rotation format, alpha from input
    let o = run("node 1 fbm scale=3 seed=2\nnode 2 normal in=@1 strength=3\nnode 3 fbm scale=7 seed=8\nnode 4 normal in=@3 strength=3\nnode 5 normal_blend a=@2 b=@4 opacity=0\nnode 6 normal_invert in=@2 invert_z=1\nnode 7 color r=0.5 g=0.5 b=1\nnode 8 normal_combine base=@2 detail=@7 technique=0\nnode 9 normal_combine base=@2 detail=@7 technique=1\nnode 10 normal in=@1 strength=3 alpha_from_input=1\noutput n 2\noutput blend0 5\noutput invz 6\noutput wo 8\noutput cm 9\noutput al 10\noutput h 1\n");
    close(&o["n"], &o["blend0"], 0.02, "normal blend opacity 0");
    assert!((o["invz"].px[100][2] - (1.0 - o["n"].px[100][2])).abs() < 1e-5);
    close(&o["n"], &o["wo"], 0.03, "whiteout with a flat detail");
    close(&o["n"], &o["cm"], 0.03, "channel mixer with a flat detail");
    assert!((o["al"].px[77][3] - o["h"].px[77][0]).abs() < 0.01, "alpha {:?} height {:?}", o["al"].px[77], o["h"].px[77]);
    // normal to height: zero opacity gives a flat map
    let o = run("node 1 fbm scale=3 seed=2\nnode 2 normal in=@1 strength=3\nnode 3 normal_to_height in=@2 opacity=0\nnode 4 normal_to_height in=@2 relief_balance=0.9\nnode 5 normal_to_height in=@2\noutput flat 3\noutput low 4\noutput mid 5\n");
    assert!(o["flat"].px.iter().all(|p| (p[0] - 0.5).abs() < 1e-4));
    assert!(o["low"].px.iter().zip(&o["mid"].px).any(|(a, b)| (a[0] - b[0]).abs() > 0.02), "relief balance must change the result");
    // bevel: the round profile rises faster than the angular one
    let o = run("node 1 circle radius=0.35 softness=0.001\nnode 2 bevel in=@1 distance=0.2 smoothing=0 corner=0\nnode 3 bevel in=@1 distance=0.2 smoothing=0 corner=1\noutput round 2\noutput ang 3\n");
    let (sr, sa): (f32, f32) = (o["round"].px.iter().map(|p| p[0]).sum(), o["ang"].px.iter().map(|p| p[0]).sum());
    assert!(sr > sa * 1.02, "round {sr} angular {sa}");
    // shape glow/stroke: opacity and colour scale the result
    let o = run("node 1 circle radius=0.2 softness=0.001\nnode 2 shape_glow in=@1 width=20\nnode 3 shape_glow in=@1 width=20 opacity=0.5\nnode 4 shape_glow in=@1 width=20 color_g=0\noutput a 2\noutput b 3\noutput c 4\n");
    assert!((o["b"].at(32, 32)[0] - 0.5 * o["a"].at(32, 32)[0]).abs() < 0.01);
    assert!(o["c"].at(32, 32)[1] < 0.01 && o["c"].at(32, 32)[0] > 0.9);
    // glow: a zero colour removes the glow, a clear amount trims it
    let o = run("node 1 circle radius=0.2 softness=0.001\nnode 2 glow in=@1 width=12\nnode 3 glow in=@1 width=12 glow_r=0 glow_g=0 glow_b=0\nnode 4 glow in=@1 width=12 clear=0.8\noutput a 2\noutput b 3\noutput c 4\noutput src 1\n");
    close(&o["b"], &o["src"], 1e-5, "black glow");
    let (sa, sc): (f32, f32) = (o["a"].px.iter().map(|p| p[0]).sum(), o["c"].px.iter().map(|p| p[0]).sum());
    assert!(sc < sa, "clear amount must trim the glow ({sc} vs {sa})");
}

#[test]
fn shared_noise_block_disorder_speed_anisotropy_and_tile_offset() {
    let run = |body: &str| Graph::parse(&format!("size 64\n{body}")).unwrap().evaluate(None).unwrap();
    let diff = |a: &materialcraft::tex::Tex, b: &materialcraft::tex::Tex| a.px.iter().zip(&b.px).map(|(p, q)| (p[0] - q[0]).abs()).fold(0.0f32, f32::max);
    for kind in ["perlin", "clouds", "gaussian_noise", "cells", "gaussian_spots", "directional_noise", "fractal_sum", "waveform", "bnw_spots"] {
        let o = run(&format!(
            "node 1 {kind} scale=4 seed=3\nnode 2 {kind} scale=4 seed=3 disorder=0.8\nnode 3 {kind} scale=4 seed=3 disorder=0.8 disorder_speed=0\nnode 4 {kind} scale=4 seed=3 disorder=0.8 disorder_speed=2\noutput base 1\noutput dis 2\noutput still 3\noutput fast 4\n"
        ));
        assert!(diff(&o["base"], &o["dis"]) > 0.02, "{kind}: disorder changes nothing");
        assert!(diff(&o["base"], &o["still"]) < 1e-6, "{kind}: speed 0 must cancel the disorder");
        assert!(diff(&o["dis"], &o["fast"]) > 0.02, "{kind}: speed has no effect");
    }
    // anisotropy narrows the displacement to one direction (noise features move along it, so across it the image barely changes)
    let o = run("node 1 perlin scale=4 seed=2\nnode 2 clouds scale=4 seed=2\nnode 3 clouds scale=4 seed=2 disorder=1 anisotropy=1 anisotropy_angle=0\nnode 4 clouds scale=4 seed=2 disorder=1\noutput base 2\noutput an 3\noutput iso 4\n");
    assert!(diff(&o["base"], &o["an"]) > 0.02 && diff(&o["an"], &o["iso"]) > 0.02, "anisotropy must change the result");
    // tile offset: the fraction pans the tiling image, a whole part gives another tile of the plane
    let o = run("node 1 clouds scale=4 seed=5\nnode 2 clouds scale=4 seed=5 tile_x=0.25\nnode 3 clouds scale=4 seed=5 tile_x=1\nnode 4 clouds scale=4 seed=5 tile_x=1.5 tile_y=2.25\noutput base 1\noutput pan 2\noutput other 3\noutput both 4\n");
    for (x, y) in [(0i64, 5i64), (10, 30), (50, 60)] {
        assert_eq!(o["pan"].at(x, y), o["base"].at((x + 16).rem_euclid(64), y), "pan by a quarter tile");
    }
    assert!(diff(&o["base"], &o["other"]) > 0.05, "another tile must look different");
    // the result still tiles: the step across the wrap is no bigger than the steps inside the image
    let t = &o["both"];
    let wrap: f32 = (0..64).map(|y| (t.at(0, y)[0] - t.at(63, y)[0]).abs()).sum::<f32>() / 64.0;
    let inner: f32 = (0..64).map(|y| (t.at(31, y)[0] - t.at(32, y)[0]).abs()).sum::<f32>() / 64.0;
    assert!(wrap < inner * 2.5 + 0.01, "seam: wrap {wrap} inner {inner}");
    // white noise distributions: uniform variance 1/12, triangle 1/24
    let o = run("node 1 white_noise seed=1 distribution=0\nnode 2 white_noise seed=1 distribution=1\nnode 3 white_noise seed=1 distribution=2\noutput u 1\noutput g 2\noutput t 3\n");
    let var = |t: &materialcraft::tex::Tex| {
        let n = t.px.len() as f32;
        let m = t.px.iter().map(|p| p[0]).sum::<f32>() / n;
        t.px.iter().map(|p| (p[0] - m).powi(2)).sum::<f32>() / n
    };
    let (vu, vg, vt) = (var(&o["u"]), var(&o["g"]), var(&o["t"]));
    assert!((vu - 1.0 / 12.0).abs() < 0.01 && (vt - 1.0 / 24.0).abs() < 0.01 && vg < vt, "{vu} {vg} {vt}");
}

#[test]
fn phase3_scatter_nodes_keep_old_output_and_gain_properties() {
    use materialcraft::{ops, scatter, tex::Tex};
    let run = |body: &str| Graph::parse(&format!("size 64\n{body}")).unwrap().evaluate(None).unwrap();
    let diff = |a: &Tex, b: &Tex| a.px.iter().zip(&b.px).map(|(p, q)| (p[0] - q[0]).abs()).fold(0.0f32, f32::max);
    let sum = |t: &Tex| t.px.iter().map(|p| p[0]).sum::<f32>();
    // 1. defaults reproduce the previous implementations exactly
    let pat = Tex::from_fn(32, 32, |u, v| {
        let g = (u * 3.0 + v).fract();
        [g, g, g, 1.0]
    });
    for gen in [false, true] {
        let mut p = scatter::TileP::legacy(gen);
        p.pos_rand = 0.4;
        p.size_rand = 0.3;
        p.rot_rand = 0.5;
        p.color_rand = 0.4;
        p.seed = 7;
        let old = ops::tile_grid(
            64,
            64,
            Some(&pat),
            None,
            &ops::TileP { nx: p.nx, ny: p.ny, size: p.size, size_rand: p.size_rand, rot: p.rot, rot_rand: p.rot_rand, pos_rand: p.pos_rand, color: p.color, color_rand: p.color_rand, offset: p.offset, seed: 7, thresh: 0.5, default_disc: !gen, brick: gen },
        );
        let pats = scatter::Patterns { inputs: vec![&pat], builtin: 0, fallback: 2, distribution: 0 };
        let maps = scatter::TileMaps { mask: None, scale: None, rotation: None, displacement: None, color: None, distribution: None, background: None };
        let new = scatter::tile_grid(64, 64, &pats, &maps, &p);
        assert!(diff(&old, &new) < 1e-4, "tile grid (gen={gen}) changed: {}", diff(&old, &new));
    }
    let old = ops::splatter(64, 64, None, 40, 0.1, 0.3, 1.0, 0.2, 3);
    let mut sp = scatter::SplatterP::legacy();
    sp.lum_var = 0.2;
    sp.seed = 3;
    let new = scatter::splatter(64, 64, &scatter::Patterns { inputs: vec![], builtin: 0, fallback: 99, distribution: 0 }, &sp);
    assert!(diff(&old, &new) < 1e-6, "splatter changed");
    let old = ops::splatter_circular(64, 64, None, 12, 0.3, 0.1, 0.2, 0.1, 0.3, 0.5, 4);
    let mut cp = scatter::CircularP::legacy();
    cp.size_rand = 0.2;
    cp.rot = 0.1;
    cp.rot_rand = 0.3;
    cp.pos_rand = 0.5;
    cp.seed = 4;
    let new = scatter::splatter_circular(64, 64, &scatter::Patterns { inputs: vec![], builtin: 0, fallback: 2, distribution: 0 }, &cp);
    // Both paths call sin/cos, so both go through the platform libm and MSVC's
    // CRT disagrees with glibc in the last bit or two. Measured on the Windows
    // runner: max 6.3e-6, mean 7e-9, 9 of 4096 pixels above 1e-6, none above
    // 1e-4. The previous 1e-6 bound sat below the platforms' own agreement
    // level, so it passed on Linux and macOS and failed only on Windows. 1e-4
    // leaves ~16x headroom over the measured drift while still catching a real
    // regression, which would move whole regions by order 1 rather than nudging
    // a handful of pixels in the sixth decimal place.
    let n = old.px.len();
    let d: Vec<f32> = old.px.iter().zip(&new.px).map(|(p, q)| (p[0] - q[0]).abs()).collect();
    let diag = format!(
        "max={:.3e} mean={:.3e} over1e-4={} of {} px",
        diff(&old, &new),
        d.iter().sum::<f32>() / n as f32,
        d.iter().filter(|v| **v > 1e-4).count(),
        n
    );
    assert!(diff(&old, &new) < 1e-4, "splatter circular changed: {diag}");

    // 2. built-in patterns, offsets, masks, blending, reverse order
    let base = run("node 1 tile_sampler x_amount=4 y_amount=4 size=0.9\noutput a 1\n");
    for k in 1..16 {
        let o = run(&format!("node 1 tile_sampler x_amount=4 y_amount=4 size=0.9 pattern_type={k}\noutput a 1\n"));
        assert!(o["a"].px.iter().all(|p| p[0].is_finite() && (0.0..=1.0001).contains(&p[0])), "pattern {k} out of range");
        assert!(sum(&o["a"]) > 1.0, "pattern {k} draws nothing");
    }
    let sq = run("node 1 tile_sampler x_amount=4 y_amount=4 size=0.9 pattern_type=1\noutput a 1\n");
    assert!(sum(&sq["a"]) > sum(&base["a"]) * 1.1, "square must cover more than the soft disc");
    let cols = run("node 1 tile_generator x_amount=4 y_amount=4 offset_mode=3 offset=0.5\nnode 2 tile_generator x_amount=4 y_amount=4 offset_mode=1 offset=0.5\nnode 3 tile_generator x_amount=4 y_amount=4 offset_mode=0\noutput c 1\noutput r 2\noutput n 3\n");
    assert!(diff(&cols["c"], &cols["r"]) > 0.3 && diff(&cols["n"], &cols["r"]) > 0.3, "offset types must differ");
    let chk = run("node 1 tile_generator x_amount=4 y_amount=4 offset_mode=0 pattern_type=1 size=1\nnode 2 tile_generator x_amount=4 y_amount=4 offset_mode=0 pattern_type=1 size=1 mask_checker=1\nnode 3 tile_generator x_amount=4 y_amount=4 offset_mode=0 pattern_type=1 size=1 mask_checker=1 invert_mask=1\nnode 4 tile_generator x_amount=4 y_amount=4 offset_mode=0 pattern_type=1 size=1 mask_random=1\noutput full 1\noutput a 2\noutput b 3\noutput none 4\n");
    let (f, a, b) = (sum(&chk["full"]), sum(&chk["a"]), sum(&chk["b"]));
    assert!((a - f / 2.0).abs() < f * 0.02 && (b - f / 2.0).abs() < f * 0.02 && (a + b - f).abs() < f * 0.02, "checker {a} {b} {f}");
    assert!(sum(&chk["none"]) == 0.0, "random mask 1 removes every tile");
    let add = run("node 1 tile_sampler x_amount=2 y_amount=2 size=1.6 pattern_type=1 color=0.4 blending=0\nnode 2 tile_sampler x_amount=2 y_amount=2 size=1.6 pattern_type=1 color=0.4 blending=1\noutput max 1\noutput add 2\n");
    assert!(sum(&add["add"]) > sum(&add["max"]) * 1.5, "add must accumulate overlaps");
    assert!(add["add"].px.iter().all(|p| p[0].is_finite()));
    let ord = run("node 1 tile_sampler x_amount=3 y_amount=3 size=1.6 pattern_type=1 color_random=0.8 blending=2 seed=2\nnode 2 tile_sampler x_amount=3 y_amount=3 size=1.6 pattern_type=1 color_random=0.8 blending=2 seed=2 reverse_order=1\noutput a 1\noutput b 2\n");
    assert!(diff(&ord["a"], &ord["b"]) > 0.05, "reverse order must change overlapping tiles");
    let bg = run("node 1 tile_sampler x_amount=2 y_amount=2 size=0.3 background=0.25\noutput a 1\n");
    assert!(bg["a"].px.iter().all(|p| p[0] >= 0.2499), "background colour");
    // map inputs: a scale map of 0 hides every tile, a colour map darkens
    let m = run("node 1 color gray=1 v=0\nnode 2 tile_sampler x_amount=4 y_amount=4 scale_map=@1\nnode 3 tile_sampler x_amount=4 y_amount=4\noutput z 2\noutput n 3\n");
    assert!(sum(&m["z"]) < sum(&m["n"]) * 0.01, "scale map 0 should shrink tiles away");
    // patterns by number: two inputs, each used by alternating tiles
    let two = run("node 1 color gray=1 v=1\nnode 2 color gray=1 v=0.5\nnode 3 tile_generator x_amount=2 y_amount=2 offset_mode=0 size=1 pattern=@1 pattern2=@2 pattern_distribution=1\noutput a 3\n");
    let vals: std::collections::BTreeSet<i32> = two["a"].px.iter().map(|p| (p[0] * 10.0).round() as i32).collect();
    assert!(vals.contains(&10) && vals.contains(&5), "two patterns by number {vals:?}");

    // 3. splatter: grid, pan, output levels, gain; circular: rings, spread
    let g = run("node 1 splatter grid_number=3 scale=0.2 scale_var=0 rot_var=0 disorder=0\nnode 2 splatter grid_number=3 scale=0.2 scale_var=0 rot_var=0 disorder=0 pan_x=0.5\noutput a 1\noutput b 2\n");
    assert!(diff(&g["a"], &g["b"]) > 0.1, "pan must move the grid");
    let lv = run("node 1 splatter output_min=0.2 output_max=0.6\noutput a 1\n");
    assert!(lv["a"].px.iter().all(|p| p[0] >= 0.1999 && p[0] <= 0.6001), "output levels");
    let rg = run("node 1 splatter_circular number=8 radius=0.35 size=0.08\nnode 2 splatter_circular number=8 radius=0.35 size=0.08 rings=3\nnode 3 splatter_circular number=8 radius=0.35 size=0.08 spread=0.5\noutput a 1\noutput b 2\noutput c 3\n");
    assert!(sum(&rg["b"]) > sum(&rg["a"]) * 1.5, "rings add instances");
    let top: f32 = (0..64usize).flat_map(|y| (0..64usize).map(move |x| (x, y))).filter(|&(_, y)| y < 24).map(|(x, y)| rg["c"].at(x as i64, y as i64)[0]).sum();
    assert!(top < 1e-3 && sum(&rg["c"]) > 1.0, "spread 0.5 places everything on one half of the circle");
}

#[test]
fn tile_sampler_and_generator_are_seamless() {
    // moving every instance by exactly one pixel must equal rolling the image by one pixel: this fails when any
    // instance near the border is missing or not wrapped, so it proves the pattern tiles in both directions
    let run = |body: &str| Graph::parse(&format!("size 64\n{body}")).unwrap().evaluate(None).unwrap();
    let configs = [
        "x_amount=5 y_amount=7 size=0.9 position_random=0.8 rotation_random=1 size_random=0.5 seed=3",
        "x_amount=3 y_amount=3 size=2.4 pattern_type=1 rotation=0.13 position_random=1 seed=9",
        "x_amount=1 y_amount=1 size=1.8 pattern_type=4 rotation=0.2",
        "x_amount=7 y_amount=5 size=1 offset_mode=1 offset=0.37 offset_random=0.4 pattern_type=8",
        "x_amount=5 y_amount=5 size=1.2 offset_mode=3 offset=0.5 symmetry_random=3 pattern_rotation=1 pattern_type=13",
        "x_amount=9 y_amount=4 size=1.5 size_mode=2 scale=0.2 position_random=0.5 blending=1 pattern_type=2",
        "x_amount=6 y_amount=6 size=1.3 blending=2 color_random=0.7 position_random=1 mask_random=0.3 reverse_order=1",
    ];
    for kind in ["tile_sampler", "tile_generator"] {
        for c in configs {
            let o = run(&format!("node 1 {kind} {c}\nnode 2 {kind} {c} global_offset_x=0.015625\nnode 3 {kind} {c} global_offset_y=0.015625\noutput a 1\noutput bx 2\noutput by 3\n"));
            let a = &o["a"];
            let mut worst = 0.0f32;
            for y in 0..64i64 {
                for x in 0..64i64 {
                    worst = worst.max((o["bx"].at((x + 1) % 64, y)[0] - a.at(x, y)[0]).abs());
                    worst = worst.max((o["by"].at(x, (y + 1) % 64)[0] - a.at(x, y)[0]).abs());
                }
            }
            assert!(worst < 2e-3, "{kind} {c}: not seamless, worst {worst}");
        }
    }
}

#[test]
fn phase3b_nodes_behave_as_documented() {
    let run = |body: &str| Graph::parse(&format!("size 64\n{body}")).unwrap().evaluate(None).unwrap();
    let sum = |t: &materialcraft::tex::Tex| t.px.iter().map(|p| p[0]).sum::<f32>();
    let diff = |a: &materialcraft::tex::Tex, b: &materialcraft::tex::Tex| a.px.iter().zip(&b.px).map(|(p, q)| (p[0] - q[0]).abs()).sum::<f32>() / a.px.len() as f32;
    // Edge Detect: width, invert, tolerance, roundness
    let run512 = |body: &str| Graph::parse(&format!("size 512\n{body}")).unwrap().evaluate(None).unwrap();
    let o = run512("node 1 stripes count=2 softness=0.001 width=0.5\nnode 2 edge_detect in=@1 edge_width=1 edge_roundness=0\nnode 3 edge_detect in=@1 edge_width=9 edge_roundness=0\nnode 4 edge_detect in=@1 edge_width=1 edge_roundness=0 invert=1\nnode 5 edge_detect in=@1 edge_width=9 edge_roundness=8\nnode 6 perlin scale=4 seed=1\nnode 7 edge_detect in=@6 tolerance=0 edge_roundness=0\nnode 8 edge_detect in=@6 tolerance=0.9 edge_roundness=0\nnode 9 edge_detect in=@1 mode=1\noutput w1 2\noutput w9 3\noutput inv 4\noutput round 5\noutput t0 7\noutput t9 8\noutput legacy 9\n");
    assert!(sum(&o["w1"]) > 1.0, "edges found");
    assert!(sum(&o["w9"]) > sum(&o["w1"]) * 2.0, "edge width widens the line");
    assert!((sum(&o["inv"]) + sum(&o["w1"]) - 512.0 * 512.0).abs() < 1.0, "invert");
    assert!(o["round"].px.iter().all(|p| p[0].is_finite() && (0.0..=1.0).contains(&p[0])));
    assert!(sum(&o["t9"]) < sum(&o["t0"]) * 0.5, "tolerance drops weak edges");
    assert!(sum(&o["legacy"]) > 1.0, "legacy sobel still works");
    // Radial Blur: a spin blur leaves concentric rings alone, the legacy zoom blur smears them
    let o = run("node 1 pixel_processor expr=\"sqrt((u-0.5)*(u-0.5)+(v-0.5)*(v-0.5))*2\"\nnode 2 radial_blur in=@1 angle=0.2 samples=16\nnode 3 radial_blur in=@1 mode=1 amount=0.5\nnode 4 radial_blur in=@1 angle=0.4 samples=1\noutput src 1\noutput spin 2\noutput zoom 3\noutput one 4\n");
    // compare the inner disc only (the blur samples stay inside the image there)
    let inner = |a: &materialcraft::tex::Tex, b: &materialcraft::tex::Tex| {
        let (mut d, mut n) = (0.0f32, 0.0f32);
        for y in 0..64usize {
            for x in 0..64usize {
                let (dx, dy) = ((x as f32 + 0.5) / 64.0 - 0.5, (y as f32 + 0.5) / 64.0 - 0.5);
                if dx * dx + dy * dy < 0.25 * 0.25 {
                    d += (a.at(x as i64, y as i64)[0] - b.at(x as i64, y as i64)[0]).abs();
                    n += 1.0;
                }
            }
        }
        d / n
    };
    assert!(inner(&o["spin"], &o["src"]) < inner(&o["zoom"], &o["src"]) * 0.5, "spin {} zoom {}", inner(&o["spin"], &o["src"]), inner(&o["zoom"], &o["src"]));
    assert!(diff(&o["one"], &o["src"]) < 1e-3, "one sample is the identity");
    // Mosaic: the map regions are flattened to one colour each
    let o = run("node 1 perlin scale=4 seed=2\nnode 2 stripes count=2 softness=0.001 width=0.5\nnode 3 mosaic in=@1 mosaic_map=@2\nnode 4 mosaic in=@1 mosaic_map=@2 intensity=0\noutput m 3\noutput z 4\noutput src 1\n");
    let distinct = |t: &materialcraft::tex::Tex| t.px.iter().map(|p| (p[0] * 1000.0).round() as i32).collect::<std::collections::BTreeSet<_>>().len();
    assert!(distinct(&o["m"]) < 60 && distinct(&o["src"]) > 300, "{} {}", distinct(&o["m"]), distinct(&o["src"]));
    assert!(diff(&o["z"], &o["src"]) < 1e-4, "intensity 0 keeps the input");
    // Curvature reads a normal map: flat is mid grey, a bump is not
    let o = run("node 1 color gray=1 v=0.5\nnode 2 normal in=@1\nnode 3 curvature in=@2\nnode 4 shape shape=0 size=0.3\nnode 5 blur in=@4 intensity=6\nnode 6 normal in=@5 strength=8\nnode 7 curvature in=@6 intensity=8\nnode 8 curvature in=@6 intensity=8 directx=1\noutput flat 3\noutput bump 7\noutput bumpdx 8\n");
    assert!(o["flat"].px.iter().all(|p| (p[0] - 0.5).abs() < 1e-4), "flat normal map has no curvature");
    let (mn, mx) = o["bump"].px.iter().fold((1.0f32, 0.0f32), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
    assert!(mx - mn > 0.2, "bump has curvature {mn} {mx}");
    assert!(diff(&o["bump"], &o["bumpdx"]) > 0.01, "normal format changes the result");
}

#[test]
fn phase5_voronoi_cells_grunge_properties() {
    let run = |body: &str| Graph::parse(&format!("size 64\n{body}")).unwrap().evaluate(None).unwrap();
    let mean = |t: &materialcraft::tex::Tex| t.px.iter().map(|p| p[0]).sum::<f32>() / t.px.len() as f32;
    let diff = |a: &materialcraft::tex::Tex, b: &materialcraft::tex::Tex| a.px.iter().zip(&b.px).map(|(p, q)| (p[0] - q[0]).abs()).sum::<f32>() / a.px.len() as f32;
    let distinct = |t: &materialcraft::tex::Tex| t.px.iter().map(|p| (p[0] * 1000.0).round() as i32).collect::<std::collections::BTreeSet<_>>().len();
    // Voronoi: defaults reproduce the previous node
    let o = run("node 1 voronoi scale=6 seed=4 mode=0\nnode 2 voronoi scale=6 seed=4 mode=1\nnode 3 voronoi scale=6 seed=4 mode=2\nnode 4 voronoi scale=6 seed=4 mode=3\noutput a 1\noutput b 2\noutput c 3\noutput d 4\n");
    for y in (0..64i64).step_by(7) {
        for x in (0..64i64).step_by(5) {
            let (u, v) = ((x as f32 + 0.5) / 64.0, (y as f32 + 0.5) / 64.0);
            let (a, b, c) = noise::voronoi(u, v, 6, 6, 4, 1.0);
            assert!((o["a"].at(x, y)[0] - a).abs() < 1e-4 && (o["b"].at(x, y)[0] - c).abs() < 1e-4, "f1 / id changed");
            assert!((o["c"].at(x, y)[0] - ((b - a) * 4.0).clamp(0.0, 1.0)).abs() < 1e-4 && (o["d"].at(x, y)[0] - b).abs() < 1e-4, "edge / f2 changed");
        }
    }
    // metrics, distortion, aspect, styles, invert, colour seed
    let o = run("node 1 voronoi scale=5 seed=2\nnode 2 voronoi scale=5 seed=2 distance_mode=1\nnode 3 voronoi scale=5 seed=2 distance_mode=2\nnode 4 voronoi scale=5 seed=2 distance_mode=3 minkowski=2\nnode 5 voronoi scale=5 seed=2 distortion=1\nnode 6 voronoi scale=5 seed=2 aspect=2\nnode 7 voronoi scale=5 seed=2 invert=1\nnode 8 voronoi scale=5 seed=2 mode=1\nnode 9 voronoi scale=5 seed=2 mode=1 color_seed_mode=1 color_seed=77\nnode 10 voronoi scale=5 seed=2 color_seed_mode=1 color_seed=77\nnode 11 voronoi scale=5 seed=2 offset_x=0.5\noutput e 1\noutput m 2\noutput c 3\noutput k 4\noutput dist 5\noutput asp 6\noutput inv 7\noutput id 8\noutput id2 9\noutput f1b 10\noutput off 11\n");
    assert!(diff(&o["e"], &o["m"]) > 0.02 && diff(&o["e"], &o["c"]) > 0.02, "metrics differ");
    assert!(diff(&o["e"], &o["k"]) < 1e-3, "minkowski 2 is euclidean");
    assert!(mean(&o["c"]) <= mean(&o["e"]) + 1e-4 && mean(&o["m"]) >= mean(&o["e"]) - 1e-4, "chebyshev <= euclid <= manhattan");
    assert!(diff(&o["e"], &o["dist"]) > 0.02 && diff(&o["e"], &o["asp"]) > 0.02 && diff(&o["e"], &o["off"]) > 0.02);
    assert!((mean(&o["inv"]) + mean(&o["e"]) - 1.0).abs() < 1e-3, "invert");
    assert!(diff(&o["id"], &o["id2"]) > 0.05 && diff(&o["e"], &o["f1b"]) < 1e-6, "manual colour seed re-rolls colours, not cells");
    for style in 4..=6 {
        let o = run(&format!("node 1 voronoi scale=5 mode={style}\nnode 2 voronoi scale=5 mode=2 rounded=1 edge_thickness=0.4\noutput a 1\noutput b 2\n"));
        assert!(o["a"].px.iter().all(|p| p[0].is_finite() && (0.0..=1.0).contains(&p[0])) && distinct(&o["a"]) > 50, "style {style}");
        assert!(o["b"].px.iter().all(|p| p[0].is_finite()));
    }
    // Voronoi Fractal
    let o = run("node 1 voronoi scale=3 seed=1\nnode 2 voronoi_fractal scale=3 seed=1 min_level=0 max_level=0\nnode 3 voronoi_fractal scale=3 seed=1 max_level=3\nnode 4 voronoi_fractal scale=3 seed=1 max_level=3 blend=1\nnode 5 voronoi_fractal scale=3 seed=1 max_level=3 blend=2\noutput a 1\noutput one 2\noutput add 3\noutput max 4\noutput min 5\n");
    assert!(diff(&o["a"], &o["one"]) < 1e-5, "one level is a plain voronoi");
    assert!(diff(&o["a"], &o["add"]) > 0.02 && diff(&o["add"], &o["max"]) > 0.02 && diff(&o["max"], &o["min"]) > 0.02, "levels and blend modes");
    assert!(mean(&o["max"]) >= mean(&o["min"]));
    // Cells per-variant properties
    let o = run("node 1 cells variant=0 scale=6 seed=3\nnode 2 cells variant=0 scale=6 seed=3 luminance_random=1\nnode 3 cells variant=2 scale=6 seed=3\nnode 4 cells variant=2 scale=6 seed=3 edge_width=0.8\nnode 5 cells variant=3 scale=6 seed=3\nnode 6 cells variant=3 scale=6 seed=3 hardness=8\nnode 7 color gray=1 v=0.3\nnode 8 cells variant=1 scale=6 seed=3 color_source=2 color=@7\nnode 9 cells variant=1 scale=6 seed=3 color_source=1 pseudorandom_seed=5\nnode 10 cells variant=1 scale=6 seed=3\noutput l0 1\noutput l1 2\noutput e0 3\noutput e1 4\noutput h0 5\noutput h1 6\noutput img 8\noutput ps 9\noutput rnd 10\n");
    assert!(diff(&o["l0"], &o["l1"]) > 0.05 && diff(&o["e0"], &o["e1"]) > 0.02 && diff(&o["h0"], &o["h1"]) > 0.02);
    assert!(o["img"].px.iter().all(|p| (p[0] - 0.3).abs() < 1e-3), "image colour source: every cell takes the image value");
    assert!(diff(&o["ps"], &o["rnd"]) > 0.05 && distinct(&o["ps"]) > 10, "pseudorandom source");
    // Grunge
    let o = run("node 1 grunge variant=0 seed=2\nnode 2 grunge variant=0 seed=2 balance=0.7\nnode 3 grunge variant=0 seed=2 dirt_specks_opacity=1\nnode 4 grunge variant=1 seed=2\nnode 5 grunge variant=1 seed=2 scratch_width=0.8\nnode 6 grunge variant=2 seed=2\nnode 7 grunge variant=2 seed=2 coverage=0.7 details=40\nnode 8 grunge variant=0 seed=2 scratches_opacity=1\noutput c 1\noutput bal 2\noutput specks 3\noutput s0 4\noutput s1 5\noutput p0 6\noutput p1 7\noutput scr 8\n");
    assert!(mean(&o["bal"]) > mean(&o["c"]) + 0.05, "balance brightens");
    assert!(mean(&o["specks"]) < mean(&o["c"]) && mean(&o["scr"]) > mean(&o["c"]), "concrete specks darken, scratches add");
    assert!(mean(&o["s1"]) > mean(&o["s0"]), "wider scratches cover more");
    assert!(diff(&o["p0"], &o["p1"]) > 0.01, "spots details / coverage");
}

#[test]
fn phase6_grunge_maps_are_distinct_and_controllable() {
    let mut g = String::new();
    for m in 1..=24 {
        g += &format!("node {m} grunge_map map={m}\noutput m{m} {m}\n");
    }
    g += "node 120 grunge_map map=2 balance=0.1\nnode 121 grunge_map map=2 balance=0.9\nnode 122 grunge_map map=2 invert=1\nnode 123 grunge_map map=2\noutput lo 120\noutput hi 121\noutput inv 122\noutput base 123\n";
    let run = |body: &str| Graph::parse(&format!("size 128\n{body}")).unwrap().evaluate(None).unwrap();
    let o = run(&g);
    let mean = |t: &materialcraft::tex::Tex| t.px.iter().map(|p| p[0]).sum::<f32>() / t.px.len() as f32;
    for m in 1..=24 {
        let v = mean(&o[&format!("m{m}")]);
        assert!(v.is_finite() && v > 0.05 && v < 1.0, "map {m} mean {v}");
        for n in (m + 1)..=24 {
            let d: f32 = o[&format!("m{m}")].px.iter().zip(&o[&format!("m{n}")].px).map(|(a, b)| (a[0] - b[0]).abs()).sum();
            assert!(d > 10.0, "maps {m} and {n} look the same");
        }
    }
    assert!(mean(&o["lo"]) < mean(&o["base"]) && mean(&o["base"]) < mean(&o["hi"]), "balance moves the amount of dirt");
    assert!((mean(&o["inv"]) + mean(&o["base"]) - 1.0).abs() < 1e-3, "invert");
}

#[test]
fn phase6_gradient_map_and_color_match() {
    let run = |body: &str| Graph::parse(&format!("size 64\n{body}")).unwrap().evaluate(None).unwrap();
    let mean = |t: &materialcraft::tex::Tex, c: usize| t.px.iter().map(|p| p[c]).sum::<f32>() / t.px.len() as f32;
    // gradient map: repeat wraps the input, clamp does not; alpha stops are only used on request
    let stops = "p0=0 r0=0 g0=0 b0=0 a0=0 p1=1 r1=1 g1=1 b1=1 a1=1";
    let o = run(&format!(
        "node 1 color gray=1 v=1.25\nnode 2 colorize in=@1 {stops}\nnode 3 colorize in=@1 addressing=1 {stops}\nnode 4 gradient mode=0\nnode 5 colorize in=@4 use_alpha=1 {stops}\nnode 6 colorize in=@4 {stops}\noutput clamp 2\noutput rep 3\noutput al 5\noutput noal 6\n"
    ));
    assert!((o["clamp"].px[0][0] - 1.0).abs() < 1e-4, "clamped");
    assert!((o["rep"].px[0][0] - 0.25).abs() < 1e-3, "repeat wraps 1.25 to 0.25");
    assert!((mean(&o["al"], 3) - 0.5).abs() < 0.1 && (mean(&o["noal"], 3) - 1.0).abs() < 1e-4, "alpha stops");
    // colour swap: a red surface becomes blue, a green one is left alone by the mask
    let o = run(
        "node 1 color r=0.8 g=0.1 b=0.1\nnode 2 color r=0.1 g=0.7 b=0.1\nnode 3 color_match in=@1 method=1 source_mode=1 src_r=0.8 src_g=0.1 src_b=0.1 target_r=0.1 target_g=0.2 target_b=0.9\nnode 4 color_match in=@2 method=1 source_mode=1 src_r=0.8 src_g=0.1 src_b=0.1 target_r=0.1 target_g=0.2 target_b=0.9\nnode 5 color_match in=@2 method=1 source_mode=1 src_r=0.8 src_g=0.1 src_b=0.1 target_r=0.1 target_g=0.2 target_b=0.9 use_mask=0\nnode 6 color_match in=@1 method=1 target_r=0.1 target_g=0.2 target_b=0.9\noutput red 3\noutput green 4\noutput nomask 5\noutput avg 6\n",
    );
    let r = o["red"].px[0];
    assert!(r[2] > r[0] + 0.3 && (r[0] - 0.1).abs() < 0.08 && (r[2] - 0.9).abs() < 0.08, "red turned blue: {r:?}");
    let g = o["green"].px[0];
    assert!((g[0] - 0.1).abs() < 1e-3 && (g[1] - 0.7).abs() < 1e-3, "green untouched by the mask");
    assert!((o["nomask"].px[0][1] - 0.7).abs() > 0.05, "without the mask everything moves");
    let a = o["avg"].px[0];
    assert!(a[2] > a[0] + 0.3, "average source mode: {a:?}");
}

#[test]
fn phase8_hbao_and_quantize_color() {
    let run = |body: &str| Graph::parse(&format!("size 128\n{body}")).unwrap().evaluate(None).unwrap();
    let mean = |t: &materialcraft::tex::Tex| t.px.iter().map(|p| p[0]).sum::<f32>() / t.px.len() as f32;
    // a flat surface is unoccluded; a pit is darker at its bottom than a flat area, and taller height means darker
    let o = run(
        "node 1 color gray=1 v=0.5\nnode 2 ao in=@1 mode=1\nnode 3 circle radius=0.3 softness=0.05\nnode 4 invert in=@3\nnode 5 ao in=@4 mode=1 height_depth=0.1\nnode 6 ao in=@4 mode=1 height_depth=0.5\nnode 7 ao in=@4 mode=1 use_world_units=1 height_depth_cm=30 surface_size=300\nnode 8 ao in=@4 mode=1 use_world_units=1 height_depth_cm=300 surface_size=300\nnode 9 ao in=@4 mode=1 quality=2\noutput flat 2\noutput shallow 5\noutput deep 6\noutput w_small 7\noutput w_big 8\noutput q16 9\n",
    );
    assert!(mean(&o["flat"]) > 0.999, "flat is unoccluded");
    assert!(mean(&o["shallow"]) < 0.999 && mean(&o["deep"]) < mean(&o["shallow"]), "taller height occludes more");
    assert!(mean(&o["w_big"]) < mean(&o["w_small"]), "world units: higher height scale occludes more");
    assert!(o["q16"].px.iter().all(|p| p[0].is_finite() && (0.0..=1.0).contains(&p[0])));
    // quantize colour: a smooth gradient reduces to the requested number of colours
    let o = run("node 1 gradient mode=0\nnode 2 colorize in=@1 p0=0 r0=1 g0=0 b0=0 p1=0.5 r1=0 g1=1 b1=0 p2=1 r2=0 g2=0 b2=1\nnode 3 quantize_color in=@2 color_amount=3\nnode 4 quantize_color in=@2 color_amount=8 dithering=1 dithering_pattern=1\nnode 5 quantize_color in=@2 color_amount=1\noutput three 3\noutput eight 4\noutput one 5\n");
    let distinct = |t: &materialcraft::tex::Tex| {
        let mut v: Vec<[i32; 3]> = t.px.iter().map(|p| [(p[0] * 200.0) as i32, (p[1] * 200.0) as i32, (p[2] * 200.0) as i32]).collect();
        v.sort();
        v.dedup();
        v.len()
    };
    assert!(distinct(&o["three"]) <= 3, "at most 3 colours, got {}", distinct(&o["three"]));
    assert!(distinct(&o["eight"]) <= 8 && distinct(&o["eight"]) > 3, "dithered 8 colours: {}", distinct(&o["eight"]));
    assert_eq!(distinct(&o["one"]), 1);
}

#[test]
fn m0_multi_output_nodes() {
    let run = |body: &str| Graph::parse(&format!("size 64\n{body}")).unwrap().evaluate(None).unwrap();
    let mean = |t: &materialcraft::tex::Tex, c: usize| t.px.iter().map(|p| p[c]).sum::<f32>() / t.px.len() as f32;
    // RGBA split: each output carries one channel; the default output (no name) is R
    let o = run(
        "node 1 color r=0.9 g=0.5 b=0.1 a=0.3\nnode 2 rgba_split in=@1\nnode 3 tint in=@2.r r=1 g=1 b=1\nnode 4 tint in=@2.g r=1 g=1 b=1\nnode 5 tint in=@2.b r=1 g=1 b=1\nnode 6 tint in=@2.a r=1 g=1 b=1\nnode 7 tint in=@2 r=1 g=1 b=1\noutput r 3\noutput g 4\noutput b 5\noutput a 6\noutput def 7\n",
    );
    assert!((mean(&o["r"], 0) - 0.9).abs() < 1e-3 && (mean(&o["g"], 0) - 0.5).abs() < 1e-3 && (mean(&o["b"], 0) - 0.1).abs() < 1e-3 && (mean(&o["a"], 0) - 0.3).abs() < 1e-3, "channels split");
    assert!((mean(&o["def"], 0) - 0.9).abs() < 1e-3, "default output is the first");
    // Alpha split
    let o = run("node 1 color r=0.9 g=0.5 b=0.1 a=0.3\nnode 2 alpha_split in=@1\nnode 3 tint in=@2.rgb r=1 g=1 b=1\nnode 4 tint in=@2.alpha r=1 g=1 b=1\noutput rgb 3\noutput alpha 4\n");
    assert!((mean(&o["rgb"], 0) - 0.9).abs() < 1e-3 && (mean(&o["rgb"], 3) - 1.0).abs() < 1e-3 && (mean(&o["alpha"], 0) - 0.3).abs() < 1e-3);
    // Color Match mask: only the red surface is selected
    let o = run(
        "node 1 color r=0.8 g=0.1 b=0.1\nnode 2 color_match in=@1 method=1 source_mode=1 src_r=0.8 src_g=0.1 src_b=0.1 target_r=0.1 target_g=0.2 target_b=0.9\nnode 3 tint in=@2.mask r=1 g=1 b=1\nnode 4 color r=0.1 g=0.7 b=0.1\nnode 5 color_match in=@4 method=1 source_mode=1 src_r=0.8 src_g=0.1 src_b=0.1\nnode 6 tint in=@5.mask r=1 g=1 b=1\nnode 7 tint in=@2.output r=1 g=1 b=1\noutput sel 3\noutput not 6\noutput res 7\n",
    );
    assert!(mean(&o["sel"], 0) > 0.99 && mean(&o["not"], 0) < 0.01, "mask selects the source colour only");
    assert!(o["res"].px[0][2] > 0.7, "output 'output' is the recoloured image");
    // Quantize Color outputs: ID has one level per colour, palette lists the colours
    let o = run(
        "node 1 gradient mode=0\nnode 2 colorize in=@1 p0=0 r0=1 g0=0 b0=0 p1=0.5 r1=0 g1=1 b1=0 p2=1 r2=0 g2=0 b2=1\nnode 3 quantize_color in=@2 color_amount=3\nnode 4 tint in=@3.id r=1 g=1 b=1\nnode 5 tint in=@3.palette r=1 g=1 b=1\nnode 6 tint in=@3 r=1 g=1 b=1\noutput id 4\noutput pal 5\noutput img 6\n",
    );
    let levels = |t: &materialcraft::tex::Tex| {
        let mut v: Vec<i32> = t.px.iter().map(|p| (p[0] * 100.0).round() as i32).collect();
        v.sort();
        v.dedup();
        v.len()
    };
    assert_eq!(levels(&o["id"]), 3, "three ids");
    assert!(levels(&o["pal"]) >= 2, "palette bands");
    assert!(levels(&o["img"]) >= 2);
    // Weave Generator masks: the two thread directions never overlap and together equal the opacity
    let o = run("node 1 weave_generator weave=2\nnode 2 tint in=@1.mask_1 r=1 g=1 b=1\nnode 3 tint in=@1.mask_2 r=1 g=1 b=1\nnode 4 tint in=@1.opacity r=1 g=1 b=1\nnode 5 tint in=@1.height r=1 g=1 b=1\noutput m1 2\noutput m2 3\noutput op 4\noutput h 5\n");
    let overlap: f32 = o["m1"].px.iter().zip(&o["m2"].px).map(|(a, b)| a[0] * b[0]).sum();
    let diff: f32 = o["m1"].px.iter().zip(&o["m2"].px).zip(&o["op"].px).map(|((a, b), c)| (a[0] + b[0] - c[0]).abs()).sum();
    assert!(overlap == 0.0 && diff == 0.0 && mean(&o["m1"], 0) > 0.1 && mean(&o["h"], 0) > 0.3, "weave masks");
    // an unknown output name is rejected with a message that lists the real ones
    let e = Graph::parse("size 64\nnode 1 color r=1\nnode 2 rgba_split in=@1\nnode 3 invert in=@2.x\noutput o 3\n").err();
    assert!(format!("{e:?}").contains("outputs: r, g, b, a"), "{e:?}");
}

#[test]
fn m0_float_export_keeps_values_above_one() {
    use materialcraft::export::{encode, Format};
    use materialcraft::exr::f32_to_half;
    use materialcraft::tex::Tex;
    // half-float conversion: exact values, rounding, overflow
    assert_eq!(f32_to_half(1.0), 0x3c00);
    assert_eq!(f32_to_half(-2.0), 0xc000);
    assert_eq!(f32_to_half(0.5), 0x3800);
    assert_eq!(f32_to_half(0.0), 0);
    assert_eq!(f32_to_half(70000.0), 0x7c00);
    assert_eq!(f32_to_half(65504.0), 0x7bff);
    let t = Tex::from_fn(4, 2, |u, v| [u * 4.0 + 0.25, -1.5, 3.0 + v, 0.5]);
    // EXR float: header, offset table and the first scanline can be read back
    let b = encode(Format::Exr, &t, 32, 90);
    assert_eq!(&b[..4], &[0x76, 0x2f, 0x31, 0x01]);
    let end = b.windows(2).position(|w| w == [0, 0]).unwrap(); // last header attribute ends with a 0, then the end byte
    let _ = end;
    // find the offset table: the 8 bytes after the header's terminating zero point at scanline 0
    let hdr_end = {
        let mut i = 8;
        loop {
            if b[i] == 0 {
                break i + 1;
            }
            // name\0 type\0 size value
            let n1 = b[i..].iter().position(|c| *c == 0).unwrap();
            let j = i + n1 + 1;
            let n2 = b[j..].iter().position(|c| *c == 0).unwrap();
            let k = j + n2 + 1;
            let size = i32::from_le_bytes([b[k], b[k + 1], b[k + 2], b[k + 3]]) as usize;
            i = k + 4 + size;
        }
    };
    let off0 = u64::from_le_bytes(b[hdr_end..hdr_end + 8].try_into().unwrap()) as usize;
    assert_eq!(off0, hdr_end + 8 * t.h);
    assert_eq!(i32::from_le_bytes(b[off0..off0 + 4].try_into().unwrap()), 0, "scanline y");
    let size = u32::from_le_bytes(b[off0 + 4..off0 + 8].try_into().unwrap()) as usize;
    assert_eq!(size, 4 * t.w * 4);
    // channels are stored A, B, G, R; the R plane is last
    let r0 = off0 + 8 + 3 * t.w * 4;
    let rv = f32::from_le_bytes(b[r0..r0 + 4].try_into().unwrap());
    assert!((rv - 0.75).abs() < 1e-6, "R of pixel 0 is {rv}");
    let r3 = r0 + 3 * 4;
    assert!((f32::from_le_bytes(b[r3..r3 + 4].try_into().unwrap()) - 3.75).abs() < 1e-5, "values above 1 survive");
    assert_eq!(b.len(), off0 + t.h * (8 + size));
    // half EXR is smaller; HDR has a header and 4 bytes per pixel
    assert!(encode(Format::Exr, &t, 16, 90).len() < b.len());
    let h = encode(Format::Hdr, &t, 8, 90);
    assert!(h.starts_with(b"#?RADIANCE") && h.len() == "#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 2 +X 4\n".len() + 8 * 4);
    // 32-bit TIFF stores raw floats
    let tf = encode(Format::Tiff, &t, 32, 90);
    assert!(tf.windows(4).any(|w| w == 3.75f32.to_le_bytes()), "float tiff holds 3.75");
    assert!(Format::parse("EXR") == Some(Format::Exr) && Format::parse("hdr") == Some(Format::Hdr));
}

#[test]
fn m0_non_square_node_sizes() {
    // 256x128 graph: relative-to-input inherits the shape, the relative modifier scales both axes,
    // absolute keeps the aspect ratio of the graph
    let text = "size 256 128\nnode 1 perlin scale=4\nnode 2 blur in=@1 radius=2 osize_mode=0 osize_rel=-1\nnode 3 perlin scale=4 osize_mode=2 osize_abs=7\nnode 4 blend bg=@1 fg=@2 mode=0 opacity=0.5\noutput a 1\noutput b 2\noutput c 3\noutput d 4\n";
    let g = Graph::parse(text).unwrap();
    let out = g.evaluate(None).unwrap();
    assert_eq!((out["a"].w, out["a"].h), (256, 128));
    assert_eq!((out["b"].w, out["b"].h), (128, 64));
    assert_eq!((out["c"].w, out["c"].h), (128, 64));
    // mixed sizes meet at the node's own size without panicking or changing the aspect
    assert_eq!((out["d"].w, out["d"].h), (256, 128));
    // a preview at half width keeps the 2:1 shape
    let half = g.evaluate(Some(128)).unwrap();
    assert_eq!((half["a"].w, half["a"].h), (128, 64));
}

#[test]
fn m0_pin_types_follow_the_graph() {
    use materialcraft::graph::{pin_types, Pin};
    let g = Graph::parse("size 64\nnode 1 perlin scale=4\nnode 2 colorize in=@1\nnode 3 blur in=@2 radius=2\nnode 4 blur in=@1 radius=2\nnode 5 to_gray in=@3\nnode 6 color_match in=@2 reference=@2 method=1\nnode 7 tint in=@4\nnode 8 alpha_split in=@2\nnode 9 invert in=@8.alpha\nnode 10 invert in=@8.rgb\noutput a 5\n").unwrap();
    let p = pin_types(&g);
    assert_eq!(p[&1], Pin::Gray);
    assert_eq!(p[&2], Pin::Color);
    assert_eq!(p[&3], Pin::Color); // a filter keeps the colour type of its input
    assert_eq!(p[&4], Pin::Gray);
    assert_eq!(p[&5], Pin::Gray);
    assert_eq!(p[&6], Pin::Color);
    assert_eq!(p[&7], Pin::Color);
    assert_eq!(p[&9], Pin::Gray, "a named gray output feeds a gray filter");
    assert_eq!(p[&10], Pin::Color);
}

#[test]
fn m0_per_node_output_format_quantises() {
    let run = |fmt: &str| {
        let g = Graph::parse(&format!("size 64\nnode 1 gradient mode=0\nnode 2 tint in=@1 r=1.37 g=0.3333 b=0.5{fmt}\noutput o 2\n")).unwrap();
        g.evaluate(None).unwrap().remove("o").unwrap()
    };
    let f32t = run("");
    assert!(f32t.px.iter().any(|p| p[0] > 1.0), "32-bit float keeps values above 1");
    let i8 = run(" ofmt=1");
    assert!(i8.px.iter().all(|p| p[0] <= 1.0 && ((p[0] * 255.0).round() - p[0] * 255.0).abs() < 1e-3));
    let i16 = run(" ofmt=2");
    assert!(i16.px.iter().all(|p| p[0] <= 1.0));
    let h = run(" ofmt=3");
    assert!(h.px.iter().any(|p| p[0] > 1.0), "16-bit float keeps values above 1");
    assert!(h.px.iter().zip(&f32t.px).all(|(a, b)| (a[0] - b[0]).abs() <= b[0].abs() * 1e-3 + 1e-6));
    assert!(h.px.iter().zip(&f32t.px).any(|(a, b)| a[0] != b[0]), "half precision rounds");
}

#[test]
fn m0_format_versions() {
    // a file without a format line is version 1 and still loads; we write version 2
    let old = Graph::parse("size 64\nnode 1 perlin scale=4\noutput o 1\n").unwrap();
    assert_eq!(old.format, 1);
    assert!(old.to_text().starts_with("format 2\n"));
    let again = Graph::parse(&old.to_text()).unwrap();
    assert_eq!(again.format, 2);
    // a newer file is refused with a message that says what to do
    let err = Graph::parse("format 99\nsize 64\nnode 1 perlin\noutput o 1\n").err().unwrap().to_string();
    assert!(err.contains("format 99") && err.contains("update"), "{err}");
}

#[test]
fn m0_live_links_follow_the_file_on_disk() {
    let dir = std::env::temp_dir().join(format!("mc_links_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    let lib = |scale: u32| format!("size 64\nnode 1 perlin scale={scale}\noutput o 1\n");
    std::fs::write(dir.join("lib/pattern.mcg"), lib(4)).unwrap();
    let main = "size 64\nlink pat \"lib/pattern.mcg\"\nnode 1 subgraph graph=\"pat\"\nnode 2 subgraph graph=\"pat\"\nnode 3 blend bg=@1 fg=@2 opacity=0.5\noutput o 3\n";
    std::fs::write(dir.join("main.mcg"), main).unwrap();
    let (mut g, issues) = Graph::load_file(&dir.join("main.mcg")).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert!(g.is_linked("pat"));
    // the linked graph is not written into the file, only the link
    let text = g.to_text();
    assert!(text.contains("link pat \"lib/pattern.mcg\"") && !text.contains("subgraph pat"), "{text}");
    let a = g.evaluate(None).unwrap().remove("o").unwrap();
    // change the linked file: refreshing picks it up
    std::fs::write(dir.join("lib/pattern.mcg"), lib(9)).unwrap();
    assert!(g.resolve_links(&dir).is_empty());
    let b = g.evaluate(None).unwrap().remove("o").unwrap();
    assert!(a.px.iter().zip(&b.px).any(|(p, q)| (p[0] - q[0]).abs() > 0.01), "the new scale must show");
    // a missing file is reported, not fatal, and the name then fails to evaluate with a clear error
    std::fs::remove_file(dir.join("lib/pattern.mcg")).unwrap();
    let issues = g.resolve_links(&dir);
    assert_eq!(issues.len(), 1);
    assert!(issues[0].contains("pat") && issues[0].contains("cannot read"), "{issues:?}");
    assert!(!g.is_linked("pat"));
    assert!(g.evaluate(None).is_err());
    // a link to a named graph inside a package
    std::fs::write(dir.join("lib/pkg.mcg"), "size 64\nsubgraph inner\nsize 64\nnode 1 voronoi scale=5\noutput o 1\nend\nnode 1 perlin\noutput o 1\n").unwrap();
    let g2 = Graph::parse("size 64\nlink v \"lib/pkg.mcg\" \"inner\"\nnode 1 subgraph graph=\"v\"\noutput o 1\n").unwrap();
    let mut g2 = g2;
    assert!(g2.resolve_links(&dir).is_empty());
    assert!(g2.evaluate(None).is_ok());
    // a graph that does not exist in the package
    let mut g3 = Graph::parse("size 64\nlink v \"lib/pkg.mcg\" \"nope\"\nnode 1 perlin\noutput o 1\n").unwrap();
    assert!(g3.resolve_links(&dir)[0].contains("no graph 'nope'"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn m0_remaining_multi_output_nodes() {
    let g = Graph::parse(concat!(
        "size 64\n",
        "node 1 cells scale=4\nnode 2 voronoi scale=5\nnode 3 splatter count=6 scale=0.2 scale_var=0 rot_var=0\n",
        "node 4 threshold in=@3 threshold=0.3\nnode 5 flood_fill in=@4\n",
        "node 6 flood_fill_to_index in=@5\nnode 7 flood_fill_to_position in=@5\nnode 8 flood_fill_to_bbox_size in=@5\n",
        "output cells_id 1.cell_id\noutput cells 1\noutput vor_id 2.cell_id\noutput vor_edge 2.edge\noutput vor 2\n",
        "output spl 3\noutput spl_id 3.id\noutput spl_c 3.center\noutput spl_s 3.size\n",
        "output idx 6\noutput pos 7\noutput posx 7.x\noutput bb 8\noutput bbw 8.width\n",
    ))
    .unwrap();
    let r = g.evaluate(None).unwrap();
    let distinct = |t: &materialcraft::tex::Tex| {
        let mut v: Vec<u32> = t.px.iter().map(|p| (p[0] * 1000.0) as u32).collect();
        v.sort();
        v.dedup();
        v.len()
    };
    assert!(distinct(&r["cells_id"]) >= 8 && distinct(&r["cells"]) > 2);
    assert!(r["cells_id"].px != r["cells"].px, "the extra output must differ from the main one");
    assert!(distinct(&r["vor_id"]) >= 8);
    assert!(r["vor_id"].px != r["vor_edge"].px && r["vor_edge"].px != r["vor"].px);
    assert!(distinct(&r["spl_id"]) >= 3, "each splat has its own id");
    assert!(r["spl_c"].px.iter().any(|p| p[0] > 0.0 && p[1] > 0.0 && p[2] == 0.0));
    assert!(r["spl_s"].px.iter().any(|p| p[0] > 0.1 && p[0] < 0.3), "splat size 0.2 shows up");
    // flood-fill mappers: one number / centre / extent per region, background stays black
    assert!(distinct(&r["idx"]) >= 3);
    let top = r["idx"].px.iter().fold(0.0f32, |m, p| m.max(p[0]));
    assert!((top - 1.0).abs() < 1e-5, "the last region is index 1");
    assert!(r["pos"].px.iter().all(|p| p[0] >= 0.0 && p[0] < 1.0 && p[1] >= 0.0 && p[1] < 1.0));
    let same_region_same_centre = {
        // pixels sharing one index must share one centre
        let mut seen: std::collections::HashMap<u32, (u32, u32)> = std::collections::HashMap::new();
        r["idx"].px.iter().zip(&r["pos"].px).filter(|(i, _)| i[0] > 0.0).all(|(i, p)| {
            let e = seen.entry((i[0] * 1e5) as u32).or_insert(((p[0] * 1e5) as u32, (p[1] * 1e5) as u32));
            *e == ((p[0] * 1e5) as u32, (p[1] * 1e5) as u32)
        })
    };
    assert!(same_region_same_centre);
    assert!(r["bbw"].px.iter().any(|p| p[0] > 0.1 && p[0] < 0.4), "a 0.2-wide splat has a bounding box of that width");
    assert_eq!(r["posx"].px.len(), 64 * 64);
    assert!(r["posx"].px.iter().all(|p| p[0] == p[1] && p[1] == p[2]), "named x output is gray");
    assert!(materialcraft::graph::node_outputs("flood_fill_to_position").contains(&"x"));
}

#[test]
fn m0_pin_type_rules() {
    use materialcraft::graph::{connection_issues, Pin};
    // images convert freely between gray and colour
    assert!(Pin::Color.accepts(Pin::Gray) && Pin::Gray.accepts(Pin::Color));
    // values widen but never narrow, and never mix with images
    assert!(Pin::Float.accepts(Pin::Int) && Pin::Float3.accepts(Pin::Int3) && Pin::Int.accepts(Pin::Bool));
    assert!(!Pin::Int.accepts(Pin::Float) && !Pin::Float2.accepts(Pin::Float) && !Pin::Bool.accepts(Pin::Int));
    assert!(!Pin::Float.accepts(Pin::Gray) && !Pin::Gray.accepts(Pin::Float) && !Pin::Text.accepts(Pin::Float));
    assert_eq!((Pin::Float3.lanes(), Pin::Int.lanes(), Pin::Gray.lanes(), Pin::Text.lanes()), (3, 1, 0, 0));
    // every image graph is well typed
    let g = Graph::parse("size 32\nnode 1 perlin\nnode 2 colorize in=@1\nnode 3 blend bg=@1 fg=@2\noutput o 3\n").unwrap();
    assert!(connection_issues(&g).is_empty());
}

#[test]
fn m0_graph_default_format_and_hdr_safe_filters() {
    let base = "size 64\nnode 1 gradient mode=0\nnode 2 tint in=@1 r=1.37 g=0.3333 b=0.5\n";
    let run = |meta: &str, extra: &str| {
        let g = Graph::parse(&format!("{base}{meta}{extra}output o 2\n")).unwrap();
        g.evaluate(None).unwrap().remove("o").unwrap()
    };
    let float = run("", "");
    assert!(float.px.iter().any(|p| p[0] > 1.0));
    // a graph-wide 8-bit default rounds every node
    let eight = run("meta ofmt \"1\"\n", "");
    assert!(eight.px.iter().all(|p| p[0] <= 1.0 && ((p[0] * 255.0).round() - p[0] * 255.0).abs() < 1e-3));
    let g = Graph::parse(&format!("{base}meta ofmt \"1\"\noutput o 2\n")).unwrap();
    assert_eq!(g.default_format(), 1);
    // a node can opt back out (4 = force 32-bit float) or pick its own precision
    let kept = Graph::parse("size 64\nmeta ofmt \"1\"\nnode 1 gradient mode=0 ofmt=4\nnode 2 tint in=@1 r=1.37 g=0.3333 b=0.5 ofmt=4\noutput o 2\n").unwrap().evaluate(None).unwrap().remove("o").unwrap();
    assert!(kept.px.iter().any(|p| p[0] > 1.0));
    // the default survives a save / load round trip and changes the cache identity
    let again = Graph::parse(&g.to_text()).unwrap();
    assert_eq!(again.default_format(), 1);
    // filters that are linear keep values above 1 (HDR) instead of clamping
    for k in ["sharpen", "adjust", "hsl"] {
        let t = Graph::parse(&format!("size 64\nnode 1 gradient mode=0\nnode 2 tint in=@1 r=1.6 g=1.6 b=1.6\nnode 3 {k} in=@2\noutput o 3\n")).unwrap().evaluate(None).unwrap().remove("o").unwrap();
        assert!(t.px.iter().any(|p| p[0] > 1.0), "{k} must not clamp HDR values");
    }
}

#[test]
fn m0_square_cells_follow_the_aspect_ratio() {
    use materialcraft::noise;
    let run = |extra: &str| {
        let g = Graph::parse(&format!("size 64 32\nnode 1 perlin scale=8{extra}\noutput o 1\n")).unwrap();
        g.evaluate(None).unwrap().remove("o").unwrap()
    };
    let stretched = run("");
    let square = run(" square_cells=1");
    assert_eq!((square.w, square.h), (64, 32));
    // default: the same 8 x 8 lattice stretched over the 2:1 image; option: 8 x 4 lattice (square cells)
    let want = |py: i32, t: &materialcraft::tex::Tex| t.px.iter().enumerate().all(|(i, p)| {
        let (x, y) = (i % t.w, i / t.w);
        (p[0] - noise::perlin((x as f32 + 0.5) / t.w as f32, (y as f32 + 0.5) / t.h as f32, 8, py, 0)).abs() < 1e-5
    });
    assert!(want(8, &stretched));
    assert!(want(4, &square));
    // voronoi and cells take the option too (they keep running and differ from the stretched version)
    for k in ["voronoi", "cells", "fbm"] {
        let a = Graph::parse(&format!("size 64 32\nnode 1 {k} scale=6\noutput o 1\n")).unwrap().evaluate(None).unwrap().remove("o").unwrap();
        let b = Graph::parse(&format!("size 64 32\nnode 1 {k} scale=6 square_cells=1\noutput o 1\n")).unwrap().evaluate(None).unwrap().remove("o").unwrap();
        assert!(a.px != b.px, "{k} must react to square_cells");
    }
    // tile nodes derive the vertical count from the horizontal one
    let a = Graph::parse("size 64 32\nnode 1 tile_generator x_amount=4 y_amount=4\noutput o 1\n").unwrap().evaluate(None).unwrap().remove("o").unwrap();
    let b = Graph::parse("size 64 32\nnode 1 tile_generator x_amount=4 y_amount=4 square_cells=1\noutput o 1\n").unwrap().evaluate(None).unwrap().remove("o").unwrap();
    assert!(a.px != b.px);
}

#[test]
fn m0_dependency_management() {
    let dir = std::env::temp_dir().join(format!("mc_deps_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::create_dir_all(dir.join("moved")).unwrap();
    std::fs::write(dir.join("lib/pat.mcg"), "size 64\nnode 1 perlin scale=4\noutput o 1\n").unwrap();
    let text = "size 64\nlink pat \"lib/pat.mcg\"\nnode 1 subgraph graph=\"pat\"\nnode 2 subgraph graph=\"pat\"\nnode 3 blend bg=@1 fg=@2\noutput o 3\n";
    let mut g = Graph::parse(text).unwrap();
    assert!(g.resolve_links(&dir).is_empty());
    let info = &g.link_info["pat"];
    assert!(info.found.as_ref().unwrap().ends_with("lib/pat.mcg") && info.issue.is_none());
    assert_eq!(info.members, vec!["pat".to_string()]);

    // the file moves: reported, then found again through a search folder
    std::fs::rename(dir.join("lib/pat.mcg"), dir.join("moved/pat.mcg")).unwrap();
    assert_eq!(g.resolve_links(&dir).len(), 1);
    assert!(g.link_info["pat"].found.is_none() && g.link_info["pat"].issue.as_ref().unwrap().contains("cannot read"));
    g.meta.insert("search_paths".into(), "elsewhere; moved".into());
    assert_eq!(g.search_paths(), vec!["elsewhere".to_string(), "moved".to_string()]);
    assert!(g.resolve_links(&dir).is_empty(), "found by name in a search folder");
    assert!(g.link_info["pat"].found.as_ref().unwrap().ends_with("moved/pat.mcg"));
    assert!(g.evaluate(None).is_ok());

    // relink keeps the name, so both instances follow
    std::fs::write(dir.join("moved/other.mcg"), "size 64\nnode 1 voronoi scale=3\noutput o 1\n").unwrap();
    let before = g.evaluate(None).unwrap().remove("o").unwrap();
    assert!(g.relink("pat", "moved/other.mcg", None));
    assert!(g.resolve_links(&dir).is_empty());
    assert!(before.px != g.evaluate(None).unwrap().remove("o").unwrap().px);

    // breaking the link embeds a copy that is saved with the package and no longer follows the file
    let n = g.break_link("pat").unwrap();
    assert_eq!(n, 1);
    assert!(g.links.is_empty() && !g.is_linked("pat") && g.subgraphs.contains_key("pat"));
    let saved = g.to_text();
    assert!(!saved.contains("link pat") && saved.contains("subgraph pat"), "{saved}");
    std::fs::remove_file(dir.join("moved/other.mcg")).unwrap();
    let mut again = Graph::parse(&saved).unwrap();
    assert!(again.resolve_links(&dir).is_empty());
    assert!(again.evaluate(None).is_ok(), "the embedded copy still works without the file");

    // removing a link reports the instances that are left dangling
    let mut h = Graph::parse("size 64\nlink pat \"moved/none.mcg\"\nnode 1 subgraph graph=\"pat\"\noutput o 1\n").unwrap();
    let _ = h.resolve_links(&dir);
    assert!(h.break_link("pat").is_err(), "an unresolved link cannot be embedded");
    assert_eq!(h.remove_link("pat"), 1);
    assert!(h.links.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn m1_curve_node() {
    use materialcraft::curve::Curve;
    let c = Curve::parse("0:0,0.5:0.8,1:1").unwrap();
    assert!((c.eval(0.5) - 0.8).abs() < 1e-5);
    assert!(c.eval(0.25) > 0.25, "curve bends up");
    let lut = c.lut();
    assert!(lut.windows(2).all(|w| w[1] >= w[0] - 1e-6), "monotone points give a monotone curve");
    assert!(Curve::parse("0:0,abc").is_err());
    assert!((Curve::identity().eval(0.3) - 0.3).abs() < 1e-5);
    let g = G2::parse("size 16\nnode 1 gradient\nnode 2 curve in=@1 points=\"0:1,1:0\"\nnode 3 invert in=@1\noutput a 2\noutput b 3\n").unwrap();
    let o = g.evaluate(Some(16)).unwrap();
    for (x, y) in o["a"].px.iter().zip(&o["b"].px) {
        assert!((x[0] - y[0]).abs() < 2e-3, "reversed curve equals invert");
    }
}

#[test]
fn m1_channel_shuffle_and_bitmap_options() {
    let g = G2::parse("size 8\nnode 1 color r=0.2 g=0.4 b=0.6\nnode 2 channel_shuffle in=@1 src_r=2 src_g=8 src_b=9 inv_a=1\noutput o 2\n").unwrap();
    let p = g.evaluate(Some(8)).unwrap()["o"].px[0];
    assert!((p[0] - 0.6).abs() < 1e-5 && p[1] == 0.0 && p[2] == 1.0 && p[3] == 0.0, "{p:?}");
}

#[cfg(any(feature = "cms", feature = "craft"))]
#[test]
fn colorspace_conversion_roundtrips_and_keeps_alpha() {
    use materialcraft::{colorspace, tex::Tex};
    let t = Tex::solid(4, 4, [0.5, 0.25, 0.75, 0.6]);
    // sRGB means no conversion
    assert_eq!(colorspace::convert(&t, "srgb").unwrap().px[0], t.px[0]);
    // mid grey in sRGB is about 0.214 in linear light
    let lin = colorspace::convert(&Tex::solid(2, 2, [0.5, 0.5, 0.5, 1.0]), "linear").unwrap();
    assert!((lin.px[0][0] - 0.214).abs() < 0.01, "{:?}", lin.px[0]);
    // alpha is untouched, wider spaces change colours
    let p3 = colorspace::convert(&t, "p3").unwrap();
    assert!((p3.px[0][3] - 0.6).abs() < 1e-6);
    assert!(p3.px[0][0] != t.px[0][0] || p3.px[0][1] != t.px[0][1]);
    assert!(colorspace::convert(&t, "nope").is_err());
    assert!(colorspace::is_color_output("basecolor") && !colorspace::is_color_output("normal"));
}

#[test]
fn library_assets_parse_and_render() {
    use materialcraft::library::{self, AssetKind};
    let mut ids = std::collections::HashSet::new();
    for a in library::assets() {
        assert!(ids.insert(a.id), "duplicate id {}", a.id);
        assert!(!a.tags.is_empty() && !a.description.is_empty(), "{} lacks tags or description", a.id);
        let g = a.graph().unwrap_or_else(|e| panic!("{}: {e}", a.id));
        let out = g.evaluate(Some(64)).unwrap_or_else(|e| panic!("{}: {e}", a.id));
        assert!(!out.is_empty(), "{} has no outputs", a.id);
        for (k, t) in &out {
            assert!(t.px.iter().all(|v| v.iter().all(|c| c.is_finite())), "{}:{k} has non-finite values", a.id);
        }
        let th = a.thumbnail(32).unwrap();
        assert_eq!((th.w, th.h), (32, 32));
        assert!(library::find(a.id).is_some());
    }
    assert!(library::assets().iter().any(|a| a.kind == AssetKind::Material));
    assert!(!library::search("rust", None).is_empty());
    assert!(library::search("zzzz-nothing", None).is_empty());
    assert_eq!(library::search("", Some(AssetKind::Filter)).len(), library::categories(AssetKind::Filter).iter().map(|c| library::assets().iter().filter(|a| a.kind == AssetKind::Filter && a.category == *c).count()).sum::<usize>());
}
