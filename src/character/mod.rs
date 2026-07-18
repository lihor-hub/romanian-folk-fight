//! Character model: folk-flavored attributes, resource pools, and fighter
//! markers shared by combat, character creation, shop, and progression.

pub mod catalog;
pub mod definition;
pub mod generation;
pub mod stats;

pub use catalog::{
    AttachmentMetadata, BodyRegion, CHARACTER_CATALOG_VERSION, CatalogError, CharacterCatalog,
    MaterialMetadata, PaletteRegion, PartRecord, ResolvedCharacter, bundled_human_catalog,
    load_human_catalog,
};
pub use definition::{
    CHARACTER_DEFINITION_VERSION, CharacterDefinition, CulturalProfile, PartId, PartIdError,
    PartSelections, SkeletonFamily,
};
pub use generation::{
    GenerationError, GenerationProfile, GenerationSlot, WeightedPart, fallback_human,
    generate_character,
};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::items::Equipment;

/// Registers the character model and its single validated catalog resource.
pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        match bundled_human_catalog() {
            Ok(catalog) => {
                if let Err(error) = catalog.validate() {
                    error!("bundled character catalog is invalid: {error}");
                } else {
                    app.insert_resource(catalog.clone());
                }
            }
            Err(error) => {
                error!("bundled character catalog is invalid: {error}");
            }
        }
    }
}

/// The eight folk attributes of a fighter (#128): strength, agility,
/// vitality, luck, attack, defense, charisma, and magic.
///
/// New characters start at each attribute's base value (see
/// [`AttributeKind::base_value`] and [`Default`]): 1 everywhere except
/// `magie`, which starts at 0 — a hero with `magie == 0` is a valid
/// non-caster ([`stats::max_mana`] is 0 and no spell is ever granted), never
/// normalized upward.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attributes {
    pub putere: u32,
    pub agilitate: u32,
    pub vitalitate: u32,
    pub noroc: u32,
    pub atac: u32,
    pub aparare: u32,
    pub carisma: u32,
    pub magie: u32,
}

impl Default for Attributes {
    fn default() -> Self {
        Self {
            putere: AttributeKind::Putere.base_value(),
            agilitate: AttributeKind::Agilitate.base_value(),
            vitalitate: AttributeKind::Vitalitate.base_value(),
            noroc: AttributeKind::Noroc.base_value(),
            atac: AttributeKind::Atac.base_value(),
            aparare: AttributeKind::Aparare.base_value(),
            carisma: AttributeKind::Carisma.base_value(),
            magie: AttributeKind::Magie.base_value(),
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
            AttributeKind::Atac => self.atac,
            AttributeKind::Aparare => self.aparare,
            AttributeKind::Carisma => self.carisma,
            AttributeKind::Magie => self.magie,
        }
    }

    /// Mutable access to one attribute, addressed by [`AttributeKind`].
    pub fn get_mut(&mut self, kind: AttributeKind) -> &mut u32 {
        match kind {
            AttributeKind::Putere => &mut self.putere,
            AttributeKind::Agilitate => &mut self.agilitate,
            AttributeKind::Vitalitate => &mut self.vitalitate,
            AttributeKind::Noroc => &mut self.noroc,
            AttributeKind::Atac => &mut self.atac,
            AttributeKind::Aparare => &mut self.aparare,
            AttributeKind::Carisma => &mut self.carisma,
            AttributeKind::Magie => &mut self.magie,
        }
    }

    /// Total attribute points of the spread, over all eight kinds — the
    /// single sum the creation point-buy, the roster budget tests, and the
    /// lap scaling all build on.
    pub fn total(&self) -> u32 {
        AttributeKind::ALL.iter().map(|kind| self.get(*kind)).sum()
    }
}

/// One of the eight allocatable attributes; lets UI screens address rows and
/// buttons generically instead of per-attribute systems.
///
/// Every kind has a declared derived hook (see [`stats`]): `putere` →
/// [`stats::base_damage`], `agilitate` → initiative
/// (`combat::engine::player_acts_first`) plus the reserved
/// continuous-position contract (#134), `vitalitate` → [`stats::max_hp`] /
/// [`stats::max_stamina`], `noroc` → [`stats::crit_percent`], `atac` /
/// `aparare` → [`stats::hit_percent`], `carisma` →
/// [`stats::taunt_percent`], `magie` → [`stats::max_mana`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeKind {
    Putere,
    Agilitate,
    Vitalitate,
    Noroc,
    Atac,
    Aparare,
    Carisma,
    Magie,
}

