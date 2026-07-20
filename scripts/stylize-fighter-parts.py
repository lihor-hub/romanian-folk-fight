#!/usr/bin/env python3
"""Apply the phase-4 folk stylization pass to fighter runtime part art.

One deterministic, idempotent surface treatment over the human and strigoi
runtime albedos (docs/combat-redesign-proposal.md section 5): a uniform
dark-walnut outline ring at every silhouette edge, one flat folk-black ink
over interior near-black clusters, flattened specular highlights, and
restrained tone quantization that merges micro-shading into painted 2-tone
clusters while preserving each part's hue identity.  Gear albedos receive
only the outline/ink harmonization; their painted material reads stay
untouched.

The stylization never redraws a silhouette: canvas dimensions and the alpha
channel are preserved byte-for-byte.  After restyling an albedo this script
regenerates its mask/normal/shadow companions with the exact builder
functions of the owning generator (``generate-human-material-channels.py``
for the legacy root set, ``extract-romanian-paper-doll-v1.py`` for the
preset/shared/gear sets), so technical maps never desync from their albedo.
The strigoi set has no companion channels.

This pass runs *after* extraction: the extractor's ``--check`` over albedo
bytes is intentionally superseded for stylized parts, while its channel
derivation and manifests remain authoritative.  Run with ``--check`` to
prove the checked-in PNGs match this stylizer (which also proves
idempotence: a stylized albedo is a fixed point of the transform).
"""

from __future__ import annotations

import argparse
import colorsys
import importlib.util
import io
import sys
from pathlib import Path

from PIL import Image

# Loading sibling generators must not litter scripts/__pycache__.
sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
HUMAN_RUNTIME = ROOT / "assets/fighters/human/runtime"
STRIGOI_RUNTIME = ROOT / "assets/fighters/strigoi/runtime"

# Theme WALNUT (src/theme) darkened below the mask classifiers' luma floor
# (34), so uniform outline ink never enters a recolorable semantic region.
OUTLINE_RGB = (48, 26, 16)
# Theme NIGHT_BLACK: interior near-black clusters (hair mass, boot leather,
# painted interior lines) flatten to this single folk-black ink instead of
# the walnut rim, keeping raven hair raven. Also below the luma floor.
NEAR_BLACK_RGB = (18, 15, 15)
OUTLINE_LUMA = 34.0
# Tone quantization grid.  The caps are exact grid multiples so quantized
# colors are fixed points of the transform (idempotence by construction).
V_STEP = 24
V_MAX = 240  # 10 * V_STEP: flattens specular highlight speckle
S_STEP = 0.16
S_MAX = 0.80  # 5 * S_STEP: mutes toward restrained folk wool


def load_sibling(name: str):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), SCRIPTS / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    # Registration before exec keeps dataclass introspection working.
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


CHANNELS = load_sibling("generate-human-material-channels")
EXTRACTOR = load_sibling("extract-romanian-paper-doll-v1")


def luma(red: int, green: int, blue: int) -> float:
    return (0.2126 * red) + (0.7152 * green) + (0.0722 * blue)


def quantized(rgb: tuple[int, int, int]) -> tuple[int, int, int]:
    hue, saturation, value = colorsys.rgb_to_hsv(*(component / 255.0 for component in rgb))
    saturation = min(round(saturation / S_STEP) * S_STEP, S_MAX)
    value_8 = min(round(value * 255 / V_STEP) * V_STEP, V_MAX)
    red, green, blue = colorsys.hsv_to_rgb(hue, saturation, value_8 / 255.0)
    return (round(red * 255), round(green * 255), round(blue * 255))


_TONE_CACHE: dict[tuple[int, int, int], tuple[int, int, int]] = {}


def styled_tone(rgb: tuple[int, int, int]) -> tuple[int, int, int]:
    """Quantized tone for one albedo color, iterated to a fixed point.

    8-bit HSV round-trips can drift across the quantization grid, so the
    raw quantizer is not always its own fixed point.  Walking the chain
    until it repeats -- and collapsing any short cycle onto its smallest
    member -- makes the mapping exactly idempotent and deterministic.
    """
    if rgb in _TONE_CACHE:
        return _TONE_CACHE[rgb]
    chain = [rgb]
    seen = {rgb: 0}
    while True:
        candidate = quantized(chain[-1])
        if luma(*candidate) < OUTLINE_LUMA:
            candidate = NEAR_BLACK_RGB
        if candidate == chain[-1]:
            break
        if candidate in seen:
            candidate = min(chain[seen[candidate] :])
            break
        seen[candidate] = len(chain)
        chain.append(candidate)
    for visited in chain:
        _TONE_CACHE[visited] = candidate
    return candidate


