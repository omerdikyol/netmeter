#!/bin/sh
# Assemble a minimal macOS .app bundle around a built binary.
#
# Usage: packaging/macos/bundle.sh [path-to-binary]
#   default binary: target/release/netmeter
#
# A bundle is what makes LaunchServices treat NetMeter as a real GUI agent
# (LSUIElement) rather than a background process, which is required for the
# menu bar item to be displayed reliably.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
binary=${1:-"$root/target/release/netmeter"}
app="$root/dist/NetMeter.app"

if [ ! -f "$binary" ]; then
    echo "error: binary not found: $binary" >&2
    echo "build it first, e.g. cargo build --release" >&2
    exit 1
fi

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/netmeter"
cp "$root/packaging/macos/Info.plist" "$app/Contents/Info.plist"
if [ -f "$root/assets/NetMeter.icns" ]; then
    cp "$root/assets/NetMeter.icns" "$app/Contents/Resources/NetMeter.icns"
fi
chmod +x "$app/Contents/MacOS/netmeter"

echo "bundled $app"
