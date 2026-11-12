#!/bin/bash
# usage: runlab.sh <name> <font size> <cols> <rows> <lab args...>   (runs the bouncing computer in Apple Terminal)
A=$(cd "$(dirname "$0")" && pwd)
name=$1; size=$2; cols=$3; rows=$4; shift 4
D=$A/lab-$name; rm -rf $D; mkdir -p $D
osascript -l JavaScript $A/mkprofile.js "$A/PhosphorLab.terminal" "Phosphor Lab" Menlo-Regular "$size" "$cols" "$rows" "/bin/sh -c '$A/../../lab/lab $* --seconds 7 --report $D/report.txt 2> $D/err.txt'; exit" >/dev/null
open -a Terminal "$A/PhosphorLab.terminal"
sleep 4.5; W=$(osascript -l JavaScript $A/winid.js); screencapture -x -o -l ${W%% *} $D/shot.png
for i in $(seq 1 300); do [ -f $D/report.txt ] && break; sleep 0.1; done
echo "== $name: $(sed -n '1p' $D/report.txt | cut -c1-24)"; sed -n '2,5p' $D/report.txt
