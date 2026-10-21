# Test suite cleanup

Date: 2026-10-21

## Scope and result

Reviewed the inventory of all 27 integration test targets and 12 source test modules, scanned for repeated code and ineffective assertions, and compared overlapping checks before consolidating them.

The integration sources went from 8,182 to 7,687 lines and from 356 to 338 test functions, including feature-gated tests. The 28 source unit tests remain. Counts include the SBS tests added earlier in this session; fewer test functions do not mean fewer parameter cases.

The checked-in package is now [`tests/assets/all_nodes.sbs`](../../tests/assets/all_nodes.sbs). Its 475,305 bytes are unchanged (SHA-256 `880039bdf74e8e0842b319acabcdc1db2429af7f7bcbdde43da28f1ac2c6c730`). The generated SBS fixture directory was removed. Import, editing, resource recovery, and CLI tests use this package or temporary edited copies.

## Consolidations

| Previous overlap or bloat | Result |
| --- | --- |
| Generated SBS package, duplicate structure/save/render checks, optional private-package loop | The real package has one explicit import/export and compatibility target, `sbs_all_nodes`. Editing, malformed input, external dependencies, and FX-map regressions use copies of it. |
| Two all-renderer-node finite-output sweeps in `core` | Kept the stronger multi-resolution check, retained its inventory guard, and parse each node demo once. |
| PNG signature/round-trip and JPEG/TIFF header checks scattered through `core` | Consolidated into `decoders`, using the export API and checking decoded pixels. Kept PNG palette/alpha coverage and added PNG to the existing corruption matrix. |
| Separate bake and plain-render smoke setup | Moved requested-map and invalid-mesh assertions into `bake`; retained plain rendering assertions in the displacement test. |
| Mesh geometry checks in `core` | Moved intact to `meshes`, including displacement direction and welded sphere seams/poles. |
| Batch/undo checks repeated in `agent` and `agent_sbs` | Kept them in `agent`, with full package equality checks, while preserving SBS command serialization and function/graph management tests. |
| Optional installed-font smoke tests and separate font-description check | Kept the deterministic bundled TTF/OTF/WOFF/TTC coverage; merged description assertions into it. Tests no longer silently pass merely because a system font is absent. |
| Handwritten base64 encoder in an SVG test | Use the existing encoder for test resource setup. |
| PLY assertion ending in `or true` | Replaced with a real assertion that truncated ASCII PLY is rejected. |
| Historical milestone names and unused setup | Renamed `core` tests for their behavior and removed unused work. |

The original fixture's specific parameter values are no longer independent golden cases. Function execution, exposed parameters, graph management, and node behavior still have focused engine and command tests. The real catalog's mostly disconnected default nodes cannot replace those behavioral assertions by itself.

## Inventory decisions

| Test targets | Decision |
| --- | --- |
| `sbs_all_nodes`, `sbs_import`, `sbs_native`, `sbs_resources` | Consolidated package coverage around the real file. Keep focused mutations for editing, errors, FX maps, external graphs, and resource resolution. |
| `agent`, `agent_sbs` | Consolidated rollback checks. Keep transport/authentication, schema validation, native serialization, exposed values, and graph ownership checks. |
| `harness`, `refine` | Retained: command behavior, review sheets, macros, and specific regression cases differ from raw importer coverage. |
| `core`, `nodes3`, `nodes4`, `nodes5`, `nodes7` | Removed the duplicate generic sweep; retained numerical and parameter-sensitive checks. Short local rendering helpers do not warrant another shared fixture directory. |
| `seam` | Retained: wrapped-shift invariance and rectangular textures detect failures that finite-output checks miss. |
| `decoders`, `resources`, `vector` | Consolidated codec/font setup. Keep format decoding, resource lookup, LUTs, text layout, and SVG geometry as separate contracts. |
| `bake`, `meshes`, `meshio` | Consolidated smoke checks and geometry placement; retained baking, mesh topology, file-format, and path-safety tests. |
| `func`, `fxmap` | Retained: function typing, recursion rejection, and pattern execution are distinct from SBS conversion. |
| `perf` | Retained: these assert worker-pool behavior, cache reuse, and numerical equivalence; they are not redundant timing benchmarks. |
| `hdri`, `viewer3d` | Retained: HDR encoding, environment sampling, energy conservation, and rendered views exercise different boundaries. |
| `gpu` | Retained: CPU/GPU parity is a separate backend contract. Requires the feature and an available adapter. |
| 12 source test modules | Retained focused parser, serialization, metadata, and adapter checks. |

## Bitmap search

Missing bitmap lookup now searches the original path, package folders, then `assets` and `assets/panorama` beside the executable and in the working directory. Matching uses the exact filename. Each application's direct assets folder precedes its panorama folder; executable assets precede working-directory assets. Package-local matches take priority over both.

This explicitly includes `panorama`; it is not a recursive directory walk. Stored relative subpaths are also tried. Regression tests use generated pixels and copies of `all_nodes.sbs`, so no third-party artwork is added. They verify direct/native rendering, lookup priority, diagnostics, and unchanged exported resource paths.

## Validation and existing failures

The complete default-feature release run passed **347 tests**, with only the known compatibility audit filtered out and exercised separately:

```sh
cargo test --release --offline --locked --no-fail-fast -- --skip all_nodes_import_maps_and_renders_every_node
```

GPU and optional CMS feature tests were reviewed but were not enabled in this run. The largest remaining suite times were `agent` (40.91 s), `agent_sbs` (20.33 s), and `core` (16.73 s); these retain transport, hostile-input, and numerical/stress coverage. No before/after runtime improvement is claimed.

- The migrated SBS import, editing, and explicit round-trip tests passed.
- The focused decoder, resource, mesh, bake, and vector tests passed.
- The panorama regression failed before the resolver change and passed afterward; all 16 resource/bitmap tests passed afterward.
- Formatting checks for the migrated/consolidated files and `git diff --check` passed.
- The strict `all_nodes_import_maps_and_renders_every_node` test remains active and failing: 207 unsupported conversions, 28 unknown editor definitions, an embedded function without a result, and 37 node rendering errors (35 absent bitmap files, compressed SVG decoding, and an empty value processor). Renaming or finding panorama files does not implement unsupported nodes. Output rendering alone misses 553 disconnected nodes.
- An initial default debug test run, before cleanup, aborted in `xmldoc::tests::rejects_broken_documents_without_panicking` with a stack overflow. It passed in the release run. This debug-only observation was not removed or marked ignored.

Reproduce the explicit package checks with `cargo test --offline --locked --test sbs_all_nodes -- --nocapture`. The compatibility failure is intentional until its reported gaps are fixed; it has not been replaced by an accepted failure count.
