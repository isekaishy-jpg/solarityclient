//! Concrete ownership and shutdown of initial client services.

#![allow(unsafe_code)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use tokio::runtime::{Builder, Runtime};

use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetError, AssetStore, AssetStoreHandle,
    BlpTextureCache, CharacterAppearanceCatalog, CharacterRaceCatalog, CharacterStartOutfitCatalog,
    CreatureCatalog, CreatureFamilyCatalog, GameObjectDisplayCatalog,
    HelmetGeosetVisibilityCatalog, ItemDefinitionCatalog, ItemDisplayCatalog, ItemVisualCatalog,
    LightCatalog, LoadingScreenCatalog, MapCatalog, ParticleColorCatalog,
};
use solarity_cpu::CpuExecutor;
use solarity_media::SoundOutputTarget;
use solarity_network::{
    AccountExpansion, CharacterCreation, CharacterRename, CharacterRenameError, RealmEntry,
    WorldAddon, WorldAddonManifest,
};
use solarity_rendering::{
    CharacterComponentTextureLevel, M2ParticleTwinkleTable, VulkanBootstrap, VulkanPresentMode,
    VulkanRenderer, VulkanReport, WorldCamera, WorldModelBaseMip, WorldModelTextureFiltering,
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
use crate::application::environment_coordinator::RuntimeWorldEnvironment;
use crate::application::gameplay_coordinator::RuntimeGameplayCoordinator;
use crate::application::login_coordinator::{
    RuntimeAuthenticatedLogin, RuntimeLoginCoordinator, RuntimeLoginError, RuntimeLoginPoll,
    RuntimeLoginState,
};
use crate::application::login_model::RuntimeGlueModelScene;
use crate::application::login_ui::RuntimeUiFrame;
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
use crate::application::transport_coordinator::{
    RuntimeTransportPoll, RuntimeTransportPresentation,
};
use crate::application::world_coordinator::{
    RuntimeCharacterScreenRequests, RuntimeWorldCoordinator, RuntimeWorldError, RuntimeWorldPoll,
    RuntimeWorldState,
};
use crate::application::world_ui::RuntimeWorldUi;
use crate::configuration::{RuntimeConfiguration, StartupProfile};
use crate::input::{InputControl, InputFrameMotion, stock_keyboard_name};
use crate::loading::{LoadingScreenDirectory, RuntimeLoadingReadiness, RuntimeLoadingScreen};
use crate::platform::{ButtonState, MouseButton, MouseWheelDirection, PlatformEvent, SdlPlatform};
use crate::random::{BlizzardRand, CrtRand};

/// Concrete services owned exclusively by the application composition root.
pub(crate) struct ClientServices {
    renderer: VulkanRenderer,
    login_ui: Option<RuntimeUiFrame>,
    /// Last Glue screen actually submitted to the swapchain.
    ///
    /// Input may select a different screen while SDL still has events queued
    /// for the old presentation. Process-ending actions are admitted only
    /// after the selected screen has crossed this presentation boundary.
    presented_glue_screen: Option<String>,
    ui_textures: BlpTextureCache,
    world_ui: Option<RuntimeWorldUi>,
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
    environment: RuntimeWorldEnvironment,
    player: RuntimePlayerPresentation,
    transport: RuntimeTransportPresentation,
    terrain: RuntimeTerrainCoordinator,
    terrain_frame: Option<TerrainFrame>,
    fps: Option<RuntimeFpsOverlay>,
    loading_directory: LoadingScreenDirectory,
    loading_screen: Option<RuntimeLoadingScreen>,
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
        let mut assets = AssetStore::mount(catalog)?;
        let animations = AnimationDataCatalog::load(&mut assets)?;
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
        let assets = AssetStoreHandle::new(assets);
        let mut fps = RuntimeFpsOverlay::prepare(&mut renderer, &assets, platform.pixel_extent())?;
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
        let sound = RuntimeSoundCoordinator::start(
            assets.clone(),
            &glue,
            SoundOutputTarget::DefaultDevice,
        )?;
        // M2Initialize consumes these before any ordinary or Glue emitter is
        // constructed. The resulting table remains process-wide.
        let mut crt_rand = CrtRand::new();
        let first = u32::from(crt_rand.next_u15());
        let second = u32::from(crt_rand.next_u15());
        let particle_twinkle = Arc::new(M2ParticleTwinkleTable::new(first << 16 | second));
        let cpu = CpuExecutor::new(configuration.cpu_pool())?;
        let mut glue_model = RuntimeGlueModelScene::new();
        if initial_screen == GlueInitialScreen::Movie
            && let Some(login_model) = glue
                .configured_model_presentation("AccountLogin")
                .map_err(GlueError::from)?
        {
            let background_light_count = login_model
                .background_lights()
                .live()
                .iter()
                .filter(|light| light.is_some())
                .count();
            glue_model.prewarm(
                login_model.path().clone(),
                background_light_count,
                &assets,
                &cpu,
            )?;
            // AccountLogin's immutable resources and live effect owner must
            // both exist before the movie starts. The movie then advances that
            // hidden owner until EULA reveals the already-current scene.
            glue_model.finish_prewarm(
                &mut renderer,
                &login_model,
                &mut crt_rand,
                Arc::clone(&particle_twinkle),
            )?;
        }
        if initial_screen != GlueInitialScreen::Movie {
            glue_model.synchronize(
                &mut renderer,
                &glue,
                &assets,
                &cpu,
                &mut crt_rand,
                Arc::clone(&particle_twinkle),
                None,
                false,
            )?;
        }
        let mut ui_textures = BlpTextureCache::new();
        let login_ui = if glue.media_intent().movie().is_some() {
            None
        } else {
            let frame = RuntimeUiFrame::prepare_glue(&mut renderer, &glue, &mut ui_textures)?;
            let overlay = if glue.cvar_boolean("showfps") {
                fps.as_ref().map_or(&[][..], RuntimeFpsOverlay::draws)
            } else {
                &[]
            };
            if !glue_model.present(&mut renderer, &glue, &frame, 0.0, &mut crt_rand, overlay)? {
                frame.present_with_overlay(&mut renderer, overlay)?;
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
                presented_glue_screen,
                ui_textures,
                world_ui: None,
                glue_model,
                cinematic: RuntimeCinematicCoordinator::default(),
                sound,
                platform,
                input,
                glue,
                assets: assets.clone(),
                startup_profile,
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
                ),
                transport: RuntimeTransportPresentation::new(assets.clone(), game_object_displays),
                terrain: RuntimeTerrainCoordinator::new(assets, maps),
                terrain_frame: None,
                fps,
                loading_directory,
                loading_screen: None,
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
    pub(crate) fn poll_platform_event(&mut self) -> Option<PlatformEvent> {
        let event = self.platform.poll_event()?;
        self.input.admit(&event);
        Some(event)
    }

    /// Returns the retained raw input state committed during event polling.
    pub(crate) const fn input_control(&self) -> &InputControl {
        &self.input
    }

    /// Takes relative pointer and wheel motion accumulated for one frame.
    pub(crate) fn take_input_frame_motion(&mut self) -> InputFrameMotion {
        self.input.take_frame_motion()
    }

    /// Routes one already-admitted platform event to the active Glue owner.
    pub(crate) fn service_platform_event(
        &mut self,
        event: &PlatformEvent,
    ) -> Result<(), ApplicationError> {
        if self.loading_screen.is_some() {
            return Ok(());
        }
        if self.gameplay.world().is_some() {
            return self.service_world_platform_event(event);
        }
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
                } else {
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
                        self.login_ui = None;
                    }
                }
            }
            PlatformEvent::TextInput(input)
                if input.window_id == self.platform.window_id()
                    && self.glue.media_intent().movie().is_none() =>
            {
                if self.glue.text_input(&input.text)?.is_some() {
                    self.login_ui = None;
                }
            }
            PlatformEvent::TextEditing(composition)
                if composition.window_id == self.platform.window_id()
                    && self.glue.media_intent().movie().is_none() =>
            {
                if self.glue.text_composition(&composition.text)?.is_some() {
                    self.login_ui = None;
                }
            }
            PlatformEvent::MouseButton(pointer)
                if pointer.window_id == self.platform.window_id()
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
                    self.login_ui = None;
                }
            }
            PlatformEvent::MouseMotion(pointer)
                if pointer.window_id == self.platform.window_id()
                    && self.glue.media_intent().movie().is_none() =>
            {
                let (window_width, window_height) = self.platform.logical_extent();
                let (ui_width, ui_height) = self.glue.geometry().ui_extent();
                let position = (
                    f64::from(pointer.x) / f64::from(window_width) * ui_width,
                    ui_height - f64::from(pointer.y) / f64::from(window_height) * ui_height,
                );
                if self.glue.pointer_motion(position)?.is_some() {
                    self.login_ui = None;
                }
            }
            PlatformEvent::MouseWheel(wheel)
                if wheel.window_id == self.platform.window_id()
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
                    self.login_ui = None;
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
    ) -> Result<(), ApplicationError> {
        let window_id = self.platform.window_id();
        let logical_extent = self.platform.logical_extent();
        let pointer_position = self.input.pointer_position();
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
        match event {
            PlatformEvent::Key(key_event) if key_event.window_id == window_id => {
                let Some(scan_code) = key_event.scan_code else {
                    return Ok(());
                };
                let Some(key) = stock_keyboard_name(scan_code) else {
                    return Ok(());
                };
                let modifiers = UiKeyboardModifiers::new(
                    key_event.modifiers.has_shift(),
                    key_event.modifiers.has_control(),
                    key_event.modifiers.has_alt(),
                );
                world_ui.keyboard_key(key, key_event.state == ButtonState::Pressed, modifiers)?;
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
                world_ui.pointer_button(
                    project_pointer(pointer.x, pointer.y),
                    button,
                    pointer.state == ButtonState::Pressed,
                    pointer.click_count,
                    UiKeyboardModifiers::new(
                        self.input.modifiers().has_shift(),
                        self.input.modifiers().has_control(),
                        self.input.modifiers().has_alt(),
                    ),
                )?;
            }
            PlatformEvent::MouseMotion(pointer) if pointer.window_id == window_id => {
                world_ui.pointer_motion(project_pointer(pointer.x, pointer.y))?;
            }
            PlatformEvent::MouseWheel(wheel) if wheel.window_id == window_id => {
                let delta = match wheel.direction {
                    MouseWheelDirection::Normal => wheel.y,
                    MouseWheelDirection::Flipped => -wheel.y,
                    MouseWheelDirection::Unknown => return Ok(()),
                };
                if let Some(pointer) = pointer_position {
                    world_ui.pointer_wheel(
                        project_pointer(pointer.x(), pointer.y()),
                        f64::from(delta),
                    )?;
                }
            }
            _ => {}
        }
        self.platform
            .set_text_input_active(world_ui.has_focused_edit_box());
        Ok(())
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
        if glue_process_action_is_presented(&current_screen, self.presented_glue_screen.as_deref())
        {
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
        let update_time = std::time::Instant::now();
        if self.platform.presentation_suspended() {
            // A minimized Vulkan surface cannot pace the main loop reliably,
            // so keep animation time bounded and yield briefly while the event
            // pump remains responsive to restoration.
            self.glue_update_clock = update_time;
            std::thread::sleep(std::time::Duration::from_millis(16));
            return Ok(());
        }
        if self.gameplay.world().is_none() {
            self.sound
                .synchronize_glue_media(&self.glue, &mut self.blizzard_rand.borrow_mut())?;
        }
        if self
            .loading_screen
            .as_ref()
            .is_some_and(RuntimeLoadingScreen::ready_to_complete)
        {
            self.loading_screen = None;
        } else if let Some(loading) = self.loading_screen.as_mut() {
            self.glue_update_clock = update_time;
            let overlay = if self.glue.cvar_boolean("showfps") {
                self.fps.as_ref().map_or(&[][..], RuntimeFpsOverlay::draws)
            } else {
                &[]
            };
            loading.present(&mut self.renderer, overlay)?;
            if let Some(fps) = self.fps.as_mut() {
                fps.record_presented(&mut self.renderer, std::time::Instant::now())?;
            }
            return Ok(());
        }
        if self.gameplay.world().is_none() {
            let glue_elapsed = update_time
                .duration_since(self.glue_update_clock)
                .as_secs_f64();
            self.glue_update_clock = update_time;
            if self.glue.update(glue_elapsed)?
                && let Some(frame) = self.login_ui.as_mut()
            {
                frame.refresh_glue(&mut self.renderer, &self.glue, &mut self.ui_textures)?;
            }
            self.sync_platform_text_input();
            self.persist_active_cvars()?;
            let movie = self.glue.media_intent().movie().cloned();
            let cinematic_overlay = self
                .glue
                .cvar_boolean("showfps")
                .then(|| {
                    self.fps
                        .as_ref()
                        .map(|fps| (fps.logical_extent(), fps.draws()))
                })
                .flatten();
            match self.cinematic.synchronize(
                movie.as_ref(),
                &mut self.renderer,
                &mut self.sound,
                cinematic_overlay,
            )? {
                RuntimeCinematicPoll::Presented => {
                    let global_time_ms = self.m2_global_clock.elapsed().as_secs_f32() * 1_000.0;
                    self.glue_model.advance_hidden(
                        &self.renderer,
                        global_time_ms,
                        &mut self.crt_rand,
                    )?;
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
                world_ui.refresh(&mut self.renderer)?;
                self.platform
                    .set_text_input_active(world_ui.has_focused_edit_box());
            } else {
                self.platform.set_text_input_active(false);
            }
            self.persist_active_cvars()?;
        }
        let (Some(environment), Some(pose)) =
            (self.environment.current(), self.player.camera_pose())
        else {
            return self.present_glue_frame();
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
            self.sound.update(
                &self.glue,
                clock,
                camera,
                &mut self.blizzard_rand.borrow_mut(),
            )?;
        }
        let plan = self.terrain.resident_mesh_plan();
        let global_animation_time_ms = self.m2_global_clock.elapsed().as_secs_f32() * 1_000.0;
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
            || {
                self.fps.as_ref().map_or_else(
                    || overlay_extent((width, height)),
                    RuntimeFpsOverlay::logical_extent,
                )
            },
            RuntimeWorldUi::logical_extent,
        );
        let frame_draws = self
            .world_ui
            .as_ref()
            .map_or(&[][..], RuntimeWorldUi::draws);
        let fps_draws = if self.glue.cvar_boolean("showfps") {
            self.fps.as_ref().map_or(&[][..], RuntimeFpsOverlay::draws)
        } else {
            &[]
        };
        let mut ui_draws = Vec::with_capacity(frame_draws.len() + fps_draws.len());
        ui_draws.extend_from_slice(frame_draws);
        ui_draws.extend_from_slice(fps_draws);
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
            self.transport.resident(),
            ui_extent,
            &ui_draws,
        )?;
        let mount_camera_sample = frame.take_mount_camera_sample();
        let camera_time_ms = mount_camera_sample
            .map_or_else(|| frame.m2_animation_time_ms(), |sample| sample.time_ms());
        let mount_camera = mount_camera_sample.map(|sample| {
            MountCameraGeometry::new(sample.animated_height(), sample.fixed_height())
        });
        self.player
            .apply_mount_camera_sample(mount_camera, camera_time_ms)?;
        let m2_events = frame.drain_m2_events();
        self.sound
            .play_m2_events(&m2_events, camera, &mut self.blizzard_rand.borrow_mut())?;
        if let Some(fps) = self.fps.as_mut() {
            fps.record_presented(&mut self.renderer, std::time::Instant::now())?;
        }
        Ok(())
    }

    fn present_glue_frame(&mut self) -> Result<(), ApplicationError> {
        self.synchronize_component_texture_level();
        if self.login_ui.is_none() {
            self.login_ui = Some(RuntimeUiFrame::prepare_glue(
                &mut self.renderer,
                &self.glue,
                &mut self.ui_textures,
            )?);
        }
        let frame = self
            .login_ui
            .as_ref()
            .ok_or_else(|| ApplicationError::NetworkRuntime {
                message: "Glue frame preparation produced no presentation state".to_owned(),
            })?;
        let current_screen = self.glue.current_screen();
        let glue_character_changed = match current_screen.as_str() {
            "charcreate" => {
                let preview = self.glue.character_creation_preview();
                self.player
                    .synchronize_character_creation(preview.as_ref())?
            }
            "charselect" => {
                let preview = self.glue.character_selection_preview();
                self.player
                    .synchronize_character_selection(preview.as_ref())?
            }
            _ => self.player.synchronize_character_creation(None)?,
        };
        let glue_character = self.player.glue_character_frame_input();
        self.glue_model.synchronize(
            &mut self.renderer,
            &self.glue,
            &self.assets,
            &self.cpu,
            &mut self.crt_rand,
            Arc::clone(&self.particle_twinkle),
            glue_character,
            glue_character_changed,
        )?;
        let global_time_ms = self.m2_global_clock.elapsed().as_secs_f32() * 1_000.0;
        let overlay = if self.glue.cvar_boolean("showfps") {
            self.fps.as_ref().map_or(&[][..], RuntimeFpsOverlay::draws)
        } else {
            &[]
        };
        if !self.glue_model.present(
            &mut self.renderer,
            &self.glue,
            frame,
            global_time_ms,
            &mut self.crt_rand,
            overlay,
        )? {
            frame.present_with_overlay(&mut self.renderer, overlay)?;
        }
        if let Some(fps) = self.fps.as_mut() {
            fps.record_presented(&mut self.renderer, std::time::Instant::now())?;
        }
        self.presented_glue_screen = Some(current_screen);
        Ok(())
    }

    /// Applies ordered Glue actions and polls one asynchronous login result.
    pub(crate) fn service_login(&mut self) -> Result<(), ApplicationError> {
        let Some(network) = self.network.as_ref() else {
            return Ok(());
        };
        let handle = network.handle().clone();
        let mut character_screen_requests = RuntimeCharacterScreenRequests::default();
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
                    self.environment.disconnect();
                    self.player.disconnect();
                    self.terrain.disconnect();
                    self.sound.disconnect()?;
                    self.terrain_frame = None;
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
                UiGlueNetworkAction::SortRealms { sort } => {
                    self.glue.sort_realm_directory(sort);
                    self.glue
                        .dispatch_event("OPEN_REALM_LIST", &UiEventPayload::empty())?;
                    self.login_ui = None;
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
                                ])?,
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
                                ])?,
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
                                ])?,
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
                                ])?,
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
                        ])?,
                    )?;
                }
                UiGlueNetworkAction::ForceCharacterRename { message_token } => {
                    self.glue.dispatch_event(
                        "FORCE_RENAME_CHARACTER",
                        &UiEventPayload::new([UiEventArgument::String(message_token.to_owned())])?,
                    )?;
                }
                UiGlueNetworkAction::SelectCharacter { index } => {
                    let payload =
                        UiEventPayload::new([UiEventArgument::Integer(i64::from(index))])?;
                    self.glue
                        .dispatch_event("UPDATE_SELECTED_CHARACTER", &payload)?;
                }
                UiGlueNetworkAction::EnterWorld { guid } => {
                    self.world_ui = None;
                    let map_id = self
                        .world
                        .character_selection()
                        .and_then(|selection| selection.directory().by_guid(guid))
                        .map(|character| character.location().map_id());
                    let loading = map_id
                        .map(|map_id| {
                            RuntimeLoadingScreen::prepare(
                                &mut self.renderer,
                                &self.assets,
                                &self.loading_directory,
                                map_id,
                                self.platform.logical_extent(),
                            )
                        })
                        .transpose()?;
                    match self.world.enter_world(&handle, guid) {
                        Ok(()) => self.loading_screen = loading,
                        Err(RuntimeWorldError::AlreadyActive) => {}
                        Err(error) => self.publish_world_failure(error),
                    }
                }
            }
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
                    &UiEventPayload::new([UiEventArgument::String("charselect".to_owned())])?,
                )?;
                self.login_ui = None;
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
                        &UiEventPayload::new([UiEventArgument::Integer(count)])?,
                    )?;
                    self.login_ui = None;
                }
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
                        &UiEventPayload::new([UiEventArgument::String("charselect".to_owned())])?,
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
                        ])?,
                    )?;
                }
                self.login_ui = None;
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
                        ])?,
                    )?;
                }
                self.login_ui = None;
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
                        ])?,
                    )?;
                }
                self.login_ui = None;
            }
            Ok(RuntimeWorldPoll::CharacterOperationCancelled) => {}
            Ok(RuntimeWorldPoll::EnteredWorld) => {
                if let Some(entry) = self.world.take_world_entry() {
                    self.gameplay.begin(&handle, entry)?;
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
                        &UiEventPayload::new([UiEventArgument::Integer(count)])?,
                    )?;
                    self.login_ui = None;
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
                    ])?,
                )?;
                self.login_ui = None;
            }
            Err(error) => self.publish_world_failure(error),
        }
        self.gameplay.service()?;
        if let (Some(world_ui), Some(buttons)) =
            (self.world_ui.as_mut(), self.gameplay.action_buttons())
        {
            world_ui.synchronize_action_buttons(buttons)?;
        }
        self.environment
            .synchronize(self.gameplay.world(), self.gameplay.realm_clock())?;
        self.synchronize_component_texture_level();
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
        match self.transport.synchronize(self.gameplay.world())? {
            RuntimeTransportPoll::ResourceLoaded { guid, kind } => {
                if let Some(frame) = self.terrain_frame.as_mut() {
                    frame.replace_transport(
                        &mut self.renderer,
                        self.transport.resident(),
                        &mut self.crt_rand,
                    )?;
                }
                tracing::debug!(
                    transport_guid = guid,
                    resource_kind = ?kind,
                    "local player transport resource became resident"
                );
            }
            RuntimeTransportPoll::Idle
            | RuntimeTransportPoll::AwaitingObject { .. }
            | RuntimeTransportPoll::NoResource { .. } => {
                if let Some(frame) = self.terrain_frame.as_mut() {
                    frame.replace_transport(&mut self.renderer, None, &mut self.crt_rand)?;
                }
            }
            RuntimeTransportPoll::Current { .. } => {}
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
                    self.transport.resident(),
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
                self.terrain_frame = Some(frame);
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
                    self.transport.resident(),
                )?;
                tracing::info!(
                    map_id,
                    m2_mesh_count = frame.m2_mesh_count(),
                    m2_placement_count = frame.m2_placement_count(),
                    world_model_placement_count = frame.world_model_placement_count(),
                    "global WMO entered renderer resources"
                );
                self.terrain_frame = Some(frame);
            }
            RuntimeTerrainPoll::Idle => {
                self.sound.disconnect()?;
                self.terrain_frame = None;
            }
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
        self.prepare_world_ui_if_ready()?;
        self.synchronize_world_ui_zone()?;
        if let Some(loading) = self.loading_screen.as_mut() {
            let readiness = RuntimeLoadingReadiness {
                world_accepted: self.gameplay.world().is_some(),
                environment_ready: self.environment.current().is_some(),
                player_ready: self.player.resident_frame_input().is_some(),
                scene_ready: self.terrain_frame.is_some(),
                ui_ready: self.world_ui.is_some(),
                transport_resource_ready: self.transport.is_ready(),
            };
            loading.advance(readiness.stage());
        }
        Ok(())
    }

    /// Builds the independently retained FrameXML owner only after the loading
    /// surface has presented and every synchronous world fact is authoritative.
    fn prepare_world_ui_if_ready(&mut self) -> Result<(), ApplicationError> {
        if self.world_ui.is_some()
            || !self
                .loading_screen
                .as_ref()
                .is_some_and(RuntimeLoadingScreen::has_presented)
            || self.environment.current().is_none()
            || self.player.resident_frame_input().is_none()
            || self.terrain_frame.is_none()
            || !self.transport.is_ready()
            || self.gameplay.action_buttons().is_none()
        {
            return Ok(());
        }
        let Some(active) = self.gameplay.world() else {
            return Ok(());
        };
        let Some(clock) = self.gameplay.realm_clock() else {
            return Ok(());
        };
        let zone = self
            .character_metadata
            .zone_state(self.terrain.current_area_id(active)?)?;
        let general_tab_name = self
            .glue
            .localized_text("GENERAL")
            .map_err(GlueError::from)?;
        let world_ui = RuntimeWorldUi::prepare(
            &mut self.renderer,
            self.assets.clone(),
            self.platform.logical_extent(),
            self.startup_profile.cvar_values(),
            &self.addon_catalog,
            &self.character_metadata,
            active,
            zone,
            clock,
            self.gameplay.action_buttons(),
            general_tab_name,
        )?;
        tracing::info!("loaded stock FrameXML and published world-entry events");
        self.world_ui = Some(world_ui);
        Ok(())
    }

    /// Keeps FrameXML's area labels synchronized with the authoritative player
    /// position after world UI bootstrap.
    fn synchronize_world_ui_zone(&mut self) -> Result<(), ApplicationError> {
        let (Some(world_ui), Some(active)) = (self.world_ui.as_mut(), self.gameplay.world()) else {
            return Ok(());
        };
        world_ui.synchronize_zone(
            self.character_metadata
                .zone_state(self.terrain.current_area_id(active)?)?,
        )
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
        self.environment.disconnect();
        self.player.disconnect();
        self.transport.disconnect();
        self.terrain.disconnect();
        self.sound.disconnect()?;
        self.terrain_frame = None;
        self.world_ui = None;
        self.loading_screen = None;
        let renderer_result = self.renderer.shutdown().map_err(ApplicationError::from);
        let cpu_result = self.cpu.shutdown().map_err(ApplicationError::from);
        if let Some(network) = self.network.take() {
            network.shutdown_timeout(self.network_shutdown_timeout);
        }
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

    fn sync_platform_text_input(&mut self) {
        self.platform
            .set_text_input_active(self.glue.focused_edit_box().is_some());
    }
}

/// Returns whether the active Glue screen has reached the swapchain at least
/// once since it was selected. Case folding follows Glue's screen dispatch.
fn glue_process_action_is_presented(current_screen: &str, presented_screen: Option<&str>) -> bool {
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
    fn publish_login_failure(&mut self, error: RuntimeLoginError) {
        self.realm_directory_published = false;
        self.glue.set_network_status(UiGlueNetworkStatus::default());
        tracing::warn!(error = %error, "login exchange failed");
        self.login_failures.push_back(error);
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
                self.character_screen_published = false;
                self.character_directory_published = false;
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
        self.login_ui = None;
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
        self.login_ui = None;
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

#[cfg(test)]
mod tests {
    use super::glue_process_action_is_presented;

    #[test]
    fn process_action_requires_the_selected_glue_screen_to_have_been_presented() {
        assert!(!glue_process_action_is_presented("login", None));
        assert!(!glue_process_action_is_presented(
            "login",
            Some("charselect")
        ));
        assert!(glue_process_action_is_presented("login", Some("LOGIN")));
    }
}
