// SPDX-License-Identifier: MIT OR Apache-2.0
//! Interface facts of the standard library graphs that `.sbs` files use as nodes: which inputs each graph has, their
//! value types, default values and slider ranges, and the names of its outputs (`sbs_library.txt`). A file only
//! stores the parameters someone changed, so the importer needs the defaults to know what every other parameter was
//! set to. Nothing else about these graphs (how they are built, their labels or descriptions) is used; see
//! INDEPENDENT_IMPLEMENTATION.md.

use std::collections::HashMap;
use std::sync::OnceLock;

const TABLE: &str = include_str!("sbs_library.txt");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LTy {
    Float(u8),
    Int(u8),
    Bool,
    /// colour image input
    Color,
    /// grayscale image input
    Gray,
}

impl LTy {
    pub fn is_image(self) -> bool {
        matches!(self, LTy::Color | LTy::Gray)
    }
    pub fn lanes(self) -> usize {
        match self {
            LTy::Float(n) | LTy::Int(n) => n as usize,
            LTy::Bool => 1,
            LTy::Color | LTy::Gray => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LInput {
    pub id: &'static str,
    pub ty: LTy,
    /// default value, one entry per lane (empty for images)
    pub def: Vec<f32>,
    /// slider range, when the graph declares one
    pub range: Option<(f32, f32)>,
}

impl LInput {
    pub fn lane(&self, i: usize) -> f32 {
        self.def.get(i).copied().unwrap_or(0.0)
    }
}

#[derive(Debug, Clone)]
pub struct LGraph {
    /// package file name without `.sbs`
    pub package: &'static str,
    pub id: &'static str,
    pub outputs: Vec<&'static str>,
    pub inputs: Vec<LInput>,
}

impl LGraph {
    pub fn input(&self, id: &str) -> Option<&LInput> {
        self.inputs.iter().find(|i| i.id == id).or_else(|| self.inputs.iter().find(|i| i.id.eq_ignore_ascii_case(id)))
    }
    /// image inputs, in the order the graph declares them
    pub fn images(&self) -> impl Iterator<Item = &LInput> {
        self.inputs.iter().filter(|i| i.ty.is_image())
    }
    /// value inputs (parameters)
    pub fn values(&self) -> impl Iterator<Item = &LInput> {
        self.inputs.iter().filter(|i| !i.ty.is_image())
    }
}

struct Table {
    graphs: Vec<LGraph>,
    by_key: HashMap<(String, String), usize>,
    by_id: HashMap<String, usize>,
}

fn parse_ty(t: &str) -> Option<LTy> {
    Some(match t {
        "b" => LTy::Bool,
        "c" => LTy::Color,
        "g" => LTy::Gray,
        _ => {
            let (k, n) = t.split_at(1);
            let n: u8 = n.parse().ok().filter(|n| (1..=4).contains(n))?;
            match k {
                "f" => LTy::Float(n),
                "i" => LTy::Int(n),
                _ => return None,
            }
        }
    })
}

fn parse(text: &'static str) -> Table {
    let mut graphs: Vec<LGraph> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('@') {
            let mut it = rest.split_whitespace();
            let (Some(package), Some(id)) = (it.next(), it.next()) else { continue };
            graphs.push(LGraph { package, id, outputs: Vec::new(), inputs: Vec::new() });
            continue;
        }
        let Some(g) = graphs.last_mut() else { continue };
        if let Some(rest) = line.strip_prefix('>') {
            g.outputs = rest.split_whitespace().collect();
            continue;
        }
        let (main, range) = match line.split_once('~') {
            Some((a, b)) => (a, Some(b)),
            None => (line, None),
        };
        let mut it = main.split_whitespace();
        let (Some(id), Some(ty)) = (it.next(), it.next().and_then(parse_ty)) else { continue };
        let def: Vec<f32> = it.filter_map(|s| s.parse::<f32>().ok()).collect();
        let range = range.and_then(|r| {
            let v: Vec<f32> = r.split_whitespace().filter_map(|s| s.parse().ok()).collect();
            (v.len() == 2).then(|| (v[0], v[1]))
        });
        g.inputs.push(LInput { id, ty, def, range });
    }
    let mut by_key = HashMap::new();
    let mut by_id = HashMap::new();
    for (i, g) in graphs.iter().enumerate() {
        by_key.entry((g.package.to_ascii_lowercase(), g.id.to_ascii_lowercase())).or_insert(i);
        by_id.entry(g.id.to_ascii_lowercase()).or_insert(i);
    }
    Table { graphs, by_key, by_id }
}

fn table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| parse(TABLE))
}

/// Every graph in the table.
pub fn all() -> &'static [LGraph] {
    &table().graphs
}

/// The package stem of a library dependency file name (`sbs://noise_perlin_noise.sbs` -> `noise_perlin_noise`).
pub fn package_stem(filename: &str) -> String {
    let f = filename.trim();
    let f = f.strip_prefix("sbs://").unwrap_or(f);
    let f = f.rsplit(['/', '\\']).next().unwrap_or(f);
    let f = f.strip_suffix(".sbs").or_else(|| f.strip_suffix(".SBS")).unwrap_or(f);
    f.to_string()
}

/// The library graph `id` of `package` (stem, see [`package_stem`]); falls back to the first graph with that
/// identifier in any package.
pub fn find(package: Option<&str>, id: &str) -> Option<&'static LGraph> {
    let t = table();
    let lid = id.to_ascii_lowercase();
    if let Some(p) = package {
        if let Some(&i) = t.by_key.get(&(p.to_ascii_lowercase(), lid.clone())) {
            return Some(&t.graphs[i]);
        }
    }
    t.by_id.get(&lid).map(|&i| &t.graphs[i])
}

