// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reader for `.sbs` packages: the XML package format documented in Adobe's Automation Toolkit reference
//! (github.com/AdobeDocs/substance-automation-toolkit, Apache-2.0). This module only reads the file into plain data
//! (graphs, nodes, connections, parameters, function graphs, resources); `sbs_import` turns that into MaterialCraft
//! graphs. Nothing here depends on any other program being installed.
//!
//! Every value is stored as `<name v="..."/>`, lists as a container element holding one element per item. Unknown
//! elements are ignored, so newer files still load; what could not be understood is reported, not guessed.

use crate::xml::{self, Element};
use std::path::{Path, PathBuf};

/// A constant parameter value as stored in the file.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    /// `constantValueFloat1`..`Float4`
    F(Vec<f32>),
    /// `constantValueInt32` / `Int1`..`Int4`
    I(Vec<i32>),
    B(bool),
    S(String),
}

impl Val {
    /// First component as a number (booleans 0 / 1, text parsed when it is a number).
    pub fn num(&self) -> f32 {
        match self {
            Val::F(v) => v.first().copied().unwrap_or(0.0),
            Val::I(v) => v.first().copied().unwrap_or(0) as f32,
            Val::B(b) => *b as i32 as f32,
            Val::S(s) => s.trim().parse().unwrap_or(0.0),
        }
    }
    /// Component `i` as a number (missing components are 0).
    pub fn at(&self, i: usize) -> f32 {
        match self {
            Val::F(v) => v.get(i).copied().unwrap_or(0.0),
            Val::I(v) => v.get(i).copied().unwrap_or(0) as f32,
            _ if i == 0 => self.num(),
            _ => 0.0,
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Val::F(v) => v.len(),
            Val::I(v) => v.len(),
            _ => 1,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn text(&self) -> String {
        match self {
            Val::S(s) => s.clone(),
            Val::B(b) => (*b as i32).to_string(),
            Val::F(v) => v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" "),
            Val::I(v) => v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" "),
        }
    }
}

/// A parameter: a constant, or a function graph that computes it.
#[derive(Debug, Clone, PartialEq)]
pub enum PValue {
    Const(Val),
    Dynamic(FuncGraph),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    /// inheritance ("0" absolute, other values relative to the parent / input); kept as written
    pub relative_to: Option<String>,
    pub value: PValue,
}

/// Rows of parameters, e.g. the keys of a gradient.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamArray {
    pub name: String,
    pub cells: Vec<Vec<Param>>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Conn {
    /// the input it arrives at
    pub slot: String,
    /// uid of the node it comes from
    pub from: String,
    /// uid of that node's output, when the node has several
    pub from_output: Option<String>,
}

/// A function graph (dynamic parameter value, pixel processor body, function definition).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FuncGraph {
    pub nodes: Vec<FNode>,
    /// uid of the node whose value is the result
    pub root: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FNode {
    pub uid: String,
    pub function: String,
    /// result type when the node handles several
    pub ty: Option<String>,
    pub conns: Vec<Conn>,
    /// extra data: the constant of a `const_*` node, the variable name of `get_*` / `set`, a swizzle, ...
    pub datas: Vec<(String, Val)>,
    pub pos: [f32; 2],
}

/// What a compositing node is.
#[derive(Debug, Clone, PartialEq)]
pub enum Impl {
    /// a built-in (atomic) filter
    Filter { filter: String, params: Vec<Param>, arrays: Vec<ParamArray>, fx_graphs: Vec<FxGraph> },
    /// an instance of a graph: `path` is `pkg:///<graph>?dependency=<uid>`
    Instance { path: String, params: Vec<Param>, bridgings: Vec<(String, String)> },
    /// reads graph input `entry` (a paramInput uid)
    InputBridge { entry: String, params: Vec<Param> },
    /// writes graph output `output` (a graphOutput uid)
    OutputBridge { output: String },
    Unknown(String),
}

