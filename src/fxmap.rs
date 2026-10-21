// SPDX-License-Identifier: MIT OR Apache-2.0
//! Recursive pattern construction ("pattern programs").
//!
//! A pattern program is a small graph (meta `kind "fxmap"`) of `fxm_*` nodes that is walked from its `fxm_out` node.
//! The walk carries a *cell*: a centre, a size, a rotation, a value and a colour. Nodes either change the cell and
//! run children (`fxm_quadrant`, `fxm_iterate`, `fxm_switch`) or draw into the canvas (`fxm_marker`).
//! The `fx_map` node of a texture graph runs a program and rasterises the markers; the canvas wraps at its edges so
//! the result tiles.
//!
//! A second family of nodes works as a quad tree, the way imported pattern maps do: a `fxm_quad` cell draws one
//! pattern into its own cell and runs up to four children in its quarters (one level deeper), `fxm_repeat` runs a
//! branch a number of times and `fxm_select` picks one of two branches. Their settings can be driven by function
//! graphs (`fn.<setting>`) that run at every visit and read `$depth`, `$depthpow2`, `$number` (the repeat number) and
//! the cell position; variables a function sets are seen further down the same branch.

use crate::func::{FnEnv, Prog, Value};
use crate::graph::{Graph, Node};
use crate::noise::rnd;
use crate::tex::{lum, Tex};
use std::collections::{BTreeMap, HashMap};
use std::f32::consts::TAU;
use std::rc::Rc;

pub const MAX_MARKERS: usize = 500_000;
/// Most node visits of one run (a quad tree visits one node per cell).
pub const MAX_VISITS: usize = 24_000_000;
/// Deepest quad-tree level (cells of 1/65536 of the image).
pub const MAX_LEVEL: u32 = 16;

pub const FXM_KINDS: &[&str] = &["fxm_out", "fxm_marker", "fxm_quadrant", "fxm_iterate", "fxm_switch", "fxm_quad", "fxm_repeat", "fxm_select"];

/// Patterns a quad cell draws (`pattern`); 256 + n draws image input n.
pub const QUAD_PATTERNS: &[&str] = &[
    "None", "Image input", "Square", "Disc", "Paraboloid", "Bell", "Gaussian", "Thorn", "Pyramid", "Brick", "Gradation", "Waves", "Half Bell", "Ridged Bell", "Crescent", "Capsule", "Cone",
];
/// How a quad cell's pattern goes onto the canvas (`blend`).
pub const QUAD_BLENDS: &[&str] = &["Add / subtract (0.5 = none)", "Max", "Alpha blend"];
/// Sampling of image patterns (`filtering`).
pub const QUAD_FILTERS: &[&str] = &["Smooth", "Smooth (no averaging)", "Nearest"];

/// Settings set together by one function (a float2 or a colour).
const VEC_PARAMS: &[(&str, &[&str])] = &[("branch", &["branch_x", "branch_y"]), ("offset", &["off_x", "off_y"]), ("size", &["size_x", "size_y"]), ("color", &["r", "g", "b", "a"])];
/// Order in which a node's functions run: the seed first (the others draw random numbers from it).
const BIND_ORDER: &[&str] = &[
    "seed", "count", "select", "color", "value", "r", "g", "b", "a", "branch", "branch_x", "branch_y", "pattern", "image", "offset", "off_x", "off_y", "size", "size_x", "size_y", "rotation", "variation", "blend",
];
const QUAD_SLOTS: [&str; 4] = ["q0", "q1", "q2", "q3"];

pub fn is_program(g: &Graph) -> bool {
    g.meta.get("kind").map(|k| k == "fxmap").unwrap_or(false)
}

#[derive(Clone, Copy)]
struct Cell {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rot: f32,
    val: f32,
    col: [f32; 3],
}

/// Settings of one run.
pub struct Run<'a> {
    pub depth: u32,
    pub seed: u32,
    /// 0 max, 1 add, 2 over (alpha blend), 3 min, 4 multiply
    pub blend: u32,
    pub color: bool,
    /// image inputs (markers with shape 3, quad cells with the image pattern)
    pub patterns: &'a [Option<&'a Tex>],
    /// the package's function graphs, for function-driven settings
    pub lib: Option<&'a BTreeMap<String, Graph>>,
    /// values of the Input Value nodes of the calling graph
    pub graph_inputs: Option<&'a BTreeMap<String, Value>>,
    /// strength of each quad-tree level relative to the one above (1 = all levels equal)
    pub roughness: f32,
    /// strength of every quad-cell pattern
    pub opacity: f32,
    /// starting image instead of the background colour
    pub background: Option<&'a Tex>,
}

