// SPDX-License-Identifier: MIT OR Apache-2.0
// ======================================================================= UI (included into gui.rs)

struct NodeSnap {
    id: u32,
    kind: String,
    slots: Vec<(&'static str, bool)>,
    ins: Vec<(String, u32)>,
    pos: [f32; 2],
    graph_name: Option<String>,
    dot: bool,
    /// Output names of a graph node with several outputs (empty = a single default output).
    outs: Vec<String>,
    in_out: Vec<(String, String)>,
}

impl App {
    fn screen_to_world(&self, p: Pos2) -> Pos2 {
        let w = ((p - self.canvas_rect.min) - self.pan) / self.zoom;
        pos2(w.x, w.y)
    }

    fn open_search(&mut self, screen: Pos2, link_from: Option<u32>) {
        let w = self.screen_to_world(screen);
        self.search = Some(SearchState { screen, world: pos2(w.x - CARD * 0.5, w.y - CARD_H * 0.5), text: String::new(), idx: 0, link_from, link_to: None, fresh: true });
    }

    fn open_search_to(&mut self, screen: Pos2, dst: u32, slot: String) {
        self.open_search(screen, None);
        if let Some(st) = self.search.as_mut() {
            st.link_to = Some((dst, slot));
        }
    }

    // ------------------------------------------------------------ graph canvas
    fn canvas(&mut self, ui: &mut egui::Ui) {
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = resp.rect;
        self.canvas_rect = rect;
        painter.rect_filled(rect, 0.0, Color32::from_rgb(58, 58, 58));
        let mods = ui.input(|i| i.modifiers);
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let released = ui.input(|i| i.pointer.any_released());
        let mut acts: Vec<Act> = Vec::new();
        // taking a payload of the wrong type would discard it, so check which kind is being dragged first
        if egui::DragAndDrop::has_payload_of_type::<GraphDrag>(ui.ctx()) {
            if let (Some(payload), Some(p)) = (resp.dnd_release_payload::<GraphDrag>(), pointer) {
                acts.push(Act::ImportFromPkg(payload.0, payload.2.clone(), Some(p)));
            }
        } else if let (Some(payload), Some(p)) = (resp.dnd_release_payload::<String>(), pointer) {
            acts.push(Act::Drop((*payload).clone(), p));
        }

        if ui.rect_contains_pointer(rect) {
            // Navigation follows the input DEVICE, not the operating system, so it behaves the same on
            // Windows, Linux and macOS:
            //   - a mouse wheel reports in lines or pages, so the wheel zooms (Shift + wheel pans sideways);
            //   - a trackpad or Magic Mouse reports in points, so two fingers pan and a pinch zooms.
            // The raw wheel events are read here rather than egui's smoothed scroll delta: egui eases one
            // wheel notch out over several frames, and zooming (re-centred on the pointer) once per frame made
            // that single notch drift and bounce. Each notch is now applied exactly once.
            let (wheel_zoom, pad_pan, pinch) = ui.input(|i| {
                let mut wheel_zoom = 0.0_f32;
                let mut pad_pan = Vec2::ZERO;
                for e in &i.events {
                    if let egui::Event::MouseWheel { unit, delta, modifiers, .. } = e {
                        match unit {
                            // classic wheel: zoom, or pan sideways with Shift held
                            egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => {
                                if modifiers.shift {
                                    pad_pan.x += (delta.x + delta.y) * 40.0;
                                } else {
                                    wheel_zoom += delta.y;
                                }
                            }
                            // trackpad / Magic Mouse: two-finger scroll pans
                            egui::MouseWheelUnit::Point => pad_pan += *delta,
                        }
                    }
                }
                (wheel_zoom, pad_pan, i.zoom_delta())
            });
            if pad_pan != Vec2::ZERO {
                self.pan += pad_pan;
            }
            // a trackpad pinch, or the mouse wheel (clamped so no single event jumps the view)
            let mut zoom_by = 1.0;
            if pinch != 1.0 {
                zoom_by = pinch;
            } else if wheel_zoom != 0.0 {
                zoom_by = (1.0 + wheel_zoom * 0.1).clamp(0.5, 2.0);
            }
            if zoom_by != 1.0 {
                let old = self.zoom;
                let nz = (old * zoom_by).clamp(0.25, 2.5);
                if let Some(p) = pointer {
                    let wp = ((p - rect.min) - self.pan) / old;
                    self.pan = (p - rect.min) - wp * nz;
                }
                self.zoom = nz;
            }
        }
        let pan_mode = mods.alt;
        if resp.dragged_by(PointerButton::Middle) || (resp.dragged_by(PointerButton::Primary) && pan_mode && (self.connecting.is_none() && self.connecting_in.is_none())) {
            self.pan += resp.drag_delta();
        }
        if resp.drag_started_by(PointerButton::Primary) && !pan_mode && (self.connecting.is_none() && self.connecting_in.is_none()) {
            self.rubber = pointer;
            if !mods.shift {
                self.sel.clear();
                self.sel_comment = None;
            }
        }
        if resp.clicked() && !mods.shift {
            self.sel.clear();
            self.sel_comment = None;
        }

        let z = self.zoom;
        let org = rect.min + self.pan;
        let ts = move |p: [f32; 2]| pos2(org.x + p[0] * z, org.y + p[1] * z);

        // grid
        let minor = Stroke::new(1.0_f32, Color32::from_rgb(63, 63, 63));
        let major = Stroke::new(1.0_f32, Color32::from_rgb(50, 50, 50));
        let step = if self.snap_nodes && self.snap_grid >= 1.0 { self.snap_grid * z } else { 32.0 * z };
        if step > 6.0 {
            let (ox, oy) = (self.pan.x.rem_euclid(step), self.pan.y.rem_euclid(step));
            let mut x = rect.min.x + ox;
            let mut k = ((rect.min.x + ox - org.x) / step).round() as i64;
            while x < rect.max.x {
                painter.line_segment([pos2(x, rect.min.y), pos2(x, rect.max.y)], if k.rem_euclid(4) == 0 { major } else { minor });
                x += step;
                k += 1;
            }
            let mut y = rect.min.y + oy;
            let mut k = ((rect.min.y + oy - org.y) / step).round() as i64;
            while y < rect.max.y {
                painter.line_segment([pos2(rect.min.x, y), pos2(rect.max.x, y)], if k.rem_euclid(4) == 0 { major } else { minor });
                y += step;
                k += 1;
            }
        }

        // comment frames (drawn under nodes)
        for ci in 0..self.g.comments.len() {
            let c = self.g.comments[ci].clone();
            let r = Rect::from_min_size(ts(c.pos), vec2(c.size[0], c.size[1]) * z);
            let col = COMMENT_COLORS[c.color as usize % COMMENT_COLORS.len()];
            let base = Color32::from_rgb(col[0], col[1], col[2]);
            let selected = self.sel_comment == Some(ci);
            painter.rect_filled(r, egui::CornerRadius::same((4.0 * z) as u8), base.gamma_multiply(0.16));
            let title = Rect::from_min_size(r.min, vec2(r.width(), 22.0 * z));
            painter.rect_filled(title, egui::CornerRadius { nw: (4.0 * z) as u8, ne: (4.0 * z) as u8, sw: (0.0) as u8, se: (0.0) as u8 }, base.gamma_multiply(0.5));
            painter.rect_stroke(r, egui::CornerRadius::same((4.0 * z) as u8), Stroke::new(if selected { 2.0_f32 } else { 1.0_f32 }, if selected { Color32::from_rgb(60, 200, 170) } else { base.gamma_multiply(0.8) }), egui::StrokeKind::Inside);
            painter.text(title.left_center() + vec2(8.0 * z, 0.0), Align2::LEFT_CENTER, &c.text, FontId::proportional(13.0 * z), Color32::WHITE);
            let tr = ui.interact(title, Id::new(("cmt", ci)), Sense::click_and_drag());
            if tr.drag_started() {
                self.snap();
                let cr = Rect::from_min_size(pos2(c.pos[0], c.pos[1]), vec2(c.size[0], c.size[1]));
                self.comment_members = self.g.nodes.iter().filter(|(_, n)| cr.contains(pos2(n.pos[0] + CARD * 0.5, n.pos[1] + CARD_H * 0.5))).map(|(i, _)| *i).collect();
                self.sel_comment = Some(ci);
                self.sel.clear();
            }
            if tr.clicked() {
                self.sel_comment = Some(ci);
                self.sel.clear();
            }
            if tr.dragged() {
                let d = tr.drag_delta() / z;
                self.g.comments[ci].pos[0] += d.x;
                self.g.comments[ci].pos[1] += d.y;
                for id in self.comment_members.clone() {
                    if let Some(n) = self.g.nodes.get_mut(&id) {
                        n.pos[0] += d.x;
                        n.pos[1] += d.y;
                    }
                }
            }
            let hs = vec2(14.0, 14.0) * z;
            let handle = Rect::from_min_size(r.max - hs, hs);
            painter.rect_filled(handle, egui::CornerRadius::same((2.0) as u8), base.gamma_multiply(0.7));
            let hr = ui.interact(handle, Id::new(("cmtr", ci)), Sense::drag());
            if hr.drag_started() {
                self.snap();
            }
            if hr.dragged() {
                let d = hr.drag_delta() / z;
                self.g.comments[ci].size[0] = (self.g.comments[ci].size[0] + d.x).max(80.0);
                self.g.comments[ci].size[1] = (self.g.comments[ci].size[1] + d.y).max(50.0);
            }
        }

        // snapshot of nodes for this frame
        let snaps: Vec<NodeSnap> = self
            .g
            .nodes
            .iter()
            .map(|(id, n)| NodeSnap {
                id: *id,
                kind: n.kind.clone(),
                // a graph node only has input sockets when that graph defines inputs
                slots: node_slots(n, &self.g.subgraphs),
                ins: n.ins.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                pos: n.pos,
                // the graph a node opens: a graph node's graph, an FX-Map's pattern graph
                graph_name: n.strs.get("#graph").or_else(|| n.strs.get("graph")).cloned().or_else(|| n.strs.get("#fx").and_then(|f| f.split(',').find(|x| !x.is_empty()).map(str::to_string))),
                dot: n.kind == "sbs:passthrough",
                outs: sd_outs(n, &self.g.subgraphs),
                in_out: n.in_out.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            })
            .collect();
        let body_of = |s: &NodeSnap| card_body(s.slots.len(), s.outs.len());
        let posmap: HashMap<u32, ([f32; 2], bool, f32)> = snaps.iter().map(|s| (s.id, (s.pos, s.dot, body_of(s)))).collect();
        let out_port = move |p: [f32; 2], dot: bool, body: f32| if dot { ts(p) + vec2(24.0 * z, 12.0 * z) } else { ts(p) + vec2(CARD * z, (BAR + body * 0.5) * z) };
        let outs_map: HashMap<u32, Vec<String>> = snaps.iter().filter(|s| !s.outs.is_empty()).map(|s| (s.id, s.outs.clone())).collect();
        let out_port_i = move |p: [f32; 2], i: usize, n: usize, body: f32| ts(p) + vec2(CARD * z, (BAR + body * (i as f32 + 1.0) / (n as f32 + 1.0)) * z);
        // where the link of output `name` of node `src` leaves its card
        let src_port = |src: u32, name: Option<&str>| -> Option<Pos2> {
            let sp = posmap.get(&src)?;
            Some(match outs_map.get(&src) {
                Some(o) => out_port_i(sp.0, name.and_then(|nm| o.iter().position(|x| x == nm)).unwrap_or(0), o.len(), sp.2),
                None => out_port(sp.0, sp.1, sp.2),
            })
        };
        let in_port = move |p: [f32; 2], i: usize, n: usize, dot: bool, body: f32| if dot { ts(p) + vec2(0.0, 12.0 * z) } else { ts(p) + vec2(0.0, (BAR + body * (i as f32 + 1.0) / (n as f32 + 1.0)) * z) };
        let link = |painter: &egui::Painter, a: Pos2, b: Pos2, col: Color32, w: f32| {
            // the handles follow the distance: close nodes get a short, tight wire; a wire that has to turn back gets a wider loop
            let (adx, ady) = ((b.x - a.x).abs(), (b.y - a.y).abs());
            let dx = if b.x >= a.x { (adx * 0.5).max(ady * 0.2).max(3.0 * z) } else { (adx * 0.25 + ady * 0.2 + 28.0 * z).min(120.0 * z) };
            let pts = [a, a + vec2(dx, 0.0), b - vec2(dx, 0.0), b];
            // the curve stays inside the hull of its control points: a wire whose hull is off screen is not drawn
            if !Rect::from_points(&pts).expand(4.0).intersects(rect) {
                return;
            }
            painter.add(CubicBezierShape::from_points_stroke(pts, false, Color32::TRANSPARENT, Stroke::new(w * z.max(0.7), col)));
        };

        // links
        for s in &snaps {
            let n = s.slots.len();
            for (slot, src) in &s.ins {
                if let (Some(i), Some(a)) = (s.slots.iter().position(|x| x.0 == slot), src_port(*src, s.in_out.iter().find(|(k, _)| k == slot).map(|(_, v)| v.as_str()))) {
                    let hot = self.sel.contains(&s.id) || self.sel.contains(src);
                    link(&painter, a, in_port(s.pos, i, n, s.dot, body_of(s)), if hot { Color32::from_rgb(90, 215, 190) } else { Color32::from_rgb(185, 185, 185) }, 1.8);
                }
            }
        }

        // pin type of every node's output: grayscale sockets are drawn gray, colour sockets blue
        let pins = materialcraft::graph::pin_types(&self.g);
        let pin_fill = |p: materialcraft::graph::Pin| if p == materialcraft::graph::Pin::Color { Color32::from_rgb(110, 190, 255) } else { Color32::from_rgb(225, 225, 225) };
        let out_names: Vec<&'static str> = OUTPUT_NAMES.to_vec();
        let mut ports: Vec<(u32, &'static str, Pos2)> = Vec::new();
        let mut out_ports: Vec<(u32, Option<String>, Pos2)> = Vec::new();
        // sockets that have a link attached (drawn filled orange)
        let used_ports: HashSet<(u32, String)> = snaps
            .iter()
            .flat_map(|s| s.ins.iter().map(move |(slot, src)| (*src, s.in_out.iter().find(|(k, _)| k == slot).map(|(_, v)| v.clone()))))
            .filter_map(|(src, name)| outs_map.get(&src).map(|o| (src, name.unwrap_or_else(|| o[0].clone()))))
            .collect();
        let mut cards: Vec<Rect> = Vec::new();

        for s in &snaps {
            let id = s.id;
            let card = Rect::from_min_size(ts(s.pos), if s.dot { vec2(24.0, 24.0) } else { vec2(CARD, BAR + body_of(s)) } * z);
            cards.push(card);
            // a card outside the view (with room for its socket labels and size caption) is neither drawn nor hit-tested;
            // a large graph then costs only what is on screen
            if !card.expand2(vec2(90.0, 30.0) * z.max(0.5)).intersects(rect) {
                continue;
            }
            let selected = self.sel.contains(&id);
            let err = self.node_err.get(&id).cloned();
            if s.dot {
                let sel_col = if selected { Color32::from_rgb(60, 200, 170) } else { Color32::from_gray(150) };
                painter.circle(card.center(), 9.0 * z, Color32::from_rgb(38, 38, 38), Stroke::new(if selected { 2.4_f32 } else { 1.3_f32 }, sel_col));
                painter.circle_filled(card.center(), 3.2 * z, Color32::from_gray(200));
            } else {
            painter.rect_filled(card.translate(vec2(2.0, 3.0) * z), egui::CornerRadius::same((3.0 * z) as u8), Color32::from_black_alpha(70));
            painter.rect_filled(card, egui::CornerRadius::same((3.0 * z) as u8), Color32::from_rgb(32, 32, 32));
            let bar = Rect::from_min_size(card.min, vec2(card.width(), BAR * z));
            painter.rect_filled(bar, egui::CornerRadius { nw: (3.0 * z) as u8, ne: (3.0 * z) as u8, sw: (0.0) as u8, se: (0.0) as u8 }, kind_color(&s.kind));
            let node_ref = self.g.nodes.get(&id);
            // an output node: what the file says it is used for
            let usage = node_ref.and_then(|n| n.strs.get("#output")).and_then(|u| self.g.sd.as_ref().and_then(|sd| sd.output_usage(u))).unwrap_or_default();
            let title = node_ref.and_then(|n| n.strs.get("title")).filter(|t| !t.is_empty()).cloned();
            let label = match (&s.graph_name, s.kind.as_str(), title) {
                (_, _, Some(t)) => t,
                _ => node_ref.map(|n| materialcraft::sbs_native::label(n, &self.g)).unwrap_or_default(),
            };
            // nodes the importer could not convert carry a note: an orange corner marks them
            if node_ref.map(|n| n.strs.contains_key("import_note")).unwrap_or(false) {
                let tri = [bar.right_top(), bar.right_top() + vec2(-10.0 * z, 0.0), bar.right_top() + vec2(0.0, 10.0 * z)];
                painter.add(egui::Shape::convex_polygon(tri.to_vec(), Color32::from_rgb(235, 120, 60), Stroke::NONE));
            }
            let fs = if label.len() > 16 { 8.0 } else if label.len() > 12 { 9.0 } else { 10.5 };
            painter.text(bar.center(), Align2::CENTER_CENTER, label, FontId::proportional(fs * z), Color32::WHITE);
            let th = Rect::from_min_size(card.min + vec2(3.0, BAR + 3.0 + (body_of(s) - CARD) * 0.5) * z, vec2(CARD - 6.0, CARD - 6.0) * z);
            match self.thumbs.get(&id) {
                Some(t) => {
                    painter.image(t.id(), th, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
                None => {
                    painter.rect_filled(th, 0.0, Color32::from_rgb(22, 22, 22));
                }
            }
            if err.is_some() {
                painter.rect_filled(th, 0.0, Color32::from_rgba_unmultiplied(170, 30, 30, 150));
                painter.text(th.center(), Align2::CENTER_CENTER, "!", FontId::proportional(30.0 * z), Color32::WHITE);
            }
            painter.rect_stroke(card, egui::CornerRadius::same((3.0 * z) as u8), Stroke::new(if selected { 2.4_f32 } else { 1.0_f32 }, if selected { Color32::from_rgb(60, 200, 170) } else { Color32::from_rgb(14, 14, 14) }), egui::StrokeKind::Inside);
            if self.active == Some(id) && !selected {
                painter.rect_stroke(card, egui::CornerRadius::same((3.0 * z) as u8), Stroke::new(1.5_f32, Color32::from_rgb(150, 150, 150)), egui::StrokeKind::Inside);
            }

            if s.kind == "sbs:output" {
                let bc = card.left_top() + vec2(3.0, -1.0) * z;
                painter.circle(bc, 10.5 * z, Color32::from_rgb(30, 30, 30), Stroke::new(1.5_f32, Color32::from_gray(205)));
                painter.circle_stroke(bc, 5.6 * z, Stroke::new(1.3_f32, Color32::from_gray(235)));
                let mut pts = Vec::new();
                for k in 0..=24 {
                    let a = k as f32 / 24.0 * std::f32::consts::TAU;
                    pts.push(bc + vec2(a.cos() * 5.6, a.sin() * 2.2) * z);
                }
                painter.add(egui::Shape::line(pts, Stroke::new(1.0_f32, Color32::from_gray(200))));
                painter.circle_filled(bc + vec2(-1.8, -1.8) * z, 1.4 * z, Color32::WHITE);
                if !usage.is_empty() {
                    painter.text(card.left_top() + vec2(16.0, -4.0) * z, Align2::LEFT_BOTTOM, format!("({usage})"), FontId::proportional(9.0 * z), Color32::from_gray(150));
                }
                let bits = if self.g.meta.get("output_format").map(|f| f == "16").unwrap_or(false) { 16 } else { 8 };
                painter.text(card.center_bottom() + vec2(0.0, 3.0) * z, Align2::CENTER_TOP, format!("{0}x{0} - C{1}", self.nominal_node_size(id), bits), FontId::proportional(10.5 * z), Color32::from_gray(185));
            }
            }

            // a node with its own Output Size shows its size under the card
            if !matches!(s.kind.as_str(), "sbs:output" | "sbs:passthrough") {
                // every node shows its resolution; teal when it differs from the graph's
                let ns = self.nominal_node_size(id);
                if z >= 0.35 {
                    let own = ns != self.g.size;
                    painter.text(card.center_bottom() + vec2(0.0, 3.0) * z, Align2::CENTER_TOP, format!("{}x{}", ns, (ns as f64 * self.g.height_for(self.g.size) as f64 / self.g.size.max(1) as f64).round() as usize), FontId::proportional(10.0 * z), if own { Color32::from_rgb(140, 230, 210) } else { Color32::from_gray(140) });
                }
            }

            // interaction: card
            let nr = ui.interact(card, Id::new(("node", id)), Sense::click_and_drag());
            if nr.drag_started() {
                self.snap();
                if !self.sel.contains(&id) {
                    if !(mods.shift || mods.command) {
                        self.sel.clear();
                    }
                    self.sel.insert(id);
                }
                self.sel_comment = None;
                if self.active != Some(id) {
                    self.active = Some(id);
                    self.view_pending = true;
                }
            }
            if nr.dragged() && (self.connecting.is_none() && self.connecting_in.is_none()) {
                let d = nr.drag_delta() / z;
                let drag_id = Id::new("node_drag_origin");
                if self.snap_nodes && self.snap_grid >= 1.0 {
                    // the pointer moves freely; the nodes land on the grid: every node's start position plus the
                    // total drag so far, rounded
                    let (mut total, origin): (Vec2, Vec<(u32, [f32; 2])>) = if nr.drag_started() {
                        (Vec2::ZERO, self.sel.iter().filter_map(|k| self.g.nodes.get(k).map(|n| (*k, n.pos))).collect())
                    } else {
                        ui.ctx().data(|dt| dt.get_temp(drag_id)).unwrap_or((Vec2::ZERO, Vec::new()))
                    };
                    total += d;
                    let gs = self.snap_grid;
                    for (k, o) in &origin {
                        if let Some(n) = self.g.nodes.get_mut(k) {
                            n.pos = [((o[0] + total.x) / gs).round() * gs, ((o[1] + total.y) / gs).round() * gs];
                        }
                    }
                    ui.ctx().data_mut(|dt| dt.insert_temp(drag_id, (total, origin)));
                } else {
                    for sid in self.sel.clone() {
                        if let Some(n) = self.g.nodes.get_mut(&sid) {
                            n.pos[0] += d.x;
                            n.pos[1] += d.y;
                        }
                    }
                }
            }
            if nr.clicked() {
                if mods.shift || mods.command {
                    if !self.sel.remove(&id) {
                        self.sel.insert(id);
                    }
                } else {
                    self.sel = [id].into_iter().collect();
                }
                self.sel_comment = None;
                if self.active != Some(id) {
                    self.active = Some(id);
                    self.view_pending = true;
                }
            }
            if nr.double_clicked() {
                if let (Some(g), "sbs:graph" | "sbs:fxmaps") = (&s.graph_name, s.kind.as_str()) {
                    acts.push(Act::EnterSub(g.clone()));
                }
            }
            if nr.secondary_clicked() && !self.sel.contains(&id) {
                self.sel = [id].into_iter().collect();
                self.active = Some(id);
                self.view_pending = true;
            }
            if let Some(e) = &err {
                nr.clone().on_hover_text(e);
            }
            let (kind, gname) = (s.kind.clone(), s.graph_name.clone());
            nr.context_menu(|ui| {
                if ui.button("Copy").clicked() {
                    acts.push(Act::Copy);
                    ui.close();
                }
                if ui.button("Duplicate Selection    Ctrl+D").clicked() {
                    acts.push(Act::Duplicate);
                    ui.close();
                }
                if ui.button("Duplicate Selection without Link    Ctrl+Shift+D").clicked() {
                    acts.push(Act::DuplicateNoLink);
                    ui.close();
                }
                if ui.button("Delete Selection    Del").clicked() {
                    acts.push(Act::Delete);
                    ui.close();
                }
                if ui.button("Delete and Relink    Backspace").clicked() {
                    acts.push(Act::DeleteRelink);
                    ui.close();
                }
                ui.separator();
                if ui.button("Select upstream nodes").clicked() {
                    acts.push(Act::SelectUpstream);
                    ui.close();
                }
                if ui.button("Select downstream nodes").clicked() {
                    acts.push(Act::SelectDownstream);
                    ui.close();
                }
                if ui.button("Disable selection    Shift+D").clicked() {
                    acts.push(Act::ToggleDisable);
                    ui.close();
                }
                ui.separator();
                if matches!(kind.as_str(), "sbs:graph" | "sbs:fxmaps") {
                    if let Some(g) = &gname {
                        let label = if kind == "sbs:fxmaps" { "Open pattern graph" } else { "Open graph" };
                        if ui.button(label).clicked() {
                            acts.push(Act::EnterSub(g.clone()));
                            ui.close();
                        }
                    }
                }
                ui.menu_button("Add output node", |ui| {
                    for name in &out_names {
                        if ui.button(usage_label(name)).clicked() {
                            acts.push(Act::NewOutput(name.to_string(), id));
                            ui.close();
                        }
                    }
                });
            });

            // ports
            if s.kind != "sbs:output" && s.outs.len() >= 2 {
                // a graph node: one socket per output, labelled to the right of the card
                for (oi, oname) in s.outs.iter().enumerate() {
                    let op = out_port_i(s.pos, oi, s.outs.len(), body_of(s));
                    out_ports.push((id, Some(oname.clone()), op));
                    let used = used_ports.contains(&(id, oname.clone()));
                    let opin = match self.g.nodes.get(&id) {
                        Some(nd) if materialcraft::graph::node_outputs(&nd.kind).contains(&oname.as_str()) => {
                            materialcraft::graph::output_pin(&nd.kind, Some(oname.as_str()), pins.get(&id) == Some(&materialcraft::graph::Pin::Color), nd)
                        }
                        _ => pins.get(&id).copied().unwrap_or(materialcraft::graph::Pin::Gray),
                    };
                    painter.circle(op, 4.8 * z, if used { Color32::from_rgb(70, 210, 180) } else { pin_fill(opin) }, Stroke::new(1.0_f32, Color32::BLACK));
                    if z >= 0.45 {
                        painter.text(op + vec2(9.0 * z, 0.0), Align2::LEFT_CENTER, usage_label(oname), FontId::proportional(10.5 * z.max(0.7)), Color32::from_gray(175));
                    }
                    let pr = ui.interact(Rect::from_center_size(op, Vec2::splat(14.0 * z)), Id::new(("out", id, oi)), Sense::drag());
                    if pr.drag_started() {
                        self.connecting = Some(id);
                        self.connecting_out = Some(oname.clone());
                        self.relink_all = mods.shift && !self.children_of(id, Some(oname.as_str())).is_empty();
                    }
                }
            } else if s.kind != "sbs:output" {
                let op = out_port(s.pos, s.dot, body_of(s));
                out_ports.push((id, None, op));
                painter.circle(op, if s.dot { 3.5 } else { 4.8 } * z, pin_fill(pins.get(&id).copied().unwrap_or(materialcraft::graph::Pin::Gray)), Stroke::new(1.0_f32, Color32::BLACK));
                let pr = ui.interact(Rect::from_center_size(op, Vec2::splat(14.0 * z)), Id::new(("out", id)), Sense::drag());
                if pr.drag_started() {
                    self.connecting = Some(id);
                    self.connecting_out = None;
                    self.relink_all = mods.shift && !self.children_of(id, None).is_empty();
                }
            }
            let n = s.slots.len();
            for (i, (slot, required)) in s.slots.iter().enumerate() {
                let ip = in_port(s.pos, i, n, s.dot, body_of(s));
                ports.push((id, *slot, ip));
                painter.circle(ip, 4.5 * z, if s.kind == "sbs:output" { Color32::from_rgb(70, 210, 180) } else if *required { Color32::from_rgb(210, 210, 210) } else { Color32::from_rgb(60, 60, 60) }, Stroke::new(1.2_f32, Color32::from_rgb(190, 190, 190)));
                let ir = ui.interact(Rect::from_center_size(ip, Vec2::splat(14.0 * z)), Id::new(("in", id, i)), Sense::drag());
                if ir.drag_started() {
                    let had = self.g.nodes.get(&id).and_then(|nd| nd.ins.get(*slot).copied());
                    if let Some(old) = had {
                        self.snap();
                        let mut old_out = None;
                        if let Some(nd) = self.g.nodes.get_mut(&id) {
                            nd.ins.remove(*slot);
                            old_out = nd.in_out.remove(*slot);
                        }
                        self.connecting = Some(old);
                        self.connecting_out = old_out;
                        self.dirty = true;
                    } else {
                        self.connecting_in = Some((id, slot.to_string()));
                    }
                }
                if ir.hovered() {
                    painter.text(ip + vec2(-8.0 * z, 0.0), Align2::RIGHT_CENTER, *slot, FontId::proportional(10.5 * z.max(0.8)), Color32::WHITE);
                }
            }
        }

        // while a link is being dragged, name the sockets of the node under the pointer (the plain hover label does
        // not fire during a drag): input names when dragging from an output, output names when dragging from an input
        if (self.connecting.is_some() && !self.relink_all) || self.connecting_in.is_some() {
            if let Some(pt) = pointer {
                let over: Option<u32> = snaps.iter().zip(&cards).find(|(_, c)| c.expand(18.0 * z).contains(pt)).map(|(sn, _)| sn.id);
                if let Some(nid) = over {
                    let font = FontId::proportional(11.0 * z.max(0.8));
                    if self.connecting.is_some() {
                        let mine: Vec<&(u32, &str, Pos2)> = ports.iter().filter(|(i, _, _)| *i == nid).collect();
                        let nearest = mine.iter().min_by(|a, b| (a.2 - pt).length().partial_cmp(&(b.2 - pt).length()).unwrap_or(std::cmp::Ordering::Equal)).map(|x| x.1);
                        for (_, slot, ip) in &mine {
                            let hot = nearest == Some(*slot) && (*ip - pt).length() < 24.0 * z;
                            let g = painter.layout_no_wrap(slot.to_string(), font.clone(), if hot { Color32::WHITE } else { Color32::from_gray(200) });
                            let r = Rect::from_min_size(*ip + vec2(-12.0 * z - g.size().x, -g.size().y / 2.0), g.size()).expand2(vec2(4.0, 1.0));
                            painter.rect(r, 3.0, Color32::from_rgba_unmultiplied(20, 20, 20, 235), Stroke::new(1.0_f32, if hot { Color32::from_rgb(110, 225, 200) } else { Color32::from_gray(80) }), egui::StrokeKind::Inside);
                            painter.galley(r.min + vec2(4.0, 1.0), g, Color32::WHITE);
                            if hot {
                                painter.circle_stroke(*ip, 8.0 * z, Stroke::new(1.5_f32, Color32::from_rgb(110, 225, 200)));
                            }
                        }
                    } else {
                        let mine: Vec<&(u32, Option<String>, Pos2)> = out_ports.iter().filter(|(i, _, _)| *i == nid).collect();
                        for (_, name, op) in &mine {
                            let hot = (*op - pt).length() < 24.0 * z;
                            let label = name.as_deref().map(usage_label).unwrap_or_else(|| "output".to_string());
                            let g = painter.layout_no_wrap(label, font.clone(), if hot { Color32::WHITE } else { Color32::from_gray(200) });
                            let r = Rect::from_min_size(*op + vec2(12.0 * z, -g.size().y / 2.0), g.size()).expand2(vec2(4.0, 1.0));
                            painter.rect(r, 3.0, Color32::from_rgba_unmultiplied(20, 20, 20, 235), Stroke::new(1.0_f32, if hot { Color32::from_rgb(110, 225, 200) } else { Color32::from_gray(80) }), egui::StrokeKind::Inside);
                            painter.galley(r.min + vec2(4.0, 1.0), g, Color32::WHITE);
                            if hot {
                                painter.circle_stroke(*op, 8.0 * z, Stroke::new(1.5_f32, Color32::from_rgb(110, 225, 200)));
                            }
                        }
                    }
                }
            }
        }

        // connection drag
        if let Some(src) = self.connecting {
            if let (Some(pt), Some(a)) = (pointer, src_port(src, self.connecting_out.as_deref())) {
                if self.relink_all {
                    // Shift-drag: every link of this output follows the pointer to its new parent
                    let kids = self.children_of(src, self.connecting_out.as_deref());
                    for (kid, slot) in &kids {
                        if let Some(ks) = snaps.iter().find(|x| x.id == *kid) {
                            if let Some(i) = ks.slots.iter().position(|x| x.0 == slot.as_str()) {
                                link(&painter, pt, in_port(ks.pos, i, ks.slots.len(), ks.dot, body_of(ks)), Color32::from_rgb(255, 190, 90), 2.0);
                            }
                        }
                    }
                    painter.circle_stroke(a, 8.0 * z, Stroke::new(1.5_f32, Color32::from_rgb(255, 190, 90)));
                } else {
                    link(&painter, a, pt, Color32::from_rgb(110, 225, 200), 2.0);
                }
            }
            if released && self.relink_all {
                if let Some(pt) = pointer {
                    // target: the nearest output socket, else the node card under the pointer
                    let target = out_ports.iter().filter(|(_, _, p)| (*p - pt).length() < 16.0 * z).min_by(|a, b| (a.2 - pt).length().partial_cmp(&(b.2 - pt).length()).unwrap_or(std::cmp::Ordering::Equal)).map(|(i, o, _)| (*i, o.clone()))
                        .or_else(|| snaps.iter().zip(&cards).find(|(sn, c)| c.contains(pt) && sn.kind != "sbs:output").map(|(sn, _)| (sn.id, None)));
                    if let Some((t, tout)) = target {
                        let o = self.connecting_out.clone();
                        self.relink_children(src, o, t, tout);
                    }
                }
                self.connecting = None;
                self.connecting_out = None;
                self.relink_all = false;
            } else if released {
                if let Some(pt) = pointer {
                    let near = ports.iter().filter(|(_, _, p)| (*p - pt).length() < 14.0 * z).min_by(|a, b| (a.2 - pt).length().partial_cmp(&(b.2 - pt).length()).unwrap_or(std::cmp::Ordering::Equal));
                    if let Some((dst, slot, _)) = near {
                        let (dst, slot) = (*dst, slot.to_string());
                        let o = self.connecting_out.clone();
                        self.try_connect_out(src, dst, &slot, o);
                    } else if rect.contains(pt) && !cards.iter().any(|c| c.contains(pt)) {
                        self.link_out = self.connecting_out.clone();
                        self.open_search(pt, Some(src));
                    }
                }
                self.connecting = None;
                self.connecting_out = None;
            }
        }

        // reverse connection: dragged out of an input port towards an output port
        if let Some((dst, slot)) = self.connecting_in.clone() {
            let target = ports.iter().find(|(i, s, _)| *i == dst && *s == slot.as_str()).map(|p| p.2);
            if let (Some(pt), Some(ip)) = (pointer, target) {
                link(&painter, pt, ip, Color32::from_rgb(110, 225, 200), 2.0);
            }
            if released {
                if let Some(pt) = pointer {
                    let near = out_ports.iter().filter(|(_, _, p)| (*p - pt).length() < 14.0 * z).min_by(|a, b| (a.2 - pt).length().partial_cmp(&(b.2 - pt).length()).unwrap_or(std::cmp::Ordering::Equal));
                    if let Some((src, oname, _)) = near {
                        let (src, oname) = (*src, oname.clone());
                        self.try_connect_out(src, dst, &slot, oname);
                    } else if rect.contains(pt) && !cards.iter().any(|c| c.contains(pt)) {
                        self.open_search_to(pt, dst, slot.clone());
                    }
                }
                self.connecting_in = None;
            }
        }

        // rubber-band selection
        if let Some(start) = self.rubber {
            if let Some(pt) = pointer {
                let r = Rect::from_two_pos(start, pt);
                painter.rect(r, 0.0, Color32::from_rgba_unmultiplied(60, 200, 170, 28), Stroke::new(1.0_f32, Color32::from_rgb(60, 200, 170)), egui::StrokeKind::Inside);
                if released {
                    for (s, c) in snaps.iter().zip(&cards) {
                        if r.intersects(*c) {
                            self.sel.insert(s.id);
                        }
                    }
                    if self.active.is_none() {
                        self.active = self.sel.iter().next().copied();
                        self.view_pending = true;
                    }
                    self.rubber = None;
                }
            } else if released {
                self.rubber = None;
            }
        }

        // background context menu
        let click_pos = pointer;
        let cats: BTreeMap<&'static str, Vec<String>> = {
            let mut m: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
            for e in self.entries() {
                if !matches!(e.kind.as_str(), "comment" | "frame") && !e.label.eq_ignore_ascii_case("dot") {
                    m.entry(e.cat).or_default().push(e.label);
                }
            }
            m
        };
        let has_sel = !self.sel.is_empty();
        let has_clip = !self.clip.is_empty();
        resp.context_menu(|ui| {
            ui.set_min_width(250.0);
            ui.menu_button("Add Node", |ui| {
                if ui.button("Search...   Tab / Space").clicked() {
                    acts.push(Act::Search(click_pos));
                    ui.close();
                }
                ui.separator();
                for (cat, labels) in &cats {
                    ui.menu_button(*cat, |ui| {
                        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                            for l in labels {
                                if ui.button(l).clicked() {
                                    if let Some(p) = click_pos {
                                        acts.push(Act::Drop(l.clone(), p));
                                    }
                                    ui.close();
                                }
                            }
                        });
                    });
                }
            });
            if ui.button("Add Comment    C").clicked() {
                acts.push(Act::Comment);
                ui.close();
            }
            if ui.button("Add Frame").clicked() {
                acts.push(Act::AddFrame(click_pos));
                ui.close();
            }
            if ui.button("Add Dot Node").clicked() {
                acts.push(Act::AddDot(click_pos));
                ui.close();
            }
            ui.separator();
            if ui.button("Compute Nodes Thumbnails").clicked() {
                acts.push(Act::ComputeThumbs);
                ui.close();
            }
            if ui.button("Clear Nodes Thumbnails    F5").clicked() {
                acts.push(Act::ClearThumbs);
                ui.close();
            }
            if ui.button("Save Package    Ctrl+S").clicked() {
                acts.push(Act::Save);
                ui.close();
            }
            ui.separator();
            if ui.add_enabled(has_clip, egui::Button::new("Paste    Ctrl+V")).clicked() {
                acts.push(Act::Paste(click_pos.map(|p| self.screen_to_world(p))));
                ui.close();
            }
            if ui.add_enabled(has_clip, egui::Button::new("Paste without Link    Ctrl+Shift+V")).clicked() {
                acts.push(Act::PasteNoLink(click_pos.map(|p| self.screen_to_world(p))));
                ui.close();
            }
            if ui.button("Select All    Ctrl+A").clicked() {
                acts.push(Act::SelectAll);
                ui.close();
            }
            ui.separator();
            if ui.add_enabled(has_sel, egui::Button::new("Copy Selection    Ctrl+C")).clicked() {
                acts.push(Act::Copy);
                ui.close();
            }
            if ui.add_enabled(has_sel, egui::Button::new("Delete Selection    Del")).clicked() {
                acts.push(Act::Delete);
                ui.close();
            }
            if ui.add_enabled(has_sel, egui::Button::new("Delete and Relink    Backspace")).clicked() {
                acts.push(Act::DeleteRelink);
                ui.close();
            }
            if ui.add_enabled(has_sel, egui::Button::new("Duplicate Selection    Ctrl+D")).clicked() {
                acts.push(Act::Duplicate);
                ui.close();
            }
            if ui.add_enabled(has_sel, egui::Button::new("Duplicate Selection without Link    Ctrl+Shift+D")).clicked() {
                acts.push(Act::DuplicateNoLink);
                ui.close();
            }
            ui.separator();
            if ui.add_enabled(has_sel, egui::Button::new("Select upstream nodes")).clicked() {
                acts.push(Act::SelectUpstream);
                ui.close();
            }
            if ui.add_enabled(has_sel, egui::Button::new("Select downstream nodes")).clicked() {
                acts.push(Act::SelectDownstream);
                ui.close();
            }
            if ui.add_enabled(has_sel, egui::Button::new("Disable selection    Shift+D")).clicked() {
                acts.push(Act::ToggleDisable);
                ui.close();
            }
            ui.separator();
            if ui.button("Frame All    F").clicked() {
                acts.push(Act::FrameAll);
                ui.close();
            }
            if ui.button("Auto-layout").clicked() {
                acts.push(Act::AutoLayout);
                ui.close();
            }
        });

        if self.g.nodes.is_empty() {
            painter.text(rect.center(), Align2::CENTER_CENTER, "Press Tab to add a node", FontId::proportional(16.0), Color32::from_gray(130));
        }
        self.acts.extend(acts);
    }

