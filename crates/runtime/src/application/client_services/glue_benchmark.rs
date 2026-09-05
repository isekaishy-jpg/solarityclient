//! Offline diagnostic replay through the production Glue presentation path.

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use solarity_ui::{
    UiCharacterDirectory, UiCharacterExpansion, UiEventArgument, UiEventPayload,
    UiGlueNetworkAction, UiGlueNetworkStatus, UiPointerButton,
};
use thiserror::Error;

use super::ClientServices;
use crate::application::ApplicationError;
use crate::application::frame_profile::RuntimeFrameProfile;
use crate::application::login_model::RuntimeGlueModelPoll;
use crate::application::run;

/// A stock Glue route used by an explicitly requested offline diagnostic replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GlueBenchmarkScreen {
    /// AccountLogin, with intro/legal dialogs disabled in the caller's profile.
    Login,
    /// A directory supplied by the diagnostic caller rather than a server.
    CharacterSelection,
    /// The real DBC-backed character creation flow with Wrath entitlement.
    CharacterCreation,
}

impl GlueBenchmarkScreen {
    /// Returns the exact SET_GLUE_SCREEN token used by build-12340 GlueXML.
    pub const fn token(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::CharacterSelection => "charselect",
            Self::CharacterCreation => "charcreate",
        }
    }
}

/// One stimulus delivered before measuring its full presentation transaction.
#[derive(Clone, Debug)]
pub enum GlueBenchmarkAction {
    /// Deliver ordered events before the next present, as one measured stimulus.
    Sequence(Vec<GlueBenchmarkAction>),
    /// Select a stock route through SET_GLUE_SCREEN and its authored fades.
    Screen(GlueBenchmarkScreen),
    /// Publish enum-time fixture data through CHARACTER_LIST_UPDATE.
    Directory(UiCharacterDirectory),
    /// Click the center of a named live button through normal pointer routing.
    Click(String),
}

/// One labelled action and the route that must actually reach the swapchain.
#[derive(Clone, Debug)]
pub struct GlueBenchmarkStep {
    /// Caller-assigned identity retained with every timing sample.
    pub name: String,
    /// The real event or pointer stimulus to measure.
    pub action: GlueBenchmarkAction,
    /// Prevents an old screen's ready state from satisfying a pending fade.
    pub expected_screen: GlueBenchmarkScreen,
}

/// Raw frame intervals for one action, including work after scene publication.
#[derive(Clone, Debug)]
pub struct GlueBenchmarkResult {
    /// Identity copied from the requested step.
    pub name: String,
    /// Time spent delivering the action, before the first frame service.
    pub action_duration: Duration,
    /// Action start to a complete requested scene with its current audio admitted.
    pub ready_duration: Duration,
    /// Present intervals through the first complete scene; the first includes input.
    pub transition_frames: Vec<Duration>,
    /// Following present intervals, retaining deferred GPU waits and UI fades.
    pub following_frames: Vec<Duration>,
}

/// A diagnostic cannot establish timing evidence for the requested interaction.
#[derive(Debug, Error)]
pub enum GlueBenchmarkError {
    /// Production archive, worker, UI, renderer, or audio processing failed.
    #[error(transparent)]
    Application(#[from] ApplicationError),
    /// The benchmark requires a fresh login scene without movie or world ownership.
    #[error("glue benchmark requires a fresh login scene without an active movie or world")]
    InitialScene,
    /// Pointer routing could not activate the exact named visible button.
    #[error("glue benchmark button {name} is absent, hidden, or did not receive its click")]
    Button {
        /// Exact GlueXML global name requested by the diagnostic.
        name: String,
    },
    /// A scene did not finish its real resource publication before the deadline.
    #[error("glue benchmark step {name} did not become ready within {timeout_ms} ms")]
    Timeout {
        /// Caller-assigned step identity.
        name: String,
        /// Allowed action-to-ready interval in milliseconds.
        timeout_ms: u128,
    },
    /// The production owner contained a character error; a backdrop alone is insufficient.
    #[error("glue benchmark step {name} failed character preparation")]
    CharacterPreparation {
        /// Caller-assigned step whose complete character failed.
        name: String,
    },
    /// Closing the diagnostic window cancels work without hiding an incomplete result.
    #[error("glue benchmark was cancelled by a platform termination event")]
    Cancelled,
}

impl ClientServices {
    /// Runs an offline replay without polling or initiating server operations.
    pub(in crate::application) fn benchmark_glue_steps(
        &mut self,
        steps: &[GlueBenchmarkStep],
        following_frame_count: NonZeroUsize,
        timeout: Duration,
    ) -> Result<Vec<GlueBenchmarkResult>, GlueBenchmarkError> {
        if self.gameplay.world().is_some()
            || self.glue.current_screen() != "login"
            || self.glue.media_intent().movie().is_some()
        {
            return Err(GlueBenchmarkError::InitialScene);
        }
        self.glue
            .set_network_status(UiGlueNetworkStatus::new(None, true));
        self.glue
            .set_character_creation_expansion(UiCharacterExpansion::WRATH_OF_THE_LICH_KING);
        let mut results = Vec::with_capacity(steps.len());
        for step in steps {
            tracing::info!(step = %step.name, "started Glue benchmark step");
            let started = Instant::now();
            self.apply_benchmark_action(&step.action)?;
            let action_duration = started.elapsed();
            let mut last_present = started;
            let mut transition_frames = Vec::new();
            loop {
                self.benchmark_frame()?;
                let presented = Instant::now();
                transition_frames.push(presented.duration_since(last_present));
                last_present = presented;
                if self.player.glue_character_request_failed() {
                    return Err(GlueBenchmarkError::CharacterPreparation {
                        name: step.name.clone(),
                    });
                }
                if self.last_glue_model_poll == RuntimeGlueModelPoll::Ready
                    && self.sound.glue_media_ready()
                    && self.glue.current_screen() == step.expected_screen.token()
                    && self.presented_glue_screen.as_deref() == Some(step.expected_screen.token())
                {
                    break;
                }
                if started.elapsed() >= timeout {
                    return Err(GlueBenchmarkError::Timeout {
                        name: step.name.clone(),
                        timeout_ms: timeout.as_millis(),
                    });
                }
            }
            let ready_duration = last_present.duration_since(started);
            let mut following_frames = Vec::with_capacity(following_frame_count.get());
            for _ in 0..following_frame_count.get() {
                self.benchmark_frame()?;
                let presented = Instant::now();
                following_frames.push(presented.duration_since(last_present));
                last_present = presented;
            }
            tracing::info!(step = %step.name, action_ms = action_duration.as_secs_f64() * 1000.0,
                ready_ms = ready_duration.as_secs_f64() * 1000.0,
                transition_frames = transition_frames.len(),
                transition_max_ms = transition_frames.iter().max().copied().unwrap_or_default().as_secs_f64() * 1000.0,
                following_fps = following_frames.len() as f64 / following_frames.iter().map(Duration::as_secs_f64).sum::<f64>(),
                following_max_ms = following_frames.iter().max().copied().unwrap_or_default().as_secs_f64() * 1000.0,
                "finished Glue benchmark step");
            results.push(GlueBenchmarkResult {
                name: step.name.clone(),
                action_duration,
                ready_duration,
                transition_frames,
                following_frames,
            });
        }
        Ok(results)
    }

