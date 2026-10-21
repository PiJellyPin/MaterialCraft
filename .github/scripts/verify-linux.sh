#!/usr/bin/env bash
# Run under xvfb-run; share its display with the clean Ubuntu installation.
set -euo pipefail
name="MaterialCraft_${VERSION}_${LABEL}"
container="materialcraft-verify-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}"
trap 'docker rm -f "$container" >/dev/null 2>&1 || true' EXIT
docker run -d --name "$container" --ipc host \
    -v "$PWD/dist:/dist:ro" -v "$PWD/.github/scripts:/scripts:ro" \
    -v /tmp/.X11-unix:/tmp/.X11-unix \
    -e VERSION -e LABEL -e DISPLAY -e XDG_RUNTIME_DIR=/tmp \
    ubuntu:22.04 sleep infinity
docker exec "$container" bash /scripts/verify-ubuntu.sh
bash .github/scripts/launch-gui.sh docker exec "$container" materialcraft-gui
docker rm -f "$container"
trap - EXIT

# Exercise the portable archive independently of the build directory.
mkdir -p "$RUNNER_TEMP/verify-tar"
tar -xzf "dist/$name.tar.gz" -C "$RUNNER_TEMP/verify-tar"
test -x "$RUNNER_TEMP/verify-tar/$name/materialcraft-gui"
bash .github/scripts/smoke-cli.sh "$RUNNER_TEMP/verify-tar/$name/materialcraft"

# Check both the FUSE launcher and its extraction fallback.
appimage="$PWD/dist/$name.AppImage"
chmod +x "$appimage"
bash .github/scripts/launch-gui.sh "$appimage"
mkdir -p "$RUNNER_TEMP/verify-appimage"
(cd "$RUNNER_TEMP/verify-appimage" && "$appimage" --appimage-extract >/dev/null)
extracted="$RUNNER_TEMP/verify-appimage/squashfs-root"
bash .github/scripts/smoke-cli.sh "$extracted/usr/bin/materialcraft"
bash .github/scripts/launch-gui.sh "$extracted/AppRun"
