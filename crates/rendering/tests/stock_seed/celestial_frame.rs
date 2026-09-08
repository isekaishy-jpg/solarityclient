//! Actual celestial descriptors, perspective interpolation, native strips and ordering.
use super::*;
use glam::Vec2;
use solarity_rendering::{
    WorldCamera, WorldCelestialDraw, WorldCelestialFrame, WorldCelestialMesh, WorldCelestials,
    WorldSkyDome, WorldSkyFrame,
};

#[test]
#[allow(unsafe_code)] // SDL transfers the hidden surface to Vulkan ownership.
fn celestial_frames_clip_sample_blend_and_reuse_slots() -> Result<(), Box<dyn Error>> {
    let pixels = [0xffe08040, 0x80a0c060, 0x402080c0, 0xc0c04080];
    let bytes = crate::model::raw3_blp_pixels(2, 2, &pixels);
    let fixture = Fixture::new(&[FixtureFile {
        path: "Celestial.blp",
        bytes: &bytes,
    }])?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let source = BlpTextureSource::load(&mut assets, &AssetPath::new("Celestial.blp")?)?;
    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity celestial frame test", 256, 256)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: Bootstrap enabled the live window's required surface extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: The window outlives the renderer's sole surface ownership.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (256, 256), 0) }?;
    let texture = renderer.upload_blp_texture(&source, BlpColorSpace::Linear)?;
    let white = renderer.upload_stock_m2_white()?;
    let world = renderer.upload_liquid_mesh(&triangle(0.8, [255, 0, 0, 255]), &[0, 1, 2])?;
    let uniform = LiquidShaderUniform::new(
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        LiquidLighting::new(-Vec3::Z, Vec3::ONE, Vec3::ZERO, Vec3::ZERO),
        LiquidFog::new(Vec3::new(0., 1., 1.), Vec3::ZERO),
    );
    let world_draws =
        [renderer.prepare_liquid_draw(world, LiquidDrawMaterial::Magma, white, uniform)?];
    let depth = LiquidDepthTexture::prepare(LiquidDepthTextureKind::River, [0; 2], [0; 2]);
    let mut sky = WorldSkyDome::new();
    let background = Vec3::new(12., 24., 48.);
    let mut visible = 0;
    for (case, day) in [
        0.23, 0.24, 0.25, 0.26, 0.27, 0.5, 0.86, 0.875, 0.885, 0., 0.02, 0.15, 0.94, 0.97, 0.99,
        0.5,
    ]
    .into_iter()
    .enumerate()
    {
        let eye = if case % 2 == 0 {
            Vec3::ZERO
        } else {
            Vec3::new(12345., -6789., 100.)
        };
        let bodies = WorldCelestials::sample(day, 20000., eye).bodies();
        let selected = if (9..15).contains(&case) { 1 } else { 0 };
        let camera = WorldCamera::new(eye, bodies[selected].position(), Vec3::Z, 0.4, 0.5, 777.)
            .frame(1.)?;
        let alpha = [255, 64, 128, 192][case % 4];
        let meshes = bodies.map(|body| WorldCelestialMesh::new(body, eye, alpha << 24 | 0xe0c0a0));
        let handles = if case % 2 == 0 {
            [texture, white, texture]
        } else {
            [white, texture, white]
        };
        let frame = WorldCelestialFrame::new(std::array::from_fn(|i| {
            WorldCelestialDraw::new(&meshes[i], bodies[i], handles[i], camera)
        }));
        sky.update_colors([background / 255.; 5], background / 255., 0., 0., camera);
        let mut scene = scene()
            .with_celestials(frame)
            .with_sky(WorldSkyFrame::new(&sky, camera));
        if case == 15 {
            scene = scene.with_liquids(LiquidFrame::new(&world_draws, &depth, &depth, &depth, 0));
        }
        renderer.request_frame_capture()?;
        let report =
            renderer.present_world_frame(scene, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
        assert_eq!(report.celestial_draw_count(), frame.draw_count());
        let capture = renderer
            .take_captured_frame()?
            .ok_or("missing celestial capture")?;
        for y in (16..240).step_by(3) {
            for x in (16..240).step_by(3) {
                let mut expected = Vec3::ZERO;
                let mut edge = false;
                for draw in frame.draws() {
                    let (sample, near_edge) = sample(
                        draw,
                        x,
                        y,
                        if draw.texture() == white {
                            &[0xffffffff; 4]
                        } else {
                            &pixels
                        },
                    );
                    edge |= near_edge;
                    visible += usize::from(sample.w > 0.02);
                    expected = sample.truncate() * sample.w + expected * (1. - sample.w);
                }
                if edge {
                    continue;
                } // Rasterizer subpixel coverage is not the shading contract.
                expected = expected * 255. + background;
                if case == 15 {
                    expected = Vec3::new(255., 0., 0.);
                }
                let pixel = &capture.rgba8()[(y * 256 + x) * 4..(y * 256 + x) * 4 + 3];
                for (a, b) in pixel.iter().zip(expected.to_array()) {
                    // Texture filtering, an RGBA8 blend store, and the packed
                    // additive gradient each contribute final-byte rounding.
                    assert!(
                        (f32::from(*a) - b.min(255.)).abs() <= 3.,
                        "case {case}, pixel {x},{y}: {pixel:?}, expected {expected}"
                    );
                }
            }
        }
        if case == 3
            && let Some(path) = std::env::var_os("SOLARITY_CELESTIAL_CAPTURE_RGBA")
        {
            std::fs::write(path, capture.rgba8())?;
        }
    }
    assert!(visible > 300, "test must cover visible textured strips");
    renderer.retire_liquid_meshes(&[world])?;
    Ok(())
}

fn sample(draw: WorldCelestialDraw<'_>, x: usize, y: usize, pixels: &[u32; 4]) -> (Vec4, bool) {
    let point = Vec2::new((x as f32 + 0.5) / 128. - 1., 1. - (y as f32 + 0.5) / 128.);
    let mesh = draw.mesh();
    for strip in mesh.indices().windows(3) {
        let ids = [strip[0] as usize, strip[1] as usize, strip[2] as usize];
        let clip =
            ids.map(|i| draw.view_projection() * Vec3::from_array(mesh.positions()[i]).extend(1.));
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
        let edge = b.iter().any(|v| *v < 0.002);
        let q = [b[0] / clip[0].w, b[1] / clip[1].w, b[2] / clip[2].w];
        let sum = q.iter().sum::<f32>();
        let uv = (0..3)
            .map(|i| Vec2::from_array(mesh.uv()[ids[i]]) * q[i])
            .sum::<Vec2>()
            / sum;
        let color = (0..3)
            .map(|i| unpack(mesh.colors()[ids[i]]) * q[i])
            .sum::<Vec4>()
            / sum;
        let texel = uv * 2. - Vec2::splat(0.5);
        let origin = texel.floor();
        let blend = texel - origin;
        let pixel = |x: i32, y: i32| {
            unpack(
                pixels[((origin.y as i32 + y).clamp(0, 1) * 2 + (origin.x as i32 + x).clamp(0, 1))
                    as usize],
            )
        };
        return (
            color
                * pixel(0, 0)
                    .lerp(pixel(1, 0), blend.x)
                    .lerp(pixel(0, 1).lerp(pixel(1, 1), blend.x), blend.y),
            edge,
        );
    }
    (Vec4::ZERO, false)
}

fn unpack(color: u32) -> Vec4 {
    Vec4::new(
        ((color >> 16) & 255) as f32,
        ((color >> 8) & 255) as f32,
        (color & 255) as f32,
        (color >> 24) as f32,
    ) / 255.
}
