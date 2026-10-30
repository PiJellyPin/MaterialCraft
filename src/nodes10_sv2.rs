// SPDX-License-Identifier: MIT OR Apache-2.0
//! Library graphs implemented natively (see `nodes10`): Shape Splatter v2, its Mapper and To Mask nodes, and Grid
//! Atlas. Their interface comes from the public node reference, not from a library file, so the identifiers in
//! `sbs_library.txt` are inferred.
//!
//! The splatter places shapes in a small 3D scene and looks straight down at it: every pixel is a ray tested against
//! the shapes near it, and the highest surface wins. Shapes wrap around the picture, so the result always tiles.
//!
//! Splatter data, MaterialCraft's own layout (close to the documented one, but per pixel instead of one record per
//! shape, because a graph resamples images of another size):
//! * UVW: shape u, v, surface height, and `id + pattern / 256` (ids start at 1; 0 is the background).
//! * Data 1: surface position in object space (-1..1 inside the shape's box); alpha 1 on a shape, -1 where the
//!   background hides it, 0 elsewhere.
//! * Data 2: 3D rotation in turns; alpha: rotation around the normal.
//! * Data 3: pivot in parts of the picture, offset along the normal, and alpha as in UVW.
//! * Data 4: first pixel the picture size; every other pixel: shape type, pattern count (negative for OpenGL normals),
//!   atlas columns, atlas rows.
use crate::func::{compile_named, FnEnv, Prog, Value};
use crate::graph::{Error, Graph, Node, PROCESSOR_INPUTS};
use crate::nodes10::{add3, dec_n, enc_n, g, grey, lerp, lerp4, mul3, norm3, rnd, Cx};
use crate::tex::{lum, Tex};
use std::collections::BTreeMap;
use std::f32::consts::{PI, TAU};
use std::sync::Arc as Rc;

// shape types
const SDF: i32 = 0;
const CUBE: i32 = 1;
const SPHERE: i32 = 2;
const CYLINDER: i32 = 3;
const PLANE: i32 = 4;
const DISC: i32 = 5;
const IMAGE: i32 = 6;
const ATLAS: i32 = 7;
const PATTERN_FN: i32 = 8;

type M3 = [[f32; 3]; 3];

fn mv(m: &M3, v: [f32; 3]) -> [f32; 3] {
    [m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2], m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2], m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2]]
}

/// The transpose (the inverse of a rotation) times `v`.
fn tv(m: &M3, v: [f32; 3]) -> [f32; 3] {
    [m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2], m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2], m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2]]
}

fn mm(a: &M3, b: &M3) -> M3 {
    let mut o = [[0.0; 3]; 3];
    for (r, row) in o.iter_mut().enumerate() {
        for (c, x) in row.iter_mut().enumerate() {
            *x = (0..3).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    o
}

fn rx(t: f32) -> M3 {
    let (s, c) = (t * TAU).sin_cos();
    [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
}

fn ry(t: f32) -> M3 {
    let (s, c) = (t * TAU).sin_cos();
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}

fn rz(t: f32) -> M3 {
    let (s, c) = (t * TAU).sin_cos();
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

/// Rotation from three angles in turns: X first, then Y, then Z.
fn euler(a: [f32; 3]) -> M3 {
    mm(&rz(a[2]), &mm(&ry(a[1]), &rx(a[0])))
}

/// The angles [`euler`] needs to give `m`.
fn to_euler(m: &M3) -> [f32; 3] {
    [m[2][1].atan2(m[2][2]) / TAU, (-m[2][0]).clamp(-1.0, 1.0).asin() / TAU, m[1][0].atan2(m[0][0]) / TAU]
}

/// The rotation taking straight up `amount` of the way to `n`.
fn tilt_to(n: [f32; 3], amount: f32) -> M3 {
    let n = norm3(n);
    let axis = [-n[1], n[0], 0.0];
    let len = (axis[0] * axis[0] + axis[1] * axis[1]).sqrt();
    if len < 1e-6 {
        return euler([0.0; 3]);
    }
    let (x, y) = (axis[0] / len, axis[1] / len);
    let (s, c) = (n[2].clamp(-1.0, 1.0).acos() * amount).sin_cos();
    let k = 1.0 - c;
    [[c + x * x * k, x * y * k, y * s], [x * y * k, c + y * y * k, -x * s], [-y * s, x * s, c]]
}

// ------------------------------------------------------------------------------------------------ functions

/// Where the function graphs of a node live.
#[derive(Clone, Copy, Default)]
pub struct Funcs<'a> {
    lib: Option<&'a BTreeMap<String, Graph>>,
    inputs: Option<&'a BTreeMap<String, Value>>,
}

struct Fun<'a> {
    prog: Prog,
    env: FnEnv<'a>,
}

impl Fun<'_> {
    fn call(&self, pos: [f32; 2], vars: &mut Vec<(String, Value)>) -> [f32; 4] {
        match self.prog.run_vars(&self.env, [pos[0] as f64, pos[1] as f64], vars) {
            Ok(Value::Num(n)) if n.lanes == 1 => [n.v[0] as f32; 4],
            Ok(Value::Num(n)) => [n.v[0] as f32, n.v[1] as f32, n.v[2] as f32, n.v[3] as f32],
            Ok(Value::Bool(b)) => [b as i32 as f32; 4],
            _ => [0.0; 4],
        }
    }
}

/// The function driving parameter `param` of the node, ready to run once per shape or per pixel.
fn fun<'a>(cx: &Cx<'a>, funcs: Funcs<'a>, param: &str, images: Vec<(&'a str, &'a Tex)>) -> Result<Option<Fun<'a>>, Error> {
    let (Some(name), Some(lib)) = (cx.n.strs.get(&format!("fn.{param}")).filter(|s| !s.is_empty()), funcs.lib) else { return Ok(None) };
    let prog = compile_named(lib, name).map_err(|e| Error::BadParam(0, format!("{param}: {e}")))?;
    let mut env = FnEnv::new([cx.s as f64, cx.hs as f64]);
    env.seed = cx.seed;
    env.images = images;
    if let Some(gin) = funcs.inputs {
        env.graph_inputs = gin;
    }
    Ok(Some(Fun { prog, env }))
}

fn var(name: &str, x: f32) -> (String, Value) {
    (name.to_string(), Value::float(x as f64))
}

fn var3(name: &str, x: [f32; 3]) -> (String, Value) {
    (name.to_string(), Value::vec(3, [x[0] as f64, x[1] as f64, x[2] as f64, 0.0]))
}

fn read(vars: &[(String, Value)], name: &str) -> Option<[f32; 4]> {
    vars.iter().rev().find(|(k, _)| k == name).and_then(|(_, v)| match v {
        Value::Num(n) => Some([n.v[0] as f32, if n.lanes == 1 { n.v[0] as f32 } else { n.v[1] as f32 }, if n.lanes == 1 { n.v[0] as f32 } else { n.v[2] as f32 }, 1.0]),
        _ => None,
    })
}

/// Parameters that a function drives per shape or per pixel here, instead of once for the whole node.
pub fn per_shape_param(kind: &str, param: &str) -> bool {
    match kind {
        "x_shape_splatter_v2" => matches!(param, "position_function_0" | "position_function_1" | "pattern_function" | "pattern_sdf_function"),
        "x_shape_splatter_v2_mapper_color" => param.starts_with("custom_function_"),
        "x_shape_splatter_v2_mapper_grayscale" => param == "custom_function",
        _ => false,
    }
}

/// Nodes that read function graphs of their own: the graph evaluator comes here with its function library.
pub fn runs_functions(kind: &str) -> bool {
    matches!(kind, "x_shape_splatter_v2" | "x_shape_splatter_v2_mapper_color" | "x_shape_splatter_v2_mapper_grayscale")
}

pub fn compute_with_functions(kind: &str, n: &Node, ins: &BTreeMap<String, Rc<Tex>>, s: usize, hs: usize, seed: u32, lib: &BTreeMap<String, Graph>, inputs: &BTreeMap<String, Value>) -> Option<Result<Tex, Error>> {
    let funcs = Funcs { lib: Some(lib), inputs: Some(inputs) };
    crate::nodes10::with_inputs(kind, n, ins, s, hs, seed, |cx| dispatch(cx, funcs))
}

fn dispatch<'a>(cx: &Cx<'a>, funcs: Funcs<'a>) -> Result<Tex, Error> {
    if cx.kind == "x_shape_splatter_v2" {
        shape_splatter(cx, funcs)
    } else {
        mapper(cx, funcs, cx.kind.ends_with("color"))
    }
}

