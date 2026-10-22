// SPDX-License-Identifier: MIT OR Apache-2.0
//! Function graphs: small value-computing graphs that drive parameters, pixel processors and pattern programs.
//!
//! A function graph is an ordinary [`Graph`] stored next to the texture graphs of a package (it carries the meta
//! entry `kind "function"`). Its nodes are the `f_*` kinds below. Links carry values instead of textures: floats and
//! float vectors, integers and integer vectors, booleans and strings. One `f_out` node returns the result.
//!
//! Rules that keep results predictable:
//! * integers only widen to floats; a float needs `f_to_int` to become an integer;
//! * a scalar combines with a vector of any size, two vectors need the same size;
//! * no operation produces NaN or infinity: division by zero, `sqrt` of a negative number, `log` of zero and the like
//!   give 0, inverse trigonometric inputs are clamped;
//! * nodes are evaluated on demand and once per state of the variables (`f_set` starts a new state).

use crate::graph::{Graph, Pin, Spec};
use crate::noise::rnd;
use crate::tex::{lum, Tex};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

// ------------------------------------------------------------------------------------------------ values

/// A numeric value: one to four lanes, integer or float.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Num {
    pub lanes: u8,
    pub int: bool,
    pub v: [f64; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(Num),
    Bool(bool),
    Str(Arc<str>),
}

impl Value {
    pub fn float(x: f64) -> Value {
        Value::Num(Num { lanes: 1, int: false, v: [x, 0.0, 0.0, 0.0] })
    }
    pub fn int(x: i64) -> Value {
        Value::Num(Num { lanes: 1, int: true, v: [x as f64, 0.0, 0.0, 0.0] })
    }
    pub fn vec(lanes: u8, v: [f64; 4]) -> Value {
        Value::Num(Num { lanes: lanes.clamp(1, 4), int: false, v })
    }
    pub fn text(s: &str) -> Value {
        Value::Str(Arc::from(s))
    }

    /// The pin type of this value.
    pub fn pin(&self) -> Pin {
        match self {
            Value::Bool(_) => Pin::Bool,
            Value::Str(_) => Pin::Text,
            Value::Num(n) => match (n.lanes, n.int) {
                (1, false) => Pin::Float,
                (2, false) => Pin::Float2,
                (3, false) => Pin::Float3,
                (_, false) => Pin::Float4,
                (1, true) => Pin::Int,
                (2, true) => Pin::Int2,
                (3, true) => Pin::Int3,
                (_, true) => Pin::Int4,
            },
        }
    }

    /// First lane as f32 (booleans are 0 / 1, strings 0): how a value drives a numeric parameter.
    pub fn as_f32(&self) -> f32 {
        match self {
            Value::Num(n) => n.v[0] as f32,
            Value::Bool(b) => *b as i32 as f32,
            Value::Str(_) => 0.0,
        }
    }

    fn num(&self) -> Result<Num, String> {
        match self {
            Value::Num(n) => Ok(*n),
            Value::Bool(b) => Ok(Num { lanes: 1, int: true, v: [*b as i32 as f64, 0.0, 0.0, 0.0] }),
            Value::Str(_) => Err("a string cannot be used as a number".into()),
        }
    }

    /// Text shown in the editor and produced by `f_to_string`.
    pub fn show(&self) -> String {
        fn f(x: f64, int: bool) -> String {
            if int {
                format!("{}", x as i64)
            } else if x == x.trunc() && x.abs() < 1e9 {
                format!("{x:.1}")
            } else {
                format!("{x:.4}").trim_end_matches('0').trim_end_matches('.').to_string()
            }
        }
        match self {
            Value::Bool(b) => b.to_string(),
            Value::Str(s) => s.to_string(),
            Value::Num(n) if n.lanes == 1 => f(n.v[0], n.int),
            Value::Num(n) => format!("({})", (0..n.lanes as usize).map(|i| f(n.v[i], n.int)).collect::<Vec<_>>().join(", ")),
        }
    }
}

/// A value of the given type with every lane 1: used to find the result type of a node without running it.
fn dummy(p: Pin) -> Value {
    match p {
        Pin::Bool => Value::Bool(true),
        Pin::Text => Value::text("x"),
        Pin::Float | Pin::Float2 | Pin::Float3 | Pin::Float4 => Value::Num(Num { lanes: p.lanes() as u8, int: false, v: [1.0; 4] }),
        Pin::Int | Pin::Int2 | Pin::Int3 | Pin::Int4 => Value::Num(Num { lanes: p.lanes() as u8, int: true, v: [1.0; 4] }),
        _ => Value::float(1.0),
    }
}

/// Pin type of a declared input / constant (`type` number): 0 float, 1 int, 2 bool, 3 float2, 4 float3, 5 float4,
/// 6 int2, 7 int3, 8 int4, 9 string.
pub fn type_pin(t: i32) -> Pin {
    match t {
        1 => Pin::Int,
        2 => Pin::Bool,
        3 => Pin::Float2,
        4 => Pin::Float3,
        5 => Pin::Float4,
        6 => Pin::Int2,
        7 => Pin::Int3,
        8 => Pin::Int4,
        9 => Pin::Text,
        _ => Pin::Float,
    }
}

/// Convert `v` to the declared type `p` the way an input does (a number fills the lanes, a vector is cut or padded).
pub fn coerce(v: &Value, p: Pin) -> Value {
    match (p, v) {
        (Pin::Bool, _) => Value::Bool(v.as_f32() != 0.0),
        (Pin::Text, _) => Value::text(&v.show()),
        (_, Value::Str(_)) => dummy(p).clone_zero(),
        (_, _) => {
            let n = v.num().unwrap_or(Num { lanes: 1, int: false, v: [0.0; 4] });
            let lanes = p.lanes().max(1) as u8;
            let int = matches!(p, Pin::Int | Pin::Int2 | Pin::Int3 | Pin::Int4);
            let mut out = [0.0; 4];
            for (i, o) in out.iter_mut().enumerate().take(lanes as usize) {
                let x = if n.lanes == 1 { n.v[0] } else if i < n.lanes as usize { n.v[i] } else { 0.0 };
                *o = if int { x.trunc() } else { x };
            }
            Value::Num(Num { lanes, int, v: out })
        }
    }
}

impl Value {
    fn clone_zero(&self) -> Value {
        match self {
            Value::Num(n) => Value::Num(Num { v: [0.0; 4], ..*n }),
            Value::Bool(_) => Value::Bool(false),
            Value::Str(_) => Value::text(""),
        }
    }
}

// ------------------------------------------------------------------------------------------------ node table

macro_rules! f {
    ($k:expr, [$($r:expr),*], [$($o:expr),*], $d:expr) => {
        Spec { kind: $k, req: &[$($r),*], opt: &[$($o),*], doc: $d }
    };
}

