// SPDX-License-Identifier: MIT OR Apache-2.0
//! A lossless XML document: reading a file and writing it back gives the same bytes. Every piece of the file is
//! kept as it was written (the declaration, comments, whitespace between elements, attribute order and quotes,
//! `<a/>` versus `<a></a>`, entity references), so a program can change one value in a large document and leave
//! everything else exactly as the program that wrote it left it.
//!
//! The reader accepts the same XML 1.0 subset as [`crate::xml`] (no DTD subsets) with the same bounds; malformed
//! input is an error with a line number, never a panic.

use crate::xml::{Element, XmlError, MAX_DEPTH};

/// One piece of an element's content.
#[derive(Debug, Clone, PartialEq)]
pub enum XNode {
    Elem(XElem),
    /// character data between tags, as written (entity references kept)
    Text(String),
    /// a comment, CDATA section, processing instruction or doctype, as written
    Raw(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct XAttr {
    pub name: String,
    /// the value as written between the quotes (entity references kept)
    pub raw: String,
    pub quote: char,
    /// whitespace before the name
    pub pre: String,
    /// what is written between the name and the opening quote (normally `=`)
    pub eq: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct XElem {
    pub name: String,
    pub attrs: Vec<XAttr>,
    /// whitespace before the `>` or `/>` of the start tag
    pub head_tail: String,
    pub children: Vec<XNode>,
    /// written as `<name/>` (has no content)
    pub empty: bool,
    /// whitespace before the `>` of the end tag
    pub end_ws: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct XDoc {
    pub bom: bool,
    /// what comes before the root element: declaration, whitespace, comments
    pub before: Vec<XNode>,
    pub root: XElem,
    /// what comes after the root element
    pub after: Vec<XNode>,
}

/// Escape text for an attribute value written between double quotes.
pub fn escape_attr(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\n' => o.push_str("&#10;"),
            '\r' => o.push_str("&#13;"),
            '\t' => o.push_str("&#9;"),
            c => o.push(c),
        }
    }
    o
}

/// Escape character data.
pub fn escape_text(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            c => o.push(c),
        }
    }
    o
}

/// Replace entity and character references (the five named entities and numeric references).
pub fn unescape(s: &str) -> Result<String, String> {
    if !s.contains('&') {
        return Ok(s.to_string());
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p + 1..];
        let end = rest.find(';').filter(|&e| e <= 12).ok_or_else(|| "'&' without a reference".to_string())?;
        let ent = &rest[..end];
        let c = match ent {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "quot" => '"',
            "apos" => '\'',
            _ if ent.starts_with("#x") || ent.starts_with("#X") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32).ok_or_else(|| format!("bad character reference &{ent};"))?,
            _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32).ok_or_else(|| format!("bad character reference &{ent};"))?,
            _ => return Err(format!("unknown entity &{ent};")),
        };
        out.push(c);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

impl XAttr {
    pub fn new(name: &str, value: &str) -> XAttr {
        XAttr { name: name.to_string(), raw: escape_attr(value), quote: '"', pre: " ".into(), eq: "=".into() }
    }

    /// The value with references replaced (the raw text when it cannot be decoded).
    pub fn value(&self) -> String {
        unescape(&self.raw).unwrap_or_else(|_| self.raw.clone())
    }
}

impl XElem {
    /// A new empty element: `<name/>`.
    pub fn new(name: &str) -> XElem {
        XElem { name: name.to_string(), empty: true, ..Default::default() }
    }

    /// `<name v="value"/>`, the way many formats store one value.
    pub fn with_v(name: &str, value: &str) -> XElem {
        let mut e = XElem::new(name);
        e.attrs.push(XAttr::new("v", value));
        e
    }

    pub fn attr(&self, name: &str) -> Option<String> {
        self.attrs.iter().find(|a| a.name == name).map(XAttr::value)
    }

    /// Set an attribute; an existing one keeps its place and quotes, a new one goes last.
    pub fn set_attr(&mut self, name: &str, value: &str) {
        match self.attrs.iter_mut().find(|a| a.name == name) {
            Some(a) => {
                let esc = escape_attr(value);
                a.raw = if a.quote == '\'' { esc.replace("&quot;", "\"").replace('\'', "&apos;") } else { esc };
            }
            None => self.attrs.push(XAttr::new(name, value)),
        }
    }

    pub fn remove_attr(&mut self, name: &str) {
        self.attrs.retain(|a| a.name != name);
    }

