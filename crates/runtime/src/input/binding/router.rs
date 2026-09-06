//! Main-thread resolution of physical transitions into stock assignments.

use solarity_ui::{
    UiBindingAction, UiBindingAssignment, UiBindingAssignmentId, UiBindingAssignments,
    UiBindingCatalog, UiBindingDefinition, UiBindingPlatform,
};

use crate::input::binding::stock_key::StockChord;
use crate::input::{InputBindingInvocation, InputBindingPhase};
use crate::platform::{
    ButtonState, KeyModifiers, MouseButton, MouseWheelDirection, PlatformEvent, ScanCode,
    WindowEvent, WindowId,
};

/// Physical identity retained until a `runOnUp` binding is released.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PhysicalControl {
    Key(ScanCode),
    Mouse(MouseButton),
}

/// One pressed assignment whose declaration requires a later up transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveRelease {
    control: PhysicalControl,
    assignment: UiBindingAssignmentId,
}

/// Resolves SDL-independent platform events through one effective key map.
///
/// Chords are assembled on the stack and used directly for borrowed hash-map
/// lookup. The router retains only compact assignment identities for
/// `runOnUp` commands, so releases do not clone Lua bodies or dynamic actions.
pub struct InputBindingRouter {
    primary_window: WindowId,
    active_releases: Vec<ActiveRelease>,
}

