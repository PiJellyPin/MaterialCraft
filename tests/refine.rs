// SPDX-License-Identifier: MIT OR Apache-2.0
//! Regression tests for the fixes of the "refine" round: Tile Random 2, Pixel Processor inputs, 3D preview
//! freshness, per-graph names, saved view settings, review panel layout, new macros and expression helpers.
use materialcraft::json::Json;
use materialcraft::session::Session;

fn run(s: &mut Session, cmd: &str, p: Json) -> Json {
    match s.execute(cmd, &p) {
        Ok(o) => o.data,
        Err(e) => panic!("{cmd} failed: {e}"),
    }
}

fn lane(j: &Json, key: &str, i: usize) -> f64 {
    j.get(key).and_then(Json::as_arr).and_then(|a| a.get(i)).and_then(Json::as_f64).unwrap_or_else(|| panic!("{key}[{i}] in {}", j.to_text()))
}

fn stats(s: &mut Session, node: &str, size: usize) -> Json {
    run(s, "render.node", Json::obj().with("node", node).with("size", size).with("image", false))
}

fn add(s: &mut Session, kind: &str, name: &str, params: Json, inputs: Json) {
    run(s, "node.add", Json::obj().with("kind", kind).with("name", name).with("params", params).with("inputs", inputs));
}

const TR2: &str = "sbs:lib:pattern_tile_random_2/tile_random_2";

#[test]
fn tile_random_2_obeys_its_parameters() {
    let mut s = Session::new();
    add(&mut s, TR2, "a", Json::obj().with("amount_x", 5).with("amount_y", 6), Json::obj());
    add(&mut s, TR2, "b", Json::obj().with("amount_x", 5).with("amount_y", 6).with("interstice_thickness", 1.0), Json::obj());
    let (ma, mb) = (lane(&stats(&mut s, "a", 256), "mean", 0), lane(&stats(&mut s, "b", 256), "mean", 0));
    assert!(mb < ma - 0.02, "wider gaps must darken the pattern: {ma} vs {mb}");
    // bevel width does not depend on the tile size: the same bevel on bigger tiles keeps a similar share of ramp pixels
    add(&mut s, TR2, "c", Json::obj().with("amount_x", 5).with("amount_y", 6).with("bevel_distance_multiplier", 0.0), Json::obj());
    let mc = lane(&stats(&mut s, "c", 256), "mean", 0);
    assert!(mc > ma, "no bevel means flatter, brighter tiles: {ma} vs {mc}");
    // random_size_y = 0 keeps every column in one piece per row (rows are straight)
    add(&mut s, TR2, "d", Json::obj().with("amount_x", 4).with("amount_y", 4).with("random_size_y", 0), Json::obj());
    let d = stats(&mut s, "d", 256);
    assert_eq!(d.get("tiles_seamlessly").and_then(Json::as_bool), Some(true), "{}", d.to_text());
}

#[test]
fn pixel_processor_reads_many_inputs() {
    let mut s = Session::new();
    for i in 1..=8 {
        add(&mut s, "sbs:uniform", &format!("u{i}"), Json::obj().with("outputcolor", Json::Arr(vec![Json::from(i as f64 / 10.0), Json::from(i as f64 / 10.0), Json::from(i as f64 / 10.0), Json::from(1)])), Json::obj());
    }
    add(&mut s, "sbs:pixelprocessor", "pp", Json::obj().with("colorswitch", 0), Json::obj());
    let mut sum = String::new();
    for i in 0..8 {
        let slot = if i == 0 { "input".to_string() } else { format!("input:{i}") };
        run(&mut s, "node.connect", Json::obj().with("from", format!("u{}", i + 1)).with("to", "pp").with("slot", slot));
        sum += &format!("{}sample($pos, {i})", if i == 0 { "" } else { " + " });
    }
    run(&mut s, "node.expression", Json::obj().with("node", "pp").with("param", "perpixel").with("expression", format!("({sum}) / 8")));
    let st = stats(&mut s, "pp", 32);
    assert!((lane(&st, "mean", 0) - 0.45).abs() < 0.02, "{}", st.to_text());
}

#[test]
fn expressions_have_repeatable_random_numbers() {
    let mut s = Session::new();
    add(&mut s, "sbs:pixelprocessor", "h", Json::obj().with("colorswitch", 0), Json::obj());
    run(&mut s, "node.expression", Json::obj().with("node", "h").with("param", "perpixel").with("expression", "cellrand(floor($pos * 8), 3)"));
    let a = stats(&mut s, "h", 64);
    let b = stats(&mut s, "h", 64);
    assert_eq!(a.to_text(), b.to_text());
    assert!(!a.get("flat").and_then(Json::as_bool).unwrap_or(true));
    run(&mut s, "node.expression", Json::obj().with("node", "h").with("param", "perpixel").with("expression", "hash(floor($pos.x * 4))"));
    assert!(!stats(&mut s, "h", 64).get("flat").and_then(Json::as_bool).unwrap_or(true));
}

