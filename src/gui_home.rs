// SPDX-License-Identifier: MIT OR Apache-2.0
// ======================================================================= Home screen, dialogs, panels (included into gui.rs)

#[derive(PartialEq, Clone, Copy)]
enum Screen {
    Home,
    Editor,
}

struct NewDlg {
    open: bool,
    /// a built-in template (index into `TEMPLATES`), or past them one of `user`
    sel: usize,
    /// the user's own templates (see `user_templates`) and the outputs of each, read when the dialog opens
    user: Vec<(UserTemplate, Vec<String>)>,
    name: String,
    size: usize,
    /// 0 = linked to the width (square), otherwise the separate height
    height: usize,
    depth16: bool,
    /// Package the graph is added to; None = a new package.
    dest: Option<usize>,
}

struct Template {
    name: &'static str,
    desc: &'static str,
    outs: &'static [(&'static str, [f32; 3])],
}

const G5: [f32; 3] = [0.5, 0.5, 0.5];
const TEMPLATES: &[Template] = &[
    Template { name: "Empty", desc: "An empty graph with no nodes and no outputs.", outs: &[] },
    Template {
        name: "Metallic Roughness",
        desc: "PBR metal/roughness workflow. Outputs: basecolor, normal, roughness, metallic, height, ambient_occlusion.",
        outs: &[("basecolor", G5), ("normal", [0.5, 0.5, 1.0]), ("roughness", G5), ("metallic", [0.0; 3]), ("height", G5), ("ambient_occlusion", [1.0; 3])],
    },
    Template {
        name: "Metallic Roughness - Emissive",
        desc: "Metal/roughness plus an emissive output.",
        outs: &[("basecolor", G5), ("normal", [0.5, 0.5, 1.0]), ("roughness", G5), ("metallic", [0.0; 3]), ("height", G5), ("ambient_occlusion", [1.0; 3]), ("emissive", [0.0; 3])],
    },
    Template {
        name: "Metallic Roughness - Opacity",
        desc: "Metal/roughness plus an opacity output.",
        outs: &[("basecolor", G5), ("normal", [0.5, 0.5, 1.0]), ("roughness", G5), ("metallic", [0.0; 3]), ("height", G5), ("ambient_occlusion", [1.0; 3]), ("opacity", [1.0; 3])],
    },
    Template {
        name: "Specular Glossiness",
        desc: "Specular/glossiness workflow. Outputs: diffuse, specular, glossiness, normal, height, ambient_occlusion.",
        outs: &[("diffuse", G5), ("specular", [0.04; 3]), ("glossiness", G5), ("normal", [0.5, 0.5, 1.0]), ("height", G5), ("ambient_occlusion", [1.0; 3])],
    },
    Template { name: "Generator (grayscale)", desc: "A single grayscale output called 'output'.", outs: &[("output", G5)] },
    Template { name: "Generator (color)", desc: "A single color output called 'output'.", outs: &[("output", [0.6, 0.4, 0.3])] },
];

// ---------------------------------------------------------------- config / recent files
fn config_dir() -> PathBuf {
    if let Ok(a) = std::env::var("APPDATA") {
        return PathBuf::from(a).join("MaterialCraft");
    }
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(x).join("materialcraft");
    }
    if let Ok(h) = std::env::var("HOME") {
        let base = PathBuf::from(h);
        return if cfg!(target_os = "macos") { base.join("Library/Application Support/MaterialCraft") } else { base.join(".config/materialcraft") };
    }
    std::env::temp_dir().join("materialcraft")
}

/// Where the user's own templates are kept (one `.sbs` package each).
fn templates_dir() -> PathBuf {
    config_dir().join("templates")
}

fn read_lines(name: &str) -> Vec<String> {
    std::fs::read_to_string(config_dir().join(name)).map(|t| t.lines().map(String::from).filter(|l| !l.is_empty()).collect()).unwrap_or_default()
}

/// Accent colour of the interface (selection, active tabs, primary buttons).
const ACCENT: Color32 = Color32::from_rgb(60, 200, 170);

/// A small abstract picture generated from a file name (no two names look alike, the same name always does).
fn swatch(p: &egui::Painter, r: Rect, name: &str, alive: bool) {
    let mut h: u32 = 2166136261;
    for b in name.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    let mut next = || {
        h ^= h << 13;
        h ^= h >> 17;
        h ^= h << 5;
        (h & 0xffff) as f32 / 65535.0
    };
    let hue = |v: f32, s: f32, l: f32| {
        let c = egui::ecolor::Hsva::new(v, s, l, 1.0);
        let [r, g, b, _] = c.to_srgba_unmultiplied();
        Color32::from_rgb(r, g, b)
    };
    let (h0, h1) = (next(), next());
    let dimf = if alive { 1.0 } else { 0.35 };
    p.rect_filled(r, 2.0, hue(h0, 0.45 * dimf, 0.32));
    for _ in 0..4 {
        let c = pos2(r.min.x + next() * r.width(), r.min.y + next() * r.height());
        p.circle_filled(c, 6.0 + next() * 18.0, hue((h1 + next() * 0.2).fract(), 0.5 * dimf, 0.45 + next() * 0.25).gamma_multiply(0.85));
    }
    let y = r.min.y + r.height() * (0.3 + next() * 0.5);
    p.rect_filled(Rect::from_min_max(pos2(r.min.x, y), pos2(r.max.x, y + 5.0)), 0.0, hue(h1, 0.3 * dimf, 0.78).gamma_multiply(0.5));
}

fn fmt_time(t: std::time::SystemTime) -> String {
    let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, sod) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", sod / 3600, (sod % 3600) / 60)
}

fn fmt_size(b: u64) -> String {
    if b >= 1 << 20 {
        format!("{:.2} MB", b as f64 / (1u64 << 20) as f64)
    } else {
        format!("{} KB", (b + 1023) / 1024)
    }
}

// ---------------------------------------------------------------- background asset generation
fn spawn_banner() -> Receiver<ColorImage> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let src = "size 512\nnode 1 fbm scale=3 octaves=6 gain=0.55 seed=11\nnode 2 fbm scale=9 octaves=5 seed=4\nnode 3 warp in=@1 warp=@2 intensity=0.14\nnode 4 voronoi scale=6 jitter=0.9 seed=5 mode=2\nnode 5 blend fg=@4 bg=@3 mode=5 opacity=0.4\nnode 6 colorize in=@5 p0=0 r0=0.25 g0=0.17 b0=0.12 p1=0.5 r1=0.62 g1=0.45 b1=0.32 p2=1 r2=0.92 g2=0.78 b2=0.6\noutput o 6\n";
        if let Ok(g) = Graph::parse(src) {
            if let Ok(m) = g.evaluate(Some(512)) {
                if let Some(t) = m.get("o") {
                    let _ = tx.send(qimg(t));
                }
            }
        }
    });
    rx
}

/// Pictures for the library's tiles: every node type the editor offers, drawn small in the background and sent in
/// batches as they are ready.
fn spawn_library_thumbs() -> Receiver<HashMap<String, ColorImage>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut out = HashMap::new();
        for d in materialcraft::sbs_defs::palette(materialcraft::sbs_defs::Fam::Texture) {
            if let Some(t) = materialcraft::sbs_native::kind_preview(d.kind, 64) {
                out.insert(d.kind.to_string(), qimg(&t));
            }
            if out.len() >= 24 && tx.send(std::mem::take(&mut out)).is_err() {
                return;
            }
        }
        let _ = tx.send(out);
    });
    rx
}

// ---------------------------------------------------------------- icons
#[derive(Clone, Copy)]
enum Ic {
    New,
    Open,
    Save,
    Undo,
    Redo,
    Frame,
    Layout,
    Comment,
    Subgraph,
    Link,
    Grid,
    Star,
    Sphere,
    Dot,
    Input,
    Package,
    GraphIcon,
}

/// Transparency checkerboard over `r` (only its part inside `clip`); the squares are fixed on screen size and start at
/// the picture's corner, so they move with it.
fn checker(p: &egui::Painter, r: Rect, clip: Rect) {
    let vis = r.intersect(clip);
    if !vis.is_positive() {
        return;
    }
    const S: f32 = 10.0;
    p.rect_filled(vis, 0.0, Color32::from_gray(62));
    let (i0, i1) = (((vis.min.x - r.min.x) / S).floor() as i64, ((vis.max.x - r.min.x) / S).ceil() as i64);
    let (j0, j1) = (((vis.min.y - r.min.y) / S).floor() as i64, ((vis.max.y - r.min.y) / S).ceil() as i64);
    for j in j0..j1 {
        for i in i0..i1 {
            if (i + j) % 2 == 0 {
                let c = Rect::from_min_size(pos2(r.min.x + i as f32 * S, r.min.y + j as f32 * S), vec2(S, S)).intersect(vis);
                p.rect_filled(c, 0.0, Color32::from_gray(98));
            }
        }
    }
}

fn draw_icon(p: &egui::Painter, c: Pos2, ic: Ic, col: Color32) {
    let st = Stroke::new(1.4_f32, col);
    let r = |dx: f32, dy: f32, w: f32, h: f32| Rect::from_min_size(pos2(c.x + dx, c.y + dy), vec2(w, h));
    let l = |a: (f32, f32), b: (f32, f32)| p.line_segment([pos2(c.x + a.0, c.y + a.1), pos2(c.x + b.0, c.y + b.1)], st);
    match ic {
        Ic::New => {
            p.rect_stroke(r(-5.0, -7.0, 10.0, 14.0), 1.0, st, egui::StrokeKind::Inside);
            l((-2.0, 0.0), (2.0, 0.0));
            l((0.0, -2.0), (0.0, 2.0));
        }
        Ic::Open => {
            p.rect_stroke(r(-7.0, -4.0, 14.0, 10.0), 1.0, st, egui::StrokeKind::Inside);
            l((-7.0, -4.0), (-7.0, -7.0));
            l((-7.0, -7.0), (-2.0, -7.0));
            l((-2.0, -7.0), (0.0, -4.0));
        }
        Ic::Save => {
            p.rect_stroke(r(-6.0, -6.0, 12.0, 12.0), 1.0, st, egui::StrokeKind::Inside);
            p.rect_stroke(r(-3.5, -6.0, 7.0, 4.0), 0.0, st, egui::StrokeKind::Inside);
            p.rect_stroke(r(-3.5, 1.0, 7.0, 5.0), 0.0, st, egui::StrokeKind::Inside);
        }
        Ic::Undo | Ic::Redo => {
            let s = if matches!(ic, Ic::Undo) { 1.0 } else { -1.0 };
            let mut prev = pos2(c.x - 5.0 * s, c.y - 1.0);
            for i in 1..=8 {
                let a = std::f32::consts::PI * (1.0 + i as f32 / 8.0 * 1.15);
                let q = pos2(c.x + 1.0 * s + a.cos() * 6.0 * s, c.y + 2.0 + a.sin() * 5.0);
                p.line_segment([prev, q], st);
                prev = q;
            }
            l((-5.0 * s, -1.0), (-7.5 * s, -4.5));
            l((-5.0 * s, -1.0), (-1.5 * s, -2.5));
        }
        Ic::Frame => {
            for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                l((sx * 7.0, sy * 7.0), (sx * 7.0, sy * 3.5));
                l((sx * 7.0, sy * 7.0), (sx * 3.5, sy * 7.0));
            }
        }
        Ic::Layout => {
            p.rect_stroke(r(-7.0, -6.0, 4.0, 4.0), 0.0, st, egui::StrokeKind::Inside);
            p.rect_stroke(r(-7.0, 2.0, 4.0, 4.0), 0.0, st, egui::StrokeKind::Inside);
            p.rect_stroke(r(3.0, -2.0, 4.0, 4.0), 0.0, st, egui::StrokeKind::Inside);
            l((-3.0, -4.0), (0.0, 0.0));
            l((-3.0, 4.0), (0.0, 0.0));
            l((0.0, 0.0), (3.0, 0.0));
        }
        Ic::Comment => {
            p.rect_stroke(r(-7.0, -6.0, 14.0, 12.0), 1.0, st, egui::StrokeKind::Inside);
            l((-7.0, -2.5), (7.0, -2.5));
        }
        Ic::Subgraph => {
            p.rect_stroke(r(-7.0, -7.0, 14.0, 14.0), 1.0, st, egui::StrokeKind::Inside);
            p.rect_stroke(r(-3.0, -3.0, 6.0, 6.0), 0.0, st, egui::StrokeKind::Inside);
        }
        Ic::Link => {
            p.rect_stroke(r(-8.0, -3.0, 9.0, 6.0), 3.0, st, egui::StrokeKind::Inside);
            p.rect_stroke(r(-1.0, -3.0, 9.0, 6.0), 3.0, st, egui::StrokeKind::Inside);
        }
        Ic::Grid => {
            for k in [-3.0f32, 3.0] {
                l((k, -7.0), (k, 7.0));
                l((-7.0, k), (7.0, k));
            }
        }
        Ic::Sphere => {
            p.circle_stroke(c, 6.0, st);
            let pts: Vec<Pos2> = (0..=20).map(|k| { let a = k as f32 / 20.0 * std::f32::consts::TAU; pos2(c.x + a.cos() * 6.0, c.y + a.sin() * 2.4) }).collect();
            p.add(egui::Shape::line(pts, Stroke::new(1.0_f32, col)));
        }
        Ic::Dot => {
            p.circle_stroke(c, 5.0, st);
            p.circle_filled(c, 1.8, col);
        }
        Ic::Input => {
            p.rect_stroke(r(-2.0, -6.0, 8.0, 12.0), 1.0, st, egui::StrokeKind::Inside);
            l((-8.0, 0.0), (1.0, 0.0));
            l((-3.5, -3.5), (1.0, 0.0));
            l((-3.5, 3.5), (1.0, 0.0));
        }
        Ic::Package => {
            let top = [pos2(c.x, c.y - 7.0), pos2(c.x + 6.5, c.y - 3.5), pos2(c.x, c.y), pos2(c.x - 6.5, c.y - 3.5)];
            p.add(egui::Shape::closed_line(top.to_vec(), st));
            l((-6.5, -3.5), (-6.5, 3.5));
            l((6.5, -3.5), (6.5, 3.5));
            l((0.0, 0.0), (0.0, 7.0));
            l((-6.5, 3.5), (0.0, 7.0));
            l((6.5, 3.5), (0.0, 7.0));
        }
        Ic::GraphIcon => {
            p.rect_stroke(r(-7.0, -6.0, 5.0, 4.0), 1.0, st, egui::StrokeKind::Inside);
            p.rect_stroke(r(-7.0, 2.0, 5.0, 4.0), 1.0, st, egui::StrokeKind::Inside);
            p.rect_stroke(r(3.0, -2.0, 5.0, 4.0), 1.0, st, egui::StrokeKind::Inside);
            l((-2.0, -4.0), (0.5, 0.0));
            l((-2.0, 4.0), (0.5, 0.0));
            l((0.5, 0.0), (3.0, 0.0));
        }
        Ic::Star => {
            let mut pts = Vec::new();
            for i in 0..10 {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                let rad = if i % 2 == 0 { 7.0 } else { 3.0 };
                pts.push(pos2(c.x + a.cos() * rad, c.y + a.sin() * rad));
            }
            p.add(egui::Shape::closed_line(pts, st));
        }
    }
}

fn explorer_icon(ui: &mut egui::Ui, ic: Ic) {
    let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
    draw_icon(ui.painter(), rect.center(), ic, Color32::from_gray(200));
}

