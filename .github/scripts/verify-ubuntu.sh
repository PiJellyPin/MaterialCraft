#!/usr/bin/env bash
# Run in a clean Ubuntu container so runner libraries cannot hide missing dependencies.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -q "/dist/MaterialCraft_${VERSION}_${LABEL}.deb"
bash /scripts/smoke-cli.sh /usr/bin/materialcraft
