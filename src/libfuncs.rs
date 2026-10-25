// SPDX-License-Identifier: MIT OR Apache-2.0
//! Built-in reimplementations of Substance Designer's function presets (its `sbs://functions.sbs` library:
//! `Functions/Math/clamp`, `Functions/colorValues/HSVtoRGB`, the easings, and so on).
//!
//! These are MaterialCraft's own implementations of the standard maths — written here as expressions and turned
//! into ordinary `f_*` function graphs — so a material that drives a parameter with one of these library functions
//! renders without Adobe Substance 3D Designer installed. Each one is checked against Designer's own output (see
//! `tests/libfuncs.rs`). Nothing from Designer's package is shipped; the library file, when present, is only a
//! fallback for functions not implemented here.

use crate::fexpr::{parse, E};
use crate::graph::{Graph, Node};
use std::collections::BTreeMap;
use std::f32::consts::{PI, TAU};
use std::sync::OnceLock;

/// Input of a library function: its identifier and type (`f1`..`f4`, `i1`..`i4`, `b`).
type In = (&'static str, &'static str);

fn type_code(ty: &str) -> f32 {
    match ty {
        "f1" => 0.0,
        "i1" => 1.0,
        "b" => 2.0,
        "f2" => 3.0,
        "f3" => 4.0,
        "f4" => 5.0,
        "i2" => 6.0,
        "i3" => 7.0,
        "i4" => 8.0,
        _ => 0.0,
    }
}

/// Builds the `f_*` nodes of one library function from its expression. `names` maps a lower-case library
/// identifier to its registered `f_call` target, so inter-function calls resolve without re-entering the registry
/// while it is still being built.
struct Emit<'a> {
    g: &'a mut Graph,
    next: u32,
    vars: BTreeMap<String, u32>,
    names: &'a BTreeMap<String, &'static str>,
}

impl Emit<'_> {
    fn add(&mut self, kind: &str, ins: &[(&str, u32)]) -> u32 {
        let id = self.next;
        self.next += 1;
        let mut n = Node { kind: kind.into(), ..Default::default() };
        for (k, v) in ins {
            n.ins.insert((*k).into(), *v);
        }
        self.g.nodes.insert(id, n);
        id
    }

    fn fconst(&mut self, v: f32) -> u32 {
        let id = self.add("f_float", &[]);
        self.g.nodes.get_mut(&id).unwrap().nums.insert("v".into(), v);
        id
    }

    fn emit(&mut self, e: &E) -> Result<u32, String> {
        Ok(match e {
            E::Num(n) => self.fconst(*n),
            E::Var(name) => match name.as_str() {
                "pi" | "PI" => self.fconst(PI),
                "tau" | "TAU" => self.fconst(TAU),
                // the format's system variables
                "$pos" => self.add("f_pos", &[]),
                "$size" => self.add("f_size", &[]),
                "$sizelog2" => {
                    let s = self.add("f_size", &[]);
                    self.add("f_log2", &[("a", s)])
                }
                "$number" => self.add("f_index", &[]),
                // others ($time, $depth, ...): a variable, 0 in a parameter function
                n if n.starts_with('$') => {
                    let id = self.add("f_get", &[]);
                    self.g.nodes.get_mut(&id).unwrap().strs.insert("name".into(), n.into());
                    id
                }
                _ => *self
                    .vars
                    .get(name)
                    .or_else(|| self.vars.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v))
                    .ok_or_else(|| format!("unknown variable '{name}'"))?,
            },
            E::Neg(a) => {
                let a = self.emit(a)?;
                self.add("f_neg", &[("a", a)])
            }
            E::Not(a) => {
                let a = self.emit(a)?;
                self.add("f_not", &[("a", a)])
            }
            E::Swz(a, mask) => {
                let a = self.emit(a)?;
                let m: String = mask.chars().map(|c| match c {
                    'r' => 'x',
                    'g' => 'y',
                    'b' => 'z',
                    'a' => 'w',
                    c => c,
                }).collect();
                let id = self.add("f_swizzle", &[("a", a)]);
                self.g.nodes.get_mut(&id).unwrap().strs.insert("mask".into(), m);
                id
            }
            E::If(c, a, b) => {
                let (c, a, b) = (self.emit(c)?, self.emit(a)?, self.emit(b)?);
                self.add("f_if", &[("cond", c), ("a", a), ("b", b)])
            }
            E::Bin(op, l, r) => {
                let (a, b) = (self.emit(l)?, self.emit(r)?);
                let kind = match *op {
                    "+" => "f_add",
                    "-" => "f_sub",
                    "*" => "f_mul",
                    "/" => "f_div",
                    "%" => "f_mod",
                    "<" => "f_lt",
                    "<=" => "f_le",
                    ">" => "f_gt",
                    ">=" => "f_ge",
                    "==" => "f_eq",
                    "!=" => "f_ne",
                    "&&" => "f_and",
                    "||" => "f_or",
                    o => return Err(format!("operator '{o}' is not supported in a library function")),
                };
                self.add(kind, &[("a", a), ("b", b)])
            }
            E::Call(name, args) => self.call(name, args)?,
        })
    }

    fn call(&mut self, name: &str, args: &[E]) -> Result<u32, String> {
        let a: Vec<u32> = args.iter().map(|e| self.emit(e)).collect::<Result<_, _>>()?;
        let arity = |n: usize| if a.len() == n { Ok(()) } else { Err(format!("{name} takes {n} argument(s), got {}", a.len())) };
        // one of our f_* ops
        let un = |k: &'static str| (k, 1usize);
        let bin = |k: &'static str| (k, 2usize);
        let tri = |k: &'static str| (k, 3usize);
        let direct: Option<(&str, usize)> = match name {
            "sqrt" => Some(un("f_sqrt")),
            "abs" => Some(un("f_abs")),
            "floor" => Some(un("f_floor")),
            "ceil" => Some(un("f_ceil")),
            "round" => Some(un("f_round")),
            "fract" => Some(un("f_fract")),
            "sign" => Some(un("f_sign")),
            "saturate" => Some(un("f_saturate")),
            "sin" => Some(un("f_sin")),
            "cos" => Some(un("f_cos")),
            "tan" => Some(un("f_tan")),
            "asin" => Some(un("f_asin")),
            "acos" => Some(un("f_acos")),
            "atan" => Some(un("f_atan")),
            "exp" => Some(un("f_exp")),
            "log" => Some(un("f_log")),
            "log2" => Some(un("f_log2")),
            "radians" => Some(un("f_radians")),
            "degrees" => Some(un("f_degrees")),
            "normalize" => Some(un("f_normalize")),
            "length" => Some(un("f_length")),
            "int" => Some(un("f_to_int")),
            "float" => Some(un("f_to_float")),
            "pow" => Some(bin("f_pow")),
            "min" => Some(bin("f_min")),
            "max" => Some(bin("f_max")),
            "mod" => Some(bin("f_mod")),
            "step" => Some(bin("f_step")),
            "dot" => Some(bin("f_dot")),
            "cross" => Some(bin("f_cross")),
            "distance" => Some(bin("f_distance")),
            "atan2" => Some(bin("f_atan2")),
            "clamp" => Some(tri("f_clamp")),
            "lerp" | "mix" => Some(tri("f_lerp")),
            "smoothstep" => Some(tri("f_smoothstep")),
            _ => None,
        };
        if let Some((kind, n)) = direct {
            arity(n)?;
            let slots = ["a", "b", "c"];
            let ins: Vec<(&str, u32)> = a.iter().enumerate().map(|(i, &v)| (slots[i], v)).collect();
            return Ok(self.add(kind, &ins));
        }
        match name {
            // 2 to the power x
            "exp2" => {
                arity(1)?;
                let two = self.fconst(2.0);
                Ok(self.add("f_pow", &[("a", two), ("b", a[0])]))
            }
            // Designer's rand(max): a random float in [0, max) (low end defaults to 0)
            "rand" => {
                arity(1)?;
                Ok(self.add("f_random", &[("b", a[0])]))
            }
            // build a vector from scalar components
            "vec2" | "vec3" | "vec4" => {
                let n = name.as_bytes()[3] - b'0';
                arity(n as usize)?;
                let kind = ["f_vec2", "f_vec3", "f_vec4"][(n - 2) as usize];
                let slots = ["x", "y", "z", "w"];
                let ins: Vec<(&str, u32)> = a.iter().enumerate().map(|(i, &v)| (slots[i], v)).collect();
                Ok(self.add(kind, &ins))
            }
            // a call to another library function
            _ => {
                let callee = self.names.get(&name.to_ascii_lowercase()).copied().ok_or_else(|| format!("unknown function '{name}'"))?;
                let slots = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"];
                if a.len() > slots.len() {
                    return Err(format!("{name} has too many arguments"));
                }
                let ins: Vec<(&str, u32)> = a.iter().enumerate().map(|(i, &v)| (slots[i], v)).collect();
                let id = self.add("f_call", &ins);
                self.g.nodes.get_mut(&id).unwrap().strs.insert("fn".into(), callee.into());
                Ok(id)
            }
        }
    }
}