/// A graph row in the Explorer: graph icon, name, and (expandable) its outputs. Returns the name's response.
fn graph_row(ui: &mut egui::Ui, name: &str, outs: &[String], selected: bool, key: (&str, String)) -> egui::Response {
    let id = ui.make_persistent_id(("grow", key.0, key.1));
    let mut resp = None;
    let head = |ui: &mut egui::Ui, resp: &mut Option<egui::Response>| {
        explorer_icon(ui, Ic::GraphIcon);
        let txt = if selected { egui::RichText::new(name).strong().color(Color32::WHITE) } else { egui::RichText::new(name) };
        *resp = Some(ui.add(egui::Label::new(txt).selectable(false).sense(Sense::click_and_drag())));
    };
    if outs.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            head(ui, &mut resp);
        });
    } else {
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
            .show_header(ui, |ui| head(ui, &mut resp))
            .body(|ui| {
                for o in outs {
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        ui.small(usage_label(o));
                    });
                }
            });
    }
    resp.unwrap_or_else(|| ui.label(""))
}

fn entry_icon(kind: &str) -> Ic {
    match kind {
        "sbs:output" => Ic::Sphere,
        "comment" => Ic::Comment,
        "frame" => Ic::Frame,
        "sbs:passthrough" => Ic::Dot,
        "sbs:input" => Ic::Input,
        "sbs:graph" => Ic::Subgraph,
        _ => match sd_lib_group(kind).0 {
            "Generators" => Ic::Grid,
            "Material tools" => Ic::Subgraph,
            _ => Ic::Layout,
        },
    }
}

fn icon_btn(ui: &mut egui::Ui, ic: Ic, tip: &str, enabled: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 24.0), if enabled { Sense::click() } else { Sense::hover() });
    if resp.hovered() && enabled {
        ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(72, 72, 72));
    }
    draw_icon(ui.painter(), rect.center(), ic, if enabled { Color32::from_gray(215) } else { Color32::from_gray(95) });
    resp.on_hover_text(tip)
}

/// Dockable-panel style header. Returns true when the close button was clicked.
fn panel_header(ui: &mut egui::Ui, title: &str, _closable: bool) -> bool {
    // closing and moving panels is done from the tab's panel menu (see the dock area); the tab already names these panels
    if matches!(title, "EXPLORER" | "LIBRARY" | "2D VIEW") {
        return false;
    }
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(30, 30, 30));
    ui.painter().text(rect.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, title, FontId::proportional(12.0), Color32::from_gray(225));
    false
}

/// The quick-add row above the graph: node label and what it is for.
const COMMON_NODES: &[(&str, &str)] = &[
    ("Bitmap", "Bitmap"),
    ("Blend", "Blend"),
    ("Blur", "Blur"),
    ("Channels Shuffle", "Channels Shuffle"),
    ("Curve", "Curve"),
    ("Directional Blur", "Directional Blur"),
    ("Directional Warp", "Directional Warp"),
    ("Distance", "Distance"),
    ("Emboss", "Emboss"),
    ("FX-Map", "FX-Map"),
    ("Gradient (Dynamic)", "Gradient (Dynamic)"),
    ("Gradient Map", "Gradient Map"),
    ("Grayscale Conversion", "Grayscale Conversion"),
    ("HSL", "HSL"),
    ("Levels", "Levels"),
    ("Normal", "Normal"),
    ("Pixel Processor", "Pixel Processor"),
    ("Sharpen", "Sharpen"),
    ("SVG", "SVG"),
    ("Text", "Text"),
    ("Transformation 2D", "Transformation 2D"),
    ("Uniform Color", "Uniform Color"),
    ("Value Processor", "Value Processor"),
    ("Warp", "Warp"),
];

/// Tile and glyph colours of a common-node button, by what the node does (generators warm, adjustments green,
/// blur and warp blue, processors purple).
fn node_tile_colors(label: &str) -> (Color32, Color32) {
    let tile = match label {
        "Bitmap" | "SVG" | "Text" | "Uniform Color" | "FX-Map" => Color32::from_rgb(150, 72, 62),
        "Pixel Processor" | "Value Processor" => Color32::from_rgb(118, 80, 160),
        "Levels" | "Curve" | "HSL" | "Gradient Map" | "Gradient (Dynamic)" | "Grayscale Conversion" | "Channels Shuffle" | "Blend" => Color32::from_rgb(70, 120, 96),
        _ => Color32::from_rgb(62, 100, 150),
    };
    (tile, Color32::from_gray(235))
}

/// A small picture of what a common node does, inside its tile.
fn node_glyph(p: &egui::Painter, r: Rect, label: &str, col: Color32) {
    let c = r.center();
    let st = Stroke::new(1.3_f32, col);
    let l = |a: (f32, f32), b: (f32, f32)| p.line_segment([pos2(c.x + a.0, c.y + a.1), pos2(c.x + b.0, c.y + b.1)], st);
    let bx = |dx: f32, dy: f32, w: f32, h: f32| Rect::from_min_size(pos2(c.x + dx, c.y + dy), vec2(w, h));
    match label {
        "Bitmap" => {
            p.rect_stroke(bx(-6.0, -5.0, 12.0, 10.0), 1.0, st, egui::StrokeKind::Inside);
            l((-5.0, 4.0), (-1.5, -1.0));
            l((-1.5, -1.0), (1.0, 2.0));
            l((1.0, 2.0), (3.0, 0.0));
            l((3.0, 0.0), (5.0, 4.0));
        }
        "Blend" => {
            p.rect_stroke(bx(-6.0, -6.0, 8.0, 8.0), 1.0, st, egui::StrokeKind::Inside);
            p.rect_filled(bx(-2.0, -2.0, 8.0, 8.0), 1.0, col);
        }
        "Blur" => {
            p.circle_filled(c, 2.5, col);
            p.circle_stroke(c, 4.5, Stroke::new(1.0_f32, col.gamma_multiply(0.6)));
            p.circle_stroke(c, 6.5, Stroke::new(1.0_f32, col.gamma_multiply(0.3)));
        }
        "Channels Shuffle" => {
            l((-6.0, -5.0), (6.0, 5.0));
            l((-6.0, 5.0), (6.0, -5.0));
            l((6.0, 5.0), (3.0, 5.0));
            l((6.0, 5.0), (6.0, 2.0));
            l((6.0, -5.0), (3.0, -5.0));
            l((6.0, -5.0), (6.0, -2.0));
        }
        "Curve" => {
            p.rect_stroke(bx(-6.0, -6.0, 12.0, 12.0), 1.0, Stroke::new(1.0_f32, col.gamma_multiply(0.5)), egui::StrokeKind::Inside);
            let mut prev = pos2(c.x - 6.0, c.y + 6.0);
            for i in 1..=8 {
                let t = i as f32 / 8.0;
                let y = 1.0 - (t * t * (3.0 - 2.0 * t));
                let q = pos2(c.x - 6.0 + 12.0 * t, c.y - 6.0 + 12.0 * y);
                p.line_segment([prev, q], st);
                prev = q;
            }
        }
        "Directional Blur" => {
            l((-6.0, -4.0), (6.0, -4.0));
            l((-6.0, 0.0), (3.0, 0.0));
            l((-6.0, 4.0), (0.0, 4.0));
        }
        "Directional Warp" | "Warp" => {
            for row in [-4.0, 0.0, 4.0] {
                let mut prev = pos2(c.x - 6.0, c.y + row);
                for i in 1..=8 {
                    let t = i as f32 / 8.0;
                    let q = pos2(c.x - 6.0 + 12.0 * t, c.y + row + (t * std::f32::consts::TAU).sin() * 1.6);
                    p.line_segment([prev, q], st);
                    prev = q;
                }
            }
            if label == "Directional Warp" {
                l((2.0, -6.0), (6.0, -6.0));
                l((6.0, -6.0), (4.5, -7.5));
                l((6.0, -6.0), (4.5, -4.5));
            }
        }
        "Distance" => {
            p.rect_stroke(bx(-6.0, -6.0, 12.0, 12.0), 2.0, Stroke::new(1.0_f32, col.gamma_multiply(0.4)), egui::StrokeKind::Inside);
            p.rect_stroke(bx(-3.5, -3.5, 7.0, 7.0), 1.5, Stroke::new(1.0_f32, col.gamma_multiply(0.7)), egui::StrokeKind::Inside);
            p.rect_filled(bx(-1.0, -1.0, 2.0, 2.0), 0.0, col);
        }
        "Emboss" => {
            p.rect_filled(bx(-6.0, -6.0, 12.0, 12.0), 1.0, col.gamma_multiply(0.35));
            p.add(egui::Shape::convex_polygon(vec![pos2(c.x - 6.0, c.y - 6.0), pos2(c.x + 6.0, c.y - 6.0), pos2(c.x - 6.0, c.y + 6.0)], col, Stroke::NONE));
        }
        "FX-Map" => {
            for (dx, dy) in [(0.0, -5.0), (-5.0, 0.0), (5.0, 0.0), (0.0, 5.0)] {
                p.rect_filled(bx(dx - 2.0, dy - 2.0, 4.0, 4.0), 0.5, col);
            }
        }
        "Gradient Map" | "Gradient (Dynamic)" => {
            let n = 12;
            for i in 0..n {
                let t = i as f32 / n as f32;
                p.rect_filled(bx(-6.0 + 12.0 * t, -3.0, 12.0 / n as f32 + 0.5, 6.0), 0.0, col.gamma_multiply(0.15 + 0.85 * t));
            }
            if label == "Gradient (Dynamic)" {
                p.circle_filled(pos2(c.x, c.y + 5.0), 1.8, col);
            }
        }
        "Grayscale Conversion" => {
            p.circle_stroke(c, 6.0, st);
            p.add(egui::Shape::convex_polygon((0..=12).map(|i| { let a = -std::f32::consts::FRAC_PI_2 + i as f32 / 12.0 * std::f32::consts::PI; pos2(c.x + a.cos() * 6.0, c.y + a.sin() * 6.0) }).collect(), col, Stroke::NONE));
        }
        "HSL" => {
            for (i, cc) in [Color32::from_rgb(230, 80, 80), Color32::from_rgb(90, 200, 90), Color32::from_rgb(90, 120, 235)].iter().enumerate() {
                let a0 = i as f32 / 3.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                let pts: Vec<Pos2> = std::iter::once(c).chain((0..=6).map(|k| { let a = a0 + k as f32 / 6.0 * std::f32::consts::TAU / 3.0; pos2(c.x + a.cos() * 6.5, c.y + a.sin() * 6.5) })).collect();
                p.add(egui::Shape::convex_polygon(pts, *cc, Stroke::NONE));
            }
        }
        "Levels" => {
            for (i, h) in [3.0, 6.0, 9.0, 11.0, 7.0, 4.0].iter().enumerate() {
                p.rect_filled(bx(-6.0 + i as f32 * 2.0, 6.0 - h, 1.6, *h), 0.0, col);
            }
        }
        "Normal" => {
            p.circle_filled(pos2(c.x - 1.0, c.y + 2.0), 4.5, col.gamma_multiply(0.5));
            l((-1.0, 2.0), (4.0, -5.0));
            l((4.0, -5.0), (1.0, -5.0));
            l((4.0, -5.0), (4.0, -2.0));
        }
        "Pixel Processor" => {
            for y in 0..3 {
                for x in 0..3 {
                    let on = (x + y) % 2 == 0;
                    p.rect_filled(bx(-6.0 + x as f32 * 4.0, -6.0 + y as f32 * 4.0, 3.5, 3.5), 0.0, col.gamma_multiply(if on { 1.0 } else { 0.4 }));
                }
            }
        }
        "Sharpen" => {
            p.add(egui::Shape::convex_polygon(vec![pos2(c.x, c.y - 7.0), pos2(c.x + 2.0, c.y - 2.0), pos2(c.x + 7.0, c.y), pos2(c.x + 2.0, c.y + 2.0), pos2(c.x, c.y + 7.0), pos2(c.x - 2.0, c.y + 2.0), pos2(c.x - 7.0, c.y), pos2(c.x - 2.0, c.y - 2.0)], col, Stroke::NONE));
        }
        "SVG" => {
            let mut prev = pos2(c.x - 6.0, c.y + 5.0);
            for i in 1..=10 {
                let t = i as f32 / 10.0;
                let x = c.x - 6.0 + 12.0 * t;
                let y = c.y + 5.0 - 10.0 * (3.0 * t * (1.0 - t) * (1.0 - t) + t * t * t);
                let q = pos2(x, y);
                p.line_segment([prev, q], st);
                prev = q;
            }
            p.circle_filled(pos2(c.x - 6.0, c.y + 5.0), 1.6, col);
            p.circle_filled(pos2(c.x + 6.0, c.y - 5.0), 1.6, col);
        }
        "Text" => {
            p.text(c, Align2::CENTER_CENTER, "T", FontId::proportional(14.0), col);
        }
        "Transformation 2D" => {
            p.rect_stroke(bx(-4.0, -4.0, 8.0, 8.0), 0.5, st, egui::StrokeKind::Inside);
            l((-7.0, -7.0), (-4.0, -4.0));
            l((7.0, 7.0), (4.0, 4.0));
            l((-7.0, -7.0), (-7.0, -4.0));
            l((-7.0, -7.0), (-4.0, -7.0));
            l((7.0, 7.0), (7.0, 4.0));
            l((7.0, 7.0), (4.0, 7.0));
        }
        "Uniform Color" => {
            p.rect_filled(bx(-6.0, -6.0, 12.0, 12.0), 1.5, col);
        }
        "Value Processor" => {
            p.text(c, Align2::CENTER_CENTER, "x=", FontId::monospace(11.0), col);
        }
        _ => {
            p.rect_stroke(bx(-6.0, -6.0, 12.0, 12.0), 1.0, st, egui::StrokeKind::Inside);
        }
    }
}

