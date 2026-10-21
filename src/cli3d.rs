// SPDX-License-Identifier: MIT OR Apache-2.0
//! `materialcraft render3d`: pictures and turntables of a material from the command line.
use crate::graph::Graph;
use crate::objects::CustomMesh;
use crate::render3d::{self, Maps, View, SHAPE_CUSTOM};
use crate::scene;
use crate::tex::Tex;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

pub const HELP: &str = "usage: materialcraft render3d <package.sbs[#graph]> <out.png> [options]

  --size WxH | N          picture size (default 768x768)         --tex N     resolution the material is computed at
  --mode raster|pt        software rasteriser (default) or path tracer     --spp N   samples per pixel for the path tracer (default 64)
  --shape NAME|NUMBER     sphere, cube, plane, ... (see --list)  --mesh FILE   show an OBJ / glTF / GLB / PLY / STL mesh
  --scene single|floor|lookdev|pedestal                          --channel lit|base|roughness|metallic|normal|ao|height|emissive|world_normal|uv
  --env NAME|FILE         environment preset or .hdr / .exr / .png image    --rig NAME   light rig (three_point, key_only, top, sun, none)
  --camera NAME           three_quarter, front, back, left, right, top, bottom, close_up
  --post NAME             neutral, cinematic, punchy, soft       --view FILE  load a settings file (key value lines)
  --set key=value         any view setting, repeatable, e.g. --set post.bloom=0.4 --set light.0.intensity=5 --set env.rotation=1.2
  --param \"Label=value\"   graph parameter, repeatable
  --turntable N           write N frames around the object into the folder <out> (frame_0000.png ...)
  --list                  show the names the options accept
";

fn list() {
    println!("shapes:    {}", render3d::SHAPES.iter().map(|s| s.0.to_lowercase().replace(' ', "_")).collect::<Vec<_>>().join(", "));
    println!("scenes:    {}", scene::SceneKind::ALL.iter().map(|s| s.id()).collect::<Vec<_>>().join(", "));
    println!("channels:  {}", scene::Channel::ALL.iter().map(|s| s.id()).collect::<Vec<_>>().join(", "));
    println!("environments: {}", crate::hdri::PRESETS.iter().map(|s| s.0).collect::<Vec<_>>().join(", "));
    println!("light rigs: {}", scene::RIGS.iter().map(|s| s.0).collect::<Vec<_>>().join(", "));
    println!("cameras:   {}", scene::CAMERAS.iter().map(|s| s.0).collect::<Vec<_>>().join(", "));
    println!("post looks: {}", scene::POST_PRESETS.iter().map(|s| s.0).collect::<Vec<_>>().join(", "));
    println!("tone maps: {}", scene::ToneMap::ALL.iter().map(|s| s.id()).collect::<Vec<_>>().join(", "));
    println!("{}", View::default().to_text());
}

/// The maps a graph's outputs give a 3D view, computed once (missing ones get neutral values: grey base colour,
/// flat normal, roughness 0.5, metallic 0, no occlusion).
pub struct MaterialMaps {
    outs: BTreeMap<String, Tex>,
    normal_gl: Option<Tex>,
    inv_gloss: Option<Tex>,
    flat: Tex,
    half: Tex,
    zero: Tex,
    one: Tex,
}

impl MaterialMaps {
    /// Evaluate the outputs of `g` at `tex` pixels.
    pub fn of(g: &Graph, tex: usize) -> Result<Self, String> {
        let outs = g.evaluate(Some(tex.clamp(16, 4096))).map_err(|e| e.to_string())?;
        Ok(Self::from_outputs(g, outs))
    }

    /// Use outputs that are already computed (by name: basecolor, normal, roughness, ...).
    pub fn from_outputs(g: &Graph, outs: BTreeMap<String, Tex>) -> Self {
        let inv_gloss = outs.get("glossiness").map(crate::ops::invert);
        // DirectX normal maps are lit the right way up
        let normal_gl = outs.get("normal").and_then(|t| g.normal_for_view(t));
        MaterialMaps {
            outs,
            normal_gl,
            inv_gloss,
            flat: Tex::solid(4, 4, [0.5, 0.5, 1.0, 1.0]),
            half: Tex::solid(4, 4, [0.5, 0.5, 0.5, 1.0]),
            zero: Tex::solid(4, 4, [0.0, 0.0, 0.0, 1.0]),
            one: Tex::solid(4, 4, [1.0; 4]),
        }
    }

