// SPDX-License-Identifier: MIT OR Apache-2.0
// ======================================================================= Spline editor (included into gui.rs)
//
// Draw splines and custom shapes with bezier curves. A spline lives in the package as an SVG resource that an SVG
// node shows (white shapes on black), so everything that takes a picture takes it: Bevel, Distance, Blur, Tile
// Sampler and the rest. The editable spline travels inside the SVG, so it opens again exactly as drawn.
//
//   Pen tool    click to add a point, drag to pull its handles; click the first point of an open path to close it
//   Edit tool   drag points and handles; double-click a curve to add a point; right-click a point to delete it
//   Both        Delete removes the selected point

use materialcraft::spline::{SPath, SPoint, ShapeStyle, Spline};

/// The package resource an SVG node reads (`pkg:///name?dependency=uid` gives `name`).
fn svg_node_res(n: &Node) -> Option<String> {
    match native::values(n).get("vectorresourcepath")? {
        PVal::Const { raw, .. } => {
            let r = raw.strip_prefix("pkg:///")?;
            Some(r.split('?').next().unwrap_or(r).to_string())
        }
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SplTool {
    Pen,
    Edit,
}

#[derive(Clone, Copy, PartialEq)]
enum SplDrag {
    None,
    Point,
    HandleIn,
    HandleOut,
    /// a point just placed with the pen: dragging pulls its handles out
    Pen,
}

struct SplineEd {
    node: u32,
    /// resource name inside the package
    res: String,
    sp: Spline,
    styles: Vec<ShapeStyle>,
    path: usize,
    pt: Option<usize>,
    tool: SplTool,
    drag: SplDrag,
    undo: Vec<(Spline, Vec<ShapeStyle>)>,
    redo: Vec<(Spline, Vec<ShapeStyle>)>,
    /// snap points to a grid of this many cells across (0 = off)
    snap: u32,
    /// the spline changed and the node's picture is out of date
    changed: bool,
    preview: Option<egui::TextureHandle>,
    preview_stale: bool,
    note: String,
}

fn spl_state(sp: &Spline, styles: &[ShapeStyle]) -> (Spline, Vec<ShapeStyle>) {
    (sp.clone(), styles.to_vec())
}

impl SplineEd {
    fn push_undo(&mut self) {
        self.undo.push(spl_state(&self.sp, &self.styles));
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn fix_styles(&mut self) {
        while self.styles.len() < self.sp.paths.len() {
            self.styles.push(ShapeStyle::default());
        }
        self.styles.truncate(self.sp.paths.len());
        if self.path >= self.sp.paths.len() {
            self.path = self.sp.paths.len().saturating_sub(1);
        }
    }

    fn edited(&mut self) {
        self.changed = true;
        self.preview_stale = true;
    }

    /// Set the tangent mode of point `i` of the active path.
    fn set_tangent(&mut self, i: usize, mode: u8) {
        let Some(path) = self.sp.paths.get_mut(self.path) else { return };
        if i >= path.pts.len() || path.pts.len() < 2 {
            return;
        }
        let (hi, ho) = path.handles(i);
        let q = &mut path.pts[i];
        q.tan = mode;
        match mode {
            0 => {
                q.hin = None;
                q.hout = None;
            }
            1 => {
                q.hout = Some(ho);
                q.hin = Some([-ho[0], -ho[1]]);
            }
            2 => {
                q.hin = Some(hi);
                q.hout = Some(ho);
            }
            _ => {
                q.hin = Some([0.0, 0.0]);
                q.hout = Some([0.0, 0.0]);
            }
        }
    }
}

impl App {
    /// Open the spline editor on SVG node `id`.
    fn open_spline_editor(&mut self, id: u32) {
        let Some(n) = self.g.nodes.get(&id) else { return };
        if n.kind != "sbs:svg" {
            return;
        }
        let res = svg_node_res(n).unwrap_or_else(|| "spline".into());
        let key_prefix = format!("{res}.");
        let svg = self.g.resources.iter().find(|(k, _)| k.starts_with(&key_prefix)).map(|(_, v)| String::from_utf8_lossy(v).to_string());
        let mut note = String::new();
        let (sp, styles) = match svg {
            Some(s) => match Spline::from_svg_meta(&s) {
                Some(x) => x,
                None => match Spline::from_svg(&s) {
                    Ok(sp) => {
                        note = "Converted from the SVG: curves became short straight segments.".into();
                        let n = sp.paths.len();
                        (sp, vec![ShapeStyle::default(); n])
                    }
                    Err(e) => {
                        note = format!("The SVG could not be read as a spline ({e}); drawing a new shape.");
                        (Spline::default(), Vec::new())
                    }
                },
            },
            None => (Spline::default(), Vec::new()),
        };
        let mut ed = SplineEd { node: id, res, sp, styles, path: 0, pt: None, tool: SplTool::Pen, drag: SplDrag::None, undo: Vec::new(), redo: Vec::new(), snap: 0, changed: false, preview: None, preview_stale: true, note };
        ed.fix_styles();
        if !ed.sp.paths.is_empty() {
            ed.tool = SplTool::Edit;
        }
        self.spl = Some(ed);
    }

    /// A new SVG node showing a spline shape (a circle to start from), with the editor open on it.
    fn new_spline_node(&mut self) {
        let c = self.view_center_world();
        let mut name = "spline".to_string();
        let taken: Vec<String> = native::package_resources(&self.g).into_iter().map(|(n, _)| n).collect();
        let mut k = 2;
        while taken.iter().any(|t| t == &name) {
            name = format!("spline_{k}");
            k += 1;
        }
        let sp = materialcraft::session::spline_preset("circle").unwrap_or_default();
        let styles = vec![ShapeStyle::default(); sp.paths.len()];
        match native::set_resource(&mut self.g, &name, "svg", "svg", sp.to_svg(&styles).into_bytes()) {
            Ok(path) => {
                for (parent, _) in self.parents.iter_mut() {
                    native::adopt_package(parent, &self.g);
                }
                let id = self.sd_add_node("sbs:svg", None, c, false);
                if let Some(n) = self.g.nodes.get_mut(&id) {
                    native::set_value(n, "colorswitch", Some(PVal::Const { ty: PTy::B, raw: "0".into(), rel: None }));
                    native::set_value(n, "vectorresourcepath", Some(PVal::Const { ty: PTy::Path, raw: path, rel: None }));
                }
                self.open_spline_editor(id);
            }
            Err(e) => self.status(e, false),
        }
    }

    /// Write the spline into the package and refresh what shows it.
    fn apply_spline(&mut self) {
        let Some(ed) = self.spl.as_mut() else { return };
        if !ed.changed {
            return;
        }
        ed.changed = false;
        let (node, res, svg) = (ed.node, ed.res.clone(), ed.sp.to_svg(&ed.styles));
        if self.g.nodes.get(&node).is_none() {
            return;
        }
        self.snap();
        match native::set_resource(&mut self.g, &res, "svg", "svg", svg.into_bytes()) {
            Ok(path) => {
                for (parent, _) in self.parents.iter_mut() {
                    native::adopt_package(parent, &self.g);
                }
                if let Some(n) = self.g.nodes.get_mut(&node) {
                    native::set_value(n, "vectorresourcepath", Some(PVal::Const { ty: PTy::Path, raw: path, rel: None }));
                }
                self.dirty = true;
            }
            Err(e) => self.status(e, false),
        }
    }

    fn spline_editor(&mut self, ctx: &egui::Context) {
        let Some(mut ed) = self.spl.take() else { return };
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Spline editor").id(Id::new("spline_editor")).open(&mut open).default_size(vec2(760.0, 640.0)).resizable(true).show(ctx, |ui| {
            self.spline_ui(ui, &mut ed, &mut apply);
        });
        if apply {
            self.spl = Some(ed);
            self.apply_spline();
            return self.spline_after_apply(open);
        }
        if open {
            self.spl = Some(ed);
        }
    }

    fn spline_after_apply(&mut self, open: bool) {
        if !open {
            self.spl = None;
        }
    }

    fn spline_ui(&mut self, ui: &mut egui::Ui, ed: &mut SplineEd, apply: &mut bool) {
        ed.fix_styles();
        // ---- toolbar
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut ed.tool, SplTool::Pen, "Pen").on_hover_text("Click to add points, drag to pull handles, click the first point to close the path");
            ui.selectable_value(&mut ed.tool, SplTool::Edit, "Edit").on_hover_text("Drag points and handles, double-click a curve to add a point, right-click a point to delete it");
            ui.separator();
            if ui.add_enabled(!ed.undo.is_empty(), egui::Button::new("Undo")).clicked() {
                if let Some(prev) = ed.undo.pop() {
                    ed.redo.push(spl_state(&ed.sp, &ed.styles));
                    (ed.sp, ed.styles) = prev;
                    ed.pt = None;
                    ed.fix_styles();
                    ed.edited();
                    *apply = true;
                }
            }
            if ui.add_enabled(!ed.redo.is_empty(), egui::Button::new("Redo")).clicked() {
                if let Some(next) = ed.redo.pop() {
                    ed.undo.push(spl_state(&ed.sp, &ed.styles));
                    (ed.sp, ed.styles) = next;
                    ed.pt = None;
                    ed.fix_styles();
                    ed.edited();
                    *apply = true;
                }
            }
            ui.separator();
            ui.menu_button("Shapes", |ui| {
                ui.label("Replace the spline with:");
                for (id, label) in [("circle", "Circle"), ("square", "Square"), ("rounded_square", "Rounded square"), ("triangle", "Triangle"), ("star", "Star"), ("gear", "Gear"), ("heart", "Heart"), ("arrow", "Arrow"), ("cross", "Cross"), ("wave", "Wave line"), ("spiral", "Spiral")] {
                    if ui.button(label).clicked() {
                        if let Some(sp) = materialcraft::session::spline_preset(id) {
                            ed.push_undo();
                            ed.sp = sp;
                            ed.styles = vec![ShapeStyle::default(); ed.sp.paths.len()];
                            ed.path = 0;
                            ed.pt = None;
                            ed.edited();
                            *apply = true;
                        }
                        ui.close();
                    }
                }
                ui.separator();
                ui.label("Add to the spline:");
                for (id, label) in [("circle", "Circle"), ("square", "Square"), ("star", "Star"), ("heart", "Heart")] {
                    if ui.button(label).clicked() {
                        if let Some(sp) = materialcraft::session::spline_preset(id) {
                            ed.push_undo();
                            ed.sp.merge(sp);
                            ed.fix_styles();
                            ed.path = ed.sp.paths.len() - 1;
                            ed.pt = None;
                            ed.edited();
                            *apply = true;
                        }
                        ui.close();
                    }
                }
            });
            if ui.button("New path").on_hover_text("Start another path (the Pen tool then adds its points)").clicked() {
                ed.push_undo();
                ed.sp.paths.push(SPath::default());
                ed.styles.push(ShapeStyle::default());
                ed.path = ed.sp.paths.len() - 1;
                ed.pt = None;
                ed.tool = SplTool::Pen;
            }
            if ui.add_enabled(!ed.sp.paths.is_empty(), egui::Button::new("Delete path")).clicked() {
                ed.push_undo();
                ed.sp.paths.remove(ed.path);
                ed.styles.remove(ed.path);
                ed.pt = None;
                ed.fix_styles();
                ed.edited();
                *apply = true;
            }
            ui.separator();
            egui::ComboBox::from_id_salt("spl_snap").width(96.0).selected_text(if ed.snap == 0 { "Snap: off".to_string() } else { format!("Snap: {}", ed.snap) }).show_ui(ui, |ui| {
                ui.selectable_value(&mut ed.snap, 0, "Off");
                for g in [8u32, 16, 32, 64] {
                    ui.selectable_value(&mut ed.snap, g, format!("{g} cells"));
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            if !ed.sp.paths.is_empty() {
                ui.label(format!("Path {} of {}", ed.path + 1, ed.sp.paths.len()));
                let mut ch = false;
                let i = ed.path;
                {
                    let p = &mut ed.sp.paths[i];
                    ch |= ui.checkbox(&mut p.closed, "Closed").changed();
                    ch |= ui.checkbox(&mut p.smooth, "Smooth").on_hover_text("Smooth curves through the points; off gives straight corners where a point has no handles").changed();
                }
                let can_fill = ed.sp.paths[i].closed;
                ch |= ui.add_enabled(can_fill, egui::Checkbox::new(&mut ed.styles[i].fill, "Fill")).changed();
                ch |= ui.add(egui::Slider::new(&mut ed.styles[i].stroke, 0.0..=0.2).text("Stroke width")).on_hover_text("0 = no stroke (an open path then uses a thin line)").changed();
                if ch {
                    ed.edited();
                    *apply = true;
                }
                if ui.button("Previous").clicked() && ed.path > 0 {
                    ed.path -= 1;
                    ed.pt = None;
                }
                if ui.button("Next").clicked() && ed.path + 1 < ed.sp.paths.len() {
                    ed.path += 1;
                    ed.pt = None;
                }
            }
        });
        if let (Some(i), true) = (ed.pt, !ed.sp.paths.is_empty()) {
            if i < ed.sp.paths[ed.path].pts.len() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("Point {}", i + 1));
                    let cur = ed.sp.paths[ed.path].pts[i].tan;
                    for (m, n) in [(0u8, "Auto"), (1, "Mirrored"), (2, "Free"), (3, "Corner")] {
                        if ui.selectable_label(cur == m, n).clicked() && cur != m {
                            ed.push_undo();
                            ed.set_tangent(i, m);
                            ed.edited();
                            *apply = true;
                        }
                    }
                    let mut w = ed.sp.paths[ed.path].pts[i].w;
                    if ui.add(egui::Slider::new(&mut w, 0.1..=3.0).text("Width")).on_hover_text("Scales the path's width at this point (read by the Path nodes' width)").changed() {
                        ed.sp.paths[ed.path].pts[i].w = w;
                        ed.edited();
                    }
                    if ui.button("Delete point").clicked() {
                        ed.push_undo();
                        ed.sp.paths[ed.path].pts.remove(i);
                        ed.pt = None;
                        ed.edited();
                        *apply = true;
                    }
                });
            }
        }
        if !ed.note.is_empty() {
            ui.colored_label(Color32::from_rgb(230, 190, 110), ed.note.clone());
        }
        ui.separator();
        // ---- canvas and preview side by side
        let side = (ui.available_width() - 210.0).min(ui.available_height() - 6.0).clamp(240.0, 900.0);
        ui.horizontal_top(|ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(side, side), Sense::click_and_drag());
            self.spline_canvas(ui, ed, rect, &resp, apply);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("RESULT").monospace().size(10.0));
                if ed.preview_stale || ed.preview.is_none() {
                    ed.preview_stale = false;
                    let svg = ed.sp.to_svg(&ed.styles);
                    if let Ok(t) = materialcraft::vector::render_svg(&svg, 192, 192, 1, 1.0, 0.0, 0.0) {
                        let px: Vec<Color32> = t.px.iter().map(|c| {
                            // white shapes on black
                            let v = (c[0].max(c[1]).max(c[2]) * c[3]).clamp(0.0, 1.0);
                            let g = (v * 255.0).round() as u8;
                            Color32::from_gray(g)
                        }).collect();
                        let img = egui::ColorImage { size: [t.w, t.h], source_size: vec2(t.w as f32, t.h as f32), pixels: px };
                        match &mut ed.preview {
                            Some(h) => h.set(img, egui::TextureOptions::LINEAR),
                            None => ed.preview = Some(ui.ctx().load_texture("spline_preview", img, egui::TextureOptions::LINEAR)),
                        }
                    }
                }
                if let Some(h) = &ed.preview {
                    ui.image((h.id(), vec2(192.0, 192.0)));
                }
                ui.small("What the SVG node outputs.\nConnect it to Bevel, Distance,\nBlur ... like any picture.");
                ui.add_space(8.0);
                ui.small(format!("{} path(s), {} point(s)", ed.sp.paths.len(), ed.sp.point_count()));
                ui.small(format!("Resource: {}", ed.res));
            });
        });
    }

    fn spline_canvas(&mut self, ui: &mut egui::Ui, ed: &mut SplineEd, rect: Rect, resp: &egui::Response, apply: &mut bool) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, Color32::from_rgb(24, 24, 24));
        let to_s = |p: [f32; 2]| pos2(rect.left() + p[0] * rect.width(), rect.top() + p[1] * rect.height());
        let to_t = |s: Pos2| [(s.x - rect.left()) / rect.width(), (s.y - rect.top()) / rect.height()];
        // grid
        for k in 1..8 {
            let t = k as f32 / 8.0;
            let c = if k == 4 { Color32::from_rgb(70, 70, 70) } else { Color32::from_rgb(44, 44, 44) };
            painter.line_segment([to_s([t, 0.0]), to_s([t, 1.0])], Stroke::new(1.0_f32, c));
            painter.line_segment([to_s([0.0, t]), to_s([1.0, t])], Stroke::new(1.0_f32, c));
        }
        painter.rect_stroke(rect, 2.0, Stroke::new(1.0_f32, Color32::from_gray(110)), egui::StrokeKind::Inside);
        // curves of every path (the active one brighter)
        let (fw, fh) = (rect.width(), rect.height());
        for (pi, path) in ed.sp.paths.iter().enumerate() {
            if path.pts.len() < 2 {
                continue;
            }
            let poly = Spline { paths: vec![path.clone()] }.flatten(fw, fh);
            let col = if pi == ed.path { Color32::from_rgb(110, 225, 200) } else { Color32::from_gray(150) };
            for pl in &poly {
                let mut pts: Vec<Pos2> = pl.pts.iter().map(|p| pos2(rect.left() + p[0], rect.top() + p[1])).collect();
                if pl.closed && pts.len() > 2 {
                    pts.push(pts[0]);
                }
                painter.add(egui::Shape::line(pts, Stroke::new(if pi == ed.path { 2.0 } else { 1.2 }, col)));
            }
        }
        // points and the handles of the selected one
        let hit_r = 8.0;
        let ptr = resp.hover_pos().or_else(|| ui.input(|i| i.pointer.interact_pos()));
        if let Some(path) = ed.sp.paths.get(ed.path) {
            if let Some(i) = ed.pt.filter(|&i| i < path.pts.len()) {
                if path.pts.len() >= 2 {
                    let (hi, ho) = path.handles(i);
                    let c = to_s(path.pts[i].p);
                    for h in [hi, ho] {
                        let hp = to_s([path.pts[i].p[0] + h[0], path.pts[i].p[1] + h[1]]);
                        painter.line_segment([c, hp], Stroke::new(1.0_f32, Color32::from_rgb(255, 190, 90)));
                        painter.circle(hp, 4.5, Color32::from_rgb(255, 190, 90), Stroke::new(1.0_f32, Color32::BLACK));
                    }
                }
            }
        }
        for (pi, path) in ed.sp.paths.iter().enumerate() {
            for (i, q) in path.pts.iter().enumerate() {
                let c = to_s(q.p);
                let sel = pi == ed.path && ed.pt == Some(i);
                let r = Rect::from_center_size(c, Vec2::splat(if sel { 10.0 } else { 7.0 }));
                let fill = if sel { Color32::WHITE } else if i == 0 { Color32::from_rgb(110, 225, 200) } else if pi == ed.path { Color32::from_gray(210) } else { Color32::from_gray(120) };
                painter.rect(r, 1.0, fill, Stroke::new(1.0_f32, Color32::BLACK), egui::StrokeKind::Inside);
            }
        }
        // a ghost of the next point under the Pen
        if ed.tool == SplTool::Pen && ed.drag == SplDrag::None {
            if let Some(p) = ptr.filter(|p| rect.contains(*p)) {
                painter.circle_stroke(p, 5.0, Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(110, 225, 200, 160)));
            }
        }
        let snap = |mut t: [f32; 2], on: u32| -> [f32; 2] {
            if on > 0 {
                let g = on as f32;
                t = [(t[0] * g).round() / g, (t[1] * g).round() / g];
            }
            t
        };
        let _ = &snap;
        // ---- hit tests
        let find_point = |ed: &SplineEd, at: Pos2| -> Option<(usize, usize)> {
            // the active path first, then the others
            let order: Vec<usize> = std::iter::once(ed.path).chain((0..ed.sp.paths.len()).filter(|&k| k != ed.path)).collect();
            for pi in order {
                let Some(path) = ed.sp.paths.get(pi) else { continue };
                for (i, q) in path.pts.iter().enumerate().rev() {
                    if (to_s(q.p) - at).length() <= hit_r {
                        return Some((pi, i));
                    }
                }
            }
            None
        };
        let find_handle = |ed: &SplineEd, at: Pos2| -> Option<SplDrag> {
            let i = ed.pt?;
            let path = ed.sp.paths.get(ed.path)?;
            if i >= path.pts.len() || path.pts.len() < 2 {
                return None;
            }
            let (hi, ho) = path.handles(i);
            let p = path.pts[i].p;
            let sp = |h: [f32; 2]| to_s([p[0] + h[0], p[1] + h[1]]);
            if (sp(ho) - at).length() <= hit_r && (hi != [0.0, 0.0] || ho != [0.0, 0.0] || true) {
                return Some(SplDrag::HandleOut);
            }
            if (sp(hi) - at).length() <= hit_r {
                return Some(SplDrag::HandleIn);
            }
            None
        };
        // ---- interaction
        if resp.drag_started_by(PointerButton::Primary) {
            if let Some(at) = resp.interact_pointer_pos() {
                if let Some(h) = find_handle(ed, at) {
                    ed.push_undo();
                    ed.drag = h;
                } else if let Some((pi, i)) = find_point(ed, at) {
                    // the first point of an open path with the Pen closes it
                    let first_close = ed.tool == SplTool::Pen && pi == ed.path && i == 0 && !ed.sp.paths[pi].closed && ed.sp.paths[pi].pts.len() >= 3;
                    ed.push_undo();
                    if first_close {
                        ed.sp.paths[pi].closed = true;
                        ed.edited();
                        *apply = true;
                    } else {
                        ed.path = pi;
                        ed.pt = Some(i);
                        ed.drag = SplDrag::Point;
                    }
                } else if ed.tool == SplTool::Pen && rect.contains(at) {
                    ed.push_undo();
                    // a closed (or no) active path: the point starts a new one
                    if ed.sp.paths.is_empty() || ed.sp.paths[ed.path].closed {
                        ed.sp.paths.push(SPath::default());
                        ed.styles.push(ShapeStyle::default());
                        ed.path = ed.sp.paths.len() - 1;
                    }
                    let t = snap(to_t(at), ed.snap);
                    let path = &mut ed.sp.paths[ed.path];
                    path.pts.push(SPoint::new(t[0], t[1]));
                    ed.pt = Some(path.pts.len() - 1);
                    ed.drag = SplDrag::Pen;
                    ed.edited();
                }
            }
        }
        if resp.dragged_by(PointerButton::Primary) {
            if let (Some(at), Some(i)) = (resp.interact_pointer_pos(), ed.pt) {
                let t = to_t(at);
                let on = ed.snap;
                match ed.drag {
                    SplDrag::Point => {
                        let t = snap(t, on);
                        if let Some(q) = ed.sp.paths.get_mut(ed.path).and_then(|p| p.pts.get_mut(i)) {
                            q.p = t;
                        }
                        ed.edited();
                    }
                    SplDrag::Pen => {
                        if let Some(q) = ed.sp.paths.get_mut(ed.path).and_then(|p| p.pts.get_mut(i)) {
                            let d = [t[0] - q.p[0], t[1] - q.p[1]];
                            if (d[0] * rect.width()).hypot(d[1] * rect.height()) > 4.0 {
                                q.hout = Some(d);
                                q.hin = Some([-d[0], -d[1]]);
                                q.tan = 1;
                                ed.edited();
                            }
                        }
                    }
                    SplDrag::HandleIn | SplDrag::HandleOut => {
                        let mode = ed.drag;
                        if let Some(q) = ed.sp.paths.get_mut(ed.path).and_then(|p| p.pts.get_mut(i)) {
                            let d = [t[0] - q.p[0], t[1] - q.p[1]];
                            let (mine, other) = if mode == SplDrag::HandleOut { (d, [-d[0], -d[1]]) } else { (d, [-d[0], -d[1]]) };
                            match (mode, q.tan) {
                                (SplDrag::HandleOut, 1) => {
                                    q.hout = Some(mine);
                                    q.hin = Some(other);
                                }
                                (SplDrag::HandleIn, 1) => {
                                    q.hin = Some(mine);
                                    q.hout = Some(other);
                                }
                                (SplDrag::HandleOut, _) => {
                                    q.hout = Some(mine);
                                    q.tan = 2;
                                }
                                _ => {
                                    q.hin = Some(mine);
                                    q.tan = 2;
                                }
                            }
                            ed.edited();
                        }
                    }
                    SplDrag::None => {}
                }
            }
        }
        if resp.drag_stopped() {
            if ed.drag != SplDrag::None {
                *apply = true;
            }
            ed.drag = SplDrag::None;
        }
        // double-click a curve: a new point on it
        if resp.double_clicked() {
            if let Some(at) = resp.interact_pointer_pos() {
                if find_point(ed, at).is_none() {
                    let best = ed.sp.paths.iter().enumerate().filter_map(|(pi, p)| p.nearest(to_t(at), rect.width(), rect.height()).map(|(seg, t, d)| (pi, seg, t, d))).min_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));
                    if let Some((pi, seg, t, d)) = best {
                        if d < 12.0 {
                            ed.push_undo();
                            let at = ed.sp.paths[pi].split(seg, t);
                            ed.path = pi;
                            ed.pt = Some(at);
                            ed.edited();
                            *apply = true;
                        }
                    }
                }
            }
        }
        // right-click a point: delete it
        if resp.secondary_clicked() {
            if let Some(at) = resp.interact_pointer_pos() {
                if let Some((pi, i)) = find_point(ed, at) {
                    ed.push_undo();
                    ed.sp.paths[pi].pts.remove(i);
                    ed.path = pi;
                    ed.pt = None;
                    ed.edited();
                    *apply = true;
                }
            }
        }
        if resp.clicked() && ed.drag == SplDrag::None {
            if let Some(at) = resp.interact_pointer_pos() {
                match find_point(ed, at) {
                    Some((pi, i)) => {
                        ed.path = pi;
                        ed.pt = Some(i);
                    }
                    None if ed.tool == SplTool::Edit => ed.pt = None,
                    None => {}
                }
            }
        }
        // Delete removes the selected point
        let del = ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
        if del && resp.hovered() {
            if let Some(i) = ed.pt {
                if let Some(p) = ed.sp.paths.get_mut(ed.path) {
                    if i < p.pts.len() {
                        ed.push_undo();
                        ed.sp.paths[ed.path].pts.remove(i);
                        ed.pt = None;
                        ed.edited();
                        *apply = true;
                    }
                }
            }
        }
    }
}
