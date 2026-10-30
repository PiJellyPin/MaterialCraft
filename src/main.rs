// SPDX-License-Identifier: MIT OR Apache-2.0
use materialcraft::bake::{self, BakeSettings};
use materialcraft::graph::Graph;
use materialcraft::export::{self, Format};
use materialcraft::render3d::{self, Maps, View};
use materialcraft::{ops, png, tex::Tex};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

const HELP: &str = "materialcraft - procedural material authoring (pure Rust)

USAGE:
Files: MaterialCraft works on Substance packages (.sbs): one file holds every graph and function of a project, and
it is saved with only what was changed rewritten. Wherever a graph is read, `package.sbs#name` picks one graph of a
package (the first graph without it).

  materialcraft render <package.sbs[#name]> <outdir> [--size N | --sizes 256,512] [--depth 8|16|32]
                  [--param \"name=value\"]... [--variations N] [--preview] [--render3d]
                  [--format png|jpg|tiff|exr|hdr] [--quality 1-100] [--only basecolor,normal]
                  [--colorspace srgb|p3|wide|romm|rec2020|linear]
  materialcraft export-mtlx <package.sbs[#name]> <outdir> [--size N] [--shader standard|openpbr] [--normal directx|opengl]
                  [--depth 8|16] [--no-displacement] [--displacement-scale F] [--bake-ao] [--tiling X,Y] [--name N]
                  MaterialX (1.39) material for Unreal Engine 5.8 and other tools: a .mtlx file plus its textures;
                  `export-mtlx calibration <outdir>` writes a test material (colour chart with an arrow, roughness ramp,
                  metal half, raised dome) to check UV orientation, colour space and normal direction in the engine
  materialcraft render3d <package.sbs[#name]> <out.png> [options]   3D picture / turntable (raster or path traced), see: materialcraft render3d --help
  materialcraft review  <package.sbs[#name]> <out.png> [--reference image] [--json out.json]   review sheet: 3D preview, maps,
                  node graph with thumbnails, comparison with a reference picture (numbers and hints), the graph as text
  materialcraft params  <package.sbs[#name]>   list the graph's parameters (render --param \"name=value\" sets them)
  materialcraft graphs  <package.sbs> [--export <graph> <out.sbs>]   list the graphs of a package, or save one (with what it uses) as a package
  materialcraft check   <package.sbs>
  materialcraft nodes [words]                       list the nodes graphs can use
  materialcraft bake    --low <mesh>... --high <mesh>... --out <dir> [options]    mesh map baker, see: materialcraft bake --help
  materialcraft example <outdir>                    write a small example package (brick_wall.sbs)
  materialcraft testmesh <outdir>                   write a demo low/high-poly OBJ pair
  materialcraft mcp [--graph f.sbs] [--bridge]      MCP server for AI assistants (standard input / output); --bridge drives the open editor
  materialcraft serve [--graph f.sbs] [--port N]    keep a package in memory and accept commands on a local port (token protected)
  materialcraft exec [options] <command> [name=value ...]   run one command (render_node, node_add, batch, ...); see: materialcraft exec --help
  materialcraft commands [words]                    list all commands with their parameters

The visual editor is a separate binary:  cargo run --release --features gui --bin materialcraft-gui
";

fn write_png(path: &Path, t: &Tex, depth: u8) -> Result<(), String> {
    materialcraft::safeio::write_file(path, &png::encode(t.w, t.h, &t.px, depth)).map_err(|e| format!("{}: {e}", path.display()))
}

fn mtlx_cli(args: &[String]) -> Result<(), String> {
    use materialcraft::mtlx::{self, NormalY, Shader};
    let gp = args.first().ok_or("usage: materialcraft export-mtlx <package.sbs[#name]|calibration> <outdir> [options]")?;
    let dir = args.get(1).ok_or("missing <outdir>")?;
    let g = if gp == "calibration" { None } else { Some(load(gp)?) };
    let mut o = mtlx::Options::default();
    let mut i = 2;
    while i < args.len() {
        let flag = args[i].clone();
        let mut val = |what: &str| -> Result<String, String> {
            i += 1;
            args.get(i).cloned().ok_or(format!("{flag} needs {what}"))
        };
        match flag.as_str() {
            "--size" => o.size = Some(val("a number")?.parse().map_err(|_| "--size needs a number")?),
            "--shader" => o.shader = Shader::parse(&val("standard or openpbr")?).ok_or("--shader must be standard or openpbr")?,
            "--normal" => o.normal = NormalY::parse(&val("directx or opengl")?).ok_or("--normal must be directx or opengl")?,
            "--depth" => o.depth = val("8 or 16")?.parse().map_err(|_| "--depth must be 8 or 16")?,
            "--no-displacement" => o.displacement = false,
            "--displacement-scale" => o.displacement_scale = val("a number")?.parse().map_err(|_| "--displacement-scale needs a number")?,
            "--bake-ao" => o.bake_ao = true,
            "--tiling" => {
                let v = val("X,Y")?;
                let (a, b) = v.split_once(',').ok_or("--tiling needs X,Y")?;
                o.tiling = [a.trim().parse().map_err(|_| "bad --tiling")?, b.trim().parse().map_err(|_| "bad --tiling")?];
            }
            "--name" => o.name = Some(val("a name")?),
            x => return Err(format!("unknown option '{x}'")),
        }
        i += 1;
    }
    let r = match &g {
        Some(g) => mtlx::export(g, Path::new(dir), &o)?,
        None => mtlx::calibration(Path::new(dir), &o)?,
    };
    for f in &r.files {
        println!("wrote {}", f.display());
    }
    println!("wrote {}", r.mtlx.display());
    for w in &r.warnings {
        println!("note: {w}");
    }
    Ok(())
}

fn load(path: &str) -> Result<Graph, String> {
    // `package.sbs#graph` picks one graph of a package (its first graph without it)
    if !Path::new(path).exists() {
        if let Some((file, name)) = path.rsplit_once('#').filter(|(f, n)| !n.is_empty() && Path::new(f).is_file()) {
            let pkg = load(file)?;
            if materialcraft::sbs_native::graph_name(&pkg) == name {
                return Ok(pkg);
            }
            return pkg.graph_view(Some(name)).map_err(|e| format!("{file}: {e}"));
        }
    }
    if !path.to_ascii_lowercase().ends_with(".sbs") {
        return Err(format!("{path}: only .sbs packages can be read"));
    }
    // linked files are looked for next to the package too
    if let Some(dir) = Path::new(path).parent().filter(|p| !p.as_os_str().is_empty()) {
        materialcraft::resources::set_search_dirs(vec![dir.to_path_buf(), dir.join("resources")]);
    }
    // the package is read as it is; it renders through the same nodes the editor shows
    let (g, warnings) = materialcraft::sbs_native::open(Path::new(path))?;
    for w in &warnings {
        eprintln!("warning: {w}");
    }
    Ok(g)
}

const BAKE_HELP: &str = "usage: materialcraft bake --low <mesh>... [--high <mesh>...] [--cage <mesh>] --out <dir> [options]
       materialcraft bake <low.obj> <high.obj> <outdir> [options]        (short form)
       materialcraft bake --set <file.bakeset> --out <dir>

Meshes: OBJ, PLY, STL, glTF, GLB. Repeat --low / --high for several files. Without --high the low-poly mesh is baked
against itself (ambient occlusion, curvature and so on of one mesh).

Options:
  --preset preview|game|final   starting point (512 / 2048 / 4096 px)
  --size N | WxH                output size           --aa N          supersampling grid (1, 2, 3, 4, 8)
  --padding N                   texels of padding     --maps a,b,c    normal,world_normal,position,ambient_occlusion,
  --front F --rear F            ray distances (fraction of the high-poly diagonal)  bent_normal,world_bent_normal,
  --dist F                      front = rear = F                                   curvature,thickness,height,object_id,
  --ao-rays N --ao-dist F       occlusion quality / reach                          material_id,group_id,vertex_color,
  --directx                     green channel down                                 opacity,world_direction,uv_layout
  --no-match                    do not match objects by name
  --device auto|cpu|gpu         where rays are traced (default auto: the GPU when there is one)
  --depth 8|16                  PNG bit depth (default 16)
  --set-opt key=value           any setting of a bake set (see a .bakeset file)
  --save-set <file>             write the settings and mesh list as a bake set";

fn bake_cli(args: &[String]) -> Result<(), String> {
    use materialcraft::bake::{BakeSet, Progress};
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{BAKE_HELP}");
        return Ok(());
    }
    let mut set = BakeSet::default();
    let (mut out, mut depth, mut save) = (None::<String>, 16u8, None::<String>);
    let mut overrides: Vec<(String, String)> = Vec::new();
    let mut preset: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    let need = |i: usize| -> Result<&String, String> { args.get(i + 1).ok_or_else(|| format!("{} needs a value", args[i])) };
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--low" => set.low.push(need(i)?.clone()),
            "--high" => set.high.push(need(i)?.clone()),
            "--cage" => set.cage = Some(need(i)?.clone()),
            "--out" => out = Some(need(i)?.clone()),
            "--depth" => depth = need(i)?.parse().map_err(|_| "bad --depth")?,
            "--preset" => preset = Some(need(i)?.clone()),
            "--set" => {
                let f = need(i)?;
                let text = fs::read_to_string(f).map_err(|e| format!("{f}: {e}"))?;
                set = BakeSet::parse("cli", &text).map_err(|e| format!("{f}: {e}"))?;
            }
            "--save-set" => save = Some(need(i)?.clone()),
            "--size" => overrides.push(("size".into(), need(i)?.replace('x', " "))),
            "--aa" | "--padding" | "--front" | "--rear" | "--maps" | "--device" => overrides.push((a[2..].into(), need(i)?.clone())),
            "--dist" => {
                overrides.push(("front".into(), need(i)?.clone()));
                overrides.push(("rear".into(), need(i)?.clone()));
            }
            "--ao-rays" => overrides.push(("ao_rays".into(), need(i)?.clone())),
            "--ao-dist" => overrides.push(("ao_dist".into(), need(i)?.clone())),
            "--set-opt" => {
                let kv = need(i)?;
                let (k, v) = kv.split_once('=').ok_or("--set-opt needs key=value")?;
                overrides.push((k.into(), v.into()));
            }
            "--directx" => {
                overrides.push(("flip_y".into(), "1".into()));
                i += 1;
                continue;
            }
            "--no-match" => {
                overrides.push(("match_by_name".into(), "0".into()));
                i += 1;
                continue;
            }
            x if x.starts_with("--") => return Err(format!("unknown option '{x}' (try: materialcraft bake --help)")),
            x => {
                positional.push(x.to_string());
                i += 1;
                continue;
            }
        }
        i += 2;
    }
    if positional.len() == 3 && set.low.is_empty() {
        set.low.push(positional[0].clone());
        set.high.push(positional[1].clone());
        out = Some(positional[2].clone());
    } else if !positional.is_empty() {
        return Err(format!("unexpected argument '{}' (try: materialcraft bake --help)", positional[0]));
    }
    if let Some(p) = preset {
        let base = match p.as_str() {
            "preview" => BakeSettings::preview(),
            "game" => BakeSettings::game(),
            "final" => BakeSettings::final_quality(),
            o => return Err(format!("unknown preset '{o}' (preview, game, final)")),
        };
        set.settings = BakeSettings { maps: set.settings.maps.clone(), ..base };
    }
    for (k, v) in &overrides {
        set.settings.set(k, v)?;
    }
    if let Some(f) = save {
        materialcraft::safeio::write_file(Path::new(&f), set.to_text().as_bytes()).map_err(|e| format!("{f}: {e}"))?;
        println!("wrote {f}");
    }
    if set.low.is_empty() {
        return Err("no low-poly mesh given (--low <file>)".into());
    }
    let out = out.ok_or("no output folder given (--out <dir>)")?;
    println!("baking {} map(s) at {}x{} px, {}x{} samples per texel ...", set.settings.maps.len(), set.settings.width, set.settings.height, set.settings.aa, set.settings.aa);
    let prog = Progress::new();
    let res = std::thread::scope(|sc| {
        let h = sc.spawn(|| bake::run_set(&set, Path::new("."), Some(&prog)));
        let mut last = String::new();
        while !h.is_finished() {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let line = format!("{} {:.0}%", prog.stage(), prog.fraction() * 100.0);
            if line != last {
                eprint!("\r{line:<60}");
                last = line;
            }
        }
        eprintln!("\r{:<60}", "");
        h.join().unwrap_or_else(|_| Err("the baker stopped unexpectedly".into()))
    })?;
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    for (name, t) in &res.maps {
        let p = Path::new(&out).join(format!("{name}.png"));
        write_png(&p, t, depth)?;
        println!("wrote {}", p.display());
    }
    let r = &res.report;
    println!(
        "{} low-poly and {} high-poly triangles, {:.1}% of the texels used, {} rays ({:.1}% of the samples hit), {:.1} s on {}",
        r.low_tris,
        r.high_tris,
        100.0 * r.covered as f32 / r.texels.max(1) as f32,
        r.rays,
        100.0 * r.hits as f32 / r.samples.max(1) as f32,
        r.seconds,
        r.device
    );
    for w in &r.warnings {
        eprintln!("warning: {w}");
    }
    Ok(())
}

/// A small brick wall material as an .sbs package (`materialcraft example`), built with the package commands.
fn example_package() -> Result<String, String> {
    use materialcraft::json::Json;
    let mut s = materialcraft::session::Session::from_graph(materialcraft::sbs_native::new_package("brick_wall", 512), None);
    let steps = Json::parse(
        r#"[
        {"command":"node_add","params":{"kind":"sbs:lib:pattern_tile_generator/tile_generator","name":"bricks","params":{"x_amount":4,"y_amount":8,"position_offset":0.5,"luminance_random":0.25}}},
        {"command":"node_add","params":{"kind":"sbs:lib:noise_perlin_noise/perlin_noise","name":"grain","params":{"scale":16}}},
        {"command":"node_add","params":{"kind":"sbs:blend","name":"height","params":{"blendingmode":"Multiply","opacitymult":0.35},"inputs":{"source":"grain","destination":"bricks"}}},
        {"command":"node_add","params":{"kind":"sbs:uniform","name":"clay","params":{"outputcolor":[0.55,0.22,0.14,1]}}},
        {"command":"node_add","params":{"kind":"sbs:blend","name":"color","params":{"blendingmode":"Multiply"},"inputs":{"source":"height","destination":"clay"}}},
        {"command":"node_add","params":{"kind":"sbs:normal","name":"nrm","inputs":{"input1":"height"}}},
        {"command":"output_add","params":{"node":"color","usage":"basecolor"}},
        {"command":"output_add","params":{"node":"nrm","usage":"normal"}},
        {"command":"output_add","params":{"node":"height","usage":"height"}},
        {"command":"graph_layout","params":{}}
    ]"#,
    )
    .map_err(|e| e.to_string())?;
    s.execute("batch", &Json::obj().with("steps", steps))?;
    let text = s.execute("graph.text", &Json::obj())?;
    text.data.get("text").and_then(Json::as_str).map(str::to_string).ok_or_else(|| "the example has no text".into())
}

/// `materialcraft review`: the review sheet of a graph (3D preview, maps, node graph, optional reference) as a PNG,
/// and its numbers and outline as JSON.
fn review_cli(args: &[String]) -> Result<(), String> {
    use materialcraft::json::Json;
    let usage = "usage: materialcraft review <package.sbs[#graph]> <out.png> [--reference image] [--compare auto|render|basecolor] [--size N] [--tex N] [--json out.json] [--set key=value]...";
    let (gp, out) = (args.first().ok_or(usage)?, args.get(1).ok_or(usage)?);
    let g = load(gp)?;
    let mut p = Json::obj();
    let mut settings = Json::obj();
    let mut json_out: Option<String> = None;
    let mut i = 2;
    while i < args.len() {
        let next = |i: usize| args.get(i + 1).cloned().ok_or_else(|| format!("{} needs a value", args[i]));
        match args[i].as_str() {
            "--reference" => p.set("reference", next(i)?),
            "--compare" => p.set("compare", next(i)?),
            "--size" => p.set("size", next(i)?.parse::<f64>().map_err(|_| "--size needs a number")?),
            "--tex" => p.set("tex", next(i)?.parse::<f64>().map_err(|_| "--tex needs a number")?),
            "--json" => json_out = Some(next(i)?),
            "--set" => {
                let kv = next(i)?;
                let (k, v) = kv.split_once('=').ok_or("--set must look like key=value")?;
                settings.set(k, v);
            }
            "--no-thumbnails" => {
                p.set("thumbnails", false);
                i += 1;
                continue;
            }
            o => return Err(format!("unknown option '{o}'\n{usage}")),
        }
        i += 2;
    }
    p.set("settings", settings);
    let mut s = materialcraft::session::Session::from_graph(g, Some(std::path::PathBuf::from(gp.split('#').next().unwrap_or(gp))));
    let o = s.execute("render.review", &p)?;
    let png = o.images.first().map(|im| im.png.clone()).ok_or("the review made no picture")?;
    materialcraft::safeio::write_file(Path::new(out), &png).map_err(|e| format!("{out}: {e}"))?;
    let text = o.data.to_text();
    match json_out {
        Some(j) => {
            materialcraft::safeio::write_file(Path::new(&j), text.as_bytes()).map_err(|e| format!("{j}: {e}"))?;
            println!("wrote {out} and {j}");
        }
        None => {
            println!("{text}");
            eprintln!("wrote {out}");
        }
    }
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(|s| s.as_str()) {
        Some("nodes") => {
            use materialcraft::sbs_defs::{palette, Fam};
            use std::io::Write as _;
            let words: Vec<String> = args[1..].iter().map(|w| w.to_lowercase()).collect();
            let mut out = std::io::stdout().lock();
            for (fam, name) in [(Fam::Texture, "graph"), (Fam::Function, "function"), (Fam::Pattern, "fx-map")] {
                for d in palette(fam) {
                    let text = format!("{} {} {}", d.kind, d.label, d.category).to_lowercase();
                    if !words.iter().all(|w| text.contains(w.as_str())) {
                        continue;
                    }
                    // a closed pipe (`| head`) ends the listing quietly
                    if writeln!(out, "{:<56} {:<9} {:<28} {}", d.kind, name, d.category, d.label).is_err() {
                        return Ok(());
                    }
                }
            }
            Ok(())
        }
        Some("check") => {
            let g = load(args.get(1).ok_or("usage: materialcraft check <package.sbs>")?)?;
            let graphs = materialcraft::sbs_native::all_graphs(&g).values().filter(|x| materialcraft::sbs_native::listed(x)).count();
            let low = materialcraft::sbs_native::lower(&g)?;
            println!("OK: {graphs} graph(s), {} nodes, {} outputs, {}x{}; rendering: {}", g.nodes.len(), g.output_nodes().len(), g.size, if g.is_square() { g.size } else { g.height }, low.report.summary());
            Ok(())
        }
        Some("params") => {
            let g = load(args.get(1).ok_or("usage: materialcraft params <package.sbs[#name]>")?)?;
            for (id, label, ty, def) in materialcraft::sbs_native::value_inputs(&g) {
                println!("\"{label}\"  input {id}  {}  default {def}", ty.code());
            }
            Ok(())
        }
        Some("graphs") => {
            let usage = "usage: materialcraft graphs <package.sbs> [--export <graph> <out.sbs>]";
            let path = args.get(1).ok_or(usage)?;
            let (pkg, _) = materialcraft::sbs_native::open(Path::new(path))?;
            if let Some(i) = args.iter().position(|a| a == "--export") {
                let (name, out) = (args.get(i + 1).ok_or(usage)?, args.get(i + 2).ok_or(usage)?);
                let g = materialcraft::sbs_native::extract_package(&pkg, name)?;
                let n = g.subgraphs.values().filter(|x| materialcraft::sbs_native::listed(x)).count();
                materialcraft::safeio::write_file(Path::new(out), materialcraft::sbs_native::save_text(&g)?.as_bytes()).map_err(|e| format!("{out}: {e}"))?;
                println!("wrote {out} ({n} graph(s) and function(s) it uses)");
                return Ok(());
            }
            for (k, g) in materialcraft::sbs_native::all_graphs(&pkg) {
                let Some(sd) = g.sd.as_ref() else { continue };
                let kind = match sd.role {
                    materialcraft::sbs_native::Role::Graph => "graph",
                    materialcraft::sbs_native::Role::Function => "function",
                    _ => continue,
                };
                let outs: Vec<String> = sd.outputs.iter().filter_map(|o| o.v("identifier")).collect();
                let users = materialcraft::sbs_native::graph_users(&pkg, &k);
                println!("{k:<32} {kind:<8} outputs: {}{}", outs.join(", "), if users.is_empty() { String::new() } else { format!("   used by: {}", users.join(", ")) });
            }
            Ok(())
        }
        Some("example") => {
            let dir = args.get(1).ok_or("usage: materialcraft example <outdir>")?;
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            let p = Path::new(dir).join("brick_wall.sbs");
            materialcraft::safeio::write_file(&p, example_package()?.as_bytes()).map_err(|e| format!("{}: {e}", p.display()))?;
            println!("wrote {}", p.display());
            Ok(())
        }
        Some("testmesh") => {
            let dir = args.get(1).ok_or("usage: materialcraft testmesh <outdir>")?;
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            fs::write(Path::new(dir).join("low.obj"), bake::uv_sphere_obj(24, 16, 1.0, 0.0)).map_err(|e| e.to_string())?;
            fs::write(Path::new(dir).join("high.obj"), bake::uv_sphere_obj(160, 100, 1.0, 0.04)).map_err(|e| e.to_string())?;
            println!("wrote {dir}/low.obj and {dir}/high.obj");
            Ok(())
        }
        Some(c @ ("mcp" | "serve" | "exec" | "commands")) => materialcraft::agent_cli::run(c, &args[1..]),
        Some("bake") => bake_cli(&args[1..]),
        Some("render3d") => materialcraft::cli3d::run(&args[1..], &load),
        Some("review") => review_cli(&args[1..]),
        Some("export-mtlx") => mtlx_cli(&args[1..]),
        Some("render") => {
            let gp = args.get(1).ok_or("usage: materialcraft render <graph> <outdir> [options]")?;
            let dir = args.get(2).ok_or("missing <outdir>")?;
            let mut g = load(gp)?;
            let (mut sizes, mut depth, mut preview, mut r3d, mut variations) = (vec![g.size], 8u8, false, false, 1u32);
            #[allow(unused_mut, unused_variables)]
            let mut space = String::from("srgb");
            let mut params: BTreeMap<String, f32> = BTreeMap::new();
            let (mut fmt, mut quality, mut only): (Format, u8, Option<Vec<String>>) = (Format::Png, 92, None);
            let mut i = 3;
            while i < args.len() {
                match args[i].as_str() {
                    "--size" => {
                        i += 1;
                        sizes = vec![args.get(i).and_then(|s| s.parse().ok()).ok_or("--size needs a number")?];
                    }
                    "--sizes" => {
                        i += 1;
                        let v = args.get(i).ok_or("--sizes needs a list")?;
                        sizes = v.split(',').map(|s| s.trim().parse::<usize>().map_err(|_| format!("bad size '{s}'"))).collect::<Result<_, _>>()?;
                    }
                    "--depth" => {
                        i += 1;
                        depth = match args.get(i).map(|s| s.as_str()) {
                            Some("8") => 8,
                            Some("16") => 16,
                            Some("32") => 32,
                            _ => return Err("--depth must be 8, 16 or 32".into()),
                        };
                    }
                    "--param" => {
                        i += 1;
                        let kv = args.get(i).ok_or("--param needs Label=value")?;
                        let (k, v) = kv.rsplit_once('=').ok_or("--param must look like \"Label=value\"")?;
                        params.insert(k.to_string(), v.parse().map_err(|_| format!("bad value in '{kv}'"))?);
                    }
                    "--variations" => {
                        i += 1;
                        variations = args.get(i).and_then(|s| s.parse().ok()).filter(|n| (1..=1000).contains(n)).ok_or("--variations needs 1..1000")?;
                    }
                    "--format" => {
                        i += 1;
                        fmt = Format::parse(args.get(i).ok_or("--format needs png, jpg, tiff, exr or hdr")?).ok_or("--format must be png, jpg, tiff, exr or hdr")?;
                    }
                    "--quality" => {
                        i += 1;
                        quality = args.get(i).and_then(|s| s.parse().ok()).filter(|q| (1..=100).contains(q)).ok_or("--quality needs 1..100")?;
                    }
                    "--only" => {
                        i += 1;
                        only = Some(args.get(i).ok_or("--only needs a comma-separated list")?.split(',').map(|s| s.trim().to_string()).collect());
                    }
                    "--colorspace" => {
                        i += 1;
                        let v = args.get(i).ok_or("--colorspace needs a name")?;
                        #[cfg(any(feature = "cms", feature = "craft"))]
                        {
                            if materialcraft::colorspace::find(v).is_none() {
                                return Err(format!("--colorspace must be one of: {}", materialcraft::colorspace::SPACES.iter().map(|s| s.0).collect::<Vec<_>>().join(", ")));
                            }
                            space = v.to_string();
                        }
                        #[cfg(not(any(feature = "cms", feature = "craft")))]
                        return Err(format!("--colorspace {v}: this build has no colour management (build with --features cms)"));
                    }
                    "--preview" => preview = true,
                    "--render3d" => r3d = true,
                    o => return Err(format!("unknown option '{o}'")),
                }
                i += 1;
            }
            // the graph's inputs (by identifier or label) start at the given values
            for (k, v) in &params {
                materialcraft::sbs_native::set_input(&mut g, k, Some(&[*v]), None, None)?;
            }
            let base_seed = g.seed_offset;
            let multi = sizes.len() > 1;
            for var in 0..variations {
                g.seed_offset = base_seed.wrapping_add(var * 101);
                for &s in &sizes {
                    let out = g.evaluate(Some(s)).map_err(|e| e.to_string())?;
                    let mut d = Path::new(dir).to_path_buf();
                    if variations > 1 {
                        d = d.join(format!("variation_{var:03}"));
                    }
                    if multi {
                        d = d.join(format!("{s}"));
                    }
                    fs::create_dir_all(&d).map_err(|e| e.to_string())?;
                    for (name, t) in &out {
                        if only.as_ref().map(|o| !o.contains(name)).unwrap_or(false) {
                            continue;
                        }
                        let p = d.join(format!("{name}.{}", fmt.ext()));
                        #[cfg(any(feature = "cms", feature = "craft"))]
                        let conv;
                        #[cfg(any(feature = "cms", feature = "craft"))]
                        let t = if materialcraft::colorspace::is_color_output(name) {
                            conv = materialcraft::colorspace::convert(t, &space)?;
                            &conv
                        } else {
                            t
                        };
                        materialcraft::safeio::write_file(&p, &export::encode(fmt, t, if fmt == Format::Jpeg { 8 } else { depth }, quality)).map_err(|e| format!("{}: {e}", p.display()))?;
                        println!("wrote {}", p.display());
                    }
                    let flat = Tex::solid(s, s, [0.5, 0.5, 1.0, 1.0]);
                    let one = Tex::solid(s, s, [1.0; 4]);
                    let zero = Tex::solid(s, s, [0.0, 0.0, 0.0, 1.0]);
                    let half = Tex::solid(s, s, [0.5, 0.5, 0.5, 1.0]);
                    // DirectX normal maps are lit the right way up in the previews
                    let normal_gl = out.get("normal").and_then(|t| g.normal_for_view(t));
                    let (b, n, r, m, a) = (
                        out.get("basecolor").unwrap_or(&half),
                        normal_gl.as_ref().or_else(|| out.get("normal")).unwrap_or(&flat),
                        out.get("roughness").unwrap_or(&half),
                        out.get("metallic").unwrap_or(&zero),
                        out.get("ambient_occlusion").unwrap_or(&one),
                    );
                    if preview {
                        let pv = ops::lit_preview(b, n, r, m, a, [0.5, 0.5, 0.7]);
                        write_png(&d.join("preview.png"), &pv, 8)?;
                        println!("wrote {}", d.join("preview.png").display());
                    }
                    if r3d {
                        let maps = Maps { base: b, normal: n, rough: r, metal: m, ao: a, emissive: out.get("emissive"), height: out.get("height"), opacity: out.get("opacity"), env: None };
                        for (label, id) in render3d::SHAPES.iter() {
                            let name: String = label.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
                            let v = View { shape: *id, ..View::default() };
                            let img = render3d::render(&maps, &v, 512, 512);
                            write_png(&d.join(format!("render_{name}.png")), &img, 8)?;
                            println!("wrote {}", d.join(format!("render_{name}.png")).display());
                        }
                    }
                }
            }
            Ok(())
        }
        _ => {
            print!("{HELP}");
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
