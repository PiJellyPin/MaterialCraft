
### Donate For Claude Max Subscription to support development (all funds go to claude max subscription)
Donate Here: [Donation Button](https://www.paypal.com/donate/?hosted_button_id=DYD94FKUZ9H7G).

our discord is here: if you want to submit features or got any cool ideas! https://discord.gg/9KVCNatNvX

### Examples:
<img width="657" height="527" alt="image" src="https://github.com/user-attachments/assets/41096864-94b1-4831-9c0f-fac0b45f7dcb" />
<img width="647" height="524" alt="image" src="https://github.com/user-attachments/assets/06a9e1bf-66f4-4a50-bc0c-33257d334d0d" />
<img width="651" height="531" alt="image" src="https://github.com/user-attachments/assets/f41d3786-15ba-4205-9371-06089cf96587" />
<img width="660" height="529" alt="image" src="https://github.com/user-attachments/assets/d92aed6e-ef2e-40e5-a1da-a1341e2d1a4e" />
<img width="651" height="525" alt="image" src="https://github.com/user-attachments/assets/9eb1d9fd-df27-4576-a908-bc81c07bf571" />

# MaterialCraft
Procedural, node-based material authoring in pure Rust (Windows / Linux / macOS).

## Download
Every push to `main` is built, installed and launched on all three platforms, then published as the rolling
[`latest` prerelease](../../releases/tag/latest). It is a development build, replaced on every push.

| Platform | File | Notes |
|---|---|---|
| Windows 10/11 x64 | `MaterialCraft_<version>_x64-windows.zip` | Unzip and run `materialcraft-gui.exe`. Unsigned, so SmartScreen may ask once: More info > Run anyway. |
| macOS 11+ (Apple Silicon and Intel) | `MaterialCraft_<version>_universal-macOS.dmg` | Drag MaterialCraft to Applications. Not notarised: the first time, right-click the app > Open (or System Settings > Privacy & Security > Open Anyway). The `materialcraft` CLI is next to the app in the dmg. |
| Debian / Ubuntu | `MaterialCraft_<version>_x86_64-linux.deb` | `sudo apt install ./MaterialCraft_<version>_x86_64-linux.deb` |
| Fedora / RHEL / openSUSE | `MaterialCraft_<version>_x86_64-linux.rpm` | `sudo dnf install ./MaterialCraft_<version>_x86_64-linux.rpm` |
| Any Linux | `MaterialCraft_<version>_x86_64-linux.AppImage` | `chmod +x` and run. Needs FUSE (`fusermount`); without it, run with `--appimage-extract-and-run`. |
| Any Linux | `MaterialCraft_<version>_x86_64-linux.tar.gz` | Both binaries, no install. Needs the X11/Wayland, xkbcommon and EGL/GL libraries every desktop already has. |

Linux builds need glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 and later).
`SHA256SUMS` lists every file's checksum, and each file carries a build provenance attestation:
`gh attestation verify <file> -R PiJellyPin/MaterialCraft`.

## Build & run
    cargo run --release --features gui --bin materialcraft-gui     # visual editor
    cargo run --release -- render wall.sbs out --size 1024 --render3d   # CLI
    cargo run --release --features gpu -- render wall.sbs out --size 2048   # CLI with GPU compute for nodes and baking (falls back to the CPU automatically; bake takes --device auto|cpu|gpu)

Requires a current stable Rust toolchain (https://rustup.rs). On Windows the MSVC build tools are needed.
The core library has zero dependencies; the editor uses egui/eframe + rfd (native file dialogs).

## Files: Substance packages (.sbs)
MaterialCraft opens, edits and saves `.sbs` packages as they are. One package holds every graph and function of a project
(no separate graph or package files); the Explorer lists them, and right-clicking one opens it, exports its maps, saves it as a
package of its own with what it uses, duplicates, renames or deletes it. The node palette offers the format's own nodes (the
atomic nodes and the library graphs MaterialCraft renders), values and graph parameters are stored the way the format stores them,
and saving rewrites only what was changed: a package that was opened and saved without changes is byte-for-byte the same file.
Values can be computed by function graphs, parameters can be exposed (the P button) and their defaults, labels and ranges set
in the graph's settings, and a graph can run any other graph of the package that does not use it.
`.sbs` is MaterialCraft's only file format: the older MaterialCraft files (`.mcg`, `.mcgp`, `.mca`) are no longer read or written.

## Editor
Start screen (recent files, New graph dialog with templates) -> editor with Explorer, Library, Graph, 3D View, 2D View and
Properties panels. The Library is a thumbnail grid of the format's own nodes (27 atomic nodes and 339 library graphs, each listed
once, with a picture of what it makes), sorted into Generators, Filters and Material tools; function and FX-Map nodes appear when
a function or pattern graph is open. Panels can be closed/reopened from the Windows menu.

Tab = add node at cursor · drag a link into empty space = add + auto-connect · Ctrl+Z/Y undo/redo ·
Ctrl+C/X/V/D · rubber-band & Shift/Ctrl multi-select · C = comment frame · double-click a graph node to open its graph ·
F = frame all · Alt/Space/Middle-drag = pan · wheel = zoom.

