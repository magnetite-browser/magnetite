#!/usr/bin/env bash
# Runs inside sway in the container of scripts/wayland-test.sh, as an
# ordinary user: starts Magnetite, then the smoke test or the steps file,
# and leaves the results in /out (its own output: test.log). sway gives it
# WAYLAND_DISPLAY, DISPLAY (XWayland) and SWAYSOCK.
set -uo pipefail
exec >/out/test.log 2>&1

# Pointer and keys for steps files. `at X Y` and `click X Y [right]` take
# output pixels (the screen is 1400x900). Keys: `wtype "text"`, `wtype -k
# Escape`. wtype's keyboard exists only while wtype runs: the first key of
# each call gets lost, and a window is focused only while a keyboard is
# there.
at() {
  echo "move $1 $2" >&3
  sleep 0.4
}
click() {
  local button=272
  if [[ ${3:-left} == right ]]; then button=273; fi
  at "$1" "$2"
  echo "down $button" >&3
  sleep 0.1
  echo "up $button" >&3
  sleep 1
}

MAGNETITE_DEVTOOLS_PORT=9333 /app/magnetite --no-import --mock-keychain \
  --data-dir /tmp/data "$@" >/out/app.log 2>&1 &
app=$!
for _ in $(seq 150); do
  curl -s -o /dev/null http://127.0.0.1:9333/json && break
  kill -0 "$app" 2>/dev/null || break
  sleep 0.2
done

if [[ -n ${STEPS:-} ]]; then
  mkfifo /tmp/pointer
  vptr 1400 900 </tmp/pointer &
  exec 3>/tmp/pointer
  # shellcheck source=/dev/null
  (source /steps.sh)
else
  node /tools/smoke.mjs
fi
status=$?

grim /out/shot.png
swaymsg -t get_tree | grep -o '"shell": "[a-z_]*"' | sort | uniq -c >/out/windows.txt
if ! kill -0 "$app" 2>/dev/null; then
  wait "$app"
  echo "magnetite ended with code $? (see app.log)" >&2
  status=1
fi
kill "$app" 2>/dev/null
echo "$status" >/tmp/status
swaymsg exit 2>/dev/null
