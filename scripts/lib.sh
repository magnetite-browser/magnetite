# Shared helpers for the scripts in this folder. Source it, do not run it:
#   source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
# shellcheck shell=bash

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# macOS bundle id of Magnetite.app (project domain magnetitebrowser.org); the
# helpers get "<id>.helper…". Never change it after a release: keychain
# access, macOS preferences and Homebrew's zap list depend on it.
# shellcheck disable=SC2034 # read by the scripts that source this file
BUNDLE_ID="org.magnetitebrowser.magnetite"

log() { printf '==> %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

# A key from [workspace.package] in the root Cargo.toml, e.g. `version`.
workspace_key() {
  awk -v key="$1" '
    /^\[workspace\.package\]/ { inside = 1; next }
    /^\[/ { inside = 0 }
    inside && $1 == key { sub(/^[^"]*"/, ""); sub(/".*$/, ""); print; exit }
  ' "$ROOT/Cargo.toml"
}

workspace_version() { workspace_key version; }

# "owner/repo" of the GitHub repository: GITHUB_REPOSITORY in Actions,
# otherwise taken from `repository` in Cargo.toml.
repo_slug() {
  if [[ -n ${GITHUB_REPOSITORY:-} ]]; then
    printf '%s\n' "$GITHUB_REPOSITORY"
    return
  fi
  local url
  url="$(workspace_key repository)"
  [[ $url == https://github.com/*/* ]] || die "repository in Cargo.toml is not a GitHub URL: $url"
  url="${url#https://github.com/}"
  printf '%s\n' "${url%.git}"
}

# macos | linux | windows (Git Bash), as in the release asset names.
host_os() {
  case "$(uname -s)" in
    Darwin) echo macos ;;
    Linux) echo linux ;;
    MINGW* | MSYS* | CYGWIN*) echo windows ;;
    *) die "unsupported OS: $(uname -s)" ;;
  esac
}

# aarch64 | x86_64, Rust's std::env::consts::ARCH names.
host_arch() {
  case "$(uname -m)" in
    arm64 | aarch64) echo aarch64 ;;
    x86_64 | amd64) echo x86_64 ;;
    *) die "unsupported CPU: $(uname -m)" ;;
  esac
}

# Windows: Git Bash puts its own /usr/bin/link (a Unix tool) ahead of MSVC's
# link.exe, and rustc would call that one. The compiler's folder goes first.
if [[ $(host_os) == windows ]] && command -v cl >/dev/null; then
  PATH="$(dirname "$(command -v cl)"):$PATH"
fi

# Cargo's target folder, as a path this shell can use.
target_dir() {
  local dir="${CARGO_TARGET_DIR:-$ROOT/target}"
  if [[ $(host_os) == windows ]]; then dir="$(cygpath -u "$dir")"; fi
  printf '%s\n' "$dir"
}

# The CEF folder the last build of cef-dll-sys used, read from its build
# script output in <target>/<profile>/build. $1 = profile folder. A CEF_PATH
# that is a CEF folder itself (with archive.json, e.g. our own CEF build) is
# the one cef-dll-sys used: the newest output may be from a build without it.
cef_dir() {
  if [[ -n ${CEF_PATH:-} && -f $CEF_PATH/archive.json ]]; then
    printf '%s\n' "$CEF_PATH"
    return
  fi
  local out newest=""
  for out in "$1"/build/cef-dll-sys-*/output; do
    grep -q '^cargo::metadata=CEF_DIR=' "$out" 2>/dev/null || continue
    if [[ -z $newest || $out -nt $newest ]]; then newest="$out"; fi
  done
  [[ -n $newest ]] || die "no CEF build output in $1/build; build magnetite first"
  local dir
  dir="$(sed -n 's/^cargo::metadata=CEF_DIR=//p' "$newest" | tr -d '\r')"
  if [[ $(host_os) == windows ]]; then dir="$(cygpath -u "$dir")"; fi
  [[ -d $dir ]] || die "CEF folder from the build is gone: $dir"
  printf '%s\n' "$dir"
}

# Lowercase hex SHA-256 of a file.
sha256_of() {
  if command -v sha256sum >/dev/null; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

# Size of a file in bytes.
size_of() { wc -c <"$1" | tr -d ' '; }

# An OpenSSL 3 binary: Ed25519 with -rawin needs it, and macOS's own
# /usr/bin/openssl is LibreSSL. OPENSSL overrides the choice.
openssl3() {
  local bin
  for bin in "${OPENSSL:-openssl}" openssl /opt/homebrew/opt/openssl@3/bin/openssl /usr/local/opt/openssl@3/bin/openssl; do
    if command -v "$bin" >/dev/null && [[ $("$bin" version) == "OpenSSL "[3-9]* ]]; then
      "$bin" "$@"
      return
    fi
  done
  die "OpenSSL 3 not found (macOS: brew install openssl@3, or set OPENSSL)"
}
