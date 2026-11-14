#!/usr/bin/env bash
# Notarize a Developer ID signed Magnetite.app and staple the ticket, then
# rebuild its zip and its disk image with the stapled app, and notarize and
# staple the disk image as well: Gatekeeper checks the image and the app in
# it apart, also without a network. The checksums change, so run this before
# make-manifest.sh.
#
#   scripts/release/notarize-macos.sh <Magnetite.app> <zip> <dmg>
#
# MACOS_SIGN_IDENTITY must name the Developer ID the app was signed with: the
# disk image is signed with it.
#
# Env (an App Store Connect API key with the Developer role):
#   APPLE_NOTARY_KEY        contents of the AuthKey_<id>.p8 file
#   APPLE_NOTARY_KEY_ID     its key id
#   APPLE_NOTARY_ISSUER_ID  the issuer id shown above the key list
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

usage="usage: notarize-macos.sh <app> <zip> <dmg>"
app="${1:?$usage}"
zip="${2:?$usage}"
dmg="${3:?$usage}"
: "${APPLE_NOTARY_KEY:?}" "${APPLE_NOTARY_KEY_ID:?}" "${APPLE_NOTARY_ISSUER_ID:?}"
[[ ${MACOS_SIGN_IDENTITY:--} != - ]] || die "MACOS_SIGN_IDENTITY is not set: the disk image needs the Developer ID"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
(umask 077 && printf '%s\n' "$APPLE_NOTARY_KEY" >"$tmp/key.p8")
auth=(--key "$tmp/key.p8" --key-id "$APPLE_NOTARY_KEY_ID" --issuer "$APPLE_NOTARY_ISSUER_ID")

# Submit a file to Apple's notary service and wait for the answer.
submit() {
  local file="$1" id
  log "submitting $(basename "$file") for notarization"
  xcrun notarytool submit "$file" "${auth[@]}" --wait --timeout 2h --output-format json >"$tmp/result.json"
  id="$(sed -n 's/.*"id" *: *"\([^"]*\)".*/\1/p' "$tmp/result.json" | head -1)"
  if ! grep -q '"status" *: *"Accepted"' "$tmp/result.json"; then
    cat "$tmp/result.json" >&2
    [[ -z $id ]] || xcrun notarytool log "$id" "${auth[@]}" >&2 || true
    die "notarization of $(basename "$file") failed"
  fi
}

# Staple the ticket to an app or disk image and check it is there. Both ask
# Apple's servers, which now and then drop the connection.
staple() {
  retry 3 xcrun stapler staple "$1"
  retry 3 xcrun stapler validate "$1"
}

submit "$zip"
staple "$app"
spctl --assess --type execute --verbose=2 "$app"
rm -f "$zip"
ditto -c -k --keepParent "$app" "$zip"
log "notarized and stapled $(basename "$app")"

# The image is made again here, with the stapled app in it.
make_dmg "$app" "$dmg"
submit "$dmg"
staple "$dmg"
spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg"
log "notarized and stapled $(basename "$dmg")"
