//! Character creation screen: choose a folklore preset or a custom path,
//! edit attributes and appearance, preview the cutout rig, and enter the
//! arena.
//!
//! The allocation rules live in [`draft`] as pure logic; this module only
//! wires them to Bevy UI following the button pattern from the main menu.

pub mod draft;

use bevy::prelude::*;

pub use draft::{AttributeKind, CharacterDraft, FOLK_NAMES, FREE_POINTS, HeroChoice, HeroPreset};

use crate::character::{Attributes, PlayerAppearance, stats};
use crate::core::{GameState, UiFont, despawn_screen};
use crate::cutout::{CutoutRig, human_template_for, spawn_cutout_rig_with_gear};
use crate::items::Equipment;
use crate::menu::DisabledButton;
use crate::save::SaveRequested;
use crate::shop::{OwnedItems, PlayerEquipment};
use crate::theme::{
    ARENA_BROWN, BUTTON_DISABLED, BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED, CREAM, FOLK_BLUE,
    GOLD, PANEL_LINEN, PanelTexture, TEXT_DISABLED, WALNUT, panel_bundle,
};
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

/// The confirmed player character: chosen name, final attributes, and saved
/// appearance. Written by the confirm button and read by the fight screen.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct PlayerCharacter {
    pub name: String,
    pub attributes: Attributes,
    pub appearance: PlayerAppearance,
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
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum AppearanceField {
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

/// What a creation-screen button does when pressed.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum CreationAction {
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
            .init_resource::<CharacterDraft>()
            .add_plugins(crate::ui_widgets::ScrollInputPlugin)
            .add_systems(OnEnter(GameState::CharacterCreation), spawn_creation_screen)
            .add_systems(
                Update,
                (
                    handle_creation_actions,
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
                ..default()
            },
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
                    spawn_preview_stage(body, &draft, &ui_font, &panel_texture);
                    spawn_control_deck(body, &draft, &ui_font, &panel_texture);
                });
        });

    let preview = commands
        .spawn((
            CreationScreen,
            CreationPreview,
            creation_preview_transform(),
        ))
        .id();
    let equipment = equipment_from_items(draft.starter_items());
    spawn_cutout_rig_with_gear(
        &mut commands,
        preview,
        human_template_for(draft.appearance()),
        asset_server.as_deref(),
        false,
        &equipment,
    );
}

