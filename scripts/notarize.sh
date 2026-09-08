#!/bin/sh
# Notarizes a DMG with an App Store Connect API key and staples the ticket.
#
# Requires APPLE_API_KEY_PATH (the .p8 file), APPLE_API_KEY_ID and
# APPLE_API_ISSUER_ID.
set -eu

image=${1:?usage: notarize.sh <dmg>}
: "${APPLE_API_KEY_PATH:?}" "${APPLE_API_KEY_ID:?}" "${APPLE_API_ISSUER_ID:?}"

submission=$(
  xcrun notarytool submit "$image" \
    --key "$APPLE_API_KEY_PATH" \
    --key-id "$APPLE_API_KEY_ID" \
    --issuer "$APPLE_API_ISSUER_ID" \
    --wait --output-format json
)
status=$(printf '%s' "$submission" | sed -n 's/.*"status":"\([^"]*\)".*/\1/p')
if [ "$status" != "Accepted" ]; then
  id=$(printf '%s' "$submission" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  echo "Notarization $status for submission $id" >&2
  xcrun notarytool log "$id" \
    --key "$APPLE_API_KEY_PATH" \
    --key-id "$APPLE_API_KEY_ID" \
    --issuer "$APPLE_API_ISSUER_ID" >&2 || true
  exit 1
fi

xcrun stapler staple "$image"
spctl --assess --type open --context context:primary-signature "$image"
(cd "$(dirname "$image")" && shasum -a 256 "$(basename "$image")" >"$(basename "$image").sha256")
