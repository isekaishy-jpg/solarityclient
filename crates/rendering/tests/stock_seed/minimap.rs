//! Native minimap axes, zoom projection, terrain seams, and masked GPU composition.

#![allow(unsafe_code)]

use std::error::Error;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use solarity_asset::{AssetPath, TerrainTileIndex};
use solarity_rendering::{MinimapView, UiMeshPlan, UiRenderSource};

fn near(actual: [f32; 2], expected: [f32; 2]) {
    assert!(
        (actual[0] - expected[0]).abs() < 0.001,
        "{actual:?} != {expected:?}"
    );
    assert!(
        (actual[1] - expected[1]).abs() < 0.001,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn minimap_projects_native_axes_and_shares_exact_terrain_edges() -> Result<(), Box<dyn Error>> {
    let bounds = [0.0, 0.0, 100.0, 100.0];
    let north_up = MinimapView::new([0.0; 2], 100.0, 0.0, bounds)?;
    near(north_up.project([0.0; 2]), [50.0; 2]);
    near(north_up.project([100.0, 0.0]), [50.0, 100.0]);
    near(north_up.project([0.0, 100.0]), [0.0, 50.0]);
    near(north_up.project([-100.0, 0.0]), [50.0, 0.0]);
    near(north_up.project([0.0, -100.0]), [100.0, 50.0]);
    let west_up = MinimapView::new([0.0; 2], 100.0, FRAC_PI_2, bounds)?;
    near(west_up.project([0.0, 100.0]), [50.0, 100.0]);
    near(west_up.project([100.0, 0.0]), [100.0, 50.0]);
    let shifted = MinimapView::new([1000.0, -4000.0], 50.0, 0.0, [20.0, 30.0, 220.0, 130.0])?;
    near(shifted.project([1000.0, -4000.0]), [120.0, 80.0]);
    near(shifted.project([1050.0, -4000.0]), [120.0, 130.0]);
    near(shifted.project([1000.0, -3950.0]), [20.0, 80.0]);
    let indices = north_up.terrain_tiles();
    assert_eq!(
        indices.map(|tile| (tile.x(), tile.y())),
        [(31, 31), (32, 31), (32, 32), (31, 32)]
    );
    let tile = AssetPath::new("Tile.blp")?;
    let mask = AssetPath::new("Mask.blp")?;
    for center in [[0.0; 2], [1562.0, -4405.0]] {
        let view = MinimapView::new(center, 233.333_33, FRAC_PI_4, bounds)?;
        let quads = view
            .terrain_tiles()
            .map(|index| view.terrain_quad(1, index, tile.clone(), mask.clone()));
        let positions = quads.each_ref().map(|quad| quad.positions());
        assert_eq!(positions[0][2], positions[1][0]);
        assert_eq!(positions[0][3], positions[1][1]);
        assert_eq!(positions[0][1], positions[3][0]);
        assert_eq!(positions[0][3], positions[3][2]);
        assert_eq!(positions[1][1], positions[2][0]);
        assert_eq!(positions[1][3], positions[2][2]);
        assert_eq!(positions[3][3], positions[2][1]);
        let plan = UiMeshPlan::prepare([100.0; 2], quads.into_iter())?;
        assert_eq!(plan.batches().len(), 1);
        assert_eq!(plan.batches()[0].clip(), Some(bounds));
    }
    for (center, first) in [([17066.666; 2], (0, 0)), ([-17066.666; 2], (62, 62))] {
        let view = MinimapView::new(center, 100.0, 0.0, bounds)?;
        let tiles = view.terrain_tiles();
        assert_eq!((tiles[0].x(), tiles[0].y()), first);
        assert_eq!((tiles[2].x(), tiles[2].y()), (first.0 + 1, first.1 + 1));
    }
    assert!(MinimapView::new([f32::NAN, 0.0], 100.0, 0.0, bounds).is_err());
    assert!(MinimapView::new([0.0; 2], 0.0, 0.0, bounds).is_err());
    assert!(MinimapView::new([0.0; 2], 100.0, f32::INFINITY, bounds).is_err());
    assert!(MinimapView::new([0.0; 2], 100.0, 0.0, [0.0; 4]).is_err());
    Ok(())
}

#[test]
fn minimap_marker_uses_native_uv_rotation_and_retained_corner_updates() -> Result<(), Box<dyn Error>>
{
    let view = MinimapView::new([0.0; 2], 100.0, 0.0, [0.0, 0.0, 100.0, 100.0])?;
    let path = AssetPath::new("Arrow.blp")?;
    let north = view.player_quad(1, path.clone(), [40.0, 30.0], 0.0);
    let diagonal = view.player_quad(1, path.clone(), [40.0, 30.0], FRAC_PI_4);
    assert_eq!(north.positions(), diagonal.positions());
    assert_eq!(diagonal.bounds(), [30.0, 35.0, 70.0, 65.0]);
    assert_eq!(
        diagonal.texture_coordinates(),
        [[0.5, -0.5], [-0.5, 0.5], [1.5, 0.5], [0.5, 1.5]]
    );
    let following = MinimapView::new([0.0; 2], 100.0, FRAC_PI_4, [0.0, 0.0, 100.0, 100.0])?;
    assert_eq!(
        following.player_quad(1, path, [40.0, 30.0], FRAC_PI_4),
        north
    );
    let mask = AssetPath::new("Mask.blp")?;
    let tile = TerrainTileIndex::new(31, 31).ok_or("invalid test tile")?;
    let path = AssetPath::new("Tile.blp")?;
    let before = view.terrain_quad(2, tile, path.clone(), mask.clone());
    let after = following.terrain_quad(2, tile, path.clone(), mask.clone());
    assert_ne!(before.positions(), after.positions());
    let mut plan = UiMeshPlan::prepare([100.0; 2], [before].into_iter())?;
    let indices = plan.index_bytes().to_vec();
    assert!(plan.replace_object_source_quads(
        2,
        &UiRenderSource::Texture(path.clone()),
        std::slice::from_ref(&after)
    )?);
    assert_eq!(
        plan.vertices()
            .iter()
            .map(|vertex| vertex.position())
            .collect::<Vec<_>>(),
        after.positions()
    );
    assert_eq!(plan.index_bytes(), indices);
    assert!(plan.replace_object_source_run(
        2,
        &UiRenderSource::Texture(path.clone()),
        &[after.clone(), after.clone()]
    )?);
    assert_eq!(plan.vertices()[4].position(), after.positions()[0]);
    let previous = plan.clone();
    let mut invalid = after.positions();
    invalid[0][0] = f32::NAN;
    assert!(
        plan.replace_object_source_quads(
            2,
            &UiRenderSource::Texture(path),
            &[after.clone().with_positions(invalid), after]
        )
        .is_err()
    );
    assert_eq!(plan, previous);
    Ok(())
}

#[test]
fn minimap_rotated_tiles_keep_the_mask_fixed_and_do_not_leave_seams() -> Result<(), Box<dyn Error>>
{
    use crate::model::{raw3_blp_pixels, rgba8_pixel};
    use crate::support::{Fixture, FixtureFile};
    use solarity_asset::{ArchiveCatalog, AssetStore, BlpTextureSource, ClientDataRoot, Locale};
    use solarity_rendering::{
        BlpColorSpace, UiRenderBlend, UiRenderQuad, UiSampledTexture, UiSamplerInfo,
        UiShaderSource, UiTextureAddressMode, UiTextureResidency, VulkanBootstrap,
    };
    let tile_colors = [0xffff0000, 0xff00ff00, 0xff0000ff, 0xffffff00];
    let tiles = tile_colors.map(|color| raw3_blp_pixels(4, 4, &[color; 16]));
    let paths = ["Red.blp", "Green.blp", "Blue.blp", "Yellow.blp"];
    let mask_pixels = (0..64)
        .flat_map(|y| {
            (0..64).map(move |x| {
                let radius_squared = (x as f32 - 31.5).powi(2) + (y as f32 - 31.5).powi(2);
                if radius_squared < 31.0 * 31.0 {
                    0xff00ff00
                } else {
                    0x0000ff00
                }
            })
        })
        .collect::<Vec<_>>();
    let mask_bytes = raw3_blp_pixels(64, 64, &mask_pixels);
    let mut files = paths
        .iter()
        .zip(&tiles)
        .map(|(path, bytes)| FixtureFile { path, bytes })
        .collect::<Vec<_>>();
    files.push(FixtureFile {
        path: "Mask.blp",
        bytes: &mask_bytes,
    });
    let fixture = Fixture::new(&files)?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity minimap projection test", 96, 96)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: This live window requested the instance's enabled surface extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: SDL transfers surface ownership, and the window outlives the renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (96, 96), 0) }?;
    let sampler = renderer.prepare_ui_sampler(UiSamplerInfo::new(
        UiTextureAddressMode::Clamp,
        UiTextureAddressMode::Clamp,
    ))?;
    let mut images = Vec::new();
    for path in paths.into_iter().chain(["Mask.blp"]) {
        let source = BlpTextureSource::load(&mut assets, &AssetPath::new(path)?)?;
        let handle = renderer.upload_blp_texture(&source, BlpColorSpace::Linear)?;
        images.push(UiSampledTexture::new(handle, sampler));
    }
    let sets = renderer.prepare_ui_texture_sets(&images)?;
    let masked =
        renderer.prepare_ui_pipeline(UiShaderSource::MaskedTexture, UiRenderBlend::Alpha)?;
    let color = renderer.prepare_ui_pipeline(UiShaderSource::VertexColor, UiRenderBlend::Alpha)?;
    let background = UiRenderQuad::new(
        0,
        UiRenderSource::VertexColor,
        UiRenderBlend::Alpha,
        UiTextureAddressMode::Clamp,
        UiTextureAddressMode::Clamp,
        UiTextureResidency::Blocking,
        false,
        [0.0, 0.0, 96.0, 96.0],
        [[0.0; 2]; 4],
        [[1.0, 0.0, 1.0, 1.0]; 4],
    );
    let mut mesh = None;
    for heading in [0.0, FRAC_PI_4, FRAC_PI_2] {
        let view = MinimapView::new([0.0; 2], 100.0, heading, [16.0, 16.0, 80.0, 80.0])?;
        let mut quads = vec![background.clone()];
        for (tile, path) in view.terrain_tiles().into_iter().zip(paths) {
            quads.push(view.terrain_quad(
                1,
                tile,
                AssetPath::new(path)?,
                AssetPath::new("Mask.blp")?,
            ));
        }
        let plan = UiMeshPlan::prepare([96.0; 2], quads.into_iter())?;
        let handle = if let Some(handle) = mesh {
            renderer.replace_ui_mesh(handle, &plan)?;
            handle
        } else {
            renderer.upload_ui_mesh(&plan)?
        };
        mesh = Some(handle);
        let mut draws = vec![renderer.prepare_ui_draw(handle, color, None, &plan, 0)?];
        for (index, set) in sets[..4].iter().enumerate() {
            draws.push(renderer.prepare_ui_draw_with_mask(
                handle,
                masked,
                Some(*set),
                Some(sets[4]),
                &plan,
                index + 1,
            )?);
        }
        renderer.request_frame_capture()?;
        renderer.present_ui([96.0; 2], &draws)?;
        let capture = renderer
            .take_captured_frame()?
            .ok_or("missing minimap capture")?;
        let pixel = |x, y| rgba8_pixel(capture.rgba8(), capture.extent().0, x, y);
        for (x, y) in [(8, 48), (88, 48), (48, 8), (48, 88), (18, 18), (78, 78)] {
            assert_eq!(
                &pixel(x, y)[..3],
                &[255, 0, 255],
                "mask/clip at {x},{y} heading {heading}"
            );
        }
        for y in 24..72 {
            for x in 24..72 {
                if (x as f32 - 48.0).powi(2) + (y as f32 - 48.0).powi(2) < 24.0 * 24.0 {
                    let rgb = pixel(x, y);
                    assert!(
                        [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]]
                            .contains(&[rgb[0], rgb[1], rgb[2]]),
                        "seam at {x},{y} heading {heading}: {rgb:?}"
                    );
                }
            }
        }
        if heading == 0.0 || heading == FRAC_PI_2 {
            let expected = if heading == 0.0 {
                [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]]
            } else {
                [[255, 255, 0], [255, 0, 0], [0, 255, 0], [0, 0, 255]]
            };
            for ((x, y), rgb) in [(32, 32), (64, 32), (64, 64), (32, 64)]
                .into_iter()
                .zip(expected)
            {
                assert_eq!(
                    &pixel(x, y)[..3],
                    &rgb,
                    "orientation at {x},{y} heading {heading}"
                );
            }
        }
    }
    Ok(())
}
