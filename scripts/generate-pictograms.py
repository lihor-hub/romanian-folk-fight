#!/usr/bin/env python3
"""Generate the combat action pictograms under assets/ui/pictograms/.

Same pipeline as scripts/generate-shop-icons.py: self-generated placeholder
art in the project's folk palette, dedicated to CC0 like the other generated
assets. One 32x32 PNG per combat action, named by the action descriptor's
stable `pictogram_id` (see `src/combat/actions.rs`), so the palette can load
`ui/pictograms/<id>.png` directly.
"""

import os
import struct
import zlib

SIZE = 32
TRANSPARENT = (0, 0, 0, 0)
BLACK = (26, 18, 20, 255)
CREAM = (232, 220, 200, 255)
DEEP_RED = (122, 31, 31, 255)
GOLD = (201, 162, 39, 255)
WOOD = (120, 82, 48, 255)
STEEL = (178, 184, 188, 255)
LEATHER = (92, 58, 36, 255)


def write_png(path, pixels):
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
        + chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as handle:
        handle.write(png)


class Canvas:
    def __init__(self):
        self.rows = [bytearray(TRANSPARENT * SIZE) for _ in range(SIZE)]

    def px(self, x, y, color):
        if 0 <= x < SIZE and 0 <= y < SIZE:
            offset = x * 4
            self.rows[y][offset : offset + 4] = bytes(color)

    def rect(self, x0, y0, x1, y1, color):
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                self.px(x, y, color)

    def disc(self, cx, cy, r, color):
        for y in range(cy - r, cy + r + 1):
            for x in range(cx - r, cx + r + 1):
                if (x - cx) ** 2 + (y - cy) ** 2 <= r * r:
                    self.px(x, y, color)

    def line(self, x0, y0, x1, y1, color, width=1):
        dx = abs(x1 - x0)
        dy = -abs(y1 - y0)
        sx = 1 if x0 < x1 else -1
        sy = 1 if y0 < y1 else -1
        err = dx + dy
        x, y = x0, y0
        while True:
            self.rect(x - width // 2, y - width // 2, x + width // 2, y + width // 2, color)
            if x == x1 and y == y1:
                break
            e2 = 2 * err
            if e2 >= dy:
                err += dy
                x += sx
            if e2 <= dx:
                err += dx
                y += sy

    def chevron_right(self, x, y0, y1, color, width=2):
        mid = (y0 + y1) // 2
        self.line(x, y0, x + (mid - y0), mid, color, width)
        self.line(x + (mid - y0), mid, x, y1, color, width)


def quick_strike():
    # Two parallel motion slashes: the fast, glancing cut.
    c = Canvas()
    c.line(6, 24, 20, 8, STEEL, 3)
    c.line(14, 26, 26, 12, CREAM, 2)
    c.line(5, 25, 9, 21, GOLD, 2)
    return c


def normal_strike():
    # A straight punch: forearm driving a fist dead ahead, impact ticks.
    c = Canvas()
    c.rect(4, 13, 17, 19, LEATHER)
    c.disc(20, 16, 5, CREAM)
    c.rect(22, 12, 24, 20, GOLD)
    c.line(27, 10, 30, 8, GOLD, 1)
    c.line(28, 16, 31, 16, GOLD, 1)
    c.line(27, 22, 30, 24, GOLD, 1)
    return c


def heavy_strike():
    # A club swung down along an arc into the ground.
    c = Canvas()
    c.line(7, 6, 20, 19, WOOD, 4)
    c.disc(22, 21, 4, WOOD)
    c.disc(23, 22, 2, LEATHER)
    c.line(6, 14, 8, 20, GOLD, 1)
    c.line(8, 20, 13, 25, GOLD, 1)
    c.rect(16, 28, 29, 29, BLACK)
    return c


def block():
    # A pointed shield with the gold cross of the shop's round one.
    c = Canvas()
    c.rect(8, 6, 24, 18, DEEP_RED)
    c.line(8, 18, 16, 28, DEEP_RED, 3)
    c.line(24, 18, 16, 28, DEEP_RED, 3)
    c.rect(15, 6, 17, 26, GOLD)
    c.rect(8, 11, 24, 13, GOLD)
    return c


def rest():
    # A steaming bowl: recovery by the fire.
    c = Canvas()
    c.rect(8, 19, 24, 24, WOOD)
    c.rect(10, 25, 22, 26, LEATHER)
    c.rect(7, 18, 25, 19, GOLD)
    c.line(12, 15, 14, 9, CREAM, 1)
    c.line(16, 16, 18, 8, CREAM, 1)
    c.line(20, 15, 22, 10, CREAM, 1)
    return c


def step_forward():
    # A footprint stepping into a rightward arrow.
    c = Canvas()
    c.disc(9, 18, 4, LEATHER)
    c.disc(12, 12, 2, LEATHER)
    c.disc(8, 11, 1, LEATHER)
    c.line(16, 22, 26, 22, GOLD, 2)
    c.line(23, 18, 27, 22, GOLD, 2)
    c.line(23, 26, 27, 22, GOLD, 2)
    return c


def step_back():
    # The mirrored footprint, arrow pointing back the other way.
    c = Canvas()
    c.disc(23, 18, 4, LEATHER)
    c.disc(20, 12, 2, LEATHER)
    c.disc(24, 11, 1, LEATHER)
    c.line(6, 22, 16, 22, GOLD, 2)
    c.line(9, 18, 5, 22, GOLD, 2)
    c.line(9, 26, 5, 22, GOLD, 2)
    return c


def leap_forward():
    # A double chevron: bounding two bands forward at once.
    c = Canvas()
    c.chevron_right(8, 8, 24, GOLD, 3)
    c.chevron_right(17, 8, 24, CREAM, 3)
    return c


PICTOGRAMS = {
    "quick-strike": quick_strike,
    "normal-strike": normal_strike,
    "heavy-strike": heavy_strike,
    "block": block,
    "rest": rest,
    "step-forward": step_forward,
    "step-back": step_back,
    "leap-forward": leap_forward,
}


def main():
    out_dir = os.path.join(
        os.path.dirname(__file__), "..", "assets", "ui", "pictograms"
    )
    os.makedirs(out_dir, exist_ok=True)
    for name, factory in PICTOGRAMS.items():
        canvas = factory()
        path = os.path.join(out_dir, f"{name}.png")
        write_png(path, canvas.rows)
        print(f"wrote {os.path.relpath(path)} ({os.path.getsize(path)} bytes)")


if __name__ == "__main__":
    main()
