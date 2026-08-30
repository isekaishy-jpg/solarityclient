//! Owned CPU-side model data independent of the selected M2 decoder.

use glam::{Vec2, Vec3};
use wow_m2::chunks::texture::M2TextureType as DependencyTextureType;
use wow_m2::model::M2Model;
use wow_m2::skin::{OldSkin, SkinBatch, SkinSubmesh};

use crate::model::m2_shared::{ParsedSkin, model_decode};
use crate::{ArchiveDescriptor, AssetError, AssetPath};

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

/// One authored M2 attachment before animation or model transforms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2Attachment {
    id: u32,
    bone_index: i32,
    position: Vec3,
}

impl M2Attachment {
    /// Returns the stock attachment identifier.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.id
    }

    /// Returns the model bone that owns this attachment.
    #[must_use]
    pub const fn bone_index(self) -> i32 {
        self.bone_index
    }

    /// Returns the stable authored model-space position.
    ///
    /// This deliberately excludes the current bone pose. Build 12340 reads
    /// the authored Breath attachment position for player-camera height so
    /// the orbit pivot does not bob with the walk animation.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
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

/// A draw-section range and bone palette within one external SKIN profile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2Submesh {
    /// Stock geoset identifier, including its character customization suffix.
    pub id: u16,
    /// High bits extending `triangle_start` for index lists beyond 65,535 entries.
    pub level: u16,
    /// First entry in the profile's vertex lookup.
    pub vertex_start: u16,
    /// Number of entries in the profile's vertex lookup.
    pub vertex_count: u16,
    /// First entry in the profile's triangle lookup.
    pub triangle_start: u16,
    /// Number of entries in the profile's triangle lookup.
    pub triangle_count: u16,
    /// Number of model bones in this draw section's palette.
    pub bone_count: u16,
    /// First palette entry in the model bone lookup table.
    pub bone_start: u16,
    /// Maximum influencing bones used by a vertex in this section.
    pub bone_influence: u16,
    /// Model bone nearest the section center.
    pub center_bone_index: u16,
    /// Section center in stock model coordinates.
    pub center: Vec3,
    /// Sorting center in stock model coordinates.
    pub sort_center: Vec3,
    /// Section bounding-sphere radius.
    pub bounding_radius: f32,
}

/// One stock material batch attached to a SKIN submesh.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M2Batch {
    /// Batch behavior flags.
    pub flags: u8,
    /// Signed render-priority plane.
    pub priority_plane: i8,
    /// Packed build-12340 vertex/pixel shader selector.
    pub shader_id: u16,
    /// Submesh selected for the batch.
    pub skin_section_index: u16,
    /// Secondary geoset selector carried by stock data.
    pub geoset_index: u16,
    /// Index into the model color-animation table.
    pub color_index: u16,
    /// Index into the model render-flags table.
    pub material_index: u16,
    /// Material layer within the batch.
    pub material_layer: u16,
    /// Number of texture units consumed by the batch.
    pub texture_count: u16,
    /// First model texture-lookup entry.
    pub texture_combo_index: u16,
    /// First texture-coordinate lookup entry.
    pub texture_coordinate_combo_index: u16,
    /// First transparency lookup entry.
    pub texture_weight_combo_index: u16,
    /// First texture-animation lookup entry.
    pub texture_transform_combo_index: u16,
}

/// One external view/LOD profile selected through stock archive precedence.
#[derive(Clone, Debug)]
pub struct M2SkinProfile {
    path: AssetPath,
    source: ArchiveDescriptor,
    vertex_lookup: Vec<u16>,
    triangle_lookup: Vec<u16>,
    bone_indices: Vec<u8>,
    submeshes: Vec<M2Submesh>,
    batches: Vec<M2Batch>,
    bone_count_max: u32,
}

impl M2SkinProfile {
    /// Converts and validates a dependency-owned WotLK SKIN representation.
    pub(super) fn from_skin(
        path: AssetPath,
        source: ArchiveDescriptor,
        parsed: ParsedSkin,
        model_vertex_count: usize,
    ) -> Result<Self, AssetError> {
        let ParsedSkin {
            skin,
            center_bone_indices,
        } = parsed;
        validate_skin(&path, &skin, model_vertex_count)?;
        if center_bone_indices.len() != skin.submeshes.len() {
            return Err(model_decode(
                &path,
                "center-bone count does not match the submesh array".to_owned(),
            ));
        }

        Ok(Self {
            path,
            source,
            vertex_lookup: skin.indices,
            triangle_lookup: skin.triangles,
            bone_indices: skin.bone_indices,
            submeshes: skin
                .submeshes
                .iter()
                .zip(center_bone_indices)
                .map(|(submesh, center_bone_index)| convert_submesh(submesh, center_bone_index))
                .collect(),
            batches: skin.batches.iter().map(convert_batch).collect(),
            bone_count_max: skin.header.bone_count_max,
        })
    }

