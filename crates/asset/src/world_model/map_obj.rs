//! Owned build-12340 WMO root state and group identity.

use crate::{ArchiveDescriptor, AssetPath};

use super::map_obj_group::DecodedWorldModelGroup;

/// One completely admitted WMO root and its independently resolved groups.
pub struct DecodedWorldModel {
    path: AssetPath,
    source: ArchiveDescriptor,
    flags: u16,
    ambient_color: [u8; 4],
    world_model_id: u32,
    bounds: [[f32; 3]; 2],
    materials: Vec<WorldModelMaterial>,
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
        materials: Vec<WorldModelMaterial>,
        groups: Vec<DecodedWorldModelGroup>,
    ) -> Self {
        Self {
            path,
            source,
            flags,
            ambient_color,
            world_model_id,
            bounds,
            materials,
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

    /// Returns every MOMT material in authored table order.
    #[must_use]
    pub fn materials(&self) -> &[WorldModelMaterial] {
        &self.materials
    }

    /// Returns every group in exact numeric file order.
    #[must_use]
    pub fn groups(&self) -> &[DecodedWorldModelGroup] {
        &self.groups
    }
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
