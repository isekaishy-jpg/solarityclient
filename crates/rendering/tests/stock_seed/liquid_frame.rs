//! Actual liquid descriptor, blend, frame reuse, and geometry retirement coverage.

use std::error::Error;

use glam::{Mat4, Vec3, Vec4};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureSource, ClientDataRoot, Locale,
};
use solarity_rendering::{
    BlpColorSpace, LiquidDepthTexture, LiquidDepthTextureKind, LiquidDrawMaterial, LiquidFog,
    LiquidFrame, LiquidLighting, LiquidRenderVertex, LiquidShaderUniform, M2LocalLightState,
    M2SceneUniform, TerrainSceneUniform, UnderwaterParticleFog, UnderwaterParticleFrame,
    UnderwaterParticleVertex, VulkanBootstrap, WaterRippleFrame, WaterRipplePass,
    WaterRippleRenderVertex, WorldFrameScene, WorldModelSceneUniform, WorldModelTextureFiltering,
};

use crate::support::{Fixture, FixtureFile};

#[path = "celestial_frame.rs"]
mod celestial;

#[path = "terrain_low_detail_frame.rs"]
mod low_detail;

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
    let background_vertices = triangle(0.8, [255, 0, 0, 255]);
    let mut water_vertices = triangle(0.5, [255; 4]).to_vec();
    water_vertices.push(water_vertices[0]);
    assert!(renderer.upload_liquid_meshes(&[])?.is_empty());
    assert!(
        renderer
            .upload_liquid_meshes(&[
                (&background_vertices, &[0, 1, 2]),
                (&water_vertices, &[0, 1, 4]),
            ])
            .is_err()
    );
    // Unequal vertex extents and an odd first index count expose incorrect
    // shared staging offsets or missing per-payload four-byte padding.
    let handles = renderer.upload_liquid_meshes(&[
        (&background_vertices, &[0, 1, 2]),
        (&water_vertices, &[0, 1, 2, 2]),
    ])?;
    let [background, water] = <[_; 2]>::try_from(handles).map_err(|_| "batch handle count")?;
    assert_ne!(background, water);
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
    renderer.retire_liquid_meshes(&[water])?;
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
    // Retiring one member must not release its sibling's independent buffers.
    let base =
        renderer.prepare_liquid_draw(background, LiquidDrawMaterial::Magma, white, uniform)?;
    let depth = LiquidDepthTexture::prepare(LiquidDepthTextureKind::River, [0; 2], [0; 2]);
    let draws = [base];
    renderer.request_frame_capture()?;
    renderer.present_world_frame(
        scene().with_liquids(LiquidFrame::new(&draws, &depth, &depth, &depth, 0)),
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
    )?;
    let capture = renderer
        .take_captured_frame()?
        .ok_or("missing surviving batch member capture")?;
    assert!(
        capture
            .rgba8()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [255, 0, 0, 255])
    );
    renderer.retire_liquid_meshes(&[background])?;
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

