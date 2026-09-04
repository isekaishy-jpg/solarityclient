//! Archive-backed CPU composition of stock character atlas mip levels.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

use solarity_asset::{AssetPath, AssetStore, BlpTextureCache, BlpTextureSource, DecodedBlpTexture};

use super::{
    CharacterAtlasLayer, CharacterAtlasLayerKind, CharacterAtlasMip, CharacterAtlasRect,
    CharacterAtlasTexture, CharacterComponentTextureLevel, CharacterTextureComposeError,
    CharacterTexturePlan,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourcePaste {
    Direct { first_mip: usize },
    ScaleTop,
}

impl CharacterTexturePlan {
    /// Loads all planned BLP sources and composes the stock body atlas mip chain.
    ///
    /// Parsed BLP sources remain shared in [`BlpTextureCache`]. Within one
    /// composition, each required authored mip is decoded only once even though
    /// the base skin is copied into several regions.
    ///
    /// # Errors
    ///
    /// Returns [`CharacterTextureComposeError`] when the required skin source
    /// is missing, an available source is malformed, or its authored mip cannot
    /// cover the destination region. Stock omits missing overlay handles after
    /// its texture-cache lookup, so optional overlay files are skipped here too.
    pub fn compose(
        &self,
        store: &mut AssetStore,
        cache: &mut BlpTextureCache,
    ) -> Result<CharacterAtlasTexture, CharacterTextureComposeError> {
        self.compose_at_level(store, cache, CharacterComponentTextureLevel::DEFAULT)
    }

    /// Composes at the live `componentTextureLevel` selected by the UI owner.
    ///
    /// # Errors
    ///
    /// Returns [`CharacterTextureComposeError`] under the same conditions as
    /// [`Self::compose`].
    pub fn compose_at_level(
        &self,
        store: &mut AssetStore,
        cache: &mut BlpTextureCache,
        level: CharacterComponentTextureLevel,
    ) -> Result<CharacterAtlasTexture, CharacterTextureComposeError> {
        let sources = load_sources(self.atlas_layers(), store, cache)?;
        let mut decoded = HashMap::new();
        let mut atlas = empty_atlas(level);

        for (layer, source) in self
            .atlas_layers()
            .iter()
            .zip(&sources)
            .filter_map(|(layer, source)| source.as_ref().map(|source| (layer, source)))
        {
            let source_paste = select_source_paste(layer, source, level)?;
            paste_layer(layer, source, source_paste, level, &mut decoded, &mut atlas)?;
        }
        Ok(CharacterAtlasTexture::new(
            level,
            self.atlas_layers().to_vec(),
            atlas,
        ))
    }
}

/// Resolves one source per layer while the shared cache deduplicates each path.
fn load_sources(
    layers: &[CharacterAtlasLayer],
    store: &mut AssetStore,
    cache: &mut BlpTextureCache,
) -> Result<Vec<Option<Arc<BlpTextureSource>>>, CharacterTextureComposeError> {
    let mut sources = Vec::with_capacity(layers.len());
    for layer in layers {
        if layer.kind() != CharacterAtlasLayerKind::Skin && !store.contains(layer.path())? {
            sources.push(None);
            continue;
        }
        sources.push(Some(cache.load(store, layer.path())?));
    }
    Ok(sources)
}

/// Allocates the selected complete top-through-one RGBA8 destination mip chain.
fn empty_atlas(component_level: CharacterComponentTextureLevel) -> Vec<CharacterAtlasMip> {
    let mut mips = Vec::with_capacity(component_level.mip_count());
    for level in 0..component_level.mip_count() {
        let size = component_level.atlas_size() >> level;
        let mut rgba8 = vec![0_u8; size as usize * size as usize * 4];
        for pixel in rgba8.as_chunks_mut::<4>().0 {
            pixel[3] = u8::MAX;
        }
        mips.push(CharacterAtlasMip::new(level, size, rgba8));
    }
    mips
}

/// Selects the same direct or one-level scale path as stock's region paste functions.
fn select_source_paste(
    layer: &CharacterAtlasLayer,
    source: &BlpTextureSource,
    component_level: CharacterComponentTextureLevel,
) -> Result<SourcePaste, CharacterTextureComposeError> {
    let atlas_size = component_level.atlas_size();
    let target = match layer.kind() {
        CharacterAtlasLayerKind::Skin => CharacterAtlasRect::atlas(atlas_size),
        CharacterAtlasLayerKind::Face
        | CharacterAtlasLayerKind::FacialHair
        | CharacterAtlasLayerKind::Hair
        | CharacterAtlasLayerKind::Underwear
        | CharacterAtlasLayerKind::Item => scaled_rect(layer.region().rect(), atlas_size, 0),
    };

    // Wow.exe 0x004F07D0/0x004F08A0 enter PasteScale at 0x004EF9D0 when
    // both source dimensions are below the destination. Its format-specific
    // loops at 0x004E89F0/0x004EC690/0x004ECC20/0x004ED200 expand exactly one
    // level, then resume the ordinary authored-mip paste chain.
    if source.width() < target.width() && source.height() < target.height() {
        if source.width().saturating_mul(2) == target.width()
            && source.height().saturating_mul(2) == target.height()
        {
            return Ok(SourcePaste::ScaleTop);
        }
        return Err(unsupported_source_scale(layer, source, target));
    }

    let mut level = 0;
    let mut width = source.width();
    while width > target.width() {
        width /= 2;
        level += 1;
    }
    if level >= source.mip_count() {
        return Err(CharacterTextureComposeError::MissingMip {
            path: layer.path().clone(),
            mip_level: level,
        });
    }
    Ok(SourcePaste::Direct { first_mip: level })
}

/// Pastes every authored source mip that maps onto the destination chain.
fn paste_layer(
    layer: &CharacterAtlasLayer,
    source: &BlpTextureSource,
    source_paste: SourcePaste,
    component_level: CharacterComponentTextureLevel,
    decoded: &mut HashMap<(AssetPath, usize), DecodedBlpTexture>,
    atlas: &mut [CharacterAtlasMip],
) -> Result<(), CharacterTextureComposeError> {
    for (atlas_level, destination) in atlas.iter_mut().enumerate() {
        let (mip_level, scaled) = match source_paste {
            SourcePaste::Direct { first_mip } => (first_mip + atlas_level, false),
            SourcePaste::ScaleTop if atlas_level == 0 => (0, true),
            SourcePaste::ScaleTop => (atlas_level - 1, false),
        };
        // Stock's paste loop terminates at the source mip count; it does not
        // synthesize missing authored levels or retry another texture.
        if mip_level >= source.mip_count() {
            break;
        }
        let source_pixels = decoded_mip(decoded, layer.path(), source, mip_level)?;
        let destination_rect = scaled_rect(
            layer.region().rect(),
            component_level.atlas_size(),
            atlas_level,
        );
        let source_rect = match layer.kind() {
            CharacterAtlasLayerKind::Skin if scaled => half_rect(destination_rect),
            CharacterAtlasLayerKind::Skin => destination_rect,
            CharacterAtlasLayerKind::Face
            | CharacterAtlasLayerKind::FacialHair
            | CharacterAtlasLayerKind::Hair
            | CharacterAtlasLayerKind::Underwear
            | CharacterAtlasLayerKind::Item => {
                let extent = if scaled {
                    half_rect(destination_rect)
                } else {
                    destination_rect
                };
                CharacterAtlasRect::new(0, 0, extent.width(), extent.height())
            }
        };
        validate_source_bounds(layer.path(), source_pixels, source_rect)?;
        if scaled {
            blend_scaled_rect(
                layer.kind(),
                source_pixels,
                source_rect,
                destination,
                destination_rect,
            );
        } else {
            blend_rect(
                layer.kind(),
                source_pixels,
                source_rect,
                destination,
                destination_rect,
            );
        }
    }
    Ok(())
}

/// Returns one locally shared decode, populating it on first use.
fn decoded_mip<'decoded>(
    decoded: &'decoded mut HashMap<(AssetPath, usize), DecodedBlpTexture>,
    path: &AssetPath,
    source: &BlpTextureSource,
    mip_level: usize,
) -> Result<&'decoded DecodedBlpTexture, CharacterTextureComposeError> {
    let key = (path.clone(), mip_level);
    let texture = match decoded.entry(key) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => entry.insert(source.decode_mip(mip_level)?),
    };
    Ok(texture)
}

