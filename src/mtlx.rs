// SPDX-License-Identifier: MIT OR Apache-2.0
//! MaterialX export (spec 1.39, the version Unreal Engine 5.8 reads): a graph's material outputs are rendered to
//! image files and a `.mtlx` document ties them to a surface shader (Autodesk Standard Surface or OpenPBR Surface).
//!
//! What goes into the file follows what Unreal's importer accepts (Epic's MaterialX documentation for 5.8):
//! * one `surfacematerial` linked to the surface shader, textures read with `tiledimage` nodes in a node graph;
//! * colour maps carry `colorspace="srgb_texture"` (the importer decides sRGB from that name only), data maps stay
//!   in the document's linear space;
//! * the normal map goes through a `normalmap` node, height through a `displacement` node;
//! * file names are relative to the `.mtlx` file and safe on every platform.
//!
//! The document is built by [`document`] (pure text, no files); [`export`] evaluates a graph and writes everything.

use crate::export::{encode, Format};
use crate::graph::Graph;
use crate::tex::Tex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The surface shader the material is written for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shader {
    /// Autodesk Standard Surface: read by Unreal with and without Substrate.
    StandardSurface,
    /// OpenPBR Surface: read by Unreal with Substrate (without it the Standard Surface functions are used).
    OpenPbr,
}

impl Shader {
    pub fn parse(s: &str) -> Option<Shader> {
        match s.to_ascii_lowercase().replace(['-', '_', ' '], "").as_str() {
            "standardsurface" | "standard" => Some(Shader::StandardSurface),
            "openpbr" | "openpbrsurface" => Some(Shader::OpenPbr),
            _ => None,
        }
    }
}

/// Which way the green channel of the written normal map points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalY {
    /// Green points down (Unreal, DirectX, Substance Designer's default).
    DirectX,
    /// Green points up (MaterialX, Blender, Unity, USD).
    OpenGl,
}

impl NormalY {
    pub fn parse(s: &str) -> Option<NormalY> {
        match s.to_ascii_lowercase().as_str() {
            "directx" | "dx" => Some(NormalY::DirectX),
            "opengl" | "gl" | "ogl" => Some(NormalY::OpenGl),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub shader: Shader,
    pub normal: NormalY,
    /// 8 or 16 bits per channel in the PNG files.
    pub depth: u8,
    /// Edge length of the written maps; `None` keeps the graph's own size.
    pub size: Option<usize>,
    /// Write the height map and a `displacement` node.
    pub displacement: bool,
    /// Scale of the `displacement` node.
    pub displacement_scale: f32,
    /// Multiply ambient occlusion into the base colour (Standard Surface has no occlusion input).
    pub bake_ao: bool,
    /// Repeat of the textures (`uvtiling`).
    pub tiling: [f32; 2],
    /// Material name; `None` uses the graph's identifier.
    pub name: Option<String>,
    /// Folder (inside the output folder) the images are written to.
    pub texture_dir: String,
}

impl Default for Options {
    fn default() -> Options {
        Options { shader: Shader::StandardSurface, normal: NormalY::DirectX, depth: 8, size: None, displacement: true, displacement_scale: 1.0, bake_ao: false, tiling: [1.0, 1.0], name: None, texture_dir: "textures".into() }
    }
}

/// The maps a material can carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Map {
    BaseColor,
    Roughness,
    Metallic,
    Normal,
    Height,
    AmbientOcclusion,
    Opacity,
    Emissive,
}

impl Map {
    pub fn suffix(self) -> &'static str {
        match self {
            Map::BaseColor => "basecolor",
            Map::Roughness => "roughness",
            Map::Metallic => "metallic",
            Map::Normal => "normal",
            Map::Height => "height",
            Map::AmbientOcclusion => "ambientocclusion",
            Map::Opacity => "opacity",
            Map::Emissive => "emissive",
        }
    }
}

