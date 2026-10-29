// SPDX-License-Identifier: MIT OR Apache-2.0
// ======================================================================= colour picker (included into gui.rs)
//
// A swatch that opens a picker with a saturation/value square and a hue bar, or a hue wheel with the square inside,
// whichever the user prefers (the choice is remembered), hex and numeric fields, alpha, and an eyedropper that takes a
// colour from anywhere inside the application window. Values are linear RGBA, shown as sRGB like Designer does.

use egui::epaint::{Hsva, Mesh, Vertex};

fn lin_to_srgb8(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

fn srgb8_to_lin(b: u8) -> f32 {
    let s = b as f32 / 255.0;
    if s <= 0.040_45 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
}

fn lin_to_color32(c: [f32; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(lin_to_srgb8(c[0]), lin_to_srgb8(c[1]), lin_to_srgb8(c[2]), (c[3].clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn hsva_to_lin(h: Hsva, alpha: f32) -> [f32; 4] {
    let [r, g, b] = h.to_srgb();
    [srgb8_to_lin(r), srgb8_to_lin(g), srgb8_to_lin(b), alpha]
}

fn lin_to_hsva(c: [f32; 4]) -> Hsva {
    Hsva::from_srgb([lin_to_srgb8(c[0]), lin_to_srgb8(c[1]), lin_to_srgb8(c[2])])
}

/// Colours picked by the eyedropper, waiting for the swatch that asked for them.
#[derive(Clone, Default)]
struct DropState {
    /// the swatch that wants a colour
    target: Option<Id>,
    /// the window picture, once the screenshot has arrived
    shot: Option<std::sync::Arc<egui::ColorImage>>,
    /// a colour picked, for `target`
    picked: Option<[u8; 3]>,
    /// the screenshot was requested
    asked: bool,
}

fn drop_state(ctx: &egui::Context) -> DropState {
    ctx.data(|d| d.get_temp::<DropState>(Id::new("eyedropper"))).unwrap_or_default()
}

fn set_drop_state(ctx: &egui::Context, s: DropState) {
    ctx.data_mut(|d| d.insert_temp(Id::new("eyedropper"), s));
}

/// Runs the eyedropper: call once per frame from the app. While a pick is under way it covers the window, shows a
/// magnifier at the pointer and hands the clicked pixel to the swatch that asked.
fn eyedropper_overlay(ctx: &egui::Context) {
    let mut st = drop_state(ctx);
    if st.target.is_none() || st.picked.is_some() {
        return;
    }
    if !st.asked {
        st.asked = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        set_drop_state(ctx, st);
        ctx.request_repaint();
        return;
    }
    if st.shot.is_none() {
        let got = ctx.input(|i| {
            i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None })
        });
        if got.is_some() {
            st.shot = got;
            set_drop_state(ctx, st);
        }
        ctx.request_repaint();
        return;
    }
    let Some(img) = st.shot.clone() else { return };
    let screen = ctx.content_rect();
    let ppp = ctx.pixels_per_point();
    let pos = ctx.input(|i| i.pointer.hover_pos());
    let clicked = ctx.input(|i| i.pointer.primary_released());
    let cancel = ctx.input(|i| i.key_pressed(Key::Escape));
    let px = |p: Pos2| -> [u8; 3] {
        let x = ((p.x * ppp) as usize).min(img.width().saturating_sub(1));
        let y = ((p.y * ppp) as usize).min(img.height().saturating_sub(1));
        let c = img.pixels[y * img.width() + x];
        [c.r(), c.g(), c.b()]
    };
    egui::Area::new(Id::new("eyedropper_area")).order(egui::Order::Tooltip).fixed_pos(screen.min).show(ctx, |ui| {
        let (_, _) = ui.allocate_exact_size(screen.size(), Sense::click());
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Crosshair);
        if let Some(p) = pos {
            let c = px(p);
            // magnifier: 9 x 9 pixels around the pointer, the centre one boxed
            let cell = 10.0;
            let org = p + vec2(18.0, 18.0);
            let org = pos2(org.x.min(screen.right() - 9.0 * cell - 8.0), org.y.min(screen.bottom() - 9.0 * cell - 40.0));
            let painter = ui.painter();
            painter.rect_filled(Rect::from_min_size(org - vec2(2.0, 2.0), vec2(9.0 * cell + 4.0, 9.0 * cell + 38.0)), 3.0, Color32::from_gray(25));
            for dy in -4i32..=4 {
                for dx in -4i32..=4 {
                    let q = pos2(p.x + dx as f32 / ppp, p.y + dy as f32 / ppp);
                    let c = px(q);
                    let r = Rect::from_min_size(org + vec2((dx + 4) as f32 * cell, (dy + 4) as f32 * cell), vec2(cell, cell));
                    painter.rect_filled(r, 0.0, Color32::from_rgb(c[0], c[1], c[2]));
                    if dx == 0 && dy == 0 {
                        painter.rect_stroke(r, 0.0, Stroke::new(1.5_f32, Color32::WHITE), egui::StrokeKind::Outside);
                    }
                }
            }
            painter.text(org + vec2(0.0, 9.0 * cell + 4.0), Align2::LEFT_TOP, format!("#{:02X}{:02X}{:02X}   Esc cancels", c[0], c[1], c[2]), FontId::monospace(12.0), Color32::WHITE);
        }
    });
    if clicked {
        if let Some(p) = pos {
            st.picked = Some(px(p));
        }
        // the swatch that asked reads `picked` next frame
        set_drop_state(ctx, DropState { target: st.target, shot: None, picked: st.picked, asked: false });
    } else if cancel {
        set_drop_state(ctx, DropState::default());
    }
    ctx.request_repaint();
}

fn hue_mesh(rect: Rect, vertical: bool) -> Mesh {
    let mut m = Mesh::default();
    let n = 6;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let c: Color32 = Hsva::new(t, 1.0, 1.0, 1.0).into();
        let (a, b) = if vertical {
            (pos2(rect.left(), rect.top() + t * rect.height()), pos2(rect.right(), rect.top() + t * rect.height()))
        } else {
            (pos2(rect.left() + t * rect.width(), rect.top()), pos2(rect.left() + t * rect.width(), rect.bottom()))
        };
        m.vertices.push(Vertex { pos: a, uv: egui::epaint::WHITE_UV, color: c });
        m.vertices.push(Vertex { pos: b, uv: egui::epaint::WHITE_UV, color: c });
        if i > 0 {
            let k = (i * 2) as u32;
            m.indices.extend_from_slice(&[k - 2, k - 1, k, k - 1, k + 1, k]);
        }
    }
    m
}

/// The saturation / value square: a grid of exact HSV colours (two triangles with corner colours only shade half of it).
fn sv_mesh(rect: Rect, hue: f32) -> Mesh {
    let mut m = Mesh::default();
    let n = 16usize;
    for j in 0..=n {
        let v = 1.0 - j as f32 / n as f32;
        for i in 0..=n {
            let sat = i as f32 / n as f32;
            let c: Color32 = Hsva::new(hue, sat, v, 1.0).into();
            m.vertices.push(Vertex { pos: pos2(rect.left() + rect.width() * sat, rect.top() + rect.height() * j as f32 / n as f32), uv: egui::epaint::WHITE_UV, color: c });
        }
    }
    let w = (n + 1) as u32;
    for j in 0..n as u32 {
        for i in 0..n as u32 {
            let k = j * w + i;
            m.indices.extend_from_slice(&[k, k + 1, k + w, k + 1, k + w + 1, k + w]);
        }
    }
    m
}

fn ring_mesh(centre: Pos2, r0: f32, r1: f32) -> Mesh {
    let mut m = Mesh::default();
    let n = 90;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let a = t * std::f32::consts::TAU;
        let c: Color32 = Hsva::new(t, 1.0, 1.0, 1.0).into();
        let d = vec2(a.cos(), a.sin());
        m.vertices.push(Vertex { pos: centre + d * r0, uv: egui::epaint::WHITE_UV, color: c });
        m.vertices.push(Vertex { pos: centre + d * r1, uv: egui::epaint::WHITE_UV, color: c });
        if i > 0 {
            let k = (i * 2) as u32;
            m.indices.extend_from_slice(&[k - 2, k - 1, k, k - 1, k + 1, k]);
        }
    }
    m
}

