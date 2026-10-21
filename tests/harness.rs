// SPDX-License-Identifier: MIT OR Apache-2.0
//! The tools an assistant uses to check and steer its own work: the review sheet, the graph as text, expressions,
//! macros, subgraphs, numbered choices by name, 3D view options, and the fixes behind them.
use materialcraft::json::Json;
use materialcraft::session::Session;

fn run(s: &mut Session, cmd: &str, p: Json) -> Json {
    match s.execute(cmd, &p) {
        Ok(o) => o.data,
        Err(e) => panic!("{cmd} failed: {e}"),
    }
}

fn steps(s: &mut Session, json: &str) -> Json {
    run(s, "batch", Json::obj().with("steps", Json::parse(json).unwrap()))
}

fn num(j: &Json, path: &[&str]) -> f64 {
    let mut cur = j;
    for p in path {
        cur = cur.get(p).unwrap_or_else(|| panic!("no {p} in {}", j.to_text()));
    }
    cur.as_f64().unwrap_or_else(|| panic!("{path:?} is not a number"))
}

fn lane(j: &Json, key: &str, i: usize) -> f64 {
    j.get(key).and_then(Json::as_arr).and_then(|a| a.get(i)).and_then(Json::as_f64).unwrap_or_else(|| panic!("{key}[{i}] in {}", j.to_text()))
}

fn fails(s: &mut Session, cmd: &str, p: Json) -> String {
    match s.execute(cmd, &p) {
        Ok(o) => panic!("{cmd} should have failed but returned {}", o.data.to_text()),
        Err(e) => e,
    }
}

fn stats(s: &mut Session, node: &str, size: usize) -> Json {
    run(s, "render.node", Json::obj().with("node", node).with("size", size).with("image", false))
}

fn brick_wall(s: &mut Session) {
    steps(
        s,
        r#"[
        {"command":"node_add","params":{"kind":"sbs:lib:pattern_tile_generator/tile_generator","name":"bricks","params":{"x_amount":4,"y_amount":8,"position_offset":0.5,"luminance_random":0.4}}},
        {"command":"node_add","params":{"kind":"sbs:lib:noise_perlin_noise/perlin_noise","name":"grain","params":{"scale":16}}},
        {"command":"node_add","params":{"kind":"sbs:blend","name":"height","params":{"blendingmode":"Multiply","opacitymult":0.35},"inputs":{"source":"grain","destination":"bricks"}}},
        {"command":"node_add","params":{"kind":"sbs:uniform","name":"clay","params":{"outputcolor":[0.55,0.22,0.14,1]}}},
        {"command":"node_add","params":{"kind":"sbs:blend","name":"color","params":{"blendingmode":"Multiply"},"inputs":{"source":"height","destination":"clay"}}},
        {"command":"node_add","params":{"kind":"sbs:normal","name":"nrm","inputs":{"input1":"height"}}},
        {"command":"output_add","params":{"node":"color","usage":"basecolor"}},
        {"command":"output_add","params":{"node":"nrm","usage":"normal"}},
        {"command":"output_add","params":{"node":"height","usage":"height"}}
    ]"#,
    );
}

