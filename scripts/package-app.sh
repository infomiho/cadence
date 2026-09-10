#!/bin/sh
# Bundles Cadence.app from a built binary and packages it as a DMG in dist/.
#
# With CADENCE_CODESIGN_IDENTITY set to a Developer ID Application identity the
# app and the DMG are signed for distribution (hardened runtime, timestamp);
# otherwise they carry an ad-hoc signature for local use.
#
# The Sparkle framework that powers in-app updates is embedded from the
# pinned release that fetch-sparkle.sh downloads.
set -eu

profile=${1:-release}
script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
cd "$script_dir/.."

metadata=$(cargo metadata --format-version 1 --no-deps)
target_directory=$(printf '%s\n' "$metadata" | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
version=$(printf '%s\n' "$metadata" | sed -n 's/.*"version":"\([^"]*\)".*/\1/p')
test -n "$target_directory"
test -n "$version"

binary="$target_directory/$profile/cadence"
test -x "$binary"

# Sparkle compares CFBundleVersion, so it has to be a monotonic number rather
# than the semver string. Pre-release suffixes have no place in that number,
# which also keeps them out of the single stable update feed.
case "$version" in
  *[!0-9.]* | *..* | .* | *.)
    echo "Version $version must be MAJOR.MINOR.PATCH to package a release." >&2
    exit 1
    ;;
esac
build_number=$(printf '%s\n' "$version" | awk -F. 'NF == 3 { print $1 * 1000000 + $2 * 1000 + $3 }')
test -n "$build_number"

sparkle_framework_source="$(./scripts/fetch-sparkle.sh)/Sparkle.framework"

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
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $build_number" "$contents/Info.plist"
if [ "$identity" = "-" ]; then
  # An ad-hoc bundle is a local build with nothing to update to, and the app
  # leaves Sparkle alone when the feed is missing.
  /usr/libexec/PlistBuddy -c "Delete :SUFeedURL" "$contents/Info.plist"
fi

# Cadence is not sandboxed, so Sparkle's XPC services never run. They go,
# along with the header and module folders, so the shipped framework carries
# no unsigned nested code and no development artifacts.
sparkle_framework="$contents/Frameworks/Sparkle.framework"
mkdir -p "$contents/Frameworks"
cp -R "$sparkle_framework_source" "$sparkle_framework"
for extra in XPCServices Headers PrivateHeaders Modules; do
  rm -rf "$sparkle_framework/$extra" "$sparkle_framework/Versions/B/$extra"
done

# Copied resources can carry Finder info that codesign rejects as detritus.
xattr -cr "$app"
# Nested code signs first. Library validation under the hardened runtime
# requires the framework to carry the same identity as the app.
sign "$sparkle_framework/Versions/B/Autoupdate"
sign "$sparkle_framework/Versions/B/Updater.app"
sign "$sparkle_framework"
sign "$app"
codesign --verify --deep --strict "$app"

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
