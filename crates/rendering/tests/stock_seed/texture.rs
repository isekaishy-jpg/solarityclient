//! External stock-compatibility tests for authored texture residency.

#![allow(unsafe_code)]

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureSource, ClientDataRoot, Locale,
};
use solarity_rendering::{
    BlpColorSpace, BlpTextureSourceKind, BlpTextureStorage, BlpTextureUploadRequest,
    VulkanBootstrap,
};

use crate::support::{Fixture, FixtureFile};

/// Authored DXT reaches sampled BC images without temporary RGBA expansion.
#[test]
fn authored_dxt_upload_retains_bc_storage() -> Result<(), Box<dyn Error>> {
    let bc1 = dxt_blp(4, 4, 0, 0, &[0x00, 0xF8, 0x00, 0xF8, 0, 0, 0, 0]);
    let bc2 = dxt_blp(4, 4, 1, 1, &[0; 16]);
    let bc3 = dxt_blp(4, 4, 8, 7, &[0; 16]);
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Textures\\DirectBc1.blp",
            bytes: &bc1,
        },
        FixtureFile {
            path: "Textures\\DirectBc2.blp",
            bytes: &bc2,
        },
        FixtureFile {
            path: "Textures\\DirectBc3.blp",
            bytes: &bc3,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let paths = [
        AssetPath::new("Textures/DirectBc1.blp")?,
        AssetPath::new("Textures/DirectBc2.blp")?,
        AssetPath::new("Textures/DirectBc3.blp")?,
    ];
    let sources = paths
        .iter()
        .map(|path| BlpTextureSource::load(&mut store, path))
        .collect::<Result<Vec<_>, _>>()?;

    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let mut window_builder = video.window("Solarity BC texture upload test", 64, 64);
    window_builder.vulkan().hidden();
    let window = window_builder.build()?;
    let extensions = window.vulkan_instance_extensions()?;
    let bootstrap = VulkanBootstrap::start(&extensions)?;
    // SAFETY: The bootstrap enabled this live window's exact extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: SDL transfers this exact surface's ownership to the renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let requests = [
        BlpTextureUploadRequest::new(&sources[0], BlpColorSpace::Srgb),
        BlpTextureUploadRequest::new(&sources[1], BlpColorSpace::Srgb),
        BlpTextureUploadRequest::new(&sources[0], BlpColorSpace::Srgb),
        BlpTextureUploadRequest::new(&sources[2], BlpColorSpace::Srgb),
    ];
    assert_eq!(renderer.blp_texture_upload_submission_count(), 0);
    assert!(renderer.upload_blp_textures(&[])?.is_empty());
    assert_eq!(renderer.blp_texture_upload_submission_count(), 0);
    let handles = renderer.upload_blp_textures(&requests)?;
    assert_eq!(renderer.blp_texture_upload_submission_count(), 1);
    assert_eq!(handles[0], handles[2]);
    assert_eq!(renderer.upload_blp_textures(&requests)?, handles);
    assert_eq!(renderer.blp_texture_upload_submission_count(), 1);

    for (source, handle, storage, byte_count) in [
        (&sources[0], handles[0], BlpTextureStorage::Bc1, 8),
        (&sources[1], handles[1], BlpTextureStorage::Bc2, 16),
        (&sources[2], handles[3], BlpTextureStorage::Bc3, 16),
    ] {
        let info = renderer
            .blp_texture_info(handle)
            .ok_or("uploaded BC image is absent")?;
        assert_eq!(info.path(), source.path());
        assert_eq!(info.source_kind(), BlpTextureSourceKind::Authored);
        assert_eq!(info.color_space(), BlpColorSpace::Srgb);
        assert_eq!(info.storage(), storage);
        assert_eq!(info.extent(), (4, 4));
        assert_eq!(info.mip_count(), 1);
        assert_eq!(info.upload_byte_count(), byte_count);
    }
    renderer.shutdown()?;
    Ok(())
}

/// Builds one minimal BLP2 DXT image with its mandatory palette area.
fn dxt_blp(width: u32, height: u32, alpha_bits: u8, alpha_type: u8, blocks: &[u8]) -> Vec<u8> {
    const HEADER_SIZE: u32 = 148;
    const PALETTE_SIZE: u32 = 256 * 4;
    const BLOCK_OFFSET: u32 = HEADER_SIZE + PALETTE_SIZE;

    let mut bytes = Vec::with_capacity(BLOCK_OFFSET as usize + blocks.len());
    bytes.extend_from_slice(b"BLP2");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&[2, alpha_bits, alpha_type, 0]);
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&BLOCK_OFFSET.to_le_bytes());
    bytes.resize(bytes.len() + 15 * size_of::<u32>(), 0);
    bytes.extend_from_slice(&(blocks.len() as u32).to_le_bytes());
    bytes.resize(bytes.len() + 15 * size_of::<u32>(), 0);
    bytes.resize(BLOCK_OFFSET as usize, 0);
    bytes.extend_from_slice(blocks);
    bytes
}