#[test]
fn numbered_settings_have_names_and_units() {
    let mut s = Session::new();
    let d = run(&mut s, "node.describe", Json::obj().with("kind", "sbs:lib:pattern_tile_generator/tile_generator"));
    let params = d.get("params").and_then(Json::as_arr).unwrap();
    let find = |n: &str| params.iter().find(|p| p.get("name").and_then(Json::as_str) == Some(n)).unwrap_or_else(|| panic!("{n}"));
    let pattern = find("pattern");
    let choices: Vec<&str> = pattern.get("choices").and_then(Json::as_arr).unwrap().iter().filter_map(Json::as_str).collect();
    assert_eq!(&choices[2..6], &["Square", "Disc", "Paraboloid", "Bell"]);
    assert_eq!(pattern.get("choice").and_then(Json::as_str), Some("Brick"));
    assert!(find("blending_mode").get("choices").is_some());
    assert!(find("rotation").get("unit").and_then(Json::as_str).unwrap().contains("turns"));
    // set by name, and the Gradient Map lists its colour keys
    run(&mut s, "node.add", Json::obj().with("kind", "sbs:lib:pattern_tile_generator/tile_generator").with("name", "t").with("params", Json::obj().with("pattern", "Bell").with("blending_mode", "Max")));
    let p = run(&mut s, "node.params", Json::obj().with("node", "t"));
    let pat = p.get("params").and_then(Json::as_arr).unwrap().iter().find(|x| x.get("name").and_then(Json::as_str) == Some("pattern")).cloned().unwrap();
    assert_eq!(pat.get("value").and_then(Json::as_f64), Some(5.0));
    let g = run(&mut s, "node.describe", Json::obj().with("kind", "sbs:gradient"));
    assert!(g.get("params").and_then(Json::as_arr).unwrap().iter().any(|x| x.get("name").and_then(Json::as_str) == Some("gradient_keys")));
    let v = run(&mut s, "node.describe", Json::obj().with("kind", "sbs:lib:noise_voronoi/voronoi"));
    assert!(v.to_text().contains("Chebyshev") && v.to_text().contains("F2 - F1"));
}

#[test]
fn engine_fixes_hold() {
    let mut s = Session::new();
    steps(
        &mut s,
        r#"[
        {"command":"node_add","params":{"kind":"sbs:lib:pattern_tile_generator/tile_generator","name":"tiles","params":{"x_amount":6,"y_amount":6,"pattern":"Disc","pattern_size":[0.8,0.8],"luminance_random":1}}},
        {"command":"node_add","params":{"kind":"sbs:distance","name":"dist","params":{"distance":20},"inputs":{"mask":"tiles"}}},
        {"command":"node_add","params":{"kind":"sbs:lib:bevel/bevel","name":"bev","params":{"distance":0.05},"inputs":{"input":"tiles"}}},
        {"command":"node_add","params":{"kind":"sbs:lib:noise_anisotropic_noise/anisotropic_noise","name":"soft","params":{"smoothness":1}}},
        {"command":"node_add","params":{"kind":"sbs:lib:noise_anisotropic_noise/anisotropic_noise","name":"hard","params":{"smoothness":0}}},
        {"command":"node_add","params":{"kind":"sbs:lib:noise_perlin_noise/perlin_noise","name":"warper","params":{"scale":4}}},
        {"command":"node_add","params":{"kind":"sbs:directionalwarp","name":"dw","params":{"intensity":40},"inputs":{"input1":"tiles","inputintensity":"warper"}}}
    ]"#,
    );
    // random luminance never goes below black
    let t = stats(&mut s, "tiles", 256);
    assert!(lane(&t, "min", 0) >= 0.0 && lane(&t, "max", 0) <= 1.0, "{}", t.to_text());
    // the format's Distance: 1 on the shapes, falling off outside
    let d = stats(&mut s, "dist", 256);
    assert!((lane(&d, "max", 0) - 1.0).abs() < 1e-3 && lane(&d, "mean", 0) > 0.3, "{}", d.to_text());
    let b = stats(&mut s, "bev", 256);
    assert!(lane(&b, "max", 0) > 0.9);
    // smoothness changes the anisotropic noise
    let (a, h) = (stats(&mut s, "soft", 128), stats(&mut s, "hard", 128));
    assert!(a.to_text() != h.to_text());
    // a warp moves the same distance at any size
    let w1 = stats(&mut s, "dw", 128);
    let w2 = stats(&mut s, "dw", 512);
    assert!((lane(&w1, "mean", 0) - lane(&w2, "mean", 0)).abs() < 0.03, "{} / {}", w1.to_text(), w2.to_text());
    // a bevel has no stair steps: neighbouring values along a row change smoothly
    let o = s.execute("render.node", &Json::obj().with("node", "bev").with("size", 256)).unwrap();
    assert_eq!(o.images.len(), 1);
}

