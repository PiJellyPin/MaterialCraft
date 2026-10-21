// SPDX-License-Identifier: MIT OR Apache-2.0
// The mesh map baker window (included into gui.rs). Bakes run on a background thread with progress and cancel; the
// result goes into the package as 16-bit PNG resources that Baked Map nodes read, and optionally into a folder.

type MeshInfoMsg = (PathBuf, Option<std::time::SystemTime>, Result<String, String>);

struct BakeJob {
    rx: Receiver<Result<(bake::BakeResult, Vec<PathBuf>), String>>,
    prog: std::sync::Arc<bake::Progress>,
    started: std::time::Instant,
}

struct BakeUi {
    open: bool,
    tab: u8,
    name: String,
    low: Vec<PathBuf>,
    high: Vec<PathBuf>,
    cage: Option<PathBuf>,
    out: Option<PathBuf>,
    s: BakeSettings,
    job: Option<BakeJob>,
    msg: String,
    ok: bool,
    report: Option<bake::BakeReport>,
    thumbs: BTreeMap<String, egui::TextureHandle>,
    shown: Option<String>,
    auto: bool,
    info: HashMap<PathBuf, (Option<std::time::SystemTime>, Result<String, String>)>,
    info_pending: HashSet<PathBuf>,
    info_tx: std::sync::mpsc::Sender<MeshInfoMsg>,
    info_rx: Receiver<MeshInfoMsg>,
    watch: Vec<(PathBuf, Option<std::time::SystemTime>)>,
    last_watch: std::time::Instant,
}

impl BakeUi {
    fn new() -> BakeUi {
        let (info_tx, info_rx) = channel();
        BakeUi {
            open: false,
            tab: 0,
            name: "bake".into(),
            low: Vec::new(),
            high: Vec::new(),
            cage: None,
            out: None,
            s: BakeSettings::preview(),
            job: None,
            msg: String::new(),
            ok: true,
            report: None,
            thumbs: BTreeMap::new(),
            shown: None,
            auto: false,
            info: HashMap::new(),
            info_pending: HashSet::new(),
            info_tx,
            info_rx,
            watch: Vec::new(),
            last_watch: std::time::Instant::now(),
        }
    }

    fn all_files(&self) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = self.low.iter().chain(self.high.iter()).cloned().collect();
        v.extend(self.cage.clone());
        v
    }
}

/// A name that is safe inside a package resource path.
fn bake_set_name(s: &str) -> String {
    let n: String = s.trim().chars().map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    if n.is_empty() {
        "bake".into()
    } else {
        n
    }
}

