//! The town hub screen (#129, `docs/navigation-proposal.md`): the neutral
//! "home" between fights. One title, one dominant primary action into the
//! arena, two secondary destination cards (the shop and the read-only
//! character view), and one consistent back action to the main menu.
//!
//! The character view ("Personaj") is a Town-local sub-view, not a
//! [`GameState`]: it reuses the creation screen's live cutout preview (the
//! shared [`spawn_character_definition_rig`] primitive positioned under a
//! transparent UI frame, exactly like `crate::creation`/`crate::shop`) plus
//! a read-only attribute list from the confirmed [`PlayerCharacter`]. No
//! editing happens here — allocation lives on the result screen and
//! equipping stays in the shop.

use bevy::prelude::*;
use bevy::ui::UiSystems;

use crate::character::{AttributeKind, stats};
use crate::core::{
    GameState, LetterboxRect, UiFont, despawn_screen, letterbox_zoom, logical_node_rect,
    world_point_for_screen_point,
};
use crate::creation::PlayerCharacter;
use crate::cutout::spawn_character_definition_rig;
use crate::flow::FlowIntent;
use crate::progression::Level;
use crate::save::{ResumeDestination, SaveRequested};
use crate::shop::PlayerEquipment;
use crate::theme::{
    ARENA_BROWN, BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED, CREAM, GOLD, PANEL_LINEN,
    PanelTexture, SCRIM, SPACE_LG, SPACE_MD, SPACE_SM, WALNUT, panel_bundle, title_font,
};
use crate::ui_widgets::focus::{
    FocusNavigationPlugin, FocusNavigationSet, InputFocus, PendingAutofocus, TabGroup,
    TabNavigation, autofocus_first_in_group,
};
use crate::ui_widgets::{button_bundle, wide_button};

/// Width of the dominant arena card; the two secondary cards size to their
/// content instead.
const ARENA_CARD_WIDTH: f32 = 420.0;
/// The primary "Luptă în arenă" button: deliberately larger than the shared
/// 260x56 [`wide_button`] so it reads as the screen's one dominant action.
const PRIMARY_BUTTON_WIDTH: f32 = 300.0;
const PRIMARY_BUTTON_HEIGHT: f32 = 64.0;
const PRIMARY_BUTTON_FONT: f32 = 26.0;
/// Back button shape shared by the hub and the character view (the same
/// footprint as the creation screen's back button).
const BACK_BUTTON_WIDTH: f32 = 128.0;
const BACK_BUTTON_HEIGHT: f32 = 52.0;

// The character-view preview mirrors the creation screen's stage so the rig
// renders at the same apparent size there and here.
const PREVIEW_STAGE_WIDTH: f32 = 392.0;
const PREVIEW_FRAME_HEIGHT: f32 = 318.0;
const PREVIEW_SCALE: f32 = 1.02;
const PREVIEW_Y: f32 = -18.0;
const PREVIEW_Z: f32 = 25.0;

/// Marker for everything the town screen spawns (the world-space backdrop,
/// the hub view, the character view, the preview rig, and the leave-confirm
/// overlay); despawned by [`despawn_screen`] on `OnExit(GameState::Town)`.
#[derive(Component)]
pub struct TownScreen;

/// Marker for the hub view's UI root (title + destination cards).
#[derive(Component)]
struct TownHubView;

/// Marker for the read-only character view's UI root.
#[derive(Component)]
struct TownCharacterView;

/// Marker for the world-space cutout preview root inside the character view
/// (the town counterpart of `creation`'s `CreationPreview`).
#[derive(Component)]
struct TownCharacterPreview;

/// Marker for the are-you-sure overlay shown before leaving to the menu.
#[derive(Component)]
struct TownLeaveConfirm;

/// Marker for the confirm overlay's panel — the `TabGroup::modal()` root
/// [`autofocus_leave_confirm`] targets (#216).
#[derive(Component)]
struct TownConfirmPanel;

/// Stable layout anchors for the town screen.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum TownLayoutRole {
    ArenaCard,
    ShopCard,
    CharacterCard,
    /// The character view's borderless cutout window the world-space rig is
    /// positioned under (see [`update_character_preview_transform`]).
    CharacterPreviewFrame,
}