/// Compile one function's expression into an `f_*` function graph.
fn compile(ins: &[In], src: &str, names: &BTreeMap<String, &'static str>) -> Result<Graph, String> {
    let mut g = Graph::default();
    g.meta.insert("kind".into(), "function".into());
    let mut em = Emit { g: &mut g, next: 1, vars: BTreeMap::new(), names };
    // inputs first
    for (name, ty) in ins {
        let id = em.add("f_input", &[]);
        let n = em.g.nodes.get_mut(&id).unwrap();
        n.strs.insert("name".into(), (*name).into());
        n.nums.insert("type".into(), type_code(ty));
        em.vars.insert((*name).to_string(), id);
    }
    let (lets, result) = parse(src)?;
    for (name, e) in &lets {
        let id = em.emit(e)?;
        em.vars.insert(name.clone(), id);
    }
    let r = em.emit(&result)?;
    em.add("f_out", &[("in", r)]);
    Ok(g)
}

/// The registered `f_call` name of library function `name` (its identifier, case-insensitive), e.g. `clamp`
/// or `HSVtoRGB`; `None` if it is not implemented here.
pub fn resolve(name: &str) -> Option<&'static str> {
    library().by_ident.get(&name.to_ascii_lowercase()).copied()
}

/// The registered `f_call` name for an instance path such as `Functions/Math/clamp`, or `None`.
pub fn resolve_path(path: &str) -> Option<&'static str> {
    let ident = path.rsplit('/').next().unwrap_or(path);
    resolve(ident)
}

/// Every built-in library function as an `f_*` graph, keyed by its registered name (`lib_call` target). Merge the
/// whole set into a renderer `lib` so `f_call`s between them resolve.
pub fn graphs() -> &'static BTreeMap<String, Graph> {
    &library().graphs
}

struct Lib {
    graphs: BTreeMap<String, Graph>,
    by_ident: BTreeMap<String, &'static str>,
}

fn library() -> &'static Lib {
    static LIB: OnceLock<Lib> = OnceLock::new();
    LIB.get_or_init(build)
}

