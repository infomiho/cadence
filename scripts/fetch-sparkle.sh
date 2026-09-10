#!/bin/sh
# Downloads the pinned Sparkle release once into .cache/sparkle and prints the
# directory that holds Sparkle.framework and the bin/ tools.
set -eu

version="2.9.6"
sha256="52bf9e88cdd972fc0c81501377a880e90d47031bd8ca5462488f843e2609e192"

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
cd "$script_dir/.."

cache=".cache/sparkle/$version"
if [ ! -d "$cache/Sparkle.framework" ]; then
  staging=$(mktemp -d)
  archive="$staging/Sparkle-$version.tar.xz"
  curl -fsSL --retry 3 -o "$archive" \
    "https://github.com/sparkle-project/Sparkle/releases/download/$version/Sparkle-$version.tar.xz"
  echo "$sha256  $archive" | shasum -a 256 -c - >/dev/null
  tar -xJf "$archive" -C "$staging" ./Sparkle.framework ./bin
  rm -f "$archive"
  rm -rf "$cache"
  mkdir -p "$(dirname "$cache")"
  mv "$staging" "$cache"
fi
printf '%s\n' "$cache"
