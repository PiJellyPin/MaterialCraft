// SPDX-License-Identifier: MIT OR Apache-2.0
//! `.sbs` packages edited as they are: reading, saving with only the changes written, and rendering through the
//! importer.
use materialcraft::graph::{Graph, Node};
use materialcraft::sbs_defs::PTy;
use materialcraft::sbs_native::{self, PVal, Role};
use materialcraft::xmldoc::XDoc;
use std::path::Path;

mod sbs_fixture;
use sbs_fixture::*;

fn open(text: &str) -> Graph {
    let (g, warnings) = sbs_native::load(text, None).expect("the package reads");
    assert!(warnings.is_empty(), "{warnings:?}");
    g
}

/// The fixture laid out one element per line, as files usually are.
fn pretty(text: &str) -> String {
    let mut d = XDoc::parse(text).unwrap();
    let ind = materialcraft::xmldoc::Indent { unit: Some(" ".into()), newline: "\n".into() };
    ind.apply(&mut d.root, 0);
    d.to_text()
}

fn val(n: &Node, k: &str) -> PVal {
    PVal::dec(n.strs.get(k).unwrap_or_else(|| panic!("no value {k}"))).unwrap()
}

#[test]
fn reads_graphs_nodes_values_and_links() {
    let g = open(&fixture());
    assert_eq!(g.meta.get("identifier").map(String::as_str), Some("Main"));
    for name in ["Folder/Sub", "double_it", "Main/3/opacitymult", "Main/10/perpixel", "Folder/Sub/2/levelouthigh", "Main/13/fx0"] {
        assert!(g.subgraphs.contains_key(name), "{name}: {:?}", g.subgraphs.keys().collect::<Vec<_>>());
    }
    // node ids are the uids of the file
    assert_eq!(g.nodes[&1].kind, "sbs:uniform");
    assert_eq!(g.nodes[&2].kind, "sbs:lib:noise_perlin_noise/perlin_noise");
    assert_eq!(g.nodes[&3].kind, "sbs:blend");
    assert_eq!(g.nodes[&11].kind, "sbs:graph");
    assert_eq!(g.nodes[&11].strs.get("#graph").map(String::as_str), Some("Folder/Sub"));
    assert_eq!(g.nodes[&6].kind, "sbs:input");
    assert_eq!(g.nodes[&7].kind, "sbs:output");
    // values keep their type and text
    assert_eq!(val(&g.nodes[&1], "outputcolor"), PVal::Const { ty: PTy::F(4), raw: "0.8 0.2 0.1 1".into(), rel: None });
    assert_eq!(val(&g.nodes[&3], "blendingmode"), PVal::Const { ty: PTy::I(1), raw: "3".into(), rel: None });
    assert_eq!(val(&g.nodes[&2], "scale"), PVal::Const { ty: PTy::F(1), raw: "8".into(), rel: Some("0".into()) });
    assert_eq!(val(&g.nodes[&3], "opacitymult"), PVal::Dyn { graph: "Main/3/opacitymult".into(), rel: None });
    // links
    assert_eq!(g.nodes[&3].ins.get("source"), Some(&1));
    assert_eq!(g.nodes[&3].ins.get("destination"), Some(&2));
    assert_eq!(g.nodes[&8].ins.get("inputNodeOutput"), Some(&11));
    // function graphs keep their nodes and result
    let f = &g.subgraphs["Main/3/opacitymult"];
    assert_eq!(f.nodes[&3].kind, "sbs:fn:mul");
    assert_eq!(f.nodes[&3].ins.get("a"), Some(&1));
    assert_eq!(f.sd.as_ref().unwrap().root, Some(3));
    assert!(matches!(f.sd.as_ref().unwrap().role, Role::Param { node: 3, .. }));
    assert_eq!(val(&f.nodes[&1], "get_float1"), PVal::Const { ty: PTy::S, raw: "roughness_amount".into(), rel: None });
    // graph data
    let sd = g.sd.as_ref().unwrap();
    assert_eq!(sd.outputs.len(), 3);
    assert_eq!(sd.output_usage("201").as_deref(), Some("baseColor"));
    assert_eq!(g.size, 128);
    assert_eq!(g.comments.len(), 1);
    assert_eq!(g.comments[0].text, "Hand written test & notes");
}

