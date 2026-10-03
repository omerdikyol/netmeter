#!/bin/sh
# Regenerate assets/NetMeter.icns from the drawing in make-icon.py.
#
# Needs python3, sips and iconutil, all of which ship with macOS.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

python3 "$root/packaging/macos/make-icon.py" "$work/icon-1024.png"

iconset="$work/NetMeter.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$work/icon-1024.png" \
        --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$work/icon-1024.png" \
        --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done

iconutil -c icns "$iconset" -o "$root/assets/NetMeter.icns"
echo "wrote $root/assets/NetMeter.icns"
