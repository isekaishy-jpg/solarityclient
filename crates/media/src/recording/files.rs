//! Manual recordings are preserved; automated recordings use an eight-file ring.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::RecordingError;

const AUTOMATED_BYTES: u64 = 256 * 1024 * 1024;
const WRITE_RESERVE: u64 = 8 * 1024 * 1024;
pub(super) const AUTOMATED_FILES: u64 = 8;
pub(super) const SEGMENT_SECONDS: u64 = 60;

/// Selects user-owned recordings or disposable, rotated automated-check evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingMode {
    /// Unique files directly in Videos; never removed by automatic retention.
    Manual,
    /// Sixty-second segments in Videos/Automated, at most eight retained files.
    Automated,
}

/// Output naming and exclusive automated-folder ownership for one encoder session.
pub(super) struct RecordingFiles {
    pub(super) path: PathBuf,
    pub(super) mode: RecordingMode,
    pub(super) first_segment: u64,
    directory: PathBuf,
    _lock: Option<File>,
}

impl RecordingFiles {
    /// Reserve a manual name or lock and prune the automated segment ring.
    pub(super) fn create(root: &Path, mode: RecordingMode) -> Result<Self, RecordingError> {
        let directory = if mode == RecordingMode::Automated {
            root.join("Automated")
        } else {
            root.to_owned()
        };
        fs::create_dir_all(&directory)?;
        let directory = directory.canonicalize()?;
        if mode == RecordingMode::Automated {
            let lock = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(directory.join(".recording.lock"))?;
            lock.try_lock().map_err(|error| {
                RecordingError::new(format!("automated recording folder is busy: {error}"))
            })?;
            let mut files = Self {
                path: directory.join("check-%02d.mp4"),
                directory,
                mode,
                first_segment: 0,
                _lock: Some(lock),
            };
            files.maintain(None)?;
            files.first_segment = files.next_segment()?;
            Ok(files)
        } else {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(RecordingError::new)?
                .as_millis();
            for suffix in 0..1000 {
                let path = directory.join(format!(
                    "video-{timestamp}-{}-{suffix}.mp4",
                    std::process::id()
                ));
                match OpenOptions::new().write(true).create_new(true).open(&path) {
                    Ok(_) => {
                        return Ok(Self {
                            path,
                            directory,
                            mode,
                            first_segment: 0,
                            _lock: None,
                        });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            Err(RecordingError::new(
                "could not reserve a unique recording filename",
            ))
        }
    }

    /// Continue rotation across separate short automated runs, choosing a free
    /// slot first and otherwise replacing the oldest owned clip.
    fn next_segment(&self) -> Result<u64, RecordingError> {
        let mut oldest = None;
        for index in 0..AUTOMATED_FILES {
            let path = self.directory.join(format!("check-{index:02}.mp4"));
            match fs::metadata(path) {
                Ok(metadata) => {
                    let candidate = (metadata.modified()?, index);
                    if oldest.is_none_or(|previous| candidate < previous) {
                        oldest = Some(candidate);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(index),
                Err(error) => return Err(error.into()),
            }
        }
        Ok(oldest.map_or(0, |(_, index)| index))
    }

    /// Only the exact eight owned filenames can be pruned. Current output is
    /// retained, and write headroom bounds the next packet and mux finalization.
    pub(super) fn maintain(&self, active: Option<u64>) -> Result<(), RecordingError> {
        if self.mode != RecordingMode::Automated {
            return Ok(());
        }
        let mut retained = Vec::with_capacity(AUTOMATED_FILES as usize);
        let mut total = 0_u64;
        for index in 0..AUTOMATED_FILES {
            let path = self.directory.join(format!("check-{index:02}.mp4"));
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            if !metadata.file_type().is_file() {
                return Err(RecordingError::new(
                    "automated recording target is not a regular file",
                ));
            }
            total = total.saturating_add(metadata.len());
            retained.push((metadata.modified()?, index, path, metadata.len()));
        }
        retained.sort_by_key(|entry| (entry.0, entry.1));
        for (_, index, path, bytes) in retained {
            if total <= AUTOMATED_BYTES - WRITE_RESERVE {
                break;
            }
            if Some(index) == active {
                continue;
            }
            fs::remove_file(path)?;
            total = total.saturating_sub(bytes);
        }
        if total > AUTOMATED_BYTES - WRITE_RESERVE {
            return Err(RecordingError::new(
                "automated recording reached its disk-space budget",
            ));
        }
        Ok(())
    }
}
