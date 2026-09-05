//! Strict WMO root/group loading through ordinary archive precedence.

use std::io::Cursor;

use wow_wmo::{ParsedWmo, parse_wmo};

use crate::{AssetError, AssetPath, AssetStore};

use super::map_obj::{
    DecodedWorldModel, WorldModelBlendMode, WorldModelMaterial, WorldModelShader,
};
use super::map_obj_group::{
    DecodedWorldModelGroup, WorldModelBatch, WorldModelBatchClass, WorldModelBspNode,
    WorldModelLiquid, WorldModelLiquidVertex, WorldModelPolygon,
};
use super::{WorldModelDoodad, WorldModelDoodadSet};

const BUILD_12340_WMO_VERSION: u32 = 17;

impl DecodedWorldModel {
    /// Loads a WMO root and every numbered group as one admitted generation.
    ///
    /// Each group resolves independently through the mounted MPQ stack. A
    /// same-path HD replacement therefore changes only selected source bytes,
    /// never model identity or the decoding route.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the root/group kind, version, bounds,
    /// triangle tables, BSP references, or build-era layout is invalid.
    pub fn load(store: &mut AssetStore, path: &AssetPath) -> Result<Self, AssetError> {
        let read = store.read(path)?;
        validate_root_chunk_layout(path, read.bytes())?;
        let parsed = parse_wmo(&mut Cursor::new(read.bytes()))
            .map_err(|error| world_model_error(path, error))?;
        let ParsedWmo::Root(root) = parsed else {
            return Err(world_model_message(path, "expected a WMO root file"));
        };
        validate_root(path, &root)?;
        let bounds = validate_bounds(path, root.bounding_box_min, root.bounding_box_max, "MOHD")?;
        let materials = decode_materials(path, read.bytes(), &root)?;
        let (doodad_sets, doodads) = decode_doodads(path, read.bytes(), &root)?;
        if root.flags & 0x02 == 0
            && materials
                .iter()
                .any(|material| material.shader() == WorldModelShader::Composite)
        {
            return Err(world_model_message(
                path,
                "MOMT shader 6 selects stock's null ordinary MapObj effect",
            ));
        }
        let group_count =
            usize::try_from(root.n_groups).map_err(|error| world_model_error(path, error))?;
        let mut groups = Vec::with_capacity(group_count);
        for index in 0..root.n_groups {
            groups.push(load_group(
                store,
                path,
                index,
                root.materials.len(),
                root.lights.len(),
                root.doodad_defs.len(),
            )?);
        }
        Ok(Self::new(
            path.clone(),
            read.source().clone(),
            root.flags,
            root.ambient_color,
            root.wmo_id,
            bounds,
            root.group_info
                .iter()
                .map(|group| [group.bounding_box_min, group.bounding_box_max])
                .collect(),
            materials,
            doodad_sets,
            doodads,
            groups,
        ))
    }
}