fn mtime_of(p: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn normal_mode_combo(ui: &mut egui::Ui, label: &str, m: &mut bake::NormalMode) {
    use bake::NormalMode as N;
    let names = ["Use the file's normals", "Flat (hard edges)", "Smooth by angle", "Smoothing groups"];
    let idx = match m {
        N::Keep => 0,
        N::Flat => 1,
        N::Angle(_) => 2,
        N::Groups => 3,
    };
    let mut new = idx;
    egui::ComboBox::from_label(label).selected_text(names[idx]).show_ui(ui, |ui| {
        for (i, n) in names.iter().enumerate() {
            ui.selectable_value(&mut new, i, *n);
        }
    });
    if new != idx {
        *m = match new {
            0 => N::Keep,
            1 => N::Flat,
            2 => N::Angle(60.0),
            _ => N::Groups,
        };
    }
    if let N::Angle(a) = m {
        ui.add(egui::Slider::new(a, 1.0..=180.0).text("Angle limit (degrees)"));
    }
}

impl App {
    fn bake_apply_preset(&mut self, p: BakeSettings) {
        let s = &mut self.bake.s;
        s.width = p.width;
        s.height = p.height;
        s.aa = p.aa;
        s.padding = p.padding;
        s.ao_rays = p.ao_rays;
        s.thick_rays = p.thick_rays;
    }

    fn bake_set(&self) -> bake::BakeSet {
        let s = |p: &PathBuf| p.to_string_lossy().to_string();
        bake::BakeSet { name: bake_set_name(&self.bake.name), low: self.bake.low.iter().map(s).collect(), high: self.bake.high.iter().map(s).collect(), cage: self.bake.cage.as_ref().map(s), settings: self.bake.s.clone() }
    }

    fn bake_load_set(&mut self, set: bake::BakeSet) {
        self.bake.name = set.name;
        self.bake.low = set.low.iter().map(PathBuf::from).collect();
        self.bake.high = set.high.iter().map(PathBuf::from).collect();
        self.bake.cage = set.cage.as_ref().map(PathBuf::from);
        self.bake.s = set.settings;
        self.bake.watch.clear();
    }

    /// Read the sizes of the listed mesh files on worker threads, again when a file changes.
    fn bake_refresh_info(&mut self) {
        while let Ok((p, m, r)) = self.bake.info_rx.try_recv() {
            self.bake.info_pending.remove(&p);
            self.bake.info.insert(p, (m, r));
        }
        for p in self.bake.all_files() {
            let m = mtime_of(&p);
            let stale = match self.bake.info.get(&p) {
                Some((old, _)) => *old != m,
                None => true,
            };
            if stale && !self.bake.info_pending.contains(&p) {
                self.bake.info_pending.insert(p.clone());
                let tx = self.bake.info_tx.clone();
                std::thread::spawn(move || {
                    let r = materialcraft::meshio::load_file(&p).map(|mesh| {
                        let st = mesh.stats();
                        format!(
                            "{} triangles, {} object(s), {} material(s){}{}{}",
                            st.tris,
                            st.objects.max(1),
                            st.materials.max(1),
                            if mesh.has_uv { ", UVs" } else { ", no UVs" },
                            if mesh.has_uv1 { " + second UV set" } else { "" },
                            if mesh.has_normals { ", normals" } else { "" }
                        )
                    });
                    let _ = tx.send((p, m, r));
                });
            }
        }
    }

    fn start_bake(&mut self, ctx: &egui::Context) {
        if self.bake.job.is_some() {
            return;
        }
        self.bake.ok = false;
        if self.bake.low.is_empty() {
            self.bake.msg = "Add at least one low-poly mesh (it needs UV coordinates).".into();
            return;
        }
        if self.bake.s.maps.is_empty() {
            self.bake.msg = "Tick at least one map to bake.".into();
            return;
        }
        if self.bake.out.is_none() {
            self.bake.msg = "Choose the output folder (Output tab): the maps are written there as 16-bit PNG files.".into();
            return;
        }
        let set = self.bake_set();
        let out = self.bake.out.clone();
        let prog = std::sync::Arc::new(bake::Progress::new());
        let (tx, rx) = channel();
        let (p2, ctx2) = (prog.clone(), ctx.clone());
        self.bake.watch = self.bake.all_files().into_iter().map(|p| (mtime_of(&p), p)).map(|(m, p)| (p, m)).collect();
        self.bake.job = Some(BakeJob { rx, prog, started: std::time::Instant::now() });
        self.bake.msg = "Baking...".into();
        self.bake.ok = true;
        std::thread::spawn(move || {
            let r = (|| -> Result<(bake::BakeResult, Vec<PathBuf>), String> {
                let res = bake::run_set(&set, std::path::Path::new("."), Some(&p2))?;
                let mut files = Vec::new();
                if let Some(dir) = out {
                    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                    for (name, t) in &res.maps {
                        let p = dir.join(format!("{}_{name}.png", set.name));
                        materialcraft::safeio::write_file(&p, &png::encode(t.w, t.h, &t.px, 16)).map_err(|e| format!("{}: {e}", p.display()))?;
                        files.push(p);
                    }
                }
                Ok((res, files))
            })();
            let _ = tx.send(r);
            ctx2.request_repaint();
        });
    }

    /// Called every frame: collects a finished bake, watches the mesh files for auto-update.
    fn poll_bake(&mut self, ctx: &egui::Context) {
        self.bake_refresh_info();
        let mut done: Option<Result<(bake::BakeResult, Vec<PathBuf>), String>> = None;
        if let Some(j) = &self.bake.job {
            match j.rx.try_recv() {
                Ok(r) => done = Some(r),
                Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(80)),
                Err(TryRecvError::Disconnected) => done = Some(Err("the baking thread stopped unexpectedly".into())),
            }
        }
        if let Some(r) = done {
            let secs = self.bake.job.take().map(|j| j.started.elapsed().as_secs_f32()).unwrap_or(0.0);
            match r {
                Ok((res, files)) => self.finish_bake(ctx, res, files, secs),
                Err(e) => {
                    self.bake.ok = false;
                    self.bake.msg = if e.contains("cancelled") { "Bake cancelled.".into() } else { format!("Bake failed: {e}") };
                }
            }
        }
        if self.bake.auto && self.bake.job.is_none() && !self.bake.watch.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
            if self.bake.last_watch.elapsed() >= std::time::Duration::from_millis(900) {
                self.bake.last_watch = std::time::Instant::now();
                let changed = self.bake.watch.iter().any(|(p, m)| mtime_of(p) != *m);
                let same_files = self.bake.all_files().len() == self.bake.watch.len();
                if changed && same_files {
                    self.start_bake(ctx);
                }
            }
        }
    }

    fn finish_bake(&mut self, ctx: &egui::Context, res: bake::BakeResult, files: Vec<PathBuf>, secs: f32) {
        // thumbnails (at most 512 px so a 4k bake does not fill the graphics memory)
        self.bake.thumbs.clear();
        for (name, t) in &res.maps {
            let (w, h) = if t.w.max(t.h) > 512 {
                let k = 512.0 / t.w.max(t.h) as f32;
                (((t.w as f32 * k) as usize).max(1), ((t.h as f32 * k) as usize).max(1))
            } else {
                (t.w, t.h)
            };
            let small = if (w, h) == (t.w, t.h) { t.clone() } else { Tex::from_fn(w, h, |u, v| t.sample(u, v)) };
            self.bake.thumbs.insert(name.clone(), ctx.load_texture(format!("bake_{name}"), qimg(&small), egui::TextureOptions::LINEAR));
        }
        if self.bake.shown.as_ref().map(|s| !res.maps.contains_key(s)).unwrap_or(true) {
            self.bake.shown = res.maps.keys().next().cloned();
        }
        let mut parts: Vec<String> = Vec::new();
        if !files.is_empty() {
            parts.push(format!("{} PNG file(s) written", files.len()));
        }
        self.bake.ok = true;
        self.bake.msg = format!("Done in {secs:.1} s: {}.", parts.join(", "));
        self.bake.report = Some(res.report);
    }

    fn bake_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let busy = self.bake.job.is_some();
        egui::Window::new("Mesh map baker").open(&mut open).collapsible(false).resizable(true).default_size([860.0, 680.0]).show(ctx, |ui| {
            // ---- header: set name, presets, set files
            ui.horizontal_wrapped(|ui| {
                ui.label("Bake set");
                ui.add(egui::TextEdit::singleline(&mut self.bake.name).desired_width(140.0));
                ui.separator();
                ui.label("Quality:");
                if ui.button("Preview").on_hover_text("512 px, no anti-aliasing, few rays").clicked() {
                    self.bake_apply_preset(BakeSettings::preview());
                }
                if ui.button("Game").on_hover_text("2048 px, 2x2 anti-aliasing").clicked() {
                    self.bake_apply_preset(BakeSettings::game());
                }
                if ui.button("Final").on_hover_text("4096 px, 4x4 anti-aliasing, many rays").clicked() {
                    self.bake_apply_preset(BakeSettings::final_quality());
                }
                ui.separator();
                if ui.button("Load set file...").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Bake set", &["bakeset", "txt"]).pick_file() {
                        let nm = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "bake".into());
                        match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| bake::BakeSet::parse(&nm, &t)) {
                            Ok(set) => self.bake_load_set(set),
                            Err(e) => {
                                self.bake.ok = false;
                                self.bake.msg = format!("{}: {e}", p.display());
                            }
                        }
                    }
                }
                if ui.button("Save set file...").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Bake set", &["bakeset"]).set_file_name(format!("{}.bakeset", bake_set_name(&self.bake.name))).save_file() {
                        let r = materialcraft::safeio::write_file(&p, self.bake_set().to_text().as_bytes());
                        self.bake.ok = r.is_ok();
                        self.bake.msg = match r {
                            Ok(()) => format!("Saved {}", p.display()),
                            Err(e) => format!("Cannot save: {e}"),
                        };
                    }
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                for (i, t) in ["Meshes", "Settings", "Maps", "Output"].iter().enumerate() {
                    if ui.selectable_label(self.bake.tab == i as u8, *t).clicked() {
                        self.bake.tab = i as u8;
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().id_salt("bake_tab").max_height(300.0).auto_shrink([false, false]).show(ui, |ui| match self.bake.tab {
                0 => self.bake_tab_meshes(ui),
                1 => self.bake_tab_settings(ui),
                2 => self.bake_tab_maps(ui),
                _ => self.bake_tab_output(ui),
            });
            ui.separator();
            // ---- run
            ui.horizontal(|ui| {
                if busy {
                    if ui.button("Cancel").clicked() {
                        if let Some(j) = &self.bake.job {
                            j.prog.request_cancel();
                        }
                    }
                    if let Some(j) = &self.bake.job {
                        ui.add(egui::ProgressBar::new(j.prog.fraction()).desired_width(260.0).text(j.prog.stage()));
                    }
                } else if ui.add(egui::Button::new("Bake").min_size(vec2(90.0, 26.0))).clicked() {
                    self.start_bake(ctx);
                }
                ui.checkbox(&mut self.bake.auto, "Re-bake when a mesh file changes");
            });
            if !self.bake.msg.is_empty() {
                let c = if self.bake.ok { ui.visuals().text_color() } else { Color32::from_rgb(255, 130, 120) };
                ui.colored_label(c, &self.bake.msg);
            }
            if let Some(r) = &self.bake.report {
                ui.small(format!(
                    "Traced on {}: {} low / {} high triangles, {} of {} texels covered ({:.0}%), {} rays, {} hits, {} misses, {} texels shared by overlapping faces",
                    r.device,
                    r.low_tris,
                    r.high_tris,
                    r.covered,
                    r.texels,
                    100.0 * r.covered as f32 / r.texels.max(1) as f32,
                    r.rays,
                    r.hits,
                    r.misses,
                    r.overlapped
                ));
                for w in &r.warnings {
                    ui.colored_label(Color32::from_rgb(235, 190, 90), format!("Warning: {w}"));
                }
            }
            // ---- result
            if !self.bake.thumbs.is_empty() {
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let names: Vec<String> = self.bake.thumbs.keys().cloned().collect();
                    for n in names {
                        let sel = self.bake.shown.as_deref() == Some(n.as_str());
                        if ui.selectable_label(sel, bake::MapKind::from_id(&n).map(|m| m.label()).unwrap_or("map")).clicked() {
                            self.bake.shown = Some(n);
                        }
                    }
                });
                if let Some(t) = self.bake.shown.as_ref().and_then(|n| self.bake.thumbs.get(n)) {
                    let sz = t.size_vec2();
                    let k = (300.0 / sz.x.max(sz.y)).min(1.0e3);
                    ui.add(egui::Image::new((t.id(), sz * k)));
                }
            }
        });
        if !open {
            self.bake.open = false;
        }
    }

    fn bake_mesh_list(&mut self, ui: &mut egui::Ui, high: bool) {
        let list = if high { self.bake.high.clone() } else { self.bake.low.clone() };
        let mut remove: Option<usize> = None;
        for (i, p) in list.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.small_button("x").on_hover_text("Remove from the list").clicked() {
                    remove = Some(i);
                }
                ui.label(p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()).on_hover_text(p.display().to_string());
                match self.bake.info.get(p) {
                    Some((_, Ok(s))) => ui.small(s),
                    Some((_, Err(e))) => ui.colored_label(Color32::from_rgb(255, 130, 120), e),
                    None => ui.small("reading..."),
                };
            });
        }
        if let Some(i) = remove {
            if high {
                self.bake.high.remove(i);
            } else {
                self.bake.low.remove(i);
            }
        }
        if ui.button(if high { "Add high-poly meshes..." } else { "Add low-poly meshes..." }).clicked() {
            if let Some(ps) = rfd::FileDialog::new().add_filter("Meshes (OBJ, glTF, GLB, PLY, STL)", &["obj", "gltf", "glb", "ply", "stl"]).pick_files() {
                for p in ps {
                    let l = if high { &mut self.bake.high } else { &mut self.bake.low };
                    if !l.contains(&p) {
                        l.push(p);
                    }
                }
            }
        }
    }

    fn bake_tab_meshes(&mut self, ui: &mut egui::Ui) {
        ui.strong("Low-poly meshes (the ones that get the textures)");
        ui.small("They need UV coordinates inside the 0..1 square. Several files are baked together into one set of maps.");
        self.bake_mesh_list(ui, false);
        ui.add_space(8.0);
        ui.strong("High-poly meshes (the detail to transfer)");
        ui.small("Leave empty to bake the low-poly mesh against itself (ambient occlusion, curvature, thickness, IDs).");
        self.bake_mesh_list(ui, true);
        ui.add_space(8.0);
        ui.strong("Cage (optional)");
        ui.small("A copy of the low-poly mesh, pushed outwards, with the same faces. Rays start on it. Without one the baker pushes the low-poly surface out by the front distance.");
        ui.horizontal(|ui| {
            if let Some(c) = self.bake.cage.clone() {
                if ui.small_button("x").clicked() {
                    self.bake.cage = None;
                }
                ui.label(c.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
                match self.bake.info.get(&c) {
                    Some((_, Ok(s))) => ui.small(s),
                    Some((_, Err(e))) => ui.colored_label(Color32::from_rgb(255, 130, 120), e),
                    None => ui.small("reading..."),
                };
            } else if ui.button("Choose cage mesh...").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("Meshes", &["obj", "gltf", "glb", "ply", "stl"]).pick_file() {
                    self.bake.cage = Some(p);
                }
            }
        });
        ui.add_space(8.0);
        if ui.button("Use demo sphere meshes").clicked() {
            let d = std::env::temp_dir().join("materialcraft_demo");
            let _ = std::fs::create_dir_all(&d);
            let (lo, hi) = (d.join("demo_low.obj"), d.join("demo_high.obj"));
            if std::fs::write(&lo, bake::uv_sphere_obj(24, 16, 1.0, 0.0)).is_ok() && std::fs::write(&hi, bake::uv_sphere_obj(160, 100, 1.0, 0.04)).is_ok() {
                self.bake.low = vec![lo];
                self.bake.high = vec![hi];
                self.bake.cage = None;
                self.bake.name = "demo".into();
            }
        }
    }

    fn bake_tab_settings(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.bake.s;
        ui.strong("Image");
        ui.horizontal(|ui| {
            let sizes = [64usize, 128, 256, 512, 1024, 2048, 4096, 8192];
            egui::ComboBox::from_id_salt("bake_w").selected_text(format!("{} px wide", s.width)).show_ui(ui, |ui| {
                for v in sizes {
                    ui.selectable_value(&mut s.width, v, v.to_string());
                }
            });
            egui::ComboBox::from_id_salt("bake_h").selected_text(format!("{} px high", s.height)).show_ui(ui, |ui| {
                for v in sizes {
                    ui.selectable_value(&mut s.height, v, v.to_string());
                }
            });
            egui::ComboBox::from_id_salt("bake_aa").selected_text(format!("Anti-aliasing {}x{}", s.aa.max(1), s.aa.max(1))).show_ui(ui, |ui| {
                for v in [1u32, 2, 4, 8] {
                    ui.selectable_value(&mut s.aa, v, format!("{v}x{v}"));
                }
            });
        });
        ui.add(egui::Slider::new(&mut s.padding, 0..=64).text("Padding (pixels around islands)"));
        ui.add_space(6.0);
        ui.strong("Rays");
        ui.horizontal(|ui| {
            ui.label("Trace rays on:");
            ui.selectable_value(&mut s.device, bake::Device::Auto, "Auto (GPU when available)");
            ui.selectable_value(&mut s.device, bake::Device::Gpu, "GPU");
            ui.selectable_value(&mut s.device, bake::Device::Cpu, "CPU");
        });
        match bake::gpu_name() {
            Some(n) => ui.small(format!("GPU: {n}")),
            None => ui.small("No usable GPU found: rays are traced on the CPU."),
        };
        ui.add(egui::Slider::new(&mut s.front, 0.0..=1.0).logarithmic(true).text("Front distance (x model size)")).on_hover_text("How far above the low-poly surface rays start.");
        ui.add(egui::Slider::new(&mut s.rear, 0.0..=1.0).logarithmic(true).text("Rear distance (x model size)")).on_hover_text("How far below the low-poly surface rays may still hit.");
        ui.add(egui::Slider::new(&mut s.bias, 0.0..=0.05).logarithmic(true).text("Ray bias (x model size)"));
        ui.horizontal(|ui| {
            ui.label("Cage normals:");
            ui.selectable_value(&mut s.cage_normals, bake::CageNormals::Averaged, "Averaged (no gaps)");
            ui.selectable_value(&mut s.cage_normals, bake::CageNormals::Shading, "Shading normals");
        });
        ui.checkbox(&mut s.ignore_backfaces, "Ignore back faces of the high-poly mesh");
        ui.add_space(6.0);
        ui.strong("Matching");
        ui.checkbox(&mut s.match_by_name, "Match low and high objects by name").on_hover_text("An object called rock_low only sees rock_high. Objects without a partner see everything.");
        if s.match_by_name {
            let mut lo = s.low_suffixes.join(", ");
            let mut hi = s.high_suffixes.join(", ");
            ui.horizontal(|ui| {
                ui.label("Low suffixes");
                if ui.add(egui::TextEdit::singleline(&mut lo).desired_width(260.0)).changed() {
                    s.low_suffixes = lo.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
                }
            });
            ui.horizontal(|ui| {
                ui.label("High suffixes");
                if ui.add(egui::TextEdit::singleline(&mut hi).desired_width(260.0)).changed() {
                    s.high_suffixes = hi.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
                }
            });
        }
        ui.add_space(6.0);
        ui.strong("Normals and tangent space");
        normal_mode_combo(ui, "Low-poly normals", &mut s.low_normals);
        normal_mode_combo(ui, "High-poly normals", &mut s.high_normals);
        ui.horizontal(|ui| {
            ui.label("Tangent basis:");
            ui.selectable_value(&mut s.basis, bake::Basis::Smooth, "Per vertex (smooth, like most engines)");
            ui.selectable_value(&mut s.basis, bake::Basis::Face, "Per face");
        });
        ui.horizontal(|ui| {
            ui.label("Normal map style:");
            ui.selectable_value(&mut s.flip_y, false, "OpenGL (Y+)");
            ui.selectable_value(&mut s.flip_y, true, "DirectX (Y-)");
        });
        ui.add_space(6.0);
        ui.strong("UV set and materials");
        ui.horizontal(|ui| {
            ui.label("UV set:");
            ui.selectable_value(&mut s.uv_set, 0, "First");
            ui.selectable_value(&mut s.uv_set, 1, "Second");
            ui.label("Only material:");
            ui.add(egui::TextEdit::singleline(&mut s.material).desired_width(140.0).hint_text("(all)"));
        });
        ui.add_space(6.0);
        ui.strong("Map options");
        ui.add(egui::Slider::new(&mut s.ao_rays, 4..=1024).logarithmic(true).text("Ambient occlusion rays"));
        ui.add(egui::Slider::new(&mut s.ao_dist, 0.005..=2.0).logarithmic(true).text("Ambient occlusion distance (x model size)"));
        ui.add(egui::Slider::new(&mut s.ao_falloff, 0.0..=1.0).text("Ambient occlusion falloff"));
        ui.add(egui::Slider::new(&mut s.thick_rays, 4..=512).logarithmic(true).text("Thickness rays"));
        ui.add(egui::Slider::new(&mut s.thick_dist, 0.005..=4.0).logarithmic(true).text("Thickness distance (x model size)"));
        ui.add(egui::Slider::new(&mut s.curv_radius, 0.001..=1.0).logarithmic(true).text("Curvature radius (x model size)"));
        ui.add(egui::Slider::new(&mut s.curv_smooth, 0..=8).text("Curvature smoothing"));
        ui.horizontal(|ui| {
            ui.label("World direction map looks along:");
            for (i, n) in ["x", "y", "z"].iter().enumerate() {
                ui.add(egui::DragValue::new(&mut s.direction[i]).speed(0.02).prefix(*n));
            }
        });
        let mut custom = s.pos_range.is_some();
        if ui.checkbox(&mut custom, "Position map range from numbers (otherwise the high-poly bounding box)").changed() {
            s.pos_range = if custom { Some(([-1.0; 3], [1.0; 3])) } else { None };
        }
        if let Some((a, b)) = &mut s.pos_range {
            ui.horizontal(|ui| {
                ui.label("min");
                for v in a.iter_mut() {
                    ui.add(egui::DragValue::new(v).speed(0.02));
                }
                ui.label("max");
                for v in b.iter_mut() {
                    ui.add(egui::DragValue::new(v).speed(0.02));
                }
            });
        }
    }

    fn bake_tab_maps(&mut self, ui: &mut egui::Ui) {
        ui.small("Tick the maps to bake. Each becomes a Baked Map node that follows the graph size.");
        let mut chosen: HashSet<bake::MapKind> = self.bake.s.maps.iter().copied().collect();
        egui::Grid::new("bake_maps").num_columns(2).spacing([24.0, 4.0]).show(ui, |ui| {
            for (i, m) in bake::MapKind::ALL.iter().enumerate() {
                let mut on = chosen.contains(m);
                if ui.checkbox(&mut on, m.label()).changed() {
                    if on {
                        chosen.insert(*m);
                    } else {
                        chosen.remove(m);
                    }
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        self.bake.s.maps = bake::MapKind::ALL.iter().copied().filter(|m| chosen.contains(m)).collect();
        ui.horizontal(|ui| {
            if ui.small_button("All").clicked() {
                self.bake.s.maps = bake::MapKind::ALL.to_vec();
            }
            if ui.small_button("Common").clicked() {
                self.bake.s.maps = BakeSettings::default().maps;
            }
            if ui.small_button("None").clicked() {
                self.bake.s.maps.clear();
            }
        });
    }

    fn bake_tab_output(&mut self, ui: &mut egui::Ui) {
        ui.label("The maps are written as 16-bit PNG files to this folder; link them into the package as bitmaps to use them in a graph.");
        ui.horizontal(|ui| {
            if ui.button("Output folder...").clicked() {
                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                    self.bake.out = Some(d);
                }
            }
            ui.label(self.bake.out.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(none)".into()));
        });
    }
}