    /// Returns the exact archive-relative companion path.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the archive selected independently for this companion file.
    #[must_use]
    pub const fn source(&self) -> &ArchiveDescriptor {
        &self.source
    }

    /// Maps each profile-local vertex number to the M2 vertex array.
    #[must_use]
    pub fn vertex_lookup(&self) -> &[u16] {
        &self.vertex_lookup
    }

    /// Returns triangle entries indexing `vertex_lookup`, in groups of three.
    #[must_use]
    pub fn triangle_lookup(&self) -> &[u16] {
        &self.triangle_lookup
    }

    /// Returns four skin-local bone palette indices per vertex-lookup entry.
    #[must_use]
    pub fn bone_indices(&self) -> &[u8] {
        &self.bone_indices
    }

    /// Returns the profile draw sections.
    #[must_use]
    pub fn submeshes(&self) -> &[M2Submesh] {
        &self.submeshes
    }

    /// Returns material batches referencing the profile draw sections.
    #[must_use]
    pub fn batches(&self) -> &[M2Batch] {
        &self.batches
    }

    /// Returns the file-declared maximum bones used by one draw call.
    #[must_use]
    pub const fn bone_count_max(&self) -> u32 {
        self.bone_count_max
    }
}

/// Decoder-independent storage consumed by later animation and render stages.
#[derive(Debug)]
pub(super) struct ModelBlob {
    pub(super) name: Option<String>,
    pub(super) flags: u32,
    pub(super) bounds: M2ModelBounds,
    pub(super) attachments: Vec<M2Attachment>,
    pub(super) attachment_lookup: Vec<u16>,
    pub(super) vertices: Vec<M2Vertex>,
    pub(super) textures: Vec<M2Texture>,
    pub(super) materials: Vec<M2Material>,
    pub(super) bone_lookup: Vec<u16>,
    pub(super) texture_lookup: Vec<u16>,
    pub(super) texture_units: Vec<u16>,
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
    ) -> Result<Self, AssetError> {
        let flags = model.header.flags.bits();
        let bounds = M2ModelBounds {
            minimum: Vec3::from_array(model.header.bounding_box_min),
            maximum: Vec3::from_array(model.header.bounding_box_max),
            sphere_radius: model.header.bounding_sphere_radius,
        };
        let texture_combiner_combos = decode_texture_combiner_combos(path, bytes, &model)?;
        validate_attachment_lookup(path, &model)?;
        let attachments = model
            .attachments
            .iter()
            .map(|attachment| M2Attachment {
                id: attachment.id,
                bone_index: attachment.bone_index,
                position: Vec3::new(
                    attachment.position.x,
                    attachment.position.y,
                    attachment.position.z,
                ),
            })
            .collect();
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
            attachments,
            attachment_lookup: model.raw_data.attachment_lookup_table,
            vertices,
            textures,
            materials,
            bone_lookup: model.raw_data.bone_lookup_table,
            texture_lookup: model.raw_data.texture_lookup_table,
            texture_units: model.raw_data.texture_units,
            transparency_lookup: model.raw_data.transparency_lookup_table,
            texture_animation_lookup: model.raw_data.texture_animation_lookup,
            texture_combiner_combos,
        })
    }
}

