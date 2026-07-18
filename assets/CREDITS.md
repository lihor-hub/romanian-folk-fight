# Asset credits

Every asset file in `assets/` is listed here with its source and license.

## Fighter cutout source sheets (`assets/fighters/`)

The fighter cutout source sheets below were generated for this project with
OpenAI image generation from prompt briefs documented in
`docs/superpowers/plans/`, then locally post-processed only to remove
chroma-key backgrounds and resize the resulting PNGs. They are project-owned
generated art and may be replaced by cleaned artist-authored parts.

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `fighters/human/source/human_cutout_parts_v1.png` | Human/player pixel-art cutout body-part source sheet | OpenAI-generated for this project | Same as project assets unless superseded |
| `fighters/human/source/romanian-paper-doll-v1/romanian-paper-doll-v1.png` | Curated contact source for the Haiduc/Cioban production library and five Romanian gear replacements | OpenAI built-in image generation; exact prompts, hashes, helper settings, and source paths in the adjacent README | Same as project assets unless superseded |
| `fighters/gear/source/starter_gear_cutout_parts_v1.png` | Starter gear pixel-art cutout source sheet | OpenAI-generated for this project | Same as project assets unless superseded |
| `fighters/strigoi/source/strigoi_cutout_parts_v1.png` | Strigoi enemy pixel-art cutout body-part source sheet | OpenAI-generated for this project | Same as project assets unless superseded |
| `fighters/zmeu/source/zmeu_cutout_parts_v1.png` | Zmeu boss pixel-art cutout body-part source sheet | OpenAI-generated for this project | Same as project assets unless superseded |

## Fighter runtime parts (`assets/fighters/*/runtime/`)

The runtime PNG parts below are direct crops derived from the credited source
sheets above so the Bevy cutout rig can show production-intent art in creator,
shop, and arena flows without introducing a new production generator pipeline.

### Human runtime parts

The Haiduc, Cioban, and shared production directories are deterministic crops
and exact-alpha derivatives of `romanian-paper-doll-v1`. Their adjacent README
records the complete prompt, extraction map, rejected duplicates, cultural
references, and pan-Romanian remix scope. Every PNG under
`fighters/human/runtime/{haiduc,cioban,shared}/` uses the license below.

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `fighters/human/runtime/foot_back.png` | Derived runtime human cutout part (foot back) | Cropped from `fighters/human/source/human_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/human/runtime/foot_front.png` | Derived runtime human cutout part (foot front) | Cropped from `fighters/human/source/human_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/human/runtime/forearm_back.png` | Derived runtime human cutout part (forearm back) | Cropped from `fighters/human/source/human_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/human/runtime/forearm_front.png` | Derived runtime human cutout part (forearm front) | Cropped from `fighters/human/source/human_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/human/runtime/hair.png` | Derived runtime human cutout part (hair) | Cropped from `fighters/human/source/human_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/human/runtime/hand_back.png` | Derived runtime human cutout part (hand back) | Cropped from `fighters/human/source/human_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/human/runtime/hand_front.png` | Derived runtime human cutout part (hand front) | Cropped from `fighters/human/source/human_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/human/runtime/head.png` | Derived runtime human cutout part (head) | Cropped from `fighters/human/source/human_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/human/runtime/shin_back.png` | Derived runtime human cutout part (shin back) | Cropped from `fighters/human/source/human_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/human/runtime/shin_front.png` | Derived runtime human cutout part (shin front) | Cropped from `fighters/human/source/human_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/human/runtime/thigh_back.png` | Derived runtime human cutout part (thigh back) | Cropped from `fighters/human/source/human_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/human/runtime/thigh_front.png` | Derived runtime human cutout part (thigh front) | Cropped from `fighters/human/source/human_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/human/runtime/torso.png` | Derived runtime human cutout part (torso) | Cropped from `fighters/human/source/human_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/human/runtime/upper_arm_back.png` | Derived runtime human cutout part (upper arm back) | Cropped from `fighters/human/source/human_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/human/runtime/upper_arm_front.png` | Derived runtime human cutout part (upper arm front) | Cropped from `fighters/human/source/human_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/human/runtime/shared/hair_scurt.png` | Shared short-hair albedo | Curated from `romanian-paper-doll-v1` | Same as project assets unless superseded |
| `fighters/human/runtime/shared/hair_scurt_mask.png` | Shared short-hair mask | Deterministically derived from `hair_scurt.png` | Same as project assets unless superseded |
| `fighters/human/runtime/shared/hair_scurt_normal.png` | Shared short-hair normal | Deterministically derived from `hair_scurt.png` | Same as project assets unless superseded |
| `fighters/human/runtime/shared/hair_scurt_shadow.png` | Shared short-hair shadow | Deterministically derived from `hair_scurt.png` | Same as project assets unless superseded |

