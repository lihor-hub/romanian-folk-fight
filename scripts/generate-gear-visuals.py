#!/usr/bin/env python3
"""Generate transparent equipment overlay sprites under assets/gear/.

Each image is a 128x128 transparent layer aligned to the fighter frame. The
art is self-generated placeholder linework in the Romanian Folk Fight palette
and is dedicated to CC0 like the other placeholder assets.
"""

import os
import struct
import zlib

SIZE = 128
TRANSPARENT = (0, 0, 0, 0)
BLACK = (26, 18, 20, 255)
CREAM = (232, 220, 200, 255)
DEEP_RED = (122, 31, 31, 255)
GOLD = (201, 162, 39, 255)
WOOD = (120, 82, 48, 255)
STEEL = (178, 184, 188, 255)
LEATHER = (92, 58, 36, 255)
WOOL = (205, 190, 166, 255)


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

    def line(self, x0, y0, x1, y1, color, width=2):
        dx = abs(x1 - x0)
        dy = -abs(y1 - y0)
        sx = 1 if x0 < x1 else -1
        sy = 1 if y0 < y1 else -1
        err = dx + dy
        x, y = x0, y0
        while True:
            half = width // 2
            self.rect(x - half, y - half, x + half, y + half, color)
            if x == x1 and y == y1:
                break
            e2 = 2 * err
            if e2 >= dy:
                err += dy
                x += sx
            if e2 <= dx:
                err += dx
                y += sy

    def disc(self, cx, cy, r, color):
        for y in range(cy - r, cy + r + 1):
            for x in range(cx - r, cx + r + 1):
                if (x - cx) ** 2 + (y - cy) ** 2 <= r * r:
                    self.px(x, y, color)


def weapon(kind, primary, accent=GOLD):
    c = Canvas()
    if kind == "club":
        c.line(78, 72, 100, 36, WOOD, 5)
        c.disc(102, 32, 8, primary)
    elif kind == "axe":
        c.line(80, 74, 102, 34, WOOD, 4)
        c.rect(96, 30, 113, 42, STEEL)
        c.rect(101, 25, 112, 47, STEEL)
    elif kind == "mace":
        c.line(78, 73, 101, 33, WOOD, 4)
        c.disc(104, 30, 9, primary)
        for dx, dy in [(-8, 0), (8, 0), (0, -8), (0, 8)]:
            c.disc(104 + dx, 30 + dy, 3, accent)
    else:
        c.line(78, 73, 109, 25, STEEL, 3)
        c.line(77, 75, 87, 65, accent, 4)
        c.disc(75, 77, 3, BLACK)
    return c


def shield(primary, accent):
    c = Canvas()
    c.disc(43, 70, 18, primary)
    c.disc(43, 70, 13, accent)
    c.disc(43, 70, 5, BLACK)
    return c


def torso(primary, accent):
    c = Canvas()
    c.rect(48, 52, 79, 89, primary)
    c.rect(48, 70, 79, 75, accent)
    c.line(55, 56, 72, 86, accent, 2)
    c.line(72, 56, 55, 86, accent, 2)
    return c


def head(primary, accent):
    c = Canvas()
    c.rect(51, 23, 77, 34, primary)
    c.rect(47, 34, 81, 39, accent)
    c.disc(64, 22, 4, accent)
    return c


def feet(primary, accent):
    c = Canvas()
    c.rect(46, 106, 61, 116, primary)
    c.rect(67, 106, 82, 116, primary)
    c.rect(42, 114, 61, 119, accent)
    c.rect(67, 114, 86, 119, accent)
    return c


ITEMS = {
    "bata_ciobaneasca": weapon("club", WOOD),
    "topor_de_padurar": weapon("axe", STEEL),
    "palos": weapon("sword", STEEL),
    "buzdugan_cu_trei_peceti": weapon("mace", STEEL),
    "scut_de_lemn": shield(WOOD, DEEP_RED),
    "scut_ferecat": shield(STEEL, GOLD),
    "ie_descantata": torso(CREAM, DEEP_RED),
    "cojoc_gros": torso(WOOL, LEATHER),
    "camasa_de_zale": torso(STEEL, BLACK),
    "caciula_de_oaie": head(WOOL, CREAM),
    "coif_de_ostean": head(STEEL, GOLD),
    "opinci_iuti": feet(LEATHER, DEEP_RED),
    "cizme_de_voinic": feet(BLACK, GOLD),
}


def main():
    out_dir = os.path.join(os.path.dirname(__file__), "..", "assets", "gear")
    os.makedirs(out_dir, exist_ok=True)
    for name, canvas in ITEMS.items():
        path = os.path.join(out_dir, f"{name}.png")
        write_png(path, SIZE, SIZE, canvas.rows)
        print(f"wrote {os.path.relpath(path)} ({os.path.getsize(path)} bytes)")


if __name__ == "__main__":
    main()
