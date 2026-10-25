// SPDX-License-Identifier: MIT OR Apache-2.0
//! One native-to-render conversion per editor cache. A structural snapshot detects edits without
//! writing and reparsing the entire package on every unchanged preview.
use super::{lower, SdGraph};
use crate::graph::{Graph, Node};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct LowerCache {
    last: Option<Entry>,
}

struct Entry {
    view: Graph,
    graph: Arc<Graph>,
    cwd: Option<PathBuf>,
    files: Vec<Option<PathBuf>>,
}

impl LowerCache {
    pub fn get(&mut self, view: &Graph) -> Result<Arc<Graph>, String> {
        let sd = view.sd.as_ref().ok_or("not an .sbs graph")?;
        // External editable packages can change independently, including their transitive dependencies.
        // Keep importing those normally until their complete dependency tree can be tracked.
        let external = sd.pkg.doc.root.child("dependencies").is_some_and(|d| d.elems().any(|x| {
            let f = x.v("filename").unwrap_or_default();
            let f = f.trim();
            !f.is_empty() && f != "?himself" && !f.starts_with("sbs://")
        }));
        if external {
            self.last = None;
            return lower(view).map(|l| Arc::new(l.graph));
        }
        let cwd = std::env::current_dir().ok();
        let files = resource_locations(sd);
        if let Some(e) = &self.last {
            if e.cwd == cwd && e.files == files && same_view(&e.view, view) {
                return Ok(e.graph.clone());
            }
        }
        let graph = Arc::new(lower(view)?.graph);
        self.last = Some(Entry { view: view.clone(), graph: graph.clone(), cwd, files });
        Ok(graph)
    }
}

fn resource_locations(sd: &SdGraph) -> Vec<Option<PathBuf>> {
    let dir = sd.pkg.file.as_ref().and_then(|f| f.parent()).unwrap_or_else(|| std::path::Path::new("."));
    let stem = sd.pkg.file.as_ref().and_then(|f| f.file_stem()).map(|s| s.to_string_lossy()).unwrap_or_else(|| "package".into());
    let dirs = [dir.to_path_buf(), dir.join(format!("{stem}.resources")), dir.join("resources")];
    // A file appearing in a higher-priority folder must invalidate a recovered path. File contents
    // at an unchanged path are handled by the renderer's existing resource hashes.
    sd.pkg.resources.iter().filter(|r| r.embedded.is_none()).map(|r| r.filepath.as_ref().and_then(|p| crate::resources::resolve_in(p, &dirs))).collect()
}

fn same_nodes(a: &BTreeMap<u32, Node>, b: &BTreeMap<u32, Node>) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|((ai, a), (bi, b))| {
        ai == bi && a.kind == b.kind && a.strs == b.strs && a.ins == b.ins && a.in_out == b.in_out
            && a.pos.map(f32::to_bits) == b.pos.map(f32::to_bits)
            && a.nums.len() == b.nums.len()
            && a.nums.iter().zip(&b.nums).all(|((ak, av), (bk, bv))| ak == bk && av.to_bits() == bv.to_bits())
    })
}

fn same_sd(a: &Option<Box<SdGraph>>, b: &Option<Box<SdGraph>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            // Holding the snapshot's Arc makes Arc::make_mut detach on any package edit, including
            // embedded resource changes. No XML or image-byte comparisons are needed on a hit.
            Arc::ptr_eq(&a.pkg, &b.pkg) && a.role == b.role && a.path == b.path && a.uid == b.uid
                && a.inputs == b.inputs && a.outputs == b.outputs && a.base == b.base && a.root == b.root && a.gui == b.gui
                && match (&a.orig, &b.orig) {
                    (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                    (None, None) => true,
                    _ => false,
                }
        }
        (None, None) => true,
        _ => false,
    }
}

