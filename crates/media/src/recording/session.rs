//! One owned encoder thread exists only during an explicit recording session.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::encoder::RecordingEncoder;
use super::files::{RecordingFiles, RecordingMode};
use super::{RecordingAudio, RecordingError};

const RUNNING: u64 = u64::MAX;
const VIDEO_BUFFERS: usize = 3;

/// Ownership of one reusable BGRA buffer crosses the worker boundary with capture time.
struct VideoSample {
    timestamp: Duration,
    pixels: Vec<u8>,
}

/// Completed recording facts; automated paths contain a segment-number pattern.
#[derive(Debug)]
pub struct RecordingReport {
    path: PathBuf,
    video_frames: u64,
    dropped_audio_frames: u64,
}

impl RecordingReport {
    /// Output file, or the eight-file automated segment pattern.
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Number of encoded 30 FPS frames, including holds across missed samples.
    pub const fn video_frames(&self) -> u64 {
        self.video_frames
    }
    /// Source audio frames replaced with silence because capture was congested.
    pub const fn dropped_audio_frames(&self) -> u64 {
        self.dropped_audio_frames
    }
}

/// Bounded producer interface. GPU readback and mixer lifetime stay with their
/// existing owners; this worker owns codecs, conversion, muxing, and file I/O.
pub struct VideoRecorder {
    started: Instant,
    audio: Arc<RecordingAudio>,
    ready: Arc<AtomicBool>,
    stop_at: Arc<AtomicU64>,
    input: Option<SyncSender<VideoSample>>,
    recycled: Receiver<Vec<u8>>,
    worker: Option<JoinHandle<Result<RecordingReport, RecordingError>>>,
}

impl VideoRecorder {
    /// Starts asynchronous hardware H.264/AAC recording with three reusable
    /// BGRA buffers. Only stereo game output is accepted by this recording API.
    ///
    /// # Errors
    /// Returns invalid extent/rate or thread-creation errors immediately. Codec
    /// and filesystem startup failures are returned later by `poll`.
    pub fn start(
        directory: &Path,
        mode: RecordingMode,
        extent: (u32, u32),
        audio_rate: u32,
    ) -> Result<Self, RecordingError> {
        if extent.0 < 2
            || extent.1 < 2
            || extent.0 > 1280
            || extent.1 > 720
            || !extent.0.is_multiple_of(2)
            || !extent.1.is_multiple_of(2)
            || !matches!(
                audio_rate,
                8_000
                    | 11_025
                    | 12_000
                    | 16_000
                    | 22_050
                    | 24_000
                    | 32_000
                    | 44_100
                    | 48_000
                    | 64_000
                    | 88_200
                    | 96_000
            )
        {
            return Err(RecordingError::new(
                "recording requires even dimensions up to 720p and a supported AAC sample rate",
            ));
        }
        let started = Instant::now();
        let audio = Arc::new(RecordingAudio::new(started, audio_rate));
        let ready = Arc::new(AtomicBool::new(false));
        let stop_at = Arc::new(AtomicU64::new(RUNNING));
        let (input, incoming) = mpsc::sync_channel::<VideoSample>(VIDEO_BUFFERS - 1);
        let (return_buffer, recycled) = mpsc::sync_channel(VIDEO_BUFFERS);
        for _ in 0..VIDEO_BUFFERS {
            return_buffer
                .send(vec![0; extent.0 as usize * extent.1 as usize * 4])
                .map_err(RecordingError::new)?;
        }
        let directory = directory.to_owned();
        let worker_audio = Arc::clone(&audio);
        let worker_ready = Arc::clone(&ready);
        let worker_stop = Arc::clone(&stop_at);
        let worker = thread::Builder::new().name("video-recorder".to_owned()).spawn(move || {
            let files = RecordingFiles::create(&directory, mode)?;
            let mut encoder = RecordingEncoder::create(&files, extent, audio_rate)?;
            tracing::info!(path = %files.path.display(), width = extent.0, height = extent.1, fps = 30,
                "started hardware video recording");
            worker_ready.store(true, Ordering::Release);
            let mut maintenance_at = Duration::ZERO;
            loop {
                let disconnected = match incoming.recv_timeout(Duration::from_millis(10)) {
                    Ok(sample) => {
                        let result = encoder.image(&sample.pixels, sample.timestamp);
                        let _returned = return_buffer.try_send(sample.pixels);
                        result?;
                        false
                    }
                    Err(RecvTimeoutError::Timeout) => false,
                    Err(RecvTimeoutError::Disconnected) => true,
                };
                if worker_audio.changed_format() {
                    encoder.finish(started.elapsed())?;
                    return Err(RecordingError::new("game audio format changed; recording was stopped"));
                }
                let requested = worker_stop.load(Ordering::Acquire);
                let elapsed = if requested == RUNNING { started.elapsed() } else { Duration::from_nanos(requested) };
                if encoder.video_frames > 0 {
                    while let Some(block) = worker_audio.pop()? { encoder.audio(block)?; }
                    // Allow asynchronous readback a short delivery window before
                    // holding the previous image across a genuinely missed sample.
                    let through = elapsed.saturating_sub(Duration::from_millis(100));
                    encoder.video_until((through.as_nanos() * 30 / 1_000_000_000) as u64)?;
                    encoder.silence_until((through.as_nanos() * u128::from(audio_rate) / 1_000_000_000) as u64)?;
                }
                if elapsed >= maintenance_at {
                    files.maintain(Some(encoder.active_segment))?;
                    maintenance_at = elapsed + Duration::from_secs(1);
                }
                if disconnected {
                    if encoder.video_frames == 0 {
                        return Err(RecordingError::new("recording stopped before a game frame was captured"));
                    }
                    encoder.finish(elapsed)?;
                    files.maintain(Some(encoder.active_segment))?;
                    return Ok(RecordingReport { path: files.path.clone(), video_frames: encoder.video_frames,
                        dropped_audio_frames: worker_audio.dropped_frames() });
                }
            }
        })?;
        Ok(Self {
            started,
            audio,
            ready,
            stop_at,
            input: Some(input),
            recycled,
            worker: Some(worker),
        })
    }

