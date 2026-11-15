#!/usr/bin/env bash
# Make the folder with the ad blocker's scriptlets that every package
# carries next to the app, and print its path:
#
#   scriptlets.json   the scriptlets' code (tools/scriptlets.mjs)
#   LICENSE.txt       uBlock Origin's licence, the GPL-3.0
#   README.txt        where the code is from
#
# The code is uBlock Origin's, taken from its repository at the commit below.
# It is under the GPL-3.0, so it stays a file of its own: the app reads it at
# run time and nothing of it is compiled in (docs/legal/license-compatibility.md).
# To update, set the commit of a newer release and build.
#
# Needs git and Node 22. Made once per commit, in the target folder.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

# uBlock Origin 1.76.2
commit=e25a1b3b9e0a9c8db345453f1b8a0a9d109668b7
repository=https://github.com/gorhill/uBlock

out="$(target_dir)/scriptlets/$commit"
if [[ ! -f $out/scriptlets.json ]]; then
  log "scriptlets (uBlock Origin ${commit:0:12})"
  work="$(target_dir)/scriptlets/checkout"
  rm -rf "$work" "$out"
  mkdir -p "$work"
  # A commit is named by the hash of its content: what arrives is what was pinned.
  git -C "$work" init -q
  git -C "$work" fetch -q --depth 1 "$repository" "$commit"
  git -C "$work" checkout -q FETCH_HEAD
  [[ $(git -C "$work" rev-parse HEAD) == "$commit" ]] || die "not commit $commit"
  mkdir -p "$out.part"
  node "$ROOT/tools/scriptlets.mjs" "$work" "$out.part/scriptlets.json"
  cp "$work/LICENSE.txt" "$out.part/LICENSE.txt"
  cat >"$out.part/README.txt" <<EOF
scriptlets.json holds the scriptlets of uBlock Origin: small scripts that
filter lists run in web pages. Magnetite's ad blocker reads the file when it
loads its lists; it is not part of the Magnetite program.

Source: $repository at commit $commit
(src/js/resources/), unchanged: each scriptlet's function as text, base64
encoded, with its name, other names and dependencies.
Copyright: Raymond Hill and the uBlock Origin contributors.
Licence: GNU General Public License, version 3 (LICENSE.txt).
EOF
  rm -rf "$work"
  mv "$out.part" "$out"
fi
printf '%s\n' "$out"
