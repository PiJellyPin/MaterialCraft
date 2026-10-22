// SPDX-License-Identifier: MIT OR Apache-2.0
//! Library graphs implemented natively (see `nodes10`): spline tools, path tools, shape splatter and scratches.
//!
//! Spline, path and point-list data travel between these nodes as pictures in MaterialCraft's own layout (kept in
//! 32-bit float): pixel 0 holds `[2077.5, spline count, point count, kind]`, the next pixels one header per spline
//! `[first point, point count, closed, 0]`, then two pixels per point: `[x, y, in-handle x, in-handle y]` and
//! `[out-handle x, out-handle y, thickness, height]` (texture coordinates, y down; handles relative to the point).
//! Shape-splatter data: `splatter_data_1 = [local u, local v, shape index, pattern number]` and
//! `splatter_data_2 = [centre x, centre y, angle (turns), size]` for the shape on top at each pixel (alpha 0: none).
use crate::nodes10::{g, grey, lerp, noise, rnd, smooth, Cx};
use crate::tex::{lum, Tex};
use std::f32::consts::{PI, TAU};

const MAGIC: f32 = 2077.5;

#[derive(Clone, Copy, Debug, Default)]
pub struct CP {
    pub p: [f32; 2],
    pub hin: [f32; 2],
    pub hout: [f32; 2],
    pub th: f32,
    pub ht: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Sp {
    pub pts: Vec<CP>,
    pub closed: bool,
}

/// Splines (or paths, or a point list) as one picture.
pub fn encode(sps: &[Sp], kind: f32, w: usize, h: usize) -> Tex {
    let mut t = Tex::solid(w, h, [0.0; 4]);
    let cap = w * h;
    let n_pts: usize = sps.iter().map(|s| s.pts.len()).sum();
    let base = 1 + sps.len();
    if cap < base + 2 {
        return t;
    }
    let max_pts = (cap - base) / 2;
    let mut written = 0usize;
    let mut heads = Vec::new();
    for sp in sps {
        let n = sp.pts.len().min(max_pts - written);
        heads.push([written as f32, n as f32, sp.closed as i32 as f32, 0.0]);
        for (j, q) in sp.pts.iter().take(n).enumerate() {
            let i = base + 2 * (written + j);
            t.px[i] = [q.p[0], q.p[1], q.hin[0], q.hin[1]];
            t.px[i + 1] = [q.hout[0], q.hout[1], q.th, q.ht];
        }
        written += n;
    }
    t.px[0] = [MAGIC, sps.len() as f32, n_pts.min(written) as f32, kind];
    for (k, hd) in heads.into_iter().enumerate() {
        t.px[1 + k] = hd;
    }
    t
}

pub fn decode(t: &Tex) -> Option<Vec<Sp>> {
    let h = t.px.first()?;
    if (h[0] - MAGIC).abs() > 1e-3 {
        return None;
    }
    let ns = h[1].max(0.0) as usize;
    let base = 1 + ns;
    let mut out = Vec::with_capacity(ns);
    for k in 0..ns {
        let hd = t.px.get(1 + k)?;
        let (first, n) = (hd[0].max(0.0) as usize, hd[1].max(0.0) as usize);
        let mut pts = Vec::with_capacity(n);
        for j in 0..n {
            let i = base + 2 * (first + j);
            let (a, b) = (t.px.get(i)?, t.px.get(i + 1)?);
            pts.push(CP { p: [a[0], a[1]], hin: [a[2], a[3]], hout: [b[0], b[1]], th: b[2], ht: b[3] });
        }
        out.push(Sp { pts, closed: hd[2] > 0.5 });
    }
    Some(out)
}

/// Splines arriving at one of a node's coordinate / data input pairs (either may carry them).
fn input_splines(cx: &Cx, coords: &str, data: &str) -> Vec<Sp> {
    cx.img(coords).and_then(decode).or_else(|| cx.img(data).and_then(decode)).unwrap_or_default()
}

fn bez(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    [w0 * a[0] + w1 * b[0] + w2 * c[0] + w3 * d[0], w0 * a[1] + w1 * b[1] + w2 * c[1] + w3 * d[1]]
}

/// A flattened spline: points with thickness, height and the curve parameter (0..1 along the spline).
pub struct Poly {
    pub p: Vec<[f32; 2]>,
    pub th: Vec<f32>,
    pub ht: Vec<f32>,
    pub t: Vec<f32>,
    pub closed: bool,
}

pub fn flatten(sp: &Sp, per_seg: usize) -> Poly {
    let n = sp.pts.len();
    let mut out = Poly { p: Vec::new(), th: Vec::new(), ht: Vec::new(), t: Vec::new(), closed: sp.closed && n > 2 };
    if n == 0 {
        return out;
    }
    if n == 1 {
        out.p.push(sp.pts[0].p);
        out.th.push(sp.pts[0].th);
        out.ht.push(sp.pts[0].ht);
        out.t.push(0.0);
        return out;
    }
    let segs = if out.closed { n } else { n - 1 };
    let per = per_seg.clamp(1, 256);
    for i in 0..segs {
        let (a, b) = (sp.pts[i], sp.pts[(i + 1) % n]);
        let (c1, c2) = ([a.p[0] + a.hout[0], a.p[1] + a.hout[1]], [b.p[0] + b.hin[0], b.p[1] + b.hin[1]]);
        for k in 0..per {
            let t = k as f32 / per as f32;
            out.p.push(bez(a.p, c1, c2, b.p, t));
            out.th.push(lerp(a.th, b.th, t));
            out.ht.push(lerp(a.ht, b.ht, t));
        }
    }
    if !out.closed {
        let l = sp.pts[n - 1];
        out.p.push(l.p);
        out.th.push(l.th);
        out.ht.push(l.ht);
    }
    // parameter by length
    let mut acc = vec![0.0f32; out.p.len()];
    for i in 1..out.p.len() {
        let (a, b) = (out.p[i - 1], out.p[i]);
        acc[i] = acc[i - 1] + ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    }
    // a closed spline's parameter also runs along the closing segment back to the start
    let close = if out.closed && out.p.len() > 1 {
        let (a, b) = (out.p[out.p.len() - 1], out.p[0]);
        ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
    } else {
        0.0
    };
    let total = (acc.last().copied().unwrap_or(0.0) + close).max(1e-9);
    out.t = acc.iter().map(|a| a / total).collect();
    out
}

/// Length of a flattened spline.
fn length(p: &Poly) -> f32 {
    p.p.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum::<f32>() + if p.closed && p.p.len() > 2 { ((p.p[0][0] - p.p[p.p.len() - 1][0]).powi(2) + (p.p[0][1] - p.p[p.p.len() - 1][1]).powi(2)).sqrt() } else { 0.0 }
}

/// Point, tangent, thickness and height at parameter `t` (0..1 by length).
fn at_param(p: &Poly, t: f32) -> ([f32; 2], [f32; 2], f32, f32) {
    let n = p.p.len();
    if n == 0 {
        return ([0.5, 0.5], [1.0, 0.0], 0.0, 1.0);
    }
    if n == 1 {
        return (p.p[0], [1.0, 0.0], p.th[0], p.ht[0]);
    }
    let t = t.clamp(0.0, 1.0);
    let last = p.t[n - 1];
    if p.closed && t > last {
        // on the closing segment from the last point back to the first
        let k = if 1.0 - last > 1e-9 { (t - last) / (1.0 - last) } else { 0.0 };
        let (pa, pb) = (p.p[n - 1], p.p[0]);
        let d = [pb[0] - pa[0], pb[1] - pa[1]];
        let l = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-9);
        return ([lerp(pa[0], pb[0], k), lerp(pa[1], pb[1], k)], [d[0] / l, d[1] / l], lerp(p.th[n - 1], p.th[0], k), lerp(p.ht[n - 1], p.ht[0], k));
    }
    let i = p.t.partition_point(|x| *x < t).clamp(1, n - 1);
    let (a, b) = (p.t[i - 1], p.t[i]);
    let k = if b - a > 1e-9 { (t - a) / (b - a) } else { 0.0 };
    let (pa, pb) = (p.p[i - 1], p.p[i]);
    let d = [pb[0] - pa[0], pb[1] - pa[1]];
    let l = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-9);
    ([lerp(pa[0], pb[0], k), lerp(pa[1], pb[1], k)], [d[0] / l, d[1] / l], lerp(p.th[i - 1], p.th[i], k), lerp(p.ht[i - 1], p.ht[i], k))
}

/// Smooth handles through the points (Catmull-Rom), scaled by `smooth`.
fn auto_handles(pts: &mut [CP], closed: bool, smooth_k: f32) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let pos: Vec<[f32; 2]> = pts.iter().map(|q| q.p).collect();
    for i in 0..n {
        let prev = if i == 0 { if closed { pos[n - 1] } else { pos[0] } } else { pos[i - 1] };
        let next = if i + 1 == n { if closed { pos[0] } else { pos[n - 1] } } else { pos[i + 1] };
        let d = [(next[0] - prev[0]) / 6.0 * smooth_k, (next[1] - prev[1]) / 6.0 * smooth_k];
        pts[i].hin = [-d[0], -d[1]];
        pts[i].hout = d;
    }
}

/// One segment of a flattened spline for drawing.
#[derive(Clone, Copy)]
struct Seg {
    a: [f32; 2],
    b: [f32; 2],
    ta: f32,
    tb: f32,
    tha: f32,
    thb: f32,
    ha: f32,
    hb: f32,
    id: usize,
}

/// Segments binned on a grid so each pixel only looks at nearby ones.
struct Bins {
    segs: Vec<Seg>,
    grid: Vec<Vec<usize>>,
    n: usize,
}

