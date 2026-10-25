// SPDX-License-Identifier: MIT OR Apache-2.0
//! Catalog-wide SBS edit preservation and rendering regressions.
mod common;
use common::{fixture, same_package, save_reopen};
use materialcraft::graph::Graph;
use materialcraft::sbs_defs::PTy;
use materialcraft::sbs_native::{self as native, PVal};
use std::collections::BTreeMap;

fn changed(ty: PTy, raw: &str, rel: Option<String>) -> PVal {
    let raw = match ty {
        PTy::B => if raw == "1" || raw.eq_ignore_ascii_case("true") { "0".into() } else { "1".into() },
        PTy::F(_) | PTy::FV => (0..ty.lanes()).map(|i| {
            let old = raw.split_whitespace().nth(i).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
            if old == 0.375 { "0.625" } else { "0.375" }
        }).collect::<Vec<_>>().join(" "),
        PTy::I(_) => (0..ty.lanes()).map(|i| {
            let old = raw.split_whitespace().nth(i).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            if old == 1 { "2" } else { "1" }
        }).collect::<Vec<_>>().join(" "),
        PTy::S | PTy::Path => format!("{raw} review & < > \" ' é\nsecond line"),
    };
    PVal::Const { ty: match ty { PTy::Path => PTy::S, PTy::FV => PTy::F(4), other => other }, raw, rel }
}

fn roundtrip(g: &Graph) {
    let (back, text, _dir) = save_reopen(g);
    same_package(g, &back, "edited round-trip");
    assert!(native::save_text(&back).unwrap() == text, "second save is unstable");
}

#[test]
fn all_existing_constant_properties_survive_edits() {
    let mut g = fixture();
    let mut properties = 0;
    let mut nodes = 0;
    let mut types = BTreeMap::new();
    for graph in std::iter::once(&mut g.nodes).chain(g.subgraphs.values_mut().map(|s| &mut s.nodes)) {
        for n in graph.values_mut() {
            let mut touched = false;
            for (key, value) in native::values(n) {
                if let PVal::Const { ty, raw, rel } = value {
                    let new = changed(ty, &raw, rel);
                    assert_ne!(new.enc(), n.strs[&key]);
                    native::set_value(n, &key, Some(new));
                    properties += 1;
                    *types.entry(ty.code()).or_insert(0) += 1;
                    touched = true;
                }
            }
            nodes += usize::from(touched);
        }
    }
    println!("Edited {properties} explicit constants across {nodes} nodes; types: {types:?}");
    assert!(properties > 100);
    roundtrip(&g);
}

#[test]
fn all_declared_numeric_properties_survive_first_edits() {
    let mut g = fixture();
    let lib = g.subgraphs.clone();
    let mut properties = 0;
    let mut nodes = 0;
    let mut skipped_text = 0;
    for graph in std::iter::once(&mut g.nodes).chain(g.subgraphs.values_mut().map(|s| &mut s.nodes)) {
        for n in graph.values_mut() {
            let mut touched = false;
            for p in native::params_of(n, &lib) {
                if p.value.is_some() { continue; }
                if p.ty.lanes() == 0 { skipped_text += 1; continue; }
                // Function data constants do not have relativeTo in the SBS format.
                let rel = if n.kind.starts_with("sbs:fn:") { None } else { Some("0".into()) };
                let new = changed(p.ty, &p.def, rel);
                native::set_value(n, &p.id, Some(new));
                properties += 1;
                touched = true;
            }
            nodes += usize::from(touched);
        }
    }
    println!("Set {properties} previously absent numeric properties across {nodes} nodes; skipped {skipped_text} absent text/path properties");
    assert!(properties > 1000);
    roundtrip(&g);
}

#[test]
fn every_node_position_and_title_survive_edits() {
    let mut g = fixture();
    let mut count = 0;
    for graph in std::iter::once(&mut g.nodes).chain(g.subgraphs.values_mut().map(|s| &mut s.nodes)) {
        for (id, n) in graph {
            n.pos = [n.pos[0] + 2.0, n.pos[1] - 3.0];
            // Function/FX nodes do not all support titles, so exercise positions on those.
            if n.kind.starts_with("sbs:lib:") || (!n.kind.starts_with("sbs:fn:") && !n.kind.starts_with("sbs:fx:")) {
                n.strs.insert("title".into(), format!("Review {id} & < > \" é"));
            }
            count += 1;
        }
    }
    println!("Moved {count} nodes across all graphs");
    roundtrip(&g);
}

