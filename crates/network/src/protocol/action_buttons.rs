//! Authoritative 144-slot action-bar image sent by the world server.

use thiserror::Error;

/// Number of packed action buttons in build 12340.
pub const WORLD_ACTION_BUTTON_COUNT: usize = 144;

/// Server intent carried by the leading `SMSG_ACTION_BUTTONS` byte.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldActionButtonUpdate {
    /// Initial action-button image.
    Initial,
    /// Complete replacement image, including specialization changes.
    Replace,
    /// Toggle the player's state-two flag without replacing retained slots.
    StateTwo,
}

/// One complete decoded `SMSG_ACTION_BUTTONS` message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldActionButtons {
    update: WorldActionButtonUpdate,
    slots: Option<Box<[u32; WORLD_ACTION_BUTTON_COUNT]>>,
}

impl WorldActionButtons {
    /// Decodes the exact decrypted packet body.
    ///
    /// # Errors
    ///
    /// Returns [`WorldActionButtonPacketError`] for an unknown state or a body
    /// that does not match that state's stock framing.
    pub fn decode(payload: &[u8]) -> Result<Self, WorldActionButtonPacketError> {
        let Some((&state, body)) = payload.split_first() else {
            return Err(WorldActionButtonPacketError::new(
                0,
                "action-button packet is missing its state byte",
            ));
        };
        let update = match state {
            0 => WorldActionButtonUpdate::Initial,
            1 => WorldActionButtonUpdate::Replace,
            2 => WorldActionButtonUpdate::StateTwo,
            _ => {
                return Err(WorldActionButtonPacketError::new(
                    0,
                    "action-button packet contains an unknown state",
                ));
            }
        };
        if update == WorldActionButtonUpdate::StateTwo {
            if !body.is_empty() {
                return Err(WorldActionButtonPacketError::new(
                    1,
                    "action-button state-two packet contains an unexpected slot body",
                ));
            }
            return Ok(Self {
                update,
                slots: None,
            });
        }
        let expected = WORLD_ACTION_BUTTON_COUNT * size_of::<u32>();
        if body.len() != expected {
            return Err(WorldActionButtonPacketError::new(
                1 + body.len(),
                "action-button image must contain exactly 144 packed slots",
            ));
        }
        let mut slots = Box::new([0; WORLD_ACTION_BUTTON_COUNT]);
        let (words, remainder) = body.as_chunks::<4>();
        debug_assert!(remainder.is_empty());
        for (slot, bytes) in slots.iter_mut().zip(words) {
            *slot = u32::from_le_bytes(*bytes);
        }
        Ok(Self {
            update,
            slots: Some(slots),
        })
    }

    /// Returns whether the packet initializes, replaces, or clears the image.
    #[must_use]
    pub const fn update(&self) -> WorldActionButtonUpdate {
        self.update
    }

    /// Returns all zero-based packed slots in server order.
    #[must_use]
    pub fn slots(&self) -> Option<&[u32; WORLD_ACTION_BUTTON_COUNT]> {
        self.slots.as_deref()
    }

    /// Returns one packed zero-based slot when it is in the stock range.
    #[must_use]
    pub fn slot(&self, index: usize) -> Option<u32> {
        self.slots.as_ref()?.get(index).copied()
    }
}

/// A malformed `SMSG_ACTION_BUTTONS` body.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("malformed action-button packet at byte {offset}: {message}")]
pub struct WorldActionButtonPacketError {
    offset: usize,
    message: &'static str,
}

impl WorldActionButtonPacketError {
    const fn new(offset: usize, message: &'static str) -> Self {
        Self { offset, message }
    }

    /// Returns the invalid field offset or rejected packet extent.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// Returns a stable description of the rejected representation.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }
}