/// What [`export`] wrote.
#[derive(Debug, Default)]
pub struct Exported {
    pub mtlx: PathBuf,
    pub files: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

/// A name that is valid in MaterialX documents and on every file system.
pub fn safe_name(s: &str) -> String {
    let mut o: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    while o.contains("__") {
        o = o.replace("__", "_");
    }
    let o = o.trim_matches('_').to_string();
    if o.is_empty() {
        "Material".into()
    } else if o.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        format!("M_{o}")
    } else {
        o
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn num(v: f32) -> String {
    let s = format!("{v}");
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
        s
    } else {
        format!("{s}.0")
    }
}

/// The `.mtlx` text for a material whose maps are the given files (relative to the document).
pub fn document(name: &str, maps: &BTreeMap<Map, String>, opt: &Options) -> String {
    let name = safe_name(name);
    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\"?>\n");
    x.push_str("<materialx version=\"1.39\" colorspace=\"lin_rec709\">\n");
    x.push_str("  <!-- written by MaterialCraft; textures are relative to this file -->\n");
    let ng = format!("NG_{name}");
    x.push_str(&format!("  <nodegraph name=\"{ng}\">\n"));
    x.push_str("    <texcoord name=\"uv0\" type=\"vector2\">\n      <input name=\"index\" type=\"integer\" value=\"0\" />\n    </texcoord>\n");
    let tiling = format!("{}, {}", num(opt.tiling[0]), num(opt.tiling[1]));
    let mut outs: Vec<(String, &str, String)> = Vec::new();
    let image = |x: &mut String, m: Map, ty: &str| {
        let Some(f) = maps.get(&m) else { return };
        let id = format!("{}_img", m.suffix());
        x.push_str(&format!("    <tiledimage name=\"{id}\" type=\"{ty}\">\n"));
        let cs = if matches!(m, Map::BaseColor | Map::Emissive) { " colorspace=\"srgb_texture\"" } else { "" };
        x.push_str(&format!("      <input name=\"file\" type=\"filename\" value=\"{}\"{cs} />\n", esc(f)));
        x.push_str("      <input name=\"texcoord\" type=\"vector2\" nodename=\"uv0\" />\n");
        x.push_str(&format!("      <input name=\"uvtiling\" type=\"vector2\" value=\"{tiling}\" />\n"));
        x.push_str("    </tiledimage>\n");
    };
    image(&mut x, Map::BaseColor, "color3");
    image(&mut x, Map::Roughness, "float");
    image(&mut x, Map::Metallic, "float");
    image(&mut x, Map::Normal, "vector3");
    image(&mut x, Map::Height, "float");
    image(&mut x, Map::AmbientOcclusion, "float");
    image(&mut x, Map::Opacity, "float");
    image(&mut x, Map::Emissive, "color3");
    if maps.contains_key(&Map::Normal) {
        x.push_str("    <normalmap name=\"normal_map\" type=\"vector3\">\n      <input name=\"in\" type=\"vector3\" nodename=\"normal_img\" />\n      <input name=\"scale\" type=\"float\" value=\"1.0\" />\n    </normalmap>\n");
    }
    let openpbr = opt.shader == Shader::OpenPbr;
    if maps.contains_key(&Map::Opacity) && !openpbr {
        x.push_str("    <convert name=\"opacity_color\" type=\"color3\">\n      <input name=\"in\" type=\"float\" nodename=\"opacity_img\" />\n    </convert>\n");
    }
    let mut add = |n: &str, ty: &str, node: &str| outs.push((n.to_string(), ty_static(ty), node.to_string()));
    if maps.contains_key(&Map::BaseColor) {
        add("base_color_out", "color3", "basecolor_img");
    }
    if maps.contains_key(&Map::Roughness) {
        add("roughness_out", "float", "roughness_img");
    }
    if maps.contains_key(&Map::Metallic) {
        add("metallic_out", "float", "metallic_img");
    }
    if maps.contains_key(&Map::Normal) {
        add("normal_out", "vector3", "normal_map");
    }
    if maps.contains_key(&Map::Height) {
        add("height_out", "float", "height_img");
    }
    if maps.contains_key(&Map::AmbientOcclusion) {
        add("ambient_occlusion_out", "float", "ambientocclusion_img");
    }
    if maps.contains_key(&Map::Opacity) {
        add("opacity_out", if openpbr { "float" } else { "color3" }, if openpbr { "opacity_img" } else { "opacity_color" });
    }
    if maps.contains_key(&Map::Emissive) {
        add("emission_out", "color3", "emissive_img");
    }
    for (n, ty, node) in &outs {
        x.push_str(&format!("    <output name=\"{n}\" type=\"{ty}\" nodename=\"{node}\" />\n"));
    }
    x.push_str("  </nodegraph>\n");

