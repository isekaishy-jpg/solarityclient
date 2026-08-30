//! SDL3 input collection and translation into client actions.
//!
//! `InputControl.cpp` and the stock DirectInput import provide the source
//! boundary. UI bindings and player control consume actions without polling
//! SDL directly.

mod input_control;