fn lib_group(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "white_noise" | "gaussian_noise" | "perlin" | "fbm" | "clouds" | "voronoi" | "voronoi_fractal" | "bnw_spots" | "crystal" | "directional_noise" | "blue_noise" | "caustics" | "liquid_noise" | "perlin_3d" | "anisotropic_noise" | "fur_noise" | "moisture_noise" | "microscope_noise" | "worley_3d" | "fbm_3d" | "grunge" | "grunge_map" | "plasma" | "cells" | "fractal_sum" | "waveform" | "gaussian_spots" | "preset_noise" | "noise_pattern" | "noise_3d" => ("Generators", "Noises"),
        "herringbone" | "arc_pavement" | "triangle_grid" | "starburst" | "gradient_reflected" | "fibers" | "scratches" | "paving" | "wire_mesh" => ("Generators", "Patterns"),
        "path_stroke" | "path_fill" | "path_distance" | "path_border" | "path_scatter" | "path_track" | "path_cable" | "path_seam" | "path_splatter" | "path_sample" => ("Generators", "Paths"),
        "path_warp" => ("Filters", "Blur and warp"),
        "color" | "gradient" | "gradient_axial" | "gradient_circular" | "gradient_2pt" | "checker" | "bricks" | "stripes" | "weave" | "weave_preset" | "tile_preset" | "tile_random2" | "weave_generator" | "tile_sampler" | "tile_generator" | "splatter" | "splatter_circular" | "pixel_processor" => ("Generators", "Patterns"),
        "shape" | "circle" | "rect" | "polygon" | "star" | "volume_mask" | "gradient_3d" => ("Generators", "Shapes"),
        "sdf_shape" | "sdf_op" | "sdf_mask" | "sdf_form" => ("Generators", "SDF"),
        "levels" | "adjust" | "invert" | "threshold" | "histogram_scan" | "histogram_range" | "quantize" | "quantize_color" | "clamp" | "hsl" | "colorize" | "tint" | "to_gray" | "rgba_split" | "alpha_split" | "channel" | "merge" | "switch" | "color_match" | "highpass" | "auto_levels" | "channel_mixer" | "alpha_merge" | "color_to_mask" | "replace_color" | "pow" | "histogram_shift" | "grayscale_conversion" | "chrominance_extract" | "contrast_luminosity" | "srgb_to_linear" | "linear_to_srgb" | "premultiply" | "histogram_equalize" | "min_max" | "morphology" | "multi_switch" | "lookup_table" | "color_lut" | "frequency_split" | "frequency_merge" | "palette_extract" | "palette_apply" => ("Filters", "Adjustments"),
        "blur" | "blur_hq" | "bilateral_blur" | "directional_blur" | "sharpen" | "warp" | "directional_warp" | "vector_warp" | "vector_morph" | "slope_blur" | "swirl" | "non_uniform_blur" | "radial_blur" | "anisotropic_blur" | "non_uniform_directional_warp" | "multi_directional_warp" => ("Filters", "Blur and warp"),
        "transform" | "mirror" | "make_it_tile_photo" | "make_it_tile_patch" | "symmetry" | "skew" | "auto_crop" | "polar_convert" | "clone_patch" | "noise_upscale" | "safe_transform" | "symmetry_slice" | "quad_transform" | "trapezoid_transform" | "make_it_tile_mirror" => ("Filters", "Transform"),
        "glow" | "mosaic" | "halftone" | "reaction_diffusion" | "fxaa" | "shape_drop_shadow" | "shape_stroke" | "shape_glow" | "sobel" | "shape_extrude" | "cross_section" | "uber_emboss" | "diffusion" | "summed_area_table" => ("Filters", "Effects"),
        "distance" | "bevel" | "edge_detect" | "emboss" | "flood_fill" | "flood_fill_random" | "flood_fill_color" | "flood_fill_gradient" | "shape_mapper" => ("Filters", "Masks and regions"),
        "normal" | "normal_blend" | "normal_combine" | "normal_invert" | "normal_normalize" | "normal_vector_rotation" | "normal_to_height" | "ao" | "curvature" | "curvature_smooth" | "height_blend" | "bent_normal" | "normal_facing" | "height_normal_blend" | "height_to_normal_world" | "normal_transform" | "normal_sobel" | "metal_rough_to_spec_gloss" | "spec_gloss_to_metal_rough" | "material_blend" | "height_combine" | "convex_concave" | "leak_streaks" | "settled_dust" | "snow_filter" | "season_filter" | "water_filter" => ("Material tools", ""),
        "edge_wear" | "dirt" | "dust" | "mesh_weathering" | "baked_map" => ("Mesh-driven", ""),
        "bitmap" | "input" | "subgraph" | "comment" | "frame" | "dot" | "output" => ("Structure", ""),
        _ => ("Filters", "Adjustments"),
    }
}

/// The library folder of a library graph: by the node MaterialCraft draws it with, else by what it takes (a graph
/// without image inputs makes a pattern or noise).
fn sd_lib_group(kind: &str) -> (&'static str, &'static str) {
    let id = kind.rsplit('/').next().unwrap_or(kind);
    let known = materialcraft::sbs_import::library_kind(id).map(lib_group);
    if let Some(g @ (m, sub)) = known {
        let in_tree = LIB_TREE.iter().any(|(n, subs)| *n == m && (sub.is_empty() || subs.contains(&sub)));
        if in_tree && m != "Structure" {
            return g;
        }
    }
    let no_inputs = materialcraft::sbs_defs::def(kind).map(|d| d.inputs.is_empty()).unwrap_or(false);
    let l = id.to_ascii_lowercase();
    if no_inputs && (l.contains("noise") || l.contains("grunge") || l.contains("cloud") || l.contains("fractal") || l.contains("perlin")) {
        ("Generators", "Noises")
    } else if no_inputs {
        ("Generators", "Patterns")
    } else if l.contains("normal") || l.contains("height") || l.contains("curvature") || l.contains("ambient") || l.contains("material") {
        ("Material tools", "")
    } else {
        ("Filters", "Adjustments")
    }
}

const LIB_TREE: &[(&str, &[&str])] = &[
    ("Favorites", &[]),
    ("Structure", &[]),
    ("Core nodes", &[]),
    ("Pattern builders", &[]),
    ("Logic nodes", &[]),
    ("Generators", &["Noises", "Patterns", "Shapes"]),
    ("Filters", &["Adjustments", "Blur and warp", "Transform", "Effects", "Masks and regions"]),
    ("Material tools", &[]),
];

fn meta_field(ui: &mut egui::Ui, meta: &mut BTreeMap<String, String>, label: &str, key: &str, multi: bool) {
    ui.strong(label);
    let mut v = meta.get(key).cloned().unwrap_or_default();
    let resp = if multi {
        ui.add(egui::TextEdit::multiline(&mut v).desired_rows(4).desired_width(f32::INFINITY))
    } else {
        ui.add(egui::TextEdit::singleline(&mut v).desired_width(f32::INFINITY))
    };
    if resp.changed() {
        meta.insert(key.to_string(), v);
    }
    ui.add_space(4.0);
}

fn two_lines(label: &str) -> (String, String) {
    let t = label.replace('_', " ").replace("subgraph: ", "");
    if t.len() <= 11 {
        return (t, String::new());
    }
    let mid = t.len() / 2;
    let split = t.char_indices().filter(|(_, c)| *c == ' ').map(|(i, _)| i).min_by_key(|i| (*i as i32 - mid as i32).abs());
    match split {
        Some(i) => (t[..i].to_string(), t[i + 1..].to_string()),
        None => (t.chars().take(11).collect(), String::new()),
    }
}

