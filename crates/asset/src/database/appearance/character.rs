//! Player customization resolution and stock M2 geoset selection.

use std::fmt;

use crate::database::character::{
    CharacterAppearanceCatalog, CharacterFacialHairStyle, CharacterHairGeoset, CharacterSection,
};

use super::AppearanceError;

const SECTION_FLAG_NPC_SKIN: u32 = 0x08;
const SECTION_FLAG_PLAYER: u32 = 0x01;
const SECTION_FLAG_DEATH_KNIGHT: u32 = 0x04;
const DEATH_KNIGHT_CLASS_ID: u8 = 6;
const STOCK_HAIR_GEOSET_FALLBACK: u32 = 1;

/// The five `CharSections.dbc` component variation categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CharacterSectionKind {
    /// Base body skin.
    Skin,
    /// Face overlays selected by face and skin color.
    Face,
    /// Beard, markings, and other facial-feature overlays.
    FacialHair,
    /// Hair texture and head overlays.
    Hair,
    /// Base underwear layers selected by skin color.
    Underwear,
}

impl CharacterSectionKind {
    /// Returns the build-12340 `BaseSection` word.
    const fn value(self) -> u32 {
        match self {
            Self::Skin => 0,
            Self::Face => 1,
            Self::FacialHair => 2,
            Self::Hair => 3,
            Self::Underwear => 4,
        }
    }
}

impl fmt::Display for CharacterSectionKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Skin => "skin",
            Self::Face => "face",
            Self::FacialHair => "facial-hair",
            Self::Hair => "hair",
            Self::Underwear => "underwear",
        };
        formatter.write_str(name)
    }
}

/// Player appearance bytes widened for exact DBC lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterCustomization {
    skin_id: u32,
    face_id: u32,
    hair_style_id: u32,
    hair_color_id: u32,
    facial_hair_style_id: u32,
}

impl CharacterCustomization {
    /// Creates a customization request from the five stock player bytes.
    #[must_use]
    pub const fn new(
        skin_id: u8,
        face_id: u8,
        hair_style_id: u8,
        hair_color_id: u8,
        facial_hair_style_id: u8,
    ) -> Self {
        Self {
            skin_id: skin_id as u32,
            face_id: face_id as u32,
            hair_style_id: hair_style_id as u32,
            hair_color_id: hair_color_id as u32,
            facial_hair_style_id: facial_hair_style_id as u32,
        }
    }

    /// Creates a customization request from widened client-table identifiers.
    ///
    /// `CreatureDisplayInfoExtra.dbc` stores the same values as 32-bit words,
    /// so retaining them avoids narrowing an authored NPC appearance.
    #[must_use]
    pub const fn from_ids(
        skin_id: u32,
        face_id: u32,
        hair_style_id: u32,
        hair_color_id: u32,
        facial_hair_style_id: u32,
    ) -> Self {
        Self {
            skin_id,
            face_id,
            hair_style_id,
            hair_color_id,
            facial_hair_style_id,
        }
    }
}

/// M2 geoset identifiers selected from hair and facial-feature tables.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterGeosetSelection {
    hair: u32,
    facial_hair: Option<[u32; 5]>,
}

impl CharacterGeosetSelection {
    /// Returns the selected hair geoset, including stock's geoset-one fallback.
    #[must_use]
    pub const fn hair(self) -> u32 {
        self.hair
    }

    /// Returns facial geosets in stock component-slot order.
    ///
    /// The five values correspond to component slots 1, 2, 3, 16, and 17.
    #[must_use]
    pub const fn facial_hair(self) -> Option<[u32; 5]> {
        self.facial_hair
    }
}

/// Exact texture sections and geometry choices for one player appearance.
pub struct CharacterModelAppearance<'catalog> {
    race_id: u32,
    gender_id: u32,
    skin: &'catalog CharacterSection,
    face: Option<&'catalog CharacterSection>,
    facial_hair: Option<&'catalog CharacterSection>,
    hair: Option<&'catalog CharacterSection>,
    underwear: Option<&'catalog CharacterSection>,
    hair_geoset: Option<&'catalog CharacterHairGeoset>,
    facial_hair_style: Option<&'catalog CharacterFacialHairStyle>,
    geosets: CharacterGeosetSelection,
}

