// SPDX-License-Identifier: MIT OR Apache-2.0
// The 3D view's menus and background jobs (included into gui.rs): light rig, environment, camera and turntable, post
// processing, channel isolation, custom meshes, the CPU path tracer and image / turntable export.
use materialcraft::hdri;
use materialcraft::objects::CustomMesh;
use materialcraft::scene::{self, Background, Channel, LightKind, SceneKind, ToneMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// What the path tracer thread hands to the UI.
struct PtShared {
    img: Option<Tex>,
    samples: u32,
    seq: u64,
    done: bool,
}

struct PtJob {
    cancel: Arc<AtomicBool>,
    shared: Arc<Mutex<PtShared>>,
    key: u64,
    seen: u64,
}

struct V3dUi {
    sel_light: usize,
    turntable: bool,
    tt_speed: f32,
    tt_last: Option<std::time::Instant>,
    /// show the CPU path tracer instead of the GPU / raster view
    pt_on: bool,
    pt_target: u32,
    pt: Option<PtJob>,
    pt_samples: u32,
    mesh_name: Option<String>,
    mesh_msg: Option<String>,
    export_rx: Option<Receiver<Result<String, String>>>,
    export_size: usize,
    export_spp: u32,
    export_frames: u32,
}

impl Default for V3dUi {
    fn default() -> Self {
        V3dUi { sel_light: 0, turntable: false, tt_speed: 0.6, tt_last: None, pt_on: false, pt_target: 256, pt: None, pt_samples: 0, mesh_name: None, mesh_msg: None, export_rx: None, export_size: 1024, export_spp: 128, export_frames: 36 }
    }
}

/// Stand-ins for the maps a graph does not produce.
struct Fallbacks {
    flat: Tex,
    half: Tex,
    zero: Tex,
    one: Tex,
    inv_gloss: Option<Tex>,
}

fn fallbacks(maps: &BTreeMap<String, Arc<Tex>>) -> Fallbacks {
    Fallbacks {
        flat: Tex::solid(4, 4, [0.5, 0.5, 1.0, 1.0]),
        half: Tex::solid(4, 4, [0.5, 0.5, 0.5, 1.0]),
        zero: Tex::solid(4, 4, [0.0, 0.0, 0.0, 1.0]),
        one: Tex::solid(4, 4, [1.0; 4]),
        inv_gloss: maps.get("glossiness").map(|g| ops::invert(g)),
    }
}

fn build_maps<'a>(maps: &'a BTreeMap<String, Arc<Tex>>, f: &'a Fallbacks) -> Maps<'a> {
    let get = |k: &str| maps.get(k).map(|a| a.as_ref());
    Maps {
        base: get("basecolor").or_else(|| get("diffuse")).unwrap_or(&f.half),
        normal: get("normal").unwrap_or(&f.flat),
        rough: get("roughness").or(f.inv_gloss.as_ref()).unwrap_or(&f.half),
        metal: get("metallic").unwrap_or(&f.zero),
        ao: get("ambient_occlusion").unwrap_or(&f.one),
        emissive: get("emissive"),
        height: get("height"),
        opacity: get("opacity"),
        env: None,
    }
}

/// The output names of a node that the 3D view can read as material maps (base colour, normal, roughness ...).
fn material_outs(outs: &[String]) -> usize {
    outs.iter().filter(|o| materialcraft::sbs_import::usage(o) != "out").count()
}

/// Pass-through nodes, one per material output of node `nid`, added to `g` (the copy that is evaluated): while the 3D
/// view shows that node it reads them in place of the graph outputs. Returns (usage, node) for each.
fn view3d_taps(g: &mut Graph, nid: u32) -> Vec<(String, u32)> {
    let Some(n) = g.nodes.get(&nid) else { return Vec::new() };
    let mut taps = Vec::new();
    for o in native::outputs(n, &g.subgraphs) {
        let usage = materialcraft::sbs_import::usage(&o);
        if usage == "out" || taps.iter().any(|(u, _)| u == usage) {
            continue;
        }
        let mut t = Node { kind: "sbs:passthrough".into(), ..Default::default() };
        t.ins.insert("input".into(), nid);
        t.in_out.insert("input".into(), o);
        let id = native::free_id(g);
        g.nodes.insert(id, t);
        taps.push((usage.to_string(), id));
    }
    taps
}

fn hash_of<T: std::hash::Hash>(t: &T) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

impl App {
    /// Show node `id`'s outputs in the 3D view, or (when it already shows them) go back to the graph outputs.
    fn toggle_view3d_node(&mut self, id: u32) {
        self.view3d_node = if self.view3d_node == Some(id) { None } else { Some(id) };
        // the next pass brings the 3D view its new maps, a thumbnail pass included
        self.maps_size = 0;
        self.dirty = true;
    }

    /// In the 3D view's menu bar: which node it shows when not the graph outputs, and the way back.
    fn view3d_source(&mut self, ui: &mut egui::Ui, ch: &mut bool) {
        let Some(id) = self.view3d_node else { return };
        let Some(n) = self.g.nodes.get(&id) else {
            self.view3d_node = None;
            return;
        };
        let name = n.strs.get("title").filter(|t| !t.is_empty()).cloned().unwrap_or_else(|| native::label(n, &self.g));
        ui.separator();
        ui.label(egui::RichText::new(format!("3D: {name}")).color(Color32::from_rgb(110, 225, 200))).on_hover_text("The 3D view shows this node's outputs, not the graph outputs");
        if ui.small_button("x").on_hover_text("Back to the graph outputs").clicked() {
            self.toggle_view3d_node(id);
            *ch = true;
        }
    }

    // ------------------------------------------------------------ menus
    /// Everything the 3D view's menu bar offers beyond the shape and material menus.
    fn view3d_menus(&mut self, ui: &mut egui::Ui, ch: &mut bool) {
        ui.menu_button("Lights", |ui| self.lights_menu(ui, ch));
        ui.menu_button("Camera", |ui| self.camera_menu(ui, ch));
        ui.menu_button("Environment", |ui| self.env_menu(ui, ch));
        ui.menu_button("Post", |ui| self.post_menu(ui, ch));
        ui.menu_button("Channel", |ui| {
            ui.label("Show one channel of the material, unlit");
            for c in Channel::ALL {
                if ui.selectable_label(self.view3d.channel == c, c.name()).clicked() {
                    self.view3d.channel = c;
                    *ch = true;
                    ui.close();
                }
            }
        });
    }

    /// Scene kind, mesh library and custom mesh (inside the Scene menu).
    fn scene_extras(&mut self, ui: &mut egui::Ui, ch: &mut bool) {
        ui.label("Setting");
        for k in SceneKind::ALL {
            if ui.selectable_label(self.view3d.scene == k, k.name()).clicked() {
                self.view3d.scene = k;
                *ch = true;
            }
        }
        ui.separator();
        if ui.button("Load a mesh from a file...").clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("Meshes (OBJ, glTF, GLB, PLY, STL)", materialcraft::meshio::FORMATS).pick_file() {
                match CustomMesh::load(&p) {
                    Ok(m) => {
                        self.v3.mesh_name = Some(p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
                        self.v3.mesh_msg = if m.generated_uv { Some("The mesh has no UVs; a box projection is used.".into()) } else { None };
                        self.view3d.custom = Some(Arc::new(m));
                        self.view3d.shape = render3d::SHAPE_CUSTOM;
                        *ch = true;
                    }
                    Err(e) => self.v3.mesh_msg = Some(format!("Could not load the mesh: {e}")),
                }
            }
        }
        if let Some(n) = &self.v3.mesh_name {
            ui.small(format!("Loaded: {n}"));
        }
        if let Some(m) = &self.v3.mesh_msg {
            ui.colored_label(Color32::from_rgb(255, 170, 120), m);
        }
    }

    fn lights_menu(&mut self, ui: &mut egui::Ui, ch: &mut bool) {
        ui.label("Light rig");
        ui.horizontal_wrapped(|ui| {
            for (id, name) in scene::RIGS {
                if ui.button(*name).clicked() {
                    self.view3d.lights = scene::rig(id);
                    self.v3.sel_light = 0;
                    *ch = true;
                }
            }
        });
        ui.separator();
        let n = self.view3d.lights.len();
        self.v3.sel_light = self.v3.sel_light.min(n.saturating_sub(1));
        let mut remove = None;
        for i in 0..n {
            ui.horizontal(|ui| {
                let l = &mut self.view3d.lights[i];
                *ch |= ui.checkbox(&mut l.enabled, "").changed();
                if ui.selectable_label(self.v3.sel_light == i, format!("{}  ({})", l.name, l.kind.name())).clicked() {
                    self.v3.sel_light = i;
                }
                if ui.small_button("remove").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.view3d.lights.remove(i);
            *ch = true;
        }
        ui.horizontal(|ui| {
            if n < scene::MAX_LIGHTS {
                for k in LightKind::ALL {
                    if ui.small_button(format!("+ {}", k.name())).clicked() {
                        let mut l = scene::Light::new_of(k);
                        l.name = format!("{} {}", k.name(), n + 1);
                        self.view3d.lights.push(l);
                        self.v3.sel_light = n;
                        *ch = true;
                    }
                }
            } else {
                ui.small(format!("at most {} lights", scene::MAX_LIGHTS));
            }
        });
        if let Some(l) = self.view3d.lights.get_mut(self.v3.sel_light) {
            ui.separator();
            ui.label(format!("{} (Shift + drag in the view moves the selected light)", l.name));
            egui::ComboBox::from_id_salt("light_kind").selected_text(l.kind.name()).show_ui(ui, |ui| {
                for k in LightKind::ALL {
                    *ch |= ui.selectable_value(&mut l.kind, k, k.name()).changed();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Colour");
                *ch |= ui.color_edit_button_rgb(&mut l.color).changed();
            });
            let unit = if l.kind == LightKind::Directional { "Intensity (irradiance)" } else { "Intensity (candela)" };
            *ch |= ui.add(egui::Slider::new(&mut l.intensity, 0.0..=60.0).text(unit).logarithmic(true).clamping(egui::SliderClamping::Never)).changed();
            *ch |= ui.add(egui::Slider::new(&mut l.yaw, -3.1416..=3.1416).text("Azimuth")).changed();
            *ch |= ui.add(egui::Slider::new(&mut l.pitch, -1.5..=1.5).text("Elevation")).changed();
            if l.kind != LightKind::Directional {
                *ch |= ui.add(egui::Slider::new(&mut l.distance, 0.5..=12.0).text("Distance")).changed();
            }
            if l.kind == LightKind::Spot {
                *ch |= ui.add(egui::Slider::new(&mut l.cone, 0.1..=1.4).text("Cone angle")).changed();
            }
            *ch |= ui.add(egui::Slider::new(&mut l.softness, 0.0..=1.0).text("Shadow softness")).changed();
            *ch |= ui.checkbox(&mut l.follows_camera, "Follows the camera").changed();
            *ch |= ui.checkbox(&mut l.shadows, "Casts shadows").changed();
            if self.gpu_ready() {
                ui.small("The GPU view draws shadows for up to 3 lights; the CPU views for all of them.");
            }
        }
    }

    fn camera_menu(&mut self, ui: &mut egui::Ui, ch: &mut bool) {
        ui.label("Camera position");
        ui.horizontal_wrapped(|ui| {
            for (_, name, yaw, pitch, dist) in scene::CAMERAS {
                if ui.button(*name).clicked() {
                    self.view3d.yaw = *yaw;
                    self.view3d.pitch = *pitch;
                    self.view3d.dist = *dist;
                    self.view3d.pan_x = 0.0;
                    self.view3d.pan_y = 0.0;
                    *ch = true;
                }
            }
        });
        *ch |= ui.add(egui::Slider::new(&mut self.view3d.dist, 1.6..=9.0).text("Distance")).changed();
        *ch |= ui.add(egui::Slider::new(&mut self.view3d.fov, 15.0..=90.0).text("Field of view (degrees)")).changed();
        ui.separator();
        if ui.checkbox(&mut self.v3.turntable, "Turntable (orbit automatically)").changed() {
            self.v3.tt_last = None;
        }
        ui.add(egui::Slider::new(&mut self.v3.tt_speed, -3.0..=3.0).text("Turntable speed (radians / second)"));
        ui.separator();
        if ui.button("Reset camera").clicked() {
            let d = View::default();
            self.view3d.yaw = d.yaw;
            self.view3d.pitch = d.pitch;
            self.view3d.dist = d.dist;
            self.view3d.fov = d.fov;
            self.view3d.pan_x = d.pan_x;
            self.view3d.pan_y = d.pan_y;
            *ch = true;
            ui.close();
        }
    }

    fn env_menu(&mut self, ui: &mut egui::Ui, ch: &mut bool) {
        ui.label("Lighting environment");
        ui.horizontal_wrapped(|ui| {
            for (id, name) in hdri::PRESETS {
                let on = self.view3d.env.file.is_none() && self.view3d.env.preset == *id;
                if ui.selectable_label(on, *name).clicked() {
                    self.view3d.env.preset = id.to_string();
                    self.view3d.env.file = None;
                    *ch = true;
                }
            }
        });
        if ui.button("Load an HDRI image (.hdr, .exr, .png)...").clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("Environment images", materialcraft::imgio::EXTENSIONS).pick_file() {
                self.view3d.env.file = Some(p.to_string_lossy().to_string());
                *ch = true;
            }
        }
        if let Some(f) = &self.view3d.env.file {
            ui.small(format!("Using: {}", std::path::Path::new(f).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
        }
        if let Some(e) = self.gpu.lock().ok().and_then(|g| g.env_error.clone()) {
            ui.colored_label(Color32::from_rgb(255, 170, 120), format!("Could not use that image: {e}"));
        }
        *ch |= ui.add(egui::Slider::new(&mut self.view3d.env.rotation, -3.1416..=3.1416).text("Rotation")).changed();
        *ch |= ui.add(egui::Slider::new(&mut self.view3d.env.exposure, -4.0..=4.0).text("Environment brightness (stops)")).changed();
        *ch |= ui.add(egui::Slider::new(&mut self.view3d.ambient, 0.0..=3.0).text("Environment light strength")).changed();
        *ch |= ui.add(egui::Slider::new(&mut self.view3d.exposure, 0.1..=4.0).text("Exposure")).changed();
        ui.separator();
        ui.label("Background");
        for b in Background::ALL {
            if ui.selectable_label(self.view3d.env.background == b, b.name()).clicked() {
                self.view3d.env.background = b;
                *ch = true;
            }
        }
        match self.view3d.env.background {
            Background::Environment => *ch |= ui.add(egui::Slider::new(&mut self.view3d.env.blur, 0.0..=1.0).text("Blur")).changed(),
            Background::Color => {
                ui.horizontal(|ui| {
                    ui.label("Colour");
                    *ch |= ui.color_edit_button_rgb(&mut self.view3d.env.bg_color).changed();
                });
            }
            Background::Gradient => {}
        }
    }

    fn post_menu(&mut self, ui: &mut egui::Ui, ch: &mut bool) {
        ui.label("Looks");
        ui.horizontal_wrapped(|ui| {
            for (id, name) in scene::POST_PRESETS {
                if ui.button(*name).clicked() {
                    self.view3d.post = scene::post_preset(id);
                    *ch = true;
                }
            }
        });
        ui.separator();
        let p = &mut self.view3d.post;
        egui::ComboBox::from_id_salt("tonemap").selected_text(p.tonemap.name()).show_ui(ui, |ui| {
            for t in ToneMap::ALL {
                *ch |= ui.selectable_value(&mut p.tonemap, t, t.name()).changed();
            }
        });
        *ch |= ui.add(egui::Slider::new(&mut p.bloom, 0.0..=2.0).text("Bloom")).changed();
        if p.bloom > 0.0 {
            *ch |= ui.add(egui::Slider::new(&mut p.bloom_threshold, 0.1..=4.0).text("Bloom threshold")).changed();
            *ch |= ui.add(egui::Slider::new(&mut p.bloom_radius, 0.0..=1.0).text("Bloom radius")).changed();
        }
        *ch |= ui.add(egui::Slider::new(&mut p.ssao, 0.0..=1.0).text("Ambient occlusion")).changed();
        if p.ssao > 0.0 {
            *ch |= ui.add(egui::Slider::new(&mut p.ssao_radius, 0.05..=1.0).text("Occlusion radius")).changed();
        }
        *ch |= ui.add(egui::Slider::new(&mut p.dof, 0.0..=4.0).text("Depth of field")).changed();
        if p.dof > 0.0 {
            *ch |= ui.add(egui::Slider::new(&mut p.focus, 0.0..=12.0).text("Focus distance (0 = middle of the scene)")).changed();
        }
        *ch |= ui.add(egui::Slider::new(&mut p.vignette, 0.0..=1.0).text("Vignette")).changed();
        ui.separator();
        ui.label("Colour grading");
        *ch |= ui.add(egui::Slider::new(&mut p.contrast, 0.5..=1.8).text("Contrast")).changed();
        *ch |= ui.add(egui::Slider::new(&mut p.saturation, 0.0..=2.0).text("Saturation")).changed();
        *ch |= ui.add(egui::Slider::new(&mut p.temperature, -1.0..=1.0).text("Temperature")).changed();
        *ch |= ui.add(egui::Slider::new(&mut p.tint, -1.0..=1.0).text("Tint")).changed();
        if ui.button("Reset post-processing").clicked() {
            *p = scene::Post::default();
            *ch = true;
        }
    }

    /// Renderer choices for the path tracer and the export of pictures (inside the Renderer menu).
    fn render_extras(&mut self, ui: &mut egui::Ui, ch: &mut bool) {
        ui.separator();
        if ui.checkbox(&mut self.v3.pt_on, "Path tracer (CPU, progressive)").changed() {
            *ch = true;
            self.v3.pt = None;
        }
        if self.v3.pt_on {
            ui.add(egui::Slider::new(&mut self.v3.pt_target, 16..=4096).text("Samples per pixel").logarithmic(true));
            ui.small("Real global illumination: soft shadows, colour bleeding and mirror reflections of the other objects. It restarts whenever something changes.");
        }
        ui.separator();
        ui.label("Save a picture");
        ui.horizontal(|ui| {
            ui.label("Size");
            for s in [512usize, 1024, 2048] {
                ui.selectable_value(&mut self.v3.export_size, s, format!("{s}"));
            }
        });
        ui.add(egui::Slider::new(&mut self.v3.export_spp, 8..=2048).text("Path tracer samples").logarithmic(true));
        let busy = self.v3.export_rx.is_some();
        ui.add_enabled_ui(!busy, |ui| {
            if ui.button("Save picture (raster)...").clicked() {
                self.export_picture(false, 0);
                ui.close();
            }
            if ui.button("Save picture (path traced)...").clicked() {
                self.export_picture(true, 0);
                ui.close();
            }
            ui.add(egui::Slider::new(&mut self.v3.export_frames, 4..=360).text("Turntable frames"));
            if ui.button("Save turntable frames (raster)...").clicked() {
                self.export_picture(false, self.v3.export_frames);
                ui.close();
            }
        });
        if busy {
            ui.small("Rendering...");
        }
    }

    // ------------------------------------------------------------ background work
    fn export_picture(&mut self, pt: bool, frames: u32) {
        let target = if frames > 0 {
            rfd::FileDialog::new().pick_folder()
        } else {
            rfd::FileDialog::new().add_filter("PNG image", &["png"]).set_file_name("view.png").save_file()
        };
        let Some(target) = target else { return };
        self.ensure_maps();
        let maps = self.maps.clone();
        let mut view = self.view3d.clone();
        view.tess = view.tess.max(256);
        let (size, spp) = (self.v3.export_size, self.v3.export_spp);
        let (tx, rx) = channel();
        self.v3.export_rx = Some(rx);
        std::thread::spawn(move || {
            let res = (|| -> Result<String, String> {
                let fb = fallbacks(&maps);
                let m = build_maps(&maps, &fb);
                let n = frames.max(1);
                for f in 0..n {
                    let mut v = view.clone();
                    if frames > 0 {
                        v.yaw += std::f32::consts::TAU * f as f32 / n as f32;
                    }
                    let img = if pt { materialcraft::pt::render(&m, &v, size, size, spp) } else { render3d::render(&m, &v, size, size) };
                    let path = if frames > 0 { target.join(format!("frame_{f:04}.png")) } else { target.clone() };
                    materialcraft::safeio::write_file(&path, &png::encode(img.w, img.h, &img.px, 8)).map_err(|e| format!("{}: {e}", path.display()))?;
                }
                Ok(if frames > 0 { format!("Saved {n} frames to {}", target.display()) } else { format!("Saved {}", target.display()) })
            })();
            let _ = tx.send(res);
        });
    }

    /// Called every frame the 3D panel is shown: turntable, path tracer and export results.
    fn view3d_tick(&mut self, ctx: &egui::Context) {
        if self.v3.turntable {
            let now = std::time::Instant::now();
            let dt = self.v3.tt_last.map(|t| now.duration_since(t).as_secs_f32()).unwrap_or(0.0).min(0.1);
            self.v3.tt_last = Some(now);
            self.view3d.yaw += self.v3.tt_speed * dt;
            self.dirty3d = true;
            ctx.request_repaint();
        }
        if let Some(rx) = &self.v3.export_rx {
            match rx.try_recv() {
                Ok(r) => {
                    self.v3.export_rx = None;
                    match r {
                        Ok(m) => self.status(m, true),
                        Err(e) => self.status(format!("Could not save the picture: {e}"), false),
                    }
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(300)),
                Err(TryRecvError::Disconnected) => self.v3.export_rx = None,
            }
        }
        if self.v3.pt_on {
            self.pt_tick(ctx);
        } else if let Some(j) = self.v3.pt.take() {
            j.cancel.store(true, Ordering::Relaxed);
        }
    }

    fn pt_tick(&mut self, ctx: &egui::Context) {
        self.ensure_maps();
        let mut view = self.view3d.clone();
        view.tess = view.tess.min(256);
        let size = self.r3d_size;
        let mut key_src = (view.to_text(), size, self.v3.pt_target);
        for (k, t) in &self.maps {
            key_src.0.push_str(&format!("{k}{:p}", Arc::as_ptr(t)));
        }
        key_src.0.push_str(&format!("{:p}", view.custom.as_ref().map(Arc::as_ptr).unwrap_or(std::ptr::null())));
        let key = hash_of(&key_src);
        if self.v3.pt.as_ref().map(|j| j.key != key).unwrap_or(true) {
            if let Some(j) = self.v3.pt.take() {
                j.cancel.store(true, Ordering::Relaxed);
            }
            let cancel = Arc::new(AtomicBool::new(false));
            let shared = Arc::new(Mutex::new(PtShared { img: None, samples: 0, seq: 0, done: false }));
            let (c2, s2, maps, target) = (cancel.clone(), shared.clone(), self.maps.clone(), self.v3.pt_target);
            std::thread::spawn(move || {
                let fb = fallbacks(&maps);
                let m = build_maps(&maps, &fb);
                let pt = materialcraft::pt::PathTracer::new(&m, &view, size, size);
                let mut st = materialcraft::pt::PtState::new(size, size);
                let mut last_pub = std::time::Instant::now();
                let mut step = 1u32;
                while !c2.load(Ordering::Relaxed) && st.samples < target {
                    let n = step.min(target - st.samples);
                    pt.render_pass(&mut st, n);
                    if st.samples >= target || last_pub.elapsed().as_millis() > 400 || st.samples <= 2 {
                        let img = pt.resolve(&st);
                        if let Ok(mut g) = s2.lock() {
                            g.img = Some(img);
                            g.samples = st.samples;
                            g.seq += 1;
                        }
                        last_pub = std::time::Instant::now();
                    }
                    step = (step * 2).min(8);
                }
                if let Ok(mut g) = s2.lock() {
                    g.done = true;
                }
            });
            self.v3.pt = Some(PtJob { cancel, shared, key, seen: 0 });
            self.v3.pt_samples = 0;
        }
        let mut running = true;
        let mut fresh = None;
        if let Some(j) = &mut self.v3.pt {
            if let Ok(g) = j.shared.lock() {
                running = !g.done;
                if g.seq != j.seen {
                    j.seen = g.seq;
                    fresh = g.img.clone().map(|i| (i, g.samples));
                }
            }
        }
        if let Some((img, n)) = fresh {
            self.v3.pt_samples = n;
            self.tex3d = Some(ctx.load_texture("view3d", qimg(&img), egui::TextureOptions::LINEAR));
        }
        if running {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
    }
}
