//! Explicit F9 recording and opt-in, rotated video evidence for automated runs.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use solarity_media::{RecordingError, RecordingMode, RecordingReport, VideoRecorder};
use solarity_rendering::VulkanRenderer;

use crate::application::sound_coordinator::RuntimeSoundCoordinator;
use crate::input::stock_keyboard_name;
use crate::platform::{ButtonState, PlatformEvent, WindowId};

/// Application-owned capture state; completion and GPU retirement progress independently.
struct ActiveRecording {
    recorder: VideoRecorder,
    buffer: Option<Vec<u8>>,
    next_sample: u64,
    stopping: bool,
    stop_sent: bool,
    audio_attached: bool,
    completion: Option<Result<RecordingReport, RecordingError>>,
    missed_samples: u64,
    capture_failed: bool,
}

/// Explicit recording controller and short-lived user status, absent from normal frame work.
pub(in crate::application) struct RuntimeRecording {
    directory: PathBuf,
    automatic_pending: bool,
    active: Option<ActiveRecording>,
    notice: Option<(&'static str, Instant)>,
}

impl RuntimeRecording {
    /// Retain only output configuration until a toggle or automated startup requests recording.
    pub(in crate::application) fn new(profile_root: &Path, automated: bool) -> Self {
        Self {
            directory: profile_root.join("Videos"),
            automatic_pending: automated,
            active: None,
            notice: None,
        }
    }

    /// Both edges and repeats are consumed so recording never invokes a game
    /// binding; only a fresh press toggles the session.
    pub(in crate::application) fn service_event(
        &mut self,
        event: &PlatformEvent,
        window: WindowId,
        renderer: &mut VulkanRenderer,
        sound: &RuntimeSoundCoordinator,
    ) -> bool {
        let PlatformEvent::Key(key) = event else {
            return false;
        };
        if key.window_id != window || key.scan_code.and_then(stock_keyboard_name) != Some("F9") {
            return false;
        }
        if key.state == ButtonState::Pressed && !key.is_repeat {
            if let Some(active) = self.active.as_mut() {
                active.stopping = true;
            } else {
                self.start(renderer, sound, RecordingMode::Manual);
            }
        }
        true
    }

