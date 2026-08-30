//! Strict build-12340 WDT loading through the mounted file stack.

use std::io::Cursor;

use wow_adt::{AdtVersion, ParsedAdt, RootAdt, parse_adt};
use wow_wdt::version::WowVersion;
use wow_wdt::{WdtFile, WdtReader};

use crate::archive::{AssetError, AssetPath};
use crate::database::MapDefinition;
use crate::file_stack::AssetStore;

use super::alpha_map::decode_alpha_map;
use super::map::{DecodedTerrainTile, TerrainMap, TerrainTextureTable};
use super::map_area::{TerrainTile, TerrainTileIndex};
use super::map_chunk::{
    TERRAIN_CHUNK_VERTEX_COUNT, TerrainChunk, TerrainChunkIndex, TerrainDoodadPlacement,
    TerrainSoundEmitter, TerrainTextureLayer, TerrainWorldModelPlacement,
};

const CLIENT_MAP_ORIGIN: f32 = 32.0 * 533.333_3;
const WDT_HAS_BIG_ALPHA: u32 = 0x0004;

impl TerrainMap {
    /// Loads and validates the selected map's exact WDT manifest.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when resolution, WDT parsing, build-era layout,
    /// global-WMO structure, or map directory paths are invalid.
    pub fn load(store: &mut AssetStore, definition: &MapDefinition) -> Result<Self, AssetError> {
        let path = Self::wdt_path(definition)?;
        let read = store.read(&path)?;
        validate_chunk_stream(&path, read.bytes())?;
        let mut reader = WdtReader::new(Cursor::new(read.bytes()), WowVersion::WotLK);
        let wdt = reader.read().map_err(|error| terrain_error(&path, error))?;
        validate_build_layout(&path, &wdt)?;
        let global_world_model = global_world_model(&path, &wdt)?;
        let mut tiles = Vec::with_capacity(64 * 64);
        for y in 0_u8..64 {
            for x in 0_u8..64 {
                let info = wdt
                    .get_tile(usize::from(x), usize::from(y))
                    .ok_or_else(|| AssetError::TerrainDecode {
                        path: path.clone(),
                        message: format!("WDT MAIN omits tile [{x}, {y}]"),
                    })?;
                let index =
                    TerrainTileIndex::new(x, y).ok_or_else(|| AssetError::TerrainDecode {
                        path: path.clone(),
                        message: format!("WDT MAIN has invalid tile [{x}, {y}]"),
                    })?;
                tiles.push(TerrainTile::new(index, info.flags, info.area_id));
            }
        }
        TerrainMap::new(
            definition,
            read.source().clone(),
            wdt.mphd.flags.bits(),
            global_world_model,
            tiles,
        )
    }

    /// Decodes one ADT declared by this map's WDT.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the tile is absent, resolves incorrectly, is
    /// not a WotLK root ADT, or violates chunk, material, or placement bounds.
    pub fn load_tile(
        &self,
        store: &mut AssetStore,
        index: TerrainTileIndex,
    ) -> Result<DecodedTerrainTile, AssetError> {
        let path = self.adt_path(index)?;
        if !self.tile(index).exists() {
            return Err(terrain_message(
                &path,
                "WDT does not declare an ADT at this tile",
            ));
        }
        let read = store.read(&path)?;
        let parsed = parse_adt(&mut Cursor::new(read.bytes()))
            .map_err(|error| terrain_error(&path, error))?;
        let ParsedAdt::Root(root) = parsed else {
            return Err(terrain_message(
                &path,
                "build-12340 terrain requires a monolithic root ADT",
            ));
        };
        decode_adt(
            index,
            read.source().clone(),
            *root,
            self.flags() & WDT_HAS_BIG_ALPHA != 0,
            &path,
            decode_texture_flags(read.bytes(), &path)?,
        )
    }
}

