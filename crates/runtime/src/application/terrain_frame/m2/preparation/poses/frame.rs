//! Collects callback-selected unit inputs and joins bounded skeletal work.

use super::super::super::{M2Frame, RuntimeTerrainFrameError};
use super::input::PoseJob;
use solarity_cpu::CpuExecutor;

impl M2Frame {
    /// Unit callbacks have selected clocks and ground transforms. Sampling these
    /// immutable inputs consumes no RNG, callbacks, attachment state or GPU state.
    pub(in crate::application::terrain_frame::m2) fn prepare_unit_poses(
        &mut self,
        cpu: Option<&CpuExecutor>,
        admission: super::PoseAdmission<'_>,
        now: u32,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.pose_batch.finish()?;
        let batch = &mut self.pose_batch;
        for job in &batch.jobs {
            batch.indices[job.placement()] = None;
        }
        batch.indices.resize(self.placements.len(), None);
        let mut active = 0;
        for &index in self.placement_visibility.dynamic_indices() {
            let placement = &self.placements[index];
            let Some(animation) = &placement.unit_animation else {
                continue;
            };
            let Some(clock) = animation.prepared_scene_clock() else {
                continue;
            };
            let Some(source) = &self.sources[placement.source_index] else {
                continue;
            };
            // Attached transforms are finalized during ordered traversal. Hidden
            // roots have no render consumers; other roots use the same camera
            // and independent shadow demands as final packet preparation.
            if !placement.placement_valid
                || self.placement_visibility.light_parent(index).is_some()
                || self.vehicle_passengers.hidden(index)
                || placement
                    .entity_opacity
                    .as_ref()
                    .is_some_and(|owner| owner.hidden())
            {
                continue;
            }
            if !admission.allows(source, placement)? {
                continue;
            }
            let Some(playback) = &placement.playback else {
                continue;
            };
            let playback = playback.borrow();
            let hands = placement
                .retirement
                .as_ref()
                .and_then(|retired| retired.finger_hands)
                .or_else(|| {
                    super::super::super::held_item_finger_pose(
                        &self.requested_items,
                        placement.owner,
                    )
                });
            let finger_pose = hands.and_then(|hands| {
                source
                    .model
                    .animations()
                    .sequence_for_variation(15, 0)
                    .map(|sequence| {
                        (
                            solarity_rendering::M2AnimationClock::new_with_global_tick(
                                sequence,
                                0.,
                                playback.global_tick(now),
                            ),
                            hands,
                        )
                    })
            });
            if active == batch.jobs.len() {
                batch
                    .jobs
                    .push(PoseJob::new(std::sync::Arc::clone(&source.model)));
            }
            batch.jobs[active].prepare(
                index,
                source,
                clock,
                admission.view * placement.transform,
                finger_pose,
                animation.body_pose().bone_transforms(),
                playback.bone_sequence_clocks(&source.model, clock, now),
            );
            batch.indices[index] = Some(active);
            active += 1;
        }
        // Removed owners release their model generation and palette immediately.
        batch.jobs.truncate(active);
        // Sampling owns its inputs. Main can continue WMO admission and ordered
        // traversal; a palette consumer waits for only its own model result.
        if let Some(cpu) = cpu {
            batch.pending.start_graph(
                cpu,
                &solarity_cpu::FrameGraphTemplate::independent(active)
                    .with_priority(solarity_cpu::FramePriority::Prerequisite),
                &mut batch.jobs,
                &[],
            )?;
            batch.handles.clear();
            for index in 0..active {
                batch.handles.push(batch.pending.job(index)?);
            }
            batch.submitted = true;
        } else {
            for job in &mut batch.jobs {
                job.sample();
            }
        }
        Ok(())
    }
}
