#!/usr/bin/env bash
# Notarize a Developer ID signed Magnetite.app, staple the ticket and rebuild
# its zip (the zip's checksum changes, so run this before make-manifest.sh).
#
#   scripts/release/notarize-macos.sh <Magnetite.app> <zip>
#
# Env (an App Store Connect API key with the Developer role):
#   APPLE_NOTARY_KEY        contents of the AuthKey_<id>.p8 file
#   APPLE_NOTARY_KEY_ID     its key id
#   APPLE_NOTARY_ISSUER_ID  the issuer id shown above the key list
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

app="${1:?usage: notarize-macos.sh <app> <zip>}"
zip="${2:?usage: notarize-macos.sh <app> <zip>}"
: "${APPLE_NOTARY_KEY:?}" "${APPLE_NOTARY_KEY_ID:?}" "${APPLE_NOTARY_ISSUER_ID:?}"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
(umask 077 && printf '%s\n' "$APPLE_NOTARY_KEY" >"$tmp/key.p8")
auth=(--key "$tmp/key.p8" --key-id "$APPLE_NOTARY_KEY_ID" --issuer "$APPLE_NOTARY_ISSUER_ID")

log "submitting $(basename "$zip") for notarization"
xcrun notarytool submit "$zip" "${auth[@]}" --wait --timeout 2h --output-format json >"$tmp/result.json"
id="$(sed -n 's/.*"id" *: *"\([^"]*\)".*/\1/p' "$tmp/result.json" | head -1)"
if ! grep -q '"status" *: *"Accepted"' "$tmp/result.json"; then
  cat "$tmp/result.json" >&2
  [[ -z $id ]] || xcrun notarytool log "$id" "${auth[@]}" >&2 || true
  die "notarization failed"
fi

xcrun stapler staple "$app"
xcrun stapler validate "$app"
spctl --assess --type execute --verbose=2 "$app"
rm -f "$zip"
ditto -c -k --keepParent "$app" "$zip"
log "notarized and stapled $(basename "$app")"