/// What a town button does when pressed. `pub` so the `review`-feature seam
/// (`src/review/mod.rs`) can press these buttons through the same
/// `pressButton` command channel every other screen's navigation buttons
/// use.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TownAction {
    /// **Luptă în arenă** → [`GameState::Fight`]: the hub's one dominant
    /// primary action.
    EnterArena,
    /// **Prăvălie** → [`GameState::Shop`]: the optional shop detour.
    GoToShop,
    /// **Personaj**: open the read-only character view (a Town-local
    /// sub-view, no state change).
    ViewCharacter,
    /// **Înapoi** (character view): close the character view back to the
    /// hub (no state change).
    CloseCharacter,
    /// **Înapoi** (hub): return to the main menu — behind an are-you-sure
    /// overlay while a run is active (which, on this screen, it always is
    /// outside headless tests). The save is kept either way; **Continuă**
    /// resumes the run at the hub.
    Back,
    /// **La meniu** (confirm overlay): actually leave for the menu.
    ConfirmLeave,
    /// **Rămâi în sat** (confirm overlay): close the overlay and stay.
    CancelLeave,
}

pub struct TownPlugin;

impl Plugin for TownPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SaveRequested>()
            .add_message::<FlowIntent>()
            .add_plugins((crate::ui_widgets::ScrollInputPlugin, FocusNavigationPlugin))
            .add_systems(
                OnEnter(GameState::Town),
                (spawn_town_screen, autosave_on_town_entry),
            )
            .add_systems(
                Update,
                (
                    handle_town_actions
                        .in_set(crate::flow::FlowIntentEmission)
                        .after(FocusNavigationSet),
                    autofocus_leave_confirm,
                    update_button_backgrounds,
                    crate::ui_widgets::scroll_with_wheel_and_touch,
                )
                    .run_if(in_state(GameState::Town)),
            )
            .add_systems(
                PostUpdate,
                update_character_preview_transform
                    .after(UiSystems::Layout)
                    .before(bevy::transform::TransformSystems::Propagate)
                    .run_if(in_state(GameState::Town)),
            )
            .add_systems(OnExit(GameState::Town), despawn_screen::<TownScreen>);
    }
}

/// Arriving at the hub autosaves with [`ResumeDestination::Town`] (#129):
/// whatever checkpoint wrote the save last (shop changes tag the shop, for
/// example), once the player is back on the hub a reload must resume here —
/// the same entry-checkpoint reasoning as `shop::autosave_on_shop_entry`.
fn autosave_on_town_entry(mut save_requests: MessageWriter<SaveRequested>) {
    save_requests.write(SaveRequested(ResumeDestination::Town));
}

/// Spawns the town screen: the world-space backdrop (the same
/// letterbox-stage sprite treatment as the creation/shop screens, so the
/// character view's cutout rig can composite under a transparent UI) and
/// the hub view.
fn spawn_town_screen(
    mut commands: Commands,
    ui_font: Res<UiFont>,
    panel_texture: Res<PanelTexture>,
) {
    commands.spawn((
        TownScreen,
        Sprite::from_color(ARENA_BROWN, Vec2::new(800.0, 600.0)),
        Transform::from_xyz(0.0, 0.0, -40.0),
    ));
    spawn_hub_view(&mut commands, &ui_font, &panel_texture);
}

/// A full-window scrollable screen root shared by the hub and character
/// views: transparent (the world-space backdrop provides the color), a
/// column starting from the top, one shared focus region.
fn view_root() -> impl Bundle {
    (
        TownScreen,
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Center,
            row_gap: Val::Px(SPACE_LG),
            padding: UiRect::all(Val::Px(SPACE_LG)),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        ScrollPosition::default(),
        crate::ui_widgets::Scrollable,
        TabGroup::new(0),
    )
}

/// The consistent top-left back action (screen contract): a full-width row
/// whose only child is the back button, so the button sits top-left while
/// the title below stays centered.
fn back_row(action: TownAction, ui_font: &UiFont) -> impl Bundle {
    (
        Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::FlexStart,
            ..default()
        },
        children![(
            button_bundle(
                "Înapoi",
                Val::Px(BACK_BUTTON_WIDTH),
                Val::Px(BACK_BUTTON_HEIGHT),
                20.0,
                ui_font,
            ),
            action,
        )],
    )
}