#[test]
fn an_unchanged_package_saves_to_the_same_bytes() {
    for text in [fixture(), pretty(&fixture())] {
        let g = open(&text);
        assert_eq!(sbs_native::save_text(&g).unwrap(), text);
        // moving nothing, setting a value to what it was and touching nothing else changes nothing either
        let mut g2 = g.clone();
        let p = g2.nodes[&3].pos;
        g2.nodes.get_mut(&3).unwrap().pos = p;
        assert_eq!(sbs_native::save_text(&g2).unwrap(), text);
    }
}

#[test]
fn a_changed_value_rewrites_only_that_value() {
    let text = pretty(&fixture());
    let mut g = open(&text);
    g.nodes.get_mut(&3).unwrap().strs.insert("blendingmode".into(), PVal::Const { ty: PTy::I(1), raw: "9".into(), rel: None }.enc());
    let out = sbs_native::save_text(&g).unwrap();
    assert_eq!(out, text.replace("<constantValueInt32 v=\"3\"/>", "<constantValueInt32 v=\"9\"/>"));
    let back = open(&out);
    assert_eq!(val(&back.nodes[&3], "blendingmode"), PVal::Const { ty: PTy::I(1), raw: "9".into(), rel: None });
    // a value the file did not set yet is added, one set back to its default (removed) disappears
    let mut g = open(&text);
    g.nodes.get_mut(&5).unwrap().strs.insert("inversedy".into(), PVal::Const { ty: PTy::B, raw: "1".into(), rel: None }.enc());
    g.nodes.get_mut(&5).unwrap().strs.remove("intensity");
    let out = sbs_native::save_text(&g).unwrap();
    let back = open(&out);
    assert!(back.nodes[&5].strs.contains_key("inversedy"));
    assert!(!back.nodes[&5].strs.contains_key("intensity"));
    // everything else is as it was
    let a = XDoc::parse(&out).unwrap();
    let b = XDoc::parse(&text).unwrap();
    let main = |d: &XDoc| d.root.path(&["content", "graph", "compNodes"]).unwrap().elems().filter(|n| n.v("uid").as_deref() != Some("5")).map(|n| n.to_xml()).collect::<Vec<_>>();
    assert_eq!(main(&a), main(&b));
}

#[test]
fn moved_added_and_removed_nodes_are_written() {
    let text = pretty(&fixture());
    let mut g = open(&text);
    g.nodes.get_mut(&4).unwrap().pos = [100.0 * sbs_native::POS_SCALE, 200.0 * sbs_native::POS_SCALE];
    // a new Levels node fed by the uniform colour
    let mut n = Node { kind: "sbs:levels".into(), pos: [0.0, 400.0], ..Default::default() };
    n.ins.insert("input1".into(), 1);
    n.strs.insert("levelinmid".into(), PVal::Const { ty: PTy::F(4), raw: "0.3 0.3 0.3 0.3".into(), rel: None }.enc());
    g.nodes.insert(500, n);
    g.nodes.remove(&17);
    let out = sbs_native::save_text(&g).unwrap();
    let d = XDoc::parse(&out).unwrap();
    let nodes = d.root.path(&["content", "graph", "compNodes"]).unwrap();
    let find = |u: &str| nodes.elems().find(|n| n.v("uid").as_deref() == Some(u)).cloned();
    assert!(find("17").is_none());
    assert_eq!(find("4").unwrap().path(&["GUILayout"]).unwrap().v("gpos").as_deref(), Some("100 200 0"));
    let new = find("500").expect("the new node is written");
    assert_eq!(new.path(&["compImplementation", "compFilter"]).unwrap().v("filter").as_deref(), Some("levels"));
    let c = new.path(&["connections", "connection"]).unwrap();
    assert_eq!(c.v("connRef").as_deref(), Some("1"));
    assert_eq!(c.v("connRefOutput").as_deref(), Some("10"));
    assert!(new.child("compOutputs").is_some());
    // it reads back the same way
    let back = open(&out);
    assert_eq!(back.nodes[&500].kind, "sbs:levels");
    assert_eq!(back.nodes[&500].ins.get("input1"), Some(&1));
    assert_eq!(val(&back.nodes[&500], "levelinmid"), PVal::Const { ty: PTy::F(4), raw: "0.3 0.3 0.3 0.3".into(), rel: None });
    assert!(!back.nodes.contains_key(&17));
    // a link moved to another node
    let mut g = open(&text);
    g.nodes.get_mut(&3).unwrap().ins.insert("source".into(), 16);
    let back = open(&sbs_native::save_text(&g).unwrap());
    assert_eq!(back.nodes[&3].ins.get("source"), Some(&16));
}

