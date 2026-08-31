//! Owned CPU-side model data independent of the selected M2 decoder.

use glam::{Vec2, Vec3};
use wow_m2::chunks::texture::M2TextureType as DependencyTextureType;
use wow_m2::model::M2Model;

use crate::model::collision::{M2CollisionMesh, decode_collision_mesh};
use crate::model::lookups::M2LookupTables;
use crate::model::m2_shared::model_decode;
use crate::{AssetError, AssetPath};

/// Stock semantic source for one M2 texture slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M2TextureKind {
    /// A concrete filename embedded in the M2.
    Hardcoded,
    /// Character body and composited clothing texture.
    Body,
    /// Item or cape replacement texture.
    Item,
    /// Base weapon or armor replacement texture.
    WeaponArmorBasic,
    /// Weapon blade replacement texture.
    WeaponBlade,
    /// Weapon handle replacement texture.
    WeaponHandle,
    /// Environment-supplied texture.
    Environment,
    /// Character hair or beard texture.
    Hair,
    /// Character accessory texture.
    SkinExtra,
    /// Inventory-art texture.
    UiSkin,
    /// Tauren mane texture.
    TaurenMane,
    /// First creature display texture replacement.
    Monster1,
    /// Second creature display texture replacement.
    Monster2,
    /// Third creature display texture replacement.
    Monster3,
    /// Item icon texture.
    ItemIcon,
}

/// One M2 texture declaration before display/customization replacement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M2Texture {
    kind: M2TextureKind,
    flags: u32,
    filename: Option<AssetPath>,
}

impl M2Texture {
    /// Returns the stock replacement category.
    #[must_use]
    pub const fn kind(&self) -> M2TextureKind {
        self.kind
    }

    /// Returns all authored texture flags, including currently unknown bits.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the embedded filename when this declaration carries one.
    #[must_use]
    pub const fn filename(&self) -> Option<&AssetPath> {
        self.filename.as_ref()
    }
}

/// Exact build-12340 M2 material blend operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum M2BlendMode {
    /// Opaque replacement.
    Opaque,
    /// One-bit alpha test.
    AlphaKey,
    /// Conventional alpha blending.
    Alpha,
    /// Additive color while retaining source alpha behavior.
    NoAlphaAdd,
    /// Additive blending.
    Add,
    /// Multiplicative blending.
    Mod,
    /// Two-times multiplicative blending.
    Mod2x,
}

/// One render-flags record referenced by SKIN batches.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M2Material {
    flags: u16,
    blend_mode: M2BlendMode,
}

impl M2Material {
    /// Returns all stock render flags, including currently unknown bits.
    #[must_use]
    pub const fn flags(self) -> u16 {
        self.flags
    }

    /// Returns the exact blend operation.
    #[must_use]
    pub const fn blend_mode(self) -> M2BlendMode {
        self.blend_mode
    }
}

/// One build-12340 M2 vertex in stock model coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2Vertex {
    position: Vec3,
    bone_weights: [u8; 4],
    bone_indices: [u8; 4],
    normal: Vec3,
    texture_coordinates: [Vec2; 2],
}

/// Authored model-space render bounds from the build-12340 M2 header.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2ModelBounds {
    minimum: Vec3,
    maximum: Vec3,
    sphere_radius: f32,
}

impl M2ModelBounds {
    /// Returns the authored lower model-space corner.
    #[must_use]
    pub const fn minimum(self) -> Vec3 {
        self.minimum
    }

    /// Returns the authored upper model-space corner.
    #[must_use]
    pub const fn maximum(self) -> Vec3 {
        self.maximum
    }

    /// Returns the authored model-space bounding-sphere radius.
    #[must_use]
    pub const fn sphere_radius(self) -> f32 {
        self.sphere_radius
    }
}

impl M2Vertex {
    /// Returns the untransformed stock-coordinate position.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }

    /// Returns the four byte-normalized bone weights exactly as stored.
    #[must_use]
    pub const fn bone_weights(self) -> [u8; 4] {
        self.bone_weights
    }

    /// Returns the four model bone indices exactly as stored.
    #[must_use]
    pub const fn bone_indices(self) -> [u8; 4] {
        self.bone_indices
    }

    /// Returns the untransformed stock-coordinate normal.
    #[must_use]
    pub const fn normal(self) -> Vec3 {
        self.normal
    }