impl CharacterModelAppearance<'_> {
    /// Returns the race identifier used for all resolved character rows.
    #[must_use]
    pub const fn race_id(&self) -> u32 {
        self.race_id
    }

    /// Returns the gender identifier used for all resolved character rows.
    #[must_use]
    pub const fn gender_id(&self) -> u32 {
        self.gender_id
    }

    /// Returns the base skin section.
    #[must_use]
    pub const fn skin(&self) -> &CharacterSection {
        self.skin
    }

    /// Returns face overlays, absent for stock NPC-skin rows.
    #[must_use]
    pub const fn face(&self) -> Option<&CharacterSection> {
        self.face
    }

    /// Returns the optional color-dependent facial-feature texture overlays.
    #[must_use]
    pub const fn facial_hair(&self) -> Option<&CharacterSection> {
        self.facial_hair
    }

    /// Returns hair and head-overlay textures when the model authors a section.
    #[must_use]
    pub const fn hair(&self) -> Option<&CharacterSection> {
        self.hair
    }

    /// Returns underwear layers, absent for stock NPC-skin rows.
    #[must_use]
    pub const fn underwear(&self) -> Option<&CharacterSection> {
        self.underwear
    }

    /// Returns the authored hair row before stock's fallback is applied.
    #[must_use]
    pub const fn hair_geoset(&self) -> Option<&CharacterHairGeoset> {
        self.hair_geoset
    }

    /// Returns the authored facial-feature geometry row when present.
    #[must_use]
    pub const fn facial_hair_style(&self) -> Option<&CharacterFacialHairStyle> {
        self.facial_hair_style
    }

    /// Returns the final stock M2 geoset identifiers.
    #[must_use]
    pub const fn geosets(&self) -> CharacterGeosetSelection {
        self.geosets
    }
}

