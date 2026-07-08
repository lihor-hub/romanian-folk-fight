#!/usr/bin/env python3
"""Generate the tiered arena parallax backgrounds under assets/backgrounds/.

Self-contained (Python 3 stdlib only), following docs/art-direction.md:
chunky painted shapes in the folk-textile palette (deep red, black, cream,
gold) plus one muted accent per tier. Each tier ships two layers consumed by
`src/arena/fx.rs`:

  <tier>_far.png   opaque backdrop (sky + distant silhouettes), slow parallax
  <tier>_near.png  transparent overlay (foreground props), faster parallax
  <tier>_foreground.png transparent stage edge, stable foreground depth

Tiers (matching the ladder tiers from #20):
  village    fights 1-4  — sat romanesc: village square, fences, haystacks
  forest     fights 5-7  — Padurea intunecata: dark forest
  mountains  fights 8-10 — Muntii Carpati: mountain pass, fortress silhouette

Layers are 900x600 (drawn at 300x200 logical pixels, scaled x3) so the
subtle idle drift never reveals an edge on the 800x600 arena.

Usage: python3 scripts/generate-backgrounds.py

The output PNGs are committed; re-run only when changing the art. All output
is self-generated placeholder art, dedicated to the public domain (CC0); see
assets/CREDITS.md.
"""

import math
import os
import struct
import zlib

LOGICAL_W, LOGICAL_H = 300, 200
SCALE = 3
OUT_W, OUT_H = LOGICAL_W * SCALE, LOGICAL_H * SCALE

# Art-direction palette (docs/art-direction.md).
DEEP_RED = (122, 31, 31, 255)
BLACK = (26, 18, 20, 255)
CREAM = (232, 220, 200, 255)
GOLD = (201, 162, 39, 255)
CLEAR = (0, 0, 0, 0)


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


class Canvas:
    def __init__(self, fill=CLEAR):
        self.px = [[fill] * LOGICAL_W for _ in range(LOGICAL_H)]

    def put(self, x, y, color):
        if 0 <= x < LOGICAL_W and 0 <= y < LOGICAL_H:
            self.px[y][x] = color

    def rect(self, x0, y0, x1, y1, color):
        for y in range(max(0, y0), min(LOGICAL_H, y1)):
            for x in range(max(0, x0), min(LOGICAL_W, x1)):
                self.px[y][x] = color

    def band(self, y0, y1, color):
        self.rect(0, y0, LOGICAL_W, y1, color)

    def triangle(self, apex_x, apex_y, base_y, half_width, color):
        """Isosceles triangle: apex up, flat base at base_y."""
        height = max(1, base_y - apex_y)
        for y in range(apex_y, base_y):
            w = int(half_width * (y - apex_y) / height)
            self.rect(apex_x - w, y, apex_x + w + 1, y + 1, color)

    def disc(self, cx, cy, r, color):
        for y in range(cy - r, cy + r + 1):
            for x in range(cx - r, cx + r + 1):
                if (x - cx) ** 2 + (y - cy) ** 2 <= r * r:
                    self.put(x, y, color)

    def line(self, x0, y0, x1, y1, color, width=1):
        dx = abs(x1 - x0)
        dy = -abs(y1 - y0)
        sx = 1 if x0 < x1 else -1
        sy = 1 if y0 < y1 else -1
        err = dx + dy
        x, y = x0, y0
        while True:
            self.rect(
                x - width // 2,
                y - width // 2,
                x + width // 2 + 1,
                y + width // 2 + 1,
                color,
            )
            if x == x1 and y == y1:
                break
            e2 = 2 * err
            if e2 >= dy:
                err += dy
                x += sx
            if e2 <= dx:
                err += dx
                y += sy

    def ridge(self, base_y, amp, freq, phase, color):
        """Fill below a rolling sine ridge down to the canvas bottom."""
        for x in range(LOGICAL_W):
            top = int(base_y - amp * (0.6 + 0.4 * math.sin(freq * x + phase)))
            for y in range(max(0, top), LOGICAL_H):
                self.px[y][x] = color