impl Bins {
    fn new(polys: &[Poly], pad: f32) -> Bins {
        let mut segs = Vec::new();
        for (id, p) in polys.iter().enumerate() {
            let m = p.p.len();
            if m == 1 {
                segs.push(Seg { a: p.p[0], b: p.p[0], ta: 0.0, tb: 0.0, tha: p.th[0], thb: p.th[0], ha: p.ht[0], hb: p.ht[0], id });
            }
            let cnt = if p.closed { m } else { m.saturating_sub(1) };
            for i in 0..cnt {
                let j = (i + 1) % m;
                let tb = if j == 0 { 1.0 } else { p.t[j] };
                segs.push(Seg { a: p.p[i], b: p.p[j], ta: p.t[i], tb, tha: p.th[i], thb: p.th[j], ha: p.ht[i], hb: p.ht[j], id });
            }
        }
        let n = 48usize;
        let mut grid = vec![Vec::new(); n * n];
        for (k, s) in segs.iter().enumerate() {
            let r = pad + s.tha.max(s.thb);
            let (x0, x1) = (s.a[0].min(s.b[0]) - r, s.a[0].max(s.b[0]) + r);
            let (y0, y1) = (s.a[1].min(s.b[1]) - r, s.a[1].max(s.b[1]) + r);
            let c = |v: f32| ((v * n as f32).floor() as i64).clamp(0, n as i64 - 1) as usize;
            if x1 < 0.0 || y1 < 0.0 || x0 > 1.0 || y0 > 1.0 {
                continue;
            }
            for gy in c(y0)..=c(y1) {
                for gx in c(x0)..=c(x1) {
                    grid[gy * n + gx].push(k);
                }
            }
        }
        Bins { segs, grid, n }
    }
    /// Nearest segment to (u, v): (distance, parameter t, thickness, height, spline id, side).
    fn nearest(&self, u: f32, v: f32) -> Option<(f32, f32, f32, f32, usize, f32)> {
        let (gx, gy) = (((u * self.n as f32) as i64).clamp(0, self.n as i64 - 1) as usize, ((v * self.n as f32) as i64).clamp(0, self.n as i64 - 1) as usize);
        let mut best: Option<(f32, f32, f32, f32, usize, f32)> = None;
        for &k in &self.grid[gy * self.n + gx] {
            let s = &self.segs[k];
            let d = [s.b[0] - s.a[0], s.b[1] - s.a[1]];
            let l2 = (d[0] * d[0] + d[1] * d[1]).max(1e-12);
            let q = (((u - s.a[0]) * d[0] + (v - s.a[1]) * d[1]) / l2).clamp(0.0, 1.0);
            let (px, py) = (s.a[0] + d[0] * q, s.a[1] + d[1] * q);
            let dist = ((u - px).powi(2) + (v - py).powi(2)).sqrt();
            let side = (d[0] * (v - s.a[1]) - d[1] * (u - s.a[0])).signum();
            // the thicker spline wins where two overlap
            let th = lerp(s.tha, s.thb, q);
            let score = dist - th;
            if best.map(|b| score < b.0 - b.2).unwrap_or(true) {
                best = Some((dist, lerp(s.ta, s.tb, q), th, lerp(s.ha, s.hb, q), s.id, side));
            }
        }
        best
    }
}

/// A picture of the splines: lines `px` pixels wide (plus their thickness envelope when asked).
fn preview(sps: &[Sp], bg: Option<&Tex>, s: usize, hs: usize, px: f32, envelope: bool, bg_k: f32) -> Tex {
    let polys: Vec<Poly> = sps.iter().map(|p| flatten(p, 24)).collect();
    let w = px.max(0.5) / s as f32;
    let bins = Bins::new(&polys, w * 2.0);
    Tex::from_fn(s, hs, |u, v| {
        let b = bg.map(|t| g(t, u, v) * bg_k).unwrap_or(0.0);
        let Some((d, _, th, _, _, _)) = bins.nearest(u, v) else { return grey(b) };
        let line = 1.0 - smooth(w * 0.5, w * 0.5 + 1.0 / s as f32, d);
        let env = if envelope && th > 0.0 { (1.0 - smooth(th - 1.0 / s as f32, th, d)) * 0.35 } else { 0.0 };
        grey(b.max(line).max(env))
    })
}

/// The four outputs of a spline-making node.
fn spline_out(cx: &Cx, sps: &[Sp]) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let thick = if cx.lib.input("thickness_px").is_some() { cx.f("thickness_px") } else { 2.0 };
    let env = cx.lib.input("show_thickness_envelope").is_some() && cx.b("show_thickness_envelope");
    let bg_k = if cx.lib.input("background_intensity").is_some() { cx.f("background_intensity") } else if cx.lib.input("preview_background_intensity").is_some() { cx.f("preview_background_intensity") } else { 0.5 };
    let bg = cx.img("preview").or_else(|| cx.img("preview_1"));
    let data = encode(sps, 0.0, s, hs);
    let all = vec![
        if cx.wants(0) { Some(preview(sps, bg, s, hs, thick * cx.rs() * 2.0, env, bg_k)) } else { None },
        if cx.wants(1) { Some(data.clone()) } else { None },
        if cx.wants(2) { Some(data) } else { None },
        if cx.wants(3) { Some(Tex::solid(s, hs, grey(sps.len() as f32 / 255.0))) } else { None },
    ];
    cx.pick(all)
}

/// Splines from the node's own spline input when "append input spline" is on.
fn appended(cx: &Cx, mut own: Vec<Sp>) -> Vec<Sp> {
    if cx.lib.input("append_input_spline").is_some() && cx.b("append_input_spline") {
        let mut all = input_splines(cx, "spline_coords", "spline_data");
        all.append(&mut own);
        all
    } else {
        own
    }
}

fn flip(sp: &mut Sp) {
    sp.pts.reverse();
    for q in sp.pts.iter_mut() {
        std::mem::swap(&mut q.hin, &mut q.hout);
    }
}

/// Correct x for a non-square picture so circles stay round.
fn aspect_fix(cx: &Cx) -> f32 {
    if cx.lib.input("non_square_correction").is_some() && cx.b("non_square_correction") {
        cx.hs as f32 / cx.s as f32
    } else {
        1.0
    }
}

