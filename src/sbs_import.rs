// SPDX-License-Identifier: MIT OR Apache-2.0
//! Turns an `.sbs` package (read by [`crate::sbs`]) into a MaterialCraft package: the first graph becomes the root
//! graph, the other graphs and functions become graphs of the package, graphs used from other `.sbs` files come along
//! as embedded graphs. Every node is converted to the closest MaterialCraft node and the import report says how
//! close: exact (same operation, values carried over), approximate (our own implementation of the same idea, or
//! values whose units differ) or not converted (a clearly marked placeholder keeps the graph connected).
//!
//! The built-in node definitions (inputs, parameters, defaults) follow the published format reference. Library graphs
//! referenced by name are mapped to MaterialCraft's own nodes; their internals are never needed or read.

use crate::graph::{Comment, Graph, Node};
use crate::sbs::{self, Conn, FuncGraph, Impl, InputKind, PValue, Package, Param, ParamInput, Val};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Info,
    Approx,
    Missing,
}

impl Level {
    pub fn tag(self) -> &'static str {
        match self {
            Level::Info => "note",
            Level::Approx => "approximate",
            Level::Missing => "not converted",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Note {
    pub level: Level,
    pub graph: String,
    pub node: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub source: String,
    pub notes: Vec<Note>,
    pub graphs: usize,
    pub functions: usize,
    pub nodes: usize,
    pub exact: usize,
    pub approx: usize,
    pub missing: usize,
}

impl Report {
    pub fn summary(&self) -> String {
        format!(
            "{} graph{}, {} function{}, {} nodes: {} exact, {} approximate, {} not converted",
            self.graphs,
            if self.graphs == 1 { "" } else { "s" },
            self.functions,
            if self.functions == 1 { "" } else { "s" },
            self.nodes,
            self.exact,
            self.approx,
            self.missing
        )
    }

    pub fn to_text(&self) -> String {
        let mut s = format!("Import of {}\n{}\n", self.source, self.summary());
        let mut notes = self.notes.clone();
        notes.sort_by(|a, b| b.level.cmp(&a.level).then(a.graph.cmp(&b.graph)).then(a.node.cmp(&b.node)));
        for n in &notes {
            let at = match n.node {
                Some(id) => format!("{} node {id}", n.graph),
                None => n.graph.clone(),
            };
            s.push_str(&format!("[{}] {at}: {}\n", n.level.tag(), n.text));
        }
        s
    }
}

pub struct Imported {
    pub graph: Graph,
    pub report: Report,
}

/// Import a `.sbs` file and the `.sbs` files it uses.
pub fn import_file(path: &Path) -> Result<Imported, String> {
    let pkg = Package::open(path)?;
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "package".into());
    import_package(pkg, &dir, &stem)
}

/// Import an already read package; `dir` is where relative paths (bitmaps, other packages) are looked up.
pub fn import_package(pkg: Package, dir: &Path, stem: &str) -> Result<Imported, String> {
    import_package_as(pkg, dir, stem, None)
}

/// Import a package with graph `root` (its path in the package) as the main graph; `None` takes the first graph
/// that has outputs.
pub fn import_package_as(pkg: Package, dir: &Path, stem: &str, root: Option<&str>) -> Result<Imported, String> {
    if pkg.graphs.is_empty() {
        return Err("the package contains no graphs".into());
    }
    let mut cx = Cx { srcs: Vec::new(), names: HashMap::new(), busy: HashSet::new(), lib: BTreeMap::new(), res: BTreeMap::new(), used: HashSet::new(), rep: Report { source: format!("{stem}.sbs"), ..Default::default() }, fn_names: HashMap::new() };
    for w in &pkg.warnings {
        cx.rep.notes.push(Note { level: Level::Info, graph: String::new(), node: None, text: w.clone() });
    }
    let file = pkg.file.as_ref().map(|f| std::fs::canonicalize(f).unwrap_or_else(|_| f.clone()));
    cx.srcs.push(Src { pkg, dir: dir.to_path_buf(), stem: stem.to_string(), file });
    // the main package's graphs keep their names; the root graph is the first one that has outputs
    let paths: Vec<String> = cx.srcs[0].pkg.graphs.iter().map(|g| g.path()).collect();
    let root_path = match root.and_then(|r| cx.srcs[0].pkg.graph(r)) {
        Some(g) => g.path(),
        None => cx.srcs[0].pkg.graphs.iter().find(|g| !g.outputs.is_empty()).map(|g| g.path()).unwrap_or_else(|| paths[0].clone()),
    };
    for p in &paths {
        let ident = cx.srcs[0].pkg.graph(p).map(|g| g.identifier.clone()).unwrap_or_default();
        let name = cx.unique(&san(&ident));
        cx.names.insert((0, p.clone()), name);
    }
    let fpaths: Vec<String> = cx.srcs[0].pkg.functions.iter().map(|f| if f.folder.is_empty() { f.identifier.clone() } else { format!("{}/{}", f.folder, f.identifier) }).collect();
    for p in &fpaths {
        cx.function_name(0, p);
    }
    for p in &paths {
        cx.graph_name(0, p);
    }
    let root_name = cx.names.get(&(0, root_path.clone())).cloned().unwrap_or_default();
    let mut root = cx.lib.remove(&root_name).unwrap_or_default();
    root.meta.insert("identifier".into(), root_name.clone());
    root.meta.insert("imported_from".into(), cx.rep.source.clone());
    root.subgraphs = std::mem::take(&mut cx.lib);
    root.resources = std::mem::take(&mut cx.res);
    Ok(Imported { graph: root, report: cx.rep })
}

// ------------------------------------------------------------------------------------------------ names

/// A MaterialCraft name: letters, digits and underscores.
pub fn san(s: &str) -> String {
    let mut o: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if o.is_empty() {
        o = "graph".into();
    }
    if o.starts_with(|c: char| c.is_ascii_digit()) {
        o.insert(0, '_');
    }
    o
}

struct Src {
    pkg: Package,
    dir: PathBuf,
    stem: String,
    file: Option<PathBuf>,
}

enum Target {
    Graph { src: usize, path: String },
    /// a graph of the standard library: package stem and graph identifier
    Library(String, String),
    Missing(String),
}

struct Cx {
    srcs: Vec<Src>,
    /// (source package, graph path) -> MaterialCraft graph name
    names: HashMap<(usize, String), String>,
    busy: HashSet<(usize, String)>,
    lib: BTreeMap<String, Graph>,
    res: BTreeMap<String, Arc<Vec<u8>>>,
    used: HashSet<String>,
    rep: Report,
    fn_names: HashMap<(usize, String), String>,
}

/// Largest number of `.sbs` files one import reads.
const MAX_PACKAGES: usize = 64;

impl Cx {
    fn unique(&mut self, base: &str) -> String {
        let mut n = base.to_string();
        let mut k = 2;
        while self.used.contains(&n) {
            n = format!("{base}_{k}");
            k += 1;
        }
        self.used.insert(n.clone());
        n
    }

    fn note(&mut self, level: Level, graph: &str, node: Option<u32>, text: impl Into<String>) {
        self.rep.notes.push(Note { level, graph: graph.to_string(), node, text: text.into() });
    }

    /// The package a dependency uid of `src` refers to (loading it when needed); `None` = the package itself.
    fn dependency(&mut self, src: usize, uid: &str) -> Result<Option<usize>, String> {
        let Some(dep) = self.srcs[src].pkg.dependency(uid).cloned() else {
            return Ok(None);
        };
        let f = dep.filename.trim();
        if f.is_empty() || f == "?himself" {
            return Ok(None);
        }
        if f.starts_with("sbs://") {
            return Err(format!("library:{}", crate::sbs_lib::package_stem(f)));
        }
        if f.to_ascii_lowercase().ends_with(".sbsar") {
            return Err(format!("'{f}' is a compiled package (.sbsar); only editable .sbs files can be imported"));
        }
        let p = {
            let raw = PathBuf::from(f);
            if raw.is_absolute() {
                raw
            } else {
                self.srcs[src].dir.join(raw)
            }
        };
        let canon = std::fs::canonicalize(&p).unwrap_or(p.clone());
        if self.srcs[src].file.as_ref() == Some(&canon) {
            return Ok(None);
        }
        if let Some(i) = self.srcs.iter().position(|s| s.file.as_ref() == Some(&canon)) {
            return Ok(Some(i));
        }
        if self.srcs.len() >= MAX_PACKAGES {
            return Err(format!("more than {MAX_PACKAGES} packages are referenced; '{f}' was not read"));
        }
        let pkg = Package::open(&canon).map_err(|e| format!("could not read '{f}': {e}"))?;
        let stem = canon.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "package".into());
        let dir = canon.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        self.srcs.push(Src { pkg, dir, stem, file: Some(canon) });
        Ok(Some(self.srcs.len() - 1))
    }

    fn resolve(&mut self, src: usize, path: &str) -> Target {
        let (gpath, dep) = sbs::split_path(path);
        let target_src = match dep {
            None => src,
            Some(uid) => match self.dependency(src, &uid) {
                Ok(None) => src,
                Ok(Some(i)) => i,
                Err(e) if e.starts_with("library:") => return Target::Library(e["library:".len()..].to_string(), gpath.rsplit('/').next().unwrap_or(&gpath).to_string()),
                Err(e) => return Target::Missing(e),
            },
        };
        if self.srcs[target_src].pkg.graph(&gpath).is_some() {
            Target::Graph { src: target_src, path: gpath }
        } else {
            Target::Missing(format!("graph '{gpath}' is not in {}", self.srcs[target_src].stem))
        }
    }

    /// Name of the converted graph, converting it first when needed.
    fn graph_name(&mut self, src: usize, path: &str) -> Option<String> {
        let g = self.srcs[src].pkg.graph(path)?;
        let key = (src, g.path());
        if !self.names.contains_key(&key) {
            let base = if src == 0 { san(&g.identifier) } else { san(&format!("{}_{}", self.srcs[src].stem, g.identifier)) };
            let n = self.unique(&base);
            self.names.insert(key.clone(), n);
        }
        let name = self.names[&key].clone();
        if self.lib.contains_key(&name) || self.busy.contains(&key) {
            return Some(name);
        }
        self.busy.insert(key.clone());
        let mut out = self.convert_graph(src, &key.1, &name);
        if src != 0 {
            out.meta.insert("embedded".into(), "1".into());
        }
        self.busy.remove(&key);
        self.rep.graphs += 1;
        self.lib.insert(name.clone(), out);
        Some(name)
    }

    /// Make the built-in library functions (native versions of Substance Designer's `functions.sbs`) available for
    /// `f_call` resolution, once.
    fn ensure_libfuncs(&mut self) {
        if self.lib.contains_key("libfn_clamp") {
            return;
        }
        for (k, g) in crate::libfuncs::graphs() {
            self.lib.entry(k.clone()).or_insert_with(|| g.clone());
        }
    }

    /// Output arity (lane count) of an already-converted function in `self.lib`, or `None` if it does not compile.
    fn fn_out_lanes(&self, name: &str) -> Option<usize> {
        let l = crate::func::compile_named(&self.lib, name).ok()?.result_type().ok()?.lanes();
        (l >= 1).then_some(l)
    }

    fn function_name(&mut self, src: usize, path: &str) -> Option<String> {
        let path = path.trim_matches('/').to_string();
        let f = self.srcs[src].pkg.functions.iter().find(|f| (if f.folder.is_empty() { f.identifier.clone() } else { format!("{}/{}", f.folder, f.identifier) }) == path || f.identifier == path)?.clone();
        let key = (src, path);
        if let Some(n) = self.fn_names.get(&key) {
            return Some(n.clone());
        }
        let base = if src == 0 { san(&f.identifier) } else { san(&format!("{}_{}", self.srcs[src].stem, f.identifier)) };
        let name = self.unique(&base);
        self.fn_names.insert(key, name.clone());
        let body = f.body.clone().unwrap_or_default();
        let fc = FnCx { src, graph_inputs: Vec::new(), fn_inputs: f.inputs.clone(), graph: name.clone() };
        let mut g = self.convert_func(&body, &fc);
        if src != 0 {
            g.meta.insert("embedded".into(), "1".into());
        }
        g.meta.insert("identifier".into(), name.clone());
        self.rep.functions += 1;
        self.lib.insert(name.clone(), g);
        Some(name)
    }
}

// ------------------------------------------------------------------------------------------------ parameters

fn find<'a>(params: &'a [Param], name: &str) -> Option<&'a Param> {
    params.iter().find(|p| p.name.eq_ignore_ascii_case(name))
}

