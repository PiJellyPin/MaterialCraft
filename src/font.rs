// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fonts for the text node: a small built-in bitmap face and a reader for TrueType (`.ttf`), OpenType with CFF
//! outlines (`.otf`), font collections (`.ttc`, first face) and WOFF web fonts.
//!
//! The reader needs a Unicode `cmap` (format 4 or 12) and either `glyf` outlines (including composite glyphs) or a
//! CFF table (Type 2 charstrings, subroutines, CID-keyed fonts). Not supported: WOFF2, CFF2, variable-font
//! instances, hinting, kerning and complex-script shaping.

use crate::vector::{flatten, Poly, Seg, Xf, P};

pub const GLYPHS: [[u8; 10]; 95] = [
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 4, 4, 4, 4, 4, 0, 4, 0, 0],
    [0, 10, 10, 0, 0, 0, 0, 0, 0, 0],
    [0, 10, 10, 31, 10, 31, 10, 10, 0, 0],
    [0, 4, 15, 20, 14, 5, 30, 4, 0, 0],
    [0, 25, 25, 2, 4, 8, 19, 19, 0, 0],
    [0, 12, 18, 20, 8, 21, 18, 13, 0, 0],
    [0, 4, 4, 0, 0, 0, 0, 0, 0, 0],
    [0, 2, 4, 8, 8, 8, 4, 2, 0, 0],
    [0, 8, 4, 2, 2, 2, 4, 8, 0, 0],
    [0, 0, 21, 14, 31, 14, 21, 0, 0, 0],
    [0, 0, 4, 4, 31, 4, 4, 0, 0, 0],
    [0, 0, 0, 0, 0, 12, 4, 8, 0, 0],
    [0, 0, 0, 0, 31, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 12, 12, 0, 0],
    [0, 1, 1, 2, 4, 8, 16, 16, 0, 0],
    [0, 14, 17, 19, 21, 25, 17, 14, 0, 0],
    [0, 4, 12, 4, 4, 4, 4, 14, 0, 0],
    [0, 14, 17, 1, 2, 4, 8, 31, 0, 0],
    [0, 30, 1, 1, 14, 1, 1, 30, 0, 0],
    [0, 2, 6, 10, 18, 31, 2, 2, 0, 0],
    [0, 31, 16, 30, 1, 1, 17, 14, 0, 0],
    [0, 14, 16, 16, 30, 17, 17, 14, 0, 0],
    [0, 31, 1, 2, 4, 8, 8, 8, 0, 0],
    [0, 14, 17, 17, 14, 17, 17, 14, 0, 0],
    [0, 14, 17, 17, 15, 1, 1, 14, 0, 0],
    [0, 0, 12, 12, 0, 12, 12, 0, 0, 0],
    [0, 0, 12, 12, 0, 12, 4, 8, 0, 0],
    [0, 2, 4, 8, 16, 8, 4, 2, 0, 0],
    [0, 0, 0, 31, 0, 31, 0, 0, 0, 0],
    [0, 8, 4, 2, 1, 2, 4, 8, 0, 0],
    [0, 14, 17, 1, 2, 4, 0, 4, 0, 0],
    [0, 14, 17, 23, 21, 23, 16, 14, 0, 0],
    [0, 14, 17, 17, 31, 17, 17, 17, 0, 0],
    [0, 30, 17, 17, 30, 17, 17, 30, 0, 0],
    [0, 14, 17, 16, 16, 16, 17, 14, 0, 0],
    [0, 30, 17, 17, 17, 17, 17, 30, 0, 0],
    [0, 31, 16, 16, 30, 16, 16, 31, 0, 0],
    [0, 31, 16, 16, 30, 16, 16, 16, 0, 0],
    [0, 14, 17, 16, 23, 17, 17, 15, 0, 0],
    [0, 17, 17, 17, 31, 17, 17, 17, 0, 0],
    [0, 14, 4, 4, 4, 4, 4, 14, 0, 0],
    [0, 7, 2, 2, 2, 2, 18, 12, 0, 0],
    [0, 17, 18, 20, 24, 20, 18, 17, 0, 0],
    [0, 16, 16, 16, 16, 16, 16, 31, 0, 0],
    [0, 17, 27, 21, 21, 17, 17, 17, 0, 0],
    [0, 17, 25, 21, 19, 17, 17, 17, 0, 0],
    [0, 14, 17, 17, 17, 17, 17, 14, 0, 0],
    [0, 30, 17, 17, 30, 16, 16, 16, 0, 0],
    [0, 14, 17, 17, 17, 21, 18, 13, 0, 0],
    [0, 30, 17, 17, 30, 20, 18, 17, 0, 0],
    [0, 15, 16, 16, 14, 1, 1, 30, 0, 0],
    [0, 31, 4, 4, 4, 4, 4, 4, 0, 0],
    [0, 17, 17, 17, 17, 17, 17, 14, 0, 0],
    [0, 17, 17, 17, 17, 17, 10, 4, 0, 0],
    [0, 17, 17, 17, 21, 21, 27, 17, 0, 0],
    [0, 17, 17, 10, 4, 10, 17, 17, 0, 0],
    [0, 17, 17, 10, 4, 4, 4, 4, 0, 0],
    [0, 31, 1, 2, 4, 8, 16, 31, 0, 0],
    [0, 14, 8, 8, 8, 8, 8, 14, 0, 0],
    [0, 16, 16, 8, 4, 2, 1, 1, 0, 0],
    [0, 14, 2, 2, 2, 2, 2, 14, 0, 0],
    [0, 4, 10, 17, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 31, 0, 0],
    [0, 8, 4, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 14, 1, 15, 17, 15, 0, 0],
    [0, 16, 16, 30, 17, 17, 17, 30, 0, 0],
    [0, 0, 0, 14, 17, 16, 17, 14, 0, 0],
    [0, 1, 1, 15, 17, 17, 17, 15, 0, 0],
    [0, 0, 0, 14, 17, 31, 16, 14, 0, 0],
    [0, 6, 9, 8, 28, 8, 8, 8, 0, 0],
    [0, 0, 0, 15, 17, 17, 15, 1, 17, 14],
    [0, 16, 16, 30, 17, 17, 17, 17, 0, 0],
    [0, 4, 0, 12, 4, 4, 4, 14, 0, 0],
    [0, 2, 0, 6, 2, 2, 2, 2, 18, 12],
    [0, 16, 16, 18, 20, 24, 20, 18, 0, 0],
    [0, 12, 4, 4, 4, 4, 4, 14, 0, 0],
    [0, 0, 0, 26, 21, 21, 21, 21, 0, 0],
    [0, 0, 0, 30, 17, 17, 17, 17, 0, 0],
    [0, 0, 0, 14, 17, 17, 17, 14, 0, 0],
    [0, 0, 0, 30, 17, 17, 30, 16, 16, 16],
    [0, 0, 0, 15, 17, 17, 15, 1, 1, 1],
    [0, 0, 0, 22, 25, 16, 16, 16, 0, 0],
    [0, 0, 0, 15, 16, 14, 1, 30, 0, 0],
    [0, 8, 8, 30, 8, 8, 9, 6, 0, 0],
    [0, 0, 0, 17, 17, 17, 19, 13, 0, 0],
    [0, 0, 0, 17, 17, 17, 10, 4, 0, 0],
    [0, 0, 0, 17, 17, 21, 21, 10, 0, 0],
    [0, 0, 0, 17, 10, 4, 10, 17, 0, 0],
    [0, 0, 0, 17, 17, 17, 15, 1, 17, 14],
    [0, 0, 0, 31, 2, 4, 8, 31, 0, 0],
    [0, 6, 8, 8, 16, 8, 8, 6, 0, 0],
    [0, 4, 4, 4, 4, 4, 4, 4, 0, 0],
    [0, 12, 2, 2, 1, 2, 2, 12, 0, 0],
    [0, 0, 0, 13, 22, 0, 0, 0, 0, 0],
];

