// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! MaterialCraft visual editor. Run: cargo run --release --features gui --bin materialcraft-gui
use eframe::egui::{
    self, epaint::CubicBezierShape, Align2, Color32, ColorImage, FontId, Id, Key, Modifiers, PointerButton, Pos2, Rect,
    Sense, Stroke, TextStyle, Vec2, pos2, vec2,
};
use materialcraft::bake::{self, BakeSettings};
use materialcraft::graph::{usage_label, Comment, Graph, Node, NodeCache, Pending, THUMB};
use materialcraft::render3d::{self, Maps, View};
use materialcraft::tex::{self, Tex};
use materialcraft::export::{self, Format};
use materialcraft::gpu3d::{self, DevInputs, MapInputs};
use materialcraft::{ops, png};
use materialcraft::user_templates::{self, UserTemplate};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

mod logo_data {
    include!(concat!(env!("OUT_DIR"), "/logo.rs"));
}

// ---------------------------------------------------------------- layout constants
const CARD: f32 = 92.0;
const BAR: f32 = 16.0;
const CARD_H: f32 = BAR + CARD;

/// Height of a node card's body: the square thumbnail, or taller when many input sockets (or outputs) need room.
fn card_body(n_slots: usize, n_outs: usize) -> f32 {
    CARD.max(15.0 * (n_slots as f32 + 1.0)).max(if n_outs >= 2 { 20.0 * n_outs as f32 } else { 0.0 })
}
/// Persistent user settings, stored as `config.toml` by `craft-settings` (atomic saves, backup, damaged-file recovery).
#[derive(serde::Serialize, serde::Deserialize, Default, Clone, PartialEq)]
#[serde(default)]
struct Prefs {
    general: PrefsGeneral,
    library: PrefsLibrary,
    recent: PrefsRecent,
    layout: PrefsLayout,
}
#[derive(serde::Serialize, serde::Deserialize, Default, Clone, PartialEq)]
#[serde(default)]
struct PrefsLayout {
    dock: Option<craft_ui::docking::Layout<Pn>>,
}

/// The panels of the editor window. `Graph` is the canvas and cannot be closed, moved or floated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
enum Pn {
    Explorer,
    Library,
    Graph,
    View3d,
    View2d,
    Props,
}

impl Pn {
    const ALL: [Pn; 6] = [Pn::Explorer, Pn::Library, Pn::Graph, Pn::View3d, Pn::View2d, Pn::Props];
    fn label(self) -> &'static str {
        match self {
            Pn::Explorer => "Explorer",
            Pn::Library => "Library",
            Pn::Graph => "Graph",
            Pn::View3d => "3D View",
            Pn::View2d => "2D View",
            Pn::Props => "Properties",
        }
    }
    /// Index into `App::show` (the canvas has none: it is always visible).
    fn show_index(self) -> Option<usize> {
        match self {
            Pn::Explorer => Some(0),
            Pn::Library => Some(1),
            Pn::View3d => Some(2),
            Pn::View2d => Some(3),
            Pn::Props => Some(4),
            Pn::Graph => None,
        }
    }
}

fn dock_tabs(p: Pn) -> craft_ui::docking::Node<Pn> {
    craft_ui::docking::Node::Tabs { panels: vec![p], active: 0 }
}

fn dock_split(axis: craft_ui::layout::SplitAxis, size: craft_ui::layout::SplitSize, a: craft_ui::docking::Node<Pn>, b: craft_ui::docking::Node<Pn>) -> craft_ui::docking::Node<Pn> {
    craft_ui::docking::Node::Split { axis, size, first: Box::new(a), second: Box::new(b) }
}

/// Explorer and Library on the left, the graph in the middle with the 3D and 2D views under it, Properties on the right.
fn default_dock() -> craft_ui::docking::Layout<Pn> {
    use craft_ui::layout::{SplitAxis::*, SplitSize::*};
    let views = dock_split(Horizontal, Ratio(0.5), dock_tabs(Pn::View3d), dock_tabs(Pn::View2d));
    let centre = dock_split(Vertical, FixedSecond(330.0), dock_tabs(Pn::Graph), views);
    let left = dock_split(Vertical, Ratio(0.33), dock_tabs(Pn::Explorer), dock_tabs(Pn::Library));
    let right = dock_split(Horizontal, FixedSecond(330.0), centre, dock_tabs(Pn::Props));
    craft_ui::docking::Layout { root: Some(dock_split(Horizontal, FixedFirst(380.0), left, right)), floating: Vec::new() }
}

/// A saved layout is used only when it is valid and holds exactly the panels this build knows.
fn checked_dock(l: Option<craft_ui::docking::Layout<Pn>>) -> craft_ui::docking::Layout<Pn> {
    match l {
        Some(l) if l.validate().is_ok() && l.contains(&Pn::Graph) && l.panels().iter().all(|p| Pn::ALL.contains(p)) => l,
        _ => default_dock(),
    }
}
#[derive(serde::Serialize, serde::Deserialize, Default, Clone, PartialEq)]
#[serde(default)]
struct PrefsGeneral {
    skip_home: bool,
    hide_tip: bool,
    snap_nodes: bool,
    /// grid size in graph units; 0 = the default (16)
    snap_grid: u32,
}
#[derive(serde::Serialize, serde::Deserialize, Default, Clone, PartialEq)]
#[serde(default)]
struct PrefsLibrary {
    favorites: Vec<String>,
}
#[derive(serde::Serialize, serde::Deserialize, Default, Clone, PartialEq)]
#[serde(default)]
struct PrefsRecent {
    files: Vec<String>,
}
impl craft_settings::Settings for Prefs {
    const APP_ID: &'static str = "materialcraft";
    const SCHEMA_VERSION: u32 = 1;
    fn migrate(_from: u32, _doc: &mut craft_settings::DocumentMut) -> Result<(), craft_settings::MigrateError> {
        Ok(())
    }
}

const OUTPUT_NAMES: [&str; 11] = ["basecolor", "normal", "roughness", "metallic", "height", "ambient_occlusion", "opacity", "emissive", "diffuse", "specular", "glossiness"];
const COMMENT_COLORS: [[u8; 3]; 6] = [[90, 140, 200], [200, 160, 60], [90, 180, 110], [190, 90, 90], [150, 100, 200], [150, 150, 150]];

// ---------------------------------------------------------------- parameter tables

/// The input sockets a node shows.
fn node_slots(n: &Node, subgraphs: &BTreeMap<String, Graph>) -> Vec<(&'static str, bool)> {
    sd_slots(n, subgraphs)
}

fn kind_color(kind: &str) -> Color32 {
    sd_color(kind)
}

fn qimg(t: &Tex) -> ColorImage {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let bytes: Vec<u8> = t.px.iter().flat_map(|p| [q(p[0]), q(p[1]), q(p[2]), q(p[3])]).collect();
    ColorImage::from_rgba_unmultiplied([t.w, t.h], &bytes)
}

/// The 2D view's picture: as made (with its alpha) and, when some pixel is not fully opaque, the same colours with the
/// alpha ignored (the view's Alpha toggle shows one or the other; the colours under a zero alpha are lost once egui
/// premultiplies them, so the opaque one is made from the bytes too).
struct Pic2d {
    img: ColorImage,
    opaque: Option<ColorImage>,
}

fn pic2d(w: usize, h: usize, bytes: &[u8]) -> Pic2d {
    let img = ColorImage::from_rgba_unmultiplied([w, h], bytes);
    let opaque = if bytes.chunks_exact(4).any(|p| p[3] < 255) {
        let solid: Vec<u8> = bytes.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        Some(ColorImage::from_rgba_unmultiplied([w, h], &solid))
    } else {
        None
    };
    Pic2d { img, opaque }
}

fn pic2d_of(t: &Tex) -> Pic2d {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let bytes: Vec<u8> = t.px.iter().flat_map(|p| [q(p[0]), q(p[1]), q(p[2]), q(p[3])]).collect();
    pic2d(t.w, t.h, &bytes)
}

/// The 2D view's picture and value range from a node result: made on the device when the picture is there (4 bytes per
/// pixel come back, nothing is converted on the host), otherwise from the pixels.
fn view_of(p: &Pending) -> Result<(Pic2d, Option<(f32, f32)>), String> {
    let (w, h) = p.dims();
    if let Some((bytes, range)) = p.view() {
        if bytes.len() == w * h * 4 {
            return Ok((pic2d(w, h, &bytes), Some(range)));
        }
    }
    let t = p.tex()?;
    let range = t.px.iter().fold((f32::MAX, f32::MIN), |m, p| (m.0.min(p[0]).min(p[1]).min(p[2]), m.1.max(p[0]).max(p[1]).max(p[2])));
    Ok((pic2d_of(&t), Some(range)))
}

