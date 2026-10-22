import json
import os
import shutil
import subprocess
from pathlib import Path

stage = Path(os.environ["RUNNER_TEMP"]) / "materialcraft-notices"
stage.mkdir(parents=True, exist_ok=True)
for name in ("LICENSE-MIT", "LICENSE-APACHE", "NOTICE", "ATTRIBUTION.md",
             "src/LICENSE-libwebp", "src/LICENSE-DirectXTex", "src/LICENSE-exr"):
    destination = stage / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(name, destination)

# Dependency-provided fonts: ship the notices from the version Cargo resolved.
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--locked", "--format-version=1", "--features", "gui"],
    text=True,
))
packages = [p for p in metadata["packages"] if p["name"] == "epaint_default_fonts"]
if len(packages) != 1:
    raise SystemExit("Expected one epaint_default_fonts version; check bundled font notices")
fonts = Path(packages[0]["manifest_path"]).parent / "fonts"
for source, destination in (
    ("Hack-Regular.txt", "LICENSE-Hack"),
    ("UFL.txt", "LICENSE-Ubuntu"),
    ("OFL.txt", "LICENSE-NotoEmoji"),
    ("emoji-icon-font-mit-license.txt", "LICENSE-emoji-icon-font"),
):
    shutil.copyfile(fonts / source, stage / destination)
