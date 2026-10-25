// SPDX-License-Identifier: MIT OR Apache-2.0
//! The command layer: every action on a material graph as a named command with typed, documented parameters.
//!
//! One [`Session`] holds a graph with its undo history and runs commands against it. The command line (`exec`),
//! the MCP server (AI assistants), the control channel of the running editor and the tests all go through
//! [`Session::execute`], so they can never disagree about what an action does.
//!
//! The design follows the command-registry idea of the FilmCraft editor (MIT OR Apache-2.0, storytold/filmcraft):
//! one registry of commands with parameter docs, undoable edits, and a stdio MCP front end. The code here is written
//! for MaterialCraft's own graph model.
//!
//! Rules every command follows:
//! * it never panics: input comes from another program, a panic inside a command is caught and reported as an error;
//! * an edit is atomic: when it fails the graph is exactly as it was before;
//! * every successful edit is one undo step (`batch` makes many commands one step).

use crate::export::{self, Format};
use crate::graph::{self, Comment, Error as GError, Graph, Node, NodeCache, OUTPUT_USAGES};
use crate::json::Json;
use crate::tex::Tex;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Longest edge of an image the render commands return.
pub const MAX_RENDER: usize = 2048;
/// Undo steps kept.
pub const MAX_UNDO: usize = 200;
/// Commands in one `batch`.
pub const MAX_BATCH: usize = 2000;

// ------------------------------------------------------------------------------------------ descriptions

/// One documented parameter of a command.
pub struct Arg {
    pub name: &'static str,
    /// string, number, integer, boolean, object, array, node (a node id or name) or nodes (a list of those)
    pub ty: &'static str,
    pub req: bool,
    pub doc: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// reads the graph, changes nothing
    Query,
    /// changes the graph (one undo step)
    Edit,
    /// handled by the session itself (undo, redo, batch)
    Special,
}

type Run = fn(&mut Session, &Json) -> Result<Outcome, String>;

pub struct Command {
    pub id: &'static str,
    pub doc: &'static str,
    pub args: &'static [Arg],
    pub kind: Kind,
    run: Run,
}

/// A picture a command returns.
pub struct Image {
    pub label: String,
    pub png: Vec<u8>,
}

/// What a command returns: data, plus pictures for the commands that render.
pub struct Outcome {
    pub data: Json,
    pub images: Vec<Image>,
}

impl Outcome {
    fn data(d: Json) -> Result<Outcome, String> {
        Ok(Outcome { data: d, images: Vec::new() })
    }
}

macro_rules! arg {
    ($n:expr, $t:literal, $r:expr, $d:literal) => {
        Arg { name: $n, ty: $t, req: $r, doc: $d }
    };
}

const NODE: &str = "node";

