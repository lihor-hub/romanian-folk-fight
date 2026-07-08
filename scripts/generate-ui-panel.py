#!/usr/bin/env python3
"""Generate the embroidery-motif 9-slice UI panel border under assets/ui/.

Self-contained (Python 3 stdlib only, no PIL), following
`docs/art-direction.md`: a geometric cross-stitch (ii) motif in gold on a
deep-red band, framed by black corners, around a dark translucent center that
becomes the panel fill once 9-slice stretches it. Consumed by
`src/theme/mod.rs` via `NodeImageMode::Sliced` on menu panels, HUD fighter
panels, and result dialogs.

Usage: python3 scripts/generate-ui-panel.py

The output PNG is committed; re-run only when changing the art. Self-generated
placeholder-quality art, dedicated to the public domain (CC0); see
assets/CREDITS.md.
"""

import os
import struct
import zlib

SIZE = 96
BORDER = 24  # 9-slice inset in pixels; matches TextureSlicer::border in the theme module.
CORNER = 24  # corner block size (kept unscaled by 9-slicing)

# Art-direction palette (docs/art-direction.md).
DEEP_RED = (122, 31, 31, 255)
BLACK = (26, 18, 20, 255)
CREAM = (232, 220, 200, 255)
GOLD = (201, 162, 39, 255)
PANEL_FILL = (18, 13, 12, 217)  # dark translucent panel interior (~0.85 alpha)

OUT_PATH = os.path.join(os.path.dirname(__file__), "..", "assets", "ui", "panel_border.png")


def write_png(path, width, height, pixels):
    """pixels: list of rows, each row a flat bytearray of RGBA."""

    def chunk(tag, data):
        payload = tag + data
        return (
            struct.pack(">I", len(data))
            + payload
            + struct.pack(">I", zlib.crc32(payload))
        )

    raw = b"".join(b"\x00" + bytes(row) for row in pixels)
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as handle:
        handle.write(png)


def diamond(px, cx, cy, r, color):
    for y in range(cy - r, cy + r + 1):
        for x in range(cx - r, cx + r + 1):
            if 0 <= x < SIZE and 0 <= y < SIZE and abs(x - cx) + abs(y - cy) <= r:
                px[y][x] = color


def build():
    px = [[PANEL_FILL for _ in range(SIZE)] for _ in range(SIZE)]

    # Deep-red border band around the whole edge.
    for y in range(SIZE):
        for x in range(SIZE):
            if x < BORDER or x >= SIZE - BORDER or y < BORDER or y >= SIZE - BORDER:
                px[y][x] = DEEP_RED

    # Black corner blocks, kept crisp at every scale (max_corner_scale = 1.0
    # in the theme module means these never stretch beyond their own pixels).
    for cy0, cy1 in ((0, CORNER), (SIZE - CORNER, SIZE)):
        for cx0, cx1 in ((0, CORNER), (SIZE - CORNER, SIZE)):
            for y in range(cy0, cy1):
                for x in range(cx0, cx1):
                    px[y][x] = BLACK

    # Gold cross-stitch (ii) diamond centered in each corner.
    for cx, cy in ((CORNER // 2, CORNER // 2), (SIZE - CORNER // 2, CORNER // 2),
                   (CORNER // 2, SIZE - CORNER // 2), (SIZE - CORNER // 2, SIZE - CORNER // 2)):
        diamond(px, cx, cy, CORNER // 2 - 4, GOLD)
        diamond(px, cx, cy, 3, CREAM)

    # Thin gold trim lines at the inner and outer edges of the red band.
    for y in range(SIZE):
        for x in range(SIZE):
            on_outer = x == 0 or x == SIZE - 1 or y == 0 or y == SIZE - 1
            on_inner = x == BORDER - 1 or x == SIZE - BORDER or y == BORDER - 1 or y == SIZE - BORDER
            in_band = BORDER <= x < SIZE - BORDER or BORDER <= y < SIZE - BORDER
            if (on_outer or on_inner) and (x < BORDER or x >= SIZE - BORDER or y < BORDER or y >= SIZE - BORDER):
                px[y][x] = GOLD

    # Repeating diamond stitches along the middle of each straight red band
    # (skipping the corners), the embroidered ii cross-stitch motif.
    mid = BORDER // 2
    step = 12
    for x in range(BORDER + step // 2, SIZE - BORDER, step):
        diamond(px, x, mid, 4, GOLD)
        diamond(px, x, SIZE - 1 - mid, 4, GOLD)
    for y in range(BORDER + step // 2, SIZE - BORDER, step):
        diamond(px, mid, y, 4, GOLD)
        diamond(px, SIZE - 1 - mid, y, 4, GOLD)

    return px


def main():
    os.makedirs(os.path.dirname(OUT_PATH), exist_ok=True)
    pixels = build()
    rows = [b"".join(bytes(c) for c in row) for row in pixels]
    write_png(OUT_PATH, SIZE, SIZE, rows)
    print(f"wrote {OUT_PATH}")


if __name__ == "__main__":
    main()
