//! Concrete stock widget behavior built on frames and script regions.
//!
//! The family includes the edit box, HTML, hyperlink, message, scrolling,
//! model, movie, button, and other concrete responsibilities evidenced by the
//! recovered `CSimple*` source names and RTTI descriptors.

mod button;
mod check_box;
mod color_select;
mod edit_box;
mod html;
mod hyperlink;
mod message;
mod model;
mod movie;
mod scroll_frame;
mod slider;
mod status_bar;
mod texture;

pub use texture::{
    UiBlendMode, UiTexCoords, UiTextureError, UiTextureFile, UiTextureLayer, UiTextureNode,
    UiTexturePlan,
};
