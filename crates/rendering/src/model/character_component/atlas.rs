//! Deterministic base-character atlas planning from resolved DBC appearance rows.

use std::array;

use solarity_asset::{AssetPath, AssetStore, CharacterModelAppearance, CharacterSection};
use solarity_ecs::PlayerEquipmentSlot;

use super::types::STOCK_CHARACTER_ATLAS_SIZE;
use super::{
    CharacterAtlasLayer, CharacterAtlasLayerKind, CharacterAtlasRegion, CharacterEquipmentItem,
    CharacterTexturePlanError,
};

const SECTION_FLAG_NPC_SKIN: u32 = 0x08;
const EQUIPMENT_PRIORITY_COUNT: usize = 7;
const BODY_REGIONS: [CharacterAtlasRegion; 8] = [
    CharacterAtlasRegion::ArmUpper,
    CharacterAtlasRegion::ArmLower,
    CharacterAtlasRegion::Hand,
    CharacterAtlasRegion::TorsoUpper,
    CharacterAtlasRegion::TorsoLower,
    CharacterAtlasRegion::LegUpper,
    CharacterAtlasRegion::LegLower,
    CharacterAtlasRegion::Foot,
];
const COMPONENT_FOLDERS: [&str; 8] = [
    "ArmUpperTexture",
    "ArmLowerTexture",
    "HandTexture",
    "TorsoUpperTexture",
    "TorsoLowerTexture",
    "LegUpperTexture",
    "LegLowerTexture",
    "FootTexture",
];

// Recovered build-12340 CCharacterComponent table. Rows are the internal
// head, shoulder, shirt, chest, waist, legs, feet, wrist, hands, and tabard
// slots; columns are the eight body component regions above.
const ITEM_PRIORITIES: [[i8; 8]; 10] = [
    [-1, -1, -1, -1, -1, -1, -1, -1],
    [-1, -1, -1, -1, -1, -1, -1, -1],
    [0, 0, -1, 0, 0, -1, -1, -1],
    [1, 1, -1, 1, 1, 1, 1, -1],
    [-1, -1, -1, -1, 5, 2, -1, -1],
    [-1, -1, -1, -1, -1, 0, 0, -1],
    [-1, -1, -1, -1, -1, -1, 2, 0],
    [-1, 2, -1, -1, -1, -1, -1, -1],
    [-1, 3, 0, -1, -1, -1, -1, -1],
    [-1, -1, -1, 4, 4, -1, -1, -1],
];

/// Item texture paths indexed by body region and paste priority.
type EquipmentLayers = [[Option<AssetPath>; EQUIPMENT_PRIORITY_COUNT]; 8];

/// Texture inputs for one stock character M2 before equipped-item composition.
///
/// `atlas_layers` produce the dynamic M2 body texture (texture type 1). Hair
/// and extra skin independently replace M2 texture types 6 and 8.
pub struct CharacterTexturePlan {
    atlas_layers: Vec<CharacterAtlasLayer>,
    hair: Option<AssetPath>,
    extra_skin: Option<AssetPath>,
}

impl CharacterTexturePlan {
    /// Builds stock base layers in component-section render-preparation order.
    ///
    /// This entry point intentionally describes an unequipped character. Item
    /// texture priority and coverage are a later input to this same atlas, not
    /// a guessed visibility fallback at the base-appearance boundary.
    ///
    /// # Errors
    ///
    /// Returns [`CharacterTexturePlanError`] when the required base skin name
    /// is empty or a nonempty DBC texture name is not a valid archive path.
    pub fn base(
        appearance: &CharacterModelAppearance<'_>,
    ) -> Result<Self, CharacterTexturePlanError> {
        Self::build(appearance, None)
    }

