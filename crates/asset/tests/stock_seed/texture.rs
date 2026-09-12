//! External stock-compatibility tests for `asset/texture`.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetStore, BlpBlockCompression, BlpTextureSource,
    ClientDataRoot, DecodedBlpTexture, Locale,
};

use crate::support::{Fixture, FixtureFile};

/// BLP2 raw pixels decode through the same archive-priority boundary as stock.
#[test]
fn blp_top_mip_decodes_to_resource_sized_rgba8() -> Result<(), Box<dyn Error>> {
    let blp = raw3_blp(2, 1, &[0xFFFF_0000, 0xFF00_FF00]);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "patch-A.MPQ",
        path: "Interface\\Icons\\Solarity.blp",
        bytes: &blp,
    }])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Interface/Icons/Solarity.blp")?;
    let texture = DecodedBlpTexture::load(&mut store, &path)?;

    assert_eq!((texture.width(), texture.height()), (2, 1));
    assert_eq!(
        texture.rgba8(),
        &[0xFF, 0x00, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF]
    );
    assert_eq!(
        texture.source().relative_path().to_string_lossy(),
        "patch-A.MPQ"
    );
    Ok(())
}

/// Stock format 2 retains every BGRA channel, including fractional alpha.
#[test]
fn blp_native_bgra8_format_preserves_alpha_and_mips() -> Result<(), Box<dyn Error>> {
    // 0x4b5fe0 accepts native format 2; 0x6affd0 uses RAW3 mip bytes directly.
    // The installed UI-PaidCharacterCustomization-Button.blp has this header.
    let blp = raw3_blp_mips(&[
        (2, 2, &[0x0012_3456, 0x7876_5432, 0xABCD_EF01, 0xFF98_7654]),
        (1, 1, &[0x42FE_DCBA]),
    ]);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "patch-A.MPQ",
        path: "Interface\\Buttons\\NativeBgra.blp",
        bytes: &blp,
    }])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Interface/Buttons/NativeBgra.blp")?;
    let source = BlpTextureSource::load(&mut store, &path)?;
    assert_eq!(source.block_compression(), None);
    assert_eq!(source.mip_count(), 2);
    assert_eq!(
        source.decode_mip(0)?.rgba8(),
        &[
            0x12, 0x34, 0x56, 0x00, 0x76, 0x54, 0x32, 0x78, 0xCD, 0xEF, 0x01, 0xAB, 0x98, 0x76,
            0x54, 0xFF,
        ]
    );
    assert_eq!(source.decode_mip(1)?.rgba8(), &[0xFE, 0xDC, 0xBA, 0x42]);
    assert_eq!(store.read(&path)?.bytes(), blp);
    Ok(())
}

/// The glare's high alpha flag does not change native format-2 BGRA pixels.
#[test]
fn blp_glare_alpha_flag_preserves_authored_transparency() -> Result<(), Box<dyn Error>> {
    let mut blp = raw3_blp(2, 1, &[0x0012_3456, 0x78FE_DCBA]);
    // Installed sunGlare.blp is RAW3, alpha byte 0x88, pixel format 2.
    // Native 4B5FE0 selects BGRA8 independently of the alpha byte for format 2.
    blp[9] = 0x88;
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Textures/SunGlare.blp",
        bytes: &blp,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let path = AssetPath::new("Textures/SunGlare.blp")?;
    let source = BlpTextureSource::load(&mut store, &path)?;
    assert_eq!(source.mip_count(), 1);
    assert_eq!(
        source.decode_mip(0)?.rgba8(),
        &[0x12, 0x34, 0x56, 0, 0xFE, 0xDC, 0xBA, 0x78]
    );
    assert_eq!(store.read(&path)?.bytes(), blp);
    Ok(())
}

/// The native BGRA adapter does not bypass content or mip bounds validation.
#[test]
fn blp_native_bgra8_rejects_truncated_and_unsupported_input() -> Result<(), Box<dyn Error>> {
    let mut truncated = raw3_blp(1, 1, &[0xFF12_3456]);
    truncated.pop();
    let unsupported_dxt = dxt_blp_mips(2, 8, 2, &[(4, 4, &[0; 16])]);
    for blp in [truncated, unsupported_dxt] {
        let fixture = Fixture::new(&[FixtureFile {
            archive: "patch-A.MPQ",
            path: "Interface\\Buttons\\Invalid.blp",
            bytes: &blp,
        }])?;
        let catalog =
            ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
        let mut store = AssetStore::mount(catalog)?;
        assert!(matches!(
            BlpTextureSource::load(
                &mut store,
                &AssetPath::new("Interface/Buttons/Invalid.blp")?
            ),
            Err(AssetError::TextureDecode { .. })
        ));
    }
    Ok(())
}

