//! Owned build-12340 WMO root state and group identity.

use crate::{ArchiveDescriptor, AssetPath};

use super::map_obj_group::DecodedWorldModelGroup;
use super::map_obj_spatial::{
    WorldModelGroupInfo, WorldModelPortal, WorldModelPortalReference, WorldModelSpatialData,
};
use super::{WorldModelDoodad, WorldModelDoodadSet, WorldModelDoodadSetError};

/// One completely admitted WMO root and its independently resolved groups.
pub struct DecodedWorldModel {
    path: AssetPath,
    source: ArchiveDescriptor,
    flags: u16,
    ambient_color: [u8; 4],
    world_model_id: u32,
    bounds: [[f32; 3]; 2],
    spatial: WorldModelSpatialData,
    materials: Vec<WorldModelMaterial>,
    doodad_sets: Vec<WorldModelDoodadSet>,
    doodads: Vec<WorldModelDoodad>,
    groups: Vec<DecodedWorldModelGroup>,
}

impl DecodedWorldModel {
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn new(
        path: AssetPath,
        source: ArchiveDescriptor,
        flags: u16,
        ambient_color: [u8; 4],
        world_model_id: u32,
        bounds: [[f32; 3]; 2],
        spatial: WorldModelSpatialData,
        materials: Vec<WorldModelMaterial>,
        doodad_sets: Vec<WorldModelDoodadSet>,
        doodads: Vec<WorldModelDoodad>,
        groups: Vec<DecodedWorldModelGroup>,
    ) -> Self {
        Self {
            path,
            source,
            flags,
            ambient_color,
            world_model_id,
            bounds,
            spatial,
            materials,
            doodad_sets,
            doodads,
            groups,
        }
    }

    /// Returns the canonical root-WMO archive path.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the archive selected independently for the root file.
    #[must_use]
    pub const fn source(&self) -> &ArchiveDescriptor {
        &self.source
    }

    /// Returns the raw build-12340 MOHD flags.
    #[must_use]
    pub const fn flags(&self) -> u16 {
        self.flags
    }

    /// Returns the exact MOHD ambient BGRA bytes.
    #[must_use]
    pub const fn ambient_color(&self) -> [u8; 4] {
        self.ambient_color
    }

    /// Returns the WMO identifier joined through `WMOAreaTable.dbc`.
    #[must_use]
    pub const fn world_model_id(&self) -> u32 {
        self.world_model_id
    }

    /// Returns root-local lower and upper authored bounds.
    #[must_use]
    pub const fn bounds(&self) -> [[f32; 3]; 2] {
        self.bounds
    }

    /// Returns root MOGI flags and bounds in group order, independently of MOGP.
    #[must_use]
    pub fn group_info(&self) -> &[WorldModelGroupInfo] {
        &self.spatial.groups
    }

    /// Returns the complete root MOPV table, preserving shared and unused vertices.
    #[must_use]
    pub fn portal_vertices(&self) -> &[[f32; 3]] {
        &self.spatial.vertices
    }

    /// Returns the MOPT polygons and planes in authored table order.
    #[must_use]
    pub fn portals(&self) -> &[WorldModelPortal] {
        &self.spatial.portals
    }

    /// Returns MOPR edges addressed by each group's retained portal range.
    #[must_use]
    pub fn portal_references(&self) -> &[WorldModelPortalReference] {
        &self.spatial.references
    }

    /// Returns optional MCVP convex-volume planes in authored order.
    /// Each record contains the unnormalized coefficients `[A, B, C, D]`.
    /// Native build 12340 retains these independently of group BSP geometry.
    #[must_use]
    pub fn convex_volume_planes(&self) -> &[[f32; 4]] {
        &self.spatial.convex_volume_planes
    }

    /// Returns every MOMT material in authored table order.
    #[must_use]
    pub fn materials(&self) -> &[WorldModelMaterial] {
        &self.materials
    }

    /// Returns root MODS ranges in exact authored table order.
    #[must_use]
    pub fn doodad_sets(&self) -> &[WorldModelDoodadSet] {
        &self.doodad_sets
    }

    /// Returns root MODD placements in exact authored table order.
    #[must_use]
    pub fn doodads(&self) -> &[WorldModelDoodad] {
        &self.doodads
    }

