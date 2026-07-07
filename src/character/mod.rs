//! Character model: folk-flavored attributes, resource pools, and fighter
//! markers shared by combat, character creation, shop, and progression.

pub mod stats;

use bevy::prelude::*;

/// Registers the character model. Components are plain data for now; systems
/// arrive with the combat and progression issues.
pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, _app: &mut App) {}
}

/// The four folk attributes of a fighter: strength, agility, vitality, luck.
///
/// New characters start at 1 in each attribute (see [`Default`]).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attributes {
    pub putere: u32,
    pub agilitate: u32,
    pub vitalitate: u32,
    pub noroc: u32,
}

impl Default for Attributes {
    fn default() -> Self {
        Self {
            putere: 1,
            agilitate: 1,
            vitalitate: 1,
            noroc: 1,
        }
    }
}

impl Attributes {
    /// Current value of one attribute, addressed by [`AttributeKind`].
    pub fn get(&self, kind: AttributeKind) -> u32 {
        match kind {
            AttributeKind::Putere => self.putere,
            AttributeKind::Agilitate => self.agilitate,
            AttributeKind::Vitalitate => self.vitalitate,
            AttributeKind::Noroc => self.noroc,
        }
    }

    /// Mutable access to one attribute, addressed by [`AttributeKind`].
    pub fn get_mut(&mut self, kind: AttributeKind) -> &mut u32 {
        match kind {
            AttributeKind::Putere => &mut self.putere,
            AttributeKind::Agilitate => &mut self.agilitate,
            AttributeKind::Vitalitate => &mut self.vitalitate,
            AttributeKind::Noroc => &mut self.noroc,
        }
    }
}

/// One of the four allocatable attributes; lets UI screens address rows and
/// buttons generically instead of per-attribute systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeKind {
    Putere,
    Agilitate,
    Vitalitate,
    Noroc,
}

impl AttributeKind {
    /// All four kinds, in display order.
    pub const ALL: [AttributeKind; 4] = [
        AttributeKind::Putere,
        AttributeKind::Agilitate,
        AttributeKind::Vitalitate,
        AttributeKind::Noroc,
    ];

    /// Romanian display label for the attribute row.
    pub fn label(self) -> &'static str {
        match self {
            AttributeKind::Putere => "Putere",
            AttributeKind::Agilitate => "Agilitate",
            AttributeKind::Vitalitate => "Vitalitate",
            AttributeKind::Noroc => "Noroc",
        }
    }
}

/// Hit points of a fighter.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Health {
    pub current: i32,
    pub max: i32,
}

/// Stamina pool of a fighter.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamina {
    pub current: i32,
    pub max: i32,
}

/// Marker for every fighter entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fighter;

/// Marker for the player-controlled fighter.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerFighter;

/// Marker for an enemy fighter.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnemyFighter;

/// Display name of a fighter.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct FighterName(pub String);

/// Spawns a fighter entity with the given name, attributes, and side marker
/// bundle (e.g. [`PlayerFighter`], or [`EnemyFighter`] plus its AI profile),
/// at full health and stamina derived from the attributes.
pub fn spawn_fighter(
    commands: &mut Commands,
    name: impl Into<String>,
    attrs: Attributes,
    marker: impl Bundle,
) -> Entity {
    let max_hp = stats::max_hp(&attrs);
    let max_stamina = stats::max_stamina(&attrs);
    commands
        .spawn((
            Fighter,
            FighterName(name.into()),
            attrs,
            Health {
                current: max_hp,
                max: max_hp,
            },
            Stamina {
                current: max_stamina,
                max: max_stamina,
            },
            marker,
        ))
        .id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_default_to_one_each() {
        let attrs = Attributes::default();
        assert_eq!(
            attrs,
            Attributes {
                putere: 1,
                agilitate: 1,
                vitalitate: 1,
                noroc: 1,
            }
        );
    }

    #[test]
    fn spawn_fighter_bundles_full_pools_and_marker() {
        let mut world = World::new();
        let attrs = Attributes {
            vitalitate: 5,
            ..Attributes::default()
        };
        let entity = {
            let mut commands = world.commands();
            spawn_fighter(&mut commands, "Ion", attrs, PlayerFighter)
        };
        world.flush();

        let fighter = world.entity(entity);
        assert!(fighter.contains::<Fighter>());
        assert!(fighter.contains::<PlayerFighter>());
        assert_eq!(fighter.get::<FighterName>().unwrap().0, "Ion");
        assert_eq!(*fighter.get::<Attributes>().unwrap(), attrs);
        assert_eq!(
            *fighter.get::<Health>().unwrap(),
            Health {
                current: 100,
                max: 100,
            }
        );
        assert_eq!(
            *fighter.get::<Stamina>().unwrap(),
            Stamina {
                current: 55,
                max: 55,
            }
        );
    }
}
