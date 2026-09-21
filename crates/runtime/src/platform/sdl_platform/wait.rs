//! Native readiness and clock waits collect input without dispatching gameplay.

use super::{NativeInput, NativeInputHandle, SdlPlatform};
use crate::platform::PlatformError;

impl SdlPlatform {
    /// Surfaces producer-side native faults on every serviced frame.
    pub(crate) fn check_wait_health(&self) -> Result<(), PlatformError> {
        self.input.borrow().wake.check()
    }

    /// Shares native notification ownership without exposing SDL to workers.
    pub(crate) fn coordinator_notifier(
        &self,
    ) -> std::sync::Arc<dyn solarity_cpu::CoordinatorNotifier> {
        self.input.borrow().wake.notifier()
    }

    /// Gives main-only services weak access to the same native input owner.
    pub(crate) fn input_handle(&self) -> NativeInputHandle {
        NativeInputHandle(std::rc::Rc::downgrade(&self.input))
    }

    /// Parks idle/movie service until work, input or its next clock boundary.
    pub(crate) fn wait_for_work(
        &mut self,
        deadline: std::time::Instant,
    ) -> Result<crate::platform::wakeup::WakeReason, PlatformError> {
        let mut input = self.input.borrow_mut();
        let ticket = input.wake.observe();
        input.event_pump.pump_events();
        // SAFETY: SDL is live on its owning thread. This checks the entire queue
        // without removing events or invoking gameplay outside its cutoff.
        if unsafe { sdl3::sys::events::SDL_HasEvents(0, u32::MAX) } {
            return Ok(crate::platform::wakeup::WakeReason::Input);
        }
        input.wake.wait(ticket, deadline)
    }

    /// Keeps the presentation deadline while collecting native input into SDL's
    /// queue. Gameplay translation remains at the next ordered frame boundary.
    pub(crate) fn wait_for_frame_deadline(
        &mut self,
        deadline: std::time::Instant,
    ) -> Result<(), PlatformError> {
        let mut input = self.input.borrow_mut();
        while std::time::Instant::now() < deadline {
            let ticket = input.wake.observe();
            input.event_pump.pump_events();
            input.wake.wait(ticket, deadline)?;
        }
        Ok(())
    }

    /// Services native input until a durable predicate is ready. Events remain
    /// queued for the next gameplay cutoff; an existing SDL event is not work
    /// completion and must not turn this into a poll loop. The ticket precedes
    /// the last predicate check so publication racing with native arm cannot
    /// lose its wake. No predicate or SDL callback runs under a wakeup lock.
    pub(crate) fn wait_until_ready<E: From<PlatformError>>(
        &mut self,
        ready: impl FnMut() -> Result<bool, E>,
    ) -> Result<(), E> {
        self.input.borrow_mut().wait_until_ready(ready)
    }
}

impl NativeInputHandle {
    pub(crate) fn wait_until_ready<E: From<PlatformError>>(
        &self,
        ready: impl FnMut() -> Result<bool, E>,
    ) -> Result<(), E> {
        let owner = self.0.upgrade().ok_or(PlatformError::InputUnavailable)?;
        let mut input = owner
            .try_borrow_mut()
            .map_err(|_| PlatformError::InputUnavailable)?;
        input.wait_until_ready(ready)
    }
}

impl NativeInput {
    fn wait_until_ready<E: From<PlatformError>>(
        &mut self,
        mut ready: impl FnMut() -> Result<bool, E>,
    ) -> Result<(), E> {
        self.wake.check()?;
        loop {
            let ticket = self.wake.observe();
            if ready()? {
                return Ok(());
            }
            self.event_pump.pump_events();
            if ready()? {
                return Ok(());
            }
            self.wake.wait(
                ticket,
                std::time::Instant::now() + crate::platform::wakeup::MAINTENANCE_INTERVAL,
            )?;
        }
    }
}
