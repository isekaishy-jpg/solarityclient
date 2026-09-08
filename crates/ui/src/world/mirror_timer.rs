//! Native mirror-timer anchors read synchronously by FrameXML.

use mlua::{Lua, Table, Value};

use crate::UiScriptEnvironment;

/// One of the three `BD0B80` records, projected with its resolved label.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiMirrorTimer {
    /// Native kind, or three for the inactive `UNKNOWN` sentinel.
    pub kind: u32,
    /// Starting value in milliseconds; the query does not clamp it.
    pub value: i32,
    /// Status-bar maximum in milliseconds.
    pub maximum: i32,
    /// Signed value change per elapsed millisecond.
    pub scale: i32,
    /// Exact start-packet pause byte.
    pub paused: u8,
    /// Localized spell name, or `None` to query the corresponding Lua global.
    pub label: Option<String>,
    /// Low 32 bits of the client clock at receipt of the start packet.
    pub timestamp_ms: u32,
}

impl Default for UiMirrorTimer {
    fn default() -> Self {
        Self {
            kind: 3,
            value: 0,
            maximum: 0,
            scale: 0,
            paused: 0,
            label: None,
            timestamp_ms: 0,
        }
    }
}

impl UiMirrorTimer {
    /// Returns `513E00`'s stable token for a native identifier.
    #[must_use]
    pub const fn token(kind: u32) -> &'static str {
        match kind {
            0 => "EXHAUSTION",
            1 => "BREATH",
            2 => "FEIGNDEATH",
            _ => "UNKNOWN",
        }
    }

    /// Returns `517AA0`'s signed, wrapping interpolation, including inactive slots.
    #[must_use]
    pub const fn progress(&self, timestamp_ms: u32) -> i32 {
        (timestamp_ms.wrapping_sub(self.timestamp_ms) as i32)
            .wrapping_mul(self.scale)
            .wrapping_add(self.value)
    }
}

pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    environment: &UiScriptEnvironment,
) -> mlua::Result<()> {
    let info = environment.world_state();
    globals.raw_set(
        "GetMirrorTimerInfo",
        lua.create_function(move |lua, argument: Value| {
            let number = lua.coerce_number(argument)?.ok_or_else(usage_error)?;
            if !number.is_finite() || !(1.0..4.0).contains(&number) {
                return Err(usage_error());
            }
            let timer = info
                .mirror_timer(number as usize - 1)
                .ok_or_else(usage_error)?;
            let token = UiMirrorTimer::token(timer.kind);
            let label = match timer.label {
                Some(label) => label,
                None => lua
                    .globals()
                    .raw_get::<Option<String>>(format!("{token}_LABEL"))?
                    .unwrap_or_default(),
            };
            Ok((
                token,
                timer.value,
                timer.maximum,
                timer.scale,
                timer.paused,
                label,
            ))
        })?,
    )?;
    let progress = environment.world_state();
    let clock = environment.client_clock();
    globals.raw_set(
        "GetMirrorTimerProgress",
        lua.create_function(move |lua, argument: Value| {
            let token = lua.coerce_string(argument)?.ok_or_else(usage_error)?;
            let bytes = token.as_bytes();
            let token = bytes.split(|byte| *byte == 0).next().unwrap_or_default();
            let index = ["EXHAUSTION", "BREATH", "FEIGNDEATH"]
                .iter()
                .position(|candidate| token.eq_ignore_ascii_case(candidate.as_bytes()))
                .ok_or_else(usage_error)?;
            progress
                .mirror_timer_progress(index, clock.milliseconds())
                .ok_or_else(usage_error)
        })?,
    )?;
    Ok(())
}

fn usage_error() -> mlua::Error {
    // Both original functions use the same message, including the Info name.
    mlua::Error::runtime("Usage: GetMirrorTimerInfo(\"timer\")")
}
