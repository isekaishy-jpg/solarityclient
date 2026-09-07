//! Concrete ownership and shutdown of initial client services.

#![allow(unsafe_code)]

pub(super) mod glue_benchmark;
pub(super) mod world_benchmark;
mod world_camera;
mod world_transfer;

#[cfg(test)]
#[path = "../../tests/application/terrain_publication.rs"]
mod terrain_publication_tests;

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;

use tokio::runtime::{Builder, Runtime};

use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetError, AssetPath, AssetStore, AssetStoreHandle,
    BlpTextureCache, CharacterAppearanceCatalog, CharacterRaceCatalog, CharacterStartOutfitCatalog,
    CreatureCatalog, CreatureFamilyCatalog, GameObjectDisplayCatalog,
    HelmetGeosetVisibilityCatalog, ItemDefinitionCatalog, ItemDisplayCatalog, ItemVisualCatalog,
    LightCatalog, LoadingScreenCatalog, MapCatalog, ParticleColorCatalog,
};
use solarity_cpu::{CpuError, CpuExecutor, CpuTask};
use solarity_media::SoundOutputTarget;
use solarity_network::{
    AccountExpansion, CharacterCreation, CharacterRename, CharacterRenameError, RealmEntry,
    WorldAddon, WorldAddonManifest,
};
use solarity_rendering::{
    CharacterComponentTextureLevel, M2ParticleTwinkleTable, UiPreparedDraw, VulkanBootstrap,
    VulkanPresentMode, VulkanRenderer, VulkanReport, WorldModelBaseMip, WorldModelTextureFiltering,
    glue_ghost_sunlight,
};
use solarity_systems::MountCameraGeometry;
use solarity_ui::{
    AddonCatalog, GlueError, GlueInitialScreen, GlueManager, GlueStartupReport, STANDARD_ADDON_CRC,
    UiCharacterExpansion, UiEventArgument, UiEventPayload, UiGlueNetworkAction,
    UiGlueNetworkStatus, UiKeyboardModifiers, UiPointerButton, UiProcessAction,
};

use crate::application::ApplicationError;
use crate::application::character_directory::RuntimeCharacterMetadata;
use crate::application::cinematic_coordinator::{
    RuntimeCinematicCoordinator, RuntimeCinematicPoll,
};
use crate::application::developer_console::RuntimeDeveloperConsole;
use crate::application::environment_coordinator::RuntimeWorldEnvironment;
use crate::application::frame_profile::RuntimeFrameProfile;
use crate::application::game_object_coordinator::{
    RuntimeGameObjectPresentation, RuntimeTransportPoll,
};
use crate::application::gameplay_coordinator::RuntimeGameplayCoordinator;
use crate::application::login_coordinator::{
    RuntimeAuthenticatedLogin, RuntimeLoginCoordinator, RuntimeLoginError, RuntimeLoginPoll,
    RuntimeLoginState,
};
use crate::application::login_model::{
    RuntimeGlueModelError, RuntimeGlueModelPoll, RuntimeGlueModelScene,
};
use crate::application::login_ui::{RuntimeUiFrame, RuntimeUiResidency};
use crate::application::performance_overlay::{RuntimeFpsOverlay, overlay_extent};
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
    RuntimeCharacterScreenRequests, RuntimeWorldCoordinator, RuntimeWorldError, RuntimeWorldPoll,
    RuntimeWorldState,
};
use crate::application::world_transfer::RuntimeWorldTransferCoordinator;
use crate::application::world_ui::RuntimeWorldUi;
use crate::configuration::{RuntimeConfiguration, StartupProfile};
use crate::input::{InputControl, InputFrameMotion, stock_keyboard_name};
use crate::loading::{LoadingScreenDirectory, RuntimeLoadingReadiness, RuntimeLoadingScreen};
use crate::platform::{ButtonState, MouseButton, MouseWheelDirection, PlatformEvent, SdlPlatform};
use crate::random::{BlizzardRand, CrtRand};

// Build 12340 shares Dwarf/Gnome and Orc/Troll Glue backdrops. There are no
// UI_Gnome.m2 or UI_Troll.m2 assets in the stock archive set.
const STOCK_CHARACTER_BACKDROPS: [&str; 8] = [
    "Interface\\Glues\\Models\\UI_Human\\UI_Human.m2",
    "Interface\\Glues\\Models\\UI_Orc\\UI_Orc.m2",
    "Interface\\Glues\\Models\\UI_Dwarf\\UI_Dwarf.m2",
    "Interface\\Glues\\Models\\UI_NightElf\\UI_NightElf.m2",
    "Interface\\Glues\\Models\\UI_Scourge\\UI_Scourge.m2",
    "Interface\\Glues\\Models\\UI_Tauren\\UI_Tauren.m2",
    "Interface\\Glues\\Models\\UI_BloodElf\\UI_BloodElf.m2",
    "Interface\\Glues\\Models\\UI_Draenei\\UI_Draenei.m2",
];