impl<'a> Run<'a> {
    /// A run with the defaults of the newer settings.
    pub fn new(depth: u32, seed: u32, blend: u32, color: bool, patterns: &'a [Option<&'a Tex>]) -> Run<'a> {
        Run { depth, seed, blend, color, patterns, lib: None, graph_inputs: None, roughness: 1.0, opacity: 1.0, background: None }
    }
}

/// A function that drives one setting of a node.
struct Binding {
    param: String,
    prog: Prog,
    args: BTreeMap<String, Value>,
}

/// What a quad-tree branch carries besides the cell.
#[derive(Clone)]
struct Ex {
    level: u32,
    branch: [f32; 2],
    number: f32,
    seed: u32,
    vars: Rc<Vec<(String, Value)>>,
}

/// One pattern of a quad cell.
struct Stamp<'t> {
    pattern: u32,
    image: Option<&'t Tex>,
    cx: f32,
    cy: f32,
    sx: f32,
    sy: f32,
    rot: f32,
    var: f32,
    col: [f32; 4],
    value: f32,
    blend: u32,
    k: f32,
    nearest: bool,
    premul: bool,
}

struct Walk<'a> {
    g: &'a Graph,
    run: &'a Run<'a>,
    binds: &'a HashMap<u32, Vec<Binding>>,
    canvas: Tex,
    markers: usize,
    visits: usize,
    counter: u32,
    over: bool,
    err: Option<String>,
}

fn mix(a: u32, b: u32) -> u32 {
    let mut x = a ^ b.wrapping_mul(0x9E37_79B9).wrapping_add(0x7F4A_7C15);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^ (x >> 16)
}

fn wrap01(x: f32) -> f32 {
    let r = x - x.floor();
    if r.is_finite() { r } else { 0.0 }
}

fn set_var(vars: &mut Vec<(String, Value)>, k: &str, v: Value) {
    match vars.iter_mut().find(|(n, _)| n == k) {
        Some(slot) => slot.1 = v,
        None => vars.push((k.to_string(), v)),
    }
}

/// The lanes of a function result.
fn lanes(v: &Value) -> Vec<f32> {
    match v {
        Value::Num(n) => n.v[..n.lanes.max(1) as usize].iter().map(|x| *x as f32).collect(),
        Value::Bool(b) => vec![*b as i32 as f32],
        Value::Str(_) => vec![0.0],
    }
}

/// A setting: what a function set at this visit, else the node's own value.
fn get(node: &Node, over: &[(String, f32)], k: &str, d: f32) -> f32 {
    over.iter().rev().find(|(n, _)| n == k).map(|x| x.1).unwrap_or_else(|| n(node, k, d))
}

fn n(node: &Node, k: &str, d: f32) -> f32 {
    node.nums.get(k).copied().unwrap_or(d)
}

impl<'a> Walk<'a> {
    fn rand(&mut self, salt: u32) -> f32 {
        self.counter = self.counter.wrapping_add(1);
        rnd(self.counter as i32, salt as i32, self.run.seed)
    }

    fn signed(&mut self, salt: u32) -> f32 {
        self.rand(salt) * 2.0 - 1.0
    }

