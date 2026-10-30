// SPDX-License-Identifier: MIT OR Apache-2.0
// ======================================================================= .sbs packages in the editor (included into gui.rs)
//
// `.sbs` packages open as they are (see `sbs_native`): their nodes are the file's own node types and values, and
// saving writes the file back with only the changes rewritten. This part holds what the editor does differently for
// them: sockets and names of the nodes, the parameter panel, the node list, opening, saving and new packages.

use materialcraft::sbs_defs::{self as sdefs, Fam, PTy};
use materialcraft::sbs_native::{self as native, PVal};

/// Interned names for the editor's `&'static str` sockets (a package's own input names are only known at run time).
fn intern(s: &str) -> &'static str {
    static SET: std::sync::OnceLock<Mutex<BTreeSet<&'static str>>> = std::sync::OnceLock::new();
    let set = SET.get_or_init(|| Mutex::new(BTreeSet::new()));
    let mut g = set.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(x) = g.get(s) {
        return x;
    }
    let x: &'static str = Box::leak(s.to_string().into_boxed_str());
    g.insert(x);
    x
}

fn sd_slots(n: &Node, lib: &BTreeMap<String, Graph>) -> Vec<(&'static str, bool)> {
    // an input the node is set not to use has no socket (unless a link still reaches it)
    native::slots(n, lib).iter().filter(|s| n.ins.contains_key(*s) || native::input_used(n, s)).map(|s| (intern(s), false)).collect()
}

fn sd_outs(n: &Node, lib: &BTreeMap<String, Graph>) -> Vec<String> {
    let o = native::outputs(n, lib);
    if o.len() >= 2 {
        o
    } else {
        Vec::new()
    }
}

fn sd_color(kind: &str) -> Color32 {
    match kind {
        "sbs:input" => Color32::from_rgb(110, 116, 128),
        "sbs:output" => Color32::from_rgb(74, 98, 120),
        "sbs:graph" => Color32::from_rgb(214, 140, 56),
        "sbs:passthrough" => Color32::from_rgb(110, 116, 128),
        k if k.starts_with("sbs:fn:") => Color32::from_rgb(92, 120, 92),
        k if k.starts_with("sbs:fx:") => Color32::from_rgb(140, 88, 184),
        k => match sdefs::def(k).map(|d| d.category) {
            Some("Atomic generators") => Color32::from_rgb(176, 76, 66),
            Some("Generators") => Color32::from_rgb(176, 76, 66),
            Some("SDF") => Color32::from_rgb(140, 88, 184),
            Some("Material") => Color32::from_rgb(190, 150, 50),
            Some("Utility") => Color32::from_rgb(100, 110, 130),
            Some(_) => Color32::from_rgb(64, 112, 176),
            None => Color32::from_rgb(120, 70, 70),
        },
    }
}

/// A float slider range for a parameter without one.
/// The colour a gradient's keys give at `t` (linear between keys, flat past the ends).
fn ramp_at(keys: &[(f32, [f32; 4])], t: f32) -> [f32; 4] {
    let mut keys = keys.to_vec();
    keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let Some(first) = keys.first() else { return [t, t, t, 1.0] };
    if t <= first.0 {
        return first.1;
    }
    for w in keys.windows(2) {
        if t <= w[1].0 {
            let f = if w[1].0 > w[0].0 { (t - w[0].0) / (w[1].0 - w[0].0) } else { 1.0 };
            return [0, 1, 2, 3].map(|i| w[0].1[i] + (w[1].1[i] - w[0].1[i]) * f);
        }
    }
    keys[keys.len() - 1].1
}

fn sd_range(p: &native::ParamInfo) -> (f32, f32) {
    match p.range {
        Some(r) => r,
        None => match p.ty {
            PTy::I(_) => (0.0, 16.0),
            _ => {
                let d = p.def.split_whitespace().next().and_then(|x| x.parse::<f32>().ok()).unwrap_or(1.0).abs();
                (0.0, if d > 1.0 { (d * 2.0).ceil() } else { 1.0 })
            }
        },
    }
}

impl App {
    /// Folders searched for linked files given by relative path: the package's folder and its `resources` folder.
    fn sync_search_dirs(&mut self) {
        let dirs: Vec<PathBuf> = match self.file.as_ref().and_then(|f| f.parent()) {
            Some(p) if !p.as_os_str().is_empty() => vec![p.to_path_buf(), p.join("resources")],
            _ => Vec::new(),
        };
        if dirs != self.res_dirs {
            materialcraft::resources::set_search_dirs(dirs.clone());
            self.res_dirs = dirs;
            self.dirty = true;
        }
    }

    /// The node types the editor offers in an `.sbs` package: the format's own nodes and the library graphs
    /// MaterialCraft renders, the package's other graphs, inputs and outputs.
    fn sd_entries(&self) -> Vec<Entry> {
        let fam = match self.g.graph_kind() {
            "function" => Fam::Function,
            "fxmap" => Fam::Pattern,
            _ => Fam::Texture,
        };
        let mut v: Vec<Entry> = sdefs::palette(fam)
            .into_iter()
            .map(|d| Entry {
                label: d.label.to_string(),
                kind: d.kind.to_string(),
                sub: None,
                cat: d.category,
                doc: match d.library {
                    Some((p, g)) => format!("library graph {g} ({p})"),
                    None => format!("{} node", d.sd),
                },
                nums: vec![],
            })
            .collect();
        v.push(Entry { label: "Comment".into(), kind: "comment".into(), sub: None, cat: "Utility", doc: "small note placed at the cursor".into(), nums: vec![] });
        v.push(Entry { label: "Frame".into(), kind: "frame".into(), sub: None, cat: "Utility", doc: "frame around the selected nodes".into(), nums: vec![] });
        if fam != Fam::Texture {
            return v;
        }
        v.push(Entry { label: "Output".into(), kind: "sbs:output".into(), sub: None, cat: "Utility", doc: "graph output: drives the 3D view and exports a texture".into(), nums: vec![] });
        v.push(Entry { label: "Input Color".into(), kind: "sbs:input".into(), sub: None, cat: "Utility", doc: "colour image input of the graph".into(), nums: vec![("gray", 0.0)] });
        v.push(Entry { label: "Input Grayscale".into(), kind: "sbs:input".into(), sub: None, cat: "Utility", doc: "grey image input of the graph".into(), nums: vec![("gray", 1.0)] });
        // the package's other graphs, as nodes
        let me = self.g.meta.get("identifier").cloned().unwrap_or_default();
        let mut names: BTreeSet<String> = BTreeSet::new();
        // the view holds every graph of the package; graphs that use this one cannot run inside it
        let all = native::all_graphs(&self.g);
        for (k, g) in &all {
            if *k != me && g.sd.as_ref().map(|s| s.role == native::Role::Graph).unwrap_or(false) && !native::uses_graph(&all, k, &me) {
                names.insert(k.clone());
            }
        }
        for name in names {
            v.push(Entry { label: format!("graph: {name}"), kind: "sbs:graph".into(), sub: Some(name.clone()), cat: "Graphs", doc: format!("instance of graph '{name}'"), nums: vec![] });
        }
        v
    }

