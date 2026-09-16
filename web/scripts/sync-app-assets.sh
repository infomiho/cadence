#!/bin/sh
# Copies the shared app artwork into web/static so the site builds from web/
# alone. Run after changing assets/cadence-mark.svg or assets/cadence.webp.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)

cp -f "$root/assets/cadence-mark.svg" "$root/web/static/cadence-mark.svg"
cp -f "$root/assets/cadence.webp" "$root/web/static/cadence.webp"

echo "synced app assets into web/static"
