//! Typed regions and ordered layers of the stock character atlas.

use std::fmt;

use solarity_asset::AssetPath;

/// Default build-12340 character component texture width and height.
pub(super) const STOCK_CHARACTER_ATLAS_SIZE: u32 = 256;

/// One of the ten non-overlapping regions in the stock 512-unit atlas layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CharacterAtlasRegion {
    /// Upper arm surface.
    ArmUpper,
    /// Lower arm surface.
    ArmLower,
    /// Hand surface.
    Hand,
    /// Upper torso surface.
    TorsoUpper,
    /// Lower torso surface.
    TorsoLower,
    /// Upper leg surface.
    LegUpper,
    /// Lower leg surface.
    LegLower,
    /// Foot surface.
    Foot,
    /// Upper head surface.
    HeadUpper,
    /// Lower head surface.
    HeadLower,
}

impl CharacterAtlasRegion {
    /// Returns the region rectangle scaled to stock's default 256-pixel atlas.
    #[must_use]
    pub const fn rect(self) -> CharacterAtlasRect {
        match self {
            Self::ArmUpper => CharacterAtlasRect::new(0, 0, 128, 64),
            Self::ArmLower => CharacterAtlasRect::new(0, 64, 128, 64),
            Self::Hand => CharacterAtlasRect::new(0, 128, 128, 32),
            Self::TorsoUpper => CharacterAtlasRect::new(128, 0, 128, 64),
            Self::TorsoLower => CharacterAtlasRect::new(128, 64, 128, 32),
            Self::LegUpper => CharacterAtlasRect::new(128, 96, 128, 64),
            Self::LegLower => CharacterAtlasRect::new(128, 160, 128, 64),
            Self::Foot => CharacterAtlasRect::new(128, 224, 128, 32),
            Self::HeadUpper => CharacterAtlasRect::new(0, 160, 128, 32),
            Self::HeadLower => CharacterAtlasRect::new(0, 192, 128, 64),
        }
    }
}

/// A validated pixel rectangle within the 256-by-256 character atlas.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterAtlasRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl CharacterAtlasRect {
    /// Creates a compile-time rectangle from the recovered stock layout.
    const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Returns the left pixel coordinate.
    #[must_use]
    pub const fn x(self) -> u32 {
        self.x
    }

    /// Returns the top pixel coordinate.
    #[must_use]
    pub const fn y(self) -> u32 {
        self.y
    }

    /// Returns the rectangle width in pixels.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Returns the rectangle height in pixels.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }
}

/// The semantic source of one stock character atlas layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CharacterAtlasLayerKind {
    /// Opaque base skin copied into body regions.
    Skin,
    /// Face detail overlay.
    Face,
    /// Beard, markings, and other facial-feature overlay.
    FacialHair,
    /// Hair detail overlay pasted onto head regions.
    Hair,
    /// Base underwear visible when equipment does not cover it.
    Underwear,
}

impl fmt::Display for CharacterAtlasLayerKind {
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

/// One archive texture paste in stock region-local render-preparation order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterAtlasLayer {
    kind: CharacterAtlasLayerKind,
    region: CharacterAtlasRegion,
    path: AssetPath,
}

impl CharacterAtlasLayer {
    /// Creates a validated layer after its DBC texture name is normalized.
    pub(super) fn new(
        kind: CharacterAtlasLayerKind,
        region: CharacterAtlasRegion,
        path: AssetPath,
    ) -> Self {
        Self { kind, region, path }
    }

    /// Returns the semantic layer source.
    #[must_use]
    pub const fn kind(&self) -> CharacterAtlasLayerKind {
        self.kind
    }

    /// Returns the destination atlas region.
    #[must_use]
    pub const fn region(&self) -> CharacterAtlasRegion {
        self.region
    }

    /// Returns the normalized MPQ texture path.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }
}