/// Authored DXT blocks remain available for direct BC image residency.
#[test]
fn blp_source_borrows_authored_block_compressed_mips() -> Result<(), Box<dyn Error>> {
    let top = [0x00, 0xF8, 0x00, 0xF8, 0, 0, 0, 0];
    let blp = dxt_blp_mips(2, 0, 0, &[(4, 4, &top)]);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "patch-A.MPQ",
        path: "Textures\\SolarityBlocks.blp",
        bytes: &blp,
    }])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Textures/SolarityBlocks.blp")?;
    let source = BlpTextureSource::load(&mut store, &path)?;

    assert_eq!(source.block_compression(), Some(BlpBlockCompression::Bc1));
    let top_mip = source.block_mip(0).ok_or("missing top BC mip")?;
    assert_eq!((top_mip.width(), top_mip.height()), (4, 4));
    assert_eq!(top_mip.bytes(), top);
    assert_eq!(top_mip.upload_byte_count(), 8);
    assert!(source.block_mip(1).is_none());
    Ok(())
}

/// A complete narrow DXT mip must retain both horizontal blocks, not pixel-area/16.
#[test]
fn blp_narrow_dxt_mips_preserve_every_authored_block() -> Result<(), Box<dyn Error>> {
    for (alpha, kind, block_size) in [(0, 0, 8), (8, 1, 16), (8, 7, 16)] {
        let block = |color: [u8; 2]| {
            let mut bytes = if kind == 1 {
                vec![255; 8]
            } else if kind == 7 {
                vec![255, 255, 0, 0, 0, 0, 0, 0]
            } else {
                Vec::new()
            };
            bytes.extend_from_slice(&[color[0], color[1], color[0], color[1], 0, 0, 0, 0]);
            bytes
        };
        let mut blocks = block([0, 0xF8]);
        blocks.extend(block([0xE0, 0x07]));
        let blp = dxt_blp_mips(2, alpha, kind, &[(8, 2, &blocks)]);
        let fixture = Fixture::new(&[FixtureFile {
            archive: "common.MPQ",
            path: "Textures/Narrow.blp",
            bytes: &blp,
        }])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let source = BlpTextureSource::load(&mut store, &AssetPath::new("Textures/Narrow.blp")?)?;
        let mip = source.block_mip(0).ok_or("missing DXT mip")?;
        assert_eq!(mip.bytes(), blocks);
        assert_eq!(mip.upload_byte_count(), block_size * 2);
        let pixels = source.decode_mip(0)?;
        for (index, pixel) in pixels.rgba8().as_chunks::<4>().0.iter().enumerate() {
            assert_eq!(
                pixel,
                if index % 8 < 4 {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 255, 0, 255]
                }
            );
        }
    }
    Ok(())
}

/// Stock narrow one-block tails are admitted; truncated files and body mips fail.
#[test]
fn blp_dxt_tail_validation_distinguishes_authored_sizes_from_truncation()
-> Result<(), Box<dyn Error>> {
    let block = [0, 0xF8, 0, 0xF8, 0, 0, 0, 0];
    let top = block.repeat(4);
    let good = dxt_blp_mips(2, 0, 0, &[(16, 4, &top), (8, 2, &block)]);
    let mut truncated = good.clone();
    truncated.pop();
    let mut outside = good.clone();
    outside[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
    let incomplete_top = dxt_blp_mips(2, 0, 0, &[(16, 4, &block)]);
    let partial_block = dxt_blp_mips(2, 0, 0, &[(16, 4, &top), (8, 2, &block[..7])]);
    for (bytes, valid) in [
        (good, true),
        (truncated, false),
        (outside, false),
        (incomplete_top, false),
        (partial_block, false),
    ] {
        let fixture = Fixture::new(&[FixtureFile {
            archive: "common.MPQ",
            path: "Textures/Tail.blp",
            bytes: &bytes,
        }])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let result = BlpTextureSource::load(&mut store, &AssetPath::new("Textures/Tail.blp")?);
        if valid {
            let source = result?;
            assert_eq!(source.mip_count(), 2);
            let mip = source.block_mip(1).ok_or("missing narrow tail")?;
            assert_eq!(mip.bytes(), block);
            assert_eq!(mip.upload_byte_count(), 16);
            assert_eq!(source.decode_mip(1)?.rgba8().len(), 8 * 2 * 4);
        } else {
            assert!(matches!(result, Err(AssetError::TextureDecode { .. })));
        }
    }
    Ok(())
}

/// Parsed BLP sources retain compressed authored mips and decode one on demand.
#[test]
fn blp_source_decodes_authored_mips_without_top_level_expansion() -> Result<(), Box<dyn Error>> {
    let blp = raw3_blp_mips(&[
        (2, 2, &[0xFFFF_0000, 0xFFFF_0000, 0xFFFF_0000, 0xFFFF_0000]),
        (1, 1, &[0xFF00_FF00]),
    ]);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "patch-A.MPQ",
        path: "Character\\Solarity\\Layer.blp",
        bytes: &blp,
    }])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Character/Solarity/Layer.blp")?;
    let source = BlpTextureSource::load(&mut store, &path)?;
    assert_eq!(source.path(), &path);

    assert_eq!((source.width(), source.height()), (2, 2));
    assert_eq!(source.mip_count(), 2);
    assert_eq!(source.decoded_rgba8_byte_count()?, 20);
    assert_eq!(source.mip_dimensions(1), Some((1, 1)));
    assert_eq!(source.mip_dimensions(2), None);
    let mip = source.decode_mip(1)?;
    assert_eq!(mip.mip_level(), 1);
    assert_eq!((mip.width(), mip.height()), (1, 1));
    assert_eq!(mip.rgba8(), &[0x00, 0xFF, 0x00, 0xFF]);
    Ok(())
}

