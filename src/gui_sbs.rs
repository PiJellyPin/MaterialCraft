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
    native::slots(n, lib).iter().map(|s| (intern(s), false)).collect()
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
        if node.kind == "sbs:output" {
            let uid = node.strs.get("#output").cloned().unwrap_or_default();
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
                        changed |= ui.color_edit_button_rgba_unmultiplied(col).changed();
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
        let mut draw_group = |ui: &mut egui::Ui, gname: &str| {
                for p in params.iter().filter(|p| p.group == gname) {
                    ui.horizontal(|ui| {
                        row_label(ui, &p.label);
                        let cur = p.current();
                        match &cur {
                            PVal::Dyn { graph, .. } if native::reads_input(&self.g, graph).is_some() => {
                                let ident = native::reads_input(&self.g, graph).unwrap_or_default();
                                ui.label(egui::RichText::new(format!("graph parameter '{ident}'")).italics().color(Color32::from_rgb(120, 170, 235)))
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
                                        if ui.color_edit_button_rgba_unmultiplied(&mut c).changed() {
                                            set(c.to_vec(), &mut sets);
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
                                if p.value.is_some() && ui.small_button("↺").on_hover_text("Back to the default").clicked() {
                                    sets.push((p.id.clone(), None));
                                }
                                if !matches!(p.ty, PTy::S | PTy::Path) && ui.small_button("fn").on_hover_text("Compute this value with a function").clicked() {
                                    make_fn = Some((p.id.clone(), cur.clone()));
                                }
                                if self.g.sd.as_ref().map(|s| s.role == native::Role::Graph).unwrap_or(false) && p.group != "Base" && ui.small_button("P").on_hover_text("Expose: make this a parameter of the graph, set by whoever uses it").clicked() {
                                    expose = Some((p.id.clone(), p.label.clone()));
                                }
                            }
                        }
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
                                    if ui.color_edit_button_rgba_unmultiplied(&mut c).changed() {
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