    /// Returns the two texture-coordinate sets present in build 12340.
    #[must_use]
    pub const fn texture_coordinates(self) -> [Vec2; 2] {
        self.texture_coordinates
    }
}

/// Decoder-independent storage consumed by later animation and render stages.
#[derive(Debug)]
pub(super) struct ModelBlob {
    pub(super) name: Option<String>,
    pub(super) flags: u32,
    pub(super) bounds: M2ModelBounds,
    pub(super) collision: Option<M2CollisionMesh>,
    pub(super) vertices: Vec<M2Vertex>,
    pub(super) textures: Vec<M2Texture>,
    pub(super) materials: Vec<M2Material>,
    pub(super) replaceable_texture_lookup: Vec<u16>,
    pub(super) bone_lookup: Vec<u16>,
    pub(super) texture_lookup: Vec<u16>,
    pub(super) texture_coordinate_lookup: Vec<i16>,
    pub(super) transparency_lookup: Vec<u16>,
    pub(super) texture_animation_lookup: Vec<u16>,
    pub(super) texture_combiner_combos: Vec<u16>,
}

impl ModelBlob {
    /// Converts every exposed vertex field without supplying absent values.
    pub(super) fn from_model(
        path: &AssetPath,
        bytes: &[u8],
        model: M2Model,
        lookups: M2LookupTables,
    ) -> Result<Self, AssetError> {
        let flags = model.header.flags.bits();
        let bounds = M2ModelBounds {
            minimum: Vec3::from_array(model.header.bounding_box_min),
            maximum: Vec3::from_array(model.header.bounding_box_max),
            sphere_radius: model.header.bounding_sphere_radius,
        };
        let collision = decode_collision_mesh(
            path,
            bytes,
            M2ModelBounds {
                minimum: Vec3::from_array(model.header.collision_box_min),
                maximum: Vec3::from_array(model.header.collision_box_max),
                sphere_radius: model.header.collision_sphere_radius,
            },
        )?;
        let texture_combiner_combos = decode_texture_combiner_combos(path, bytes, &model)?;
        let mut vertices = Vec::with_capacity(model.vertices.len());
        for vertex in model.vertices {
            let texture_coordinates2 = vertex.tex_coords2.ok_or_else(|| {
                model_decode(
                    path,
                    "build-12340 vertex has no second texture coordinates".to_owned(),
                )
            })?;
            vertices.push(M2Vertex {
                position: Vec3::new(vertex.position.x, vertex.position.y, vertex.position.z),
                bone_weights: vertex.bone_weights,
                bone_indices: vertex.bone_indices,
                normal: Vec3::new(vertex.normal.x, vertex.normal.y, vertex.normal.z),
                texture_coordinates: [
                    Vec2::new(vertex.tex_coords.x, vertex.tex_coords.y),
                    Vec2::new(texture_coordinates2.x, texture_coordinates2.y),
                ],
            });
        }

        let textures = model
            .textures
            .iter()
            .enumerate()
            .map(|(index, texture)| convert_texture(path, bytes, index, texture))
            .collect::<Result<Vec<_>, _>>()?;
        let materials = model
            .materials
            .iter()
            .enumerate()
            .map(|(index, material)| convert_material(path, index, material))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            name: model.name,
            flags,
            bounds,
            collision,
            vertices,
            textures,
            materials,
            replaceable_texture_lookup: lookups.replaceable_textures,
            bone_lookup: lookups.bones,
            texture_lookup: lookups.textures,
            texture_coordinate_lookup: lookups.texture_coordinates,
            transparency_lookup: lookups.texture_weights,
            texture_animation_lookup: lookups.texture_transforms,
            texture_combiner_combos,
        })
    }
}

/// Reads WotLK's optional trailing `u16` combiner table exactly as `M2Data` stores it.
fn decode_texture_combiner_combos(
    path: &AssetPath,
    bytes: &[u8],
    model: &M2Model,
) -> Result<Vec<u16>, AssetError> {
    let Some(array) = model.header.texture_combiner_combos else {
        return Ok(Vec::new());
    };
    let count = array.count as usize;
    let offset = array.offset as usize;
    let byte_count = count.checked_mul(size_of::<u16>()).ok_or_else(|| {
        model_decode(
            path,
            "texture-combiner table byte count overflows".to_owned(),
        )
    })?;
    let end = offset.checked_add(byte_count).ok_or_else(|| {
        model_decode(
            path,
            "texture-combiner table byte range overflows".to_owned(),
        )
    })?;
    let raw = bytes.get(offset..end).ok_or_else(|| {
        model_decode(
            path,
            "texture-combiner table exceeds the M2 file".to_owned(),
        )
    })?;
    Ok(raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|value| u16::from_le_bytes([value[0], value[1]]))
        .collect())
}

