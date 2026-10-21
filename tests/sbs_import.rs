// SPDX-License-Identifier: MIT OR Apache-2.0
//! Import regressions use copies of assets/all_nodes.sbs, with targeted edits for
//! errors, linked packages and FX-map behavior. Full coverage is in sbs_all_nodes.rs.
use materialcraft::sbs::{Impl, Package};
use materialcraft::sbs_import::{import_file, import_package, Level};
use materialcraft::xmldoc::{XDoc, XNode};
use std::path::Path;

const SOURCE: &str = include_str!("assets/all_nodes.sbs");

#[test]
fn reports_what_was_not_converted() {
    let mut pkg = Package::parse(SOURCE).expect("all_nodes.sbs parses");
    let node = pkg.graphs[0].nodes.iter_mut().find(|n| n.uid == "1584632881").unwrap();
    let Impl::Filter { filter, .. } = &mut node.imp else { panic!("expected Blend filter") };
    *filter = "some_unknown_thing".into();
    let imported = import_package(pkg, Path::new("."), "all_nodes").unwrap();
    let note = imported.report.notes.iter().find(|n| n.node == Some(1584632881) && n.level == Level::Missing).expect("unknown filter must have a diagnostic identifying its node");
    assert!(note.text.contains("some_unknown_thing"), "{}", note.text);
    assert_eq!(note.graph, "Substance_graph");
    assert!(imported.graph.nodes[&1584632881].strs["import_note"].contains("some_unknown_thing"));
}

#[test]
fn broken_files_are_errors_not_panics() {
    // Sample truncations across the real file, including its final byte.
    let cuts = (0..SOURCE.len()).step_by((SOURCE.len() / 40).max(1)).chain(std::iter::once(SOURCE.len() - 1));
    for mut end in cuts {
        while !SOURCE.is_char_boundary(end) {
            end -= 1;
        }
        if let Ok(p) = Package::parse(&SOURCE[..end]) {
            let _ = import_package(p, Path::new("."), "cut");
        }
    }
    assert!(Package::parse("<notapackage/>").is_err());
    assert!(Package::parse("").is_err());
    assert!(import_package(Package::parse("<package><content/></package>").unwrap(), Path::new("."), "empty").is_err());
}

#[test]
fn imports_from_a_file_with_linked_packages() {
    let dir = std::env::temp_dir().join(format!("mc_sbs_linked_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("other.sbs"), SOURCE).unwrap();
    let mut doc = XDoc::parse(SOURCE).unwrap();
    let dependency = XDoc::parse("<dependency><filename v=\"other.sbs\"/><uid v=\"42\"/><type v=\"package\"/></dependency>").unwrap().root;
    doc.root.child_mut("dependencies").unwrap().children.push(XNode::Elem(dependency));
    let nodes = doc.root.path_mut(&["content", "graph", "compNodes"]).unwrap();
    let uniform = nodes.elems_mut().find(|n| n.v("uid").as_deref() == Some("1359211355")).unwrap();
    *uniform.child_mut("compImplementation").unwrap() = XDoc::parse("<compImplementation><compInstance><path v=\"pkg:///Substance_graph?dependency=42\"/><outputBridgings><outputBridging><uid v=\"1359211356\"/><identifier v=\"basecolor\"/></outputBridging></outputBridgings></compInstance></compImplementation>").unwrap().root;
    std::fs::write(dir.join("main.sbs"), doc.to_text()).unwrap();
    let im = import_file(&dir.join("main.sbs")).expect("all_nodes.sbs with an external graph instance imports");
    let helper = im.graph.subgraphs.get("other_Substance_graph").expect("the linked native graph is included");
    assert_eq!(helper.meta.get("embedded").map(String::as_str), Some("1"));
    let out = im.graph.evaluate(Some(16)).expect("external graph output renders");
    assert_eq!(out["basecolor"].px[0], [0.5, 0.5, 0.5, 1.0]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_command_line_checks_and_renders_packages() {
    let dir = std::env::temp_dir().join(format!("mc_sbs_cli_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("test.sbs");
    std::fs::write(&src, SOURCE).unwrap();
    let exe = env!("CARGO_BIN_EXE_materialcraft");
    let out = std::process::Command::new(exe).args(["check", src.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("rendering:") && text.contains("not converted"), "{text}");
    let o = std::process::Command::new(exe).args(["render", src.to_str().unwrap(), dir.join("maps").to_str().unwrap(), "--size", "64"]).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(dir.join("maps").read_dir().unwrap().count() == 7);
    // the example package renders (the release builds check their command-line tool with it)
    let o = std::process::Command::new(exe).args(["example", dir.join("ex").to_str().unwrap()]).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let o =
        std::process::Command::new(exe).args(["render", dir.join("ex/brick_wall.sbs").to_str().unwrap(), dir.join("ex/out").to_str().unwrap(), "--size", "64"]).output().unwrap();
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
    // Edit the actual FX-map node in a complete copy of all_nodes.sbs.
    let fx = format!("<compFilter><filter v=\"fxmaps\"/><parameters>{fx_params}</parameters><paramsGraphs><paramsGraph><name v=\"fxmap\"/><uid v=\"1584632951\"/><rootnode v=\"{root}\"/><paramsGraphDatas>{datas}</paramsGraphDatas><paramsGraphNodes>{nodes}</paramsGraphNodes></paramsGraph></paramsGraphs></compFilter>");
    let mut doc = XDoc::parse(SOURCE).unwrap();
    let node = doc.root.path_mut(&["content", "graph", "compNodes"]).unwrap().elems_mut().find(|n| n.v("uid").as_deref() == Some("1584632949")).unwrap();
    *node.path_mut(&["compImplementation", "compFilter"]).unwrap() = XDoc::parse(&fx).unwrap().root;
    doc.to_text()
}
fn p_int(name: &str, value: i32) -> String {
    format!("<parameter><name v=\"{name}\"/><paramValue><constantValueInt32 v=\"{value}\"/></paramValue></parameter>")
}
fn render_fx(text: &str) -> materialcraft::tex::Tex {
    let im = import_package(Package::parse(text).expect("all_nodes.sbs FX edit parses"), Path::new("."), "all_nodes").expect("all_nodes.sbs FX edit imports");
    im.graph.evaluate_node(1584632949, 64).unwrap_or_else(|e| panic!("all_nodes.sbs: FX-map node 1584632949 failed: {e}"))
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
    let datas = fx_data(21, "addnode", &p_int("numberadded", 4))
        + &fx_data(22, "paramset", &(p_int("patterntype", 3) + &p_f2("patternsize", "0.1 0.1") + &p_int("blendingmode", 1) + offset));
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
        + &format!(
            "<paramsGraphData><identifier v=\"d24\"/><uid v=\"24\"/><type v=\"paramset\"/><inherit v=\"22\"/><parameters>{}</parameters></paramsGraphData>",
            p_f2("frameoffset", "-0.25 0")
        );
    // branch a: square moved right; branch b: (inherits the square) moved left
    let nodes = fx_node(11, "markov2", 21, &[("output0", 12), ("output1", 13)])
        + &format!("<paramsGraphNode><uid v=\"12\"/><type v=\"paramset\"/><data v=\"23\"/></paramsGraphNode>")
        + &fx_node(13, "paramset", 24, &[]);
    let t = render_fx(&fx_package("", 11, &datas, &nodes));
    assert!(at(&t, 0.25, 0.5) > 0.99, "{}", at(&t, 0.25, 0.5));
    assert!(at(&t, 0.75, 0.5) < 0.01, "{}", at(&t, 0.75, 0.5));
}
