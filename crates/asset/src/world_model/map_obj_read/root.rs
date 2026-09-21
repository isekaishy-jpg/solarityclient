//! Root material, doodad and fog tables retain authored order and strict build-era validation.
use super::super::map_obj::{WorldModelBlendMode, WorldModelMaterial, WorldModelShader};
use super::super::{WorldModelDoodad, WorldModelDoodadSet};
use super::BUILD_12340_WMO_VERSION;
use super::layout::{
    chunk_name, read_f32, read_u32, require_chunk, scan_chunks, validate_bounds,
    validate_unique_chunks, world_model_error, world_model_message,
};
use crate::{AssetError, AssetPath};

pub(super) fn decode_fogs(
    path: &AssetPath,
    bytes: &[u8],
) -> Result<Vec<super::super::WorldModelFog>, AssetError> {
    let chunks = scan_chunks(path, bytes, "WMO root")?;
    let Some(chunk) = chunks.iter().find(|chunk| chunk.magic == *b"GOFM") else {
        return Ok(Vec::new());
    };
    let (records, remainder) = bytes[chunk.payload_start..chunk.payload_end].as_chunks::<48>();
    if !remainder.is_empty() {
        return Err(world_model_message(
            path,
            "MFOG requires complete 48-byte records",
        ));
    }
    records
        .iter()
        .map(|record| {
            let mut scalars = [0.; 9];
            for (value, offset) in scalars.iter_mut().zip([4, 8, 12, 16, 20, 24, 28, 36, 40]) {
                *value = read_f32(path, record, offset, "MFOG scalar")?;
            }
            if scalars.iter().any(|value| !value.is_finite()) {
                return Err(world_model_message(
                    path,
                    "MFOG contains a nonfinite scalar",
                ));
            }
            Ok(super::super::WorldModelFog {
                flags: read_u32(path, record, 0, "MFOG flags")?,
                position: glam::Vec3::new(scalars[0], scalars[1], scalars[2]),
                inner_radius: scalars[3],
                outer_radius: scalars[4],
                banks: [
                    super::super::WorldModelFogBank {
                        end: scalars[5],
                        start_ratio: scalars[6],
                        color: read_u32(path, record, 32, "MFOG dry color")?,
                    },
                    super::super::WorldModelFogBank {
                        end: scalars[7],
                        start_ratio: scalars[8],
                        color: read_u32(path, record, 44, "MFOG wet color")?,
                    },
                ],
            })
        })
        .collect()
}

pub(super) fn decode_doodads(
    path: &AssetPath,
    bytes: &[u8],
    root: &wow_wmo::root_parser::WmoRoot,
) -> Result<(Vec<WorldModelDoodadSet>, Vec<WorldModelDoodad>), AssetError> {
    let chunks = scan_chunks(path, bytes, "WMO root")?;
    let names = chunks
        .iter()
        .find(|chunk| chunk.magic == *b"NDOM")
        .and_then(|chunk| bytes.get(chunk.payload_start..chunk.payload_end))
        .unwrap_or_default();
    let mut doodads = Vec::with_capacity(root.doodad_defs.len());
    for (index, doodad) in root.doodad_defs.iter().enumerate() {
        let name_offset = doodad.name_index();
        let model_path = decode_doodad_path(path, names, name_offset, index)?;
        if doodad.position.into_iter().any(|value| !value.is_finite())
            || doodad
                .orientation
                .into_iter()
                .any(|value| !value.is_finite())
            || !doodad.scale.is_finite()
            || doodad.scale <= 0.0
        {
            return Err(world_model_message(
                path,
                format!("MODD doodad {index} has an invalid transform"),
            ));
        }
        let orientation_length_squared = doodad
            .orientation
            .into_iter()
            .map(|value| value * value)
            .sum::<f32>();
        if orientation_length_squared <= f32::EPSILON {
            return Err(world_model_message(
                path,
                format!("MODD doodad {index} has a zero quaternion"),
            ));
        }
        doodads.push(WorldModelDoodad::new(
            model_path,
            name_offset,
            (doodad.name_index_and_flags >> 24) as u8,
            doodad.position,
            doodad.orientation,
            doodad.scale,
            doodad.color,
        ));
    }

    let mut sets = Vec::with_capacity(root.doodad_sets.len());
    for (index, set) in root.doodad_sets.iter().enumerate() {
        let end = set
            .start_index
            .checked_add(set.count)
            .and_then(|end| usize::try_from(end).ok())
            .ok_or_else(|| {
                world_model_message(path, format!("MODS set {index} range overflows"))
            })?;
        if end > doodads.len() {
            return Err(world_model_message(
                path,
                format!("MODS set {index} references a doodad outside MODD"),
            ));
        }
        let name_end = set
            .name
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(set.name.len());
        let name = std::str::from_utf8(&set.name[..name_end])
            .map_err(|error| {
                world_model_message(path, format!("MODS set {index} name is not UTF-8: {error}"))
            })?
            .to_owned();
        sets.push(WorldModelDoodadSet::new(
            name,
            set.start_index,
            set.count,
            set.padding,
        ));
    }
    Ok((sets, doodads))
}

