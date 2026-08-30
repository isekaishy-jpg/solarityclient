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

pub use event::{UiEventArgument, UiEventDispatch, UiEventError, UiEventPayload};
pub use font::{
    FontCatalog, FontColor, FontDefinition, FontError, FontOutline, FontRasterization, FontShadow,
    FontSystem, HorizontalJustification, RasterizedGlyph, VerticalJustification,
};
pub use frame::{
    UiDrawLayer, UiElementLayer, UiFrameError, UiFrameLayer, UiFrameNode, UiFramePlan,
    UiFrameState, UiFrameStatePlan, UiFrameStrata, UiInheritanceTarget, UiObjectBatch,
    UiObjectCatalog, UiObjectDefinition, UiObjectError, UiObjectKind, UiObjectNode, UiObjectRole,
    UiObjectTree,
};
pub use glue::{GlueError, GlueManager, GlueObject, GlueStartupReport};
pub use region::{
    UiAnchor, UiAnchorTarget, UiDimensions, UiLayoutError, UiLayoutLayer, UiLayoutPlan,
    UiNodeLayout, UiPoint, UiRegionAnchor, UiRegionGeometry, UiRegionGeometryPlan, UiRegionState,
    UiRegionStatePlan, UiScreenRect,
};
pub use script::{
    UiRuntimeTemplate, UiRuntimeTemplateNode, UiRuntimeTemplatePlan, UiScriptBinding,
    UiScriptEnvironment, UiScriptError, UiScriptHandler, UiScriptNode, UiScriptPlan,
    UiScriptRuntime, UiScriptRuntimePlan, UiScriptTarget,
};
pub use widget::{
    UiBlendMode, UiGradientOrientation, UiTexCoords, UiTextureColor, UiTextureError, UiTextureFile,
    UiTextureGradient, UiTextureLayer, UiTextureNode, UiTexturePlan,
};
pub use xml::{
    LuaSource, UiBundle, UiLoadAction, UiLoadError, UiManifest, UiManifestEntry,
    UiManifestEntryKind, UiManifestKind, UiResource, UiResourceContent, XmlAttribute, XmlContent,
    XmlDocument, XmlElement,
};
