#!/bin/sh
set -eu

script_dir=$(CDPATH='' cd -- "$(dirname "$0")" && pwd)
root=$(dirname "$script_dir")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

qlmanage -t -s 1024 -o "$work" "$root/assets/AppIcon.svg" >/dev/null
source="$work/AppIcon.svg.png"
iconset="$work/AppIcon.iconset"
mkdir -p "$iconset"

render() {
  size=$1
  output=$2
  sips -z "$size" "$size" "$source" --out "$iconset/$output" >/dev/null
}

render 16 icon_16x16.png
render 32 icon_16x16@2x.png
render 32 icon_32x32.png
render 64 icon_32x32@2x.png
render 128 icon_128x128.png
render 256 icon_128x128@2x.png
render 256 icon_256x256.png
render 512 icon_256x256@2x.png
render 512 icon_512x512.png
cp "$source" "$iconset/icon_512x512@2x.png"

iconutil -c icns "$iconset" -o "$root/assets/AppIcon.icns"

sips -s format png -z 64 64 "$root/assets/cadence-mark.svg" \
  --out "$root/assets/cadence-mark.png" >/dev/null
