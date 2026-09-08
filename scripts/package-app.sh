#!/bin/sh
# Bundles Cadence.app from a built binary and packages it as a DMG in dist/.
#
# With CADENCE_CODESIGN_IDENTITY set to a Developer ID Application identity the
# app and the DMG are signed for distribution (hardened runtime, timestamp);
# otherwise they carry an ad-hoc signature for local use.
set -eu

profile=${1:-release}
script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
cd "$script_dir/.."

metadata=$(cargo metadata --format-version 1 --no-deps)
target_directory=$(printf '%s\n' "$metadata" | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
version=$(printf '%s\n' "$metadata" | sed -n 's/.*"version":"\([^"]*\)".*/\1/p')
test -n "$target_directory"
test -n "$version"

binary="$target_directory/$profile/spotify-gpui-client"
test -x "$binary"

identity=${CADENCE_CODESIGN_IDENTITY:--}
sign() {
  if [ "$identity" = "-" ]; then
    codesign --force --sign - "$@"
  else
    codesign --force --timestamp --options runtime --sign "$identity" "$@"
  fi
}

app="$target_directory/$profile/Cadence.app"
contents="$app/Contents"
rm -rf "$app"
mkdir -p "$contents/MacOS" "$contents/Resources" dist
cp "$binary" "$contents/MacOS/Cadence"
cp assets/Info.plist "$contents/Info.plist"
cp assets/AppIcon.icns "$contents/Resources/AppIcon.icns"
cp LICENSE THIRD_PARTY_NOTICES.md "$contents/Resources/"

/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $version" "$contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $version" "$contents/Info.plist"

sign "$app"
codesign --verify --strict "$app"

architecture=$(uname -m)
image="dist/Cadence-$version-macOS-$architecture.dmg"
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
cp -R "$app" "$staging/"
ln -s /Applications "$staging/Applications"
rm -f "$image" "$image.sha256"
hdiutil create -quiet -volname "Cadence" -srcfolder "$staging" -ov -format UDZO "$image"
sign "$image"
(cd "$(dirname "$image")" && shasum -a 256 "$(basename "$image")" >"$(basename "$image").sha256")
echo "$image"
