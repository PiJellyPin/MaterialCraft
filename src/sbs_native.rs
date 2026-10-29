// SPDX-License-Identifier: MIT OR Apache-2.0
//! `.sbs` packages edited as they are: the editor's graphs hold the file's own nodes and values, and saving writes
//! the file back with only what was changed rewritten (an unchanged package saves to the same bytes).
//!
//! A package becomes a [`Graph`] (the first compositing graph) whose `subgraphs` hold the package's other graphs,
//! its functions, and the function graphs inside nodes (function-driven values, pixel processors, FX-maps). Every
//! one carries an [`SdGraph`] that says where it lives in the file. Nodes use the kinds of [`crate::sbs_defs`]
//! (`sbs:blend`, `sbs:lib:<package>/<graph>`, `sbs:fn:add` ...); a node's id is its uid in the file. Its values are
//! kept in `strs` exactly as the file writes them (see [`PVal`]); keys starting with `#` hold what the file needs
//! besides values (output uids, gradient keys ...). To render, [`lower`] turns the package into MaterialCraft nodes
//! through the importer, so what is rendered is what is saved.

use crate::graph::{Comment, Graph, Node};
use crate::sbs_defs::{self, Fam, PTy};
use crate::xmldoc::{Indent, XDoc, XElem, XNode};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[path = "sbs_native_cache.rs"]
mod preview_cache;
pub(crate) use preview_cache::LowerCache;

/// Positions in the editor are the file's positions times this: 1, so nodes sit as densely as in the format's own
/// editor (a node card is about as wide as the format's node spacing allows).
pub const POS_SCALE: f32 = 1.0;

/// Largest package opened (embedded bitmaps make these big).
pub const MAX_FILE: u64 = 1 << 30;

// ------------------------------------------------------------------------------------------------ data

/// Where a graph of the package lives in the file.
#[derive(Debug, Clone, PartialEq)]
pub enum Role {
    /// a compositing graph of the package
    Graph,
    /// a function of the package
    Function,
    /// the function that computes parameter `param` of node `node` of graph `owner` (a function-driven value,
    /// the body of a pixel or value processor)
    Param { owner: String, node: u32, param: String },
    /// the FX-map graph number `index` of node `node` of graph `owner`
    Fx { owner: String, node: u32, index: usize },
}

/// What the package as a whole keeps: the document it was read from and what rendering needs from it.
#[derive(Debug, Clone, Default)]
pub struct SdPackage {
    /// the file as read (or the empty package a new one starts from)
    pub doc: XDoc,
    pub file: Option<PathBuf>,
    /// resources (bitmaps, fonts, SVG) as read, for rendering
    pub resources: Vec<crate::sbs::Resource>,
}

/// What a graph of an `.sbs` package keeps besides its nodes.
#[derive(Debug, Clone)]
pub struct SdGraph {
    pub role: Role,
    /// `folder/identifier` for graphs and functions
    pub path: String,
    pub uid: String,
    /// graph inputs (`paramInputs`), as written
    pub inputs: Vec<XElem>,
    /// graph outputs (`graphOutputs`), as written
    pub outputs: Vec<XElem>,
    /// graph settings (`baseParameters`) by name, encoded as node values
    pub base: BTreeMap<String, String>,
    /// function graphs: the node whose value is the result
    pub root: Option<u32>,
    /// the comments and frames of `Graph::comments`, as read: (type, text shown) to tell what changed
    pub gui: Vec<(String, String)>,
    /// function and FX-map graphs: their nodes and result as read, so an unchanged one is written as it was
    pub orig: Option<Arc<(BTreeMap<u32, Node>, Option<u32>)>>,
    pub pkg: Arc<SdPackage>,
}

impl SdGraph {
    pub fn new(role: Role, path: &str, pkg: Arc<SdPackage>) -> SdGraph {
        SdGraph { role, path: path.to_string(), uid: String::new(), inputs: Vec::new(), outputs: Vec::new(), base: BTreeMap::new(), root: None, gui: Vec::new(), orig: None, pkg }
    }

    /// The function or FX-map graph `g` (which carries this) is as it was read.
    pub fn unchanged(&self, g: &Graph) -> bool {
        self.orig.as_ref().map(|o| o.0 == g.nodes && o.1 == self.root).unwrap_or(false)
    }

    /// Identifier of graph output `uid`.
    pub fn output_identifier(&self, uid: &str) -> Option<String> {
        self.outputs.iter().find(|o| o.v("uid").as_deref() == Some(uid)).and_then(|o| o.v("identifier"))
    }

    /// Usage of graph output `uid` (`baseColor`, `normal` ...).
    pub fn output_usage(&self, uid: &str) -> Option<String> {
        let o = self.outputs.iter().find(|o| o.v("uid").as_deref() == Some(uid))?;
        o.child("usages").and_then(|u| u.elems().find_map(|x| x.v("name")))
    }

    /// Identifier of graph input `uid`.
    pub fn input_identifier(&self, uid: &str) -> Option<String> {
        self.inputs.iter().find(|o| o.v("uid").as_deref() == Some(uid)).and_then(|o| o.v("identifier"))
    }
}

/// A value on a native node.
#[derive(Debug, Clone, PartialEq)]
pub enum PVal {
    /// a constant: its type and the text the file writes (`0.5`, `1 1 1 1`)
    Const { ty: PTy, raw: String, rel: Option<String> },
    /// computed by the function graph `graph` of the package
    Dyn { graph: String, rel: Option<String> },
}

impl PVal {
    /// `<type>[@<relativeTo>]:<value>` or `dyn[@<relativeTo>]:<graph>`.
    pub fn enc(&self) -> String {
        let rel = |r: &Option<String>| r.as_ref().map(|r| format!("@{r}")).unwrap_or_default();
        match self {
            PVal::Const { ty, raw, rel: r } => format!("{}{}:{raw}", ty.code(), rel(r)),
            PVal::Dyn { graph, rel: r } => format!("dyn{}:{graph}", rel(r)),
        }
    }

    pub fn dec(s: &str) -> Option<PVal> {
        let (head, raw) = s.split_once(':')?;
        let (ty, rel) = match head.split_once('@') {
            Some((t, r)) => (t, Some(r.to_string())),
            None => (head, None),
        };
        if ty == "dyn" {
            return Some(PVal::Dyn { graph: raw.to_string(), rel });
        }
        Some(PVal::Const { ty: PTy::parse(ty)?, raw: raw.to_string(), rel })
    }

    /// The numbers of a constant (booleans 0 / 1); empty for text and functions.
    pub fn nums(&self) -> Vec<f32> {
        match self {
            PVal::Const { ty: PTy::B, raw, .. } => vec![if matches!(raw.trim(), "1" | "true" | "True") { 1.0 } else { 0.0 }],
            PVal::Const { ty, raw, .. } if ty.lanes() > 0 => raw.split_whitespace().filter_map(|x| x.parse::<f32>().ok()).map(|x| if x.is_finite() { x } else { 0.0 }).collect(),
            _ => Vec::new(),
        }
    }

    /// A constant of type `ty` holding `v` (as the file writes numbers).
    pub fn of_nums(ty: PTy, v: &[f32], rel: Option<String>) -> PVal {
        let raw = match ty {
            PTy::B => (if v.first().copied().unwrap_or(0.0) != 0.0 { "1" } else { "0" }).to_string(),
            PTy::I(_) => v.iter().map(|x| format!("{}", x.round() as i64)).collect::<Vec<_>>().join(" "),
            _ => v.iter().map(|x| fmt_num(*x)).collect::<Vec<_>>().join(" "),
        };
        PVal::Const { ty, raw, rel }
    }
}

