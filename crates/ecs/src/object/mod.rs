//! Common object identity, type, update-field, and lifecycle components.
//!
//! `Object_C.cpp` and `ObjectAlloc.cpp` establish the shared base of stock
//! world objects. Storage is implemented with Shipyard rather than recreating
//! the stock allocator hierarchy.

mod object_alloc;
mod object_c;
mod object_mgr_client;

pub use object_c::{ObjectFields, ObjectGuid, ObjectKind};
