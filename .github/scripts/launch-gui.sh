#!/usr/bin/env bash
# Usage: launch-gui.sh <command> [args...]
#
# Starts the editor on the X display named by $DISPLAY and passes only if it
# opens a window titled MaterialCraft within 60 s and is still running 5 s
# after that. A crash on startup, a missing shared library, or a GL context the
# driver refuses all end the process early and fail with its output shown.
#
# The command may be a wrapper (an AppImage runtime, `docker exec ...`); only
# the window title is checked, so it does not matter which process owns it.
set -uo pipefail

log=$(mktemp)
setsid "$@" >"$log" 2>&1 &
pid=$!

cleanup() {
  kill -- -"$pid" 2>/dev/null
  # Let the window close so the next launch on this display cannot mistake it
  # for its own.
  for _ in 1 2 3 4 5; do
    xdotool search --name MaterialCraft >/dev/null 2>&1 || return 0
    sleep 1
  done
  kill -9 -- -"$pid" 2>/dev/null
}
trap cleanup EXIT

fail() {
  echo "::error::$*"
  echo "--- editor output ---"
  cat "$log"
  exit 1
}

if xdotool search --name MaterialCraft >/dev/null 2>&1; then
  fail "a MaterialCraft window was already open before launch; cannot attribute it"
fi

for i in $(seq 60); do
  kill -0 "$pid" 2>/dev/null || { wait "$pid"; fail "exited with status $? before opening a window: $*"; }
  if xdotool search --name MaterialCraft >/dev/null 2>&1; then
    sleep 5
    kill -0 "$pid" 2>/dev/null || { wait "$pid"; fail "opened a window, then exited with status $?: $*"; }
    echo "window open after ${i}s and still running 5s later: $*"
    cat "$log"
    exit 0
  fi
  sleep 1
done
fail "still running after 60s but opened no window: $*"