/// Node kinds of function graphs. Slots: `a`, `b`, `c` for operands, `x y z w` for vector parts, `cond` for `f_if`.
pub const FUNC_SPECS: &[Spec] = &[
    f!("f_out", ["in"], [], "result of the function"),
    f!("f_pin", ["a"], [], "re-route a value link"),
    f!("f_float", [], [], "constant float: v"),
    f!("f_int", [], [], "constant integer: v"),
    f!("f_bool", [], [], "constant boolean: v (0 or 1)"),
    f!("f_string", [], [], "constant string: v"),
    f!("f_vec2", [], ["x", "y"], "float2 from parts (x y, linked parts win over the typed values)"),
    f!("f_vec3", [], ["x", "y", "z"], "float3 from parts"),
    f!("f_vec4", [], ["x", "y", "z", "w"], "float4 from parts"),
    f!("f_ivec2", [], ["x", "y"], "integer2 from parts"),
    f!("f_ivec3", [], ["x", "y", "z"], "integer3 from parts"),
    f!("f_ivec4", [], ["x", "y", "z", "w"], "integer4 from parts"),
    f!("f_swizzle", ["a"], [], "pick or reorder components: mask (xyzw or rgba, e.g. \"zyx\")"),
    f!("f_neg", ["a"], [], "negate"),
    f!("f_abs", ["a"], [], "absolute value"),
    f!("f_sqrt", ["a"], [], "square root (0 for negative input)"),
    f!("f_exp", ["a"], [], "e to the power a"),
    f!("f_log", ["a"], [], "natural logarithm (0 for input <= 0)"),
    f!("f_log2", ["a"], [], "base-2 logarithm (0 for input <= 0)"),
    f!("f_sin", ["a"], [], "sine (radians)"),
    f!("f_cos", ["a"], [], "cosine (radians)"),
    f!("f_tan", ["a"], [], "tangent (radians)"),
    f!("f_asin", ["a"], [], "inverse sine (input clamped to -1..1)"),
    f!("f_acos", ["a"], [], "inverse cosine (input clamped to -1..1)"),
    f!("f_atan", ["a"], [], "inverse tangent"),
    f!("f_radians", ["a"], [], "degrees to radians"),
    f!("f_degrees", ["a"], [], "radians to degrees"),
    f!("f_floor", ["a"], [], "round down"),
    f!("f_ceil", ["a"], [], "round up"),
    f!("f_round", ["a"], [], "round to nearest"),
    f!("f_fract", ["a"], [], "fractional part"),
    f!("f_sign", ["a"], [], "-1, 0 or 1"),
    f!("f_saturate", ["a"], [], "clamp to 0..1"),
    f!("f_not", ["a"], [], "boolean not"),
    f!("f_length", ["a"], [], "length of a vector"),
    f!("f_normalize", ["a"], [], "vector of length 1 (zero stays zero)"),
    f!("f_add", ["a", "b"], [], "a + b"),
    f!("f_sub", ["a", "b"], [], "a - b"),
    f!("f_mul", ["a", "b"], [], "a * b"),
    f!("f_div", ["a", "b"], [], "a / b (0 when b is 0; integer division truncates)"),
    f!("f_mod", ["a", "b"], [], "remainder of a / b with the sign of b (0 when b is 0)"),
    f!("f_pow", ["a", "b"], [], "a to the power b"),
    f!("f_min", ["a", "b"], [], "smaller value"),
    f!("f_max", ["a", "b"], [], "larger value"),
    f!("f_atan2", ["a", "b"], [], "angle of the point (b, a)"),
    f!("f_step", ["a", "b"], [], "0 when b < a, else 1"),
    f!("f_dot", ["a", "b"], [], "dot product"),
    f!("f_cross", ["a", "b"], [], "cross product of two float3"),
    f!("f_distance", ["a", "b"], [], "distance between two points"),
    f!("f_clamp", ["a", "b", "c"], [], "clamp a between b (low) and c (high)"),
    f!("f_lerp", ["a", "b", "c"], [], "a + (b - a) * c"),
    f!("f_smoothstep", ["a", "b", "c"], [], "smooth 0..1 ramp of c between edges a and b"),
    f!("f_eq", ["a", "b"], [], "a == b"),
    f!("f_ne", ["a", "b"], [], "a != b"),
    f!("f_lt", ["a", "b"], [], "a < b"),
    f!("f_le", ["a", "b"], [], "a <= b"),
    f!("f_gt", ["a", "b"], [], "a > b"),
    f!("f_ge", ["a", "b"], [], "a >= b"),
    f!("f_and", ["a", "b"], [], "boolean and"),
    f!("f_or", ["a", "b"], [], "boolean or"),
    f!("f_xor", ["a", "b"], [], "boolean exclusive or"),
    f!("f_if", ["cond", "a", "b"], [], "a when cond is true, else b (only the chosen branch runs)"),
    f!("f_sequence", ["a", "b"], [], "run a, then b; returns b"),
    f!("f_while", ["a", "b", "c"], [], "run a once, then repeat c until b is true (at most `max` times, -1 no limit); returns the last c (a when c never ran)"),
    f!("f_set", ["a"], [], "store a in the variable `name`; returns a"),
    f!("f_get", [], [], "read the variable `name` (0 until it is set)"),
    f!("f_random", [], ["a", "b"], "random float between a (default 0) and b (default 1); `seed` shifts the sequence"),
    f!("f_sample_gray", ["a"], [], "brightness of input image `slot` (default in) at the position a (float2, 0..1)"),
    f!("f_sample_color", ["a"], [], "colour (float4) of input image `slot` at the position a"),
    f!("f_image_avg", [], [], "average colour (float4) of input image `slot`"),
    f!("f_image_min", [], [], "smallest channel values (float4) of input image `slot`"),
    f!("f_image_max", [], [], "largest channel values (float4) of input image `slot`"),
    f!("f_pos", [], [], "position being evaluated (float2, 0..1)"),
    f!("f_size", [], [], "output size in pixels (float2)"),
    f!("f_index", [], [], "instance / iteration number (integer)"),
    f!("f_count", [], [], "number of instances / iterations (integer)"),
    f!("f_seed", [], [], "random seed of the caller (integer)"),
    f!("f_input", [], [], "input of this function: name, type (0 float, 1 int, 2 bool, 3 float2, 4 float3, 5 float4, 6 int2, 7 int3, 8 int4, 9 string), default v / x y z w"),
    f!("f_graph_input", [], [], "value of an Input Value node of the graph the caller lives in: name, type, default v / x y z w"),
    f!("f_to_float", ["a"], [], "convert to float"),
    f!("f_to_int", ["a"], [], "convert to integer (drops the fraction)"),
    f!("f_to_bool", ["a"], [], "non-zero is true"),
    f!("f_to_string", ["a"], [], "text of a value"),
    f!("f_concat", ["a", "b"], [], "join two strings"),
    f!("f_call", [], ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"], "run another function (`fn`); a b c d ... l fill its inputs in the order of their node numbers"),
];

/// Is this kind a function-graph node?
pub fn is_func_kind(kind: &str) -> bool {
    kind.starts_with("f_")
}

/// True when a graph is a function graph (meta `kind "function"`).
pub fn is_function(g: &Graph) -> bool {
    g.meta.get("kind").map(|k| k == "function").unwrap_or(false)
}

// ------------------------------------------------------------------------------------------------ operations

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Out,
    Pin,
    Float,
    Int,
    Bool,
    Str,
    Vec(u8, bool),
    Swizzle,
    Un(U),
    Bin(B),
    Clamp,
    Lerp,
    Smooth,
    If,
    Seq,
    While,
    Set,
    Get,
    Random,
    SampleGray,
    SampleColor,
    Stat(u8),
    Pos,
    Size,
    Index,
    Count,
    Seed,
    Input,
    GraphInput,
    ToFloat,
    ToInt,
    ToBool,
    ToStr,
    Concat,
    Call,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum U {
    Neg,
    Abs,
    Sqrt,
    Exp,
    Log,
    Log2,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Rad,
    Deg,
    Floor,
    Ceil,
    Round,
    Fract,
    Sign,
    Sat,
    Not,
    Length,
    Normalize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum B {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Min,
    Max,
    Atan2,
    Step,
    Dot,
    Cross,
    Distance,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Xor,
}

fn op_of(kind: &str) -> Option<Op> {
    use B::*;
    use U::*;
    Some(match kind {
        "f_out" => Op::Out,
        "f_pin" => Op::Pin,
        "f_float" => Op::Float,
        "f_int" => Op::Int,
        "f_bool" => Op::Bool,
        "f_string" => Op::Str,
        "f_vec2" => Op::Vec(2, false),
        "f_vec3" => Op::Vec(3, false),
        "f_vec4" => Op::Vec(4, false),
        "f_ivec2" => Op::Vec(2, true),
        "f_ivec3" => Op::Vec(3, true),
        "f_ivec4" => Op::Vec(4, true),
        "f_swizzle" => Op::Swizzle,
        "f_neg" => Op::Un(Neg),
        "f_abs" => Op::Un(Abs),
        "f_sqrt" => Op::Un(Sqrt),
        "f_exp" => Op::Un(Exp),
        "f_log" => Op::Un(Log),
        "f_log2" => Op::Un(Log2),
        "f_sin" => Op::Un(Sin),
        "f_cos" => Op::Un(Cos),
        "f_tan" => Op::Un(Tan),
        "f_asin" => Op::Un(Asin),
        "f_acos" => Op::Un(Acos),
        "f_atan" => Op::Un(Atan),
        "f_radians" => Op::Un(Rad),
        "f_degrees" => Op::Un(Deg),
        "f_floor" => Op::Un(Floor),
        "f_ceil" => Op::Un(Ceil),
        "f_round" => Op::Un(Round),
        "f_fract" => Op::Un(Fract),
        "f_sign" => Op::Un(Sign),
        "f_saturate" => Op::Un(Sat),
        "f_not" => Op::Un(Not),
        "f_length" => Op::Un(Length),
        "f_normalize" => Op::Un(Normalize),
        "f_add" => Op::Bin(Add),
        "f_sub" => Op::Bin(Sub),
        "f_mul" => Op::Bin(Mul),
        "f_div" => Op::Bin(Div),
        "f_mod" => Op::Bin(Mod),
        "f_pow" => Op::Bin(Pow),
        "f_min" => Op::Bin(Min),
        "f_max" => Op::Bin(Max),
        "f_atan2" => Op::Bin(Atan2),
        "f_step" => Op::Bin(Step),
        "f_dot" => Op::Bin(Dot),
        "f_cross" => Op::Bin(Cross),
        "f_distance" => Op::Bin(Distance),
        "f_eq" => Op::Bin(Eq),
        "f_ne" => Op::Bin(Ne),
        "f_lt" => Op::Bin(Lt),
        "f_le" => Op::Bin(Le),
        "f_gt" => Op::Bin(Gt),
        "f_ge" => Op::Bin(Ge),
        "f_and" => Op::Bin(And),
        "f_or" => Op::Bin(Or),
        "f_xor" => Op::Bin(Xor),
        "f_clamp" => Op::Clamp,
        "f_lerp" => Op::Lerp,
        "f_smoothstep" => Op::Smooth,
        "f_if" => Op::If,
        "f_sequence" => Op::Seq,
        "f_while" => Op::While,
        "f_set" => Op::Set,
        "f_get" => Op::Get,
        "f_random" => Op::Random,
        "f_sample_gray" => Op::SampleGray,
        "f_sample_color" => Op::SampleColor,
        "f_image_avg" => Op::Stat(0),
        "f_image_min" => Op::Stat(1),
        "f_image_max" => Op::Stat(2),
        "f_pos" => Op::Pos,
        "f_size" => Op::Size,
        "f_index" => Op::Index,
        "f_count" => Op::Count,
        "f_seed" => Op::Seed,
        "f_input" => Op::Input,
        "f_graph_input" => Op::GraphInput,
        "f_to_float" => Op::ToFloat,
        "f_to_int" => Op::ToInt,
        "f_to_bool" => Op::ToBool,
        "f_to_string" => Op::ToStr,
        "f_concat" => Op::Concat,
        "f_call" => Op::Call,
        _ => return None,
    })
}

fn clean(x: f64) -> f64 {
    if x.is_finite() {
        x
    } else {
        0.0
    }
}

fn lanes_of(a: &Num, b: &Num) -> Result<u8, String> {
    if a.lanes == b.lanes {
        Ok(a.lanes)
    } else if a.lanes == 1 {
        Ok(b.lanes)
    } else if b.lanes == 1 {
        Ok(a.lanes)
    } else {
        Err(format!("cannot combine {} lanes with {} lanes", a.lanes, b.lanes))
    }
}

fn lane(n: &Num, i: usize) -> f64 {
    if n.lanes == 1 {
        n.v[0]
    } else {
        n.v[i]
    }
}

fn map1(a: &Num, int_out: bool, f: impl Fn(f64) -> f64) -> Value {
    let mut v = [0.0; 4];
    for (i, o) in v.iter_mut().enumerate().take(a.lanes as usize) {
        let r = clean(f(a.v[i]));
        *o = if int_out { r.trunc() } else { r };
    }
    Value::Num(Num { lanes: a.lanes, int: int_out, v })
}

fn map2(a: &Num, b: &Num, int_out: bool, f: impl Fn(f64, f64) -> f64) -> Result<Value, String> {
    let lanes = lanes_of(a, b)?;
    let mut v = [0.0; 4];
    for (i, o) in v.iter_mut().enumerate().take(lanes as usize) {
        let r = clean(f(lane(a, i), lane(b, i)));
        *o = if int_out { r.trunc() } else { r };
    }
    Ok(Value::Num(Num { lanes, int: int_out, v }))
}

fn unary(op: U, x: &Value) -> Result<Value, String> {
    if op == U::Not {
        return match x {
            Value::Bool(b) => Ok(Value::Bool(!b)),
            _ => Err("f_not needs a boolean".into()),
        };
    }
    let a = x.num()?;
    let keep_int = a.int;
    Ok(match op {
        U::Neg => map1(&a, keep_int, |x| -x),
        U::Abs => map1(&a, keep_int, f64::abs),
        U::Sign => map1(&a, keep_int, |x| if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 }),
        U::Floor => map1(&a, keep_int, f64::floor),
        U::Ceil => map1(&a, keep_int, f64::ceil),
        U::Round => map1(&a, keep_int, f64::round),
        U::Sqrt => map1(&a, false, |x| x.max(0.0).sqrt()),
        U::Exp => map1(&a, false, |x| x.min(80.0).exp()),
        U::Log => map1(&a, false, |x| if x > 0.0 { x.ln() } else { 0.0 }),
        U::Log2 => map1(&a, false, |x| if x > 0.0 { x.log2() } else { 0.0 }),
        U::Sin => map1(&a, false, f64::sin),
        U::Cos => map1(&a, false, f64::cos),
        U::Tan => map1(&a, false, f64::tan),
        U::Asin => map1(&a, false, |x| x.clamp(-1.0, 1.0).asin()),
        U::Acos => map1(&a, false, |x| x.clamp(-1.0, 1.0).acos()),
        U::Atan => map1(&a, false, f64::atan),
        U::Rad => map1(&a, false, f64::to_radians),
        U::Deg => map1(&a, false, f64::to_degrees),
        U::Fract => map1(&a, false, |x| x - x.floor()),
        U::Sat => map1(&a, false, |x| x.clamp(0.0, 1.0)),
        U::Length => {
            let s: f64 = (0..a.lanes as usize).map(|i| a.v[i] * a.v[i]).sum();
            Value::float(clean(s.sqrt()))
        }
        U::Normalize => {
            let s: f64 = (0..a.lanes as usize).map(|i| a.v[i] * a.v[i]).sum::<f64>().sqrt();
            if s > 1e-12 {
                map1(&Num { int: false, ..a }, false, |x| x / s)
            } else {
                map1(&Num { int: false, ..a }, false, |_| 0.0)
            }
        }
        U::Not => unreachable!(),
    })
}

fn binary(op: B, x: &Value, y: &Value) -> Result<Value, String> {
    match op {
        B::And | B::Or | B::Xor => {
            return match (x, y) {
                (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(match op {
                    B::And => *a && *b,
                    B::Or => *a || *b,
                    _ => a != b,
                })),
                _ => Err("boolean operators need two booleans".into()),
            }
        }
        B::Eq | B::Ne => {
            let same = match (x, y) {
                (Value::Str(a), Value::Str(b)) => a == b,
                (Value::Bool(a), Value::Bool(b)) => a == b,
                (Value::Str(_), _) | (_, Value::Str(_)) => return Err("cannot compare a string with a number".into()),
                _ => {
                    let (a, b) = (x.num()?, y.num()?);
                    let lanes = lanes_of(&a, &b)?;
                    (0..lanes as usize).all(|i| lane(&a, i) == lane(&b, i))
                }
            };
            return Ok(Value::Bool(if op == B::Eq { same } else { !same }));
        }
        _ => {}
    }
    let (a, b) = (x.num()?, y.num()?);
    let both_int = a.int && b.int;
    match op {
        B::Add => map2(&a, &b, both_int, |p, q| p + q),
        B::Sub => map2(&a, &b, both_int, |p, q| p - q),
        B::Mul => map2(&a, &b, both_int, |p, q| p * q),
        B::Div => map2(&a, &b, both_int, |p, q| if q == 0.0 { 0.0 } else if both_int { (p / q).trunc() } else { p / q }),
        B::Mod => map2(&a, &b, both_int, |p, q| if q == 0.0 { 0.0 } else { p - q * (p / q).floor() }),
        B::Pow => map2(&a, &b, false, |p, q| {
            let r = p.powf(q);
            if p < 0.0 && q.fract() != 0.0 {
                0.0
            } else {
                r
            }
        }),
        B::Min => map2(&a, &b, both_int, f64::min),
        B::Max => map2(&a, &b, both_int, f64::max),
        B::Atan2 => map2(&a, &b, false, f64::atan2),
        B::Step => map2(&a, &b, false, |e, v| if v < e { 0.0 } else { 1.0 }),
        B::Lt | B::Le | B::Gt | B::Ge => {
            if a.lanes != 1 || b.lanes != 1 {
                return Err("ordering comparisons need scalars".into());
            }
            let (p, q) = (a.v[0], b.v[0]);
            Ok(Value::Bool(match op {
                B::Lt => p < q,
                B::Le => p <= q,
                B::Gt => p > q,
                _ => p >= q,
            }))
        }
        B::Dot => {
            let lanes = lanes_of(&a, &b)?;
            Ok(Value::float(clean((0..lanes as usize).map(|i| lane(&a, i) * lane(&b, i)).sum())))
        }
        B::Distance => {
            let lanes = lanes_of(&a, &b)?;
            Ok(Value::float(clean((0..lanes as usize).map(|i| (lane(&a, i) - lane(&b, i)).powi(2)).sum::<f64>().sqrt())))
        }
        B::Cross => {
            if a.lanes != 3 || b.lanes != 3 {
                return Err("f_cross needs two float3 values".into());
            }
            let (p, q) = (a.v, b.v);
            Ok(Value::vec(3, [p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0], 0.0]))
        }
        B::And | B::Or | B::Xor | B::Eq | B::Ne => unreachable!(),
    }
}

