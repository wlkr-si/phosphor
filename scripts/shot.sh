#!/bin/bash
# Open the screensaver in Apple's Terminal, photograph only that window, then close it.
# usage: scripts/shot.sh OUT.png [WAIT_SECONDS] [WIDTHxHEIGHT] [-- extra screensaver args]
# An OUT ending in / takes stills as fast as it can for WAIT_SECONDS instead, after a two-second
# start, named by the millisecond they were taken: a cheap film for checking motion. A film
# brings the test window to the front, because Apple's Terminal stops painting a hidden window,
# and gives the front back to whatever had it.
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
mkdir -p "$(dirname "$OUT")"; [[ "$OUT" == */ ]] && mkdir -p "$OUT"
LIFE=$((WAIT + 5))
RUN=${PHOSPHOR_BIN:-bun apps/dashboard/main.ts}
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
if [[ "$OUT" == */ ]]; then
  BEFORE=$(osascript -e 'POSIX path of (path to frontmost application as text)' 2>/dev/null || true)
  osascript -e "tell application \"Terminal\" to set index of (first window whose id is $ID) to 1" -e 'tell application "Terminal" to activate'
  trap 'rmdir "$LOCK" 2>/dev/null; [ -n "$BEFORE" ] && open -a "$BEFORE"' EXIT
  sleep 2
  end=$(( $(date +%s) + WAIT ))
  while [ "$(date +%s)" -lt "$end" ]; do
    screencapture -x -o -t jpg -l "$ID" "$OUT$(python3 -c 'import time; print(int(time.time()*1000))').jpg"
  done
else
  sleep "$WAIT"
  screencapture -x -o -l "$ID" "$OUT"
fi
sleep 4
osascript -e "tell application \"Terminal\" to close (every window whose id is $ID) saving no" 2>/dev/null || true
echo "$OUT"