    /// Builds base appearance and equipped body textures in stock priority order.
    ///
    /// The archive probe first selects each universal `_U` component. Only
    /// when that exact path is absent does stock substitute `_M` or `_F`.
    /// Higher-resolution packs require no separate path or item representation.
    ///
    /// # Errors
    ///
    /// Returns [`CharacterTexturePlanError`] for invalid paths, archive lookup
    /// failures, missing required base skin, or a non-player gender needed for
    /// a sex-specific item texture.
    pub fn equipped<'catalog, I>(
        appearance: &CharacterModelAppearance<'_>,
        store: &AssetStore,
        equipment: I,
    ) -> Result<Self, CharacterTexturePlanError>
    where
        I: IntoIterator<Item = CharacterEquipmentItem<'catalog>>,
    {
        let equipment = plan_equipment_layers(appearance.gender_id(), store, equipment)?;
        Self::build(appearance, Some(&equipment))
    }

    /// Builds the shared region sequence with optional item-priority cells.
    fn build(
        appearance: &CharacterModelAppearance<'_>,
        equipment: Option<&EquipmentLayers>,
    ) -> Result<Self, CharacterTexturePlanError> {
        let skin_names = appearance.skin().texture_names();
        let skin = required_path(CharacterAtlasLayerKind::Skin, 0, skin_names[0])?;
        let mut atlas_layers = Vec::with_capacity(32);

        // Stock prepares sections in this exact physical order. Each region's
        // overlays follow its opaque skin paste, preserving alpha blend order.
        for (section, region) in BODY_REGIONS.into_iter().enumerate() {
            push_layer(
                &mut atlas_layers,
                CharacterAtlasLayerKind::Skin,
                region,
                &skin,
            );
            if let Some(underwear) = appearance.underwear() {
                let underwear_slot = match region {
                    CharacterAtlasRegion::TorsoUpper
                        if !has_item_priority(equipment, section, 0..3) =>
                    {
                        Some(1)
                    }
                    CharacterAtlasRegion::LegUpper
                        if !has_item_priority(equipment, section, 0..2) =>
                    {
                        Some(0)
                    }
                    _ => None,
                };
                if let Some(slot) = underwear_slot {
                    push_optional_layer(
                        &mut atlas_layers,
                        CharacterAtlasLayerKind::Underwear,
                        region,
                        underwear,
                        slot,
                    )?;
                }
            }
            if let Some(equipment) = equipment {
                for path in equipment[section].iter().flatten() {
                    push_layer(
                        &mut atlas_layers,
                        CharacterAtlasLayerKind::Item,
                        region,
                        path,
                    );
                }
            }
        }

        push_head_layers(
            &mut atlas_layers,
            CharacterAtlasRegion::HeadUpper,
            1,
            2,
            appearance,
            &skin,
        )?;
        push_head_layers(
            &mut atlas_layers,
            CharacterAtlasRegion::HeadLower,
            0,
            1,
            appearance,
            &skin,
        )?;

        Ok(Self {
            atlas_layers,
            hair: optional_path(appearance.hair(), 0)?,
            extra_skin: optional_path(appearance.skin(), 1)?,
        })
    }

    /// Returns the stock-default dynamic body atlas width and height.
    #[must_use]
    pub const fn atlas_size(&self) -> u32 {
        STOCK_CHARACTER_ATLAS_SIZE
    }

    /// Returns archive pastes in stock component-section order.
    #[must_use]
    pub fn atlas_layers(&self) -> &[CharacterAtlasLayer] {
        &self.atlas_layers
    }

    /// Returns the texture replacing M2 hair texture type 6 when authored.
    #[must_use]
    pub const fn hair(&self) -> Option<&AssetPath> {
        self.hair.as_ref()
    }

    /// Returns the texture replacing M2 extra-skin type 8 when authored.
    #[must_use]
    pub const fn extra_skin(&self) -> Option<&AssetPath> {
        self.extra_skin.as_ref()
    }
}

/// Plans all nonempty item component stems into their final priority cells.
fn plan_equipment_layers<'catalog, I>(
    gender_id: u32,
    store: &AssetStore,
    equipment: I,
) -> Result<EquipmentLayers, CharacterTexturePlanError>
where
    I: IntoIterator<Item = CharacterEquipmentItem<'catalog>>,
{
    let mut layers = array::from_fn(|_| array::from_fn(|_| None));
    for item in equipment {
        let Some(slot) = internal_item_slot(item.slot()) else {
            continue;
        };
        let display = item.display();
        let stems = display.component_textures();
        let geosets = display.geoset_groups();
        for (section, stem) in stems.into_iter().enumerate() {
            let base_priority = ITEM_PRIORITIES[slot][section];
            if stem.is_empty() || base_priority < 0 {
                continue;
            }
            let priority = adjusted_item_priority(slot, section, base_priority as usize, geosets);
            layers[section][priority] = Some(item_texture_path(
                store,
                gender_id,
                COMPONENT_FOLDERS[section],
                stem,
            )?);
        }
    }
    Ok(layers)
}

/// Maps public player slots to CCharacterComponent's texture-bearing rows.
const fn internal_item_slot(slot: PlayerEquipmentSlot) -> Option<usize> {
    match slot {
        PlayerEquipmentSlot::Head => Some(0),
        PlayerEquipmentSlot::Shoulders => Some(1),
        PlayerEquipmentSlot::Shirt => Some(2),
        PlayerEquipmentSlot::Chest => Some(3),
        PlayerEquipmentSlot::Waist => Some(4),
        PlayerEquipmentSlot::Legs => Some(5),
        PlayerEquipmentSlot::Feet => Some(6),
        PlayerEquipmentSlot::Wrists => Some(7),
        PlayerEquipmentSlot::Hands => Some(8),
        PlayerEquipmentSlot::Tabard => Some(9),
        PlayerEquipmentSlot::Neck
        | PlayerEquipmentSlot::FingerOne
        | PlayerEquipmentSlot::FingerTwo
        | PlayerEquipmentSlot::TrinketOne
        | PlayerEquipmentSlot::TrinketTwo
        | PlayerEquipmentSlot::Back
        | PlayerEquipmentSlot::MainHand
        | PlayerEquipmentSlot::OffHand
        | PlayerEquipmentSlot::Ranged => None,
    }
}