pub const COMMANDS: &[Command] = &[
    // ---- discovery
    Command { id: "command.list", doc: "List every command with its parameters.", args: &[arg!("filter", "string", false, "only commands whose id or text contains this")], kind: Kind::Query, run: c_command_list },
    Command {
        id: "node.types",
        doc: "List the node types you can add: name, category and what it does. Graphs, function graphs and FX-map pattern graphs each have their own nodes (sbs:<filter>, sbs:lib:<package>/<graph>, sbs:fn:<function>, sbs:fx:<node>). Filter by words (name, category or description).",
        args: &[
            arg!("query", "string", false, "words to look for, e.g. 'noise' or 'blur'"),
            arg!("category", "string", false, "only this category, e.g. Noises"),
            arg!("family", "string", false, "texture, function or pattern (FX-map) (default: the family of the focused graph)"),
            arg!("functions", "boolean", false, "same as family=function"),
        ],
        kind: Kind::Query,
        run: c_node_types,
    },
    Command { id: "node.describe", doc: "Everything about one node type: inputs, outputs, and every parameter with its default, range, choices and group.", args: &[arg!("kind", "string", true, "node type from node.types, e.g. sbs:blend, sbs:fn:add or sbs:fx:quadrant")], kind: Kind::Query, run: c_node_describe },
    // ---- the package and its graphs
    Command { id: "graph.list", doc: "Every graph of the package that is open: its graphs and functions (in .sbs packages, also the functions inside node values), with their kind, outputs, inputs and which graphs use them. Graph and node commands work on the focused graph unless they get a `graph` parameter.", args: &[], kind: Kind::Query, run: c_graph_list },
    Command { id: "graph.focus", doc: "Open one graph of the package for editing, like opening it in the editor's explorer: later commands without a `graph` parameter work on it. Omit `graph` to go back to the main graph.", args: &[arg!("graph", "string", false, "graph name from graph.list; omit for the main graph")], kind: Kind::Query, run: c_graph_focus },
    Command {
        id: "graph.create",
        doc: "Add a new graph to the package. Other graphs use a graph through a node of kind sbs:graph (params {\"graph\": <name>}); its input nodes become that node's inputs. A function graph computes a value: node.set_result picks the node that gives it, and nodes call it with sbs:fn:instance. (FX-map pattern graphs live inside FX-Map nodes: node.add kind=sbs:fxmaps.)",
        args: &[
            arg!("name", "string", true, "graph name: letters, digits and _"),
            arg!("kind", "string", false, "texture (default), material (with the usual outputs) or function"),
            arg!("focus", "boolean", false, "focus the new graph (as graph.focus)"),
        ],
        kind: Kind::Edit,
        run: c_graph_create,
    },
    Command { id: "graph.rename", doc: "Rename a graph of the package; every node that uses it (graph instances, function calls) follows.", args: &[arg!("graph", "string", true, "the graph to rename (the main graph's name is its identifier)"), arg!("name", "string", true, "new name: letters, digits and _")], kind: Kind::Edit, run: c_graph_rename },
    Command { id: "graph.delete", doc: "Remove a graph from the package (refused while another graph still uses it).", args: &[arg!("graph", "string", true, "graph name")], kind: Kind::Edit, run: c_graph_delete },
    Command { id: "graph.duplicate", doc: "Copy a graph of the package as a new graph. Returns the new name.", args: &[arg!("graph", "string", false, "graph to copy (default: the focused graph)"), arg!("name", "string", false, "name of the copy (default: <name>_copy)")], kind: Kind::Edit, run: c_graph_duplicate },
    Command { id: "graph.inspect", doc: "One graph: settings, every node with its parameters and connections, the outputs, and problems found. In function graphs every node also shows the type of value it makes.", args: &[arg!("compact", "boolean", false, "leave out positions and unchanged-looking details")], kind: Kind::Query, run: c_graph_inspect },
    Command { id: "graph.validate", doc: "Check a graph: missing inputs, wrong pin types, cycles, outputs; in function graphs also the value types. Returns `problems` (errors to fix; empty = fine) and `warnings` (allowed but probably not meant: nodes whose result reaches no output).", args: &[], kind: Kind::Query, run: c_graph_validate },
    Command { id: "graph.text", doc: "The package as .sbs file text (XML), or with `graph` one graph and the graphs it uses as a package of its own.", args: &[arg!("graph", "string", false, "just this graph and what it uses")], kind: Kind::Query, run: c_graph_text },
    Command {
        id: "graph.new",
        doc: "Start a new .sbs package with one graph (empty, or with the usual material outputs fed by Uniform Color nodes).",
        args: &[
            arg!("size", "integer", false, "width in pixels, a power of two from 16 to 8192 (default 512)"),
            arg!("height", "integer", false, "height in pixels; omit for square"),
            arg!("name", "string", false, "name of the graph (default Substance_graph)"),
            arg!("template", "string", false, "empty (default) or material"),
        ],
        kind: Kind::Edit,
        run: c_graph_new,
    },
    Command { id: "graph.open", doc: "Open an .sbs package to edit it as it is (the result says what draws approximately).", args: &[arg!("path", "string", true, "file path")], kind: Kind::Edit, run: c_graph_open },
    Command { id: "graph.load_text", doc: "Replace the package with the text of an .sbs package (its XML).", args: &[arg!("text", "string", true, "the file text")], kind: Kind::Edit, run: c_graph_load_text },
    Command { id: "graph.save", doc: "Save the package as .sbs (to `path`, or to the file it was opened from). Only the changes are rewritten, so a package opened and saved unchanged stays byte-identical.", args: &[arg!("path", "string", false, "file path (.sbs)")], kind: Kind::Query, run: c_graph_save },
    Command { id: "graph.add_file", doc: "Add every graph and function of another .sbs package to this one; names that are taken get a number. Returns the name its first graph got.", args: &[arg!("path", "string", true, "file path")], kind: Kind::Edit, run: c_graph_add_file },
    Command {
        id: "graph.set",
        doc: "Change settings of a graph.",
        args: &[
            arg!("size", "integer", false, "width in pixels (16..8192)"),
            arg!("height", "integer", false, "height in pixels (0 = square)"),
            arg!("seed_offset", "integer", false, "added to every node's random seed"),
            arg!("precision", "integer", false, "node precision: 0 = 32-bit float, 1 = 8 bit, 2 = 16 bit, 3 = 16-bit float"),
            arg!("output_format", "integer", false, "bits per channel of exported maps: 8 or 16"),
            arg!("default_format", "any", false, "format of every node of the graph: 8, 16, \"16f\" or \"32f\". Existing nodes get it now and nodes added later start with it (saved in the file). null clears it for new nodes"),
            arg!("view", "object", false, "3D preview defaults saved with the graph (any render.3d view option: tiling, displacement, shape, env, yaw ...); render.3d and render.review use them unless a call gives its own. {} clears them"),
            arg!("normal_format", "string", false, "directx (green down, what imported .sbs graphs use) or opengl (green up)"),
            arg!("details", "object", false, "label, description, category, author, author_url, tags, type (Standard material, Filter, Generator, Other), phys_x / phys_y / phys_z (cm), exposed and show_in_library (yes / no)"),
        ],
        kind: Kind::Edit,
        run: c_graph_set,
    },
    Command { id: "graph.layout", doc: "Arrange all nodes of a graph in tidy columns from inputs to outputs (so the editor shows a readable graph).", args: &[], kind: Kind::Edit, run: c_graph_layout },
    // ---- resources
    Command { id: "resource.list", doc: "The resources of the package (bitmaps, SVG, fonts ...): their name, type, whether they are embedded and the file they link.", args: &[], kind: Kind::Query, run: c_resource_list },
    Command {
        id: "spline.set",
        doc: "Draw a spline shape: it becomes an SVG resource of the package and an SVG node showing it (white shapes on black; closed paths are filled, any path can be stroked), so it feeds Bevel, Distance, Blur and every other node like any picture. Give `spline` (text: lines `path closed=1 smooth=1` followed by `pt x y [hin=dx,dy] [hout=dx,dy]` in 0..1 texture coordinates, y down) or a `preset`. With `node` an existing SVG node is updated.",
        args: &[
            arg!("spline", "string", false, "the spline text (see spline.get for the format)"),
            arg!("preset", "string", false, "circle, square, rounded_square, triangle, star, heart, arrow, gear, wave, spiral or cross"),
            arg!("styles", "array", false, "one {fill: bool, stroke: width in 0..1 units} per path (default: closed paths filled, no stroke; open paths are stroked)"),
            arg!("name", "string", false, "resource name (letters, digits, _; default spline)"),
            arg!("node", "node", false, "an existing SVG node to update; omitted: a new SVG node is added"),
        ],
        kind: Kind::Edit,
        run: c_spline_set,
    },
    Command {
        id: "spline.get",
        doc: "The spline an SVG node of this package shows when it was drawn with spline.set or the spline editor: its text and styles.",
        args: &[arg!("node", "node", true, "the SVG node")],
        kind: Kind::Query,
        run: c_spline_get,
    },
    // ---- nodes
    Command {
        id: "node.add",
        doc: "Add a node to a graph. Returns its id. Parameters not given keep the node's defaults (see node.describe). The node must belong to the graph's family: sbs:<filter> and sbs:lib:* nodes in graphs, sbs:fn:* nodes in function graphs, sbs:fx:* nodes in FX-map pattern graphs.",
        args: &[
            arg!("kind", "string", true, "node type (node.types)"),
            arg!("params", "object", false, "parameter values: numbers, choice names for dropdowns, true/false, or text for text parameters"),
            arg!("inputs", "object", false, "connections as {slot: node} or {slot: {node: id, output: name}}"),
            arg!("name", "string", false, "a short name you can use instead of the id in later commands"),
            arg!("x", "number", false, "position in the editor"),
            arg!("y", "number", false, "position in the editor"),
        ],
        kind: Kind::Edit,
        run: c_node_add,
    },
    Command {
        id: "node.set",
        doc: "Set parameters of a node. Numbers are range-checked (out-of-range values are clamped and reported); dropdowns take a choice name or index; null restores the default. On a graph instance the parameters are those of its graph (node.params lists them).",
        args: &[arg!(NODE, "node", true, "node id or name"), arg!("params", "object", true, "{parameter: value}"), arg!("allow_unknown", "boolean", false, "accept parameter names this node type does not list")],
        kind: Kind::Edit,
        run: c_node_set,
    },
    Command { id: "node.params", doc: "Every parameter one node can take with its current value: its own, those of the graph a graph instance runs, and which values are computed by a function graph.", args: &[arg!(NODE, "node", true, "node id or name")], kind: Kind::Query, run: c_node_params },
    Command {
        id: "node.drive",
        doc: "Compute a value of a node with a function graph of its own (it starts as one constant of the current value; add sbs:fn:* nodes to it with graph=<the returned name>), or stop computing it (stop=true).",
        args: &[
            arg!(NODE, "node", true, "node id or name"),
            arg!("param", "string", true, "parameter name"),
            arg!("stop", "boolean", false, "stop computing the value with a function (back to its default)"),
        ],
        kind: Kind::Edit,
        run: c_node_drive,
    },
    Command {
        id: "node.expression",
        doc: "Compute a value with a math expression written as text instead of wiring function nodes by hand: the expression is turned into the value's function graph (sbs:fn:* nodes, saved in the file like hand-made ones). Works for any parameter, the Pixel Processor's per-pixel function (param perpixel; sample($pos) reads its first input) and the Value Processor (param function); with `function` instead of node/param it rebuilds a function graph of the package. Returns the function graph's name and an outline of its nodes. Language: see the `expression` argument.",
        args: &[
            arg!(NODE, "node", false, "node id or name (with param)"),
            arg!("param", "string", false, "parameter to compute (perpixel for a Pixel Processor)"),
            arg!("function", "string", false, "instead of node/param: a function graph of the package to rebuild"),
            Arg { name: "expression", ty: "string", req: true, doc: crate::fexpr::HELP },
        ],
        kind: Kind::Edit,
        run: c_node_expression,
    },
    Command {
        id: "macro.add",
        doc: "Insert a ready-made group of standard nodes for a common job that has no single node: grow / shrink / outline a mask, a star with any number of points, a polygon, your own shape repeated as tiles, directional strokes per cell, a whole-number zoom for patterns without a scale setting. Everything is made of ordinary nodes (Distance, Levels, Blend, Pixel Processor, Tile Generator, Tile Sampler, Transformation 2D), so it can be edited afterwards. macro=list shows them all with their options. Returns the new nodes and the one to connect onwards (output_node).",
        args: &[
            arg!("macro", "string", true, "grow, shrink, outline, star, polygon, tile_shape, strokes, tile_scale (or list)"),
            arg!("input", "node", false, "the node it works on (grow, shrink, outline, tile_shape, tile_scale)"),
            arg!("options", "object", false, "settings of the macro (see macro=list), e.g. {\"pixels\": 6} or {\"points\": 7}"),
            arg!("name", "string", false, "name for the output node"),
        ],
        kind: Kind::Edit,
        run: c_macro_add,
    },
    Command { id: "node.set_result", doc: "In a function graph of an .sbs package: the node whose value is the function's result.", args: &[arg!(NODE, "node", true, "node id or name")], kind: Kind::Edit, run: c_node_set_result },
    Command { id: "node.remove", doc: "Delete nodes. Links that used them are removed too.", args: &[arg!("nodes", "nodes", false, "node ids or names (`node` works too)"), arg!("node", "node", false, "one node id or name")], kind: Kind::Edit, run: c_node_remove },
    Command { id: "node.duplicate", doc: "Copy nodes with their parameters; links between the copied nodes are copied too. Returns the new ids in the same order.", args: &[arg!("nodes", "nodes", true, "node ids or names"), arg!("dx", "number", false, "offset of the copies (default 40)"), arg!("dy", "number", false, "offset of the copies (default 40)")], kind: Kind::Edit, run: c_node_duplicate },
    Command { id: "node.connect", doc: "Connect the output of one node to an input slot of another (replacing what was there).", args: &[arg!("from", "node", true, "source node"), arg!("to", "node", true, "target node"), arg!("slot", "string", false, "input slot of the target (default: the first free required slot, else 'in')"), arg!("output", "string", false, "which output of a multi-output source (for a Graph node: an output of its graph)")], kind: Kind::Edit, run: c_node_connect },
    Command { id: "node.disconnect", doc: "Remove the link into an input slot.", args: &[arg!("to", "node", true, "target node"), arg!("slot", "string", true, "input slot")], kind: Kind::Edit, run: c_node_disconnect },
    Command { id: "node.move", doc: "Move a node in the editor.", args: &[arg!(NODE, "node", true, "node id or name"), arg!("x", "number", true, "x"), arg!("y", "number", true, "y")], kind: Kind::Edit, run: c_node_move },
    Command { id: "output.add", doc: "Make a node a material output (base colour, normal, roughness, metallic, height, ambient_occlusion, opacity, emissive ...). Replaces an existing output of the same usage. (A function graph gives its value through node.set_result instead.)", args: &[arg!(NODE, "node", true, "node that provides the image"), arg!("usage", "string", true, "basecolor, normal, roughness, metallic, height, ambient_occlusion, opacity, emissive, diffuse, specular, glossiness or out"), arg!("output", "string", false, "which output of a multi-output node"), arg!("name", "string", false, "file name for the exported map (default: the usage)")], kind: Kind::Edit, run: c_output_add },
    Command { id: "output.remove", doc: "Remove the output with this usage.", args: &[arg!("usage", "string", true, "the usage to remove")], kind: Kind::Edit, run: c_output_remove },
    Command { id: "graph.expose", doc: "Expose a node parameter as a graph parameter (a slider for whoever uses the graph).", args: &[arg!("label", "string", true, "name shown to the user"), arg!(NODE, "node", true, "node"), arg!("param", "string", true, "parameter name"), arg!("min", "number", false, "smallest value (default: the parameter's range)"), arg!("max", "number", false, "largest value")], kind: Kind::Edit, run: c_graph_expose },
    Command { id: "graph.unexpose", doc: "Stop exposing a graph parameter.", args: &[arg!("label", "string", true, "its label")], kind: Kind::Edit, run: c_graph_unexpose },
    Command { id: "graph.set_input", doc: "Change a graph parameter (made by graph.expose or read from the file): its default value, its label or its slider range. graph.inspect lists them (value_inputs).", args: &[arg!("input", "string", true, "its identifier or label"), arg!("value", "any", false, "new default: a number, a list of numbers (colours, vectors) or true/false"), arg!("label", "string", false, "new label"), arg!("min", "number", false, "slider minimum"), arg!("max", "number", false, "slider maximum")], kind: Kind::Edit, run: c_graph_set_input },
    Command { id: "comment.add", doc: "Add a comment box to the editor canvas.", args: &[arg!("text", "string", true, "text"), arg!("x", "number", false, "x"), arg!("y", "number", false, "y"), arg!("width", "number", false, "width"), arg!("height", "number", false, "height")], kind: Kind::Edit, run: c_comment_add },
    Command { id: "comment.remove", doc: "Remove a comment box (graph.inspect lists them in order, starting at 0).", args: &[arg!("index", "integer", true, "which comment")], kind: Kind::Edit, run: c_comment_remove },
    // ---- looking at results
    Command {
        id: "render.node",
        doc: "Render one node and return a picture and statistics (min / max / mean per channel, whether it is flat, and a tiling check: seam_horizontal / seam_vertical compare the step across the tile border with the largest step inside the image, so values at or below about 1 mean the edges match and tiles_seamlessly is true below 1.5; for Flood Fill nodes and the nodes reading them also the number of regions and a warning when shapes merged). In a function graph the node's value is drawn per pixel; in an FX-map pattern graph the pattern up to that node is drawn.",
        args: &[
            arg!(NODE, "node", true, "node id or name"),
            arg!("output", "string", false, "which output of a multi-output node"),
            arg!("size", "integer", false, "pixels on the longest side (16..2048, default 256)"),
            arg!("channel", "string", false, "rgb (default), r, g, b, alpha or luma"),
            arg!("normalize", "boolean", false, "stretch the values to 0..1 (for height maps)"),
            arg!("tile", "integer", false, "show the image repeated this many times per side (1..4) to judge seams"),
            arg!("image", "boolean", false, "set false to get only the statistics"),
        ],
        kind: Kind::Query,
        run: c_render_node,
    },
    Command { id: "render.outputs", doc: "A contact sheet of every output of a graph (base colour, normal, roughness ...), labelled in the data. For a function graph or FX-map pattern graph: its result.", args: &[arg!("size", "integer", false, "cell size in pixels (default 192)"), arg!("columns", "integer", false, "columns (default 3)")], kind: Kind::Query, run: c_render_outputs },
    Command { id: "render.sheet", doc: "A contact sheet of several nodes, to compare steps of the graph.", args: &[arg!("nodes", "nodes", true, "node ids or names"), arg!("size", "integer", false, "cell size in pixels (default 192)"), arg!("columns", "integer", false, "columns (default 3)")], kind: Kind::Query, run: c_render_sheet },
    Command {
        id: "render.3d",
        doc: "Render the material of a graph on a 3D object (software renderer) and return the picture. list=true returns the names every option accepts (shapes, environments, light rigs, cameras, post looks) and all view settings instead. The path tracer needs about 128 samples or more before rough surfaces stop looking grainy; compare looks with the raster mode or with high spp.",
        args: &[
            arg!("size", "integer", false, "picture width and height (16..1024, default 512)"),
            arg!("shape", "string", false, "sphere, cube, plane, cylinder, ... (default sphere)"),
            arg!("scene", "string", false, "single, floor, lookdev or pedestal"),
            arg!("channel", "string", false, "lit (default), base, roughness, metallic, normal, ao, height, emissive, world_normal, uv"),
            arg!("camera", "string", false, "three_quarter, front, back, left, right, top, bottom, close_up"),
            arg!("env", "string", false, "environment preset (list=true shows them)"),
            arg!("env_rotation", "number", false, "turn the environment (radians)"),
            arg!("env_exposure", "number", false, "environment brightness in stops (-8..8)"),
            arg!("background", "string", false, "environment, color or gradient"),
            arg!("rig", "string", false, "light rig: three_point, key_only, top, sun, none"),
            arg!("light_intensity", "number", false, "multiply the rig's lights (0 = environment light only)"),
            arg!("exposure", "number", false, "picture exposure multiplier (default 1)"),
            arg!("post", "string", false, "post look: neutral, cinematic, punchy, soft"),
            arg!("tiling", "number", false, "how many times the texture repeats across the object (default 1)"),
            arg!("displacement", "number", false, "how far the height map moves the surface (object units, sphere radius 1; default 0.08; 0 = off)"),
            arg!("relief", "string", false, "how height is shown: displace (default), parallax, bump or off"),
            arg!("bump", "number", false, "strength of bump relief from the height map (relief=bump)"),
            arg!("normal_strength", "number", false, "strength of the normal map 0..1 (default 1)"),
            arg!("yaw", "number", false, "camera angle around the object (radians)"),
            arg!("pitch", "number", false, "camera angle above the object (radians)"),
            arg!("settings", "object", false, "any other view settings by key, e.g. {\"light.0.intensity\": 5, \"post.bloom\": 0.2, \"fov\": 30}"),
            arg!("mode", "string", false, "raster (fast, default) or pt (path traced, slow)"),
            arg!("spp", "integer", false, "path tracer samples per pixel (default 128)"),
            arg!("tex", "integer", false, "resolution the material is computed at (default 512)"),
            arg!("list", "boolean", false, "return the option names and view settings instead of a picture"),
        ],
        kind: Kind::Query,
        run: c_render_3d,
    },
    Command {
        id: "graph.extract",
        doc: "Make a subgraph from nodes: move them into a new graph of the package and put one node running that graph in their place. Links coming in from other nodes become the new graph's inputs, links going out (to other nodes or to the graph's outputs) become its outputs, named after the nodes they come from; node names move along. Edit the new graph with graph=<name> (or graph.focus).",
        args: &[
            arg!("nodes", "nodes", true, "the nodes to move (ids or names)"),
            arg!("name", "string", true, "name of the new graph (letters, digits and _)"),
            arg!("replace", "boolean", false, "put a node running the new graph in their place (default true); false only copies them"),
            arg!("node_name", "string", false, "a name for the node that runs the new graph"),
        ],
        kind: Kind::Edit,
        run: c_graph_extract,
    },
    Command { id: "graph.outline", doc: "The graph as compact text: one line per node with its changed values (numbered choices with their names) and where each input comes from, then the outputs. Cheaper to read than graph.inspect.", args: &[], kind: Kind::Query, run: c_graph_outline },
    Command {
        id: "render.review",
        doc: "One picture to check the whole material, plus numbers and the graph as text: the lit 3D preview, an optional reference picture beside it, every output map, and the node graph with a thumbnail in each node (outputs marked). The data has `outline` (every node with its changed values, the names of numbered choices and where each input comes from), statistics and a tiling check per map, problems / warnings / approximations, and with a reference: a comparison (brightness, colour, contrast, fine detail, how many features repeat across, a similarity score) with hints in words on what to change. Use it after each round of edits and compare against the reference.",
        args: &[
            arg!("reference", "string", false, "an image file (photo or render of the target material) to compare with"),
            arg!("compare", "string", false, "what to compare the reference with: auto (default: the closer of the two), render (the middle of the 3D preview) or basecolor (the base colour map)"),
            arg!("size", "integer", false, "size of the 3D preview (128..1024, default 384)"),
            arg!("tex", "integer", false, "resolution the maps are computed at (default 512)"),
            arg!("thumbnails", "boolean", false, "draw a picture in every node of the graph (default true)"),
            arg!("shape", "string", false, "3D object (default sphere); the view options are the same as render.3d's, and the defaults saved with the graph (graph.set view) apply"),
            arg!("scene", "string", false, "single, floor, lookdev or pedestal"),
            arg!("channel", "string", false, "lit (default), base, roughness, metallic, normal, ao, height, emissive"),
            arg!("camera", "string", false, "three_quarter, front, back, left, right, top, bottom, close_up"),
            arg!("env", "string", false, "environment preset"),
            arg!("env_rotation", "number", false, "turn the environment (radians)"),
            arg!("env_exposure", "number", false, "environment brightness in stops"),
            arg!("background", "string", false, "environment, color or gradient"),
            arg!("rig", "string", false, "light rig: three_point, key_only, top, sun, none"),
            arg!("light_intensity", "number", false, "multiply the rig's lights"),
            arg!("exposure", "number", false, "picture exposure multiplier"),
            arg!("post", "string", false, "post look: neutral, cinematic, punchy, soft"),
            arg!("tiling", "number", false, "texture repeats on the object"),
            arg!("displacement", "number", false, "height displacement amount (0 = off)"),
            arg!("relief", "string", false, "displace, parallax, bump or off"),
            arg!("bump", "number", false, "strength of bump relief"),
            arg!("normal_strength", "number", false, "strength of the normal map 0..1"),
            arg!("yaw", "number", false, "camera angle around the object (radians)"),
            arg!("pitch", "number", false, "camera angle above the object (radians)"),
            arg!("match_lighting", "boolean", false, "with a reference: also report how the render's brightness would match the reference after an exposure change, and use that to judge colour and contrast (default true)"),
            arg!("crop", "object", false, "with a reference: the part of the reference picture that shows the material, as {x, y, w, h} fractions of the picture (default: found automatically by dropping the plain background)"),
            arg!("settings", "object", false, "any other view settings by key"),
        ],
        kind: Kind::Query,
        run: c_render_review,
    },
    Command { id: "export.materialx", doc: "Write the graph as a MaterialX 1.39 material (.mtlx file plus textures in a sub-folder) that Unreal Engine 5.8 imports: colour maps are tagged srgb_texture, the normal map goes through a normalmap node, height becomes a displacement shader.", args: &[arg!("dir", "string", true, "folder"), arg!("size", "integer", false, "resolution (default: the graph's size)"), arg!("shader", "string", false, "standard (default, Standard Surface) or openpbr"), arg!("normal", "string", false, "directx (default) or opengl: the green channel of the written normal map"), arg!("depth", "integer", false, "8 (default) or 16"), arg!("displacement", "boolean", false, "write the height map as a displacement shader (default true)"), arg!("bake_ao", "boolean", false, "multiply ambient occlusion into the base colour (default false)"), arg!("name", "string", false, "material name (default: the graph's identifier)")], kind: Kind::Query, run: c_export_materialx },
    Command { id: "export", doc: "Write the output maps of a graph to a folder as image files (any graph of the package with its `graph` parameter).", args: &[arg!("dir", "string", true, "folder"), arg!("size", "integer", false, "resolution (default: the graph's size)"), arg!("format", "string", false, "png (default), jpg, tiff, exr, hdr"), arg!("depth", "integer", false, "8, 16 or 32")], kind: Kind::Query, run: c_export },
    // ---- history
    Command { id: "edit.undo", doc: "Undo the last change.", args: &[], kind: Kind::Special, run: c_noop },
    Command { id: "edit.redo", doc: "Redo the change that was undone.", args: &[], kind: Kind::Special, run: c_noop },
    Command { id: "edit.history", doc: "How many undo and redo steps there are, and the last commands.", args: &[], kind: Kind::Query, run: c_history },
    Command { id: "batch", doc: "Run several commands in order as ONE undoable step. All or nothing: if one fails, none of them is applied. Use node names (the `name` of node.add) to refer to nodes created earlier in the batch. Each step may carry its own `graph` parameter.", args: &[arg!("steps", "array", true, "list of {command, params}")], kind: Kind::Special, run: c_noop },
];

