#!/usr/bin/env python3
"""Reproduce the curated Romanian paper-doll v1 runtime art.

The two OpenAI built-in image-generation outputs are chroma-key production
masters, not attachment-ready geometry.  This script verifies their exact
SHA-256 hashes, runs the installed imagegen chroma-key helper, extracts only
the accepted drawings, splits complete sleeve/leg drawings at deliberate
overlap seams, scales every attachment to the established rig pixel density,
and derives mask/normal/shadow maps from the accepted albedo alpha.

The extraction rectangles are recorded in source-master coordinates.  Parts
not named below are deliberately rejected duplicates.  ``--check`` compares
all generated PNG bytes with the checked-in outputs.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import math
import os
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_HUMAN = Path(
    "/Users/ioachimlihor/.codex/generated_images/"
    "019f701f-492d-70e1-9c8a-e64740c1d407/"
    "exec-0249e3ae-514e-4c04-aef9-9721358f2ddf.png"
)
DEFAULT_EQUIPMENT = Path(
    "/Users/ioachimlihor/.codex/generated_images/"
    "019f701f-492d-70e1-9c8a-e64740c1d407/"
    "exec-51bc0f86-e059-429c-89ba-56447e74e048.png"
)
SOURCE_SHA256 = {
    "human": "8518d470aae529898a79ed22303cb97b4d358b13fa7bd80088dfac2c0d7334d7",
    "equipment": "84b35457846ed8b7e8707e2aedf135cb2a94b3c4b900648307c7215b26f8e0a9",
}
MASK_WEIGHT = 200
RIG = {
    "upper_arm_back": ("upper_arm_back", [-20.0, 26.0], [15.0, 44.0]),
    "forearm_back": ("forearm_back", [-28.0, -2.0], [13.0, 38.0]),
    "hand_back": ("hand_back", [-32.0, -26.0], [13.0, 13.0]),
    "upper_arm_front": ("upper_arm_front", [21.0, 25.0], [15.0, 45.0]),
    "forearm_front": ("forearm_front", [29.0, -3.0], [13.0, 39.0]),
    "hand_front": ("hand_front", [33.0, -28.0], [13.0, 13.0]),
    "head": ("head", [4.0, 60.0], [38.0, 42.0]),
    "hair": ("hair", [1.0, 71.0], [32.0, 20.0]),
    "hair_scurt": ("hair", [1.0, 71.0], [32.0, 20.0]),
    "torso": ("torso", [0.0, 6.0], [44.0, 74.0]),
    "thigh_back": ("thigh_back", [-13.0, -42.0], [17.0, 42.0]),
    "shin_back": ("shin_back", [-15.0, -76.0], [14.0, 38.0]),
    "thigh_front": ("thigh_front", [13.0, -42.0], [17.0, 42.0]),
    "shin_front": ("shin_front", [15.0, -76.0], [14.0, 38.0]),
    "foot_back": ("foot_back", [-8.0, -102.0], [28.0, 12.0]),
    "foot_front": ("foot_front", [23.0, -102.0], [28.0, 12.0]),
}


@dataclass(frozen=True)
class Part:
    source: str
    output: str
    box: tuple[int, int, int, int]
    size: tuple[int, int]
    regions: tuple[str, ...]
    treatment: str = "plain"


# Full sleeve/leg candidates are intentionally split with a small overlap at
# an embroidered cuff or knee.  This gives the articulated rig clean coverage
# as the child joint rotates while avoiding a visible green/transparent seam.
PARTS = (
    # Shared opinci: two distinct right-facing drawings; six duplicate feet rejected.
    Part("human", "shared/foot_back", (410, 779, 502, 903), (56, 24), ("leather", "cloth")),
    Part("human", "shared/foot_front", (539, 776, 638, 904), (56, 24), ("leather", "cloth")),
    Part("human", "shared/hair_scurt", (101, 743, 216, 825), (64, 40), ("hair",), "short_hair"),
    # Haiduc: two complete sleeve drawings and two distinct hands; third sleeve/hand rejected.
    Part("human", "haiduc/upper_arm_back", (608, 80, 691, 205), (30, 88), ("cloth", "embroidery")),
    Part("human", "haiduc/forearm_back", (608, 170, 691, 304), (26, 76), ("cloth", "embroidery")),
    Part("human", "haiduc/hand_back", (890, 84, 934, 139), (26, 26), ("skin",)),
    Part("human", "haiduc/upper_arm_front", (707, 82, 789, 205), (30, 88), ("cloth", "embroidery")),
    Part("human", "haiduc/forearm_front", (707, 172, 789, 305), (26, 76), ("cloth", "embroidery")),
    Part("human", "haiduc/hand_front", (895, 155, 938, 218), (26, 26), ("skin",)),
    Part("human", "haiduc/head", (71, 67, 217, 314), (76, 84), ("skin",), "haiduc_face"),
    Part("human", "haiduc/hair", (240, 67, 358, 327), (64, 40), ("hair",)),
    Part("human", "haiduc/torso", (381, 72, 595, 333), (88, 148), ("cloth", "embroidery")),
    Part("human", "haiduc/thigh_back", (984, 72, 1086, 225), (34, 84), ("cloth", "embroidery")),
    Part("human", "haiduc/shin_back", (1229, 185, 1312, 352), (28, 76), ("cloth", "embroidery", "leather")),
    Part("human", "haiduc/thigh_front", (1102, 72, 1203, 225), (34, 84), ("cloth", "embroidery")),
    Part("human", "haiduc/shin_front", (1343, 185, 1424, 352), (28, 76), ("cloth", "embroidery", "leather")),
    # Cioban: sturdy cream sleeves, dark wool cioareci, tied hair.
    Part("human", "cioban/upper_arm_back", (607, 424, 691, 548), (30, 88), ("cloth", "embroidery")),
    Part("human", "cioban/forearm_back", (607, 515, 691, 646), (26, 76), ("cloth", "embroidery")),
    Part("human", "cioban/hand_back", (894, 432, 939, 492), (26, 26), ("skin",)),
    Part("human", "cioban/upper_arm_front", (706, 427, 789, 550), (30, 88), ("cloth", "embroidery")),
    Part("human", "cioban/forearm_front", (706, 517, 789, 648), (26, 76), ("cloth", "embroidery")),
    Part("human", "cioban/hand_front", (898, 504, 944, 570), (26, 26), ("skin",)),
    Part("human", "cioban/head", (72, 404, 210, 655), (76, 84), ("skin",), "cioban_face"),
    Part("human", "cioban/hair", (259, 406, 359, 669), (64, 40), ("hair",), "hair"),
    Part("human", "cioban/torso", (386, 420, 594, 678), (88, 148), ("cloth", "embroidery")),
    Part("human", "cioban/thigh_back", (970, 409, 1082, 566), (34, 84), ("cloth", "embroidery")),
    Part("human", "cioban/shin_back", (1235, 535, 1319, 714), (28, 76), ("cloth", "embroidery", "leather")),
    Part("human", "cioban/thigh_front", (1092, 411, 1201, 568), (34, 84), ("cloth", "embroidery")),
    Part("human", "cioban/shin_front", (1349, 535, 1439, 714), (28, 76), ("cloth", "embroidery", "leather")),
    # Exactly five equipment components, preserving the established rig display geometry.
    Part("equipment", "gear/bata_ciobaneasca", (83, 98, 219, 829), (36, 272), ("leather",)),
    Part("equipment", "gear/topor_de_padurar", (326, 221, 519, 738), (84, 164), ("leather",)),
    Part("equipment", "gear/cojoc_gros", (566, 211, 942, 746), (108, 144), ("cloth", "leather")),
    Part("equipment", "gear/caciula_de_oaie", (998, 290, 1298, 636), (80, 60), ("hair",)),
    Part("equipment", "gear/opinci_iuti", (1353, 512, 1701, 738), (60, 36), ("leather", "cloth")),
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def remove_chroma(source: Path, output: Path, helper: Path) -> None:
    subprocess.run(
        [
            "python3",
            os.fspath(helper),
            "--input",
            os.fspath(source),
            "--out",
            os.fspath(output),
            "--auto-key",
            "border",
            "--soft-matte",
            "--transparent-threshold",
            "12",
            "--opaque-threshold",
            "220",
            "--despill",
        ],
        check=True,
    )


def treatment_mask(size: tuple[int, int], treatment: str) -> Image.Image | None:
    if treatment not in {"haiduc_face", "cioban_face"}:
        return None
    mask = Image.new("L", size, 0)
    draw = ImageDraw.Draw(mask)
    if treatment == "haiduc_face":
        polygon = [(62, 4), (121, 0), (145, 28), (145, 151), (126, 181), (101, 186), (98, 247), (72, 247), (70, 184), (48, 104)]
    else:
        polygon = [(58, 7), (108, 1), (135, 34), (137, 151), (118, 180), (92, 187), (88, 251), (43, 251), (42, 177), (45, 100)]
    draw.polygon(polygon, fill=255)
    return mask


def apply_treatment(image: Image.Image, treatment: str) -> Image.Image:
    if treatment in {"haiduc_face", "cioban_face"}:
        mask = treatment_mask(image.size, treatment)
        assert mask is not None
        image.putalpha(ImageChops.multiply(image.getchannel("A"), mask))
    elif treatment in {"hair", "short_hair"}:
        pixels = image.load()
        alpha = image.getchannel("A")
        selected = Image.new("L", image.size, 0)
        target = selected.load()
        source_alpha = alpha.load()
        for y in range(image.height):
            for x in range(image.width):
                red, green, blue, _ = pixels[x, y]
                light = 0.2126 * red + 0.7152 * green + 0.0722 * blue
                if source_alpha[x, y] and light < 126 and (red < 165 or blue < 72):
                    target[x, y] = source_alpha[x, y]
        if treatment == "short_hair":
            silhouette = Image.new("L", image.size, 0)
            ImageDraw.Draw(silhouette).polygon(
                [(0, 0), (image.width, 0), (image.width, 41), (82, 58), (35, 56), (0, 43)],
                fill=255,
            )
            selected = ImageChops.multiply(selected, silhouette)
        image.putalpha(selected)
    return image


def tight_crop(image: Image.Image, padding: int = 2) -> Image.Image:
    alpha = image.getchannel("A")
    bbox = alpha.point(lambda value: 255 if value > 20 else 0).getbbox()
    if bbox is None:
        raise ValueError("accepted crop has no opaque pixels")
    left, top, right, bottom = bbox
    return image.crop(
        (
            max(0, left - padding),
            max(0, top - padding),
            min(image.width, right + padding),
            min(image.height, bottom + padding),
        )
    )


def accepted_albedo(source: Image.Image, part: Part) -> Image.Image:
    image = source.crop(part.box).convert("RGBA")
    image = tight_crop(apply_treatment(image, part.treatment))
    image = image.resize(part.size, Image.Resampling.NEAREST)
    alpha = image.getchannel("A")
    # Keep invisible RGB deterministic before restrained 96-colour quantization.
    background = Image.new("RGBA", image.size, (0, 0, 0, 0))
    image = Image.alpha_composite(background, image)
    rgb = image.convert("RGB").quantize(colors=96, method=Image.Quantize.MEDIANCUT).convert("RGB")
    rgb.putalpha(alpha)
    return rgb


def luma(red: int, green: int, blue: int) -> float:
    return 0.2126 * red + 0.7152 * green + 0.0722 * blue


def classify(red: int, green: int, blue: int, alpha: int, regions: tuple[str, ...]) -> str | None:
    if alpha < 128 or luma(red, green, blue) < 34:
        return None
    high, low = max(red, green, blue), min(red, green, blue)
    saturation = 0.0 if high == 0 else (high - low) / high
    if regions == ("hair",):
        return "hair" if luma(red, green, blue) < 150 else None
    if regions == ("skin",):
        return "skin" if red > 70 and red > green * 1.03 and green > blue * 1.02 else None
    if regions[0] == "leather":
        if red > 44 and green > 18 and blue < 105 and red > blue * 1.25:
            return "leather"
        if "cloth" in regions and red > 105 and green > 78 and blue > 45 and saturation < 0.48:
            return "cloth"
        return None
    cloth = red > 95 and green > 67 and blue > 38 and saturation < 0.48
    if cloth:
        return "cloth"
    if "embroidery" in regions and red > 78 and blue < 75 and saturation > 0.38:
        return "embroidery"
    if "leather" in regions and red > 44 and green > 18 and blue < 92:
        return "leather"
    return None


def build_mask(albedo: Image.Image, regions: tuple[str, ...]) -> Image.Image:
    output = Image.new("RGBA", albedo.size)
    source, target = albedo.load(), output.load()
    for y in range(albedo.height):
        for x in range(albedo.width):
            red, green, blue, alpha = source[x, y]
            semantic = classify(red, green, blue, alpha, regions)
            channels = [0, 0, 0]
            if semantic in regions:
                channels[regions.index(semantic)] = MASK_WEIGHT
            target[x, y] = (*channels, alpha)
    return output


def build_normal(albedo: Image.Image) -> Image.Image:
    output = Image.new("RGBA", albedo.size)
    source, target = albedo.load(), output.load()

    def height_at(x: int, y: int, fallback: float) -> float:
        red, green, blue, alpha = source[x, y]
        return fallback if alpha == 0 else luma(red, green, blue) / 255.0

    for y in range(albedo.height):
        for x in range(albedo.width):
            red, green, blue, alpha = source[x, y]
            center = luma(red, green, blue) / 255.0
            nx = -(height_at(min(albedo.width - 1, x + 1), y, center) - height_at(max(0, x - 1), y, center)) * 0.18
            ny = (height_at(x, min(albedo.height - 1, y + 1), center) - height_at(x, max(0, y - 1), center)) * 0.18
            length = math.sqrt(nx * nx + ny * ny + 1.0)
            encoded = tuple(round((component / length * 0.5 + 0.5) * 255) for component in (nx, ny, 1.0))
            target[x, y] = (*encoded, alpha)
    return output


def build_shadow(albedo: Image.Image) -> Image.Image:
    output = Image.new("RGBA", albedo.size)
    source, target = albedo.load(), output.load()
    for y in range(albedo.height):
        for x in range(albedo.width):
            red, green, blue, alpha = source[x, y]
            darkness = max(0.0, min(1.0, (154.0 - luma(red, green, blue)) / 154.0))
            value = 255 - round(31 * darkness)
            target[x, y] = (value, value, value, alpha)
    return output


def png_bytes(image: Image.Image) -> bytes:
    buffer = io.BytesIO()
    image.save(buffer, format="PNG", optimize=False, compress_level=9)
    return buffer.getvalue()


def output_path(part: Part, suffix: str = "") -> Path:
    if part.output.startswith("gear/"):
        stem = part.output.removeprefix("gear/")
        return ROOT / f"assets/fighters/gear/runtime/{stem}{suffix}.png"
    return ROOT / f"assets/fighters/human/runtime/{part.output}{suffix}.png"


def source_sheet(human: Image.Image, equipment: Image.Image) -> Image.Image:
    human_preview = human.convert("RGB").resize((768, 512), Image.Resampling.NEAREST)
    equipment_height = round(equipment.height * 768 / equipment.width)
    equipment_preview = equipment.convert("RGB").resize((768, equipment_height), Image.Resampling.NEAREST)
    sheet = Image.new("RGB", (768, 512 + equipment_height), (0, 255, 0))
    sheet.paste(human_preview, (0, 0))
    sheet.paste(equipment_preview, (0, 512))
    return sheet.quantize(colors=128, method=Image.Quantize.MEDIANCUT)


def toml_array(values: list[float]) -> str:
    return "[" + ", ".join(f"{value:.1f}" for value in values) + "]"


def human_manifest(role: str) -> bytes:
    lines = [
        "# Generated by scripts/extract-romanian-paper-doll-v1.py; edit the extraction map, not this file.",
        "version = 1",
        "",
    ]
    for part in (candidate for candidate in PARTS if candidate.output.startswith(f"{role}/")):
        stem = part.output.split("/", 1)[1]
        attachment, pivot, display = RIG[stem]
        for suffix in ("", "_mask", "_normal", "_shadow"):
            id_suffix = suffix.replace("_", "-")
            lines.extend(
                [
                    "[[record]]",
                    f'id = "fighters.human.runtime.{role}.{stem.replace("_", "-")}{id_suffix}"',
                    f'path = "{stem}{suffix}.png"',
                    'kind = "image"',
                    'category = "fighter-runtime-part"',
                    'status = "runtime"',
                    (
                        'provenance = "cropped-from-openai-generated-source-sheet"'
                        if not suffix
                        else 'provenance = "deterministic-technical-map-from-accepted-albedo"'
                    ),
                    (
                        'source_sheet = "fighters.human.source.romanian-paper-doll-v1"'
                        if not suffix
                        else 'generator = "scripts/extract-romanian-paper-doll-v1.py"'
                    ),
                    'license = "Same as project assets unless superseded"',
                    f"dimensions = [{part.size[0]}, {part.size[1]}]",
                    'sampler = "nearest"',
                    f'attachment = "{attachment}"',
                    f"pivot = {toml_array(pivot)}",
                    f"display = {toml_array(display)}",
                    'crop = "unknown"',
                    "",
                ]
            )
    return "\n".join(lines).encode()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--human-source", type=Path, default=DEFAULT_HUMAN)
    parser.add_argument("--equipment-source", type=Path, default=DEFAULT_EQUIPMENT)
    parser.add_argument(
        "--helper",
        type=Path,
        default=Path.home() / ".codex/skills/.system/imagegen/scripts/remove_chroma_key.py",
    )
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    for name, path in (("human", args.human_source), ("equipment", args.equipment_source)):
        actual = sha256(path)
        if actual != SOURCE_SHA256[name]:
            raise SystemExit(f"{name} source SHA-256 mismatch: {actual}")

    mismatches: list[str] = []
    with tempfile.TemporaryDirectory(prefix="romanian-paper-doll-v1-") as temporary:
        temporary_path = Path(temporary)
        alpha_paths = {
            "human": temporary_path / "human-alpha.png",
            "equipment": temporary_path / "equipment-alpha.png",
        }
        remove_chroma(args.human_source, alpha_paths["human"], args.helper)
        remove_chroma(args.equipment_source, alpha_paths["equipment"], args.helper)
        sources = {name: Image.open(path).convert("RGBA") for name, path in alpha_paths.items()}

        for part in PARTS:
            albedo = accepted_albedo(sources[part.source], part)
            generated = {
                "": albedo,
                "_mask": build_mask(albedo, part.regions),
                "_normal": build_normal(albedo),
                "_shadow": build_shadow(albedo),
            }
            for suffix, image in generated.items():
                path = output_path(part, suffix)
                expected = png_bytes(image)
                if args.check:
                    if not path.exists() or path.read_bytes() != expected:
                        mismatches.append(path.relative_to(ROOT).as_posix())
                else:
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(expected)

        tracked_source = ROOT / "assets/fighters/human/source/romanian-paper-doll-v1/romanian-paper-doll-v1.png"
        expected_source = png_bytes(source_sheet(
            Image.open(args.human_source), Image.open(args.equipment_source)
        ))
        if args.check:
            if not tracked_source.exists() or tracked_source.read_bytes() != expected_source:
                mismatches.append(tracked_source.relative_to(ROOT).as_posix())
        else:
            tracked_source.parent.mkdir(parents=True, exist_ok=True)
            tracked_source.write_bytes(expected_source)

        for role in ("shared", "haiduc", "cioban"):
            manifest = ROOT / f"assets/fighters/human/runtime/{role}/manifest.toml"
            expected_manifest = human_manifest(role)
            if args.check:
                if not manifest.exists() or manifest.read_bytes() != expected_manifest:
                    mismatches.append(manifest.relative_to(ROOT).as_posix())
            else:
                manifest.write_bytes(expected_manifest)

    if mismatches:
        for path in mismatches:
            print(f"stale or missing: {path}")
        return 1
    if args.check:
        print(f"ok ({len(PARTS)} albedos, {len(PARTS) * 3} exact-alpha companions, 1 source sheet, 3 manifests)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
