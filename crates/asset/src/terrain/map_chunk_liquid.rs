//! Normalized build-12340 MH2O liquid tables.

const LIQUID_CHUNK_COUNT: usize = 16 * 16;
const LIQUID_HEADER_SIZE: usize = 12;
const LIQUID_INSTANCE_SIZE: usize = 24;

/// One complete root-level MH2O table, parallel to the ADT's MCNK grid.
pub struct TerrainLiquidTable {
    chunks: Box<[TerrainLiquidChunk; LIQUID_CHUNK_COUNT]>,
}

/// All liquid layers and attributes belonging to one MCNK.
#[derive(Default)]
pub struct TerrainLiquidChunk {
    fishable_mask: u64,
    deep_mask: u64,
    layers: Vec<TerrainLiquidLayer>,
}

/// One rectangular stock liquid surface authored inside an MCNK.
pub struct TerrainLiquidLayer {
    liquid_type: u16,
    vertex_format: u16,
    minimum_height: f32,
    maximum_height: f32,
    x_offset: u8,
    y_offset: u8,
    width: u8,
    height: u8,
    exists: Box<[u8]>,
    heights: Box<[f32]>,
    depths: Box<[u8]>,
    texture_coordinates: Option<Box<[[u16; 2]]>>,
}

impl TerrainLiquidTable {
    /// Returns the row-major 16-by-16 table parallel to the tile's MCNKs.
    #[must_use]
    pub fn chunks(&self) -> &[TerrainLiquidChunk; LIQUID_CHUNK_COUNT] {
        &self.chunks
    }

    /// Returns the total number of authored liquid layers in the ADT.
    #[must_use]
    pub fn layer_count(&self) -> usize {
        self.chunks.iter().map(|chunk| chunk.layers.len()).sum()
    }
}

impl TerrainLiquidChunk {
    /// Returns the raw 8-by-8 MH2O fishable-cell mask.
    #[must_use]
    pub const fn fishable_mask(&self) -> u64 {
        self.fishable_mask
    }

    /// Returns the raw 8-by-8 MH2O deep-cell mask.
    #[must_use]
    pub const fn deep_mask(&self) -> u64 {
        self.deep_mask
    }

    /// Returns liquid layers in their authored order.
    #[must_use]
    pub fn layers(&self) -> &[TerrainLiquidLayer] {
        &self.layers
    }
}

impl TerrainLiquidLayer {
    /// Returns the `LiquidType.dbc` identifier.
    #[must_use]
    pub const fn liquid_type(&self) -> u16 {
        self.liquid_type
    }

    /// Returns the effective build-12340 liquid vertex format from zero to three.
    ///
    /// Stock promotes a non-type-2 layer without vertex data to format two.
    #[must_use]
    pub const fn vertex_format(&self) -> u16 {
        self.vertex_format
    }

    /// Returns the authored minimum surface height.
    #[must_use]
    pub const fn minimum_height(&self) -> f32 {
        self.minimum_height
    }

    /// Returns the authored maximum surface height.
    #[must_use]
    pub const fn maximum_height(&self) -> f32 {
        self.maximum_height
    }

    /// Returns the first cell column inside the MCNK's 8-by-8 grid.
    #[must_use]
    pub const fn x_offset(&self) -> u8 {
        self.x_offset
    }

    /// Returns the first cell row inside the MCNK's 8-by-8 grid.
    #[must_use]
    pub const fn y_offset(&self) -> u8 {
        self.y_offset
    }

    /// Returns the liquid rectangle's width in cells.
    #[must_use]
    pub const fn width(&self) -> u8 {
        self.width
    }

    /// Returns the liquid rectangle's height in cells.
    #[must_use]
    pub const fn height(&self) -> u8 {
        self.height
    }

    /// Returns one byte per cell, in row-major order, containing zero or one.
    #[must_use]
    pub fn exists(&self) -> &[u8] {
        &self.exists
    }

    /// Returns the row-major `(width + 1)` by `(height + 1)` absolute heights.
    #[must_use]
    pub fn heights(&self) -> &[f32] {
        &self.heights
    }

    /// Returns the normalized source depth byte for every liquid vertex.
    #[must_use]
    pub fn depths(&self) -> &[u8] {
        &self.depths
    }