fn decode_doodad_path(
    root_path: &AssetPath,
    names: &[u8],
    offset: u32,
    doodad_index: usize,
) -> Result<AssetPath, AssetError> {
    let offset = usize::try_from(offset).map_err(|error| world_model_error(root_path, error))?;
    let tail = names.get(offset..).ok_or_else(|| {
        world_model_message(
            root_path,
            format!(
                "MODD doodad {doodad_index} references MODN offset {offset} outside {} bytes",
                names.len()
            ),
        )
    })?;
    let end = tail.iter().position(|byte| *byte == 0).ok_or_else(|| {
        world_model_message(
            root_path,
            format!("MODD doodad {doodad_index} model path is not null terminated"),
        )
    })?;
    if end == 0 {
        return Err(world_model_message(
            root_path,
            format!("MODD doodad {doodad_index} model path is empty"),
        ));
    }
    let value = std::str::from_utf8(&tail[..end]).map_err(|error| {
        world_model_message(
            root_path,
            format!("MODD doodad {doodad_index} model path is not UTF-8: {error}"),
        )
    })?;
    AssetPath::new(value).map_err(|error| world_model_error(root_path, error))
}

pub(super) fn decode_materials(
    path: &AssetPath,
    bytes: &[u8],
    root: &wow_wmo::root_parser::WmoRoot,
) -> Result<Vec<WorldModelMaterial>, AssetError> {
    let chunks = scan_chunks(path, bytes, "WMO root")?;
    let texture_bytes = chunks
        .iter()
        .find(|chunk| chunk.magic == *b"XTOM")
        .and_then(|chunk| bytes.get(chunk.payload_start..chunk.payload_end))
        .unwrap_or_default();
    let mut materials = Vec::with_capacity(root.materials.len());
    for (index, material) in root.materials.iter().enumerate() {
        let authored_shader = WorldModelShader::decode(material.shader).ok_or_else(|| {
            world_model_message(
                path,
                format!(
                    "MOMT material {index} shader {} is outside build-12340 range 0..6",
                    material.shader
                ),
            )
        })?;
        let blend_mode = WorldModelBlendMode::decode(material.blend_mode).ok_or_else(|| {
            world_model_message(
                path,
                format!(
                    "MOMT material {index} blend mode {} is outside build-12340 EGxBlend range 0..10",
                    material.blend_mode
                ),
            )
        })?;
        let offsets = [material.texture_1, material.texture_2, material.texture_3];
        let mut textures = [None, None, None];
        for slot in 0..3 {
            let required = slot == 0 || (slot == 1 && authored_shader.requires_secondary_texture());
            textures[slot] =
                decode_texture_path(path, texture_bytes, offsets[slot], required, index, slot)?;
        }
        // CWmo::FinishLoad at 0x007D7710 changes a two-texture effect with a
        // valid empty second MOTX string to MapObjOpaque. It does not invent a
        // replacement texture, and the authored selector remains observable.
        let shader = if authored_shader.requires_secondary_texture() && textures[1].is_none() {
            WorldModelShader::Opaque
        } else {
            authored_shader
        };
        let runtime_data: [u8; 16] = material.runtime_data.as_slice().try_into().map_err(|_| {
            world_model_message(
                path,
                format!("MOMT material {index} runtime tail is not 16 bytes"),
            )
        })?;
        materials.push(WorldModelMaterial::new(
            material.flags,
            authored_shader,
            shader,
            blend_mode,
            offsets,
            textures,
            u32::from_le_bytes(material.emissive_color),
            u32::from_le_bytes(material.diff_color),
            material.ground_type,
            material.color_2,
            material.flags_2,
            runtime_data,
        ));
    }
    Ok(materials)
}

