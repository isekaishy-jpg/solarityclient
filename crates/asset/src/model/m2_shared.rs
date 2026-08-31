//! Build-12340 M2 parsing rules shared by model assets.

use std::io::Cursor;

use wow_m2::header::M2Header;
use wow_m2::model::M2Model;

use crate::{AssetError, AssetPath};

/// The only M2 header version shipped by client build 12340.
const BUILD_12340_M2_VERSION: u32 = 264;

/// Bytes occupied by a build-12340 M2 vertex.
const M2_VERTEX_SIZE: usize = 48;

/// Applies build 12340's model-cache filename conversion before archive lookup.
///
/// Stock DBC records still carry legacy `.mdl` and `.mdx` names even though
/// the corresponding archive entry contains an M2. The cache accepts exactly
/// those two legacy extensions and `.m2`; it does not infer an absent or
/// unrelated extension.
pub(crate) fn canonical_model_path(path: &AssetPath) -> Result<AssetPath, AssetError> {
    let value = path.as_str();
    let Some((stem, extension)) = value.rsplit_once('.') else {
        return Err(model_decode(
            path,
            "model path has no supported extension".to_owned(),
        ));
    };

    match extension {
        "M2" => Ok(path.clone()),
        "MDL" | "MDX" => AssetPath::new(format!("{stem}.M2")),
        _ => Err(model_decode(
            path,
            format!("unsupported model path extension .{extension}"),
        )),
    }
}

/// Parses an exact build-12340 legacy M2 without retaining dependency types.
pub(super) fn parse_model(path: &AssetPath, bytes: &mut [u8]) -> Result<M2Model, AssetError> {
    validate_model_prefix(path, bytes)?;
    validate_model_texture_arrays(path, bytes)?;

    // Solarity owns the complete animation/skeleton boundary. wow-m2 0.7 also
    // reads signed lookups as unsigned, repairs malformed lookup headers, and
    // reads build-12340 animated/effect records as later layouts. Hide all of
    // those arrays; exact WotLK decoders consume the restored bytes.
    // Patching in place avoids cloning an HD-sized model for a parser view.
    let exact_arrays = [
        0x14_usize, 0x1c, 0x24, 0x2c, 0x34, 0x48, 0x58, 0x60, 0x68, 0x78, 0x80, 0x88, 0x90, 0x98,
        0xd8, 0xe0, 0xe8, 0xf0, 0xf8, 0x100, 0x108, 0x110, 0x118, 0x120, 0x128,
    ];
    let mut saved = [[0_u8; 8]; 25];
    for (slot, offset) in saved.iter_mut().zip(exact_arrays) {
        let header = bytes.get_mut(offset..offset + 8).ok_or_else(|| {
            model_decode(
                path,
                "build-12340 exact-array header is truncated".to_owned(),
            )
        })?;
        slot.copy_from_slice(header);
        header.fill(0);
    }
    let parsed = M2Model::parse_legacy(&mut Cursor::new(&*bytes));
    for (slot, offset) in saved.iter().zip(exact_arrays) {
        bytes[offset..offset + 8].copy_from_slice(slot);
    }
    let mut model = parsed
        .map_err(|source| model_decode(path, format!("invalid build-12340 MD20 data: {source}")))?;

    if model.header.version != BUILD_12340_M2_VERSION {
        return Err(model_decode(
            path,
            format!(
                "expected M2 version {BUILD_12340_M2_VERSION}, found {}",
                model.header.version
            ),
        ));
    }

    restore_vertex_influences(path, bytes, &mut model)?;
    validate_model_name(path, bytes, &model)?;
    Ok(model)
}

/// Preflights texture records and their nested strings before decoder allocation.
fn validate_model_texture_arrays(path: &AssetPath, bytes: &[u8]) -> Result<(), AssetError> {
    const TEXTURE_RECORD_SIZE: usize = 16;

    let mut cursor = Cursor::new(bytes);
    let header = M2Header::parse(&mut cursor)
        .map_err(|source| model_decode(path, format!("invalid build-12340 header: {source}")))?;
    let count = header.textures.count as usize;
    let offset = header.textures.offset as usize;
    let byte_count = count
        .checked_mul(TEXTURE_RECORD_SIZE)
        .ok_or_else(|| model_decode(path, "texture-record byte range overflows".to_owned()))?;
    let end = offset
        .checked_add(byte_count)
        .ok_or_else(|| model_decode(path, "texture-record byte range overflows".to_owned()))?;
    if end > bytes.len() {
        return Err(model_decode(
            path,
            "texture-record array exceeds the M2 file".to_owned(),
        ));
    }

    for index in 0..count {
        let record = offset + index * TEXTURE_RECORD_SIZE;
        let name_count = read_u32(path, bytes, record + 8, "texture name")? as usize;
        let name_offset = read_u32(path, bytes, record + 12, "texture name")? as usize;
        let name_end = name_offset
            .checked_add(name_count)
            .ok_or_else(|| model_decode(path, "texture-name byte range overflows".to_owned()))?;
        if name_end > bytes.len() {
            return Err(model_decode(
                path,
                format!("texture {index} name exceeds the M2 file"),
            ));
        }
    }
    Ok(())
}