#### Human hybrid material channels

These technical maps are deterministically derived from the corresponding
runtime albedo by `scripts/generate-human-material-channels.py`; they preserve
the source dimensions and alpha silhouette exactly. The cultural and lighting
reference prompt/source, SHA-256, rights record, and repository-size exclusion
decision are preserved in
`docs/art-references/known-good-human-material-reference.md`.

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `fighters/human/runtime/foot_back_mask.png` | Known-good human mask channel (foot back) | Deterministically derived from `fighters/human/runtime/foot_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/foot_back_normal.png` | Known-good human normal channel (foot back) | Deterministically derived from `fighters/human/runtime/foot_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/foot_back_shadow.png` | Known-good human shadow channel (foot back) | Deterministically derived from `fighters/human/runtime/foot_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/foot_front_mask.png` | Known-good human mask channel (foot front) | Deterministically derived from `fighters/human/runtime/foot_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/foot_front_normal.png` | Known-good human normal channel (foot front) | Deterministically derived from `fighters/human/runtime/foot_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/foot_front_shadow.png` | Known-good human shadow channel (foot front) | Deterministically derived from `fighters/human/runtime/foot_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/forearm_back_mask.png` | Known-good human mask channel (forearm back) | Deterministically derived from `fighters/human/runtime/forearm_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/forearm_back_normal.png` | Known-good human normal channel (forearm back) | Deterministically derived from `fighters/human/runtime/forearm_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/forearm_back_shadow.png` | Known-good human shadow channel (forearm back) | Deterministically derived from `fighters/human/runtime/forearm_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/forearm_front_mask.png` | Known-good human mask channel (forearm front) | Deterministically derived from `fighters/human/runtime/forearm_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/forearm_front_normal.png` | Known-good human normal channel (forearm front) | Deterministically derived from `fighters/human/runtime/forearm_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/forearm_front_shadow.png` | Known-good human shadow channel (forearm front) | Deterministically derived from `fighters/human/runtime/forearm_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/hair_mask.png` | Known-good human mask channel (hair) | Deterministically derived from `fighters/human/runtime/hair.png` | Same as project assets unless superseded |
| `fighters/human/runtime/hair_normal.png` | Known-good human normal channel (hair) | Deterministically derived from `fighters/human/runtime/hair.png` | Same as project assets unless superseded |
| `fighters/human/runtime/hair_shadow.png` | Known-good human shadow channel (hair) | Deterministically derived from `fighters/human/runtime/hair.png` | Same as project assets unless superseded |
| `fighters/human/runtime/hand_back_mask.png` | Known-good human mask channel (hand back) | Deterministically derived from `fighters/human/runtime/hand_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/hand_back_normal.png` | Known-good human normal channel (hand back) | Deterministically derived from `fighters/human/runtime/hand_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/hand_back_shadow.png` | Known-good human shadow channel (hand back) | Deterministically derived from `fighters/human/runtime/hand_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/hand_front_mask.png` | Known-good human mask channel (hand front) | Deterministically derived from `fighters/human/runtime/hand_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/hand_front_normal.png` | Known-good human normal channel (hand front) | Deterministically derived from `fighters/human/runtime/hand_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/hand_front_shadow.png` | Known-good human shadow channel (hand front) | Deterministically derived from `fighters/human/runtime/hand_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/head_mask.png` | Known-good human mask channel (head) | Deterministically derived from `fighters/human/runtime/head.png` | Same as project assets unless superseded |
| `fighters/human/runtime/head_normal.png` | Known-good human normal channel (head) | Deterministically derived from `fighters/human/runtime/head.png` | Same as project assets unless superseded |
| `fighters/human/runtime/head_shadow.png` | Known-good human shadow channel (head) | Deterministically derived from `fighters/human/runtime/head.png` | Same as project assets unless superseded |
| `fighters/human/runtime/shin_back_mask.png` | Known-good human mask channel (shin back) | Deterministically derived from `fighters/human/runtime/shin_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/shin_back_normal.png` | Known-good human normal channel (shin back) | Deterministically derived from `fighters/human/runtime/shin_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/shin_back_shadow.png` | Known-good human shadow channel (shin back) | Deterministically derived from `fighters/human/runtime/shin_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/shin_front_mask.png` | Known-good human mask channel (shin front) | Deterministically derived from `fighters/human/runtime/shin_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/shin_front_normal.png` | Known-good human normal channel (shin front) | Deterministically derived from `fighters/human/runtime/shin_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/shin_front_shadow.png` | Known-good human shadow channel (shin front) | Deterministically derived from `fighters/human/runtime/shin_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/thigh_back_mask.png` | Known-good human mask channel (thigh back) | Deterministically derived from `fighters/human/runtime/thigh_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/thigh_back_normal.png` | Known-good human normal channel (thigh back) | Deterministically derived from `fighters/human/runtime/thigh_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/thigh_back_shadow.png` | Known-good human shadow channel (thigh back) | Deterministically derived from `fighters/human/runtime/thigh_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/thigh_front_mask.png` | Known-good human mask channel (thigh front) | Deterministically derived from `fighters/human/runtime/thigh_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/thigh_front_normal.png` | Known-good human normal channel (thigh front) | Deterministically derived from `fighters/human/runtime/thigh_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/thigh_front_shadow.png` | Known-good human shadow channel (thigh front) | Deterministically derived from `fighters/human/runtime/thigh_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/torso_mask.png` | Known-good human mask channel (torso) | Deterministically derived from `fighters/human/runtime/torso.png` | Same as project assets unless superseded |
| `fighters/human/runtime/torso_normal.png` | Known-good human normal channel (torso) | Deterministically derived from `fighters/human/runtime/torso.png` | Same as project assets unless superseded |
| `fighters/human/runtime/torso_shadow.png` | Known-good human shadow channel (torso) | Deterministically derived from `fighters/human/runtime/torso.png` | Same as project assets unless superseded |
| `fighters/human/runtime/upper_arm_back_mask.png` | Known-good human mask channel (upper arm back) | Deterministically derived from `fighters/human/runtime/upper_arm_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/upper_arm_back_normal.png` | Known-good human normal channel (upper arm back) | Deterministically derived from `fighters/human/runtime/upper_arm_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/upper_arm_back_shadow.png` | Known-good human shadow channel (upper arm back) | Deterministically derived from `fighters/human/runtime/upper_arm_back.png` | Same as project assets unless superseded |
| `fighters/human/runtime/upper_arm_front_mask.png` | Known-good human mask channel (upper arm front) | Deterministically derived from `fighters/human/runtime/upper_arm_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/upper_arm_front_normal.png` | Known-good human normal channel (upper arm front) | Deterministically derived from `fighters/human/runtime/upper_arm_front.png` | Same as project assets unless superseded |
| `fighters/human/runtime/upper_arm_front_shadow.png` | Known-good human shadow channel (upper arm front) | Deterministically derived from `fighters/human/runtime/upper_arm_front.png` | Same as project assets unless superseded |

