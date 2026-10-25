<p align="center">
  <img src="assets/logo.png" alt="MaterialCraft" width="120">
</p>

<h1 align="center">MaterialCraft</h1>

<p align="center">
  <b>Procedural material creation; an open-source, clean-room reimplementation of Adobe Substance 3D Designer, rebuilt in pure Rust.</b><br>
  Build with nodes and preview your materials in 3D using familiar workflows, shaped by artist feedback.<br>
</p>

<p align="center">
  <img alt="100% Rust" src="https://img.shields.io/badge/100%25-Rust-b7410e?style=flat-square&amp;logo=rust">
  <img alt="macOS · Windows · Linux" src="https://img.shields.io/badge/macOS%20%C2%B7%20Windows%20%C2%B7%20Linux-native-2f7bf5?style=flat-square">
  <a href="#license-and-credits"><img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-3a3a3a?style=flat-square"></a>
  <a href="docs/ROADMAP.md"><img alt="Status: early alpha" src="https://img.shields.io/badge/status-early%20alpha-d69e2e?style=flat-square"></a>
</p>

<p align="center">
  <a href="#downloads"><img src="https://img.shields.io/badge/Download-Development%20build-7950D6?style=for-the-badge" alt="Download the development build"></a>
</p>

<p align="center">
  <a href="#features">Features</a> ·
  <a href="#downloads">Downloads</a> ·
  <a href="docs/ROADMAP.md"><b>Roadmap</b></a> ·
  <a href="#getting-started">Getting Started</a>
</p>

<p align="center">
  <a href="https://discord.gg/9KVCNatNvX"><img alt="Join the MaterialCraft community on Discord" src="https://img.shields.io/badge/Join%20us%20on%20Discord-5865F2?style=for-the-badge&amp;logo=discord&amp;logoColor=white" height="40"></a><br>
  Share your materials, ask questions, and tell us which workflows could be better.
</p>

<h3 align="center">Donate</h3>

<p align="center">
  <a href="https://www.paypal.com/donate/?hosted_button_id=DYD94FKUZ9H7G"><img src="https://img.shields.io/badge/Donate%20via%20PayPal-0070BA?style=for-the-badge&amp;logo=paypal&amp;logoColor=white" alt="Donate via PayPal"></a><br>
  Help support MaterialCraft's development.<br>
  <sub>All donations currently fund the Claude Max subscription used for development.</sub>
</p>

<!-- DRAFT: Replace this placeholder with a full editor screenshot. -->
<p align="center">
  <img width="2555" height="1387" alt="image" src="https://github.com/user-attachments/assets/aed97881-c091-4ef3-b2f4-065f99dd44fe" />
  <sub><b>Screenshot placeholder:</b> the full editor, showing a node graph and its material in 3D.</sub>
</p>

## Features

**306 node types:** 217 texture nodes, 84 function nodes, and 5 pattern-program nodes. [Explore the node reference →](docs/NODES.md).

<table>
  <tr>
    <td width="50%" valign="top">
      <h3>🧩 Familiar by design</h3>
      <img width="932" height="632" alt="image" src="https://github.com/user-attachments/assets/90076571-4278-4cdf-bf47-b01343185785" />
      <p>A node graph, Properties panel, 2D/3D previews, and familiar editing shortcuts help Adobe Substance 3D Designer users get started. Build reusable materials and keep experimenting.</p>
    </td>
    <td width="50%" valign="top">
      <h3>🎨 Start from examples</h3>
      <img src="https://github.com/user-attachments/assets/f41d3786-15ba-4205-9371-06089cf96587" alt="Wood-patterned material on a sphere in MaterialCraft's 3D View" width="100%"><br>
      <sub>A wood-patterned material in the 3D View.</sub>
      <p>Open a bundled material or template and make it your own. Explore how a graph works, then reuse what you need.</p>
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top">
      <h3>🔍 Preview in 3D</h3>
      <img src="https://github.com/user-attachments/assets/d92aed6e-ef2e-40e5-a1da-a1341e2d1a4e" alt="Purple riveted metal plates displayed on a sphere in MaterialCraft's 3D View" width="100%"><br>
      <sub>Riveted metal plates under the preview lighting.</sub>
      <p>Inspect textures in 2D and preview materials on built-in shapes or your own mesh. Explore lighting, surface detail, and individual material channels.</p>
    </td>
    <td width="50%" valign="top">
      <h3>📦 Bake and export</h3>
      <img width="341" height="422" alt="image" src="https://github.com/user-attachments/assets/0be79b69-db28-4553-8aa0-497436c6e58b" />
      <p>Bake maps from meshes and export your textures. Choose the outputs, resolution, and image format for your project.</p>
    </td>
  </tr>
