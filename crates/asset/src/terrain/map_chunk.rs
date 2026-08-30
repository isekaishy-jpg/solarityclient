//! Owned build-12340 MCNK geometry, material, and placement records.

use crate::archive::AssetPath;

use super::alpha_map::TerrainAlphaMap;

/// Number of terrain chunks along either axis of one ADT.
pub const TERRAIN_CHUNK_WIDTH: u8 = 16;

/// Number of staggered height/normal vertices in one MCNK.
pub const TERRAIN_CHUNK_VERTEX_COUNT: usize = 145;

/// Coordinates of one MCNK inside its parent ADT.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TerrainChunkIndex {
    x: u8,
    y: u8,
}

impl TerrainChunkIndex {
    /// Creates an index inside the stock 16-by-16 ADT chunk grid.
    #[must_use]
    pub const fn new(x: u8, y: u8) -> Option<Self> {
        if x < TERRAIN_CHUNK_WIDTH && y < TERRAIN_CHUNK_WIDTH {
            Some(Self { x, y })
        } else {
            None
        }
    }

    /// Returns the MCNK X coordinate.
    #[must_use]
    pub const fn x(self) -> u8 {
        self.x
    }

    /// Returns the MCNK Y coordinate.
    #[must_use]
    pub const fn y(self) -> u8 {
        self.y
    }
}

/// One stock MCLY layer referencing the ADT texture table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerrainTextureLayer {
    texture_index: u32,
    flags: u32,
    alpha_offset: u32,
    effect_id: u32,
}

impl TerrainTextureLayer {
    pub(super) const fn new(
        texture_index: u32,
        flags: u32,
        alpha_offset: u32,
        effect_id: u32,
    ) -> Self {
        Self {
            texture_index,
            flags,
            alpha_offset,
            effect_id,
        }
    }

    /// Returns the index into [`DecodedTerrainTile::textures`](super::DecodedTerrainTile::textures).
    #[must_use]
    pub const fn texture_index(self) -> u32 {
        self.texture_index
    }

    /// Returns the unmodified MCLY flags.
    #[must_use]
    pub const fn flags(self) -> u32 {
        self.flags
    }

    /// Returns this layer's byte offset in the MCAL payload.
    #[must_use]
    pub const fn alpha_offset(self) -> u32 {
        self.alpha_offset
    }

    /// Returns the `GroundEffectTexture.dbc` identifier.
    #[must_use]
    pub const fn effect_id(self) -> u32 {
        self.effect_id
    }
}

/// One fully decoded MCNK required for terrain mesh construction.
pub struct TerrainChunk {
    index: TerrainChunkIndex,
    flags: u32,
    area_id: u32,
    position: [f32; 3],
    holes: u16,
    heights: Box<[f32; TERRAIN_CHUNK_VERTEX_COUNT]>,
    normals: Box<[[f32; 3]; TERRAIN_CHUNK_VERTEX_COUNT]>,
    vertex_colors_bgra: Option<Box<[[u8; 4]; TERRAIN_CHUNK_VERTEX_COUNT]>>,
    layers: Vec<TerrainTextureLayer>,
    alpha_map: Option<TerrainAlphaMap>,
    shadow_bytes: Option<Box<[u8; 512]>>,
    doodad_references: Vec<u32>,
    world_model_references: Vec<u32>,
    sound_emitters: Vec<TerrainSoundEmitter>,
}

impl TerrainChunk {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        index: TerrainChunkIndex,
        flags: u32,
        area_id: u32,
        position: [f32; 3],
        holes: u16,
        heights: Box<[f32; TERRAIN_CHUNK_VERTEX_COUNT]>,
        normals: Box<[[f32; 3]; TERRAIN_CHUNK_VERTEX_COUNT]>,
        vertex_colors_bgra: Option<Box<[[u8; 4]; TERRAIN_CHUNK_VERTEX_COUNT]>>,
        layers: Vec<TerrainTextureLayer>,
        alpha_map: Option<TerrainAlphaMap>,
        shadow_bytes: Option<Box<[u8; 512]>>,
        doodad_references: Vec<u32>,
        world_model_references: Vec<u32>,
        sound_emitters: Vec<TerrainSoundEmitter>,
    ) -> Self {
        Self {
            index,
            flags,
            area_id,
            position,
            holes,
            heights,
            normals,
            vertex_colors_bgra,
            layers,
            alpha_map,
            shadow_bytes,
            doodad_references,
            world_model_references,
            sound_emitters,
        }
    }

    /// Returns this chunk's position in the ADT grid.
    #[must_use]
    pub const fn index(&self) -> TerrainChunkIndex {
        self.index
    }

    /// Returns the unmodified MCNK flags.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the client area identifier assigned to this chunk.
    #[must_use]
    pub const fn area_id(&self) -> u32 {
        self.area_id
    }

    /// Returns the authored MCNK base position.
    #[must_use]
    pub const fn position(&self) -> [f32; 3] {
        self.position
    }

    /// Returns the 4-by-4 low-resolution terrain-hole bitmap.
    #[must_use]
    pub const fn holes(&self) -> u16 {
        self.holes
    }

    /// Returns the 9-by-9 outer plus 8-by-8 inner relative heights.
    #[must_use]
    pub fn heights(&self) -> &[f32; TERRAIN_CHUNK_VERTEX_COUNT] {
        &self.heights
    }

    /// Returns normals in the same staggered order as the height vertices.
    #[must_use]
    pub fn normals(&self) -> &[[f32; 3]; TERRAIN_CHUNK_VERTEX_COUNT] {
        &self.normals
    }

    /// Returns optional authored MCCV bytes in their stored BGRA order.
    #[must_use]
    pub fn vertex_colors_bgra(&self) -> Option<&[[u8; 4]; TERRAIN_CHUNK_VERTEX_COUNT]> {
        self.vertex_colors_bgra.as_deref()
    }

    /// Returns at most four stock texture layers in draw order.
    #[must_use]
    pub fn layers(&self) -> &[TerrainTextureLayer] {
        &self.layers
    }

    /// Returns the optional decoded RGB blend planes in an RGBA8 upload map.
    #[must_use]
    pub const fn alpha_map(&self) -> Option<&TerrainAlphaMap> {
        self.alpha_map.as_ref()
    }

    /// Returns the optional 64-by-64 one-bit MCSH map.
    #[must_use]
    pub fn shadow_bytes(&self) -> Option<&[u8; 512]> {
        self.shadow_bytes.as_deref()
    }

    /// Returns indices into the tile's doodad-placement table.
    #[must_use]
    pub fn doodad_references(&self) -> &[u32] {
        &self.doodad_references
    }

    /// Returns indices into the tile's WMO-placement table.
    #[must_use]
    pub fn world_model_references(&self) -> &[u32] {
        &self.world_model_references
    }

    /// Returns positioned ambient sound emitters from MCSE.
    #[must_use]
    pub fn sound_emitters(&self) -> &[TerrainSoundEmitter] {
        &self.sound_emitters
    }
}