def render(canvas):
    """Scale the logical canvas x3 into PNG rows."""
    rows = []
    for ly in range(LOGICAL_H):
        row = bytearray()
        for lx in range(LOGICAL_W):
            row.extend(bytes(canvas.px[ly][lx]) * SCALE)
        for _ in range(SCALE):
            rows.append(row)
    return rows


def house(c, x, y, w, h, body, roof):
    """Cottage silhouette: body with a triangular shingle roof."""
    c.rect(x, y, x + w, y + h, body)
    c.triangle(x + w // 2, y - h // 2, y, w // 2 + 2, roof)
    # Warm window.
    c.rect(x + w // 2 - 1, y + h // 3, x + w // 2 + 1, y + h // 3 + 2, GOLD)


def fir(c, x, base_y, height, half, color):
    """Fir tree: stacked triangles on a stub trunk."""
    c.rect(x - 1, base_y - 2, x + 1, base_y, BLACK)
    for i in range(3):
        seg_base = base_y - 2 - i * (height // 4)
        c.triangle(x, seg_base - height // 2, seg_base, half - i * 2, color)


# --- Village: sat romanesc, dusk over a village square -------------------

def village_far():
    c = Canvas()
    dusk_hi = (58, 34, 46, 255)
    dusk_lo = (122, 58, 44, 255)
    hills = (52, 44, 34, 255)
    for i, y in enumerate(range(0, 140, 20)):
        t = i / 6.0
        col = tuple(
            int(a + (b - a) * t) for a, b in zip(dusk_hi[:3], dusk_lo[:3])
        ) + (255,)
        c.band(y, y + 20, col)
    c.disc(226, 52, 12, GOLD)  # low dusk sun
    c.ridge(128, 14, 0.030, 0.8, hills)
    # Distant cottages along the square's far edge.
    body = (44, 32, 26, 255)
    roof = (86, 30, 26, 255)
    for hx, hw in ((28, 26), (86, 20), (150, 30), (222, 22), (262, 26)):
        house(c, hx, 138, hw, 22, body, roof)
    c.band(160, LOGICAL_H, (72, 56, 36, 255))  # packed-earth square
    return c


def village_near():
    c = Canvas()
    fence = (94, 66, 38, 255)
    hay = (172, 134, 58, 255)
    hay_dark = (140, 106, 44, 255)
    # Wooden fence run.
    c.rect(0, 158, LOGICAL_W, 161, fence)
    c.rect(0, 168, LOGICAL_W, 171, fence)
    for x in range(4, LOGICAL_W, 18):
        c.rect(x, 150, x + 4, 178, fence)
        c.triangle(x + 2, 146, 151, 3, fence)
    # Haystacks flanking the square.
    for hx, r in ((36, 20), (258, 24)):
        c.triangle(hx, 178 - 2 * r, 180, r, hay)
        c.triangle(hx, 178 - 2 * r + 4, 180, r - 3, hay_dark)
        c.rect(hx - 1, 178 - 2 * r - 4, hx + 1, 178 - 2 * r, fence)
    return c


def village_foreground():
    c = Canvas()
    wood = (94, 66, 38, 255)
    dark = (52, 32, 24, 255)
    red = DEEP_RED
    # Low painted plank stage edge, behind the fighter feet in world z.
    c.rect(0, 178, LOGICAL_W, LOGICAL_H, dark)
    c.rect(0, 174, LOGICAL_W, 181, wood)
    for x in range(0, LOGICAL_W, 18):
        c.rect(x + 2, 176, x + 6, 196, wood)
        c.rect(x + 9, 184, x + 13, 198, (72, 48, 30, 255))
    # Woven ii diamond trim along the lip.
    for x in range(8, LOGICAL_W, 24):
        c.triangle(x, 174, 180, 6, red)
        c.triangle(x, 180, 186, 6, red)
        c.rect(x - 1, 178, x + 1, 182, GOLD)
    # Flanking posts/crowd silhouettes stay out of the duel center.
    for x in (18, 270):
        c.rect(x, 132, x + 8, 180, wood)
        c.triangle(x + 4, 122, 132, 8, red)
    for x in range(34, 82, 12):
        c.disc(x, 168, 5, BLACK)
    for x in range(220, 268, 12):
        c.disc(x, 168, 5, BLACK)
    return c


# --- Forest: Padurea intunecata -------------------------------------------

def forest_far():
    c = Canvas()
    night = (16, 20, 24, 255)
    haze = (30, 40, 38, 255)
    deep = (24, 34, 30, 255)
    c.band(0, LOGICAL_H, night)
    c.disc(60, 40, 10, (196, 200, 190, 255))  # pale moon
    c.disc(56, 37, 9, night)  # crescent bite
    c.ridge(118, 10, 0.05, 0.0, haze)
    # Two depths of fir silhouettes.
    for x in range(6, LOGICAL_W, 24):
        fir(c, x, 150, 44, 12, deep)
    dark = (18, 26, 22, 255)
    for x in range(18, LOGICAL_W, 30):
        fir(c, x, 170, 60, 15, dark)
    c.band(168, LOGICAL_H, (20, 26, 20, 255))  # mossy floor
    return c


def forest_near():
    c = Canvas()
    trunk = (24, 18, 16, 255)
    canopy = (14, 20, 16, 255)
    fern = (34, 52, 38, 255)
    # Canopy fringe across the top.
    for x in range(0, LOGICAL_W, 10):
        depth = 26 + int(10 * math.sin(x * 0.21))
        c.rect(x, 0, x + 10, depth, canopy)
    # Great trunks at the flanks.
    for tx, tw in ((10, 16), (270, 20)):
        c.rect(tx, 0, tx + tw, 184, trunk)
        c.rect(tx - 4, 150, tx + tw + 4, 158, trunk)  # root flare
    # Undergrowth ferns.
    for x in range(0, LOGICAL_W, 22):
        c.triangle(x + 8, 168, 184, 8, fern)
    return c


def forest_foreground():
    c = Canvas()
    root = (38, 24, 20, 255)
    root_hi = (70, 46, 30, 255)
    moss = (38, 60, 42, 255)
    stone = (72, 70, 64, 255)
    # Root shelf and mossy stones along the front of the forest stage.
    c.rect(0, 182, LOGICAL_W, LOGICAL_H, root)
    for x in range(-10, LOGICAL_W, 34):
        c.line(x, 190, x + 44, 170, root_hi, 3)
        c.line(x + 6, 196, x + 40, 182, root, 3)
    for x, y, r in ((26, 174, 7), (58, 184, 5), (242, 176, 8), (272, 186, 5)):
        c.disc(x, y, r, stone)
        c.disc(x - 2, y - 2, max(2, r // 2), (98, 96, 86, 255))
    for x in range(0, LOGICAL_W, 16):
        c.triangle(x + 6, 176, 190, 6, moss)
    # Twisted roots frame the duel without crossing the center.
    for x in (6, 282):
        c.rect(x, 118, x + 10, 185, root)
        c.line(x + 5, 150, x + (-18 if x > 150 else 28), 174, root_hi, 3)
    return c


# --- Mountains: Muntii Carpati --------------------------------------------

def mountains_far():
    c = Canvas()
    sky = (22, 22, 34, 255)
    far_ridge = (52, 54, 66, 255)
    near_ridge = (36, 38, 48, 255)
    snow = (208, 210, 214, 255)
    c.band(0, LOGICAL_H, sky)
    # Stars.
    for i in range(40):
        c.put((i * 37 + 11) % LOGICAL_W, (i * 23 + 5) % 90, CREAM)
    # Far snow-capped ridge.
    for px, ph, hw in ((40, 46, 44), (120, 30, 52), (210, 40, 60), (285, 52, 40)):
        c.triangle(px, ph, 150, hw, far_ridge)
        c.triangle(px, ph, ph + 12, hw // 5 + 2, snow)
    # Nearer ridge with a fortress silhouette on its shoulder.
    c.ridge(146, 18, 0.02, 2.1, near_ridge)
    fort = (20, 20, 26, 255)
    c.rect(196, 96, 232, 122, fort)  # keep
    for tx in (192, 226):
        c.rect(tx, 88, tx + 8, 122, fort)  # towers
        c.triangle(tx + 4, 80, 88, 6, fort)
    for bx in range(198, 230, 6):
        c.rect(bx, 92, bx + 3, 96, fort)  # battlements
    c.rect(212, 108, 216, 112, GOLD)  # lit arrow slit
    c.band(170, LOGICAL_H, (42, 40, 44, 255))  # scree pass
    return c


def mountains_near():
    c = Canvas()
    rock = (54, 50, 52, 255)
    rock_dark = (40, 36, 40, 255)
    snow = (190, 194, 200, 255)
    # Rocky outcrops at the flanks of the pass.
    c.triangle(18, 108, 190, 52, rock)
    c.triangle(6, 128, 190, 34, rock_dark)
    c.triangle(284, 96, 190, 58, rock)
    c.triangle(296, 120, 190, 38, rock_dark)
    c.triangle(18, 108, 122, 12, snow)
    c.triangle(284, 96, 112, 13, snow)
    # Wind-blown snow wisps.
    for x in range(60, 240, 24):
        c.rect(x, 60 + (x // 3) % 18, x + 10, 61 + (x // 3) % 18, snow)
    return c


def mountains_foreground():
    c = Canvas()
    stone = (62, 58, 62, 255)
    dark = (34, 30, 34, 255)
    snow = (190, 194, 200, 255)
    red = DEEP_RED
    # Fortress-stone arena lip with carved textile diamonds.
    c.rect(0, 176, LOGICAL_W, LOGICAL_H, dark)
    c.rect(0, 166, LOGICAL_W, 184, stone)
    for x in range(0, LOGICAL_W, 30):
        c.rect(x, 166, x + 2, 184, dark)
        c.rect(x + 15, 174, x + 17, 200, (44, 40, 44, 255))
    for x in range(12, LOGICAL_W, 30):
        c.triangle(x, 168, 176, 8, red)
        c.triangle(x, 176, 184, 8, red)
        c.rect(x - 1, 173, x + 1, 179, GOLD)
    # Low snow banks and broken stones at the flanks.
    for x, y, r in ((34, 160, 11), (60, 174, 7), (238, 164, 10), (266, 174, 8)):
        c.disc(x, y, r, stone)
        c.rect(x - r, y - 2, x + r, y + 2, snow)
    c.rect(0, 160, 46, 166, snow)
    c.rect(254, 158, LOGICAL_W, 166, snow)
    return c


LAYERS = {
    "village_far": village_far,
    "village_near": village_near,
    "village_foreground": village_foreground,
    "forest_far": forest_far,
    "forest_near": forest_near,
    "forest_foreground": forest_foreground,
    "mountains_far": mountains_far,
    "mountains_near": mountains_near,
    "mountains_foreground": mountains_foreground,
}


def main():
    out_dir = os.path.join(os.path.dirname(__file__), "..", "assets", "backgrounds")
    os.makedirs(out_dir, exist_ok=True)
    for name, build in LAYERS.items():
        path = os.path.join(out_dir, f"{name}.png")
        write_png(path, OUT_W, OUT_H, render(build()))
        print(f"wrote {os.path.relpath(path)} ({os.path.getsize(path)} bytes)")


if __name__ == "__main__":
    main()
