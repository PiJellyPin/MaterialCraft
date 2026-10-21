// SPDX-License-Identifier: MIT OR Apache-2.0
use materialcraft::bake::{self, BakeSet, BakeSettings, MapKind, Mesh, Progress};
use materialcraft::bvh::{Bvh, Filter};
use materialcraft::tex::Tex;

fn obj(t: &str) -> Mesh {
    bake::parse_obj(t).unwrap()
}

/// A unit plane in XY (z = 0) facing +Z with UVs equal to xy. `name` is the object name.
fn plane(name: &str, z: f32) -> String {
    format!("o {name}\nv 0 0 {z}\nv 1 0 {z}\nv 1 1 {z}\nv 0 1 {z}\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nvn 0 0 1\nf 1/1/1 2/2/1 3/3/1\nf 1/1/1 3/3/1 4/4/1\n")
}

/// A closed box [x0,x1]x[y0,y1]x[0,z1] (outward faces, flat normals) with the plane's UVs ignored.
fn box_obj(name: &str, x0: f32, x1: f32, y0: f32, y1: f32, z1: f32) -> String {
    let mut s = format!("o {name}\n");
    let v = [[x0, y0, 0.0], [x1, y0, 0.0], [x1, y1, 0.0], [x0, y1, 0.0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
    for p in v {
        s.push_str(&format!("v {} {} {}\n", p[0], p[1], p[2]));
    }
    s.push_str("f 1 4 3 2\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n");
    s
}

fn small() -> BakeSettings {
    BakeSettings { width: 32, height: 32, aa: 1, padding: 2, front: 0.5, rear: 0.5, ao_rays: 16, thick_rays: 8, maps: vec![MapKind::Normal], ..Default::default() }
}

fn centre(t: &Tex) -> [f32; 4] {
    t.px[(t.h / 2) * t.w + t.w / 2]
}

#[test]
fn identity_bake_gives_flat_normals_and_open_ao() {
    let low = obj(&plane("p", 0.0));
    let mut s = small();
    s.maps = vec![MapKind::Normal, MapKind::AmbientOcclusion, MapKind::Opacity];
    let r = bake::bake_full(&low, Some(&low), None, &s, None).unwrap();
    assert_eq!(r.maps.len(), s.maps.len(), "every requested bake map must be returned");
    let n = centre(&r.maps["normal"]);
    assert!((n[0] - 0.5).abs() < 0.01 && (n[1] - 0.5).abs() < 0.01 && n[2] > 0.99, "{n:?}");
    assert!(centre(&r.maps["ambient_occlusion"])[0] > 0.95);
    assert!(centre(&r.maps["opacity"])[0] > 0.99);
    assert_eq!(r.report.low_tris, 2);
    assert!(r.report.covered > 0 && r.report.covered <= r.report.texels);
}

#[test]
fn sphere_against_itself_has_unit_normals_and_bump_changes_them() {
    let low = obj(&bake::uv_sphere_obj(12, 8, 1.0, 0.0));
    let flat = bake::bake(&low, &low, &BakeSettings { width: 48, height: 48, ..small() }).unwrap();
    let bumped = obj(&bake::uv_sphere_obj(48, 32, 1.0, 0.06));
    let b = bake::bake(&low, &bumped, &BakeSettings { width: 48, height: 48, ..small() }).unwrap();
    let dev = |t: &Tex| t.px.iter().map(|p| (p[0] - 0.5).abs() + (p[1] - 0.5).abs()).sum::<f32>() / t.px.len() as f32;
    println!("dev {} {}", dev(&b["normal"]), dev(&flat["normal"]));
    assert!(dev(&b["normal"]) > dev(&flat["normal"]) + 0.01, "{} {}", dev(&b["normal"]), dev(&flat["normal"]));
    assert!(b["normal"].px.iter().all(|p| p[2] > 0.2));
}

#[test]
fn plane_with_raised_box() {
    let low = obj(&plane("p", 0.0));
    let hi = Mesh::merge(&[obj(&plane("p", 0.0)), obj(&box_obj("b", 0.25, 0.75, 0.25, 0.75, 0.2))]);
    let s = BakeSettings { match_by_name: false, maps: vec![MapKind::Normal, MapKind::Height, MapKind::Position, MapKind::WorldNormal, MapKind::AmbientOcclusion, MapKind::ObjectId, MapKind::Thickness], front: 0.4, rear: 0.1, ao_dist: 0.3, ..small() };
    let r = bake::bake_full(&low, Some(&hi), None, &s, None).unwrap();
    let h = &r.maps["height"];
    let c = centre(h)[0];
    let e = h.px[(2 * h.w) + 2][0];
    assert!(c > e + 0.05, "the box top is higher than the plane: {c} vs {e}");
    // the box top faces up, so its tangent normal is flat; the corner of the plane is flat too
    let n = &r.maps["normal"];
    assert!(centre(n)[2] > 0.95);
    // a texel on the box side: just inside the box edge at x = 0.25 would be a top; between plane and box the AO darkens
    let ao = &r.maps["ambient_occlusion"];
    let near_wall = ao.px[(ao.h / 2) * ao.w + (0.2 * ao.w as f32) as usize][0];
    let far = ao.px[(ao.h / 2) * ao.w + 2][0];
    assert!(near_wall < far, "ambient occlusion is darker next to the box wall: {near_wall} vs {far}");
    // two objects in the high-poly mesh get different id colours, but they are both hit from the plane
    let id = &r.maps["object_id"];
    assert!(id.px.iter().any(|p| p[0] + p[1] + p[2] > 0.0));
    let pos = &r.maps["position"];
    assert!(centre(pos) != pos.px[(2 * pos.w) + 2]);
}

#[test]
fn match_by_name_hits_only_the_partner() {
    // two low planes side by side in UV space but at the same position in space as two high planes
    let mk = |name: &str, x0: f32, z: f32, uv0: f32| {
        format!("o {name}\nv {x0} 0 {z}\nv {} 0 {z}\nv {} 1 {z}\nv {x0} 1 {z}\nvt {uv0} 0\nvt {} 0\nvt {} 1\nvt {uv0} 1\nvn 0 0 1\nf 1/1/1 2/2/1 3/3/1\nf 1/1/1 3/3/1 4/4/1\n", x0 + 0.5, x0 + 0.5, uv0 + 0.5, uv0 + 0.5)
    };
    // low a at x 0..0.5 and b at x 0..0.5 as well but shifted in uv; high_a is flat at z=0.1, high_b at z=-0.1
    let join = |parts: &[String]| {
        Mesh::merge(&parts.iter().map(|p| obj(p)).collect::<Vec<_>>())
    };
    let low = join(&[mk("a_low", 0.0, 0.0, 0.0), mk("b_low", 0.0, 0.0, 0.5)]);
    let high = join(&[mk("a_high", 0.0, 0.1, 0.0), mk("b_high", 0.0, -0.1, 0.0)]);
    let s = BakeSettings { maps: vec![MapKind::Height, MapKind::ObjectId], front: 0.3, rear: 0.3, ..small() };
    let r = bake::bake_full(&low, Some(&high), None, &s, None).unwrap();
    let h = &r.maps["height"];
    // left half of the texture belongs to a_low (hits z=+0.1), right half to b_low (hits z=-0.1)
    let l = h.px[(h.h / 2) * h.w + h.w / 4][0];
    let rr = h.px[(h.h / 2) * h.w + 3 * h.w / 4][0];
    assert!(l > rr + 0.05, "a_low sees the plane above, b_low the plane below: {l} {rr}");
    let mut off = s.clone();
    off.match_by_name = false;
    let r2 = bake::bake_full(&low, Some(&high), None, &off, None).unwrap();
    let h2 = &r2.maps["height"];
    let l2 = h2.px[(h2.h / 2) * h2.w + h2.w / 4][0];
    let r2v = h2.px[(h2.h / 2) * h2.w + 3 * h2.w / 4][0];
    assert!((l2 - r2v).abs() < 0.05, "without matching both halves see the same nearest hit: {l2} {r2v}");
}

#[test]
fn thickness_grows_with_slab() {
    let low = obj(&plane("p", 0.0));
    let thick = |z: f32| {
        let hi = obj(&box_obj("b", -1.0, 2.0, -1.0, 2.0, z));
        let s = BakeSettings { maps: vec![MapKind::Thickness], thick_dist: 1.0, thick_rays: 16, ..small() };
        // the plane sits on the bottom face of the slab, so look at the slab from its own surface
        centre(&bake::bake_full(&low, Some(&hi), None, &s, None).unwrap().maps["thickness"])[0]
    };
    let (a, b) = (thick(0.2), thick(0.6));
    assert!(b > a, "{a} {b}");
}

#[test]
fn padding_fills_gutters_and_background_beyond() {
    // a small island in the middle of the texture
    let island = "o i\nv 0.4 0.4 0\nv 0.6 0.4 0\nv 0.6 0.6 0\nv 0.4 0.6 0\nvt 0.4 0.4\nvt 0.6 0.4\nvt 0.6 0.6\nvt 0.4 0.6\nvn 0 0 1\nf 1/1/1 2/2/1 3/3/1\nf 1/1/1 3/3/1 4/4/1\n";
    let low = obj(island);
    let mut s = small();
    s.width = 64;
    s.height = 64;
    s.padding = 4;
    s.maps = vec![MapKind::AmbientOcclusion, MapKind::Position];
    let r = bake::bake_full(&low, Some(&low), None, &s, None).unwrap();
    let t = &r.maps["position"];
    let at = |x: usize, y: usize| t.px[y * t.w + x];
    let bg = MapKind::Position.background();
    // island covers x,y in 25.6..38.4; two texels outside is gutter (not background), far away is background
    assert!(at(32, 32) != bg);
    let gutter = at(40, 32);
    assert!(gutter != bg, "within the padding the island colour is extended");
    assert_eq!(at(60, 32), bg);
    assert_eq!(at(2, 2), bg);
}

#[test]
fn cancel_stops_the_bake() {
    let low = obj(&bake::uv_sphere_obj(24, 16, 1.0, 0.0));
    let high = obj(&bake::uv_sphere_obj(96, 64, 1.0, 0.03));
    let p = Progress::new();
    p.request_cancel();
    let s = BakeSettings { width: 256, height: 256, aa: 2, maps: vec![MapKind::Normal, MapKind::AmbientOcclusion], ..small() };
    let r = bake::bake_full(&low, Some(&high), None, &s, Some(&p));
    assert!(r.is_err() && r.unwrap_err().to_lowercase().contains("cancel"));
    let p = Progress::new();
    let ok = bake::bake_full(&low, Some(&high), None, &BakeSettings { width: 32, height: 32, ..s.clone() }, Some(&p)).unwrap();
    assert!(!ok.maps.is_empty() && p.fraction() > 0.99, "{}", p.fraction());
}

#[test]
fn errors_are_explained() {
    assert!(bake::parse_obj("v 0 0 0\n").is_err(), "a vertex without faces is not a mesh");
    let low = obj(&plane("p", 0.0));
    let nouv = obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n");
    assert!(bake::bake(&nouv, &low, &small()).unwrap_err().contains("UV"));
    let cage = obj(&box_obj("c", 0.0, 1.0, 0.0, 1.0, 0.1));
    assert!(bake::bake_full(&low, Some(&low), Some(&cage), &small(), None).unwrap_err().contains("cage"));
    let mut s = small();
    s.maps.clear();
    assert!(bake::bake(&low, &low, &s).is_err());
    let mut s = small();
    s.material = "nope".into();
    assert!(bake::bake(&low, &low, &s).unwrap_err().contains("nope"));
    let mut s = small();
    s.uv_set = 1;
    assert!(bake::bake(&low, &low, &s).unwrap_err().contains("second UV"));
}

#[test]
fn material_filter_and_second_uv_set() {
    let two = "o m\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 0.5 0\nvt 0.5 1\nvt 0 1\nvn 0 0 1\nusemtl a\nf 1/1/1 2/2/1 3/3/1\nusemtl b\nf 1/1/1 3/3/1 4/4/1\n";
    let low = obj(two);
    assert_eq!(low.materials.len(), 2);
    let mut s = small();
    s.maps = vec![MapKind::Opacity];
    let all = bake::bake(&low, &low, &s).unwrap();
    s.material = "a".into();
    let only = bake::bake(&low, &low, &s).unwrap();
    let cov = |m: &std::collections::BTreeMap<String, Tex>| m["opacity"].px.iter().filter(|p| p[0] > 0.5).count();
    assert!(cov(&only) < cov(&all));
}

#[test]
fn directx_flips_green() {
    let low = obj(&bake::uv_sphere_obj(12, 8, 1.0, 0.0));
    let high = obj(&bake::uv_sphere_obj(48, 32, 1.0, 0.06));
    let mut s = BakeSettings { width: 32, height: 32, ..small() };
    let gl = bake::bake(&low, &high, &s).unwrap();
    s.flip_y = true;
    let dx = bake::bake(&low, &high, &s).unwrap();
    for (a, b) in gl["normal"].px.iter().zip(&dx["normal"].px) {
        if a[2] > 0.0 && (a[1] != 0.5 || a[0] != 0.5) {
            assert!((a[1] + b[1] - 1.0).abs() < 1e-4, "{a:?} {b:?}");
        }
    }
}

#[test]
fn curvature_sign_follows_shape() {
    let sph = obj(&bake::uv_sphere_obj(24, 16, 1.0, 0.0));
    let mut inv = sph.clone();
    for t in &mut inv.tris {
        t.p.swap(1, 2);
        t.uv.swap(1, 2);
        t.n = [[-t.n[0][0], -t.n[0][1], -t.n[0][2]], [-t.n[2][0], -t.n[2][1], -t.n[2][2]], [-t.n[1][0], -t.n[1][1], -t.n[1][2]]];
    }
    let s = BakeSettings { width: 48, height: 48, maps: vec![MapKind::Curvature], curv_radius: 0.2, ..small() };
    let mean = |m: &Mesh| {
        let t = &bake::bake_full(m, None, None, &s, None).unwrap().maps["curvature"];
        t.px.iter().map(|p| p[0]).sum::<f32>() / t.px.len() as f32
    };
    let (a, b) = (mean(&sph), mean(&inv));
    assert!(a > 0.5 && b < 0.5 || a < 0.5 && b > 0.5, "convex and concave must fall on opposite sides of 0.5: {a} {b}");
}

#[test]
fn settings_and_sets_round_trip() {
    let mut s = BakeSettings::game();
    s.flip_y = true;
    s.maps = vec![MapKind::Normal, MapKind::Curvature, MapKind::UvLayout];
    s.low_normals = bake::NormalMode::Angle(45.0);
    s.pos_range = Some(([0.0, 0.0, 0.0], [1.0, 2.0, 3.0]));
    let t = s.to_text();
    let r = BakeSettings::from_text(&t).unwrap();
    assert_eq!(r.to_text(), t);
    assert!(BakeSettings::from_text("wibble 3\n").is_err());
    assert!(BakeSettings::from_text("size abc\n").is_err());
    assert!(BakeSettings::from_text("maps normal, bogus\n").is_err());
    let set = BakeSet { name: "rock".into(), low: vec!["a.obj".into()], high: vec!["b.obj".into(), "c.ply".into()], cage: Some("cage.obj".into()), settings: s };
    let back = BakeSet::parse("rock", &set.to_text()).unwrap();
    assert_eq!(back.low, set.low);
    assert_eq!(back.high, set.high);
    assert_eq!(back.cage, set.cage);
    assert_eq!(back.settings.to_text(), set.settings.to_text());
    assert!(BakeSet::parse("x", "garbage").is_err());
    for m in MapKind::ALL {
        assert_eq!(MapKind::from_id(m.id()), Some(m));
    }
}

#[test]
fn uv_layout_marks_wires() {
    let low = obj(&plane("p", 0.0));
    let s = BakeSettings { maps: vec![MapKind::UvLayout], width: 64, height: 64, padding: 0, ..small() };
    let t = &bake::bake(&low, &low, &s).unwrap()["uv_layout"];
    let bright = t.px.iter().filter(|p| p[0] > 0.5).count();
    assert!(bright > 100 && bright < t.px.len(), "{bright}");
}

#[test]
fn bvh_matches_brute_force() {
    let mut seed = 12345u32;
    let mut rnd = move || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / 16777216.0
    };
    let mut text = String::new();
    for i in 0..300 {
        let c = [rnd() * 4.0, rnd() * 4.0, rnd() * 4.0];
        for _ in 0..3 {
            text.push_str(&format!("v {} {} {}\n", c[0] + rnd() * 0.5, c[1] + rnd() * 0.5, c[2] + rnd() * 0.5));
        }
        text.push_str(&format!("f {} {} {}\n", i * 3 + 1, i * 3 + 2, i * 3 + 3));
    }
    let m = obj(&text);
    let bvh = Bvh::build(&m.tris);
    for _ in 0..200 {
        let o = [rnd() * 6.0 - 1.0, rnd() * 6.0 - 1.0, rnd() * 6.0 - 1.0];
        let d = {
            let v = [rnd() - 0.5, rnd() - 0.5, rnd() - 0.5];
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
            [v[0] / l, v[1] / l, v[2] / l]
        };
        let fast = bvh.closest(o, d, 1e-5, 100.0, &Filter::ALL);
        // brute force Moller-Trumbore
        let mut best: Option<(f32, usize)> = None;
        for (i, t) in m.tris.iter().enumerate() {
            let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
            let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
            let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            let (e1, e2) = (sub(t.p[1], t.p[0]), sub(t.p[2], t.p[0]));
            let pv = cross(d, e2);
            let det = dot(e1, pv);
            if det.abs() < 1e-12 {
                continue;
            }
            let tv = sub(o, t.p[0]);
            let u = dot(tv, pv) / det;
            let q = cross(tv, e1);
            let v = dot(d, q) / det;
            let tt = dot(e2, q) / det;
            if u >= 0.0 && v >= 0.0 && u + v <= 1.0 && tt > 1e-5 && best.map_or(true, |b| tt < b.0) {
                best = Some((tt, i));
            }
        }
        match (fast, best) {
            (None, None) => {}
            (Some(h), Some((t, _))) => assert!((h.t - t).abs() < 1e-3 * t.max(1.0), "{} {}", h.t, t),
            (a, b) => panic!("mismatch {:?} {:?}", a.map(|h| h.t), b.map(|b| b.0)),
        }
        assert_eq!(fast.is_some(), bvh.any(o, d, 1e-5, 100.0, &Filter::ALL));
    }
}

#[test]
fn baked_maps_feed_the_graph() {
    use materialcraft::graph::Graph;
    let low = obj(&bake::uv_sphere_obj(16, 10, 1.0, 0.0));
    let high = obj(&bake::uv_sphere_obj(48, 32, 1.0, 0.05));
    let s = BakeSettings { width: 64, height: 64, maps: vec![MapKind::Normal, MapKind::Curvature, MapKind::AmbientOcclusion, MapKind::WorldNormal], curv_radius: 0.2, ..small() };
    let r = bake::bake_full(&low, Some(&high), None, &s, None).unwrap();
    let text = "size 32\\nnode 1 baked_map set=\"rock\" map=\"curvature\"\\nnode 2 baked_map set=\"rock\" map=\"ambient_occlusion\"\\nnode 3 baked_map set=\"rock\" map=\"world_normal\"\\nnode 4 mesh_weathering curvature=@1 ao=@2 normal=@3 wear=0.8 dirt=0.8 dust=0.8\\nnode 5 baked_map set=\"rock\" map=\"normal\"\\noutput w 4\\noutput n 5\\n".replace("\\n", "\n");
    let mut g = Graph::parse(&text).unwrap();
    // without maps the node explains what to do
    let e = g.evaluate(None).unwrap_err().to_string();
    assert!(e.contains("mesh baker") && e.contains("rock"), "{e}");
    assert_eq!(bake::store_maps(&mut g.resources, "rock", &r.maps), 4);
    assert_eq!(bake::stored_sets(&g.resources)["rock"].len(), 4);
    let out = g.evaluate(None).unwrap();
    assert_eq!((out["w"].w, out["w"].h), (32, 32));
    let lo = out["w"].px.iter().map(|p| p[0]).fold(1.0f32, f32::min);
    let hi = out["w"].px.iter().map(|p| p[0]).fold(0.0f32, f32::max);
    assert!(hi - lo > 0.2, "the weathering mask varies: {lo} {hi}");
    // the colour map keeps its blue channel; size follows the graph
    assert!(out["n"].px.iter().filter(|p| p[2] > 0.5).count() > 100);
    // the package text round trips with the maps inside
    let g2 = Graph::parse(&g.to_text()).unwrap();
    assert_eq!(g2.resources.len(), g.resources.len());
    let o2 = g2.evaluate(None).unwrap();
    assert_eq!(o2["w"].px, out["w"].px);
    // a bad node is rejected
    let bad = |t: &str| Graph::parse(t).map(|g| g.evaluate(None).is_err()).unwrap_or(true);
    assert!(bad("size 16\nnode 1 baked_map set=\"a\" map=\"bogus\"\noutput o 1\n"));
    assert!(bad("size 16\nnode 1 baked_map map=\"normal\"\noutput o 1\n"));
}

#[test]
fn png_compression_round_trips_and_shrinks() {
    use materialcraft::png;
    // smooth gradient with a flat area and some noise: 8 and 16 bit, opaque and with alpha
    let (w, h) = (96, 70);
    let mut seed = 7u32;
    let mut px = Vec::new();
    for y in 0..h {
        for x in 0..w {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let n = (seed >> 24) as f32 / 255.0;
            let v = if x < 30 { 0.25 } else { (x as f32 / w as f32 + y as f32 / h as f32) * 0.5 + n * 0.01 };
            px.push([v, 1.0 - v, (v * 3.0).fract(), 1.0]);
        }
    }
    for depth in [8u8, 16] {
        let bytes = png::encode(w, h, &px, depth);
        assert!(bytes.len() < w * h * 3 * (depth as usize / 8), "compressed: {} bytes", bytes.len());
        let back = png::decode(&bytes).unwrap();
        assert_eq!((back.w, back.h), (w, h));
        let tol = if depth == 16 { 1.0 / 65535.0 } else { 1.0 / 255.0 } + 1e-4;
        for (a, b) in px.iter().zip(&back.px) {
            for c in 0..3 {
                assert!((a[c] - b[c]).abs() <= tol, "{a:?} {b:?}");
            }
        }
    }
    // with alpha
    let mut pa = px.clone();
    for (i, p) in pa.iter_mut().enumerate() {
        p[3] = (i % 7) as f32 / 7.0;
    }
    let back = png::decode(&png::encode(w, h, &pa, 16)).unwrap();
    assert!((back.px[5][3] - pa[5][3]).abs() < 1e-3);
    // tiny and empty-ish images
    for (tw, th) in [(1usize, 1usize), (2, 1), (1, 3), (17, 1)] {
        let t: Vec<[f32; 4]> = (0..tw * th).map(|i| [i as f32 / 20.0, 0.5, 0.25, 1.0]).collect();
        let b = png::decode(&png::encode(tw, th, &t, 8)).unwrap();
        assert_eq!((b.w, b.h), (tw, th));
    }
    // random data still round trips (nothing to gain, nothing may break)
    let mut seed = 99u32;
    let noise: Vec<[f32; 4]> = (0..64 * 64).map(|_| { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223); [(seed >> 24) as f32 / 255.0, (seed >> 16 & 255) as f32 / 255.0, (seed >> 8 & 255) as f32 / 255.0, 1.0] }).collect();
    let b = png::decode(&png::encode(64, 64, &noise, 8)).unwrap();
    for (a, c) in noise.iter().zip(&b.px) {
        assert!((a[0] - c[0]).abs() < 1.0 / 255.0 + 1e-4);
    }
}