pub fn compute(cx: &Cx) -> Option<Tex> {
    Some(match cx.kind {
        "x_shape_splatter_v2" | "x_shape_splatter_v2_mapper_color" | "x_shape_splatter_v2_mapper_grayscale" => return dispatch(cx, Funcs::default()).ok(),
        "x_shape_splatter_v2_to_mask" => to_mask(cx),
        "x_grid_atlas_color" => grid_atlas(cx, true),
        "x_grid_atlas_grayscale" => grid_atlas(cx, false),
        _ => return None,
    })
}

// ------------------------------------------------------------------------------------------------ placing shapes

/// One placed shape.
struct Shape {
    id: u32,
    /// the pattern of the list or atlas, or the material
    pattern: u32,
    /// pivot in the picture (world units: the picture's longest side is 1)
    pivot: [f32; 2],
    /// centre of the shape's box
    centre: [f32; 3],
    half: [f32; 3],
    r: M3,
    /// half size of the picture area the shape can touch
    reach: [f32; 2],
    angles: [f32; 3],
    spin: f32,
    along_normal: f32,
    /// height: `surface * gain + lift`, plus the background under the pivot or pixel as the settings ask
    gain: f32,
    lift: f32,
    ground: f32,
}

/// Unit-box surface point a ray reached.
#[derive(Clone, Copy)]
struct Hit {
    p: [f32; 3],
    n: [f32; 3],
    uv: [f32; 2],
    /// the pattern's value here, for flat patterns
    value: Option<f32>,
    /// `n` is already a world-free direction (from a normal map) and is not scaled by the box
    mapped: bool,
    colour: [f32; 3],
    metal: f32,
    rough: f32,
    material: Option<f32>,
}

fn hit(p: [f32; 3], n: [f32; 3], uv: [f32; 2]) -> Hit {
    Hit { p, n, uv, value: None, mapped: false, colour: [1.0; 3], metal: 0.0, rough: 0.5, material: None }
}

fn at(o: [f32; 3], d: [f32; 3], t: f32) -> [f32; 3] {
    add3(o, mul3(d, t))
}

/// Where a ray enters and leaves the box -1..1, and which face it entered by.
fn slab(o: [f32; 3], d: [f32; 3]) -> Option<(f32, f32, usize)> {
    let (mut near, mut far, mut face) = (f32::NEG_INFINITY, f32::INFINITY, 2);
    for i in 0..3 {
        if d[i].abs() < 1e-7 {
            if o[i].abs() > 1.0 {
                return None;
            }
            continue;
        }
        let (a, b) = ((-1.0 - o[i]) / d[i], (1.0 - o[i]) / d[i]);
        let (a, b) = (a.min(b), a.max(b));
        if a > near {
            near = a;
            face = i;
        }
        far = far.min(b);
    }
    (near <= far).then_some((near, far, face))
}

fn cube(o: [f32; 3], d: [f32; 3]) -> Option<Hit> {
    let (t, _, face) = slab(o, d)?;
    let p = at(o, d, t);
    let mut n = [0.0; 3];
    n[face] = -d[face].signum();
    let (a, b) = [(1, 2), (0, 2), (0, 1)][face];
    Some(hit(p, n, [(p[a] + 1.0) * 0.5, (p[b] + 1.0) * 0.5]))
}

fn sphere(o: [f32; 3], d: [f32; 3]) -> Option<Hit> {
    let (a, b, c) = (crate::nodes10::dot3(d, d), crate::nodes10::dot3(o, d), crate::nodes10::dot3(o, o) - 1.0);
    let disc = b * b - a * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / a;
    let p = at(o, d, t);
    Some(hit(p, p, [p[1].atan2(p[0]) / TAU + 0.5, p[2].clamp(-1.0, 1.0).acos() / PI]))
}

fn cylinder(o: [f32; 3], d: [f32; 3], cap_scale: [f32; 2], planar_caps: bool) -> Option<Hit> {
    let (a, b, c) = (d[0] * d[0] + d[1] * d[1], o[0] * d[0] + o[1] * d[1], o[0] * o[0] + o[1] * o[1] - 1.0);
    let (near, far) = if a < 1e-9 {
        if c > 0.0 {
            return None;
        }
        (f32::NEG_INFINITY, f32::INFINITY)
    } else {
        let disc = b * b - a * c;
        if disc < 0.0 {
            return None;
        }
        ((-b - disc.sqrt()) / a, (-b + disc.sqrt()) / a)
    };
    let (mut z0, mut z1) = (f32::NEG_INFINITY, f32::INFINITY);
    if d[2].abs() > 1e-7 {
        let (p, q) = ((-1.0 - o[2]) / d[2], (1.0 - o[2]) / d[2]);
        (z0, z1) = (p.min(q), p.max(q));
    } else if o[2].abs() > 1.0 {
        return None;
    }
    let (enter, leave) = (near.max(z0), far.min(z1));
    if enter > leave {
        return None;
    }
    let p = at(o, d, enter);
    let angle = p[1].atan2(p[0]) / TAU + 0.5;
    Some(if z0 > near {
        let uv = if planar_caps { [(p[0] + 1.0) * 0.5, (p[1] + 1.0) * 0.5] } else { [angle, p[0].hypot(p[1])] };
        hit(p, [0.0, 0.0, -d[2].signum()], [uv[0] * cap_scale[0], uv[1] * cap_scale[1]])
    } else {
        hit(p, [p[0], p[1], 0.0], [angle, (p[2] + 1.0) * 0.5])
    })
}

/// The plane z = 0 inside the box, or inside the circle when `round`.
fn flat(o: [f32; 3], d: [f32; 3], round: bool) -> Option<Hit> {
    if d[2].abs() < 1e-7 {
        return None;
    }
    let t = -o[2] / d[2];
    let p = at(o, d, t);
    let inside = if round { p[0] * p[0] + p[1] * p[1] <= 1.0 } else { p[0].abs() <= 1.0 && p[1].abs() <= 1.0 };
    inside.then(|| hit(p, [0.0, 0.0, -d[2].signum()], [(p[0] + 1.0) * 0.5, (p[1] + 1.0) * 0.5]))
}

/// Value at (u, v) of a picture, stopping half a texel from its border.
fn inside(t: &Tex, u: f32, v: f32) -> [f32; 4] {
    let (eu, ev) = (0.5 / t.w as f32, 0.5 / t.h as f32);
    t.sample(u.clamp(eu, 1.0 - eu), v.clamp(ev, 1.0 - ev))
}

/// Value at (u, v) of cell `i` of a `cols` x `rows` atlas, staying inside the cell.
fn cell(t: &Tex, i: u32, cols: u32, rows: u32, u: f32, v: f32) -> [f32; 4] {
    let (c, r) = ((i % cols) as f32, ((i / cols) % rows) as f32);
    let (eu, ev) = (0.5 * cols as f32 / t.w as f32, 0.5 * rows as f32 / t.h as f32);
    t.sample((c + u.clamp(eu, 1.0 - eu)) / cols as f32, (r + v.clamp(ev, 1.0 - ev)) / rows as f32)
}

/// What the pixels of the scene need to know besides the shapes.
struct Scene<'a> {
    kind: i32,
    patterns: Vec<Option<&'a Tex>>,
    atlas: Option<&'a Tex>,
    atlas_normal: Option<&'a Tex>,
    atlas_dx: bool,
    cols: u32,
    rows: u32,
    pattern_fn: Option<Fun<'a>>,
    pattern_value: f32,
    sdf_fn: Option<Fun<'a>>,
    sdf_round: f32,
    frame: [f32; 3],
    extrude: bool,
    symmetric: bool,
    samples: usize,
    cutout: Option<f32>,
    uv_scale: [f32; 2],
    cap_scale: [f32; 2],
    planar_caps: bool,
}