    /// Returns authored UV words for vertex formats one and three.
    #[must_use]
    pub fn texture_coordinates(&self) -> Option<&[[u16; 2]]> {
        self.texture_coordinates.as_deref()
    }
}

/// Finds and strictly normalizes the optional top-level MH2O payload.
pub(super) fn decode_liquid_table(bytes: &[u8]) -> Result<Option<TerrainLiquidTable>, String> {
    let mut offset = 0_usize;
    let mut table = None;
    while offset < bytes.len() {
        let header_end = checked_add(offset, 8, "ADT chunk header offset overflow")?;
        let header = slice(
            bytes,
            offset,
            header_end,
            "ADT ends inside a top-level chunk header",
        )?;
        let size = usize::try_from(read_u32(header, 4, "ADT chunk size is truncated")?)
            .map_err(|error| format!("ADT chunk size does not fit this target: {error}"))?;
        let chunk_end = checked_add(header_end, size, "ADT chunk extent overflow")?;
        let payload = slice(
            bytes,
            header_end,
            chunk_end,
            "ADT top-level chunk exceeds file size",
        )?;
        if &header[..4] == b"O2HM" {
            if table.is_some() {
                return Err("ADT repeats top-level MH2O".to_owned());
            }
            table = Some(decode_liquid_payload(payload)?);
        }
        offset = chunk_end;
    }
    Ok(table)
}

fn decode_liquid_payload(payload: &[u8]) -> Result<TerrainLiquidTable, String> {
    let headers_end = LIQUID_CHUNK_COUNT * LIQUID_HEADER_SIZE;
    if payload.len() < headers_end {
        return Err("MH2O is smaller than its 256-entry liquid header table".to_owned());
    }
    let mut chunks: Box<[TerrainLiquidChunk; LIQUID_CHUNK_COUNT]> =
        Box::new(std::array::from_fn(|_| TerrainLiquidChunk::default()));
    for (chunk_index, chunk) in chunks.iter_mut().enumerate() {
        let header = chunk_index * LIQUID_HEADER_SIZE;
        let instance_offset = read_offset(payload, header, "MH2O instance offset")?;
        let layer_count = usize::try_from(read_u32(
            payload,
            header + 4,
            "MH2O layer count is truncated",
        )?)
        .map_err(|error| format!("MH2O layer count does not fit this target: {error}"))?;
        let attributes_offset = read_offset(payload, header + 8, "MH2O attributes offset")?;
        if layer_count == 0 {
            continue;
        }
        if instance_offset == 0 {
            return Err(format!(
                "MH2O chunk {chunk_index} has layers but no instance offset"
            ));
        }
        let instance_bytes = layer_count
            .checked_mul(LIQUID_INSTANCE_SIZE)
            .ok_or_else(|| format!("MH2O chunk {chunk_index} instance array size overflow"))?;
        let instance_end = checked_add(
            instance_offset,
            instance_bytes,
            &format!("MH2O chunk {chunk_index} instance array extent overflow"),
        )?;
        slice(
            payload,
            instance_offset,
            instance_end,
            &format!("MH2O chunk {chunk_index} instance array exceeds its payload"),
        )?;
        if attributes_offset != 0 {
            let attributes_end = checked_add(
                attributes_offset,
                16,
                &format!("MH2O chunk {chunk_index} attributes extent overflow"),
            )?;
            let attributes = slice(
                payload,
                attributes_offset,
                attributes_end,
                &format!("MH2O chunk {chunk_index} attributes exceed its payload"),
            )?;
            chunk.fishable_mask = read_u64(attributes, 0, "MH2O fishable mask is truncated")?;
            chunk.deep_mask = read_u64(attributes, 8, "MH2O deep mask is truncated")?;
        }
        chunk.layers.reserve(layer_count);
        for layer_index in 0..layer_count {
            let base = instance_offset + layer_index * LIQUID_INSTANCE_SIZE;
            chunk.layers.push(decode_liquid_layer(
                payload,
                chunk_index,
                layer_index,
                base,
            )?);
        }
    }
    Ok(TerrainLiquidTable { chunks })
}

