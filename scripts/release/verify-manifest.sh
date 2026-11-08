#!/usr/bin/env bash
# Check latest.json.sig the way the app does: with only the public key.
#
#   scripts/release/verify-manifest.sh <latest.json> <public key>
#
# <public key> is the base64 of the raw 32 bytes (new-update-key.sh prints it).
# The signature is read from <latest.json>.sig.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

manifest="${1:?usage: verify-manifest.sh <latest.json> <public key>}"
public="${2:?usage: verify-manifest.sh <latest.json> <public key>}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# An Ed25519 SubjectPublicKeyInfo is this fixed 12-byte DER header plus the key.
{
  printf '\x30\x2a\x30\x05\x06\x03\x2b\x65\x70\x03\x21\x00'
  printf '%s' "$public" | openssl3 base64 -d -A
} >"$tmp/public.der"
[[ $(size_of "$tmp/public.der") == 44 ]] || die "the public key is not 32 bytes of base64"
openssl3 base64 -d -A -in "$manifest.sig" >"$tmp/sig"
openssl3 pkeyutl -verify -pubin -keyform DER -inkey "$tmp/public.der" -rawin \
  -in "$manifest" -sigfile "$tmp/sig"
