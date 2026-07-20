# Combat Vertical Slice Redesign — Design Proposal

Status: accepted design for the combat redesign phase (see delivery order at the
end). Companion navigation proposal: `docs/navigation-proposal.md` (phase 6).

## 1. Problem classification

**Gameplay problems (engine):** almost none. The deterministic turn engine
already models close/near/far (`DuelDistance`, `src/combat/engine.rs:33`),
movement actions mutate it correctly, out-of-reach strikes resolve safely, and
`ActionDescriptor` (`src/combat/actions.rs:107`) is a clean UI derivation of
engine rules. The only gameplay gap is the missing middle-risk strike between
QuickStrike (5 st / 80% / 1x) and HeavyStrike (15 st / 60% / 2x).

**Presentation problems (the bulk of this work):**

- Distance is state without a body: fighters stand at hardcoded anchors
  (`src/arena/mod.rs:46`, x = ±220) and both `FootworkStep` and `AttackLunge`
  tween out and return *to the anchor*. Every fight reads as the same static
  spacing regardless of band; the player must read the log or the HUD text to
  know reach.
- The action UI is a full-width bottom strip of seven identical 100×64
  rectangles — a debugging toolbar, not a fighter's option set. Grouping
  exists in data (`ActionCategory`) but is only used on phone.
- Fighters (128px in an 800×600 stage) are visually subordinate to the HUD
  panels, embroidered borders, and background detail.

**Art-direction problems:**

- Cutout proportions are naturalistic, not the friendly/exaggerated folk
  target (bigger head and hands, strong silhouette).
- Poses are single-keyframe offsets with no anticipation/impact/recovery.
- Outlines and shading are inconsistent between body parts and gear layers.

## 2. Persistent positional staging (phase 2)

Engine stays untouched. A new presentation resource owns fighter world
positions and is the single source of truth for where fighters stand:

```
ArenaStaging {
    player_x: f32,
    enemy_x: f32,
}
```

- **Band → gap mapping:** close = 140, near = 250, far = 360 world units
  (fighter-center to fighter-center). Fights start at `DuelDistance::starting()`
  (close), centered on `STAGE_BIAS = +40` (biased right to keep clear of the
  palette): player `STAGE_BIAS - gap/2 = -30`, enemy `STAGE_BIAS + gap/2 =
  +110`.
- **On `CombatEvent::Moved { from, to }`:** only the actor moves. New actor x
  = opponent x ∓ gap(to) (player always left of enemy; sides never cross).
- **Clamping:** fighter centers are clamped to `[-150, +330]` (the left band
  of the stage is reserved for the action palette, §3). If the actor's target
  violates a wall, the residual shifts *both* fighters (pair slide) so the
  gap stays exact — spacing is truth, absolute position is composition.
- **No return-to-anchor, ever.** `FootworkStep` is replaced by a real position
  tween (ease-out, ~0.45 s) from current x to new x. `AttackLunge` starts from
  the fighter's *current* staged x, peaks toward the opponent at 35% of the
  *current* gap, and returns to the current staged x.
- **Reduced motion:** position is semantic state, not decoration — the fighter
  still ends at the new x; the tween is replaced by a near-instant snap
  (existing reduced-motion displacement rules keep applying to lunges).
