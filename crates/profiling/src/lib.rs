//! Process-wide, opt-in execution measurements with bounded per-thread storage.

mod capture;
mod host;
mod recorder;
mod scope;

#[cfg(test)]
#[path = "../tests/unit/aggregate.rs"]
mod tests;

pub use capture::Capture;
pub use scope::{Profile, Site, begin_frame, detail_enabled, enabled, generation};

/// Measures active polls of an async operation, excluding time parked on I/O.
/// Invoke inside an async function; the future is pinned on the caller's stack.
#[macro_export]
macro_rules! profile_await {
    ($label:literal, $future:expr) => {{
        let mut future = std::pin::pin!($future);
        std::future::poll_fn(|context| {
            let _profile = $crate::profile!($label);
            std::future::Future::poll(future.as_mut(), context)
        })
        .await
    }};
}

/// Starts a coarse scope; disabled calls perform no clock, allocation or locking.
#[macro_export]
macro_rules! profile {
    ($label:literal) => {{
        static SITE: $crate::Site = $crate::Site::new($label, false);
        $crate::Profile::new(&SITE)
    }};
}

/// Samples repeated inner work only on explicitly classified detail frames.
#[macro_export]
macro_rules! detail_profile {
    ($label:literal) => {{
        static SITE: $crate::Site = $crate::Site::new($label, true);
        $crate::Profile::new(&SITE)
    }};
}

/// Records a workload value without evaluating the value expression while off.
#[macro_export]
macro_rules! profile_value {
    ($label:literal, $value:expr) => {{
        if $crate::enabled() {
            static SITE: $crate::Site = $crate::Site::counter($label);
            SITE.value($value as u64);
        }
    }};
}