    /// The computed output `name`, if the graph has it.
    pub fn output(&self, name: &str) -> Option<&Tex> {
        self.outs.get(name)
    }

    /// Names of the computed outputs (basecolor, normal, roughness, ...).
    pub fn names(&self) -> Vec<String> {
        self.outs.keys().cloned().collect()
    }

    pub fn maps(&self) -> Maps<'_> {
        let get = |k: &str| self.outs.get(k);
        Maps {
            base: get("basecolor").or_else(|| get("diffuse")).unwrap_or(&self.half),
            normal: self.normal_gl.as_ref().or_else(|| get("normal")).unwrap_or(&self.flat),
            rough: get("roughness").or(self.inv_gloss.as_ref()).unwrap_or(&self.half),
            metal: get("metallic").unwrap_or(&self.zero),
            ao: get("ambient_occlusion").unwrap_or(&self.one),
            emissive: get("emissive"),
            height: get("height"),
            env: None,
        }
    }
}

/// `t` shrunk by a whole `factor` (box filter). A normal map is averaged as vectors and made unit length again, so
/// fine bumps fade out instead of turning into speckle when the texture is smaller on screen than in memory.
fn box_down(t: &Tex, factor: usize, normal: bool) -> Tex {
    let (nw, nh) = ((t.w / factor).max(1), (t.h / factor).max(1));
    Tex::from_fn(nw, nh, |u, v| {
        let (x0, y0) = ((u * nw as f32) as usize * factor, (v * nh as f32) as usize * factor);
        let mut acc = [0.0f32; 4];
        let mut n = 0.0;
        for dy in 0..factor {
            for dx in 0..factor {
                let p = t.px[((y0 + dy).min(t.h - 1)) * t.w + (x0 + dx).min(t.w - 1)];
                for k in 0..4 {
                    acc[k] += if normal && k < 3 { p[k] * 2.0 - 1.0 } else { p[k] };
                }
                n += 1.0;
            }
        }
        for a in acc.iter_mut() {
            *a /= n;
        }
        if normal {
            let l = (acc[0] * acc[0] + acc[1] * acc[1] + acc[2] * acc[2]).sqrt().max(1e-4);
            [acc[0] / l * 0.5 + 0.5, acc[1] / l * 0.5 + 0.5, acc[2] / l * 0.5 + 0.5, acc[3]]
        } else {
            acc
        }
    })
}

impl MaterialMaps {
    /// The maps filtered down so one texel is about one pixel of the picture (`covers` pixels show `tiling`
    /// repeats), as a mip level would: tiling a fine normal map into a small picture does not speckle.
    pub fn prefiltered(&self, tiling: f32, covers: f32) -> Option<MaterialMaps> {
        let tex_n = self.outs.values().map(|t| t.w).max()? as f32;
        let ratio = tex_n * tiling.max(1.0) / covers.max(8.0);
        let factor = (ratio.floor() as usize).clamp(1, 16);
        if factor < 2 {
            return None;
        }
        let outs: BTreeMap<String, Tex> = self.outs.iter().map(|(k, t)| (k.clone(), box_down(t, factor.min(t.w.max(1)), k == "normal"))).collect();
        Some(MaterialMaps {
            outs,
            normal_gl: self.normal_gl.as_ref().map(|t| box_down(t, factor.min(t.w.max(1)), true)),
            inv_gloss: self.inv_gloss.as_ref().map(|t| box_down(t, factor.min(t.w.max(1)), false)),
            flat: self.flat.clone(),
            half: self.half.clone(),
            zero: self.zero.clone(),
            one: self.one.clone(),
        })
    }
}

