//! Ordered native event subscriptions, independent of the live region arena size.

mod dispatch;
mod methods;
mod subscriptions;

pub(super) use dispatch::dispatch_subscribers;
pub(super) use methods::register_frame_event_methods;
pub(super) use subscriptions::initialize;
