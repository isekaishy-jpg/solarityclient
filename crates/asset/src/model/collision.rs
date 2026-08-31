//! Exact build-12340 dedicated M2 collision geometry.

use glam::Vec3;

use super::model_blob::M2ModelBounds;
use crate::model::m2_shared::model_decode;
use crate::{AssetError, AssetPath};

/// Dedicated unanimated M2 collision mesh from the build-12340 header arrays.
#[derive(Debug)]
pub struct M2CollisionMesh {
    bounds: M2ModelBounds,
    vertices: Vec<Vec3>,
    indices: Vec<u16>,
    face_normals: Vec<Vec3>,
}

impl M2CollisionMesh {
    /// Returns collision-local bounds, distinct from render bounds.
    #[must_use]
    pub const fn bounds(&self) -> M2ModelBounds {
        self.bounds
    }

    /// Returns authored collision vertices in model coordinates.
    #[must_use]
    pub fn vertices(&self) -> &[Vec3] {
        &self.vertices
    }

    /// Returns the direct collision triangle-list indices.
    #[must_use]
    pub fn indices(&self) -> &[u16] {
        &self.indices
    }

    /// Returns one authored model-space normal per collision triangle.
    #[must_use]
    pub fn face_normals(&self) -> &[Vec3] {
        &self.face_normals
    }
}

/// Decodes the three parallel collision arrays without dependency byte copies.
pub(super) fn decode_collision_mesh(
    path: &AssetPath,
    bytes: &[u8],
    bounds: M2ModelBounds,
) -> Result<Option<M2CollisionMesh>, AssetError> {
    let indices_ref = array_ref(path, bytes, 0xd8, "collision indices")?;
    let vertices_ref = array_ref(path, bytes, 0xe0, "collision vertices")?;
    let normals_ref = array_ref(path, bytes, 0xe8, "collision face normals")?;
    validate_array(path, bytes, indices_ref, 2, "collision indices")?;
    validate_array(path, bytes, vertices_ref, 12, "collision vertices")?;
    validate_array(path, bytes, normals_ref, 12, "collision face normals")?;

    if indices_ref.count == 0 && vertices_ref.count == 0 && normals_ref.count == 0 {
        return Ok(None);
    }
    if indices_ref.count == 0
        || vertices_ref.count == 0
        || !indices_ref.count.is_multiple_of(3)
        || normals_ref.count != indices_ref.count / 3
    {
        return Err(model_decode(
            path,
            "M2 collision must contain vertices and one face normal per triangle".to_owned(),
        ));
    }

    let vertices = decode_vectors(path, bytes, vertices_ref, "collision vertex")?;
    let face_normals = decode_vectors(path, bytes, normals_ref, "collision face normal")?;
    let mut indices = Vec::with_capacity(indices_ref.count);
    for index in 0..indices_ref.count {
        let value = read_u16(
            path,
            bytes,
            indices_ref.offset + index * 2,
            "collision index",
        )?;
        if usize::from(value) >= vertices.len() {
            return Err(model_decode(
                path,
                format!("collision index {index} references missing vertex {value}"),
            ));
        }
        indices.push(value);
    }
    if !bounds.minimum().is_finite()
        || !bounds.maximum().is_finite()
        || !bounds.sphere_radius().is_finite()
        || bounds.sphere_radius() < 0.0
        || (0..3).any(|axis| bounds.minimum()[axis] > bounds.maximum()[axis])
    {
        return Err(model_decode(
            path,
            "M2 collision bounds are invalid".to_owned(),
        ));
    }

    Ok(Some(M2CollisionMesh {
        bounds,
        vertices,
        indices,
        face_normals,
    }))
}

fn decode_vectors(
    path: &AssetPath,
    bytes: &[u8],
    array: ArrayRef,
    field: &str,
) -> Result<Vec<Vec3>, AssetError> {
    let mut values = Vec::with_capacity(array.count);
    for index in 0..array.count {
        let offset = array.offset + index * 12;
        let value = Vec3::new(
            read_f32(path, bytes, offset, field)?,
            read_f32(path, bytes, offset + 4, field)?,
            read_f32(path, bytes, offset + 8, field)?,
        );
        if !value.is_finite() {
            return Err(model_decode(path, format!("{field} {index} is not finite")));
        }
        values.push(value);
    }
    Ok(values)
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
            format!("{field} array exceeds the M2 file"),
        ));
    }
    Ok(())
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