/// One picture of the material: the software rasteriser, or the path tracer with `spp` samples per pixel. Maps that
/// are finer than the picture can show are filtered down first, and displacement gets a mesh as fine as the
/// texture asks for (up to the limit), so sharp height steps do not turn into zipper edges.
pub fn render_picture(mm: &MaterialMaps, v: &View, w: usize, h: usize, path_traced: bool, spp: u32) -> Tex {
    let mut view = v.clone();
    if view.relief == render3d::RELIEF_DISPLACE && view.height_scale > 0.0 {
        if let Some(t) = mm.outs.get("height") {
            let want = ((t.w as f32 * view.tiling.max(1.0) * 0.75 / 64.0).ceil() as u32 * 64).clamp(view.tess, 768);
            view.tess = want;
        }
    }
    let v = &view;
    let reduced = mm.prefiltered(v.tiling, w.min(h) as f32 * 0.7);
    let mm = reduced.as_ref().unwrap_or(mm);
    let maps = mm.maps();
    if path_traced {
        crate::pt::render(&maps, v, w, h, spp.clamp(1, 65536))
    } else {
        render3d::render(&maps, v, w, h)
    }
}

/// Every name the view settings accept, as `(what, names)`.
pub fn option_lists() -> Vec<(&'static str, Vec<String>)> {
    vec![
        ("shapes", render3d::SHAPES.iter().map(|s| s.0.to_lowercase().replace(' ', "_")).collect()),
        ("scenes", scene::SceneKind::ALL.iter().map(|s| s.id().to_string()).collect()),
        ("channels", scene::Channel::ALL.iter().map(|s| s.id().to_string()).collect()),
        ("environments", crate::hdri::PRESETS.iter().map(|s| s.0.to_string()).collect()),
        ("light_rigs", scene::RIGS.iter().map(|s| s.0.to_string()).collect()),
        ("cameras", scene::CAMERAS.iter().map(|s| s.0.to_string()).collect()),
        ("post_looks", scene::POST_PRESETS.iter().map(|s| s.0.to_string()).collect()),
        ("tone_maps", scene::ToneMap::ALL.iter().map(|s| s.id().to_string()).collect()),
        ("relief", vec!["off".into(), "bump".into(), "parallax".into(), "displace".into()]),
    ]
}

pub fn run(args: &[String], load: &dyn Fn(&str) -> Result<Graph, String>) -> Result<(), String> {
    run_inner(args, load, false)
}

/// Like [`run`] but never writes to standard output (the MCP server uses standard output for its protocol).
pub fn run_quiet(args: &[String], load: &dyn Fn(&str) -> Result<Graph, String>) -> Result<(), String> {
    run_inner(args, load, true)
}