### Strigoi runtime parts

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `fighters/strigoi/runtime/foot_back.png` | Derived runtime strigoi cutout part (foot back) | Cropped from `fighters/strigoi/source/strigoi_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/strigoi/runtime/foot_front.png` | Derived runtime strigoi cutout part (foot front) | Cropped from `fighters/strigoi/source/strigoi_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/strigoi/runtime/forearm_back.png` | Derived runtime strigoi cutout part (forearm back) | Cropped from `fighters/strigoi/source/strigoi_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/strigoi/runtime/forearm_front.png` | Derived runtime strigoi cutout part (forearm front) | Cropped from `fighters/strigoi/source/strigoi_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/strigoi/runtime/hand_back.png` | Derived runtime strigoi cutout part (hand back) | Cropped from `fighters/strigoi/source/strigoi_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/strigoi/runtime/hand_front.png` | Derived runtime strigoi cutout part (hand front) | Cropped from `fighters/strigoi/source/strigoi_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/strigoi/runtime/head.png` | Derived runtime strigoi cutout part (head) | Cropped from `fighters/strigoi/source/strigoi_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/strigoi/runtime/shin_back.png` | Derived runtime strigoi cutout part (shin back) | Cropped from `fighters/strigoi/source/strigoi_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/strigoi/runtime/shin_front.png` | Derived runtime strigoi cutout part (shin front) | Cropped from `fighters/strigoi/source/strigoi_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/strigoi/runtime/thigh_back.png` | Derived runtime strigoi cutout part (thigh back) | Cropped from `fighters/strigoi/source/strigoi_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/strigoi/runtime/thigh_front.png` | Derived runtime strigoi cutout part (thigh front) | Cropped from `fighters/strigoi/source/strigoi_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/strigoi/runtime/torso.png` | Derived runtime strigoi cutout part (torso) | Cropped from `fighters/strigoi/source/strigoi_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/strigoi/runtime/upper_arm_back.png` | Derived runtime strigoi cutout part (upper arm back) | Cropped from `fighters/strigoi/source/strigoi_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/strigoi/runtime/upper_arm_front.png` | Derived runtime strigoi cutout part (upper arm front) | Cropped from `fighters/strigoi/source/strigoi_cutout_parts_v1.png` | Same as project assets unless superseded |