impl Scene<'_> {
    /// Pattern value (0..1) of shape `sh` at (u, v) of its box; below the cutout it is `None`.
    fn pattern(&self, sh: &Shape, u: f32, v: f32) -> Option<f32> {
        let x = match self.kind {
            IMAGE => self.patterns.get(sh.pattern as usize).copied().flatten().map(|t| lum(inside(t, u, v))).unwrap_or(1.0),
            ATLAS => self.atlas.map(|t| lum(cell(t, sh.pattern, self.cols, self.rows, u, v))).unwrap_or(1.0),
            _ => match &self.pattern_fn {
                Some(f) => {
                    let mut vars = vec![var("shape.id", sh.id as f32), var("material.id", sh.pattern as f32)];
                    f.call([u, v], &mut vars)[0]
                }
                None => self.pattern_value,
            },
        };
        match self.cutout {
            Some(c) if x < c => None,
            _ => Some(x),
        }
    }

    /// The pattern's slopes at (u, v), for its normal.
    fn pattern_normal(&self, sh: &Shape, u: f32, v: f32) -> [f32; 3] {
        let e = 1.5 / 128.0;
        let p = |a: f32, b: f32| self.pattern(sh, a, b).unwrap_or(0.0);
        [-(p(u + e, v) - p(u - e, v)) / (2.0 * e), -(p(u, v + e) - p(u, v - e)) / (2.0 * e), 1.0]
    }

    fn flat_pattern(&self, sh: &Shape, o: [f32; 3], d: [f32; 3]) -> Option<Hit> {
        let mut h = flat(o, d, false)?;
        let (u, v) = (h.uv[0], h.uv[1]);
        h.value = Some(self.pattern(sh, u, v)?);
        self.pattern_shading(sh, &mut h, u, v);
        Some(h)
    }

    /// The pattern read as a height field: walks down the ray until it is under the surface.
    fn extruded(&self, sh: &Shape, o: [f32; 3], d: [f32; 3]) -> Option<Hit> {
        let (near, far, _) = slab(o, d)?;
        let inside_at = |t: f32| -> bool {
            let p = at(o, d, t);
            match self.pattern(sh, (p[0] + 1.0) * 0.5, (p[1] + 1.0) * 0.5) {
                Some(v) if self.symmetric => p[2].abs() <= v,
                Some(v) => (p[2] + 1.0) * 0.5 <= v,
                None => false,
            }
        };
        let step = (far - near) / self.samples as f32;
        for i in 0..=self.samples {
            let t = near + step * i as f32;
            if !inside_at(t) {
                continue;
            }
            // closer, between the last sample outside and this one
            let (mut a, mut b) = (if i == 0 { t } else { t - step }, t);
            for _ in 0..5 {
                let m = 0.5 * (a + b);
                if inside_at(m) {
                    b = m;
                } else {
                    a = m;
                }
            }
            let p = at(o, d, b);
            let (u, v) = ((p[0] + 1.0) * 0.5, (p[1] + 1.0) * 0.5);
            let mut h = hit(p, [0.0, 0.0, 1.0], [u, v]);
            self.pattern_shading(sh, &mut h, u, v);
            return Some(h);
        }
        None
    }

    fn pattern_shading(&self, sh: &Shape, h: &mut Hit, u: f32, v: f32) {
        h.n = self.pattern_normal(sh, u, v);
        if let (ATLAS, Some(t)) = (self.kind, self.atlas_normal) {
            let n = dec_n(cell(t, sh.pattern, self.cols, self.rows, u, v), self.atlas_dx);
            h.n = [n[0], -n[1], n[2]];
            h.mapped = true;
        }
    }

    /// Distance from `p` (object space) to the shape: the function's, or a rounded box filling the frame.
    fn distance(&self, sh: &Shape, p: [f32; 3], vars: &mut Vec<(String, Value)>) -> f32 {
        match &self.sdf_fn {
            Some(f) => {
                vars.clear();
                vars.push(var3("shape.position.os", p));
                vars.push(var("shape.id", sh.id as f32));
                f.call([0.5, 0.5], vars)[0]
            }
            None => {
                let q = [p[0].abs() - (0.5 * self.frame[0] - self.sdf_round), p[1].abs() - (0.5 * self.frame[1] - self.sdf_round), p[2].abs() - (0.5 * self.frame[2] - self.sdf_round)];
                let out = [q[0].max(0.0), q[1].max(0.0), q[2].max(0.0)];
                crate::nodes10::dot3(out, out).sqrt() + q[0].max(q[1]).max(q[2]).min(0.0) - self.sdf_round
            }
        }
    }

    /// Walks the ray through the shape's box by the distance the shape reports.
    fn sdf(&self, sh: &Shape, o: [f32; 3], d: [f32; 3]) -> Option<Hit> {
        let (near, far, _) = slab(o, d)?;
        let k = [0.5 * self.frame[0], 0.5 * self.frame[1], 0.5 * self.frame[2]];
        let obj = |p: [f32; 3]| [p[0] * k[0], p[1] * k[1], p[2] * k[2]];
        let speed = ((d[0] * k[0]).hypot(d[1] * k[1])).hypot(d[2] * k[2]).max(1e-6);
        let mut vars = Vec::new();
        let mut t = near;
        for _ in 0..64 {
            let dist = self.distance(sh, obj(at(o, d, t)), &mut vars);
            if dist < 1e-3 {
                let p = at(o, d, t);
                let e = 2e-3;
                let q = obj(p);
                let s = |a: [f32; 3]| self.distance(sh, a, &mut Vec::new());
                let n = [s([q[0] + e, q[1], q[2]]) - s([q[0] - e, q[1], q[2]]), s([q[0], q[1] + e, q[2]]) - s([q[0], q[1] - e, q[2]]), s([q[0], q[1], q[2] + e]) - s([q[0], q[1], q[2] - e])];
                let mut h = hit(p, [n[0] * k[0], n[1] * k[1], n[2] * k[2]], [(p[0] + 1.0) * 0.5, (p[1] + 1.0) * 0.5]);
                self.distance(sh, q, &mut vars);
                if let Some(c) = read(&vars, "shape.color") {
                    h.colour = [c[0], c[1], c[2]];
                }
                h.metal = read(&vars, "shape.metalness").map_or(h.metal, |c| c[0]);
                h.rough = read(&vars, "shape.roughness").map_or(h.rough, |c| c[0]);
                h.material = read(&vars, "material.id").map(|c| c[0]);
                return Some(h);
            }
            t += dist / speed;
            if t > far {
                return None;
            }
        }
        None
    }

    fn trace(&self, sh: &Shape, x: f32, y: f32) -> Option<Hit> {
        let world = [x - sh.centre[0], y - sh.centre[1], 0.0];
        let o = tv(&sh.r, world);
        let d = tv(&sh.r, [0.0, 0.0, -1.0]);
        let unit = |v: [f32; 3]| [v[0] / sh.half[0], v[1] / sh.half[1], v[2] / sh.half[2]];
        let (o, d) = (unit(o), unit(d));
        let mut h = match self.kind {
            SDF => self.sdf(sh, o, d),
            CUBE => cube(o, d),
            SPHERE => sphere(o, d),
            CYLINDER => cylinder(o, d, self.cap_scale, self.planar_caps),
            PLANE => flat(o, d, false),
            DISC => flat(o, d, true),
            _ if self.extrude => self.extruded(sh, o, d),
            _ => self.flat_pattern(sh, o, d),
        }?;
        h.uv = [h.uv[0] * self.uv_scale[0], h.uv[1] * self.uv_scale[1]];
        Some(h)
    }

    /// Height of a surface point: 0 at the bottom of the shape's box, 1 at its top. A flat pattern's value stands in
    /// for the place on the box.
    fn surface(&self, sh: &Shape, h: &Hit) -> f32 {
        let z = sh.centre[2] + (0..3).map(|i| sh.r[2][i] * h.p[i] * sh.half[i]).sum::<f32>();
        let span = 2.0 * sh.half[2];
        match h.value {
            Some(v) => v + z / span,
            None => (z + sh.half[2]) / span,
        }
    }
}