    /// Audio callback sink, retained until the mixer explicitly detaches it.
    pub fn audio(&self) -> Arc<RecordingAudio> {
        Arc::clone(&self.audio)
    }
    /// Monotonic recording time shared with video and audio timestamps.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
    /// Whether codecs and the output file have finished opening successfully.
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
    /// Takes an available pixel buffer without allocating or waiting.
    pub fn take_buffer(&self) -> Option<Vec<u8>> {
        self.recycled.try_recv().ok()
    }

    /// Offers a completed image. A rejected buffer is returned for caller reuse;
    /// capture pressure never blocks the game waiting for encoder throughput.
    pub fn submit(&self, pixels: Vec<u8>, timestamp: Duration) -> Option<Vec<u8>> {
        let Some(input) = self.input.as_ref() else {
            return Some(pixels);
        };
        match input.try_send(VideoSample { timestamp, pixels }) {
            Ok(()) => None,
            Err(TrySendError::Full(sample) | TrySendError::Disconnected(sample)) => {
                Some(sample.pixels)
            }
        }
    }

    /// Ends input and lets the worker finish queued frames and close the file.
    /// Detach the game's audio callback before calling this method.
    pub fn stop(&mut self) {
        if self.input.is_some() {
            self.stop_at.store(
                self.elapsed().as_nanos().min(u128::from(u64::MAX - 1)) as u64,
                Ordering::Release,
            );
            self.input = None;
        }
    }

    /// Collects completion without waiting. A finished worker is joined once.
    pub fn poll(&mut self) -> Option<Result<RecordingReport, RecordingError>> {
        if !self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            return None;
        }
        Some(
            self.worker
                .take()?
                .join()
                .unwrap_or_else(|_| Err(RecordingError::new("encoder worker panicked"))),
        )
    }

    /// Finishes and joins during orderly process shutdown.
    ///
    /// # Errors
    /// Returns codec, file, or worker failures, including asynchronous startup errors.
    pub fn finish(mut self) -> Result<RecordingReport, RecordingError> {
        self.stop();
        self.worker
            .take()
            .ok_or_else(|| RecordingError::new("recording was already collected"))?
            .join()
            .unwrap_or_else(|_| Err(RecordingError::new("encoder worker panicked")))
    }
}

impl Drop for VideoRecorder {
    fn drop(&mut self) {
        self.stop();
        if let Some(worker) = self.worker.take() {
            match worker.join() {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => tracing::warn!(%error, "could not finish dropped video recorder"),
                Err(_) => tracing::warn!("dropped video recorder worker panicked"),
            }
        }
    }
}