    fn visit(&mut self, id: u32, cell: Cell, depth: u32, ex: &Ex) {
        if self.markers >= MAX_MARKERS || self.visits >= MAX_VISITS || self.over || self.err.is_some() {
            return;
        }
        self.visits += 1;
        let Some(node) = self.g.nodes.get(&id) else { return };
        // the quad-tree nodes end by themselves (the program has no cycles, cells stop at MAX_LEVEL)
        if depth > self.run.depth && !matches!(node.kind.as_str(), "fxm_out" | "fxm_quad" | "fxm_repeat" | "fxm_select") {
            return;
        }
        match node.kind.as_str() {
            "fxm_out" => {
                if let Some(c) = node.ins.get("in") {
                    self.visit(*c, cell, depth + 1, ex);
                }
            }
            "fxm_quad" => self.quad(id, node, cell, depth, ex),
            "fxm_repeat" => {
                let Some((vals, vars, seed)) = self.drive(id, node, &cell, ex) else { return };
                let count = get(node, &vals, "count", 1.0).round().clamp(0.0, 100_000.0) as u32;
                if let Some(&c) = node.ins.get("once") {
                    let e = Ex { seed: mix(seed, 0), vars: vars.clone(), ..ex.clone() };
                    self.visit(c, cell, depth + 1, &e);
                }
                if let Some(&c) = node.ins.get("each") {
                    for i in 0..count {
                        if self.over || self.err.is_some() || self.visits >= MAX_VISITS {
                            break;
                        }
                        let e = Ex { number: i as f32, seed: mix(seed, i + 1), vars: vars.clone(), ..ex.clone() };
                        self.visit(c, cell, depth + 1, &e);
                    }
                }
            }
            "fxm_select" => {
                let Some((vals, vars, seed)) = self.drive(id, node, &cell, ex) else { return };
                let pick = if get(node, &vals, "select", 0.0) > 0.5 { "b" } else { "a" };
                if let Some(&c) = node.ins.get(pick) {
                    let e = Ex { seed: mix(seed, 1), vars, ..ex.clone() };
                    self.visit(c, cell, depth + 1, &e);
                }
            }
            "fxm_marker" => self.marker(node, cell),
            "fxm_quadrant" => {
                let (sin, cos) = cell.rot.sin_cos();
                let scale = n(node, "scale", 0.5).max(0.0);
                for q in 0..4usize {
                    let Some(child) = node.ins.get(&format!("q{q}")) else { continue };
                    if self.rand(10 + q as u32) > n(node, &format!("p{q}"), 1.0) {
                        continue;
                    }
                    // quadrant centres at (-1/4, -1/4), (1/4, -1/4), (-1/4, 1/4), (1/4, 1/4) of the cell
                    let (qx, qy) = (if q % 2 == 0 { -0.25 } else { 0.25 }, if q < 2 { -0.25 } else { 0.25 });
                    let (ox, oy) = (qx * cell.w + n(node, "off_x", 0.0) * cell.w, qy * cell.h + n(node, "off_y", 0.0) * cell.h);
                    let rx = n(node, "rnd_pos", 0.0) * cell.w * self.signed(20 + q as u32);
                    let ry = n(node, "rnd_pos", 0.0) * cell.h * self.signed(30 + q as u32);
                    let rs = 1.0 + n(node, "rnd_scale", 0.0) * self.signed(40 + q as u32);
                    let rr = n(node, "rnd_rot", 0.0).to_radians() * self.signed(50 + q as u32);
                    let rv = 1.0 + n(node, "rnd_val", 0.0) * self.signed(60 + q as u32);
                    let c = Cell {
                        x: cell.x + (ox + rx) * cos - (oy + ry) * sin,
                        y: cell.y + (ox + rx) * sin + (oy + ry) * cos,
                        w: cell.w * scale * rs,
                        h: cell.h * scale * rs,
                        rot: cell.rot + n(node, &format!("r{q}"), 0.0).to_radians() + rr,
                        val: cell.val * n(node, &format!("v{q}"), 1.0) * rv,
                        col: cell.col,
                    };
                    self.visit(*child, c, depth + 1, ex);
                }
            }
            "fxm_iterate" => {
                let Some(body) = node.ins.get("body").copied() else { return };
                let (gx, gy) = (n(node, "grid_x", 0.0).round() as i32, n(node, "grid_y", 0.0).round() as i32);
                let count = if gx > 0 && gy > 0 { gx * gy } else { n(node, "count", 4.0).round().clamp(0.0, 100_000.0) as i32 };
                let (sin, cos) = cell.rot.sin_cos();
                let mut cur = cell;
                for i in 0..count {
                    if self.markers >= MAX_MARKERS {
                        break;
                    }
                    let mut c = cur;
                    if gx > 0 && gy > 0 {
                        // a grid of equal cells filling the current one
                        let (ix, iy) = (i % gx, i / gx);
                        let stagger = if iy % 2 == 1 { n(node, "stagger", 0.0) } else { 0.0 };
                        let (lx, ly) = (((ix as f32 + 0.5 + stagger) / gx as f32 - 0.5) * cell.w, ((iy as f32 + 0.5) / gy as f32 - 0.5) * cell.h);
                        c.x = cell.x + lx * cos - ly * sin;
                        c.y = cell.y + lx * sin + ly * cos;
                        c.w = cell.w / gx as f32;
                        c.h = cell.h / gy as f32;
                    }
                    let rx = n(node, "rnd_x", 0.0) * cur.w * self.signed(70);
                    let ry = n(node, "rnd_y", 0.0) * cur.h * self.signed(71);
                    c.x += rx * cos - ry * sin;
                    c.y += rx * sin + ry * cos;
                    let rs = 1.0 + n(node, "rnd_scale", 0.0) * self.signed(72);
                    c.w *= rs;
                    c.h *= rs;
                    c.rot += n(node, "rnd_rot", 0.0).to_radians() * self.signed(73);
                    c.val *= 1.0 + n(node, "rnd_val", 0.0) * self.signed(74);
                    self.visit(body, c, depth + 1, ex);
                    // per-iteration change of the running cell
                    let (dx, dy) = (n(node, "dx", 0.0) * cur.w, n(node, "dy", 0.0) * cur.h);
                    let (s2, c2) = cur.rot.sin_cos();
                    cur.x += dx * c2 - dy * s2;
                    cur.y += dx * s2 + dy * c2;
                    let ds = n(node, "dscale", 1.0);
                    cur.w *= ds;
                    cur.h *= ds;
                    cur.rot += n(node, "drot", 0.0).to_radians();
                    cur.val *= n(node, "dval", 1.0);
                }
            }
            "fxm_switch" => {
                let slots = ["a", "b", "c", "d"];
                let live: Vec<usize> = (0..4).filter(|i| node.ins.contains_key(slots[*i])).collect();
                if live.is_empty() {
                    return;
                }
                let pick = if n(node, "mode", 0.0) > 0.5 {
                    // weighted random choice among the connected slots
                    let total: f32 = live.iter().map(|i| n(node, &format!("w{i}"), 1.0).max(0.0)).sum();
                    if total <= 0.0 {
                        return;
                    }
                    let mut r = self.rand(80) * total;
                    let mut chosen = live[live.len() - 1];
                    for i in &live {
                        r -= n(node, &format!("w{i}"), 1.0).max(0.0);
                        if r <= 0.0 {
                            chosen = *i;
                            break;
                        }
                    }
                    chosen
                } else {
                    let want = n(node, "index", 0.0).round().max(0.0) as usize;
                    live[want % live.len()]
                };
                self.visit(node.ins[slots[pick]], cell, depth + 1, ex);
            }
            _ => {}
        }
    }