#[test]
fn function_graphs_are_written_back() {
    let text = pretty(&fixture());
    let mut g = open(&text);
    // the opacity function multiplies by 3 now
    let f = g.subgraphs.get_mut("Main/3/opacitymult").unwrap();
    f.nodes.get_mut(&2).unwrap().strs.insert("const_float1".into(), PVal::Const { ty: PTy::F(1), raw: "3".into(), rel: None }.enc());
    // and the package function adds instead of multiplying
    g.subgraphs.get_mut("double_it").unwrap().nodes.get_mut(&3).unwrap().kind = "sbs:fn:add".into();
    let out = sbs_native::save_text(&g).unwrap();
    let back = open(&out);
    assert_eq!(val(&back.subgraphs["Main/3/opacitymult"].nodes[&2], "const_float1"), PVal::Const { ty: PTy::F(1), raw: "3".into(), rel: None });
    assert_eq!(back.subgraphs["double_it"].nodes[&3].kind, "sbs:fn:add");
    assert_eq!(back.subgraphs["double_it"].sd.as_ref().unwrap().root, Some(3));
    // the sub graph's function was not touched
    let sub = |t: &str| XDoc::parse(t).unwrap().root.path(&["content", "group", "content", "graph"]).unwrap().to_xml();
    assert_eq!(sub(&out), sub(&text));
}

#[test]
fn graphs_can_be_renamed_and_removed() {
    let text = pretty(&fixture());
    let mut g = open(&text);
    let sub = g.subgraphs.remove("Folder/Sub").unwrap();
    g.subgraphs.insert("Folder/Renamed".into(), sub);
    g.nodes.get_mut(&11).unwrap().strs.insert("#graph".into(), "Folder/Renamed".into());
    g.subgraphs.remove("double_it");
    let out = sbs_native::save_text(&g).unwrap();
    let back = open(&out);
    assert!(back.subgraphs.contains_key("Folder/Renamed"));
    assert!(!back.subgraphs.contains_key("double_it"));
    assert_eq!(back.nodes[&11].strs.get("#graph").map(String::as_str), Some("Folder/Renamed"));
    assert!(!out.contains("double_it"));
}

#[test]
fn the_package_renders_like_the_import() {
    let g = open(&fixture());
    let low = sbs_native::lower(&g).expect("lowers");
    // the ids of the file's nodes stay
    assert_eq!(low.graph.nodes[&3].kind, "blend");
    assert_eq!(low.graph.nodes[&5].kind, "normal");
    let a = low.graph.evaluate(Some(64)).expect("renders");
    let pkg = materialcraft::sbs::Package::parse(&fixture()).unwrap();
    let imp = materialcraft::sbs_import::import_package(pkg, Path::new("."), "Test").unwrap();
    let b = imp.graph.evaluate(Some(64)).unwrap();
    assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
    for (k, t) in &a {
        assert_eq!(t.px, b[k].px, "output {k}");
    }
    // the evaluation entry points take native graphs directly
    let c = g.evaluate(Some(64)).expect("a native graph renders");
    assert_eq!(c["basecolor"].px, a["basecolor"].px);
    let outs = g.output_nodes();
    assert!(outs.iter().any(|o| o.0 == "basecolor" && o.2 == 7), "{outs:?}");
}

#[test]
fn a_new_package_saves_and_reads_back() {
    let mut g = sbs_native::new_package("my_material", 256);
    assert!(g.sd.is_some());
    let mut n = Node { kind: "sbs:uniform".into(), ..Default::default() };
    n.strs.insert("outputcolor".into(), PVal::Const { ty: PTy::F(4), raw: "1 0 0 1".into(), rel: None }.enc());
    g.nodes.insert(7, n);
    let text = sbs_native::save_text(&g).unwrap();
    let back = open(&text);
    assert_eq!(back.meta.get("identifier").map(String::as_str), Some("my_material"));
    assert_eq!(back.nodes[&7].kind, "sbs:uniform");
    assert_eq!(back.size, 256);
    // Designer needs the graph's own output size, every compFilter a parameters element and a grey node comptype 2
    assert!(text.contains("<baseParameters>") && text.contains("outputsize") && text.contains("defaultParentSize"), "{text}");
    assert!(text.contains("<parameters>"), "{text}");
    // and opens as an ordinary package
    materialcraft::sbs::Package::parse(&text).expect("valid");
}

