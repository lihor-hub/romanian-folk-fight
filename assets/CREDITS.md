# Asset credits

Every asset file in `assets/` is listed here with its source and license.

## Sprites (`assets/sprites/`)

All sprite sheets below are **self-generated placeholder art** produced by
`scripts/generate-placeholder-sprites.py` for this project, following
`docs/art-direction.md`. They are dedicated to the public domain
(**CC0 1.0**, <https://creativecommons.org/publicdomain/zero/1.0/>) and are
pending replacement by bespoke final art.

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `sprites/player.png` | Player fighter (Făt-Frumos) | self-generated | CC0 1.0 |
| `sprites/hot_de_codru.png` | Hoț de codru | self-generated | CC0 1.0 |
| `sprites/strigoi.png` | Strigoi | self-generated | CC0 1.0 |
| `sprites/varcolac.png` | Vârcolac | self-generated | CC0 1.0 |
| `sprites/capcaun.png` | Căpcăun | self-generated | CC0 1.0 |
| `sprites/muma_padurii.png` | Muma Pădurii | self-generated | CC0 1.0 |
| `sprites/iele.png` | Iele | self-generated | CC0 1.0 |
| `sprites/solomonar.png` | Solomonar | self-generated | CC0 1.0 |
| `sprites/balaur.png` | Balaur cu trei capete | self-generated | CC0 1.0 |
| `sprites/zmeu.png` | Zmeu | self-generated | CC0 1.0 |
| `sprites/zmeul_zmeilor.png` | Zmeul Zmeilor | self-generated | CC0 1.0 |

## UI panel border (`assets/ui/`)

The embroidery-motif 9-slice panel border below is **self-generated
placeholder art** produced by `scripts/generate-ui-panel.py` for this
project (issue #28: UI reskin), following `docs/art-direction.md` — a
geometric cross-stitch (ii) diamond motif in gold on a deep-red band, framed
by black corners, around a dark translucent center. It is dedicated to the
public domain (**CC0 1.0**,
<https://creativecommons.org/publicdomain/zero/1.0/>) and is pending
replacement by bespoke final art.

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `ui/panel_border.png` | 9-slice embroidery panel border | self-generated | CC0 1.0 |

## Arena backgrounds (`assets/backgrounds/`)

All parallax background layers below are **self-generated placeholder art**
produced by `scripts/generate-backgrounds.py` for this project (issue #23),
following `docs/art-direction.md`. They are dedicated to the public domain
(**CC0 1.0**, <https://creativecommons.org/publicdomain/zero/1.0/>) and are
pending replacement by bespoke final art.

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `backgrounds/village_far.png` | Sat românesc — dusk sky, hills, cottages (far layer) | self-generated | CC0 1.0 |
| `backgrounds/village_near.png` | Sat românesc — wooden fence, haystacks (near layer) | self-generated | CC0 1.0 |
| `backgrounds/forest_far.png` | Pădurea întunecată — moonlit fir silhouettes (far layer) | self-generated | CC0 1.0 |
| `backgrounds/forest_near.png` | Pădurea întunecată — trunks, canopy, ferns (near layer) | self-generated | CC0 1.0 |
| `backgrounds/mountains_far.png` | Munții Carpați — peaks, fortress silhouette (far layer) | self-generated | CC0 1.0 |
| `backgrounds/mountains_near.png` | Munții Carpați — rocky outcrops of the pass (near layer) | self-generated | CC0 1.0 |

## Audio (music and sound effects)

All audio below is **self-generated placeholder sound** synthesized from
scratch by `scripts/generate-audio.py` (numpy + soundfile, no samples or
external recordings). It is dedicated to the public domain (**CC0 1.0**,
<https://creativecommons.org/publicdomain/zero/1.0/>) and is pending
replacement by bespoke final audio.

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `audio/music_menu.ogg` | Menu theme (slow doina-like drone + melody loop) | self-generated | CC0 1.0 |
| `audio/music_arena.ogg` | Arena theme (upbeat hora-like loop) | self-generated | CC0 1.0 |
| `audio/music_boss.ogg` | Boss theme (ominous low-drone loop) | self-generated | CC0 1.0 |
| `audio/sfx_hit.ogg` | Sword hit | self-generated | CC0 1.0 |
| `audio/sfx_crit.ogg` | Critical hit (hit + metallic ring) | self-generated | CC0 1.0 |
| `audio/sfx_block.ogg` | Block / guard (wood thunk) | self-generated | CC0 1.0 |
| `audio/sfx_whoosh.ogg` | Missed strike whoosh | self-generated | CC0 1.0 |
| `audio/sfx_rest.ogg` | Rest (recovering breath) | self-generated | CC0 1.0 |
| `audio/sfx_fail.ogg` | Out-of-stamina thud | self-generated | CC0 1.0 |
| `audio/sfx_defeated.ogg` | Fight-ending blow | self-generated | CC0 1.0 |
| `audio/sfx_click.ogg` | UI button click | self-generated | CC0 1.0 |
| `audio/sfx_coin.ogg` | Purchase coin jingle | self-generated | CC0 1.0 |
| `audio/sting_victory.ogg` | Victory sting | self-generated | CC0 1.0 |
| `audio/sting_defeat.ogg` | Defeat sting | self-generated | CC0 1.0 |

## Web page icons and social card (issue #32)

- `web/favicon.svg`, `web/favicon-32.png`, `web/apple-touch-icon.png`, `web/og-image.png` — self-generated for this project (hand-written SVG shield/embroidery mark in the game palette; PNGs rendered from the SVGs with rsvg-convert). No third-party assets. License: same as the project.

## Fonts (`assets/fonts/`)

| File | Font | Source | License |
| --- | --- | --- | --- |
| `fonts/Alegreya-Variable.ttf` | Alegreya (variable, wght 400–900) | [google/fonts `ofl/alegreya`](https://github.com/google/fonts/tree/main/ofl/alegreya), © 2011 The Alegreya Project Authors | [SIL OFL 1.1](fonts/OFL-Alegreya.txt) |

Alegreya was chosen for its rustic serif feel and confirmed Latin Extended-B
coverage: the cmap contains the Romanian comma-below letters Ș/ș/Ț/ț
(U+0218–U+021B) as well as Ă/ă, Â/â, Î/î (verified with fontTools; see PR #27).
The full OFL license text ships alongside the font as `fonts/OFL-Alegreya.txt`.