    /// The seed of a quad-tree node: drawn from the branch's seed, or its own when it does not inherit.
    fn own_seed(&self, node: &Node, parent: u32, own: f32) -> u32 {
        let own = own.max(0.0) as u32;
        if n(node, "inherit", 1.0) > 0.5 {
            mix(parent, own)
        } else {
            mix(self.run.seed ^ 0x5EED_0F0F, own)
        }
    }

    /// Run the functions that drive a node's settings for this visit. Returns the values they set, the variables for
    /// the branch below and the node's seed. `None` when a function failed (the error is kept).
    fn drive(&mut self, id: u32, node: &Node, cell: &Cell, ex: &Ex) -> Option<(Vec<(String, f32)>, Rc<Vec<(String, Value)>>, u32)> {
        let mut seed = self.own_seed(node, ex.seed, n(node, "seed", 0.0));
        let mut over: Vec<(String, f32)> = Vec::new();
        let binds = self.binds;
        let Some(list) = binds.get(&id) else { return Some((over, ex.vars.clone(), seed)) };
        let mut vars = (*ex.vars).clone();
        set_var(&mut vars, "$depth", Value::float(ex.level as f64));
        set_var(&mut vars, "$depthpow2", Value::float((1u64 << ex.level.min(62)) as f64));
        set_var(&mut vars, "$number", Value::float(ex.number as f64));
        let pos = [wrap01(cell.x + ex.branch[0]) as f64, wrap01(cell.y + ex.branch[1]) as f64];
        for b in list {
            let mut env = FnEnv::new([self.canvas.w as f64, self.canvas.h as f64]);
            env.index = ex.number as i64;
            env.seed = seed;
            if let Some(gi) = self.run.graph_inputs {
                env.graph_inputs = gi;
            }
            env.args = b.args.clone();
            let v = match b.prog.run_vars(&env, pos, &mut vars) {
                Ok(v) => v,
                Err(e) => {
                    self.err = Some(format!("node {id}, {}: {e}", b.param));
                    return None;
                }
            };
            let l = lanes(&v);
            match VEC_PARAMS.iter().find(|(k, _)| *k == b.param) {
                Some((k, names)) => {
                    if l.len() == 1 && *k == "color" {
                        // one value: the grey level
                        for name in ["value", "r", "g", "b"] {
                            over.push((name.to_string(), l[0]));
                        }
                    } else {
                        if *k == "color" {
                            over.push(("value".into(), l[0]));
                        }
                        for (i, name) in names.iter().enumerate() {
                            if let Some(x) = l.get(i).or(if l.len() == 1 { l.first() } else { None }) {
                                over.push((name.to_string(), *x));
                            }
                        }
                    }
                }
                None => {
                    let x = l.first().copied().unwrap_or(0.0);
                    if b.param == "seed" {
                        seed = self.own_seed(node, ex.seed, x);
                    }
                    over.push((b.param.clone(), x));
                }
            }
        }
        Some((over, Rc::new(vars), seed))
    }

