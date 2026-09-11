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
use super::map_chunk_liquid::{TerrainLiquidTable, decode_liquid_table};
use super::map_shadow::TerrainShadowMap;

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
        let liquids =
            decode_liquid_table(read.bytes()).map_err(|message| terrain_message(&path, message))?;
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
            liquids,
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
    liquids: Option<TerrainLiquidTable>,
) -> Result<DecodedTerrainTile, AssetError> {
    // Build 12340 ships legacy-authored Vanilla and TBC tiles unchanged
    // alongside WotLK-authored tiles. Their monolithic root layout remains
    // a native stock input; only post-WotLK split-era layouts are invalid.
    if root.version > AdtVersion::WotLK {
        return Err(terrain_message(
            path,
            format!(
                "expected a monolithic build-12340-compatible ADT; decoder identified {:?}",
                root.version
            ),
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
            weighted_blending: big_alpha,
        },
        chunks,
        doodads,
        world_models,
        liquids,
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
    // Build-12340 MCNK origins are already absolute server/ECS [X, Y, Z].
    // ADT filename coordinates are transposed relative to those world axes;
    // transposing the chunk itself instead moves the tile across the diagonal.
    let world_position = chunk.header.position;
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
    let shadow_map = decode_shadow_map(
        path,
        chunk_x,
        chunk.header.size_shadow,
        chunk.header.flags.value,
        chunk.shadow.as_ref(),
    )?;
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
            // 7C4620 copies the three stored signed bytes directly to world
            // XYZ. The dependency's to_normalized() instead exposes Y-up
            // coordinates, so its field names must not reorder this payload.
            [normal.x, normal.z, normal.y].map(|value| f32::from(value) * (1.0 / 127.0))
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
            // The dependency retains historical labels for both values. The
            // first references SoundEntriesAdvanced.ID; `size_min` is passed
            // to FMOD as a directional-cone orientation by build 12340.
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
    // wow-adt names the two halves separately; native 0x007A0530 reads all
    // sixteen bytes at 0x40 as eight little-endian two-bit texture rows.
    let texture_selection = std::array::from_fn(|row| {
        let bytes = if row < 4 {
            &chunk.header.pred_tex
        } else {
            &chunk.header.no_effect_doodad
        };
        let offset = (row % 4) * 2;
        u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
    });
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
        shadow_map,
        doodad_references.to_vec(),
        world_model_references.to_vec(),
        sound_emitters,
        texture_selection,
        // Native 7C64B0 retains header+0x50 separately from the full sixteen-
        // byte texture selector. wow-adt labels these eight bytes unknown.
        chunk.header.unknown_8bytes,
    ))
}

fn decode_shadow_map(
    path: &AssetPath,
    chunk_index: TerrainChunkIndex,
    declared_size: u32,
    chunk_flags: u32,
    shadow: Option<&wow_adt::McshChunk>,
) -> Result<Option<TerrainShadowMap>, AssetError> {
    // Stock treats sizeMCSH as authoritative because patched files can retain
    // a stale flag and offset after removing the payload.
    if declared_size == 0 {
        return Ok(None);
    }
    if declared_size != 512 {
        return Err(terrain_message(
            path,
            format!(
                "MCNK {chunk_index:?} declares {} MCSH bytes; expected 512",
                declared_size
            ),
        ));
    }
    let bytes = shadow
        .ok_or_else(|| terrain_message(path, format!("MCNK {chunk_index:?} omits MCSH")))?
        .shadow_map
        .as_slice()
        .try_into()
        .map_err(|_source| {
            terrain_message(
                path,
                format!("MCNK {chunk_index:?} MCSH does not contain 512 bytes"),
            )
        })?;
    Ok(Some(TerrainShadowMap::from_packed(
        bytes,
        chunk_flags & 0x8000 != 0,
    )))
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
pub(super) const fn placement_position(position: [f32; 3]) -> [f32; 3] {
    [
        CLIENT_MAP_ORIGIN - position[2],
        CLIENT_MAP_ORIGIN - position[0],
        position[1],
    ]
}

/// Converts both corners while preserving lower/upper ordering after the two
/// horizontal axes are reflected around the client map origin.
pub(super) const fn placement_bounds(minimum: [f32; 3], maximum: [f32; 3]) -> [[f32; 3]; 2] {
    [
        [
            CLIENT_MAP_ORIGIN - maximum[2],
            CLIENT_MAP_ORIGIN - maximum[0],
            minimum[1],
        ],
        [
            CLIENT_MAP_ORIGIN - minimum[2],
            CLIENT_MAP_ORIGIN - minimum[0],
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

fn global_world_model(
    path: &AssetPath,
    wdt: &WdtFile,
) -> Result<Option<TerrainWorldModelPlacement>, AssetError> {
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
    let model = AssetPath::new(filename).map_err(|error| AssetError::TerrainDecode {
        path: path.clone(),
        message: format!("invalid global WMO path {filename:?}: {error}"),
    })?;
    let placement = wdt
        .modf
        .as_ref()
        .and_then(|chunk| chunk.entries.first())
        .ok_or_else(|| AssetError::TerrainDecode {
            path: path.clone(),
            message: "global-WMO WDT omits MODF".to_owned(),
        })?;
    if placement.id != 0 {
        return Err(AssetError::TerrainDecode {
            path: path.clone(),
            message: format!(
                "global MODF references MWMO name {} instead of the sole name 0",
                placement.id
            ),
        });
    }
    Ok(Some(TerrainWorldModelPlacement::new(
        model,
        placement.unique_id,
        // Build 12340's global WDT loader (0x7BF8B0) changes axes without
        // the ADT map-origin offset. Applying that offset moves an instance
        // away from its packet coordinates and prevents initial floor contact.
        [
            -placement.position[2],
            -placement.position[0],
            placement.position[1],
        ],
        placement.rotation,
        [
            [
                -placement.upper_bounds[2],
                -placement.upper_bounds[0],
                placement.lower_bounds[1],
            ],
            [
                -placement.lower_bounds[2],
                -placement.lower_bounds[0],
                placement.upper_bounds[1],
            ],
        ],
        placement.flags,
        placement.doodad_set,
        placement.name_set,
    )))
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
