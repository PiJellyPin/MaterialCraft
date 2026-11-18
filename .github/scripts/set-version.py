#!/usr/bin/env python3
"""Update only the root package version, without resolving dependencies, and add the release to the AppStream data."""
from pathlib import Path
import datetime
import re
import sys
import tomllib
import xml.etree.ElementTree as ET

version = sys.argv[1]
if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version):
    raise SystemExit("Expected a version such as 0.4.0.")

manifest = Path("Cargo.toml")
lockfile = Path("Cargo.lock")
manifest_text = manifest.read_text()
lock_text = lockfile.read_text()
package = tomllib.loads(manifest_text)["package"]
root = [p for p in tomllib.loads(lock_text)["package"]
        if p["name"] == package["name"] and "source" not in p]
if len(root) != 1 or root[0]["version"] != package["version"]:
    raise SystemExit("Cargo.toml and the root Cargo.lock entry must agree before a release.")

def replace_version(text, section):
    pattern = rf'({section}\n(?:(?!\[).*\n)*?version = ")[^"]+(")'
    updated, count = re.subn(pattern, lambda m: m[1] + version + m[2], text)
    if count != 1:
        raise SystemExit("Expected exactly one root package version.")
    return updated

manifest_text = replace_version(manifest_text, r"\[package\]")
lock_text = replace_version(lock_text, rf'\[\[package\]\]\nname = "{re.escape(package["name"])}"')
# The Flatpak's AppStream data lists each release; the newest goes first.
metainfo = Path("packaging/flatpak/io.github.PiJellyPin.MaterialCraft.metainfo.xml")
metainfo_text = metainfo.read_text()
if f'<release version="{version}"' not in metainfo_text:
    today = datetime.date.today().isoformat()
    metainfo_text, count = re.subn(r"(<releases>\n)", rf'\1    <release version="{version}" date="{today}"/>\n', metainfo_text)
    if count != 1:
        raise SystemExit("Expected one <releases> list in the AppStream metainfo.")
# Validate the replacements before writing any file.
tomllib.loads(manifest_text)
tomllib.loads(lock_text)
ET.fromstring(metainfo_text)
manifest.write_text(manifest_text)
lockfile.write_text(lock_text)
metainfo.write_text(metainfo_text)
