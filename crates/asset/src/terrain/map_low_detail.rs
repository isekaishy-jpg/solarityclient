//! Build-12340 WDL heights and face-culling masks, loaded by `0x007CC310`.

use std::io::Cursor;

use wow_wdl::WdlVersion;
use wow_wdl::parser::WdlParser;

use crate::{AssetError, AssetPath, AssetStore};

use super::map_load::{placement_bounds, placement_position};
use super::{TerrainMap, TerrainTileIndex, TerrainWorldModelPlacement};

/// One map's authored low-detail terrain, independent of resident ADTs.
pub struct TerrainLowDetail {
    tiles: Vec<TerrainLowDetailTile>,
    world_models: Vec<TerrainWorldModelPlacement>,
}

/// A WDL tile's 17-by-17 corners followed by its 16-by-16 cell centers.
pub struct TerrainLowDetailTile {
    index: TerrainTileIndex,
    heights: [i16; 545],
    face_masks: [u16; 16],
}

impl TerrainLowDetail {
    /// Loads the map's WDL through normal archive precedence.
    ///
    /// `0x007CC310` returns absence when opening the WDL fails. Archive and
    /// decoding errors remain errors; only an absent archive member is optional.
    ///
    /// # Errors
    /// Returns archive failures or malformed WDL chunk/offset/height data.
    pub fn load(store: &mut AssetStore, map: &TerrainMap) -> Result<Option<Self>, AssetError> {
        let path = map.wdl_path()?;
        let read = match store.read(&path) {
            Ok(read) => read,
            Err(AssetError::AssetNotFound { .. }) => return Ok(None),
            Err(error) => return Err(error),
        };
        Self::decode(&path, read.bytes()).map(Some)
    }

    /// Decodes the original 64-by-64 MAOF addressing and signed MARE heights.
    ///
    /// # Errors
    /// Returns an error for truncated chunks, unsupported layouts, or invalid offsets.
    pub fn decode(path: &AssetPath, bytes: &[u8]) -> Result<Self, AssetError> {
        validate_chunks(path, bytes)?;
        let mut file = WdlParser::with_version(WdlVersion::Wotlk)
            .parse(&mut Cursor::new(bytes))
            .map_err(|error| decode_error(path, error.to_string()))?;
        let mut tiles = Vec::with_capacity(file.heightmap_tiles.len());
        for y in 0..64_u8 {
            for x in 0..64_u8 {
                let key = (u32::from(x), u32::from(y));
                let Some(heightmap) = file.heightmap_tiles.remove(&key) else {
                    continue;
                };
                let index = TerrainTileIndex::new(x, y)
                    .ok_or_else(|| decode_error(path, "invalid WDL tile index"))?;
                let mut heights = [0; 545];
                heights[..289].copy_from_slice(&heightmap.outer_values);
                heights[289..].copy_from_slice(&heightmap.inner_values);
                // Native 7D5240 partitions on the raw bit. The dependency's
                // has_hole accessor describes the opposite polarity, so retain
                // the authored words and let rendering select the two banks.
                let face_masks = file
                    .holes_data
                    .remove(&key)
                    .map_or([0; 16], |data| data.hole_masks);
                tiles.push(TerrainLowDetailTile {
                    index,
                    heights,
                    face_masks,
                });
            }
        }
        let world_models = decode_world_models(path, &file)?;
        Ok(Self {
            tiles,
            world_models,
        })
    }

    /// Returns authored tiles in MAOF row order, including tiles without an ADT.
    pub fn tiles(&self) -> &[TerrainLowDetailTile] {
        &self.tiles
    }

    /// Returns the map-wide MODF roots retained by native `7CC310` for the far queue.
    pub fn world_models(&self) -> &[TerrainWorldModelPlacement] {
        &self.world_models
    }
}

impl TerrainLowDetailTile {
    /// Returns the MAOF tile coordinates, in the same order as ADT filenames.
    pub const fn index(&self) -> TerrainTileIndex {
        self.index
    }

