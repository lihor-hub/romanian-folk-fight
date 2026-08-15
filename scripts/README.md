- **`generate-gear-visuals.py`**
  - **Purpose:** Generates transparent 128x128 equipment overlay placeholder sprites (weapons, shields, torsos, helmets, boots) in the Romanian Folk Fight color palette.
  - **Output Path:** `assets/gear/`
  - **Dependencies:** `stdlib-only` (Uses only native `struct` and `zlib` modules for custom PNG encoding).
  - **Idempotent:** Yes. Automatically overwrites existing sprites upon re-run without needing flags.

- **`generate-pictogram.py`**
  - **Purpose:** Generates 32x32 combat action UI pictograms (strikes, blocking, movement, resting) using the folk color palette, matching IDs mapping to combat state descriptors.
  - **Output Path:** `assets/ui/pictograms/`
  - **Dependencies:** `stdlib-only` (Encodes pixel grids directly using native custom PNG binary streams).
  - **Idempotent:** Yes. Automatically overwrites existing icon assets upon re-run without parameters.

- **`generate-shop-icons.py`**
  - **Purpose:** Generates small 32x32 folk-themed interface elements for shopping screens (coins, weapons, shields, clothing slots, helmets, boots).
  - **Output Path:** `assets/ui/`
  - **Dependencies:** `stdlib-only` (Direct custom PNG byte injection using standard library streams).
  - **Idempotent:** Yes. Automatically replaces matching icon imagery assets upon re-run without manual configuration switches.

- **`generate-ui-panel.py`**
  - **Purpose:** Generates a 96x96 pixel embroidery-motif 9-slice UI panel border (featuring gold cross-stitch diamonds on a deep-red band with crisp black corners) around a translucent center for menus and dialog fills.
  - **Output Path:** `assets/ui/panel_border.png`
  - **Dependencies:** `stdlib-only` (Direct byte matrix processing mapping natively into a zlib-compressed binary PNG chunk format).
  - **Idempotent:** Yes. Overwrites the static compiled framework canvas element directly upon execution.

- **`stylize-fighter-parts.py`**
  - **Purpose:** Applies a deterministic, phase-4 folk stylization pass to fighter character sprite sheet configurations—painting high-contrast dark outline ink bounds onto silhouettes and quantizing interior coloring matrices to flatten shading surfaces into two-tone blocks.
  - **Output Path:** Overwrites matching albedo assets inside `assets/fighters/human/runtime/` and `assets/fighters/strigoi/runtime/`, and automatically updates their accompanying technical maps (`_mask`, `_normal`, `_shadow`).
  - **Dependencies:** `Pillow (PIL)` (Requires an active external Python image processing layer).
  - **Idempotent:** Yes. Features an optional automated verification configuration path. Run with the `--check` parameter flag to validate asset consistency without re-writing files.
