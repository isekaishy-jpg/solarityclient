//! GPU verification of independent map and mask sampling in the ordinary UI pass.

#![allow(unsafe_code)]

use super::*;
use crate::model::{raw3_blp_pixels, rgba8_pixel};
use crate::support::{Fixture, FixtureFile};

#[test]
fn ui_mask_identity_and_rectangle_split_material_batches() -> Result<(), Box<dyn Error>> {
    let source = UiRenderSource::Texture(AssetPath::new("Tile.blp")?);
    let path = AssetPath::new("Mask.blp")?;
    let base = quad(1, source.clone(), [0.0, 0.0, 32.0, 32.0]);
    let first = base
        .clone()
        .with_mask(UiRenderMask::new(path.clone(), [0.0, 0.0, 64.0, 64.0]));
    let shifted = base
        .clone()
        .with_mask(UiRenderMask::new(path, [8.0, 0.0, 72.0, 64.0]));
    let other = base.clone().with_mask(UiRenderMask::new(
        AssetPath::new("Other.blp")?,
        [0.0, 0.0, 64.0, 64.0],
    ));
    let mut plan = UiMeshPlan::prepare(
        [96.0; 2],
        [first.clone(), first.clone(), shifted, other, base.clone()].into_iter(),
    )?;
    assert_eq!(plan.batches().len(), 4);
    assert_eq!(plan.batches()[0].quad_count(), 2);
    let before = plan.clone();
    assert!(!plan.replace_object_source_quads(
        1,
        &source,
        &[
            first.clone(),
            first.clone(),
            first.clone(),
            first.clone(),
            first
        ]
    )?);
    assert_eq!(plan, before);
    for bounds in [
        [0.0, 0.0, f32::INFINITY, 1.0],
        [0.0, 0.0, f32::from_bits(1), 1.0],
    ] {
        assert!(
            UiMeshPlan::prepare(
                [96.0; 2],
                [base
                    .clone()
                    .with_mask(UiRenderMask::new(AssetPath::new("Mask.blp")?, bounds))]
                .into_iter()
            )
            .is_err()
        );
    }
    Ok(())
}
use solarity_asset::{ArchiveCatalog, AssetStore, BlpTextureSource, ClientDataRoot, Locale};
use solarity_rendering::{
    BlpColorSpace, UiRenderMask, UiSampledTexture, UiSamplerInfo, UiShaderSource, VulkanBootstrap,
};