impl App {
    fn gname(&self) -> String {
        let root = self.parents.first().map(|p| &p.0).unwrap_or(&self.g);
        root.meta.get("identifier").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| "Graph".into())
    }

    fn package_label(&self) -> String {
        match &self.file {
            Some(p) => p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
            None => self.unsaved_name.clone(),
        }
    }

    fn push_recent(&mut self, p: &std::path::Path) {
        self.recent.retain(|r| r != p);
        self.recent.insert(0, p.to_path_buf());
        self.recent.truncate(20);
        let files: Vec<String> = self.recent.iter().map(|r| r.to_string_lossy().to_string()).collect();
        self.prefs.update(|v| v.recent.files = files);
    }

    fn save_settings(&mut self) {
        let (skip, tip, fav) = (self.skip_home, self.hide_suggestions, self.favorites.iter().cloned().collect::<Vec<_>>());
        let (snap, grid) = (self.snap_nodes, self.snap_grid.round() as u32);
        self.prefs.update(|v| {
            v.general.skip_home = skip;
            v.general.hide_tip = tip;
            v.general.snap_nodes = snap;
            v.general.snap_grid = grid;
            v.library.favorites = fav;
        });
    }

    fn open_path(&mut self, p: PathBuf) {
        if is_sbs(&p) {
            return self.open_sbs(p);
        }
        self.status(format!("{}: only .sbs packages can be opened", p.display()), false);
    }

    fn open_new_dialog(&mut self, dest: Option<usize>) {
        self.ws_init();
        self.new_dlg.dest = if self.screen == Screen::Editor { dest } else { None };
        self.new_dlg.name = if self.new_dlg.dest.is_some() { "Material_Graph".into() } else { "Graph".into() };
        self.new_dlg.user = user_templates::list(&templates_dir())
            .into_iter()
            .map(|t| {
                let outs = user_templates::outputs(&t).unwrap_or_default();
                (t, outs)
            })
            .collect();
        if self.new_dlg.sel >= TEMPLATES.len() + self.new_dlg.user.len() {
            self.new_dlg.sel = 1;
        }
        self.new_dlg.open = true;
    }

    /// Save graph `name` of package `i` (None: its main graph) as a template of the New dialog, under `tname`.
    fn save_as_template(&mut self, i: usize, name: Option<String>, tname: &str) {
        let Some(root) = self.package_root(i) else { return };
        let gname = name.unwrap_or_else(|| materialcraft::sbs_native::graph_name(&root));
        match user_templates::save(&templates_dir(), tname, &root, &gname) {
            Ok(p) => self.status(format!("Saved graph '{gname}' as template '{}' ({})", user_templates::clean_name(tname), p.display()), true),
            Err(e) => self.status(format!("Cannot save the template: {e}"), false),
        }
    }

    /// The "Save as template" box: the template's name, and a warning when one of that name exists.
    fn save_template_ui(&mut self, ctx: &egui::Context) {
        let Some((i, graph, mut tname)) = self.save_tmpl.take() else { return };
        let (mut ok, mut cancel) = (false, false);
        egui::Window::new("Save as template").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            ui.label("The graph (with the graphs and functions it uses) becomes a template of File > New.");
            ui.horizontal(|ui| {
                ui.label("Template name");
                let r = ui.add(egui::TextEdit::singleline(&mut tname).desired_width(240.0));
                r.request_focus();
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    ok = true;
                }
            });
            let clean = user_templates::clean_name(&tname);
            if !clean.is_empty() && user_templates::path_for(&templates_dir(), &clean).exists() {
                ui.colored_label(Color32::from_rgb(255, 190, 90), format!("A template called '{clean}' exists: saving replaces it."));
            }
            ui.small(format!("Folder: {}", templates_dir().display()));
            ui.horizontal(|ui| {
                ok |= ui.add_enabled(!clean.is_empty(), egui::Button::new("Save")).clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if ok && !user_templates::clean_name(&tname).is_empty() {
            self.save_as_template(i, graph, &tname);
        } else if !cancel {
            self.save_tmpl = Some((i, graph, tname));
        }
    }

    /// Delete the user's template `k` of the New dialog (its file).
    fn delete_user_template(&mut self, k: usize) {
        let Some((t, _)) = self.new_dlg.user.get(k).cloned() else { return };
        match std::fs::remove_file(&t.path) {
            Ok(()) => {
                self.new_dlg.user.remove(k);
                self.new_dlg.sel = 1;
                self.status(format!("Deleted template '{}'", t.name), true);
            }
            Err(e) => self.status(format!("Cannot delete {}: {e}", t.path.display()), false),
        }
    }

    fn create_from_dialog(&mut self) {
        let mut name: String = self.new_dlg.name.trim().replace(' ', "_");
        if name.is_empty() {
            name = if self.new_dlg.dest.is_some() { "Material_Graph".into() } else { "Graph".into() };
        }
        self.new_dlg.open = false;
        let dest = self.new_dlg.dest.filter(|_| self.screen == Screen::Editor);
        if let Some((ut, _)) = self.new_dlg.sel.checked_sub(TEMPLATES.len()).and_then(|k| self.new_dlg.user.get(k)).cloned() {
            return self.create_from_user_template(&ut, &name, dest);
        }
        let t = &TEMPLATES[self.new_dlg.sel.min(TEMPLATES.len() - 1)];
        let size = self.new_dlg.size;
        match dest {
            Some(d) => {
                self.switch_pkg(d);
                self.goto_root();
                self.snap();
                let base: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
                let mut unique = base.clone();
                let mut k = 2;
                while materialcraft::sbs_native::all_graphs(&self.g).contains_key(&unique) {
                    unique = format!("{base}_{k}");
                    k += 1;
                }
                match materialcraft::sbs_native::create_graph(&mut self.g, &unique, "graph") {
                    Ok(()) => {
                        if let Some(sg) = self.g.subgraphs.get_mut(&unique) {
                            materialcraft::sbs_native::template_into(sg, t.outs);
                        }
                        self.enter_sub(&unique);
                        self.modified = true;
                        self.status(format!("Created graph '{unique}' in {}", self.package_label()), true);
                    }
                    Err(e) => self.status(e, false),
                }
            }
            None => {
                let ident: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
                let mut g = materialcraft::sbs_native::new_package(&ident, size);
                materialcraft::sbs_native::template_into(&mut g, t.outs);
                let unsaved = if self.screen == Screen::Editor { self.unique_unsaved_name() } else { "Unsaved Package".to_string() };
                let pkg = Pkg { path: None, name: unsaved, g, modified: true, tabs: Vec::new() };
                // nothing is open yet (the empty start graph): the package takes its place
                if self.pkgs.is_empty() && self.file.is_none() && self.g.nodes.is_empty() {
                    self.ws_fresh();
                    self.load_active(pkg);
                } else {
                    self.add_pkg_active(pkg);
                }
                self.screen = Screen::Editor;
                self.need_fit = true;
                self.status(format!("Created package with '{ident}' from template '{}'", t.name), true);
            }
        }
    }

    /// A copy of the user's template `t`, its graph called `name`: a new package, or a graph added to package `dest`.
    fn create_from_user_template(&mut self, t: &UserTemplate, name: &str, dest: Option<usize>) {
        let g = match user_templates::instantiate(t, name) {
            Ok(g) => g,
            Err(e) => return self.status(format!("Template '{}' failed to load: {e}", t.name), false),
        };
        match dest {
            Some(d) => {
                self.switch_pkg(d);
                self.goto_root();
                self.snap();
                match materialcraft::sbs_native::add_package(&mut self.g, g) {
                    Ok((first, _, warnings)) => {
                        self.modified = true;
                        self.dirty = true;
                        self.enter_sub(&first);
                        let more = if warnings.is_empty() { String::new() } else { format!(" ({})", warnings.join("; ")) };
                        self.status(format!("Created graph '{first}' from template '{}'{more}", t.name), warnings.is_empty());
                    }
                    Err(e) => {
                        self.undo.pop();
                        self.status(format!("Cannot add template '{}': {e}", t.name), false);
                    }
                }
            }
            None => {
                let ident = materialcraft::sbs_native::graph_name(&g);
                let unsaved = if self.screen == Screen::Editor { self.unique_unsaved_name() } else { "Unsaved Package".to_string() };
                let pkg = Pkg { path: None, name: unsaved, g, modified: true, tabs: Vec::new() };
                if self.pkgs.is_empty() && self.file.is_none() && self.g.nodes.is_empty() {
                    self.ws_fresh();
                    self.load_active(pkg);
                } else {
                    self.add_pkg_active(pkg);
                }
                self.screen = Screen::Editor;
                self.need_fit = true;
                self.status(format!("Created package with '{ident}' from template '{}'", t.name), true);
            }
        }
    }

    /// A new package from the user's template at `path`, with the New dialog's other settings at their defaults.
    fn new_from_user_template(&mut self, path: PathBuf) {
        let Some(t) = user_templates::list(&templates_dir()).into_iter().find(|t| t.path == path) else {
            return self.status(format!("Template {} no longer exists", path.display()), false);
        };
        self.create_from_user_template(&t, &t.name.replace([' ', '-'], "_"), None);
    }

    /// An editable copy of shipped material template `i`.
    fn new_from_material(&mut self, i: usize) {
        let (ident, text, _) = materialcraft::material_templates::MATERIALS[i];
        match materialcraft::sbs_native::load(text, None) {
            Ok((g, _)) => {
                let name = if self.screen == Screen::Editor { self.unique_unsaved_name() } else { "Unsaved Package".to_string() };
                let pkg = Pkg { path: None, name, g, modified: true, tabs: Vec::new() };
                if self.pkgs.is_empty() && self.file.is_none() && !self.modified && self.g.nodes.is_empty() {
                    self.ws_fresh();
                    self.load_active(pkg);
                } else {
                    self.add_pkg_active(pkg);
                }
                self.screen = Screen::Editor;
                self.need_fit = true;
                self.status(format!("Opened a copy of the '{}' material template", materialcraft::material_templates::title(ident)), true);
            }
            Err(e) => self.status(format!("Template '{ident}' failed to load: {e}"), false),
        }
    }

    /// A new package from template `i` of the New dialog, with the dialog's other settings at their defaults.
    fn new_from_template(&mut self, i: usize) {
        self.new_dlg.sel = i.min(TEMPLATES.len() - 1);
        self.new_dlg.dest = None;
        self.new_dlg.name = "Graph".into();
        self.create_from_dialog();
    }

    // ------------------------------------------------------------ new graph dialog
    fn new_dialog(&mut self, ctx: &egui::Context) {
        if !self.new_dlg.open {
            return;
        }
        let mut open = true;
        let mut create = false;
        let mut cancel = false;
        egui::Window::new(if self.new_dlg.dest.is_some() { "New Material Graph" } else { "New Package" }).open(&mut open).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).fixed_size(vec2(780.0, 470.0)).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(340.0, 400.0), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    ui.label(egui::RichText::new("TEMPLATES").size(12.0).color(Color32::from_gray(200)));
                    ui.add_space(4.0);
                    egui::Frame::NONE.fill(Color32::from_rgb(32, 32, 32)).stroke(Stroke::new(1.0_f32, Color32::from_rgb(60, 60, 60))).show(ui, |ui| {
                        ui.set_min_size(vec2(334.0, 366.0));
                        egui::ScrollArea::vertical().id_salt("tmpl").max_height(366.0).show(ui, |ui| {
                            for (i, t) in TEMPLATES.iter().enumerate() {
                                let sel = self.new_dlg.sel == i;
                                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 21.0), Sense::click());
                                if sel {
                                    ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(40, 62, 100));
                                    ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(60, 120, 230)), egui::StrokeKind::Inside);
                                } else if resp.hovered() {
                                    ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(44, 44, 44));
                                }
                                draw_icon(ui.painter(), rect.left_center() + vec2(14.0, 0.0), Ic::Layout, Color32::from_gray(190));
                                ui.painter().text(rect.left_center() + vec2(30.0, 0.0), Align2::LEFT_CENTER, t.name, FontId::proportional(12.5), Color32::from_gray(225));
                                if resp.clicked() {
                                    self.new_dlg.sel = i;
                                }
                                if resp.double_clicked() {
                                    self.new_dlg.sel = i;
                                }
                            }
                            // the user's own templates (File > Save graph as template...)
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new("MY TEMPLATES").size(11.0).color(Color32::from_gray(170)));
                            if self.new_dlg.user.is_empty() {
                                ui.small("None yet: File > Save graph as template... makes one from the graph on screen.");
                            }
                            let mut delete = None;
                            for (k, (t, _)) in self.new_dlg.user.iter().enumerate() {
                                let i = TEMPLATES.len() + k;
                                let sel = self.new_dlg.sel == i;
                                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 21.0), Sense::click());
                                if sel {
                                    ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(40, 62, 100));
                                    ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(60, 120, 230)), egui::StrokeKind::Inside);
                                } else if resp.hovered() {
                                    ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(44, 44, 44));
                                }
                                draw_icon(ui.painter(), rect.left_center() + vec2(14.0, 0.0), Ic::GraphIcon, Color32::from_gray(190));
                                ui.painter().text(rect.left_center() + vec2(30.0, 0.0), Align2::LEFT_CENTER, &t.name, FontId::proportional(12.5), Color32::from_gray(225));
                                if resp.clicked() {
                                    self.new_dlg.sel = i;
                                }
                                if resp.double_clicked() {
                                    self.new_dlg.sel = i;
                                    create = true;
                                }
                                resp.context_menu(|ui| {
                                    if ui.button("Delete template").on_hover_text(format!("Delete {}", t.path.display())).clicked() {
                                        delete = Some(k);
                                        ui.close();
                                    }
                                });
                            }
                            if let Some(k) = delete {
                                self.delete_user_template(k);
                            }
                        });
                    });
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width(380.0);
                    ui.label(egui::RichText::new("PROPERTIES").size(12.0).color(Color32::from_gray(200)));
                    ui.add_space(8.0);
                    ui.strong("Graph Name:");
                    ui.add(egui::TextEdit::singleline(&mut self.new_dlg.name).desired_width(f32::INFINITY));
                    ui.add_space(8.0);
                    ui.strong("Parent size (initial working resolution)");
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("nd_w").width(150.0).selected_text(format!("{} px", self.new_dlg.size)).show_ui(ui, |ui| {
                            for s in [128usize, 256, 512, 1024, 2048, 4096, 8192] {
                                ui.selectable_value(&mut self.new_dlg.size, s, format!("{s} px"));
                            }
                        });
                        let linked = self.new_dlg.height == 0;
                        let (r, resp) = ui.allocate_exact_size(vec2(26.0, 24.0), Sense::click());
                        ui.painter().rect_filled(r, 3.0, Color32::from_rgb(60, 60, 60));
                        draw_icon(ui.painter(), r.center(), Ic::Link, if linked { Color32::WHITE } else { Color32::from_gray(110) });
                        if resp.on_hover_text(if linked { "Linked: the graph is square (click to set a separate height)" } else { "Unlinked: click to make the graph square again" }).clicked() {
                            self.new_dlg.height = if linked { self.new_dlg.size } else { 0 };
                        }
                        let shown = if linked { self.new_dlg.size } else { self.new_dlg.height };
                        ui.add_enabled_ui(!linked, |ui| {
                            egui::ComboBox::from_id_salt("nd_h").width(150.0).selected_text(format!("{shown} px")).show_ui(ui, |ui| {
                                for s in [128usize, 256, 512, 1024, 2048, 4096, 8192] {
                                    ui.selectable_value(&mut self.new_dlg.height, s, format!("{s} px"));
                                }
                            });
                        });
                    });
                    ui.add_space(8.0);
                    ui.strong("Output format");
                    egui::ComboBox::from_id_salt("nd_fmt").width(380.0).selected_text(if self.new_dlg.depth16 { "16 bits per channel" } else { "8 bits per channel" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.new_dlg.depth16, false, "8 bits per channel");
                        ui.selectable_value(&mut self.new_dlg.depth16, true, "16 bits per channel");
                    });
                    ui.add_space(8.0);
                    ui.strong("Create graph in package");
                    let names: Vec<String> = if self.screen == Screen::Editor { (0..self.pkgs.len()).map(|i| self.pkg_title(i)).collect() } else { Vec::new() };
                    let shown = match self.new_dlg.dest {
                        Some(i) => names.get(i).cloned().unwrap_or_else(|| "New package".into()),
                        None => "New package".into(),
                    };
                    egui::ComboBox::from_id_salt("nd_pkg").width(380.0).selected_text(shown).show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            if ui.selectable_label(self.new_dlg.dest == Some(i), n).clicked() {
                                self.new_dlg.dest = Some(i);
                            }
                        }
                        if ui.selectable_label(self.new_dlg.dest.is_none(), "New package").clicked() {
                            self.new_dlg.dest = None;
                        }
                    });
                    ui.add_space(12.0);
                    egui::CollapsingHeader::new(egui::RichText::new("Template details").strong()).default_open(true).show(ui, |ui| {
                        match self.new_dlg.sel.checked_sub(TEMPLATES.len()).and_then(|k| self.new_dlg.user.get(k)) {
                            Some((t, outs)) => {
                                ui.label(format!("Your template '{}': a copy of the graph it was saved from, with its nodes, settings and the graphs it uses. It keeps its own resolution.", t.name));
                                if !outs.is_empty() {
                                    ui.add_space(4.0);
                                    ui.small(format!("Outputs: {}", outs.join(", ")));
                                }
                                ui.small(t.path.display().to_string());
                            }
                            None => {
                                let t = &TEMPLATES[self.new_dlg.sel.min(TEMPLATES.len() - 1)];
                                ui.label(t.desc);
                                if !t.outs.is_empty() {
                                    ui.add_space(4.0);
                                    ui.small(format!("Outputs: {}", t.outs.iter().map(|o| o.0).collect::<Vec<_>>().join(", ")));
                                }
                            }
                        }
                    });
                });
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("RESET").clicked() {
                    self.new_dlg.name = "Graph".into();
                    self.new_dlg.size = 2048;
                    self.new_dlg.sel = 1;
                    self.new_dlg.depth16 = false;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new("CANCEL").min_size(vec2(90.0, 24.0))).clicked() {
                        cancel = true;
                    }
                    if ui.add(egui::Button::new(egui::RichText::new("OK").strong()).fill(Color32::from_rgb(20, 90, 190)).min_size(vec2(90.0, 24.0))).clicked() {
                        create = true;
                    }
                });
            });
        });
        if create {
            self.create_from_dialog();
        }
        if cancel || !open {
            self.new_dlg.open = false;
        }
    }

    /// Ask before closing a package that has unsaved changes.
    fn confirm_close_ui(&mut self, ctx: &egui::Context) {
        let Some(i) = self.confirm_close else { return };
        if i >= self.pkgs.len() {
            self.confirm_close = None;
            return;
        }
        let name = self.pkg_title(i);
        let (mut save, mut discard, mut cancel) = (false, false, false);
        egui::Window::new("Close package").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            ui.label(format!("'{name}' has unsaved changes."));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                save = ui.button("Save").clicked();
                discard = ui.button("Discard changes").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if save {
            self.confirm_close = None;
            self.switch_pkg(i);
            self.save();
            if !self.modified {
                let i = self.pkg_active;
                self.close_pkg(i);
            }
        } else if discard {
            self.confirm_close = None;
            self.close_pkg(i);
        } else if cancel {
            self.confirm_close = None;
        }
    }

    // ------------------------------------------------------------ explorer
    fn explorer_ui(&mut self, ui: &mut egui::Ui) {
        self.ws_init();
        if panel_header(ui, "EXPLORER", true) {
            self.show[0] = false;
        }
        ui.horizontal(|ui| {
            if icon_btn(ui, Ic::Save, "Save package", true).clicked() {
                self.acts.push(Act::Save);
            }
            if icon_btn(ui, Ic::Open, "Open package(s)", true).clicked() {
                self.acts.push(Act::Open);
            }
        });
        // right-click on empty space: package actions
        let area = ui.available_rect_before_wrap();
        let bg = ui.interact(area, Id::new("explorer_bg"), Sense::click());
        bg.context_menu(|ui| {
            if ui.button("New Package").clicked() {
                self.acts.push(Act::NewPackage);
                ui.close();
            }
            if ui.button("New Material Graph").clicked() {
                self.acts.push(Act::NewGraphIn(self.pkg_sel));
                ui.close();
            }
            ui.separator();
            if ui.button("Open Package(s)...").clicked() {
                self.acts.push(Act::Open);
                ui.close();
            }
        });
        let current = self.parents.last().map(|(_, n)| n.clone());
        let n_pkgs = self.pkgs.len();
        egui::ScrollArea::vertical().id_salt("explorer").auto_shrink([false, false]).show(ui, |ui| {
            for i in 0..n_pkgs {
                let is_active = i == self.pkg_active;
                let label = format!("{}{}", self.pkg_title(i), if self.pkg_modified(i) { "*" } else { "" });
                // graphs of this package: (name, outputs, is the package's main graph)
                let (main_name, graphs): (String, Vec<(String, Vec<String>, bool)>) = {
                    let root: &Graph = match self.pkgs.get(i) {
                        Some(Some(p)) => &p.g,
                        _ => self.parents.first().map(|p| &p.0).unwrap_or(&self.g),
                    };
                    let main = root.meta.get("identifier").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| "Graph".into());
                    let mut v = vec![(main.clone(), root.output_idents(), true)];
                    v.extend(root.subgraphs.iter().filter(|(_, s)| materialcraft::sbs_native::listed(s)).map(|(k, s)| (k.clone(), s.output_idents(), false)));
                    (main, v)
                };
                let _ = main_name;
                let id = ui.make_persistent_id(("pkg", i));
                let selected = self.pkg_sel == i;
                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                    .show_header(ui, |ui| {
                        explorer_icon(ui, Ic::Package);
                        let mut txt = egui::RichText::new(&label).italics().strong();
                        if selected {
                            txt = txt.color(Color32::WHITE);
                        }
                        let r = ui.add(egui::Label::new(txt).selectable(false).sense(Sense::click()));
                        if r.clicked() {
                            self.pkg_sel = i;
                        }
                        r.on_hover_text(match self.pkgs.get(i) {
                            Some(Some(p)) => p.path.as_ref().map(|x| x.display().to_string()).unwrap_or_else(|| "Not saved yet".into()),
                            _ => self.file.as_ref().map(|x| x.display().to_string()).unwrap_or_else(|| "Not saved yet".into()),
                        })
                        .context_menu(|ui| {
                            self.pkg_sel = i;
                            ui.menu_button("New", |ui| {
                                if ui.button("Material Graph").clicked() {
                                    self.acts.push(Act::NewGraphIn(i));
                                    ui.close();
                                }
                                if ui.button("Function").on_hover_text("A function graph of the package: values call it with an Instance node").clicked() {
                                    self.acts.push(Act::NewFunctionIn(i));
                                    ui.close();
                                }
                            });
                            if ui.button("Save Package").clicked() {
                                self.acts.push(Act::SavePkg(i));
                                ui.close();
                            }
                            if ui.button("Add Graph File...").on_hover_text("Copy every graph and function of another .sbs package into this one").clicked() {
                                self.acts.push(Act::AddGraphFile(i));
                                ui.close();
                            }
                            if ui.button("Close Package").clicked() {
                                self.acts.push(Act::ClosePkg(i));
                                ui.close();
                            }
                        });
                    })
                    .body(|ui| {
                        // graphs kept in folders of the package are listed under their folder
                        let mut last_folder: Option<String> = None;
                        for (name, outs, is_main) in &graphs {
                            let on = is_active && if *is_main { current.is_none() } else { current.as_deref() == Some(name.as_str()) };
                            let folder = name.rsplit_once('/').map(|(f, _)| f.to_string());
                            if folder != last_folder {
                                if let Some(f) = &folder {
                                    ui.label(egui::RichText::new(format!("{f}/")).small().color(Color32::from_gray(150)));
                                }
                                last_folder = folder.clone();
                            }
                            let shown = match name.rsplit_once('/') {
                                Some((_, short)) => format!("   {short}"),
                                None => name.clone(),
                            };
                            let r = graph_row(ui, &shown, outs, on, ("pkg", format!("{i}/{is_main}/{name}")));
                            let sub = if *is_main { None } else { Some(name.clone()) };
                            if r.dragged() {
                                if is_active {
                                    // a graph of this package becomes a node that runs it (when it may run here)
                                    r.dnd_set_drag_payload(format!("subgraph: {name}"));
                                } else {
                                    r.dnd_set_drag_payload(GraphDrag(i, name.clone(), sub.clone()));
                                }
                            }
                            if r.clicked() {
                                self.pkg_sel = i;
                                if !is_active {
                                    self.acts.push(Act::SwitchPkg(i));
                                } else {
                                    for _ in 0..self.parents.len() {
                                        self.acts.push(Act::ExitSub);
                                    }
                                }
                                if let Some(s) = sub.clone() {
                                    self.acts.push(Act::EnterSub(s));
                                }
                            }
                            let r = r.on_hover_text("Click to open - drag onto the canvas to use it as a node - right-click for more");
                            r.context_menu(|ui| {
                                self.pkg_sel = i;
                                if ui.button("Open").clicked() {
                                    if !is_active {
                                        self.acts.push(Act::SwitchPkg(i));
                                    } else {
                                        for _ in 0..self.parents.len() {
                                            self.acts.push(Act::ExitSub);
                                        }
                                    }
                                    if let Some(s) = sub.clone() {
                                        self.acts.push(Act::EnterSub(s));
                                    }
                                    ui.close();
                                }
                                ui.separator();
                                if ui.button("Export maps...").on_hover_text("Write this graph's outputs as image files").clicked() {
                                    self.acts.push(Act::ExportMaps(i, sub.clone()));
                                    ui.close();
                                }
                                if ui.button("Save as its own package (.sbs)...").on_hover_text("This graph in a package of its own, with the graphs and functions it uses").clicked() {
                                    self.acts.push(Act::ExportGraphFile(i, sub.clone()));
                                    ui.close();
                                }
                                if ui.button("Save as template...").on_hover_text("Make this graph a template of File > New, for new projects").clicked() {
                                    self.acts.push(Act::SaveAsTemplate(i, sub.clone()));
                                    ui.close();
                                }
                                ui.separator();
                                if ui.button("Duplicate").clicked() {
                                    self.acts.push(Act::DuplicateGraph(i, sub.clone()));
                                    ui.close();
                                }
                                // the graph a package opens on can be renamed too
                                if sub.is_none() && ui.button("Rename...").clicked() {
                                    self.acts.push(Act::RenameGraph(i, name.clone()));
                                    ui.close();
                                }
                                if let Some(s) = &sub {
                                    if ui.button("Rename...").clicked() {
                                        self.acts.push(Act::RenameGraph(i, s.clone()));
                                        ui.close();
                                    }
                                }
                                let tip = if sub.is_some() {
                                    "Only when no other graph uses it (Ctrl+Z brings it back)"
                                } else {
                                    "The package then opens on its next graph. Only when no other graph uses it (Ctrl+Z brings it back)"
                                };
                                if ui.button("Delete").on_hover_text(tip).clicked() {
                                    self.acts.push(Act::DeleteGraph(i, name.clone()));
                                    ui.close();
                                }
                            });
                        }
                        // the package's resources (bitmaps, fonts, SVG), as the file lists them
                        let root: &Graph = match self.pkgs.get(i) {
                            Some(Some(p)) => &p.g,
                            _ => self.parents.first().map(|p| &p.0).unwrap_or(&self.g),
                        };
                        let entries = materialcraft::sbs_native::package_resources(root);
                        if !entries.is_empty() {
                            ui.add_space(4.0);
                            ui.small("Resources");
                            for (name, how) in entries {
                                ui.label(egui::RichText::new(format!("      {name}")).color(Color32::from_gray(190))).on_hover_text(how);
                            }
                        }
                    });
            }
        });
    }

    // ------------------------------------------------------------ library
    fn lib_items(&self, search: &str) -> Vec<Entry> {
        let all = self.entries();
        let s = search.to_lowercase();
        if !s.is_empty() {
            return all.into_iter().filter(|e| e.label.to_lowercase().contains(&s) || e.doc.to_lowercase().contains(&s)).collect();
        }
        let cat = self.lib_cat.clone();
        // a function or pattern graph takes only its own family of nodes: all of them, whatever the folder
        if matches!(self.g.graph_kind(), "function" | "fxmap") && cat != "Favorites" {
            return all;
        }
        // the format's atomic nodes, the library graphs by what they make, function and FX-Map nodes
        all.into_iter()
            .filter(|e| {
                let k = e.kind.as_str();
                let structure = matches!(k, "sbs:input" | "sbs:output" | "sbs:graph" | "sbs:passthrough");
                match cat.as_str() {
                    "Favorites" => self.favorites.contains(&e.label),
                    "Structure" => structure,
                    "Core nodes" => !k.starts_with("sbs:lib:") && !k.starts_with("sbs:fn:") && !k.starts_with("sbs:fx:") && !structure,
                    "Logic nodes" => k.starts_with("sbs:fn:"),
                    "Pattern builders" => k.starts_with("sbs:fx:"),
                    c if k.starts_with("sbs:lib:") => {
                        let (m, sub) = sd_lib_group(k);
                        match c.split_once('/') {
                            Some((cm, cs)) => m == cm && sub == cs,
                            None => m == c,
                        }
                    }
                    _ => false,
                }
            })
            .collect()
    }

    fn library_ui(&mut self, ui: &mut egui::Ui, search: &mut String) {
        if panel_header(ui, "LIBRARY", true) {
            self.show[1] = false;
        }
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.add(egui::TextEdit::singleline(search).hint_text("Search").desired_width(170.0));
            if !search.is_empty() && ui.small_button("x").clicked() {
                search.clear();
            }
        });
        let (h, w) = (ui.available_height(), ui.available_width());
        let items = self.lib_items(search);
        let mut fav_toggle: Option<String> = None;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(158.0, h), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                egui::ScrollArea::vertical().id_salt("libtree").auto_shrink([false, false]).show(ui, |ui| {
                    for (name, subs) in LIB_TREE {
                        let sel = self.lib_cat == *name;
                        let icon = if *name == "Favorites" { "*  " } else { "" };
                        if subs.is_empty() {
                            if ui.selectable_label(sel, format!("{icon}{name}")).clicked() {
                                self.lib_cat = name.to_string();
                            }
                        } else {
                            let open = self.lib_cat.starts_with(name);
                            egui::CollapsingHeader::new(egui::RichText::new(*name).strong()).default_open(true).open(if open { Some(true) } else { None }).show(ui, |ui| {
                                for sub in *subs {
                                    let full = format!("{name}/{sub}");
                                    if ui.selectable_label(self.lib_cat == full, format!("   {sub}")).clicked() {
                                        self.lib_cat = full;
                                    }
                                }
                            });
                            let hdr_resp = ui.interact(ui.min_rect(), Id::new(("libcat", *name)), Sense::hover());
                            let _ = hdr_resp;
                            if sel {
                                // clicking the header text selects the whole group via the first sub-category
                            }
                        }
                    }
                });
            });
            ui.separator();
            ui.allocate_ui_with_layout(vec2((w - 168.0).max(80.0), h), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                egui::ScrollArea::vertical().id_salt("libgrid").auto_shrink([false, false]).show(ui, |ui| {
                    if items.is_empty() {
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new(match self.lib_cat.as_str() {
                            "Logic nodes" => "Function nodes: open a function graph (a value's fn button, or Explorer > New > Function) to add them.",
                            "Pattern builders" => "FX-Map nodes: open an FX-Map's pattern graph to add them.",
                            "Favorites" => "Right-click a node to add it to favorites.",
                            _ => "Nothing here.",
                        }).color(Color32::from_gray(130)));
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 6.0);
                        for e in &items {
                            let (rect, resp) = ui.allocate_exact_size(vec2(68.0, 94.0), Sense::click_and_drag());
                            let p = ui.painter();
                            if resp.hovered() {
                                p.rect_filled(rect, 3.0, Color32::from_rgb(58, 58, 58));
                            }
                            let tr = Rect::from_min_size(rect.min + vec2(2.0, 2.0), vec2(64.0, 64.0));
                            // core nodes show their icon, the others a picture of what they make
                            if !core_icon(p, tr, &e.kind) {
                                match self.lib_thumbs.get(&e.kind) {
                                    Some(t) => {
                                        p.image(t.id(), tr, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                                    }
                                    _ => {
                                        p.rect_filled(tr, 3.0, kind_color(&e.kind).gamma_multiply(0.55));
                                        draw_icon(p, tr.center(), entry_icon(&e.kind), Color32::WHITE);
                                    }
                                }
                            }
                            if self.favorites.contains(&e.label) {
                                draw_icon(p, tr.right_top() + vec2(-8.0, 8.0), Ic::Star, Color32::from_rgb(90, 220, 190));
                            }
                            let (l1, l2) = two_lines(&e.label);
                            p.text(pos2(rect.center().x, rect.min.y + 70.0), Align2::CENTER_TOP, l1, FontId::proportional(11.0), Color32::from_gray(225));
                            if !l2.is_empty() {
                                p.text(pos2(rect.center().x, rect.min.y + 82.0), Align2::CENTER_TOP, l2, FontId::proportional(11.0), Color32::from_gray(225));
                            }
                            if resp.drag_started() {
                                resp.dnd_set_drag_payload(e.label.clone());
                            }
                            let resp = resp.on_hover_text(format!("{}\n(drag onto the graph to add)", e.doc));
                            resp.context_menu(|ui| {
                                let fav = self.favorites.contains(&e.label);
                                if ui.button(if fav { "Remove from favorites" } else { "Add to favorites" }).clicked() {
                                    fav_toggle = Some(e.label.clone());
                                    ui.close();
                                }
                            });
                        }
                    });
                });
            });
        });
        if let Some(l) = fav_toggle {
            if !self.favorites.remove(&l) {
                self.favorites.insert(l);
            }
            self.save_settings();
        }
    }

    // ------------------------------------------------------------ 2D view
    fn view2d_panel(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
        let title = match self.active.and_then(|a| self.g.nodes.get(&a).map(|n| (a, materialcraft::sbs_native::label(n, &self.g)))) {
            Some((id, k)) => format!("{k} #{id} - 2D VIEW"),
            None => "2D VIEW".into(),
        };
        if panel_header(ui, &title, true) {
            self.show[3] = false;
        }
        let bottom_h = 24.0;
        let avail = vec2(ui.available_width(), (ui.available_height() - bottom_h).max(40.0));
        let (rect, resp) = ui.allocate_exact_size(avail, Sense::click_and_drag());
        self.view2d_rect = rect;
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(34, 34, 34));
        let fit = rect.width().min(rect.height()) - 20.0;
        if resp.hovered() {
            let sc = ui.input(|i| i.smooth_scroll_delta.y);
            if sc != 0.0 {
                self.z2d = (self.z2d * (1.0 + sc * 0.0015)).clamp(0.1, 16.0);
            }
        }
        if resp.dragged() {
            self.pan2d += resp.drag_delta();
        }
        if resp.double_clicked() {
            self.z2d = 1.0;
            self.pan2d = Vec2::ZERO;
        }
        let side = fit * self.z2d;
        // the image keeps the texture's own aspect ratio; `side` is its longer edge
        let aspect = if self.tex2d_size.0 > 0 && self.tex2d_size.1 > 0 { self.tex2d_size.1 as f32 / self.tex2d_size.0 as f32 } else { 1.0 };
        let img_size = if aspect <= 1.0 { vec2(side, side * aspect) } else { vec2(side / aspect, side) };
        let img = Rect::from_center_size(rect.center() + self.pan2d, img_size);
        // with the Alpha toggle off a picture with alpha is shown opaque; on, over a checkerboard
        let alpha = self.alpha2d && self.tex2d_opaque.is_some();
        let shown = if alpha { self.tex2d.as_ref() } else { self.tex2d_opaque.as_ref().or(self.tex2d.as_ref()) };
        if let Some(t) = shown {
            let uv = if self.tile_preview { Rect::from_min_max(pos2(-0.5, -0.5), pos2(1.5, 1.5)) } else { Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)) };
            if self.tile_preview {
                let big = Rect::from_center_size(img.center(), img.size() * 2.0);
                if alpha {
                    checker(&painter, big, rect);
                }
                painter.image(t.id(), big, Rect::from_min_max(pos2(0.0, 0.0), pos2(2.0, 2.0)), Color32::WHITE);
            } else {
                if alpha {
                    checker(&painter, img, rect);
                }
                painter.image(t.id(), img, uv, Color32::WHITE);
            }
            painter.rect_stroke(img, 0.0, Stroke::new(1.0_f32, Color32::from_gray(70)), egui::StrokeKind::Inside);
        } else {
            painter.text(rect.center(), Align2::CENTER_CENTER, "select a node to view its result", FontId::proportional(12.5), Color32::from_gray(120));
        }
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.small(format!("{} x {} (RGBA, {}bpc)", self.tex2d_size.0, self.tex2d_size.1, if self.g.meta.get("output_format").map(|s| s == "16").unwrap_or(false) { 16 } else { 8 }));
            if let Some((lo, hi)) = self.tex2d_range {
                if lo <= hi {
                    let hdr = hi > 1.0001 || lo < -0.0001;
                    let txt = egui::RichText::new(format!("range {lo:.3} .. {hi:.3}")).small().color(if hdr { Color32::from_rgb(255, 190, 90) } else { Color32::from_gray(150) });
                    ui.label(txt).on_hover_text(if hdr { "Values outside 0..1 (HDR). The view clips them; exports keep them in float formats (EXR, HDR, 32-bit TIFF)." } else { "Smallest and largest channel value of this result" });
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                let px = if self.tex2d_size.0 > 0 { img_size.x / self.tex2d_size.0 as f32 * 100.0 } else { 100.0 };
                ui.small(format!("{px:.2}%"));
                if icon_btn(ui, Ic::Frame, "Fit to view (double-click)", true).clicked() {
                    self.z2d = 1.0;
                    self.pan2d = Vec2::ZERO;
                }
                let before = self.tile_preview;
                if icon_btn(ui, Ic::Grid, "Tile 2x2 preview", true).clicked() {
                    self.tile_preview = !before;
                }
                if self.tex2d_opaque.is_some()
                    && ui
                        .selectable_label(self.alpha2d, egui::RichText::new("Alpha").small())
                        .on_hover_text("This result has an alpha channel. On: transparent pixels show a checkerboard. Off: the colours are shown fully opaque.")
                        .clicked()
                {
                    self.alpha2d = !self.alpha2d;
                }
                let shown = if self.prev_size == 0 { format!("{}px (graph)", self.preview_full()) } else { format!("{}px", self.prev_size) };
                egui::ComboBox::from_id_salt("psize").width(110.0).selected_text(shown).show_ui(ui, |ui| {
                    let b = self.prev_size;
                    ui.selectable_value(&mut self.prev_size, 0, "Graph size");
                    for s in [128usize, 256, 512, 1024, 2048, 4096] {
                        ui.selectable_value(&mut self.prev_size, s, format!("{s}px"));
                    }
                    if b != self.prev_size {
                        self.dirty = true;
                    }
                });
            });
        });
    }

    // ------------------------------------------------------------ 3D view
    fn view3d_panel(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
        let shape = render3d::shape_name(self.view3d.shape);
        let engine = if self.gpu_ready() { "GPU" } else { "CPU" };
        if panel_header(ui, &format!("{shape} - {engine} - 3D VIEW"), true) {
            self.show[2] = false;
        }
        let mut ch = false;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Scene", |ui| {
                for (k, (n, id)) in render3d::SHAPES.iter().enumerate() {
                    if k == 7 {
                        ui.separator();
                    }
                    if ui.selectable_label(self.view3d.shape == *id, *n).clicked() {
                        self.view3d.shape = *id;
                        ch = true;
                        ui.close();
                    }
                }
                ui.separator();
                ch |= ui.add(egui::Slider::new(&mut self.view3d.tiling, 0.25..=16.0).text("UV tiling").logarithmic(true)).changed();
                ui.separator();
                ui.label("Mesh resolution (triangles per side)");
                let tess_list: &[u32] = if self.gpu_ready() { &[64, 128, 256, 384, 512, 768] } else { &[64, 128, 256] };
                for &t in tess_list {
                    if ui.selectable_label(self.view3d.tess == t, format!("{t} x {t}")).clicked() {
                        self.view3d.tess = t;
                        ch = true;
                    }
                }
                ch |= ui.checkbox(&mut self.view3d.wireframe, "Show wireframe").changed();
                ui.separator();
                self.scene_extras(ui, &mut ch);
            });
            ui.menu_button("Materials", |ui| {
                self.ensure_maps();
                if self.maps.contains_key("height") {
                    ui.label("Height display");
                    for (i, n) in ["Normal map only", "Bump", "Parallax (outward)", "Displacement (real geometry)"].iter().enumerate() {
                        if ui.selectable_label(self.view3d.relief == i as u32, *n).clicked() {
                            self.view3d.relief = i as u32;
                            ch = true;
                        }
                    }
                    ui.separator();
                    if self.view3d.relief >= 2 {
                        ch |= ui.add(egui::Slider::new(&mut self.view3d.height_scale, 0.0..=0.5).text("Height scale")).changed();
                    }
                    if self.view3d.relief == 1 {
                        ch |= ui.add(egui::Slider::new(&mut self.view3d.bump, 0.0..=12.0).text("Bump strength")).changed();
                    }
                    if self.view3d.relief >= 2 {
                        ch |= ui.add(egui::Slider::new(&mut self.view3d.height_level, 0.0..=1.0).text("Neutral level (0 = all outward)")).changed();
                    }
                } else {
                    ui.colored_label(Color32::from_rgb(255, 170, 120), "No Height output - add an Output node with usage Height.");
                }
                ch |= ui.add(egui::Slider::new(&mut self.view3d.normal_amt, 0.0..=1.0).text("Normal map strength")).changed();
                ui.separator();
                ui.small("Maps used by the viewer:");
                for k in self.maps.keys() {
                    ui.label(format!("  {k}"));
                }
            });
            self.view3d_menus(ui, &mut ch);
            self.view3d_source(ui, &mut ch);
            ui.menu_button("Renderer", |ui| {
                if ui.checkbox(&mut self.gpu_on, "GPU rendering (wgpu)").changed() {
                    self.gpu_maps_dirty = true;
                    ch = true;
                }
                let (err, name) = self.gpu.lock().map(|g| (g.error.clone(), g.renderer.clone())).unwrap_or((None, String::new()));
                if let Some(e) = err {
                    ui.colored_label(Color32::from_rgb(255, 170, 120), format!("GPU renderer unavailable: {e}"));
                    ui.label("Using the CPU software renderer.");
                } else if self.gpu_on {
                    if !name.is_empty() {
                        ui.small(format!("GPU: {name}"));
                    }
                    ui.small("Real displacement, parallax relief, GGX PBR, mipmaps + anisotropic filtering, MSAA");
                    if let Ok(mut g) = self.gpu.lock() {
                        if ui.checkbox(&mut g.opts.self_shadow, "Height self-shadowing").changed() {
                            ch = true;
                        }
                        ui.menu_button("Texture sampling", |ui| {
                            let o = &mut g.opts;
                            ui.label("Filtering");
                            for (v, n) in [(0u8, "Nearest (pixelated)"), (1, "Bilinear"), (2, "Trilinear (smooth mip levels)")] {
                                ch |= ui.radio_value(&mut o.filter, v, n).changed();
                            }
                            ui.separator();
                            ch |= ui.checkbox(&mut o.mipmaps, "Mip maps").on_hover_text("Smaller copies of the texture for far away or slanted surfaces: no shimmering").changed();
                            ui.separator();
                            ui.label("Anisotropic filtering");
                            for (v, n) in [(1.0_f32, "Off"), (2.0, "2x"), (4.0, "4x"), (8.0, "8x"), (16.0, "16x"), (0.0, "Maximum")] {
                                ch |= ui.radio_value(&mut o.aniso, v, n).changed();
                            }
                            ui.separator();
                            ui.label("Outside the texture (UV tiling)");
                            for (v, n) in [(0u8, "Repeat"), (1, "Mirrored repeat"), (2, "Clamp to edge")] {
                                ch |= ui.radio_value(&mut o.wrap, v, n).changed();
                            }
                            ui.separator();
                            ch |= ui.add(egui::Slider::new(&mut o.lod_bias, -2.0..=3.0).text("Mip bias")).on_hover_text("Positive blurs the texture, negative sharpens it (and may shimmer)").changed();
                            if ui.button("Reset sampling").clicked() {
                                let d = materialcraft::gpu3d::GpuOpts::default();
                                (o.filter, o.mipmaps, o.aniso, o.wrap, o.lod_bias) = (d.filter, d.mipmaps, d.aniso, d.wrap, d.lod_bias);
                                ch = true;
                            }
                        });
                    }
                }
                {
                    let out = self.target_3d_auto();
                    let label = if self.mesh_tex_px == 0 { format!("3D texture size: output ({out} px)") } else { format!("3D texture size: {} px", self.mesh_tex_px) };
                    ui.menu_button(label, |ui| {
                        let b = self.mesh_tex_px;
                        ui.selectable_value(&mut self.mesh_tex_px, 0, format!("Output resolution ({out} px)")).on_hover_text("The size of your output nodes");
                        for s in [256usize, 512, 1024, 2048, 4096] {
                            ui.selectable_value(&mut self.mesh_tex_px, s, format!("{s} px"));
                        }
                        if b != self.mesh_tex_px {
                            self.dirty = true;
                            ui.close();
                        }
                    });
                }
                self.render_extras(ui, &mut ch);
                if !self.gpu_on || self.v3.pt_on {
                    ui.separator();
                    ui.label("CPU software PBR (GGX, normal mapping)");
                    let b = self.r3d_size;
                    for s in [256usize, 420, 640] {
                        ui.selectable_value(&mut self.r3d_size, s, format!("{s} px"));
                    }
                    if b != self.r3d_size {
                        ch = true;
                    }
                }
            });
        });
        if ch {
            self.dirty3d = true;
        }
        let bottom_h = 24.0;
        let avail = vec2(ui.available_width(), (ui.available_height() - bottom_h).max(40.0));
        let (rect, resp) = ui.allocate_exact_size(avail, Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(30, 30, 30));
        let side = rect.width().min(rect.height());
        let img_rect = Rect::from_center_size(rect.center(), vec2(side, side));
        if resp.dragged() {
            let d = resp.drag_delta();
            let (alt, shift) = ui.input(|i| (i.modifiers.alt, i.modifiers.shift));
            if alt || resp.dragged_by(egui::PointerButton::Middle) {
                let k = 0.004 * self.view3d.dist / 3.4;
                self.view3d.pan_x -= d.x * k;
                self.view3d.pan_y += d.y * k;
                self.view3d.pan_x = self.view3d.pan_x.clamp(-3.0, 3.0);
                self.view3d.pan_y = self.view3d.pan_y.clamp(-3.0, 3.0);
            } else if shift {
                let sel = self.v3.sel_light.min(self.view3d.lights.len().saturating_sub(1));
                if let Some(l) = self.view3d.lights.get_mut(sel) {
                    l.yaw -= d.x * 0.01;
                    l.pitch = (l.pitch + d.y * 0.01).clamp(-1.4, 1.4);
                }
            } else {
                self.view3d.yaw -= d.x * 0.01;
                self.view3d.pitch = (self.view3d.pitch + d.y * 0.01).clamp(-1.4, 1.4);
            }
            self.dirty3d = true;
        }
        if resp.hovered() {
            let sc = ui.input(|i| i.smooth_scroll_delta.y);
            if sc != 0.0 {
                self.view3d.dist = (self.view3d.dist * (1.0 - sc * 0.001)).clamp(1.6, 9.0);
                self.dirty3d = true;
            }
        }
        self.view3d_tick(ui.ctx());
        if self.gpu_ready() && !self.v3.pt_on {
            // drawn on the device that holds the maps; only the finished picture comes back (and only when it changed)
            let ppp = ui.ctx().pixels_per_point();
            let (pw, ph) = (((rect.width() * ppp).round() as usize).max(1), ((rect.height() * ppp).round() as usize).max(1));
            match gpu3d::frame(&self.gpu, pw, ph, &self.view3d) {
                Some(f) => {
                    if f.seq != self.gpu_seq || self.tex_gpu.is_none() {
                        let img = ColorImage::from_rgba_unmultiplied([f.w, f.h], &f.rgba);
                        match &mut self.tex_gpu {
                            Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                            None => self.tex_gpu = Some(ui.ctx().load_texture("view3d_gpu", img, egui::TextureOptions::LINEAR)),
                        }
                        self.gpu_seq = f.seq;
                    }
                    if let Some(t) = &self.tex_gpu {
                        ui.painter().image(t.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                }
                // the device refused: the next frame shows the software renderer
                None => self.dirty3d = true,
            }
        } else if let Some(t) = &self.tex3d {
            ui.painter().image(t.id(), img_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        if self.v3.pt_on {
            let msg = format!("Path tracing: {} / {} samples", self.v3.pt_samples, self.v3.pt_target);
            ui.painter().text(rect.right_top() + vec2(-8.0, 6.0), Align2::RIGHT_TOP, msg, FontId::proportional(11.0), Color32::from_gray(170));
        }
        ui.painter().text(rect.left_bottom() + vec2(6.0, -5.0), Align2::LEFT_BOTTOM, "drag: orbit   Alt+drag / middle: move   Shift+drag: light   wheel: zoom", FontId::proportional(10.0), Color32::from_gray(110));
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.small("sRGB (default)");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                let before = self.view3d.shape;
                egui::ComboBox::from_id_salt("shape3d").width(120.0).selected_text(shape).show_ui(ui, |ui| {
                    for (n, id) in render3d::SHAPES.iter() {
                        ui.selectable_value(&mut self.view3d.shape, *id, *n);
                    }
                });
                if before != self.view3d.shape {
                    self.dirty3d = true;
                }
            });
        });
    }

    // ------------------------------------------------------------ graph properties
    fn graph_props(&mut self, ui: &mut egui::Ui) {
        self.sd_graph_props(ui);
    }

    // ------------------------------------------------------------ editor layout
    fn editor_ui(&mut self, root: &mut egui::Ui) {
        let ctx = &root.ctx().clone();
        let mut acts: Vec<Act> = Vec::new();
        let in_sub = !self.parents.is_empty();
        let (can_undo, can_redo, has_sel, has_clip) = (!self.undo.is_empty(), !self.redo.is_empty(), !self.sel.is_empty(), !self.clip.is_empty());
        let panel_fill = Color32::from_rgb(40, 40, 40);

        egui::Panel::top("menu").frame(egui::Frame::NONE.fill(Color32::from_rgb(34, 34, 34)).inner_margin(egui::Margin::symmetric(6, 2))).show(root, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    ui.menu_button("New", |ui| {
                        if ui.button("Material Graph...   Ctrl+N").clicked() {
                            acts.push(Act::NewGraph);
                            ui.close();
                        }
                        if ui.button("Empty Package...").clicked() {
                            acts.push(Act::NewPackage);
                            ui.close();
                        }
                    });
                    for (label, a) in [("Open package(s)...   Ctrl+O", Act::Open), ("Save   Ctrl+S", Act::Save), ("Save package as...", Act::SaveAs)] {
                        if ui.button(label).clicked() {
                            acts.push(a);
                            ui.close();
                        }
                    }
                    ui.separator();
                    ui.menu_button("New from template", |ui| {
                        for (ti, t) in TEMPLATES.iter().enumerate() {
                            if ui.button(t.name).on_hover_text(t.desc).clicked() {
                                acts.push(Act::NewFromTemplate(ti));
                                ui.close();
                            }
                        }
                        let mine = user_templates::list(&templates_dir());
                        if !mine.is_empty() {
                            ui.separator();
                            ui.label(egui::RichText::new("My templates").small().color(Color32::from_gray(160)));
                            for t in mine {
                                if ui.button(&t.name).on_hover_text(t.path.display().to_string()).clicked() {
                                    acts.push(Act::NewFromUserTemplate(t.path.clone()));
                                    ui.close();
                                }
                            }
                        }
                    });
                    if ui.button("Save graph as template...").on_hover_text("Make the graph on screen a template of File > New, for new projects").clicked() {
                        acts.push(Act::SaveAsTemplate(self.pkg_active, self.parents.last().map(|(_, n)| n.clone())));
                        ui.close();
                    }
                    ui.menu_button("Package", |ui| {
                        let (pi, cur) = (self.pkg_active, self.parents.last().map(|(_, n)| n.clone()));
                        let items: Vec<(&str, Act)> = vec![
                            ("Save this graph as its own .sbs...", Act::ExportGraphFile(pi, cur.clone())),
                            ("Add the graphs of another .sbs...", Act::AddGraphFile(pi)),
                            ("Dependencies...", Act::ToggleDeps),
                        ];
                        for (label, a) in items {
                            if ui.button(label).clicked() {
                                acts.push(a);
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
                    for (label, a) in [("Export textures...", Act::Export), ("Export MaterialX (Unreal Engine)...", Act::ExportMaterialX)] {
                        if ui.button(label).clicked() {
                            acts.push(a);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button("Home screen").clicked() {
                        acts.push(Act::Home);
                        ui.close();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.add_enabled(can_undo, egui::Button::new("Undo   Ctrl+Z")).clicked() {
                        acts.push(Act::Undo);
                        ui.close();
                    }
                    if ui.add_enabled(can_redo, egui::Button::new("Redo   Ctrl+Y")).clicked() {
                        acts.push(Act::Redo);
                        ui.close();
                    }
                    ui.separator();
                    if ui.add_enabled(self.g.sd.is_some(), egui::Button::new("New spline shape...")).on_hover_text("An SVG node with a drawable spline: connect it to Bevel, Distance, Blur ...").clicked() {
                        acts.push(Act::NewSpline);
                        ui.close();
                    }
                    ui.separator();
                    for (label, a, en) in [
                        ("Cut   Ctrl+X", Act::Cut, has_sel),
                        ("Copy   Ctrl+C", Act::Copy, has_sel),
                        ("Paste   Ctrl+V", Act::Paste(None), has_clip),
                        ("Duplicate   Ctrl+D", Act::Duplicate, has_sel),
                        ("Delete   Del", Act::Delete, true),
                        ("Select all   Ctrl+A", Act::SelectAll, true),
                    ] {
                        if ui.add_enabled(en, egui::Button::new(label)).clicked() {
                            acts.push(a);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button("Add node...   Tab").clicked() {
                        acts.push(Act::Search(None));
                        ui.close();
                    }
                    if ui.button("Comment frame   C").clicked() {
                        acts.push(Act::Comment);
                        ui.close();
                    }
                    ui.separator();
                    let mut snap_changed = ui.checkbox(&mut self.snap_nodes, "Snap nodes to grid").on_hover_text("Dragged nodes land on the grid below").changed();
                    ui.menu_button(format!("Grid size: {}", self.snap_grid.round() as u32), |ui| {
                        for g in [4.0_f32, 8.0, 16.0, 32.0, 64.0, 128.0] {
                            if ui.radio(self.snap_grid == g, format!("{} units", g as u32)).clicked() {
                                self.snap_grid = g;
                                snap_changed = true;
                                ui.close();
                            }
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Custom");
                            if ui.add(egui::DragValue::new(&mut self.snap_grid).range(2.0..=256.0).speed(0.5).max_decimals(0)).changed() {
                                self.snap_grid = self.snap_grid.round().max(2.0);
                                snap_changed = true;
                            }
                        });
                    });
                    if snap_changed {
                        self.save_settings();
                    }
                });
                ui.menu_button("Tools", |ui| {
                    if ui.button("Mesh map baker...").clicked() {
                        acts.push(Act::Bake);
                        ui.close();
                    }
                    if ui.button("Frame all   F").clicked() {
                        acts.push(Act::FrameAll);
                        ui.close();
                    }
                    if ui.button("Auto-layout").clicked() {
                        acts.push(Act::AutoLayout);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("AI control...").on_hover_text("Let an AI assistant build and edit the open graph").clicked() {
                        self.show_ai = true;
                        ui.close();
                    }
                    match materialcraft::gpu::adapter_name() {
                        Some(name) => {
                            if ui.checkbox(&mut self.gpu_eval, "GPU acceleration").on_hover_text(format!("Large noises, blurs and normal maps run on {name}. Results match the CPU path within rounding.")).changed() {
                                materialcraft::gpu::set_enabled(self.gpu_eval);
                                self.dirty = true;
                            }
                        }
                        None => {
                            ui.add_enabled(false, egui::Button::new("GPU acceleration (no suitable adapter)"));
                        }
                    }
                });
                ui.menu_button("Windows", |ui| {
                    for (i, n) in ["Explorer", "Library", "3D View", "2D View", "Properties"].iter().enumerate() {
                        ui.checkbox(&mut self.show[i], *n);
                    }
                    ui.separator();
                    if ui.button("Reset layout").clicked() {
                        self.show = [true; 5];
                        self.dock = default_dock();
                        self.dock_closed.clear();
                        ui.close();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("Keyboard shortcuts").clicked() {
                        self.show_about = true;
                        ui.close();
                    }
                    ui.separator();
                    ui.small(self.logo_note.as_str());
                });
            });
        });
        // one tab per open package; clicking switches the editor to it, the cross closes it
        if self.pkgs.len() > 0 {
            egui::Panel::top("tabs").frame(egui::Frame::NONE.fill(Color32::from_rgb(30, 30, 30)).inner_margin(egui::Margin::symmetric(4, 2))).show(root, |ui| {
                egui::ScrollArea::horizontal().id_salt("tabs_scroll").auto_shrink([false, true]).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for i in 0..self.pkgs.len() {
                            let active = i == self.pkg_active;
                            let title = format!("{}{}", self.pkg_title(i), if self.pkg_modified(i) { "*" } else { "" });
                            let fill = if active { Color32::from_rgb(58, 58, 58) } else { Color32::from_rgb(40, 40, 40) };
                            let txt = egui::RichText::new(title).color(if active { Color32::WHITE } else { Color32::from_gray(170) });
                            let r = ui.add(egui::Button::new(txt).fill(fill).min_size(vec2(90.0, 20.0)));
                            if r.clicked() && !active {
                                acts.push(Act::SwitchPkg(i));
                            }
                            if r.middle_clicked() {
                                acts.push(Act::ClosePkg(i));
                            }
                            if ui.add(egui::Button::new(egui::RichText::new("x").color(Color32::from_gray(150))).fill(fill).min_size(vec2(18.0, 20.0))).on_hover_text("Close package").clicked() {
                                acts.push(Act::ClosePkg(i));
                            }
                            ui.add_space(4.0);
                        }
                        if ui.add(egui::Button::new("+").min_size(vec2(22.0, 20.0))).on_hover_text("New empty package").clicked() {
                            acts.push(Act::NewPackage);
                        }
                    });
                });
                // one tab per open graph of the active package: the main graph first, then every graph opened from it
                let root_names: BTreeSet<String> = self.parents.first().map(|p| &p.0).unwrap_or(&self.g).subgraphs.keys().cloned().collect();
                let open: Vec<String> = self.graph_tabs.iter().filter(|n| root_names.contains(*n)).cloned().collect();
                if !open.is_empty() {
                    let current = self.parents.last().map(|(_, n)| n.clone()).unwrap_or_default();
                    egui::ScrollArea::horizontal().id_salt("gtabs_scroll").auto_shrink([false, true]).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for name in std::iter::once(String::new()).chain(open.iter().cloned()) {
                                let active = current == name;
                                let fill = if active { Color32::from_rgb(50, 62, 60) } else { Color32::from_rgb(36, 36, 36) };
                                let label = if name.is_empty() { "Main graph".to_string() } else { name.clone() };
                                let txt = egui::RichText::new(label).small().color(if active { ACCENT } else { Color32::from_gray(160) });
                                if ui.add(egui::Button::new(txt).fill(fill).min_size(vec2(70.0, 18.0))).clicked() && !active {
                                    acts.push(Act::OpenGraphTab(name.clone()));
                                }
                                if !name.is_empty() && ui.add(egui::Button::new(egui::RichText::new("x").small().color(Color32::from_gray(140))).fill(fill).min_size(vec2(16.0, 18.0))).on_hover_text("Close graph tab").clicked() {
                                    acts.push(Act::CloseGraphTab(name.clone()));
                                }
                                ui.add_space(3.0);
                            }
                        });
                    });
                }
            });
        }
        egui::Panel::top("toolbar").frame(egui::Frame::NONE.fill(Color32::from_rgb(38, 38, 38)).inner_margin(egui::Margin::symmetric(6, 1))).show(root, |ui| {
            ui.horizontal(|ui| {
                for (ic, tip, a, en) in [
                    (Ic::New, "New material graph (Ctrl+N)", Act::NewGraph, true),
                    (Ic::Open, "Open (Ctrl+O)", Act::Open, true),
                    (Ic::Save, "Save (Ctrl+S)", Act::Save, true),
                ] {
                    if icon_btn(ui, ic, tip, en).clicked() {
                        acts.push(a);
                    }
                }
                ui.separator();
                if icon_btn(ui, Ic::Undo, "Undo (Ctrl+Z)", can_undo).clicked() {
                    acts.push(Act::Undo);
                }
                if icon_btn(ui, Ic::Redo, "Redo (Ctrl+Y)", can_redo).clicked() {
                    acts.push(Act::Redo);
                }
            });
        });
        egui::Panel::bottom("status").frame(egui::Frame::NONE.fill(Color32::from_rgb(30, 30, 30)).inner_margin(egui::Margin::symmetric(8, 2))).show(root, |ui| {
            ui.horizontal(|ui| {
                let (msg, ok) = &self.status;
                ui.colored_label(if *ok { Color32::from_gray(170) } else { Color32::from_rgb(255, 120, 110) }, msg);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
                    let engine = match materialcraft::gpu::adapter_name().filter(|_| self.gpu_eval) {
                        Some(a) => format!("CPU ({threads} threads) + GPU compute: {a}"),
                        None => format!("CPU ({threads} threads)"),
                    };
                    ui.label(format!("MaterialCraft Engine: {engine}{}    Version: {}", if self.job.is_some() { "   computing..." } else { "" }, env!("CARGO_PKG_VERSION")));
                });
            });
        });

        // keep the panel layout and the Windows menu in step
        for pn in Pn::ALL {
            let Some(i) = pn.show_index() else { continue };
            let present = self.dock.contains(&pn);
            if !self.show[i] && present {
                if let Ok(loc) = self.dock.location(&pn) {
                    self.dock_closed.insert(pn, loc);
                }
                let _ = self.dock.apply(craft_ui::docking::Action::Close { panel: pn });
            } else if self.show[i] && !present {
                let restored = match self.dock_closed.get(&pn).cloned() {
                    Some(loc) => self.dock.restore(pn, &loc).is_ok(),
                    None => false,
                };
                if !restored {
                    let _ = self.dock.apply(craft_ui::docking::Action::OpenAt {
                        panel: pn,
                        anchor: Pn::Graph,
                        placement: craft_ui::docking::Placement::Split(craft_ui::docking::Zone::Right),
                    });
                }
            }
        }
        let mut layout = std::mem::take(&mut self.dock);
        let before = layout.clone();
        let mut dock_actions: Vec<craft_ui::docking::Action<Pn>> = Vec::new();
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(panel_fill)).show(root, |ui| {
            let mut style = craft_ui::docking::DockStyle::from_ui(ui);
            style.tab_height = 22.0;
            style.gap = 3.0;
            style.background = Color32::from_rgb(24, 24, 24);
            style.tab_background = Color32::from_rgb(30, 30, 30);
            style.active_background = Color32::from_rgb(52, 52, 52);
            style.accent = ACCENT;
            style.text = Color32::from_gray(230);
            style.inactive_text = Color32::from_gray(150);
            let out = craft_ui::docking::DockArea::new(Id::new("editor_dock")).show_with_limits(
                ui,
                &layout,
                &style,
                |p| p.label().to_string(),
                |p| if *p == Pn::Graph { craft_ui::docking::Permissions::PROTECTED } else { craft_ui::docking::Permissions::default() },
                |p| match p {
                    Pn::Graph => craft_ui::docking::PanelLimits { min: vec2(240.0, 160.0), ..Default::default() },
                    _ => craft_ui::docking::PanelLimits { min: vec2(120.0, 80.0), ..Default::default() },
                },
                |ui, p| match p {
                    Pn::Explorer => self.explorer_ui(ui),
                    Pn::Library => {
                        let mut s = std::mem::take(&mut self.lib_search);
                        self.library_ui(ui, &mut s);
                        self.lib_search = s;
                    }
                    Pn::View3d => self.view3d_panel(ui),
                    Pn::View2d => self.view2d_panel(ui),
                    Pn::Props => {
                let title = match self.active.filter(|a| self.sel.contains(a)).and_then(|a| self.g.nodes.get(&a).map(|n| materialcraft::sbs_native::label(n, &self.g))) {
                    Some(k) => format!("{} - PROPERTIES", k.to_uppercase()),
                    None => format!("{} - PROPERTIES", self.gname()),
                };
                panel_header(ui, &title, false);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(4.0);
                    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 0)).show(ui, |ui| self.inspector(ui));
                });
                    }
                    Pn::Graph => {
            panel_header(ui, &format!("{} - GRAPH", self.gname()), false);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                for (ic, tip, a, en) in [
                    (Ic::Frame, "Frame all (F)", Act::FrameAll, true),
                    (Ic::Layout, "Auto-layout", Act::AutoLayout, true),
                    (Ic::Comment, "Comment frame (C)", Act::Comment, true),
                ] {
                    if icon_btn(ui, ic, tip, en).clicked() {
                        acts.push(a);
                    }
                }
                ui.separator();
                ui.label("Parent Size:");
                let before = self.g.size;
                egui::ComboBox::from_id_salt("tb_size").width(70.0).selected_text(format!("{}", self.g.size)).show_ui(ui, |ui| {
                    for s in [128usize, 256, 512, 1024, 2048, 4096, 8192] {
                        ui.selectable_value(&mut self.g.size, s, format!("{s}"));
                    }
                });
                if before != self.g.size {
                    self.dirty = true;
                    self.modified = true;
                }
                let linked = self.g.height == 0;
                let (r, resp) = ui.allocate_exact_size(vec2(24.0, 22.0), Sense::click());
                ui.painter().rect_filled(r, 3.0, if linked { Color32::from_rgb(70, 70, 70) } else { Color32::from_rgb(50, 50, 50) });
                draw_icon(ui.painter(), r.center(), Ic::Link, if linked { Color32::WHITE } else { Color32::from_gray(110) });
                if resp.on_hover_text(if linked { "Width and height are linked (click to unlink)" } else { "Width and height are unlinked (click to link)" }).clicked() {
                    // unlinking keeps the current square size; the height is then edited on its own
                    self.g.height = if linked { self.g.size } else { 0 };
                    self.dirty = true;
                    self.modified = true;
                }
                if linked {
                    ui.add_enabled(false, egui::Button::new(format!("{}", self.g.size)).min_size(vec2(70.0, 20.0)));
                } else {
                    let hbefore = self.g.height;
                    egui::ComboBox::from_id_salt("tb_height").width(70.0).selected_text(format!("{}", self.g.height)).show_ui(ui, |ui| {
                        for s in [128usize, 256, 512, 1024, 2048, 4096, 8192] {
                            ui.selectable_value(&mut self.g.height, s, format!("{s}"));
                        }
                    });
                    if hbefore != self.g.height {
                        self.dirty = true;
                        self.modified = true;
                    }
                }
                ui.separator();
                if in_sub && ui.button("< Back").clicked() {
                    acts.push(Act::ExitSub);
                }
                let mut path = String::new();
                for (_, n) in &self.parents {
                    path.push_str(&format!(" > {n}"));
                }
                if !path.is_empty() {
                    ui.strong(path);
                }
                if has_sel {
                    ui.small(format!("{} selected", self.sel.len()));
                }
            });
            // common nodes, one click away (Designer keeps the same row above the graph)
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
                ui.add_space(4.0);
                for (label, _) in COMMON_NODES {
                    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 22.0), Sense::click_and_drag());
                    let (tile, glyph) = node_tile_colors(label);
                    let tile = if resp.hovered() { tile.gamma_multiply(1.35) } else { tile };
                    ui.painter().rect_filled(rect.shrink(1.0), 0.0, tile);
                    node_glyph(ui.painter(), rect.shrink(1.0), label, glyph);
                    // a click adds the node after the selected one; a drag drops it where it lands on the graph
                    if resp.drag_started() {
                        resp.dnd_set_drag_payload(label.to_string());
                    }
                    if resp.on_hover_text(format!("{label}\n(click to add, or drag onto the graph)")).clicked() {
                        acts.push(Act::QuickAdd(label));
                    }
                }
            });
            self.canvas(ui);
            if self.need_fit {
                self.need_fit = false;
                self.fit_view();
            }
                    }
                },
            );
            dock_actions = out.actions;
        });
        for a in dock_actions {
            if let craft_ui::docking::Action::Close { panel } = &a {
                if let Some(i) = panel.show_index() {
                    self.show[i] = false;
                }
                if let Ok(loc) = layout.location(panel) {
                    self.dock_closed.insert(*panel, loc);
                }
            }
            let _ = layout.apply_with_permissions(a, |p| if *p == Pn::Graph { craft_ui::docking::Permissions::PROTECTED } else { craft_ui::docking::Permissions::default() });
        }
        self.dock = layout;
        if self.dock != before {
            let l = self.dock.clone();
            self.prefs.update(|v| v.layout.dock = Some(l));
        }

        self.windows(ctx);
        self.search_ui(ctx);
        self.acts.extend(acts);

        if let Some(p) = egui::DragAndDrop::payload::<GraphDrag>(ctx) {
            if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("dnd_ghost_graph")));
                let text = format!("{} (graph)", p.1);
                let r = painter.text(pos + vec2(14.0, 14.0), Align2::LEFT_TOP, text.clone(), FontId::proportional(12.0), Color32::WHITE);
                painter.rect_filled(r.expand(4.0), 3.0, Color32::from_rgba_unmultiplied(20, 20, 20, 200));
                painter.text(pos + vec2(14.0, 14.0), Align2::LEFT_TOP, text, FontId::proportional(12.0), Color32::WHITE);
            }
        }
        if let Some(p) = egui::DragAndDrop::payload::<String>(ctx) {
            if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("dnd_ghost")));
                let shown = p.strip_prefix("subgraph: ").map(|g| format!("{g} (graph)")).unwrap_or_else(|| (*p).clone());
                let r = painter.text(pos + vec2(14.0, 14.0), Align2::LEFT_TOP, shown.clone(), FontId::proportional(12.0), Color32::WHITE);
                painter.rect_filled(r.expand(4.0), 3.0, Color32::from_rgba_unmultiplied(20, 20, 20, 200));
                painter.text(pos + vec2(14.0, 14.0), Align2::LEFT_TOP, shown, FontId::proportional(12.0), Color32::WHITE);
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        self.sync_search_dirs();
        // packages dropped from the OS onto the window open in the workspace
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for p in dropped {
            self.open_path(p);
        }
        // background asset results
        if let Some(rx) = &self.banner_rx {
            if let Ok(img) = rx.try_recv() {
                let mut o = egui::TextureOptions::LINEAR;
                o.wrap_mode = egui::TextureWrapMode::Repeat;
                self.banner = Some(ctx.load_texture("banner", img, o));
                self.banner_rx = None;
            }
        }
        if let Some(rx) = &self.lib_rx {
            // the library's pictures arrive in batches until the worker is done
            loop {
                match rx.try_recv() {
                    Ok(m) => {
                        for (k, img) in m {
                            self.lib_thumbs.insert(k.clone(), ctx.load_texture(format!("lib_{k}"), img, egui::TextureOptions::LINEAR));
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.lib_rx = None;
                        break;
                    }
                }
            }
        }
        if self.banner_rx.is_some() || self.lib_rx.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }

        let title = format!("MaterialCraft - {}{}", self.package_label(), if self.modified { "*" } else { "" });
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }

        if let Some(port) = AUTO_CONTROL.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
            self.ctl_start(ctx, port);
        }
        self.poll_control(ctx);
        self.ai_window(ctx);
        self.import_window(ctx);
        self.spline_editor(ctx);
        if self.screen == Screen::Home {
            if !self.text_focus && !self.new_dlg.open {
                let (newg, open) = ctx.input_mut(|i| (i.consume_key(Modifiers::COMMAND, Key::N), i.consume_key(Modifiers::COMMAND, Key::O)));
                if newg {
                    self.new_dlg.open = true;
                } else if open {
                    self.open_file();
                }
            }
            self.home_ui(root);
            self.new_dialog(ctx);
            self.windows(ctx);
            if self.screen == Screen::Editor {
                ctx.request_repaint();
            }
            self.text_focus = ctx.egui_wants_keyboard_input();
            // settings changed here (recent files, skip / tip options) are saved shortly after, like in the editor
            let tick = self.prefs.tick(std::time::Instant::now());
            if let Some(deadline) = tick.next_deadline {
                ctx.request_repaint_after(deadline.saturating_duration_since(std::time::Instant::now()));
            }
            return;
        }

        if !ctx.input(|i| i.pointer.any_down()) {
            self.edit_open = false;
        }
        self.poll_eval(ctx);
        self.poll_bake(ctx);
        if self.dirty3d {
            self.render_3d(ctx);
        }

        // keyboard shortcuts
        let pointer = ctx.input(|i| i.pointer.hover_pos()).filter(|p| self.canvas_rect.contains(*p));
        if self.search.is_some() {
            ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab));
        } else if !self.text_focus && !self.new_dlg.open {
            // Tab or Space opens the node search at the pointer, as in Designer
            let tab = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab) | i.consume_key(Modifiers::NONE, Key::Space));
            if tab {
                ctx.memory_mut(|m| {
                    if let Some(id) = m.focused() {
                        m.surrender_focus(id);
                    }
                });
                self.open_search(pointer.unwrap_or_else(|| self.canvas_rect.center()), None);
            }
        }
        if self.search.is_none() && !self.text_focus && !self.new_dlg.open {
            let k = ctx.input_mut(|i| {
                let redo = i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y);
                let undo = i.consume_key(Modifiers::COMMAND, Key::Z);
                let copy = i.events.iter().any(|e| matches!(e, egui::Event::Copy)) || i.consume_key(Modifiers::COMMAND, Key::C);
                let cut = i.events.iter().any(|e| matches!(e, egui::Event::Cut)) || i.consume_key(Modifiers::COMMAND, Key::X);
                let paste = i.consume_key(Modifiers::COMMAND, Key::V);
                let pasted: Option<String> = i.events.iter().find_map(|e| if let egui::Event::Paste(t) = e { Some(t.clone()) } else { None });
                let dup_nl = i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::D);
                let paste_nl = i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::V);
                let disable = i.consume_key(Modifiers::SHIFT, Key::D);
                let relink = i.consume_key(Modifiers::NONE, Key::Backspace);
                let clear_th = i.consume_key(Modifiers::NONE, Key::F5);
                let dup = i.consume_key(Modifiers::COMMAND, Key::D);
                let all = i.consume_key(Modifiers::COMMAND, Key::A);
                let save = i.consume_key(Modifiers::COMMAND, Key::S);
                let open = i.consume_key(Modifiers::COMMAND, Key::O);
                let newg = i.consume_key(Modifiers::COMMAND, Key::N);
                let del = i.consume_key(Modifiers::NONE, Key::Delete);
                let frame = i.consume_key(Modifiers::NONE, Key::F);
                let com = i.consume_key(Modifiers::NONE, Key::C);
                let swap = i.consume_key(Modifiers::NONE, Key::X);
                (redo, undo, copy, cut, paste, dup, all, save, open, del, frame, com, newg, pasted, dup_nl, paste_nl, disable, relink, clear_th, swap)
            });
            let w = pointer.map(|p| self.screen_to_world(p));
            if let Some(text) = &k.13 {
                if let Some(body) = text.strip_prefix("MaterialCraft nodes\n") {
                    if let Ok(g) = Graph::parse_unchecked(body) {
                        self.clip = g.nodes.into_iter().collect();
                        self.acts.push(Act::Paste(w));
                    }
                }
            }
            for (on, a) in [
                (k.0, Act::Redo),
                (k.1, Act::Undo),
                (k.2, Act::Copy),
                (k.3, Act::Cut),
                (k.4, Act::Paste(w)),
                (k.5, Act::Duplicate),
                (k.6, Act::SelectAll),
                (k.7, Act::Save),
                (k.8, Act::Open),
                (k.9, Act::Delete),
                (k.10, Act::FrameAll),
                (k.11, Act::Comment),
                (k.12, Act::NewGraph),
                (k.14, Act::DuplicateNoLink),
                (k.15, Act::PasteNoLink(w)),
                (k.16, Act::ToggleDisable),
                (k.17, Act::DeleteRelink),
                (k.18, Act::ClearThumbs),
                (k.19, Act::SwapLinks),
            ] {
                if on {
                    self.acts.push(a);
                }
            }
        }

        self.editor_ui(root);
        eyedropper_overlay(ctx);
        self.new_dialog(ctx);
        self.confirm_close_ui(ctx);
        self.rename_graph_ui(ctx);
        self.save_template_ui(ctx);
        if let Some(t) = self.copy_out.take() {
            ctx.copy_text(t);
        }

        let todo = std::mem::take(&mut self.acts);
        for a in todo {
            self.do_act(a);
        }
        let now = std::time::Instant::now();
        const SETTLE: std::time::Duration = std::time::Duration::from_millis(110);
        if self.dirty_wait && self.job.is_none() {
            self.dirty_wait = false;
            self.dirty = true;
        }
        if self.dirty && self.job.is_some() && self.job_pass == Pass::Interactive && self.job_started.elapsed() < std::time::Duration::from_millis(250) {
            // a short interactive pass is under way: let it finish and show its picture, then run again with the latest
            // values. Cancelling it for every frame of a drag would leave the view frozen until the drag pauses.
            self.dirty = false;
            self.dirty_wait = true;
            self.thumbs_due = true;
            self.final_due = true;
            self.settle_at = Some(now + SETTLE);
            if let Ok(mut t) = EDIT_AT.lock() {
                *t = Some(now);
            }
        }
        if self.dirty {
            // a value changed this frame: show it at once with a cheap pass over just the on-screen nodes, and
            // (re)arm the settle timer so the passes over the whole graph run only once the input stops moving
            self.dirty = false;
            if let Ok(mut t) = EDIT_AT.lock() {
                *t = Some(now);
            }
            self.refine = false;
            self.thumbs_due = true;
            self.final_due = true;
            self.settle_at = Some(now + SETTLE);
            let first = self.interactive_size();
            self.maps_size = 0;
            self.spawn_eval(first, Pass::Interactive);
        } else if self.view_pending && self.job.is_none() {
            // only the selection moved: show that node's picture from the cache, leave everything else alone
            self.view_pending = false;
            self.view_only_next = true;
            let size = self.pass_size.max(self.first_tier());
            self.spawn_eval(size, Pass::Interactive);
        } else if self.job.is_none() {
            let settled = self.settle_at.map_or(true, |t| now >= t);
            if settled && self.thumbs_due {
                // input has settled: every node once at a small size, so every card shows its picture right away
                // (newly added nodes and freshly opened or imported graphs included) and the 3D view gets first maps
                self.thumbs_due = false;
                self.spawn_eval(self.first_tier(), Pass::Thumbs);
            } else if self.refine {
                // progressively sharpen the 2D preview while the user holds still
                self.refine = false;
                if let Some(sz) = self.refine_size {
                    self.view_only_next = self.job_view_only;
                    self.spawn_eval(sz, Pass::Interactive);
                }
            } else if settled && self.final_due {
                // the outputs at the full preview size: the final 3D maps. They get there in steps of at most
                // double the size, so the 3D view sharpens as soon as each step is done instead of after the last
                let target = self.target_3d().max(self.preview_full());
                let have = self.maps_size.max(self.first_tier());
                if have >= target {
                    self.final_due = false;
                    self.settle_at = None;
                } else {
                    let next = (have * 2).min(target);
                    if next >= target {
                        self.final_due = false;
                        self.settle_at = None;
                    }
                    self.spawn_eval(next, Pass::Final);
                }
            }
        }
        if self.job.is_some() || self.settle_at.is_some() || self.thumbs_due || self.final_due || self.refine || self.view_pending || self.dirty_wait {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        self.text_focus = ctx.egui_wants_keyboard_input();
        // save changed settings shortly after the last change
        let tick = self.prefs.tick(std::time::Instant::now());
        if let Some(deadline) = tick.next_deadline {
            ctx.request_repaint_after(deadline.saturating_duration_since(std::time::Instant::now()));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.prefs.flush();
    }
}