pub enum Font {
    Builtin,
    TrueType(Box<Ttf>),
}

enum Outlines {
    Glyf { loca: usize, loca_long: bool, glyf: usize, glyf_len: usize },
    Cff(Box<Cff>),
}

pub struct Ttf {
    data: Vec<u8>,
    units: f32,
    outlines: Outlines,
    family: String,
    style: String,
    hmtx: usize,
    n_hmtx: usize,
    n_glyphs: usize,
    cmap: Vec<(u32, u32, u32, i32)>, // start, end, first glyph or 0 for delta mapping, delta
    cmap4: Option<(usize, usize)>,
    ascent: f32,
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]))
}
fn i16_at(d: &[u8], o: usize) -> Option<i16> {
    u16_at(d, o).map(|v| v as i16)
}
fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

impl Ttf {
    pub fn parse(input: &[u8]) -> Result<Ttf, String> {
        let tag = input.get(0..4).ok_or("font file is too short")?;
        let (buf, dir): (Vec<u8>, usize) = match tag {
            b"wOF2" => return Err("WOFF2 (Brotli) fonts are not supported; use .ttf, .otf, .ttc or .woff".into()),
            b"wOFF" => (unwoff(input)?, 0),
            b"ttcf" => (input.to_vec(), u32_at(input, 12).ok_or("bad font collection header")? as usize),
            _ => (input.to_vec(), 0),
        };
        let data = &buf[..];
        let flavor = data.get(dir..dir + 4).ok_or("font file is too short")?;
        let cff = flavor == b"OTTO";
        if !cff && flavor != [0, 1, 0, 0] && flavor != b"true" {
            return Err("not a TrueType or OpenType font".into());
        }
        let n = u16_at(data, dir + 4).ok_or("bad font header")? as usize;
        let table = |name: &[u8; 4]| -> Option<(usize, usize)> {
            (0..n).find_map(|i| {
                let r = dir + 12 + i * 16;
                if data.get(r..r + 4)? == name {
                    let (o, l) = (u32_at(data, r + 8)? as usize, u32_at(data, r + 12)? as usize);
                    if o.checked_add(l)? <= data.len() {
                        Some((o, l))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        };
        let (head, _) = table(b"head").ok_or("font has no head table")?;
        let (maxp, _) = table(b"maxp").ok_or("font has no maxp table")?;
        let (hhea, _) = table(b"hhea").ok_or("font has no hhea table")?;
        let (hmtx, _) = table(b"hmtx").ok_or("font has no hmtx table")?;
        let outlines = if cff {
            let (c, cl) = table(b"CFF ").ok_or("OpenType font has no CFF table (CFF2 outlines are not supported)")?;
            Outlines::Cff(Box::new(Cff::parse(data, c, cl)?))
        } else {
            let (loca, _) = table(b"loca").ok_or("font has no loca table (not a TrueType outline font)")?;
            let (glyf, glyf_len) = table(b"glyf").ok_or("font has no glyf table (not a TrueType outline font)")?;
            Outlines::Glyf { loca, loca_long: i16_at(data, head + 50).ok_or("bad head table")? != 0, glyf, glyf_len }
        };
        let (family, style) = match table(b"name") {
            Some((o, l)) => font_names(data, o, l),
            None => (String::new(), String::new()),
        };
        let (cmap, cmap_len) = table(b"cmap").ok_or("font has no cmap table")?;
        let units = u16_at(data, head + 18).ok_or("bad head table")? as f32;
        if units < 16.0 {
            return Err("bad units-per-em".into());
        }
        let n_glyphs = u16_at(data, maxp + 4).ok_or("bad maxp table")? as usize;
        let n_hmtx = u16_at(data, hhea + 34).ok_or("bad hhea table")? as usize;
        let ascent = i16_at(data, hhea + 4).ok_or("bad hhea table")? as f32;
        // pick a Unicode subtable
        let count = u16_at(data, cmap + 2).ok_or("bad cmap")? as usize;
        let mut best: Option<(usize, u16)> = None;
        for i in 0..count {
            let r = cmap + 4 + i * 8;
            let (pid, eid, off) = (u16_at(data, r).ok_or("bad cmap")?, u16_at(data, r + 2).ok_or("bad cmap")?, u32_at(data, r + 4).ok_or("bad cmap")? as usize);
            let fmt = u16_at(data, cmap + off).unwrap_or(0);
            let unicode = pid == 0 || (pid == 3 && (eid == 1 || eid == 10));
            if unicode && (fmt == 4 || fmt == 12) && best.map(|b| fmt == 12 && b.1 != 12).unwrap_or(true) {
                best = Some((cmap + off, fmt));
            }
        }
        let (sub, fmt) = best.ok_or("font has no Unicode character map this reader understands")?;
        let _ = cmap_len;
        let mut font = Ttf { data: buf.clone(), units, outlines, family, style, hmtx, n_hmtx, n_glyphs, cmap: Vec::new(), cmap4: None, ascent };
        if fmt == 12 {
            let groups = u32_at(data, sub + 12).ok_or("bad cmap")? as usize;
            for g in 0..groups.min(100_000) {
                let r = sub + 16 + g * 12;
                let (s, e, gl) = (u32_at(data, r).ok_or("bad cmap")?, u32_at(data, r + 4).ok_or("bad cmap")?, u32_at(data, r + 8).ok_or("bad cmap")?);
                font.cmap.push((s, e, gl, 0));
            }
        } else {
            font.cmap4 = Some((sub, 0));
        }
        Ok(font)
    }

    pub fn glyph_index(&self, ch: char) -> usize {
        let c = ch as u32;
        if let Some((sub, _)) = self.cmap4 {
            let d = &self.data;
            let segx2 = u16_at(d, sub + 6).unwrap_or(0) as usize;
            let seg = segx2 / 2;
            let ends = sub + 14;
            let starts = ends + segx2 + 2;
            let deltas = starts + segx2;
            let ranges = deltas + segx2;
            for i in 0..seg {
                let end = u16_at(d, ends + i * 2).unwrap_or(0) as u32;
                if c <= end {
                    let start = u16_at(d, starts + i * 2).unwrap_or(0) as u32;
                    if c < start {
                        return 0;
                    }
                    let delta = i16_at(d, deltas + i * 2).unwrap_or(0) as i32;
                    let ro = u16_at(d, ranges + i * 2).unwrap_or(0) as usize;
                    let g = if ro == 0 {
                        (c as i32 + delta) as u32 & 0xffff
                    } else {
                        let at = ranges + i * 2 + ro + (c - start) as usize * 2;
                        match u16_at(d, at) {
                            Some(0) | None => 0,
                            Some(g) => (g as i32 + delta) as u32 & 0xffff,
                        }
                    };
                    return g as usize;
                }
            }
            return 0;
        }
        for &(s, e, g, _) in &self.cmap {
            if c >= s && c <= e {
                return (g + (c - s)) as usize;
            }
        }
        0
    }

    pub fn advance(&self, glyph: usize) -> f32 {
        let i = glyph.min(self.n_hmtx.saturating_sub(1));
        u16_at(&self.data, self.hmtx + i * 4).unwrap_or(0) as f32 / self.units
    }

    fn glyph_range(&self, g: usize) -> Option<(usize, usize)> {
        let Outlines::Glyf { loca, loca_long, glyf, glyf_len } = &self.outlines else { return None };
        if g >= self.n_glyphs {
            return None;
        }
        let (a, b) = if *loca_long {
            (u32_at(&self.data, loca + g * 4)? as usize, u32_at(&self.data, loca + g * 4 + 4)? as usize)
        } else {
            (u16_at(&self.data, loca + g * 2)? as usize * 2, u16_at(&self.data, loca + g * 2 + 2)? as usize * 2)
        };
        if b <= a || b > *glyf_len {
            return None;
        }
        Some((glyf + a, glyf + b))
    }

    pub fn family(&self) -> &str {
        &self.family
    }
    pub fn style(&self) -> &str {
        &self.style
    }
    pub fn glyph_count(&self) -> usize {
        self.n_glyphs
    }
    pub fn is_cff(&self) -> bool {
        matches!(self.outlines, Outlines::Cff(_))
    }

    /// Outline in em units (y up), as path segments.
    pub fn outline(&self, glyph: usize, depth: u32) -> Vec<Seg> {
        let mut segs = Vec::new();
        if let Outlines::Cff(c) = &self.outlines {
            if glyph < self.n_glyphs {
                let u = self.units;
                for sg in c.glyph(&self.data, glyph) {
                    segs.push(match sg {
                        Seg::Move(p) => Seg::Move([p[0] / u, p[1] / u]),
                        Seg::Line(p) => Seg::Line([p[0] / u, p[1] / u]),
                        Seg::Quad(a, p) => Seg::Quad([a[0] / u, a[1] / u], [p[0] / u, p[1] / u]),
                        Seg::Cubic(a, b, p) => Seg::Cubic([a[0] / u, a[1] / u], [b[0] / u, b[1] / u], [p[0] / u, p[1] / u]),
                        Seg::Close => Seg::Close,
                    });
                }
            }
            return segs;
        }
        let Some((start, end)) = self.glyph_range(glyph) else { return segs };
        let d = &self.data[..end];
        let Some(nc) = i16_at(d, start) else { return segs };
        let u = self.units;
        if nc >= 0 {
            let nc = nc as usize;
            let mut o = start + 10;
            let mut ends = Vec::with_capacity(nc);
            for _ in 0..nc {
                let Some(e) = u16_at(d, o) else { return segs };
                ends.push(e as usize);
                o += 2;
            }
            let npts = ends.last().map(|e| e + 1).unwrap_or(0);
            let Some(il) = u16_at(d, o) else { return segs };
            o += 2 + il as usize;
            let mut flags = Vec::with_capacity(npts);
            while flags.len() < npts {
                let Some(&f) = d.get(o) else { return segs };
                o += 1;
                flags.push(f);
                if f & 8 != 0 {
                    let Some(&r) = d.get(o) else { return segs };
                    o += 1;
                    for _ in 0..r {
                        if flags.len() < npts {
                            flags.push(f);
                        }
                    }
                }
            }
            let mut read = |short: u8, same: u8| -> Option<Vec<i32>> {
                let mut v = Vec::with_capacity(npts);
                let mut cur = 0i32;
                for &f in &flags {
                    if f & short != 0 {
                        let b = *d.get(o)? as i32;
                        o += 1;
                        cur += if f & same != 0 { b } else { -b };
                    } else if f & same == 0 {
                        cur += i16_at(d, o)? as i32;
                        o += 2;
                    }
                    v.push(cur);
                }
                Some(v)
            };
            let (Some(xs), Some(ys)) = (read(2, 16), read(4, 32)) else { return segs };
            let mut first = 0usize;
            for &e in &ends {
                if e < first || e >= npts {
                    break;
                }
                let pts: Vec<(P, bool)> = (first..=e).map(|i| ([xs[i] as f32 / u, ys[i] as f32 / u], flags[i] & 1 != 0)).collect();
                contour(&pts, &mut segs);
                first = e + 1;
            }
        } else if depth < 4 {
            let mut o = start + 10;
            loop {
                let (Some(fl), Some(gi)) = (u16_at(d, o), u16_at(d, o + 2)) else { break };
                o += 4;
                let (dx, dy) = if fl & 1 != 0 {
                    let r = (i16_at(d, o).unwrap_or(0) as f32, i16_at(d, o + 2).unwrap_or(0) as f32);
                    o += 4;
                    r
                } else {
                    let r = (*d.get(o).unwrap_or(&0) as i8 as f32, *d.get(o + 1).unwrap_or(&0) as i8 as f32);
                    o += 2;
                    r
                };
                let f2 = |o: usize| i16_at(d, o).unwrap_or(0) as f32 / 16384.0;
                let (mut a, mut b, mut c, mut dd) = (1.0, 0.0, 0.0, 1.0);
                if fl & 8 != 0 {
                    a = f2(o);
                    dd = a;
                    o += 2;
                } else if fl & 0x40 != 0 {
                    a = f2(o);
                    dd = f2(o + 2);
                    o += 4;
                } else if fl & 0x80 != 0 {
                    a = f2(o);
                    b = f2(o + 2);
                    c = f2(o + 4);
                    dd = f2(o + 6);
                    o += 8;
                }
                // only offsets are supported for placing a component (point matching is rare)
                let (dx, dy) = if fl & 2 != 0 { (dx / u, dy / u) } else { (0.0, 0.0) };
                let xf = Xf([a, b, c, dd, dx, dy]);
                for s in self.outline(gi as usize, depth + 1) {
                    segs.push(match s {
                        Seg::Move(p) => Seg::Move(xf.apply(p)),
                        Seg::Line(p) => Seg::Line(xf.apply(p)),
                        Seg::Quad(c1, p) => Seg::Quad(xf.apply(c1), xf.apply(p)),
                        Seg::Cubic(c1, c2, p) => Seg::Cubic(xf.apply(c1), xf.apply(c2), xf.apply(p)),
                        Seg::Close => Seg::Close,
                    });
                }
                if fl & 0x20 == 0 {
                    break;
                }
            }
        }
        segs
    }
}

fn contour(pts: &[(P, bool)], out: &mut Vec<Seg>) {
    if pts.len() < 2 {
        return;
    }
    let mid = |a: P, b: P| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
    // start at an on-curve point, or at the midpoint of two off-curve points
    let n = pts.len();
    let (start, begin) = match pts.iter().position(|p| p.1) {
        Some(i) => (pts[i].0, i + 1),
        None => (mid(pts[0].0, pts[n - 1].0), 0),
    };
    out.push(Seg::Move(start));
    let mut ctrl: Option<P> = None;
    let count = if pts.iter().any(|p| p.1) { n - 1 } else { n };
    for k in 0..count {
        let (p, on) = pts[(begin + k) % n];
        match (ctrl, on) {
            (None, true) => out.push(Seg::Line(p)),
            (None, false) => ctrl = Some(p),
            (Some(c), true) => {
                out.push(Seg::Quad(c, p));
                ctrl = None;
            }
            (Some(c), false) => {
                out.push(Seg::Quad(c, mid(c, p)));
                ctrl = Some(p);
            }
        }
    }
    if let Some(c) = ctrl {
        out.push(Seg::Quad(c, start));
    }
    out.push(Seg::Close);
}

impl Font {
    pub fn load(bytes: &[u8]) -> Result<Font, String> {
        Ok(Font::TrueType(Box::new(Ttf::parse(bytes)?)))
    }

    /// One line about the font for the resource list.
    pub fn describe(&self) -> String {
        match self {
            Font::Builtin => "built-in pixel font".into(),
            Font::TrueType(t) => {
                let kind = if t.is_cff() { "OpenType (CFF)" } else { "TrueType" };
                let name = match (t.family(), t.style()) {
                    ("", _) => String::new(),
                    (f, "") => format!("{f}, "),
                    (f, st) => format!("{f} {st}, "),
                };
                format!("{name}{kind}, {} glyphs", t.glyph_count())
            }
        }
    }

    /// Advance of one character in em units.
    fn advance(&self, ch: char) -> f32 {
        match self {
            Font::Builtin => 0.65,
            Font::TrueType(t) => t.advance(t.glyph_index(ch)),
        }
    }

    /// Line height in em units.
    pub fn line_height(&self) -> f32 {
        match self {
            Font::Builtin => 1.2,
            Font::TrueType(t) => (t.ascent / t.units * 1.45).max(1.0),
        }
    }

    pub fn ascent(&self) -> f32 {
        match self {
            Font::Builtin => 0.9,
            Font::TrueType(t) => t.ascent / t.units,
        }
    }

    pub fn text_width(&self, line: &str, spacing: f32) -> f32 {
        line.chars().map(|c| self.advance(c) + spacing).sum::<f32>() - if line.is_empty() { 0.0 } else { spacing }
    }

    /// Word-wraps `text` so no line is wider than `max_em` (in em units); explicit line breaks are kept and a word
    /// longer than a line is broken at the edge. Returns the text with newlines inserted.
    pub fn wrap(&self, text: &str, max_em: f32, spacing: f32) -> String {
        if !(max_em > 0.0) {
            return text.to_string();
        }
        let space = self.advance(' ') + spacing;
        let mut out = String::new();
        for (pi, para) in text.split('\n').enumerate() {
            if pi > 0 {
                out.push('\n');
            }
            let mut line_w = 0.0f32;
            let mut first = true;
            for word in para.split(' ') {
                let ww = self.text_width(word, spacing);
                if !first && line_w + space + ww > max_em {
                    out.push('\n');
                    line_w = 0.0;
                    first = true;
                }
                if !first {
                    out.push(' ');
                    line_w += space;
                }
                if ww > max_em {
                    // break the long word
                    for ch in word.chars() {
                        let cw = self.advance(ch) + spacing;
                        if line_w + cw > max_em && line_w > 0.0 {
                            out.push('\n');
                            line_w = 0.0;
                        }
                        out.push(ch);
                        line_w += cw;
                    }
                } else {
                    out.push_str(word);
                    line_w += ww;
                }
                first = false;
            }
        }
        out
    }

    /// Glyph outlines of one character as polygons in em units (y down, baseline at 0), origin at the pen.
    fn glyph_polys(&self, ch: char, pen: f32, xf_after: &Xf) -> Vec<Poly> {
        match self {
            Font::Builtin => {
                let code = ch as u32;
                if !(32..127).contains(&code) {
                    return vec![];
                }
                let rows = &GLYPHS[(code - 32) as usize];
                // 5 x 10 cell: 1 em = 9 rows, cap height is 7 rows, baseline below row 7
                let px = 1.0 / 9.0 * 0.9;
                let mut out = Vec::new();
                for (r, bits) in rows.iter().enumerate() {
                    for c in 0..5 {
                        if bits & (1 << (4 - c)) != 0 {
                            let x0 = pen + c as f32 * px * 1.0;
                            let y0 = (r as f32 - 8.0) * px;
                            out.push(Poly { pts: vec![xf_after.apply([x0, y0]), xf_after.apply([x0 + px, y0]), xf_after.apply([x0 + px, y0 + px]), xf_after.apply([x0, y0 + px])], closed: true });
                        }
                    }
                }
                out
            }
            Font::TrueType(t) => {
                let segs = t.outline(t.glyph_index(ch), 0);
                // font units are y-up: flip, then move to the pen position
                let xf = Xf([1.0, 0.0, 0.0, -1.0, pen, 0.0]).then(*xf_after);
                flatten(&segs, &xf)
            }
        }
    }

    /// Lay out text (lines split at '\n') as polygons in pixel space.
    /// `size`: em height in pixels; `align`: 0 left, 1 centre, 2 right; the block is centred on (0, 0) and moved by `xf`.
    pub fn layout(&self, text: &str, size: f32, align: u32, spacing: f32, line_spacing: f32, xf: &Xf) -> Vec<Poly> {
        let lines: Vec<&str> = text.split('\n').collect();
        let lh = self.line_height() * line_spacing;
        let block_h = lh * lines.len() as f32;
        let mut out = Vec::new();
        for (li, line) in lines.iter().enumerate() {
            let w = self.text_width(line, spacing);
            let mut pen = match align {
                1 => -w * 0.5,
                2 => -w,
                _ => 0.0,
            };
            if align == 0 {
                pen = -self.text_width(lines.iter().max_by_key(|l| l.chars().count()).unwrap_or(&""), spacing) * 0.5;
            } else if align == 2 {
                pen += self.text_width(lines.iter().max_by_key(|l| l.chars().count()).unwrap_or(&""), spacing) * 0.5;
            }
            let base_y = (li as f32 + 0.5) * lh - block_h * 0.5 + self.ascent() * 0.35;
            for ch in line.chars() {
                let local = Xf::translate(0.0, base_y).then(Xf::scale(size, size)).then(*xf);
                out.extend(self.glyph_polys(ch, pen, &local));
                pen += self.advance(ch) + spacing;
            }
        }
        out
    }
}

// ------------------------------------------------------------------------------------------------ containers
/// Unpacks a WOFF file into a plain sfnt font.
fn unwoff(d: &[u8]) -> Result<Vec<u8>, String> {
    let flavor = d.get(4..8).ok_or("truncated WOFF header")?;
    let n = u16_at(d, 12).ok_or("truncated WOFF header")? as usize;
    let mut out = Vec::new();
    out.extend_from_slice(flavor);
    out.extend_from_slice(&(n as u16).to_be_bytes());
    let mut pow = 1usize;
    let mut log = 0u16;
    while pow * 2 <= n {
        pow *= 2;
        log += 1;
    }
    out.extend_from_slice(&((pow * 16) as u16).to_be_bytes());
    out.extend_from_slice(&log.to_be_bytes());
    out.extend_from_slice(&((n * 16 - pow * 16) as u16).to_be_bytes());
    let mut recs = Vec::new();
    for i in 0..n {
        let r = 44 + i * 20;
        let tag = d.get(r..r + 4).ok_or("truncated WOFF table directory")?;
        let (off, comp, orig, sum) = (u32_at(d, r + 4).ok_or("bad WOFF")? as usize, u32_at(d, r + 8).ok_or("bad WOFF")? as usize, u32_at(d, r + 12).ok_or("bad WOFF")? as usize, u32_at(d, r + 16).ok_or("bad WOFF")?);
        let raw = d.get(off..off.checked_add(comp).ok_or("bad WOFF")?).ok_or("truncated WOFF table")?;
        let data = if comp < orig {
            if raw.len() < 3 {
                return Err("bad WOFF table".into());
            }
            let v = crate::png::inflate(&raw[2..])?;
            if v.len() != orig {
                return Err("WOFF table has the wrong size".into());
            }
            v
        } else {
            raw.to_vec()
        };
        recs.push((tag.to_vec(), sum, data));
    }
    recs.sort_by(|a, b| a.0.cmp(&b.0));
    let mut off = 12 + n * 16;
    for (tag, sum, data) in &recs {
        out.extend_from_slice(tag);
        out.extend_from_slice(&sum.to_be_bytes());
        out.extend_from_slice(&(off as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        off += (data.len() + 3) & !3;
    }
    for (_, _, data) in &recs {
        out.extend_from_slice(data);
        out.resize((out.len() + 3) & !3, 0);
    }
    Ok(out)
}

/// Family and style names from the `name` table (Windows Unicode or Mac Roman records).
fn font_names(d: &[u8], o: usize, len: usize) -> (String, String) {
    let Some(count) = u16_at(d, o + 2) else { return Default::default() };
    let Some(str_off) = u16_at(d, o + 4) else { return Default::default() };
    let mut best: [Option<(u8, String)>; 2] = [None, None];
    for i in 0..(count as usize).min(400) {
        let r = o + 6 + i * 12;
        let (Some(pid), Some(enc), Some(lang), Some(nid), Some(l), Some(so)) = (u16_at(d, r), u16_at(d, r + 2), u16_at(d, r + 4), u16_at(d, r + 6), u16_at(d, r + 8), u16_at(d, r + 10)) else { break };
        let slot = match nid {
            1 | 16 => 0,
            2 | 17 => 1,
            _ => continue,
        };
        let start = o + str_off as usize + so as usize;
        if so as usize + l as usize > len || start + l as usize > d.len() {
            continue;
        }
        let raw = &d[start..start + l as usize];
        let (text, rank) = if pid == 3 || pid == 0 {
            let u: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
            (String::from_utf16_lossy(&u), if pid == 3 && lang == 0x409 { 3 } else { 2 })
        } else if pid == 1 && enc == 0 {
            (raw.iter().map(|b| *b as char).collect(), 1)
        } else {
            continue;
        };
        // typographic names (16, 17) beat the legacy ones (1, 2)
        let rank = rank + if nid >= 16 { 4 } else { 0 };
        if best[slot].as_ref().map(|b| rank > b.0).unwrap_or(true) && !text.is_empty() {
            best[slot] = Some((rank, text));
        }
    }
    let [a, b] = best;
    (a.map(|x| x.1).unwrap_or_default(), b.map(|x| x.1).unwrap_or_default())
}

// ------------------------------------------------------------------------------------------------ CFF outlines
struct CffPriv {
    subrs: Vec<(usize, usize)>,
}

struct Cff {
    charstrings: Vec<(usize, usize)>,
    gsubrs: Vec<(usize, usize)>,
    privs: Vec<CffPriv>,
    fd_select: Option<Vec<u8>>,
}

fn cff_index(d: &[u8], at: usize) -> Result<(Vec<(usize, usize)>, usize), String> {
    let count = u16_at(d, at).ok_or("truncated CFF index")? as usize;
    if count == 0 {
        return Ok((Vec::new(), at + 2));
    }
    let osz = *d.get(at + 2).ok_or("truncated CFF index")? as usize;
    if !(1..=4).contains(&osz) {
        return Err("bad CFF offset size".into());
    }
    let off = |i: usize| -> Option<usize> {
        let p = at + 3 + i * osz;
        let b = d.get(p..p + osz)?;
        Some(b.iter().fold(0usize, |a, x| (a << 8) | *x as usize))
    };
    let base = at + 3 + (count + 1) * osz - 1;
    let mut v = Vec::with_capacity(count);
    for i in 0..count {
        let (a, b) = (off(i).ok_or("truncated CFF index")?, off(i + 1).ok_or("truncated CFF index")?);
        if b < a || base + b > d.len() {
            return Err("bad CFF index".into());
        }
        v.push((base + a, base + b));
    }
    let end = base + off(count).ok_or("truncated CFF index")?;
    Ok((v, end))
}

/// Parses a CFF DICT into (operator, operands) pairs.
fn cff_dict(d: &[u8]) -> Vec<(u16, Vec<f64>)> {
    let mut out = Vec::new();
    let mut ops: Vec<f64> = Vec::new();
    let mut i = 0;
    while i < d.len() {
        let b = d[i];
        i += 1;
        match b {
            0..=21 => {
                let op = if b == 12 {
                    let b1 = d.get(i).copied().unwrap_or(0);
                    i += 1;
                    0x0c00 | b1 as u16
                } else {
                    b as u16
                };
                out.push((op, std::mem::take(&mut ops)));
            }
            28 => {
                if i + 2 > d.len() {
                    break;
                }
                ops.push(i16::from_be_bytes([d[i], d[i + 1]]) as f64);
                i += 2;
            }
            29 => {
                if i + 4 > d.len() {
                    break;
                }
                ops.push(i32::from_be_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]) as f64);
                i += 4;
            }
            30 => {
                let mut s = String::new();
                'nib: while i < d.len() {
                    let v = d[i];
                    i += 1;
                    for n in [v >> 4, v & 15] {
                        match n {
                            0..=9 => s.push((b'0' + n) as char),
                            10 => s.push('.'),
                            11 => s.push('E'),
                            12 => s.push_str("E-"),
                            14 => s.push('-'),
                            15 => break 'nib,
                            _ => {}
                        }
                    }
                }
                ops.push(s.parse().unwrap_or(0.0));
            }
            32..=246 => ops.push(b as f64 - 139.0),
            247..=250 => {
                let b1 = d.get(i).copied().unwrap_or(0) as f64;
                i += 1;
                ops.push((b as f64 - 247.0) * 256.0 + b1 + 108.0);
            }
            251..=254 => {
                let b1 = d.get(i).copied().unwrap_or(0) as f64;
                i += 1;
                ops.push(-(b as f64 - 251.0) * 256.0 - b1 - 108.0);
            }
            _ => {}
        }
    }
    out
}

impl Cff {
    fn parse(d: &[u8], start: usize, len: usize) -> Result<Cff, String> {
        let end = start + len;
        let hdr = *d.get(start + 2).ok_or("truncated CFF header")? as usize;
        if d.get(start) != Some(&1) {
            return Err("unsupported CFF version".into());
        }
        let (_names, p) = cff_index(&d[..end], start + hdr)?;
        let (tops, p) = cff_index(&d[..end], p)?;
        let (_strings, p) = cff_index(&d[..end], p)?;
        let (gsubrs, _) = cff_index(&d[..end], p)?;
        let top = tops.first().ok_or("CFF has no top dictionary")?;
        let dict = cff_dict(&d[top.0..top.1]);
        let get = |op: u16| dict.iter().find(|(o, _)| *o == op).map(|(_, v)| v.clone());
        let cs_off = get(17).and_then(|v| v.first().copied()).ok_or("CFF has no CharStrings")? as usize;
        let (charstrings, _) = cff_index(&d[..end], start + cs_off)?;
        let read_private = |size: usize, off: usize| -> Result<CffPriv, String> {
            let a = start + off;
            let pd = d.get(a..a + size).ok_or("bad CFF private dictionary")?;
            let pdict = cff_dict(pd);
            let subrs = match pdict.iter().find(|(o, _)| *o == 19).and_then(|(_, v)| v.first().copied()) {
                Some(so) => cff_index(&d[..end], a + so as usize)?.0,
                None => Vec::new(),
            };
            Ok(CffPriv { subrs })
        };
        let mut privs = Vec::new();
        let mut fd_select = None;
        if let Some(fda) = get(0x0c24).and_then(|v| v.first().copied()) {
            // CID-keyed font: one private dictionary per font dict
            let (fds, _) = cff_index(&d[..end], start + fda as usize)?;
            for (a, b) in fds {
                let fd = cff_dict(&d[a..b]);
                let pr = fd.iter().find(|(o, _)| *o == 18).map(|(_, v)| v.clone()).unwrap_or_default();
                privs.push(if pr.len() == 2 { read_private(pr[0] as usize, pr[1] as usize)? } else { CffPriv { subrs: Vec::new() } });
            }
            if let Some(fs) = get(0x0c25).and_then(|v| v.first().copied()) {
                let a = start + fs as usize;
                let n = charstrings.len();
                let mut sel = vec![0u8; n];
                match d.get(a) {
                    Some(0) => {
                        for (g, s) in sel.iter_mut().enumerate() {
                            *s = d.get(a + 1 + g).copied().unwrap_or(0);
                        }
                    }
                    Some(3) => {
                        let nr = u16_at(d, a + 1).ok_or("bad FDSelect")? as usize;
                        for r in 0..nr {
                            let o = a + 3 + r * 3;
                            let first = u16_at(d, o).ok_or("bad FDSelect")? as usize;
                            let fd = *d.get(o + 2).ok_or("bad FDSelect")?;
                            let next = u16_at(d, o + 3).ok_or("bad FDSelect")? as usize;
                            for s in sel.iter_mut().take(next.min(n)).skip(first) {
                                *s = fd;
                            }
                        }
                    }
                    _ => {}
                }
                fd_select = Some(sel);
            }
        } else {
            let pr = get(18).unwrap_or_default();
            privs.push(if pr.len() == 2 { read_private(pr[0] as usize, pr[1] as usize)? } else { CffPriv { subrs: Vec::new() } });
        }
        if privs.is_empty() {
            privs.push(CffPriv { subrs: Vec::new() });
        }
        Ok(Cff { charstrings, gsubrs, privs, fd_select })
    }

    fn glyph(&self, d: &[u8], gid: usize) -> Vec<Seg> {
        let Some(&(a, b)) = self.charstrings.get(gid) else { return Vec::new() };
        let fd = self.fd_select.as_ref().and_then(|s| s.get(gid)).copied().unwrap_or(0) as usize;
        let mut st = Cs { stack: Vec::new(), x: 0.0, y: 0.0, open: false, nh: 0, have_w: false, segs: Vec::new(), done: false, steps: 0 };
        self.exec(d, &mut st, a, b, 0, self.privs.get(fd).unwrap_or(&self.privs[0]));
        if st.open {
            st.segs.push(Seg::Close);
        }
        st.segs
    }

    fn exec(&self, d: &[u8], st: &mut Cs, start: usize, end: usize, depth: u32, pv: &CffPriv) {
        if depth > 10 {
            return;
        }
        let bias = |n: usize| -> i32 {
            if n < 1240 {
                107
            } else if n < 33900 {
                1131
            } else {
                32768
            }
        };
        let mut i = start;
        while i < end && !st.done {
            st.steps += 1;
            if st.steps > 200_000 {
                st.done = true;
                return;
            }
            let b = d[i];
            i += 1;
            match b {
                28 => {
                    if i + 2 > end {
                        return;
                    }
                    st.push(i16::from_be_bytes([d[i], d[i + 1]]) as f32);
                    i += 2;
                }
                32..=246 => st.push(b as f32 - 139.0),
                247..=250 => {
                    let b1 = d.get(i).copied().unwrap_or(0) as f32;
                    i += 1;
                    st.push((b as f32 - 247.0) * 256.0 + b1 + 108.0);
                }
                251..=254 => {
                    let b1 = d.get(i).copied().unwrap_or(0) as f32;
                    i += 1;
                    st.push(-(b as f32 - 251.0) * 256.0 - b1 - 108.0);
                }
                255 => {
                    if i + 4 > end {
                        return;
                    }
                    st.push(i32::from_be_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]) as f32 / 65536.0);
                    i += 4;
                }
                1 | 3 | 18 | 23 => st.stems(),
                19 | 20 => {
                    st.stems();
                    i += (st.nh + 7) / 8;
                }
                21 => {
                    st.width_if(|n| n > 2);
                    let (dx, dy) = (st.arg(0), st.arg(1));
                    st.moveto(dx, dy);
                }
                22 => {
                    st.width_if(|n| n > 1);
                    let dx = st.arg(0);
                    st.moveto(dx, 0.0);
                }
                4 => {
                    st.width_if(|n| n > 1);
                    let dy = st.arg(0);
                    st.moveto(0.0, dy);
                }
                5 => {
                    let mut k = 0;
                    while k + 2 <= st.stack.len() {
                        let (dx, dy) = (st.stack[k], st.stack[k + 1]);
                        st.line(dx, dy);
                        k += 2;
                    }
                    st.stack.clear();
                }
                6 | 7 => {
                    let mut h = b == 6;
                    for k in 0..st.stack.len() {
                        let v = st.stack[k];
                        if h {
                            st.line(v, 0.0);
                        } else {
                            st.line(0.0, v);
                        }
                        h = !h;
                    }
                    st.stack.clear();
                }
                8 => {
                    let mut k = 0;
                    while k + 6 <= st.stack.len() {
                        let a: [f32; 6] = st.stack[k..k + 6].try_into().unwrap();
                        st.curve(a);
                        k += 6;
                    }
                    st.stack.clear();
                }
                24 => {
                    let mut k = 0;
                    while k + 6 <= st.stack.len().saturating_sub(2) {
                        let a: [f32; 6] = st.stack[k..k + 6].try_into().unwrap();
                        st.curve(a);
                        k += 6;
                    }
                    if k + 2 <= st.stack.len() {
                        let (dx, dy) = (st.stack[k], st.stack[k + 1]);
                        st.line(dx, dy);
                    }
                    st.stack.clear();
                }
                25 => {
                    let mut k = 0;
                    while k + 2 <= st.stack.len().saturating_sub(6) {
                        let (dx, dy) = (st.stack[k], st.stack[k + 1]);
                        st.line(dx, dy);
                        k += 2;
                    }
                    if k + 6 <= st.stack.len() {
                        let a: [f32; 6] = st.stack[k..k + 6].try_into().unwrap();
                        st.curve(a);
                    }
                    st.stack.clear();
                }
                26 => {
                    let s = st.stack.clone();
                    let mut k = 0;
                    let mut dx1 = 0.0;
                    if s.len() % 2 == 1 {
                        dx1 = s[0];
                        k = 1;
                    }
                    while k + 4 <= s.len() {
                        st.curve([dx1, s[k], s[k + 1], s[k + 2], 0.0, s[k + 3]]);
                        dx1 = 0.0;
                        k += 4;
                    }
                    st.stack.clear();
                }
                27 => {
                    let s = st.stack.clone();
                    let mut k = 0;
                    let mut dy1 = 0.0;
                    if s.len() % 2 == 1 {
                        dy1 = s[0];
                        k = 1;
                    }
                    while k + 4 <= s.len() {
                        st.curve([s[k], dy1, s[k + 1], s[k + 2], s[k + 3], 0.0]);
                        dy1 = 0.0;
                        k += 4;
                    }
                    st.stack.clear();
                }
                30 | 31 => {
                    let s = st.stack.clone();
                    let mut h = b == 31;
                    let mut k = 0;
                    while k + 4 <= s.len() {
                        let last = s.len() - k == 5;
                        let extra = if last { s[k + 4] } else { 0.0 };
                        if h {
                            st.curve([s[k], 0.0, s[k + 1], s[k + 2], extra, s[k + 3]]);
                        } else {
                            st.curve([0.0, s[k], s[k + 1], s[k + 2], s[k + 3], extra]);
                        }
                        h = !h;
                        k += 4;
                    }
                    st.stack.clear();
                }
                10 | 29 => {
                    let (list, n) = if b == 10 { (&pv.subrs, pv.subrs.len()) } else { (&self.gsubrs, self.gsubrs.len()) };
                    if let Some(v) = st.stack.pop() {
                        let idx = v as i32 + bias(n);
                        if idx >= 0 {
                            if let Some(&(a, e)) = list.get(idx as usize) {
                                self.exec(d, st, a, e, depth + 1, pv);
                            }
                        }
                    }
                }
                11 => return,
                14 => {
                    st.width_if(|n| n == 1 || n == 5);
                    if st.open {
                        st.segs.push(Seg::Close);
                        st.open = false;
                    }
                    st.done = true;
                    return;
                }
                12 => {
                    let b1 = d.get(i).copied().unwrap_or(0);
                    i += 1;
                    let s = st.stack.clone();
                    match b1 {
                        34 if s.len() >= 7 => {
                            st.curve([s[0], 0.0, s[1], s[2], s[3], 0.0]);
                            st.curve([s[4], 0.0, s[5], -s[2], s[6], 0.0]);
                        }
                        35 if s.len() >= 12 => {
                            st.curve([s[0], s[1], s[2], s[3], s[4], s[5]]);
                            st.curve([s[6], s[7], s[8], s[9], s[10], s[11]]);
                        }
                        36 if s.len() >= 9 => {
                            st.curve([s[0], s[1], s[2], s[3], s[4], 0.0]);
                            st.curve([s[5], 0.0, s[6], s[7], s[8], -(s[1] + s[3] + s[7])]);
                        }
                        37 if s.len() >= 11 => {
                            let dx = s[0] + s[2] + s[4] + s[6] + s[8];
                            let dy = s[1] + s[3] + s[5] + s[7] + s[9];
                            st.curve([s[0], s[1], s[2], s[3], s[4], s[5]]);
                            let (lx, ly) = if dx.abs() > dy.abs() { (s[10], -dy) } else { (-dx, s[10]) };
                            st.curve([s[6], s[7], s[8], s[9], lx, ly]);
                        }
                        _ => {}
                    }
                    st.stack.clear();
                }
                _ => st.stack.clear(),
            }
        }
    }
}

struct Cs {
    stack: Vec<f32>,
    x: f32,
    y: f32,
    open: bool,
    nh: usize,
    have_w: bool,
    segs: Vec<Seg>,
    done: bool,
    steps: usize,
}

impl Cs {
    fn push(&mut self, v: f32) {
        if self.stack.len() < 96 {
            self.stack.push(v);
        }
    }
    fn arg(&self, i: usize) -> f32 {
        self.stack.get(i).copied().unwrap_or(0.0)
    }
    /// The first stack-clearing operator may carry the glyph width as an extra leading argument.
    fn width_if(&mut self, extra: impl Fn(usize) -> bool) {
        if !self.have_w {
            self.have_w = true;
            if extra(self.stack.len()) && !self.stack.is_empty() {
                self.stack.remove(0);
            }
        }
    }
    fn stems(&mut self) {
        self.width_if(|n| n % 2 == 1);
        self.nh += self.stack.len() / 2;
        self.stack.clear();
    }
    fn moveto(&mut self, dx: f32, dy: f32) {
        if self.open {
            self.segs.push(Seg::Close);
        }
        self.x += dx;
        self.y += dy;
        self.segs.push(Seg::Move([self.x, self.y]));
        self.open = true;
        self.stack.clear();
    }
    fn line(&mut self, dx: f32, dy: f32) {
        self.x += dx;
        self.y += dy;
        if !self.open {
            self.segs.push(Seg::Move([self.x - dx, self.y - dy]));
            self.open = true;
        }
        self.segs.push(Seg::Line([self.x, self.y]));
    }
    fn curve(&mut self, a: [f32; 6]) {
        if !self.open {
            self.segs.push(Seg::Move([self.x, self.y]));
            self.open = true;
        }
        let c1 = [self.x + a[0], self.y + a[1]];
        let c2 = [c1[0] + a[2], c1[1] + a[3]];
        let p = [c2[0] + a[4], c2[1] + a[5]];
        self.segs.push(Seg::Cubic(c1, c2, p));
        self.x = p[0];
        self.y = p[1];
    }
}

// ---------------------------------------------------------------- installed fonts

/// Family and style of a font file (the first face of a collection), read from its name table only.
pub fn names_of(input: &[u8]) -> Option<(String, String)> {
    let dir = if input.get(0..4)? == b"ttcf" { u32_at(input, 12)? as usize } else { 0 };
    let n = u16_at(input, dir + 4)? as usize;
    for i in 0..n.min(256) {
        let r = dir + 12 + i * 16;
        if input.get(r..r + 4)? == b"name" {
            let (o, l) = (u32_at(input, r + 8)? as usize, u32_at(input, r + 12)? as usize);
            if o.checked_add(l)? <= input.len() {
                return Some(font_names(input, o, l));
            }
        }
    }
    None
}

/// Lower-case letters and digits only: "Times New Roman" and "times-new-roman" compare equal.
fn font_key(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
}

/// Folders where the operating system keeps installed fonts (Windows, macOS and Linux).
pub fn font_dirs() -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    let mut v: Vec<PathBuf> = Vec::new();
    if let Some(w) = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot")) {
        v.push(PathBuf::from(w).join("Fonts"));
    }
    if let Some(l) = std::env::var_os("LOCALAPPDATA") {
        v.push(PathBuf::from(l).join("Microsoft").join("Windows").join("Fonts"));
    }
    for d in ["/System/Library/Fonts", "/Library/Fonts", "/usr/share/fonts", "/usr/local/share/fonts", "/usr/X11R6/lib/X11/fonts"] {
        v.push(PathBuf::from(d));
    }
    if let Some(h) = std::env::var_os("HOME") {
        let h = PathBuf::from(h);
        v.push(h.join("Library").join("Fonts"));
        v.push(h.join(".fonts"));
        v.push(h.join(".local").join("share").join("fonts"));
    }
    v.retain(|d| d.is_dir());
    v
}

