# Final review fixes — provenance and RGB-only material masks

## Scope

This follow-up fixes the two confirmed whole-branch review findings:

1. The ignored Task 3 report was the only location containing the exact
   generated-reference prompt, source path, SHA-256, rights context, and
   large-file-hook exclusion decision.
2. Catalog v2 accepted more palette regions than the documented RGB mask
   contract, while Rust/WGSL exposed mask alpha as a fourth recolor channel.

No `CharacterDefinition`, `PartId`, catalog selection, runtime transform,
material asset, visual baseline, or production fallback behavior changed.

## Changes

- Added the tracked durable record
  `docs/art-references/known-good-human-material-reference.md` with the exact
  prompt, original built-in generation path, SHA-256
  `2b57e1553c0b83c529ae593987f4ed44988a4d020a7c9f30a7536bba7919de37`,
  project-rights context, and the decision not to weaken the 1 MB
  `check-added-large-files` hook for the 1.867 MB reference.
- Updated `assets/CREDITS.md`, the human runtime manifest, and
  `docs/art-direction.md` to point at that tracked record.
- Added catalog validation and a regression test rejecting more than three
  positional palette regions because alpha remains the silhouette.
- Removed the fourth palette uniform and `mask.a` recoloring branch from both
  Rust and WGSL. Defensive renderer cardinality now also clamps to RGB's three
  regions.

## TDD evidence

RED:

```text
cargo test --lib character::catalog::tests::catalog_rejects_a_palette_that_would_recolor_through_mask_alpha -- --exact --nocapture

FAILED: mask alpha is reserved for the silhouette, so only RGB may identify palette regions
test result: FAILED. 0 passed; 1 failed; 666 filtered out
```

GREEN after the minimum catalog validation:

```text
cargo test --lib character::catalog::tests::catalog_rejects_a_palette_that_would_recolor_through_mask_alpha -- --exact --nocapture

test result: ok. 1 passed; 0 failed; 666 filtered out
```

## Focused verification

```text
cargo test --lib character::catalog
test result: ok. 18 passed; 0 failed; 649 filtered out

cargo test --lib character::material
test result: ok. 5 passed; 0 failed; 662 filtered out

cargo test --lib cutout::
test result: ok. 19 passed; 0 failed; 648 filtered out

cargo fmt --all -- --check
PASS

cargo clippy --all-targets -- -D warnings
PASS

cargo xtask assets check
PASS: all sidecar records validated cleanly; 173/173 files covered

cargo check --target wasm32-unknown-unknown
PASS

git diff --check
PASS
```

The branch was fetched immediately before editing; `origin/main` remained
`230aaa1` and was already an ancestor of the working branch. No rebase, visual
baseline capture, push, or unrelated working-tree mutation was performed.
