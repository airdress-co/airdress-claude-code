#!/usr/bin/env bash
# Developer ID signature and notarization, for the macOS server and the
# macOS launcher alike.
#
# Reproducibility is checked on the PRE-SIGNING bytes, because signing
# and notarization change them. Anybody can strip the signature and
# compare for themselves — scripts/macho-unsigned.py, which also says
# the one field codesign leaves behind — and that is the honest version
# of "reproducible on macOS".
#
# Usage: scripts/sign-macos.sh <path to a universal binary>
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
archive=$(mktemp -d)/$(basename "$binary").zip
ditto -c -k --keepParent "$binary" "$archive"
xcrun notarytool submit "$archive" \
    --key "$key" \
    --key-id "$APPLE_API_KEY_ID" \
    --issuer "$APPLE_API_ISSUER_ID" \
    --wait
rm -f "$key"

# A bare binary cannot be stapled, so the check is Gatekeeper's own
# verdict on the signature and the notarization ticket it fetches.
#
# Not `--type execute`: that assesses app bundles only, and refuses ANY
# bare command-line binary with "the code is valid but does not seem to
# be an app" — measured 2026-10-05 on a launcher Apple had just accepted.
# `--type open` with the primary-signature context assesses the binary
# itself, and answers "source=Notarized Developer ID" for a notarized
# tool (docker, code-tunnel and claude measured on a Mac) while an
# un-notarized one is rejected. Require that exact source: "accepted" on
# its own would also pass a binary signed but never notarized.
verdict=$(spctl --assess -vv --type open --context context:primary-signature "$binary" 2>&1) || true
echo "$verdict"
case "$verdict" in
    *"accepted"*"source=Notarized Developer ID"*) ;;
    *)
        echo "Gatekeeper did not accept the signed binary as notarized" >&2
        exit 1
        ;;
esac
security delete-keychain "$keychain"
echo "signed and notarized: $binary"
