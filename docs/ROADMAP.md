# MaterialCraft Roadmap

Roadmap items are listed in no particular order. Milestone tracking will be available directly in GitHub.

## Initial Release

- [x] Use `.sbs` as the standard project format, interoperable with [Adobe Substance 3D Designer](https://experienceleague.adobe.com/en/docs/substance-3d-designer/using/getting-started/overview/overview).
- [x] Support all native Adobe Substance 3D Designer nodes, including [atomic and library nodes](https://experienceleague.adobe.com/en/docs/substance-3d-designer/using/substance-graphs/nodes-reference-for-substance-graphs/nodes-reference-for-substance-compositing-graphs) and [function nodes](https://experienceleague.adobe.com/en/docs/substance-3d-designer/using/substance-function-graphs/nodes-reference-for-substance-function-graphs/function-nodes-overview).
- [ ] Provide a complete workflow from project creation through editing, previewing, saving, and export, with minimum necessary keyboard shortcuts, buttons, menus, and panels.
- [x] Have sample HDRI panoramas from [PolyHaven](https://polyhaven.com/)
- [ ] Export [MaterialX](https://materialx.org/Specification.html) (`.mtlx`) and image files only, with controls matching [Adobe Substance 3D Designer](https://experienceleague.adobe.com/en/docs/substance-3d-designer/using/substance-graphs/exporting-bitmaps)
- [ ] Verified [Unreal Engine support](https://dev.epicgames.com/documentation/unreal-engine/importing-materialx-files-in-unreal-engine).

Note: Initial MaterialX exports may not fully reproduce advanced node graph behavior compared to Adobe `.sbar` format which is proprietary*

## ArtCraft Ecosystem

- [ ] Build on [craft-libs](https://github.com/storytold/craft-libs) for shared controls, settings, color management, safe saving, translations, and AI features; contribute reusable improvements upstream.
- [ ] Follow ArtCraft conventions for installation, launcher support, updates, and user experience.
- [ ] Become an official [ArtCraft application](https://getartcraft.com/apps).

## Optimized MaterialX Graphs

- [ ] Simplify exported [MaterialX graphs](https://github.com/AcademySoftwareFoundation/MaterialX/blob/main/documents/Specification/MaterialX.Specification.md): precompute constant values, simplify math, reuse repeated calculations, and remove unused nodes.
- [ ] Best match proprietary `.sbsar` functionality for downstream applications.
- [ ] Preserve editable parameters and input dependencies; compare optimized and original results across parameter ranges, with documented accuracy limits.
- [ ] Bridge unsupported operations with baked textures, lookup tables, or application adapters; explain any resulting parameter or resolution limits.
- [ ] Show required assets, processing time, memory use, file size, and changes in appearance.

## Application Integrations

- [ ] Export functionality to Unity, Unreal Engine, Godot, Blender, Houdini, Maya, 3ds Max, etc. using native [MaterialX support](https://materialx.org/ThirdPartySupport.html) or adapters.
- [ ] Test material channels, colors, normal maps, parameters, and rendering for each supported application and renderer version; preserve user edits when updating materials.
- [ ] Read and write [OpenUSD](https://openusd.org/release/intro.html) scenes, combine scene layers, and assign MaterialX materials with their textures.
- [ ] Support for sending assets to PhotoCraft, VectorCraft, and other ArtCraft tools for editing, then refresh them without losing links or color information.

## Graphics and Performance

- [ ] Profile and optimize CPU/GPU work across graph evaluation, baking, the editor canvas, thumbnails, and 2D/3D previews; set frame-time and memory targets.
- [ ] Keep more work on the GPU, combine compatible compute steps, update only affected nodes, and limit cache memory.
- [ ] Benchmark large graphs and high-resolution assets on NVIDIA, AMD, Intel, and Apple GPUs; check results, keep the interface responsive, and allow cancellation.

## Authoring and Extended Assets

- [ ] Add reusable templates, saved parameter presets, and shared material libraries with version history.
- [ ] Make large graphs easier to navigate with a minimap, bookmarks, dependency inspection, and tools to debug and profile individual nodes.
- [ ] Support multi-tile textures (UDIMs), texture atlases, channel packing, and batch creation of material variants.
- [ ] Guide users through turning scans into materials and checking physically based material values.

## Production Workflows

- [ ] Add OpenColorIO workflows and extend ICC support to keep colors consistent from source images to previews and exports.
- [ ] Improve crash recovery, collect assets for sharing, show graph changes between versions, and safely upgrade older projects.
- [ ] Improve configurable shortcuts, accessibility, high-DPI displays, translations, and getting-started examples.

## Command-line and AI Automation

- [ ] Build on the existing [command-line interface (CLI) and Model Context Protocol (MCP) support](../AI_CONTROL.md).
- [ ] Keep graph editing, validation, rendering, and export consistent across the editor, CLI, and MCP.
- [ ] Add resumable batch jobs and repeatable unattended exports, with progress reports, clear errors, and machine-readable results.

## Analytics

- [ ] Add opt-in analytics for feature usage, performance, and errors to guide application improvements (defaults off).
- [ ] Report aggregate trends without collecting project contents, asset names, or file paths.
- [ ] Fully transparent and public dashboards: every collected field and data point will be made public on Grafana dashboards.

## User Interface

- [ ] Add additional outstanding keyboard shortcuts, UI widgets, menus, etc.
- [ ] Add localization to major languages as well as a clean workflow for additional ones.
- [ ] Prioritize practical workflow improvements from community feedback and unmet [Adobe Substance 3D Designer feature requests](https://community.adobe.com/substance-3d-designer-47?content_type=idea).
- [ ] Reduce repetitive work in graph navigation and parameter editing.
- [ ] Provide [Physically Based](https://physicallybased.info/) reference values in material controls.
