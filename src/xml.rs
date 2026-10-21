// SPDX-License-Identifier: MIT OR Apache-2.0
//! A small XML reader: enough of XML 1.0 for data files (elements, attributes, text, CDATA, comments, the five named
//! entities and numeric character references). No DTDs, namespaces are kept as part of the name. Malformed input is
//! an error with a line number, never a panic; depth and size are bounded.

use std::fmt;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct XmlError {
    pub line: usize,
    pub msg: String,
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

impl std::error::Error for XmlError {}

/// Deepest nesting accepted.
pub const MAX_DEPTH: usize = 512;

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// First child with this name (ASCII case-insensitive: writers differ in the case of a few tags).
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name.eq_ignore_ascii_case(name))
    }

    /// Value of a `<name v="..."/>` child.
    pub fn v(&self, name: &str) -> Option<&str> {
        self.child(name).and_then(|c| c.attr("v"))
    }

    /// The children of the container `list` (`<list><item/><item/></list>`), whatever the item tags are called.
    pub fn list<'a>(&'a self, list: &str) -> &'a [Element] {
        self.child(list).map(|c| c.children.as_slice()).unwrap_or(&[])
    }

    /// Follow a path of child names.
    pub fn path(&self, names: &[&str]) -> Option<&Element> {
        let mut e = self;
        for n in names {
            e = e.child(n)?;
        }
        Some(e)
    }
}

struct Reader<'a> {
    s: &'a [u8],
    i: usize,
    line: usize,
}

impl<'a> Reader<'a> {
    fn err<T>(&self, msg: impl Into<String>) -> Result<T, XmlError> {
        Err(XmlError { line: self.line, msg: msg.into() })
    }
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn starts(&self, p: &str) -> bool {
        self.s[self.i..].starts_with(p.as_bytes())
    }
    fn bump(&mut self, n: usize) {
        for k in 0..n {
            if self.s.get(self.i + k) == Some(&b'\n') {
                self.line += 1;
            }
        }
        self.i = (self.i + n).min(self.s.len());
    }
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.bump(1);
            } else {
                break;
            }
        }
    }
    /// Skip past the next occurrence of `end`.
    fn skip_until(&mut self, end: &str, what: &str) -> Result<&'a [u8], XmlError> {
        let start = self.i;
        let hay = &self.s[self.i..];
        match hay.windows(end.len()).position(|w| w == end.as_bytes()) {
            Some(p) => {
                self.bump(p + end.len());
                Ok(&self.s[start..start + p])
            }
            None => self.err(format!("unterminated {what}")),
        }
    }
    fn name(&mut self) -> Result<String, XmlError> {
        let start = self.i;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b':') || c >= 0x80 {
                self.bump(1);
            } else {
                break;
            }
        }
        if self.i == start {
            return self.err("expected a name");
        }
        Ok(String::from_utf8_lossy(&self.s[start..self.i]).into_owned())
    }
    /// Skip comments, processing instructions and a doctype between elements.
    fn skip_misc(&mut self) -> Result<(), XmlError> {
        loop {
            self.skip_ws();
            if self.starts("<!--") {
                self.bump(4);
                self.skip_until("-->", "comment")?;
            } else if self.starts("<?") {
                self.bump(2);
                self.skip_until("?>", "processing instruction")?;
            } else if self.starts("<!DOCTYPE") || self.starts("<!doctype") {
                // no internal subsets in data files: skip to the closing bracket, nested brackets included
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
            } else {
                return Ok(());
            }
        }
    }

    fn element(&mut self, depth: usize) -> Result<Element, XmlError> {
        if depth > MAX_DEPTH {
            return self.err("elements nested too deeply");
        }
        if self.peek() != Some(b'<') {
            return self.err("expected '<'");
        }
        self.bump(1);
        let mut el = Element { name: self.name()?, ..Default::default() };
        // attributes
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'/') => {
                    self.bump(1);
                    if self.peek() != Some(b'>') {
                        return self.err("expected '>' after '/'");
                    }
                    self.bump(1);
                    return Ok(el);
                }
                Some(b'>') => {
                    self.bump(1);
                    break;
                }
                Some(_) => {
                    let k = self.name()?;
                    self.skip_ws();
                    if self.peek() != Some(b'=') {
                        return self.err(format!("attribute '{k}' has no value"));
                    }
                    self.bump(1);
                    self.skip_ws();
                    let q = match self.peek() {
                        Some(q @ (b'"' | b'\'')) => q,
                        _ => return self.err(format!("attribute '{k}' value must be quoted")),
                    };
                    self.bump(1);
                    let raw = self.skip_until(if q == b'"' { "\"" } else { "'" }, "attribute value")?;
                    let v = decode(raw).map_err(|m| XmlError { line: self.line, msg: m })?;
                    el.attrs.push((k, v));
                }
                None => return self.err(format!("unterminated tag <{}>", el.name)),
            }
        }
        // content
        loop {
            if self.i >= self.s.len() {
                return self.err(format!("<{}> is not closed", el.name));
            }
            if self.starts("</") {
                self.bump(2);
                let n = self.name()?;
                if n != el.name {
                    return self.err(format!("</{n}> closes <{}>", el.name));
                }
                self.skip_ws();
                if self.peek() != Some(b'>') {
                    return self.err("expected '>'");
                }
                self.bump(1);
                return Ok(el);
            } else if self.starts("<!--") {
                self.bump(4);
                self.skip_until("-->", "comment")?;
            } else if self.starts("<![CDATA[") {
                self.bump(9);
                let raw = self.skip_until("]]>", "CDATA section")?;
                el.text.push_str(&String::from_utf8_lossy(raw));
            } else if self.starts("<?") {
                self.bump(2);
                self.skip_until("?>", "processing instruction")?;
            } else if self.peek() == Some(b'<') {
                let c = self.element(depth + 1)?;
                el.children.push(c);
            } else {
                let start = self.i;
                while let Some(c) = self.peek() {
                    if c == b'<' {
                        break;
                    }
                    self.bump(1);
                }
                let t = decode(&self.s[start..self.i]).map_err(|m| XmlError { line: self.line, msg: m })?;
                // whitespace between elements is layout, not data
                if !t.trim().is_empty() || !el.text.is_empty() {
                    el.text.push_str(&t);
                }
            }
        }
    }
}