#[test]
fn ui_mask_uses_independent_coordinates_and_preserves_tile_rgb() -> Result<(), Box<dyn Error>> {
    let red = raw3_blp_pixels(4, 4, &[0xffff0000; 16]);
    let mask_pixels = (0..64)
        .flat_map(|y| {
            (0..64).map(move |x| {
                let alpha = if y >= 32 {
                    255
                } else if x < 32 {
                    0
                } else {
                    128
                };
                (alpha << 24) | 0x0000ff00
            })
        })
        .collect::<Vec<_>>();
    let mask = raw3_blp_pixels(64, 64, &mask_pixels);
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Tile.blp",
            bytes: &red,
        },
        FixtureFile {
            path: "Mask.blp",
            bytes: &mask,
        },
    ])?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let tile_path = AssetPath::new("Tile.blp")?;
    let mask_path = AssetPath::new("Mask.blp")?;
    let tile = BlpTextureSource::load(&mut assets, &tile_path)?;
    let mask = BlpTextureSource::load(&mut assets, &mask_path)?;
    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity UI mask test", 96, 96)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The instance enabled this live window's required extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: SDL transfers surface ownership; the window outlives the renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (96, 96), 0) }?;
    let tile = renderer.upload_blp_texture(&tile, BlpColorSpace::Linear)?;
    let mask = renderer.upload_blp_texture(&mask, BlpColorSpace::Linear)?;
    let sampler = renderer.prepare_ui_sampler(UiSamplerInfo::new(
        UiTextureAddressMode::Clamp,
        UiTextureAddressMode::Clamp,
    ))?;
    let sets = renderer.prepare_ui_texture_sets(&[
        UiSampledTexture::new(tile, sampler),
        UiSampledTexture::new(mask, sampler),
    ])?;
    let textured = renderer.prepare_ui_pipeline(UiShaderSource::Texture, UiRenderBlend::Alpha)?;
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
        [[0.0, 0.0, 1.0, 1.0]; 4],
    );
    // The two tiles each sample their complete image, but share one mask rectangle.
    let mask_bounds = [16.0, 16.0, 80.0, 80.0];
    let left = quad(
        1,
        UiRenderSource::Texture(tile_path.clone()),
        [16.0, 16.0, 48.0, 80.0],
    )
    .with_mask(UiRenderMask::new(mask_path.clone(), mask_bounds));
    let right = quad(
        1,
        UiRenderSource::Texture(tile_path.clone()),
        [48.0, 16.0, 80.0, 80.0],
    )
    .with_mask(UiRenderMask::new(mask_path.clone(), mask_bounds));
    let ordinary = quad(2, UiRenderSource::Texture(tile_path), [0.0, 0.0, 8.0, 8.0]);
    let mut plan = UiMeshPlan::prepare([96.0; 2], [background, left, right, ordinary].into_iter())?;
    assert_eq!(plan.batches().len(), 3);
    assert_eq!(plan.batches()[1].quad_count(), 2);
    let mesh = renderer.upload_ui_mesh(&plan)?;
    assert!(
        renderer
            .prepare_ui_draw(mesh, masked, Some(sets[0]), &plan, 1)
            .is_err()
    );
    assert!(
        renderer
            .prepare_ui_draw_with_mask(mesh, masked, Some(sets[0]), Some(sets[0]), &plan, 1)
            .is_err()
    );
    assert!(
        renderer
            .prepare_ui_draw_with_mask(mesh, textured, Some(sets[0]), Some(sets[1]), &plan, 1)
            .is_err()
    );
    let mut draws = [
        renderer.prepare_ui_draw(mesh, color, None, &plan, 0)?,
        renderer.prepare_ui_draw_with_mask(mesh, masked, Some(sets[0]), Some(sets[1]), &plan, 1)?,
        renderer.prepare_ui_draw(mesh, textured, Some(sets[0]), &plan, 2)?,
    ];
    for translation in [[0.0, 0.0], [8.0, 8.0]] {
        // Moving the retained draw must carry the mask without a vertex upload.
        draws[1].set_transform_state(translation, 1.0, None);
        renderer.request_frame_capture()?;
        renderer.present_ui([96.0; 2], &draws)?;
        let capture = renderer
            .take_captured_frame()?
            .ok_or("missing masked UI capture")?;
        let pixel = |x, y| {
            rgba8_pixel(
                capture.rgba8(),
                capture.extent().0,
                x + translation[0] as u32,
                y - translation[1] as u32,
            )
        };
        assert_eq!(&pixel(32, 32)[..3], &[0, 0, 255]);
        assert_eq!(&pixel(32, 64)[..3], &[255, 0, 0]);
        let half = pixel(64, 32);
        assert!((126..=129).contains(&half[0]), "half red: {half:?}");
        assert_eq!(half[1], 0, "mask RGB must not tint the tile");
        assert!((126..=129).contains(&half[2]), "half blue: {half:?}");
        assert_eq!(
            &rgba8_pixel(capture.rgba8(), capture.extent().0, 4, 92)[..3],
            &[255, 0, 0]
        );
    }
    let before = plan.clone();
    let invalid = quad(
        1,
        UiRenderSource::Texture(AssetPath::new("Tile.blp")?),
        [16.0, 16.0, 48.0, 80.0],
    )
    .with_mask(UiRenderMask::new(mask_path, [1.0, 1.0, 1.0, 2.0]));
    assert!(
        plan.replace_object_source_quads(
            1,
            &UiRenderSource::Texture(AssetPath::new("Tile.blp")?),
            &[invalid.clone(), invalid]
        )
        .is_err()
    );
    assert_eq!(plan, before);
    Ok(())
}