fn cval<'a>(params: &'a [Param], name: &str) -> Option<&'a Val> {
    match find(params, name).map(|p| &p.value) {
        Some(PValue::Const(v)) => Some(v),
        _ => None,
    }
}

fn dynv<'a>(params: &'a [Param], name: &str) -> Option<&'a FuncGraph> {
    match find(params, name).map(|p| &p.value) {
        Some(PValue::Dynamic(f)) => Some(f),
        _ => None,
    }
}

/// A function bound to a parameter of the node being converted.
pub(crate) struct Bind {
    pub(crate) param: String,
    pub(crate) func: FuncGraph,
    pub(crate) lane: Option<usize>,
    pub(crate) scale: f32,
    pub(crate) offset: f32,
}

/// What converting one node produced.
pub(crate) struct Conv {
    pub(crate) node: Node,
    /// input slot in the file -> MaterialCraft slot
    pub(crate) slots: Vec<(String, String)>,
    /// the MaterialCraft slots, in order, for inputs that match no name
    pub(crate) order: Vec<String>,
    /// output uid -> MaterialCraft output name (for nodes with several outputs)
    pub(crate) outputs: HashMap<String, String>,
    pub(crate) level: Level,
    pub(crate) notes: Vec<(Level, String)>,
    pub(crate) binds: Vec<Bind>,
    /// a function graph that drives the whole node (pixel / value processor)
    pub(crate) body: Option<FuncGraph>,
    /// the pattern graph of a pattern map, converted once the node's graph is known
    pub(crate) fx: Option<sbs::FxGraph>,
}

impl Conv {
    pub(crate) fn new(kind: &str, level: Level) -> Conv {
        Conv { node: Node { kind: kind.into(), ..Default::default() }, slots: Vec::new(), order: Vec::new(), outputs: HashMap::new(), level, notes: Vec::new(), binds: Vec::new(), body: None, fx: None }
    }
    fn slot(mut self, sd: &str, mc: &str) -> Conv {
        self.slots.push((sd.into(), mc.into()));
        self
    }
    pub(crate) fn approx(&mut self, why: impl Into<String>) {
        if self.level < Level::Approx {
            self.level = Level::Approx;
        }
        self.notes.push((Level::Approx, why.into()));
    }
    pub(crate) fn num(&mut self, k: &str, v: f32) {
        self.node.nums.insert(k.into(), if v.is_finite() { v } else { 0.0 });
    }
    /// Carry parameter `sd` (default `def`) into `mc` as `v * scale + offset`, constant or function-driven.
    fn lin(&mut self, params: &[Param], sd: &str, def: f32, mc: &str, scale: f32, offset: f32) {
        self.lin_lane(params, sd, def, mc, None, scale, offset);
    }
    fn lin_lane(&mut self, params: &[Param], sd: &str, def: f32, mc: &str, lane: Option<usize>, scale: f32, offset: f32) {
        match find(params, sd).map(|p| &p.value) {
            Some(PValue::Const(v)) => {
                let x = match lane {
                    Some(i) => v.at(i),
                    None => v.num(),
                };
                self.num(mc, x * scale + offset);
            }
            Some(PValue::Dynamic(f)) => {
                self.num(mc, def * scale + offset);
                self.binds.push(Bind { param: mc.into(), func: f.clone(), lane, scale, offset });
            }
            None => self.num(mc, def * scale + offset),
        }
    }
    /// Same, for every lane of a colour (`mcs` names one parameter per lane).
    fn color(&mut self, params: &[Param], sd: &str, def: [f32; 4], mcs: [&str; 4]) {
        for (i, mc) in mcs.iter().enumerate() {
            if mc.is_empty() {
                continue;
            }
            let d = def[i];
            match find(params, sd).map(|p| &p.value) {
                Some(PValue::Const(v)) => {
                    // a grayscale value fills all colour lanes
                    let x = if v.len() == 1 && i < 3 { v.num() } else if v.len() == 1 { d } else { v.at(i) };
                    self.num(mc, x);
                }
                Some(PValue::Dynamic(f)) => {
                    self.num(mc, d);
                    self.binds.push(Bind { param: (*mc).into(), func: f.clone(), lane: Some(i), scale: 1.0, offset: 0.0 });
                }
                None => self.num(mc, d),
            }
        }
    }
}

/// Image-pattern inputs of our pattern program map, in order.
const FX_IMAGE_SLOTS: [&str; 8] = ["in", "in2", "in3", "in4", "in5", "in6", "in7", "in8"];

// ------------------------------------------------------------------------------------------------ enums

/// Blending modes of the built-in blend, in the order the editor that writes these files lists them, mapped to ours.
/// (Verified against sample files when they are available; see the import report.)
fn blend_mode(v: i32) -> Option<f32> {
    Some(match v {
        0 => 0.0,   // copy
        1 => 1.0,   // add
        2 => 2.0,   // subtract
        3 => 3.0,   // multiply
        4 => 20.0,  // add / subtract
        5 => 6.0,   // max (lighten)
        6 => 7.0,   // min (darken)
        7 => 21.0,  // switch
        8 => 15.0,  // divide
        9 => 5.0,   // overlay
        10 => 4.0,  // screen
        11 => 9.0,  // soft light
        _ => return None,
    })
}

/// Output usages written in the file -> our output usages.
pub fn usage(sd: &str) -> &'static str {
    match sd.to_ascii_lowercase().as_str() {
        "basecolor" => "basecolor",
        "diffuse" => "diffuse",
        "normal" => "normal",
        "roughness" => "roughness",
        "metallic" | "metalness" => "metallic",
        "height" | "displacement" | "bump" => "height",
        "ambientocclusion" => "ambient_occlusion",
        "opacity" => "opacity",
        "emissive" | "emissioncolor" => "emissive",
        "specular" | "specularlevel" | "specularcolor" => "specular",
        "glossiness" => "glossiness",
        _ => "out",
    }
}

// ------------------------------------------------------------------------------------------------ library nodes

/// Graphs from the standard library, by identifier, mapped to our node kinds. Names that differ only by a
/// `_grayscale` / `_color` / number suffix fall back to the bare name.
const LIBRARY: &[(&str, &str)] = &[
    ("perlin_noise", "perlin"),
    ("perlin_noise_zoom", "perlin"),
    ("clouds", "clouds"),
    ("gaussian_noise", "gaussian_noise"),
    ("white_noise", "white_noise"),
    ("bnw_spots", "bnw_spots"),
    ("crystal", "crystal"),
    ("directional_noise", "directional_noise"),
    ("directional_scratches", "scratches"),
    ("cells", "cells"),
    ("fractal_sum", "fractal_sum"),
    ("fractal_sum_base", "fractal_sum"),
    ("anisotropic_noise", "anisotropic_noise"),
    ("fur", "fur_noise"),
    ("moisture_noise", "moisture_noise"),
    ("microscope_view", "microscope_noise"),
    ("blue_noise_fast", "blue_noise"),
    ("plasma", "plasma"),
    ("grunge_map", "grunge_map"),
    ("shape", "shape"),
    ("polygon", "polygon"),
    ("star", "star"),
    ("tile_generator", "tile_generator"),
    ("tile_sampler", "tile_sampler"),
    ("splatter", "splatter"),
    ("splatter_circular", "splatter_circular"),
    ("weave_generator", "weave_generator"),
    ("weave", "weave"),
    ("stripes", "stripes"),
    ("checker", "checker"),
    ("waveform", "waveform"),
    ("triangle_grid", "triangle_grid"),
    ("gradient_linear", "gradient_axial"),
    ("gradient_axial", "gradient_axial"),
    ("gradient_circular", "gradient_circular"),
    ("gradient_axial_reflected", "gradient_reflected"),
    ("histogram_scan", "histogram_scan"),
    ("histogram_range", "histogram_range"),
    ("histogram_shift", "histogram_shift"),
    ("auto_levels", "auto_levels"),
    ("invert", "invert"),
    ("clamp", "clamp"),
    ("pow", "pow"),
    ("highpass", "highpass"),
    ("quantize", "quantize"),
    ("threshold", "threshold"),
    ("contrast_luminosity", "contrast_luminosity"),
    ("replace_color", "replace_color"),
    ("color_match", "color_match"),
    ("color_to_mask", "color_to_mask"),
    ("chrominance_extract", "chrominance_extract"),
    ("grayscale_conversion_advanced", "grayscale_conversion"),
    ("blur_hq", "blur_hq"),
    ("non_uniform_blur", "non_uniform_blur"),
    ("slope_blur", "slope_blur"),
    ("anisotropic_blur", "anisotropic_blur"),
    ("radial_blur", "radial_blur"),
    ("vector_warp", "vector_warp"),
    ("multi_directional_warp", "multi_directional_warp"),
    ("non_uniform_directional_warp", "non_uniform_directional_warp"),
    ("swirl", "swirl"),
    ("mirror", "mirror"),
    ("symmetry", "symmetry"),
    ("safe_transform", "safe_transform"),
    ("skew", "skew"),
    ("clone_patch", "clone_patch"),
    ("edge_detect", "edge_detect"),
    ("bevel", "bevel"),
    ("curvature", "curvature"),
    ("curvature_smooth", "curvature_smooth"),
    ("ambient_occlusion", "ao"),
    ("ambient_occlusion_hbao", "ao"),
    ("height_blend", "height_blend"),
    ("material_blend", "material_blend"),
    ("normal_combine", "normal_combine"),
    ("normal_blend", "normal_blend"),
    ("normal_normalise", "normal_normalize"),
    ("normal_normalize", "normal_normalize"),
    ("normal_invert", "normal_invert"),
    ("normal_to_height", "normal_to_height"),
    ("normal_vector_rotation", "normal_vector_rotation"),
    ("make_it_tile_patch", "make_it_tile_patch"),
    ("make_it_tile_photo", "make_it_tile_photo"),
    ("flood_fill", "flood_fill"),
    ("flood_fill_to_random_grayscale", "flood_fill_random"),
    ("flood_fill_to_random_color", "flood_fill_color"),
    ("flood_fill_to_gradient", "flood_fill_gradient"),
    ("flood_fill_to_index", "flood_fill_to_index"),
    ("flood_fill_to_position", "flood_fill_to_position"),
    ("flood_fill_to_bbox_size", "flood_fill_to_bbox_size"),
    ("rgba_merge", "merge"),
    ("rgba_split", "rgba_split"),
    ("alpha_merge", "alpha_merge"),
    ("alpha_split", "alpha_split"),
    ("switch", "switch"),
    ("multi_switch", "multi_switch"),
    ("glow", "glow"),
    ("mosaic", "mosaic"),
    ("edge_wear", "edge_wear"),
    ("dirt", "dirt"),
    ("dust", "dust"),
    ("emboss_with_gloss", "emboss"),
    ("uber_emboss", "uber_emboss"),
    ("quad_transform", "quad_transform"),
    ("trapezoid_transform", "trapezoid_transform"),
    ("shape_glow", "shape_glow"),
    ("shape_drop_shadow", "shape_drop_shadow"),
    ("shape_stroke", "shape_stroke"),
    ("bilateral_blur", "bilateral_blur"),
    ("frequency_split", "frequency_split"),
    ("frequency_merge", "frequency_merge"),
    ("lookup_table", "lookup_table"),
    ("polar_to_cartesian", "polar_convert"),
    ("cartesian_to_polar", "polar_convert"),
];

/// Our node kind for a library graph identifier.
/// MaterialCraft renders library graph `id` of `package` with one of its own nodes.
pub fn library_renders(package: &str, id: &str) -> bool {
    crate::sbs_adapt::adapter(package, id).is_some() || crate::nodes10::kind_for(package, id).is_some() || library_kind(id).is_some()
}

pub fn library_kind(id: &str) -> Option<&'static str> {
    let mut base = id.to_ascii_lowercase();
    for _ in 0..4 {
        if let Some((_, k)) = LIBRARY.iter().find(|(n, _)| *n == base) {
            return Some(k);
        }
        if crate::graph::spec(&base).is_some() {
            return crate::graph::SPECS.iter().find(|s| s.kind == base).map(|s| s.kind);
        }
        // strip one suffix: _grayscale / _color / _gray / _2 ...
        let stripped = ["_grayscale", "_color", "_colour", "_gray", "_grey"].iter().find_map(|s| base.strip_suffix(s).map(str::to_string)).or_else(|| {
            let t = base.trim_end_matches(|c: char| c.is_ascii_digit());
            t.strip_suffix('_').filter(|_| t.len() < base.len()).map(str::to_string)
        });
        match stripped {
            Some(s) if !s.is_empty() => base = s,
            _ => break,
        }
    }
    None
}