impl CharacterAppearanceCatalog {
    /// Resolves stock player customization keys into texture and M2 geometry choices.
    ///
    /// Skin and underwear use variation zero with skin as their color. Face uses
    /// face as the variation and skin as the color. Hair and facial hair use
    /// their respective variation with hair color. These asymmetric keys are
    /// the indices used by `CCharacterComponent` in build 12340.
    ///
    /// # Errors
    ///
    /// Returns [`AppearanceError`] when a required texture section is absent.
    pub fn resolve_player(
        &self,
        race_id: u32,
        gender_id: u32,
        customization: CharacterCustomization,
    ) -> Result<CharacterModelAppearance<'_>, AppearanceError> {
        self.resolve_player_internal(race_id, gender_id, customization, None)
    }

    /// Resolves a playable character using build-12340's class-aware section flags.
    ///
    /// Death Knights prefer player rows carrying flag `0x04` and fall back to
    /// ordinary player rows. Every other class excludes those DK replacements.
    /// This keeps duplicate NPC rows available to [`Self::resolve_player`]'s
    /// existing creature-display consumers without leaking them into players.
    pub fn resolve_player_for_class(
        &self,
        race_id: u32,
        gender_id: u32,
        class_id: u8,
        customization: CharacterCustomization,
    ) -> Result<CharacterModelAppearance<'_>, AppearanceError> {
        self.resolve_player_internal(race_id, gender_id, customization, Some(class_id))
    }

    fn resolve_player_internal(
        &self,
        race_id: u32,
        gender_id: u32,
        customization: CharacterCustomization,
        class_id: Option<u8>,
    ) -> Result<CharacterModelAppearance<'_>, AppearanceError> {
        let skin = self.require_section(
            race_id,
            gender_id,
            CharacterSectionKind::Skin,
            0,
            customization.skin_id,
            class_id,
        )?;
        let is_npc_skin = skin.flags() & SECTION_FLAG_NPC_SKIN != 0;
        let face = if is_npc_skin {
            None
        } else {
            Some(self.require_section(
                race_id,
                gender_id,
                CharacterSectionKind::Face,
                customization.face_id,
                customization.skin_id,
                class_id,
            )?)
        };
        // Stock permits geometry-only facial features. CharSections supplies
        // an overlay only for authored style/color pairs; it does not define
        // the selectable feature set from CharacterFacialHairStyles.
        let facial_hair = self.select_section(
            race_id,
            gender_id,
            CharacterSectionKind::FacialHair.value(),
            customization.facial_hair_style_id,
            customization.hair_color_id,
            class_id,
        );
        // 0x004EA150 / 0x004EA1F0 admit only existing section rows. NPC
        // models such as GoblinMale have no hair section for their baked skin;
        // absence leaves these optional bindings empty, not a failed model.
        let hair = self.select_section(
            race_id,
            gender_id,
            CharacterSectionKind::Hair.value(),
            customization.hair_style_id,
            customization.hair_color_id,
            class_id,
        );
        let underwear = if is_npc_skin {
            None
        } else {
            Some(self.require_section(
                race_id,
                gender_id,
                CharacterSectionKind::Underwear,
                0,
                customization.skin_id,
                class_id,
            )?)
        };

        let hair_geoset = self.hair_geoset(race_id, gender_id, customization.hair_style_id);
        // ComponentGetHairGeoset returns one when the row is absent or selects
        // zero. This is an explicit stock fallback, not compatibility repair.
        let hair_selection = hair_geoset.map_or(STOCK_HAIR_GEOSET_FALLBACK, |row| {
            let selected = row.geoset_id();
            if selected == 0 {
                STOCK_HAIR_GEOSET_FALLBACK
            } else {
                selected
            }
        });
        let facial_hair_style =
            self.facial_hair_style(race_id, gender_id, customization.facial_hair_style_id);
        let facial_selection = facial_hair_style.map(|row| {
            let raw = row.geosets();
            [
                100 + raw[0],
                200 + raw[2],
                300 + raw[1],
                1600 + raw[3],
                1700 + raw[4],
            ]
        });

        Ok(CharacterModelAppearance {
            race_id,
            gender_id,
            skin,
            face,
            facial_hair,
            hair,
            underwear,
            hair_geoset,
            facial_hair_style,
            geosets: CharacterGeosetSelection {
                hair: hair_selection,
                facial_hair: facial_selection,
            },
        })
    }

    /// Selects the last physical row for a key, matching stock's component-array fill.
    fn require_section(
        &self,
        race_id: u32,
        gender_id: u32,
        kind: CharacterSectionKind,
        variation_index: u32,
        color_index: u32,
        class_id: Option<u8>,
    ) -> Result<&CharacterSection, AppearanceError> {
        self.select_section(
            race_id,
            gender_id,
            kind.value(),
            variation_index,
            color_index,
            class_id,
        )
        .ok_or(AppearanceError::MissingCharacterSection {
            race_id,
            gender_id,
            kind,
            variation_index,
            color_index,
        })
    }

    fn select_section(
        &self,
        race_id: u32,
        gender_id: u32,
        base_section: u32,
        variation_index: u32,
        color_index: u32,
        class_id: Option<u8>,
    ) -> Option<&CharacterSection> {
        let sections = self.sections_for(
            race_id,
            gender_id,
            base_section,
            variation_index,
            color_index,
        );
        let Some(class_id) = class_id else {
            return sections.last();
        };
        let ordinary = || {
            sections.iter().find(|section| {
                section.flags() & SECTION_FLAG_PLAYER != 0
                    && section.flags() & SECTION_FLAG_DEATH_KNIGHT == 0
            })
        };
        if class_id == DEATH_KNIGHT_CLASS_ID {
            sections
                .iter()
                .find(|section| {
                    section.flags() & SECTION_FLAG_PLAYER != 0
                        && section.flags() & SECTION_FLAG_DEATH_KNIGHT != 0
                })
                .or_else(ordinary)
        } else {
            ordinary()
        }
    }
}