#[test]
fn node_remove_takes_node_or_nodes() {
    let mut s = Session::new();
    for n in ["a", "b", "c"] {
        add(&mut s, "sbs:uniform", n, Json::obj(), Json::obj());
    }
    run(&mut s, "node.remove", Json::obj().with("node", "a"));
    run(&mut s, "node.remove", Json::obj().with("nodes", Json::Arr(vec![Json::from("b")])));
    let o = run(&mut s, "graph.outline", Json::obj());
    let text = o.to_text();
    assert!(text.contains('c') && !text.contains("\"a\""), "{text}");
}

#[test]
fn the_3d_picture_follows_the_edits() {
    let mut s = Session::new();
    add(&mut s, "sbs:uniform", "c", Json::obj().with("outputcolor", Json::Arr(vec![Json::from(0.9), Json::from(0.1), Json::from(0.1), Json::from(1)])), Json::obj());
    run(&mut s, "output.add", Json::obj().with("node", "c").with("usage", "basecolor"));
    let first = s.execute("render.3d", &Json::obj().with("size", 64).with("tex", 64)).unwrap().images[0].png.clone();
    run(&mut s, "node.set", Json::obj().with("node", "c").with("params", Json::obj().with("outputcolor", Json::Arr(vec![Json::from(0.1), Json::from(0.9), Json::from(0.1), Json::from(1)]))));
    let second = s.execute("render.3d", &Json::obj().with("size", 64).with("tex", 64)).unwrap().images[0].png.clone();
    assert_ne!(first, second, "the picture must change after the edit");
}

