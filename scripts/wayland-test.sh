#!/usr/bin/env bash
# Run a staged Linux build in a headless Wayland session (sway with
# XWayland) in a container, with Podman or Docker. Without a steps file it
# runs the smoke test, tools/smoke.mjs.
#
#   scripts/wayland-test.sh <stage folder> [--steps FILE] [--out DIR] [-- <magnetite options>]
#
# <stage folder> is dist/magnetite-<version>/ of a development build made in
# the container (scripts/linux-container.sh build --no-archive): the smoke
# test needs the DevTools port, which release builds ignore.
#
#   --steps FILE   a bash script to run in place of the smoke test once the
#                  app is up, with pointer and keys (`at`, `click`, wtype:
#                  tools/wayland/session.sh) and grim for screenshots to /out
#   --out DIR      where the results go (default dist/wayland): test.log
#                  (also printed), app.log, sway.log, shot.png (the screen
#                  at the end) and windows.txt (how many windows are
#                  xdg_shell, how many xwayland)
#
# Magnetite runs through XWayland unless told otherwise; `--
# --ozone-platform=wayland` runs it as a Wayland client (ADR-0012).
# The exit code is the smoke test's or the steps', and 1 if Magnetite died.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

stage="" steps="" out="$ROOT/dist/wayland"
while (($#)); do
  case $1 in
    --steps) steps="${2:?--steps needs a file}"; shift ;;
    --out) out="${2:?--out needs a folder}"; shift ;;
    --) shift; break ;;
    -h | --help) sed -n '/^set -euo/q; 2,$p' "$0"; exit 0 ;;
    -*) die "unknown option: $1 (see --help)" ;;
    *) stage="$1" ;;
  esac
  shift
done
[[ -x $stage/magnetite ]] || die "usage: scripts/wayland-test.sh <stage folder> [--steps FILE] [--out DIR] [-- <magnetite options>]"
[[ -z $steps || -f $steps ]] || die "no steps file at $steps"
stage="$(cd "$stage" && pwd)"
mkdir -p "$out"
out="$(cd "$out" && pwd)"

engine="${CONTAINER_ENGINE:-$(command -v podman || command -v docker || true)}"
[[ -n $engine ]] || die "install Podman or Docker"
toolchain="$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$ROOT/rust-toolchain.toml")"
log "container image magnetite-wayland"
# -f: Docker looks for a Dockerfile by itself, Podman for either name.
"$engine" build -q -t magnetite-linux-builder --build-arg "RUST_TOOLCHAIN=$toolchain" \
  -f "$ROOT/packaging/linux/Containerfile" "$ROOT/packaging/linux" >/dev/null
"$engine" build -q -t magnetite-wayland -f "$ROOT/tools/wayland/Containerfile" "$ROOT/tools/wayland" >/dev/null

mounts=(-v "$stage:/stage:ro" -v "$ROOT/tools:/tools:ro" -v "$out:/out")
if [[ -n $steps ]]; then
  mounts+=(-v "$(cd "$(dirname "$steps")" && pwd)/$(basename "$steps"):/steps.sh:ro" -e STEPS=1)
fi
# label=disable: SELinux hosts (Fedora) would deny the container the bind
# mounts. The shared memory is Chromium's.
"$engine" run --rm -i --security-opt label=disable --shm-size=2g "${mounts[@]}" \
  magnetite-wayland bash -s -- "$@" <<'EOF_IN'
set -euo pipefail
# A copy the test user may run: Chromium's sandbox as packaged, setuid root.
cp -a /stage /app
chown root:root /app/chrome-sandbox
chmod 4755 /app/chrome-sandbox
chmod 777 /out
args=""
for arg in "$@"; do args+=" $(printf '%q' "$arg")"; done
cat >/tmp/sway.conf <<CONF
output HEADLESS-1 resolution 1400x900
exec bash /tools/wayland/session.sh$args
CONF
exec runuser -u tester -- env XDG_RUNTIME_DIR=/tmp/run WLR_BACKENDS=headless WLR_RENDERER=pixman \
  WLR_LIBINPUT_NO_DEVICES=1 bash -c '
    mkdir -m 700 /tmp/run
    timeout 300 sway -c /tmp/sway.conf >/out/sway.log 2>&1
    cat /out/test.log
    exit "$(cat /tmp/status 2>/dev/null || echo 1)"'
EOF_IN
