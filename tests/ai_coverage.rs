// SPDX-License-Identifier: MIT OR Apache-2.0
//! The assistant interface reaches every node: each node type can be listed, described with all of its parameters,
//! added, set and drawn through the command layer, and every renderable native node kind is reachable from it.
use materialcraft::json::Json;
use materialcraft::sbs_defs::{defs, Fam};
use materialcraft::session::Session;
use std::collections::BTreeSet;

#[test]
fn every_texture_node_is_described_added_and_drawn() {
    let mut s = Session::new();
    let mut bad: Vec<String> = Vec::new();
    let mut n = 0;
    for d in defs().list.iter().filter(|d| d.fam == Fam::Texture && d.renders && d.kind != "sbs:graph") {
        n += 1;
        let desc = match s.execute("node.describe", &Json::obj().with("kind", d.kind)) {
            Ok(o) => o.data,
            Err(e) => {
                bad.push(format!("{} describe: {e}", d.kind));
                continue;
            }
        };
        let listed: BTreeSet<String> = desc.get("params").and_then(Json::as_arr).map(|a| a.iter().filter_map(|p| p.get("name").and_then(Json::as_str).map(str::to_string)).collect()).unwrap_or_default();
        for p in &d.params {
            if !listed.contains(&p.id.to_ascii_lowercase()) && !listed.contains(p.id) {
                bad.push(format!("{} parameter {} missing from node.describe", d.kind, p.id));
            }
        }
        let add = Json::obj().with("kind", d.kind).with("name", "n1");
        if let Err(e) = s.execute("node.add", &add) {
            bad.push(format!("{} add: {e}", d.kind));
            continue;
        }
        if let Err(e) = s.execute("node.params", &Json::obj().with("node", "n1")) {
            bad.push(format!("{} params: {e}", d.kind));
        }
        let r = Json::obj().with("node", "n1").with("size", 32);
        if let Err(e) = s.execute("render.node", &r) {
            // nodes that need an input picture say so; anything else is a gap
            if !e.contains("input") && !e.contains("needs") && !e.contains("connect") {
                bad.push(format!("{} render: {e}", d.kind));
            }
        }
        let _ = s.execute("node.remove", &Json::obj().with("node", "n1"));
    }
    assert!(n > 500, "only {n} texture node types found");
    assert!(bad.is_empty(), "{} problems:\n{}", bad.len(), bad.join("\n"));
}

