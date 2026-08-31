//! SDL3 input collection and translation into client actions.
//!
//! `InputControl.cpp` and the stock DirectInput import provide the source
//! boundary. UI bindings and player control consume actions without polling
//! SDL directly.

mod binding;
mod input_control;
mod state;
mod types;

pub use binding::{InputBindingInvocation, InputBindingPhase, InputBindingRouter};
pub use input_control::InputControl;
pub use types::{InputFrameMotion, PointerPosition};
