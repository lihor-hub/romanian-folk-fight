# First Pixel-Art Assets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first production-intent pixel-art cutout source sheet for the human/player body style.

**Architecture:** This is an asset-source slice, not runtime integration. The PNG lives under `assets/fighters/` with a small manifest that explains intended parts, style constraints, and how later rig work should interpret it.

**Tech Stack:** Built-in `image_gen` for raster generation; existing imagegen chroma-key helper only for alpha removal; shell validation for PNG metadata; Markdown docs for manifest and credits.

## Global Constraints

- Generate raster pixel-art assets directly; do not add a new procedural Python asset-generation script.
- Store project-bound generated assets inside the workspace, never only under `$CODEX_HOME/generated_images`.
- Use transparent PNG output for accepted assets.
- Document every accepted asset in `assets/CREDITS.md`.
- This task does not wire the asset into Bevy runtime rendering.

---

### Task 1: Generate the Human Cutout Source Sheet

**Files:**
- Create: `assets/fighters/human/source/human_cutout_parts_v1.png`
- Create: `tmp/imagegen/human_cutout_parts_v1_chroma.png`

**Interfaces:**
- Consumes: `docs/art-direction.md`
- Produces: A transparent PNG source sheet for later slicing/rigging.

- [ ] **Step 1: Generate with the built-in image tool**

Prompt:

```text
Use case: stylized-concept
Asset type: game asset source sheet for a 2D Bevy arena RPG
Primary request: a polished pixel-art cutout body-part source sheet for a Romanian folklore arena fighter, authored facing right, inspired by the modular paper-doll customization feel of Swords and Sandals but with original project-owned art.
Scene/backdrop: perfectly flat solid #00ff00 chroma-key background for removal; no floor, no shadows, no gradients.
Subject: separated human fighter parts arranged neatly with generous padding: head, torso with cream linen shirt and red/gold folk belt accents, upper arms, forearms, hands, upper legs, shins, feet/opinci. Include dark hair and moustache as separate parts near the head. Side-view readable proportions, heroic but compact.
Style/medium: polished high-detail pixel art, crisp clusters, thick dark outline, clean 192-256px-tall game readability, Romanian folk textile motifs.
Color palette: deep red #7a1f1f, black #1a1214, cream #e8dcc8, gold #c9a227, warm skin tones.
Composition/framing: one source sheet, all parts isolated and not touching, facing right, no labels or text.
Constraints: no copied Swords and Sandals assets, no watermark, no text, no photorealism, no blurry painterly texture, no #00ff00 in the subject.
Avoid: full character sprite sheet frames, weapons, shields, background scenery, cast shadows.
```

- [ ] **Step 2: Copy the generated chroma-key PNG into `tmp/imagegen/`**

Run:

```bash
mkdir -p tmp/imagegen assets/fighters/human/source
cp "$GENERATED_IMAGE_PATH" tmp/imagegen/human_cutout_parts_v1_chroma.png
```

- [ ] **Step 3: Remove the chroma-key background**

Run:

```bash
python "${CODEX_HOME:-$HOME/.codex}/skills/.system/imagegen/scripts/remove_chroma_key.py" \
  --input tmp/imagegen/human_cutout_parts_v1_chroma.png \
  --out assets/fighters/human/source/human_cutout_parts_v1.png \
  --auto-key border \
  --soft-matte \
  --transparent-threshold 12 \
  --opaque-threshold 220 \
  --despill
```

- [ ] **Step 4: Validate the PNG alpha channel**

Run:

```bash
python - <<'PY'
from pathlib import Path
from PIL import Image

path = Path("assets/fighters/human/source/human_cutout_parts_v1.png")
im = Image.open(path)
assert im.mode == "RGBA", im.mode
w, h = im.size
corners = [im.getpixel((0, 0)), im.getpixel((w - 1, 0)), im.getpixel((0, h - 1)), im.getpixel((w - 1, h - 1))]
assert all(pixel[3] == 0 for pixel in corners), corners
opaque = sum(1 for pixel in im.getdata() if pixel[3] > 0)
assert opaque > 1000, opaque
print(f"{path}: {w}x{h}, opaque pixels={opaque}")
PY
```