fn same_view(a: &Graph, b: &Graph) -> bool {
    a.format == b.format && a.size == b.size && a.height == b.height && a.seed_offset == b.seed_offset
        && a.outputs == b.outputs && a.output_pins == b.output_pins && a.meta == b.meta && same_sd(&a.sd, &b.sd)
        && same_nodes(&a.nodes, &b.nodes)
        && a.resources.len() == b.resources.len()
        && a.resources.iter().zip(&b.resources).all(|((ak, av), (bk, bv))| ak == bk && Arc::ptr_eq(av, bv))
        && a.exposed.len() == b.exposed.len() && a.exposed.iter().zip(&b.exposed).all(|(a, b)| {
            a.label == b.label && a.node == b.node && a.param == b.param && a.min.to_bits() == b.min.to_bits() && a.max.to_bits() == b.max.to_bits()
        })
        && a.comments.len() == b.comments.len() && a.comments.iter().zip(&b.comments).all(|(a, b)| {
            a.text == b.text && a.color == b.color && a.pos.map(f32::to_bits) == b.pos.map(f32::to_bits) && a.size.map(f32::to_bits) == b.size.map(f32::to_bits)
        })
        && a.subgraphs.len() == b.subgraphs.len()
        && a.subgraphs.iter().zip(&b.subgraphs).all(|((ak, av), (bk, bv))| ak == bk && same_view(av, bv))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sbs_defs::PTy;
    use crate::sbs_native::{self as native, PVal};
    use crate::xmldoc::{XElem, XNode};

    fn color(raw: &str) -> PVal {
        PVal::Const { ty: PTy::F(4), raw: raw.into(), rel: Some("0".into()) }
    }

    #[test]
    fn unchanged_packages_reuse_conversion_and_edits_invalidate_it() {
        let mut g = native::new_package("cached", 32);
        g.nodes.insert(1, native::new_node("sbs:uniform", None, [0.0, 0.0]));
        native::set_value(g.nodes.get_mut(&1).unwrap(), "outputcolor", Some(color("0.2 0.4 0.6 1")));
        let mut cache = LowerCache::default();
        let first = cache.get(&g).unwrap();
        assert!(Arc::ptr_eq(&first, &cache.get(&g).unwrap()));
        assert!(Arc::ptr_eq(&first, &cache.get(&g.clone()).unwrap()), "editor snapshots should reuse the conversion");
        let edits: &[fn(&mut Graph)] = &[
            |g| native::set_value(g.nodes.get_mut(&1).unwrap(), "outputcolor", Some(color("0.7 0.3 0.1 1"))),
            |g| { g.nodes.get_mut(&1).unwrap().ins.insert("input".into(), 2); },
            |g| { g.nodes.get_mut(&1).unwrap().in_out.insert("input".into(), "alpha".into()); },
            |g| { g.sd.as_mut().unwrap().base.insert("outputsize".into(), "i2@0:5 4".into()); },
            |g| { g.sd.as_mut().unwrap().inputs.push(XElem::new("paraminput")); },
            |g| { g.sd.as_mut().unwrap().outputs.push(XElem::new("graphoutput")); },
            |g| { g.meta.insert("label".into(), "Changed".into()); },
            |g| { Arc::make_mut(&mut g.sd.as_mut().unwrap().pkg).file = Some(PathBuf::from("moved/package.sbs")); },
            |g| { g.nodes.insert(2, native::new_node("sbs:uniform", None, [0.0, 0.0])); },
        ];
        for edit in edits {
            let before = cache.get(&g).unwrap();
            let mut edited = g.clone();
            edit(&mut edited);
            let after = cache.get(&edited).unwrap();
            assert!(!Arc::ptr_eq(&before, &after), "a graph edit reused its old conversion");
            assert!(Arc::ptr_eq(&after, &cache.get(&edited).unwrap()));
        }

        let pkg = g.sd.as_ref().unwrap().pkg.clone();
        let name = native::make_function(&mut g.subgraphs, pkg, "cached", 1, "outputcolor", &color("0.2 0.4 0.6 1"));
        native::set_value(g.nodes.get_mut(&1).unwrap(), "outputcolor", Some(PVal::Dyn { graph: name.clone(), rel: Some("0".into()) }));
        let old = cache.get(&g).unwrap();
        let fun = g.subgraphs.get_mut(&name).unwrap();
        let mut other = fun.nodes[&1].clone();
        native::set_value(&mut other, "const_float4", Some(color("0.8 0.1 0.3 1")));
        fun.nodes.insert(2, other);
        let added = cache.get(&g).unwrap();
        assert!(!Arc::ptr_eq(&old, &added));
        g.subgraphs.get_mut(&name).unwrap().sd.as_mut().unwrap().root = Some(2);
        let changed_root = cache.get(&g).unwrap();
        assert!(!Arc::ptr_eq(&added, &changed_root));
        assert_ne!(old.evaluate_node(1, 16).unwrap().px, changed_root.evaluate_node(1, 16).unwrap().px);
        assert_eq!(changed_root.evaluate_node(1, 16).unwrap().px, native::lower(&g).unwrap().graph.evaluate_node(1, 16).unwrap().px);
    }

    #[test]
    fn external_packages_are_not_cached() {
        let mut g = native::new_package("external", 16);
        let pkg = Arc::make_mut(&mut g.sd.as_mut().unwrap().pkg);
        let mut dep = XElem::new("dependency");
        dep.children.extend([XNode::Elem(XElem::with_v("filename", "linked.sbs")), XNode::Elem(XElem::with_v("uid", "123"))]);
        pkg.doc.root.child_mut("dependencies").unwrap().children.push(XNode::Elem(dep));
        let mut cache = LowerCache::default();
        let a = cache.get(&g).unwrap();
        let b = cache.get(&g).unwrap();
        assert!(!Arc::ptr_eq(&a, &b));
        assert!(cache.last.is_none());
    }

    #[test]
    fn all_nodes_reuses_the_whole_package_conversion() {
        let (g, _) = native::load(include_str!("../tests/assets/all_nodes.sbs"), None).unwrap();
        let mut cache = LowerCache::default();
        let first = cache.get(&g).unwrap();
        for _ in 0..3 {
            assert!(Arc::ptr_eq(&first, &cache.get(&g.clone()).unwrap()));
        }
        // A package resource edit detaches its Arc and invalidates even when no node changes.
        let mut changed = g.clone();
        let pkg = Arc::make_mut(&mut changed.sd.as_mut().unwrap().pkg);
        pkg.resources[0].filepath = Some("changed.png".into());
        assert!(!Arc::ptr_eq(&first, &cache.get(&changed).unwrap()));
    }
}
