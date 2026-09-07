//! Actual liquid descriptor, blend, frame reuse, and geometry retirement coverage.

use std::error::Error;

use glam::{Mat4, Vec3, Vec4};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureSource, ClientDataRoot, Locale,
};
use solarity_rendering::{
    BlpColorSpace, LiquidDepthTexture, LiquidDepthTextureKind, LiquidDrawMaterial, LiquidFog,
    LiquidFrame, LiquidLighting, LiquidRenderVertex, LiquidShaderUniform, M2LocalLightState,
    M2SceneUniform, TerrainSceneUniform, VulkanBootstrap, WaterRippleFrame, WaterRipplePass,
    WaterRippleRenderVertex, WorldFrameScene, WorldModelSceneUniform, WorldModelTextureFiltering,
};

use crate::support::{Fixture, FixtureFile};

/// Surface alpha zero isolates native depth alpha while opaque magma supplies
/// the destination. Repeated changing frames exercise each slot and growth.
#[test]
#[allow(unsafe_code)] // SDL transfers the hidden test surface to Vulkan ownership.
fn liquid_frames_update_depth_images_blend_and_retire_meshes() -> Result<(), Box<dyn Error>> {
    let black = crate::model::solid_raw3_blp(1, 1, &[0]);
    let fixture = Fixture::new(&[FixtureFile {
        path: "Black.blp",
        bytes: &black,
    }])?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let black = BlpTextureSource::load(&mut assets, &AssetPath::new("Black.blp")?)?;
    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity liquid frame test", 32, 32)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The bootstrap enabled all extensions required by this live window.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: SDL transfers sole ownership; the window outlives the renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (32, 32), 0) }?;
    let black = renderer.upload_blp_texture(&black, BlpColorSpace::Linear)?;
    let white = renderer.upload_stock_m2_white()?;
    let background = renderer.upload_liquid_mesh(&triangle(0.8, [255, 0, 0, 255]), &[0, 1, 2])?;
    let water = renderer.upload_liquid_mesh(&triangle(0.5, [255; 4]), &[0, 1, 2])?;
    let uniform = LiquidShaderUniform::new(
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        LiquidLighting::new(-Vec3::Z, Vec3::ONE, Vec3::ZERO, Vec3::ZERO),
        LiquidFog::new(Vec3::new(0.0, 1.0, 1.0), Vec3::ZERO),
    );
    let base =
        renderer.prepare_liquid_draw(background, LiquidDrawMaterial::Magma, white, uniform)?;
    for (frame_index, count) in [1, 3, 1, 7, 2, 1, 4, 1, 2].into_iter().enumerate() {
        let alpha = [64, 128, 192][frame_index % 3];
        let river = LiquidDepthTexture::prepare(
            LiquidDepthTextureKind::River,
            [0x0000_00ff; 2],
            [alpha; 2],
        );
        let ocean = LiquidDepthTexture::prepare(
            LiquidDepthTextureKind::Ocean,
            [0x0000_ff00; 2],
            [alpha; 2],
        );
        let wmo = LiquidDepthTexture::prepare(
            LiquidDepthTextureKind::WorldModel,
            [0x0000_ffff; 2],
            [alpha; 2],
        );
        let kind = [
            LiquidDepthTextureKind::River,
            LiquidDepthTextureKind::Ocean,
            LiquidDepthTextureKind::WorldModel,
        ][frame_index % 3];
        let material = if frame_index % 2 == 0 {
            LiquidDrawMaterial::Water(kind)
        } else {
            LiquidDrawMaterial::WaterNoSpecular(kind)
        };
        let draw = renderer.prepare_liquid_draw(water, material, black, uniform)?;
        let mut draws = vec![base];
        draws.extend(std::iter::repeat_n(draw, count));
        // Change immutable sampler state while slots also reuse existing capacity.
        let filtering = [
            WorldModelTextureFiltering::Bilinear,
            WorldModelTextureFiltering::Anisotropic4x,
            WorldModelTextureFiltering::Trilinear,
            WorldModelTextureFiltering::Anisotropic16x,
        ][frame_index % 4];
        let scene = scene().with_liquids(
            LiquidFrame::new(&draws, &river, &ocean, &wmo, 0).with_texture_filtering(filtering),
        );
        renderer.request_frame_capture()?;
        let report =
            renderer.present_world_frame(scene, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
        assert_eq!(report.liquid_draw_count(), count + 1);
        let capture = renderer
            .take_captured_frame()?
            .ok_or("missing liquid capture")?;
        let source: [f32; 3] = match kind {
            LiquidDepthTextureKind::River => [0.0, 0.0, 255.0],
            LiquidDepthTextureKind::Ocean => [0.0, 255.0, 0.0],
            LiquidDepthTextureKind::WorldModel => [0.0, 255.0, 255.0],
        };
        let mut expected = [255.0, 0.0, 0.0];
        let alpha = f32::from(alpha) / 255.0;
        for _ in 0..count {
            expected = std::array::from_fn(|channel| {
                (source[channel] * alpha + expected[channel] * (1.0 - alpha)).round()
            });
        }
        for pixel in capture.rgba8().as_chunks::<4>().0 {
            for channel in 0..3 {
                assert!(
                    (f32::from(pixel[channel]) - expected[channel]).abs() <= 1.0,
                    "frame {frame_index}: {pixel:?} expected {expected:?}"
                );
            }
            assert_eq!(pixel[3], 255);
        }
    }
    renderer.retire_liquid_meshes(&[water, background])?;
    assert!(
        renderer
            .prepare_liquid_draw(
                water,
                LiquidDrawMaterial::Water(LiquidDepthTextureKind::River),
                black,
                uniform
            )
            .is_err()
    );
    let replacement = renderer.upload_liquid_mesh(&triangle(0.5, [255; 4]), &[0, 1, 2])?;
    assert_ne!(water, replacement);
    renderer.retire_liquid_meshes(&[replacement])?;
    Ok(())
}

/// Different authored mip colors expose accidental global/mip filtering.
/// Coplanar depth tests, pass order, absent writes, and wrapping native counts
/// are observed in captured pixels while all frame slots grow and reuse data.
#[test]
#[allow(unsafe_code)] // SDL transfers the hidden test surface to Vulkan ownership.
fn water_ripple_frames_preserve_native_passes_depth_bias_and_no_mip_sampling()
-> Result<(), Box<dyn Error>> {
    let green =
        crate::model::solid_raw3_blp(8, 8, &[0xff00_ff00, 0xffff_0000, 0xffff_0000, 0xffff_0000]);
    let blue =
        crate::model::solid_raw3_blp(8, 8, &[0xff00_00ff, 0xffff_0000, 0xffff_0000, 0xffff_0000]);
    let black = crate::model::solid_raw3_blp(1, 1, &[0]);
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Green.blp",
            bytes: &green,
        },
        FixtureFile {
            path: "Blue.blp",
            bytes: &blue,
        },
        FixtureFile {
            path: "Black.blp",
            bytes: &black,
        },
    ])?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let green = BlpTextureSource::load(&mut assets, &AssetPath::new("Green.blp")?)?;
    let blue = BlpTextureSource::load(&mut assets, &AssetPath::new("Blue.blp")?)?;
    let black = BlpTextureSource::load(&mut assets, &AssetPath::new("Black.blp")?)?;
    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity ripple frame test", 32, 32)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The bootstrap enabled this live window's surface extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: SDL transfers ownership and the window outlives the renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (32, 32), 0) }?;
    let green = renderer.upload_blp_texture(&green, BlpColorSpace::Linear)?;
    let blue = renderer.upload_blp_texture(&blue, BlpColorSpace::Linear)?;
    let black = renderer.upload_blp_texture(&black, BlpColorSpace::Linear)?;
    let white = renderer.upload_stock_m2_white()?;
    let background = renderer.upload_liquid_mesh(&triangle(0.8, [255, 0, 0, 255]), &[0, 1, 2])?;
    let water = renderer.upload_liquid_mesh(&triangle(0.79, [255; 4]), &[0, 1, 2])?;
    let uniform = LiquidShaderUniform::new(
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        LiquidLighting::new(-Vec3::Z, Vec3::ONE, Vec3::ZERO, Vec3::ZERO),
        LiquidFog::new(Vec3::new(0.0, 1.0, 1.0), Vec3::ZERO),
    );
    let draws = [
        renderer.prepare_liquid_draw(background, LiquidDrawMaterial::Magma, white, uniform)?,
        renderer.prepare_liquid_draw(
            water,
            LiquidDrawMaterial::WaterNoSpecular(LiquidDepthTextureKind::River),
            black,
            uniform,
        )?,
    ];
    let depth =
        LiquidDepthTexture::prepare(LiquidDepthTextureKind::River, [0x0000_00ff; 2], [64; 2]);
    let mut circular = Vec::new();
    let mut directional = Vec::new();
    let base_triangle = |z| {
        [
            Vec3::new(-1., -1., z),
            Vec3::new(3., -1., z),
            Vec3::new(-1., 3., z),
        ]
    };
    for (index, count) in [1, 3, 1, 7, 2, 1, 21_846, 65_536, 1]
        .into_iter()
        .enumerate()
    {
        circular.clear();
        directional.clear();
        WaterRippleRenderVertex::project_into(
            &vec![base_triangle(0.800_06); count],
            Mat4::from_scale(Vec3::splat(16.)),
            0.5,
            &mut circular,
        )?;
        WaterRippleRenderVertex::project_into(
            &[base_triangle(0.800_08)],
            Mat4::from_scale(Vec3::splat(16.)),
            0.5,
            &mut directional,
        )?;
        let swapped = index % 2 != 0;
        let circular_texture = if swapped { blue } else { green };
        let directional_texture = if swapped { green } else { blue };
        let circular_pass = WaterRipplePass::new(circular_texture, &circular);
        let directional_pass = WaterRipplePass::new(directional_texture, &directional);
        let bias = if index == 5 { 0. } else { 0.125 };
        let frame = WaterRippleFrame::new(
            Mat4::IDENTITY,
            bias,
            0,
            Some(circular_pass),
            Some(directional_pass),
        )?;
        let scene = scene()
            .with_liquids(
                LiquidFrame::new(&draws, &depth, &depth, &depth, 0)
                    .with_texture_filtering(WorldModelTextureFiltering::Anisotropic16x),
            )
            .with_ripples(frame);
        renderer.request_frame_capture()?;
        let report =
            renderer.present_world_frame(scene, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
        assert_eq!(report.ripple_draw_count(), frame.draw_count());
        let capture = renderer
            .take_captured_frame()?
            .ok_or("missing ripple capture")?;
        let mut expected: [f32; 4] = [191.0, 0.0, 64.0, 255.0];
        if bias != 0. {
            for (color, count) in [
                (
                    if swapped {
                        [0., 0., 255.]
                    } else {
                        [0., 255., 0.]
                    },
                    circular_pass.draw_vertex_count() / 3,
                ),
                (
                    if swapped {
                        [0., 255., 0.]
                    } else {
                        [0., 0., 255.]
                    },
                    1,
                ),
            ] {
                for _ in 0..count {
                    let alpha = 128. / 255.;
                    for channel in 0..3 {
                        expected[channel] = (color[channel] * alpha + expected[channel])
                            .round()
                            .min(255.);
                    }
                    expected[3] = (128. * alpha + expected[3]).round().min(255.);
                }
            }
        }
        for pixel in capture.rgba8().as_chunks::<4>().0 {
            for channel in 0..4 {
                assert!(
                    (f32::from(pixel[channel]) - expected[channel]).abs() <= 2.,
                    "ripple frame {index}: {pixel:?}, expected {expected:?}"
                );
            }
        }
    }
    renderer.retire_liquid_meshes(&[water, background])?;
    Ok(())
}

/// A full-frame triangle has constant depth UVs away from the river tail/WMO split.
fn triangle(depth: f32, color: [u8; 4]) -> [LiquidRenderVertex; 3] {
    [[-1.0, -1.0, depth], [3.0, -1.0, depth], [-1.0, 3.0, depth]].map(|position| {
        LiquidRenderVertex::new(position, [0.0, 0.0, 1.0], color, [0.0, 0.25], [0.0; 2])
    })
}

/// Unused world shader families retain valid uniforms in a liquid-only frame.
fn scene() -> WorldFrameScene<'static> {
    WorldFrameScene::new(
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
}
