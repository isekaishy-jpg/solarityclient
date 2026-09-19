//! Native readiness keeps input queued and consumes exact owned CPU results.

use super::{FrameWait, FrameWaitError};
use crate::configuration::{WindowConfiguration, WindowMode};
use crate::platform::{PlatformError, PlatformEvent, SdlPlatform};
use crate::test_support::SDL_TEST_LOCK;
use solarity_cpu::{CpuExecutor, CpuPoolConfig, CpuStoragePlan, FrameBatch};
use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::mpsc;
use std::time::Duration;

/// Source publication is controlled independently of the main readiness check.
struct Job {
    release: mpsc::Receiver<()>,
    value: usize,
}

impl Job {
    fn run(&mut self) {
        assert!(self.release.recv_timeout(Duration::from_secs(10)).is_ok());
        self.value = 42;
    }
}

#[test]
fn native_frame_readiness_preserves_input_and_publication_ownership() -> Result<(), Box<dyn Error>>
{
    let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock poisoned")?;
    let mut platform = SdlPlatform::start(WindowConfiguration::new(64, 64, WindowMode::Windowed))?;
    while platform.poll_event().is_some() {}
    let sdl = sdl3::init()?;
    let events = sdl.event()?;
    events.push_event(sdl3::event::Event::Quit { timestamp: 123 })?;
    let mut cpu = CpuExecutor::with_notifier(
        CpuPoolConfig::new(
            NonZeroUsize::MIN,
            NonZeroUsize::MIN,
            CpuStoragePlan::new(64 << 20, 64 << 20, 0),
        ),
        platform.coordinator_notifier(),
    )?;
    let (release, blocked) = mpsc::channel();
    let mut jobs = vec![Job {
        release: blocked,
        value: 0,
    }];
    let mut batch = FrameBatch::new(Job::run);
    assert!(batch.is_finished());
    batch.start(&cpu, &mut jobs)?;
    let handle = batch.job(0)?;
    assert!(!batch.is_finished());
    assert!(batch.outcome(&handle)?.is_none());
    let mut release = Some(release);
    platform.wait_until_ready(|| -> Result<bool, FrameWaitError> {
        if let Some(release) = release.take() {
            // Release after the native ticket is observed. Publication can race
            // directly with arm, but must not lose the durable completion.
            assert!(release.send(()).is_ok());
            return Ok(false);
        }
        Ok(batch.outcome(&handle)?.is_some())
    })?;
    let mut wait = FrameWait::Native(&mut platform);
    wait.before_result(&batch, &handle)?;
    let mut consumed = 0;
    assert_eq!(
        batch.with_result(&handle, |job| {
            consumed += 1;
            job.value
        })?,
        42
    );
    assert_eq!(consumed, 1);
    wait.before_reclaim(&batch)?;
    batch.reclaim(&mut jobs)?;
    assert!(batch.is_finished());
    assert_eq!(jobs[0].value, 42);
    let mut quits = 0;
    while let Some(event) = platform.poll_event() {
        if matches!(event.event, PlatformEvent::QuitRequested) {
            quits += 1;
        }
    }
    assert_eq!(
        quits, 1,
        "waiting cannot dispatch or discard gameplay input"
    );
    // A predicate failure must return unchanged, not park forever or claim ready.
    assert!(matches!(
        platform.wait_until_ready(|| Err::<bool, _>(PlatformError::NativeWait {
            operation: "fixture readiness failure",
            code: 1234,
        })),
        Err(PlatformError::NativeWait { code: 1234, .. })
    ));
    cpu.shutdown()?;
    Ok(())
}
