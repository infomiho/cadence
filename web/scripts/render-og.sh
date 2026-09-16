#!/bin/sh
# Renders scripts/card.html to static/og.png at 1200x630.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
chrome=${CHROME:-"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"}

"$chrome" --headless --disable-gpu --hide-scrollbars \
  --force-device-scale-factor=1 --window-size=1200,630 \
  --screenshot="$root/web/static/og.png" \
  "file://$root/web/scripts/card.html" >/dev/null 2>&1

echo "wrote web/static/og.png"
