// SPDX-License-Identifier: MIT OR Apache-2.0
//! The node types of `.sbs` packages as the editor offers them: the format's built-in filters, function-graph and
//! FX-map nodes (`sbs_atomic.txt`) and the standard library graphs files use as nodes (`sbs_library.txt`, interface
//! facts only). Each type has a MaterialCraft node kind:
//!
//! * `sbs:<filter>` — a built-in filter (`sbs:blend`, `sbs:levels` ...)
//! * `sbs:lib:<package>/<graph>` — an instance of a library graph (`sbs:lib:noise_perlin_noise/perlin_noise`)
//! * `sbs:graph` — an instance of another graph of the package or of another package file
//! * `sbs:input`, `sbs:output` — the nodes that read a graph input and write a graph output
//! * `sbs:fn:<function>` — a node of a function graph (`sbs:fn:add`, `sbs:fn:get_float1` ...)
//! * `sbs:fx:<type>` — a node of an FX-map graph (`sbs:fx:paramset` ...)
//!
//! Parameters keep the format's identifiers and value types; see `sbs_native` for how values are stored on nodes.

use crate::graph::Spec;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

const TABLE: &str = include_str!("sbs_atomic.txt");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fam {
    Texture,
    Function,
    Pattern,
}

impl Fam {
    pub fn name(self) -> &'static str {
        match self {
            Fam::Texture => "texture",
            Fam::Function => "function",
            Fam::Pattern => "pattern",
        }
    }
}

/// Value type of a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PTy {
    F(u8),
    I(u8),
    B,
    S,
    Path,
    /// a float4 colour in colour mode, a float1 grey value in grey mode
    FV,
}

impl PTy {
    pub fn parse(t: &str) -> Option<PTy> {
        Some(match t {
            "b" => PTy::B,
            "s" => PTy::S,
            "path" => PTy::Path,
            "fv" => PTy::FV,
            _ => {
                let (k, n) = t.split_at(1);
                let n: u8 = n.parse().ok().filter(|n| (1..=4).contains(n))?;
                match k {
                    "f" => PTy::F(n),
                    "i" => PTy::I(n),
                    _ => return None,
                }
            }
        })
    }

    /// Short code: f1..f4, i1..i4, b, s, path, fv.
    pub fn code(self) -> String {
        match self {
            PTy::F(n) => format!("f{n}"),
            PTy::I(n) => format!("i{n}"),
            PTy::B => "b".into(),
            PTy::S => "s".into(),
            PTy::Path => "path".into(),
            PTy::FV => "fv".into(),
        }
    }

    /// Number of numeric lanes (0 for text).
    pub fn lanes(self) -> usize {
        match self {
            PTy::F(n) | PTy::I(n) => n as usize,
            PTy::B => 1,
            PTy::FV => 4,
            PTy::S | PTy::Path => 0,
        }
    }