fn spawn_preview_stage(
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
                    width: Val::Px(CREATION_PREVIEW_STAGE_WIDTH),
                    max_width: Val::Percent(100.0),
                    min_height: Val::Px(CREATION_PANEL_HEIGHT),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceBetween,
                    padding: UiRect::all(Val::Px(18.0)),
                    ..default()
                },
            ),
            BackgroundColor(PANEL_LINEN),
            CreationLayoutRole::PreviewStage,
        ))
        .with_children(|stage| {
            stage.spawn((
                Text::new(draft.name()),
                ui_font.text_font_bold(30.0),
                TextColor(CREAM),
                CreationLabel::Name,
            ));
            stage.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(CREATION_PREVIEW_FRAME_HEIGHT),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(WALNUT),
                BorderColor::all(GOLD),
            ));
            stage
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(6.0),
                        ..default()
                    },
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
                            BackgroundColor(FOLK_BLUE),
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
                    row_gap: Val::Px(8.0),
                    padding: UiRect::all(Val::Px(16.0)),
                    ..default()
                },
            ),
            BackgroundColor(PANEL_LINEN),
            CreationLayoutRole::ControlDeck,
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
                        button_bundle(choice.label(), Val::Px(118.0), Val::Px(42.0), 15.0, ui_font),
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
                    min_height: Val::Px(46.0),
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
                row.spawn(Node {
                    width: Val::Px(232.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                })
                .with_children(|slot| {
                    slot.spawn((
                        Text::new(draft.name()),
                        ui_font.text_font(24.0),
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

            deck.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
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
            row.spawn(Node {
                width: Val::Px(108.0),
                ..default()
            })
            .with_children(|slot| {
                slot.spawn((Text::new(label), ui_font.text_font(18.0), TextColor(CREAM)));
            });
            row.spawn((
                button_bundle("<", Val::Px(36.0), Val::Px(36.0), 20.0, ui_font),
                previous,
            ));
            row.spawn(Node {
                width: Val::Px(156.0),
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
                button_bundle(">", Val::Px(36.0), Val::Px(36.0), 20.0, ui_font),
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

fn creation_preview_stage_center_x() -> f32 {
    -CREATION_BODY_WIDTH / 2.0 + CREATION_PREVIEW_STAGE_WIDTH / 2.0
}

fn creation_preview_transform() -> Transform {
    Transform::from_xyz(
        creation_preview_stage_center_x(),
        CREATION_PREVIEW_Y,
        PREVIEW_Z,
    )
    .with_scale(Vec3::splat(CREATION_PREVIEW_SCALE))
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
    mut next_state: ResMut<NextState<GameState>>,
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
                    });
                    let equipment = equipment_from_items(draft.starter_items());
                    commands.insert_resource(OwnedItems(
                        draft.starter_items().iter().copied().collect(),
                    ));
                    commands.insert_resource(PlayerEquipment(equipment));
                    save_requests.write(SaveRequested);
                    draft.reset();
                    next_state.set(GameState::Fight);
                }
            }
            CreationAction::Back => {
                draft.reset();
                next_state.set(GameState::MainMenu);
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
        spawn_cutout_rig_with_gear(
            &mut commands,
            preview,
            human_template_for(draft.appearance()),
            asset_server.as_deref(),
            false,
            &equipment,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{AccentColor, BodyBuild, HairStyle, SkinTone};
    use crate::core::CorePlugin;
    use crate::cutout::{CutoutPartKind, CutoutPartMarker, GearVisualLayer, human_template};
    use crate::items::ItemId;
    use crate::save::{SaveGame, SavePlugin, SaveStore};
    use bevy::state::app::StatesPlugin;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, CorePlugin, CreationPlugin));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::CharacterCreation);
        app.update();
        app.update();
        app
    }

    fn test_app_with_save() -> (App, std::sync::Arc<std::sync::Mutex<Option<String>>>) {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            CorePlugin,
            CreationPlugin,
            SavePlugin,
        ));
        let (store, cell) = SaveStore::in_memory();
        app.insert_resource(store);
        app.insert_resource(crate::progression::Level::default());
        app.insert_resource(crate::progression::Wallet::default());
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
        assert_eq!(buttons, 25);
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
    }

    #[test]
    fn creation_preview_rig_is_centered_from_stage_constants() {
        let mut app = test_app();
        let transform = app
            .world_mut()
            .query_filtered::<&Transform, With<CreationPreview>>()
            .single(app.world())
            .expect("creation preview transform exists");
        let expected = creation_preview_transform();
        assert_eq!(transform.translation, expected.translation);
        assert_eq!(transform.scale, expected.scale);
        assert!((transform.translation.x - creation_preview_stage_center_x()).abs() < f32::EPSILON);
        assert!(transform.translation.x.abs() <= CREATION_PREVIEW_STAGE_WIDTH / 2.0);
        assert!(transform.translation.y.abs() <= CREATION_PREVIEW_FRAME_HEIGHT / 2.0);
        assert_eq!(
            app.world_mut()
                .query_filtered::<(), (With<CreationScreen>, With<crate::ui_widgets::Scrollable>)>()
                .iter(app.world())
                .count(),
            0,
            "preview stage cannot scroll independently from the world rig"
        );
    }

    #[test]
    fn entering_creation_spawns_a_cutout_preview() {
        let mut app = test_app();
        let preview = app
            .world_mut()
            .query_filtered::<Entity, (With<CreationPreview>, With<CutoutRig>)>()
            .single(app.world())
            .expect("one cutout preview root exists");
        let children = app
            .world()
            .get::<Children>(preview)
            .expect("preview has rig children")
            .to_vec();
        let parts = children
            .into_iter()
            .filter(|child| app.world().get::<CutoutPartMarker>(*child).is_some())
            .count();
        assert_eq!(parts, human_template().parts.len());
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
        let part_info: Vec<(Entity, Entity, CutoutPartKind)> = app
            .world_mut()
            .query::<(Entity, &CutoutPartMarker, &ChildOf)>()
            .iter(app.world())
            .map(|(part, marker, child_of)| (part, child_of.parent(), marker.kind))
            .collect();
        let mut layers: Vec<(ItemId, CutoutPartKind)> = app
            .world_mut()
            .query::<(&GearVisualLayer, &ChildOf)>()
            .iter(app.world())
            .filter_map(|(layer, child_of)| {
                let (_, owner, kind) = part_info
                    .iter()
                    .find(|(part, _, _)| *part == child_of.parent())?;
                (*owner == preview).then_some((layer.item, *kind))
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
                noroc: 3,
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
            "Puncte rămase: 10"
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
            "Puncte rămase: 9"
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
}
