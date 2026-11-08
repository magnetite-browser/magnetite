#!/usr/bin/env bash
# CI only: put the Developer ID certificate into a temporary keychain and
# export its identity as MACOS_SIGN_IDENTITY for scripts/build.sh.
#
# Env: APPLE_CERTIFICATE_P12_BASE64  base64 of the .p12 (certificate + private key)
#      APPLE_CERTIFICATE_PASSWORD    the .p12's password
#      RUNNER_TEMP, GITHUB_ENV       set by GitHub Actions
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

: "${APPLE_CERTIFICATE_P12_BASE64:?}" "${APPLE_CERTIFICATE_PASSWORD:?}" "${RUNNER_TEMP:?}" "${GITHUB_ENV:?}"
keychain="$RUNNER_TEMP/magnetite-signing.keychain-db"
p12="$RUNNER_TEMP/magnetite-signing.p12"
password="$(openssl rand -base64 24)"

printf '%s' "$APPLE_CERTIFICATE_P12_BASE64" | base64 --decode >"$p12"
security create-keychain -p "$password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$password" "$keychain"
security import "$p12" -k "$keychain" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
rm -f "$p12"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" >/dev/null
# Search the new keychain first, keep the existing ones.
# shellcheck disable=SC2046 # one word per keychain path
security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')

identity="$(security find-identity -v -p codesigning "$keychain" | awk '/Developer ID Application/ { print $2; exit }')"
[[ -n $identity ]] || die "no Developer ID Application identity in the certificate"
echo "MACOS_SIGN_IDENTITY=$identity" >>"$GITHUB_ENV"
log "signing identity $identity"