fn decode_doodads(
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

fn load_group(
    store: &mut AssetStore,
    root_path: &AssetPath,
    index: u32,
    material_count: usize,
    light_count: usize,
    doodad_count: usize,
) -> Result<DecodedWorldModelGroup, AssetError> {
    let group_path = group_path(root_path, index)?;
    let read = store.read(&group_path)?;
    let references = decode_group_references(&group_path, read.bytes())?;
    let liquid = decode_group_liquid(&group_path, read.bytes())?;
    let parsed = parse_wmo(&mut Cursor::new(read.bytes()))
        .map_err(|error| world_model_error(&group_path, error))?;
    let ParsedWmo::Group(mut group) = parsed else {
        return Err(world_model_message(
            &group_path,
            "expected a WMO group file",
        ));
    };
    // wow-wmo 0.7's nested MOGP parser omits MOLR/MODR (its top-level
    // parser handles them). Recover the exact bounded arrays before validating
    // their root-table membership; an omitted list must not look like no refs.
    group.light_refs = references.lights;
    group.doodad_refs = references.doodads;
    validate_group(
        &group_path,
        &group,
        material_count,
        light_count,
        doodad_count,
    )?;
    let bounds = validate_bounds(
        &group_path,
        [
            group.bounding_box[0],
            group.bounding_box[1],
            group.bounding_box[2],
        ],
        [
            group.bounding_box[3],
            group.bounding_box[4],
            group.bounding_box[5],
        ],
        "MOGP",
    )?;
    let vertices = group
        .vertex_positions
        .iter()
        .map(|vertex| [vertex.x, vertex.y, vertex.z])
        .collect();
    let normals = group
        .vertex_normals
        .iter()
        .map(|normal| [normal.x, normal.y, normal.z])
        .collect();
    let presentation = decode_group_presentation(
        &group_path,
        read.bytes(),
        group.vertex_positions.len(),
        group.vertex_indices.len(),
        material_count,
        group.trans_batch_count,
        group.int_batch_count,
    )?;
    let polygons = group
        .material_info
        .iter()
        .map(|polygon| WorldModelPolygon::new(polygon.flags, polygon.material_id))
        .collect();
    let bsp_nodes = group
        .bsp_nodes
        .iter()
        .map(|node| {
            WorldModelBspNode::new(
                node.flags,
                node.neg_child,
                node.pos_child,
                node.n_faces,
                node.face_start,
                node.plane_distance,
            )
        })
        .collect();
    let fog_ids = group
        .fog_ids
        .as_slice()
        .try_into()
        .map_err(|_| world_model_message(&group_path, "MOGP requires four fog IDs"))?;
    Ok(DecodedWorldModelGroup::new(
        index,
        group_path,
        read.source().clone(),
        group.flags,
        bounds,
        group.portal_start,
        group.portal_count,
        group.trans_batch_count,
        group.int_batch_count,
        group.ext_batch_count,
        group.batch_type_d,
        fog_ids,
        group.group_liquid,
        group.area_table_id,
        liquid,
        vertices,
        normals,
        presentation.texture_coordinates,
        presentation.vertex_colors,
        group.vertex_indices,
        polygons,
        presentation.batches,
        group.light_refs,
        group.doodad_refs,
        bsp_nodes,
        group.bsp_face_indices,
    ))
}

fn decode_materials(
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

struct GroupPresentation {
    texture_coordinates: Vec<Vec<[f32; 2]>>,
    vertex_colors: Vec<Vec<[u8; 4]>>,
    batches: Vec<WorldModelBatch>,
}

fn decode_group_presentation(
    path: &AssetPath,
    bytes: &[u8],
    vertex_count: usize,
    index_count: usize,
    material_count: usize,
    transition_batch_count: u16,
    interior_batch_count: u16,
) -> Result<GroupPresentation, AssetError> {
    let outer = scan_chunks(path, bytes, "WMO group")?;
    let container = require_chunk(path, &outer, *b"PGOM", "MOGP")?;
    let payload = bytes
        .get(container.payload_start..container.payload_end)
        .ok_or_else(|| world_model_message(path, "MOGP payload exceeds its group file"))?;
    let nested_bytes = payload
        .get(68..)
        .ok_or_else(|| world_model_message(path, "MOGP is smaller than its 68-byte header"))?;
    let chunks = scan_chunks(path, nested_bytes, "MOGP")?;
    let mut texture_coordinates = Vec::new();
    let mut vertex_colors = Vec::new();
    let mut batches = Vec::new();
    for chunk in chunks {
        let data = nested_bytes
            .get(chunk.payload_start..chunk.payload_end)
            .ok_or_else(|| world_model_message(path, "nested WMO chunk exceeds MOGP"))?;
        match chunk.magic {
            magic if magic == *b"VTOM" => {
                if texture_coordinates.len() >= 3 {
                    return Err(world_model_message(
                        path,
                        "MOGP contains more than three MOTV layers",
                    ));
                }
                if !data.len().is_multiple_of(8) || data.len() / 8 != vertex_count {
                    return Err(world_model_message(
                        path,
                        "each MOTV layer must contain one Vec2 per MOVT vertex",
                    ));
                }
                let mut layer = Vec::with_capacity(vertex_count);
                for offset in (0..data.len()).step_by(8) {
                    layer.push([
                        read_f32(path, data, offset, "MOTV U")?,
                        read_f32(path, data, offset + 4, "MOTV V")?,
                    ]);
                }
                texture_coordinates.push(layer);
            }
            magic if magic == *b"VCOM" => {
                if vertex_colors.len() >= 2 {
                    return Err(world_model_message(
                        path,
                        "MOGP contains more than two MOCV layers",
                    ));
                }
                if !data.len().is_multiple_of(4) || data.len() / 4 != vertex_count {
                    return Err(world_model_message(
                        path,
                        "each MOCV layer must contain one BGRA color per MOVT vertex",
                    ));
                }
                vertex_colors.push(data.as_chunks::<4>().0.to_vec());
            }
            magic if magic == *b"ABOM" => {
                if !data.len().is_multiple_of(24) {
                    return Err(world_model_message(
                        path,
                        "MOBA size is not a whole number of 24-byte records",
                    ));
                }
                for record in data.as_chunks::<24>().0 {
                    let bounds = [
                        [
                            i16::from_le_bytes(
                                record[0..2]
                                    .try_into()
                                    .map_err(|error| world_model_error(path, error))?,
                            ),
                            i16::from_le_bytes(
                                record[2..4]
                                    .try_into()
                                    .map_err(|error| world_model_error(path, error))?,
                            ),
                            i16::from_le_bytes(
                                record[4..6]
                                    .try_into()
                                    .map_err(|error| world_model_error(path, error))?,
                            ),
                        ],
                        [
                            i16::from_le_bytes(
                                record[6..8]
                                    .try_into()
                                    .map_err(|error| world_model_error(path, error))?,
                            ),
                            i16::from_le_bytes(
                                record[8..10]
                                    .try_into()
                                    .map_err(|error| world_model_error(path, error))?,
                            ),
                            i16::from_le_bytes(
                                record[10..12]
                                    .try_into()
                                    .map_err(|error| world_model_error(path, error))?,
                            ),
                        ],
                    ];
                    let first_index = u32::from_le_bytes(
                        record[12..16]
                            .try_into()
                            .map_err(|error| world_model_error(path, error))?,
                    );
                    let batch_index_count = u16::from_le_bytes(
                        record[16..18]
                            .try_into()
                            .map_err(|error| world_model_error(path, error))?,
                    );
                    let first_vertex = u16::from_le_bytes(
                        record[18..20]
                            .try_into()
                            .map_err(|error| world_model_error(path, error))?,
                    );
                    let last_vertex = u16::from_le_bytes(
                        record[20..22]
                            .try_into()
                            .map_err(|error| world_model_error(path, error))?,
                    );
                    let material_id = record[23];
                    let first = usize::try_from(first_index)
                        .map_err(|error| world_model_error(path, error))?;
                    let end = first
                        .checked_add(usize::from(batch_index_count))
                        .ok_or_else(|| world_model_message(path, "MOBA index range overflows"))?;
                    if end > index_count
                        || first_vertex > last_vertex
                        || usize::from(last_vertex) >= vertex_count
                    {
                        return Err(world_model_message(
                            path,
                            "MOBA references geometry outside MOVI/MOVT",
                        ));
                    }
                    if bounds[0]
                        .iter()
                        .zip(bounds[1])
                        .any(|(minimum, maximum)| *minimum > maximum)
                    {
                        return Err(world_model_message(
                            path,
                            "MOBA has inverted culling bounds",
                        ));
                    }
                    if usize::from(material_id) >= material_count {
                        return Err(world_model_message(
                            path,
                            "MOBA references a material outside MOMT",
                        ));
                    }
                    let index = batches.len();
                    let transition_end = usize::from(transition_batch_count);
                    let interior_end = transition_end + usize::from(interior_batch_count);
                    let class = if index < transition_end {
                        WorldModelBatchClass::Transition
                    } else if index < interior_end {
                        WorldModelBatchClass::Interior
                    } else {
                        WorldModelBatchClass::Exterior
                    };
                    batches.push(WorldModelBatch::new(
                        bounds,
                        first_index,
                        batch_index_count,
                        first_vertex,
                        last_vertex,
                        record[22],
                        material_id,
                        class,
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(GroupPresentation {
        texture_coordinates,
        vertex_colors,
        batches,
    })
}

fn decode_group_liquid(
    path: &AssetPath,
    bytes: &[u8],
) -> Result<Option<WorldModelLiquid>, AssetError> {
    let outer = scan_chunks(path, bytes, "WMO group")?;
    let container = require_chunk(path, &outer, *b"PGOM", "MOGP")?;
    let payload = bytes
        .get(container.payload_start..container.payload_end)
        .ok_or_else(|| world_model_message(path, "MOGP payload exceeds its group file"))?;
    let nested_bytes = payload
        .get(68..)
        .ok_or_else(|| world_model_message(path, "MOGP is smaller than its 68-byte header"))?;
    let nested = scan_chunks(path, nested_bytes, "MOGP")?;
    let Some(chunk) = nested.iter().find(|chunk| chunk.magic == *b"QILM") else {
        return Ok(None);
    };
    let liquid = nested_bytes
        .get(chunk.payload_start..chunk.payload_end)
        .ok_or_else(|| world_model_message(path, "MLIQ payload exceeds MOGP"))?;
    if liquid.len() < 30 {
        return Err(world_model_message(path, "MLIQ is smaller than 30 bytes"));
    }

    let vertex_width = read_u32(path, liquid, 0, "MLIQ vertex width")?;
    let vertex_height = read_u32(path, liquid, 4, "MLIQ vertex height")?;
    let tile_width = read_u32(path, liquid, 8, "MLIQ tile width")?;
    let tile_height = read_u32(path, liquid, 12, "MLIQ tile height")?;
    let expected_vertex_width = tile_width
        .checked_add(1)
        .ok_or_else(|| world_model_message(path, "MLIQ tile width overflows"))?;
    let expected_vertex_height = tile_height
        .checked_add(1)
        .ok_or_else(|| world_model_message(path, "MLIQ tile height overflows"))?;
    if vertex_width != expected_vertex_width || vertex_height != expected_vertex_height {
        return Err(world_model_message(
            path,
            "MLIQ vertex dimensions must be one larger than tile dimensions",
        ));
    }
    let corner = [
        read_f32(path, liquid, 16, "MLIQ corner X")?,
        read_f32(path, liquid, 20, "MLIQ corner Y")?,
        read_f32(path, liquid, 24, "MLIQ corner Z")?,
    ];
    if corner.into_iter().any(|value| !value.is_finite()) {
        return Err(world_model_message(path, "MLIQ corner is not finite"));
    }
    let material_id = read_u16(path, liquid, 28, "MLIQ material")?;
    let vertex_count = usize::try_from(vertex_width)
        .ok()
        .and_then(|width| {
            usize::try_from(vertex_height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| world_model_message(path, "MLIQ vertex count overflows"))?;
    let tile_count = usize::try_from(tile_width)
        .ok()
        .and_then(|width| {
            usize::try_from(tile_height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| world_model_message(path, "MLIQ tile count overflows"))?;
    let vertex_bytes = vertex_count
        .checked_mul(8)
        .ok_or_else(|| world_model_message(path, "MLIQ vertex extent overflows"))?;
    let tile_start = 30_usize
        .checked_add(vertex_bytes)
        .ok_or_else(|| world_model_message(path, "MLIQ vertex extent overflows"))?;
    let expected = tile_start
        .checked_add(tile_count)
        .ok_or_else(|| world_model_message(path, "MLIQ tile extent overflows"))?;
    if liquid.len() != expected {
        return Err(world_model_message(
            path,
            format!(
                "MLIQ dimensions require {expected} bytes; found {}",
                liquid.len()
            ),
        ));
    }

    let mut vertices = Vec::with_capacity(vertex_count);
    for index in 0..vertex_count {
        let offset = 30 + index * 8;
        let overloaded: [u8; 4] = liquid[offset..offset + 4]
            .try_into()
            .map_err(|error| world_model_error(path, error))?;
        let height = read_f32(path, liquid, offset + 4, "MLIQ vertex height")?;
        if !height.is_finite() {
            return Err(world_model_message(
                path,
                format!("MLIQ vertex {index} height is not finite"),
            ));
        }
        vertices.push(WorldModelLiquidVertex::new(overloaded, height));
    }
    let tiles = liquid[tile_start..].to_vec();
    Ok(Some(WorldModelLiquid::new(
        vertex_width,
        vertex_height,
        tile_width,
        tile_height,
        corner,
        material_id,
        vertices,
        tiles,
    )))
}

fn read_u16(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<u16, AssetError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| world_model_message(path, format!("{field} is truncated")))?;
    Ok(u16::from_le_bytes(
        value
            .try_into()
            .map_err(|error| world_model_error(path, error))?,
    ))
}

fn read_u32(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<u32, AssetError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| world_model_message(path, format!("{field} is truncated")))?;
    Ok(u32::from_le_bytes(
        value
            .try_into()
            .map_err(|error| world_model_error(path, error))?,
    ))
}

fn read_f32(path: &AssetPath, bytes: &[u8], offset: usize, field: &str) -> Result<f32, AssetError> {
    Ok(f32::from_bits(read_u32(path, bytes, offset, field)?))
}

fn validate_root(path: &AssetPath, root: &wow_wmo::root_parser::WmoRoot) -> Result<(), AssetError> {
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
        || !root.convex_volume_planes.is_empty()
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

fn validate_group(
    path: &AssetPath,
    group: &wow_wmo::group_parser::WmoGroup,
    material_count: usize,
    light_count: usize,
    doodad_count: usize,
) -> Result<(), AssetError> {
    if group.version != BUILD_12340_WMO_VERSION {
        return Err(world_model_message(
            path,
            format!("expected WMO version 17; found {}", group.version),
        ));
    }
    if group.bounding_box.len() != 6 {
        return Err(world_model_message(path, "MOGP bounds require six floats"));
    }
    if group.flags2 != 0
        || group.parent_split_group != 0
        || group.next_split_child != 0
        || group.query_face_start.is_some()
        || !group.extended_materials.is_empty()
        || !group.extended_vertex_indices.is_empty()
        || !group.query_faces.is_empty()
        || !group.triangle_strip_indices.is_empty()
        || !group.additional_render_batches.is_empty()
        || !group.tangent_arrays.is_empty()
        || !group.shadow_batches.is_empty()
    {
        return Err(world_model_message(
            path,
            "WMO group contains a post-build-12340 layout",
        ));
    }
    if !group.vertex_indices.len().is_multiple_of(3)
        || group.material_info.len() != group.vertex_indices.len() / 3
    {
        return Err(world_model_message(
            path,
            "MOPY must contain one entry per MOVI triangle",
        ));
    }
    if group
        .vertex_positions
        .iter()
        .any(|vertex| !vertex.x.is_finite() || !vertex.y.is_finite() || !vertex.z.is_finite())
        || group
            .vertex_normals
            .iter()
            .any(|normal| !normal.x.is_finite() || !normal.y.is_finite() || !normal.z.is_finite())
    {
        return Err(world_model_message(
            path,
            "MOVT or MONR contains a non-finite vector",
        ));
    }
    if group
        .light_refs
        .iter()
        .any(|reference| usize::from(*reference) >= light_count)
    {
        return Err(world_model_message(
            path,
            "MOLR references a light outside MOLT",
        ));
    }
    if group
        .doodad_refs
        .iter()
        .any(|reference| usize::from(*reference) >= doodad_count)
    {
        return Err(world_model_message(
            path,
            "MODR references a doodad outside MODD",
        ));
    }
    if group.vertex_normals.len() != group.vertex_positions.len() {
        return Err(world_model_message(
            path,
            "MONR must contain one normal per MOVT vertex",
        ));
    }
    if group
        .vertex_indices
        .iter()
        .any(|index| usize::from(*index) >= group.vertex_positions.len())
    {
        return Err(world_model_message(
            path,
            "MOVI references a vertex outside MOVT",
        ));
    }
    if group.material_info.iter().any(|polygon| {
        polygon.material_id != 0xff && usize::from(polygon.material_id) >= material_count
    }) {
        return Err(world_model_message(
            path,
            "MOPY references a material outside MOMT",
        ));
    }
    if group
        .bsp_face_indices
        .iter()
        .any(|face| usize::from(*face) >= group.material_info.len())
    {
        return Err(world_model_message(
            path,
            "MOBR references a face outside MOPY",
        ));
    }
    for (index, node) in group.bsp_nodes.iter().enumerate() {
        if !node.plane_distance.is_finite() {
            return Err(world_model_message(
                path,
                format!("MOBN node {index} has a non-finite plane"),
            ));
        }
        let first =
            usize::try_from(node.face_start).map_err(|error| world_model_error(path, error))?;
        let end = first
            .checked_add(usize::from(node.n_faces))
            .ok_or_else(|| {
                world_model_message(path, format!("MOBN node {index} face range overflows"))
            })?;
        if end > group.bsp_face_indices.len() {
            return Err(world_model_message(
                path,
                format!("MOBN node {index} face range exceeds MOBR"),
            ));
        }
        for child in [node.neg_child, node.pos_child] {
            if child >= 0
                && usize::try_from(child)
                    .ok()
                    .is_none_or(|child| child >= group.bsp_nodes.len())
            {
                return Err(world_model_message(
                    path,
                    format!("MOBN node {index} references a missing child"),
                ));
            }
        }
    }
    Ok(())
}

fn validate_root_chunk_layout(path: &AssetPath, bytes: &[u8]) -> Result<(), AssetError> {
    const ALLOWED: [[u8; 4]; 17] = [
        *b"REVM", *b"DHOM", *b"XTOM", *b"TMOM", *b"NGOM", *b"IGOM", *b"BSOM", *b"VPOM", *b"TPOM",
        *b"RPOM", *b"VVOM", *b"BVOM", *b"TLOM", *b"SDOM", *b"NDOM", *b"DDOM", *b"GOFM",
    ];
    let chunks = scan_chunks(path, bytes, "WMO root")?;
    validate_unique_chunks(path, &chunks, &ALLOWED, "WMO root", &[])?;
    require_chunk(path, &chunks, *b"REVM", "MVER")?;
    require_chunk(path, &chunks, *b"DHOM", "MOHD")?;
    Ok(())
}

struct GroupReferences {
    lights: Vec<u16>,
    doodads: Vec<u16>,
}

fn decode_group_references(path: &AssetPath, bytes: &[u8]) -> Result<GroupReferences, AssetError> {
    const OUTER: [[u8; 4]; 2] = [*b"REVM", *b"PGOM"];
    const NESTED: [[u8; 4]; 12] = [
        *b"YPOM", *b"IVOM", *b"TVOM", *b"RNOM", *b"VTOM", *b"ABOM", *b"RLOM", *b"RDOM", *b"NBOM",
        *b"RBOM", *b"VCOM", *b"QILM",
    ];
    let chunks = scan_chunks(path, bytes, "WMO group")?;
    validate_unique_chunks(path, &chunks, &OUTER, "WMO group", &[])?;
    require_chunk(path, &chunks, *b"REVM", "MVER")?;
    let container = require_chunk(path, &chunks, *b"PGOM", "MOGP")?;
    let payload = bytes
        .get(container.payload_start..container.payload_end)
        .ok_or_else(|| world_model_message(path, "MOGP payload exceeds its group file"))?;
    let nested_bytes = payload
        .get(68..)
        .ok_or_else(|| world_model_message(path, "MOGP is smaller than its 68-byte header"))?;
    let nested = scan_chunks(path, nested_bytes, "MOGP")?;
    validate_unique_chunks(path, &nested, &NESTED, "MOGP", &[*b"VTOM", *b"VCOM"])?;
    let decode = |magic, name| -> Result<Vec<u16>, AssetError> {
        let Some(chunk) = nested.iter().find(|chunk| chunk.magic == magic) else {
            return Ok(Vec::new());
        };
        let data = &nested_bytes[chunk.payload_start..chunk.payload_end];
        let (words, remainder) = data.as_chunks::<2>();
        if !remainder.is_empty() {
            return Err(world_model_message(
                path,
                format!("{name} requires complete u16 references"),
            ));
        }
        Ok(words.iter().map(|word| u16::from_le_bytes(*word)).collect())
    };
    Ok(GroupReferences {
        lights: decode(*b"RLOM", "MOLR")?,
        doodads: decode(*b"RDOM", "MODR")?,
    })
}

#[derive(Clone, Copy)]
struct ChunkSpan {
    magic: [u8; 4],
    payload_start: usize,
    payload_end: usize,
}

fn scan_chunks(
    path: &AssetPath,
    bytes: &[u8],
    container: &str,
) -> Result<Vec<ChunkSpan>, AssetError> {
    let mut chunks = Vec::new();
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let header_end = offset.checked_add(8).ok_or_else(|| {
            world_model_message(path, format!("{container} chunk header offset overflows"))
        })?;
        let header = bytes.get(offset..header_end).ok_or_else(|| {
            world_model_message(path, format!("{container} ends inside a chunk header"))
        })?;
        let magic: [u8; 4] = header[0..4].try_into().map_err(|error| {
            world_model_error(
                path,
                format!("{container} chunk magic is truncated: {error}"),
            )
        })?;
        let size = usize::try_from(u32::from_le_bytes(header[4..8].try_into().map_err(
            |error| {
                world_model_error(
                    path,
                    format!("{container} chunk size is truncated: {error}"),
                )
            },
        )?))
        .map_err(|error| world_model_error(path, error))?;
        let payload_end = header_end.checked_add(size).ok_or_else(|| {
            world_model_message(path, format!("{container} chunk extent overflows"))
        })?;
        if payload_end > bytes.len() {
            return Err(world_model_message(
                path,
                format!("{container} chunk exceeds its containing payload"),
            ));
        }
        chunks.push(ChunkSpan {
            magic,
            payload_start: header_end,
            payload_end,
        });
        offset = payload_end;
    }
    Ok(chunks)
}

fn validate_unique_chunks(
    path: &AssetPath,
    chunks: &[ChunkSpan],
    allowed: &[[u8; 4]],
    container: &str,
    repeatable: &[[u8; 4]],
) -> Result<(), AssetError> {
    let mut seen = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        if !allowed.contains(&chunk.magic) {
            return Err(world_model_message(
                path,
                format!(
                    "{container} contains unknown build-12340 chunk {}",
                    chunk_name(chunk.magic)
                ),
            ));
        }
        if seen.contains(&chunk.magic) && !repeatable.contains(&chunk.magic) {
            return Err(world_model_message(
                path,
                format!("{container} repeats chunk {}", chunk_name(chunk.magic)),
            ));
        }
        seen.push(chunk.magic);
    }
    Ok(())
}

fn require_chunk<'a>(
    path: &AssetPath,
    chunks: &'a [ChunkSpan],
    magic: [u8; 4],
    name: &str,
) -> Result<&'a ChunkSpan, AssetError> {
    chunks
        .iter()
        .find(|chunk| chunk.magic == magic)
        .ok_or_else(|| world_model_message(path, format!("WMO omits required {name}")))
}

fn chunk_name(mut magic: [u8; 4]) -> String {
    magic.reverse();
    String::from_utf8_lossy(&magic).into_owned()
}

fn validate_bounds(
    path: &AssetPath,
    minimum: [f32; 3],
    maximum: [f32; 3],
    table: &str,
) -> Result<[[f32; 3]; 2], AssetError> {
    if minimum
        .into_iter()
        .chain(maximum)
        .any(|value| !value.is_finite())
        || (0..3).any(|axis| minimum[axis] > maximum[axis])
    {
        return Err(world_model_message(
            path,
            format!("{table} has invalid bounds"),
        ));
    }
    Ok([minimum, maximum])
}

fn group_path(root: &AssetPath, index: u32) -> Result<AssetPath, AssetError> {
    let Some(stem) = root.as_str().strip_suffix(".WMO") else {
        return Err(world_model_message(root, "root path does not end in .WMO"));
    };
    AssetPath::new(format!("{stem}_{index:03}.WMO"))
}

fn world_model_error(path: &AssetPath, error: impl std::fmt::Display) -> AssetError {
    world_model_message(path, error.to_string())
}

fn world_model_message(path: &AssetPath, message: impl Into<String>) -> AssetError {
    AssetError::WorldModelDecode {
        path: path.clone(),
        message: message.into(),
    }
}
