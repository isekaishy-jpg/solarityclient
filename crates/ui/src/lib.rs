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

pub use xml::{
    LuaSource, UiBundle, UiLoadError, UiManifest, UiManifestEntry, UiManifestEntryKind,
    UiManifestKind, UiResource, UiResourceContent, XmlAttribute, XmlContent, XmlDocument,
    XmlElement,
};