    let sr = format!("SR_{name}");
    let link = |inp: &str, ty: &str, out: &str| format!("    <input name=\"{inp}\" type=\"{ty}\" nodegraph=\"{ng}\" output=\"{out}\" />\n");
    if !openpbr {
        x.push_str(&format!("  <standard_surface name=\"{sr}\" type=\"surfaceshader\">\n"));
        x.push_str("    <input name=\"base\" type=\"float\" value=\"1.0\" />\n");
        if maps.contains_key(&Map::BaseColor) {
            x.push_str(&link("base_color", "color3", "base_color_out"));
        }
        if maps.contains_key(&Map::Roughness) {
            x.push_str(&link("specular_roughness", "float", "roughness_out"));
        }
        x.push_str(&format!("    <input name=\"metalness\" type=\"float\" {} />\n", if maps.contains_key(&Map::Metallic) { format!("nodegraph=\"{ng}\" output=\"metallic_out\"") } else { "value=\"0.0\"".into() }));
        if maps.contains_key(&Map::Normal) {
            x.push_str(&link("normal", "vector3", "normal_out"));
        }
        if maps.contains_key(&Map::Emissive) {
            x.push_str("    <input name=\"emission\" type=\"float\" value=\"1.0\" />\n");
            x.push_str(&link("emission_color", "color3", "emission_out"));
        }
        if maps.contains_key(&Map::Opacity) {
            x.push_str(&link("opacity", "color3", "opacity_out"));
        }
        x.push_str("  </standard_surface>\n");
    } else {
        x.push_str(&format!("  <open_pbr_surface name=\"{sr}\" type=\"surfaceshader\">\n"));
        x.push_str("    <input name=\"base_weight\" type=\"float\" value=\"1.0\" />\n");
        if maps.contains_key(&Map::BaseColor) {
            x.push_str(&link("base_color", "color3", "base_color_out"));
        }
        if maps.contains_key(&Map::Roughness) {
            x.push_str(&link("specular_roughness", "float", "roughness_out"));
        }
        x.push_str(&format!("    <input name=\"base_metalness\" type=\"float\" {} />\n", if maps.contains_key(&Map::Metallic) { format!("nodegraph=\"{ng}\" output=\"metallic_out\"") } else { "value=\"0.0\"".into() }));
        if maps.contains_key(&Map::Normal) {
            x.push_str(&link("geometry_normal", "vector3", "normal_out"));
        }
        if maps.contains_key(&Map::Emissive) {
            x.push_str("    <input name=\"emission_luminance\" type=\"float\" value=\"1.0\" />\n");
            x.push_str(&link("emission_color", "color3", "emission_out"));
        }
        if maps.contains_key(&Map::Opacity) {
            x.push_str(&link("geometry_opacity", "float", "opacity_out"));
        }
        x.push_str("  </open_pbr_surface>\n");
    }
    let disp = opt.displacement && maps.contains_key(&Map::Height);
    if disp {
        x.push_str(&format!("  <displacement name=\"DISP_{name}\" type=\"displacementshader\">\n"));
        x.push_str(&link("displacement", "float", "height_out"));
        x.push_str(&format!("    <input name=\"scale\" type=\"float\" value=\"{}\" />\n", num(opt.displacement_scale)));
        x.push_str("  </displacement>\n");
    }
    x.push_str(&format!("  <surfacematerial name=\"M_{name}\" type=\"material\">\n    <input name=\"surfaceshader\" type=\"surfaceshader\" nodename=\"{sr}\" />\n"));
    if disp {
        x.push_str(&format!("    <input name=\"displacementshader\" type=\"displacementshader\" nodename=\"DISP_{name}\" />\n"));
    }
    x.push_str("  </surfacematerial>\n</materialx>\n");
    x
}

fn ty_static(t: &str) -> &'static str {
    match t {
        "color3" => "color3",
        "vector3" => "vector3",
        _ => "float",
    }
}

fn gray(t: &Tex) -> Tex {
    t.map(|p| {
        let v = (p[0] + p[1] + p[2]) / 3.0;
        [v, v, v, 1.0]
    })
}

fn opaque(t: &Tex) -> Tex {
    t.map(|p| [p[0], p[1], p[2], 1.0])
}