fn decode_texture_path(
    root_path: &AssetPath,
    texture_bytes: &[u8],
    offset: u32,
    required: bool,
    material_index: usize,
    slot: usize,
) -> Result<Option<AssetPath>, AssetError> {
    let offset = usize::try_from(offset).map_err(|error| world_model_error(root_path, error))?;
    let Some(tail) = texture_bytes.get(offset..) else {
        return if required {
            Err(world_model_message(
                root_path,
                format!(
                    "MOMT material {material_index} texture {slot} references MOTX offset {offset} outside {} bytes",
                    texture_bytes.len()
                ),
            ))
        } else {
            Ok(None)
        };
    };
    let Some(end) = tail.iter().position(|byte| *byte == 0) else {
        return if required {
            Err(world_model_message(
                root_path,
                format!("MOMT material {material_index} texture {slot} is not null terminated"),
            ))
        } else {
            Ok(None)
        };
    };
    if end == 0 {
        return Ok(None);
    }
    let value = std::str::from_utf8(&tail[..end]).map_err(|error| {
        world_model_message(
            root_path,
            format!("MOMT material {material_index} texture {slot} is not UTF-8: {error}"),
        )
    })?;
    AssetPath::new(value).map(Some)
}

pub(super) fn validate_root(
    path: &AssetPath,
    root: &wow_wmo::root_parser::WmoRoot,
) -> Result<(), AssetError> {
    if root.version != BUILD_12340_WMO_VERSION {
        return Err(world_model_message(
            path,
            format!("expected WMO version 17; found {}", root.version),
        ));
    }
    if usize::try_from(root.n_groups).ok() != Some(root.group_info.len())
        || usize::try_from(root.n_materials).ok() != Some(root.materials.len())
        || usize::try_from(root.n_portals).ok() != Some(root.portals.len())
        || usize::try_from(root.n_lights).ok() != Some(root.lights.len())
        || usize::try_from(root.n_doodad_names).ok() != Some(root.doodad_names.len())
        || usize::try_from(root.n_doodad_sets).ok() != Some(root.doodad_sets.len())
    {
        return Err(world_model_message(
            path,
            "MOHD counts disagree with decoded root tables",
        ));
    }
    // Stock roots can retain an exporter-era inflated n_doodad_defs value.
    // The client sizes the placement table from MODD and MODS selects ranges
    // within that decoded table, so MOHD is not authoritative for this field.
    if root.num_lod != 0
        || !root.uv_transforms.is_empty()
        || !root.portal_extras.is_empty()
        || !root.light_extensions.is_empty()
        || !root.doodad_ids.is_empty()
        || !root.new_materials.is_empty()
        || !root.group_file_ids.is_empty()
    {
        return Err(world_model_message(
            path,
            "WMO root contains a post-build-12340 layout",
        ));
    }
    validate_bounds(path, root.bounding_box_min, root.bounding_box_max, "MOHD")?;
    for (index, group) in root.group_info.iter().enumerate() {
        validate_bounds(
            path,
            group.bounding_box_min,
            group.bounding_box_max,
            &format!("MOGI group {index}"),
        )?;
    }
    Ok(())
}

pub(super) fn validate_root_chunk_layout(path: &AssetPath, bytes: &[u8]) -> Result<(), AssetError> {
    const ALLOWED: [[u8; 4]; 18] = [
        *b"REVM", *b"DHOM", *b"XTOM", *b"TMOM", *b"NGOM", *b"IGOM", *b"BSOM", *b"VPOM", *b"TPOM",
        *b"RPOM", *b"VVOM", *b"BVOM", *b"TLOM", *b"SDOM", *b"NDOM", *b"DDOM", *b"GOFM", *b"PVCM",
    ];
    let chunks = scan_chunks(path, bytes, "WMO root")?;
    validate_unique_chunks(path, &chunks, &ALLOWED, "WMO root", &[])?;
    require_chunk(path, &chunks, *b"REVM", "MVER")?;
    require_chunk(path, &chunks, *b"DHOM", "MOHD")?;
    for (magic, size) in [
        (*b"IGOM", 32),
        (*b"VPOM", 12),
        (*b"TPOM", 20),
        (*b"RPOM", 8),
        (*b"PVCM", 16),
    ] {
        if chunks.iter().any(|chunk| {
            chunk.magic == magic && !(chunk.payload_end - chunk.payload_start).is_multiple_of(size)
        }) {
            return Err(world_model_message(
                path,
                format!(
                    "{} requires complete {size}-byte records",
                    chunk_name(magic)
                ),
            ));
        }
    }
    Ok(())
}