    /// A quad-tree cell: its pattern, then its four quarters one level deeper.
    fn quad(&mut self, id: u32, node: &Node, cell: Cell, depth: u32, ex: &Ex) {
        if ex.level > MAX_LEVEL {
            return;
        }
        let Some((vals, vars, seed)) = self.drive(id, node, &cell, ex) else { return };
        let p = |k: &str, d: f32| get(node, &vals, k, d);
        let branch = [ex.branch[0] + p("branch_x", 0.0), ex.branch[1] + p("branch_y", 0.0)];
        let mut pattern = p("pattern", 0.0).round() as i64;
        let mut image = p("image", 0.0).round().max(0.0) as usize;
        if pattern >= 256 {
            image = (pattern - 256) as usize;
            pattern = 1;
        }
        if pattern > 0 && pattern < QUAD_PATTERNS.len() as i64 {
            let img = if pattern == 1 { self.run.patterns.get(image).copied().flatten() } else { None };
            if pattern != 1 || img.is_some() {
                let st = Stamp {
                    pattern: pattern as u32,
                    image: img,
                    cx: cell.x + branch[0] + p("off_x", 0.0),
                    cy: cell.y + branch[1] + p("off_y", 0.0),
                    sx: p("size_x", 1.0) * cell.w,
                    sy: p("size_y", 1.0) * cell.h,
                    rot: p("rotation", 0.0),
                    var: p("variation", 0.0),
                    col: [p("r", 1.0), p("g", 1.0), p("b", 1.0), p("a", 1.0)],
                    value: p("value", 1.0),
                    blend: p("blend", 0.0).round().clamp(0.0, 2.0) as u32,
                    k: self.run.opacity * self.run.roughness.powi(ex.level as i32),
                    nearest: p("filtering", 0.0).round() as i32 == 2,
                    premul: p("premul", 0.0) > 0.5,
                };
                self.stamp(&st);
            }
        }
        let child = Ex { level: ex.level + 1, branch, number: ex.number, seed, vars };
        for (q, slot) in QUAD_SLOTS.iter().enumerate() {
            let Some(&c) = node.ins.get(*slot) else { continue };
            let (qx, qy) = (if q % 2 == 0 { -0.25 } else { 0.25 }, if q < 2 { -0.25 } else { 0.25 });
            let cc = Cell { x: cell.x + qx * cell.w, y: cell.y + qy * cell.h, w: cell.w * 0.5, h: cell.h * 0.5, ..cell };
            let e = Ex { seed: mix(seed, q as u32 + 1), ..child.clone() };
            self.visit(c, cc, depth + 1, &e);
        }
    }

