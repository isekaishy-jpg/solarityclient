//! Real archive, database, MH2O, and material-cache boundaries for liquid admission.

use std::{error::Error, sync::Arc};

use solarity_asset::{
    ArchiveCatalog, AssetStore, BlpTextureCache, ClientDataRoot, LightCatalog, Locale, MapCatalog,
    TerrainMap, TerrainTileIndex, WorldLightQuery,
};

use super::{
    LiquidAssetCache, ResidentLiquidSurface, liquid_depth_images, prepare_terrain_liquids,
};
use crate::test_support::{ClientFixture, SDL_TEST_LOCK, bootstrap_texture_blp};
use glam::{Mat4, Vec3, Vec4};
use solarity_rendering::{
    LiquidDepthTexture, LiquidDepthTextureKind, LiquidFog, LiquidFrame, LiquidLighting,
    M2LocalLightState, M2SceneUniform, TerrainSceneUniform, VulkanBootstrap, WorldCamera,
    WorldFrameScene, WorldFrustum, WorldModelSceneUniform, WorldModelTextureFiltering,
    WorldScreenWindow,
};

/// Worker-built geometry and its failed animated ordinal reach actual Vulkan pixels.
#[test]
#[allow(unsafe_code)] // SDL transfers this hidden test surface to the renderer.
fn liquid_resident_terrain_reaches_the_world_frame() -> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let mut store = mounted(&fixture)?;
    let maps = MapCatalog::load(&mut store)?;
    let map = TerrainMap::load(&mut store, maps.map(571).ok_or("missing map")?)?;
    let tile = map.load_tile(&mut store, TerrainTileIndex::new(32, 32).ok_or("tile")?)?;
    let batches = prepare_terrain_liquids(
        &tile,
        &mut LiquidAssetCache::default(),
        &mut BlpTextureCache::default(),
        &mut store,
    )?;
    let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity resident liquid test", 32, 32)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: Bootstrap enables the extensions required by this live SDL window.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: Sole surface ownership transfers and the window outlives the renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (32, 32), 0) }?;
    let mut materials = super::LiquidGpuMaterialCache::default();
    let prepared = materials.prepare_terrain(&mut renderer, &batches)?;
    let center = batches[0].origin + Vec3::new(-2.0, -2.0, 10.0);
    let camera = WorldCamera::orthographic(
        center + Vec3::Z * 50.0,
        center,
        Vec3::Y,
        [-1.0, 1.0],
        [-1.0, 1.0],
        0.1,
        100.0,
    )
    .frame(1.0)?;
    let frustum = WorldFrustum::new(camera, WorldScreenWindow::FULL)?;
    let draws = prepared
        .iter()
        .map(|batch| {
            batch.prepare_draw(
                &renderer,
                frustum,
                camera,
                LiquidLighting::new(-Vec3::Z, Vec3::ONE, Vec3::ZERO, Vec3::ZERO),
                LiquidFog::new(Vec3::new(0.0, 1.0, 1.0), Vec3::ZERO),
                700,
                false,
            )
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert!(!draws.is_empty());
    let depths = [
        LiquidDepthTextureKind::River,
        LiquidDepthTextureKind::Ocean,
        LiquidDepthTextureKind::WorldModel,
    ]
    .map(|kind| LiquidDepthTexture::prepare(kind, [0; 2], [255; 2]));
    let scene = WorldFrameScene::new(
        TerrainSceneUniform::new(Mat4::IDENTITY, Vec3::ONE, Vec3::ZERO, Vec3::Z),
        WorldModelSceneUniform::new(
            Mat4::IDENTITY,
            Vec3::ZERO,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            Vec4::ZERO,
        ),
        M2SceneUniform::new(
            Mat4::IDENTITY,
            Mat4::IDENTITY,
            Vec3::ZERO,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            Vec4::ZERO,
            Vec3::ZERO,
            [M2LocalLightState::disabled(); 4],
        ),
    )
    .with_liquids(
        LiquidFrame::new(&draws, &depths[0], &depths[1], &depths[2], 0)
            .with_texture_filtering(WorldModelTextureFiltering::Anisotropic4x),
    );
    renderer.request_frame_capture()?;
    let report =
        renderer.present_world_frame(scene, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
    assert_eq!(report.liquid_draw_count(), draws.len());
    let capture = renderer.take_captured_frame()?.ok_or("missing capture")?;
    for pixel in capture.rgba8().as_chunks::<4>().0 {
        assert_eq!(
            *pixel,
            [0, 255, 0, 255],
            "missing ordinal keeps stock opaque green"
        );
    }
    renderer.retire_liquid_meshes(
        &prepared
            .iter()
            .map(super::TerrainLiquidGpuBatch::mesh)
            .collect::<Vec<_>>(),
    )?;
    renderer.shutdown()?;
    Ok(())
}

/// Native 7EBFF0 projects the authored row before 8A2BF0/8A2AC0 generate all pixels.
#[test]
fn liquid_environment_matches_original_dbc_projection_and_depth_images()
-> Result<(), Box<dyn Error>> {
    const INPUT_SIZE: usize = (9 + 18 + 6) * 4;
    const RECORD_SIZE: usize = INPUT_SIZE + 3 * 2048;
    let captured = include_bytes!("../fixtures/liquid_environment.bin");
    assert_eq!(captured.len(), 3 * RECORD_SIZE);
    for record in captured.as_chunks::<RECORD_SIZE>().0 {
        let words: Vec<u32> = record[..INPUT_SIZE]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| u32::from_le_bytes(*bytes))
            .collect();
        let fixture = ClientFixture::with_common_files(&[
            (
                "DBFilesClient\\Light.dbc",
                &dbc(15, &[1, 571, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0], &[0]),
            ),
            ("DBFilesClient\\LightParams.dbc", &dbc(9, &words[..9], &[0])),
            ("DBFilesClient\\LightSkybox.dbc", &dbc(3, &[], &[0])),
            (
                "DBFilesClient\\LightIntBand.dbc",
                &constant_bands(&words[9..27]),
            ),
            (
                "DBFilesClient\\LightFloatBand.dbc",
                &constant_bands(&words[27..33]),
            ),
        ])?;
        let light = LightCatalog::load(&mut mounted(&fixture)?)?.sample(WorldLightQuery::new(
            571,
            glam::Vec3::ZERO,
            720,
        ))?;
        assert_eq!(light.glow().to_bits(), words[4]);
        assert_eq!(light.liquid_alphas().map(f32::to_bits), words[5..9]);
        for (kind, image) in liquid_depth_images(light).iter().enumerate() {
            let expected = &record[INPUT_SIZE + kind * 2048..INPUT_SIZE + (kind + 1) * 2048];
            for (pixel, (actual, bgra)) in image
                .pixels_rgba()
                .as_chunks::<4>()
                .0
                .iter()
                .zip(expected.as_chunks::<4>().0)
                .enumerate()
            {
                assert_eq!(
                    *actual,
                    [bgra[2], bgra[1], bgra[0], bgra[3]],
                    "depth kind {kind}, pixel {pixel}"
                );
            }
        }
    }
    Ok(())
}

/// A one-key band returns the captured provider word at every sample time.
fn constant_bands(values: &[u32]) -> Vec<u8> {
    let fields: Vec<u32> = values
        .iter()
        .enumerate()
        .flat_map(|(index, &value)| {
            let mut row = [0; 34];
            row[0] = index as u32 + 1;
            row[1] = 1;
            row[18] = value;
            row
        })
        .collect();
    dbc(34, &fields, &[0])
}

/// 8A2450 requests every ordinal; a missing 17th texture must not shorten the bank.
#[test]
fn liquid_animation_keeps_failed_slots_and_shares_live_materials() -> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let mut store = mounted(&fixture)?;
    let mut textures = BlpTextureCache::default();
    let mut cache = LiquidAssetCache::default();
    let first = cache.load(1, &mut textures, &mut store)?;
    let second = cache.load(1, &mut textures, &mut store)?;
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(first.surfaces.len(), 30);
    for (index, surface) in first.surfaces.iter().enumerate() {
        assert_eq!(
            matches!(surface, ResidentLiquidSurface::StockFailure),
            index == 16
        );
    }
    // 8A1B10 advances by integer arithmetic across the complete 1250 ms cycle.
    assert_eq!(first.timeline.frame_index(0), 0);
    assert_eq!(first.timeline.frame_index(700), 16);
    assert_eq!(first.timeline.frame_index(1249), 29);
    assert_eq!(first.timeline.frame_index(1250), 0);
    let weak = Arc::downgrade(&first);
    drop(first);
    drop(second);
    assert!(
        weak.upgrade().is_none(),
        "the cache must not retain departed generations"
    );
    assert_eq!(cache.load(1, &mut textures, &mut store)?.surfaces.len(), 30);
    Ok(())
}

