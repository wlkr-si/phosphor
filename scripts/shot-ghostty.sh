#!/bin/bash
# Like shot.sh, but in Ghostty, a terminal that draws block characters itself, so the case
# glides by eighths of a cell. Takes stills as fast as it can for WAIT_SECONDS into OUT_DIR/.
# Only use it when Ghostty is not already open: it quits Ghostty when done.
# usage: scripts/shot-ghostty.sh OUT_DIR/ [WAIT_SECONDS] [-- extra screensaver args]
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=$1; WAIT=${2:-8}
shift $(( $# < 2 ? $# : 2 )); [ "${1:-}" = "--" ] && shift
if pgrep -x ghostty >/dev/null; then echo "Ghostty is already open; not touching it" >&2; exit 1; fi
mkdir -p "$OUT"
BEFORE=$(osascript -e 'POSIX path of (path to frontmost application as text)' 2>/dev/null || true)
trap 'osascript -e "tell application \"Ghostty\" to quit" 2>/dev/null; [ -n "$BEFORE" ] && open -a "$BEFORE"' EXIT
open -na /Applications/Ghostty.app --args --window-width=200 --window-height=56 --font-size=11 \
  --command="/bin/bash -lc 'cd $ROOT && bun apps/screensaver/main.ts --seconds $((WAIT + 6)) $*'"
WINID=""
for _ in $(seq 1 40); do
  WINID=$(osascript -l JavaScript -e 'ObjC.import("CoreGraphics"); const a = ObjC.castRefToObject($.CGWindowListCopyWindowInfo(1, 0)); let id = ""; for (let i = 0; i < a.count; i++) { const w = a.objectAtIndex(i); if (ObjC.unwrap(w.objectForKey("kCGWindowOwnerName")) === "Ghostty" && ObjC.unwrap(w.objectForKey("kCGWindowLayer")) === 0) id = String(ObjC.unwrap(w.objectForKey("kCGWindowNumber"))); } id' 2>/dev/null || true)
  [ -n "$WINID" ] && break; sleep 0.25
done
[ -z "$WINID" ] && { echo "no Ghostty window" >&2; exit 1; }
osascript -e 'tell application "Ghostty" to activate' 2>/dev/null || true
sleep 2.5
end=$(( $(date +%s) + WAIT ))
while [ "$(date +%s)" -lt "$end" ]; do
  screencapture -x -o -t jpg -l "$WINID" "$OUT$(python3 -c 'import time; print(int(time.time()*1000))').jpg"
done
echo "$OUT"
