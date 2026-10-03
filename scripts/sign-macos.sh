#!/usr/bin/env bash
# Developer ID signature and notarization for the macOS server.
#
# Reproducibility is checked on the PRE-SIGNING bytes, in the compare
# job, because signing and notarization change them. Anybody can strip
# the signature and compare for themselves, which is the honest version
# of "reproducible on macOS".
#
# Usage: scripts/sign-macos.sh <path to the universal airdress-mcp>
set -euo pipefail

binary="${1:?usage: $0 <binary>}"

: "${APPLE_CERT_P12:?}"
: "${APPLE_CERT_PASSWORD:?}"
: "${APPLE_SIGNING_IDENTITY:?}"
: "${APPLE_API_KEY_ID:?}"
: "${APPLE_API_ISSUER_ID:?}"
: "${APPLE_API_KEY_P8:?}"

keychain=$(mktemp -d)/build.keychain
password=$(python3 -c 'import secrets;print(secrets.token_urlsafe(32))')

security create-keychain -p "$password" "$keychain"
security set-keychain-settings -lut 900 "$keychain"
security unlock-keychain -p "$password" "$keychain"
security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')

cert=$(mktemp).p12
echo "$APPLE_CERT_P12" | base64 --decode > "$cert"
security import "$cert" -k "$keychain" -P "$APPLE_CERT_PASSWORD" \
    -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple:,codesign: \
    -s -k "$password" "$keychain" > /dev/null
rm -f "$cert"

# Hardened runtime and a timestamp: notarization refuses without both.
codesign --force --options runtime --timestamp \
    --sign "$APPLE_SIGNING_IDENTITY" "$binary"
codesign --verify --strict --verbose=2 "$binary"

key=$(mktemp).p8
echo "$APPLE_API_KEY_P8" | base64 --decode > "$key"

# notarytool takes an archive, not a bare binary.
archive=$(mktemp -d)/airdress-mcp.zip
ditto -c -k --keepParent "$binary" "$archive"
xcrun notarytool submit "$archive" \
    --key "$key" \
    --key-id "$APPLE_API_KEY_ID" \
    --issuer "$APPLE_API_ISSUER_ID" \
    --wait
rm -f "$key"

# A bare binary cannot be stapled, so the check is Gatekeeper's own
# verdict on the signature and the notarization ticket it fetches.
spctl --assess --type execute --verbose=2 "$binary" || {
    echo "Gatekeeper refused the signed binary" >&2
    exit 1
}
security delete-keychain "$keychain"
echo "signed and notarized: $binary"