pub fn compute(cx: &Cx) -> Option<Tex> {
    let (s, hs) = (cx.s, cx.hs);
    let rs = cx.rs();
    let seed = cx.seed;
    Some(match cx.kind {
        // ---------------------------------------------------------------- making splines
        "x_spline_circle" => {
            let (r, pre, size, post, pos) = (cx.f("circle_radius"), cx.f("circle_pre_rotation"), cx.v2("circle_size"), cx.f("circle_post_rotation"), cx.v2("circle_position"));
            let (sh, eh, st, et) = (cx.f("start_height"), cx.f("end_height"), cx.f("start_thickness"), cx.f("end_thickness"));
            let trim = cx.v2("trim");
            let (spiral, turns, power) = (cx.f("spiral"), cx.f("spiral_turns").max(0.01), cx.f("spiral_power").max(0.01));
            let ax = aspect_fix(cx);
            let total = if spiral > 0.0 { turns } else { 1.0 };
            let n = ((total * 16.0).ceil() as usize).clamp(4, 512);
            let (t0, t1) = (trim[0].min(trim[1]), trim[0].max(trim[1]));
            let full = t0 <= 0.0 && t1 >= 1.0 && spiral <= 0.0;
            let mut pts = Vec::new();
            let count = if full { n } else { n + 1 };
            for k in 0..count {
                let f = k as f32 / n as f32;
                let tt = lerp(t0, t1, f);
                let a = (tt * total + pre) * TAU;
                let rr = r * if spiral > 0.0 { 1.0 - spiral * (1.0 - tt.powf(power)) } else { 1.0 };
                let (x, y) = (a.cos() * rr * size[0], a.sin() * rr * size[1]);
                let (sp_, cp) = (post * TAU).sin_cos();
                let (x, y) = (x * cp - y * sp_, x * sp_ + y * cp);
                pts.push(CP { p: [pos[0] + x * ax, pos[1] + y], th: lerp(st, et, tt), ht: lerp(sh, eh, tt), ..Default::default() });
            }
            auto_handles(&mut pts, full, 1.0);
            let mut sp = Sp { pts, closed: full };
            if cx.b("flip_direction") {
                flip(&mut sp);
            }
            spline_out(cx, &appended(cx, vec![sp]))
        }
        "x_spline_cubic" | "x_spline_quadratic" => {
            let ax = aspect_fix(cx);
            let fixp = |p: [f32; 2]| [0.5 + (p[0] - 0.5) * ax, p[1]];
            let (sh, eh, st, et) = (cx.f("start_height"), cx.f("end_height"), cx.f("start_thickness"), cx.f("end_thickness"));
            let mut sp = if cx.kind == "x_spline_cubic" {
                let (p1, t1, p2, t2) = (fixp(cx.v2("p1")), fixp(cx.v2("p1_tangent")), fixp(cx.v2("p2")), fixp(cx.v2("p2_tangent")));
                Sp {
                    pts: vec![
                        CP { p: p1, hout: [t1[0] - p1[0], t1[1] - p1[1]], th: st, ht: sh, ..Default::default() },
                        CP { p: p2, hin: [t2[0] - p2[0], t2[1] - p2[1]], th: et, ht: eh, ..Default::default() },
                    ],
                    closed: false,
                }
            } else {
                let (p1, p2, p3) = (fixp(cx.v2("p1")), fixp(cx.v2("p2")), fixp(cx.v2("p3")));
                let k = cx.f("smoothness").clamp(0.0, 1.0) * 2.0 / 3.0;
                Sp {
                    pts: vec![
                        CP { p: p1, hout: [(p2[0] - p1[0]) * k, (p2[1] - p1[1]) * k], th: st, ht: sh, ..Default::default() },
                        CP { p: p3, hin: [(p2[0] - p3[0]) * k, (p2[1] - p3[1]) * k], th: et, ht: eh, ..Default::default() },
                    ],
                    closed: false,
                }
            };
            if cx.b("flip_direction") {
                flip(&mut sp);
            }
            spline_out(cx, &appended(cx, vec![sp]))
        }
        "x_spline_poly_quadratic" | "x_point_list" => {
            let ax = aspect_fix(cx);
            let from_list = cx.lib.input("use_input_point_list").is_some() && cx.b("use_input_point_list");
            let n = if cx.kind == "x_point_list" { cx.i("point_number").clamp(1, 10) } else { cx.i("point_amount").clamp(3, 10) } as usize;
            let gs = cx.f("global_smoothness_adjustment");
            let mut pts: Vec<CP> = Vec::new();
            let listed = if from_list || cx.kind == "x_point_list" { cx.img("point_list_input").and_then(decode).and_then(|v| v.into_iter().next()) } else { None };
            match listed {
                Some(sp) if from_list || cx.kind == "x_point_list" && cx.i("point_number_input") > 0 => pts = sp.pts,
                _ => {
                    for k in 1..=n {
                        let p = cx.v2(&format!("p{k}"));
                        let pr = cx.v3(&format!("p{k}_properties"));
                        // properties: smoothness, height, thickness
                        pts.push(CP { p: [0.5 + (p[0] - 0.5) * ax, p[1]], th: pr[2] * 0.05, ht: pr[1], hin: [(pr[0] + gs).clamp(0.0, 2.0), 0.0], ..Default::default() });
                    }
                }
            }
            if cx.kind == "x_point_list" {
                let list = Sp { pts: pts.clone(), closed: false };
                let thick = cx.f("points_size") * s as f32;
                let dots = Tex::from_fn(s, hs, |u, v| {
                    let b = cx.img("preview").map(|t| g(t, u, v) * 0.5).unwrap_or(0.0);
                    let mut m = 0.0f32;
                    if cx.b("show_points") {
                        for (i, q) in pts.iter().enumerate() {
                            if (i == 0 && cx.b("hide_first_point")) || (i + 1 == pts.len() && cx.b("hide_last_point")) {
                                continue;
                            }
                            let d = ((u - q.p[0]).powi(2) + (v - q.p[1]).powi(2)).sqrt() * s as f32;
                            m = m.max(1.0 - smooth(thick.max(1.5), thick.max(1.5) + 1.0, d));
                        }
                    }
                    grey(b.max(m))
                });
                let data = encode(&[list], 2.0, s, hs);
                return Some(cx.pick(vec![Some(dots), Some(data), Some(Tex::solid(s, hs, grey(n as f32 / 255.0)))]));
            }
            // a quadratic B-spline through the control points: each segment runs between midpoints
            let closed = cx.b("close_spline");
            let m = pts.len();
            let mut out = Vec::new();
            let mid = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
            let segs = if closed { m } else { m.saturating_sub(2) };
            for i in 0..segs {
                let (a, b, c) = (pts[i % m], pts[(i + 1) % m], pts[(i + 2) % m]);
                let start = if !closed && i == 0 { a.p } else { mid(a.p, b.p) };
                let end = if !closed && i + 1 == segs { c.p } else { mid(b.p, c.p) };
                let k = b.hin[0].clamp(0.0, 2.0) * 2.0 / 3.0;
                if out.is_empty() {
                    out.push(CP { p: start, th: lerp(a.th, b.th, 0.5), ht: lerp(a.ht, b.ht, 0.5), ..Default::default() });
                }
                let last = out.len() - 1;
                out[last].hout = [(b.p[0] - start[0]) * k, (b.p[1] - start[1]) * k];
                out.push(CP { p: end, hin: [(b.p[0] - end[0]) * k, (b.p[1] - end[1]) * k], th: lerp(b.th, c.th, 0.5), ht: lerp(b.ht, c.ht, 0.5), ..Default::default() });
            }
            if closed && out.len() > 1 {
                // the last point repeats the first
                let last = out.pop().map(|q| q.hin).unwrap_or([0.0, 0.0]);
                out[0].hin = last;
            }
            let mut sp = Sp { pts: out, closed };
            if cx.b("flip_direction") {
                flip(&mut sp);
            }
            spline_out(cx, &appended(cx, vec![sp]))
        }
        "x_spline_2dtransform" => {
            let m = cx.v4("matrix22");
            let off = cx.v2("offset");
            let mut sps = input_splines(cx, "spline_coords", "spline_data");
            for sp in sps.iter_mut() {
                for q in sp.pts.iter_mut() {
                    let (x, y) = (q.p[0] - 0.5, q.p[1] - 0.5);
                    q.p = [m[0] * x + m[1] * y + 0.5 + off[0], m[2] * x + m[3] * y + 0.5 + off[1]];
                    for hnd in [&mut q.hin, &mut q.hout] {
                        let (a, b) = (hnd[0], hnd[1]);
                        *hnd = [m[0] * a + m[1] * b, m[2] * a + m[3] * b];
                    }
                }
                if cx.b("flip_direction") {
                    flip(sp);
                }
            }
            spline_out(cx, &sps)
        }
        "x_spline_append" => {
            let mut a = input_splines(cx, "spline_coords_1", "spline_data_1");
            let mut b = input_splines(cx, "spline_coords_2", "spline_data_2");
            if cx.b("flip_spline_1_direction") {
                a.iter_mut().for_each(flip);
            }
            if cx.b("flip_spline_2_direction") {
                b.iter_mut().for_each(flip);
            }
            a.append(&mut b);
            spline_out(cx, &a)
        }
        "x_spline_merge_list" => {
            let th = cx.f("closed_spline_distance_threshold").max(1e-6);
            let sps = input_splines(cx, "spline_coords", "spline_data");
            let mut out: Vec<Sp> = Vec::new();
            for sp in sps {
                if let Some(last) = out.last_mut() {
                    if let (Some(e), Some(f)) = (last.pts.last().copied(), sp.pts.first().copied()) {
                        if ((e.p[0] - f.p[0]).powi(2) + (e.p[1] - f.p[1]).powi(2)).sqrt() <= th {
                            let n = last.pts.len();
                            last.pts[n - 1].hout = f.hout;
                            last.pts.extend(sp.pts.into_iter().skip(1));
                            continue;
                        }
                    }
                }
                out.push(sp);
            }
            for sp in out.iter_mut() {
                if sp.pts.len() > 2 {
                    let (a, b) = (sp.pts[0].p, sp.pts[sp.pts.len() - 1].p);
                    if ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt() <= th {
                        let hin = sp.pts[sp.pts.len() - 1].hin;
                        sp.pts.pop();
                        sp.pts[0].hin = hin;
                        sp.closed = true;
                    }
                }
            }
            spline_out(cx, &out)
        }
        "x_spline_select" => {
            let sps = input_splines(cx, "spline_coords", "spline_data");
            let mode = cx.i("selection_mode");
            let (idx, a, b) = (cx.i("spline_index").max(0) as usize, cx.i("range_start").max(0) as usize, cx.i("range_end").max(0) as usize);
            let (t0, t1) = (cx.f("start"), cx.f("end"));
            let picked: Vec<Sp> = sps
                .into_iter()
                .enumerate()
                .filter(|(i, _)| if mode == 0 { *i == idx } else { *i + 1 >= a.min(b).max(1) && *i < a.max(b) })
                .map(|(_, sp)| trim(&sp, t0, t1))
                .collect();
            spline_out(cx, &picked)
        }
        "x_spline_warp" => {
            let mut sps = input_splines(cx, "spline_coords", "spline_data");
            let k = cx.f("warp_intensity");
            let centre = cx.f("warp_center");
            let tiling = cx.f("intensity_map_tiling");
            let im = cx.img("intensity_map");
            let vm = if cx.b("use_vector_map") { cx.img("vector_map") } else { None };
            let (sa, ea) = (cx.f("start_attenuation"), cx.f("end_attenuation"));
            for sp in sps.iter_mut() {
                let n = sp.pts.len();
                for (j, q) in sp.pts.iter_mut().enumerate() {
                    let t = if n > 1 { j as f32 / (n - 1) as f32 } else { 0.0 };
                    let att = smooth(0.0, sa.max(1e-4), t) * (1.0 - smooth(1.0 - ea.max(1e-4), 1.0, t));
                    let att = if sa <= 0.0 && ea <= 0.0 { 1.0 } else { att };
                    let off = match vm {
                        Some(v) => {
                            let p = v.sample(q.p[0] * tiling, q.p[1] * tiling);
                            [(p[0] - 0.5) * 2.0, (p[1] - 0.5) * 2.0]
                        }
                        None => {
                            let w = im.map(|m| g(m, q.p[0] * tiling, q.p[1] * tiling)).unwrap_or(centre) - centre;
                            let tn = [q.hout[0] - q.hin[0], q.hout[1] - q.hin[1]];
                            let l = (tn[0] * tn[0] + tn[1] * tn[1]).sqrt().max(1e-6);
                            [-tn[1] / l * w * 2.0, tn[0] / l * w * 2.0]
                        }
                    };
                    q.p = [q.p[0] + off[0] * k * att, q.p[1] + off[1] * k * att];
                }
                if cx.b("recompute_tangents") {
                    let closed = sp.closed;
                    auto_handles(&mut sp.pts, closed, 1.0);
                }
            }
            spline_out(cx, &sps)
        }
        "x_spline_sample_height" | "x_spline_sample_thickness" => {
            let mut sps = input_splines(cx, "spline_coords", "spline_data");
            let height = cx.kind.ends_with("height");
            let map = cx.img(if height { "height_map" } else { "thickness_map" });
            let opacity = cx.f("opacity").clamp(0.0, 1.0);
            let mode = cx.i("blending_mode");
            if let Some(m) = map {
                for sp in sps.iter_mut() {
                    for q in sp.pts.iter_mut() {
                        let x = g(m, q.p[0], q.p[1]);
                        let x = if height { x } else { x * 0.1 };
                        let cur = if height { q.ht } else { q.th };
                        let r = match mode {
                            1 => cur * x,
                            2 => cur + x,
                            _ => x,
                        };
                        let nv = lerp(cur, r, opacity);
                        if height {
                            q.ht = nv;
                        } else {
                            q.th = nv;
                        }
                    }
                }
            }
            spline_out(cx, &sps)
        }
        "x_spline_bridge_2_splines" => {
            let a = input_splines(cx, "spline_coords_1", "spline_data_1");
            let b = input_splines(cx, "spline_coords_2", "spline_data_2");
            let n = cx.i("bridge_splines_amount").clamp(1, 256) as usize;
            let (pa, pb) = (a.first().map(|s| flatten(s, 24)), b.first().map(|s| flatten(s, 24)));
            let mut out = Vec::new();
            if let (Some(pa), Some(pb)) = (pa, pb) {
                let (s1, e1, s2, e2) = (cx.f("spline_1_start"), cx.f("spline_1_end"), cx.f("spline_2_start"), cx.f("spline_2_end"));
                let (o1, o2) = (cx.f("spline_1_offset"), cx.f("spline_2_offset"));
                let (sr, er) = (cx.f("start_random_offset"), cx.f("end_random_offset"));
                let (stl, etl, str_, etr) = (cx.f("start_tangent_length"), cx.f("end_tangent_length"), cx.f("start_tangent_rotation"), cx.f("end_tangent_rotation"));
                for k in 0..n {
                    let f = if n == 1 { 0.5 } else { k as f32 / (n - 1) as f32 };
                    let ta = (lerp(s1, e1, f) + o1 + (rnd(k as i32, 1, seed) - 0.5) * sr).clamp(0.0, 1.0);
                    let tb = (lerp(s2, e2, f) + o2 + (rnd(k as i32, 2, seed) - 0.5) * er).clamp(0.0, 1.0);
                    let (p, tp, tha, ha) = at_param(&pa, ta);
                    let (q, tq, thb, hb) = at_param(&pb, tb);
                    let d = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt() / 3.0;
                    let rot = |t: [f32; 2], a: f32| {
                        let (sn, cs) = (a * TAU).sin_cos();
                        // leave the spline at a right angle (turned by the rotation)
                        let nrm = [-t[1], t[0]];
                        [nrm[0] * cs - nrm[1] * sn, nrm[0] * sn + nrm[1] * cs]
                    };
                    let (ra, rb) = (rot(tp, str_), rot(tq, etr));
                    let dir = [q[0] - p[0], q[1] - p[1]];
                    let sgn_a = if ra[0] * dir[0] + ra[1] * dir[1] < 0.0 { -1.0 } else { 1.0 };
                    let sgn_b = if rb[0] * dir[0] + rb[1] * dir[1] > 0.0 { -1.0 } else { 1.0 };
                    out.push(Sp {
                        pts: vec![
                            CP { p, hout: [ra[0] * d * stl * sgn_a, ra[1] * d * stl * sgn_a], th: tha, ht: ha, ..Default::default() },
                            CP { p: q, hin: [rb[0] * d * etl * sgn_b, rb[1] * d * etl * sgn_b], th: thb, ht: hb, ..Default::default() },
                        ],
                        closed: false,
                    });
                }
            }
            spline_out(cx, &out)
        }
        "x_spline_bridge_list_v2" => {
            let sps = input_splines(cx, "spline_coords", "spline_data");
            let n = cx.i("bridge_spline_amount").clamp(1, 64) as usize;
            let (fo, lo) = (cx.v2("first_bridge_spline_offset"), cx.v2("last_bridge_spline_offset"));
            let (thm, th, thr) = (cx.i("thickness_mode"), cx.f("thickness"), cx.f("thickness_random"));
            let (hm, hoff, hv, hr) = (cx.i("height_mode"), cx.f("height_offset"), cx.f("height"), cx.f("height_random"));
            let close = cx.b("close_bridge_spline");
            let polys: Vec<Poly> = sps.iter().map(|s| flatten(s, 24)).collect();
            let mut out = Vec::new();
            for k in 0..n {
                let f = if n == 1 { 0.5 } else { k as f32 / (n - 1) as f32 };
                let mut pts = Vec::new();
                for (i, p) in polys.iter().enumerate() {
                    let t = lerp(lerp(fo[0], lo[0], f), lerp(fo[1], lo[1], f), i as f32 / (polys.len().max(2) - 1) as f32);
                    let (q, _, pth, pht) = at_param(p, t);
                    let r = rnd(k as i32, i as i32, seed);
                    let thv = if thm == 0 { pth } else { th * (1.0 - r * thr) };
                    let htv = if hm == 0 { pht + hoff } else { hv * (1.0 - r * hr) };
                    pts.push(CP { p: q, th: thv, ht: htv, ..Default::default() });
                }
                auto_handles(&mut pts, close, 1.0);
                let mut sp = Sp { pts, closed: close };
                if cx.b("flip_direction") {
                    flip(&mut sp);
                }
                out.push(sp);
            }
            spline_out(cx, &out)
        }
        "x_scatter_splines_on_splines" => {
            let sps = input_splines(cx, "spline_coords", "spline_data");
            let custom = input_splines(cx, "custom_spline_coords", "custom_spline_data");
            let per = cx.i("spline_amount_per_spline").clamp(1, 32) as usize;
            let (t0, t1) = (cx.f("start"), cx.f("end"));
            let side = cx.i("side");
            let (scale, scale_rand) = (cx.f("scale"), cx.f("scale_random"));
            let (rot, rot_rand) = (cx.f("rotation"), cx.f("rotation_random"));
            let (lr_angle, lr_rand) = (cx.f("left_right_angle"), cx.f("left_right_angle_random"));
            let flip_dir = cx.b("flip_direction");
            let (sh, eh, sth, eth) = (cx.f("start_height"), cx.f("end_height"), cx.f("start_thickness"), cx.f("end_thickness"));
            let mut out = Vec::new();
            for (si, sp) in sps.iter().enumerate() {
                let p = flatten(sp, 24);
                for k in 0..per {
                    let f = if per == 1 { 0.5 } else { k as f32 / (per - 1) as f32 };
                    let t = lerp(t0, t1, f);
                    let (q, tn, _, _) = at_param(&p, t);
                    let r = |c: i32| rnd(si as i32 * 977 + k as i32, c, seed);
                    let left = match side {
                        1 => false,
                        2 => r(1) < 0.5,
                        _ => true,
                    };
                    let sgn = if left { 1.0 } else { -1.0 };
                    let a = (lr_angle * (1.0 - r(2) * lr_rand)) * sgn + rot + (r(3) - 0.5) * rot_rand;
                    let base_ang = tn[1].atan2(tn[0]) + a * TAU;
                    let len = scale * (1.0 - r(4) * scale_rand);
                    let mut child = match custom.first() {
                        Some(c) => {
                            let mut c = c.clone();
                            // the custom spline runs from its first point; it is turned and scaled onto the branch
                            let origin = c.pts.first().map(|q| q.p).unwrap_or([0.5, 0.5]);
                            let (sn, cs) = base_ang.sin_cos();
                            for qq in c.pts.iter_mut() {
                                let (x, y) = ((qq.p[0] - origin[0]) * len * 4.0, (qq.p[1] - origin[1]) * len * 4.0);
                                qq.p = [q[0] + x * cs - y * sn, q[1] + x * sn + y * cs];
                                for h in [&mut qq.hin, &mut qq.hout] {
                                    let (x, y) = (h[0] * len * 4.0, h[1] * len * 4.0);
                                    *h = [x * cs - y * sn, x * sn + y * cs];
                                }
                            }
                            c
                        }
                        None => {
                            let end = [q[0] + base_ang.cos() * len, q[1] + base_ang.sin() * len];
                            let mut pts = vec![CP { p: q, th: sth.max(0.0), ht: sh, ..Default::default() }, CP { p: end, th: eth.max(0.0), ht: eh, ..Default::default() }];
                            auto_handles(&mut pts, false, 1.0);
                            Sp { pts, closed: false }
                        }
                    };
                    if flip_dir {
                        flip(&mut child);
                    }
                    out.push(child);
                }
            }
            spline_out(cx, &out)
        }
        // ---------------------------------------------------------------- drawing splines
        "x_spline_render" => {
            let sps = input_splines(cx, "spline_coords", "spline_data");
            let mode = cx.i("draw_spline_index");
            let range = (cx.lane("draw_spline_range", 0).max(0.0) as usize, cx.lane("draw_spline_range", 1).max(0.0) as usize);
            let draw_mode = cx.i("mode");
            let (t0, t1) = (cx.f("start"), cx.f("end"));
            let picked: Vec<Sp> = sps
                .iter()
                .enumerate()
                .filter(|(i, _)| match draw_mode {
                    1 => *i == mode.max(0) as usize,
                    2 => *i + 1 >= range.0.max(1) && *i < range.1,
                    _ => true,
                })
                .map(|(_, sp)| trim(sp, t0, t1))
                .collect();
            let polys: Vec<Poly> = picked.iter().map(|s| flatten(s, cx.i("segments_amount").clamp(2, 64) as usize)).collect();
            let size_mode = cx.i("thickness_size_mode");
            let fixed = match size_mode {
                1 => cx.f("thickness_px") / s as f32,
                _ => cx.f("thickness_image"),
            };
            let bins = Bins::new(&polys, fixed + 0.05);
            let bg = cx.img("background");
            let bgk = cx.f("background_intensity");
            let style = cx.i("spline_style");
            let col = cx.f("spline_color");
            let rl = cx.f("random_luminance");
            let blend = cx.i("blend_mode");
            Tex::from_fn(s, hs, |u, v| {
                let b = bg.map(|t| g(t, u, v) * bgk).unwrap_or(0.0);
                let Some((d, t, th, ht, id, _)) = bins.nearest(u, v) else { return grey(b) };
                let w = if size_mode == 2 { th.max(0.5 / s as f32) } else { fixed.max(0.5 / s as f32) + th };
                let cover = 1.0 - smooth(w - 1.0 / s as f32, w, d);
                if cover <= 0.0 {
                    return grey(b);
                }
                let lumv = col * (1.0 - rnd(id as i32, 1, seed) * rl);
                // styles: 0 flat, 1 gradient along the spline, 2 the spline's height, 3 rounded profile
                let x = match style {
                    1 => t,
                    2 => ht,
                    3 => ht * (1.0 - (d / w.max(1e-6)).powi(2)).max(0.0).sqrt(),
                    _ => 1.0,
                } * lumv;
                let r = match blend {
                    0 => x,
                    2 => b + x,
                    3 => b * x,
                    _ => b.max(x),
                };
                grey(lerp(b, r, cover))
            })
        }
        "x_spline_fill" => {
            let sps = input_splines(cx, "spline_coords", "spline_data");
            let polys: Vec<Poly> = sps.iter().map(|s| flatten(s, 24)).collect();
            let mode = cx.i("output");
            Tex::from_fn(s, hs, |u, v| {
                // even-odd fill of every spline (open ones are closed by a straight line)
                let mut inside = 0usize;
                let mut idx = 0usize;
                for (k, p) in polys.iter().enumerate() {
                    let n = p.p.len();
                    if n < 3 {
                        continue;
                    }
                    let mut c = false;
                    let mut j = n - 1;
                    for i in 0..n {
                        let (a, b) = (p.p[i], p.p[j]);
                        if (a[1] > v) != (b[1] > v) && u < (b[0] - a[0]) * (v - a[1]) / (b[1] - a[1]) + a[0] {
                            c = !c;
                        }
                        j = i;
                    }
                    if c {
                        inside += 1;
                        idx = k + 1;
                    }
                }
                match mode {
                    1 => grey(idx as f32 / polys.len().max(1) as f32),
                    _ => grey((inside % 2) as f32),
                }
            })
        }
        "x_spline_flow_mapper" => {
            let sps = input_splines(cx, "spline_coords", "spline_data");
            let polys: Vec<Poly> = sps.iter().map(|s| flatten(s, cx.i("segments_amount").clamp(2, 64) as usize)).collect();
            let th_mode = cx.i("thickness_mode");
            let (th, thm) = (cx.f("thickness"), cx.f("thickness_multiplier"));
            let fixed = th * 0.25;
            let bins = Bins::new(&polys, fixed);
            let flip_d = cx.b("flip_direction");
            let dir_mode = cx.i("direction");
            let (satt, eatt) = (cx.b("start_attenuation"), cx.b("end_attenuation"));
            let bg = cx.img("background");
            let prof = cx.img("attenuation_profile_curve");
            Tex::from_fn(s, hs, |u, v| {
                let base = bg.map(|t| t.sample(u, v)).unwrap_or([0.5, 0.5, 0.5, 1.0]);
                let Some((d, t, sth, _, id, side)) = bins.nearest(u, v) else { return base };
                let w = if th_mode == 0 { fixed } else { sth * thm };
                if w <= 0.0 || d > w {
                    return base;
                }
                let p = &polys[id];
                let (_, tn, _, _) = at_param(p, t);
                let tn = if flip_d { [-tn[0], -tn[1]] } else { tn };
                let x = d / w;
                let mut k = match prof {
                    Some(c) => g(c, x, 0.5),
                    None => 1.0 - x * x,
                };
                if satt {
                    k *= smooth(0.0, 0.1, t);
                }
                if eatt {
                    k *= 1.0 - smooth(0.9, 1.0, t);
                }
                // direction 0 along the spline, 1 across it (toward it)
                let vec = if dir_mode == 1 { [-tn[1] * side, tn[0] * side] } else { tn };
                [lerp(base[0], vec[0] * 0.5 + 0.5, k), lerp(base[1], vec[1] * 0.5 + 0.5, k), base[2], 1.0]
            })
        }
        "x_spline_mapper_color" | "x_spline_mapper_grayscale" | "x_spline_bridge_mapper_color" | "x_spline_bridge_mapper_grayscale" => mapper(cx),
        "x_scatter_on_spline_color" | "x_scatter_on_spline_grayscale" => scatter_on_spline(cx),
        // ---------------------------------------------------------------- paths
        "x_paths_polygon" => {
            let n = cx.i("nbSides").clamp(3, 20) as usize;
            let pts: Vec<CP> = (0..n)
                .map(|i| {
                    let a = (i as f32 / n as f32 - 0.25) * TAU;
                    CP { p: [0.5 + a.cos() * 0.4, 0.5 + a.sin() * 0.4], ht: 1.0, ..Default::default() }
                })
                .collect();
            encode(&[Sp { pts, closed: true }], 1.0, s, hs)
        }
        "x_mask_to_paths_2" => {
            let mask = cx.img_or("mask", grey(0.0));
            let blur = cx.f("mask_filtering");
            let m = if blur > 0.1 { crate::ops::blur(&mask, blur * rs) } else { mask };
            let tol = cx.f("tolerance").max(1e-5);
            let sps: Vec<Sp> = match crate::spline::Spline::from_mask(&m, cx.f("threshold"), tol) {
                Ok(spl) => spl
                    .paths
                    .iter()
                    .map(|p| Sp {
                        pts: (0..p.pts.len())
                            .map(|i| {
                                let (hi, ho) = p.handles(i);
                                let corner = cx.f("corner_threshold");
                                // sharp corners keep no handles
                                let keep = corner < 1.0;
                                CP { p: p.pts[i].p, hin: if keep { hi } else { [0.0; 2] }, hout: if keep { ho } else { [0.0; 2] }, ht: 1.0, ..Default::default() }
                            })
                            .collect(),
                        closed: p.closed || cx.b("force_close_paths"),
                    })
                    .collect(),
                Err(_) => Vec::new(),
            };
            let pv = preview(&sps, None, s, hs, 1.5 * rs.max(0.5), false, 0.0);
            cx.pick(vec![Some(pv), Some(encode(&sps, 1.0, s, hs))])
        }
        "x_2dtransform_path" | "x_quad_transform_path" | "x_paths_warp" | "x_select_path" | "x_vertex_processor" | "x_vertex_processor_simple" => {
            let mut sps = cx.img("paths").and_then(decode).unwrap_or_default();
            match cx.kind {
                "x_2dtransform_path" => {
                    let m = cx.v4("matrix22");
                    let off = cx.v2("offset");
                    for sp in sps.iter_mut() {
                        for q in sp.pts.iter_mut() {
                            let (x, y) = (q.p[0] - 0.5, q.p[1] - 0.5);
                            q.p = [m[0] * x + m[1] * y + 0.5 + off[0], m[2] * x + m[3] * y + 0.5 + off[1]];
                            for hnd in [&mut q.hin, &mut q.hout] {
                                let (a, b) = (hnd[0], hnd[1]);
                                *hnd = [m[0] * a + m[1] * b, m[2] * a + m[3] * b];
                            }
                        }
                    }
                }
                "x_quad_transform_path" => {
                    let (p00, p01, p10, p11) = (cx.v2("p00"), cx.v2("p01"), cx.v2("p10"), cx.v2("p11"));
                    let map = |p: [f32; 2]| {
                        let (x, y) = (p[0], p[1]);
                        let a = [lerp(p00[0], p10[0], x), lerp(p00[1], p10[1], x)];
                        let b = [lerp(p01[0], p11[0], x), lerp(p01[1], p11[1], x)];
                        [lerp(a[0], b[0], y), lerp(a[1], b[1], y)]
                    };
                    for sp in sps.iter_mut() {
                        for q in sp.pts.iter_mut() {
                            let np = map(q.p);
                            let hi = map([q.p[0] + q.hin[0], q.p[1] + q.hin[1]]);
                            let ho = map([q.p[0] + q.hout[0], q.p[1] + q.hout[1]]);
                            q.hin = [hi[0] - np[0], hi[1] - np[1]];
                            q.hout = [ho[0] - np[0], ho[1] - np[1]];
                            q.p = np;
                        }
                    }
                }
                "x_paths_warp" => {
                    let k = cx.f("intensity");
                    if let Some(gm) = cx.img("gradient") {
                        let e = 1.0 / s as f32;
                        for sp in sps.iter_mut() {
                            for q in sp.pts.iter_mut() {
                                let gx = g(gm, q.p[0] + e, q.p[1]) - g(gm, q.p[0] - e, q.p[1]);
                                let gy = g(gm, q.p[0], q.p[1] + e) - g(gm, q.p[0], q.p[1] - e);
                                q.p = [q.p[0] + gx * k * 4.0, q.p[1] + gy * k * 4.0];
                            }
                        }
                    }
                }
                "x_select_path" => {
                    let mode = cx.i("mode");
                    let id = cx.f("path_id").round().max(1.0) as usize;
                    let greater = cx.b("greater");
                    let lim = cx.f("length");
                    sps = sps
                        .into_iter()
                        .enumerate()
                        .filter(|(i, sp)| if mode == 0 { *i + 1 == id } else { (length(&flatten(sp, 8)) > lim) == greater })
                        .map(|(_, sp)| sp)
                        .collect();
                }
                _ => {
                    // vertex processor: every vertex is moved by the first colour input (red/green, centred on 0.5)
                    let pv = cx.v2("perVertex");
                    if let Some(img) = cx.img("input_1") {
                        for sp in sps.iter_mut() {
                            for q in sp.pts.iter_mut() {
                                let c = img.sample(q.p[0], q.p[1]);
                                q.p = [q.p[0] + (c[0] - 0.5) * pv[0] * 0.1, q.p[1] + (c[1] - 0.5) * pv[1] * 0.1];
                            }
                        }
                    }
                    if let Some(img) = cx.img("input_2") {
                        for sp in sps.iter_mut() {
                            for q in sp.pts.iter_mut() {
                                q.ht = g(img, q.p[0], q.p[1]);
                            }
                        }
                    }
                }
            }
            encode(&sps, 1.0, s, hs)
        }
        "x_paths_trace" => {
            let sps = cx.img("paths").and_then(decode).unwrap_or_default();
            let bg = cx.img("background");
            let th = cx.f("thickness").max(0.5) * rs;
            let line = preview(&sps, None, s, hs, th, false, 0.0);
            let (corners, verts) = (cx.b("trace_corners"), cx.b("trace_vertices"));
            Tex::from_fn(s, hs, |u, v| {
                let b = bg.map(|t| t.sample(u, v)).unwrap_or([0.0, 0.0, 0.0, 1.0]);
                let l = line.sample(u, v)[0];
                let mut dot = 0.0f32;
                if corners || verts {
                    for sp in &sps {
                        for q in &sp.pts {
                            let sharp = q.hin == [0.0, 0.0] && q.hout == [0.0, 0.0];
                            if verts || (corners && sharp) {
                                let d = ((u - q.p[0]).powi(2) + (v - q.p[1]).powi(2)).sqrt() * s as f32;
                                dot = dot.max(1.0 - smooth(th * 1.5, th * 1.5 + 1.0, d));
                            }
                        }
                    }
                }
                [lerp(b[0], 1.0, l.max(dot)), lerp(b[1], 0.6, l) * (1.0 - dot) + dot * 0.2, lerp(b[2], 0.1, l.max(dot)), 1.0]
            })
        }
        "x_path_to_spline" => {
            let mut sps = cx.img("paths").and_then(decode).unwrap_or_default();
            for sp in sps.iter_mut() {
                let sharp = sp.pts.iter().all(|q| q.hin == [0.0, 0.0] && q.hout == [0.0, 0.0]);
                if sharp {
                    let closed = sp.closed;
                    auto_handles(&mut sp.pts, closed, 0.0);
                }
            }
            let data = encode(&sps, 0.0, s, hs);
            cx.pick(vec![Some(data.clone()), Some(data), Some(Tex::solid(s, hs, grey(sps.len() as f32 / 255.0)))])
        }
        // ---------------------------------------------------------------- shape splatter
        "x_shape_splatter" => shape_splatter(cx),
        "x_shape_splatter_blend" | "x_shape_splatter_blend_color" => {
            let d1 = cx.img("splatter_data_1");
            let d2 = cx.img("splatter_data_2");
            let color = cx.kind.ends_with("color");
            let n = cx.i("pattern_number").clamp(1, 8) as usize;
            let pats: Vec<Option<&Tex>> = (1..=n).map(|i| cx.img(&format!("pattern_{i}"))).collect();
            let bg = cx.img("background_color");
            let ci = cx.img("color_input");
            let cio = cx.f("color_input_opacity");
            let (ladj, lrand) = if color { (0.0, 0.0) } else { (cx.f("luminance_adjustement"), cx.f("luminance_random")) };
            let (hsl, hslr, nrand, rpa) = if color { (cx.v3("hsl_adjustement"), cx.v3("hsl_random"), cx.f("normal_angle_random"), cx.f("random_pattern_assignment")) } else { ([0.0; 3], [0.0; 3], 0.0, 0.0) };
            let is_normal = color && cx.b("is_normal");
            Tex::from_fn(s, hs, |u, v| {
                let base = bg.map(|t| t.sample(u, v)).unwrap_or(if is_normal { [0.5, 0.5, 1.0, 1.0] } else { [0.0, 0.0, 0.0, 1.0] });
                let Some(a) = d1.map(|t| t.sample(u, v)) else { return base };
                if a[3] <= 0.5 {
                    return base;
                }
                let idx = a[2] as i32;
                let mut pid = (a[3].max(1.0) - 1.0) as usize;
                if rpa > 0.0 && rnd(idx, 9, seed) < rpa {
                    pid = (rnd(idx, 10, seed) * n as f32) as usize;
                }
                let Some(Some(p)) = pats.get(pid.min(n - 1)) else { return base };
                let mut c = p.sample(a[0], a[1]);
                let centre = d2.map(|t| t.sample(u, v)).unwrap_or([u, v, 0.0, 1.0]);
                if !color {
                    let l = (c[0] + ladj + (rnd(idx, 3, seed) - 0.5) * 2.0 * lrand).clamp(0.0, 1.0);
                    let l = lerp(l, l * ci.map(|t| g(t, centre[0], centre[1])).unwrap_or(1.0), cio);
                    return grey(base[0].max(l));
                }
                if is_normal {
                    let n = crate::nodes10::dec_n(c, false);
                    let ang = centre[2] * TAU + (rnd(idx, 4, seed) - 0.5) * nrand * TAU;
                    let (sn, cs) = ang.sin_cos();
                    c = crate::nodes10::enc_n([n[0] * cs - n[1] * sn, n[0] * sn + n[1] * cs, n[2]], false);
                } else {
                    let (h, sa, va) = crate::ops::rgb_to_hsv(c[0], c[1], c[2]);
                    let r = |k: i32| rnd(idx, k, seed) - 0.5;
                    let (rr, gg, bb) = crate::ops::hsv_to_rgb((h + hsl[0] * 0.5 + r(5) * hslr[0]).rem_euclid(1.0), (sa + hsl[1] + r(6) * hslr[1]).clamp(0.0, 1.0), (va + hsl[2] + r(7) * hslr[2]).clamp(0.0, 1.0));
                    c = [rr, gg, bb, c[3]];
                    if let Some(ct) = ci {
                        c = crate::nodes10::lerp4(c, ct.sample(centre[0], centre[1]), cio);
                    }
                }
                crate::nodes10::lerp4(base, c, c[3].clamp(0.0, 1.0))
            })
        }
        "x_shape_splatter_data_extract" => {
            let d1 = cx.img_or("splatter_data_1", [0.0; 4]);
            let d2 = cx.img_or("splatter_data_2", [0.0; 4]);
            let outs: Vec<Option<Tex>> = (0..7)
                .map(|o| {
                    if !cx.wants(o) {
                        return None;
                    }
                    Some(Tex::from_fn(s, hs, |u, v| {
                        let (a, b) = (d1.sample(u, v), d2.sample(u, v));
                        let on = a[3] > 0.5;
                        match o {
                            0 => [b[0], b[1], 0.0, on as i32 as f32],
                            1 => [a[0], a[1], 0.0, on as i32 as f32],
                            2 => [u, v, 0.0, on as i32 as f32],
                            3 => [b[3], b[3], 0.0, on as i32 as f32],
                            4 => grey(if on { (a[2] + 1.0) / 1024.0 } else { 0.0 }),
                            5 => grey(if on { a[3] / 8.0 } else { 0.0 }),
                            _ => grey(if on { b[2].rem_euclid(1.0) } else { 0.0 }),
                        }
                    }))
                })
                .collect();
            cx.pick(outs)
        }
        "x_shape_splatter_to_mask" => {
            let d1 = cx.img_or("splatter_data_1", [0.0; 4]);
            let (a, b) = (cx.i("id_range_start").clamp(1, 8), cx.i("id_range_end").clamp(1, 8));
            let rm = cx.f("random_mask");
            let mode = cx.i("output");
            d1.map(|p| {
                if p[3] <= 0.5 {
                    return grey(0.0);
                }
                let pid = p[3].round() as i32;
                let keep = pid >= a.min(b) && pid <= a.max(b) && rnd(p[2] as i32, 11, seed) >= rm;
                grey(if !keep { 0.0 } else if mode == 1 { rnd(p[2] as i32, 12, seed) } else { 1.0 })
            })
        }
        // ---------------------------------------------------------------- scratches
        "x_scratches_generator_normal" => {
            let n = cx.i("spline_number").clamp(1, 512) as usize;
            let (rot, rot_r, sc, sc_r) = (cx.f("spline_rotation"), cx.f("spline_rotation_random"), cx.f("spline_scale"), cx.f("spline_scale_random"));
            let (dist, dist_r, freq) = (cx.f("spline_distortion"), cx.f("spline_distortion_random"), cx.f("spline_distortion_frequency"));
            let (wd, wd_r, pos_r) = (cx.f("spline_width"), cx.f("spline_width_random"), cx.f("spline_position_random"));
            let (ni, ni_r) = (cx.f("normal_intensity"), cx.f("normal_intensity_random"));
            let dx = cx.i("normal_format") == 0;
            let fade_mode = cx.i("fade_mode");
            let fade_len = cx.f("fade_length").clamp(0.0, 1.0);
            let px_w = cx.b("spline_width_in_pixel");
            let mut polys = Vec::new();
            let mut meta = Vec::new();
            for i in 0..n {
                let r = |k: i32| rnd(i as i32, k, seed.wrapping_add(77));
                let c = [lerp(0.5, r(1), pos_r), lerp(0.5, r(2), pos_r)];
                let a = (rot + (r(3) - 0.5) * rot_r) * TAU;
                let len = sc * (1.0 - r(4) * sc_r) * 0.5;
                let bend = dist * (1.0 - r(5) * dist_r) * 0.2;
                let seg = 8;
                let mut pts = Vec::new();
                for k in 0..=seg {
                    let t = k as f32 / seg as f32 - 0.5;
                    let off = (t * freq * 8.0 + r(6) * TAU).sin() * bend;
                    let (x, y) = (t * len, off);
                    pts.push(CP { p: [c[0] + x * a.cos() - y * a.sin(), c[1] + x * a.sin() + y * a.cos()], ..Default::default() });
                }
                auto_handles(&mut pts, false, 1.0);
                polys.push(flatten(&Sp { pts, closed: false }, 6));
                let w = wd * (1.0 - r(7) * wd_r) * if px_w { 1.0 / s as f32 } else { 1.0 / 512.0 };
                meta.push((w.max(0.5 / s as f32), ni * (1.0 - r(8) * ni_r)));
            }
            let bins = Bins::new(&polys, 4.0 / 512.0 * wd.max(0.1));
            let res: Vec<([f32; 4], [f32; 4])> = crate::tex::par_map(s * hs, |i| {
                let (u, v) = (((i % s) as f32 + 0.5) / s as f32, ((i / s) as f32 + 0.5) / hs as f32);
                let flat = (crate::nodes10::enc_n([0.0, 0.0, 1.0], dx), grey(0.0));
                let Some((d, t, _, _, id, side)) = bins.nearest(u, v) else { return flat };
                let (w, k) = meta[id];
                if d > w {
                    return flat;
                }
                let fade = match fade_mode {
                    0 => 1.0,
                    _ => smooth(0.0, fade_len * 0.5 + 1e-3, t) * (1.0 - smooth(1.0 - fade_len * 0.5 - 1e-3, 1.0, t)),
                };
                let (_, tn, _, _) = at_param(&polys[id], t);
                // a V groove: the walls lean toward the scratch's middle
                let slope = side * k * fade * 2.0;
                let nrm = crate::nodes10::enc_n([-tn[1] * slope, tn[0] * slope, 1.0], dx);
                (nrm, grey(fade * (1.0 - d / w)))
            });
            let (nrm, mask): (Vec<[f32; 4]>, Vec<[f32; 4]>) = res.into_iter().unzip();
            cx.pick(vec![Some(Tex { w: s, h: hs, px: nrm }), Some(Tex { w: s, h: hs, px: mask })])
        }
        _ => return None,
    })
}