    /// The element children, in order.
    pub fn elems(&self) -> impl Iterator<Item = &XElem> {
        self.children.iter().filter_map(|c| if let XNode::Elem(e) = c { Some(e) } else { None })
    }

    pub fn elems_mut(&mut self) -> impl Iterator<Item = &mut XElem> {
        self.children.iter_mut().filter_map(|c| if let XNode::Elem(e) = c { Some(e) } else { None })
    }

    /// First child element with this name (ASCII case-insensitive, as [`crate::xml::Element::child`]).
    pub fn child(&self, name: &str) -> Option<&XElem> {
        self.elems().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn child_mut(&mut self, name: &str) -> Option<&mut XElem> {
        self.elems_mut().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a XElem> + 'a {
        self.elems().filter(move |c| c.name.eq_ignore_ascii_case(name))
    }

    /// Value of a `<name v="..."/>` child.
    pub fn v(&self, name: &str) -> Option<String> {
        self.child(name).and_then(|c| c.attr("v"))
    }

    /// Follow a path of child names.
    pub fn path(&self, names: &[&str]) -> Option<&XElem> {
        let mut e = self;
        for n in names {
            e = e.child(n)?;
        }
        Some(e)
    }

    pub fn path_mut(&mut self, names: &[&str]) -> Option<&mut XElem> {
        let mut e = self;
        for n in names {
            e = e.child_mut(n)?;
        }
        Some(e)
    }

    /// The child `name`, created (last, formatted like its siblings) when missing.
    pub fn child_or_add(&mut self, name: &str, indent: &Indent, depth: usize) -> &mut XElem {
        let pos = self.children.iter().position(|c| matches!(c, XNode::Elem(e) if e.name.eq_ignore_ascii_case(name)));
        let i = match pos {
            Some(i) => i,
            None => self.insert_elem(XElem::new(name), None, indent, depth),
        };
        match &mut self.children[i] {
            XNode::Elem(e) => e,
            _ => unreachable!("insert_elem returns the index of an element"),
        }
    }

    /// Set the `v` attribute of child `name`, creating `<name v=".."/>` when missing.
    pub fn set_v(&mut self, name: &str, value: &str, indent: &Indent, depth: usize) {
        self.child_or_add(name, indent, depth).set_attr("v", value);
    }

    /// Index (in `children`) of the `n`-th element child.
    fn elem_index(&self, n: usize) -> Option<usize> {
        self.children.iter().enumerate().filter(|(_, c)| matches!(c, XNode::Elem(_))).nth(n).map(|(i, _)| i)
    }

    /// Insert `el` as element child number `at` (`None`: last), with the whitespace of the document's layout.
    /// `depth` is the depth of `self` (the root is 0). Returns the index of the new element in `children`.
    pub fn insert_elem(&mut self, mut el: XElem, at: Option<usize>, indent: &Indent, depth: usize) -> usize {
        indent.apply(&mut el, depth + 1);
        let n_elems = self.elems().count();
        if self.empty {
            self.empty = false;
        }
        match &indent.unit {
            None => {
                let i = match at.filter(|a| *a < n_elems).and_then(|a| self.elem_index(a)) {
                    Some(i) => i,
                    None => self.children.len(),
                };
                self.children.insert(i, XNode::Elem(el));
                i
            }
            Some(_) => {
                let inner = indent.line(depth + 1);
                let outer = indent.line(depth);
                if n_elems == 0 {
                    // only whitespace (or nothing) inside: lay the element out on its own line
                    self.children.retain(|c| !matches!(c, XNode::Text(t) if t.trim().is_empty()));
                    let i = self.children.len();
                    self.children.push(XNode::Text(inner));
                    self.children.push(XNode::Elem(el));
                    self.children.push(XNode::Text(outer));
                    return i + 1;
                }
                match at.filter(|a| *a < n_elems).and_then(|a| self.elem_index(a)) {
                    Some(i) => {
                        // before an existing element: the new element, then the whitespace that element had
                        self.children.insert(i, XNode::Text(inner));
                        self.children.insert(i, XNode::Elem(el));
                        i
                    }
                    None => {
                        let last = self.elem_index(n_elems - 1).unwrap_or(0);
                        self.children.insert(last + 1, XNode::Elem(el));
                        self.children.insert(last + 1, XNode::Text(inner));
                        last + 2
                    }
                }
            }
        }
    }

    /// Remove the element child at index `i` of `children` together with the whitespace that led up to it.
    pub fn remove_at(&mut self, i: usize) -> Option<XElem> {
        if !matches!(self.children.get(i), Some(XNode::Elem(_))) {
            return None;
        }
        let XNode::Elem(e) = self.children.remove(i) else { return None };
        if i > 0 && matches!(self.children.get(i - 1), Some(XNode::Text(t)) if t.trim().is_empty()) {
            self.children.remove(i - 1);
        }
        if self.elems().next().is_none() && self.children.iter().all(|c| matches!(c, XNode::Text(t) if t.trim().is_empty())) {
            // nothing left: keep the closing layout but no inner whitespace
            self.children.clear();
        }
        Some(e)
    }

    /// Remove every child element for which `drop` returns true; returns how many were removed.
    pub fn remove_where(&mut self, mut drop: impl FnMut(&XElem) -> bool) -> usize {
        let mut n = 0;
        let mut i = 0;
        while i < self.children.len() {
            if matches!(&self.children[i], XNode::Elem(e) if drop(e)) {
                self.remove_at(i);
                n += 1;
                // the whitespace before it went too
                i = i.saturating_sub(1);
            } else {
                i += 1;
            }
        }
        n
    }

    /// The decoded text content (character data and CDATA; whitespace-only text between elements is layout).
    pub fn text(&self) -> String {
        let mut t = String::new();
        for c in &self.children {
            match c {
                XNode::Text(s) => {
                    let d = unescape(s).unwrap_or_else(|_| s.clone());
                    if !d.trim().is_empty() || !t.is_empty() {
                        t.push_str(&d);
                    }
                }
                XNode::Raw(r) if r.starts_with("<![CDATA[") => {
                    t.push_str(r.trim_start_matches("<![CDATA[").trim_end_matches("]]>"));
                }
                _ => {}
            }
        }
        t
    }

    /// The same element as the simple reader builds it (decoded attributes and text, no layout).
    pub fn to_element(&self) -> Element {
        Element { name: self.name.clone(), attrs: self.attrs.iter().map(|a| (a.name.clone(), a.value())).collect(), children: self.elems().map(XElem::to_element).collect(), text: self.text() }
    }

    fn write(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.name);
        for a in &self.attrs {
            out.push_str(&a.pre);
            out.push_str(&a.name);
            out.push_str(&a.eq);
            out.push(a.quote);
            out.push_str(&a.raw);
            out.push(a.quote);
        }
        out.push_str(&self.head_tail);
        if self.empty && self.children.is_empty() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        for c in &self.children {
            match c {
                XNode::Elem(e) => e.write(out),
                XNode::Text(t) | XNode::Raw(t) => out.push_str(t),
            }
        }
        out.push_str("</");
        out.push_str(&self.name);
        out.push_str(&self.end_ws);
        out.push('>');
    }