static LOGO: std::sync::OnceLock<(Tex, String)> = std::sync::OnceLock::new();

fn logo() -> &'static (Tex, String) {
    LOGO.get_or_init(load_logo)
}

/// Embedded logo, else assets/logo.png next to the exe / working dir, else a built-in purple squid.
fn load_logo() -> (Tex, String) {
    let mut bytes: Option<(Vec<u8>, String)> = logo_data::LOGO_PNG.map(|b| (b.to_vec(), "embedded".to_string()));
    if bytes.is_none() {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(d) = exe.parent() {
                dirs.push(d.join("assets"));
                dirs.push(d.to_path_buf());
            }
        }
        if let Ok(c) = std::env::current_dir() {
            dirs.push(c.join("assets"));
            dirs.push(c);
        }
        for d in dirs {
            let path = d.join("logo.png");
            if let Ok(b) = std::fs::read(&path) {
                bytes = Some((b, path.display().to_string()));
                break;
            }
        }
    }
    match bytes {
        Some((b, src)) => match png::decode(&b) {
            Ok(t) => (t, format!("Logo loaded from {src}")),
            Err(e) => (fallback_logo(128), format!("Logo file found but could not be read ({e}); using the built-in logo")),
        },
        None => (fallback_logo(128), "No logo.png found; using the built-in logo".into()),
    }
}

