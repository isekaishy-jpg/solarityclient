//! Archive-backed CPU composition of stock character atlas mip levels.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

use solarity_asset::{AssetPath, AssetStore, BlpTextureCache, BlpTextureSource, DecodedBlpTexture};

use super::types::STOCK_CHARACTER_ATLAS_SIZE;
use super::{
    CharacterAtlasLayer, CharacterAtlasLayerKind, CharacterAtlasMip, CharacterAtlasRect,
    CharacterAtlasTexture, CharacterTextureComposeError, CharacterTexturePlan,
};

const STOCK_CHARACTER_ATLAS_MIP_COUNT: usize = 9;

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
    /// is missing, an available source is malformed, its authored mip cannot
    /// cover the destination region, or stock's smaller-source scaling path
    /// would be required. Stock omits missing overlay handles after its texture
    /// cache lookup, so optional overlay files are skipped here as well.
    pub fn compose(
        &self,
        store: &mut AssetStore,
        cache: &mut BlpTextureCache,
    ) -> Result<CharacterAtlasTexture, CharacterTextureComposeError> {
        let sources = load_sources(self.atlas_layers(), store, cache)?;
        let mut decoded = HashMap::new();
        let mut atlas = empty_atlas();

        for (layer, source) in self
            .atlas_layers()
            .iter()
            .zip(&sources)
            .filter_map(|(layer, source)| source.as_ref().map(|source| (layer, source)))
        {
            let source_mip = select_source_mip(layer, source)?;
            paste_layer(layer, source, source_mip, &mut decoded, &mut atlas)?;
        }
        Ok(CharacterAtlasTexture::new(atlas))
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

/// Allocates the complete 256-through-1 RGBA8 destination mip chain.
fn empty_atlas() -> Vec<CharacterAtlasMip> {
    let mut mips = Vec::with_capacity(STOCK_CHARACTER_ATLAS_MIP_COUNT);
    for level in 0..STOCK_CHARACTER_ATLAS_MIP_COUNT {
        let size = STOCK_CHARACTER_ATLAS_SIZE >> level;
        let mut rgba8 = vec![0_u8; size as usize * size as usize * 4];
        for pixel in rgba8.as_chunks_mut::<4>().0 {
            pixel[3] = u8::MAX;
        }
        mips.push(CharacterAtlasMip::new(level, size, rgba8));
    }
    mips
}

/// Selects the same starting authored mip as stock's region paste functions.
fn select_source_mip(
    layer: &CharacterAtlasLayer,
    source: &BlpTextureSource,
) -> Result<usize, CharacterTextureComposeError> {
    let target = match layer.kind() {
        CharacterAtlasLayerKind::Skin => CharacterAtlasRect::atlas(),
        CharacterAtlasLayerKind::Face
        | CharacterAtlasLayerKind::FacialHair
        | CharacterAtlasLayerKind::Hair
        | CharacterAtlasLayerKind::Underwear
        | CharacterAtlasLayerKind::Item => layer.region().rect(),
    };

    // PasteScale is a real stock path, but its filtering algorithm is not yet
    // recovered. Reject it explicitly instead of substituting a guessed filter.
    if source.width() < target.width() && source.height() < target.height() {
        return Err(CharacterTextureComposeError::SourceScalingRequired {
            path: layer.path().clone(),
            source_width: source.width(),
            source_height: source.height(),
            target_width: target.width(),
            target_height: target.height(),
        });
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
    Ok(level)
}

/// Pastes every authored source mip that maps onto the destination chain.
fn paste_layer(
    layer: &CharacterAtlasLayer,
    source: &BlpTextureSource,
    source_mip: usize,
    decoded: &mut HashMap<(AssetPath, usize), DecodedBlpTexture>,
    atlas: &mut [CharacterAtlasMip],
) -> Result<(), CharacterTextureComposeError> {
    for (atlas_level, destination) in atlas.iter_mut().enumerate() {
        let mip_level = source_mip + atlas_level;
        // Stock's paste loop terminates at the source mip count; it does not
        // synthesize missing authored levels or retry another texture.
        if mip_level >= source.mip_count() {
            break;
        }
        let source_pixels = decoded_mip(decoded, layer.path(), source, mip_level)?;
        let destination_rect = scaled_rect(layer.region().rect(), atlas_level);
        let source_rect = match layer.kind() {
            CharacterAtlasLayerKind::Skin => destination_rect,
            CharacterAtlasLayerKind::Face
            | CharacterAtlasLayerKind::FacialHair
            | CharacterAtlasLayerKind::Hair
            | CharacterAtlasLayerKind::Underwear
            | CharacterAtlasLayerKind::Item => {
                CharacterAtlasRect::new(0, 0, destination_rect.width(), destination_rect.height())
            }
        };
        validate_source_bounds(layer.path(), source_pixels, source_rect)?;
        blend_rect(
            layer.kind(),
            source_pixels,
            source_rect,
            destination,
            destination_rect,
        );
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

/// Scales one stock-default region down to a destination atlas mip.
fn scaled_rect(rect: CharacterAtlasRect, mip_level: usize) -> CharacterAtlasRect {
    CharacterAtlasRect::new(
        rect.x() >> mip_level,
        rect.y() >> mip_level,
        (rect.width() >> mip_level).max(1),
        (rect.height() >> mip_level).max(1),
    )
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

/// Applies stock's straight-alpha integer blend to one RGBA8 pixel.
fn alpha_blend(source: &[u8], destination: &mut [u8]) {
    let alpha = u32::from(source[3]);
    let inverse_alpha = u32::from(u8::MAX) - alpha;
    for channel in 0..3 {
        destination[channel] = ((u32::from(source[channel]) * alpha
            + u32::from(destination[channel]) * inverse_alpha)
            / u32::from(u8::MAX)) as u8;
    }
}