/// The `graph` parameter every command that works on one graph takes.
pub static GRAPH_ARG: Arg = Arg { name: "graph", ty: "string", req: false, doc: "which graph of the package (graph.list); default: the focused graph" };

/// Commands that work on one graph of the package: they take the `graph` parameter and otherwise use the focused
/// graph.
pub fn scoped(id: &str) -> bool {
    matches!(
        id,
        "graph.inspect" | "graph.outline" | "graph.extract" | "graph.validate" | "graph.set" | "graph.layout" | "graph.expose" | "graph.unexpose" | "graph.set_input" | "comment.add" | "comment.remove" | "render.node" | "render.outputs" | "render.sheet" | "render.3d" | "render.review" | "export" | "export.materialx" | "function.probe"
    ) || (id.starts_with("node.") && !matches!(id, "node.types" | "node.describe"))
        || id == "macro.add"
        || id.starts_with("output.")
}

/// Commands that change the package as a whole (its list of graphs, links and resources) rather than one graph.
pub fn package_level(id: &str) -> bool {
    matches!(id, "graph.create" | "graph.rename" | "graph.delete" | "graph.duplicate" | "graph.add_file" | "graph.link" | "graph.unlink" | "resource.add" | "resource.remove" | "graph.load_text")
}

pub fn find(id: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.id == id)
}

impl Command {
    /// The command's parameters, with the `graph` parameter of the commands that work on one graph.
    pub fn all_args(&self) -> Vec<&'static Arg> {
        let mut v: Vec<&'static Arg> = self.args.iter().collect();
        if scoped(self.id) {
            v.push(&GRAPH_ARG);
        }
        v
    }

    /// The JSON Schema (draft 2020-12) of the command's parameters, for MCP.
    pub fn schema(&self) -> Json {
        let mut props = Json::obj();
        let mut req: Vec<Json> = Vec::new();
        for a in self.all_args() {
            let mut p = match a.ty {
                "node" => Json::obj().with("type", Json::Arr(vec!["integer".into(), "string".into()])),
                "nodes" => Json::obj().with("type", "array").with("items", Json::obj().with("type", Json::Arr(vec!["integer".into(), "string".into()]))),
                "array" => Json::obj().with("type", "array").with("items", Json::obj()),
                // a value of any shape (a number, a list, true/false, text)
                "any" => Json::obj(),
                t => Json::obj().with("type", t),
            };
            p.set("description", a.doc);
            props.set(a.name, p);
            if a.req {
                req.push(a.name.into());
            }
        }
        let mut s = Json::obj().with("type", "object").with("properties", props);
        if !req.is_empty() {
            s.set("required", Json::Arr(req));
        }
        s
    }

    /// Name used for the MCP tool: the id with `.` replaced by `_`.
    pub fn tool_name(&self) -> String {
        self.id.replace('.', "_")
    }
}

/// Command for an MCP tool name (`node_add`) or a command id (`node.add`).
pub fn find_tool(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.id == name || c.tool_name() == name)
}

// ------------------------------------------------------------------------------------------ the session

pub struct Session {
    /// the package: its main graph, which holds the other graphs (`subgraphs`) and the resources
    pub graph: Graph,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    undo: Vec<Graph>,
    redo: Vec<Graph>,
    /// node names of the graph being worked on
    aliases: BTreeMap<String, u32>,
    /// node names of the other graphs ("" = the main graph)
    other_aliases: BTreeMap<String, BTreeMap<String, u32>>,
    /// the graph commands work on when they get no `graph` parameter (`None` = the main graph)
    focus: Option<String>,
    /// while a command works on another graph: the package it was taken from
    outer: Option<Outer>,
    log: Vec<String>,
    cache: Arc<NodeCache>,
    /// when set, file commands may only touch paths inside this folder
    pub root: Option<PathBuf>,
    changed: bool,
}

/// The package while one of its graphs is being worked on, and what that graph had of its own.
struct Outer {
    name: String,
    root: Graph,
    seed: u32,
    own_subgraphs: BTreeMap<String, Graph>,
    own_resources: BTreeMap<String, Arc<Vec<u8>>>,
}

/// Node names of every graph of a package: graph name ("" = the main graph) to node name to id.
pub type AliasBook = BTreeMap<String, BTreeMap<String, u32>>;

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

fn panic_text(p: Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "unknown panic".into())
}

impl Session {
    pub fn new() -> Session {
        Session::from_graph(native::new_package("Substance_graph", 512), None)
    }

    pub fn from_graph(graph: Graph, path: Option<PathBuf>) -> Session {
        Session {
            graph,
            path,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            aliases: BTreeMap::new(),
            other_aliases: BTreeMap::new(),
            focus: None,
            outer: None,
            log: Vec::new(),
            cache: Arc::new(NodeCache::with_default_budget()),
            root: None,
            changed: false,
        }
    }

    /// A session over a whole package that lives somewhere else (the open editor): `book` holds the node names
    /// handed out so far in each graph and `focus` is the graph commands work on by default (`None`: the main one).
    pub fn from_package(graph: Graph, path: Option<PathBuf>, dirty: bool, mut book: AliasBook, focus: Option<String>, cache: Arc<NodeCache>) -> Session {
        let mut s = Session::from_graph(graph, path);
        s.dirty = dirty;
        s.aliases = book.remove("").unwrap_or_default();
        s.other_aliases = book;
        s.focus = focus.filter(|f| s.graph.subgraphs.contains_key(f));
        s.cache = cache;
        s
    }

