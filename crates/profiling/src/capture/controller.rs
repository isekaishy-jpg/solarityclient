//! F10 start/stop is asynchronous; only application shutdown joins a running writer.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::scope::ACTIVE;

/// One application's diagnostic writer and profile output directory.
pub struct Capture {
    directory: PathBuf,
    identity: String,
    running: Option<Running>,
    stopping: bool,
}

/// The writer is owned until completion; stopping never detaches disk work.
struct Running {
    epoch: u64,
    path: PathBuf,
    stop: SyncSender<()>,
    thread: JoinHandle<io::Result<()>>,
}

impl Capture {
    /// Configures output without allocating measurement buffers or starting a thread.
    pub fn new(profile_root: &Path, identity: String) -> Self {
        Self {
            directory: profile_root.join("Profiles"),
            identity,
            running: None,
            stopping: false,
        }
    }

    /// Starts recording or requests the current writer's final snapshot.
    ///
    /// # Errors
    /// Returns filesystem/thread failures or a still-finishing previous capture.
    pub fn toggle(&mut self) -> io::Result<(bool, PathBuf)> {
        if self.stopping {
            if let Some(result) = self.poll() {
                result?;
            }
            if self.running.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "previous profile capture is still finishing",
                ));
            }
        }
        if let Some(running) = &self.running {
            let _ = ACTIVE.compare_exchange(running.epoch, 0, Ordering::Relaxed, Ordering::Relaxed);
            let _ = running.stop.try_send(());
            self.stopping = true;
            return Ok((false, running.path.clone()));
        }
        if crate::enabled() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "another profile capture is active",
            ));
        }
        std::fs::create_dir_all(&self.directory)?;
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let epoch = NEXT.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_millis();
        let path = self
            .directory
            .join(format!("capture-{timestamp}-{epoch}.csv"));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let (stop, receiver) = sync_channel(1);
        let identity = self.identity.clone();
        let output = path.clone();
        ACTIVE
            .compare_exchange(0, epoch, Ordering::Relaxed, Ordering::Relaxed)
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "another profile capture is active",
                )
            })?;
        let thread = match std::thread::Builder::new()
            .name("solarity-profile-writer".to_owned())
            .spawn(move || {
                let result = super::writer::run(epoch, file, &output, &identity, receiver);
                let _ = ACTIVE.compare_exchange(epoch, 0, Ordering::Relaxed, Ordering::Relaxed);
                result
            }) {
            Ok(thread) => thread,
            Err(error) => {
                ACTIVE.store(0, Ordering::Relaxed);
                return Err(error);
            }
        };
        self.running = Some(Running {
            epoch,
            path: path.clone(),
            stop,
            thread,
        });
        Ok((true, path))
    }

    /// Takes a completed report result without waiting in the frame loop.
    pub fn poll(&mut self) -> Option<io::Result<PathBuf>> {
        if !self
            .running
            .as_ref()
            .is_some_and(|running| running.thread.is_finished())
        {
            return None;
        }
        let running = self.running.take()?;
        self.stopping = false;
        Some(
            running
                .thread
                .join()
                .map_err(|_| io::Error::other("profile writer panicked"))
                .and_then(|result| result)
                .map(|()| running.path),
        )
    }

    /// Finishes all diagnostic ownership during application shutdown, not presentation.
    ///
    /// # Errors
    /// Returns the writer's final file error or thread failure.
    pub fn shutdown(&mut self) -> io::Result<()> {
        let Some(running) = self.running.take() else {
            return Ok(());
        };
        let _ = ACTIVE.compare_exchange(running.epoch, 0, Ordering::Relaxed, Ordering::Relaxed);
        let _ = running.stop.try_send(());
        self.stopping = false;
        running
            .thread
            .join()
            .map_err(|_| io::Error::other("profile writer panicked"))?
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            tracing::warn!(%error, "could not finish profile capture");
        }
    }
}
