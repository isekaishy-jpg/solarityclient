//! Original Diffuse_T1, MapObj and MapObjU fog pixels across camera projections.

use super::*;
use solarity_asset::DecodedWorldModel;
use solarity_rendering::{
    WorldModelBaseMip, WorldModelMaterialState, WorldModelMeshPlan, WorldModelSampledTexture,
    WorldModelSurfacePassPlan, WorldModelTextureFiltering, WorldModelTextureSet,
};

#[test]
#[allow(unsafe_code)] // Sole ownership of the hidden SDL surface transfers to Vulkan.
fn surface_fog_matches_original_depth_programs() -> Result<(), Box<dyn Error>> {
    let positions = [[-1_f32, -1., 0.], [3., -1., 0.], [-1., 3., 0.]];
    let mut bytes = render_m2_bytes("Surface", 1)?;
    let vertices = m2_array_offset(&bytes, 0x3c)?;
    for (index, position) in positions.iter().enumerate() {
        for (axis, value) in position.iter().enumerate() {
            let offset = vertices + index * 48 + axis * 4;
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    let materials = m2_array_offset(&bytes, 0x70)?;
    bytes[materials..materials + 2].copy_from_slice(&5_u16.to_le_bytes());
    bytes[materials + 2..materials + 4].fill(0);
    let mut skin = render_skin_bytes()?;
    let batches = u32::from_le_bytes(skin[40..44].try_into()?) as usize;
    // The second batch selects the single-texture Diffuse_T1/Combiners_Opaque pair.
    skin[batches + 24 + 14..batches + 24 + 16].copy_from_slice(&1u16.to_le_bytes());
    let white_bytes = solid_raw3_blp(1, 1, &[0xffff_ffff]);
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Surface.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Surface00.skin",
            bytes: &skin,
        },
        FixtureFile {
            path: "Surface.blp",
            bytes: &white_bytes,
        },
    ])?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = DecodedM2Model::load(&mut assets, &AssetPath::new("Surface.m2")?)?;
    let plan = M2MeshPlan::prepare(&model, 0)?;
    let draw_index = plan
        .draws()
        .iter()
        .position(|draw| draw.batch().material_index == 0)
        .ok_or("surface batch")?;
    let draw = &plan.draws()[draw_index];
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Native surface fog", 128, 128)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The window outlives the renderer and its solely owned surface.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (128, 128), 0) }?;
    let mesh = renderer.upload_m2_mesh(&plan)?;
    let shader = M2ShaderPlan::resolve(&model, draw)?;
    assert_eq!(shader.vertex_shader(), M2VertexShader::DiffuseT1);
    assert_eq!(shader.pixel_shader(), M2PixelShader::Opaque);
    let pipeline = renderer.prepare_m2_pipeline(
        shader,
        M2ShaderPermutation::resolve(
            draw,
            M2LocalLightCount::Zero,
            M2ShadowPermutation::Disabled,
            M2ShadowFiltering::Direct,
        ),
    )?;
    let white = renderer.upload_stock_m2_white()?;
    let world_white = BlpTextureSource::load(&mut assets, &AssetPath::new("Surface.blp")?)?;
    let world_white = renderer.upload_blp_texture(&world_white, BlpColorSpace::Linear)?;
    let sampler = renderer.prepare_m2_sampler(&model.textures()[0])?;
    let textures = renderer
        .prepare_m2_texture_sets(&[M2TextureSet::One(M2SampledTexture::new(white, sampler))])?[0];
    let fog_color = Vec3::new(204. / 255., 51. / 255., 102. / 255.);
    let mut captures = 0;
    for family in ["m2", "wmo", "wmo_u"] {
        let mut root = crate::world_model::root_fixture();
        let texture_chunk = root
            .windows(4)
            .position(|value| value == b"XTOM")
            .ok_or("MOTX")?;
        root[texture_chunk + 4..texture_chunk + 8].copy_from_slice(&12u32.to_le_bytes());
        root.splice(texture_chunk + 8..texture_chunk + 9, *b"Surface.blp\0");
        let flags = if family == "wmo_u" { 10u32 } else { 8u32 };
        chunk_mut(&mut root, b"DHOM")?[60..64].copy_from_slice(&flags.to_le_bytes());
        let material = chunk_mut(&mut root, b"TMOM")?;
        material[..4].copy_from_slice(&5u32.to_le_bytes());
        material[8..12].fill(0);
        material[16..20].fill(0);
        let mut group = crate::world_model::group_fixture();
        group[60..64].fill(0);
        group[64..66].copy_from_slice(&1u16.to_le_bytes());
        let vertices = chunk_mut(&mut group[88..], b"TVOM")?;
        for (offset, value) in positions.iter().flatten().enumerate() {
            vertices[offset * 4..offset * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        chunk_mut(&mut group[88..], b"VCOM")?.copy_from_slice(&[96, 64, 32, 255].repeat(3));
        let world_fixture = Fixture::new(&[
            FixtureFile {
                path: "Surface.wmo",
                bytes: &root,
            },
            FixtureFile {
                path: "Surface_000.wmo",
                bytes: &group,
            },
        ])?;
        let mut assets = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(world_fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let world_model = DecodedWorldModel::load(&mut assets, &AssetPath::new("Surface.wmo")?)?;
        let world_plan = WorldModelMeshPlan::prepare(&world_model)?;
        let world_mesh = renderer.upload_world_model_mesh(&world_plan)?;
        let material = &world_plan.materials()[0];
        let sampler = renderer.prepare_world_model_sampler(
            WorldModelMaterialState::from_material(material),
            WorldModelTextureFiltering::Bilinear,
            WorldModelBaseMip::Zero,
        )?;
        let world_textures =
            renderer.prepare_world_model_texture_sets(&[WorldModelTextureSet::One(
                WorldModelSampledTexture::new(world_white, sampler),
            )])?[0];
        let passes = WorldModelSurfacePassPlan::prepare(
            world_plan.root_flags(),
            world_plan.groups()[0].flags(),
            world_plan.draws()[0].class(),
            material,
        );
        assert_eq!(passes.passes().len(), 1);
        let world_pipeline =
            renderer.prepare_world_model_pipeline(passes.is_unified(), passes.passes()[0])?;
        for row in include_str!("../../fixtures/surface_fog_native.txt")
            .lines()
            .filter(|line| line.split_whitespace().next() == Some(family))
        {
            let values = row
                .split_whitespace()
                .skip(1)
                .map(str::parse::<f32>)
                .collect::<Result<Vec<_>, _>>()?;
            let (depth, lateral, exponent) = (values[0], values[1], values[2]);
            for (eye, direction, up) in [
                (Vec3::ZERO, Vec3::NEG_Z, Vec3::Y),
                (Vec3::new(17000., -4200., 150.), Vec3::X, Vec3::Z),
            ] {
                for perspective in [false, true] {
                    let camera = if perspective {
                        WorldCamera::stock(eye, eye + direction, up, 100.)
                    } else {
                        WorldCamera::orthographic(
                            eye,
                            eye + direction,
                            up,
                            [-10., 10.],
                            [-10., 10.],
                            0.1,
                            100.,
                        )
                    }
                    .frame(1.)?;
                    let center = Vec3::new(lateral, 0., -depth);
                    let clip = camera.projection() * center.extend(1.);
                    let ndc = clip.truncate() / clip.w;
                    if ndc.x.abs() > 0.85 {
                        continue;
                    }
                    let placement = camera.view().inverse() * Mat4::from_translation(center);
                    let fog = Vec4::new(2., 22., 0., exponent);
                    let scene = WorldFrameScene::new(
                        TerrainSceneUniform::new(
                            camera.projection(),
                            camera.view(),
                            Vec3::ONE,
                            Vec3::ZERO,
                            Vec3::Z,
                        ),
                        WorldModelSceneUniform::new(
                            camera.projection(),
                            camera.view(),
                            eye,
                            Vec3::ONE,
                            Vec3::ZERO,
                            Vec3::Z,
                            fog,
                        ),
                        M2SceneUniform::new(
                            camera.projection(),
                            camera.view(),
                            eye,
                            Vec3::ONE,
                            Vec3::ZERO,
                            Vec3::Z,
                            fog,
                            fog_color,
                            [M2LocalLightState::disabled(); 4],
                        ),
                    );
                    let mut model_draws = Vec::new();
                    let mut world_draws = Vec::new();
                    let mut bones = Vec::new();
                    if family == "m2" {
                        let draw = renderer.prepare_m2_draw(
                            mesh,
                            pipeline,
                            textures,
                            &plan,
                            draw_index,
                            false,
                            M2MaterialUniform::new(
                                placement,
                                [Mat4::IDENTITY; 2],
                                camera.view() * placement,
                                Vec4::new(64. / 255., 128. / 255., 192. / 255., 1.),
                                fog_color.extend(0.),
                                Vec4::new(0., 1., 0., 0.),
                            ),
                            0,
                            0,
                        )?;
                        bones.resize(draw.required_bone_transforms(), Mat4::IDENTITY);
                        model_draws.push(draw);
                    } else {
                        world_draws.push(renderer.prepare_world_model_draw(
                            world_mesh,
                            world_pipeline,
                            world_textures,
                            &world_plan,
                            0,
                            0,
                            placement,
                            1.,
                            fog_color,
                        )?);
                    }
                    renderer.request_frame_capture()?;
                    renderer.present_world_frame(
                        scene,
                        &bones,
                        &[],
                        &world_draws,
                        &model_draws,
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                    )?;
                    let capture = renderer
                        .take_captured_frame()?
                        .ok_or("surface fog capture")?;
                    let pixel = rgba8_pixel(
                        capture.rgba8(),
                        128,
                        ((ndc.x * 0.5 + 0.5) * 128.) as u32,
                        64,
                    );
                    for (actual, expected) in pixel.iter().zip(&values[3..]) {
                        assert!(
                            (f32::from(*actual) - expected).abs() <= 1.,
                            "{row}, eye={eye:?}, perspective={perspective}: {pixel:?}"
                        );
                    }
                    captures += 1;
                }
            }
        }
    }
    assert!(captures >= 400, "only {captures} captures");
    Ok(())
}

fn chunk_mut<'a>(bytes: &'a mut [u8], magic: &[u8; 4]) -> Result<&'a mut [u8], Box<dyn Error>> {
    let mut offset = 0;
    while offset + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into()?) as usize;
        if &bytes[offset..offset + 4] == magic {
            return Ok(&mut bytes[offset + 8..offset + 8 + size]);
        }
        offset += 8 + size;
    }
    Err("missing surface chunk".into())
}
