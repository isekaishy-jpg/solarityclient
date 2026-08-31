//! Exact build-12340 external SKIN profile decoding.

use glam::Vec3;

use crate::model::m2_shared::model_decode;
use crate::{ArchiveDescriptor, AssetError, AssetPath};

const HEADER_SIZE: usize = 48;
const SUBMESH_SIZE: usize = 48;
const BATCH_SIZE: usize = 24;

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
    /// Decodes one exact version-264 external profile without format detection.
    pub(super) fn decode(
        path: AssetPath,
        source: ArchiveDescriptor,
        bytes: &[u8],
        model_vertex_count: usize,
    ) -> Result<Self, AssetError> {
        if bytes.len() < HEADER_SIZE {
            return Err(model_decode(
                &path,
                "external SKIN header is truncated".to_owned(),
            ));
        }
        if bytes.get(..4) != Some(b"SKIN") {
            return Err(model_decode(
                &path,
                "expected build-12340 SKIN magic".to_owned(),
            ));
        }

        let vertex_ref = array_ref(&path, bytes, 4, "vertex lookup")?;
        let triangle_ref = array_ref(&path, bytes, 12, "triangle lookup")?;
        let bone_ref = array_ref(&path, bytes, 20, "bone indices")?;
        let submesh_ref = array_ref(&path, bytes, 28, "submeshes")?;
        let batch_ref = array_ref(&path, bytes, 36, "batches")?;
        validate_array(&path, bytes, vertex_ref, 2, "vertex lookup")?;
        validate_array(&path, bytes, triangle_ref, 2, "triangle lookup")?;
        validate_array(&path, bytes, bone_ref, 4, "bone indices")?;
        validate_array(&path, bytes, submesh_ref, SUBMESH_SIZE, "submeshes")?;
        validate_array(&path, bytes, batch_ref, BATCH_SIZE, "batches")?;

        let vertex_lookup = decode_u16_array(&path, bytes, vertex_ref, "vertex lookup")?;
        if vertex_lookup
            .iter()
            .any(|value| usize::from(*value) >= model_vertex_count)
        {
            return Err(model_decode(
                &path,
                "vertex lookup references a missing M2 vertex".to_owned(),
            ));
        }
        let triangle_lookup = decode_u16_array(&path, bytes, triangle_ref, "triangle lookup")?;
        if !triangle_lookup.len().is_multiple_of(3) {
            return Err(model_decode(
                &path,
                "triangle lookup length is not divisible by three".to_owned(),
            ));
        }
        if triangle_lookup
            .iter()
            .any(|value| usize::from(*value) >= vertex_lookup.len())
        {
            return Err(model_decode(
                &path,
                "triangle lookup references a missing profile vertex".to_owned(),
            ));
        }

        let bone_byte_count = bone_ref
            .count
            .checked_mul(4)
            .ok_or_else(|| model_decode(&path, "bone-index length overflows".to_owned()))?;
        let bone_end = bone_ref
            .offset
            .checked_add(bone_byte_count)
            .ok_or_else(|| model_decode(&path, "bone-index range overflows".to_owned()))?;
        let bone_indices = bytes[bone_ref.offset..bone_end].to_vec();
        if bone_ref.count != vertex_lookup.len() {
            return Err(model_decode(
                &path,
                "bone-index count does not match the vertex lookup".to_owned(),
            ));
        }

        let mut submeshes = Vec::with_capacity(submesh_ref.count);
        for index in 0..submesh_ref.count {
            let offset = record_offset(&path, submesh_ref, index, SUBMESH_SIZE, "submesh")?;
            let submesh = M2Submesh {
                id: read_u16(&path, bytes, offset, "submesh ID")?,
                level: read_u16(&path, bytes, offset + 2, "submesh level")?,
                vertex_start: read_u16(&path, bytes, offset + 4, "submesh vertex start")?,
                vertex_count: read_u16(&path, bytes, offset + 6, "submesh vertex count")?,
                triangle_start: read_u16(&path, bytes, offset + 8, "submesh triangle start")?,
                triangle_count: read_u16(&path, bytes, offset + 10, "submesh triangle count")?,
                bone_count: read_u16(&path, bytes, offset + 12, "submesh bone count")?,
                bone_start: read_u16(&path, bytes, offset + 14, "submesh bone start")?,
                bone_influence: read_u16(&path, bytes, offset + 16, "submesh bone influence")?,
                center_bone_index: read_u16(&path, bytes, offset + 18, "submesh center bone")?,
                center: read_vec3(&path, bytes, offset + 20, "submesh center")?,
                sort_center: read_vec3(&path, bytes, offset + 32, "submesh sort center")?,
                bounding_radius: read_f32(&path, bytes, offset + 44, "submesh radius")?,
            };
            validate_submesh(
                &path,
                index,
                &submesh,
                vertex_lookup.len(),
                triangle_lookup.len(),
            )?;
            submeshes.push(submesh);
        }

        let mut batches = Vec::with_capacity(batch_ref.count);
        for index in 0..batch_ref.count {
            let offset = record_offset(&path, batch_ref, index, BATCH_SIZE, "batch")?;
            let batch = M2Batch {
                flags: read_u8(&path, bytes, offset, "batch flags")?,
                priority_plane: read_i8(&path, bytes, offset + 1, "batch priority plane")?,
                shader_id: read_u16(&path, bytes, offset + 2, "batch shader")?,
                skin_section_index: read_u16(&path, bytes, offset + 4, "batch submesh")?,
                geoset_index: read_u16(&path, bytes, offset + 6, "batch geoset")?,
                color_index: read_u16(&path, bytes, offset + 8, "batch color")?,
                material_index: read_u16(&path, bytes, offset + 10, "batch material")?,
                material_layer: read_u16(&path, bytes, offset + 12, "batch layer")?,
                texture_count: read_u16(&path, bytes, offset + 14, "batch texture count")?,
                texture_combo_index: read_u16(&path, bytes, offset + 16, "batch texture combo")?,
                texture_coordinate_combo_index: read_u16(
                    &path,
                    bytes,
                    offset + 18,
                    "batch texture-coordinate combo",
                )?,
                texture_weight_combo_index: read_u16(
                    &path,
                    bytes,
                    offset + 20,
                    "batch texture-weight combo",
                )?,
                texture_transform_combo_index: read_u16(
                    &path,
                    bytes,
                    offset + 22,
                    "batch texture-transform combo",
                )?,
            };
            if usize::from(batch.skin_section_index) >= submeshes.len() {
                return Err(model_decode(
                    &path,
                    format!(
                        "batch {index} references missing submesh {}",
                        batch.skin_section_index
                    ),
                ));
            }
            batches.push(batch);
        }

        let bone_count_max = read_u32(&path, bytes, 44, "maximum bone count")?;
        Ok(Self {
            path,
            source,
            vertex_lookup,
            triangle_lookup,
            bone_indices,
            submeshes,
            batches,
            bone_count_max,
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

fn validate_submesh(
    path: &AssetPath,
    index: usize,
    submesh: &M2Submesh,
    vertex_count: usize,
    triangle_count: usize,
) -> Result<(), AssetError> {
    let vertex_end = usize::from(submesh.vertex_start)
        .checked_add(usize::from(submesh.vertex_count))
        .ok_or_else(|| model_decode(path, format!("submesh {index} vertex range overflows")))?;
    if vertex_end > vertex_count {
        return Err(model_decode(
            path,
            format!("submesh {index} vertex range exceeds the profile lookup"),
        ));
    }
    let triangle_start = usize::from(submesh.triangle_start)
        .checked_add(usize::from(submesh.level) << 16)
        .ok_or_else(|| model_decode(path, format!("submesh {index} triangle range overflows")))?;
    let triangle_end = triangle_start
        .checked_add(usize::from(submesh.triangle_count))
        .ok_or_else(|| model_decode(path, format!("submesh {index} triangle range overflows")))?;
    if triangle_end > triangle_count {
        return Err(model_decode(
            path,
            format!("submesh {index} triangle range exceeds the profile lookup"),
        ));
    }
    if !submesh.center.is_finite()
        || !submesh.sort_center.is_finite()
        || !submesh.bounding_radius.is_finite()
        || submesh.bounding_radius < 0.0
    {
        return Err(model_decode(
            path,
            format!("submesh {index} has invalid bounds"),
        ));
    }
    Ok(())
}

fn decode_u16_array(
    path: &AssetPath,
    bytes: &[u8],
    array: ArrayRef,
    field: &str,
) -> Result<Vec<u16>, AssetError> {
    (0..array.count)
        .map(|index| read_u16(path, bytes, array.offset + index * 2, field))
        .collect()
}

#[derive(Clone, Copy)]
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
            format!("{field} array exceeds the SKIN file"),
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

fn read_u32(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<u32, AssetError> {
    Ok(u32::from_le_bytes(read_bytes(path, bytes, offset, field)?))
}

fn read_u16(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<u16, AssetError> {
    Ok(u16::from_le_bytes(read_bytes(path, bytes, offset, field)?))
}

fn read_u8(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<u8, AssetError> {
    Ok(read_bytes::<1>(path, bytes, offset, field)?[0])
}

fn read_i8(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<i8, AssetError> {
    Ok(read_bytes::<1>(path, bytes, offset, field)?[0] as i8)
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
