//! Frame hierarchy, strata, visibility, focus, scripts, and child ownership.
//!
//! The boundary follows `CSimpleFrame.cpp`, `CSimpleFrameScript.cpp`, and
//! `CSimpleTop.cpp`. Concrete widgets build on this state without exposing the
//! internal tree across the crate facade.

mod c_simple_frame;
mod c_simple_frame_script;
mod c_simple_top;
mod status;

pub use c_simple_frame::{UiInheritanceTarget, UiObjectCatalog, UiObjectDefinition, UiObjectKind};
pub use c_simple_top::{UiDrawLayer, UiElementLayer, UiObjectNode, UiObjectRole, UiObjectTree};
pub use status::UiObjectError;
