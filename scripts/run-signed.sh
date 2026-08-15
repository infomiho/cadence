#!/bin/sh
set -eu

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
cd "$script_dir/.."

metadata=$(cargo metadata --format-version 1 --no-deps)
target_directory=$(printf '%s\n' "$metadata" | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
if [ -z "$target_directory" ]; then
  echo "Cargo did not report a target directory." >&2
  exit 1
fi
app="$target_directory/debug/Cadence.app"
executable="$app/Contents/MacOS/Cadence"

mkdir -p "$target_directory/debug"
build_lock="$target_directory/debug/.cadence-build-lock"
if ! mkdir "$build_lock" 2>/dev/null; then
  lock_pid=$(cat "$build_lock/pid" 2>/dev/null || true)
  if [ -n "$lock_pid" ] && ! kill -0 "$lock_pid" 2>/dev/null; then
    rm -rf "$build_lock"
    mkdir "$build_lock"
  else
    echo "Another Cadence signed build is already running." >&2
    exit 1
  fi
fi
printf '%s\n' "$$" >"$build_lock/pid"
trap 'rm -f "$build_lock/pid"; rmdir "$build_lock" 2>/dev/null || true' EXIT HUP INT TERM

identity=${CADENCE_CODESIGN_IDENTITY:-}
if [ -z "$identity" ]; then
  identities=$(
    security find-identity -v -p codesigning |
      sed -n 's/^[[:space:]]*[0-9][0-9]*) \([A-F0-9]*\) ".*"$/\1/p'
  )
  identity_count=$(printf '%s\n' "$identities" | sed '/^$/d' | wc -l | tr -d ' ')
  if [ "$identity_count" -ne 1 ]; then
    echo "Found $identity_count code-signing identities. Set CADENCE_CODESIGN_IDENTITY." >&2
    exit 1
  fi
  identity=$identities
fi
binary="$target_directory/debug/spotify-gpui-client"
contents="$app/Contents"

if [ ! -f assets/AppIcon.icns ] ||
  [ assets/AppIcon.svg -nt assets/AppIcon.icns ] ||
  [ assets/cadence-mark.svg -nt assets/cadence-mark.png ] ||
  [ scripts/build-icon.sh -nt assets/AppIcon.icns ]; then
  ./scripts/build-icon.sh
fi

cargo build

rm -rf "$app"
mkdir -p "$contents/MacOS" "$contents/Resources"
cp "$binary" "$contents/MacOS/Cadence"
cp assets/Info.plist "$contents/Info.plist"
cp assets/AppIcon.icns "$contents/Resources/AppIcon.icns"

codesign --force --sign "$identity" --identifier com.Cadence.Cadence "$app"

running_pid=$(ps -axo pid=,command= | awk -v executable="$executable" '$2 == executable && NF == 2 { print $1; exit }')
if [ -n "$running_pid" ]; then
  osascript -e 'tell application id "com.Cadence.Cadence" to quit' >/dev/null 2>&1 || true
  attempts=0
  while kill -0 "$running_pid" 2>/dev/null && [ "$attempts" -lt 50 ]; do
    sleep 0.1
    attempts=$((attempts + 1))
  done
  if kill -0 "$running_pid" 2>/dev/null; then
    kill -KILL "$running_pid"
  fi
fi

set -- "$app" "$@"
if [ -n "${SPOTIFY_CLIENT_ID:-}" ]; then
  set -- "$@" --env "SPOTIFY_CLIENT_ID=$SPOTIFY_CLIENT_ID"
fi
if ! open "$@"; then
  sleep 1
  open "$@"
fi
