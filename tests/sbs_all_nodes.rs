// SPDX-License-Identifier: MIT OR Apache-2.0
//! Always exercise the checked-in native node catalog; no MC_SBS_SAMPLES setup is needed.
//!
//! Run: cargo test --test sbs_all_nodes -- --nocapture
//! The round-trip test checks preservation. The compatibility test fails on any unsupported
//! node, missing editor definition, unresolved resource or render error; it passes once the
//! fixture's environment maps are in `assets/panorama/` (see FIXTURE_LIMITATIONS for the two
//! problems that come from how the fixture was authored).
//! Do not replace those failures with a count snapshot or a should_panic assertion:
//! a gray placeholder is not a successful node implementation.

use materialcraft::graph::Graph;
use materialcraft::sbs_defs;
use materialcraft::sbs_import::{self, Level};
use materialcraft::sbs_native::{self as native, PVal, Role};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const FIXTURE: &str = "tests/assets/all_nodes.sbs";

/// Problems that belong to how the fixture was authored, not to MaterialCraft: each is still listed in the output
/// (as a fixture limitation), but it does not fail the check. Keep this list to things no implementation can fix.
/// The environment maps the fixture links (`sd-3dview-maps://...`) are NOT listed: supply them in
/// `assets/panorama/` and they resolve like any other linked file.
const FIXTURE_LIMITATIONS: &[(&str, &str)] = &[
    (
        "XXXXXX-Untitled_Bitmap.png",
        "a temporary bitmap from the authoring machine's temp folder: the file never travelled with the fixture",
    ),
    (
        "Substance_graph/1584843868/outputsize",
        "the Blend node's output-size function is a palette holding one of every function node with no result node set; \
         like the file's own editor, MaterialCraft gives the default size",
    ),
];

fn fixture_limitation(detail: &str) -> Option<&'static str> {
    FIXTURE_LIMITATIONS.iter().find(|(key, _)| detail.contains(key)).map(|(_, why)| *why)
}
const OUTPUTS: [&str; 7] = ["ambientocclusion", "basecolor", "height", "metallic", "normal", "output", "roughness"];

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE)
}

fn open(path: &Path, stage: &str) -> Graph {
    let (g, warnings) = native::open(path).unwrap_or_else(|e| panic!("{FIXTURE}: {stage}: {}: {e}", path.display()));
    assert!(warnings.is_empty(), "{FIXTURE}: {stage} produced load warnings:\n{}", warnings.join("\n"));
    g
}

fn original() -> Graph {
    let path = fixture_path();
    let source = std::fs::read_to_string(&path).expect("read all_nodes.sbs");
    let package = materialcraft::sbs::Package::parse(&source).expect("parse all_nodes.sbs");
    assert_eq!(package.graphs.len(), 1, "{FIXTURE}: package graph inventory changed");
    assert_eq!(package.resources.len(), 36, "{FIXTURE}: resource inventory changed");
    let g = open(&path, "import");
    assert_eq!(g.nodes.len(), 566, "{FIXTURE}: import lost compositing nodes or the fixture inventory changed");
    assert_eq!(g.nodes.values().map(|n| n.ins.len()).sum::<usize>(), 6, "{FIXTURE}: import changed the connection inventory");
    let mut outputs: Vec<_> = g.output_nodes().into_iter().map(|(_, name, _)| name).collect();
    outputs.sort();
    assert_eq!(outputs, OUTPUTS, "{FIXTURE}: imported output names changed");
    for (name, count) in [("Substance_graph/1584632949/fx0", 1), ("Substance_graph/1584843868/outputsize", 237)] {
        let sub = g.subgraphs.get(name).unwrap_or_else(|| panic!("{FIXTURE}: import lost embedded graph '{name}'"));
        assert_eq!(sub.nodes.len(), count, "{FIXTURE}: embedded graph '{name}' lost nodes");
    }
    g
}

/// Keep failure output bounded: the fixture is minified XML, so assert_eq! on
/// the complete text would bury the useful mismatch in nearly a megabyte of XML.
fn same_bytes(expected: &str, actual: &str, stage: &str) {
    if expected == actual {
        return;
    }
    let at = expected.bytes().zip(actual.bytes()).position(|(a, b)| a != b).unwrap_or(expected.len().min(actual.len()));
    let excerpt = |s: &str| String::from_utf8_lossy(&s.as_bytes()[at.saturating_sub(60)..(at + 60).min(s.len())]).into_owned();
    panic!(
        "{FIXTURE}: {stage}: first byte difference at offset {at}; expected {} bytes, got {}.\nExpected nearby: {:?}\nActual nearby: {:?}",
        expected.len(),
        actual.len(),
        excerpt(expected),
        excerpt(actual)
    );
}

struct ExportDir(PathBuf);

