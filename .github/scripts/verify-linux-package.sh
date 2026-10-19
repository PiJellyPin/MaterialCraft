#!/usr/bin/env bash
# Runs INSIDE a bare distro container (see the verify job in release.yml).
# Installs one Linux artifact the way a user would and runs the CLI from the
# installed location. Launching the editor is done afterwards, from the runner,
# by launch-gui.sh -- so nothing installed for the test harness (Xvfb, xdotool)
# can leak into this container and mask a missing package dependency.
#
# Env: CHECK (deb | rpm | tarball), VERSION, ARTIFACT. /dist holds the files.
set -euo pipefail

base="/dist/MaterialCraft_${VERSION}_${ARTIFACT}"

case "$CHECK" in
  deb)
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq
    # Default apt behaviour, Recommends included, as a user's `apt install`
    # gets it. Depends alone must still pull in every library the editor
    # dlopen()s; the GL driver (Mesa) arrives via Recommends.
    apt-get install -y -q "$base.deb"
    dpkg -s materialcraft | sed -n '/^Depends:/p'
    cli=/usr/bin/materialcraft
    ;;
  rpm)
    # Fedora's container image turns weak dependencies off; a desktop install
    # has them on, which is how the Mesa driver arrives. Match the desktop.
    dnf install -y -q --setopt=install_weak_deps=True "$base.rpm"
    rpm -q --requires materialcraft
    cli=/usr/bin/materialcraft
    ;;
  tarball)
    # The tarball declares no dependencies, so only the CLI is checked: it
    # needs nothing beyond glibc, which is what this proves on an older distro.
    mkdir -p /opt/mc
    tar -xzf "$base.tar.gz" -C /opt/mc
    cli="/opt/mc/MaterialCraft_${VERSION}_${ARTIFACT}/materialcraft"
    test -x "${cli}-gui"
    ;;
  *)
    echo "unknown CHECK=$CHECK"; exit 2 ;;
esac

work=$(mktemp -d)
"$cli" example "$work"
"$cli" render "$work/brick_wall.mcg" "$work/out" --size 64
ls "$work/out"/*.png >/dev/null
echo "CLI OK from $cli"
