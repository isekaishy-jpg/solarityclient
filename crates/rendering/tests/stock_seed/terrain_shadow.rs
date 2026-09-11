//! Animated stock M2 silhouettes rendered into the primary terrain shadow map.

use crate::support::{Fixture, FixtureFile};
use glam::{Mat4, Vec3, Vec4};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureSource, ClientDataRoot, DecodedM2Model, Locale,
};
use solarity_rendering::{
    BlpColorSpace, M2LocalLightCount, M2LocalLightState, M2MaterialUniform, M2MeshPlan,
    M2SampledTexture, M2SceneUniform, M2ShaderPermutation, M2ShaderPlan, M2ShadowFiltering,
    M2ShadowPermutation, M2TextureSet, TerrainPreparedDraw, TerrainSceneUniform, VulkanRenderer,
    WorldFrameScene, WorldModelSceneUniform, WorldPrimaryShadowFrame, WorldShadowProjection,
    WorldShadowQuality,
};
use std::error::Error;

/// Covers animated palette offsets, cutout boundaries, empty maps, and slot resizing.
pub(super) fn compare_unit_shadow(
    renderer: &mut VulkanRenderer,
    terrain: TerrainPreparedDraw,
) -> Result<(), Box<dyn Error>> {
    for bone_class in [0, 1, 2] {
        compare_bone_class(renderer, terrain, bone_class)?;
    }
    Ok(())
}

