//! Character creation screen: choose a folklore preset or a custom path,
//! edit attributes and appearance, preview the cutout rig, and enter the
//! arena.
//!
//! The allocation rules live in [`draft`] as pure logic; this module only
//! wires them to Bevy UI following the button pattern from the main menu.

pub mod draft;

use bevy::prelude::*;
use bevy::ui::UiSystems;

pub use draft::{AttributeKind, CharacterDraft, FOLK_NAMES, FREE_POINTS, HeroChoice, HeroPreset};

use crate::character::{Attributes, CharacterDefinition, PlayerAppearance, stats};
use crate::core::{
    GameState, LetterboxRect, UiFont, despawn_screen, letterbox_zoom, logical_node_rect,
    world_point_for_screen_point,
};
#[cfg(test)]
use crate::core::{ViewportInfo, screen_point_for_world_point};
use crate::cutout::{CutoutRig, resolve_human_character, spawn_character_rig};
use crate::flow::FlowIntent;
use crate::items::Equipment;
use crate::menu::DisabledButton;
use crate::save::{ResumeDestination, SaveRequested};
use crate::shop::{OwnedItems, PlayerEquipment};
use crate::theme::{
    ARENA_BROWN, BUTTON_DISABLED, BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED, CREAM, GOLD,
    MIN_TOUCH_TARGET, PANEL_LINEN, PanelTexture, TEXT_DISABLED, WALNUT, panel_bundle,
};
use crate::ui_widgets::focus::{FocusNavigationPlugin, FocusNavigationSet, TabGroup};
use crate::ui_widgets::{
    attribute_row::spawn_attribute_row, button_bundle, scroll_with_wheel_and_touch, small_button,
};

#[cfg(test)]
const CREATION_TARGET_WIDTH: f32 = 800.0;
const CREATION_ROOT_PADDING: f32 = 14.0;
const CREATION_BODY_WIDTH: f32 = 760.0;
const CREATION_BODY_GAP: f32 = 12.0;
const CREATION_PREVIEW_STAGE_WIDTH: f32 = 392.0;
const CREATION_CONTROL_DECK_WIDTH: f32 = 356.0;
const CREATION_PANEL_HEIGHT: f32 = 482.0;
const CREATION_PREVIEW_FRAME_HEIGHT: f32 = 318.0;
const CREATION_PREVIEW_SCALE: f32 = 1.02;
const CREATION_PREVIEW_Y: f32 = -18.0;
const PREVIEW_Z: f32 = 25.0;

/// The confirmed player character: chosen name, final attributes, legacy
/// appearance projection, and stable resolved identity. Written by the
/// confirm button and read by the fight screen.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct PlayerCharacter {
    pub name: String,
    pub attributes: Attributes,
    pub appearance: PlayerAppearance,
    pub definition: CharacterDefinition,
}

/// Marker for the creation-screen root; everything under it is despawned by
/// [`despawn_screen`] on `OnExit(GameState::CharacterCreation)`.
#[derive(Component)]
struct CreationScreen;

/// Marker for the cutout preview root so resource changes can re-render it.
#[derive(Component)]
struct CreationPreview;

/// Stable layout anchors for the preview-first creator screen.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum CreationLayoutRole {
    PreviewStage,
    ControlDeck,
    PresetGrid,
    AppearanceControls,
    AttributeControls,
    StatStrip,
}

/// Which appearance selector row a label or button belongs to.
/// `pub(crate)` only because [`CreationAction`] (whose visibility the
/// `review` seam needs, see its doc comment) carries it in two variants;
/// nothing outside this module uses it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppearanceField {
    SkinTone,
    Build,
    Hair,
    Accent,
}

impl AppearanceField {
    fn label(self) -> &'static str {
        match self {
            Self::SkinTone => "Piele",
            Self::Build => "Trup",
            Self::Hair => "Păr",
            Self::Accent => "Accent",
        }
    }
}

/// What a creation-screen button does when pressed. `pub(crate)` (rather
/// than private) solely so the `review`-feature seam (`src/review/mod.rs`,
/// #187) can locate the Confirm/Back buttons and press them exactly like a
/// player's click; nothing outside this module constructs or matches it
/// otherwise.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CreationAction {
    SelectChoice(HeroChoice),
    PreviousName,
    NextName,
    Increase(AttributeKind),
    Decrease(AttributeKind),
    PreviousAppearance(AppearanceField),
    NextAppearance(AppearanceField),
    Confirm,
    Back,
}

/// Which piece of draft state a text label displays; one generic system
/// refreshes all of them whenever the draft changes.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum CreationLabel {
    Description,
    Name,
    Points,
    Value(AttributeKind),
    Appearance(AppearanceField),
    PreviewStat(PreviewStat),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewStat {
    Health,
    Stamina,
    Damage,
}

impl PreviewStat {
    const ALL: [Self; 3] = [Self::Health, Self::Stamina, Self::Damage];
}

pub struct CreationPlugin;

impl Plugin for CreationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SaveRequested>()
            .add_message::<FlowIntent>()
            .init_resource::<CharacterDraft>()
            .add_plugins((crate::ui_widgets::ScrollInputPlugin, FocusNavigationPlugin))
            .add_systems(OnEnter(GameState::CharacterCreation), spawn_creation_screen)
            .add_systems(
                Update,
                (
                    handle_creation_actions
                        .in_set(crate::flow::FlowIntentEmission)
                        .after(FocusNavigationSet),
                    update_button_backgrounds,
                    update_control_availability,
                    update_choice_button_styles,
                    update_labels.run_if(resource_changed::<CharacterDraft>),
                    refresh_preview_rig.run_if(resource_changed::<CharacterDraft>),
                    scroll_with_wheel_and_touch,
                )
                    .chain()
                    .run_if(in_state(GameState::CharacterCreation)),
            )
            .add_systems(
                PostUpdate,
                update_preview_transform
                    .after(UiSystems::Layout)
                    // So this frame's placement is reflected in
                    // `GlobalTransform` (and thus rendered) this same frame,
                    // rather than merely being ordered after layout with no
                    // guarantee relative to transform propagation.
                    .before(bevy::transform::TransformSystems::Propagate)
                    .run_if(in_state(GameState::CharacterCreation)),
            )
            .add_systems(
                OnExit(GameState::CharacterCreation),
                despawn_screen::<CreationScreen>,
            );
    }
}

fn spawn_creation_screen(
    mut commands: Commands,
    draft: Res<CharacterDraft>,
    ui_font: Res<UiFont>,
    panel_texture: Res<PanelTexture>,
    asset_server: Option<Res<AssetServer>>,
) {
    commands.spawn((
        CreationScreen,
        Sprite::from_color(ARENA_BROWN, Vec2::new(800.0, 600.0)),
        Transform::from_xyz(0.0, 0.0, -40.0),
    ));

    commands
        .spawn((
            CreationScreen,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Center,
                row_gap: Val::Px(8.0),
                padding: UiRect::all(Val::Px(CREATION_ROOT_PADDING)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            crate::ui_widgets::Scrollable,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Creează-ți eroul"),
                ui_font.text_font_bold(38.0),
                TextColor(CREAM),
                Node {
                    margin: UiRect::bottom(Val::Px(4.0)),
                    ..default()
                },
            ));

            parent
                .spawn(Node {
                    width: Val::Px(CREATION_BODY_WIDTH),
                    max_width: Val::Percent(94.0),
                    min_height: Val::Px(CREATION_PANEL_HEIGHT),
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Stretch,
                    column_gap: Val::Px(CREATION_BODY_GAP),
                    row_gap: Val::Px(10.0),
                    ..default()
                })
                .with_children(|body| {
                    spawn_preview_stage(body, &draft, &ui_font);
                    spawn_control_deck(body, &draft, &ui_font, &panel_texture);
                });
        });

    // Placeholder until `update_preview_transform` places it for real:
    // `PreviewStage`'s `ComputedNode`/`UiGlobalTransform` don't exist until
    // Bevy's own UI layout pass runs, which only happens later this same
    // frame (`PostUpdate`, after `OnEnter` has been applied) -- see that
    // system's doc comment (#123).
    let preview = commands
        .spawn((
            CreationScreen,
            CreationPreview,
            Transform::from_xyz(0.0, CREATION_PREVIEW_Y, PREVIEW_Z)
                .with_scale(Vec3::splat(CREATION_PREVIEW_SCALE)),
        ))
        .id();
    let equipment = equipment_from_items(draft.starter_items());
    let resolved = resolve_human_character(&draft.definition())
        .expect("the creation draft resolves against the bundled human catalog");
    spawn_character_rig(
        &mut commands,
        preview,
        &resolved,
        asset_server.as_deref(),
        false,
        Some(&equipment),
        None,
    );
}

