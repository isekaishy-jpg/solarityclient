//! Shared visible-GameObject generations and loading-card transport readiness.

mod passenger;
mod transport;
mod transport_animation;
mod transport_model;
mod worker;
mod world_model;

#[cfg(test)]
#[path = "../../tests/application/game_object_jobs.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/application/game_object_transports.rs"]
pub(in crate::application) mod transport_tests;

use crate::application::liquid::LiquidAssetCache;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Weak};

use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetPath, AssetStoreHandle, BlpTextureCache,
    GameObjectDisplayCatalog, M2ModelCache, TransportCatalog, WmoModelCache, canonical_model_path,
};
use solarity_cpu::{CpuError, CpuExecutor, CpuTask};
use solarity_ecs::{
    ActiveWorld, GameObjectPresentation, ObjectKind, WorldObjectIdentity, WorldTransform,
};
use solarity_systems::{
    GameObjectPlacement, GameObjectPlacementError, GameObjectPlacementResolver,
    MovementTransportFrame,
};
use thiserror::Error;

use crate::application::game_object_behavior::{GameObjectBehavior, GameObjectNotification};
use crate::application::gameplay_coordinator::{
    GameObjectTemplateBinding, GameObjectTemplateCache,
};
use crate::application::terrain_coordinator::RuntimeTerrainError;
use crate::application::terrain_coordinator::m2_residency::ResidentM2Source;
use crate::application::terrain_frame::RuntimeTerrainFrameError;
use crate::random::CrtRand;
use transport::GameObjectTransportBehavior;
use worker::{
    GameObjectWorkerCompletion, GameObjectWorkerSource, GameObjectWorkerState, prepare_on_worker,
};
pub(in crate::application) use world_model::{
    GameObjectWorldModelSource, GameObjectWorldModelState,
};