    /// Returns all signed world-space heights in native upload order.
    pub const fn heights(&self) -> &[i16; 545] {
        &self.heights
    }

    /// Returns the MAHO masks used to partition unculled and culled faces.
    pub const fn face_masks(&self) -> &[u16; 16] {
        &self.face_masks
    }
}

/// Bounds the dependency's allocating parser and prevents truncated EOF recovery.
fn validate_chunks(path: &AssetPath, bytes: &[u8]) -> Result<(), AssetError> {
    let mut offset = 0;
    let mut offsets = None;
    let mut height_chunks = Vec::new();
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset + 8)
            .ok_or_else(|| decode_error(path, "truncated WDL chunk header"))?;
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let end = (offset + 8)
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| decode_error(path, "truncated WDL chunk payload"))?;
        let valid = match &header[..4] {
            b"REVM" => size == 4 && bytes[offset + 8..end] == 18_u32.to_le_bytes(),
            b"FOAM" => {
                if offsets.is_some() {
                    return Err(decode_error(path, "duplicate WDL MAOF table"));
                }
                offsets = Some(&bytes[offset + 8..end]);
                size == 4096 * 4
            }
            b"ERAM" => {
                height_chunks.push(offset);
                size == 545 * 2
            }
            b"OHAM" => size == 16 * 2,
            b"OMWM" => true,
            b"DIWM" => size.is_multiple_of(4),
            b"FDOM" => size.is_multiple_of(64),
            _ => false,
        };
        if !valid {
            return Err(decode_error(
                path,
                "unsupported build-12340 WDL chunk layout",
            ));
        }
        offset = end;
    }
    let offsets = offsets.ok_or_else(|| decode_error(path, "WDL omits MAOF offsets"))?;
    // 7CC310 addresses chunk headers from the map-wide base. Validate that
    // boundary before allowing the dependency to allocate from a pointed size.
    for word in offsets.as_chunks::<4>().0 {
        let target = u32::from_le_bytes(*word) as usize;
        if target != 0 && height_chunks.binary_search(&target).is_err() {
            return Err(decode_error(path, "WDL MAOF offset does not address MARE"));
        }
    }
    Ok(())
}

/// Resolves MODF through MWID byte offsets, as the native map-wide loader does.
fn decode_world_models(
    path: &AssetPath,
    file: &wow_wdl::WdlFile,
) -> Result<Vec<TerrainWorldModelPlacement>, AssetError> {
    let mut placements = Vec::with_capacity(file.wmo_placements.len());
    for placement in &file.wmo_placements {
        let name_offset = file
            .wmo_indices
            .get(placement.wmo_id as usize)
            .ok_or_else(|| decode_error(path, "WDL MODF references absent MWID entry"))?;
        let names = file
            .chunks
            .iter()
            .find(|chunk| chunk.magic == *b"OMWM")
            .ok_or_else(|| decode_error(path, "WDL MODF omits MWMO names"))?;
        let tail = names
            .data
            .get(*name_offset as usize..)
            .ok_or_else(|| decode_error(path, "WDL MWID string offset is out of bounds"))?;
        let end = tail
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| decode_error(path, "unterminated WDL WMO path"))?;
        let name = std::str::from_utf8(&tail[..end])
            .map_err(|error| decode_error(path, error.to_string()))?;
        let model_path =
            AssetPath::new(name).map_err(|error| decode_error(path, error.to_string()))?;
        let vector = |v: &wow_wdl::types::Vec3d| [v.x, v.y, v.z];
        placements.push(TerrainWorldModelPlacement::new(
            model_path,
            placement.id,
            placement_position(vector(&placement.position)),
            vector(&placement.rotation),
            placement_bounds(vector(&placement.bounds.min), vector(&placement.bounds.max)),
            placement.flags,
            placement.doodad_set,
            placement.name_set,
        ));
    }
    Ok(placements)
}

/// Keeps WDL failures under the existing terrain asset error boundary.
fn decode_error(path: &AssetPath, message: impl Into<String>) -> AssetError {
    AssetError::TerrainDecode {
        path: path.clone(),
        message: message.into(),
    }
}
