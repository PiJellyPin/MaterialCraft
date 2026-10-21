// SPDX-License-Identifier: MIT OR Apache-2.0
// ======================================================================= Start screen (included into gui.rs)
//
// The launcher shares the editor's look: the same neutral greys and accent, square corners, hairline borders and
// small monospace capitals for labels. Tabs, the remove buttons and the widget colours come from craft-ui, the
// shared Craft control library the editor's docking already uses.

use craft_ui::buttons::{ButtonVisuals, IconButton, IconButtonStyle};
use craft_ui::theme::WidgetPalette;

const L_BG: Color32 = Color32::from_rgb(27, 27, 27);
const L_BAR: Color32 = Color32::from_rgb(33, 33, 33);
const L_SURF: Color32 = Color32::from_rgb(34, 34, 34);
const L_SURF_HOT: Color32 = Color32::from_rgb(43, 43, 43);
const L_FIELD: Color32 = Color32::from_rgb(20, 20, 20);
const L_LINE: Color32 = Color32::from_rgb(56, 56, 56);
const L_LINE_HOT: Color32 = Color32::from_rgb(100, 100, 100);
const L_TEXT: Color32 = Color32::from_rgb(232, 232, 232);
const L_DIM: Color32 = Color32::from_rgb(160, 160, 160);
const L_FAINT: Color32 = Color32::from_rgb(112, 112, 112);
/// Text on the light primary buttons and selected tabs.
const L_INK: Color32 = Color32::from_rgb(22, 22, 22);
const L_LIGHT: Color32 = Color32::from_rgb(226, 226, 226);
const L_WARN: Color32 = Color32::from_rgb(222, 128, 100);
const L_R: f32 = 2.0;
const L_GAP: f32 = 12.0;

/// One line of small monospace capitals; an optional count follows in a dimmer shade.
fn mono_job(text: &str, count: Option<usize>, size: f32, color: Color32) -> egui::text::LayoutJob {
    let fmt = |c: Color32| egui::TextFormat { font_id: FontId::monospace(size), color: c, extra_letter_spacing: (size * 0.12).round(), ..Default::default() };
    let mut job = egui::text::LayoutJob::default();
    job.append(&text.to_uppercase(), 0.0, fmt(color));
    if let Some(n) = count {
        job.append(&format!("  {n}"), 0.0, fmt(color.gamma_multiply(0.6)));
    }
    job.wrap.max_rows = 1;
    job
}

fn paint_mono(p: &egui::Painter, pos: Pos2, anchor: Align2, text: &str, size: f32, color: Color32) -> Rect {
    paint_mono_w(p, pos, anchor, text, size, color, f32::INFINITY)
}

/// Like [`paint_mono`], cut short with an ellipsis when it is wider than `max_w`.
fn paint_mono_w(p: &egui::Painter, pos: Pos2, anchor: Align2, text: &str, size: f32, color: Color32, max_w: f32) -> Rect {
    let mut job = mono_job(text, None, size, color);
    job.wrap.max_width = max_w;
    job.wrap.overflow_character = Some('…');
    let g = p.layout_job(job);
    let r = anchor.anchor_size(pos, g.size());
    p.galley(r.min, g, color);
    r
}

fn kicker(text: &str) -> egui::RichText {
    egui::RichText::new(text.to_uppercase()).monospace().size(10.5).extra_letter_spacing(1.3).color(L_FAINT)
}

/// The launcher's widget colours (text fields, check boxes, scroll bars), set with craft-ui's palette helper.
fn launcher_style(ui: &mut egui::Ui) {
    let v = &mut ui.style_mut().visuals;
    WidgetPalette { noninteractive: L_BG, inactive: L_SURF, hovered: L_SURF_HOT, active: Color32::from_rgb(52, 52, 52), open: L_SURF_HOT }.apply_to(&mut v.widgets, egui::CornerRadius::same(L_R as u8), Stroke::new(1.0_f32, L_TEXT));
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, L_DIM);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, L_LINE);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, L_LINE);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, L_LINE_HOT);
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    for w in [&mut v.widgets.hovered, &mut v.widgets.active] {
        w.expansion = 0.0;
    }
    v.extreme_bg_color = L_FIELD;
    v.text_edit_bg_color = Some(L_FIELD);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
}