    /// The element as text.
    pub fn to_xml(&self) -> String {
        let mut s = String::new();
        self.write(&mut s);
        s
    }
}

/// How a document lays out its elements: one per line indented by `unit` per level, or all on one line.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Indent {
    /// `None`: no whitespace between elements
    pub unit: Option<String>,
    /// the line break (`\n` or `\r\n`)
    pub newline: String,
}

impl Indent {
    pub fn line(&self, depth: usize) -> String {
        match &self.unit {
            Some(u) => format!("{}{}", self.newline, u.repeat(depth)),
            None => String::new(),
        }
    }

    /// Lay out a new element (and everything in it) for `depth`: whitespace between child elements as the
    /// document does it. Existing whitespace inside `el` is replaced.
    pub fn apply(&self, el: &mut XElem, depth: usize) {
        let has_elems = el.elems().next().is_some();
        if !has_elems {
            return;
        }
        let kids: Vec<XNode> = std::mem::take(&mut el.children).into_iter().filter(|c| !matches!(c, XNode::Text(t) if t.trim().is_empty())).collect();
        for c in kids {
            if matches!(c, XNode::Elem(_) | XNode::Raw(_)) {
                if self.unit.is_some() {
                    el.children.push(XNode::Text(self.line(depth + 1)));
                }
            }
            match c {
                XNode::Elem(mut e) => {
                    self.apply(&mut e, depth + 1);
                    el.children.push(XNode::Elem(e));
                }
                other => el.children.push(other),
            }
        }
        if self.unit.is_some() {
            el.children.push(XNode::Text(self.line(depth)));
        }
        el.empty = false;
    }
}