/// The picker's body. `c` is linear RGBA; returns true when it changed.
fn color_picker_body(ui: &mut egui::Ui, id: Id, c: &mut [f32; 4], alpha: bool) -> bool {
    let mut changed = false;
    let wheel_id = Id::new("color_picker_wheel");
    let mut wheel = ui.ctx().data_mut(|d| d.get_persisted::<bool>(wheel_id)).unwrap_or(false);
    // keep the hue and saturation of a grey colour between frames
    let hid = id.with("hsva");
    let mut hsva = ui.ctx().data(|d| d.get_temp::<(Hsva, [u8; 3])>(hid)).filter(|(_, rgb)| *rgb == [lin_to_srgb8(c[0]), lin_to_srgb8(c[1]), lin_to_srgb8(c[2])]).map(|(h, _)| h).unwrap_or_else(|| lin_to_hsva(*c));
    ui.horizontal(|ui| {
        if ui.selectable_label(!wheel, "Square").clicked() {
            wheel = false;
        }
        if ui.selectable_label(wheel, "Wheel").clicked() {
            wheel = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Eyedropper").on_hover_text("Click anywhere in the window to take that colour").clicked() {
                set_drop_state(ui.ctx(), DropState { target: Some(id), ..Default::default() });
            }
        });
    });
    ui.ctx().data_mut(|d| d.insert_persisted(wheel_id, wheel));
    let side = 200.0;
    if !wheel {
        let (r, resp) = ui.allocate_exact_size(vec2(side + 26.0, side), Sense::click_and_drag());
        let sq = Rect::from_min_size(r.min, vec2(side, side));
        let bar = Rect::from_min_size(pos2(sq.right() + 8.0, r.top()), vec2(18.0, side));
        ui.painter().add(egui::Shape::mesh(sv_mesh(sq, hsva.h)));
        ui.painter().add(egui::Shape::mesh(hue_mesh(bar, true)));
        if let Some(p) = resp.interact_pointer_pos() {
            if resp.is_pointer_button_down_on() {
                if p.x > sq.right() + 3.0 {
                    hsva.h = ((p.y - bar.top()) / bar.height()).clamp(0.0, 0.9999);
                } else {
                    hsva.s = ((p.x - sq.left()) / sq.width()).clamp(0.0, 1.0);
                    hsva.v = 1.0 - ((p.y - sq.top()) / sq.height()).clamp(0.0, 1.0);
                }
                changed = true;
            }
        }
        let m = pos2(sq.left() + hsva.s * sq.width(), sq.top() + (1.0 - hsva.v) * sq.height());
        ui.painter().circle_stroke(m, 5.0, Stroke::new(2.0_f32, Color32::WHITE));
        ui.painter().circle_stroke(m, 6.5, Stroke::new(1.0_f32, Color32::BLACK));
        let y = bar.top() + hsva.h * bar.height();
        ui.painter().rect_stroke(Rect::from_center_size(pos2(bar.center().x, y), vec2(22.0, 5.0)), 1.0, Stroke::new(1.5_f32, Color32::WHITE), egui::StrokeKind::Outside);
    } else {
        let (r, resp) = ui.allocate_exact_size(vec2(side, side), Sense::click_and_drag());
        let ctr = r.center();
        let (r1, r0) = (side * 0.5, side * 0.5 - 18.0);
        ui.painter().add(egui::Shape::mesh(ring_mesh(ctr, r0, r1)));
        let half = r0 * 0.70;
        let sq = Rect::from_center_size(ctr, Vec2::splat(half * 2.0));
        ui.painter().add(egui::Shape::mesh(sv_mesh(sq, hsva.h)));
        if resp.is_pointer_button_down_on() {
            // what the press landed on decides what the whole drag moves: the ring turns the hue, the square the rest
            let origin = ui.input(|i| i.pointer.press_origin()).or_else(|| resp.interact_pointer_pos());
            if let (Some(o), Some(p)) = (origin, resp.interact_pointer_pos()) {
                if (o - ctr).length() >= r0 - 3.0 {
                    let d = p - ctr;
                    hsva.h = (d.y.atan2(d.x) / std::f32::consts::TAU).rem_euclid(1.0).min(0.9999);
                } else {
                    hsva.s = ((p.x - sq.left()) / sq.width()).clamp(0.0, 1.0);
                    hsva.v = 1.0 - ((p.y - sq.top()) / sq.height()).clamp(0.0, 1.0);
                }
                changed = true;
            }
        }
        let a = hsva.h * std::f32::consts::TAU;
        let hp = ctr + vec2(a.cos(), a.sin()) * (r0 + r1) * 0.5;
        ui.painter().circle_stroke(hp, 6.0, Stroke::new(2.0_f32, Color32::WHITE));
        ui.painter().circle_stroke(hp, 7.5, Stroke::new(1.0_f32, Color32::BLACK));
        let m = pos2(sq.left() + hsva.s * sq.width(), sq.top() + (1.0 - hsva.v) * sq.height());
        ui.painter().circle_stroke(m, 5.0, Stroke::new(2.0_f32, Color32::WHITE));
        ui.painter().circle_stroke(m, 6.5, Stroke::new(1.0_f32, Color32::BLACK));
    }
    if changed {
        let a = c[3];
        *c = hsva_to_lin(hsva, a);
    }
    // numeric fields: sRGB bytes, as Designer shows them
    let mut rgb = [lin_to_srgb8(c[0]), lin_to_srgb8(c[1]), lin_to_srgb8(c[2])];
    let mut hex = format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
    ui.horizontal(|ui| {
        ui.label("Hex #");
        if ui.add(egui::TextEdit::singleline(&mut hex).desired_width(62.0)).changed() {
            let h = hex.trim().trim_start_matches('#');
            if h.len() == 6 {
                if let Ok(v) = u32::from_str_radix(h, 16) {
                    let b = [(v >> 16) as u8, (v >> 8) as u8, v as u8];
                    for i in 0..3 {
                        c[i] = srgb8_to_lin(b[i]);
                    }
                    hsva = lin_to_hsva(*c);
                    changed = true;
                }
            }
        }
    });
    ui.horizontal(|ui| {
        let mut ch = false;
        for (i, name) in ["R", "G", "B"].iter().enumerate() {
            ui.label(*name);
            let mut v = rgb[i] as u32;
            if ui.add(egui::DragValue::new(&mut v).range(0..=255).speed(0.5)).changed() {
                rgb[i] = v as u8;
                ch = true;
            }
        }
        if ch {
            for i in 0..3 {
                c[i] = srgb8_to_lin(rgb[i]);
            }
            hsva = lin_to_hsva(*c);
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        let (mut h, mut s, mut v) = (hsva.h * 360.0, hsva.s * 100.0, hsva.v * 100.0);
        let mut ch = false;
        ui.label("H");
        ch |= ui.add(egui::DragValue::new(&mut h).range(0.0..=359.9).speed(0.5).fixed_decimals(0)).changed();
        ui.label("S");
        ch |= ui.add(egui::DragValue::new(&mut s).range(0.0..=100.0).speed(0.3).fixed_decimals(0)).changed();
        ui.label("V");
        ch |= ui.add(egui::DragValue::new(&mut v).range(0.0..=100.0).speed(0.3).fixed_decimals(0)).changed();
        if ch {
            hsva.h = h / 360.0;
            hsva.s = s / 100.0;
            hsva.v = v / 100.0;
            let a = c[3];
            *c = hsva_to_lin(hsva, a);
            changed = true;
        }
    });
    if alpha {
        ui.horizontal(|ui| {
            ui.label("A");
            changed |= ui.add(egui::Slider::new(&mut c[3], 0.0..=1.0)).changed();
        });
    }
    let rgb_now = [lin_to_srgb8(c[0]), lin_to_srgb8(c[1]), lin_to_srgb8(c[2])];
    ui.ctx().data_mut(|d| d.insert_temp(hid, (hsva, rgb_now)));
    changed
}