fn halton(i: u32, base: u32) -> f32 {
    let (mut f, mut r, mut i) = (1.0, 0.0, i);
    while i > 0 {
        f /= base as f32;
        r += f * (i % base) as f32;
        i /= base;
    }
    r
}

/// Moves overlapping circles apart, on the wrapped picture.
fn relax(p: &mut [[f32; 2]], radius: &[f32], size: [f32; 2]) {
    let n = p.len();
    let widest = radius.iter().fold(0.0f32, |a, b| a.max(*b)) * 2.0;
    if widest <= 0.0 || n < 2 {
        return;
    }
    // with fewer than 3 buckets a side the neighbouring buckets would repeat
    let bx = ((size[0] / widest).floor() as usize).clamp(1, 256);
    let by = ((size[1] / widest).floor() as usize).clamp(1, 256);
    let (bx, by) = if bx < 3 || by < 3 { (1, 1) } else { (bx, by) };
    for pass in 0..32 {
        let mut buckets = vec![Vec::new(); bx * by];
        let index = |q: [f32; 2]| (((q[0] / size[0]).rem_euclid(1.0) * bx as f32) as usize).min(bx - 1) + by.min(((q[1] / size[1]).rem_euclid(1.0) * by as f32) as usize).min(by - 1) * bx;
        for (i, q) in p.iter().enumerate() {
            buckets[index(*q)].push(i);
        }
        let mut push = vec![[0.0f32; 2]; n];
        let range: &[i64] = if bx == 1 { &[0] } else { &[-1, 0, 1] };
        for (i, q) in p.iter().enumerate() {
            let b = index(*q);
            let (cx, cy) = ((b % bx) as i64, (b / bx) as i64);
            for dy in range {
                for dx in range {
                    let other = (cx + dx).rem_euclid(bx as i64) as usize + (cy + dy).rem_euclid(by as i64) as usize * bx;
                    for &j in &buckets[other] {
                        if j == i {
                            continue;
                        }
                        let wrap = |d: f32, s: f32| d - (d / s).round() * s;
                        let (mut ax, mut ay) = (wrap(q[0] - p[j][0], size[0]), wrap(q[1] - p[j][1], size[1]));
                        let mut dist = ax.hypot(ay);
                        if dist < 1e-6 {
                            let a = rnd(i as i32, pass, 91) * TAU;
                            (ax, ay, dist) = (a.cos(), a.sin(), 1.0);
                        }
                        let over = radius[i] + radius[j] - dist;
                        if over > 0.0 {
                            push[i][0] += 0.4 * over * ax / dist;
                            push[i][1] += 0.4 * over * ay / dist;
                        }
                    }
                }
            }
        }
        for (q, m) in p.iter_mut().zip(&push) {
            q[0] += m[0];
            q[1] += m[1];
        }
    }
}

/// The pixel-level result of one shape pixel or background pixel.
#[derive(Clone, Copy, Default)]
struct Px {
    height: f32,
    normal: [f32; 4],
    uvw: [f32; 4],
    d1: [f32; 4],
    d2: [f32; 4],
    d3: [f32; 4],
    colour: [f32; 4],
    metal: f32,
    rough: f32,
}