/// Spawns the preview stage's own layout container plus its three rows
/// (name, cutout frame, stat strip). Unlike every other panel on this
/// screen, this one deliberately does **not** use [`panel_bundle`]: the
/// world-space cutout rig it frames (spawned separately by
/// [`spawn_creation_screen`], positioned by [`update_preview_transform`])
/// is rendered by the *world* camera, composited underneath the *UI*
/// camera's output -- so any opaque UI background covering the frame's rect
/// would hide the rig completely, no matter how the rig itself is
/// positioned or scaled (#123). `panel_bundle`'s 9-slice image always fills
/// its *entire* node, so giving the frame row itself (or an ancestor
/// spanning it) that treatment would paint right over the rig. Instead:
/// the outer stage is a plain transparent layout container, the frame row
/// stays borderless-fill (just its existing `BorderColor` outline) so the
/// rig shows through untinted, and only the name/stat rows -- which never
/// overlap the frame -- keep a `PANEL_LINEN` backing for legibility.
fn spawn_preview_stage(
    parent: &mut ChildSpawnerCommands,
    draft: &CharacterDraft,
    ui_font: &UiFont,
) {
    parent
        .spawn((
            Node {
                width: Val::Px(CREATION_PREVIEW_STAGE_WIDTH),
                max_width: Val::Percent(100.0),
                min_height: Val::Px(CREATION_PANEL_HEIGHT),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(Val::Px(18.0)),
                ..default()
            },
            CreationLayoutRole::PreviewStage,
        ))
        .with_children(|stage| {
            stage.spawn((
                Text::new(draft.name()),
                ui_font.text_font_bold(30.0),
                TextColor(CREAM),
                BackgroundColor(PANEL_LINEN),
                CreationLabel::Name,
            ));
            // The cutout "window": no fill of its own, so the world-space
            // rig underneath (positioned to land here, see
            // `update_preview_transform`) renders unobscured. Only the
            // gold outline remains, framing the rig without a PNG asset
            // that would need a genuinely transparent center (#123).
            stage.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(CREATION_PREVIEW_FRAME_HEIGHT),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BorderColor::all(GOLD),
            ));
            stage
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(6.0),
                        padding: UiRect::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(PANEL_LINEN),
                    CreationLayoutRole::StatStrip,
                ))
                .with_children(|strip| {
                    for stat in PreviewStat::ALL {
                        strip.spawn((
                            Node {
                                width: Val::Px(92.0),
                                height: Val::Px(36.0),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BackgroundColor(WALNUT),
                            BorderColor::all(GOLD),
                            children![(
                                Text::new(preview_stat_text(draft, stat)),
                                ui_font.text_font(15.0),
                                TextColor(CREAM),
                                CreationLabel::PreviewStat(stat),
                            )],
                        ));
                    }
                });
        });
}

fn spawn_control_deck(
    parent: &mut ChildSpawnerCommands,
    draft: &CharacterDraft,
    ui_font: &UiFont,
    panel_texture: &PanelTexture,
) {
    parent
        .spawn((
            panel_bundle(
                panel_texture,
                Node {
                    width: Val::Px(CREATION_CONTROL_DECK_WIDTH),
                    max_width: Val::Percent(100.0),
                    min_height: Val::Px(CREATION_PANEL_HEIGHT),
                    flex_direction: FlexDirection::Column,
                    // #128: 8 -> 6 to help the taller attribute grid fit.
                    row_gap: Val::Px(6.0),
                    padding: UiRect::all(Val::Px(16.0)),
                    ..default()
                },
            ),
            BackgroundColor(PANEL_LINEN),
            CreationLayoutRole::ControlDeck,
            // #216: one shared focus region for the whole control deck --
            // see `crate::ui_widgets::focus`'s registration API.
            TabGroup::new(0),
        ))
        .with_children(|deck| {
            deck.spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                CreationLayoutRole::PresetGrid,
            ))
            .with_children(|row| {
                for choice in HeroChoice::ALL {
                    row.spawn((
                        // #216: height `MIN_TOUCH_TARGET` (44) keeps the
                        // touch-target floor. #128: width 118 -> 96 (and the
                        // label 15 -> 13) so three tiles per row fit the
                        // deck's 308px inner width (3 * 96 + 2 * 6 = 300),
                        // freeing the vertical room the four new attribute
                        // cells need.
                        button_bundle(
                            choice.label(),
                            Val::Px(96.0),
                            Val::Px(MIN_TOUCH_TARGET),
                            13.0,
                            ui_font,
                        ),
                        CreationAction::SelectChoice(choice),
                    ));
                }
            });

            deck.spawn((
                Text::new(description_text(draft)),
                ui_font.text_font(15.0),
                TextColor(CREAM),
                CreationLabel::Description,
                Node {
                    width: Val::Percent(100.0),
                    // #128: 46 -> 40; two 15px lines still fit.
                    min_height: Val::Px(40.0),
                    ..default()
                },
            ));

            deck.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(10.0),
                ..default()
            })
            .with_children(|row| {
                row.spawn((small_button("<", ui_font), CreationAction::PreviousName));
                // #216: 232 -> 192 (and the name font 24 -> 20) so the row
                // fits the deck's inner width without flex-squeezing the
                // 48x48 arrows below the touch-target floor -- buttons no
                // longer shrink at all (see
                // `ui_widgets::labeled_button_bundle`), so the row must
                // genuinely fit. "Ileana Cosânzeana"/"Ucenicul Solomonar"
                // still render on one line at 20px.
                row.spawn(Node {
                    width: Val::Px(192.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                })
                .with_children(|slot| {
                    slot.spawn((
                        Text::new(draft.name()),
                        ui_font.text_font(20.0),
                        TextColor(CREAM),
                        CreationLabel::Name,
                    ));
                });
                row.spawn((small_button(">", ui_font), CreationAction::NextName));
            });

            deck.spawn((
                Text::new(points_text(draft)),
                ui_font.text_font(18.0),
                TextColor(CREAM),
                CreationLabel::Points,
            ));

            deck.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    ..default()
                },
                CreationLayoutRole::AppearanceControls,
            ))
            .with_children(|appearance| {
                for field in [
                    AppearanceField::SkinTone,
                    AppearanceField::Build,
                    AppearanceField::Hair,
                    AppearanceField::Accent,
                ] {
                    spawn_option_row(
                        appearance,
                        field.label(),
                        appearance_text(draft, field),
                        CreationAction::PreviousAppearance(field),
                        CreationAction::NextAppearance(field),
                        CreationLabel::Appearance(field),
                        ui_font,
                    );
                }
            });

            // #128: eight attributes render as compact cells, two per row
            // (see `ui_widgets::attribute_row`'s width math), so the deck
            // still fits a 900px-tall desktop viewport and the 356px deck
            // width that already carries the phone breakpoint.
            deck.spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    column_gap: Val::Px(8.0),
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                CreationLayoutRole::AttributeControls,
            ))
            .with_children(|attributes| {
                for kind in AttributeKind::ALL {
                    spawn_attribute_row(
                        attributes,
                        kind,
                        draft.get(kind),
                        CreationAction::Decrease(kind),
                        CreationAction::Increase(kind),
                        CreationLabel::Value(kind),
                        ui_font,
                    );
                }
            });

            deck.spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(10.0),
                ..default()
            })
            .with_children(|actions| {
                actions.spawn((
                    button_bundle("Începe lupta", Val::Px(174.0), Val::Px(52.0), 20.0, ui_font),
                    CreationAction::Confirm,
                ));
                actions.spawn((
                    button_bundle("Înapoi", Val::Px(128.0), Val::Px(52.0), 20.0, ui_font),
                    CreationAction::Back,
                ));
            });
        });
}