// ------------------------------------------------------------------------------------------------ functions

struct FnCx {
    src: usize,
    /// value inputs of the graph the function lives in (read with `get_*`)
    graph_inputs: Vec<ParamInput>,
    /// inputs of a function definition
    fn_inputs: Vec<ParamInput>,
    graph: String,
}

fn type_code_of_get(func: &str) -> f32 {
    match func {
        "get_bool" => 2.0,
        "get_integer1" => 1.0,
        "get_integer2" => 6.0,
        "get_integer3" => 7.0,
        "get_integer4" => 8.0,
        "get_float2" => 3.0,
        "get_float3" => 4.0,
        "get_float4" => 5.0,
        "get_string" => 9.0,
        _ => 0.0,
    }
}

fn type_code_of_val(v: &Val) -> f32 {
    match v {
        Val::B(_) => 2.0,
        Val::S(_) => 9.0,
        Val::I(x) => match x.len() {
            2 => 6.0,
            3 => 7.0,
            4 => 8.0,
            _ => 1.0,
        },
        Val::F(x) => match x.len() {
            2 => 3.0,
            3 => 4.0,
            4 => 5.0,
            _ => 0.0,
        },
    }
}

/// Number of lanes a function node returns (1 when unknown).
/// Lane count of function node `uid`'s output. `inst` gives the return arity of `instance` nodes (which call
/// another function, so their lanes are that function's output, not their first input's); keyed by node uid.
fn lanes(fg: &FuncGraph, uid: &str, depth: usize, inst: &HashMap<String, usize>) -> usize {
    if let Some(&l) = inst.get(uid) {
        return l;
    }
    let Some(n) = fg.nodes.iter().find(|n| n.uid == uid) else { return 1 };
    if depth > 64 {
        return 1;
    }
    let f = n.function.as_str();
    let tail = |s: &str| s.chars().last().and_then(|c| c.to_digit(10)).unwrap_or(1) as usize;
    match f {
        _ if f.starts_with("const_float") || f.starts_with("const_int") || f.starts_with("get_float") || f.starts_with("get_integer") => tail(f),
        "vector2" | "ivector2" | "cartesian" | "swizzle2" | "iswizzle2" | "toint2" | "tofloat2" => 2,
        "vector3" | "ivector3" | "swizzle3" | "iswizzle3" | "toint3" | "tofloat3" => 3,
        "vector4" | "ivector4" | "swizzle4" | "iswizzle4" | "toint4" | "tofloat4" | "samplecol" => 4,
        "dot" | "samplelum" | "atan2" | "rand" | "swizzle1" | "iswizzle1" | "toint1" | "tofloat" | "eq" | "noteq" | "gt" | "gteq" | "lr" | "lreq" | "and" | "or" | "not" => 1,
        _ => n.conns.first().map(|c| lanes(fg, &c.from, depth + 1, inst)).unwrap_or(1),
    }
}

/// Input slot of our function node for a slot name of the file's function node.
fn fn_slot(func: &str, slot: &str) -> Option<&'static str> {
    Some(match (func, slot) {
        ("ifelse", "condition") => "cond",
        ("ifelse", "ifpath") => "a",
        ("ifelse", "elsepath") => "b",
        ("sequence", "seqin") => "a",
        ("sequence", "seqlast") => "b",
        ("while", "init" | "initialization" | "seqin" | "in") => "a",
        ("while", "condition" | "cond" | "exitcond" | "exit" | "exitcondition") => "b",
        ("while", "loop" | "loopbody" | "body" | "seqlast" | "last") => "c",
        ("lerp", "a") => "a",
        ("lerp", "b") => "b",
        ("lerp", "x") => "c",
        ("mulscalar", "a") => "a",
        ("mulscalar", "scalar") => "b",
        (_, "a") => "a",
        (_, "b") => "b",
        (_, "value") => "a",
        (_, "vector") => "a",
        (_, "pos") => "a",
        (_, "input") => "a",
        _ => return None,
    })
}