fn clamp3(x: &Value, lo: &Value, hi: &Value) -> Result<Value, String> {
    let (a, b, c) = (x.num()?, lo.num()?, hi.num()?);
    let lanes = lanes_of(&a, &lanes_num(&b, &c)?)?;
    let int = a.int && b.int && c.int;
    let mut v = [0.0; 4];
    for (i, o) in v.iter_mut().enumerate().take(lanes as usize) {
        let (l, h) = (lane(&b, i), lane(&c, i));
        *o = clean(lane(&a, i).max(l.min(h)).min(l.max(h)));
    }
    Ok(Value::Num(Num { lanes, int, v }))
}

fn lanes_num(a: &Num, b: &Num) -> Result<Num, String> {
    Ok(Num { lanes: lanes_of(a, b)?, int: false, v: [0.0; 4] })
}

fn lerp3(a: &Value, b: &Value, t: &Value) -> Result<Value, String> {
    let (a, b, t) = (a.num()?, b.num()?, t.num()?);
    let lanes = lanes_of(&lanes_num(&a, &b)?, &t)?;
    let mut v = [0.0; 4];
    for (i, o) in v.iter_mut().enumerate().take(lanes as usize) {
        let (p, q) = (lane(&a, i), lane(&b, i));
        *o = clean(p + (q - p) * lane(&t, i));
    }
    Ok(Value::Num(Num { lanes, int: false, v }))
}

