//! Main-thread ownership of authored movie timing and frame presentation.

use std::time::{Duration, Instant};

use solarity_media::{CinematicDecoder, CinematicError, CinematicVideoFrame};
use solarity_rendering::{VulkanError, VulkanRenderer};
use solarity_ui::UiGlueMovieRequest;

/// One result from synchronizing the active Glue movie request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RuntimeCinematicPoll {
    /// Glue currently requests no movie.
    Idle,
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
    pending: Option<CinematicVideoFrame>,
    started_at: Instant,
    first_presentation_time: Duration,
    last_interval: Duration,
    end_time: Option<Duration>,
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
    ) -> Result<RuntimeCinematicPoll, RuntimeCinematicError> {
        let Some(request) = request else {
            self.active = None;
            return Ok(RuntimeCinematicPoll::Idle);
        };
        let replacement_required = self
            .active
            .as_ref()
            .is_none_or(|active| active.generation != request.generation());
        if replacement_required {
            self.active = Some(ActiveCinematic::open(request)?);
        }
        let active = self.active.as_mut().ok_or(RuntimeCinematicError::State)?;
        let elapsed = active.started_at.elapsed();
        active.advance(elapsed)?;
        if active.end_time.is_some_and(|end_time| elapsed >= end_time) {
            let object_index = active.object_index;
            self.active = None;
            return Ok(RuntimeCinematicPoll::Finished { object_index });
        }
        renderer.present_rgba8(
            (active.current.width(), active.current.height()),
            active.current.rgba8(),
        )?;
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
            pending,
            started_at: Instant::now(),
            first_presentation_time,
            last_interval,
            end_time: None,
        })
    }

    fn advance(&mut self, elapsed: Duration) -> Result<(), RuntimeCinematicError> {
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
            self.pending = self.decoder.next_video_frame()?;
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
    /// Coordinator ownership was internally inconsistent.
    #[error("cinematic coordinator lost its active movie state")]
    State,
}