impl Cx {
    /// A function graph of the file as one of our function graphs.
    fn convert_func(&mut self, fg: &FuncGraph, fc: &FnCx) -> Graph {
        let mut g = Graph::default();
        g.meta.insert("kind".into(), "function".into());
        let mut next = 1u32;
        let mut ids: HashMap<String, u32> = HashMap::new();
        let mut add = |g: &mut Graph, n: Node| -> u32 {
            let id = next;
            next += 1;
            g.nodes.insert(id, n);
            id
        };
        // function inputs first, in their declared order (calls fill them in node-number order)
        let mut fin: HashMap<String, u32> = HashMap::new();
        for (i, p) in fc.fn_inputs.iter().enumerate() {
            let mut n = Node { kind: "f_input".into(), pos: [-300.0, i as f32 * 60.0], ..Default::default() };
            n.strs.insert("name".into(), p.identifier.clone());
            if let InputKind::Value { default: Some(d) } = &p.kind {
                n.nums.insert("type".into(), type_code_of_val(d));
                set_default(&mut n, d);
            }
            let id = add(&mut g, n);
            fin.insert(p.identifier.clone(), id);
        }
        let mut pending: Vec<(u32, &crate::sbs::FNode)> = Vec::new();
        // the output arity of each `instance` node (its called function's output), so vector assembly below knows
        // how many lanes a call result contributes
        let mut inst_lanes: HashMap<String, usize> = HashMap::new();
        for fnode in &fg.nodes {
            let pos = [fnode.pos[0] * POS_SCALE, fnode.pos[1] * POS_SCALE];
            let data = |name: &str| fnode.datas.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone()).or_else(|| fnode.datas.first().map(|(_, v)| v.clone()));
            let f = fnode.function.as_str();
            let mut n = Node { pos, ..Default::default() };
            match f {
                _ if f.starts_with("const_") => {
                    let v = data(f).unwrap_or(Val::F(vec![0.0]));
                    match &v {
                        Val::B(b) => {
                            n.kind = "f_bool".into();
                            n.nums.insert("v".into(), *b as i32 as f32);
                        }
                        Val::S(s) => {
                            n.kind = "f_string".into();
                            n.strs.insert("v".into(), s.clone());
                        }
                        Val::I(x) => {
                            n.kind = match x.len() {
                                2 => "f_ivec2",
                                3 => "f_ivec3",
                                4 => "f_ivec4",
                                _ => "f_int",
                            }
                            .into();
                            set_default(&mut n, &v);
                        }
                        Val::F(x) => {
                            n.kind = match x.len() {
                                2 => "f_vec2",
                                3 => "f_vec3",
                                4 => "f_vec4",
                                _ => "f_float",
                            }
                            .into();
                            set_default(&mut n, &v);
                        }
                    }
                }
                _ if f.starts_with("get_") => {
                    let var = data(f).map(|v| v.text()).unwrap_or_default();
                    match var.as_str() {
                        "$pos" => n.kind = "f_pos".into(),
                        "$size" => n.kind = "f_size".into(),
                        "$number" if f == "get_float1" => {
                            // the repeat number, read as a float
                            let i = add(&mut g, Node { kind: "f_index".into(), pos: [pos[0] - 120.0, pos[1]], ..Default::default() });
                            n.kind = "f_to_float".into();
                            n.ins.insert("a".into(), i);
                        }
                        "$number" => n.kind = "f_index".into(),
                        "$depth" | "$depthpow2" | "$time" => {
                            // set by pattern programs at every visit (0 elsewhere)
                            n.kind = "f_get".into();
                            n.strs.insert("name".into(), var.clone());
                        }
                        "$randomseed" | "$seed" => n.kind = "f_seed".into(),
                        "$sizelog2" => {
                            let s = add(&mut g, Node { kind: "f_size".into(), pos: [pos[0] - 120.0, pos[1]], ..Default::default() });
                            n.kind = "f_log2".into();
                            n.ins.insert("a".into(), s);
                        }
                        v if v.starts_with('$') => {
                            n.kind = "f_float".into();
                            n.nums.insert("v".into(), 0.0);
                            self.note(Level::Approx, &fc.graph, None, format!("function variable {v} is not available; 0 is used"));
                        }
                        v => {
                            if let Some(&id) = fin.get(v) {
                                // a function's own input: re-route to it
                                n.kind = "f_pin".into();
                                n.ins.insert("a".into(), id);
                            } else if let Some(p) = fc.graph_inputs.iter().find(|p| p.identifier == v) {
                                n.kind = "f_graph_input".into();
                                n.strs.insert("name".into(), san(v));
                                n.nums.insert("type".into(), type_code_of_get(f));
                                if let InputKind::Value { default: Some(d) } = &p.kind {
                                    set_default(&mut n, d);
                                }
                            } else {
                                n.kind = "f_get".into();
                                n.strs.insert("name".into(), v.to_string());
                            }
                        }
                    }
                }
                "set" => {
                    n.kind = "f_set".into();
                    n.strs.insert("name".into(), data("set").map(|v| v.text()).unwrap_or_default());
                }
                "sequence" => n.kind = "f_sequence".into(),
                "pow" => n.kind = "f_pow".into(),
                "while" => {
                    n.kind = "f_while".into();
                    n.nums.insert("max".into(), data("while").map(|v| v.num()).unwrap_or(32.0));
                }
                "ifelse" => n.kind = "f_if".into(),
                "vector2" | "vector3" | "vector4" | "ivector2" | "ivector3" | "ivector4" => n.kind = format!("f_{}", f.replace("vector", "vec")),
                _ if f.starts_with("swizzle") || f.starts_with("iswizzle") => {
                    n.kind = "f_swizzle".into();
                    let idx = data(f).map(|v| (0..v.len()).map(|i| v.at(i) as i32).collect::<Vec<_>>()).unwrap_or_default();
                    let mask: String = idx.iter().map(|i| ['x', 'y', 'z', 'w'][(*i).clamp(0, 3) as usize]).collect();
                    // without explicit components, swizzleN keeps the first N (swizzle2 -> "xy", not a single lane)
                    let fallback = || {
                        let n = f.trim_start_matches("iswizzle").trim_start_matches("swizzle").chars().last().and_then(|c| c.to_digit(10)).unwrap_or(1) as usize;
                        "xyzw".chars().take(n.clamp(1, 4)).collect::<String>()
                    };
                    n.strs.insert("mask".into(), if mask.is_empty() { fallback() } else { mask });
                }
                _ if f.starts_with("toint") => n.kind = "f_to_int".into(),
                _ if f.starts_with("tofloat") => n.kind = "f_to_float".into(),
                "add" | "sub" | "mul" | "div" | "neg" | "mod" | "dot" | "cross" | "and" | "or" | "not" | "eq" | "abs" | "floor" | "ceil" | "cos" | "sin" | "sqrt" | "log" | "log2" | "exp" | "tan" | "min" | "max" | "lerp" => n.kind = format!("f_{f}"),
                "mulscalar" => n.kind = "f_mul".into(),
                "noteq" => n.kind = "f_ne".into(),
                "gt" => n.kind = "f_gt".into(),
                "gteq" => n.kind = "f_ge".into(),
                "lr" => n.kind = "f_lt".into(),
                "lreq" => n.kind = "f_le".into(),
                "pow2" => {
                    // 2 to the power a
                    let two = add(&mut g, Node { kind: "f_float".into(), nums: [("v".to_string(), 2.0)].into(), pos: [pos[0] - 120.0, pos[1] - 40.0], ..Default::default() });
                    n.kind = "f_pow".into();
                    n.ins.insert("a".into(), two);
                }
                "atan2" => n.kind = "f_atan2".into(),
                "cartesian" => n.kind = "f_vec2".into(),
                "rand" => n.kind = "f_random".into(),
                "samplelum" | "samplecol" => {
                    n.kind = if f == "samplelum" { "f_sample_gray" } else { "f_sample_color" }.into();
                    let idx = data(f).map(|v| v.at(0) as i32).unwrap_or(0).max(0);
                    n.strs.insert("slot".into(), mc_in_slot(idx as usize));
                }
                "passthrough" => n.kind = "f_pin".into(),
                "instance" => {
                    let path = data("instance").map(|v| v.text()).unwrap_or_default();
                    let (fpath, dep) = sbs::split_path(&path);
                    // a Substance Designer library function (Functions/Math/clamp, ...) implemented natively, so the
                    // material renders without Designer installed; the file is only consulted for ones not built in
                    if let Some(name) = crate::libfuncs::resolve_path(&fpath) {
                        self.ensure_libfuncs();
                        if let Some(l) = self.fn_out_lanes(name) {
                            inst_lanes.insert(fnode.uid.clone(), l);
                        }
                        n.kind = "f_call".into();
                        n.strs.insert("fn".into(), name.into());
                        let id = add(&mut g, n);
                        ids.insert(fnode.uid.clone(), id);
                        pending.push((id, fnode));
                        continue;
                    }
                    let called = match dep {
                        None => self.function_name(fc.src, &fpath),
                        // a function in another package of the project; a Substance Designer library package
                        // (sbs://...) is not consulted — its functions are built in (see crate::libfuncs)
                        Some(uid) => match self.dependency(fc.src, &uid) {
                            Ok(Some(s)) => self.function_name(s, &fpath),
                            _ => None,
                        },
                    };
                    match called {
                        Some(name) => {
                            if let Some(l) = self.fn_out_lanes(&name) {
                                inst_lanes.insert(fnode.uid.clone(), l);
                            }
                            n.kind = "f_call".into();
                            n.strs.insert("fn".into(), name);
                        }
                        None => {
                            n.kind = "f_float".into();
                            n.nums.insert("v".into(), 0.0);
                            self.note(
                                Level::Missing,
                                &fc.graph,
                                None,
                                format!("function '{fpath}' is not available; 0 is used (Substance Designer library functions are built in; this one is not among them)"),
                            );
                        }
                    }
                }
                other => {
                    n.kind = "f_float".into();
                    n.nums.insert("v".into(), 0.0);
                    self.note(Level::Missing, &fc.graph, None, format!("function node '{other}' is not supported; 0 is used"));
                }
            }
            let id = add(&mut g, n);
            ids.insert(fnode.uid.clone(), id);
            pending.push((id, fnode));
        }
        // links
        let mut extra: Vec<Node> = Vec::new();
        for (id, fnode) in &pending {
            let f = fnode.function.as_str();
            let link = |c: &Conn| ids.get(&c.from).copied();
            match f {
                "vector2" | "vector3" | "vector4" | "ivector2" | "ivector3" | "ivector4" => {
                    // components arrive as a first part and a last part, each one or more lanes wide
                    let parts: Vec<(u32, usize)> = ["componentsin", "componentslast"].iter().filter_map(|s| fnode.conns.iter().find(|c| c.slot.eq_ignore_ascii_case(s))).filter_map(|c| link(c).map(|i| (i, lanes(fg, &c.from, 0, &inst_lanes)))).collect();
                    let mut lane_srcs: Vec<(u32, Option<usize>)> = Vec::new();
                    for (src, n) in parts {
                        if n <= 1 {
                            lane_srcs.push((src, None));
                        } else {
                            for k in 0..n {
                                lane_srcs.push((src, Some(k)));
                            }
                        }
                    }
                    let slots = ["x", "y", "z", "w"];
                    for (k, (src, lane)) in lane_srcs.into_iter().enumerate().take(4) {
                        let from = match lane {
                            None => src,
                            Some(l) => {
                                let mut s = Node { kind: "f_swizzle".into(), ..Default::default() };
                                s.strs.insert("mask".into(), slots[l].to_string());
                                s.ins.insert("a".into(), src);
                                extra.push(s);
                                u32::MAX - (extra.len() as u32 - 1)
                            }
                        };
                        if let Some(n) = g.nodes.get_mut(id) {
                            n.ins.insert(slots[k].into(), from);
                        }
                    }
                }
                "cartesian" => {
                    // (rho cos theta, rho sin theta)
                    let rho = fnode.conns.iter().find(|c| c.slot == "rho").and_then(link);
                    let theta = fnode.conns.iter().find(|c| c.slot == "theta").and_then(link);
                    if let (Some(r), Some(t)) = (rho, theta) {
                        for (slot, trig) in [("x", "f_cos"), ("y", "f_sin")] {
                            let mut tn = Node { kind: trig.into(), ..Default::default() };
                            tn.ins.insert("a".into(), t);
                            extra.push(tn);
                            let tid = u32::MAX - (extra.len() as u32 - 1);
                            let mut m = Node { kind: "f_mul".into(), ..Default::default() };
                            m.ins.insert("a".into(), r);
                            m.ins.insert("b".into(), tid);
                            extra.push(m);
                            let mid = u32::MAX - (extra.len() as u32 - 1);
                            if let Some(n) = g.nodes.get_mut(id) {
                                n.ins.insert(slot.into(), mid);
                            }
                        }
                    }
                }
                "atan2" => {
                    // the file passes one float2 (x, y); ours takes y then x
                    if let Some(v) = fnode.conns.first().and_then(link) {
                        for (slot, mask) in [("a", "y"), ("b", "x")] {
                            let mut s = Node { kind: "f_swizzle".into(), ..Default::default() };
                            s.strs.insert("mask".into(), mask.into());
                            s.ins.insert("a".into(), v);
                            extra.push(s);
                            let sid = u32::MAX - (extra.len() as u32 - 1);
                            if let Some(n) = g.nodes.get_mut(id) {
                                n.ins.insert(slot.into(), sid);
                            }
                        }
                    }
                }
                "rand" => {
                    // a random number between 0 and a
                    if let Some(a) = fnode.conns.first().and_then(link) {
                        if let Some(n) = g.nodes.get_mut(id) {
                            n.ins.insert("b".into(), a);
                        }
                    }
                }
                "pow2" => {
                    if let Some(a) = fnode.conns.first().and_then(link) {
                        if let Some(n) = g.nodes.get_mut(id) {
                            n.ins.insert("b".into(), a);
                        }
                    }
                }
                "instance" => {
                    // arguments are named after the called function's inputs; ours fill a b c d in input order
                    let called = g.nodes.get(id).and_then(|n| n.strs.get("fn")).cloned();
                    let order: Vec<String> = called
                        .and_then(|c| self.lib.get(&c))
                        .map(|cg| {
                            let mut v: Vec<(u32, String)> = cg.nodes.iter().filter(|(_, n)| n.kind == "f_input").map(|(i, n)| (*i, n.strs.get("name").cloned().unwrap_or_default())).collect();
                            v.sort();
                            v.into_iter().map(|x| x.1).collect()
                        })
                        .unwrap_or_default();
                    for c in &fnode.conns {
                        if let (Some(pos), Some(from)) = (order.iter().position(|o| *o == c.slot), link(c)) {
                            if let Some(slot) = crate::func::CALL_SLOTS.get(pos) {
                                if let Some(n) = g.nodes.get_mut(id) {
                                    n.ins.insert((*slot).into(), from);
                                }
                            } else {
                                self.note(Level::Approx, &fc.graph, None, format!("input '{}' of a function call is beyond the {} a call passes on; its default is used", c.slot, crate::func::CALL_SLOTS.len()));
                            }
                        }
                    }
                }
                _ => {
                    for c in &fnode.conns {
                        let Some(from) = link(c) else { continue };
                        if let Some(slot) = fn_slot(f, &c.slot) {
                            if let Some(n) = g.nodes.get_mut(id) {
                                n.ins.insert(slot.into(), from);
                            }
                        }
                    }
                }
            }
        }
        // helper nodes were given provisional ids from the top of the range; give them real ones
        if !extra.is_empty() {
            let base = next;
            let fix = |x: u32| if x > u32::MAX - 100_000 { base + (u32::MAX - x) } else { x };
            for n in g.nodes.values_mut() {
                for v in n.ins.values_mut() {
                    *v = fix(*v);
                }
            }
            for (k, mut n) in extra.into_iter().enumerate() {
                for v in n.ins.values_mut() {
                    *v = fix(*v);
                }
                n.pos = [0.0, -200.0 - k as f32 * 40.0];
                g.nodes.insert(base + k as u32, n);
            }
        }
        let out_id = g.nodes.keys().max().copied().unwrap_or(0) + 1;
        let mut out = Node { kind: "f_out".into(), ..Default::default() };
        match ids.get(&fg.root) {
            Some(&r) => {
                let p = g.nodes.get(&r).map(|n| n.pos).unwrap_or([0.0, 0.0]);
                out.pos = [p[0] + 160.0, p[1]];
                out.ins.insert("in".into(), r);
            }
            None => {
                let zero = out_id + 1;
                g.nodes.insert(zero, Node { kind: "f_float".into(), nums: [("v".to_string(), 0.0)].into(), ..Default::default() });
                out.ins.insert("in".into(), zero);
                if !fg.nodes.is_empty() {
                    self.note(Level::Approx, &fc.graph, None, "a function has no result node; it returns 0");
                }
            }
        }
        g.nodes.insert(out_id, out);
        g
    }
}

/// Node ids of a converted graph: the uid of the file's node when it is a free number, else the next free one.
#[derive(Default)]
struct IdAlloc {
    taken: HashSet<u32>,
    next: u32,
}

