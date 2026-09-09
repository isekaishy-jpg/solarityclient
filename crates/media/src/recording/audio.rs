//! Nonblocking, preallocated handoff from the mixer's final stereo output.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub(super) const AUDIO_BLOCK_FRAMES: usize = 1024;
const QUEUE_BLOCKS: usize = 64;

/// One preallocated stereo block, addressed by its original sample-clock position.
pub(super) struct AudioBlock {
    pub(super) first_frame: u64,
    pub(super) frames: usize,
    pub(super) samples: [f32; AUDIO_BLOCK_FRAMES * 2],
}

/// Recording-only audio sink. No callback is installed while recording is off.
/// A full or contended queue drops capture samples, never the playable audio.
pub struct RecordingAudio {
    started: Instant,
    rate: u32,
    queue: Mutex<VecDeque<AudioBlock>>,
    dropped_frames: AtomicU64,
    format_changed: AtomicBool,
}

impl RecordingAudio {
    /// Reserve callback storage before attaching to the real-time mixer.
    pub(super) fn new(started: Instant, rate: u32) -> Self {
        Self {
            started,
            rate,
            queue: Mutex::new(VecDeque::with_capacity(QUEUE_BLOCKS)),
            dropped_frames: AtomicU64::new(0),
            format_changed: AtomicBool::new(false),
        }
    }

    pub(crate) fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub(crate) fn rate(&self) -> u32 {
        self.rate
    }

    /// Source frames use a continuous device-sample clock anchored on attachment.
    /// Malformed or changed output formats end recording instead of corrupting sound.
    pub(crate) fn capture(&self, first_frame: u64, samples: &[f32], rate: i32, channels: i32) {
        if rate != self.rate as i32 || channels != 2 || !samples.len().is_multiple_of(2) {
            self.format_changed.store(true, Ordering::Relaxed);
            return;
        }
        let Ok(mut queue) = self.queue.try_lock() else {
            self.dropped_frames
                .fetch_add((samples.len() / 2) as u64, Ordering::Relaxed);
            return;
        };
        for (index, source) in samples.chunks(AUDIO_BLOCK_FRAMES * 2).enumerate() {
            if queue.len() == QUEUE_BLOCKS {
                self.dropped_frames
                    .fetch_add((source.len() / 2) as u64, Ordering::Relaxed);
                continue;
            }
            let mut block = AudioBlock {
                first_frame: first_frame + (index * AUDIO_BLOCK_FRAMES) as u64,
                frames: source.len() / 2,
                samples: [0.0; AUDIO_BLOCK_FRAMES * 2],
            };
            block.samples[..source.len()].copy_from_slice(source);
            queue.push_back(block);
        }
    }

    /// The encoder may wait for the short callback copy; the callback never waits.
    pub(super) fn pop(&self) -> Result<Option<AudioBlock>, super::RecordingError> {
        self.queue
            .lock()
            .map(|mut queue| queue.pop_front())
            .map_err(|_| super::RecordingError::new("recording audio queue was poisoned"))
    }

    pub(super) fn changed_format(&self) -> bool {
        self.format_changed.load(Ordering::Relaxed)
    }

    /// Number of source sample frames omitted under capture queue pressure.
    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames.load(Ordering::Relaxed)
    }
}
