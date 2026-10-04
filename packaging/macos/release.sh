#!/bin/sh
# Build everything that gets published: the app, and the two archives.
#
# Usage: packaging/macos/release.sh [path-to-binary]
#   default binary: target/release/netmeter
#
# Signing is skipped, with an explanation, when no credentials are present; see
# packaging/macos/sign.sh.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
binary=${1:-"$root/target/release/netmeter"}

app="$root/dist/NetMeter.app"
dmg="$root/dist/NetMeter-macos-universal.dmg"
zip="$root/dist/NetMeter-macos-universal.zip"

"$root/packaging/macos/bundle.sh" "$binary"

# The app is signed and stapled first, so a copy dragged out of the disk image
# verifies even with no network.
"$root/packaging/macos/sign.sh" "$app"

"$root/packaging/macos/dmg.sh" "$app" "$dmg"

# The disk image is what people actually download, so it carries its own
# signature and its own notarization ticket.
"$root/packaging/macos/sign.sh" "$dmg"

rm -f "$zip"
ditto -c -k --keepParent "$app" "$zip"

echo
echo "--- artifacts ---"
ls -lh "$dmg" "$zip" | awk '{print $9, $5}'
echo "--- hashes (the cask needs the dmg one) ---"
shasum -a 256 "$dmg" "$zip"
