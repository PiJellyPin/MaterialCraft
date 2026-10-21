// SPDX-License-Identifier: MIT OR Apache-2.0
// ======================================================================= .sbs import (included into gui.rs)
//
// The rendering report of an `.sbs` package: which nodes MaterialCraft renders with its own nodes approximately or not at
// all (the file keeps them as they are); "Show" jumps to the node.

use materialcraft::sbs_import::{Level, Report};

struct ImportView {
    report: Report,
    /// show notes of at least this level
    min: Level,
}

fn is_sbs(p: &std::path::Path) -> bool {
    p.extension().map(|e| e.eq_ignore_ascii_case("sbs")).unwrap_or(false)
}

impl App {
    fn import_window(&mut self, ctx: &egui::Context) {
        let Some(v) = &mut self.import_view else { return };
        let mut open = true;
        let mut show: Option<(String, Option<u32>)> = None;
        let mut copy = false;
        egui::Window::new("Rendering report").open(&mut open).default_width(560.0).default_height(420.0).resizable(true).collapsible(false).show(ctx, |ui| {
            let r = &v.report;
            ui.label(egui::RichText::new(&r.source).strong());
            ui.label(r.summary());
            ui.horizontal(|ui| {
                for (lv, label, col) in [(Level::Info, format!("{} exact", r.exact), Color32::from_rgb(120, 200, 120)), (Level::Approx, format!("{} approximate", r.approx), Color32::from_rgb(230, 190, 90)), (Level::Missing, format!("{} not converted", r.missing), Color32::from_rgb(230, 110, 90))] {
                    let _ = lv;
                    ui.colored_label(col, label);
                }
            });
            ui.small("Approximate: MaterialCraft's own node or different units, check the result. Not converted: a marked placeholder keeps the graph connected.");
            ui.horizontal(|ui| {
                ui.label("Show:");
                ui.radio_value(&mut v.min, Level::Info, "everything");
                ui.radio_value(&mut v.min, Level::Approx, "approximate and missing");
                ui.radio_value(&mut v.min, Level::Missing, "missing only");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Copy report").clicked() {
                        copy = true;
                    }
                });
            });
            ui.separator();
            let mut notes: Vec<&materialcraft::sbs_import::Note> = r.notes.iter().filter(|n| n.level >= v.min).collect();
            notes.sort_by(|a, b| b.level.cmp(&a.level).then(a.graph.cmp(&b.graph)).then(a.node.cmp(&b.node)));
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                if notes.is_empty() {
                    ui.label("Nothing to report.");
                }
                for n in notes {
                    ui.horizontal_wrapped(|ui| {
                        let col = match n.level {
                            Level::Info => Color32::from_gray(150),
                            Level::Approx => Color32::from_rgb(230, 190, 90),
                            Level::Missing => Color32::from_rgb(230, 110, 90),
                        };
                        ui.colored_label(col, n.level.tag());
                        let at = match n.node {
                            Some(id) => format!("{} #{id}", n.graph),
                            None => n.graph.clone(),
                        };
                        if !at.is_empty() {
                            ui.label(egui::RichText::new(at).monospace().size(11.0));
                        }
                        ui.label(&n.text);
                        if n.node.is_some() && ui.small_button("Show").clicked() {
                            show = Some((n.graph.clone(), n.node));
                        }
                    });
                }
            });
        });
        if copy {
            if let Some(v) = &self.import_view {
                ctx.copy_text(v.report.to_text());
            }
            self.status("Import report copied", true);
        }
        if !open {
            self.import_view = None;
        }
        if let Some((graph, node)) = show {
            let root = self.gname();
            self.goto_root();
            if graph != root && self.g.subgraphs.contains_key(&graph) {
                self.enter_sub(&graph);
            }
            if let Some(id) = node.filter(|id| self.g.nodes.contains_key(id)) {
                self.sel.clear();
                self.sel.insert(id);
                self.active = Some(id);
                // centre the node at a readable zoom
                let pos = self.g.nodes[&id].pos;
                let c = self.canvas_rect;
                self.zoom = 1.0;
                self.pan = vec2(c.width() * 0.5 - (pos[0] + CARD * 0.5), c.height() * 0.5 - (pos[1] + CARD_H * 0.5));
                self.need_fit = false;
            }
        }
    }
}