/// Native 7CF200 groups four row-major chunks by liquid type, then starts a new batch.
#[test]
fn terrain_liquid_batches_preserve_chunk_order_and_strip_boundaries() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture()?;
    let mut store = mounted(&fixture)?;
    let maps = MapCatalog::load(&mut store)?;
    let map = TerrainMap::load(&mut store, maps.map(571).ok_or("missing map")?)?;
    let tile = map.load_tile(&mut store, TerrainTileIndex::new(32, 32).ok_or("tile")?)?;
    let mut textures = BlpTextureCache::default();
    let mut cache = LiquidAssetCache::default();
    let batches = prepare_terrain_liquids(&tile, &mut cache, &mut textures, &mut store)?;
    assert_eq!(batches.len(), 2);
    assert!(Arc::ptr_eq(&batches[0].material, &batches[1].material));
    assert_eq!(batches[0].vertices.len(), 16);
    assert_eq!(batches[1].vertices.len(), 4);
    let expected: Vec<u16> = (0..4)
        .flat_map(|member| [0, 0, 2, 1, 3, 3].map(|index| index + member * 4))
        .collect();
    assert_eq!(batches[0].indices, expected);
    // Give each member a distinct authored elevation to expose ordering mistakes.
    for (member, height) in [10.0, 11.0, 26.0, 27.0].into_iter().enumerate() {
        assert_eq!(
            batches[0].vertices[member * 4].position()[2] + batches[0].origin.z,
            height
        );
    }
    // Every nondegenerate triangle stays within its source MCNK's four vertices.
    let triangles = batches[0]
        .indices
        .windows(3)
        .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]);
    let mut count = 0;
    for triangle in triangles {
        assert_eq!(triangle[0] / 4, triangle[1] / 4);
        assert_eq!(triangle[1] / 4, triangle[2] / 4);
        count += 1;
    }
    assert_eq!(count, 8);
    Ok(())
}