#[test]
fn view_and_default_format_are_saved_with_the_graph() {
    let dir = std::env::temp_dir().join(format!("mc_refine_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let root = std::fs::canonicalize(&dir).unwrap();
    let mut s = Session::new();
    s.root = Some(root.clone());
    add(&mut s, "sbs:uniform", "c", Json::obj(), Json::obj());
    run(&mut s, "output.add", Json::obj().with("node", "c").with("usage", "basecolor"));
    run(&mut s, "graph.set", Json::obj().with("default_format", 16).with("view", Json::obj().with("tiling", 3).with("displacement", 0.2)));
    run(&mut s, "graph.save", Json::obj().with("path", "m.sbs"));
    let mut t = Session::new();
    t.root = Some(root);
    run(&mut t, "graph.open", Json::obj().with("path", "m.sbs"));
    let o = t.execute("render.3d", &Json::obj().with("size", 32).with("tex", 32)).unwrap();
    let view = o.data.get("view").and_then(Json::as_str).unwrap().to_string();
    assert!(view.contains("tiling 3") && view.contains("height_scale 0.2"), "{view}");
    // a new node gets the saved format
    add(&mut t, "sbs:blur", "b", Json::obj(), Json::obj());
    let n = run(&mut t, "graph.outline", Json::obj()).to_text();
    assert!(n.contains("format"), "{n}");
}

#[test]
fn the_review_graph_panel_lays_out_many_nodes() {
    let mut s = Session::new();
    add(&mut s, "sbs:lib:noise_perlin_noise/perlin_noise", "n0", Json::obj(), Json::obj());
    let mut prev = "n0".to_string();
    for i in 1..30 {
        let name = format!("n{i}");
        add(&mut s, "sbs:blur", &name, Json::obj(), Json::obj().with("input1", prev.as_str()));
        prev = name;
    }
    run(&mut s, "output.add", Json::obj().with("node", prev.as_str()).with("usage", "basecolor"));
    let o = s.execute("render.review", &Json::obj().with("size", 128)).unwrap();
    assert_eq!(o.images.len(), 1);
    let outline = o.data.get("outline").and_then(Json::as_str).unwrap_or("");
    assert!(outline.contains("n29"), "{outline}");
}

#[test]
fn choice_lists_do_not_show_unused_entries() {
    let mut s = Session::new();
    add(&mut s, "sbs:lib:slope_blur/slope_blur_grayscale", "sb", Json::obj(), Json::obj());
    let d = run(&mut s, "node.describe", Json::obj().with("kind", "sbs:lib:slope_blur/slope_blur_grayscale")).to_text();
    assert!(!d.contains("(unused)") && !d.contains("(assumed)"), "{d}");
}

#[test]
fn curvature_smooth_has_three_outputs() {
    let mut s = Session::new();
    add(&mut s, "sbs:lib:curvature_smooth_v2/curvature_smooth_v2", "cs", Json::obj(), Json::obj());
    let d = run(&mut s, "node.describe", Json::obj().with("kind", "sbs:lib:curvature_smooth_v2/curvature_smooth_v2")).to_text();
    assert!(d.contains("convexity") && d.contains("concavity"), "{d}");
}

#[test]
fn new_macros_make_tiling_patterns() {
    let mut s = Session::new();
    for (m, name) in [("bricks", "br"), ("cracks", "cr"), ("plates", "pl"), ("fractured_rock", "fr"), ("moss", "mo"), ("strata", "st")] {
        run(&mut s, "macro.add", Json::obj().with("macro", m).with("name", name));
        let st = stats(&mut s, name, 128);
        assert!(!st.get("flat").and_then(Json::as_bool).unwrap_or(true), "{m}: {}", st.to_text());
        assert_eq!(st.get("tiles_seamlessly").and_then(Json::as_bool), Some(true), "{m}: {}", st.to_text());
    }
    let r = run(&mut s, "macro.add", Json::obj().with("macro", "edge_chip").with("input", "br").with("name", "ec"));
    assert!(r.get("outputs").and_then(|o| o.get("band")).is_some(), "{}", r.to_text());
    let r = run(&mut s, "macro.add", Json::obj().with("macro", "paint_chip").with("input", "st").with("name", "pc"));
    assert!(r.get("outputs").and_then(|o| o.get("highlight")).is_some() && r.get("outputs").and_then(|o| o.get("shadow")).is_some());
    run(&mut s, "macro.add", Json::obj().with("macro", "grow_organic").with("input", "cr").with("name", "go"));
    assert!(!stats(&mut s, "go", 128).get("flat").and_then(Json::as_bool).unwrap_or(true));
}

#[test]
fn flood_fill_gray_has_a_range_and_contrast() {
    let mut s = Session::new();
    add(&mut s, "sbs:lib:pattern_tile_generator/tile_generator", "t", Json::obj().with("pattern", "Disc").with("x_amount", 4).with("y_amount", 4), Json::obj());
    add(&mut s, "sbs:lib:flood_fill_2/flood_fill", "ff", Json::obj(), Json::obj().with("mask", "t"));
    run(&mut s, "macro.add", Json::obj().with("macro", "flood_gray").with("input", "ff").with("options", Json::obj().with("min", 0.3).with("max", 0.6)).with("name", "g"));
    let st = stats(&mut s, "g", 128);
    assert!(lane(&st, "max", 0) <= 0.61 && lane(&st, "max", 0) > 0.4, "{}", st.to_text());
}

#[test]
fn grunge_extras_change_the_picture_and_defaults_are_neutral() {
    const G: &str = "sbs:lib:grunge_concrete/grunge_concrete";
    let mut s = Session::new();
    add(&mut s, G, "base", Json::obj(), Json::obj());
    let sig = |j: &Json| format!("{:.5}/{:.5}/{:.5}", lane(j, "mean", 0), lane(j, "min", 0), lane(j, "max", 0));
    let base = sig(&stats(&mut s, "base", 64));
    for (k, v) in [("sharpen", 1.0), ("dirt_specks_opacity", 1.0), ("scratches_opacity", 1.0), ("base_noisiness", 0.0)] {
        add(&mut s, G, k, Json::obj().with(k, v), Json::obj());
        assert_ne!(sig(&stats(&mut s, k, 64)), base, "{k} had no effect");
    }
    add(&mut s, G, "same", Json::obj().with("sharpen", 0.5), Json::obj());
    let (a, b) = (stats(&mut s, "same", 64), stats(&mut s, "base", 64));
    assert!((lane(&a, "mean", 0) - lane(&b, "mean", 0)).abs() < 1e-9 && (lane(&a, "max", 0) - lane(&b, "max", 0)).abs() < 1e-9);
}

#[test]
fn library_parameters_are_grouped_like_the_original_panels() {
    use materialcraft::sbs_defs::lib_group;
    assert_eq!(lib_group("grunge_concrete", "grunge_concrete", "sharpen"), "Advanced");
    assert_eq!(lib_group("grunge_concrete", "grunge_concrete", "balance"), "");
    assert_eq!(lib_group("pattern_tile_generator", "tile_generator", "pattern_size"), "Size");
    assert_eq!(lib_group("pattern_tile_generator", "tile_generator", "rotation_random"), "Rotation");
    assert_eq!(lib_group("pattern_tile_generator", "tile_generator", "luminance_random"), "Color");
    assert_eq!(lib_group("pattern_tile_generator", "tile_generator", "global_offset"), "Position");
    assert_eq!(lib_group("pattern_tile_random", "tile_random", "split_mode"), "Split");
    assert_eq!(lib_group("pattern_tile_random", "tile_random", "interstice_amount"), "Interstice");
    assert_eq!(lib_group("noise_perlin_noise", "perlin_noise", "scale"), "");
}