fn shape_splatter<'a>(cx: &Cx<'a>, funcs: Funcs<'a>) -> Result<Tex, Error> {
    let (s, hs) = (cx.s, cx.hs);
    let seed = cx.seed.wrapping_add(811);
    let longest = s.max(hs) as f32;
    let size = [s as f32 / longest, hs as f32 / longest];
    let mode = cx.i("position_distribution_mode").clamp(0, 3);
    let kind = cx.i("shape_type").clamp(SDF, PATTERN_FN);
    let (mut nx, mut ny) = (cx.i("x_amount").clamp(1, 64), cx.i("y_amount").clamp(1, 64));
    if cx.b("non_square_expansion") {
        nx = ((nx as f32 * size[0]).round() as i32).max(1);
        ny = ((ny as f32 * size[1]).round() as i32).max(1);
    }
    let on_grid = mode < 2;
    let count = if on_grid { (nx * ny) as usize } else { cx.i("amount").clamp(1, 4096) as usize };
    let cell_size = if on_grid {
        [size[0] / nx as f32, size[1] / ny as f32]
    } else {
        let side = (size[0] * size[1] / count as f32).sqrt();
        [side, side]
    };
    let r = |i: usize, k: i32| rnd(i as i32, k, seed);
    let both = |i: usize, k: i32| 2.0 * r(i, k) - 1.0;

    // the maps
    let at_uv = |q: [f32; 2]| (q[0] / size[0], q[1] / size[1]);
    let read_map = |id: &str| cx.img(id);
    let map_at = |m: Option<&Tex>, q: [f32; 2], fallback: f32| m.map_or(fallback, |t| {
        let (u, v) = at_uv(q);
        g(t, u, v)
    });
    let vector = cx.img("vector_map");
    let background = read_map("background_height");
    let smooth = cx.f("smooth_background_slope").clamp(0.0, 1.0);
    let smoothed = background.filter(|_| smooth > 0.0).map(|t| crate::ops::blur(t, smooth * s as f32 / 16.0));
    let ground = smoothed.as_ref().or(background);

    // sizes
    let scale = cx.f("shape_scale");
    let scale_r = cx.f("shape_scale_random").clamp(0.0, 1.0);
    let random_scale: Vec<f32> = (0..count).map(|i| scale * (1.0 - scale_r * r(i, 1))).collect();
    let square = if cx.b("keep_size_ratio") { [cell_size[0].min(cell_size[1]); 2] } else { cell_size };
    let unit = if cx.i("size_mode") == 1 { [1.0; 3] } else { [square[0], square[1], square[0].min(square[1])] };
    let is_pattern = matches!(kind, IMAGE | ATLAS | PATTERN_FN);
    let (size3, size3_r) = if is_pattern {
        let (a, b) = (cx.v2("shape_size_2d"), cx.v2("shape_size_random_2d"));
        ([a[0], a[1], 1.0], [b[0], b[1], 0.0])
    } else {
        (cx.v3("shape_size"), cx.v3("shape_size_random"))
    };

    // positions
    let function = [fun(cx, funcs, "position_function_0", Vec::new())?, fun(cx, funcs, "position_function_1", Vec::new())?];
    let fixed = cx.v2("position_function");
    let sequence = cx.i("position_distribution_sequence");
    let (line, line_mode) = (cx.f("line_position_offset"), cx.i("line_position_offset_mode"));
    let (jitter, jitter_k) = (cx.f("position_random"), cx.v2("position_random_multiplier"));
    let mut pos: Vec<[f32; 2]> = (0..count)
        .map(|i| {
            if !on_grid && mode == 2 {
                let n = i as u32 + 1;
                let q = if sequence == 1 { [halton(n, 2), halton(n, 3)] } else { [(0.5 + n as f32 * 0.754_877_7).fract(), (0.5 + n as f32 * 0.569_840_3).fract()] };
                return [q[0] * size[0] + jitter * jitter_k[0] * both(i, 2), q[1] * size[1] + jitter * jitter_k[1] * both(i, 3)];
            }
            if mode == 3 {
                let mut out = fixed;
                for (lane, f) in function.iter().enumerate() {
                    if let Some(f) = f {
                        let mut vars = vec![var("shape.id", i as f32 + 1.0), var("shape.amount", count as f32)];
                        let v = f.call([0.5, 0.5], &mut vars);
                        out[lane] = v[lane];
                    }
                }
                return [out[0] * size[0] + jitter * jitter_k[0] * both(i, 2), out[1] * size[1] + jitter * jitter_k[1] * both(i, 3)];
            }
            let (ix, iy) = ((i as i32 % nx) as f32, (i as i32 / nx) as f32);
            let (mut x, mut y) = ((ix + 0.5) * cell_size[0], (iy + 0.5) * cell_size[1]);
            match line_mode {
                0 => x += line * iy,
                1 => y += line * ix,
                2 if iy as i32 & 1 == 1 => x += line * cell_size[0],
                3 if ix as i32 & 1 == 1 => y += line * cell_size[1],
                _ => {}
            }
            if mode == 1 {
                // the simulation needs them apart from the start
                x += 0.5 * cell_size[0] * both(i, 4);
                y += 0.5 * cell_size[1] * both(i, 5);
            }
            [x + jitter * jitter_k[0] * both(i, 2), y + jitter * jitter_k[1] * both(i, 3)]
        })
        .collect();
    if mode == 1 {
        let radius: Vec<f32> = random_scale.iter().map(|k| 0.5 * k * unit[0].max(unit[1])).collect();
        relax(&mut pos, &radius, size);
    }

    // brighter places of the density map draw the shapes in
    let density = cx.img("density_map");
    let pull = cx.f("density_map_multiplier");
    if let Some(m) = density.filter(|_| pull != 0.0) {
        let step = cell_size[0].max(cell_size[1]);
        for q in pos.iter_mut() {
            for _ in 0..12 {
                let p = |x: f32, y: f32| map_at(Some(m), [x, y], 0.0);
                let e = 0.75 * step;
                let (gx, gy) = (p(q[0] + e, q[1]) - p(q[0] - e, q[1]), p(q[0], q[1] + e) - p(q[0], q[1] - e));
                let len = gx.hypot(gy);
                if len > 1e-4 {
                    let far = pull * step * 0.25 * (1.0 - p(q[0], q[1])).max(0.1);
                    q[0] += gx / len * far;
                    q[1] += gy / len * far;
                }
            }
        }
    }

    let (mask_r, threshold) = (cx.f("mask_random"), cx.f("mask_map_threshold"));
    let mask = cx.img("mask_map");
    let (vd, vd_k) = (cx.f("vector_map_displacement"), cx.v3("vector_displacement_multiplier"));
    let (scale_map, scale_k) = (cx.img("shape_scale_map"), cx.f("scale_map_multiplier"));
    let post = if mode == 1 { cx.f("shape_scale_post_poisson") } else { 1.0 };
    let radius = cx.f("cylinder_radius");
    let extrude = is_pattern && cx.b("enable_shape_extrude");
    let extrude_h = cx.f("shape_extrude_height");
    let frame = cx.v3("sdf_bounding_frame_size");
    let (angles, angles_r, angles_k, angles_map) = (cx.v3("3d_rotation"), cx.f("3d_rotation_random"), cx.v3("3d_rotation_random_multiplier"), cx.v3("3d_rotation_map_multiplier"));
    let rotation_map = cx.img("shape_rotation_map");
    let (spin, spin_r) = (cx.f("rotation_around_normal"), cx.f("rotation_around_normal_random"));
    let (slope, slope_input) = (cx.f("slope_rotation"), cx.i("slope_rotation_input"));
    let (vector_turn, vector_axis) = (cx.f("vector_map_multiplier"), cx.i("vector_map_rotation_axis"));
    let (along, along_r) = (cx.f("offset_along_normal"), cx.f("offset_along_normal_random"));
    let pivot_local = cx.v3("pivot_position_local");
    let (hscale, hscale_r, hscale_map, hscale_k) = (cx.f("height_scale"), cx.f("height_scale_random"), cx.img("height_scale_map"), cx.f("height_scale_map_multiplier"));
    let (normalized, affects) = (cx.b("normalized_workflow"), cx.b("shape_scale_affects_height_scale"));
    let (hoff, hoff_r, hoff_id, hoff_map, hoff_k) = (cx.f("height_offset"), cx.f("height_offset_random"), cx.f("height_offset_from_id"), cx.img("height_offset_map"), cx.f("height_offset_map_multiplier"));
    let (from_ground, conform) = (cx.f("height_offset_from_background"), cx.f("conform_to_background").clamp(0.0, 1.0));
    let global = cx.v2("global_offset");
    let patterns: Vec<Option<&Tex>> = (1..=cx.i("pattern_input_number").clamp(1, 8)).map(|i| cx.img(&format!("pattern_input_{i}"))).collect();
    let atlas_size = [cx.lane("grid_atlas_size", 0).round().clamp(1.0, 16.0) as u32, cx.lane("grid_atlas_size", 1).round().clamp(1.0, 16.0) as u32];
    let atlas = cx.img("grid_atlas_height");
    let choices = match kind {
        IMAGE => patterns.len() as u32,
        ATLAS => atlas_size[0] * atlas_size[1],
        _ => 1,
    };
    // an empty cell of the atlas leaves a gap
    let filled: Vec<bool> = (0..choices)
        .map(|i| match (kind, atlas) {
            (ATLAS, Some(t)) => (0..8).any(|a| (0..8).any(|b| lum(cell(t, i, atlas_size[0], atlas_size[1], (a as f32 + 0.5) / 8.0, (b as f32 + 0.5) / 8.0)) > 1e-3)),
            _ => true,
        })
        .collect();

    let mut shapes = Vec::with_capacity(count);
    for i in 0..count {
        let mut q = pos[i];
        let v = vector.map(|t| {
            let (u, w) = at_uv(q);
            t.sample(u, w)
        });
        let mut lift_z = 0.0;
        if let Some(c) = v {
            q[0] += c[0] * vd * vd_k[0];
            q[1] += c[1] * vd * vd_k[1];
            lift_z += c[2] * vd * vd_k[2];
        }
        if r(i, 6) < mask_r || mask.is_some() && map_at(mask, q, 1.0) < threshold {
            continue;
        }
        let pattern = (r(i, 7) * choices as f32) as u32 % choices.max(1);
        if !filled[pattern as usize] {
            continue;
        }

        let sc = random_scale[i] * map_at(scale_map, q, 1.0).max(0.0).min(1.0).mul_add(scale_k, 1.0 - scale_k) * post;
        let mut half = [0.0; 3];
        for k in 0..3 {
            half[k] = 0.5 * unit[k] * sc * size3[k] * (1.0 - size3_r[k].clamp(0.0, 1.0) * r(i, 8 + k as i32));
        }
        match kind {
            CYLINDER => {
                half[0] = radius * unit[0] * sc * size3[0] * (1.0 - size3_r[0].clamp(0.0, 1.0) * r(i, 8));
                half[1] = radius * unit[1] * sc * size3[1] * (1.0 - size3_r[1].clamp(0.0, 1.0) * r(i, 9));
            }
            SDF => {
                for (k, h) in half.iter_mut().enumerate() {
                    *h = 0.5 * unit[0].min(unit[1]) * sc * frame[k];
                }
            }
            PLANE | DISC => half[2] = half[0].min(half[1]),
            _ if extrude => half[2] = (0.5 * extrude_h * sc).max(1e-5),
            _ if is_pattern => half[2] = half[0].min(half[1]),
            _ => {}
        }
        if half.iter().any(|h| *h <= 1e-6) {
            continue;
        }

        // orientation
        let mut a = [0.0; 3];
        let turn = map_at(rotation_map, q, 0.0);
        for k in 0..3 {
            a[k] = angles[k] + angles_r * angles_k[k] * both(i, 11 + k as i32) + turn * angles_map[k];
        }
        let mut around = spin + spin_r * both(i, 14);
        let mut around_z = 0.0;
        if let Some(c) = v {
            let toward = c[1].atan2(c[0]) / TAU * vector_turn;
            if vector_axis == 1 {
                around_z = toward;
            } else {
                around += toward;
            }
        }
        let target = match (slope_input, v, background) {
            (1, Some(c), _) => Some([c[0], c[1], c[2]]),
            (0, _, Some(t)) => {
                let (u, w) = at_uv(q);
                let e = 2.0;
                let (du, dw) = (e / s as f32, e / hs as f32);
                Some([-(g(t, u + du, w) - g(t, u - du, w)) / (2.0 * du * size[0]), -(g(t, u, w + dw) - g(t, u, w - dw)) / (2.0 * dw * size[1]), 1.0])
            }
            _ => None,
        };
        let outer = mm(&mm(&rz(around_z), &target.filter(|_| slope != 0.0).map_or(euler([0.0; 3]), |n| tilt_to(n, slope))), &euler(a));
        let rot = mm(&outer, &rz(around));

        let offset = along + along_r * both(i, 15);
        let up = mv(&rot, [0.0, 0.0, 1.0]);
        q[0] += up[0] * offset;
        q[1] += up[1] * offset;
        let lift_n = up[2] * offset;
        q[0] += global[0];
        q[1] += global[1];
        q = [q[0].rem_euclid(size[0]), q[1].rem_euclid(size[1])];

        let shift = mv(&rot, [-pivot_local[0] * half[0], -pivot_local[1] * half[1], -pivot_local[2] * half[2]]);
        let centre = [q[0] + shift[0], q[1] + shift[1], shift[2]];
        let reach = [(0..3).map(|k| (rot[0][k] * half[k]).abs()).sum::<f32>(), (0..3).map(|k| (rot[1][k] * half[k]).abs()).sum::<f32>()];

        // the height the surface gets
        let span = 2.0 * half[2];
        let by_scale = if normalized {
            if affects { span } else { span / sc.max(1e-5) }
        } else if affects {
            sc
        } else {
            1.0
        };
        let gain = by_scale * hscale * (1.0 - hscale_r * r(i, 16)) * map_at(hscale_map, q, 1.0).mul_add(hscale_k, 1.0 - hscale_k);
        let lift = hoff + hoff_r * both(i, 17) + hoff_id * i as f32 / (count.max(2) - 1) as f32 + map_at(hoff_map, q, 0.0) * hoff_k + lift_z + lift_n;
        shapes.push(Shape {
            id: i as u32 + 1,
            pattern,
            pivot: q,
            centre,
            half,
            r: rot,
            reach,
            angles: to_euler(&outer),
            spin: around,
            along_normal: offset,
            gain,
            lift,
            ground: map_at(ground, q, 0.0),
        });
    }

    let cutout = cx.b("enable_cutout").then(|| cx.f("cutout_threshold"));
    let sdf_fn = fun(cx, funcs, "pattern_sdf_function", Vec::new())?;
    let pattern_fn = fun(cx, funcs, "pattern_function", Vec::new())?;
    let cap_mode = cx.i("cap_uv_mode") == 1;
    let dx_out = cx.i("output_normal_format") == 0;
    let scene = Scene {
        kind,
        patterns,
        atlas,
        atlas_normal: cx.img("grid_atlas_normal").filter(|_| !cx.b("recompute_grid_atlas_normal")),
        atlas_dx: cx.i("grid_atlas_normal_format") == 0,
        cols: atlas_size[0],
        rows: atlas_size[1],
        pattern_fn,
        pattern_value: cx.f("pattern_function"),
        sdf_fn,
        sdf_round: cx.f("pattern_sdf_function").clamp(0.0, 0.5 * frame[0].min(frame[1]).min(frame[2]).max(0.0)),
        frame,
        extrude,
        symmetric: cx.b("shape_extrude_symmetry"),
        samples: cx.i("shape_extrude_samples").clamp(1, 256) as usize,
        cutout,
        uv_scale: cx.v2("uv_scale"),
        cap_scale: cx.v2("cap_uv_scale"),
        planar_caps: cap_mode,
    };

    // shapes by the part of the picture they reach, with the wrapped copies that fall there
    let widest = shapes.iter().map(|h| h.reach[0].max(h.reach[1])).fold(0.0f32, f32::max);
    let bins = ((0.5 / widest.max(1e-3)) as usize).clamp(1, 128);
    let mut table: Vec<Vec<(u32, f32, f32)>> = vec![Vec::new(); bins * bins];
    for (i, h) in shapes.iter().enumerate() {
        let span = |c: f32, e: f32, side: f32| -> (i64, i64) {
            let (a, b) = (((c - e) / side * bins as f32).floor() as i64, ((c + e) / side * bins as f32).floor() as i64);
            (a, b.min(a + 3 * bins as i64))
        };
        let (x0, x1) = span(h.centre[0], h.reach[0], size[0]);
        let (y0, y1) = span(h.centre[1], h.reach[1], size[1]);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let b = x.rem_euclid(bins as i64) as usize + y.rem_euclid(bins as i64) as usize * bins;
                table[b].push((i as u32, x.div_euclid(bins as i64) as f32 * size[0], y.div_euclid(bins as i64) as f32 * size[1]));
            }
        }
    }

    let opacity = cx.f("background_input_opacity");
    let px: Vec<Px> = crate::tex::par_map(s * hs, |i| {
        let (ix, iy) = (i % s, i / s);
        let (u, v) = ((ix as f32 + 0.5) / s as f32, (iy as f32 + 0.5) / hs as f32);
        let (x, y) = (u * size[0], v * size[1]);
        let under = background.map_or(0.0, |t| g(t, u, v)) * opacity;
        let mut best: Option<(f32, &Shape, Hit)> = None;
        let bin = (u * bins as f32) as usize + (v * bins as f32) as usize * bins;
        for &(k, sx, sy) in &table[bin.min(bins * bins - 1)] {
            let sh = &shapes[k as usize];
            let Some(h) = scene.trace(sh, x + sx, y + sy) else { continue };
            let level = if from_ground != 0.0 { from_ground * lerp(sh.ground, ground.map_or(0.0, |t| g(t, u, v)), conform) } else { 0.0 };
            let total = scene.surface(sh, &h) * sh.gain + sh.lift + level;
            if best.as_ref().is_none_or(|b| total > b.0) {
                best = Some((total, sh, h));
            }
        }
        let mut out = Px { height: under, normal: enc_n([0.0, 0.0, 1.0], dx_out), ..Px::default() };
        let Some((total, sh, h)) = best else { return out };
        let id = sh.id as f32 + sh.pattern.min(255) as f32 / 256.0;
        let visible = total >= under;
        out.d1 = [h.p[0], h.p[1], h.p[2], if visible { 1.0 } else { -1.0 }];
        out.d2 = [sh.angles[0], sh.angles[1], sh.angles[2], sh.spin];
        out.d3 = [sh.pivot[0] / size[0], sh.pivot[1] / size[1], sh.along_normal, id];
        if visible {
            out.height = total;
            let mut material = sh.pattern as f32;
            if let Some(m) = h.material {
                material = m;
            }
            out.uvw = [h.uv[0], h.uv[1], total, sh.id as f32 + material.clamp(0.0, 255.0) / 256.0];
            let local = if h.mapped { h.n } else { [h.n[0] / sh.half[0], h.n[1] / sh.half[1], h.n[2] / sh.half[2]] };
            let n = norm3(mv(&sh.r, local));
            out.normal = enc_n([n[0], -n[1], n[2]], dx_out);
            out.colour = [h.colour[0], h.colour[1], h.colour[2], 1.0];
            out.metal = h.metal;
            out.rough = h.rough;
        }
        out
    });

    let mut heights: Vec<f32> = px.iter().map(|p| p.height).collect();
    let (box2, box3, show_pivot) = (cx.b("show_shape_2d_bbox"), cx.b("show_shape_3d_bbox"), cx.b("show_shape_pivot"));
    if box2 || box3 || show_pivot {
        let mut line = |a: [f32; 2], b: [f32; 2]| {
            let n = (((a[0] - b[0]).abs() * longest).max((a[1] - b[1]).abs() * longest).ceil() as usize).clamp(1, 4 * s.max(hs));
            for k in 0..=n {
                let t = k as f32 / n as f32;
                let (x, y) = (lerp(a[0], b[0], t) / size[0] * s as f32, lerp(a[1], b[1], t) / size[1] * hs as f32);
                let (x, y) = ((x.floor() as i64).rem_euclid(s as i64) as usize, (y.floor() as i64).rem_euclid(hs as i64) as usize);
                heights[y * s + x] = 1.0;
            }
        };
        for h in &shapes {
            let corner = |x: f32, y: f32, z: f32| {
                let p = mv(&h.r, [x * h.half[0], y * h.half[1], z * h.half[2]]);
                [h.centre[0] + p[0], h.centre[1] + p[1]]
            };
            if box2 {
                let (c, e) = (h.centre, h.reach);
                let q = [[c[0] - e[0], c[1] - e[1]], [c[0] + e[0], c[1] - e[1]], [c[0] + e[0], c[1] + e[1]], [c[0] - e[0], c[1] + e[1]]];
                for k in 0..4 {
                    line(q[k], q[(k + 1) % 4]);
                }
            }
            if box3 {
                for k in 0..12 {
                    let f = |b: i32| if b != 0 { 1.0 } else { -1.0 };
                    let (a, w) = (k / 4, k % 4);
                    let (p, q) = (f(w & 1), f(w >> 1));
                    let (s0, s1) = match a {
                        0 => ([-1.0, p, q], [1.0, p, q]),
                        1 => ([p, -1.0, q], [p, 1.0, q]),
                        _ => ([p, q, -1.0], [p, q, 1.0]),
                    };
                    line(corner(s0[0], s0[1], s0[2]), corner(s1[0], s1[1], s1[2]));
                }
            }
            if show_pivot {
                let p = h.pivot;
                for k in 0..3 {
                    let mut e = [0.0; 3];
                    e[k] = h.half[k];
                    let w = mv(&h.r, e);
                    line(p, [p[0] + w[0], p[1] + w[1]]);
                }
            }
        }
    }

    let header = [
        kind as f32,
        cx.i("pattern_input_number").clamp(1, 8) as f32 * if dx_out { 1.0 } else { -1.0 },
        atlas_size[0] as f32,
        atlas_size[1] as f32,
    ];
    let make = |i: usize, f: &dyn Fn(usize) -> [f32; 4]| -> Option<Tex> {
        cx.wants(i).then(|| Tex { w: s, h: hs, px: (0..s * hs).map(f).collect() })
    };
    Ok(cx.pick(vec![
        make(0, &|i| grey(heights[i])),
        make(1, &|i| px[i].colour),
        make(2, &|i| grey(px[i].metal)),
        make(3, &|i| grey(px[i].rough)),
        make(4, &|i| px[i].normal),
        make(5, &|i| px[i].uvw),
        make(6, &|i| px[i].d1),
        make(7, &|i| px[i].d2),
        make(8, &|i| px[i].d3),
        make(9, &|i| if i == 0 { [s as f32, hs as f32, s as f32, hs as f32] } else { header }),
    ]))
}