    /// Assemble GPU capture and game audio around an asynchronously opening encoder.
    fn start(
        &mut self,
        renderer: &mut VulkanRenderer,
        sound: &RuntimeSoundCoordinator,
        mode: RecordingMode,
    ) {
        let result = (|| {
            let info = sound.output_info();
            if info.channel_count() != 2 {
                return Err("recording requires stereo game output".to_owned());
            }
            let extent = renderer
                .begin_video_capture()
                .map_err(|error| error.to_string())?;
            let recorder =
                match VideoRecorder::start(&self.directory, mode, extent, info.sample_rate_hz()) {
                    Ok(recorder) => recorder,
                    Err(error) => {
                        let _ended = renderer.end_video_capture();
                        return Err(error.to_string());
                    }
                };
            let mut active = ActiveRecording {
                recorder,
                buffer: None,
                next_sample: 0,
                stopping: false,
                stop_sent: false,
                audio_attached: false,
                completion: None,
                missed_samples: 0,
                capture_failed: false,
            };
            match sound.set_recording_audio(Some(active.recorder.audio())) {
                Ok(()) => active.audio_attached = true,
                Err(error) => {
                    active.stopping = true;
                    active.capture_failed = true;
                    active.stop_sent = true;
                    active.recorder.stop();
                    let _ended = renderer.end_video_capture();
                    self.failure(&error.to_string());
                }
            }
            self.active = Some(active);
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            self.failure(&error);
        }
    }

    /// The inactive path only checks optional state; no clock, GPU, audio, or
    /// worker work is performed until capture is explicitly enabled.
    pub(in crate::application) fn poll(
        &mut self,
        renderer: &mut VulkanRenderer,
        sound: &RuntimeSoundCoordinator,
    ) {
        if self.automatic_pending {
            self.automatic_pending = false;
            if self.active.is_none() {
                self.start(renderer, sound, RecordingMode::Automated);
            }
        }
        if self
            .notice
            .is_some_and(|(_, until)| Instant::now() >= until)
        {
            self.notice = None;
        }
        let Some(active) = self.active.as_mut() else {
            return;
        };
        if let Some(completion) = active.recorder.poll() {
            active.completion = Some(completion);
            active.stopping = true;
        }
        let step = (|| {
            if active.capture_failed {
                return Ok(());
            }
            if active.stopping && active.audio_attached {
                sound
                    .set_recording_audio(None)
                    .map_err(|error| error.to_string())?;
                active.audio_attached = false;
            }
            if active.buffer.is_none() {
                active.buffer = active.recorder.take_buffer();
            }
            if let Some(buffer) = active.buffer.as_mut()
                && let Some(timestamp) = renderer
                    .poll_video_frame(buffer)
                    .map_err(|error| error.to_string())?
            {
                let pixels = active.buffer.take().ok_or("recording buffer disappeared")?;
                active.buffer = active.recorder.submit(pixels, timestamp);
                if active.buffer.is_some() {
                    active.missed_samples += 1;
                }
            }
            if active.stopping {
                if renderer
                    .end_video_capture()
                    .map_err(|error| error.to_string())?
                    && !active.stop_sent
                {
                    active.recorder.stop();
                    active.stop_sent = true;
                }
                return Ok(());
            }
            // Queue the first images while the hardware encoder opens. Its
            // bounded input already exists, so a short recording can still
            // finish with video even if codec startup takes most of its duration.
            {
                let elapsed = active.recorder.elapsed();
                let sample = (elapsed.as_nanos() * 30 / 1_000_000_000) as u64;
                if sample >= active.next_sample {
                    if active.buffer.is_none() {
                        active.buffer = active.recorder.take_buffer();
                    }
                    if active.buffer.is_none()
                        || !renderer
                            .request_video_frame(elapsed)
                            .map_err(|error| error.to_string())?
                    {
                        active.missed_samples += 1;
                    }
                    active.next_sample = sample.saturating_add(1);
                }
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = step {
            active.stopping = true;
            active.capture_failed = true;
            active.stop_sent = true;
            if active.audio_attached && sound.set_recording_audio(None).is_ok() {
                active.audio_attached = false;
            }
            active.recorder.stop();
            let _ended = renderer.end_video_capture();
            // The renderer retains in-flight storage until fence retirement or
            // its normal device-idle teardown; a capture failure cannot free it early.
            self.failure(&error);
            return;
        }
        if active.stop_sent
            && let Some(result) = active.completion.take()
        {
            let missed = active.missed_samples;
            let failed = active.capture_failed;
            self.active = None;
            match result {
                Ok(report) if !failed => {
                    tracing::info!(path = %report.path().display(), frames = report.video_frames(),
                        missed_capture_samples = missed, dropped_audio_frames = report.dropped_audio_frames(), "saved video recording");
                    self.notice = Some(("VIDEO SAVED", Instant::now() + Duration::from_secs(3)));
                }
                Ok(_) => {
                    self.notice = Some(("VIDEO FAILED", Instant::now() + Duration::from_secs(5)))
                }
                Err(error) => self.failure(&error.to_string()),
            }
        }
    }

    /// Report capture failure without turning a diagnostic feature into a gameplay failure.
    fn failure(&mut self, message: &str) {
        tracing::warn!(error = message, "video recording stopped");
        self.notice = Some(("VIDEO FAILED", Instant::now() + Duration::from_secs(5)));
    }

    pub(in crate::application) fn status(&self) -> Option<&'static str> {
        if let Some(active) = self.active.as_ref() {
            Some(if active.stopping {
                "SAVING"
            } else if active.recorder.is_ready() {
                "REC"
            } else {
                "STARTING"
            })
        } else {
            self.notice.map(|(text, _)| text)
        }
    }

    /// Detach the mixer before joining the encoder during orderly application shutdown.
    pub(in crate::application) fn shutdown(&mut self, sound: &RuntimeSoundCoordinator) {
        if let Some(active) = self.active.take() {
            if active.audio_attached
                && let Err(error) = sound.set_recording_audio(None)
            {
                tracing::warn!(%error, "could not detach recording audio during shutdown");
            }
            match active.recorder.finish() {
                Ok(report) => {
                    tracing::info!(path = %report.path().display(), frames = report.video_frames(), "saved video at shutdown")
                }
                Err(error) => tracing::warn!(%error, "could not finish video at shutdown"),
            }
        }
    }
}
