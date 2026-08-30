//! External stock-compatibility tests for `asset/texture`.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureSource, ClientDataRoot, DecodedBlpTexture,
    Locale,
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

    assert_eq!((source.width(), source.height()), (2, 2));
    assert_eq!(source.mip_count(), 2);
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
    bytes.extend_from_slice(&[3, 8, 8, u8::from(mips.len() > 1)]);
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