/// Mounts only generated archives; no installed client state participates.
fn mounted(fixture: &ClientFixture) -> Result<AssetStore, Box<dyn Error>> {
    Ok(AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?)
}

/// Complete material sequence with one intentional native texture failure slot.
fn fixture() -> Result<ClientFixture, Box<dyn Error>> {
    let mut strings = b"\0XTextures\\river.%d.blp\0".to_vec();
    let depth = strings.len() as u32;
    strings.extend_from_slice(b"proceduralRiverDepthTex\0");
    let mut liquid = [0; 45];
    liquid[0] = 1;
    liquid[14] = 1;
    liquid[15] = 1;
    liquid[16] = depth;
    liquid[23] = 1.0_f32.to_bits();
    liquid[25] = 1.0_f32.to_bits();
    liquid[42] = 1250;
    let mut map = [0; 66];
    map[0] = 571;
    map[1] = 1;
    map[5] = 1;
    map[22] = 571;
    map[59] = u32::MAX;
    map[63] = 2;
    let mut manifest = wow_wdt::WdtFile::new(wow_wdt::version::WowVersion::WotLK);
    manifest.mwmo = Some(wow_wdt::chunks::MwmoChunk::new());
    manifest
        .main
        .get_mut(32, 32)
        .ok_or("tile")?
        .set_has_adt(true);
    let mut wdt = Vec::new();
    wow_wdt::WdtWriter::new(&mut wdt).write(&manifest)?;
    let mut adt = wow_adt::builder::AdtBuilder::new()
        .with_version(wow_adt::AdtVersion::WotLK)
        .add_texture("tileset/fixture/waterbed.blp")
        .build()?
        .to_bytes()?;
    let mut payload = vec![0_u8; 256 * 12];
    for chunk in [0, 1, 16, 17, 2] {
        append_layer(&mut payload, chunk);
    }
    adt.extend_from_slice(b"O2HM");
    adt.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    adt.extend_from_slice(&payload);
    let mut files = vec![
        (
            "DBFilesClient\\LiquidType.dbc".to_owned(),
            dbc(45, &liquid, &strings),
        ),
        (
            "DBFilesClient\\LiquidMaterial.dbc".to_owned(),
            dbc(3, &[1, 0, 1], &[0]),
        ),
        (
            "DBFilesClient\\Map.dbc".to_owned(),
            dbc(66, &map, b"\0Northrend\0"),
        ),
        ("World\\Maps\\Northrend\\Northrend.wdt".to_owned(), wdt),
        (
            "World\\Maps\\Northrend\\Northrend_32_32.adt".to_owned(),
            adt,
        ),
    ];
    for frame in (1..=30).filter(|&frame| frame != 17) {
        files.push((
            format!("XTextures\\river.{frame}.blp"),
            bootstrap_texture_blp(),
        ));
    }
    ClientFixture::with_common_files(
        &files
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
            .collect::<Vec<_>>(),
    )
}

/// Adds a one-cell height/depth layer through the exact MH2O wire representation.
fn append_layer(payload: &mut Vec<u8>, chunk: usize) {
    let offset = payload.len();
    payload[chunk * 12..chunk * 12 + 4].copy_from_slice(&(offset as u32).to_le_bytes());
    payload[chunk * 12 + 4..chunk * 12 + 8].copy_from_slice(&1_u32.to_le_bytes());
    payload.extend_from_slice(&1_u16.to_le_bytes());
    payload.extend_from_slice(&0_u16.to_le_bytes());
    let height = chunk as f32 + 10.0;
    payload.extend_from_slice(&height.to_le_bytes());
    payload.extend_from_slice(&height.to_le_bytes());
    payload.extend_from_slice(&[0, 0, 1, 1]);
    payload.extend_from_slice(&0_u32.to_le_bytes());
    payload.extend_from_slice(&((offset + 24) as u32).to_le_bytes());
    for _ in 0..4 {
        payload.extend_from_slice(&height.to_le_bytes());
    }
    payload.extend_from_slice(&[0, 85, 170, 255]);
}

/// Writes fixed-word WDBC records and their zero-terminated string bank.
fn dbc(fields: u32, values: &[u32], strings: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [
        values.len() as u32 / fields,
        fields,
        fields * 4,
        strings.len() as u32,
    ] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    for word in values {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(strings);
    bytes
}