/// A float as the file writes it: the shortest text that reads back the same.
pub fn fmt_num(x: f32) -> String {
    if !x.is_finite() {
        return "0".into();
    }
    let s = format!("{x}");
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

/// The values of a native node by parameter name (keys that are not values are skipped).
pub fn values(n: &Node) -> BTreeMap<String, PVal> {
    n.strs.iter().filter(|(k, _)| !k.starts_with('#') && k.as_str() != "title").filter_map(|(k, v)| Some((k.clone(), PVal::dec(v)?))).collect()
}

/// A package is native when its graph carries `.sbs` data.
pub fn is_native(g: &Graph) -> bool {
    g.sd.is_some()
}

/// A graph the editor lists as a graph of its package: compositing graphs and functions, not the function and
/// FX-map graphs inside nodes (nor graphs copied in to feed a node).
pub fn listed(g: &Graph) -> bool {
    match &g.sd {
        Some(sd) => matches!(sd.role, Role::Graph | Role::Function),
        None => !g.meta.contains_key("embedded"),
    }
}

// ------------------------------------------------------------------------------------------------ reading

fn floats(s: &str) -> Vec<f32> {
    s.split_whitespace().filter_map(|x| x.parse::<f32>().ok()).map(|x| if x.is_finite() { x } else { 0.0 }).collect()
}

fn read_pos(e: &XElem) -> [f32; 2] {
    let g = e.child("GUILayout").and_then(|l| l.v("gpos")).map(|s| floats(&s)).unwrap_or_default();
    [g.first().copied().unwrap_or(0.0) * POS_SCALE, g.get(1).copied().unwrap_or(0.0) * POS_SCALE]
}

/// A `constantValue*` element as a value.
fn read_const(e: &XElem, rel: Option<String>) -> Option<PVal> {
    let ty = PTy::of_element(&e.name)?;
    let raw = e.attr("v").unwrap_or_else(|| e.text());
    Some(PVal::Const { ty, raw, rel })
}

fn const_in(e: &XElem) -> Option<PVal> {
    e.elems().find_map(|c| read_const(c, None))
}

struct Loader {
    pkg: Arc<SdPackage>,
    graphs: BTreeMap<String, Graph>,
    order: Vec<String>,
    warnings: Vec<String>,
}

/// Node ids from uids: the uid when it is a free number, else a free id (kept in `#uid`).
#[derive(Default)]
struct Ids {
    taken: BTreeSet<u32>,
    next: u32,
}

impl Ids {
    fn claim(&mut self, uid: &str) -> (u32, bool) {
        match uid.trim().parse::<u32>() {
            Ok(i) if i != 0 && i < u32::MAX && self.taken.insert(i) => (i, true),
            _ => (self.fresh(), false),
        }
    }
    fn fresh(&mut self) -> u32 {
        loop {
            self.next += 1;
            if self.taken.insert(self.next) {
                return self.next;
            }
        }
    }
}

/// Read a package file.
pub fn open(path: &Path) -> Result<(Graph, Vec<String>), String> {
    let len = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    if len > MAX_FILE {
        return Err(format!("{}: file is larger than {} MB", path.display(), MAX_FILE >> 20));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = String::from_utf8(bytes).map_err(|_| format!("{}: not a UTF-8 text file", path.display()))?;
    load(&text, Some(path.to_path_buf()))
}

/// Read a package from its text. Returns the package and what could not be read.
pub fn load(text: &str, file: Option<PathBuf>) -> Result<(Graph, Vec<String>), String> {
    let doc = XDoc::parse(text).map_err(|e| format!("not a readable package ({e})"))?;
    if !doc.root.name.eq_ignore_ascii_case("package") {
        return Err(format!("not a package file (root element is <{}>)", doc.root.name));
    }
    let simple = crate::sbs::Package::from_root(&doc.root.to_element())?;
    let resources = simple.resources.clone();
    let pkg = Arc::new(SdPackage { doc, file, resources });
    let mut l = Loader { pkg: pkg.clone(), graphs: BTreeMap::new(), order: Vec::new(), warnings: simple.warnings.clone() };
    if let Some(c) = pkg.doc.root.child("content") {
        l.content(c, "", 0);
    }
    if l.order.is_empty() {
        return Err("the package contains no graphs".into());
    }
    // the first compositing graph is the one the package opens on; the others sit next to it
    let first = l.order.iter().find(|n| l.graphs.get(*n).map(|g| matches!(g.sd.as_ref().map(|s| &s.role), Some(Role::Graph))).unwrap_or(false)).cloned().unwrap_or_else(|| l.order[0].clone());
    let mut root = l.graphs.remove(&first).unwrap_or_default();
    root.meta.insert("identifier".into(), first.clone());
    root.subgraphs = std::mem::take(&mut l.graphs);
    root.resources = embedded_resources(&pkg);
    Ok((root, l.warnings))
}

/// Embedded resources by the key the importer gives them, for the nodes that read them.
fn embedded_resources(pkg: &SdPackage) -> BTreeMap<String, Arc<Vec<u8>>> {
    let mut m = BTreeMap::new();
    for r in &pkg.resources {
        if let Some(b) = &r.embedded {
            let ext = r.format.clone().unwrap_or_else(|| "png".into()).to_ascii_lowercase();
            m.insert(format!("{}.{}", crate::sbs_import::san(&r.identifier), ext), Arc::new(b.clone()));
        }
    }
    m
}

impl Loader {
    fn content(&mut self, c: &XElem, folder: &str, depth: usize) {
        if depth > 64 {
            self.warnings.push("folders nested too deeply; the deepest were skipped".into());
            return;
        }
        for e in c.elems() {
            let path = |e: &XElem| {
                let id = e.v("identifier").unwrap_or_default();
                if folder.is_empty() {
                    id
                } else {
                    format!("{folder}/{id}")
                }
            };
            match e.name.to_ascii_lowercase().as_str() {
                "graph" => {
                    let p = path(e);
                    self.graph(e, &p);
                }
                "function" => {
                    let p = path(e);
                    let mut sd = SdGraph::new(Role::Function, &p, self.pkg.clone());
                    sd.uid = e.v("uid").unwrap_or_default();
                    sd.inputs = e.child("paramInputs").map(|l| l.elems().cloned().collect()).unwrap_or_default();
                    let body = e.child("paramValue").and_then(|v| v.child("dynamicValue"));
                    let g = self.func_graph(body, sd, &p);
                    self.add(p, g);
                }
                "group" => {
                    let p = path(e);
                    if let Some(inner) = e.child("content") {
                        self.content(inner, &p, depth + 1);
                    }
                }
                "graphs" | "functions" | "groups" | "content" => self.content(e, folder, depth + 1),
                _ => {}
            }
        }
    }

    fn add(&mut self, name: String, g: Graph) {
        if self.graphs.contains_key(&name) {
            self.warnings.push(format!("two graphs are called '{name}'; the second one is not shown"));
            return;
        }
        self.order.push(name.clone());
        self.graphs.insert(name, g);
    }

    fn graph(&mut self, e: &XElem, path: &str) {
        let mut sd = SdGraph::new(Role::Graph, path, self.pkg.clone());
        sd.uid = e.v("uid").unwrap_or_default();
        sd.inputs = e.child("paramInputs").map(|l| l.elems().cloned().collect()).unwrap_or_default();
        sd.outputs = e.child("graphOutputs").map(|l| l.elems().cloned().collect()).unwrap_or_default();
        let mut g = Graph::default();
        g.meta.insert("identifier".into(), path.to_string());
        if let Some(a) = e.child("attributes") {
            for k in ["label", "description", "category", "author", "authorURL", "tags", "userTags"] {
                if let Some(v) = a.v(k) {
                    g.meta.insert(k.to_string(), v);
                }
            }
        }
        for p in e.child("baseParameters").map(|l| l.elems().collect::<Vec<_>>()).unwrap_or_default() {
            if let Some((name, v, _)) = read_param(p) {
                sd.base.insert(name, v.enc());
            }
        }
        // MaterialCraft's own graph settings (3D preview defaults, the default format of new nodes)
        for key in MC_OPTIONS {
            if let Some(val) = e.child("options").and_then(|o| o.elems().find(|x| x.v("name").as_deref() == Some(*key))).and_then(|x| x.v("value")) {
                g.meta.insert((*key).to_string(), val);
            }
        }
        // the size the graph is shown at: its own absolute size, else the size it is opened with on its own
        let parent = e.child("options").and_then(|o| o.elems().find(|x| x.v("name").as_deref() == Some("defaultParentSize"))).and_then(|x| x.v("value"));
        if let Some((w, h)) = parent.as_deref().and_then(|p| p.split_once('x')).and_then(|(a, b)| Some((a.trim().parse::<u32>().ok()?, b.trim().parse::<u32>().ok()?))) {
            let (w, h) = (w.clamp(4, 13), h.clamp(4, 13));
            g.size = 1 << w;
            g.height = if h != w { 1 << h } else { 0 };
        }
        if let Some(PVal::Const { raw, rel, .. }) = sd.base.get("outputsize").and_then(|v| PVal::dec(v)) {
            let v = floats(&raw);
            if rel.as_deref().map(|r| r == "0").unwrap_or(true) && v.len() >= 2 {
                let (w, h) = (v[0].clamp(4.0, 13.0) as u32, v[1].clamp(4.0, 13.0) as u32);
                g.size = 1 << w;
                g.height = if h != w { 1 << h } else { 0 };
            }
        }
        let name = path.to_string();
        // nodes: ids first, so links can be read in any order
        let list: Vec<&XElem> = e.child("compNodes").map(|l| l.elems().collect()).unwrap_or_default();
        let mut ids = Ids::default();
        let mut by_uid: HashMap<String, u32> = HashMap::new();
        let mut keep_uid: Vec<bool> = Vec::new();
        for n in &list {
            let uid = n.v("uid").unwrap_or_default();
            let (id, same) = ids.claim(&uid);
            by_uid.insert(uid, id);
            keep_uid.push(same);
        }
        let mut nodes: Vec<(u32, Node)> = Vec::new();
        for (i, n) in list.iter().enumerate() {
            let uid = n.v("uid").unwrap_or_default();
            let id = by_uid[&uid];
            let mut node = self.comp_node(n, &name, id);
            if !keep_uid[i] {
                node.strs.insert("#uid".into(), uid);
            }
            nodes.push((id, node));
        }
        // links: the output a link reads is named when its node has several
        let outs: HashMap<u32, Vec<(String, String)>> = nodes.iter().map(|(id, n)| (*id, node_outputs(n))).collect();
        for (i, n) in list.iter().enumerate() {
            let id = nodes[i].0;
            for c in n.child("connections").map(|l| l.elems().collect::<Vec<_>>()).unwrap_or_default() {
                let (Some(slot), Some(from)) = (c.v("identifier").or_else(|| c.v("entry")), c.v("connRef")) else { continue };
                let Some(&src) = by_uid.get(&from) else {
                    self.warnings.push(format!("graph '{name}': node {id}: input '{slot}' comes from a node that is not in the graph"));
                    continue;
                };
                nodes[i].1.ins.insert(slot.clone(), src);
                if let Some(ou) = c.v("connRefOutput").filter(|s| !s.is_empty()) {
                    let list = &outs[&src];
                    match list.iter().find(|(u, _)| *u == ou) {
                        Some((_, ident)) if list.len() > 1 => {
                            nodes[i].1.in_out.insert(slot, ident.clone());
                        }
                        Some(_) => {}
                        None => {
                            nodes[i].1.in_out.insert(slot, format!("#{ou}"));
                        }
                    }
                }
            }
        }
        for (id, n) in nodes {
            g.nodes.insert(id, n);
        }
        // comments and frames
        for o in e.child("GUIObjects").map(|l| l.elems().collect::<Vec<_>>()).unwrap_or_default() {
            let kind = o.v("type").unwrap_or_else(|| "COMMENT".into());
            let title = o.v("title").filter(|s| !s.is_empty());
            let text = o.v("GUIName").unwrap_or_default();
            let shown = match (&title, text.is_empty()) {
                (Some(t), true) => t.clone(),
                (Some(t), false) => format!("{t}: {text}"),
                (None, _) => text.clone(),
            };
            let layout = o.child("GUILayout");
            let gp = layout.and_then(|l| l.v("gpos")).map(|s| floats(&s)).unwrap_or_default();
            let size = layout.and_then(|l| l.v("size")).map(|s| floats(&s)).filter(|s| s.len() >= 2).map(|s| [s[0] * POS_SCALE, s[1] * POS_SCALE]).unwrap_or([220.0, 60.0]);
            g.comments.push(Comment { text: shown.clone(), pos: [gp.first().copied().unwrap_or(0.0) * POS_SCALE, gp.get(1).copied().unwrap_or(0.0) * POS_SCALE], size, color: 0 });
            sd.gui.push((kind, shown));
        }
        g.sd = Some(Box::new(sd));
        self.add(name, g);
    }

    /// A node of a compositing graph.
    fn comp_node(&mut self, e: &XElem, gname: &str, id: u32) -> Node {
        let mut n = Node { pos: read_pos(e), ..Default::default() };
        if let Some(t) = e.v("GUIName").filter(|s| !s.is_empty()) {
            n.strs.insert("title".into(), t);
        }
        if e.v("disabled").as_deref() == Some("1") {
            n.strs.insert("#disabled".into(), "1".into());
        }
        if let Some(o) = e.child("compOutputs") {
            n.strs.insert("#outputs".into(), o.to_xml());
        }
        let imp = e.child("compImplementation").and_then(|c| c.elems().next());
        let Some(imp) = imp else {
            n.kind = "sbs:unknown".into();
            return n;
        };
        match imp.name.to_ascii_lowercase().as_str() {
            "compfilter" => {
                let filter = imp.v("filter").unwrap_or_default();
                n.kind = sbs_defs::kind_of(Fam::Texture, &filter);
                self.params(imp, &mut n, gname, id);
                if let Some(a) = imp.child("paramsArrays") {
                    n.strs.insert("#arrays".into(), a.to_xml());
                }
                if let Some(pg) = imp.child("paramsGraphs") {
                    let mut names = Vec::new();
                    for (i, fx) in pg.elems().enumerate() {
                        let name = format!("{gname}/{id}/fx{i}");
                        let g = self.fx_graph(fx, &name, gname, id, i);
                        names.push(name.clone());
                        self.add(name, g);
                    }
                    n.strs.insert("#fx".into(), names.join(","));
                }
            }
            "compinstance" => {
                let path = imp.v("path").unwrap_or_default();
                n.strs.insert("#path".into(), path.clone());
                n.kind = self.instance_kind(&path);
                if n.kind == "sbs:graph" {
                    let (gpath, _) = crate::sbs::split_path(&path);
                    n.strs.insert("#graph".into(), gpath);
                }
                self.params(imp, &mut n, gname, id);
                if let Some(b) = imp.child("outputBridgings") {
                    n.strs.insert("#bridgings".into(), b.to_xml());
                }
            }
            "compinputbridge" => {
                n.kind = "sbs:input".into();
                n.strs.insert("#entry".into(), imp.v("entry").unwrap_or_default());
                self.params(imp, &mut n, gname, id);
            }
            "compoutputbridge" => {
                n.kind = "sbs:output".into();
                n.strs.insert("#output".into(), imp.v("output").unwrap_or_default());
            }
            other => {
                n.kind = format!("sbs:unknown:{other}");
            }
        }
        n
    }

    /// The kind of an instance node from its path: a library graph, or a graph of this or another package.
    fn instance_kind(&self, path: &str) -> String {
        let (gpath, dep) = crate::sbs::split_path(path);
        let Some(dep) = dep else { return "sbs:graph".into() };
        let file = self.pkg.doc.root.child("dependencies").and_then(|d| d.elems().find(|x| x.v("uid").as_deref() == Some(dep.as_str()))).and_then(|x| x.v("filename")).unwrap_or_default();
        if file.trim().starts_with("sbs://") {
            let stem = crate::sbs_lib::package_stem(&file);
            let id = gpath.rsplit('/').next().unwrap_or(&gpath);
            let kind = format!("sbs:lib:{stem}/{id}");
            if sbs_defs::def(&kind).is_some() {
                return sbs_defs::def(&kind).map(|d| d.kind.to_string()).unwrap_or(kind);
            }
            return kind;
        }
        "sbs:graph".into()
    }

    /// The parameters of a filter, instance or input node.
    fn params(&mut self, imp: &XElem, n: &mut Node, gname: &str, id: u32) {
        let Some(list) = imp.child("parameters") else { return };
        for p in list.elems() {
            let Some((name, v, dynv)) = read_param(p) else { continue };
            match (v, dynv) {
                (PVal::Dyn { rel, .. }, Some(d)) => {
                    let fname = format!("{gname}/{id}/{name}");
                    let sd = SdGraph::new(Role::Param { owner: gname.to_string(), node: id, param: name.clone() }, &fname, self.pkg.clone());
                    let g = self.func_graph(Some(d), sd, &fname);
                    self.add(fname.clone(), g);
                    n.strs.insert(name, PVal::Dyn { graph: fname, rel }.enc());
                }
                (v, _) => {
                    n.strs.insert(name, v.enc());
                }
            }
        }
    }

    /// A function graph (`dynamicValue`): its nodes and its result node.
    fn func_graph(&mut self, d: Option<&XElem>, mut sd: SdGraph, name: &str) -> Graph {
        let mut g = Graph::default();
        g.meta.insert("kind".into(), "function".into());
        g.meta.insert("identifier".into(), name.to_string());
        let Some(d) = d else {
            g.sd = Some(Box::new(sd));
            return g;
        };
        let list: Vec<&XElem> = d.child("paramNodes").map(|l| l.elems().collect()).unwrap_or_default();
        let mut ids = Ids::default();
        let mut by_uid: HashMap<String, u32> = HashMap::new();
        for pn in &list {
            let uid = pn.v("uid").unwrap_or_default();
            let (id, same) = ids.claim(&uid);
            by_uid.insert(uid.clone(), id);
            let mut n = Node { pos: read_pos(pn), ..Default::default() };
            if pn.child("GUILayout").is_none() {
                n.strs.insert("#nolayout".into(), "1".into());
            }
            let f = pn.v("function").unwrap_or_default();
            n.kind = sbs_defs::kind_of(Fam::Function, &f);
            if !same {
                n.strs.insert("#uid".into(), uid);
            }
            if let Some(t) = pn.v("type") {
                n.strs.insert("#type".into(), t);
            }
            if let Some(t) = pn.v("GUIName").filter(|s| !s.is_empty()) {
                n.strs.insert("title".into(), t);
            }
            for fd in pn.child("funcDatas").map(|l| l.elems().collect::<Vec<_>>()).unwrap_or_default() {
                let Some(dn) = fd.v("name") else { continue };
                if let Some(v) = fd.child("constantValue").and_then(const_in).or_else(|| const_in(fd)) {
                    n.strs.insert(dn, v.enc());
                }
            }
            g.nodes.insert(id, n);
        }
        for pn in &list {
            let uid = pn.v("uid").unwrap_or_default();
            let id = by_uid[&uid];
            for c in pn.child("connections").map(|l| l.elems().collect::<Vec<_>>()).unwrap_or_default() {
                let (Some(slot), Some(from)) = (c.v("identifier").or_else(|| c.v("entry")), c.v("connRef")) else { continue };
                if let (Some(&src), Some(n)) = (by_uid.get(&from), g.nodes.get_mut(&id)) {
                    n.ins.insert(slot, src);
                }
            }
        }
        sd.root = d.v("rootnode").or_else(|| d.v("rootNode")).and_then(|r| by_uid.get(&r).copied());
        sd.orig = Some(Arc::new((g.nodes.clone(), sd.root)));
        g.sd = Some(Box::new(sd));
        g
    }

    /// An FX-map graph (`paramsGraph`): its nodes with the settings of their data (inherited ones included).
    fn fx_graph(&mut self, e: &XElem, name: &str, owner: &str, node: u32, index: usize) -> Graph {
        let mut g = Graph::default();
        g.meta.insert("kind".into(), "fxmap".into());
        g.meta.insert("identifier".into(), name.to_string());
        let sd = SdGraph::new(Role::Fx { owner: owner.to_string(), node, index }, name, self.pkg.clone());
        let datas: Vec<&XElem> = e.child("paramsGraphDatas").map(|l| l.elems().collect()).unwrap_or_default();
        let data_of = |uid: &str| datas.iter().copied().find(|d| d.v("uid").as_deref() == Some(uid));
        let list: Vec<&XElem> = e.child("paramsGraphNodes").map(|l| l.elems().collect()).unwrap_or_default();
        let mut ids = Ids::default();
        let mut by_uid: HashMap<String, u32> = HashMap::new();
        for pn in &list {
            let uid = pn.v("uid").unwrap_or_default();
            let (id, same) = ids.claim(&uid);
            by_uid.insert(uid.clone(), id);
            let data_uid = pn.v("data").unwrap_or_default();
            let data = data_of(&data_uid);
            let ty = pn.v("type").or_else(|| data.and_then(|d| d.v("type"))).unwrap_or_default();
            let mut n = Node { kind: sbs_defs::kind_of(Fam::Pattern, &ty), pos: read_pos(pn), ..Default::default() };
            if !same {
                n.strs.insert("#uid".into(), uid);
            }
            n.strs.insert("#data".into(), data_uid);
            if pn.v("disabled").as_deref() == Some("1") {
                n.strs.insert("#disabled".into(), "1".into());
            }
            // a data set may inherit what it does not set from another one
            let mut seen: Vec<String> = Vec::new();
            let mut cur = data;
            while let Some(d) = cur {
                let du = d.v("uid").unwrap_or_default();
                if seen.contains(&du) || seen.len() > 32 {
                    break;
                }
                seen.push(du);
                for p in d.child("parameters").map(|l| l.elems().collect::<Vec<_>>()).unwrap_or_default() {
                    let Some((pname, v, dynv)) = read_param(p) else { continue };
                    if n.strs.contains_key(&pname) {
                        continue;
                    }
                    match (v, dynv) {
                        (PVal::Dyn { rel, .. }, Some(dv)) => {
                            let fname = format!("{name}/{id}/{pname}");
                            let fsd = SdGraph::new(Role::Param { owner: name.to_string(), node: id, param: pname.clone() }, &fname, self.pkg.clone());
                            let fg = self.func_graph(Some(dv), fsd, &fname);
                            self.add(fname.clone(), fg);
                            n.strs.insert(pname, PVal::Dyn { graph: fname, rel }.enc());
                        }
                        (v, _) => {
                            n.strs.insert(pname, v.enc());
                        }
                    }
                }
                cur = d.v("inherit").filter(|s| !s.is_empty()).and_then(|u| data_of(&u));
            }
            g.nodes.insert(id, n);
        }
        for pn in &list {
            let id = by_uid[&pn.v("uid").unwrap_or_default()];
            for c in pn.child("connections").map(|l| l.elems().collect::<Vec<_>>()).unwrap_or_default() {
                let (Some(slot), Some(from)) = (c.v("identifier").or_else(|| c.v("entry")), c.v("connRef")) else { continue };
                if let (Some(&src), Some(n)) = (by_uid.get(&from), g.nodes.get_mut(&id)) {
                    n.ins.insert(slot.clone(), src);
                    if let Some(o) = c.v("connRefOutput").filter(|s| !s.is_empty()) {
                        n.in_out.insert(slot, o);
                    }
                }
            }
        }
        let mut sd = sd;
        sd.root = e.v("rootnode").or_else(|| e.v("rootNode")).and_then(|r| by_uid.get(&r).copied());
        sd.uid = e.v("uid").unwrap_or_default();
        sd.orig = Some(Arc::new((g.nodes.clone(), sd.root)));
        g.sd = Some(Box::new(sd));
        g
    }
}

/// `(name, value, function)` of a `parameter` element.
fn read_param(p: &XElem) -> Option<(String, PVal, Option<&XElem>)> {
    let name = p.v("name")?;
    let rel = p.v("relativeTo");
    let pv = p.child("paramValue")?;
    if let Some(d) = pv.child("dynamicValue") {
        return Some((name, PVal::Dyn { graph: String::new(), rel }, Some(d)));
    }
    let v = pv.elems().find_map(|c| read_const(c, rel.clone()))?;
    Some((name, v, None))
}

/// `(output uid, identifier)` of each output of a node, in order.
pub fn node_outputs(n: &Node) -> Vec<(String, String)> {
    let uids: Vec<String> = n
        .strs
        .get("#outputs")
        .and_then(|x| XDoc::parse(x).ok())
        .map(|d| d.root.elems().filter_map(|o| o.v("uid")).collect())
        .unwrap_or_default();
    let bridged: Vec<(String, String)> = n
        .strs
        .get("#bridgings")
        .and_then(|x| XDoc::parse(x).ok())
        .map(|d| d.root.elems().filter_map(|b| Some((b.v("uid")?, b.v("identifier")?))).collect())
        .unwrap_or_default();
    let names = output_names(n);
    uids.iter()
        .enumerate()
        .map(|(i, u)| {
            let ident = bridged.iter().find(|(b, _)| b == u).map(|(_, id)| id.clone()).or_else(|| names.get(i).cloned()).unwrap_or_else(|| "output".into());
            (u.clone(), ident)
        })
        .collect()
}

/// Output identifiers of a node kind (for nodes that do not list theirs).
fn output_names(n: &Node) -> Vec<String> {
    sbs_defs::def(&n.kind).map(|d| d.outputs.iter().map(|s| s.to_string()).collect()).unwrap_or_default()
}

// ------------------------------------------------------------------------------------------------ writing

/// Writes native graphs as elements. `pkg` is the package the graphs belong to (for dependencies).
struct Writer<'a> {
    /// every graph of the package by name
    graphs: &'a BTreeMap<String, &'a Graph>,
    /// dependencies: filename -> uid (existing ones, plus those added)
    deps: Vec<(String, String)>,
    /// a counter for new uids
    seed: u64,
    /// nodes whose output is grayscale (decided before the nodes are written)
    gray: std::collections::HashSet<u32>,
}

/// Grayscale propagation: library generators are gray, and so is a blend/levels/… whose inputs all are.
fn gray_set(g: &Graph) -> std::collections::HashSet<u32> {
    let mut set = std::collections::HashSet::new();
    let keep = ["sbs:blend", "sbs:levels", "sbs:blur", "sbs:curve", "sbs:sharpen", "sbs:invert"];
    for (id, n) in &g.nodes {
        let lib = sbs_defs::def(&n.kind).and_then(|d| d.library).map(|(_, gr)| !["color", "normal", "material", "gradient"].iter().any(|w| gr.contains(w))).unwrap_or(false);
        if lib || matches!(values(n).get("colorswitch"), Some(PVal::Const { raw, .. }) if raw.trim() == "0") {
            set.insert(*id);
        }
    }
    loop {
        let mut changed = false;
        for (id, n) in &g.nodes {
            if set.contains(id) || !keep.contains(&n.kind.as_str()) || n.ins.is_empty() {
                continue;
            }
            if n.ins.values().all(|s| set.contains(s)) {
                set.insert(*id);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    set
}

fn v(name: &str, value: &str) -> XElem {
    XElem::with_v(name, value)
}

fn push(e: &mut XElem, c: XElem) {
    e.empty = false;
    e.children.push(XNode::Elem(c));
}

fn list(name: &str, items: Vec<XElem>) -> XElem {
    let mut e = XElem::new(name);
    for i in items {
        push(&mut e, i);
    }
    e
}

fn gui_layout(pos: [f32; 2]) -> XElem {
    let mut l = XElem::new("GUILayout");
    push(&mut l, v("gpos", &format!("{} {} 0", fmt_num(pos[0] / POS_SCALE), fmt_num(pos[1] / POS_SCALE))));
    l
}

/// A constant value element.
fn const_elem(ty: PTy, raw: &str) -> XElem {
    v(ty.element(), raw)
}

impl<'a> Writer<'a> {
    fn new_uid(&mut self) -> String {
        // uids look like the ones the format's own tools make: large numbers, unique in their context
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        format!("{}", 1_000_000_000 + (self.seed >> 33) % 3_000_000_000)
    }

    /// The uid of dependency `filename`, added when the package does not have it yet.
    fn dependency(&mut self, filename: &str) -> String {
        if let Some((_, u)) = self.deps.iter().find(|(f, _)| f == filename) {
            return u.clone();
        }
        let u = self.new_uid();
        self.deps.push((filename.to_string(), u.clone()));
        u
    }

    /// The `paramValue` of a value.
    fn param_value(&mut self, v: &PVal) -> XElem {
        let mut pv = XElem::new("paramValue");
        match v {
            PVal::Const { ty, raw, .. } => push(&mut pv, const_elem(*ty, raw)),
            PVal::Dyn { graph, .. } => {
                let d = match self.graphs.get(graph.as_str()) {
                    Some(g) => self.dynamic_value(g),
                    None => XElem::new("dynamicValue"),
                };
                push(&mut pv, d);
            }
        }
        pv
    }

    fn parameter(&mut self, name: &str, v: &PVal) -> XElem {
        let mut p = XElem::new("parameter");
        push(&mut p, XElem::with_v("name", name));
        let rel = match v {
            PVal::Const { rel, .. } | PVal::Dyn { rel, .. } => rel.clone(),
        };
        if let Some(r) = rel {
            push(&mut p, v_rel(&r));
        }
        let pv = self.param_value(v);
        push(&mut p, pv);
        p
    }

    fn parameters(&mut self, n: &Node) -> Option<XElem> {
        let vals = values(n);
        if vals.is_empty() {
            return None;
        }
        let items: Vec<XElem> = vals.iter().map(|(k, val)| self.parameter(k, val)).collect();
        Some(list("parameters", items))
    }

    /// A function graph as a `dynamicValue`.
    fn dynamic_value(&mut self, g: &Graph) -> XElem {
        let mut d = XElem::new("dynamicValue");
        // a function without a result node stays without one (it yields its parameter's default, as in the file)
        let root = g.sd.as_ref().and_then(|s| s.root);
        if let Some(r) = root {
            let uid = g.nodes.get(&r).map(|n| uid_of(r, n)).unwrap_or_else(|| r.to_string());
            push(&mut d, v("rootnode", &uid));
        }
        let mut nodes = Vec::new();
        for (id, n) in &g.nodes {
            let mut pn = XElem::new("paramNode");
            push(&mut pn, v("uid", &uid_of(*id, n)));
            if let Some(t) = n.strs.get("title") {
                push(&mut pn, v("GUIName", t));
            }
            let f = n.kind.strip_prefix("sbs:fn:").unwrap_or(&n.kind).to_string();
            push(&mut pn, v("function", &f));
            if let Some(t) = n.strs.get("#type") {
                push(&mut pn, v("type", t));
            }
            if !n.ins.is_empty() {
                let mut cs = Vec::new();
                for (slot, src) in &n.ins {
                    let mut c = XElem::new("connection");
                    push(&mut c, v("identifier", slot));
                    let su = g.nodes.get(src).map(|s| uid_of(*src, s)).unwrap_or_else(|| src.to_string());
                    push(&mut c, v("connRef", &su));
                    cs.push(c);
                }
                push(&mut pn, list("connections", cs));
            }
            let vals = values(n);
            if !vals.is_empty() {
                let mut fds = Vec::new();
                for (k, val) in &vals {
                    let PVal::Const { ty, raw, .. } = val else { continue };
                    let mut fd = XElem::new("funcData");
                    push(&mut fd, v("name", k));
                    let mut cv = XElem::new("constantValue");
                    push(&mut cv, const_elem(*ty, raw));
                    push(&mut fd, cv);
                    fds.push(fd);
                }
                push(&mut pn, list("funcDatas", fds));
            }
            if !(n.strs.contains_key("#nolayout") && n.pos == [0.0, 0.0]) {
                push(&mut pn, gui_layout(n.pos));
            }
            nodes.push(pn);
        }
        push(&mut d, list("paramNodes", nodes));
        d
    }

    /// An FX-map graph as a `paramsGraph`: one data set per node, holding all of the node's settings.
    fn fx_graph(&mut self, g: &Graph) -> XElem {
        let mut e = XElem::new("paramsGraph");
        let sd = g.sd.as_ref();
        if let Some(u) = sd.map(|s| s.uid.clone()).filter(|u| !u.is_empty()) {
            push(&mut e, v("uid", &u));
        }
        if let Some(r) = sd.and_then(|s| s.root) {
            let uid = g.nodes.get(&r).map(|n| uid_of(r, n)).unwrap_or_else(|| r.to_string());
            push(&mut e, v("rootnode", &uid));
        }
        let mut datas = Vec::new();
        let mut nodes = Vec::new();
        for (id, n) in &g.nodes {
            let ty = n.kind.strip_prefix("sbs:fx:").unwrap_or(&n.kind).to_string();
            let data_uid = n.strs.get("#data").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| self.new_uid());
            let mut d = XElem::new("paramsGraphData");
            push(&mut d, v("uid", &data_uid));
            push(&mut d, v("type", &ty));
            if let Some(p) = self.parameters(n) {
                push(&mut d, p);
            }
            datas.push(d);
            let mut pn = XElem::new("paramsGraphNode");
            push(&mut pn, v("uid", &uid_of(*id, n)));
            push(&mut pn, v("type", &ty));
            if n.strs.get("#disabled").map(|s| s == "1").unwrap_or(false) {
                push(&mut pn, v("disabled", "1"));
            }
            if !n.ins.is_empty() {
                let mut cs = Vec::new();
                for (slot, src) in &n.ins {
                    let mut c = XElem::new("connection");
                    push(&mut c, v("identifier", slot));
                    let su = g.nodes.get(src).map(|s| uid_of(*src, s)).unwrap_or_else(|| src.to_string());
                    push(&mut c, v("connRef", &su));
                    if let Some(o) = n.in_out.get(slot) {
                        push(&mut c, v("connRefOutput", o));
                    }
                    cs.push(c);
                }
                push(&mut pn, list("connections", cs));
            }
            push(&mut pn, v("data", &data_uid));
            push(&mut pn, gui_layout(n.pos));
            nodes.push(pn);
        }
        push(&mut e, list("paramsGraphDatas", datas));
        push(&mut e, list("paramsGraphNodes", nodes));
        e
    }

    /// `(uid, comptype)` of each output of node `n`, inventing them for a node that has none yet.
    fn outputs_of(&mut self, n: &Node) -> Vec<(String, Option<String>)> {
        if let Some(d) = n.strs.get("#outputs").and_then(|x| XDoc::parse(x).ok()) {
            let v: Vec<(String, Option<String>)> = d.root.elems().filter_map(|o| Some((o.v("uid")?, o.v("comptype")))).collect();
            if !v.is_empty() || n.kind == "sbs:output" {
                return v;
            }
        }
        if n.kind == "sbs:output" {
            return Vec::new();
        }
        let count = instance_outputs(n, self.graphs).len().max(1);
        (0..count).map(|_| (self.new_uid(), None)).collect()
    }

    /// A node of a compositing graph, written whole. `outs` are the uids of every node's outputs.
    fn comp_node(&mut self, gname: &str, id: u32, n: &Node, outs: &HashMap<u32, Vec<(String, String)>>, uids: &HashMap<u32, String>) -> XElem {
        let _ = gname;
        let mut e = XElem::new("compNode");
        push(&mut e, v("uid", &uids[&id]));
        if let Some(t) = n.strs.get("title").filter(|t| !t.is_empty()) {
            push(&mut e, v("GUIName", t));
        }
        if n.strs.get("#disabled").map(|s| s == "1").unwrap_or(false) {
            push(&mut e, v("disabled", "1"));
        }
        if let Some(c) = self.connections(n, outs, uids) {
            push(&mut e, c);
        }
        push(&mut e, gui_layout(n.pos));
        if let Some(o) = outs.get(&id).filter(|o| !o.is_empty()) {
            // colour unless the node is set to grey
            let gray = self.gray.contains(&id);
            let items = o
                .iter()
                .map(|(u, _)| {
                    let mut c = XElem::new("compOutput");
                    push(&mut c, v("uid", u));
                    push(&mut c, v("comptype", if gray { "2" } else { "1" }));
                    c
                })
                .collect();
            let mut co = list("compOutputs", items);
            // keep the output types the file had
            if let Some(d) = n.strs.get("#outputs").and_then(|x| XDoc::parse(x).ok()) {
                co = d.root;
            }
            push(&mut e, co);
        }
        let mut ci = XElem::new("compImplementation");
        push(&mut ci, self.implementation(n, outs.get(&id)));
        push(&mut e, ci);
        e
    }

    fn connections(&mut self, n: &Node, outs: &HashMap<u32, Vec<(String, String)>>, uids: &HashMap<u32, String>) -> Option<XElem> {
        if n.ins.is_empty() {
            return None;
        }
        let mut cs = Vec::new();
        for (slot, src) in &n.ins {
            let Some(su) = uids.get(src) else { continue };
            let mut c = XElem::new("connection");
            push(&mut c, v("identifier", slot));
            push(&mut c, v("connRef", su));
            let ou = match n.in_out.get(slot) {
                Some(o) if o.starts_with('#') => Some(o[1..].to_string()),
                Some(o) => outs.get(src).and_then(|l| l.iter().find(|(_, i)| i == o).map(|(u, _)| u.clone())),
                None => outs.get(src).and_then(|l| l.first().map(|(u, _)| u.clone())),
            };
            if let Some(ou) = ou {
                push(&mut c, v("connRefOutput", &ou));
            }
            cs.push(c);
        }
        Some(list("connections", cs))
    }

    /// The `compFilter` / `compInstance` / bridge element of a node.
    fn implementation(&mut self, n: &Node, outs: Option<&Vec<(String, String)>>) -> XElem {
        match n.kind.as_str() {
            "sbs:input" => {
                let mut b = XElem::new("compInputBridge");
                push(&mut b, v("entry", n.strs.get("#entry").map(String::as_str).unwrap_or("")));
                if let Some(p) = self.parameters(n) {
                    push(&mut b, p);
                }
                b
            }
            "sbs:output" => {
                let mut b = XElem::new("compOutputBridge");
                push(&mut b, v("output", n.strs.get("#output").map(String::as_str).unwrap_or("")));
                b
            }
            k if k.starts_with("sbs:lib:") || k == "sbs:graph" => {
                let mut ci = XElem::new("compInstance");
                let path = self.instance_path(n);
                push(&mut ci, v("path", &path));
                if let Some(p) = self.parameters(n) {
                    push(&mut ci, p);
                }
                let bridged = n.strs.get("#bridgings").and_then(|x| XDoc::parse(x).ok()).map(|d| d.root);
                match bridged {
                    Some(b) => push(&mut ci, b),
                    None => {
                        let names = instance_outputs(n, self.graphs);
                        let items: Vec<XElem> = outs
                            .map(|o| {
                                o.iter()
                                    .enumerate()
                                    .map(|(i, (u, _))| {
                                        let mut b = XElem::new("outputBridging");
                                        push(&mut b, v("uid", u));
                                        push(&mut b, v("identifier", names.get(i).map(String::as_str).unwrap_or("output")));
                                        b
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        if !items.is_empty() {
                            push(&mut ci, list("outputBridgings", items));
                        }
                    }
                }
                ci
            }
            k => {
                let mut f = XElem::new("compFilter");
                let filter = k.strip_prefix("sbs:").unwrap_or(k);
                push(&mut f, v("filter", filter));
                // the editor wants a `parameters` member even when every value is the default
                push(&mut f, self.parameters(n).unwrap_or_else(|| XElem::new("parameters")));
                if let Some(a) = n.strs.get("#arrays").and_then(|x| XDoc::parse(x).ok()) {
                    push(&mut f, a.root);
                }
                if let Some(names) = n.strs.get("#fx") {
                    let mut gs = Vec::new();
                    for name in names.split(',').filter(|s| !s.is_empty()) {
                        if let Some(g) = self.graphs.get(name) {
                            gs.push(self.fx_graph(g));
                        }
                    }
                    if !gs.is_empty() {
                        push(&mut f, list("paramsGraphs", gs));
                    }
                }
                f
            }
        }
    }

    /// The path an instance node writes: what the file had while the node still points at the same graph.
    fn instance_path(&mut self, n: &Node) -> String {
        // a library graph MaterialCraft does not know: its kind names the package and the graph
        let lib = sbs_defs::def(&n.kind).and_then(|d| d.library).map(|(p, g)| (p.to_string(), g.to_string())).or_else(|| n.kind.strip_prefix("sbs:lib:").and_then(|r| r.split_once('/')).map(|(p, g)| (p.to_string(), g.to_string())));
        if let Some((pkg, id)) = lib {
            let id = id.as_str();
            if let Some(p) = n.strs.get("#path") {
                if crate::sbs::split_path(p).0.rsplit('/').next() == Some(id) {
                    return p.clone();
                }
            }
            let dep = self.dependency(&format!("sbs://{pkg}.sbs"));
            return format!("pkg:///{id}?dependency={dep}");
        }
        let target = n.strs.get("#graph").cloned().unwrap_or_default();
        if let Some(p) = n.strs.get("#path") {
            if crate::sbs::split_path(p).0 == target {
                return p.clone();
            }
        }
        // a graph of this package: the format names the package itself as a dependency
        let dep = self.dependency("?himself");
        format!("pkg:///{target}?dependency={dep}")
    }
}

fn v_rel(r: &str) -> XElem {
    XElem::with_v("relativeTo", r)
}

/// The uid a node is written with.
pub fn uid_of(id: u32, n: &Node) -> String {
    n.strs.get("#uid").cloned().unwrap_or_else(|| id.to_string())
}

/// Output identifiers of an instance node: those of its graph.
fn instance_outputs(n: &Node, graphs: &BTreeMap<String, &Graph>) -> Vec<String> {
    if let Some(d) = sbs_defs::def(&n.kind).filter(|d| d.library.is_some()) {
        return d.outputs.iter().map(|s| s.to_string()).collect();
    }
    if n.kind == "sbs:graph" {
        if let Some(g) = n.strs.get("#graph").and_then(|t| graphs.get(t.as_str())) {
            if let Some(sd) = &g.sd {
                return sd.outputs.iter().filter_map(|o| o.v("identifier")).collect();
            }
        }
    }
    output_names(n)
}

// ------------------------------------------------------------------------------------------------ saving

/// Every graph of a package by name (the package's own graph under its identifier).
pub fn all_graphs(root: &Graph) -> BTreeMap<String, &Graph> {
    let mut m: BTreeMap<String, &Graph> = root.subgraphs.iter().map(|(k, g)| (k.clone(), g)).collect();
    let name = root.meta.get("identifier").cloned().unwrap_or_default();
    m.insert(name, root);
    m
}

/// The package as file text: the document it was read from with every change written into it.
pub fn save_text(root: &Graph) -> Result<String, String> {
    Ok(sync(root)?.to_text())
}

/// The document of a package with the changes made since it was read.
pub fn sync(root: &Graph) -> Result<XDoc, String> {
    let sd = root.sd.as_ref().ok_or("this graph was not read from an .sbs package")?;
    let mut doc = sd.pkg.doc.clone();
    let ind = doc.indent();
    let graphs = all_graphs(root);
    let deps: Vec<(String, String)> = doc.root.child("dependencies").map(|d| d.elems().filter_map(|x| Some((x.v("filename")?, x.v("uid")?))).collect()).unwrap_or_default();
    let mut w = Writer { graphs: &graphs, deps: deps.clone(), seed: 0x9e37_79b9_7f4a_7c15 ^ (graphs.len() as u64), gray: Default::default() };
    // the graphs and functions of the package, where they are in the document
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for (name, g) in &graphs {
        let Some(gsd) = &g.sd else { continue };
        match gsd.role {
            Role::Graph => {
                seen.insert(gsd.uid.clone());
                let content = ensure_child(&mut doc.root, "content", &ind, 0);
                match find_by_uid(content, "graph", &gsd.uid, 1) {
                    Some((el, depth)) => sync_graph(&mut w, el, name, g, &ind, depth),
                    None => {
                        let el = new_graph_elem(&mut w, name, g);
                        insert_in_folder(content, &gsd.path, el, &ind, 1);
                    }
                }
            }
            Role::Function => {
                seen.insert(gsd.uid.clone());
                let content = ensure_child(&mut doc.root, "content", &ind, 0);
                match find_by_uid(content, "function", &gsd.uid, 1) {
                    Some((el, depth)) => sync_function(&mut w, el, g, &ind, depth),
                    None => {
                        let el = new_function_elem(&mut w, name, g);
                        insert_in_folder(content, &gsd.path, el, &ind, 1);
                    }
                }
            }
            _ => {}
        }
    }
    // graphs and functions that were deleted
    if let Some(content) = doc.root.child_mut("content") {
        remove_unseen(content, &seen);
    }
    // dependencies that were added
    let added: Vec<(String, String)> = w.deps.iter().filter(|d| !deps.contains(d)).cloned().collect();
    if !added.is_empty() {
        let dl = ensure_child(&mut doc.root, "dependencies", &ind, 0);
        for (f, u) in added {
            let mut d = XElem::new("dependency");
            push(&mut d, v("filename", &f));
            push(&mut d, v("uid", &u));
            push(&mut d, v("type", "package"));
            // the editor requires these two members in every dependency
            push(&mut d, v("fileUID", "0"));
            push(&mut d, v("versionUID", "0"));
            dl.insert_elem(d, None, &ind, 1);
        }
    }
    Ok(doc)
}

/// Add a new graph or function `el` to the package's `content` (at `depth`) inside the folders (`group`s) of its
/// `path`, made when missing.
fn insert_in_folder(content: &mut XElem, path: &str, el: XElem, ind: &Indent, depth: usize) {
    let mut parts: Vec<&str> = path.split('/').collect();
    parts.pop();
    let Some((folder, rest)) = parts.split_first() else {
        content.insert_elem(el, None, ind, depth);
        return;
    };
    let pos = content.children.iter().position(|c| matches!(c, XNode::Elem(e) if e.name == "group" && e.v("identifier").as_deref() == Some(*folder)));
    let i = match pos {
        Some(i) => i,
        None => {
            let mut gr = XElem::new("group");
            push(&mut gr, v("identifier", folder));
            push(&mut gr, XElem::new("content"));
            content.insert_elem(gr, None, ind, depth)
        }
    };
    let XNode::Elem(gr) = &mut content.children[i] else { return };
    let inner = gr.child_or_add("content", ind, depth + 1);
    // the rest of the path, under this folder
    let sub = rest.iter().copied().chain(std::iter::once(path.rsplit('/').next().unwrap_or(path))).collect::<Vec<_>>().join("/");
    insert_in_folder(inner, &sub, el, ind, depth + 2);
}

/// The order the format's own files write children in, by parent element.
fn child_order(parent: &str) -> &'static [&'static str] {
    match parent {
        "package" => &["identifier", "desc", "formatVersion", "updaterVersion", "fileUID", "versionUID", "dependencies", "content"],
        "graph" => &["identifier", "uid", "graphtype", "attributes", "paraminputs", "graphOutputs", "compNodes", "baseParameters", "GUIObjects", "options", "root"],
        "compNode" => &["uid", "GUIName", "disabled", "connections", "GUILayout", "compOutputs", "compImplementation"],
        "compFilter" => &["filter", "parameters", "paramsArrays", "paramsGraphs"],
        "compInstance" => &["path", "parameters", "outputBridgings"],
        "compInputBridge" => &["entry", "parameters"],
        "parameter" => &["name", "relativeTo", "paramValue"],
        "function" => &["identifier", "uid", "attributes", "paramInputs", "type", "paramValue"],
        _ => &[],
    }
}

/// Child `tag` of `parent`, added where the format's files have it when missing.
fn ensure_child<'e>(parent: &'e mut XElem, tag: &str, ind: &Indent, depth: usize) -> &'e mut XElem {
    if parent.child(tag).is_none() {
        let el = XElem::new(tag);
        add_ordered(parent, el, ind, depth);
    }
    parent.child_mut(tag).expect("just added")
}

/// Add `el` to `parent` before the first child the format writes after it.
fn add_ordered(parent: &mut XElem, el: XElem, ind: &Indent, depth: usize) {
    let order = child_order(&parent.name);
    let rank = |n: &str| order.iter().position(|o| o.eq_ignore_ascii_case(n));
    let at = match rank(&el.name) {
        Some(r) => parent.elems().position(|c| rank(&c.name).map(|cr| cr > r).unwrap_or(false)),
        None => None,
    };
    parent.insert_elem(el, at, ind, depth);
}

/// Set `<tag v=".."/>` of `parent`, adding it in the format's place when missing.
fn set_v_ordered(parent: &mut XElem, tag: &str, value: &str, ind: &Indent, depth: usize) {
    if parent.child(tag).is_none() {
        add_ordered(parent, XElem::with_v(tag, value), ind, depth);
    } else {
        parent.set_v(tag, value, ind, depth);
    }
}

/// The graph or function with this uid somewhere in a content tree (inside groups), and its depth.
fn find_by_uid<'e>(c: &'e mut XElem, tag: &str, uid: &str, depth: usize) -> Option<(&'e mut XElem, usize)> {
    if uid.is_empty() {
        return None;
    }
    // first find the path of indices, then walk it mutably
    fn path(c: &XElem, tag: &str, uid: &str, out: &mut Vec<usize>) -> bool {
        for (i, ch) in c.children.iter().enumerate() {
            let XNode::Elem(e) = ch else { continue };
            if e.name.eq_ignore_ascii_case(tag) && e.v("uid").as_deref() == Some(uid) {
                out.push(i);
                return true;
            }
            let n = e.name.to_ascii_lowercase();
            if n == "group" || n == "content" || n == "graphs" || n == "functions" || n == "groups" {
                out.push(i);
                if path(e, tag, uid, out) {
                    return true;
                }
                out.pop();
            }
        }
        false
    }
    let mut p = Vec::new();
    if !path(c, tag, uid, &mut p) {
        return None;
    }
    let d = depth + p.len();
    let mut cur = c;
    for i in p {
        let XNode::Elem(e) = &mut cur.children[i] else { return None };
        cur = e;
    }
    Some((cur, d))
}

fn remove_unseen(c: &mut XElem, seen: &BTreeSet<String>) {
    c.remove_where(|e| {
        let n = e.name.to_ascii_lowercase();
        (n == "graph" || n == "function") && !seen.contains(&e.v("uid").unwrap_or_default())
    });
    for e in c.elems_mut() {
        let n = e.name.to_ascii_lowercase();
        if n == "group" || n == "content" || n == "graphs" || n == "functions" || n == "groups" {
            remove_unseen(e, seen);
        }
    }
}

/// Make `parent`'s child list `tag` hold exactly `items` (layout-insensitive comparison; untouched when equal).
fn set_list(parent: &mut XElem, tag: &str, items: Vec<XElem>, ind: &Indent, depth: usize) {
    // the document's own spelling of the tag (`paraminputs` / `paramInputs`)
    let tag = parent.child(tag).map(|c| c.name.clone()).unwrap_or_else(|| tag.to_string());
    let tag = tag.as_str();
    let want = list(tag, items);
    let have = parent.child(tag).map(|c| c.to_element());
    if have.as_ref() == Some(&want.to_element()) {
        return;
    }
    if want.children.is_empty() {
        parent.remove_where(|e| e.name.eq_ignore_ascii_case(tag));
        return;
    }
    replace_child(parent, tag, want, ind, depth);
}

/// Replace child `tag` of `parent` (or add it last) with `el`, laid out like the document.
fn replace_child(parent: &mut XElem, tag: &str, mut el: XElem, ind: &Indent, depth: usize) {
    match parent.children.iter().position(|c| matches!(c, XNode::Elem(e) if e.name.eq_ignore_ascii_case(tag))) {
        Some(i) => {
            ind.apply(&mut el, depth + 1);
            parent.children[i] = XNode::Elem(el);
        }
        None => add_ordered(parent, el, ind, depth),
    }
}

/// Set `<tag v="..."/>` of `parent` when it differs (`None` removes it).
fn set_v_opt(parent: &mut XElem, tag: &str, value: Option<&str>, ind: &Indent, depth: usize) {
    match value {
        Some(val) => {
            if parent.v(tag).as_deref() != Some(val) {
                set_v_ordered(parent, tag, val, ind, depth);
            }
        }
        None => {
            parent.remove_where(|e| e.name == tag);
        }
    }
}

fn sync_graph(w: &mut Writer, el: &mut XElem, name: &str, g: &Graph, ind: &Indent, depth: usize) {
    let Some(sd) = &g.sd else { return };
    // identifier (a rename)
    let ident = name.rsplit('/').next().unwrap_or(name);
    set_v_opt(el, "identifier", Some(ident), ind, depth);
    // attributes
    let keys = ["label", "description", "category", "author", "authorURL", "tags", "userTags"];
    let have: Vec<(String, String)> = el.child("attributes").map(|a| keys.iter().filter_map(|k| Some((k.to_string(), a.v(k)?))).collect()).unwrap_or_default();
    let want: Vec<(String, String)> = keys.iter().filter_map(|k| Some((k.to_string(), g.meta.get(*k)?.clone()))).collect();
    if have != want {
        let a = ensure_child(el, "attributes", ind, depth);
        for k in keys {
            let val = g.meta.get(k).map(String::as_str);
            set_v_opt(a, k, val, ind, depth + 1);
        }
    }
    set_list(el, "paraminputs", sd.inputs.clone(), ind, depth);
    set_list(el, "graphOutputs", sd.outputs.clone(), ind, depth);
    sync_root_outputs(el, sd, ind, depth);
    // nodes
    sync_nodes(w, el, name, g, ind, depth);
    // settings
    sync_base(w, el, sd, ind, depth);
    // comments and frames
    sync_gui(el, g, sd, ind, depth);
    sync_mc_options(el, g, ind, depth);
}

/// Graph settings of MaterialCraft kept in the graph's `options` (other programs ignore options they do not know).
pub const MC_OPTIONS: &[&str] = &["mc_view", "mc_default_format"];

fn sync_mc_options(el: &mut XElem, g: &Graph, ind: &Indent, depth: usize) {
    for key in MC_OPTIONS {
        let want = g.meta.get(*key).cloned();
        let have = el.child("options").and_then(|o| o.elems().find(|x| x.v("name").as_deref() == Some(*key))).and_then(|x| x.v("value"));
        if want == have {
            continue;
        }
        let opts = ensure_child(el, "options", ind, depth);
        opts.remove_where(|o| o.v("name").as_deref() == Some(*key));
        if let Some(val) = want {
            let mut o = XElem::new("option");
            push(&mut o, v("name", key));
            push(&mut o, v("value", &val));
            opts.insert_elem(o, None, ind, depth + 1);
        }
    }
}

/// The `root` list of a graph names every graph output: entries for new outputs are added, those of removed
/// outputs dropped.
fn sync_root_outputs(el: &mut XElem, sd: &SdGraph, ind: &Indent, depth: usize) {
    let uids: Vec<String> = sd.outputs.iter().filter_map(|o| o.v("uid")).collect();
    let Some(root) = el.child_mut("root") else { return };
    let have: Vec<String> = root.child("rootOutputs").map(|l| l.elems().filter_map(|r| r.v("output")).collect()).unwrap_or_default();
    if have.iter().all(|u| uids.contains(u)) && uids.iter().all(|u| have.contains(u)) {
        return;
    }
    let ro = root.child_or_add("rootOutputs", ind, depth + 1);
    ro.remove_where(|r| !r.v("output").map(|u| uids.contains(&u)).unwrap_or(false));
    for u in uids.iter().filter(|u| !have.contains(u)) {
        let mut r = XElem::new("rootOutput");
        push(&mut r, v("output", u));
        push(&mut r, v("format", "0"));
        push(&mut r, v("usertag", ""));
        ro.insert_elem(r, None, ind, depth + 2);
    }
}

fn sync_base(w: &mut Writer, el: &mut XElem, sd: &SdGraph, ind: &Indent, depth: usize) {
    let have: BTreeMap<String, String> = el.child("baseParameters").map(|l| l.elems().filter_map(|p| read_param(p).map(|(n, v, _)| (n, v.enc()))).collect()).unwrap_or_default();
    if have == sd.base {
        return;
    }
    let bp = ensure_child(el, "baseParameters", ind, depth);
    sync_params(w, bp, &sd.base, ind, depth + 1);
}

/// Bring a `parameters` list in line with `want` (name -> encoded value): changed parameters are rewritten,
/// missing ones added, the ones no longer set removed; the others are left as they are.
fn sync_params(w: &mut Writer, plist: &mut XElem, want: &BTreeMap<String, String>, ind: &Indent, depth: usize) {
    let mut done: BTreeSet<String> = BTreeSet::new();
    let mut i = 0;
    while i < plist.children.len() {
        let XNode::Elem(p) = &plist.children[i] else {
            i += 1;
            continue;
        };
        let Some(name) = p.v("name") else {
            i += 1;
            continue;
        };
        match want.get(&name) {
            None => {
                plist.remove_at(i);
                continue;
            }
            Some(enc) => {
                done.insert(name.clone());
                let same = match (read_param(p), PVal::dec(enc)) {
                    (Some((_, PVal::Const { ty, raw, rel }, _)), Some(PVal::Const { ty: t2, raw: r2, rel: rl2 })) => ty == t2 && raw == r2 && rel == rl2,
                    (Some((_, PVal::Dyn { rel, .. }, Some(d))), Some(PVal::Dyn { graph, rel: rl2 })) => {
                        rel == rl2
                            && w.graphs.get(graph.as_str()).map(|g| g.sd.as_ref().map(|s| s.unchanged(g)).unwrap_or(false) || w.dynamic_value(g).to_element() == d.to_element()).unwrap_or(true)
                    }
                    _ => false,
                };
                if !same {
                    if let Some(val) = PVal::dec(enc) {
                        let mut np = w.parameter(&name, &val);
                        ind.apply(&mut np, depth + 1);
                        plist.children[i] = XNode::Elem(np);
                    }
                }
            }
        }
        i += 1;
    }
    for (name, enc) in want {
        if done.contains(name) {
            continue;
        }
        if let Some(val) = PVal::dec(enc) {
            let np = w.parameter(name, &val);
            plist.insert_elem(np, None, ind, depth);
        }
    }
}

fn sync_nodes(w: &mut Writer, el: &mut XElem, gname: &str, g: &Graph, ind: &Indent, depth: usize) {
    w.gray = gray_set(g);
    let uids: HashMap<u32, String> = g.nodes.iter().map(|(id, n)| (*id, uid_of(*id, n))).collect();
    // output uids of every node (new nodes get theirs now)
    let mut outs: HashMap<u32, Vec<(String, String)>> = HashMap::new();
    for (id, n) in &g.nodes {
        let o = w.outputs_of(n);
        let names = instance_outputs(n, w.graphs);
        let named: Vec<(String, String)> = match n.strs.get("#outputs") {
            Some(_) => node_outputs(n),
            None => o.iter().enumerate().map(|(i, (u, _))| (u.clone(), names.get(i).cloned().unwrap_or_else(|| "output".into()))).collect(),
        };
        outs.insert(*id, named);
    }
    let cn = ensure_child(el, "compNodes", ind, depth);
    let by_uid: HashMap<&str, u32> = uids.iter().map(|(id, u)| (u.as_str(), *id)).collect();
    // nodes that were removed
    cn.remove_where(|e| !e.v("uid").map(|u| by_uid.contains_key(u.as_str())).unwrap_or(false));
    let present: BTreeSet<String> = cn.elems().filter_map(|e| e.v("uid")).collect();
    let d1 = depth + 1;
    for e in cn.elems_mut() {
        let Some(u) = e.v("uid") else { continue };
        let Some(&id) = by_uid.get(u.as_str()) else { continue };
        let n = &g.nodes[&id];
        sync_node(w, e, gname, id, n, &outs, &uids, ind, d1 + 1);
    }
    for (id, n) in &g.nodes {
        if present.contains(&uids[id]) {
            continue;
        }
        let e = w.comp_node(gname, *id, n, &outs, &uids);
        cn.insert_elem(e, None, ind, d1);
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_node(w: &mut Writer, e: &mut XElem, gname: &str, id: u32, n: &Node, outs: &HashMap<u32, Vec<(String, String)>>, uids: &HashMap<u32, String>, ind: &Indent, depth: usize) {
    let _ = gname;
    set_v_opt(e, "GUIName", n.strs.get("title").filter(|t| !t.is_empty()).map(String::as_str), ind, depth);
    set_v_opt(e, "disabled", n.strs.get("#disabled").filter(|s| *s == "1").map(String::as_str), ind, depth);
    // position: rewritten only when it moved
    let have = e.child("GUILayout").and_then(|l| l.v("gpos")).map(|s| floats(&s)).unwrap_or_default();
    let want = [n.pos[0] / POS_SCALE, n.pos[1] / POS_SCALE];
    if have.len() < 2 || (have[0] - want[0]).abs() > 0.01 || (have[1] - want[1]).abs() > 0.01 {
        let z = have.get(2).copied().unwrap_or(0.0);
        let l = ensure_child(e, "GUILayout", ind, depth);
        l.set_v("gpos", &format!("{} {} {}", fmt_num(want[0]), fmt_num(want[1]), fmt_num(z)), ind, depth + 1);
    }
    // links
    let want_c = w.connections(n, outs, uids);
    let have_c: Vec<(String, String, Option<String>)> = e.child("connections").map(|c| c.elems().filter_map(|x| Some((x.v("identifier").or_else(|| x.v("entry"))?, x.v("connRef")?, x.v("connRefOutput")))).collect()).unwrap_or_default();
    let want_l: Vec<(String, String, Option<String>)> = want_c.as_ref().map(|c| c.elems().filter_map(|x| Some((x.v("identifier")?, x.v("connRef")?, x.v("connRefOutput")))).collect()).unwrap_or_default();
    let norm = |v: &[(String, String, Option<String>)]| {
        let mut s: Vec<(String, String, Option<String>)> = v.to_vec();
        s.sort();
        s
    };
    // a link the file wrote without naming the output reads the first one: the same link
    let same_links = norm(&have_c).iter().zip(norm(&want_l).iter()).all(|(a, b)| a.0 == b.0 && a.1 == b.1 && (a.2 == b.2 || a.2.is_none())) && have_c.len() == want_l.len();
    if !same_links {
        match want_c {
            Some(c) => replace_child(e, "connections", c, ind, depth),
            None => {
                e.remove_where(|x| x.name.eq_ignore_ascii_case("connections"));
            }
        }
    }
    // the implementation
    let kind_now = {
        let Some(imp) = e.child("compImplementation").and_then(|c| c.elems().next()) else { return };
        match imp.name.to_ascii_lowercase().as_str() {
            "compfilter" => Some(format!("sbs:{}", imp.v("filter").unwrap_or_default())),
            "compinputbridge" => Some("sbs:input".to_string()),
            "compoutputbridge" => Some("sbs:output".to_string()),
            "compinstance" => None,
            _ => Some(String::new()),
        }
    };
    let same_kind = match &kind_now {
        Some(k) => *k == n.kind,
        None => n.kind.starts_with("sbs:lib:") || n.kind == "sbs:graph",
    };
    if !same_kind {
        let mut fresh = w.implementation(n, outs.get(&id));
        ind.apply(&mut fresh, depth + 2);
        if let Some(ci) = e.child_mut("compImplementation") {
            if let Some(XNode::Elem(x)) = ci.children.iter_mut().find(|c| matches!(c, XNode::Elem(_))) {
                *x = fresh;
            }
        }
        return;
    }
    let Some(imp) = e.child_mut("compImplementation").and_then(|c| c.elems_mut().next()) else { return };
    let idepth = depth + 2;
    match n.kind.as_str() {
        "sbs:output" => set_v_opt(imp, "output", n.strs.get("#output").map(String::as_str), ind, idepth),
        "sbs:input" => {
            set_v_opt(imp, "entry", n.strs.get("#entry").map(String::as_str), ind, idepth);
            sync_node_params(w, imp, n, ind, idepth);
        }
        k if k.starts_with("sbs:lib:") || k == "sbs:graph" => {
            let path = w.instance_path(n);
            set_v_opt(imp, "path", Some(&path), ind, idepth);
            sync_node_params(w, imp, n, ind, idepth);
        }
        _ => {
            sync_node_params(w, imp, n, ind, idepth);
            // gradient keys and curve points
            match n.strs.get("#arrays").and_then(|x| XDoc::parse(x).ok()) {
                Some(a) => {
                    if imp.child("paramsArrays").map(|h| h.to_element()) != Some(a.root.to_element()) {
                        replace_child(imp, "paramsArrays", a.root, ind, idepth);
                    }
                }
                None => {
                    imp.remove_where(|x| x.name.eq_ignore_ascii_case("paramsArrays"));
                }
            }
            if let Some(names) = n.strs.get("#fx") {
                let fx: Vec<&Graph> = names.split(',').filter(|s| !s.is_empty()).filter_map(|nm| w.graphs.get(nm).copied()).collect();
                if !fx.iter().all(|g| g.sd.as_ref().map(|s| s.unchanged(g)).unwrap_or(false)) {
                    let gs: Vec<XElem> = fx.into_iter().map(|g| w.fx_graph(g)).collect();
                    set_list(imp, "paramsGraphs", gs, ind, idepth);
                }
            }
        }
    }
}

fn sync_node_params(w: &mut Writer, imp: &mut XElem, n: &Node, ind: &Indent, depth: usize) {
    let want: BTreeMap<String, String> = values(n).into_iter().map(|(k, v)| (k, v.enc())).collect();
    let have: BTreeMap<String, String> = imp.child("parameters").map(|l| l.elems().filter_map(|p| read_param(p).map(|(n, v, _)| (n, v.enc()))).collect()).unwrap_or_default();
    // function-driven values compare by their graphs, so look closer when any is present
    let any_dyn = want.values().any(|v| v.starts_with("dyn"));
    let same_names = have.keys().eq(want.keys());
    if !any_dyn && have == want {
        return;
    }
    if want.is_empty() {
        imp.remove_where(|x| x.name.eq_ignore_ascii_case("parameters"));
        return;
    }
    if !same_names || any_dyn || have != want {
        let pl = ensure_child(imp, "parameters", ind, depth);
        sync_params(w, pl, &want, ind, depth + 1);
    }
}

fn sync_gui(el: &mut XElem, g: &Graph, sd: &SdGraph, ind: &Indent, depth: usize) {
    // comments keep their place; only what changed is written
    let unchanged = g.comments.len() == sd.gui.len() && g.comments.iter().zip(sd.gui.iter()).all(|(c, (_, t))| c.text == *t);
    let objs = el.child("GUIObjects").map(|l| l.elems().count()).unwrap_or(0);
    if unchanged && objs == sd.gui.len() {
        // positions and sizes
        let Some(list) = el.child_mut("GUIObjects") else { return };
        for (o, c) in list.elems_mut().zip(g.comments.iter()) {
            let gp = o.child("GUILayout").and_then(|l| l.v("gpos")).map(|s| floats(&s)).unwrap_or_default();
            let want = [c.pos[0] / POS_SCALE, c.pos[1] / POS_SCALE];
            if gp.len() < 2 || (gp[0] - want[0]).abs() > 0.01 || (gp[1] - want[1]).abs() > 0.01 {
                let z = gp.get(2).copied().unwrap_or(0.0);
                let l = o.child_or_add("GUILayout", ind, depth + 2);
                l.set_v("gpos", &format!("{} {} {}", fmt_num(want[0]), fmt_num(want[1]), fmt_num(z)), ind, depth + 3);
            }
            let sz = o.child("GUILayout").and_then(|l| l.v("size")).map(|s| floats(&s)).unwrap_or_default();
            let wsz = [c.size[0] / POS_SCALE, c.size[1] / POS_SCALE];
            if sz.len() >= 2 && ((sz[0] - wsz[0]).abs() > 0.01 || (sz[1] - wsz[1]).abs() > 0.01) {
                let l = o.child_or_add("GUILayout", ind, depth + 2);
                l.set_v("size", &format!("{} {}", fmt_num(wsz[0]), fmt_num(wsz[1])), ind, depth + 3);
            }
        }
        return;
    }
    // comments were added, removed or edited: write the list again, keeping what each kept object had
    let old: Vec<XElem> = el.child("GUIObjects").map(|l| l.elems().cloned().collect()).unwrap_or_default();
    let mut items = Vec::new();
    for (i, c) in g.comments.iter().enumerate() {
        let mut o = match old.get(i).filter(|_| i < sd.gui.len()) {
            Some(o) => o.clone(),
            None => {
                let mut o = XElem::new("GUIObject");
                push(&mut o, v("type", "COMMENT"));
                o
            }
        };
        let kind = o.v("type").unwrap_or_else(|| "COMMENT".into());
        let shown_before = sd.gui.get(i).map(|x| x.1.clone());
        if shown_before.as_deref() != Some(c.text.as_str()) {
            if kind.eq_ignore_ascii_case("FRAME") && o.v("title").is_some() {
                o.set_v("title", &c.text, &Indent::default(), 0);
                o.remove_where(|x| x.name == "GUIName");
            } else {
                o.set_v("GUIName", &c.text, &Indent::default(), 0);
            }
        }
        let mut l = XElem::new("GUILayout");
        push(&mut l, v("gpos", &format!("{} {} 0", fmt_num(c.pos[0] / POS_SCALE), fmt_num(c.pos[1] / POS_SCALE))));
        push(&mut l, v("size", &format!("{} {}", fmt_num(c.size[0] / POS_SCALE), fmt_num(c.size[1] / POS_SCALE))));
        if o.child("GUILayout").is_some() {
            o.remove_where(|x| x.name == "GUILayout");
        }
        push(&mut o, l);
        items.push(o);
    }
    set_list(el, "GUIObjects", items, ind, depth);
}

fn sync_function(w: &mut Writer, el: &mut XElem, g: &Graph, ind: &Indent, depth: usize) {
    let Some(sd) = &g.sd else { return };
    let ident = sd.path.rsplit('/').next().unwrap_or(&sd.path).to_string();
    set_v_opt(el, "identifier", Some(&ident), ind, depth);
    set_list(el, "paramInputs", sd.inputs.clone(), ind, depth);
    if sd.unchanged(g) {
        return;
    }
    let want = w.dynamic_value(g);
    let have = el.child("paramValue").and_then(|p| p.child("dynamicValue")).map(|d| d.to_element());
    if have.as_ref() != Some(&want.to_element()) {
        let pv = ensure_child(el, "paramValue", ind, depth);
        replace_child(pv, "dynamicValue", want, ind, depth + 1);
    }
}

fn new_graph_elem(w: &mut Writer, name: &str, g: &Graph) -> XElem {
    let mut e = XElem::new("graph");
    let sd = g.sd.as_ref();
    let ident = name.rsplit('/').next().unwrap_or(name);
    push(&mut e, v("identifier", ident));
    let uid = sd.map(|s| s.uid.clone()).filter(|u| !u.is_empty()).unwrap_or_else(|| w.new_uid());
    push(&mut e, v("uid", &uid));
    let keys = ["label", "description", "category", "author", "authorURL", "tags", "userTags"];
    if keys.iter().any(|k| g.meta.contains_key(*k)) {
        let mut a = XElem::new("attributes");
        for k in keys {
            if let Some(val) = g.meta.get(k) {
                push(&mut a, v(k, val));
            }
        }
        push(&mut e, a);
    }
    if let Some(sd) = sd {
        if !sd.inputs.is_empty() {
            push(&mut e, list("paraminputs", sd.inputs.clone()));
        }
        push(&mut e, list("graphOutputs", sd.outputs.clone()));
    }
    let uids: HashMap<u32, String> = g.nodes.iter().map(|(id, n)| (*id, uid_of(*id, n))).collect();
    let mut outs: HashMap<u32, Vec<(String, String)>> = HashMap::new();
    for (id, n) in &g.nodes {
        let o = w.outputs_of(n);
        let names = instance_outputs(n, w.graphs);
        let named: Vec<(String, String)> = match n.strs.get("#outputs") {
            Some(_) => node_outputs(n),
            None => o.iter().enumerate().map(|(i, (u, _))| (u.clone(), names.get(i).cloned().unwrap_or_else(|| "output".into()))).collect(),
        };
        outs.insert(*id, named);
    }
    w.gray = gray_set(g);
    let nodes: Vec<XElem> = g.nodes.iter().map(|(id, n)| w.comp_node(name, *id, n, &outs, &uids)).collect();
    push(&mut e, list("compNodes", nodes));
    if let Some(sd) = sd {
        if !sd.base.is_empty() {
            let items: Vec<XElem> = sd.base.iter().filter_map(|(k, val)| PVal::dec(val).map(|p| w.parameter(k, &p))).collect();
            push(&mut e, list("baseParameters", items));
        }
    }
    if !g.comments.is_empty() {
        let items = g
            .comments
            .iter()
            .map(|c| {
                let mut o = XElem::new("GUIObject");
                push(&mut o, v("type", "COMMENT"));
                push(&mut o, v("GUIName", &c.text));
                let mut l = XElem::new("GUILayout");
                push(&mut l, v("gpos", &format!("{} {} 0", fmt_num(c.pos[0] / POS_SCALE), fmt_num(c.pos[1] / POS_SCALE))));
                push(&mut l, v("size", &format!("{} {}", fmt_num(c.size[0] / POS_SCALE), fmt_num(c.size[1] / POS_SCALE))));
                push(&mut o, l);
                o
            })
            .collect();
        push(&mut e, list("GUIObjects", items));
    }
    let mc: Vec<XElem> = MC_OPTIONS
        .iter()
        .filter_map(|k| {
            let val = g.meta.get(*k)?;
            let mut o = XElem::new("option");
            push(&mut o, v("name", k));
            push(&mut o, v("value", val));
            Some(o)
        })
        .collect();
    if !mc.is_empty() {
        push(&mut e, list("options", mc));
    }
    if let Some(sd) = sd {
        let roots: Vec<XElem> = sd
            .outputs
            .iter()
            .filter_map(|o| o.v("uid"))
            .map(|u| {
                let mut r = XElem::new("rootOutput");
                push(&mut r, v("output", &u));
                push(&mut r, v("format", "0"));
                push(&mut r, v("usertag", ""));
                r
            })
            .collect();
        let mut root = XElem::new("root");
        if !roots.is_empty() {
            push(&mut root, list("rootOutputs", roots));
        }
        push(&mut e, root);
    }
    e
}

fn new_function_elem(w: &mut Writer, name: &str, g: &Graph) -> XElem {
    let mut e = XElem::new("function");
    let ident = name.rsplit('/').next().unwrap_or(name);
    push(&mut e, v("identifier", ident));
    let uid = g.sd.as_ref().map(|s| s.uid.clone()).filter(|u| !u.is_empty()).unwrap_or_else(|| w.new_uid());
    push(&mut e, v("uid", &uid));
    if let Some(sd) = &g.sd {
        if !sd.inputs.is_empty() {
            push(&mut e, list("paramInputs", sd.inputs.clone()));
        }
    }
    let mut pv = XElem::new("paramValue");
    push(&mut pv, w.dynamic_value(g));
    push(&mut e, pv);
    e
}

// ------------------------------------------------------------------------------------------------ rendering

/// A native graph as MaterialCraft nodes, ready to evaluate: the package is written as it would be saved and
/// read by the importer. Node ids are the same as in `view`.
pub struct Lowered {
    pub graph: Graph,
    pub report: crate::sbs_import::Report,
}

/// Turn `view` (a graph of a native package, with the package's other graphs as its `subgraphs`) into
/// MaterialCraft nodes.
pub fn lower(view: &Graph) -> Result<Lowered, String> {
    let sd = view.sd.as_ref().ok_or("not an .sbs graph")?;
    let name = view.meta.get("identifier").cloned().unwrap_or_default();
    let graphs = all_graphs(view);
    let deps: Vec<(String, String)> = sd.pkg.doc.root.child("dependencies").map(|d| d.elems().filter_map(|x| Some((x.v("filename")?, x.v("uid")?))).collect()).unwrap_or_default();
    let mut w = Writer { graphs: &graphs, deps, seed: 0x5eed, gray: Default::default() };
    let mut content = XElem::new("content");
    // the graph shown goes first: the importer opens on the first graph with outputs
    let mut order: Vec<(&String, &&Graph)> = graphs.iter().collect();
    order.sort_by_key(|(n, _)| **n != name);
    for (gname, g) in order {
        let el = match g.sd.as_ref().map(|s| &s.role) {
            Some(Role::Graph) => new_graph_elem(&mut w, gname, g),
            Some(Role::Function) => new_function_elem(&mut w, gname, g),
            _ => continue,
        };
        let path = g.sd.as_ref().map(|s| s.path.clone()).unwrap_or_else(|| gname.clone());
        in_folder(&mut content, &path, el);
    }
    let mut root = XElem::new("package");
    push(&mut root, v("identifier", &sd.pkg.doc.root.v("identifier").unwrap_or_default()));
    let mut dl = XElem::new("dependencies");
    for (f, u) in &w.deps {
        let mut d = XElem::new("dependency");
        push(&mut d, v("filename", f));
        push(&mut d, v("uid", u));
        push(&mut d, v("type", "package"));
        push(&mut d, v("fileUID", "0"));
        push(&mut d, v("versionUID", "0"));
        push(&mut dl, d);
    }
    push(&mut root, dl);
    push(&mut root, content);
    let mut pkg = crate::sbs::Package::from_root(&root.to_element())?;
    pkg.resources = sd.pkg.resources.clone();
    pkg.file = sd.pkg.file.clone();
    let dir = sd.pkg.file.as_ref().and_then(|f| f.parent().map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."));
    let stem = sd.pkg.file.as_ref().and_then(|f| f.file_stem().map(|s| s.to_string_lossy().to_string())).unwrap_or_else(|| "package".into());
    let imp = crate::sbs_import::import_package_as(pkg, &dir, &stem, Some(&sd.path))?;
    Ok(Lowered { graph: imp.graph, report: imp.report })
}

/// Put `el` into the `group` folders of `path` (`a/b/name` goes into group `a`, then `b`) inside `content`.
fn in_folder(content: &mut XElem, path: &str, el: XElem) {
    let parts: Vec<&str> = path.split('/').collect();
    let mut cur = content;
    for folder in &parts[..parts.len().saturating_sub(1)] {
        let pos = cur.children.iter().position(|c| matches!(c, XNode::Elem(e) if e.name == "group" && e.v("identifier").as_deref() == Some(*folder)));
        let i = match pos {
            Some(i) => i,
            None => {
                let mut gr = XElem::new("group");
                push(&mut gr, v("identifier", folder));
                push(&mut gr, XElem::new("content"));
                cur.children.push(XNode::Elem(gr));
                cur.empty = false;
                cur.children.len() - 1
            }
        };
        let XNode::Elem(gr) = &mut cur.children[i] else { return };
        if gr.child("content").is_none() {
            push(gr, XElem::new("content"));
        }
        cur = gr.child_mut("content").expect("just added");
    }
    push(cur, el);
}

// ------------------------------------------------------------------------------------------------ new packages

/// An empty package with one empty graph called `name`, laid out as the format's own editor writes packages.
pub fn new_package(name: &str, size: usize) -> Graph {
    let log2 = (size.max(16) as f32).log2().round() as i32;
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0) as u64;
    let mut x = t ^ 0x9e37_79b9_7f4a_7c15;
    let mut hex = |n: usize| -> String {
        (0..n)
            .map(|_| {
                x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                std::char::from_digit(((x >> 60) & 15) as u32, 16).unwrap_or('0')
            })
            .collect()
    };
    let file_uid = format!("{{{}-{}-4{}-a{}-{}}}", hex(8), hex(4), hex(3), hex(3), hex(12));
    let uid = 1_000_000_000 + (t % 1_000_000_000);
    let text = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><package><identifier v=\"Unsaved Package\"/><formatVersion v=\"1.1.0.202401\"/><updaterVersion v=\"1.1.0.202401\"/><fileUID v=\"{file_uid}\"/><versionUID v=\"0\"/><dependencies/><content><graph><identifier v=\"{}\"/><uid v=\"{uid}\"/><graphtype v=\"material\"/><graphOutputs/><compNodes/><baseParameters><parameter><name v=\"outputsize\"/><relativeTo v=\"0\"/><paramValue><constantValueInt2 v=\"{log2} {log2}\"/></paramValue></parameter></baseParameters><options><option><name v=\"defaultParentSize\"/><value v=\"{log2}x{log2}\"/></option></options><root/></graph></content></package>",
        crate::xmldoc::escape_attr(name)
    );
    match load(&text, None) {
        Ok((g, _)) => g,
        Err(_) => Graph::default(),
    }
}

// ------------------------------------------------------------------------------------------------ editing

/// One parameter a native node can take, for the editor and the command layer.
#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub id: String,
    pub label: String,
    pub ty: PTy,
    /// the value used while the node does not set it
    pub def: String,
    pub range: Option<(f32, f32)>,
    pub choices: Vec<String>,
    /// `Base` for the settings every filter has, else the group the graph declares (or empty)
    pub group: String,
    /// what the node has: its own value, or `None` (the default applies)
    pub value: Option<PVal>,
    pub rel: bool,
}

impl ParamInfo {
    /// The value in effect (the node's own, else the default).
    pub fn current(&self) -> PVal {
        self.value.clone().unwrap_or_else(|| PVal::Const { ty: self.ty, raw: self.def.clone(), rel: None })
    }
}

/// The definition of a native node's kind.
pub fn node_def(n: &Node) -> Option<&'static sbs_defs::NodeDef> {
    sbs_defs::def(&n.kind)
}

/// Type, default, range and choices of a graph input (`paraminput`) of a value type.
fn input_info(p: &XElem) -> Option<(String, PTy, String, Option<(f32, f32)>, Vec<String>, String, String)> {
    let id = p.v("identifier")?;
    let dv = p.child("defaultValue").and_then(const_in);
    let code: u32 = p.v("type").and_then(|t| t.trim().parse().ok()).unwrap_or(0);
    let ty = match &dv {
        Some(PVal::Const { ty, .. }) => *ty,
        _ => PTy::from_type_code(code)?,
    };
    if code != 0 && code < 4 && dv.is_none() {
        return None;
    }
    let def = match &dv {
        Some(PVal::Const { raw, .. }) => raw.clone(),
        _ => String::new(),
    };
    let w = p.child("defaultWidget").or_else(|| p.child("widget"));
    let opt = |name: &str| w.and_then(|w| w.child("options")).and_then(|o| o.elems().find(|x| x.v("name").as_deref() == Some(name))).and_then(|x| x.v("value"));
    let num = |name: &str| opt(name).and_then(|s| s.split_whitespace().next().and_then(|x| x.parse::<f32>().ok())).filter(|x| x.is_finite());
    let range = match (num("min"), num("max")) {
        (Some(a), Some(b)) if b > a => Some((a, b)),
        _ => None,
    };
    let mut choices = Vec::new();
    if let Some(list) = opt("parameters") {
        let parts: Vec<&str> = list.split([';', '\n']).filter(|s| !s.is_empty()).collect();
        let mut pairs: Vec<(i32, String)> = parts.chunks(2).filter_map(|c| if let [v, l] = c { Some((v.trim().parse::<i32>().ok()?, l.trim().to_string())) } else { None }).collect();
        pairs.sort();
        if pairs.iter().enumerate().all(|(i, (v, _))| *v == i as i32) {
            choices = pairs.into_iter().map(|(_, l)| l).collect();
        }
    }
    let label = p.child("attributes").and_then(|a| a.v("label")).filter(|s| !s.is_empty()).unwrap_or_else(|| sbs_defs::label_of(&id));
    let group = p.v("group").unwrap_or_default();
    Some((id, ty, def, range, choices, label, group))
}

/// Every parameter `n` can take: those of its type (or, for a graph node, the value inputs of its graph), then
/// the settings every filter has.
pub fn params_of(n: &Node, lib: &BTreeMap<String, Graph>) -> Vec<ParamInfo> {
    let vals = values(n);
    let mut out = Vec::new();
    let push_def = |p: &sbs_defs::PDef, group: &str, out: &mut Vec<ParamInfo>| {
        out.push(ParamInfo {
            id: p.id.to_string(),
            label: sbs_defs::label_of(p.id),
            ty: p.ty,
            def: p.def.clone(),
            range: p.range,
            choices: p.choices.iter().map(|c| c.to_string()).collect(),
            group: group.to_string(),
            value: vals.get(p.id).cloned(),
            rel: p.rel,
        });
    };
    if n.kind == "sbs:graph" {
        if let Some(sd) = n.strs.get("#graph").and_then(|g| lib.get(g)).and_then(|g| g.sd.as_ref()) {
            for p in &sd.inputs {
                if let Some((id, ty, def, range, choices, label, group)) = input_info(p) {
                    let value = vals.get(&id).cloned();
                    out.push(ParamInfo { id, label, ty, def, range, choices, group, value, rel: false });
                }
            }
        }
    } else if let Some(d) = node_def(n) {
        for p in &d.params {
            let group = d.library.map(|(pk, g)| sbs_defs::lib_group(pk, g, p.id)).unwrap_or("");
            push_def(p, group, &mut out);
        }
    }
    let texture = sbs_defs::family(&n.kind) == Some(Fam::Texture) && !matches!(n.kind.as_str(), "sbs:output" | "sbs:passthrough");
    if texture {
        for p in sbs_defs::base() {
            if !out.iter().any(|x| x.id == p.id) {
                push_def(p, "Base", &mut out);
            }
        }
    }
    // values the node has that its type does not list (kept, shown last)
    for (k, v) in &vals {
        if !out.iter().any(|x| &x.id == k) {
            let ty = match v {
                PVal::Const { ty, .. } => *ty,
                PVal::Dyn { .. } => PTy::F(1),
            };
            out.push(ParamInfo { id: k.clone(), label: sbs_defs::label_of(k), ty, def: String::new(), range: None, choices: Vec::new(), group: "Other".into(), value: Some(v.clone()), rel: false });
        }
    }
    out
}

/// Set value `name` of a node (`None`: back to its default, the value is no longer written).
pub fn set_value(n: &mut Node, name: &str, v: Option<PVal>) {
    match v {
        Some(v) => {
            n.strs.insert(name.to_string(), v.enc());
        }
        None => {
            n.strs.remove(name);
        }
    }
}

/// The colour keys of a Gradient Map node: (position, colour), in the order the file keeps them. A node without keys of its own has
/// the format's default black-to-white ramp.
pub fn gradient_keys(n: &Node) -> Vec<(f32, [f32; 4])> {
    let mut keys: Vec<(f32, [f32; 4])> = n
        .strs
        .get("#arrays")
        .and_then(|x| XDoc::parse(x).ok())
        .map(|d| {
            d.root
                .elems()
                .filter(|a| a.v("name").map(|x| x.to_ascii_lowercase().starts_with("gradient")).unwrap_or(false))
                .flat_map(|a| a.child("paramsArrayCells").map(|c| c.elems().cloned().collect::<Vec<_>>()).unwrap_or_default())
                .filter_map(|cell| {
                    let ps = cell.child("parameters")?;
                    let get = |name: &str| ps.elems().find(|p| p.v("name").as_deref() == Some(name)).and_then(|p| p.child("paramValue")).and_then(const_in).map(|v| v.nums());
                    let pos = get("position")?.first().copied()?;
                    let v = get("value")?;
                    let c = match v.len() {
                        0 => return None,
                        1 | 2 => [v[0], v[0], v[0], 1.0],
                        3 => [v[0], v[1], v[2], 1.0],
                        _ => [v[0], v[1], v[2], v[3]],
                    };
                    Some((pos, c))
                })
                .collect()
        })
        .unwrap_or_default();
    if keys.is_empty() {
        keys = vec![(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 1.0])];
    }
    keys
}

/// Set the colour keys of a Gradient Map node (written as the format's `gradientrgba` array).
pub fn set_gradient_keys(n: &mut Node, keys: &[(f32, [f32; 4])]) {
    let keys: Vec<(f32, [f32; 4])> = keys.iter().map(|(p, c)| (if p.is_finite() { p.clamp(0.0, 1.0) } else { 0.0 }, c.map(|x| if x.is_finite() { x } else { 0.0 }))).collect();
    // the array keeps the uid it had
    let uid = n.strs.get("#arrays").and_then(|x| XDoc::parse(x).ok()).and_then(|d| d.root.elems().find_map(|a| a.v("uid"))).unwrap_or_else(|| "1".into());
    let param = |name: &str, el: XElem| {
        let mut p = XElem::new("parameter");
        push(&mut p, v("name", name));
        let mut pv = XElem::new("paramValue");
        push(&mut pv, el);
        push(&mut p, pv);
        p
    };
    let mut cells = XElem::new("paramsArrayCells");
    for (i, (pos, c)) in keys.iter().enumerate() {
        let mut cell = XElem::new("paramsArrayCell");
        push(&mut cell, v("uid", &(i + 1).to_string()));
        let mut ps = XElem::new("parameters");
        push(&mut ps, param("position", const_elem(PTy::F(1), &fmt_num(*pos))));
        push(&mut ps, param("value", const_elem(PTy::F(4), &c.iter().map(|x| fmt_num(*x)).collect::<Vec<_>>().join(" "))));
        push(&mut cell, ps);
        push(&mut cells, cell);
    }
    let mut a = XElem::new("paramsArray");
    push(&mut a, v("name", "gradientrgba"));
    push(&mut a, v("uid", &uid));
    push(&mut a, cells);
    let mut arrays = XElem::new("paramsArrays");
    push(&mut arrays, a);
    n.strs.insert("#arrays".into(), arrays.to_xml());
}

/// A constant for parameter `p` from numbers, with the inheritance the format writes on node values.
pub fn value_from_nums(p: &ParamInfo, v: &[f32]) -> PVal {
    let ty = match p.ty {
        PTy::FV => {
            if v.len() == 1 {
                PTy::F(1)
            } else {
                PTy::F(4)
            }
        }
        t => t,
    };
    let rel = match &p.value {
        Some(PVal::Const { rel, .. }) | Some(PVal::Dyn { rel, .. }) => rel.clone(),
        None => Some("0".into()),
    };
    PVal::of_nums(ty, v, rel)
}

/// The input sockets of a native node.
pub fn slots(n: &Node, lib: &BTreeMap<String, Graph>) -> Vec<String> {
    match n.kind.as_str() {
        "sbs:output" => vec!["inputNodeOutput".into()],
        "sbs:input" => Vec::new(),
        "sbs:graph" => {
            let Some(sd) = n.strs.get("#graph").and_then(|g| lib.get(g)).and_then(|g| g.sd.as_ref()) else {
                return n.ins.keys().cloned().collect();
            };
            let mut v: Vec<String> = sd.inputs.iter().filter(|p| input_info(p).is_none()).filter_map(|p| p.v("identifier")).collect();
            for k in n.ins.keys() {
                if !v.contains(k) {
                    v.push(k.clone());
                }
            }
            v
        }
        _ => {
            let Some(d) = node_def(n) else { return n.ins.keys().cloned().collect() };
            let mut v = Vec::new();
            for i in &d.inputs {
                if i.kind.len() > 1 || matches!(i.kind, "t" | "b" | "s") {
                    // a function node: its inputs are values
                }
                v.push(i.id.to_string());
                if i.many {
                    // the linked extra inputs, and one free one
                    let mut k = 1;
                    while n.ins.contains_key(&format!("{}:{k}", i.id)) {
                        v.push(format!("{}:{k}", i.id));
                        k += 1;
                    }
                    if n.ins.contains_key(i.id) && k < 16 {
                        v.push(format!("{}:{k}", i.id));
                    }
                }
            }
            for k in n.ins.keys() {
                if !v.contains(k) {
                    v.push(k.clone());
                }
            }
            v
        }
    }
}

/// The output sockets of a native node (one name for most nodes).
pub fn outputs(n: &Node, lib: &BTreeMap<String, Graph>) -> Vec<String> {
    if n.strs.contains_key("#bridgings") {
        return node_outputs(n).into_iter().map(|(_, i)| i).collect();
    }
    if n.kind == "sbs:graph" {
        if let Some(sd) = n.strs.get("#graph").and_then(|g| lib.get(g)).and_then(|g| g.sd.as_ref()) {
            return sd.outputs.iter().filter_map(|o| o.v("identifier")).collect();
        }
    }
    match n.kind.as_str() {
        "sbs:output" => Vec::new(),
        _ => node_def(n).map(|d| d.outputs.iter().map(|s| s.to_string()).collect()).unwrap_or_else(|| vec!["output".into()]),
    }
}

/// What the editor calls a native node.
pub fn label(n: &Node, g: &Graph) -> String {
    match n.kind.as_str() {
        "sbs:output" => {
            let uid = n.strs.get("#output").cloned().unwrap_or_default();
            g.sd.as_ref().and_then(|s| s.output_identifier(&uid)).unwrap_or_else(|| "Output".into())
        }
        "sbs:input" => {
            let uid = n.strs.get("#entry").cloned().unwrap_or_default();
            g.sd.as_ref().and_then(|s| s.input_identifier(&uid)).unwrap_or_else(|| "Input".into())
        }
        "sbs:graph" => n.strs.get("#graph").map(|p| p.rsplit('/').next().unwrap_or(p).to_string()).unwrap_or_else(|| "Graph".into()),
        k => match node_def(n) {
            Some(d) => d.label.to_string(),
            None => k.rsplit([':', '/']).next().map(sbs_defs::label_of).unwrap_or_default(),
        },
    }
}

/// A new node of a native kind. `graph` is a graph of the package for `sbs:graph` (an instance of it).
pub fn new_node(kind: &str, graph: Option<&str>, pos: [f32; 2]) -> Node {
    let mut n = Node { kind: kind.to_string(), pos, ..Default::default() };
    if kind == "sbs:graph" {
        n.strs.insert("#graph".into(), graph.unwrap_or_default().to_string());
    }
    n
}

/// A free node id of `g` that also works as a uid (never 0, below the top of the range).
pub fn free_id(g: &Graph) -> u32 {
    let max = g.nodes.keys().max().copied().unwrap_or(0);
    if max < u32::MAX - 2 {
        return max + 1;
    }
    (1..u32::MAX).find(|i| !g.nodes.contains_key(i)).unwrap_or(1)
}

/// A uid not used by any graph input or output of `g` (they share a context with the graph's nodes).
fn free_uid(g: &Graph, seed: u64) -> String {
    let mut used: BTreeSet<String> = g.nodes.iter().map(|(id, n)| uid_of(*id, n)).collect();
    if let Some(sd) = &g.sd {
        used.extend(sd.inputs.iter().chain(sd.outputs.iter()).filter_map(|e| e.v("uid")));
    }
    let mut x = seed ^ 0x2545_f491_4f6c_dd1d;
    loop {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let u = format!("{}", 1_000_000_000 + (x >> 33) % 3_000_000_000);
        if !used.contains(&u) {
            return u;
        }
    }
}

/// Usage names the format uses for graph outputs, with what MaterialCraft calls them.
pub const USAGES: &[(&str, &str)] = &[
    ("baseColor", "basecolor"),
    ("normal", "normal"),
    ("roughness", "roughness"),
    ("metallic", "metallic"),
    ("height", "height"),
    ("ambientOcclusion", "ambient_occlusion"),
    ("opacity", "opacity"),
    ("emissive", "emissive"),
    ("specular", "specular"),
    ("glossiness", "glossiness"),
    ("diffuse", "diffuse"),
    ("anisotropyLevel", "anisotropy_level"),
    ("anisotropyAngle", "anisotropy_angle"),
];

/// The format's usage name for a MaterialCraft usage (`basecolor` -> `baseColor`); unknown ones are kept.
pub fn sd_usage(mc: &str) -> String {
    USAGES.iter().find(|(s, m)| *m == mc || s.eq_ignore_ascii_case(mc)).map(|(s, _)| s.to_string()).unwrap_or_else(|| mc.to_string())
}

/// Add a graph output with `usage` (format or MaterialCraft name) and its output node. Returns the node id.
pub fn add_output(g: &mut Graph, usage: &str, ident: Option<&str>, pos: [f32; 2]) -> u32 {
    add_output_ex(g, usage, ident, None, pos)
}

/// [`add_output`] that also says whether the output is grayscale (`None`: decided by the usage).
pub fn add_output_ex(g: &mut Graph, usage: &str, ident: Option<&str>, gray: Option<bool>, pos: [f32; 2]) -> u32 {
    let usage = sd_usage(usage);
    let base = ident.map(str::to_string).unwrap_or_else(|| usage.to_ascii_lowercase());
    let taken: BTreeSet<String> = g.sd.as_ref().map(|s| s.outputs.iter().filter_map(|o| o.v("identifier")).collect()).unwrap_or_default();
    let mut identifier = base.clone();
    let mut k = 1;
    while taken.contains(&identifier) {
        identifier = format!("{base}_{k}");
        k += 1;
    }
    let uid = free_uid(g, g.nodes.len() as u64 + 7);
    let gray = gray.unwrap_or_else(|| !matches!(usage.as_str(), "baseColor" | "normal" | "emissive" | "specular" | "diffuse"));
    let mut o = XElem::new("graphoutput");
    push(&mut o, v("identifier", &identifier));
    push(&mut o, v("uid", &uid));
    let mut a = XElem::new("attributes");
    push(&mut a, v("label", &sbs_defs::label_of(&identifier)));
    push(&mut o, a);
    let mut u = XElem::new("usage");
    push(&mut u, v("components", "RGBA"));
    push(&mut u, v("name", &usage));
    push(&mut o, list("usages", vec![u]));
    if gray {
        push(&mut o, v("channels", "2"));
    }
    push(&mut o, v("group", "Material"));
    if let Some(sd) = g.sd.as_mut() {
        sd.outputs.push(o);
    }
    let id = free_id(g);
    let mut n = Node { kind: "sbs:output".into(), pos, ..Default::default() };
    n.strs.insert("#output".into(), uid);
    g.nodes.insert(id, n);
    id
}

/// Change the usage of the graph output an output node writes.
pub fn set_output_usage(g: &mut Graph, node: u32, usage: &str) -> Result<(), String> {
    let uid = g.nodes.get(&node).and_then(|n| n.strs.get("#output")).cloned().ok_or("not an output node")?;
    let usage = sd_usage(usage);
    let sd = g.sd.as_mut().ok_or("not an .sbs graph")?;
    let o = sd.outputs.iter_mut().find(|o| o.v("uid").as_deref() == Some(uid.as_str())).ok_or("the graph does not declare this output")?;
    let mut u = XElem::new("usage");
    push(&mut u, v("components", "RGBA"));
    push(&mut u, v("name", &usage));
    let ul = list("usages", vec![u]);
    match o.children.iter().position(|c| matches!(c, XNode::Elem(e) if e.name == "usages")) {
        Some(i) => o.children[i] = XNode::Elem(ul),
        None => push(o, ul),
    }
    Ok(())
}

/// Rename the graph output an output node writes (its identifier, and the label that follows it). Returns the old and
/// the new identifier; the nodes of other graphs that read it by name follow with [`retarget_output`].
pub fn rename_output(g: &mut Graph, node: u32, new: &str) -> Result<(String, String), String> {
    let new = new.trim();
    if new.is_empty() || !new.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || new.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(format!("'{new}' is not a valid output name (letters, digits and _, not starting with a digit)"));
    }
    let uid = g.nodes.get(&node).and_then(|n| n.strs.get("#output")).cloned().ok_or("not an output node")?;
    let sd = g.sd.as_mut().ok_or("not an .sbs graph")?;
    if sd.outputs.iter().any(|o| o.v("uid").as_deref() != Some(uid.as_str()) && o.v("identifier").as_deref() == Some(new)) {
        return Err(format!("the graph already has an output called '{new}'"));
    }
    let o = sd.outputs.iter_mut().find(|o| o.v("uid").as_deref() == Some(uid.as_str())).ok_or("the graph does not declare this output")?;
    let old = o.v("identifier").unwrap_or_default();
    if old == new {
        return Ok((old, new.to_string()));
    }
    match o.child_mut("identifier") {
        Some(e) => e.set_attr("v", new),
        None => push(o, v("identifier", new)),
    }
    if let Some(l) = o.child_mut("attributes").and_then(|a| a.child_mut("label")) {
        l.set_attr("v", &sbs_defs::label_of(new));
    }
    Ok((old, new.to_string()))
}

