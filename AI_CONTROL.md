# AI control and automation

This guide explains MaterialCraft's optional automation interface for external AI assistants and scripts.
It lets them add and connect nodes, set parameters, inspect graphs, and render results through the
Model Context Protocol (MCP), CLI commands, or a connection to the running editor. In the editor, changes
appear in the graph as they happen, and each edit is one undo step (Ctrl+Z).

The live editor connection is off by default. Enable it from Tools > AI control or start the editor with
`--control`. The commands also work without an editor window, as described below.

## Quick start with the editor open

1. Tools > AI control... > Turn on.
2. Claude Code: copy the shown command and run it once in a terminal:
   `claude mcp add materialcraft -- materialcraft mcp --bridge`
   Other assistants: paste the shown JSON into their MCP settings.
3. Ask: "Build a mossy cobblestone material, check that it tiles, and fix what looks wrong."

Start the editor with `--control [PORT]` to turn it on automatically.

## Without the editor

    materialcraft mcp --graph wall.sbs --root ./work     # MCP on stdin/stdout, package in memory
    materialcraft serve --graph wall.sbs                 # keep a package and accept commands on a local port
    materialcraft exec node_add kind=sbs:blur params.intensity=4 name=soft --graph wall.sbs --save
    materialcraft commands render                        # list commands matching a word

## Commands

`materialcraft commands` lists all of them with their parameters. Groups: `graph.*` (new, open, save, list,
focus, create, extract, rename, delete, duplicate, add_file, inspect, outline, validate, layout, set, expose,
set_input), `node.*` (types, describe, add, set, drive, expression, connect, remove (`node` or `nodes`), duplicate, ...), `macro.add`,
`output.*`, `render.*` (node, outputs, sheet, 3d, review: pictures plus statistics such as flatness and tiling
seams), `resource.list`, `edit.*` (undo, redo, history), `export`, and `batch` (many edits, all or nothing, one
undo step, nodes referred to by name).

An assistant works on `.sbs` packages exactly as the editor does: a new session is an empty package, `node.types`
lists the format's own nodes (texture, function and pattern families), values are set with their types, any value
can be computed by a function graph (`node.drive`, then build the function with `sbs:fn:*` nodes and
`node.set_result`), parameters are exposed with `graph.expose` and changed with `graph.set_input`, and the
package's graphs and functions are made, used as nodes (`sbs:graph`), renamed, copied and removed with the
`graph.*` commands.

Mistakes come back as plain explanations (unknown parameter with a suggestion, allowed choices, value clamped).

### Seeing and checking the work

* `render.review` is the one-look check: a picture with the lit 3D preview, an optional reference picture next
  to it, every output map, and the node graph with a thumbnail in each node. Its data holds the graph as text
  (`outline`), statistics and a tiling check per map, problems, warnings and approximations. With
  `reference=<image>` it measures how the material differs from the reference (brightness, colour, contrast, fine
  detail, how many features repeat across, a similarity score) and says in words what to change. The same sheet
  from the command line: `materialcraft review wall.sbs review.png --reference photo.jpg --json review.json`.
* `graph.outline` is the graph as compact text: one line per node with its changed values (numbered choices with
  their names) and where each input comes from, then the outputs.
* `render.node` gives a picture and statistics of one node; for Flood Fill and the nodes reading it, also the
  number of regions and a warning when shapes merged (gaps too thin at that size). Its tiling check compares the
  step across the tile border with the largest step inside the image, so banded or tiled images are not
  mistaken for seams.
* `graph.set view={...}` stores the 3D settings (tiling, displacement, ...) in the material, so `render.3d` and
  `render.review` use them until a call overrides them; `graph.set default_format=16` gives every new texture node
  that bit depth (8, 16, "16f", 32f). Displacement is tessellated finer as tiling grows, and maps are
  prefiltered when texels are smaller than pixels, so normal maps no longer sparkle.
* `render.review` also takes the 3D options of `render.3d`, `crop` (a box `{x,y,w,h}` or automatic: the reference
  is cropped to the object) and `match_lighting` (on by default: the render is scaled to the reference's
  brightness before colour, contrast and detail are compared).
* `render.3d` takes every view option: environment preset and rotation, light rig and intensity, exposure,
  camera, texture repeat (`tiling`), height displacement amount and relief mode, normal strength, post look, and
  any other view setting by key (`settings`). `list=true` returns all the names. The path tracer needs about 128
  samples before rough surfaces stop looking grainy.
* `graph.validate` returns problems (must fix) and warnings (nodes whose result reaches no output).

### Parameters

`node.describe` lists every parameter with its default, slider range, unit (angles are turns: 0.25 = 90 degrees)
and, for numbered settings of library nodes (Tile Generator pattern, blending modes, Voronoi style, normal
format ...), the name of each number as MaterialCraft draws it (`src/sbs_choices.txt`). Numbered settings can be
set by number or by name (`"pattern": "Bell"`). Slider ranges are not limits: larger values are accepted. The
Gradient Map's colour stops are the `gradient_keys` parameter. Notes point out what is easy to miss (the Distance
node works in pixels of a 256-pixel image; only whole-number zooms keep a texture tiling).

### Math as text

`node.expression` turns an expression into the function graph of a value (ordinary `sbs:fn:*` nodes, saved in the
file): any parameter, the Pixel Processor's per-pixel function (`param=perpixel`; `sample($pos)` reads its first
input) or a package function (`function=<name>`). Example:
`p = $pos - 0.5; r = length(p); r < 0.3 ? 1 : smoothstep(0.5, 0.3, r)`. The language has + - * / %, comparisons,
&& || !, `c ? a : b`, repeatable random numbers `hash(x)`, `hash2(vec2)` and `cellrand(cell, seed)`, components (`v.xy`), vec2/vec3/vec4, the usual math functions, `$pos`, `$size` and the
graph's inputs by identifier.

### Building blocks

`macro.add` inserts tested groups of standard nodes for jobs without a single node: `grow` / `shrink` / `outline`
a mask by pixels, `star` with any number of points, `polygon`, `tile_shape` (your own shape repeated by a Tile
Generator, for interlocking tiles), `strokes` (directional brush strokes per cell), `tile_scale` (a whole-number
zoom for patterns without a scale setting), `bricks` (rows, running or stacked bond, mortar, bevel, half and double
bricks, chips), `cracks` (tapering cracks, soft ends), `plates`, `fractured_rock`, `moss` (clumped leaves),
`strata` (wavy layers), `edge_chip` (chip mask plus a band around each chip), `paint_chip` (chip mask with
highlight and shadow bands), `grow_organic` (an uneven grow) and `flood_gray` (a Flood Fill's random greys with a
luminance range and contrast). Several of them return more than one node: see `outputs` in the result.
`polygon` takes `bevel`. `macro=list` shows their options.

### Several graphs

`graph.list` shows the package's graphs. Every graph and node command takes `graph=<name>`; `graph.focus` picks
the graph later commands work on. `graph.create` makes a texture, material or function graph;
`graph.extract nodes=[...] name=<new>` moves chosen nodes into a new graph and puts one node running it in their
place (links from outside become its inputs, links to outside its outputs; node names move along), so an
assistant told to "put the brick shape in its own graph" does it in one step.

## Security

* The port listens on 127.0.0.1 only and requires a random access token, regenerated each time it is turned
  on and stored in a user-only file (`control.json` in the MaterialCraft config folder).
* Only JSON requests are accepted, so a web page cannot talk to the port.
* `--root DIR` limits every file an assistant can read or write to that folder.
* Turn the feature off from the same window; nothing listens while it is off.