// ------------------------------------------------------------------------------------------------ mapper

/// Splatter data keeps exact values: the nearest pixel, never a blend of two shapes.
fn nearest(t: &Tex, u: f32, v: f32) -> [f32; 4] {
    t.at((u * t.w as f32).floor() as i64, (v * t.h as f32).floor() as i64)
}

fn id_of(a: f32) -> (i32, u32) {
    (a.floor() as i32, ((a - a.floor()) * 256.0).round() as u32)
}

fn blend(mode: i32, a: f32, b: f32) -> f32 {
    match mode {
        1 => a + b,
        2 => a - b,
        3 => a * b,
        4 => {
            if a < 0.5 {
                2.0 * a * b
            } else {
                1.0 - 2.0 * (1.0 - a) * (1.0 - b)
            }
        }
        _ => b,
    }
}

fn mapper<'a>(cx: &Cx<'a>, funcs: Funcs<'a>, colour: bool) -> Result<Tex, Error> {
    let (s, hs, seed) = (cx.s, cx.hs, cx.seed.wrapping_add(823));
    let Some(uvw) = cx.img("splatter_uvw") else {
        return Ok(match cx.img("background_input") {
            Some(t) => Tex::from_fn(s, hs, |u, v| t.sample(u, v)),
            None => Tex::solid(s, hs, [0.0, 0.0, 0.0, 1.0]),
        });
    };
    let (d1, d2, d3) = (cx.img("splatter_data_1"), cx.img("splatter_data_2"), cx.img("splatter_data_3"));
    let normals = cx.img("normal");
    // the header is on every pixel of the fourth data image but the first
    let header = cx.img("splatter_data_4").map(|t| t.at(t.w as i64 / 2, t.h as i64 / 2));
    let shape_type = header.map_or(IMAGE, |h| h[0].round() as i32);
    let (cols, rows) = header.map_or((1, 1), |h| (h[2].round().max(1.0) as u32, h[3].round().max(1.0) as u32));
    let dx_out = header.is_none_or(|h| h[1] >= 0.0);
    let count = cx.i("pattern_input_number").clamp(1, 8) as usize;
    let patterns: Vec<Option<&Tex>> = (1..=8).map(|i| cx.img(&format!("pattern_input_{i}"))).collect();
    let atlas = cx.img("grid_atlas_input");
    let mode = cx.i("projection_mode");
    let pick = cx.i("input_selection_mode");
    let index = cx.i("image_index").max(0) as u32;
    let range = [cx.lane("random_range", 0).round().max(0.0) as u32, cx.lane("random_range", 1).round().max(0.0) as u32];
    let (lo, hi) = (range[0].min(range[1]), range[0].max(range[1]));
    let tiling = cx.i("tiling_mode");
    let (uv_tiling, uv_scale, uv_offset, uv_random) = (cx.f("uv_tiling"), cx.v2("uv_scale"), cx.v2("uv_offset"), cx.f("random_offset"));
    let (contrast, spread, centre, proj_scale) = (cx.f("blending_contrast").clamp(0.0, 1.0), cx.i("image_projection"), cx.v3("projection_center"), cx.f("projection_scale"));
    let sharp = 1.0 + 63.0 * contrast * contrast;
    let color_input = cx.img("color_input");
    let (ci_opacity, ci_mode) = (cx.f("color_input_opacity").clamp(0.0, 1.0), cx.i("color_input_blending_mode"));
    let (hsl, hsl_r) = if colour { (cx.v3("hsl_adjustment"), cx.v3("hsl_random")) } else { ([0.0; 3], [0.0; 3]) };
    let (lum_a, lum_r) = if colour { (0.0, 0.0) } else { (cx.f("luminance_adjustment"), cx.f("luminance_random")) };
    let is_normal = colour && cx.b("is_normal_map");
    let input_dx = colour && cx.i("input_normal_format") == 0;
    let cone = if colour { cx.f("normal_angle_random").clamp(0.0, 1.0) } else { 0.0 };
    let background = cx.img("background_input");
    let plain = if is_normal { enc_n([0.0, 0.0, 1.0], dx_out) } else { [0.0, 0.0, 0.0, 1.0] };

    let mut images: Vec<(&'a str, &'a Tex)> = Vec::new();
    if let Some(t) = atlas {
        images.push((PROCESSOR_INPUTS[0], t));
    }
    for (i, t) in patterns.iter().enumerate() {
        if let Some(t) = t {
            images.push((PROCESSOR_INPUTS[i + 1], t));
        }
    }
    let custom: Vec<Option<Fun<'a>>> = if colour {
        vec![fun(cx, funcs, "custom_function_0", images.clone())?, fun(cx, funcs, "custom_function_1", images.clone())?, fun(cx, funcs, "custom_function_2", images.clone())?, fun(cx, funcs, "custom_function_3", images.clone())?]
    } else {
        vec![fun(cx, funcs, "custom_function", images)?]
    };
    let fixed = if colour {
        cx.v4("custom_function")
    } else {
        let v = cx.f("custom_function");
        [v, v, v, 1.0]
    };

    // image `i` of the mapper at (u, v); a missing image is plain white so the shapes still show
    let image = |i: u32, u: f32, v: f32| -> [f32; 4] {
        let wrapped = |x: f32, on: bool| if on { Some(x.rem_euclid(1.0)) } else { (0.0..=1.0).contains(&x).then_some(x) };
        let (Some(u), Some(v)) = (wrapped(u, tiling == 1 || tiling == 3), wrapped(v, tiling == 2 || tiling == 3)) else { return [0.0; 4] };
        if let (Some(t), true) = (atlas, shape_type == ATLAS || patterns.iter().all(Option::is_none)) {
            return cell(t, i, cols, rows, u, v);
        }
        patterns[(i as usize % count).min(7)].map_or([1.0; 4], |t| inside(t, u, v))
    };

    Ok(Tex::from_fn(s, hs, |u, v| {
        let base = background.map_or(plain, |t| t.sample(u, v));
        let a = nearest(uvw, u, v);
        if a[3] <= 0.0 {
            return base;
        }
        let (id, material) = id_of(a[3]);
        let r = |k: i32| rnd(id, k, seed);
        let which = match pick {
            1 => index,
            2 => lo + ((r(1) * (hi - lo + 1) as f32) as u32).min(hi - lo),
            _ => material,
        };
        let turns = d2.map_or([0.0; 4], |t| nearest(t, u, v));
        let rot = mm(&euler([turns[0], turns[1], turns[2]]), &rz(turns[3]));
        let world_n = normals.map_or([0.0, 0.0, 1.0], |t| {
            let n = dec_n(nearest(t, u, v), dx_out);
            [n[0], -n[1], n[2]]
        });
        let object_n = tv(&rot, world_n);
        let p = d1.map_or([0.0; 4], |t| nearest(t, u, v));
        let p = [p[0], p[1], p[2]];

        let mut c = match mode {
            1 => {
                let weight: Vec<f32> = object_n.iter().map(|n| n.abs().powf(sharp)).collect();
                let total: f32 = weight.iter().sum::<f32>().max(1e-6);
                let mut sum = [0.0; 4];
                for axis in 0..3 {
                    let (a0, a1) = [(1, 2), (0, 2), (0, 1)][axis];
                    let at = |k: usize| (p[k] * 0.5 + centre[k]) * proj_scale + 0.5;
                    let side = (object_n[axis] < 0.0) as u32;
                    let i = match spread {
                        1 => axis as u32,
                        2 => axis as u32 * 2 + side,
                        3 => which,
                        _ => 0,
                    };
                    let q = image(i, at(a0), at(a1));
                    for ch in 0..4 {
                        sum[ch] += q[ch] * weight[axis] / total;
                    }
                }
                sum
            }
            2 => {
                let mut out = fixed;
                for (lane, f) in custom.iter().enumerate() {
                    if let Some(f) = f {
                        let mut vars = vec![var3("shape.position.os", p), var3("shape.position.ws", mv(&rot, p)), var3("shape.normal.os", object_n), var3("shape.normal.ws", world_n), var("shape.id", id as f32), var("material.id", material as f32)];
                        let q = f.call([u, v], &mut vars);
                        if colour {
                            out[lane] = q[lane];
                        } else {
                            out = [q[0], q[0], q[0], 1.0];
                        }
                    }
                }
                out
            }
            _ => {
                let at = |x: f32, k: usize| (x - 0.5) * uv_tiling * uv_scale[k] + 0.5 + uv_offset[k] + uv_random * (2.0 * r(2 + k as i32) - 1.0);
                image(which, at(a[0], 0), at(a[1], 1))
            }
        };

        if is_normal {
            let n = dec_n(c, input_dx);
            let mut w = mv(&rot, [n[0], -n[1], n[2]]);
            if cone > 0.0 {
                let (a, z) = (r(4) * TAU, r(5));
                let side = (1.0 - z * z).max(0.0).sqrt();
                let d = [side * a.cos(), side * a.sin(), z];
                w = norm3(add3(mul3(w, 1.0 - cone), mul3(d, cone)));
            }
            let alpha = c[3];
            c = enc_n([w[0], -w[1], w[2]], dx_out);
            c[3] = alpha;
        } else {
            if colour {
                let (h, sa, va) = crate::ops::rgb_to_hsv(c[0], c[1], c[2]);
                let d = |k: i32, amount: f32| amount * (2.0 * r(k) - 1.0);
                let (rr, gg, bb) = crate::ops::hsv_to_rgb((h + hsl[0] + d(6, hsl_r[0])).rem_euclid(1.0), (sa + hsl[1] + d(7, hsl_r[1])).clamp(0.0, 1.0), (va + hsl[2] + d(8, hsl_r[2])).clamp(0.0, 1.0));
                c = [rr, gg, bb, c[3]];
            } else {
                let l = (lum(c) + lum_a + lum_r * (2.0 * r(6) - 1.0)).clamp(0.0, 1.0);
                c = [l, l, l, c[3]];
            }
            if let (Some(t), Some(q)) = (color_input, d3) {
                let pivot = nearest(q, u, v);
                let tint = t.sample(pivot[0], pivot[1]);
                for ch in 0..3 {
                    c[ch] = lerp(c[ch], blend(ci_mode, c[ch], tint[ch]), ci_opacity);
                }
            }
        }
        let out = lerp4(base, c, c[3].clamp(0.0, 1.0));
        if colour {
            out
        } else {
            grey(out[0])
        }
    }))
}