fn smooth3(e0: &Value, e1: &Value, x: &Value) -> Result<Value, String> {
    let (a, b, x) = (e0.num()?, e1.num()?, x.num()?);
    let lanes = lanes_of(&lanes_num(&a, &b)?, &x)?;
    let mut v = [0.0; 4];
    for (i, o) in v.iter_mut().enumerate().take(lanes as usize) {
        let (lo, hi) = (lane(&a, i), lane(&b, i));
        let t = if (hi - lo).abs() < 1e-12 { if lane(&x, i) < lo { 0.0 } else { 1.0 } } else { ((lane(&x, i) - lo) / (hi - lo)).clamp(0.0, 1.0) };
        *o = t * t * (3.0 - 2.0 * t);
    }
    Ok(Value::Num(Num { lanes, int: false, v }))
}

fn swizzle(a: &Value, mask: &str) -> Result<Value, String> {
    let n = a.num()?;
    if mask.is_empty() || mask.chars().count() > 4 {
        return Err("a swizzle mask has 1 to 4 letters".into());
    }
    let mut v = [0.0; 4];
    let mut lanes = 0u8;
    for c in mask.chars() {
        let i = match c {
            'x' | 'r' => 0,
            'y' | 'g' => 1,
            'z' | 'b' => 2,
            'w' | 'a' => 3,
            _ => return Err(format!("bad swizzle letter '{c}'")),
        };
        if i >= n.lanes as usize && !(n.lanes == 1 && i == 0) {
            return Err(format!("component '{c}' does not exist in a value with {} lane(s)", n.lanes));
        }
        v[lanes as usize] = n.v[i];
        lanes += 1;
    }
    Ok(Value::Num(Num { lanes, int: n.int, v }))
}

