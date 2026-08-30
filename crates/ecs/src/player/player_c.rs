//! Stock implementation responsibility recovered from `Player_C.cpp` and `Player_C.h`.

use shipyard::Component;

/// Player name and stable character identity state.
#[derive(Clone, Debug, Eq, PartialEq, Component)]
pub struct PlayerIdentity {
    name: String,
}

impl PlayerIdentity {
    pub(crate) fn new(name: String) -> Self {
        Self { name }
    }

    /// Returns the server-provided character display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Marker identifying the one player controlled by this client.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Component)]
pub struct LocalPlayer;

/// Player customization bytes consumed by model and portrait presentation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Component)]
pub struct PlayerAppearance {
    skin_id: u8,
    face_id: u8,
    hair_style_id: u8,
    hair_color_id: u8,
    facial_hair_style_id: u8,
}

impl PlayerAppearance {
    /// Creates the typed view of the stock player customization bytes.
    #[must_use]
    pub const fn new(
        skin_id: u8,
        face_id: u8,
        hair_style_id: u8,
        hair_color_id: u8,
        facial_hair_style_id: u8,
    ) -> Self {
        Self {
            skin_id,
            face_id,
            hair_style_id,
            hair_color_id,
            facial_hair_style_id,
        }
    }

    /// Returns the CharSections.dbc skin variant.
    #[must_use]
    pub const fn skin_id(self) -> u8 {
        self.skin_id
    }

    /// Returns the CharSections.dbc face variant.
    #[must_use]
    pub const fn face_id(self) -> u8 {
        self.face_id
    }

    /// Returns the CharHairGeosets.dbc hair-style variant.
    #[must_use]
    pub const fn hair_style_id(self) -> u8 {
        self.hair_style_id
    }

    /// Returns the CharSections.dbc hair-color variant.
    #[must_use]
    pub const fn hair_color_id(self) -> u8 {
        self.hair_color_id
    }

    /// Returns the CharacterFacialHairStyles.dbc variant.
    #[must_use]
    pub const fn facial_hair_style_id(self) -> u8 {
        self.facial_hair_style_id
    }
}