    /// The element a constant of this type is written as (`constantValueFloat4` ...).
    pub fn element(self) -> &'static str {
        match self {
            PTy::F(1) | PTy::FV => "constantValueFloat1",
            PTy::F(2) => "constantValueFloat2",
            PTy::F(3) => "constantValueFloat3",
            PTy::F(_) => "constantValueFloat4",
            PTy::I(1) => "constantValueInt32",
            PTy::I(2) => "constantValueInt2",
            PTy::I(3) => "constantValueInt3",
            PTy::I(_) => "constantValueInt4",
            PTy::B => "constantValueBool",
            PTy::S | PTy::Path => "constantValueString",
        }
    }

    /// The type of a `constantValue*` element.
    pub fn of_element(name: &str) -> Option<PTy> {
        let n = name.to_ascii_lowercase();
        let t = n.strip_prefix("constantvalue")?;
        Some(match t {
            "bool" => PTy::B,
            "string" => PTy::S,
            "float1" | "float" => PTy::F(1),
            "float2" => PTy::F(2),
            "float3" => PTy::F(3),
            "float4" => PTy::F(4),
            "int32" | "int1" | "int" => PTy::I(1),
            "int2" => PTy::I(2),
            "int3" => PTy::I(3),
            "int4" => PTy::I(4),
            _ => return None,
        })
    }

    /// The format's type code for graph inputs and function values.
    pub fn type_code(self) -> u32 {
        match self {
            PTy::B => 4,
            PTy::I(1) => 16,
            PTy::I(2) => 32,
            PTy::I(3) => 64,
            PTy::I(_) => 128,
            PTy::F(1) => 256,
            PTy::F(2) => 512,
            PTy::F(3) => 1024,
            PTy::F(_) | PTy::FV => 2048,
            PTy::S | PTy::Path => 16384,
        }
    }

    pub fn from_type_code(c: u32) -> Option<PTy> {
        Some(match c {
            4 => PTy::B,
            16 => PTy::I(1),
            32 => PTy::I(2),
            64 => PTy::I(3),
            128 => PTy::I(4),
            256 => PTy::F(1),
            512 => PTy::F(2),
            1024 => PTy::F(3),
            2048 => PTy::F(4),
            16384 => PTy::S,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PDef {
    pub id: &'static str,
    pub ty: PTy,
    /// the value a new node starts with, as the file writes it (`1 1 1 1`, `0.5`, text)
    pub def: String,
    pub range: Option<(f32, f32)>,
    /// labels of the codes 0, 1, 2 ... of a drop-down
    pub choices: Vec<&'static str>,
    /// may be relative to the parent or to the input
    pub rel: bool,
}

#[derive(Debug, Clone)]
pub struct IDef {
    pub id: &'static str,
    /// `c` colour or grey image, `g` grey image, or a value type code of a function input (f1, t ...)
    pub kind: &'static str,
    /// can be repeated: `input`, `input:1`, `input:2` ...
    pub many: bool,
}

#[derive(Clone)]
pub struct NodeDef {
    /// MaterialCraft node kind (`sbs:blend` ...)
    pub kind: &'static str,
    /// what the file calls it: the filter, function or FX node type, or the library graph identifier
    pub sd: &'static str,
    pub fam: Fam,
    pub label: &'static str,
    pub category: &'static str,
    pub outputs: Vec<&'static str>,
    pub inputs: Vec<IDef>,
    pub params: Vec<PDef>,
    /// library graphs: (package stem, graph identifier)
    pub library: Option<(&'static str, &'static str)>,
    /// MaterialCraft can render it
    pub renders: bool,
    pub spec: &'static Spec,
}

impl std::fmt::Debug for NodeDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeDef").field("kind", &self.kind).field("inputs", &self.inputs).field("params", &self.params).finish()
    }
}

impl NodeDef {
    pub fn param(&self, id: &str) -> Option<&PDef> {
        self.params.iter().find(|p| p.id == id).or_else(|| if self.fam == Fam::Texture && self.library.is_none() { base().iter().find(|p| p.id == id) } else { None })
    }

    pub fn input(&self, id: &str) -> Option<&IDef> {
        let base = id.split(':').next().unwrap_or(id);
        self.inputs.iter().find(|i| i.id == id || (i.many && i.id == base))
    }
}

pub struct Defs {
    pub list: Vec<NodeDef>,
    by_kind: HashMap<&'static str, usize>,
    base: Vec<PDef>,
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

/// A label from an identifier: `perlin_noise` -> `Perlin Noise`.
pub fn label_of(id: &str) -> String {
    // names written as one word read better spelled out (the wording is MaterialCraft's own)
    let named = match id {
        "addressingrepeat" => "Repeat",
        "alphamult" => "Alpha Multiplier",
        "bitmapresourcepath" => "Bitmap",
        "blendingmode" => "Blending Mode",
        "branchoffset" => "Branch Offset",
        "channelalpha" => "Alpha Channel",
        "channelblue" => "Blue Channel",
        "channelgreen" => "Green Channel",
        "channelred" => "Red Channel",
        "channelsweights" => "Channel Weights",
        "clampinterm" => "Clamp Between Steps",
        "colorblending" => "Alpha Blending",
        "colorswitch" => "Color Mode",
        "combinedistance" => "Combine Distance",
        "fontdata" => "Font",
        "fontsize" => "Font Size",
        "frameoffset" => "Frame Offset",
        "frontcolor" => "Text Color",
        "highlightcolor" => "Highlight Color",
        "imagefiltering" => "Image Filtering",
        "imageindex" => "Image Index",
        "imagepremul" => "Premultiplied Image",
        "input2alpha" => "Alpha From Input",
        "inversedy" => "Normal Format",
        "levelinhigh" => "Level In High",
        "levelinlow" => "Level In Low",
        "levelinmid" => "Level In Mid",
        "levelouthigh" => "Level Out High",
        "leveloutlow" => "Level Out Low",
        "lightangle" => "Light Angle",
        "manualmiplevel" => "Mip Level",
        "maskrectangle" => "Cropping Area",
        "matrix22" => "Transform Matrix",
        "mattecolor" => "Matte Color",
        "mattelevel" => "Matte Level",
        "mblurangle" => "Angle",
        "mipmapmode" => "Mipmap Mode",
        "numberadded" => "Number Added",
        "opacitymult" => "Opacity",
        "outputcolor" => "Color",
        "outputsize" => "Output Size",
        "patternrotation" => "Pattern Rotation",
        "patternsize" => "Pattern Size",
        "patternsuppl" => "Pattern Parameter",
        "patterntype" => "Pattern",
        "perpixel" => "Per Pixel Function",
        "pixelratio" => "Pixel Ratio",
        "pixelsize" => "Pixel Size",
        "randominherited" => "Inherit Random Seed",
        "randomseed" => "Random Seed",
        "renderroi" => "Render Region",
        "resizemethod" => "Resize Method",
        "shadowcolor" => "Shadow Color",
        "Substance_AO_Intensity" => "Detail AO Intensity",
        "Substance_Normal_Intensity" => "Detail Normal Intensity",
        "tilingroi" => "Tiling Region",
        "uvselector" => "Coordinate Axis",
        "vectorresourcepath" => "SVG",
        "warpangle" => "Warp Angle",
        _ => "",
    };
    if !named.is_empty() {
        return named.to_string();
    }
    id.split(['_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_param(line: &str) -> Option<PDef> {
    let line = line.trim();
    let (line, rel) = match line.strip_suffix(" rel") {
        Some(l) => (l, true),
        None => (line, false),
    };
    let (main, choices) = match line.split_once(" = ") {
        Some((a, b)) => (a, b.split('|').map(|s| leak(s.trim())).collect()),
        None => (line, Vec::new()),
    };
    let (main, range) = match main.split_once(" ~ ") {
        Some((a, b)) => {
            let v: Vec<f32> = b.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            (a, (v.len() == 2).then(|| (v[0], v[1])))
        }
        None => (main, None),
    };
    let mut it = main.split_whitespace();
    let id = leak(it.next()?);
    let ty = PTy::parse(it.next()?)?;
    let def = it.collect::<Vec<_>>().join(" ");
    Some(PDef { id, ty, def, range, choices, rel })
}

fn spec_for(kind: &'static str, inputs: &[IDef], doc: &str) -> &'static Spec {
    let opt: Vec<&'static str> = inputs.iter().map(|i| i.id).collect();
    Box::leak(Box::new(Spec { kind, req: &[], opt: Box::leak(opt.into_boxed_slice()), doc: leak(doc) }))
}

fn category_of(fam: Fam, sd: &str) -> &'static str {
    match fam {
        Fam::Function => "Function",
        Fam::Pattern => "FX-Map",
        Fam::Texture => match sd {
            "uniform" | "bitmap" | "svg" | "text" | "fxmaps" | "pixelprocessor" | "valueprocessor" => "Atomic generators",
            "passthrough" => "Utility",
            _ => "Atomic filters",
        },
    }
}

fn build() -> Defs {
    let mut list: Vec<NodeDef> = Vec::new();
    let mut base: Vec<PDef> = Vec::new();
    struct Cur {
        sd: String,
        fam: Fam,
        label: String,
        outputs: Vec<&'static str>,
        inputs: Vec<IDef>,
        params: Vec<PDef>,
    }
    let mut cur: Option<Cur> = None;
    let flush = |c: Option<Cur>, list: &mut Vec<NodeDef>, base: &mut Vec<PDef>| {
        let Some(c) = c else { return };
        if c.sd == "$base" {
            *base = c.params;
            return;
        }
        let kind = leak(&match c.fam {
            Fam::Texture => format!("sbs:{}", c.sd),
            Fam::Function => format!("sbs:fn:{}", c.sd),
            Fam::Pattern => format!("sbs:fx:{}", c.sd),
        });
        let doc = format!("{} ({})", c.label, c.sd);
        let spec = spec_for(kind, &c.inputs, &doc);
        list.push(NodeDef { kind, sd: leak(&c.sd), fam: c.fam, label: leak(&c.label), category: category_of(c.fam, &c.sd), outputs: c.outputs, inputs: c.inputs, params: c.params, library: None, renders: true, spec });
    };
    for raw in TABLE.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('@') {
            flush(cur.take(), &mut list, &mut base);
            let mut it = rest.splitn(3, ' ');
            let sd = it.next().unwrap_or_default().to_string();
            let fam = match it.next().unwrap_or_default() {
                "function" => Fam::Function,
                "pattern" => Fam::Pattern,
                _ => Fam::Texture,
            };
            let label = it.next().unwrap_or(&sd).to_string();
            cur = Some(Cur { sd, fam, label, outputs: Vec::new(), inputs: Vec::new(), params: Vec::new() });
            continue;
        }
        let Some(c) = cur.as_mut() else { continue };
        if let Some(rest) = line.strip_prefix('>') {
            c.outputs = rest.split_whitespace().map(leak).collect();
        } else if let Some(rest) = line.strip_prefix('<') {
            let mut it = rest.split_whitespace();
            let (Some(id), Some(kind)) = (it.next(), it.next()) else { continue };
            c.inputs.push(IDef { id: leak(id), kind: leak(kind), many: it.next() == Some("many") });
        } else if let Some(rest) = line.strip_prefix('-') {
            if let Some(p) = parse_param(rest) {
                c.params.push(p);
            }
        }
    }
    flush(cur.take(), &mut list, &mut base);
    // the library graphs files use as nodes
    for g in crate::sbs_lib::all() {
        let kind = leak(&format!("sbs:lib:{}/{}", g.package, g.id));
        let inputs: Vec<IDef> = g.images().map(|i| IDef { id: i.id, kind: if i.ty == crate::sbs_lib::LTy::Gray { "g" } else { "c" }, many: false }).collect();
        let params: Vec<PDef> = g
            .values()
            .map(|i| {
                let ty = match i.ty {
                    crate::sbs_lib::LTy::Float(n) => PTy::F(n),
                    crate::sbs_lib::LTy::Int(n) => PTy::I(n),
                    _ => PTy::B,
                };
                let def = i.def.iter().map(|v| if ty == PTy::B || matches!(ty, PTy::I(_)) { format!("{}", *v as i64) } else { format!("{v}") }).collect::<Vec<_>>().join(" ");
                PDef { id: i.id, ty, def, range: i.range, choices: if ty == PTy::I(1) { crate::sbs_lib::choices(g.package, g.id, i.id) } else { Vec::new() }, rel: false }
            })
            .collect();
        let label = leak(&label_of(g.id));
        let renders = crate::sbs_import::library_renders(g.package, g.id);
        let category = match crate::sbs_import::library_kind(g.id) {
            Some(k) => crate::graph::category(k),
            None => "Library",
        };
        let spec = spec_for(kind, &inputs, &format!("{label} (library graph {}/{})", g.package, g.id));
        list.push(NodeDef { kind, sd: g.id, fam: Fam::Texture, label, category, outputs: g.outputs.clone(), inputs, params, library: Some((g.package, g.id)), renders, spec });
    }
    // the node kinds that are not types of their own
    for (kind, label, fam, inputs, outputs) in [
        ("sbs:input", "Input", Fam::Texture, vec![], vec!["output"]),
        ("sbs:output", "Output", Fam::Texture, vec![IDef { id: "inputNodeOutput", kind: "c", many: false }], vec![]),
        ("sbs:graph", "Graph", Fam::Texture, vec![], vec![]),
    ] {
        let spec = spec_for(kind, &inputs, label);
        list.push(NodeDef { kind, sd: kind, fam, label, category: "Utility", outputs, inputs, params: Vec::new(), library: None, renders: true, spec });
    }
    let mut by_kind = HashMap::new();
    for (i, d) in list.iter().enumerate() {
        by_kind.entry(d.kind).or_insert(i);
    }
    Defs { list, by_kind, base }
}

pub fn defs() -> &'static Defs {
    static D: OnceLock<Defs> = OnceLock::new();
    D.get_or_init(build)
}

/// The definition of a node kind (`sbs:blend`, `sbs:lib:<package>/<graph>` ...).
pub fn def(kind: &str) -> Option<&'static NodeDef> {
    let d = defs();
    if let Some(&i) = d.by_kind.get(kind) {
        return Some(&d.list[i]);
    }
    // a library graph named without its package
    let rest = kind.strip_prefix("sbs:lib:")?;
    let (pkg, id) = rest.split_once('/').map(|(p, g)| (Some(p), g)).unwrap_or((None, rest));
    let g = crate::sbs_lib::find(pkg, id)?;
    d.by_kind.get(format!("sbs:lib:{}/{}", g.package, g.id).as_str()).map(|&i| &d.list[i])
}

/// The settings every image filter has.
pub fn base() -> &'static [PDef] {
    &defs().base
}

