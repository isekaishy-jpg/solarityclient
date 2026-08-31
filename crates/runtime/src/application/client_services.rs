//! Concrete ownership and shutdown of initial client services.

#![allow(unsafe_code)]

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::runtime::{Builder, Runtime};

use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetStore, AssetStoreHandle, CharacterAppearanceCatalog,
    CharacterRaceCatalog, CreatureCatalog, HelmetGeosetVisibilityCatalog, ItemDefinitionCatalog,
    ItemDisplayCatalog, ItemVisualCatalog, LightCatalog, MapCatalog, ParticleColorCatalog,
};
use solarity_cpu::CpuExecutor;
use solarity_media::SoundOutputTarget;
use solarity_network::{RealmEntry, WorldAddon, WorldAddonManifest};
use solarity_rendering::{
    M2ParticleTwinkleTable, VulkanBootstrap, VulkanRenderer, VulkanReport, WorldCamera,
    WorldModelBaseMip, WorldModelTextureFiltering,
};
use solarity_ui::{
    AddonCatalog, GlueManager, GlueStartupReport, STANDARD_ADDON_CRC, UiEventArgument,
    UiEventPayload, UiGlueNetworkAction, UiGlueNetworkStatus,
};

use crate::application::ApplicationError;
use crate::application::character_directory::RuntimeCharacterMetadata;
use crate::application::environment_coordinator::RuntimeWorldEnvironment;
use crate::application::gameplay_coordinator::RuntimeGameplayCoordinator;
use crate::application::login_coordinator::{
    RuntimeAuthenticatedLogin, RuntimeLoginCoordinator, RuntimeLoginError, RuntimeLoginPoll,
    RuntimeLoginState,
};
use crate::application::login_ui::LoginUiFrame;
use crate::application::player_coordinator::{
    RuntimeCreaturePoll, RuntimePlayerCatalogs, RuntimePlayerItemCatalogs, RuntimePlayerPoll,
    RuntimePlayerPresentation, RuntimeRemotePlayerPoll,
};
use crate::application::realm_directory::RuntimeRealmMetadata;
use crate::application::sound_coordinator::RuntimeSoundCoordinator;
use crate::application::terrain_coordinator::RuntimeTerrainCoordinator;
use crate::application::terrain_coordinator::RuntimeTerrainPoll;
use crate::application::terrain_frame::{RuntimeTerrainFrameError, TerrainFrame};
use crate::application::world_coordinator::{
    RuntimeWorldCoordinator, RuntimeWorldError, RuntimeWorldPoll, RuntimeWorldState,
};
use crate::configuration::RuntimeConfiguration;
use crate::platform::{PlatformEvent, SdlPlatform};
use crate::random::{BlizzardRand, CrtRand};

/// Concrete services owned exclusively by the application composition root.
pub(crate) struct ClientServices {
    renderer: VulkanRenderer,
    login_ui: LoginUiFrame,
    sound: RuntimeSoundCoordinator,
    platform: SdlPlatform,
    glue: GlueManager,
    cpu: CpuExecutor,
    network: Option<Runtime>,
    login: RuntimeLoginCoordinator,
    world: RuntimeWorldCoordinator,
    gameplay: RuntimeGameplayCoordinator,
    environment: RuntimeWorldEnvironment,
    player: RuntimePlayerPresentation,
    terrain: RuntimeTerrainCoordinator,
    terrain_frame: Option<TerrainFrame>,
    m2_global_clock: std::time::Instant,
    crt_rand: CrtRand,
    particle_twinkle: Arc<M2ParticleTwinkleTable>,
    blizzard_rand: BlizzardRand,
    realm_metadata: RuntimeRealmMetadata,
    character_metadata: RuntimeCharacterMetadata,
    addon_manifest: WorldAddonManifest,
    realm_directory_published: bool,
    world_session_published: bool,
    pending_realm_id: Option<u32>,
    selected_realm: Option<SelectedRealmFacts>,
    login_failures: VecDeque<RuntimeLoginError>,
    world_failures: VecDeque<RuntimeWorldError>,
    network_shutdown_timeout: std::time::Duration,
}

