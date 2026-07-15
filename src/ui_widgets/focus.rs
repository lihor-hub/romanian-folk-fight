//! Shared keyboard/gamepad focus navigation (#213, a child of #143), built to
//! be reused by every later screen that needs it (#216) rather than grown
//! bespoke per screen.
//!
//! ## Why this wraps Bevy's own focus primitives instead of inventing a
//! second "current focused entity"
//!
//! Bevy already ships a focus model (`bevy::input_focus`, on by default via
//! this crate's `ui` feature): [`InputFocus`] is the one resource tracking
//! which entity has focus, and
//! [`tab_navigation`](bevy::input_focus::tab_navigation) supplies an ordered
//! traversal keyed off [`TabGroup`]/[`TabIndex`]. This module does not
//! duplicate that -- it only adds the pieces the engine's crate deliberately
//! leaves to a "widget crate" (see that crate's own module docs): reading
//! this game's actual keyboard/gamepad input to drive [`TabNavigation`],
//! activating the focused control the same way a pointer click already
//! does, and rendering a visible marker. Bevy's own `TabNavigationPlugin`/
//! `InputDispatchPlugin` are *not* added here: both dispatch through a
//! `PrimaryWindow` observer chain (real windowing + `bevy_picking`), which
//! headless tests in this codebase (`MinimalPlugins`, no window) don't run --
//! every system below reads `ButtonInput`/`Query<&Gamepad>` directly instead,
//! so it works identically in a headless test app and the real windowed
//! build.
//!
//! ## Registration API (read this before wiring a new screen for #216)
//!
//! 1. Add [`FocusNavigationPlugin`] to the screen's plugin (idempotent --
//!    safe to add from more than one screen plugin, matching
//!    [`super::ScrollInputPlugin`]'s pattern).
//! 2. Wrap the screen's focusable root (the panel/bar/row every reachable
//!    control lives under) in `TabGroup::new(0)`.
//! 3. Give every reachable control both [`Focusable`] and `TabIndex(0)`.
//!    Every control in a group gets the *same* index deliberately: ties are
//!    broken by tree order (`Children` iteration, i.e. spawn order), which
//!    is already each screen's left-to-right / top-to-bottom visual order,
//!    so there is no separate index to keep in sync as controls are added,
//!    removed, or reordered. A [`Focusable`] entity that stops existing
//!    (e.g. a closed phone category's action buttons, #199) simply drops out
//!    of the traversal the very next frame -- "focus order matches only
//!    currently visible controls" falls out of this for free, it is not a
//!    separate mechanism to maintain.
//! 4. Give a control `Button` (as every clickable control in this codebase
//!    already does) to let [`activate_focused_control`] "select" it: Enter,
//!    Space, or a gamepad's South button sets `Interaction::Pressed` on the
//!    focused entity -- the exact same write a pointer click produces (and
//!    the same mechanism the `review` feature's `pressButton`/
//!    `pressActionCategory` commands already use, see `crate::review`'s
//!    module docs), so a screen's *existing* `Changed<Interaction>`-gated
//!    handler needs no separate keyboard/gamepad-aware code path. A control
//!    the caller has marked `crate::menu::DisabledButton` stays focusable
//!    (so its disabled reason stays readable/announced) but the resulting
//!    press is inert wherever the screen's own handler already filters
//!    `Without<DisabledButton>` -- true of every button handler in this
//!    codebase already, so "disabled controls never emit, on any input
//!    path" needs no extra code here.
//! 5. If a screen despawns some of its focusable controls in response to its
//!    own state (the phone palette closing/switching a category is the
//!    first example), call [`redirect_focus_if_inside`] with the about-to-
//!    despawn entities *before* despawning them, and a documented fallback
//!    entity (or `None` to just clear focus) -- see
//!    `combat::action_palette::sync_phone_open_category` for the concrete
//!    pattern.
//! 6. Order any system whose behavior depends on "was the focused control
//!    just activated this frame" `.after(FocusNavigationSet)` (see that
//!    set's docs) -- exactly the same ordering `crate::flow::FlowIntentEmission`
//!    documents for its own same-frame-observation requirement.
//!
//! ## Visible marker
//!
//! The focused control gets a high-contrast gold ring
//! ([`crate::theme::GOLD`]) via `bevy_ui`'s `Outline` component, which never
//! affects layout (a sibling's flex box does not shift when the ring
//! appears/disappears). Every [`Focusable`] is given an (initially
//! invisible) `Outline` once by [`ensure_focus_outline`] so
//! [`render_focus_marker`] only ever *mutates* its color -- `Outline`'s own
//! docs recommend against repeatedly inserting/removing the component.