/// A text button: light and filled when `primary`, otherwise a hairline outline.
fn l_button(ui: &mut egui::Ui, label: &str, primary: bool) -> egui::Response {
    let w = ui.painter().layout_job(mono_job(label, None, 11.0, L_TEXT)).size().x;
    let (rect, resp) = ui.allocate_exact_size(vec2(w + 30.0, 28.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label));
    let (hot, down) = (resp.hovered(), resp.is_pointer_button_down_on());
    let (fill, stroke, text) = if primary {
        (if down { Color32::from_gray(196) } else if hot { Color32::WHITE } else { L_LIGHT }, Stroke::NONE, L_INK)
    } else {
        (if hot { L_SURF_HOT } else { Color32::TRANSPARENT }, Stroke::new(1.0_f32, if hot { L_LINE_HOT } else { L_LINE }), L_TEXT)
    };
    let p = ui.painter();
    p.rect(rect, L_R, fill, stroke, egui::StrokeKind::Inside);
    paint_mono(p, rect.center(), Align2::CENTER_CENTER, label, 11.0, text);
    if resp.has_focus() {
        p.rect_stroke(rect.shrink(2.0), L_R, Stroke::new(1.0_f32, ACCENT), egui::StrokeKind::Inside);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Small capitals that act as a link.
fn l_link(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let g = ui.painter().layout_job(mono_job(label, None, 10.5, L_DIM));
    let (rect, resp) = ui.allocate_exact_size(g.size() + vec2(4.0, 8.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label));
    let col = if resp.hovered() { L_TEXT } else { L_DIM };
    paint_mono(ui.painter(), rect.center(), Align2::CENTER_CENTER, label, 10.5, col);
    if resp.has_focus() {
        ui.painter().rect_stroke(rect, L_R, Stroke::new(1.0_f32, ACCENT), egui::StrokeKind::Inside);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A row of craft-ui tabs. The selected tab is a light chip; `boxed` draws the row as one outlined segmented control.
/// Returns the index that was clicked.
fn l_tabs(ui: &mut egui::Ui, id: &str, labels: &[(&str, Option<usize>)], sel: usize, boxed: bool) -> Option<usize> {
    let size = 10.5;
    let h = if boxed { 28.0 } else { 30.0 };
    let widths: Vec<f32> = labels.iter().map(|(l, n)| ui.painter().layout_job(mono_job(l, *n, size, L_TEXT)).size().x + 26.0).collect();
    let (row, _) = ui.allocate_exact_size(vec2(widths.iter().sum(), h), Sense::hover());
    if boxed {
        ui.painter().rect_stroke(row, L_R, Stroke::new(1.0_f32, L_LINE), egui::StrokeKind::Inside);
    }
    let mut x = row.min.x;
    let mut clicked = None;
    for (i, (label, count)) in labels.iter().enumerate() {
        let r = Rect::from_min_size(pos2(x, row.min.y), vec2(widths[i], h));
        x += widths[i];
        let selected = i == sel;
        let resp = craft_ui::tabs::Tab::new(egui::Id::new(("launcher_tab", id, i)), label, selected).focus_stroke(Stroke::new(1.0_f32, ACCENT)).show_at(ui, r, |ui, resp| {
            let p = ui.painter();
            let hot = resp.hovered();
            let chip = if boxed { r.shrink(3.0) } else { r.shrink2(vec2(2.0, 3.0)) };
            if selected {
                p.rect_filled(chip, L_R, L_LIGHT);
            } else if hot {
                p.rect_filled(chip, L_R, L_SURF_HOT);
            }
            let col = if selected { L_INK } else if hot { L_TEXT } else { L_DIM };
            let g = p.layout_job(mono_job(label, *count, size, col));
            p.galley(Align2::CENTER_CENTER.anchor_size(r.center(), g.size()).min, g, col);
        });
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

fn l_search(ui: &mut egui::Ui, text: &mut String, hint: &str, width: f32) -> egui::Response {
    ui.add(egui::TextEdit::singleline(text).hint_text(egui::RichText::new(hint).color(L_FAINT)).desired_width(width).margin(egui::Margin::symmetric(8, 5)))
}

/// The little remove button on a recent file: faint until the card is under the pointer.
fn remove_style(card_hot: bool) -> IconButtonStyle {
    let normal = ButtonVisuals { fill: Color32::TRANSPARENT, stroke: Stroke::NONE, icon: if card_hot { L_DIM } else { L_FAINT } };
    IconButtonStyle {
        normal,
        hovered: ButtonVisuals { fill: Color32::from_rgb(62, 62, 62), stroke: Stroke::NONE, icon: Color32::WHITE },
        pressed: Some(ButtonVisuals { fill: Color32::from_rgb(78, 78, 78), stroke: Stroke::NONE, icon: Color32::WHITE }),
        selected: normal,
        corner_radius: egui::CornerRadius::same(L_R as u8),
        focus_stroke: Stroke::new(1.0_f32, ACCENT),
    }
}

fn paint_cross(p: &egui::Painter, r: Rect, col: Color32) {
    let r = Rect::from_center_size(r.center(), vec2(8.0, 8.0));
    let st = Stroke::new(1.4_f32, col);
    p.line_segment([r.left_top(), r.right_bottom()], st);
    p.line_segment([r.right_top(), r.left_bottom()], st);
}

/// Hairline along the bottom (or top) edge of a bar panel, across its full width.
fn bar_edge(ui: &egui::Ui, margin: f32, bottom: bool) {
    let full = ui.max_rect().expand2(vec2(margin, 0.0));
    let y = if bottom { full.bottom() - 0.5 } else { full.top() + 0.5 };
    ui.painter().with_clip_rect(full.expand(1.0)).hline(full.x_range(), y, Stroke::new(1.0_f32, L_LINE));
}

fn shortcut(key: char) -> String {
    if cfg!(target_os = "macos") {
        format!("CMD+{key}")
    } else {
        format!("CTRL+{key}")
    }
}

impl App {
    /// Start screen: a top bar with the pages, a centred column of content and a footer with the startup option.
    fn home_ui(&mut self, root: &mut egui::Ui) {
        let template_count = TEMPLATES.len();
        egui::Panel::top("home_top").exact_size(50.0).frame(egui::Frame::NONE.fill(L_BAR).inner_margin(egui::Margin::symmetric(18, 0))).show(root, |ui| {
            launcher_style(ui);
            bar_edge(ui, 18.0, true);
            ui.horizontal_centered(|ui| {
                if let Some(t) = &self.logo {
                    let sz = t.size_vec2();
                    let hgt = 26.0;
                    ui.add(egui::Image::new((t.id(), vec2(hgt * sz.x / sz.y.max(1.0), hgt))));
                }
                ui.add_space(4.0);
                let mut word = egui::text::LayoutJob::default();
                let f = |c: Color32| egui::TextFormat { font_id: FontId::proportional(16.5), color: c, ..Default::default() };
                word.append("Material", 0.0, f(L_TEXT));
                word.append("Craft", 0.0, f(ACCENT));
                ui.label(word);
                ui.add_space(26.0);
                if let Some(i) = l_tabs(ui, "nav", &[("Start", None), ("Templates", None), ("Learn", None)], self.home_page as usize, false) {
                    self.home_page = i as u8;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("V{}", env!("CARGO_PKG_VERSION"))).monospace().size(10.5).extra_letter_spacing(1.2).color(L_FAINT));
                    if self.ctl.is_some() {
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("AI CONTROL ON").monospace().size(10.0).extra_letter_spacing(1.2).color(ACCENT)).on_hover_text("An assistant can edit the open graph (Tools > AI control)");
                    }
                });
            });
        });
        egui::Panel::bottom("home_foot").exact_size(44.0).frame(egui::Frame::NONE.fill(L_BAR).inner_margin(egui::Margin::symmetric(18, 0))).show(root, |ui| {
            launcher_style(ui);
            bar_edge(ui, 18.0, false);
            ui.horizontal_centered(|ui| {
                if ui.checkbox(&mut self.skip_home, egui::RichText::new("Skip this screen on startup").size(12.0).color(L_DIM)).changed() {
                    self.save_settings();
                }
                ui.add_space(16.0);
                ui.label(egui::RichText::new(format!("{} RECENT  ·  {template_count} TEMPLATES", self.recent.len())).monospace().size(10.0).extra_letter_spacing(1.2).color(L_FAINT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if l_button(ui, "Go to editor", true).clicked() {
                        self.screen = Screen::Editor;
                    }
                    ui.add_space(10.0);
                    if l_link(ui, "Keyboard shortcuts").clicked() {
                        self.show_about = true;
                    }
                });
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(L_BG).inner_margin(egui::Margin::symmetric(24, 0))).show(root, |ui| {
            launcher_style(ui);
            egui::ScrollArea::vertical().id_salt("home_scroll").auto_shrink([false, false]).show(ui, |ui| {
                let full = ui.available_width();
                let col_w = full.min(1120.0);
                ui.add_space(34.0);
                ui.horizontal(|ui| {
                    ui.add_space(((full - col_w) * 0.5).max(0.0));
                    ui.vertical(|ui| {
                        ui.set_width(col_w);
                        match self.home_page {
                            1 => self.templates_page(ui, col_w, template_count),
                            2 => self.learn_page(ui),
                            _ => self.start_page(ui, col_w),
                        }
                    });
                });
                ui.add_space(30.0);
            });
        });
    }

    fn page_heading(ui: &mut egui::Ui, kick: &str, title: &str, sub: &str) {
        ui.label(kicker(kick));
        ui.add_space(6.0);
        ui.label(egui::RichText::new(title).size(27.0).color(L_TEXT));
        ui.add_space(2.0);
        ui.label(egui::RichText::new(sub).size(13.0).color(L_DIM));
        ui.add_space(22.0);
    }

    /// Section title in small capitals with a count, and a search field on the right.
    fn section_bar(ui: &mut egui::Ui, title: &str, count: usize, search: &mut String, hint: &str) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(title.to_uppercase()).monospace().size(11.0).extra_letter_spacing(1.4).color(L_TEXT));
            ui.label(egui::RichText::new(count.to_string()).monospace().size(11.0).color(L_FAINT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                l_search(ui, search, hint, 240.0);
            });
        });
    }

    fn start_page(&mut self, ui: &mut egui::Ui, col_w: f32) {
        Self::page_heading(ui, "Get started", "What will you make today?", "Seamless materials, patterns and filters from nodes - every result tiles.");

        // ---- actions: one outlined strip divided into cells
        let actions: [(&str, &str, Ic, Option<char>); 3] = [
            ("New graph", "Start from an empty graph or a workflow template", Ic::New, Some('N')),
            ("Open package", "Load a Substance package (.sbs)", Ic::Open, Some('O')),
            ("Mesh map baker", "Bake normal, AO and curvature maps from meshes", Ic::Sphere, None),
        ];
        let per_row = if col_w >= 640.0 { 3 } else { 1 };
        let cw = col_w / per_row as f32;
        let ch = 96.0;
        let mut chosen: Option<usize> = None;
        for (ri, row) in actions.chunks(per_row).enumerate() {
            let (strip, _) = ui.allocate_exact_size(vec2(col_w, ch), Sense::hover());
            for (k, (title, sub, ic, key)) in row.iter().enumerate() {
                let cell = Rect::from_min_size(pos2(strip.min.x + cw * k as f32, strip.min.y), vec2(cw, ch));
                let resp = ui.interact(cell, egui::Id::new(("home_action", ri, k)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, *title));
                let hot = resp.hovered();
                let p = ui.painter();
                p.rect_filled(cell, 0.0, if hot { L_SURF_HOT } else { L_SURF });
                if hot {
                    p.rect_filled(Rect::from_min_size(cell.min, vec2(cell.width(), 2.0)), 0.0, ACCENT);
                }
                if k > 0 {
                    p.vline(cell.min.x, cell.y_range(), Stroke::new(1.0_f32, L_LINE));
                }
                let tile = Rect::from_min_size(cell.min + vec2(18.0, 20.0), vec2(38.0, 38.0));
                p.rect(tile, L_R, ACCENT.gamma_multiply(if hot { 0.22 } else { 0.12 }), Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.55)), egui::StrokeKind::Inside);
                draw_icon(p, tile.center(), *ic, ACCENT);
                let clip = p.with_clip_rect(cell.shrink(1.0));
                clip.text(cell.min + vec2(72.0, 21.0), Align2::LEFT_TOP, *title, FontId::proportional(15.0), L_TEXT);
                clip.text(cell.min + vec2(72.0, 44.0), Align2::LEFT_TOP, *sub, FontId::proportional(11.5), L_DIM);
                if let Some(c) = key {
                    paint_mono(p, cell.right_bottom() + vec2(-14.0, -12.0), Align2::RIGHT_BOTTOM, &shortcut(*c), 9.5, L_FAINT);
                }
                if resp.has_focus() {
                    p.rect_stroke(cell.shrink(2.0), L_R, Stroke::new(1.0_f32, ACCENT), egui::StrokeKind::Inside);
                }
                if resp.clicked() {
                    chosen = Some(ri * per_row + k);
                }
            }
            ui.painter().rect_stroke(strip, L_R, Stroke::new(1.0_f32, L_LINE), egui::StrokeKind::Inside);
        }
        match chosen {
            Some(0) => self.new_dlg.open = true,
            Some(1) => self.open_file(),
            Some(2) => {
                self.screen = Screen::Editor;
                self.bake.open = true;
            }
            _ => {}
        }
        ui.add_space(L_GAP);

        // ---- tip strip
        if !self.hide_suggestions {
            egui::Frame::NONE.fill(L_SURF).stroke(Stroke::new(1.0_f32, L_LINE)).corner_radius(L_R).inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
                ui.set_width(col_w - 22.0);
                ui.horizontal(|ui| {
                    egui::Frame::NONE.fill(ACCENT).corner_radius(L_R).inner_margin(egui::Margin::symmetric(6, 1)).show(ui, |ui| {
                        ui.label(egui::RichText::new("TIP").monospace().size(10.0).extra_letter_spacing(1.2).color(L_INK));
                    });
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Press Tab in the graph to add a node at the cursor, or drag a link into empty space to add one and connect it.").size(12.0).color(L_DIM));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if l_link(ui, "Hide").clicked() {
                            self.hide_suggestions = true;
                            self.save_settings();
                        }
                    });
                });
            });
        } else if l_link(ui, "Show tip").clicked() {
            self.hide_suggestions = false;
            self.save_settings();
        }
        ui.add_space(34.0);

        // ---- recent files
        let count = self.recent.len();
        Self::section_bar(ui, "Recent files", count, &mut self.home_filter, "Search recent files");
        ui.add_space(10.0);
        let f = self.home_filter.to_lowercase();
        let mut open: Option<PathBuf> = None;
        let mut remove: Option<PathBuf> = None;
        let files: Vec<PathBuf> = self.recent.iter().filter(|p| f.is_empty() || p.file_name().map(|s| s.to_string_lossy().to_lowercase().contains(&f)).unwrap_or(false)).cloned().collect();
        if files.is_empty() {
            let (r, _) = ui.allocate_exact_size(vec2(col_w, 78.0), Sense::hover());
            let p = ui.painter();
            p.rect_stroke(r, L_R, Stroke::new(1.0_f32, L_LINE), egui::StrokeKind::Inside);
            let msg = if self.recent.is_empty() { "Nothing here yet - create a graph or open a package and it will show up here." } else { "No recent files match the search." };
            p.text(r.center(), Align2::CENTER_CENTER, msg, FontId::proportional(12.5), L_FAINT);
        }
        let rcols = (((col_w + L_GAP) / (340.0 + L_GAP)).floor() as usize).clamp(1, 6);
        let rcw = (col_w - L_GAP * (rcols as f32 - 1.0)) / rcols as f32;
        for chunk in files.chunks(rcols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = L_GAP;
                for p in chunk {
                    let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    let folder = p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
                    let meta = std::fs::metadata(p).ok();
                    let alive = meta.is_some();
                    let (rect, resp) = ui.allocate_exact_size(vec2(rcw, 74.0), Sense::click());
                    let hot = ui.rect_contains_pointer(rect);
                    let pr = ui.painter();
                    pr.rect(rect, L_R, if hot && alive { L_SURF_HOT } else { L_SURF }, Stroke::new(1.0_f32, if hot && alive { L_LINE_HOT } else { L_LINE }), egui::StrokeKind::Inside);
                    let sw = Rect::from_min_size(rect.left_top() + vec2(11.0, 11.0), vec2(52.0, 52.0));
                    swatch(&pr.with_clip_rect(sw), sw, &name, alive);
                    let tx = rect.min.x + 76.0;
                    let clip = pr.with_clip_rect(Rect::from_min_max(pos2(tx, rect.min.y), pos2(rect.max.x - 34.0, rect.max.y)));
                    clip.text(pos2(tx, rect.min.y + 12.0), Align2::LEFT_TOP, &name, FontId::proportional(13.5), if alive { L_TEXT } else { L_FAINT });
                    clip.text(pos2(tx, rect.min.y + 32.0), Align2::LEFT_TOP, &folder, FontId::proportional(10.5), L_FAINT);
                    match &meta {
                        Some(m) => {
                            let line = format!("{}  ·  {}", m.modified().map(fmt_time).unwrap_or_default(), fmt_size(m.len()));
                            paint_mono(&clip, pos2(tx, rect.min.y + 50.0), Align2::LEFT_TOP, &line, 9.5, L_FAINT);
                        }
                        None => {
                            paint_mono(&clip, pos2(tx, rect.min.y + 50.0), Align2::LEFT_TOP, "File not found", 9.5, L_WARN);
                        }
                    }
                    // remove button in the corner (craft-ui icon button, above the card so it gets the click)
                    let xr = Rect::from_min_size(pos2(rect.max.x - 28.0, rect.min.y + 6.0), vec2(22.0, 22.0));
                    let style = remove_style(hot);
                    // a child Ui at a fixed spot: it does not move the row's layout cursor
                    let mut xui = ui.new_child(egui::UiBuilder::new().id_salt(("recent_remove", p)).max_rect(xr));
                    let x = IconButton::new("Remove from recent files", vec2(22.0, 22.0), &style).show(&mut xui, |ui, r, c| paint_cross(ui.painter(), r, c));
                    if x.on_hover_text("Remove from recent files (the file itself is kept)").clicked() {
                        remove = Some(p.clone());
                    }
                    let resp = resp.on_hover_text(p.display().to_string());
                    let resp = if alive { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp };
                    if resp.clicked() && alive {
                        open = Some(p.clone());
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Remove from recent files").clicked() {
                            remove = Some(p.clone());
                            ui.close();
                        }
                    });
                }
            });
            ui.add_space(L_GAP);
        }
        if let Some(p) = remove {
            self.recent.retain(|r| *r != p);
            let files: Vec<String> = self.recent.iter().map(|r| r.to_string_lossy().to_string()).collect();
            self.prefs.update(|v| v.recent.files = files);
        }
        if let Some(p) = open {
            self.open_path(p);
        }
    }

    fn templates_page(&mut self, ui: &mut egui::Ui, col_w: f32, total: usize) {
        Self::page_heading(ui, "Templates", "Start a new package", "Each template starts an .sbs package with the outputs of a workflow, ready for your nodes.");
        let _ = total;
        // square tiles that fill the column
        let cols = (((col_w + L_GAP) / (150.0 + L_GAP)).floor() as usize).max(2);
        let t = ((col_w - L_GAP * (cols as f32 - 1.0)) / cols as f32).floor();
        let mut chosen: Option<usize> = None;
        let cards: Vec<(usize, &Template)> = TEMPLATES.iter().enumerate().collect();
        for chunk in cards.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = L_GAP;
                for (i, tpl) in chunk {
                    let (cell, resp) = ui.allocate_exact_size(vec2(t, t + 42.0), Sense::click());
                    let hot = resp.hovered();
                    let tile = Rect::from_min_size(cell.min, vec2(t, t));
                    let p = ui.painter();
                    p.rect_filled(tile, L_R, L_SURF);
                    let img = tile.shrink(1.0);
                    // one band per output, in the colour it starts with
                    let n = tpl.outs.len();
                    if n == 0 {
                        swatch(&p.with_clip_rect(img), img, tpl.name, true);
                    } else {
                        for (k, (_, c)) in tpl.outs.iter().enumerate() {
                            let x0 = img.left() + img.width() * k as f32 / n as f32;
                            let x1 = img.left() + img.width() * (k + 1) as f32 / n as f32;
                            let to8 = |v: f32| (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8;
                            p.rect_filled(Rect::from_min_max(pos2(x0, img.top()), pos2(x1 + 0.5, img.bottom())), 0.0, Color32::from_rgb(to8(c[0]), to8(c[1]), to8(c[2])));
                        }
                    }
                    p.rect_stroke(tile, L_R, Stroke::new(1.0_f32, if hot { ACCENT } else { L_LINE }), egui::StrokeKind::Inside);
                    let clip = p.with_clip_rect(Rect::from_min_max(pos2(cell.min.x, tile.max.y), cell.max));
                    clip.text(pos2(cell.min.x + 1.0, tile.max.y + 8.0), Align2::LEFT_TOP, tpl.name, FontId::proportional(13.0), if hot { Color32::WHITE } else { L_TEXT });
                    paint_mono_w(&clip, pos2(cell.min.x + 1.0, tile.max.y + 27.0), Align2::LEFT_TOP, &format!("{} output(s)", n), 9.0, L_FAINT, t - 2.0);
                    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("{}\n{}\nClick to start a package from it", tpl.name, tpl.desc));
                    if resp.clicked() {
                        chosen = Some(*i);
                    }
                }
            });
            ui.add_space(L_GAP + 4.0);
        }
        if let Some(i) = chosen {
            self.new_from_template(i);
        }
        // ---- the shipped material templates: stylized PBR materials, each a complete package
        let mats = materialcraft::material_templates::MATERIALS;
        ui.label(egui::RichText::new(format!("MATERIAL TEMPLATES  {}", mats.len())).monospace().size(11.0).extra_letter_spacing(1.4).color(L_TEXT));
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Stylized PBR materials built from nodes. Click one to open an editable copy.").size(12.0).color(L_DIM));
        ui.add_space(12.0);
        let mut pick: Option<usize> = None;
        let cards: Vec<usize> = (0..mats.len()).collect();
        for chunk in cards.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = L_GAP;
                for &i in chunk {
                    let (ident, _, png) = mats[i];
                    let (cell, resp) = ui.allocate_exact_size(vec2(t, t + 42.0), Sense::click());
                    let hot = resp.hovered();
                    let tile = Rect::from_min_size(cell.min, vec2(t, t));
                    let id = egui::Id::new(("material_template_preview", i));
                    let tex: Option<egui::TextureHandle> = ui.ctx().data_mut(|d| d.get_temp(id));
                    let tex = match tex {
                        Some(x) => Some(x),
                        None => materialcraft::png::decode(png).ok().map(|img| {
                            let px: Vec<Color32> = img.px.iter().map(|c| {
                                let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                                Color32::from_rgb(to8(c[0]), to8(c[1]), to8(c[2]))
                            }).collect();
                            let h = ui.ctx().load_texture(format!("mt_{ident}"), egui::ColorImage { size: [img.w, img.h], source_size: vec2(img.w as f32, img.h as f32), pixels: px }, egui::TextureOptions::LINEAR);
                            ui.ctx().data_mut(|d| d.insert_temp(id, h.clone()));
                            h
                        }),
                    };
                    let p = ui.painter();
                    p.rect_filled(tile, L_R, L_SURF);
                    if let Some(h) = &tex {
                        p.image(h.id(), tile.shrink(1.0), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    p.rect_stroke(tile, L_R, Stroke::new(1.0_f32, if hot { ACCENT } else { L_LINE }), egui::StrokeKind::Inside);
                    let clip = p.with_clip_rect(Rect::from_min_max(pos2(cell.min.x, tile.max.y), cell.max));
                    clip.text(pos2(cell.min.x + 1.0, tile.max.y + 8.0), Align2::LEFT_TOP, materialcraft::material_templates::title(ident), FontId::proportional(13.0), if hot { Color32::WHITE } else { L_TEXT });
                    paint_mono_w(&clip, pos2(cell.min.x + 1.0, tile.max.y + 27.0), Align2::LEFT_TOP, "6 PBR outputs", 9.0, L_FAINT, t - 2.0);
                    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("{}\nClick to open an editable copy", materialcraft::material_templates::title(ident)));
                    if resp.clicked() {
                        pick = Some(i);
                    }
                }
            });
            ui.add_space(L_GAP + 4.0);
        }
        if let Some(i) = pick {
            self.new_from_material(i);
        }
    }

    fn learn_page(&mut self, ui: &mut egui::Ui) {
        Self::page_heading(ui, "Learn", "Getting started", "The few things worth knowing before your first graph.");
        let steps = [
            "Create a package with New graph and pick a template (Metallic Roughness gives you all PBR outputs). Packages are .sbs files.",
            "Press Tab over the graph to add a node at the cursor, or drag a link into empty space to add one and connect it.",
            "Drag nodes from the Library onto the graph. Right-click a library item to favorite it.",
            "A package holds several graphs and functions: the Explorer lists them. Drag a graph onto the canvas to use it as a node; double-click that node to open it.",
            "The 'fn' button next to a value computes it with a function graph; the 'P' button makes it a parameter of the graph.",
            "Tools > Mesh map baker bakes maps from OBJ, glTF, PLY and STL meshes into image files.",
            "Tools > AI control lets an assistant such as Claude build and edit the open package while you watch.",
        ];
        egui::Frame::NONE.stroke(Stroke::new(1.0_f32, L_LINE)).corner_radius(L_R).show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (i, t) in steps.iter().enumerate() {
                if i > 0 {
                    let r = ui.available_rect_before_wrap();
                    ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0_f32, L_LINE));
                }
                egui::Frame::NONE.inner_margin(egui::Margin::symmetric(16, 11)).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.label(egui::RichText::new(format!("{:02}", i + 1)).monospace().size(11.0).color(ACCENT));
                        ui.add_space(10.0);
                        ui.add(egui::Label::new(egui::RichText::new(*t).size(12.5).color(L_TEXT)).wrap());
                    });
                });
            }
        });
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            if l_button(ui, "New package", true).clicked() {
                self.new_from_template(1);
            }
            ui.add_space(8.0);
            if l_button(ui, "Keyboard shortcuts", false).clicked() {
                self.show_about = true;
            }
        });
    }
}
