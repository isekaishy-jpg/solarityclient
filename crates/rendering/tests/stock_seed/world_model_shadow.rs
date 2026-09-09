//! All MapObj material families receive the shared animated unit map.

use crate::support::{Fixture, FixtureFile};
use glam::{Mat4, Vec3};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureSource, ClientDataRoot, DecodedWorldModel,
    Locale,
};
use solarity_rendering::{
    BlpColorSpace, M2PreparedDraw, VulkanRenderer, WorldFrameScene, WorldModelBaseMip,
    WorldModelMaterialState, WorldModelMeshPlan, WorldModelSampledTexture,
    WorldModelSurfacePassPlan, WorldModelTextureFiltering, WorldModelTextureSet,
    WorldPrimaryShadowFrame, WorldShadowProjection,
};
use std::error::Error;

/// Compares ordinary, empty-map, and occupied-map frames at a large world placement.
pub(super) fn compare_receivers(
    renderer: &mut VulkanRenderer,
    scene: WorldFrameScene<'_>,
    base: Vec3,
    shadow: WorldShadowProjection,
    casters: &[M2PreparedDraw],
    bones: &[Mat4],
) -> Result<(), Box<dyn Error>> {
    for unified in [false, true] {
        for shader in 0_u32..=6 {
            // Ordinary MapObj's final effect-table entry is null; only MapObjU
            // admits Composite (the decoder deliberately rejects the former).
            if !unified && shader == 6 {
                continue;
            }
            let mut root = crate::world_model::root_fixture();
            let strings = root
                .windows(4)
                .position(|bytes| bytes == b"XTOM")
                .ok_or("fixture texture strings")?;
            root.splice(strings + 8..strings + 9, b"Shadow.blp\0".iter().copied());
            root[strings + 4..strings + 8].copy_from_slice(&11_u32.to_le_bytes());
            chunk_mut(&mut root, b"DHOM")?[60..64]
                .copy_from_slice(&(u32::from(unified) * 2).to_le_bytes());
            let material = chunk_mut(&mut root, b"TMOM")?;
            material[0..4].copy_from_slice(&6_u32.to_le_bytes());
            material[4..8].copy_from_slice(&shader.to_le_bytes());
            material[8..12].fill(0);
            material[16..20].fill(0);
            let mut group = crate::world_model::group_fixture();
            // Exterior opaque surface with neutral MOCV and one ordinary batch.
            group[28..32].copy_from_slice(&8_u32.to_le_bytes());
            group[60..64].fill(0);
            group[64..66].copy_from_slice(&1_u16.to_le_bytes());
            let colors = chunk_mut(&mut group[88..], b"VCOM")?;
            for vertex in colors.as_chunks_mut::<4>().0 {
                vertex.copy_from_slice(&[128, 128, 128, 255]);
            }
            let texture = crate::model::solid_raw3_blp(2, 2, &[0x2020_2020]);
            let fixture = Fixture::new(&[
                FixtureFile {
                    path: "Shadow.wmo",
                    bytes: &root,
                },
                FixtureFile {
                    path: "Shadow_000.wmo",
                    bytes: &group,
                },
                FixtureFile {
                    path: "Shadow.blp",
                    bytes: &texture,
                },
            ])?;
            let mut store = AssetStore::mount(ArchiveCatalog::discover(
                ClientDataRoot::new(fixture.data_root())?,
                Locale::EnUs,
            )?)?;
            let model = DecodedWorldModel::load(&mut store, &AssetPath::new("Shadow.wmo")?)?;
            let plan = WorldModelMeshPlan::prepare(&model)?;
            let mesh = renderer.upload_world_model_mesh(&plan)?;
            let material = &plan.materials()[0];
            let sampler = renderer.prepare_world_model_sampler(
                WorldModelMaterialState::from_material(material),
                WorldModelTextureFiltering::Bilinear,
                WorldModelBaseMip::Zero,
            )?;
            let source = BlpTextureSource::load(&mut store, &AssetPath::new("Shadow.blp")?)?;
            let texture = renderer.upload_blp_texture(&source, BlpColorSpace::Linear)?;
            let stage = WorldModelSampledTexture::new(texture, sampler);
            let textures = if matches!(shader, 3 | 5 | 6) {
                WorldModelTextureSet::Two([stage; 2])
            } else {
                WorldModelTextureSet::One(stage)
            };
            let textures = renderer.prepare_world_model_texture_sets(&[textures])?[0];
            let passes = WorldModelSurfacePassPlan::prepare(
                plan.root_flags(),
                plan.groups()[0].flags(),
                plan.draws()[0].class(),
                material,
            );
            assert_eq!(passes.passes().len(), 1);
            let pipeline = renderer.prepare_world_model_pipeline(unified, passes.passes()[0])?;
            let draw = renderer.prepare_world_model_draw(
                mesh,
                pipeline,
                textures,
                &plan,
                0,
                0,
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(8.),
                    glam::Quat::IDENTITY,
                    base - Vec3::new(4., 4., 0.),
                ),
                1.,
                Vec3::ZERO,
            )?;
            let mut colors = Vec::new();
            for casting in [None, Some(0), Some(casters.len())] {
                let frame_scene = casting.map_or(scene, |count| {
                    scene.with_primary_shadows(WorldPrimaryShadowFrame::new(
                        shadow,
                        &casters[..count],
                    ))
                });
                renderer.request_frame_capture()?;
                renderer.present_world_frame(
                    frame_scene,
                    bones,
                    &[],
                    &[draw],
                    &[],
                    &[],
                    &[],
                    &[],
                    &[],
                    &[],
                )?;
                let capture = renderer
                    .take_captured_frame()?
                    .ok_or("WMO shadow capture")?;
                let offset = (32 * 64 + 32) * 4;
                colors.push(<[u8; 4]>::try_from(&capture.rgba8()[offset..offset + 4])?);
            }
            assert_eq!(
                colors[0], colors[1],
                "empty WMO map: unified={unified}, shader={shader}"
            );
            assert!(colors[0][0] > 20, "visible WMO: {colors:?}");
            assert!(
                colors[2][0] + 3 < colors[0][0],
                "shadowed WMO: unified={unified}, shader={shader}, {colors:?}"
            );
            assert_eq!(colors[0][3], colors[2][3], "shadow retains surface opacity");
        }
    }
    Ok(())
}

/// Locates a decoded fixture chunk without relying on preceding payload sizes.
fn chunk_mut<'a>(bytes: &'a mut [u8], magic: &[u8; 4]) -> Result<&'a mut [u8], Box<dyn Error>> {
    let mut cursor = 0;
    while cursor + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into()?) as usize;
        if &bytes[cursor..cursor + 4] == magic {
            return Ok(&mut bytes[cursor + 8..cursor + 8 + size]);
        }
        cursor += 8 + size;
    }
    Err("fixture chunk absent".into())
}
