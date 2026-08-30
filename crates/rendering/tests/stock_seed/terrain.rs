//! External stock-compatibility tests for terrain mesh preparation.

#![allow(unsafe_code)]

use std::error::Error;

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureSource, ClientDataRoot, Locale, MapCatalog,
    TerrainMap, TerrainTileIndex,
};
use solarity_rendering::{
    BlpColorSpace, TERRAIN_MATERIAL_ATLAS_BYTE_COUNT, TerrainChunkMeshPlan, TerrainLayerCount,
    TerrainSceneUniform, TerrainTextureSet, TerrainTileMeshPlan, VulkanBootstrap, WorldCamera,
    WorldFrustum, WorldScreenWindow,
};
use wow_adt::AdtVersion;
use wow_adt::builder::AdtBuilder;
use wow_wdt::chunks::MwmoChunk;
use wow_wdt::version::WowVersion;
use wow_wdt::{WdtFile, WdtWriter};

use crate::support::{Fixture, FixtureFile};

/// The 145-vertex MCNK grid becomes 256 stock fan triangles with no holes.
#[test]
fn terrain_chunk_mesh_preserves_staggered_topology() -> Result<(), Box<dyn Error>> {
    let map_table = map_table();
    let wdt = terrain_wdt()?;
    let grass = solid_raw3_blp(2, 2, 0xFF00_FF00);
    let adt = AdtBuilder::new()
        .with_version(AdtVersion::WotLK)
        .add_texture("tileset/fixture/grass.blp")
        .build()?
        .to_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\Map.dbc",
            bytes: &map_table,
        },
        FixtureFile {
            path: "World\\Maps\\Northrend\\Northrend.wdt",
            bytes: &wdt,
        },
        FixtureFile {
            path: "World\\Maps\\Northrend\\Northrend_32_32.adt",
            bytes: &adt,
        },
        FixtureFile {
            path: "tileset\\fixture\\grass.blp",
            bytes: &grass,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let maps = MapCatalog::load(&mut store)?;
    let definition = maps.map(571).ok_or("Northrend map is absent")?;
    let map = TerrainMap::load(&mut store, definition)?;
    let tile_index = TerrainTileIndex::new(32, 32).ok_or("fixture tile is invalid")?;
    let tile = map.load_tile(&mut store, tile_index)?;
    let grass_path = AssetPath::new("tileset\\fixture\\grass.blp")?;
    let grass_source = BlpTextureSource::load(&mut store, &grass_path)?;
    let chunk_index =
        solarity_asset::TerrainChunkIndex::new(0, 0).ok_or("fixture chunk is invalid")?;

    let mesh = TerrainChunkMeshPlan::prepare(&tile, chunk_index);
    assert_eq!(mesh.tile(), tile_index);
    assert_eq!(mesh.chunk(), chunk_index);
    assert_eq!(mesh.vertices().len(), 145);
    assert_eq!(mesh.indices().len(), 768);
    assert_eq!(mesh.triangle_count(), 256);
    assert_eq!(mesh.layers().len(), 1);
    assert!(mesh.indices().iter().all(|index| *index < 145));
    assert_eq!(mesh.vertices()[0].texture_coordinates(), [0.0, 0.0]);
    assert_eq!(mesh.vertices()[9].texture_coordinates(), [0.25, 0.25]);
    assert_eq!(mesh.vertices()[144].texture_coordinates(), [4.0, 4.0]);
    assert_eq!(
        mesh.vertices()[0].alpha_coordinates(),
        [0.5 / 64.0, 0.5 / 64.0]
    );
    assert_eq!(
        mesh.vertices()[144].alpha_coordinates(),
        [63.5 / 64.0, 63.5 / 64.0]
    );
    assert_eq!(
        mesh.vertex_bytes().len(),
        145 * solarity_rendering::TerrainRenderVertex::BYTE_SIZE
    );
    assert_eq!(mesh.index_bytes().len(), 768 * size_of::<u16>());
    assert_eq!(mesh.bounds(), [[-33.333_332, -33.333_332, 0.0], [0.0; 3]]);
    let visible = WorldCamera::stock(
        Vec3::new(10.0, -16.0, 2.0),
        Vec3::new(9.0, -16.0, 2.0),
        Vec3::Z,
        100.0,
    )
    .frame(1.0)?;
    let scene = TerrainSceneUniform::new(
        visible.view_projection(),
        Vec3::new(0.25, 0.3, 0.35),
        Vec3::new(0.75, 0.7, 0.65),
        Vec3::new(0.0, 0.0, 1.0),
    );
    let scene_bytes = scene.to_bytes();
    assert_eq!(scene_bytes.len(), TerrainSceneUniform::BYTE_SIZE);
    assert_eq!(f32::from_le_bytes(scene_bytes[64..68].try_into()?), 0.25);
    assert_eq!(f32::from_le_bytes(scene_bytes[68..72].try_into()?), 0.3);
    assert_eq!(f32::from_le_bytes(scene_bytes[72..76].try_into()?), 0.35);
    assert!(mesh.is_visible(WorldFrustum::new(visible, WorldScreenWindow::FULL)?)?);
    let hidden = WorldCamera::stock(
        Vec3::new(10.0, -16.0, 2.0),
        Vec3::new(11.0, -16.0, 2.0),
        Vec3::Z,
        100.0,
    )
    .frame(1.0)?;
    assert!(!mesh.is_visible(WorldFrustum::new(hidden, WorldScreenWindow::FULL)?)?);

    let tile_mesh = TerrainTileMeshPlan::prepare(&tile)?;
    assert_eq!(tile_mesh.tile(), tile_index);
    assert_eq!(tile_mesh.vertices().len(), 256 * 145);
    assert_eq!(tile_mesh.indices().len(), 256 * 768);
    assert_eq!(tile_mesh.chunks().len(), 256);
    assert_eq!(tile_mesh.chunks()[0].first_index(), 0);
    assert_eq!(tile_mesh.chunks()[0].index_count(), 768);
    assert_eq!(tile_mesh.chunks()[1].first_index(), 768);
    assert_eq!(tile_mesh.chunks()[0].atlas_chunk(), [0, 0]);
    assert_eq!(tile_mesh.chunks()[255].atlas_chunk(), [15, 15]);
    assert_eq!(tile_mesh.textures(), std::slice::from_ref(&grass_path));
    assert_eq!(tile_mesh.texture_flags(), Some([0_u32].as_slice()));
    assert_eq!(
        tile_mesh.vertex_bytes().len(),
        256 * 145 * solarity_rendering::TerrainRenderVertex::BYTE_SIZE
    );
    assert_eq!(tile_mesh.index_bytes().len(), 256 * 768 * 2);
    assert_eq!(
        tile_mesh.material_atlas_rgba().len(),
        TERRAIN_MATERIAL_ATLAS_BYTE_COUNT
    );
    assert!(
        tile_mesh
            .indices()
            .iter()
            .all(|index| usize::from(*index) < tile_mesh.vertices().len())
    );

    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let mut window_builder = video.window("Solarity terrain upload test", 64, 64);
    window_builder.vulkan().hidden();
    let window = window_builder.build()?;
    let extensions = window.vulkan_instance_extensions()?;
    let bootstrap = VulkanBootstrap::start(&extensions)?;
    // SAFETY: The bootstrap enabled this live window's exact extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: SDL created the surface from this exact instance and transfers
    // ownership immediately to the renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let handle = renderer.upload_terrain_mesh(&tile_mesh)?;
    assert_eq!(renderer.upload_terrain_mesh(&tile_mesh)?, handle);
    let info = renderer
        .terrain_mesh_info(handle)
        .ok_or("uploaded terrain resource is absent")?;
    assert_eq!(info.tile(), tile_index);
    assert_eq!(info.vertex_count(), 256 * 145);
    assert_eq!(info.index_count(), 256 * 768);
    assert_eq!(info.chunk_count(), 256);
    assert_eq!(
        info.vertex_byte_count(),
        256 * 145 * solarity_rendering::TerrainRenderVertex::BYTE_SIZE
    );
    assert_eq!(info.index_byte_count(), 256 * 768 * 2);
    let material = renderer.upload_terrain_material(&tile_mesh)?;
    assert_eq!(renderer.upload_terrain_material(&tile_mesh)?, material);
    let material_info = renderer
        .terrain_material_info(material)
        .ok_or("uploaded terrain material atlas is absent")?;
    assert_eq!(material_info.tile(), tile_index);
    assert_eq!(material_info.extent(), (1_024, 1_024));
    assert_eq!(
        material_info.byte_count(),
        TERRAIN_MATERIAL_ATLAS_BYTE_COUNT
    );
    let grass_texture = renderer.upload_blp_texture(&grass_source, BlpColorSpace::Srgb)?;
    let texture_set = TerrainTextureSet::new(material, &[grass_texture])?;
    let texture_sets =
        renderer.prepare_terrain_texture_sets(&[texture_set.clone(), texture_set.clone()])?;
    assert_eq!(texture_sets.len(), 2);
    assert_eq!(texture_sets[0], texture_sets[1]);
    assert_eq!(
        renderer
            .terrain_texture_set_info(texture_sets[0])
            .ok_or("terrain texture set is absent")?
            .layer_count(),
        TerrainLayerCount::One
    );
    let terrain_pipeline = renderer.prepare_terrain_pipeline(TerrainLayerCount::One)?;
    let draw = renderer.prepare_terrain_draw(
        handle,
        terrain_pipeline,
        texture_sets[0],
        &texture_set,
        &tile_mesh,
        0,
    )?;
    assert_eq!(draw.first_index(), 0);
    assert_eq!(draw.index_count(), 768);
    assert_eq!(draw.push_bytes(), [0; 8]);
    let frame = renderer.present_terrain(scene, &[draw])?;
    assert_eq!(frame.draw_count(), 1);
    for layer_count in [
        TerrainLayerCount::One,
        TerrainLayerCount::Two,
        TerrainLayerCount::Three,
        TerrainLayerCount::Four,
    ] {
        let pipeline = renderer.prepare_terrain_pipeline(layer_count)?;
        assert_eq!(renderer.prepare_terrain_pipeline(layer_count)?, pipeline);
        assert_eq!(
            renderer
                .terrain_pipeline_info(pipeline)
                .ok_or("uploaded terrain pipeline is absent")?
                .layer_count(),
            layer_count
        );
    }
    Ok(())
}

