//! Owned CPU-side model data independent of the selected M2 decoder.

use glam::{Vec2, Vec3};

use crate::model::collision::{M2CollisionMesh, decode_collision_mesh};
use crate::model::lookups::M2LookupTables;
use crate::model::m2_shared::{model_decode, validate_model_prefix};
use crate::{AssetError, AssetPath};

const HEADER_SIZE: usize = 0x130;
const EXTENDED_HEADER_SIZE: usize = 0x138;
const VERTEX_SIZE: usize = 48;
const TEXTURE_SIZE: usize = 16;
const MATERIAL_SIZE: usize = 4;
const USE_TEXTURE_COMBINERS: u32 = 0x8;

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
    invalid_filename: bool,
}

/// Stock image selected for one hardcoded M2 texture declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M2HardcodedTextureSource<'path> {
    /// A valid client-internal path is passed to the shared texture loader.
    Archive(&'path AssetPath),
    /// An empty filename produces stock's opaque white generated texture.
    StockWhite,
    /// A non-archive filename reaches the shared missing-texture fallback.
    StockFailure,
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

    /// Resolves stock's hardcoded-filename boundary without exposing unsafe
    /// build-machine paths to the archive API.
    #[must_use]
    pub const fn hardcoded_source(&self) -> Option<M2HardcodedTextureSource<'_>> {
        if !matches!(self.kind, M2TextureKind::Hardcoded) {
            return None;
        }
        if self.invalid_filename {
            return Some(M2HardcodedTextureSource::StockFailure);
        }
        match self.filename.as_ref() {
            Some(path) => Some(M2HardcodedTextureSource::Archive(path)),
            None => Some(M2HardcodedTextureSource::StockWhite),
        }
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

/// Fixed build-12340 header fields needed by the unanimated model body.
#[derive(Clone, Copy, Debug)]
pub(super) struct ModelBodyHeader {
    name: ArrayRef,
    flags: u32,
    vertices: ArrayRef,
    skin_profile_count: u32,
    textures: ArrayRef,
    materials: ArrayRef,
    bounds: M2ModelBounds,
    collision_bounds: M2ModelBounds,
    texture_combiner_combos: Option<ArrayRef>,
}

impl ModelBodyHeader {
    /// Reads exact version-264 offsets and preflights body arrays before allocation.
    pub(super) fn decode(path: &AssetPath, bytes: &[u8]) -> Result<Self, AssetError> {
        validate_model_prefix(path, bytes)?;
        if bytes.len() < HEADER_SIZE {
            return Err(model_decode(
                path,
                "build-12340 M2 header is truncated".to_owned(),
            ));
        }

        let name = array_ref(path, bytes, 0x08, "model name")?;
        let flags = read_u32(path, bytes, 0x10, "global flags")?;
        let vertices = array_ref(path, bytes, 0x3c, "vertices")?;
        let textures = array_ref(path, bytes, 0x50, "textures")?;
        let materials = array_ref(path, bytes, 0x70, "materials")?;
        let texture_combiner_combos = if flags & USE_TEXTURE_COMBINERS != 0 {
            if bytes.len() < EXTENDED_HEADER_SIZE {
                return Err(model_decode(
                    path,
                    "texture-combiner flag requires the extended M2 header".to_owned(),
                ));
            }
            Some(array_ref(path, bytes, 0x130, "texture-combiner table")?)
        } else {
            None
        };

        validate_array(path, bytes, name, 1, "model name")?;
        validate_array(path, bytes, vertices, VERTEX_SIZE, "vertices")?;
        validate_array(path, bytes, textures, TEXTURE_SIZE, "textures")?;
        validate_array(path, bytes, materials, MATERIAL_SIZE, "materials")?;
        if let Some(array) = texture_combiner_combos {
            validate_array(path, bytes, array, 2, "texture-combiner table")?;
        }
        for index in 0..textures.count {
            let record = record_offset(path, textures, index, TEXTURE_SIZE, "texture")?;
            let filename = array_ref(path, bytes, record + 8, "texture filename")?;
            validate_array(path, bytes, filename, 1, "texture filename")?;
        }

        Ok(Self {
            name,
            flags,
            vertices,
            skin_profile_count: read_u32(path, bytes, 0x44, "skin-profile count")?,
            textures,
            materials,
            bounds: read_bounds(path, bytes, 0xa0, "model bounds")?,
            collision_bounds: read_bounds(path, bytes, 0xbc, "collision bounds")?,
            texture_combiner_combos,
        })
    }

    pub(super) const fn skin_profile_count(self) -> u32 {
        self.skin_profile_count
    }

    pub(super) const fn vertex_count(self) -> usize {
        self.vertices.count
    }

    pub(super) const fn texture_count(self) -> usize {
        self.textures.count
    }
}

/// Decoder-independent storage consumed by later animation and render stages.
#[derive(Debug)]
pub(super) struct ModelBlob {
    pub(super) name: Option<String>,
    pub(super) flags: u32,
    pub(super) skin_profile_count: u32,
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
    /// Decodes the exact unanimated body once, directly into retained storage.
    pub(super) fn decode(
        path: &AssetPath,
        bytes: &[u8],
        header: ModelBodyHeader,
        lookups: M2LookupTables,
    ) -> Result<Self, AssetError> {
        Ok(Self {
            name: decode_model_name(path, bytes, header.name)?,
            flags: header.flags,
            skin_profile_count: header.skin_profile_count,
            bounds: header.bounds,
            collision: decode_collision_mesh(path, bytes, header.collision_bounds)?,
            vertices: decode_vertices(path, bytes, header.vertices)?,
            textures: decode_textures(path, bytes, header.textures)?,
            materials: decode_materials(path, bytes, header.materials)?,
            replaceable_texture_lookup: lookups.replaceable_textures,
            bone_lookup: lookups.bones,
            texture_lookup: lookups.textures,
            texture_coordinate_lookup: lookups.texture_coordinates,
            transparency_lookup: lookups.texture_weights,
            texture_animation_lookup: lookups.texture_transforms,
            texture_combiner_combos: decode_u16_array(
                path,
                bytes,
                header.texture_combiner_combos,
                "texture-combiner table",
            )?,
        })
    }
}

fn decode_vertices(
    path: &AssetPath,
    bytes: &[u8],
    vertices: ArrayRef,
) -> Result<Vec<M2Vertex>, AssetError> {
    let mut decoded = Vec::with_capacity(vertices.count);
    for index in 0..vertices.count {
        let offset = record_offset(path, vertices, index, VERTEX_SIZE, "vertex")?;
        decoded.push(M2Vertex {
            position: read_vec3(path, bytes, offset, "vertex position")?,
            bone_weights: read_bytes(path, bytes, offset + 12, "vertex bone weights")?,
            bone_indices: read_bytes(path, bytes, offset + 16, "vertex bone indices")?,
            normal: read_vec3(path, bytes, offset + 20, "vertex normal")?,
            texture_coordinates: [
                read_vec2(path, bytes, offset + 32, "vertex texture coordinates")?,
                read_vec2(path, bytes, offset + 40, "vertex texture coordinates")?,
            ],
        });
    }
    Ok(decoded)
}

fn decode_textures(
    path: &AssetPath,
    bytes: &[u8],
    textures: ArrayRef,
) -> Result<Vec<M2Texture>, AssetError> {
    let mut decoded = Vec::with_capacity(textures.count);
    for index in 0..textures.count {
        let offset = record_offset(path, textures, index, TEXTURE_SIZE, "texture")?;
        let kind = match read_u32(path, bytes, offset, "texture replacement type")? {
            0 => M2TextureKind::Hardcoded,
            1 => M2TextureKind::Body,
            2 => M2TextureKind::Item,
            3 => M2TextureKind::WeaponArmorBasic,
            4 => M2TextureKind::WeaponBlade,
            5 => M2TextureKind::WeaponHandle,
            6 => M2TextureKind::Environment,
            7 => M2TextureKind::Hair,
            8 => M2TextureKind::SkinExtra,
            9 => M2TextureKind::UiSkin,
            10 => M2TextureKind::TaurenMane,
            11 => M2TextureKind::Monster1,
            12 => M2TextureKind::Monster2,
            13 => M2TextureKind::Monster3,
            14 => M2TextureKind::ItemIcon,
            value => {
                return Err(model_decode(
                    path,
                    format!("texture {index} has unsupported replacement type {value}"),
                ));
            }
        };
        let filename_ref = array_ref(path, bytes, offset + 8, "texture filename")?;
        let (filename, invalid_filename) =
            match decode_c_string(path, bytes, filename_ref, "texture filename")?
                .filter(|filename| !filename.is_empty())
            {
                Some(filename) => match AssetPath::new(&filename) {
                    Ok(filename) => (Some(filename), false),
                    // M2Shared.cpp 0x0083CC80 passes nonempty authored names
                    // directly to Texture.cpp 0x004B9760. A failed archive
                    // request returns the shared green texture rather than
                    // rejecting the complete model. Keep that result typed so
                    // build-machine names never cross the AssetPath boundary.
                    Err(_source) if filename.starts_with("Z:\\World of Warcraft Proj Server\\") => {
                        (None, true)
                    }
                    Err(source) => {
                        return Err(model_decode(
                            path,
                            format!("texture {index} name is invalid: {source}"),
                        ));
                    }
                },
                None => (None, false),
            };
        decoded.push(M2Texture {
            kind,
            flags: read_u32(path, bytes, offset + 4, "texture flags")?,
            filename,
            invalid_filename,
        });
    }
    Ok(decoded)
}

fn decode_materials(
    path: &AssetPath,
    bytes: &[u8],
    materials: ArrayRef,
) -> Result<Vec<M2Material>, AssetError> {
    let mut decoded = Vec::with_capacity(materials.count);
    for index in 0..materials.count {
        let offset = record_offset(path, materials, index, MATERIAL_SIZE, "material")?;
        let blend_mode = match read_u16(path, bytes, offset + 2, "material blend mode")? {
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
        decoded.push(M2Material {
            flags: read_u16(path, bytes, offset, "material flags")?,
            blend_mode,
        });
    }
    Ok(decoded)
}

fn decode_u16_array(
    path: &AssetPath,
    bytes: &[u8],
    array: Option<ArrayRef>,
    field: &str,
) -> Result<Vec<u16>, AssetError> {
    let Some(array) = array else {
        return Ok(Vec::new());
    };
    (0..array.count)
        .map(|index| read_u16(path, bytes, array.offset + index * 2, field))
        .collect()
}

fn decode_c_string(
    path: &AssetPath,
    bytes: &[u8],
    array: ArrayRef,
    field: &str,
) -> Result<Option<String>, AssetError> {
    if array.count == 0 {
        return Ok(None);
    }
    let end = array
        .offset
        .checked_add(array.count)
        .ok_or_else(|| model_decode(path, format!("{field} byte range overflows")))?;
    let raw = bytes
        .get(array.offset..end)
        .ok_or_else(|| model_decode(path, format!("{field} exceeds the M2 file")))?;
    if raw.last() != Some(&0) || raw[..raw.len() - 1].contains(&0) {
        return Err(model_decode(
            path,
            format!("{field} is not one complete C string"),
        ));
    }
    let value = std::str::from_utf8(&raw[..raw.len() - 1])
        .map_err(|source| model_decode(path, format!("{field} is not UTF-8: {source}")))?;
    Ok(Some(value.to_owned()))
}

/// Reads the padded model-name field using its first terminator.
fn decode_model_name(
    path: &AssetPath,
    bytes: &[u8],
    array: ArrayRef,
) -> Result<Option<String>, AssetError> {
    if array.count == 0 {
        return Ok(None);
    }
    let end = array
        .offset
        .checked_add(array.count)
        .ok_or_else(|| model_decode(path, "model-name byte range overflows".to_owned()))?;
    let raw = bytes
        .get(array.offset..end)
        .ok_or_else(|| model_decode(path, "model name exceeds the M2 file".to_owned()))?;
    if raw.last() != Some(&0) {
        return Err(model_decode(
            path,
            "model name is not NUL-terminated".to_owned(),
        ));
    }
    let terminator = raw
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(raw.len());
    let value = std::str::from_utf8(&raw[..terminator])
        .map_err(|source| model_decode(path, format!("model name is not UTF-8: {source}")))?;
    Ok(Some(value.to_owned()))
}

fn read_bounds(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<M2ModelBounds, AssetError> {
    Ok(M2ModelBounds {
        minimum: read_vec3(path, bytes, offset, field)?,
        maximum: read_vec3(path, bytes, offset + 12, field)?,
        sphere_radius: read_f32(path, bytes, offset + 24, field)?,
    })
}

fn read_vec3(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<Vec3, AssetError> {
    Ok(Vec3::new(
        read_f32(path, bytes, offset, field)?,
        read_f32(path, bytes, offset + 4, field)?,
        read_f32(path, bytes, offset + 8, field)?,
    ))
}

fn read_vec2(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<Vec2, AssetError> {
    Ok(Vec2::new(
        read_f32(path, bytes, offset, field)?,
        read_f32(path, bytes, offset + 4, field)?,
    ))
}

#[derive(Clone, Copy, Debug)]
struct ArrayRef {
    count: usize,
    offset: usize,
}

fn array_ref(
    path: &AssetPath,
    bytes: &[u8],
    pair_offset: usize,
    field: &str,
) -> Result<ArrayRef, AssetError> {
    Ok(ArrayRef {
        count: read_u32(path, bytes, pair_offset, field)? as usize,
        offset: read_u32(path, bytes, pair_offset + 4, field)? as usize,
    })
}

fn validate_array(
    path: &AssetPath,
    bytes: &[u8],
    array: ArrayRef,
    stride: usize,
    field: &str,
) -> Result<(), AssetError> {
    let byte_count = array
        .count
        .checked_mul(stride)
        .ok_or_else(|| model_decode(path, format!("{field} byte range overflows")))?;
    let end = array
        .offset
        .checked_add(byte_count)
        .ok_or_else(|| model_decode(path, format!("{field} byte range overflows")))?;
    if end > bytes.len() {
        return Err(model_decode(
            path,
            format!("{field} array exceeds the M2 file"),
        ));
    }
    Ok(())
}

fn record_offset(
    path: &AssetPath,
    array: ArrayRef,
    index: usize,
    stride: usize,
    field: &str,
) -> Result<usize, AssetError> {
    index
        .checked_mul(stride)
        .and_then(|relative| array.offset.checked_add(relative))
        .ok_or_else(|| model_decode(path, format!("{field} byte range overflows")))
}

fn read_u32(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<u32, AssetError> {
    Ok(u32::from_le_bytes(read_bytes(path, bytes, offset, field)?))
}

fn read_u16(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<u16, AssetError> {
    Ok(u16::from_le_bytes(read_bytes(path, bytes, offset, field)?))
}

fn read_f32(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<f32, AssetError> {
    Ok(f32::from_le_bytes(read_bytes(path, bytes, offset, field)?))
}

fn read_bytes<const N: usize>(
    path: &AssetPath,
    bytes: &[u8],
    offset: usize,
    field: &str,
) -> Result<[u8; N], AssetError> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| model_decode(path, format!("{field} byte range overflows")))?;
    bytes
        .get(offset..end)
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| model_decode(path, format!("{field} is truncated")))
}
