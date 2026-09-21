//! One local appearance request owns loading while movement remains on main.

use super::super::population_worker::{PopulationLoading, PopulationRequest};
use super::super::{
    PlayerAppearanceInputs, RuntimePlayerError, RuntimePlayerPoll, RuntimePlayerPresentation,
};
use solarity_cpu::CpuExecutor;
use solarity_ecs::ActiveWorld;
use solarity_rendering::VulkanRenderer;

impl RuntimePlayerPresentation {
    /// Synchronous asset consumers explicitly own loading; live frames use the executor.
    /// # Errors
    /// Preserves authoritative field, table, asset and camera failures.
    pub fn synchronize(
        &mut self,
        world: Option<&ActiveWorld>,
    ) -> Result<RuntimePlayerPoll, RuntimePlayerError> {
        self.synchronize_local_inner(world, PopulationLoading::Synchronous)
    }

    /// Shares the primary M2 request and constructs the complete appearance on a worker.
    pub(in crate::application) fn synchronize_local_async(
        &mut self,
        world: Option<&ActiveWorld>,
        cpu: &CpuExecutor,
        renderer: &mut VulkanRenderer,
    ) -> Result<RuntimePlayerPoll, RuntimePlayerError> {
        let _profile = solarity_profiling::profile!("player.local.synchronize");
        self.local_worker.service(world, cpu)?;
        self.synchronize_local_inner(world, PopulationLoading::Asynchronous { cpu, renderer })
    }

    fn synchronize_local_inner(
        &mut self,
        world: Option<&ActiveWorld>,
        mut loading: PopulationLoading<'_>,
    ) -> Result<RuntimePlayerPoll, RuntimePlayerError> {
        let Some(world) = world else {
            self.remove_local_resident(loading.is_asynchronous());
            self.unit_animations.clear();
            self.textures.collect_unused();
            return Ok(RuntimePlayerPoll::Idle);
        };
        self.unit_animations.synchronize_passenger_inputs(
            world,
            &self.vehicles,
            &self.passenger_frames,
        );
        let guid = world.local_player_guid()?;
        let Some(identity) = world.object_identity(guid) else {
            self.remove_local_resident(loading.is_asynchronous());
            return Ok(RuntimePlayerPoll::Pending);
        };
        if self
            .resident
            .as_ref()
            .is_some_and(|resident| resident.identity != identity)
        {
            self.remove_local_resident(loading.is_asynchronous());
        }
        let inputs = PlayerAppearanceInputs::read(world, guid);
        if inputs.is_some()
            && self
                .resident
                .as_ref()
                .is_some_and(|resident| resident.appearance_inputs == inputs)
        {
            self.local_worker.discard_ready(identity)?;
            return self.update_local_pose(world);
        }
        if let (Some(inputs), PopulationLoading::Asynchronous { renderer, .. }) =
            (inputs, &mut loading)
            && self
                .local_worker
                .matches_request(identity, &inputs, self.component_texture_level)
        {
            if let Some(resident) =
                self.local_worker
                    .take_ready(&inputs, self.component_texture_level, renderer)?
            {
                return self.publish_local(world, resident, true);
            }
            return self.retain_local_pose(world);
        }
        let Some(appearance) = self.resolve_local_appearance(world)? else {
            self.local_worker.discard_ready(identity)?;
            return Ok(RuntimePlayerPoll::Pending);
        };
        self.local_dimensions = Some((
            appearance.desired.collision_extent,
            appearance.desired.object_scale,
        ));
        if self
            .resident
            .as_ref()
            .is_some_and(|resident| resident.matches_appearance(&appearance.desired))
        {
            self.local_worker.discard_ready(identity)?;
            if let Some(resident) = &mut self.resident {
                resident.appearance_inputs = inputs;
            }
            return self.update_local_pose(world);
        }
        // A valid player construction needs every projection in the immutable
        // snapshot. Missing input retains the existing Pending result.
        let Some(inputs) = inputs else {
            return Ok(RuntimePlayerPoll::Pending);
        };
        if let Some(resident) = self.take_local_glue(&appearance, inputs)? {
            self.local_worker.discard_ready(identity)?;
            return self.publish_local(world, resident, loading.is_asynchronous());
        }
        let desired = appearance.desired;
        if let PopulationLoading::Asynchronous { cpu, renderer } = &mut loading {
            // This also withdraws a different pending key before a new request
            // can claim the exclusive archive/cache bank.
            if let Some(resident) =
                self.local_worker
                    .take_ready(&inputs, self.component_texture_level, renderer)?
            {
                return self.publish_local(world, resident, true);
            }
            let catalog = self
                .glue_worker_catalog
                .as_ref()
                .ok_or(RuntimePlayerError::MissingPopulationWorkerCatalog)?
                .clone();
            let catalogs = self.shared_catalogs();
            self.local_worker.submit(
                cpu,
                PopulationRequest {
                    identity,
                    key: inputs,
                    level: self.component_texture_level,
                    model_path: desired.path.clone(),
                    sources: crate::application::player_coordinator::worker_presentation::AppearanceSources::Player {
                        attachments: desired.attachment_plan.clone(),
                        mount: desired.mount_key.as_ref().map(|key| key.path.clone()),
                    },
                },
                catalog,
                catalogs,
                move |presentation, model| presentation.prepare_player(desired.clone(), inputs, model),
            )?;
            self.retain_local_pose(world)
        } else {
            let model = self
                .models
                .load(&mut self.assets.borrow_mut(), &desired.path)?;
            let resident = self.prepare_player(desired, inputs, model)?;
            self.publish_local(world, resident, false)
        }
    }

    /// Existing residency owns its live clocks until an exact replacement publishes.
    fn retain_local_pose(
        &mut self,
        world: &ActiveWorld,
    ) -> Result<RuntimePlayerPoll, RuntimePlayerError> {
        if self.resident.is_some() {
            self.update_local_pose(world)
        } else {
            Ok(RuntimePlayerPoll::Pending)
        }
    }

    fn remove_local_resident(&mut self, asynchronous: bool) {
        self.local_dimensions = None;
        if asynchronous {
            self.local_worker.retire(self.resident.take());
        } else {
            self.resident = None;
        }
    }
}