fn build() -> Lib {
    // name every function first, so an expression can call another by identifier while they are all compiled
    let mut by_ident: BTreeMap<String, &'static str> = BTreeMap::new();
    for (ident, _, _) in TABLE {
        let name: &'static str = Box::leak(format!("libfn_{ident}").into_boxed_str());
        if by_ident.insert(ident.to_ascii_lowercase(), name).is_some() {
            panic!("duplicate library function identifier '{ident}'");
        }
    }
    let mut graphs = BTreeMap::new();
    for (ident, ins, src) in TABLE {
        let name = by_ident[&ident.to_ascii_lowercase()];
        let g = compile(ins, src, &by_ident).unwrap_or_else(|e| panic!("library function '{ident}': {e}"));
        graphs.insert(name.to_string(), g);
    }
    Lib { graphs, by_ident }
}

/// (registered name = Designer identifier, inputs, expression). Keep the name exactly as Designer's function
/// identifier so `resolve_path` matches `pkg:///Functions/.../<identifier>`.
#[rustfmt::skip]
const TABLE: &[(&str, &[In], &str)] = &[
    // Functions/Cycles
    ("Wave", &[("Amplitude", "f1"), ("Frequency", "f1"), ("greaterThanZero", "b")], "w = Amplitude * sin(Frequency * $time); greaterThanZero ? Amplitude + w : w"),
    // Functions/Math
    ("Ln", &[("a", "f1")], "log(a) / exp(1)"),
    ("Pi", &[], "pi"),
    ("2Pi", &[], "tau"),
    ("Pow", &[("x", "f1"), ("n", "f1")], "x <= 0 ? 0 : pow(x, n)"),
    ("Roughness", &[("Roughness", "f1"), ("CurrentLevel", "i1"), ("TotalLevel", "i1"), ("GlobalOpacity", "f1")], "Roughness == 0 ? GlobalOpacity : (Roughness == 1 ? GlobalOpacity / float(TotalLevel) : GlobalOpacity * ((1 - Roughness) * (Roughness <= 0 ? 0 : pow(Roughness, float(CurrentLevel)))) / (1 - (Roughness <= 0 ? 0 : pow(Roughness, float(TotalLevel)))))"),
    ("Equality_Float2", &[("A", "f2"), ("B", "f2")], "A == B"),
    ("Equality_Float3", &[("A", "f3"), ("B", "f3")], "A == B"),
    ("Equality_Boolean", &[("A", "b"), ("B", "b")], "A == B"),
    ("Equality_Float4", &[("A", "f4"), ("B", "f4")], "A == B"),
    ("NotEqual_Float2", &[("A", "f2"), ("B", "f2")], "A != B"),
    ("NotEqual_Float3", &[("A", "f3"), ("B", "f3")], "A != B"),
    ("NotEqual_Float4", &[("A", "f4"), ("B", "f4")], "A != B"),
    ("NotEqual_Boolean", &[("A", "b"), ("B", "b")], "A != B"),
    ("Curve_Function", &[("curve", "f1"), ("input", "f1")], "curve < 0 ? (input * (1 + curve) - curve * sqrt(1 - (1 - input) * (1 - input))) : ((1 - sqrt(1 - input * input)) * curve + input * (1 - curve))"),
    ("merge_float4", &[("x", "f1"), ("y", "f1"), ("z", "f1"), ("w", "f1")], "vec4(x, y, z, w)"),
    ("merge_float3", &[("x", "f1"), ("y", "f1"), ("z", "f1")], "vec3(x, y, z)"),
    ("distance_vec3", &[("a", "f3"), ("b", "f3")], "distance(a, b)"),
    ("distance_vec2", &[("a", "f2"), ("b", "f2")], "distance(a, b)"),
    ("length_vec2", &[("v", "f2")], "length(v)"),
    ("length_vec3", &[("v", "f3")], "length(v)"),
    ("sign", &[("x", "f1")], "x < 0 ? 0 - 1 : 1"),
    ("reflect", &[("i", "f3"), ("n", "f3")], "i - n * (dot(i, n) * 2)"),
    ("oneminus", &[("x", "f1")], "1 - x"),
    ("fmod", &[("a", "f1"), ("b", "f1")], "a - b * float(int(a / b))"),
    ("frac", &[("input", "f1")], "fract(input)"),
    ("cross_product", &[("a", "f3"), ("b", "f3")], "cross(a, b)"),
    ("cross_product_vec2", &[("a", "f2"), ("b", "f2")], "a.x * b.y - a.y * b.x"),
    ("step", &[("a", "f1"), ("x", "f1")], "step(a, x)"),
    ("divide_float4", &[("input", "f4"), ("scalar", "f1")], "input / scalar"),
    ("divide_float3", &[("input", "f3"), ("scalar", "f1")], "input / scalar"),
    ("divide_float2", &[("input", "f2"), ("scalar", "f1")], "input / scalar"),
    ("clamp", &[("input", "f1"), ("min", "f1"), ("max", "f1")], "clamp(input, min, max)"),
    ("normalize_vec2", &[("input", "f2")], "normalize(input)"),
    ("normalize_vec3", &[("input", "f3")], "normalize(input)"),
    ("normalize_vec4", &[("input", "f4")], "normalize(input)"),
    ("smoothstep", &[("a", "f1"), ("b", "f1"), ("x", "f1")], "t = saturate((x - a) / (b - a)); t * t * (3 - 2 * t)"),
    ("saturate_float2", &[("input", "f2")], "saturate(input)"),
    ("average_float1", &[("a", "f1"), ("b", "f1")], "(a + b) * 0.5"),
    ("average_float2", &[("a", "f2"), ("b", "f2")], "(a + b) * 0.5"),
    ("average_float3", &[("a", "f3"), ("b", "f3")], "(a + b) * 0.5"),
    ("average_float4", &[("a", "f4"), ("b", "f4")], "(a + b) * 0.5"),
    ("orthogonal_vec2", &[("input", "f2")], "vec2(input.y, 0 - input.x)"),
    ("saturate", &[("input", "f1")], "saturate(input)"),
    ("truncate_float1_decimals", &[("input", "f1"), ("decimals", "i1")], "p = pow(10, float(decimals)); decimals <= 0 ? input : floor(input * p) / p"),
    ("round_float1", &[("input", "f1")], "floor(input + 0.5)"),
    // Functions/Math/Parity
    ("EvenCount", &[("Input", "f1")], "ceil(floor(Input) / 2)"),
    ("OddCount", &[("Input", "f1")], "floor(floor(Input) / 2)"),
    ("ParityTest", &[("Input", "f1")], "t = 0.25 * (2 * Input + 1); 0.5 > t - floor(t)"),
    // Functions/Math/Trigo
    ("deg_to_rad", &[("deg", "f1")], "deg * (pi / 180)"),
    ("asin", &[("x", "f1")], "asin(x)"),
    ("acos", &[("x", "f1")], "acos(x)"),
    // Functions/Random
    ("Uniform_[A,B[", &[("A", "f1"), ("B", "f1")], "rand(B - A) + A"),
    ("Discrete_[A,B]", &[("A", "f1"), ("B", "f1"), ("Probability", "f1")], "rand(1) >= Probability ? A : B"),
    ("Uniform_[-1,1[", &[], "rand(2) - 1"),
    ("Uniform_F2_[A,B[", &[("A2", "f2"), ("B2", "f2")], "d = B2 - A2; vec2(rand(d.x), rand(d.y)) + A2"),
    ("Uniform_F3_[A,B[", &[("A", "f3"), ("B", "f3")], "d = B - A; vec3(rand(d.x), rand(d.y), rand(d.z)) + A"),
    ("Uniform_F4_[A,B[", &[("A", "f4"), ("B", "f4")], "d = B - A; vec4(rand(d.x), rand(d.y), rand(d.z), rand(d.w)) + A"),
    ("Degrees_to_Turn", &[("input", "f1")], "input / 360"),
    ("GlobalRandom", &[("Input", "f1"), ("Seed", "i1")], "fract(sin(Input * 12.9898 + float(Seed) * 78.233) * 43758.5453)"),
    ("normal_distribution", &[("radius", "f1"), ("uniform_random", "f2")], "r = 0.25 * sqrt(0 - 2 * log(max(uniform_random.x, 0.0001))) * radius; t = uniform_random.y * tau; vec2(r * cos(t), r * sin(t))"),
    // Functions/Ranges
    ("[-1_1]_to_[0_1]", &[("Input", "f1")], "Input * 0.5 + 0.5"),
    ("[0_1]_to_[-1_1]", &[("Input", "f1")], "Input * 2 - 1"),
    ("[0_1]_to_[1_0]", &[("Input", "f1")], "1 - Input"),
    ("negateFloat", &[("Input", "f1")], "0 - Input"),
    ("Height_Balance_Fn", &[("Depth_Balance", "f1")], "vec4(max(Depth_Balance, 0), min(1 + Depth_Balance, 1), max(0 - Depth_Balance, 0), min(1 - Depth_Balance, 1))"),
    ("[0,1]_to_[0,1,0]", &[("input", "f1")], "1 - abs(2 * input - 1)"),
    ("[a,b]_to_[0,1]", &[("X", "f1"), ("A", "f1"), ("B", "f1")], "saturate((X - A) / (B - A))"),
    ("triangle_wave", &[("input", "f1")], "1 - abs(1 - 2 * fract(input / 2))"),
    ("sawtooth_wave", &[("x", "f1")], "m = mod(x, 1); m == 0 ? (abs(x) > 1 ? 1 : abs(x)) : m"),
    // Functions/Transforms
    ("Directional_Offset", &[("Direction", "f1"), ("Distance", "f1")], "r = Direction * tau; vec2(cos(r) * Distance, sin(r) * Distance)"),
    ("MatrixMultiply", &[("MatrixA", "f4"), ("MatrixB", "f4")], "a0 = MatrixA.x; a1 = MatrixA.y; a2 = MatrixA.z; a3 = MatrixA.w; b0 = MatrixB.x; b1 = MatrixB.y; b2 = MatrixB.z; b3 = MatrixB.w; vec4(a0 * b0 + a1 * b2, a0 * b1 + a1 * b3, a2 * b0 + a3 * b2, a2 * b1 + a3 * b3)"),
    ("RotationMatrix", &[("RotationW", "f1")], "r = RotationW * tau; vec4(cos(r), 0 - sin(r), sin(r), cos(r))"),
    ("ScaleMatrix", &[("ScaleUV", "f2")], "vec4(100 / ScaleUV.x, 0, 0, 100 / ScaleUV.y)"),
    ("TileMatrix", &[("tiles_xy", "f2")], "vec4(tiles_xy.x, 0, 0, tiles_xy.y)"),
    ("rotate_vec2", &[("vec2", "f2"), ("angle", "f1"), ("pivot", "f2")], "p = vec2 - pivot; a = angle * tau; c = cos(a); s = sin(a); vec2(p.x * c + p.y * s, p.y * c - p.x * s) + pivot"),
    ("matrix_invert", &[("matrix22", "f4")], "m0 = matrix22.x; m1 = matrix22.y; m2 = matrix22.z; m3 = matrix22.w; d = m0 * m3 - m1 * m2; vec4(m3 / d, (0 - m1) / d, (0 - m2) / d, m0 / d)"),
    ("polar_to_carthesian", &[("input", "f2")], "x = input.x; y = input.y; e = pi * (y - 0.5); vec3(cos(e) * cos(x * tau), cos(e) * sin(x * tau), 0 - sin(e))"),
    ("carthesian_to_polar", &[("input", "f3")], "x = input.x; y = input.y; z = input.z; vec2(mod(atan2(y, x) / tau, 1), 1 - (atan2(z, sqrt(x * x + y * y)) / pi + 0.5))"),
    // Functions/Various
    ("disorder", &[("disorder_radius", "f1"), ("disorder_angle", "f1")], "r = rand(disorder_radius); t = 6.28318501 * (rand(2) - 1); vec2(r * cos(t), r * sin(t))"),
    ("non_square_output_size", &[("scale", "i1"), ("output_size", "i2"), ("non_square", "b")], "output_size"),
    ("non_square_expansion_uv_scale", &[("position", "f2"), ("non_square_expansion", "b")], "sl = $sizelog2; sx = sl.x; sy = sl.y; d = max(sx, sy) - min(sx, sy); df = sx > sy ? vec2(d, 0) : vec2(0, d); p2 = vec2(exp2(df.x), exp2(df.y)); non_square_expansion ? (p2 * position - (vec2(1, 1) - p2) * 0.5) : position"),
    // Functions/colorValues
    ("directionToNormal", &[("Direction", "f1"), ("slopeAngle", "f1"), ("yUp", "b")], "off = yUp ? 0 : pi; dr = Direction * tau + off; sa = slopeAngle * tau; cx = cos(Direction * tau + pi); vec4(cx * sin(sa) * 0.5 + 0.5, sin(dr) * sin(sa) * 0.5 + 0.5, 1, 1)"),
    ("RGBtoHSL", &[("RGB", "f3")], "vec3(RGB_hue_hexagonal(rgb), RGB_saturation_HSL(rgb), RGB_lightness_bihexcone(rgb))"),
    ("HSLtoRGB", &[("HSL", "f3")], "h = HSL.x; s = HSL.y; L = HSL.z; C = (1 - abs(2 * L - 1)) * s; hp = h * 6; X = C * (1 - abs(mod(hp, 2) - 1)); m = L - C / 2; hp < 0 ? vec3(m, m, m) : (hp <= 1 ? (vec3(C, X, 0) + vec3(m, m, m)) : (hp <= 2 ? (vec3(X, C, 0) + vec3(m, m, m)) : (hp <= 3 ? (vec3(0, C, X) + vec3(m, m, m)) : (hp <= 4 ? (vec3(0, X, C) + vec3(m, m, m)) : (hp <= 5 ? (vec3(X, 0, C) + vec3(m, m, m)) : (hp < 6 ? (vec3(C, 0, X) + vec3(m, m, m)) : (vec3(m, m, m))))))))"),
    ("Random_Color", &[("RGB", "f3"), ("RGB_Randomness", "f3")], "vec3(RGB.x + (rand(1) * 2 - 1) * RGB_Randomness.x, RGB.y + (rand(1) * 2 - 1) * RGB_Randomness.y, RGB.z + (rand(1) * 2 - 1) * RGB_Randomness.z)"),
    ("Random_Luminosity", &[("RGB", "f3"), ("Luminosity_Randomness", "f1")], "off = (rand(1) * 2 - 1) * Luminosity_Randomness; vec3(RGB.x + off, RGB.y + off, RGB.z + off)"),
    ("RGBtoHSV", &[("rgb", "f3")], "vec3(RGB_hue_hexagonal(rgb), RGB_saturation_HSV(rgb), RGB_lightness_hexcone(rgb))"),
    ("RGBtoHSI", &[("rgb", "f3")], "vec3(RGB_hue_2_polar(rgb), RGB_saturation_HSI(rgb), RGB_lightness_average(rgb))"),
    ("RGBtoHCL", &[("rgb", "f3")], "vec3(RGB_hue_hexagonal(rgb), RGB_chroma_hexagonal(rgb), RGB_lightness_luma_rec601(rgb))"),
    ("RGB_lightness_average", &[("rgb", "f3")], "(rgb.x + rgb.y + rgb.z) / 3"),
    ("RGB_lightness_hexcone", &[("rgb", "f3")], "max(rgb.x, max(rgb.y, rgb.z))"),
    ("RGB_lightness_bihexcone", &[("rgb", "f3")], "(max(rgb.x, max(rgb.y, rgb.z)) + min(rgb.x, min(rgb.y, rgb.z))) * 0.5"),
    ("RGB_lightness_luma_rec601", &[("rgb", "f3")], "dot(rgb, vec3(0.299, 0.587, 0.114))"),
    ("RGB_lightness_luma_rec709", &[("rgb", "f3")], "dot(rgb, vec3(0.2126, 0.7152, 0.0722))"),
    ("RGB_chroma_hexagonal", &[("rgb", "f3")], "max(rgb.x, max(rgb.y, rgb.z)) - min(rgb.x, min(rgb.y, rgb.z))"),
    ("RGB_chroma_2_polar", &[("rgb", "f3")], "r = rgb.x; g = rgb.y; b = rgb.z; alpha = 0.5 * (2 * r - g - b); beta = (g - b) * sqrt(3) * 0.5; sqrt(alpha * alpha + beta * beta)"),
    ("RGB_hue_hexagonal", &[("rgb", "f3")], "r = rgb.x; g = rgb.y; b = rgb.z; mx = max(r, max(g, b)); mn = min(r, min(g, b)); c = mx - mn; h = c == 0 ? 0 : (r >= mx ? mod((g - b) / c, 6) : (g >= mx ? (b - r) / c + 2 : (b >= mx ? (r - g) / c + 4 : 1))); h * 60 / 360"),
    ("RGB_hue_2_polar", &[("rgb", "f3")], "r = rgb.x; g = rgb.y; b = rgb.z; alpha = 0.5 * (2 * r - g - b); beta = (g - b) * sqrt(3) * 0.5; h = atan2(beta, alpha) / pi * 0.5 + 1; mod(h, 1)"),
    ("RGB_saturation_HSV", &[("rgb", "f3")], "(max(rgb.x, max(rgb.y, rgb.z)) - min(rgb.x, min(rgb.y, rgb.z))) / max(rgb.x, max(rgb.y, rgb.z))"),
    ("RGB_saturation_HSL", &[("rgb", "f3")], "mx = max(rgb.x, max(rgb.y, rgb.z)); mn = min(rgb.x, min(rgb.y, rgb.z)); L = (mx + mn) * 0.5; (mx - mn) / (1 - abs(2 * L - 1))"),
    ("RGB_saturation_HSI", &[("rgb", "f3")], "mn = min(rgb.x, min(rgb.y, rgb.z)); I = (rgb.x + rgb.y + rgb.z) / 3; (I - mn) / I"),
    ("HSItoRGB", &[("hsi", "f3")], "H = hsi.x; S = hsi.y; I = hsi.z; lo = I * (1 - S); th = I * 3; a0 = H * tau; k0 = I * (1 + S * cos(a0) / cos(pi / 3 - a0)); a1 = (H - 1 / 3) * tau; k1 = I * (1 + S * cos(a1) / cos(pi / 3 - a1)); a2 = (H - 2 / 3) * tau; k2 = I * (1 + S * cos(a2) / cos(pi / 3 - a2)); (H >= 0 && H < 1 / 3) ? vec3(k0, th - k0 - lo, lo) : ((H >= 1 / 3 && H < 2 / 3) ? vec3(lo, k1, th - k1 - lo) : ((H >= 2 / 3 && H < 1) ? vec3(th - k2 - lo, lo, k2) : vec3(lo, lo, lo)))"),
    ("HSVtoRGB", &[("hsv", "f3")], "h = hsv.x; s = hsv.y; v = hsv.z; C = s * v; hp = h * 6; X = C * (1 - abs(mod(hp, 2) - 1)); m = v - C; hp < 0 ? vec3(m, m, m) : (hp <= 1 ? (vec3(C, X, 0) + vec3(m, m, m)) : (hp <= 2 ? (vec3(X, C, 0) + vec3(m, m, m)) : (hp <= 3 ? (vec3(0, C, X) + vec3(m, m, m)) : (hp <= 4 ? (vec3(0, X, C) + vec3(m, m, m)) : (hp <= 5 ? (vec3(X, 0, C) + vec3(m, m, m)) : (hp < 6 ? (vec3(C, 0, X) + vec3(m, m, m)) : (vec3(m, m, m))))))))"),
    ("HCLtoRGB", &[("hcl", "f3")], "h = hcl.x; C = hcl.y; L = hcl.z; hp = h * 6; X = C * (1 - abs(mod(hp, 2) - 1)); rgbp = hp < 0 ? vec3(0, 0, 0) : (hp <= 1 ? vec3(C, X, 0) : (hp <= 2 ? vec3(X, C, 0) : (hp <= 3 ? vec3(0, C, X) : (hp <= 4 ? vec3(0, X, C) : (hp <= 5 ? vec3(X, 0, C) : (hp < 6 ? vec3(C, 0, X) : (vec3(0, 0, 0)))))))); m = L - dot(rgbp, vec3(0.299, 0.587, 0.114)); rgbp + vec3(m, m, m)"),
    ("linear_to_sRGB_luminance", &[("input", "f1")], "input <= 0.0031308 ? input * 12.92 : 1.055 * pow(input, 1 / 2.4) - 0.055"),
    ("linear_to_sRGB_rgb", &[("input", "f3")], "vec3(linear_to_sRGB_luminance(input.x), linear_to_sRGB_luminance(input.y), linear_to_sRGB_luminance(input.z))"),
    ("sRGB_to_linear_rgb", &[("input", "f3")], "vec3(sRGB_to_linear_luminance(input.x), sRGB_to_linear_luminance(input.y), sRGB_to_linear_luminance(input.z))"),
    ("sRGB_to_linear_luminance", &[("input", "f1")], "input <= 0.04045 ? input / 12.92 : pow((input + 0.055) / 1.055, 2.4)"),
    ("temperature_to_rgb", &[("temperature", "f1")], "t = temperature / 100; r = temperature_to_rgb_fit(t, 0.679299712, 0.0463962592, 10.5762053, -1.51725721); g = temperature_to_rgb_fit(t, -0.267794788, 1344.49194, 0.996967077, 2.72198915); bl = temperature_to_rgb_fit(t, -0.369759142, 865.372559, 2.09022808, 1.68704808); max(vec3(r, g, bl), vec3(0, 0, 0))"),
    ("temperature_to_rgb_fit", &[("input", "f1"), ("a", "f1"), ("b", "f1"), ("c", "f1"), ("d", "f1")], "p = input <= 0 ? 0 : pow(input, d); (a * b + c * p) / (p + b)"),
    ("acescg_to_linear_srgb", &[("input", "f3")], "vec3(dot(vec3(1.70490003, -0.624159992, -0.0809141025), input), dot(vec3(-0.129553005, 1.13837004, -0.00876800995), input), dot(vec3(-0.0241260994, -0.124632999, 1.14882004), input))"),
    ("linear_srgb_to_acescg", &[("input", "f3")], "vec3(dot(vec3(0.613117993, 0.341181993, 0.0457873009), input), dot(vec3(0.0699340999, 0.91810298, 0.0119327996), input), dot(vec3(0.0204629991, 0.106769003, 0.87271601), input))"),
    ("hsl_offset", &[("rgb", "f3"), ("hue", "f1"), ("saturation", "f1"), ("lightness", "f1")], "hsl = RGBtoHSL(rgb); HSLtoRGB(vec3(mod(hsl.x + hue, 1), hsl.y + saturation, hsl.z + lightness))"),
    // Functions/easings/circ
    ("ease_in_circ", &[("input", "f1")], "1 - sqrt(1 - pow(input, 2))"),
    ("ease_out_circ", &[("input", "f1")], "sqrt(1 - pow(input - 1, 2))"),
    ("ease_in_out_circ", &[("input", "f1")], "input < 0.5 ? (1 - sqrt(1 - pow(2 * input, 2))) / 2 : (sqrt(1 - pow(-2 * input + 2, 2)) + 1) / 2"),
    // Functions/easings/cubic
    ("ease_in_cubic", &[("input", "f1")], "input * input * input"),
    ("ease_out_cubic", &[("input", "f1")], "1 - pow(1 - input, 3)"),
    ("ease_in_out_cubic", &[("input", "f1")], "input < 0.5 ? 4 * input * input * input : 1 - pow(-2 * input + 2, 3) / 2"),
    // Functions/easings/expo
    ("ease_in_expo", &[("input", "f1")], "input == 0 ? 0 : pow(2, 10 * input - 10)"),
    ("ease_out_expo", &[("input", "f1")], "input == 1 ? 1 : 1 - pow(2, -10 * input)"),
    ("ease_in_out_expo", &[("input", "f1")], "input == 0 ? 0 : (input == 1 ? 1 : (input < 0.5 ? pow(2, 20 * input - 10) / 2 : (2 - pow(2, -20 * input + 10)) / 2))"),
    // Functions/easings/quad
    ("ease_in_quad", &[("input", "f1")], "input * input"),
    ("ease_out_quad", &[("input", "f1")], "1 - (1 - input) * (1 - input)"),
    ("ease_in_out_quad", &[("input", "f1")], "input < 0.5 ? 2 * input * input : 1 - pow(-2 * input + 2, 2) / 2"),
    // Functions/easings/quart
    ("ease_in_quart", &[("input", "f1")], "input * input * input * input"),
    ("ease_out_quart", &[("input", "f1")], "1 - pow(1 - input, 4)"),
    ("ease_in_out_quart", &[("input", "f1")], "input < 0.5 ? 8 * pow(input, 4) : 1 - pow(-2 * input + 2, 4) / 2"),
    // Functions/easings/quint
    ("ease_in_quint", &[("input", "f1")], "pow(input, 5)"),
    ("ease_out_quint", &[("input", "f1")], "1 - pow(1 - input, 5)"),
    ("ease_in_out_quint", &[("input", "f1")], "input < 0.5 ? 16 * pow(input, 5) : 1 - pow(-2 * input + 2, 5) / 2"),
    // Functions/easings/sine
    ("ease_in_sine", &[("input", "f1")], "1 - cos(input * pi / 2)"),
    ("ease_out_sine", &[("input", "f1")], "sin(input * pi / 2)"),
    ("ease_in_out_sine", &[("input", "f1")], "0 - (cos(pi * input) - 1) / 2"),
    // Functions/switch/Float1
    ("switch_float1_2_inputs", &[("input_selection", "i1"), ("value_1", "f1"), ("value_2", "f1")], "input_selection == 0 ? value_1 : (value_2)"),
    ("switch_float1_4_inputs", &[("input_selection", "i1"), ("value_1", "f1"), ("value_2", "f1"), ("value_3", "f1"), ("value_4", "f1")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (value_4)))"),
    ("switch_float1_8_inputs", &[("input_selection", "i1"), ("value_1", "f1"), ("value_2", "f1"), ("value_3", "f1"), ("value_4", "f1"), ("value_5", "f1"), ("value_6", "f1"), ("value_7", "f1"), ("value_8", "f1")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (input_selection == 3 ? value_4 : (input_selection == 4 ? value_5 : (input_selection == 5 ? value_6 : (input_selection == 6 ? value_7 : (value_8)))))))"),
    // Functions/switch/Float2
    ("switch_float2_2_inputs", &[("input_selection", "i1"), ("value_1", "f2"), ("value_2", "f2")], "input_selection == 0 ? value_1 : (value_2)"),
    ("switch_float2_4_inputs", &[("input_selection", "i1"), ("value_1", "f2"), ("value_2", "f2"), ("value_3", "f2"), ("value_4", "f2")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (value_4)))"),
    ("switch_float2_8_inputs", &[("input_selection", "i1"), ("value_1", "f2"), ("value_2", "f2"), ("value_3", "f2"), ("value_4", "f2"), ("value_5", "f2"), ("value_6", "f2"), ("value_7", "f2"), ("value_8", "f2")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (input_selection == 3 ? value_4 : (input_selection == 4 ? value_5 : (input_selection == 5 ? value_6 : (input_selection == 6 ? value_7 : (value_8)))))))"),
    // Functions/switch/Float3
    ("switch_float3_2_inputs", &[("input_selection", "i1"), ("value_1", "f3"), ("value_2", "f3")], "input_selection == 0 ? value_1 : (value_2)"),
    ("switch_float3_4_inputs", &[("input_selection", "i1"), ("value_1", "f3"), ("value_2", "f3"), ("value_3", "f3"), ("value_4", "f3")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (value_4)))"),
    ("switch_float3_8_inputs", &[("input_selection", "i1"), ("value_1", "f3"), ("value_2", "f3"), ("value_3", "f3"), ("value_4", "f3"), ("value_5", "f3"), ("value_6", "f3"), ("value_7", "f3"), ("value_8", "f3")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (input_selection == 3 ? value_4 : (input_selection == 4 ? value_5 : (input_selection == 5 ? value_6 : (input_selection == 6 ? value_7 : (value_8)))))))"),
    // Functions/switch/Float4
    ("switch_float4_2_inputs", &[("input_selection", "i1"), ("value_1", "f4"), ("value_2", "f4")], "input_selection == 0 ? value_1 : (value_2)"),
    ("switch_float4_4_inputs", &[("input_selection", "i1"), ("value_1", "f4"), ("value_2", "f4"), ("value_3", "f4"), ("value_4", "f4")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (value_4)))"),
    ("switch_float4_8_inputs", &[("input_selection", "i1"), ("value_1", "f4"), ("value_2", "f4"), ("value_3", "f4"), ("value_4", "f4"), ("value_5", "f4"), ("value_6", "f4"), ("value_7", "f4"), ("value_8", "f4")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (input_selection == 3 ? value_4 : (input_selection == 4 ? value_5 : (input_selection == 5 ? value_6 : (input_selection == 6 ? value_7 : (value_8)))))))"),
    // Functions/switch/Integer1
    ("switch_integer1_2_inputs", &[("input_selection", "i1"), ("value_1", "i1"), ("value_2", "i1")], "input_selection == 0 ? value_1 : (value_2)"),
    ("switch_integer1_4_inputs", &[("input_selection", "i1"), ("value_1", "i1"), ("value_2", "i1"), ("value_3", "i1"), ("value_4", "i1")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (value_4)))"),
    ("switch_integer1_8_inputs", &[("input_selection", "i1"), ("value_1", "i1"), ("value_2", "i1"), ("value_3", "i1"), ("value_4", "i1"), ("value_5", "i1"), ("value_6", "i1"), ("value_7", "i1"), ("value_8", "i1")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (input_selection == 3 ? value_4 : (input_selection == 4 ? value_5 : (input_selection == 5 ? value_6 : (input_selection == 6 ? value_7 : (value_8)))))))"),
    // Functions/switch/Integer2
    ("switch_integer2_2_inputs", &[("input_selection", "i1"), ("value_1", "i2"), ("value_2", "i2")], "input_selection == 0 ? value_1 : (value_2)"),
    ("switch_integer2_4_inputs", &[("input_selection", "i1"), ("value_1", "i2"), ("value_2", "i2"), ("value_3", "i2"), ("value_4", "i2")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (value_4)))"),
    ("switch_integer2_8_inputs", &[("input_selection", "i1"), ("value_1", "i2"), ("value_2", "i2"), ("value_3", "i2"), ("value_4", "i2"), ("value_5", "i2"), ("value_6", "i2"), ("value_7", "i2"), ("value_8", "i2")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (input_selection == 3 ? value_4 : (input_selection == 4 ? value_5 : (input_selection == 5 ? value_6 : (input_selection == 6 ? value_7 : (value_8)))))))"),
    // Functions/switch/Integer3
    ("switch_integer3_2_inputs", &[("input_selection", "i1"), ("value_1", "i3"), ("value_2", "i3")], "input_selection == 0 ? value_1 : (value_2)"),
    ("switch_integer3_4_inputs", &[("input_selection", "i1"), ("value_1", "i3"), ("value_2", "i3"), ("value_3", "i3"), ("value_4", "i3")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (value_4)))"),
    ("switch_integer3_8_inputs", &[("input_selection", "i1"), ("value_1", "i3"), ("value_2", "i3"), ("value_3", "i3"), ("value_4", "i3"), ("value_5", "i3"), ("value_6", "i3"), ("value_7", "i3"), ("value_8", "i3")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (input_selection == 3 ? value_4 : (input_selection == 4 ? value_5 : (input_selection == 5 ? value_6 : (input_selection == 6 ? value_7 : (value_8)))))))"),
    // Functions/switch/Integer4
    ("switch_integer4_2_inputs", &[("input_selection", "i1"), ("value_1", "i4"), ("value_2", "i4")], "input_selection == 0 ? value_1 : (value_2)"),
    ("switch_integer4_4_inputs", &[("input_selection", "i1"), ("value_1", "i4"), ("value_2", "i4"), ("value_3", "i4"), ("value_4", "i4")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (value_4)))"),
    ("switch_integer4_8_inputs", &[("input_selection", "i1"), ("value_1", "i4"), ("value_2", "i4"), ("value_3", "i4"), ("value_4", "i4"), ("value_5", "i4"), ("value_6", "i4"), ("value_7", "i4"), ("value_8", "i4")], "input_selection == 0 ? value_1 : (input_selection == 1 ? value_2 : (input_selection == 2 ? value_3 : (input_selection == 3 ? value_4 : (input_selection == 4 ? value_5 : (input_selection == 5 ? value_6 : (input_selection == 6 ? value_7 : (value_8)))))))"),
    // Functions/typeConverters
    ("booleanToFloat1", &[("Boolean", "b")], "Boolean ? 1 : 0"),
    ("turnsToDegrees", &[("turns", "f1")], "turns * 360"),
];