impl ClientServices {
    /// Constructs services in dependency order after all configuration validates.
    pub(crate) fn start(
        configuration: &RuntimeConfiguration,
    ) -> Result<(Self, usize, usize), ApplicationError> {
        let catalog =
            ArchiveCatalog::discover(configuration.data_root().clone(), configuration.locale())?;
        let archive_count = catalog.descriptors().len();
        let mut assets = AssetStore::mount(catalog)?;
        let animations = AnimationDataCatalog::load(&mut assets)?;
        let realm_metadata = RuntimeRealmMetadata::load(&mut assets)?;
        let character_metadata = RuntimeCharacterMetadata::load(&mut assets)?;
        let creatures = CreatureCatalog::load(&mut assets)?;
        let characters = CharacterAppearanceCatalog::load(&mut assets)?;
        let races = CharacterRaceCatalog::load(&mut assets)?;
        let helmet_visibility = HelmetGeosetVisibilityCatalog::load(&mut assets)?;
        let item_definitions = ItemDefinitionCatalog::load(&mut assets)?;
        let item_displays = ItemDisplayCatalog::load(&mut assets)?;
        let item_visuals = ItemVisualCatalog::load(&mut assets)?;
        let particle_colors = ParticleColorCatalog::load(&mut assets)?;
        let addon_catalog = AddonCatalog::discover(&mut assets)?;
        let maps = MapCatalog::load(&mut assets)?;
        let lights = LightCatalog::load(&mut assets)?;
        let addon_manifest = WorldAddonManifest::new(
            addon_catalog
                .addons()
                .iter()
                .map(|addon| {
                    WorldAddon::new(
                        addon.name(),
                        addon.is_initially_enabled(),
                        if addon.is_signed() {
                            STANDARD_ADDON_CRC
                        } else {
                            0
                        },
                        0,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        )?;
        let addon_count = addon_manifest.addons().len();
        // SDL must be initialized by the process main thread before worker
        // construction can make lifecycle mistakes harder to diagnose.
        let mut platform = SdlPlatform::start(configuration.window())?;
        let total_physical_memory_bytes = platform.total_physical_memory_bytes();
        let instance_extensions = platform.vulkan_instance_extensions()?;
        let bootstrap = VulkanBootstrap::start(&instance_extensions)?;
        // SAFETY: The bootstrap enabled SDL's exact extension list and remains
        // live while SDL creates a surface for the owned window.
        let surface = unsafe { platform.create_vulkan_surface(bootstrap.instance_handle()) }?;
        // SAFETY: SDL created `surface` from this bootstrap's instance, and
        // ownership transfers immediately to the rendering owner.
        let mut renderer = unsafe {
            bootstrap.attach_surface(surface, platform.pixel_extent(), configuration.gpu_index())
        }?;
        let assets = AssetStoreHandle::new(assets);
        let glue = GlueManager::start_shared(assets.clone(), platform.logical_extent(), false)?;
        glue.set_realm_directory(realm_metadata.empty_directory());
        let sound = RuntimeSoundCoordinator::start(
            assets.clone(),
            &glue,
            SoundOutputTarget::DefaultDevice,
        )?;
        let login_ui = LoginUiFrame::prepare(&mut renderer, &glue)?;
        login_ui.present(&mut renderer)?;
        platform.show()?;
        let cpu = CpuExecutor::new(configuration.cpu_pool())?;
        let network = Builder::new_multi_thread()
            .worker_threads(configuration.network_workers().get())
            .thread_name("solarity-network")
            .enable_io()
            .enable_time()
            .build()
            .map_err(|source| ApplicationError::NetworkRuntime {
                message: source.to_string(),
            })?;
        let login = RuntimeLoginCoordinator::new(configuration.login().clone());
        let world = RuntimeWorldCoordinator::new();
        // M2Initialize consumes these before constructing any emitter. The
        // resulting table remains process-wide across terrain generations.
        let mut crt_rand = CrtRand::new();
        let first = u32::from(crt_rand.next_u15());
        let second = u32::from(crt_rand.next_u15());
        let particle_twinkle = Arc::new(M2ParticleTwinkleTable::new(first << 16 | second));

        Ok((
            Self {
                renderer,
                login_ui,
                sound,
                platform,
                glue,
                cpu,
                network: Some(network),
                login,
                world,
                gameplay: RuntimeGameplayCoordinator::new(),
                environment: RuntimeWorldEnvironment::new(lights, total_physical_memory_bytes)?,
                player: RuntimePlayerPresentation::new(
                    assets.clone(),
                    RuntimePlayerCatalogs::new(
                        animations,
                        creatures,
                        characters,
                        races,
                        helmet_visibility,
                        RuntimePlayerItemCatalogs::new(
                            item_definitions,
                            item_displays,
                            item_visuals,
                        ),
                        particle_colors,
                    ),
                ),
                terrain: RuntimeTerrainCoordinator::new(assets, maps),
                terrain_frame: None,
                m2_global_clock: std::time::Instant::now(),
                crt_rand,
                particle_twinkle,
                blizzard_rand: BlizzardRand::new(sdl3::timer::ticks() as u32),
                realm_metadata,
                character_metadata,
                addon_manifest,
                realm_directory_published: false,
                world_session_published: false,
                pending_realm_id: None,
                selected_realm: None,
                login_failures: VecDeque::new(),
                world_failures: VecDeque::new(),
                network_shutdown_timeout: configuration.network_shutdown_timeout(),
            },
            archive_count,
            addon_count,
        ))
    }

    /// Returns the concrete adapter and swapchain facts selected at startup.
    pub(crate) fn vulkan_report(&self) -> &VulkanReport {
        self.renderer.report()
    }

    /// Returns proof that the built-in pre-world UI finished ordered startup.
    pub(crate) fn glue_report(&self) -> GlueStartupReport {
        self.glue.report()
    }

    /// Polls one translated main-thread platform event without allocating a batch.
    pub(crate) fn poll_platform_event(&mut self) -> Option<PlatformEvent> {
        self.platform.poll_event()
    }

    /// Presents one FIFO-paced Glue or resident-world frame.
    pub(crate) fn present_frame(&mut self) -> Result<(), ApplicationError> {
        let (Some(environment), Some(pose)) =
            (self.environment.current(), self.player.camera_pose())
        else {
            self.login_ui.present(&mut self.renderer)?;
            return Ok(());
        };
        let (width, height) = self.platform.pixel_extent();
        let aspect_ratio = width as f32 / height as f32;
        // These are the registered build-12340 defaults. The settings owner
        // will pass live CVar values through this same explicit boundary.
        let pose = self
            .terrain
            .resolve_player_camera(pose, aspect_ratio, true, true)?;
        let camera = WorldCamera::stock_following(
            pose.eye(),
            pose.target(),
            pose.up(),
            pose.orbit_pivot(),
            pose.subject(),
            environment.view_distance().value(),
        )
        .frame(aspect_ratio)?;
        if let Some(clock) = self.gameplay.realm_clock() {
            self.sound
                .update(&self.glue, clock, camera, &mut self.blizzard_rand)?;
        }
        let Some(plan) = self.terrain.resident_mesh_plan() else {
            self.login_ui.present(&mut self.renderer)?;
            return Ok(());
        };
        let global_animation_time_ms = self.m2_global_clock.elapsed().as_secs_f32() * 1_000.0;
        let Some(frame) = self.terrain_frame.as_mut() else {
            self.login_ui.present(&mut self.renderer)?;
            return Ok(());
        };
        let player = self
            .player
            .resident_frame_input()
            .ok_or(RuntimeTerrainFrameError::MissingPlayerM2FrameInput)?;
        let creatures = self.player.resident_creature_frame_inputs();
        let remote_players = self.player.resident_remote_player_frame_inputs();
        frame.present(
            &mut self.renderer,
            plan,
            environment,
            camera,
            global_animation_time_ms,
            &mut self.crt_rand,
            player,
            &creatures,
            &remote_players,
        )?;
        Ok(())
    }

    /// Applies ordered Glue actions and polls one asynchronous login result.
    pub(crate) fn service_login(&mut self) -> Result<(), ApplicationError> {
        let Some(network) = self.network.as_ref() else {
            return Ok(());
        };
        let handle = network.handle().clone();
        while let Some(action) = self.glue.take_network_action() {
            match action {
                UiGlueNetworkAction::Login(request) => {
                    let result = std::str::from_utf8(request.password_bytes())
                        .map_err(|_source| RuntimeLoginError::PasswordEncoding)
                        .and_then(|password| {
                            self.login.begin(&handle, request.account_name(), password)
                        });
                    match result {
                        Ok(()) => {
                            self.realm_directory_published = false;
                        }
                        Err(RuntimeLoginError::AlreadyActive)
                        | Err(RuntimeLoginError::AlreadyAuthenticated) => {}
                        Err(error) => self.publish_login_failure(error),
                    }
                }
                UiGlueNetworkAction::CancelLogin => {
                    self.login.cancel();
                    self.realm_directory_published = false;
                    self.glue.set_network_status(UiGlueNetworkStatus::default());
                }
                UiGlueNetworkAction::Disconnect => {
                    self.login.disconnect();
                    self.world.disconnect();
                    self.gameplay.disconnect();
                    self.environment.disconnect();
                    self.player.disconnect();
                    self.terrain.disconnect();
                    self.sound.disconnect()?;
                    self.terrain_frame = None;
                    self.realm_directory_published = false;
                    self.world_session_published = false;
                    self.pending_realm_id = None;
                    self.selected_realm = None;
                    self.glue
                        .set_realm_directory(self.realm_metadata.empty_directory());
                    self.glue.set_network_status(UiGlueNetworkStatus::default());
                }
                UiGlueNetworkAction::RequestRealmList {
                    show_progress_dialog,
                    status_message,
                } => {
                    if show_progress_dialog {
                        let status_message =
                            status_message.ok_or_else(|| ApplicationError::NetworkRuntime {
                                message: "Glue omitted REALM_LIST_IN_PROGRESS".to_owned(),
                            })?;
                        let payload = UiEventPayload::new([
                            UiEventArgument::String("CANCEL".to_owned()),
                            UiEventArgument::String(status_message),
                        ])?;
                        self.glue.dispatch_event("OPEN_STATUS_DIALOG", &payload)?;
                    }
                    match self.login.refresh_realms(&handle) {
                        Ok(()) | Err(RuntimeLoginError::AlreadyActive) => {}
                        Err(RuntimeLoginError::NotAuthenticated) => {}
                        Err(error) => self.publish_login_failure(error),
                    }
                }
                UiGlueNetworkAction::CancelRealmListQuery => {
                    self.login.cancel_realm_refresh();
                }
                UiGlueNetworkAction::ChangeRealm { realm_id } => {
                    self.pending_realm_id = Some(realm_id);
                    self.begin_pending_world(&handle);
                }
                UiGlueNetworkAction::SetPreferredRealmInfo {
                    category_index,
                    player_killing_allowed,
                    roleplaying,
                } => {
                    self.publish_preferred_realm(
                        category_index,
                        player_killing_allowed,
                        roleplaying,
                    )?;
                }
                UiGlueNetworkAction::SortRealms => {}
                UiGlueNetworkAction::RealmListDialogCancelled { from_login_screen } => {
                    if from_login_screen {
                        self.login.disconnect();
                        self.realm_directory_published = false;
                        self.glue.set_network_status(UiGlueNetworkStatus::default());
                    }
                }
                UiGlueNetworkAction::SelectCharacter { guid } => {
                    let payload = UiEventPayload::new([UiEventArgument::Number(guid as f64)])?;
                    self.glue
                        .dispatch_event("UPDATE_SELECTED_CHARACTER", &payload)?;
                }
                UiGlueNetworkAction::EnterWorld { guid } => {
                    match self.world.enter_world(&handle, guid) {
                        Ok(()) | Err(RuntimeWorldError::AlreadyActive) => {}
                        Err(error) => self.publish_world_failure(error),
                    }
                }
            }
        }

        match self.login.poll() {
            Ok(RuntimeLoginPoll::Idle | RuntimeLoginPoll::Pending) => {}
            Ok(RuntimeLoginPoll::Authenticated) if !self.realm_directory_published => {
                self.glue
                    .set_network_status(UiGlueNetworkStatus::new(None, true));
                self.glue
                    .dispatch_event("CLOSE_STATUS_DIALOG", &UiEventPayload::empty())?;
                let category_count = self
                    .authenticated_login()
                    .map(|authenticated| {
                        self.realm_metadata
                            .project(
                                authenticated.realms(),
                                None,
                                authenticated.has_tournament_access(),
                            )
                            .categories()
                            .iter()
                            .filter(|category| !category.realms().is_empty())
                            .count()
                    })
                    .unwrap_or(0);
                let category_count = i64::try_from(category_count).map_err(|source| {
                    ApplicationError::NetworkRuntime {
                        message: source.to_string(),
                    }
                })?;
                let payload = UiEventPayload::new([UiEventArgument::Integer(category_count)])?;
                self.publish_realm_directory("GET_PREFERRED_REALM_INFO", payload)?;
                self.realm_directory_published = true;
            }
            Ok(RuntimeLoginPoll::Authenticated) => {}
            Ok(RuntimeLoginPoll::RealmDirectoryUpdated) => {
                self.glue
                    .dispatch_event("CLOSE_STATUS_DIALOG", &UiEventPayload::empty())?;
                self.publish_realm_directory("OPEN_REALM_LIST", UiEventPayload::empty())?;
                self.realm_directory_published = true;
                self.begin_pending_world(&handle);
            }
            Ok(RuntimeLoginPoll::RealmDirectoryCancelled) => {}
            Err(error) => self.publish_login_failure(error),
        }
        match self.world.poll() {
            Ok(RuntimeWorldPoll::Idle | RuntimeWorldPoll::Pending) => {}
            Ok(RuntimeWorldPoll::CharacterDirectoryReady) if !self.world_session_published => {
                self.world_session_published = true;
                if let Some(selected) = &self.selected_realm {
                    self.glue.set_network_status(selected.status(true, false));
                }
                if let Some(selection) = self.world.character_selection() {
                    let characters = self.character_metadata.project(selection.directory())?;
                    let count = i64::try_from(characters.characters().len()).map_err(|source| {
                        ApplicationError::NetworkRuntime {
                            message: source.to_string(),
                        }
                    })?;
                    self.glue.set_character_directory(characters);
                    self.glue.dispatch_event(
                        "SET_GLUE_SCREEN",
                        &UiEventPayload::new([UiEventArgument::String("charselect".to_owned())])?,
                    )?;
                    self.glue.dispatch_event(
                        "CHARACTER_LIST_UPDATE",
                        &UiEventPayload::new([UiEventArgument::Integer(count)])?,
                    )?;
                    self.login_ui = LoginUiFrame::prepare(&mut self.renderer, &self.glue)?;
                }
            }
            Ok(RuntimeWorldPoll::CharacterDirectoryReady) => {}
            Ok(RuntimeWorldPoll::EnteredWorld) => {
                if let Some(entry) = self.world.take_world_entry() {
                    self.gameplay.begin(&handle, entry)?;
                }
            }
            Ok(RuntimeWorldPoll::CharacterRejected(rejection)) => {
                tracing::warn!(
                    result_code = rejection.result_code(),
                    reason = ?rejection.reason(),
                    "character login rejected"
                );
                if let Some(selection) = self.world.character_selection() {
                    let characters = self.character_metadata.project(selection.directory())?;
                    let count = i64::try_from(characters.characters().len()).map_err(|source| {
                        ApplicationError::NetworkRuntime {
                            message: source.to_string(),
                        }
                    })?;
                    self.glue.set_character_directory(characters);
                    self.glue.dispatch_event(
                        "CHARACTER_LIST_UPDATE",
                        &UiEventPayload::new([UiEventArgument::Integer(count)])?,
                    )?;
                    self.login_ui = LoginUiFrame::prepare(&mut self.renderer, &self.glue)?;
                }
            }
            Err(error) => self.publish_world_failure(error),
        }
        self.gameplay.service()?;
        self.environment
            .synchronize(self.gameplay.world(), self.gameplay.realm_clock())?;
        match self.player.synchronize(self.gameplay.world())? {
            RuntimePlayerPoll::ModelLoaded => {
                if let (Some(model), Some(height)) = (
                    self.player.resident_model(),
                    self.player.camera_subject_height(),
                ) {
                    tracing::debug!(
                        path = %model.path(),
                        camera_height = height.value(),
                        camera_height_source = ?height.source(),
                        "local player model became resident"
                    );
                }
                if let Some(frame) = self.terrain_frame.as_mut() {
                    frame.replace_player(
                        &mut self.renderer,
                        self.player.resident_frame_input(),
                        &mut self.crt_rand,
                    )?;
                }
            }
            RuntimePlayerPoll::Idle | RuntimePlayerPoll::Pending => {
                if let Some(frame) = self.terrain_frame.as_mut() {
                    frame.replace_player(&mut self.renderer, None, &mut self.crt_rand)?;
                }
            }
            RuntimePlayerPoll::Current => {}
        }
        match self.player.synchronize_creatures(self.gameplay.world())? {
            RuntimeCreaturePoll::ModelsChanged => {
                if let Some(frame) = self.terrain_frame.as_mut() {
                    let creatures = self.player.resident_creature_frame_inputs();
                    frame.replace_creatures(&mut self.renderer, &creatures, &mut self.crt_rand)?;
                }
            }
            RuntimeCreaturePoll::Idle => {
                if let Some(frame) = self.terrain_frame.as_mut() {
                    frame.replace_creatures(&mut self.renderer, &[], &mut self.crt_rand)?;
                }
            }
            RuntimeCreaturePoll::Current => {}
        }
        match self
            .player
            .synchronize_remote_players(self.gameplay.world())?
        {
            RuntimeRemotePlayerPoll::ModelsChanged => {
                if let Some(frame) = self.terrain_frame.as_mut() {
                    let players = self.player.resident_remote_player_frame_inputs();
                    frame.replace_remote_players(
                        &mut self.renderer,
                        &players,
                        &mut self.crt_rand,
                    )?;
                }
            }
            RuntimeRemotePlayerPoll::Idle => {
                if let Some(frame) = self.terrain_frame.as_mut() {
                    frame.replace_remote_players(&mut self.renderer, &[], &mut self.crt_rand)?;
                }
            }
            RuntimeRemotePlayerPoll::Current => {}
        }
        match self.terrain.synchronize(self.gameplay.world())? {
            RuntimeTerrainPoll::TileLoaded { tile, .. } => {
                let resident_tile = self.terrain.resident_tile().ok_or(
                    RuntimeTerrainFrameError::MissingMeshPlan {
                        tile_x: tile.x(),
                        tile_y: tile.y(),
                    },
                )?;
                self.sound.stage_terrain_tile(resident_tile);
                let plan = self.terrain.resident_mesh_plan().ok_or(
                    RuntimeTerrainFrameError::MissingMeshPlan {
                        tile_x: tile.x(),
                        tile_y: tile.y(),
                    },
                )?;
                let sources = self.terrain.resident_texture_sources().ok_or(
                    RuntimeTerrainFrameError::MissingTextureSources {
                        tile_x: tile.x(),
                        tile_y: tile.y(),
                    },
                )?;
                let world_models = self.terrain.resident_world_models().ok_or(
                    RuntimeTerrainFrameError::MissingWorldModelScene {
                        tile_x: tile.x(),
                        tile_y: tile.y(),
                    },
                )?;
                let m2_scene = self.terrain.resident_m2_scene().ok_or(
                    RuntimeTerrainFrameError::MissingM2Scene {
                        tile_x: tile.x(),
                        tile_y: tile.y(),
                    },
                )?;
                // Registered build-12340 defaults: textureFilteringMode 3 is
                // anisotropic 4x and BaseMip 0 begins at the authored top mip.
                // A settings owner will pass live typed values here directly.
                let frame = TerrainFrame::prepare(
                    &mut self.renderer,
                    plan,
                    sources,
                    m2_scene,
                    world_models,
                    WorldModelTextureFiltering::Anisotropic4x,
                    WorldModelBaseMip::Zero,
                    &mut self.crt_rand,
                    Arc::clone(&self.particle_twinkle),
                    self.player.resident_frame_input(),
                    &self.player.resident_creature_frame_inputs(),
                    &self.player.resident_remote_player_frame_inputs(),
                )?;
                tracing::debug!(
                    tile_x = tile.x(),
                    tile_y = tile.y(),
                    draw_count = frame.draw_count(),
                    m2_mesh_count = frame.m2_mesh_count(),
                    m2_placement_count = frame.m2_placement_count(),
                    world_model_placement_count = frame.world_model_placement_count(),
                    "resident terrain entered renderer resources"
                );
                self.terrain_frame = Some(frame);
            }
            RuntimeTerrainPoll::Idle | RuntimeTerrainPoll::GlobalWorldModel { .. } => {
                self.sound.disconnect()?;
                self.terrain_frame = None;
            }
            RuntimeTerrainPoll::Current { tile, .. } => {
                if self.terrain_frame.as_ref().map(TerrainFrame::tile) != Some(tile) {
                    return Err(RuntimeTerrainFrameError::MissingGpuGeneration {
                        tile_x: tile.x(),
                        tile_y: tile.y(),
                    }
                    .into());
                }
            }
        }
        Ok(())
    }

    /// Returns synchronous login ownership for diagnostics and Glue routing.
    pub(crate) const fn login_state(&self) -> RuntimeLoginState {
        self.login.state()
    }

    /// Returns the retained authenticated result before explicit realm selection.
    pub(crate) const fn authenticated_login(&self) -> Option<&RuntimeAuthenticatedLogin> {
        self.login.authenticated()
    }

    /// Returns synchronous ownership of the selected world-server phase.
    pub(crate) const fn world_state(&self) -> RuntimeWorldState {
        if self.gameplay.world().is_some() {
            RuntimeWorldState::InWorld
        } else {
            self.world.state()
        }
    }

    /// Takes the oldest login failure without collapsing distinct attempts.
    pub(crate) fn take_login_failure(&mut self) -> Option<RuntimeLoginError> {
        self.login_failures.pop_front()
    }

    /// Takes the oldest selected-world failure without collapsing attempts.
    pub(crate) fn take_world_failure(&mut self) -> Option<RuntimeWorldError> {
        self.world_failures.pop_front()
    }

    /// Returns startup facts that prove the configured window exists.
    pub(crate) fn window_facts(&self) -> (u32, (u32, u32), (u32, u32)) {
        (
            self.platform.window_id().value(),
            self.platform.logical_extent(),
            self.platform.pixel_extent(),
        )
    }

    /// Returns output format, software count, and hard virtual capacity.
    pub(crate) fn sound_facts(&self) -> (u32, u8, usize, usize) {
        let info = self.sound.output_info();
        (
            info.sample_rate_hz(),
            info.channel_count(),
            self.sound.software_channel_count(),
            self.sound.engine_voice_capacity(),
        )
    }

    /// Shuts down task admission before consuming the async runtime.
    pub(crate) fn shutdown(&mut self) -> Result<(), ApplicationError> {
        self.login.disconnect();
        self.world.disconnect();
        self.gameplay.disconnect();
        self.environment.disconnect();
        self.player.disconnect();
        self.terrain.disconnect();
        self.sound.disconnect()?;
        self.terrain_frame = None;
        let renderer_result = self.renderer.shutdown().map_err(ApplicationError::from);
        let cpu_result = self.cpu.shutdown().map_err(ApplicationError::from);
        if let Some(network) = self.network.take() {
            network.shutdown_timeout(self.network_shutdown_timeout);
        }
        renderer_result?;
        cpu_result
    }
}

impl ClientServices {
    fn publish_login_failure(&mut self, error: RuntimeLoginError) {
        self.realm_directory_published = false;
        self.glue.set_network_status(UiGlueNetworkStatus::default());
        tracing::warn!(error = %error, "login exchange failed");
        self.login_failures.push_back(error);
    }

    fn publish_world_failure(&mut self, error: RuntimeWorldError) {
        self.world_session_published = false;
        if let Some(selected) = &self.selected_realm {
            self.glue.set_network_status(selected.status(false, true));
        } else {
            self.glue.set_network_status(UiGlueNetworkStatus::default());
        }
        tracing::warn!(error = %error, "world authentication failed");
        self.world_failures.push_back(error);
    }

    fn begin_pending_world(&mut self, runtime: &tokio::runtime::Handle) {
        let Some(realm_id) = self.pending_realm_id else {
            return;
        };
        let Ok(realm_id) = u8::try_from(realm_id) else {
            return;
        };
        let Some(realm) = self
            .login
            .authenticated()
            .and_then(|authenticated| authenticated.realms().by_id(realm_id))
            .cloned()
        else {
            return;
        };
        let Some(authenticated) = self.login.take_authenticated() else {
            return;
        };
        let selected = SelectedRealmFacts::new(&self.realm_metadata, &realm);
        match self
            .world
            .begin(runtime, authenticated, realm, self.addon_manifest.clone())
        {
            Ok(()) => {
                self.pending_realm_id = None;
                self.world_session_published = false;
                self.glue.set_network_status(selected.status(false, false));
                self.selected_realm = Some(selected);
            }
            Err(error) => self.publish_world_failure(error),
        }
    }

    fn publish_realm_directory(
        &mut self,
        event: &str,
        payload: UiEventPayload,
    ) -> Result<(), ApplicationError> {
        let Some(authenticated) = self.login.authenticated() else {
            return Ok(());
        };
        let realms = self.realm_metadata.project(
            authenticated.realms(),
            None,
            authenticated.has_tournament_access(),
        );
        self.glue.set_realm_directory(realms);
        self.glue.dispatch_event(event, &payload)?;
        self.login_ui = LoginUiFrame::prepare(&mut self.renderer, &self.glue)?;
        Ok(())
    }

    fn publish_preferred_realm(
        &mut self,
        category_index: u32,
        player_killing_allowed: bool,
        roleplaying: bool,
    ) -> Result<(), ApplicationError> {
        let Some(authenticated) = self.login.authenticated() else {
            return Ok(());
        };
        let event = self.realm_metadata.preferred_realm(
            authenticated.realms(),
            category_index,
            player_killing_allowed,
            roleplaying,
        );
        if let Some((_realm_id, realm_index)) = event {
            let payload = UiEventPayload::new([
                UiEventArgument::Integer(i64::from(category_index)),
                UiEventArgument::Integer(i64::from(realm_index)),
            ])?;
            self.glue.dispatch_event("SUGGEST_REALM", &payload)?;
        } else {
            self.glue
                .dispatch_event("OPEN_REALM_LIST", &UiEventPayload::empty())?;
        }
        self.login_ui = LoginUiFrame::prepare(&mut self.renderer, &self.glue)?;
        Ok(())
    }
}

/// Selected realm facts retained while transport ownership moves to a worker.
struct SelectedRealmFacts {
    name: String,
    player_killing_allowed: bool,
    roleplaying: bool,
}

impl SelectedRealmFacts {
    fn new(metadata: &RuntimeRealmMetadata, realm: &RealmEntry) -> Self {
        let (player_killing_allowed, roleplaying) = metadata.realm_rules(realm);
        Self {
            name: realm.name().to_owned(),
            player_killing_allowed,
            roleplaying,
        }
    }

    fn status(&self, connected: bool, server_down: bool) -> UiGlueNetworkStatus {
        UiGlueNetworkStatus::new(Some(self.name.clone()), connected).with_realm_rules(
            self.player_killing_allowed,
            self.roleplaying,
            server_down,
        )
    }
}

impl Drop for ClientServices {
    fn drop(&mut self) {
        let _shutdown_result = self.shutdown();
    }
}