impl ExportDir {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("system clock before UNIX epoch").as_nanos();
        let path = std::env::temp_dir().join(format!("materialcraft_all_nodes_{}_{stamp}", std::process::id()));
        std::fs::create_dir(&path).unwrap_or_else(|e| panic!("{FIXTURE}: create export directory {}: {e}", path.display()));
        Self(path)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        materialcraft::safeio::write_file(&path, text.as_bytes()).unwrap_or_else(|e| panic!("{FIXTURE}: export {}: {e}", path.display()));
        path
    }
}

impl Drop for ExportDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn same_package(expected: &Graph, actual: &Graph, stage: &str) {
    let before = native::all_graphs(expected);
    let after = native::all_graphs(actual);
    assert_eq!(before.keys().collect::<Vec<_>>(), after.keys().collect::<Vec<_>>(), "{FIXTURE}: {stage}: graph names changed");
    for (name, a) in before {
        let b = after[&name];
        assert_eq!(a.nodes.keys().collect::<Vec<_>>(), b.nodes.keys().collect::<Vec<_>>(), "{FIXTURE}: {stage}: graph '{name}' lost or gained node UIDs");
        for (id, n) in &a.nodes {
            assert_eq!(n, &b.nodes[id], "{FIXTURE}: {stage}: graph '{name}', node {id} ({}): kind, parameters, links, output pins or position changed", n.kind);
        }
        assert_eq!((a.size, a.height), (b.size, b.height), "{FIXTURE}: {stage}: graph '{name}' dimensions changed");
        assert_eq!(a.output_nodes(), b.output_nodes(), "{FIXTURE}: {stage}: graph '{name}' outputs changed");
        let (a_sd, b_sd) = (a.sd.as_ref().unwrap(), b.sd.as_ref().unwrap());
        assert_eq!(a_sd.role, b_sd.role, "{FIXTURE}: {stage}: graph '{name}' role changed");
        assert_eq!(a_sd.root, b_sd.root, "{FIXTURE}: {stage}: graph '{name}' result node changed");
    }
    assert!(expected.resources == actual.resources, "{FIXTURE}: {stage}: embedded resource names or bytes changed");
}

#[test]
fn all_nodes_import_export_and_reimport_preserve_the_package() {
    let path = fixture_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{FIXTURE}: read original bytes: {e}"));
    let g = original();
    // Whitespace-preserving saves are also checked for a formatted copy of the real file.
    let mut formatted = materialcraft::xmldoc::XDoc::parse(&text).unwrap();
    materialcraft::xmldoc::Indent { unit: Some(" ".into()), newline: "\n".into() }.apply(&mut formatted.root, 0);
    let formatted = formatted.to_text();
    let (pretty, warnings) = native::load(&formatted, Some(path.clone())).expect("formatted all_nodes.sbs imports");
    assert!(warnings.is_empty(), "formatted all_nodes.sbs: {warnings:?}");
    same_bytes(&formatted, &native::save_text(&pretty).expect("formatted all_nodes.sbs exports"), "formatted unchanged export");
    let exported = native::save_text(&g).unwrap_or_else(|e| panic!("{FIXTURE}: unchanged export failed: {e}"));
    same_bytes(&text, &exported, "unchanged export");

    let dir = ExportDir::new();
    let exported_path = dir.write("all_nodes.sbs", &exported);
    let back = open(&exported_path, "re-import exported file");
    same_package(&g, &back, "unchanged round-trip");
    same_bytes(&text, &native::save_text(&back).unwrap_or_else(|e| panic!("{FIXTURE}: second export failed: {e}")), "second export");

    // Exercise serialization of an actual edit as well: a writer that merely
    // returns the original document must not satisfy the import/export contract.
    let mut edited = g.clone();
    let id = 1359211355; // Uniform Color feeding basecolor.
    let node = edited.nodes.get_mut(&id).unwrap_or_else(|| panic!("{FIXTURE}: missing edit target node {id}"));
    assert_eq!(node.kind, "sbs:uniform", "{FIXTURE}: edit target {id} changed kind");
    let value = node.strs.get_mut("outputcolor").unwrap_or_else(|| panic!("{FIXTURE}: node {id} has no outputcolor to edit"));
    let Some(PVal::Const { ty, rel, .. }) = PVal::dec(value) else {
        panic!("{FIXTURE}: node {id} outputcolor is not a constant: {value}");
    };
    let new_value = PVal::Const { ty, raw: "0.125 0.25 0.75 1".into(), rel }.enc();
    assert_ne!(*value, new_value, "{FIXTURE}: the export edit must change a value");
    *value = new_value;
    let changed = native::save_text(&edited).unwrap_or_else(|e| panic!("{FIXTURE}: export after editing node {id}: {e}"));
    assert!(changed != text, "{FIXTURE}: export ignored the edit to node {id} outputcolor");
    let changed_path = dir.write("all_nodes_edited.sbs", &changed);
    let changed_back = open(&changed_path, "re-import edited export");
    same_package(&edited, &changed_back, "edited round-trip (all other nodes must survive)");
    same_bytes(&changed, &native::save_text(&changed_back).unwrap_or_else(|e| panic!("{FIXTURE}: export after edited re-import: {e}")), "edited export stability");
    same_bytes(&text, &std::fs::read_to_string(path).expect("re-read source fixture"), "source fixture must remain untouched");
}