impl IdAlloc {
    fn claim(&mut self, uid: &str) -> u32 {
        match uid.trim().parse::<u32>() {
            Ok(i) if i != 0 && i < u32::MAX && self.taken.insert(i) => i,
            _ => self.fresh(),
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

fn set_default(n: &mut Node, v: &Val) {
    if v.len() <= 1 {
        n.nums.insert("v".into(), v.num());
    } else {
        for (i, k) in ["x", "y", "z", "w"].iter().enumerate().take(v.len()) {
            n.nums.insert((*k).into(), v.at(i));
        }
    }
}

/// Our input slot for image input `i` of a pixel processor / function: in, in2, in3, in4.
fn mc_in_slot(i: usize) -> String {
    if i == 0 {
        "in".into()
    } else {
        format!("in{}", i + 1)
    }
}

// ------------------------------------------------------------------------------------------------ graphs

/// Node positions in the file are a little tighter than our cards.
const POS_SCALE: f32 = 1.35;

impl Cx {
    fn convert_graph(&mut self, src: usize, path: &str, name: &str) -> Graph {
        let Some(sg) = self.srcs[src].pkg.graph(path).cloned() else { return Graph::default() };
        let mut out = Graph::default();
        out.meta.insert("identifier".into(), name.to_string());
        for (k, v) in [("label", &sg.label), ("description", &sg.description), ("category", &sg.category)] {
            if let Some(v) = v {
                out.meta.insert(k.into(), v.replace(['"', '#', '\n'], " "));
            }
        }
        // graph size, format and seed
        if let Some(v) = cval(&sg.base_params, "outputsize") {
            let rel = find(&sg.base_params, "outputsize").and_then(|p| p.relative_to.clone()).unwrap_or_default();
            if rel == "0" || rel.is_empty() {
                let (w, h) = (v.at(0).clamp(4.0, 13.0) as u32, v.at(1).clamp(4.0, 13.0) as u32);
                out.size = 1 << w;
                if h != w {
                    out.height = 1 << h;
                }
            }
        }
        if let Some(v) = cval(&sg.base_params, "format") {
            if v.num() >= 1.0 {
                out.meta.insert("output_format".into(), "16".into());
            }
        }
        if let Some(v) = cval(&sg.base_params, "randomseed") {
            out.seed_offset = v.num().max(0.0) as u32;
        }
        // image inputs: declared as images, or read by an input node
        let bridged: HashSet<String> = sg.nodes.iter().filter_map(|n| if let Impl::InputBridge { entry, .. } = &n.imp { Some(entry.clone()) } else { None }).collect();
        let is_image = |p: &ParamInput| matches!(p.kind, InputKind::Image { .. }) || bridged.contains(&p.uid);
        let image_inputs: Vec<ParamInput> = sg.inputs.iter().filter(|p| is_image(p)).cloned().collect();
        let value_inputs: Vec<ParamInput> = sg.inputs.iter().filter(|p| !is_image(p)).cloned().collect();
        if image_inputs.len() > crate::graph::MAX_SUB_INPUTS {
            self.note(Level::Approx, name, None, format!("{} image inputs; graphs used as nodes take up to {}, the rest are left unconnected when this graph is used elsewhere", image_inputs.len(), crate::graph::MAX_SUB_INPUTS));
        }
        let min_x = sg.nodes.iter().map(|n| n.pos[0]).fold(f32::INFINITY, f32::min);
        let min_y = sg.nodes.iter().map(|n| n.pos[1]).fold(f32::INFINITY, f32::min);
        let (ox, oy) = (if min_x.is_finite() { min_x } else { 0.0 }, if min_y.is_finite() { min_y } else { 0.0 });
        // nodes keep their uid as id (so an edited package converts to the same ids again); the nodes added here
        // (input values, fills) take ids no node of the file uses
        let mut ids = IdAlloc::default();
        let node_ids: Vec<u32> = sg.nodes.iter().map(|n| ids.claim(&n.uid)).collect();
        // value inputs become Input Value nodes in a column on the left
        for (i, p) in value_inputs.iter().enumerate() {
            let mut n = Node { kind: "input_value".into(), pos: [(ox - 260.0) * POS_SCALE, (oy + i as f32 * 70.0) * POS_SCALE], ..Default::default() };
            n.strs.insert("name".into(), san(&p.identifier));
            if let Some(l) = &p.label {
                n.strs.insert("label".into(), l.replace(['"', '#', '\n'], " "));
            }
            if let Some(g) = &p.group {
                n.strs.insert("group".into(), g.replace(['"', '#', '\n'], " "));
            }
            if let InputKind::Value { default: Some(d) } = &p.kind {
                n.nums.insert("type".into(), type_code_of_val(d));
                set_default(&mut n, d);
                if let Val::S(s) = d {
                    n.strs.insert("v".into(), s.replace(['"', '#', '\n'], " "));
                }
            }
            if let Some(m) = p.min {
                n.nums.insert("min".into(), m);
            }
            if let Some(m) = p.max {
                n.nums.insert("max".into(), m);
            }
            out.nodes.insert(ids.fresh(), n);
        }
        let fc = FnCx { src, graph_inputs: value_inputs.clone(), fn_inputs: Vec::new(), graph: name.to_string() };
        // nodes
        let mut uid_to_id: HashMap<String, u32> = HashMap::new();
        let mut convs: Vec<(u32, Conv, &sbs::Node)> = Vec::new();
        for (n, &nid) in sg.nodes.iter().zip(node_ids.iter()) {
            let mut c = self.convert_node(src, &sg, n, &image_inputs, name);
            c.node.pos = [n.pos[0] * POS_SCALE, n.pos[1] * POS_SCALE];
            if let Some(t) = &n.name {
                c.node.strs.insert("title".into(), t.replace(['"', '#', '\n'], " "));
            }
            if n.disabled {
                c.notes.push((Level::Info, "the node was disabled in the file".into()));
            }
            uid_to_id.insert(n.uid.clone(), nid);
            convs.push((nid, c, n));
        }
        // links
        let outputs_of: HashMap<u32, HashMap<String, String>> = convs.iter().map(|(id, c, _)| (*id, c.outputs.clone())).collect();
        for (_, c, n) in convs.iter_mut() {
            let mut free: Vec<String> = c.order.iter().filter(|s| !c.slots.iter().any(|(_, m)| m == *s)).cloned().collect();
            for conn in &n.conns {
                let Some(&from) = uid_to_id.get(&conn.from) else {
                    c.notes.push((Level::Approx, format!("input '{}' comes from a node that is not in the graph", conn.slot)));
                    continue;
                };
                let slot = match c.slots.iter().find(|(s, _)| s.eq_ignore_ascii_case(&conn.slot)) {
                    Some((_, m)) => m.clone(),
                    None if !free.is_empty() => {
                        let s = free.remove(0);
                        if c.node.kind != "dot" {
                            c.notes.push((Level::Approx, format!("input '{}' was connected to '{s}' (by order)", conn.slot)));
                        }
                        s
                    }
                    None => {
                        c.notes.push((Level::Approx, format!("input '{}' has no matching input here and was left unconnected", conn.slot)));
                        continue;
                    }
                };
                c.node.ins.insert(slot.clone(), from);
                if let Some(o) = &conn.from_output {
                    if let Some(pin) = outputs_of.get(&from).and_then(|m| m.get(o)) {
                        c.node.in_out.insert(slot, pin.clone());
                    }
                }
            }
        }
        // a required input left unconnected reads black, as an empty input does in the file's own editor
        let mut fills: Vec<(u32, Node)> = Vec::new();
        for (_, c, _) in convs.iter_mut() {
            let Some(spec) = crate::graph::spec(&c.node.kind) else { continue };
            for slot in spec.req {
                if c.node.ins.contains_key(*slot) {
                    continue;
                }
                let mut f = Node { kind: "color".into(), pos: [c.node.pos[0] - 160.0, c.node.pos[1] + 40.0 * fills.len() as f32 % 160.0], ..Default::default() };
                f.nums.insert("gray".into(), 1.0);
                f.nums.insert("v".into(), 0.0);
                f.strs.insert("title".into(), "empty input".into());
                let fid = ids.fresh();
                c.node.ins.insert(slot.to_string(), fid);
                fills.push((fid, f));
            }
        }
        for (id, f) in fills {
            out.nodes.insert(id, f);
        }
        // function-driven parameters and processor bodies become function graphs of the package
        for (id, c, _) in convs.iter_mut() {
            if let Some(fx) = c.fx.take() {
                let prog = self.convert_fx(&fx, &fc, name, *id);
                c.node.strs.insert("program".into(), prog);
            }
            for b in std::mem::take(&mut c.binds) {
                let fname = self.unique(&format!("{name}_n{id}_{}", san(&b.param)));
                let mut fg = self.convert_func(&b.func, &fc);
                wrap_result(&mut fg, b.lane, b.scale, b.offset);
                fg.meta.insert("embedded".into(), "1".into());
                fg.meta.insert("identifier".into(), fname.clone());
                self.lib.insert(fname.clone(), fg);
                c.node.strs.insert(format!("fn.{}", b.param), fname);
            }
            if let Some(body) = c.body.take() {
                let fname = self.unique(&format!("{name}_n{id}_fn"));
                let mut fg = self.convert_func(&body, &fc);
                fg.meta.insert("embedded".into(), "1".into());
                fg.meta.insert("identifier".into(), fname.clone());
                self.lib.insert(fname.clone(), fg);
                c.node.strs.insert("fn".into(), fname);
            }
        }
        for (id, mut c, _) in convs {
            if c.level == Level::Approx && !c.notes.iter().any(|(l, _)| *l == Level::Approx) {
                c.notes.push((Level::Approx, format!("values are converted to the units of MaterialCraft's {} node; check the result", crate::graph::label(&c.node.kind))));
            }
            self.rep.nodes += 1;
            match c.level {
                Level::Info => self.rep.exact += 1,
                Level::Approx => self.rep.approx += 1,
                Level::Missing => self.rep.missing += 1,
            }
            for (lv, t) in c.notes {
                self.note(lv, name, Some(id), t);
            }
            out.nodes.insert(id, c.node);
        }
        // comments and frames
        for o in &sg.gui {
            if !o.kind.eq_ignore_ascii_case("COMMENT") && !o.kind.eq_ignore_ascii_case("FRAME") {
                continue;
            }
            let text = match &o.title {
                Some(t) if !t.is_empty() && !o.text.is_empty() => format!("{t}: {}", o.text),
                Some(t) if !t.is_empty() => t.clone(),
                _ => o.text.clone(),
            };
            let size = o.size.map(|s| [s[0] * POS_SCALE, s[1] * POS_SCALE]).unwrap_or([220.0, 60.0]);
            out.comments.push(Comment { text: text.replace(['#', '\n', '\r'], " "), pos: [o.pos[0] * POS_SCALE, o.pos[1] * POS_SCALE], size, color: 0 });
        }
        // the format the graph's normal maps are made in, so the 3D view lights them the right way up: what its
        // normal-making nodes use, else the writing program's default (DirectX) when the graph has a normal output
        let flags: Vec<bool> = out.nodes.values().filter(|n| crate::params::pdefs(&n.kind).iter().any(|d| d.name == "directx")).filter_map(|n| n.nums.get("directx").map(|v| *v > 0.5)).collect();
        let has_normal = out.nodes.values().any(|n| n.kind == "output" && n.strs.get("usage").map(|u| u == "normal").unwrap_or(false));
        if flags.iter().any(|d| *d) || (flags.is_empty() && has_normal) {
            out.meta.insert("normal_format".into(), "directx".into());
        } else if !flags.is_empty() {
            out.meta.insert("normal_format".into(), "opengl".into());
        }
        out
    }

    fn convert_node(&mut self, src: usize, sg: &sbs::Graph, n: &sbs::Node, image_inputs: &[ParamInput], gname: &str) -> Conv {
        match &n.imp {
            Impl::InputBridge { entry, .. } => {
                let mut c = Conv::new("input", Level::Info);
                let idx = image_inputs.iter().position(|p| &p.uid == entry);
                let p = image_inputs.iter().find(|p| &p.uid == entry);
                c.num("idx", idx.unwrap_or(0) as f32);
                if let Some(InputKind::Image { gray: true }) = p.map(|p| &p.kind) {
                    c.num("gray", 1.0);
                }
                if let Some(p) = p {
                    c.node.strs.insert("title".into(), p.label.clone().unwrap_or_else(|| p.identifier.clone()).replace(['"', '#', '\n'], " "));
                }
                if idx.is_none() {
                    c.approx("the input this node reads is not declared by the graph");
                }
                c
            }
            Impl::OutputBridge { output } => {
                let mut c = Conv::new("output", Level::Info).slot("inputNodeOutput", "in");
                c.order = vec!["in".into()];
                match sg.outputs.iter().find(|o| &o.uid == output) {
                    Some(o) => {
                        let u = o.usage.as_deref().map(usage).unwrap_or_else(|| usage(&o.identifier));
                        c.node.strs.insert("usage".into(), u.into());
                        c.node.strs.insert("identifier".into(), san(&o.identifier));
                    }
                    None => c.approx("the output this node writes is not declared by the graph"),
                }
                c
            }
            Impl::Filter { filter, params, arrays, fx_graphs } => self.atomic(src, filter, params, arrays, fx_graphs, &n.conns, gname, &n.outputs),
            Impl::Instance { path, params, bridgings } => {
                let target = self.resolve(src, path);
                let mut lib_outs: Option<Vec<(String, String)>> = None;
                let mut c = match target {
                    Target::Graph { src: ts, path: tp } => self.instance(ts, &tp, params),
                    Target::Library(pkg, id) => match self.library(&pkg, &id, params) {
                        Some((c, outs)) => {
                            lib_outs = Some(outs);
                            c
                        }
                        None => {
                            let mut c = placeholder(&n.conns, format!("library node '{id}' has no MaterialCraft equivalent yet"));
                            c.node.strs.insert("title".into(), id.replace(['"', '#', '\n'], " "));
                            c
                        }
                    },
                    Target::Missing(why) => placeholder(&n.conns, why),
                };
                // the node's outputs, by uid, named after the graph outputs they bridge to
                match lib_outs {
                    Some(outs) => {
                        // a mapped node: bridged outputs read the matching output of our node, else its first one
                        for (uid, ident) in bridgings {
                            if let Some((_, mc)) = outs.iter().find(|(s, _)| s.eq_ignore_ascii_case(ident)) {
                                c.outputs.insert(uid.clone(), mc.clone());
                            }
                        }
                    }
                    None if c.node.kind == "subgraph" => {
                        for (uid, ident) in bridgings {
                            c.outputs.insert(uid.clone(), san(ident));
                        }
                    }
                    None => {}
                }
                size_and_format(&mut c, params);
                c
            }
            Impl::Unknown(k) => placeholder(&n.conns, format!("unknown node kind '{k}'")),
        }
    }

    /// An instance of a graph that is converted too.
    fn instance(&mut self, ts: usize, tp: &str, params: &[Param]) -> Conv {
        let Some(name) = self.graph_name(ts, tp) else {
            return placeholder(&[], format!("graph '{tp}' could not be converted"));
        };
        let mut c = Conv::new("subgraph", Level::Info);
        c.node.strs.insert("graph".into(), name.clone());
        let tg = self.srcs[ts].pkg.graph(tp).cloned();
        if let Some(tg) = &tg {
            let bridged: HashSet<String> = tg.nodes.iter().filter_map(|n| if let Impl::InputBridge { entry, .. } = &n.imp { Some(entry.clone()) } else { None }).collect();
            let imgs: Vec<&ParamInput> = tg.inputs.iter().filter(|p| matches!(p.kind, InputKind::Image { .. }) || bridged.contains(&p.uid)).collect();
            for (i, p) in imgs.iter().enumerate() {
                if i < crate::graph::MAX_SUB_INPUTS {
                    c.slots.push((p.identifier.clone(), format!("in{i}")));
                } else {
                    c.notes.push((Level::Approx, format!("image input '{}' (number {}) cannot be connected: graphs used as nodes take up to {}", p.identifier, i + 1, crate::graph::MAX_SUB_INPUTS)));
                }
            }
        }
        // parameters set on the instance write into the Input Value nodes of the converted graph
        let inputs: HashMap<String, u32> = self.lib.get(&name).map(|g| g.nodes.iter().filter(|(_, n)| n.kind == "input_value").filter_map(|(i, n)| Some((n.strs.get("name")?.clone(), *i))).collect()).unwrap_or_default();
        for p in params {
            if p.name.starts_with('$') || matches!(p.name.as_str(), "outputsize" | "format" | "pixelsize" | "pixelratio" | "tiling" | "quality") {
                continue;
            }
            if p.name == "randomseed" {
                if let PValue::Const(v) = &p.value {
                    if v.num() != 0.0 {
                        c.notes.push((Level::Approx, "the instance's random seed is not carried over".into()));
                    }
                }
                continue;
            }
            let Some(&iid) = inputs.get(&san(&p.name)) else {
                c.notes.push((Level::Approx, format!("parameter '{}' is not an input of '{name}'", p.name)));
                continue;
            };
            match &p.value {
                PValue::Const(v) => match v {
                    Val::S(_) => c.notes.push((Level::Approx, format!("text parameter '{}' is not carried over", p.name))),
                    _ if v.len() <= 1 => c.num(&format!("{iid}.v"), v.num()),
                    _ => {
                        for (k, l) in ["x", "y", "z", "w"].iter().enumerate().take(v.len()) {
                            c.num(&format!("{iid}.{l}"), v.at(k));
                        }
                    }
                },
                PValue::Dynamic(f) => {
                    let lanes_n = lanes(f, &f.root, 0, &std::collections::HashMap::new());
                    if lanes_n <= 1 {
                        c.binds.push(Bind { param: format!("{iid}.v"), func: f.clone(), lane: None, scale: 1.0, offset: 0.0 });
                    } else {
                        for (k, l) in ["x", "y", "z", "w"].iter().enumerate().take(lanes_n) {
                            c.binds.push(Bind { param: format!("{iid}.{l}"), func: f.clone(), lane: Some(k), scale: 1.0, offset: 0.0 });
                        }
                    }
                }
            }
        }
        c
    }

    /// A graph from the standard library: our own node of the same kind, with every parameter translated (the
    /// file's values, and the library defaults for the rest).
    fn library(&mut self, package: &str, id: &str, params: &[Param]) -> Option<(Conv, Vec<(String, String)>)> {
        let lib = crate::sbs_lib::find(Some(package), id);
        let (kind, f): (&'static str, fn(&mut crate::sbs_adapt::Ad)) = match crate::sbs_adapt::adapter(package, id) {
            Some(a) => (a.kind, a.f),
            None => match crate::nodes10::kind_for(package, id) {
                // library graphs MaterialCraft implements under their own names (see nodes10)
                Some(k) => (k, crate::nodes10::adapt),
                None => (library_kind(id)?, crate::sbs_adapt::generic),
            },
        };
        let mut ad = crate::sbs_adapt::Ad::new(kind, id, lib, params);
        f(&mut ad);
        Some(ad.finish())
    }

    /// Path of a resource parameter (`pkg:///name?dependency=...`) as a file our nodes can read.
    fn resource_file(&mut self, src: usize, path: &str, gname: &str) -> Option<String> {
        let (rpath, dep) = sbs::split_path(path);
        let rs = match dep {
            None => src,
            Some(uid) => match self.dependency(src, &uid) {
                Ok(None) => src,
                Ok(Some(i)) => i,
                Err(_) => src,
            },
        };
        let r = self.srcs[rs].pkg.resources.iter().find(|r| r.path() == rpath || r.identifier == rpath).cloned();
        let Some(r) = r else {
            self.note(Level::Missing, gname, None, format!("resource '{rpath}' is not in the package"));
            return None;
        };
        if let Some(bytes) = r.embedded {
            let ext = r.format.clone().unwrap_or_else(|| "png".into()).to_ascii_lowercase();
            let key = format!("{}.{}", san(&r.identifier), ext);
            self.res.insert(key.clone(), Arc::new(bytes));
            return Some(key);
        }
        let fp = r.filepath.clone()?;
        let dir = &self.srcs[rs].dir;
        let stem = &self.srcs[rs].stem;
        let dirs = [dir.clone(), dir.join(format!("{stem}.resources")), dir.join("resources")];
        match crate::resources::resolve_in(&fp, &dirs) {
            Some(p) => Some(p.display().to_string()),
            None => {
                let searched = dirs.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ");
                self.note(
                    Level::Missing,
                    gname,
                    None,
                    format!("file '{fp}' of resource '{}' was not found; searched the original path, package folders [{searched}], and application assets folders (assets and assets/panorama beside the executable or in the working directory)", r.identifier),
                );
                Some(fp)
            }
        }
    }

    /// A pattern map: MaterialCraft's Pattern Program Map running the converted pattern graph (see [`Self::convert_fx`]).
    fn fx_node(&mut self, p: &[Param], fx: &[sbs::FxGraph], conns: &[Conn], color_mode: Option<bool>, out_types: &[(String, Option<String>)]) -> Conv {
        let mut c = Conv::new("fx_map", Level::Approx);
        c.slots.push(("background".into(), "background".into()));
        for cn in conns {
            if let Some(rest) = cn.slot.strip_prefix("inputpattern") {
                let idx = rest.trim_start_matches(':').parse::<usize>().unwrap_or(0);
                match FX_IMAGE_SLOTS.get(idx) {
                    Some(mc) => c.slots.push((cn.slot.clone(), (*mc).into())),
                    None => c.approx(format!("image input {} is not used (up to {} are)", idx + 1, FX_IMAGE_SLOTS.len())),
                }
            }
        }
        // colour or grayscale: the setting, else the type of the node's output
        let color = color_mode.unwrap_or_else(|| out_types.first().and_then(|(_, t)| t.as_deref()).map(|t| t != "2").unwrap_or(true));
        c.num("color", if color { 1.0 } else { 0.0 });
        c.num("depth", 32.0);
        c.lin_lane(p, "background", 0.0, "bg", Some(0), 1.0, 0.0);
        c.color(p, "background", [0.0, 0.0, 0.0, 0.0], ["bg_r", "bg_g", "bg_b", ""]);
        c.lin(p, "hurst", 1.0, "roughness", 1.0, 0.0);
        c.lin(p, "opacitymult", 1.0, "opacity", 1.0, 0.0);
        for (k, what) in [("renderroi", "render region"), ("tilingroi", "tiling region")] {
            match find(p, k).map(|x| &x.value) {
                Some(PValue::Const(v)) if (0..4).any(|i| (v.at(i) - [0.0, 1.0, 0.0, 1.0][i]).abs() > 1e-6) => c.approx(format!("the {what} is not used (the whole image is drawn)")),
                Some(PValue::Dynamic(_)) => c.approx(format!("the function-driven {what} is not used")),
                _ => {}
            }
        }
        if fx.len() > 1 {
            c.approx(format!("only the first of {} pattern graphs is used", fx.len()));
        }
        c.fx = Some(fx.first().cloned().unwrap_or_default());
        c.notes.push((Level::Approx, "drawn by MaterialCraft's quad-tree engine: the layout follows the file, random numbers differ".into()));
        c
    }

    /// Convert the pattern graph of a pattern map into a pattern program (Quad Cell, Repeat and Select Branch nodes).
    fn convert_fx(&mut self, fx: &sbs::FxGraph, fc: &FnCx, gname: &str, nid: u32) -> String {
        let pname = self.unique(&format!("{gname}_n{nid}_pattern"));
        let mut g = Graph::default();
        g.meta.insert("kind".into(), "fxmap".into());
        g.meta.insert("embedded".into(), "1".into());
        g.meta.insert("identifier".into(), pname.clone());
        let ids: HashMap<&str, u32> = fx.nodes.iter().enumerate().map(|(i, n)| (n.uid.as_str(), i as u32 + 1)).collect();
        let (mut disabled, mut lost) = (0usize, 0usize);
        for (i, fnode) in fx.nodes.iter().enumerate() {
            let id = i as u32 + 1;
            let kind = match fnode.ty.as_str() {
                "paramset" => "fxm_quad",
                "addnode" => "fxm_repeat",
                "markov2" => "fxm_select",
                other => {
                    self.note(Level::Missing, gname, Some(nid), format!("pattern-graph node type '{other}' is not known; it draws nothing"));
                    g.nodes.insert(id, Node { kind: "fxm_quad".into(), pos: [fnode.pos[0] * POS_SCALE, fnode.pos[1] * POS_SCALE], ..Default::default() });
                    continue;
                }
            };
            let mut node = Node { kind: kind.into(), pos: [fnode.pos[0] * POS_SCALE, fnode.pos[1] * POS_SCALE], ..Default::default() };
            for prm in &fnode.params {
                self.fx_param(&mut node, prm, fc, &pname, id, gname, nid);
            }
            if fnode.disabled {
                // a switched-off node draws nothing and runs nothing below it
                disabled += 1;
                node.nums.insert("pattern".into(), 0.0);
                node.strs.retain(|k, _| !k.starts_with("fn."));
            } else {
                for cn in &fnode.conns {
                    let slot = match (kind, cn.slot.as_str()) {
                        ("fxm_quad", sl) => sl.strip_prefix("output").and_then(|d| d.parse::<usize>().ok()).and_then(|d| ["q0", "q1", "q2", "q3"].get(d).copied()),
                        ("fxm_repeat", "output0") => Some("once"),
                        ("fxm_repeat", "output1") => Some("each"),
                        ("fxm_select", "output0") => Some("a"),
                        ("fxm_select", "output1") => Some("b"),
                        _ => None,
                    };
                    match (slot, ids.get(cn.from.as_str())) {
                        (Some(sl), Some(&cid)) => {
                            node.ins.insert(sl.into(), cid);
                        }
                        _ => lost += 1,
                    }
                }
            }
            g.nodes.insert(id, node);
        }
        // the root: the one the graph names, else the node nothing leads to
        let root = ids.get(fx.root.as_str()).copied().or_else(|| {
            let led: HashSet<&str> = fx.nodes.iter().flat_map(|n| n.conns.iter().map(|c| c.from.as_str())).collect();
            fx.nodes.iter().position(|n| !led.contains(n.uid.as_str())).map(|i| i as u32 + 1)
        });
        let mut out = Node { kind: "fxm_out".into(), ..Default::default() };
        match root {
            Some(r) => {
                let rp = g.nodes.get(&r).map(|n| n.pos).unwrap_or([0.0, 0.0]);
                out.pos = [rp[0], rp[1] - 140.0];
                out.ins.insert("in".into(), r);
            }
            None if !fx.nodes.is_empty() => self.note(Level::Approx, gname, Some(nid), "the pattern graph has no start node; it draws nothing"),
            None => self.note(Level::Approx, gname, Some(nid), "the pattern map has no pattern graph; it shows its background"),
        }
        g.nodes.insert(fx.nodes.len() as u32 + 1, out);
        if disabled > 0 {
            self.note(Level::Info, gname, Some(nid), format!("{disabled} switched-off pattern-graph node(s) draw nothing"));
        }
        if lost > 0 {
            self.note(Level::Approx, gname, Some(nid), format!("{lost} pattern-graph connection(s) could not be followed"));
        }
        self.lib.insert(pname.clone(), g);
        pname
    }

    /// One setting of a pattern-graph node.
    #[allow(clippy::too_many_arguments)]
    fn fx_param(&mut self, node: &mut Node, prm: &Param, fc: &FnCx, pname: &str, id: u32, gname: &str, nid: u32) {
        let kind = node.kind.clone();
        // (our setting or the base name of a group of settings, the settings of the group)
        let (base, lanes): (&str, &[&str]) = match (kind.as_str(), prm.name.as_str()) {
            ("fxm_quad", "opacity") => ("color", &["r", "g", "b", "a"]),
            ("fxm_quad", "branchoffset") => ("branch", &["branch_x", "branch_y"]),
            ("fxm_quad", "frameoffset") => ("offset", &["off_x", "off_y"]),
            ("fxm_quad", "patternsize") => ("size", &["size_x", "size_y"]),
            ("fxm_quad", "patterntype") => ("pattern", &[]),
            ("fxm_quad", "patternrotation") => ("rotation", &[]),
            ("fxm_quad", "patternsuppl") => ("variation", &[]),
            ("fxm_quad", "blendingmode") => ("blend", &[]),
            ("fxm_quad", "imageindex") => ("image", &[]),
            ("fxm_quad", "imagefiltering") => ("filtering", &[]),
            ("fxm_quad", "imagepremul") => ("premul", &[]),
            ("fxm_repeat", "numberadded") => ("count", &[]),
            ("fxm_select", "switch") => ("select", &[]),
            (_, "randomseed") => ("seed", &[]),
            (_, "randominherited") => ("inherit", &[]),
            (_, other) => {
                self.note(Level::Approx, gname, Some(nid), format!("pattern-graph setting '{other}' is not used"));
                return;
            }
        };
        match &prm.value {
            PValue::Const(v) => {
                if base == "color" {
                    if v.len() == 1 {
                        for k in ["value", "r", "g", "b"] {
                            node.nums.insert(k.into(), v.num());
                        }
                    } else {
                        node.nums.insert("value".into(), v.at(0));
                        for (i, k) in lanes.iter().enumerate() {
                            node.nums.insert((*k).into(), if i < v.len() { v.at(i) } else { 1.0 });
                        }
                    }
                } else if lanes.is_empty() {
                    node.nums.insert(base.into(), v.num());
                } else {
                    for (i, k) in lanes.iter().enumerate() {
                        node.nums.insert((*k).into(), v.at(i));
                    }
                }
            }
            PValue::Dynamic(f) => {
                let fname = self.unique(&format!("{pname}_{id}_{base}"));
                let mut fg = self.convert_func(f, fc);
                fg.meta.insert("embedded".into(), "1".into());
                fg.meta.insert("identifier".into(), fname.clone());
                self.lib.insert(fname.clone(), fg);
                node.strs.insert(format!("fn.{base}"), fname);
            }
        }
    }

    /// The built-in nodes.
    #[allow(clippy::too_many_arguments)]
    fn atomic(&mut self, src: usize, filter: &str, p: &[Param], arrays: &[sbs::ParamArray], fx_graphs: &[sbs::FxGraph], conns: &[Conn], gname: &str, out_types: &[(String, Option<String>)]) -> Conv {
        let int = |name: &str, d: i32| cval(p, name).map(|v| v.num() as i32).unwrap_or(d);
        let color_mode = cval(p, "colorswitch").map(|v| v.num() > 0.5);
        let mut c = match filter {
            "passthrough" | "dot" => {
                let mut c = Conv::new("dot", Level::Info).slot("input", "in").slot("input1", "in");
                c.order = vec!["in".into()];
                c
            }
            "blend" => {
                let mut c = Conv::new("blend", Level::Info).slot("source", "fg").slot("destination", "bg").slot("opacity", "mask");
                c.lin(p, "opacitymult", 1.0, "opacity", 1.0, 0.0);
                let mode = int("blendingmode", 0);
                match blend_mode(mode) {
                    Some(m) => c.num("mode", m),
                    None => c.approx(format!("blending mode {mode} is not known; Copy is used")),
                }
                if dynv(p, "blendingmode").is_some() {
                    c.approx("a function-driven blending mode is not carried over");
                }
                c.num("alpha_mode", int("colorblending", 0).clamp(0, 3) as f32);
                // crop rectangle: left right top bottom
                c.lin_lane(p, "maskrectangle", 0.0, "crop_x0", Some(0), 1.0, 0.0);
                c.lin_lane(p, "maskrectangle", 1.0, "crop_x1", Some(1), 1.0, 0.0);
                c.lin_lane(p, "maskrectangle", 0.0, "crop_y0", Some(2), 1.0, 0.0);
                c.lin_lane(p, "maskrectangle", 1.0, "crop_y1", Some(3), 1.0, 0.0);
                c
            }
            "blur" => {
                let mut c = Conv::new("blur", Level::Approx).slot("input1", "in");
                // the format's intensity counts the reach of the blur in pixels of a 256-pixel image; ours is the standard
                // deviation of a Gaussian (the reach is about three of them) in pixels of a 512-pixel image
                c.lin(p, "intensity", 10.0, "radius", 2.0 / 3.0, 0.0);
                c.notes.push((Level::Approx, "blur strength is converted (intensity / 1.5 as a Gaussian deviation); compare with a reference render".into()));
                c
            }
            "sharpen" => {
                let mut c = Conv::new("sharpen", Level::Approx).slot("input1", "in");
                c.lin(p, "intensity", 1.0, "amount", 1.0, 0.0);
                c.num("radius", 1.0);
                c
            }
            "dirmotionblur" => {
                let mut c = Conv::new("directional_blur", Level::Approx).slot("input1", "in");
                c.lin(p, "mblurangle", 0.0, "angle", 1.0, 0.0);
                c.lin(p, "intensity", 10.0, "length", 1.0 / 256.0, 0.0);
                c.notes.push((Level::Approx, "blur length is converted from pixels at 256".into()));
                c
            }
            "directionalwarp" => {
                let mut c = Conv::new("directional_warp", Level::Approx).slot("input1", "in").slot("inputintensity", "warp");
                c.lin(p, "warpangle", 0.0, "angle", 1.0, 0.0);
                c.lin(p, "intensity", 10.0, "intensity", 1.0 / 256.0, 0.0);
                c.num("offset", 0.0);
                c
            }
            "warp" => {
                let mut c = Conv::new("warp", Level::Approx).slot("input1", "in").slot("inputgradient", "warp");
                c.lin(p, "intensity", 1.0, "intensity", 1.0 / 20.0, 0.0);
                c
            }
            "emboss" => {
                // the input lit by the height map's slopes, in the highlight and shadow colours
                let mut c = Conv::new("emboss", Level::Approx).slot("input1", "in").slot("inputgradient", "height");
                c.num("mode", 1.0);
                c.lin(p, "lightangle", 0.0, "angle", 1.0, 0.0);
                c.lin(p, "intensity", 5.0, "intensity", 1.0, 0.0);
                c.color(p, "highlightcolor", [1.0, 1.0, 1.0, 1.0], ["hl_r", "hl_g", "hl_b", ""]);
                c.color(p, "shadowcolor", [0.0, 0.0, 0.0, 1.0], ["sh_r", "sh_g", "sh_b", ""]);
                c.notes.push((Level::Approx, "the emboss lighting follows Substance Designer's model (highlight and shadow colours on the height map's slopes); its strength is not yet matched against a reference render".into()));
                c
            }
            "distance" => {
                let mut c = Conv::new("distance", Level::Approx).slot("mask", "in");
                c.lin(p, "distance", 10.0, "distance", 1.0 / 256.0, 0.0);
                c.num("mode", 1.0);
                if find(p, "source").is_some() || int("combinedistance", 1) == 0 {
                    c.approx("propagating a source image is not supported; the distance field is produced");
                }
                c
            }
            "levels" => {
                let mut c = Conv::new("levels", Level::Info).slot("input1", "in");
                c.lin(p, "levelinlow", 0.0, "low", 1.0, 0.0);
                c.lin(p, "levelinhigh", 1.0, "high", 1.0, 0.0);
                c.lin(p, "leveloutlow", 0.0, "out_low", 1.0, 0.0);
                c.lin(p, "levelouthigh", 1.0, "out_high", 1.0, 0.0);
                // the mid point m maps to 0.5: out = in^(1/gamma) with gamma = ln m / ln 0.5
                let mid = cval(p, "levelinmid").map(|v| v.num()).unwrap_or(0.5).clamp(0.001, 0.999);
                c.num("gamma", mid.ln() / 0.5f32.ln());
                if dynv(p, "levelinmid").is_some() {
                    c.approx("a function-driven mid level is not carried over");
                }
                for k in ["levelinlow", "levelinhigh", "levelinmid", "leveloutlow", "levelouthigh"] {
                    if let Some(v) = cval(p, k) {
                        if v.len() >= 3 && (v.at(0) != v.at(1) || v.at(1) != v.at(2)) {
                            c.approx("per-channel levels: the red channel's values are used for all channels");
                            break;
                        }
                    }
                }
                c
            }
            "hsl" => {
                let mut c = Conv::new("hsl", Level::Approx).slot("input1", "in");
                c.lin(p, "hue", 0.5, "hue", 1.0, -0.5);
                c.lin(p, "saturation", 0.5, "sat", 2.0, 0.0);
                c.lin(p, "luminosity", 0.5, "val", 2.0, 0.0);
                c.notes.push((Level::Approx, "saturation and lightness are converted to multipliers".into()));
                c
            }
            "normal" => {
                let mut c = Conv::new("normal", Level::Approx).slot("input1", "in");
                c.lin(p, "intensity", 1.0, "strength", 2.0, 0.0);
                // the flag inverts the Y slope relative to the program's own (DirectX) convention: set = OpenGL
                c.num("directx", if int("inversedy", 0) != 0 { 0.0 } else { 1.0 });
                c.num("alpha_from_input", if int("input2alpha", 0) != 0 { 1.0 } else { 0.0 });
                c
            }
            "uniform" => {
                let mut c = Conv::new("color", Level::Info);
                if color_mode == Some(false) {
                    c.num("gray", 1.0);
                    c.lin_lane(p, "outputcolor", 0.0, "v", Some(0), 1.0, 0.0);
                    c.num("a", 1.0);
                } else {
                    c.color(p, "outputcolor", [0.0, 0.0, 0.0, 1.0], ["r", "g", "b", "a"]);
                }
                c
            }
            "shuffle" => {
                let mut c = Conv::new("channel_shuffle", Level::Info).slot("input1", "in").slot("input2", "in2");
                for (sd, mc, d) in [("channelred", "src_r", 0), ("channelgreen", "src_g", 1), ("channelblue", "src_b", 2), ("channelalpha", "src_a", 3)] {
                    c.num(mc, int(sd, d).clamp(0, 7) as f32);
                }
                c
            }
            "grayscaleconversion" => {
                let mut c = Conv::new("grayscale_conversion", Level::Info).slot("input1", "in");
                c.color(p, "channelsweights", [0.33, 0.33, 0.33, 0.0], ["r", "g", "b", ""]);
                if cval(p, "channelsweights").map(|v| v.at(3) != 0.0).unwrap_or(false) {
                    c.approx("the alpha weight is not used");
                }
                c
            }
            "gradient" => {
                let mut c = Conv::new("colorize", Level::Info).slot("input1", "in");
                c.num("addressing", if int("addressingrepeat", 0) != 0 { 1.0 } else { 0.0 });
                let keys: Vec<(f32, [f32; 4])> = arrays
                    .iter()
                    .filter(|a| a.name.to_ascii_lowercase().starts_with("gradient"))
                    .flat_map(|a| a.cells.iter())
                    .filter_map(|cell| {
                        let pos = cval(cell, "position").map(|v| v.num())?;
                        let v = cval(cell, "value")?;
                        let col = if v.len() == 1 { [v.num(), v.num(), v.num(), 1.0] } else { [v.at(0), v.at(1), v.at(2), if v.len() > 3 { v.at(3) } else { 1.0 }] };
                        Some((pos, col))
                    })
                    .collect();
                let mut keys = keys;
                keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                if keys.is_empty() {
                    keys = vec![(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 1.0])];
                }
                if keys.len() > 16 {
                    c.approx(format!("{} gradient keys; the first 16 are used", keys.len()));
                }
                for (i, (pos, col)) in keys.iter().take(16).enumerate() {
                    c.num(&format!("p{i}"), *pos);
                    for (k, ch) in ["r", "g", "b", "a"].iter().enumerate() {
                        c.num(&format!("{ch}{i}"), col[k]);
                    }
                }
                c
            }
            "curve" => {
                let mut c = Conv::new("curve", Level::Info).slot("input1", "in");
                for a in arrays {
                    let an = a.name.to_ascii_lowercase();
                    let key = if an.contains("red") {
                        "points_r"
                    } else if an.contains("green") {
                        "points_g"
                    } else if an.contains("blue") {
                        "points_b"
                    } else if an.contains("alpha") {
                        "points_a"
                    } else {
                        "points"
                    };
                    // each point keeps its Bezier handles (absolute positions) so the segments bend as in the file
                    let mut pts: Vec<(f32, f32, Option<[f32; 4]>)> = a
                        .cells
                        .iter()
                        .filter_map(|cell| {
                            let v = cval(cell, "position").filter(|v| v.len() >= 2)?;
                            let (l, r) = (cval(cell, "left").filter(|v| v.len() >= 2), cval(cell, "right").filter(|v| v.len() >= 2));
                            let hd = match (l, r) {
                                (Some(l), Some(r)) => Some([l.at(0), l.at(1), r.at(0), r.at(1)]),
                                _ => None,
                            };
                            Some((v.at(0).clamp(0.0, 1.0), v.at(1).clamp(0.0, 1.0), hd))
                        })
                        .collect();
                    pts.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
                    if pts.len() >= 2 {
                        let text = pts
                            .iter()
                            .map(|(x, y, hd)| match hd {
                                Some(h) => format!("{x}:{y}:{}:{}:{}:{}", h[0], h[1], h[2], h[3]),
                                None => format!("{x}:{y}"),
                            })
                            .collect::<Vec<_>>()
                            .join(",");
                        c.node.strs.insert(key.into(), text);
                    }
                }
                c
            }
            "transformation" => {
                let mut c = Conv::new("transform", Level::Info).slot("input1", "in");
                // the stored matrix maps output to input coordinates; ours is given as the forward transform
                let m = cval(p, "matrix22").cloned().unwrap_or(Val::F(vec![1.0, 0.0, 0.0, 1.0]));
                let (a, b, cc, d) = (m.at(0), m.at(1), m.at(2), m.at(3));
                let det = a * d - b * cc;
                if det.abs() < 1e-8 {
                    c.approx("the transform matrix is degenerate; it is left as identity");
                } else {
                    let (fa, fb, fc_, fd) = (d / det, -b / det, -cc / det, a / det);
                    let sx = (fa * fa + fc_ * fc_).sqrt();
                    let rot = fc_.atan2(fa);
                    let sy = (fa * fd - fb * fc_) / sx.max(1e-8);
                    c.num("scale_x", sx);
                    c.num("scale_y", sy.abs());
                    c.num("rotation", (rot / std::f32::consts::TAU).rem_euclid(1.0));
                    let skew = (fa * fb + fc_ * fd).abs() > 1e-3 * sx * sy.abs();
                    if skew {
                        c.approx("the transform has skew, which is not carried over");
                    }
                }
                c.lin_lane(p, "offset", 0.0, "offset_x", Some(0), 1.0, 0.0);
                c.lin_lane(p, "offset", 0.0, "offset_y", Some(1), 1.0, 0.0);
                if dynv(p, "matrix22").is_some() {
                    c.approx("a function-driven transform matrix is not carried over");
                }
                c
            }
            "bitmap" => {
                let mut c = Conv::new("bitmap", Level::Info);
                if color_mode == Some(false) {
                    c.num("gray", 1.0);
                }
                if let Some(path) = cval(p, "bitmapresourcepath").map(|v| v.text()) {
                    if let Some(f) = self.resource_file(src, &path, gname) {
                        c.node.strs.insert("file".into(), f);
                    }
                }
                c
            }
            "svg" => {
                let mut c = Conv::new("svg", Level::Approx).slot("background", "bg");
                if let Some(path) = cval(p, "vectorresourcepath").map(|v| v.text()) {
                    if let Some(f) = self.resource_file(src, &path, gname) {
                        c.node.strs.insert("file".into(), f);
                    }
                }
                c.num("fit", 1.0);
                c
            }
            "text" => {
                let mut c = Conv::new("text", Level::Info);
                if let Some(t) = cval(p, "text") {
                    c.node.strs.insert("text".into(), t.text().replace(['"', '#', '\n'], " "));
                }
                c.lin(p, "fontsize", 0.25, "size", 1.0, 0.0);
                c.lin_lane(p, "position", 0.0, "x", Some(0), 1.0, 0.0);
                c.lin_lane(p, "position", 0.0, "y", Some(1), 1.0, 0.0);
                c.num("align", int("alignment", 1).clamp(0, 2) as f32);
                if color_mode != Some(false) {
                    c.num("use_color", 1.0);
                    c.color(p, "frontcolor", [1.0, 1.0, 1.0, 1.0], ["color_r", "color_g", "color_b", ""]);
                }
                // the font: a font resource of the package, or the family name of an installed font
                if let Some(fd) = cval(p, "fontdata").map(|v| v.text()).filter(|t| !t.trim().is_empty()) {
                    if fd.contains("pkg:") || fd.contains("://") {
                        if let Some(f) = self.resource_file(src, &fd, gname) {
                            c.node.strs.insert("font".into(), f);
                        }
                    } else {
                        let fam = fd.trim().to_string();
                        if crate::font::system_font(&fam).is_none() {
                            c.approx(format!("the font '{fam}' is not installed on this computer; the built-in face is used until it is"));
                        }
                        c.node.strs.insert("font_family".into(), fam);
                    }
                }
                c
            }
            "pixelprocessor" => {
                let mut c = Conv::new("pixel_processor", Level::Info);
                for i in 0..crate::graph::PROCESSOR_INPUTS.len() {
                    let s = mc_in_slot(i);
                    c.slots.push((if i == 0 { "input".into() } else { format!("input:{i}") }, s.clone()));
                    c.order.push(s);
                }
                match dynv(p, "perpixel") {
                    Some(f) => c.body = Some(f.clone()),
                    None => {
                        let col = cval(p, "perpixel").cloned().unwrap_or(Val::F(vec![0.0, 0.0, 0.0, 1.0]));
                        c.node.strs.insert("expr".into(), format!("{}", col.at(0)));
                        c.approx("the per-pixel function is a constant");
                    }
                }
                c
            }
            "valueprocessor" => {
                let mut c = Conv::new("value_processor", Level::Info);
                for i in 0..crate::graph::PROCESSOR_INPUTS.len() {
                    let s = mc_in_slot(i);
                    c.slots.push((if i == 0 { "input".into() } else { format!("input:{i}") }, s.clone()));
                    c.order.push(s);
                }
                match dynv(p, "function") {
                    Some(f) => c.body = Some(f.clone()),
                    None => c.approx("the value processor has no function; it gives 0, like an empty one in the file's own editor"),
                }
                c
            }
            "fxmaps" => self.fx_node(p, fx_graphs, conns, color_mode, out_types),
            "dyngradient" => {
                // the gradient is a line of the second image, read at the source's value
                let mut c = Conv::new("lookup_table", Level::Info).slot("input1", "in").slot("input2", "lut").slot("inputgradient", "lut");
                c.order = vec!["in".into(), "lut".into()];
                c.num("mode", 1.0);
                c.lin(p, "coordinate", 0.0, "row", 1.0, 0.0);
                c.lin(p, "uvselector", 0.0, "vertical", 1.0, 0.0);
                c.lin(p, "addressingrepeat", 0.0, "repeat", 1.0, 0.0);
                c
            }
            other => placeholder(conns, format!("built-in node '{other}' is not converted yet")),
        };
        // the base seed of a node
        if let Some(v) = cval(p, "randomseed") {
            if crate::params::pdefs(&c.node.kind).iter().any(|d| d.name == "seed") {
                c.num("seed", v.num().max(0.0));
            }
        }
        size_and_format(&mut c, p);
        c
    }
}

/// A node's own output size and format (relative to its input or to the graph, or absolute) become our node's
/// Output Size and Output Format settings.
pub(crate) fn size_and_format(c: &mut Conv, p: &[Param]) {
    if let Some(par) = find(p, "outputsize") {
        match &par.value {
            PValue::Const(v) => {
                let (w, h) = (v.at(0).round(), if v.len() > 1 { v.at(1).round() } else { v.at(0).round() });
                match par.relative_to.as_deref().unwrap_or("1") {
                    "0" => {
                        c.num("osize_mode", 2.0);
                        c.num("osize_abs", w.clamp(4.0, 13.0));
                        if h != w {
                            c.num("osize_abs_h", h.clamp(4.0, 13.0));
                        }
                    }
                    rel => {
                        c.num("osize_mode", if rel == "2" { 0.0 } else { 1.0 });
                        c.num("osize_rel", w.clamp(-12.0, 12.0));
                        if h != w {
                            c.notes.push((Level::Approx, "a different height offset of the output size is not used (the width's is)".into()));
                        }
                    }
                }
            }
            PValue::Dynamic(f) => {
                let rel = par.relative_to.as_deref().unwrap_or("1");
                let has_result = !f.root.is_empty() && f.nodes.iter().any(|n| n.uid == f.root);
                if rel == "0" {
                    c.num("osize_mode", 2.0);
                    c.num("osize_abs", 9.0);
                } else {
                    c.num("osize_mode", if rel == "2" { 0.0 } else { 1.0 });
                    c.num("osize_rel", 0.0);
                }
                if has_result {
                    // the function's result (the width's power-of-two offset, or exponent) sets the size when the node runs
                    c.binds.push(Bind { param: if rel == "0" { "osize_abs".into() } else { "osize_rel".into() }, func: f.clone(), lane: Some(0), scale: 1.0, offset: 0.0 });
                } else {
                    c.notes.push((Level::Info, "the output size function has no result node; the default size is used, as in the file's own editor".into()));
                }
            }
        }
    }
    if let Some(par) = find(p, "format") {
        if let (PValue::Const(v), Some("0")) = (&par.value, par.relative_to.as_deref()) {
            // 8 bit, 16 bit, 16 bit float, 32 bit float -> our per-node formats 1..4
            c.num("ofmt", (v.num().round() + 1.0).clamp(1.0, 4.0));
        }
    }
}

/// A node that stands in for one that could not be converted: it passes its first input through (so the graph stays
/// connected), or shows mid grey.
fn placeholder(conns: &[Conn], why: String) -> Conv {
    let mut c = if conns.is_empty() {
        let mut c = Conv::new("color", Level::Missing);
        c.num("gray", 1.0);
        c.num("v", 0.5);
        c
    } else {
        let mut c = Conv::new("dot", Level::Missing);
        c.order = vec!["in".into()];
        c
    };
    c.node.strs.insert("import_note".into(), why.replace(['"', '#', '\n'], " "));
    c.notes.push((Level::Missing, why));
    c
}

/// Pick a lane of the function's result and apply `* scale + offset`.
fn wrap_result(g: &mut Graph, lane: Option<usize>, scale: f32, offset: f32) {
    let Some((&out_id, _)) = g.nodes.iter().find(|(_, n)| n.kind == "f_out") else { return };
    let Some(mut src) = g.nodes.get(&out_id).and_then(|n| n.ins.get("in")).copied() else { return };
    let mut next = g.nodes.keys().max().copied().unwrap_or(0) + 1;
    let pos = g.nodes.get(&out_id).map(|n| n.pos).unwrap_or([0.0, 0.0]);
    let mut push = |g: &mut Graph, n: Node| {
        g.nodes.insert(next, n);
        next += 1;
        next - 1
    };
    if let Some(l) = lane {
        let mut s = Node { kind: "f_swizzle".into(), pos: [pos[0] - 60.0, pos[1] + 60.0], ..Default::default() };
        s.strs.insert("mask".into(), ["x", "y", "z", "w"][l.min(3)].into());
        s.ins.insert("a".into(), src);
        src = push(g, s);
    }
    if (scale - 1.0).abs() > 1e-9 {
        let k = push(g, Node { kind: "f_float".into(), nums: [("v".to_string(), scale)].into(), pos: [pos[0] - 60.0, pos[1] + 120.0], ..Default::default() });
        let mut m = Node { kind: "f_mul".into(), pos: [pos[0] - 30.0, pos[1] + 90.0], ..Default::default() };
        m.ins.insert("a".into(), src);
        m.ins.insert("b".into(), k);
        src = push(g, m);
    }
    if offset.abs() > 1e-9 {
        let k = push(g, Node { kind: "f_float".into(), nums: [("v".to_string(), offset)].into(), pos: [pos[0] - 60.0, pos[1] + 180.0], ..Default::default() });
        let mut a = Node { kind: "f_add".into(), pos: [pos[0] - 30.0, pos[1] + 150.0], ..Default::default() };
        a.ins.insert("a".into(), src);
        a.ins.insert("b".into(), k);
        src = push(g, a);
    }
    if let Some(o) = g.nodes.get_mut(&out_id) {
        o.ins.insert("in".into(), src);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_names_fall_back_to_the_bare_name() {
        assert_eq!(library_kind("tile_generator"), Some("tile_generator"));
        assert_eq!(library_kind("blur_hq_grayscale"), Some("blur_hq"));
        assert_eq!(library_kind("slope_blur_grayscale_2"), Some("slope_blur"));
        assert_eq!(library_kind("perlin_noise"), Some("perlin"));
        assert_eq!(library_kind("clouds_2"), Some("clouds"));
        assert_eq!(library_kind("flood_fill_to_random_grayscale"), Some("flood_fill_random"));
        assert_eq!(library_kind("no_such_node_here"), None);
    }
}