// ------------------------------------------------------------------------------------------------ to mask

fn to_mask(cx: &Cx) -> Tex {
    let (s, hs, seed) = (cx.s, cx.hs, cx.seed.wrapping_add(827));
    let Some(uvw) = cx.img("splatter_uvw") else { return Tex::solid(s, hs, grey(0.0)) };
    let (a, b) = (cx.i("shape_id_start_range"), cx.i("shape_id_end_range"));
    let (ida, idb) = (a.min(b), a.max(b));
    let shift = cx.i("shape_id_offset");
    let (a, b) = (cx.i("material_pattern_id_start_range"), cx.i("material_pattern_id_end_range"));
    let (ma, mb) = (a.min(b), a.max(b));
    let combine = cx.i("material_pattern_id_mask_combination");
    let drop = cx.f("shape_random_mask");
    let output = cx.i("output");
    let selected = |p: [f32; 4]| -> Option<(i32, i32)> {
        if p[3] <= 0.0 {
            return None;
        }
        let (id, material) = id_of(p[3]);
        let by_id = (ida..=idb).contains(&(id + shift));
        let by_material = (ma..=mb).contains(&(material as i32));
        let keep = match combine {
            1 => by_id && by_material,
            2 => by_id || by_material,
            _ => by_id,
        };
        (keep && rnd(id, 1, seed) >= drop).then_some((id, material as i32))
    };
    // the range the normalized outputs stretch to 0..1 is the selection's own
    let (mut low, mut high) = ([i32::MAX; 2], [i32::MIN; 2]);
    for &p in &uvw.px {
        if let Some((id, material)) = selected(p) {
            low = [low[0].min(id), low[1].min(material)];
            high = [high[0].max(id), high[1].max(material)];
        }
    }
    let scaled = |v: i32, k: usize| if high[k] > low[k] { (v - low[k]) as f32 / (high[k] - low[k]) as f32 } else { 1.0 };
    Tex::from_fn(s, hs, |u, v| {
        grey(match selected(nearest(uvw, u, v)) {
            None => 0.0,
            Some((id, material)) => match output {
                1 => id as f32,
                2 => material as f32,
                3 => scaled(id, 0),
                4 => scaled(material, 1),
                _ => 1.0,
            },
        })
    })
}

