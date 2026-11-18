#!/usr/bin/env bash
# Build from the release checkout. VERSION and RUNNER_TEMP come from the workflow.
set -euo pipefail
app_id=io.github.PiJellyPin.MaterialCraft
runtime_repo=https://dl.flathub.org/repo/flathub.flatpakrepo
work=$(mktemp -d "$RUNNER_TEMP/materialcraft-flatpak.XXXXXX")
trap 'rm -rf "$work"' EXIT

# Stage only tracked files so local Cargo configuration and build outputs cannot leak in.
mkdir -p "$work/source" dist
git -c safe.directory="$PWD" archive HEAD | tar -x -C "$work/source"
manifest_dir="$work/source/packaging/flatpak"

generator="$work/flatpak-cargo-generator.py"
commit=2a5e1a558a56e06dfaa6a16f35247cd210f95b35
curl -sSfL -o "$generator" \
  "https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$commit/cargo/flatpak-cargo-generator.py"
sha256sum --check --strict <<EOF_SHA
28f9e6bdb640f66ede8918ba853d228bd792273bcf316e3097039e6faefb18c7  $generator
EOF_SHA

# The generator stages crates.io packages and the locked craft-libs Git revision.
python3 "$generator" "$work/source/Cargo.lock" -o "$manifest_dir/cargo-sources.json"

flatpak remote-add --user --if-not-exists flathub "$runtime_repo"
flatpak-builder --user --arch=x86_64 --install-deps-from=flathub \
  --state-dir="$work/state" --repo="$work/repo" --default-branch=stable \
  "$work/build" "$manifest_dir/$app_id.yml"
flatpak build-bundle --arch=x86_64 --runtime-repo="$runtime_repo" \
  "$work/repo" "dist/MaterialCraft_${VERSION}_x86_64-linux.flatpak" "$app_id" stable
