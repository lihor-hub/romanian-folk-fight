#!/usr/bin/env python3
"""Derive the known-good human's hybrid material channels from its albedos.

The source albedo is authoritative for dimensions and alpha.  This generator
never redraws a part: masks use deterministic colour classification, normals
use a shallow luminance gradient, and shadows use a restrained luminance
profile.  Run with ``--check`` to prove checked-in PNGs match the generator.
"""

from __future__ import annotations

import argparse
import io
import math
from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parents[1]
RUNTIME = ROOT / "assets/fighters/human/runtime"
MASK_WEIGHT = 200

# Every albedo rendered by the complete known-good human rig.  Values describe
# the positional mask channels (R, G, B); alpha always remains the silhouette.
MASK_REGIONS: dict[str, tuple[str, ...]] = {
    "hair": ("hair",),
    "head": ("skin",),
    "torso": ("cloth", "embroidery"),
    "upper_arm_back": ("cloth", "embroidery"),
    "forearm_back": ("cloth", "embroidery"),
    "hand_back": ("skin",),
    "upper_arm_front": ("cloth", "embroidery"),
    "forearm_front": ("cloth", "embroidery"),
    "hand_front": ("skin",),
    "thigh_back": ("cloth", "embroidery"),
    "shin_back": ("cloth", "embroidery", "leather"),
    "foot_back": ("leather", "cloth"),
    "thigh_front": ("cloth", "embroidery"),
    "shin_front": ("cloth", "embroidery", "leather"),
    "foot_front": ("leather", "cloth"),
}


def luma(red: int, green: int, blue: int) -> float:
    return (0.2126 * red) + (0.7152 * green) + (0.0722 * blue)


def classify(
    red: int,
    green: int,
    blue: int,
    alpha: int,
    regions: tuple[str, ...],
) -> str | None:
    """Classify one authored albedo texel without changing its coverage."""
    if alpha < 128 or luma(red, green, blue) < 38:
        return None

    high = max(red, green, blue)
    low = min(red, green, blue)
    saturation = 0.0 if high == 0 else (high - low) / high

    if regions == ("hair",):
        if not (red > 115 and red > green * 1.8):
            return "hair"
        return None

    if regions == ("skin",):
        if red > 74 and red > green * 1.04 and green > blue * 1.04:
            return "skin"
        return None

    cloth = red > 105 and green > 72 and blue > 42 and saturation < 0.47
    if cloth:
        return "cloth"

    if regions[0] == "leather":
        if red > 48 and green > 20 and blue < 92 and red > blue * 1.35:
            return "leather"
        return None

    vivid_red_or_gold = (
        red > 90
        and blue < 68
        and saturation > 0.48
        and red > green * 1.18
    )
    if vivid_red_or_gold:
        return "embroidery"
    if "leather" in regions and red > 48 and green > 20 and blue < 82:
        return "leather"
    return None


def build_mask(albedo: Image.Image, regions: tuple[str, ...]) -> Image.Image:
    source = albedo.load()
    output = Image.new("RGBA", albedo.size)
    pixels = output.load()
    for y in range(albedo.height):
        for x in range(albedo.width):
            red, green, blue, alpha = source[x, y]
            semantic = classify(red, green, blue, alpha, regions)
            channels = [0, 0, 0]
            if semantic in regions:
                # Keep recoloring restrained and stay outside the asset
                # validator's intentional pure-green chroma-key tolerance.
                channels[regions.index(semantic)] = MASK_WEIGHT
            pixels[x, y] = (*channels, alpha)
    return output


def build_normal(albedo: Image.Image) -> Image.Image:
    source = albedo.load()
    output = Image.new("RGBA", albedo.size)
    pixels = output.load()

    def height_at(x: int, y: int, fallback: float) -> float:
        red, green, blue, alpha = source[x, y]
        if alpha == 0:
            return fallback
        return luma(red, green, blue) / 255.0

    for y in range(albedo.height):
        for x in range(albedo.width):
            red, green, blue, alpha = source[x, y]
            center = luma(red, green, blue) / 255.0
            left = height_at(max(0, x - 1), y, center)
            right = height_at(min(albedo.width - 1, x + 1), y, center)
            up = height_at(x, max(0, y - 1), center)
            down = height_at(x, min(albedo.height - 1, y + 1), center)
            # Image-space y increases downward; tangent-space y points upward.
            nx = -(right - left) * 0.18
            ny = (down - up) * 0.18
            nz = 1.0
            length = math.sqrt(nx * nx + ny * ny + nz * nz)
            encoded = tuple(
                round((component / length * 0.5 + 0.5) * 255)
                for component in (nx, ny, nz)
            )
            pixels[x, y] = (*encoded, alpha)
    return output


def build_shadow(albedo: Image.Image) -> Image.Image:
    source = albedo.load()
    output = Image.new("RGBA", albedo.size)
    pixels = output.load()
    for y in range(albedo.height):
        for x in range(albedo.width):
            red, green, blue, alpha = source[x, y]
            darkness = max(0.0, min(1.0, (154.0 - luma(red, green, blue)) / 154.0))
            # The shader additionally caps shadow strength at 0.35.  Keeping
            # the authored signal in 224..255 makes the combined result soft.
            value = 255 - round(31 * darkness)
            pixels[x, y] = (value, value, value, alpha)
    return output


def png_bytes(image: Image.Image) -> bytes:
    buffer = io.BytesIO()
    image.save(buffer, format="PNG", optimize=False, compress_level=9)
    return buffer.getvalue()


def generated_channels(stem: str) -> dict[str, Image.Image]:
    albedo = Image.open(RUNTIME / f"{stem}.png").convert("RGBA")
    return {
        "mask": build_mask(albedo, MASK_REGIONS[stem]),
        "normal": build_normal(albedo),
        "shadow": build_shadow(albedo),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify checked-in channels are byte-identical to regenerated PNGs",
    )
    args = parser.parse_args()

    mismatches: list[str] = []
    for stem in MASK_REGIONS:
        for suffix, image in generated_channels(stem).items():
            path = RUNTIME / f"{stem}_{suffix}.png"
            expected = png_bytes(image)
            if args.check:
                if not path.exists() or path.read_bytes() != expected:
                    mismatches.append(path.relative_to(ROOT).as_posix())
            else:
                path.write_bytes(expected)

    if mismatches:
        for path in mismatches:
            print(f"stale or missing: {path}")
        return 1
    if args.check:
        print(f"ok ({len(MASK_REGIONS) * 3} deterministic human material channels)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