/// Concrete services owned exclusively by the application composition root.
pub(crate) struct ClientServices {
    renderer: VulkanRenderer,
    login_ui: Option<RuntimeUiFrame>,
    /// Second UI slot used as a candidate during an atomic screen transition.
    ///
    /// After publication the retired frame returns here, so Glue screen
    /// changes reuse two renderer allocations instead of leaking one mesh per
    /// transition.
    pending_login_ui: Option<(String, RuntimeUiFrame)>,
    /// Live Glue state changed while the renderer-resident frame remains owned.
    glue_ui_dirty: bool,
    /// Last Glue screen actually submitted to the swapchain.
    ///
    /// Input may select a different screen while SDL still has events queued
    /// for the old presentation. Process-ending actions are admitted only
    /// after the selected screen has crossed this presentation boundary.
    presented_glue_screen: Option<String>,
    /// Whether the last presented Glue frame contained its entire requested scene.
    last_glue_model_poll: RuntimeGlueModelPoll,
    ui_textures: BlpTextureCache,
    ui_texture_residency: RuntimeUiResidency,
    glue_gpu_texture_prewarm_pending: bool,
    pending_glue_texture_prewarm: Option<ConfiguredGlueTexturePrewarmJob>,
    world_ui: Option<RuntimeWorldUi>,
    world_ui_catalog: ArchiveCatalog,
    glue_model: RuntimeGlueModelScene,
    cinematic: RuntimeCinematicCoordinator,
    sound: RuntimeSoundCoordinator,
    platform: SdlPlatform,
    input: InputControl,
    glue: GlueManager,
    assets: AssetStoreHandle,
    startup_profile: StartupProfile,
    cpu: CpuExecutor,
    network: Option<Runtime>,
    login: RuntimeLoginCoordinator,
    world: RuntimeWorldCoordinator,
    gameplay: RuntimeGameplayCoordinator,
    world_transfer: RuntimeWorldTransferCoordinator,
    environment: RuntimeWorldEnvironment,
    player: RuntimePlayerPresentation,
    player_movement: super::player_movement::RuntimePlayerMovement,
    remote_movement: super::player_movement::remote::RuntimeRemoteMovement,
    game_objects: RuntimeGameObjectPresentation,
    terrain: RuntimeTerrainCoordinator,
    /// Shared authored liquid behavior for the camera's resident water query.
    liquids: solarity_asset::LiquidTypeCatalog,
    terrain_frame: Option<TerrainFrame>,
    fps: Option<RuntimeFpsOverlay>,
    developer_console: RuntimeDeveloperConsole,
    runtime_overlay_draws: Vec<UiPreparedDraw>,
    loading_directory: LoadingScreenDirectory,
    loading_screen: Option<RuntimeLoadingScreen>,
    loading_screen_cache: HashMap<(u32, (u32, u32)), RuntimeLoadingScreen>,
    loading_screen_prewarm_queue: VecDeque<u32>,
    glue_update_clock: std::time::Instant,
    m2_global_clock: std::time::Instant,
    crt_rand: CrtRand,
    particle_twinkle: Arc<M2ParticleTwinkleTable>,
    blizzard_rand: Rc<RefCell<BlizzardRand>>,
    realm_metadata: RuntimeRealmMetadata,
    character_metadata: RuntimeCharacterMetadata,
    addon_manifest: WorldAddonManifest,
    addon_catalog: AddonCatalog,
    realm_directory_published: bool,
    character_screen_published: bool,
    character_directory_published: bool,
    /// Backdrop GPU residency is drained only after the authentication popup
    /// has reached the swapchain, keeping that one-time work covered.
    authentication_prewarm_active: bool,
    pending_character_screen_requests: RuntimeCharacterScreenRequests,
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
        let mut startup_profile = StartupProfile::load(configuration.profile_root())?;
        let initial_screen = if startup_profile.play_intro_movie() {
            GlueInitialScreen::Movie
        } else {
            GlueInitialScreen::Login
        };
        let catalog =
            ArchiveCatalog::discover(configuration.data_root().clone(), configuration.locale())?;
        let archive_count = catalog.descriptors().len();
        let sound_catalog = catalog.clone();
        let backdrop_catalog = catalog.clone();
        let ui_texture_catalog = catalog.clone();
        let player_catalog = catalog.clone();
        let transport_catalog = catalog.clone();
        let terrain_catalog = catalog.clone();
        let world_ui_catalog = catalog.clone();
        let mut assets = AssetStore::mount(catalog)?;
        let animations = Arc::new(AnimationDataCatalog::load(&mut assets)?);
        let realm_metadata = RuntimeRealmMetadata::load(&mut assets)?;
        let character_metadata = RuntimeCharacterMetadata::load(&mut assets)?;
        let creatures = CreatureCatalog::load(&mut assets)?;
        let creature_families = CreatureFamilyCatalog::load(&mut assets)?;
        let characters = CharacterAppearanceCatalog::load(&mut assets)?;
        let races = CharacterRaceCatalog::load(&mut assets)?;
        let helmet_visibility = HelmetGeosetVisibilityCatalog::load(&mut assets)?;
        let start_outfits = CharacterStartOutfitCatalog::load(&mut assets)?;
        let item_definitions = ItemDefinitionCatalog::load(&mut assets)?;
        let item_displays = ItemDisplayCatalog::load(&mut assets)?;
        let item_visuals = ItemVisualCatalog::load(&mut assets)?;
        let particle_colors = ParticleColorCatalog::load(&mut assets)?;
        let game_object_displays = GameObjectDisplayCatalog::load(&mut assets)?;
        let addon_catalog = AddonCatalog::discover(&mut assets)?;
        let maps = MapCatalog::load(&mut assets)?;
        let loading_screens = match LoadingScreenCatalog::load(&mut assets) {
            Ok(catalog) => Some(catalog),
            Err(AssetError::AssetNotFound { .. }) => None,
            Err(error) => return Err(error.into()),
        };
        let loading_directory = LoadingScreenDirectory::new(&maps, loading_screens);
        let lights = LightCatalog::load(&mut assets)?;
        let liquids = solarity_asset::LiquidTypeCatalog::load(&mut assets)?;
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
        let input = InputControl::new(platform.window_id());
        let instance_extensions = platform.vulkan_instance_extensions()?;
        let bootstrap = VulkanBootstrap::start(&instance_extensions)?;
        // SAFETY: The bootstrap enabled SDL's exact extension list and remains
        // live while SDL creates a surface for the owned window.
        let surface = unsafe { platform.create_vulkan_surface(bootstrap.instance_handle()) }?;
        // SAFETY: SDL created `surface` from this bootstrap's instance, and
        // ownership transfers immediately to the rendering owner.
        let vsync = startup_profile
            .cvar_values()
            .iter()
            .rev()
            .find(|(name, _value)| name.eq_ignore_ascii_case("gxVSync"))
            .is_none_or(|(_name, value)| value.parse::<f64>().is_ok_and(|value| value != 0.0));
        let present_mode = if vsync {
            VulkanPresentMode::Synchronized
        } else {
            VulkanPresentMode::Uncapped
        };
        let mut renderer = unsafe {
            bootstrap.attach_surface_with_present_mode(
                surface,
                platform.pixel_extent(),
                configuration.gpu_index(),
                present_mode,
            )
        }?;
        renderer.configure_pipeline_cache(
            &configuration
                .profile_root()
                .join("Cache")
                .join("vulkan-pipelines.bin"),
        )?;
        let assets = AssetStoreHandle::new(assets);
        let mut fps = RuntimeFpsOverlay::prepare(&mut renderer, &assets, platform.pixel_extent())?;
        let developer_console =
            RuntimeDeveloperConsole::new(overlay_extent(platform.pixel_extent()));
        // The stock process owns one Blizzard RNG stream. Character creation
        // and sound variation consume it in actual main-thread call order.
        let blizzard_rand = Rc::new(RefCell::new(BlizzardRand::new(sdl3::timer::ticks() as u32)));
        if initial_screen == GlueInitialScreen::Movie {
            // Consume only once the expensive native prerequisites have
            // succeeded and immediately before selecting the movie Glue
            // screen. A failed archive, SDL, or Vulkan startup must not spend
            // the user's one-shot intro request without presenting a frame.
            let consumed = startup_profile.consume_intro_movie()?;
            debug_assert!(consumed);
        }
        tracing::info!(?initial_screen, "selected initial Glue screen");
        let glue = GlueManager::start_shared_with_profile_and_random(
            assets.clone(),
            platform.logical_extent(),
            false,
            initial_screen,
            startup_profile.cvar_values(),
            &addon_catalog,
            blizzard_rand.clone(),
        )?;
        glue.set_realm_directory(realm_metadata.empty_directory());
        let mut sound = RuntimeSoundCoordinator::start(
            assets.clone(),
            &glue,
            SoundOutputTarget::DefaultDevice,
            sound_catalog,
        )?;
        // M2Initialize consumes these before any ordinary or Glue emitter is
        // constructed. The resulting table remains process-wide.
        let mut crt_rand = CrtRand::new();
        let first = u32::from(crt_rand.next_u15());
        let second = u32::from(crt_rand.next_u15());
        let particle_twinkle = Arc::new(M2ParticleTwinkleTable::new(first << 16 | second));
        let cpu = CpuExecutor::new(configuration.cpu_pool())?;
        // The stock default ghost callback samples LightParams row 3 at time
        // zero, independent of the active world, camera, and Glue animation.
        let ghost_sunlight = lights
            .model_light_colors(3, 0)
            .map(|colors| glue_ghost_sunlight(colors.ambient(), colors.diffuse()));
        let mut glue_model =
            RuntimeGlueModelScene::new(backdrop_catalog, ghost_sunlight, Arc::clone(&animations));
        let login_model = glue
            .configured_model_presentation("AccountLogin")
            .map_err(GlueError::from)?;
        let background_light_count = login_model
            .as_ref()
            .map(|login_model| {
                login_model
                    .background_lights()
                    .live()
                    .iter()
                    .filter(|light| light.is_some())
                    .count()
            })
            .unwrap_or(0);
        if initial_screen == GlueInitialScreen::Movie
            && let Some(login_model) = login_model.as_ref()
        {
            glue_model.prewarm(login_model.path().clone(), background_light_count, &cpu)?;
            glue_model.synchronize_script_models(&glue, &cpu, &mut crt_rand)?;
            // OnLoad assigns the source while hidden. OnShow starts sequence
            // zero when login appears, after the movie has finished.
            glue_model.finish_prewarm(&mut renderer, login_model)?;
        }
        if initial_screen != GlueInitialScreen::Movie {
            let _initial_model_poll = glue_model.synchronize(
                &mut renderer,
                &glue,
                &cpu,
                &mut crt_rand,
                Arc::clone(&particle_twinkle),
                None,
                false,
                false,
            )?;
        }
        // The finite racial set is decoded through a separately mounted worker
        // archive stack. Startup therefore does not parse ten M2s and all of
        // their textures on the presentation thread before showing the window.
        let backdrop_paths = STOCK_CHARACTER_BACKDROPS
            .into_iter()
            .map(AssetPath::new)
            .collect::<Result<Vec<_>, _>>()?;
        glue_model.prewarm_backdrops(backdrop_paths, &cpu)?;
        let configured_texture_paths = glue.configured_texture_paths();
        let pending_glue_texture_prewarm = if configured_texture_paths.is_empty() {
            None
        } else {
            let worker_catalog = ui_texture_catalog.clone();
            let worker_paths = configured_texture_paths.clone();
            match cpu
                .try_submit(move || load_configured_glue_textures(worker_catalog, worker_paths))
            {
                Ok(task) => Some(ConfiguredGlueTexturePrewarmJob::Running(task)),
                Err(CpuError::AtCapacity { limit }) => {
                    tracing::warn!(
                        max_in_flight = limit.get(),
                        "deferred configured Glue texture prewarm because workers are full"
                    );
                    Some(ConfiguredGlueTexturePrewarmJob::Deferred {
                        catalog: ui_texture_catalog,
                        paths: configured_texture_paths,
                    })
                }
                Err(error) => return Err(error.into()),
            }
        };
        let mut ui_textures = BlpTextureCache::new();
        let mut ui_texture_residency = RuntimeUiResidency::new();
        let login_ui = if glue.media_intent().movie().is_some() {
            None
        } else {
            let frame = RuntimeUiFrame::prepare_glue(
                &mut renderer,
                &glue,
                &mut ui_textures,
                &mut ui_texture_residency,
            )?;
            let overlay = if glue.cvar_boolean("showfps") {
                fps.as_ref().map_or(&[][..], RuntimeFpsOverlay::draws)
            } else {
                &[]
            };
            let model_presented =
                glue_model.present(&mut renderer, &glue, &frame, 0.0, &mut crt_rand, overlay)?;
            if !model_presented {
                frame.present_with_overlay(&mut renderer, overlay)?;
            }
            if let Some((camera, events)) = glue_model.drain_sound_events() {
                sound.play_m2_events(&events, camera, &mut blizzard_rand.borrow_mut())?;
            }
            if let Some(fps) = fps.as_mut() {
                fps.record_presented(&mut renderer, std::time::Instant::now())?;
            }
            Some(frame)
        };
        let presented_glue_screen = login_ui.as_ref().map(|_| glue.current_screen());
        platform.set_text_input_active(glue.focused_edit_box().is_some());
        platform.show()?;
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
        Ok((
            Self {
                renderer,
                login_ui,
                pending_login_ui: None,
                glue_ui_dirty: false,
                presented_glue_screen,
                last_glue_model_poll: RuntimeGlueModelPoll::Pending,
                ui_textures,
                ui_texture_residency,
                glue_gpu_texture_prewarm_pending: false,
                pending_glue_texture_prewarm,
                world_ui: None,
                glue_model,
                cinematic: RuntimeCinematicCoordinator::default(),
                sound,
                platform,
                input,
                glue,
                assets: assets.clone(),
                world_ui_catalog,
                startup_profile,
                cpu,
                network: Some(network),
                login,
                world,
                gameplay: RuntimeGameplayCoordinator::new(),
                world_transfer: RuntimeWorldTransferCoordinator::new(),
                environment: RuntimeWorldEnvironment::new(lights, total_physical_memory_bytes)?,
                player_movement: super::player_movement::RuntimePlayerMovement::default(),
                remote_movement: super::player_movement::remote::RuntimeRemoteMovement::default(),
                player: RuntimePlayerPresentation::new(
                    assets.clone(),
                    RuntimePlayerCatalogs::new(
                        Arc::clone(&animations),
                        creatures,
                        creature_families,
                        characters,
                        races,
                        helmet_visibility,
                        start_outfits,
                        RuntimePlayerItemCatalogs::new(
                            item_definitions,
                            item_displays,
                            item_visuals,
                        ),
                        particle_colors,
                    ),
                )
                .with_glue_worker_catalog(player_catalog),
                game_objects: RuntimeGameObjectPresentation::new(
                    assets.clone(),
                    game_object_displays,
                    animations,
                )
                .with_worker_catalog(transport_catalog),
                terrain: RuntimeTerrainCoordinator::new(assets, maps)
                    .with_worker_catalog(terrain_catalog),
                terrain_frame: None,
                liquids,
                fps,
                developer_console,
                runtime_overlay_draws: Vec::new(),
                loading_directory,
                loading_screen: None,
                loading_screen_cache: HashMap::new(),
                loading_screen_prewarm_queue: VecDeque::new(),
                glue_update_clock: std::time::Instant::now(),
                m2_global_clock: std::time::Instant::now(),
                crt_rand,
                particle_twinkle,
                blizzard_rand,
                realm_metadata,
                character_metadata,
                addon_manifest,
                addon_catalog,
                realm_directory_published: false,
                character_screen_published: false,
                character_directory_published: false,
                authentication_prewarm_active: false,
                pending_character_screen_requests: RuntimeCharacterScreenRequests::default(),
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
    pub(crate) fn poll_platform_event(&mut self) -> Option<crate::TimedPlatformEvent> {
        let event = self.platform.poll_event()?;
        self.input.admit(&event.event);
        self.publish_ui_modifier_keys();
        Some(event)
    }

    fn publish_ui_modifier_keys(&self) {
        let bits = self.input.modifiers().bits();
        let keys = solarity_ui::UiModifierKeys::new(
            bits & 1 != 0,
            bits & 2 != 0,
            bits & 0x40 != 0,
            bits & 0x80 != 0,
            bits & 0x100 != 0,
            bits & 0x200 != 0,
        );
        self.glue.set_modifier_keys(keys);
        if let Some(ui) = &self.world_ui {
            ui.set_modifier_keys(keys);
        }
    }

    /// Returns the retained raw input state committed during event polling.
    pub(crate) const fn input_control(&self) -> &InputControl {
        &self.input
    }

    /// Takes relative pointer and wheel motion accumulated for one frame.
    pub(crate) fn take_input_frame_motion(&mut self) -> InputFrameMotion {
        self.input.take_frame_motion()
    }

    /// Captures a live UI fault without weakening renderer or protocol failures.
    pub(crate) fn record_recoverable_error(&mut self, error: &ApplicationError) -> bool {
        if !is_recoverable_presentation_error(error) {
            return false;
        }
        let message = error.to_string();
        tracing::error!(error = %error, "contained recoverable runtime UI error");
        self.developer_console.record_error(&message);
        true
    }

    /// Routes one already-admitted platform event to the active Glue owner.
    pub(crate) fn service_platform_event(
        &mut self,
        event: &PlatformEvent,
        timestamp_ms: u32,
    ) -> Result<(), ApplicationError> {
        if let Some(world_ui) = self.world_ui.as_mut() {
            world_ui.set_input_event_time(timestamp_ms);
        }
        if self.developer_console.service_event(
            event,
            self.platform.window_id(),
            self.platform.logical_extent(),
        ) {
            self.platform
                .set_text_input_active(self.developer_console.is_visible());
            if let Some(world_ui) = self.world_ui.as_mut() {
                world_ui.route_binding(event, self.input.modifiers(), true)?;
            }
            return Ok(());
        }
        if self.loading_screen.is_some() || self.world_transfer.is_entering_world() {
            if let Some(world_ui) = self.world_ui.as_mut() {
                world_ui.route_binding(event, self.input.modifiers(), true)?;
            }
            return Ok(());
        }
        if self.gameplay.world().is_some() {
            return self.service_world_platform_event(event, timestamp_ms);
        }
        if !matches!(event, PlatformEvent::MouseMotion(_)) && self.glue.flush_deferred_refresh()? {
            self.glue_ui_dirty = true;
        }
        // Lua owns the selected screen before its renderer transaction is
        // necessarily complete. Do not route input into that invisible tree;
        // it could activate controls that do not belong to the retained frame
        // the user can still see.
        let current_screen = self.glue.current_screen();
        let selected_screen_is_presented =
            glue_screen_is_presented(&current_screen, self.presented_glue_screen.as_deref());
        match event {
            PlatformEvent::Key(key_event) if key_event.window_id == self.platform.window_id() => {
                let Some(scan_code) = key_event.scan_code else {
                    return Ok(());
                };
                let Some(key) = stock_keyboard_name(scan_code) else {
                    return Ok(());
                };
                let movie = self.glue.media_intent().movie().cloned();
                if let Some(movie) = movie {
                    if key_event.state == ButtonState::Released {
                        self.glue.movie_key_up(movie.object_index(), key)?;
                        if let Some(object_index) = self.glue.take_movie_stop_completion() {
                            self.glue.movie_finished(object_index)?;
                        }
                        self.login_ui = None;
                    }
                } else if selected_screen_is_presented {
                    let modifiers = UiKeyboardModifiers::new(
                        key_event.modifiers.has_shift(),
                        key_event.modifiers.has_control(),
                        key_event.modifiers.has_alt(),
                    );
                    if self
                        .glue
                        .keyboard_key(key, key_event.state == ButtonState::Pressed, modifiers)?
                        .is_some()
                    {
                        self.glue_ui_dirty = true;
                    }
                }
            }
            PlatformEvent::TextInput(input)
                if input.window_id == self.platform.window_id()
                    && selected_screen_is_presented
                    && self.glue.media_intent().movie().is_none() =>
            {
                if self.glue.text_input(&input.text)?.is_some() {
                    self.glue_ui_dirty = true;
                }
            }
            PlatformEvent::TextEditing(composition)
                if composition.window_id == self.platform.window_id()
                    && selected_screen_is_presented
                    && self.glue.media_intent().movie().is_none() =>
            {
                if self.glue.text_composition(&composition.text)?.is_some() {
                    self.glue_ui_dirty = true;
                }
            }
            PlatformEvent::MouseButton(pointer)
                if pointer.window_id == self.platform.window_id()
                    && selected_screen_is_presented
                    && self.glue.media_intent().movie().is_none() =>
            {
                let Some(button) = glue_pointer_button(pointer.button) else {
                    return Ok(());
                };
                let (window_width, window_height) = self.platform.logical_extent();
                let (ui_width, ui_height) = self.glue.geometry().ui_extent();
                let position = (
                    f64::from(pointer.x) / f64::from(window_width) * ui_width,
                    ui_height - f64::from(pointer.y) / f64::from(window_height) * ui_height,
                );
                let modifiers = self.input.modifiers();
                let dispatch = self.glue.pointer_button_with_modifiers(
                    position,
                    button,
                    pointer.state == ButtonState::Pressed,
                    pointer.click_count,
                    UiKeyboardModifiers::new(
                        modifiers.has_shift(),
                        modifiers.has_control(),
                        modifiers.has_alt(),
                    ),
                )?;
                if dispatch.object_index().is_some() {
                    self.glue_ui_dirty = true;
                }
            }
            PlatformEvent::MouseMotion(pointer)
                if pointer.window_id == self.platform.window_id()
                    && selected_screen_is_presented
                    && self.glue.media_intent().movie().is_none() =>
            {
                let (window_width, window_height) = self.platform.logical_extent();
                let (ui_width, ui_height) = self.glue.geometry().ui_extent();
                let position = (
                    f64::from(pointer.x) / f64::from(window_width) * ui_width,
                    ui_height - f64::from(pointer.y) / f64::from(window_height) * ui_height,
                );
                if self
                    .glue
                    .pointer_motion_deferred_refresh(position)?
                    .is_some()
                {
                    self.glue_ui_dirty = true;
                }
            }
            PlatformEvent::MouseWheel(wheel)
                if wheel.window_id == self.platform.window_id()
                    && selected_screen_is_presented
                    && self.glue.media_intent().movie().is_none() =>
            {
                let delta = match wheel.direction {
                    MouseWheelDirection::Normal => wheel.y,
                    MouseWheelDirection::Flipped => -wheel.y,
                    MouseWheelDirection::Unknown => return Ok(()),
                };
                let Some(pointer) = self.input.pointer_position() else {
                    return Ok(());
                };
                let (window_width, window_height) = self.platform.logical_extent();
                let (ui_width, ui_height) = self.glue.geometry().ui_extent();
                let position = (
                    f64::from(pointer.x()) / f64::from(window_width) * ui_width,
                    ui_height - f64::from(pointer.y()) / f64::from(window_height) * ui_height,
                );
                if self
                    .glue
                    .pointer_wheel(position, f64::from(delta))?
                    .is_some()
                {
                    self.glue_ui_dirty = true;
                }
            }
            _ => {}
        }
        self.sync_platform_text_input();
        Ok(())
    }

    /// Routes one admitted platform event to the active FrameXML owner.
    fn service_world_platform_event(
        &mut self,
        event: &PlatformEvent,
        timestamp_ms: u32,
    ) -> Result<(), ApplicationError> {
        let window_id = self.platform.window_id();
        let logical_extent = self.platform.logical_extent();
        let pointer_position = self.input.pointer_position();
        let mouse_free_look = self.player_movement.mouse_free_look();
        let Some(world_ui) = self.world_ui.as_mut() else {
            return Ok(());
        };
        let ui_extent = world_ui.logical_extent();
        let project_pointer = |x: f32, y: f32| {
            (
                f64::from(x) / f64::from(logical_extent.0) * f64::from(ui_extent[0]),
                f64::from(ui_extent[1])
                    - f64::from(y) / f64::from(logical_extent.1) * f64::from(ui_extent[1]),
            )
        };
        let mut captured = false;
        let ui_result = (|| -> Result<(), ApplicationError> {
            match event {
                PlatformEvent::Key(key_event) if key_event.window_id == window_id => {
                    captured = world_ui.has_focused_edit_box();
                    let Some(key) = key_event.scan_code.and_then(stock_keyboard_name) else {
                        return Ok(());
                    };
                    let modifiers = UiKeyboardModifiers::new(
                        key_event.modifiers.has_shift(),
                        key_event.modifiers.has_control(),
                        key_event.modifiers.has_alt(),
                    );
                    captured |= world_ui
                        .keyboard_key(key, key_event.state == ButtonState::Pressed, modifiers)?
                        .is_some();
                }
                PlatformEvent::TextInput(input) if input.window_id == window_id => {
                    world_ui.text_input(&input.text)?;
                }
                PlatformEvent::TextEditing(composition) if composition.window_id == window_id => {
                    world_ui.text_composition(&composition.text)?;
                }
                PlatformEvent::MouseButton(pointer) if pointer.window_id == window_id => {
                    let Some(button) = glue_pointer_button(pointer.button) else {
                        return Ok(());
                    };
                    captured = world_ui
                        .pointer_button(
                            project_pointer(pointer.x, pointer.y),
                            button,
                            pointer.state == ButtonState::Pressed,
                            pointer.click_count,
                            UiKeyboardModifiers::new(
                                self.input.modifiers().has_shift(),
                                self.input.modifiers().has_control(),
                                self.input.modifiers().has_alt(),
                            ),
                        )?
                        .object_index()
                        .is_some();
                }
                PlatformEvent::MouseMotion(pointer)
                    if pointer.window_id == window_id && !mouse_free_look =>
                {
                    world_ui.pointer_motion(project_pointer(pointer.x, pointer.y))?;
                }
                PlatformEvent::MouseWheel(wheel) if wheel.window_id == window_id => {
                    let delta = match wheel.direction {
                        MouseWheelDirection::Normal => wheel.y,
                        MouseWheelDirection::Flipped => -wheel.y,
                        MouseWheelDirection::Unknown => return Ok(()),
                    };
                    if let Some(pointer) = pointer_position {
                        captured = world_ui
                            .pointer_wheel(
                                project_pointer(pointer.x(), pointer.y()),
                                f64::from(delta),
                            )?
                            .is_some();
                    }
                }
                _ => {}
            }
            Ok(())
        })();
        let binding_result = world_ui.route_binding(
            event,
            self.input.modifiers(),
            captured || ui_result.is_err(),
        );
        // Preserve the Lua edge / relative motion / Lua release order even
        // when all three arrive in a single platform pump.
        while let Some(command) = world_ui.take_movement_command() {
            self.player_movement.push(command);
        }
        if let PlatformEvent::MouseMotion(pointer) = event
            && pointer.window_id == window_id
        {
            let scalar = |name: &str, default: f32| world_ui.cvar_number(name).unwrap_or(default);
            let settings = super::player_camera::PlayerCameraMouseSettings {
                yaw_speed: scalar("camerayawmovespeed", 180.),
                pitch_speed: scalar("camerapitchmovespeed", 90.),
                invert_yaw: scalar("mouseinvertyaw", 0.) != 0.,
                invert_pitch: scalar("mouseinvertpitch", 0.) != 0.,
            };
            self.player_movement.push_mouse_motion(
                [pointer.delta_x, pointer.delta_y],
                settings,
                timestamp_ms,
            );
        }
        self.platform
            .set_text_input_active(world_ui.has_focused_edit_box());
        ui_result.and(binding_result)
    }

    /// Takes one process-level action emitted by the currently owned built-in UI.
    pub(crate) fn take_process_action(&mut self) -> Option<UiProcessAction> {
        if let Some(action) = self
            .world_ui
            .as_ref()
            .and_then(RuntimeWorldUi::take_process_action)
        {
            return Some(action);
        }
        let action = self.glue.take_process_action()?;
        let current_screen = self.glue.current_screen();
        if glue_screen_is_presented(&current_screen, self.presented_glue_screen.as_deref()) {
            Some(action)
        } else {
            tracing::warn!(
                ?action,
                current_screen,
                presented_screen = self.presented_glue_screen.as_deref(),
                "ignored Glue process action from an unpresented screen"
            );
            None
        }
    }

    /// Presents one Glue or resident-world frame under the active VSync policy.
    pub(crate) fn present_frame(&mut self) -> Result<(), ApplicationError> {
        let mut profile = RuntimeFrameProfile::new("application present");
        let update_time = std::time::Instant::now();
        self.sound.apply_focus_policy(
            self.world_ui.as_ref().map_or(
                &self.glue as &dyn super::sound_coordinator::SoundCvarSource,
                |ui| ui,
            ),
            self.input.is_focused(),
        )?;
        if self.platform.presentation_suspended() {
            if self.gameplay.world().is_none() {
                self.sound.synchronize_glue_media(
                    &self.glue,
                    &mut self.blizzard_rand.borrow_mut(),
                    &self.cpu,
                )?;
            } else {
                self.sound.poll_loads(&self.cpu)?;
                if let Some(clock) = self.gameplay.realm_clock() {
                    self.sound.update_suspended_world(
                        self.world_ui.as_ref().map_or(
                            &self.glue as &dyn super::sound_coordinator::SoundCvarSource,
                            |ui| ui,
                        ),
                        clock,
                        &mut self.blizzard_rand.borrow_mut(),
                    )?;
                }
            }
            // A minimized Vulkan surface cannot pace the main loop reliably,
            // so keep animation time bounded and yield briefly while the event
            // pump remains responsive to restoration.
            self.glue_update_clock = update_time;
            std::thread::sleep(std::time::Duration::from_millis(16));
            return Ok(());
        }
        self.poll_glue_texture_prewarm();
        self.service_glue_gpu_texture_prewarm()?;
        profile.mark("texture prewarm");
        let developer_elapsed = update_time
            .duration_since(self.glue_update_clock)
            .as_secs_f32();
        self.developer_console
            .prepare_frame(&mut self.renderer, developer_elapsed)?;
        self.refresh_runtime_overlay_draws();
        profile.mark("developer overlay");
        if self.gameplay.world().is_none() && self.glue.flush_deferred_refresh()? {
            self.glue_ui_dirty = true;
        }
        if self.gameplay.world().is_none() {
            self.sound.synchronize_glue_media(
                &self.glue,
                &mut self.blizzard_rand.borrow_mut(),
                &self.cpu,
            )?;
        }
        profile.mark("deferred UI and audio");
        if self
            .loading_screen
            .as_ref()
            .is_some_and(RuntimeLoadingScreen::ready_to_complete)
        {
            self.loading_screen = None;
        } else if let Some(loading) = self.loading_screen.as_mut() {
            self.glue_update_clock = update_time;
            loading.present(&mut self.renderer, &self.runtime_overlay_draws)?;
            if let Some(fps) = self.fps.as_mut() {
                fps.record_presented(&mut self.renderer, std::time::Instant::now())?;
            }
            return Ok(());
        }
        if self.world_transfer.is_entering_world() {
            return Ok(());
        }
        if self.gameplay.world().is_none() {
            let glue_elapsed = update_time
                .duration_since(self.glue_update_clock)
                .as_secs_f64();
            self.glue_update_clock = update_time;
            if self.glue.update(glue_elapsed)? {
                self.glue_ui_dirty = true;
            }
            profile.mark("Glue update");
            while let Some(message) = self.glue.take_update_failure() {
                tracing::error!(error = %message, "contained failing GlueXML OnUpdate handler");
                self.developer_console.record_error(&message);
            }
            let current_screen = self.glue.current_screen();
            if self.glue_ui_dirty
                && glue_screen_is_presented(&current_screen, self.presented_glue_screen.as_deref())
                && let Some(frame) = self.login_ui.as_mut()
            {
                frame.refresh_glue(
                    &mut self.renderer,
                    &self.glue,
                    &mut self.ui_textures,
                    &mut self.ui_texture_residency,
                )?;
                self.glue_ui_dirty = false;
            }
            self.sync_platform_text_input();
            profile.mark("UI upload and text input");
            self.persist_active_cvars()?;
            profile.mark("CVar persistence");
            let movie = self.glue.media_intent().movie().cloned();
            if movie.is_some() {
                self.glue_model.hide();
            }
            let cinematic_overlay = (!self.runtime_overlay_draws.is_empty()).then_some((
                self.developer_console.logical_extent(),
                self.runtime_overlay_draws.as_slice(),
            ));
            match self.cinematic.synchronize(
                movie.as_ref(),
                &mut self.renderer,
                &mut self.sound,
                cinematic_overlay,
            )? {
                RuntimeCinematicPoll::Presented => {
                    if let Some(fps) = self.fps.as_mut() {
                        fps.record_presented(&mut self.renderer, std::time::Instant::now())?;
                    }
                    return Ok(());
                }
                RuntimeCinematicPoll::Waiting { remaining } => {
                    // Keep input latency bounded while avoiding duplicate swapchain
                    // presents between authored movie frames. The audio device is
                    // the stock master clock, so the next pass recomputes the exact
                    // remaining interval instead of accumulating sleep error.
                    std::thread::sleep(
                        remaining
                            .max(std::time::Duration::from_millis(1))
                            .min(std::time::Duration::from_millis(8)),
                    );
                    return Ok(());
                }
                RuntimeCinematicPoll::Finished { object_index } => {
                    self.glue.movie_finished(object_index)?;
                    self.login_ui = None;
                    return self.present_glue_frame();
                }
                RuntimeCinematicPoll::Stopped => {
                    return self.present_glue_frame();
                }
                RuntimeCinematicPoll::Idle => {}
            }
            profile.mark("cinematic service");
        } else {
            // The stock client transfers event and update ownership from GlueXML
            // to FrameXML after world entry. Keeping Glue alive for disconnect
            // does not permit its hidden edit boxes or buttons to remain active.
            let ui_elapsed = update_time
                .duration_since(self.glue_update_clock)
                .as_secs_f64();
            self.glue_update_clock = update_time;
            if let Some(world_ui) = self.world_ui.as_mut() {
                world_ui.update(ui_elapsed)?;
                while let Some(message) = world_ui.take_update_failure() {
                    tracing::error!(error = %message, "contained failing FrameXML OnUpdate handler");
                    self.developer_console.record_error(&message);
                }
                if let (Some(terrain), Some(player)) = (
                    self.terrain_frame.as_ref(),
                    self.player.resident_frame_input(),
                ) {
                    world_ui.synchronize_portrait(&mut self.renderer, terrain, &player)?;
                }
                world_ui.refresh(&mut self.renderer)?;
                world_ui.synchronize_minimap(
                    &mut self.renderer,
                    &self.cpu,
                    self.terrain.active_map(),
                    self.player
                        .resident_frame_input()
                        .map(|player| player.world_transform()),
                )?;
                self.platform
                    .set_text_input_active(world_ui.has_focused_edit_box());
            } else {
                self.platform.set_text_input_active(false);
            }
            self.persist_active_cvars()?;
            profile.mark("world FrameXML update and upload");
        }
        let Some(environment) = self.environment.current() else {
            return self.present_glue_frame();
        };
        let Some(camera) = self.resolved_world_camera()? else {
            return self.present_glue_frame();
        };
        profile.mark("world camera");
        if let Some(ui) = &self.world_ui {
            while let Some(action) = ui.take_media_action() {
                if let Err(error) = self.sound.apply_glue_media_action(
                    action,
                    &mut self.blizzard_rand.borrow_mut(),
                    ui,
                ) {
                    tracing::warn!(%error, "FrameXML audio action was not played");
                }
            }
        }
        let location = self.current_world_location()?;
        let underwater = self
            .terrain
            .camera_submerged_liquid(camera.camera().position(), &self.liquids)
            .map_err(super::sound_coordinator::RuntimeSoundError::from)?;
        self.sound.stage_zone(
            location,
            self.gameplay.world(),
            underwater.map_or(0, |liquid| liquid.liquid_type),
        );
        // Retire completed fades before zone selection can consider revival.
        self.sound.poll_loads(&self.cpu)?;
        if let Some(clock) = self.gameplay.realm_clock() {
            self.sound.update(
                self.world_ui.as_ref().map_or(
                    &self.glue as &dyn super::sound_coordinator::SoundCvarSource,
                    |ui| ui as &dyn super::sound_coordinator::SoundCvarSource,
                ),
                clock,
                camera,
                self.gameplay
                    .world()
                    .map(|world| {
                        world
                            .local_player_transform()
                            .map(|transform| transform.position())
                    })
                    .transpose()
                    .map_err(super::gameplay_coordinator::RuntimeGameplayError::from)?,
                &mut self.blizzard_rand.borrow_mut(),
            )?;
        }
        let plan = self.terrain.resident_mesh_plan();
        let global_animation_time_ms = self.m2_global_clock.elapsed().as_secs_f32() * 1_000.0;
        let specular_enabled = self.glue.cvar_boolean("specular");
        let Some(frame) = self.terrain_frame.as_mut() else {
            return self.present_glue_frame();
        };
        let player = self
            .player
            .resident_frame_input()
            .ok_or(RuntimeTerrainFrameError::MissingPlayerM2FrameInput)?;
        let creatures = self.player.resident_creature_frame_inputs();
        let remote_players = self.player.resident_remote_player_frame_inputs();
        let ui_extent = self.world_ui.as_ref().map_or_else(
            || self.developer_console.logical_extent(),
            RuntimeWorldUi::logical_extent,
        );
        let frame_draws = self
            .world_ui
            .as_ref()
            .map_or(&[][..], RuntimeWorldUi::draws);
        profile.mark("world sound and render inputs");
        frame.present(
            &mut self.renderer,
            plan,
            environment,
            camera,
            global_animation_time_ms,
            specular_enabled,
            &mut self.crt_rand,
            player,
            &creatures,
            &remote_players,
            self.game_objects.frame_input(self.gameplay.world()),
            ui_extent,
            frame_draws,
            &self.runtime_overlay_draws,
        )?;
        profile.mark("world animation and Vulkan present");
        let mount_camera_sample = frame.take_mount_camera_sample();
        let camera_time_ms = mount_camera_sample
            .map_or_else(|| frame.m2_animation_time_ms(), |sample| sample.time_ms());
        let mount_camera = mount_camera_sample.map(|sample| {
            MountCameraGeometry::new(sample.animated_height(), sample.fixed_height())
        });
        self.player
            .apply_mount_camera_sample(mount_camera, camera_time_ms)?;
        let m2_events = frame.drain_m2_events();
        let frame_errors = frame.drain_recoverable_errors();
        if let Some(world) = self.gameplay.world() {
            let (creatures, items) = self.player.sound_catalogs();
            let context = super::sound_coordinator::UnitSoundContext {
                world,
                creatures,
                items,
                cvars: self.world_ui.as_ref().map_or(
                    &self.glue as &dyn super::sound_coordinator::SoundCvarSource,
                    |ui| ui as &dyn super::sound_coordinator::SoundCvarSource,
                ),
            };
            self.sound.play_unit_events(
                &m2_events,
                camera,
                context,
                |guid, foot, sounds| {
                    let Some(transform) = world.object_transform(guid) else {
                        return Ok((u32::MAX, false));
                    };
                    let position = transform.position();
                    let ground = self.terrain.unit_ground_sound_type(position, sounds)?;
                    let wet = self.terrain.unit_wet_footstep(
                        position,
                        foot.z,
                        world
                            .movement_state(guid)
                            .map_or(0, |state| state.flags() as u32),
                        sounds,
                    )?;
                    Ok((ground, wet))
                },
                &mut self.blizzard_rand.borrow_mut(),
            )?;
        }
        self.sound
            .play_m2_events(&m2_events, camera, &mut self.blizzard_rand.borrow_mut())?;
        for message in frame_errors {
            tracing::error!(error = %message, "contained recoverable M2 presentation error");
            self.developer_console.record_error(&message);
        }
        profile.mark("world animation events");
        if let Some(fps) = self.fps.as_mut() {
            fps.record_presented(&mut self.renderer, std::time::Instant::now())?;
        }
        Ok(())
    }

    /// Adopts a completed speculative Glue source cache without waiting.
    fn poll_glue_texture_prewarm(&mut self) {
        let Some(pending) = self.pending_glue_texture_prewarm.take() else {
            return;
        };
        let pending = match pending {
            ConfiguredGlueTexturePrewarmJob::Deferred { catalog, paths } => {
                match self.cpu.can_admit_speculative() {
                    Ok(false) => {
                        self.pending_glue_texture_prewarm =
                            Some(ConfiguredGlueTexturePrewarmJob::Deferred { catalog, paths });
                        return;
                    }
                    Err(error) => {
                        let message = error.to_string();
                        tracing::warn!(error = %message, "could not inspect Glue texture prewarm capacity");
                        self.developer_console.record_error(&message);
                        return;
                    }
                    Ok(true) => {}
                }
                match self
                    .cpu
                    .try_submit(move || load_configured_glue_textures(catalog, paths))
                {
                    Ok(task) => {
                        self.pending_glue_texture_prewarm =
                            Some(ConfiguredGlueTexturePrewarmJob::Running(task));
                        return;
                    }
                    Err(error) => {
                        let message = error.to_string();
                        tracing::warn!(error = %message, "could not submit deferred Glue texture prewarm");
                        self.developer_console.record_error(&message);
                        return;
                    }
                }
            }
            ConfiguredGlueTexturePrewarmJob::Running(task) if !task.is_finished() => {
                self.pending_glue_texture_prewarm =
                    Some(ConfiguredGlueTexturePrewarmJob::Running(task));
                return;
            }
            ConfiguredGlueTexturePrewarmJob::Running(task) => task,
        };
        match pending.join() {
            Ok(Ok(prepared)) => {
                let admitted = self.ui_textures.merge(prepared.cache);
                self.glue_gpu_texture_prewarm_pending |= admitted != 0;
                tracing::info!(
                    admitted_texture_count = admitted,
                    resident_texture_count = self.ui_textures.len(),
                    failed_texture_count = prepared.failures.len(),
                    "adopted configured Glue texture prewarm"
                );
                for failure in prepared.failures {
                    tracing::warn!(error = %failure, "configured Glue texture source failed to prewarm");
                    self.developer_console.record_error(&failure);
                }
            }
            Ok(Err(error)) => {
                let message = error.to_string();
                tracing::warn!(error = %message, "configured Glue texture prewarm failed");
                self.developer_console.record_error(&message);
            }
            Err(error) => {
                let message = error.to_string();
                tracing::warn!(error = %message, "configured Glue texture worker failed");
                self.developer_console.record_error(&message);
            }
        }
    }

    /// Publishes worker-decoded Glue images only while an authored cover is visible.
    fn service_glue_gpu_texture_prewarm(&mut self) -> Result<(), ApplicationError> {
        let covered =
            self.authentication_prewarm_active || self.glue.media_intent().movie().is_some();
        if !covered || !self.glue_gpu_texture_prewarm_pending {
            return Ok(());
        }
        let uploaded = self
            .ui_texture_residency
            .prewarm(&mut self.renderer, &self.ui_textures)?;
        self.glue_gpu_texture_prewarm_pending = false;
        tracing::info!(
            uploaded_texture_count = uploaded,
            "published configured Glue textures behind transition cover"
        );
        Ok(())
    }

    fn present_glue_frame(&mut self) -> Result<(), ApplicationError> {
        let mut profile = RuntimeFrameProfile::new("Glue present");
        self.synchronize_component_texture_level();
        let current_screen = self.glue.current_screen();
        let screen_transition =
            !glue_screen_is_presented(&current_screen, self.presented_glue_screen.as_deref());
        if screen_transition {
            if let Some((candidate_screen, frame)) = self.pending_login_ui.as_mut() {
                if !candidate_screen.eq_ignore_ascii_case(&current_screen) || self.glue_ui_dirty {
                    frame.refresh_glue(
                        &mut self.renderer,
                        &self.glue,
                        &mut self.ui_textures,
                        &mut self.ui_texture_residency,
                    )?;
                    candidate_screen.clone_from(&current_screen);
                    self.glue_ui_dirty = false;
                }
            } else {
                let frame = RuntimeUiFrame::prepare_glue(
                    &mut self.renderer,
                    &self.glue,
                    &mut self.ui_textures,
                    &mut self.ui_texture_residency,
                )?;
                self.pending_login_ui = Some((current_screen.clone(), frame));
                self.glue_ui_dirty = false;
            }
        }
        if self.login_ui.is_none() && !screen_transition {
            self.login_ui = Some(RuntimeUiFrame::prepare_glue(
                &mut self.renderer,
                &self.glue,
                &mut self.ui_textures,
                &mut self.ui_texture_residency,
            )?);
            self.glue_ui_dirty = false;
        }
        profile.mark("UI screen preparation");
        let (glue_character_result, glue_character_expected) = match current_screen.as_str() {
            "charcreate" => {
                let preview = self.glue.character_creation_preview();
                let expected = preview.is_some();
                (
                    self.player
                        .synchronize_character_creation_async(preview.as_ref(), &self.cpu),
                    expected,
                )
            }
            "charselect" => {
                let preview = self.glue.character_selection_preview();
                let expected = preview.is_some();
                (
                    self.player
                        .synchronize_character_selection_async(preview.as_ref(), &self.cpu),
                    expected,
                )
            }
            _ => (
                self.player
                    .synchronize_character_creation_async(None, &self.cpu),
                false,
            ),
        };
        let (glue_character_changed, mut glue_character_expected) = match glue_character_result {
            Ok(changed) => (changed, glue_character_expected),
            Err(error) => {
                let message = error.to_string();
                tracing::error!(error = %message, "contained Glue character preparation error");
                self.developer_console.record_error(&message);
                (false, false)
            }
        };
        glue_character_expected &= !self.player.glue_character_request_failed();
        let glue_character = self.player.glue_character_frame_input();
        profile.mark("character synchronization");
        let model_poll = self.glue_model.synchronize(
            &mut self.renderer,
            &self.glue,
            &self.cpu,
            &mut self.crt_rand,
            Arc::clone(&self.particle_twinkle),
            glue_character,
            glue_character_changed,
            glue_character_expected,
        )?;
        profile.mark("model synchronization");
        if screen_transition && model_poll == RuntimeGlueModelPoll::Ready {
            let (candidate_screen, candidate) =
                self.pending_login_ui
                    .take()
                    .ok_or_else(|| ApplicationError::NetworkRuntime {
                        message: "Glue scene became ready without a prepared UI candidate"
                            .to_owned(),
                    })?;
            if !candidate_screen.eq_ignore_ascii_case(&current_screen) {
                return Err(ApplicationError::NetworkRuntime {
                    message: "Glue UI candidate does not match the ready scene".to_owned(),
                });
            }
            let retired = self.login_ui.replace(candidate);
            self.pending_login_ui = retired.map(|frame| {
                (
                    self.presented_glue_screen.clone().unwrap_or_default(),
                    frame,
                )
            });
        }
        let frame = self
            .login_ui
            .as_ref()
            .or_else(|| self.pending_login_ui.as_ref().map(|(_screen, frame)| frame))
            .ok_or_else(|| ApplicationError::NetworkRuntime {
                message: "Glue frame preparation produced no presentation state".to_owned(),
            })?;
        let global_time_ms = self.m2_global_clock.elapsed().as_secs_f32() * 1_000.0;
        let model_presented = self.glue_model.present(
            &mut self.renderer,
            &self.glue,
            frame,
            global_time_ms,
            &mut self.crt_rand,
            &self.runtime_overlay_draws,
        )?;
        if !model_presented {
            frame.present_with_overlay(&mut self.renderer, &self.runtime_overlay_draws)?;
        }
        profile.mark("model and UI present");
        if let Some((camera, events)) = self.glue_model.drain_sound_events() {
            self.sound
                .play_m2_events(&events, camera, &mut self.blizzard_rand.borrow_mut())?;
        }
        profile.mark("model sound events");
        if let Some(fps) = self.fps.as_mut() {
            fps.record_presented(&mut self.renderer, std::time::Instant::now())?;
        }
        if model_poll == RuntimeGlueModelPoll::Ready {
            self.presented_glue_screen = Some(current_screen);
        }
        self.last_glue_model_poll = model_poll;
        Ok(())
    }

    /// Applies ordered Glue actions and polls one asynchronous login result.
    pub(crate) fn service_login(&mut self) -> Result<(), ApplicationError> {
        let mut profile = RuntimeFrameProfile::new("session and world service");
        let Some(network) = self.network.as_ref() else {
            return Ok(());
        };
        let handle = network.handle().clone();
        let mut character_screen_requests = RuntimeCharacterScreenRequests::default();
        let mut authentication_started = false;
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
                            self.authentication_prewarm_active = true;
                            authentication_started = true;
                            let message = self
                                .glue
                                .localized_text("CSTATUS_CONNECTING")
                                .map_err(GlueError::from)?;
                            self.glue.dispatch_event(
                                "OPEN_STATUS_DIALOG",
                                &UiEventPayload::new([
                                    UiEventArgument::String("CANCEL".to_owned()),
                                    UiEventArgument::String(message),
                                ]),
                            )?;
                            self.glue_ui_dirty = true;
                        }
                        Err(RuntimeLoginError::AlreadyActive)
                        | Err(RuntimeLoginError::AlreadyAuthenticated) => {}
                        Err(error) => self.publish_login_failure(error)?,
                    }
                }
                UiGlueNetworkAction::CancelLogin => {
                    self.login.cancel();
                    self.authentication_prewarm_active = false;
                    self.realm_directory_published = false;
                    self.glue.set_network_status(UiGlueNetworkStatus::default());
                }
                UiGlueNetworkAction::StatusDialogClick => {
                    // Stock's raw `StatusDialogClick` thunk at `0x004DC500`
                    // reaches the general `0x004D98D0` dispatcher. Its create
                    // and character-entry arms publish local cancellation but
                    // send no world opcode, so the worker must finish any
                    // partially consumed encrypted frame before reuse.
                    let character_operation_cancelled = self.world.cancel_character_operation();
                    if character_operation_cancelled {
                        self.loading_screen = None;
                    } else if !self.login.cancel_realm_refresh() && self.login.cancel() {
                        self.realm_directory_published = false;
                        self.glue.set_network_status(UiGlueNetworkStatus::default());
                    }
                }
                UiGlueNetworkAction::Disconnect => {
                    self.login.disconnect();
                    self.world.disconnect();
                    self.gameplay.disconnect();
                    self.world_transfer.disconnect();
                    self.environment.disconnect();
                    self.player.disconnect();
                    self.game_objects.disconnect();
                    self.terrain.disconnect();
                    self.sound.disconnect()?;
                    if let Some(frame) = self.terrain_frame.take() {
                        frame.retire(&mut self.renderer)?;
                    }
                    self.loading_screen_cache.clear();
                    self.loading_screen_prewarm_queue.clear();
                    self.authentication_prewarm_active = false;
                    self.realm_directory_published = false;
                    self.character_screen_published = false;
                    self.character_directory_published = false;
                    self.pending_character_screen_requests =
                        RuntimeCharacterScreenRequests::default();
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
                        ]);
                        self.glue.dispatch_event("OPEN_STATUS_DIALOG", &payload)?;
                    }
                    match self.login.refresh_realms(&handle) {
                        Ok(()) | Err(RuntimeLoginError::AlreadyActive) => {}
                        Err(RuntimeLoginError::NotAuthenticated) => {}
                        Err(error) => self.publish_login_failure(error)?,
                    }
                }
                UiGlueNetworkAction::CancelRealmListQuery => {
                    self.login.cancel_realm_refresh();
                }
                UiGlueNetworkAction::ChangeRealm { realm_id } => {
                    self.pending_realm_id = Some(realm_id);
                    self.begin_pending_world(&handle)?;
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
                UiGlueNetworkAction::SortRealms { sort } => {
                    self.glue.sort_realm_directory(sort);
                    self.glue
                        .dispatch_event("OPEN_REALM_LIST", &UiEventPayload::empty())?;
                    self.glue_ui_dirty = true;
                }
                UiGlueNetworkAction::RealmListDialogCancelled { from_login_screen } => {
                    if from_login_screen {
                        self.login.disconnect();
                        self.realm_directory_published = false;
                        self.glue.set_network_status(UiGlueNetworkStatus::default());
                    }
                }
                UiGlueNetworkAction::ReadyForAccountDataTimes => {
                    character_screen_requests.ready_for_account_data_times = true;
                }
                UiGlueNetworkAction::RequestCharacterListUpdate => {
                    character_screen_requests.refresh_character_directory = true;
                }
                UiGlueNetworkAction::RequestRealmSplitInfo => {
                    character_screen_requests.request_realm_split_info = true;
                }
                UiGlueNetworkAction::CreateCharacter(request) => {
                    let creation = CharacterCreation::new(
                        request.name().to_owned(),
                        request.race_id(),
                        request.class_id(),
                        request.gender_id(),
                        request.appearance(),
                    )
                    .map_err(RuntimeWorldError::from)
                    .and_then(|creation| self.world.create_character(&handle, creation));
                    match creation {
                        Ok(()) => {
                            let message = self
                                .glue
                                .localized_text("CHAR_CREATE_IN_PROGRESS")
                                .map_err(GlueError::from)?;
                            self.glue.dispatch_event(
                                "OPEN_STATUS_DIALOG",
                                &UiEventPayload::new([
                                    UiEventArgument::String("CANCEL".to_owned()),
                                    UiEventArgument::String(message),
                                ]),
                            )?;
                        }
                        Err(RuntimeWorldError::AlreadyActive) => {}
                        Err(error) => self.publish_world_failure(error),
                    }
                }
                UiGlueNetworkAction::DeleteCharacter { guid } => {
                    match self.world.delete_character(&handle, guid) {
                        Ok(()) => {
                            let message = self
                                .glue
                                .localized_text("CHAR_DELETE_IN_PROGRESS")
                                .map_err(GlueError::from)?;
                            self.glue.dispatch_event(
                                "OPEN_STATUS_DIALOG",
                                &UiEventPayload::new([
                                    UiEventArgument::String("CANCEL".to_owned()),
                                    UiEventArgument::String(message),
                                ]),
                            )?;
                        }
                        Err(RuntimeWorldError::AlreadyActive) => {}
                        Err(error) => self.publish_world_failure(error),
                    }
                }
                UiGlueNetworkAction::RenameCharacter { guid, name } => {
                    let request = match CharacterRename::new(name) {
                        Ok(request) => request,
                        Err(CharacterRenameError::InvalidName { result }) => {
                            let message = self
                                .glue
                                .localized_text(result.message_token())
                                .map_err(GlueError::from)?;
                            self.glue.dispatch_event(
                                "OPEN_STATUS_DIALOG",
                                &UiEventPayload::new([
                                    UiEventArgument::String("OKAY".to_owned()),
                                    UiEventArgument::String(message),
                                ]),
                            )?;
                            continue;
                        }
                        Err(error) => {
                            self.publish_world_failure(RuntimeWorldError::from(error));
                            continue;
                        }
                    };
                    match self.world.rename_character(&handle, guid, request) {
                        Ok(()) => {
                            let message = self
                                .glue
                                .localized_text("CHAR_RENAME_IN_PROGRESS")
                                .map_err(GlueError::from)?;
                            self.glue.dispatch_event(
                                "OPEN_STATUS_DIALOG",
                                &UiEventPayload::new([
                                    UiEventArgument::String("CANCEL".to_owned()),
                                    UiEventArgument::String(message),
                                ]),
                            )?;
                        }
                        Err(RuntimeWorldError::AlreadyActive) => {}
                        Err(error) => self.publish_world_failure(error),
                    }
                }
                UiGlueNetworkAction::CharacterRenameValidationFailed { message_token } => {
                    let message = self
                        .glue
                        .localized_text(message_token)
                        .map_err(GlueError::from)?;
                    self.glue.dispatch_event(
                        "OPEN_STATUS_DIALOG",
                        &UiEventPayload::new([
                            UiEventArgument::String("OKAY".to_owned()),
                            UiEventArgument::String(message),
                        ]),
                    )?;
                }
                UiGlueNetworkAction::ForceCharacterRename { message_token } => {
                    self.glue.dispatch_event(
                        "FORCE_RENAME_CHARACTER",
                        &UiEventPayload::new([UiEventArgument::String(message_token.to_owned())]),
                    )?;
                }
                UiGlueNetworkAction::SelectCharacter { index } => {
                    let payload = UiEventPayload::new([UiEventArgument::Integer(i64::from(index))]);
                    self.glue
                        .dispatch_event("UPDATE_SELECTED_CHARACTER", &payload)?;
                }
                UiGlueNetworkAction::EnterWorld { guid } => {
                    self.world_ui = None;
                    let display_extent = self.platform.logical_extent();
                    let character_location = self
                        .world
                        .character_selection()
                        .and_then(|selection| selection.directory().by_guid(guid))
                        .map(|character| character.location());
                    let map_id = character_location.map(|location| location.map_id());
                    if map_id.is_none() {
                        tracing::warn!(
                            character_guid = format_args!("{guid:#018X}"),
                            "selected character has no directory map; using generic loading card"
                        );
                    }
                    if let Some(location) = character_location
                        && let Err(error) = self.terrain.prewarm_location(
                            location.map_id(),
                            location.x(),
                            location.y(),
                            &self.cpu,
                        )
                    {
                        // Character-directory coordinates are a performance
                        // hint only. World verification remains authoritative
                        // and will retry through the ordinary error path.
                        tracing::warn!(
                            character_guid = format_args!("{guid:#018X}"),
                            map_id = location.map_id(),
                            error = %error,
                            "could not start terrain preparation during world-entry handshake"
                        );
                    }
                    // World UI bootstrap intentionally waits for one loading
                    // present. Always retain a generic card when a malformed
                    // or racing Glue action omits directory metadata; leaving
                    // this as None strands an accepted world behind Glue.
                    let loading = map_id
                        .and_then(|map_id| {
                            self.loading_screen_cache.remove(&(map_id, display_extent))
                        })
                        .map_or_else(
                            || {
                                RuntimeLoadingScreen::prepare(
                                    &mut self.renderer,
                                    &self.assets,
                                    &mut self.ui_textures,
                                    &self.loading_directory,
                                    map_id,
                                    display_extent,
                                )
                            },
                            Ok,
                        )?;
                    match self.world.enter_world(&handle, guid) {
                        Ok(()) => self.loading_screen = Some(loading),
                        Err(RuntimeWorldError::AlreadyActive) => {}
                        Err(error) => self.publish_world_failure(error),
                    }
                }
            }
        }
        // One render plan and one Vulkan generation may cross their ownership
        // boundaries per frame, but Vulkan publication is presentation-thread
        // work. Keep those finite spikes behind an authored cover instead of
        // stealing 1-6 ms from otherwise idle EULA or AccountLogin frames.
        // The movie and authentication dialog are the two stock transitions
        // that can safely conceal this residency work.
        let backdrop_prewarm_window_active =
            self.authentication_prewarm_active || self.glue.media_intent().movie().is_some();
        let backdrop_prewarms_complete = if backdrop_prewarm_window_active {
            self.glue_model
                .service_backdrop_prewarms(&mut self.renderer, &self.cpu)?
        } else {
            false
        };
        if self.authentication_prewarm_active
            && !authentication_started
            && backdrop_prewarms_complete
        {
            self.authentication_prewarm_active = false;
        }
        if !character_screen_requests.is_empty() {
            self.pending_character_screen_requests
                .merge(character_screen_requests);
        }
        if !self.pending_character_screen_requests.is_empty() {
            let pending_requests = self.pending_character_screen_requests;
            match self
                .world
                .request_character_screen_data(&handle, pending_requests)
            {
                Ok(()) => {
                    self.pending_character_screen_requests =
                        RuntimeCharacterScreenRequests::default();
                    if pending_requests.refresh_character_directory {
                        self.character_directory_published = false;
                    }
                }
                Err(RuntimeWorldError::AlreadyActive | RuntimeWorldError::NoCharacterScreen) => {}
                Err(error) => self.publish_world_failure(error),
            }
        }

        match self.login.poll() {
            Ok(RuntimeLoginPoll::Idle | RuntimeLoginPoll::Pending) => {}
            Ok(RuntimeLoginPoll::Authenticated) if self.authentication_prewarm_active => {
                // Retain the status dialog until the finite racial backdrop set
                // is renderer-resident. Publishing the realm/character route
                // sooner merely trades a few covered frames for a visible black
                // or partially prepared CharacterSelect transition.
            }
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
                let payload = UiEventPayload::new([UiEventArgument::Integer(category_count)]);
                self.publish_realm_directory("GET_PREFERRED_REALM_INFO", payload)?;
                self.realm_directory_published = true;
            }
            Ok(RuntimeLoginPoll::Authenticated) => {}
            Ok(RuntimeLoginPoll::RealmDirectoryUpdated) => {
                self.glue
                    .dispatch_event("CLOSE_STATUS_DIALOG", &UiEventPayload::empty())?;
                self.publish_realm_directory("OPEN_REALM_LIST", UiEventPayload::empty())?;
                self.realm_directory_published = true;
                self.begin_pending_world(&handle)?;
            }
            Ok(RuntimeLoginPoll::RealmDirectoryCancelled) => {}
            Err(error) => self.publish_login_failure(error)?,
        }
        match self.world.poll() {
            Ok(RuntimeWorldPoll::Idle | RuntimeWorldPoll::Pending) => {}
            Ok(RuntimeWorldPoll::CharacterScreenReady) if !self.character_screen_published => {
                self.character_screen_published = true;
                tracing::info!("authenticated character screen became ready");
                if let Some(session) = self.world.authenticated() {
                    let expansion = match session.info().expansion() {
                        AccountExpansion::Original => UiCharacterExpansion::ORIGINAL,
                        AccountExpansion::TheBurningCrusade => {
                            UiCharacterExpansion::THE_BURNING_CRUSADE
                        }
                        AccountExpansion::WrathOfTheLichKing => {
                            UiCharacterExpansion::WRATH_OF_THE_LICH_KING
                        }
                    };
                    self.glue.set_character_creation_expansion(expansion);
                }
                if let Some(selected) = &self.selected_realm {
                    self.glue.set_network_status(selected.status(true, false));
                }
                self.glue.set_character_directory(Default::default());
                self.glue.dispatch_event(
                    "SET_GLUE_SCREEN",
                    &UiEventPayload::new([UiEventArgument::String("charselect".to_owned())]),
                )?;
                self.glue_ui_dirty = true;
            }
            Ok(RuntimeWorldPoll::CharacterScreenReady) => {}
            Ok(RuntimeWorldPoll::CharacterDirectoryReady)
                if !self.character_directory_published =>
            {
                self.character_directory_published = true;
                if let Some(selection) = self.world.character_selection() {
                    let characters = self.character_metadata.project(selection.directory())?;
                    let count = i64::try_from(characters.characters().len()).map_err(|source| {
                        ApplicationError::NetworkRuntime {
                            message: source.to_string(),
                        }
                    })?;
                    tracing::info!(character_count = count, "character directory became ready");
                    self.glue.set_character_directory(characters);
                    self.glue.dispatch_event(
                        "CHARACTER_LIST_UPDATE",
                        &UiEventPayload::new([UiEventArgument::Integer(count)]),
                    )?;
                    self.glue_ui_dirty = true;
                }
                self.queue_loading_screen_prewarms();
            }
            Ok(RuntimeWorldPoll::CharacterDirectoryReady) => {}
            Ok(RuntimeWorldPoll::CharacterCreationFinished(result)) => {
                if result.is_success() {
                    self.character_directory_published = false;
                    self.glue
                        .dispatch_event("CLOSE_STATUS_DIALOG", &UiEventPayload::empty())?;
                    self.glue
                        .dispatch_event("SELECT_LAST_CHARACTER", &UiEventPayload::empty())?;
                    self.glue.dispatch_event(
                        "SET_GLUE_SCREEN",
                        &UiEventPayload::new([UiEventArgument::String("charselect".to_owned())]),
                    )?;
                } else {
                    let message = self
                        .glue
                        .localized_text(result.message_token())
                        .map_err(GlueError::from)?;
                    self.glue.dispatch_event(
                        "OPEN_STATUS_DIALOG",
                        &UiEventPayload::new([
                            UiEventArgument::String("OKAY".to_owned()),
                            UiEventArgument::String(message),
                        ]),
                    )?;
                }
                self.glue_ui_dirty = true;
            }
            Ok(RuntimeWorldPoll::CharacterDeletionFinished(result)) => {
                if result.is_success() {
                    self.character_directory_published = false;
                    self.pending_character_screen_requests
                        .refresh_character_directory = true;
                    self.glue
                        .dispatch_event("CLOSE_STATUS_DIALOG", &UiEventPayload::empty())?;
                    self.glue
                        .dispatch_event("SELECT_FIRST_CHARACTER", &UiEventPayload::empty())?;
                } else {
                    let message = self
                        .glue
                        .localized_text(result.message_token())
                        .map_err(GlueError::from)?;
                    self.glue.dispatch_event(
                        "OPEN_STATUS_DIALOG",
                        &UiEventPayload::new([
                            UiEventArgument::String("OKAY".to_owned()),
                            UiEventArgument::String(message),
                        ]),
                    )?;
                }
                self.glue_ui_dirty = true;
            }
            Ok(RuntimeWorldPoll::CharacterRenameFinished(result)) => {
                if result.is_success() {
                    self.character_directory_published = false;
                    self.pending_character_screen_requests
                        .refresh_character_directory = true;
                    self.glue
                        .dispatch_event("CLOSE_STATUS_DIALOG", &UiEventPayload::empty())?;
                } else {
                    let message = self
                        .glue
                        .localized_text(result.message_token())
                        .map_err(GlueError::from)?;
                    self.glue.dispatch_event(
                        "OPEN_STATUS_DIALOG",
                        &UiEventPayload::new([
                            UiEventArgument::String("OKAY".to_owned()),
                            UiEventArgument::String(message),
                        ]),
                    )?;
                }
                self.glue_ui_dirty = true;
            }
            Ok(RuntimeWorldPoll::CharacterOperationCancelled) => {}
            Ok(RuntimeWorldPoll::EnteredWorld) => {
                if let Some(entry) = self.world.take_world_entry() {
                    let (session, setup_packets) = entry.into_parts();
                    self.gameplay.begin_with_game_objects(
                        &handle,
                        session,
                        setup_packets,
                        &mut |world, identity, notification| {
                            self.game_objects
                                .observe_notification(
                                    world,
                                    identity,
                                    notification,
                                    &mut self.crt_rand,
                                )
                                .map_err(Into::into)
                        },
                    )?;
                    tracing::info!("selected character entered the active world");
                }
            }
            Ok(RuntimeWorldPoll::CharacterRejected(rejection)) => {
                self.loading_screen = None;
                tracing::warn!(
                    reason_code = rejection.reason_code(),
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
                    self.character_directory_published = true;
                    self.glue.dispatch_event(
                        "CHARACTER_LIST_UPDATE",
                        &UiEventPayload::new([UiEventArgument::Integer(count)]),
                    )?;
                    self.glue_ui_dirty = true;
                }
                // Stock `0x006B2070` tears the optimistic world load back to
                // character selection, then `0x004DAB40` state 11 presents
                // the response-table token through an OKAY status dialog.
                let message = self
                    .glue
                    .localized_text(rejection.message_token())
                    .map_err(GlueError::from)?;
                self.glue.dispatch_event(
                    "OPEN_STATUS_DIALOG",
                    &UiEventPayload::new([
                        UiEventArgument::String("OKAY".to_owned()),
                        UiEventArgument::String(message),
                    ]),
                )?;
                self.glue_ui_dirty = true;
            }
            Err(error) => self.publish_world_failure(error),
        }
        self.service_loading_screen_prewarm()?;
        self.service_world_transfers()?;
        profile.mark("session actions and transfers");
        // Stock continues world/UI initialization while the loading card owns
        // presentation. Start immutable terrain generation first so FrameXML,
        // character, and transport preparation overlap its worker execution.
        let terrain_poll = self
            .terrain
            .synchronize_async(self.gameplay.world(), &self.cpu)?;
        profile.mark("terrain residency");
        // The coordinator has already committed this terrain generation.
        // Publish it before any recoverable UI or unit appearance failure can
        // return to the event loop and present against the new CPU scene.
        match terrain_poll {
            RuntimeTerrainPoll::TileLoaded { map_id, tile } => {
                let resident_tile = self.terrain.resident_tile().ok_or(
                    RuntimeTerrainFrameError::MissingMeshPlan {
                        tile_x: tile.x(),
                        tile_y: tile.y(),
                    },
                )?;
                self.sound.stage_terrain_tile(resident_tile);
                if let Some(frame) = self
                    .terrain_frame
                    .as_mut()
                    .filter(|frame| frame.belongs_to_map(map_id))
                {
                    frame.synchronize_tiles(
                        &mut self.renderer,
                        tile,
                        self.terrain.resident_tiles(),
                        &mut self.crt_rand,
                    )?;
                } else {
                    let plan = self
                        .terrain
                        .resident_tiles()
                        .find(|resident| resident.mesh().tile() == tile)
                        .map(|resident| resident.mesh())
                        .ok_or(RuntimeTerrainFrameError::MissingMeshPlan {
                            tile_x: tile.x(),
                            tile_y: tile.y(),
                        })?;
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
                        map_id,
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
                        self.game_objects.frame_input(self.gameplay.world()),
                    )?;
                    tracing::info!(
                        tile_x = tile.x(),
                        tile_y = tile.y(),
                        draw_count = frame.draw_count(),
                        m2_mesh_count = frame.m2_mesh_count(),
                        m2_placement_count = frame.m2_placement_count(),
                        world_model_placement_count = frame.world_model_placement_count(),
                        "resident terrain entered renderer resources"
                    );
                    if let Some(previous) = self.terrain_frame.replace(frame) {
                        previous.retire(&mut self.renderer)?;
                    }
                }
            }
            RuntimeTerrainPoll::GlobalWorldModelLoaded { map_id } => {
                // Global-WMO maps have no MCSE/MH2O tile generation. Release
                // any prior tiled-map audio before publishing their scene.
                self.sound.disconnect()?;
                let world_models = self
                    .terrain
                    .resident_world_models()
                    .ok_or(RuntimeTerrainFrameError::SceneKindMismatch)?;
                let m2_scene = self
                    .terrain
                    .resident_m2_scene()
                    .ok_or(RuntimeTerrainFrameError::SceneKindMismatch)?;
                let frame = TerrainFrame::prepare_global_world_model(
                    &mut self.renderer,
                    m2_scene,
                    world_models,
                    WorldModelTextureFiltering::Anisotropic4x,
                    WorldModelBaseMip::Zero,
                    &mut self.crt_rand,
                    Arc::clone(&self.particle_twinkle),
                    self.player.resident_frame_input(),
                    &self.player.resident_creature_frame_inputs(),
                    &self.player.resident_remote_player_frame_inputs(),
                    self.game_objects.frame_input(self.gameplay.world()),
                )?;
                tracing::info!(
                    map_id,
                    m2_mesh_count = frame.m2_mesh_count(),
                    m2_placement_count = frame.m2_placement_count(),
                    world_model_placement_count = frame.world_model_placement_count(),
                    "global WMO entered renderer resources"
                );
                if let Some(previous) = self.terrain_frame.replace(frame) {
                    previous.retire(&mut self.renderer)?;
                }
            }
            RuntimeTerrainPoll::Idle => {
                self.sound.disconnect()?;
                if let Some(frame) = self.terrain_frame.take() {
                    frame.retire(&mut self.renderer)?;
                }
            }
            RuntimeTerrainPoll::Pending { .. } => {}
            RuntimeTerrainPoll::Current { tile, .. } => {
                if self.terrain_frame.as_ref().and_then(TerrainFrame::tile) != Some(tile) {
                    return Err(RuntimeTerrainFrameError::MissingGpuGeneration {
                        tile_x: tile.x(),
                        tile_y: tile.y(),
                    }
                    .into());
                }
            }
            RuntimeTerrainPoll::GlobalWorldModelCurrent { .. } => {
                if self
                    .terrain_frame
                    .as_ref()
                    .is_none_or(|frame| frame.tile().is_some())
                {
                    return Err(RuntimeTerrainFrameError::SceneKindMismatch.into());
                }
            }
        }
        profile.mark("scene GPU publication");
        self.prepare_world_ui_if_ready()?;
        if let (Some(world_ui), Some(clock)) = (self.world_ui.as_mut(), self.gameplay.realm_clock())
        {
            world_ui.synchronize_realm_clock(clock)?;
        }
        if let (Some(world_ui), Some(buttons)) =
            (self.world_ui.as_mut(), self.gameplay.action_buttons())
        {
            world_ui.synchronize_action_buttons(buttons)?;
        }
        self.environment
            .synchronize(self.gameplay.world(), self.gameplay.realm_clock())?;
        self.synchronize_component_texture_level();
        profile.mark("world UI and environment");
        if let Some(ui) = &self.world_ui {
            while let Some(command) = ui.take_movement_command() {
                self.player_movement.push(command);
            }
        }
        while let Some(event) = self.gameplay.take_player_control_event() {
            self.player_movement.push_control(event);
            if let super::player_control::PlayerControlEvent::PlayerControl { enabled, .. } = event
                && let Some(ui) = self.world_ui.as_mut()
            {
                ui.player_control_changed(enabled)?;
                while let Some(command) = ui.take_movement_command() {
                    self.player_movement.push(command);
                }
            }
        }
        // Publish current map/object collision references before the movement
        // owner queries them, including the first admitted terrain generation.
        let previous_game_object_revision = self.game_objects.scene_revision();
        let transport_poll = self
            .game_objects
            .synchronize_async(self.gameplay.world(), &self.cpu)?;
        self.game_objects
            .synchronize_animations(self.gameplay.world(), &mut self.crt_rand)?;
        self.terrain.synchronize_game_object_movement(
            self.gameplay.world(),
            &self.game_objects,
            solarity_systems::MovementBspCacheMode::Enabled,
        )?;
        profile.mark("game object residency and collision registry");
        if let Some(ui) = &self.world_ui {
            self.player_movement
                .refresh_camera_settings(ui.cvar_revision(), |name| ui.cvar_number(name));
        }
        self.player_movement.service(
            &mut self.gameplay,
            &mut self.terrain,
            &self.game_objects,
            self.player.movement_dimensions(),
            crate::platform::client_milliseconds(),
        )?;
        profile.mark("player movement");
        self.remote_movement.service(
            &self.gameplay,
            &mut self.terrain,
            &self.game_objects,
            &self.player,
            crate::platform::client_milliseconds(),
        )?;
        while let Some(event) = self.remote_movement.take_animation_event() {
            self.sound.notify_unit_movement(event);
            self.player.notify_movement_animation(event);
        }
        while let Some(event) = self.player_movement.take_animation_event() {
            self.sound.notify_unit_movement(event);
            self.player.notify_movement_animation(event);
        }
        self.platform
            .set_mouse_free_look(self.player_movement.mouse_free_look())?;
        self.player.set_animation_mouse_turning(
            self.player_movement
                .animation_mouse_turning(self.gameplay.world()),
        );
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
        profile.mark("local player residency");
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
        profile.mark("creature and remote player residency");
        if self.game_objects.scene_revision() != previous_game_object_revision
            && let Some(frame) = self.terrain_frame.as_mut()
        {
            frame.synchronize_game_objects(
                &mut self.renderer,
                self.game_objects.frame_input(self.gameplay.world()),
                &mut self.crt_rand,
            )?;
        }
        if let RuntimeTransportPoll::ResourceLoaded { guid, kind }
        | RuntimeTransportPoll::PlacementChanged { guid, kind } = transport_poll
        {
            tracing::debug!(transport_guid = guid, resource_kind = ?kind,
                "updated local player transport readiness");
        }
        self.service_terrain_streaming()?;
        profile.mark("terrain streaming");
        self.synchronize_world_ui_zone()?;
        self.complete_world_transfer_map()?;
        if self.player.resident_frame_input().is_some()
            && self.terrain_frame.is_some()
            && self.environment.current().is_some()
            && self.game_objects.is_ready()
            && self.world_ui.is_some()
            && self.world_transfer.complete_player()
            && let (Some(ui), Some(active)) = (self.world_ui.as_mut(), self.gameplay.world())
        {
            ui.enter_replacement_world(&self.character_metadata, active)?;
        }
        if self.loading_screen.is_some()
            && let (Some(ui), Some(terrain), Some(player)) = (
                self.world_ui.as_mut(),
                self.terrain_frame.as_ref(),
                self.player.resident_frame_input(),
            )
        {
            ui.synchronize_portrait(&mut self.renderer, terrain, &player)?;
            ui.refresh(&mut self.renderer)?;
            ui.synchronize_minimap(
                &mut self.renderer,
                &self.cpu,
                self.terrain.active_map(),
                Some(player.world_transform()),
            )?;
        }
        if let Some(loading) = self.loading_screen.as_mut() {
            let readiness = RuntimeLoadingReadiness {
                world_accepted: self.gameplay.world().is_some()
                    && !self.world_transfer.is_awaiting_destination(),
                environment_ready: self.environment.current().is_some(),
                player_ready: self.player.resident_frame_input().is_some()
                    && self.player_movement.initial_contact_ready(),
                scene_ready: self.terrain_frame.is_some()
                    && !self.world_transfer.holds_loading_card(),
                ui_ready: self
                    .world_ui
                    .as_ref()
                    .is_some_and(RuntimeWorldUi::minimap_ready),
                transport_resource_ready: self.game_objects.is_ready(),
            };
            loading.advance(readiness);
        }
        Ok(())
    }

    /// Builds the independently retained FrameXML owner behind the loading card.
    fn prepare_world_ui_if_ready(&mut self) -> Result<(), ApplicationError> {
        if self.world_ui.is_some()
            || !self
                .loading_screen
                .as_ref()
                .is_some_and(RuntimeLoadingScreen::has_presented)
        {
            return Ok(());
        }
        let Some(active) = self.gameplay.world() else {
            return Ok(());
        };
        if !RuntimeCharacterMetadata::active_player_is_ready(active) {
            return Ok(());
        }
        let zone = self
            .character_metadata
            .zone_state(self.terrain.current_area_id(active)?)?;
        let general_tab_name = self
            .glue
            .localized_text("GENERAL")
            .map_err(GlueError::from)?;
        let (world_ui, startup_errors) = RuntimeWorldUi::prepare(
            &mut self.renderer,
            self.platform.window_id(),
            self.assets.clone(),
            self.world_ui_catalog.clone(),
            self.platform.logical_extent(),
            self.startup_profile.cvar_values(),
            &self.addon_catalog,
            &self.character_metadata,
            active,
            zone,
            self.gameplay.realm_clock(),
            self.gameplay.action_buttons(),
            general_tab_name,
            self.sound.output_names(),
        )?;
        for error in &startup_errors {
            let captured = self.record_recoverable_error(error);
            debug_assert!(captured, "FrameXML dispatch errors must remain recoverable");
        }
        tracing::info!(
            startup_error_count = startup_errors.len(),
            "loaded stock FrameXML and published world-entry events"
        );
        self.world_ui = Some(world_ui);
        self.publish_ui_modifier_keys();
        Ok(())
    }

    /// Joins the player's registered WMO with terrain through one metadata owner.
    fn current_world_location(
        &mut self,
    ) -> Result<super::character_directory::RuntimeWorldLocation, ApplicationError> {
        let Some(world) = self.gameplay.world() else {
            return Ok(Default::default());
        };
        let terrain_area = self.terrain.current_area_id(world)?;
        let position = world
            .local_player_transform()
            .map_err(super::gameplay_coordinator::RuntimeGameplayError::from)?
            .position();
        let world_model = self
            .terrain
            .unit_world_model_location(position)
            .map_err(super::sound_coordinator::RuntimeSoundError::from)?;
        let mut location = self
            .character_metadata
            .world_location(terrain_area, world_model)?;
        location.chunk_key =
            solarity_media::world_chunk_sound_key(world.map_id().value(), position.x, position.y);
        Ok(location)
    }

    /// Keeps FrameXML's area labels synchronized with the authoritative player
    /// position after world UI bootstrap.
    fn synchronize_world_ui_zone(&mut self) -> Result<(), ApplicationError> {
        let location = self.current_world_location()?;
        let Some(world_ui) = self.world_ui.as_mut() else {
            return Ok(());
        };
        world_ui.synchronize_zone(self.character_metadata.zone_state(location.area_id)?)
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
        self.persist_active_cvars()?;
        self.login.disconnect();
        self.world.disconnect();
        self.gameplay.disconnect();
        self.world_transfer.disconnect();
        self.environment.disconnect();
        self.player.disconnect();
        self.game_objects.disconnect();
        self.terrain.disconnect();
        self.sound.disconnect()?;
        self.terrain_frame = None;
        self.world_ui = None;
        self.loading_screen = None;
        let sound_result = self.sound.shutdown().map_err(ApplicationError::from);
        let renderer_result = self.renderer.shutdown().map_err(ApplicationError::from);
        let cpu_result = self.cpu.shutdown().map_err(ApplicationError::from);
        if let Some(network) = self.network.take() {
            network.shutdown_timeout(self.network_shutdown_timeout);
        }
        sound_result?;
        renderer_result?;
        cpu_result
    }

    fn persist_active_cvars(&mut self) -> Result<(), ApplicationError> {
        let mut changed = self.glue.take_changed_cvars();
        if let Some(world_ui) = self.world_ui.as_ref() {
            changed.extend(world_ui.take_changed_cvars());
        }
        self.startup_profile.persist_cvars(&changed)?;
        Ok(())
    }

    fn synchronize_component_texture_level(&mut self) {
        let value = self.world_ui.as_ref().map_or_else(
            || self.glue.cvar_value("componentTextureLevel"),
            |world_ui| world_ui.cvar_value("componentTextureLevel"),
        );
        let level = value
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite())
            .map_or(CharacterComponentTextureLevel::DEFAULT, |value| {
                CharacterComponentTextureLevel::clamped(value.round() as u32)
            });
        if self.player.set_component_texture_level(level) {
            tracing::info!(
                component_texture_level = level.value(),
                atlas_size = level.atlas_size(),
                "applied character component texture level"
            );
        }
    }

    /// Queues each distinct character map for hidden loading-card residency.
    fn queue_loading_screen_prewarms(&mut self) {
        let display_extent = self.platform.logical_extent();
        let map_ids = self
            .world
            .character_selection()
            .map(|selection| {
                selection
                    .directory()
                    .entries()
                    .iter()
                    .map(|character| character.location().map_id())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for map_id in map_ids {
            let key = (map_id, display_extent);
            if !self.loading_screen_cache.contains_key(&key)
                && !self.loading_screen_prewarm_queue.contains(&map_id)
            {
                self.loading_screen_prewarm_queue.push_back(map_id);
            }
        }
    }

    /// Makes one queued card renderer-resident behind an established scene.
    fn service_loading_screen_prewarm(&mut self) -> Result<(), ApplicationError> {
        if self.loading_screen.is_some()
            || self.gameplay.world().is_some()
            || !self
                .glue
                .current_screen()
                .eq_ignore_ascii_case("charselect")
            || !self
                .presented_glue_screen
                .as_deref()
                .is_some_and(|screen| screen.eq_ignore_ascii_case("charselect"))
        {
            return Ok(());
        }
        let display_extent = self.platform.logical_extent();
        while let Some(map_id) = self.loading_screen_prewarm_queue.pop_front() {
            let key = (map_id, display_extent);
            if self.loading_screen_cache.contains_key(&key) {
                continue;
            }
            let loading = RuntimeLoadingScreen::prepare(
                &mut self.renderer,
                &self.assets,
                &mut self.ui_textures,
                &self.loading_directory,
                Some(map_id),
                display_extent,
            )?;
            self.loading_screen_cache.insert(key, loading);
            tracing::info!(map_id, ?display_extent, "prewarmed resident loading card");
            break;
        }
        Ok(())
    }

    fn sync_platform_text_input(&mut self) {
        let current_screen = self.glue.current_screen();
        let glue_focus_is_visible =
            glue_screen_is_presented(&current_screen, self.presented_glue_screen.as_deref())
                && self.glue.focused_edit_box().is_some();
        self.platform
            .set_text_input_active(self.developer_console.is_visible() || glue_focus_is_visible);
    }

    /// Reuses one retained vector for the overlays shared by every presenter.
    fn refresh_runtime_overlay_draws(&mut self) {
        self.runtime_overlay_draws.clear();
        if self.glue.cvar_boolean("showfps")
            && let Some(fps) = self.fps.as_ref()
        {
            self.runtime_overlay_draws.extend_from_slice(fps.draws());
        }
        self.runtime_overlay_draws
            .extend_from_slice(self.developer_console.draws());
    }
}

/// Separates presentation defects that can leave the event loop interactive
/// from device, archive, worker, and protocol failures that invalidate owners.
fn is_recoverable_presentation_error(error: &ApplicationError) -> bool {
    match error {
        ApplicationError::Ui(_)
        | ApplicationError::UiRender(_)
        | ApplicationError::NativeText(_)
        | ApplicationError::UiEvent(_)
        | ApplicationError::WorldUi(_)
        | ApplicationError::Player(_) => true,
        ApplicationError::GlueModel(error) => !matches!(
            error,
            RuntimeGlueModelError::Asset(_)
                | RuntimeGlueModelError::Vulkan(_)
                | RuntimeGlueModelError::Cpu(_)
                | RuntimeGlueModelError::Frame(
                    RuntimeTerrainFrameError::TextureUpload(_)
                        | RuntimeTerrainFrameError::Vulkan(_)
                )
        ),
        _ => false,
    }
}

/// Returns whether the active Glue screen has reached the swapchain at least
/// once since it was selected. Case folding follows Glue's screen dispatch.
fn glue_screen_is_presented(current_screen: &str, presented_screen: Option<&str>) -> bool {
    presented_screen.is_some_and(|presented| presented.eq_ignore_ascii_case(current_screen))
}

/// Maps SDL's admitted desktop button vocabulary onto Glue script names.
const fn glue_pointer_button(button: MouseButton) -> Option<UiPointerButton> {
    match button {
        MouseButton::Left => Some(UiPointerButton::Left),
        MouseButton::Middle => Some(UiPointerButton::Middle),
        MouseButton::Right => Some(UiPointerButton::Right),
        MouseButton::AuxiliaryOne => Some(UiPointerButton::Button4),
        MouseButton::AuxiliaryTwo => Some(UiPointerButton::Button5),
        MouseButton::Unknown => None,
    }
}

impl ClientServices {
    /// Restores the stock login-screen error dialog while retaining the typed
    /// failure for embedding callers that consume the runtime diagnostic API.
    fn publish_login_failure(&mut self, error: RuntimeLoginError) -> Result<(), ApplicationError> {
        self.authentication_prewarm_active = false;
        self.realm_directory_published = false;
        self.glue.set_network_status(UiGlueNetworkStatus::default());
        tracing::warn!(error = %error, "login exchange failed");
        let message = self
            .glue
            .localized_text(error.message_token())
            .map_err(GlueError::from)?;
        self.glue.dispatch_event(
            "OPEN_STATUS_DIALOG",
            &UiEventPayload::new([
                UiEventArgument::String("OKAY".to_owned()),
                UiEventArgument::String(message),
            ]),
        )?;
        self.glue_ui_dirty = true;
        self.login_failures.push_back(error);
        Ok(())
    }

    fn publish_world_failure(&mut self, error: RuntimeWorldError) {
        self.loading_screen = None;
        self.world_ui = None;
        self.character_screen_published = false;
        self.character_directory_published = false;
        if let Some(selected) = &self.selected_realm {
            self.glue.set_network_status(selected.status(false, true));
        } else {
            self.glue.set_network_status(UiGlueNetworkStatus::default());
        }
        tracing::warn!(error = %error, "world authentication failed");
        self.world_failures.push_back(error);
    }

    /// Transfers the chosen realmd identity into world authentication and
    /// persists stock's last-realm label once that transfer is admitted.
    fn begin_pending_world(
        &mut self,
        runtime: &tokio::runtime::Handle,
    ) -> Result<(), ApplicationError> {
        let Some(realm_id) = self.pending_realm_id else {
            return Ok(());
        };
        let Ok(realm_id) = u8::try_from(realm_id) else {
            return Ok(());
        };
        let Some(realm) = self
            .login
            .authenticated()
            .and_then(|authenticated| authenticated.realms().by_id(realm_id))
            .cloned()
        else {
            return Ok(());
        };
        let Some(authenticated) = self.login.take_authenticated() else {
            return Ok(());
        };
        let selected = SelectedRealmFacts::new(&self.realm_metadata, &realm);
        match self
            .world
            .begin(runtime, authenticated, realm, self.addon_manifest.clone())
        {
            Ok(()) => {
                self.pending_realm_id = None;
                self.character_screen_published = false;
                self.character_directory_published = false;
                self.glue.set_network_status(selected.status(false, false));
                self.startup_profile
                    .persist_cvars(&[("realmName".to_owned(), selected.name.clone())])?;
                self.selected_realm = Some(selected);
            }
            Err(error) => self.publish_world_failure(error),
        }
        Ok(())
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
        self.glue_ui_dirty = true;
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
            ]);
            self.glue.dispatch_event("SUGGEST_REALM", &payload)?;
        } else {
            self.glue
                .dispatch_event("OPEN_REALM_LIST", &UiEventPayload::empty())?;
        }
        self.glue_ui_dirty = true;
        Ok(())
    }
}

