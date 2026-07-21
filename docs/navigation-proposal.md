# Navigation Design Proposal — Town Hub Journey

Status: implemented (#129, the town-hub follow-up to the combat redesign).
Companion to `docs/combat-redesign-proposal.md` (phase 6 of that plan); this
document remains the reference for the journey and the per-screen contract.

## Current journey (as shipped)

```
MainMenu → CharacterCreation → Fight → FightResult → Shop → Fight → …
                                  ↘ GameOver → MainMenu
Victory → Shop (next lap) | MainMenu
Continuă → Fight | Shop (ResumeDestination)
```

The Shop ("Prăvălia lui Moș Pintea") doubles as the between-fights hub. That
works, but it forces shopping into every loop iteration, gives the player no
neutral "home" screen, and leaves nowhere natural to hang future destinations
(character sheet, training, tavern rumors) without overloading the shop.

## Target journey

```
MainMenu → CharacterCreation → Town → Arena (Fight) → FightResult → Town → …
```

**Town** is a deliberately small hub screen — one background illustration,
three destination cards, no overworld:

| Destination | Action | Notes |
|---|---|---|
| **Arena** | primary (dominant button) | starts the next ladder fight |
| Prăvălie (Shop) | secondary | existing shop screen, now optional per loop |
| Personaj (Character) | secondary | read-only character view: reuse the creation screen's live cutout preview + attribute list; equip stays in the shop |

Screen contract (applies to Town and is the standard for every screen):

- **One clear title** (`TITLE` type preset, top center).
- **One dominant primary action** (largest button, theme primary style) — on
  Town that is "Luptă în arenă".
- **One consistent back action** — top-left, same label style everywhere
  ("Înapoi"; on Town it returns to MainMenu with an are-you-sure only if a
  run is active).
- Less panel clutter: destination cards use the embroidered panel sparingly
  (one border per card, no nested panels), spacing from the `SPACE_*` scale.

## Per-screen deltas (audit)

| Screen | Title | Primary action | Back | Delta needed |
|---|---|---|---|---|
| MainMenu | ok | Joc nou / Continuă | n/a | none |
| CharacterCreation | ok | Începe → **Town** (was Fight) | Înapoi → Menu | retarget confirm intent |
| **Town (new)** | "Satul" (or named village) | Luptă în arenă | → MainMenu | new screen |
| Fight | n/a (arena) | — | Pause → Abandon | none |
| FightResult | ok | Continuă → **Town** (was Shop) | n/a | retarget |
| Shop | ok | Cumpără/Echipează | Înapoi → **Town** (was Fight) | retarget |
| Victory | ok | Turul următor → **Town** | → MainMenu | retarget |
| GameOver | ok | → MainMenu | n/a | none |

## Implementation sketch (follow-up phase, not now)

1. `GameState::Town` variant (`src/core/mod.rs`) + `TownPlugin`
   (`src/town/mod.rs`) built from existing theme primitives.
2. New `FlowIntent` variants and transition-table rows in `src/flow/mod.rs`
   (the table + AST guard is the only writer of `NextState` — follow the
   documented procedure at `flow/mod.rs:41-64`). Retarget the four rows in
   the audit table; journey tests updated to the new loop.
3. `ResumeDestination` gains `Town` and becomes the default resume target;
   existing saves with `Shop`/`Fight` keep resuming exactly where they said
   (no save-version bump needed if the enum is additive — verify serde
   compatibility; bump to v6 only if not).
4. Baselines: one new scenario checkpoint for Town; `gold-journey` re-capture
   (it walks the whole loop).

Estimated size: one focused PR (state + screen + flow rows + tests +
gold-journey re-baseline). No new content systems, no overworld.