/// The part of a spline between parameters `t0` and `t1` (by point count).
fn trim(sp: &Sp, t0: f32, t1: f32) -> Sp {
    let (a, b) = (t0.min(t1).clamp(0.0, 1.0), t0.max(t1).clamp(0.0, 1.0));
    if a <= 0.0 && b >= 1.0 || sp.pts.len() < 2 {
        return sp.clone();
    }
    // resample: a dense polyline of the curve, then the kept part as straight points with smooth handles
    let p = flatten(sp, 16);
    let n = ((p.p.len() as f32) * (b - a)).ceil().max(2.0) as usize;
    let mut pts: Vec<CP> = (0..=n)
        .map(|k| {
            let t = lerp(a, b, k as f32 / n as f32);
            let (q, _, th, ht) = at_param(&p, t);
            CP { p: q, th, ht, ..Default::default() }
        })
        .collect();
    auto_handles(&mut pts, false, 1.0);
    Sp { pts, closed: false }
}

/// Spline Mapper and Spline Bridge Mapper: a picture mapped along the splines (or across the ribbons between
/// consecutive splines). Outputs: colour, height, uv, id (or mask).
fn mapper(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let _seed = cx.seed;
    let sps = input_splines(cx, "spline_coords", "spline_data");
    let segs = cx.i("segments_amount").clamp(2, 64) as usize;
    let polys: Vec<Poly> = sps.iter().map(|sp| flatten(sp, segs)).collect();
    let bridge = cx.kind.starts_with("x_spline_bridge");
    let color = cx.kind.ends_with("color");
    let cmap = cx.img("color_map");
    let bgc = if cx.lib.input("background_color").is_some() { cx.col("background_color") } else { [0.0, 0.0, 0.0, 1.0] };
    let bg = cx.img("background");
    let uv_scale = cx.v2("manual_uv_scale");
    // per pixel: (colour, height, uv, id/mask)
    let res: Vec<[[f32; 4]; 4]> = if bridge {
        let rot = cx.f("uv_rotation");
        let mut out = vec![[bgc, grey(0.0), [0.0; 4], grey(0.0)]; s * hs];
        if let Some(b) = bg {
            for (i, o) in out.iter_mut().enumerate() {
                let (u, v) = (((i % s) as f32 + 0.5) / s as f32, ((i / s) as f32 + 0.5) / hs as f32);
                o[1] = grey(g(b, u, v));
            }
        }
        // rasterise the ribbon between spline k and k+1 as triangles of a (t, side) grid
        for k in 0..polys.len().saturating_sub(1) {
            let n = segs * 4;
            let at = |p: &Poly, t: f32| at_param(p, t).0;
            for j in 0..n {
                let (t0, t1) = (j as f32 / n as f32, (j + 1) as f32 / n as f32);
                let (a0, a1, b0, b1) = (at(&polys[k], t0), at(&polys[k], t1), at(&polys[k + 1], t0), at(&polys[k + 1], t1));
                for tri in [[(a0, t0, 0.0), (a1, t1, 0.0), (b1, t1, 1.0)], [(a0, t0, 0.0), (b1, t1, 1.0), (b0, t0, 1.0)]] {
                    raster_tri(&tri, s, hs, &mut |i, tt, ss| {
                        let (su, sv) = (tt * uv_scale[0], ss * uv_scale[1]);
                        let (sn, cs) = (rot * TAU).sin_cos();
                        let (uu, vv) = (su * cs - sv * sn, su * sn + sv * cs);
                        let c = cmap.map(|m| m.sample(uu, vv)).unwrap_or(grey(1.0));
                        out[i] = [if color { c } else { grey(lum(c)) }, grey(1.0 - (ss - 0.5).abs() * 2.0), [crate::noise::wrap01(uu), crate::noise::wrap01(vv), 0.0, 1.0], grey(1.0)];
                    });
                }
            }
        }
        out
    } else {
        let th_mode = cx.i("thickness_mode");
        let (th, thm) = (cx.f("thickness"), cx.f("thickness_multiplier"));
        let fixed = th * 0.25;
        let bins = Bins::new(&polys, fixed);
        let shape = cx.i("shape");
        let (ch_mul, ch_off) = (cx.f("cylinder_height_multiplier"), cx.f("cylinder_height_offset"));
        let hmap = cx.img("height_map");
        let (shm, imh) = (cx.f("spline_height_multiplier"), cx.f("input_map_height_multplier"));
        let twist = cx.f("twist_uvs_intensity");
        let (t0, t1) = (cx.f("start"), cx.f("end"));
        let auto = cx.b("auto_scale_uvs");
        let lengths: Vec<f32> = polys.iter().map(length).collect();
        crate::tex::par_map(s * hs, |i| {
            let (u, v) = (((i % s) as f32 + 0.5) / s as f32, ((i / s) as f32 + 0.5) / hs as f32);
            let back = [bgc, grey(bg.map(|b| g(b, u, v)).unwrap_or(0.0)), [0.0; 4], grey(0.0)];
            let Some((d, t, sth, ht, id, side)) = bins.nearest(u, v) else { return back };
            if t < t0.min(t1) || t > t0.max(t1) {
                return back;
            }
            let w = if th_mode == 0 { fixed } else { sth * thm };
            if w <= 0.0 || d > w {
                return back;
            }
            let across = 0.5 + side * d / w * 0.5;
            let along = if auto { t * lengths[id] / (2.0 * w).max(1e-4) } else { t };
            let (uu, vv) = (along * uv_scale[0] + twist * across, across * uv_scale[1]);
            let c = cmap.map(|m| m.sample(uu, vv)).unwrap_or(grey(1.0));
            let profile = if shape == 1 { ((1.0 - (d / w).powi(2)).max(0.0).sqrt() * ch_mul + ch_off + 1.0).clamp(0.0, 1.0) } else { 1.0 };
            let h = (profile * ht * shm + hmap.map(|m| (m.sample(uu, vv)[0] - 0.5) * imh).unwrap_or(0.0)).clamp(0.0, 1.0);
            let col = if color { c } else { grey(lum(c)) };
            [col, grey(h.max(back[1][0])), [crate::noise::wrap01(uu), crate::noise::wrap01(vv), 0.0, 1.0], grey((id + 1) as f32 / polys.len().max(1) as f32)]
        })
    };
    let mut outs: Vec<Vec<[f32; 4]>> = vec![Vec::with_capacity(s * hs); 4];
    for px in res {
        for (k, o) in outs.iter_mut().enumerate() {
            o.push(px[k]);
        }
    }
    cx.pick(outs.into_iter().map(|px| Some(Tex { w: s, h: hs, px })).collect())
}

