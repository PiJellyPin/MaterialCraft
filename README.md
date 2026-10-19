
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
    cargo run --release -- render examples/brick.mcg out --size 1024 --render3d   # CLI
    cargo run --release --features gpu -- render examples/brick.mcg out --size 2048   # CLI with GPU compute for nodes and baking (falls back to the CPU automatically; bake takes --device auto|cpu|gpu)

Requires a current stable Rust toolchain (https://rustup.rs). On Windows the MSVC build tools are needed.
The core library has zero dependencies; the editor uses egui/eframe + rfd (native file dialogs).

## Editor
Start screen (recent files, New graph dialog with templates) -> editor with Explorer, Library (thumbnail grid of 217 texture nodes plus "Ready-made" generator and filter folders; all 33 finished graphs also open as examples from the start screen), Graph,
3D View, 2D View and Properties panels. Panels can be closed/reopened from the Windows menu.

Path nodes (stroke, fill, distance, border, scatter, splatter, track, cable, seam, warp, sample) follow splines that you draw in the 2D view (select a path node > Edit spline), import from an SVG or trace from any node's picture (right-click > Trace to spline).

Tab = add node at cursor · drag a link into empty space = add + auto-connect · Ctrl+Z/Y undo/redo ·
Ctrl+C/X/V/D · rubber-band & Shift/Ctrl multi-select · C = comment frame · double-click a subgraph to edit it ·
F = frame all · Alt/Space/Middle-drag = pan · wheel = zoom.

## CLI
    materialcraft render <graph.mcg|.mca> <outdir> [--sizes 512,1024] [--depth 16] [--param "Label=0.5"] [--variations 8] [--render3d]
    materialcraft publish <graph.mcg> <out.mca>      # embeds bitmaps, flattens editor data
    materialcraft bake --low <mesh>... [--high <mesh>...] [--cage <mesh>] --out <dir> [--preset game] [--size 2048] [--aa 2] [--maps normal,ao,curvature]   # see: materialcraft bake --help
    materialcraft bake <low> <high> <outdir>                      # short form; meshes can be OBJ, glTF / GLB, PLY or STL
    materialcraft library [words] [--kind material|generator|filter] [--export <id> <file>]   # built-in ready-made graphs
    materialcraft resources <graph.mcg> [--embed] [--update] [--collect] [--prune]   # list / manage the files a graph uses
    materialcraft nodes | check | params | testmesh | example

## AI control

Assistants and scripts can build and edit graphs through an MCP server, a command line and a live editor bridge. See [AI_CONTROL.md](AI_CONTROL.md).

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
Besides texture graphs, a package can hold **function graphs** (`meta kind "function"`, about 85 value nodes: math, vectors,
logic, variables, random, image sampling, calls) and **pattern programs** (`meta kind "fxmap"`: quadrant, iterate, switch and
marker nodes, run by the Pattern Program Map node). Any parameter can be driven by a function, and the pixel processor and
value processor run function graphs. Also: curve node, SVG and text nodes (own rasteriser and font reader).

## 3D viewer and renderer
Lights (directional, point, spot, shadows), HDR environments (.hdr, .exr, .png, presets; rotation, exposure, blur), camera presets and
turntable, 16 shapes plus custom meshes (OBJ, glTF, PLY, STL), scenes (floor, look-dev, pedestal), displacement / parallax / bump,
tone mapping and post (SSAO, DOF, bloom, grading, vignette), channel isolation, and a progressive path tracer (View > Render > Path tracer).
Headless: `materialcraft render3d graph.mcg out.png --mode pt --spp 256 --env sunset --rig three_point --turntable 36`.

## Importing files and managing resources
The Bitmap node reads PNG, JPEG (baseline and progressive), TIFF (LZW, PackBits, Deflate, 8/16/32 bit), OpenEXR, Radiance HDR, WebP
(lossy with alpha, lossless, animated: first frame), DDS (BC1-BC7 including BC6H, and uncompressed), TGA and BMP; all readers are
written in Rust, with no dependencies. The format is recognised from the file contents. Drop an image, `.svg` or `.cube` file onto the graph
to create the node. The Color LUT node applies `.cube` (1D and 3D), `.3dl`, or a strip / Hald lookup image. The Text node takes TrueType, OpenType
(CFF), `.ttc` (first face) and WOFF fonts, wraps to a width and can be coloured.

Tools > Resources lists every file the graph depends on - bitmaps, SVG, fonts, lookup tables, splines, baked maps - as embedded or linked, with a thumbnail,
size, format, the nodes that use it and a warning when a linked file is missing. For each you can embed, extract to a folder, relink, replace its contents, update it
from the file it was imported from, rename or delete it; for the whole package: embed all, update all, collect linked files into the project's `resources`
folder, remove unused. Relative links are found next to the project file and in its `resources` folder. Publishing embeds all of them (not only bitmaps).
CLI: `materialcraft resources graph.mcg [--embed] [--update] [--collect] [--prune] [--extract dir] [--relink old new]`.

## Not implemented (honest list)
AxF import, OCIO/ACE colour management, scripting API, SVG gradients, WOFF2 fonts and font kerning / shaping,
PIZ-compressed EXR, 12-bit and arithmetic-coded JPEG, JPEG-in-TIFF, BigTIFF, and any testing of the 3D viewer on real GPUs (only a software GL driver was available).
See ROADMAP.md for the complete list.


## Nodes (217 texture nodes, 84 function nodes, 5 pattern-program nodes)
Generators: shape, tile sampler, tile generator, splatter, splatter circular, perlin, fractal noise, clouds, gaussian noise,
BnW spots, crystal, directional noise, grunge, grunge maps, plasma, voronoi, voronoi fractal, stripes, weave, weave generator, bricks, checker, gradients, star, SDF shapes.
Filters: blend, RGBA merge, switch, flood fill (+ random grayscale / color / gradient), transform 2D, directional / vector /
slope warps, blur and blur HQ, levels, histogram scan / range, bevel, distance, gradient map, uniform color, HSL, color match,
make-it-tile photo / patch and more. Material: normal, ambient occlusion, curvature (smooth), height blend, edge wear, dirt, dust.
Structure: output (drives the 3D view), input color / grayscale (subgraphs), subgraph instances, bitmap, SVG, text, curve, pattern program map, pixel / value processor, input value, comment, frame, dot.

