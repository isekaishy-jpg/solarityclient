//! Owned values crossing from client state into Lua event callbacks.

pub(crate) const LEGACY_EVENT_ARGUMENT_GLOBALS: usize = 9;

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

/// The complete positional argument sequence passed to a stock `OnEvent` handler.
///
/// Legacy `arg1` through `arg9` globals expose only the first nine values. The
/// callback receives every value: `0x004FDBC0`, for example, sends thirteen
/// chat arguments through the unbounded format walk at `0x0081AC90`.
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

    /// Owns every callback argument without truncating it to legacy globals.
    #[must_use]
    pub fn new(arguments: impl IntoIterator<Item = UiEventArgument>) -> Self {
        Self {
            arguments: arguments.into_iter().collect(),
        }
    }

    /// Returns arguments in Lua-visible order.
    #[must_use]
    pub fn arguments(&self) -> &[UiEventArgument] {
        &self.arguments
    }
}