pub use bevy::input_focus::tab_navigation::{NavAction, TabGroup, TabIndex, TabNavigation};
pub use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;

use crate::theme::GOLD;

/// Width of the rendered focus ring, in logical px.
const FOCUS_RING_WIDTH: f32 = 3.0;
/// Gap between a focusable control's own border and the ring, in logical px.
const FOCUS_RING_OFFSET: f32 = 2.0;

/// Marks an entity as one stop in a focus region's tab order. See this
/// module's registration API doc for the full contract (`TabIndex`,
/// `TabGroup`, `Button`).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Focusable;

/// System set every system in this module runs in. A screen's own
/// activation-sensitive handler (its `Changed<Interaction>`-gated click
/// system) orders itself `.after(FocusNavigationSet)` so a same-frame Enter/
/// gamepad-South press is observed this same `Update` pass instead of one
/// frame later -- the same reasoning `crate::flow::FlowIntentEmission`
/// documents for the `review` feature's `pressButton` seam.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FocusNavigationSet;

/// Registers [`InputFocus`] and the keyboard/gamepad navigation, activation,
/// and focus-marker-rendering systems. Idempotent (safe to add from more
/// than one screen plugin, the same defensive pattern
/// [`super::ScrollInputPlugin`] already uses) -- a screen opts into visible
/// navigation purely by tagging its own controls per this module's
/// registration API; this plugin itself never needs per-screen
/// configuration.
pub struct FocusNavigationPlugin;

impl Plugin for FocusNavigationPlugin {
    fn build(&self, app: &mut App) {
        // #216: every screen plugin adds this plugin per this module's
        // registration API step 1, and unlike the single-screen test apps in
        // this file, the real app has every screen plugin loaded at once, so
        // `build` runs once per screen. `App::add_boxed_plugin` only records
        // a plugin's name *after* `build` returns (see its own source), so
        // this check is `false` on the very first build (the systems below
        // are added exactly once) and `true` on every later one (they are
        // skipped) -- without this guard, `Update` would carry one full copy
        // of this chain per screen plugin, and a single Tab press would
        // walk focus forward once per copy in the same frame instead of
        // once overall (see this module's
        // `adding_the_plugin_from_two_screen_plugins_still_advances_focus_by_one_step_per_tab`
        // test).
        if app.is_plugin_added::<Self>() {
            return;
        }
        app.init_resource::<InputFocus>().add_systems(
            Update,
            (
                ensure_focus_outline,
                navigate_keyboard_focus,
                navigate_gamepad_focus,
                activate_focused_control,
                scroll_focused_into_view,
                render_focus_marker,
            )
                .chain()
                .in_set(FocusNavigationSet),
        );
    }

    fn is_unique(&self) -> bool {
        false
    }
}

/// Ensures every currently [`Focusable`] entity carries an `Outline`
/// (initially invisible) so [`render_focus_marker`] only ever mutates
/// `Outline::color` on an already-present component -- seeing this system
/// run once per newly spawned focusable is expected and cheap.
fn ensure_focus_outline(
    mut commands: Commands,
    missing: Query<Entity, (With<Focusable>, Without<Outline>)>,
) {
    for entity in &missing {
        commands.entity(entity).insert(Outline {
            width: Val::Px(FOCUS_RING_WIDTH),
            offset: Val::Px(FOCUS_RING_OFFSET),
            color: Color::NONE,
        });
    }
}