/// Failure while admitting the exact display resource owned by a GameObject.
#[derive(Debug, Error)]
pub enum RuntimeGameObjectError {
    /// Authored type-11 keys cannot supply the requested native interval.
    #[error(transparent)]
    TransportAnimation(#[from] solarity_systems::TransportAnimationError),
    /// A native behavior callback rejected model or collision animation input.
    #[error(transparent)]
    AnimationFrame(#[from] RuntimeTerrainFrameError),
    /// Type-11 construction must use the creation packet's exact field image.
    #[error("transport {guid:#018x} has no complete creation-time animation state")]
    MissingTransportCreation {
        /// Object whose early creation callback was absent or incomplete.
        guid: u64,
    },
    /// Native creation cannot substitute empty tracks for an unavailable catalog.
    #[error("transport animation catalog is unavailable")]
    MissingTransportCatalog,
    /// A template route or its exact DBC controls cannot be admitted.
    #[error(transparent)]
    TransportRoute(#[from] solarity_systems::TransportRouteError),
    /// The referenced transport physics row violates its finite input contract.
    #[error(transparent)]
    TransportPhysics(#[from] solarity_systems::TransportRoutePhysicsError),
    /// A sampled route cannot form a valid native object transform.
    #[error(transparent)]
    TransportPose(#[from] GameObjectPlacementError),
    /// The sampled object lifetime could not publish its animated pose.
    #[error(transparent)]
    World(#[from] solarity_ecs::WorldStateError),
    /// The bounded CPU executor rejected or lost GameObject preparation work.
    #[error(transparent)]
    Cpu(#[from] CpuError),
    /// M2, WMO, or authored texture residency failed strict decoding.
    #[error(transparent)]
    Resource(#[from] RuntimeTerrainError),
    /// Production synchronization lacks its independently mounted archive stack.
    #[error("GameObject CPU worker archive catalog is unavailable")]
    MissingWorkerCatalog,
}

/// Resource family selected by one `GameObjectDisplayInfo.dbc` row.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeGameObjectResourceKind {
    /// An M2, MDX, or MDL generation with its primary SKIN profile.
    M2,
    /// A root WMO with every numbered group.
    WorldModel,
}

/// Observable result of synchronizing the controlled player's transport parent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeTransportPoll {
    /// No active world or transport relationship exists.
    Idle,
    /// The movement relationship names an object that has not entered the world.
    AwaitingObject {
        /// Exact server GUID named by local-player movement.
        guid: u64,
    },
    /// The admitted object owns no supported display resource.
    NoResource {
        /// Exact admitted GameObject GUID.
        guid: u64,
    },
    /// The exact display generation is being prepared by the CPU worker pool.
    Pending {
        /// Exact admitted GameObject GUID.
        guid: u64,
        /// Resource family selected by the display row.
        kind: RuntimeGameObjectResourceKind,
    },
    /// A new M2 or WMO generation and its texture inputs became resident.
    ResourceLoaded {
        /// Exact admitted GameObject GUID.
        guid: u64,
        /// Resource family selected by the display row.
        kind: RuntimeGameObjectResourceKind,
    },
    /// The previously admitted resource generation remains current.
    Current {
        /// Exact admitted GameObject GUID.
        guid: u64,
        /// Resource family selected by the display row.
        kind: RuntimeGameObjectResourceKind,
    },
    /// The retained resource's placement became available or unavailable.
    PlacementChanged {
        /// Exact admitted GameObject GUID.
        guid: u64,
        /// Resource family retained without another asset decode request.
        kind: RuntimeGameObjectResourceKind,
    },
}

/// Immutable CPU preparation shared independently of GUID and object lifetime.
pub(in crate::application) enum GameObjectResource {
    M2(ResidentM2Source),
    WorldModel(GameObjectWorldModelSource),
}

impl GameObjectResource {
    const fn kind(&self) -> RuntimeGameObjectResourceKind {
        match self {
            Self::M2(_) => RuntimeGameObjectResourceKind::M2,
            Self::WorldModel(_) => RuntimeGameObjectResourceKind::WorldModel,
        }
    }

    fn path(&self) -> &AssetPath {
        match self {
            Self::M2(source) => source.model().path(),
            Self::WorldModel(source) => source.model().path(),
        }
    }
}

/// Current inputs and optional resource for one admitted GameObject lifetime.
pub(in crate::application) struct GameObjectInstance {
    entry: u32,
    template: Option<GameObjectTemplateBinding>,
    identity: WorldObjectIdentity,
    presentation: GameObjectPresentation,
    transform: Option<WorldTransform>,
    scale: Option<f32>,
    placement: Result<GameObjectPlacement, GameObjectPlacementError>,
    /// GO+1A8, separate from the map handle's initial scale-one/yaw-only pose.
    passenger_placement: Result<GameObjectPlacement, GameObjectPlacementError>,
    request: Option<ResourceRequest>,
    resource: Option<Arc<GameObjectResource>>,
    failed: bool,
    behavior: Option<Rc<GameObjectBehavior>>,
    transport: Option<Rc<GameObjectTransportBehavior>>,
    world_model_state: RefCell<Option<Rc<GameObjectWorldModelState>>>,
}

/// Borrowed object order and lifetime lookup for one renderer publication/update.
#[derive(Clone, Copy)]
pub(in crate::application) struct GameObjectFrameInput<'a> {
    animations: &'a Arc<AnimationDataCatalog>,
    instances: &'a [GameObjectInstance],
    indices: &'a HashMap<WorldObjectIdentity, usize>,
    world: Option<&'a ActiveWorld>,
    scene_time_ms: &'a Cell<u32>,
}

impl<'a> GameObjectFrameInput<'a> {
    pub(in crate::application) fn advance_scene(
        self,
        scene_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.scene_time_ms.set(scene_time_ms as u32);
        if let Some(world) = self.world {
            for instance in self.instances {
                if let Some(behavior) = instance.behavior() {
                    behavior.advance_scene(world, scene_time_ms, random)?;
                }
                if let Some(model) = instance.transport_model() {
                    model.advance_scene(scene_time_ms, random)?;
                }
            }
        }
        Ok(())
    }
    pub(in crate::application) const fn animations(self) -> &'a Arc<AnimationDataCatalog> {
        self.animations
    }
    pub(in crate::application) const fn instances(self) -> &'a [GameObjectInstance] {
        self.instances
    }
    pub(in crate::application) fn get(
        self,
        identity: WorldObjectIdentity,
    ) -> Option<&'a GameObjectInstance> {
        self.indices
            .get(&identity)
            .and_then(|index| self.instances.get(*index))
    }
}

impl GameObjectInstance {
    pub(in crate::application) fn transport_model(
        &self,
    ) -> Option<&transport_model::TransportMapModel> {
        self.transport.as_ref().map(|transport| &transport.model)
    }
    pub(in crate::application) fn world_model_state(
        &self,
    ) -> Option<Rc<GameObjectWorldModelState>> {
        self.world_model_state.borrow().clone()
    }
    pub(in crate::application) fn behavior(&self) -> Option<&GameObjectBehavior> {
        self.behavior.as_deref()
    }
    pub(in crate::application) const fn identity(&self) -> WorldObjectIdentity {
        self.identity
    }
    pub(in crate::application) const fn guid(&self) -> u64 {
        self.identity.guid()
    }
    pub(in crate::application) const fn display_id(&self) -> u32 {
        self.presentation.display_id()
    }
    pub(in crate::application) const fn state(&self) -> u8 {
        self.presentation.state()
    }
    pub(in crate::application) fn placement(&self) -> Option<GameObjectPlacement> {
        self.placement.ok()
    }
    /// Transport map handles exist only after template admission (783500).
    /// CPU resource completion alone cannot register a WMO collision root.
    pub(in crate::application) fn map_placement(&self) -> Option<GameObjectPlacement> {
        match &self.transport {
            Some(transport) => transport.map_placement(),
            None => self.placement(),
        }
    }
    pub(in crate::application) fn resource(&self) -> Option<&GameObjectResource> {
        self.resource.as_deref()
    }
}

/// Shares model preparation across visible GameObjects while retaining each lifetime.
pub struct RuntimeGameObjectPresentation {
    animations: Arc<AnimationDataCatalog>,
    assets: AssetStoreHandle,
    displays: GameObjectDisplayCatalog,
    textures: BlpTextureCache,
    models: M2ModelCache,
    world_models: WmoModelCache,
    liquid_assets: LiquidAssetCache,
    instances: Vec<GameObjectInstance>,
    indices: HashMap<WorldObjectIdentity, usize>,
    resources: HashMap<ResourceRequest, Arc<GameObjectResource>>,
    placement_resolver: GameObjectPlacementResolver,
    world_identity: Option<WorldObjectIdentity>,
    transport_guid: Option<u64>,
    transport_identity: Option<WorldObjectIdentity>,
    last_transport: Option<TransportAdmission>,
    readiness: bool,
    scene_revision: u64,
    worker_catalog: Option<ArchiveCatalog>,
    worker: Option<Box<GameObjectWorkerState>>,
    pending: Option<PendingGeneration>,
    behaviors: HashMap<WorldObjectIdentity, Rc<GameObjectBehavior>>,
    transport_behaviors: HashMap<WorldObjectIdentity, Rc<GameObjectTransportBehavior>>,
    transport_catalog: Option<TransportCatalog>,
    passenger_transports: HashSet<WorldObjectIdentity>,
    game_object_passenger_transports: HashSet<WorldObjectIdentity>,
    scene_time_ms: Cell<u32>,
}

impl RuntimeGameObjectPresentation {
    pub(in crate::application) fn movement_instances(&self) -> &[GameObjectInstance] {
        &self.instances
    }

    pub(in crate::application) fn movement_instance(
        &self,
        identity: WorldObjectIdentity,
    ) -> Option<&GameObjectInstance> {
        self.indices
            .get(&identity)
            .and_then(|&index| self.instances.get(index))
    }
    /// Creates a shared object owner over the process archive stack.
    #[must_use]
    pub fn new(
        assets: AssetStoreHandle,
        displays: GameObjectDisplayCatalog,
        animations: Arc<AnimationDataCatalog>,
    ) -> Self {
        Self {
            animations,
            assets,
            displays,
            textures: BlpTextureCache::new(),
            models: M2ModelCache::new(),
            world_models: WmoModelCache::new(),
            liquid_assets: LiquidAssetCache::default(),
            instances: Vec::new(),
            indices: HashMap::new(),
            resources: HashMap::new(),
            placement_resolver: GameObjectPlacementResolver::default(),
            world_identity: None,
            transport_guid: None,
            transport_identity: None,
            last_transport: None,
            readiness: true,
            scene_revision: 0,
            worker_catalog: None,
            worker: None,
            pending: None,
            behaviors: HashMap::new(),
            transport_behaviors: HashMap::new(),
            transport_catalog: None,
            passenger_transports: HashSet::new(),
            game_object_passenger_transports: HashSet::new(),
            scene_time_ms: Cell::new(0),
        }
    }

    /// Supplies the separately mounted archive stack used by the bounded CPU worker.
    #[must_use]
    pub fn with_worker_catalog(mut self, catalog: ArchiveCatalog) -> Self {
        self.worker_catalog = Some(catalog);
        self
    }

    /// Supplies the native DBC route and motion tables for transport behaviors.
    #[must_use]
    pub fn with_transport_catalog(mut self, catalog: TransportCatalog) -> Self {
        self.transport_catalog = Some(catalog);
        self
    }

    /// Refreshes all object lifetimes and polls one shared resource preparation job.
    /// The local player's transport is first among requests not already running.
    /// The returned poll describes only that loading-card dependency.
    ///
    /// # Errors
    /// Returns resource/worker errors once for each failed admitted request.
    pub fn synchronize_async(
        &mut self,
        world: Option<&ActiveWorld>,
        cpu: &CpuExecutor,
    ) -> Result<RuntimeTransportPoll, RuntimeGameObjectError> {
        self.refresh_instances(world)?;
        self.finish_pending()?;
        if self.pending.is_none()
            && let Some(request) = self.next_request()
        {
            let transport_request = self
                .transport_identity
                .and_then(|identity| self.indices.get(&identity))
                .and_then(|index| self.instances[*index].request.as_ref());
            if transport_request != Some(&request) && !cpu.can_admit_speculative()? {
                return Ok(self.poll_transport());
            }
            let source = if let Some(worker) = self.worker.take() {
                GameObjectWorkerSource::Ready(worker)
            } else {
                GameObjectWorkerSource::Catalog(
                    self.worker_catalog
                        .as_ref()
                        .ok_or(RuntimeGameObjectError::MissingWorkerCatalog)?
                        .clone(),
                )
            };
            let task_request = request.clone();
            let task = cpu.try_submit(move || prepare_on_worker(source, &task_request))?;
            self.pending = Some(PendingGeneration {
                request,
                eligible: true,
                task,
            });
        }
        Ok(self.poll_transport())
    }

    /// Prepares the same shared generations synchronously for controlled callers.
    ///
    /// # Errors
    /// Returns strict archive/model/texture preparation failures.
    pub fn synchronize(
        &mut self,
        world: Option<&ActiveWorld>,
    ) -> Result<RuntimeTransportPoll, RuntimeGameObjectError> {
        self.refresh_instances(world)?;
        self.finish_pending()?;
        while let Some(request) = self.next_request() {
            let result = match request.kind {
                RuntimeGameObjectResourceKind::M2 => ResidentM2Source::load(
                    &request.path,
                    &mut self.models,
                    &mut self.textures,
                    &mut self.assets.borrow_mut(),
                )
                .map(GameObjectResource::M2),
                RuntimeGameObjectResourceKind::WorldModel => GameObjectWorldModelSource::load(
                    &request.path,
                    &mut self.world_models,
                    &mut self.models,
                    &mut self.textures,
                    &mut self.liquid_assets,
                    &mut self.assets.borrow_mut(),
                )
                .map(GameObjectResource::WorldModel),
            };
            self.publish(&request, result.map_err(RuntimeGameObjectError::from))?;
        }
        self.collect_unused();
        Ok(self.poll_transport())
    }

    fn refresh_instances(
        &mut self,
        world: Option<&ActiveWorld>,
    ) -> Result<(), RuntimeGameObjectError> {
        self.admit_world(world);
        let Some(world) = world else {
            return Ok(());
        };
        self.behaviors
            .retain(|identity, _| world.object_identity(identity.guid()) == Some(*identity));
        self.transport_behaviors
            .retain(|identity, _| world.object_identity(identity.guid()) == Some(*identity));
        self.transport_guid = world.local_player_transport_guid();
        self.transport_identity = self
            .transport_guid
            .and_then(|guid| world.object_identity(guid));

        let previous_revision = self.scene_revision;
        let before = self.instances.len();
        self.instances.retain(|instance| {
            world.object_identity(instance.guid()) == Some(instance.identity)
                && world.object_kind(instance.guid()) == Some(ObjectKind::GameObject)
        });
        if self.instances.len() != before {
            self.indices.clear();
            self.indices.extend(
                self.instances
                    .iter()
                    .enumerate()
                    .map(|(index, instance)| (instance.identity, index)),
            );
            self.scene_revision = self.scene_revision.wrapping_add(1);
        }

        for identity in world.visible_game_objects() {
            let presentation = world
                .game_object_presentation(identity.guid())
                .unwrap_or_default();
            let behavior = self.behavior_for(identity, presentation);
            let transport = self.transport_for(identity, presentation);
            let index = self.indices.get(&identity).copied();
            let display_changed = index.is_none_or(|index| {
                self.instances[index].display_id() != presentation.display_id()
            });
            let request = if display_changed {
                self.request_for(presentation.display_id())?
            } else {
                None
            };
            let placement = self.placement_resolver.resolve(world, identity.guid());
            let transform = world
                .object_transform(identity.guid())
                .filter(valid_transform);
            let object = world.object_presentation(identity.guid());
            let entry = object.map_or(0, solarity_ecs::ObjectPresentation::entry_id);
            let scale = object
                .map(solarity_ecs::ObjectPresentation::scale)
                .filter(|scale| scale.is_finite() && *scale > 0.0);
            if let Some(index) = index {
                let instance = &mut self.instances[index];
                if display_changed {
                    instance.world_model_state.borrow_mut().take();
                    if let Some(behavior) = instance.behavior() {
                        behavior.detach_model();
                    }
                    if let Some(transport) = &instance.transport {
                        transport.detach_model();
                    }
                    instance.resource = request
                        .as_ref()
                        .and_then(|request| self.resources.get(request))
                        .cloned();
                    instance.request = request;
                    instance.failed = false;
                    self.scene_revision = self.scene_revision.wrapping_add(1);
                }
                if instance.placement.is_ok() != placement.is_ok() {
                    self.scene_revision = self.scene_revision.wrapping_add(1);
                }
                instance.presentation = presentation;
                instance.entry = entry;
                instance.placement = placement;
                instance.passenger_placement = placement;
                instance.transform = transform;
                instance.scale = scale;
                instance.behavior = behavior;
                instance.transport = transport;
            } else {
                let resource = request
                    .as_ref()
                    .and_then(|request| self.resources.get(request))
                    .cloned();
                self.indices.insert(identity, self.instances.len());
                self.instances.push(GameObjectInstance {
                    entry,
                    template: None,
                    identity,
                    presentation,
                    transform,
                    scale,
                    placement,
                    passenger_placement: placement,
                    request,
                    resource,
                    failed: false,
                    behavior,
                    transport,
                    world_model_state: RefCell::new(None),
                });
                self.scene_revision = self.scene_revision.wrapping_add(1);
            }
        }
        if self.scene_revision != previous_revision {
            self.collect_unused();
            self.placement_resolver.retain_world(world);
        }
        Ok(())
    }

    fn admit_world(&mut self, world: Option<&ActiveWorld>) {
        let identity = world.and_then(|world| {
            world
                .local_player_guid()
                .ok()
                .and_then(|guid| world.object_identity(guid))
        });
        if self.world_identity != identity {
            self.disconnect();
            self.world_identity = identity;
        }
    }

    fn behavior_for(
        &mut self,
        identity: WorldObjectIdentity,
        fields: GameObjectPresentation,
    ) -> Option<Rc<GameObjectBehavior>> {
        // 714250's constructors enter 7124B0 only for these generic families.
        // Path transports and specialized destructible/trap clocks have other owners.
        if !matches!(fields.object_type(), 0..=3 | 5..=6 | 8..=10 | 12 | 16..=19 | 22..=27 | 29..=30 | 34)
        {
            return None;
        }
        Some(Rc::clone(self.behaviors.entry(identity).or_insert_with(
            || {
                Rc::new(GameObjectBehavior::new(
                    identity,
                    fields,
                    Arc::clone(&self.animations),
                ))
            },
        )))
    }

    pub(in crate::application) fn observe_notification(
        &mut self,
        world: &mut ActiveWorld,
        identity: WorldObjectIdentity,
        notification: GameObjectNotification,
        receipt_ms: u32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeGameObjectError> {
        self.admit_world(Some(world));
        if let Some(fields) = world.game_object_presentation(identity.guid())
            && world.object_identity(identity.guid()) == Some(identity)
        {
            if let Some(behavior) = self.behavior_for(identity, fields) {
                behavior.notify(world, notification, self.scene_time_ms.get(), random)?;
            }
            if let Some(transport) = self.transport_for(identity, fields) {
                transport.notify(
                    world,
                    fields,
                    notification,
                    self.transport_catalog.as_ref(),
                    receipt_ms,
                    &mut self.placement_resolver,
                )?;
            }
        }
        Ok(())
    }

    /// Types 11 and 15 have separate native motion and map-model owners.
    fn transport_for(
        &mut self,
        identity: WorldObjectIdentity,
        fields: GameObjectPresentation,
    ) -> Option<Rc<GameObjectTransportBehavior>> {
        if !matches!(fields.object_type(), 11 | 15) {
            return None;
        }
        Some(Rc::clone(
            self.transport_behaviors.entry(identity).or_insert_with(|| {
                Rc::new(GameObjectTransportBehavior::new(
                    identity,
                    fields,
                    Arc::clone(&self.animations),
                ))
            }),
        ))
    }

    /// Retains occupied parent lifetimes from the local and remote movement
    /// owners. Unresolved wire GUIDs are not native passenger-list membership.
    pub(in crate::application) fn synchronize_transport_passengers(
        &mut self,
        parents: impl IntoIterator<Item = WorldObjectIdentity>,
    ) {
        self.passenger_transports.clear();
        self.passenger_transports.extend(parents);
    }

    /// Samples native transport motion and refreshes every dependent placement.
    /// Call before collision synchronization and local/remote passenger movement.
    /// `elapsed_ms` is the shared 6F1490 movement-frame delta, not an object's age;
    /// non-positive signed deltas admit new handles without advancing live motion.
    ///
    /// # Errors
    /// Returns invalid route, physics, pose, or ECS publication failures.
    pub fn advance_transports(
        &mut self,
        world: Option<&mut ActiveWorld>,
        client_time_ms: u32,
        elapsed_ms: u32,
    ) -> Result<(), RuntimeGameObjectError> {
        let (Some(world), Some(catalog)) = (world, self.transport_catalog.as_ref()) else {
            return Ok(());
        };
        self.game_object_passenger_transports.clear();
        // 712F30/4F4230 register a GameObject's resolved movement context through
        // 74B340 -> 712EB0 -> 711AB0. It occupies the list without the unit flag
        // used by 760720, so it still enables 7139E0's translation epsilon.
        for instance in &self.instances {
            if let Some(parent) = world
                .game_object_movement(instance.guid())
                .and_then(|movement| movement.transport())
                .filter(|parent| parent.guid != 0)
                && world
                    .game_object_presentation(parent.guid)
                    .is_some_and(|fields| matches!(fields.object_type(), 11 | 15))
                && let Some(identity) = world.object_identity(parent.guid)
                && self
                    .placement_resolver
                    .resolve(world, instance.guid())
                    .is_ok()
            {
                self.game_object_passenger_transports.insert(identity);
            }
        }
        for instance in &self.instances {
            if let Some(transport) = &instance.transport
                && transport.is_animation()
            {
                let has_map_model = instance.resource.is_some()
                    && instance
                        .template
                        .as_ref()
                        .and_then(|binding| binding.template())
                        .is_some();
                if has_map_model && transport.needs_map_placement() {
                    transport.admit_animation_map(
                        self.placement_resolver.resolve(world, instance.guid())?,
                    );
                }
                let raw_time_ms = world
                    .game_object_movement(instance.guid())
                    .ok_or(RuntimeGameObjectError::MissingTransportCreation {
                        guid: instance.guid(),
                    })?
                    .transport_clock_ms(client_time_ms);
                if elapsed_ms as i32 > 0 {
                    transport.advance_animation(
                        world,
                        transport_animation::AnimationFrame {
                            raw_time_ms,
                            elapsed_ms,
                            map_model: if has_map_model {
                                transport_animation::AnimationMapModel::Ready
                            } else {
                                transport_animation::AnimationMapModel::Absent
                            },
                            has_passengers: self.passenger_transports.contains(&instance.identity)
                                || self
                                    .game_object_passenger_transports
                                    .contains(&instance.identity),
                        },
                        &mut self.placement_resolver,
                    )?;
                }
                if has_map_model {
                    transport.set_map_placement(Some(
                        self.placement_resolver.resolve(world, instance.guid())?,
                    ));
                }
                continue;
            }
            if instance.resource.is_some()
                && let Some(transport) = &instance.transport
                && let Some(template) = instance
                    .template
                    .as_ref()
                    .and_then(|binding| binding.template())
            {
                if transport.needs_map_placement() {
                    transport.set_map_placement(Some(
                        self.placement_resolver
                            .resolve_map_model_initial(world, instance.guid())?,
                    ));
                }
                if (elapsed_ms as i32 > 0 || transport.needs_map_placement())
                    && transport.advance(world, catalog, &template, client_time_ms, elapsed_ms)?
                {
                    transport.set_map_placement(Some(
                        self.placement_resolver.resolve(world, instance.guid())?,
                    ));
                }
            }
        }
        // Resolve after all parents have moved, regardless of packet/object order.
        for instance in &mut self.instances {
            let mut placement = self.placement_resolver.resolve(world, instance.guid());
            instance.passenger_placement = placement;
            if let Some(transport) = &instance.transport
                && let Some(current) = transport.map_placement()
            {
                placement = Ok(current);
            }
            if instance.placement.is_ok() != placement.is_ok() {
                self.scene_revision = self.scene_revision.wrapping_add(1);
            }
            instance.placement = placement;
        }
        Ok(())
    }

    /// Attaches admitted CPU models to retained behavior and refreshes collision
    /// placements. Call after resource synchronization and before movement
    /// registration or rendering; unchanged models retain their playback state.
    ///
    /// # Errors
    /// Returns an error for invalid animation inputs or collision placements.
    pub fn synchronize_animations(
        &self,
        world: Option<&ActiveWorld>,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(world) = world else {
            return Ok(());
        };
        for instance in &self.instances {
            if let Some(GameObjectResource::WorldModel(source)) = instance.resource() {
                let mut state = instance.world_model_state.borrow_mut();
                if state
                    .as_ref()
                    .is_none_or(|state| !state.matches(source, instance.display_id()))
                {
                    *state = Some(Rc::new(GameObjectWorldModelState::new(
                        source,
                        &self.animations,
                        instance.display_id(),
                        self.scene_time_ms.get(),
                        random,
                    )?));
                }
            } else {
                instance.world_model_state.borrow_mut().take();
            }
            if let (Some(behavior), Some(GameObjectResource::M2(source))) =
                (instance.behavior(), instance.resource())
            {
                behavior.attach_model(
                    world,
                    instance.display_id(),
                    source.model(),
                    instance.placement(),
                    self.scene_time_ms.get(),
                    random,
                )?;
            }
            if let (Some(transport), Some(GameObjectResource::M2(source))) =
                (&instance.transport, instance.resource())
            {
                transport.attach_model(
                    instance.display_id(),
                    source.model(),
                    self.scene_time_ms.get(),
                    random,
                )?;
                transport
                    .model
                    .synchronize_collision(transport.map_placement(), transport.map_revision())?;
            } else if let Some(transport) = &instance.transport {
                transport.discard_model_sequences();
            }
        }
        Ok(())
    }

    /// Native 0x712F30/0x713130 request the template after model admission.
    /// Keep this registration across display changes, and drop it with the
    /// instance so late network replies cannot target a reused GUID.
    pub(in crate::application) fn synchronize_templates(
        &mut self,
        cache: &mut GameObjectTemplateCache,
    ) {
        for instance in &mut self.instances {
            if instance.resource.is_some() && instance.template.is_none() {
                instance.template = Some(cache.bind(instance.entry, instance.guid()));
            }
        }
    }

    /// Shares the admitted object's server template, once its query completes.
    /// Pending and missing templates return `None` without issuing another query.
    #[must_use]
    pub fn game_object_template(
        &self,
        guid: u64,
    ) -> Option<Rc<solarity_network::GameObjectTemplate>> {
        self.instances
            .iter()
            .find(|instance| instance.guid() == guid)?
            .template
            .as_ref()?
            .template()
    }

    fn request_for(
        &self,
        display_id: u32,
    ) -> Result<Option<ResourceRequest>, RuntimeGameObjectError> {
        let Some(display) = self.displays.display(display_id) else {
            return Ok(None);
        };
        let path = display.asset_path();
        let Some(kind) = resource_kind(path.as_str()) else {
            return Ok(None);
        };
        let path = match kind {
            RuntimeGameObjectResourceKind::M2 => {
                canonical_model_path(path).map_err(RuntimeTerrainError::from)?
            }
            RuntimeGameObjectResourceKind::WorldModel => path.clone(),
        };
        Ok(Some(ResourceRequest { kind, path }))
    }

    fn next_request(&self) -> Option<ResourceRequest> {
        let needs_resource = |instance: &&GameObjectInstance| {
            instance.resource.is_none() && !instance.failed && instance.request.is_some()
        };
        self.transport_identity
            .and_then(|identity| self.indices.get(&identity))
            .and_then(|index| self.instances.get(*index))
            .filter(needs_resource)
            .or_else(|| self.instances.iter().find(needs_resource))
            .and_then(|instance| instance.request.clone())
    }

    fn finish_pending(&mut self) -> Result<(), RuntimeGameObjectError> {
        if !self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.task.is_finished())
        {
            return Ok(());
        }
        let Some(pending) = self.pending.take() else {
            return Ok(());
        };
        let completion = match pending.task.join() {
            Ok(completion) => completion,
            Err(error) if !pending.eligible => {
                tracing::warn!(path = %pending.request.path, %error, "retired GameObject CPU task failed");
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        if let Some(worker) = completion.worker {
            self.worker = Some(worker);
        }
        if pending.eligible {
            self.publish(&pending.request, completion.result)?;
        } else if let Err(error) = completion.result {
            tracing::warn!(path = %pending.request.path, %error, "retired GameObject resource preparation failed");
        }
        Ok(())
    }

    fn publish(
        &mut self,
        request: &ResourceRequest,
        result: Result<GameObjectResource, RuntimeGameObjectError>,
    ) -> Result<(), RuntimeGameObjectError> {
        // Immutable asset work may serve a newly admitted lifetime with the same
        // path. Its transform/state always come from that lifetime's latest inputs.
        if !self
            .instances
            .iter()
            .any(|instance| instance.request.as_ref() == Some(request))
        {
            if let Err(error) = result {
                tracing::warn!(path = %request.path, %error, "unreferenced GameObject preparation failed");
            }
            return Ok(());
        }
        match result {
            Ok(resource) => {
                let resource = Arc::new(resource);
                self.resources
                    .insert(request.clone(), Arc::clone(&resource));
                for instance in &mut self.instances {
                    if instance.request.as_ref() == Some(request) {
                        instance.resource = Some(Arc::clone(&resource));
                        instance.failed = false;
                    }
                }
                self.scene_revision = self.scene_revision.wrapping_add(1);
                Ok(())
            }
            Err(error) => {
                for instance in &mut self.instances {
                    if instance.request.as_ref() == Some(request) {
                        instance.failed = true;
                    }
                }
                Err(error)
            }
        }
    }

    fn poll_transport(&mut self) -> RuntimeTransportPoll {
        let poll = match self.transport_guid {
            None => RuntimeTransportPoll::Idle,
            Some(guid) => match self
                .transport_identity
                .and_then(|identity| self.indices.get(&identity))
                .and_then(|index| self.instances.get(*index))
            {
                None => RuntimeTransportPoll::AwaitingObject { guid },
                Some(instance) => match (&instance.request, &instance.resource) {
                    (None, _) => RuntimeTransportPoll::NoResource { guid },
                    (Some(request), None) => RuntimeTransportPoll::Pending {
                        guid,
                        kind: request.kind,
                    },
                    (_, Some(resource)) => {
                        let kind = resource.kind();
                        let placed = instance.placement.is_ok();
                        let current = self.last_transport.as_ref().is_some_and(|last| {
                            last.identity == instance.identity
                                && last.display_id == instance.display_id()
                                && last.resource.ptr_eq(&Arc::downgrade(resource))
                        });
                        let poll = if !current {
                            RuntimeTransportPoll::ResourceLoaded { guid, kind }
                        } else if self
                            .last_transport
                            .as_ref()
                            .is_some_and(|last| last.placed != placed)
                        {
                            RuntimeTransportPoll::PlacementChanged { guid, kind }
                        } else {
                            RuntimeTransportPoll::Current { guid, kind }
                        };
                        if !current || matches!(poll, RuntimeTransportPoll::PlacementChanged { .. })
                        {
                            self.last_transport = Some(TransportAdmission {
                                identity: instance.identity,
                                display_id: instance.display_id(),
                                resource: Arc::downgrade(resource),
                                placed,
                            });
                        }
                        poll
                    }
                },
            },
        };
        self.readiness = !matches!(
            poll,
            RuntimeTransportPoll::AwaitingObject { .. } | RuntimeTransportPoll::Pending { .. }
        );
        if matches!(
            poll,
            RuntimeTransportPoll::Idle
                | RuntimeTransportPoll::AwaitingObject { .. }
                | RuntimeTransportPoll::NoResource { .. }
                | RuntimeTransportPoll::Pending { .. }
        ) {
            self.last_transport = None;
        }
        poll
    }

    fn collect_unused(&mut self) {
        self.resources
            .retain(|_, resource| Arc::strong_count(resource) > 1);
        self.textures.collect_unused();
        self.models.collect_unused();
        self.world_models.collect_unused();
    }

    pub(in crate::application) fn frame_input<'a>(
        &'a self,
        world: Option<&'a ActiveWorld>,
    ) -> GameObjectFrameInput<'a> {
        GameObjectFrameInput {
            animations: &self.animations,
            instances: &self.instances,
            indices: &self.indices,
            world,
            scene_time_ms: &self.scene_time_ms,
        }
    }

    /// Changes when object/resource admission or placement availability changes.
    #[must_use]
    pub const fn scene_revision(&self) -> u64 {
        self.scene_revision
    }

    /// Returns the live generic behavior state for an exact object lifetime.
    /// Exposes the model completion state needed by native door collision eligibility.
    #[must_use]
    pub fn animation_state(
        &self,
        identity: WorldObjectIdentity,
    ) -> Option<solarity_systems::GameObjectAnimationState> {
        self.behaviors
            .get(&identity)
            .and_then(|behavior| behavior.state())
    }

    /// Returns the retained generic collision flag and native query-mask gate.
    ///
    /// Geometry collection also requires the current model, valid placement,
    /// and spatial registration. This flag alone does not prove residency.
    #[must_use]
    pub fn collision_eligible(
        &self,
        world: &ActiveWorld,
        identity: WorldObjectIdentity,
        query_flags: u32,
    ) -> Option<bool> {
        if world.object_identity(identity.guid()) != Some(identity) {
            return None;
        }
        let fields = world.game_object_presentation(identity.guid())?;
        self.behaviors
            .get(&identity)
            .map(|behavior| behavior.collision_eligible(fields.object_type(), query_flags))
    }

    /// Reports only stock's local-player transport object/resource gate.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.readiness
    }

    fn resident(&self) -> Option<&GameObjectInstance> {
        self.transport_identity
            .and_then(|identity| self.indices.get(&identity))
            .and_then(|index| self.instances.get(*index))
            .filter(|instance| instance.resource.is_some())
    }

    /// Counts loaded object lifetimes, including objects with unresolved placement.
    #[must_use]
    pub fn resident_object_count(&self) -> usize {
        self.instances
            .iter()
            .filter(|instance| instance.resource.is_some())
            .count()
    }
    /// Counts shared prepared resources with at least one current object owner.
    #[must_use]
    pub fn resident_resource_count(&self) -> usize {
        self.resources.len()
    }
    /// Returns one retained object lifetime by its current GUID.
    #[must_use]
    pub fn object_identity(&self, guid: u64) -> Option<WorldObjectIdentity> {
        self.instances
            .iter()
            .find(|instance| instance.guid() == guid)
            .map(|instance| instance.identity)
    }
    /// Returns a currently resolved object matrix, independently of loading readiness.
    #[must_use]
    pub fn object_placement(&self, guid: u64) -> Option<GameObjectPlacement> {
        self.instances
            .iter()
            .find(|instance| instance.guid() == guid)
            .and_then(GameObjectInstance::placement)
    }

    /// Returns the GameObject's native passenger matrix and virtual facing.
    /// The initial map-model render/collision pose can differ from this matrix.
    ///
    /// # Errors
    /// Reports missing dependencies or an invalid retained parent conversion.
    pub fn object_movement_frame(
        &self,
        identity: WorldObjectIdentity,
    ) -> Result<Option<MovementTransportFrame>, GameObjectPlacementError> {
        let Some(instance) = self.movement_instance(identity) else {
            return Ok(None);
        };
        let placement = instance.passenger_placement?;
        MovementTransportFrame::new(placement.matrix(), placement.facing())
            .map(Some)
            .map_err(|_| GameObjectPlacementError::InvalidTransform)
    }
    /// Returns the retained transport resource family.
    #[must_use]
    pub fn resident_resource_kind(&self) -> Option<RuntimeGameObjectResourceKind> {
        self.resident()
            .and_then(GameObjectInstance::resource)
            .map(GameObjectResource::kind)
    }
    /// Returns the loaded local player's transport GUID.
    #[must_use]
    pub fn resident_guid(&self) -> Option<u64> {
        self.resident().map(GameObjectInstance::guid)
    }
    /// Returns the loaded transport's display row.
    #[must_use]
    pub fn resident_display_id(&self) -> Option<u32> {
        self.resident().map(GameObjectInstance::display_id)
    }
    /// Returns the loaded transport's generic state byte.
    #[must_use]
    pub fn resident_state(&self) -> Option<u8> {
        self.resident().map(GameObjectInstance::state)
    }
    /// Returns the loaded transport's supplied world position and facing.
    #[must_use]
    pub fn resident_transform(&self) -> Option<WorldTransform> {
        self.resident().and_then(|instance| instance.transform)
    }
    /// Returns the loaded transport's positive object scale.
    #[must_use]
    pub fn resident_scale(&self) -> Option<f32> {
        self.resident().and_then(|instance| instance.scale)
    }
    /// Returns the loaded transport's complete native placement.
    #[must_use]
    pub fn resident_placement(&self) -> Option<GameObjectPlacement> {
        self.resident().and_then(GameObjectInstance::placement)
    }
    /// Returns the loaded transport's unresolved placement dependency.
    #[must_use]
    pub fn resident_placement_error(&self) -> Option<GameObjectPlacementError> {
        self.resident()
            .and_then(|instance| instance.placement.err())
    }
    /// Returns the loaded transport's canonical asset identity.
    #[must_use]
    pub fn resident_asset_path(&self) -> Option<&AssetPath> {
        self.resident()
            .and_then(GameObjectInstance::resource)
            .map(GameObjectResource::path)
    }

    /// Retires every object lifetime and prevents old-world job publication.
    pub fn disconnect(&mut self) {
        if !self.instances.is_empty() {
            self.scene_revision = self.scene_revision.wrapping_add(1);
        }
        self.instances.clear();
        self.behaviors.clear();
        self.transport_behaviors.clear();
        self.passenger_transports.clear();
        self.game_object_passenger_transports.clear();
        self.scene_time_ms.set(0);
        self.placement_resolver.clear();
        self.indices.clear();
        self.resources.clear();
        self.world_identity = None;
        self.transport_guid = None;
        self.transport_identity = None;
        self.last_transport = None;
        if let Some(pending) = &mut self.pending {
            pending.eligible = false;
        }
        self.readiness = true;
        self.collect_unused();
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ResourceRequest {
    kind: RuntimeGameObjectResourceKind,
    path: AssetPath,
}
struct PendingGeneration {
    request: ResourceRequest,
    eligible: bool,
    task: CpuTask<GameObjectWorkerCompletion>,
}
struct TransportAdmission {
    identity: WorldObjectIdentity,
    display_id: u32,
    resource: Weak<GameObjectResource>,
    placed: bool,
}

fn resource_kind(path: &str) -> Option<RuntimeGameObjectResourceKind> {
    if path.ends_with(".WMO") {
        Some(RuntimeGameObjectResourceKind::WorldModel)
    } else if path.ends_with(".M2") || path.ends_with(".MDX") || path.ends_with(".MDL") {
        Some(RuntimeGameObjectResourceKind::M2)
    } else {
        None
    }
}
fn valid_transform(transform: &WorldTransform) -> bool {
    transform.position().is_finite() && transform.orientation().is_finite()
}
