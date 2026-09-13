#!/bin/sh
# Creates the Sparkle update signing key, or reuses the one already in the
# login keychain, then stores the private key as the SPARKLE_PRIVATE_KEY GitHub
# secret and writes the public key into assets/Info.plist.
#
# The exported private key is the only copy outside this Mac's keychain. Keep
# it somewhere safe: every installed Cadence trusts only updates signed with
# it, so losing the key means users have to reinstall by hand.
set -eu

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
cd "$script_dir/.."

gh auth status >/dev/null 2>&1 || { echo "Run 'gh auth login' first." >&2; exit 1; }
repo=$(gh repo view --json nameWithOwner -q .nameWithOwner)
tools="$(./scripts/fetch-sparkle.sh)/bin"

printf 'Path to write the private key backup (outside the repository): '
read -r backup
case "$backup" in
  "~") backup="$HOME" ;;
  "~/"*) backup="$HOME/${backup#"~/"}" ;;
esac
case "$backup" in
  "$PWD"/* | ./* | "") echo "Choose a path outside the repository." >&2; exit 1 ;;
esac
mkdir -p "$(dirname "$backup")"

"$tools/generate_keys" >/dev/null
"$tools/generate_keys" -x "$backup"
public_key=$("$tools/generate_keys" -p)
test -n "$public_key"

gh secret set SPARKLE_PRIVATE_KEY --repo "$repo" <"$backup"
/usr/libexec/PlistBuddy -c "Delete :SUPublicEDKey" assets/Info.plist 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :SUPublicEDKey string $public_key" assets/Info.plist
# PlistBuddy rewrites the file with tabs; keep the repository's indentation.
sed -i '' 's/^	/  /' assets/Info.plist

echo "Private key backed up to $backup and stored as SPARKLE_PRIVATE_KEY on $repo."
echo "Public key written to assets/Info.plist. Commit that change."
