# all_nodes.sbs compatibility report

**Import/export preservation passes. Full node compatibility fails.** The package opens and survives saving, but it does not yet render every node as intended.

This report comes from a fresh run of the two tests in [sbs_all_nodes.rs](../../tests/sbs_all_nodes.rs). The fixture and test expectations were not changed for this audit.

**Subsequent fixture update:** The 34 unavailable HDRI references and bitmap nodes have been replaced with four EXRs from `assets/panorama/`. The missing `Untitled_Bitmap` PNG now uses the logo from `assets/logo.png`. All five references store only their filenames, allowing lookup in the package folders and fallback to application `assets/` and `assets/panorama/` folders. The current fixture has 536 compositing nodes and six resources. All four panoramas and the logo resolve and render through direct import and native conversion. The audit counts and missing-file list below describe the earlier fixture.

## Run and reproduction

- Completed: `2026-10-21T19:37:32.083410+00:00`.
- Fixture: [`tests/assets/all_nodes.sbs`](../../tests/assets/all_nodes.sbs).
- Fixture SHA-256: `880039bdf74e8e0842b319acabcdc1db2429af7f7bcbdde43da28f1ac2c6c730`.
- Base commit: `e8498c465264fdf85f20082b4a64a881c141cabb`, with the current uncommitted working-tree changes.
- Toolchain: `rustc 1.99.0 (b940084d7 2026-09-28)`.
- Result: **1 passed, 1 failed, 0 ignored**; Cargo exited with **101**.
- Scope: CPU renderer, default Cargo features, release build; all compositing nodes at 32×32 and graph outputs at 64×64.

```sh
cargo test --release --offline --locked --test sbs_all_nodes -- --nocapture --test-threads=1
```

| Test | Result |
| --- | --- |
| `all_nodes_import_export_and_reimport_preserve_the_package` | PASS |
| `all_nodes_import_maps_and_renders_every_node` | FAIL: unsupported conversions, unavailable files, missing definitions, render errors, and an incomplete embedded graph |

## Support summary

Both direct import and conversion from the editor's native graph report the same mapping counts:

| Mapping classification | Nodes | Returned textures | Render errors |
| --- | ---: | ---: | ---: |
| Exact mapping | 59 | 24 | 35 |
| Approximate mapping | 300 | 298 | 2 |
| Unconverted | 207 | 207 gray placeholders | 0 |
| **Total** | **566** | **529** | **37** |

**Returned textures do not imply supported nodes:** all 207 unconverted nodes render gray placeholders. They represent 206 distinct library node kinds; `sbs:lib:paths_tools/path_to_spline` occurs twice.

“Exact mapping” is the importer's classification, not a visual comparison against the originating application. It includes 35 bitmap nodes whose external images are unavailable. It also includes Blend node `1584843868`, which ignores a function-driven output size: that warning is recorded without changing the node's mapping classification.

There are **323 approximation diagnostics across 301 nodes**: the 300 approximate mappings plus that Blend node. Of those diagnostics, 284 describe routine substitutions with MaterialCraft implementations; the remaining 39 specific behavior warnings appear below.

The categories below overlap. For example, the 35 missing resources also account for 35 of the rendering errors, and 26 missing editor definitions belong to unconverted nodes. Do not add those counts as independent failures.

[The complete node mapping](ALL_NODES_NODE_MAPPING.md) lists all 566 compositing node UIDs, native kinds, renderer mappings, editor-definition coverage, render outcomes, and limitations.

## What passed and what this fixture cannot prove

- All 566 compositing nodes, six connections, seven output names, 36 resource records, and embedded graphs retain their expected inventory.
- Unchanged native saves are byte-for-byte identical, including a separately formatted copy of the package. Export/reimport preserves node identities, settings, links, positions, graph metadata, and embedded resources.
- Editing Uniform Color node `1359211355` changes the exported value and survives reimport; other nodes remain intact. The original fixture stays unchanged.
- Direct import and conversion from the native editor graph agree on node data and output pixels.
- All seven outputs render finite 64×64 textures: `ambientocclusion`, `basecolor`, `height`, `metallic`, `normal`, `output`, and `roughness`.