fn terrain_wdt() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut wdt = WdtFile::new(WowVersion::WotLK);
    wdt.mwmo = Some(MwmoChunk::new());
    let entry = wdt.main.get_mut(32, 32).ok_or("fixture tile is invalid")?;
    entry.set_has_adt(true);
    let mut bytes = Vec::new();
    WdtWriter::new(&mut bytes).write(&wdt)?;
    Ok(bytes)
}

fn map_table() -> Vec<u8> {
    let mut strings = vec![0_u8];
    let directory = append_string(&mut strings, "Northrend");
    let name = append_string(&mut strings, "Northrend");
    let mut fields = [0_u32; 66];
    fields[0] = 571;
    fields[1] = directory;
    fields[5] = name;
    fields[22] = 571;
    fields[59] = u32::MAX;
    fields[63] = 2;
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + strings.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&66_u32.to_le_bytes());
    bytes.extend_from_slice(&(66_u32 * 4).to_le_bytes());
    bytes.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(&strings);
    bytes
}

fn append_string(block: &mut Vec<u8>, value: &str) -> u32 {
    let offset = block.len() as u32;
    block.extend_from_slice(value.as_bytes());
    block.push(0);
    offset
}

fn solid_raw3_blp(width: u32, height: u32, color: u32) -> Vec<u8> {
    const PIXEL_OFFSET: u32 = 148 + 256 * 4;

    let byte_count = width.saturating_mul(height).saturating_mul(4);
    let mut offsets = [0_u32; 16];
    let mut sizes = [0_u32; 16];
    offsets[0] = PIXEL_OFFSET;
    sizes[0] = byte_count;
    let mut bytes = Vec::with_capacity((PIXEL_OFFSET + byte_count) as usize);
    bytes.extend_from_slice(b"BLP2");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&[3, 8, 8, 0]);
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    for offset in offsets {
        bytes.extend_from_slice(&offset.to_le_bytes());
    }
    for size in sizes {
        bytes.extend_from_slice(&size.to_le_bytes());
    }
    bytes.resize(PIXEL_OFFSET as usize, 0);
    for _pixel in 0..width.saturating_mul(height) {
        bytes.extend_from_slice(&color.to_le_bytes());
    }
    bytes
}