</table>

**MaterialX export (new).** **File → Export MaterialX (Unreal Engine)…** (or `materialcraft export-mtlx`) writes a [MaterialX](https://materialx.org/Specification.html) 1.39 material (`.mtlx` plus its textures) for Unreal Engine 5.8: Standard Surface or OpenPBR, colour maps tagged `srgb_texture`, a normal map in the direction you choose, height as displacement. `materialcraft export-mtlx calibration <folder>` writes a test material for checking an import in the engine. The files are checked with the official MaterialX 1.39.4 library; an import into a real Unreal Engine 5.8 project is still to be confirmed. Our [integration roadmap](docs/ROADMAP.md#application-integrations) also targets Unity Engine, Godot Engine, Blender, SideFX Houdini, Autodesk Maya, and Autodesk 3ds Max.

**More materials made with MaterialCraft**

<table>
  <tr>
    <td width="33%" align="center">
      <img src="https://github.com/user-attachments/assets/41096864-94b1-4831-9c0f-fac0b45f7dcb" alt="Dark cracked surface with glowing lava-colored seams on a sphere" width="100%"><br>
      <sub>Lava-like cracks</sub>
    </td>
    <td width="33%" align="center">
      <img src="https://github.com/user-attachments/assets/06a9e1bf-66f4-4a50-bc0c-33257d334d0d" alt="Raised woven pattern displayed on a sphere" width="100%"><br>
      <sub>Woven patterns</sub>
    </td>
    <td width="33%" align="center">
      <img src="https://github.com/user-attachments/assets/9eb1d9fd-df27-4576-a908-bc81c07bf571" alt="Cracked red brick material displayed on a sphere" width="100%"><br>
      <sub>Weathered brick</sub>
    </td>
  </tr>
</table>

## Downloads

[Windows, macOS, and Linux builds](../../releases/tag/latest) are updated nightly when changes are available.

<details>
<summary>Choose a package and install it</summary>

| Platform | File | Notes |
|---|---|---|
| Windows 10/11 x64 | `MaterialCraft_<version>_x64-windows.zip` | Unzip and run `materialcraft-gui.exe`. Unsigned, so SmartScreen may ask once: More info > Run anyway. |
| macOS 11+ (Apple Silicon and Intel) | `MaterialCraft_<version>_universal-macOS.dmg` | Drag MaterialCraft to Applications. Not notarised: the first time, right-click the app > Open (or System Settings > Privacy & Security > Open Anyway). The `materialcraft` CLI is next to the app in the dmg. |
| Ubuntu 22.04+ / Debian 12+ (x86-64) | `MaterialCraft_<version>_x86_64-linux.deb` | `sudo apt install ./MaterialCraft_<version>_x86_64-linux.deb` |
| Fedora / other RPM-based Linux (x86-64) | `MaterialCraft_<version>_x86_64-linux.rpm` | On systems using DNF: `sudo dnf install ./MaterialCraft_<version>_x86_64-linux.rpm`. See Linux requirements below. |
| Linux x86-64 (portable editor) | `MaterialCraft_<version>_x86_64-linux.AppImage` | `chmod +x` and run. Needs FUSE (`fusermount`); without it, run with `--appimage-extract-and-run`. |
| Linux x86-64 (editor and CLI archive) | `MaterialCraft_<version>_x86_64-linux.tar.gz` | Extract and run. Requires the desktop X11/Wayland, xkbcommon and EGL/GL libraries. |

Linux builds require **glibc 2.35 or newer** (Ubuntu 22.04, Debian 12, and later).

`SHA256SUMS` lists every file's checksum, and each file carries a build provenance attestation:
`gh attestation verify <file> -R PiJellyPin/MaterialCraft`.

</details>

## Getting started

1. **Open an example.** Launch MaterialCraft and choose a material from **TEMPLATES**, or use **File → Templates and examples**.
2. **Make it yours.** Select a node, adjust its properties, and inspect the result in the 2D and 3D views.
3. **Save and export.** Save your project, then choose **File → Export textures** to select your outputs, resolution, and image format.

Use **Tab** to add a node and **Ctrl+S** to save (**Cmd+S** on macOS).

## License and credits

MaterialCraft is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
Copyright (c) 2026 PiJellyPin and MaterialCraft contributors.
[Notices](NOTICE) · [Credits](ATTRIBUTION.md).
