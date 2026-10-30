// SPDX-License-Identifier: MIT OR Apache-2.0
// Icons of the core (atomic) nodes for the library's tiles, drawn as vectors so they stay sharp at any size. They are
// laid out on a 72 x 62 box (the artwork's units), scaled to fit the tile.

/// Draw the icon of core node `kind` (`sbs:<identifier>`) in `r`; false when the node has none.
fn core_icon(p: &egui::Painter, r: Rect, kind: &str) -> bool {
    let Some(id) = kind.strip_prefix("sbs:") else { return false };
    let known = matches!(
        id,
        "blend" | "blur" | "shuffle" | "curve" | "dirmotionblur" | "directionalwarp" | "distance" | "emboss" | "dyngradient" | "gradient" | "grayscaleconversion" | "hsl" | "levels"
            | "normal" | "sharpen" | "transformation" | "warp" | "bitmap" | "fxmaps" | "pixelprocessor" | "svg" | "text" | "uniform" | "valueprocessor"
    );
    if !known {
        return false;
    }
    // the artwork's box (x 14..86, y 8..70) fitted into `r`, centred
    let s = (r.width() / 72.0).min(r.height() / 62.0);
    let o = r.center() - vec2(36.0 * s, 31.0 * s);
    let c = |x: f32, y: f32| o + vec2((x - 14.0) * s, (y - 8.0) * s);
    let rc = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_max(c(x, y), c(x + w, y + h));
    let st = |w: f32, col: Color32| Stroke::new(w * s, col);
    let poly = |pts: &[(f32, f32)], stroke: Stroke| p.add(egui::Shape::line(pts.iter().map(|&(x, y)| c(x, y)).collect(), stroke));
    let quad = |a: (f32, f32), b: (f32, f32), e: (f32, f32), stroke: Stroke| {
        p.add(egui::epaint::QuadraticBezierShape::from_points_stroke([c(a.0, a.1), c(b.0, b.1), c(e.0, e.1)], false, Color32::TRANSPARENT, stroke));
    };
    let hgrad = |rect: Rect, a: Color32, b: Color32| {
        let mut m = egui::Mesh::default();
        for (pos, col) in [(rect.left_top(), a), (rect.right_top(), b), (rect.right_bottom(), b), (rect.left_bottom(), a)] {
            m.colored_vertex(pos, col);
        }
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        p.add(m);
    };
    let (ink, soft) = (hex(0xeeeeee, 1.0), hex(0xdddddd, 1.0));
    p.rect_filled(rc(14.0, 8.0, 72.0, 62.0), 10.0 * s, hex(0x2a2a2a, 1.0));
    match id {
        "blend" => {
            for x in [43.0, 57.0] {
                p.circle(c(x, 39.0), 14.0 * s, hex(0xcccccc, 0.4), st(2.5, ink));
            }
        }
        "blur" => {
            for (rad, a) in [(17.0, 0.25), (11.0, 0.5), (5.0, 0.9)] {
                p.circle_filled(c(50.0, 39.0), rad * s, hex(0xdddddd, a));
            }
        }
        "shuffle" => {
            let k = st(3.0, ink);
            poly(&[(32.0, 32.0), (66.0, 32.0)], k);
            poly(&[(60.0, 26.0), (66.0, 32.0), (60.0, 38.0)], k);
            poly(&[(68.0, 46.0), (34.0, 46.0)], k);
            poly(&[(40.0, 40.0), (34.0, 46.0), (40.0, 52.0)], k);
        }
        "curve" => {
            poly(&[(32.0, 26.0), (32.0, 52.0), (68.0, 52.0)], st(2.0, hex(0x777777, 1.0)));
            p.add(egui::epaint::CubicBezierShape::from_points_stroke([c(34.0, 50.0), c(46.0, 50.0), c(44.0, 30.0), c(66.0, 30.0)], false, Color32::TRANSPARENT, st(3.0, ink)));
        }
        "dirmotionblur" => {
            for (y, a) in [(27.0, 0.35), (37.0, 0.7), (47.0, 1.0)] {
                p.rect_filled(rc(32.0, y, 36.0, 4.0), 2.0 * s, hex(0xeeeeee, a));
            }
        }
        "directionalwarp" | "distance" => {
            // three waves: Q 36,y-6 then smooth continuations (the reflected control points alternate)
            for y in [30.0, 40.0, 50.0] {
                let k = st(2.5, ink);
                quad((30.0, y), (36.0, y - 6.0), (42.0, y), k);
                quad((42.0, y), (48.0, y + 6.0), (54.0, y), k);
                quad((54.0, y), (60.0, y - 6.0), (66.0, y), k);
            }
        }
        "emboss" => {
            p.rect_filled(rc(36.0, 26.0, 28.0, 26.0), 0.0, hex(0x666666, 1.0));
            poly(&[(36.0, 52.0), (36.0, 26.0), (64.0, 26.0)], st(3.0, hex(0xffffff, 1.0)));
            poly(&[(64.0, 26.0), (64.0, 52.0), (36.0, 52.0)], st(3.0, hex(0xaaaaaa, 1.0)));
        }
        "dyngradient" => {
            let b = rc(32.0, 22.0, 36.0, 34.0);
            hgrad(b, hex(0xffffff, 1.0), hex(0x333333, 1.0));
            p.rect_stroke(b, 0.0, st(1.0, hex(0x888888, 1.0)), egui::StrokeKind::Middle);
        }
        "gradient" => hgrad(rc(32.0, 22.0, 36.0, 34.0), hex(0x6a5acd, 1.0), hex(0xf0c040, 1.0)),
        "grayscaleconversion" => {
            p.rect_filled(rc(32.0, 22.0, 18.0, 34.0), 0.0, hex(0x888888, 1.0));
            p.rect_filled(rc(50.0, 22.0, 18.0, 34.0), 0.0, hex(0xe0a040, 1.0));
        }
        "hsl" | "levels" => {
            for (x, y, col) in [(42.0, 34.0, 0xe04040), (58.0, 34.0, 0x40c040), (50.0, 48.0, 0x4080e0)] {
                p.circle_filled(c(x, y), 9.0 * s, hex(col, 0.85));
            }
        }
        "normal" => {
            let b = rc(32.0, 22.0, 36.0, 34.0);
            p.rect_filled(b, 3.0 * s, hex(0x7a7aff, 1.0));
            p.rect_stroke(b, 3.0 * s, st(1.5, hex(0xc8c8ff, 1.0)), egui::StrokeKind::Middle);
        }
        "sharpen" => {
            poly(&[(32.0, 54.0), (42.0, 42.0), (48.0, 48.0), (56.0, 30.0), (68.0, 54.0)], st(3.0, ink));
        }
        "transformation" => {
            p.rect_stroke(rc(30.0, 28.0, 22.0, 22.0), 0.0, st(2.5, ink), egui::StrokeKind::Middle);
            // the second square turned 20 degrees about (53, 33)
            let (sn, cs) = 20f32.to_radians().sin_cos();
            let turn = |x: f32, y: f32| {
                let (dx, dy) = (x - 53.0, y - 33.0);
                c(53.0 + dx * cs - dy * sn, 33.0 + dx * sn + dy * cs)
            };
            let pts = vec![turn(42.0, 22.0), turn(64.0, 22.0), turn(64.0, 44.0), turn(42.0, 44.0)];
            p.add(egui::Shape::closed_line(pts, st(2.5, ink)));
        }
        "warp" => {
            p.rect_stroke(rc(32.0, 22.0, 36.0, 34.0), 0.0, st(2.5, ink), egui::StrokeKind::Middle);
            let k = st(2.0, hex(0xeeeeee, 0.6));
            quad((32.0, 22.0), (44.0, 39.0), (32.0, 56.0), k);
            quad((68.0, 22.0), (56.0, 39.0), (68.0, 56.0), k);
        }
        "bitmap" | "fxmaps" => {
            let corners = [(32.0, 22.0), (68.0, 22.0), (68.0, 56.0), (32.0, 56.0), (32.0, 22.0)];
            let pts: Vec<Pos2> = corners.iter().map(|&(x, y)| c(x, y)).collect();
            p.extend(egui::Shape::dashed_line(&pts, st(2.0, ink), 4.0 * s, 3.0 * s));
            p.circle_filled(c(50.0, 39.0), 7.0 * s, soft);
        }
        "pixelprocessor" => {
            let shades = [[0xdddddd, 0x888888, 0x444444], [0x444444, 0xdddddd, 0x888888], [0x888888, 0x444444, 0xdddddd]];
            for (j, row) in shades.iter().enumerate() {
                for (i, col) in row.iter().enumerate() {
                    p.rect_filled(rc(30.0 + 14.0 * i as f32, 20.0 + 14.0 * j as f32, 12.0, 12.0), 0.0, hex(*col, 1.0));
                }
            }
        }
        "svg" => {
            let pts = vec![c(34.0, 26.0), c(66.0, 26.0), c(50.0, 54.0)];
            p.add(egui::Shape::closed_line(pts, st(2.5, ink)));
            for (x, y) in [(34.0, 26.0), (66.0, 26.0), (50.0, 54.0)] {
                p.circle_filled(c(x, y), 4.0 * s, soft);
            }
        }
        "text" => {
            p.text(c(50.0, 41.0), Align2::CENTER_CENTER, "T", FontId::proportional(36.0 * s), soft);
        }
        "uniform" => {
            p.rect_filled(rc(32.0, 22.0, 36.0, 34.0), 0.0, hex(0xd4a017, 1.0));
        }
        "valueprocessor" => {
            let k = st(2.0, hex(0x777777, 1.0));
            for y in [28.0, 39.0, 50.0] {
                poly(&[(32.0, y), (68.0, y)], k);
            }
            for (x, y) in [(40.0, 28.0), (60.0, 39.0), (46.0, 50.0)] {
                p.circle_filled(c(x, y), 4.0 * s, ink);
            }
        }
        _ => {}
    }
    true
}

/// An sRGB colour `0xrrggbb` with opacity `a`.
fn hex(rgb: u32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}