Expected: command exits 0 and prints the image dimensions plus opaque pixel count.

### Task 2: Document the Asset

**Files:**
- Create: `assets/fighters/human/source/README.md`
- Modify: `assets/CREDITS.md`

**Interfaces:**
- Consumes: `assets/fighters/human/source/human_cutout_parts_v1.png`
- Produces: asset metadata for future rig and licensing work.

- [ ] **Step 1: Add the source-sheet manifest**

Create `assets/fighters/human/source/README.md` with:

```markdown
# Human Cutout Source Sheet

`human_cutout_parts_v1.png` is the first production-intent source sheet for the
pixel-art cutout fighter direction.

It is not wired into runtime rendering yet. Later rig work should either slice
this sheet into individual transparent parts or replace it with cleaned
individual part files that preserve the same style.

## Intended Parts

- Head
- Hair and moustache
- Torso with cream linen shirt and red/gold folk accents
- Upper arms
- Forearms
- Hands
- Upper legs
- Shins
- Feet/opinci

## Style Contract

- Side-view fighter parts authored facing right.
- Thick dark outlines using the shared art-direction palette.
- Polished pixel-art clusters, not painterly texture.
- Romanian folk textile accents on clothing bands and belt areas.
- Transparent background after chroma-key cleanup.

## Runtime Status

This is a source asset for issue #99. It does not replace `assets/sprites/player.png`
and does not change the Bevy runtime path yet.
```

- [ ] **Step 2: Add credits**

Add a new `## Fighter cutout source sheets` section near the top of `assets/CREDITS.md`:

```markdown
## Fighter cutout source sheets (`assets/fighters/`)

The fighter cutout source sheet below was generated for this project with
OpenAI image generation from the prompt documented in
`docs/superpowers/plans/2026-07-08-first-pixel-art-assets.md`, then locally
post-processed only to remove the chroma-key background. It is project-owned
generated art and may be replaced by cleaned artist-authored parts.

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `fighters/human/source/human_cutout_parts_v1.png` | Human/player pixel-art cutout body-part source sheet | OpenAI-generated for this project | Same as project assets unless superseded |
```

### Task 3: Verify and Ship

**Files:**
- Modify: repository git metadata only through commit/push/PR.

**Interfaces:**
- Consumes: all files from Tasks 1-2.
- Produces: committed branch and PR linked to issue #99.

- [ ] **Step 1: Verify no procedural asset script was added**

Run:

```bash
git diff --name-only origin/main...HEAD | rg '^scripts/.*\\.py$' && exit 1 || true
```

Expected: command exits 0 with no new script path.

- [ ] **Step 2: Verify whitespace and formatting**

Run:

```bash
git diff --check
PATH="/opt/homebrew/opt/rustup/bin:$PATH" cargo fmt --all -- --check
```

Expected: both commands exit 0.

- [ ] **Step 3: Review the diff**

Run:

```bash
git diff --stat
git diff -- assets/CREDITS.md assets/fighters/human/source/README.md docs/superpowers/plans/2026-07-08-first-pixel-art-assets.md
```

Expected: diff contains only asset files, manifest, credits, and this plan.

- [ ] **Step 4: Commit**

Run:

```bash
git add assets/fighters/human/source/human_cutout_parts_v1.png \
  assets/fighters/human/source/README.md \
  assets/CREDITS.md \
  docs/superpowers/plans/2026-07-08-first-pixel-art-assets.md
git commit -m "assets: add first pixel-art cutout source sheet"
```

- [ ] **Step 5: Push and open PR**

Run:

```bash
git push -u origin HEAD
gh pr create --repo lihor-hub/romanian-folk-fight --base main --head codex/first-pixel-art-assets-99 --title "assets: add first pixel-art cutout source sheet" --body "Closes #99"
```