impl InputBindingRouter {
    /// Routes a physical transition into the retained FrameXML command state.
    /// UI capture suppresses new presses, while releases and focus loss always
    /// drain commands already admitted by this router. Every release is attempted
    /// even if an earlier command fails.
    ///
    /// # Errors
    /// Returns the first authored command or presentation error after dispatching
    /// the complete batch. Dynamic secure actions require their separate owner.
    pub fn route_to_frame(
        &mut self,
        event: &PlatformEvent,
        modifiers: KeyModifiers,
        captured: bool,
        frame: &mut solarity_ui::FrameManager,
    ) -> Result<usize, solarity_ui::UiEventError> {
        let new_press = match event {
            PlatformEvent::Key(event) => event.state == ButtonState::Pressed,
            PlatformEvent::MouseButton(event) => event.state == ButtonState::Pressed,
            PlatformEvent::MouseWheel(_) => true,
            _ => false,
        };
        if captured && new_press {
            return Ok(0);
        }
        let mut commands = Vec::new();
        let count = frame.with_bindings(|assignments, catalog| {
            self.route(event, modifiers, assignments, catalog, |invocation| {
                commands.push((invocation.assignment().action().clone(), invocation.phase()));
            })
        });
        let mut first_error = None;
        let button = match event {
            PlatformEvent::MouseButton(event) => match event.button {
                MouseButton::Left => Some(solarity_ui::UiPointerButton::Left),
                MouseButton::Right => Some(solarity_ui::UiPointerButton::Right),
                MouseButton::Middle => Some(solarity_ui::UiPointerButton::Middle),
                MouseButton::AuxiliaryOne => Some(solarity_ui::UiPointerButton::Button4),
                MouseButton::AuxiliaryTwo => Some(solarity_ui::UiPointerButton::Button5),
                MouseButton::Unknown => None,
            },
            _ => None,
        };
        for (action, phase) in commands {
            let result = match action {
                UiBindingAction::Command(name) => frame
                    .invoke_binding_with_mouse_button(
                        &name,
                        phase == InputBindingPhase::Down,
                        button,
                    )
                    .map(|_| ()),
                action => Err(solarity_ui::UiScriptError::Execution {
                    label: "dynamic binding action".to_owned(),
                    message: format!("secure action execution is unavailable: {action:?}"),
                }
                .into()),
            };
            if let Err(error) = result {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(count), Err)
    }

    /// Creates an empty router for the process's primary client window.
    #[must_use]
    pub fn new(primary_window: WindowId) -> Self {
        Self {
            primary_window,
            active_releases: Vec::new(),
        }
    }

    /// Resolves one event and emits zero or more binding invocations in order.
    ///
    /// `current_modifiers` must be the retained mask after the same event was
    /// admitted by [`crate::InputControl`]. Keyboard events use their atomic
    /// event mask directly; the retained mask is needed only for mouse chords.
    /// Repeated key-down events are not physical transitions. Focus loss emits
    /// `up` for all active `runOnUp` commands to prevent stuck movement.
    ///
    /// Returns the number of invocations supplied to `dispatch`.
    pub fn route<'a>(
        &mut self,
        event: &PlatformEvent,
        current_modifiers: KeyModifiers,
        assignments: &'a UiBindingAssignments,
        catalog: &'a UiBindingCatalog,
        mut dispatch: impl FnMut(InputBindingInvocation<'a>),
    ) -> usize {
        match event {
            PlatformEvent::ApplicationDidEnterBackground => {
                self.release_all(assignments, catalog, &mut dispatch)
            }
            PlatformEvent::Window {
                window_id,
                event: WindowEvent::FocusLost,
            } if *window_id == self.primary_window => {
                self.release_all(assignments, catalog, &mut dispatch)
            }
            PlatformEvent::Key(event) if event.window_id == self.primary_window => {
                let Some(scan_code) = event.scan_code else {
                    return 0;
                };
                let control = PhysicalControl::Key(scan_code);
                match event.state {
                    ButtonState::Pressed if event.is_repeat => 0,
                    ButtonState::Pressed => {
                        let Some(chord) = StockChord::keyboard(event.modifiers, scan_code) else {
                            return 0;
                        };
                        self.press(control, chord, assignments, catalog, &mut dispatch)
                    }
                    ButtonState::Released => {
                        self.release(control, assignments, catalog, &mut dispatch)
                    }
                }
            }
            PlatformEvent::MouseButton(event) if event.window_id == self.primary_window => {
                let control = PhysicalControl::Mouse(event.button);
                match event.state {
                    ButtonState::Pressed => {
                        let Some(chord) = StockChord::mouse(current_modifiers, event.button) else {
                            return 0;
                        };
                        self.press(control, chord, assignments, catalog, &mut dispatch)
                    }
                    ButtonState::Released => {
                        self.release(control, assignments, catalog, &mut dispatch)
                    }
                }
            }
            PlatformEvent::MouseWheel(event) if event.window_id == self.primary_window => {
                let direction = match event.direction {
                    MouseWheelDirection::Normal => event.y,
                    MouseWheelDirection::Flipped => -event.y,
                    MouseWheelDirection::Unknown => return 0,
                };
                let upward = if direction > 0.0 {
                    true
                } else if direction < 0.0 {
                    false
                } else {
                    return 0;
                };
                let Some(chord) = StockChord::wheel(current_modifiers, upward) else {
                    return 0;
                };
                Self::dispatch_down(chord, assignments, catalog, &mut dispatch)
            }
            _ => 0,
        }
    }

    /// Returns the number of physical controls awaiting a release callback.
    #[must_use]
    pub fn active_release_count(&self) -> usize {
        self.active_releases.len()
    }

    fn press<'a>(
        &mut self,
        control: PhysicalControl,
        chord: StockChord,
        assignments: &'a UiBindingAssignments,
        catalog: &'a UiBindingCatalog,
        dispatch: &mut impl FnMut(InputBindingInvocation<'a>),
    ) -> usize {
        let Some(key) = chord.as_str() else {
            return 0;
        };
        let Some((identifier, assignment)) = assignments.binding_with_id(key) else {
            return 0;
        };
        let Some(definition) = admitted_definition(assignment, catalog) else {
            if matches!(assignment.action(), UiBindingAction::Command(_)) {
                return 0;
            }
            dispatch(InputBindingInvocation::new(
                assignment,
                None,
                InputBindingPhase::Down,
            ));
            return 1;
        };

        dispatch(InputBindingInvocation::new(
            assignment,
            Some(definition),
            InputBindingPhase::Down,
        ));
        if definition.runs_on_up() {
            if let Some(active) = self
                .active_releases
                .iter_mut()
                .find(|active| active.control == control)
            {
                active.assignment = identifier;
            } else {
                self.active_releases.push(ActiveRelease {
                    control,
                    assignment: identifier,
                });
            }
        }
        1
    }

    fn dispatch_down<'a>(
        chord: StockChord,
        assignments: &'a UiBindingAssignments,
        catalog: &'a UiBindingCatalog,
        dispatch: &mut impl FnMut(InputBindingInvocation<'a>),
    ) -> usize {
        let Some(key) = chord.as_str() else {
            return 0;
        };
        let Some(assignment) = assignments.binding_for(key) else {
            return 0;
        };
        let definition = admitted_definition(assignment, catalog);
        if definition.is_none() && matches!(assignment.action(), UiBindingAction::Command(_)) {
            return 0;
        }
        dispatch(InputBindingInvocation::new(
            assignment,
            definition,
            InputBindingPhase::Down,
        ));
        1
    }

    fn release<'a>(
        &mut self,
        control: PhysicalControl,
        assignments: &'a UiBindingAssignments,
        catalog: &'a UiBindingCatalog,
        dispatch: &mut impl FnMut(InputBindingInvocation<'a>),
    ) -> usize {
        let Some(index) = self
            .active_releases
            .iter()
            .position(|active| active.control == control)
        else {
            return 0;
        };
        let active = self.active_releases.remove(index);
        Self::dispatch_release(active, assignments, catalog, dispatch)
    }

    fn release_all<'a>(
        &mut self,
        assignments: &'a UiBindingAssignments,
        catalog: &'a UiBindingCatalog,
        dispatch: &mut impl FnMut(InputBindingInvocation<'a>),
    ) -> usize {
        let mut count = 0;
        for active in self.active_releases.drain(..) {
            count += Self::dispatch_release(active, assignments, catalog, dispatch);
        }
        count
    }

    fn dispatch_release<'a>(
        active: ActiveRelease,
        assignments: &'a UiBindingAssignments,
        catalog: &'a UiBindingCatalog,
        dispatch: &mut impl FnMut(InputBindingInvocation<'a>),
    ) -> usize {
        let Some(assignment) = assignments.assignment(active.assignment) else {
            return 0;
        };
        let Some(definition) = admitted_definition(assignment, catalog) else {
            return 0;
        };
        if !definition.runs_on_up() {
            return 0;
        }
        dispatch(InputBindingInvocation::new(
            assignment,
            Some(definition),
            InputBindingPhase::Up,
        ));
        1
    }
}

/// Resolves a named action and applies the Win32 declaration gate.
fn admitted_definition<'a>(
    assignment: &UiBindingAssignment,
    catalog: &'a UiBindingCatalog,
) -> Option<&'a UiBindingDefinition> {
    let UiBindingAction::Command(name) = assignment.action() else {
        return None;
    };
    catalog.binding(name).filter(|definition| {
        !definition.is_debug() && definition.is_available_on(UiBindingPlatform::Windows)
    })
}
