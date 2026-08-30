//! Deterministic base-character atlas planning from resolved DBC appearance rows.

use solarity_asset::{AssetPath, CharacterModelAppearance, CharacterSection};

use super::types::STOCK_CHARACTER_ATLAS_SIZE;
use super::{
    CharacterAtlasLayer, CharacterAtlasLayerKind, CharacterAtlasRegion, CharacterTexturePlanError,
};

const SECTION_FLAG_NPC_SKIN: u32 = 0x08;

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
        let skin_names = appearance.skin().texture_names();
        let skin = required_path(CharacterAtlasLayerKind::Skin, 0, skin_names[0])?;
        let mut atlas_layers = Vec::with_capacity(18);

        // Stock prepares sections in this exact physical order. Each region's
        // overlays follow its opaque skin paste, preserving alpha blend order.
        push_layer(
            &mut atlas_layers,
            CharacterAtlasLayerKind::Skin,
            CharacterAtlasRegion::ArmUpper,
            &skin,
        );
        push_layer(
            &mut atlas_layers,
            CharacterAtlasLayerKind::Skin,
            CharacterAtlasRegion::ArmLower,
            &skin,
        );
        push_layer(
            &mut atlas_layers,
            CharacterAtlasLayerKind::Skin,
            CharacterAtlasRegion::Hand,
            &skin,
        );
        push_layer(
            &mut atlas_layers,
            CharacterAtlasLayerKind::Skin,
            CharacterAtlasRegion::TorsoUpper,
            &skin,
        );
        if let Some(underwear) = appearance.underwear() {
            push_optional_layer(
                &mut atlas_layers,
                CharacterAtlasLayerKind::Underwear,
                CharacterAtlasRegion::TorsoUpper,
                underwear,
                1,
            )?;
        }
        push_layer(
            &mut atlas_layers,
            CharacterAtlasLayerKind::Skin,
            CharacterAtlasRegion::TorsoLower,
            &skin,
        );
        push_layer(
            &mut atlas_layers,
            CharacterAtlasLayerKind::Skin,
            CharacterAtlasRegion::LegUpper,
            &skin,
        );
        if let Some(underwear) = appearance.underwear() {
            push_optional_layer(
                &mut atlas_layers,
                CharacterAtlasLayerKind::Underwear,
                CharacterAtlasRegion::LegUpper,
                underwear,
                0,
            )?;
        }
        push_layer(
            &mut atlas_layers,
            CharacterAtlasLayerKind::Skin,
            CharacterAtlasRegion::LegLower,
            &skin,
        );
        push_layer(
            &mut atlas_layers,
            CharacterAtlasLayerKind::Skin,
            CharacterAtlasRegion::Foot,
            &skin,
        );

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
