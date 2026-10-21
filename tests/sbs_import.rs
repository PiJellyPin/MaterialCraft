// SPDX-License-Identifier: MIT OR Apache-2.0
//! Importing `.sbs` packages. The fixture below is written by hand for these tests from the published structure of
//! the format; it is not taken from any other program.
use materialcraft::graph::Graph;
use materialcraft::sbs::Package;
use materialcraft::sbs_import::{import_file, import_package, Level};
use std::path::Path;

mod sbs_fixture;
use sbs_fixture::*;

fn import() -> materialcraft::sbs_import::Imported {
    let pkg = Package::parse(&fixture()).expect("the fixture parses");
    import_package(pkg, Path::new("."), "Test").expect("the fixture imports")
}

fn by_kind<'a>(g: &'a Graph, kind: &str) -> Vec<(u32, &'a materialcraft::graph::Node)> {
    g.nodes.iter().filter(|(_, n)| n.kind == kind).map(|(i, n)| (*i, n)).collect()
}

#[test]
fn reads_the_package_structure() {
    let p = Package::parse(&fixture()).unwrap();
    assert_eq!(p.graphs.len(), 2);
    assert_eq!(p.graphs[1].path(), "Folder/Sub");
    assert_eq!(p.functions.len(), 1);
    assert_eq!(p.resources.len(), 1);
    assert!(p.resources[0].embedded.as_ref().map(|b| b.starts_with(b"\x89PNG")).unwrap_or(false));
    assert_eq!(p.graphs[0].nodes.len(), 17);
    assert_eq!(p.graphs[0].gui[0].text, "Hand written test & notes");
}

#[test]
fn imports_graphs_nodes_and_links() {
    let im = import();
    let g = &im.graph;
    assert_eq!(g.meta.get("identifier").map(String::as_str), Some("Main"));
    assert_eq!(g.size, 128, "the graph size comes from the file");
    assert!(g.subgraphs.contains_key("Sub"));
    assert!(g.subgraphs.contains_key("double_it"));
    assert_eq!(g.subgraphs["double_it"].meta.get("kind").map(String::as_str), Some("function"));
    // outputs with their usages
    let mut usages: Vec<String> = g.output_nodes().into_iter().map(|o| o.0).collect();
    usages.sort();
    assert_eq!(usages, ["basecolor", "normal", "roughness"]);
    // blend: multiply, foreground and background linked, opacity driven by a function
    let (bid, blend) = by_kind(g, "blend")[0];
    assert_eq!(blend.nums.get("mode"), Some(&3.0));
    assert_eq!(blend.nums.get("alpha_mode"), Some(&0.0), "the file's default alpha handling is carried over");
    assert!(blend.ins.contains_key("fg") && blend.ins.contains_key("bg"));
    let f = blend.strs.get("fn.opacity").expect("opacity is function-driven");
    assert!(g.subgraphs.contains_key(f));
    let _ = bid;
    // levels: the mid point becomes a gamma
    let (_, lv) = by_kind(g, "levels")[0];
    let gamma = lv.nums["gamma"];
    assert!((0.25f32.powf(1.0 / gamma) - 0.5).abs() < 1e-4, "mid 0.25 maps to 0.5, gamma {gamma}");
    // the value input became an Input Value node, the image input an input node
    assert!(by_kind(g, "input_value").iter().any(|(_, n)| n.strs.get("name").map(String::as_str) == Some("roughness_amount") && n.nums.get("v") == Some(&0.4)));
    assert_eq!(by_kind(g, "input").len(), 1);
    // the instance of Sub: parameter written into its Input Value node, output picked by name
    let (sid, sub) = by_kind(g, "subgraph")[0];
    assert_eq!(sub.strs.get("graph").map(String::as_str), Some("Sub"));
    let sg = &g.subgraphs["Sub"];
    let (amount_id, _) = by_kind(sg, "input_value").into_iter().find(|(_, n)| n.strs.get("name").map(String::as_str) == Some("amount")).unwrap();
    assert_eq!(sub.nums.get(&format!("{amount_id}.v")), Some(&0.7));
    assert_eq!(sub.ins.get("in0").is_some(), true, "the image input of the instance is linked");
    let reader = g.nodes.values().find(|n| n.ins.values().any(|v| *v == sid)).unwrap();
    assert!(reader.in_out.values().any(|o| o == "result"), "the link reads the named output");
    // library node: our own node with the parameters that have the same name
    let (_, perlin) = by_kind(g, "perlin")[0];
    assert_eq!(perlin.nums.get("scale"), Some(&8.0));
    // gradient keys
    let (_, grad) = by_kind(g, "colorize")[0];
    assert_eq!(grad.nums.get("p1"), Some(&1.0));
    assert_eq!(grad.nums.get("g1"), Some(&0.9));
    // embedded bitmap is carried in the package
    let (_, bm) = by_kind(g, "bitmap")[0];
    let file = bm.strs.get("file").unwrap();
    assert!(g.resources.contains_key(file));
    // transform: the stored inverse 0.5 means the image is shown twice as large
    let (_, tr) = by_kind(g, "transform")[0];
    assert!((tr.nums["scale_x"] - 2.0).abs() < 1e-5 && (tr.nums["offset_x"] - 0.25).abs() < 1e-6);
    // channel shuffle sources
    let (_, sh) = by_kind(g, "channel_shuffle")[0];
    assert_eq!((sh.nums["src_r"], sh.nums["src_b"]), (2.0, 0.0));
    // pixel processor runs a function graph
    let (_, pp) = by_kind(g, "pixel_processor")[0];
    assert!(g.subgraphs.contains_key(pp.strs.get("fn").unwrap()));
    // the comment
    assert_eq!(g.comments.len(), 1);
}