/// Keyboard tab order: `Tab`/`Shift+Tab`, or the arrow keys (right/down as
/// [`NavAction::Next`], left/up as [`NavAction::Previous`]) -- every current
/// focus region (the desktop bar's single row, the phone bar's two rows) is
/// a flat, one-dimensional order, so a linear next/previous model (not 2D
/// directional nav) is all either needs.
fn navigate_keyboard_focus(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    nav: TabNavigation,
    mut focus: ResMut<InputFocus>,
) {
    let Some(keys) = keys else { return };
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let action = if keys.just_pressed(KeyCode::Tab) {
        Some(if shift {
            NavAction::Previous
        } else {
            NavAction::Next
        })
    } else if keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::ArrowDown) {
        Some(NavAction::Next)
    } else if keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::ArrowUp) {
        Some(NavAction::Previous)
    } else {
        None
    };
    let Some(action) = action else { return };
    navigate_with_fallback(&nav, &mut focus, action);
}

/// Gamepad tab order: any connected gamepad's D-pad drives the same linear
/// order keyboard input does (right/down = next, left/up = previous).
fn navigate_gamepad_focus(
    gamepads: Query<&Gamepad>,
    nav: TabNavigation,
    mut focus: ResMut<InputFocus>,
) {
    let mut next = false;
    let mut previous = false;
    for gamepad in &gamepads {
        next |= gamepad.any_just_pressed([GamepadButton::DPadRight, GamepadButton::DPadDown]);
        previous |= gamepad.any_just_pressed([GamepadButton::DPadLeft, GamepadButton::DPadUp]);
    }
    let action = if next {
        Some(NavAction::Next)
    } else if previous {
        Some(NavAction::Previous)
    } else {
        None
    };
    let Some(action) = action else { return };
    navigate_with_fallback(&nav, &mut focus, action);
}

/// Navigates `focus` by `action`, falling back to "as if nothing were
/// focused" if the current focus can't be resolved (#216).
///
/// A real windowed app is observed to leave [`InputFocus`] pointing at the
/// primary window entity (not `None`) before the player ever interacts with
/// a focusable control (unclear which upstream system sets this -- the
/// game's own code never does; discovered via the `keyboard-accessibility`
/// browser scenario, which found the very first ArrowRight press in a real
/// browser session did nothing at all). `TabNavigation::navigate` treats
/// "focus is `Some(x)` but `x` has no `TabGroup` ancestor" as a hard error
/// (`NoTabGroupForCurrentFocus`) rather than falling back the way "focus is
/// `None`" already does -- so a first press silently changed nothing.
/// Retrying with an explicitly-cleared `InputFocus` reproduces the "nothing
/// focused yet" success path for this case too, without weakening any other
/// behavior: a genuinely-invalid `Err` (no tab groups/focusable entities at
/// all) still leaves `focus` untouched either way.
fn navigate_with_fallback(nav: &TabNavigation, focus: &mut InputFocus, action: NavAction) {
    if let Ok(next) = nav.navigate(focus, action) {
        focus.set(next, FocusCause::Navigated);
        return;
    }
    if focus.get().is_some()
        && let Ok(next) = nav.navigate(&InputFocus::default(), action)
    {
        focus.set(next, FocusCause::Navigated);
    }
}

/// "Selects" the currently focused control on Enter/Space (keyboard) or the
/// gamepad's South button: sets `Interaction::Pressed` on it, exactly the
/// write a pointer click already produces. See this module's registration
/// API doc for why a disabled focused control is safe to press here (the
/// screen's own handler is what filters it out).
fn activate_focused_control(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    gamepads: Query<&Gamepad>,
    focus: Res<InputFocus>,
    mut buttons: Query<&mut Interaction, With<Button>>,
) {
    let keyboard_activate = keys
        .is_some_and(|keys| keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space));
    let gamepad_activate = gamepads
        .iter()
        .any(|gamepad| gamepad.just_pressed(GamepadButton::South));
    if !keyboard_activate && !gamepad_activate {
        return;
    }
    let Some(entity) = focus.get() else { return };
    if let Ok(mut interaction) = buttons.get_mut(entity) {
        *interaction = Interaction::Pressed;
    }
}

