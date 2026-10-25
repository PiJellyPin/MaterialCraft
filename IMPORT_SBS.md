# Working with .sbs packages

`.sbs` is MaterialCraft's file format: the XML package format of another node-based material editor. MaterialCraft reads and writes
it from the format's published reference (Apache-2.0, see `THIRD_PARTY_LICENSES.md` and `INDEPENDENT_IMPLEMENTATION.md`) and reads
only the files you give it.

## Opening, editing, saving

- Editor: File > Open (choose an `.sbs` file), drop the file on the window, or pick it from Recent. File > New starts an empty
  package. Every graph and function of the package is in the Explorer; the graph's settings list what draws approximately.
- Saving writes the package back as it was read with only the changes rewritten: nodes, connections, values, positions, graph
  inputs and outputs, functions, comments and frames. Everything MaterialCraft does not edit (resources, dependencies, presets,
  icons, attributes it does not know) is written back untouched, and a package saved without changes is the same file, byte for byte.
- Nodes are the format's own: the atomic nodes, the library graphs MaterialCraft draws, graphs and functions of the package, and
  image and value inputs. Values are stored with their type and whether they are relative to the parent, as the format does.
- A value can be computed by a function graph (the `fn` button), exposed as a graph parameter (`P`), and graph parameters get their
  default, label and slider range in the graph's settings. A graph can run any other graph of the package that does not use it.
- Gradient Map keys are edited in the node's GRADIENT group (AI: `gradient_keys`); an FX-Map comes with its pattern graph
  (one Quadrant) and opens it from its node. Curve points are kept as they are but not yet editable.
- Explorer, right-click a graph: open, export maps, save as a package of its own (with the graphs and functions it uses),
  duplicate, rename (nodes that use it follow), delete (refused while it is used). Package > Add the graphs of another .sbs copies
  every graph of another package in (taken names get a number).
- Command line: `materialcraft render file.sbs[#graph] outdir`, `materialcraft graphs file.sbs [--export graph out.sbs]`,
  `materialcraft check file.sbs`, `materialcraft params file.sbs` (`render --param name=value` sets a graph parameter).
- AI control: `graph_open path=file.sbs`; every command works on the package (see AI_CONTROL.md).

Graphs used from other `.sbs` files are read from the same folder (as the file refers to them). Linked bitmaps are looked up next
to the package and in its `<name>.resources` folder.

`.sbs` is the only file format: the older MaterialCraft files (`.mcgp`, `.mcg`, `.mca`) are no longer read or written, and
there is no converter to them.

## How packages are drawn

MaterialCraft draws each node of a package with its own implementation of that node:


| Part of the file | Drawn as |
|---|---|
| Graphs | Graphs of one package; the first graph with outputs is the main graph |
| Graph outputs and their usages | Output nodes (base colour, normal, roughness, metallic, height, AO, opacity, emissive, ...) |
| Image inputs | Input nodes (graphs used as nodes take up to 16 image inputs) |
| Value inputs (parameters) | Input Value nodes; functions read them, instances set them |
| Built-in nodes | MaterialCraft nodes (table below) |
| Graph instances of the same or another `.sbs` | Subgraph nodes, with their parameters and named outputs |
| Library nodes referred to by name | MaterialCraft's own node, its settings translated so it looks the way it did (see "Library nodes" below) |
| Function-driven parameters | Function graphs bound to the parameter (`fn.<parameter>`) |
| Functions defined in the package | Function graphs |
| Pixel processor / value processor bodies | Function graphs run by Pixel Processor / Value Processor |
| Comments and frames | Comments |
| Node positions, node names | Positions (scaled to our cards), node titles |
| Embedded / linked bitmaps, SVG | Package resources / file paths |

Built-in nodes:

| In the file | MaterialCraft | Notes |
|---|---|---|
| blend | Blend | modes, opacity, alpha handling, cropping |
| levels | Levels | the mid level becomes a gamma (exact); per-channel levels use the red channel's values |
| gradient | Gradient Map | up to 16 keys |
| uniform | Uniform Color | |
| shuffle | Channel Shuffle | |
| grayscaleconversion | Grayscale Conversion | |
| transformation | Transform 2D | matrix decomposed to scale and rotation; skew is reported |
| bitmap, svg | Bitmap, SVG | |
| pixelprocessor, valueprocessor | Pixel / Value Processor | per-pixel function converted |
| passthrough | Dot | |
| normal | Normal From Height | approximate intensity |
| hsl | HSL | approximate: saturation and lightness become multipliers |
| blur, sharpen, dirmotionblur, warp, directionalwarp, emboss, distance, curve, text | closest node | approximate units |
| dyngradient | Lookup Table | the row (or column) of the second image at the coordinate, read at the source value |
| fxmaps | Pattern Program Map | the pattern graph becomes a pattern program (see "Pattern maps" below) |

## Library nodes

Most graphs use nodes from the standard library that comes with the program that wrote them (noises, patterns, filters). A file
only stores the settings someone changed, so the importer needs each library node's defaults to know what the rest were. It uses
a table of the library's **interface facts** (`src/sbs_library.txt`: every node's input and output identifiers, value types,
default values and value ranges; nothing else, see `INDEPENDENT_IMPLEMENTATION.md`). An adapter per library node
(`src/sbs_adapt.rs`) then sets MaterialCraft's own node from all of them, converting units where they differ, and says which of
our inputs and outputs the node's image inputs and outputs map to. Settings it cannot reproduce are listed in the report.
Nodes without their own adapter keep the settings that have the same name and meaning as ours.