    // ------------------------------------------------------------ inspector
    // ------------------------------------------------------------ actions
    fn do_act(&mut self, a: Act) {
        match a {
            Act::Open => self.open_file(),
            Act::Save => self.save(),
            Act::SaveAs => self.save_as(),
            Act::Export => {
                // the graph on screen
                self.export.target = None;
                self.export.open = true;
            }
            Act::ExportMaterialX => {
                self.export.target = None;
                self.mtlx_ui.open = true;
            }
            Act::Bake => self.bake.open = true,
            Act::NewSpline => self.new_spline_node(),
            Act::Undo => self.undo(),
            Act::Redo => self.redo(),
            Act::Copy => self.copy(),
            Act::Cut => {
                self.copy();
                self.delete_selected();
            }
            Act::Paste(at) => self.paste(at),
            Act::Duplicate => {
                self.copy();
                self.paste(None);
            }
            Act::Delete => self.delete_selected(),
            Act::SelectAll => {
                self.sel = self.g.nodes.keys().copied().collect();
                self.active = self.sel.iter().next().copied();
                self.view_pending = true;
            }
            Act::Comment => self.add_comment(),
            Act::EnterSub(n) => {
                if self.parents.iter().any(|(_, name)| *name == n) {
                    return;
                }
                if !self.g.subgraphs.contains_key(&n) {
                    self.goto_root();
                }
                self.enter_sub(&n);
            }
            Act::ExitSub => self.exit_sub(),
            Act::AutoLayout => {
                self.snap();
                auto_layout(&mut self.g);
                self.fit_view();
            }
            Act::FrameAll => self.fit_view(),
            Act::QuickAdd(label) => {
                if let Some(e) = self.entries().into_iter().find(|e| e.label.eq_ignore_ascii_case(label)) {
                    let from = self.active.filter(|a| self.g.nodes.contains_key(a));
                    let at = match from.and_then(|a| self.g.nodes.get(&a)) {
                        Some(n) => pos2(n.pos[0] + CARD + 40.0, n.pos[1]),
                        None => {
                            let c = self.view_center_world();
                            pos2(c.x - CARD * 0.5, c.y - CARD_H * 0.5)
                        }
                    };
                    self.spawn(&e, at, from);
                }
            }
            Act::PasteNoLink(p) => self.paste_ex(p, false),
            Act::DuplicateNoLink => {
                self.copy();
                self.paste_ex(None, false);
            }
            Act::DeleteRelink => self.delete_relink(),
            Act::SelectUpstream => self.select_reach(true),
            Act::SelectDownstream => self.select_reach(false),
            Act::ToggleDisable => self.toggle_disable(),
            Act::ComputeThumbs => {
                self.thumbs.clear();
                self.thumb_px.clear();
                self.dirty = true;
            }
            Act::ClearThumbs => {
                self.thumbs.clear();
                self.thumb_px.clear();
            }
            Act::AddDot(p) => {
                let p = p.unwrap_or_else(|| self.canvas_rect.center());
                let world = self.screen_to_world(p);
                if let Some(e) = self.entries().into_iter().find(|e| e.label.eq_ignore_ascii_case("dot")) {
                    self.spawn(&e, pos2(world.x - CARD * 0.5, world.y - CARD_H * 0.5), None);
                }
            }
            Act::AddFrame(p) => match p {
                Some(p) => {
                    let w = self.screen_to_world(p);
                    self.add_comment_at(Some(w));
                }
                None => self.add_comment(),
            },
            Act::Search(p) => {
                let p = p.unwrap_or_else(|| self.canvas_rect.center());
                self.open_search(p, None);
            }
            Act::NewGraph => {
                let dest = if self.screen == Screen::Editor { self.ws_init(); Some(self.pkg_sel) } else { None };
                self.open_new_dialog(dest);
            }
            Act::Home => self.screen = Screen::Home,
            Act::Drop(key, p) => {
                let world = self.screen_to_world(p);
                let w = pos2(world.x - CARD * 0.5, world.y - CARD_H * 0.5);
                if let Some(e) = self.entries().into_iter().find(|e| e.label == key || (e.kind == "sbs:graph" && key.strip_prefix("subgraph: ") == e.sub.as_deref())) {
                    self.spawn(&e, w, None);
                }
            }
            Act::ImportFromPkg(i, gn, p) => {
                let at = p.map(|p| {
                    let w = self.screen_to_world(p);
                    pos2(w.x - CARD * 0.5, w.y - CARD_H * 0.5)
                });
                self.import_from_pkg(i, gn, at);
            }
            Act::NewPackage => self.open_new_dialog(None),
            Act::NewGraphIn(i) => self.open_new_dialog(Some(i)),
            Act::NewFunctionIn(i) => {
                let mut made = None;
                self.graph_edit(i, |g| {
                    let mut name = "Function".to_string();
                    let mut k = 2;
                    while materialcraft::sbs_native::all_graphs(g).contains_key(&name) {
                        name = format!("Function_{k}");
                        k += 1;
                    }
                    materialcraft::sbs_native::create_graph(g, &name, "function")?;
                    made = Some(name.clone());
                    Ok(format!("Created function '{name}': add function nodes, then make one the result"))
                });
                if let Some(n) = made {
                    self.enter_sub(&n);
                }
            }
            Act::SwitchPkg(i) => self.switch_pkg(i),
            Act::OpenGraphTab(n) => self.open_graph_tab(&n),
            Act::CloseGraphTab(n) => self.close_graph_tab(&n),
            Act::ToggleDeps => self.show_deps = !self.show_deps,
            Act::SavePkg(i) => {
                self.switch_pkg(i);
                self.save();
            }
            Act::ClosePkg(i) => {
                if self.pkg_dirty(i) {
                    self.confirm_close = Some(i);
                } else {
                    self.close_pkg(i);
                }
            }
            Act::NewOutput(usage, from) => self.new_output(&usage, from),
            Act::NewFromTemplate(i) => self.new_from_template(i),
            Act::ExportMaps(i, name) => {
                self.export.target = Some((i, name));
                self.export.open = true;
            }
            Act::ExportGraphFile(i, name) => self.export_graph_file(i, name),
            Act::AddGraphFile(i) => self.add_graph_file(i),
            Act::RenameGraph(i, name) => self.rename_graph = Some((i, name.clone(), name)),
            Act::DeleteGraph(i, name) => self.graph_edit(i, move |g| g.remove_graph(&name).map(|_| format!("Removed graph '{name}'"))),
            Act::DuplicateGraph(i, name) => self.graph_edit(i, move |g| g.duplicate_graph(name.as_deref(), None).map(|k| format!("Copied as '{k}'"))),
        }
    }

