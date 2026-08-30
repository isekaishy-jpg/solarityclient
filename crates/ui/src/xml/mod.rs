//! FrameXML and GlueXML parsing, template inheritance, and object construction.
//!
//! `XMLTree.cpp` and the `CSimple*` object families establish an XML-driven UI
//! pipeline. `quick-xml` provides tokenization; this module owns stock schema,
//! load order, inheritance, validation, and Lua binding behavior.

mod manifest;
mod status;
mod xml_tree;

pub use manifest::{
    LuaSource, UiBundle, UiManifest, UiManifestEntry, UiManifestEntryKind, UiManifestKind,
    UiResource, UiResourceContent,
};
pub use status::UiLoadError;
pub use xml_tree::{XmlAttribute, XmlContent, XmlDocument, XmlElement};