/// The maps of an evaluated graph by role, converted to what the material needs (notes go to `warnings`).
pub fn collect(g: &Graph, out: &BTreeMap<String, Tex>, opt: &Options, warnings: &mut Vec<String>) -> BTreeMap<Map, Tex> {
    let mut by_usage: BTreeMap<String, &Tex> = BTreeMap::new();
    for (usage, ident, _) in g.output_nodes() {
        if let Some(t) = out.get(&ident).or_else(|| out.get(&usage)) {
            by_usage.entry(usage).or_insert(t);
        }
    }
    for (k, t) in out {
        by_usage.entry(k.clone()).or_insert(t);
    }
    let mut m: BTreeMap<Map, Tex> = BTreeMap::new();
    if let Some(t) = by_usage.get("basecolor") {
        m.insert(Map::BaseColor, opaque(t));
    } else if let Some(t) = by_usage.get("diffuse") {
        m.insert(Map::BaseColor, opaque(t));
        warnings.push("no base color output: the diffuse output is used as the base color (specular / glossiness inputs are approximated)".into());
    }
    if let Some(t) = by_usage.get("roughness") {
        m.insert(Map::Roughness, gray(t));
    } else if let Some(t) = by_usage.get("glossiness") {
        m.insert(Map::Roughness, gray(&t.map(|p| [1.0 - p[0], 1.0 - p[1], 1.0 - p[2], 1.0])));
        warnings.push("no roughness output: roughness is made from the glossiness output (1 - glossiness)".into());
    } else {
        warnings.push("the graph has no roughness output: the material keeps the shader's default roughness".into());
    }
    if let Some(t) = by_usage.get("metallic") {
        m.insert(Map::Metallic, gray(t));
    }
    if let Some(t) = by_usage.get("normal") {
        // the graph's own convention against the one asked for
        let flip = g.normal_directx() != (opt.normal == NormalY::DirectX);
        m.insert(Map::Normal, if flip { crate::ops::flip_green(&opaque(t)) } else { opaque(t) });
    } else {
        warnings.push("the graph has no normal output".into());
    }
    if let Some(t) = by_usage.get("height") {
        if opt.displacement {
            m.insert(Map::Height, gray(t));
        }
    }
    if let Some(t) = by_usage.get("ambient_occlusion") {
        if opt.bake_ao {
            if let Some(b) = m.get_mut(&Map::BaseColor) {
                let ao = gray(t);
                for (p, a) in b.px.iter_mut().zip(&ao.px) {
                    for c in 0..3 {
                        p[c] *= a[0];
                    }
                }
            }
        } else {
            m.insert(Map::AmbientOcclusion, gray(t));
            warnings.push("ambient occlusion is written as its own texture: Standard Surface and OpenPBR have no occlusion input (use --bake-ao to multiply it into the base color)".into());
        }
    }
    if let Some(t) = by_usage.get("opacity") {
        m.insert(Map::Opacity, gray(t));
    }
    if let Some(t) = by_usage.get("emissive") {
        if t.px.iter().any(|p| p[0] > 0.0005 || p[1] > 0.0005 || p[2] > 0.0005) {
            m.insert(Map::Emissive, opaque(t));
        }
    }
    if m.is_empty() {
        warnings.push("the graph has no material outputs to export".into());
    }
    m
}

/// Write a set of maps and the document that ties them together (`<dir>/<name>.mtlx`, textures in `<dir>/<texture_dir>/`).
pub fn write_material(dir: &Path, name: &str, tex: &BTreeMap<Map, Tex>, opt: &Options, mut warnings: Vec<String>) -> Result<Exported, String> {
    if opt.depth != 8 && opt.depth != 16 {
        return Err("depth must be 8 or 16".into());
    }
    if opt.texture_dir.is_empty() || opt.texture_dir.contains(['/', '\\', ':']) || opt.texture_dir.starts_with('.') {
        return Err("the texture folder must be a plain folder name".into());
    }
    if tex.is_empty() {
        return Err("the graph has no material outputs to export".into());
    }
    let name = safe_name(name);
    let mut res = Exported { warnings: std::mem::take(&mut warnings), ..Exported::default() };
    let tdir = dir.join(&opt.texture_dir);
    std::fs::create_dir_all(&tdir).map_err(|e| format!("{}: {e}", tdir.display()))?;
    let mut files: BTreeMap<Map, String> = BTreeMap::new();
    for (m, t) in tex {
        let file = format!("{name}_{}.png", m.suffix());
        let p = tdir.join(&file);
        crate::safeio::write_file(&p, &encode(Format::Png, t, opt.depth, 100)).map_err(|e| format!("{}: {e}", p.display()))?;
        files.insert(*m, format!("{}/{}", opt.texture_dir, file));
        res.files.push(p);
    }
    let text = document(&name, &files, opt);
    let p = dir.join(format!("{name}.mtlx"));
    crate::safeio::write_file(&p, text.as_bytes()).map_err(|e| format!("{}: {e}", p.display()))?;
    res.mtlx = p;
    Ok(res)
}