    /// The node names handed out in every graph ("" = the main graph).
    pub fn alias_book(&self) -> AliasBook {
        let mut b = self.other_aliases.clone();
        b.insert(String::new(), self.aliases.clone());
        b.retain(|_, v| !v.is_empty());
        b
    }

    /// The graph commands work on when they get no `graph` parameter (`None`: the main graph).
    pub fn focus(&self) -> Option<&str> {
        self.focus.as_deref()
    }

    pub fn set_focus(&mut self, f: Option<String>) {
        self.focus = f.filter(|f| self.graph.subgraphs.contains_key(f));
    }

    /// Forget node names and focus (the package was replaced).
    fn reset_names(&mut self) {
        self.aliases.clear();
        self.other_aliases.clear();
        self.focus = None;
    }

    /// Work on graph `name` of the package: it becomes `self.graph` and sees the package's other graphs and
    /// resources, as when the editor opens it. [`Session::leave`] puts everything back.
    fn enter(&mut self, name: &str) {
        let Some(mut w) = self.graph.subgraphs.remove(name) else { return };
        let mut root = std::mem::take(&mut self.graph);
        let own_subgraphs = std::mem::replace(&mut w.subgraphs, std::mem::take(&mut root.subgraphs));
        native::add_owner(&mut w.subgraphs, &mut root);
        let own_resources = std::mem::replace(&mut w.resources, std::mem::take(&mut root.resources));
        let seed = std::mem::replace(&mut w.seed_offset, root.seed_offset);
        self.outer = Some(Outer { name: name.to_string(), root, seed, own_subgraphs, own_resources });
        self.graph = w;
        let mine = self.other_aliases.remove(name).unwrap_or_default();
        let main = std::mem::replace(&mut self.aliases, mine);
        self.other_aliases.insert(String::new(), main);
    }

    /// Put the graph worked on back into the package (see [`Session::enter`]).
    fn leave(&mut self) {
        let Some(o) = self.outer.take() else { return };
        let mut root = o.root;
        let mut w = std::mem::take(&mut self.graph);
        native::adopt_package(&mut root, &w);
        native::drop_owner(&mut w.subgraphs, &root);
        root.subgraphs = std::mem::replace(&mut w.subgraphs, o.own_subgraphs);
        root.resources = std::mem::replace(&mut w.resources, o.own_resources);
        if w.seed_offset == root.seed_offset {
            w.seed_offset = o.seed;
        }
        root.subgraphs.insert(o.name.clone(), w);
        self.graph = root;
        let main = self.other_aliases.remove("").unwrap_or_default();
        let mine = std::mem::replace(&mut self.aliases, main);
        if mine.is_empty() {
            self.other_aliases.remove(&o.name);
        } else {
            self.other_aliases.insert(o.name, mine);
        }
    }

    /// Name of the graph being worked on.
    fn current_name(&self) -> String {
        match &self.outer {
            Some(o) => o.name.clone(),
            None => main_name(&self.graph),
        }
    }

    /// The package (also while a command works on one of its other graphs).
    fn package(&self) -> &Graph {
        self.outer.as_ref().map(|o| &o.root).unwrap_or(&self.graph)
    }

    /// A session over a graph that lives somewhere else (the open editor): `aliases` are the node names handed
    /// out so far and `cache` is shared between calls so repeated renders reuse earlier results. The editor keeps
    /// the real undo history; check [`Session::take_changed`] to see whether the graph must be taken back.
    pub fn from_parts(graph: Graph, path: Option<PathBuf>, dirty: bool, aliases: BTreeMap<String, u32>, cache: Arc<NodeCache>) -> Session {
        let mut s = Session::from_graph(graph, path);
        s.dirty = dirty;
        s.aliases = aliases;
        s.cache = cache;
        s
    }

    pub fn aliases(&self) -> &BTreeMap<String, u32> {
        &self.aliases
    }

    pub fn into_graph(self) -> Graph {
        self.graph
    }

    /// True once if an edit changed the graph since the last call.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Run one command. Never panics; an edit that fails leaves the graph unchanged.
    pub fn execute(&mut self, id: &str, params: &Json) -> Result<Outcome, String> {
        let cmd = find_tool(id).ok_or_else(|| format!("unknown command '{id}' (command.list shows them all)"))?;
        let empty = Json::obj();
        let params = match params {
            Json::Null => &empty,
            Json::Obj(_) => params,
            _ => return Err("the parameters must be an object".into()),
        };
        check_args(cmd, params)?;
        let r = match cmd.id {
            "edit.undo" => self.do_undo(),
            "edit.redo" => self.do_redo(),
            "batch" => self.do_batch(params),
            _ => self.run_one(cmd, params, true),
        };
        if r.is_ok() && cmd.kind != Kind::Query {
            self.log.push(cmd.id.to_string());
            if self.log.len() > 50 {
                self.log.remove(0);
            }
        }
        r
    }

    fn run_one(&mut self, cmd: &'static Command, params: &Json, record: bool) -> Result<Outcome, String> {
        // the graph the command works on: its `graph` parameter, else the focused graph
        let target = if scoped(cmd.id) {
            let want = params.get("graph").and_then(Json::as_str);
            if want.is_none() && self.focus.as_ref().map(|f| !self.graph.subgraphs.contains_key(f)).unwrap_or(false) {
                // the focused graph went away (undo, delete): back to the main graph
                self.focus = None;
            }
            match want {
                Some(w) => resolve_graph(&self.graph, w)?,
                None => self.focus.clone(),
            }
        } else {
            None
        };
        let before = (cmd.kind == Kind::Edit).then(|| self.graph.clone());
        let aliases = (self.aliases.clone(), self.other_aliases.clone(), self.focus.clone());
        let run = cmd.run;
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(t) = &target {
                self.enter(t);
            }
            let r = run(self, params);
            self.prune_names();
            r
        }));
        self.leave();
        let res = match res {
            Ok(r) => r,
            Err(p) => Err(format!("internal error in {}: {}", cmd.id, panic_text(p))),
        };
        let restore = |s: &mut Session| {
            s.aliases = aliases.0.clone();
            s.other_aliases = aliases.1.clone();
            s.focus = aliases.2.clone();
        };
        match (res, before) {
            (Ok(o), Some(b)) => {
                if record {
                    self.push_undo(b);
                }
                self.dirty = true;
                self.changed = true;
                Ok(o)
            }
            (Ok(o), None) => Ok(o),
            (Err(e), Some(b)) => {
                self.graph = b;
                restore(self);
                Err(e)
            }
            (Err(e), None) => Err(e),
        }
    }

    fn push_undo(&mut self, g: Graph) {
        self.undo.push(g);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn do_undo(&mut self) -> Result<Outcome, String> {
        let prev = self.undo.pop().ok_or("nothing to undo")?;
        self.redo.push(std::mem::replace(&mut self.graph, prev));
        self.dirty = true;
        self.changed = true;
        Outcome::data(Json::obj().with("undone", true).with("undo_steps", self.undo.len()).with("redo_steps", self.redo.len()))
    }

    fn do_redo(&mut self) -> Result<Outcome, String> {
        let next = self.redo.pop().ok_or("nothing to redo")?;
        self.undo.push(std::mem::replace(&mut self.graph, next));
        self.dirty = true;
        self.changed = true;
        Outcome::data(Json::obj().with("redone", true).with("undo_steps", self.undo.len()).with("redo_steps", self.redo.len()))
    }

    fn do_batch(&mut self, p: &Json) -> Result<Outcome, String> {
        let steps = p.get("steps").and_then(Json::as_arr).ok_or("steps must be a list")?;
        if steps.len() > MAX_BATCH {
            return Err(format!("a batch can hold at most {MAX_BATCH} commands"));
        }
        let before = self.graph.clone();
        let saved = (self.aliases.clone(), self.other_aliases.clone(), self.focus.clone());
        let mut results: Vec<Json> = Vec::new();
        let mut edits = 0;
        for (i, s) in steps.iter().enumerate() {
            let step = |m: String| format!("step {} ({}): {m}", i + 1, s.get("command").and_then(Json::as_str).unwrap_or("?"));
            let name = match s.get("command").or_else(|| s.get("id")).and_then(Json::as_str) {
                Some(n) => n,
                None => {
                    self.graph = before;
                    (self.aliases, self.other_aliases, self.focus) = saved;
                    return Err(step("every step needs a \"command\"".into()));
                }
            };
            let cmd = match find_tool(name) {
                Some(c) if !matches!(c.id, "batch" | "edit.undo" | "edit.redo") => c,
                _ => {
                    self.graph = before;
                    (self.aliases, self.other_aliases, self.focus) = saved;
                    return Err(step("not a command that can be used in a batch".into()));
                }
            };
            let empty = Json::obj();
            let sp = s.get("params").filter(|v| !v.is_null()).unwrap_or(&empty);
            let r = check_args(cmd, sp).and_then(|_| self.run_one(cmd, sp, false));
            match r {
                Ok(o) => {
                    if cmd.kind == Kind::Edit {
                        edits += 1;
                    }
                    results.push(o.data);
                }
                Err(e) => {
                    self.graph = before;
                    (self.aliases, self.other_aliases, self.focus) = saved;
                    return Err(step(e));
                }
            }
        }
        if edits > 0 {
            self.push_undo(before);
            self.dirty = true;
            self.changed = true;
        }
        Outcome::data(Json::obj().with("steps", steps.len()).with("results", Json::Arr(results)))
    }

    // ---------------------------------------------------------------- helpers

    fn node_ref(&self, v: &Json) -> Result<u32, String> {
        let id = match v {
            Json::Num(_) => v.as_u32().ok_or("a node id is a whole number")?,
            Json::Str(s) => {
                let t = s.trim().trim_start_matches('@');
                match t.parse::<u32>() {
                    Ok(n) => n,
                    Err(_) => *self.aliases.get(t).ok_or_else(|| format!("no node named '{t}'"))?,
                }
            }
            _ => return Err("a node is an id (number) or a name".into()),
        };
        if self.graph.nodes.contains_key(&id) {
            Ok(id)
        } else {
            Err(format!("there is no node {id}"))
        }
    }

    fn arg_node(&self, p: &Json, key: &str) -> Result<u32, String> {
        self.node_ref(p.get(key).ok_or_else(|| format!("missing '{key}'"))?).map_err(|e| format!("{key}: {e}"))
    }

    fn nodes_arg(&self, p: &Json, key: &str) -> Result<Vec<u32>, String> {
        let a = p.get(key).ok_or_else(|| format!("missing '{key}'"))?;
        match a {
            Json::Arr(v) => v.iter().map(|x| self.node_ref(x)).collect::<Result<_, _>>().map_err(|e| format!("{key}: {e}")),
            one => Ok(vec![self.node_ref(one).map_err(|e| format!("{key}: {e}"))?]),
        }
    }

    fn alias_of(&self, id: u32) -> Option<&str> {
        self.aliases.iter().find(|(_, v)| **v == id).map(|(k, _)| k.as_str())
    }

    /// Forget names of nodes that no longer exist in the graph being worked on.
    fn prune_names(&mut self) {
        let g = &self.graph;
        self.aliases.retain(|_, v| g.nodes.contains_key(v));
    }

    fn resolve_path(&self, p: &str) -> Result<PathBuf, String> {
        match &self.root {
            Some(root) => confine(root, p),
            None => Ok(PathBuf::from(p)),
        }
    }

    fn eval(&self, id: u32, size: usize, out: Option<&str>) -> Result<Tex, String> {
        if let Some(sd) = &self.graph.sd {
            if sd.role != native::Role::Graph {
                // a function or FX-map graph: drawn with `id` as its result
                let mut probe = self.graph.clone();
                if let Some(psd) = probe.sd.as_mut() {
                    psd.root = Some(id);
                }
                let (host, show) = native::preview_host(&probe).ok_or("this graph cannot be drawn on its own")?;
                let r = host.evaluate_all_cached(size, &[show], 0, &self.cache, Some(show), false, &mut |_| {});
                return r.kept.get(&show).map(|t| (**t).clone()).ok_or_else(|| r.errors.values().next().cloned().unwrap_or_else(|| "the graph produced no image".into()));
            }
            let n = self.graph.nodes.get(&id).ok_or("no such node")?;
            if let Some(o) = out {
                // one output of a node with several: read through a Dot linked to it, as a link in the file reads it
                let outs = native::outputs(n, &self.graph.subgraphs);
                let label = native::label(n, &self.graph);
                if !outs.iter().any(|x| x == o) {
                    return Err(if outs.len() < 2 { format!("{label} has a single output") } else { format!("{label} has no output '{o}' (outputs: {})", outs.join(", ")) });
                }
                if outs.len() > 1 {
                    let mut probe = self.graph.clone();
                    let dot = native::free_id(&probe);
                    let mut d = native::new_node("sbs:passthrough", None, n.pos);
                    d.ins.insert("input".into(), id);
                    d.in_out.insert("input".into(), o.to_string());
                    probe.nodes.insert(dot, d);
                    let low = native::lower(&probe)?.graph;
                    if !low.nodes.contains_key(&dot) {
                        return Err("this output gives no image".into());
                    }
                    let mut s2 = Session { graph: low, ..Session::from_graph(Graph::default(), None) };
                    s2.cache = self.cache.clone();
                    return s2.eval(dot, size, None);
                }
            }
            let low = native::lower(&self.graph)?.graph;
            if !low.nodes.contains_key(&id) {
                return Err("this node gives no image".into());
            }
            let mut s2 = Session { graph: low, ..Session::from_graph(Graph::default(), None) };
            s2.cache = self.cache.clone();
            return s2.eval(id, size, None);
        }
        let fam = family_of_graph(&self.graph);
        if fam != "texture" {
            // a function or pattern program is drawn by a host graph that runs it, with `id` as its result
            let end = if fam == "function" { "f_out" } else { "fxm_out" };
            let mut probe = self.graph.clone();
            let src = match probe.nodes.get(&id) {
                Some(n) if n.kind == end => n.ins.get("in").copied().ok_or_else(|| format!("the {end} node has no input yet"))?,
                Some(_) => id,
                None => return Err("no such node".into()),
            };
            probe.nodes.retain(|_, n| n.kind != end);
            let mut o = Node { kind: end.into(), ..Default::default() };
            o.ins.insert("in".into(), src);
            let oid = next_id(&probe);
            probe.nodes.insert(oid, o);
            let (host, show) = probe.preview_host().ok_or("not a function graph or pattern program")?;
            let r = host.evaluate_all_cached(size, &[show], 0, &self.cache, Some(show), false, &mut |_| {});
            if let Some(t) = r.kept.get(&show) {
                return Ok((**t).clone());
            }
            return Err(r.errors.values().next().cloned().unwrap_or_else(|| "the graph produced no image".into()));
        }
        let mut g = &self.graph;
        let tmp;
        let mut target = id;
        if let Some(o) = out {
            let src = self.graph.nodes.get(&id).ok_or("no such node")?;
            let outs = graph::node_outputs(&src.kind);
            if !outs.contains(&o) {
                return Err(if outs.is_empty() { format!("{} has a single output", src.kind) } else { format!("{} has no output '{o}' (outputs: {})", src.kind, outs.join(", ")) });
            }
            let mut c = self.graph.clone();
            let nid = c.nodes.keys().max().copied().unwrap_or(0) + 1;
            let mut n = Node { kind: "dot".into(), ..Default::default() };
            n.ins.insert("in".into(), id);
            n.in_out.insert("in".into(), o.to_string());
            c.nodes.insert(nid, n);
            tmp = c;
            g = &tmp;
            target = nid;
        }
        g.check_cycles().map_err(|e| e.to_string())?;
        let r = g.evaluate_all_cached(size, &[target], 0, &self.cache, Some(target), false, &mut |_| {});
        if let Some(t) = r.kept.get(&target) {
            return Ok((**t).clone());
        }
        let mut errs: Vec<(&u32, &String)> = r.errors.iter().collect();
        errs.sort();
        match errs.first() {
            Some((n, e)) => Err(format!("node {n} ({}): {e}", g.nodes.get(n).map(|x| x.kind.as_str()).unwrap_or("?"))),
            None => Err("the node produced no image".into()),
        }
    }
}

