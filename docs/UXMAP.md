# MaterialCraft UX Map

A guide to finding controls and completing common tasks in the editor.
Node types, inputs, and parameters are documented in [NODES.md](NODES.md).

- [1. Common tasks](#1-common-tasks)
- [2. Keyboard and mouse shortcuts](#2-keyboard-and-mouse-shortcuts)
- [3. File formats](#3-file-formats)
- [4. Command line](#4-command-line)
- [5. Screens](#5-screens)
- [6. Home screen](#6-home-screen)
- [7. Editor layout and menus](#7-editor-layout-and-menus)
- [8. Panels](#8-panels)
- [9. Windows and dialogs](#9-windows-and-dialogs)
- [10. Feedback and saved state](#10-feedback-and-saved-state)

Use Cmd in place of Ctrl on macOS. Menu paths use the labels shown in the app.
For installation, see [README.md](../README.md); for automation, see
[AI_CONTROL.md](../AI_CONTROL.md); for planned work, see [ROADMAP.md](ROADMAP.md).

## 1. Common tasks

| Task | Where to start |
|---|---|
| Create a material | Home > New graph, File > New > Material Graph, or Ctrl+N. Choose a template, name, package, size, and output format. |
| Open a package | File > Open package(s), Ctrl+O, a recent-file card, or drop an .sbs file on the window. |
| Try a finished material | Home > TEMPLATES or File > Templates and examples. |
| Add a node | Tab over the graph, drag a Library tile, or right-click the graph > Add node. |
| Connect nodes | Drag between output and input sockets. Drag into empty space to open node search and auto-connect. |
| Change a parameter | Select a node and use Properties. See [the node reference](NODES.md) for its parameters. |
| Inspect a result | Select a node to view its result in 2D View. The material outputs feed 3D View. |
| Check a tile seam | 2D View > Tile 2x2 preview. |
| Change working resolution | Canvas toolbar > Parent Size, or Properties > GRAPH SETTINGS > Output Size. Unlink width and height for a non-square graph. |
| Organize a graph | Select nodes and press C for a comment frame, or use Tools > Auto-layout. |
| Reuse selected nodes | Edit > Create subgraph from selection; double-click the resulting subgraph to edit it. |
| Reuse a graph from another package | Drag it from Explorer, or File > Package > Link package as node for a live link. |
| Expose a parameter | Click P beside the parameter in Properties. Edit its label and range under PUBLISHED PARAMETERS. |
| Drive a parameter with a function | Click fn beside the parameter and choose a function graph. |
| Edit a spline | Select a path node, then Properties > Edit spline. Edit points in 2D View and press Done. |
| Manage imported files | Tools > Resources. |
| Bake maps from meshes | Tools > Mesh map baker. |
| Export textures | File > Export textures. Choose a folder, format, resolutions, and outputs. |
| Save your work | Ctrl+S; File > Save as for a new path. |
| Recover a closed panel | Windows > the panel name. Windows > Reset layout restores the default layout. |
| Automate the open graph | Tools > AI control; follow [AI_CONTROL.md](../AI_CONTROL.md). |

### Examples, embedded copies, and live links

- A template opened as an **example** gets its own tab. Reopening the same example selects that tab.
- Examples are separate from the project and do not enter the recent-file list.
  Use **Add to project**, **Save as**, or File > Package > Add template to keep your work.
- Ready-made generators and filters dragged from Library become instances of an embedded template.
  Right-click the instance > Edit template to change the shared copy in that package.
- A **live-linked package** follows an external file. Use File > Package > Dependencies to relink it,
  embed a copy, or remove the link. Refresh linked packages rereads the external files.

## 2. Keyboard and mouse shortcuts

Graph shortcuts are suspended while typing in a text box or using the New dialog.
Help > Keyboard shortcuts opens the in-app reference.

| Shortcut | Action |
|---|---|
| Ctrl+N / Ctrl+O / Ctrl+S | New material graph / open packages / save |
| Ctrl+Z | Undo |
| Ctrl+Y or Ctrl+Shift+Z | Redo |
| Ctrl+C / Ctrl+X / Ctrl+V / Ctrl+D | Copy / cut / paste / duplicate nodes |
| Ctrl+A | Select all nodes |
| Delete or Backspace | Delete the selection (nodes and selected links) |
| Click a link (Shift-click adds) | Select the link |
| X | Swap connections: the two selected links, one selected link with the next input of its node, or the first two inputs of the selected node |
| Tab over the graph | Search for a node at the cursor |
| F | Frame all nodes |
| C | Add a comment frame around the selection |
| Shift-click or Ctrl-click | Toggle a node in the selection |
| Drag empty graph space | Rubber-band select |
| Click empty graph space | Clear selection |
| Middle-drag, Alt-drag, or Space-drag | Pan the graph |
| Mouse wheel over the graph | Zoom around the cursor |
| Double-click a subgraph | Open its contents |
| Up/Down or Page Up/Page Down in node search | Move the highlighted result |
| Enter / Esc in node search | Add the result / close search |

| View | Mouse action | Result |
|---|---|---|
| 2D View | Wheel / drag / double-click | Zoom / pan / fit to view |
| 3D View | Drag | Orbit the camera |
| 3D View | Alt-drag or middle-drag | Move the camera |
| 3D View | Shift-drag | Move the light |
| 3D View | Wheel | Zoom |

While editing a spline: click to add/select a point, drag points or handles, and Delete to remove
the selected point. Double-click a point to switch corner/smooth behavior. Alt-drag a handle to
move it independently; Shift temporarily flips grid snapping. Use Alt-, Space-, or middle-drag to pan.

## 3. File formats

### Files you work with

| File | Purpose and entry point |
|---|---|
| `.sbs` | Substance XML package containing graphs, functions, links, and resources. Open/save from File. |
| `.png, .jpg, .tif, .exr, .hdr, .webp, .dds, .tga, .bmp` | Bitmap input; drop on the graph, File > Import bitmap, or browse from a Bitmap node. Format is recognized from file contents. |
| `.svg` | Vector input; drop on the graph or browse from an SVG node. |
| `.ttf, .otf, .ttc, .woff` | Font input for the Text node. |
| `.cube, .3dl, lookup-table images` | Input for the Color LUT node. |
| `.obj, .gltf, .glb, .ply, .stl` | Mesh input for the baker or 3D View > Scene > Load a mesh from a file. A baking target needs UVs. |
| `.bakeset` | Mesh baker file containing a mesh list and settings. |
| `bake/<set>/<map>.png` | Baked map embedded in the package; bake/<set>.cfg stores its settings. |

See [README.md](../README.md#importing-files-and-managing-resources) for supported image variants
and [NODES.md](NODES.md) for node-specific import controls.

### Graph and spline resources

An .sbs package contains compositing graphs, function graphs, FX-Maps, parameters,
connections, and resource references in XML. MaterialCraft preserves the original package
and rewrites the parts changed by edits.

Spline resources are stored inside the package, usually with a .spl name. Their text format is:

~~~text
spline 1
path closed=0 smooth=1
pt 0.2 0.5
pt 0.5 0.2 hin=-0.1,0 hout=0.1,0 w=1.5 z=1 tan=free
~~~

| Field | Meaning |
|---|---|
| `path closed=0\|1 smooth=0\|1` | Begin a path and set closure/automatic smoothing. |
| `pt x y` | Point in texture coordinates: 0..1, with y pointing down. |
| `hin=dx,dy / hout=dx,dy` | Optional handle offsets relative to the point. |
| `w=factor` | Optional width multiplier. |
| `z=height` | Optional height in 0..1; default 1. |
| `tan=auto\|mirrored\|free\|corner` | Optional tangent mode; also accepts 0, 1, 2, or 3. |
| `#` | Start a comment. |

A spline can contain several paths. Smooth paths give points automatic handles when none are supplied.
The package stores the text as a resource named, for example, name.spl.

### Exports and app data

- Texture exports: PNG (8/16-bit), JPEG (8-bit), TIFF (8/16-bit or 32-bit float),
  EXR (16-bit half or 32-bit float), and HDR (RGBE).
- Export names use the selected prefix, output identifier, and extension.
  Selecting several resolutions creates a subfolder for each size.
- config.toml stores recent files, favorites, panel layout, and home-screen choices in the user config folder.
  A .bak holds the previous settings; damaged settings are moved aside before defaults are used.
- Legacy recent.txt and settings.txt are imported when migrating older settings.

## 4. Command line

Use `materialcraft --help` for the command summary, `materialcraft bake --help` for baking,
`materialcraft render3d --help` for scene rendering, and `materialcraft commands` for automation commands.

A complete first render:

~~~sh
materialcraft example out/example
materialcraft render out/example/brick_wall.sbs out/maps --size 1024
~~~

| Command | Purpose |
|---|---|
| `materialcraft render <graph> <outdir>` | Export graph outputs; choose resolutions, formats, parameters, and variations. |
| `materialcraft render3d <graph> <out.png>` | Render a scene image or turntable. |
| `materialcraft check <graph> / materialcraft params <graph>` | Inspect a graph or its exposed parameters. |
| `materialcraft graphs <package.sbs>` | List package graphs; `--export <graph> <out.sbs>` saves a graph with its dependencies. |
| `materialcraft bake --low <mesh> --out <dir>` | Bake mesh maps; add optional high-poly meshes, a cage, and map settings. |
| `materialcraft testmesh <outdir>` | Write demonstration low/high mesh files. |
| `materialcraft nodes` | Print node inputs and parameters; see [NODES.md](NODES.md) for the reference. |
| `materialcraft-gui [file.sbs]` | Open the editor, optionally with a package. |

For MCP, exec, serve, and the live editor bridge, see [AI_CONTROL.md](../AI_CONTROL.md).

## 5. Screens

The app has a Home screen and an Editor screen. Home opens at startup unless
**Skip this screen on startup** is selected. Use File > Home screen to return,
or **GO TO EDITOR** to enter the editor with the current graph.

## 6. Home screen

| Tab or control | What it does |
|---|---|
| START | Create a graph, open a package, open the mesh baker, or reopen a recent file. |
| TEMPLATES | Search finished graphs and filter by material, generator, or filter. Click to open an example. |
| LEARN | Short getting-started guide, brick-wall example, and keyboard shortcuts. |
| Search recent files | Filter the recent-file cards by filename. |
| × on a recent-file card | Remove the recent entry while keeping the file on disk. |
| Skip this screen on startup | Save a preference to start in the Editor. |
| AI CONTROL ON | Indicates that the local editor connection is enabled. |

Recent files show their path on hover. Missing files stay visible with FILE NOT FOUND;
remove their entries or open the file at its new location. Ctrl+N and Ctrl+O also work on Home.

## 7. Editor layout and menus

The editor contains package tabs, graph tabs, a toolbar, the workspace panels, and a status bar.
Package tabs select the active package; graph tabs select its main graph or an opened subgraph.
An asterisk marks unsaved changes.

The default workspace places Explorer and Library on the left, Graph in the center,
2D/3D views below it, and Properties on the right.

- Drag a panel tab to another group, an edge, or outside a group to dock, split, or float it.
- Drag dividers to resize groups; use the tab menu to move or close a panel.
- Graph stays fixed. Windows reopens closed panels; Windows > Reset layout restores defaults.
- Layout changes are remembered between runs.

| Menu | Main destinations |
|---|---|
| File | New, Open package(s), Save, Save as, Templates and examples, Import bitmap, Export textures, Home screen |
| File > Package | Add template, Link package as node, Refresh linked packages, Dependencies |
| Edit | Undo/redo, clipboard operations, selection, Add node, Comment frame, Create subgraph from selection |
| Tools | Mesh map baker, Resources, Frame all, Auto-layout, AI control, GPU acceleration |
| Windows | Panel visibility and Reset layout |
| Help | Keyboard shortcuts |

## 8. Panels

### Explorer

- Lists open packages, their graphs, outputs, and resources.
- Click a graph to open it; drag a graph to the canvas to reuse it.
- Right-click empty space to create/open packages or create a material graph.
- A package's menu offers graph creation, save, and close.
- Closing a modified package offers Save, Discard changes, and Cancel.

### Library

- Search matches node names/descriptions and ready-made graph tags.
- Use folders to browse, or Favorites for frequently used nodes.
- Drag a tile onto Graph to add it; right-click a node tile to change its favorite status.
- Generators/Ready-made and Filters/Ready-made contain reusable graphs.
  Right-click one > Open as example to inspect it separately.
- Material examples are available in Home > TEMPLATES.
- For the complete node catalog, use [NODES.md](NODES.md).

### Graph

- Drag selected cards to move them together. Hover sockets for their names and errors for explanations.
- Drag a connected input to detach/reconnect it; drop in empty space and close search to disconnect.
- Shift-drag an output to move its links to another output; incompatible links remain in place.
- Cycles and incompatible socket types are rejected.
- Right-click nodes for copy, duplicate, delete, subgraph/template editing, and Trace to spline.
- The canvas toolbar provides Frame all, Auto-layout, Comment frame, Create subgraph, and Parent Size.
- Inside a subgraph, < Back returns to its parent.

### 2D View

- Displays the selected node's image, dimensions, zoom, and value range.
- Fit to view or double-click to fit; Tile 2x2 preview checks edges.
- Values outside 0..1 produce an HDR warning. Use a floating-point export format to retain that range.
- Edit spline adds controls for paths, closure, smoothing, snapping, point width/height, and tangent mode.
  Done leaves spline editing; edits participate in undo/redo.

### 3D View

The material outputs feed the preview. Add a height output to use height-based relief.

| Menu | Controls |
|---|---|
| Scene | Preview shape, UV tiling, mesh resolution, wireframe, scene layout, and loading a custom mesh |
| Materials | Height display, scale, neutral level, bump strength, and normal strength |
| Lights | Rig presets, adding/removing lights, light type, color, intensity, position, and shadows |
| Camera | Presets, camera position, field of view, reset, and turntable |
| Environment | HDR image or preset, rotation, brightness, background, and blur |
| Post | Tone mapping, exposure, bloom, SSAO, depth of field, grading, and vignette |
| Channel | Lit material or individual material channels |
| Renderer | GPU/CPU rendering, progressive CPU path tracer, picture export, and turntable frames |

Path tracing restarts when the scene changes. Renderer > Save picture provides raster
and path-traced exports; Save turntable frames writes a sequence.

### Properties

- With no node selected, shows graph size, precision, seed, and metadata.
- With a node selected, shows its inputs, parameter controls, output size, and output format.
- Lower per-node resolution can reduce the cost of expensive operations.
- P exposes a parameter; fn assigns a function graph.
- FUNCTIONS AND PATTERN PROGRAMS creates/opens those graphs.
- PUBLISHED PARAMETERS lists exposed labels and ranges, with a remove control.
- Selecting a comment frame exposes its title, color, body, and delete control.

Use [NODES.md](NODES.md) for node-specific controls and parameter descriptions.

## 9. Windows and dialogs

### New graph or package

Choose a workflow template, graph name, destination package, initial width/height, and output format.
Unlink dimensions for a non-square graph. RESET restores the defaults; CANCEL leaves the current work.
File > New > Empty Package uses the package variant of this dialog.

### Export textures

1. Choose the output folder and optional filename prefix.
2. Pick PNG, JPEG, TIFF, EXR, or HDR and the format's depth/quality settings.
3. Choose an output color space. Conversion applies to color outputs; data maps keep their values.
4. Select resolutions and outputs, then Export.

Errors appear in the status message. Multiple resolutions get separate folders.

### Mesh map baker

1. Open Tools > Mesh map baker and name the bake set.
2. In Meshes, add low-poly targets, optional high-poly detail, and an optional cage.
3. In Settings, choose resolution, anti-aliasing, padding, ray distances, matching, UV set, and normal convention.
   Preview/Game/Final provide starting presets.
4. In Maps, select the required maps.
5. In Output, choose package storage, creation of Baked Map nodes, and/or external PNG files.
6. Bake; use Cancel to stop. Review coverage, warnings, and map previews.

Load/save .bakeset files to reuse setup. Existing package bake sets retain their settings.
Re-bake when a mesh file changes is optional. For available maps and settings, see
[NODES.md: Mesh baking](NODES.md#mesh-baking).

### Package dependencies

File > Package > Dependencies manages live-linked packages. Return to the package root before editing links.

- Relink selects a new path.
- Embed copy stores a local copy; it requires the source file to be available.
- Remove drops the link.
- Search folders supplies fallback locations; Refresh links rereads the files.
- Linking a file with several graphs opens Link which graph to choose the source graph.

### Resources

Tools > Resources manages images, fonts, vector art, lookup tables, splines, and baked maps.

- Select a resource to see its preview, origin, status, and the nodes that use it.
- Embed copies a linked resource into the package; Extract to folder writes embedded data out.
- Relink changes a file reference; Replace contents keeps the embedded name while replacing its data.
- Update from source refreshes an embedded resource from its original file.
- Rename changes an embedded resource's name; Delete is offered when it is unused.
- Bulk actions: Embed all linked, Collect into project folder, Update from source files, Remove unused.
- Missing resources are flagged. Relative files are searched beside the package and in its resources folder.

### AI control and shortcuts

Tools > AI control turns the editor connection on/off and displays assistant setup commands.
It is off by default; [AI_CONTROL.md](../AI_CONTROL.md) covers setup and CLI alternatives.

Help > Keyboard shortcuts opens the shortcut reference. Native file/folder pickers follow the operating system.

## 10. Feedback and saved state

- Evaluation runs in the background; previews and thumbnails update as work completes.
- The status bar reports work in progress, the active compute backend, and success/error messages.
- Tools > GPU acceleration controls supported node/baker compute. The 3D View Renderer menu controls the viewport.
- Unsaved packages show an asterisk. Closing a modified package prompts for a decision.
- Saving a package preserves its graph edits and embedded resources.
- Settings retain recent files, favorites, and layout. A missing recent file remains listed until removed.

When updating this guide, describe current controls and workflows. Keep node catalogs in
[NODES.md](NODES.md), automation setup in [AI_CONTROL.md](../AI_CONTROL.md), and future work in
[ROADMAP.md](ROADMAP.md).
