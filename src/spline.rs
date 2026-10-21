// SPDX-License-Identifier: MIT OR Apache-2.0
//! Splines: editable bezier paths that drive the path nodes. A spline is a list of paths; each path is a list of
//! points with optional handles and a width factor. Coordinates are texture coordinates (0..1, y down), so a spline
//! does not depend on the resolution and it wraps with the texture.
//!
//! Text format (a spline is stored as an embedded resource, normally named `*.spl`):
//! ```text
//! spline 1
//! path closed=1 smooth=1
//! pt 0.2 0.5
//! pt 0.5 0.2 hin=-0.1,0 hout=0.1,0 w=1.5
//! ```
//! `hin` / `hout` are handle offsets from the point. A point without handles gets automatic smooth ones when the
//! path has `smooth=1` (the default) and a sharp corner when `smooth=0`. `w` scales the path width at that point.

use std::fmt::Write as _;

pub type P = [f32; 2];

#[derive(Clone, Debug, PartialEq)]
pub struct SPoint {
    pub p: P,
    pub hin: Option<P>,
    pub hout: Option<P>,
    /// width factor at this point (1 = the node's width)
    pub w: f32,
    /// height value at this point, 0..1 (read by the path sampling node)
    pub z: f32,
    /// tangent mode in the editor: 0 auto, 1 mirrored, 2 free, 3 corner
    pub tan: u8,
}