#[test]
fn flood_fill_reports_its_regions() {
    let mut s = Session::new();
    steps(
        &mut s,
        r#"[
        {"command":"node_add","params":{"kind":"sbs:lib:pattern_tile_generator/tile_generator","name":"tiles","params":{"x_amount":4,"y_amount":4,"pattern":"Square","pattern_size":[0.9,0.9]}}},
        {"command":"node_add","params":{"kind":"sbs:lib:flood_fill_2/flood_fill","name":"ff","inputs":{"mask":"tiles"}}},
        {"command":"node_add","params":{"kind":"sbs:lib:flood_fill_2/flood_fill_to_random_grayscale","name":"rg","inputs":{"input":"ff"}}},
        {"command":"node_add","params":{"kind":"sbs:lib:flood_fill_2/flood_fill_to_gradient","name":"gr","inputs":{"input":"ff"},"params":{"angle_variation":1}}},
        {"command":"node_add","params":{"kind":"sbs:lib:pattern_tile_generator/tile_generator","name":"joined","params":{"x_amount":4,"y_amount":4,"pattern":"Square","pattern_size":[1,1]}}},
        {"command":"node_add","params":{"kind":"sbs:lib:flood_fill_2/flood_fill","name":"ff2","inputs":{"mask":"joined"}}}
    ]"#,
    );
    for size in [128, 256, 512] {
        let f = stats(&mut s, "rg", size);
        assert_eq!(num(&f, &["flood_fill", "regions"]), 16.0, "{size}: {}", f.to_text());
    }
    let g = stats(&mut s, "gr", 256);
    assert_eq!(num(&g, &["flood_fill", "regions"]), 16.0);
    let m = stats(&mut s, "ff2", 256);
    assert!(m.get("flood_fill").and_then(|f| f.get("warning")).is_some(), "{}", m.to_text());
}

#[test]
fn expressions_build_function_graphs() {
    let mut s = Session::new();
    run(&mut s, "node.add", Json::obj().with("kind", "sbs:pixelprocessor").with("name", "pp").with("params", Json::obj().with("colorswitch", 0)));
    let r = run(&mut s, "node.expression", Json::obj().with("node", "pp").with("param", "perpixel").with("expression", "$pos.x > 0.5 ? 1 : 0"));
    assert!(r.get("outline").and_then(Json::as_str).unwrap().contains("ifelse"));
    let st = stats(&mut s, "pp", 64);
    assert!((lane(&st, "mean", 0) - 0.5).abs() < 0.05, "{}", st.to_text());
    // a parameter computed from an expression
    run(&mut s, "node.add", Json::obj().with("kind", "sbs:blur").with("name", "b").with("inputs", Json::obj().with("input1", "pp")));
    run(&mut s, "node.expression", Json::obj().with("node", "b").with("param", "intensity").with("expression", "pow(2, 2) + 0.5"));
    let o = run(&mut s, "graph.outline", Json::obj());
    assert!(o.get("outline").and_then(Json::as_str).unwrap().contains("<function"));
    // mistakes are explained and change nothing
    let e = fails(&mut s, "node.expression", Json::obj().with("node", "pp").with("param", "perpixel").with("expression", "nothing + 1"));
    assert!(e.contains("unknown name"), "{e}");
    // the package still saves and reopens
    let text = run(&mut s, "graph.text", Json::obj());
    assert!(text.get("text").and_then(Json::as_str).unwrap().contains("ifelse"));
}