/// Builds a minimal BLP2/RAW3 file with one internal top mip.
pub(crate) fn raw3_blp(width: u32, height: u32, bgra_pixels: &[u32]) -> Vec<u8> {
    raw3_blp_mips(&[(width, height, bgra_pixels)])
}

/// Builds a minimal BLP2/RAW3 file with the supplied authored mip chain.
pub(crate) fn raw3_blp_mips(mips: &[(u32, u32, &[u32])]) -> Vec<u8> {
    const HEADER_SIZE: u32 = 148;
    const PALETTE_SIZE: u32 = 256 * 4;
    const PIXEL_OFFSET: u32 = HEADER_SIZE + PALETTE_SIZE;

    let (width, height, _) = mips.first().copied().unwrap_or((0, 0, &[]));
    let mut offsets = [0_u32; 16];
    let mut sizes = [0_u32; 16];
    let mut next_offset = PIXEL_OFFSET;
    for (index, (_width, _height, pixels)) in mips.iter().take(16).enumerate() {
        let byte_size = u32::try_from(pixels.len().saturating_mul(4)).unwrap_or(u32::MAX);
        offsets[index] = next_offset;
        sizes[index] = byte_size;
        next_offset = next_offset.saturating_add(byte_size);
    }

    let mut bytes = Vec::with_capacity(next_offset as usize);
    bytes.extend_from_slice(b"BLP2");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&[3, 8, 2, u8::from(mips.len() > 1)]);
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    for offset in offsets {
        bytes.extend_from_slice(&offset.to_le_bytes());
    }
    for size in sizes {
        bytes.extend_from_slice(&size.to_le_bytes());
    }
    bytes.resize(PIXEL_OFFSET as usize, 0);
    for (_width, _height, pixels) in mips.iter().take(16) {
        for pixel in *pixels {
            bytes.extend_from_slice(&pixel.to_le_bytes());
        }
    }
    bytes
}

/// Builds a minimal BLP2/DXT file with internal authored mip blocks.
pub(crate) fn dxt_blp_mips(
    compression: u8,
    alpha_bits: u8,
    alpha_type: u8,
    mips: &[(u32, u32, &[u8])],
) -> Vec<u8> {
    const HEADER_SIZE: u32 = 148;
    const PALETTE_SIZE: u32 = 256 * 4;
    const BLOCK_OFFSET: u32 = HEADER_SIZE + PALETTE_SIZE;

    let (width, height, _) = mips.first().copied().unwrap_or((0, 0, &[]));
    let mut offsets = [0_u32; 16];
    let mut sizes = [0_u32; 16];
    let mut next_offset = BLOCK_OFFSET;
    for (index, (_width, _height, blocks)) in mips.iter().take(16).enumerate() {
        let byte_size = u32::try_from(blocks.len()).unwrap_or(u32::MAX);
        offsets[index] = next_offset;
        sizes[index] = byte_size;
        next_offset = next_offset.saturating_add(byte_size);
    }

    let mut bytes = Vec::with_capacity(next_offset as usize);
    bytes.extend_from_slice(b"BLP2");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&[
        compression,
        alpha_bits,
        alpha_type,
        u8::from(mips.len() > 1),
    ]);
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    for offset in offsets {
        bytes.extend_from_slice(&offset.to_le_bytes());
    }
    for size in sizes {
        bytes.extend_from_slice(&size.to_le_bytes());
    }
    bytes.resize(BLOCK_OFFSET as usize, 0);
    for (_width, _height, blocks) in mips.iter().take(16) {
        bytes.extend_from_slice(blocks);
    }
    bytes
}
