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

# Gatekeeper's own answer is the one that matters, so fail the release rather
# than shipping something it will refuse. Needs --verbose=4: plain `codesign -dv`
# prints no Authority lines, so a weaker check silently skips this entirely.
if codesign -dv --verbose=4 "$app" 2>&1 | grep -q "Developer ID"; then
    echo "--- checking what Gatekeeper will say ---"
    spctl -a -vvv -t install "$app"
    xcrun stapler validate "$app"
    xcrun stapler validate "$dmg"
    echo "--- Gatekeeper accepts it ---"
else
    echo "--- skipping the Gatekeeper check: nothing signed this build ---"
fi

echo "--- hashes (the cask needs the dmg one) ---"
shasum -a 256 "$dmg" "$zip"
