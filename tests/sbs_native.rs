// SPDX-License-Identifier: MIT OR Apache-2.0
//! Focused editing regressions on the checked-in package. Full import/export and
//! node compatibility live in sbs_all_nodes.rs.
use materialcraft::graph::{Graph, Node};
use materialcraft::sbs_defs::PTy;
use materialcraft::sbs_native::{self, PVal};
use materialcraft::xmldoc::XDoc;

const SOURCE: &str = include_str!("assets/all_nodes.sbs");
const UNIFORM: u32 = 1359211355;
const NORMAL: u32 = 1359211383;
const LEVELS: u32 = 1584633036;
const FUNCTION: &str = "Substance_graph/1584843868/outputsize";
const FX: &str = "Substance_graph/1584632949/fx0";

fn open(text: &str) -> Graph {
    let (g, warnings) = sbs_native::load(text, None).expect("all_nodes.sbs or its edited copy must import");
    assert!(warnings.is_empty(), "all_nodes.sbs load warnings: {warnings:?}");
    g
}

fn val(n: &Node, k: &str) -> PVal {
    PVal::dec(n.strs.get(k).unwrap_or_else(|| panic!("{}: no value {k}", n.kind))).unwrap()
}

#[test]
fn a_changed_value_rewrites_only_that_value() {
    let mut g = open(SOURCE);
    let value = PVal::Const { ty: PTy::F(4), raw: "0.8 0.2 0.1 1".into(), rel: Some("0".into()) };
    g.nodes.get_mut(&UNIFORM).unwrap().strs.insert("outputcolor".into(), value.enc());
    let out = sbs_native::save_text(&g).unwrap();
    let expected = SOURCE.replacen("<constantValueFloat4 v=\"0.5 0.5 0.5 1\"/>", "<constantValueFloat4 v=\"0.8 0.2 0.1 1\"/>", 1);
    assert!(out == expected, "all_nodes.sbs: editing node {UNIFORM} changed unrelated XML");
    assert_eq!(val(&open(&out).nodes[&UNIFORM], "outputcolor"), value);

    let mut g = open(SOURCE);
    g.nodes.get_mut(&NORMAL).unwrap().strs.insert("inversedy".into(), PVal::Const { ty: PTy::B, raw: "1".into(), rel: None }.enc());
    g.nodes.get_mut(&NORMAL).unwrap().strs.remove("input2alpha");
    let out = sbs_native::save_text(&g).unwrap();
    let back = open(&out);
    assert!(back.nodes[&NORMAL].strs.contains_key("inversedy"));
    assert!(!back.nodes[&NORMAL].strs.contains_key("input2alpha"));
    let other_nodes = |text: &str| {
        XDoc::parse(text)
            .unwrap()
            .root
            .path(&["content", "graph", "compNodes"])
            .unwrap()
            .elems()
            .filter(|n| n.v("uid").as_deref() != Some("1359211383"))
            .map(|n| n.to_xml())
            .collect::<Vec<_>>()
    };
    assert!(other_nodes(&out) == other_nodes(SOURCE), "all_nodes.sbs: adding/removing a parameter on {NORMAL} changed other nodes");
}

