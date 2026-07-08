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
use crate::cutout::{CutoutRig, human_template_for, spawn_cutout_rig};
use crate::items::Equipment;
use crate::menu::DisabledButton;
use crate::save::SaveRequested;
use crate::shop::{OwnedItems, PlayerEquipment};
use crate::theme::{
    BUTTON_DISABLED, BUTTON_HOVERED, BUTTON_NORMAL, BUTTON_PRESSED, CREAM, NIGHT_BLACK,
    TEXT_DISABLED,
};
use crate::ui_widgets::{
    attribute_row::spawn_attribute_row, button_bundle, scroll_with_wheel_and_touch, small_button,
    wide_button,
};

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
    Preview,
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
    asset_server: Option<Res<AssetServer>>,
) {
    commands.spawn((
        CreationScreen,
        Sprite::from_color(NIGHT_BLACK, Vec2::new(800.0, 600.0)),
        Transform::from_xyz(0.0, 0.0, -40.0),
    ));

    commands
        .spawn((
            CreationScreen,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(12.0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            crate::ui_widgets::Scrollable,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Creează-ți eroul"),
                ui_font.text_font_bold(44.0),
                TextColor(CREAM),
                Node {
                    margin: UiRect::bottom(Val::Px(12.0)),
                    ..default()
                },
            ));

            parent
                .spawn(Node {
                    width: Val::Px(680.0),
                    max_width: Val::Percent(94.0),
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0),
                    row_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|row| {
                    for choice in HeroChoice::ALL {
                        row.spawn((
                            button_bundle(
                                choice.label(),
                                Val::Px(150.0),
                                Val::Px(46.0),
                                18.0,
                                &ui_font,
                            ),
                            CreationAction::SelectChoice(choice),
                        ));
                    }
                });

            parent.spawn((
                Text::new(description_text(&draft)),
                ui_font.text_font(18.0),
                TextColor(CREAM),
                CreationLabel::Description,
                Node {
                    width: Val::Px(680.0),
                    max_width: Val::Percent(94.0),
                    margin: UiRect::bottom(Val::Px(8.0)),
                    ..default()
                },
            ));

            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(16.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((small_button("<", &ui_font), CreationAction::PreviousName));
                    row.spawn(Node {
                        width: Val::Px(320.0),
                        justify_content: JustifyContent::Center,
                        ..default()
                    })
                    .with_children(|slot| {
                        slot.spawn((
                            Text::new(draft.name()),
                            ui_font.text_font(30.0),
                            TextColor(CREAM),
                            CreationLabel::Name,
                        ));
                    });
                    row.spawn((small_button(">", &ui_font), CreationAction::NextName));
                });

            parent.spawn((
                Text::new(points_text(&draft)),
                ui_font.text_font(24.0),
                TextColor(CREAM),
                CreationLabel::Points,
                Node {
                    margin: UiRect::vertical(Val::Px(4.0)),
                    ..default()
                },
            ));

            for field in [
                AppearanceField::SkinTone,
                AppearanceField::Build,
                AppearanceField::Hair,
                AppearanceField::Accent,
            ] {
                spawn_option_row(
                    parent,
                    field.label(),
                    appearance_text(&draft, field),
                    CreationAction::PreviousAppearance(field),
                    CreationAction::NextAppearance(field),
                    CreationLabel::Appearance(field),
                    &ui_font,
                );
            }

            for kind in AttributeKind::ALL {
                spawn_attribute_row(
                    parent,
                    kind,
                    draft.get(kind),
                    CreationAction::Decrease(kind),
                    CreationAction::Increase(kind),
                    CreationLabel::Value(kind),
                    &ui_font,
                );
            }

            parent.spawn((
                Text::new(preview_text(&draft)),
                ui_font.text_font(22.0),
                TextColor(CREAM),
                CreationLabel::Preview,
                Node {
                    margin: UiRect::vertical(Val::Px(8.0)),
                    ..default()
                },
            ));

            parent.spawn((
                wide_button("Începe lupta", &ui_font),
                CreationAction::Confirm,
            ));
            parent.spawn((wide_button("Înapoi", &ui_font), CreationAction::Back));
        });

    let preview = commands
        .spawn((
            CreationScreen,
            CreationPreview,
            Transform::from_xyz(255.0, 5.0, 25.0).with_scale(Vec3::splat(0.82)),
        ))
        .id();
    spawn_cutout_rig(
        &mut commands,
        preview,
        human_template_for(draft.appearance()),
        asset_server.as_deref(),
        false,
    );
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
            column_gap: Val::Px(12.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node {
                width: Val::Px(140.0),
                ..default()
            })
            .with_children(|slot| {
                slot.spawn((Text::new(label), ui_font.text_font(24.0), TextColor(CREAM)));
            });
            row.spawn((small_button("<", ui_font), previous));
            row.spawn(Node {
                width: Val::Px(180.0),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|slot| {
                slot.spawn((
                    Text::new(value),
                    ui_font.text_font(24.0),
                    TextColor(CREAM),
                    value_label,
                ));
            });
            row.spawn((small_button(">", ui_font), next));
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

fn preview_text(draft: &CharacterDraft) -> String {
    let attrs = draft.attributes();
    format!(
        "HP: {} | Stamina: {} | Damage: {}",
        stats::max_hp(&attrs),
        stats::max_stamina(&attrs),
        stats::base_damage(&attrs),
    )
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
                    let mut equipment = Equipment::default();
                    for &item in draft.starter_items() {
                        equipment.equip(item);
                    }
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
            CreationLabel::Preview => preview_text(&draft),
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
        spawn_cutout_rig(
            &mut commands,
            preview,
            human_template_for(draft.appearance()),
            asset_server.as_deref(),
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{AccentColor, BodyBuild, HairStyle, SkinTone};
    use crate::core::CorePlugin;
    use crate::cutout::{CutoutPartMarker, human_template};
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
            label_text(&mut app, CreationLabel::Preview),
            "HP: 60 | Stamina: 35 | Damage: 3"
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
            label_text(&mut app, CreationLabel::Preview),
            "HP: 70 | Stamina: 40 | Damage: 3"
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