/// The nodes of `g` that run graph `graph` and read its output `old` by name read `new` instead.
pub fn retarget_output(g: &mut Graph, graph: &str, old: &str, new: &str) {
    let users: BTreeSet<u32> = g.nodes.iter().filter(|(_, n)| n.kind == "sbs:graph" && n.strs.get("#graph").map(String::as_str) == Some(graph)).map(|(id, _)| *id).collect();
    if users.is_empty() {
        return;
    }
    for (id, n) in g.nodes.iter_mut() {
        if users.contains(id) {
            if let Some(mut b) = n.strs.get("#bridgings").and_then(|x| XDoc::parse(x).ok()).map(|d| d.root) {
                for e in b.elems_mut() {
                    if e.v("identifier").as_deref() == Some(old) {
                        if let Some(i) = e.child_mut("identifier") {
                            i.set_attr("v", new);
                        }
                    }
                }
                n.strs.insert("#bridgings".into(), b.to_xml());
            }
        }
        let slots: Vec<String> = n.in_out.iter().filter(|(slot, o)| o.as_str() == old && n.ins.get(*slot).map(|s| users.contains(s)).unwrap_or(false)).map(|(k, _)| k.clone()).collect();
        for slot in slots {
            n.in_out.insert(slot, new.to_string());
        }
    }
}

