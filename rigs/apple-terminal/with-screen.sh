#!/bin/bash
# Run a command while holding the screen: only one test window may be open in Apple's
# Terminal at a time, or the measurements disturb each other.
# usage: with-screen.sh <command> [args...]
LOCK="${TMPDIR:-/tmp}/phosphor-screen.lock"
waited=0
until mkdir "$LOCK" 2>/dev/null; do
  # a lock left by a run that died more than 15 minutes ago is stale
  if [ -n "$(find "$LOCK" -maxdepth 0 -mmin +15 2>/dev/null)" ]; then rmdir "$LOCK" 2>/dev/null; continue; fi
  sleep 2; waited=$((waited + 2))
  [ $((waited % 60)) -eq 0 ] && echo "with-screen: still waiting for the screen (${waited}s)" >&2
done
trap 'rmdir "$LOCK" 2>/dev/null' EXIT
"$@"