    /// Add a node of a native kind at `pos`.
    fn sd_add_node(&mut self, kind: &str, sub: Option<&str>, pos: Pos2, gray: bool) -> u32 {
        self.snap();
        let p = [pos.x, pos.y];
        let id = match kind {
            "sbs:output" => native::add_output(&mut self.g, if self.parents.is_empty() { "baseColor" } else { "output" }, None, p),
            "sbs:input" => native::add_input(&mut self.g, gray, None, p),
            _ => {
                let id = native::free_id(&self.g);
                self.g.nodes.insert(id, native::new_node(kind, sub, p));
                // an FX-Map comes with its pattern graph
                native::ensure_fx_graph(&mut self.g, id);
                id
            }
        };
        self.sel = [id].into_iter().collect();
        self.active = Some(id);
        self.sel_comment = None;
        self.dirty = true;
        id
    }

    /// Open an `.sbs` package as it is.
    fn open_sbs(&mut self, p: PathBuf) {
        if let Some(i) = (0..self.pkgs.len()).find(|&i| match &self.pkgs[i] {
            Some(k) => k.path.as_ref() == Some(&p),
            None => self.file.as_ref() == Some(&p),
        }) {
            self.switch_pkg(i);
            self.screen = Screen::Editor;
            return;
        }
        match native::open(&p) {
            Ok((g, warnings)) => {
                let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                // what MaterialCraft renders differently (its own nodes, approximations)
                let report = native::lower(&g).map(|l| l.report).ok();
                let pkg = Pkg { path: Some(p.clone()), name, g, modified: false, tabs: Vec::new() };
                // nothing is open yet (the empty start graph): the package takes its place
                if self.pkgs.is_empty() && self.file.is_none() && !self.modified && self.g.nodes.is_empty() {
                    self.ws_fresh();
                    self.load_active(pkg);
                } else {
                    self.add_pkg_active(pkg);
                }
                self.screen = Screen::Editor;
                self.need_fit = true;
                self.push_recent(&p);
                let mut msg = format!("Opened {}", p.display());
                if !warnings.is_empty() {
                    msg.push_str(&format!("  -  {}", warnings.join("; ")));
                }
                match report {
                    Some(r) if r.missing + r.approx > 0 => {
                        msg.push_str(&format!("  -  rendering: {}", r.summary()));
                        self.status(msg, r.missing == 0);
                        self.import_view = Some(ImportView { min: Level::Approx, report: r });
                    }
                    _ => self.status(msg, warnings.is_empty()),
                }
            }
            Err(e) => self.status(format!("Open failed: {}: {e}", p.display()), false),
        }
    }

    /// Write an `.sbs` package (the graph on screen and everything else of the package).
    fn save_sbs(&mut self, p: PathBuf) {
        self.goto_root();
        let p = if is_sbs(&p) { p } else { p.with_extension("sbs") };
        let text = match native::save_text(&self.g) {
            Ok(t) => t,
            Err(e) => return self.status(format!("Save failed: {e}"), false),
        };
        match materialcraft::safeio::write_file(&p, text.as_bytes()) {
            Ok(()) => {
                self.status(format!("Saved {}", p.display()), true);
                self.push_recent(&p);
                self.file = Some(p);
                self.modified = false;
            }
            Err(e) => self.status(format!("Save failed: {e}"), false),
        }
    }

    /// The parameter panel of a native node.
    /// Rename the graph output of output node `id`; the nodes of the other graphs that read it by name follow.
    fn rename_output(&mut self, id: u32, new: &str) {
        self.snap();
        match native::rename_output(&mut self.g, id, new) {
            Ok((old, new)) if old != new => {
                // the other graphs of the package: the view's copies, and the graphs this one was opened from
                let me = native::graph_name(&self.g);
                for g in self.g.subgraphs.values_mut() {
                    native::retarget_output(g, &me, &old, &new);
                }
                for (p, _) in self.parents.iter_mut() {
                    native::retarget_output(p, &me, &old, &new);
                }
                self.status(format!("Output '{old}' renamed to '{new}'"), true);
                self.dirty = true;
            }
            Ok(_) => {
                self.undo.pop();
            }
            Err(e) => {
                self.undo.pop();
                self.status(e, false);
            }
        }
    }