    /// Delivers only fixture events and actual button clicks, preserving Lua callbacks.
    fn apply_benchmark_action(
        &mut self,
        action: &GlueBenchmarkAction,
    ) -> Result<(), GlueBenchmarkError> {
        match action {
            GlueBenchmarkAction::Sequence(actions) => {
                for action in actions {
                    self.apply_benchmark_action(action)?;
                }
            }
            GlueBenchmarkAction::Screen(screen) => {
                let payload =
                    UiEventPayload::new([UiEventArgument::String(screen.token().to_owned())])
                        .map_err(ApplicationError::from)?;
                self.glue
                    .dispatch_event("SET_GLUE_SCREEN", &payload)
                    .map_err(ApplicationError::from)?;
            }
            GlueBenchmarkAction::Directory(directory) => {
                let count = directory.characters().len() as i64;
                self.glue.set_character_directory(directory.clone());
                let payload = UiEventPayload::new([UiEventArgument::Integer(count)])
                    .map_err(ApplicationError::from)?;
                self.glue
                    .dispatch_event("CHARACTER_LIST_UPDATE", &payload)
                    .map_err(ApplicationError::from)?;
            }
            GlueBenchmarkAction::Click(name) => {
                let failure = || GlueBenchmarkError::Button { name: name.clone() };
                let index = self
                    .glue
                    .objects()
                    .iter()
                    .position(|object| object.name() == Some(name.as_str()))
                    .ok_or_else(failure)?;
                let region = self
                    .glue
                    .geometry()
                    .region(index)
                    .filter(|region| region.effectively_shown() && region.effective_alpha() > 0.0)
                    .ok_or_else(failure)?;
                let bounds = region.presentation_bounds();
                let position = (
                    (bounds.left() + bounds.right()) * 0.5,
                    (bounds.bottom() + bounds.top()) * 0.5,
                );
                self.glue
                    .pointer_motion(position)
                    .map_err(ApplicationError::from)?;
                self.glue
                    .pointer_button(position, UiPointerButton::Left, true)
                    .map_err(ApplicationError::from)?;
                let release = self
                    .glue
                    .pointer_button(position, UiPointerButton::Left, false)
                    .map_err(ApplicationError::from)?;
                if release.object_index() != Some(index) || !release.click_activated() {
                    return Err(failure());
                }
            }
        }
        self.glue_ui_dirty = true;
        self.last_glue_model_poll = RuntimeGlueModelPoll::Pending;
        Ok(())
    }

    /// Services the real renderer and local selection dispatch while keeping networking offline.
    fn benchmark_frame(&mut self) -> Result<(), GlueBenchmarkError> {
        let mut profile = RuntimeFrameProfile::new("benchmark frame");
        for _ in 0..run::MAX_PLATFORM_EVENTS_PER_FRAME {
            let Some(event) = self.poll_platform_event() else {
                break;
            };
            if run::exit_reason(&event, self.platform.window_id().value()).is_some() {
                return Err(GlueBenchmarkError::Cancelled);
            }
            // SDL retains native window state during polling. Diagnostic
            // stimuli above exclusively own UI input during the replay.
        }
        profile.mark("platform events");
        while let Some(action) = self.glue.take_network_action() {
            if let UiGlueNetworkAction::SelectCharacter { index } = action {
                let payload = UiEventPayload::new([UiEventArgument::Integer(i64::from(index))])
                    .map_err(ApplicationError::from)?;
                self.glue
                    .dispatch_event("UPDATE_SELECTED_CHARACTER", &payload)
                    .map_err(ApplicationError::from)?;
                self.glue_ui_dirty = true;
            }
        }
        profile.mark("selection dispatch");
        self.present_frame()?;
        profile.mark("application present");
        Ok(())
    }
}
