#!/usr/bin/env python3
"""Generate preset-first cutout body-part PNG variants under
``assets/fighters/human/runtime/``.

Self-contained (Python 3 stdlib only). Produces transparent PNGs sized to
match the existing runtime parts so the Bevy cutout rig can select distinct
preset-driven silhouettes for costume, head-feature, and hair-variant slots
authored in the appearance taxonomy.

Each preset gets its own dedicated torso silhouette (no preset shares the
base ``torso.png``) so preset identity reads at the sprite layer beyond
palette alone. See ``assets/fighters/human/source/README.md`` for the
preset-bundle convention.

Files written:
  * torso_haiduc_coat.png / torso_voinic_tunic.png /
    torso_cioban_cojoc.png / torso_solomonar_robe.png
  * head_moustache.png / head_beard.png
  * hair_alternate.png / hair_ornate.png

Defaults (Clean head, Primary hair) keep using the already-committed
``head.png`` / ``hair.png`` files; only ``CostumeStyle::HaiducCoat`` now
points at its own ``torso_haiduc_coat.png``.

Usage: python3 scripts/generate-hero-preset-parts.py
"""

import os
import struct
import zlib

TORSO_W, TORSO_H = 127, 173
HEAD_W, HEAD_H = 70, 92
HAIR_W, HAIR_H = 74, 92

BLACK = (26, 18, 20, 255)
CREAM = (232, 220, 200, 255)
GOLD = (201, 162, 39, 255)
DEEP_RED = (122, 31, 31, 255)
FOREST = (48, 82, 42, 255)
STORM = (78, 84, 106, 255)
SKIN = (222, 178, 138, 255)
HAIR_DARK = (52, 32, 22, 255)
HAIR_MED = (108, 74, 40, 255)
HAIR_SILVER = (196, 190, 180, 255)


def write_png(path, width, height, rows):
    def chunk(tag, data):
        payload = tag + data
        return (
            struct.pack(">I", len(data))
            + payload
            + struct.pack(">I", zlib.crc32(payload))
        )

    raw = b"".join(b"\x00" + bytes(row) for row in rows)
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as handle:
        handle.write(png)


def blank(width, height):
    return [bytearray(width * 4) for _ in range(height)]


def put(rows, x, y, color):
    x, y = int(x), int(y)
    if 0 <= y < len(rows) and 0 <= x < len(rows[0]) // 4:
        offset = x * 4
        rows[y][offset : offset + 4] = bytes(color)


def rect(rows, x0, y0, x1, y1, color):
    for y in range(int(y0), int(y1) + 1):
        for x in range(int(x0), int(x1) + 1):
            put(rows, x, y, color)


def hline(rows, x0, x1, y, color):
    rect(rows, x0, y, x1, y, color)


def _torso_body(rows, body, top, bottom, left, right):
    rect(rows, left, top, right, bottom, body)
    rect(rows, left, top, left, bottom, BLACK)
    rect(rows, right, top, right, bottom, BLACK)
    hline(rows, left, right, top, BLACK)
    hline(rows, left, right, bottom, BLACK)


def torso_haiduc_coat():
    """Short haiducesc coat: asymmetric front-flap silhouette + hip sash."""
    rows = blank(TORSO_W, TORSO_H)
    cx = TORSO_W // 2
    top, bottom = 14, TORSO_H - 4
    half = TORSO_W // 2 - 14
    _torso_body(rows, FOREST, top, bottom, cx - half, cx + half)
    # asymmetric front lapel: diagonal fold from left shoulder to right hip
    for y in range(top + 3, bottom - 24):
        step = (y - top - 3) // 3
        x = cx - half + 4 + step
        rect(rows, x, y, x + 5, y, DEEP_RED)
    # sash slung across the waist, ends visibly on the right hip
    for y in range(bottom - 30, bottom - 18):
        rect(rows, cx - half + 4, y, cx + half - 6, y, DEEP_RED)
    rect(rows, cx + half - 8, bottom - 18, cx + half - 2, bottom - 4, DEEP_RED)
    # embroidery band at chest
    hline(rows, cx - half + 4, cx + half - 4, top + 20, GOLD)
    return rows


