// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared SBS fixture and export helpers. Each integration suite uses a subset.
#![allow(dead_code)]
use materialcraft::{graph::Graph, sbs_native as native};
use std::path::{Path, PathBuf};

pub const FIXTURE: &str = "tests/assets/all_nodes.sbs";

pub fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE)
}

pub fn open(path: &Path, stage: &str) -> Graph {
    let (g, warnings) = native::open(path).unwrap_or_else(|e| panic!("{stage}: {}: {e}", path.display()));
    assert!(warnings.is_empty(), "{stage}: {}: {}", path.display(), warnings.join("\n"));
    g
}

pub fn fixture() -> Graph {
    open(&fixture_path(), "import fixture")
}

pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new() -> Self {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("materialcraft_sbs_{}_{stamp}", std::process::id()));
        std::fs::create_dir(&path).unwrap_or_else(|e| panic!("create {}: {e}", path.display()));
        Self(path)
    }

    pub fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        materialcraft::safeio::write_file(&path, text.as_bytes()).unwrap_or_else(|e| panic!("export {}: {e}", path.display()));
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The caller retains the directory while using the reopened package. Linked resources
/// resolve from the exported file's real location and the normal application fallbacks.
pub fn save_reopen(g: &Graph) -> (Graph, String, TempDir) {
    let text = native::save_text(g).unwrap();
    materialcraft::sbs::Package::parse(&text).expect("saved XML parses through the render importer");
    let dir = TempDir::new();
    let back = open(&dir.write("edited.sbs", &text), "reopen edited export");
    (back, text, dir)
}

pub fn same_bytes(expected: &str, actual: &str, stage: &str) {
    if expected == actual {
        return;
    }
    let at = expected.bytes().zip(actual.bytes()).position(|(a, b)| a != b).unwrap_or(expected.len().min(actual.len()));
    let excerpt = |s: &str| String::from_utf8_lossy(&s.as_bytes()[at.saturating_sub(60)..(at + 60).min(s.len())]).into_owned();
    panic!("{stage}: first byte difference at {at}; expected {} bytes, got {}.\nExpected: {:?}\nActual: {:?}", expected.len(), actual.len(), excerpt(expected), excerpt(actual));
}

pub fn same_package(expected: &Graph, actual: &Graph, stage: &str) {
    let before = native::all_graphs(expected);
    let after = native::all_graphs(actual);
    assert_eq!(before.keys().collect::<Vec<_>>(), after.keys().collect::<Vec<_>>(), "{stage}: graph names");
    for (name, a) in before {
        let b = after[&name];
        assert_eq!(a.nodes.keys().collect::<Vec<_>>(), b.nodes.keys().collect::<Vec<_>>(), "{stage}: {name}: node UIDs");
        for (id, n) in &a.nodes {
            assert_eq!(n, &b.nodes[id], "{stage}: {name}: node {id} ({})", n.kind);
        }
        assert_eq!((a.size, a.height), (b.size, b.height), "{stage}: {name}: dimensions");
        assert_eq!(a.output_nodes(), b.output_nodes(), "{stage}: {name}: outputs");
        let (a, b) = (a.sd.as_ref().unwrap(), b.sd.as_ref().unwrap());
        assert_eq!(a.role, b.role, "{stage}: {name}: role");
        assert_eq!(a.root, b.root, "{stage}: {name}: result node");
        assert_eq!(a.base, b.base, "{stage}: {name}: base parameters");
    }
    assert!(expected.resources == actual.resources, "{stage}: embedded resource names or bytes");
}