fn build_vec(lanes: u8, int: bool, parts: &[Option<Value>; 4], defaults: [f64; 4]) -> Result<Value, String> {
    let mut v = [0.0; 4];
    let mut at = 0usize;
    let mut slot = 0usize;
    // a linked part may itself be a vector: its lanes fill the following components
    while at < lanes as usize && slot < 4 {
        match &parts[slot] {
            Some(p) => {
                let n = p.num()?;
                for k in 0..n.lanes as usize {
                    if at < lanes as usize {
                        v[at] = n.v[k];
                        at += 1;
                    }
                }
            }
            None => {
                v[at] = defaults[at];
                at += 1;
            }
        }
        slot += 1;
    }
    while at < lanes as usize {
        v[at] = defaults[at];
        at += 1;
    }
    if int {
        for x in v.iter_mut() {
            *x = x.trunc();
        }
    }
    Ok(Value::Num(Num { lanes, int, v }))
}

/// Result type of applying a pure operation to inputs of the given types (used by the editor and the checker).
fn infer_pure(op: Op, args: &[Option<Pin>], mask: &str) -> Result<Pin, String> {
    let d: Vec<Option<Value>> = args.iter().map(|p| p.map(dummy)).collect();
    let get = |i: usize| -> Result<&Value, String> { d.get(i).and_then(|v| v.as_ref()).ok_or_else(|| "an input is not connected".to_string()) };
    let r = match op {
        Op::Un(u) => unary(u, get(0)?)?,
        Op::Bin(b) => binary(b, get(0)?, get(1)?)?,
        Op::Clamp => clamp3(get(0)?, get(1)?, get(2)?)?,
        Op::Lerp => lerp3(get(0)?, get(1)?, get(2)?)?,
        Op::Smooth => smooth3(get(0)?, get(1)?, get(2)?)?,
        Op::Swizzle => swizzle(get(0)?, mask)?,
        Op::ToFloat => coerce(&Value::Num(get(0)?.num()?), Pin::Float),
        Op::ToInt => {
            let n = get(0)?.num()?;
            Value::Num(Num { int: true, ..n })
        }
        Op::ToBool => Value::Bool(true),
        Op::ToStr => Value::text(""),
        Op::Concat => match (get(0)?, get(1)?) {
            (Value::Str(_), Value::Str(_)) => Value::text(""),
            _ => return Err("f_concat needs two strings".into()),
        },
        _ => return Err("not a pure operation".into()),
    };
    Ok(r.pin())
}

// ------------------------------------------------------------------------------------------------ programs

/// What the caller provides to a running function.
pub struct FnEnv<'a> {
    pub size: [f64; 2],
    pub index: i64,
    pub count: i64,
    pub seed: u32,
    /// input images by slot name (`in`, `in2`, ...), read by the sampling nodes
    pub images: Vec<(&'a str, &'a Tex)>,
    /// values of the Input Value nodes of the graph the caller lives in
    pub graph_inputs: &'a BTreeMap<String, Value>,
    /// the function's own inputs (`f_input`)
    pub args: BTreeMap<String, Value>,
    stats: Vec<(String, [f32; 4], [f32; 4], [f32; 4])>,
}

static NO_INPUTS: BTreeMap<String, Value> = BTreeMap::new();

impl<'a> FnEnv<'a> {
    pub fn new(size: [f64; 2]) -> FnEnv<'a> {
        FnEnv { size, index: 0, count: 1, seed: 0, images: Vec::new(), graph_inputs: &NO_INPUTS, args: BTreeMap::new(), stats: Vec::new() }
    }

    /// Precompute image statistics for `f_image_avg` / `f_image_min` / `f_image_max` (call once before many runs).
    pub fn with_stats(mut self) -> FnEnv<'a> {
        self.stats = self
            .images
            .iter()
            .map(|(name, t)| {
                let (mut sum, mut mn, mut mx) = ([0.0f64; 4], [f32::MAX; 4], [f32::MIN; 4]);
                for p in &t.px {
                    for c in 0..4 {
                        sum[c] += p[c] as f64;
                        mn[c] = mn[c].min(p[c]);
                        mx[c] = mx[c].max(p[c]);
                    }
                }
                let n = t.px.len().max(1) as f64;
                (name.to_string(), [(sum[0] / n) as f32, (sum[1] / n) as f32, (sum[2] / n) as f32, (sum[3] / n) as f32], mn, mx)
            })
            .collect();
        self
    }

    fn image(&self, slot: &str) -> Option<&'a Tex> {
        self.images.iter().find(|(n, _)| *n == slot).map(|(_, t)| *t)
    }
}

/// The inputs a function call passes on (a node's input slots, at most).
pub const CALL_SLOTS: [&str; 12] = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"];
const MAX_INS: usize = CALL_SLOTS.len();

struct PNode {
    id: u32,
    op: Op,
    /// input node indices by slot position (the order of the spec: required slots, then optional ones)
    ins: [Option<usize>; MAX_INS],
    num: BTreeMap<String, f64>,
    text: String,
    name: String,
    callee: Option<Arc<Prog>>,
}

/// A compiled function graph.
pub struct Prog {
    nodes: Vec<PNode>,
    out: usize,
    /// declared inputs: (name, type, default), in node-number order
    pub inputs: Vec<(String, Pin, Value)>,
    pub uses_stats: bool,
}

struct State {
    memo: Vec<Option<(Value, u64)>>,
    epoch: u64,
    vars: Vec<(String, Value)>,
    rng: u32,
}

const MAX_CALL_DEPTH: usize = 8;

fn declared(n: &crate::graph::Node) -> (Pin, Value) {
    let pin = type_pin(n.nums.get("type").copied().unwrap_or(0.0).round() as i32);
    let g = |k: &str| n.nums.get(k).copied().unwrap_or(0.0) as f64;
    let v = match pin {
        Pin::Bool => Value::Bool(g("v") != 0.0),
        Pin::Text => Value::text(n.strs.get("v").map(String::as_str).unwrap_or("")),
        p if p.lanes() == 1 => coerce(&Value::float(g("v")), p),
        p => coerce(&Value::vec(p.lanes() as u8, [g("x"), g("y"), g("z"), g("w")]), p),
    };
    (pin, v)
}

impl Prog {
    /// Compile function graph `g`; `lib` supplies the functions that `f_call` nodes run.
    pub fn compile(g: &Graph, lib: &BTreeMap<String, Graph>) -> Result<Prog, String> {
        Prog::compile_depth(g, lib, 0)
    }

