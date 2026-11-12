#!/bin/bash
# How many bytes a second the screensaver sends a terminal, measured in a hidden terminal session.
# usage: scripts/rate.sh OUT_DIR [screensaver args]
set -u
DIR=$1; shift
mkdir -p "$DIR"
LOG="$DIR/pty-$$.out"
script -q "$LOG" bash -c "stty cols 263 rows 67; bun apps/screensaver/main.ts --seconds 8 $*" </dev/null >/dev/null &
sleep 3; a=$(wc -c < "$LOG"); sleep 4; b=$(wc -c < "$LOG")
wait
echo "$(( (b - a) / 4 )) bytes a second"