/// Applies stock's geoset-dependent priority changes for sleeves and boots.
const fn adjusted_item_priority(
    slot: usize,
    section: usize,
    base_priority: usize,
    geosets: [u32; 3],
) -> usize {
    match (slot, section) {
        (3, 1) if geosets[0] != 0 => 5,
        (8, 1) if geosets[0] != 0 => 6,
        (3, 6) if geosets[2] != 0 => 4,
        (6, 6) if geosets[0] != 0 => 3,
        _ => base_priority,
    }
}

/// Selects universal equipment art before stock's sex-specific substitution.
fn item_texture_path(
    store: &AssetStore,
    gender_id: u32,
    folder: &str,
    stem: &str,
) -> Result<AssetPath, CharacterTexturePlanError> {
    let universal = AssetPath::new(format!("Item\\TextureComponents\\{folder}\\{stem}_U.blp"))?;
    if store.contains(&universal)? {
        return Ok(universal);
    }
    let suffix = match gender_id {
        0 => 'M',
        1 => 'F',
        _ => {
            return Err(CharacterTexturePlanError::UnsupportedEquipmentGender { gender_id });
        }
    };
    Ok(AssetPath::new(format!(
        "Item\\TextureComponents\\{folder}\\{stem}_{suffix}.blp"
    ))?)
}

/// Tests whether any item occupies a stock underwear-coverage priority.
fn has_item_priority(
    equipment: Option<&EquipmentLayers>,
    section: usize,
    priorities: std::ops::Range<usize>,
) -> bool {
    equipment.is_some_and(|layers| layers[section][priorities].iter().any(Option::is_some))
}

/// Appends the stock skin/face/facial-hair/hair sequence for one head region.
fn push_head_layers(
    layers: &mut Vec<CharacterAtlasLayer>,
    region: CharacterAtlasRegion,
    face_slot: usize,
    hair_slot: usize,
    appearance: &CharacterModelAppearance<'_>,
    skin: &AssetPath,
) -> Result<(), CharacterTexturePlanError> {
    if appearance.skin().flags() & SECTION_FLAG_NPC_SKIN != 0 {
        push_layer(layers, CharacterAtlasLayerKind::Skin, region, skin);
    }
    if let Some(face) = appearance.face() {
        push_optional_layer(
            layers,
            CharacterAtlasLayerKind::Face,
            region,
            face,
            face_slot,
        )?;
    }
    push_optional_layer(
        layers,
        CharacterAtlasLayerKind::FacialHair,
        region,
        appearance.facial_hair(),
        face_slot,
    )?;
    push_optional_layer(
        layers,
        CharacterAtlasLayerKind::Hair,
        region,
        appearance.hair(),
        hair_slot,
    )?;
    Ok(())
}

/// Appends an already validated path without re-normalizing it per region.
fn push_layer(
    layers: &mut Vec<CharacterAtlasLayer>,
    kind: CharacterAtlasLayerKind,
    region: CharacterAtlasRegion,
    path: &AssetPath,
) {
    layers.push(CharacterAtlasLayer::new(kind, region, path.clone()));
}

/// Appends a nonempty optional texture slot used by stock for this region.
fn push_optional_layer(
    layers: &mut Vec<CharacterAtlasLayer>,
    kind: CharacterAtlasLayerKind,
    region: CharacterAtlasRegion,
    section: &CharacterSection,
    slot: usize,
) -> Result<(), CharacterTexturePlanError> {
    if let Some(path) = optional_path(section, slot)? {
        layers.push(CharacterAtlasLayer::new(kind, region, path));
    }
    Ok(())
}

/// Validates a required DBC texture name at the rendering boundary.
fn required_path(
    kind: CharacterAtlasLayerKind,
    slot: usize,
    value: &str,
) -> Result<AssetPath, CharacterTexturePlanError> {
    if value.is_empty() {
        return Err(CharacterTexturePlanError::MissingTexture { kind, slot });
    }
    Ok(AssetPath::new(value)?)
}

/// Normalizes an authored optional DBC texture name without inventing a path.
fn optional_path(
    section: &CharacterSection,
    slot: usize,
) -> Result<Option<AssetPath>, CharacterTexturePlanError> {
    let value = section.texture_names()[slot];
    if value.is_empty() {
        return Ok(None);
    }
    Ok(Some(AssetPath::new(value)?))
}