/// Built-in purple squid logo (used when no logo file is present).
fn fallback_logo(n: usize) -> Tex {
    let ell = |x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32| ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) <= 1.0;
    Tex::from_fn(n, n, |x, y| {
        // rounded square background
        let (qx, qy) = ((x - 0.5).abs() - 0.5 + 0.2, (y - 0.5).abs() - 0.5 + 0.2);
        let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - 0.2;
        if d > 0.0 {
            return [0.0, 0.0, 0.0, 0.0];
        }
        let t = 0.5 + 0.5 * (y - 0.5);
        let mut c = [0.32 - 0.1 * t, 0.15 - 0.05 * t, 0.62 - 0.12 * t, 1.0];
        let lilac = [0.92, 0.88, 1.0, 1.0];
        let fin = [0.78, 0.7, 0.96, 1.0];
        if ell(x, y, 0.37, 0.24, 0.07, 0.05) || ell(x, y, 0.63, 0.24, 0.07, 0.05) {
            c = fin;
        }
        let tri = y > 0.12 && y < 0.5 && (x - 0.5).abs() < (y - 0.12) * 0.42;
        if ell(x, y, 0.5, 0.4, 0.16, 0.24) || tri || ell(x, y, 0.5, 0.58, 0.16, 0.09) {
            c = lilac;
        }
        for i in 0..5 {
            let x0 = 0.36 + 0.07 * i as f32;
            let wx = x0 + 0.012 * (y * 38.0 + i as f32).sin();
            if y > 0.62 && y < 0.84 && (x - wx).abs() < 0.017 || ell(x, y, wx, 0.84, 0.021, 0.021) {
                c = lilac;
            }
        }
        for ex in [0.435, 0.565] {
            if ell(x, y, ex, 0.55, 0.036, 0.036) {
                c = [0.18, 0.08, 0.36, 1.0];
            }
            if ell(x, y, ex - 0.01, 0.54, 0.011, 0.011) {
                c = [1.0, 1.0, 1.0, 1.0];
            }
        }
        c
    })
}

