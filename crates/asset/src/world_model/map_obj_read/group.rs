//! One numbered WMO group's geometry, references, liquid and spatial validation.
use super::super::map_obj_group::{
    DecodedWorldModelGroup, WorldModelBatch, WorldModelBatchClass, WorldModelBspNode,
    WorldModelLiquid, WorldModelLiquidVertex, WorldModelPolygon,
};
use super::BUILD_12340_WMO_VERSION;
use super::layout::{
    read_f32, read_u16, read_u32, require_chunk, scan_chunks, validate_bounds,
    validate_unique_chunks, world_model_error, world_model_message,
};
use crate::{AssetError, AssetPath, AssetStore};
use std::io::Cursor;
use wow_wmo::{ParsedWmo, parse_wmo};

pub(super) fn load_group(
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

fn group_path(root: &AssetPath, index: u32) -> Result<AssetPath, AssetError> {
    let Some(stem) = root.as_str().strip_suffix(".WMO") else {
        return Err(world_model_message(root, "root path does not end in .WMO"));
    };
    AssetPath::new(format!("{stem}_{index:03}.WMO"))
}