    /// The Explorer's rename box.
    fn rename_graph_ui(&mut self, ctx: &egui::Context) {
        let Some((i, old, mut new)) = self.rename_graph.take() else { return };
        let (mut ok, mut cancel) = (false, false);
        egui::Window::new("Rename graph").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            ui.label(format!("New name for '{old}' (letters, digits and _); every node that uses it follows."));
            let r = ui.add(egui::TextEdit::singleline(&mut new).desired_width(260.0));
            r.request_focus();
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                ok = true;
            }
            ui.horizontal(|ui| {
                ok |= ui.button("Rename").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if ok {
            let (o, n) = (old.clone(), new.trim().to_string());
            self.graph_edit(i, move |g| g.rename_graph(&o, &n).map(|_| format!("Renamed '{o}' to '{n}'")));
            let n = new.trim().to_string();
            for t in self.graph_tabs.iter_mut() {
                if *t == old {
                    *t = n.clone();
                }
            }
        } else if !cancel {
            self.rename_graph = Some((i, old, new));
        }
    }

    fn search_ui(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.search.take() else { return };
        let q = st.text.to_lowercase();
        let rank = |c: &str| ["Utility", "Atomic generators", "Atomic filters", "Generators", "Filters", "Material", "Graphs"].iter().position(|x| *x == c).unwrap_or(9);
        let mut list: Vec<(u8, Entry)> = self
            .entries()
            .into_iter()
            .filter_map(|e| {
                let l = e.label.to_lowercase();
                let s = if q.is_empty() {
                    1
                } else if l.starts_with(&q) {
                    0
                } else if l.contains(&q) {
                    1
                } else if e.doc.to_lowercase().contains(&q) {
                    2
                } else {
                    return None;
                };
                Some((s, e))
            })
            .collect();
        list.sort_by(|a, b| (a.0, rank(a.1.cat), &a.1.label).cmp(&(b.0, rank(b.1.cat), &b.1.label)));
        let (down, up, enter, esc, pgdn, pgup) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
                i.consume_key(Modifiers::NONE, Key::PageDown),
                i.consume_key(Modifiers::NONE, Key::PageUp),
            )
        });
        let moved = down || up || pgdn || pgup;
        if down {
            st.idx += 1;
        }
        if pgdn {
            st.idx += 10;
        }
        if up {
            st.idx = st.idx.saturating_sub(1);
        }
        if pgup {
            st.idx = st.idx.saturating_sub(10);
        }
        st.idx = st.idx.min(list.len().saturating_sub(1));
        let mut chosen: Option<Entry> = None;
        let mut close = esc;
        let screen = ctx.content_rect();
        let pos = pos2(st.screen.x.min(screen.right() - 330.0).max(4.0), st.screen.y.min(screen.bottom() - 420.0).max(4.0));
        let max_h = (screen.bottom() - pos.y - 120.0).clamp(160.0, 520.0);
        let area = egui::Area::new(Id::new("node_search")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(310.0);
                let r = ui.add(egui::TextEdit::singleline(&mut st.text).hint_text("Search nodes - Enter to add").desired_width(f32::INFINITY));
                r.request_focus();
                if r.changed() {
                    st.idx = 0;
                }
                ui.separator();
                if list.is_empty() {
                    ui.small("no matching nodes");
                }
                egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
                    for (i, (_, e)) in list.iter().enumerate() {
                        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
                        if i == st.idx {
                            ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(60, 92, 150));
                        } else if resp.hovered() {
                            ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(58, 58, 58));
                        }
                        let ir = Rect::from_min_size(rect.min + vec2(3.0, 3.0), vec2(16.0, 16.0));
                        ui.painter().rect_filled(ir, 3.0, kind_color(&e.kind));
                        draw_icon(ui.painter(), ir.center(), entry_icon(&e.kind), Color32::WHITE.gamma_multiply(0.9));
                        ui.painter().text(rect.left_center() + vec2(26.0, 0.0), Align2::LEFT_CENTER, &e.label, FontId::proportional(12.5), Color32::from_gray(230));
                        ui.painter().text(rect.right_center() - vec2(6.0, 0.0), Align2::RIGHT_CENTER, e.cat, FontId::proportional(10.0), Color32::from_gray(130));
                        if resp.clicked() {
                            chosen = Some(e.clone());
                        }
                        if i == st.idx && moved {
                            resp.scroll_to_me(None);
                        }
                    }
                });
                ui.separator();
                match list.get(st.idx) {
                    Some((_, e)) => {
                        ui.small(&e.doc);
                    }
                    None => {}
                }
                ui.small(format!("{} nodes  -  scroll or use arrow keys", list.len()));
            });
        });
        if enter && chosen.is_none() {
            chosen = list.get(st.idx).map(|x| x.1.clone());
        }
        if !st.fresh {
            let clicked_outside = ctx.input(|i| i.pointer.any_click()) && ctx.input(|i| i.pointer.interact_pos()).map(|p| !area.response.rect.contains(p)).unwrap_or(false);
            if clicked_outside {
                close = true;
            }
        }
        st.fresh = false;
        if let Some(e) = chosen {
            let (w, l) = (st.world, st.link_from);
            let new_id = self.spawn(&e, w, l);
            if let (Some(id), Some((dst, slot))) = (new_id, st.link_to.clone()) {
                if e.kind != "sbs:output" {
                    self.try_connect(id, dst, &slot);
                }
            }
            close = true;
        }
        if !close {
            self.search = Some(st);
        }
    }

    fn windows(&mut self, ctx: &egui::Context) {
        if self.export.open {
            let mut open = true;
            let (gtitle, outputs) = self.export_outputs();
            // thumbnails are those of the graph on screen; another graph's outputs show without them
            let on_screen = self.export.target.as_ref().map(|(i, n)| *i == self.pkg_active && n.as_deref() == self.parents.last().map(|(_, n)| n.as_str())).unwrap_or(true);
            let fmt = self.export.fmt;
            let mut do_export = false;
            let mut close = false;
            egui::Window::new(egui::RichText::new("Export Outputs").strong()).open(&mut open).collapsible(false).resizable(false).default_width(540.0).show(ctx, |ui| {
                ui.set_width(540.0);
                ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("GRAPH").small().color(Color32::from_gray(140)));
                    ui.label(egui::RichText::new(&gtitle).size(15.0).strong());
                });
                ui.add_space(2.0);
                // ---- outputs
                export_section(ui, "OUTPUTS", |ui| {
                    if ui.small_button("None").clicked() {
                        self.export.skip = outputs.iter().map(|o| o.1.clone()).collect();
                    }
                    if ui.small_button("All").clicked() {
                        self.export.skip.clear();
                    }
                });
                if outputs.is_empty() {
                    ui.colored_label(Color32::from_rgb(255, 150, 120), "This graph has no Output nodes yet.");
                }
                egui::Frame::NONE.fill(Color32::from_gray(38)).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::ScrollArea::vertical().max_height(176.0).auto_shrink([false, true]).show(ui, |ui| {
                        for (usage, ident, nid) in &outputs {
                            let mut on = !self.export.skip.contains(ident);
                            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
                            ui.painter().rect_filled(row, 2.0, if on { Color32::from_gray(48) } else { Color32::from_gray(42) });
                            let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(row.shrink2(vec2(6.0, 4.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
                            if cui.checkbox(&mut on, "").changed() {
                                if on {
                                    self.export.skip.remove(ident);
                                } else {
                                    self.export.skip.insert(ident.clone());
                                }
                            }
                            let (tr, _) = cui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                            match self.thumbs.get(nid).filter(|_| on_screen) {
                                Some(t) => {
                                    cui.painter().image(t.id(), tr, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                                }
                                None => {
                                    cui.painter().rect_filled(tr, 2.0, Color32::from_gray(60));
                                }
                            }
                            cui.painter().rect_stroke(tr, 2.0, Stroke::new(1.0_f32, Color32::from_gray(25)), egui::StrokeKind::Outside);
                            cui.add_space(4.0);
                            cui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.label(egui::RichText::new(usage).color(if on { Color32::from_gray(230) } else { Color32::from_gray(140) }));
                                let eg = self.export_file_name(&gtitle, ident, usage, 0);
                                ui.label(egui::RichText::new(format!("{eg}.{}", fmt.ext())).small().color(Color32::from_gray(130)));
                            });
                        }
                    });
                });
                // ---- destination
                export_section(ui, "DESTINATION", |_| {});
                egui::Grid::new("exp_dest").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Folder");
                    ui.horizontal(|ui| {
                        let mut d = self.export.dir.as_ref().map(|d| d.display().to_string()).unwrap_or_default();
                        let w = ui.available_width() - 80.0;
                        if ui.add(egui::TextEdit::singleline(&mut d).desired_width(w).hint_text("Choose a folder...")).changed() {
                            self.export.dir = Some(PathBuf::from(d.trim())).filter(|p| !p.as_os_str().is_empty());
                        }
                        if ui.button("Browse...").clicked() {
                            if let Some(d) = rfd::FileDialog::new().pick_folder() {
                                self.export.dir = Some(d);
                            }
                        }
                    });
                    ui.end_row();
                    ui.label("File name").on_hover_text("$(graph), $(identifier), $(usage) and $(size) are filled in");
                    ui.add(egui::TextEdit::singleline(&mut self.export.pattern).desired_width(ui.available_width() - 8.0));
                    ui.end_row();
                });
                // ---- format
                export_section(ui, "FORMAT", |_| {});
                egui::Grid::new("exp_fmt_grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Format");
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("exp_fmt").width(110.0).selected_text(self.export.fmt.name()).show_ui(ui, |ui| {
                            for f in [Format::Png, Format::Jpeg, Format::Tiff, Format::Exr, Format::Hdr] {
                                ui.selectable_value(&mut self.export.fmt, f, f.name());
                            }
                        });
                        ui.add_space(12.0);
                        match self.export.fmt {
                            Format::Jpeg => {
                                ui.label("Quality");
                                ui.add(egui::Slider::new(&mut self.export.quality, 1..=100));
                            }
                            Format::Hdr => {
                                ui.label(egui::RichText::new("32-bit RGBE").color(Color32::from_gray(150)));
                            }
                            f => {
                                ui.label("Bit depth");
                                let mut depth = if f.has_float32() && self.export.depth32 { 32 } else if self.export.depth16 || f == Format::Exr { 16 } else { 8 };
                                let before = depth;
                                if f != Format::Exr {
                                    ui.selectable_value(&mut depth, 8, "8");
                                }
                                ui.selectable_value(&mut depth, 16, if f == Format::Exr { "16 half" } else { "16" });
                                if f.has_float32() {
                                    ui.selectable_value(&mut depth, 32, "32 float");
                                }
                                if depth != before {
                                    self.export.depth16 = depth == 16;
                                    self.export.depth32 = depth == 32;
                                }
                            }
                        }
                    });
                    ui.end_row();
                    ui.label("Colour space").on_hover_text("Applies to colour maps only (base colour, emissive, diffuse, specular). Data maps are never converted.");
                    egui::ComboBox::from_id_salt("exp_space").width(200.0).selected_text(materialcraft::colorspace::SPACES[self.export.space].1).show_ui(ui, |ui| {
                        for (i, s) in materialcraft::colorspace::SPACES.iter().enumerate() {
                            ui.selectable_value(&mut self.export.space, i, s.1);
                        }
                    });
                    ui.end_row();
                    ui.label("Resolution").on_hover_text("Several at once go to one sub-folder each");
                    ui.horizontal(|ui| {
                        for (i, s) in [256, 512, 1024, 2048, 4096, 8192].iter().enumerate() {
                            let on = self.export.sizes[i];
                            if ui.selectable_label(on, s.to_string()).clicked() {
                                self.export.sizes[i] = !on;
                            }
                        }
                    });
                    ui.end_row();
                });
                // ---- footer
                ui.add_space(6.0);
                ui.separator();
                ui.horizontal(|ui| {
                    let n_out = outputs.iter().filter(|o| !self.export.skip.contains(&o.1)).count();
                    let n_sz = self.export.sizes.iter().filter(|b| **b).count();
                    let files = n_out * n_sz;
                    let dest = self.export.dir.as_ref().map(|d| d.display().to_string()).unwrap_or_else(|| "no folder chosen".into());
                    ui.label(egui::RichText::new(format!("{files} file{} to {dest}", if files == 1 { "" } else { "s" })).small().color(Color32::from_gray(150)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ready = files > 0 && self.export.dir.is_some();
                        let btn = egui::Button::new(egui::RichText::new("  Export  ").strong().color(Color32::BLACK)).fill(if ready { ACCENT } else { Color32::from_gray(90) });
                        if ui.add_enabled(ready, btn).clicked() {
                            do_export = true;
                        }
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                });
                let (msg, ok) = &self.status;
                if !msg.is_empty() && (msg.starts_with("Exported") || msg.starts_with("Export")) {
                    ui.label(egui::RichText::new(msg).small().color(if *ok { ACCENT } else { Color32::from_rgb(255, 150, 120) }));
                }
            });
            if do_export {
                self.run_export();
            }
            self.export.open = open && !close;
        }
        if self.mtlx_ui.open {
            let mut open = true;
            let (gtitle, _) = self.export_outputs();
            egui::Window::new("Export MaterialX").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                use materialcraft::mtlx::{NormalY, Shader};
                ui.horizontal(|ui| {
                    ui.label("Graph:");
                    ui.strong(&gtitle);
                });
                ui.small("A .mtlx file (MaterialX 1.39, read by Unreal Engine 5.8) with its textures in a sub-folder.");
                ui.horizontal(|ui| {
                    if ui.button("Choose folder...").clicked() {
                        if let Some(d) = rfd::FileDialog::new().pick_folder() {
                            self.mtlx_ui.dir = Some(d);
                        }
                    }
                    ui.label(self.mtlx_ui.dir.as_ref().map(|d| d.display().to_string()).unwrap_or_else(|| "(none)".into()));
                });
                ui.horizontal(|ui| {
                    ui.label("Material name");
                    ui.text_edit_singleline(&mut self.mtlx_ui.name).on_hover_text("Empty: the graph's name");
                });
                ui.horizontal(|ui| {
                    ui.label("Shader");
                    egui::ComboBox::from_id_salt("mtlx_shader").selected_text(if self.mtlx_ui.opt.shader == Shader::StandardSurface { "Standard Surface" } else { "OpenPBR Surface" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.mtlx_ui.opt.shader, Shader::StandardSurface, "Standard Surface (works with and without Substrate)");
                        ui.selectable_value(&mut self.mtlx_ui.opt.shader, Shader::OpenPbr, "OpenPBR Surface (Unreal with Substrate)");
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Normal map green channel");
                    egui::ComboBox::from_id_salt("mtlx_normal").selected_text(if self.mtlx_ui.opt.normal == NormalY::DirectX { "DirectX (down)" } else { "OpenGL (up)" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.mtlx_ui.opt.normal, NormalY::DirectX, "DirectX (down)");
                        ui.selectable_value(&mut self.mtlx_ui.opt.normal, NormalY::OpenGl, "OpenGL (up)");
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Resolution");
                    let names = ["Graph size", "256", "512", "1024", "2048", "4096", "8192"];
                    egui::ComboBox::from_id_salt("mtlx_size").selected_text(names[self.mtlx_ui.size.min(6)]).show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            ui.selectable_value(&mut self.mtlx_ui.size, i, *n);
                        }
                    });
                    let mut d16 = self.mtlx_ui.opt.depth == 16;
                    ui.checkbox(&mut d16, "16-bit");
                    self.mtlx_ui.opt.depth = if d16 { 16 } else { 8 };
                });
                ui.checkbox(&mut self.mtlx_ui.opt.displacement, "Height as displacement (Unreal shows it with Nanite)");
                if self.mtlx_ui.opt.displacement {
                    ui.add(egui::DragValue::new(&mut self.mtlx_ui.opt.displacement_scale).speed(0.01).range(0.0..=100.0).prefix("Displacement scale "));
                }
                ui.checkbox(&mut self.mtlx_ui.opt.bake_ao, "Multiply ambient occlusion into the base colour");
                ui.horizontal(|ui| {
                    ui.label("Texture repeat");
                    ui.add(egui::DragValue::new(&mut self.mtlx_ui.opt.tiling[0]).speed(0.05).range(0.01..=1000.0));
                    ui.add(egui::DragValue::new(&mut self.mtlx_ui.opt.tiling[1]).speed(0.05).range(0.01..=1000.0));
                });
                ui.add_space(4.0);
                if ui.button("Export").clicked() {
                    self.run_mtlx_export();
                }
            });
            self.mtlx_ui.open = open;
        }
        if self.bake.open {
            self.bake_window(ctx);
        }
        if self.show_deps {
            let mut open = true;
            let root: &Graph = self.parents.first().map(|p| &p.0).unwrap_or(&self.g);
            let deps = materialcraft::sbs_native::package_dependencies(root);
            egui::Window::new("Package dependencies").open(&mut open).collapsible(false).default_width(520.0).show(ctx, |ui| {
                ui.small("The packages this one uses: library graphs, and graphs of other .sbs files.");
                ui.separator();
                if deps.is_empty() {
                    ui.label("This package uses no other packages.");
                }
                for (file, uses) in &deps {
                    ui.horizontal(|ui| {
                        ui.strong(file);
                        ui.small(format!("{uses} node(s)"));
                    });
                }
            });
            self.show_deps = open;
        }
        if self.show_about {
            let mut open = true;
            egui::Window::new("Keyboard shortcuts").open(&mut open).collapsible(false).show(ctx, |ui| {
                for (k, d) in [
                    ("Tab", "add a node at the cursor (search)"),
                    ("Drag link to empty space", "add a node and auto-connect"),
                    ("Ctrl+Z / Ctrl+Y", "undo / redo"),
                    ("Ctrl+C / X / V / D", "copy / cut / paste / duplicate"),
                    ("Ctrl+A", "select all"),
                    ("Shift/Ctrl + click", "toggle selection"),
                    ("Drag background", "rubber-band select"),
                    ("Middle-drag, Alt+drag, Space+drag", "pan"),
                    ("Wheel / pinch", "zoom (Shift+wheel pans)"),
                    ("Two-finger drag (trackpad / Magic Mouse)", "pan"),
                    ("F", "frame all"),
                    ("C", "comment frame around selection"),
                    ("Delete", "delete selection"),
                    ("Double-click a graph node", "open its graph"),
                    ("Ctrl+S / Ctrl+O", "save / open"),
                ] {
                    ui.horizontal(|ui| {
                        ui.strong(k);
                        ui.label(d);
                    });
                }
            });
            self.show_about = open;
        }
    }
}


/// A section header of the export window, with room for small buttons at the right.
fn export_section(ui: &mut egui::Ui, title: &str, right: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(4.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    ui.painter().rect_filled(r, 2.0, Color32::from_gray(30));
    ui.painter().text(r.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, title, FontId::proportional(12.0), Color32::from_gray(215));
    let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(6.0, 2.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
    right(&mut cui);
}