#[test]
fn moved_added_and_removed_nodes_are_written() {
    let mut g = open(SOURCE);
    g.nodes.get_mut(&LEVELS).unwrap().pos = [100.0 * sbs_native::POS_SCALE, 200.0 * sbs_native::POS_SCALE];
    let mut n = Node { kind: "sbs:levels".into(), pos: [0.0, 400.0], ..Default::default() };
    n.ins.insert("input1".into(), UNIFORM);
    let value = PVal::Const { ty: PTy::F(4), raw: "0.3 0.3 0.3 0.3".into(), rel: None };
    n.strs.insert("levelinmid".into(), value.enc());
    assert!(g.nodes.insert(500, n).is_none(), "test node ID must be unused");
    g.nodes.remove(&1584633107); // Disconnected Warp.
                                 // Move the basecolor output's existing connection to a different Uniform Color.
    g.nodes.get_mut(&1213284337).unwrap().ins.insert("inputNodeOutput".into(), 1359211391);
    g.nodes.get_mut(&1213284337).unwrap().in_out.remove("inputNodeOutput");
    let out = sbs_native::save_text(&g).unwrap();
    let d = XDoc::parse(&out).unwrap();
    let nodes = d.root.path(&["content", "graph", "compNodes"]).unwrap();
    let find = |u: &str| nodes.elems().find(|n| n.v("uid").as_deref() == Some(u)).cloned().unwrap_or_else(|| panic!("all_nodes.sbs: node {u} missing from export"));
    assert_eq!(find("1584633036").path(&["GUILayout"]).unwrap().v("gpos").as_deref(), Some("100 200 0"));
    let new = find("500");
    assert_eq!(new.path(&["compImplementation", "compFilter"]).unwrap().v("filter").as_deref(), Some("levels"));
    let c = new.path(&["connections", "connection"]).unwrap();
    assert_eq!(c.v("connRef").as_deref(), Some("1359211355"));
    assert_eq!(c.v("connRefOutput").as_deref(), Some("1359211356"));
    assert!(new.child("compOutputs").is_some());
    let back = open(&out);
    assert_eq!(back.nodes[&500].kind, "sbs:levels");
    assert_eq!(back.nodes[&500].ins.get("input1"), Some(&UNIFORM));
    assert_eq!(val(&back.nodes[&500], "levelinmid"), value);
    assert!(!back.nodes.contains_key(&1584633107));
    assert_eq!(back.nodes[&1213284337].ins.get("inputNodeOutput"), Some(&1359211391));
}

#[test]
fn function_graphs_are_written_back() {
    let mut g = open(SOURCE);
    let f = g.subgraphs.get_mut(FUNCTION).expect("all_nodes.sbs: embedded function graph");
    let value = PVal::Const { ty: PTy::F(1), raw: "3".into(), rel: None };
    f.nodes.get_mut(&1584844554).unwrap().strs.insert("const_float1".into(), value.enc());
    f.nodes.get_mut(&1584844890).unwrap().kind = "sbs:fn:add".into();
    // The original catalog has no result; setting one must survive export too.
    f.sd.as_mut().unwrap().root = Some(1584844559);
    let back = open(&sbs_native::save_text(&g).unwrap());
    assert_eq!(val(&back.subgraphs[FUNCTION].nodes[&1584844554], "const_float1"), value);
    assert_eq!(back.subgraphs[FUNCTION].nodes[&1584844890].kind, "sbs:fn:add");
    assert_eq!(back.subgraphs[FUNCTION].sd.as_ref().unwrap().root, Some(1584844559));
    assert_eq!(back.subgraphs[FX].nodes, g.subgraphs[FX].nodes, "editing a function must preserve the separate FX graph");
}

#[test]
fn graphs_can_be_renamed_and_removed() {
    let mut g = open(SOURCE);
    sbs_native::duplicate_graph(&mut g, None, Some("Copy")).unwrap();
    sbs_native::rename_graph(&mut g, "Copy", "Renamed").unwrap();
    let mut back = open(&sbs_native::save_text(&g).unwrap());
    assert!(back.subgraphs.contains_key("Renamed"));
    assert!(!back.subgraphs.contains_key("Copy"));
    assert_eq!(back.subgraphs["Renamed"].nodes.len(), g.nodes.len());
    sbs_native::remove_graph(&mut back, "Renamed").unwrap();
    let back = open(&sbs_native::save_text(&back).unwrap());
    assert!(!back.subgraphs.contains_key("Renamed"));
    assert_eq!(back.nodes, open(SOURCE).nodes, "removing the copy must preserve the original graph");
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
    // a bitmap or SVG needs its file before it makes anything (a value processor without a function gives 0,
    // as an empty one does in Substance Designer)
    assert_eq!(none, vec!["sbs:bitmap", "sbs:svg"], "no picture");
}

#[test]
fn broken_packages_are_errors_not_panics() {
    for bad in ["", "<package>", "<notapackage/>", "<package><content/></package>"] {
        assert!(sbs_native::load(bad, None).is_err(), "{bad:?}");
    }
    let odd = SOURCE.replacen("<connRef v=\"1359211355\"/>", "<connRef v=\"999999\"/>", 1);
    assert_ne!(odd.len(), SOURCE.len(), "all_nodes.sbs: missing connection to corrupt");
    let (_, warnings) = sbs_native::load(&odd, None).expect("damaged links should produce warnings");
    assert!(!warnings.is_empty(), "all_nodes.sbs: broken link was silently accepted");
}