    /// Returns MODD indices enabled by one MODF doodad-set selector.
    ///
    /// Set zero is the additive global range and is always included. A
    /// nonzero selector adds exactly that alternative range after the global
    /// entries. Empty WMO doodad tables accept only selector zero.
    ///
    /// # Errors
    ///
    /// Returns [`WorldModelDoodadSetError`] when the selector is outside MODS.
    pub fn active_doodad_indices(
        &self,
        selector: u16,
    ) -> Result<Vec<usize>, WorldModelDoodadSetError> {
        if self.doodad_sets.is_empty() {
            return if selector == 0 {
                Ok(Vec::new())
            } else {
                Err(WorldModelDoodadSetError::new(
                    self.path.clone(),
                    selector,
                    0,
                ))
            };
        }
        let selected = self.doodad_sets.get(usize::from(selector)).ok_or_else(|| {
            WorldModelDoodadSetError::new(self.path.clone(), selector, self.doodad_sets.len())
        })?;
        let global = &self.doodad_sets[0];
        let mut indices = Vec::new();
        append_set_indices(&mut indices, global);
        if selector != 0 {
            append_set_indices(&mut indices, selected);
        }
        Ok(indices)
    }

    /// Returns active MODD owners in first group-MODR reference order.
    ///
    /// Native `0x007BF740` constructs an owner only when a loaded group names
    /// it. Unreferenced records do not create models, timers, or emitters;
    /// repeated references resolve the same root-local MODD lifetime.
    ///
    /// # Errors
    /// Returns [`WorldModelDoodadSetError`] for an invalid MODS selector.
    pub fn referenced_active_doodad_indices(
        &self,
        selector: u16,
    ) -> Result<Vec<usize>, WorldModelDoodadSetError> {
        let mut active = vec![false; self.doodads.len()];
        for index in self.active_doodad_indices(selector)? {
            active[index] = true;
        }
        let mut indices = Vec::new();
        for group in &self.groups {
            for &index in group.doodad_references() {
                let index = usize::from(index);
                if std::mem::take(&mut active[index]) {
                    indices.push(index);
                }
            }
        }
        Ok(indices)
    }

    /// Returns every group in exact numeric file order.
    #[must_use]
    pub fn groups(&self) -> &[DecodedWorldModelGroup] {
        &self.groups
    }
}

fn append_set_indices(indices: &mut Vec<usize>, set: &WorldModelDoodadSet) {
    let end = set.first_doodad() + set.doodad_count();
    indices.extend((set.first_doodad()..end).map(|index| index as usize));
}

/// Build-12340 MapObj shader selector after stock `FinishLoad` normalization.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorldModelShader {
    /// MapObj diffuse.
    Diffuse,
    /// MapObj specular.
    Specular,
    /// MapObj metal.
    Metal,
    /// Two-texture MapObj environment.
    Environment,
    /// Single-texture MapObj opaque.
    Opaque,
    /// Two-texture MapObj environment metal.
    EnvironmentMetal,
    /// Unified composite, also known as two-layer diffuse.
    Composite,
}

/// Direct build-12340 `EGxBlend` index stored by a WMO MOMT record.
///
/// Unlike M2's `M2BLEND` field, this value has already passed stock's format
/// translation and must be applied directly by the graphics backend.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorldModelBlendMode {
    /// Replace the framebuffer without alpha blending.
    Opaque,
    /// Replace the framebuffer after the stock 224/255 alpha test.
    AlphaKey,
    /// Conventional source-alpha interpolation.
    Alpha,
    /// Source-alpha-scaled additive blending.
    Add,
    /// Destination-color modulation.
    Mod,
    /// Doubled source/destination color modulation.
    Mod2x,
    /// Destination-color modulation followed by addition.
    ModAdd,
    /// Add the source scaled by inverse source alpha.
    InverseSourceAlphaAdd,
    /// Replace with the source scaled by inverse source alpha.
    InverseSourceAlphaOpaque,
    /// Replace with the source scaled by source alpha.
    SourceAlphaOpaque,
    /// Add the source without using source alpha.
    NoAlphaAdd,
}

impl WorldModelBlendMode {
    pub(super) const fn decode(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Opaque),
            1 => Some(Self::AlphaKey),
            2 => Some(Self::Alpha),
            3 => Some(Self::Add),
            4 => Some(Self::Mod),
            5 => Some(Self::Mod2x),
            6 => Some(Self::ModAdd),
            7 => Some(Self::InverseSourceAlphaAdd),
            8 => Some(Self::InverseSourceAlphaOpaque),
            9 => Some(Self::SourceAlphaOpaque),
            10 => Some(Self::NoAlphaAdd),
            _ => None,
        }
    }

    /// Returns the direct numeric GX state-table index.
    #[must_use]
    pub const fn index(self) -> u32 {
        match self {
            Self::Opaque => 0,
            Self::AlphaKey => 1,
            Self::Alpha => 2,
            Self::Add => 3,
            Self::Mod => 4,
            Self::Mod2x => 5,
            Self::ModAdd => 6,
            Self::InverseSourceAlphaAdd => 7,
            Self::InverseSourceAlphaOpaque => 8,
            Self::SourceAlphaOpaque => 9,
            Self::NoAlphaAdd => 10,
        }
    }
}