/// Scales one 256-unit stock region into a selected destination atlas mip.
fn scaled_rect(rect: CharacterAtlasRect, atlas_size: u32, mip_level: usize) -> CharacterAtlasRect {
    let scale = atlas_size / 256;
    CharacterAtlasRect::new(
        rect.x().saturating_mul(scale) >> mip_level,
        rect.y().saturating_mul(scale) >> mip_level,
        (rect.width().saturating_mul(scale) >> mip_level).max(1),
        (rect.height().saturating_mul(scale) >> mip_level).max(1),
    )
}

/// Returns the source rectangle consumed by stock's exact two-times scaler.
fn half_rect(rect: CharacterAtlasRect) -> CharacterAtlasRect {
    CharacterAtlasRect::new(
        rect.x() / 2,
        rect.y() / 2,
        (rect.width() / 2).max(1),
        (rect.height() / 2).max(1),
    )
}

fn unsupported_source_scale(
    layer: &CharacterAtlasLayer,
    source: &BlpTextureSource,
    target: CharacterAtlasRect,
) -> CharacterTextureComposeError {
    CharacterTextureComposeError::SourceScalingRequired {
        path: layer.path().clone(),
        source_width: source.width(),
        source_height: source.height(),
        target_width: target.width(),
        target_height: target.height(),
    }
}

