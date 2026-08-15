#!/bin/sh
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

codesign --force --deep --sign "${CADENCE_CODESIGN_IDENTITY:--}" "$app"

architecture=$(uname -m)
archive="dist/Cadence-$version-macOS-$architecture.zip"
rm -f "$archive" "$archive.sha256"
ditto -c -k --keepParent "$app" "$archive"
shasum -a 256 "$archive" >"$archive.sha256"
