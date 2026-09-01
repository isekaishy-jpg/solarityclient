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

pub use addon::{
    AddonCatalog, AddonCatalogError, AddonCompatibility, AddonDefinition, STANDARD_ADDON_CRC,
    STOCK_INTERFACE_VERSION,
};
pub use animation::{
    UiAnimation, UiAnimationError, UiAnimationGroup, UiAnimationKind, UiAnimationLooping,
    UiAnimationPlan, UiAnimationValue,
};
pub use binding::{
    UiBindingAction, UiBindingAssignment, UiBindingAssignmentError, UiBindingAssignmentId,
    UiBindingAssignments, UiBindingCatalog, UiBindingDefinition, UiBindingDocument, UiBindingError,
    UiBindingKey, UiBindingMode, UiBindingPlatform, UiModifiedClickAssignment,
    UiModifiedClickChord, UiModifiedClickDefinition,
};
pub use event::{UiEventArgument, UiEventDispatch, UiEventError, UiEventPayload};
pub use feature::{
    MAX_BATTLEFIELD_QUEUES, MAX_WORLD_PVP_QUEUES, UiBattlefieldQueueError, UiBattlefieldQueueState,
    UiBattlefieldQueueStatus, UiBattlefieldSlot, UiMinimapTrackingState, UiTrackingCategory,
    UiTrackingError, UiTrackingType, UiWorldPvpQueueSlot,
};
pub use feature::{UiActionBarPageError, UiActionBarState};
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
pub use render::{
    UiPresentationPacket, UiPresentationPacketKey, UiPresentationPlan, UiRenderError, UiRenderPlan,
    UiTextureAssetBindings, UiTextureAssetPlan, UiTextureAssetRequest, UiTexturePresentation,
    UiTextureSource,
};
pub use script::{
    UiCharacterDirectory, UiCharacterInfo, UiDeferredRuntimeTemplate, UiGlueMediaIntent,
    UiGlueNetworkAction, UiGlueNetworkStatus, UiLoginRequest, UiRealmCategory, UiRealmDirectory,
    UiRealmFlags, UiRealmInfo, UiRealmVersion, UiRuntimeTemplate, UiRuntimeTemplateNode,
    UiRuntimeTemplatePlan, UiScriptBinding, UiScriptEnvironment, UiScriptError, UiScriptHandler,
    UiScriptNode, UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan, UiScriptTarget,
};
pub use widget::{
    UiBlendMode, UiGradientOrientation, UiTexCoords, UiTextureColor, UiTextureError, UiTextureFile,
    UiTextureGradient, UiTextureLayer, UiTextureNode, UiTexturePlan, UiTextureState,
    UiTextureStatePlan,
};
pub use world::{
    UiFactionGroup, UiPlayerFactionState, UiPlayerProgressionState, UiPlayerState, UiWorldState,
    UiZonePvpType, UiZoneState,
};
pub use xml::{
    LuaSource, UiBundle, UiLoadAction, UiLoadError, UiManifest, UiManifestEntry,
    UiManifestEntryKind, UiManifestKind, UiResource, UiResourceContent, XmlAttribute, XmlContent,
    XmlDocument, XmlElement,
};