impl XDoc {
    /// Read a document.
    pub fn parse(text: &str) -> Result<XDoc, XmlError> {
        let (bom, body) = match text.strip_prefix('\u{feff}') {
            Some(b) => (true, b),
            None => (false, text),
        };
        let mut r = Reader { s: body, i: 0, line: 1 };
        let before = r.misc()?;
        if !r.rest().starts_with('<') {
            return r.err("no root element");
        }
        let root = r.element(0)?;
        let after = r.misc()?;
        if r.i < r.s.len() {
            return r.err("text after the root element");
        }
        Ok(XDoc { bom, before, root, after })
    }

    /// The document as text: exactly the text it was read from, with the changes made since.
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        if self.bom {
            s.push('\u{feff}');
        }
        for n in &self.before {
            match n {
                XNode::Elem(e) => e.write(&mut s),
                XNode::Text(t) | XNode::Raw(t) => s.push_str(t),
            }
        }
        self.root.write(&mut s);
        for n in &self.after {
            match n {
                XNode::Elem(e) => e.write(&mut s),
                XNode::Text(t) | XNode::Raw(t) => s.push_str(t),
            }
        }
        s
    }

    /// The layout the document uses, found from the whitespace before the root's first child element.
    pub fn indent(&self) -> Indent {
        let newline = if self.to_newline_hint().contains("\r\n") { "\r\n" } else { "\n" }.to_string();
        let mut unit = None;
        // the first element with element children shows the step from one level to the next
        fn find(e: &XElem, depth: usize) -> Option<(String, usize)> {
            let mut prev: Option<&str> = None;
            for c in &e.children {
                match c {
                    XNode::Text(t) => prev = Some(t),
                    XNode::Elem(_) => {
                        if let Some(t) = prev {
                            if let Some(p) = t.rfind('\n') {
                                return Some((t[p + 1..].to_string(), depth + 1));
                            }
                        }
                        return None;
                    }
                    _ => prev = None,
                }
            }
            None
        }
        if let Some((ws, depth)) = find(&self.root, 0) {
            if depth > 0 && !ws.is_empty() && ws.len() % depth == 0 && ws.chars().all(|c| c == ' ' || c == '\t') {
                unit = Some(ws[..ws.len() / depth].to_string());
            } else if ws.is_empty() {
                unit = Some(String::new());
            }
        }
        Indent { unit, newline }
    }

    fn to_newline_hint(&self) -> String {
        for c in &self.root.children {
            if let XNode::Text(t) = c {
                if t.contains('\n') {
                    return t.clone();
                }
            }
        }
        String::new()
    }
}

struct Reader<'a> {
    s: &'a str,
    i: usize,
    line: usize,
}

