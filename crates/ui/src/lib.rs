//! User-interface state, interaction, layout, and presentation boundaries.

mod addon;
mod animation;
mod binding;
mod event;
mod feature;
mod font;
mod frame;
mod glue;
mod input;
mod notification;
mod region;
mod render;
mod script;
mod widget;
mod world;
mod xml;

pub use addon::{
    AddonCatalog, AddonCatalogError, AddonCompatibility, AddonDefinition, STANDARD_ADDON_CRC,
    STOCK_INTERFACE_VERSION, UiAddonLoadState, UiSavedVariableState,
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
    MAX_BATTLEFIELD_QUEUES, MAX_WORLD_PVP_QUEUES, UI_BASE_SEND_MAIL_PRICE, UI_CHAT_WINDOW_COUNT,
    UiAccountExpansion, UiAccountState, UiBattlefieldQueueError, UiBattlefieldQueueState,
    UiBattlefieldQueueStatus, UiBattlefieldSlot, UiBattlegroundType, UiChannelCategory,
    UiChannelDisplay, UiChannelMember, UiChannelState, UiChatWindow, UiChatWindowState,
    UiCompanion, UiCompanionState, UiCompanionType, UiGroupFinderError, UiGroupFinderProposal,
    UiGroupFinderRole, UiGroupFinderRoleCheck, UiGroupFinderServerInfo, UiGroupFinderState,
    UiGroupRosterState, UiGuildState, UiLootMethod, UiLootState, UiMailComposeState,
    UiMinimapState, UiMinimapTrackingState, UiPetAction, UiPetActionState, UiPossessAction,
    UiQuestLogEntry, UiQuestLogQuest, UiQuestLogState, UiRune, UiRuneState, UiRuneType,
    UiShapeshiftForm, UiSkillLine, UiSkillLineSkill, UiSkillLineState, UiSocialQueryState,
    UiSpellBookState, UiSpellBookTab, UiStanceState, UiSupportState, UiTabardState,
    UiTrackingCategory, UiTrackingError, UiTrackingType, UiVoiceChatState, UiWorldMapState,
    UiWorldPvpQueueSlot, UiWorldStateIndicator, UiWorldStateUiState,
};
pub use feature::{
    UI_ACTION_SLOT_COUNT, UI_PET_ACTION_SLOT_COUNT, UI_RUNE_SLOT_COUNT, UiActionBarPageError,
    UiActionBarState, UiActionBarStateError,
};
pub use font::{
    FontCatalog, FontColor, FontDefinition, FontError, FontOutline, FontRasterization, FontShadow,
    FontSystem, HorizontalJustification, RasterizedGlyph, UiGlyphAtlasPlan, UiGlyphQuad,
    UiNativeTextStyle, VerticalJustification,
};
pub use frame::{
    UiBackdropFile, UiBackdropLayer, UiBackdropNode, UiBackdropPlan, UiBackdropState,
    UiBackdropStatePlan, UiDrawLayer, UiElementLayer, UiFrameError, UiFrameLayer, UiFrameNode,
    UiFramePlan, UiFrameState, UiFrameStatePlan, UiFrameStrata, UiInheritanceTarget, UiObjectBatch,
    UiObjectCatalog, UiObjectDefinition, UiObjectError, UiObjectKind, UiObjectNode, UiObjectRole,
    UiObjectTree,
};
pub use glue::{
    FrameManager, GlueError, GlueInitialScreen, GlueManager, GlueObject, GlueStartupReport,
    UiCharacterCreationError, UiCharacterCreationPreview, UiCharacterCreationRequest,
    UiCharacterCreationState, UiCharacterExpansion, UiCreationClassRoles, UiKeyboardModifiers,
    UiPointerButton, UiPointerDispatch,
};
pub use input::{UiModifierKeyState, UiModifierKeys};
pub use region::{
    UiAnchor, UiAnchorTarget, UiDimensions, UiLayoutError, UiLayoutLayer, UiLayoutPlan,
    UiNodeLayout, UiPoint, UiRegionAnchor, UiRegionGeometry, UiRegionGeometryPlan, UiRegionState,
    UiRegionStatePlan, UiScreenRect,
};
pub use render::{
    UiMinimapPresentation, UiModelFog, UiModelLight, UiModelLightSets, UiModelPresentation,
    UiPresentationPacket, UiPresentationPacketKey, UiPresentationPlan, UiRenderError, UiRenderPlan,
    UiTextureAssetBindings, UiTextureAssetPlan, UiTextureAssetRequest, UiTexturePresentation,
    UiTextureSource,
};
pub use script::{
    UiCharacterDirectory, UiCharacterEquipment, UiCharacterInfo, UiCharacterPetPreview,
    UiCharacterSelectionPreview, UiClientClock, UiDeferredRuntimeTemplate, UiGlueMediaAction,
    UiGlueMediaIntent, UiGlueMovieRequest, UiGlueNetworkAction, UiGlueNetworkStatus,
    UiLoginRequest, UiModelAction, UiModelInstance, UiMovementAction, UiMovementCommand,
    UiMovementControl, UiProcessAction, UiRealmCategory, UiRealmDirectory, UiRealmFlags,
    UiRealmInfo, UiRealmSort, UiRealmVersion, UiRuntimeTemplate, UiRuntimeTemplateNode,
    UiRuntimeTemplatePlan, UiScriptBinding, UiScriptEnvironment, UiScriptError, UiScriptHandler,
    UiScriptNode, UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan, UiScriptTarget,
};
pub use widget::{
    UiBlendMode, UiGradientOrientation, UiScrollFramePlan, UiScrollFrameState,
    UiSimpleHtmlAlignment, UiSimpleHtmlBlock, UiSimpleHtmlDocument, UiSimpleHtmlError,
    UiSimpleHtmlFontSlot, UiSimpleHtmlLine, UiSimpleHtmlNode, UiSimpleHtmlPlan, UiTexCoords,
    UiTextureColor, UiTextureError, UiTextureFile, UiTextureGradient, UiTextureLayer,
    UiTextureNode, UiTexturePlan, UiTextureState, UiTextureStatePlan,
};
pub use world::{
    UiCombatLogEntry, UiCombatLogEventError, UiCombatLogObject, UiCombatLogSpell, UiCombatLogState,
    UiFactionGroup, UiFriendCounts, UiInstanceType, UiPlayerClassState, UiPlayerFactionState,
    UiPlayerIdentityState, UiPlayerLanguage, UiPlayerProgressionState, UiPlayerRaceState,
    UiPlayerState, UiPlayerStatsState, UiPlayerVitalsState, UiRealmDate, UiRealmDateError,
    UiRealmTime, UiRealmTimeError, UiUnitPowerType, UiWorldState, UiZonePvpType, UiZoneState,
};
pub use xml::{
    LuaSource, UiBundle, UiLoadAction, UiLoadError, UiManifest, UiManifestEntry,
    UiManifestEntryKind, UiManifestKind, UiResource, UiResourceContent, XmlAttribute, XmlContent,
    XmlDocument, XmlElement,
};