fn load_configured_glue_textures(
    catalog: ArchiveCatalog,
    paths: Vec<AssetPath>,
) -> Result<ConfiguredGlueTexturePrewarm, AssetError> {
    let mut store = AssetStore::mount(catalog)?;
    let mut cache = BlpTextureCache::new();
    let mut failures = Vec::new();
    for path in paths {
        if let Err(error) = cache.load(&mut store, &path) {
            failures.push(format!("failed to prewarm Glue texture {path}: {error}"));
        }
    }
    Ok(ConfiguredGlueTexturePrewarm { cache, failures })
}

struct ConfiguredGlueTexturePrewarm {
    cache: BlpTextureCache,
    failures: Vec<String>,
}

enum ConfiguredGlueTexturePrewarmJob {
    Deferred {
        catalog: ArchiveCatalog,
        paths: Vec<AssetPath>,
    },
    Running(CpuTask<Result<ConfiguredGlueTexturePrewarm, AssetError>>),
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

#[cfg(test)]
mod tests {
    use super::{STOCK_CHARACTER_BACKDROPS, glue_screen_is_presented};

    #[test]
    fn stock_backdrop_prewarm_uses_only_distinct_archive_models() {
        assert_eq!(STOCK_CHARACTER_BACKDROPS.len(), 8);
        assert!(
            STOCK_CHARACTER_BACKDROPS
                .iter()
                .all(|path| !path.contains("UI_Gnome") && !path.contains("UI_Troll"))
        );
    }

    #[test]
    fn process_action_requires_the_selected_glue_screen_to_have_been_presented() {
        assert!(!glue_screen_is_presented("login", None));
        assert!(!glue_screen_is_presented("login", Some("charselect")));
        assert!(glue_screen_is_presented("login", Some("LOGIN")));
    }
}