/// Spawns the hub view: back action, the "Satul" title, and the three
/// destination cards — the dominant arena card on top, the two secondary
/// cards (shop, character) in a wrapping row below it, each one an
/// embroidered panel with no nested panels (screen contract).
fn spawn_hub_view(commands: &mut Commands, ui_font: &UiFont, panel_texture: &PanelTexture) {
    commands
        .spawn((view_root(), TownHubView))
        .with_children(|root| {
            root.spawn(back_row(TownAction::Back, ui_font));
            root.spawn((Text::new("Satul"), title_font(ui_font), TextColor(CREAM)));

            root.spawn((
                panel_bundle(
                    panel_texture,
                    Node {
                        width: Val::Px(ARENA_CARD_WIDTH),
                        max_width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(SPACE_MD),
                        ..default()
                    },
                ),
                BackgroundColor(PANEL_LINEN),
                TownLayoutRole::ArenaCard,
            ))
            .with_children(|card| {
                card.spawn((
                    Text::new("Următoarea luptă te așteaptă."),
                    ui_font.text_font(18.0),
                    TextColor(CREAM),
                ));
                card.spawn((
                    button_bundle(
                        "Luptă în arenă",
                        Val::Px(PRIMARY_BUTTON_WIDTH),
                        Val::Px(PRIMARY_BUTTON_HEIGHT),
                        PRIMARY_BUTTON_FONT,
                        ui_font,
                    ),
                    TownAction::EnterArena,
                ));
            });

            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(SPACE_LG),
                row_gap: Val::Px(SPACE_LG),
                max_width: Val::Percent(100.0),
                ..default()
            })
            .with_children(|row| {
                for (role, line, label, action) in [
                    (
                        TownLayoutRole::ShopCard,
                        "Târguiește arme și strai.",
                        "Prăvălie",
                        TownAction::GoToShop,
                    ),
                    (
                        TownLayoutRole::CharacterCard,
                        "Privește-ți eroul.",
                        "Personaj",
                        TownAction::ViewCharacter,
                    ),
                ] {
                    row.spawn((
                        panel_bundle(
                            panel_texture,
                            Node {
                                max_width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::Center,
                                row_gap: Val::Px(SPACE_SM),
                                ..default()
                            },
                        ),
                        BackgroundColor(PANEL_LINEN),
                        role,
                    ))
                    .with_children(|card| {
                        card.spawn((Text::new(line), ui_font.text_font(18.0), TextColor(CREAM)));
                        card.spawn((wide_button(label, ui_font), action));
                    });
                }
            });
        });
}

/// Spawns the read-only character view: back action, "Personaj" title, the
/// live cutout preview (name, borderless-fill frame, derived-stat strip —
/// the creation screen's stage shape) and the attribute panel. The rig root
/// is a separate world-space entity positioned under the frame by
/// [`update_character_preview_transform`].
fn spawn_character_view(
    commands: &mut Commands,
    player: &PlayerCharacter,
    level: &Level,
    equipment: Option<&PlayerEquipment>,
    ui_font: &UiFont,
    panel_texture: &PanelTexture,
    asset_server: Option<&AssetServer>,
) {
    let attributes = player.attributes;
    commands
        .spawn((view_root(), TownCharacterView))
        .with_children(|root| {
            root.spawn(back_row(TownAction::CloseCharacter, ui_font));
            root.spawn((Text::new("Personaj"), title_font(ui_font), TextColor(CREAM)));

            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::FlexStart,
                column_gap: Val::Px(SPACE_LG),
                row_gap: Val::Px(SPACE_LG),
                max_width: Val::Percent(100.0),
                ..default()
            })
            .with_children(|body| {
                // The preview stage: deliberately *not* a `panel_bundle`
                // (its 9-slice art would paint over the world-space rig) —
                // see `creation::spawn_preview_stage`'s doc comment for the
                // compositing rationale this mirrors.
                body.spawn(Node {
                    width: Val::Px(PREVIEW_STAGE_WIDTH),
                    max_width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(SPACE_SM),
                    ..default()
                })
                .with_children(|stage| {
                    stage.spawn((
                        Text::new(player.name.clone()),
                        ui_font.text_font_bold(30.0),
                        TextColor(CREAM),
                        BackgroundColor(PANEL_LINEN),
                    ));
                    stage.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(PREVIEW_FRAME_HEIGHT),
                            border: UiRect::all(Val::Px(2.0)),
                            ..default()
                        },
                        BorderColor::all(GOLD),
                        TownLayoutRole::CharacterPreviewFrame,
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
                        ))
                        .with_children(|strip| {
                            for text in [
                                format!("HP {}", stats::max_hp(&attributes)),
                                format!("STA {}", stats::max_stamina(&attributes)),
                                format!("DMG {}", stats::base_damage(&attributes)),
                            ] {
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
                                        Text::new(text),
                                        ui_font.text_font(15.0),
                                        TextColor(CREAM),
                                    )],
                                ));
                            }
                        });
                });

                body.spawn((
                    panel_bundle(
                        panel_texture,
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(SPACE_SM),
                            ..default()
                        },
                    ),
                    BackgroundColor(PANEL_LINEN),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new(format!("Nivel {}", level.level)),
                        ui_font.text_font_bold(24.0),
                        TextColor(CREAM),
                    ));
                    for kind in AttributeKind::ALL {
                        panel
                            .spawn(Node {
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::SpaceBetween,
                                column_gap: Val::Px(SPACE_MD),
                                min_width: Val::Px(200.0),
                                ..default()
                            })
                            .with_children(|row| {
                                row.spawn((
                                    Text::new(kind.label()),
                                    ui_font.text_font(18.0),
                                    TextColor(CREAM),
                                ));
                                row.spawn((
                                    Text::new(attributes.get(kind).to_string()),
                                    ui_font.text_font_bold(18.0),
                                    TextColor(CREAM),
                                ));
                            });
                    }
                });
            });
        });

    // The world-space rig, reusing the exact same primitive the creation
    // and shop previews use; positioned for real by
    // `update_character_preview_transform` once UI layout has resolved.
    let preview = commands
        .spawn((
            TownScreen,
            TownCharacterPreview,
            Transform::from_xyz(0.0, PREVIEW_Y, PREVIEW_Z).with_scale(Vec3::splat(PREVIEW_SCALE)),
        ))
        .id();
    spawn_character_definition_rig(
        commands,
        preview,
        &player.definition,
        asset_server,
        false,
        equipment.map(|equipment| &equipment.0),
        None,
    );
}

