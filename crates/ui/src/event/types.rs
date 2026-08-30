//! Stable event-dispatch results and failures.

use thiserror::Error;

use crate::{UiLayoutError, UiScriptError};

/// Result of delivering one canonical UI event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiEventDispatch {
    subscriber_count: usize,
}

impl UiEventDispatch {
    pub(crate) const fn new(subscriber_count: usize) -> Self {
        Self { subscriber_count }
    }

    /// Returns the number of subscribed `OnEvent` handlers invoked.
    #[must_use]
    pub const fn subscriber_count(self) -> usize {
        self.subscriber_count
    }
}

/// A UI event could not be validated or delivered.
#[derive(Debug, Error)]
pub enum UiEventError {
    /// The name is not present in the selected stock event registry.
    #[error("unknown GlueXML event {name}")]
    Unknown {
        /// Caller-supplied event name.
        name: String,
    },
    /// The legacy client exposes no more than nine event argument globals.
    #[error("UI event payload has {count} arguments; stock supports at most {maximum}")]
    PayloadTooLarge {
        /// Supplied argument count.
        count: usize,
        /// Maximum stock argument count.
        maximum: usize,
    },
    /// A subscribed Lua handler failed.
    #[error(transparent)]
    Script(#[from] UiScriptError),
    /// A handler left live anchors in an invalid geometry state.
    #[error(transparent)]
    Layout(#[from] UiLayoutError),
}