def stylize_albedo(albedo: Image.Image, tones: bool) -> Image.Image:
    """Applies the folk surface pass; alpha stays byte-identical."""
    source = albedo.load()
    output = Image.new("RGBA", albedo.size)
    pixels = output.load()

    def transparent(x: int, y: int) -> bool:
        if x < 0 or y < 0 or x >= albedo.width or y >= albedo.height:
            return True
        return source[x, y][3] < 128

    for y in range(albedo.height):
        for x in range(albedo.width):
            red, green, blue, alpha = source[x, y]
            if alpha == 0:
                pixels[x, y] = (0, 0, 0, 0)
                continue
            edge = (
                alpha < 128
                or transparent(x - 1, y)
                or transparent(x + 1, y)
                or transparent(x, y - 1)
                or transparent(x, y + 1)
            )
            if edge:
                pixels[x, y] = (*OUTLINE_RGB, alpha)
            elif luma(red, green, blue) < OUTLINE_LUMA:
                pixels[x, y] = (*NEAR_BLACK_RGB, alpha)
            elif tones:
                pixels[x, y] = (*styled_tone((red, green, blue)), alpha)
            else:
                pixels[x, y] = (red, green, blue, alpha)
    return output


def png_bytes(image: Image.Image) -> bytes:
    buffer = io.BytesIO()
    image.save(buffer, format="PNG", optimize=False, compress_level=9)
    return buffer.getvalue()


def jobs() -> list[tuple[Path, bool, object]]:
    """Every (albedo path, full tones?, channel regenerator) this pass owns.

    The channel regenerator is a callable producing ``{suffix: Image}`` from
    the restyled albedo path, or ``None`` for sets without companions.
    """
    result: list[tuple[Path, bool, object]] = []
    for stem in CHANNELS.MASK_REGIONS:
        path = HUMAN_RUNTIME / f"{stem}.png"

        def root_channels(path: Path = path, stem: str = stem):
            albedo = Image.open(path).convert("RGBA")
            return {
                "_mask": CHANNELS.build_mask(albedo, CHANNELS.MASK_REGIONS[stem]),
                "_normal": CHANNELS.build_normal(albedo),
                "_shadow": CHANNELS.build_shadow(albedo),
            }

        result.append((path, True, root_channels))
    for part in EXTRACTOR.PARTS:
        path = EXTRACTOR.output_path(part)
        is_gear = part.output.startswith("gear/")

        def extractor_channels(path: Path = path, part=part):
            albedo = Image.open(path).convert("RGBA")
            return {
                "_mask": EXTRACTOR.build_mask(
                    albedo,
                    part.regions,
                    allow_dark_cloth=part.source in {"voinic", "ucenic_solomonar"},
                ),
                "_normal": EXTRACTOR.build_normal(albedo),
                "_shadow": EXTRACTOR.build_shadow(albedo),
            }

        result.append((path, not is_gear, extractor_channels))
    for path in sorted(STRIGOI_RUNTIME.glob("*.png")):
        if path.stem != "manifest":
            result.append((path, True, None))
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify checked-in PNGs are byte-identical to this stylization",
    )
    args = parser.parse_args()

    mismatches: list[str] = []
    albedo_count = 0
    channel_count = 0

    def emit(path: Path, expected: bytes) -> None:
        if args.check:
            if not path.exists() or path.read_bytes() != expected:
                mismatches.append(path.relative_to(ROOT).as_posix())
        else:
            path.write_bytes(expected)

    for path, tones, channels in jobs():
        original = Image.open(path).convert("RGBA")
        styled = stylize_albedo(original, tones)
        assert styled.getchannel("A").tobytes() == original.getchannel("A").tobytes(), path
        emit(path, png_bytes(styled))
        albedo_count += 1
        if channels is None:
            continue
        # Channels derive from the restyled albedo bytes already on disk.
        for suffix, image in channels().items():
            emit(path.with_name(f"{path.stem}{suffix}{path.suffix}"), png_bytes(image))
            channel_count += 1

    if mismatches:
        for mismatch in mismatches:
            print(f"stale or missing: {mismatch}")
        return 1
    if args.check:
        print(f"ok ({albedo_count} stylized albedos, {channel_count} regenerated channels)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