fn location(g: &Graph, graph: &str, id: Option<u32>) -> String {
    match id {
        Some(id) => {
            let graphs = native::all_graphs(g);
            let kind = graphs.get(graph).and_then(|g| g.nodes.get(&id)).map(|n| n.kind.as_str()).unwrap_or("unknown node");
            format!("graph '{graph}', node {id} ({kind})")
        }
        None => format!("graph '{graph}'"),
    }
}

fn texture_problem(t: &materialcraft::tex::Tex) -> Option<String> {
    if t.w == 0 || t.h == 0 || t.px.len() != t.w * t.h {
        return Some(format!("invalid texture: {}x{}, {} pixels", t.w, t.h, t.px.len()));
    }
    t.px.iter().enumerate().find(|(_, p)| p.iter().any(|v| !v.is_finite())).map(|(i, p)| format!("non-finite pixel {i}: {p:?}"))
}

#[test]
fn all_nodes_import_maps_and_renders_every_node() {
    let g = original();
    let direct = sbs_import::import_file(&fixture_path()).unwrap_or_else(|e| panic!("{FIXTURE}: direct import failed: {e}"));
    let lowered = native::lower(&g).unwrap_or_else(|e| panic!("{FIXTURE}: editor-to-renderer conversion failed: {e}"));
    for (id, kind) in [(1359211355, "color"), (1359211383, "normal"), (1584632881, "blend"), (1213284337, "output")] {
        assert_eq!(direct.graph.nodes[&id].kind, kind, "{FIXTURE}: node {id} mapped to the wrong renderer kind");
    }
    let graph_name = native::graph_name(&g);
    // Sorted, deduplicated diagnostics make CI output stable and collect every
    // issue instead of stopping at the first unsupported library node.
    let mut failures: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut limitations: BTreeSet<String> = BTreeSet::new();
    let mut approximations = BTreeSet::new();
    for (route, report) in [("direct import", &direct.report), ("editor conversion", &lowered.report)] {
        if report.nodes != g.nodes.len() {
            failures.entry("Conversion coverage").or_default().insert(format!("{route}: converted {} of {} source nodes", report.nodes, g.nodes.len()));
        }
        if report.missing > 0 {
            failures.entry("Conversion summary").or_default().insert(format!("{route}: {}", report.summary()));
        }
        for note in &report.notes {
            let detail = format!("{}: {}", location(&g, &note.graph, note.node), note.text);
            match note.level {
                Level::Missing => {
                    failures.entry("Unsupported conversions and unavailable resources").or_default().insert(detail);
                }
                Level::Approx => {
                    approximations.insert(detail);
                }
                Level::Info => {}
            }
        }
    }

    for (name, sub) in native::all_graphs(&g) {
        for (id, node) in &sub.nodes {
            if sbs_defs::def(&node.kind).is_none() {
                failures.entry("Unknown editor node definitions").or_default().insert(format!("{}: no native node definition", location(&g, &name, Some(*id))));
            }
        }
        if let Some(sd) = &sub.sd {
            if !matches!(sd.role, Role::Graph) && !sub.nodes.is_empty() && sd.root.map(|id| !sub.nodes.contains_key(&id)).unwrap_or(true) {
                failures
                    .entry("Incomplete embedded graphs in the fixture")
                    .or_default()
                    .insert(format!("graph '{name}': {} nodes but no valid result node; this graph cannot demonstrate function compatibility", sub.nodes.len()));
            }
        }
    }

    for (id, node) in &g.nodes {
        for (route, converted) in [("direct import", &direct.graph), ("editor conversion", &lowered.graph)] {
            if !converted.nodes.contains_key(id) {
                failures.entry("Conversion coverage").or_default().insert(format!("{}: absent after {route}", location(&g, &graph_name, Some(*id))));
            }
        }
        if let (Some(a), Some(b)) = (direct.graph.nodes.get(id), lowered.graph.nodes.get(id)) {
            if a != b {
                failures
                    .entry("Import path disagreement")
                    .or_default()
                    .insert(format!("{}: direct import and editor conversion differ\n  direct: {a:?}\n  editor: {b:?}", location(&g, &graph_name, Some(*id))));
            }
        }
        // Preserve an explicit diagnostic even if the importer forgets to flag a
        // placeholder in its report.
        if let Some(reason) = lowered.graph.nodes.get(id).and_then(|n| n.strs.get("import_note")) {
            failures.entry("Unsupported conversions and unavailable resources").or_default().insert(format!("graph '{graph_name}', node {id} ({}): {reason}", node.kind));
        }
    }

    let ids: Vec<_> = g.nodes.keys().copied().collect();
    let rendered = lowered.graph.evaluate_all(32, &ids);
    for (id, error) in &rendered.errors {
        failures.entry("Node rendering failures at 32x32").or_default().insert(format!("{}: {error}", location(&g, &graph_name, Some(*id))));
    }
    for id in &ids {
        match rendered.kept.get(id) {
            Some(t) => {
                if let Some(problem) = texture_problem(t) {
                    failures.entry("Invalid node textures").or_default().insert(format!("{}: {problem}", location(&g, &graph_name, Some(*id))));
                }
            }
            None if !rendered.errors.contains_key(id) => {
                failures.entry("Unevaluated nodes").or_default().insert(format!("{}: no texture and no error returned", location(&g, &graph_name, Some(*id))));
            }
            None => {}
        }
    }

    match g.evaluate(Some(64)) {
        Err(e) => {
            failures.entry("Output rendering at 64x64").or_default().insert(e.to_string());
        }
        Ok(maps) => {
            let names: Vec<_> = maps.keys().map(String::as_str).collect();
            if names != OUTPUTS {
                failures.entry("Output rendering at 64x64").or_default().insert(format!("expected {OUTPUTS:?}, got {names:?}"));
            }
            let direct_maps = direct.graph.evaluate(Some(64)).expect("direct import outputs render");
            for (name, tex) in maps {
                if direct_maps.get(&name).map(|t| &t.px) != Some(&tex.px) {
                    failures.entry("Import path disagreement").or_default().insert(format!("output '{name}': native and direct import pixels differ"));
                }
                if (tex.w, tex.h) != (64, 64) {
                    failures.entry("Output rendering at 64x64").or_default().insert(format!("output '{name}': expected 64x64, got {}x{}", tex.w, tex.h));
                }
                if let Some(problem) = texture_problem(&tex) {
                    failures.entry("Output rendering at 64x64").or_default().insert(format!("output '{name}': {problem}"));
                }
            }
        }
    }

    // fixture-authored problems are reported apart and do not fail the check
    for details in failures.values_mut() {
        let moved: Vec<String> = details.iter().filter(|d| fixture_limitation(d).is_some()).cloned().collect();
        for d in moved {
            details.remove(&d);
            limitations.insert(format!("{d}\n    why it is not a failure: {}", fixture_limitation(&d).unwrap_or_default()));
        }
    }
    failures.retain(|_, details| !details.is_empty());
    let mut diagnostic = format!(
        "{FIXTURE}: full compatibility check\nDirect import: {}\nEditor conversion: {}\nEvaluated all {} compositing nodes: {} returned textures, {} errors.\n",
        direct.report.summary(),
        lowered.report.summary(),
        ids.len(),
        rendered.kept.len(),
        rendered.errors.len()
    );
    diagnostic.push_str("Output rendering alone is insufficient: this fixture has 553 disconnected nodes.\nSuccessful gray placeholders do not count as implemented nodes.\n");
    for (section, details) in &failures {
        let _ = write!(diagnostic, "\n{section} ({} diagnostics):\n", details.len());
        for detail in details {
            let _ = writeln!(diagnostic, "  - {detail}");
        }
    }
    let _ = write!(diagnostic, "\nFixture limitations ({} diagnostics; how the fixture was authored, not failure criteria):\n", limitations.len());
    for detail in &limitations {
        let _ = writeln!(diagnostic, "  - {detail}");
    }
    // Approximate algorithms are supported by design, but still surface their
    // limitations. Do not make exact parity a requirement for this import test.
    let _ = write!(diagnostic, "\nApproximate behavior ({} diagnostics; informational, not failure criteria):\n", approximations.len());
    diagnostic.push_str("Routine library-to-MaterialCraft substitutions are omitted here; behavior warnings follow.\n");
    for detail in approximations.iter().filter(|detail| !detail.contains("uses MaterialCraft's own")) {
        let _ = writeln!(diagnostic, "  - {detail}");
    }
    diagnostic.push_str("\nReproduce: cargo test --test sbs_all_nodes -- --nocapture\nFix the listed adapters/definitions, resource resolution/decoding, or fixture setup; do not accept a missing-node count as success.\n");
    assert!(failures.is_empty(), "{diagnostic}");
    println!("{diagnostic}");
}