fn window_icon() -> egui::IconData {
    let n = 128usize;
    let (t, _) = logo();
    let scale = (n as f32 / t.w as f32).min(n as f32 / t.h as f32);
    let (sw, sh) = (t.w as f32 * scale, t.h as f32 * scale);
    let mut rgba = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let (px, py) = (x as f32 + 0.5 - (n as f32 - sw) * 0.5, y as f32 + 0.5 - (n as f32 - sh) * 0.5);
            if px < 0.0 || py < 0.0 || px >= sw || py >= sh {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let p = t.sample(px / sw, py / sh);
                rgba.extend_from_slice(&[(p[0].clamp(0.0, 1.0) * 255.0) as u8, (p[1].clamp(0.0, 1.0) * 255.0) as u8, (p[2].clamp(0.0, 1.0) * 255.0) as u8, (p[3].clamp(0.0, 1.0) * 255.0) as u8]);
            }
        }
    }
    egui::IconData { rgba, width: n as u32, height: n as u32 }
}

/// Developer helper: `materialcraft-gui --dump-ux kind...` prints library nodes (`sbs:blend` ...) in the UX map format.
fn dump_ux(kinds: &[String]) {
    for kind in kinds {
        let Some(d) = materialcraft::sbs_defs::def(kind) else { continue };
        // the Library folder that lists it (as `lib_items` sorts them)
        let folder = match kind.as_str() {
            "sbs:input" | "sbs:output" | "sbs:graph" | "sbs:passthrough" => "Structure".to_string(),
            k if k.starts_with("sbs:fn:") => "Logic nodes".to_string(),
            k if k.starts_with("sbs:fx:") => "Pattern builders".to_string(),
            k if k.starts_with("sbs:lib:") => match sd_lib_group(k) {
                (m, "") => m.to_string(),
                (m, sub) => format!("{m} > {sub}"),
            },
            _ => "Core nodes".to_string(),
        };
        println!("- {} (`{kind}`)", d.label);
        println!("  - Library: {folder}");
        if d.inputs.is_empty() {
            println!("  - Inputs: none");
        } else {
            println!("  - Inputs: {}", d.inputs.iter().map(|i| i.id).collect::<Vec<_>>().join(", "));
            for i in &d.inputs {
                println!("    - {}\n      - Left-edge socket", i.id);
            }
        }
        let outs: Vec<&str> = if d.outputs.is_empty() && kind != "sbs:output" { vec!["output"] } else { d.outputs.clone() };
        println!("  - Outputs: {}", if outs.is_empty() { "none".to_string() } else { outs.join(", ") });
        if outs.len() > 1 {
            for o in &outs {
                println!("    - {o}\n      - Right-edge socket");
            }
        }
        println!("  - Properties:");
        for p in &d.params {
            println!("    - {}", materialcraft::sbs_defs::label_of(p.id));
            if !p.choices.is_empty() {
                let def = p.def.trim().parse::<usize>().ok().and_then(|i| p.choices.get(i).copied()).map(str::to_string).unwrap_or_else(|| p.def.clone());
                let unused = materialcraft::sbs_lib::UNUSED_CHOICE;
                let list: Vec<String> = if p.choices.contains(&unused) {
                    p.choices.iter().enumerate().filter(|(_, c)| **c != unused).map(|(i, c)| format!("{i} {c}")).collect()
                } else {
                    p.choices.iter().map(|c| c.to_string()).collect()
                };
                println!("      - Control: dropdown\n      - Default: {def}\n      - Choices: {}", list.join(" / "));
            } else if let Some((lo, hi)) = p.range {
                println!("      - Control: slider\n      - Default: {}\n      - Range: {lo}..{hi}", p.def);
            } else {
                println!("      - Default: {}", p.def);
            }
        }
    }
}

/// The editor, with the file named on the command line (if any) already open.
fn new_app(cc: &eframe::CreationContext<'_>) -> App {
    let mut app = App::new(cc);
    if let Some(p) = std::env::args().nth(1).filter(|a| !a.starts_with("--")) {
        app.open_path(PathBuf::from(p));
    }
    app
}

/// Port requested with `--control`, taken by the first frame.
static AUTO_CONTROL: std::sync::Mutex<Option<u16>> = std::sync::Mutex::new(None);

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|a| a == "--dump-ux").unwrap_or(false) {
        dump_ux(&args[2..]);
        return Ok(());
    }
    // --control [PORT]: let assistants connect from the start (port 0 or none = any free port)
    if let Some(i) = args.iter().position(|a| a == "--control") {
        let port = args.get(i + 1).and_then(|p| p.parse::<u16>().ok()).unwrap_or(0);
        *AUTO_CONTROL.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(port);
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1500.0, 920.0]).with_title("MaterialCraft").with_icon(window_icon()),
        ..Default::default()
    };
    eframe::run_native("MaterialCraft", options, Box::new(|cc| Ok(Box::new(new_app(cc)))))
}