/// Scrolls the focused control's scrollable ancestors just far enough that
/// the control is fully visible (#216). Several screens rely on designed
/// in-UI scrolling (`super::Scrollable` + `ScrollPosition`, #31) to fit
/// short viewports -- 200% desktop zoom included -- but wheel/touch-drag
/// are pointer inputs; without this, a keyboard-only player tabbing to an
/// off-screen control would move an invisible focus marker.
///
/// Both rects come from the same live layout values
/// (`ComputedNode`/`UiGlobalTransform`, physical px scaled back to logical
/// by the node's own `inverse_scale_factor`), which already reflect the
/// *current* scroll offset -- so the delta between the control's box and
/// the container's visible box is exactly how far the scroll must move.
/// Bevy UI's layout system clamps an out-of-range `ScrollPosition` back
/// into range on its own. Runs only on an actual focus change; a fully
/// visible control never mutates the scroll (no change-detection noise).
fn scroll_focused_into_view(
    focus: Res<InputFocus>,
    parents: Query<&ChildOf>,
    nodes: Query<(&ComputedNode, &UiGlobalTransform), With<Focusable>>,
    mut scrollables: Query<(&mut ScrollPosition, &ComputedNode, &UiGlobalTransform)>,
) {
    if !focus.is_changed() {
        return;
    }
    let Some(focused) = focus.get() else { return };
    let Ok((node, transform)) = nodes.get(focused) else {
        return;
    };
    let scale = node.inverse_scale_factor();
    let target = Rect::from_center_size(transform.translation * scale, node.size() * scale);

    let mut current = focused;
    while let Ok(child_of) = parents.get(current) {
        current = child_of.parent();
        let Ok((mut scroll, container_node, container_transform)) = scrollables.get_mut(current)
        else {
            continue;
        };
        let container_scale = container_node.inverse_scale_factor();
        let container = Rect::from_center_size(
            container_transform.translation * container_scale,
            container_node.size() * container_scale,
        );
        let delta_y = if target.max.y > container.max.y {
            target.max.y - container.max.y
        } else if target.min.y < container.min.y {
            target.min.y - container.min.y
        } else {
            0.0
        };
        let delta_x = if target.max.x > container.max.x {
            target.max.x - container.max.x
        } else if target.min.x < container.min.x {
            target.min.x - container.min.x
        } else {
            0.0
        };
        if delta_x != 0.0 || delta_y != 0.0 {
            scroll.0.x += delta_x;
            scroll.0.y += delta_y;
        }
    }
}

/// Renders the focus marker: whichever [`Focusable`] entity [`InputFocus`]
/// currently names gets a gold `Outline`; every other one is cleared to
/// `Color::NONE`. Cheap to run unconditionally -- every current focus region
/// has at most a handful of entities.
fn render_focus_marker(
    focus: Res<InputFocus>,
    mut outlines: Query<(Entity, &mut Outline), With<Focusable>>,
) {
    let current = focus.get();
    for (entity, mut outline) in &mut outlines {
        let wanted = if Some(entity) == current {
            GOLD
        } else {
            Color::NONE
        };
        if outline.color != wanted {
            outline.color = wanted;
        }
    }
}