### Zmeu runtime parts

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `fighters/zmeu/runtime/foot_back.png` | Derived runtime zmeu cutout part (foot back) | Cropped from `fighters/zmeu/source/zmeu_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/zmeu/runtime/foot_front.png` | Derived runtime zmeu cutout part (foot front) | Cropped from `fighters/zmeu/source/zmeu_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/zmeu/runtime/forearm_back.png` | Derived runtime zmeu cutout part (forearm back) | Cropped from `fighters/zmeu/source/zmeu_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/zmeu/runtime/forearm_front.png` | Derived runtime zmeu cutout part (forearm front) | Cropped from `fighters/zmeu/source/zmeu_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/zmeu/runtime/hand_back.png` | Derived runtime zmeu cutout part (hand back) | Cropped from `fighters/zmeu/source/zmeu_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/zmeu/runtime/hand_front.png` | Derived runtime zmeu cutout part (hand front) | Cropped from `fighters/zmeu/source/zmeu_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/zmeu/runtime/head.png` | Derived runtime zmeu cutout part (head) | Cropped from `fighters/zmeu/source/zmeu_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/zmeu/runtime/shin_back.png` | Derived runtime zmeu cutout part (shin back) | Cropped from `fighters/zmeu/source/zmeu_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/zmeu/runtime/shin_front.png` | Derived runtime zmeu cutout part (shin front) | Cropped from `fighters/zmeu/source/zmeu_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/zmeu/runtime/thigh_back.png` | Derived runtime zmeu cutout part (thigh back) | Cropped from `fighters/zmeu/source/zmeu_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/zmeu/runtime/thigh_front.png` | Derived runtime zmeu cutout part (thigh front) | Cropped from `fighters/zmeu/source/zmeu_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/zmeu/runtime/torso.png` | Derived runtime zmeu cutout part (torso) | Cropped from `fighters/zmeu/source/zmeu_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/zmeu/runtime/upper_arm_back.png` | Derived runtime zmeu cutout part (upper arm back) | Cropped from `fighters/zmeu/source/zmeu_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/zmeu/runtime/upper_arm_front.png` | Derived runtime zmeu cutout part (upper arm front) | Cropped from `fighters/zmeu/source/zmeu_cutout_parts_v1.png` | Same as project assets unless superseded |

