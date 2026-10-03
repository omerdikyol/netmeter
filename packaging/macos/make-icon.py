#!/usr/bin/env python3
"""Draw the app icon and write it as a PNG.

The icon is generated rather than hand-drawn in an editor so it can be tweaked
in code: a dark rounded square with the same up/down arrows as the menu bar
glyph, in the same blue and orange as the panel.

    packaging/macos/make-icon.py /tmp/icon-1024.png

Then build the .icns:

    mkdir -p /tmp/NetMeter.iconset
    for s in 16 32 64 128 256 512 1024; do
        sips -z $s $s /tmp/icon-1024.png --out /tmp/NetMeter.iconset/icon_$s.png
    done
    # ...rename to the sizes iconutil expects, then:
    iconutil -c icns /tmp/NetMeter.iconset -o assets/NetMeter.icns

(Or just use packaging/macos/make-icon.sh, which does all of it.)
"""

import struct
import sys
import zlib

SIZE = 1024
CORNER = 220
INSET = 32

BG_TOP = (0x2C, 0x2C, 0x33)
BG_BOTTOM = (0x14, 0x14, 0x18)
DOWN = (0x4D, 0xA3, 0xFF)
UP = (0xFF, 0x9F, 0x0A)

# Arrow geometry, chosen to sit on a 1024 grid with even margins.
LEFT_CX, RIGHT_CX = 340.0, 684.0
STEM_HALF = 40.0
HEAD_HALF = 130.0
TOP, MID, BOTTOM = 250.0, 520.0, 790.0

SUPERSAMPLE = 4


def in_rounded_square(x, y):
    left = top = INSET
    right = bottom = SIZE - INSET
    if x < left or x > right or y < top or y > bottom:
        return False
    cx = min(max(x, left + CORNER), right - CORNER)
    cy = min(max(y, top + CORNER), bottom - CORNER)
    dx, dy = x - cx, y - cy
    return dx * dx + dy * dy <= CORNER * CORNER


def in_down_arrow(x, y):
    """Stem on top, head narrowing to a point at the bottom."""
    if abs(x - LEFT_CX) <= STEM_HALF and TOP <= y <= MID:
        return True
    if MID <= y <= BOTTOM:
        t = (y - MID) / (BOTTOM - MID)
        return abs(x - LEFT_CX) <= HEAD_HALF * (1.0 - t)
    return False


def in_up_arrow(x, y):
    """Head narrowing to a point at the top, stem below."""
    if TOP <= y <= MID:
        t = (y - TOP) / (MID - TOP)
        return abs(x - RIGHT_CX) <= HEAD_HALF * t
    if abs(x - RIGHT_CX) <= STEM_HALF and MID <= y <= BOTTOM:
        return True
    return False


def sample(x, y):
    if not in_rounded_square(x, y):
        return (0, 0, 0, 0)
    if in_down_arrow(x, y):
        return DOWN + (255,)
    if in_up_arrow(x, y):
        return UP + (255,)
    t = (y - INSET) / float(SIZE - 2 * INSET)
    bg = tuple(
        int(BG_TOP[i] + (BG_BOTTOM[i] - BG_TOP[i]) * t) for i in range(3)
    )
    return bg + (255,)


def render():
    rows = []
    step = 1.0 / SUPERSAMPLE
    total = float(SUPERSAMPLE * SUPERSAMPLE)
    for py in range(SIZE):
        row = bytearray()
        for px in range(SIZE):
            r = g = b = a = 0
            for sy in range(SUPERSAMPLE):
                for sx in range(SUPERSAMPLE):
                    cr, cg, cb, ca = sample(px + (sx + 0.5) * step, py + (sy + 0.5) * step)
                    r += cr * ca
                    g += cg * ca
                    b += cb * ca
                    a += ca
            if a == 0:
                row += b"\x00\x00\x00\x00"
            else:
                row += bytes((r // a, g // a, b // a, int(a / total)))
        rows.append(row)
    return rows


def chunk(tag, data):
    body = tag + data
    return (
        struct.pack(">I", len(data))
        + body
        + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)
    )


def to_png(rows):
    raw = b"".join(b"\x00" + bytes(row) for row in rows)
    header = struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "icon-1024.png"
    with open(out, "wb") as handle:
        handle.write(to_png(render()))
    print(f"wrote {out} ({SIZE}x{SIZE})")


if __name__ == "__main__":
    main()
