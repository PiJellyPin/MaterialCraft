# Attribution

Third-party material and design credits. MaterialCraft's own code and original assets are
MIT OR Apache-2.0 (see [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE) and
[NOTICE](NOTICE)). When adding third-party material, add a row here in the same change
and keep its license file beside the copied material. Fonts below come from
`epaint_default_fonts` in GUI builds.

| Path | Title | Author | Source | License |
|---|---|---|---|---|
| `src/webp_tables.rs` | VP8/VP8L probability, quantizer and distance tables | Google Inc. | [libwebp](https://chromium.googlesource.com/webm/libwebp/) | BSD-3-Clause, [full notice](src/LICENSE-libwebp) |
| `src/dds_tables.rs` | BC6H/BC7 partition, fix-up and bit-layout tables | Microsoft Corporation | [DirectXTex](https://github.com/microsoft/DirectXTex) | MIT, [full notice](src/LICENSE-DirectXTex) |
| `epaint_default_fonts/fonts/Hack-Regular.ttf` | Hack monospace font | Source Foundry Authors; Bitstream; DejaVu contributors | [Hack](https://github.com/source-foundry/Hack) via [egui 0.36.2](https://github.com/emilk/egui/tree/0.36.2/crates/epaint_default_fonts/fonts) | MIT and Bitstream Vera License; DejaVu contributions are public domain. [Full notices](https://github.com/emilk/egui/blob/0.36.2/crates/epaint_default_fonts/fonts/Hack-Regular.txt) |
| `epaint_default_fonts/fonts/Ubuntu-Light.ttf` | Ubuntu UI font | Canonical Ltd.; Dalton Maag | [Ubuntu font](https://design.ubuntu.com/font) via egui | Ubuntu Font Licence 1.0, [full text](https://github.com/emilk/egui/blob/0.36.2/crates/epaint_default_fonts/fonts/UFL.txt) |
| `epaint_default_fonts/fonts/NotoEmoji-Regular.ttf` | Noto emoji font | Google and Noto contributors | [Noto Emoji](https://fonts.google.com/noto/specimen/Noto+Emoji) via egui | SIL Open Font License 1.1, [full text](https://github.com/emilk/egui/blob/0.36.2/crates/epaint_default_fonts/fonts/OFL.txt) |
| `epaint_default_fonts/fonts/emoji-icon-font.ttf` | Emoji icon font | John Slegers | [emoji-icon-font](https://github.com/jslegers/emoji-icon-font) via egui | MIT, [full notice](https://github.com/emilk/egui/blob/0.36.2/crates/epaint_default_fonts/fonts/emoji-icon-font-mit-license.txt) |
| `Cargo.toml` (`craft-atomic`, `craft-settings`, `craft-cms`, `craft-ui`, `craft-quit`) | Shared Craft libraries | ArtCraft Team and craft-libs contributors | [craft-libs](https://github.com/storytold/craft-libs/tree/04e83f095ab080c5672243514f9a44ca6364b0e2) | MIT OR Apache-2.0; notices in the upstream repository |
| `src/session.rs`, `src/agent.rs`, `src/agent_cli.rs`, `src/gui_control.rs` | Command registry, CLI, MCP and editor control architecture | FilmCraft contributors (design inspiration); MaterialCraft contributors (implementation) | [FilmCraft](https://github.com/storytold/filmcraft) | Original implementation, MIT OR Apache-2.0; no FilmCraft code copied |