impl SPoint {
    pub fn new(x: f32, y: f32) -> SPoint {
        SPoint { p: [x, y], hin: None, hout: None, w: 1.0, z: 1.0, tan: 0 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SPath {
    pub closed: bool,
    pub smooth: bool,
    pub pts: Vec<SPoint>,
}

impl Default for SPath {
    fn default() -> Self {
        SPath { closed: false, smooth: true, pts: Vec::new() }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spline {
    pub paths: Vec<SPath>,
}

/// A flattened path in pixel coordinates.
#[derive(Clone, Debug)]
pub struct Polyline {
    pub pts: Vec<P>,
    /// width factor at every point
    pub w: Vec<f32>,
    /// height value at every point
    pub z: Vec<f32>,
    pub closed: bool,
}

/// A point on a path with its direction, taken at an even spacing.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub p: P,
    /// unit tangent
    pub t: P,
    pub w: f32,
    /// distance along the path in pixels
    pub s: f32,
}

impl Polyline {
    pub fn length(&self) -> f32 {
        let n = self.pts.len();
        let segs = if self.closed { n } else { n.saturating_sub(1) };
        (0..segs).map(|i| dist(self.pts[i], self.pts[(i + 1) % n])).sum()
    }

    /// Points at every `step` pixels, starting at `offset`. A closed path ends before it would repeat its start.
    pub fn resample(&self, step: f32, offset: f32) -> Vec<Sample> {
        let n = self.pts.len();
        let mut out = Vec::new();
        if n < 2 || step <= 1e-3 {
            return out;
        }
        let segs = if self.closed { n } else { n - 1 };
        let total = self.length();
        let mut next = offset.max(0.0);
        let mut acc = 0.0f32;
        let limit = if self.closed { total - 1e-3 } else { total + 1e-3 };
        for i in 0..segs {
            let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
            let (wa, wb) = (self.w[i], self.w[(i + 1) % n]);
            let l = dist(a, b);
            if l < 1e-6 {
                continue;
            }
            let t = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
            while next <= acc + l && next <= limit {
                let k = (next - acc) / l;
                out.push(Sample { p: [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k], t, w: wa + (wb - wa) * k, s: next });
                next += step;
            }
            acc += l;
        }
        out
    }
}

fn dist(a: P, b: P) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

impl SPath {
    /// Control points of the segment that leaves point `i`: (start, c1, c2, end, width a, width b).
    fn segment(&self, i: usize) -> (P, P, P, P, f32, f32) {
        let n = self.pts.len();
        let j = (i + 1) % n;
        let (a, b) = (&self.pts[i], &self.pts[j]);
        let c1 = match a.hout {
            Some(h) => [a.p[0] + h[0], a.p[1] + h[1]],
            None => {
                let o = self.auto_handle(i);
                [a.p[0] + o[0], a.p[1] + o[1]]
            }
        };
        let c2 = match b.hin {
            Some(h) => [b.p[0] + h[0], b.p[1] + h[1]],
            None => {
                let o = self.auto_handle(j);
                [b.p[0] - o[0], b.p[1] - o[1]]
            }
        };
        (a.p, c1, c2, b.p, a.w, b.w)
    }

    /// Automatic handle offset (pointing forward) of point `i`.
    fn auto_handle(&self, i: usize) -> P {
        if !self.smooth || self.pts[i].tan == 3 {
            return [0.0, 0.0];
        }
        let n = self.pts.len();
        let p = self.pts[i].p;
        let prev = if i > 0 { self.pts[i - 1].p } else if self.closed { self.pts[n - 1].p } else { p };
        let next = if i + 1 < n { self.pts[i + 1].p } else if self.closed { self.pts[0].p } else { p };
        [(next[0] - prev[0]) / 6.0, (next[1] - prev[1]) / 6.0]
    }

    pub fn segment_count(&self) -> usize {
        let n = self.pts.len();
        if self.closed && n > 2 {
            n
        } else {
            n.saturating_sub(1)
        }
    }

    /// Effective handle offsets of point `i`: (towards the previous point, towards the next point).
    pub fn handles(&self, i: usize) -> (P, P) {
        let a = self.auto_handle(i);
        let p = &self.pts[i];
        (p.hin.unwrap_or([-a[0], -a[1]]), p.hout.unwrap_or(a))
    }

    /// Nearest point of the curve to `p` (texture coordinates): (segment, t, distance in pixels).
    pub fn nearest(&self, p: P, w: f32, h: f32) -> Option<(usize, f32, f32)> {
        let mut best: Option<(usize, f32, f32)> = None;
        for i in 0..self.segment_count() {
            let (a, c1, c2, b, _, _) = self.segment(i);
            for k in 0..=32 {
                let t = k as f32 / 32.0;
                let q = bezier(a, c1, c2, b, t);
                let d = (((q[0] - p[0]) * w).powi(2) + ((q[1] - p[1]) * h).powi(2)).sqrt();
                if best.map_or(true, |b| d < b.2) {
                    best = Some((i, t, d));
                }
            }
        }
        best
    }

    /// Insert a point on segment `seg` at parameter `t` without changing the curve. Returns the new point's index.
    pub fn split(&mut self, seg: usize, t: f32) -> usize {
        let n = self.pts.len();
        if seg >= self.segment_count() {
            return n.saturating_sub(1);
        }
        // automatic handles depend on the neighbours, which change here: freeze them so no other segment moves
        for k in [seg, (seg + 1) % n] {
            let (hi, ho) = self.handles(k);
            self.pts[k].hin = Some(hi);
            self.pts[k].hout = Some(ho);
        }
        let (p0, c1, c2, p3, wa, wb) = self.segment(seg);
        let lerp = |a: P, b: P| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        let (q0, q1, q2) = (lerp(p0, c1), lerp(c1, c2), lerp(c2, p3));
        let (r0, r1) = (lerp(q0, q1), lerp(q1, q2));
        let s = lerp(r0, r1);
        let j = (seg + 1) % n;
        self.pts[seg].hout = Some([q0[0] - p0[0], q0[1] - p0[1]]);
        self.pts[j].hin = Some([q2[0] - p3[0], q2[1] - p3[1]]);
        let np = SPoint { p: s, hin: Some([r0[0] - s[0], r0[1] - s[1]]), hout: Some([r1[0] - s[0], r1[1] - s[1]]), w: wa + (wb - wa) * t, z: self.pts[seg].z + (self.pts[j].z - self.pts[seg].z) * t, tan: 2 };
        let at = seg + 1;
        self.pts.insert(at, np);
        at
    }

    /// Flatten to pixels for a texture of `w` x `h` pixels.
    pub fn flatten(&self, w: f32, h: f32) -> Polyline {
        let n = self.pts.len();
        let mut pts: Vec<P> = Vec::new();
        let mut ws: Vec<f32> = Vec::new();
        let mut zs: Vec<f32> = Vec::new();
        if n == 0 {
            return Polyline { pts, w: ws, z: zs, closed: self.closed };
        }
        let px = |p: P| [p[0] * w, p[1] * h];
        if n == 1 {
            return Polyline { pts: vec![px(self.pts[0].p)], w: vec![self.pts[0].w], z: vec![self.pts[0].z], closed: false };
        }
        let segs = if self.closed && n > 2 { n } else { n - 1 };
        for i in 0..segs {
            let (a, c1, c2, b, wa, wb) = self.segment(i);
            let (a, c1, c2, b) = (px(a), px(c1), px(c2), px(b));
            let len = dist(a, c1) + dist(c1, c2) + dist(c2, b);
            let steps = ((len / 3.0).ceil() as usize).clamp(2, 256);
            for k in 0..steps {
                let t = k as f32 / steps as f32;
                pts.push(bezier(a, c1, c2, b, t));
                ws.push(wa + (wb - wa) * t);
                let (za, zb) = (self.pts[i].z, self.pts[(i + 1) % n].z);
                zs.push(za + (zb - za) * t);
            }
        }
        if !(self.closed && n > 2) {
            let last = &self.pts[n - 1];
            pts.push(px(last.p));
            ws.push(last.w);
            zs.push(last.z);
        }
        Polyline { pts, w: ws, z: zs, closed: self.closed && n > 2 }
    }
}

fn bezier(a: P, b: P, c: P, d: P, t: f32) -> P {
    let u = 1.0 - t;
    let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    [w0 * a[0] + w1 * b[0] + w2 * c[0] + w3 * d[0], w0 * a[1] + w1 * b[1] + w2 * c[1] + w3 * d[1]]
}

impl Spline {
    /// All paths flattened for a `w` x `h` pixel texture.
    pub fn flatten(&self, w: f32, h: f32) -> Vec<Polyline> {
        self.paths.iter().filter(|p| !p.pts.is_empty()).map(|p| p.flatten(w, h)).collect()
    }

    /// Scale, rotate, mirror, move and optionally reverse every path. Works in texture coordinates around the centre
    /// (0.5, 0.5); `rotate` is in turns. A moved path that leaves the texture wraps around when it is drawn.
    pub fn transformed(&self, scale: P, rotate: f32, offset: P, mirror: (bool, bool), reverse: bool) -> Spline {
        let (sa, ca) = (rotate * std::f32::consts::TAU).sin_cos();
        let mx = if mirror.0 { -1.0 } else { 1.0 };
        let my = if mirror.1 { -1.0 } else { 1.0 };
        let vec = |v: P| -> P {
            let (x, y) = (v[0] * scale[0] * mx, v[1] * scale[1] * my);
            [x * ca - y * sa, x * sa + y * ca]
        };
        let mut out = self.clone();
        for path in &mut out.paths {
            for q in &mut path.pts {
                let v = vec([q.p[0] - 0.5, q.p[1] - 0.5]);
                q.p = [0.5 + v[0] + offset[0], 0.5 + v[1] + offset[1]];
                q.hin = q.hin.map(vec);
                q.hout = q.hout.map(vec);
            }
            // a mirror flips the direction of travel, which the reverse option undoes or adds
            if reverse != (mirror.0 ^ mirror.1) {
                path.pts.reverse();
                for q in &mut path.pts {
                    std::mem::swap(&mut q.hin, &mut q.hout);
                }
            }
        }
        out
    }

    /// Append the paths of another spline.
    pub fn merge(&mut self, other: Spline) {
        self.paths.extend(other.paths);
    }

    /// Outlines of the light areas of a picture (`lum >= threshold`) as closed splines, found with marching squares on
    /// the wrapped pixel grid. The result is simplified to `tolerance` (a fraction of the texture size). A shape that
    /// runs across the seam comes out as an open path that ends one texture further, so it still tiles.
    pub fn from_mask(t: &crate::tex::Tex, threshold: f32, tolerance: f32) -> Result<Spline, String> {
        use std::collections::HashMap;
        let (w, h) = (t.w, t.h);
        if w < 2 || h < 2 {
            return Err("the picture is too small to trace".into());
        }
        let val = |x: i64, y: i64| -> f32 { crate::tex::lum(t.px[(y.rem_euclid(h as i64) as usize) * w + x.rem_euclid(w as i64) as usize]) };
        // an edge of the pixel grid: (kind 0 = from (x,y) to (x+1,y), kind 1 = from (x,y) to (x,y+1), x, y)
        type Edge = (u8, i64, i64);
        let crossing = |e: Edge| -> P {
            let (a, b) = if e.0 == 0 { (val(e.1, e.2), val(e.1 + 1, e.2)) } else { (val(e.1, e.2), val(e.1, e.2 + 1)) };
            let k = if (b - a).abs() < 1e-9 { 0.5 } else { ((threshold - a) / (b - a)).clamp(0.0, 1.0) };
            if e.0 == 0 {
                [e.1 as f32 + k, e.2 as f32]
            } else {
                [e.1 as f32, e.2 as f32 + k]
            }
        };
        let wrap = |e: Edge| -> Edge { (e.0, e.1.rem_euclid(w as i64), e.2.rem_euclid(h as i64)) };
        let mut links: HashMap<Edge, Vec<Edge>> = HashMap::new();
        let mut any = false;
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                let c = [val(x, y), val(x + 1, y), val(x + 1, y + 1), val(x, y + 1)];
                let inside = [c[0] >= threshold, c[1] >= threshold, c[2] >= threshold, c[3] >= threshold];
                let case = inside.iter().enumerate().fold(0, |a, (i, &b)| a | ((b as u32) << i));
                if case == 0 || case == 15 {
                    continue;
                }
                any = true;
                let top: Edge = wrap((0, x, y));
                let bottom: Edge = wrap((0, x, y + 1));
                let left: Edge = wrap((1, x, y));
                let right: Edge = wrap((1, x + 1, y));
                let pairs: Vec<(Edge, Edge)> = match case {
                    1 | 14 => vec![(top, left)],
                    2 | 13 => vec![(top, right)],
                    3 | 12 => vec![(left, right)],
                    4 | 11 => vec![(right, bottom)],
                    6 | 9 => vec![(top, bottom)],
                    7 | 8 => vec![(left, bottom)],
                    5 | 10 => {
                        let centre = (c[0] + c[1] + c[2] + c[3]) * 0.25 >= threshold;
                        if (case == 5) == centre {
                            vec![(top, right), (left, bottom)]
                        } else {
                            vec![(top, left), (right, bottom)]
                        }
                    }
                    _ => vec![],
                };
                for (a, b) in pairs {
                    links.entry(a).or_default().push(b);
                    links.entry(b).or_default().push(a);
                }
            }
        }
        if !any {
            return Err("nothing to trace: the picture is entirely above or below the threshold".into());
        }
        let mut seen: std::collections::HashSet<Edge> = std::collections::HashSet::new();
        let mut loops: Vec<(Vec<P>, bool)> = Vec::new();
        let keys: Vec<Edge> = {
            let mut k: Vec<Edge> = links.keys().copied().collect();
            k.sort();
            k
        };
        for start in keys {
            if seen.contains(&start) {
                continue;
            }
            let mut pts: Vec<P> = Vec::new();
            let mut prev: Option<Edge> = None;
            let mut cur = start;
            let mut shift = [0.0f32, 0.0f32];
            let mut last: Option<P> = None;
            loop {
                seen.insert(cur);
                let mut q = crossing(cur);
                if let Some(l) = last {
                    // keep the path continuous across the seam
                    if q[0] + shift[0] - l[0] > w as f32 * 0.5 {
                        shift[0] -= w as f32;
                    } else if q[0] + shift[0] - l[0] < -(w as f32) * 0.5 {
                        shift[0] += w as f32;
                    }
                    if q[1] + shift[1] - l[1] > h as f32 * 0.5 {
                        shift[1] -= h as f32;
                    } else if q[1] + shift[1] - l[1] < -(h as f32) * 0.5 {
                        shift[1] += h as f32;
                    }
                }
                q = [q[0] + shift[0], q[1] + shift[1]];
                last = Some(q);
                pts.push(q);
                let nb = links.get(&cur).cloned().unwrap_or_default();
                let next = nb.iter().copied().find(|&e| Some(e) != prev && !seen.contains(&e)).or_else(|| nb.iter().copied().find(|&e| e == start && prev.is_some() && prev != Some(e) && pts.len() > 2));
                match next {
                    Some(e) if e != start => {
                        prev = Some(cur);
                        cur = e;
                    }
                    _ => break,
                }
                if pts.len() > w * h * 2 {
                    break;
                }
            }
            if pts.len() < 3 {
                continue;
            }
            // closed when the last point is next to the first one in unwrapped space
            let (f, l) = (pts[0], pts[pts.len() - 1]);
            let gap = [f[0] - l[0], f[1] - l[1]];
            let closed = gap[0].abs() < 2.0 && gap[1].abs() < 2.0;
            if !closed {
                // it wrapped around the texture: end exactly one texture further so it repeats
                let jump = [(gap[0] / w as f32).round() * w as f32, (gap[1] / h as f32).round() * h as f32];
                pts.push([f[0] - jump[0], f[1] - jump[1]]);
            }
            loops.push((pts, closed));
        }
        let mut sp = Spline::default();
        let tol = tolerance.clamp(0.0001, 0.1);
        loops.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        for (pts, closed) in loops.into_iter().take(256) {
            let norm: Vec<P> = pts.iter().map(|q| [(q[0] + 0.5) / w as f32, (q[1] + 0.5) / h as f32]).collect();
            let simp = simplify(&norm, tol, closed);
            if simp.len() >= 3 || (!closed && simp.len() >= 2) {
                sp.paths.push(SPath { closed, smooth: false, pts: simp.into_iter().map(|p| SPoint::new(p[0], p[1])).collect() });
            }
        }
        if sp.paths.is_empty() {
            return Err("no outlines found at this threshold".into());
        }
        Ok(sp)
    }

    pub fn point_count(&self) -> usize {
        self.paths.iter().map(|p| p.pts.len()).sum()
    }

    // ---------------------------------------------------------------- text format
    pub fn to_text(&self) -> String {
        let mut s = String::from("spline 1\n");
        for p in &self.paths {
            let _ = writeln!(s, "path closed={} smooth={}", p.closed as u8, p.smooth as u8);
            for q in &p.pts {
                let _ = write!(s, "pt {} {}", num(q.p[0]), num(q.p[1]));
                if let Some(h) = q.hin {
                    let _ = write!(s, " hin={},{}", num(h[0]), num(h[1]));
                }
                if let Some(h) = q.hout {
                    let _ = write!(s, " hout={},{}", num(h[0]), num(h[1]));
                }
                if (q.w - 1.0).abs() > 1e-6 {
                    let _ = write!(s, " w={}", num(q.w));
                }
                if (q.z - 1.0).abs() > 1e-6 {
                    let _ = write!(s, " z={}", num(q.z));
                }
                if q.tan != 0 {
                    let _ = write!(s, " tan={}", q.tan);
                }
                s.push('\n');
            }
        }
        s
    }

    pub fn parse(text: &str) -> Result<Spline, String> {
        let mut sp = Spline::default();
        let mut seen_header = false;
        for (i, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let ln = i + 1;
            let mut t = line.split_whitespace();
            match t.next() {
                Some("spline") => seen_header = true,
                Some("path") => {
                    let mut p = SPath::default();
                    for kv in t {
                        match kv.split_once('=') {
                            Some(("closed", v)) => p.closed = v != "0",
                            Some(("smooth", v)) => p.smooth = v != "0",
                            _ => return Err(format!("line {ln}: unknown path option '{kv}'")),
                        }
                    }
                    sp.paths.push(p);
                }
                Some("pt") => {
                    let path = sp.paths.last_mut().ok_or(format!("line {ln}: 'pt' before any 'path'"))?;
                    let x: f32 = t.next().and_then(|v| v.parse().ok()).ok_or(format!("line {ln}: pt needs x y"))?;
                    let y: f32 = t.next().and_then(|v| v.parse().ok()).ok_or(format!("line {ln}: pt needs x y"))?;
                    if !x.is_finite() || !y.is_finite() {
                        return Err(format!("line {ln}: coordinates must be finite"));
                    }
                    let mut q = SPoint::new(x, y);
                    for kv in t {
                        let (k, v) = kv.split_once('=').ok_or(format!("line {ln}: bad option '{kv}'"))?;
                        let pair = |v: &str| -> Result<P, String> {
                            let (a, b) = v.split_once(',').ok_or(format!("line {ln}: '{k}' needs dx,dy"))?;
                            let (a, b): (f32, f32) = (a.parse().map_err(|_| format!("line {ln}: bad number"))?, b.parse().map_err(|_| format!("line {ln}: bad number"))?);
                            if a.is_finite() && b.is_finite() {
                                Ok([a, b])
                            } else {
                                Err(format!("line {ln}: handles must be finite"))
                            }
                        };
                        match k {
                            "hin" => q.hin = Some(pair(v)?),
                            "hout" => q.hout = Some(pair(v)?),
                            "w" => q.w = v.parse::<f32>().ok().filter(|w| w.is_finite()).ok_or(format!("line {ln}: bad width"))?.clamp(0.0, 16.0),
                            "z" => q.z = v.parse::<f32>().ok().filter(|z| z.is_finite()).ok_or(format!("line {ln}: bad height"))?.clamp(0.0, 1.0),
                            "tan" => q.tan = match v {
                                "0" | "auto" => 0,
                                "1" | "mirrored" => 1,
                                "2" | "free" => 2,
                                "3" | "corner" => 3,
                                _ => return Err(format!("line {ln}: tan must be 0..3")),
                            },
                            _ => return Err(format!("line {ln}: unknown point option '{k}'")),
                        }
                    }
                    path.pts.push(q);
                }
                Some(other) => return Err(format!("line {ln}: unknown keyword '{other}'")),
                None => {}
            }
        }
        if !seen_header && sp.paths.is_empty() {
            return Err("not a spline file (expected 'spline 1')".into());
        }
        Ok(sp)
    }

    pub fn from_bytes(b: &[u8]) -> Result<Spline, String> {
        Spline::parse(std::str::from_utf8(b).map_err(|_| "spline file is not text".to_string())?)
    }

    // ---------------------------------------------------------------- built-in shapes
    /// Ready-made splines: 1 horizontal line, 2 diagonal line, 3 wave, 4 circle, 5 spiral, 6 zigzag, 7 figure eight.
    /// `amp` and `freq` shape the wave, zigzag and spiral (amplitude in texture units, cycles across the texture).
    pub fn preset(kind: u32, amp: f32, freq: f32) -> Spline {
        let amp = amp.clamp(0.0, 0.5);
        let freq = freq.clamp(0.25, 32.0);
        let two_pi = std::f32::consts::TAU;
        let path = match kind {
            1 => SPath { pts: vec![SPoint::new(0.0, 0.5), SPoint::new(1.0, 0.5)], smooth: false, ..Default::default() },
            2 => SPath { pts: vec![SPoint::new(0.0, 0.0), SPoint::new(1.0, 1.0)], smooth: false, ..Default::default() },
            3 => {
                let n = (freq.ceil() as usize * 8).clamp(8, 256);
                let pts = (0..=n).map(|i| {
                    let x = i as f32 / n as f32;
                    SPoint::new(x, 0.5 + amp * (x * freq.round().max(1.0) * two_pi).sin())
                });
                SPath { pts: pts.collect(), ..Default::default() }
            }
            4 => {
                let pts = (0..8).map(|i| {
                    let a = i as f32 / 8.0 * two_pi;
                    SPoint::new(0.5 + 0.3 * a.cos(), 0.5 + 0.3 * a.sin())
                });
                SPath { closed: true, pts: pts.collect(), ..Default::default() }
            }
            5 => {
                let turns = freq.clamp(1.0, 6.0);
                let n = (turns * 16.0) as usize;
                let pts = (0..=n).map(|i| {
                    let t = i as f32 / n as f32;
                    let (r, a) = (0.04 + 0.34 * t, t * turns * two_pi);
                    SPoint::new(0.5 + r * a.cos(), 0.5 + r * a.sin())
                });
                SPath { pts: pts.collect(), ..Default::default() }
            }
            6 => {
                let n = (freq.round().max(1.0) as usize) * 2;
                let pts = (0..=n).map(|i| SPoint::new(i as f32 / n as f32, 0.5 + if i % 2 == 0 { -amp } else { amp }));
                SPath { pts: pts.collect(), smooth: false, ..Default::default() }
            }
            _ => {
                let pts = (0..16).map(|i| {
                    let a = i as f32 / 16.0 * two_pi;
                    SPoint::new(0.5 + 0.34 * a.sin(), 0.5 + 0.2 * (2.0 * a).sin())
                });
                SPath { closed: true, pts: pts.collect(), ..Default::default() }
            }
        };
        Spline { paths: vec![path] }
    }

    /// Paths of an SVG file as a spline (curves are kept as short straight segments; open or closed as in the file).
    pub fn from_svg(src: &str) -> Result<Spline, String> {
        let polys = crate::vector::svg_outlines(src)?;
        let mut sp = Spline::default();
        for poly in polys {
            let pts = simplify(&poly.pts, 0.0015, poly.closed);
            if pts.len() >= 2 {
                sp.paths.push(SPath { closed: poly.closed, smooth: false, pts: pts.into_iter().map(|p| SPoint::new(p[0], p[1])).collect() });
            }
        }
        if sp.paths.is_empty() {
            return Err("the SVG has no paths or shapes".into());
        }
        Ok(sp)
    }
}

/// How a path is drawn when a spline becomes a picture: filled (closed paths) and/or stroked with a width in
/// texture units (0 = no stroke).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeStyle {
    pub fill: bool,
    pub stroke: f32,
}

impl Default for ShapeStyle {
    fn default() -> Self {
        ShapeStyle { fill: true, stroke: 0.0 }
    }
}

impl SPath {
    /// The path as SVG path data in texture coordinates (cubic curves).
    pub fn svg_d(&self) -> String {
        let n = self.pts.len();
        if n == 0 {
            return String::new();
        }
        let mut d = format!("M{} {}", num(self.pts[0].p[0]), num(self.pts[0].p[1]));
        if n == 1 {
            return d;
        }
        for i in 0..self.segment_count() {
            let (_, c1, c2, b, _, _) = self.segment(i);
            let _ = write!(d, " C{} {} {} {} {} {}", num(c1[0]), num(c1[1]), num(c2[0]), num(c2[1]), num(b[0]), num(b[1]));
        }
        if self.closed && n > 2 {
            d.push_str(" Z");
        }
        d
    }
}

impl Spline {
    /// The spline as an SVG picture (white shapes on a transparent ground, the unit square as view box) that the SVG
    /// node renders. The editable spline travels inside the file as metadata, so it can be opened again exactly.
    pub fn to_svg(&self, styles: &[ShapeStyle]) -> String {
        let mut s = String::from("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\" width=\"1024\" height=\"1024\">\n");
        s.push_str("<metadata id=\"materialcraft-spline\">");
        s.push_str(self.to_text().trim_end());
        for (i, st) in styles.iter().enumerate().take(self.paths.len()) {
            let _ = write!(s, "\nstyle {} {} {}", i, st.fill as u8, num(st.stroke));
        }
        s.push_str("</metadata>\n");
        for (i, p) in self.paths.iter().enumerate() {
            if p.pts.len() < 2 {
                continue;
            }
            let st = styles.get(i).copied().unwrap_or_default();
            let fill = st.fill && p.closed && p.pts.len() > 2;
            let stroke = st.stroke > 1e-5 || !fill;
            let _ = write!(s, "<path d=\"{}\" fill=\"{}\"", p.svg_d(), if fill { "#fff" } else { "none" });
            if stroke {
                let _ = write!(s, " stroke=\"#fff\" stroke-width=\"{}\" stroke-linejoin=\"round\" stroke-linecap=\"round\"", num(if st.stroke > 1e-5 { st.stroke } else { 0.01 }));
            }
            s.push_str("/>\n");
        }
        s.push_str("</svg>\n");
        s
    }

