#!/usr/bin/env bash
# Run the checks CI runs. Without stage names it runs all of them, in order.
#
#   scripts/test.sh [--no-cef] [stage...]
#
#   fmt      cargo fmt --check
#   clippy   clippy on all targets, warnings are errors
#   test     cargo test (finds CEF's libraries in target/debug on Linux and Windows)
#   ui       sidebar UI: typecheck and build
#   deny     cargo deny check (skipped when cargo-deny is not installed)
#   notices  THIRD_PARTY_NOTICES.md is up to date
#
# --no-cef leaves out the crates that need CEF (magnetite, magnetite-engine):
# a quick run without cmake, Ninja and the CEF download.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

all=" fmt clippy test ui deny notices"
no_cef=0 stages=""
while (($#)); do
  case $1 in
    --no-cef) no_cef=1 ;;
    -h | --help) sed -n '/^set -euo/q; 2,$p' "$0"; exit 0 ;;
    fmt | clippy | test | ui | deny | notices) stages="$stages $1" ;;
    *) die "unknown stage or option: $1 (see --help)" ;;
  esac
  shift
done
stages="${stages:-$all}"

scope=(--workspace)
if ((no_cef)); then scope+=(--exclude magnetite --exclude magnetite-engine); fi

pnpm_install() { (cd "$ROOT/crates/ui/web" && pnpm install --frozen-lockfile); }

run_tests() {
  local libs
  libs="$(target_dir)/debug"
  case $(host_os) in
    linux) LD_LIBRARY_PATH="$libs${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" cargo test --locked "${scope[@]}" ;;
    windows) PATH="$libs:$PATH" cargo test --locked "${scope[@]}" ;;
    macos) cargo test --locked "${scope[@]}" ;;
  esac
}

cd "$ROOT"
for stage in $stages; do
  log "$stage"
  case $stage in
    fmt) cargo fmt --all --check ;;
    clippy) cargo clippy --locked "${scope[@]}" --all-targets -- -D warnings ;;
    test) run_tests ;;
    ui) pnpm_install && (cd crates/ui/web && pnpm build) ;;
    deny)
      if cargo deny --version >/dev/null 2>&1; then
        cargo deny check
      else
        log "skipped: cargo-deny is not installed (cargo install cargo-deny --locked)"
      fi
      ;;
    notices)
      # The generator reads the sidebar's npm packages, so they must be installed.
      [[ -d crates/ui/web/node_modules ]] || pnpm_install
      python3 tools/third-party-notices.py --check
      ;;
  esac
done
log "passed:$stages"
