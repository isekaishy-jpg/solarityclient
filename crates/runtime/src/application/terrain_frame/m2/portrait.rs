//! Frozen portrait snapshots share resident appearance resources, never playback.

use super::*;
use glam::{Vec3, Vec4};
use solarity_rendering::{BlpTextureHandle, M2SceneUniform, WorldCamera, sample_m2_camera_frame};

impl M2Frame {
    /// Reads the complete published appearance only after its generation matches.
    /// The local palettes and draw list leave live animation, effects, and RNG untouched.
    pub(in crate::application) fn render_player_portrait(
        &self,
        renderer: &mut VulkanRenderer,
        player: &ResidentPlayerFrameInput<'_>,
        mask: BlpTextureHandle,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        let guid = player.guid();
        let Some(body) = self.placements.iter().find(|placement| {
            placement.owner == (M2GpuPlacementOwner::PlayerBody { guid })
                && placement
                    .unit_presentation
                    .as_ref()
                    .is_some_and(|generation| generation.matches(player.generation()))
        }) else {
            return Ok(false);
        };
        let Some(body_source) = self.sources[body.source_index].as_ref() else {
            return Ok(false);
        };
        let clock = portrait_clock(&body_source.model, &self.animations);
        let camera = portrait_camera(&body_source.model, clock)?;
        let mut bones = Vec::new();
        let mut draws = Vec::new();
        let mut transparent = Vec::new();
        let mut poses = Vec::new();
        let mut pose_scratch = M2BonePose::default();
        for (index, placement) in self.placements.iter().enumerate() {
            let (transform, parent_distance_sort) = match placement.owner {
                M2GpuPlacementOwner::PlayerBody { guid: owner } if owner == guid => {
                    (Mat4::IDENTITY, true)
                }
                M2GpuPlacementOwner::PlayerItem { guid: owner, point } if owner == guid => {
                    // 0x004EAF70 removes transient unit attachments; every current
                    // equipment slot belongs to the retained stock set.
                    let Some((transform, distance_sort)) = attachment_transform(
                        &poses,
                        M2GpuPlacementOwner::PlayerBody { guid },
                        point.id(),
                    )?
                    else {
                        continue;
                    };
                    (
                        transform * placement.orientation.local_transform(),
                        distance_sort,
                    )
                }
                M2GpuPlacementOwner::PlayerItemVisual {
                    guid: owner,
                    item_point,
                    effect_point,
                } if owner == guid => {
                    let Some((transform, distance_sort)) = attachment_transform(
                        &poses,
                        M2GpuPlacementOwner::PlayerItem {
                            guid,
                            point: item_point,
                        },
                        effect_point,
                    )?
                    else {
                        continue;
                    };
                    (transform, distance_sort)
                }
                _ => continue,
            };
            let Some(source) = self.sources[placement.source_index].as_ref() else {
                continue;
            };
            let clock = portrait_clock(&source.model, &self.animations);
            let model_distance_sort =
                source.model.skin_profile_count() >= 2 && parent_distance_sort;
            let model_view = camera.view() * transform;
            pose_scratch.recompose_with_model_view_and_orientation_mask(
                source.model.animations(),
                clock,
                model_view,
                &source.model_oriented_billboard_bones,
            )?;
            let bone_offset = u32::try_from(bones.len())
                .map_err(|_| solarity_rendering::VulkanError::M2BoneTransformRange)?;
            bones.extend_from_slice(pose_scratch.transforms());
            if let Some(mesh) = source.mesh {
                for (draw_index, resources) in source.draws.iter().enumerate() {
                    let Some(resources) = resources else {
                        continue;
                    };
                    let pose =
                        M2MaterialPose::sample(&source.model, &source.plan, draw_index, clock)?;
                    let draw = &source.plan.draws()[draw_index];
                    let state = M2MaterialState::from_material(draw.material());
                    let alpha = M2ElementAlphaState::classify(pose.mesh_color().w);
                    if alpha == M2ElementAlphaState::Hidden {
                        continue;
                    }
                    let fade = alpha == M2ElementAlphaState::Translucent && !state.blend_enabled();
                    let pipeline = if fade {
                        resources.runtime_fade_pipeline.ok_or_else(|| {
                            RuntimeTerrainFrameError::M2RuntimeFadePipeline {
                                model: source.model.path().clone(),
                                draw_index,
                            }
                        })?
                    } else {
                        resources.pipeline
                    };
                    let material = M2MaterialUniform::new(
                        transform,
                        pose.texture_transforms(),
                        model_view,
                        pose.mesh_color(),
                        Vec4::ZERO,
                        // Portraits disable world fog for every material.
                        Vec4::new(state.alpha_reference(1.0), 0.0, 0.0, 0.0),
                    );
                    let prepared = renderer.prepare_m2_draw(
                        mesh,
                        pipeline,
                        resources.texture_set,
                        &source.plan,
                        draw_index,
                        fade,
                        material,
                        bone_offset,
                        0,
                    )?;
                    if draw.transparent_sort_unit() || alpha == M2ElementAlphaState::Translucent {
                        let section_distance =
                            section_distance_key(draw, &pose_scratch, model_view)?;
                        let distance = if model_distance_sort {
                            m2_model_distance_key(model_view)
                        } else {
                            section_distance
                        };
                        let key = M2TransparentSortKey::new(
                            distance,
                            false,
                            i16::from(draw.batch().priority_plane),
                            section_distance,
                            index,
                            draw.batch().material_layer,
                        )
                        .with_scene_element(0, transparent.len() as u32);
                        transparent.push((key, prepared));
                    } else {
                        draws.push(prepared);
                    }
                }
            }
            poses.push(PortraitPose {
                owner: placement.owner,
                source,
                clock,
                transform,
                distance_sort: model_distance_sort,
                pose: std::mem::take(&mut pose_scratch),
            });
        }
        transparent.sort_by(|left, right| compare_m2_transparent(&left.0, &right.0));
        draws.extend(transparent.into_iter().map(|(_, draw)| draw));
        if draws.is_empty() {
            return Ok(false);
        }
        // 0x00616BC0 supplies ambient .45 and one white (-1, 0, -1) D3D ray.
        let scene = M2SceneUniform::new(
            camera.view_projection(),
            camera.view(),
            camera.camera().position(),
            Vec3::splat(0.45),
            Vec3::ONE,
            Vec3::new(-1.0, 0.0, -1.0),
            Vec4::ZERO,
            Vec3::ZERO,
            [solarity_rendering::M2LocalLightState::disabled(); 4],
        );
        renderer.render_unit_portrait("player", scene, &bones, &draws, mask)?;
        Ok(true)
    }
}