#[test]
fn a_renamed_output_keeps_its_users() {
    let mut g = sbs_native::new_package("mat", 64);
    let out = sbs_native::add_output(&mut g, "baseColor", Some("base"), [0.0, 0.0]);
    sbs_native::add_output(&mut g, "normal", Some("nrm"), [0.0, 80.0]);
    // a graph of the package that runs this one and reads the output by name
    let mut user = sbs_native::new_package("user", 64);
    user.nodes.insert(1, Node { kind: "sbs:graph".into(), strs: [("#graph".to_string(), "mat".to_string())].into(), ..Default::default() });
    let mut reader = Node { kind: "sbs:blend".into(), ..Default::default() };
    reader.ins.insert("source".into(), 1);
    reader.in_out.insert("source".into(), "base".into());
    user.nodes.insert(2, reader);

    assert!(sbs_native::rename_output(&mut g, out, "nrm").is_err(), "two outputs cannot share a name");
    assert!(sbs_native::rename_output(&mut g, out, "bad name").is_err());
    assert_eq!(sbs_native::rename_output(&mut g, out, "albedo").unwrap(), ("base".to_string(), "albedo".to_string()));
    sbs_native::retarget_output(&mut user, "mat", "base", "albedo");
    assert_eq!(user.nodes[&2].in_out["source"], "albedo");
    assert_eq!(sbs_native::label(&g.nodes[&out], &g), "albedo");
    assert!(g.output_nodes().iter().any(|(usage, ident, id)| *id == out && usage == "basecolor" && ident == "albedo"));
    let text = sbs_native::save_text(&g).unwrap();
    assert!(text.contains("\"albedo\"") && !text.contains("\"base\""), "{text}");
}

#[test]
fn pbr_base_material_inputs_follow_their_switches() {
    let mut n = Node { kind: "sbs:lib:pbr_base_material/pbr_base_material".into(), ..Default::default() };
    assert!(!sbs_native::input_switches(&n).is_empty());
    assert!(!sbs_native::input_used(&n, "basecolor_input"), "every map input starts off");
    n.strs.insert("user_basecolor".into(), PVal::Const { ty: PTy::B, raw: "1".into(), rel: Some("0".into()) }.enc());
    assert!(sbs_native::input_used(&n, "basecolor_input"));
    assert!(!sbs_native::input_used(&n, "normal_input"));
    // inputs without a switch are always read
    assert!(sbs_native::input_used(&Node { kind: "sbs:blend".into(), ..Default::default() }, "source"));
}

#[test]
fn a_pass_through_reads_one_output_of_a_material_node() {
    // the 3D view shows a material node through one pass-through per output
    let mut g = sbs_native::new_package("m", 32);
    let mut n = Node { kind: "sbs:lib:pbr_base_material/pbr_base_material".into(), ..Default::default() };
    n.strs.insert("metallic".into(), PVal::Const { ty: PTy::F(1), raw: "0".into(), rel: None }.enc());
    n.strs.insert("roughness".into(), PVal::Const { ty: PTy::F(1), raw: "0.3".into(), rel: None }.enc());
    g.nodes.insert(5, n);
    for (id, pin) in [(100, "diffuse"), (101, "roughness"), (102, "glossiness"), (103, "normal")] {
        let mut t = Node { kind: "sbs:passthrough".into(), ..Default::default() };
        t.ins.insert("input".into(), 5);
        t.in_out.insert("input".into(), pin.into());
        g.nodes.insert(id, t);
    }
    let keep = [100, 101, 102, 103];
    let r = g.evaluate_all_cached_ex(32, &keep, &[], 0, &std::sync::Arc::new(materialcraft::graph::NodeCache::new(64)), None, false, &mut |_| {});
    let px = |id: u32| r.kept.get(&id).unwrap_or_else(|| panic!("{id}: {:?}", r.errors.get(&id))).px[0];
    assert!((px(100)[0] - 0.498).abs() < 0.01, "a dielectric's diffuse is its base colour");
    assert!((px(101)[0] - 0.3).abs() < 1e-4);
    assert!((px(102)[0] - 0.7).abs() < 1e-4, "glossiness follows roughness");
    assert_eq!(px(103), [0.5, 0.5, 1.0, 1.0]);
}

