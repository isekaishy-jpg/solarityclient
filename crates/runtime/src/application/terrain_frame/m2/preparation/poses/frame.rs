//! Collects callback-selected unit inputs and joins bounded skeletal work.

use super::super::super::{M2Frame, RuntimeTerrainFrameError};
use super::input::PoseJob;
use solarity_asset::ResourceLease;
use solarity_cpu::CpuExecutor;

impl M2Frame {
    /// Unit callbacks have selected clocks and ground transforms. Sampling these
    /// immutable inputs consumes no RNG, callbacks, attachment state or GPU state.
    pub(in crate::application::terrain_frame::m2) fn prepare_unit_poses(
        &mut self,
        cpu: Option<&CpuExecutor>,
        admission: super::PoseAdmission<'_>,
        world_lighting: bool,
        now: u32,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.pose_batch
            .finish(&mut crate::application::frame_pipeline::FrameWait::Offline)?;
        let batch = &mut self.pose_batch;
        for job in &batch.jobs {
            batch.indices[job.placement()] = None;
        }
        batch.indices.resize(self.placements.len(), None);
        let mut active = 0;
        for &index in self.placement_visibility.dynamic_indices() {
            let placement = &self.placements[index];
            let animation = placement.unit_animation.as_ref();
            let Some(clock) = animation
                .and_then(|animation| animation.prepared_scene_clock())
                .or_else(|| {
                    placement
                        .passenger_playback_advance
                        .as_ref()
                        .map(|advance| advance.clock)
                })
            else {
                continue;
            };
            let Some(source) = &self.sources[placement.source_index] else {
                continue;
            };
            // Attached transforms remain with ordered traversal. Independent roots
            // supply render palettes or only the bones their CPU consumers request.
            if !placement.placement_valid
                || self.placement_visibility.light_parent(index).is_some()
                || self.vehicle_passengers.hidden(index)
            {
                continue;
            }
            let palette = !placement
                .entity_opacity
                .as_ref()
                .is_some_and(|owner| owner.hidden())
                && admission.allows(source, placement)?;
            let Some(playback) = &placement.playback else {
                continue;
            };
            let playback = playback.borrow();
            if !palette {
                // The independent offline reference samples these bones during
                // its serial traversal instead of using the worker extension.
                if cpu.is_none() {
                    continue;
                }
                let window = animation
                    .and_then(|animation| animation.prepared_scene_event_window())
                    .unwrap_or_else(|| playback.sample_event_window(now as f32));
                self.bone_demand
                    .model(super::super::demand::CpuModelInputs {
                        placement,
                        model: &source.model,
                        window,
                        items: &self.requested_items,
                        visuals: &self.requested_visuals,
                        glue_ids: &self.glue_attachment_ids,
                        effects: &self.unit_effects,
                        publishes_lights: world_lighting
                            && self.placement_visibility.has_lights(index),
                    });
                if self.bone_demand.bones().is_empty() {
                    continue;
                }
            }
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
                    .push(PoseJob::new(ResourceLease::clone(&source.model)));
            }
            let body_pose = animation.map(|animation| animation.body_pose());
            batch.jobs[active].prepare(
                index,
                source,
                clock,
                admission.view * placement.transform,
                finger_pose,
                body_pose
                    .as_ref()
                    .map_or(&[], |pose| pose.bone_transforms()),
                playback.bone_sequence_clocks(&source.model, clock, now),
            );
            if !palette {
                batch.jobs[active].request_samples(
                    self.bone_demand.bones(),
                    cpu.unwrap_or_else(|| unreachable!("named requests require an executor"))
                        .storage(),
                )?;
            }
            batch.jobs[active].measurement = batch.calibration.prepare(if palette {
                source.model.animations().bones().len()
            } else {
                self.bone_demand.bones().len()
            });
            batch.indices[index] = Some(active);
            active += 1;
        }
        // Removed owners release their model generation and palette immediately.
        batch.jobs.truncate(active);
        // Sampling owns its inputs. Main can continue WMO admission and ordered
        // traversal; a palette consumer waits for only its own model result.
        if let Some(cpu) = cpu {
            for job in &mut batch.jobs {
                job.admit(cpu)?;
            }
            batch.costs.clear();
            batch.costs.reserve(
                cpu.storage(),
                solarity_cpu::CpuStorageClass::Frame,
                solarity_cpu::CpuStorageKind::Metadata,
                active,
            )?;
            for job in &batch.jobs {
                batch.costs.push(job.measurement.cost())?;
            }
            batch.pending.start_costed_graph(
                cpu,
                &solarity_cpu::FrameGraphTemplate::independent(active)
                    .with_priority(solarity_cpu::FramePriority::Prerequisite),
                &mut batch.jobs,
                &[],
                &batch.costs,
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