### Gear runtime parts

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `fighters/gear/runtime/bata_ciobaneasca.png` | Production bâtă ciobănească attachment | Curated from `romanian-paper-doll-v1` | Same as project assets unless superseded |
| `fighters/gear/runtime/caciula_de_oaie.png` | Production căciulă de oaie attachment | Curated from `romanian-paper-doll-v1` | Same as project assets unless superseded |
| `fighters/gear/runtime/camasa_de_zale.png` | Derived runtime starter gear asset (camasa de zale) | Cropped from `fighters/gear/source/starter_gear_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/cizme_de_voinic.png` | Derived runtime starter gear asset (cizme de voinic) | Cropped from `fighters/gear/source/starter_gear_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/coif_de_ostean.png` | Derived runtime starter gear asset (coif de ostean) | Cropped from `fighters/gear/source/starter_gear_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/cojoc_gros.png` | Production cojoc gros attachment | Curated from `romanian-paper-doll-v1` | Same as project assets unless superseded |
| `fighters/gear/runtime/ie_descantata.png` | Derived runtime starter gear asset (ie descantata) | Cropped from `fighters/gear/source/starter_gear_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/opinci_iuti.png` | Production opinci iuți attachment | Curated from `romanian-paper-doll-v1` | Same as project assets unless superseded |
| `fighters/gear/runtime/palos.png` | Derived runtime starter gear asset (palos) | Cropped from `fighters/gear/source/starter_gear_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/scut_de_lemn.png` | Derived runtime starter gear asset (scut de lemn) | Cropped from `fighters/gear/source/starter_gear_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/scut_ferecat.png` | Derived runtime starter gear asset (scut ferecat) | Cropped from `fighters/gear/source/starter_gear_cutout_parts_v1.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/topor_de_padurar.png` | Production topor de pădurar attachment | Curated from `romanian-paper-doll-v1` | Same as project assets unless superseded |

#### Romanian production gear material channels

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `fighters/gear/runtime/bata_ciobaneasca_mask.png` | Bâtă semantic mask | Deterministically derived from `bata_ciobaneasca.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/bata_ciobaneasca_normal.png` | Bâtă shallow normal | Deterministically derived from `bata_ciobaneasca.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/bata_ciobaneasca_shadow.png` | Bâtă local-depth map | Deterministically derived from `bata_ciobaneasca.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/topor_de_padurar_mask.png` | Topor semantic mask | Deterministically derived from `topor_de_padurar.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/topor_de_padurar_normal.png` | Topor shallow normal | Deterministically derived from `topor_de_padurar.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/topor_de_padurar_shadow.png` | Topor local-depth map | Deterministically derived from `topor_de_padurar.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/cojoc_gros_mask.png` | Cojoc semantic mask | Deterministically derived from `cojoc_gros.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/cojoc_gros_normal.png` | Cojoc shallow normal | Deterministically derived from `cojoc_gros.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/cojoc_gros_shadow.png` | Cojoc local-depth map | Deterministically derived from `cojoc_gros.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/caciula_de_oaie_mask.png` | Căciulă semantic mask | Deterministically derived from `caciula_de_oaie.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/caciula_de_oaie_normal.png` | Căciulă shallow normal | Deterministically derived from `caciula_de_oaie.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/caciula_de_oaie_shadow.png` | Căciulă local-depth map | Deterministically derived from `caciula_de_oaie.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/opinci_iuti_mask.png` | Opinci semantic mask | Deterministically derived from `opinci_iuti.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/opinci_iuti_normal.png` | Opinci shallow normal | Deterministically derived from `opinci_iuti.png` | Same as project assets unless superseded |
| `fighters/gear/runtime/opinci_iuti_shadow.png` | Opinci local-depth map | Deterministically derived from `opinci_iuti.png` | Same as project assets unless superseded |

## UI presentation source sheets (`assets/ui/source/`)

The UI presentation source sheet below was generated for this project with
OpenAI image generation from the prompt brief documented in
`docs/superpowers/plans/2026-07-08-1554-next-pixel-art-asset-batch.md`, then
locally post-processed only to remove the chroma-key background and resize the
resulting PNG. It is project-owned generated art and may be replaced by cleaned
artist-authored parts.

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `ui/source/ui_presentation_motifs_v1.png` | Pixel-art UI presentation motifs and HUD frame source sheet | OpenAI-generated for this project | Same as project assets unless superseded |

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

## Shop icons (`assets/ui/`)

