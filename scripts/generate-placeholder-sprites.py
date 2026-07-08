#!/usr/bin/env python3
"""Generate the placeholder fighter sprite sheets under assets/sprites/.

Self-contained (Python 3 stdlib only). Each sheet is a 4x4 grid of 128x128
frames drawn as chunky pixel art (32x32 logical pixels, scaled x4), following
docs/art-direction.md: side-view fighters facing right, palette of deep red,
black, cream, and gold with one accent color per creature.

Frame layout (row-major, matching src/arena/animation.rs):
  row 0: idle   frames 0-3
  row 1: attack frames 4-7
  row 2: hurt   frames 8-9, KO frames 10-11
  row 3: step forward frames 12-13, step back frames 14-15

Usage: python3 scripts/generate-placeholder-sprites.py

The output PNGs are committed; re-run only when changing the art. All output
is self-generated placeholder art, dedicated to the public domain (CC0); see
assets/CREDITS.md.
"""

import os
import struct
import zlib

LOGICAL = 32  # logical pixels per frame side
SCALE = 4  # screen pixels per logical pixel
FRAME = LOGICAL * SCALE  # 128
COLS, ROWS = 4, 4
SHEET_W, SHEET_H = COLS * FRAME, ROWS * FRAME

# Art-direction palette (docs/art-direction.md).
DEEP_RED = (122, 31, 31, 255)
BLACK = (26, 18, 20, 255)
CREAM = (232, 220, 200, 255)
GOLD = (201, 162, 39, 255)
SHADOW = (0, 0, 0, 70)


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


class Frame:
    """One 32x32 logical-pixel frame."""

    def __init__(self):
        self.grid = [[None] * LOGICAL for _ in range(LOGICAL)]

    def px(self, x, y, color):
        x, y = int(x), int(y)
        if 0 <= x < LOGICAL and 0 <= y < LOGICAL:
            self.grid[y][x] = color

    def rect(self, x0, y0, x1, y1, color):
        for y in range(int(y0), int(y1) + 1):
            for x in range(int(x0), int(x1) + 1):
                self.px(x, y, color)

    def disc(self, cx, cy, r, color):
        for y in range(int(cy - r), int(cy + r) + 1):
            for x in range(int(cx - r), int(cx + r) + 1):
                if (x - cx) ** 2 + (y - cy) ** 2 <= r * r + r * 0.5:
                    self.px(x, y, color)

    def hline(self, x0, x1, y, color):
        self.rect(x0, y, x1, y, color)


def tint(color, factor, alpha=255):
    return tuple(min(255, int(c * factor)) for c in color[:3]) + (alpha,)


GROUND_Y = 29  # logical y of the ground line (feet)


def draw_standing(f, spec, bob=0, arm=0.0, lean=0, flash=False):
    """Draw the fighter standing, facing right.

    bob: vertical idle offset. arm: 0..1 attack-arm extension (weapon out).
    lean: horizontal hurt lean (pixels backwards). flash: hurt highlight.
    """
    body = tint(spec["body"], 1.35) if flash else spec["body"]
    accent = tint(spec["accent"], 1.35) if flash else spec["accent"]
    bulky = spec.get("bulky", False)
    slim = spec.get("slim", False)

    cx = 14 - lean  # body center x
    w = 5 if bulky else (2 if slim else 3)  # torso half width
    top = 12 + bob + (1 if bulky else 0)
    hip = 22 + bob

    # ground shadow
    f.hline(cx - w - 2, cx + w + 4, GROUND_Y + 1, SHADOW)
    # legs
    leg = spec.get("leg", BLACK)
    f.rect(cx - w + 1, hip, cx - w + 2, GROUND_Y, leg)
    f.rect(cx + w - 2, hip, cx + w - 1, GROUND_Y, leg)
    # torso
    f.rect(cx - w, top, cx + w, hip, body)
    # sash / belt accent
    f.rect(cx - w, hip - 2, cx + w, hip - 1, accent)
    # chest trim (folk embroidery hint)
    f.hline(cx - w + 1, cx + w - 1, top + 2, accent)

    # rear arm (static, at side)
    f.rect(cx - w - 1, top + 2, cx - w, top + 7, body)

    # front arm + weapon: extends forward with `arm`
    ax = cx + w + int(arm * 6)
    ay = top + 3 - int(arm * 2)
    f.rect(cx + w, top + 2, ax, ay + 2, body)
    weapon = spec.get("weapon")
    if weapon == "club":
        f.rect(ax + 1, ay - 2, ax + 2, ay + 3, tint(BLACK, 2.2))
        f.disc(ax + 2, ay - 3, 1.6, tint(BLACK, 2.2))
    elif weapon == "sword":
        f.rect(ax + 1, ay - int(2 + arm * 3), ax + 1, ay + 2, CREAM)
        f.px(ax + 1, ay - int(3 + arm * 3), GOLD)
    elif weapon == "staff":
        f.rect(ax + 1, ay - 5, ax + 1, ay + 6, tint(BLACK, 2.2))
        f.px(ax + 1, ay - 6, GOLD)
    else:  # fist
        f.rect(ax, ay, ax + 1, ay + 1, tint(body, 1.2))

    draw_head(f, spec, cx, top, lean, flash)