Matched so far (checked side by side against renders at their defaults):

- Noises: the clouds, black-and-white spots, fractal sums, moisture, dirt 1-6, grunge maps 001-015 and the named grunges,
  directional noises and scratches, wood fibres, crystal 2, gaussian spots, dirt gradient, noise zoom, the older versions of
  these, creased, fluid, herb, horizontal lines, liquid, messy fibres 1, sponge stains and paper path (all through
  **Preset Noise**); cells 1-4, crystal 1, gaussian noise, perlin noise and plasma (**Noise Patterns**); anisotropic noise
  (value-grid style of **Anisotropic Noise**); white noise; voronoi and voronoi fractal; the 3D noises read at a position
  map (**3D Noise (Position)**).
- Patterns: checkers 1 and 2, stripes, linear gradients 1-3, axial, circular and radial gradients (**Gradient (Two Points)**),
  polygons 1 and 2, shape, starburst (light-ray style), tile generator and tile sampler (both versions), splatter circular,
  fibres 1 and 2, weaves 1-4 (**Weave Patterns**), weave generator (twill style), scratches and thorns 1-3, waveform 1
  (sound-bar style of **Waveform**), messy fibres 2 and 3 (lines and spikes), bricks 1-3 and the brick generator,
  alveolus, gaussian spots 1 and 2, meshes 1 and 2, parquet, tile random 1 and 2 (**Tile Patterns**), triangle grid.
- Close but not identical (MaterialCraft draws its own strands and cells): fur 1-4, microscope view, scratches generator.
- Filters whose settings are translated by name and meaning (not yet checked against renders): clamp, colour to mask,
  curvature, glow, histogram scan, mirror, mosaic, multi-directional and non-uniform directional warp, switch and
  multi-switch, normal blend / invert / to height / vector rotation / combine, quad and trapezoid transforms, quantize,
  radial blur, replace colour, vector warp, height blend, emboss with gloss, non-uniform blur, safe transform.

Drop-down numbers that were only seen at their defaults (shape patterns other than the default, blending modes of the
tile and splatter nodes) are mapped by their likely order and reported; the calibration kit below settles them.

## Pattern maps

A pattern map's own graph (its quadrant, iterate and switch nodes) becomes a pattern program of **Quad Cell**,
**Repeat** and **Select Branch** nodes that the Pattern Program Map runs; open it from the node's properties to edit it.

- A Quad Cell draws one pattern (none, an image input, or one of the built-in shapes) in its cell, with the cell's
  colour or grey level, pattern offset, size, rotation (turns), variation and blending (add / subtract around 0.5,
  max, alpha blend), then runs up to four children in its quarters, one level deeper. Its branch offset moves its
  own pattern and everything below it.
- Repeat runs its first branch once and its second branch the given number of times; Select Branch runs one of two.
- Settings driven by functions stay functions: they run at every visit and read `$depth`, `$depthpow2`, `$number`
  (the repeat number) and `$pos` (the cell position); variables a function sets are seen further down the branch.
- The map's roughness scales each level relative to the one above, its opacity every pattern; the background
  colour or image is the starting canvas.
- Random numbers come from MaterialCraft's own generator, so random layouts differ in detail from the original;
  the structure, sizes and blending follow the file. Render and tiling regions are not used.

## The report

Every node is counted as exact, approximate or not converted. Approximate means our own implementation of the same operation, or
values whose units differ: check the result. A node that could not be converted becomes a placeholder (a dot that passes its input
through, or mid grey), marked with an orange corner and a note in the Properties panel. "Show" in the report jumps to it.

## Limits of this first version

- Pattern maps are redrawn by MaterialCraft's quad-tree engine (see above): random layouts differ in detail.
- Library nodes are MaterialCraft's own versions. The ones listed above are matched; others keep only the settings with the same
  name and meaning, and nodes with no MaterialCraft counterpart yet (some material filters, arc pavement and pavement,
  shape extrude, height extrude, cube 3D, panorama and ray-traced nodes) are placeholders or
  approximations.
- A node's own output size and format carry over to its Output Size and Output Format settings. Width and height are
  independent, including function-driven sizes and offsets relative to the parent or input.
- Compiled `.sbsar` files cannot be imported (they are not editable packages).
- Number codes of drop-down settings (blend modes and the like) follow the order the writing program lists them in; they are being
  checked against sample files.

## Helping make it exact

Real files are what make the importer better. Copy your own `.sbs` projects (with their `.resources` folders) into `sbs_samples/` in
the repository folder; that folder is ignored by git, so the files stay on your computer.

A calibration file settles the drop-down codes for good. Make one graph with:

1. twelve Blend nodes in a row, left to right, set to the blending modes in the order the drop-down lists them;
2. below them, four Blend nodes set to each alpha blending option in order;
3. one Transformation 2D node scaled to 200 % and rotated 45 degrees, with a horizontal offset of 0.25;
4. one Levels node with the mid level at 0.25, one HSL node with hue 0.75, one Normal node in DirectX format.

Save it as `sbs_samples/calibration.sbs`.

The calibration kit in `reference/calib` (ignored by git) does the same for the library and the built-in nodes:
`run_calibration.ps1` renders test cases with the program's own command-line tools on your computer (library nodes
straight from the archives in `reference/sbsar` with changed settings; built-in nodes and pattern maps from small test
packages), and the results are compared privately with MaterialCraft's. The script can be stopped and started again;
finished cases are skipped. Only fitted numbers come back into the code; the renders themselves are never committed.
