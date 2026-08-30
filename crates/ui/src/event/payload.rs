//! Owned values crossing from client state into Lua event callbacks.

use crate::UiEventError;

pub(crate) const MAX_EVENT_ARGUMENTS: usize = 9;

/// One Lua-visible stock event argument.
#[derive(Clone, Debug, PartialEq)]
pub enum UiEventArgument {
    /// An explicit Lua `nil` slot.
    Nil,
    /// A boolean value.
    Boolean(bool),
    /// An exact integral value.
    Integer(i64),
    /// A finite or non-finite Lua number supplied by the caller.
    Number(f64),
    /// An owned byte-preserving UTF-8 string.
    String(String),
}

/// A bounded event argument sequence in stock `arg1` through `arg9` order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiEventPayload {
    arguments: Vec<UiEventArgument>,
}

impl UiEventPayload {
    /// Creates an empty event payload.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            arguments: Vec::new(),
        }
    }

    /// Copies an argument sequence after enforcing the stock global limit.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError::PayloadTooLarge`] for more than nine values.
    pub fn new(arguments: impl IntoIterator<Item = UiEventArgument>) -> Result<Self, UiEventError> {
        let arguments = arguments.into_iter().collect::<Vec<_>>();
        if arguments.len() > MAX_EVENT_ARGUMENTS {
            return Err(UiEventError::PayloadTooLarge {
                count: arguments.len(),
                maximum: MAX_EVENT_ARGUMENTS,
            });
        }
        Ok(Self { arguments })
    }

    /// Returns arguments in Lua-visible order.
    #[must_use]
    pub fn arguments(&self) -> &[UiEventArgument] {
        &self.arguments
    }
}