/// Image inputs a node only reads while a switch of its own is on: `(input, switch)`. The editor shows the socket
/// of such an input only while its switch is on (or a link still reaches it).
pub fn input_switches(n: &Node) -> &'static [(&'static str, &'static str)] {
    match n.kind.strip_prefix("sbs:lib:").map(|k| k.rsplit('/').next().unwrap_or(k)) {
        Some("pbr_base_material") => &[
            ("basecolor_input", "user_basecolor"),
            ("diffuse_input", "user_diffuse"),
            ("normal_input", "user_normal"),
            ("specular_input", "user_specular"),
            ("glossiness_input", "user_glossiness"),
            ("roughness_input", "user_roughness"),
            ("metallic_input", "user_metallic"),
            ("height_input", "user_height"),
            ("ambientOcclusion_input", "user_ao"),
            ("grunge_input", "grunge_custom_input"),
        ],
        _ => &[],
    }
}

/// Is image input `slot` of `n` read (always, unless a switch of [`input_switches`] turns it off)?
pub fn input_used(n: &Node, slot: &str) -> bool {
    let Some((_, sw)) = input_switches(n).iter().find(|(i, _)| *i == slot) else { return true };
    match values(n).get(*sw) {
        Some(v @ PVal::Const { .. }) => v.nums().first().map(|x| *x != 0.0).unwrap_or(false),
        // computed by a function: it may be on
        Some(_) => true,
        None => false,
    }
}