#[test]
fn the_older_library_material_graphs_import_as_native_nodes() {
    use materialcraft::{nodes10, sbs_defs, sbs_import};
    let graphs = [
        ("ma_directionnal_leaks", "directionnal_leaks"), ("ma_edge_blur", "edge_blur"), ("ma_edge_dirt", "edge_dirt"), ("ma_edge_spatter", "edge_spatter"), ("ma_occlusion", "occlusion"), ("ma_occlusion_noise", "occlusion_noise"),
        ("ma_random_dirt", "random_dirt"), ("ma_selective_dirt", "selective_dirt"), ("ma_sun_bleach", "sun_bleach"), ("metal_weathering", "metal_weathering"), ("moss_weathering", "moss_weahtering"), ("wood_weathering", "wood_weathering"),
        ("material_composite", "material_composite"), ("material_transforms", "material_transforms"), ("pbr_converter", "basecolor_metallic_roughness_to_diffuse_specular_glossiness"), ("pbr_converter", "diffuse_metallic_roughness_to_diffuse_specular_glossiness"),
    ];
    for (p, id) in graphs {
        assert!(sbs_import::library_renders(p, id), "{p}/{id}");
        assert_eq!(nodes10::kind_for(p, id), Some(format!("x_{id}").as_str()).map(|k| nodes10::info(k).unwrap().kind), "{p}/{id}");
    }
    // the two moss graphs stay apart
    assert_eq!(nodes10::kind_for("moss_weathering_material", "moss_weathering"), Some("x_moss_weathering"));
    // registered names stay out of the labels, the identifiers do not change
    for id in ["Substance_Normal_Intensity", "Substance_AO_Intensity"] {
        assert!(!sbs_defs::label_of(id).contains("Substance"));
        for k in ["x_metal_weathering", "x_wood_weathering"] {
            let p = nodes10::pdefs(k).into_iter().find(|p| p.name == id).unwrap();
            assert!(!p.label.contains("Substance"), "{k}: {}", p.label);
        }
    }

    let mut g = sbs_native::new_package("m", 16);
    for (id, raw) in [(1, "0.8 0.4 0.2 1"), (2, "0.25 0.25 0.25 1"), (3, "0.5 0.5 0.5 1")] {
        let mut u = Node { kind: "sbs:uniform".into(), ..Default::default() };
        u.strs.insert("outputcolor".into(), PVal::Const { ty: PTy::F(4), raw: raw.into(), rel: None }.enc());
        g.nodes.insert(id, u);
    }
    let mut occ = Node { kind: "sbs:lib:ma_occlusion/occlusion".into(), ..Default::default() };
    occ.ins.insert("Input_2".into(), 1);
    occ.ins.insert("Input_1".into(), 2);
    occ.strs.insert("opacity".into(), PVal::Const { ty: PTy::F(1), raw: "0.5".into(), rel: None }.enc());
    g.nodes.insert(4, occ);
    let mut pbr = Node { kind: "sbs:lib:pbr_converter/basecolor_metallic_roughness_to_diffuse_specular_glossiness".into(), ..Default::default() };
    pbr.ins.insert("basecolor".into(), 1);
    pbr.ins.insert("roughness".into(), 2);
    pbr.ins.insert("metallic".into(), 3);
    g.nodes.insert(5, pbr);
    let mut moss = Node { kind: "sbs:lib:moss_weathering/moss_weahtering".into(), ..Default::default() };
    moss.ins.insert("Input_2".into(), 1);
    moss.strs.insert("moss_propagation".into(), PVal::Const { ty: PTy::F(1), raw: "0".into(), rel: None }.enc());
    g.nodes.insert(6, moss);
    for (id, src, pin) in [(100, 5, "glossiness"), (101, 5, "diffuse"), (102, 6, "Diffuse"), (103, 6, "Glossiness_Roughness")] {
        let mut t = Node { kind: "sbs:passthrough".into(), ..Default::default() };
        t.ins.insert("input".into(), src);
        t.in_out.insert("input".into(), pin.into());
        g.nodes.insert(id, t);
    }
    let r = g.evaluate_all_cached_ex(16, &[4, 100, 101, 102, 103], &[], 0, &std::sync::Arc::new(materialcraft::graph::NodeCache::new(64)), None, false, &mut |_| {});
    let px = |id: u32| r.kept.get(&id).unwrap_or_else(|| panic!("{id}: {:?}", r.errors.get(&id))).px[0];
    let k = 0.84375 * 0.5;
    assert!((px(4)[0] - 0.8 * (1.0 - k)).abs() < 2e-3, "occlusion through its opacity: {:?}", px(4));
    assert!((px(100)[0] - 0.75).abs() < 2e-3, "glossiness is the inverse roughness: {:?}", px(100));
    assert!((px(101)[0] - 0.4).abs() < 2e-3, "half a metal keeps half its colour as diffuse: {:?}", px(101));
    assert!((px(102)[0] - 0.8).abs() < 2e-3, "no moss at no propagation: {:?}", px(102));
    assert!((px(103)[0] - 0.5).abs() < 2e-3, "the moss graph's own output name: {:?}", px(103));
}