- **Readability without the log:**
  - Spacing itself (140 vs 250 vs 360 is unmistakable at 800px).
  - A small ground chip centered between the fighters showing the band name
    (Aproape / Aproximativ / Departe — reuse the existing HUD distance
    readout strings), restyled as a low-contrast etched marker at ground level.
  - Out-of-range: strike buttons already disable with a Romanian reason;
    additionally, while any strike button is hovered out of reach, the ground
    gap chip pulses and shows the reach shortfall (e.g. "Prea departe — fă un
    pas"). No new mechanics — pure presentation of `position_legal`.

Tests: rewrite the anchor-pinning arena tests to pin the staging math
(gap-per-band, clamping, pair slide, lunge-from-current-position). The frozen
desktop-fight phase helper (`desktop_fight_freeze`) keeps working — staging is
deterministic per event sequence.

## 3. Contextual combat palette (phase 3, desktop-first)

Replace the desktop bottom strip with a **vertical command banner on the left
edge**, visually "held" by the player's side of the arena: a narrow
embroidered-linen column (~200px wide, anchored left:16, bottom:16, height to
~65% of stage) with four labeled groups in decision order:

1. **Lovituri** (attacks) — QuickStrike, NormalStrike, HeavyStrike
2. **Mișcare** (movement) — StepForward, LeapForward, StepBack
3. **Apărare** (defense) — Block
4. **Refacere** (recovery) — Rest

Per-action row: pictogram tile (40px) + short label + info line (strikes:
`70% · −9 st`; movement: band arrow; rest: `+20 st`). Data comes exclusively
from `generate_action_descriptors` — no rule duplication; the palette keeps
rendering whatever descriptors say (an eighth action appears automatically).

- **Disabled actions** stay visible, dimmed, with the descriptor's
  `disabled_reason` inline under the label (already produced by
  `action_disabled_reason`). Reach-disabled strikes also show a tiny distance
  pictogram linking them to the ground gap chip.
- **Pictograms:** 8 small PNG glyphs generated by script (same pipeline as
  `scripts/generate-shop-icons.py`), replacing the ASCII placeholder glyphs.
- **Phone layout keeps its structure** (bottom category disclosure bar below
  the letterboxed stage — it already avoids covering fighters) and only picks
  up the new pictograms/labels. Desktop is the redesign target this phase.
- Keyboard: existing digit mapping extends to 8 (debug feature); tab order
  follows group order.

The staging clamp in §2 (`player_x ≥ -150`) guarantees fighters never walk
more than a sliver behind the banner.

## 4. NormalStrike (phase 3, engine part)

Between the existing strikes, tuned to be the "honest default":

| | Quick | **Normal** | Heavy |
|---|---|---|---|
| Stamina | 5 | **9** | 15 |
| Base hit | 80 | **70** | 60 |
| Damage | 1× | **1.5×** | 2× |
| Reach | melee | melee | melee |

- Engine: change `strike()`'s `i32` multiplier to a percent (`100/150/200`),
  damage = `base * percent / 100`, floor 1. Quick/Heavy numbers are identical
  before and after, so the scripted regression pin
  (`unarmed_fighters_reproduce_pre_equipment_numbers`) must still pass —
  that's the safety proof the engine wasn't destabilized.
- Data: new arms in every exhaustive `CombatAction` match (all enumerated in
  the recon: `engine.rs`, `actions.rs`, `ai.rs` test, `ALL_ACTIONS: [_; 8]`,
  digit-8 debug mapping). Label: **"Lovitură dreaptă"**, id `normal-strike`,
  category Strikes.
- AI: weight `0.9` in the weighted pick when affordable; kill-range ordering
  Heavy > Normal > Quick. This changes RNG draw counts, so the two seeded
  behavioral pins (`duels_against_the_strigoi_are_winnable_and_losable`,
  `seeded_combat_is_bit_for_bit_identical…`) are re-verified and re-pinned
  deliberately in the same commit — expected and documented, not drift.
- Count-of-seven assertions in `actions.rs` tests update to eight.

If any of this destabilizes the engine pins in a way that can't be explained
line-by-line, NormalStrike ships as design-only (the brief allows this) — the
palette does not depend on it.

## 5. Character presentation (phase 4)

Scope: **one player treatment (human template) + one enemy (strigoi
template)**. Zmeu/boss untouched this phase.

- **Proportions (code, `cutout.rs` templates):** head ×~1.18, hands ×~1.25,
  slightly wider torso stance, marginally shorter legs — friendly/exaggerated
  but readable. Increase joint overlap margins so elbows/knees hide seams.
- **Surface (assets, scripted):** one stylization pass over
  `assets/fighters/{human,strigoi}/runtime/*.png` via a new
  `scripts/stylize-fighter-parts.py`: uniform dark-walnut outline, flattened
  highlights (no plastic specular), 2-tone restrained shading, palette
  harmonized with the theme's folk colors. Painted look, no 3D/plastic, no
  generic fantasy armour (gear art untouched except outline harmonization).
- **Modularity preserved:** part set, gear attachment points, appearance
  system (skin/hair/build/accent) and preset costumes all unchanged —
  equipment keeps changing the silhouette via the existing gear layers.

## 6. Animation & feedback pass (phase 5)

- **Three-phase pose envelope:** extend `CutoutPoseTimer` to
  anticipation → impact → recovery with per-phase durations and eased
  blending (Attack ≈ 120/80/180 ms; Hurt = sharp recoil then settle; Block =
  quick brace; Defeat = stagger → fall, holds; Step poses lean into the real
  position tween from §2; Idle gets a subtle breathing sway).
- **Impact feedback (restrained):** keep the existing damage numbers,
  particles, and damage-scaled camera shake; add ~70 ms hit-stop on the
  struck fighter and a single-frame brightness pop on impact. Everything
  stays distinguishable in grayscale (#214 contract) and never covers the
  HUD or the gap chip. Reduced-motion pins all of it as today.
- **Fighter primacy in the frame:** `FIGHTER_SIZE` 128 → 160 with matched
  staging/ground adjustments; soft ground contact shadows; slightly
  desaturated/dimmed mid-ground behind the fighter zone (background
  regeneration script); slimmer top panels and a 4-line, lower-opacity log so
  UI competes less.

## 7. What this deliberately does not do

- No engine/turn-order/determinism changes beyond the strike-percent refactor.
- No per-fighter engine distance (the shared band stays; per-fighter *staging*
  is presentation).
- No Town implementation (proposal only, phase 6), no new opponents, shops,
  or progression systems.
- No mid-phase baseline captures: all fight/palette/character baselines are
  recaptured **once** in phase 7, after a rebase onto `origin/main`, with
  before/after comparisons taken from the pre-redesign baselines.

## 8. Verification plan

- Per phase: `cargo xtask test logic` while iterating, full
  `cargo xtask pre-push` before each phase commit lands.
- Phase 7: single serialized web-smoke pass with `--update-baselines` over
  the affected scenarios (gold-journey, fight-palette-desktop/phone/
  accessible, high-contrast, reduced-motion-fight, hybrid-2-5d-character,
  romanian-paper-doll-library), plus a before/after screenshot set assembled
  from the old baselines vs the new captures.

## Delivery order

1. this proposal;
2. persistent distance staging (§2);
3. palette + NormalStrike (§3–4);
4. character treatment (§5);
5. animation/feedback (§6);
6. navigation proposal (separate doc);
7. tests, baseline recapture, before/after screenshots.