/// Add an image input (grey or colour) to a graph and the node that reads it. Returns the node id.
pub fn add_input(g: &mut Graph, gray: bool, ident: Option<&str>, pos: [f32; 2]) -> u32 {
    let base = ident.unwrap_or("input").to_string();
    let taken: BTreeSet<String> = g.sd.as_ref().map(|s| s.inputs.iter().filter_map(|o| o.v("identifier")).collect()).unwrap_or_default();
    let mut identifier = base.clone();
    let mut k = 1;
    while taken.contains(&identifier) {
        identifier = format!("{base}_{k}");
        k += 1;
    }
    let uid = free_uid(g, g.nodes.len() as u64 + 11);
    let mut p = XElem::new("paraminput");
    push(&mut p, v("identifier", &identifier));
    push(&mut p, v("uid", &uid));
    let mut a = XElem::new("attributes");
    push(&mut a, v("label", &sbs_defs::label_of(&identifier)));
    push(&mut p, a);
    push(&mut p, v("type", if gray { "2" } else { "1" }));
    if let Some(sd) = g.sd.as_mut() {
        sd.inputs.push(p);
    }
    let id = free_id(g);
    let mut n = Node { kind: "sbs:input".into(), pos, ..Default::default() };
    n.strs.insert("#entry".into(), uid);
    g.nodes.insert(id, n);
    id
}

