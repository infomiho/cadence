#!/bin/sh
# Signs the DMGs in the archives directory with the Sparkle key from
# SPARKLE_PRIVATE_KEY and writes appcast.xml beside them, with every enclosure
# pointing at the given tag's release assets. The tag message becomes the
# release notes embedded in the feed.
set -eu

tag=${1:?usage: generate-appcast.sh <tag> [archives-dir]}
archives=${2:-dist}
: "${SPARKLE_PRIVATE_KEY:?}"

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
cd "$script_dir/.."
tools="$(./scripts/fetch-sparkle.sh)/bin"

notes=$(git tag -l --format='%(contents)' "$tag")
trap 'rm -f "$archives"/*.md' EXIT
for image in "$archives"/*.dmg; do
  test -f "$image"
  if [ -n "$notes" ]; then
    printf '%s\n' "$notes" >"${image%.dmg}.md"
  fi
done

printf '%s' "$SPARKLE_PRIVATE_KEY" | "$tools/generate_appcast" \
  --ed-key-file - \
  --download-url-prefix "https://github.com/infomiho/cadence/releases/download/$tag/" \
  --embed-release-notes \
  -o "$archives/appcast.xml" \
  "$archives"

# generate_appcast still exits 0 when the key does not match and writes a
# feed nobody can install from, so the signature is checked explicitly.
grep -q 'sparkle:edSignature="' "$archives/appcast.xml" ||
  { echo "appcast.xml carries no EdDSA signature: check SPARKLE_PRIVATE_KEY." >&2; exit 1; }
echo "$archives/appcast.xml"