// ---------------------------------------------------------------- types
#[derive(Clone)]
struct Entry {
    label: String,
    kind: String,
    sub: Option<String>,
    cat: &'static str,
    doc: String,
    nums: Vec<(&'static str, f32)>,
}

struct SearchState {
    screen: Pos2,
    world: Pos2,
    text: String,
    idx: usize,
    link_from: Option<u32>,
    link_to: Option<(u32, String)>,
    fresh: bool,
}

enum Act {
    NewSpline,
    Open,
    Save,
    SaveAs,
    Export,
    /// Export the graph on screen as a MaterialX material (.mtlx + textures).
    ExportMaterialX,
    Bake,
    Undo,
    Redo,
    Copy,
    Cut,
    Paste(Option<Pos2>),
    Duplicate,
    /// Duplicate the selection without the links to nodes outside it, and paste without links
    DuplicateNoLink,
    PasteNoLink(Option<Pos2>),
    /// Delete the selection and connect what fed it to what it fed
    DeleteRelink,
    SelectUpstream,
    SelectDownstream,
    ToggleDisable,
    ComputeThumbs,
    /// Add a node from the common-nodes toolbar (by label), after the active node when there is one
    QuickAdd(&'static str),
    ClearThumbs,
    AddDot(Option<Pos2>),
    AddFrame(Option<Pos2>),
    Delete,
    SelectAll,
    Comment,
    EnterSub(String),
    ExitSub,
    AutoLayout,
    FrameAll,
    Search(Option<Pos2>),
    NewOutput(String, u32),
    /// Swap two connections: the two selected links, or the first two inputs of the selected node (X)
    SwapLinks,
    /// Show this node's outputs in the 3D view instead of the graph outputs (again: back to the graph outputs)
    View3dNode(u32),
    NewGraph,
    Home,
    Drop(String, Pos2),
    /// Import a graph of an open package (index, graph name or None for its main graph) as a node.
    ImportFromPkg(usize, Option<String>, Option<Pos2>),
    NewPackage,
    NewGraphIn(usize),
    /// a new function graph in the `.sbs` package at this index
    NewFunctionIn(usize),
    SwitchPkg(usize),
    OpenGraphTab(String),
    CloseGraphTab(String),
    ToggleDeps,
    SavePkg(usize),
    ClosePkg(usize),
    /// Start a new package from one of the New dialog's templates (index into `TEMPLATES`).
    NewFromTemplate(usize),
    /// Start a new package from one of the user's own templates (its file).
    NewFromUserTemplate(PathBuf),
    /// Save a graph of an open package (index, graph name or None for its main graph) as a template of the New dialog.
    SaveAsTemplate(usize, Option<String>),
    /// Export the maps of a graph of an open package (package index, graph name or None for its main graph).
    ExportMaps(usize, Option<String>),
    /// Save one graph of an open package as a package of its own (with the graphs and functions it needs).
    ExportGraphFile(usize, Option<String>),
    /// Copy the graphs of another `.sbs` package into an open package.
    AddGraphFile(usize),
    /// Ask for a new name for a graph of an open package.
    RenameGraph(usize, String),
    DeleteGraph(usize, String),
    DuplicateGraph(usize, Option<String>),
}

/// Drag payload: a graph of an open package (package index, graph label, graph name or None for the main graph).
#[derive(Clone)]
struct GraphDrag(usize, String, Option<String>);

/// A package that is open in the workspace but not the one being edited right now.
struct Pkg {
    path: Option<PathBuf>,
    name: String,
    g: Graph,
    modified: bool,
    /// graph tabs that were open in this package (subgraph names)
    tabs: Vec<String>,
}

struct EvalResult {
    gen: u64,
    thumbs: HashMap<u32, Vec<u8>>,
    errors: HashMap<u32, String>,
    kept: HashMap<u32, std::sync::Arc<Tex>>,
    outputs: Vec<(String, u32)>,
    sizes: HashMap<u32, usize>,
    /// the 2D view image and the GPU textures are converted on the worker thread so the editor never stalls on them
    img2d: Option<Pic2d>,
    img_for: Option<u32>,
    gpu: Option<gpu3d::GpuMaps>,
    /// the normal map turned to the OpenGL convention the viewers light (made on the worker thread too)
    normal_gl: Option<std::sync::Arc<Tex>>,
    /// material outputs that were packed for the 3D view on the device and never read back: (output name, result)
    pending: Vec<(String, Pending)>,
    /// the pending normal map is DirectX style and has to be flipped before a CPU viewer uses it
    pending_flip: bool,
    /// lowest and highest colour value of the 2D view's image (scanned on the worker thread)
    range2d: Option<(f32, f32)>,
    /// resolution this pass was computed at
    size: usize,
    /// how long the pass took (evaluation and conversion), in milliseconds, and the parts of it
    ms: f32,
    ms_eval: f32,
    ms_prep: f32,
    ms_2d: f32,
    pass: Pass,
}

/// `MC_TRACE_EVAL=1` prints a timeline of every edit to the console: when each pass starts and when the 3D view gets
/// its maps, in milliseconds since the edit. It is how the speed of the edit -> 3D view path is measured.
fn trace_on() -> bool {
    static T: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *T.get_or_init(|| std::env::var_os("MC_TRACE_EVAL").is_some())
}

static EDIT_AT: Mutex<Option<std::time::Instant>> = Mutex::new(None);

fn trace(msg: &str) {
    if trace_on() {
        let t = EDIT_AT.lock().ok().and_then(|g| *g).map(|t| t.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0);
        eprintln!("[eval +{t:>7.0} ms] {msg}");
    }
}

/// What an evaluation pass is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pass {
    /// the viewed node and the outputs only, at a small or growing size: while a value moves
    Interactive,
    /// every node at a small size: all thumbnails, and first 3D maps (run once the input has settled)
    Thumbs,
    /// the outputs at the full preview size: the final 3D maps
    Final,
}

/// The 2D view image of the selected node, sent as soon as that node is done (before the rest of the graph).
struct Partial {
    gen: u64,
    img2d: Pic2d,
    img_for: u32,
}

struct MtlxUi {
    open: bool,
    dir: Option<PathBuf>,
    opt: materialcraft::mtlx::Options,
    /// index into [graph size, 256, 512, 1024, 2048, 4096, 8192]
    size: usize,
    name: String,
}

struct ExportUi {
    open: bool,
    dir: Option<PathBuf>,
    prefix: String,
    /// file name pattern: `$(graph)`, `$(identifier)`, `$(usage)` and `$(size)` are filled in
    pattern: String,
    sizes: [bool; 6],
    depth16: bool,
    depth32: bool,
    fmt: Format,
    quality: u8,
    skip: BTreeSet<String>,
    /// output colour space for colour maps (index into `colorspace::SPACES`)
    space: usize,
    /// whose maps: (package index, graph name or None for its main graph); None = the graph on screen
    target: Option<(usize, Option<String>)>,
}

struct App {
    gpu_eval: bool,
    /// graphs of the open package that have a tab (the main graph always has one)
    graph_tabs: Vec<String>,
    show_deps: bool,
    /// smallest / largest channel value of the node shown in the 2D view (HDR range readout)
    tex2d_range: Option<(f32, f32)>,
    g: Graph,
    parents: Vec<(Graph, String)>,
    undo: Vec<Graph>,
    redo: Vec<Graph>,
    edit_open: bool,
    sel: BTreeSet<u32>,
    /// selected links, each named by the node it feeds and that node's input slot
    sel_links: BTreeSet<(u32, String)>,
    active: Option<u32>,
    sel_comment: Option<usize>,
    clip: Vec<(u32, Node)>,
    pan: Vec2,
    zoom: f32,
    canvas_rect: Rect,
    /// the spline editor, when open
    spl: Option<SplineEd>,
    connecting: Option<u32>,
    /// Output name of the graph-node socket a link is being dragged from.
    connecting_out: Option<String>,
    /// Shift-drag from an output: move all of its links to another node
    relink_all: bool,
    /// Output name to apply when a node created from a link drag gets connected.
    link_out: Option<String>,
    rubber: Option<Pos2>,
    comment_members: Vec<u32>,
    search: Option<SearchState>,
    acts: Vec<Act>,
    status: (String, bool),
    file: Option<PathBuf>,
    /// editor preview resolution in pixels; 0 = the graph's own size (capped at 4096)
    prev_size: usize,
    /// a quick low-resolution result was just shown; compute the full-size one next
    refine: bool,
    /// when the input has settled enough to run the thumbnail and final passes; reset on every change
    settle_at: Option<std::time::Instant>,
    /// the thumbnail pass (every node, small) is still to run for the current graph state
    thumbs_due: bool,
    /// the final pass (outputs at full size, 3D maps) is still to run for the current graph state
    final_due: bool,
    /// resolution of the pass being computed, and the next (bigger) one to run when it finishes
    job_size: usize,
    refine_size: Option<usize>,
    node_sizes: HashMap<u32, usize>,
    /// resolution of the pass whose results are on screen (node_sizes are measured at this size)
    pass_size: usize,
    /// resolution of the maps the 3D view shows
    maps_size: usize,
    /// milliseconds per pixel of the last interactive pass: the size used while a value moves follows it
    drag_ms_px: f32,
    /// per-pixel cost of the last few interactive passes (the worst one counts)
    drag_hist: [f32; 4],
    tile_preview: bool,
    thumbs: HashMap<u32, egui::TextureHandle>,
    /// pixels behind each thumbnail texture (an unchanged thumbnail is not uploaded again)
    thumb_px: HashMap<u32, Vec<u8>>,
    node_err: HashMap<u32, String>,
    tex2d: Option<egui::TextureHandle>,
    /// the 2D view's picture without its alpha (only when the picture has some)
    tex2d_opaque: Option<egui::TextureHandle>,
    /// the 2D view shows the alpha over a checkerboard (off: the colours fully opaque)
    alpha2d: bool,
    tex2d_size: (usize, usize),
    maps: BTreeMap<String, std::sync::Arc<Tex>>,
    /// a node whose outputs the 3D view shows instead of the graph outputs (a material node such as PBR Base Material)
    view3d_node: Option<u32>,
    /// outputs still on the device (see `ensure_maps`), and whether the normal map needs its green channel flipped
    maps_pending: Vec<(String, Pending)>,
    maps_pending_flip: bool,
    view3d: View,
    v3: V3dUi,
    tex3d: Option<egui::TextureHandle>,
    /// the GPU viewer's last picture (made on the device, shown as a texture) and its number
    tex_gpu: Option<egui::TextureHandle>,
    gpu_seq: u64,
    r3d_size: usize,
    gpu: gpu3d::Shared,
    gpu_on: bool,
    gpu_maps_dirty: bool,
    /// Open packages in display order; `None` marks the one being edited (its live state is in `g`, `file`, ...).
    pkgs: Vec<Option<Pkg>>,
    pkg_active: usize,
    /// Package that new graphs go to (selected in the Explorer).
    pkg_sel: usize,
    unsaved_name: String,
    confirm_close: Option<usize>,
    /// the rename box of the Explorer: (package index, graph, new name being typed)
    rename_graph: Option<(usize, String, String)>,
    /// the "Save as template" box: (package index, graph or None for its main graph, template name being typed)
    save_tmpl: Option<(usize, Option<String>, String)>,
    dirty: bool,
    /// the selected node changed and nothing else: only the 2D view needs another node's cached picture
    view_pending: bool,
    /// the next pass is a view-only one (see `job_view_only`)
    view_only_next: bool,
    /// the running (or last) pass only serves the 2D view: no maps, no thumbnails
    job_view_only: bool,
    /// the pass the running job is, and when it started
    job_pass: Pass,
    job_started: std::time::Instant,
    /// a value changed while a short interactive pass was still running: start the next one when it is done instead
    /// of cancelling it (cancelling every frame would never show anything during a steady drag)
    dirty_wait: bool,
    dirty3d: bool,
    gen: u64,
    job: Option<Receiver<EvalResult>>,
    part: Option<Receiver<Partial>>,
    node_cache: std::sync::Arc<NodeCache>,
    lib_search: String,
    text_focus: bool,
    export: ExportUi,
    mtlx_ui: MtlxUi,
    bake: BakeUi,
    res_dirs: Vec<PathBuf>,
    show_about: bool,
    /// the AI control window is open
    show_ai: bool,
    import_view: Option<ImportView>,
    /// listening for assistants on a local port
    ctl: Option<ControlLink>,
    screen: Screen,
    new_dlg: NewDlg,
    recent: Vec<PathBuf>,
    home_filter: String,
    home_page: u8,
    skip_home: bool,
    hide_suggestions: bool,
    prefs: craft_settings::SettingsStore<Prefs>,
    dock: craft_ui::docking::Layout<Pn>,
    dock_closed: HashMap<Pn, craft_ui::docking::Location<Pn>>,
    banner: Option<egui::TextureHandle>,
    banner_rx: Option<Receiver<ColorImage>>,
    lib_thumbs: HashMap<String, egui::TextureHandle>,
    lib_rx: Option<Receiver<HashMap<String, ColorImage>>>,
    lib_cat: String,
    view2d_rect: Rect,
    favorites: BTreeSet<String>,
    show: [bool; 5],
    modified: bool,
    last_title: String,
    z2d: f32,
    pan2d: Vec2,
    need_fit: bool,
    connecting_in: Option<(u32, String)>,
    /// snap dragged nodes to a grid, and the grid size in graph units
    snap_nodes: bool,
    snap_grid: f32,
    /// size of the textures on the 3D mesh: 0 = the output nodes' own resolution, else a fixed size
    mesh_tex_px: usize,
    copy_out: Option<String>,
    logo: Option<egui::TextureHandle>,
    logo_note: String,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> App {
        apply_theme(&cc.egui_ctx);
        let (logo_tex, logo_note) = logo().clone();
        let logo_handle = cc.egui_ctx.load_texture("logo", qimg(&logo_tex), egui::TextureOptions::LINEAR);
        // the editor always holds an .sbs package: a fresh one until a package is created or opened
        let g = materialcraft::sbs_native::new_package("Graph", 512);
        let active: Option<u32> = None;
        let (mut prefs, _prefs_report) = craft_settings::SettingsStore::<Prefs>::open(craft_settings::Location::Auto);
        if prefs.path().map(|p| !p.exists()).unwrap_or(false) {
            // first run with the new store: carry over the old plain-text settings
            let old = read_lines("settings.txt");
            let mut v = Prefs::default();
            v.general.skip_home = old.iter().any(|l| l == "skip_home=1");
            v.library.favorites = old.iter().find_map(|l| l.strip_prefix("favorites=")).map(|f| f.split(',').filter(|x| !x.is_empty()).map(String::from).collect()).unwrap_or_default();
            v.recent.files = read_lines("recent.txt");
            prefs.set(v);
        }
        let skip_home = prefs.get().general.skip_home;
        let favorites: BTreeSet<String> = prefs.get().library.favorites.iter().cloned().collect();
        // only Substance packages open: older MaterialCraft files drop out of the list
        let recent: Vec<PathBuf> = prefs.get().recent.files.iter().map(PathBuf::from).filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("sbs")).unwrap_or(false)).collect();
        App {
            gpu_eval: true,
            graph_tabs: Vec::new(),
            show_deps: false,
            tex2d_range: None,
            parents: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            edit_open: false,
            sel: BTreeSet::new(),
            sel_links: BTreeSet::new(),
            active,
            sel_comment: None,
            clip: Vec::new(),
            pan: vec2(30.0, 30.0),
            zoom: 0.9,
            canvas_rect: Rect::from_min_size(Pos2::ZERO, vec2(900.0, 500.0)),
            spl: None,
            connecting: None,
            connecting_out: None,
            relink_all: false,
            link_out: None,
            rubber: None,
            comment_members: Vec::new(),
            search: None,
            acts: Vec::new(),
            status: ("Press Tab in the graph to add a node at the cursor.".into(), true),
            file: None,
            prev_size: 0,
            refine: false,
            settle_at: None,
            thumbs_due: false,
            final_due: false,
            job_size: 0,
            refine_size: None,
            node_sizes: HashMap::new(),
            pass_size: 0,
            maps_size: 0,
            drag_ms_px: 0.0,
            drag_hist: [0.0; 4],
            tile_preview: false,
            thumbs: HashMap::new(),
            thumb_px: HashMap::new(),
            node_err: HashMap::new(),
            tex2d: None,
            tex2d_opaque: None,
            alpha2d: true,
            tex2d_size: (0, 0),
            maps: BTreeMap::new(),
            view3d_node: None,
            maps_pending: Vec::new(),
            maps_pending_flip: false,
            view3d: View { tess: 512, ..View::default() },
            v3: V3dUi::default(),
            tex3d: None,
            tex_gpu: None,
            gpu_seq: 0,
            r3d_size: 420,
            gpu: gpu3d::new_shared(),
            gpu_on: true,
            gpu_maps_dirty: true,
            pkgs: Vec::new(),
            pkg_active: 0,
            pkg_sel: 0,
            unsaved_name: "Unsaved Package".into(),
            confirm_close: None,
            rename_graph: None,
            save_tmpl: None,
            dirty: true,
            view_pending: false,
            view_only_next: false,
            job_view_only: false,
            job_pass: Pass::Thumbs,
            job_started: std::time::Instant::now(),
            dirty_wait: false,
            dirty3d: true,
            gen: 0,
            job: None,
            part: None,
            node_cache: std::sync::Arc::new(NodeCache::with_default_budget()),
            lib_search: String::new(),
            text_focus: false,
            mtlx_ui: MtlxUi { open: false, dir: None, opt: materialcraft::mtlx::Options::default(), size: 0, name: String::new() },
            export: ExportUi { open: false, dir: None, prefix: String::new(), pattern: "$(graph)_$(identifier)".into(), sizes: [false, true, false, false, false, false], depth16: false, depth32: false, fmt: Format::Png, quality: 92, skip: BTreeSet::new(), space: 0, target: None },
            bake: BakeUi::new(),
            res_dirs: Vec::new(),
            show_about: false,
            show_ai: false,
            import_view: None,
            ctl: None,
            screen: if skip_home { Screen::Editor } else { Screen::Home },
            new_dlg: NewDlg { open: false, sel: 1, user: Vec::new(), name: "Graph".into(), size: 2048, height: 0, depth16: false, dest: None },
            recent,
            home_filter: String::new(),
            home_page: 0,
            skip_home,
            hide_suggestions: prefs.get().general.hide_tip,
            mesh_tex_px: 0,
            snap_nodes: prefs.get().general.snap_nodes,
            snap_grid: if prefs.get().general.snap_grid == 0 { 16.0 } else { prefs.get().general.snap_grid as f32 },
            dock: checked_dock(prefs.get().layout.dock.clone()),
            dock_closed: HashMap::new(),
            prefs,
            banner: None,
            banner_rx: Some(spawn_banner()),
            lib_thumbs: HashMap::new(),
            lib_rx: Some(spawn_library_thumbs()),
            lib_cat: "Generators/Patterns".into(),
            view2d_rect: Rect::NOTHING,
            favorites,
            show: [true; 5],
            modified: false,
            last_title: String::new(),
            z2d: 1.0,
            pan2d: Vec2::ZERO,
            need_fit: false,
            connecting_in: None,
            copy_out: None,
            logo: Some(logo_handle),
            logo_note,
            g,
        }
    }

    /// Show graph `name` ("" = the package's main graph) in the editor.
    fn open_graph_tab(&mut self, name: &str) {
        let current = self.parents.last().map(|(_, n)| n.clone()).unwrap_or_default();
        if current == name {
            return;
        }
        self.goto_root();
        if !name.is_empty() {
            self.enter_sub(name);
        }
    }

    fn close_graph_tab(&mut self, name: &str) {
        let current = self.parents.last().map(|(_, n)| n.clone()).unwrap_or_default();
        self.graph_tabs.retain(|t| t != name);
        if current == name {
            self.goto_root();
        }
    }

    /// Copy a graph of another open package (with the graphs and functions it uses) into this one and place it as
    /// a node.
    fn import_from_pkg(&mut self, i: usize, graph: Option<String>, at: Option<Pos2>) {
        let Some(pk) = self.pkgs.get(i).and_then(|p| p.as_ref()) else {
            return self.status("Drag graphs from another package onto this graph", false);
        };
        let name = graph.unwrap_or_else(|| materialcraft::sbs_native::graph_name(&pk.g));
        let copied = match materialcraft::sbs_native::extract_package(&pk.g, &name) {
            Ok(g) => g,
            Err(e) => return self.status(format!("Cannot copy '{name}': {e}"), false),
        };
        self.snap();
        match materialcraft::sbs_native::add_package(&mut self.g, copied) {
            Ok((first, n, warnings)) => {
                let pos = at.unwrap_or_else(|| self.view_center_world());
                let id = self.sd_add_node("sbs:graph", Some(&first), pos, false);
                self.undo.pop();
                self.sel = [id].into_iter().collect();
                self.active = Some(id);
                self.dirty = true;
                self.modified = true;
                let more = if warnings.is_empty() { String::new() } else { format!(" ({})", warnings.join("; ")) };
                self.status(format!("Copied '{first}' ({n} graph(s)) into this package and placed it as a node{more}"), warnings.is_empty());
            }
            Err(e) => {
                self.undo.pop();
                self.status(format!("Cannot copy '{name}': {e}"), false);
            }
        }
    }

    // ------------------------------------------------------------ workspace of packages
    fn ws_init(&mut self) {
        if self.pkgs.is_empty() {
            self.pkgs = vec![None];
            self.pkg_active = 0;
            self.pkg_sel = 0;
        }
        if self.pkg_active >= self.pkgs.len() {
            self.pkg_active = 0;
        }
        if self.pkg_sel >= self.pkgs.len() {
            self.pkg_sel = self.pkg_active;
        }
    }

    /// Start a fresh workspace holding just the package that is about to be loaded.
    fn ws_fresh(&mut self) {
        self.pkgs = vec![None];
        self.pkg_active = 0;
        self.pkg_sel = 0;
        self.confirm_close = None;
    }

    fn pkg_title(&self, i: usize) -> String {
        match self.pkgs.get(i) {
            Some(Some(p)) => p.name.clone(),
            _ => self.package_label(),
        }
    }

    fn pkg_modified(&self, i: usize) -> bool {
        match self.pkgs.get(i) {
            Some(Some(p)) => p.modified || p.path.is_none(),
            _ => self.modified || self.file.is_none(),
        }
    }

    /// True when closing package `i` would lose changes.
    fn pkg_dirty(&self, i: usize) -> bool {
        match self.pkgs.get(i) {
            Some(Some(p)) => p.modified,
            _ => self.modified,
        }
    }

    fn unique_unsaved_name(&self) -> String {
        let taken = |n: &str| (0..self.pkgs.len()).any(|i| self.pkg_title(i) == n);
        let mut n = "Unsaved Package".to_string();
        let mut k = 2;
        while taken(&n) {
            n = format!("Unsaved Package {k}");
            k += 1;
        }
        n
    }

    fn snapshot_active(&mut self) -> Pkg {
        self.goto_root();
        Pkg { path: self.file.clone(), name: self.package_label(), g: self.g.clone(), modified: self.modified, tabs: self.graph_tabs.clone() }
    }

    fn load_active(&mut self, p: Pkg) {
        self.parents.clear();
        self.g = p.g;
        self.graph_tabs = p.tabs;
        self.clear_views();
        self.file = p.path;
        self.unsaved_name = p.name;
        self.modified = p.modified;
        self.clear_history();
        self.thumbs.clear();
        self.thumb_px.clear();
        self.screen = Screen::Editor;
        self.fit_view();
        self.active = self.g.nodes.keys().next().copied();
    }

    /// Make open package `i` the one shown in the editor.
    fn switch_pkg(&mut self, i: usize) {
        self.ws_init();
        self.pkg_sel = i.min(self.pkgs.len() - 1);
        if i == self.pkg_active || i >= self.pkgs.len() {
            return;
        }
        let Some(target) = self.pkgs[i].take() else { return };
        let snap = self.snapshot_active();
        self.pkgs[self.pkg_active] = Some(snap);
        self.load_active(target);
        self.pkg_active = i;
    }

    /// Add a new package to the workspace and edit it.
    fn add_pkg_active(&mut self, p: Pkg) {
        // a workspace that is open behind the home screen stays open: the new package joins it
        if self.screen != Screen::Editor && self.pkgs.is_empty() {
            self.ws_fresh();
            self.load_active(p);
            return;
        }
        self.ws_init();
        let snap = self.snapshot_active();
        self.pkgs[self.pkg_active] = Some(snap);
        self.pkgs.push(None);
        self.pkg_active = self.pkgs.len() - 1;
        self.pkg_sel = self.pkg_active;
        self.load_active(p);
    }

    fn close_pkg(&mut self, i: usize) {
        self.ws_init();
        if i >= self.pkgs.len() {
            return;
        }
        if self.pkgs.len() == 1 {
            self.pkgs.clear();
            self.screen = Screen::Home;
            // what "Go to editor" shows next is a fresh package, not the one just closed
            self.load_active(Pkg { path: None, name: "Unsaved Package".into(), g: materialcraft::sbs_native::new_package("Graph", 512), modified: false, tabs: Vec::new() });
            self.screen = Screen::Home;
            return;
        }
        if i == self.pkg_active {
            let t = if i + 1 < self.pkgs.len() { i + 1 } else { i - 1 };
            let Some(target) = self.pkgs[t].take() else { return };
            self.load_active(target);
            self.pkgs.remove(i);
            self.pkg_active = if t > i { t - 1 } else { t };
        } else {
            self.pkgs.remove(i);
            if i < self.pkg_active {
                self.pkg_active -= 1;
            }
        }
        self.pkg_sel = self.pkg_active;
    }

    fn status(&mut self, msg: impl Into<String>, ok: bool) {
        self.status = (msg.into(), ok);
    }

    // ------------------------------------------------------------ undo / redo
    fn snap(&mut self) {
        self.modified = true;
        self.undo.push(self.g.clone());
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.g, prev));
            self.after_restore();
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.g, next));
            self.after_restore();
        }
    }

    fn after_restore(&mut self) {
        let g = &self.g;
        self.sel.retain(|i| g.nodes.contains_key(i));
        if self.active.map(|a| !g.nodes.contains_key(&a)).unwrap_or(false) {
            self.active = None;
        }
        self.sel_comment = None;
        self.dirty = true;
    }

    fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.sel.clear();
        self.active = None;
        self.sel_comment = None;
        self.dirty = true;
    }

    // ------------------------------------------------------------ graph editing
    fn depends_on(&self, a: u32, b: u32) -> bool {
        let mut stack = vec![a];
        let mut seen = BTreeSet::new();
        while let Some(x) = stack.pop() {
            if x == b {
                return true;
            }
            if !seen.insert(x) {
                continue;
            }
            if let Some(n) = self.g.nodes.get(&x) {
                stack.extend(n.ins.values().copied());
            }
        }
        false
    }

    fn try_connect(&mut self, src: u32, dst: u32, slot: &str) {
        self.try_connect_out(src, dst, slot, None);
    }

    /// Connect `src` (reading its named output when it is a graph node) to input `slot` of `dst`.
    /// The inputs (node, slot) fed by output `out` of `src` (None: its only or first output).
    fn children_of(&self, src: u32, out: Option<&str>) -> Vec<(u32, String)> {
        let first = self.g.nodes.get(&src).and_then(|n| materialcraft::graph::node_outputs(&n.kind).first().map(|s| s.to_string()));
        let mut v = Vec::new();
        for (id, n) in &self.g.nodes {
            for (slot, from) in &n.ins {
                if *from != src {
                    continue;
                }
                let link_out = n.in_out.get(slot).cloned().or_else(|| first.clone());
                let want = out.map(|s| s.to_string()).or_else(|| first.clone());
                if link_out == want {
                    v.push((*id, slot.clone()));
                }
            }
        }
        v
    }

    /// Shift-drag: detach every link of output `out` of `src` and connect them to `target` instead. The nodes below keep
    /// their place in the graph; links that would make a cycle or mismatch in type stay where they were.
    fn relink_children(&mut self, src: u32, out: Option<String>, target: u32, tout: Option<String>) {
        if src == target {
            return;
        }
        let kids = self.children_of(src, out.as_deref());
        let pins = materialcraft::graph::pin_types(&self.g);
        let from = match (&tout, self.g.nodes.get(&target)) {
            (Some(o), Some(tn)) if !materialcraft::graph::node_outputs(&tn.kind).is_empty() => materialcraft::graph::output_pin(&tn.kind, Some(o.as_str()), pins.get(&target) == Some(&materialcraft::graph::Pin::Color), tn),
            _ => pins.get(&target).copied().unwrap_or(materialcraft::graph::Pin::Gray),
        };
        let mut todo = Vec::new();
        let mut skipped = 0;
        for (kid, slot) in kids {
            let ok_type = self.g.nodes.get(&kid).map(|k| materialcraft::graph::input_value_type(&k.kind, &slot).unwrap_or(materialcraft::graph::Pin::Gray).accepts(from)).unwrap_or(false);
            if kid == target || self.depends_on(target, kid) || !ok_type {
                skipped += 1;
            } else {
                todo.push((kid, slot));
            }
        }
        if todo.is_empty() {
            self.status("Nothing moved: the links would make a cycle or do not fit that output.", false);
            return;
        }
        self.snap();
        let moved = todo.len();
        for (kid, slot) in todo {
            if let Some(n) = self.g.nodes.get_mut(&kid) {
                n.ins.insert(slot.clone(), target);
                match &tout {
                    Some(o) => {
                        n.in_out.insert(slot, o.clone());
                    }
                    None => {
                        n.in_out.remove(&slot);
                    }
                }
            }
        }
        self.dirty = true;
        let extra = if skipped > 0 { format!(" ({skipped} left in place)") } else { String::new() };
        self.status(format!("Moved {moved} link(s) to the new node{extra}"), true);
    }

    fn try_connect_out(&mut self, src: u32, dst: u32, slot: &str, out: Option<String>) {
        if src == dst || self.depends_on(src, dst) {
            self.status("Connection rejected: it would create a cycle.", false);
            return;
        }
        // a value pin never connects to an image pin (or the other way round)
        {
            let pins = materialcraft::graph::pin_types(&self.g);
            let (sn, dn) = (self.g.nodes.get(&src), self.g.nodes.get(&dst));
            if let (Some(sn), Some(dn)) = (sn, dn) {
                let from = match &out {
                    Some(o) if !materialcraft::graph::node_outputs(&sn.kind).is_empty() => materialcraft::graph::output_pin(&sn.kind, Some(o.as_str()), pins.get(&src) == Some(&materialcraft::graph::Pin::Color), sn),
                    _ => pins.get(&src).copied().unwrap_or(materialcraft::graph::Pin::Gray),
                };
                let want = materialcraft::graph::input_value_type(&dn.kind, slot).unwrap_or(materialcraft::graph::Pin::Gray);
                if !want.accepts(from) {
                    self.status(format!("Connection rejected: {} input needs {}, this output is {}.", slot, want.name(), from.name()), false);
                    return;
                }
            }
        }
        self.snap();
        if let Some(n) = self.g.nodes.get_mut(&dst) {
            n.ins.insert(slot.to_string(), src);
            match out {
                Some(o) => {
                    n.in_out.insert(slot.to_string(), o);
                }
                None => {
                    n.in_out.remove(slot);
                }
            }
            self.dirty = true;
        }
    }

    fn view_center_world(&self) -> Pos2 {
        let c = self.canvas_rect;
        let w = ((c.center() - c.min) - self.pan) / self.zoom;
        pos2(w.x - CARD * 0.5, w.y - CARD_H * 0.5)
    }

    fn spawn(&mut self, e: &Entry, world: Pos2, link_from: Option<u32>) -> Option<u32> {
        match e.kind.as_str() {
            "comment" => {
                self.add_comment_at(Some(world));
                return None;
            }
            "frame" => {
                self.add_comment_at(None);
                return None;
            }
            _ => {}
        }
        let gray = e.nums.iter().any(|(k, v)| *k == "gray" && *v > 0.5);
        let id = self.sd_add_node(&e.kind, e.sub.as_deref(), world, gray);
        if let Some(src) = link_from.filter(|s| *s != id && e.kind != "sbs:input") {
            let first = self.g.nodes.get(&id).map(|n| sd_slots(n, &self.g.subgraphs)).and_then(|v| v.first().map(|s| s.0.to_string()));
            if let Some(slot) = first {
                let o = self.link_out.take();
                if let Some(n) = self.g.nodes.get_mut(&id) {
                    n.ins.insert(slot.clone(), src);
                    if let Some(o) = o {
                        n.in_out.insert(slot, o);
                    }
                }
            }
        }
        Some(id)
    }

    fn new_output(&mut self, usage: &str, from: u32) {
        self.snap();
        let pos = self.g.nodes.get(&from).map(|n| [n.pos[0] + CARD + 70.0, n.pos[1]]).unwrap_or([0.0, 0.0]);
        let id = materialcraft::sbs_native::add_output(&mut self.g, usage, None, pos);
        if let Some(n) = self.g.nodes.get_mut(&id) {
            n.ins.insert("inputNodeOutput".into(), from);
        }
        self.sel = [id].into_iter().collect();
        self.active = Some(id);
        self.dirty = true;
    }

    /// Take the selected links out of the graph (no undo step of its own).
    fn remove_selected_links(&mut self) {
        for (dst, slot) in std::mem::take(&mut self.sel_links) {
            if let Some(n) = self.g.nodes.get_mut(&dst) {
                n.ins.remove(&slot);
                n.in_out.remove(&slot);
            }
        }
        self.dirty = true;
    }

    /// Swap what feeds two inputs: the two selected links; one selected link and the next input of its node; or the
    /// first two inputs of the selected node (a Blend's foreground and background). A swap that would join pins of
    /// different types is refused.
    fn swap_links(&mut self) {
        let next_slot = |app: &App, id: u32, not: Option<&str>| -> Option<String> {
            let n = app.g.nodes.get(&id)?;
            node_slots(n, &app.g.subgraphs).into_iter().map(|s| s.0).find(|s| Some(*s) != not).map(str::to_string)
        };
        let links: Vec<(u32, String)> = self.sel_links.iter().cloned().collect();
        let pair = match links.as_slice() {
            [a, b] => Some((a.clone(), b.clone())),
            [a] => next_slot(self, a.0, Some(a.1.as_str())).map(|s| (a.clone(), (a.0, s))),
            [] if self.sel.len() == 1 => {
                let id = *self.sel.iter().next().unwrap();
                let first = next_slot(self, id, None);
                let second = first.as_deref().and_then(|f| next_slot(self, id, Some(f)));
                first.zip(second).map(|(f, s)| ((id, f), (id, s)))
            }
            _ => None,
        };
        let Some((a, b)) = pair else {
            return self.status("Swap (X): select two links, one link, or a node with two inputs.", false);
        };
        let feed = |g: &Graph, (dst, slot): &(u32, String)| g.nodes.get(dst).and_then(|n| n.ins.get(slot).map(|s| (*s, n.in_out.get(slot).cloned())));
        let (fa, fb) = (feed(&self.g, &a), feed(&self.g, &b));
        if fa.is_none() && fb.is_none() {
            return self.status("Swap (X): neither input is connected.", false);
        }
        let set = |g: &mut Graph, (dst, slot): &(u32, String), f: Option<(u32, Option<String>)>| {
            if let Some(n) = g.nodes.get_mut(dst) {
                n.ins.remove(slot);
                n.in_out.remove(slot);
                if let Some((src, out)) = f {
                    n.ins.insert(slot.clone(), src);
                    if let Some(o) = out {
                        n.in_out.insert(slot.clone(), o);
                    }
                }
            }
        };
        // a source may not feed a node it depends on (a link between two nodes swapped into a loop)
        let cycle = [(&a, &fb), (&b, &fa)].iter().any(|(dst, f)| f.as_ref().map(|(src, _)| *src == dst.0 || self.depends_on(*src, dst.0)).unwrap_or(false));
        if cycle {
            return self.status("Swap rejected: it would create a cycle.", false);
        }
        let issues = |g: &Graph| materialcraft::graph::connection_issues(g).into_iter().map(|(n, s, _)| (n, s)).collect::<BTreeSet<_>>();
        let before = issues(&self.g);
        self.snap();
        set(&mut self.g, &a, fb.clone());
        set(&mut self.g, &b, fa.clone());
        if let Some(bad) = issues(&self.g).difference(&before).next() {
            let (_, slot) = bad.clone();
            self.undo.pop();
            set(&mut self.g, &a, fa);
            set(&mut self.g, &b, fb);
            return self.status(format!("Swap rejected: input '{slot}' cannot take that connection's type."), false);
        }
        // the selection follows the links to their new inputs
        self.sel_links = [a, b].into_iter().filter(|(dst, slot)| self.g.nodes.get(dst).map(|n| n.ins.contains_key(slot)).unwrap_or(false)).filter(|_| !links.is_empty()).collect();
        self.dirty = true;
    }

    fn delete_selected(&mut self) {
        if self.sel.is_empty() && !self.sel_links.is_empty() {
            self.snap();
            return self.remove_selected_links();
        }
        if self.sel.is_empty() {
            if let Some(ci) = self.sel_comment.take() {
                if ci < self.g.comments.len() {
                    self.snap();
                    self.g.comments.remove(ci);
                }
            }
            return;
        }
        self.snap();
        self.remove_selected_links();
        let sel = std::mem::take(&mut self.sel);
        // graph inputs and outputs go with the nodes that read and write them
        materialcraft::sbs_native::remove_nodes(&mut self.g, &sel);
        for id in &sel {
            self.g.nodes.remove(id);
        }
        for n in self.g.nodes.values_mut() {
            n.ins.retain(|_, s| !sel.contains(s));
        }
        self.g.outputs.retain(|(_, s)| !sel.contains(s));
        self.g.exposed.retain(|e| !sel.contains(&e.node));
        self.active = None;
        self.dirty = true;
    }

    /// Delete the selection; each link that read a deleted node is rewired to what that node read first.
    fn delete_relink(&mut self) {
        if self.sel.is_empty() {
            return self.delete_selected();
        }
        let before = self.g.clone();
        let sel = self.sel.clone();
        // what each deleted node passes through: its first input (outside the selection, following chains)
        let mut pass: HashMap<u32, Option<(u32, Option<String>)>> = HashMap::new();
        for id in &sel {
            let mut cur = *id;
            let mut seen = BTreeSet::new();
            let mut res = None;
            while seen.insert(cur) {
                let Some(n) = self.g.nodes.get(&cur) else { break };
                let first = n.ins.iter().next().map(|(slot, s)| (*s, n.in_out.get(slot).cloned()));
                match first {
                    Some((src, _)) if sel.contains(&src) => cur = src,
                    Some((src, o)) => {
                        res = Some((src, o));
                        break;
                    }
                    None => break,
                }
            }
            pass.insert(*id, res);
        }
        for (nid, n) in self.g.nodes.iter_mut() {
            if sel.contains(nid) {
                continue;
            }
            let slots: Vec<String> = n.ins.iter().filter(|(_, s)| sel.contains(s)).map(|(k, _)| k.clone()).collect();
            for slot in slots {
                let from = n.ins[&slot];
                match pass.get(&from).cloned().flatten() {
                    Some((src, o)) => {
                        n.ins.insert(slot.clone(), src);
                        match o {
                            Some(o) => {
                                n.in_out.insert(slot, o);
                            }
                            None => {
                                n.in_out.remove(&slot);
                            }
                        }
                    }
                    None => {
                        n.ins.remove(&slot);
                        n.in_out.remove(&slot);
                    }
                }
            }
        }
        // the nodes are gone and nothing reads them any more
        self.delete_selected();
        if let Some(last) = self.undo.last_mut() {
            *last = before;
        }
    }

    /// Select everything the selection reads (upstream) or feeds (downstream).
    fn select_reach(&mut self, up: bool) {
        let mut set = self.sel.clone();
        let mut todo: Vec<u32> = set.iter().copied().collect();
        while let Some(id) = todo.pop() {
            let next: Vec<u32> = if up {
                self.g.nodes.get(&id).map(|n| n.ins.values().copied().collect()).unwrap_or_default()
            } else {
                self.g.nodes.iter().filter(|(_, n)| n.ins.values().any(|s| *s == id)).map(|(i, _)| *i).collect()
            };
            for i in next {
                if self.g.nodes.contains_key(&i) && set.insert(i) {
                    todo.push(i);
                }
            }
        }
        self.sel = set;
    }

    fn toggle_disable(&mut self) {
        if self.sel.is_empty() {
            return;
        }
        self.snap();
        let all_off = self.sel.iter().all(|i| self.g.nodes.get(i).map(|n| n.nums.get("_disabled").copied().unwrap_or(0.0) > 0.5).unwrap_or(true));
        for i in self.sel.clone() {
            if let Some(n) = self.g.nodes.get_mut(&i) {
                if all_off {
                    n.nums.remove("_disabled");
                } else {
                    n.nums.insert("_disabled".into(), 1.0);
                }
            }
        }
        self.dirty = true;
    }

    fn copy(&mut self) {
        self.clip = self.sel.iter().filter_map(|i| self.g.nodes.get(i).map(|n| (*i, n.clone()))).collect();
        if self.clip.is_empty() {
            return;
        }
        let ids: BTreeSet<u32> = self.clip.iter().map(|(i, _)| *i).collect();
        let mut tmp = Graph::default();
        for (id, n) in &self.clip {
            let mut nn = n.clone();
            nn.ins.retain(|_, s| ids.contains(s));
            tmp.nodes.insert(*id, nn);
        }
        self.copy_out = Some(format!("MaterialCraft nodes\n{}", tmp.to_text()));
        self.status(format!("Copied {} node(s)", self.clip.len()), true);
    }

    fn paste(&mut self, at: Option<Pos2>) {
        self.paste_ex(at, true);
    }

    /// Paste the clipboard; `link` keeps the links from copied nodes to nodes that were not copied.
    fn paste_ex(&mut self, at: Option<Pos2>, link: bool) {
        if self.clip.is_empty() {
            return;
        }
        self.snap();
        let origin = self.clip.iter().fold([f32::MAX, f32::MAX], |a, (_, n)| [a[0].min(n.pos[0]), a[1].min(n.pos[1])]);
        let target = at.unwrap_or_else(|| pos2(origin[0] + 30.0, origin[1] + 30.0));
        let mut next = self.g.nodes.keys().max().copied().unwrap_or(0) + 1;
        let mut map = HashMap::new();
        for (id, _) in &self.clip {
            map.insert(*id, next);
            next += 1;
        }
        let mut new_sel = BTreeSet::new();
        let owner = self.g.meta.get("identifier").cloned().unwrap_or_default();
        for (id, n) in self.clip.clone() {
            // a node of an .sbs package gets its own uid and its own copies of the functions its values use
            let mut nn = materialcraft::sbs_native::copy_node(&n, &owner, map[&id], &mut self.g.subgraphs);
            nn.pos = [target.x + n.pos[0] - origin[0], target.y + n.pos[1] - origin[1]];
            let outside: Vec<String> = nn.ins.iter().filter(|(_, s)| !map.contains_key(s)).map(|(k, _)| k.clone()).collect();
            for k in outside {
                if !link || !self.g.nodes.contains_key(&nn.ins[&k]) {
                    nn.ins.remove(&k);
                    nn.in_out.remove(&k);
                }
            }
            for v in nn.ins.values_mut() {
                if let Some(m) = map.get(v) {
                    *v = *m;
                }
            }
            self.g.nodes.insert(map[&id], nn);
            new_sel.insert(map[&id]);
        }
        self.active = new_sel.iter().next_back().copied();
        self.sel = new_sel;
        self.dirty = true;
    }

    fn add_comment(&mut self) {
        self.add_comment_at(None);
    }

    fn add_comment_at(&mut self, at: Option<Pos2>) {
        self.snap();
        let mut c = Comment { text: "Comment".into(), pos: [0.0, 0.0], size: [320.0, 220.0], color: 0 };
        if let Some(p) = at {
            c.pos = [p.x, p.y];
            c.size = [260.0, 130.0];
            c.text = "Comment".into();
        } else if self.sel.is_empty() {
            let p = self.view_center_world();
            c.pos = [p.x - 120.0, p.y - 90.0];
        } else {
            let (mut mn, mut mx) = ([f32::MAX; 2], [f32::MIN; 2]);
            for id in &self.sel {
                if let Some(n) = self.g.nodes.get(id) {
                    mn = [mn[0].min(n.pos[0]), mn[1].min(n.pos[1])];
                    mx = [mx[0].max(n.pos[0] + CARD), mx[1].max(n.pos[1] + BAR + card_body(node_slots(n, &self.g.subgraphs).len(), 0))];
                }
            }
            c.pos = [mn[0] - 24.0, mn[1] - 44.0];
            c.size = [mx[0] - mn[0] + 48.0, mx[1] - mn[1] + 68.0];
        }
        self.g.comments.push(c);
        self.sel_comment = Some(self.g.comments.len() - 1);
        self.sel.clear();
    }

    /// The shown graph changed (switch, open, enter or leave a subgraph): drop the previews of the old one right away
    /// instead of leaving them on screen until the new evaluation finishes.
    fn clear_views(&mut self) {
        self.view3d_node = None;
        self.maps.clear();
        self.maps_pending.clear();
        self.tex2d = None;
        self.thumbs.clear();
        self.thumb_px.clear();
        self.node_err.clear();
        self.dirty3d = true;
        self.gpu_maps_dirty = true;
        self.dirty = true;
    }

    fn enter_sub(&mut self, name: &str) {
        let Some(sub) = self.g.subgraphs.get(name).cloned() else {
            return self.status(format!("graph '{name}' not found"), false);
        };
        let mut w = sub;
        w.subgraphs = self.g.subgraphs.clone();
        w.subgraphs.remove(name);
        materialcraft::sbs_native::add_owner(&mut w.subgraphs, &mut self.g);
        w.resources = self.g.resources.clone();
        w.seed_offset = self.g.seed_offset;
        w.size = self.g.size;
        auto_layout_if_unplaced(&mut w);
        let parent = std::mem::replace(&mut self.g, w);
        self.parents.push((parent, name.to_string()));
        if !self.graph_tabs.iter().any(|t| t == name) {
            self.graph_tabs.push(name.to_string());
        }
        self.clear_views();
        self.clear_history();
        self.fit_view();
    }

    fn exit_sub(&mut self) {
        if let Some((mut parent, name)) = self.parents.pop() {
            let mut sub = std::mem::take(&mut self.g);
            materialcraft::sbs_native::adopt_package(&mut parent, &sub);
            let mut libs = std::mem::take(&mut sub.subgraphs);
            materialcraft::sbs_native::drop_owner(&mut libs, &parent);
            // splines made or edited inside the subgraph live in the package
            for (k, v) in std::mem::take(&mut sub.resources) {
                parent.resources.insert(k, v);
            }
            // the view's copies of the other graphs are the newest: graphs opened from here (a function of one of
            // its nodes) were edited in them
            for (k, v) in libs {
                parent.subgraphs.insert(k, v);
            }
            parent.subgraphs.insert(name, sub);
            self.g = parent;
            self.clear_views();
            self.clear_history();
            self.fit_view();
        }
    }

    fn goto_root(&mut self) {
        while !self.parents.is_empty() {
            self.exit_sub();
        }
    }

    fn fit_view(&mut self) {
        if self.g.nodes.is_empty() {
            self.pan = vec2(30.0, 30.0);
            self.zoom = 0.9;
            return;
        }
        let (mut mn, mut mx) = ([f32::MAX; 2], [f32::MIN; 2]);
        for n in self.g.nodes.values() {
            mn = [mn[0].min(n.pos[0]), mn[1].min(n.pos[1])];
            mx = [mx[0].max(n.pos[0] + CARD), mx[1].max(n.pos[1] + BAR + card_body(node_slots(n, &self.g.subgraphs).len(), 0))];
        }
        let c = self.canvas_rect;
        let z = ((c.width() - 80.0) / (mx[0] - mn[0]).max(1.0)).min((c.height() - 80.0) / (mx[1] - mn[1]).max(1.0)).clamp(0.3, 1.2);
        self.zoom = z;
        self.pan = vec2(40.0 - mn[0] * z, 40.0 - mn[1] * z);
    }

    fn entries(&self) -> Vec<Entry> {
        self.sd_entries()
    }

    // ------------------------------------------------------------ file operations
    fn open_file(&mut self) {
        let Some(files) = rfd::FileDialog::new().add_filter("Substance package", &["sbs"]).pick_files() else { return };
        for p in files {
            self.open_sbs(p);
        }
    }

    fn save_to(&mut self, p: PathBuf) {
        self.save_sbs(p);
    }

    fn save(&mut self) {
        match self.file.clone() {
            Some(p) => self.save_to(p),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) {
        self.save_package_as(None);
    }

    /// Save the package under a new name.
    fn save_package_as(&mut self, suggest: Option<PathBuf>) {
        let stem = suggest
            .as_ref()
            .or(self.file.as_ref())
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| self.gname());
        let mut dlg = rfd::FileDialog::new().add_filter("Substance package", &["sbs"]).set_file_name(format!("{stem}.sbs"));
        if let Some(dir) = suggest.as_ref().or(self.file.as_ref()).and_then(|p| p.parent()) {
            dlg = dlg.set_directory(dir);
        }
        if let Some(p) = dlg.save_file() {
            self.save_to(p);
        }
    }

    /// The package at `i` with the editor's unsaved edits of the graph on screen folded in (nothing is changed).
    fn package_root(&self, i: usize) -> Option<Graph> {
        if i == self.pkg_active || self.pkgs.is_empty() {
            let mut cur = self.g.clone();
            for (parent, name) in self.parents.iter().rev() {
                let mut p = parent.clone();
                let mut sub = cur;
                let mut libs = std::mem::take(&mut sub.subgraphs);
                materialcraft::sbs_native::drop_owner(&mut libs, &p);
                for (k, v) in std::mem::take(&mut sub.resources) {
                    p.resources.insert(k, v);
                }
                for (k, v) in libs {
                    p.subgraphs.insert(k, v);
                }
                p.subgraphs.insert(name.clone(), sub);
                cur = p;
            }
            Some(cur)
        } else {
            self.pkgs.get(i).and_then(|p| p.as_ref()).map(|p| p.g.clone())
        }
    }

    /// Save graph `name` of package `i` as a package of its own with the graphs and functions it needs.
    fn export_graph_file(&mut self, i: usize, name: Option<String>) {
        let Some(root) = self.package_root(i) else { return };
        let gname = name.unwrap_or_else(|| materialcraft::sbs_native::graph_name(&root));
        let text = match materialcraft::sbs_native::extract_package(&root, &gname).and_then(|g| materialcraft::sbs_native::save_text(&g)) {
            Ok(t) => t,
            Err(e) => return self.status(format!("Cannot save the graph: {e}"), false),
        };
        let stem = gname.rsplit('/').next().unwrap_or("graph").to_string();
        let Some(p) = rfd::FileDialog::new().add_filter("Substance package", &["sbs"]).set_file_name(format!("{stem}.sbs")).save_file() else { return };
        let p = if p.extension().is_none() { p.with_extension("sbs") } else { p };
        match materialcraft::safeio::write_file(&p, text.as_bytes()) {
            Ok(()) => self.status(format!("Saved graph '{gname}' as {} with the graphs it uses", p.display()), true),
            Err(e) => self.status(format!("Save failed: {e}"), false),
        }
    }

    /// Copy every graph and function of another `.sbs` package into package `i` (names that are taken get a number).
    fn add_graph_file(&mut self, i: usize) {
        let Some(p) = rfd::FileDialog::new().add_filter("Substance package", &["sbs"]).pick_file() else { return };
        let other = match materialcraft::sbs_native::open(&p) {
            Ok((g, _)) => g,
            Err(e) => return self.status(format!("Cannot read {}: {e}", p.display()), false),
        };
        self.switch_pkg(i);
        self.goto_root();
        self.snap();
        match materialcraft::sbs_native::add_package(&mut self.g, other) {
            Ok((mn, added, warnings)) => {
                self.dirty = true;
                self.modified = true;
                let more = if warnings.is_empty() { String::new() } else { format!(" ({})", warnings.join("; ")) };
                self.status(format!("Added {added} graph(s) from {}, first '{mn}'{more}", p.display()), warnings.is_empty());
            }
            Err(e) => {
                self.undo.pop();
                self.status(format!("Cannot add {}: {e}", p.display()), false);
            }
        }
    }

    /// Rename, remove or copy a graph of package `i` (as an undoable edit of that package).
    fn graph_edit(&mut self, i: usize, f: impl FnOnce(&mut Graph) -> Result<String, String>) {
        self.switch_pkg(i);
        self.goto_root();
        self.snap();
        match f(&mut self.g) {
            Ok(msg) => {
                self.graph_tabs.retain(|t| self.g.subgraphs.contains_key(t));
                self.dirty = true;
                self.status(msg, true);
            }
            Err(e) => {
                self.undo.pop();
                self.status(e, false);
            }
        }
    }

    /// Title and outputs of the graph the Export window is for (read in place, without copying the package).
    fn export_outputs(&self) -> (String, Vec<(String, String, u32)>) {
        let list = |g: &Graph| g.output_nodes().into_iter().map(|(u, i, n)| (usage_label(&u), i, n)).collect::<Vec<_>>();
        let main_name = |g: &Graph| g.meta.get("identifier").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| "Graph".into());
        let active_root = self.parents.first().map(|p| &p.0).unwrap_or(&self.g);
        let current = self.parents.last().map(|(_, n)| n.clone());
        let (i, name) = match &self.export.target {
            Some((i, n)) => (*i, n.clone()),
            None => (self.pkg_active, current.clone()),
        };
        if i == self.pkg_active || self.pkgs.is_empty() {
            match name {
                None => (main_name(active_root), list(active_root)),
                Some(n) if current.as_deref() == Some(n.as_str()) => (n.clone(), list(&self.g)),
                Some(n) => (n.clone(), active_root.subgraphs.get(&n).map(list).unwrap_or_default()),
            }
        } else {
            match self.pkgs.get(i).and_then(|p| p.as_ref()) {
                Some(p) => match name {
                    None => (main_name(&p.g), list(&p.g)),
                    Some(n) => (n.clone(), p.g.subgraphs.get(&n).map(list).unwrap_or_default()),
                },
                None => (String::new(), Vec::new()),
            }
        }
    }

    /// The graph whose maps the Export window writes (the graph on screen unless it was opened for another one).
    fn export_graph(&self) -> Result<Graph, String> {
        match &self.export.target {
            Some((i, name)) => self.package_root(*i).ok_or_else(|| "that package is no longer open".to_string())?.graph_view(name.as_deref()),
            None => self.package_root(self.pkg_active).ok_or_else(|| "nothing is open".to_string())?.graph_view(self.parents.last().map(|(_, n)| n.as_str())),
        }
    }

    fn run_mtlx_export(&mut self) {
        let Some(dir) = self.mtlx_ui.dir.clone() else {
            return self.status("Choose an export folder first.", false);
        };
        let g = match self.export_graph() {
            Ok(g) => g,
            Err(e) => return self.status(format!("Export failed: {e}"), false),
        };
        let mut o = self.mtlx_ui.opt.clone();
        o.size = [None, Some(256), Some(512), Some(1024), Some(2048), Some(4096), Some(8192)][self.mtlx_ui.size.min(6)];
        o.name = Some(self.mtlx_ui.name.trim().to_string()).filter(|n| !n.is_empty());
        match materialcraft::mtlx::export(&g, &dir, &o) {
            Ok(r) => {
                let note = r.warnings.first().map(|w| format!(" ({w})")).unwrap_or_default();
                self.status(format!("Exported {} with {} texture(s){note}", r.mtlx.display(), r.files.len()), true);
            }
            Err(e) => self.status(format!("MaterialX export failed: {e}"), false),
        }
    }

    /// The file name (without extension) the export pattern gives one output.
    fn export_file_name(&self, graph: &str, ident: &str, usage: &str, size: usize) -> String {
        let pat = self.export.pattern.trim();
        let name = if pat.is_empty() {
            format!("{}{}", self.export.prefix, ident)
        } else {
            pat.replace("$(graph)", graph).replace("$(identifier)", ident).replace("$(usage)", usage).replace("$(size)", &size.to_string())
        };
        // whatever a file name cannot hold becomes an underscore
        name.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c }).collect()
    }

    fn run_export(&mut self) {
        let Some(dir) = self.export.dir.clone() else {
            return self.status("Choose an export folder first.", false);
        };
        let g = match self.export_graph() {
            Ok(g) => g,
            Err(e) => return self.status(format!("Export failed: {e}"), false),
        };
        let sizes: Vec<usize> = [256usize, 512, 1024, 2048, 4096, 8192].iter().zip(self.export.sizes).filter(|(_, on)| *on).map(|(s, _)| *s).collect();
        if sizes.is_empty() {
            return self.status("Select at least one resolution.", false);
        }
        let outs = g.output_nodes();
        let idents: Vec<String> = outs.iter().map(|(_, i, _)| i.clone()).collect();
        let usages: HashMap<String, String> = outs.iter().map(|(u, i, _)| (i.clone(), usage_label(u))).collect();
        let gname = g.meta.get("identifier").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| "Graph".into());
        if idents.iter().all(|i| self.export.skip.contains(i)) {
            return self.status("Select at least one output to export.", false);
        }
        let fmt = self.export.fmt;
        let depth = if fmt.has_float32() && self.export.depth32 { 32 } else if fmt != Format::Jpeg && (self.export.depth16 || fmt == Format::Exr) { 16 } else { 8 };
        let mut count = 0;
        for &s in &sizes {
            match g.evaluate(Some(s)) {
                Ok(out) => {
                    let d = if sizes.len() > 1 { dir.join(s.to_string()) } else { dir.clone() };
                    if let Err(e) = std::fs::create_dir_all(&d) {
                        return self.status(format!("Export failed: {e}"), false);
                    }
                    for (name, t) in &out {
                        if self.export.skip.contains(name) {
                            continue;
                        }
                        let p = d.join(format!("{}.{}", self.export_file_name(&gname, name, usages.get(name).map(String::as_str).unwrap_or(name), s), fmt.ext()));
                        let converted;
                        let t = if materialcraft::colorspace::is_color_output(name) && self.export.space != 0 {
                            match materialcraft::colorspace::convert(t, materialcraft::colorspace::SPACES[self.export.space].0) {
                                Ok(c) => {
                                    converted = c;
                                    &converted
                                }
                                Err(e) => return self.status(format!("Colour conversion failed: {e}"), false),
                            }
                        } else {
                            t
                        };
                        if let Err(e) = materialcraft::safeio::write_file(&p, &export::encode(fmt, t, depth, self.export.quality)) {
                            return self.status(format!("Export failed: {e}"), false);
                        }
                        count += 1;
                    }
                }
                Err(e) => return self.status(format!("Export blocked: {e}"), false),
            }
        }
        self.status(format!("Exported {count} {} file(s) to {}", fmt.name(), dir.display()), true);
    }

    // ------------------------------------------------------------ background evaluation
    /// First, cheap resolution of a progressive preview: a tiny pass shows the change at once, then every pass doubles
    /// the size until the chosen preview size is reached.
    /// The size a node will have in the final result (shown under a node): its size in the pass on screen,
    /// scaled up to the graph's own size.
    fn nominal_node_size(&self, id: u32) -> usize {
        // stored already scaled to the graph's size (passes run at different sizes)
        self.node_sizes.get(&id).copied().unwrap_or(self.g.size)
    }

    /// The resolution of the material outputs as the output nodes set it (the largest one), at most 4096.
    fn target_3d_auto(&self) -> usize {
        let outs = self.g.output_nodes();
        let m = outs.iter().map(|o| self.nominal_node_size(o.2)).max().unwrap_or(self.g.size);
        m.clamp(16, 4096)
    }

    /// The size the 3D view's textures are evaluated at: the output nodes' resolution unless a fixed one is chosen.
    fn target_3d(&self) -> usize {
        if self.mesh_tex_px == 0 {
            self.target_3d_auto()
        } else {
            self.mesh_tex_px
        }
    }

    /// The size of the passes that run while a value moves: as large as keeps a pass within about 80 ms (judged by
    /// how long the last ones took per pixel), between 128 and 512 pixels. A light graph is then shown sharp while
    /// it is dragged, a heavy one stays responsive at a coarser size, and the settled passes bring it to full size.
    fn interactive_size(&self) -> usize {
        // the biggest size the last edits suggest fits in a frame budget: on a GPU that is usually the full size, so
        // the 2D view does not drop to a coarse picture and climb back up with every change
        const BUDGET_MS: f32 = 50.0;
        let full = self.preview_full();
        let cap = full.min(1024);
        if self.drag_ms_px <= 0.0 {
            return self.first_tier();
        }
        let mut s = 256usize.min(cap);
        while s * 2 <= cap && self.drag_ms_px * ((s * 2) * (s * 2)) as f32 <= BUDGET_MS {
            s *= 2;
        }
        s
    }

    fn first_tier(&self) -> usize {
        let full = self.preview_full();
        // thin lines (edges, borders) look the same on a card as in the 2D view only from about 512 pixels up
        if full > 512 {
            512
        } else {
            full
        }
    }

    /// Node whose result the 2D view shows: the selected node, or for function graphs and pattern programs the
    /// helper that runs the whole graph.
    fn view_node(&self) -> Option<u32> {
        if self.g.graph_kind() != "texture" {
            Some(materialcraft::graph::PREVIEW_ID)
        } else {
            self.active
        }
    }

    /// Run one evaluation pass in the background (see [`Pass`]). An interactive pass evaluates only the viewed node
    /// and the material outputs and leaves the 3D maps alone, so dragging a value stays responsive on a large graph;
    /// the thumbnail pass evaluates every node at a small size so every card gets its picture quickly; the final pass
    /// brings the outputs (and so the 3D view) to the full preview size.
    fn spawn_eval(&mut self, size: usize, pass: Pass) {
        let full = pass == Pass::Thumbs;
        // every pass feeds the 3D view (a drag too, at the small size it runs at), so the viewport follows the edit
        let maps = true;
        let host = self.g.preview_host();
        let own_graph = host.is_none();
        let mut g = match host {
            Some((h, _)) => h,
            None => self.g.clone(),
        };
        // what the 3D view reads: the graph outputs, or the outputs of the node it was set to show
        let outputs: Vec<(String, u32)> = match self.view3d_node.filter(|n| own_graph && g.nodes.contains_key(n)) {
            Some(nid) => view3d_taps(&mut g, nid),
            None => g.output_nodes().into_iter().map(|(u, _, id)| (u, id)).collect(),
        };
        let prev_full = self.preview_full();
        // the final pass may go above the 2D preview size: the 3D textures follow the output resolution
        let ceil = if pass == Pass::Final { prev_full.max(self.target_3d()) } else { prev_full };
        let size = size.clamp(16, ceil.max(16));
        self.job_size = size;
        self.refine_size = None;
        // a pass for the 2D view alone (another node was selected): the outputs, 3D maps and thumbnails stay as they are
        let view_only = std::mem::take(&mut self.view_only_next);
        self.job_view_only = view_only;
        self.job_pass = pass;
        self.job_started = std::time::Instant::now();
        let maps = maps && !view_only;
        let mut keep: Vec<u32> = if view_only { Vec::new() } else { outputs.iter().map(|o| o.1).collect() };
        let active = self.view_node();
        if let Some(a) = active {
            keep.push(a);
        }
        trace(&format!("start {pass:?} pass at {size}px"));
        // a newer job makes every older one stale: their loops stop within a few rows
        self.gen += 1;
        let gen = self.gen;
        tex::CURRENT_JOB.store(gen, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = channel();
        self.job = Some(rx);
        let (ptx, prx) = channel();
        self.part = Some(prx);
        let cache = self.node_cache.clone();
        // the GL viewer takes its maps packed from the device when the GPU evaluated them
        let defer_outputs = self.gpu_on && materialcraft::gpu::active();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let mut on_first = |p: &Pending| {
                if let Some(a) = active {
                    if !tex::stale(gen) {
                        if let Ok((img, _)) = view_of(p) {
                            let _ = ptx.send(Partial { gen, img2d: img, img_for: a });
                        }
                    }
                }
            };
            // the material outputs are not read back when the 3D view can take them packed from the device
            // (and the node of the 2D view, which the viewer packs on the device as well)
            let mut defer: Vec<u32> = if maps && defer_outputs { outputs.iter().map(|o| o.1).filter(|id| Some(*id) != active).collect() } else { Vec::new() };
            if defer_outputs {
                defer.extend(active);
            }
            let mut r = g.evaluate_all_cached_ex(size, &keep, &defer, gen, &cache, active, full, &mut on_first);
            let ms_eval = started.elapsed().as_secs_f32() * 1000.0;
            if tex::stale(gen) {
                return; // superseded: nobody waits for this result
            }
            tex::set_job(gen);
            // the 2D view's picture and range, from the device when the node is there
            let mut view_pic: Option<(Pic2d, Option<(f32, f32)>)> = None;
            if let Some(a) = active {
                if let Some(p) = r.pending.get(&a).cloned() {
                    if !outputs.iter().any(|o| o.1 == a) {
                        r.pending.remove(&a);
                    }
                    match view_of(&p) {
                        Ok(v) => view_pic = Some(v),
                        Err(e) => {
                            r.errors.insert(a, e);
                        }
                    }
                }
            }
            let mut dev_maps: Option<gpu3d::GpuMaps> = None;
            let mut pending: Vec<(String, Pending)> = Vec::new();
            let pending_flip = g.normal_directx();
            if !r.pending.is_empty() {
                let pend = |n: &str| outputs.iter().find(|o| o.0 == n).and_then(|o| r.pending.get(&o.1));
                let dev = |n: &str| pend(n).and_then(|p| p.device());
                // only when every deferred output really is on the device; otherwise the host code does it all
                if r.pending.values().all(|p| p.device().is_some()) {
                    let base = ["basecolor", "out", "diffuse"].into_iter().find(|n| pend(n).is_some());
                    let (rough, gloss) = (dev("roughness"), dev("glossiness"));
                    let (base_t, normal_t, metal_t, ao_t, emis_t, height_t) = (base.and_then(dev), dev("normal"), dev("metallic"), dev("ambient_occlusion"), dev("emissive"), dev("height"));
                    let opacity_t = dev("opacity");
                    dev_maps = gpu3d::prepare_dev(&DevInputs {
                        base: base_t.as_ref(),
                        normal: normal_t.as_ref(),
                        flip_normal_g: pending_flip,
                        rough: rough.as_ref().or(gloss.as_ref()),
                        rough_invert: rough.is_none() && gloss.is_some(),
                        metal: metal_t.as_ref(),
                        ao: ao_t.as_ref(),
                        emissive: emis_t.as_ref(),
                        height: height_t.as_ref(),
                        opacity: opacity_t.as_ref(),
                    });
                }
                if dev_maps.is_some() {
                    for (name, id) in &outputs {
                        if let Some(p) = r.pending.get(id) {
                            pending.push((name.clone(), p.clone()));
                        }
                    }
                    r.pending.clear();
                } else {
                    // read the outputs back and prepare them on the host
                    for (id, p) in std::mem::take(&mut r.pending) {
                        match p.tex() {
                            Ok(t) => {
                                r.kept.insert(id, t);
                            }
                            Err(e) => {
                                r.errors.insert(id, e);
                            }
                        }
                    }
                }
            }
            let named = |n: &str| outputs.iter().find(|o| o.0 == n).and_then(|o| r.kept.get(&o.1)).map(|a| a.as_ref());
            // DirectX normal maps are shown the right way up (done here once, for the GPU maps and the CPU viewers)
            let normal_gl = named("normal").and_then(|t| g.normal_for_view(t)).map(std::sync::Arc::new);
            let gpu = if dev_maps.is_some() {
                dev_maps
            } else if maps {
                let inv_gloss = named("glossiness").map(ops::invert);
                Some(gpu3d::prepare(&MapInputs {
                    base: named("basecolor").or_else(|| named("out")).or_else(|| named("diffuse")),
                    normal: normal_gl.as_deref().or_else(|| named("normal")),
                    rough: named("roughness").or(inv_gloss.as_ref()),
                    metal: named("metallic"),
                    ao: named("ambient_occlusion"),
                    emissive: named("emissive"),
                    height: named("height"),
                    opacity: named("opacity"),
                }))
            } else {
                None
            };
            let ms_prep = started.elapsed().as_secs_f32() * 1000.0 - ms_eval;
            let (img2d, range2d) = match view_pic {
                Some((img, range)) => (Some(img), range),
                None => (
                    active.and_then(|a| r.kept.get(&a)).map(|a| pic2d_of(a)),
                    active.and_then(|a| r.kept.get(&a)).map(|t| t.px.iter().fold((f32::MAX, f32::MIN), |m, p| (m.0.min(p[0]).min(p[1]).min(p[2]), m.1.max(p[0]).max(p[1]).max(p[2])))),
                ),
            };
            tex::set_job(0);
            if tex::stale(gen) {
                return;
            }
            let _ = tx.send(EvalResult {
                gen,
                thumbs: r.thumbs,
                errors: r.errors,
                kept: r.kept,
                outputs,
                sizes: r.sizes,
                img2d,
                img_for: active,
                gpu,
                normal_gl,
                pending,
                pending_flip,
                range2d,
                size,
                ms: started.elapsed().as_secs_f32() * 1000.0,
                ms_eval,
                ms_prep,
                ms_2d: started.elapsed().as_secs_f32() * 1000.0 - ms_eval - ms_prep,
                pass,
            });
        });
    }

    /// Show `pic` in the 2D view (and its opaque twin, when it has one).
    fn set_view2d(&mut self, ctx: &egui::Context, pic: Pic2d, opts: egui::TextureOptions) {
        self.tex2d_size = (pic.img.size[0], pic.img.size[1]);
        self.tex2d = Some(ctx.load_texture("view2d", pic.img, opts));
        self.tex2d_opaque = pic.opaque.map(|o| ctx.load_texture("view2d_opaque", o, opts));
    }

    /// Resolution of the editor preview: the chosen size, or the graph's own size (at most 4096).
    fn preview_full(&self) -> usize {
        if self.prev_size == 0 {
            self.g.size.clamp(16, 4096)
        } else {
            self.prev_size
        }
    }

    fn poll_eval(&mut self, ctx: &egui::Context) {
        // the selected node usually finishes long before the whole graph: show it right away
        if let Some(prx) = &self.part {
            let mut latest = None;
            while let Ok(p) = prx.try_recv() {
                latest = Some(p);
            }
            if let Some(p) = latest {
                if p.gen == self.gen && self.view_node() == Some(p.img_for) {
                    // a picture smaller than the final one is shown smooth, not as blocks, until the full size arrives
                    let crisp = p.img2d.img.size[0] >= self.preview_full();
                    let opts = egui::TextureOptions {
                        magnification: if crisp { egui::TextureFilter::Nearest } else { egui::TextureFilter::Linear },
                        minification: egui::TextureFilter::Linear,
                        wrap_mode: egui::TextureWrapMode::Repeat,
                        mipmap_mode: Some(egui::TextureFilter::Linear),
                    };
                    self.set_view2d(ctx, p.img2d, opts);
                    ctx.request_repaint();
                }
            }
        }
        let Some(rx) = &self.job else { return };
        match rx.try_recv() {
            Ok(mut r) => {
                self.job = None;
                if r.gen != self.gen {
                    return;
                }
                // how long an interactive pass costs per pixel: the size used while a value moves follows it
                if r.pass == Pass::Interactive && r.size >= 64 {
                    let per_px = r.ms / (r.size * r.size) as f32;
                    self.drag_hist.rotate_left(1);
                    self.drag_hist[3] = per_px;
                    // passes that found most of the graph in the cache look cheaper than the next edit will be: the worst
                    // of the last few is the guide
                    self.drag_ms_px = self.drag_hist.iter().cloned().fold(0.0, f32::max);
                }
                // a thumbnail pass smaller than the picture already on screen leaves the 2D view (and its refinement) alone
                let show2d = !(r.pass == Pass::Thumbs && r.size < self.pass_size);
                // progressive preview: this pass is on screen, the next (double size) one starts right away
                let full = self.preview_full();
                if r.size < full && show2d {
                    self.refine_size = Some((r.size * 2).min(full));
                    self.refine = true;
                }
                // zoomed in, every texel is a crisp square; zoomed out, a mip chain keeps it smooth. A picture that is
                // still being refined is shown smooth so the steps up to the full size do not jump between block sizes
                let crisp = r.size >= full;
                let opts = egui::TextureOptions {
                    magnification: if crisp { egui::TextureFilter::Nearest } else { egui::TextureFilter::Linear },
                    minification: egui::TextureFilter::Linear,
                    wrap_mode: egui::TextureWrapMode::Repeat,
                    mipmap_mode: Some(egui::TextureFilter::Linear),
                };
                // thumbnails come from the thumbnail pass; the other passes keep the ones on screen
                if r.pass == Pass::Thumbs {
                    let thumbs = std::mem::take(&mut r.thumbs);
                    self.thumbs.retain(|id, _| thumbs.contains_key(id));
                    self.thumb_px.retain(|id, _| thumbs.contains_key(id));
                    for (id, bytes) in thumbs {
                        // most cards are unchanged after an edit: only a thumbnail whose pixels differ is uploaded again
                        if self.thumbs.contains_key(&id) && self.thumb_px.get(&id) == Some(&bytes) {
                            continue;
                        }
                        let img = ColorImage::from_rgba_unmultiplied([THUMB, THUMB], &bytes);
                        match self.thumbs.get_mut(&id) {
                            Some(h) => h.set(img, egui::TextureOptions::LINEAR),
                            None => {
                                self.thumbs.insert(id, ctx.load_texture(format!("thumb{id}"), img, egui::TextureOptions::LINEAR));
                            }
                        }
                        self.thumb_px.insert(id, bytes);
                    }
                }
                if self.g.graph_kind() != "texture" {
                    if let Some(e) = r.errors.get(&materialcraft::graph::PREVIEW_ID) {
                        self.status(format!("{}: {e}", if self.g.graph_kind() == "function" { "function" } else { "pattern graph" }), false);
                    }
                }
                // node sizes are kept scaled to the graph's own size (the passes run at different sizes)
                let gsize = self.g.size as u64;
                let scaled: HashMap<u32, usize> = r.sizes.iter().map(|(k, v)| (*k, ((*v as u64 * gsize) / r.size.max(1) as u64).clamp(1, 8192) as usize)).collect();
                // the thumbnail pass gives the complete picture; the others only refresh the nodes they touched
                let errors = std::mem::take(&mut r.errors);
                // a problem with the whole graph (it could not be turned into nodes to run) is not any node's
                if let Some(e) = errors.get(&0).filter(|_| !self.g.nodes.contains_key(&0)) {
                    self.status(format!("This graph cannot be drawn: {e}"), false);
                }
                if r.pass == Pass::Thumbs {
                    self.node_err = errors;
                    self.node_sizes = scaled;
                } else {
                    self.node_err.extend(errors);
                    self.node_sizes.extend(scaled);
                }
                if !show2d {
                    if !self.job_view_only {
                        self.apply_maps(&mut r);
                    }
                    return;
                }
                self.pass_size = r.size;
                self.tex2d_range = if self.view_node() == r.img_for { r.range2d } else { self.view_node().and_then(|a| r.kept.get(&a)).map(|t| {
                    t.px.iter().fold((f32::MAX, f32::MIN), |m, p| (m.0.min(p[0]).min(p[1]).min(p[2]), m.1.max(p[0]).max(p[1]).max(p[2])))
                }) };
                if let Some(a) = self.view_node() {
                    if let (Some(img), true) = (r.img2d.take(), r.img_for == Some(a)) {
                        self.set_view2d(ctx, img, opts);
                    } else if let Some(t) = r.kept.get(&a) {
                        self.set_view2d(ctx, pic2d_of(t), opts);
                    } else {
                        self.tex2d = None;
                    }
                } else {
                    self.tex2d = None;
                }
                if !self.job_view_only {
                    self.apply_maps(&mut r);
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.job = None;
                self.final_due = false;
                self.status("Evaluation failed unexpectedly.", false);
            }
        }
    }

    /// The material outputs and the 3D view follow the thumbnail and final passes; a drag leaves 3D as it was.
    fn apply_maps(&mut self, r: &mut EvalResult) {
        // a thumbnail pass never replaces sharper maps from a final pass of the same graph state
        // (the size is reset to 0 whenever the graph changes, so this only keeps the picture of the current state)
        if r.pass == Pass::Thumbs && self.maps_size > r.size {
            return;
        }
        self.maps_size = r.size;
        trace(&format!("3D view gets {:?} maps at {}px (pass {:.0} ms = graph {:.0} + 3D maps {:.0} + 2D image {:.0})", r.pass, r.size, r.ms, r.ms_eval, r.ms_prep, r.ms_2d));
        self.maps.clear();
        self.maps_pending = std::mem::take(&mut r.pending);
        self.maps_pending_flip = r.pending_flip;
        for (name, id) in &r.outputs {
            if let Some(t) = r.kept.get(id) {
                // the viewers light normal maps in the OpenGL convention: a DirectX one is flipped for them
                let t = match (name.as_str(), r.normal_gl.as_ref()) {
                    ("normal", Some(f)) => f.clone(),
                    _ => t.clone(),
                };
                self.maps.insert(name.clone(), t);
            }
        }
        if self.maps_pending.is_empty() && !self.maps.contains_key("basecolor") {
            if let Some(t) = self.maps.get("out").cloned() {
                self.maps.insert("basecolor".into(), t);
            }
        }
        self.dirty3d = true;
        if let Some(m) = r.gpu.take() {
            if let Ok(mut g) = self.gpu.lock() {
                g.set_maps(m);
            }
            self.gpu_maps_dirty = false;
        } else {
            self.gpu_maps_dirty = true;
        }
    }

    /// Brings the outputs that only exist on the device (packed straight into the 3D view) to the host: needed by the
    /// CPU viewer, the path tracer, picture export and anything else that reads the pixels. A no-op when nothing is
    /// pending.
    fn ensure_maps(&mut self) {
        if self.maps_pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.maps_pending);
        let mut failed = None;
        for (name, p) in pending {
            match p.tex() {
                Ok(t) => {
                    let t = if name == "normal" && self.maps_pending_flip { std::sync::Arc::new(ops::flip_green(&t)) } else { t };
                    self.maps.insert(name, t);
                }
                Err(e) => failed = Some(e),
            }
        }
        if !self.maps.contains_key("basecolor") {
            if let Some(t) = self.maps.get("out").cloned() {
                self.maps.insert("basecolor".into(), t);
            }
        }
        if let Some(e) = failed {
            self.status(format!("Could not read a map back from the GPU: {e}"), false);
        }
    }

    /// Hand the current graph outputs to the GPU viewer (uploaded on its next paint).
    fn upload_gpu_maps(&mut self) {
        self.ensure_maps();
        self.gpu_maps_dirty = false;
        let get = |k: &str| self.maps.get(k).map(|a| a.as_ref());
        let inv_gloss = get("glossiness").map(ops::invert);
        let m = MapInputs {
            base: get("basecolor").or_else(|| get("diffuse")),
            normal: get("normal"),
            rough: get("roughness").or(inv_gloss.as_ref()),
            metal: get("metallic"),
            ao: get("ambient_occlusion"),
            emissive: get("emissive"),
            height: get("height"),
            opacity: get("opacity"),
        };
        let prepared = gpu3d::prepare(&m);
        if let Ok(mut g) = self.gpu.lock() {
            g.set_maps(prepared);
        }
    }

    fn gpu_ready(&self) -> bool {
        self.gpu_on && self.gpu.lock().map(|g| g.ready()).unwrap_or(false)
    }

    fn render_3d(&mut self, ctx: &egui::Context) {
        self.dirty3d = false;
        if self.gpu_maps_dirty {
            self.upload_gpu_maps();
        }
        if self.v3.pt_on {
            return;
        }
        if self.gpu_ready() {
            ctx.request_repaint();
            return;
        }
        self.ensure_maps();
        let fb = fallbacks(&self.maps);
        let m = build_maps(&self.maps, &fb);
        let mut cpu_view = self.view3d.clone();
        cpu_view.tess = cpu_view.tess.min(256);
        let img = render3d::render(&m, &cpu_view, self.r3d_size, self.r3d_size);
        self.tex3d = Some(ctx.load_texture("view3d", qimg(&img), egui::TextureOptions::LINEAR));
    }
}