    fn compile_depth(g: &Graph, lib: &BTreeMap<String, Graph>, depth: usize) -> Result<Prog, String> {
        if depth >= MAX_CALL_DEPTH {
            return Err("functions call each other too deeply (a function calls itself?)".into());
        }
        let outs: Vec<u32> = g.nodes.iter().filter(|(_, n)| n.kind == "f_out").map(|(i, _)| *i).collect();
        let out_id = match outs.as_slice() {
            [one] => *one,
            [] => return Err("the function has no Output node".into()),
            _ => return Err("the function has more than one Output node".into()),
        };
        let mut prog = Prog { nodes: Vec::new(), out: 0, inputs: Vec::new(), uses_stats: false };
        let mut index: HashMap<u32, usize> = HashMap::new();
        let mut visiting: HashSet<u32> = HashSet::new();
        prog.out = prog.add(g, lib, out_id, depth, &mut index, &mut visiting)?;
        // declared inputs, including ones the result does not use (so a caller can still bind them)
        let mut ids: Vec<u32> = g.nodes.iter().filter(|(_, n)| n.kind == "f_input").map(|(i, _)| *i).collect();
        ids.sort();
        for id in ids {
            let n = &g.nodes[&id];
            let (pin, def) = declared(n);
            prog.inputs.push((n.strs.get("name").cloned().unwrap_or_default(), pin, def));
        }
        Ok(prog)
    }

    fn add(&mut self, g: &Graph, lib: &BTreeMap<String, Graph>, id: u32, depth: usize, index: &mut HashMap<u32, usize>, visiting: &mut HashSet<u32>) -> Result<usize, String> {
        if let Some(i) = index.get(&id) {
            return Ok(*i);
        }
        if !visiting.insert(id) {
            return Err("the function graph contains a cycle".into());
        }
        let n = g.nodes.get(&id).ok_or_else(|| format!("node {id} is missing"))?;
        let op = op_of(&n.kind).ok_or_else(|| format!("node {id} ({}) is not a function node", n.kind))?;
        let sp = crate::graph::spec(&n.kind).ok_or_else(|| format!("unknown node {}", n.kind))?;
        let mut ins = [None; MAX_INS];
        for (pos, slot) in sp.req.iter().chain(sp.opt.iter()).enumerate().take(MAX_INS) {
            if let Some(src) = n.ins.get(*slot) {
                ins[pos] = Some(self.add(g, lib, *src, depth, index, visiting)?);
            } else if pos < sp.req.len() {
                return Err(format!("node {id} ({}) is missing its '{slot}' input", n.kind));
            }
        }
        let callee = if op == Op::Call {
            let name = n.strs.get("fn").ok_or_else(|| format!("node {id} (f_call) needs fn=\"name\""))?;
            let fg = lib.get(name).ok_or_else(|| format!("node {id}: unknown function '{name}'"))?;
            if !is_function(fg) {
                return Err(format!("node {id}: '{name}' is not a function graph"));
            }
            Some(Arc::new(Prog::compile_depth(fg, lib, depth + 1).map_err(|e| format!("in function '{name}': {e}"))?))
        } else {
            None
        };
        if matches!(op, Op::Stat(_)) {
            self.uses_stats = true;
        }
        if let Some(c) = &callee {
            if c.uses_stats {
                self.uses_stats = true;
            }
        }
        let i = self.nodes.len();
        self.nodes.push(PNode {
            id,
            op,
            ins,
            num: n.nums.iter().map(|(k, v)| (k.clone(), *v as f64)).collect(),
            text: n.strs.get("v").or_else(|| n.strs.get("mask")).cloned().unwrap_or_default(),
            name: n.strs.get("name").or_else(|| n.strs.get("slot")).cloned().unwrap_or_default(),
            callee,
        });
        index.insert(id, i);
        visiting.remove(&id);
        Ok(i)
    }

    /// Run the function at `pos` (uv, 0..1).
    pub fn run(&self, env: &FnEnv, pos: [f64; 2]) -> Result<Value, String> {
        let mut st = State { memo: vec![None; self.nodes.len()], epoch: 0, vars: Vec::new(), rng: 0 };
        self.eval(self.out, env, pos, &mut st)
    }

    /// Like [`Prog::run`], with variables that live across runs: `vars` is what `f_get` reads before anything is
    /// set, and holds every variable the run set afterwards (pattern programs share variables down a branch).
    pub fn run_vars(&self, env: &FnEnv, pos: [f64; 2], vars: &mut Vec<(String, Value)>) -> Result<Value, String> {
        let mut st = State { memo: vec![None; self.nodes.len()], epoch: 0, vars: std::mem::take(vars), rng: 0 };
        let r = self.eval(self.out, env, pos, &mut st);
        *vars = st.vars;
        r
    }

    fn eval(&self, i: usize, env: &FnEnv, pos: [f64; 2], st: &mut State) -> Result<Value, String> {
        if let Some((v, e)) = &st.memo[i] {
            if *e == st.epoch {
                return Ok(v.clone());
            }
        }
        let n = &self.nodes[i];
        let r = self.eval_node(n, env, pos, st).map_err(|e| if e.starts_with("node ") { e } else { format!("node {} ({}): {e}", n.id, kind_of(n.op)) })?;
        st.memo[i] = Some((r.clone(), st.epoch));
        Ok(r)
    }

    fn arg(&self, n: &PNode, k: usize, env: &FnEnv, pos: [f64; 2], st: &mut State) -> Result<Value, String> {
        match n.ins[k] {
            Some(j) => self.eval(j, env, pos, st),
            None => Err("an input is not connected".into()),
        }
    }

    fn opt_arg(&self, n: &PNode, k: usize, env: &FnEnv, pos: [f64; 2], st: &mut State) -> Result<Option<Value>, String> {
        match n.ins[k] {
            Some(j) => self.eval(j, env, pos, st).map(Some),
            None => Ok(None),
        }
    }