#[test]
fn the_library_lists_every_node_once() {
    use materialcraft::sbs_defs::{palette, Fam};
    for fam in [Fam::Texture, Fam::Function, Fam::Pattern] {
        let mut labels = std::collections::HashSet::new();
        let mut kinds = std::collections::HashSet::new();
        for d in palette(fam) {
            // the editor finds a dropped or searched node by its name
            assert!(labels.insert(d.label), "'{}' is listed twice ({})", d.label, d.kind);
            assert!(kinds.insert(d.kind), "{} is listed twice", d.kind);
        }
    }
    // an old and a new version of the same library graph are one entry, the one with its settings
    let tex = palette(Fam::Texture);
    for name in ["Tile Generator", "Tile Sampler Color", "Splatter Circular", "Weave 1", "Cells 1"] {
        let d = tex.iter().find(|d| d.label == name).unwrap_or_else(|| panic!("{name} is in the library"));
        assert!(!d.params.is_empty(), "{name} ({}) has no settings", d.kind);
    }
}

#[test]
fn every_library_node_draws_its_picture() {
    use materialcraft::sbs_defs::{palette, Fam};
    let mut none = Vec::new();
    for d in palette(Fam::Texture) {
        if matches!(d.kind, "sbs:output" | "sbs:graph" | "sbs:input") {
            continue;
        }
        match sbs_native::kind_preview(d.kind, 24) {
            Some(t) => {
                assert_eq!((t.w, t.h), (24, 24), "{}", d.kind);
                assert!(t.px.iter().all(|p| p.iter().all(|v| v.is_finite())), "{} has non-finite values", d.kind);
            }
            None => none.push(d.kind),
        }
    }
    // a bitmap or SVG needs its file and a value processor its function before they make anything
    assert_eq!(none, vec!["sbs:bitmap", "sbs:svg", "sbs:valueprocessor"], "no picture");
}

#[test]
fn broken_packages_are_errors_not_panics() {
    for bad in ["", "<package>", "<notapackage/>", "<package><content/></package>"] {
        assert!(sbs_native::load(bad, None).is_err(), "{bad:?}");
    }
    // damaged pieces are skipped
    let odd = fixture().replace("<connRef v=\"1\"/>", "<connRef v=\"999999\"/>");
    let (_, w) = sbs_native::load(&odd, None).unwrap();
    assert!(!w.is_empty());
}

/// Packages saved by the format's own editor (kept privately, never committed): `MC_SBS_SAMPLES=<folder>`.
#[test]
fn real_packages_save_unchanged_and_render() {
    let Ok(dir) = std::env::var("MC_SBS_SAMPLES") else { return };
    let mut n = 0;
    for e in std::fs::read_dir(&dir).expect("the folder") {
        let p = e.unwrap().path();
        if p.extension().map(|x| x != "sbs").unwrap_or(true) {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap();
        let (g, w) = sbs_native::load(&text, Some(p.clone())).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        println!("{}: {} graphs, warnings {:?}", p.display(), g.subgraphs.len() + 1, w);
        assert_eq!(sbs_native::save_text(&g).unwrap(), text, "{}", p.display());
        for (name, sg) in sbs_native::all_graphs(&g) {
            if sg.sd.as_ref().map(|s| s.role == Role::Graph).unwrap_or(false) && !sg.output_nodes().is_empty() {
                let view = g.graph_view(if name == *g.meta.get("identifier").unwrap() { None } else { Some(name.as_str()) }).unwrap();
                let r = view.evaluate(Some(64)).unwrap_or_else(|e| panic!("{} {name}: {e}", p.display()));
                println!("  {name}: {:?}", r.keys().collect::<Vec<_>>());
            }
        }
        n += 1;
    }
    println!("{n} packages");
}