The shop icon sprites below are **self-generated placeholder art** produced
by `scripts/generate-shop-icons.py` for this project (issue #73), following
`docs/art-direction.md`. They are dedicated to the public domain
(**CC0 1.0**, <https://creativecommons.org/publicdomain/zero/1.0/>).

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `ui/icon_coin.png` | Galbeni / wallet icon | self-generated | CC0 1.0 |
| `ui/icon_weapon.png` | Weapon slot icon | self-generated | CC0 1.0 |
| `ui/icon_shield.png` | Shield slot icon | self-generated | CC0 1.0 |
| `ui/icon_torso.png` | Torso armor slot icon | self-generated | CC0 1.0 |
| `ui/icon_head.png` | Head armor slot icon | self-generated | CC0 1.0 |
| `ui/icon_feet.png` | Feet armor slot icon | self-generated | CC0 1.0 |

## Equipment overlays (`assets/gear/`)

All equipment overlay sprites below are **self-generated placeholder art**
produced by `scripts/generate-gear-visuals.py` for this project (issue #72),
following `docs/art-direction.md`. They are transparent 128x128 layers aligned
to the fighter frame and dedicated to the public domain (**CC0 1.0**,
<https://creativecommons.org/publicdomain/zero/1.0/>).

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `gear/bata_ciobaneasca.png` | Bâtă ciobănească equipment overlay | self-generated | CC0 1.0 |
| `gear/topor_de_padurar.png` | Topor de pădurar equipment overlay | self-generated | CC0 1.0 |
| `gear/palos.png` | Paloș equipment overlay | self-generated | CC0 1.0 |
| `gear/buzdugan_cu_trei_peceti.png` | Buzdugan cu trei peceți equipment overlay | self-generated | CC0 1.0 |
| `gear/scut_de_lemn.png` | Scut de lemn equipment overlay | self-generated | CC0 1.0 |
| `gear/scut_ferecat.png` | Scut ferecat equipment overlay | self-generated | CC0 1.0 |
| `gear/ie_descantata.png` | Ie descântată equipment overlay | self-generated | CC0 1.0 |
| `gear/cojoc_gros.png` | Cojoc gros equipment overlay | self-generated | CC0 1.0 |
| `gear/camasa_de_zale.png` | Cămașă de zale equipment overlay | self-generated | CC0 1.0 |
| `gear/caciula_de_oaie.png` | Căciulă de oaie equipment overlay | self-generated | CC0 1.0 |
| `gear/coif_de_ostean.png` | Coif de oștean equipment overlay | self-generated | CC0 1.0 |
| `gear/opinci_iuti.png` | Opinci iuți equipment overlay | self-generated | CC0 1.0 |
| `gear/cizme_de_voinic.png` | Cizme de voinic equipment overlay | self-generated | CC0 1.0 |

## Arena backgrounds (`assets/backgrounds/`)

All parallax background layers below are **self-generated placeholder art**
produced by `scripts/generate-backgrounds.py` for this project (issue #23),
with foreground depth added for issue #74, following `docs/art-direction.md`.
They are dedicated to the public domain
(**CC0 1.0**, <https://creativecommons.org/publicdomain/zero/1.0/>) and are
pending replacement by bespoke final art.

| File | Depicts | Source | License |
| --- | --- | --- | --- |
| `backgrounds/village_far.png` | Sat românesc — dusk sky, hills, cottages (far layer) | self-generated | CC0 1.0 |
| `backgrounds/village_near.png` | Sat românesc — wooden fence, haystacks (near layer) | self-generated | CC0 1.0 |
| `backgrounds/village_foreground.png` | Sat românesc — plank stage edge, posts, crowd silhouettes (foreground depth) | self-generated | CC0 1.0 |
| `backgrounds/forest_far.png` | Pădurea întunecată — moonlit fir silhouettes (far layer) | self-generated | CC0 1.0 |
| `backgrounds/forest_near.png` | Pădurea întunecată — trunks, canopy, ferns (near layer) | self-generated | CC0 1.0 |
| `backgrounds/forest_foreground.png` | Pădurea întunecată — roots, moss, and stones (foreground depth) | self-generated | CC0 1.0 |
| `backgrounds/mountains_far.png` | Munții Carpați — peaks, fortress silhouette (far layer) | self-generated | CC0 1.0 |
| `backgrounds/mountains_near.png` | Munții Carpați — rocky outcrops of the pass (near layer) | self-generated | CC0 1.0 |
| `backgrounds/mountains_foreground.png` | Munții Carpați — carved stone arena lip and snow banks (foreground depth) | self-generated | CC0 1.0 |

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
