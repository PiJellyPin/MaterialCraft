# all_nodes.sbs compatibility report

**Both tests pass once the fixture's 34 environment maps are in `assets/panorama/`.** Every one of the 566 compositing nodes now converts to a MaterialCraft node that renders (no gray placeholders), every node in the package has an editor definition, the embedded SVG decodes, and the import paths agree. The only remaining items are two problems in how the fixture was authored; the test lists them separately and they do not fail it.

The previous run of this report found 207 unconverted nodes, 35 unresolved images, 28 missing definitions, an undecodable SVG, an empty value processor and an ignored output-size function. [What changed](#what-changed-since-the-previous-report) addresses each finding. [The complete node mapping](ALL_NODES_NODE_MAPPING.md) lists all 566 nodes.

## Run and reproduction

- Completed: `2026-10-21`.
- Fixture: [`tests/assets/all_nodes.sbs`](../../tests/assets/all_nodes.sbs) (unchanged), SHA-256 `3d259e31fb25e46a4a3fbfbe8bd575e56dbec4da0b13690f7a76af8c92dfeafb`.
- Base commit: `1d319317b45203e0c6e6c2e9f12338cbfb9428ab` (branch `13-address-missing-and-broken-nodes`) plus the changes described here.
- Toolchain: `rustc 1.97.0`; CPU renderer, default Cargo features, release build; all compositing nodes at 32×32, graph outputs at 64×64.
- Environment maps: the run used stand-in EXR/HDR files with the fixture's 34 file names in `assets/panorama/` (the real maps are not in the repository). They exercise lookup and decoding; the real files resolve the same way.

```sh
cargo test --release --test sbs_all_nodes -- --nocapture --test-threads=1
```

| Test | Result |
| --- | --- |
| `all_nodes_import_export_and_reimport_preserve_the_package` | PASS |
| `all_nodes_import_maps_and_renders_every_node` | PASS (with the environment maps in `assets/panorama/`; without them it fails and names each missing map) |

## Support summary

Direct import and conversion from the editor's native graph report the same counts and the same nodes:

| Mapping classification | Nodes | Returned textures | Render errors |
| --- | ---: | ---: | ---: |
| Exact mapping | 61 | 60 | 1 (the fixture's temporary bitmap) |
| Approximate mapping | 505 | 505 | 0 |
| Unconverted | 0 | 0 | 0 |
| **Total** | **566** | **565** | **1** |

“Approximate” means the node runs MaterialCraft's own implementation rather than the library's graph, or converts a value between units; the specific behavior notes are in section 6. “Exact” is still the importer's classification, not a pixel comparison against Substance 3D Designer.

All seven outputs render finite 64×64 textures, and direct import and conversion from the native editor graph give identical output pixels. Preservation is unchanged: unchanged saves are byte-for-byte identical, and an edit survives export and reimport.

## What changed since the previous report

### 1. External images: resolve from `assets/panorama/`

The 34 environment maps (`sd-3dview-maps://<name>.exr`, one `.hdr`) resolve by file name from `assets/panorama/` beside the executable or in the working directory, like any linked file. Put the real maps there and the bitmap nodes read them; the test deliberately does **not** excuse them, so a missing map still fails the check with its name.

`XXXXXX-Untitled_Bitmap.png` lives in the authoring machine's temp folder (`../AppData/Local/Temp/Adobe/...`) and never travelled with the fixture. It is listed as a **fixture limitation**: reported, not a failure.

### 2. The 207 unconverted library nodes: implemented

Every library graph the fixture uses now has a native implementation (kinds starting with `x_`, in `src/nodes10*.rs`), with the library's parameters, inputs and outputs (multi-output nodes keep every output). They are in the editor's node menu under their categories. By family:

- **Image filters and adjustments**: ACEScg / linear sRGB / sRGB conversions, colour temperature, exposure and HDR range viewers, blackbody, HDR merge, get size, Hald CLUT, histogram compute / render, ID to mask, lighting cancel (high and low frequency), median, mean of least variance, anisotropic Kuwahara, directional distance, extend shape, flood-fill mapper, RT shadows and shadows, RT irradiance, normal intensity and uncombine, clone, non-square transform, non-uniform rotation, crop, multi crop, multi clone patch, colour equalizer, multi-angle albedo / normal, UV mappers, triplanar, 3D planar projection, 3D linear gradient, 3D volume mask, AO cancel, apply / create / modify colour palette, height extrude, height-map frequencies mapper.
- **Materials**: material height blend, switch, transform, adjustment and colour blends, multi-material blend (16 materials and an ID map), material selector, mesh-data blender and combiner, snow cover, water level, material crop and clone patch, albedo-safe, PBR validation, PBR converters, dielectric F0, metal reflectance, PBR base material, bitmap to material, PBR render, atlas splitter and atlas scatter, smart auto tile, all 24 mask generators, the seven weathering materials, and the 26 material presets.
- **Splines and paths**: spline circle / cubic / quadratic / poly-quadratic, point list, 2D transform, append, merge list, bridge (two splines and list), select, warp, sample height / thickness, scatter on spline and splines on splines, spline render / fill / flow mapper / mappers / bridge mappers, path polygon, mask to paths, path 2D and quad transforms, path warp, select path, vertex processors, path trace, path to spline.
- **Shape splatter**: splatter, splatter blend (grayscale and colour), data extract, to mask; scratches generator (normal).
- **Panoramas and 3D textures**: panorama shapes, line / plane / sphere lights, physical sun and sky, position, gradients, rotation, nadir extract / patch, straighten horizon; 3D texture position, SDF, transforms, surface and volume render; cube 3D and its G-buffers.

Spline, path and splatter data travel between these nodes in MaterialCraft's own encoding (an image whose pixels hold the points), so these nodes work with each other, not with data images made by other programs. 3D textures are slice atlases in MaterialCraft's own layout.

### 3. Editor definitions: complete

The 26 material presets now have catalogue entries, so the editor shows their parameters and outputs. The function nodes `pow` and `while` have definitions and run in the function engine (`while` evaluates its initial value once, then its body while the condition is true, up to the node's iteration limit). The test's “Unknown editor node definitions” check finds none.

### 4. Embedded SVG: decoded

Embedded resources without a declared format that carry a 4-byte length prefix followed by a zlib stream are inflated before decoding. `Untitled_Vector_Graphics.svg` now renders.

### 5. Value processor and dynamic output size

- **Value Processor `1584633100`** has no function in the file. It now gives 0 (the value an empty processor gives in Designer) instead of failing to render.
- **Function-driven output sizes are now used**: when an output-size parameter is driven by a function with a result node, the function's result sets the node's size when it runs.
- **Blend `1584843868`**'s output-size function in the fixture is a palette of 237 function nodes with no result node, so it cannot produce a size. The native loader no longer invents a result node for it (which had made the two import paths disagree); the default size is used, as in Designer. This graph is listed as a **fixture limitation**.

### 6. Approximate behavior: the flagged items

The previous report asked for curve tangents, font choice, non-uniform histogram inputs, emboss gloss and light colours, and dynamic output sizing to be prioritised. Each is now carried over:

| Node | Before | Now |
| --- | --- | --- |
| Curve (`sbs:curve`) | tangents dropped; the curve passed through the same points | each point keeps its left / right Bezier handles; segments are cubic Beziers solved for x, as in Designer. Classified exact. |
| Text (`sbs:text`) | font ignored; built-in face | a package font resource is read as a file; a family name (the fixture's “Arial”) is looked up among the installed fonts (Windows, macOS and Linux font folders), with metric-compatible stand-ins such as Liberation Sans when the family is not installed. Classified exact; a note appears only if no matching face is installed. |
| Histogram Scan Non-Uniform | position and contrast maps unused | the maps drive position and contrast per pixel when the node's “custom” switches are off (the sliders are used when they are on, the default). |
| Emboss (`sbs:emboss`) and Emboss With Gloss | ran on the input's own relief; height input, colours and gloss ignored | the input is lit by the height map's slopes with the highlight and shadow colours; Emboss With Gloss adds a specular highlight whose sharpness follows Gloss. Still marked approximate: the strength is not yet matched against a reference render. |
| Dynamic output size | ignored | used when the function has a result (see section 5). |

Other improvements made while checking renders: panorama world positions follow the library's Z-up default (the default lights sit above the horizon), RT Shadows uses its attenuation and light intensity and steps about one pixel at a time, atlas splitting and scattering are anti-aliased and fast, and closed splines close their bridges and mappers without a seam.

The remaining behavior notes the test prints (informational, not failures) are:

- unit conversions to MaterialCraft's own nodes (normal, blur, directional blur and warp, distance, sharpen, SVG, warp, HSL), and the FX-map drawn by MaterialCraft's quad-tree engine;
- the fixture's Pixel Processor, whose function is a constant;
- 3D noises read on the texture plane when no position map is connected;
- MaterialCraft's own fur, caustics, scratches, random tiles and tile-sampler blending;
- MaterialCraft's own versions of the weathering materials, the 26 material presets, PBR render and RT irradiance;
- the spline, path, splatter and 3D-texture encodings described in section 2;
- emboss strength (above) and glow size / strength, which are converted approximately.

## Remaining limitations

- **Library functions** (`sbs://functions.sbs`, such as `Functions/Math/clamp`, `HSVtoRGB`, the easings) are now **implemented natively** (`src/libfuncs.rs`): all 161 of Substance Designer's function presets, written as MaterialCraft's own `f_*` function graphs, so a material that drives a parameter with one of them renders **without Designer installed and with no `functions.sbs` present**. 151 of them are checked to match Designer's own output exactly (`tests/libfuncs.rs` keeps a file-free subset; the full check compares every function against Designer's library when it is available); the other 10 are the random generators (`Uniform_*`, `Discrete`, `GlobalRandom`, `Random_Color/Luminosity`, `disorder`), which are functionally equivalent but not bit-identical because their randomness is seeded differently. Designer's own `functions.sbs` is never consulted: every library function is built in, and a call to a `sbs://` library function that is **not** among the 161 gives 0 with a note. The fixture's palette function holds 154 such calls but is never run (it has no result node).
- **`while` inputs** are matched by name (`init`, `condition`, `loop` and common variants), because the fixture's `while` has no connections to confirm Designer's identifiers.
- **Visual fidelity** of the natively implemented library graphs has been checked on contact sheets of every node and output, not against reference renders from Designer.

## Evidence and interpretation

The Rust test supplied the pass / fail result and the behavior notes. A read-only diagnostic program linked against the same release library collected the per-node classifications, notes and 32×32 render results for the [mapping appendix](ALL_NODES_NODE_MAPPING.md). Contact sheets rendering every new node's outputs from shared test inputs were reviewed by eye; several issues found that way were fixed (listed in section 6).

The mapping appendix is a snapshot of this fixture and run, not a claim about every possible SBS package or configuration.