fn font_files() -> &'static Vec<std::path::PathBuf> {
    static FILES: std::sync::OnceLock<Vec<std::path::PathBuf>> = std::sync::OnceLock::new();
    FILES.get_or_init(|| {
        let mut out = Vec::new();
        let mut stack: Vec<(std::path::PathBuf, u32)> = font_dirs().into_iter().map(|d| (d, 0)).collect();
        while let Some((d, depth)) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if depth < 5 {
                        stack.push((p, depth + 1));
                    }
                } else if matches!(p.extension().and_then(|x| x.to_str()).map(|x| x.to_ascii_lowercase()).as_deref(), Some("ttf" | "otf" | "ttc" | "woff")) {
                    out.push(p);
                }
                if out.len() > 50_000 {
                    return out;
                }
            }
        }
        out.sort();
        out
    })
}

/// (family key, style key, file) of every installed font, read once.
fn font_index() -> &'static Vec<(String, String, std::path::PathBuf)> {
    static INDEX: std::sync::OnceLock<Vec<(String, String, std::path::PathBuf)>> = std::sync::OnceLock::new();
    INDEX.get_or_init(|| {
        font_files()
            .iter()
            .filter(|p| std::fs::metadata(p).map(|m| m.len() < 64 << 20).unwrap_or(false))
            .filter_map(|p| {
                let bytes = std::fs::read(p).ok()?;
                let (f, s) = names_of(&bytes)?;
                Some((font_key(&f), font_key(&s), p.clone()))
            })
            .collect()
    })
}

