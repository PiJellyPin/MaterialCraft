# Developer utilities

These standalone Rust programs generate assets and inspect rendering. Run them from the
repository root. Editable material packages and their previews are in
[`assets/templates/`](../assets/templates/).

| Utility | Purpose | Arguments |
| --- | --- | --- |
| `templates` | Regenerate the shipped material designs as SBS packages and PNG previews. | `[material-name]`; `SZ` sets package resolution (default 1024). Writes to `/tmp/tpl/`; review results before copying them into assets. |
| `gpu_shot` | Capture the GPU viewer and optionally compare it with the CPU renderer. | See its source header; requires `--features gui`, a display, and OpenGL. `graph=package.sbs` loads an SBS material. |

```sh
SZ=256 cargo run --release --example templates -- brick_wall
cargo run --release --features gui --example gpu_shot -- out.png 2 3 640 480 graph=assets/templates/brick_wall.sbs size=128
```