/// Replace entity and character references.
fn decode(raw: &[u8]) -> Result<String, String> {
    let s = String::from_utf8_lossy(raw);
    if !s.contains('&') {
        return Ok(s.into_owned());
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = &s[..];
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

/// Parse a document and return its root element.
pub fn parse(text: &str) -> Result<Element, XmlError> {
    let bytes = text.as_bytes();
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let mut r = Reader { s: bytes, i: 0, line: 1 };
    r.skip_misc()?;
    if r.peek() != Some(b'<') {
        return r.err("no root element");
    }
    let root = r.element(0)?;
    r.skip_misc()?;
    if r.i < r.s.len() {
        return r.err("text after the root element");
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_elements_attributes_and_text() {
        let d = parse("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- c -->\n<a x=\"1\" y='two'><b v=\"3\"/><c>hi &amp; &lt;bye&gt; &#65;&#x42;</c><![CDATA[<raw>]]></a>").unwrap();
        assert_eq!(d.name, "a");
        assert_eq!(d.attr("y"), Some("two"));
        assert_eq!(d.v("b"), Some("3"));
        assert_eq!(d.child("C").map(|c| c.text.as_str()), Some("hi & <bye> AB"));
        assert_eq!(d.text, "<raw>");
    }

    #[test]
    fn rejects_broken_documents_without_panicking() {
        for bad in ["", "<a>", "<a></b>", "<a x=1/>", "<a x=\"1/>", "<a>&bogus;</a>", "<a/><b/>", "<a><!-- </a>", "<", "<a &>", "<a>&#xFFFFFFFF;</a>"] {
            assert!(parse(bad).is_err(), "{bad:?} should fail");
        }
        let deep = "<a>".repeat(MAX_DEPTH + 2) + &"</a>".repeat(MAX_DEPTH + 2);
        assert!(parse(&deep).is_err());
    }

    #[test]
    fn reports_the_line() {
        let e = parse("<a>\n<b>\n</c></a>").unwrap_err();
        assert_eq!(e.line, 3);
    }
}
