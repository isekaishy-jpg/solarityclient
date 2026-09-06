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
pub(crate) use message::{Message, MessageConfig, MessageHistory};

pub use texture::{
    UiBlendMode, UiGradientOrientation, UiTexCoords, UiTextureColor, UiTextureError, UiTextureFile,
    UiTextureGradient, UiTextureLayer, UiTextureNode, UiTexturePlan, UiTextureState,
    UiTextureStatePlan,
};

pub use html::{
    UiSimpleHtmlAlignment, UiSimpleHtmlBlock, UiSimpleHtmlDocument, UiSimpleHtmlError,
    UiSimpleHtmlFontSlot, UiSimpleHtmlLine, UiSimpleHtmlNode, UiSimpleHtmlPlan,
};
pub(crate) use scroll_frame::nearest_owning_scroll_frame;
pub use scroll_frame::{UiScrollFramePlan, UiScrollFrameState};

pub(crate) use texture::canonical_texture_asset;
