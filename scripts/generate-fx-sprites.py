#!/usr/bin/env python3
"""Generate small arena FX sprites under assets/sprites/.

Self-contained (Python 3 stdlib only), following docs/art-direction.md.
Currently one sprite:

  contact_shadow.png  soft dark ellipse blob rendered under each fighter's
                      feet by src/arena/mod.rs (combat redesign §6, fighter
                      primacy). Painted as a radial alpha falloff in the
                      palette's near-black so the runtime can tint/fade it
                      with Sprite::color alone.

Usage: python3 scripts/generate-fx-sprites.py

The output PNG is committed; re-run only when changing the art. All output
is self-generated placeholder-free FX art, dedicated to the public domain
(CC0); see assets/CREDITS.md.
"""

import os
import struct
import zlib

WIDTH, HEIGHT = 96, 24
# Near-black walnut from the art-direction palette (#1a1214).
SHADOW_RGB = (26, 18, 20)
# Peak alpha at the ellipse center; the runtime applies a further global
# alpha via Sprite::color, so this stays strong for tinting headroom.
PEAK_ALPHA = 230


def write_png(path, width, height, pixels):
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


def contact_shadow():
    rows = []
    cx, cy = (WIDTH - 1) / 2.0, (HEIGHT - 1) / 2.0
    rx, ry = WIDTH / 2.0, HEIGHT / 2.0
    for y in range(HEIGHT):
        row = bytearray()
        for x in range(WIDTH):
            # Normalized elliptical distance from the center.
            d = ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2
            if d >= 1.0:
                alpha = 0
            else:
                # Smooth quadratic falloff: dense core, feathered edge.
                alpha = int(PEAK_ALPHA * (1.0 - d) ** 2)
            row.extend((*SHADOW_RGB, alpha))
        rows.append(row)
    return rows


def main():
    out_dir = os.path.join(os.path.dirname(__file__), "..", "assets", "sprites")
    path = os.path.join(out_dir, "contact_shadow.png")
    write_png(path, WIDTH, HEIGHT, contact_shadow())
    print(f"wrote {os.path.relpath(path)} ({os.path.getsize(path)} bytes)")


if __name__ == "__main__":
    main()