    fn eval_node(&self, n: &PNode, env: &FnEnv, pos: [f64; 2], st: &mut State) -> Result<Value, String> {
        let num = |k: &str| n.num.get(k).copied().unwrap_or(0.0);
        Ok(match n.op {
            Op::Out | Op::Pin => self.arg(n, 0, env, pos, st)?,
            Op::Float => Value::float(num("v")),
            Op::Int => Value::int(num("v").trunc() as i64),
            Op::Bool => Value::Bool(num("v") != 0.0),
            Op::Str => Value::text(&n.text),
            Op::Vec(lanes, int) => {
                let mut parts: [Option<Value>; 4] = [None, None, None, None];
                for (k, p) in parts.iter_mut().enumerate() {
                    *p = self.opt_arg(n, k, env, pos, st)?;
                }
                build_vec(lanes, int, &parts, [num("x"), num("y"), num("z"), num("w")])?
            }
            Op::Swizzle => swizzle(&self.arg(n, 0, env, pos, st)?, &n.text)?,
            Op::Un(u) => unary(u, &self.arg(n, 0, env, pos, st)?)?,
            Op::Bin(b) => {
                let (x, y) = (self.arg(n, 0, env, pos, st)?, self.arg(n, 1, env, pos, st)?);
                binary(b, &x, &y)?
            }
            Op::Clamp => {
                let (x, y, z) = (self.arg(n, 0, env, pos, st)?, self.arg(n, 1, env, pos, st)?, self.arg(n, 2, env, pos, st)?);
                clamp3(&x, &y, &z)?
            }
            Op::Lerp => {
                let (x, y, z) = (self.arg(n, 0, env, pos, st)?, self.arg(n, 1, env, pos, st)?, self.arg(n, 2, env, pos, st)?);
                lerp3(&x, &y, &z)?
            }
            Op::Smooth => {
                let (x, y, z) = (self.arg(n, 0, env, pos, st)?, self.arg(n, 1, env, pos, st)?, self.arg(n, 2, env, pos, st)?);
                smooth3(&x, &y, &z)?
            }
            Op::If => match self.arg(n, 0, env, pos, st)? {
                Value::Bool(true) => self.arg(n, 1, env, pos, st)?,
                Value::Bool(false) => self.arg(n, 2, env, pos, st)?,
                _ => return Err("the condition must be a boolean".into()),
            },
            Op::Seq => {
                self.arg(n, 0, env, pos, st)?;
                self.arg(n, 1, env, pos, st)?
            }
            Op::While => {
                let mut last = self.arg(n, 0, env, pos, st)?;
                let max = num("max");
                let cap: u64 = if max < 0.0 { 1_000_000 } else { (max as u64).min(1_000_000) };
                let mut done = 0u64;
                loop {
                    // a new epoch: the condition and the body read the variables as they are now
                    st.epoch += 1;
                    match self.arg(n, 1, env, pos, st)? {
                        Value::Bool(true) => break,
                        Value::Bool(false) => {}
                        _ => return Err("the exit condition must be a boolean".into()),
                    }
                    if done >= cap {
                        break;
                    }
                    last = self.arg(n, 2, env, pos, st)?;
                    done += 1;
                }
                last
            }
            Op::Set => {
                let v = self.arg(n, 0, env, pos, st)?;
                match st.vars.iter_mut().find(|(k, _)| *k == n.name) {
                    Some(slot) => slot.1 = v.clone(),
                    None => st.vars.push((n.name.clone(), v.clone())),
                }
                st.epoch += 1;
                v
            }
            Op::Get => st.vars.iter().find(|(k, _)| *k == n.name).map(|(_, v)| v.clone()).unwrap_or_else(|| Value::float(0.0)),
            Op::Random => {
                let lo = self.opt_arg(n, 0, env, pos, st)?.map(|v| v.as_f32() as f64).unwrap_or(0.0);
                let hi = self.opt_arg(n, 1, env, pos, st)?.map(|v| v.as_f32() as f64).unwrap_or(1.0);
                st.rng = st.rng.wrapping_add(1);
                let seed = env.seed.wrapping_add(num("seed") as u32).wrapping_add(st.rng.wrapping_mul(7919));
                let r = rnd((pos[0] * 4096.0) as i32 ^ (n.id as i32).wrapping_mul(131), (pos[1] * 4096.0) as i32 ^ env.index as i32, seed) as f64;
                Value::float(lo + (hi - lo) * r)
            }
            Op::SampleGray | Op::SampleColor => {
                let p = self.arg(n, 0, env, pos, st)?.num()?;
                if p.lanes != 2 {
                    return Err("the sample position must be a float2".into());
                }
                let slot = if n.name.is_empty() { "in" } else { n.name.as_str() };
                let t = env.image(slot).ok_or_else(|| format!("no input image '{slot}' to sample"))?;
                let c = t.sample(p.v[0] as f32, p.v[1] as f32);
                if n.op == Op::SampleGray {
                    Value::float(lum(c) as f64)
                } else {
                    Value::vec(4, [c[0] as f64, c[1] as f64, c[2] as f64, c[3] as f64])
                }
            }
            Op::Stat(k) => {
                let slot = if n.name.is_empty() { "in" } else { n.name.as_str() };
                let s = env.stats.iter().find(|s| s.0 == slot).ok_or_else(|| format!("no input image '{slot}' for the statistic"))?;
                let a = match k {
                    0 => s.1,
                    1 => s.2,
                    _ => s.3,
                };
                Value::vec(4, [a[0] as f64, a[1] as f64, a[2] as f64, a[3] as f64])
            }
            Op::Pos => Value::vec(2, [pos[0], pos[1], 0.0, 0.0]),
            Op::Size => Value::vec(2, [env.size[0], env.size[1], 0.0, 0.0]),
            Op::Index => Value::int(env.index),
            Op::Count => Value::int(env.count),
            Op::Seed => Value::int(env.seed as i64),
            Op::Input | Op::GraphInput => {
                let pin = type_pin(num("type").round() as i32);
                let default = {
                    let g = |k: &str| num(k);
                    match pin {
                        Pin::Bool => Value::Bool(g("v") != 0.0),
                        Pin::Text => Value::text(&n.text),
                        p if p.lanes() == 1 => coerce(&Value::float(g("v")), p),
                        p => coerce(&Value::vec(p.lanes() as u8, [g("x"), g("y"), g("z"), g("w")]), p),
                    }
                };
                let from = if n.op == Op::Input { env.args.get(&n.name) } else { env.graph_inputs.get(&n.name) };
                match from {
                    Some(v) => coerce(v, pin),
                    None => default,
                }
            }
            Op::ToFloat => {
                let v = self.arg(n, 0, env, pos, st)?;
                let nn = v.num()?;
                Value::Num(Num { int: false, ..nn })
            }
            Op::ToInt => {
                let nn = self.arg(n, 0, env, pos, st)?.num()?;
                map1(&nn, true, |x| x.trunc())
            }
            Op::ToBool => {
                let v = self.arg(n, 0, env, pos, st)?;
                Value::Bool(match &v {
                    Value::Bool(b) => *b,
                    Value::Str(s) => !s.is_empty(),
                    Value::Num(nn) => (0..nn.lanes as usize).any(|i| nn.v[i] != 0.0),
                })
            }
            Op::ToStr => Value::text(&self.arg(n, 0, env, pos, st)?.show()),
            Op::Concat => match (self.arg(n, 0, env, pos, st)?, self.arg(n, 1, env, pos, st)?) {
                (Value::Str(a), Value::Str(b)) => Value::text(&format!("{a}{b}")),
                _ => return Err("f_concat needs two strings".into()),
            },
            Op::Call => {
                let callee = n.callee.as_ref().ok_or("call without a function")?;
                let mut sub_env = FnEnv { size: env.size, index: env.index, count: env.count, seed: env.seed, images: env.images.clone(), graph_inputs: env.graph_inputs, args: BTreeMap::new(), stats: env.stats.clone() };
                for (k, (name, pin, _)) in callee.inputs.iter().enumerate().take(MAX_INS) {
                    if let Some(v) = self.opt_arg(n, k, env, pos, st)? {
                        sub_env.args.insert(name.clone(), coerce(&v, *pin));
                    }
                }
                callee.run(&sub_env, pos)?
            }
        })
    }

