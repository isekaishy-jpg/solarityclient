//! Offline measurements through the retained production World presentation owners.

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::Vec3;
use solarity_ecs::{ActiveWorld, PlayerViewState, WorldStateError, WorldTransform};
use solarity_rendering::{WorldModelBaseMip, WorldModelTextureFiltering};
use solarity_ui::GlueError;
use thiserror::Error;

use super::ClientServices;
use crate::RealmClock;
use crate::application::character_directory::RuntimeCharacterMetadata;
use crate::application::run;
use crate::application::terrain_frame::TerrainFrame;
use crate::application::world_ui::RuntimeWorldUi;
use crate::application::{ApplicationError, RuntimeTerrainFrameError};

/// One measured production World transaction, with startup/streaming retained.
#[derive(Clone, Debug)]
pub struct WorldBenchmarkSample {
    /// Explicit fixture stimulus applied during this interval.
    pub phase: &'static str,
    /// Zero-based frame within the phase.
    pub frame: usize,
    /// Complete measured interval, including event polling and stimulus.
    pub total: Duration,
    /// Environment and player residency synchronization.
    pub service: Duration,
    /// Camera demand, completed CPU generation publication, and GPU tile upload.
    pub streaming: Duration,
    /// FrameXML input, OnUpdate, and any GPU UI refresh.
    pub ui: Duration,
    /// Final collision-resolved camera composition.
    pub camera: Duration,
    /// World animation, visibility, command recording, and Vulkan presentation.
    pub present: Duration,
    /// Complete ADTs retained after this transaction.
    pub resident_tiles: usize,
    /// Complete ADTs admitted during this frame's streaming transaction.
    pub admitted_tiles: usize,
    /// Previously resident ADTs released during that transaction.
    pub evicted_tiles: usize,
    /// Visible native terrain-detail texture buckets submitted this frame.
    pub ground_detail_draws: usize,
    /// Eligible unit material packets submitted to the primary shadow map.
    pub primary_shadow_draws: usize,
    /// Final effect inputs after the camera liquid and WMO fog queries.
    pub screen_effect: Option<solarity_rendering::WorldFrameScreenEffect>,
    /// Authored liquid type admitted at the collision-resolved camera.
    pub camera_liquid_type: Option<u32>,
    /// Fixture world position used for this frame's residency demand.
    pub position: Vec3,
}

