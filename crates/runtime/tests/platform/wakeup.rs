//! Controlled wake races and real Windows/SDL lifetime checks.

use super::sequence::{SignalAction, WakeSequence};

#[test]
fn notification_before_or_during_arm_invalidates_the_old_ticket() {
    let sequence = WakeSequence::default();
    let before_drain = sequence.observe();
    assert!(matches!(sequence.notify(), SignalAction::Coalesced));
    assert!(!sequence.arm(before_drain));
    let during_reset = sequence.observe();
    assert!(matches!(sequence.notify(), SignalAction::Coalesced));
    assert!(!sequence.arm(during_reset));
    let current = sequence.observe();
    assert!(sequence.arm(current));
    assert!(matches!(sequence.notify(), SignalAction::Wake));
    for _ in 0..64 {
        assert!(matches!(sequence.notify(), SignalAction::Coalesced));
    }
    sequence.disarm();
    assert!(sequence.arm(sequence.observe()));
}

#[test]
fn concurrent_notifiers_signal_once_per_arm_without_losing_generations() {
    let sequence = WakeSequence::default();
    let ticket = sequence.observe();
    assert!(sequence.arm(ticket));
    let signals = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                for _ in 0..128 {
                    if matches!(sequence.notify(), SignalAction::Wake) {
                        signals.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            });
        }
    });
    assert_eq!(signals.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert!(!sequence.arm(ticket));
    assert!(sequence.arm(sequence.observe()));
    sequence.disarm();
}

#[cfg(windows)]
mod native {
    use std::error::Error;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use super::super::{WakeReason, windows::WakeBridge};
    use crate::test_support::SDL_TEST_LOCK;

    #[test]
    fn native_deadline_and_completion_preserve_owned_signal_lifetime() -> Result<(), Box<dyn Error>>
    {
        let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock poisoned")?;
        let sdl = sdl3::init()?;
        let _events = sdl.event()?;
        let mut bridge = WakeBridge::new()?;
        let start = Instant::now();
        let mut reason = bridge.wait(bridge.observe(), start + Duration::from_micros(800))?;
        while reason != WakeReason::Deadline {
            reason = bridge.wait(bridge.observe(), start + Duration::from_micros(800))?;
        }
        assert!(Instant::now() >= start + Duration::from_micros(800));
        let notifier = bridge.notifier();
        let ticket = bridge.observe();
        notifier.notify();
        assert_eq!(
            bridge.wait(ticket, Instant::now() + Duration::from_secs(1))?,
            WakeReason::Completion
        );
        drop(bridge);
        // The final producer owns the event even after main's timer/watch leaves.
        notifier.notify();
        drop(notifier);
        Ok(())
    }

    #[test]
    fn background_sdl_event_survives_watch_wakeup_and_is_consumed_once()
    -> Result<(), Box<dyn Error>> {
        let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock poisoned")?;
        let sdl = sdl3::init()?;
        let events = sdl.event()?;
        let mut pump = sdl.event_pump()?;
        while pump.poll_event().is_some() {}
        let mut bridge = WakeBridge::new()?;
        let sender = events.event_sender();
        let published = Arc::new(AtomicBool::new(false));
        let published_by_worker = Arc::clone(&published);
        let worker = std::thread::spawn(move || {
            let result = sender.push_event(sdl3::event::Event::Quit { timestamp: 0 });
            published_by_worker.store(true, Ordering::Release);
            result
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut quits = 0;
        while Instant::now() < deadline {
            let ticket = bridge.observe();
            while let Some(event) = pump.poll_event() {
                if matches!(event, sdl3::event::Event::Quit { .. }) {
                    quits += 1;
                }
            }
            if quits != 0 && published.load(Ordering::Acquire) {
                break;
            }
            bridge.wait(ticket, deadline)?;
        }
        worker.join().map_err(|_| "SDL producer panicked")??;
        while let Some(event) = pump.poll_event() {
            if matches!(event, sdl3::event::Event::Quit { .. }) {
                quits += 1;
            }
        }
        assert_eq!(quits, 1);
        Ok(())
    }

    #[test]
    fn notification_after_arming_wakes_for_published_state() -> Result<(), Box<dyn Error>> {
        let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock poisoned")?;
        let sdl = sdl3::init()?;
        let _events = sdl.event()?;
        let mut bridge = WakeBridge::new()?;
        let notifier = bridge.notifier();
        let published = Arc::new(AtomicBool::new(false));
        let worker_state = Arc::clone(&published);
        let worker = std::thread::spawn(move || {
            // A controlled late producer spans a native maintenance wait.
            std::thread::sleep(Duration::from_millis(20));
            worker_state.store(true, Ordering::Release);
            notifier.notify();
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let ticket = bridge.observe();
            if published.load(Ordering::Acquire) {
                break;
            }
            if bridge.wait(ticket, deadline)? == WakeReason::Deadline {
                break;
            }
        }
        let observed_before_join = published.load(Ordering::Acquire);
        worker.join().map_err(|_| "notifier producer panicked")?;
        assert!(observed_before_join);
        Ok(())
    }
}