/// Node kind of a built-in filter, function or FX-map node of the file.
pub fn kind_of(fam: Fam, sd: &str) -> String {
    match fam {
        Fam::Texture => format!("sbs:{sd}"),
        Fam::Function => format!("sbs:fn:{sd}"),
        Fam::Pattern => format!("sbs:fx:{sd}"),
    }
}

/// The family of a node kind.
pub fn family(kind: &str) -> Option<Fam> {
    let rest = kind.strip_prefix("sbs:")?;
    Some(if rest.starts_with("fn:") {
        Fam::Function
    } else if rest.starts_with("fx:") {
        Fam::Pattern
    } else {
        Fam::Texture
    })
}

pub fn is_native(kind: &str) -> bool {
    kind.starts_with("sbs:")
}

/// The spec (inputs) of a node kind, for the graph machinery.
pub fn spec(kind: &str) -> Option<&'static Spec> {
    def(kind).map(|d| d.spec)
}

/// Node types a user can add, in the editor's order: built-in filters first, then library graphs MaterialCraft
/// renders.
pub fn palette(fam: Fam) -> Vec<&'static NodeDef> {
    static CACHE: OnceLock<[Vec<&'static NodeDef>; 3]> = std::sync::OnceLock::new();
    let all = CACHE.get_or_init(|| [build_palette(Fam::Texture), build_palette(Fam::Function), build_palette(Fam::Pattern)]);
    all[match fam {
        Fam::Texture => 0,
        Fam::Function => 1,
        Fam::Pattern => 2,
    }]
    .clone()
}