fn spawn_option_row(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    value: String,
    previous: impl Bundle,
    next: impl Bundle,
    value_label: impl Bundle,
    ui_font: &UiFont,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|row| {
            // #216: label 108 -> 76 and value 156 -> 100 so the row -- with
            // its two `MIN_TOUCH_TARGET` (44x44) stepper buttons, which no
            // longer flex-shrink at all (see
            // `ui_widgets::labeled_button_bundle`) -- genuinely fits the
            // deck's inner width (356 - 2x24 border inset = 308):
            // 76 + 44 + 100 + 44 + 3x8 gaps = 288. The longest label
            // ("Accent") and value ("Echilibrat") both fit their slots at
            // 18px.
            row.spawn(Node {
                width: Val::Px(76.0),
                ..default()
            })
            .with_children(|slot| {
                slot.spawn((Text::new(label), ui_font.text_font(18.0), TextColor(CREAM)));
            });
            row.spawn((
                // #216: 36x36 -> `MIN_TOUCH_TARGET` (44) square so the
                // appearance stepper meets the touch-target floor.
                button_bundle(
                    "<",
                    Val::Px(MIN_TOUCH_TARGET),
                    Val::Px(MIN_TOUCH_TARGET),
                    20.0,
                    ui_font,
                ),
                previous,
            ));
            row.spawn(Node {
                width: Val::Px(100.0),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|slot| {
                slot.spawn((
                    Text::new(value),
                    ui_font.text_font(18.0),
                    TextColor(CREAM),
                    value_label,
                ));
            });
            row.spawn((
                button_bundle(
                    ">",
                    Val::Px(MIN_TOUCH_TARGET),
                    Val::Px(MIN_TOUCH_TARGET),
                    20.0,
                    ui_font,
                ),
                next,
            ));
        });
}

fn points_text(draft: &CharacterDraft) -> String {
    format!("Puncte rămase: {}", draft.points_remaining())
}

fn description_text(draft: &CharacterDraft) -> String {
    let gear = if draft.starter_items().is_empty() {
        "Echipare: fără echipament de pornire.".to_string()
    } else {
        let names = draft
            .starter_items()
            .iter()
            .map(|item| item.item().name)
            .collect::<Vec<_>>()
            .join(", ");
        format!("Echipare: {names}.")
    };
    format!("{}\n{}", draft.description(), gear)
}

fn preview_stat_text(draft: &CharacterDraft, stat: PreviewStat) -> String {
    let attrs = draft.attributes();
    match stat {
        PreviewStat::Health => format!("HP {}", stats::max_hp(&attrs)),
        PreviewStat::Stamina => format!("STA {}", stats::max_stamina(&attrs)),
        PreviewStat::Damage => format!("DMG {}", stats::base_damage(&attrs)),
    }
}

fn equipment_from_items(items: &[crate::items::ItemId]) -> Equipment {
    let mut equipment = Equipment::default();
    for &item in items {
        equipment.equip(item);
    }
    equipment
}

#[cfg(test)]
fn creation_preview_allocation_fits_width(viewport_width: f32) -> bool {
    let usable_width = viewport_width - CREATION_ROOT_PADDING * 2.0;
    let preview_width = CREATION_PREVIEW_STAGE_WIDTH.min(usable_width);
    let control_width = CREATION_CONTROL_DECK_WIDTH.min(usable_width);
    let desktop_width =
        CREATION_PREVIEW_STAGE_WIDTH + CREATION_BODY_GAP + CREATION_CONTROL_DECK_WIDTH;
    desktop_width <= CREATION_BODY_WIDTH
        && CREATION_BODY_WIDTH <= CREATION_TARGET_WIDTH - CREATION_ROOT_PADDING * 2.0
        && preview_width <= usable_width
        && control_width <= usable_width
}

/// Reads the `PreviewStage` node's resolved screen rect and repositions
/// every [`CreationPreview`] root so its projected screen position lands at
/// that rect's center, scaling it so its *apparent* on-screen size stays
/// constant regardless of the letterbox zoom (matching the UI panel's own
/// fixed `Val::Px` size, which does not itself grow/shrink with the world
/// camera's zoom) -- see [`world_point_for_screen_point`]'s doc comment for
/// why this replaces the old `viewport.width`-only placement (#123).
///
/// Runs unconditionally (not gated on a resource-changed check): a
/// `CreationPreview` spawned this same frame (`OnEnter`) only gets a real
/// `PreviewStage` `ComputedNode`/`UiGlobalTransform` partway through this
/// very `PostUpdate`, once Bevy's own UI layout pass has run for it -- so
/// the first correct placement has to land on an ordinary frame, not a
/// change-detected one. Ordered `.after(UiSystems::Layout)` so it always
/// reads this frame's freshly resolved layout, never a stale one.
fn update_preview_transform(
    letterbox: Res<LetterboxRect>,
    stage_nodes: Query<(&ComputedNode, &UiGlobalTransform, &CreationLayoutRole)>,
    mut previews: Query<&mut Transform, With<CreationPreview>>,
) {
    let Some((node, transform)) = stage_nodes.iter().find_map(|(node, transform, role)| {
        (*role == CreationLayoutRole::PreviewStage).then_some((node, transform))
    }) else {
        return;
    };
    let stage_rect = logical_node_rect(transform, node);
    let target = world_point_for_screen_point(stage_rect.center(), *letterbox);
    let zoom = letterbox_zoom(*letterbox);
    for mut preview_transform in &mut previews {
        preview_transform.translation.x = target.x;
        preview_transform.translation.y = target.y + CREATION_PREVIEW_Y;
        preview_transform.translation.z = PREVIEW_Z;
        preview_transform.scale = Vec3::splat(CREATION_PREVIEW_SCALE / zoom);
    }
}

fn appearance_text(draft: &CharacterDraft, field: AppearanceField) -> String {
    match field {
        AppearanceField::SkinTone => draft.skin_tone().label().to_string(),
        AppearanceField::Build => draft.build().label().to_string(),
        AppearanceField::Hair => draft.hair().label().to_string(),
        AppearanceField::Accent => draft.accent().label().to_string(),
    }
}

type ChangedEnabledButton = (Changed<Interaction>, With<Button>, Without<DisabledButton>);

type AvailabilityControlled = (
    Entity,
    &'static CreationAction,
    Has<DisabledButton>,
    &'static mut BackgroundColor,
    &'static Children,
);