impl WorldModelShader {
    pub(super) const fn decode(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Diffuse),
            1 => Some(Self::Specular),
            2 => Some(Self::Metal),
            3 => Some(Self::Environment),
            4 => Some(Self::Opaque),
            5 => Some(Self::EnvironmentMetal),
            6 => Some(Self::Composite),
            _ => None,
        }
    }

    pub(super) const fn requires_secondary_texture(self) -> bool {
        matches!(
            self,
            Self::Environment | Self::EnvironmentMetal | Self::Composite
        )
    }

    /// Returns the direct index into stock's ordinary or unified effect table.
    #[must_use]
    pub const fn index(self) -> u32 {
        match self {
            Self::Diffuse => 0,
            Self::Specular => 1,
            Self::Metal => 2,
            Self::Environment => 3,
            Self::Opaque => 4,
            Self::EnvironmentMetal => 5,
            Self::Composite => 6,
        }
    }
}

/// One exact WotLK MOMT record with resolved archive texture paths.
#[derive(Clone, Debug)]
pub struct WorldModelMaterial {
    flags: u32,
    authored_shader: WorldModelShader,
    shader: WorldModelShader,
    blend_mode: WorldModelBlendMode,
    texture_offsets: [u32; 3],
    textures: [Option<AssetPath>; 3],
    emissive_color: u32,
    diffuse_color: u32,
    ground_type: u32,
    secondary_color: u32,
    secondary_flags: u32,
    runtime_data: [u8; 16],
}

impl WorldModelMaterial {
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn new(
        flags: u32,
        authored_shader: WorldModelShader,
        shader: WorldModelShader,
        blend_mode: WorldModelBlendMode,
        texture_offsets: [u32; 3],
        textures: [Option<AssetPath>; 3],
        emissive_color: u32,
        diffuse_color: u32,
        ground_type: u32,
        secondary_color: u32,
        secondary_flags: u32,
        runtime_data: [u8; 16],
    ) -> Self {
        Self {
            flags,
            authored_shader,
            shader,
            blend_mode,
            texture_offsets,
            textures,
            emissive_color,
            diffuse_color,
            ground_type,
            secondary_color,
            secondary_flags,
            runtime_data,
        }
    }

    /// Returns all authored MOMT flags without M2 flag reinterpretation.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the on-disk shader selector before stock normalization.
    #[must_use]
    pub const fn authored_shader(&self) -> WorldModelShader {
        self.authored_shader
    }

    /// Returns the selector used after build-12340 `FinishLoad` processing.
    #[must_use]
    pub const fn shader(&self) -> WorldModelShader {
        self.shader
    }

    /// Returns the direct EGxBlend index stored by MOMT.
    #[must_use]
    pub const fn blend_mode(&self) -> WorldModelBlendMode {
        self.blend_mode
    }

    /// Returns the three exact offsets into the MOTX byte string table.
    #[must_use]
    pub const fn texture_offsets(&self) -> [u32; 3] {
        self.texture_offsets
    }

    /// Returns resolved texture paths; valid empty optional slots remain absent.
    #[must_use]
    pub const fn textures(&self) -> &[Option<AssetPath>; 3] {
        &self.textures
    }

    /// Returns the packed MOMT emissive BGRA word.
    #[must_use]
    pub const fn emissive_color(&self) -> u32 {
        self.emissive_color
    }

    /// Returns the packed MOMT diffuse BGRA word.
    #[must_use]
    pub const fn diffuse_color(&self) -> u32 {
        self.diffuse_color
    }

    /// Returns the authored ground-effect identifier.
    #[must_use]
    pub const fn ground_type(&self) -> u32 {
        self.ground_type
    }

    /// Returns MOMT's second packed color word.
    #[must_use]
    pub const fn secondary_color(&self) -> u32 {
        self.secondary_color
    }

    /// Returns MOMT's second raw flag word.
    #[must_use]
    pub const fn secondary_flags(&self) -> u32 {
        self.secondary_flags
    }

    /// Returns the final sixteen on-disk MOMT bytes retained without meaning.
    #[must_use]
    pub const fn runtime_data(&self) -> [u8; 16] {
        self.runtime_data
    }
}
