# MaterialCraft Roadmap

MaterialCraft is a node-based procedural material editor written in pure Rust (Windows, Linux, macOS). The core library has zero dependencies; only the GUI binary uses eframe + rfd. File formats: `.mcg` (editable graph), `.mca` (published package).

Status note: everything has been built and tested on Linux only (CI for Windows, Linux and macOS is set up in `.github/workflows/ci.yml` but has not run yet). The GPU viewport has been verified on Mesa llvmpipe only, not on real GPUs.

---

## 1. Implemented (216 texture node types, 84 function nodes, 5 pattern-program nodes)

### Generators: noise
- White Noise
- Gaussian Noise
- Perlin Noise
- Fractal Noise (fbm, ridged, turbulence modes)
- Clouds
- Plasma
- Voronoi (distance, cell ID, edge, F2 modes)
- Crystal
- BnW Spots
- Directional Noise (seamless at any angle)
- Grunge

### Generators: shapes and gradients
- Circle, Rectangle, Polygon, Star, Shape
- Gradient (linear, radial), Axial Gradient, Circular Gradient
- Checker
- Uniform Color

### Generators: patterns
- Bricks
- Stripes (seamless at any angle)
- Weave
- Tile Sampler, Tile Generator
- Splatter, Circular Splatter

### Generators: custom
- Pixel Processor (per-pixel expression language)
- Bitmap import (PNG)

### Filters: blur and sharpen
- Blur, Blur HQ, Directional Blur, Slope Blur
- Sharpen, High Pass

### Filters: color and values
- Levels, Adjust, Invert, Threshold
- Histogram Scan, Histogram Range, Quantize, Clamp
- HSL, Tint, Gradient Map, Color Match
- To Gray, Channel, RGBA Merge

### Filters: geometry
- Transform 2D, Mirror, Swirl
- Warp, Directional Warp, Vector Warp
- Make It Tile Photo, Make It Tile Patch

### Filters: compositing and analysis
- Blend (with blend modes), Switch
- Flood Fill, Flood Fill Random, Flood Fill Color, Flood Fill Gradient
- Bevel, Distance, Edge Detect, Emboss

### Material and surface
- Normal From Height, Normal Blend
- Ambient Occlusion, Curvature, Curvature Smooth
- Height Blend
- Edge Wear, Dirt, Dust

### SDF
- SDF Shape, SDF Combine, SDF Mask, SDF 3D Form

### Graph structure
- Output (11 usages: basecolor, normal, roughness, metallic, height, ambient_occlusion, opacity, emissive, diffuse, specular, glossiness)
- Input (for subgraphs), Subgraph, Dot (link re-route)
- Comment, Frame

