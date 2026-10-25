// SPDX-License-Identifier: MIT OR Apache-2.0
//! Typed math expressions written as text, turned into the nodes of an `.sbs` function graph (`sbs:fn:*`).
//!
//! An expression such as `lerp(0.2, 0.8, smoothstep(0.3, 0.7, $pos.x)) * intensity` becomes the function nodes a
//! person would wire by hand (Get Float2 `$pos`, Swizzle, Linear Interpolation, ...), so the result is an ordinary
//! function graph: it is saved in the file, opens in Substance-compatible tools and can be edited node by node.
//!
//! Values have the format's types: float1..float4, int1..int4 and bool. Number literals are floats; integers are
//! turned into floats where they meet floats. A scalar combines with a vector (`v * 2`, `v + 0.5`).

use crate::graph::{Graph, Node};
use crate::sbs_defs::PTy;
use crate::sbs_native::{self as native, PVal};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f32),
    Id(String),
    Op(&'static str),
}

const OPS: &[&str] = &["<=", ">=", "==", "!=", "&&", "||", "+", "-", "*", "/", "%", "<", ">", "!", "?", ":", "(", ")", ",", ".", "=", ";"];

fn tokenize(s: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        // a number: digits with an optional fraction and exponent (".5" too, but not after a value: "v.x")
        let after_value = matches!(out.last(), Some(Tok::Num(_)) | Some(Tok::Id(_)) | Some(Tok::Op(")")));
        if ch.is_ascii_digit() || (ch == '.' && !after_value && c.get(i + 1).map(|d| d.is_ascii_digit()).unwrap_or(false)) {
            let st = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            if i < c.len() && (c[i] == 'e' || c[i] == 'E') && c.get(i + 1).map(|d| d.is_ascii_digit() || *d == '-' || *d == '+').unwrap_or(false) {
                i += 2;
                while i < c.len() && c[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let t: String = c[st..i].iter().collect();
            out.push(Tok::Num(t.parse::<f32>().map_err(|_| format!("'{t}' is not a number"))?));
            continue;
        }
        if ch.is_alphabetic() || ch == '_' || ch == '$' {
            let st = i;
            i += 1;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(Tok::Id(c[st..i].iter().collect()));
            continue;
        }
        let rest: String = c[i..c.len().min(i + 2)].iter().collect();
        match OPS.iter().find(|o| rest.starts_with(**o)) {
            Some(o) => {
                out.push(Tok::Op(o));
                i += o.len();
            }
            None => return Err(format!("unexpected '{ch}'")),
        }
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub(crate) enum E {
    Num(f32),
    Var(String),
    Call(String, Vec<E>),
    Bin(&'static str, Box<E>, Box<E>),
    Neg(Box<E>),
    Not(Box<E>),
    If(Box<E>, Box<E>, Box<E>),
    Swz(Box<E>, String),
}

struct P {
    t: Vec<Tok>,
    i: usize,
}

impl P {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn is(&self, o: &str) -> bool {
        matches!(self.peek(), Some(Tok::Op(x)) if *x == o)
    }
    fn eat(&mut self, o: &str) -> bool {
        if self.is(o) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, o: &str) -> Result<(), String> {
        if self.eat(o) {
            Ok(())
        } else {
            Err(format!("expected '{o}'{}", self.near()))
        }
    }
    fn near(&self) -> String {
        match self.peek() {
            Some(Tok::Num(n)) => format!(" near {n}"),
            Some(Tok::Id(s)) => format!(" near '{s}'"),
            Some(Tok::Op(o)) => format!(" near '{o}'"),
            None => " at the end".into(),
        }
    }
    fn ternary(&mut self) -> Result<E, String> {
        let c = self.or()?;
        if self.eat("?") {
            let a = self.ternary()?;
            self.expect(":")?;
            let b = self.ternary()?;
            return Ok(E::If(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }
    fn or(&mut self) -> Result<E, String> {
        let mut l = self.and()?;
        while self.eat("||") {
            l = E::Bin("||", Box::new(l), Box::new(self.and()?));
        }
        Ok(l)
    }
    fn and(&mut self) -> Result<E, String> {
        let mut l = self.cmp()?;
        while self.eat("&&") {
            l = E::Bin("&&", Box::new(l), Box::new(self.cmp()?));
        }
        Ok(l)
    }
    fn cmp(&mut self) -> Result<E, String> {
        let l = self.add()?;
        for o in ["<=", ">=", "==", "!=", "<", ">"] {
            if self.eat(o) {
                return Ok(E::Bin(o, Box::new(l), Box::new(self.add()?)));
            }
        }
        Ok(l)
    }
    fn add(&mut self) -> Result<E, String> {
        let mut l = self.mul()?;
        loop {
            if self.eat("+") {
                l = E::Bin("+", Box::new(l), Box::new(self.mul()?));
            } else if self.eat("-") {
                l = E::Bin("-", Box::new(l), Box::new(self.mul()?));
            } else {
                return Ok(l);
            }
        }
    }
    fn mul(&mut self) -> Result<E, String> {
        let mut l = self.unary()?;
        loop {
            let o = if self.eat("*") {
                "*"
            } else if self.eat("/") {
                "/"
            } else if self.eat("%") {
                "%"
            } else {
                return Ok(l);
            };
            l = E::Bin(o, Box::new(l), Box::new(self.unary()?));
        }
    }
    fn unary(&mut self) -> Result<E, String> {
        if self.eat("-") {
            return Ok(match self.unary()? {
                E::Num(n) => E::Num(-n),
                e => E::Neg(Box::new(e)),
            });
        }
        if self.eat("+") {
            return self.unary();
        }
        if self.eat("!") {
            return Ok(E::Not(Box::new(self.unary()?)));
        }
        self.postfix()
    }
    fn postfix(&mut self) -> Result<E, String> {
        let mut e = self.primary()?;
        while self.eat(".") {
            match self.peek().cloned() {
                Some(Tok::Id(s)) => {
                    self.i += 1;
                    e = E::Swz(Box::new(e), s);
                }
                _ => return Err(format!("expected components after '.' (x, y, z, w or r, g, b, a){}", self.near())),
            }
        }
        Ok(e)
    }
    fn primary(&mut self) -> Result<E, String> {
        match self.peek().cloned() {
            Some(Tok::Num(n)) => {
                self.i += 1;
                Ok(E::Num(n))
            }
            Some(Tok::Id(name)) => {
                self.i += 1;
                if self.eat("(") {
                    let mut args = Vec::new();
                    if !self.eat(")") {
                        loop {
                            args.push(self.ternary()?);
                            if self.eat(")") {
                                break;
                            }
                            self.expect(",")?;
                        }
                    }
                    return Ok(E::Call(name.to_ascii_lowercase(), args));
                }
                Ok(E::Var(name))
            }
            Some(Tok::Op("(")) => {
                self.i += 1;
                let e = self.ternary()?;
                self.expect(")")?;
                Ok(e)
            }
            _ => Err(format!("expected a value{}", self.near())),
        }
    }
}

/// `name = expression;` lines (each name can be used by the lines after it), then the result expression.
pub(crate) fn parse(src: &str) -> Result<(Vec<(String, E)>, E), String> {
    let t = tokenize(src)?;
    if t.is_empty() {
        return Err("the expression is empty".into());
    }
    let mut p = P { t, i: 0 };
    let mut lets = Vec::new();
    loop {
        let is_let = matches!((p.t.get(p.i), p.t.get(p.i + 1)), (Some(Tok::Id(_)), Some(Tok::Op("="))));
        if !is_let {
            break;
        }
        let Some(Tok::Id(name)) = p.t.get(p.i).cloned() else { break };
        if name.starts_with('$') || SYSTEM_VARS.iter().any(|v| v.0 == name) {
            return Err(format!("'{name}' is a system variable and cannot be set"));
        }
        p.i += 2;
        let e = p.ternary()?;
        p.expect(";")?;
        lets.push((name, e));
    }
    let e = p.ternary()?;
    p.eat(";");
    if p.i < p.t.len() {
        return Err(format!("unexpected text{}", p.near()));
    }
    Ok((lets, e))
}

/// The variables of the format every function can read, with their types.
pub const SYSTEM_VARS: &[(&str, PTy)] = &[("$pos", PTy::F(2)), ("$size", PTy::F(2)), ("$sizelog2", PTy::F(2)), ("$time", PTy::F(1)), ("$number", PTy::I(1)), ("$depth", PTy::I(1)), ("$normalformat", PTy::I(1))];

/// Help text for the expression language (for command documentation).
pub const HELP: &str = "optional lines `name = expression;` before the result (later lines use the names), numbers (floats), + - * / % (modulo), comparisons < <= > >= == !=, && || !, c ? a : b, \
components v.x v.xy v.rgb, vec2(a, b) vec3(..) vec4(..) (also from a vector and a float: vec4(v.rgb, 1)), \
sin cos tan sqrt abs floor ceil fract exp log log2 exp2 pow(a, b) min max clamp(x, lo, hi) saturate lerp(a, b, t) (mix) \
smoothstep(e0, e1, x) step(edge, x) dot cross length normalize distance(a, b) atan2(y, x) cartesian(r, angle) \
rand(max) hash(x or vec2) hash2(vec2) cellrand(cell, seed) (repeatable random numbers 0..1) int(x) float(x), sample(pos [, input]) (grey level of an image input, 0 = first) samplecolor(pos [, input]), \
$pos $size $sizelog2 $time $number $depth, the graph's value inputs by identifier, pi tau";

/// A value made so far: the node that gives it and its type.
type Val = (u32, PTy);

struct Emit<'a> {
    nodes: BTreeMap<u32, Node>,
    next: u32,
    vars: &'a dyn Fn(&str) -> Option<PTy>,
    /// one node per constant / variable read
    memo: BTreeMap<String, (u32, PTy)>,
    /// values named with `name = ...;`
    locals: BTreeMap<String, (u32, PTy)>,
}

fn tname(t: PTy) -> String {
    match t {
        PTy::F(1) => "float".into(),
        PTy::F(n) => format!("float{n}"),
        PTy::I(1) => "integer".into(),
        PTy::I(n) => format!("integer{n}"),
        PTy::B => "bool".into(),
        _ => "text".into(),
    }
}

impl<'a> Emit<'a> {
    fn node(&mut self, f: &str, ty: PTy, ins: &[(&str, u32)]) -> u32 {
        let id = self.next;
        self.next += 1;
        let mut n = native::new_node(&format!("sbs:fn:{f}"), None, [0.0, 0.0]);
        n.strs.insert("#type".into(), ty.type_code().to_string());
        for (slot, src) in ins {
            n.ins.insert(slot.to_string(), *src);
        }
        self.nodes.insert(id, n);
        id
    }

    fn set(&mut self, id: u32, key: &str, v: PVal) {
        if let Some(n) = self.nodes.get_mut(&id) {
            native::set_value(n, key, Some(v));
        }
    }

    fn float(&mut self, x: f32) -> (u32, PTy) {
        let key = format!("f:{}", native::fmt_num(x));
        if let Some(r) = self.memo.get(&key) {
            return *r;
        }
        let id = self.node("const_float1", PTy::F(1), &[]);
        self.set(id, "const_float1", PVal::of_nums(PTy::F(1), &[x], None));
        self.memo.insert(key, (id, PTy::F(1)));
        (id, PTy::F(1))
    }

    fn get(&mut self, name: &str, ty: PTy) -> (u32, PTy) {
        let key = format!("v:{name}");
        if let Some(r) = self.memo.get(&key) {
            return *r;
        }
        let f = match ty {
            PTy::F(n) => format!("get_float{n}"),
            PTy::I(n) => format!("get_integer{n}"),
            PTy::B => "get_bool".into(),
            _ => "get_string".into(),
        };
        let id = self.node(&f, ty, &[]);
        self.set(id, &f, PVal::Const { ty: PTy::S, raw: name.to_string(), rel: None });
        self.memo.insert(key, (id, ty));
        (id, ty)
    }

    /// Integers become floats (same number of lanes).
    fn as_float(&mut self, v: (u32, PTy)) -> (u32, PTy) {
        match v.1 {
            PTy::I(n) => {
                let f = if n == 1 { "tofloat".to_string() } else { format!("tofloat{n}") };
                (self.node(&f, PTy::F(n), &[("value", v.0)]), PTy::F(n))
            }
            PTy::B => {
                let (one, zero) = (self.float(1.0).0, self.float(0.0).0);
                (self.node("ifelse", PTy::F(1), &[("condition", v.0), ("ifpath", one), ("elsepath", zero)]), PTy::F(1))
            }
            _ => v,
        }
    }

    /// A float vector of `n` lanes made of the scalar `s` repeated.
    fn splat(&mut self, s: (u32, PTy), n: u8) -> (u32, PTy) {
        let mut cur = s;
        for k in 2..=n {
            let f = format!("vector{k}");
            cur = (self.node(&f, PTy::F(k), &[("componentsin", cur.0), ("componentslast", s.0)]), PTy::F(k));
        }
        cur
    }

    fn lanes(t: PTy) -> u8 {
        match t {
            PTy::F(n) | PTy::I(n) => n,
            _ => 1,
        }
    }

    /// Bring two values to one type for an operator: integers meet floats as floats, a scalar meets a vector as a
    /// vector of that scalar.
    fn unify(&mut self, a: Val, b: Val, what: &str) -> Result<(Val, Val), String> {
        if a.1 == b.1 && a.1 != PTy::B {
            return Ok((a, b));
        }
        if a.1 == PTy::B || b.1 == PTy::B {
            return Err(format!("{what}: a bool cannot be used as a number here (use c ? a : b)"));
        }
        let (mut a, mut b) = (a, b);
        if matches!(a.1, PTy::F(_)) || matches!(b.1, PTy::F(_)) {
            a = self.as_float(a);
            b = self.as_float(b);
        }
        let (la, lb) = (Self::lanes(a.1), Self::lanes(b.1));
        if la == lb {
            return Ok((a, b));
        }
        if la == 1 {
            let fa = self.as_float(a);
            a = self.splat(fa, lb);
            b = self.as_float(b);
        } else if lb == 1 {
            let fb = self.as_float(b);
            b = self.splat(fb, la);
            a = self.as_float(a);
        } else {
            return Err(format!("{what}: cannot combine a {} with a {}", tname(a.1), tname(b.1)));
        }
        Ok((a, b))
    }

    fn emit(&mut self, e: &E) -> Result<(u32, PTy), String> {
        Ok(match e {
            E::Num(x) => self.float(*x),
            E::Var(name) => {
                if let Some(v) = self.locals.get(name) {
                    return Ok(*v);
                }
                let lower = name.to_ascii_lowercase();
                match lower.as_str() {
                    "pi" => return Ok(self.float(std::f32::consts::PI)),
                    "tau" => return Ok(self.float(std::f32::consts::TAU)),
                    "true" | "false" => {
                        let id = self.node("const_bool", PTy::B, &[]);
                        self.set(id, "const_bool", PVal::of_nums(PTy::B, &[(lower == "true") as i32 as f32], None));
                        return Ok((id, PTy::B));
                    }
                    _ => {}
                }
                if let Some((_, t)) = SYSTEM_VARS.iter().find(|(n, _)| *n == lower) {
                    return Ok(self.get(&lower, *t));
                }
                let t = (self.vars)(name).ok_or_else(|| format!("unknown name '{name}' (system variables: {}; or an input of the graph)", SYSTEM_VARS.iter().map(|v| v.0).collect::<Vec<_>>().join(", ")))?;
                self.get(name, t)
            }
            E::Neg(a) => {
                let v = self.emit(a)?;
                if v.1 == PTy::B {
                    return Err("'-' needs a number, not a bool".into());
                }
                (self.node("neg", v.1, &[("a", v.0)]), v.1)
            }
            E::Not(a) => {
                let v = self.emit(a)?;
                if v.1 != PTy::B {
                    return Err("'!' needs a bool (a comparison)".into());
                }
                (self.node("not", PTy::B, &[("a", v.0)]), PTy::B)
            }
            E::If(c, a, b) => {
                let c = self.emit(c)?;
                if c.1 != PTy::B {
                    return Err("the condition of '? :' must be a comparison or a bool".into());
                }
                let (a, b) = (self.emit(a)?, self.emit(b)?);
                let (a, b) = if a.1 == PTy::B && b.1 == PTy::B { (a, b) } else { self.unify(a, b, "? :")? };
                (self.node("ifelse", a.1, &[("condition", c.0), ("ifpath", a.0), ("elsepath", b.0)]), a.1)
            }
            E::Bin(op, a, b) => {
                let (va, vb) = (self.emit(a)?, self.emit(b)?);
                self.binary(op, va, vb)?
            }
            E::Swz(v, comps) => {
                let v = self.emit(v)?;
                let n = Self::lanes(v.1);
                let idx: Vec<u8> = comps
                    .chars()
                    .map(|c| match c {
                        'x' | 'r' => Ok(0u8),
                        'y' | 'g' => Ok(1),
                        'z' | 'b' => Ok(2),
                        'w' | 'a' => Ok(3),
                        _ => Err(format!("'.{comps}': components are x y z w (or r g b a)")),
                    })
                    .collect::<Result<_, _>>()?;
                if idx.is_empty() || idx.len() > 4 {
                    return Err(format!("'.{comps}': one to four components"));
                }
                if let Some(bad) = idx.iter().find(|i| **i >= n) {
                    return Err(format!("'.{comps}': a {} has no component {}", tname(v.1), ["x", "y", "z", "w"][*bad as usize]));
                }
                if n == 1 && idx.len() == 1 {
                    return Ok(v);
                }
                let k = idx.len() as u8;
                let int = matches!(v.1, PTy::I(_));
                let f = format!("{}swizzle{k}", if int { "i" } else { "" });
                let ty = if int { PTy::I(k) } else { PTy::F(k) };
                let id = self.node(&f, ty, &[("vector", v.0)]);
                let nums: Vec<f32> = idx.iter().map(|i| *i as f32).collect();
                self.set(id, &f, PVal::of_nums(PTy::I(k), &nums, None));
                (id, ty)
            }
            E::Call(f, args) => self.call(f, args)?,
        })
    }

    fn binary(&mut self, op: &str, va: (u32, PTy), vb: (u32, PTy)) -> Result<(u32, PTy), String> {
        if op == "&&" || op == "||" {
            if va.1 != PTy::B || vb.1 != PTy::B {
                return Err(format!("'{op}' joins comparisons (bools)"));
            }
            let f = if op == "&&" { "and" } else { "or" };
            return Ok((self.node(f, PTy::B, &[("a", va.0), ("b", vb.0)]), PTy::B));
        }
        // a vector times / divided by a scalar uses Scalar Multiply (no extra nodes)
        if (op == "*" || op == "/") && Self::lanes(va.1) > 1 && Self::lanes(vb.1) == 1 && matches!(va.1, PTy::F(_)) {
            let s = self.as_float(vb);
            let s = if op == "/" {
                let one = self.float(1.0);
                (self.node("div", PTy::F(1), &[("a", one.0), ("b", s.0)]), PTy::F(1))
            } else {
                s
            };
            return Ok((self.node("mulscalar", va.1, &[("a", va.0), ("scalar", s.0)]), va.1));
        }
        if op == "*" && Self::lanes(vb.1) > 1 && Self::lanes(va.1) == 1 && matches!(vb.1, PTy::F(_)) {
            let s = self.as_float(va);
            return Ok((self.node("mulscalar", vb.1, &[("a", vb.0), ("scalar", s.0)]), vb.1));
        }
        let (a, b) = self.unify(va, vb, op)?;
        let (f, ty) = match op {
            "+" => ("add", a.1),
            "-" => ("sub", a.1),
            "*" => ("mul", a.1),
            "/" => ("div", a.1),
            "%" => ("mod", a.1),
            "<" => ("lr", PTy::B),
            "<=" => ("lreq", PTy::B),
            ">" => ("gt", PTy::B),
            ">=" => ("gteq", PTy::B),
            "==" => ("eq", PTy::B),
            "!=" => ("noteq", PTy::B),
            _ => return Err(format!("unknown operator '{op}'")),
        };
        Ok((self.node(f, ty, &[("a", a.0), ("b", b.0)]), ty))
    }

    fn need(f: &str, args: &[E], n: std::ops::RangeInclusive<usize>) -> Result<(), String> {
        if n.contains(&args.len()) {
            Ok(())
        } else if n.start() == n.end() {
            Err(format!("{f}() takes {} value{}", n.start(), if *n.start() == 1 { "" } else { "s" }))
        } else {
            Err(format!("{f}() takes {} to {} values", n.start(), n.end()))
        }
    }

    fn float_arg(&mut self, e: &E) -> Result<(u32, PTy), String> {
        let v = self.emit(e)?;
        if v.1 == PTy::B {
            return Err("a number is needed here, not a bool".into());
        }
        Ok(self.as_float(v))
    }

    fn scalar_arg(&mut self, e: &E, f: &str) -> Result<(u32, PTy), String> {
        let v = self.float_arg(e)?;
        if v.1 != PTy::F(1) {
            return Err(format!("{f}(): this value must be a single float, not a {}", tname(v.1)));
        }
        Ok(v)
    }

    /// `hash(x)` / `hash(vec2)`: a repeatable random number in 0..1; `hash2(vec2)`: two of them (a vec2);
    /// `cellrand(id, seed)`: the same for a cell number (float or vec2) with a seed of your own.
    fn hash_call(&mut self, f: &str, args: &[E]) -> Result<(u32, PTy), String> {
        let num = |x: f32| E::Num(x);
        let var = |n: &str| E::Var(n.to_string());
        let (v, seed) = match f {
            "cellrand" => {
                Self::need(f, args, 2..=2)?;
                (self.float_arg(&args[0])?, Some(self.scalar_arg(&args[1], f)?))
            }
            _ => {
                Self::need(f, args, 1..=1)?;
                (self.float_arg(&args[0])?, None)
            }
        };
        self.locals.insert("$hv".into(), v);
        // the cell number moved by the seed (a different seed gives an unrelated pattern)
        let moved = match seed {
            Some(sd) => {
                self.locals.insert("$hs".into(), sd);
                match v.1 {
                    PTy::F(2) => E::Bin("+", Box::new(var("$hv")), Box::new(E::Call("vec2".into(), vec![E::Bin("*", Box::new(var("$hs")), Box::new(num(7.13))), E::Bin("*", Box::new(var("$hs")), Box::new(num(3.77)))]))),
                    PTy::F(1) => E::Bin("+", Box::new(var("$hv")), Box::new(E::Bin("*", Box::new(var("$hs")), Box::new(num(7.13))))),
                    _ => return Err(format!("{f}() takes a float or a float2")),
                }
            }
            None => var("$hv"),
        };
        let one = |x: E| -> E {
            let dotted = match v.1 {
                PTy::F(2) => E::Call("dot".into(), vec![x, E::Call("vec2".into(), vec![num(12.9898), num(78.233)])]),
                PTy::F(1) => E::Bin("*", Box::new(x), Box::new(num(12.9898))),
                _ => E::Num(0.0),
            };
            E::Call("fract".into(), vec![E::Bin("*", Box::new(E::Call("sin".into(), vec![dotted])), Box::new(num(43758.5453)))])
        };
        if !matches!(v.1, PTy::F(1) | PTy::F(2)) {
            return Err(format!("{f}() takes a float or a float2"));
        }
        let ast = if f == "hash2" {
            if v.1 != PTy::F(2) {
                return Err("hash2() takes a float2".into());
            }
            let shifted = E::Bin("+", Box::new(moved.clone()), Box::new(E::Call("vec2".into(), vec![num(31.7), num(17.3)])));
            E::Call("vec2".into(), vec![one(moved), one(shifted)])
        } else {
            one(moved)
        };
        self.emit(&ast)
    }

    fn call(&mut self, f: &str, args: &[E]) -> Result<(u32, PTy), String> {
        let unary = ["sin", "cos", "tan", "sqrt", "abs", "floor", "ceil", "exp", "log", "log2"];
        if unary.contains(&f) {
            Self::need(f, args, 1..=1)?;
            let v = if matches!(f, "abs" | "floor" | "ceil") { self.emit(&args[0])? } else { self.float_arg(&args[0])? };
            if v.1 == PTy::B {
                return Err(format!("{f}() needs a number"));
            }
            return Ok((self.node(f, v.1, &[("a", v.0)]), v.1));
        }
        if matches!(f, "hash" | "hash2" | "cellrand") {
            return self.hash_call(f, args);
        }
        Ok(match f {
            "exp2" | "pow2" => {
                Self::need(f, args, 1..=1)?;
                let v = self.float_arg(&args[0])?;
                (self.node("pow2", v.1, &[("a", v.0)]), v.1)
            }
            "pow" => {
                // a^b = 2^(b * log2 a)
                Self::need(f, args, 2..=2)?;
                let (a, b) = (self.float_arg(&args[0])?, self.float_arg(&args[1])?);
                let l = (self.node("log2", a.1, &[("a", a.0)]), a.1);
                let m = self.binary("*", b, l)?;
                (self.node("pow2", m.1, &[("a", m.0)]), m.1)
            }
            "min" | "max" | "mod" => {
                Self::need(f, args, 2..=2)?;
                let (a, b) = (self.emit(&args[0])?, self.emit(&args[1])?);
                let (a, b) = self.unify(a, b, f)?;
                (self.node(f, a.1, &[("a", a.0), ("b", b.0)]), a.1)
            }
            "clamp" | "saturate" => {
                let (x, lo, hi) = if f == "saturate" {
                    Self::need(f, args, 1..=1)?;
                    (self.emit(&args[0])?, self.float(0.0), self.float(1.0))
                } else {
                    Self::need(f, args, 3..=3)?;
                    (self.emit(&args[0])?, self.emit(&args[1])?, self.emit(&args[2])?)
                };
                let (x, hi) = self.unify(x, hi, f)?;
                let m = (self.node("min", x.1, &[("a", x.0), ("b", hi.0)]), x.1);
                let (m, lo) = self.unify(m, lo, f)?;
                (self.node("max", m.1, &[("a", m.0), ("b", lo.0)]), m.1)
            }
            "lerp" | "mix" => {
                Self::need(f, args, 3..=3)?;
                let (a, b) = (self.float_arg(&args[0])?, self.float_arg(&args[1])?);
                let (a, b) = self.unify(a, b, f)?;
                let t = self.scalar_arg(&args[2], f)?;
                (self.node("lerp", a.1, &[("a", a.0), ("b", b.0), ("x", t.0)]), a.1)
            }
            "fract" => {
                Self::need(f, args, 1..=1)?;
                let v = self.float_arg(&args[0])?;
                let fl = (self.node("floor", v.1, &[("a", v.0)]), v.1);
                (self.node("sub", v.1, &[("a", v.0), ("b", fl.0)]), v.1)
            }
            "step" => {
                Self::need(f, args, 2..=2)?;
                let (edge, x) = (self.scalar_arg(&args[0], f)?, self.scalar_arg(&args[1], f)?);
                let c = (self.node("gteq", PTy::B, &[("a", x.0), ("b", edge.0)]), PTy::B);
                let (one, zero) = (self.float(1.0), self.float(0.0));
                (self.node("ifelse", PTy::F(1), &[("condition", c.0), ("ifpath", one.0), ("elsepath", zero.0)]), PTy::F(1))
            }
            "smoothstep" => {
                // t = saturate((x - e0) / (e1 - e0)); t * t * (3 - 2 t)
                Self::need(f, args, 3..=3)?;
                let (e0, e1, x) = (self.scalar_arg(&args[0], f)?, self.scalar_arg(&args[1], f)?, self.float_arg(&args[2])?);
                let num = self.binary("-", x, e0)?;
                let den = self.binary("-", e1, e0)?;
                let q = self.binary("/", num, den)?;
                let (zero, one) = (self.float(0.0), self.float(1.0));
                let (q, one_v) = self.unify(q, one, f)?;
                let mn = (self.node("min", q.1, &[("a", q.0), ("b", one_v.0)]), q.1);
                let (mn, zero_v) = self.unify(mn, zero, f)?;
                let t = (self.node("max", mn.1, &[("a", mn.0), ("b", zero_v.0)]), mn.1);
                let tt = self.binary("*", t, t)?;
                let two = self.float(2.0);
                let two_t = self.binary("*", t, two)?;
                let three = self.float(3.0);
                let k = self.binary("-", three, two_t)?;
                self.binary("*", tt, k)?
            }
            "vec2" | "vec3" | "vec4" | "float2" | "float3" | "float4" => {
                let want: u8 = f.chars().last().and_then(|c| c.to_digit(10)).unwrap_or(2) as u8;
                if args.is_empty() || args.len() > want as usize {
                    return Err(format!("{f}() takes 1 to {want} values"));
                }
                let parts: Vec<(u32, PTy)> = args.iter().map(|a| self.float_arg(a)).collect::<Result<_, _>>()?;
                if parts.len() == 1 && parts[0].1 == PTy::F(1) {
                    return Ok(self.splat(parts[0], want));
                }
                let total: u8 = parts.iter().map(|p| Self::lanes(p.1)).sum();
                if total != want {
                    return Err(format!("{f}(): the values have {total} components, {want} are needed"));
                }
                // the format builds vectors as (first part, last float): fold the parts from the left
                let mut cur = parts[0];
                for p in &parts[1..] {
                    if p.1 == PTy::F(1) {
                        let n = Self::lanes(cur.1) + 1;
                        cur = (self.node(&format!("vector{n}"), PTy::F(n), &[("componentsin", cur.0), ("componentslast", p.0)]), PTy::F(n));
                    } else {
                        // a vector after the first part: add its lanes one by one
                        for k in 0..Self::lanes(p.1) {
                            let c = self.node("swizzle1", PTy::F(1), &[("vector", p.0)]);
                            self.set(c, "swizzle1", PVal::of_nums(PTy::I(1), &[k as f32], None));
                            let n = Self::lanes(cur.1) + 1;
                            cur = (self.node(&format!("vector{n}"), PTy::F(n), &[("componentsin", cur.0), ("componentslast", c)]), PTy::F(n));
                        }
                    }
                }
                cur
            }
            "dot" => {
                Self::need(f, args, 2..=2)?;
                let (a, b) = (self.float_arg(&args[0])?, self.float_arg(&args[1])?);
                let (a, b) = self.unify(a, b, f)?;
                (self.node("dot", PTy::F(1), &[("a", a.0), ("b", b.0)]), PTy::F(1))
            }
            "cross" => {
                Self::need(f, args, 2..=2)?;
                let (a, b) = (self.float_arg(&args[0])?, self.float_arg(&args[1])?);
                let (a, b) = self.unify(a, b, f)?;
                (self.node("cross", a.1, &[("a", a.0), ("b", b.0)]), a.1)
            }
            "length" => {
                Self::need(f, args, 1..=1)?;
                let v = self.float_arg(&args[0])?;
                if v.1 == PTy::F(1) {
                    return Ok((self.node("abs", v.1, &[("a", v.0)]), v.1));
                }
                let d = self.node("dot", PTy::F(1), &[("a", v.0), ("b", v.0)]);
                (self.node("sqrt", PTy::F(1), &[("a", d)]), PTy::F(1))
            }
            "distance" => {
                Self::need(f, args, 2..=2)?;
                let (a, b) = (self.float_arg(&args[0])?, self.float_arg(&args[1])?);
                let d = self.binary("-", a, b)?;
                if d.1 == PTy::F(1) {
                    return Ok((self.node("abs", d.1, &[("a", d.0)]), d.1));
                }
                let dd = self.node("dot", PTy::F(1), &[("a", d.0), ("b", d.0)]);
                (self.node("sqrt", PTy::F(1), &[("a", dd)]), PTy::F(1))
            }
            "normalize" => {
                Self::need(f, args, 1..=1)?;
                let v = self.float_arg(&args[0])?;
                let d = self.node("dot", PTy::F(1), &[("a", v.0), ("b", v.0)]);
                let l = self.node("sqrt", PTy::F(1), &[("a", d)]);
                self.binary("/", v, (l, PTy::F(1)))?
            }
            "atan2" => {
                // the format's Arc Tangent 2 reads one float2 (x, y)
                Self::need(f, args, 1..=2)?;
                let v = if args.len() == 1 {
                    let v = self.float_arg(&args[0])?;
                    if v.1 != PTy::F(2) {
                        return Err("atan2(v) needs a float2 (x, y); or write atan2(y, x)".into());
                    }
                    v
                } else {
                    let (y, x) = (self.scalar_arg(&args[0], f)?, self.scalar_arg(&args[1], f)?);
                    (self.node("vector2", PTy::F(2), &[("componentsin", x.0), ("componentslast", y.0)]), PTy::F(2))
                };
                (self.node("atan2", PTy::F(1), &[("a", v.0)]), PTy::F(1))
            }
            "cartesian" => {
                Self::need(f, args, 2..=2)?;
                let (r, t) = (self.scalar_arg(&args[0], f)?, self.scalar_arg(&args[1], f)?);
                (self.node("cartesian", PTy::F(2), &[("rho", r.0), ("theta", t.0)]), PTy::F(2))
            }
            "rand" | "random" => {
                Self::need(f, args, 0..=1)?;
                let a = if args.is_empty() { self.float(1.0) } else { self.scalar_arg(&args[0], f)? };
                (self.node("rand", PTy::F(1), &[("a", a.0)]), PTy::F(1))
            }
            "int" | "toint" => {
                Self::need(f, args, 1..=1)?;
                let v = self.float_arg(&args[0])?;
                let n = Self::lanes(v.1);
                (self.node(&format!("toint{n}"), PTy::I(n), &[("value", v.0)]), PTy::I(n))
            }
            "float" | "tofloat" => {
                Self::need(f, args, 1..=1)?;
                let v = self.emit(&args[0])?;
                self.as_float(v)
            }
            "sample" | "samplelum" | "samplegray" | "samplecolor" | "samplecol" => {
                Self::need(f, args, 1..=2)?;
                let pos = self.float_arg(&args[0])?;
                if pos.1 != PTy::F(2) {
                    return Err(format!("{f}(): the position must be a float2 (for example $pos)"));
                }
                let input = match args.get(1) {
                    None => 0.0,
                    Some(E::Num(n)) if *n >= 0.0 && n.fract() == 0.0 => *n,
                    Some(_) => return Err(format!("{f}(): the input number must be a whole number (0 = first input)")),
                };
                let color = matches!(f, "samplecolor" | "samplecol");
                let (fname, ty) = if color { ("samplecol", PTy::F(4)) } else { ("samplelum", PTy::F(1)) };
                let id = self.node(fname, ty, &[("pos", pos.0)]);
                self.set(id, fname, PVal::of_nums(PTy::I(2), &[input, 0.0], None));
                (id, ty)
            }
            _ => return Err(format!("unknown function '{f}()'. Available: {HELP}")),
        })
    }
}

/// Convert the result to the type the value needs: floats and integers into each other, a float into a colour
/// (grey, opaque), a float3 colour into float4.
fn coerce(em: &mut Emit, v: (u32, PTy), want: PTy) -> Result<(u32, PTy), String> {
    let want = if want == PTy::FV { PTy::F(4) } else { want };
    if v.1 == want {
        return Ok(v);
    }
    match (v.1, want) {
        (PTy::B, PTy::F(_)) | (PTy::B, PTy::I(_)) => {
            let f = em.as_float(v);
            coerce(em, f, want)
        }
        (PTy::I(a), PTy::F(b)) if a == b => Ok(em.as_float(v)),
        (PTy::F(a), PTy::I(b)) if a == b => Ok((em.node(&format!("toint{a}"), PTy::I(a), &[("value", v.0)]), PTy::I(a))),
        (PTy::I(_), PTy::F(_)) => {
            let f = em.as_float(v);
            coerce(em, f, want)
        }
        (PTy::F(1), PTy::F(4)) => {
            let g = em.splat(v, 3);
            let one = em.float(1.0);
            Ok((em.node("vector4", PTy::F(4), &[("componentsin", g.0), ("componentslast", one.0)]), PTy::F(4)))
        }
        (PTy::F(3), PTy::F(4)) => {
            let one = em.float(1.0);
            Ok((em.node("vector4", PTy::F(4), &[("componentsin", v.0), ("componentslast", one.0)]), PTy::F(4)))
        }
        (PTy::F(1), PTy::F(n)) => Ok(em.splat(v, n)),
        (PTy::F(_), PTy::B) | (PTy::I(_), PTy::B) => Err("the value is a bool: write a comparison (for example x > 0.5)".into()),
        _ => Err(format!("the expression gives a {} but the value is a {}", tname(v.1), tname(want))),
    }
}

/// Replace the nodes of function graph `fg` with the nodes of `src` and make the last one its result.
/// `want` is the type the value needs (`None`: keep the expression's type); `vars` gives the type of a graph
/// input by identifier. Returns the number of nodes and the result type.
pub fn build(fg: &mut Graph, src: &str, want: Option<PTy>, vars: &dyn Fn(&str) -> Option<PTy>) -> Result<(usize, PTy), String> {
    let (lets, e) = parse(src)?;
    let mut em = Emit { nodes: BTreeMap::new(), next: 1, vars, memo: BTreeMap::new(), locals: BTreeMap::new() };
    for (name, le) in &lets {
        let v = em.emit(le).map_err(|m| format!("{name} = ...: {m}"))?;
        em.locals.insert(name.clone(), v);
    }
    let mut v = em.emit(&e)?;
    if let Some(w) = want {
        v = coerce(&mut em, v, w)?;
    }
    // lay the nodes out from the result leftwards, one column per step away from it
    let mut depth: BTreeMap<u32, usize> = BTreeMap::new();
    let mut stack = vec![(v.0, 0usize)];
    while let Some((id, d)) = stack.pop() {
        if depth.get(&id).map(|x| *x >= d).unwrap_or(false) {
            continue;
        }
        depth.insert(id, d);
        if let Some(n) = em.nodes.get(&id) {
            for s in n.ins.values() {
                stack.push((*s, d + 1));
            }
        }
    }
    let mut rows: BTreeMap<usize, usize> = BTreeMap::new();
    for (id, n) in em.nodes.iter_mut() {
        let d = depth.get(id).copied().unwrap_or(0);
        let r = rows.entry(d).or_insert(0);
        n.pos = [-(d as f32) * 170.0, *r as f32 * 110.0];
        *r += 1;
    }
    let count = em.nodes.len();
    fg.nodes = em.nodes;
    match fg.sd.as_mut() {
        Some(sd) => sd.root = Some(v.0),
        None => return Err("not a function graph of an .sbs package".into()),
    }
    Ok((count, v.1))
}

/// A short text outline of a function graph: one line per node, the result last (for checking what was built).
pub fn outline(fg: &Graph) -> String {
    let root = fg.sd.as_ref().and_then(|s| s.root);
    let mut out = String::new();
    for (id, n) in &fg.nodes {
        let f = n.kind.strip_prefix("sbs:fn:").unwrap_or(&n.kind);
        let vals: Vec<String> = native::values(n).into_iter().map(|(k, v)| format!("{k}={}", v.enc().split_once(':').map(|x| x.1.to_string()).unwrap_or_default())).collect();
        let ins: Vec<String> = n.ins.iter().map(|(k, v)| format!("{k}<-{v}")).collect();
        out.push_str(&format!("{id}{} {f} {} {}\n", if Some(*id) == root { " (result)" } else { "" }, vals.join(" "), ins.join(" ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fg() -> Graph {
        let mut lib = BTreeMap::new();
        let pkg = crate::sbs_native::new_package("G", 256).sd.map(|s| s.pkg).expect("package");
        let name = native::make_function(&mut lib, pkg, "G", 1, "v", &PVal::of_nums(PTy::F(1), &[0.0], None));
        lib.remove(&name).expect("function graph")
    }

    fn kinds(g: &Graph) -> Vec<String> {
        g.nodes.values().map(|n| n.kind.trim_start_matches("sbs:fn:").to_string()).collect()
    }

    #[test]
    fn expressions_become_typed_function_nodes() {
        let mut g = fg();
        let vars = |n: &str| (n == "amount").then_some(PTy::F(1));
        let (n, t) = build(&mut g, "lerp(0.2, 0.8, smoothstep(0.3, 0.7, $pos.x)) * amount", Some(PTy::F(1)), &vars).expect("builds");
        assert_eq!(t, PTy::F(1));
        assert!(n > 8);
        let k = kinds(&g);
        for want in ["get_float2", "swizzle1", "lerp", "get_float1", "mul"] {
            assert!(k.iter().any(|x| x == want), "{want} in {k:?}");
        }
        // a colour result from a grey expression
        let (_, t) = build(&mut g, "$pos.x > 0.5 ? 1 : 0.25", Some(PTy::FV), &vars).expect("builds");
        assert_eq!(t, PTy::F(4));
        // vectors and scalars combine
        let (_, t) = build(&mut g, "vec4($pos * 2 - 1, 0, 1)", None, &vars).expect("builds");
        assert_eq!(t, PTy::F(4));
        let (_, t) = build(&mut g, "sample($pos + vec2(0.01, 0)) - sample($pos)", Some(PTy::F(1)), &vars).expect("builds");
        assert_eq!(t, PTy::F(1));
        // named steps
        let (n1, _) = build(&mut g, "p = $pos - 0.5; r = length(p); r < 0.3 ? 1 : 0", Some(PTy::F(1)), &vars).expect("builds");
        let (n2, _) = build(&mut g, "length($pos - 0.5) < 0.3 ? 1 : 0", Some(PTy::F(1)), &vars).expect("builds");
        assert_eq!(n1, n2);
    }

    #[test]
    fn mistakes_are_explained() {
        let mut g = fg();
        let none = |_: &str| None;
        for (src, part) in [("1 +", "expected a value"), ("a = 1 a", "expected ';'"), ("$pos = 1; 2", "system variable"), ("foo * 2", "unknown name 'foo'"), ("vec2(1, 2).z", "has no component z"), ("vec2(1,2) + vec3(1,2,3)", "cannot combine"), ("bar(1)", "unknown function"), ("(1 > 2) * 3", "bool")] {
            let e = build(&mut g, src, None, &none).expect_err(src);
            assert!(e.contains(part), "{src}: {e}");
        }
    }
}
