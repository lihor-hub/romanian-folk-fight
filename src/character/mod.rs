//! Character model: folk-flavored attributes, resource pools, and fighter
//! markers shared by combat, character creation, shop, and progression.

pub mod stats;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::items::Equipment;

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

/// Stable, save-friendly appearance of the player hero.
///
/// New taxonomy fields (`costume`, `head_feature`, `hair_variant`) are marked
/// `#[serde(default)]` so v1 saves written before those fields existed still
/// deserialize cleanly, filling the missing dimensions from
/// [`PlayerAppearance::default`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlayerAppearance {
    pub skin_tone: SkinTone,
    pub build: BodyBuild,
    pub hair: HairStyle,
    pub accent: AccentColor,
    #[serde(default)]
    pub costume: CostumeStyle,
    #[serde(default)]
    pub head_feature: HeadFeature,
    #[serde(default)]
    pub hair_variant: HairVariant,
}

impl Default for PlayerAppearance {
    fn default() -> Self {
        Self {
            skin_tone: SkinTone::Warm,
            build: BodyBuild::Balanced,
            hair: HairStyle::Braided,
            accent: AccentColor::Crimson,
            costume: CostumeStyle::default(),
            head_feature: HeadFeature::default(),
            hair_variant: HairVariant::default(),
        }
    }
}

/// Selectable skin tone for the player's cutout rig.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkinTone {
    Fair,
    Warm,
    Olive,
    Deep,
}

impl SkinTone {
    pub const ALL: [Self; 4] = [Self::Fair, Self::Warm, Self::Olive, Self::Deep];

    pub fn label(self) -> &'static str {
        match self {
            Self::Fair => "Deschis",
            Self::Warm => "Miere",
            Self::Olive => "Măsliniu",
            Self::Deep => "Brun",
        }
    }
}

/// Broad body silhouette choice for the player's cutout rig.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyBuild {
    Lean,
    Balanced,
    Sturdy,
    Powerful,
}

impl BodyBuild {
    pub const ALL: [Self; 4] = [Self::Lean, Self::Balanced, Self::Sturdy, Self::Powerful];

    pub fn label(self) -> &'static str {
        match self {
            Self::Lean => "Sprinten",
            Self::Balanced => "Echilibrat",
            Self::Sturdy => "Solid",
            Self::Powerful => "Vânjos",
        }
    }
}

/// Hairstyle choice for the player's cutout rig.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HairStyle {
    Braided,
    Long,
    Short,
    Tied,
}

impl HairStyle {
    pub const ALL: [Self; 4] = [Self::Braided, Self::Long, Self::Short, Self::Tied];

    pub fn label(self) -> &'static str {
        match self {
            Self::Braided => "Împletit",
            Self::Long => "Plete",
            Self::Short => "Scurt",
            Self::Tied => "Prins",
        }
    }
}

/// Clothing accent palette for the player's cutout rig.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccentColor {
    Crimson,
    Forest,
    Gold,
    Storm,
}

impl AccentColor {
    pub const ALL: [Self; 4] = [Self::Crimson, Self::Forest, Self::Gold, Self::Storm];

    pub fn label(self) -> &'static str {
        match self {
            Self::Crimson => "Roșu",
            Self::Forest => "Verde",
            Self::Gold => "Auriu",
            Self::Storm => "Cenușiu",
        }
    }
}

/// Preset-first costume silhouette worn over the base body. First-pass Wave 2
/// taxonomy: one variant per predefined hero archetype so preset selection
/// can resolve to a distinct authored outfit once art wiring lands. Kept as a
/// stable, save-friendly enum with `snake_case` serialization.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostumeStyle {
    #[default]
    HaiducCoat,
    VoinicTunic,
    CiobanCojoc,
    SolomonarRobe,
}

impl CostumeStyle {
    pub const ALL: [Self; 4] = [
        Self::HaiducCoat,
        Self::VoinicTunic,
        Self::CiobanCojoc,
        Self::SolomonarRobe,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::HaiducCoat => "Suman de haiduc",
            Self::VoinicTunic => "Tunică de voinic",
            Self::CiobanCojoc => "Cojoc ciobănesc",
            Self::SolomonarRobe => "Robă solomonară",
        }
    }
}

/// Facial-hair choice layered on the head. Adds silhouette-level identity to
/// preset heroes without doubling the hair/costume axes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadFeature {
    #[default]
    Clean,
    Moustache,
    Beard,
}

impl HeadFeature {
    pub const ALL: [Self; 3] = [Self::Clean, Self::Moustache, Self::Beard];

    pub fn label(self) -> &'static str {
        match self {
            Self::Clean => "Bărbierit",
            Self::Moustache => "Mustață",
            Self::Beard => "Barbă",
        }
    }
}

/// Per-style silhouette variant, so multiple "Long" or "Braided" cuts can
/// coexist without doubling [`HairStyle`] cardinality. Individual asset
/// bundles are wired up in a follow-up task.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HairVariant {
    #[default]
    Primary,
    Alternate,
    Ornate,
}

impl HairVariant {
    pub const ALL: [Self; 3] = [Self::Primary, Self::Alternate, Self::Ornate];

    pub fn label(self) -> &'static str {
        match self {
            Self::Primary => "Standard",
            Self::Alternate => "Alternativă",
            Self::Ornate => "Ornat",
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
/// at full health and stamina derived from the attributes, and with empty
/// [`Equipment`] (gear arrives via the shop or enemy loadouts).
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
            Equipment::default(),
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
        assert_eq!(
            *fighter.get::<Equipment>().unwrap(),
            Equipment::default(),
            "fighters spawn unequipped"
        );
    }
}