/// Fill a triangle of (position, t, side) corners, giving each covered pixel its interpolated (t, side).
fn raster_tri(tri: &[([f32; 2], f32, f32); 3], s: usize, hs: usize, f: &mut dyn FnMut(usize, f32, f32)) {
    let p: Vec<[f32; 2]> = tri.iter().map(|c| [c.0[0] * s as f32, c.0[1] * hs as f32]).collect();
    let (x0, x1) = (p.iter().map(|q| q[0]).fold(f32::MAX, f32::min).floor().max(0.0) as i64, p.iter().map(|q| q[0]).fold(f32::MIN, f32::max).ceil().min(s as f32 - 1.0) as i64);
    let (y0, y1) = (p.iter().map(|q| q[1]).fold(f32::MAX, f32::min).floor().max(0.0) as i64, p.iter().map(|q| q[1]).fold(f32::MIN, f32::max).ceil().min(hs as f32 - 1.0) as i64);
    let area = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
    if area.abs() < 1e-9 {
        return;
    }
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = ((p[1][0] - px) * (p[2][1] - py) - (p[2][0] - px) * (p[1][1] - py)) / area;
            let w1 = ((p[2][0] - px) * (p[0][1] - py) - (p[0][0] - px) * (p[2][1] - py)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                continue;
            }
            let t = w0 * tri[0].1 + w1 * tri[1].1 + w2 * tri[2].1;
            let sd = w0 * tri[0].2 + w1 * tri[1].2 + w2 * tri[2].2;
            f(y as usize * s + x as usize, t, sd);
        }
    }
}