def torso_voinic_tunic():
    """Straight-cut voinicesc tunic: square neckline + wide belt."""
    rows = blank(TORSO_W, TORSO_H)
    cx = TORSO_W // 2
    top, bottom = 12, TORSO_H - 6
    half = TORSO_W // 2 - 10  # slightly broader than base for a fit tunic
    _torso_body(rows, CREAM, top, bottom, cx - half, cx + half)
    # square neckline with red trim
    rect(rows, cx - 12, top + 2, cx + 12, top + 12, DEEP_RED)
    rect(rows, cx - 10, top + 4, cx + 10, top + 12, CREAM)
    # embroidered chest bands
    hline(rows, cx - half + 3, cx + half - 3, top + 22, DEEP_RED)
    hline(rows, cx - half + 3, cx + half - 3, top + 28, DEEP_RED)
    # wide leather belt at waist
    rect(rows, cx - half, bottom - 30, cx + half, bottom - 14, HAIR_DARK)
    rect(rows, cx - 6, bottom - 26, cx + 6, bottom - 18, GOLD)  # buckle
    return rows


def torso_cioban_cojoc():
    """Sheepskin cojoc: broad fleece shoulders and a jagged fleece hem."""
    rows = blank(TORSO_W, TORSO_H)
    cx = TORSO_W // 2
    top, bottom = 10, TORSO_H - 12  # leave room for a jagged hem
    half = TORSO_W // 2 - 6  # cojoc is visibly broader than the base torso
    _torso_body(rows, HAIR_MED, top, bottom, cx - half, cx + half)
    # fleece collar tufts along the top edge
    for x in range(cx - half + 2, cx + half - 1, 4):
        rect(rows, x, top - 4, x + 2, top + 2, CREAM)
    # fleece cuff shoulders (extra opaque bumps outside the base body)
    rect(rows, cx - half - 4, top + 4, cx - half + 2, top + 18, CREAM)
    rect(rows, cx + half - 2, top + 4, cx + half + 4, top + 18, CREAM)
    # embroidery accent across chest
    hline(rows, cx - half + 3, cx + half - 3, top + 24, DEEP_RED)
    # jagged fleece hem: alternating tufts extending below the coat body
    for i, x in enumerate(range(cx - half, cx + half + 1, 4)):
        low = bottom + (6 if i % 2 == 0 else 10)
        rect(rows, x, bottom, x + 3, low, CREAM)
    return rows