/// Rejects malformed or dimensionally inconsistent authored mip data.
fn validate_source_bounds(
    path: &AssetPath,
    source: &DecodedBlpTexture,
    rect: CharacterAtlasRect,
) -> Result<(), CharacterTextureComposeError> {
    if rect.x() + rect.width() <= source.width() && rect.y() + rect.height() <= source.height() {
        return Ok(());
    }
    Err(CharacterTextureComposeError::SourceRegionOutOfBounds {
        path: path.clone(),
        mip_level: source.mip_level(),
        source_width: source.width(),
        source_height: source.height(),
        x: rect.x(),
        y: rect.y(),
        width: rect.width(),
        height: rect.height(),
    })
}

/// Copies opaque skin or alpha-blends an overlay into one destination region.
fn blend_rect(
    kind: CharacterAtlasLayerKind,
    source: &DecodedBlpTexture,
    source_rect: CharacterAtlasRect,
    destination: &mut CharacterAtlasMip,
    destination_rect: CharacterAtlasRect,
) {
    let source_stride = source.width() as usize * 4;
    let destination_stride = destination.width() as usize * 4;
    for row in 0..source_rect.height() as usize {
        let source_start =
            (source_rect.y() as usize + row) * source_stride + source_rect.x() as usize * 4;
        let destination_start = (destination_rect.y() as usize + row) * destination_stride
            + destination_rect.x() as usize * 4;
        for column in 0..source_rect.width() as usize {
            let source_index = source_start + column * 4;
            let destination_index = destination_start + column * 4;
            let source_pixel = &source.rgba8()[source_index..source_index + 4];
            let destination_pixel =
                &mut destination.rgba8_mut()[destination_index..destination_index + 4];
            if kind == CharacterAtlasLayerKind::Skin {
                destination_pixel[..3].copy_from_slice(&source_pixel[..3]);
            } else {
                alpha_blend(source_pixel, destination_pixel);
            }
            destination_pixel[3] = u8::MAX;
        }
    }
}

/// Expands one authored mip by exactly two while pasting into the top atlas.
fn blend_scaled_rect(
    kind: CharacterAtlasLayerKind,
    source: &DecodedBlpTexture,
    source_rect: CharacterAtlasRect,
    destination: &mut CharacterAtlasMip,
    destination_rect: CharacterAtlasRect,
) {
    let source_stride = source.width() as usize * 4;
    let destination_stride = destination.width() as usize * 4;
    for row in 0..destination_rect.height() as usize {
        let source_y = row / 2;
        let next_source_y = (source_y + 1).min(source_rect.height() as usize - 1);
        for column in 0..destination_rect.width() as usize {
            let source_x = column / 2;
            let next_source_x = (source_x + 1).min(source_rect.width() as usize - 1);
            let pixel = |x: usize, y: usize| {
                let index = (source_rect.y() as usize + y) * source_stride
                    + (source_rect.x() as usize + x) * 4;
                &source.rgba8()[index..index + 4]
            };
            let upper_left = pixel(source_x, source_y);
            let upper_right = pixel(next_source_x, source_y);
            let lower_left = pixel(source_x, next_source_y);
            let lower_right = pixel(next_source_x, next_source_y);
            let mut sampled = [0_u8; 4];
            for channel in 0..4 {
                sampled[channel] = match (column & 1, row & 1) {
                    (0, 0) => upper_left[channel],
                    (1, 0) => {
                        ((u16::from(upper_left[channel]) + u16::from(upper_right[channel])) / 2)
                            as u8
                    }
                    (0, 1) => {
                        ((u16::from(upper_left[channel]) + u16::from(lower_left[channel])) / 2)
                            as u8
                    }
                    (1, 1) => {
                        ((u16::from(upper_left[channel])
                            + u16::from(upper_right[channel])
                            + u16::from(lower_left[channel])
                            + u16::from(lower_right[channel]))
                            / 4) as u8
                    }
                    _ => unreachable!(),
                };
            }
            let destination_index = (destination_rect.y() as usize + row) * destination_stride
                + (destination_rect.x() as usize + column) * 4;
            let destination_pixel =
                &mut destination.rgba8_mut()[destination_index..destination_index + 4];
            if kind == CharacterAtlasLayerKind::Skin {
                destination_pixel[..3].copy_from_slice(&sampled[..3]);
            } else {
                alpha_blend(&sampled, destination_pixel);
            }
            destination_pixel[3] = u8::MAX;
        }
    }
}

/// Applies stock's straight-alpha integer blend to one RGBA8 pixel.
fn alpha_blend(source: &[u8], destination: &mut [u8]) {
    let alpha = u32::from(source[3]);
    let inverse_alpha = u32::from(u8::MAX) - alpha;
    for channel in 0..3 {
        // The format-specific Paste/PasteScale loops at Wow.exe
        // 0x004E84F0/0x004EC690 (and their 4/8-bit-alpha siblings) use a
        // right shift here: the divisor is 256, not a normalized 255.
        destination[channel] = ((u32::from(source[channel]) * alpha
            + u32::from(destination[channel]) * inverse_alpha)
            >> 8) as u8;
    }
}