/// The are-you-sure overlay for leaving to the menu while a run is active:
/// a scrim over the hub with a modal embroidered panel. Leaving keeps the
/// save (**Continuă** resumes at the hub); staying just closes the overlay.
fn spawn_leave_confirm(commands: &mut Commands, ui_font: &UiFont, panel_texture: &PanelTexture) {
    commands.spawn((
        TownScreen,
        TownLeaveConfirm,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(SCRIM),
        // Above the hub, and the scrim swallows clicks aimed at it.
        GlobalZIndex(10),
        children![(
            panel_bundle(
                panel_texture,
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(SPACE_MD),
                    padding: UiRect::all(Val::Px(28.0)),
                    ..default()
                },
            ),
            TownConfirmPanel,
            // #216: a *modal* group — see `crate::combat::pause`'s overlay
            // for the pattern this mirrors.
            TabGroup::modal(),
            children![
                (
                    Text::new("Te întorci la meniu?"),
                    ui_font.text_font(28.0),
                    TextColor(CREAM),
                ),
                (
                    Text::new("Progresul rămâne salvat."),
                    ui_font.text_font(18.0),
                    TextColor(CREAM),
                ),
                (
                    wide_button("Rămâi în sat", ui_font),
                    TownAction::CancelLeave,
                ),
                (wide_button("La meniu", ui_font), TownAction::ConfirmLeave),
            ],
        )],
    ));
}

/// Query filter: buttons whose interaction changed this frame.
type ChangedButton = (Changed<Interaction>, With<Button>);

