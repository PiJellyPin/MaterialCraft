// SPDX-License-Identifier: MIT OR Apache-2.0
//! The command layer on `.sbs` packages: everything an assistant does works on the file's own nodes and values.
use materialcraft::json::Json;
use materialcraft::session::{self, Session};

fn run(s: &mut Session, cmd: &str, p: Json) -> Json {
    match s.execute(cmd, &p) {
        Ok(o) => o.data,
        Err(e) => panic!("{cmd} failed: {e}"),
    }
}

fn fails(s: &mut Session, cmd: &str, p: Json) -> String {
    match s.execute(cmd, &p) {
        Ok(o) => panic!("{cmd} should have failed but returned {}", o.data.to_text()),
        Err(e) => e,
    }
}

fn steps(json: &str) -> Json {
    Json::obj().with("steps", Json::parse(json).unwrap())
}

/// Perlin noise (library graph) through a Blend with a Uniform Color, a Normal, and two outputs.
fn sd_material(s: &mut Session) {
    run(
        s,
        "batch",
        steps(
            r#"[
        {"command":"node_add","params":{"kind":"sbs:lib:perlin_noise","name":"noise","params":{"scale":8}}},
        {"command":"node_add","params":{"kind":"sbs:uniform","name":"red","params":{"outputcolor":[0.8,0.2,0.1,1]}}},
        {"command":"node_add","params":{"kind":"sbs:blend","name":"mix","params":{"blendingmode":"Multiply","opacitymult":0.75},"inputs":{"source":"red","destination":"noise"}}},
        {"command":"node_add","params":{"kind":"sbs:normal","name":"nrm","inputs":{"input1":"noise"}}},
        {"command":"output_add","params":{"node":"mix","usage":"basecolor"}},
        {"command":"output_add","params":{"node":"nrm","usage":"normal"}}
    ]"#,
        ),
    );
}

#[test]
fn a_new_session_is_an_sbs_package() {
    let mut s = Session::new();
    let g = run(&mut s, "graph.inspect", Json::obj());
    assert_eq!(g.get("format").and_then(Json::as_str), Some("sbs"));
    run(&mut s, "graph.new", Json::obj().with("template", "material").with("size", 256).with("name", "wall"));
    let g = run(&mut s, "graph.inspect", Json::obj());
    assert_eq!(g.get("outputs").and_then(Json::as_arr).map(|a| a.len()), Some(6));
    assert_eq!(g.get("problems").and_then(Json::as_arr).map(|a| a.len()), Some(0), "{}", g.to_text());
    let o = s.execute("render.outputs", &Json::obj().with("size", 32)).unwrap();
    assert_eq!(o.images.len(), 1);
    // the older MaterialCraft format is gone
    assert!(fails(&mut s, "graph.new", Json::obj().with("format", "materialcraft")).contains("unknown parameter"));
}

#[test]
fn node_types_are_the_formats_own() {
    let mut s = Session::new();
    let t = run(&mut s, "node.types", Json::obj());
    let kinds: Vec<String> = t.get("nodes").and_then(Json::as_arr).unwrap().iter().filter_map(|n| n.get("kind").and_then(Json::as_str).map(str::to_string)).collect();
    assert!(kinds.iter().any(|k| k == "sbs:blend"));
    assert!(kinds.iter().any(|k| k.starts_with("sbs:lib:")));
    assert!(kinds.iter().all(|k| k.starts_with("sbs:")), "only the format's node types are offered");
    let d = run(&mut s, "node.describe", Json::obj().with("kind", "sbs:blend"));
    let params = d.get("params").and_then(Json::as_arr).unwrap();
    let mode = params.iter().find(|p| p.get("name").and_then(Json::as_str) == Some("blendingmode")).unwrap();
    assert!(mode.get("choices").and_then(Json::as_arr).map(|c| c.len() >= 10).unwrap_or(false));
    let f = run(&mut s, "node.types", Json::obj().with("family", "function"));
    assert!(f.get("count").and_then(Json::as_u32).unwrap_or(0) > 50);
    // an MaterialCraft node does not go into an .sbs package
    let e = fails(&mut s, "node.add", Json::obj().with("kind", "voronoi"));
    assert!(e.contains("unknown node type"), "{e}");
}