/// `p` as a path inside `root` (relative paths are taken from `root`). `..` and paths outside are refused.
pub fn confine(root: &Path, p: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(p);
    let full = if path.is_absolute() { path } else { root.join(path) };
    if full.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(format!("{p}: paths may not contain '..' (files are limited to {})", root.display()));
    }
    if !full.starts_with(root) {
        return Err(format!("{p}: outside the allowed folder {}", root.display()));
    }
    Ok(full)
}

/// Name of a package's main graph (its identifier).
fn main_name(g: &Graph) -> String {
    g.meta.get("identifier").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| "main".into())
}

/// Graph `name` of package `root`: `None` for the main graph (its identifier, "main" or ""), `Some` for one of
/// the other graphs.
pub fn resolve_graph(root: &Graph, name: &str) -> Result<Option<String>, String> {
    let name = name.trim();
    if root.subgraphs.contains_key(name) {
        return Ok(Some(name.to_string()));
    }
    if name.is_empty() || name == "main" || root.meta.get("identifier").map(|i| i == name).unwrap_or(false) {
        return Ok(None);
    }
    let mut names: Vec<String> = root.subgraphs.keys().cloned().collect();
    names.push(main_name(root));
    let hint = closest(name, &names);
    Err(if hint.is_empty() { format!("the package has no graph '{name}' (graph.list shows them)") } else { format!("the package has no graph '{name}'. Did you mean: {}?", hint.join(", ")) })
}

/// The family of nodes a graph holds: texture, function or pattern.
fn family_of_graph(g: &Graph) -> &'static str {
    match g.graph_kind() {
        "function" => "function",
        "fxmap" => "pattern",
        _ => "texture",
    }
}

