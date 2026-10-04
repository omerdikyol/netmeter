#!/bin/sh
# Package the assembled app as a .dmg, with an Applications shortcut to drag onto.
#
# Usage: packaging/macos/dmg.sh [path/to/NetMeter.app] [output.dmg]
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
app=${1:-"$root/dist/NetMeter.app"}
out=${2:-"$root/dist/NetMeter-macos-universal.dmg"}

if [ ! -d "$app" ]; then
    echo "error: no app bundle at $app" >&2
    echo "run packaging/macos/bundle.sh first" >&2
    exit 1
fi

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

cp -R "$app" "$stage/NetMeter.app"
ln -s /Applications "$stage/Applications"

rm -f "$out"
# UDZO: compressed, read-only, which is what you want for distribution.
hdiutil create -volname "NetMeter" -srcfolder "$stage" -ov -format UDZO "$out" >/dev/null

echo "wrote $out"
