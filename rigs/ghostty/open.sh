#!/bin/bash
# Opens a new Ghostty, apart from any Ghostty already open, with one test window that runs a
# command and closes when it ends; that Ghostty then quits. Your own Ghostty settings are not
# read, so every film sees the same window: Ghostty's default font and colours, no saved
# windows restored, no update check.
# usage: open.sh <font size> <cols> <rows> <command> [args...]
# Run it through rigs/apple-terminal/with-screen.sh, as witness-film does.
set -e
[ $# -ge 4 ] || { echo "usage: open.sh <font size> <cols> <rows> <command> [args...]" >&2; exit 2; }
font=$1 cols=$2 rows=$3
shift 3
exec open -na /Applications/Ghostty.app --args \
  --config-default-files=false \
  --window-save-state=never \
  --quit-after-last-window-closed=true \
  --confirm-close-surface=false \
  --auto-update=off \
  --font-size="$font" \
  --window-width="$cols" \
  --window-height="$rows" \
  -e "$@"
