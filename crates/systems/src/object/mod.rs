//! Coordinates common world-object creation, update, and removal behavior.
//!
//! `Object_C.cpp`, `ObjectAlloc.cpp`, and `ObjectMgrClient.cpp` evidence a
//! shared lifecycle responsibility. Storage details remain owned by ECS.

mod lifecycle;
mod types;
mod update;