#[test]
fn every_declared_output_pin_survives_connection_save_and_render() {
    let mut g = fixture();
    let targets: Vec<_> = g.nodes.iter().flat_map(|(&id, n)| {
        native::outputs(n, &g.subgraphs).into_iter().map(move |out| (id, n.kind.clone(), out))
    }).collect();
    let mut probes = BTreeMap::new();
    for (i, (source, kind, out)) in targets.iter().enumerate() {
        let id = 2_000_000_000 + i as u32;
        let mut n = native::new_node("sbs:passthrough", None, [0.0, 0.0]);
        n.ins.insert("input".into(), *source);
        n.in_out.insert("input".into(), out.clone());
        g.nodes.insert(id, n);
        probes.insert(id, format!("{source} {kind} output {out}"));
    }
    let (back, _, _dir) = save_reopen(&g);
    for (id, target) in &probes {
        assert_eq!(g.nodes[id].ins, back.nodes[id].ins, "{target}");
        let source = g.nodes[id].ins["input"];
        let first = native::outputs(&g.nodes[&source], &g.subgraphs).into_iter().next().unwrap();
        let reopened_output = back.nodes[id].in_out.get("input").unwrap_or(&first);
        assert_eq!(&g.nodes[id].in_out["input"], reopened_output, "{target}");
    }
    let low = native::lower(&back).unwrap();
    assert_eq!(low.report.missing, 0, "{}", low.report.summary());
    let result = low.graph.evaluate_all(32, &probes.keys().copied().collect::<Vec<_>>());
    let mut errors = Vec::new();
    for (id, target) in &probes {
        match result.kept.get(id) {
            Some(t) if !t.px.is_empty() && t.px.iter().flatten().all(|v| v.is_finite()) => {},
            _ => errors.push(format!("{target}: {:?}", result.errors.get(id))),
        }
    }
    println!("Saved, reopened, and rendered {} named output connections", probes.len());
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

fn set(g: &mut Graph, id: u32, key: &str, ty: PTy, raw: &str) {
    native::set_value(g.nodes.get_mut(&id).unwrap(), key, Some(PVal::Const { ty, raw: raw.into(), rel: Some("0".into()) }));
}

fn rendered_roundtrip(g: &Graph, id: u32) -> materialcraft::tex::Tex {
    let before = g.evaluate_node(id, 64).unwrap();
    let (back, _, _dir) = save_reopen(g);
    same_package(g, &back, "edited round-trip");
    let after = back.evaluate_node(id, 64).unwrap();
    assert_eq!((before.w, before.h), (after.w, after.h));
    assert!(before.px == after.px, "node {id} changed pixels after reopening");
    before
}

#[test]
fn exposure_and_blackbody_edits_change_pixels_and_survive_reopening() {
    let mut g = fixture();
    let source = 1359211355;
    let exposure = 1584828635;
    let blackbody = 1584828559;
    set(&mut g, source, "outputcolor", PTy::F(4), "0.1 0.15 0.2 1");
    g.nodes.get_mut(&exposure).unwrap().ins.insert("input".into(), source);
    for (ev, multiplier) in [("-1", 0.5), ("2", 4.0)] {
        set(&mut g, exposure, "exposure", PTy::F(1), ev);
        let t = rendered_roundtrip(&g, exposure);
        for (i, base) in [0.1, 0.15, 0.2].iter().enumerate() {
            assert!((t.px[0][i] - base * multiplier).abs() < 0.01, "exposure {ev}, channel {i}: {:?}", t.px[0]);
        }
    }
    set(&mut g, blackbody, "temperature", PTy::F(1), "2000");
    let warm = rendered_roundtrip(&g, blackbody);
    set(&mut g, blackbody, "temperature", PTy::F(1), "12000");
    let cold = rendered_roundtrip(&g, blackbody);
    assert!(warm.px[0][0] > warm.px[0][2]);
    assert!(cold.px[0][2] > cold.px[0][0]);
}

#[test]
fn crop_filtering_edits_change_sampling() {
    let checker = 1584687242;
    for crop in [1584802178, 1584802301] { // Color and grayscale.
        for source_size in ["0 0", "-1 -1"] { // Also sample a smaller input without interpolating it first.
            let mut g = fixture();
            g.nodes.get_mut(&crop).unwrap().ins.insert("input".into(), checker);
            for (id, raw) in [(checker, source_size), (crop, "0 0")] {
                native::set_value(g.nodes.get_mut(&id).unwrap(), "outputsize", Some(PVal::Const { ty: PTy::I(2), raw: raw.into(), rel: Some("1".into()) }));
            }
            let filtering = native::params_of(&g.nodes[&crop], &g.subgraphs).into_iter().find(|p| p.id == "filtering").unwrap();
            assert_eq!(filtering.choices, ["Bilinear", "Nearest"]);
            set(&mut g, crop, "matrix", PTy::F(4), "1.7 0 0 1.7");
            set(&mut g, crop, "offset", PTy::F(2), "0.013 0.021");
            set(&mut g, crop, "filtering", PTy::I(1), "0");
            let a = rendered_roundtrip(&g, crop);
            assert!(a.px.windows(2).any(|p| p[0] != p[1]), "probe input must have edges");
            set(&mut g, crop, "filtering", PTy::I(1), "1");
            let b = rendered_roundtrip(&g, crop);
            let source = g.evaluate_node(checker, 64).unwrap();
            assert!(b.px.iter().all(|p| source.px.contains(p)), "nearest filtering must preserve source texels");
            assert!(a.px.iter().any(|p| !source.px.contains(p)), "bilinear filtering must interpolate across edges");
            assert!(a.px != b.px, "Crop node {crop}: filtering 0 and 1 must differ on a scaled checkerboard");
        }
    }
}

#[test]
fn dynamic_color_components_share_work_and_survive_edits() {
    use materialcraft::graph::NodeCache;
    use std::sync::Arc;
    let mut g = native::new_package("color", 32);
    let id = 1;
    g.nodes.insert(id, native::new_node("sbs:uniform", None, [0.0, 0.0]));
    let value = PVal::Const { ty: PTy::F(4), raw: "0.125 0.25 0.5 0.75".into(), rel: Some("0".into()) };
    let pkg = g.sd.as_ref().unwrap().pkg.clone();
    let name = native::make_function(&mut g.subgraphs, pkg, "color", id, "outputcolor", &value);
    native::set_value(g.nodes.get_mut(&id).unwrap(), "outputcolor", Some(PVal::Dyn { graph: name.clone(), rel: Some("0".into()) }));
    let cache = Arc::new(NodeCache::new(32));
    for (raw, expected) in [("0.125 0.25 0.5 0.75", [0.125, 0.25, 0.5, 0.75]), ("0.75 0.5 0.25 1", [0.75, 0.5, 0.25, 1.0])] {
        native::set_value(g.subgraphs.get_mut(&name).unwrap().nodes.get_mut(&1).unwrap(), "const_float4", Some(PVal::Const { ty: PTy::F(4), raw: raw.into(), rel: None }));
        let low = native::lower(&g).unwrap();
        let bindings: Vec<_> = low.graph.nodes[&id].strs.iter().filter(|(k, _)| k.starts_with("fn.")).map(|(_, name)| name).collect();
        assert_eq!(bindings.len(), 4);
        assert!(bindings.iter().all(|name| *name == bindings[0]));
        let (back, _, _dir) = save_reopen(&g);
        for graph in [&g, &back] {
            let result = graph.evaluate_all_cached(32, &[id], 0, &cache, None, true, &mut |_| {});
            assert!(result.errors.is_empty(), "{:?}", result.errors);
            assert!(result.kept[&id].px.iter().all(|p| *p == expected), "color {raw}: {:?}", result.kept[&id].px[0]);
        }
    }
}

#[test]
fn dynamic_output_size_matches_the_equivalent_constant() {
    // Explicit expected sizes cover absolute sizing and offsets from a rectangular parent/input.
    for (parent, rel, raw, expected) in [
        ("8 8", "0", "6 5", (64, 32)),
        ("8 7", "0", "6 6", (64, 64)),
        ("8 7", "1", "-1 -2", (128, 32)),
        ("8 7", "1", "0 1", (256, 256)),
        ("8 7", "2", "-1 -2", (64, 16)),
        ("8 7", "2", "1 0", (256, 64)),
    ] {
        let mut g = fixture();
        let id = 1359211383; // Normal from Height, fed by a 128x64 Uniform Color.
        g.sd.as_mut().unwrap().base.insert("outputsize".into(), format!("i2@0:{parent}"));
        set(&mut g, 1359211355, "outputsize", PTy::I(2), "7 6");
        g.nodes.get_mut(&id).unwrap().ins.insert("input1".into(), 1359211355);
        let value = PVal::Const { ty: PTy::I(2), raw: raw.into(), rel: Some(rel.into()) };
        native::set_value(g.nodes.get_mut(&id).unwrap(), "outputsize", Some(value.clone()));
        let size = native::lower(&g).unwrap().graph.size;
        let constant = g.evaluate_node(id, size).unwrap();
        assert_eq!((constant.w, constant.h), expected, "constant: parent {parent}, relativeTo {rel}, value {raw}");
        let owner = native::graph_name(&g);
        let pkg = g.sd.as_ref().unwrap().pkg.clone();
        let name = native::make_function(&mut g.subgraphs, pkg, &owner, id, "outputsize", &value);
        native::set_value(g.nodes.get_mut(&id).unwrap(), "outputsize", Some(PVal::Dyn { graph: name, rel: Some(rel.into()) }));
        let lowered = native::lower(&g).unwrap();
        let node = &lowered.graph.nodes[&id];
        let param = if rel == "0" { "osize_abs" } else { "osize_rel" };
        assert_eq!(node.strs[&format!("fn.{param}")], node.strs[&format!("fn.{param}_h")], "width and height should share one converted function");
        let (back, saved, _dir) = save_reopen(&g);
        assert_eq!(native::values(&g.nodes[&id]), native::values(&back.nodes[&id]));
        assert!(native::save_text(&back).unwrap() == saved, "dynamic output size save must be stable");
        for (stage, graph) in [("before save", &g), ("after reopening", &back)] {
            let dynamic = graph.evaluate_node(id, size).unwrap();
            assert_eq!((dynamic.w, dynamic.h), expected, "{stage}: parent {parent}, relativeTo {rel}, value {raw}");
            assert!(dynamic.px == constant.px, "{stage}: function and constant must render identical pixels");
        }
    }
}