fn is_open(ctx: &egui::Context, id: Id) -> bool {
    ctx.data(|d| d.get_temp::<bool>(id.with("open"))).unwrap_or(false)
}

fn set_open(ctx: &egui::Context, id: Id, on: bool) {
    ctx.data_mut(|d| d.insert_temp(id.with("open"), on));
}

/// A colour swatch with the picker behind it. `c` is linear RGBA.
fn color_field(ui: &mut egui::Ui, id_src: Id, c: &mut [f32; 4], alpha: bool) -> egui::Response {
    let id = id_src;
    let mut changed = false;
    // a colour taken by the eyedropper for this swatch
    let st = drop_state(ui.ctx());
    if st.target == Some(id) && st.picked.is_some() {
        if let Some(p) = st.picked {
            for i in 0..3 {
                c[i] = srgb8_to_lin(p[i]);
            }
            changed = true;
        }
        set_drop_state(ui.ctx(), DropState::default());
        // the picker was closed by the overlay; show it again with the new colour
        set_open(ui.ctx(), id, true);
    }
    let (rect, mut resp) = ui.allocate_exact_size(vec2(44.0, 18.0), Sense::click());
    let painter = ui.painter();
    // checkerboard under translucent colours
    if alpha && c[3] < 1.0 {
        let cell = 6.0;
        let mut y = rect.top();
        let mut row = 0;
        while y < rect.bottom() {
            let mut x = rect.left();
            let mut col = row;
            while x < rect.right() {
                let r = Rect::from_min_max(pos2(x, y), pos2((x + cell).min(rect.right()), (y + cell).min(rect.bottom())));
                painter.rect_filled(r, 0.0, if col % 2 == 0 { Color32::from_gray(200) } else { Color32::from_gray(120) });
                x += cell;
                col += 1;
            }
            y += cell;
            row += 1;
        }
    }
    painter.rect_filled(rect, 2.0, lin_to_color32(*c));
    painter.rect_stroke(rect, 2.0, Stroke::new(1.0_f32, Color32::from_gray(25)), egui::StrokeKind::Outside);
    let open = is_open(ui.ctx(), id);
    if resp.clicked() {
        set_open(ui.ctx(), id, !open);
    }
    let picking = drop_state(ui.ctx()).target == Some(id);
    if open && !picking {
        let area = egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(rect.left_bottom() + vec2(0.0, 4.0)).constrain(true).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                changed |= color_picker_body(ui, id, c, alpha);
            });
        });
        if ui.input(|i| i.pointer.any_pressed()) && !resp.clicked() {
            if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                if !area.response.rect.contains(p) && !rect.contains(p) {
                    set_open(ui.ctx(), id, false);
                }
            }
        }
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            set_open(ui.ctx(), id, false);
        }
    }
    if picking {
        // the picker closes while the eyedropper works
        set_open(ui.ctx(), id, false);
    }
    if changed {
        resp.mark_changed();
    }
    resp
}