fn decode_liquid_layer(
    payload: &[u8],
    chunk_index: usize,
    layer_index: usize,
    base: usize,
) -> Result<TerrainLiquidLayer, String> {
    let label = format!("MH2O chunk {chunk_index} layer {layer_index}");
    let liquid_type = read_u16(payload, base, &format!("{label} liquid type is truncated"))?;
    let mut vertex_format = read_u16(
        payload,
        base + 2,
        &format!("{label} vertex format is truncated"),
    )?;
    let minimum_height = read_f32(
        payload,
        base + 4,
        &format!("{label} minimum height is truncated"),
    )?;
    let maximum_height = read_f32(
        payload,
        base + 8,
        &format!("{label} maximum height is truncated"),
    )?;
    let x_offset = read_u8(
        payload,
        base + 12,
        &format!("{label} X offset is truncated"),
    )?;
    let y_offset = read_u8(
        payload,
        base + 13,
        &format!("{label} Y offset is truncated"),
    )?;
    let width = read_u8(payload, base + 14, &format!("{label} width is truncated"))?;
    let height = read_u8(payload, base + 15, &format!("{label} height is truncated"))?;
    let exists_offset = read_offset(payload, base + 16, &format!("{label} exists offset"))?;
    let vertex_offset = read_offset(payload, base + 20, &format!("{label} vertex offset"))?;

    // This is a native build-12340 loader rule, not a recovery default.
    // Null vertex data on a non-type-2 liquid denotes the depth-only ocean
    // encoding, whose surface vertices are all at Z=0.
    if vertex_offset == 0 && liquid_type != 2 {
        vertex_format = 2;
    }
    if !minimum_height.is_finite()
        || !maximum_height.is_finite()
        || vertex_format > 3
        || width == 0
        || height == 0
        || x_offset.checked_add(width).is_none_or(|end| end > 8)
        || y_offset.checked_add(height).is_none_or(|end| end > 8)
    {
        return Err(format!(
            "{label} has invalid bounds, heights, or vertex format"
        ));
    }

    let tile_count = usize::from(width) * usize::from(height);
    let vertex_count = (usize::from(width) + 1) * (usize::from(height) + 1);
    let mut exists = vec![1_u8; tile_count];
    if exists_offset != 0 {
        let bitmap_bytes = tile_count.div_ceil(8);
        let bitmap_end = checked_add(
            exists_offset,
            bitmap_bytes,
            &format!("{label} existence bitmap extent overflow"),
        )?;
        let bitmap = slice(
            payload,
            exists_offset,
            bitmap_end,
            &format!("{label} existence bitmap exceeds MH2O"),
        )?;
        for (index, value) in exists.iter_mut().enumerate() {
            *value = (bitmap[index / 8] >> (index % 8)) & 1;
        }
    }

    let has_heights = vertex_format != 2;
    let has_texture_coordinates = matches!(vertex_format, 1 | 3);
    let has_depths = matches!(vertex_format, 0 | 2 | 3);
    let default_height = if vertex_format == 2 {
        0.0
    } else {
        minimum_height
    };
    let mut heights = vec![default_height; vertex_count];
    let mut depths = vec![u8::MAX; vertex_count];
    let mut texture_coordinates = has_texture_coordinates.then(|| vec![[0_u16; 2]; vertex_count]);
    if vertex_offset != 0 {
        decode_vertex_arrays(
            payload,
            &label,
            vertex_offset,
            has_heights,
            has_texture_coordinates,
            has_depths,
            &mut heights,
            texture_coordinates.as_mut(),
            &mut depths,
        )?;
    }
    Ok(TerrainLiquidLayer {
        liquid_type,
        vertex_format,
        minimum_height,
        maximum_height,
        x_offset,
        y_offset,
        width,
        height,
        exists: exists.into_boxed_slice(),
        heights: heights.into_boxed_slice(),
        depths: depths.into_boxed_slice(),
        texture_coordinates: texture_coordinates.map(Vec::into_boxed_slice),
    })
}

