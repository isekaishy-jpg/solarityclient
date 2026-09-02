//! Main-thread ownership of authored movie timing and frame presentation.

use std::time::{Duration, Instant};

use solarity_media::{CinematicDecoder, CinematicError, CinematicVideoFrame};
use solarity_rendering::{CinematicFrameIdentity, UiPreparedDraw, VulkanError, VulkanRenderer};
use solarity_ui::UiGlueMovieRequest;

use super::sound_coordinator::{RuntimeSoundCoordinator, RuntimeSoundError};

/// One result from synchronizing the active Glue movie request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RuntimeCinematicPoll {
    /// Glue currently requests no movie.
    Idle,
    /// Glue withdrew a movie request after at least one cinematic frame.
    Stopped,
    /// A decoded frame was presented for the active movie.
    Presented,
    /// The final frame duration elapsed and the owning widget must be notified.
    Finished { object_index: usize },
}

/// Active decoder and presentation clock for one `StartMovie` generation.
struct ActiveCinematic {
    generation: u64,
    object_index: usize,
    decoder: CinematicDecoder,
    current: CinematicVideoFrame,
    frame_index: u64,
    pending: Option<CinematicVideoFrame>,
    started_at: Instant,
    first_presentation_time: Duration,
    last_interval: Duration,
    end_time: Option<Duration>,
    volume: u32,
    audio_started: bool,
}

/// Synchronizes Glue's one active MovieFrame with FFmpeg and Vulkan.
#[derive(Default)]
pub(super) struct RuntimeCinematicCoordinator {
    active: Option<ActiveCinematic>,
}

impl RuntimeCinematicCoordinator {
    /// Decodes and presents the frame selected by the authored movie clock.
    pub(super) fn synchronize(
        &mut self,
        request: Option<&UiGlueMovieRequest>,
        renderer: &mut VulkanRenderer,
        sound: &mut RuntimeSoundCoordinator,
        overlay: Option<([f32; 2], &[UiPreparedDraw])>,
    ) -> Result<RuntimeCinematicPoll, RuntimeCinematicError> {
        let Some(request) = request else {
            if let Some(active) = self.active.take() {
                sound.stop_cinematic_audio()?;
                tracing::info!(
                    object_index = active.object_index,
                    generation = active.generation,
                    "stopped Glue cinematic"
                );
                return Ok(RuntimeCinematicPoll::Stopped);
            }
            return Ok(RuntimeCinematicPoll::Idle);
        };
        let replacement_required = self
            .active
            .as_ref()
            .is_none_or(|active| active.generation != request.generation());
        if replacement_required {
            if self.active.take().is_some() {
                sound.stop_cinematic_audio()?;
            }
            let mut active = ActiveCinematic::open(request)?;
            active.feed_audio(sound)?;
            tracing::info!(
                path = %request.path().display(),
                object_index = request.object_index(),
                generation = request.generation(),
                width = active.current.width(),
                height = active.current.height(),
                "started Glue cinematic"
            );
            self.active = Some(active);
        }
        let active = self.active.as_mut().ok_or(RuntimeCinematicError::State)?;
        // Stock `CSimpleMovieFrame.cpp` update at 0x0095EBF0 reads the active
        // movie channel before falling back to its monotonic tick clock.
        let elapsed = if active.audio_started {
            sound
                .cinematic_playback_time()
                .unwrap_or_else(|| active.started_at.elapsed())
        } else {
            active.started_at.elapsed()
        };
        active.advance(elapsed, sound)?;
        if active.end_time.is_some_and(|end_time| elapsed >= end_time) {
            let object_index = active.object_index;
            self.active = None;
            sound.stop_cinematic_audio()?;
            tracing::info!(
                object_index,
                elapsed_ms = elapsed.as_millis(),
                "finished Glue cinematic"
            );
            return Ok(RuntimeCinematicPoll::Finished { object_index });
        }
        let source_extent = (active.current.width(), active.current.height());
        let identity = CinematicFrameIdentity::new(active.generation, active.frame_index);
        if let Some((logical_extent, draws)) = overlay {
            renderer.present_cinematic_rgba8_with_ui(
                identity,
                source_extent,
                active.current.rgba8(),
                logical_extent,
                draws,
            )?;
        } else {
            renderer.present_cinematic_rgba8(identity, source_extent, active.current.rgba8())?;
        }
        Ok(RuntimeCinematicPoll::Presented)
    }
}