impl AttributeKind {
    /// All eight kinds, in display order: the original four, then the #128
    /// additions with `atac`/`apărare` adjacent (they oppose each other in
    /// [`stats::hit_percent`]).
    pub const ALL: [AttributeKind; 8] = [
        AttributeKind::Putere,
        AttributeKind::Agilitate,
        AttributeKind::Vitalitate,
        AttributeKind::Noroc,
        AttributeKind::Atac,
        AttributeKind::Aparare,
        AttributeKind::Carisma,
        AttributeKind::Magie,
    ];

    /// Romanian display label for the attribute row, diacritics included.
    pub fn label(self) -> &'static str {
        match self {
            AttributeKind::Putere => "Putere",
            AttributeKind::Agilitate => "Agilitate",
            AttributeKind::Vitalitate => "Vitalitate",
            AttributeKind::Noroc => "Noroc",
            AttributeKind::Atac => "Atac",
            AttributeKind::Aparare => "Apărare",
            AttributeKind::Carisma => "Carismă",
            AttributeKind::Magie => "Magie",
        }
    }

    /// The value every fresh fighter starts this attribute at, and the floor
    /// the creation point-buy can never drop below. 1 for every kind except
    /// [`AttributeKind::Magie`], whose base is 0 so a non-caster
    /// (`magie == 0`, zero mana, no starting spell) is buildable and never
    /// normalized upward.
    pub const fn base_value(self) -> u32 {
        match self {
            AttributeKind::Magie => 0,
            _ => 1,
        }
    }

    /// Sum of every kind's [`Self::base_value`] — the attribute total a
    /// fresh, unallocated fighter carries (7 today: seven 1s plus `magie` 0).
    pub fn base_total() -> u32 {
        Self::ALL.iter().map(|kind| kind.base_value()).sum()
    }
}

/// Stable, save-friendly appearance of the player hero.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlayerAppearance {
    pub skin_tone: SkinTone,
    pub build: BodyBuild,
    pub hair: HairStyle,
    pub accent: AccentColor,
}

impl Default for PlayerAppearance {
    fn default() -> Self {
        Self {
            skin_tone: SkinTone::Warm,
            build: BodyBuild::Balanced,
            hair: HairStyle::Braided,
            accent: AccentColor::Crimson,
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
    fn character_plugin_registers_the_validated_shared_catalog_resource() {
        let mut app = App::new();
        app.add_plugins(CharacterPlugin);

        let resource = app
            .world()
            .get_resource::<CharacterCatalog>()
            .expect("the bundled validated catalog is an ECS resource");
        let bundled = bundled_human_catalog().expect("the bundled catalog validates");
        assert_eq!(resource.version(), bundled.version());
        assert_eq!(resource.known_good_human(), bundled.known_good_human());
    }

    #[test]
    fn attributes_default_to_the_per_kind_base_values() {
        let attrs = Attributes::default();
        assert_eq!(
            attrs,
            Attributes {
                putere: 1,
                agilitate: 1,
                vitalitate: 1,
                noroc: 1,
                atac: 1,
                aparare: 1,
                carisma: 1,
                magie: 0,
            },
            "magie starts at 0 (a valid non-caster); everything else at 1"
        );
        for kind in AttributeKind::ALL {
            assert_eq!(attrs.get(kind), kind.base_value(), "{kind:?}");
        }
    }

    #[test]
    fn base_total_sums_the_eight_base_values() {
        assert_eq!(AttributeKind::base_total(), 7, "seven 1s plus magie 0");
        assert_eq!(Attributes::default().total(), AttributeKind::base_total());
    }

    #[test]
    fn get_and_get_mut_address_every_kind() {
        let mut attrs = Attributes::default();
        for (index, kind) in AttributeKind::ALL.into_iter().enumerate() {
            *attrs.get_mut(kind) = index as u32 + 10;
        }
        for (index, kind) in AttributeKind::ALL.into_iter().enumerate() {
            assert_eq!(attrs.get(kind), index as u32 + 10, "{kind:?}");
        }
        assert_eq!(attrs.total(), (10..18).sum::<u32>());
    }

    #[test]
    fn labels_keep_the_romanian_diacritics() {
        assert_eq!(AttributeKind::Aparare.label(), "Apărare");
        assert_eq!(AttributeKind::Carisma.label(), "Carismă");
        assert_eq!(AttributeKind::Atac.label(), "Atac");
        assert_eq!(AttributeKind::Magie.label(), "Magie");
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
