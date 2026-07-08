#!/usr/bin/env python3
"""Generate small folk-themed shop UI icons under assets/ui/.

The icons are self-generated placeholder art in the project palette and are
dedicated to CC0 like the other generated assets.
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


def coin():
    c = Canvas()
    c.disc(16, 16, 11, GOLD)
    c.disc(16, 16, 8, (226, 184, 58, 255))
    c.line(12, 11, 20, 21, BLACK, 2)
    c.line(20, 11, 12, 21, BLACK, 2)
    return c


def weapon():
    c = Canvas()
    c.line(9, 24, 24, 7, STEEL, 3)
    c.line(8, 25, 14, 19, GOLD, 3)
    c.disc(8, 25, 2, BLACK)
    return c


def shield():
    c = Canvas()
    c.disc(16, 16, 11, DEEP_RED)
    c.rect(14, 6, 18, 26, GOLD)
    c.rect(7, 14, 25, 18, GOLD)
    c.disc(16, 16, 4, BLACK)
    return c


def torso():
    c = Canvas()
    c.rect(9, 9, 22, 25, CREAM)
    c.rect(9, 17, 22, 20, DEEP_RED)
    c.line(11, 11, 20, 24, GOLD, 1)
    c.line(20, 11, 11, 24, GOLD, 1)
    return c


def head():
    c = Canvas()
    c.disc(16, 17, 9, CREAM)
    c.rect(8, 8, 23, 11, BLACK)
    c.rect(10, 5, 21, 8, WOOD)
    return c


def feet():
    c = Canvas()
    c.rect(7, 17, 14, 24, LEATHER)
    c.rect(18, 17, 25, 24, LEATHER)
    c.rect(5, 23, 14, 26, DEEP_RED)
    c.rect(18, 23, 27, 26, DEEP_RED)
    return c


ICONS = {
    "icon_coin": coin,
    "icon_weapon": weapon,
    "icon_shield": shield,
    "icon_torso": torso,
    "icon_head": head,
    "icon_feet": feet,
}


def main():
    out_dir = os.path.join(os.path.dirname(__file__), "..", "assets", "ui")
    os.makedirs(out_dir, exist_ok=True)
    for name, factory in ICONS.items():
        canvas = factory()
        path = os.path.join(out_dir, f"{name}.png")
        write_png(path, canvas.rows)
        print(f"wrote {os.path.relpath(path)} ({os.path.getsize(path)} bytes)")


if __name__ == "__main__":
    main()