def draw_head(f, spec, cx, top, lean, flash):
    skin = tint(spec["skin"], 1.35) if flash else spec["skin"]
    hy = top - 4  # head center y
    hx = cx + 1
    heads = spec.get("heads", 1)
    if heads == 3:  # balaur: one central head, two smaller behind
        f.disc(hx - 4, hy - 2, 2.0, tint(skin, 0.8))
        f.disc(hx - 2, hy - 4, 2.0, tint(skin, 0.9))
    f.disc(hx, hy, 3.0, skin)
    # eye
    f.px(hx + 2, hy - 1, BLACK if not spec.get("gold_eye") else GOLD)
    style = spec.get("head", "round")
    if style == "hood":
        f.rect(hx - 4, hy - 4, hx + 2, hy - 3, spec["body"])
        f.rect(hx - 4, hy - 2, hx - 3, hy + 2, spec["body"])
    elif style == "horns":
        f.rect(hx - 2, hy - 6, hx - 2, hy - 4, GOLD)
        f.rect(hx + 2, hy - 6, hx + 2, hy - 4, GOLD)
    elif style == "hat":
        f.rect(hx - 3, hy - 5, hx + 2, hy - 4, BLACK)
    elif style == "ears":
        f.px(hx - 2, hy - 4, spec["skin"])
        f.px(hx + 1, hy - 4, spec["skin"])
        # snout
        f.rect(hx + 3, hy, hx + 5, hy + 1, tint(spec["skin"], 0.85))
    elif style == "crown":
        f.hline(hx - 3, hx + 2, hy - 5, GOLD)
        f.px(hx - 3, hy - 6, GOLD)
        f.px(hx, hy - 6, GOLD)
        f.px(hx + 2, hy - 6, GOLD)
    elif style == "hair":
        f.rect(hx - 4, hy - 4, hx + 1, hy - 3, tint(spec["body"], 0.7))
        f.rect(hx - 5, hy - 3, hx - 4, hy + 3, tint(spec["body"], 0.7))
    if spec.get("tail"):
        ty = GROUND_Y - 4
        f.rect(cx - 9 + lean, ty, cx - 5 + lean, ty + 1, spec["skin"])
        f.px(cx - 10 + lean, ty - 1, spec["skin"])


def draw_kneeling(f, spec):
    """KO frame 1: collapsed to the knees."""
    body, cx = spec["body"], 14
    w = 5 if spec.get("bulky") else 3
    f.hline(cx - w - 2, cx + w + 4, GROUND_Y + 1, SHADOW)
    f.rect(cx - w, 20, cx + w, GROUND_Y, body)
    f.rect(cx - w, 24, cx + w, 25, tint(spec["accent"], 0.8))
    draw_head(f, spec, cx + 1, 24, 0, False)


def draw_lying(f, spec):
    """KO frame 2: flat on the ground, head to the left."""
    body, skin = tint(spec["body"], 0.8), tint(spec["skin"], 0.8)
    f.hline(4, 28, GROUND_Y + 1, SHADOW)
    f.rect(10, GROUND_Y - 4, 24, GROUND_Y, body)
    f.rect(11, GROUND_Y - 5, 22, GROUND_Y - 5, tint(spec["accent"], 0.7))
    f.disc(7, GROUND_Y - 2, 3.0, skin)
    f.px(7, GROUND_Y - 3, BLACK)  # closed eye


def draw_footwork(f, spec, forward=True, phase=0):
    """Presentation-only step frames; the engine supplies the real x tween."""
    draw_standing(
        f,
        spec,
        bob=phase,
        lean=-1 if forward else 1,
        arm=0.15 if forward else 0.0,
    )