fn run_inner(args: &[String], load: &dyn Fn(&str) -> Result<Graph, String>, quiet: bool) -> Result<(), String> {
    if quiet && (args.iter().any(|a| a == "--list") || args.len() < 2 || args[0].starts_with("--")) {
        return Err("usage: <graph> <out.png> [options]".into());
    }
    if args.iter().any(|a| a == "--list") {
        list();
        return Ok(());
    }
    if args.len() < 2 || args[0].starts_with("--") {
        print!("{HELP}");
        return Err("usage: materialcraft render3d <graph> <out.png> [options]".into());
    }
    let mut g = load(&args[0])?;
    let out = args[1].clone();
    let mut view = View { tess: 256, ..View::default() };
    let (mut w, mut h) = (768usize, 768usize);
    let (mut tex_size, mut mode, mut spp, mut turn) = (None::<usize>, "raster".to_string(), 64u32, 0u32);
    let mut params: BTreeMap<String, f32> = BTreeMap::new();
    let mut i = 2;
    let next = |i: &mut usize, what: &str| -> Result<String, String> {
        *i += 1;
        args.get(*i).cloned().ok_or_else(|| format!("{what} needs a value"))
    };
    // the settings file first, so the command line wins
    let mut sets: Vec<(String, String)> = Vec::new();
    while i < args.len() {
        match args[i].as_str() {
            "--size" => {
                let v = next(&mut i, "--size")?;
                let (a, b) = v.split_once('x').unwrap_or((&v, &v));
                w = a.parse().map_err(|_| format!("bad size '{v}'"))?;
                h = b.parse().map_err(|_| format!("bad size '{v}'"))?;
                if !(16..=8192).contains(&w) || !(16..=8192).contains(&h) {
                    return Err("the picture size must be between 16 and 8192".into());
                }
            }
            "--tex" => tex_size = Some(next(&mut i, "--tex")?.parse().map_err(|_| "--tex needs a number")?),
            "--mode" => {
                mode = next(&mut i, "--mode")?;
                if mode != "raster" && mode != "pt" {
                    return Err("--mode must be raster or pt".into());
                }
            }
            "--spp" => spp = next(&mut i, "--spp")?.parse().ok().filter(|n| (1..=65536).contains(n)).ok_or("--spp needs 1..65536")?,
            "--turntable" => turn = next(&mut i, "--turntable")?.parse().ok().filter(|n| (1..=3600).contains(n)).ok_or("--turntable needs 1..3600")?,
            "--shape" => sets.push(("shape".into(), next(&mut i, "--shape")?)),
            "--scene" => sets.push(("scene".into(), next(&mut i, "--scene")?)),
            "--channel" => sets.push(("channel".into(), next(&mut i, "--channel")?)),
            "--rig" => sets.push(("rig".into(), next(&mut i, "--rig")?)),
            "--camera" => sets.push(("camera".into(), next(&mut i, "--camera")?)),
            "--env" => {
                let v = next(&mut i, "--env")?;
                if crate::hdri::PRESETS.iter().any(|p| p.0 == v) {
                    sets.push(("env.preset".into(), v));
                } else {
                    sets.push(("env.file".into(), v));
                    sets.push(("env.background".into(), "environment".into()));
                }
            }
            "--post" => {
                let v = next(&mut i, "--post")?;
                if !scene::POST_PRESETS.iter().any(|p| p.0 == v) {
                    return Err(format!("unknown post look '{v}' ({})", scene::POST_PRESETS.iter().map(|p| p.0).collect::<Vec<_>>().join(", ")));
                }
                view.post = scene::post_preset(&v);
            }
            "--view" => {
                let p = next(&mut i, "--view")?;
                let text = std::fs::read_to_string(&p).map_err(|e| format!("{p}: {e}"))?;
                view.apply_text(&text).map_err(|e| format!("{p}: {e}"))?;
            }
            "--mesh" => {
                let p = next(&mut i, "--mesh")?;
                let cm = CustomMesh::load(Path::new(&p)).map_err(|e| format!("{p}: {e}"))?;
                view.custom = Some(Arc::new(cm));
                view.shape = SHAPE_CUSTOM;
            }
            "--set" => {
                let kv = next(&mut i, "--set")?;
                let (k, v) = kv.split_once('=').ok_or("--set must look like key=value")?;
                sets.push((k.to_string(), v.to_string()));
            }
            "--param" => {
                let kv = next(&mut i, "--param")?;
                let (k, v) = kv.rsplit_once('=').ok_or("--param must look like \"Label=value\"")?;
                params.insert(k.to_string(), v.parse().map_err(|_| format!("bad value in '{kv}'"))?);
            }
            o => return Err(format!("unknown option '{o}' (try --help)")),
        }
        i += 1;
    }
    for (k, v) in &sets {
        if k == "env.file" && !Path::new(v).exists() {
            return Err(format!("{v}: no such environment image (presets: {})", crate::hdri::PRESETS.iter().map(|p| p.0).collect::<Vec<_>>().join(", ")));
        }
        view.set(k, v)?;
    }
    if let Some(f) = &view.env.file {
        crate::hdri::load_env("studio", Some(f)).map_err(|e| format!("{f}: {e}"))?;
    }
    for (k, v) in &params {
        crate::sbs_native::set_input(&mut g, k, Some(&[*v]), None, None)?;
    }
    let mm = MaterialMaps::of(&g, tex_size.unwrap_or(g.size.min(1024)))?;
    let maps = mm.maps();
    let frames = turn.max(1);
    if turn > 0 {
        std::fs::create_dir_all(&out).map_err(|e| format!("{out}: {e}"))?;
    }
    for f in 0..frames {
        let mut v = view.clone();
        if turn > 0 {
            v.yaw += std::f32::consts::TAU * f as f32 / turn as f32;
        }
        let img = if mode == "pt" { crate::pt::render(&maps, &v, w, h, spp) } else { render3d::render(&maps, &v, w, h) };
        let path = if turn > 0 { Path::new(&out).join(format!("frame_{f:04}.png")) } else { Path::new(&out).to_path_buf() };
        crate::safeio::write_file(&path, &crate::png::encode(img.w, img.h, &img.px, 8)).map_err(|e| format!("{}: {e}", path.display()))?;
        if !quiet {
            println!("wrote {}", path.display());
        }
    }
    Ok(())
}