/// Render the graph and write `<dir>/<name>.mtlx` with its textures in `<dir>/<texture_dir>/`.
pub fn export(g: &Graph, dir: &Path, opt: &Options) -> Result<Exported, String> {
    let size = opt.size.unwrap_or(g.size);
    if !(16..=8192).contains(&size) {
        return Err(format!("size {size} is out of range (16..8192)"));
    }
    if opt.depth != 8 && opt.depth != 16 {
        return Err("depth must be 8 or 16".into());
    }
    let out = g.evaluate(Some(size)).map_err(|e| e.to_string())?;
    let mut warnings = Vec::new();
    let tex = collect(g, &out, opt, &mut warnings);
    let ident = g.meta.get("identifier").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| "Material".into());
    write_material(dir, opt.name.as_deref().unwrap_or(&ident), &tex, opt, warnings)
}

/// A test material for checking an import in the engine: a colour chart that shows flips and rotations of the UVs
/// (red top-left, green top-right, blue bottom-left, yellow bottom-right, an arrow in the white square), a roughness
/// ramp from smooth (left) to rough (right), a metallic half (bottom) and a raised dome in the normal map. Lit from
/// above, a correctly read normal map shows the dome as a bump (bright on its upper side); if it looks like a dent
/// the green channel is read the wrong way round (use the other `normal` option).
pub fn calibration(dir: &Path, opt: &Options) -> Result<Exported, String> {
    let n = opt.size.unwrap_or(512).clamp(64, 4096);
    let dome = |u: f32, v: f32| {
        let (x, y) = (u - 0.5, v - 0.5);
        (-(x * x + y * y) * 28.0).exp()
    };
    let base = Tex::from_fn(n, n, |u, v| {
        let (cx, cy) = ((u * 8.0) as i32, (v * 8.0) as i32);
        let mut c = if (cx + cy) % 2 == 0 { [0.82, 0.82, 0.82] } else { [0.18, 0.18, 0.18] };
        let quad = match (u < 0.5, v < 0.5) {
            (true, true) => [0.85, 0.12, 0.1],
            (false, true) => [0.1, 0.7, 0.15],
            (true, false) => [0.1, 0.2, 0.85],
            (false, false) => [0.9, 0.8, 0.1],
        };
        // coloured bands along the outer edge of every quadrant, the checker inside
        let edge = u.min(1.0 - u).min(v.min(1.0 - v));
        if edge < 0.06 || (u - 0.5).abs() < 0.015 || (v - 0.5).abs() < 0.015 {
            c = quad;
        }
        // an arrow pointing up (towards v = 0) in the middle: a shaft and a head
        let (x, y) = (u - 0.5, v - 0.5);
        let shaft = x.abs() < 0.03 && y > -0.1 && y < 0.14;
        let head = y <= -0.1 && y > -0.2 && x.abs() < (y + 0.2) * 0.9;
        if shaft || head {
            c = [1.0, 1.0, 1.0];
        }
        [c[0], c[1], c[2], 1.0]
    });
    let rough = Tex::from_fn(n, n, |u, _| [u, u, u, 1.0]);
    let metal = Tex::from_fn(n, n, |_, v| {
        let m = if v > 0.5 { 1.0 } else { 0.0 };
        [m, m, m, 1.0]
    });
    let height = Tex::from_fn(n, n, |u, v| {
        let h = dome(u, v);
        [h, h, h, 1.0]
    });
    // the dome's normal map in the convention asked for (a bump: normals tilt away from the centre)
    let normal = crate::ops::normal_from_height(&height, 6.0, opt.normal == NormalY::DirectX);
    let mut tex = BTreeMap::new();
    tex.insert(Map::BaseColor, base);
    tex.insert(Map::Roughness, rough);
    tex.insert(Map::Metallic, metal);
    tex.insert(Map::Normal, normal);
    if opt.displacement {
        tex.insert(Map::Height, height);
    }
    let mut o = opt.clone();
    o.size = Some(n);
    write_material(dir, o.name.as_deref().unwrap_or("MaterialCraft_Calibration"), &tex, &o, vec!["calibration material written: import it in the engine and compare with the description in `materialcraft export-mtlx --help`".into()])
}