/// One entry per node: a library graph that comes in several packages (an older and a newer version, or a copy
/// inside another graph's package) is offered once, in its fullest version, and never under the name of a
/// built-in node.
fn build_palette(fam: Fam) -> Vec<&'static NodeDef> {
    let mut v: Vec<&'static NodeDef> = defs().list.iter().filter(|d| d.fam == fam && d.renders && !matches!(d.kind, "sbs:graph")).collect();
    let builtin: std::collections::BTreeSet<String> = v.iter().filter(|d| d.library.is_none()).map(|d| d.label.to_ascii_lowercase()).collect();
    let mut best: BTreeMap<String, &'static NodeDef> = BTreeMap::new();
    for d in v.iter().filter(|d| d.library.is_some()) {
        let key = d.label.to_ascii_lowercase();
        if builtin.contains(&key) {
            continue;
        }
        // the fullest version wins: more parameters, then more inputs, then the package named after the graph's
        // family (`noise_` / `pattern_` packages are the newer ones), then the package of the graph's own name, then
        // the later package (`flood_fill_2` after `flood_fill`)
        let rank = |x: &NodeDef| {
            let (pkg, id) = x.library.unwrap_or(("", ""));
            (x.params.len(), x.inputs.len(), pkg != id && pkg.ends_with(id), pkg == id, pkg)
        };
        match best.get(&key) {
            Some(b) if rank(b) >= rank(d) => {}
            _ => {
                best.insert(key, d);
            }
        }
    }
    v.retain(|d| d.library.is_none() || best.get(&d.label.to_ascii_lowercase()).map(|b| std::ptr::eq(*b, *d)).unwrap_or(false));
    v.sort_by_key(|d| (d.library.is_some(), d.category, d.label));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tables_parse() {
        let b = def("sbs:blend").expect("blend");
        assert_eq!(b.inputs.iter().map(|i| i.id).collect::<Vec<_>>(), ["source", "destination", "opacity"]);
        let m = b.param("blendingmode").unwrap();
        assert_eq!(m.ty, PTy::I(1));
        assert_eq!(m.choices[3], "Multiply");
        assert!(b.param("outputsize").map(|p| p.rel).unwrap_or(false));
        assert_eq!(def("sbs:fn:add").map(|d| d.fam), Some(Fam::Function));
        assert_eq!(def("sbs:fx:paramset").map(|d| d.outputs.len()), Some(4));
        assert_eq!(def("sbs:fn:const_float4").and_then(|d| d.param("const_float4")).map(|p| p.def.as_str()), Some("0 0 0 0"));
        let p = def("sbs:lib:noise_perlin_noise/perlin_noise").expect("perlin");
        assert!(p.param("scale").is_some());
        assert!(def("sbs:lib:perlin_noise").is_some());
        assert_eq!(base().len(), 6);
        let n = defs().list.iter().filter(|d| d.library.is_none()).count();
        assert!(n > 100, "{n}");
        for d in &defs().list {
            for p in &d.params {
                if p.ty.lanes() > 0 && !p.def.is_empty() {
                    let lanes = p.def.split_whitespace().count();
                    assert!(lanes == p.ty.lanes() || (p.ty == PTy::FV && lanes == 4), "{} {} '{}'", d.kind, p.id, p.def);
                }
            }
        }
        assert!(palette(Fam::Texture).iter().any(|d| d.kind == "sbs:blend"));
        assert!(palette(Fam::Texture).iter().any(|d| d.library.is_some()));
    }
}