    /// Draw one quad-cell pattern; it wraps at the canvas edges.
    fn stamp(&mut self, s: &Stamp) {
        let (cw, ch) = (self.canvas.w as i64, self.canvas.h as i64);
        let (fw, fh) = (cw as f32, ch as f32);
        let (ax, ay) = (s.sx.abs(), s.sy.abs());
        if !(ax > 0.0 && ay > 0.0 && ax.is_finite() && ay.is_finite() && s.cx.is_finite() && s.cy.is_finite()) {
            return;
        }
        let (sn, cs) = (s.rot * TAU).sin_cos();
        let hx = 0.5 * (ax * cs.abs() + ay * sn.abs());
        let hy = 0.5 * (ax * sn.abs() + ay * cs.abs());
        // at most a few canvases wide: a huge pattern covers everything anyway
        let lim_x = (cw as f32 * 1.5).max(4.0);
        let lim_y = (ch as f32 * 1.5).max(4.0);
        let (cx, cy) = (s.cx * fw, s.cy * fh);
        let x0 = (cx - (hx * fw).min(lim_x)).floor() as i64;
        let x1 = (cx + (hx * fw).min(lim_x)).ceil() as i64;
        let y0 = (cy - (hy * fh).min(lim_y)).floor() as i64;
        let y1 = (cy + (hy * fh).min(lim_y)).ceil() as i64;
        let small = (ax * fw).min(ay * fh);
        let ss: i64 = if small < 4.0 { 4 } else if small < 16.0 { 2 } else { 1 };
        let inv = 1.0 / (ss * ss) as f32;
        let alpha = if self.run.color { s.col[3] } else { 1.0 };
        let src = if self.run.color { [s.col[0], s.col[1], s.col[2]] } else { [s.value; 3] };
        for y in y0..y1 {
            for x in x0..x1 {
                // premultiplied pattern colour and coverage, averaged over the sub-samples
                let mut acc = [0.0f32; 4];
                let mut hit = false;
                for j in 0..ss {
                    for i in 0..ss {
                        let u = (x as f32 + (i as f32 + 0.5) / ss as f32) / fw - s.cx;
                        let v = (y as f32 + (j as f32 + 0.5) / ss as f32) / fh - s.cy;
                        let lx = (u * cs + v * sn) / s.sx + 0.5;
                        let ly = (-u * sn + v * cs) / s.sy + 0.5;
                        if !(0.0..1.0).contains(&lx) || !(0.0..1.0).contains(&ly) {
                            continue;
                        }
                        hit = true;
                        match s.image {
                            Some(t) => {
                                let c = if s.nearest {
                                    let (px, py) = (((lx * t.w as f32) as usize).min(t.w - 1), ((ly * t.h as f32) as usize).min(t.h - 1));
                                    t.px[py * t.w + px]
                                } else {
                                    t.sample(lx, ly)
                                };
                                let a = c[3];
                                let m = if s.premul { 1.0 } else { a };
                                acc[0] += c[0] * m;
                                acc[1] += c[1] * m;
                                acc[2] += c[2] * m;
                                acc[3] += a;
                            }
                            None => {
                                let b = crate::scatter::builtin(s.pattern - 1, lx, ly, s.var);
                                acc[0] += b;
                                acc[1] += b;
                                acc[2] += b;
                                acc[3] += b;
                            }
                        }
                    }
                }
                if !hit {
                    continue;
                }
                for a in acc.iter_mut() {
                    *a *= inv;
                }
                if !self.run.color {
                    let l = if s.image.is_some() { lum([acc[0], acc[1], acc[2], 1.0]) } else { acc[0] };
                    acc[0] = l;
                    acc[1] = l;
                    acc[2] = l;
                }
                let dst = &mut self.canvas.px[(y.rem_euclid(ch) * cw + x.rem_euclid(cw)) as usize];
                let a = alpha * s.k;
                for c in 0..3 {
                    dst[c] = match s.blend {
                        1 => dst[c].max(acc[c] * src[c] * a),
                        2 => dst[c] * (1.0 - acc[3] * a) + acc[c] * src[c] * a,
                        _ => dst[c] + acc[c] * (2.0 * src[c] - 1.0) * a,
                    };
                }
                dst[3] = 1.0;
            }
        }
    }