fn handle_creation_actions(
    mut commands: Commands,
    interactions: Query<(&Interaction, &CreationAction), ChangedEnabledButton>,
    mut draft: ResMut<CharacterDraft>,
    mut flow_intents: MessageWriter<FlowIntent>,
    mut save_requests: MessageWriter<SaveRequested>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *action {
            CreationAction::SelectChoice(choice) => draft.select_choice(choice),
            CreationAction::PreviousName => draft.previous_name(),
            CreationAction::NextName => draft.next_name(),
            CreationAction::Increase(kind) => {
                draft.increase(kind);
            }
            CreationAction::Decrease(kind) => {
                draft.decrease(kind);
            }
            CreationAction::PreviousAppearance(field) => match field {
                AppearanceField::SkinTone => draft.previous_skin_tone(),
                AppearanceField::Build => draft.previous_build(),
                AppearanceField::Hair => draft.previous_hair(),
                AppearanceField::Accent => draft.previous_accent(),
            },
            CreationAction::NextAppearance(field) => match field {
                AppearanceField::SkinTone => draft.next_skin_tone(),
                AppearanceField::Build => draft.next_build(),
                AppearanceField::Hair => draft.next_hair(),
                AppearanceField::Accent => draft.next_accent(),
            },
            CreationAction::Confirm => {
                if draft.is_complete() {
                    commands.insert_resource(PlayerCharacter {
                        name: draft.name().to_string(),
                        attributes: draft.attributes(),
                        appearance: draft.appearance(),
                        definition: draft.definition(),
                    });
                    let equipment = equipment_from_items(draft.starter_items());
                    commands.insert_resource(OwnedItems(
                        draft.starter_items().iter().copied().collect(),
                    ));
                    commands.insert_resource(PlayerEquipment(equipment));
                    save_requests.write(SaveRequested(ResumeDestination::Fight));
                    draft.reset();
                    flow_intents.write(FlowIntent::ConfirmHero);
                }
            }
            CreationAction::Back => {
                draft.reset();
                flow_intents.write(FlowIntent::BackToMenu);
            }
        }
    }
}

fn update_button_backgrounds(
    mut buttons: Query<(&Interaction, &mut BackgroundColor), ChangedEnabledButton>,
) {
    for (interaction, mut background) in &mut buttons {
        background.0 = match interaction {
            Interaction::Pressed => BUTTON_PRESSED,
            Interaction::Hovered => BUTTON_HOVERED,
            Interaction::None => BUTTON_NORMAL,
        };
    }
}

fn update_control_availability(
    draft: Res<CharacterDraft>,
    mut commands: Commands,
    mut buttons: Query<AvailabilityControlled, With<Button>>,
    mut text_colors: Query<&mut TextColor>,
) {
    for (entity, action, was_disabled, mut background, children) in &mut buttons {
        let enabled = match action {
            CreationAction::PreviousName | CreationAction::NextName => draft.can_cycle_name(),
            CreationAction::Increase(_) => draft.can_increase(),
            CreationAction::Decrease(kind) => draft.can_decrease(*kind),
            CreationAction::Confirm => draft.is_complete(),
            CreationAction::SelectChoice(_)
            | CreationAction::PreviousAppearance(_)
            | CreationAction::NextAppearance(_)
            | CreationAction::Back => continue,
        };
        if enabled != was_disabled {
            continue;
        }
        let text_color = if enabled {
            commands.entity(entity).remove::<DisabledButton>();
            background.0 = BUTTON_NORMAL;
            CREAM
        } else {
            commands.entity(entity).insert(DisabledButton);
            background.0 = BUTTON_DISABLED;
            TEXT_DISABLED
        };
        for child in children.iter() {
            if let Ok(mut color) = text_colors.get_mut(child) {
                color.0 = text_color;
            }
        }
    }
}

fn update_choice_button_styles(
    draft: Res<CharacterDraft>,
    mut buttons: Query<
        (
            &CreationAction,
            &Interaction,
            &mut BackgroundColor,
            &Children,
        ),
        With<Button>,
    >,
    mut text_colors: Query<&mut TextColor>,
) {
    for (action, interaction, mut background, children) in &mut buttons {
        let CreationAction::SelectChoice(choice) = action else {
            continue;
        };
        let selected = *choice == draft.choice();
        background.0 = if selected {
            BUTTON_PRESSED
        } else {
            match interaction {
                Interaction::Pressed => BUTTON_PRESSED,
                Interaction::Hovered => BUTTON_HOVERED,
                Interaction::None => BUTTON_NORMAL,
            }
        };
        for child in children.iter() {
            if let Ok(mut color) = text_colors.get_mut(child) {
                color.0 = CREAM;
            }
        }
    }
}

fn update_labels(draft: Res<CharacterDraft>, mut labels: Query<(&mut Text, &CreationLabel)>) {
    for (mut text, label) in &mut labels {
        text.0 = match label {
            CreationLabel::Description => description_text(&draft),
            CreationLabel::Name => draft.name().to_string(),
            CreationLabel::Points => points_text(&draft),
            CreationLabel::Value(kind) => draft.get(*kind).to_string(),
            CreationLabel::Appearance(field) => appearance_text(&draft, *field),
            CreationLabel::PreviewStat(stat) => preview_stat_text(&draft, *stat),
        };
    }
}