/// The sub-heading a library graph's parameter sits under in the Properties panel (empty: directly under Instance
/// Parameters), following the way the original editor groups them: grunges put everything but the four common
/// controls under "Advanced"; the tilers and splatters use Pattern / Size / Position / Rotation / Color / Mask
/// (Split, Interstice and Shape for Tile Random).
pub fn lib_group(package: &str, graph: &str, id: &str) -> &'static str {
    let i = id.to_ascii_lowercase();
    if package.starts_with("grunge_") || graph.starts_with("grunge_") {
        return if matches!(i.as_str(), "balance" | "contrast" | "invert" | "non_square_expansion") { "" } else { "Advanced" };
    }
    let tiler = package.starts_with("pattern_tile_") || package.starts_with("pattern_splatter") || package.starts_with("shape_splatter") || package.starts_with("tile_") || package.starts_with("splatter") || package.starts_with("pattern_arc");
    if !tiler || graph == "tile_random_2" {
        return "";
    }
    let random = package.starts_with("pattern_tile_random");
    if matches!(i.as_str(), "x_amount" | "y_amount" | "non_square_expansion" | "pattern_amount" | "pattern_amount_random" | "pattern_amount_random_min" | "ring_amount") {
        return "";
    }
    if i.contains("rotation") {
        "Rotation"
    } else if i.contains("mask") {
        "Mask"
    } else if i.contains("color") || i.contains("luminance") || i.contains("blending") || i.contains("opacity") || i.contains("background") || i.contains("reverse_rendering") || i.contains("parametri") {
        "Color"
    } else if random && i.starts_with("split") {
        "Split"
    } else if random && i.starts_with("interstice") {
        "Interstice"
    } else if random && (i.starts_with("shape_") || i.starts_with("size_random")) {
        if i.starts_with("size_random") { "Size" } else { "Shape" }
    } else if i.contains("size") || i.contains("scale") || i.contains("interstice") || i.contains("middle") {
        "Size"
    } else if i.contains("position") || i.contains("offset") || i.contains("radius") || i.contains("spiral") || i.contains("spread") || i.contains("angle_random") || i.contains("connect") {
        "Position"
    } else if i.starts_with("pattern") || i.contains("image_input") || i == "pattern" {
        "Pattern"
    } else {
        ""
    }
}