/// Remove nodes; the graph inputs and outputs only they used go with them.
pub fn remove_nodes(g: &mut Graph, ids: &BTreeSet<u32>) {
    let mut outs = Vec::new();
    let mut ins = Vec::new();
    for id in ids {
        if let Some(n) = g.nodes.remove(id) {
            if let Some(u) = n.strs.get("#output") {
                outs.push(u.clone());
            }
            if let Some(u) = n.strs.get("#entry") {
                ins.push(u.clone());
            }
        }
    }
    for n in g.nodes.values_mut() {
        n.ins.retain(|_, s| !ids.contains(s));
    }
    let still_out: BTreeSet<String> = g.nodes.values().filter_map(|n| n.strs.get("#output").cloned()).collect();
    let still_in: BTreeSet<String> = g.nodes.values().filter_map(|n| n.strs.get("#entry").cloned()).collect();
    if let Some(sd) = g.sd.as_mut() {
        sd.outputs.retain(|o| !o.v("uid").map(|u| outs.contains(&u) && !still_out.contains(&u)).unwrap_or(false));
        sd.inputs.retain(|o| !o.v("uid").map(|u| ins.contains(&u) && !still_in.contains(&u)).unwrap_or(false));
    }
}

/// A copy of node `n` for a new place: it gets its own uid and its own copies of the function graphs its values
/// use (stored in `lib` under names for `owner` / `new_id`).
pub fn copy_node(n: &Node, owner: &str, new_id: u32, lib: &mut BTreeMap<String, Graph>) -> Node {
    let mut c = n.clone();
    c.strs.remove("#uid");
    let keys: Vec<String> = c.strs.keys().filter(|k| !k.starts_with('#') && k.as_str() != "title").cloned().collect();
    for k in keys {
        if let Some(PVal::Dyn { graph, rel }) = c.strs.get(&k).and_then(|v| PVal::dec(v)) {
            let name = format!("{owner}/{new_id}/{k}");
            if copy_inline(lib, &graph, &name, Role::Param { owner: owner.to_string(), node: new_id, param: k.clone() }) {
                c.strs.insert(k, PVal::Dyn { graph: name, rel }.enc());
            }
        }
    }
    if let Some(names) = c.strs.get("#fx").cloned() {
        let mut new_names = Vec::new();
        for (i, nm) in names.split(',').filter(|s| !s.is_empty()).enumerate() {
            let name = format!("{owner}/{new_id}/fx{i}");
            if copy_inline(lib, nm, &name, Role::Fx { owner: owner.to_string(), node: new_id, index: i }) {
                new_names.push(name);
            }
        }
        c.strs.insert("#fx".into(), new_names.join(","));
    }
    c
}

/// Copy the function or FX-map graph `from` as `to` (with `role`), with copies of the functions inside it.
fn copy_inline(lib: &mut BTreeMap<String, Graph>, from: &str, to: &str, role: Role) -> bool {
    // functions nest only so deep; a value that leads back into itself is not followed
    if to.matches('/').count() > 96 {
        return false;
    }
    let Some(mut fg) = lib.get(from).cloned() else { return false };
    fg.meta.insert("identifier".into(), to.to_string());
    if let Some(sd) = fg.sd.as_mut() {
        if matches!(role, Role::Fx { .. }) {
            sd.uid = String::new();
        }
        sd.role = role;
        sd.path = to.to_string();
        sd.orig = None;
    }
    // the nodes inside keep their ids and uids: those only need to be unique inside their graph
    let ids: Vec<u32> = fg.nodes.keys().copied().collect();
    for id in ids {
        let n = fg.nodes[&id].clone();
        let mut c = copy_node(&n, to, id, lib);
        if let Some(u) = n.strs.get("#uid") {
            c.strs.insert("#uid".into(), u.clone());
        }
        fg.nodes.insert(id, c);
    }
    lib.insert(to.to_string(), fg);
    true
}

/// Drive parameter `param` of node `node` of graph `owner` by a new function graph holding one constant of the
/// parameter's current value (the format's way to start a function). Returns the function graph's name.
pub fn make_function(lib: &mut BTreeMap<String, Graph>, pkg: Arc<SdPackage>, owner: &str, node: u32, param: &str, current: &PVal) -> String {
    let name = format!("{owner}/{node}/{param}");
    let mut g = Graph::default();
    g.meta.insert("kind".into(), "function".into());
    g.meta.insert("identifier".into(), name.clone());
    let (ty, raw) = match current {
        PVal::Const { ty, raw, .. } => (*ty, raw.clone()),
        PVal::Dyn { .. } => (PTy::F(1), "0".into()),
    };
    let (ty, raw) = if ty == PTy::FV { (PTy::F(4), raw) } else { (ty, raw) };
    let f = match ty {
        PTy::F(n) => format!("const_float{n}"),
        PTy::I(n) => format!("const_int{n}"),
        PTy::B => "const_bool".into(),
        _ => "const_string".into(),
    };
    let mut n = Node { kind: sbs_defs::kind_of(Fam::Function, &f), ..Default::default() };
    n.strs.insert(f.clone(), PVal::Const { ty, raw, rel: None }.enc());
    g.nodes.insert(1, n);
    let mut sd = SdGraph::new(Role::Param { owner: owner.to_string(), node, param: param.to_string() }, &name, pkg);
    sd.root = Some(1);
    g.sd = Some(Box::new(sd));
    lib.insert(name.clone(), g);
    name
}

/// Add a compositing graph or a function to the package. `kind`: graph (empty), material (the usual material
/// outputs) or function.
pub fn create_graph(root: &mut Graph, name: &str, kind: &str) -> Result<(), String> {
    if name.is_empty() || !name.split('/').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
        return Err(format!("'{name}' is not a valid graph name (letters, digits and _, folders with /)"));
    }
    if all_graphs(root).contains_key(name) {
        return Err(format!("the package already has a graph called '{name}'"));
    }
    let pkg = root.sd.as_ref().map(|s| s.pkg.clone()).ok_or("not an .sbs package")?;
    let mut g = Graph { size: root.size, height: root.height, ..Default::default() };
    g.meta.insert("identifier".into(), name.to_string());
    let role = if kind == "function" { Role::Function } else { Role::Graph };
    if kind == "function" {
        g.meta.insert("kind".into(), "function".into());
    }
    let mut sd = SdGraph::new(role, name, pkg);
    sd.uid = free_uid(root, root.subgraphs.len() as u64 + 3);
    g.sd = Some(Box::new(sd));
    if kind == "material" {
        material_outputs(&mut g);
    }
    root.subgraphs.insert(name.to_string(), g);
    Ok(())
}