def torso_solomonar_robe():
    """Solomonar robe: tall hood collar rising above shoulders + narrow body."""
    rows = blank(TORSO_W, TORSO_H)
    cx = TORSO_W // 2
    top, bottom = 18, TORSO_H - 4
    half = TORSO_W // 2 - 18  # robe is narrower than the base torso
    _torso_body(rows, STORM, top, bottom, cx - half, cx + half)
    # hood: taller pointed collar that rises above the torso top edge
    for dy in range(0, 20):
        width = max(2, 14 - dy // 2)
        rect(rows, cx - width, top - 20 + dy, cx + width, top - 20 + dy, STORM)
    # hood outline
    for dy in range(0, 20):
        width = max(2, 14 - dy // 2)
        put(rows, cx - width, top - 20 + dy, BLACK)
        put(rows, cx + width, top - 20 + dy, BLACK)
    # gold star sigil on the chest
    hline(rows, cx - 4, cx + 4, top + 20, GOLD)
    hline(rows, cx - 4, cx + 4, top + 24, GOLD)
    put(rows, cx, top + 18, GOLD)
    put(rows, cx, top + 26, GOLD)
    # cinched cord at waist
    hline(rows, cx - half, cx + half, bottom - 32, GOLD)
    return rows


def head_with_feature(feature):
    rows = blank(HEAD_W, HEAD_H)
    cx, cy = HEAD_W // 2, HEAD_H // 2
    # face oval
    for y in range(cy - 24, cy + 24):
        for x in range(cx - 18, cx + 18):
            if ((x - cx) ** 2) / 320 + ((y - cy) ** 2) / 700 <= 1.0:
                put(rows, x, y, SKIN)
    # eyes
    put(rows, cx - 6, cy - 4, BLACK)
    put(rows, cx + 6, cy - 4, BLACK)
    if feature == "moustache":
        # wide horizontal moustache with curled tips that extend past the jaw
        rect(rows, cx - 12, cy + 5, cx + 12, cy + 8, HAIR_DARK)
        rect(rows, cx - 14, cy + 6, cx - 12, cy + 10, HAIR_DARK)
        rect(rows, cx + 12, cy + 6, cx + 14, cy + 10, HAIR_DARK)
    elif feature == "beard":
        # full beard that extends below the jaw, widening the head silhouette
        for y in range(cy + 4, cy + 26):
            width = 14 - max(0, y - cy - 18)
            rect(rows, cx - width, y, cx + width, y, HAIR_DARK)
        # sideburns joining the beard along the jaw line
        rect(rows, cx - 16, cy + 2, cx - 12, cy + 14, HAIR_DARK)
        rect(rows, cx + 12, cy + 2, cx + 16, cy + 14, HAIR_DARK)
    return rows


def hair_variant(variant):
    rows = blank(HAIR_W, HAIR_H)
    cx = HAIR_W // 2
    if variant == "alternate":
        # short cropped cap: compact silhouette sitting high on the head
        rect(rows, cx - 18, 22, cx + 18, 38, HAIR_MED)
        # slight side-tufts above the ears, no long fall
        rect(rows, cx - 20, 30, cx - 18, 44, HAIR_MED)
        rect(rows, cx + 18, 30, cx + 20, 44, HAIR_MED)
    else:  # ornate
        # long solomonar mane: crown cap + long fall past the shoulders
        rect(rows, cx - 26, 14, cx + 26, 44, HAIR_SILVER)
        # gold ornament band across the crown
        hline(rows, cx - 22, cx + 22, 24, GOLD)
        hline(rows, cx - 22, cx + 22, 32, GOLD)
        # long side falls reaching down past the head
        rect(rows, cx - 26, 44, cx - 18, HAIR_H - 2, HAIR_SILVER)
        rect(rows, cx + 18, 44, cx + 26, HAIR_H - 2, HAIR_SILVER)
        # center point trailing down the back
        rect(rows, cx - 6, 44, cx + 6, HAIR_H - 6, HAIR_SILVER)
    return rows


def main():
    out_dir = os.path.join(
        os.path.dirname(__file__), "..", "assets", "fighters", "human", "runtime"
    )
    os.makedirs(out_dir, exist_ok=True)
    outputs = {
        "torso_haiduc_coat.png": (TORSO_W, TORSO_H, torso_haiduc_coat()),
        "torso_voinic_tunic.png": (TORSO_W, TORSO_H, torso_voinic_tunic()),
        "torso_cioban_cojoc.png": (TORSO_W, TORSO_H, torso_cioban_cojoc()),
        "torso_solomonar_robe.png": (TORSO_W, TORSO_H, torso_solomonar_robe()),
        "head_moustache.png": (HEAD_W, HEAD_H, head_with_feature("moustache")),
        "head_beard.png": (HEAD_W, HEAD_H, head_with_feature("beard")),
        "hair_alternate.png": (HAIR_W, HAIR_H, hair_variant("alternate")),
        "hair_ornate.png": (HAIR_W, HAIR_H, hair_variant("ornate")),
    }
    for name, (w, h, rows) in outputs.items():
        path = os.path.join(out_dir, name)
        write_png(path, w, h, rows)
        print(f"wrote {os.path.relpath(path)} ({os.path.getsize(path)} bytes)")


if __name__ == "__main__":
    main()