// ---------------------------------------------------------------- free helpers
fn auto_layout(g: &mut Graph) {
    let ids: Vec<u32> = g.nodes.keys().copied().collect();
    let mut level: HashMap<u32, usize> = ids.iter().map(|i| (*i, 0)).collect();
    for _ in 0..=ids.len() {
        let mut changed = false;
        for id in &ids {
            let l = g.nodes[id].ins.values().map(|s| level.get(s).copied().unwrap_or(0) + 1).max().unwrap_or(0);
            if l > level[id] && l <= ids.len() {
                level.insert(*id, l);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut rows: HashMap<usize, f32> = HashMap::new();
    for id in &ids {
        let l = level[id];
        let y = rows.entry(l).or_insert(0.0);
        if let Some(n) = g.nodes.get_mut(id) {
            n.pos = [l as f32 * (CARD + 70.0), *y];
        }
        *y += CARD_H + 40.0;
    }
}

/// Lay the graph out when its nodes have no usable positions: all at the origin, or several sharing one spot
/// (graphs written by hand or by the library only place the nodes that were moved, e.g. migrated output nodes).
fn auto_layout_if_unplaced(g: &mut Graph) {
    if g.nodes.len() < 2 {
        return;
    }
    let mut seen: BTreeSet<(i32, i32)> = BTreeSet::new();
    let dups = g.nodes.values().filter(|n| !seen.insert(((n.pos[0] / 8.0).round() as i32, (n.pos[1] / 8.0).round() as i32))).count();
    if dups >= 1 && dups * 3 >= g.nodes.len() {
        auto_layout(g);
    }
}

fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(46, 46, 46);
    v.window_fill = Color32::from_rgb(52, 52, 52);
    v.extreme_bg_color = Color32::from_rgb(30, 30, 30);
    v.faint_bg_color = Color32::from_rgb(52, 52, 52);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(28, 28, 28));
    v.widgets.inactive.bg_fill = Color32::from_rgb(66, 66, 66);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(62, 62, 62);
    v.widgets.hovered.bg_fill = Color32::from_rgb(88, 88, 88);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(84, 84, 84);
    v.widgets.active.bg_fill = Color32::from_rgb(40, 170, 145);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(40, 170, 145);
    v.selection.bg_fill = Color32::from_rgb(35, 150, 130);
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.window_corner_radius = egui::CornerRadius::same((3.0) as u8);
    v.menu_corner_radius = egui::CornerRadius::same((3.0) as u8);
    style.visuals = v;
    style.text_styles.insert(TextStyle::Body, FontId::proportional(12.5));
    style.text_styles.insert(TextStyle::Button, FontId::proportional(12.5));
    style.text_styles.insert(TextStyle::Small, FontId::proportional(10.5));
    style.text_styles.insert(TextStyle::Heading, FontId::proportional(14.0));
    style.spacing.item_spacing = vec2(6.0, 4.0);
    style.spacing.button_padding = vec2(6.0, 2.0);
    style.spacing.slider_width = 150.0;
    // always dark: never follow the operating system's light theme (it made text unreadable on the dark panels)
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_style_of(egui::Theme::Dark, style.clone());
    ctx.set_style_of(egui::Theme::Light, style);
}

include!("gui_color.rs");
include!("gui_params.rs");
include!("gui_control.rs");
include!("gui_import.rs");
include!("gui_sbs.rs");
include!("gui_spline.rs");
include!("gui_bake.rs");
include!("gui_ui.rs");
include!("gui_home.rs");
include!("gui_icons.rs");
include!("gui_launcher.rs");
include!("gui_view3d.rs");