#[test]
fn a_graph_saved_as_template_starts_new_projects() {
    use materialcraft::user_templates;
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("user_templates_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(user_templates::list(&dir).is_empty(), "no folder yet: no templates");
    let mut g = sbs_native::new_package("brick_base", 64);
    sbs_native::add_output(&mut g, "baseColor", Some("albedo"), [0.0, 0.0]);
    sbs_native::add_output(&mut g, "roughness", None, [0.0, 80.0]);
    let mut n = Node { kind: "sbs:uniform".into(), ..Default::default() };
    n.strs.insert("outputcolor".into(), PVal::Const { ty: PTy::F(4), raw: "1 0 0 1".into(), rel: None }.enc());
    g.nodes.insert(7, n);

    assert!(user_templates::save(&dir, "  ", &g, "brick_base").is_err(), "a template needs a name");
    let p = user_templates::save(&dir, "My Bricks/v2", &g, "brick_base").unwrap();
    assert_eq!(p.file_name().unwrap(), "My Bricks_v2.sbs", "unsafe characters leave the file name");
    let list = user_templates::list(&dir);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "My Bricks_v2");
    assert_eq!(user_templates::outputs(&list[0]).unwrap(), ["albedo", "roughness"]);

    // a new project: a copy with the graph renamed, the nodes and outputs kept
    let copy = user_templates::instantiate(&list[0], "Wall 01").unwrap();
    assert_eq!(sbs_native::graph_name(&copy), "Wall_01");
    assert_eq!(copy.nodes.values().filter(|n| n.kind == "sbs:uniform").count(), 1);
    assert_eq!(copy.output_idents(), ["albedo", "roughness"]);
    // or a graph added to a package that is open
    let mut other = sbs_native::new_package("other", 64);
    let (first, added, _) = sbs_native::add_package(&mut other, copy).unwrap();
    assert_eq!((first.as_str(), added), ("Wall_01", 1));
    assert!(other.subgraphs.contains_key("Wall_01"));

    // saving under the same name replaces the template
    user_templates::save(&dir, "My Bricks/v2", &g, "brick_base").unwrap();
    assert_eq!(user_templates::list(&dir).len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_graph_a_package_opens_on_can_be_deleted() {
    let mut g = sbs_native::new_package("first", 64);
    sbs_native::add_output(&mut g, "baseColor", None, [0.0, 0.0]);
    // the only graph stays
    assert!(g.remove_graph("first").is_err(), "a package keeps at least one graph");
    let second = sbs_native::duplicate_graph(&mut g, None, Some("second")).unwrap();
    sbs_native::duplicate_graph(&mut g, None, Some("third")).unwrap();
    // not while another graph uses it
    let mut user = Node { kind: "sbs:graph".into(), ..Default::default() };
    user.strs.insert("#graph".into(), "first".into());
    g.subgraphs.get_mut("third").unwrap().nodes.insert(50, user);
    assert!(g.remove_graph("first").is_err(), "'third' still uses it");
    g.subgraphs.get_mut("third").unwrap().nodes.remove(&50);
    g.remove_graph("first").unwrap();
    // the next graph is the one the package opens on, with the rest of the package
    assert_eq!(g.meta.get("identifier").map(String::as_str), Some(second.as_str()));
    assert_eq!(g.subgraphs.keys().cloned().collect::<Vec<_>>(), vec!["third".to_string()]);
    assert_eq!(g.output_idents().len(), 1);
    let back = open(&sbs_native::save_text(&g).unwrap());
    assert_eq!(back.meta.get("identifier").map(String::as_str), Some("second"));
    assert!(back.subgraphs.contains_key("third") && !back.subgraphs.contains_key("first"));
}