/// An FX-map parameter graph (the pattern program of an `fxmaps` filter).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FxGraph {
    pub root: String,
    pub nodes: Vec<FxNode>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FxNode {
    pub uid: String,
    /// `quadrant`, `switch`, `iterate` (or the data's type)
    pub ty: String,
    pub conns: Vec<Conn>,
    pub params: Vec<Param>,
    pub pos: [f32; 2],
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub uid: String,
    pub name: Option<String>,
    pub pos: [f32; 2],
    pub disabled: bool,
    /// (uid, type) of each output
    pub outputs: Vec<(String, Option<String>)>,
    pub conns: Vec<Conn>,
    pub imp: Impl,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputKind {
    /// an image input; `gray` when declared grayscale
    Image { gray: bool },
    Value { default: Option<Val> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParamInput {
    pub identifier: String,
    pub uid: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub group: Option<String>,
    pub kind: InputKind,
    /// widget name (slider, color, dropdownlist, ...)
    pub widget: Option<String>,
    pub min: Option<f32>,
    pub max: Option<f32>,
    /// drop-down entries: (value, label)
    pub choices: Vec<(i32, String)>,
    pub visible_if: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphOutput {
    pub identifier: String,
    pub uid: String,
    pub label: Option<String>,
    pub usage: Option<String>,
    pub group: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuiObject {
    /// `COMMENT`, `FRAME`, `PIN`, ...
    pub kind: String,
    pub text: String,
    pub title: Option<String>,
    pub pos: [f32; 2],
    pub size: Option<[f32; 2]>,
    pub color: Option<[f32; 4]>,
    /// the node a comment is attached to
    pub attached_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Graph {
    pub identifier: String,
    pub uid: String,
    /// folders the graph sits in, `a/b`
    pub folder: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub inputs: Vec<ParamInput>,
    pub outputs: Vec<GraphOutput>,
    pub nodes: Vec<Node>,
    pub base_params: Vec<Param>,
    pub gui: Vec<GuiObject>,
}

impl Graph {
    /// `folder/identifier`, the name instances use.
    pub fn path(&self) -> String {
        if self.folder.is_empty() {
            self.identifier.clone()
        } else {
            format!("{}/{}", self.folder, self.identifier)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub identifier: String,
    pub uid: String,
    pub folder: String,
    pub inputs: Vec<ParamInput>,
    pub body: Option<FuncGraph>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    pub identifier: String,
    pub uid: String,
    pub folder: String,
    /// `bitmap`, `svg`, `font`, `scene`, ...
    pub kind: String,
    pub format: Option<String>,
    /// file path as written (relative to the package, or absolute)
    pub filepath: Option<String>,
    /// content stored inside the package
    pub embedded: Option<Vec<u8>>,
}

impl Resource {
    pub fn path(&self) -> String {
        if self.folder.is_empty() {
            self.identifier.clone()
        } else {
            format!("{}/{}", self.folder, self.identifier)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dependency {
    pub uid: String,
    pub filename: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Package {
    pub file: Option<PathBuf>,
    pub identifier: String,
    pub format_version: String,
    pub dependencies: Vec<Dependency>,
    pub graphs: Vec<Graph>,
    pub functions: Vec<Function>,
    pub resources: Vec<Resource>,
    /// what was skipped while reading (unknown node kinds, damaged values)
    pub warnings: Vec<String>,
}

/// Largest package read (embedded bitmaps make these big).
pub const MAX_FILE: u64 = 1 << 30;

impl Package {
    pub fn open(path: &Path) -> Result<Package, String> {
        let len = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
        if len > MAX_FILE {
            return Err(format!("{}: file is larger than {} MB", path.display(), MAX_FILE >> 20));
        }
        let text = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut p = Package::parse(&String::from_utf8_lossy(&text))?;
        p.file = Some(path.to_path_buf());
        Ok(p)
    }

    pub fn parse(text: &str) -> Result<Package, String> {
        let root = xml::parse(text).map_err(|e| format!("not a readable package ({e})"))?;
        Package::from_root(&root)
    }

    /// A package from its already read root element.
    pub fn from_root(root: &Element) -> Result<Package, String> {
        if !root.name.eq_ignore_ascii_case("package") {
            return Err(format!("not a package file (root element is <{}>)", root.name));
        }
        let mut p = Package {
            identifier: root.v("identifier").unwrap_or_default().to_string(),
            format_version: root.v("formatVersion").unwrap_or_default().to_string(),
            ..Default::default()
        };
        for d in root.list("dependencies") {
            p.dependencies.push(Dependency { uid: d.v("uid").unwrap_or_default().to_string(), filename: d.v("filename").unwrap_or_default().to_string() });
        }
        if let Some(c) = root.child("content") {
            read_content(c, "", &mut p, 0);
        }
        Ok(p)
    }

    pub fn graph(&self, path: &str) -> Option<&Graph> {
        let path = path.trim_matches('/');
        self.graphs.iter().find(|g| g.path() == path).or_else(|| self.graphs.iter().find(|g| g.identifier == path))
    }

    pub fn dependency(&self, uid: &str) -> Option<&Dependency> {
        self.dependencies.iter().find(|d| d.uid == uid)
    }
}

/// Split `pkg:///folder/graph?dependency=123` into (`folder/graph`, Some("123")).
pub fn split_path(p: &str) -> (String, Option<String>) {
    let p = p.trim();
    let body = p.strip_prefix("pkg://").unwrap_or(p);
    let (graph, query) = match body.split_once('?') {
        Some((g, q)) => (g, Some(q)),
        None => (body, None),
    };
    let dep = query.and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("dependency=")).map(str::to_string));
    (graph.trim_matches('/').to_string(), dep)
}

fn read_content(c: &Element, folder: &str, p: &mut Package, depth: usize) {
    if depth > 64 {
        p.warnings.push("folders nested too deeply; the deepest were skipped".into());
        return;
    }
    for e in &c.children {
        match e.name.to_ascii_lowercase().as_str() {
            "graph" => p.graphs.push(read_graph(e, folder, &mut p.warnings)),
            "function" => p.functions.push(Function {
                identifier: e.v("identifier").unwrap_or_default().to_string(),
                uid: e.v("uid").unwrap_or_default().to_string(),
                folder: folder.to_string(),
                inputs: e.list("paramInputs").iter().map(read_param_input).collect(),
                body: e.child("paramValue").and_then(|v| v.child("dynamicValue")).map(read_func),
            }),
            "resource" | "resourcescene" => p.resources.push(read_resource(e, folder, &mut p.warnings)),
            "group" => {
                let id = e.v("identifier").unwrap_or_default();
                let sub = if folder.is_empty() { id.to_string() } else { format!("{folder}/{id}") };
                if let Some(inner) = e.child("content") {
                    read_content(inner, &sub, p, depth + 1);
                }
            }
            // the content of a single kind of item may be wrapped in its own list
            "graphs" | "functions" | "resources" | "groups" | "content" => read_content(e, folder, p, depth + 1),
            _ => {}
        }
    }
}

fn floats(s: &str) -> Vec<f32> {
    s.split_whitespace().filter_map(|x| x.parse::<f32>().ok()).map(|x| if x.is_finite() { x } else { 0.0 }).collect()
}

fn ints(s: &str) -> Vec<i32> {
    s.split_whitespace().filter_map(|x| x.parse::<i64>().ok().or_else(|| x.parse::<f64>().ok().map(|f| f as i64))).map(|x| x.clamp(i32::MIN as i64, i32::MAX as i64) as i32).collect()
}

/// A `constantValue<Type>` element.
pub fn read_const(e: &Element) -> Option<Val> {
    let n = e.name.to_ascii_lowercase();
    let t = n.strip_prefix("constantvalue")?;
    let v = e.attr("v").unwrap_or(&e.text);
    Some(match t {
        "bool" => Val::B(matches!(v.trim(), "1" | "true" | "True")),
        "string" => Val::S(v.to_string()),
        _ if t.starts_with("float") => Val::F(floats(v)),
        _ if t.starts_with("int") => Val::I(ints(v)),
        _ => Val::S(v.to_string()),
    })
}

/// The first `constantValue*` child of `e`.
fn const_in(e: &Element) -> Option<Val> {
    e.children.iter().find_map(read_const)
}

fn read_params(list: &[Element]) -> Vec<Param> {
    list.iter()
        .filter_map(|p| {
            let name = p.v("name")?.to_string();
            let pv = p.child("paramValue")?;
            let value = match pv.child("dynamicValue") {
                Some(d) => PValue::Dynamic(read_func(d)),
                None => PValue::Const(const_in(pv)?),
            };
            Some(Param { name, relative_to: p.v("relativeTo").map(str::to_string), value })
        })
        .collect()
}

fn read_conns(list: &[Element]) -> Vec<Conn> {
    list.iter()
        .filter_map(|c| {
            Some(Conn {
                slot: c.v("identifier")?.to_string(),
                from: c.v("connRef")?.to_string(),
                from_output: c.v("connRefOutput").filter(|s| !s.is_empty()).map(str::to_string),
            })
        })
        .collect()
}

fn read_pos(e: &Element) -> [f32; 2] {
    let g = e.child("GUILayout").and_then(|l| l.v("gpos")).map(floats).unwrap_or_default();
    [g.first().copied().unwrap_or(0.0), g.get(1).copied().unwrap_or(0.0)]
}

pub fn read_func(d: &Element) -> FuncGraph {
    let nodes = d
        .list("paramNodes")
        .iter()
        .map(|n| FNode {
            uid: n.v("uid").unwrap_or_default().to_string(),
            function: n.v("function").unwrap_or_default().to_string(),
            ty: n.v("type").map(str::to_string),
            conns: read_conns(n.list("connections")),
            datas: n.list("funcDatas").iter().filter_map(|f| Some((f.v("name")?.to_string(), f.child("constantValue").and_then(const_in).or_else(|| const_in(f))?))).collect(),
            pos: read_pos(n),
        })
        .collect();
    FuncGraph { nodes, root: d.v("rootnode").or_else(|| d.v("rootNode")).unwrap_or_default().to_string() }
}

fn read_param_input(e: &Element) -> ParamInput {
    let default = e.child("defaultValue").and_then(const_in);
    let ty = e.v("type").unwrap_or_default();
    // image inputs have an entry type (1 colour, 2 grayscale, 3 either) and no default value; the importer also
    // treats every input read by an input node as an image
    let is_image = default.is_none() && (matches!(ty, "1" | "2" | "3") || ty.is_empty());
    let widget = e.child("defaultWidget").or_else(|| e.child("widget"));
    let opt = |name: &str| widget.and_then(|w| w.list("options").iter().find(|o| o.v("name") == Some(name)).and_then(|o| o.v("value")));
    let num = |name: &str| opt(name).and_then(|s| s.split_whitespace().next().and_then(|x| x.parse::<f32>().ok())).filter(|x| x.is_finite());
    let mut choices = Vec::new();
    if let Some(list) = opt("parameters") {
        // drop-down entries are written as "value;label;value;label..." or "value:label" pairs
        let parts: Vec<&str> = list.split([';', '\n']).filter(|s| !s.is_empty()).collect();
        for pair in parts.chunks(2) {
            if let [v, l] = pair {
                if let Ok(v) = v.trim().parse::<i32>() {
                    choices.push((v, l.trim().to_string()));
                }
            }
        }
    }
    let attrs = e.child("attributes");
    ParamInput {
        identifier: e.v("identifier").unwrap_or_default().to_string(),
        uid: e.v("uid").unwrap_or_default().to_string(),
        label: attrs.and_then(|a| a.v("label")).map(str::to_string),
        description: attrs.and_then(|a| a.v("description")).map(str::to_string),
        group: e.v("group").map(str::to_string),
        kind: if is_image { InputKind::Image { gray: ty == "2" } } else { InputKind::Value { default } },
        widget: widget.and_then(|w| w.v("name")).map(str::to_string),
        min: num("min"),
        max: num("max"),
        choices,
        visible_if: e.v("visibleIf").map(str::to_string),
    }
}

fn read_node(e: &Element, warnings: &mut Vec<String>) -> Node {
    let imp = match e.child("compImplementation").and_then(|c| c.children.first()) {
        Some(i) => match i.name.to_ascii_lowercase().as_str() {
            "compfilter" => Impl::Filter {
                filter: i.v("filter").unwrap_or_default().to_string(),
                params: read_params(i.list("parameters")),
                arrays: i
                    .list("paramsArrays")
                    .iter()
                    .map(|a| ParamArray { name: a.v("name").unwrap_or_default().to_string(), cells: a.list("paramsArrayCells").iter().map(|c| read_params(c.list("parameters"))).collect() })
                    .collect(),
                fx_graphs: i.list("paramsGraphs").iter().map(read_fx).collect(),
            },
            "compinstance" => Impl::Instance {
                path: i.v("path").unwrap_or_default().to_string(),
                params: read_params(i.list("parameters")),
                bridgings: i.list("outputBridgings").iter().filter_map(|b| Some((b.v("uid")?.to_string(), b.v("identifier")?.to_string()))).collect(),
            },
            "compinputbridge" => Impl::InputBridge { entry: i.v("entry").unwrap_or_default().to_string(), params: read_params(i.list("parameters")) },
            "compoutputbridge" => Impl::OutputBridge { output: i.v("output").unwrap_or_default().to_string() },
            other => Impl::Unknown(other.to_string()),
        },
        None => Impl::Unknown(String::new()),
    };
    let uid = e.v("uid").unwrap_or_default().to_string();
    if let Impl::Unknown(k) = &imp {
        warnings.push(format!("node {uid}: unknown kind '{k}'"));
    }
    Node {
        uid,
        name: e.v("GUIName").filter(|s| !s.is_empty()).map(str::to_string),
        pos: read_pos(e),
        disabled: e.v("disabled") == Some("1"),
        outputs: e.list("compOutputs").iter().filter_map(|o| Some((o.v("uid")?.to_string(), o.v("comptype").map(str::to_string)))).collect(),
        conns: read_conns(e.list("connections")),
        imp,
    }
}

fn read_fx(g: &Element) -> FxGraph {
    let datas = g.list("paramsGraphDatas");
    let data_of = |uid: &str| datas.iter().find(|d| d.v("uid") == Some(uid));
    let nodes = g
        .list("paramsGraphNodes")
        .iter()
        .map(|n| {
            let data_uid = n.v("data").unwrap_or_default();
            let data = data_of(data_uid);
            // a data set may inherit the settings it does not define from another one
            let mut params: Vec<Param> = Vec::new();
            let mut seen: Vec<&str> = Vec::new();
            let mut cur = data;
            while let Some(d) = cur {
                let uid = d.v("uid").unwrap_or_default();
                if seen.contains(&uid) || seen.len() > 32 {
                    break;
                }
                seen.push(uid);
                for p in read_params(d.list("parameters")) {
                    if !params.iter().any(|q| q.name == p.name) {
                        params.push(p);
                    }
                }
                cur = d.v("inherit").filter(|s| !s.is_empty()).and_then(data_of);
            }
            FxNode {
                uid: n.v("uid").unwrap_or_default().to_string(),
                ty: n.v("type").or_else(|| data.and_then(|d| d.v("type"))).unwrap_or_default().to_string(),
                conns: read_conns(n.list("connections")),
                params,
                pos: read_pos(n),
                disabled: n.v("disabled") == Some("1"),
            }
        })
        .collect();
    FxGraph { root: g.v("rootnode").or_else(|| g.v("rootNode")).unwrap_or_default().to_string(), nodes }
}

fn read_graph(e: &Element, folder: &str, warnings: &mut Vec<String>) -> Graph {
    let attrs = e.child("attributes");
    let outputs = e
        .list("graphOutputs")
        .iter()
        .map(|o| GraphOutput {
            identifier: o.v("identifier").unwrap_or_default().to_string(),
            uid: o.v("uid").unwrap_or_default().to_string(),
            label: o.child("attributes").and_then(|a| a.v("label")).map(str::to_string),
            usage: o.list("usages").iter().find_map(|u| u.v("name")).map(str::to_string),
            group: o.v("group").map(str::to_string),
        })
        .collect();
    let gui = e
        .list("GUIObjects")
        .iter()
        .map(|g| {
            let layout = g.child("GUILayout");
            let gp = layout.and_then(|l| l.v("gpos")).map(floats).unwrap_or_default();
            let size = layout.and_then(|l| l.v("size")).map(floats).filter(|s| s.len() >= 2).map(|s| [s[0], s[1]]);
            let color = g.v("frameColor").map(floats).filter(|c| c.len() >= 3).map(|c| [c[0], c[1], c[2], c.get(3).copied().unwrap_or(1.0)]);
            let dep = g.v("GUIDependency").and_then(|d| d.rsplit('?').next()).filter(|s| !s.is_empty() && !s.contains("NONE")).map(str::to_string);
            GuiObject {
                kind: g.v("type").unwrap_or("COMMENT").to_string(),
                text: g.v("GUIName").unwrap_or_default().to_string(),
                title: g.v("title").map(str::to_string),
                pos: [gp.first().copied().unwrap_or(0.0), gp.get(1).copied().unwrap_or(0.0)],
                size,
                color,
                attached_to: dep,
            }
        })
        .collect();
    let identifier = e.v("identifier").unwrap_or_default().to_string();
    let mut w = Vec::new();
    let nodes = e.list("compNodes").iter().map(|n| read_node(n, &mut w)).collect();
    warnings.extend(w.into_iter().map(|m| format!("graph '{identifier}': {m}")));
    Graph {
        uid: e.v("uid").unwrap_or_default().to_string(),
        folder: folder.to_string(),
        label: attrs.and_then(|a| a.v("label")).map(str::to_string),
        description: attrs.and_then(|a| a.v("description")).map(str::to_string),
        category: attrs.and_then(|a| a.v("category")).map(str::to_string),
        inputs: e.list("paramInputs").iter().map(read_param_input).collect(),
        outputs,
        nodes,
        base_params: read_params(e.list("baseParameters")),
        gui,
        identifier,
    }
}

fn read_resource(e: &Element, folder: &str, warnings: &mut Vec<String>) -> Resource {
    let identifier = e.v("identifier").unwrap_or_default().to_string();
    let embedded = e.path(&["source", "embedded"]).or_else(|| e.path(&["source", "binembedded"])).and_then(|b| {
        let data = b.v("strdata").map(str::to_string).unwrap_or_else(|| b.text.clone());
        let clean: String = data.chars().filter(|c| !c.is_whitespace()).collect();
        let raw = match crate::graph::b64_decode(&clean) {
            Ok(r) => r,
            Err(e) => {
                warnings.push(format!("resource '{identifier}': {e}"));
                return None;
            }
        };
        let fmt = b.v("format").unwrap_or("rawbase64").to_ascii_lowercase();
        if fmt.starts_with("zlib") {
            // zlib stream: 2-byte header, deflate data, 4-byte checksum
            let body = if raw.len() > 2 && raw[0] & 0x0F == 8 { &raw[2..] } else { &raw[..] };
            match crate::png::inflate(body) {
                Ok(d) => Some(d),
                Err(e) => {
                    warnings.push(format!("resource '{identifier}': {e}"));
                    None
                }
            }
        } else {
            Some(raw)
        }
    });
    Resource {
        uid: e.v("uid").unwrap_or_default().to_string(),
        folder: folder.to_string(),
        kind: e.v("type").unwrap_or_default().to_string(),
        format: e.v("format").map(str::to_string),
        filepath: e.v("filepath").or_else(|| e.v("filePath")).filter(|s| !s.is_empty()).map(str::to_string),
        embedded,
        identifier,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_instance_paths() {
        assert_eq!(split_path("pkg:///tile_generator?dependency=1290776959"), ("tile_generator".into(), Some("1290776959".into())));
        assert_eq!(split_path("pkg:///folder/My_Graph"), ("folder/My_Graph".into(), None));
    }

    #[test]
    fn reads_constants() {
        let e = xml::parse("<constantValueFloat3 v=\"0.5 1 nan\"/>").unwrap();
        assert_eq!(read_const(&e), Some(Val::F(vec![0.5, 1.0, 0.0])));
        let e = xml::parse("<constantValueInt2 v=\"11 11\"/>").unwrap();
        assert_eq!(read_const(&e), Some(Val::I(vec![11, 11])));
        let e = xml::parse("<constantValueBool v=\"1\"/>").unwrap();
        assert_eq!(read_const(&e), Some(Val::B(true)));
    }
}
