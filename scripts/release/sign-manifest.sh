#!/usr/bin/env bash
# Sign the update manifest: writes <manifest>.sig, the base64 (one line, no
# newline) of the raw 64-byte Ed25519 signature over the manifest's bytes.
#
#   MAGNETITE_UPDATE_KEY="$(cat key.pem)" scripts/release/sign-manifest.sh latest.json
#   scripts/release/sign-manifest.sh latest.json key.pem
#
# The key is the PEM private key from new-update-key.sh. Without one this
# fails: an unsigned manifest must never be published. If
# MAGNETITE_UPDATE_PUBLIC_KEY is set (base64 of the raw 32 bytes, as the app
# embeds it), the signing key must match it.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

manifest="${1:?usage: sign-manifest.sh <manifest> [key.pem]}"
[[ -f $manifest ]] || die "no manifest at $manifest"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
key="${2:-}"
if [[ -z $key ]]; then
  [[ -n ${MAGNETITE_UPDATE_KEY:-} ]] || die "MAGNETITE_UPDATE_KEY is not set: refusing to publish an unsigned manifest"
  key="$tmp/key.pem"
  (umask 077 && printf '%s\n' "$MAGNETITE_UPDATE_KEY" >"$key")
fi

public="$(openssl3 pkey -in "$key" -pubout -outform DER | tail -c 32 | openssl3 base64 -A)"
if [[ -n ${MAGNETITE_UPDATE_PUBLIC_KEY:-} && $public != "$MAGNETITE_UPDATE_PUBLIC_KEY" ]]; then
  die "signing key $public is not the app's key $MAGNETITE_UPDATE_PUBLIC_KEY"
fi

openssl3 pkeyutl -sign -inkey "$key" -rawin -in "$manifest" -out "$tmp/sig"
[[ $(size_of "$tmp/sig") == 64 ]] || die "unexpected signature size"
openssl3 pkey -in "$key" -pubout -out "$tmp/public.pem"
openssl3 pkeyutl -verify -pubin -inkey "$tmp/public.pem" -rawin -in "$manifest" -sigfile "$tmp/sig" >/dev/null
openssl3 base64 -A -in "$tmp/sig" | tr -d '\n' >"$manifest.sig"
log "signed $manifest with public key $public"