#[test]
fn reports_what_was_not_converted() {
    let im = import();
    let r = &im.report;
    assert_eq!(r.graphs, 2);
    assert_eq!(r.functions, 1);
    assert_eq!(r.nodes, 17 + 3);
    assert!(r.missing >= 1, "{}", r.to_text());
    assert!(r.approx >= 2, "{}", r.to_text());
    let text = r.to_text();
    assert!(text.contains("pattern map"), "{text}");
    assert!(text.contains("some_unknown_thing"), "{text}");
    assert!(text.contains("not_a_real_param"), "{text}");
    assert!(r.notes.iter().any(|n| n.level == Level::Missing));
    // the unknown library node keeps the chain connected
    let ph = im.graph.nodes.values().find(|n| n.strs.get("import_note").map(|s| s.contains("some_unknown_thing")).unwrap_or(false)).unwrap();
    assert_eq!(ph.kind, "dot");
    assert!(ph.ins.contains_key("in"));
}

#[test]
fn the_imported_package_renders_and_saves() {
    let im = import();
    let out = im.graph.evaluate(Some(64)).unwrap_or_else(|e| panic!("evaluation failed: {e}\n{}", im.graph.to_text()));
    // results are keyed by output name (the roughness output is called "rough" in the file)
    for k in ["basecolor", "rough", "normal"] {
        assert!(out.contains_key(k), "missing output {k}: {:?}", out.keys().collect::<Vec<_>>());
    }
    // the base colour is the multiply of a constant colour and the noise: not flat, and red-dominant
    let bc = &out["basecolor"];
    let avg = bc.px.iter().fold([0.0f32; 3], |a, p| [a[0] + p[0], a[1] + p[1], a[2] + p[2]]);
    assert!(avg[0] > avg[1] && avg[0] > avg[2], "{avg:?}");
    // every node evaluates on its own too (functions, pixel processor, subgraph, bitmap)
    for (id, n) in &im.graph.nodes {
        if let Err(e) = im.graph.evaluate_node(*id, 32) {
            panic!("node {id} ({}) failed: {e}", n.kind);
        }
    }
    // the text form reads back
    let text = im.graph.to_text();
    let back = Graph::parse_unchecked(&text).expect("saved package reads back");
    assert_eq!(back.nodes.len(), im.graph.nodes.len());
    assert_eq!(back.subgraphs.len(), im.graph.subgraphs.len());
    back.evaluate(Some(32)).expect("the saved package evaluates");
}

#[test]
fn the_roughness_input_drives_the_blend_opacity() {
    // roughness_amount 0.4 gives opacity 0.8; changing the input changes the result
    let im = import();
    let mut g = im.graph.clone();
    let a = g.evaluate(Some(32)).unwrap()["basecolor"].clone();
    let iv = g.nodes.values_mut().find(|n| n.kind == "input_value").unwrap();
    iv.nums.insert("v".into(), 0.0);
    let b = g.evaluate(Some(32)).unwrap()["basecolor"].clone();
    assert_ne!(a.px, b.px, "the graph input must reach the function-driven opacity");
}

