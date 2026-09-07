//! Native transport stop requests, departure freezes, and replicated phase.

use super::route::TransportRoute;

/// Requested transport motion from GAMEOBJECT_STATE, after template admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportRouteMotion {
    /// Continue the route, releasing an existing stop.
    Moving,
    /// Stop at the next station's departure clock.
    StopAtStation,
}

/// Per-object `007F7840` clock state, independent of shared route geometry.
#[derive(Clone, Copy, Debug, Default)]
pub struct TransportRouteClock {
    offset_ms: u32,
    stop_ms: u32,
    stop_requested: bool,
    stopped: bool,
}

impl TransportRouteClock {
    /// Applies the normalized high half of GAMEOBJECT_DYNAMIC as `007F8120`.
    /// Both float stores and the wrapping client-clock offset are retained.
    pub fn synchronize_progress(&mut self, route: &TransportRoute, time_ms: u32, progress: u16) {
        let period = route.period_ms();
        if period == 0 {
            return;
        }
        let fraction = (f64::from(progress) * f64::from(f32::from_bits(0x37800080))) as f32;
        let progress_ms =
            ((f64::from(period) * f64::from(fraction)) as f32).round_ties_even() as u32;
        self.set_offset(period, time_ms, progress_ms);
    }

    /// `007F8000` preserves a requested stop until its departure, and resumes
    /// a frozen transport from that departure instead of jumping to wall time.
    pub fn set_motion(
        &mut self,
        route: &TransportRoute,
        time_ms: u32,
        motion: TransportRouteMotion,
    ) {
        let period = route.period_ms();
        let requested = motion == TransportRouteMotion::StopAtStation;
        if period == 0 || self.stop_requested == requested {
            return;
        }
        let query = if self.stopped {
            self.stop_ms.wrapping_sub(1).wrapping_add(period)
        } else {
            time_ms.wrapping_add(self.offset_ms)
        } % period;
        let Some(departure) = route.next_departure_ms(query) else {
            return;
        };
        if requested {
            self.stop_ms = departure % period;
        } else if self.stopped {
            self.set_offset(period, time_ms, departure);
        }
        self.stop_requested = requested;
        self.stopped = false;
    }

    /// `007F80A0` immediately freezes at the station selected from clock minus
    /// one, as required by GAMEOBJECT_DYNAMIC flag 0x10 during initialization.
    pub fn freeze_at_station(&mut self, route: &TransportRoute, time_ms: u32) {
        let period = route.period_ms();
        if period == 0 || self.stopped {
            return;
        }
        let query = self
            .offset_ms
            .wrapping_add(period)
            .wrapping_sub(1)
            .wrapping_add(time_ms)
            % period;
        let Some(departure) = route.next_departure_ms(query) else {
            return;
        };
        self.stop_requested = true;
        self.stopped = true;
        self.stop_ms = departure % period;
    }

    /// `007F7840` consumes a frame interval to detect crossing the requested
    /// departure, including a crossing through the route's wrap boundary.
    pub fn clock_ms(&mut self, route: &TransportRoute, time_ms: u32, elapsed_ms: u32) -> u32 {
        let period = route.period_ms();
        if period == 0 {
            return 0;
        }
        let clock = time_ms.wrapping_add(self.offset_ms) % period;
        if !self.stop_requested {
            return clock;
        }
        if self.stopped {
            return self.stop_ms;
        }
        let end = self.stop_ms.wrapping_add(elapsed_ms) % period;
        if includes(self.stop_ms, end, clock) {
            self.stopped = true;
            return self.stop_ms;
        }
        clock
    }

    /// `007F78C0` chooses the nonnegative offset in the native period domain.
    fn set_offset(&mut self, period: u32, time_ms: u32, progress_ms: u32) {
        let clock = time_ms % period;
        self.offset_ms = if clock < progress_ms {
            progress_ms - clock
        } else {
            period.wrapping_sub(clock).wrapping_add(progress_ms)
        };
    }
}

/// `007F77D0` uses (start, end], except a zero-width interval includes start.
fn includes(start: u32, end: u32, value: u32) -> bool {
    if start == end {
        return value == end;
    }
    if start < end {
        start < value && value <= end
    } else {
        value <= end || start < value
    }
}
