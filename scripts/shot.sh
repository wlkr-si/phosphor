#!/bin/bash
# Open the screensaver in Apple's Terminal, photograph only that window, then close it.
# usage: scripts/shot.sh OUT.png [WAIT_SECONDS] [WIDTHxHEIGHT] [-- extra screensaver args]
# Only one test window at a time: a second run waits for the first.
# Set PHOSPHOR_BIN to photograph a built program instead of the source.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=$1; WAIT=${2:-4}; SIZE=${3:-1600x1000}
shift $(( $# < 3 ? $# : 3 )); [ "${1:-}" = "--" ] && shift
W=${SIZE%x*}; H=${SIZE#*x}
LOCK="${TMPDIR:-/tmp}/phosphor-shot.lock"
until mkdir "$LOCK" 2>/dev/null; do
  [ -n "$(find "$LOCK" -maxdepth 0 -mmin +10 2>/dev/null)" ] && rmdir "$LOCK" 2>/dev/null; sleep 1
done
trap 'rmdir "$LOCK" 2>/dev/null' EXIT
mkdir -p "$(dirname "$OUT")"
LIFE=$((WAIT + 3))
RUN=${PHOSPHOR_BIN:-bun apps/screensaver/main.ts}
CMD="cd '$ROOT' && $RUN --seconds $LIFE $* ; exit"
ID=$(osascript - "$CMD" "$W" "$H" <<'OSA'
on run argv
  tell application "Terminal"
    set t to do script (item 1 of argv)
    delay 0.3
    set w to first window whose selected tab is t
    set bounds of w to {40, 60, 40 + (item 2 of argv as integer), 60 + (item 3 of argv as integer)}
    return id of w
  end tell
end run
OSA
)
sleep "$WAIT"
screencapture -x -o -l "$ID" "$OUT"
sleep 4
osascript -e "tell application \"Terminal\" to close (every window whose id is $ID) saving no" 2>/dev/null || true
echo "$OUT"