    /// The spline an SVG made by [`Spline::to_svg`] carries (with its styles); `None` for any other SVG.
    pub fn from_svg_meta(svg: &str) -> Option<(Spline, Vec<ShapeStyle>)> {
        let a = svg.find("<metadata id=\"materialcraft-spline\">")? + "<metadata id=\"materialcraft-spline\">".len();
        let b = svg[a..].find("</metadata>")? + a;
        let body = &svg[a..b];
        let mut text = String::new();
        let mut styles: Vec<ShapeStyle> = Vec::new();
        for line in body.lines() {
            if let Some(r) = line.trim().strip_prefix("style ") {
                let f: Vec<&str> = r.split_whitespace().collect();
                if f.len() == 3 {
                    let i: usize = f[0].parse().ok()?;
                    while styles.len() <= i {
                        styles.push(ShapeStyle::default());
                    }
                    styles[i] = ShapeStyle { fill: f[1] == "1", stroke: f[2].parse().ok()? };
                }
            } else {
                text.push_str(line);
                text.push('\n');
            }
        }
        let sp = Spline::parse(&text).ok()?;
        while styles.len() < sp.paths.len() {
            styles.push(ShapeStyle::default());
        }
        Some((sp, styles))
    }
}

fn num(v: f32) -> String {
    let t = format!("{v:.5}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t.is_empty() || t == "-0" {
        "0".into()
    } else {
        t.into()
    }
}

/// Douglas-Peucker simplification (keeps the ends; a closed path keeps its start).
fn simplify(pts: &[P], tol: f32, closed: bool) -> Vec<P> {
    let mut v: Vec<P> = pts.to_vec();
    if closed && v.len() > 1 && dist(v[0], v[v.len() - 1]) < 1e-5 {
        v.pop();
    }
    if v.len() <= 2 {
        return v;
    }
    let mut keep = vec![false; v.len()];
    keep[0] = true;
    keep[v.len() - 1] = true;
    let mut stack = vec![(0usize, v.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (pa, pb) = (v[a], v[b]);
        let (dx, dy) = (pb[0] - pa[0], pb[1] - pa[1]);
        let l2 = (dx * dx + dy * dy).max(1e-12);
        let mut best = (0.0f32, a);
        for i in a + 1..b {
            let t = (((v[i][0] - pa[0]) * dx + (v[i][1] - pa[1]) * dy) / l2).clamp(0.0, 1.0);
            let d = dist(v[i], [pa[0] + dx * t, pa[1] + dy * t]);
            if d > best.0 {
                best = (d, i);
            }
        }
        if best.0 > tol {
            keep[best.1] = true;
            stack.push((a, best.1));
            stack.push((best.1, b));
        }
    }
    v.into_iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| p).collect()
}
