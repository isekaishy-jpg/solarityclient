//! Typed regions and ordered layers of the stock character atlas.

use std::fmt;

use solarity_asset::AssetPath;

/// Lowest script-visible build-12340 component-texture level.
pub(super) const STOCK_CHARACTER_COMPONENT_TEXTURE_LEVEL_MINIMUM: u8 = 8;

/// Highest and default script-visible build-12340 component-texture level.
pub(super) const STOCK_CHARACTER_COMPONENT_TEXTURE_LEVEL_MAXIMUM: u8 = 9;

/// Typed build-12340 character component-texture resolution.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CharacterComponentTextureLevel(u8);

impl CharacterComponentTextureLevel {
    /// Stock's registered default, a 512-by-512 component texture.
    pub const DEFAULT: Self = Self(STOCK_CHARACTER_COMPONENT_TEXTURE_LEVEL_MAXIMUM);

    /// Creates a level accepted by the stock-facing settings boundary.
    #[must_use]
    pub const fn new(level: u8) -> Option<Self> {
        if level >= STOCK_CHARACTER_COMPONENT_TEXTURE_LEVEL_MINIMUM
            && level <= STOCK_CHARACTER_COMPONENT_TEXTURE_LEVEL_MAXIMUM
        {
            Some(Self(level))
        } else {
            None
        }
    }

    /// Clamps an untrusted numeric CVar value to the supported stock range.
    #[must_use]
    pub const fn clamped(level: u32) -> Self {
        if level < STOCK_CHARACTER_COMPONENT_TEXTURE_LEVEL_MINIMUM as u32 {
            Self(STOCK_CHARACTER_COMPONENT_TEXTURE_LEVEL_MINIMUM)
        } else if level > STOCK_CHARACTER_COMPONENT_TEXTURE_LEVEL_MAXIMUM as u32 {
            Self(STOCK_CHARACTER_COMPONENT_TEXTURE_LEVEL_MAXIMUM)
        } else {
            Self(level as u8)
        }
    }

    /// Returns the script-visible logarithmic texture level.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }

    /// Returns the square top-mip extent selected by this level.
    #[must_use]
    pub const fn atlas_size(self) -> u32 {
        1_u32 << self.0
    }

    /// Returns the complete top-through-one mip count.
    #[must_use]
    pub const fn mip_count(self) -> usize {
        self.0 as usize + 1
    }
}

impl Default for CharacterComponentTextureLevel {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One of the ten non-overlapping regions in stock's 256-unit base layout.
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
    /// Returns the region rectangle in stock's 256-unit base layout.
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

/// A validated rectangle within a character-atlas mip.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterAtlasRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl CharacterAtlasRect {
    /// Returns a rectangle covering an atlas with the supplied extent.
    pub(super) const fn atlas(size: u32) -> Self {
        Self::new(0, 0, size, size)
    }

    /// Creates a compile-time rectangle from the recovered stock layout.
    pub(super) const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
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

/// One fully composed RGBA8 mip of the dynamic stock body texture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterAtlasMip {
    level: usize,
    width: u32,
    rgba8: Vec<u8>,
}

impl CharacterAtlasMip {
    /// Creates one internally validated square destination mip.
    pub(super) fn new(level: usize, width: u32, rgba8: Vec<u8>) -> Self {
        Self {
            level,
            width,
            rgba8,
        }
    }

    /// Returns the zero-based destination mip level.
    #[must_use]
    pub const fn level(&self) -> usize {
        self.level
    }

    /// Returns this square mip's width and height.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns tightly packed row-major RGBA8 pixels.
    #[must_use]
    pub fn rgba8(&self) -> &[u8] {
        &self.rgba8
    }

    /// Returns mutable pixels within the private composition boundary.
    pub(super) fn rgba8_mut(&mut self) -> &mut [u8] {
        &mut self.rgba8
    }
}

/// Complete CPU-side mip chain for the dynamic stock character body texture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterAtlasTexture {
    mips: Vec<CharacterAtlasMip>,
}

impl CharacterAtlasTexture {
    /// Wraps the fixed complete mip chain after composition.
    pub(super) fn new(mips: Vec<CharacterAtlasMip>) -> Self {
        Self { mips }
    }

    /// Returns all destination mips from the selected top extent through 1-by-1.
    #[must_use]
    pub fn mips(&self) -> &[CharacterAtlasMip] {
        &self.mips
    }

    /// Returns one zero-based destination mip when present.
    #[must_use]
    pub fn mip(&self, level: usize) -> Option<&CharacterAtlasMip> {
        self.mips.get(level)
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
    /// Equipped-item component texture pasted at its stock slot priority.
    Item,
}

impl fmt::Display for CharacterAtlasLayerKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Skin => "skin",
            Self::Face => "face",
            Self::FacialHair => "facial-hair",
            Self::Hair => "hair",
            Self::Underwear => "underwear",
            Self::Item => "item",
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