impl<'a> Reader<'a> {
    fn err<T>(&self, msg: impl Into<String>) -> Result<T, XmlError> {
        Err(XmlError { line: self.line, msg: msg.into() })
    }
    fn rest(&self) -> &'a str {
        &self.s[self.i..]
    }
    fn peek(&self) -> Option<u8> {
        self.s.as_bytes().get(self.i).copied()
    }
    /// Advance by `n` bytes (kept on a character boundary).
    fn bump(&mut self, n: usize) -> &'a str {
        let mut end = (self.i + n).min(self.s.len());
        while !self.s.is_char_boundary(end) {
            end += 1;
        }
        let part = &self.s[self.i..end];
        self.line += part.bytes().filter(|b| *b == b'\n').count();
        self.i = end;
        part
    }
    fn ws(&mut self) -> &'a str {
        let n = self.rest().bytes().take_while(|c| c.is_ascii_whitespace()).count();
        self.bump(n)
    }
    /// Everything up to and including the next `end`.
    fn until(&mut self, end: &str, what: &str) -> Result<&'a str, XmlError> {
        match self.rest().find(end) {
            Some(p) => Ok(self.bump(p + end.len())),
            None => self.err(format!("unterminated {what}")),
        }
    }
    fn name(&mut self) -> Result<String, XmlError> {
        let n = self.rest().bytes().take_while(|&c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b':') || c >= 0x80).count();
        if n == 0 {
            return self.err("expected a name");
        }
        Ok(self.bump(n).to_string())
    }
    /// A comment, processing instruction, doctype or CDATA section starting here, as written.
    fn raw(&mut self) -> Result<Option<String>, XmlError> {
        let r = self.rest();
        if r.starts_with("<!--") {
            return Ok(Some(self.until("-->", "comment")?.to_string()));
        }
        if r.starts_with("<![CDATA[") {
            return Ok(Some(self.until("]]>", "CDATA section")?.to_string()));
        }
        if r.starts_with("<?") {
            return Ok(Some(self.until("?>", "processing instruction")?.to_string()));
        }
        if r.starts_with("<!DOCTYPE") || r.starts_with("<!doctype") {
            let start = self.i;
            let mut depth = 0i32;
            while let Some(c) = self.peek() {
                self.bump(1);
                match c {
                    b'<' => depth += 1,
                    b'>' => {
                        depth -= 1;
                        if depth <= 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            return Ok(Some(self.s[start..self.i].to_string()));
        }
        Ok(None)
    }
    /// Whitespace, comments and the like outside the root element.
    fn misc(&mut self) -> Result<Vec<XNode>, XmlError> {
        let mut v = Vec::new();
        loop {
            let w = self.ws();
            if !w.is_empty() {
                v.push(XNode::Text(w.to_string()));
            }
            if self.rest().starts_with("<![CDATA[") {
                return self.err("CDATA outside the root element");
            }
            match self.raw()? {
                Some(r) => v.push(XNode::Raw(r)),
                None => return Ok(v),
            }
        }
    }

    fn element(&mut self, depth: usize) -> Result<XElem, XmlError> {
        if depth > MAX_DEPTH {
            return self.err("elements nested too deeply");
        }
        if self.peek() != Some(b'<') {
            return self.err("expected '<'");
        }
        self.bump(1);
        let mut el = XElem { name: self.name()?, ..Default::default() };
        loop {
            let pre = self.ws().to_string();
            match self.peek() {
                Some(b'/') => {
                    self.bump(1);
                    if self.peek() != Some(b'>') {
                        return self.err("expected '>' after '/'");
                    }
                    self.bump(1);
                    el.head_tail = pre;
                    el.empty = true;
                    return Ok(el);
                }
                Some(b'>') => {
                    self.bump(1);
                    el.head_tail = pre;
                    break;
                }
                Some(_) => {
                    if pre.is_empty() && !el.attrs.is_empty() {
                        return self.err("attributes must be separated by whitespace");
                    }
                    let name = self.name()?;
                    let eq_start = self.i;
                    self.ws();
                    if self.peek() != Some(b'=') {
                        return self.err(format!("attribute '{name}' has no value"));
                    }
                    self.bump(1);
                    self.ws();
                    let eq = self.s[eq_start..self.i].to_string();
                    let quote = match self.peek() {
                        Some(b'"') => '"',
                        Some(b'\'') => '\'',
                        _ => return self.err(format!("attribute '{name}' value must be quoted")),
                    };
                    self.bump(1);
                    let v = self.until(if quote == '"' { "\"" } else { "'" }, "attribute value")?;
                    let raw = &v[..v.len() - 1];
                    if raw.contains('<') {
                        return self.err(format!("'<' in the value of attribute '{name}'"));
                    }
                    unescape(raw).map_err(|m| XmlError { line: self.line, msg: m })?;
                    el.attrs.push(XAttr { name, raw: raw.to_string(), quote, pre, eq });
                }
                None => return self.err(format!("unterminated tag <{}>", el.name)),
            }
        }
        loop {
            if self.i >= self.s.len() {
                return self.err(format!("<{}> is not closed", el.name));
            }
            if self.rest().starts_with("</") {
                self.bump(2);
                let n = self.name()?;
                if n != el.name {
                    return self.err(format!("</{n}> closes <{}>", el.name));
                }
                el.end_ws = self.ws().to_string();
                if self.peek() != Some(b'>') {
                    return self.err("expected '>'");
                }
                self.bump(1);
                return Ok(el);
            }
            if let Some(r) = self.raw()? {
                el.children.push(XNode::Raw(r));
            } else if self.peek() == Some(b'<') {
                let c = self.element(depth + 1)?;
                el.children.push(XNode::Elem(c));
            } else {
                let n = self.rest().find('<').unwrap_or(self.s.len() - self.i);
                let t = self.bump(n);
                unescape(t).map_err(|m| XmlError { line: self.line, msg: m })?;
                el.children.push(XNode::Text(t.to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- top -->\n<package>\n <identifier v=\"pkg\"/>\n <content>\n  <graph x='a &amp; b'  y = \"2\" >\n   <name v=\"&lt;x&gt;\"/>\n   <empty></empty>\n   <text>hi &amp; <![CDATA[<raw>]]></text>\n  </graph>\n </content>\n</package >\n";

    #[test]
    fn writes_back_exactly_what_it_read() {
        let d = XDoc::parse(SAMPLE).unwrap();
        assert_eq!(d.to_text(), SAMPLE);
        let bom = format!("\u{feff}{SAMPLE}");
        assert_eq!(XDoc::parse(&bom).unwrap().to_text(), bom);
        let crlf = SAMPLE.replace('\n', "\r\n");
        assert_eq!(XDoc::parse(&crlf).unwrap().to_text(), crlf);
        let compact = "<a><b v=\"1\"/><c/></a>";
        assert_eq!(XDoc::parse(compact).unwrap().to_text(), compact);
    }

    #[test]
    fn reads_values_and_text() {
        let d = XDoc::parse(SAMPLE).unwrap();
        let g = d.root.path(&["content", "graph"]).unwrap();
        assert_eq!(g.attr("x").as_deref(), Some("a & b"));
        assert_eq!(g.v("name").as_deref(), Some("<x>"));
        assert_eq!(g.child("text").unwrap().text(), "hi & <raw>");
        let simple = d.root.to_element();
        assert_eq!(simple, crate::xml::parse(SAMPLE).unwrap());
    }

    #[test]
    fn changes_touch_only_what_they_change() {
        let mut d = XDoc::parse(SAMPLE).unwrap();
        let ind = d.indent();
        assert_eq!(ind.unit.as_deref(), Some(" "));
        d.root.path_mut(&["content", "graph"]).unwrap().set_attr("y", "3\"");
        let t = d.to_text();
        assert_eq!(t, SAMPLE.replace("y = \"2\"", "y = \"3&quot;\""));
        // a new element is laid out like its siblings
        let mut d = XDoc::parse(SAMPLE).unwrap();
        d.root.set_v("fileUID", "{1}", &ind, 0);
        assert!(d.to_text().contains("</content>\n <fileUID v=\"{1}\"/>\n</package >"), "{}", d.to_text());
        // a new element with children inside an empty one
        let mut d = XDoc::parse(SAMPLE).unwrap();
        let mut n = XElem::new("node");
        n.insert_elem(XElem::with_v("uid", "7"), None, &Indent::default(), 0);
        let g = d.root.path_mut(&["content", "graph"]).unwrap();
        let e = g.child_mut("empty").unwrap();
        e.insert_elem(n, None, &ind, 3);
        assert!(d.to_text().contains("   <empty>\n    <node>\n     <uid v=\"7\"/>\n    </node>\n   </empty>\n"), "{}", d.to_text());
        // removing an element takes its line with it
        let mut d = XDoc::parse(SAMPLE).unwrap();
        let g = d.root.path_mut(&["content", "graph"]).unwrap();
        assert_eq!(g.remove_where(|e| e.name == "empty"), 1);
        assert_eq!(d.to_text(), SAMPLE.replace("   <empty></empty>\n", ""));
    }

    #[test]
    fn compact_documents_stay_compact() {
        let mut d = XDoc::parse("<a><b v=\"1\"/></a>").unwrap();
        let ind = d.indent();
        assert_eq!(ind.unit, None);
        let mut c = XElem::new("c");
        c.insert_elem(XElem::with_v("d", "2"), None, &ind, 1);
        d.root.insert_elem(c, Some(0), &ind, 0);
        assert_eq!(d.to_text(), "<a><c><d v=\"2\"/></c><b v=\"1\"/></a>");
    }

    #[test]
    fn rejects_broken_documents_without_panicking() {
        for bad in ["", "<a>", "<a></b>", "<a x=1/>", "<a x=\"1/>", "<a>&bogus;</a>", "<a/><b/>", "<a><!-- </a>", "<", "<a &>", "<a>&#xFFFFFFFF;</a>", "<a x=\"<\"/>", "<a x=\"1\"y=\"2\"/>", "é<a/>"] {
            assert!(XDoc::parse(bad).is_err(), "{bad:?} should fail");
        }
        let deep = "<a>".repeat(MAX_DEPTH + 2) + &"</a>".repeat(MAX_DEPTH + 2);
        assert!(XDoc::parse(&deep).is_err());
        // multi-byte text survives
        let t = "<a v=\"ä€😀\">ß</a>";
        assert_eq!(XDoc::parse(t).unwrap().to_text(), t);
    }
}