fn check_args(cmd: &Command, p: &Json) -> Result<(), String> {
    let args = cmd.all_args();
    for a in &args {
        match p.get(a.name) {
            None | Some(Json::Null) if a.req => return Err(format!("{}: missing required parameter '{}' ({})", cmd.id, a.name, a.doc)),
            None | Some(Json::Null) => {}
            Some(v) => {
                let ok = match a.ty {
                    "string" => matches!(v, Json::Str(_)),
                    "number" => v.as_f64().is_some(),
                    "integer" => v.as_f64().map(|n| n.fract() == 0.0).unwrap_or(false),
                    "boolean" => matches!(v, Json::Bool(_)),
                    "object" => matches!(v, Json::Obj(_)),
                    "array" => matches!(v, Json::Arr(_)),
                    "node" => matches!(v, Json::Num(_) | Json::Str(_)),
                    "nodes" => matches!(v, Json::Arr(_) | Json::Num(_) | Json::Str(_)),
                    _ => true,
                };
                if !ok {
                    return Err(format!("{}: parameter '{}' must be of type {}", cmd.id, a.name, a.ty));
                }
            }
        }
    }
    if let Json::Obj(o) = p {
        for (k, _) in o {
            if !args.iter().any(|a| a.name == k) {
                let names: Vec<&str> = args.iter().map(|a| a.name).collect();
                return Err(format!("{}: unknown parameter '{k}' (parameters: {})", cmd.id, if names.is_empty() { "none".into() } else { names.join(", ") }));
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------ utilities

fn opt_usize(p: &Json, k: &str, lo: usize, hi: usize, def: usize) -> Result<usize, String> {
    match p.get(k).filter(|v| !v.is_null()) {
        None => Ok(def),
        Some(v) => {
            let n = v.as_f64().filter(|n| n.fract() == 0.0).ok_or_else(|| format!("{k} must be a whole number"))?;
            if n < lo as f64 || n > hi as f64 {
                return Err(format!("{k} must be between {lo} and {hi}"));
            }
            Ok(n as usize)
        }
    }
}

fn gerr(e: GError) -> String {
    e.to_string()
}

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + usize::from(ca != cb)).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

fn closest(word: &str, options: &[String]) -> Vec<String> {
    let w = word.to_lowercase();
    // a name that holds the word (`opacity` in `opacitymult`) is as close as names get
    let score = |o: &str| {
        let o = o.to_lowercase();
        if w.len() >= 3 && o.len() >= 3 && (o.contains(&w) || w.contains(&o)) {
            0
        } else {
            edit_distance(&w, &o)
        }
    };
    let mut v: Vec<(usize, &String)> = options.iter().map(|o| (score(o), o)).collect();
    v.sort();
    v.into_iter().filter(|(d, _)| *d <= 3.max(word.len() / 3)).take(3).map(|(_, o)| o.clone()).collect()
}

// ------------------------------------------------------------------------------------------ discovery

fn c_noop(_: &mut Session, _: &Json) -> Result<Outcome, String> {
    Outcome::data(Json::obj())
}

fn c_command_list(_: &mut Session, p: &Json) -> Result<Outcome, String> {
    let f = p.get("filter").and_then(Json::as_str).unwrap_or("").to_lowercase();
    let list: Vec<Json> = COMMANDS
        .iter()
        .filter(|c| f.is_empty() || c.id.contains(&f) || c.doc.to_lowercase().contains(&f))
        .map(|c| {
            let args: Vec<Json> = c.all_args().iter().map(|a| Json::obj().with("name", a.name).with("type", a.ty).with("required", a.req).with("doc", a.doc)).collect();
            Json::obj().with("id", c.id).with("tool", c.tool_name()).with("kind", match c.kind { Kind::Query => "query", Kind::Edit => "edit", Kind::Special => "special" }).with("doc", c.doc).with("params", Json::Arr(args))
        })
        .collect();
    Outcome::data(Json::obj().with("commands", Json::Arr(list)))
}

fn c_node_types(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_node_types(s, p)
}

fn c_node_describe(_: &mut Session, p: &Json) -> Result<Outcome, String> {
    let kind = p.get("kind").and_then(Json::as_str).unwrap_or("").trim();
    if kind.is_empty() {
        return Err("give a node kind (node.types lists them)".into());
    }
    let k = if kind.starts_with("sbs:") { kind.to_string() } else { format!("sbs:{kind}") };
    sd_node_describe(&k)
}

// ------------------------------------------------------------------------------------------ graph

fn c_graph_inspect(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_graph_inspect(s, p)
}

fn c_graph_validate(s: &mut Session, _: &Json) -> Result<Outcome, String> {
    let pr = sd_problems(&s.graph);
    let warn = sd_warnings(&s.graph);
    Outcome::data(
        Json::obj()
            .with("ok", pr.is_empty())
            .with("problems", Json::Arr(pr.into_iter().map(Json::from).collect::<Vec<_>>()))
            .with("warnings", Json::Arr(warn.into_iter().map(Json::from).collect::<Vec<_>>())),
    )
}

fn c_graph_text(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let text = match p.get("graph").and_then(Json::as_str) {
        Some(name) => native::save_text(&native::extract_package(&s.graph, name)?)?,
        None => native::save_text(&s.graph)?,
    };
    Outcome::data(Json::obj().with("format", "sbs").with("file_kind", "package (.sbs)").with("text", text))
}

fn c_graph_new(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_new_package(s, p)
}

fn c_graph_open(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let path = s.resolve_path(p.get("path").and_then(Json::as_str).unwrap_or(""))?;
    if path.extension().map(|e| e.eq_ignore_ascii_case("sbs")).unwrap_or(false) {
        // an .sbs package opens as it is: its own nodes and values, saved back with only the changes written
        let (g, warnings) = native::open(&path)?;
        let report = native::lower(&g).map(|l| l.report.summary()).unwrap_or_default();
        let graphs = native::all_graphs(&g).values().filter(|x| native::listed(x)).count();
        let (n, o) = (g.nodes.len(), g.output_nodes().len());
        s.graph = g;
        s.path = Some(path.clone());
        s.reset_names();
        return Outcome::data(Json::obj().with("file", path.display().to_string()).with("format", "sbs").with("graphs", graphs).with("nodes", n).with("outputs", o).with("rendering", report).with("warnings", Json::Arr(warnings.into_iter().map(Json::from).collect::<Vec<_>>())));
    }
    Err(format!("{}: only Substance packages (.sbs) can be opened", path.display()))
}

fn c_graph_load_text(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let text = p.get("text").and_then(Json::as_str).unwrap_or("");
    let head = text.trim_start();
    if head.is_empty() {
        return Err("text is empty: give the file text of a package".into());
    }
    if head.starts_with("<?xml") || head.starts_with("<package") {
        let (g, warnings) = native::load(text, None)?;
        let n = g.nodes.len();
        s.graph = g;
        s.reset_names();
        return Outcome::data(Json::obj().with("format", "sbs").with("nodes", n).with("warnings", Json::Arr(warnings.into_iter().map(Json::from).collect::<Vec<_>>())));
    }
    Err("this is not the text of an .sbs package (it starts with <?xml or <package)".into())
}

fn c_graph_save(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let path = match p.get("path").and_then(Json::as_str) {
        Some(x) => s.resolve_path(x)?,
        None => s.path.clone().ok_or("this graph has no file yet: give a path")?,
    };
    let path = if path.extension().map(|e| e.eq_ignore_ascii_case("sbs")).unwrap_or(false) { path } else { path.with_extension("sbs") };
    let text = native::save_text(&s.graph)?;
    crate::safeio::write_file(&path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))?;
    s.path = Some(path.clone());
    s.dirty = false;
    Outcome::data(Json::obj().with("saved", path.display().to_string()).with("format", "sbs"))
}

fn c_graph_set(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_graph_set(s, p)
}

fn c_graph_layout(s: &mut Session, _: &Json) -> Result<Outcome, String> {
    s.graph.check_cycles().map_err(gerr)?;
    let g = &mut s.graph;
    // column = longest path from a node without inputs
    let ids: Vec<u32> = g.nodes.keys().copied().collect();
    let mut depth: BTreeMap<u32, usize> = BTreeMap::new();
    for _ in 0..ids.len().max(1) {
        let mut changed = false;
        for id in &ids {
            let d = g.nodes[id].ins.values().filter_map(|i| depth.get(i)).map(|d| d + 1).max().unwrap_or(0);
            if depth.get(id) != Some(&d) {
                depth.insert(*id, d);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut cols: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
    for id in &ids {
        cols.entry(depth[id]).or_default().push(*id);
    }
    // order each column by the average row of its inputs so links stay short
    let mut row_of: BTreeMap<u32, f32> = BTreeMap::new();
    for (c, list) in cols.iter_mut() {
        if *c > 0 {
            list.sort_by(|a, b| {
                let avg = |i: &u32| {
                    let r: Vec<f32> = g.nodes[i].ins.values().filter_map(|x| row_of.get(x)).copied().collect();
                    if r.is_empty() {
                        f32::MAX
                    } else {
                        r.iter().sum::<f32>() / r.len() as f32
                    }
                };
                avg(a).partial_cmp(&avg(b)).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        for (r, id) in list.iter().enumerate() {
            row_of.insert(*id, r as f32);
        }
    }
    for (c, list) in &cols {
        for (r, id) in list.iter().enumerate() {
            if let Some(n) = g.nodes.get_mut(id) {
                n.pos = [*c as f32 * 170.0, r as f32 * 150.0];
            }
        }
    }
    Outcome::data(Json::obj().with("columns", cols.len()).with("nodes", ids.len()))
}

// ------------------------------------------------------------------------------------------ the package

fn kind_label(g: &Graph) -> &'static str {
    family_of_graph(g)
}

fn c_graph_list(s: &mut Session, _: &Json) -> Result<Outcome, String> {
    sd_graph_list(s)
}

fn c_graph_focus(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let want = p.get("graph").and_then(Json::as_str).unwrap_or("");
    let t = resolve_graph(&s.graph, want)?;
    s.focus = t.clone();
    let (name, kind) = match &t {
        Some(n) => (n.clone(), s.graph.subgraphs.get(n).map(kind_label).unwrap_or("texture")),
        None => (main_name(&s.graph), kind_label(&s.graph)),
    };
    Outcome::data(Json::obj().with("focused", name).with("kind", kind))
}

fn c_graph_create(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_graph_create(s, p)
}

fn c_graph_rename(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_graph_rename(s, p)
}

fn c_graph_delete(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let name = p.get("graph").and_then(Json::as_str).unwrap_or("");
    let Some(n) = resolve_graph(&s.graph, name)? else {
        return Err("the main graph cannot be deleted".into());
    };
    s.graph.remove_graph(&n)?;
    s.other_aliases.remove(&n);
    if s.focus.as_deref() == Some(n.as_str()) {
        s.focus = None;
    }
    Outcome::data(Json::obj().with("deleted", n).with("graphs", s.graph.subgraphs.len() + 1))
}

fn c_graph_duplicate(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let src = match p.get("graph").and_then(Json::as_str) {
        Some(n) => resolve_graph(&s.graph, n)?,
        None => s.focus.clone().filter(|f| s.graph.subgraphs.contains_key(f)),
    };
    let new = s.graph.duplicate_graph(src.as_deref(), p.get("name").and_then(Json::as_str))?;
    Outcome::data(Json::obj().with("created", new))
}

fn c_graph_add_file(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let path = s.resolve_path(p.get("path").and_then(Json::as_str).unwrap_or(""))?;
    if !path.extension().map(|e| e.eq_ignore_ascii_case("sbs")).unwrap_or(false) {
        return Err(format!("{}: only the graphs of other .sbs packages can be added", path.display()));
    }
    let (other, mut warnings) = native::open(&path)?;
    let (name, n, more) = native::add_package(&mut s.graph, other)?;
    warnings.extend(more);
    Outcome::data(Json::obj().with("added", name).with("with_graphs", n).with("warnings", Json::Arr(warnings.into_iter().map(Json::from).collect::<Vec<_>>())))
}

// ------------------------------------------------------------------------------------------ resources

fn c_spline_set(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_spline_set(s, p)
}

fn c_spline_get(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_spline_get(s, p)
}

fn c_resource_list(s: &mut Session, _: &Json) -> Result<Outcome, String> {
    let list: Vec<Json> = match s.package().sd.as_ref() {
        Some(sd) => sd.pkg.resources.iter().map(|r| Json::obj().with("name", r.path()).with("type", r.kind.as_str()).with("embedded", r.embedded.is_some()).with("file", r.filepath.clone())).collect(),
        None => Vec::new(),
    };
    Outcome::data(Json::obj().with("count", list.len()).with("resources", Json::Arr(list)))
}

// ------------------------------------------------------------------------------------------ nodes

fn next_id(g: &Graph) -> u32 {
    g.nodes.keys().max().copied().unwrap_or(0) + 1
}

/// A free spot to the right of the node's inputs (or below everything when it has none).
fn free_pos(g: &Graph, inputs: &BTreeMap<String, u32>) -> [f32; 2] {
    let srcs: Vec<[f32; 2]> = inputs.values().filter_map(|i| g.nodes.get(i)).map(|n| n.pos).collect();
    let mut pos = if srcs.is_empty() {
        let bottom = g.nodes.values().map(|n| n.pos[1]).fold(f32::MIN, f32::max);
        [0.0, if g.nodes.is_empty() { 0.0 } else { bottom + 130.0 }]
    } else {
        [srcs.iter().map(|p| p[0]).fold(f32::MIN, f32::max) + 170.0, srcs.iter().map(|p| p[1]).sum::<f32>() / srcs.len() as f32]
    };
    for _ in 0..200 {
        if g.nodes.values().any(|n| (n.pos[0] - pos[0]).abs() < 150.0 && (n.pos[1] - pos[1]).abs() < 120.0) {
            pos[1] += 130.0;
        } else {
            break;
        }
    }
    pos
}

fn parse_link(s: &Session, v: &Json) -> Result<(u32, Option<String>), String> {
    match v {
        Json::Obj(_) => {
            let id = s.node_ref(v.get("node").ok_or("a link object needs \"node\"")?)?;
            Ok((id, v.get("output").and_then(Json::as_str).map(str::to_string)))
        }
        other => Ok((s.node_ref(other)?, None)),
    }
}

fn c_node_add(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_node_add(s, p)
}

fn c_node_set(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_node_set(s, p)
}

fn c_node_params(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_node_params(s, p)
}

fn c_node_drive(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_node_drive(s, p)
}

fn c_graph_extract(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_graph_extract(s, p)
}

fn c_graph_outline(s: &mut Session, _: &Json) -> Result<Outcome, String> {
    Outcome::data(Json::obj().with("graph", s.current_name()).with("outline", sd_outline(s)))
}

fn c_render_review(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_render_review(s, p)
}

fn c_macro_add(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_macro_add(s, p)
}

fn c_node_expression(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_node_expression(s, p)
}

fn c_node_duplicate(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_node_duplicate(s, p)
}

fn c_node_set_result(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_node_set_result(s, p)
}

fn c_comment_remove(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let i = opt_usize(p, "index", 0, usize::MAX >> 1, 0)?;
    if i >= s.graph.comments.len() {
        return Err(format!("there are {} comment(s); index {i} does not exist", s.graph.comments.len()));
    }
    s.graph.comments.remove(i);
    Outcome::data(Json::obj().with("comments", s.graph.comments.len()))
}

fn c_node_remove(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_node_remove(s, p)
}

fn c_node_connect(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let from = s.arg_node(p, "from")?;
    let to = s.arg_node(p, "to")?;
    let out = p.get("output").and_then(Json::as_str).map(str::to_string);
    let n = s.graph.nodes[&to].clone();
    let slot = match p.get("slot").and_then(Json::as_str) {
        Some(x) => x.to_string(),
        None => {
            let slots = native::slots(&n, &s.graph.subgraphs);
            slots.iter().find(|x| !n.ins.contains_key(*x)).or(slots.first()).cloned().ok_or_else(|| format!("{} has no inputs", n.kind))?
        }
    };
    sd_set_link(&mut s.graph, to, &slot, from, out)?;
    Outcome::data(Json::obj().with("from", from).with("to", to).with("slot", slot))
}

fn c_node_disconnect(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let to = s.arg_node(p, "to")?;
    let slot = p.get("slot").and_then(Json::as_str).unwrap_or("");
    let n = s.graph.nodes.get_mut(&to).ok_or("no such node")?;
    if n.ins.remove(slot).is_none() {
        return Err(format!("input '{slot}' of node {to} is not connected"));
    }
    n.in_out.remove(slot);
    Outcome::data(Json::obj().with("to", to).with("slot", slot))
}

fn c_node_move(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let id = s.arg_node(p, NODE)?;
    let (x, y) = (p.get("x").and_then(Json::as_f32).ok_or("x must be a number")?, p.get("y").and_then(Json::as_f32).ok_or("y must be a number")?);
    if let Some(n) = s.graph.nodes.get_mut(&id) {
        n.pos = [x, y];
    }
    Outcome::data(Json::obj().with("id", id))
}

fn c_output_add(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_output_add(s, p)
}

fn c_output_remove(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_output_remove(s, p)
}

fn c_graph_expose(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_graph_expose(s, p)
}

fn c_graph_unexpose(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    sd_graph_unexpose(s, p)
}

fn c_graph_set_input(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let input = p.get("input").and_then(Json::as_str).ok_or("give input: the identifier or label of a graph parameter")?.to_string();
    let value: Option<Vec<f32>> = match p.get("value").filter(|v| !v.is_null()) {
        None => None,
        Some(v) => Some(match v {
            Json::Bool(b) => vec![if *b { 1.0 } else { 0.0 }],
            Json::Arr(a) => a.iter().map(|x| x.as_f32().filter(|f| f.is_finite()).ok_or("value: a list of numbers")).collect::<Result<Vec<f32>, _>>()?,
            _ => vec![v.as_f32().filter(|f| f.is_finite()).ok_or("value: a number, a list of numbers or true/false")?],
        }),
    };
    if value.as_ref().map(|v| v.is_empty() || v.len() > 4).unwrap_or(false) {
        return Err("value: 1 to 4 numbers".into());
    }
    let label = p.get("label").and_then(Json::as_str).map(|l| l.trim().to_string()).filter(|l| !l.is_empty());
    let range = match (p.get("min").and_then(Json::as_f32), p.get("max").and_then(Json::as_f32)) {
        (Some(a), Some(b)) if a.is_finite() && b.is_finite() && b > a => Some((a, b)),
        (None, None) => None,
        _ => return Err("give both min and max, min smaller than max".into()),
    };
    if value.is_none() && label.is_none() && range.is_none() {
        return Err("give at least one of value, label, min/max".into());
    }
    let id = native::set_input(&mut s.graph, &input, value.as_deref(), label.as_deref(), range)?;
    Outcome::data(Json::obj().with("input", id))
}

fn c_comment_add(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let text = p.get("text").and_then(Json::as_str).unwrap_or("").replace(['#', '\n'], " ");
    let f = |k: &str, d: f32| p.get(k).and_then(Json::as_f32).unwrap_or(d);
    s.graph.comments.push(Comment { text, pos: [f("x", 0.0), f("y", -160.0)], size: [f("width", 300.0).clamp(40.0, 4000.0), f("height", 120.0).clamp(30.0, 4000.0)], color: 0 });
    Outcome::data(Json::obj().with("comments", s.graph.comments.len()))
}

// ------------------------------------------------------------------------------------------ rendering

fn stats_json(t: &Tex) -> Json {
    let n = t.px.len().max(1) as f64;
    let (mut mn, mut mx, mut sum) = ([f32::MAX; 4], [f32::MIN; 4], [0f64; 4]);
    let mut bad = 0usize;
    for p in &t.px {
        for c in 0..4 {
            if !p[c].is_finite() {
                bad += 1;
                continue;
            }
            mn[c] = mn[c].min(p[c]);
            mx[c] = mx[c].max(p[c]);
            sum[c] += p[c] as f64;
        }
    }
    let ch = |a: [f32; 4]| Json::Arr(a.iter().map(|v| Json::from(if v.abs() > 1e30 { 0.0 } else { (*v * 10000.0).round() / 10000.0 })).collect());
    let mean: Vec<Json> = sum.iter().map(|v| Json::from(((v / n) * 10000.0).round() / 10000.0)).collect();
    let flat = (0..3).all(|c| mx[c] - mn[c] < 1e-5);
    let l = |p: &[f32; 4]| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
    let (w, h) = (t.w, t.h);
    // tiling check: the step across the tile border (last line -> first line) compared with the largest step between
    // neighbouring lines inside the image. A tileable image has no step at the border that is bigger than the steps
    // it already has inside (hard bands, brick joints), so the value stays near or below 1; a seam shows up as a
    // border step far larger than anything inside.
    let seam = |horizontal: bool| -> f32 {
        // horizontal = compare the left and right edges (columns); otherwise the top and bottom edges (rows)
        let (lines, len) = if horizontal { (w, h) } else { (h, w) };
        if lines < 4 || len < 1 {
            return 1.0;
        }
        let at = |line: usize, k: usize| if horizontal { l(&t.px[k * w + line]) } else { l(&t.px[line * w + k]) };
        let step = |a: usize, b: usize| (0..len).map(|k| (at(a, k) - at(b, k)).abs() as f64).sum::<f64>() / len as f64;
        let mut inside: Vec<f64> = (0..lines - 1).map(|i| step(i, i + 1)).collect();
        let border = step(lines - 1, 0);
        inside.sort_by(|a, b| a.total_cmp(b));
        let top = inside[inside.len() - 1];
        let mean = inside.iter().sum::<f64>() / inside.len() as f64;
        let scale = top.max(mean * 2.0);
        if scale < 1e-6 {
            if border < 1e-5 {
                1.0
            } else {
                99.0
            }
        } else {
            (border / scale).min(99.0) as f32
        }
    };
    let r2 = |v: f32| (v * 100.0).round() / 100.0;
    let (sh, sv) = (r2(seam(true)), r2(seam(false)));
    let mut j = Json::obj().with("width", w).with("height", h).with("min", ch(mn)).with("max", ch(mx)).with("mean", Json::Arr(mean));
    j.set("flat", flat);
    j.set("seam_horizontal", sh);
    j.set("seam_vertical", sv);
    j.set("tiles_seamlessly", sh < 1.5 && sv < 1.5);
    if bad > 0 {
        j.set("non_finite_values", bad);
    }
    j
}

fn checker(x: usize, y: usize) -> f32 {
    if (x / 8 + y / 8) % 2 == 0 {
        0.55
    } else {
        0.4
    }
}

/// The picture to show for a texture: a channel, optionally stretched, with transparency drawn over a checkerboard.
fn display(t: &Tex, channel: &str, normalize: bool) -> Result<Tex, String> {
    let mut out = Tex { w: t.w, h: t.h, px: Vec::with_capacity(t.px.len()) };
    let lum = |p: &[f32; 4]| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
    let pick = |p: &[f32; 4]| -> [f32; 3] {
        match channel {
            "r" => [p[0]; 3],
            "g" => [p[1]; 3],
            "b" => [p[2]; 3],
            "alpha" | "a" => [p[3]; 3],
            "luma" => [lum(p); 3],
            _ => [p[0], p[1], p[2]],
        }
    };
    if !matches!(channel, "rgb" | "r" | "g" | "b" | "alpha" | "a" | "luma") {
        return Err("channel must be rgb, r, g, b, alpha or luma".into());
    }
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    if normalize {
        for p in &t.px {
            for v in pick(p) {
                if v.is_finite() {
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
            }
        }
    }
    let scale = if normalize && hi > lo { 1.0 / (hi - lo) } else { 1.0 };
    let base = if normalize && hi > lo { lo } else { 0.0 };
    for (i, p) in t.px.iter().enumerate() {
        let c = pick(p);
        let mut o = [(c[0] - base) * scale, (c[1] - base) * scale, (c[2] - base) * scale];
        let a = if matches!(channel, "alpha" | "a") { 1.0 } else { p[3].clamp(0.0, 1.0) };
        if a < 0.999 {
            let k = checker(i % t.w.max(1), i / t.w.max(1));
            for v in o.iter_mut() {
                *v = *v * a + k * (1.0 - a);
            }
        }
        for v in o.iter_mut() {
            *v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 1.0 };
        }
        out.px.push([o[0], o[1], o[2], 1.0]);
    }
    Ok(out)
}

fn tiled(t: &Tex, n: usize) -> Tex {
    if n <= 1 {
        return t.clone();
    }
    let (w, h) = (t.w * n, t.h * n);
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            px.push(t.px[(y % t.h) * t.w + (x % t.w)]);
        }
    }
    Tex { w, h, px }
}

fn png_of(t: &Tex) -> Vec<u8> {
    crate::png::encode(t.w, t.h, &t.px, 8)
}

fn fit_cell(t: &Tex, s: usize) -> Tex {
    let mut px = Vec::with_capacity(s * s);
    for y in 0..s {
        for x in 0..s {
            let sx = (x * t.w / s).min(t.w.saturating_sub(1));
            let sy = (y * t.h / s).min(t.h.saturating_sub(1));
            px.push(t.px[sy * t.w + sx]);
        }
    }
    Tex { w: s, h: s, px }
}

fn sheet(cells: &[Tex], size: usize, cols: usize) -> Tex {
    let cols = cols.clamp(1, cells.len().max(1));
    let rows = cells.len().div_ceil(cols).max(1);
    let gap = 4;
    let (w, h) = (cols * size + (cols + 1) * gap, rows * size + (rows + 1) * gap);
    let mut px = vec![[0.1, 0.1, 0.1, 1.0]; w * h];
    for (i, c) in cells.iter().enumerate() {
        let (cx, cy) = (gap + (i % cols) * (size + gap), gap + (i / cols) * (size + gap));
        for y in 0..size {
            for x in 0..size {
                px[(cy + y) * w + cx + x] = c.px[y * size + x];
            }
        }
    }
    Tex { w, h, px }
}

fn c_render_node(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let id = s.arg_node(p, NODE)?;
    let size = opt_usize(p, "size", 16, MAX_RENDER, 256)?;
    let tile = opt_usize(p, "tile", 1, 4, 1)?;
    let channel = p.get("channel").and_then(Json::as_str).unwrap_or("rgb");
    let normalize = p.get("normalize").and_then(Json::as_bool).unwrap_or(false);
    let want_image = p.get("image").and_then(Json::as_bool).unwrap_or(true);
    let out = p.get("output").and_then(Json::as_str);
    let base = if tile > 1 { (size / tile).max(32) } else { size };
    let t = s.eval(id, base, out)?;
    let mut j = stats_json(&t);
    j.set("node", id);
    j.set("kind", s.graph.nodes.get(&id).map(|n| n.kind.as_str()));
    if let Some(info) = flood_info(s, id, base, &t) {
        j.set("flood_fill", info);
    }
    let mut images = Vec::new();
    if want_image {
        let d = display(&tiled(&t, tile), channel, normalize)?;
        images.push(Image { label: format!("node {id}"), png: png_of(&d) });
    }
    Ok(Outcome { data: j, images })
}

/// For the Flood Fill node and the nodes that read its result: how many regions the mask was split into, so merged
/// shapes (a gap too thin at this size) are visible in the numbers instead of only in the picture.
fn flood_info(s: &Session, id: u32, size: usize, t: &Tex) -> Option<Json> {
    let is_fill = |k: &str| k.ends_with("/flood_fill") || k == "flood_fill";
    let mut at = id;
    let mut tex = None;
    let mut fill_node = id;
    for _ in 0..6 {
        let n = s.graph.nodes.get(&at)?;
        if !n.kind.contains("flood_fill") {
            return None;
        }
        if is_fill(&n.kind) {
            tex = Some(if at == id { t.clone() } else { s.eval(at, size, None).ok()? });
            fill_node = at;
            break;
        }
        at = *n.ins.get("input").or_else(|| n.ins.values().next())?;
    }
    let tex = tex?;
    let (_, regions) = crate::ops::flood_regions(&tex);
    let counts: Vec<u32> = regions.iter().skip(1).map(|r| r.2).collect();
    let covered: u64 = counts.iter().map(|c| *c as u64).sum();
    let largest = counts.iter().copied().max().unwrap_or(0);
    let tiny = counts.iter().filter(|c| **c < 4).count();
    let share = if covered > 0 { largest as f64 / covered as f64 } else { 0.0 };
    let mut j = Json::obj().with("regions", counts.len()).with("largest_region_share", (share * 1000.0).round() / 1000.0).with("tiny_regions", tiny);
    if counts.len() > 1 && share > 0.4 {
        j.set(
            "warning",
            format!(
                "one region holds {:.0}% of the shapes' pixels: neighbouring shapes probably merged through a gap that is too thin at {size} px (or touches at a corner through grey edge pixels). Widen the gaps, render larger, or raise the Flood Fill threshold.",
                share * 100.0
            ),
        );
    } else if counts.len() == 1 && covered as usize > tex.px.len() / 2 {
        j.set("warning", "the whole mask is one region: the gaps between shapes did not separate them at this size (Flood Fill needs dark gaps at least one pixel wide; raise the threshold or widen the gaps)");
    } else if tiny * 4 > counts.len().max(1) {
        j.set("warning", "many regions are only a few pixels: the mask is noisy or has grey speckles; lower the threshold or clean the mask first");
    }
    // the preview may be smaller than the graph: gaps thinner than a pixel merge shapes only here, so also count
    // the regions at the graph's own resolution
    let gs = s.graph.size.min(1024);
    if gs > size {
        if let Ok(big) = s.eval(fill_node, gs, None) {
            let (_, r2) = crate::ops::flood_regions(&big);
            let n2 = r2.iter().skip(1).filter(|r| r.2 > 0).count();
            j.set("regions_at_graph_size", n2);
            j.set("graph_size_used", gs);
            if n2 != counts.len() {
                j.set(
                    "resolution_note",
                    format!("{} regions at {size} px but {n2} at {gs} px: the preview is too small for the gaps (thinner than a pixel); the {gs} px count is the one that matches the real graph (render larger with size={gs})", counts.len()),
                );
                let note = format!("the preview ({size} px) and the graph ({gs} px) disagree: trust the {gs} px count; render with size={gs} to see it");
                j.set("warning", note);
            }
        }
    }
    Some(j)
}

fn cells_from(s: &Session, items: &[(String, u32)], size: usize) -> Result<Vec<Tex>, String> {
    let mut cells = Vec::new();
    for (label, id) in items {
        let t = s.eval(*id, size, None).map_err(|e| format!("{label}: {e}"))?;
        cells.push(fit_cell(&display(&t, "rgb", false)?, size));
    }
    Ok(cells)
}

fn c_render_outputs(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let size = opt_usize(p, "size", 32, 1024, 192)?;
    let cols = opt_usize(p, "columns", 1, 8, 3)?;
    let mut outs = s.graph.output_nodes();
    outs.sort_by_key(|o| OUTPUT_USAGES.iter().position(|x| *x == o.0).unwrap_or(99));
    let fam = family_of_graph(&s.graph);
    if fam != "texture" {
        let end = if fam == "function" { "f_out" } else { "fxm_out" };
        outs = s.graph.nodes.iter().filter(|(_, n)| n.kind == end).map(|(i, _)| ("result".to_string(), "result".to_string(), *i)).collect();
        if outs.is_empty() {
            return Err(format!("the graph has no {end} node yet"));
        }
    }
    if outs.is_empty() {
        return Err("the graph has no outputs yet (output.add)".into());
    }
    let items: Vec<(String, u32)> = outs.iter().map(|o| (o.1.clone(), o.2)).collect();
    let cells = cells_from(s, &items, size)?;
    let order: Vec<Json> = items.iter().map(|(n, _)| Json::from(n.as_str())).collect();
    Ok(Outcome { data: Json::obj().with("order_left_to_right_top_to_bottom", Json::Arr(order)), images: vec![Image { label: "outputs".into(), png: png_of(&sheet(&cells, size, cols)) }] })
}

fn c_render_sheet(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let size = opt_usize(p, "size", 32, 1024, 192)?;
    let cols = opt_usize(p, "columns", 1, 8, 3)?;
    let ids = s.nodes_arg(p, "nodes")?;
    if ids.is_empty() || ids.len() > 36 {
        return Err("give 1 to 36 nodes".into());
    }
    let items: Vec<(String, u32)> = ids.iter().map(|i| (format!("node {i}"), *i)).collect();
    let cells = cells_from(s, &items, size)?;
    let order: Vec<Json> = ids.iter().map(|i| Json::from(*i)).collect();
    Ok(Outcome { data: Json::obj().with("order_left_to_right_top_to_bottom", Json::Arr(order)), images: vec![Image { label: "sheet".into(), png: png_of(&sheet(&cells, size, cols)) }] })
}

/// The 3D view a command asks for: named options first, then `settings` (any key of the view settings).
/// The call's view options on top of the ones saved with the graph (`graph.set view`).
fn view_params(g: &Graph, p: &Json) -> Json {
    let Some(Json::Obj(saved)) = g.meta.get("mc_view").and_then(|t| Json::parse(t).ok()) else { return p.clone() };
    let mut out = p.clone();
    for (k, v) in saved {
        match (k.as_str(), out.get(&k).cloned(), &v) {
            (_, None, _) => out.set(&k, v),
            ("settings", Some(Json::Obj(mine)), Json::Obj(base)) => {
                let mut merged = Json::obj();
                for (bk, bv) in base {
                    merged.set(bk, bv.clone());
                }
                for (mk, mv) in mine {
                    merged.set(&mk, mv.clone());
                }
                out.set("settings", merged);
            }
            _ => {}
        }
    }
    out
}

fn view_from(p: &Json) -> Result<crate::render3d::View, String> {
    let mut v = crate::render3d::View { tess: 256, ..Default::default() };
    let text = |k: &str| p.get(k).and_then(|x| match x {
        Json::Str(t) => Some(t.clone()),
        Json::Num(_) | Json::Bool(_) => Some(x.to_text()),
        _ => None,
    });
    let set = |v: &mut crate::render3d::View, key: &str, val: &str| v.set(key, val).map_err(|e| format!("{key}: {e}"));
    for (arg, key) in [("shape", "shape"), ("scene", "scene"), ("channel", "channel"), ("camera", "camera"), ("rig", "rig"), ("post", "post"), ("tiling", "tiling"), ("relief", "relief"), ("bump", "bump"), ("normal_strength", "normal_amt"), ("exposure", "exposure"), ("ambient", "ambient"), ("yaw", "yaw"), ("pitch", "pitch"), ("env_rotation", "env.rotation"), ("env_exposure", "env.exposure"), ("background", "env.background")] {
        if let Some(t) = text(arg) {
            set(&mut v, key, &t)?;
        }
    }
    if let Some(e) = text("env") {
        if !crate::hdri::PRESETS.iter().any(|x| x.0 == e) {
            return Err(format!("env: unknown environment '{e}' ({})", crate::hdri::PRESETS.iter().map(|x| x.0).collect::<Vec<_>>().join(", ")));
        }
        set(&mut v, "env.preset", &e)?;
    }
    if let Some(d) = p.get("displacement").and_then(Json::as_f32) {
        // the height map moves the surface by this much (object units; the sphere's radius is 1); 0 turns it off
        set(&mut v, "height_scale", &d.to_string())?;
        if d > 0.0 && p.get("relief").is_none() {
            v.relief = crate::render3d::RELIEF_DISPLACE;
        }
    }
    if let Some(k) = p.get("light_intensity").and_then(Json::as_f32) {
        if !(0.0..=100.0).contains(&k) {
            return Err("light_intensity: a multiplier from 0 to 100".into());
        }
        for l in &mut v.lights {
            l.intensity *= k;
        }
    }
    if let Some(Json::Obj(o)) = p.get("settings") {
        for (k, val) in o {
            let t = match val {
                Json::Str(t) => t.clone(),
                other => other.to_text(),
            };
            if k == "env.file" || k == "mesh" {
                return Err(format!("settings.{k}: files cannot be set here (use the env presets)"));
            }
            set(&mut v, k, &t)?;
        }
    }
    Ok(v)
}

fn c_render_3d(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    if p.get("list").and_then(Json::as_bool).unwrap_or(false) {
        let mut j = Json::obj();
        for (what, names) in crate::cli3d::option_lists() {
            j.set(what, Json::Arr(names.into_iter().map(Json::from).collect::<Vec<_>>()));
        }
        j.set("settings", crate::render3d::View::default().to_text());
        j.set("note", "settings lists every view setting as `key value`; pass any of them in `settings` (e.g. {\"light.0.intensity\": 5, \"post.bloom\": 0})");
        return Outcome::data(j);
    }
    let size = opt_usize(p, "size", 16, 1024, 512)?;
    let tex = opt_usize(p, "tex", 16, 2048, 512)?;
    let pt = match p.get("mode").and_then(Json::as_str).unwrap_or("raster") {
        "raster" => false,
        "pt" | "path_traced" | "pathtraced" => true,
        m => return Err(format!("mode: raster or pt, not '{m}'")),
    };
    let spp = opt_usize(p, "spp", 1, 4096, 128)? as u32;
    let view = view_from(&view_params(&s.graph, p))?;
    s.graph.validate().map_err(|e| format!("cannot render yet: {e}"))?;
    let mm = crate::cli3d::MaterialMaps::of(&s.graph, tex)?;
    let img = crate::cli3d::render_picture(&mm, &view, size, size, pt, spp);
    let mut j = Json::obj().with("size", size).with("mode", if pt { "pt" } else { "raster" });
    if pt {
        j.set("spp", spp);
    }
    j.set("view", view.to_text());
    Ok(Outcome { data: j, images: vec![Image { label: "3d".into(), png: png_of(&img) }] })
}

fn c_export_materialx(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    use crate::mtlx::{self, NormalY, Shader};
    let dir = s.resolve_path(p.get("dir").and_then(Json::as_str).unwrap_or(""))?;
    let mut o = mtlx::Options::default();
    o.size = Some(opt_usize(p, "size", 16, 8192, s.graph.size)?);
    if let Some(v) = p.get("shader").and_then(Json::as_str) {
        o.shader = Shader::parse(v).ok_or("shader must be standard or openpbr")?;
    }
    if let Some(v) = p.get("normal").and_then(Json::as_str) {
        o.normal = NormalY::parse(v).ok_or("normal must be directx or opengl")?;
    }
    o.depth = match opt_usize(p, "depth", 8, 16, 8)? {
        8 => 8,
        16 => 16,
        _ => return Err("depth must be 8 or 16".into()),
    };
    if let Some(v) = p.get("displacement").and_then(Json::as_bool) {
        o.displacement = v;
    }
    if let Some(v) = p.get("bake_ao").and_then(Json::as_bool) {
        o.bake_ao = v;
    }
    o.name = p.get("name").and_then(Json::as_str).map(String::from).filter(|n| !n.is_empty());
    let r = mtlx::export(&s.graph, &dir, &o)?;
    Outcome::data(
        Json::obj()
            .with("mtlx", r.mtlx.display().to_string())
            .with("textures", Json::Arr(r.files.iter().map(|f| Json::from(f.display().to_string())).collect::<Vec<_>>()))
            .with("notes", Json::Arr(r.warnings.iter().map(|w| Json::from(w.as_str())).collect::<Vec<_>>())),
    )
}

fn c_export(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let dir = s.resolve_path(p.get("dir").and_then(Json::as_str).unwrap_or(""))?;
    let size = opt_usize(p, "size", 16, 8192, s.graph.size)?;
    let depth = match opt_usize(p, "depth", 8, 32, 8)? {
        8 => 8u8,
        16 => 16,
        32 => 32,
        _ => return Err("depth must be 8, 16 or 32".into()),
    };
    let fmt = match p.get("format").and_then(Json::as_str) {
        Some(f) => Format::parse(f).ok_or("format must be png, jpg, tiff, exr or hdr")?,
        None => Format::Png,
    };
    let maps = s.graph.evaluate(Some(size)).map_err(gerr)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut files = Vec::new();
    for (name, t) in &maps {
        let path = dir.join(format!("{name}.{}", fmt.ext()));
        crate::safeio::write_file(&path, &export::encode(fmt, t, if fmt == Format::Jpeg { 8 } else { depth }, 92)).map_err(|e| format!("{}: {e}", path.display()))?;
        files.push(Json::from(path.display().to_string()));
    }
    Outcome::data(Json::obj().with("written", Json::Arr(files)))
}

// ------------------------------------------------------------------------------------------ history

fn c_history(s: &mut Session, _: &Json) -> Result<Outcome, String> {
    Outcome::data(Json::obj().with("undo_steps", s.undo.len()).with("redo_steps", s.redo.len()).with("recent_commands", Json::Arr(s.log.iter().rev().take(20).map(|c| Json::from(c.as_str())).collect::<Vec<_>>())))
}

include!("session_sbs.rs");