### Editor and workflow
- Packages workspace: any number of packages open at once, shown in the Explorer as roots (icon, italic bold name, `*` when modified) with their graphs underneath; each graph expands to list its outputs. File > New > Material Graph (Ctrl+N) adds a graph to the selected package; File > New > Empty Package creates a new unsaved package (preset chooser with graph name); File > Open adds packages without closing others. Right-click empty Explorer space: New Package / New Material Graph / Open Package(s); right-click a package: New > Material Graph, Save Package, Close Package (asks when unsaved). Graphs from any open package can be dragged onto the canvas to become nodes
- Graph nodes have one labelled output socket per output (Base Color, Normal, Roughness, ...); links remember which output they read (`in=@3.height` in the file format), sockets with links are filled orange
- Graphs as nodes: any saved graph (`.mcg`/`.mca`) can be dragged from the Explorer onto the canvas, dropped from the OS onto the window; it becomes a subgraph node (input sockets only appear if the graph defines inputs) whose exposed parameters and chosen output work like any node, so complex materials can be built from simpler ones
- Node graph with zoom, pan, rubber-band select, drag-to-connect from either end, cycle protection
- Tab search with auto-connect, undo/redo (100 steps), copy/paste (also via OS clipboard), duplicate, auto-layout
- Subgraph creation and editing, multiple graphs, exposed parameters
- Per-node property panels with color wheel and gradient editor
- Home screen with recent files, New Graph dialog, node library with thumbnails
- Dockable panels (shared `craft-ui` docking): drag a tab to another group, to a side or out into a floating panel; Windows > Reset layout; the layout is remembered between runs; the graph canvas stays fixed
- Settings in `config.toml` (shared `craft-settings`): atomic saves, backup file, damaged files moved aside; old `settings.txt` / `recent.txt` are imported on first run
- Forced dark theme (does not follow the operating system's light theme)

### 3D viewport (GPU, OpenGL 3.3 via egui_glow)
- 13 preview objects: Cube, Cylinder, Hollow box, Inner box (seen from inside), Plane, Plane (hi-res), Sphere, Cloth, Mat ball, Rounded cube, Rounded cylinder, Sphere 2 tiles, Torus (mesh resolution up to 768 x 768; all closed shapes stay crack-free when displaced)
- True outward displacement, plus parallax relief and bump modes
- GGX PBR, mipmaps with anisotropic filtering, 4x MSAA (falls back to none), contact shadows, hue-preserving filmic tone mapping (bright oranges no longer wash out to pink), horizon-occluded reflections
- Light, camera, exposure and ambient controls, UV tiling, wireframe, normal strength; the key light follows the camera by default (Lights > Light follows the camera), a soft fill light keeps shadowed sides readable
- Studio reflection environment (softboxes, rim strip, horizon) that blurs with roughness, so roughness and metallic changes read clearly
- CPU renderer as automatic fallback (Renderer menu)
- Headless test: `cargo run --release --features gui --example gpu_shot -- out.png [shape] [relief] [w h] [nomsaa] [wire]`

### Export, files and tools
- Export to PNG, JPEG, TIFF at 8 or 16 bit, EXR, HDR
- Crash-safe writes for graphs, packages and exports (`craft-atomic`: staged file, flush, atomic replace)
- Output colour space on export for colour maps: sRGB, P3, wide-gamut RGB, ROMM RGB, Rec. 2020, linear sRGB (`craft-cms` ICC engine); CLI `--colorspace`
- Several resolutions at once (256 to 8192 px) and per-output selection
- `.mcg` editable graphs, `.mca` published packages with exposed parameters
- Mesh baker (BVH ray tracing, multi-core, progress and cancel): OBJ, glTF / GLB, PLY and STL meshes; tangent and world normal, position, AO, bent normal, curvature, thickness, height, object / material / group ID, vertex colour, opacity, direction and UV layout maps; cage, match by name, anti-aliasing, padding, DirectX / OpenGL, multiple UV sets and materials; results live in the package and feed Baked Map nodes
- CLI: render, publish, params, bake (several meshes, cage, presets, bake set files), trace

### Quality and resolution
- Graph size 128 to 8192 px; the editor previews at the graph's own size (2048 px by default for a 2K graph, capped at 4096 for the preview), shows a quick 256 px pass first and refines in the background. All parameters are resolution independent (checked by a test)
- Every node tiles seamlessly: centred shapes and gradients wrap around the tile border, checker/weave use even counts, flood fill regions that cross the border keep one continuous gradient, Transform 2D cross-fades only when the matrix cannot tile by itself. `tests/seam.rs` renders every node and 100+ parameter variants and fails on any visible seam
- Anti-aliased hard edges (shapes, checker, bricks), bicubic resampling in Transform/Warp, Scharr-filter normals, fast gaussian blur (cost independent of radius, multi-threaded), 4096-entry gradient table for noise, rotated fractal octaves (no visible grid), spots as a union of discs (no cell-edge cuts)
- 2D view shows true texels: crisp squares when zoomed in, mip-mapped when zoomed out. The shipped example graph is 2048 px. Maps above 4096 px are box-averaged (not point-sampled) when they go to the 3D viewport
- Per-node Output Size: every node has Relative to Input (default), Relative to Parent or Absolute (16-8192 px), plus a -12..+12 power-of-two modifier. Inputs of another size are resampled automatically, so heavy nodes (AO, blurs, noises) can run at half or quarter resolution. Nodes with their own size show it under the card
- Node result cache: the editor keeps every node's result between evaluations, found by a hash of its settings, the evaluation size and the hashes of its inputs. An edit recomputes only the edited node and what is downstream of it; thumbnails and unchanged outputs come straight from the cache. Least-recently-used entries are dropped above a memory budget (3 GB, set `MC_CACHE_MB` to change), and grayscale results are stored with one channel. On the 2048 brick example (2 cores) editing the normal strength went from about 4.5 s to about 0.35 s; `cargo run --release --example cache_bench -- graph.mcg 2048 <node> <param> <v1> <v2>` measures it
- The node shown in the 2D view is evaluated first and put on screen as soon as it is done, before the outputs, the 3D maps and the other thumbnails
- Faster kernels (cold run of the 2048 brick example: 3.9 s to about 1.4 s on 2 cores): Fractal Noise builds its gradient lattices once (about 1.8x faster, identical values); grayscale blurs run on one channel and the vertical pass needs no transpose (Ambient Occlusion about 4x faster); Normal From Height, Levels and Blend run on all cores (Normal about 6x, Levels about 4x faster); Slope Blur uses grayscale planes; Directional Noise samples the streak less densely (2x faster, mean difference below 0.01); bilinear and bicubic sampling skip the wrap arithmetic inside the image; the editor shares the kept output textures instead of copying them
- Responsive previews: editing cancels the running evaluation (it stops within a few rows, even inside a heavy node) and starts the new one immediately; previews are progressive (256, 512, 1024 ... up to the preview size) so a change shows in a fraction of a second; the 2D image and GPU textures are prepared on the worker thread, so the editor never stalls; switching or entering a graph clears the old previews at once
- Interactive vs settled passes: while a value is being dragged the editor evaluates only the subgraph feeding the 2D view and the material outputs — off-screen branches, disconnected experiments, thumbnails and the 3D maps cost nothing until the input stops moving. About 110 ms after the last change one full pass rebuilds every thumbnail and the 3D view. So a change on a large graph with many unrelated nodes stays responsive, and the GPU material maps are prepared on the worker thread instead of the main thread
- Memory: intermediate textures are freed as soon as every node that reads them has finished, so large graphs at 4K fit
- Speed pass (October 2026; every output checked bit for bit against a snapshot of all node demo graphs and library graphs, except the directional noise change below). Measured on all library graphs at 1024 px on 2 cores: a cold evaluation 25.3 s to 15.9 s in total, a pass with nothing changed 2.2 s to 4 ms, the full pass after an edit 2.5 s to 0.09 s; `cargo run --release --example profile -- 1024` measures it
  - Cache passes: a cached grayscale result that is still on screen is handed out again instead of being expanded from its one-channel copy; nodes the pass only shows as a card take their thumbnail without their pixels; a node already evaluated as another node's input is not evaluated a second time for its card
  - Persistent worker pool (`src/pool.rs`, no new dependency): the per-pixel loops no longer start operating-system threads for every node (0.1-0.6 ms each, more than a whole node at the quick 256 px pass), and work is handed out in small pieces so uneven rows balance out; image loops walk rows instead of dividing per pixel
  - Multi-output nodes: the outputs a pass reads are produced by one run (Path Track, Cable, Border, Seam and Sample: about 2.4x for Railway Track and Rope), and a node's inputs are kept until all of its outputs are done (before, they could be freed after the first output and computed again)
  - Kernels: Grunge Map builds its noise lattices once (1.5x); fractal and cellular noises wrap without integer divisions (1.15-1.25x); Directional Noise takes at most 4 samples per lattice cell along a streak (up to 3.5x; no pixel changes by more than one 8-bit step); small-radius Blur runs on all cores; Ambient Occlusion samples one channel and converts once; Blend no longer copies the background first; Output and Dot nodes pass their input on without copying it
  - Editor: cards and links outside the view are neither drawn nor hit-tested; a thumbnail is uploaded again only when its pixels changed; with displacement in the Floor, Look-dev and Pedestal scenes the floor follows the displaced object without the scene mesh being rebuilt on every edit

### Quality checks
- 69 automated tests, including: a wrap-around check that rolls the input of 30 filters and requires the output to roll identically (proves every filter wraps at the borders), seam checks on every node, every node at four resolutions (finite, sane range, not blank), every documented parameter fuzzed with extreme values (no panics/NaN), and whole-graph import and per-output links
- `cargo run --release --example node_sheet -- out_dir` renders a PNG for every node type for visual review

---

### Milestone 2 additions (nodes, independent implementations)
- Adjustments: Contrast Luminosity, sRGB to Linear, Linear to sRGB, Premultiply, Histogram Equalize, Min Max, Morphology, Multi Switch, Lookup Table, Frequency Split / Merge, Palette Extract / Apply
- Transforms: Auto Crop, Polar Convert, Clone Patch, Noise Upscale, Symmetry Slice, Quad Transform, Trapezoid Transform, Make It Tile Mirror
- Normal and material tools: Bent Normal, Normal Facing, Height Normal Blend, Height To Normal World, Normal Transform, Normal Sobel, Metal-Rough <-> Spec-Gloss converters, Material Blend, Height Combine, Convex Concave, Leak Streaks, Settled Dust
- Generators: Blue Noise, Fibers, Scratches, Caustics, Liquid Noise, Perlin 3D, Paving, Wire Mesh
- Effects: Halftone, Reaction Diffusion, FXAA, Bilateral Blur
- Grunge maps 16-24

### Milestone 2 closing additions
- Noises: herringbone and arc pavements, anisotropic noise, fur, moisture and microscope noises, 3D worley and 3D fractal noise (tile in x and y, free z)
- Shapes and volumes: shape extrude (height and walls), shape mapper, volume mask and 3D gradient
- Effects: cross section, uber emboss (emboss, highlight, shadow), diffusion (pull-push fill), summed area table, vector morph, safe transform (tile-safe integer transform)
- Material filters: snow, season and water filters

### Milestone 3 additions (splines and paths)
- Spline resources (`.spl` text, bezier points, handles, width factors, open or closed, several paths), built-in shapes, SVG outlines to spline
- Path nodes: Path Stroke (dashes, taper, profiles), Path Fill, Path Distance (four modes), Path Border, Path Scatter (shapes along a path, orient to the curve, jitter, density, pattern input), Path Track, Path Cable, Path Seam, Path Splatter (rows of copies, mask input), Path Warp (push, slide, swirl), Path Sample (position, width, height and side maps)
- Every path node: up to four merged splines (`file`, `file2` to `file4`), path range, scale, rotation, offset, mirror and reverse
- Mask to spline: `Spline::from_mask`, `materialcraft trace`, and the node context menu entry Trace to spline
- Spline points carry a height value and a tangent mode (auto, mirrored, free, corner), both editable in the spline editor
- Spline editor in the 2D view (add, move, delete points, handles, tangent modes, closed / open, width, height, snapping, undo)
- Library: Rope, Railway Track, Beaded Chain, Bevel Border, Stitched Leather, Cobbled Path, Herringbone Floor, Fan Pavement

## 2. Not implemented yet

### Graph logic
- [ ] Package context menu: Rename, Save As, Duplicate, Reveal in file explorer, Delete; graph context menu: Rename, Duplicate, Delete, Close Graph
- [ ] Several graph tabs open at once (one graph is shown at a time today)
- [ ] Imported graphs are embedded copies; no live link or "update from source" when the package file changes
- [x] Size caption on graph nodes (shown under a node whose Output Size differs from the graph; outputs show size and bit depth)
- [x] Function graphs: about 85 `f_*` value nodes (variables, math, vectors, comparisons, logic, if / sequence, random, image sampling and statistics, inputs, casts, calls), a type checker, function-driven parameters (`fn.<param>`), pixel processor and value processor driven by a function
- [x] Pattern programs (our FX-Map): `fx_map` node running a graph of quadrant / iterate / switch / marker nodes with random seeds, depth limit, blend modes and tileable output
- [x] Pin (`f_pin`) and Input Value (`input_value`) nodes
- [x] Curve node (per-channel tone curves) with an interactive editor
- [x] Non-square graphs (width/height, `size W H`, tests), New Graph dialog with separate sizes, per-node `square_cells` (keeps cells square)
- [x] Live-linked packages, instances through shared links, format versioning, package tab bar, per-graph tabs, link picker for multi-graph packages, dependency manager (relink, embed copy, remove, search folders)
- [ ] Graph templates, parameter presets, randomized variations

### Nodes
- [x] Spline and path tools: shapes along curves, tracks, cables, seams, borders, distance and fill (see Milestone 3 additions); done: mask to path, path warp, spline splatter, thickness / height sampling, several splines and transforms per node; paths reach nodes by resource name rather than by pin, so merge and transform are node options
- [ ] Several hundred more nodes, for example:
  - more weaves and tile patterns
  - Non-Uniform Blur
  - Multi-Directional Warp
  - Cells and fur patterns
  - Emboss with Gradient
  - Histogram Equalize
  - Gradient Dynamic
  - Auto Levels
  - Cube Map tools
- [x] Built-in library of ready-made graphs (26 materials: the first 15 plus Pink Granite, Asphalt, Gravel, Snow, Cracked Ice, Tree Bark, Brushed Steel, Diamond Plate, Sci-Fi Panels, Dungeon Wall and Cooling Lava; 13 generators, 5 filters; every output is checked to tile by `tests/seam.rs`) with thumbnails, tag search, categories, `materialcraft library` ; templates open as examples from the start screen (own tab, not part of the project, no duplicate tabs, "Add to project" / File > Package > Add template to copy), ready-made generators and filters drag in as one template node with right-click "Edit template" (more entries to come)

### 3D and rendering
- Custom light rigs (several lights, HDR environment images), a true seamless-edge check in the 3D view
- [x] HDRI and image-based lighting
- [x] Load custom meshes and 3D scenes into the viewport (Milestone 5; path-traced mode, HDRI, light rigs, post chain and `render3d` CLI also done)
- [x] Path-traced rendering and post-processing (bloom, DOF, SSAO)
- [ ] GPU tessellation (displacement is per vertex on a pre-built mesh today)
- [x] Camera and light presets, turntable rendering

### Import and color
- [x] SVG import (own rasteriser: shapes, paths, transforms, solid fill and stroke), font import (TrueType, OpenType/CFF, .ttc, WOFF; own reader) and a text node with wrap width and colour. Not supported: SVG gradients / clipping / filters, WOFF2, kerning and complex-script shaping
- [x] Bitmap import (Milestone 6): PNG, JPEG (baseline / progressive), TIFF, EXR, HDR, WebP, DDS (BC1-BC7), TGA, BMP, all in own code and recognised by content; drag-and-drop of image, SVG and lookup-table files onto the graph
- [x] Colour lookup tables: `.cube` (1D / 3D), `.3dl`, strip and Hald images, Color LUT node
- [x] Resource manager (Tools > Resources, `materialcraft resources`): embedded / linked / missing, thumbnails and metadata, embed, extract, relink, replace, update from source, rename, collect into the project folder, remove unused; publishing embeds every kind of file
- [ ] AxF material import
- [x] ICC-based output colour spaces for colour maps on export (built-in profiles, `craft-cms`)
- [ ] OCIO config support, per-bitmap input colour space, ICC profile import, colour-managed 2D / 3D views
- [x] 32-bit float output: EXR, HDR and float TIFF export (graph clamps still to be reviewed)

### Publishing and automation
- [x] AI control: command registry, MCP server, `exec` / `serve` CLI and live editor bridge (see AI_CONTROL.md)
- [ ] Unreal / Unity plugins and a small runtime for published packages
- [ ] Scripting API and batch automation
- [ ] Engine-specific output templates (Unreal ORM packing and similar)
- [ ] Texture atlases for the shape/splatter workflow

### Polish and verification
- [x] A path node can merge up to four splines (`file` to `file4`) and move, scale, rotate, mirror or reverse them
- [ ] Library thumbnails are only 64 px and generated once at startup (node and ready-made tiles alike)
- [ ] `.ico` resource for the Windows exe file (the window and taskbar icon already work on all platforms; the release build hides the console window on Windows)
- [ ] Visual check of the 2D/3D splitter and the export window
- [ ] Run and verify on Windows and macOS
- [ ] Test the GPU viewport on real GPUs (NVIDIA, AMD, Intel, Apple)
- [x] CPU fallback renderer lights with the same environment as the GPU viewport
- [ ] GPU viewport: wireframe at mesh resolution 256+ renders as a solid blob
- [ ] Clouds at scale 1-3 and Fractal Noise at scale 1-2 show faint diagonal streaks: every octave reuses the base lattice (rotated and scaled), so a 2x2 lattice repeats only four gradients. The fix changes the pixels of existing graphs that use those scales, so it waits for a decision

---

## 2b. Feature gaps by area

We have 216 texture node types. Missing, by area (described by function; `NODES.md` lists what exists):

**Property coverage.** Done so far: the shared noise block (disorder, disorder speed, anisotropy, tile offset), rebuilt scatter nodes (built-in patterns, six inputs, offsets, masks, blending, map inputs, rings), radial blur, edge detect, mosaic and curvature, Voronoi / Cells / Grunge properties, a jittered triangle mesh style, the weave generator, the 24 grunge maps, gradient-map addressing and alpha, colour swapping, horizon AO with world units, colour quantising, multi-output nodes (including Voronoi, Cells, Splatter and the Flood Fill mappers), float export, graph-wide node precision, non-square graphs, typed pins and value pin types, per-node output format, live links, optional GPU compute. Still open: some node properties that need engine features (extra image inputs, function graphs).

### Noises and generators
- More cloud and directional-noise variants; dirt gradients; more grunge recipes (fur, moisture, microscope, anisotropic and 3D worley / fractal noises are done)
- Simplex and ridged 3D noises

### Patterns and shapes
- More brick and checker variants, gaussian and gradient variants, panorama shapes, random tile patterns
- Shape splatter with blending, data extraction and mask output

### Adjustments, blending, blurs, channels
- HDR range viewer, non-uniform and selecting histogram tools, lighting-cancel filters, luminance highpass

### Effects and warps
- Extend shape, UV-aware diffusion variants
- Nearest sampling in warps
- Flood-fill mapping outputs (bounding box size, index, position)
- Ray-traced nodes: AO, shadows, irradiance; 3D texture nodes (the height-based Bent Normal exists)

### Normal map tools
- Done in Milestone 2 (Bent Normal, Normal Facing, Height Normal Blend, Height To Normal World, Normal Transform, Normal Sobel); nothing open here

### Transforms
- Non-uniform rotation (the rest was done in Milestone 2)

### Material filters
- Colour and adjustment blend variants
- PBR utilities: validators, dielectric and metal reflectance, PBR preview
- Scan processing: AO cancellation, atlas tools, patch cloning, colour equalising, cropping, multi-angle capture to albedo / normal, smart auto tiling
- One-click bitmap to material

### Mesh-based generators (driven by baked mesh maps)
- Wear, dirt, dust, edge damage, leaks, grease, bleaching, brush and similar masks; weathering looks (cracks, fabric, leather, metal, moss, rock, rust)
- Utilities: tri-planar and planar projection, mesh data combining, material selection
- Done in Milestone 4: baked maps are graph inputs (Baked Map node), Mesh Weathering combines baked curvature, AO and normal maps into wear, dirt and dust; Edge Wear, Dirt, Dust, Leak Streaks and Settled Dust accept baked maps on their inputs
- Still open: the specialised looks (cracks, fabric, leather, metal, moss, rock, rust) as mesh-driven nodes, tri-planar projection, mesh data combining

### Splines and paths
- Done in Milestone 3: create (editor, built-in shapes, SVG import), render (stroke, fill, scatter along a path, track, cable, seam, border, distance)
- Also done: assemble (merge, select by range), modify (transform, warp, sample thickness / height), mask to paths
- Still open: create (quadratic and circle helpers, point list, bridge), vertex processing, a pin-based path type

### Graph system
- Function graph extras: loops, sampler border modes, bitwise operators
- Portal nodes, parameter presets, randomized variations, graph templates
- Tiling modes (none / horizontal / vertical)

### 3D view and rendering
- Done in Milestone 5: HDRI / image-based lighting, path-traced renderer, post-processing, camera and light presets, turntable, custom meshes and scenes
- Still open: HDRI tool nodes (blackbody, exposure, HDR merge, physical sun / sky, area lights, panorama tools), area-light shapes, real-GPU testing, tessellation shaders, user-assembled multi-mesh scenes

### Import, export, colour, automation
- SVG gradients; WOFF2 fonts; DDS cube maps / volumes; CMYK-free colour handling of imported bitmaps (see colour management)
- OCIO and ICC colour management
- Scripting API and batch automation, engine output templates (Unreal / Unity packing), UDIM, texture atlases, user asset folders
- Editor extras: user library folders, navigator minimap, customizable shortcuts, language packs

### Engine
- Evaluation is multi-threaded CPU, with an optional GPU compute path (`--features gpu`, Tools > GPU acceleration; Metal on macOS, DX12/Vulkan on Windows, Vulkan on Linux through wgpu) at 512 x 512 and up. GPU kernels: Perlin / fractal / directional noise (without disorder), blur, directional blur, anisotropic blur, radial (spin and zoom) blur, bilateral blur, slope blur, levels, blend, normal-from-height, bent normal / horizon AO, shape extrude (96 dependent passes in one submission), and the warp / directional warp / vector warp family (bilinear, nearest and bicubic). The mesh baker traces its rays on the GPU too (BVH in storage buffers; `device` setting Auto / GPU / CPU, `bake --device`). Results match the CPU within rounding (tests/gpu.rs, 17 tests). Everything else runs on the multi-threaded CPU. Each node still uploads its inputs and reads its result back; keeping chains resident on the GPU and the remaining kernels (Voronoi, distance, FX-Maps, quantize, curvature, edge detect) are Milestone 10. Not yet checked on real GPUs, only on a software Vulkan adapter

---

## 3. Suggested order

1. Function graphs + pattern programs (done, Milestone 1); next the node library by area (Milestone 2), which they unlock
2. Spline / path tools
3. HDRI image-based lighting and custom mesh loading
4. More nodes and a built-in material library
5. Color management, EXR/JPEG/TIFF import, HDR export
6. Scripting/batch API and engine output templates
