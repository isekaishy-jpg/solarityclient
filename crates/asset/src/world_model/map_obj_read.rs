//! Strict WMO root/group loading through ordinary archive precedence.

use std::io::Cursor;

use wow_wmo::{ParsedWmo, parse_wmo};

use crate::{AssetError, AssetPath, AssetStore};

use super::map_obj::DecodedWorldModel;
use super::map_obj_group::{
    DecodedWorldModelGroup, WorldModelBspNode, WorldModelLiquid, WorldModelLiquidVertex,
    WorldModelPolygon,
};

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
        let group_count =
            usize::try_from(root.n_groups).map_err(|error| world_model_error(path, error))?;
        let mut groups = Vec::with_capacity(group_count);
        for index in 0..root.n_groups {
            groups.push(load_group(store, path, index, root.materials.len())?);
        }
        Ok(Self::new(
            path.clone(),
            read.source().clone(),
            root.flags,
            root.wmo_id,
            bounds,
            groups,
        ))
    }
}

fn load_group(
    store: &mut AssetStore,
    root_path: &AssetPath,
    index: u32,
    material_count: usize,
) -> Result<DecodedWorldModelGroup, AssetError> {
    let group_path = group_path(root_path, index)?;
    let read = store.read(&group_path)?;
    validate_group_chunk_layout(&group_path, read.bytes())?;
    let liquid = decode_group_liquid(&group_path, read.bytes())?;
    let parsed = parse_wmo(&mut Cursor::new(read.bytes()))
        .map_err(|error| world_model_error(&group_path, error))?;
    let ParsedWmo::Group(group) = parsed else {
        return Err(world_model_message(
            &group_path,
            "expected a WMO group file",
        ));
    };
    validate_group(&group_path, &group, material_count)?;
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
    Ok(DecodedWorldModelGroup::new(
        index,
        group_path,
        read.source().clone(),
        group.flags,
        bounds,
        group.group_liquid,
        liquid,
        vertices,
        group.vertex_indices,
        polygons,
        bsp_nodes,
        group.bsp_face_indices,
    ))
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
        || usize::try_from(root.n_doodad_defs).ok() != Some(root.doodad_defs.len())
        || usize::try_from(root.n_doodad_sets).ok() != Some(root.doodad_sets.len())
    {
        return Err(world_model_message(
            path,
            "MOHD counts disagree with decoded root tables",
        ));
    }
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

fn validate_group_chunk_layout(path: &AssetPath, bytes: &[u8]) -> Result<(), AssetError> {
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
    validate_unique_chunks(path, &nested, &NESTED, "MOGP", &[*b"VTOM", *b"VCOM"])
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