    fn marker(&mut self, node: &Node, cell: Cell) {
        self.markers += 1;
        let (px, py) = (n(node, "x", 0.0), n(node, "y", 0.0));
        let (sin, cos) = cell.rot.sin_cos();
        let rx = n(node, "rnd_pos", 0.0) * self.signed(90);
        let ry = n(node, "rnd_pos", 0.0) * self.signed(91);
        let ox = (px + rx) * cell.w;
        let oy = (py + ry) * cell.h;
        let cx = cell.x + ox * cos - oy * sin;
        let cy = cell.y + ox * sin + oy * cos;
        let rs = 1.0 + n(node, "rnd_scale", 0.0) * self.signed(92);
        let (w, h) = (cell.w * n(node, "w", 1.0) * rs, cell.h * n(node, "h", 1.0) * rs);
        let rot = cell.rot + n(node, "rot", 0.0).to_radians() + n(node, "rnd_rot", 0.0).to_radians() * self.signed(93);
        let val = (cell.val * n(node, "value", 1.0) * (1.0 + n(node, "rnd_val", 0.0) * self.signed(94))).max(0.0);
        let mut col = cell.col;
        let rc = n(node, "rnd_color", 0.0);
        let tint = [n(node, "r", 1.0), n(node, "g", 1.0), n(node, "b", 1.0)];
        for (k, c) in col.iter_mut().enumerate() {
            *c *= tint[k] * (1.0 + rc * self.signed(95 + k as u32));
        }
        let shape = n(node, "shape", 0.0).round() as i32;
        let soft = n(node, "softness", 0.02).clamp(0.0, 1.0);
        let src = n(node, "src", 0.0).round().clamp(0.0, 3.0) as usize;
        let pattern = if shape == 3 { self.run.patterns.get(src).copied().flatten() } else { None };
        let (cw, ch) = (self.canvas.w as i32, self.canvas.h as i32);
        let (pxw, pxh) = (cw as f32, ch as f32);
        // pixel bounding circle of the rotated rectangle
        let rad = 0.5 * (w * w + h * h).sqrt();
        if rad <= 0.0 || !rad.is_finite() {
            return;
        }
        let (x0, x1) = (((cx - rad) * pxw).floor() as i32, ((cx + rad) * pxw).ceil() as i32);
        let (y0, y1) = (((cy - rad) * pxh).floor() as i32, ((cy + rad) * pxh).ceil() as i32);
        if (x1 - x0) > cw * 2 || (y1 - y0) > ch * 2 {
            return;
        }
        let (rs_, rc_) = (-rot).sin_cos();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (u, v) = ((x as f32 + 0.5) / pxw - cx, (y as f32 + 0.5) / pxh - cy);
                let (lx, ly) = ((u * rc_ - v * rs_) / (w * 0.5).max(1e-6), (u * rs_ + v * rc_) / (h * 0.5).max(1e-6));
                let (cover, pat_col) = match shape {
                    1 => (edge(lx.abs().max(ly.abs()), soft), None),
                    2 => (edge(lx.abs() + ly.abs(), soft), None),
                    3 => {
                        if lx.abs() > 1.0 || ly.abs() > 1.0 {
                            (0.0, None)
                        } else if let Some(t) = pattern {
                            let c = t.sample(lx * 0.5 + 0.5, ly * 0.5 + 0.5);
                            (lum(c) * c[3], Some([c[0], c[1], c[2]]))
                        } else {
                            (1.0, None)
                        }
                    }
                    _ => (edge((lx * lx + ly * ly).sqrt(), soft), None),
                };
                if cover <= 0.0 {
                    continue;
                }
                let (wx, wy) = (x.rem_euclid(cw) as usize, y.rem_euclid(ch) as usize);
                let dst = &mut self.canvas.px[wy * cw as usize + wx];
                let src_c = match (self.run.color, pat_col) {
                    (true, Some(p)) => [p[0] * col[0], p[1] * col[1], p[2] * col[2]],
                    (true, None) => [col[0], col[1], col[2]],
                    (false, Some(p)) => {
                        let l = (p[0] + p[1] + p[2]) / 3.0;
                        [l, l, l]
                    }
                    (false, None) => [1.0, 1.0, 1.0],
                };
                for k in 0..3 {
                    let sv = src_c[k] * val;
                    let s = sv * if pat_col.is_some() { 1.0 } else { cover };
                    let c = cover.min(1.0);
                    dst[k] = match self.run.blend {
                        1 => dst[k] + s,
                        2 => dst[k] + (sv - dst[k]) * c,
                        3 => dst[k] + (dst[k].min(sv) - dst[k]) * c,
                        4 => dst[k] * (1.0 + (sv - 1.0) * c),
                        _ => dst[k].max(s),
                    };
                }
                dst[3] = 1.0;
            }
        }
    }
}

/// 1 inside, 0 outside, soft over the outer `soft` of the unit radius.
fn edge(d: f32, soft: f32) -> f32 {
    if d >= 1.0 {
        0.0
    } else if soft <= 1e-4 || d <= 1.0 - soft {
        1.0
    } else {
        let t = (1.0 - d) / soft;
        t * t * (3.0 - 2.0 * t)
    }
}

