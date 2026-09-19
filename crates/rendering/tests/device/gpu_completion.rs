//! Controlled host backends exercise real completion ownership without a GPU.

use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use ash::vk::{self, Handle};
use solarity_cpu::CoordinatorNotifier;

use super::GpuCompletionService;
use crate::device::VulkanError;

/// Lossless fixture notifications; bounded timeout is only a deadlock guard.
struct Signal(Sender<()>);

impl CoordinatorNotifier for Signal {
    fn notify(&self) {
        let _sent = self.0.send(());
    }
}

/// Receives a controlled fixture event, failing instead of hanging the suite.
fn receive<T>(receiver: &Receiver<T>) -> T {
    receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|error| panic!("completion fixture did not progress: {error}"))
}

/// A pending backend must not publish readiness; each completion precedes its
/// notification. Reuse must never observe the preceding request's ready bit.
#[test]
fn controlled_pending_fences_publish_once_and_reuse_one_worker() -> Result<(), Box<dyn Error>> {
    let (started_tx, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let (notifier, notified) = mpsc::channel();
    let mut service = GpuCompletionService::new(
        move |fence| {
            let _sent = started_tx.send((fence, std::thread::current().id()));
            receive(&released);
            Ok(())
        },
        Arc::new(Signal(notifier)),
    )?;
    let mut worker = None;
    for generation in 1..=256 {
        let fence = vk::Fence::from_raw(generation);
        service.wait_for::<VulkanError>(fence, |completion| {
            let (observed, thread) = receive(&started);
            assert_eq!(observed, fence);
            assert_ne!(thread, std::thread::current().id());
            assert_eq!(*worker.get_or_insert(thread), thread);
            assert!(!completion.is_ready());
            release.send(()).unwrap_or_else(|error| panic!("{error}"));
            receive(&notified);
            assert!(completion.is_ready());
            Ok(())
        })?;
        assert!(notified.try_recv().is_err());
    }
    Ok(())
}

/// Driver errors release the host observer and preserve their actual diagnosis.
#[test]
fn driver_failure_is_terminal_and_does_not_poison_the_next_request() -> Result<(), Box<dyn Error>> {
    let (notifier, notified) = mpsc::channel();
    let mut service = GpuCompletionService::new(
        |fence| {
            if fence.as_raw() == 1 {
                Err(vk::Result::ERROR_DEVICE_LOST)
            } else {
                Ok(())
            }
        },
        Arc::new(Signal(notifier)),
    )?;
    for generation in 1..=2 {
        let result =
            service.wait_for::<VulkanError>(vk::Fence::from_raw(generation), |completion| {
                receive(&notified);
                assert!(completion.is_ready());
                Ok(())
            });
        if generation == 1 {
            assert!(
                matches!(result, Err(VulkanError::Operation { operation: "wait for GPU frame slot", message })
                if message == vk::Result::ERROR_DEVICE_LOST.to_string())
            );
        } else {
            result?;
        }
    }
    Ok(())
}

/// Native failure and unwind both drain backend ownership before the caller can
/// reuse or destroy its fence. Neither relies on the callback waiting correctly.
#[test]
fn native_error_and_unwind_drain_the_observer() -> Result<(), Box<dyn Error>> {
    let (release, released) = mpsc::channel();
    let (notifier, _notified) = mpsc::channel();
    let completed = Arc::new(AtomicUsize::new(0));
    let backend_completed = Arc::clone(&completed);
    let mut service = GpuCompletionService::new(
        move |_| {
            receive(&released);
            backend_completed.fetch_add(1, Ordering::Release);
            Ok(())
        },
        Arc::new(Signal(notifier)),
    )?;
    let result = service.wait_for::<VulkanError>(vk::Fence::from_raw(1), |_| {
        release.send(()).unwrap_or_else(|error| panic!("{error}"));
        Err(VulkanError::operation(
            "fixture native wait",
            "controlled failure",
        ))
    });
    assert!(result.is_err_and(|error| error.to_string().contains("controlled failure")));
    assert_eq!(completed.load(Ordering::Acquire), 1);
    let unwind = catch_unwind(AssertUnwindSafe(|| {
        service.wait_for::<VulkanError>(vk::Fence::from_raw(2), |_| {
            release.send(()).unwrap_or_else(|error| panic!("{error}"));
            panic!("controlled native unwind")
        })
    }));
    assert!(unwind.is_err());
    assert_eq!(completed.load(Ordering::Acquire), 2);
    Ok(())
}

/// Backend panic must publish failure instead of leaving main parked forever.
#[test]
fn backend_panic_publishes_failure_and_shutdown_joins_the_thread() -> Result<(), Box<dyn Error>> {
    /// Backend capture retirement proves that service drop actually joins.
    struct Retired(Arc<AtomicBool>);
    impl Drop for Retired {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let retired = Arc::new(AtomicBool::new(false));
    let owner = Retired(Arc::clone(&retired));
    let (notifier, notified) = mpsc::channel();
    let mut service = GpuCompletionService::new(
        move |fence| {
            let _owner = &owner;
            assert_ne!(fence.as_raw(), 1, "controlled backend panic");
            Ok(())
        },
        Arc::new(Signal(notifier)),
    )?;
    let result = service.wait_for::<VulkanError>(vk::Fence::from_raw(1), |completion| {
        receive(&notified);
        assert!(completion.is_ready());
        Ok(())
    });
    assert!(result.is_err_and(|error| error.to_string().contains("backend panicked")));
    service.wait_for::<VulkanError>(vk::Fence::from_raw(2), |completion| {
        receive(&notified);
        assert!(completion.is_ready());
        Ok(())
    })?;
    assert!(!retired.load(Ordering::Acquire));
    drop(service);
    assert!(retired.load(Ordering::Acquire));
    Ok(())
}

/// A broken external notifier must not kill the worker or silently make future
/// work unwakeable. Its first failure is latched before the second backend runs.
#[test]
fn notifier_panic_is_latched_before_the_next_completion() -> Result<(), Box<dyn Error>> {
    struct BrokenNotifier;
    impl CoordinatorNotifier for BrokenNotifier {
        fn notify(&self) {
            panic!("controlled notifier contract violation");
        }
    }
    let mut service = GpuCompletionService::new(|_| Ok(()), Arc::new(BrokenNotifier))?;
    // Native service may observe durable completion before notification runs.
    let _first = service.wait_for::<VulkanError>(vk::Fence::from_raw(1), |_| Ok(()));
    let second = service.wait_for::<VulkanError>(vk::Fence::from_raw(2), |_| Ok(()));
    assert!(second.is_err_and(|error| error.to_string().contains("notifier panicked")));
    Ok(())
}
