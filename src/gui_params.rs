// SPDX-License-Identifier: MIT OR Apache-2.0
// ======================================================================= Node properties (included into gui.rs)

// ---------------------------------------------------------------- property panels
fn row_label(ui: &mut egui::Ui, text: &str) {
    ui.allocate_ui_with_layout(vec2(112.0, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.label(text);
    });
}

impl App {
    fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().slider_width = (ui.available_width() - 190.0).clamp(70.0, 220.0);
        if let Some(ci) = self.sel_comment.filter(|c| *c < self.g.comments.len()) {
            ui.strong("Comment frame");
            let mut c = self.g.comments[ci].clone();
            let mut changed = ui.text_edit_singleline(&mut c.text).changed();
            ui.horizontal(|ui| {
                for (i, col) in COMMENT_COLORS.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(22.0, 18.0), Sense::click());
                    ui.painter().rect_filled(r, egui::CornerRadius::same((3.0) as u8), Color32::from_rgb(col[0], col[1], col[2]));
                    if c.color as usize == i {
                        ui.painter().rect_stroke(r, egui::CornerRadius::same((3.0) as u8), Stroke::new(2.0_f32, Color32::WHITE), egui::StrokeKind::Inside);
                    }
                    if resp.clicked() {
                        c.color = i as u8;
                        changed = true;
                    }
                }
            });
            if changed {
                if !self.edit_open {
                    self.snap();
                    self.edit_open = true;
                }
                self.g.comments[ci] = c;
            }
            if ui.button("Delete frame").clicked() {
                self.acts.push(Act::Delete);
            }
            return;
        }
        if let Some(id) = self.active.filter(|a| self.sel.contains(a) && self.g.nodes.contains_key(a)) {
            self.node_props(ui, id);
            return;
        }
        if !self.sel.is_empty() {
            ui.label(format!("{} nodes selected", self.sel.len()));
            return;
        }
        self.graph_props(ui);
        self.functions_panel(ui);
    }

    /// Function graphs and pattern programs of this package: open or create them.
    /// Function graphs of this package: open or create them.
    fn functions_panel(&mut self, ui: &mut egui::Ui) {
        self.sd_functions_panel(ui);
    }

    fn node_props(&mut self, ui: &mut egui::Ui, id: u32) {
        self.sd_node_props(ui, id);
    }
}

