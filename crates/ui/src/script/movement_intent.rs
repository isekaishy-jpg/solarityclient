//! Timestamped native InputControl requests emitted by trusted FrameXML.

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

/// One named control retained by the native InputControl owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiMovementControl {
    /// Forward translation.
    Forward,
    /// Backward translation.
    Backward,
    /// Leftward translation.
    StrafeLeft,
    /// Rightward translation.
    StrafeRight,
    /// Positive yaw, or left strafe during mouselook.
    TurnLeft,
    /// Negative yaw, or right strafe during mouselook.
    TurnRight,
    /// Positive pitch or vehicle aim.
    PitchUp,
    /// Negative pitch or vehicle aim.
    PitchDown,
    /// Ascending input, also held by JumpOrAscendStart.
    Ascend,
    /// Descending input, also held by SitStandOrDescendStart.
    Descend,
}

/// InputControl action; unit mode and eligibility are resolved by its owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiMovementAction {
    /// Starts or stops a named held control.
    Hold {
        /// Logical control selected by the Lua native.
        control: UiMovementControl,
        /// Whether this request starts the control.
        pressed: bool,
    },
    /// Holds ascent and conditionally requests the controlled unit's jump.
    JumpOrAscend,
    /// Holds descent and conditionally changes the controlled unit's stand state.
    SitStandOrDescend,
    /// Changes the native autorun held bit.
    ToggleAutoRun,
    /// Changes the controlled unit's walking/running mode.
    ToggleRun,
}

/// One ordered command with the source input time captured at the Lua call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiMovementCommand {
    /// Native command selected by trusted FrameXML.
    pub action: UiMovementAction,
    /// Original wrapping input-event time, not the time of a later frame.
    pub timestamp_ms: u32,
}

#[derive(Default)]
struct MovementInputState {
    event_time_ms: u32,
    pending: VecDeque<UiMovementCommand>,
}

/// Shared main-thread input clock and ordered native-command mailbox.
#[derive(Clone, Default)]
pub(crate) struct UiMovementInput(Rc<RefCell<MovementInputState>>);

impl UiMovementInput {
    pub(crate) fn set_event_time(&self, timestamp_ms: u32) {
        self.0.borrow_mut().event_time_ms = timestamp_ms;
    }

    pub(crate) fn take(&self) -> Option<UiMovementCommand> {
        self.0.borrow_mut().pending.pop_front()
    }

    fn request(&self, action: UiMovementAction) {
        let mut state = self.0.borrow_mut();
        let timestamp_ms = state.event_time_ms;
        state.pending.push_back(UiMovementCommand {
            action,
            timestamp_ms,
        });
    }
}

/// Native wrappers ignore Lua arguments and read the retained input-event clock.
/// Only trusted built-in FrameXML currently executes in this environment. The
/// native class-zero taint gate must accompany future untrusted AddOn execution;
/// a hardware event alone does not authorize tainted movement calls.
pub(crate) fn register_globals(
    lua: &mlua::Lua,
    globals: &mlua::Table,
    input: UiMovementInput,
) -> mlua::Result<()> {
    use UiMovementControl as Control;
    for (name, control, pressed) in [
        ("MoveForwardStart", Control::Forward, true),
        ("MoveForwardStop", Control::Forward, false),
        ("MoveBackwardStart", Control::Backward, true),
        ("MoveBackwardStop", Control::Backward, false),
        ("StrafeLeftStart", Control::StrafeLeft, true),
        ("StrafeLeftStop", Control::StrafeLeft, false),
        ("StrafeRightStart", Control::StrafeRight, true),
        ("StrafeRightStop", Control::StrafeRight, false),
        ("TurnLeftStart", Control::TurnLeft, true),
        ("TurnLeftStop", Control::TurnLeft, false),
        ("TurnRightStart", Control::TurnRight, true),
        ("TurnRightStop", Control::TurnRight, false),
        ("PitchUpStart", Control::PitchUp, true),
        ("PitchUpStop", Control::PitchUp, false),
        ("PitchDownStart", Control::PitchDown, true),
        ("PitchDownStop", Control::PitchDown, false),
        ("VehicleAimUpStart", Control::PitchUp, true),
        ("VehicleAimUpStop", Control::PitchUp, false),
        ("VehicleAimDownStart", Control::PitchDown, true),
        ("VehicleAimDownStop", Control::PitchDown, false),
        ("AscendStop", Control::Ascend, false),
        ("DescendStop", Control::Descend, false),
    ] {
        let input = input.clone();
        globals.raw_set(
            name,
            lua.create_function(move |_, ()| {
                input.request(UiMovementAction::Hold { control, pressed });
                Ok(())
            })?,
        )?;
    }
    for (name, action) in [
        ("JumpOrAscendStart", UiMovementAction::JumpOrAscend),
        (
            "SitStandOrDescendStart",
            UiMovementAction::SitStandOrDescend,
        ),
        ("ToggleAutoRun", UiMovementAction::ToggleAutoRun),
        ("ToggleRun", UiMovementAction::ToggleRun),
    ] {
        let input = input.clone();
        globals.raw_set(
            name,
            lua.create_function(move |_, ()| {
                input.request(action);
                Ok(())
            })?,
        )?;
    }
    Ok(())
}