/// Converts one texture declaration and validates its nested archive path.
fn convert_texture(
    path: &AssetPath,
    bytes: &[u8],
    index: usize,
    texture: &wow_m2::chunks::texture::M2Texture,
) -> Result<M2Texture, AssetError> {
    let kind = match texture.texture_type {
        DependencyTextureType::Hardcoded => M2TextureKind::Hardcoded,
        DependencyTextureType::Body => M2TextureKind::Body,
        DependencyTextureType::Item => M2TextureKind::Item,
        DependencyTextureType::WeaponArmorBasic => M2TextureKind::WeaponArmorBasic,
        DependencyTextureType::WeaponBlade => M2TextureKind::WeaponBlade,
        DependencyTextureType::WeaponHandle => M2TextureKind::WeaponHandle,
        DependencyTextureType::Environment => M2TextureKind::Environment,
        DependencyTextureType::Hair => M2TextureKind::Hair,
        DependencyTextureType::SkinExtra => M2TextureKind::SkinExtra,
        DependencyTextureType::UiSkin => M2TextureKind::UiSkin,
        DependencyTextureType::TaurenMane => M2TextureKind::TaurenMane,
        DependencyTextureType::Monster1 => M2TextureKind::Monster1,
        DependencyTextureType::Monster2 => M2TextureKind::Monster2,
        DependencyTextureType::Monster3 => M2TextureKind::Monster3,
        DependencyTextureType::ItemIcon => M2TextureKind::ItemIcon,
        DependencyTextureType::Unknown => {
            return Err(model_decode(
                path,
                format!("texture {index} has an unsupported replacement type"),
            ));
        }
    };
    let filename = decode_texture_filename(path, bytes, index, texture)?;
    Ok(M2Texture {
        kind,
        flags: texture.flags.bits(),
        filename,
    })
}

/// Reads a complete NUL-terminated ASCII path rather than the dependency's lossy string.
fn decode_texture_filename(
    path: &AssetPath,
    bytes: &[u8],
    index: usize,
    texture: &wow_m2::chunks::texture::M2Texture,
) -> Result<Option<AssetPath>, AssetError> {
    let count = texture.filename.array.count as usize;
    if count == 0 {
        return Ok(None);
    }
    let offset = texture.filename.array.offset as usize;
    let end = offset
        .checked_add(count)
        .ok_or_else(|| model_decode(path, "texture-name byte range overflows".to_owned()))?;
    let raw = bytes
        .get(offset..end)
        .ok_or_else(|| model_decode(path, format!("texture {index} name exceeds the M2 file")))?;
    if raw.last() != Some(&0) || raw[..raw.len() - 1].contains(&0) {
        return Err(model_decode(
            path,
            format!("texture {index} name is not one complete C string"),
        ));
    }
    let name = std::str::from_utf8(&raw[..raw.len() - 1]).map_err(|source| {
        model_decode(path, format!("texture {index} name is not UTF-8: {source}"))
    })?;
    AssetPath::new(name)
        .map(Some)
        .map_err(|source| model_decode(path, format!("texture {index} name is invalid: {source}")))
}

/// Narrows the dependency bitfield to build 12340's seven blend values.
fn convert_material(
    path: &AssetPath,
    index: usize,
    material: &wow_m2::chunks::material::M2Material,
) -> Result<M2Material, AssetError> {
    let blend_mode = match material.blend_mode.bits() {
        0 => M2BlendMode::Opaque,
        1 => M2BlendMode::AlphaKey,
        2 => M2BlendMode::Alpha,
        3 => M2BlendMode::NoAlphaAdd,
        4 => M2BlendMode::Add,
        5 => M2BlendMode::Mod,
        6 => M2BlendMode::Mod2x,
        value => {
            return Err(model_decode(
                path,
                format!("material {index} has unsupported blend mode {value}"),
            ));
        }
    };
    Ok(M2Material {
        flags: material.flags.bits(),
        blend_mode,
    })
}