#[test]
fn macros_insert_standard_nodes() {
    let mut s = Session::new();
    let l = run(&mut s, "macro.add", Json::obj().with("macro", "list"));
    assert!(l.get("macros").and_then(Json::as_arr).unwrap().len() >= 8);
    run(&mut s, "node.add", Json::obj().with("kind", "sbs:lib:pattern_tile_generator/tile_generator").with("name", "t").with("params", Json::obj().with("pattern", "Disc").with("x_amount", 4).with("y_amount", 4).with("pattern_size", Json::Arr(vec![Json::from(0.5), Json::from(0.5)]))));
    let base = lane(&stats(&mut s, "t", 256), "mean", 0);
    run(&mut s, "macro.add", Json::obj().with("macro", "grow").with("input", "t").with("options", Json::obj().with("pixels", 6)).with("name", "g"));
    run(&mut s, "macro.add", Json::obj().with("macro", "shrink").with("input", "t").with("options", Json::obj().with("pixels", 6)).with("name", "sh"));
    let (g, sh) = (lane(&stats(&mut s, "g", 256), "mean", 0), lane(&stats(&mut s, "sh", 256), "mean", 0));
    assert!(g > base + 0.05 && sh < base - 0.05, "{sh} < {base} < {g}");
    for (m, opts) in [("star", Json::obj().with("points", 6)), ("polygon", Json::obj().with("sides", 8)), ("strokes", Json::obj().with("angle", 0.125))] {
        let r = run(&mut s, "macro.add", Json::obj().with("macro", m).with("options", opts));
        let id = r.get("output_node").and_then(Json::as_u32).unwrap();
        let st = run(&mut s, "render.node", Json::obj().with("node", id).with("size", 128).with("image", false));
        assert!(lane(&st, "max", 0) > 0.5 && lane(&st, "min", 0) < 0.1, "{m}: {}", st.to_text());
    }
    run(&mut s, "macro.add", Json::obj().with("macro", "tile_shape").with("input", "sh").with("options", Json::obj().with("x_amount", 2)).with("name", "ts"));
    run(&mut s, "macro.add", Json::obj().with("macro", "tile_scale").with("input", "t").with("options", Json::obj().with("times", 2)).with("name", "z"));
    let z = stats(&mut s, "z", 128);
    assert_eq!(z.get("tiles_seamlessly").and_then(Json::as_bool), Some(true), "{}", z.to_text());
}