Only 13 nodes, including the seven Output nodes, are reachable from those outputs. **553 nodes do not feed any output**, so successful output rendering says little about most of the catalog. The test evaluates every compositing node separately at 32×32 to reveal those failures.

The native loader preserves a one-node FX-Map graph and a 237-node parameter function. The conversion summary's “0 functions” is not a count of preserved native function nodes; this fixture does not establish working conversion of that incomplete function. The test checks structure and finite pixels, not visual equivalence to reference renders or every parameter configuration.

## 1. Missing external bitmap assets: 35 files

These errors concern referenced image assets, not missing bitmap-node implementations. The fixture has one temporary PNG reference and 34 environment/lighting references: 33 EXR files and one HDR file. None of these 35 files resolved in this run. The separate SVG resource is embedded and has a decoding issue described below.

The resolver checks the original path, then the stored relative path and matching filename in:

- `tests/assets/`, next to this SBS file;
- `tests/assets/all_nodes.resources/`;
- `tests/assets/resources/`;
- `assets/` and `assets/panorama/` beside the executable and in the working directory.

For this run, the working directory was the repository root. `assets/panorama/` exists but is empty. The test executable's assets roots would be under `target/release/deps/`. Other subfolders are not scanned recursively, though an existing stored relative path can name a subfolder.

Supplying a matching file in a searched folder can address its lookup error; decoding still has to succeed. It will not address the 207 unsupported conversions or the embedded SVG/function issues. No image files were downloaded, copied into the repository, or embedded in this report.

| Node UID | Matching filename required | Asset role |
| --- | --- | --- |
| `1584633001` | `XXXXXX-Untitled_Bitmap.png` | Temporary bitmap |
| `1584832322` | `abandoned_sanatorium_staircase.exr` | Environment / lighting |
| `1584832324` | `airport.exr` | Environment / lighting |
| `1584832326` | `bonifacio_aragon_stairs.exr` | Environment / lighting |
| `1584832328` | `bonifacio_street.exr` | Environment / lighting |
| `1584832330` | `bus_garage.exr` | Environment / lighting |
| `1584832332` | `cave_entry_in_the_forest.exr` | Environment / lighting |
| `1584832334` | `corsica_beach.exr` | Environment / lighting |
| `1584832336` | `elevator_corridor.exr` | Environment / lighting |
| `1584832338` | `Gdansk_shipyard_buildings.exr` | Environment / lighting |
| `1584832340` | `glazed_patio.exr` | Environment / lighting |
| `1584832342` | `industrial_room.exr` | Environment / lighting |
| `1584832344` | `interior_atelier_soft_daylight.exr` | Environment / lighting |
| `1584832346` | `panorama_map.hdr` | Environment / lighting |
| `1584832348` | `road_in_tenerife_mountain.exr` | Environment / lighting |
| `1584832350` | `small_apartment.exr` | Environment / lighting |
| `1584832352` | `Soft_1Front.exr` | Environment / lighting |
| `1584832354` | `Soft_1Front_2Backs.exr` | Environment / lighting |
| `1584832356` | `Soft_1LowContrastFront_2Backs.exr` | Environment / lighting |
| `1584832358` | `Soft_2RingHighContrast.exr` | Environment / lighting |
| `1584832360` | `Soft_4TubeBank_2BlackFlags.exr` | Environment / lighting |
| `1584832362` | `Soft_5DaylightStudio.exr` | Environment / lighting |
| `1584832364` | `studio.exr` | Environment / lighting |
| `1584832366` | `studio 02.exr` | Environment / lighting |
| `1584832368` | `studio_03.exr` | Environment / lighting |
| `1584832370` | `studio_04.exr` | Environment / lighting |
| `1584832372` | `studio_05.exr` | Environment / lighting |
| `1584832374` | `studio_06.exr` | Environment / lighting |
| `1584832376` | `studio_black_soft_light.exr` | Environment / lighting |
| `1584832378` | `studio_white_hard_sunlight.exr` | Environment / lighting |
| `1584832380` | `studio_white_soft_light.exr` | Environment / lighting |
| `1584832382` | `studio_white_umbrella_soft_light.exr` | Environment / lighting |
| `1584832384` | `terrace_near_the_granaries.exr` | Environment / lighting |
| `1584832386` | `tomoco_studio.exr` | Environment / lighting |
| `1584832388` | `urban_exploring_interior.exr` | Environment / lighting |