def frames_for(spec):
    """The 16 frames of one fighter, in atlas order."""
    frames = []
    for bob in (0, 1, 2, 1):  # idle
        f = Frame()
        draw_standing(f, spec, bob=bob)
        frames.append(f)
    for arm in (-0.3, 0.3, 1.0, 0.5):  # attack: windup, extend, strike, recover
        f = Frame()
        draw_standing(f, spec, arm=max(arm, 0.0), lean=-1 if arm < 0 else 0)
        frames.append(f)
    for lean, flash in ((2, True), (3, False)):  # hurt
        f = Frame()
        draw_standing(f, spec, lean=lean, flash=flash)
        frames.append(f)
    f = Frame()
    draw_kneeling(f, spec)
    frames.append(f)
    f = Frame()
    draw_lying(f, spec)
    frames.append(f)
    for phase in (1, 0):  # step forward
        f = Frame()
        draw_footwork(f, spec, forward=True, phase=phase)
        frames.append(f)
    for phase in (1, 0):  # step back
        f = Frame()
        draw_footwork(f, spec, forward=False, phase=phase)
        frames.append(f)
    return frames


def render_sheet(path, spec):
    rows = [bytearray(SHEET_W * 4) for _ in range(SHEET_H)]
    for index, frame in enumerate(frames_for(spec)):
        fx, fy = (index % COLS) * FRAME, (index // COLS) * FRAME
        for ly in range(LOGICAL):
            for lx in range(LOGICAL):
                color = frame.grid[ly][lx]
                if color is None:
                    continue
                for sy in range(SCALE):
                    row = rows[fy + ly * SCALE + sy]
                    for sx in range(SCALE):
                        offset = (fx + lx * SCALE + sx) * 4
                        row[offset : offset + 4] = bytes(color)
    write_png(path, SHEET_W, SHEET_H, rows)


# One spec per fighter: palette + shape variation of the same base rig.
SPECS = {
    # The player: Făt-Frumos — cream shirt, red sash, black hat, sword.
    "player": {
        "body": CREAM,
        "accent": DEEP_RED,
        "skin": (222, 178, 138, 255),
        "head": "hat",
        "weapon": "sword",
    },
    "hot_de_codru": {
        "body": (74, 66, 42, 255),  # forest-brown cloak
        "accent": (120, 104, 60, 255),
        "skin": (200, 160, 120, 255),
        "head": "hood",
        "weapon": "club",
        "slim": True,
    },
    "strigoi": {
        "body": (150, 150, 158, 255),  # pale shroud
        "accent": (94, 94, 104, 255),
        "skin": (208, 210, 214, 255),
        "slim": True,
    },
    "varcolac": {
        "body": (96, 78, 58, 255),  # fur
        "accent": (60, 48, 36, 255),
        "skin": (128, 104, 78, 255),
        "head": "ears",
        "tail": True,
    },
    "capcaun": {
        "body": (156, 122, 64, 255),  # ochre hide
        "accent": DEEP_RED,
        "skin": (176, 140, 84, 255),
        "weapon": "club",
        "bulky": True,
    },
    "muma_padurii": {
        "body": (52, 78, 46, 255),  # mossy green
        "accent": GOLD,
        "skin": (130, 140, 100, 255),
        "head": "hair",
        "gold_eye": True,
    },
    "iele": {
        "body": (236, 230, 214, 255),  # ethereal white dress
        "accent": GOLD,
        "skin": (240, 226, 208, 255),
        "slim": True,
        "gold_eye": True,
    },
    "solomonar": {
        "body": (70, 78, 96, 255),  # storm-gray robe
        "accent": (150, 160, 180, 255),
        "skin": (198, 172, 140, 255),
        "head": "hood",
        "weapon": "staff",
    },
    "balaur": {
        "body": (58, 96, 58, 255),  # green scales
        "accent": (120, 160, 90, 255),
        "skin": (84, 128, 74, 255),
        "heads": 3,
        "tail": True,
        "bulky": True,
    },
    "zmeu": {
        "body": DEEP_RED,
        "accent": BLACK,
        "skin": (150, 52, 44, 255),
        "head": "horns",
        "weapon": "sword",
    },
    "zmeul_zmeilor": {
        "body": (74, 22, 24, 255),  # near-black red
        "accent": GOLD,
        "skin": (120, 40, 38, 255),
        "head": "crown",
        "weapon": "club",
        "bulky": True,
    },
}


def main():
    out_dir = os.path.join(os.path.dirname(__file__), "..", "assets", "sprites")
    os.makedirs(out_dir, exist_ok=True)
    for name, spec in SPECS.items():
        path = os.path.join(out_dir, f"{name}.png")
        render_sheet(path, spec)
        print(f"wrote {os.path.relpath(path)} ({os.path.getsize(path)} bytes)")


if __name__ == "__main__":
    main()