fn decode_adt(
    index: TerrainTileIndex,
    source: crate::archive::ArchiveDescriptor,
    root: RootAdt,
    big_alpha: bool,
    path: &AssetPath,
    texture_flags: Option<Vec<u32>>,
) -> Result<DecodedTerrainTile, AssetError> {
    if root.version != AdtVersion::WotLK {
        return Err(terrain_message(
            path,
            format!("expected WotLK ADT; decoder identified {:?}", root.version),
        ));
    }
    validate_string_offsets(path, "MMID", &root.models, &root.model_indices)?;
    validate_string_offsets(path, "MWID", &root.wmos, &root.wmo_indices)?;
    let textures = root
        .textures
        .iter()
        .map(|texture| terrain_asset_path(path, "terrain texture", texture))
        .collect::<Result<Vec<_>, _>>()?;
    let doodads = decode_doodads(path, &root)?;
    let world_models = decode_world_models(path, &root)?;
    if let Some(flags) = &texture_flags
        && flags.len() != textures.len()
    {
        return Err(terrain_message(
            path,
            format!(
                "MTXF has {} words for {} MTEX entries",
                flags.len(),
                textures.len()
            ),
        ));
    }
    let chunks = decode_chunks(
        path,
        root.mcnk_chunks,
        textures.len(),
        doodads.len(),
        world_models.len(),
        big_alpha,
    )?;
    Ok(DecodedTerrainTile::new(
        index,
        source,
        TerrainTextureTable {
            paths: textures,
            flags: texture_flags,
        },
        chunks,
        doodads,
        world_models,
        root.water_data.is_some(),
    ))
}

/// Reads MTXF within its declared top-level chunk extent.
///
/// `wow-adt` 0.7 parses the variable-length MTXF body until the input cursor's
/// end rather than the enclosing chunk end. Keep the dependency for the rest
/// of the ADT but recover these parallel words from the original byte stream.
fn decode_texture_flags(bytes: &[u8], path: &AssetPath) -> Result<Option<Vec<u32>>, AssetError> {
    let mut offset = 0_usize;
    let mut flags = None;
    while offset < bytes.len() {
        let header_end = offset
            .checked_add(8)
            .ok_or_else(|| terrain_message(path, "ADT chunk header offset overflow"))?;
        let header = bytes
            .get(offset..header_end)
            .ok_or_else(|| terrain_message(path, "ADT ends inside a top-level chunk header"))?;
        let magic = &header[..4];
        let size = u32::from_le_bytes(
            header[4..8]
                .try_into()
                .map_err(|_| terrain_message(path, "ADT chunk size is truncated"))?,
        ) as usize;
        let chunk_end = header_end
            .checked_add(size)
            .ok_or_else(|| terrain_message(path, "ADT chunk extent overflow"))?;
        let payload = bytes
            .get(header_end..chunk_end)
            .ok_or_else(|| terrain_message(path, "ADT top-level chunk exceeds file size"))?;
        if magic == b"FXTM" {
            if flags.is_some() {
                return Err(terrain_message(path, "ADT repeats top-level MTXF"));
            }
            if payload.len() % size_of::<u32>() != 0 {
                return Err(terrain_message(
                    path,
                    "MTXF size is not a whole number of words",
                ));
            }
            flags = Some(
                payload
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
                    .collect(),
            );
        }
        offset = chunk_end;
    }
    Ok(flags)
}