## CLI
    materialcraft render <package.sbs[#graph]> <outdir> [--sizes 512,1024] [--depth 16] [--param "name=0.5"] [--variations 8] [--render3d]
    materialcraft graphs <package.sbs> [--export <graph> <out.sbs>]   # list the graphs, or save one with what it uses as a package
    materialcraft check | params <package.sbs[#graph]>
    materialcraft nodes [words]                       # the nodes graphs can use
    materialcraft bake --low <mesh>... [--high <mesh>...] [--cage <mesh>] --out <dir> [--preset game] [--size 2048] [--aa 2] [--maps normal,ao,curvature]   # see: materialcraft bake --help
    materialcraft bake <low> <high> <outdir>                      # short form; meshes can be OBJ, glTF / GLB, PLY or STL
    materialcraft review <package.sbs[#graph]> <out.png> [--reference photo.jpg] [--json out.json]   # review sheet: 3D preview, maps, node graph, comparison
    materialcraft example <outdir>                    # a small example package (brick_wall.sbs)
    materialcraft testmesh <outdir>                   # a demo low / high-poly OBJ pair for the baker

## AI control

Assistants and scripts can build and edit graphs through an MCP server, a command line and a live editor bridge. They can check
their own work with a review sheet (3D preview, maps, node graph with thumbnails, the graph as text, a comparison with a
reference picture with hints on what to change), write math as text (turned into function graphs), insert ready-made groups
of standard nodes (grow / shrink masks, stars, strokes, custom tiles) and move nodes into subgraphs. See [AI_CONTROL.md](AI_CONTROL.md).

## Platforms
Windows, Linux and macOS use the same code: `eframe` (glow / OpenGL 3.3 core) for the window and the 3D viewport, `rfd` for
file dialogs, plus the shared Craft libraries (`craft-ui` docking, `craft-settings`, `craft-atomic`, `craft-cms`, `craft-quit`, from
https://github.com/storytold/craft-libs, MIT OR Apache-2.0). The core library and the command-line tool still use only the Rust standard
library unless built with `--features cms` or `--features gui`. Settings live in `config.toml` under `%APPDATA%\materialcraft`, `~/.config/materialcraft` or
`~/Library/Application Support/materialcraft`. Building needs Rust 1.95 or newer. GitHub Actions builds and tests all three (`.github/workflows/ci.yml`); `release.yml` packages every push to `main` and installs and launches each package before `publish.yml` releases it.
Linux needs the usual X11/Wayland/GL development packages (`libxkbcommon-dev libwayland-dev libx11-dev libgl1-mesa-dev`).

## Licence
Dual licensed under [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option. See [NOTICE](NOTICE),
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md) and [INDEPENDENT_IMPLEMENTATION.md](INDEPENDENT_IMPLEMENTATION.md)
(how we keep the code original). MaterialCraft is not affiliated with any other software vendor.

## Graph language
Besides graphs, a package holds **function graphs** (the format's 84 function nodes: math, vectors, logic, variables, random,
image sampling, calls). Any value of a node can be computed by a function graph of its own (the fn button), the Pixel Processor
and Value Processor run function graphs, and an FX-Map node draws its pattern graph (Quadrant, Iterate and Switch nodes).

## 3D viewer and renderer
Lights (directional, point, spot, shadows), HDR environments (.hdr, .exr, .png, presets; rotation, exposure, blur), camera presets and
turntable, 16 shapes plus custom meshes (OBJ, glTF, PLY, STL), scenes (floor, look-dev, pedestal), displacement / parallax / bump,
tone mapping and post (SSAO, DOF, bloom, grading, vignette), channel isolation, and a progressive path tracer (View > Render > Path tracer).
Headless: `materialcraft render3d wall.sbs out.png --mode pt --spp 256 --env sunset --rig three_point --turntable 36`.

## Opening packages and linked files
**`.sbs` packages** open with File > Open or by dropping them on the window, and stay `.sbs` (see Files above). To draw them,
MaterialCraft renders each node with its own implementation of that node: built-in nodes exactly, library graphs referred to by
name (the common noises, patterns, tiles and weaves) with their settings translated, pattern maps (FX-maps) and function graphs
as they are; anything without an implementation yet shows a marked placeholder while the file keeps it untouched. The graph's
settings list what renders approximately. See IMPORT_SBS.md.

The Explorer lists the package's resources (bitmaps, SVG, fonts). Linked files are found by their path, next to the package and in
its `resources` folder. Bitmaps can be PNG, JPEG (baseline and progressive), TIFF (LZW, PackBits, Deflate, 8/16/32 bit), OpenEXR,
Radiance HDR, WebP (lossy with alpha, lossless, animated: first frame), DDS (BC1-BC7 including BC6H, and uncompressed), TGA or BMP;
all readers are written in Rust, with no dependencies, and recognise the format from the file contents.

## Not implemented (honest list)
AxF import, OCIO/ACE colour management, scripting API, SVG gradients, WOFF2 fonts and font kerning / shaping,
PIZ-compressed EXR, 12-bit and arithmetic-coded JPEG, JPEG-in-TIFF, BigTIFF, and any testing of the 3D viewer on real GPUs (only a software GL driver was available).
See ROADMAP.md for the complete list.


## Nodes
The format's own nodes: 27 atomic nodes (Blend, Levels, Blur, Transformation 2D, Gradient Map, FX-Map, Pixel Processor, Text, SVG,
Bitmap, ...), 339 library graphs (noises, patterns, tiles, weaves, grunges, filters, material tools), 84 function nodes and the
FX-Map pattern nodes. `materialcraft nodes [words]` lists them; NODES.md describes the engine nodes they render through.