// ------------------------------------------------------------------------------------------------ grid atlas

/// Size of node `n`, given the size the graph gives it. Automatic ratio gives a Grid Atlas the grid's proportions,
/// rounded to a power of two, so grids of 3 are stretched a little. The graph decides this before it runs the node:
/// a result of any other size is resampled to the node's size.
pub fn output_size(n: &Node, size: (usize, usize)) -> (usize, usize) {
    let get = |k: &str, d: f32| n.nums.get(k).copied().unwrap_or(d).round();
    if !n.kind.starts_with("x_grid_atlas") || get("output_size_mode", 0.0) != 1.0 {
        return size;
    }
    let steps = (get("grid_size_x", 2.0).clamp(1.0, 4.0) / get("grid_size_y", 2.0).clamp(1.0, 4.0)).log2().round() as i32;
    if steps > 0 {
        (size.0, (size.1 >> steps).max(1))
    } else {
        ((size.0 >> -steps).max(1), size.1)
    }
}

/// The pictures in a grid, row by row.
fn grid_atlas(cx: &Cx, colour: bool) -> Tex {
    let (cols, rows) = (cx.i("grid_size_x").clamp(1, 4) as usize, cx.i("grid_size_y").clamp(1, 4) as usize);
    let (w, h) = (cx.s, cx.hs);
    let inputs: Vec<Option<&Tex>> = (1..=16).map(|i| cx.img(&format!("input_{i}"))).collect();
    let empty = if colour { [0.0; 4] } else { grey(0.0) };
    Tex::from_fn(w, h, |u, v| {
        let (x, y) = (u * cols as f32, v * rows as f32);
        let (c, r) = ((x as usize).min(cols - 1), (y as usize).min(rows - 1));
        match inputs[r * cols + c] {
            Some(t) => {
                let p = t.sample(x - c as f32, y - r as f32);
                if colour {
                    p
                } else {
                    grey(lum(p))
                }
            }
            None => empty,
        }
    })
}
