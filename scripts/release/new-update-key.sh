#!/usr/bin/env bash
# Make a new Ed25519 key pair for signing update manifests.
#
#   scripts/release/new-update-key.sh [file]   (default: ~/magnetite-update-key.pem)
#
# Writes the private key (mode 0600) outside the repository and prints the
# public key the app embeds. See docs/development/release.md.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

file="${1:-$HOME/magnetite-update-key.pem}"
[[ ! -e $file ]] || die "$file exists; pick another name (never overwrite a signing key)"
dir="$(cd "$(dirname "$file")" && pwd)"
[[ $dir != "$ROOT" && $dir != "$ROOT"/* ]] || die "keep the private key outside the repository"
file="$dir/$(basename "$file")"

(umask 077 && openssl3 genpkey -algorithm ed25519 -out "$file")
public="$(openssl3 pkey -in "$file" -pubout -outform DER | tail -c 32 | openssl3 base64 -A)"

cat <<EOF
Private key: $file
Public key:  $public

Next:
1. Store the private key in the GitHub secret MAGNETITE_UPDATE_KEY:
     gh secret set MAGNETITE_UPDATE_KEY --env release < "$file"
2. Put the public key into the app's updater and, as a check for the
   release workflow, into the repository variable MAGNETITE_UPDATE_PUBLIC_KEY:
     gh variable set MAGNETITE_UPDATE_PUBLIC_KEY --body "$public"
3. Keep an offline backup of the private key (password manager), then delete
   this file. Lose it and installed apps can no longer be updated; leak it
   and someone else can.
EOF