/// Runs the [`TownAction`] of whichever town button was pressed. Navigation
/// emits a [`FlowIntent`] (the table in `crate::flow` routes it); the
/// character view and the leave-confirm overlay are Town-local spawns with
/// no state change. Leaving to the menu never resets the run or clears the
/// save — the hub-entry checkpoint already persisted it, so **Continuă**
/// resumes right back here.
// A Bevy system: each parameter is a distinct ECS handle the sub-view
// spawns need (player data, fonts, panel art, the live view roots).
#[allow(clippy::too_many_arguments)]
fn handle_town_actions(
    mut commands: Commands,
    interactions: Query<(&Interaction, &TownAction), ChangedButton>,
    player: Option<Res<PlayerCharacter>>,
    level: Option<Res<Level>>,
    equipment: Option<Res<PlayerEquipment>>,
    ui_font: Res<UiFont>,
    panel_texture: Res<PanelTexture>,
    asset_server: Option<Res<AssetServer>>,
    hub_views: Query<Entity, With<TownHubView>>,
    character_views: Query<Entity, With<TownCharacterView>>,
    previews: Query<Entity, With<TownCharacterPreview>>,
    confirms: Query<Entity, With<TownLeaveConfirm>>,
    mut intents: MessageWriter<FlowIntent>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            TownAction::EnterArena => {
                intents.write(FlowIntent::EnterArena);
            }
            TownAction::GoToShop => {
                intents.write(FlowIntent::GoToShop);
            }
            TownAction::ViewCharacter => {
                let Some(player) = player.as_deref() else {
                    warn!("Personaj pressed without a confirmed PlayerCharacter; ignoring");
                    continue;
                };
                for hub in &hub_views {
                    commands.entity(hub).despawn();
                }
                let level = level.as_deref().copied().unwrap_or_default();
                spawn_character_view(
                    &mut commands,
                    player,
                    &level,
                    equipment.as_deref(),
                    &ui_font,
                    &panel_texture,
                    asset_server.as_deref(),
                );
            }
            TownAction::CloseCharacter => {
                for view in &character_views {
                    commands.entity(view).despawn();
                }
                for preview in &previews {
                    commands.entity(preview).despawn();
                }
                spawn_hub_view(&mut commands, &ui_font, &panel_texture);
            }
            TownAction::Back => {
                // Are-you-sure only while a run is active (screen contract);
                // Town is only reachable mid-run, so outside headless tests
                // the overlay always shows.
                if player.is_some() {
                    if confirms.is_empty() {
                        spawn_leave_confirm(&mut commands, &ui_font, &panel_texture);
                    }
                } else {
                    intents.write(FlowIntent::BackToMenu);
                }
            }
            TownAction::ConfirmLeave => {
                intents.write(FlowIntent::BackToMenu);
            }
            TownAction::CancelLeave => {
                for confirm in &confirms {
                    commands.entity(confirm).despawn();
                }
            }
        }
    }
}

/// Focuses the leave-confirm overlay's first control the frame it spawns
/// (#216), with [`PendingAutofocus`] retrying if the panel's children are
/// still a command-flush behind — the same modal-focus contract as
/// `combat::pause::autofocus_pause_overlay`.
fn autofocus_leave_confirm(
    nav: TabNavigation,
    mut focus: ResMut<InputFocus>,
    mut pending: ResMut<PendingAutofocus>,
    panels: Query<Entity, Added<TownConfirmPanel>>,
) {
    for panel in &panels {
        autofocus_first_in_group(&nav, &mut focus, &mut pending, panel);
    }
}

/// Hover/pressed background feedback, same palette as every other screen.
fn update_button_backgrounds(
    mut buttons: Query<(&Interaction, &mut BackgroundColor), ChangedButton>,
) {
    for (interaction, mut background) in &mut buttons {
        background.0 = match interaction {
            Interaction::Pressed => BUTTON_PRESSED,
            Interaction::Hovered => BUTTON_HOVERED,
            Interaction::None => BUTTON_NORMAL,
        };
    }
}