    fn sd_node_props(&mut self, ui: &mut egui::Ui, id: u32) {
        let node = self.g.nodes[&id].clone();
        let title = native::label(&node, &self.g);
        // controls share the row with the label (112 px) and up to three small buttons
        let ctl = (ui.available_width() - 112.0 - 96.0).clamp(70.0, 170.0);
        ui.spacing_mut().slider_width = (ctl - 56.0).max(40.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(6.0, 20.0), Sense::hover());
            ui.painter().rect_filled(r, 1.0, sd_color(&node.kind));
            ui.label(egui::RichText::new(format!("{title}  #{id}")).size(15.0).strong());
        });
        let def = native::node_def(&node);
        match def {
            Some(d) => {
                match d.library {
                    Some((p, g)) => ui.small(format!("Library graph {g} ({p})")),
                    None => ui.small(format!("{} node", d.sd)),
                };
                if !d.renders {
                    ui.colored_label(Color32::from_rgb(235, 140, 80), "MaterialCraft does not render this library graph yet: it shows a placeholder. The file keeps it as it is.");
                }
            }
            None if node.kind == "sbs:graph" => {
                ui.small(format!("Graph {}", node.strs.get("#graph").map(String::as_str).unwrap_or("?")));
            }
            None => {
                ui.colored_label(Color32::from_rgb(235, 140, 80), format!("Unknown node type '{}': the file keeps it as it is", node.kind));
            }
        }
        if let Some(e) = self.node_err.get(&id) {
            ui.colored_label(Color32::from_rgb(255, 120, 110), e);
        }
        // function and FX-map graphs: one node gives the graph's result
        if node.kind.starts_with("sbs:fn:") || node.kind.starts_with("sbs:fx:") {
            let is_root = self.g.sd.as_ref().and_then(|s| s.root) == Some(id);
            if is_root {
                ui.colored_label(Color32::from_rgb(130, 200, 160), "This node gives the graph's result");
            } else if ui.button("Make this the result").on_hover_text("The graph's value is what this node computes").clicked() {
                self.snap();
                if let Some(sd) = self.g.sd.as_mut() {
                    sd.root = Some(id);
                }
                self.dirty = true;
            }
        }
        let mut sets: Vec<(String, Option<PVal>)> = Vec::new();
        let mut strs: Vec<(String, Option<String>)> = Vec::new();
        let mut enter: Option<String> = None;
        let mut make_fn: Option<(String, PVal)> = None;
        let mut usage: Option<String> = None;
        let mut expose: Option<(String, String)> = None;
        let mut unexpose: Option<String> = None;
        let mut apply_preset: Option<usize> = None;
        let mut make_fx = false;
        // ---- name shown on the card
        ui.horizontal(|ui| {
            row_label(ui, "Name");
            let mut t = node.strs.get("title").cloned().unwrap_or_default();
            if ui.add(egui::TextEdit::singleline(&mut t).desired_width(f32::INFINITY).hint_text(&title)).changed() {
                strs.push(("title".into(), if t.is_empty() { None } else { Some(t) }));
            }
        });
        if node.kind == "sbs:graph" {
            if let Some(g) = node.strs.get("#graph").cloned() {
                if self.g.subgraphs.contains_key(&g) && ui.button("Open graph").clicked() {
                    enter = Some(g);
                }
            }
            // the presets of the graph it runs: one sets the values it holds, the others stay
            let presets = native::node_presets(&node, &self.g.subgraphs);
            if !presets.is_empty() {
                ui.horizontal(|ui| {
                    row_label(ui, "Preset");
                    egui::ComboBox::from_id_salt(("sd_preset", id)).width(150.0).selected_text("Apply a preset...").show_ui(ui, |ui| {
                        for (i, p) in presets.iter().enumerate() {
                            if ui.selectable_label(false, &p.label).on_hover_text(format!("Sets {} parameter(s); the others keep their values", p.values.len())).clicked() {
                                apply_preset = Some(i);
                            }
                        }
                    });
                });
            }
        }
        if node.kind == "sbs:fxmaps" {
            let names: Vec<String> = node.strs.get("#fx").map(|x| x.split(',').filter(|s| !s.is_empty() && self.g.subgraphs.contains_key(*s)).map(str::to_string).collect()).unwrap_or_default();
            if names.is_empty() {
                if ui.button("Make pattern graph").on_hover_text("Give this FX-Map its pattern graph (one Quadrant node)").clicked() {
                    make_fx = true;
                }
            }
            for (i, f) in names.iter().enumerate() {
                let label = if names.len() == 1 { "Open pattern graph".to_string() } else { format!("Open pattern graph {}", i + 1) };
                if ui.button(label).on_hover_text("Edit the Quadrant, Iterate and Switch nodes that draw this FX-Map").clicked() {
                    enter = Some(f.clone());
                }
            }
        }
        let mut rename_out: Option<String> = None;
        if node.kind == "sbs:output" {
            let uid = node.strs.get("#output").cloned().unwrap_or_default();
            // the output's identifier: its name on the card, in exported file names and for whoever uses the graph
            let ident = self.g.sd.as_ref().and_then(|s| s.output_identifier(&uid)).unwrap_or_default();
            ui.horizontal(|ui| {
                row_label(ui, "Identifier");
                // typed into a buffer and applied when the box loses focus, so half a name never renames anything
                let buf = Id::new(("sd_out_ident", id));
                let mut t = ui.ctx().data(|d| d.get_temp::<String>(buf)).unwrap_or_else(|| ident.clone());
                let r = ui.add(egui::TextEdit::singleline(&mut t).desired_width(f32::INFINITY)).on_hover_text("Rename this output (letters, digits and _); Enter applies");
                if r.lost_focus() {
                    // Esc gives the edit up
                    if t.trim() != ident && !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        rename_out = Some(t.trim().to_string());
                    }
                    ui.ctx().data_mut(|d| d.remove::<String>(buf));
                } else if r.has_focus() {
                    ui.ctx().data_mut(|d| d.insert_temp(buf, t));
                }
            });
            let cur = self.g.sd.as_ref().and_then(|s| s.output_usage(&uid)).unwrap_or_default();
            ui.horizontal(|ui| {
                row_label(ui, "Usage");
                egui::ComboBox::from_id_salt(("sd_usage", id)).selected_text(&cur).show_ui(ui, |ui| {
                    for (u, _) in native::USAGES {
                        if ui.selectable_label(cur == *u, *u).clicked() {
                            usage = Some(u.to_string());
                        }
                    }
                });
            });
        }
        // ---- which image inputs the node uses (each has its own switch): the others have no socket
        let switches = native::input_switches(&node);
        if !switches.is_empty() {
            egui::CollapsingHeader::new(egui::RichText::new("IMAGE INPUTS").strong()).id_salt(("sd_used_ins", id)).default_open(true).show(ui, |ui| {
                ui.small("Tick the maps this node should read; the others have no socket.");
                for (slot, sw) in switches {
                    let mut on = native::input_used(&node, slot);
                    let r = ui.checkbox(&mut on, sdefs::label_of(slot.trim_end_matches("_input")));
                    let r = if node.ins.contains_key(*slot) && !on { r.on_hover_text("Still linked, but not read until it is ticked again") } else { r };
                    if r.changed() {
                        sets.push((sw.to_string(), Some(PVal::Const { ty: PTy::B, raw: (on as i32).to_string(), rel: Some("0".into()) })));
                    }
                }
            });
        }
        // ---- inputs overview
        let sl = native::slots(&node, &self.g.subgraphs);
        if !sl.is_empty() {
            egui::CollapsingHeader::new(egui::RichText::new("INPUTS").strong()).id_salt(("sd_ins", id)).default_open(false).show(ui, |ui| {
                for name in &sl {
                    match node.ins.get(name) {
                        Some(src) => ui.label(format!("{name}  <-  #{src}")),
                        None => ui.colored_label(Color32::from_gray(140), format!("{name}  (not connected)")),
                    };
                }
            });
        }
        // ---- the spline of an SVG node
        let mut edit_spline = false;
        if node.kind == "sbs:svg" {
            egui::CollapsingHeader::new(egui::RichText::new("SPLINE").strong()).id_salt(("sd_spl", id)).default_open(true).show(ui, |ui| {
                if ui.button("Edit as spline...").on_hover_text("Draw and edit the shape with bezier curves; the node's picture follows").clicked() {
                    edit_spline = true;
                }
            });
        }
        // ---- the colour keys of a Gradient Map
        let mut new_keys: Option<Vec<(f32, [f32; 4])>> = None;
        if node.kind == "sbs:gradient" {
            let keys = native::gradient_keys(&node);
            egui::CollapsingHeader::new(egui::RichText::new("GRADIENT").strong()).id_salt(("sd_grad", id)).default_open(true).show(ui, |ui| {
                // the ramp as it maps: black (left) to white (right)
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width().min(260.0), 16.0), Sense::hover());
                let steps = 64;
                for i in 0..steps {
                    let t = (i as f32 + 0.5) / steps as f32;
                    let c = ramp_at(&keys, t);
                    let x0 = r.left() + r.width() * i as f32 / steps as f32;
                    let x1 = r.left() + r.width() * (i + 1) as f32 / steps as f32;
                    let to8 = |v: f32| (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8;
                    ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(x0, r.top()), egui::pos2(x1 + 0.5, r.bottom())), 0.0, Color32::from_rgb(to8(c[0]), to8(c[1]), to8(c[2])));
                }
                let mut edited = keys.clone();
                let mut changed = false;
                let mut remove: Option<usize> = None;
                for (k, (pos, col)) in edited.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::Slider::new(pos, 0.0..=1.0)).on_hover_text("Position of the key along the ramp").changed();
                        changed |= color_field(ui, Id::new(("ramp_key", k)), col, true).changed();
                        if keys.len() > 1 && ui.small_button("x").on_hover_text("Remove this key").clicked() {
                            remove = Some(k);
                        }
                    });
                }
                if let Some(k) = remove {
                    edited.remove(k);
                    changed = true;
                }
                if ui.button("+ Add key").on_hover_text("A new key halfway along the widest gap").clicked() {
                    let mut at = 0.5;
                    let mut gap = -1.0;
                    let mut sorted = keys.clone();
                    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    for w in sorted.windows(2) {
                        if w[1].0 - w[0].0 > gap {
                            gap = w[1].0 - w[0].0;
                            at = (w[0].0 + w[1].0) * 0.5;
                        }
                    }
                    edited.push((at, ramp_at(&keys, at)));
                    changed = true;
                }
                if changed {
                    new_keys = Some(edited);
                }
            });
        }
        // ---- Levels: Designer's histogram with its handles
        if node.kind == "sbs:levels" {
            let vals = native::values(&node);
            let read = |k: &str, d: f32| -> [f32; 4] {
                match vals.get(k) {
                    Some(v @ PVal::Const { .. }) => {
                        let n = v.nums();
                        [n.first().copied().unwrap_or(d), n.get(1).copied().unwrap_or(d), n.get(2).copied().unwrap_or(d), n.get(3).copied().unwrap_or(d)]
                    }
                    _ => [d; 4],
                }
            };
            let mut lv = LevelsVals { in_lo: read("levelinlow", 0.0), in_mid: read("levelinmid", 0.5), in_hi: read("levelinhigh", 1.0), out_lo: read("leveloutlow", 0.0), out_hi: read("levelouthigh", 1.0) };
            let src_px = node.ins.get("input1").and_then(|s| self.thumb_px.get(s)).cloned();
            let chan_id = Id::new(("sd_levels_chan", id));
            let mut chan = ui.ctx().data(|d| d.get_temp::<usize>(chan_id)).unwrap_or(0);
            let mut changed = false;
            egui::CollapsingHeader::new(egui::RichText::new("SPECIFIC PARAMETERS").strong()).id_salt(("sd_lvl", id)).default_open(true).show(ui, |ui| {
                ui.label(egui::RichText::new("Levels Histogram").color(Color32::from_gray(205)));
                egui::ComboBox::from_id_salt(("sd_levels_chan_cb", id)).width(ui.available_width() - 12.0).selected_text(LEVELS_CHANNELS[chan]).show_ui(ui, |ui| {
                    for (i, c) in LEVELS_CHANNELS.iter().enumerate() {
                        if ui.selectable_label(chan == i, *c).clicked() {
                            chan = i;
                        }
                    }
                });
                ui.ctx().data_mut(|d| d.insert_temp(chan_id, chan));
                ui.add_space(4.0);
                changed |= levels_widget(ui, ("sd_levels_w", id), &mut lv, chan, src_px.as_deref());
                ui.add_space(4.0);
                // the numbers, for exact values
                let lane = if chan == 0 { 0 } else { chan - 1 };
                let mut num = |ui: &mut egui::Ui, label: &str, v: &mut [f32; 4]| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(label).color(Color32::from_gray(170)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let mut x = v[lane];
                            if ui.add(egui::DragValue::new(&mut x).range(0.0..=1.0).speed(0.002).fixed_decimals(3)).changed() {
                                set_lane(v, chan, x);
                                changed = true;
                            }
                        });
                    });
                };
                num(ui, "Level In Low", &mut lv.in_lo);
                num(ui, "Level In Mid", &mut lv.in_mid);
                num(ui, "Level In High", &mut lv.in_hi);
                num(ui, "Level Out Low", &mut lv.out_lo);
                num(ui, "Level Out High", &mut lv.out_hi);
                let mut clamp = read("clampinterm", 1.0)[0] != 0.0;
                if ui.checkbox(&mut clamp, "Clamp in term").on_hover_text("Clamp the values between low and high before the output range").changed() {
                    sets.push(("clampinterm".to_string(), Some(PVal::Const { ty: PTy::B, raw: (clamp as i32).to_string(), rel: Some("0".into()) })));
                }
            });
            if changed {
                for (k, v) in [("levelinlow", lv.in_lo), ("levelinmid", lv.in_mid), ("levelinhigh", lv.in_hi), ("leveloutlow", lv.out_lo), ("levelouthigh", lv.out_hi)] {
                    sets.push((k.to_string(), Some(PVal::Const { ty: PTy::F(4), raw: format!("{} {} {} {}", v[0], v[1], v[2], v[3]), rel: Some("0".into()) })));
                }
            }
        }
        // ---- parameters, by group
        let params = native::params_of(&node, &self.g.subgraphs);
        let gray_node = matches!(native::values(&node).get("colorswitch"), Some(PVal::Const { raw, .. }) if raw.trim() == "0");
        let mut groups: Vec<String> = Vec::new();
        for p in &params {
            if !groups.contains(&p.group) {
                groups.push(p.group.clone());
            }
        }
        groups.sort_by_key(|g| match g.as_str() {
            "" => 0,
            "Base" => 2,
            "Other" => 3,
            _ => 1,
        });
        // the base settings start folded when the node has parameters of its own
        let only_base = groups.iter().all(|g| g == "Base");
        let is_levels = node.kind == "sbs:levels";
        let mut draw_group = |ui: &mut egui::Ui, gname: &str| {
                // the input switches are ticked under IMAGE INPUTS instead
                for p in params.iter().filter(|p| p.group == gname && !(is_levels && (p.id.starts_with("levelin") || p.id.starts_with("levelout") || p.id == "clampinterm")) && !switches.iter().any(|(_, sw)| *sw == p.id)) {
                    // Designer's layout: the name on its own line with small tools at the right, the control full width below
                    ui.add_space(2.0);
                    ui.vertical(|ui| {
                        let cur = p.current();
                        let ctl = (ui.available_width() - 12.0).max(90.0);
                        ui.spacing_mut().slider_width = (ctl - 70.0).max(60.0);
                        ui.spacing_mut().interact_size.y = 20.0;
                        let is_const = matches!(cur, PVal::Const { .. });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&p.label).color(Color32::from_gray(205)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if !is_const {
                                    return;
                                }
                                let tool = |ui: &mut egui::Ui, t: &str, tip: &str| ui.add(egui::Button::new(egui::RichText::new(t).size(10.5).color(Color32::from_gray(150))).small().frame(false)).on_hover_text(tip).clicked();
                                if self.g.sd.as_ref().map(|s| s.role == native::Role::Graph).unwrap_or(false) && p.group != "Base" && tool(ui, "P", "Expose: make this a parameter of the graph, set by whoever uses it") {
                                    expose = Some((p.id.clone(), p.label.clone()));
                                }
                                if !matches!(p.ty, PTy::S | PTy::Path) && tool(ui, "fn", "Compute this value with a function") {
                                    make_fn = Some((p.id.clone(), cur.clone()));
                                }
                                if p.value.is_some() && tool(ui, "↺", "Back to the default") {
                                    sets.push((p.id.clone(), None));
                                }
                            });
                        });
                        ui.horizontal(|ui| {
                        match &cur {
                            PVal::Dyn { graph, .. } if native::reads_input(&self.g, graph).is_some() => {
                                let ident = native::reads_input(&self.g, graph).unwrap_or_default();
                                ui.label(egui::RichText::new(format!("● {ident}")).color(Color32::from_rgb(120, 170, 235)))
                                    .on_hover_text("Set by whoever uses this graph; its default is in the graph's settings (click the canvas)");
                                if ui.small_button("x").on_hover_text("Stop exposing it: back to the parameter's default").clicked() {
                                    unexpose = Some(ident);
                                }
                            }
                            PVal::Dyn { graph, .. } => {
                                ui.label(egui::RichText::new("function").italics().color(Color32::from_rgb(130, 200, 160)));
                                if ui.small_button("Edit").on_hover_text("Open the function that computes this value").clicked() {
                                    enter = Some(graph.clone());
                                }
                                if ui.small_button("x").on_hover_text("Stop computing it: back to the default value").clicked() {
                                    sets.push((p.id.clone(), None));
                                }
                            }
                            PVal::Const { raw, .. } => {
                                let nums = cur.nums();
                                let set = |v: Vec<f32>, sets: &mut Vec<(String, Option<PVal>)>| sets.push((p.id.clone(), Some(native::value_from_nums(p, &v))));
                                match p.ty {
                                    PTy::S | PTy::Path => {
                                        let mut t = raw.clone();
                                        if ui.add(egui::TextEdit::singleline(&mut t).desired_width(ctl)).changed() {
                                            sets.push((p.id.clone(), Some(PVal::Const { ty: PTy::S, raw: t, rel: Some("0".into()) })));
                                        }
                                    }
                                    PTy::B if p.choices.len() != 2 => {
                                        let mut b = nums.first().copied().unwrap_or(0.0) != 0.0;
                                        if ui.checkbox(&mut b, "").changed() {
                                            set(vec![b as i32 as f32], &mut sets);
                                        }
                                    }
                                    PTy::B | PTy::I(1) if !p.choices.is_empty() => {
                                        let v = nums.first().copied().unwrap_or(0.0).round() as i64;
                                        let shown = p.choices.get(v.max(0) as usize).cloned().unwrap_or_else(|| v.to_string());
                                        egui::ComboBox::from_id_salt(("sd_choice", id, p.id.clone())).width(ctl).selected_text(shown).show_ui(ui, |ui| {
                                            for (i, c) in p.choices.iter().enumerate().filter(|(_, c)| c.as_str() != materialcraft::sbs_lib::UNUSED_CHOICE) {
                                                if ui.selectable_label(v == i as i64, c).clicked() {
                                                    set(vec![i as f32], &mut sets);
                                                }
                                            }
                                        });
                                    }
                                    PTy::F(1) | PTy::I(1) => {
                                        let (lo, hi) = sd_range(p);
                                        let mut x = nums.first().copied().unwrap_or(0.0);
                                        let r = if matches!(p.ty, PTy::I(_)) {
                                            let mut i = x.round() as i64;
                                            let r = ui.add(egui::Slider::new(&mut i, lo as i64..=hi as i64).clamping(egui::SliderClamping::Never));
                                            x = i as f32;
                                            r
                                        } else {
                                            ui.add(egui::Slider::new(&mut x, lo..=hi).clamping(egui::SliderClamping::Never))
                                        };
                                        if r.changed() {
                                            set(vec![x], &mut sets);
                                        }
                                    }
                                    PTy::FV | PTy::F(4) if (p.ty == PTy::FV || p.id.contains("color")) && !gray_node && nums.len() >= 3 => {
                                        let mut c = [nums[0], nums[1], nums[2], nums.get(3).copied().unwrap_or(1.0)];
                                        if color_field(ui, ui.next_auto_id(), &mut c, true).changed() {
                                            set(c.to_vec(), &mut sets);
                                        }
                                    }
                                    PTy::FV if gray_node => {
                                        // a grey level: the same picker as a colour, in one channel
                                        let mut g = nums.first().copied().unwrap_or(0.0);
                                        if gray_field(ui, ui.next_auto_id(), &mut g).changed() {
                                            set(vec![g, g, g, 1.0], &mut sets);
                                        }
                                    }
                                    _ => {
                                        let lanes = if p.ty == PTy::FV && gray_node { 1 } else { p.ty.lanes().max(nums.len()).max(1) };
                                        ui.spacing_mut().interact_size.x = ((ctl - 4.0 * lanes as f32) / lanes as f32).max(28.0);
                                        let mut v: Vec<f32> = (0..lanes).map(|i| nums.get(i).copied().unwrap_or(0.0)).collect();
                                        let mut changed = false;
                                        for x in v.iter_mut() {
                                            changed |= ui.add(egui::DragValue::new(x).speed(if matches!(p.ty, PTy::I(_)) { 0.1 } else { 0.005 })).changed();
                                        }
                                        if changed {
                                            if p.ty == PTy::FV && gray_node {
                                                let g = v[0];
                                                v = vec![g, g, g, 1.0];
                                            }
                                            set(v, &mut sets);
                                        }
                                    }
                                }
                            }
                        }
                        });
                    });
                }
        };
        // Instance Parameters holds the ungrouped controls and, nested, the node's own sub-headings (Advanced, Pattern,
        // Size ...); Base Parameters is its own folded section
        let subs: Vec<String> = groups.iter().filter(|g| !matches!(g.as_str(), "" | "Base")).cloned().collect();
        if groups.iter().any(|g| g != "Base") {
            egui::CollapsingHeader::new(egui::RichText::new("INSTANCE PARAMETERS").strong()).id_salt(("sd_grp", id, "inst")).default_open(true).show(ui, |ui| {
                draw_group(ui, "");
                for sub in &subs {
                    egui::CollapsingHeader::new(egui::RichText::new(sub.as_str()).strong()).id_salt(("sd_grp", id, sub.clone())).default_open(true).show(ui, |ui| draw_group(ui, sub));
                }
            });
        }
        if groups.iter().any(|g| g == "Base") {
            egui::CollapsingHeader::new(egui::RichText::new("BASE PARAMETERS").strong()).id_salt(("sd_grp", id, "Base")).default_open(only_base).show(ui, |ui| draw_group(ui, "Base"));
        }
        if let Some(u) = usage {
            self.snap();
            if let Err(e) = native::set_output_usage(&mut self.g, id, &u) {
                self.status(e, false);
            }
            self.dirty = true;
        }
        if let Some(new) = rename_out {
            self.rename_output(id, &new);
        }
        if make_fx {
            self.snap();
            if let Some(f) = native::ensure_fx_graph(&mut self.g, id) {
                enter = Some(f);
            }
            self.dirty = true;
        }
        if edit_spline {
            self.open_spline_editor(id);
        }
        if let Some(keys) = new_keys {
            if !self.edit_open {
                self.snap();
                self.edit_open = true;
            }
            if let Some(n) = self.g.nodes.get_mut(&id) {
                native::set_gradient_keys(n, &keys);
            }
            self.dirty = true;
        }
        if let Some((param, label)) = expose {
            self.snap();
            match native::expose(&mut self.g, id, &param, &param, &label, None) {
                Ok(ident) => self.status(format!("'{label}' is now graph parameter '{ident}'"), true),
                Err(e) => {
                    self.undo.pop();
                    self.status(e, false);
                }
            }
            self.dirty = true;
        }
        if let Some(ident) = unexpose {
            self.snap();
            if let Err(e) = native::unexpose(&mut self.g, &ident) {
                self.undo.pop();
                self.status(e, false);
            }
            self.dirty = true;
        }
        if let Some(i) = apply_preset {
            self.snap();
            let lib = &self.g.subgraphs;
            let res = match self.g.nodes.get(&id) {
                Some(n) => {
                    let mut n = n.clone();
                    native::apply_preset(&mut n, lib, i).map(|k| (n, k))
                }
                None => Err("no such node".into()),
            };
            match res {
                Ok((n, k)) => {
                    self.g.nodes.insert(id, n);
                    self.status(format!("Preset applied: {k} value(s) set"), true);
                    self.dirty = true;
                }
                Err(e) => {
                    self.undo.pop();
                    self.status(e, false);
                }
            }
        }
        if let Some((param, cur)) = make_fn {
            self.snap();
            let owner = self.g.meta.get("identifier").cloned().unwrap_or_default();
            if let Some(pkg) = self.g.sd.as_ref().map(|s| s.pkg.clone()) {
                let name = native::make_function(&mut self.g.subgraphs, pkg, &owner, id, &param, &cur);
                if let Some(n) = self.g.nodes.get_mut(&id) {
                    native::set_value(n, &param, Some(PVal::Dyn { graph: name, rel: Some("0".into()) }));
                }
                self.dirty = true;
            }
        }
        if !sets.is_empty() || !strs.is_empty() {
            if !self.edit_open {
                self.snap();
                self.edit_open = true;
            }
            if let Some(n) = self.g.nodes.get_mut(&id) {
                for (k, v) in sets {
                    native::set_value(n, &k, v);
                }
                for (k, v) in strs {
                    match v {
                        Some(s) => {
                            n.strs.insert(k, s);
                        }
                        None => {
                            n.strs.remove(&k);
                        }
                    }
                }
            }
            self.dirty = true;
        }
        if let Some(g) = enter {
            self.acts.push(Act::EnterSub(g));
        }
    }

    /// Graph settings of an `.sbs` graph: its size and format (relative to the parent or fixed), seed and details.
    fn sd_graph_props(&mut self, ui: &mut egui::Ui) {
        let Some(sd) = self.g.sd.clone() else { return };
        // a function inside a node's value, or an FX-Map's pattern graph: it has no settings of its own
        match &sd.role {
            native::Role::Param { node, param, .. } => {
                ui.add_space(4.0);
                ui.label(format!("This function computes '{}' of node #{node}.", sdefs::label_of(param)));
                ui.small("Add function nodes (Tab), connect them, and make the last one the result (select it, 'Make this the result').");
                return;
            }
            native::Role::Fx { node, .. } => {
                ui.add_space(4.0);
                ui.label(format!("The pattern graph of FX-Map node #{node}."));
                ui.small("Quadrant nodes draw a pattern in their quarter of the image and pass four quarters on; Iterate repeats, Switch picks. The result node is where drawing starts.");
                return;
            }
            _ => {}
        }
        let is_graph = sd.role == native::Role::Graph;
        let mut base = sd.base.clone();
        let mut meta = self.g.meta.clone();
        if is_graph {
            egui::CollapsingHeader::new(egui::RichText::new("GRAPH SETTINGS").strong()).id_salt("sd_gs").default_open(true).show(ui, |ui| {
                // output size: log2 values, absolute or relative to the parent
                let (rel, v) = match base.get("outputsize").and_then(|v| PVal::dec(v)) {
                    Some(PVal::Const { raw, rel, .. }) => {
                        let n: Vec<i32> = raw.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                        (rel.unwrap_or_else(|| "1".into()), [n.first().copied().unwrap_or(0), n.get(1).copied().unwrap_or(0)])
                    }
                    _ => ("1".into(), [0, 0]),
                };
                let absolute = rel == "0";
                let mut abs = absolute;
                ui.horizontal(|ui| {
                    row_label(ui, "Output size");
                    if ui.selectable_label(!abs, "Relative to parent").clicked() {
                        abs = false;
                    }
                    if ui.selectable_label(abs, "Absolute").clicked() {
                        abs = true;
                    }
                });
                let mut w = v;
                if abs != absolute {
                    w = if abs { [9, 9] } else { [0, 0] };
                }
                ui.horizontal(|ui| {
                    row_label(ui, "Width / height");
                    for (k, x) in w.iter_mut().enumerate() {
                        let fmt = move |x: f64| if abs { format!("{} px", 1i64 << (x as i64).clamp(4, 13)) } else if x >= 0.0 { format!("x{}", 1i64 << (x as i64).clamp(0, 12)) } else { format!("/{}", 1i64 << (-x as i64).clamp(0, 12)) };
                        let range = if abs { 4..=13 } else { -12..=12 };
                        ui.add(egui::DragValue::new(x).range(range).speed(0.05).custom_formatter(move |v, _| fmt(v)));
                        let _ = k;
                    }
                });
                if abs != absolute || w != v {
                    base.insert("outputsize".into(), PVal::Const { ty: PTy::I(2), raw: format!("{} {}", w[0], w[1]), rel: Some(if abs { "0" } else { "1" }.into()) }.enc());
                }
                let fmt_now = match base.get("format").and_then(|v| PVal::dec(v)).map(|v| v.nums()) {
                    Some(n) => n.first().copied().unwrap_or(0.0) as usize,
                    None => 0,
                };
                let names = ["8 bits", "16 bits", "16 bits float", "32 bits float"];
                ui.horizontal(|ui| {
                    row_label(ui, "Output format");
                    egui::ComboBox::from_id_salt("sd_fmt").width(150.0).selected_text(names[fmt_now.min(3)]).show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            if ui.selectable_label(fmt_now == i, *n).clicked() {
                                base.insert("format".into(), PVal::Const { ty: PTy::I(1), raw: i.to_string(), rel: Some("0".into()) }.enc());
                            }
                        }
                    });
                });
                let mut seed = base.get("randomseed").and_then(|v| PVal::dec(v)).map(|v| v.nums().first().copied().unwrap_or(0.0) as i64).unwrap_or(0);
                ui.horizontal(|ui| {
                    row_label(ui, "Random seed");
                    if ui.add(egui::DragValue::new(&mut seed).range(0..=1_000_000)).changed() {
                        base.insert("randomseed".into(), PVal::Const { ty: PTy::I(1), raw: seed.to_string(), rel: Some("0".into()) }.enc());
                    }
                });
            });
        }

        // the graph's parameters: their defaults, labels and slider ranges
        let inputs = native::inputs_info(&self.g);
        let mut input_edit: Option<(String, Option<Vec<f32>>, Option<String>, Option<(f32, f32)>)> = None;
        let mut input_remove: Option<String> = None;
        if !inputs.is_empty() {
            egui::CollapsingHeader::new(egui::RichText::new("INPUT PARAMETERS").strong()).default_open(true).show(ui, |ui| {
                for inp in &inputs {
                    ui.push_id(("sd_input", inp.id.clone()), |ui| {
                        ui.horizontal(|ui| {
                            let mut l = inp.label.clone();
                            if ui.add(egui::TextEdit::singleline(&mut l).desired_width(120.0)).on_hover_text(format!("identifier: {}", inp.id)).changed() && !l.trim().is_empty() {
                                input_edit = Some((inp.id.clone(), None, Some(l), None));
                            }
                            if ui.small_button("x").on_hover_text("Remove this parameter: the values it set go back to its default").clicked() {
                                input_remove = Some(inp.id.clone());
                            }
                        });
                        ui.horizontal(|ui| {
                            row_label(ui, "Default");
                            let nums: Vec<f32> = inp.default.split_whitespace().filter_map(|x| x.parse::<f32>().ok()).collect();
                            let mut set: Option<Vec<f32>> = None;
                            match inp.ty {
                                PTy::S | PTy::Path => {
                                    ui.label(&inp.default);
                                }
                                PTy::B => {
                                    let mut b = nums.first().copied().unwrap_or(0.0) != 0.0 || matches!(inp.default.trim(), "true" | "True");
                                    if ui.checkbox(&mut b, "").changed() {
                                        set = Some(vec![b as i32 as f32]);
                                    }
                                }
                                PTy::I(1) if !inp.choices.is_empty() => {
                                    let v = nums.first().copied().unwrap_or(0.0).round() as i64;
                                    let shown = inp.choices.get(v.max(0) as usize).cloned().unwrap_or_else(|| v.to_string());
                                    egui::ComboBox::from_id_salt("choice").width(150.0).selected_text(shown).show_ui(ui, |ui| {
                                        for (i, c) in inp.choices.iter().enumerate() {
                                            if ui.selectable_label(v == i as i64, c).clicked() {
                                                set = Some(vec![i as f32]);
                                            }
                                        }
                                    });
                                }
                                PTy::F(1) | PTy::I(1) => {
                                    let (lo, hi) = inp.range.unwrap_or(if matches!(inp.ty, PTy::I(_)) { (0.0, 10.0) } else { (0.0, 1.0) });
                                    let mut x = nums.first().copied().unwrap_or(0.0);
                                    let r = if matches!(inp.ty, PTy::I(_)) {
                                        let mut i = x.round() as i64;
                                        let r = ui.add(egui::Slider::new(&mut i, lo as i64..=hi as i64).clamping(egui::SliderClamping::Never));
                                        x = i as f32;
                                        r
                                    } else {
                                        ui.add(egui::Slider::new(&mut x, lo..=hi).clamping(egui::SliderClamping::Never))
                                    };
                                    if r.changed() {
                                        set = Some(vec![x]);
                                    }
                                }
                                PTy::F(4) | PTy::F(3) if nums.len() >= 3 => {
                                    let mut c = [nums[0], nums[1], nums[2], nums.get(3).copied().unwrap_or(1.0)];
                                    if color_field(ui, ui.next_auto_id(), &mut c, true).changed() {
                                        set = Some(c[..inp.ty.lanes()].to_vec());
                                    }
                                }
                                _ => {
                                    let mut v: Vec<f32> = (0..inp.ty.lanes().max(1)).map(|i| nums.get(i).copied().unwrap_or(0.0)).collect();
                                    let mut changed = false;
                                    for x in v.iter_mut() {
                                        changed |= ui.add(egui::DragValue::new(x).speed(if matches!(inp.ty, PTy::I(_)) { 0.1 } else { 0.005 })).changed();
                                    }
                                    if changed {
                                        set = Some(v);
                                    }
                                }
                            }
                            if let Some(v) = set {
                                input_edit = Some((inp.id.clone(), Some(v), None, None));
                            }
                        });
                        if let (Some((lo, hi)), PTy::F(1) | PTy::I(1)) = (inp.range, inp.ty) {
                            if inp.choices.is_empty() {
                                ui.horizontal(|ui| {
                                    row_label(ui, "Range");
                                    let (mut a, mut b) = (lo, hi);
                                    let ra = ui.add(egui::DragValue::new(&mut a).speed(0.01));
                                    let rb = ui.add(egui::DragValue::new(&mut b).speed(0.01));
                                    if (ra.changed() || rb.changed()) && a < b {
                                        input_edit = Some((inp.id.clone(), None, None, Some((a, b))));
                                    }
                                });
                            }
                        }
                    });
                    ui.add_space(4.0);
                }
            });
        }
        if let Some((ident, value, label, range)) = input_edit {
            if !self.edit_open {
                self.snap();
                self.edit_open = true;
            }
            match native::set_input(&mut self.g, &ident, value.as_deref(), label.as_deref(), range) {
                Ok(_) => self.dirty = true,
                Err(e) => self.status(e, false),
            }
        }
        if let Some(ident) = input_remove {
            self.snap();
            match native::unexpose(&mut self.g, &ident) {
                Ok(()) => self.dirty = true,
                Err(e) => {
                    self.undo.pop();
                    self.status(e, false);
                }
            }
        }
        // presets: named sets of parameter values, offered on the nodes that run this graph
        if is_graph && !inputs.is_empty() {
            let presets = native::presets(&self.g);
            let mut rename: Option<(usize, String)> = None;
            let mut apply: Option<usize> = None;
            let mut remove: Option<usize> = None;
            let mut add = false;
            egui::CollapsingHeader::new(egui::RichText::new("PRESETS").strong()).id_salt("sd_presets").default_open(!presets.is_empty()).show(ui, |ui| {
                if presets.is_empty() {
                    ui.small("A preset keeps the parameters' current defaults under a name; nodes that run this graph can apply it.");
                }
                for (i, p) in presets.iter().enumerate() {
                    ui.push_id(("sd_preset_row", i), |ui| {
                        ui.horizontal(|ui| {
                            let mut l = p.label.clone();
                            if ui.add(egui::TextEdit::singleline(&mut l).desired_width(120.0)).on_hover_text(format!("{} parameter value(s)", p.values.len())).changed() && !l.trim().is_empty() {
                                rename = Some((i, l));
                            }
                            if ui.small_button("Apply").on_hover_text("Make this preset's values the parameters' defaults").clicked() {
                                apply = Some(i);
                            }
                            if ui.small_button("x").on_hover_text("Delete this preset").clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                }
                if ui.button("+ New preset").on_hover_text("A preset holding every parameter's current default").clicked() {
                    add = true;
                }
            });
            if let Some((i, l)) = rename {
                if !self.edit_open {
                    self.snap();
                    self.edit_open = true;
                }
                match native::rename_preset(&mut self.g, i, &l) {
                    Ok(()) => self.dirty = true,
                    Err(e) => self.status(e, false),
                }
            }
            let edit: Option<Result<String, String>> = if add {
                self.snap();
                Some(native::add_preset(&mut self.g, "").map(|i| format!("Added '{}'", native::presets(&self.g).get(i).map(|p| p.label.clone()).unwrap_or_default())))
            } else if let Some(i) = apply {
                self.snap();
                Some(native::apply_preset_defaults(&mut self.g, i).map(|k| format!("Preset applied: {k} default(s) set")))
            } else if let Some(i) = remove {
                self.snap();
                Some(native::remove_preset(&mut self.g, i).map(|()| "Preset deleted".to_string()))
            } else {
                None
            };
            match edit {
                Some(Ok(msg)) => {
                    self.status(msg, true);
                    self.dirty = true;
                }
                Some(Err(e)) => {
                    self.undo.pop();
                    self.status(e, false);
                }
                None => {}
            }
        }
        egui::CollapsingHeader::new(egui::RichText::new("DETAILS").strong()).default_open(false).show(ui, |ui| {
            for (label, key) in [("Label", "label"), ("Description", "description"), ("Category", "category"), ("Author", "author"), ("Author URL", "authorURL"), ("Tags", "tags")] {
                meta_field(ui, &mut meta, label, key, key == "description");
            }
        });
        if base != sd.base || meta != self.g.meta {
            if !self.edit_open {
                self.snap();
                self.edit_open = true;
            }
            if let Some(s) = self.g.sd.as_mut() {
                s.base = base;
            }
            self.g.meta = meta;
            self.dirty = true;
        }
    }
}

impl App {
    /// The functions of an `.sbs` package: open them, or make a new one.
    fn sd_functions_panel(&mut self, ui: &mut egui::Ui) {
        let list: Vec<String> = native::all_graphs(&self.g).into_iter().filter(|(_, g)| g.sd.as_ref().map(|s| s.role == native::Role::Function).unwrap_or(false)).map(|(k, _)| k).collect();
        let mut open = None;
        let mut make = false;
        egui::CollapsingHeader::new(egui::RichText::new("FUNCTIONS").strong()).default_open(!list.is_empty()).show(ui, |ui| {
            if list.is_empty() {
                ui.small("A function of the package computes a value; nodes' values and other functions can call it.");
            }
            for name in &list {
                ui.horizontal(|ui| {
                    ui.label(name);
                    if ui.small_button("Open").clicked() {
                        open = Some(name.clone());
                    }
                });
            }
            if ui.button("New function").clicked() {
                make = true;
            }
        });
        if let Some(n) = open {
            self.acts.push(Act::EnterSub(n));
        }
        if make {
            let all = native::all_graphs(&self.g);
            let mut k = 1;
            let name = loop {
                let c = format!("function_{k}");
                if !all.contains_key(&c) && !self.parents.iter().any(|(p, _)| p.subgraphs.contains_key(&c)) {
                    break c;
                }
                k += 1;
            };
            self.snap();
            match native::create_graph(&mut self.g, &name, "function") {
                Ok(()) => {
                    self.dirty = true;
                    self.status(format!("Created function '{name}'. Open it to add nodes; the node marked as the result gives its value."), true);
                }
                Err(e) => self.status(e, false),
            }
        }
    }
}

const LEVELS_CHANNELS: &[&str] = &["Luminance Channel", "Red Channel", "Green Channel", "Blue Channel", "Alpha Channel"];

struct LevelsVals {
    in_lo: [f32; 4],
    in_mid: [f32; 4],
    in_hi: [f32; 4],
    out_lo: [f32; 4],
    out_hi: [f32; 4],
}

/// Luminance sets red, green and blue together; a channel sets its own lane.
fn set_lane(v: &mut [f32; 4], chan: usize, x: f32) {
    match chan {
        0 => {
            v[0] = x;
            v[1] = x;
            v[2] = x;
        }
        c => v[c - 1] = x,
    }
}

/// Designer's Levels control: the histogram of the input with the three input handles above it and the two output
/// handles below. `src` is the input's thumbnail (RGBA, linear, THUMB x THUMB). Returns true when a handle moved.
fn levels_widget(ui: &mut egui::Ui, id_src: (&str, u32), lv: &mut LevelsVals, chan: usize, src: Option<&[u8]>) -> bool {
    let lane = if chan == 0 { 0 } else { chan - 1 };
    let full_w = ui.available_width() - 12.0;
    let (rect, _) = ui.allocate_exact_size(vec2(full_w, 200.0), Sense::hover());
    let painter = ui.painter();
    let pad = 8.0;
    let hist = Rect::from_min_max(pos2(rect.left() + pad, rect.top() + 14.0), pos2(rect.right() - pad, rect.bottom() - 34.0));
    // histogram of the chosen channel
    let mut bins = [0u32; 128];
    if let Some(px) = src {
        for p in px.chunks_exact(4) {
            let v = match chan {
                0 => (0.3 * p[0] as f32 + 0.59 * p[1] as f32 + 0.11 * p[2] as f32) / 255.0,
                c => p[c - 1] as f32 / 255.0,
            };
            bins[((v * 127.0).round() as usize).min(127)] += 1;
        }
    }
    painter.rect_filled(hist, 0.0, Color32::from_gray(150));
    let peak = bins.iter().copied().max().unwrap_or(1).max(1) as f32;
    let bw = hist.width() / 128.0;
    for (i, b) in bins.iter().enumerate() {
        if *b == 0 {
            continue;
        }
        let h = (*b as f32 / peak).sqrt() * (hist.height() - 2.0);
        let x0 = hist.left() + i as f32 * bw;
        painter.rect_filled(Rect::from_min_max(pos2(x0, hist.bottom() - h), pos2(x0 + bw + 0.5, hist.bottom())), 0.0, Color32::from_gray(95));
    }
    painter.text(hist.left_bottom() + vec2(3.0, -2.0), Align2::LEFT_BOTTOM, "0", FontId::proportional(10.0), Color32::from_gray(120));
    painter.text(hist.right_bottom() + vec2(-3.0, -2.0), Align2::RIGHT_BOTTOM, "255", FontId::proportional(10.0), Color32::from_gray(120));
    // the output ramp under the histogram
    let ramp = Rect::from_min_max(pos2(hist.left(), hist.bottom() + 6.0), pos2(hist.right(), hist.bottom() + 14.0));
    let n = 32;
    for i in 0..n {
        let t0 = i as f32 / n as f32;
        let t1 = (i + 1) as f32 / n as f32;
        let g = (t0 * 255.0) as u8;
        painter.rect_filled(Rect::from_min_max(pos2(ramp.left() + ramp.width() * t0, ramp.top()), pos2(ramp.left() + ramp.width() * t1 + 0.5, ramp.bottom())), 0.0, Color32::from_gray(g));
    }
    let mut changed = false;
    let x_of = |v: f32| hist.left() + v.clamp(0.0, 1.0) * hist.width();
    let v_of = |x: f32| ((x - hist.left()) / hist.width()).clamp(0.0, 1.0);
    // handles: (value, y of the tip, points up?, fill)
    let (lo, hi, mid) = (lv.in_lo[lane], lv.in_hi[lane], lv.in_mid[lane]);
    let mid_abs = lo + (hi - lo) * mid;
    let top_y = hist.top() - 2.0;
    let bot_y = hist.bottom() + 16.0;
    let handles: [(f32, f32, bool, Color32, &str); 5] = [
        (lo, top_y, false, Color32::from_gray(20), "in_lo"),
        (mid_abs, top_y, false, Color32::from_gray(120), "in_mid"),
        (hi, top_y, false, Color32::from_gray(245), "in_hi"),
        (lv.out_lo[lane], bot_y, true, Color32::from_gray(20), "out_lo"),
        (lv.out_hi[lane], bot_y, true, Color32::from_gray(245), "out_hi"),
    ];
    let base = Id::new(id_src);
    let size = 7.0;
    for (v, y, up, fill, name) in handles {
        let x = x_of(v);
        let tri = if up {
            vec![pos2(x, y - size), pos2(x - size, y + size * 0.6), pos2(x + size, y + size * 0.6)]
        } else {
            vec![pos2(x, y + size), pos2(x - size, y - size * 0.6), pos2(x + size, y - size * 0.6)]
        };
        let hit = Rect::from_center_size(pos2(x, y), vec2(size * 2.4, size * 2.4));
        let r = ui.interact(hit, base.with(name), Sense::click_and_drag());
        let stroke = if r.hovered() || r.dragged() { Color32::WHITE } else { Color32::from_gray(60) };
        painter.add(egui::Shape::convex_polygon(tri, fill, Stroke::new(1.0_f32, stroke)));
        if r.dragged() {
            if let Some(p) = r.interact_pointer_pos() {
                let nv = v_of(p.x);
                match name {
                    "in_lo" => {
                        let nv = nv.min(hi - 0.002);
                        set_lane(&mut lv.in_lo, chan, nv);
                    }
                    "in_hi" => {
                        let nv = nv.max(lo + 0.002);
                        set_lane(&mut lv.in_hi, chan, nv);
                    }
                    "in_mid" => {
                        let span = (hi - lo).max(1e-4);
                        set_lane(&mut lv.in_mid, chan, ((nv - lo) / span).clamp(0.001, 0.999));
                    }
                    "out_lo" => set_lane(&mut lv.out_lo, chan, nv),
                    _ => set_lane(&mut lv.out_hi, chan, nv),
                }
                changed = true;
            }
        }
        if r.double_clicked() {
            let d = match name {
                "in_lo" | "out_lo" => 0.0,
                "in_mid" => 0.5,
                _ => 1.0,
            };
            match name {
                "in_lo" => set_lane(&mut lv.in_lo, chan, d),
                "in_hi" => set_lane(&mut lv.in_hi, chan, d),
                "in_mid" => set_lane(&mut lv.in_mid, chan, d),
                "out_lo" => set_lane(&mut lv.out_lo, chan, d),
                _ => set_lane(&mut lv.out_hi, chan, d),
            }
            changed = true;
        }
    }
    changed
}