/// Built-in pattern shapes of the scatter nodes, at local (-1..1) coordinates.
fn builtin(pattern: i32, x: f32, y: f32, var: f32) -> f32 {
    let r = (x * x + y * y).sqrt();
    match pattern {
        0 => 0.0,
        2 => ((x.abs() < 1.0) && (y.abs() < 1.0)) as i32 as f32,
        3 => 1.0 - smooth(0.9, 1.0, r),
        4 => (1.0 - r * r).max(0.0),
        5 => (-(r * r) * 4.0).exp() * (r < 1.0) as i32 as f32,
        6 => (1.0 - r).max(0.0).powf(1.0 + var * 4.0),
        _ => 1.0 - smooth(0.9, 1.0, r),
    }
}

fn scatter_on_spline(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let seed = cx.seed;
    let color = cx.kind.ends_with("color");
    let sps = input_splines(cx, "spline_coords", "spline_data");
    let polys: Vec<Poly> = sps.iter().map(|sp| flatten(sp, 24)).collect();
    let n_pat = cx.i("pattern_input_number").clamp(1, 8) as usize;
    let pats: Vec<Option<&Tex>> = (1..=n_pat).map(|i| cx.img(&format!("pattern_input_{i}"))).collect();
    let pattern = cx.i("pattern");
    let var = cx.f("pattern_variation");
    let mode = cx.i("scatter_mode");
    let amount = cx.i("shape_amount").clamp(1, 1024) as usize;
    let spacing = cx.f("shape_spacing").max(1e-4);
    let (t0, t1) = (cx.f("start"), cx.f("end"));
    let size = cx.v2("pattern_size");
    let size_r = cx.v2("size_random");
    let pscale = cx.f("pattern_scale");
    let th_scale = cx.f("thickness_scale");
    let scale_r = cx.f("scale_random");
    let size_mode = cx.i("size_mode");
    let align = cx.b("rotation_align_with_tangent");
    let (rot, rot_r) = (cx.f("rotation_around_center"), cx.f("rotation_around_center_random"));
    let loff = cx.v2("local_offset");
    let loff_r = cx.v2("local_offset_random");
    let off_on = cx.f("offset_on_spline");
    let off_on_r = cx.f("random_offset_on_spline");
    let blend = cx.i("blend_mode");
    let mask_r = cx.f("mask_random");
    let scale_map = cx.img("scale_map");
    let smul = cx.f("scale_map_input_multiplier");
    let mask_map = cx.img("mask_map");
    let mth = cx.f("mask_map_threshold");
    let base_col = if color { cx.col("shape_base_color") } else { grey(cx.f("shape_base_color") * if cx.lib.input("shape_base_color_multiplier").is_some() { cx.f("shape_base_color_multiplier") } else { 1.0 }) };
    let rand_col = if color { cx.v4("random_color") } else { [0.0; 4] };
    let rand_lum = if color { cx.f("random_color_luminance") } else { cx.f("random_luminance") };
    let cmap = if color { cx.img("color_map") } else { None };
    let cmul = if color { cx.f("color_map_input_multiplier") } else { 0.0 };
    // instances: (centre, angle, half size x, half size y, pattern, colour)
    let mut inst: Vec<([f32; 2], f32, f32, f32, usize, [f32; 4])> = Vec::new();
    for (si, p) in polys.iter().enumerate() {
        let len = length(p).max(1e-6);
        let count = if mode == 0 { ((len * (t1 - t0).abs()) / spacing).floor() as usize + 1 } else { amount };
        for k in 0..count.min(4096) {
            let id = (si * 4096 + k) as i32;
            let r = |c: i32| rnd(id, c, seed.wrapping_add(313));
            if r(1) < mask_r {
                continue;
            }
            let f = if count == 1 { 0.5 } else { k as f32 / (count - 1) as f32 };
            let t = (lerp(t0, t1, f) + off_on + (r(2) - 0.5) * off_on_r).rem_euclid(1.0 + 1e-6).min(1.0);
            let (q, tn, th, ht) = at_param(p, t);
            let nrm = [-tn[1], tn[0]];
            let o = [loff[0] + (r(3) - 0.5) * loff_r[0] * 0.1, loff[1] + (r(4) - 0.5) * loff_r[1] * 0.1];
            let c = [q[0] + tn[0] * o[1] + nrm[0] * o[0], q[1] + tn[1] * o[1] + nrm[1] * o[0]];
            if let Some(m) = mask_map {
                if g(m, c[0], c[1]) < mth {
                    continue;
                }
            }
            let mut sc = pscale * (1.0 - r(5) * scale_r) * scale_map.map(|m| lerp(1.0, g(m, c[0], c[1]), smul)).unwrap_or(1.0);
            if size_mode == 1 {
                sc = th * th_scale.max(1e-3);
            }
            let (hx, hy) = (sc * size[0] * (1.0 - r(6) * size_r[0]), sc * size[1] * (1.0 - r(7) * size_r[1]));
            let a = if align { tn[1].atan2(tn[0]) } else { 0.0 } + (rot + (r(8) - 0.5) * rot_r) * TAU;
            let pid = (r(9) * n_pat as f32) as usize % n_pat;
            let lumk = 1.0 - r(10) * rand_lum;
            let mut col = [base_col[0] * lumk, base_col[1] * lumk, base_col[2] * lumk, base_col[3]];
            for ch in 0..4 {
                col[ch] += (r(11 + ch as i32) - 0.5) * rand_col[ch];
            }
            if let Some(cm) = cmap {
                col = crate::nodes10::lerp4(col, cm.sample(c[0], c[1]), cmul);
            }
            if !color {
                col = grey(col[0] * if cx.lib.input("spline_height_color_multiplier").is_some() { lerp(1.0, ht, cx.f("spline_height_color_multiplier")) } else { 1.0 });
            }
            inst.push((c, a, hx.max(1e-5), hy.max(1e-5), pid, col));
        }
    }
    // bin the instances
    let gn = 32usize;
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); gn * gn];
    for (k, it) in inst.iter().enumerate() {
        let r = it.2.max(it.3) * 1.5;
        let c = |v: f32| ((v * gn as f32).floor() as i64).clamp(0, gn as i64 - 1) as usize;
        for gy in c(it.0[1] - r)..=c(it.0[1] + r) {
            for gx in c(it.0[0] - r)..=c(it.0[0] + r) {
                grid[gy * gn + gx].push(k);
            }
        }
    }
    let bg = cx.img("background");
    let bgc = if color { cx.col("background_color") } else { grey(0.0) };
    Tex::from_fn(s, hs, |u, v| {
        let mut acc = bg.map(|b| b.sample(u, v)).unwrap_or(bgc);
        let cell = &grid[((v * gn as f32) as usize).min(gn - 1) * gn + ((u * gn as f32) as usize).min(gn - 1)];
        for &k in cell {
            let (c, a, hx, hy, pid, col) = inst[k];
            let (dx, dy) = (u - c[0], v - c[1]);
            let (sn, cs) = a.sin_cos();
            let (lx, ly) = ((dx * cs + dy * sn) / hx, (-dx * sn + dy * cs) / hy);
            if lx.abs() > 1.0 || ly.abs() > 1.0 {
                continue;
            }
            let val = if pattern == 1 {
                match pats[pid] {
                    Some(t) => t.sample(lx * 0.5 + 0.5, ly * 0.5 + 0.5),
                    None => continue,
                }
            } else {
                grey(builtin(pattern, lx, ly, var))
            };
            let w = if color { val[3] * lum(val).max(if pattern == 1 { 1.0 } else { 0.0 }) } else { val[0] };
            let shape = if color && pattern == 1 { [val[0] * col[0], val[1] * col[1], val[2] * col[2], val[3]] } else { [col[0] * w, col[1] * w, col[2] * w, 1.0] };
            acc = match blend {
                0 => crate::nodes10::lerp4(acc, shape, if color { val[3].min(1.0).max(w.min(1.0)) } else { (w > 0.0) as i32 as f32 }),
                1 => [acc[0].max(shape[0]), acc[1].max(shape[1]), acc[2].max(shape[2]), acc[3].max(shape[3])],
                3 => [acc[0] * (1.0 - w) + acc[0] * shape[0] * w, acc[1] * (1.0 - w) + acc[1] * shape[1] * w, acc[2] * (1.0 - w) + acc[2] * shape[2] * w, acc[3]],
                _ => [acc[0] + shape[0], acc[1] + shape[1], acc[2] + shape[2], acc[3]],
            };
        }
        if !color {
            acc = grey(acc[0]);
        }
        acc
    })
}

