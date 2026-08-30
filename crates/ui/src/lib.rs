//! User-interface state, interaction, layout, and presentation boundaries.

mod addon;
mod animation;
mod binding;
mod event;
mod feature;
mod font;
mod frame;
mod glue;
mod notification;
mod region;
mod render;
mod script;
mod widget;
mod world;
mod xml;

pub use font::{
    FontCatalog, FontColor, FontDefinition, FontError, FontOutline, FontRasterization, FontShadow,
    FontSystem, HorizontalJustification, RasterizedGlyph, VerticalJustification,
};
pub use frame::{
    UiElementLayer, UiInheritanceTarget, UiObjectCatalog, UiObjectDefinition, UiObjectError,
    UiObjectKind, UiObjectNode, UiObjectRole, UiObjectTree,
};
pub use region::{
    UiAnchor, UiDimensions, UiLayoutError, UiLayoutLayer, UiLayoutPlan, UiNodeLayout, UiPoint,
};
pub use xml::{
    LuaSource, UiBundle, UiLoadAction, UiLoadError, UiManifest, UiManifestEntry,
    UiManifestEntryKind, UiManifestKind, UiResource, UiResourceContent, XmlAttribute, XmlContent,
    XmlDocument, XmlElement,
};
