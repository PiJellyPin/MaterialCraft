# Independent implementation policy

MaterialCraft is an original program. Everything in this repository is written for this project and is licensed under Apache-2.0 OR MIT (see `LICENSE-APACHE`, `LICENSE-MIT`). This page says how we keep it that way. It is a working policy for contributors, not legal advice.

## What goes in the repository

- Code, shaders, algorithms, icons, fonts, sample graphs, images and text that the contributor wrote, or that come from a source whose licence allows redistribution under Apache-2.0 OR MIT, with the source named in `THIRD_PARTY_LICENSES.md` or `NOTICE`.
- Algorithms taken from published literature or from first principles. Procedural looks (noises, grunges, patterns) are our own recipes.
- Parameter names and value ranges are chosen for this program. Where a name is a plain technical term (blur radius, threshold, seed) it is used as such.

## What stays out

- No source code, files, graphs, presets, bitmaps, icons, fonts, screenshots, documentation text or sample content from any commercial or proprietary product.
- No decompiling, disassembling, traffic or memory inspection of other software, and no use of material obtained under a licence that forbids it.
- No reading or writing of another tool's file formats unless the format is publicly documented, the documentation's licence allows implementing it, and the maintainers have agreed.
  - `.sbs` packages (read and write): the format is described in Adobe's Automation Toolkit reference, published under the Apache-2.0 licence at github.com/AdobeDocs/substance-automation-toolkit; the maintainer asked for the importer and later for `.sbs` to become MaterialCraft's own file format (Oct 2026), so packages are opened, edited and saved as they are. `src/sbs.rs`, `src/sbs_import.rs`, `src/sbs_native.rs`, `src/sbs_defs.rs` and `src/sbs_atomic.txt` are written from that reference and checked against small `.sbs` files the maintainer saved for this purpose (kept in the ignored `sbs_samples/` folder). The built-in node types (`sbs_atomic.txt`) list the reference's identifiers, value types and creation defaults; their labels, groupings and slider ranges are MaterialCraft's own. A saved file keeps everything it was read with; only what the user changed is rewritten. Library nodes are written as references to the library (its package file name, graph identifier and parameter identifiers - interface facts, see below); nothing from the library itself is copied into a file. Only the user's own files are read: no library graphs, presets or other content of the program that writes the format are read, copied or shipped. Test files in the repository are written by hand.
  - So that imported `.sbs` files keep their look, the maintainer approved using the **interface facts** of the library nodes such files refer to: each node's input and output identifiers, value types, default values and value ranges, as found in an installed copy of the program that writes the format. Only these facts are used, to translate parameters and to choose MaterialCraft's own parameter defaults and ranges. No graphs, node wiring, functions, presets, images, labels, descriptions or documentation text are copied, and the library is never read at run time. The names MaterialCraft shows for the numbers of library drop-downs (`src/sbs_choices.txt`) are written from how our own adapters read each number, not taken from the library. How each MaterialCraft node computes its result is our own implementation. Renders made with that program are used only privately, on the maintainer's computer, to compare results; they are never committed. The extracted material stays in ignored folders (`/Functional Node Framework SBS`, `/reference`).
- No third-party trademarks in product, node, file-format or command names, and no wording that suggests affiliation or endorsement.
- No copying of another product's window layout, icon set, colour scheme or start screen. The editor's interface is designed for this project.
- Nothing from a vendor's tools, renders or files is committed, attached to issues, or kept in the repository tree. If a contributor compares output against other software privately, the result stays on their machine and the code is still written independently from public descriptions of the technique.

## For contributors

By opening a pull request you confirm that you wrote the change yourself (or have the right to contribute it), that it is not copied or translated from proprietary code or content, and that you license it under Apache-2.0 OR MIT. Do not contribute code you have seen under an NDA or a licence that restricts using what you learned.

If you are unsure whether something is safe to add, ask first in an issue.

## Names

Node and feature names are generic descriptions of what they do ("Blur", "Levels", "Voronoi", "Flood Fill"). If a name turns out to be a brand, it gets renamed. Reports are welcome and handled quickly.
