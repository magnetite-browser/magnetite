#!/usr/bin/env bash
# Development build into target/bundle/Magnetite.app (no zip), for running
# from the checkout. Same bundle as scripts/build.sh; see its --help.
# Usage: scripts/bundle.sh [--release]
set -euo pipefail
exec "$(dirname "$0")/build.sh" --no-archive --out "$(dirname "$0")/../target/bundle" "$@"