/// Repositions the character-view rig under the preview frame's resolved
/// screen rect, compensating the letterbox zoom — the town counterpart of
/// `creation::update_preview_transform` (see that system's doc comment for
/// why it runs unconditionally after `UiSystems::Layout`).
fn update_character_preview_transform(
    letterbox: Res<LetterboxRect>,
    frames: Query<(&ComputedNode, &UiGlobalTransform, &TownLayoutRole)>,
    mut previews: Query<&mut Transform, With<TownCharacterPreview>>,
) {
    let Some((node, transform)) = frames.iter().find_map(|(node, transform, role)| {
        (*role == TownLayoutRole::CharacterPreviewFrame).then_some((node, transform))
    }) else {
        return;
    };
    let frame_rect = logical_node_rect(transform, node);
    let target = world_point_for_screen_point(frame_rect.center(), *letterbox);
    let zoom = letterbox_zoom(*letterbox);
    for mut preview_transform in &mut previews {
        preview_transform.translation.x = target.x;
        preview_transform.translation.y = target.y + PREVIEW_Y;
        preview_transform.translation.z = PREVIEW_Z;
        preview_transform.scale = Vec3::splat(PREVIEW_SCALE / zoom);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Attributes;
    use crate::core::CorePlugin;
    use crate::flow::FlowPlugin;
    use crate::theme::MIN_TOUCH_TARGET;
    use bevy::state::app::StatesPlugin;

    /// Headless app settled past `Loading` into `MainMenu` (#114), ready for
    /// `set_state(GameState::Town)`.
    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            FlowPlugin,
            TownPlugin,
        ));
        app.update();
        app.update();
        app
    }

    /// A confirmed mid-run hero, so the hub sees an active run.
    fn player_character() -> PlayerCharacter {
        PlayerCharacter {
            name: "Greuceanu".to_string(),
            attributes: Attributes {
                putere: 6,
                agilitate: 2,
                vitalitate: 4,
                noroc: 2,
                atac: 3,
                aparare: 2,
                carisma: 1,
                magie: 1,
            },
            appearance: crate::character::PlayerAppearance::default(),
            definition: crate::character::CharacterDefinition::legacy_human(
                crate::character::PlayerAppearance::default(),
            ),
        }
    }

    fn set_state(app: &mut App, state: GameState) {
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(state);
        app.update();
    }

    fn state(app: &App) -> GameState {
        *app.world().resource::<State<GameState>>().get()
    }

    fn texts(app: &mut App) -> Vec<String> {
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .map(|text| text.0.clone())
            .collect()
    }

    fn count<C: Component>(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<(), With<C>>()
            .iter(app.world())
            .count()
    }

    fn find_button(app: &mut App, action: TownAction) -> Entity {
        app.world_mut()
            .query_filtered::<(Entity, &TownAction), With<Button>>()
            .iter(app.world())
            .find(|&(_, &a)| a == action)
            .map(|(entity, _)| entity)
            .unwrap_or_else(|| panic!("no town button carries {action:?}"))
    }

    /// Presses `button`: one update runs the handler (and queues any
    /// transition), the second applies it — the flow module's two-update
    /// contract.
    fn press(app: &mut App, button: Entity) {
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
        app.update();
    }

    /// Finds the button carrying `action` and presses it.
    fn press_action(app: &mut App, action: TownAction) {
        let button = find_button(app, action);
        press(app, button);
    }

    // --- Hub screen contract (#129) ---

    #[test]
    fn entering_town_spawns_the_hub_per_the_screen_contract() {
        let mut app = test_app();
        set_state(&mut app, GameState::Town);

        let texts = texts(&mut app);
        assert!(texts.contains(&"Satul".to_string()), "one title: {texts:?}");
        assert!(
            texts.contains(&"Luptă în arenă".to_string()),
            "the dominant primary action: {texts:?}"
        );
        assert!(
            texts.contains(&"Prăvălie".to_string()) && texts.contains(&"Personaj".to_string()),
            "both secondary destinations: {texts:?}"
        );
        assert!(
            texts.contains(&"Înapoi".to_string()),
            "the consistent back action: {texts:?}"
        );

        let roles: Vec<TownLayoutRole> = app
            .world_mut()
            .query::<&TownLayoutRole>()
            .iter(app.world())
            .copied()
            .collect();
        for role in [
            TownLayoutRole::ArenaCard,
            TownLayoutRole::ShopCard,
            TownLayoutRole::CharacterCard,
        ] {
            assert!(roles.contains(&role), "missing {role:?}");
        }

        let scroll_roots = app
            .world_mut()
            .query_filtered::<(), (With<TownScreen>, With<crate::ui_widgets::Scrollable>)>()
            .iter(app.world())
            .count();
        assert_eq!(scroll_roots, 1, "short viewports can scroll the hub");
    }

    /// The destination cards are embroidered panels (one border each, no
    /// nested panels): each card carries the panel `ImageNode`, and no
    /// descendant of a card carries another.
    #[test]
    fn destination_cards_use_one_embroidered_border_each_without_nesting() {
        let mut app = test_app();
        set_state(&mut app, GameState::Town);

        let cards: Vec<Entity> = app
            .world_mut()
            .query_filtered::<(Entity, &TownLayoutRole), ()>()
            .iter(app.world())
            .filter(|(_, role)| {
                matches!(
                    role,
                    TownLayoutRole::ArenaCard
                        | TownLayoutRole::ShopCard
                        | TownLayoutRole::CharacterCard
                )
            })
            .map(|(entity, _)| entity)
            .collect();
        assert_eq!(cards.len(), 3);
        for card in cards {
            assert!(
                app.world().entity(card).contains::<ImageNode>(),
                "each card renders the embroidered panel border"
            );
            let children = app
                .world()
                .get::<Children>(card)
                .expect("card has children");
            for child in children.iter() {
                assert!(
                    !app.world().entity(child).contains::<ImageNode>(),
                    "no nested panels inside a card"
                );
            }
        }
    }

    /// The primary action must be the largest button on the screen (screen
    /// contract: one dominant primary action).
    #[test]
    fn the_arena_button_is_the_largest_touch_target_on_the_hub() {
        let mut app = test_app();
        set_state(&mut app, GameState::Town);

        let px = |val: Val| match val {
            Val::Px(px) => px,
            other => panic!("expected a Px button dimension, got {other:?}"),
        };
        let mut primary_area = 0.0;
        let mut best_other = 0.0f32;
        for (node, action) in app
            .world_mut()
            .query_filtered::<(&Node, &TownAction), With<Button>>()
            .iter(app.world())
        {
            let area = px(node.width) * px(node.height);
            let width = px(node.width);
            let height = px(node.height);
            assert!(
                width >= MIN_TOUCH_TARGET && height >= MIN_TOUCH_TARGET,
                "{action:?} touch target {width}x{height} under the {MIN_TOUCH_TARGET}px floor"
            );
            if *action == TownAction::EnterArena {
                primary_area = area;
            } else {
                best_other = best_other.max(area);
            }
        }
        assert!(
            primary_area > best_other,
            "Luptă în arenă ({primary_area}) must out-size every other button ({best_other})"
        );
    }

    // --- Navigation ---

    #[test]
    fn lupta_in_arena_starts_the_fight_and_cleans_up() {
        let mut app = test_app();
        set_state(&mut app, GameState::Town);

        let button = find_button(&mut app, TownAction::EnterArena);
        press(&mut app, button);

        assert_eq!(state(&app), GameState::Fight);
        assert_eq!(count::<TownScreen>(&mut app), 0, "root despawned");
        assert_eq!(count::<Button>(&mut app), 0, "buttons despawned");
        assert_eq!(count::<Text>(&mut app), 0, "labels despawned");
    }

    #[test]
    fn pravalie_leads_to_the_shop() {
        let mut app = test_app();
        set_state(&mut app, GameState::Town);

        let button = find_button(&mut app, TownAction::GoToShop);
        press(&mut app, button);

        assert_eq!(state(&app), GameState::Shop);
        assert_eq!(count::<TownScreen>(&mut app), 0, "root despawned");
    }

    /// #216: Enter on the focused primary button drives the same transition
    /// a click does.
    #[test]
    fn enter_on_the_focused_arena_button_starts_the_fight() {
        let mut app = test_app();
        app.init_resource::<ButtonInput<KeyCode>>();
        set_state(&mut app, GameState::Town);

        let button = find_button(&mut app, TownAction::EnterArena);
        app.world_mut()
            .insert_resource(InputFocus::from_entity(button));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        app.update();

        assert_eq!(state(&app), GameState::Fight);
    }

    // --- The read-only character view ---

    #[test]
    fn personaj_opens_the_read_only_character_view() {
        let mut app = test_app();
        app.insert_resource(player_character());
        app.insert_resource(Level {
            level: 3,
            xp: 40,
            unspent_points: 0,
        });
        set_state(&mut app, GameState::Town);

        let button = find_button(&mut app, TownAction::ViewCharacter);
        press(&mut app, button);

        assert_eq!(
            state(&app),
            GameState::Town,
            "a sub-view, not a state change"
        );
        assert_eq!(count::<TownHubView>(&mut app), 0, "hub view closed");
        assert_eq!(
            count::<TownCharacterView>(&mut app),
            1,
            "character view open"
        );
        assert_eq!(
            count::<TownCharacterPreview>(&mut app),
            1,
            "the live cutout preview rig root is spawned"
        );

        let texts = texts(&mut app);
        assert!(texts.contains(&"Personaj".to_string()), "{texts:?}");
        assert!(texts.contains(&"Greuceanu".to_string()), "{texts:?}");
        assert!(texts.contains(&"Nivel 3".to_string()), "{texts:?}");
        for kind in AttributeKind::ALL {
            assert!(
                texts.contains(&kind.label().to_string()),
                "missing {kind:?} label: {texts:?}"
            );
        }
        assert!(
            texts.contains(&"6".to_string()),
            "attribute values shown: {texts:?}"
        );
        assert!(
            !texts.contains(&"+".to_string()) && !texts.contains(&"-".to_string()),
            "read-only: no stepper buttons: {texts:?}"
        );
        assert!(
            texts.contains(&format!(
                "HP {}",
                stats::max_hp(&player_character().attributes)
            )),
            "derived stats shown: {texts:?}"
        );
    }

    #[test]
    fn closing_the_character_view_returns_to_the_hub() {
        let mut app = test_app();
        app.insert_resource(player_character());
        set_state(&mut app, GameState::Town);

        press_action(&mut app, TownAction::ViewCharacter);
        press_action(&mut app, TownAction::CloseCharacter);

        assert_eq!(state(&app), GameState::Town);
        assert_eq!(count::<TownCharacterView>(&mut app), 0, "view closed");
        assert_eq!(
            count::<TownCharacterPreview>(&mut app),
            0,
            "the preview rig is despawned with the view"
        );
        assert_eq!(count::<TownHubView>(&mut app), 1, "hub is back");
    }

    #[test]
    fn personaj_without_a_confirmed_hero_is_ignored() {
        let mut app = test_app();
        set_state(&mut app, GameState::Town);

        press_action(&mut app, TownAction::ViewCharacter);

        assert_eq!(count::<TownHubView>(&mut app), 1, "hub stays");
        assert_eq!(count::<TownCharacterView>(&mut app), 0);
    }

    // --- Back to menu (are-you-sure only with an active run) ---

    #[test]
    fn back_with_an_active_run_asks_before_leaving_and_cancel_stays() {
        let mut app = test_app();
        app.insert_resource(player_character());
        set_state(&mut app, GameState::Town);

        press_action(&mut app, TownAction::Back);

        assert_eq!(state(&app), GameState::Town, "no navigation yet");
        assert_eq!(count::<TownLeaveConfirm>(&mut app), 1, "overlay shown");
        let texts = texts(&mut app);
        assert!(
            texts.contains(&"Te întorci la meniu?".to_string()),
            "{texts:?}"
        );

        press_action(&mut app, TownAction::CancelLeave);
        assert_eq!(state(&app), GameState::Town);
        assert_eq!(count::<TownLeaveConfirm>(&mut app), 0, "overlay closed");
        assert_eq!(count::<TownHubView>(&mut app), 1, "hub untouched");
    }

    #[test]
    fn confirming_leave_returns_to_the_menu_and_keeps_the_run() {
        let mut app = test_app();
        app.insert_resource(player_character());
        set_state(&mut app, GameState::Town);

        press_action(&mut app, TownAction::Back);
        press_action(&mut app, TownAction::ConfirmLeave);

        assert_eq!(state(&app), GameState::MainMenu);
        assert_eq!(count::<TownScreen>(&mut app), 0, "screen despawned");
        assert!(
            app.world().get_resource::<PlayerCharacter>().is_some(),
            "leaving keeps the run (the save resumes it); never a reset"
        );
    }

    #[test]
    fn back_without_an_active_run_goes_straight_to_the_menu() {
        let mut app = test_app();
        set_state(&mut app, GameState::Town);

        press_action(&mut app, TownAction::Back);

        assert_eq!(state(&app), GameState::MainMenu);
        assert_eq!(count::<TownLeaveConfirm>(&mut app), 0, "no overlay");
    }

    #[test]
    fn pressing_back_twice_never_stacks_two_confirm_overlays() {
        let mut app = test_app();
        app.insert_resource(player_character());
        set_state(&mut app, GameState::Town);

        let back = find_button(&mut app, TownAction::Back);
        press(&mut app, back);
        app.world_mut().entity_mut(back).insert(Interaction::None);
        app.update();
        press(&mut app, back);

        assert_eq!(count::<TownLeaveConfirm>(&mut app), 1);
    }

    // --- Autosave on entry ---

    /// Entering the hub autosaves with [`ResumeDestination::Town`] so a
    /// reload resumes here, even when the previous checkpoint (e.g. a shop
    /// purchase) tagged somewhere else.
    #[test]
    fn entering_town_autosaves_the_town_resume_destination() {
        use crate::items::Equipment;
        use crate::progression::{LifetimeEarnings, Wallet};
        use crate::roster::LadderProgress;
        use crate::save::{SaveGame, SavePlugin, SaveStore};
        use crate::shop::OwnedItems;

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            FlowPlugin,
            TownPlugin,
            SavePlugin,
        ));
        let (store, cell) = SaveStore::in_memory();
        app.insert_resource(store);
        app.insert_resource(player_character());
        app.insert_resource(Level::default());
        app.insert_resource(Wallet(210));
        app.insert_resource(LifetimeEarnings(260));
        app.insert_resource(OwnedItems::default());
        app.insert_resource(PlayerEquipment(Equipment::default()));
        app.insert_resource(LadderProgress(4));
        app.update();
        app.update();

        set_state(&mut app, GameState::Town);
        app.update();

        let json = cell
            .lock()
            .expect("test store lock")
            .clone()
            .expect("town entry autosaves");
        let save = SaveGame::from_json(&json).expect("own JSON loads");
        assert_eq!(save.resume_destination(), ResumeDestination::Town);
        assert_eq!(save.ladder_progress, 4, "current run values are captured");
    }
}
