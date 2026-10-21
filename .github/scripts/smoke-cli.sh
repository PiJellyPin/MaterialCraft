#!/usr/bin/env bash
# Accept a binary, optionally preceded by a launcher such as arch -x86_64.
set -euo pipefail
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
"$@" example "$work"
"$@" render "$work/brick_wall.sbs" "$work/out" --size 64
compgen -G "$work/out/*.png" >/dev/null
echo "CLI smoke test passed: $*"