/// Offline diagnostics retain failures instead of reporting partial success.
#[derive(Debug, Error)]
pub enum WorldBenchmarkError {
    /// A production subsystem failed.
    #[error(transparent)]
    Application(#[from] ApplicationError),
    /// The caller's fixture lacks required ECS state.
    #[error(transparent)]
    WorldState(#[from] WorldStateError),
    /// A required diagnostic precondition or generation is unavailable.
    #[error("offline World benchmark: {0}")]
    State(&'static str),
    /// A FrameXML callback failed during the replay.
    #[error("offline World FrameXML failed: {0}")]
    Ui(String),
    /// A recoverable World presentation error occurred during the replay.
    #[error("offline World presentation failed: {0}")]
    Presentation(String),
    /// Explicit framebuffer output could not be written.
    #[error("could not write World benchmark capture {path}")]
    CaptureIo {
        /// Requested capture path.
        path: PathBuf,
        /// Filesystem failure.
        #[source]
        source: std::io::Error,
    },
    /// The user closed or minimized the diagnostic window.
    #[error("offline World benchmark window was closed or minimized")]
    Cancelled,
}

impl ClientServices {
    pub(in crate::application) fn benchmark_world(
        &mut self,
        world: &mut ActiveWorld,
        clock: &RealmClock,
        frames_per_phase: NonZeroUsize,
        capture_directory: Option<&Path>,
        travel_offset: Option<Vec3>,
        screen_effect: Option<u32>,
    ) -> Result<Vec<WorldBenchmarkSample>, WorldBenchmarkError> {
        if self.gameplay.world().is_some()
            || self.terrain_frame.is_some()
            || self.world_ui.is_some()
        {
            return Err(WorldBenchmarkError::State(
                "requires a fresh diagnostic application",
            ));
        }
        if !RuntimeCharacterMetadata::active_player_is_ready(world) {
            return Err(WorldBenchmarkError::State(
                "fixture player is missing FrameXML facts",
            ));
        }
        let initial_transform = world.local_player_transform()?;
        let player_guid = world.local_player_guid()?;
        if travel_offset.is_some_and(|offset| {
            !offset.is_finite() || !(initial_transform.position() + offset).is_finite()
        }) {
            return Err(WorldBenchmarkError::State("travel segment must be finite"));
        }
        let start = Instant::now();
        self.terrain
            .synchronize(Some(world))
            .map_err(ApplicationError::from)?;
        self.player
            .synchronize(Some(world))
            .map_err(ApplicationError::from)?;
        let effect_policy = |name| {
            self.world_ui
                .as_ref()
                .map_or_else(|| self.glue.cvar_integer(name), |ui| ui.cvar_integer(name))
                .is_some_and(|value| value != 0)
        };
        self.environment
            .set_full_screen_effects(effect_policy("ffx"));
        self.environment
            .set_death_effects(effect_policy("ffxdeath"));
        self.environment.set_glow_effects(effect_policy("ffxglow"));
        if let Some(id) = screen_effect {
            self.environment.select_screen_effect(id);
        }
        if let Some(farclip) = self.world_ui.as_ref().map_or_else(
            || self.glue.cvar_number("farclip"),
            |ui| ui.cvar_number("farclip"),
        ) {
            self.environment.set_view_distance(farclip);
        }
        self.environment
            .synchronize(Some(world), Some(clock))
            .map_err(ApplicationError::from)?;
        let resident = self
            .terrain
            .resident_tiles()
            .next()
            .ok_or(WorldBenchmarkError::State("requires an ADT map"))?;
        let frame = TerrainFrame::prepare(
            &mut self.renderer,
            world.map_id().value(),
            resident.mesh(),
            resident.textures(),
            resident.liquid_batches(),
            resident.m2_scene(),
            resident.world_models(),
            WorldModelTextureFiltering::Anisotropic4x,
            WorldModelBaseMip::Zero,
            &mut self.crt_rand,
            Arc::clone(&self.particle_twinkle),
            self.player.resident_frame_input(),
            &[],
            &[],
            self.game_objects.frame_input(Some(world)),
        )
        .map_err(ApplicationError::from)?;
        self.terrain_frame = Some(frame);
        let zone = self
            .character_metadata
            .zone_state(
                self.terrain
                    .current_area_id(world)
                    .map_err(ApplicationError::from)?,
            )
            .map_err(ApplicationError::from)?;
        let general = self
            .glue
            .localized_text("GENERAL")
            .map_err(GlueError::from)
            .map_err(ApplicationError::from)?;
        let (ui, errors) = RuntimeWorldUi::prepare(
            &mut self.renderer,
            self.platform.window_id(),
            self.assets.clone(),
            self.world_ui_catalog.clone(),
            self.platform.logical_extent(),
            self.startup_profile.cvar_values(),
            &self.addon_catalog,
            &self.character_metadata,
            world,
            zone,
            Some(clock),
            None,
            &super::super::gameplay_coordinator::player_ui::RuntimePlayerUiState::default(),
            general,
            self.sound.output_names(),
        )?;
        if let Some(error) = errors.into_iter().next() {
            return Err(error.into());
        }
        self.world_ui = Some(ui);
        tracing::info!(
            initialization_ms = start.elapsed().as_secs_f64() * 1000.,
            "initialized offline World benchmark"
        );
        if let Some(directory) = capture_directory {
            std::fs::create_dir_all(directory).map_err(|source| {
                WorldBenchmarkError::CaptureIo {
                    path: directory.to_owned(),
                    source,
                }
            })?;
        }
        let initial_view = world.local_player_view()?;
        let mut samples = Vec::new();
        let mut previous = Instant::now();
        let phases = ["streaming", "stationary", "orbit", "pointer"]
            .into_iter()
            .chain(
                travel_offset
                    .into_iter()
                    .flat_map(|_| ["travel_out", "travel_back", "settled"]),
            );
        for phase in phases {
            tracing::info!(phase, "started offline World benchmark phase");
            for index in 0..frames_per_phase.get() {
                let frame_start = Instant::now();
                let elapsed = frame_start.duration_since(previous);
                previous = frame_start;
                for _ in 0..run::MAX_PLATFORM_EVENTS_PER_FRAME {
                    let Some(event) = self.poll_platform_event() else {
                        break;
                    };
                    if run::exit_reason(&event.event, self.platform.window_id().value()).is_some() {
                        return Err(WorldBenchmarkError::Cancelled);
                    }
                }
                if self.platform.presentation_suspended() {
                    return Err(WorldBenchmarkError::Cancelled);
                }
                if let Some(offset) = travel_offset {
                    let progress = (index + 1) as f32 / frames_per_phase.get() as f32;
                    let fraction = match phase {
                        "travel_out" => progress,
                        "travel_back" => 1. - progress,
                        _ => 0.,
                    };
                    world.update_transform(
                        player_guid,
                        WorldTransform::new(
                            initial_transform.position() + offset * fraction,
                            initial_transform.orientation(),
                        ),
                    )?;
                }
                let view = if phase == "orbit" {
                    PlayerViewState::new(
                        initial_view.distance(),
                        initial_view.pitch_radians(),
                        initial_view.yaw_offset_radians()
                            + index as f32 * std::f32::consts::TAU / frames_per_phase.get() as f32,
                        initial_view.view(),
                    )
                } else {
                    initial_view
                };
                world.set_local_player_view(view)?;
                let capture = capture_directory.filter(|_| {
                    index == 0
                        || index + 1 == frames_per_phase.get()
                        || (matches!(phase, "orbit" | "travel_out" | "travel_back")
                            && index.is_multiple_of((frames_per_phase.get() / 4).max(1)))
                });
                if capture.is_some() {
                    self.renderer
                        .request_frame_capture()
                        .map_err(ApplicationError::from)?;
                }
                self.service_recording();
                let sample =
                    self.benchmark_world_frame(world, clock, phase, index, elapsed, frame_start)?;
                samples.push(sample);
                if let Some(directory) = capture {
                    let frame = self
                        .renderer
                        .take_captured_frame()
                        .map_err(ApplicationError::from)?
                        .ok_or(WorldBenchmarkError::State(
                            "requested capture was not presented",
                        ))?;
                    let path = directory.join(format!("{phase}-{index:04}.ppm"));
                    super::glue_benchmark::write_capture(&path, &frame).map_err(|source| {
                        WorldBenchmarkError::CaptureIo {
                            path: path.clone(),
                            source,
                        }
                    })?;
                    tracing::info!(path = %path.display(), "wrote World framebuffer capture");
                }
            }
        }
        world.set_local_player_view(initial_view)?;
        world.update_transform(player_guid, initial_transform)?;
        Ok(samples)
    }

    fn benchmark_world_frame(
        &mut self,
        world: &ActiveWorld,
        clock: &RealmClock,
        phase: &'static str,
        index: usize,
        elapsed: Duration,
        frame_start: Instant,
    ) -> Result<WorldBenchmarkSample, WorldBenchmarkError> {
        let start = Instant::now();
        let effect_policy = |name| {
            self.world_ui
                .as_ref()
                .map_or_else(|| self.glue.cvar_integer(name), |ui| ui.cvar_integer(name))
                .is_some_and(|value| value != 0)
        };
        self.environment
            .set_full_screen_effects(effect_policy("ffx"));
        self.environment
            .set_death_effects(effect_policy("ffxdeath"));
        self.environment.set_glow_effects(effect_policy("ffxglow"));
        if let Some(farclip) = self.world_ui.as_ref().map_or_else(
            || self.glue.cvar_number("farclip"),
            |ui| ui.cvar_number("farclip"),
        ) {
            self.environment.set_view_distance(farclip);
        }
        self.environment
            .synchronize(Some(world), Some(clock))
            .map_err(ApplicationError::from)?;
        self.player
            .synchronize(Some(world))
            .map_err(ApplicationError::from)?;
        let service = start.elapsed();
        let start = Instant::now();
        let previous_tiles = self
            .terrain
            .resident_tiles()
            .map(|tile| tile.mesh().tile())
            .collect::<Vec<_>>();
        // Normal World servicing promotes the followed player's ADT before
        // submitting the camera window. Travel must exercise that ownership
        // transition as well as loading neighbors around a fixed primary.
        self.terrain
            .synchronize_async(Some(world), &self.cpu)
            .map_err(ApplicationError::from)?;
        self.service_terrain_streaming()?;
        let current_tiles = self
            .terrain
            .resident_tiles()
            .map(|tile| tile.mesh().tile())
            .collect::<Vec<_>>();
        let admitted_tiles = current_tiles
            .iter()
            .filter(|tile| !previous_tiles.contains(tile))
            .count();
        let evicted_tiles = previous_tiles
            .iter()
            .filter(|tile| !current_tiles.contains(tile))
            .count();
        let streaming = start.elapsed();
        let start = Instant::now();
        let ui = self
            .world_ui
            .as_mut()
            .ok_or(WorldBenchmarkError::State("missing FrameXML"))?;
        ui.synchronize_realm_clock(clock)
            .map_err(ApplicationError::from)?;
        if phase == "pointer" {
            let [width, height] = ui.logical_extent();
            ui.pointer_motion((
                f64::from(width) * (0.1 + (index % 240) as f64 / 300.),
                f64::from(height) - 24.,
            ))?;
        }
        ui.update(elapsed.as_secs_f64())?;
        if let Some(error) = ui.take_callback_failure() {
            return Err(WorldBenchmarkError::Ui(error));
        }
        if let (Some(terrain), Some(player)) = (
            self.terrain_frame.as_ref(),
            self.player.resident_frame_input(),
        ) {
            ui.synchronize_portrait(&mut self.renderer, terrain, &player)?;
        }
        ui.refresh(&mut self.renderer)?;
        ui.synchronize_minimap(
            &mut self.renderer,
            &self.cpu,
            self.terrain.active_map(),
            self.player
                .resident_frame_input()
                .map(|player| player.world_transform()),
        )?;
        let ui_duration = start.elapsed();
        let start = Instant::now();
        let camera = self
            .resolved_world_camera()?
            .ok_or(WorldBenchmarkError::State("missing camera"))?;
        let (underwater, indoor_fog) = self
            .terrain
            .camera_environment(camera.camera().position(), &self.liquids)
            .map_err(super::super::sound_coordinator::RuntimeSoundError::from)
            .map_err(ApplicationError::from)?;
        let camera_duration = start.elapsed();
        let start = Instant::now();
        let frame = self
            .terrain_frame
            .as_mut()
            .ok_or(WorldBenchmarkError::State("missing terrain frame"))?;
        let player = self
            .player
            .resident_frame_input()
            .ok_or(RuntimeTerrainFrameError::MissingPlayerM2FrameInput)
            .map_err(ApplicationError::from)?;
        let environment = self
            .environment
            .current()
            .ok_or(WorldBenchmarkError::State("missing environment"))?;
        let environment = self
            .environment
            .resolve_liquid(environment, underwater, &self.liquids)
            .map_err(ApplicationError::from)?
            .with_world_model_fog(indoor_fog);
        let ui = self
            .world_ui
            .as_ref()
            .ok_or(WorldBenchmarkError::State("missing FrameXML"))?;
        let celestial_resources = self
            .sky_resources
            .prepare(&mut self.renderer, environment)
            .map_err(ApplicationError::from)?;
        frame
            .set_environment_detail(ui.cvar_number("environmentDetail"))
            .map_err(ApplicationError::from)?;
        frame
            .set_shadow_quality(ui.cvar_number("extShadowQuality"))
            .map_err(ApplicationError::from)?;
        frame
            .set_horizon_scale(ui.cvar_number("horizonFarclipScale"))
            .map_err(ApplicationError::from)?;
        frame
            .set_ground_detail(
                ui.cvar_number("groundEffectDensity"),
                ui.cvar_number("groundEffectDist"),
            )
            .map_err(ApplicationError::from)?;
        let presentation_time_ms = sdl3::timer::ticks() as u32;
        let screen_effect = environment.screen_effect(presentation_time_ms);
        let report = frame
            .present(
                &mut self.renderer,
                self.terrain
                    .resident_mesh_plan()
                    .map(solarity_rendering::TerrainTileMeshPlan::tile),
                environment,
                &mut self.terrain,
                camera,
                presentation_time_ms,
                underwater.is_some(),
                self.glue.cvar_boolean("specular"),
                None,
                None,
                celestial_resources,
                &mut self.sky_resources,
                None,
                None,
                &mut self.crt_rand,
                player,
                &[],
                &[],
                self.game_objects.frame_input(Some(world)),
                ui.logical_extent(),
                ui.draws(),
                &[],
            )
            .map_err(ApplicationError::from)?;
        let errors = frame.drain_recoverable_errors();
        if let Some(error) = errors.into_iter().next() {
            return Err(WorldBenchmarkError::Presentation(error));
        }
        // This replay excludes audio; consume events so storage matches normal frame lifetime.
        let _ = frame.drain_m2_events();
        let present = start.elapsed();
        Ok(WorldBenchmarkSample {
            phase,
            frame: index,
            total: frame_start.elapsed(),
            service,
            streaming,
            ui: ui_duration,
            camera: camera_duration,
            present,
            resident_tiles: self.terrain.resident_tile_count(),
            admitted_tiles,
            evicted_tiles,
            ground_detail_draws: report.ground_detail_draw_count(),
            primary_shadow_draws: report.primary_shadow_draw_count(),
            screen_effect,
            camera_liquid_type: underwater.map(|liquid| liquid.liquid_type),
            position: world.local_player_transform()?.position(),
        })
    }
}