const CHOICES: &str = include_str!("sbs_choices.txt");

/// Label shown for a number a setting does not use (choices are listed by number, so gaps need a placeholder).
pub const UNUSED_CHOICE: &str = "(unused)";

struct ChoiceLine {
    package: Option<String>,
    /// graph id (lower case), or a prefix when `glob`
    graph: String,
    glob: bool,
    param: &'static str,
    labels: Vec<&'static str>,
}

fn choice_lines() -> &'static [ChoiceLine] {
    static C: OnceLock<Vec<ChoiceLine>> = OnceLock::new();
    C.get_or_init(|| {
        let mut out = Vec::new();
        for line in CHOICES.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((head, list)) = line.split_once(" = ") else { continue };
            let mut it = head.split_whitespace();
            let (Some(key), Some(param)) = (it.next(), it.next()) else { continue };
            let (package, graph) = match key.split_once('/') {
                Some((p, g)) => (Some(p.to_ascii_lowercase()), g),
                None => (None, key),
            };
            let (graph, glob) = match graph.strip_suffix('*') {
                Some(g) => (g.to_ascii_lowercase(), true),
                None => (graph.to_ascii_lowercase(), false),
            };
            // labels by number: `N:text` sets the number, else the next one
            let mut labels: Vec<&'static str> = Vec::new();
            for part in list.split('|') {
                let part = part.trim();
                let (at, text) = match part.split_once(':').and_then(|(n, t)| n.trim().parse::<usize>().ok().map(|n| (n, t.trim()))) {
                    Some((n, t)) => (n, t),
                    None => (labels.len(), part),
                };
                if at > 64 {
                    continue;
                }
                while labels.len() < at {
                    labels.push(UNUSED_CHOICE);
                }
                if at < labels.len() {
                    labels[at] = text;
                } else {
                    labels.push(text);
                }
            }
            out.push(ChoiceLine { package, graph, glob, param, labels });
        }
        out
    })
}

/// What the numbers of setting `param` of library graph `package/id` mean in MaterialCraft (index = number;
/// [`UNUSED_CHOICE`] marks numbers the setting does not use). Empty when we have no reading for it.
pub fn choices(package: &str, id: &str, param: &str) -> Vec<&'static str> {
    let (p, g) = (package.to_ascii_lowercase(), id.to_ascii_lowercase());
    let hit = |c: &ChoiceLine| c.param == param && if c.glob { g.starts_with(&c.graph) } else { g == c.graph };
    let lines = choice_lines();
    lines
        .iter()
        .find(|c| c.package.as_deref() == Some(p.as_str()) && hit(c))
        .or_else(|| lines.iter().find(|c| c.package.is_none() && hit(c)))
        .map(|c| c.labels.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_choice_line_names_a_numbered_setting() {
        for c in choice_lines() {
            let graphs: Vec<&LGraph> = all()
                .iter()
                .filter(|g| c.package.as_deref().map(|p| p == g.package.to_ascii_lowercase()).unwrap_or(true))
                .filter(|g| {
                    let id = g.id.to_ascii_lowercase();
                    if c.glob {
                        id.starts_with(&c.graph)
                    } else {
                        id == c.graph
                    }
                })
                .collect();
            assert!(!graphs.is_empty(), "no library graph matches '{}'", c.graph);
            let with: Vec<&&LGraph> = graphs.iter().filter(|g| g.inputs.iter().any(|i| i.id == c.param && i.ty == LTy::Int(1))).collect();
            assert!(!with.is_empty(), "no graph matching '{}' has a numbered setting '{}'", c.graph, c.param);
            assert!(c.labels.len() >= 2 && c.labels.iter().all(|l| !l.is_empty()), "{} {}: labels", c.graph, c.param);
            // the default number has a label
            for g in with {
                let d = g.input(c.param).map(|i| i.lane(0) as usize).unwrap_or(0);
                assert!(d < c.labels.len() && c.labels[d] != UNUSED_CHOICE, "{}/{} {}: default {d} has no label", g.package, g.id, c.param);
            }
        }
        assert_eq!(choices("pattern_tile_generator", "tile_generator", "pattern")[5], "Bell");
        assert_eq!(choices("pattern_tile_sampler", "tile_sampler_color", "blending_mode")[2], "Add");
        assert_eq!(choices("slope_blur", "slope_blur_grayscale", "mode")[7], "Blur");
        assert_eq!(choices("slope_blur", "slope_blur_grayscale", "mode")[0], UNUSED_CHOICE);
        assert_eq!(choices("shape", "shape", "Pattern")[1], "Square");
        assert_eq!(choices("pattern_shape", "shape", "Pattern")[1], "Dome");
        assert!(choices("pattern_tile_generator", "tile_generator", "x_amount").is_empty());
    }

    #[test]
    fn the_table_parses() {
        let g = all();
        assert!(g.len() > 600, "{}", g.len());
        for x in g {
            assert!(!x.outputs.is_empty() || x.inputs.is_empty() || x.id.is_empty() || !x.package.is_empty());
            for i in &x.inputs {
                if !i.ty.is_image() {
                    assert_eq!(i.def.len(), i.ty.lanes(), "{} {} {}", x.package, x.id, i.id);
                }
            }
        }
    }

    #[test]
    fn lookups_use_the_package_first() {
        let p = find(Some("noise_perlin_noise"), "perlin_noise").expect("perlin");
        assert_eq!(p.package, "noise_perlin_noise");
        assert!(p.input("scale").is_some());
        assert!(find(None, "PERLIN_NOISE").is_some());
        assert!(find(Some("nope"), "perlin_noise").is_some());
        assert!(find(None, "not_a_library_graph").is_none());
        assert_eq!(package_stem("sbs://pattern_tile_generator.sbs"), "pattern_tile_generator");
    }
}