fn refresh_preview_rig(
    draft: Res<CharacterDraft>,
    mut commands: Commands,
    previews: Query<(Entity, Option<&Children>), With<CreationPreview>>,
    asset_server: Option<Res<AssetServer>>,
) {
    for (preview, children) in &previews {
        if let Some(children) = children {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }
        commands.entity(preview).remove::<CutoutRig>();
        let equipment = equipment_from_items(draft.starter_items());
        let resolved = resolve_human_character(&draft.definition())
            .expect("the creation draft resolves against the bundled human catalog");
        spawn_character_rig(
            &mut commands,
            preview,
            &resolved,
            asset_server.as_deref(),
            false,
            Some(&equipment),
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{AccentColor, BodyBuild, HairStyle, SkinTone};
    use crate::core::CorePlugin;
    use crate::cutout::{
        CutoutPartKind, CutoutPartMarker, CutoutPartRestPose, GearVisualLayer, cutout_rig_owner,
        human_template,
    };
    use crate::items::ItemId;
    use crate::save::{SaveGame, SavePlugin, SaveStore};
    use bevy::math::Affine2;
    use bevy::state::app::StatesPlugin;
    use bevy::window::PrimaryWindow;

    fn test_app_with_viewport(viewport: ViewportInfo) -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            crate::flow::FlowPlugin,
            CreationPlugin,
        ));
        app.update();
        app.world_mut()
            .resource_mut::<ViewportInfo>()
            .set_if_neq(viewport);
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::CharacterCreation);
        app.update();
        app.update();
        app
    }

    fn test_app() -> App {
        test_app_with_viewport(ViewportInfo::default())
    }

    /// Menu + creation + the flow plugin together, starting on the main
    /// menu — for journeys that cross both screens (#155).
    fn test_app_with_menu() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            crate::flow::FlowPlugin,
            crate::menu::MenuPlugin,
            CreationPlugin,
        ));
        app.update(); // headless `Loading` fall-through queues MainMenu (#114)
        app.update(); // transition applies; `OnEnter(MainMenu)` spawns the menu
        app
    }

    fn press_menu(app: &mut App, action: crate::menu::MenuAction) {
        let button = app
            .world_mut()
            .query_filtered::<(Entity, &crate::menu::MenuAction), With<Button>>()
            .iter(app.world())
            .find(|&(_, &a)| a == action)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("menu button for {action:?} exists"));
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
    }

    fn test_app_with_save() -> (App, std::sync::Arc<std::sync::Mutex<Option<String>>>) {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            crate::flow::FlowPlugin,
            CreationPlugin,
            SavePlugin,
        ));
        let (store, cell) = SaveStore::in_memory();
        app.insert_resource(store);
        app.insert_resource(crate::progression::Level::default());
        app.insert_resource(crate::progression::Wallet::default());
        app.insert_resource(crate::progression::LifetimeEarnings::default());
        app.insert_resource(crate::roster::LadderProgress::default());
        app.insert_resource(OwnedItems::default());
        app.insert_resource(PlayerEquipment::default());
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::CharacterCreation);
        app.update();
        app.update();
        (app, cell)
    }

    fn find_button(app: &mut App, action: CreationAction) -> Entity {
        app.world_mut()
            .query_filtered::<(Entity, &CreationAction), With<Button>>()
            .iter(app.world())
            .find(|(_, a)| **a == action)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("button for {action:?} exists"))
    }

    fn press(app: &mut App, action: CreationAction) {
        let button = find_button(app, action);
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
    }

    fn draft(app: &App) -> &CharacterDraft {
        app.world().resource::<CharacterDraft>()
    }

    fn label_text(app: &mut App, wanted: CreationLabel) -> String {
        app.world_mut()
            .query::<(&Text, &CreationLabel)>()
            .iter(app.world())
            .find(|(_, label)| **label == wanted)
            .map(|(text, _)| text.0.clone())
            .unwrap_or_else(|| panic!("label {wanted:?} exists"))
    }

    #[test]
    fn entering_creation_spawns_the_screen() {
        let mut app = test_app();
        let roots = app
            .world_mut()
            .query_filtered::<(), (With<CreationScreen>, With<Node>)>()
            .iter(app.world())
            .count();
        assert_eq!(roots, 1, "creation screen root spawned");
        let buttons = app
            .world_mut()
            .query_filtered::<(), With<Button>>()
            .iter(app.world())
            .count();
        assert_eq!(
            buttons, 33,
            "8 more stepper buttons since #128's four new attribute rows"
        );
    }

    #[test]
    fn creation_layout_gives_the_preview_stage_primary_space() {
        let mut app = test_app();
        let preview_stage = app
            .world_mut()
            .query::<(&Node, &CreationLayoutRole)>()
            .iter(app.world())
            .find(|(_, role)| **role == CreationLayoutRole::PreviewStage)
            .map(|(node, _)| node)
            .expect("preview stage exists");
        assert_eq!(preview_stage.width, Val::Px(CREATION_PREVIEW_STAGE_WIDTH));
        assert_eq!(preview_stage.min_height, Val::Px(CREATION_PANEL_HEIGHT));

        let control_deck = app
            .world_mut()
            .query::<(&Node, &CreationLayoutRole)>()
            .iter(app.world())
            .find(|(_, role)| **role == CreationLayoutRole::ControlDeck)
            .map(|(node, _)| node)
            .expect("control deck exists");
        assert_eq!(control_deck.width, Val::Px(CREATION_CONTROL_DECK_WIDTH));
        const {
            assert!(CREATION_PREVIEW_STAGE_WIDTH > CREATION_CONTROL_DECK_WIDTH);
        }
        assert!(creation_preview_allocation_fits_width(375.0));
        assert_eq!(
            app.world_mut()
                .query_filtered::<(), (With<CreationScreen>, With<crate::ui_widgets::Scrollable>)>()
                .iter(app.world())
                .count(),
            1,
            "stacked narrow creator layout must stay vertically reachable"
        );
    }

    /// Spawns a `Window`/[`PrimaryWindow`] of the given logical size (scale
    /// factor 1.0) so [`crate::core::letterbox_camera`] -- already wired by
    /// `CorePlugin` -- computes a real, non-default [`LetterboxRect`] for it,
    /// exactly like the running game. Headless test apps otherwise have no
    /// window at all, so `letterbox_camera` skips (see its `windows.single()`
    /// guard) and `LetterboxRect` stays at its unlettered default.
    fn spawn_primary_window(app: &mut App, width: f32, height: f32) {
        let mut window = Window::default();
        window.resolution = bevy::window::WindowResolution::new(width as u32, height as u32);
        app.world_mut().spawn((window, PrimaryWindow));
    }

    /// A full app on the creation screen with a real primary window of the
    /// given logical size, so [`LetterboxRect`] reflects genuine letterboxing
    /// (bars, zoom) instead of staying at its unlettered default -- the
    /// production code path #123 fixes only matters once there's an actual
    /// letterbox to project through.
    fn test_app_with_window(width: f32, height: f32) -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            crate::flow::FlowPlugin,
            CreationPlugin,
        ));
        app.update();
        spawn_primary_window(&mut app, width, height);
        app.world_mut()
            .resource_mut::<ViewportInfo>()
            .set_if_neq(ViewportInfo {
                width,
                height,
                is_mobile: crate::theme::is_mobile_width(width),
            });
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::CharacterCreation);
        app.update();
        app.update();
        app
    }

    fn preview_stage_entity(app: &mut App) -> Entity {
        app.world_mut()
            .query::<(Entity, &CreationLayoutRole)>()
            .iter(app.world())
            .find(|(_, role)| **role == CreationLayoutRole::PreviewStage)
            .map(|(e, _)| e)
            .expect("preview stage exists")
    }

    /// Hand-supplies the `PreviewStage` node's resolved `ComputedNode`/
    /// `UiGlobalTransform` -- headless test apps never run Bevy's real
    /// `ui_layout_system` (no `RenderPlugin`/window-backed camera target), so
    /// this module's tests simulate "already laid out" the same way
    /// `ui_widgets::focus`'s tests do, rather than relying on a live layout
    /// pass that doesn't happen here.
    fn set_preview_stage_rect(app: &mut App, rect: Rect) {
        let stage = preview_stage_entity(app);
        app.world_mut().entity_mut(stage).insert((
            ComputedNode {
                size: rect.size(),
                inverse_scale_factor: 1.0,
                ..Default::default()
            },
            UiGlobalTransform::from(Affine2::from_translation(rect.center())),
        ));
    }

    /// A plausible `PreviewStage` on-screen rect for a given viewport width:
    /// centered in the desktop two-column body once it fits side by side
    /// with the control deck, or centered on its own wrapped row once it no
    /// longer does -- the same shape the real flexbox layout produces, but
    /// computed by hand for a test that (per [`set_preview_stage_rect`]'s
    /// doc comment) never runs the real layout system.
    fn sample_stage_rect(viewport_width: f32) -> Rect {
        let desktop_width =
            CREATION_PREVIEW_STAGE_WIDTH + CREATION_BODY_GAP + CREATION_CONTROL_DECK_WIDTH;
        let usable_width = viewport_width - CREATION_ROOT_PADDING * 2.0;
        let center_x = if usable_width >= desktop_width {
            viewport_width / 2.0 - (CREATION_CONTROL_DECK_WIDTH + CREATION_BODY_GAP) / 2.0
        } else {
            viewport_width / 2.0
        };
        Rect::from_center_size(
            Vec2::new(center_x, 300.0),
            Vec2::new(CREATION_PREVIEW_STAGE_WIDTH, CREATION_PANEL_HEIGHT),
        )
    }

    fn creation_preview_transform(app: &mut App) -> Transform {
        *app.world_mut()
            .query_filtered::<&Transform, With<CreationPreview>>()
            .single(app.world())
            .expect("creation preview transform exists")
    }

    /// #123 red-first/green: the preview rig's `Transform`, once projected
    /// back to screen space through the same letterboxed camera math it was
    /// placed with, must land inside the `PreviewStage` node's *actual*
    /// resolved rect -- at desktop (1280x800), at the exact design
    /// resolution (800x600, no letterbox bars), and at a narrow mobile width
    /// (375x812) -- instead of the old `viewport.width`-only placement,
    /// which only ever happened to be correct at the exact design
    /// resolution (#123).
    #[test]
    fn preview_rig_projects_inside_the_preview_stage_rect_at_several_widths() {
        for (width, height) in [
            (1280.0_f32, 800.0_f32),
            (CREATION_TARGET_WIDTH, 600.0),
            (375.0, 812.0),
        ] {
            let mut app = test_app_with_window(width, height);
            let stage_rect = sample_stage_rect(width);
            set_preview_stage_rect(&mut app, stage_rect);
            app.update();

            let letterbox = *app.world().resource::<LetterboxRect>();
            assert!(
                letterbox.size.x > 0.0,
                "at {width}x{height}: letterbox_camera must have computed a real rect"
            );
            let transform = creation_preview_transform(&mut app);
            let projected =
                screen_point_for_world_point(transform.translation.truncate(), letterbox);
            assert!(
                stage_rect.contains(projected),
                "at {width}x{height}: projected preview position {projected:?} must land \
                 inside the preview stage rect {stage_rect:?}"
            );
        }
    }

    /// The rig's apparent on-screen size must stay roughly constant
    /// regardless of the letterbox zoom, matching the UI panel's own fixed
    /// `Val::Px` size -- otherwise a wide desktop window (more zoom) would
    /// render the same character enormous next to an unchanged-size frame,
    /// and a narrow phone width would shrink it to a speck.
    #[test]
    fn preview_rig_scale_compensates_for_letterbox_zoom() {
        let mut wide = test_app_with_window(1280.0, 800.0);
        set_preview_stage_rect(&mut wide, sample_stage_rect(1280.0));
        wide.update();
        let mut narrow = test_app_with_window(375.0, 812.0);
        set_preview_stage_rect(&mut narrow, sample_stage_rect(375.0));
        narrow.update();

        let wide_zoom = letterbox_zoom(*wide.world().resource::<LetterboxRect>());
        let narrow_zoom = letterbox_zoom(*narrow.world().resource::<LetterboxRect>());
        assert!(wide_zoom > narrow_zoom, "sanity: wide window zooms in more");

        let wide_scale = creation_preview_transform(&mut wide).scale.x;
        let narrow_scale = creation_preview_transform(&mut narrow).scale.x;
        // Apparent size = world scale * zoom; must match within float noise.
        assert!(
            (wide_scale * wide_zoom - narrow_scale * narrow_zoom).abs() < 1e-4,
            "wide apparent size {} must match narrow apparent size {}",
            wide_scale * wide_zoom,
            narrow_scale * narrow_zoom
        );
    }

    /// #123's actual root cause, proven directly: the rig must derive its
    /// position from the `PreviewStage` node's *real* resolved rect, not
    /// recompute an independent guess from `ViewportInfo::width` alone (the
    /// old `creation_preview_x_for_width`, which never looked at the node's
    /// `ComputedNode`/`UiGlobalTransform` at all). Two different, quite
    /// deliberately odd stage rects at the *same* viewport width must
    /// produce two different projected positions, each landing inside its
    /// own rect -- a width-keyed formula would produce the identical result
    /// both times and fail this.
    #[test]
    fn preview_rig_tracks_the_stage_rects_actual_position_not_a_width_keyed_guess() {
        let mut app = test_app_with_window(1280.0, 800.0);

        let odd_rect_one = Rect::from_center_size(Vec2::new(900.0, 120.0), Vec2::new(392.0, 482.0));
        set_preview_stage_rect(&mut app, odd_rect_one);
        app.update();
        let letterbox = *app.world().resource::<LetterboxRect>();
        let projected_one = screen_point_for_world_point(
            creation_preview_transform(&mut app).translation.truncate(),
            letterbox,
        );
        assert!(
            odd_rect_one.contains(projected_one),
            "must land inside the first rect {odd_rect_one:?}, got {projected_one:?}"
        );

        let odd_rect_two = Rect::from_center_size(Vec2::new(200.0, 600.0), Vec2::new(392.0, 482.0));
        set_preview_stage_rect(&mut app, odd_rect_two);
        app.update();
        let projected_two = screen_point_for_world_point(
            creation_preview_transform(&mut app).translation.truncate(),
            letterbox,
        );
        assert!(
            odd_rect_two.contains(projected_two),
            "must land inside the second rect {odd_rect_two:?}, got {projected_two:?}"
        );
        assert!(
            projected_one.distance(projected_two) > 100.0,
            "moving the stage rect must move the projected preview position, proving it's \
             derived from the node's actual resolved layout rather than a fixed/width-keyed guess"
        );
    }

    /// Recursively counts every `CutoutPartMarker` entity under `root`, at
    /// any depth. Forearms/hands/shins/feet are nested several joints deep
    /// rather than being direct children of the rig root (#117), so this
    /// walks the whole subtree instead of only `root`'s immediate
    /// `Children`.
    fn cutout_descendant_count(app: &mut App, root: Entity) -> usize {
        let world = app.world();
        let mut count = 0;
        let mut stack = vec![root];
        while let Some(entity) = stack.pop() {
            let Some(children) = world.get::<Children>(entity) else {
                continue;
            };
            for child in children.iter() {
                if world.get::<CutoutPartMarker>(child).is_some() {
                    count += 1;
                }
                stack.push(child);
            }
        }
        count
    }

    fn cutout_identity_and_rest_snapshot(
        app: &App,
        root: Entity,
    ) -> std::collections::HashMap<
        CutoutPartKind,
        (Option<crate::character::PartId>, CutoutPartRestPose),
    > {
        let world = app.world();
        let mut snapshot = std::collections::HashMap::new();
        let mut stack = vec![root];
        while let Some(entity) = stack.pop() {
            let Some(children) = world.get::<Children>(entity) else {
                continue;
            };
            for child in children.iter() {
                if let (Some(marker), Some(rest)) = (
                    world.get::<CutoutPartMarker>(child),
                    world.get::<CutoutPartRestPose>(child),
                ) {
                    snapshot.insert(marker.kind, (marker.source_id.clone(), *rest));
                }
                stack.push(child);
            }
        }
        snapshot
    }

    #[test]
    fn entering_creation_spawns_a_cutout_preview() {
        let mut app = test_app();
        let preview = app
            .world_mut()
            .query_filtered::<Entity, (With<CreationPreview>, With<CutoutRig>)>()
            .single(app.world())
            .expect("one cutout preview root exists");
        assert_eq!(
            cutout_descendant_count(&mut app, preview),
            human_template().parts.len()
        );
    }

    #[test]
    fn creation_and_arena_render_the_same_definition_identity_and_rest_pose() {
        let mut creation = test_app();
        let definition = draft(&creation).definition();
        let creation_root = creation
            .world_mut()
            .query_filtered::<Entity, With<CreationPreview>>()
            .single(creation.world())
            .expect("creation preview exists");
        let creation_snapshot = cutout_identity_and_rest_snapshot(&creation, creation_root);

        let mut arena = App::new();
        arena.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            crate::arena::ArenaPlugin,
        ));
        arena.insert_resource(PlayerCharacter {
            name: "Făt-Frumos".to_owned(),
            attributes: Attributes::default(),
            appearance: definition.appearance,
            definition,
        });
        arena.insert_resource(crate::roster::LadderProgress::default());
        arena.update();
        arena
            .world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Fight);
        arena.update();
        let arena_root = arena
            .world_mut()
            .query_filtered::<Entity, With<crate::character::PlayerFighter>>()
            .single(arena.world())
            .expect("arena player exists");
        let arena_snapshot = cutout_identity_and_rest_snapshot(&arena, arena_root);

        assert_eq!(creation_snapshot, arena_snapshot);
        assert!(
            creation_snapshot
                .values()
                .all(|(source_id, _)| source_id.is_some()),
            "every rendered semantic region retains its resolved stable ID"
        );
    }

    /// The same subtree walk as [`cutout_descendant_count`], but collecting
    /// entity ids instead of just a count -- so a test can prove a set of
    /// parts was genuinely despawned and replaced by a fresh set (not just
    /// left with the same count coincidentally).
    fn cutout_descendant_entities(
        app: &mut App,
        root: Entity,
    ) -> std::collections::HashSet<Entity> {
        let world = app.world();
        let mut found = std::collections::HashSet::new();
        let mut stack = vec![root];
        while let Some(entity) = stack.pop() {
            let Some(children) = world.get::<Children>(entity) else {
                continue;
            };
            for child in children.iter() {
                if world.get::<CutoutPartMarker>(child).is_some() {
                    found.insert(child);
                }
                stack.push(child);
            }
        }
        found
    }

    /// #123 test-expectation: `refresh_preview_rig` must actually despawn
    /// the old cutout parts and spawn fresh ones on every `CharacterDraft`
    /// change, not mutate something in place -- proven by the part entity
    /// ids being completely disjoint before/after, rather than just
    /// re-checking a label or count that could coincidentally match.
    #[test]
    fn refresh_preview_rig_respawns_parts_when_the_draft_mutates() {
        let mut app = test_app();
        let preview = app
            .world_mut()
            .query_filtered::<Entity, (With<CreationPreview>, With<CutoutRig>)>()
            .single(app.world())
            .expect("one cutout preview root exists");
        let before = cutout_descendant_entities(&mut app, preview);
        assert!(!before.is_empty());

        press(
            &mut app,
            CreationAction::NextAppearance(AppearanceField::Hair),
        );

        let after = cutout_descendant_entities(&mut app, preview);
        assert!(!after.is_empty());
        assert_eq!(after.len(), before.len());
        assert!(
            before.is_disjoint(&after),
            "refresh_preview_rig must despawn the old parts and spawn fresh \
             ones on every CharacterDraft change, not mutate in place"
        );
    }

    #[test]
    fn preset_starter_items_attach_to_the_creation_preview_rig() {
        let mut app = test_app();
        press(
            &mut app,
            CreationAction::SelectChoice(HeroChoice::Preset(HeroPreset::Ciobanul)),
        );

        let preview = app
            .world_mut()
            .query_filtered::<Entity, (With<CreationPreview>, With<CutoutRig>)>()
            .single(app.world())
            .expect("one cutout preview root exists");
        let part_parents: std::collections::HashMap<Entity, Entity> = app
            .world_mut()
            .query::<(Entity, &CutoutPartMarker, &ChildOf)>()
            .iter(app.world())
            .map(|(part, _, child_of)| (part, child_of.parent()))
            .collect();
        let part_kinds: std::collections::HashMap<Entity, CutoutPartKind> = app
            .world_mut()
            .query::<(Entity, &CutoutPartMarker)>()
            .iter(app.world())
            .map(|(part, marker)| (part, marker.kind))
            .collect();
        let mut layers: Vec<(ItemId, CutoutPartKind)> = app
            .world_mut()
            .query::<(&GearVisualLayer, &ChildOf)>()
            .iter(app.world())
            .filter_map(|(layer, child_of)| {
                let part = child_of.parent();
                let kind = *part_kinds.get(&part)?;
                let owner = cutout_rig_owner(part, |e| part_parents.get(&e).copied());
                (owner == preview).then_some((layer.item, kind))
            })
            .collect();
        layers.sort_by_key(|(item, _)| *item as usize);

        assert_eq!(
            layers,
            vec![
                (ItemId::BataCiobaneasca, CutoutPartKind::HandFront),
                (ItemId::CojocGros, CutoutPartKind::Torso),
                (ItemId::CaciulaDeOaie, CutoutPartKind::Head),
            ]
        );
    }

    #[test]
    fn selecting_a_preset_populates_the_editable_draft() {
        let mut app = test_app();
        press(
            &mut app,
            CreationAction::SelectChoice(HeroChoice::Preset(HeroPreset::Ciobanul)),
        );
        assert_eq!(
            draft(&app).choice(),
            HeroChoice::Preset(HeroPreset::Ciobanul)
        );
        assert_eq!(draft(&app).name(), "Ciobanul");
        assert_eq!(draft(&app).attributes(), HeroPreset::Ciobanul.attributes());
        assert_eq!(draft(&app).appearance(), HeroPreset::Ciobanul.appearance());
        assert!(draft(&app).is_complete());
        assert!(label_text(&mut app, CreationLabel::Description).contains("Echipare:"));
    }

    /// #216: presses `Tab` once and returns the newly-focused entity.
    fn tab_focus(app: &mut App) -> Option<Entity> {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Tab);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::Tab);
        keys.clear();
        app.world()
            .resource::<crate::ui_widgets::focus::InputFocus>()
            .get()
    }

    /// #216: every control on the control deck -- preset grid, name arrows,
    /// appearance/attribute steppers, Confirm, Back -- shares one `TabGroup`
    /// in the deck's visual (spawn) order: the first Tab from unset focus
    /// lands on the first preset tile, and tabbing all the way around wraps
    /// back to it.
    #[test]
    fn tab_order_starts_on_the_first_preset_and_wraps_around() {
        let mut app = test_app();
        app.init_resource::<ButtonInput<KeyCode>>();

        let first_preset = find_button(&mut app, CreationAction::SelectChoice(HeroChoice::ALL[0]));
        assert_eq!(tab_focus(&mut app), Some(first_preset));

        // Walk all the way around: 5 presets, 2 name arrows, 4 appearance
        // rows x2, 8 attribute rows x2 (#128), Confirm, Back = 28 controls.
        let total_controls = HeroChoice::ALL.len() + 2 + 4 * 2 + 8 * 2 + 2;
        for _ in 1..total_controls {
            tab_focus(&mut app);
        }
        assert_eq!(
            tab_focus(&mut app),
            Some(first_preset),
            "tab order must wrap back to the first preset after every control"
        );
    }

    /// #216: Enter on a focused preset tile selects it exactly like a click
    /// (see `selecting_a_preset_populates_the_editable_draft` for the click
    /// version) -- `FocusNavigationPlugin::activate_focused_control` writes
    /// the same `Interaction::Pressed` a click produces.
    #[test]
    fn enter_on_a_focused_preset_selects_it_like_a_click() {
        let mut app = test_app();
        app.init_resource::<ButtonInput<KeyCode>>();

        let target = HeroChoice::Preset(HeroPreset::Ciobanul);
        let button = find_button(&mut app, CreationAction::SelectChoice(target));
        app.world_mut()
            .insert_resource(crate::ui_widgets::focus::InputFocus::from_entity(button));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();

        assert_eq!(draft(&app).choice(), target);
    }

    #[test]
    fn preset_names_disable_name_cycling_but_custom_reenables_it() {
        let mut app = test_app();
        let previous = find_button(&mut app, CreationAction::PreviousName);
        assert!(
            !app.world().entity(previous).contains::<DisabledButton>(),
            "custom starts with editable names"
        );
        press(
            &mut app,
            CreationAction::SelectChoice(HeroChoice::Preset(HeroPreset::Haiducul)),
        );
        assert!(
            app.world().entity(previous).contains::<DisabledButton>(),
            "preset names are fixed"
        );
        press(&mut app, CreationAction::NextName);
        assert_eq!(draft(&app).name(), "Haiducul");

        press(&mut app, CreationAction::SelectChoice(HeroChoice::Custom));
        assert!(
            !app.world().entity(previous).contains::<DisabledButton>(),
            "custom path reenables curated names"
        );
    }

    #[test]
    fn custom_path_still_gates_confirm_until_all_points_are_spent() {
        let mut app = test_app();
        let confirm = find_button(&mut app, CreationAction::Confirm);
        assert!(
            app.world().entity(confirm).contains::<DisabledButton>(),
            "confirm starts disabled for custom heroes"
        );
        press(&mut app, CreationAction::Confirm);
        app.update();
        assert!(app.world().get_resource::<PlayerCharacter>().is_none());
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::CharacterCreation
        );
    }

    #[test]
    fn custom_confirm_persists_edited_appearance_and_empty_loadout() {
        let mut app = test_app();
        press(
            &mut app,
            CreationAction::NextAppearance(AppearanceField::SkinTone),
        );
        press(
            &mut app,
            CreationAction::NextAppearance(AppearanceField::Build),
        );
        press(
            &mut app,
            CreationAction::NextAppearance(AppearanceField::Hair),
        );
        press(
            &mut app,
            CreationAction::NextAppearance(AppearanceField::Accent),
        );
        for _ in 0..4 {
            press(&mut app, CreationAction::Increase(AttributeKind::Putere));
        }
        for _ in 0..6 {
            press(
                &mut app,
                CreationAction::Increase(AttributeKind::Vitalitate),
            );
        }
        // #128: the custom pool is 16 points now; spend the last 6 on the
        // new kinds (magie included, up from its 0 base).
        for _ in 0..3 {
            press(&mut app, CreationAction::Increase(AttributeKind::Atac));
        }
        for _ in 0..2 {
            press(&mut app, CreationAction::Increase(AttributeKind::Aparare));
        }
        press(&mut app, CreationAction::Increase(AttributeKind::Magie));

        press(&mut app, CreationAction::Confirm);
        app.update();

        let player = app
            .world()
            .get_resource::<PlayerCharacter>()
            .expect("PlayerCharacter stored on confirm");
        assert_eq!(player.name, FOLK_NAMES[0]);
        assert_eq!(
            player.appearance,
            PlayerAppearance {
                skin_tone: SkinTone::Olive,
                build: BodyBuild::Sturdy,
                hair: HairStyle::Long,
                accent: AccentColor::Forest,
            }
        );
        assert_eq!(
            *app.world().resource::<PlayerEquipment>(),
            PlayerEquipment(Equipment::default())
        );
        assert_eq!(
            *app.world().resource::<OwnedItems>(),
            OwnedItems(Default::default())
        );
    }

    #[test]
    fn confirmation_persists_the_definition_shown_by_the_preview() {
        let mut app = test_app();
        press(
            &mut app,
            CreationAction::SelectChoice(HeroChoice::Preset(HeroPreset::Haiducul)),
        );
        let previewed_definition = draft(&app).definition();

        press(&mut app, CreationAction::Confirm);
        app.update();

        let player = app
            .world()
            .get_resource::<PlayerCharacter>()
            .expect("PlayerCharacter stored on confirm");
        assert_eq!(player.definition, previewed_definition);
    }

    #[test]
    fn preset_confirm_stores_player_character_loadout_and_starts_the_fight() {
        let mut app = test_app();
        press(
            &mut app,
            CreationAction::SelectChoice(HeroChoice::Preset(HeroPreset::Voinicul)),
        );
        press(&mut app, CreationAction::Decrease(AttributeKind::Putere));
        press(&mut app, CreationAction::Increase(AttributeKind::Agilitate));
        press(&mut app, CreationAction::Confirm);
        app.update();

        let player = app
            .world()
            .get_resource::<PlayerCharacter>()
            .expect("PlayerCharacter stored on confirm");
        assert_eq!(player.name, "Voinicul");
        assert_eq!(
            player.attributes,
            Attributes {
                putere: 3,
                agilitate: 4,
                vitalitate: 4,
                noroc: 2,
                atac: 4,
                aparare: 4,
                carisma: 2,
                magie: 0,
            }
        );
        assert_eq!(player.appearance, HeroPreset::Voinicul.appearance());
        let loadout = app.world().resource::<PlayerEquipment>();
        assert_eq!(
            loadout.0.equipped(crate::items::Slot::Weapon),
            Some(ItemId::BataCiobaneasca)
        );
        assert_eq!(
            loadout.0.equipped(crate::items::Slot::Shield),
            Some(ItemId::ScutDeLemn)
        );
        let owned = app.world().resource::<OwnedItems>();
        assert!(owned.0.contains(&ItemId::BataCiobaneasca));
        assert!(owned.0.contains(&ItemId::ScutDeLemn));
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Fight
        );
        assert_eq!(*draft(&app), CharacterDraft::default());
    }

    #[test]
    fn confirming_a_new_hero_autosaves_that_run_immediately() {
        let (mut app, cell) = test_app_with_save();
        let old_save = r#"{"version":1,"name":"Old Hero","attrs":{"putere":9,"agilitate":1,"vitalitate":1,"noroc":1},"level":3,"xp":99,"unspent_points":0,"wallet":444,"owned_items":[],"equipped":[],"ladder_progress":4,"lap":1}"#;
        *cell.lock().expect("test store lock") = Some(old_save.to_string());

        press(
            &mut app,
            CreationAction::SelectChoice(HeroChoice::Preset(HeroPreset::Ciobanul)),
        );
        press(&mut app, CreationAction::Confirm);
        app.update();

        let json = cell
            .lock()
            .expect("test store lock")
            .clone()
            .expect("confirm writes a save");
        let save = SaveGame::from_json(&json).expect("new hero save is valid");
        assert_eq!(save.name, "Ciobanul");
        assert_eq!(save.attrs, HeroPreset::Ciobanul.attributes().into());
        assert_eq!(save.appearance, HeroPreset::Ciobanul.appearance());
        assert_eq!(save.ladder_progress, 0, "new run starts at first fight");
        assert!(
            save.equipped.contains(&"BataCiobaneasca".to_string()),
            "preset starter loadout is captured"
        );
        assert_eq!(
            save.resume_destination(),
            crate::save::ResumeDestination::Fight,
            "hero confirmation resumes straight into the arena (#217)"
        );
    }

    #[test]
    fn preview_and_labels_track_clicks() {
        let mut app = test_app();
        assert_eq!(
            label_text(&mut app, CreationLabel::PreviewStat(PreviewStat::Health)),
            "HP 60"
        );
        assert_eq!(
            label_text(&mut app, CreationLabel::PreviewStat(PreviewStat::Stamina)),
            "STA 35"
        );
        assert_eq!(
            label_text(&mut app, CreationLabel::PreviewStat(PreviewStat::Damage)),
            "DMG 3"
        );
        assert_eq!(
            label_text(&mut app, CreationLabel::Points),
            "Puncte rămase: 16"
        );
        press(
            &mut app,
            CreationAction::Increase(AttributeKind::Vitalitate),
        );
        assert_eq!(
            label_text(&mut app, CreationLabel::PreviewStat(PreviewStat::Health)),
            "HP 70"
        );
        assert_eq!(
            label_text(&mut app, CreationLabel::PreviewStat(PreviewStat::Stamina)),
            "STA 40"
        );
        assert_eq!(
            label_text(&mut app, CreationLabel::Points),
            "Puncte rămase: 15"
        );
        press(
            &mut app,
            CreationAction::NextAppearance(AppearanceField::Accent),
        );
        assert_eq!(
            label_text(&mut app, CreationLabel::Appearance(AppearanceField::Accent)),
            "Verde"
        );
    }

    // --- Flow-intent journeys (#155): menu <-> creation, driven entirely
    // through FlowIntent/the transition table rather than either screen
    // writing NextState directly. ---

    #[test]
    fn menu_new_game_through_creation_confirm_reaches_fight() {
        let mut app = test_app_with_menu();
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::MainMenu
        );

        press_menu(&mut app, crate::menu::MenuAction::NewGame);
        app.update(); // transition applies; creation screen spawns
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::CharacterCreation,
            "New Game routes menu -> creation through the flow table"
        );

        press(
            &mut app,
            CreationAction::SelectChoice(HeroChoice::Preset(HeroPreset::Ciobanul)),
        );
        press(&mut app, CreationAction::Confirm);
        app.update(); // transition applies

        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Fight,
            "confirming the hero routes creation -> fight through the flow table"
        );
        let player = app
            .world()
            .get_resource::<PlayerCharacter>()
            .expect("PlayerCharacter stored on confirm");
        assert_eq!(player.name, "Ciobanul");
    }

    #[test]
    fn pressing_back_returns_to_the_main_menu_and_despawns_creation() {
        let mut app = test_app_with_menu();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::CharacterCreation);
        app.update();
        app.update();
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::CharacterCreation
        );

        press(&mut app, CreationAction::Back);
        app.update(); // transition applies; creation despawns, menu spawns

        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::MainMenu,
            "Back routes creation -> menu through the flow table"
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<(), With<CreationScreen>>()
                .iter(app.world())
                .count(),
            0,
            "creation screen fully despawned"
        );
        assert!(
            app.world_mut()
                .query_filtered::<(), With<crate::menu::MenuAction>>()
                .iter(app.world())
                .next()
                .is_some(),
            "main menu respawned with its action buttons"
        );
    }
}
