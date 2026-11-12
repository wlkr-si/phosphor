#!/bin/bash
# usage: run.sh <name> <font size> <cols> <rows> [extra rig args]
A=$(cd "$(dirname "$0")" && pwd)
name=$1; size=$2; cols=$3; rows=$4; shift 4
D=$A/out-$name; rm -rf $D; mkdir -p $D
osascript -l JavaScript $A/mkprofile.js "$A/PhosphorLab.terminal" "Phosphor Lab" Menlo-Regular "$size" "$cols" "$rows" "/bin/sh -c '$A/fb --dir $D $* 2> $D/err.txt'; exit" >/dev/null
open -a Terminal "$A/PhosphorLab.terminal"
for s in 1 2 3; do
  for i in $(seq 1 1500); do [ -f $D/stage-$s ] || [ -f $D/report.txt ] && break; sleep 0.1; done
  [ -f $D/stage-$s ] && { sleep 0.6; W=$(osascript -l JavaScript $A/winid.js); echo "window $W" >> $D/win.txt; screencapture -x -o -l ${W%% *} $D/shot-$s.png; }
done
for i in $(seq 1 1500); do [ -f $D/report.txt ] && break; sleep 0.1; done
cat $D/report.txt 2>/dev/null || echo "no report"; ls $D