/// Captured pixels distinguish alpha blending, depth testing without bias,
/// absent depth writes, nearest mip selection, and late-world queue placement.
#[test]
#[allow(unsafe_code)] // SDL transfers the hidden test surface to Vulkan ownership.
fn underwater_particle_frames_preserve_native_blend_depth_mips_and_slot_reuse()
-> Result<(), Box<dyn Error>> {
    let green_bytes = crate::model::solid_raw3_blp(1, 1, &[0x8000_ff00]);
    let blue_bytes = crate::model::solid_raw3_blp(1, 1, &[0x8000_00ff]);
    let mip_bytes = crate::model::solid_raw3_blp(
        1024,
        1024,
        &[
            0x8000_ff00,
            0x80ff_0000,
            0x80ff_0000,
            0x80ff_0000,
            0x80ff_0000,
            0x80ff_0000,
            0x80ff_0000,
            0x80ff_0000,
            0x80ff_0000,
            0x80ff_0000,
            0x80ff_0000,
        ],
    );
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Green.blp",
            bytes: &green_bytes,
        },
        FixtureFile {
            path: "Blue.blp",
            bytes: &blue_bytes,
        },
        FixtureFile {
            path: "Mips.blp",
            bytes: &mip_bytes,
        },
    ])?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity underwater frame test", 32, 32)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The bootstrap enabled this live window's surface extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: SDL transfers ownership and the window outlives the renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (32, 32), 0) }?;
    let mut textures = Vec::new();
    for name in ["Green.blp", "Blue.blp", "Mips.blp"] {
        let source = BlpTextureSource::load(&mut assets, &AssetPath::new(name)?)?;
        textures.push(renderer.upload_blp_texture(&source, BlpColorSpace::Linear)?);
    }
    let white = renderer.upload_stock_m2_white()?;
    let background = renderer.upload_liquid_mesh(&triangle(0.8, [255, 0, 0, 255]), &[0, 1, 2])?;
    let uniform = LiquidShaderUniform::new(
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        LiquidLighting::new(-Vec3::Z, Vec3::ONE, Vec3::ZERO, Vec3::ZERO),
        LiquidFog::new(Vec3::new(0., 1., 1.), Vec3::ZERO),
    );
    let draws =
        [renderer.prepare_liquid_draw(background, LiquidDrawMaterial::Magma, white, uniform)?];
    let depth = LiquidDepthTexture::prepare(LiquidDepthTextureKind::River, [0; 2], [0; 2]);
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (case, (count, z, texture_index, intensity)) in [
        (1, 0.5, 0, 1.),
        (2, 0.5, 1, 1.),
        (2, 0.800_06, 0, 1.),
        (2, 0.8, 1, 1.),
        (666, 0.5, 0, 1.),
        (0, 0.5, 1, 1.),
        (2, 0.5, 2, 1.),
        (2, 0.5, 0, 0.5),
        (1, 0.5, 1, 1.),
    ]
    .into_iter()
    .enumerate()
    {
        // The second particle is farther away. It remains visible because
        // the first particle must not write depth into the world attachment.
        let points: Vec<_> = (0..count)
            .map(|index| [0., 0., z + (index.min(1) as f32 * 0.01), 2.2])
            .collect();
        UnderwaterParticleVertex::project_into(
            &points,
            Mat4::IDENTITY,
            0,
            &mut vertices,
            &mut indices,
        )?;
        let particles = UnderwaterParticleFrame::new(
            Mat4::IDENTITY,
            if intensity == 1. {
                None
            } else {
                Some(UnderwaterParticleFog::new(
                    0.,
                    1.,
                    Vec3::new(0.1, 0.2, 0.3),
                )?)
            },
            textures[texture_index],
            &vertices,
            &indices,
        )?;
        let mut ripple_vertices = Vec::new();
        WaterRippleRenderVertex::project_into(
            &[[
                Vec3::new(-1., -1., 0.7),
                Vec3::new(3., -1., 0.7),
                Vec3::new(-1., 3., 0.7),
            ]],
            Mat4::IDENTITY,
            0.25,
            &mut ripple_vertices,
        )?;
        let ripple = WaterRippleFrame::new(
            Mat4::IDENTITY,
            0.,
            0,
            Some(WaterRipplePass::new(white, &ripple_vertices)),
            None,
        )?;
        let scene = scene()
            .with_liquids(LiquidFrame::new(&draws, &depth, &depth, &depth, 0))
            .with_ripples(ripple)
            .with_underwater_particles(particles);
        renderer.request_frame_capture()?;
        let report =
            renderer.present_world_frame(scene, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
        assert_eq!(report.underwater_draw_count(), particles.draw_count());
        let capture = renderer
            .take_captured_frame()?
            .ok_or("missing underwater capture")?;
        // White additive ripple precedes the alpha-blended underwater bank.
        let mut expected = [255_f32, 64., 64., 255.];
        let color = match texture_index {
            0 => [0., 255., 0.],
            1 => [0., 0., 255.],
            _ => [255., 0., 0.],
        };
        let alpha = 128. / 255.;
        for point in &points {
            if point[2] <= 0.8 {
                for channel in 0..3 {
                    let source = if intensity == 1. {
                        color[channel]
                    } else {
                        let fog_color = [26., 51., 77.][channel];
                        color[channel] * (1. - point[2]) + fog_color * point[2]
                    };
                    expected[channel] = (source * alpha + expected[channel] * (1. - alpha)).round();
                }
                expected[3] = (128. * alpha + expected[3] * (1. - alpha)).round();
            }
        }
        for pixel in capture.rgba8().as_chunks::<4>().0 {
            for channel in 0..4 {
                assert!(
                    (f32::from(pixel[channel]) - expected[channel]).abs() <= 2.,
                    "underwater frame {case}: {pixel:?}, expected {expected:?}"
                );
            }
        }
    }
    renderer.retire_liquid_meshes(&[background])?;
    Ok(())
}