/// One positioned `SoundEntries.dbc` reference from MCSE.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainSoundEmitter {
    sound_entry_id: u32,
    position: [f32; 3],
    size: [f32; 3],
}

impl TerrainSoundEmitter {
    pub(super) const fn new(sound_entry_id: u32, position: [f32; 3], size: [f32; 3]) -> Self {
        Self {
            sound_entry_id,
            position,
            size,
        }
    }

    /// Returns the `SoundEntries.dbc` identifier.
    #[must_use]
    pub const fn sound_entry_id(self) -> u32 {
        self.sound_entry_id
    }

    /// Returns the authored world position.
    #[must_use]
    pub const fn position(self) -> [f32; 3] {
        self.position
    }

    /// Returns the three authored attenuation extents.
    #[must_use]
    pub const fn size(self) -> [f32; 3] {
        self.size
    }
}

/// One MDDF model placement with its resolved M2 path.
pub struct TerrainDoodadPlacement {
    path: AssetPath,
    unique_id: u32,
    position: [f32; 3],
    rotation: [f32; 3],
    scale: f32,
    flags: u16,
}

impl TerrainDoodadPlacement {
    pub(super) const fn new(
        path: AssetPath,
        unique_id: u32,
        position: [f32; 3],
        rotation: [f32; 3],
        scale: f32,
        flags: u16,
    ) -> Self {
        Self {
            path,
            unique_id,
            position,
            rotation,
            scale,
            flags,
        }
    }

    /// Returns the resolved M2 archive path.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the placement identifier shared across nearby ADTs.
    #[must_use]
    pub const fn unique_id(&self) -> u32 {
        self.unique_id
    }

    /// Returns the authored WoW world position.
    #[must_use]
    pub const fn position(&self) -> [f32; 3] {
        self.position
    }

    /// Returns authored Euler rotation in degrees.
    #[must_use]
    pub const fn rotation(&self) -> [f32; 3] {
        self.rotation
    }

    /// Returns the MDDF scale where 1024 represents 1.0.
    #[must_use]
    pub const fn scale(&self) -> f32 {
        self.scale
    }

    /// Returns the unmodified MDDF flags.
    #[must_use]
    pub const fn flags(&self) -> u16 {
        self.flags
    }
}

/// One MODF WMO placement with its resolved root-WMO path.
pub struct TerrainWorldModelPlacement {
    path: AssetPath,
    unique_id: u32,
    position: [f32; 3],
    rotation: [f32; 3],
    bounds: [[f32; 3]; 2],
    flags: u16,
    doodad_set: u16,
    name_set: u16,
}

impl TerrainWorldModelPlacement {
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn new(
        path: AssetPath,
        unique_id: u32,
        position: [f32; 3],
        rotation: [f32; 3],
        bounds: [[f32; 3]; 2],
        flags: u16,
        doodad_set: u16,
        name_set: u16,
    ) -> Self {
        Self {
            path,
            unique_id,
            position,
            rotation,
            bounds,
            flags,
            doodad_set,
            name_set,
        }
    }

    /// Returns the resolved root-WMO archive path.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the placement identifier shared across nearby ADTs.
    #[must_use]
    pub const fn unique_id(&self) -> u32 {
        self.unique_id
    }

    /// Returns the authored WoW world position.
    #[must_use]
    pub const fn position(&self) -> [f32; 3] {
        self.position
    }

    /// Returns authored Euler rotation in degrees.
    #[must_use]
    pub const fn rotation(&self) -> [f32; 3] {
        self.rotation
    }

    /// Returns the authored world-space lower and upper bounds.
    #[must_use]
    pub const fn bounds(&self) -> [[f32; 3]; 2] {
        self.bounds
    }

    /// Returns the unmodified MODF flags.
    #[must_use]
    pub const fn flags(&self) -> u16 {
        self.flags
    }

    /// Returns the WMO doodad-set selector.
    #[must_use]
    pub const fn doodad_set(&self) -> u16 {
        self.doodad_set
    }

    /// Returns the WMO name-set selector.
    #[must_use]
    pub const fn name_set(&self) -> u16 {
        self.name_set
    }
}