#[test]
fn broken_files_are_errors_not_panics() {
    let f = fixture();
    let mut cuts: Vec<usize> = (0..f.len()).step_by(97).collect();
    cuts.push(f.len() - 1);
    for c in cuts {
        let mut end = c;
        while !f.is_char_boundary(end) {
            end -= 1;
        }
        if let Ok(p) = Package::parse(&f[..end]) {
            let _ = import_package(p, Path::new("."), "cut");
        }
    }
    assert!(Package::parse("<notapackage/>").is_err());
    assert!(Package::parse("").is_err());
    assert!(import_package(Package::parse("<package><content/></package>").unwrap(), Path::new("."), "empty").is_err());
}

#[test]
fn imports_from_a_file_with_linked_packages() {
    let dir = std::env::temp_dir().join(format!("mc_sbs_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // other.sbs holds a graph that main.sbs uses
    let other = "<package><identifier v=\"other\"/><content><graph><identifier v=\"Helper\"/><uid v=\"1\"/>\
        <graphOutputs><graphoutput><identifier v=\"out\"/><uid v=\"9\"/></graphoutput></graphOutputs>\
        <compNodes><compNode><uid v=\"1\"/><compImplementation><compFilter><filter v=\"uniform\"/><parameters><parameter><name v=\"outputcolor\"/><paramValue><constantValueFloat4 v=\"0 1 0 1\"/></paramValue></parameter></parameters></compFilter></compImplementation></compNode>\
        <compNode><uid v=\"2\"/><connections><connection><identifier v=\"inputNodeOutput\"/><connRef v=\"1\"/></connection></connections><compImplementation><compOutputBridge><output v=\"9\"/></compOutputBridge></compImplementation></compNode></compNodes>\
        </graph></content></package>";
    std::fs::write(dir.join("other.sbs"), other).unwrap();
    let main = "<package><identifier v=\"main\"/><dependencies><dependency><filename v=\"other.sbs\"/><uid v=\"5\"/></dependency></dependencies><content><graph><identifier v=\"Top\"/><uid v=\"1\"/>\
        <graphOutputs><graphoutput><identifier v=\"basecolor\"/><uid v=\"9\"/><usages><usage><name v=\"baseColor\"/></usage></usages></graphoutput></graphOutputs>\
        <compNodes><compNode><uid v=\"1\"/><compOutputs><compOutput><uid v=\"11\"/></compOutput></compOutputs><compImplementation><compInstance><path v=\"pkg:///Helper?dependency=5\"/><outputBridgings><outputBridging><uid v=\"11\"/><identifier v=\"out\"/></outputBridging></outputBridgings></compInstance></compImplementation></compNode>\
        <compNode><uid v=\"2\"/><connections><connection><identifier v=\"inputNodeOutput\"/><connRef v=\"1\"/><connRefOutput v=\"11\"/></connection></connections><compImplementation><compOutputBridge><output v=\"9\"/></compOutputBridge></compImplementation></compNode></compNodes>\
        </graph></content></package>";
    std::fs::write(dir.join("main.sbs"), main).unwrap();
    let im = import_file(&dir.join("main.sbs")).unwrap();
    let helper = im.graph.subgraphs.get("other_Helper").expect("the used graph comes along");
    assert_eq!(helper.meta.get("embedded").map(String::as_str), Some("1"));
    let out = im.graph.evaluate(Some(16)).unwrap();
    let px = out["basecolor"].px[0];
    assert!(px[1] > 0.9 && px[0] < 0.1, "{px:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_command_line_checks_and_renders_packages() {
    let dir = std::env::temp_dir().join(format!("mc_sbs_cli_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("test.sbs");
    std::fs::write(&src, fixture()).unwrap();
    let exe = env!("CARGO_BIN_EXE_materialcraft");
    let out = std::process::Command::new(exe).args(["check", src.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("rendering:") && text.contains("not converted"), "{text}");
    let o = std::process::Command::new(exe).args(["render", src.to_str().unwrap(), dir.join("maps").to_str().unwrap(), "--size", "64"]).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(dir.join("maps").read_dir().unwrap().count() >= 3);
    // the example package renders (the release builds check their command-line tool with it)
    let o = std::process::Command::new(exe).args(["example", dir.join("ex").to_str().unwrap()]).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let o = std::process::Command::new(exe).args(["render", dir.join("ex/brick_wall.sbs").to_str().unwrap(), dir.join("ex/out").to_str().unwrap(), "--size", "64"]).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(dir.join("ex/out").read_dir().unwrap().count(), 3);
    // only Substance packages are read
    std::fs::write(dir.join("old.mcg"), "size 64\nnode 1 perlin\noutput o 1\n").unwrap();
    let o = std::process::Command::new(exe).args(["render", dir.join("old.mcg").to_str().unwrap(), dir.join("maps2").to_str().unwrap()]).output().unwrap();
    assert!(!o.status.success() && String::from_utf8_lossy(&o.stderr).contains(".sbs"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------------------------------------------------------------ pattern maps

fn fx_data(uid: u32, ty: &str, params: &str) -> String {
    format!("<paramsGraphData><identifier v=\"d{uid}\"/><uid v=\"{uid}\"/><type v=\"{ty}\"/><parameters>{params}</parameters></paramsGraphData>")
}
fn fx_node(uid: u32, ty: &str, data: u32, outs: &[(&str, u32)]) -> String {
    let conns: String = outs.iter().map(|(o, to)| format!("<connection><identifier v=\"{o}\"/><connRef v=\"{to}\"/></connection>")).collect();
    format!("<paramsGraphNode><uid v=\"{uid}\"/><type v=\"{ty}\"/><data v=\"{data}\"/><connections>{conns}</connections><GUILayout><gpos v=\"0 {uid} 0\"/></GUILayout></paramsGraphNode>")
}
fn p_f2(name: &str, v: &str) -> String {
    format!("<parameter><name v=\"{name}\"/><paramValue><constantValueFloat2 v=\"{v}\"/></paramValue></parameter>")
}
fn fx_package(fx_params: &str, root: u32, datas: &str, nodes: &str) -> String {
    let fx = format!(
        "<compFilter><filter v=\"fxmaps\"/><parameters>{fx_params}</parameters><paramsGraphs><paramsGraph><name v=\"fxmap\"/><uid v=\"1\"/><rootnode v=\"{root}\"/><paramsGraphDatas>{datas}</paramsGraphDatas><paramsGraphNodes>{nodes}</paramsGraphNodes></paramsGraph></paramsGraphs></compFilter>"
    );
    let compnodes = [node(1, 0.0, &fx, ""), node(2, 150.0, "<compOutputBridge><output v=\"201\"/></compOutputBridge>", &conn("inputNodeOutput", 1))].join("");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<package><identifier v=\"Fx\"/><formatVersion v=\"1.1.0.0\"/>\
         <dependencies><dependency><filename v=\"?himself\"/><uid v=\"1\"/><type v=\"package\"/></dependency></dependencies>\
         <content><graph><identifier v=\"Main\"/><uid v=\"1000\"/>\
           <graphOutputs><graphoutput><identifier v=\"height\"/><uid v=\"201\"/><usages><usage><components v=\"R\"/><name v=\"height\"/></usage></usages></graphoutput></graphOutputs>\
           <compNodes>{compnodes}</compNodes>\
         </graph></content></package>"
    )
}
fn render_fx(text: &str) -> materialcraft::tex::Tex {
    let im = import_package(Package::parse(text).expect("parses"), Path::new("."), "Fx").expect("imports");
    let out = im.graph.evaluate(Some(64)).unwrap_or_else(|e| panic!("evaluation failed: {e}\n{}", im.graph.to_text()));
    out["height"].clone()
}
fn at(t: &materialcraft::tex::Tex, u: f32, v: f32) -> f32 {
    t.px[((v * t.h as f32) as usize).min(t.h - 1) * t.w + ((u * t.w as f32) as usize).min(t.w - 1)][0]
}

#[test]
fn pattern_maps_split_into_quarters() {
    // the root cell draws nothing; each of its four quarters draws a square half the quarter's size
    let gray = "<parameter><name v=\"colorswitch\"/><paramValue><constantValueBool v=\"0\"/></paramValue></parameter>";
    let datas = fx_data(21, "paramset", "") + &fx_data(22, "paramset", &(p_int("patterntype", 2) + &p_f2("patternsize", "0.5 0.5") + &p_int("blendingmode", 1)));
    let nodes = fx_node(11, "paramset", 21, &[("output0", 12), ("output1", 12), ("output2", 12), ("output3", 12)]) + &fx_node(12, "paramset", 22, &[]);
    let t = render_fx(&fx_package(gray, 11, &datas, &nodes));
    for (u, v) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
        assert!(at(&t, u, v) > 0.99, "square at {u} {v}: {}", at(&t, u, v));
    }
    for (u, v) in [(0.5, 0.5), (0.05, 0.05), (0.25, 0.45), (0.95, 0.6)] {
        assert!(at(&t, u, v) < 0.01, "empty at {u} {v}: {}", at(&t, u, v));
    }
}

#[test]
fn pattern_maps_repeat_with_the_number() {
    // a repeat of 4 discs whose offset is $number * 0.2 (the last one wraps around the edge)
    let offset = "<parameter><name v=\"frameoffset\"/><paramValue><dynamicValue><rootnode v=\"4\"/><paramNodes>\
        <paramNode><uid v=\"1\"/><function v=\"get_float1\"/><funcDatas><funcData><name v=\"get_float1\"/><constantValue><constantValueString v=\"$number\"/></constantValue></funcData></funcDatas></paramNode>\
        <paramNode><uid v=\"2\"/><function v=\"const_float1\"/><funcDatas><funcData><name v=\"const_float1\"/><constantValue><constantValueFloat1 v=\"0.2\"/></constantValue></funcData></funcDatas></paramNode>\
        <paramNode><uid v=\"3\"/><function v=\"mul\"/><connections><connection><identifier v=\"a\"/><connRef v=\"1\"/></connection><connection><identifier v=\"b\"/><connRef v=\"2\"/></connection></connections></paramNode>\
        <paramNode><uid v=\"5\"/><function v=\"const_float1\"/><funcDatas><funcData><name v=\"const_float1\"/><constantValue><constantValueFloat1 v=\"0\"/></constantValue></funcData></funcDatas></paramNode>\
        <paramNode><uid v=\"4\"/><function v=\"vector2\"/><connections><connection><identifier v=\"componentsin\"/><connRef v=\"3\"/></connection><connection><identifier v=\"componentslast\"/><connRef v=\"5\"/></connection></connections></paramNode>\
        </paramNodes></dynamicValue></paramValue></parameter>";
    let datas = fx_data(21, "addnode", &p_int("numberadded", 4)) + &fx_data(22, "paramset", &(p_int("patterntype", 3) + &p_f2("patternsize", "0.1 0.1") + &p_int("blendingmode", 1) + offset));
    let nodes = fx_node(11, "addnode", 21, &[("output1", 12)]) + &fx_node(12, "paramset", 22, &[]);
    let t = render_fx(&fx_package("", 11, &datas, &nodes));
    for u in [0.5, 0.7, 0.9, 0.1] {
        assert!(at(&t, u, 0.5) > 0.9, "disc at {u}: {}", at(&t, u, 0.5));
    }
    for u in [0.3, 0.6, 0.8] {
        assert!(at(&t, u, 0.5) < 0.05, "gap at {u}: {}", at(&t, u, 0.5));
    }
    assert!(at(&t, 0.5, 0.2) < 0.05);
}

#[test]
fn pattern_maps_select_branches_and_inherit_settings() {
    // a switch picks its second branch; the drawing node inherits its size from another data set
    let datas = fx_data(21, "markov2", &format!("<parameter><name v=\"switch\"/><paramValue><constantValueBool v=\"1\"/></paramValue></parameter>"))
        + &fx_data(22, "paramset", &(p_int("patterntype", 2) + &p_f2("patternsize", "0.5 0.5") + &p_int("blendingmode", 1)))
        + &fx_data(23, "paramset", &p_f2("frameoffset", "0.25 0"))
        + &format!("<paramsGraphData><identifier v=\"d24\"/><uid v=\"24\"/><type v=\"paramset\"/><inherit v=\"22\"/><parameters>{}</parameters></paramsGraphData>", p_f2("frameoffset", "-0.25 0"));
    // branch a: square moved right; branch b: (inherits the square) moved left
    let nodes = fx_node(11, "markov2", 21, &[("output0", 12), ("output1", 13)])
        + &format!("<paramsGraphNode><uid v=\"12\"/><type v=\"paramset\"/><data v=\"23\"/></paramsGraphNode>")
        + &fx_node(13, "paramset", 24, &[]);
    let t = render_fx(&fx_package("", 11, &datas, &nodes));
    assert!(at(&t, 0.25, 0.5) > 0.99, "{}", at(&t, 0.25, 0.5));
    assert!(at(&t, 0.75, 0.5) < 0.01, "{}", at(&t, 0.75, 0.5));
}