#[test]
fn builds_renders_saves_and_reopens_a_material() {
    let dir = std::env::temp_dir().join(format!("mc_agent_sbs_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let root = std::fs::canonicalize(&dir).unwrap();
    let mut s = Session::new();
    s.root = Some(root.clone());
    sd_material(&mut s);
    let v = run(&mut s, "graph.validate", Json::obj());
    assert_eq!(v.get("ok").and_then(Json::as_bool), Some(true), "{}", v.to_text());
    let o = s.execute("render.node", &Json::obj().with("node", "mix").with("size", 32)).unwrap();
    assert_eq!(o.images.len(), 1);
    assert_eq!(o.data.get("flat").and_then(Json::as_bool), Some(false));
    // values are checked: a typo is explained, a choice is taken by name
    let e = fails(&mut s, "node.set", Json::obj().with("node", "mix").with("params", Json::obj().with("opacity", 1)));
    assert!(e.contains("opacitymult"), "{e}");
    // a link to an input the node does not have is refused
    let e = fails(&mut s, "node.connect", Json::obj().with("from", "noise").with("to", "mix").with("slot", "fg"));
    assert!(e.contains("source"), "{e}");
    run(&mut s, "graph.save", Json::obj().with("path", "wall.sbs"));
    let text = std::fs::read_to_string(root.join("wall.sbs")).unwrap();
    assert!(text.contains("<filter v=\"blend\"/>") && text.contains("sbs://noise_perlin_noise.sbs"), "{text}");
    materialcraft::sbs::Package::parse(&text).expect("a valid package");
    let mut t = Session::new();
    t.root = Some(root.clone());
    run(&mut t, "graph.open", Json::obj().with("path", "wall.sbs"));
    let g = run(&mut t, "graph.inspect", Json::obj());
    let nodes = g.get("nodes").and_then(Json::as_arr).unwrap();
    let blend = nodes.iter().find(|n| n.get("kind").and_then(Json::as_str) == Some("sbs:blend")).unwrap();
    assert_eq!(blend.get("params").and_then(|p| p.get("blendingmode")).and_then(Json::as_f64), Some(3.0));
    assert_eq!(blend.get("params").and_then(|p| p.get("opacitymult")).and_then(Json::as_f64), Some(0.75));
    // saving the reopened package unchanged writes the same file
    run(&mut t, "graph.save", Json::obj().with("path", "again.sbs"));
    assert_eq!(std::fs::read_to_string(root.join("again.sbs")).unwrap(), text);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn values_can_be_computed_by_function_graphs() {
    let mut s = Session::new();
    sd_material(&mut s);
    let r = run(&mut s, "node.drive", Json::obj().with("node", "mix").with("param", "opacitymult"));
    let f = r.get("function_graph").and_then(Json::as_str).unwrap().to_string();
    // the function: 0.25 + 0.25
    run(
        &mut s,
        "batch",
        Json::obj().with(
            "steps",
            Json::Arr(vec![
                Json::obj().with("command", "node_add").with("params", Json::obj().with("graph", f.as_str()).with("kind", "sbs:fn:const_float1").with("name", "a").with("params", Json::obj().with("const_float1", 0.25))),
                Json::obj().with("command", "node_add").with("params", Json::obj().with("graph", f.as_str()).with("kind", "sbs:fn:add").with("name", "sum").with("inputs", Json::obj().with("a", "a").with("b", "a"))),
                Json::obj().with("command", "node_set_result").with("params", Json::obj().with("graph", f.as_str()).with("node", "sum")),
            ]),
        ),
    );
    let fg = run(&mut s, "graph.inspect", Json::obj().with("graph", f.as_str()));
    assert_eq!(fg.get("kind").and_then(Json::as_str), Some("function"));
    assert_eq!(fg.get("problems").and_then(Json::as_arr).map(|a| a.len()), Some(0), "{}", fg.to_text());
    // a texture node does not go into a function graph
    let e = fails(&mut s, "node.add", Json::obj().with("graph", f.as_str()).with("kind", "sbs:blur"));
    assert!(e.contains("function graph"), "{e}");
    let text = run(&mut s, "graph.text", Json::obj());
    let text = text.get("text").and_then(Json::as_str).unwrap();
    assert!(text.contains("<dynamicValue>") && text.contains("<function v=\"add\"/>"), "{text}");
    // it still renders
    let o = s.execute("render.node", &Json::obj().with("node", "mix").with("size", 32)).unwrap();
    assert_eq!(o.images.len(), 1);
    // and can be stopped
    run(&mut s, "node.drive", Json::obj().with("node", "mix").with("param", "opacitymult").with("stop", true));
    let g = run(&mut s, "graph.inspect", Json::obj());
    let nodes = g.get("nodes").and_then(Json::as_arr).unwrap();
    let mix = nodes.iter().find(|n| n.get("name").and_then(Json::as_str) == Some("mix")).unwrap();
    assert!(mix.get("params").and_then(|p| p.get("opacitymult")).is_none());
}

#[test]
fn graphs_of_the_package_are_made_used_and_managed() {
    let mut s = Session::new();
    sd_material(&mut s);
    run(&mut s, "graph.create", Json::obj().with("name", "detail").with("kind", "material"));
    run(&mut s, "graph.create", Json::obj().with("name", "helper").with("kind", "function"));
    let l = run(&mut s, "graph.list", Json::obj());
    let names: Vec<&str> = l.get("graphs").and_then(Json::as_arr).unwrap().iter().filter_map(|g| g.get("name").and_then(Json::as_str)).collect();
    assert!(names.contains(&"detail") && names.contains(&"helper"), "{names:?}");
    // the material graph runs as a node of the main graph, with a named output
    let r = run(&mut s, "node.add", Json::obj().with("kind", "sbs:graph").with("name", "inst").with("params", Json::obj().with("graph", "detail")));
    assert!(r.get("id").is_some());
    let p = run(&mut s, "node.params", Json::obj().with("node", "inst"));
    assert!(p.get("outputs").and_then(Json::as_arr).map(|o| o.len() == 6).unwrap_or(false), "{}", p.to_text());
    run(&mut s, "node.connect", Json::obj().with("from", "inst").with("output", "roughness").with("to", "nrm"));
    assert!(fails(&mut s, "node.connect", Json::obj().with("from", "inst").with("output", "nope").with("to", "nrm")).contains("roughness"));
    // a graph that is used cannot go; renaming follows
    assert!(fails(&mut s, "graph.delete", Json::obj().with("graph", "detail")).contains("used"));
    run(&mut s, "graph.rename", Json::obj().with("graph", "detail").with("name", "fine_detail"));
    let g = run(&mut s, "graph.inspect", Json::obj());
    assert!(g.to_text().contains("fine_detail"));
    run(&mut s, "graph.delete", Json::obj().with("graph", "helper"));
    // nodes in other graphs, by name
    run(&mut s, "node.add", Json::obj().with("graph", "fine_detail").with("kind", "sbs:blur").with("name", "soft"));
    let g = run(&mut s, "graph.inspect", Json::obj().with("graph", "fine_detail"));
    assert_eq!(g.get("nodes").and_then(Json::as_arr).map(|a| a.len()), Some(13));
    let o = s.execute("render.outputs", &Json::obj().with("graph", "fine_detail").with("size", 32)).unwrap();
    assert_eq!(o.images.len(), 1);
}

#[test]
fn parameters_can_be_exposed() {
    let mut s = Session::new();
    sd_material(&mut s);
    run(&mut s, "graph.expose", Json::obj().with("label", "Tint strength").with("node", "mix").with("param", "opacitymult").with("min", 0).with("max", 1));
    let g = run(&mut s, "graph.inspect", Json::obj());
    let params = g.get("parameters").and_then(Json::as_arr).unwrap();
    assert_eq!(params[0].get("identifier").and_then(Json::as_str), Some("tint_strength"));
    let text = run(&mut s, "graph.text", Json::obj());
    let text = text.get("text").and_then(Json::as_str).unwrap();
    assert!(text.contains("<paraminput>") && text.contains("get_float1"), "{text}");
    let o = s.execute("render.node", &Json::obj().with("node", "mix").with("size", 32)).unwrap();
    assert_eq!(o.images.len(), 1);
    run(&mut s, "graph.unexpose", Json::obj().with("label", "Tint strength"));
    let g = run(&mut s, "graph.inspect", Json::obj());
    assert!(g.get("parameters").is_none());
}

#[test]
fn hostile_parameters_never_panic_on_packages() {
    let junk = [Json::Null, Json::Bool(true), Json::Num(-1.0), Json::Num(1e300), Json::Num(4_294_967_296.0), Json::Str(String::new()), Json::Str("x\"y#z\n<&>".into()), Json::Arr(vec![Json::Null, Json::Num(1.0)]), Json::Obj(vec![("a".into(), Json::Arr(vec![]))])];
    for cmd in session::COMMANDS {
        if matches!(cmd.id, "graph.save" | "export" | "graph.open" | "graph.add_file") {
            continue;
        }
        let mut s = Session::new();
        sd_material(&mut s);
        for j in &junk {
            for a in cmd.all_args() {
                let p = Json::obj().with(a.name, j.clone());
                if let Err(e) = s.execute(cmd.id, &p) {
                    assert!(!e.starts_with("internal error"), "{} {} = {}: {e}", cmd.id, a.name, j.to_text());
                }
            }
        }
        // the package still saves as a readable file
        let t = s.execute("graph.text", &Json::obj()).unwrap();
        materialcraft::sbs_native::load(t.data.get("text").and_then(Json::as_str).unwrap(), None).unwrap_or_else(|e| panic!("after {}: {e}", cmd.id));
    }
}

#[test]
fn graph_parameters_take_new_defaults() {
    let mut s = Session::new();
    sd_material(&mut s);
    run(&mut s, "graph.expose", Json::obj().with("label", "Tint strength").with("node", "mix").with("param", "opacitymult").with("min", 0).with("max", 1));
    let png = |s: &mut Session| s.execute("render.node", &Json::obj().with("node", "mix").with("size", 32)).unwrap().images[0].png.clone();
    let before = png(&mut s);
    run(&mut s, "graph.set_input", Json::obj().with("input", "Tint strength").with("value", 0.0).with("label", "Tint").with("min", 0).with("max", 2));
    assert_ne!(png(&mut s), before, "the new default is what renders");
    let g = run(&mut s, "graph.inspect", Json::obj());
    let p = &g.get("parameters").and_then(Json::as_arr).unwrap()[0];
    assert_eq!(p.get("label").and_then(Json::as_str), Some("Tint"), "{}", p.to_text());
    let text = run(&mut s, "graph.text", Json::obj());
    let text = text.get("text").and_then(Json::as_str).unwrap();
    assert!(text.contains(r#"<name v="max"/><value v="2"/>"#), "{text}");
    // by identifier too; mistakes are explained
    run(&mut s, "graph.set_input", Json::obj().with("input", "tint_strength").with("value", 0.75));
    assert_eq!(png(&mut s), before);
    assert!(fails(&mut s, "graph.set_input", Json::obj().with("input", "nope").with("value", 1)).contains("no input"));
    assert!(fails(&mut s, "graph.set_input", Json::obj().with("input", "tint_strength").with("min", 3).with("max", 1)).contains("min"));
    assert!(fails(&mut s, "graph.set_input", Json::obj().with("input", "tint_strength")).contains("at least one"));
}

#[test]
fn graphs_of_another_package_come_over() {
    let dir = std::env::temp_dir().join(format!("mc_agent_sbs_add_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let root = std::fs::canonicalize(&dir).unwrap();
    // a package with two graphs, one running the other, a driven value and a graph parameter
    let mut a = Session::new();
    a.root = Some(root.clone());
    sd_material(&mut a);
    run(&mut a, "graph.expose", Json::obj().with("label", "Tint").with("node", "mix").with("param", "opacitymult"));
    let d = run(&mut a, "node.drive", Json::obj().with("node", "nrm").with("param", "intensity"));
    assert!(d.get("function_graph").is_some(), "{}", d.to_text());
    run(&mut a, "graph.create", Json::obj().with("name", "detail").with("kind", "material"));
    run(&mut a, "node.add", Json::obj().with("kind", "sbs:graph").with("name", "inst").with("params", Json::obj().with("graph", "detail")));
    run(&mut a, "graph.save", Json::obj().with("path", "a.sbs"));
    // added twice: the second copy's graphs get new names, nodes follow
    let mut b = Session::new();
    b.root = Some(root.clone());
    sd_material(&mut b);
    let first = run(&mut b, "graph.add_file", Json::obj().with("path", "a.sbs"));
    assert_eq!(first.get("with_graphs").and_then(Json::as_u32), Some(2), "{}", first.to_text());
    let second = run(&mut b, "graph.add_file", Json::obj().with("path", "a.sbs"));
    let main2 = second.get("added").and_then(Json::as_str).unwrap().to_string();
    assert_ne!(main2, first.get("added").and_then(Json::as_str).unwrap());
    let l = run(&mut b, "graph.list", Json::obj());
    let names: Vec<String> = l.get("graphs").and_then(Json::as_arr).unwrap().iter().filter_map(|g| g.get("name").and_then(Json::as_str).map(str::to_string)).collect();
    assert!(names.contains(&"detail".to_string()) && names.contains(&"detail_2".to_string()), "{names:?}");
    let g = run(&mut b, "graph.inspect", Json::obj().with("graph", main2.as_str()));
    assert!(g.to_text().contains("detail_2"), "the copy runs its own copy of the graph: {}", g.to_text());
    let o = s_render(&mut b, &main2);
    assert_eq!(o, 1);
    // saved and read back: every graph is there once, with its own uid
    run(&mut b, "graph.save", Json::obj().with("path", "b.sbs"));
    let text = std::fs::read_to_string(root.join("b.sbs")).unwrap();
    let (pkg, _) = materialcraft::sbs_native::load(&text, None).unwrap();
    let graphs = materialcraft::sbs_native::all_graphs(&pkg);
    let uids: std::collections::BTreeSet<String> = graphs.values().filter(|g| materialcraft::sbs_native::listed(g)).filter_map(|g| g.sd.as_ref().map(|s| s.uid.clone())).collect();
    assert_eq!(uids.len(), graphs.values().filter(|g| materialcraft::sbs_native::listed(g)).count());
    assert_eq!(graphs.values().filter(|g| materialcraft::sbs_native::listed(g)).count(), 5);
    // one graph saved on its own takes what it needs along
    let one = materialcraft::sbs_native::extract_package(&pkg, &main2).unwrap();
    let one_text = materialcraft::sbs_native::save_text(&one).unwrap();
    let (back, _) = materialcraft::sbs_native::load(&one_text, None).unwrap();
    let kept: Vec<String> = materialcraft::sbs_native::all_graphs(&back).into_iter().filter(|(_, g)| materialcraft::sbs_native::listed(g)).map(|(k, _)| k).collect();
    assert_eq!(kept.len(), 2, "{kept:?}");
    assert!(one_text.contains("dynamicValue"), "the driven value came along");
    let _ = std::fs::remove_dir_all(&dir);
}

fn s_render(s: &mut Session, graph: &str) -> usize {
    s.execute("render.outputs", &Json::obj().with("graph", graph).with("size", 32)).unwrap().images.len()
}

#[test]
fn graphs_run_each_other_but_not_in_circles() {
    let mut s = Session::new();
    sd_material(&mut s);
    run(&mut s, "graph.create", Json::obj().with("name", "detail").with("kind", "material"));
    let main = run(&mut s, "graph.inspect", Json::obj()).get("graph").and_then(Json::as_str).map(str::to_string).unwrap_or_default();
    // the second graph can run the first one
    run(&mut s, "graph.focus", Json::obj().with("graph", "detail"));
    run(&mut s, "node.add", Json::obj().with("kind", "sbs:graph").with("name", "first").with("params", Json::obj().with("graph", main.as_str())));
    let o = s.execute("render.node", &Json::obj().with("node", "first").with("size", 32)).unwrap();
    assert_eq!(o.images.len(), 1);
    // and then the first cannot run the second
    run(&mut s, "graph.focus", Json::obj());
    let e = fails(&mut s, "node.add", Json::obj().with("kind", "sbs:graph").with("params", Json::obj().with("graph", "detail")));
    assert!(e.contains("cannot run inside"), "{e}");
    // nor a graph itself
    let e = fails(&mut s, "node.add", Json::obj().with("kind", "sbs:graph").with("params", Json::obj().with("graph", main.as_str())));
    assert!(e.contains("cannot run inside") || e.contains("no graph"), "{e}");
    // what the second graph holds is still there after going back
    let g = run(&mut s, "graph.inspect", Json::obj().with("graph", "detail"));
    assert!(g.to_text().contains("\"first\""), "{}", g.to_text());
}

#[test]
fn renaming_a_graph_keeps_its_driven_values() {
    let mut s = Session::new();
    sd_material(&mut s);
    run(&mut s, "graph.create", Json::obj().with("name", "detail").with("kind", "material"));
    run(&mut s, "graph.focus", Json::obj().with("graph", "detail"));
    run(&mut s, "node.add", Json::obj().with("kind", "sbs:blur").with("name", "soft"));
    let r = run(&mut s, "node.drive", Json::obj().with("node", "soft").with("param", "intensity"));
    let f = r.get("function_graph").and_then(Json::as_str).unwrap().to_string();
    assert!(f.starts_with("detail/"), "{f}");
    run(&mut s, "graph.focus", Json::obj());
    run(&mut s, "graph.rename", Json::obj().with("graph", "detail").with("name", "fine"));
    // a new graph called like the old one does not take its functions
    run(&mut s, "graph.create", Json::obj().with("name", "detail").with("kind", "material"));
    let text = run(&mut s, "graph.text", Json::obj());
    let text = text.get("text").and_then(Json::as_str).unwrap();
    let (pkg, _) = materialcraft::sbs_native::load(text, None).unwrap();
    let fine = &pkg.subgraphs["fine"];
    let soft = fine.nodes.values().find(|n| n.kind == "sbs:blur").unwrap();
    let v = materialcraft::sbs_native::values(soft);
    assert!(matches!(v.get("intensity"), Some(materialcraft::sbs_native::PVal::Dyn { graph, .. }) if graph.starts_with("fine/")), "{v:?}");
}

#[test]
fn gradient_map_keys_are_set_and_saved() {
    let mut s = Session::new();
    sd_material(&mut s);
    run(&mut s, "node.add", Json::obj().with("kind", "sbs:gradient").with("name", "ramp").with("inputs", Json::obj().with("input1", "noise")));
    let png = |s: &mut Session| s.execute("render.node", &Json::obj().with("node", "ramp").with("size", 32)).unwrap().images[0].png.clone();
    let plain = png(&mut s);
    let keys = Json::parse(r#"[{"position":0,"color":[0.2,0.05,0.0,1]},[0.6,1,0.5,0.1,1],{"position":1,"color":0.9}]"#).unwrap();
    run(&mut s, "node.set", Json::obj().with("node", "ramp").with("params", Json::obj().with("gradient_keys", keys)));
    assert_ne!(png(&mut s), plain, "the keys are what renders");
    let p = run(&mut s, "node.params", Json::obj().with("node", "ramp"));
    let k = p.get("gradient_keys").and_then(Json::as_arr).unwrap();
    assert_eq!(k.len(), 3);
    assert_eq!(k[1].get("position").and_then(Json::as_f64), Some(0.6));
    let text = run(&mut s, "graph.text", Json::obj());
    let text = text.get("text").and_then(Json::as_str).unwrap();
    assert!(text.contains(r#"<name v="gradientrgba"/>"#) && text.contains(r#"<constantValueFloat4 v="0.9 0.9 0.9 1"/>"#), "{text}");
    // read back the same
    let (pkg, _) = materialcraft::sbs_native::load(text, None).unwrap();
    let ramp = pkg.nodes.values().find(|n| n.kind == "sbs:gradient").unwrap();
    assert_eq!(materialcraft::sbs_native::gradient_keys(ramp).len(), 3);
    assert!(fails(&mut s, "node.set", Json::obj().with("node", "ramp").with("params", Json::obj().with("gradient_keys", "red"))).contains("gradient_keys"));
    assert!(fails(&mut s, "node.set", Json::obj().with("node", "ramp").with("params", Json::obj().with("gradient_keys", Json::Arr(vec![])))).contains("1 to 64"));
}

#[test]
fn fx_maps_come_with_a_pattern_graph() {
    let mut s = Session::new();
    let r = run(&mut s, "node.add", Json::obj().with("kind", "sbs:fxmaps").with("name", "fx"));
    let fx = r.get("fx_graph").and_then(Json::as_str).unwrap().to_string();
    let g = run(&mut s, "graph.inspect", Json::obj().with("graph", fx.as_str()));
    assert_eq!(g.get("nodes").and_then(Json::as_arr).map(|a| a.len()), Some(1), "{}", g.to_text());
    let quad = g.get("nodes").and_then(Json::as_arr).unwrap()[0].get("id").and_then(Json::as_u32).unwrap();
    // a square pattern, half size
    run(&mut s, "node.set", Json::obj().with("graph", fx.as_str()).with("node", quad).with("params", Json::obj().with("patterntype", "Square").with("patternsize", Json::Arr(vec![Json::from(0.5), Json::from(0.5)]))));
    let o = s.execute("render.node", &Json::obj().with("node", "fx").with("size", 32)).unwrap();
    assert_eq!(o.data.get("flat").and_then(Json::as_bool), Some(false), "{}", o.data.to_text());
    let text = run(&mut s, "graph.text", Json::obj());
    let text = text.get("text").and_then(Json::as_str).unwrap();
    assert!(text.contains("<paramsGraph>") && text.contains(r#"<type v="paramset"/>"#), "{text}");
    let (pkg, _) = materialcraft::sbs_native::load(text, None).unwrap();
    let back = pkg.subgraphs.values().find(|g| g.meta.get("kind").map(|k| k == "fxmap").unwrap_or(false)).expect("the pattern graph is read back");
    assert_eq!(back.nodes.len(), 1);
}

#[test]
fn new_graphs_go_into_their_folders() {
    let mut s = Session::new();
    sd_material(&mut s);
    run(&mut s, "graph.create", Json::obj().with("name", "mats/stone/detail").with("kind", "material"));
    run(&mut s, "graph.create", Json::obj().with("name", "mats/helper").with("kind", "function"));
    let text = run(&mut s, "graph.text", Json::obj());
    let text = text.get("text").and_then(Json::as_str).unwrap();
    assert!(text.contains(r#"<group><identifier v="mats"/>"#), "{text}");
    let (pkg, _) = materialcraft::sbs_native::load(text, None).unwrap();
    let names: Vec<String> = materialcraft::sbs_native::all_graphs(&pkg).into_iter().filter(|(_, g)| materialcraft::sbs_native::listed(g)).map(|(k, _)| k).collect();
    assert!(names.contains(&"mats/stone/detail".to_string()) && names.contains(&"mats/helper".to_string()), "{names:?}");
}