fn decode_chunks(
    path: &AssetPath,
    source: Vec<wow_adt::McnkChunk>,
    texture_count: usize,
    doodad_count: usize,
    world_model_count: usize,
    big_alpha: bool,
) -> Result<Vec<TerrainChunk>, AssetError> {
    if source.len() != 256 {
        return Err(terrain_message(
            path,
            format!("root ADT requires 256 MCNK chunks; found {}", source.len()),
        ));
    }
    let mut chunks = source
        .into_iter()
        .map(|chunk| {
            decode_chunk(
                path,
                chunk,
                texture_count,
                doodad_count,
                world_model_count,
                big_alpha,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    chunks.sort_unstable_by_key(|chunk| (chunk.index().y(), chunk.index().x()));
    for (expected, chunk) in chunks.iter().enumerate() {
        let actual = usize::from(chunk.index().y()) * 16 + usize::from(chunk.index().x());
        if actual != expected {
            return Err(terrain_message(
                path,
                format!("MCNK grid duplicates or omits row-major index {expected}"),
            ));
        }
    }
    Ok(chunks)
}

fn decode_chunk(
    path: &AssetPath,
    chunk: wow_adt::McnkChunk,
    texture_count: usize,
    doodad_count: usize,
    world_model_count: usize,
    big_alpha: bool,
) -> Result<TerrainChunk, AssetError> {
    // The file names these fields `[zpos, xpos, ypos]`: the first two are
    // horizontal client-view axes and `ypos` is elevation. Normalize that
    // viewer `[X, Y-up, Z]` basis to the server/ECS `[X, Y, Z-up]` basis.
    let world_position = [
        chunk.header.position[1],
        chunk.header.position[0],
        chunk.header.position[2],
    ];
    let chunk_x = u8::try_from(chunk.header.index_x)
        .ok()
        .and_then(|x| {
            u8::try_from(chunk.header.index_y)
                .ok()
                .and_then(|y| TerrainChunkIndex::new(x, y))
        })
        .ok_or_else(|| {
            terrain_message(
                path,
                format!(
                    "MCNK index [{}, {}] is outside the 16-by-16 grid",
                    chunk.header.index_x, chunk.header.index_y
                ),
            )
        })?;
    let heights = chunk
        .heights
        .ok_or_else(|| terrain_message(path, format!("MCNK {:?} omits MCVT", chunk_x)))?
        .heights
        .into_boxed_slice()
        .try_into()
        .map_err(|values: Box<[f32]>| {
            terrain_message(
                path,
                format!(
                    "MCNK {:?} requires {TERRAIN_CHUNK_VERTEX_COUNT} heights; found {}",
                    chunk_x,
                    values.len()
                ),
            )
        })?;
    let normals = chunk
        .normals
        .ok_or_else(|| terrain_message(path, format!("MCNK {:?} omits MCNR", chunk_x)))?
        .normals
        .into_iter()
        .map(|normal| {
            let decoded = normal.to_normalized();
            // The decoder reports client-view `[X, vertical Y, Z]`. Convert
            // to the network/ECS `[map X, map Y, vertical Z]` convention.
            [decoded[0], decoded[2], decoded[1]]
        })
        .collect::<Vec<_>>()
        .into_boxed_slice()
        .try_into()
        .map_err(|values: Box<[[f32; 3]]>| {
            terrain_message(
                path,
                format!(
                    "MCNK {:?} requires {TERRAIN_CHUNK_VERTEX_COUNT} normals; found {}",
                    chunk_x,
                    values.len()
                ),
            )
        })?;
    let vertex_colors_bgra = chunk
        .vertex_colors
        .map(|colors| {
            colors
                .colors
                .into_iter()
                .map(|color| [color.b, color.g, color.r, color.a])
                .collect::<Vec<_>>()
                .into_boxed_slice()
                .try_into()
                .map_err(|values: Box<[[u8; 4]]>| {
                    terrain_message(
                        path,
                        format!(
                            "MCNK {:?} requires {TERRAIN_CHUNK_VERTEX_COUNT} vertex colors; found {}",
                            chunk_x,
                            values.len()
                        ),
                    )
                })
        })
        .transpose()?;
    let layer_source = chunk
        .layers
        .ok_or_else(|| terrain_message(path, format!("MCNK {:?} omits MCLY", chunk_x)))?;
    if layer_source.layers.is_empty()
        || layer_source.layers.len() > 4
        || layer_source.layers.len() != chunk.header.n_layers as usize
    {
        return Err(terrain_message(
            path,
            format!(
                "MCNK {:?} declares {} layers but decodes {} (expected 1..=4)",
                chunk_x,
                chunk.header.n_layers,
                layer_source.layers.len()
            ),
        ));
    }
    let alpha_bytes = chunk.alpha.map_or_else(Vec::new, |alpha| alpha.data);
    let shadow_bytes = chunk
        .shadow
        .map(|shadow| {
            shadow
                .shadow_map
                .into_boxed_slice()
                .try_into()
                .map_err(|values: Box<[u8]>| {
                    terrain_message(
                        path,
                        format!(
                            "MCNK {:?} requires 512 shadow bytes; found {}",
                            chunk_x,
                            values.len()
                        ),
                    )
                })
        })
        .transpose()?;
    let mut layers = Vec::with_capacity(layer_source.layers.len());
    for (layer_index, layer) in layer_source.layers.into_iter().enumerate() {
        if layer.texture_id as usize >= texture_count {
            return Err(terrain_message(
                path,
                format!(
                    "MCNK {:?} layer {layer_index} references texture {} of {texture_count}",
                    chunk_x, layer.texture_id
                ),
            ));
        }
        if layer_index > 0 && layer.offset_in_mcal as usize >= alpha_bytes.len() {
            return Err(terrain_message(
                path,
                format!(
                    "MCNK {:?} layer {layer_index} alpha offset {} exceeds {} bytes",
                    chunk_x,
                    layer.offset_in_mcal,
                    alpha_bytes.len()
                ),
            ));
        }
        layers.push(TerrainTextureLayer::new(
            layer.texture_id,
            layer.flags.value,
            layer.offset_in_mcal,
            layer.effect_id,
        ));
    }
    let alpha_map = decode_alpha_map(
        chunk_x,
        chunk.header.flags.value,
        &layers,
        &alpha_bytes,
        big_alpha,
    )
    .map_err(|message| terrain_message(path, message))?;
    let references = chunk.refs.map_or_else(Vec::new, |refs| refs.references);
    let expected_references = chunk
        .header
        .n_doodad_refs
        .checked_add(chunk.header.n_map_obj_refs)
        .ok_or_else(|| {
            terrain_message(path, format!("MCNK {:?} reference count overflow", chunk_x))
        })? as usize;
    if references.len() != expected_references {
        return Err(terrain_message(
            path,
            format!(
                "MCNK {:?} declares {expected_references} object references but decodes {}",
                chunk_x,
                references.len()
            ),
        ));
    }
    let split = chunk.header.n_doodad_refs as usize;
    let (doodad_references, world_model_references) = references.split_at(split);
    validate_references(path, chunk_x, "doodad", doodad_references, doodad_count)?;
    validate_references(
        path,
        chunk_x,
        "world model",
        world_model_references,
        world_model_count,
    )?;
    let sound_emitters = chunk
        .sound_emitters
        .map_or_else(Vec::new, |emitters| emitters.emitters)
        .into_iter()
        .map(|emitter| {
            TerrainSoundEmitter::new(emitter.sound_entry_id, emitter.position, emitter.size_min)
        })
        .collect::<Vec<_>>();
    if sound_emitters.len() != chunk.header.n_snd_emitters as usize {
        return Err(terrain_message(
            path,
            format!(
                "MCNK {:?} declares {} sound emitters but decodes {}",
                chunk_x,
                chunk.header.n_snd_emitters,
                sound_emitters.len()
            ),
        ));
    }
    Ok(TerrainChunk::new(
        chunk_x,
        chunk.header.flags.value,
        chunk.header.area_id,
        world_position,
        chunk.header.holes_low_res,
        heights,
        normals,
        vertex_colors_bgra,
        layers,
        alpha_map,
        shadow_bytes,
        doodad_references.to_vec(),
        world_model_references.to_vec(),
        sound_emitters,
    ))
}

fn validate_references(
    path: &AssetPath,
    chunk: TerrainChunkIndex,
    kind: &str,
    references: &[u32],
    count: usize,
) -> Result<(), AssetError> {
    if let Some(reference) = references
        .iter()
        .find(|reference| **reference as usize >= count)
    {
        return Err(terrain_message(
            path,
            format!("MCNK {:?} references {kind} {reference} of {count}", chunk),
        ));
    }
    Ok(())
}

fn decode_doodads(
    path: &AssetPath,
    root: &RootAdt,
) -> Result<Vec<TerrainDoodadPlacement>, AssetError> {
    root.doodad_placements
        .iter()
        .enumerate()
        .map(|(index, placement)| {
            let model = root.models.get(placement.name_id as usize).ok_or_else(|| {
                terrain_message(
                    path,
                    format!(
                        "MDDF placement {index} references model {} of {}",
                        placement.name_id,
                        root.models.len()
                    ),
                )
            })?;
            Ok(TerrainDoodadPlacement::new(
                terrain_asset_path(path, "doodad", model)?,
                placement.unique_id,
                placement_position(placement.position),
                placement.rotation,
                placement.get_scale(),
                placement.flags,
            ))
        })
        .collect()
}

fn decode_world_models(
    path: &AssetPath,
    root: &RootAdt,
) -> Result<Vec<TerrainWorldModelPlacement>, AssetError> {
    root.wmo_placements
        .iter()
        .enumerate()
        .map(|(index, placement)| {
            let model = root.wmos.get(placement.name_id as usize).ok_or_else(|| {
                terrain_message(
                    path,
                    format!(
                        "MODF placement {index} references WMO {} of {}",
                        placement.name_id,
                        root.wmos.len()
                    ),
                )
            })?;
            Ok(TerrainWorldModelPlacement::new(
                terrain_asset_path(path, "world model", model)?,
                placement.unique_id,
                placement_position(placement.position),
                placement.rotation,
                placement_bounds(placement.extents_min, placement.extents_max),
                placement.flags,
                placement.doodad_set,
                placement.name_set,
            ))
        })
        .collect()
}

/// Converts the client renderer's offset X/Y-up/Z placement basis to the
/// server/ECS map X/Y/Z-up basis carried in world packets.
const fn placement_position(position: [f32; 3]) -> [f32; 3] {
    [
        CLIENT_MAP_ORIGIN - position[0],
        CLIENT_MAP_ORIGIN - position[2],
        position[1],
    ]
}

/// Converts both corners while preserving lower/upper ordering after the two
/// horizontal axes are reflected around the client map origin.
const fn placement_bounds(minimum: [f32; 3], maximum: [f32; 3]) -> [[f32; 3]; 2] {
    [
        [
            CLIENT_MAP_ORIGIN - maximum[0],
            CLIENT_MAP_ORIGIN - maximum[2],
            minimum[1],
        ],
        [
            CLIENT_MAP_ORIGIN - minimum[0],
            CLIENT_MAP_ORIGIN - minimum[2],
            maximum[1],
        ],
    ]
}

fn validate_string_offsets(
    path: &AssetPath,
    table: &str,
    strings: &[String],
    offsets: &[u32],
) -> Result<(), AssetError> {
    if strings.len() != offsets.len() {
        return Err(terrain_message(
            path,
            format!(
                "{table} has {} offsets for {} strings",
                offsets.len(),
                strings.len()
            ),
        ));
    }
    let mut expected = 0_u32;
    for (index, (string, offset)) in strings.iter().zip(offsets).enumerate() {
        if *offset != expected {
            return Err(terrain_message(
                path,
                format!("{table} offset {index} is {offset}; expected {expected}"),
            ));
        }
        expected = expected
            .checked_add(
                u32::try_from(string.len() + 1).map_err(|error| terrain_error(path, error))?,
            )
            .ok_or_else(|| terrain_message(path, format!("{table} string extent overflow")))?;
    }
    Ok(())
}

fn terrain_asset_path(
    terrain_path: &AssetPath,
    kind: &str,
    value: &str,
) -> Result<AssetPath, AssetError> {
    AssetPath::new(value).map_err(|error| {
        terrain_message(
            terrain_path,
            format!("invalid {kind} path {value:?}: {error}"),
        )
    })
}

fn validate_chunk_stream(path: &AssetPath, bytes: &[u8]) -> Result<(), AssetError> {
    const ALLOWED_CHUNKS: [[u8; 4]; 5] = [*b"REVM", *b"DHPM", *b"NIAM", *b"OMWM", *b"FDOM"];
    let mut offset = 0_usize;
    let mut seen = Vec::with_capacity(ALLOWED_CHUNKS.len());
    while offset < bytes.len() {
        let header_end = offset
            .checked_add(8)
            .ok_or_else(|| terrain_message(path, "WDT chunk header offset overflow"))?;
        let header = bytes
            .get(offset..header_end)
            .ok_or_else(|| terrain_message(path, "WDT ends inside a chunk header"))?;
        let magic: [u8; 4] = header[0..4]
            .try_into()
            .map_err(|_| terrain_message(path, "WDT chunk magic is truncated"))?;
        if !ALLOWED_CHUNKS.contains(&magic) {
            return Err(terrain_message(
                path,
                format!(
                    "build-12340 WDT contains unknown chunk {:?}",
                    String::from_utf8_lossy(&magic)
                ),
            ));
        }
        if seen.contains(&magic) {
            return Err(terrain_message(
                path,
                format!(
                    "build-12340 WDT repeats chunk {:?}",
                    String::from_utf8_lossy(&magic)
                ),
            ));
        }
        seen.push(magic);
        let size = u32::from_le_bytes(
            header[4..8]
                .try_into()
                .map_err(|_| terrain_message(path, "WDT chunk size is truncated"))?,
        );
        offset = header_end
            .checked_add(size as usize)
            .ok_or_else(|| terrain_message(path, "WDT chunk extent overflow"))?;
        if offset > bytes.len() {
            return Err(terrain_message(
                path,
                format!("WDT chunk extends to {offset} beyond {} bytes", bytes.len()),
            ));
        }
    }
    Ok(())
}

fn validate_build_layout(path: &AssetPath, wdt: &WdtFile) -> Result<(), AssetError> {
    if wdt.maid.is_some() || wdt.mphd.has_maid() {
        return Err(AssetError::TerrainDecode {
            path: path.clone(),
            message: "build-12340 WDT cannot contain a MAID file-data-id table".to_owned(),
        });
    }
    if wdt.is_wmo_only() {
        if wdt.mwmo.as_ref().map(|chunk| chunk.filenames.len()) != Some(1)
            || wdt.modf.as_ref().map(|chunk| chunk.entries.len()) != Some(1)
        {
            return Err(AssetError::TerrainDecode {
                path: path.clone(),
                message: "global-WMO WDT requires exactly one MWMO name and one MODF placement"
                    .to_owned(),
            });
        }
    } else if wdt.modf.is_some() {
        return Err(AssetError::TerrainDecode {
            path: path.clone(),
            message: "terrain WDT contains a global MODF placement".to_owned(),
        });
    }
    Ok(())
}

fn global_world_model(path: &AssetPath, wdt: &WdtFile) -> Result<Option<AssetPath>, AssetError> {
    if !wdt.is_wmo_only() {
        return Ok(None);
    }
    let filename = &wdt
        .mwmo
        .as_ref()
        .ok_or_else(|| AssetError::TerrainDecode {
            path: path.clone(),
            message: "global-WMO WDT omits MWMO".to_owned(),
        })?
        .filenames[0];
    AssetPath::new(filename)
        .map(Some)
        .map_err(|error| AssetError::TerrainDecode {
            path: path.clone(),
            message: format!("invalid global WMO path {filename:?}: {error}"),
        })
}

fn terrain_error(path: &AssetPath, error: impl std::fmt::Display) -> AssetError {
    terrain_message(path, error.to_string())
}

fn terrain_message(path: &AssetPath, message: impl Into<String>) -> AssetError {
    AssetError::TerrainDecode {
        path: path.clone(),
        message: message.into(),
    }
}