impl ActiveCinematic {
    fn open(request: &UiGlueMovieRequest) -> Result<Self, CinematicError> {
        let mut decoder = CinematicDecoder::open(request.path())?;
        let current = decoder
            .next_video_frame()?
            .ok_or_else(|| CinematicError::EmptyVideo {
                path: request.path().to_path_buf(),
            })?;
        let first_presentation_time = current.presentation_time();
        let pending = decoder.next_video_frame()?;
        let last_interval = pending
            .as_ref()
            .and_then(|frame| {
                frame
                    .presentation_time()
                    .checked_sub(first_presentation_time)
            })
            .filter(|interval| !interval.is_zero())
            .unwrap_or(Duration::from_millis(16));
        Ok(Self {
            generation: request.generation(),
            object_index: request.object_index(),
            decoder,
            current,
            frame_index: 0,
            pending,
            started_at: Instant::now(),
            first_presentation_time,
            last_interval,
            end_time: None,
            volume: request.volume(),
            audio_started: false,
        })
    }

    fn advance(
        &mut self,
        elapsed: Duration,
        sound: &mut RuntimeSoundCoordinator,
    ) -> Result<(), RuntimeCinematicError> {
        while self.pending.as_ref().is_some_and(|frame| {
            frame
                .presentation_time()
                .saturating_sub(self.first_presentation_time)
                <= elapsed
        }) {
            let next = self.pending.take().ok_or(RuntimeCinematicError::State)?;
            let interval = next
                .presentation_time()
                .saturating_sub(self.current.presentation_time());
            if !interval.is_zero() {
                self.last_interval = interval;
            }
            self.current = next;
            self.frame_index = self
                .frame_index
                .checked_add(1)
                .ok_or(RuntimeCinematicError::FrameIndexCapacity)?;
            self.pending = self.decoder.next_video_frame()?;
            self.feed_audio(sound)?;
            if self.pending.is_none() {
                self.end_time = Some(
                    self.current
                        .presentation_time()
                        .saturating_sub(self.first_presentation_time)
                        .saturating_add(self.last_interval),
                );
            }
        }
        Ok(())
    }

    fn feed_audio(&mut self, sound: &mut RuntimeSoundCoordinator) -> Result<(), RuntimeSoundError> {
        for frame in self.decoder.take_audio_frames() {
            if frame.samples().is_empty() {
                continue;
            }
            if self.audio_started {
                sound.queue_cinematic_audio(frame.samples())?;
            } else {
                sound.start_cinematic_audio(frame.samples(), self.volume)?;
                self.audio_started = true;
            }
        }
        Ok(())
    }
}

/// A movie could not advance through its exact decode/presentation path.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeCinematicError {
    /// FFmpeg could not open or decode the selected stock movie.
    #[error(transparent)]
    Decode(#[from] CinematicError),
    /// Vulkan could not present a decoded movie frame.
    #[error(transparent)]
    Present(#[from] VulkanError),
    /// SDL could not start, feed, or stop the movie audio track.
    #[error(transparent)]
    Sound(#[from] RuntimeSoundError),
    /// Coordinator ownership was internally inconsistent.
    #[error("cinematic coordinator lost its active movie state")]
    State,
    /// A movie exceeded the representable authored frame identity.
    #[error("cinematic decoded frame index exceeds u64 capacity")]
    FrameIndexCapacity,
}