/// Native background covers the viewport, follows the camera and yields to world geometry.
#[test]
#[allow(unsafe_code)] // SDL transfers the hidden surface to Vulkan ownership.
fn sky_frames_cover_background_reuse_slots_and_preserve_world_depth() -> Result<(), Box<dyn Error>>
{
    use solarity_rendering::{WorldCamera, WorldSkyDome, WorldSkyFrame};
    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity sky frame test", 256, 256)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: Bootstrap owns extensions for this live window's surface.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: The window outlives the renderer, which assumes surface ownership.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (256, 256), 0) }?;
    let white = renderer.upload_stock_m2_white()?;
    let mesh = renderer.upload_liquid_mesh(&triangle(0.8, [255, 0, 0, 255]), &[0, 1, 2])?;
    let uniform = LiquidShaderUniform::new(
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        LiquidLighting::new(-Vec3::Z, Vec3::ONE, Vec3::ZERO, Vec3::ZERO),
        LiquidFog::new(Vec3::new(0., 1., 1.), Vec3::ZERO),
    );
    let draws = [renderer.prepare_liquid_draw(mesh, LiquidDrawMaterial::Magma, white, uniform)?];
    let depth = LiquidDepthTexture::prepare(LiquidDepthTextureKind::River, [0; 2], [0; 2]);
    let mut dome = WorldSkyDome::new();
    for (case, (eye, direction)) in [
        (Vec3::ZERO, Vec3::X),
        (Vec3::new(12000., -3000., 100.), Vec3::X),
        (Vec3::ZERO, Vec3::Y),
        (Vec3::ZERO, Vec3::new(1., 0., 0.75)),
        (Vec3::ZERO, Vec3::new(1., 0., -0.75)),
        (Vec3::ZERO, -Vec3::X),
    ]
    .into_iter()
    .enumerate()
    {
        let camera = WorldCamera::stock(eye, eye + direction, Vec3::Z, 777.).frame(1.)?;
        let rgb = if case % 2 == 0 {
            [32, 96, 160]
        } else {
            [160, 64, 32]
        };
        let color = Vec3::from_array(rgb.map(|c| c as f32 / 255.));
        dome.update_colors([color; 5], color, 0., 0., camera);
        let mut frame = scene().with_sky(WorldSkyFrame::new(&dome, camera));
        if case == 4 {
            frame = frame.with_liquids(LiquidFrame::new(&draws, &depth, &depth, &depth, 0));
        }
        renderer.request_frame_capture()?;
        let report =
            renderer.present_world_frame(frame, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
        assert_eq!(report.sky_draw_count(), 1);
        let capture = renderer
            .take_captured_frame()?
            .ok_or("missing sky capture")?;
        let expected = if case == 4 { [255, 0, 0] } else { rgb };
        for pixel in capture.rgba8().as_chunks::<4>().0 {
            for (actual, expected) in pixel[..3].iter().zip(expected) {
                assert!(
                    (i32::from(*actual) - expected).abs() <= 1,
                    "sky case {case}: {pixel:?} expected {expected}"
                );
            }
        }
    }
    let camera = WorldCamera::stock(Vec3::ZERO, Vec3::X, Vec3::Z, 777.).frame(1.)?;
    let colors = [
        [16., 32., 96.],
        [32., 96., 192.],
        [64., 128., 224.],
        [160., 192., 240.],
        [192., 208., 240.],
    ]
    .map(|c| Vec3::from_array(c) / 255.);
    dome.update_colors(colors, Vec3::new(96., 128., 160.) / 255., 0.5, 0., camera);
    let frame = scene().with_sky(WorldSkyFrame::new(&dome, camera));
    renderer.request_frame_capture()?;
    renderer.present_world_frame(frame, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
    let capture = renderer
        .take_captured_frame()?
        .ok_or("missing sky gradient capture")?;
    let top = &capture.rgba8()[128 * 4..128 * 4 + 4];
    let bottom = &capture.rgba8()[(255 * 256 + 128) * 4..(255 * 256 + 128) * 4 + 4];
    assert!(top[2] > top[0] + 50, "top sky band is missing: {top:?}");
    assert_eq!(
        &bottom[..3],
        &[96, 128, 160],
        "lower hemisphere must match fog"
    );
    if let Some(path) = std::env::var_os("SOLARITY_SKY_CAPTURE_RGBA") {
        std::fs::write(path, capture.rgba8())?;
    }
    renderer.retire_liquid_meshes(&[mesh])?;
    Ok(())
}

/// Native texels, perspective-correct radial UVs, alpha fade and per-slot uploads.
#[test]
#[allow(unsafe_code)] // SDL transfers the hidden surface to Vulkan ownership.
fn cloud_frames_sample_native_pixels_blend_and_reuse_slots() -> Result<(), Box<dyn Error>> {
    use solarity_rendering::{
        WorldCamera, WorldCloudDome, WorldCloudFrame, WorldCloudLighting, WorldClouds,
        WorldSkyDome, WorldSkyFrame,
    };
    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity cloud frame test", 256, 256)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: Bootstrap enabled the live window's required surface extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: The window outlives the renderer's sole surface ownership.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (256, 256), 0) }?;
    let dome = WorldCloudDome::new();
    let mut clouds = WorldClouds::new(1);
    let mut sky = WorldSkyDome::new();
    let background = Vec3::new(24., 48., 96.);
    for case in 0..20 {
        let eye = if case % 2 == 0 {
            Vec3::ZERO
        } else {
            Vec3::new(12345., -6789., 100.)
        };
        let direction = if case < 10 {
            Vec3::new(1., 0., 0.7)
        } else {
            Vec3::new(0., 1., 0.7)
        };
        let camera = WorldCamera::stock(eye, eye + direction, Vec3::Z, 777.).frame(1.)?;
        if case % 5 == 0 {
            clouds.invalidate();
            clouds.update(
                0.5,
                if case < 10 { 0.7 } else { 0.9 },
                WorldCloudLighting::new(
                    [0.2, 0.3, 0.4],
                    [0.1, 0.2, 0.3],
                    [0.1, 0.05, 0.0],
                    [85.333336, 64., 64.],
                    1.,
                ),
            );
        }
        sky.update_colors([background / 255.; 5], background / 255., 0., 0., camera);
        let cloud_frame = WorldCloudFrame::new(&dome, &clouds, camera);
        let frame = scene()
            .with_sky(WorldSkyFrame::new(&sky, camera))
            .with_clouds(cloud_frame);
        renderer.request_frame_capture()?;
        renderer.present_world_frame(frame, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
        let capture = renderer
            .take_captured_frame()?
            .ok_or("missing cloud capture")?;
        let mut visible = 0;
        for (x, y) in [(64, 64), (128, 96), (180, 120), (100, 160), (128, 220)] {
            let source = cloud_sample(cloud_frame, x, y);
            visible += usize::from(source.w > 0.05);
            let expected =
                (source.truncate() * source.w + background / 255. * (1. - source.w)) * 255.;
            let pixel = &capture.rgba8()[(y * 256 + x) * 4..(y * 256 + x) * 4 + 3];
            for (actual, expected) in pixel.iter().zip(expected.to_array()) {
                assert!(
                    (f32::from(*actual) - expected).abs() < 2.,
                    "case {case}, pixel {x},{y}: {pixel:?}, expected {expected}, source {source}"
                );
            }
        }
        assert!(visible >= 2, "test must exercise actual cloud texels");
        if case == 0
            && let Some(path) = std::env::var_os("SOLARITY_CLOUD_CAPTURE_RGBA")
        {
            std::fs::write(path, capture.rgba8())?;
        }
    }
    Ok(())
}

/// Independent scalar reference for the original mesh's PCT fixed-function draw.
fn cloud_sample(frame: solarity_rendering::WorldCloudFrame<'_>, x: usize, y: usize) -> Vec4 {
    use glam::Vec2;
    let point = Vec2::new((x as f32 + 0.5) / 128. - 1., 1. - (y as f32 + 0.5) / 128.);
    let dome = frame.dome();
    for strip in dome.indices().windows(3) {
        let ids = [strip[0] as usize, strip[1] as usize, strip[2] as usize];
        let clip =
            ids.map(|i| frame.view_projection() * Vec3::from_array(dome.positions()[i]).extend(1.));
        if clip.iter().any(|v| v.w <= 0.) {
            continue;
        }
        let p = clip.map(|v| v.truncate().truncate() / v.w);
        let area = (p[1] - p[0]).perp_dot(p[2] - p[0]);
        if area.abs() < 1e-8 {
            continue;
        }
        let b = [
            (p[1] - point).perp_dot(p[2] - point) / area,
            (p[2] - point).perp_dot(p[0] - point) / area,
            (p[0] - point).perp_dot(p[1] - point) / area,
        ];
        if b.iter().any(|v| *v < 0.) {
            continue;
        }
        let q = [b[0] / clip[0].w, b[1] / clip[1].w, b[2] / clip[2].w];
        let sum = q.iter().sum::<f32>();
        let uv = (0..3)
            .map(|i| Vec2::from_array(dome.coordinates()[ids[i]]) * q[i])
            .sum::<Vec2>()
            / sum;
        let alpha = (0..3)
            .map(|i| (dome.colors()[ids[i]] >> 24) as f32 * q[i])
            .sum::<f32>()
            / (sum * 255.);
        let texel = uv * 128. - Vec2::splat(0.5);
        let origin = texel.floor();
        let blend = texel - origin;
        let pixel = |dx: i32, dy: i32| {
            let tx = (origin.x as i32 + dx).clamp(0, 127) as usize;
            let ty = (origin.y as i32 + dy).clamp(0, 127) as usize;
            let bytes = &frame.bgra8()[(ty * 128 + tx) * 4..(ty * 128 + tx) * 4 + 4];
            Vec4::new(
                f32::from(bytes[2]),
                f32::from(bytes[1]),
                f32::from(bytes[0]),
                f32::from(bytes[3]),
            ) / 255.
        };
        let mut sample = pixel(0, 0)
            .lerp(pixel(1, 0), blend.x)
            .lerp(pixel(0, 1).lerp(pixel(1, 1), blend.x), blend.y);
        sample.w *= alpha;
        return sample;
    }
    Vec4::ZERO
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
        TerrainSceneUniform::new(
            Mat4::IDENTITY,
            Mat4::IDENTITY,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
        ),
        WorldModelSceneUniform::new(
            Mat4::IDENTITY,
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