/// A grey swatch with a picker behind it: a value ramp to drag on, the number as 0..255 and 0..1, and the eyedropper
/// (which takes the luminance of the picked colour). `g` is a linear value.
fn gray_field(ui: &mut egui::Ui, id: Id, g: &mut f32) -> egui::Response {
    let mut changed = false;
    let st = drop_state(ui.ctx());
    if st.target == Some(id) && st.picked.is_some() {
        if let Some(p) = st.picked {
            *g = 0.2126 * srgb8_to_lin(p[0]) + 0.7152 * srgb8_to_lin(p[1]) + 0.0722 * srgb8_to_lin(p[2]);
            changed = true;
        }
        set_drop_state(ui.ctx(), DropState::default());
        set_open(ui.ctx(), id, true);
    }
    let (rect, mut resp) = ui.allocate_exact_size(vec2(44.0, 18.0), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, Color32::from_gray(lin_to_srgb8(*g)));
    painter.rect_stroke(rect, 2.0, Stroke::new(1.0_f32, Color32::from_gray(25)), egui::StrokeKind::Outside);
    let open = is_open(ui.ctx(), id);
    if resp.clicked() {
        set_open(ui.ctx(), id, !open);
    }
    let picking = drop_state(ui.ctx()).target == Some(id);
    if open && !picking {
        let area = egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(rect.left_bottom() + vec2(0.0, 4.0)).constrain(true).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Grayscale");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Eyedropper").on_hover_text("Click anywhere in the window to take that colour's brightness").clicked() {
                            set_drop_state(ui.ctx(), DropState { target: Some(id), ..Default::default() });
                        }
                    });
                });
                // the ramp, black to white in display terms
                let (r, rr) = ui.allocate_exact_size(vec2(226.0, 28.0), Sense::click_and_drag());
                let n = 48;
                for i in 0..n {
                    let t0 = i as f32 / n as f32;
                    let t1 = (i + 1) as f32 / n as f32;
                    ui.painter().rect_filled(Rect::from_min_max(pos2(r.left() + r.width() * t0, r.top()), pos2(r.left() + r.width() * t1 + 0.5, r.bottom())), 0.0, Color32::from_gray((t0 * 255.0) as u8));
                }
                if rr.is_pointer_button_down_on() {
                    if let Some(p) = rr.interact_pointer_pos() {
                        let t = ((p.x - r.left()) / r.width()).clamp(0.0, 1.0);
                        *g = srgb8_to_lin((t * 255.0).round() as u8);
                        changed = true;
                    }
                }
                let x = r.left() + lin_to_srgb8(*g) as f32 / 255.0 * r.width();
                ui.painter().rect_stroke(Rect::from_center_size(pos2(x, r.center().y), vec2(5.0, r.height() + 4.0)), 1.0, Stroke::new(1.5_f32, Color32::WHITE), egui::StrokeKind::Outside);
                ui.painter().rect_stroke(Rect::from_center_size(pos2(x, r.center().y), vec2(7.0, r.height() + 6.0)), 1.0, Stroke::new(1.0_f32, Color32::BLACK), egui::StrokeKind::Outside);
                ui.horizontal(|ui| {
                    ui.label("Value");
                    let mut b = lin_to_srgb8(*g) as u32;
                    if ui.add(egui::DragValue::new(&mut b).range(0..=255).speed(0.5)).changed() {
                        *g = srgb8_to_lin(b as u8);
                        changed = true;
                    }
                    ui.label("Linear");
                    let mut f = *g;
                    if ui.add(egui::DragValue::new(&mut f).range(0.0..=1.0).speed(0.002).fixed_decimals(3)).changed() {
                        *g = f;
                        changed = true;
                    }
                });
            });
        });
        if ui.input(|i| i.pointer.any_pressed()) && !resp.clicked() {
            if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                if !area.response.rect.contains(p) && !rect.contains(p) {
                    set_open(ui.ctx(), id, false);
                }
            }
        }
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            set_open(ui.ctx(), id, false);
        }
    }
    if picking {
        set_open(ui.ctx(), id, false);
    }
    if changed {
        resp.mark_changed();
    }
    resp
}