/// Shape Splatter: patterns placed on a grid with random size, rotation and height; the height picture plus the
/// placement records the other splatter nodes read.
fn shape_splatter(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let seed = cx.seed;
    let (xa, ya) = (cx.i("x_amount").clamp(1, 64), cx.i("y_amount").clamp(1, 64));
    let n = cx.i("pattern_input_number").clamp(1, 8) as usize;
    let pats: Vec<Option<&Tex>> = (1..=n).map(|i| cx.img(&format!("pattern_{i}"))).collect();
    let (scale, scale_r) = (cx.f("shape_scale"), cx.f("shape_scale_random"));
    let size = cx.v2("shape_size");
    let pos_r = cx.f("position_random");
    let (ox, oy) = (cx.f("position_offset_horizontal"), cx.f("position_offset_vertical"));
    let (rot, rot_r) = (cx.f("shape_rotation"), cx.f("shape_rotation_random"));
    let (hsc, hsc_r, hoff, hoff_r) = (cx.f("height_scale"), cx.f("height_scale_random"), cx.f("height_offset"), cx.f("height_offset_random"));
    let mask_r = cx.f("mask_random");
    let dist_mode = cx.i("pattern_distribution_mode");
    let dist_map = cx.img("pattern_distribution_map");
    let scale_map = cx.img("shape_scale_map");
    let smul = cx.f("shape_scale_map_multiplier");
    let rot_map = cx.img("shape_rotation_map");
    let rmul = cx.f("shape_rotation_map_multiplier");
    let bgh = cx.img("background_height");
    let cut = cx.b("height_cutout_black_pixels");
    let prev = cx.img("splatter_data_1");
    let avoid = cx.f("avoid_previous_splatter_overlap");
    let res: Vec<([f32; 4], [f32; 4], [f32; 4])> = crate::tex::par_map(s * hs, |i| {
        let (u, v) = (((i % s) as f32 + 0.5) / s as f32, ((i / s) as f32 + 0.5) / hs as f32);
        let base_h = bgh.map(|t| g(t, u, v)).unwrap_or(0.0);
        let mut best: Option<(f32, [f32; 4], [f32; 4])> = None;
        let (gx, gy) = ((u - ox) * xa as f32, (v - oy) * ya as f32);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (cxi, cyi) = (gx.floor() as i32 + dx, gy.floor() as i32 + dy);
                let (wx, wy) = (cxi.rem_euclid(xa), cyi.rem_euclid(ya));
                let idx = wy * xa + wx;
                let r = |k: i32| rnd(idx, k, seed.wrapping_add(555));
                if r(1) < mask_r {
                    continue;
                }
                let centre = [((cxi as f32 + 0.5 + (r(2) - 0.5) * pos_r) / xa as f32) + ox, ((cyi as f32 + 0.5 + (r(3) - 0.5) * pos_r) / ya as f32) + oy];
                let wc = [crate::noise::wrap01(centre[0]), crate::noise::wrap01(centre[1])];
                if avoid > 0.0 {
                    if let Some(pd) = prev {
                        if pd.sample(wc[0], wc[1])[3] > 0.5 && r(4) < avoid {
                            continue;
                        }
                    }
                }
                let sc = scale * (1.0 - r(5) * scale_r) * scale_map.map(|m| lerp(1.0, g(m, wc[0], wc[1]), smul)).unwrap_or(1.0);
                let (hx, hy) = (sc * size[0] * 0.5 / xa as f32, sc * size[1] * 0.5 / ya as f32);
                let a = rot + (r(6) - 0.5) * rot_r + rot_map.map(|m| g(m, wc[0], wc[1]) * rmul).unwrap_or(0.0);
                let (sn, cs) = (a * TAU).sin_cos();
                let (du, dv) = (crate::ops::wrapd(u - centre[0]), crate::ops::wrapd(v - centre[1]));
                let (lx, ly) = ((du * cs + dv * sn) / hx.max(1e-5), (-du * sn + dv * cs) / hy.max(1e-5));
                if lx.abs() > 1.0 || ly.abs() > 1.0 {
                    continue;
                }
                let pid = match dist_mode {
                    1 => (idx as usize) % n,
                    2 => (dist_map.map(|m| g(m, wc[0], wc[1])).unwrap_or(0.0) * n as f32) as usize,
                    _ => (r(7) * n as f32) as usize,
                }
                .min(n - 1);
                let (lu, lv) = (lx * 0.5 + 0.5, ly * 0.5 + 0.5);
                let pv = match pats[pid] {
                    Some(t) => g(t, lu, lv),
                    None => (1.0 - (lx * lx + ly * ly)).max(0.0).sqrt(),
                };
                if cut && pv <= 0.0 {
                    continue;
                }
                let hv = pv * hsc * (1.0 - r(8) * hsc_r) + hoff + (r(9) - 0.5) * hoff_r;
                if best.map(|b| hv > b.0).unwrap_or(true) {
                    best = Some((hv, [lu, lv, idx as f32, (pid + 1) as f32], [wc[0], wc[1], a, sc]));
                }
            }
        }
        match best {
            Some((h, d1, d2)) => (grey(base_h.max(h)), d1, d2),
            None => (grey(base_h), [0.0; 4], [0.0; 4]),
        }
    });
    let mut a = Vec::with_capacity(s * hs);
    let mut b = Vec::with_capacity(s * hs);
    let mut c = Vec::with_capacity(s * hs);
    for (h, d1, d2) in res {
        a.push(h);
        b.push(d1);
        c.push(d2);
    }
    let _ = (PI, noise);
    cx.pick(vec![Some(Tex { w: s, h: hs, px: a }), Some(Tex { w: s, h: hs, px: b }), Some(Tex { w: s, h: hs, px: c })])
}