struct PortraitPose<'a> {
    owner: M2GpuPlacementOwner,
    source: &'a M2GpuSource,
    clock: M2AnimationClock,
    transform: Mat4,
    distance_sort: bool,
    pose: M2BonePose,
}

fn attachment_transform(
    poses: &[PortraitPose<'_>],
    owner: M2GpuPlacementOwner,
    id: u32,
) -> Result<Option<(Mat4, bool)>, RuntimeTerrainFrameError> {
    let Some(parent) = poses.iter().find(|pose| pose.owner == owner) else {
        return Ok(None);
    };
    let Some(attachment) = parent.source.model.attachment(id) else {
        return Ok(None);
    };
    parent
        .pose
        .attachment_transform(
            parent.source.model.animations(),
            attachment,
            parent.clock,
            parent.transform,
        )
        .map(|transform| transform.map(|transform| (transform, parent.distance_sort)))
        .map_err(RuntimeTerrainFrameError::from)
}

fn portrait_clock(model: &DecodedM2Model, catalog: &AnimationDataCatalog) -> M2AnimationClock {
    let animations = model.animations();
    let sequence = animations
        .resolve_model_animation(catalog, 0)
        .and_then(|resolved| animations.model_sequence_for_variation(resolved.animation_id(), 0))
        .unwrap_or(0);
    M2AnimationClock::new(sequence, 0.0, 0.0)
}

fn portrait_camera(
    model: &DecodedM2Model,
    clock: M2AnimationClock,
) -> Result<WorldCameraFrame, RuntimeTerrainFrameError> {
    let animations = model.animations();
    if let Some(Some(index)) = animations.camera_lookup().first() {
        return sample_m2_camera_frame(animations, usize::from(*index), clock, 1.0, Mat4::IDENTITY)
            .map_err(RuntimeTerrainFrameError::from);
    }
    // 0x0082CED0 returns the selected sequence's center and radius. The
    // portrait fallback places the eye 1.7 radii along +X from that center.
    let (minimum, maximum, radius) = animations
        .sequences()
        .get(clock.sequence())
        .map(|sequence| sequence.bounds())
        .unwrap_or((Vec3::ZERO, Vec3::ZERO, 0.0));
    let target = (minimum + maximum) * 0.5;
    WorldCamera::new(
        target + Vec3::X * (radius * 1.7),
        target,
        Vec3::Z,
        1.57 / 2.0_f32.sqrt(),
        0.027_777_778,
        5_000.0,
    )
    .frame(1.0)
    .map_err(RuntimeTerrainFrameError::from)
}
