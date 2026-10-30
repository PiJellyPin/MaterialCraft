// SPDX-License-Identifier: MIT OR Apache-2.0
//! "Visible if" conditions of graph inputs: a short expression over the other inputs' values (`input["use_mask"]`,
//! `input.mode == 2 && input.offset.x > 0.5`) that decides whether an instance node shows the control.
//!
//! Values are numbers or vectors of numbers; booleans are 1 and 0, and a number is true when it is not 0. Vectors
//! take part through their components (`.x .y .z .w`, or `.r .g .b .a`). An expression that does not parse, names
//! an input that is not there or does not give a single value is not an answer, and the control stays visible.

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Id(String),
    Str(String),
    Op(&'static str),
}

const OPS: &[&str] = &["&&", "||", "<=", ">=", "==", "!=", "<", ">", "!", "+", "-", "*", "/", "%", "(", ")", "[", "]", "."];

fn tokenize(s: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_digit() || (ch == '.' && c.get(i + 1).map(|d| d.is_ascii_digit()).unwrap_or(false)) {
            let st = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            // an exponent: 1e-3
            if i < c.len() && (c[i] == 'e' || c[i] == 'E') {
                let mut j = i + 1;
                if j < c.len() && (c[j] == '+' || c[j] == '-') {
                    j += 1;
                }
                if j < c.len() && c[j].is_ascii_digit() {
                    i = j;
                    while i < c.len() && c[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let t: String = c[st..i].iter().collect();
            out.push(Tok::Num(t.parse().map_err(|_| format!("bad number '{t}'"))?));
        } else if ch.is_alphabetic() || ch == '_' {
            let st = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(Tok::Id(c[st..i].iter().collect()));
        } else if ch == '"' || ch == '\'' {
            let st = i + 1;
            i += 1;
            while i < c.len() && c[i] != ch {
                i += 1;
            }
            if i >= c.len() {
                return Err("unterminated string".into());
            }
            out.push(Tok::Str(c[st..i].iter().collect()));
            i += 1;
        } else {
            let rest: String = c[i..(i + 2).min(c.len())].iter().collect();
            let op = OPS.iter().find(|o| rest.starts_with(**o)).ok_or_else(|| format!("unexpected '{ch}'"))?;
            out.push(Tok::Op(op));
            i += op.len();
        }
    }
    Ok(out)
}

#[derive(Debug, Clone)]
enum Ex {
    Num(f64),
    Input(String),
    Comp(Box<Ex>, usize),
    Not(Box<Ex>),
    Neg(Box<Ex>),
    Bin(&'static str, Box<Ex>, Box<Ex>),
}

struct Parser {
    t: Vec<Tok>,
    i: usize,
}

impl Parser {
    fn peek_op(&self, ops: &[&'static str]) -> Option<&'static str> {
        match self.t.get(self.i) {
            Some(Tok::Op(o)) if ops.contains(o) => Some(o),
            _ => None,
        }
    }

    fn expect(&mut self, op: &str) -> Result<(), String> {
        match self.t.get(self.i) {
            Some(Tok::Op(o)) if *o == op => {
                self.i += 1;
                Ok(())
            }
            _ => Err(format!("expected '{op}'")),
        }
    }

    /// One level of left-associative binary operators, above `next`.
    fn binary(&mut self, ops: &[&'static str], next: fn(&mut Parser) -> Result<Ex, String>) -> Result<Ex, String> {
        let mut a = next(self)?;
        while let Some(op) = self.peek_op(ops) {
            self.i += 1;
            let b = next(self)?;
            a = Ex::Bin(op, Box::new(a), Box::new(b));
        }
        Ok(a)
    }

    // from the loosest to the tightest: || then && then == != then < > <= >= then + - then * / %
    fn or(&mut self) -> Result<Ex, String> {
        self.binary(&["||"], Parser::and)
    }
    fn and(&mut self) -> Result<Ex, String> {
        self.binary(&["&&"], Parser::eq)
    }
    fn eq(&mut self) -> Result<Ex, String> {
        self.binary(&["==", "!="], Parser::rel)
    }
    fn rel(&mut self) -> Result<Ex, String> {
        self.binary(&["<", ">", "<=", ">="], Parser::add)
    }
    fn add(&mut self) -> Result<Ex, String> {
        self.binary(&["+", "-"], Parser::mul)
    }
    fn mul(&mut self) -> Result<Ex, String> {
        self.binary(&["*", "/", "%"], Parser::unary)
    }

    fn unary(&mut self) -> Result<Ex, String> {
        match self.peek_op(&["!", "-", "+"]) {
            Some(op) => {
                self.i += 1;
                let a = self.unary()?;
                Ok(match op {
                    "!" => Ex::Not(Box::new(a)),
                    "-" => Ex::Neg(Box::new(a)),
                    _ => a,
                })
            }
            None => self.postfix(),
        }
    }

    fn postfix(&mut self) -> Result<Ex, String> {
        let mut a = self.primary()?;
        while self.peek_op(&["."]).is_some() {
            self.i += 1;
            let lane = match self.t.get(self.i) {
                Some(Tok::Id(s)) => match s.as_str() {
                    "x" | "r" => 0,
                    "y" | "g" => 1,
                    "z" | "b" => 2,
                    "w" | "a" => 3,
                    _ => return Err(format!("no component '{s}'")),
                },
                _ => return Err("expected a component after '.'".into()),
            };
            self.i += 1;
            a = Ex::Comp(Box::new(a), lane);
        }
        Ok(a)
    }

    fn primary(&mut self) -> Result<Ex, String> {
        let t = self.t.get(self.i).cloned().ok_or("unexpected end")?;
        self.i += 1;
        match t {
            Tok::Num(x) => Ok(Ex::Num(x)),
            Tok::Op("(") => {
                let a = self.or()?;
                self.expect(")")?;
                Ok(a)
            }
            Tok::Id(s) if s == "true" => Ok(Ex::Num(1.0)),
            Tok::Id(s) if s == "false" => Ok(Ex::Num(0.0)),
            // input.name or input["name"]
            Tok::Id(s) if s == "input" => match (self.t.get(self.i).cloned(), self.t.get(self.i + 1).cloned()) {
                (Some(Tok::Op(".")), Some(Tok::Id(name))) => {
                    self.i += 2;
                    Ok(Ex::Input(name))
                }
                (Some(Tok::Op("[")), Some(Tok::Str(name))) => {
                    self.i += 2;
                    self.expect("]")?;
                    Ok(Ex::Input(name))
                }
                _ => Err("expected input.name or input[\"name\"]".into()),
            },
            other => Err(format!("unexpected {other:?}")),
        }
    }
}

fn parse(s: &str) -> Result<Ex, String> {
    let mut p = Parser { t: tokenize(s)?, i: 0 };
    let e = p.or()?;
    if p.i != p.t.len() {
        return Err("unexpected text after the expression".into());
    }
    Ok(e)
}

/// A single number (a one-lane value).
fn one(v: Vec<f64>) -> Result<f64, String> {
    match v.as_slice() {
        [x] => Ok(*x),
        _ => Err("a vector where a single value is needed; use .x, .y ...".into()),
    }
}

fn eval_ex(e: &Ex, input: &dyn Fn(&str) -> Option<Vec<f32>>) -> Result<Vec<f64>, String> {
    let b = |x: bool| vec![x as i32 as f64];
    Ok(match e {
        Ex::Num(x) => vec![*x],
        Ex::Input(name) => {
            let v = input(name).ok_or_else(|| format!("no value input '{name}'"))?;
            if v.is_empty() {
                return Err(format!("'{name}' has no value"));
            }
            v.into_iter().map(f64::from).collect()
        }
        Ex::Comp(a, lane) => vec![*eval_ex(a, input)?.get(*lane).ok_or("no such component")?],
        Ex::Not(a) => b(one(eval_ex(a, input)?)? == 0.0),
        Ex::Neg(a) => vec![-one(eval_ex(a, input)?)?],
        // both sides are always computed, so a wrong name anywhere makes the whole condition invalid
        Ex::Bin(op, a, c) => {
            let (x, y) = (one(eval_ex(a, input)?)?, one(eval_ex(c, input)?)?);
            match *op {
                "||" => b(x != 0.0 || y != 0.0),
                "&&" => b(x != 0.0 && y != 0.0),
                "==" => b(x == y),
                "!=" => b(x != y),
                "<" => b(x < y),
                ">" => b(x > y),
                "<=" => b(x <= y),
                ">=" => b(x >= y),
                "+" => vec![x + y],
                "-" => vec![x - y],
                "*" => vec![x * y],
                "/" => vec![x / y],
                _ => vec![x % y],
            }
        }
    })
}

/// The value of condition `expr`, with `input` giving the value of each input by identifier (one number per lane;
/// `None` when there is no such input or its value is not a plain number). An error when the expression does not
/// parse or cannot be computed.
pub fn eval(expr: &str, input: &dyn Fn(&str) -> Option<Vec<f32>>) -> Result<bool, String> {
    let x = one(eval_ex(&parse(expr)?, input)?)?;
    if x.is_nan() {
        return Err("not a number".into());
    }
    Ok(x != 0.0)
}

/// Whether a control with condition `expr` shows: an empty or invalid condition shows it.
pub fn visible(expr: &str, input: &dyn Fn(&str) -> Option<Vec<f32>>) -> bool {
    expr.trim().is_empty() || eval(expr, input).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vals(name: &str) -> Option<Vec<f32>> {
        match name {
            "use_mask" => Some(vec![1.0]),
            "off" => Some(vec![0.0]),
            "mode" => Some(vec![2.0]),
            "amount" => Some(vec![0.75]),
            "offset" => Some(vec![0.25, 3.0]),
            "tint" => Some(vec![0.1, 0.2, 0.3, 1.0]),
            _ => None,
        }
    }

    fn ev(s: &str) -> Result<bool, String> {
        eval(s, &vals)
    }

    #[test]
    fn booleans_and_lookups() {
        assert_eq!(ev("input[\"use_mask\"]"), Ok(true));
        assert_eq!(ev("input.use_mask"), Ok(true));
        assert_eq!(ev("input['off']"), Ok(false));
        assert_eq!(ev("!input.off"), Ok(true));
        assert_eq!(ev("input.use_mask == true"), Ok(true));
        assert_eq!(ev("input.use_mask == false"), Ok(false));
        assert_eq!(ev("input.off != true"), Ok(true));
        assert_eq!(ev("true"), Ok(true));
        assert_eq!(ev("false"), Ok(false));
        // a number is true when it is not 0
        assert_eq!(ev("input.amount"), Ok(true));
        assert_eq!(ev("input.mode - 2"), Ok(false));
    }

    #[test]
    fn comparisons_logic_and_arithmetic() {
        assert_eq!(ev("input[\"mode\"] == 2"), Ok(true));
        assert_eq!(ev("input.mode < 2"), Ok(false));
        assert_eq!(ev("input.mode <= 2 && input.amount > 0.5"), Ok(true));
        assert_eq!(ev("input.mode >= 3 || input.amount > 0.8"), Ok(false));
        assert_eq!(ev("input.off || input.use_mask"), Ok(true));
        assert_eq!(ev("input.off || input.use_mask && input.mode == 1"), Ok(false), "&& binds tighter than ||");
        assert_eq!(ev("(input.off || input.use_mask) && !(input.mode == 1)"), Ok(true));
        assert_eq!(ev("input.mode * 2 + 1 == 5"), Ok(true));
        assert_eq!(ev("-input.amount < -0.5"), Ok(true));
        assert_eq!(ev("input.mode % 2 == 0 && 1.5e1 / 3 == 5"), Ok(true));
        assert_eq!(ev(".5 < input.amount"), Ok(true));
    }

    #[test]
    fn vector_components() {
        assert_eq!(ev("input.offset.y < 3"), Ok(false));
        assert_eq!(ev("input[\"offset\"].y >= 3"), Ok(true));
        assert_eq!(ev("input.offset.x == 0.25"), Ok(true));
        assert_eq!(ev("input.tint.b > input.tint.r"), Ok(true));
        assert_eq!(ev("input.tint.w == 1"), Ok(true));
        assert!(ev("input.offset.z > 0").is_err(), "a float2 has no z");
        assert!(ev("input.offset > 0").is_err(), "a whole vector is not a condition");
    }

    #[test]
    fn invalid_conditions_keep_the_control_visible() {
        for bad in ["input.missing", "input[\"Use_Mask\"]", "input.use_mask &&", "input.use_mask = 1", "(input.mode == 2", "input.mode == 2)", "input", "input[mode]", "use_mask", "True", "input.mode @ 2", "input.tint", "input.amount.q", "0/0", "\"x\""] {
            assert!(ev(bad).is_err(), "{bad} should be invalid");
            assert!(visible(bad, &vals), "{bad} should keep the control visible");
        }
        // a wrong name hides nothing even where the other side alone would decide
        assert!(visible("input.off && input.missing", &vals));
        assert!(visible("", &vals) && visible("   ", &vals));
        assert!(!visible("input.off", &vals));
        assert!(!visible("input.mode != 2", &vals));
    }
}