#[allow(clippy::too_many_arguments)]
fn decode_vertex_arrays(
    payload: &[u8],
    label: &str,
    vertex_offset: usize,
    has_heights: bool,
    has_texture_coordinates: bool,
    has_depths: bool,
    heights: &mut [f32],
    texture_coordinates: Option<&mut Vec<[u16; 2]>>,
    depths: &mut [u8],
) -> Result<(), String> {
    let vertex_count = heights.len();
    let height_bytes = if has_heights {
        vertex_count
            .checked_mul(4)
            .ok_or_else(|| format!("{label} height array size overflow"))?
    } else {
        0
    };
    let texture_bytes = if has_texture_coordinates {
        vertex_count
            .checked_mul(4)
            .ok_or_else(|| format!("{label} UV array size overflow"))?
    } else {
        0
    };
    let depth_bytes = if has_depths { vertex_count } else { 0 };
    let arrays_size = height_bytes
        .checked_add(texture_bytes)
        .and_then(|size| size.checked_add(depth_bytes))
        .ok_or_else(|| format!("{label} vertex array size overflow"))?;
    let arrays_end = checked_add(
        vertex_offset,
        arrays_size,
        &format!("{label} vertex array extent overflow"),
    )?;
    slice(
        payload,
        vertex_offset,
        arrays_end,
        &format!("{label} vertex arrays exceed MH2O"),
    )?;
    let mut cursor = vertex_offset;
    if has_heights {
        for (index, height) in heights.iter_mut().enumerate() {
            *height = read_f32(
                payload,
                cursor + index * 4,
                &format!("{label} height is truncated"),
            )?;
            if !height.is_finite() {
                return Err(format!("{label} height {index} is not finite"));
            }
        }
        cursor += height_bytes;
    }
    if let Some(texture_coordinates) = texture_coordinates {
        for (index, coordinates) in texture_coordinates.iter_mut().enumerate() {
            coordinates[0] = read_u16(
                payload,
                cursor + index * 4,
                &format!("{label} U coordinate is truncated"),
            )?;
            coordinates[1] = read_u16(
                payload,
                cursor + index * 4 + 2,
                &format!("{label} V coordinate is truncated"),
            )?;
        }
        cursor += texture_bytes;
    }
    if has_depths {
        depths.copy_from_slice(slice(
            payload,
            cursor,
            cursor + depth_bytes,
            &format!("{label} depth array is truncated"),
        )?);
    }
    Ok(())
}

fn read_offset(bytes: &[u8], offset: usize, label: &str) -> Result<usize, String> {
    usize::try_from(read_u32(bytes, offset, &format!("{label} is truncated"))?)
        .map_err(|error| format!("{label} does not fit this target: {error}"))
}

fn read_u8(bytes: &[u8], offset: usize, message: &str) -> Result<u8, String> {
    bytes.get(offset).copied().ok_or_else(|| message.to_owned())
}

fn read_u16(bytes: &[u8], offset: usize, message: &str) -> Result<u16, String> {
    let end = checked_add(offset, 2, message)?;
    let value: [u8; 2] = slice(bytes, offset, end, message)?
        .try_into()
        .map_err(|_| message.to_owned())?;
    Ok(u16::from_le_bytes(value))
}

fn read_u32(bytes: &[u8], offset: usize, message: &str) -> Result<u32, String> {
    let end = checked_add(offset, 4, message)?;
    let value: [u8; 4] = slice(bytes, offset, end, message)?
        .try_into()
        .map_err(|_| message.to_owned())?;
    Ok(u32::from_le_bytes(value))
}

fn read_u64(bytes: &[u8], offset: usize, message: &str) -> Result<u64, String> {
    let end = checked_add(offset, 8, message)?;
    let value: [u8; 8] = slice(bytes, offset, end, message)?
        .try_into()
        .map_err(|_| message.to_owned())?;
    Ok(u64::from_le_bytes(value))
}

fn read_f32(bytes: &[u8], offset: usize, message: &str) -> Result<f32, String> {
    Ok(f32::from_bits(read_u32(bytes, offset, message)?))
}

fn checked_add(offset: usize, size: usize, message: &str) -> Result<usize, String> {
    offset.checked_add(size).ok_or_else(|| message.to_owned())
}

fn slice<'a>(bytes: &'a [u8], start: usize, end: usize, message: &str) -> Result<&'a [u8], String> {
    bytes.get(start..end).ok_or_else(|| message.to_owned())
}
