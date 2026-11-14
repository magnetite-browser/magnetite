#!/usr/bin/env bash
# Run scripts/build.sh or scripts/test.sh for Linux in a container (Podman or
# Docker), for example from macOS. Packages land in dist/ as usual.
#
#   scripts/linux-container.sh build [--release] [--no-archive]
#   scripts/linux-container.sh test [--no-cef] [stage...]
#
# The checkout is mounted read-only and copied into the container, so nothing
# but dist/ is written to it. Cargo's target folder, the crate registry, the
# pnpm store and CEF live in volumes named magnetite-linux-* and survive runs.
# Builds are for the container's CPU (arm64 on Apple silicon).
# CONTAINER_ENGINE=docker picks Docker when both are installed.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

cmd="${1:-}"
case $cmd in
  build | test) shift ;;
  *) die "usage: scripts/linux-container.sh build|test [options]" ;;
esac

engine="${CONTAINER_ENGINE:-$(command -v podman || command -v docker || true)}"
[[ -n $engine ]] || die "install Podman or Docker"
image=magnetite-linux-builder
toolchain="$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$ROOT/rust-toolchain.toml")"
log "container image $image (Rust $toolchain)"
"$engine" build -q -t "$image" --build-arg "RUST_TOOLCHAIN=$toolchain" "$ROOT/packaging/linux" >/dev/null

mkdir -p "$ROOT/dist"
vol=magnetite-linux
# label=disable: SELinux hosts (Fedora) would deny the container the bind
# mounts; relabelling them (:z) would change the checkout's labels instead.
"$engine" run --rm -i --security-opt label=disable \
  -v "$ROOT:/src:ro" \
  -v "$ROOT/dist:/out" \
  -v "$vol-target:/work/target" \
  -v "$vol-cargo:/usr/local/cargo/registry" \
  -v "$vol-pnpm:/root/.local/share/pnpm" \
  -v "$vol-cef:/work/cef" \
  -e CARGO_TARGET_DIR=/work/target \
  -e CEF_PATH=/work/cef \
  "$image" bash -s -- "$cmd" "$@" <<'EOF' | sed "s|^/out/|$ROOT/dist/|"
set -euo pipefail
mkdir -p /work/src
tar -C /src --exclude=./target --exclude=./dist --exclude=./.git --exclude=node_modules -cf - . |
  tar -C /work/src -xf -
cd /work/src
cmd=$1
shift
if [[ $cmd == build ]]; then
  exec scripts/build.sh --out /out "$@"
fi
exec scripts/test.sh "$@"
EOF