/// The outputs a new material starts with, each fed by a Uniform Color at the usual value.
pub fn material_outputs(g: &mut Graph) {
    let rows: [(&str, &str, Option<&str>, bool); 6] = [
        ("baseColor", "basecolor", Some("0.5 0.5 0.5 1"), true),
        ("normal", "normal", None, true),
        ("roughness", "roughness", Some("0.25 0.25 0.25 1"), false),
        ("metallic", "metallic", None, false),
        ("height", "height", Some("0.5 0.5 0.5 1"), false),
        ("ambientOcclusion", "ambientocclusion", Some("1 1 1 1"), false),
    ];
    for (i, (usage, ident, color, colour_mode)) in rows.iter().enumerate() {
        let y = (i as f32 * 160.0 - 240.0) * POS_SCALE;
        let out = add_output(g, usage, Some(ident), [-48.0 * POS_SCALE, y]);
        let src = free_id(g);
        let mut n = if *usage == "normal" {
            let mut n = Node { kind: "sbs:normal".into(), ..Default::default() };
            n.strs.insert("input2alpha".into(), PVal::Const { ty: PTy::B, raw: "0".into(), rel: Some("0".into()) }.enc());
            n
        } else {
            let mut n = Node { kind: "sbs:uniform".into(), ..Default::default() };
            if !colour_mode {
                n.strs.insert("colorswitch".into(), PVal::Const { ty: PTy::B, raw: "0".into(), rel: Some("0".into()) }.enc());
            }
            if let Some(c) = color {
                n.strs.insert("outputcolor".into(), PVal::Const { ty: PTy::F(4), raw: c.to_string(), rel: Some("0".into()) }.enc());
            }
            n
        };
        n.pos = [-208.0 * POS_SCALE, y];
        g.nodes.insert(src, n);
        if let Some(o) = g.nodes.get_mut(&out) {
            o.ins.insert("inputNodeOutput".into(), src);
        }
    }
}

/// Give FX-Map node `node` of graph `g` its pattern graph when it has none: one Quadrant node, the graph's result
/// (what a new FX-Map holds). Returns the pattern graph's name.
pub fn ensure_fx_graph(g: &mut Graph, node: u32) -> Option<String> {
    let n = g.nodes.get(&node)?;
    if n.kind != "sbs:fxmaps" {
        return None;
    }
    if let Some(first) = n.strs.get("#fx").and_then(|x| x.split(',').find(|s| !s.is_empty()).map(str::to_string)) {
        if g.subgraphs.contains_key(&first) {
            return Some(first);
        }
    }
    let owner = graph_name(g);
    let pkg = g.sd.as_ref()?.pkg.clone();
    let name = format!("{owner}/{node}/fx0");
    let mut fg = Graph::default();
    fg.meta.insert("kind".into(), "fxmap".into());
    fg.meta.insert("identifier".into(), name.clone());
    let mut sd = SdGraph::new(Role::Fx { owner, node, index: 0 }, &name, pkg);
    sd.uid = free_uid(g, node as u64 * 7 + 3);
    sd.root = Some(1);
    fg.nodes.insert(1, Node { kind: sbs_defs::kind_of(Fam::Pattern, "paramset"), ..Default::default() });
    fg.sd = Some(Box::new(sd));
    g.subgraphs.insert(name.clone(), fg);
    if let Some(n) = g.nodes.get_mut(&node) {
        n.strs.insert("#fx".into(), name.clone());
    }
    Some(name)
}

/// A picture of what a node type makes, for the editor's library: the node alone, its image inputs fed from a
/// noise and a gradient, drawn at `size`.
pub fn kind_preview(kind: &str, size: usize) -> Option<crate::tex::Tex> {
    let d = sbs_defs::def(kind)?;
    if d.fam != Fam::Texture || matches!(kind, "sbs:output" | "sbs:graph") {
        return None;
    }
    let mut g = new_package("preview", size);
    g.size = size;
    let id = free_id(&g);
    g.nodes.insert(id, new_node(kind, None, [0.0, 0.0]));
    let slots = slots(&g.nodes[&id], &g.subgraphs);
    if !slots.is_empty() {
        let sources = ["sbs:lib:noise_perlin_noise/perlin_noise", "sbs:lib:gradient_linear_1/gradient_linear_1"];
        let mut src_ids = Vec::new();
        for (i, k) in sources.iter().enumerate() {
            let s = free_id(&g);
            g.nodes.insert(s, new_node(k, None, [-200.0, i as f32 * 150.0]));
            src_ids.push(s);
        }
        if let Some(n) = g.nodes.get_mut(&id) {
            for (i, slot) in slots.iter().take(2).enumerate() {
                n.ins.insert(slot.clone(), src_ids[i]);
            }
        }
    }
    g.evaluate_node(id, size).ok()
}

/// The packages a package uses (library packages and other `.sbs` files) with how many nodes use each.
pub fn package_dependencies(root: &Graph) -> Vec<(String, usize)> {
    let mut uses: BTreeMap<String, usize> = BTreeMap::new();
    for g in all_graphs(root).values() {
        for n in g.nodes.values() {
            let file = match sbs_defs::def(&n.kind).and_then(|d| d.library) {
                Some((pkg, _)) => format!("{pkg}.sbs (library)"),
                None => match n.kind.strip_prefix("sbs:lib:").and_then(|r| r.split_once('/')) {
                    Some((pkg, _)) => format!("{pkg}.sbs (library)"),
                    None => continue,
                },
            };
            *uses.entry(file).or_insert(0) += 1;
        }
    }
    // other packages the file names
    if let Some(sd) = root.sd.as_ref() {
        if let Some(d) = sd.pkg.doc.root.child("dependencies") {
            for f in d.elems().filter_map(|x| x.v("filename")) {
                if f != "?himself" && !f.starts_with("sbs://") {
                    uses.entry(f).or_insert(0);
                }
            }
        }
    }
    uses.into_iter().collect()
}

/// Store `bytes` as an embedded resource of the package under `identifier` (replacing one of that name): the
/// document gets a `resource` element, so saving writes it, and the nodes that read it see the new content. Returns the
/// path a node's resource parameter takes (`pkg:///identifier?dependency=uid`).
pub fn set_resource(root: &mut Graph, identifier: &str, kind: &str, format: &str, bytes: Vec<u8>) -> Result<String, String> {
    if identifier.is_empty() || !identifier.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("'{identifier}' is not a valid resource name (letters, digits and _)"));
    }
    let sd = root.sd.as_ref().ok_or("not an .sbs package")?;
    let mut pkg = (*sd.pkg).clone();
    let existing = pkg.resources.iter().position(|r| r.identifier == identifier && r.folder.is_empty());
    let uid = match existing {
        Some(i) => pkg.resources[i].uid.clone(),
        None => {
            let mut x = identifier.bytes().fold(0x9e37_79b9u64, |a, b| a.wrapping_mul(131).wrapping_add(b as u64));
            loop {
                x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let u = format!("{}", 1_000_000_000 + (x >> 33) % 3_000_000_000);
                if !pkg.resources.iter().any(|r| r.uid == u) {
                    break u;
                }
            }
        }
    };
    let res = crate::sbs::Resource { identifier: identifier.to_string(), uid: uid.clone(), folder: String::new(), kind: kind.to_string(), format: Some(format.to_string()), filepath: None, embedded: Some(bytes.clone()) };
    match existing {
        Some(i) => pkg.resources[i] = res,
        None => pkg.resources.push(res),
    }
    // the document: one `resource` element in the package's content
    let ind = pkg.doc.indent();
    let mut el = XElem::new("resource");
    push(&mut el, v("identifier", identifier));
    push(&mut el, v("uid", &uid));
    push(&mut el, v("type", kind));
    push(&mut el, v("format", format));
    let mut src = XElem::new("source");
    let mut emb = XElem::new("embedded");
    push(&mut emb, v("format", "rawbase64"));
    push(&mut emb, v("strdata", &crate::graph::b64_encode(&bytes)));
    push(&mut src, emb);
    push(&mut el, src);
    if let Some(content) = pkg.doc.root.child_mut("content") {
        let pos = content.children.iter().position(|c| matches!(c, XNode::Elem(e) if e.name == "resource" && e.v("identifier").as_deref() == Some(identifier)));
        match pos {
            Some(i) => {
                content.children[i] = XNode::Elem(el);
            }
            None => {
                content.insert_elem(el, None, &ind, 2);
            }
        }
    } else {
        return Err("the package has no content".into());
    }
    let pkg = Arc::new(pkg);
    let res = embedded_resources(&pkg);
    if let Some(sd) = root.sd.as_mut() {
        sd.pkg = pkg.clone();
    }
    for g in root.subgraphs.values_mut() {
        if let Some(sd) = g.sd.as_mut() {
            sd.pkg = pkg.clone();
        }
    }
    root.resources = res;
    Ok(format!("pkg:///{identifier}?dependency={uid}"))
}

/// Make `dst` (and its graphs) use the package of `src`: after a resource was added while a graph of the package was
/// open, the package the other views hold is the older one.
pub fn adopt_package(dst: &mut Graph, src: &Graph) {
    let Some(pkg) = src.sd.as_ref().map(|s| s.pkg.clone()) else { return };
    if dst.sd.as_ref().map(|s| Arc::ptr_eq(&s.pkg, &pkg)).unwrap_or(true) {
        return;
    }
    if let Some(sd) = dst.sd.as_mut() {
        sd.pkg = pkg.clone();
    }
    for g in dst.subgraphs.values_mut() {
        if let Some(sd) = g.sd.as_mut() {
            sd.pkg = pkg.clone();
        }
    }
}

/// The resources of a package (bitmaps, fonts, SVG ...): (name, how it is kept).
pub fn package_resources(root: &Graph) -> Vec<(String, String)> {
    let Some(sd) = root.sd.as_ref() else { return Vec::new() };
    sd.pkg
        .resources
        .iter()
        .map(|r| {
            let how = match (&r.embedded, &r.filepath) {
                (Some(_), _) => format!("{} stored in the package", r.kind),
                (None, Some(f)) => format!("{} linked: {f}", r.kind),
                (None, None) => r.kind.clone(),
            };
            (r.path(), how)
        })
        .collect()
}

/// The format's normal maps point green down unless a Normal node of the graph says otherwise.
pub fn normal_directx(g: &Graph) -> bool {
    !g.nodes.values().any(|n| n.kind == "sbs:normal" && matches!(values(n).get("inversedy"), Some(PVal::Const { raw, .. }) if raw.trim() == "1"))
}

/// A function or FX-map graph of a package drawn on its own: a pixel processor (or FX-map) running it, as
/// MaterialCraft nodes, and the id of the node that shows the result.
pub fn preview_host(view: &Graph) -> Option<(Graph, u32)> {
    let sd = view.sd.as_ref()?;
    let name = view.meta.get("identifier").cloned().unwrap_or_default();
    let mut host = Graph { size: view.size, height: view.height, ..Default::default() };
    host.meta.insert("identifier".into(), "__preview".into());
    let mut hsd = SdGraph::new(Role::Graph, "__preview", sd.pkg.clone());
    hsd.uid = "1".into();
    host.sd = Some(Box::new(hsd));
    let mut n = Node::default();
    match &sd.role {
        Role::Fx { .. } => {
            n.kind = "sbs:fxmaps".into();
            n.strs.insert("#fx".into(), name.clone());
        }
        _ => {
            n.kind = "sbs:pixelprocessor".into();
            n.strs.insert("perpixel".into(), PVal::Dyn { graph: name.clone(), rel: None }.enc());
        }
    }
    let show = crate::graph::PREVIEW_ID;
    host.nodes.insert(show, n);
    let out = add_output(&mut host, "baseColor", Some("preview"), [200.0, 0.0]);
    if let Some(o) = host.nodes.get_mut(&out) {
        o.ins.insert("inputNodeOutput".into(), show);
    }
    host.subgraphs = view.subgraphs.clone();
    let mut me = view.clone();
    me.subgraphs.clear();
    host.subgraphs.insert(name, me);
    let low = lower(&host).ok()?;
    Some((low.graph, show))
}

/// Whether graph `target` uses graph `name`, itself or through other graphs of `graphs`: an instance of `target`
/// inside `name` would never finish.
pub fn uses_graph(graphs: &BTreeMap<String, &Graph>, target: &str, name: &str) -> bool {
    let (mut seen, mut todo) = (BTreeSet::new(), vec![target.to_string()]);
    while let Some(t) = todo.pop() {
        if t == name {
            return true;
        }
        if !seen.insert(t.clone()) {
            continue;
        }
        if let Some(g) = graphs.get(&t) {
            todo.extend(g.nodes.values().filter(|n| n.kind == "sbs:graph").filter_map(|n| n.strs.get("#graph").cloned()));
        }
    }
    false
}

/// Name a package's graph is kept under (its path in the package).
pub fn graph_name(g: &Graph) -> String {
    g.meta.get("identifier").cloned().unwrap_or_default()
}

/// A copy of `owner` (a graph of a package that is being left for one of its other graphs) without the package's
/// other graphs, to sit among the graphs of the view so instances of it still resolve there. Taken out again with
/// [`drop_owner`] when the view is put back.
pub fn owner_copy(owner: &mut Graph) -> Option<(String, Graph)> {
    owner.sd.as_ref()?;
    let subs = std::mem::take(&mut owner.subgraphs);
    let c = owner.clone();
    owner.subgraphs = subs;
    Some((graph_name(owner), c))
}

/// Put `owner`'s copy among the graphs of a view of one of its package's graphs (see [`owner_copy`]).
pub fn add_owner(subgraphs: &mut BTreeMap<String, Graph>, owner: &mut Graph) {
    if let Some((name, c)) = owner_copy(owner) {
        subgraphs.entry(name).or_insert(c);
    }
}

/// Take the copy [`add_owner`] put into a view's graphs back out before they go back to `owner`.
pub fn drop_owner(subgraphs: &mut BTreeMap<String, Graph>, owner: &Graph) {
    if owner.sd.is_some() {
        subgraphs.remove(&graph_name(owner));
    }
}

/// The graphs of package `root` that use graph `name` as a node.
pub fn graph_users(root: &Graph, name: &str) -> Vec<String> {
    all_graphs(root).into_iter().filter(|(k, g)| k != name && g.nodes.values().any(|n| n.kind == "sbs:graph" && n.strs.get("#graph").map(|t| t == name).unwrap_or(false))).map(|(k, _)| k).collect()
}

fn for_each_graph_mut(root: &mut Graph, mut f: impl FnMut(&mut Graph)) {
    f(root);
    for g in root.subgraphs.values_mut() {
        f(g);
    }
}

/// Rename graph `old` of a package (a bare new name keeps the folder); nodes that use it follow.
pub fn rename_graph(root: &mut Graph, old: &str, new: &str) -> Result<String, String> {
    let folder = old.rsplit_once('/').map(|(f, _)| f.to_string());
    let new = match (new.contains('/'), folder) {
        (false, Some(f)) => format!("{f}/{new}"),
        _ => new.to_string(),
    };
    if new == old {
        return Ok(new);
    }
    if new.is_empty() || !new.split('/').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
        return Err(format!("'{new}' is not a valid graph name (letters, digits and _)"));
    }
    if all_graphs(root).contains_key(&new) {
        return Err(format!("the package already has a graph called '{new}'"));
    }
    rename_unchecked(root, old, &new)?;
    Ok(new)
}

/// [`rename_graph`] without checking the new name.
fn rename_unchecked(root: &mut Graph, old: &str, new: &str) -> Result<(), String> {
    let new = new.to_string();
    let main = root.meta.get("identifier").cloned().unwrap_or_default();
    if main == old {
        root.meta.insert("identifier".into(), new.clone());
        if let Some(sd) = root.sd.as_mut() {
            sd.path = new.clone();
        }
    } else {
        let mut g = root.subgraphs.remove(old).ok_or_else(|| format!("the package has no graph '{old}'"))?;
        g.meta.insert("identifier".into(), new.clone());
        if let Some(sd) = g.sd.as_mut() {
            sd.path = new.clone();
        }
        root.subgraphs.insert(new.clone(), g);
    }
    let (o, n) = (old.to_string(), new.clone());
    for_each_graph_mut(root, |g| {
        for node in g.nodes.values_mut() {
            if node.kind == "sbs:graph" && node.strs.get("#graph") == Some(&o) {
                node.strs.insert("#graph".into(), n.clone());
                node.strs.remove("#path");
            }
        }
    });
    rekey_inline(root, old, &new);
    Ok(())
}

/// The graphs and functions graph `name` needs to render: itself, the graphs it runs, the functions it calls and
/// those inside its nodes, each followed through.
pub fn graphs_needed(root: &Graph, name: &str) -> BTreeSet<String> {
    let graphs = all_graphs(root);
    let (mut keep, mut todo) = (BTreeSet::new(), vec![name.to_string()]);
    while let Some(t) = todo.pop() {
        let Some(g) = graphs.get(&t) else { continue };
        if !keep.insert(t) {
            continue;
        }
        for n in g.nodes.values() {
            if let Some(x) = n.strs.get("#graph") {
                todo.push(x.clone());
            }
            if let Some(fx) = n.strs.get("#fx") {
                todo.extend(fx.split(',').filter(|x| !x.is_empty()).map(str::to_string));
            }
            for v in values(n).into_values() {
                match v {
                    PVal::Dyn { graph, .. } => todo.push(graph),
                    // a function called by path
                    PVal::Const { raw, .. } if raw.starts_with("pkg:///") => todo.push(crate::sbs::split_path(&raw).0.trim_start_matches("pkg:///").to_string()),
                    _ => {}
                }
            }
        }
    }
    keep
}

/// Graph `name` of package `root` as a package of its own, with the graphs and functions it needs; it opens on
/// `name`. Bitmaps and other resources of the package stay in it.
pub fn extract_package(root: &Graph, name: &str) -> Result<Graph, String> {
    let keep = graphs_needed(root, name);
    let mut all: BTreeMap<String, Graph> = root.subgraphs.clone();
    let mut main = root.clone();
    main.subgraphs.clear();
    all.insert(graph_name(root), main);
    let mut out = all.remove(name).ok_or_else(|| format!("the package has no graph '{name}'"))?;
    if !matches!(out.sd.as_ref().map(|s| &s.role), Some(Role::Graph)) {
        return Err(format!("'{name}' is not a compositing graph"));
    }
    out.subgraphs = all.into_iter().filter(|(k, _)| keep.contains(k)).collect();
    out.resources = root.resources.clone();
    Ok(out)
}

