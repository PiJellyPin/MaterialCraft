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

    materialcraft mcp --graph wall.mcg --root ./work     # MCP on stdin/stdout, graph in memory
    materialcraft serve --graph wall.mcg                 # keep a graph and accept commands on a local port
    materialcraft exec node_add kind=voronoi params.scale=8 name=cells --graph wall.mcg --save
    materialcraft commands render                        # list commands matching a word

## Commands

`materialcraft commands` lists all of them with their parameters. Groups: `graph.*` (inspect, validate, layout,
set, expose, save, open), `node.*` (types, describe, add, set, connect, remove, ...), `output.*`, `render.*`
(node, outputs, sheet, 3d: pictures plus statistics such as flatness and tiling seams), `library.*`, `edit.*`
(undo, redo, history), `export`, and `batch` (many edits, all or nothing, one undo step, nodes referred to by
name).

Mistakes come back as plain explanations (unknown parameter with a suggestion, allowed choices, value clamped).

## Security

* The port listens on 127.0.0.1 only and requires a random access token, regenerated each time it is turned
  on and stored in a user-only file (`control.json` in the MaterialCraft config folder).
* Only JSON requests are accepted, so a web page cannot talk to the port.
* `--root DIR` limits every file an assistant can read or write to that folder.
* Turn the feature off from the same window; nothing listens while it is off.