/// Derives the stock `Model00.skin` through `ModelNN.skin` companion name.
pub(super) fn skin_path(model_path: &AssetPath, profile: u32) -> Result<AssetPath, AssetError> {
    let Some(stem) = model_path.as_str().strip_suffix(".M2") else {
        return Err(model_decode(
            model_path,
            "model path does not end in .m2".to_owned(),
        ));
    };

    AssetPath::new(format!("{stem}{profile:02}.skin"))
}

/// Rejects later MD21/chunked files and non-12340 legacy layouts up front.
fn validate_model_prefix(path: &AssetPath, bytes: &[u8]) -> Result<(), AssetError> {
    let Some(prefix) = bytes.get(..8) else {
        return Err(model_decode(path, "M2 header is truncated".to_owned()));
    };
    if &prefix[..4] != b"MD20" {
        return Err(model_decode(
            path,
            "expected build-12340 MD20 magic".to_owned(),
        ));
    }

    let version = u32::from_le_bytes(
        prefix[4..8]
            .try_into()
            .map_err(|_| model_decode(path, "M2 version field is truncated".to_owned()))?,
    );
    if version != BUILD_12340_M2_VERSION {
        return Err(model_decode(
            path,
            format!("expected M2 version {BUILD_12340_M2_VERSION}, found {version}"),
        ));
    }
    Ok(())
}

/// Replaces the dependency's repaired weights and indices with archive bytes.
///
/// `wow-m2` intentionally clamps invalid bone indices in its default parser.
/// Stock build 12340 does not perform that compatibility repair at this format
/// boundary, so the facade restores the exact four weights and four indices.
fn restore_vertex_influences(
    path: &AssetPath,
    bytes: &[u8],
    model: &mut M2Model,
) -> Result<(), AssetError> {
    let count = usize::try_from(model.header.vertices.count)
        .map_err(|source| model_decode(path, source.to_string()))?;
    if count != model.vertices.len() {
        return Err(model_decode(
            path,
            "decoded vertex count does not match the M2 header".to_owned(),
        ));
    }

    let start = usize::try_from(model.header.vertices.offset)
        .map_err(|source| model_decode(path, source.to_string()))?;
    let byte_count = count
        .checked_mul(M2_VERTEX_SIZE)
        .ok_or_else(|| model_decode(path, "vertex byte range overflows".to_owned()))?;
    let end = start
        .checked_add(byte_count)
        .ok_or_else(|| model_decode(path, "vertex byte range overflows".to_owned()))?;
    let raw_vertex_bytes = bytes
        .get(start..end)
        .ok_or_else(|| model_decode(path, "vertex array exceeds the M2 file".to_owned()))?;
    let (raw_vertices, remainder) = raw_vertex_bytes.as_chunks::<M2_VERTEX_SIZE>();
    if !remainder.is_empty() {
        return Err(model_decode(
            path,
            "vertex array is not record-aligned".to_owned(),
        ));
    }

    for (vertex, raw) in model.vertices.iter_mut().zip(raw_vertices) {
        vertex.bone_weights.copy_from_slice(&raw[12..16]);
        vertex.bone_indices.copy_from_slice(&raw[16..20]);
    }
    Ok(())
}

/// Requires the in-file model name to be a complete UTF-8 C string when present.
fn validate_model_name(path: &AssetPath, bytes: &[u8], model: &M2Model) -> Result<(), AssetError> {
    let count = usize::try_from(model.header.name.count)
        .map_err(|source| model_decode(path, source.to_string()))?;
    if count == 0 {
        return Ok(());
    }

    let start = usize::try_from(model.header.name.offset)
        .map_err(|source| model_decode(path, source.to_string()))?;
    let end = start
        .checked_add(count)
        .ok_or_else(|| model_decode(path, "model-name byte range overflows".to_owned()))?;
    let raw_name = bytes
        .get(start..end)
        .ok_or_else(|| model_decode(path, "model name exceeds the M2 file".to_owned()))?;
    if raw_name.last() != Some(&0) {
        return Err(model_decode(
            path,
            "model name is not NUL-terminated".to_owned(),
        ));
    }
    std::str::from_utf8(&raw_name[..raw_name.len() - 1])
        .map_err(|source| model_decode(path, format!("model name is not UTF-8: {source}")))?;
    Ok(())
}

/// Reads one required old-SKIN header word without an unchecked slice conversion.
fn read_u32(path: &AssetPath, bytes: &[u8], offset: usize, label: &str) -> Result<u32, AssetError> {
    let word = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| model_decode(path, format!("{label} header is truncated")))?;
    Ok(u32::from_le_bytes(word.try_into().map_err(|_| {
        model_decode(path, format!("{label} header is truncated"))
    })?))
}

/// Builds the stable asset error used for both M2 and companion failures.
pub(super) fn model_decode(path: &AssetPath, message: String) -> AssetError {
    AssetError::ModelDecode {
        path: path.clone(),
        message,
    }
}