/// Sets [`InputFocus`] to the first focusable descendant of `group_root` (a
/// [`TabGroup`] entity) -- the shared building block behind "opening a modal
/// overlay (settings, pause) makes it the tab target immediately" (#216).
///
/// This is necessary, not cosmetic: [`TabNavigation::navigate`] only treats a
/// [`TabGroup::modal`] specially once the *current* focus is already one of
/// its descendants (see this module's doc comment on the registration API);
/// while focus is still unset or sitting on the screen behind the scrim, Tab
/// keeps cycling that screen's own non-modal group, and the modal overlay is
/// never reached. Call this once, right after spawning the overlay (e.g. in
/// the same system that spawns it), so the very next Tab press already
/// starts confined to the overlay.
///
/// A no-op if `group_root` has no focusable descendant yet.
pub fn autofocus_first_in_group(nav: &TabNavigation, focus: &mut InputFocus, group_root: Entity) {
    if let Ok(entity) = nav.initialize(group_root, NavAction::First) {
        focus.set(entity, FocusCause::Navigated);
    }
}

/// If [`InputFocus`] currently names one of `despawning`, redirects it to
/// `fallback` (or clears it if `fallback` is `None`) -- the shared building
/// block behind "closing a category moves focus to its category control or
/// documented safe neighbor" (#213). Call this *before* the entities in
/// `despawning` are actually despawned (set membership is all that matters;
/// `Commands`-issued despawns apply later in the schedule anyway).
pub fn redirect_focus_if_inside(
    focus: &mut InputFocus,
    despawning: impl IntoIterator<Item = Entity>,
    fallback: Option<Entity>,
) {
    let Some(current) = focus.get() else {
        return;
    };
    if despawning.into_iter().any(|entity| entity == current) {
        match fallback {
            Some(entity) => focus.set(entity, FocusCause::Navigated),
            None => focus.clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Affine2;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(FocusNavigationPlugin);
        app.init_resource::<ButtonInput<KeyCode>>();
        app
    }

    /// Spawns a `TabGroup` root with `count` `Focusable`+`TabIndex(0)`
    /// children, in order, returning the root and the children's entities.
    fn spawn_group(app: &mut App, count: usize) -> (Entity, Vec<Entity>) {
        let root = app.world_mut().spawn(TabGroup::new(0)).id();
        let mut children = Vec::new();
        for _ in 0..count {
            let child = app
                .world_mut()
                .spawn((Button, Focusable, TabIndex(0), ChildOf(root)))
                .id();
            children.push(child);
        }
        (root, children)
    }

    fn press_and_settle(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
    }

    #[test]
    fn adding_the_plugin_from_two_screen_plugins_still_advances_focus_by_one_step_per_tab() {
        // #216: every screen plugin adds `FocusNavigationPlugin` per this
        // module's registration API step 1, and more than one such plugin is
        // present in the real app at once (unlike a single-screen test app).
        // If re-adding the plugin actually re-registered its systems, one
        // key press would advance focus by as many steps as there are
        // registrations -- this guards against that regression.
        let mut app = test_app();
        app.add_plugins(FocusNavigationPlugin);
        app.add_plugins(FocusNavigationPlugin);
        let (_, children) = spawn_group(&mut app, 3);
        app.world_mut()
            .insert_resource(InputFocus::from_entity(children[0]));

        press_and_settle(&mut app, KeyCode::Tab);
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(children[1]),
            "one Tab press must move focus by exactly one step, no matter how many \
             screen plugins added FocusNavigationPlugin"
        );
    }

    /// #216: a real windowed app is observed to leave [`InputFocus`] pointing
    /// at the primary window entity (not `None`) before the player ever
    /// interacts with a focusable control -- unlike this module's other
    /// tests, which start from a bare `MinimalPlugins` world where
    /// `InputFocus` really is `None` until something sets it. `TabNavigation
    /// ::navigate` treats "focus is `Some(x)` but `x` has no `TabGroup`
    /// ancestor" as a hard error (`NoTabGroupForCurrentFocus`) rather than
    /// falling back to "as if nothing were focused" -- so the very first
    /// ArrowRight/Tab press in a real browser session silently did nothing
    /// at all (discovered via the `keyboard-accessibility` browser scenario,
    /// #216). Reproduced headlessly here with a bare, unrelated entity
    /// standing in for the window.
    #[test]
    fn tab_recovers_when_focus_starts_on_an_unrelated_entity_with_no_tab_group_ancestor() {
        let mut app = test_app();
        let (_, children) = spawn_group(&mut app, 3);
        let window_stand_in = app.world_mut().spawn_empty().id();
        app.world_mut()
            .insert_resource(InputFocus::from_entity(window_stand_in));

        press_and_settle(&mut app, KeyCode::Tab);
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(children[0]),
            "Tab must still land on the first focusable control, not silently do nothing"
        );
    }

    #[test]
    fn tab_moves_focus_to_the_next_focusable_in_tree_order() {
        let mut app = test_app();
        let (_, children) = spawn_group(&mut app, 3);
        app.world_mut()
            .insert_resource(InputFocus::from_entity(children[0]));

        press_and_settle(&mut app, KeyCode::Tab);
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(children[1])
        );

        press_and_settle(&mut app, KeyCode::Tab);
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(children[2])
        );

        // Wraps back to the first.
        press_and_settle(&mut app, KeyCode::Tab);
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(children[0])
        );
    }

    #[test]
    fn shift_tab_moves_focus_backward() {
        let mut app = test_app();
        let (_, children) = spawn_group(&mut app, 3);
        app.world_mut()
            .insert_resource(InputFocus::from_entity(children[1]));

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        press_and_settle(&mut app, KeyCode::Tab);
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(children[0])
        );
    }

    #[test]
    fn arrow_keys_are_an_alternative_linear_order_to_tab() {
        let mut app = test_app();
        let (_, children) = spawn_group(&mut app, 3);
        app.world_mut()
            .insert_resource(InputFocus::from_entity(children[0]));

        press_and_settle(&mut app, KeyCode::ArrowRight);
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(children[1])
        );

        press_and_settle(&mut app, KeyCode::ArrowLeft);
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(children[0])
        );
    }

    #[test]
    fn a_despawned_focusable_drops_out_of_the_order_on_the_next_frame() {
        let mut app = test_app();
        let (_, children) = spawn_group(&mut app, 3);
        app.world_mut().entity_mut(children[1]).despawn();
        app.world_mut()
            .insert_resource(InputFocus::from_entity(children[0]));

        // Tab must skip the despawned middle entity and land on the third.
        press_and_settle(&mut app, KeyCode::Tab);
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(children[2])
        );
    }

    #[test]
    fn enter_activates_the_focused_button() {
        let mut app = test_app();
        let (_, children) = spawn_group(&mut app, 2);
        app.world_mut()
            .insert_resource(InputFocus::from_entity(children[0]));

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();

        assert_eq!(
            *app.world().get::<Interaction>(children[0]).unwrap(),
            Interaction::Pressed
        );
        assert_eq!(
            *app.world().get::<Interaction>(children[1]).unwrap(),
            Interaction::None,
            "only the focused control is pressed"
        );
    }

    #[test]
    fn space_activates_the_focused_button() {
        let mut app = test_app();
        let (_, children) = spawn_group(&mut app, 1);
        app.world_mut()
            .insert_resource(InputFocus::from_entity(children[0]));

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        app.update();

        assert_eq!(
            *app.world().get::<Interaction>(children[0]).unwrap(),
            Interaction::Pressed
        );
    }

    #[test]
    fn gamepad_dpad_moves_focus_and_south_activates() {
        let mut app = test_app();
        let (_, children) = spawn_group(&mut app, 2);
        app.world_mut()
            .insert_resource(InputFocus::from_entity(children[0]));
        let gamepad = app.world_mut().spawn(Gamepad::default()).id();

        app.world_mut()
            .get_mut::<Gamepad>(gamepad)
            .unwrap()
            .digital_mut()
            .press(GamepadButton::DPadRight);
        app.update();
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(children[1])
        );

        {
            let mut gp = app.world_mut().get_mut::<Gamepad>(gamepad).unwrap();
            gp.digital_mut().release(GamepadButton::DPadRight);
            gp.digital_mut().clear();
            gp.digital_mut().press(GamepadButton::South);
        }
        app.update();
        assert_eq!(
            *app.world().get::<Interaction>(children[1]).unwrap(),
            Interaction::Pressed
        );
    }

    /// Spawns a *modal* `TabGroup` root with `count` `Focusable`+`TabIndex(0)`
    /// children, mirroring [`spawn_group`] but for the settings/pause overlay
    /// shape (#216).
    fn spawn_modal_group(app: &mut App, count: usize) -> (Entity, Vec<Entity>) {
        let root = app.world_mut().spawn(TabGroup::modal()).id();
        let mut children = Vec::new();
        for _ in 0..count {
            let child = app
                .world_mut()
                .spawn((Button, Focusable, TabIndex(0), ChildOf(root)))
                .id();
            children.push(child);
        }
        (root, children)
    }

    #[test]
    fn autofocus_first_in_group_focuses_the_modal_overlays_first_control() {
        use bevy::ecs::system::RunSystemOnce;

        let mut app = test_app();
        let (root, children) = spawn_modal_group(&mut app, 3);

        app.world_mut()
            .run_system_once(move |nav: TabNavigation, mut focus: ResMut<InputFocus>| {
                autofocus_first_in_group(&nav, &mut focus, root);
            })
            .unwrap();

        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(children[0]),
            "opening a modal overlay (settings, pause) must focus its first control \
             immediately -- otherwise the first Tab press still targets whatever \
             screen sits behind the scrim"
        );
    }

    #[test]
    fn a_modal_group_confines_tab_navigation_to_its_own_children_once_focus_is_inside() {
        // A non-modal screen group coexists behind the modal overlay, the
        // same shape as settings opened over the main menu: both `TabGroup`s
        // are alive in the world at once (the menu is an overlay target, not
        // despawned), but once focus is inside the modal, Tab must never
        // reach the screen behind it.
        let mut app = test_app();
        let (_, screen_children) = spawn_group(&mut app, 2);
        let (_, modal_children) = spawn_modal_group(&mut app, 2);
        app.world_mut()
            .insert_resource(InputFocus::from_entity(modal_children[0]));

        press_and_settle(&mut app, KeyCode::Tab);
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(modal_children[1]),
            "Tab must move within the modal overlay"
        );

        press_and_settle(&mut app, KeyCode::Tab);
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(modal_children[0]),
            "Tab must wrap back inside the modal overlay, never reaching \
             the screen's own group ({screen_children:?}) behind it"
        );
    }

    /// #216: at 200% desktop zoom several screens rely on their designed
    /// in-UI scrolling (`crate::ui_widgets::Scrollable`, #31) to keep every
    /// control reachable -- but wheel/touch-drag are pointer inputs, so a
    /// keyboard-only player tabbing to an off-screen control could never
    /// bring it into view without this: focusing a control must scroll its
    /// scrollable ancestor just far enough that the control is visible.
    #[test]
    fn focusing_an_offscreen_control_scrolls_its_scrollable_ancestor_into_view() {
        let mut app = test_app();
        // A 400px-tall scroll container at y 0..400 (center y=200).
        let container = app
            .world_mut()
            .spawn((
                TabGroup::new(0),
                ScrollPosition::default(),
                ComputedNode {
                    size: Vec2::new(600.0, 400.0),
                    inverse_scale_factor: 1.0,
                    ..Default::default()
                },
                UiGlobalTransform::from(Affine2::from_translation(Vec2::new(300.0, 200.0))),
            ))
            .id();
        // A 56px-tall focusable child currently rendered at y 488..544 --
        // fully below the container's visible 0..400 window.
        let below = app
            .world_mut()
            .spawn((
                Button,
                Focusable,
                TabIndex(0),
                ChildOf(container),
                ComputedNode {
                    size: Vec2::new(260.0, 56.0),
                    inverse_scale_factor: 1.0,
                    ..Default::default()
                },
                UiGlobalTransform::from(Affine2::from_translation(Vec2::new(300.0, 516.0))),
            ))
            .id();

        app.world_mut()
            .insert_resource(InputFocus::from_entity(below));
        app.update();

        let scroll = app.world().get::<ScrollPosition>(container).unwrap();
        assert_eq!(
            scroll.0.y, 144.0,
            "focusing the off-screen control must scroll it just into view \
             (544 - 400 = 144), not leave it hidden below the fold"
        );
    }

    /// The counterpart: a control already fully visible must not move the
    /// scroll position at all.
    #[test]
    fn focusing_an_already_visible_control_does_not_scroll() {
        let mut app = test_app();
        let container = app
            .world_mut()
            .spawn((
                TabGroup::new(0),
                ScrollPosition(Vec2::new(0.0, 37.0)),
                ComputedNode {
                    size: Vec2::new(600.0, 400.0),
                    inverse_scale_factor: 1.0,
                    ..Default::default()
                },
                UiGlobalTransform::from(Affine2::from_translation(Vec2::new(300.0, 200.0))),
            ))
            .id();
        let visible = app
            .world_mut()
            .spawn((
                Button,
                Focusable,
                TabIndex(0),
                ChildOf(container),
                ComputedNode {
                    size: Vec2::new(260.0, 56.0),
                    inverse_scale_factor: 1.0,
                    ..Default::default()
                },
                UiGlobalTransform::from(Affine2::from_translation(Vec2::new(300.0, 200.0))),
            ))
            .id();

        app.world_mut()
            .insert_resource(InputFocus::from_entity(visible));
        app.update();

        let scroll = app.world().get::<ScrollPosition>(container).unwrap();
        assert_eq!(
            scroll.0.y, 37.0,
            "a fully visible focused control must leave the scroll untouched"
        );
    }

    #[test]
    fn focus_marker_is_gold_on_the_focused_control_only() {
        let mut app = test_app();
        let (_, children) = spawn_group(&mut app, 2);
        app.world_mut()
            .insert_resource(InputFocus::from_entity(children[0]));
        app.update();

        assert_eq!(app.world().get::<Outline>(children[0]).unwrap().color, GOLD);
        assert_eq!(
            app.world().get::<Outline>(children[1]).unwrap().color,
            Color::NONE
        );

        app.world_mut()
            .insert_resource(InputFocus::from_entity(children[1]));
        app.update();
        assert_eq!(
            app.world().get::<Outline>(children[0]).unwrap().color,
            Color::NONE
        );
        assert_eq!(app.world().get::<Outline>(children[1]).unwrap().color, GOLD);
    }

    #[test]
    fn redirect_focus_if_inside_moves_to_the_fallback() {
        let mut world = World::new();
        let despawning = world.spawn_empty().id();
        let fallback = world.spawn_empty().id();
        let mut focus = InputFocus::from_entity(despawning);

        redirect_focus_if_inside(&mut focus, [despawning], Some(fallback));
        assert_eq!(focus.get(), Some(fallback));
    }

    #[test]
    fn redirect_focus_if_inside_clears_when_no_fallback_is_given() {
        let mut world = World::new();
        let despawning = world.spawn_empty().id();
        let mut focus = InputFocus::from_entity(despawning);

        redirect_focus_if_inside(&mut focus, [despawning], None);
        assert_eq!(focus.get(), None);
    }

    #[test]
    fn redirect_focus_if_inside_leaves_unrelated_focus_alone() {
        let mut world = World::new();
        let focused = world.spawn_empty().id();
        let unrelated = world.spawn_empty().id();
        let fallback = world.spawn_empty().id();
        let mut focus = InputFocus::from_entity(focused);

        redirect_focus_if_inside(&mut focus, [unrelated], Some(fallback));
        assert_eq!(focus.get(), Some(focused));
    }
}