    /// Check types without running the function. `Ok` carries the type of the result.
    pub fn result_type(&self) -> Result<Pin, String> {
        let mut memo: Vec<Option<Pin>> = vec![None; self.nodes.len()];
        self.type_of(self.out, &mut memo)
    }

    fn type_of(&self, i: usize, memo: &mut Vec<Option<Pin>>) -> Result<Pin, String> {
        if let Some(p) = memo[i] {
            return Ok(p);
        }
        let n = &self.nodes[i];
        let r = self.type_node(n, memo).map_err(|e| if e.starts_with("node ") { e } else { format!("node {} ({}): {e}", n.id, kind_of(n.op)) })?;
        memo[i] = Some(r);
        Ok(r)
    }

    fn type_node(&self, n: &PNode, memo: &mut Vec<Option<Pin>>) -> Result<Pin, String> {
        let t = |k: usize, memo: &mut Vec<Option<Pin>>| -> Result<Option<Pin>, String> {
            match n.ins[k] {
                Some(j) => self.type_of(j, memo).map(Some),
                None => Ok(None),
            }
        };
        Ok(match n.op {
            Op::Out | Op::Pin => t(0, memo)?.ok_or("an input is not connected")?,
            Op::Float | Op::Random | Op::SampleGray => Pin::Float,
            Op::Int | Op::Index | Op::Count | Op::Seed => Pin::Int,
            Op::Bool => Pin::Bool,
            Op::Str => Pin::Text,
            Op::Vec(lanes, int) => {
                for k in 0..4 {
                    t(k, memo)?;
                }
                type_pin(if int { 5 + lanes as i32 - 1 } else { 3 + lanes as i32 - 2 })
            }
            Op::Seq => {
                t(0, memo)?;
                t(1, memo)?.ok_or("an input is not connected")?
            }
            Op::While => {
                t(1, memo)?;
                t(2, memo)?;
                t(0, memo)?.ok_or("an input is not connected")?
            }
            Op::Set => t(0, memo)?.ok_or("an input is not connected")?,
            Op::Get => Pin::Float,
            Op::If => {
                let c = t(0, memo)?.ok_or("an input is not connected")?;
                if c != Pin::Bool {
                    return Err("the condition must be a boolean".into());
                }
                let (a, b) = (t(1, memo)?.ok_or("an input is not connected")?, t(2, memo)?.ok_or("an input is not connected")?);
                if a != b && !(b.accepts(a) || a.accepts(b)) {
                    return Err(format!("the branches have different types ({} and {})", a.name(), b.name()));
                }
                if b.accepts(a) { b } else { a }
            }
            Op::SampleColor | Op::Stat(_) => Pin::Float4,
            Op::Pos | Op::Size => Pin::Float2,
            Op::Input | Op::GraphInput => type_pin(n.num.get("type").copied().unwrap_or(0.0).round() as i32),
            Op::Call => n.callee.as_ref().ok_or("call without a function")?.result_type()?,
            Op::ToFloat | Op::ToInt | Op::ToBool | Op::ToStr | Op::Concat | Op::Un(_) | Op::Bin(_) | Op::Clamp | Op::Lerp | Op::Smooth | Op::Swizzle => {
                let args: Vec<Option<Pin>> = (0..3).map(|k| t(k, memo)).collect::<Result<_, _>>()?;
                infer_pure(n.op, &args, &n.text)?
            }
        })
    }
}

fn kind_of(op: Op) -> &'static str {
    match op {
        Op::Bin(_) | Op::Un(_) => "math",
        Op::If => "if",
        Op::Call => "call",
        Op::Swizzle => "swizzle",
        _ => "function node",
    }
}

// ------------------------------------------------------------------------------------------------ helpers for callers

/// Values of the Input Value nodes of a texture graph, by name (the editor's graph parameters).
pub fn graph_inputs(g: &Graph) -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    for n in g.nodes.values().filter(|n| n.kind == "input_value") {
        let name = n.strs.get("name").cloned().unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let (_, v) = declared(n);
        m.insert(name, v);
    }
    m
}

/// Compile the function called `name` from `lib`.
pub fn compile_named(lib: &BTreeMap<String, Graph>, name: &str) -> Result<Prog, String> {
    let g = lib.get(name).ok_or_else(|| format!("unknown function '{name}'"))?;
    if !is_function(g) {
        return Err(format!("'{name}' is not a function graph"));
    }
    Prog::compile(g, lib).map_err(|e| format!("function '{name}': {e}"))
}

/// Static types of the value nodes of a function graph, for the editor: node id to type or problem.
pub fn node_types(g: &Graph, lib: &BTreeMap<String, Graph>) -> HashMap<u32, Result<Pin, String>> {
    let mut out: HashMap<u32, Result<Pin, String>> = HashMap::new();
    // compile each node as if it were the output: cheap for the sizes of function graphs
    for (&id, n) in &g.nodes {
        if !is_func_kind(&n.kind) || n.kind == "f_out" {
            if n.kind == "f_out" {
                if let Some(src) = n.ins.get("in") {
                    if let Some(r) = out.get(src).cloned().or_else(|| probe(g, lib, *src)) {
                        out.insert(id, r);
                    }
                }
            }
            continue;
        }
        if let Some(r) = probe(g, lib, id) {
            out.insert(id, r);
        }
    }
    out
}

fn probe(g: &Graph, lib: &BTreeMap<String, Graph>, id: u32) -> Option<Result<Pin, String>> {
    // build a throw-away graph whose output reads node `id`
    let mut h = Graph::default();
    h.nodes = g.nodes.clone();
    h.nodes.retain(|_, n| n.kind != "f_out");
    let mut out = crate::graph::Node { kind: "f_out".into(), ..Default::default() };
    out.ins.insert("in".into(), id);
    let oid = h.nodes.keys().max().copied().unwrap_or(0) + 1;
    h.nodes.insert(oid, out);
    h.meta.insert("kind".into(), "function".into());
    Some(Prog::compile(&h, lib).and_then(|p| p.result_type()))
}

/// Run function `name` once (no position, no images): the common case of a function-driven parameter.
pub fn eval_once(lib: &BTreeMap<String, Graph>, name: &str, env: &FnEnv) -> Result<Value, String> {
    compile_named(lib, name)?.run(env, [0.5, 0.5])
}

/// Run the function graph `g` as if `id` were its result, at the centre of the image: the value an editor shows
/// next to a node.
pub fn probe_value(g: &Graph, lib: &BTreeMap<String, Graph>, id: u32, graph_inputs: &BTreeMap<String, Value>) -> Result<Value, String> {
    let mut h = Graph::default();
    h.nodes = g.nodes.clone();
    h.nodes.retain(|_, n| n.kind != "f_out");
    let mut out = crate::graph::Node { kind: "f_out".into(), ..Default::default() };
    out.ins.insert("in".into(), id);
    let oid = h.nodes.keys().max().copied().unwrap_or(0) + 1;
    h.nodes.insert(oid, out);
    h.meta.insert("kind".into(), "function".into());
    let prog = Prog::compile(&h, lib)?;
    let mut env = FnEnv::new([512.0, 512.0]);
    env.graph_inputs = graph_inputs;
    prog.run(&env, [0.5, 0.5])
}