The environment paths are `sd-3dview-maps://<filename>`. The PNG's stored path is `../AppData/Local/Temp/Adobe/Adobe Substance 3D Designer/resources/XXXXXX-Untitled_Bitmap.png`. Its resource also names an external copy under `SubstanceNodes.resources/`, but the current resolver uses the stored filepath and the folders listed above; the renamed fixture's automatic companion folder is `all_nodes.resources/`.

## 2. Unsupported renderer conversions: 207 nodes

Every unconverted entry is a library node for which the importer reports **“has no MaterialCraft equivalent yet.”** All 207 are represented by gray `color` placeholders in this fixture. The SBS data survives import/export, but the node's operation has no renderer implementation/mapping here. Providing bitmap artwork will not implement these nodes.

The [unconverted-node table](ALL_NODES_NODE_MAPPING.md#unconverted-library-nodes-207) lists every affected UID and exact library identifier. Of those nodes, 181 have a native editor definition but no renderer conversion; the other 26 also lack an editor definition.

## 3. Missing native editor definitions: 28 nodes

These are 26 library/material nodes in the main graph plus `pow` and `while` in the embedded output-size function. Preserving their serialized data does not establish editor support for their interface or behavior.

| Graph | Node UID | Native kind without a definition |
| --- | --- | --- |
| Main graph | `1584833851` | `sbs:lib:bricks_001/bricks_001` |
| Main graph | `1584833861` | `sbs:lib:bricks_005/bricks_005` |
| Main graph | `1584834378` | `sbs:lib:cardboard_001/cardboard_001` |
| Main graph | `1584835031` | `sbs:lib:concrete_002/concrete_002` |
| Main graph | `1584835321` | `sbs:lib:concrete_006/concrete_006` |
| Main graph | `1584835647` | `sbs:lib:concrete_010/concrete_010` |
| Main graph | `1584835948` | `sbs:lib:concrete_011/concrete_011` |
| Main graph | `1584836158` | `sbs:lib:concrete_044/concrete_044` |
| Main graph | `1584836287` | `sbs:lib:concrete_070/concrete_070` |
| Main graph | `1584836996` | `sbs:lib:fabric_002/fabric_002` |
| Main graph | `1584837273` | `sbs:lib:fabric_009/fabric_009` |
| Main graph | `1584838663` | `sbs:lib:fabric_025/fabric_025` |
| Main graph | `1584839008` | `sbs:lib:gravel/gravel` |
| Main graph | `1584839491` | `sbs:lib:metal_002/metal_002` |
| Main graph | `1584840061` | `sbs:lib:metal_003/metal_003` |
| Main graph | `1584840263` | `sbs:lib:metal_006/metal_006` |
| Main graph | `1584840449` | `sbs:lib:metal_plate_001/metal_plate_001` |
| Main graph | `1584840983` | `sbs:lib:metal_plate_004/metal_plate_004` |
| Main graph | `1584841298` | `sbs:lib:old_painted_planks/old_painted_planks` |
| Main graph | `1584841700` | `sbs:lib:paper_005/paper_005` |
| Main graph | `1584841869` | `sbs:lib:rough_concrete_with_lines/rough_concrete_with_lines` |
| Main graph | `1584842120` | `sbs:lib:tiles_002/tiles_002` |
| Main graph | `1584842397` | `sbs:lib:tiles_008/tiles_008` |
| Main graph | `1584842671` | `sbs:lib:tiles_011/tiles_011` |
| Main graph | `1584842975` | `sbs:lib:tiles_025/tiles_025` |
| Main graph | `1584843264` | `sbs:lib:wood_american_cherry/wood_american_cherry` |
| `Substance_graph/1584843868/outputsize` | `1584844962` | `sbs:fn:pow` |
| `Substance_graph/1584843868/outputsize` | `1584844985` | `sbs:fn:while` |

## 4. Embedded SVG decoding failure

- Node: `1584633070` (`sbs:svg`).
- Resource: `Untitled_Vector_Graphics.svg`.
- Error: `not an SVG document (no <svg> element)`.

The artwork is present in the SBS. Base64 decoding produces 123 bytes: a four-byte big-endian uncompressed-length prefix followed by zlib data. Independently decompressing the stream after that prefix produces a valid 157-byte XML document with an SVG root, matching the declared length.

The resource's `<binembedded>` element does not declare its encoding with a `<format>` child. [The resource reader](../../src/sbs.rs) currently defaults that case to raw base64, so the compressed payload reaches the SVG decoder unchanged. This is an embedded-resource decoding gap, not an absent image file. The decompressed art was inspected in memory only.

## 5. Incomplete fixture graphs and ignored dynamic behavior

| Location | Finding | Implication |
| --- | --- | --- |
| Value Processor `1584633100` | Its `<compFilter>` has an empty `<parameters/>` and no function body. Rendering fails with `value_processor needs fn="function"`. | This node cannot demonstrate working value processing as authored. It needs a usable function to test that behavior. |
| `Substance_graph/1584843868/outputsize` | 237 nodes, no valid result node; `pow` and `while` also lack editor definitions. | Serialization preserves the graph, but this fixture cannot demonstrate complete function evaluation. |
| Blend `1584843868` | Its function-driven output size is ignored during conversion. | This is an implementation limitation independent of repairing the missing function result. The “exact” count does not include it as an approximate node. |
| Pixel Processor `1584633054` | Imported per-pixel function is a constant. | A successful texture here does not exercise a nontrivial pixel-processor program. |

## 6. Specific approximate behavior: 39 diagnostics

These warnings are informational in the existing test; they are not assertions requiring pixel-perfect parity. They identify altered behavior, omitted inputs or settings, approximate units, and cases awaiting reference comparison. The full mapping appendix also lists the 284 routine implementation-substitution diagnostics.

| Node UID | Native kind | Behavior warning |
| --- | --- | --- |
| `1359211383` | `sbs:normal` | values are converted to the units of MaterialCraft's Normal From Height node; check the result |
| `1584632887` | `sbs:blur` | blur strength is converted (intensity / 1.5 as a Gaussian deviation); compare with a reference render |
| `1584632919` | `sbs:curve` | curve tangents are not carried over; the curve passes through the same points |
| `1584632925` | `sbs:dirmotionblur` | blur length is converted from pixels at 256 |
| `1584632931` | `sbs:directionalwarp` | values are converted to the units of MaterialCraft's Directional Warp node; check the result |
| `1584632937` | `sbs:distance` | values are converted to the units of MaterialCraft's Distance node; check the result |
| `1584632943` | `sbs:emboss` | values are converted to the units of MaterialCraft's Emboss node; check the result |
| `1584632949` | `sbs:fxmaps` | drawn by MaterialCraft's quad-tree engine: the layout follows the file, random numbers differ |
| `1584632980` | `sbs:hsl` | saturation and lightness are converted to multipliers |
| `1584633042` | `sbs:normal` | values are converted to the units of MaterialCraft's Normal From Height node; check the result |
| `1584633054` | `sbs:pixelprocessor` | the per-pixel function is a constant |
| `1584633061` | `sbs:sharpen` | values are converted to the units of MaterialCraft's Sharpen node; check the result |
| `1584633070` | `sbs:svg` | values are converted to the units of MaterialCraft's SVG node; check the result |
| `1584633077` | `sbs:text` | the font is not carried over; the built-in face is used |
| `1584633100` | `sbs:valueprocessor` | the value processor has no function |
| `1584633107` | `sbs:warp` | values are converted to the units of MaterialCraft's Warp node; check the result |
| `1584687025` | `sbs:lib:3d_perlin_noise/3D_perlin_noise` | without a position map the noise is read on the texture plane |
| `1584687027` | `sbs:lib:3d_perlin_noise/3D_perlin_noise_fractal` | without a position map the noise is read on the texture plane |
| `1584687029` | `sbs:lib:3d_perlin_noise/3D_ridged_noise_fractal` | without a position map the noise is read on the texture plane |
| `1584687031` | `sbs:lib:3d_simplex_noise/3d_simplex_noise` | without a position map the noise is read on the texture plane |
| `1584687033` | `sbs:lib:noise_voronoi/3d_voronoi` | without a position map the noise is read on the texture plane |
| `1584687035` | `sbs:lib:noise_voronoi/3d_voronoi_fractal` | without a position map the noise is read on the texture plane |
| `1584687037` | `sbs:lib:3d_worley_noise/3D_worley_noise` | without a position map the noise is read on the texture plane |
| `1584687103` | `sbs:lib:noise_fur_1/fur_1` | fur is drawn with MaterialCraft's own hairs; the look is similar, not identical |
| `1584687105` | `sbs:lib:noise_fur_2/fur_2` | fur is drawn with MaterialCraft's own hairs; the look is similar, not identical |
| `1584687107` | `sbs:lib:noise_fur_3/fur_3` | fur is drawn with MaterialCraft's own hairs; the look is similar, not identical |
| `1584687191` | `sbs:lib:rt_caustics/rt_caustics_color` | the caustics are MaterialCraft's own pattern, not traced through the input surface |
| `1584687193` | `sbs:lib:rt_caustics/rt_caustics_grayscale` | the caustics are MaterialCraft's own pattern, not traced through the input surface |
| `1584687261` | `sbs:lib:gradient_axial/gradient_axial_reflected` | the reflected axial gradient is not yet checked against a reference render |
| `1584687287` | `sbs:lib:pattern_scratches_generator/scratches_generator` | the scratches are drawn with MaterialCraft's own strokes |
| `1584687324` | `sbs:lib:pattern_star/star` | the star is not yet checked against a reference render |
| `1584687334` | `sbs:lib:pattern_tile_random/tile_random` | random tiles follow the same rules with their own random choices |
| `1584687339` | `sbs:lib:pattern_tile_random/tile_random_color` | random tiles follow the same rules with their own random choices |
| `1584687343` | `sbs:lib:pattern_tile_sampler/tile_sampler_color` | blending mode 2 is assumed to be our 'Add' mode |
| `1584689879` | `sbs:lib:histogram_scan_non_uniform/histogram_scan_non_uniform` | the position and contrast maps are not used |
| `1584756604` | `sbs:lib:emboss_with_gloss/emboss_with_gloss` | the gloss and the light colours are not reproduced |
| `1584760342` | `sbs:lib:glow/glow` | the glow's size and strength are converted approximately |
| `1584760349` | `sbs:lib:glow/glow_grayscale` | the glow's size and strength are converted approximately |
| `1584843868` | `sbs:blend` | a function-driven output size is not used |

## Suggested follow-up

1. Implement or map the 207 unconverted library nodes; the appendix separates missing editor definitions from missing renderer support.
2. Decode the length-prefixed compressed embedded SVG before passing it to the SVG parser.
3. Define the missing editor nodes, including `pow` and `while`, and address ignored function-driven output sizes.
4. Supply usable fixture function bodies/results and resolve the 35 external image dependencies to enable broader rendering checks.
5. Compare approximate mappings against reference renders, prioritizing ignored curve tangents, font choice, non-uniform histogram inputs, emboss gloss/light colors, and dynamic output sizing.

The strict compatibility test remains failing. No failures were ignored, no expected-missing counts were accepted as success, and no application code or fixture data was changed during this report run.

## Evidence and interpretation

The fresh Rust test supplied the failure categories, UID-specific errors, and preservation/render results. A temporary read-only diagnostic program, linked against the release library produced by that run, collected all import notes and node mappings because the test intentionally omits routine substitution messages from its console output. Mapping totals agree between those two sources. The SVG payload and output reachability were independently checked from the fixture XML.

The complete mapping appendix is a snapshot of this fixture and run, not a claim about every possible SBS package or configuration.
