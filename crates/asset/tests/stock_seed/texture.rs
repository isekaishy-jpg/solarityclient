//! External stock-compatibility tests for `asset/texture`.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedBlpTexture, Locale,
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

/// Builds a minimal BLP2/RAW3 file with one internal top mip.
fn raw3_blp(width: u32, height: u32, bgra_pixels: &[u32]) -> Vec<u8> {
    const HEADER_SIZE: u32 = 148;
    const PALETTE_SIZE: u32 = 256 * 4;
    const PIXEL_OFFSET: u32 = HEADER_SIZE + PALETTE_SIZE;

    let byte_size = u32::try_from(bgra_pixels.len().saturating_mul(4)).unwrap_or(u32::MAX);
    let mut bytes = Vec::with_capacity(PIXEL_OFFSET as usize + byte_size as usize);
    bytes.extend_from_slice(b"BLP2");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&[3, 8, 8, 0]);
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&PIXEL_OFFSET.to_le_bytes());
    for _unused in 1..16 {
        bytes.extend_from_slice(&0_u32.to_le_bytes());
    }
    bytes.extend_from_slice(&byte_size.to_le_bytes());
    for _unused in 1..16 {
        bytes.extend_from_slice(&0_u32.to_le_bytes());
    }
    bytes.resize(PIXEL_OFFSET as usize, 0);
    for pixel in bgra_pixels {
        bytes.extend_from_slice(&pixel.to_le_bytes());
    }
    bytes
}