/// Add every graph and function of package `other` (read with [`open`]) to package `root`: names that are taken
/// get a number and every graph a new uid. Returns the name `other`'s first graph got, how many graphs came over
/// and what did not come along.
pub fn add_package(root: &mut Graph, mut other: Graph) -> Result<(String, usize, Vec<String>), String> {
    let pkg = root.sd.as_ref().map(|s| s.pkg.clone()).ok_or("not an .sbs package")?;
    let other_pkg = other.sd.as_ref().map(|s| s.pkg.clone()).ok_or("not an .sbs package")?;
    let mut warnings = Vec::new();
    let is_top = |g: &Graph| matches!(g.sd.as_ref().map(|s| &s.role), Some(Role::Graph | Role::Function));
    let mut first = graph_name(&other);
    let tops: Vec<String> = all_graphs(&other).into_iter().filter(|(_, g)| is_top(g)).map(|(k, _)| k).collect();
    let mut taken: BTreeSet<String> = all_graphs(root).into_keys().collect();
    taken.extend(all_graphs(&other).into_keys());
    for name in &tops {
        if !all_graphs(root).contains_key(name) {
            continue;
        }
        let mut k = 2;
        let mut cand = format!("{name}_{k}");
        while taken.contains(&cand) {
            k += 1;
            cand = format!("{name}_{k}");
        }
        rename_unchecked(&mut other, name, &cand)?;
        taken.insert(cand.clone());
        if *name == first {
            first = cand;
        }
    }
    // new uids: graphs are found in the file by them
    let mut used: BTreeSet<String> = all_graphs(root).values().filter_map(|g| g.sd.as_ref().map(|s| s.uid.clone())).collect();
    let mut x = 0x51_7cc1_b727_220a_u64 ^ (used.len() as u64) << 7;
    let mut fresh = || loop {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let u = format!("{}", 1_000_000_000 + (x >> 33) % 3_000_000_000);
        if used.insert(u.clone()) {
            return u;
        }
    };
    let mut subs = std::mem::take(&mut other.subgraphs);
    let name = graph_name(&other);
    subs.insert(name, other);
    let mut count = 0;
    let mut bitmaps = 0;
    for g in subs.values_mut() {
        let top = is_top(g);
        if let Some(sd) = g.sd.as_mut() {
            if top {
                sd.uid = fresh();
                count += 1;
            }
            sd.pkg = pkg.clone();
            sd.orig = None;
        }
        for n in g.nodes.values_mut() {
            // instance paths name dependencies of the package they came from
            if n.kind == "sbs:graph" || n.kind.starts_with("sbs:lib:") || sbs_defs::def(&n.kind).map(|d| d.library.is_some()).unwrap_or(false) {
                n.strs.remove("#path");
            }
            if n.strs.values().any(|v| v.contains("pkg://Resources") || v.contains("pkg:///Resources")) {
                bitmaps += 1;
            }
        }
    }
    if bitmaps > 0 || !other_pkg.resources.is_empty() {
        warnings.push(format!("{} resource(s) of that file (bitmaps, fonts, SVG) are not brought over: add them again with resource.add", other_pkg.resources.len().max(bitmaps)));
    }
    root.subgraphs.extend(subs);
    Ok((first, count, warnings))
}

/// The functions and FX-maps inside the nodes of graph `old` are kept under `old/...`: move them to `new/...` and
/// point the values that use them there.
fn rekey_inline(root: &mut Graph, old: &str, new: &str) {
    let prefix = format!("{old}/");
    let moved: BTreeMap<String, String> = root
        .subgraphs
        .iter()
        .filter(|(k, g)| k.starts_with(&prefix) && matches!(g.sd.as_ref().map(|s| &s.role), Some(Role::Param { .. } | Role::Fx { .. })))
        .map(|(k, _)| (k.clone(), format!("{new}/{}", &k[prefix.len()..])))
        .collect();
    if moved.is_empty() {
        return;
    }
    let owner = |o: &str| if o == old { new.to_string() } else { moved.get(o).cloned().unwrap_or_else(|| o.to_string()) };
    for (from, to) in &moved {
        if let Some(mut g) = root.subgraphs.remove(from) {
            g.meta.insert("identifier".into(), to.clone());
            if let Some(sd) = g.sd.as_mut() {
                sd.path = to.clone();
                match &mut sd.role {
                    Role::Param { owner: o, .. } | Role::Fx { owner: o, .. } => *o = owner(o),
                    _ => {}
                }
            }
            root.subgraphs.insert(to.clone(), g);
        }
    }
    for_each_graph_mut(root, |g| {
        for node in g.nodes.values_mut() {
            for (k, v) in node.strs.iter_mut() {
                if k == "#fx" {
                    *v = v.split(',').map(|x| moved.get(x).cloned().unwrap_or_else(|| x.to_string())).collect::<Vec<_>>().join(",");
                } else if let Some(PVal::Dyn { graph, rel }) = (!k.starts_with('#') && k != "title").then(|| PVal::dec(v)).flatten() {
                    if let Some(to) = moved.get(&graph) {
                        *v = PVal::Dyn { graph: to.clone(), rel }.enc();
                    }
                }
            }
        }
    });
}

/// Remove graph `name` from a package; refused while a node uses it.
pub fn remove_graph(root: &mut Graph, name: &str) -> Result<(), String> {
    let users = graph_users(root, name);
    if !users.is_empty() {
        return Err(format!("'{name}' is still used by {}", users.join(", ")));
    }
    if root.meta.get("identifier").map(|m| m == name).unwrap_or(false) {
        return Err("the graph the package opens on cannot be deleted; open another graph first".into());
    }
    root.subgraphs.remove(name).map(|_| ()).ok_or_else(|| format!("the package has no graph '{name}'"))
}

/// Copy graph `name` (`None`: the one the package opens on) as a new graph; returns the new name.
pub fn duplicate_graph(root: &mut Graph, name: Option<&str>, new: Option<&str>) -> Result<String, String> {
    let main = root.meta.get("identifier").cloned().unwrap_or_default();
    let src = name.unwrap_or(&main).to_string();
    let mut g = if src == main {
        let mut g = root.clone();
        g.subgraphs.clear();
        g
    } else {
        root.subgraphs.get(&src).cloned().ok_or_else(|| format!("the package has no graph '{src}'"))?
    };
    let base = new.map(str::to_string).unwrap_or_else(|| format!("{src}_copy"));
    let graphs = all_graphs(root).into_keys().collect::<BTreeSet<String>>();
    let mut unique = base.clone();
    let mut k = 2;
    while graphs.contains(&unique) {
        if new.is_some() {
            return Err(format!("the package already has a graph called '{base}'"));
        }
        unique = format!("{base}_{k}");
        k += 1;
    }
    g.meta.insert("identifier".into(), unique.clone());
    let uid = free_uid(root, graphs.len() as u64 * 31 + 5);
    if let Some(sd) = g.sd.as_mut() {
        sd.path = unique.clone();
        sd.uid = uid;
    }
    // the copy's nodes get their own copies of the function graphs their values use
    let ids: Vec<u32> = g.nodes.keys().copied().collect();
    for id in ids {
        let n = g.nodes[&id].clone();
        let c = copy_node(&n, &unique, id, &mut root.subgraphs);
        let mut c = c;
        // the copy keeps the node uids: they only need to be unique inside their graph
        if let Some(u) = n.strs.get("#uid") {
            c.strs.insert("#uid".into(), u.clone());
        }
        g.nodes.insert(id, c);
    }
    root.subgraphs.insert(unique.clone(), g);
    Ok(unique)
}

/// Add outputs to a graph, each fed by a Uniform Color of `color` (a Normal node for a normal output).
/// `outs` are (usage, colour) with MaterialCraft or format usage names.
pub fn template_into(g: &mut Graph, outs: &[(&str, [f32; 3])]) {
    for (i, (usage, c)) in outs.iter().enumerate() {
        let y = (i as f32 * 160.0 - 240.0) * POS_SCALE;
        let sdu = sd_usage(usage);
        let ident = sdu.to_ascii_lowercase();
        let out = add_output(g, &sdu, Some(&ident), [-48.0 * POS_SCALE, y]);
        let src = free_id(g);
        let gray = (c[0] - c[1]).abs() < 1e-6 && (c[1] - c[2]).abs() < 1e-6 && sdu != "baseColor" && sdu != "diffuse";
        let mut n = if sdu == "normal" {
            let mut n = Node { kind: "sbs:normal".into(), ..Default::default() };
            n.strs.insert("input2alpha".into(), PVal::Const { ty: PTy::B, raw: "0".into(), rel: Some("0".into()) }.enc());
            n
        } else {
            let mut n = Node { kind: "sbs:uniform".into(), ..Default::default() };
            if gray {
                n.strs.insert("colorswitch".into(), PVal::Const { ty: PTy::B, raw: "0".into(), rel: Some("0".into()) }.enc());
            }
            n.strs.insert("outputcolor".into(), PVal::of_nums(PTy::F(4), &[c[0], c[1], c[2], 1.0], Some("0".into())).enc());
            n
        };
        n.pos = [-208.0 * POS_SCALE, y];
        g.nodes.insert(src, n);
        if let Some(o) = g.nodes.get_mut(&out) {
            o.ins.insert("inputNodeOutput".into(), src);
        }
    }
}

/// The `get_*` function that reads a graph input of type `ty`.
fn get_fn(ty: PTy) -> &'static str {
    match ty {
        PTy::F(1) => "get_float1",
        PTy::F(2) => "get_float2",
        PTy::F(3) => "get_float3",
        PTy::F(_) | PTy::FV => "get_float4",
        PTy::I(1) => "get_integer1",
        PTy::I(2) => "get_integer2",
        PTy::I(3) => "get_integer3",
        PTy::I(_) => "get_integer4",
        PTy::B => "get_bool",
        PTy::S | PTy::Path => "get_string",
    }
}

/// Expose parameter `param` of node `node` of graph `g` as a graph input called `ident` (shown as `label`): the
/// graph gets the input, with the node's current value as default, and the value reads it (the format's way).
/// Returns the input's identifier (made unique).
pub fn expose(g: &mut Graph, node: u32, param: &str, ident: &str, label: &str, range: Option<(f32, f32)>) -> Result<String, String> {
    let n = g.nodes.get(&node).ok_or("no such node")?.clone();
    let info = params_of(&n, &g.subgraphs).into_iter().find(|p| p.id == param).ok_or_else(|| format!("the node has no parameter '{param}'"))?;
    let (ty, raw) = match info.current() {
        PVal::Const { ty, raw, .. } => (ty, raw),
        PVal::Dyn { .. } => return Err(format!("'{param}' is computed by a function already")),
    };
    let ty = if ty == PTy::FV { if raw.split_whitespace().count() == 1 { PTy::F(1) } else { PTy::F(4) } } else { ty };
    let ty = if ty == PTy::Path { PTy::S } else { ty };
    let base: String = ident.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    let base = if base.is_empty() { param.to_string() } else { base };
    let taken: BTreeSet<String> = g.sd.as_ref().map(|s| s.inputs.iter().filter_map(|o| o.v("identifier")).collect()).unwrap_or_default();
    let mut identifier = base.clone();
    let mut k = 1;
    while taken.contains(&identifier) {
        identifier = format!("{base}_{k}");
        k += 1;
    }
    let uid = free_uid(g, node as u64 * 13 + taken.len() as u64);
    let mut p = XElem::new("paraminput");
    push(&mut p, v("identifier", &identifier));
    push(&mut p, v("uid", &uid));
    let mut a = XElem::new("attributes");
    push(&mut a, v("label", if label.is_empty() { &identifier } else { label }));
    push(&mut p, a);
    push(&mut p, v("type", &ty.type_code().to_string()));
    let mut dv = XElem::new("defaultValue");
    push(&mut dv, const_elem(ty, &raw));
    push(&mut p, dv);
    let opt = |name: &str, value: &str| {
        let mut o = XElem::new("option");
        push(&mut o, v("name", name));
        push(&mut o, v("value", value));
        o
    };
    let semi = raw.split_whitespace().collect::<Vec<_>>().join(";");
    let mut w = XElem::new("defaultWidget");
    let (wname, opts) = match ty {
        PTy::B => ("buttons", vec![opt("booleditortype", "pushbuttons"), opt("default", &raw), opt("label0", "False"), opt("label1", "True")]),
        PTy::S => ("text", vec![opt("default", &raw)]),
        PTy::F(3) | PTy::F(4) if param.contains("color") || info.ty == PTy::FV => ("color", vec![opt("default", &semi)]),
        PTy::I(1) if !info.choices.is_empty() => {
            let list = info.choices.iter().enumerate().map(|(i, c)| format!("{i};{c}")).collect::<Vec<_>>().join(";");
            ("dropdownlist", vec![opt("default", &raw), opt("parameters", &list)])
        }
        _ => {
            let (lo, hi) = range.or(info.range).unwrap_or(if matches!(ty, PTy::I(_)) { (0.0, 10.0) } else { (0.0, 1.0) });
            let step = if matches!(ty, PTy::I(_)) { "1" } else { "0.01" };
            ("slider", vec![opt("clamp", "0"), opt("default", &semi), opt("max", &fmt_num(hi)), opt("min", &fmt_num(lo)), opt("step", step)])
        }
    };
    push(&mut w, v("name", wname));
    push(&mut w, list("options", opts));
    push(&mut p, w);
    if let Some(sd) = g.sd.as_mut() {
        sd.inputs.push(p);
    }
    // the value now reads the input
    let owner = g.meta.get("identifier").cloned().unwrap_or_default();
    let fname = format!("{owner}/{node}/{param}");
    let pkg = g.sd.as_ref().map(|s| s.pkg.clone()).unwrap_or_default();
    let mut fg = Graph::default();
    fg.meta.insert("kind".into(), "function".into());
    fg.meta.insert("identifier".into(), fname.clone());
    let f = get_fn(ty);
    let mut gn = Node { kind: sbs_defs::kind_of(Fam::Function, f), ..Default::default() };
    gn.strs.insert(f.to_string(), PVal::Const { ty: PTy::S, raw: identifier.clone(), rel: None }.enc());
    gn.strs.insert("#type".into(), ty.type_code().to_string());
    gn.strs.insert("#nolayout".into(), "1".into());
    fg.nodes.insert(1, gn);
    let mut fsd = SdGraph::new(Role::Param { owner, node, param: param.to_string() }, &fname, pkg);
    fsd.root = Some(1);
    fg.sd = Some(Box::new(fsd));
    g.subgraphs.insert(fname.clone(), fg);
    let rel = match &info.value {
        Some(PVal::Const { rel, .. }) => rel.clone(),
        _ => Some("0".into()),
    };
    if let Some(n) = g.nodes.get_mut(&node) {
        set_value(n, param, Some(PVal::Dyn { graph: fname, rel }));
    }
    Ok(identifier)
}

/// Remove graph input `ident`; values that only read it go back to its default.
pub fn unexpose(g: &mut Graph, ident: &str) -> Result<(), String> {
    let sd = g.sd.as_mut().ok_or("not an .sbs graph")?;
    let i = sd.inputs.iter().position(|p| p.v("identifier").as_deref() == Some(ident)).ok_or_else(|| format!("the graph has no input '{ident}'"))?;
    let p = sd.inputs.remove(i);
    let default = p.child("defaultValue").and_then(const_in);
    let ids: Vec<u32> = g.nodes.keys().copied().collect();
    for id in ids {
        let vals = values(&g.nodes[&id]);
        for (k, val) in vals {
            let PVal::Dyn { graph, rel } = val else { continue };
            let reads_only_it = g.subgraphs.get(&graph).map(|f| f.nodes.len() == 1 && f.nodes.values().all(|n| n.kind.starts_with("sbs:fn:get_") && values(n).values().any(|v| matches!(v, PVal::Const { raw, .. } if raw == ident)))).unwrap_or(false);
            if reads_only_it {
                let back = match &default {
                    Some(PVal::Const { ty, raw, .. }) => Some(PVal::Const { ty: *ty, raw: raw.clone(), rel }),
                    _ => None,
                };
                if let Some(n) = g.nodes.get_mut(&id) {
                    set_value(n, &k, back);
                }
                g.subgraphs.remove(&graph);
            }
        }
    }
    Ok(())
}

/// Change graph input `ident` (its identifier or label): its default value, label and slider range. Returns the
/// identifier.
pub fn set_input(g: &mut Graph, ident: &str, value: Option<&[f32]>, label: Option<&str>, range: Option<(f32, f32)>) -> Result<String, String> {
    let sd = g.sd.as_mut().ok_or("not an .sbs graph")?;
    let i = sd
        .inputs
        .iter()
        .position(|p| p.v("identifier").as_deref() == Some(ident))
        .or_else(|| sd.inputs.iter().position(|p| input_info(p).map(|x| x.5 == ident).unwrap_or(false)))
        .ok_or_else(|| format!("the graph has no input '{ident}'"))?;
    let ind = Indent { unit: None, newline: String::new() };
    let p = &mut sd.inputs[i];
    let (id, ty, _, _, choices, _, _) = input_info(p).ok_or("that input takes an image; it has no value to set")?;
    if let Some(v) = value {
        let ty = if ty == PTy::FV { if v.len() == 1 { PTy::F(1) } else { PTy::F(4) } } else { ty };
        if matches!(ty, PTy::S | PTy::Path) {
            return Err("text inputs keep the text they were given".into());
        }
        if !choices.is_empty() && v.first().map(|x| *x < 0.0 || *x as usize >= choices.len()).unwrap_or(true) {
            return Err(format!("choose 0..{} ({})", choices.len() - 1, choices.join(", ")));
        }
        // as many numbers as the type has: a missing alpha is opaque, other missing lanes repeat the last one
        let mut v = v.to_vec();
        v.truncate(ty.lanes().max(1));
        while v.len() < ty.lanes() {
            v.push(if v.len() == 3 { 1.0 } else { v.last().copied().unwrap_or(0.0) });
        }
        let PVal::Const { raw, .. } = PVal::of_nums(ty, &v, None) else { return Err("not a value".into()) };
        let dv = p.child_or_add("defaultValue", &ind, 0);
        dv.children.clear();
        dv.empty = false;
        dv.children.push(XNode::Elem(const_elem(ty, &raw)));
        let semi = raw.split_whitespace().collect::<Vec<_>>().join(";");
        if let Some(o) = p.path_mut(&["defaultWidget", "options"]) {
            for x in o.elems_mut().filter(|x| x.v("name").as_deref() == Some("default")) {
                x.set_v("value", if ty == PTy::B || !choices.is_empty() { &raw } else { &semi }, &ind, 0);
            }
        }
    }
    if let Some(l) = label {
        let a = p.child_or_add("attributes", &ind, 0);
        a.set_v("label", l, &ind, 0);
    }
    if let Some((lo, hi)) = range {
        if !(lo < hi) {
            return Err("min must be smaller than max".into());
        }
        if let Some(o) = p.path_mut(&["defaultWidget", "options"]) {
            for x in o.elems_mut() {
                match x.v("name").as_deref() {
                    Some("min") => x.set_v("value", &fmt_num(lo), &ind, 0),
                    Some("max") => x.set_v("value", &fmt_num(hi), &ind, 0),
                    _ => {}
                }
            }
        }
    }
    Ok(id)
}

/// A value input of a graph, as its parameter panel shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct InputInfo {
    pub id: String,
    pub label: String,
    pub ty: PTy,
    /// the default as the file writes it (`0.5`, `1 0 0 1`)
    pub default: String,
    pub range: Option<(f32, f32)>,
    pub choices: Vec<String>,
}

/// The value inputs of a graph with their ranges and choices.
pub fn inputs_info(g: &Graph) -> Vec<InputInfo> {
    g.sd.as_ref()
        .map(|s| s.inputs.iter().filter_map(input_info).map(|(id, ty, default, range, choices, label, _)| InputInfo { id, label, ty, default, range, choices }).collect())
        .unwrap_or_default()
}

/// The graph input a value reads when its function graph `fname` only reads one (what exposing a value makes).
pub fn reads_input(g: &Graph, fname: &str) -> Option<String> {
    let f = g.subgraphs.get(fname)?;
    if f.nodes.len() != 1 {
        return None;
    }
    let n = f.nodes.values().next()?;
    if !n.kind.starts_with("sbs:fn:get_") {
        return None;
    }
    values(n).into_values().find_map(|v| match v {
        PVal::Const { raw, .. } => Some(raw),
        _ => None,
    })
}

/// The value inputs of a graph: (identifier, label, type, default).
pub fn value_inputs(g: &Graph) -> Vec<(String, String, PTy, String)> {
    g.sd.as_ref().map(|s| s.inputs.iter().filter_map(input_info).map(|(id, ty, def, _, _, label, _)| (id, label, ty, def)).collect()).unwrap_or_default()
}