/// Rigid, single-bone, and weighted silhouettes share the original map contract.
fn compare_bone_class(
    renderer: &mut VulkanRenderer,
    terrain: TerrainPreparedDraw,
    bone_class: u16,
) -> Result<(), Box<dyn Error>> {
    let mut model_bytes = crate::model::render_m2_bytes("Shadow", 1)?;
    let vertices = u32::from_le_bytes(model_bytes[0x40..0x44].try_into()?) as usize;
    // One broad triangle above the receiver; its visible world queue stays empty.
    for (index, position) in [[-12_f32, -12., 2.], [12., -12., 2.], [0., 12., 2.]]
        .iter()
        .enumerate()
    {
        for (axis, value) in position.iter().enumerate() {
            let offset = vertices + index * 48 + axis * 4;
            model_bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    let materials = u32::from_le_bytes(model_bytes[0x74..0x78].try_into()?) as usize;
    model_bytes[materials + 4..materials + 6].copy_from_slice(&0_u16.to_le_bytes());
    let mut skin = crate::model::render_skin_bytes()?;
    let section = u32::from_le_bytes(skin[32..36].try_into()?) as usize;
    skin[section + 16..section + 18].copy_from_slice(&bone_class.to_le_bytes());
    if bone_class == 2 {
        // Four independent weights resolve through the SKIN palette. Opposed
        // transforms below cancel only when every weighted term is retained.
        let lookup = u32::try_from(model_bytes.len())?;
        model_bytes[0x78..0x7c].copy_from_slice(&4_u32.to_le_bytes());
        model_bytes[0x7c..0x80].copy_from_slice(&lookup.to_le_bytes());
        for bone in [0_u16, 1, 2, 2] {
            model_bytes.extend_from_slice(&bone.to_le_bytes());
        }
        skin[section + 12..section + 14].copy_from_slice(&4_u16.to_le_bytes());
        let palette = u32::from_le_bytes(skin[24..28].try_into()?) as usize;
        for vertex in 0..3 {
            model_bytes[vertices + vertex * 48 + 12..vertices + vertex * 48 + 16]
                .copy_from_slice(&[64, 64, 64, 63]);
            skin[palette + vertex * 4..palette + vertex * 4 + 4].copy_from_slice(&[0, 1, 2, 3]);
        }
    }
    let batches = u32::from_le_bytes(skin[40..44].try_into()?) as usize;
    skin[batches + 12..batches + 14].copy_from_slice(&0_u16.to_le_bytes());
    let opaque = super::solid_raw3_blp(2, 2, 0x80FF_FFFF);
    let transparent = super::solid_raw3_blp(2, 2, 0x7FFF_FFFF);
    let model_path = format!("Creature/Solarity/Shadow{bone_class}.m2");
    let skin_path = format!("Creature/Solarity/Shadow{bone_class}00.skin");
    let fixture = Fixture::new(&[
        FixtureFile {
            path: &model_path,
            bytes: &model_bytes,
        },
        FixtureFile {
            path: &skin_path,
            bytes: &skin,
        },
        FixtureFile {
            path: "Creature/Solarity/Shadow128.blp",
            bytes: &opaque,
        },
        FixtureFile {
            path: "Creature/Solarity/Shadow127.blp",
            bytes: &transparent,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = DecodedM2Model::load(&mut store, &AssetPath::new(model_path)?)?;
    let plan = M2MeshPlan::prepare(&model, 0)?;
    let mesh = renderer.upload_m2_mesh(&plan)?;
    let draw = &plan.draws()[0];
    let permutation = M2ShaderPermutation::resolve(
        draw,
        M2LocalLightCount::Zero,
        M2ShadowPermutation::Disabled,
        M2ShadowFiltering::Direct,
    );
    let pipeline =
        renderer.prepare_m2_pipeline(M2ShaderPlan::resolve(&model, draw)?, permutation)?;
    let sampler = renderer.prepare_m2_sampler(&model.textures()[0])?;
    let mut texture_sets = Vec::new();
    for path in [
        "Creature/Solarity/Shadow128.blp",
        "Creature/Solarity/Shadow127.blp",
    ] {
        let source = BlpTextureSource::load(&mut store, &AssetPath::new(path)?)?;
        let texture = renderer.upload_blp_texture(&source, BlpColorSpace::Linear)?;
        let stage = M2SampledTexture::new(texture, sampler);
        texture_sets.push(renderer.prepare_m2_texture_sets(&[M2TextureSet::Two([stage; 2])])?[0]);
    }
    let center = Vec3::new(-16., -16., 0.);
    let eye = center + Vec3::Z * 10.;
    let view = Mat4::look_at_rh(eye, center, Vec3::Y);
    let projection = Mat4::orthographic_rh(-10., 10., -10., 10., 0.1, 100.);
    let fog = Vec4::ZERO;
    let terrain_scene =
        TerrainSceneUniform::new(projection, view, Vec3::splat(0.25), Vec3::ZERO, Vec3::Z);
    let scene = WorldFrameScene::new(
        terrain_scene,
        WorldModelSceneUniform::new(projection, view, eye, Vec3::ONE, Vec3::ZERO, Vec3::Z, fog),
        M2SceneUniform::new(
            projection,
            view,
            eye,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            fog,
            Vec3::ZERO,
            [M2LocalLightState::disabled(); 4],
        ),
    );
    let mut cases = 0;
    for quality_value in [1, 2, 1] {
        let quality = WorldShadowQuality::from_cvar(quality_value).ok_or("quality")?;
        // 7BBC50 disables culling for both windings. Oblique rays also catch
        // mismatched caster/receiver signs that a vertical light would hide.
        for (scale_x, ray) in [
            (1., -Vec3::Z),
            (-1., -Vec3::Z),
            (1., Vec3::new(0.3, 0.4, -0.866_025_4)),
            (-1., Vec3::new(0.3, 0.4, -0.866_025_4)),
        ] {
            for (texture_index, movement, count, expected) in [
                (0, 0., 1, 45_u8),
                (1, 0., 1, 64),
                (0, 30., 1, 64),
                (0, 0., 0, 64),
            ] {
                let material = M2MaterialUniform::new(
                    Mat4::from_translation(
                        center
                            + if bone_class == 0 {
                                Vec3::X * movement
                            } else {
                                Vec3::ZERO
                            },
                    ) * Mat4::from_scale(Vec3::new(scale_x, 1., 1.)),
                    [Mat4::IDENTITY; 2],
                    view,
                    Vec4::ONE,
                    Vec4::ZERO,
                    Vec4::ZERO,
                );
                let caster = renderer.prepare_m2_draw(
                    mesh,
                    pipeline,
                    texture_sets[texture_index],
                    &plan,
                    0,
                    false,
                    material,
                    4,
                    0,
                )?;
                assert!(caster.shadow_material().is_some());
                let casters = [caster];
                let mut bones = vec![Mat4::IDENTITY; caster.required_bone_transforms().max(7)];
                bones[6] = Mat4::from_translation(Vec3::X * movement);
                if bone_class == 2 {
                    bones[4] = Mat4::from_translation(Vec3::X * 30.);
                    bones[5] = Mat4::from_translation(Vec3::X * -30.);
                }
                let shadow = WorldShadowProjection::primary(quality, center, eye, ray)?;
                renderer.request_frame_capture()?;
                let report = renderer.present_world_frame(
                    scene.with_primary_shadows(WorldPrimaryShadowFrame::new(
                        shadow,
                        &casters[..count],
                    )),
                    &bones,
                    &[terrain],
                    &[],
                    &[],
                    &[],
                    &[],
                    &[],
                    &[],
                    &[],
                )?;
                assert_eq!(report.primary_shadow_draw_count(), count);
                let frame = renderer.take_captured_frame()?.ok_or("shadow capture")?;
                for y in [28, 32, 36] {
                    for x in [28, 32, 36] {
                        let pixel = &frame.rgba8()[(y * 64 + x) * 4..(y * 64 + x) * 4 + 3];
                        assert!(
                            pixel[1].abs_diff(expected) <= 2,
                            "bones={bone_class}, quality={quality_value}, texture={texture_index}, movement={movement}, count={count}: {x}/{y}={pixel:?}, expected green {expected}"
                        );
                    }
                }
                cases += 1;
                if bone_class == 1
                    && quality_value == 2
                    && scale_x == 1.
                    && ray == -Vec3::Z
                    && texture_index == 0
                    && movement == 0.
                    && count == 1
                {
                    crate::environment_shadow::compare_cache(
                        renderer, scene, terrain, caster, &bones, center, eye,
                    )?;
                }
            }
        }
    }
    assert_eq!(cases, 48);
    if bone_class != 1 {
        return Ok(());
    }
    // The same mesh now receives a separate caster through the M2 descriptor
    // bank. Translation and eye depth vary independently of the map center.
    for height in [0., 10_000.] {
        for eye_depth in [5., 20.] {
            let base = Vec3::new(-16_000., 12_000., height);
            let eye = base + Vec3::Z * eye_depth;
            let view = Mat4::look_at_rh(eye, base, Vec3::Y);
            let projection = Mat4::orthographic_rh(-10., 10., -10., 10., 0.1, 100.);
            let scene = WorldFrameScene::new(
                terrain_scene,
                WorldModelSceneUniform::new(
                    projection,
                    view,
                    eye,
                    Vec3::ONE,
                    Vec3::ZERO,
                    Vec3::Z,
                    fog,
                ),
                M2SceneUniform::new(
                    projection,
                    view,
                    eye,
                    Vec3::ONE,
                    Vec3::ZERO,
                    Vec3::Z,
                    fog,
                    Vec3::ZERO,
                    [M2LocalLightState::disabled(); 4],
                ),
            );
            let receiver_model = Mat4::from_translation(base - Vec3::Z * 2.);
            let material = M2MaterialUniform::new(
                receiver_model,
                [Mat4::IDENTITY; 2],
                view * receiver_model,
                Vec4::new(0.2, 0.2, 0.2, 1.),
                Vec4::ZERO,
                Vec4::ZERO,
            );
            let receiver = renderer.prepare_m2_draw(
                mesh,
                pipeline,
                texture_sets[0],
                &plan,
                0,
                false,
                material,
                4,
                0,
            )?;
            let caster_model = Mat4::from_translation(base);
            let material = M2MaterialUniform::new(
                caster_model,
                [Mat4::IDENTITY; 2],
                view * caster_model,
                Vec4::ONE,
                Vec4::ZERO,
                Vec4::ZERO,
            );
            let casters = [renderer.prepare_m2_draw(
                mesh,
                pipeline,
                texture_sets[0],
                &plan,
                0,
                false,
                material,
                4,
                0,
            )?];
            let bones = vec![Mat4::IDENTITY; receiver.required_bone_transforms()];
            let shadow =
                WorldShadowProjection::primary(WorldShadowQuality::UnitsHigh, base, eye, -Vec3::Z)?;
            if height == 10_000. && eye_depth == 20. {
                crate::world_model_shadow::compare_receivers(
                    renderer, scene, base, shadow, &casters, &bones,
                )?;
            }
            if eye_depth == 20. {
                super::ground_detail_shadow::compare_receivers(renderer, base, &casters, &bones)?;
            }
            let mut colors = Vec::new();
            for casting in [None, Some(0), Some(1)] {
                let scene = casting.map_or(scene, |count| {
                    scene.with_primary_shadows(WorldPrimaryShadowFrame::new(
                        shadow,
                        &casters[..count],
                    ))
                });
                renderer.request_frame_capture()?;
                renderer.present_world_frame(
                    scene,
                    &bones,
                    &[],
                    &[],
                    &[receiver],
                    &[],
                    &[],
                    &[],
                    &[],
                    &[],
                )?;
                let capture = renderer
                    .take_captured_frame()?
                    .ok_or("M2 shadow receiver capture")?;
                colors.push(capture.rgba8()[(32 * 64 + 32) * 4]);
            }
            assert!(colors[0] > 50, "M2 receiver must be visible: {colors:?}");
            assert_eq!(colors[0], colors[1], "an empty M2 map retains lighting");
            // Combiners_Opaque's normal relief is (1.2 - abs(dot))^4.
            let expected = (f32::from(colors[0]) * (0.7 + 0.3 * 0.2_f32.powi(4))).round() as u8;
            assert!(
                colors[2].abs_diff(expected) <= 2,
                "height={height}, eye_depth={eye_depth}: {colors:?}, expected {expected}"
            );
        }
    }
    Ok(())
}
