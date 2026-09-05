//! Ordered mutations targeting native Model instances rather than UI snapshots.

use std::collections::VecDeque;

use solarity_asset::AssetPath;

/// Identity of one mutable M2 instance assigned to a UI widget.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UiModelInstance {
    object_index: usize,
    generation: u32,
}

impl UiModelInstance {
    pub(crate) const fn new(object_index: usize, generation: u32) -> Self {
        Self {
            object_index,
            generation,
        }
    }

    /// Returns the widget's zero-based arena index.
    #[must_use]
    pub const fn object_index(self) -> usize {
        self.object_index
    }

    /// Returns the generation advanced by every model assignment or clear.
    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

/// One Lua operation retained in call order for the model owner.
///
/// Sequence calls must not be coalesced: equal requests restart their timer
/// and consume separate CRT rolls in `CM2Model::SetSequence` (`0x00832AB0`).
#[derive(Clone, Debug, PartialEq)]
pub enum UiModelAction {
    /// Replace the widget's mutable instance, or clear it when `path` is absent.
    Assign {
        /// New mutable owner, including a generation for explicit clears.
        instance: UiModelInstance,
        /// Canonical resource selected by stock lookup, or no model.
        path: Option<AssetPath>,
    },
    /// Select a new sequence timer, including repeated requests for the same ID.
    Sequence {
        /// Mutable owner that existed at the time of this call.
        instance: UiModelInstance,
        /// Full native argument word, including the `u32::MAX` bone-clear sentinel.
        animation_id: u32,
        /// Signed seek offset after the native Lua numeric conversion.
        time_offset_ms: i32,
    },
}

/// Shared bridge between Lua closures and the process's model owner.
#[derive(Default)]
pub(crate) struct UiModelBridge {
    /// FrameXML has no model compositor consumer yet; it must not accumulate
    /// an unconsumed per-frame journal. Glue explicitly installs this owner.
    actions: Option<VecDeque<UiModelAction>>,
}

impl UiModelBridge {
    /// Installs the process model owner before executing the Glue manifest.
    pub(crate) fn start_recording(&mut self) {
        self.actions = Some(VecDeque::new());
    }

    pub(crate) fn push(&mut self, action: UiModelAction) {
        if let Some(actions) = self.actions.as_mut() {
            actions.push_back(action);
        }
    }

    pub(crate) fn take_action(&mut self) -> Option<UiModelAction> {
        self.actions.as_mut()?.pop_front()
    }
}

/// 0x009607E0/0x009608B0 truncate through x87 FISTP i64, then take EAX.
/// Invalid conversion produces the integer-indefinite i64 value, whose low
/// word is zero. Rust's saturating float casts do not reproduce this ABI.
pub(super) fn model_animation_id(value: f64) -> u32 {
    if (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value) {
        value.trunc() as i64 as u32
    } else {
        0
    }
}

/// The SSE2 branch of `_ftol2` (0x0088B9C0) uses CVTTSD2SI for time offsets.
pub(super) fn model_time_offset(value: f64) -> i32 {
    let truncated = value.trunc();
    if (-2_147_483_648.0..2_147_483_648.0).contains(&truncated) {
        truncated as i32
    } else {
        i32::MIN
    }
}