/// Metric-compatible stand-ins for common faces that are not installed everywhere.
fn font_substitutes(key: &str) -> &'static [&'static str] {
    match key {
        "arial" | "helvetica" | "helveticaneue" | "arialmt" => &["liberationsans", "arimo", "dejavusans", "freesans", "notosans"],
        "timesnewroman" | "times" | "timesnewromanpsmt" => &["liberationserif", "tinos", "dejavuserif", "freeserif", "notoserif"],
        "couriernew" | "courier" => &["liberationmono", "cousine", "dejavusansmono", "freemono", "notosansmono"],
        "verdana" | "tahoma" | "segoeui" | "calibri" => &["dejavusans", "liberationsans", "notosans"],
        "georgia" | "cambria" => &["dejavuserif", "liberationserif", "notoserif"],
        "consolas" | "lucidaconsole" => &["dejavusansmono", "liberationmono", "notosansmono"],
        _ => &[],
    }
}

/// The installed font file for a family name such as "Arial" (the regular face when there are several),
/// or a metric-compatible stand-in when that family is not installed. Lookups are cached.
pub fn system_font(family: &str) -> Option<std::path::PathBuf> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: std::sync::OnceLock<Mutex<HashMap<String, Option<std::path::PathBuf>>>> = std::sync::OnceLock::new();
    let want = font_key(family);
    if want.is_empty() {
        return None;
    }
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().ok().and_then(|c| c.get(&want).cloned()) {
        return hit;
    }
    let mut keys = vec![want.clone()];
    keys.extend(font_substitutes(&want).iter().map(|s| s.to_string()));
    let regular = |s: &str| matches!(s, "" | "regular" | "book" | "normal" | "roman" | "medium");
    let mut found = None;
    'search: for k in &keys {
        // by file name first (arial.ttf, Arial-Regular.ttf, ArialMT.ttf): no file has to be read
        for p in font_files() {
            let stem = p.file_stem().map(|s| font_key(&s.to_string_lossy())).unwrap_or_default();
            if stem == *k || stem == format!("{k}regular") || stem == format!("{k}mt") || stem == format!("{k}book") {
                found = Some(p.clone());
                break 'search;
            }
        }
        // then by the names inside the files, preferring the regular face
        let idx = font_index();
        let hit = idx.iter().find(|(f, s, _)| f == k && regular(s)).or_else(|| idx.iter().find(|(f, s, _)| format!("{f}{s}") == *k)).or_else(|| idx.iter().find(|(f, _, _)| f == k));
        if let Some((_, _, p)) = hit {
            found = Some(p.clone());
            break;
        }
    }
    if let Ok(mut c) = cache.lock() {
        c.insert(want, found.clone());
    }
    found
}