/// Run `program` on a `w` by `h` canvas filled with `background` (or the run's background image).
pub fn render(program: &Graph, w: usize, h: usize, background: [f32; 3], run: &Run) -> Result<Tex, String> {
    let outs: Vec<u32> = program.nodes.iter().filter(|(_, n)| n.kind == "fxm_out").map(|(i, _)| *i).collect();
    let root = match outs.as_slice() {
        [one] => *one,
        [] => return Err("the pattern program has no Output node".into()),
        _ => return Err("the pattern program has more than one Output node".into()),
    };
    for (id, node) in &program.nodes {
        if !FXM_KINDS.contains(&node.kind.as_str()) {
            return Err(format!("node {id} ({}) is not a pattern-program node", node.kind));
        }
    }
    program.check_cycles().map_err(|e| e.to_string())?;
    // functions that drive the quad-tree nodes, compiled once
    let mut binds: HashMap<u32, Vec<Binding>> = HashMap::new();
    for (id, node) in &program.nodes {
        if !matches!(node.kind.as_str(), "fxm_quad" | "fxm_repeat" | "fxm_select") {
            continue;
        }
        let mut list = Vec::new();
        for (k, fname) in &node.strs {
            let Some(param) = k.strip_prefix("fn.") else { continue };
            if fname.is_empty() {
                continue;
            }
            let lib = run.lib.ok_or_else(|| format!("node {id}: function-driven settings need the package's functions"))?;
            let prog = crate::func::compile_named(lib, fname).map_err(|e| format!("node {id}, {k}: {e}"))?;
            let prefix = format!("fa.{param}.");
            let mut args = BTreeMap::new();
            for (ak, av) in &node.nums {
                if let Some(input) = ak.strip_prefix(&prefix) {
                    let pin = prog.inputs.iter().find(|i| i.0 == input).map(|i| i.1).unwrap_or(crate::graph::Pin::Float);
                    args.insert(input.to_string(), crate::func::coerce(&Value::float(*av as f64), pin));
                }
            }
            list.push(Binding { param: param.to_string(), prog, args });
        }
        list.sort_by_key(|b| BIND_ORDER.iter().position(|o| *o == b.param).unwrap_or(BIND_ORDER.len()));
        if !list.is_empty() {
            binds.insert(*id, list);
        }
    }
    let canvas = match run.background {
        Some(t) if t.w == w && t.h == h => t.clone(),
        Some(t) => {
            let mut c = Tex::solid(w, h, [0.0, 0.0, 0.0, 1.0]);
            for y in 0..h {
                for x in 0..w {
                    c.px[y * w + x] = t.sample((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
                }
            }
            c
        }
        None => Tex::solid(w, h, [background[0], background[1], background[2], 1.0]),
    };
    let mut walk = Walk { g: program, run, binds: &binds, canvas, markers: 0, visits: 0, counter: 0, over: false, err: None };
    let start = Cell { x: 0.5, y: 0.5, w: 1.0, h: 1.0, rot: 0.0, val: 1.0, col: [1.0, 1.0, 1.0] };
    let ex = Ex { level: 0, branch: [0.0, 0.0], number: 0.0, seed: mix(run.seed, 0x51ED), vars: Rc::new(Vec::new()) };
    walk.visit(root, start, 0, &ex);
    if let Some(e) = walk.err {
        return Err(e);
    }
    if walk.markers >= MAX_MARKERS {
        return Err(format!("the pattern program draws more than {MAX_MARKERS} markers"));
    }
    if walk.visits >= MAX_VISITS {
        return Err(format!("the pattern program visits more than {MAX_VISITS} nodes"));
    }
    Ok(walk.canvas)
}

pub const FXM_SPECS: &[crate::graph::Spec] = &[
    crate::graph::Spec { kind: "fxm_out", req: &["in"], opt: &[], doc: "start of a pattern program" },
    crate::graph::Spec { kind: "fxm_marker", req: &[], opt: &[], doc: "draw a shape: shape (0 circle 1 square 2 diamond 3 image) src x y w h rot value r g b softness rnd_pos rnd_scale rnd_rot rnd_val rnd_color" },
    crate::graph::Spec { kind: "fxm_quadrant", req: &[], opt: &["q0", "q1", "q2", "q3"], doc: "split the cell in four: scale off_x off_y p0..p3 (chance) v0..v3 (value) r0..r3 (rotation) rnd_pos rnd_scale rnd_rot rnd_val" },
    crate::graph::Spec { kind: "fxm_iterate", req: &[], opt: &["body"], doc: "repeat body: count dx dy dscale drot dval, or grid_x grid_y stagger; rnd_x rnd_y rnd_scale rnd_rot rnd_val" },
    crate::graph::Spec { kind: "fxm_switch", req: &[], opt: &["a", "b", "c", "d"], doc: "run one branch: mode (0 index, 1 random) index w0..w3" },
    crate::graph::Spec {
        kind: "fxm_quad",
        req: &[],
        opt: &["q0", "q1", "q2", "q3"],
        doc: "quad-tree cell: draws a pattern in its cell, then runs q0..q3 (top-left, top-right, bottom-left, bottom-right) in its quarters one level deeper: pattern (0 none 1 image 2 square 3 disc ... 16 cone) image value r g b a off_x off_y size_x size_y rotation (turns) variation blend (0 add/subtract 1 max 2 alpha) branch_x branch_y (moves this cell and the ones below) seed inherit filtering premul; functions driving a setting read $depth $depthpow2 $number and the cell position",
    },
    crate::graph::Spec { kind: "fxm_repeat", req: &[], opt: &["once", "each"], doc: "runs once, then each count times with $number = 0, 1, ...: count seed inherit" },
    crate::graph::Spec { kind: "fxm_select", req: &[], opt: &["a", "b"], doc: "runs a, or b when select is on: select seed inherit" },
];