#[test]
fn the_review_sheet_shows_and_measures_everything() {
    let mut s = Session::new();
    brick_wall(&mut s);
    let o = s.execute("render.review", &Json::obj().with("size", 160).with("tex", 128)).unwrap();
    assert_eq!(o.images.len(), 1);
    let outline = o.data.get("outline").and_then(Json::as_str).unwrap();
    assert!(outline.contains("blendingmode=3 (Multiply)") && outline.contains("outputs: "), "{outline}");
    assert!(o.data.get("maps").and_then(|m| m.get("basecolor")).is_some());
    // compare with a reference picture of the same material
    let dir = std::env::temp_dir().join(format!("mc_review_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let base = s.execute("render.node", &Json::obj().with("node", "color").with("size", 128)).unwrap();
    let ref_path = dir.join("ref.png");
    std::fs::write(&ref_path, &base.images[0].png).unwrap();
    let r = s.execute("render.review", &Json::obj().with("size", 128).with("tex", 128).with("reference", ref_path.display().to_string()).with("compare", "basecolor")).unwrap();
    let c = r.data.get("comparison").unwrap();
    assert!(num(c, &["similarity"]) > 0.9, "{}", c.to_text());
    // a darker reference gives a hint about brightness
    let dark = materialcraft::png::encode(64, 64, &vec![[0.05, 0.03, 0.02, 1.0]; 64 * 64], 8);
    let dark_path = dir.join("dark.png");
    std::fs::write(&dark_path, dark).unwrap();
    let r = s.execute("render.review", &Json::obj().with("size", 128).with("tex", 64).with("reference", dark_path.display().to_string()).with("thumbnails", false)).unwrap();
    let hints = r.data.get("comparison").and_then(|c| c.get("hints")).unwrap().to_text();
    assert!(hints.contains("brighter"), "{hints}");
    let _ = std::fs::remove_dir_all(&dir);
    // the review from the command line
    let pkg = run(&mut s, "graph.text", Json::obj());
    let dir2 = std::env::temp_dir().join(format!("mc_review_cli_{}", std::process::id()));
    std::fs::create_dir_all(&dir2).unwrap();
    std::fs::write(dir2.join("wall.sbs"), pkg.get("text").and_then(Json::as_str).unwrap()).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_materialcraft"))
        .args(["review", &dir2.join("wall.sbs").display().to_string(), &dir2.join("r.png").display().to_string(), "--size", "128", "--tex", "64", "--json", &dir2.join("r.json").display().to_string()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(dir2.join("r.png").exists());
    let j = Json::parse(&std::fs::read_to_string(dir2.join("r.json")).unwrap()).unwrap();
    assert!(j.get("outline").is_some());
    let _ = std::fs::remove_dir_all(&dir2);
}

#[test]
fn nodes_move_into_a_subgraph() {
    let mut s = Session::new();
    brick_wall(&mut s);
    let before = s.execute("render.outputs", &Json::obj().with("size", 64)).unwrap().images[0].png.clone();
    let r = run(&mut s, "graph.extract", Json::obj().with("nodes", Json::Arr(vec![Json::from("height"), Json::from("clay"), Json::from("color")])).with("name", "colorize").with("node_name", "sub"));
    assert_eq!(r.get("inputs").and_then(Json::as_arr).map(|a| a.len()), Some(2), "{}", r.to_text());
    assert_eq!(r.get("outputs").and_then(Json::as_arr).map(|a| a.len()), Some(2), "{}", r.to_text());
    let after = s.execute("render.outputs", &Json::obj().with("size", 64)).unwrap().images[0].png.clone();
    assert_eq!(before, after, "the material looks the same after the move");
    // the new graph is edited with graph=<name>; the names of the moved nodes came along
    let o = run(&mut s, "graph.outline", Json::obj().with("graph", "colorize"));
    assert!(o.get("outline").and_then(Json::as_str).unwrap().contains("\"clay\""), "{}", o.to_text());
    run(&mut s, "node.set", Json::obj().with("graph", "colorize").with("node", "clay").with("params", Json::obj().with("outputcolor", Json::Arr(vec![Json::from(0.2), Json::from(0.3), Json::from(0.6), Json::from(1.0)]))));
    let changed = s.execute("render.outputs", &Json::obj().with("size", 64)).unwrap().images[0].png.clone();
    assert_ne!(before, changed);
    let v = run(&mut s, "graph.validate", Json::obj());
    assert_eq!(v.get("ok").and_then(Json::as_bool), Some(true), "{}", v.to_text());
    // one undo puts everything back
    run(&mut s, "edit.undo", Json::obj());
    run(&mut s, "edit.undo", Json::obj());
    let back = s.execute("render.outputs", &Json::obj().with("size", 64)).unwrap().images[0].png.clone();
    assert_eq!(before, back);
}

#[test]
fn unused_nodes_are_warned_about() {
    let mut s = Session::new();
    brick_wall(&mut s);
    run(&mut s, "node.add", Json::obj().with("kind", "sbs:blur").with("name", "stray").with("inputs", Json::obj().with("input1", "grain")));
    let v = run(&mut s, "graph.validate", Json::obj());
    assert_eq!(v.get("ok").and_then(Json::as_bool), Some(true));
    let w = v.get("warnings").unwrap().to_text();
    assert!(w.contains("reaches no output"), "{w}");
}

#[test]
fn the_3d_view_takes_every_option() {
    let mut s = Session::new();
    brick_wall(&mut s);
    let l = run(&mut s, "render.3d", Json::obj().with("list", true));
    for k in ["environments", "light_rigs", "cameras", "shapes", "settings"] {
        assert!(l.get(k).is_some(), "{k}");
    }
    let o = s
        .execute(
            "render.3d",
            &Json::obj().with("size", 64).with("tex", 64).with("tiling", 3).with("displacement", 0.2).with("rig", "key_only").with("light_intensity", 2).with("env", "sunset").with("settings", Json::obj().with("post.bloom", 0).with("fov", 30)),
        )
        .unwrap();
    assert_eq!(o.images.len(), 1);
    let view = o.data.get("view").and_then(Json::as_str).unwrap();
    assert!(view.contains("tiling 3") && view.contains("height_scale 0.2") && view.contains("env.preset sunset"), "{view}");
    assert!(fails(&mut s, "render.3d", Json::obj().with("env", "mars")).contains("unknown environment"));
}