/// Rejects a lookup entry that cannot name an authored attachment record.
fn validate_attachment_lookup(path: &AssetPath, model: &M2Model) -> Result<(), AssetError> {
    if let Some((slot, index)) = model
        .raw_data
        .attachment_lookup_table
        .iter()
        .copied()
        .enumerate()
        .find(|(_slot, index)| *index != u16::MAX && usize::from(*index) >= model.attachments.len())
    {
        return Err(model_decode(
            path,
            format!("attachment lookup slot {slot} references missing attachment {index}"),
        ));
    }
    Ok(())
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

/// Rejects cross-array references that the stock renderer cannot consume.
fn validate_skin(
    path: &AssetPath,
    skin: &OldSkin,
    model_vertex_count: usize,
) -> Result<(), AssetError> {
    if !skin.triangles.len().is_multiple_of(3) {
        return Err(model_decode(
            path,
            "triangle lookup length is not divisible by three".to_owned(),
        ));
    }
    if skin
        .indices
        .iter()
        .any(|&index| usize::from(index) >= model_vertex_count)
    {
        return Err(model_decode(
            path,
            "vertex lookup references a missing M2 vertex".to_owned(),
        ));
    }
    if skin
        .triangles
        .iter()
        .any(|&index| usize::from(index) >= skin.indices.len())
    {
        return Err(model_decode(
            path,
            "triangle lookup references a missing profile vertex".to_owned(),
        ));
    }

    let expected_bone_indices = skin
        .indices
        .len()
        .checked_mul(4)
        .ok_or_else(|| model_decode(path, "bone-index length overflows".to_owned()))?;
    if skin.bone_indices.len() != expected_bone_indices {
        return Err(model_decode(
            path,
            "bone-index count does not match the vertex lookup".to_owned(),
        ));
    }

    for submesh in &skin.submeshes {
        validate_submesh(path, submesh, skin.indices.len(), skin.triangles.len())?;
    }
    if skin
        .batches
        .iter()
        .any(|batch| usize::from(batch.skin_section_index) >= skin.submeshes.len())
    {
        return Err(model_decode(
            path,
            "material batch references a missing submesh".to_owned(),
        ));
    }
    Ok(())
}

/// Validates a submesh's two ranges without widening them through wrapping math.
fn validate_submesh(
    path: &AssetPath,
    submesh: &SkinSubmesh,
    vertex_count: usize,
    triangle_count: usize,
) -> Result<(), AssetError> {
    let vertex_end = usize::from(submesh.vertex_start)
        .checked_add(usize::from(submesh.vertex_count))
        .ok_or_else(|| model_decode(path, "submesh vertex range overflows".to_owned()))?;
    if vertex_end > vertex_count {
        return Err(model_decode(
            path,
            "submesh vertex range exceeds the profile lookup".to_owned(),
        ));
    }

    let triangle_start = usize::from(submesh.triangle_start)
        .checked_add(usize::from(submesh.level) << 16)
        .ok_or_else(|| model_decode(path, "submesh triangle range overflows".to_owned()))?;
    let triangle_end = triangle_start
        .checked_add(usize::from(submesh.triangle_count))
        .ok_or_else(|| model_decode(path, "submesh triangle range overflows".to_owned()))?;
    if triangle_end > triangle_count {
        return Err(model_decode(
            path,
            "submesh triangle range exceeds the profile lookup".to_owned(),
        ));
    }
    Ok(())
}

/// Converts one dependency submesh while preserving all build-12340 fields.
fn convert_submesh(submesh: &SkinSubmesh, center_bone_index: u16) -> M2Submesh {
    M2Submesh {
        id: submesh.id,
        level: submesh.level,
        vertex_start: submesh.vertex_start,
        vertex_count: submesh.vertex_count,
        triangle_start: submesh.triangle_start,
        triangle_count: submesh.triangle_count,
        bone_count: submesh.bone_count,
        bone_start: submesh.bone_start,
        bone_influence: submesh.bone_influence,
        center_bone_index,
        center: Vec3::from_array(submesh.center),
        sort_center: Vec3::from_array(submesh.sort_center),
        bounding_radius: submesh.bounding_radius,
    }
}

/// Converts one dependency batch while preserving all build-12340 fields.
fn convert_batch(batch: &SkinBatch) -> M2Batch {
    M2Batch {
        flags: batch.flags,
        priority_plane: batch.priority_plane,
        shader_id: batch.shader_id,
        skin_section_index: batch.skin_section_index,
        geoset_index: batch.geoset_index,
        color_index: batch.color_index,
        material_index: batch.material_index,
        material_layer: batch.material_layer,
        texture_count: batch.texture_count,
        texture_combo_index: batch.texture_combo_index,
        texture_coordinate_combo_index: batch.texture_coord_combo_index,
        texture_weight_combo_index: batch.texture_weight_combo_index,
        texture_transform_combo_index: batch.texture_transform_combo_index,
    }
}
