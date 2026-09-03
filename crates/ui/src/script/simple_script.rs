//! Ordered Lua source execution and stock object identity methods.

mod buttons;
mod cvars;
mod globals;
mod tooltips;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::rc::Rc;

use mlua::{LightUserData, Lua, MultiValue, RegistryKey, Table, Value, Variadic};
use solarity_asset::{AssetPath, AssetStore, AssetStoreHandle, Locale};

use crate::event::{UiEventArgument, UiEventPayload, canonical_frame_event, canonical_glue_event};
use crate::script::UiGlueNetworkBridge;
use crate::{
    FontCatalog, FontDefinition, FontOutline, FontShadow, HorizontalJustification, UiAnchorTarget,
    UiAnimationPlan, UiBindingAssignments, UiBlendMode, UiBundle, UiDrawLayer, UiFrameStatePlan,
    UiFrameStrata, UiKeyboardModifiers, UiLoadAction, UiManifestKind, UiObjectBatch, UiObjectKind,
    UiObjectRole, UiObjectTree, UiPoint, UiRegionGeometryPlan, UiRegionStatePlan,
    UiResourceContent, UiRuntimeTemplatePlan, UiScriptError, UiScriptHandler, UiScriptPlan,
    UiScriptTarget, UiTextureFile, UiTextureStatePlan, VerticalJustification, XmlContent,
};

use crate::animation::{
    UiAnimationMetatables, advance_animations, create_animation_metatables,
    register_owner_animations,
};

use self::cvars::UiCVarRegistry;
use self::globals::register_base_globals;
use super::handlers::handler_for;
use super::runtime_state::{finite_region_number, snapshot_slider};
use super::templates::TEMPLATE_REGISTRY;

pub(crate) const OBJECT_REGISTRY: &str = "solarity.ui.objects";
const METATABLE_REGISTRY: &str = "solarity.ui.object_metatables";
const LIVE_STATE_GENERATION_REGISTRY: &str = "solarity.ui.live_state_generation";
const ON_UPDATE_OBJECTS_REGISTRY: &str = "solarity.ui.on_update_objects";
const ON_UPDATE_MEMBERS_REGISTRY: &str = "solarity.ui.on_update_members";
const ON_UPDATE_SEEN_REGISTRY: &str = "solarity.ui.on_update_seen";
const FOCUSED_EDIT_BOX_REGISTRY: &str = "solarity.ui.focused_edit_box";
// Distinct values prevent identical-data folding from merging these private
// light-userdata keys in optimized builds.
static NAME_TOKEN: u8 = 1;
static TYPE_TOKEN: u8 = 2;
static PARENT_TOKEN: u8 = 3;
static EVENTS_TOKEN: u8 = 4;
static ALL_EVENTS_TOKEN: u8 = 5;
static WIDTH_TOKEN: u8 = 6;
static HEIGHT_TOKEN: u8 = 7;
static BACKDROP_COLOR_TOKEN: u8 = 8;
static BACKDROP_BORDER_COLOR_TOKEN: u8 = 9;
static INDEX_TOKEN: u8 = 10;
static ANCHORS_TOKEN: u8 = 11;
static ENABLED_TOKEN: u8 = 12;
static HORIZONTAL_SCROLL_TOKEN: u8 = 13;
static VERTICAL_SCROLL_TOKEN: u8 = 14;
static HORIZONTAL_SCROLL_RANGE_TOKEN: u8 = 15;
static VERTICAL_SCROLL_RANGE_TOKEN: u8 = 16;
static SLIDER_MIN_TOKEN: u8 = 17;
static SLIDER_MAX_TOKEN: u8 = 18;
static SLIDER_VALUE_TOKEN: u8 = 19;
static SLIDER_STEP_TOKEN: u8 = 20;
static SHOWN_TOKEN: u8 = 21;
static ID_TOKEN: u8 = 22;
static NORMAL_FONT_TOKEN: u8 = 23;
static DISABLED_FONT_TOKEN: u8 = 24;
static HIGHLIGHT_FONT_TOKEN: u8 = 25;
static TEXT_TOKEN: u8 = 26;
static HIGHLIGHT_LOCKED_TOKEN: u8 = 27;
static FONT_SET_TOKEN: u8 = 28;
static FONT_OBJECT_TOKEN: u8 = 29;
static TEX_COORD_TOKEN: u8 = 30;
static JUSTIFY_H_TOKEN: u8 = 31;
static JUSTIFY_V_TOKEN: u8 = 32;
static CHECKED_TOKEN: u8 = 33;
static MODEL_CAMERA_TOKEN: u8 = 34;
static MODEL_SEQUENCE_TOKEN: u8 = 35;
static MODEL_FILE_TOKEN: u8 = 36;
static CLICK_ACTION_TOKEN: u8 = 37;
static MODEL_SEQUENCE_TIME_SEQUENCE_TOKEN: u8 = 38;
static MODEL_SEQUENCE_TIME_TOKEN: u8 = 39;
static FRAME_LEVEL_TOKEN: u8 = 40;
static MODEL_SCALE_TOKEN: u8 = 41;
static KEYBOARD_ENABLED_TOKEN: u8 = 42;
static TEXTURE_COLOR_TOKEN: u8 = 43;
static NORMAL_TEXTURE_TOKEN: u8 = 44;
static PUSHED_TEXTURE_TOKEN: u8 = 45;
static DISABLED_TEXTURE_TOKEN: u8 = 46;
static HIGHLIGHT_TEXTURE_TOKEN: u8 = 47;
static SCRIPT_HANDLERS_TOKEN: u8 = 48;
static BUTTON_TEXT_TOKEN: u8 = 49;
static ROLE_TOKEN: u8 = 50;
static ALPHA_TOKEN: u8 = 51;
static SCALE_TOKEN: u8 = 52;
static TEXTURE_FILE_TOKEN: u8 = 53;
static TEXTURE_SOLID_COLOR_TOKEN: u8 = 54;
static TEXTURE_BLEND_MODE_TOKEN: u8 = 55;
static HORIZONTAL_TILING_TOKEN: u8 = 56;
static VERTICAL_TILING_TOKEN: u8 = 57;
static NON_BLOCKING_TOKEN: u8 = 58;
static DRAW_LAYER_TOKEN: u8 = 59;
static DRAW_SUB_LEVEL_TOKEN: u8 = 60;
static FRAME_STRATA_TOKEN: u8 = 61;
static FRAME_DEPTH_TOKEN: u8 = 62;
static IGNORE_DEPTH_TOKEN: u8 = 63;
static FONT_FACE_TOKEN: u8 = 64;
static FONT_HEIGHT_TOKEN: u8 = 65;
static FONT_FLAGS_TOKEN: u8 = 66;
static MOUSE_ENABLED_TOKEN: u8 = 67;
static ATTRIBUTES_TOKEN: u8 = 68;
static STATUS_BAR_COLOR_TOKEN: u8 = 69;
static STATUS_BAR_TEXTURE_TOKEN: u8 = 70;
static TOOLTIP_OWNER_TOKEN: u8 = 71;
static TOOLTIP_ANCHOR_TOKEN: u8 = 72;
static TOOLTIP_OFFSET_X_TOKEN: u8 = 73;
static TOOLTIP_OFFSET_Y_TOKEN: u8 = 74;
static TEXT_COLOR_TOKEN: u8 = 75;
static DESATURATED_TOKEN: u8 = 76;
static DRAG_BUTTON_TOKEN: u8 = 77;
static SPACING_TOKEN: u8 = 78;
static SCROLL_CHILD_TOKEN: u8 = 79;
static EDIT_FOCUSED_TOKEN: u8 = 80;
static EDIT_ALT_ARROW_TOKEN: u8 = 81;
static EDIT_HISTORY_LINES_TOKEN: u8 = 82;
static EDIT_HISTORY_TOKEN: u8 = 83;
static EDIT_MAX_LETTERS_TOKEN: u8 = 84;
static EDIT_MAX_BYTES_TOKEN: u8 = 85;
static EDIT_CURSOR_TOKEN: u8 = 86;
static EDIT_SELECTION_START_TOKEN: u8 = 87;
static EDIT_SELECTION_END_TOKEN: u8 = 88;
static EDIT_BLINK_SPEED_TOKEN: u8 = 89;
static EDIT_PASSWORD_TOKEN: u8 = 90;
static EDIT_NUMERIC_TOKEN: u8 = 91;
static EDIT_MULTI_LINE_TOKEN: u8 = 92;
static EDIT_COUNT_INVISIBLE_TOKEN: u8 = 93;
static EDIT_AUTO_FOCUS_TOKEN: u8 = 94;
static EDIT_TEXT_INSETS_TOKEN: u8 = 95;
static FRAME_CLAMPED_TOKEN: u8 = 96;
static FRAME_CLAMP_INSETS_TOKEN: u8 = 97;
static FONT_SHADOW_OFFSET_TOKEN: u8 = 98;
static FONT_SHADOW_COLOR_TOKEN: u8 = 99;
static FRAME_MOVABLE_TOKEN: u8 = 100;
static FRAME_RESIZABLE_TOKEN: u8 = 101;
static FRAME_TOP_LEVEL_TOKEN: u8 = 102;
static FRAME_USER_PLACED_TOKEN: u8 = 103;
static FRAME_DONT_SAVE_POSITION_TOKEN: u8 = 104;
static PORTRAIT_UNIT_TOKEN: u8 = 105;
static HIT_RECT_INSETS_TOKEN: u8 = 106;
static MOUSE_WHEEL_ENABLED_TOKEN: u8 = 107;
static TOOLTIP_PADDING_TOKEN: u8 = 108;
static MOVIE_SUBTITLES_TOKEN: u8 = 109;
static BUTTON_PRESSED_TOKEN: u8 = 110;
static MODEL_FOG_COLOR_TOKEN: u8 = 111;
static MODEL_FOG_NEAR_TOKEN: u8 = 112;
static MODEL_FOG_FAR_TOKEN: u8 = 113;
static MODEL_GLOW_TOKEN: u8 = 114;
static MODEL_BACKGROUND_LIGHT_LIVE_TOKEN: u8 = 115;
static MODEL_BACKGROUND_LIGHT_GHOST_TOKEN: u8 = 116;
static MODEL_CHARACTER_LIGHT_LIVE_TOKEN: u8 = 117;
static MODEL_CHARACTER_LIGHT_GHOST_TOKEN: u8 = 118;
static MODEL_PET_LIGHT_LIVE_TOKEN: u8 = 119;
static MODEL_PET_LIGHT_GHOST_TOKEN: u8 = 120;
static BUTTON_STATE_LOCKED_TOKEN: u8 = 121;
static SLIDER_ORIENTATION_TOKEN: u8 = 122;
static HOVERED_TOKEN: u8 = 123;
static DISABLED_TEXT_COLOR_TOKEN: u8 = 124;
static AUTO_TEXT_WIDTH_TOKEN: u8 = 125;
static AUTO_TEXT_HEIGHT_TOKEN: u8 = 126;
static WORD_WRAP_TOKEN: u8 = 127;
static NON_SPACE_WRAP_TOKEN: u8 = 128;
static MAX_TEXT_LINES_TOKEN: u8 = 129;
static EDIT_CARET_ELAPSED_TOKEN: u8 = 130;
static EDIT_CARET_VISIBLE_TOKEN: u8 = 131;
static EDIT_HIGHLIGHT_COLOR_TOKEN: u8 = 132;

const OBJECT_KINDS: [UiObjectKind; 21] = [
    UiObjectKind::Frame,
    UiObjectKind::Button,
    UiObjectKind::CheckButton,
    UiObjectKind::ColorSelect,
    UiObjectKind::Cooldown,
    UiObjectKind::EditBox,
    UiObjectKind::FontString,
    UiObjectKind::GameTooltip,
    UiObjectKind::MessageFrame,
    UiObjectKind::Minimap,
    UiObjectKind::QuestPoiFrame,
    UiObjectKind::Model,
    UiObjectKind::ModelFfx,
    UiObjectKind::MovieFrame,
    UiObjectKind::ScrollFrame,
    UiObjectKind::ScrollingMessageFrame,
    UiObjectKind::SimpleHtml,
    UiObjectKind::Slider,
    UiObjectKind::StatusBar,
    UiObjectKind::Texture,
    UiObjectKind::WorldFrame,
];

#[derive(Clone, Copy)]
struct InitialAnchor {
    point: UiPoint,
    target: Option<usize>,
    relative_point: UiPoint,
    offset: (f64, f64),
}

#[derive(Clone, Debug)]
struct InitialFont {
    assigned: bool,
    object_name: Option<String>,
    text_reference: Option<String>,
    justify_h: String,
    justify_v: String,
    spacing: f64,
    word_wrap: bool,
    non_space_wrap: bool,
    max_lines: u32,
    edit_max_letters: u32,
    edit_password: bool,
    edit_multiline: bool,
    edit_text_insets: [f64; 4],
    edit_highlight_color: [f64; 4],
}

impl Default for InitialFont {
    fn default() -> Self {
        Self {
            assigned: false,
            object_name: None,
            text_reference: None,
            justify_h: "CENTER".to_owned(),
            justify_v: "MIDDLE".to_owned(),
            spacing: 0.0,
            word_wrap: true,
            non_space_wrap: false,
            max_lines: 0,
            edit_max_letters: 0,
            edit_password: false,
            edit_multiline: false,
            edit_text_insets: [0.0; 4],
            edit_highlight_color: [96.0 / 255.0, 96.0 / 255.0, 96.0 / 255.0, 1.0],
        }
    }
}

#[derive(Clone, Debug, Default)]
struct InitialButton {
    text_reference: Option<String>,
    normal_font: Option<String>,
    disabled_font: Option<String>,
    highlight_font: Option<String>,
}

#[derive(Clone, Debug)]
struct InitialTexture {
    file: Option<String>,
    coords: [f64; 8],
    colors: [[f64; 4]; 4],
    blend_mode: &'static str,
    horizontal_tiling: bool,
    vertical_tiling: bool,
    non_blocking: bool,
    draw_layer: &'static str,
    draw_sub_level: i16,
}

impl Default for InitialTexture {
    fn default() -> Self {
        Self {
            file: None,
            coords: [0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0],
            colors: [[1.0, 1.0, 1.0, 1.0]; 4],
            blend_mode: "BLEND",
            horizontal_tiling: false,
            vertical_tiling: false,
            non_blocking: false,
            draw_layer: "ARTWORK",
            draw_sub_level: 0,
        }
    }
}

/// Incremental executor for one built-in UI bundle.
///
/// Object tables are registered at their exact XML action rather than before
/// execution begins. The initial identity methods are shared by one metatable;
/// concrete widget method tables extend this boundary in later stages.
pub struct UiScriptRuntime {
    next_action: usize,
    object_metatables: Vec<RegistryKey>,
    font_metatable: RegistryKey,
    animation_metatables: UiAnimationMetatables,
    animations: UiAnimationPlan,
    font_actions: Vec<Option<FontDefinition>>,
    region_dimensions: Vec<(f64, f64)>,
    region_shown: Vec<bool>,
    region_alpha: Vec<f64>,
    region_scale: Vec<f64>,
    region_anchors: Vec<Vec<InitialAnchor>>,
    font_strings: Vec<InitialFont>,
    buttons: Vec<InitialButton>,
    textures: Vec<InitialTexture>,
    simple_html: crate::UiSimpleHtmlPlan,
    frame_ids: Vec<Option<i32>>,
    frame_levels: Vec<Option<i32>>,
    frame_strata: Vec<Option<&'static str>>,
    frame_keyboard_enabled: Vec<Option<bool>>,
    frame_mouse_enabled: Vec<Option<bool>>,
    frame_clamped_to_screen: Vec<Option<bool>>,
    frame_movable: Vec<Option<bool>>,
    frame_resizable: Vec<Option<bool>>,
    frame_top_level: Vec<Option<bool>>,
    frame_dont_save_position: Vec<Option<bool>>,
    text_measurement: buttons::TextMeasurement,
    registered_objects: Rc<Cell<usize>>,
    executed_chunks: usize,
    executed_load_handlers: usize,
}

/// Resolved, mutually aligned plans consumed by ordered Lua construction.
pub struct UiScriptRuntimePlan<'plan, 'bundle> {
    tree: &'plan UiObjectTree<'bundle>,
    animations: &'plan UiAnimationPlan,
    frames: &'plan UiFrameStatePlan,
    regions: &'plan UiRegionStatePlan,
    templates: &'plan UiRuntimeTemplatePlan,
    fonts: &'plan FontCatalog,
    texture_states: &'plan UiTextureStatePlan,
}

impl<'plan, 'bundle> UiScriptRuntimePlan<'plan, 'bundle> {
    /// Groups the typed plans that must describe the same UI object arena.
    #[must_use]
    pub const fn new(
        tree: &'plan UiObjectTree<'bundle>,
        animations: &'plan UiAnimationPlan,
        frames: &'plan UiFrameStatePlan,
        regions: &'plan UiRegionStatePlan,
        templates: &'plan UiRuntimeTemplatePlan,
        fonts: &'plan FontCatalog,
        texture_states: &'plan UiTextureStatePlan,
    ) -> Self {
        Self {
            tree,
            animations,
            frames,
            regions,
            templates,
            fonts,
            texture_states,
        }
    }
}

/// One ordered stock audio operation emitted by GlueXML.
#[derive(Clone, Debug, PartialEq)]
pub enum UiGlueMediaAction {
    /// `PlaySound` with a numeric identifier or script lookup name.
    PlaySound(String),
    /// `PlaySoundFile` with one exact archive path.
    PlaySoundFile(String),
    /// `PlayMusic` with one exact archive path.
    PlayMusic(String),
    /// `PlayGlueMusic` with one internal `SoundEntries` name.
    PlayGlueMusic(String),
    /// `PlayCreditsMusic` with one internal `SoundEntries` name.
    PlayCreditsMusic(String),
    /// `PlayGlueAmbience` with its authored fade-in duration.
    PlayGlueAmbience {
        /// Internal `SoundEntries` name.
        name: String,
        /// Requested fade-in duration in seconds.
        fade_seconds: f64,
    },
    /// `StopMusic` or `StopGlueMusic`.
    StopMusic,
    /// `StopGlueAmbience`.
    StopGlueAmbience,
    /// `StopAllSFX` with its authored fade-out duration.
    StopAllSfx {
        /// Requested fade-out duration in seconds.
        fade_seconds: f64,
    },
}

/// Retained stock audio state and ordered requests awaiting the media backend.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiGlueMediaIntent {
    pub(crate) music: Option<String>,
    pub(crate) ambience: Option<String>,
    actions: VecDeque<UiGlueMediaAction>,
    movie: Option<UiGlueMovieRequest>,
    movie_stop_completion: Option<usize>,
    next_movie_generation: u64,
}

/// One active stock MovieFrame request resolved to a locale-loose AVI.
#[derive(Clone, Debug, PartialEq)]
pub struct UiGlueMovieRequest {
    object_index: usize,
    generation: u64,
    path: std::path::PathBuf,
    volume: u32,
    subtitles_enabled: bool,
}

impl UiGlueMovieRequest {
    /// Returns the live MovieFrame arena index receiving completion callbacks.
    #[must_use]
    pub const fn object_index(&self) -> usize {
        self.object_index
    }

    /// Returns the monotonically increasing playback generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the exact locale-loose AVI selected through the file stack.
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Returns the unmodified integer volume supplied by stock Glue Lua.
    #[must_use]
    pub const fn volume(&self) -> u32 {
        self.volume
    }

    /// Returns the subtitle policy captured when playback began.
    #[must_use]
    pub const fn subtitles_enabled(&self) -> bool {
        self.subtitles_enabled
    }
}

impl UiGlueMediaIntent {
    /// Returns the most recently requested Glue music resource.
    #[must_use]
    pub fn music(&self) -> Option<&str> {
        self.music.as_deref()
    }

    /// Returns the most recently requested Glue ambience resource.
    #[must_use]
    pub fn ambience(&self) -> Option<&str> {
        self.ambience.as_deref()
    }

    /// Returns the currently active locale-loose Glue movie request.
    #[must_use]
    pub const fn movie(&self) -> Option<&UiGlueMovieRequest> {
        self.movie.as_ref()
    }

    /// Takes the oldest unconsumed audio operation.
    pub(crate) fn take_action(&mut self) -> Option<UiGlueMediaAction> {
        self.actions.pop_front()
    }

    pub(crate) fn retire_movie(&mut self, object_index: usize) {
        if self
            .movie
            .as_ref()
            .is_some_and(|movie| movie.object_index == object_index)
        {
            self.movie = None;
        }
        if self.movie_stop_completion == Some(object_index) {
            self.movie_stop_completion = None;
        }
    }

    pub(crate) fn take_movie_stop_completion(&mut self) -> Option<usize> {
        self.movie_stop_completion.take()
    }
}

/// Immutable process facts required by built-in Lua globals.
#[derive(Clone)]
pub struct UiScriptEnvironment {
    logical_extent: (u32, u32),
    ui_extent: (f64, f64),
    streaming_trial: bool,
    client_clock: crate::UiClientClock,
    cvars: UiCVarRegistry,
    assets: Option<AssetStoreHandle>,
    character_creation: Option<crate::UiCharacterCreationState>,
    media_intent: Rc<RefCell<UiGlueMediaIntent>>,
    network: Rc<RefCell<UiGlueNetworkBridge>>,
    process: Rc<RefCell<crate::script::UiProcessBridge>>,
    current_screen: Rc<RefCell<String>>,
    cursor_visible: Rc<Cell<bool>>,
    cursor_position: Rc<Cell<(f64, f64)>>,
    mouse_focus: Rc<Cell<Option<usize>>>,
    modifiers: crate::UiModifierKeyState,
    world: crate::UiWorldState,
    account: crate::UiAccountState,
    action_bar: crate::UiActionBarState,
    battlefield: crate::UiBattlefieldQueueState,
    chat_windows: crate::UiChatWindowState,
    channels: crate::UiChannelState,
    companions: crate::UiCompanionState,
    loot: crate::UiLootState,
    mail: crate::UiMailComposeState,
    minimap_tracking: crate::UiMinimapTrackingState,
    group_finder: crate::UiGroupFinderState,
    group_roster: crate::UiGroupRosterState,
    guild: crate::UiGuildState,
    quest_log: crate::UiQuestLogState,
    runes: crate::UiRuneState,
    pet_actions: crate::UiPetActionState,
    skill_lines: crate::UiSkillLineState,
    social_queries: crate::UiSocialQueryState,
    spell_book: crate::UiSpellBookState,
    stances: crate::UiStanceState,
    tabard: crate::UiTabardState,
    voice_chat: crate::UiVoiceChatState,
    world_map: crate::UiWorldMapState,
    world_state_ui: crate::UiWorldStateUiState,
    addons: crate::UiAddonLoadState,
    saved_variables: crate::UiSavedVariableState,
    bindings: Option<Rc<RefCell<UiBindingAssignments>>>,
    battlenet: crate::feature::UiBattleNetState,
    locale: Option<Locale>,
    existing_locales: Rc<[Locale]>,
}

impl UiScriptEnvironment {
    /// Derives the stock 768-unit UI canvas from a logical window extent.
    ///
    /// # Errors
    ///
    /// Returns [`UiScriptError::Plan`] when either logical dimension is zero.
    pub fn new(
        logical_width: u32,
        logical_height: u32,
        streaming_trial: bool,
    ) -> Result<Self, UiScriptError> {
        if logical_width == 0 || logical_height == 0 {
            return Err(UiScriptError::Plan {
                message: "UI script logical window extent must be nonzero".to_owned(),
            });
        }
        let ui_height = 768.0;
        let ui_width = f64::from(logical_width) / f64::from(logical_height) * ui_height;
        Ok(Self {
            logical_extent: (logical_width, logical_height),
            ui_extent: (ui_width, ui_height),
            streaming_trial,
            client_clock: crate::UiClientClock::new(),
            cvars: UiCVarRegistry::stock_initial(),
            assets: None,
            character_creation: None,
            media_intent: Rc::new(RefCell::new(UiGlueMediaIntent::default())),
            network: Rc::new(RefCell::new(UiGlueNetworkBridge::default())),
            process: Rc::new(RefCell::new(crate::script::UiProcessBridge::default())),
            current_screen: Rc::new(RefCell::new(String::new())),
            cursor_visible: Rc::new(Cell::new(true)),
            cursor_position: Rc::new(Cell::new((0.0, 0.0))),
            mouse_focus: Rc::new(Cell::new(None)),
            modifiers: crate::UiModifierKeyState::new(),
            world: crate::UiWorldState::new(),
            account: crate::UiAccountState::new(),
            action_bar: crate::UiActionBarState::new(),
            battlefield: crate::UiBattlefieldQueueState::new(),
            chat_windows: crate::UiChatWindowState::new(),
            channels: crate::UiChannelState::new(),
            companions: crate::UiCompanionState::new(),
            loot: crate::UiLootState::new(),
            mail: crate::UiMailComposeState::new(),
            minimap_tracking: crate::UiMinimapTrackingState::new(),
            group_finder: crate::UiGroupFinderState::new(),
            group_roster: crate::UiGroupRosterState::new(),
            guild: crate::UiGuildState::new(),
            quest_log: crate::UiQuestLogState::new(),
            runes: crate::UiRuneState::new(),
            pet_actions: crate::UiPetActionState::new(),
            skill_lines: crate::UiSkillLineState::new(),
            social_queries: crate::UiSocialQueryState::new(),
            spell_book: crate::UiSpellBookState::new(),
            stances: crate::UiStanceState::new(),
            tabard: crate::UiTabardState::new(),
            voice_chat: crate::UiVoiceChatState::new(),
            world_map: crate::UiWorldMapState::new(),
            world_state_ui: crate::UiWorldStateUiState::new(),
            addons: crate::UiAddonLoadState::default(),
            saved_variables: crate::UiSavedVariableState::new(),
            bindings: None,
            // A process without an attached Battle.net platform service must
            // not expose a second authentication or social-network path.
            battlenet: crate::feature::UiBattleNetState::new(false),
            locale: None,
            existing_locales: Rc::from([]),
        })
    }

    /// Returns the SDL logical window extent used to derive UI coordinates.
    #[must_use]
    pub const fn logical_extent(&self) -> (u32, u32) {
        self.logical_extent
    }

    /// Returns the stock aspect-compensated UI width and fixed 768-unit height.
    #[must_use]
    pub const fn ui_extent(&self) -> (f64, f64) {
        self.ui_extent
    }

    /// Returns whether assets come from the stock streaming-trial mode.
    #[must_use]
    pub const fn streaming_trial(&self) -> bool {
        self.streaming_trial
    }

    /// Returns the monotonic process-relative client clock.
    #[must_use]
    pub const fn client_clock(&self) -> crate::UiClientClock {
        self.client_clock
    }

    /// Returns one registered console variable's current script-visible text.
    #[must_use]
    pub fn cvar_value(&self, name: &str) -> Option<String> {
        self.cvars.get(name)
    }

    /// Applies stock's numeric truth test to one registered console variable.
    #[must_use]
    pub fn cvar_boolean(&self, name: &str) -> bool {
        self.cvars.boolean(name)
    }

    /// Applies Config.wtf values before built-in scripts execute.
    #[must_use]
    pub(crate) fn with_cvar_values(self, values: &[(String, String)]) -> Self {
        for (name, value) in values {
            self.cvars.load(name, value.clone());
        }
        self
    }

    /// Takes CVars changed by native script calls since the previous poll.
    pub(crate) fn take_changed_cvars(&self) -> Vec<(String, String)> {
        self.cvars.take_changed()
    }

    /// Attaches the mounted stock archive stack used by synchronous UI loads.
    #[must_use]
    pub fn with_asset_store(mut self, store: AssetStore) -> Self {
        self.locale = Some(store.locale());
        self.existing_locales = Rc::from(store.existing_locales());
        self.assets = Some(AssetStoreHandle::new(store));
        self
    }

    /// Attaches the effective stock key and modified-click assignment set.
    #[must_use]
    pub fn with_binding_assignments(mut self, assignments: UiBindingAssignments) -> Self {
        self.bindings = Some(Rc::new(RefCell::new(assignments)));
        self
    }

    /// Attaches the catalog-ordered AddOn loading progress exposed to FrameXML.
    #[must_use]
    pub fn with_addon_load_state(mut self, addons: crate::UiAddonLoadState) -> Self {
        self.addons = addons;
        self
    }

    /// Attaches a main-thread archive stack already owned by a UI manager.
    #[must_use]
    pub(crate) fn with_shared_asset_store(mut self, store: AssetStoreHandle) -> Self {
        {
            let store = store.borrow();
            self.locale = Some(store.locale());
            self.existing_locales = Rc::from(store.existing_locales());
        }
        self.assets = Some(store);
        self
    }

    /// Attaches the DBC-backed character-creation state used by Glue natives.
    #[must_use]
    pub(crate) fn with_character_creation_state(
        mut self,
        state: crate::UiCharacterCreationState,
    ) -> Self {
        self.character_creation = Some(state);
        self
    }

    fn cvars(&self) -> UiCVarRegistry {
        self.cvars.clone()
    }

    fn assets(&self) -> Option<AssetStoreHandle> {
        self.assets.clone()
    }

    pub(crate) fn character_creation_state(&self) -> Option<crate::UiCharacterCreationState> {
        self.character_creation.clone()
    }

    fn binding_assignments(&self) -> Option<Rc<RefCell<UiBindingAssignments>>> {
        self.bindings.clone()
    }

    fn addon_load_state(&self) -> crate::UiAddonLoadState {
        self.addons.clone()
    }

    /// Returns built-in saved-variable declarations collected during startup.
    #[must_use]
    pub fn saved_variable_state(&self) -> crate::UiSavedVariableState {
        self.saved_variables.clone()
    }

    fn battlenet_state(&self) -> crate::feature::UiBattleNetState {
        self.battlenet.clone()
    }

    fn locale(&self) -> Option<Locale> {
        self.locale
    }

    fn existing_locales(&self) -> Rc<[Locale]> {
        self.existing_locales.clone()
    }

    pub(crate) fn media_intent(&self) -> Rc<RefCell<UiGlueMediaIntent>> {
        self.media_intent.clone()
    }

    pub(crate) fn network(&self) -> Rc<RefCell<UiGlueNetworkBridge>> {
        self.network.clone()
    }

    pub(crate) fn process(&self) -> Rc<RefCell<crate::script::UiProcessBridge>> {
        self.process.clone()
    }

    pub(crate) fn current_screen(&self) -> Rc<RefCell<String>> {
        self.current_screen.clone()
    }

    pub(crate) fn cursor_visible(&self) -> Rc<Cell<bool>> {
        self.cursor_visible.clone()
    }

    pub(crate) fn cursor_position(&self) -> Rc<Cell<(f64, f64)>> {
        self.cursor_position.clone()
    }

    pub(crate) fn mouse_focus(&self) -> Rc<Cell<Option<usize>>> {
        self.mouse_focus.clone()
    }

    /// Returns the shared main-thread projection consumed by FrameXML globals.
    #[must_use]
    pub fn world_state(&self) -> crate::UiWorldState {
        self.world.clone()
    }

    /// Returns the shared authenticated account entitlement image.
    #[must_use]
    pub fn account_state(&self) -> crate::UiAccountState {
        self.account.clone()
    }

    /// Returns the shared physical modifier image consumed by FrameXML.
    #[must_use]
    pub fn modifier_key_state(&self) -> crate::UiModifierKeyState {
        self.modifiers.clone()
    }

    /// Returns the shared client-owned primary action-bar state.
    #[must_use]
    pub fn action_bar_state(&self) -> crate::UiActionBarState {
        self.action_bar.clone()
    }

    /// Returns the shared battlefield queue projection consumed by FrameXML.
    #[must_use]
    pub fn battlefield_state(&self) -> crate::UiBattlefieldQueueState {
        self.battlefield.clone()
    }

    /// Returns the shared client-owned persistent chat-window image.
    #[must_use]
    pub fn chat_window_state(&self) -> crate::UiChatWindowState {
        self.chat_windows.clone()
    }

    /// Returns the shared joined-channel display and roster state.
    #[must_use]
    pub fn channel_state(&self) -> crate::UiChannelState {
        self.channels.clone()
    }

    /// Returns the shared learned mount and critter collection.
    #[must_use]
    pub fn companion_state(&self) -> crate::UiCompanionState {
        self.companions.clone()
    }

    /// Returns the shared active loot-window eligibility projection.
    #[must_use]
    pub fn loot_state(&self) -> crate::UiLootState {
        self.loot.clone()
    }

    /// Returns the shared outgoing-mail compose quote.
    #[must_use]
    pub fn mail_compose_state(&self) -> crate::UiMailComposeState {
        self.mail.clone()
    }

    /// Returns the shared player-capability tracking projection.
    #[must_use]
    pub fn minimap_tracking_state(&self) -> crate::UiMinimapTrackingState {
        self.minimap_tracking.clone()
    }

    /// Returns the shared dungeon and raid finder lifecycle projection.
    #[must_use]
    pub fn group_finder_state(&self) -> crate::UiGroupFinderState {
        self.group_finder.clone()
    }

    /// Returns the shared party and raid roster count projection.
    #[must_use]
    pub fn group_roster_state(&self) -> crate::UiGroupRosterState {
        self.group_roster.clone()
    }

    /// Returns the shared active-character guild membership state.
    #[must_use]
    pub fn guild_state(&self) -> crate::UiGuildState {
        self.guild.clone()
    }

    /// Returns the shared ordered quest log and client selection state.
    #[must_use]
    pub fn quest_log_state(&self) -> crate::UiQuestLogState {
        self.quest_log.clone()
    }

    /// Returns the shared six-slot death-knight rune projection.
    #[must_use]
    pub fn rune_state(&self) -> crate::UiRuneState {
        self.runes.clone()
    }

    /// Returns the shared controlled-unit pet action bar.
    #[must_use]
    pub fn pet_action_state(&self) -> crate::UiPetActionState {
        self.pet_actions.clone()
    }

    /// Returns the shared player skill-line sequence and selection.
    #[must_use]
    pub fn skill_line_state(&self) -> crate::UiSkillLineState {
        self.skill_lines.clone()
    }

    /// Returns the shared social-directory result routing state.
    #[must_use]
    pub fn social_query_state(&self) -> crate::UiSocialQueryState {
        self.social_queries.clone()
    }

    /// Returns the shared ordered player spell-book projection.
    #[must_use]
    pub fn spell_book_state(&self) -> crate::UiSpellBookState {
        self.spell_book.clone()
    }

    /// Returns the shared controlled-unit shapeshift and possession state.
    #[must_use]
    pub fn stance_state(&self) -> crate::UiStanceState {
        self.stances.clone()
    }

    /// Returns the shared guild-tabard vendor session state.
    #[must_use]
    pub fn tabard_state(&self) -> crate::UiTabardState {
        self.tabard.clone()
    }

    /// Returns the shared voice-service availability projection.
    #[must_use]
    pub fn voice_chat_state(&self) -> crate::UiVoiceChatState {
        self.voice_chat.clone()
    }

    /// Returns the shared world-map native helper state.
    #[must_use]
    pub fn world_map_state(&self) -> crate::UiWorldMapState {
        self.world_map.clone()
    }

    /// Returns the ordered world-PvP and battleground status indicators.
    #[must_use]
    pub fn world_state_ui_state(&self) -> crate::UiWorldStateUiState {
        self.world_state_ui.clone()
    }
}

impl UiScriptRuntime {
    /// Creates an empty object registry and the stock identity method table.
    ///
    /// # Errors
    ///
    /// Returns [`UiScriptError::Execution`] when Lua cannot allocate or retain
    /// the registry and metatable objects.
    pub fn new(
        bundle: &UiBundle,
        plan: &UiScriptRuntimePlan<'_, '_>,
        environment: UiScriptEnvironment,
    ) -> Result<Self, UiScriptError> {
        let lua = bundle.lua();
        plan.templates
            .install(lua)
            .map_err(|error| execution_error("runtime templates", error))?;
        register_base_globals(lua, &environment, bundle.manifest().kind())
            .map_err(|error| execution_error("base globals", error))?;
        let objects = lua
            .create_table()
            .map_err(|error| execution_error("registry", error))?;
        lua.set_named_registry_value(OBJECT_REGISTRY, objects)
            .map_err(|error| execution_error("registry", error))?;
        lua.set_named_registry_value(
            ON_UPDATE_OBJECTS_REGISTRY,
            lua.create_table()
                .map_err(|error| execution_error("registry", error))?,
        )
        .and_then(|()| {
            lua.set_named_registry_value(ON_UPDATE_MEMBERS_REGISTRY, lua.create_table()?)
        })
        .and_then(|()| lua.set_named_registry_value(ON_UPDATE_SEEN_REGISTRY, lua.create_table()?))
        .and_then(|()| lua.set_named_registry_value(FOCUSED_EDIT_BOX_REGISTRY, 0_usize))
        .map_err(|error| execution_error("registry", error))?;
        lua.set_named_registry_value(LIVE_STATE_GENERATION_REGISTRY, 0_u64)
            .map_err(|error| execution_error("registry", error))?;
        let metatables = lua
            .create_table()
            .map_err(|error| execution_error("object metatables", error))?;
        let registered_objects = Rc::new(Cell::new(0));
        let dynamic_objects = Rc::new(Cell::new(0));
        let static_object_count = plan.regions.state_count();
        let dynamic_arena = DynamicArenaState {
            static_object_count,
            registered_objects: registered_objects.clone(),
            dynamic_objects,
        };
        let font_definitions = Rc::new(
            plan.fonts
                .definitions()
                .iter()
                .cloned()
                .map(|definition| (definition.name().to_owned(), definition))
                .collect::<HashMap<_, _>>(),
        );
        let text_measurement = buttons::TextMeasurement::new(
            environment.assets(),
            font_definitions.clone(),
            environment.logical_extent().1,
        )
        .map_err(|error| {
            execution_error("text measurement", mlua::Error::runtime(error.to_string()))
        })?;
        let object_metatables = OBJECT_KINDS
            .into_iter()
            .map(|kind| {
                let metatable = create_object_metatable(
                    lua,
                    bundle.manifest().kind(),
                    kind,
                    environment.ui_extent(),
                    environment.assets(),
                    environment.media_intent(),
                    Some(text_measurement.clone()),
                    dynamic_arena.clone(),
                )
                .map_err(|error| execution_error("object metatable", error))?;
                metatables
                    .raw_set(object_type_name(kind), metatable.clone())
                    .map_err(|error| execution_error("object metatable", error))?;
                lua.create_registry_value(metatable)
                    .map_err(|error| execution_error("object metatable", error))
            })
            .collect::<Result<Vec<_>, UiScriptError>>()?;
        lua.set_named_registry_value(METATABLE_REGISTRY, metatables)
            .map_err(|error| execution_error("object metatables", error))?;
        let font_metatable = create_font_metatable(lua)
            .and_then(|metatable| lua.create_registry_value(metatable))
            .map_err(|error| execution_error("font metatable", error))?;
        let animation_metatables = create_animation_metatables(lua)
            .map_err(|error| execution_error("animation metatables", error))?;
        let mut font_actions = vec![None; bundle.actions().len()];
        for definition in plan.fonts.definitions() {
            let action_index = definition.action_index();
            let Some(slot) = font_actions.get_mut(action_index) else {
                return Err(UiScriptError::Plan {
                    message: format!(
                        "font {} has out-of-range construction action {action_index}",
                        definition.name()
                    ),
                });
            };
            *slot = Some(definition.clone());
        }
        let mut region_dimensions = (0..plan.regions.state_count())
            .map(|index| {
                let state = plan
                    .regions
                    .state(index)
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("region state {index} is outside the arena"),
                    })?;
                Ok((f64::from(state.width()), f64::from(state.height())))
            })
            .collect::<Result<Vec<_>, UiScriptError>>()?;
        let simple_html = if let Some(assets) = environment.assets() {
            crate::UiSimpleHtmlPlan::resolve(
                plan.tree,
                plan.regions,
                plan.fonts,
                &mut assets.borrow_mut(),
                environment.logical_extent().1,
            )?
        } else if crate::UiSimpleHtmlPlan::requires_asset_store(plan.tree) {
            return Err(UiScriptError::Plan {
                message: "locale-backed SimpleHTML requires the mounted client asset store"
                    .to_owned(),
            });
        } else {
            crate::UiSimpleHtmlPlan::empty(plan.tree.nodes().len())
        };
        for (index, dimensions) in region_dimensions.iter_mut().enumerate() {
            if let Some(html) = simple_html.node(index) {
                dimensions.1 = dimensions.1.max(f64::from(html.content_height()));
            }
        }
        let region_shown = (0..plan.regions.state_count())
            .map(|index| {
                plan.regions
                    .state(index)
                    .map(|state| state.shown())
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("region state {index} is outside the arena"),
                    })
            })
            .collect::<Result<Vec<_>, UiScriptError>>()?;
        let region_alpha = (0..plan.regions.state_count())
            .map(|index| {
                plan.regions
                    .state(index)
                    .map(|state| f64::from(state.alpha()))
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("region state {index} is outside the arena"),
                    })
            })
            .collect::<Result<Vec<_>, UiScriptError>>()?;
        let region_scale = (0..plan.regions.state_count())
            .map(|index| {
                plan.regions
                    .state(index)
                    .map(|state| f64::from(state.scale()))
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("region state {index} is outside the arena"),
                    })
            })
            .collect::<Result<Vec<_>, UiScriptError>>()?;
        let region_anchors = (0..plan.regions.state_count())
            .map(|index| {
                let state = plan
                    .regions
                    .state(index)
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("region state {index} is outside the arena"),
                    })?;
                Ok(plan
                    .regions
                    .anchors_for(state)
                    .iter()
                    .map(|anchor| InitialAnchor {
                        point: anchor.point(),
                        target: match anchor.target() {
                            UiAnchorTarget::Screen => None,
                            UiAnchorTarget::Object(target) => Some(target + 1),
                        },
                        relative_point: anchor.relative_point(),
                        offset: (f64::from(anchor.offset().0), f64::from(anchor.offset().1)),
                    })
                    .collect::<Vec<_>>())
            })
            .collect::<Result<Vec<_>, UiScriptError>>()?;
        let frame_ids = (0..plan.regions.state_count())
            .map(|index| plan.frames.state(index).map(|state| state.id()))
            .collect();
        let frame_levels = (0..plan.regions.state_count())
            .map(|index| plan.frames.state(index).map(|state| state.level()))
            .collect();
        let frame_strata = (0..plan.regions.state_count())
            .map(|index| {
                plan.frames
                    .state(index)
                    .map(|state| frame_strata_name(state.strata()))
            })
            .collect();
        let frame_keyboard_enabled = (0..plan.regions.state_count())
            .map(|index| {
                plan.frames
                    .state(index)
                    .map(|state| state.keyboard_enabled())
            })
            .collect();
        let frame_mouse_enabled = (0..plan.regions.state_count())
            .map(|index| plan.frames.state(index).map(|state| state.mouse_enabled()))
            .collect();
        let frame_clamped_to_screen = (0..plan.regions.state_count())
            .map(|index| {
                plan.frames
                    .state(index)
                    .map(|state| state.clamped_to_screen())
            })
            .collect();
        let frame_movable = (0..plan.regions.state_count())
            .map(|index| plan.frames.state(index).map(|state| state.movable()))
            .collect();
        let frame_resizable = (0..plan.regions.state_count())
            .map(|index| plan.frames.state(index).map(|state| state.resizable()))
            .collect();
        let frame_top_level = (0..plan.regions.state_count())
            .map(|index| plan.frames.state(index).map(|state| state.top_level()))
            .collect();
        let frame_dont_save_position = (0..plan.regions.state_count())
            .map(|index| {
                plan.frames
                    .state(index)
                    .map(|state| state.position_persistence_disabled())
            })
            .collect();
        let font_strings = tree_font_strings(plan.tree, plan.fonts);
        let buttons = tree_buttons(plan.tree);
        let textures = tree_textures(plan.tree, plan.texture_states)?;
        register_create_frame(lua, dynamic_arena)
            .map_err(|error| execution_error("CreateFrame", error))?;
        Ok(Self {
            next_action: 0,
            object_metatables,
            font_metatable,
            animation_metatables,
            animations: plan.animations.clone(),
            font_actions,
            region_dimensions,
            region_shown,
            region_alpha,
            region_scale,
            region_anchors,
            font_strings,
            buttons,
            textures,
            simple_html,
            frame_ids,
            frame_levels,
            frame_strata,
            frame_keyboard_enabled,
            frame_mouse_enabled,
            frame_clamped_to_screen,
            frame_movable,
            frame_resizable,
            frame_top_level,
            frame_dont_save_position,
            text_measurement,
            registered_objects,
            executed_chunks: 0,
            executed_load_handlers: 0,
        })
    }

    /// Executes one expanded bundle action and advances the stock load cursor.
    ///
    /// Returns `false` only when every action has already completed.
    ///
    /// # Errors
    ///
    /// Returns [`UiScriptError`] for mismatched plans, object registration
    /// failures, missing named handlers, or a Lua runtime error. The cursor
    /// remains on the failing action for deterministic diagnostics.
    pub fn execute_next(
        &mut self,
        bundle: &UiBundle,
        tree: &UiObjectTree<'_>,
        scripts: &UiScriptPlan,
    ) -> Result<bool, UiScriptError> {
        let Some(action) = bundle.actions().get(self.next_action) else {
            return Ok(false);
        };
        match action {
            UiLoadAction::XmlElement { .. } => {
                if let Some(definition) = self
                    .font_actions
                    .get(self.next_action)
                    .and_then(Option::as_ref)
                {
                    register_font(bundle.lua(), &self.font_metatable, definition)?;
                }
                if let Some(batch) = tree.batch_for_action(self.next_action) {
                    self.execute_batch(bundle.lua(), tree, scripts, batch)?;
                }
            }
            UiLoadAction::LuaResource { resource_index } => {
                let resource =
                    bundle
                        .resource(*resource_index)
                        .ok_or_else(|| UiScriptError::Plan {
                            message: format!(
                                "Lua action {} has an out-of-range resource index {resource_index}",
                                self.next_action
                            ),
                        })?;
                let UiResourceContent::Lua(source) = resource.content() else {
                    return Err(UiScriptError::Plan {
                        message: format!(
                            "Lua action {} refers to a non-Lua resource",
                            self.next_action
                        ),
                    });
                };
                bundle
                    .lua()
                    .load(source.as_str())
                    .set_name(resource.path().as_str())
                    .exec()
                    .map_err(|error| execution_error(resource.path().as_str(), error))?;
                self.executed_chunks += 1;
            }
            UiLoadAction::InlineLua { path, source } => {
                let label = format!("{}:<inline>", path.as_str());
                bundle
                    .lua()
                    .load(source)
                    .set_name(&label)
                    .exec()
                    .map_err(|error| execution_error(&label, error))?;
                self.executed_chunks += 1;
            }
        }
        self.next_action += 1;
        Ok(true)
    }

    /// Executes the remaining expanded actions in order.
    ///
    /// # Errors
    ///
    /// Returns the first error from [`Self::execute_next`].
    pub fn execute_all(
        &mut self,
        bundle: &UiBundle,
        tree: &UiObjectTree<'_>,
        scripts: &UiScriptPlan,
    ) -> Result<(), UiScriptError> {
        while self.execute_next(bundle, tree, scripts)? {}
        Ok(())
    }

    /// Returns the next unexecuted expanded action index.
    #[must_use]
    pub const fn next_action(&self) -> usize {
        self.next_action
    }

    /// Returns the number of object tables exposed so far.
    #[must_use]
    pub fn registered_object_count(&self) -> usize {
        self.registered_objects.get()
    }

    /// Returns locale-loaded and measured static `SimpleHTML` state.
    #[must_use]
    pub const fn simple_html(&self) -> &crate::UiSimpleHtmlPlan {
        &self.simple_html
    }

    /// Applies Lua-authored `SimpleHTML:SetText` documents to native layout.
    ///
    /// Document parsing and font measurement happen once per changed string.
    /// Updated child heights and scroll ranges are published before the next
    /// live-state snapshot, matching the observable stock frame-tick result.
    pub(crate) fn refresh_simple_html_layout(
        &mut self,
        bundle: &UiBundle,
        live: &super::runtime_state::UiRuntimeObjectPlan,
        geometry: &UiRegionGeometryPlan,
        fonts: &FontCatalog,
        assets: &mut AssetStore,
        logical_height: u32,
    ) -> Result<bool, UiScriptError> {
        let mut changes = Vec::new();
        for (object_index, object) in live.objects().iter().enumerate() {
            if object.kind != UiObjectKind::SimpleHtml {
                continue;
            }
            let width = geometry
                .region(object_index)
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("SimpleHTML object {object_index} has no resolved geometry"),
                })?
                .logical_bounds()
                .width();
            if let Some((height, clip_object)) = self.simple_html.refresh_text(
                object_index,
                object.simple_html_text.as_deref(),
                width,
                fonts,
                assets,
                logical_height,
            )? {
                changes.push((object_index, f64::from(height), clip_object));
            }
        }
        if changes.is_empty() {
            return Ok(false);
        }

        let lua = bundle.lua();
        let objects: Table = lua
            .named_registry_value(OBJECT_REGISTRY)
            .map_err(|error| execution_error("refresh SimpleHTML layout", error))?;
        for (object_index, height, clip_object) in changes {
            let object: Table = objects
                .raw_get(object_index + 1)
                .map_err(|error| execution_error("refresh SimpleHTML layout", error))?;
            object
                .raw_set(height_key(), height)
                .map_err(|error| execution_error("refresh SimpleHTML layout", error))?;
            let Some(clip_object) = clip_object else {
                continue;
            };
            let scroll_frame: Table = objects
                .raw_get(clip_object + 1)
                .map_err(|error| execution_error("refresh SimpleHTML scroll range", error))?;
            let previous = (
                scroll_frame
                    .raw_get::<f64>(horizontal_scroll_range_key())
                    .map_err(|error| execution_error("refresh SimpleHTML scroll range", error))?,
                scroll_frame
                    .raw_get::<f64>(vertical_scroll_range_key())
                    .map_err(|error| execution_error("refresh SimpleHTML scroll range", error))?,
            );
            update_scroll_child_rect(&scroll_frame)
                .map_err(|error| execution_error("refresh SimpleHTML scroll range", error))?;
            let current = (
                scroll_frame
                    .raw_get::<f64>(horizontal_scroll_range_key())
                    .map_err(|error| execution_error("refresh SimpleHTML scroll range", error))?,
                scroll_frame
                    .raw_get::<f64>(vertical_scroll_range_key())
                    .map_err(|error| execution_error("refresh SimpleHTML scroll range", error))?,
            );
            if current != previous
                && let Some(function) =
                    object_script_function(lua, &scroll_frame, UiScriptHandler::ScrollRangeChanged)
                        .map_err(|error| {
                            execution_error("SimpleHTML scroll-range callback", error)
                        })?
            {
                call_two_number_object_handler(lua, &function, scroll_frame, current.0, current.1)
                    .map_err(|error| execution_error("SimpleHTML scroll-range callback", error))?;
            }
        }
        mark_live_state_changed(lua)
            .map_err(|error| execution_error("refresh SimpleHTML layout", error))?;
        Ok(true)
    }

    /// Publishes native layout-pass dimensions used by region query methods.
    ///
    /// Opposing anchors can resolve an authored zero width or height into a
    /// concrete live size. Stock `GetWidth`/`GetHeight` expose that resolved
    /// unscaled value to Lua, including the scrollbar height used for wheel
    /// steps.
    pub(crate) fn publish_resolved_geometry(
        &mut self,
        bundle: &UiBundle,
        geometry: &crate::UiRegionGeometryPlan,
    ) -> Result<(), UiScriptError> {
        if geometry.region_count() != self.registered_object_count() {
            return Err(UiScriptError::Plan {
                message: format!(
                    "resolved geometry has {} objects; runtime registered {}",
                    geometry.region_count(),
                    self.registered_object_count()
                ),
            });
        }
        let lua = bundle.lua();
        let objects: Table = lua
            .named_registry_value(OBJECT_REGISTRY)
            .map_err(|error| execution_error("publish resolved geometry", error))?;
        for object_index in 0..geometry.region_count() {
            let region = geometry
                .region(object_index)
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("resolved geometry object {object_index} is unavailable"),
                })?;
            let object: Table = objects
                .raw_get(object_index + 1)
                .map_err(|error| execution_error("publish resolved geometry", error))?;
            let bounds = region.logical_bounds();
            object
                .raw_set(width_key(), bounds.width())
                .and_then(|()| object.raw_set(height_key(), bounds.height()))
                .map_err(|error| execution_error("publish resolved geometry", error))?;
        }
        Ok(())
    }

    /// Copies the authoritative post-script region state out of Lua.
    ///
    /// # Errors
    ///
    /// Returns [`UiScriptError`] when a live object table is incomplete or
    /// contains an invalid arena reference or non-finite layout value.
    pub(crate) fn snapshot_objects(
        &self,
        bundle: &UiBundle,
    ) -> Result<super::runtime_state::UiRuntimeObjectPlan, UiScriptError> {
        self.text_measurement
            .synchronize_auto_font_strings(bundle.lua(), self.registered_object_count())
            .map_err(|error| execution_error("automatic FontString extent", error))?;
        super::runtime_state::snapshot_runtime_objects(bundle.lua(), self.registered_object_count())
    }

    /// Returns the number of external and inline Lua chunks executed so far.
    #[must_use]
    pub const fn executed_chunk_count(&self) -> usize {
        self.executed_chunks
    }

    /// Returns the number of `OnLoad` callbacks executed so far.
    #[must_use]
    pub const fn executed_load_handler_count(&self) -> usize {
        self.executed_load_handlers
    }

    /// Delivers one canonical Glue event to subscribers in creation order.
    ///
    /// # Errors
    ///
    /// Returns [`UiScriptError`] when Lua registry access or a subscribed
    /// `OnEvent` callback fails. Dispatch stops at the first failed handler.
    pub(crate) fn dispatch_glue_event(
        &mut self,
        bundle: &UiBundle,
        event: &'static str,
        payload: &UiEventPayload,
    ) -> Result<usize, UiScriptError> {
        let lua = bundle.lua();
        let globals = lua.globals();
        let previous_event = globals
            .raw_get::<Value>("event")
            .map_err(|error| execution_error(event, error))?;
        let mut previous_arguments = Vec::with_capacity(crate::event::MAX_EVENT_ARGUMENTS);
        for index in 1..=crate::event::MAX_EVENT_ARGUMENTS {
            previous_arguments.push(
                globals
                    .raw_get::<Value>(format!("arg{index}"))
                    .map_err(|error| execution_error(event, error))?,
            );
        }
        let arguments = payload
            .arguments()
            .iter()
            .map(|argument| event_argument(lua, argument))
            .collect::<mlua::Result<Vec<_>>>()
            .map_err(|error| execution_error(event, error))?;
        globals
            .raw_set("event", event)
            .map_err(|error| execution_error(event, error))?;
        for index in 1..=crate::event::MAX_EVENT_ARGUMENTS {
            globals
                .raw_set(
                    format!("arg{index}"),
                    arguments.get(index - 1).cloned().unwrap_or(Value::Nil),
                )
                .map_err(|error| execution_error(event, error))?;
        }

        let dispatch = dispatch_subscribers(lua, self.registered_object_count(), event, &arguments);
        let restore = restore_event_globals(lua, previous_event, previous_arguments);
        match (dispatch, restore) {
            (Ok(count), Ok(())) => Ok(count),
            (Err(error), _) => Err(execution_error(event, error)),
            (Ok(_), Err(error)) => Err(execution_error(event, error)),
        }
    }

    /// Delivers one rendered-frame elapsed interval to visible `OnUpdate`
    /// handlers in construction order.
    pub(crate) fn dispatch_updates(
        &mut self,
        bundle: &UiBundle,
        elapsed_seconds: f64,
    ) -> Result<(usize, bool), UiScriptError> {
        if !elapsed_seconds.is_finite() || elapsed_seconds < 0.0 {
            return Err(UiScriptError::Plan {
                message: format!("invalid Glue update interval {elapsed_seconds}"),
            });
        }
        let lua = bundle.lua();
        let generation =
            live_state_generation(lua).map_err(|error| execution_error("Glue OnUpdate", error))?;
        let objects: Table = lua
            .named_registry_value(OBJECT_REGISTRY)
            .map_err(|error| execution_error("Glue OnUpdate", error))?;
        let animation_changed = advance_animations(lua, elapsed_seconds)
            .map_err(|error| execution_error("FrameXML animation update", error))?;
        advance_edit_box_caret(lua, &objects, elapsed_seconds)
            .map_err(|error| execution_error("Glue EditBox caret", error))?;
        let update_objects: Table = lua
            .named_registry_value(ON_UPDATE_OBJECTS_REGISTRY)
            .map_err(|error| execution_error("Glue OnUpdate", error))?;
        let update_members: Table = lua
            .named_registry_value(ON_UPDATE_MEMBERS_REGISTRY)
            .map_err(|error| execution_error("Glue OnUpdate", error))?;
        let mut dispatched = 0;
        for slot in 1..=update_objects.raw_len() {
            let index = update_objects
                .raw_get::<usize>(slot)
                .map_err(|error| execution_error("Glue OnUpdate", error))?;
            if !update_members
                .raw_get::<bool>(index)
                .map_err(|error| execution_error("Glue OnUpdate", error))?
            {
                continue;
            }
            let object = objects
                .raw_get::<Table>(index)
                .map_err(|error| execution_error("Glue OnUpdate", error))?;
            if !is_script_frame_table(&object)
                .and_then(|is_frame| {
                    if is_frame {
                        object_is_visible(lua, object.clone())
                    } else {
                        Ok(false)
                    }
                })
                .map_err(|error| execution_error("Glue OnUpdate", error))?
            {
                continue;
            }
            let Some(function) = object_script_function(lua, &object, UiScriptHandler::Update)
                .map_err(|error| execution_error("Glue OnUpdate", error))?
            else {
                continue;
            };
            call_number_object_handler(lua, &function, object, elapsed_seconds)
                .map_err(|error| execution_error("Glue OnUpdate", error))?;
            dispatched += 1;
        }
        let changed = animation_changed
            || live_state_generation(lua)
                .map_err(|error| execution_error("Glue OnUpdate", error))?
                != generation;
        Ok((dispatched, changed))
    }

    /// Delivers native movie completion to one live `MovieFrame`.
    pub(crate) fn dispatch_movie_finished(
        &mut self,
        bundle: &UiBundle,
        object_index: usize,
    ) -> Result<(), UiScriptError> {
        let label = format!("MovieFrame object {object_index}:OnMovieFinished");
        if object_index >= self.registered_object_count() {
            return Err(UiScriptError::Plan {
                message: format!(
                    "movie completion object {object_index} is outside the live object arena"
                ),
            });
        }
        let lua = bundle.lua();
        let objects: Table = lua
            .named_registry_value(OBJECT_REGISTRY)
            .map_err(|error| execution_error(&label, error))?;
        let object: Table = objects
            .raw_get(object_index + 1)
            .map_err(|error| execution_error(&label, error))?;
        let Some(function) = object_script_function(lua, &object, UiScriptHandler::MovieFinished)
            .map_err(|error| execution_error(&label, error))?
        else {
            return Ok(());
        };
        call_object_handler(lua, &function, object).map_err(|error| execution_error(&label, error))
    }

    /// Delivers one stock key name to a live `MovieFrame` `OnKeyUp` handler.
    pub(crate) fn dispatch_movie_key_up(
        &mut self,
        bundle: &UiBundle,
        object_index: usize,
        key: &str,
    ) -> Result<(), UiScriptError> {
        let label = format!("MovieFrame object {object_index}:OnKeyUp");
        if object_index >= self.registered_object_count() {
            return Err(UiScriptError::Plan {
                message: format!(
                    "movie key object {object_index} is outside the live object arena"
                ),
            });
        }
        let lua = bundle.lua();
        let objects: Table = lua
            .named_registry_value(OBJECT_REGISTRY)
            .map_err(|error| execution_error(&label, error))?;
        let object: Table = objects
            .raw_get(object_index + 1)
            .map_err(|error| execution_error(&label, error))?;
        let Some(function) = object_script_function(lua, &object, UiScriptHandler::KeyUp)
            .map_err(|error| execution_error(&label, error))?
        else {
            return Ok(());
        };
        call_string_object_handler(lua, &function, object, key)
            .map_err(|error| execution_error(&label, error))
    }

    /// Delivers one captured pointer transition and optional registered click.
    pub(crate) fn dispatch_button_pointer(
        &mut self,
        bundle: &UiBundle,
        object_index: usize,
        mouse_button: &str,
        pressed: bool,
        activate_click: bool,
        activate_double_click: bool,
    ) -> Result<(), UiScriptError> {
        let phase = if pressed { "down" } else { "up" };
        let label = format!("Button object {object_index}:pointer-{phase}");
        if object_index >= self.registered_object_count() {
            return Err(UiScriptError::Plan {
                message: format!(
                    "button pointer object {object_index} is outside the live object arena"
                ),
            });
        }
        let lua = bundle.lua();
        let objects: Table = lua
            .named_registry_value(OBJECT_REGISTRY)
            .map_err(|error| execution_error(&label, error))?;
        let object: Table = objects
            .raw_get(object_index + 1)
            .map_err(|error| execution_error(&label, error))?;
        let enabled = object
            .raw_get::<bool>(enabled_key())
            .map_err(|error| execution_error(&label, error))?;
        let state_locked = object
            .raw_get::<bool>(button_state_locked_key())
            .map_err(|error| execution_error(&label, error))?;
        if !state_locked {
            object
                .raw_set(button_pressed_key(), pressed && enabled)
                .map_err(|error| execution_error(&label, error))?;
        }

        let mouse_handler = if pressed {
            UiScriptHandler::MouseDown
        } else {
            UiScriptHandler::MouseUp
        };
        if let Some(function) = object_script_function(lua, &object, mouse_handler)
            .map_err(|error| execution_error(&label, error))?
        {
            call_legacy_string_handler(lua, &function, object.clone(), mouse_button)
                .map_err(|error| execution_error(&label, error))?;
        }
        if enabled && activate_click {
            if object
                .raw_get::<String>(type_key())
                .map_err(|error| execution_error(&label, error))?
                == "CheckButton"
            {
                apply_check_button_click(lua, &object)
                    .map_err(|error| execution_error(&label, error))?;
            }
            for handler in [
                UiScriptHandler::PreClick,
                UiScriptHandler::Click,
                UiScriptHandler::PostClick,
            ] {
                if let Some(function) = object_script_function(lua, &object, handler)
                    .map_err(|error| execution_error(&label, error))?
                {
                    buttons::call_click_handler(
                        lua,
                        &function,
                        object.clone(),
                        mouse_button,
                        pressed,
                    )
                    .map_err(|error| execution_error(&label, error))?;
                }
            }
            if activate_double_click
                && let Some(function) =
                    object_script_function(lua, &object, UiScriptHandler::DoubleClick)
                        .map_err(|error| execution_error(&label, error))?
            {
                call_legacy_string_handler(lua, &function, object, mouse_button)
                    .map_err(|error| execution_error(&label, error))?;
            }
        }
        Ok(())
    }

    /// Applies one native pointer-boundary transition before running the
    /// frame's authored `OnEnter` or `OnLeave` handler.
    pub(crate) fn dispatch_pointer_hover(
        &mut self,
        bundle: &UiBundle,
        object_index: usize,
        entered: bool,
    ) -> Result<(), UiScriptError> {
        let phase = if entered { "enter" } else { "leave" };
        let label = format!("UI object {object_index}:pointer-{phase}");
        let lua = bundle.lua();
        let object = self.runtime_object(lua, object_index, &label)?;
        let kind = object
            .raw_get::<String>(type_key())
            .map_err(|error| execution_error(&label, error))?;
        if matches!(kind.as_str(), "Button" | "CheckButton") {
            object
                .raw_set(hovered_key(), entered)
                .map_err(|error| execution_error(&label, error))?;
        }
        let handler = if entered {
            UiScriptHandler::Enter
        } else {
            UiScriptHandler::Leave
        };
        let Some(function) = object_script_function(lua, &object, handler)
            .map_err(|error| execution_error(&label, error))?
        else {
            return Ok(());
        };
        call_object_handler(lua, &function, object).map_err(|error| execution_error(&label, error))
    }

    /// Delivers one normalized wheel delta to a live ScrollFrame handler.
    pub(crate) fn dispatch_mouse_wheel(
        &mut self,
        bundle: &UiBundle,
        object_index: usize,
        delta: f64,
    ) -> Result<(), UiScriptError> {
        let label = format!("ScrollFrame object {object_index}:OnMouseWheel");
        if object_index >= self.registered_object_count() || !delta.is_finite() {
            return Err(UiScriptError::Plan {
                message: format!("invalid mouse-wheel delivery for object {object_index}"),
            });
        }
        let lua = bundle.lua();
        let objects: Table = lua
            .named_registry_value(OBJECT_REGISTRY)
            .map_err(|error| execution_error(&label, error))?;
        let object: Table = objects
            .raw_get(object_index + 1)
            .map_err(|error| execution_error(&label, error))?;
        let Some(function) = object_script_function(lua, &object, UiScriptHandler::MouseWheel)
            .map_err(|error| execution_error(&label, error))?
        else {
            return Ok(());
        };
        call_number_object_handler(lua, &function, object, delta)
            .map_err(|error| execution_error(&label, error))
    }

    /// Gives native focus to one live EditBox and dispatches focus callbacks.
    pub(crate) fn focus_edit_box(
        &mut self,
        bundle: &UiBundle,
        object_index: usize,
    ) -> Result<(), UiScriptError> {
        let label = format!("EditBox object {object_index}:focus");
        let object = self.runtime_object(bundle.lua(), object_index, &label)?;
        if object
            .raw_get::<String>(type_key())
            .map_err(|error| execution_error(&label, error))?
            != "EditBox"
        {
            return Err(UiScriptError::Plan {
                message: format!("focus target {object_index} is not an EditBox"),
            });
        }
        set_edit_box_focus(bundle.lua(), &object, true)
            .map_err(|error| execution_error(&label, error))
    }

    /// Applies one pointer-derived cursor or selection endpoint to an EditBox.
    ///
    /// `CSimpleEditBox` stores byte offsets, so both inputs are clamped to UTF-8
    /// boundaries before the ordered anchor/cursor pair enters live state.
    pub(crate) fn set_edit_box_pointer_selection(
        &mut self,
        bundle: &UiBundle,
        object_index: usize,
        anchor: usize,
        cursor: usize,
    ) -> Result<(), UiScriptError> {
        let label = format!("EditBox object {object_index}:pointer-selection");
        let object = self.runtime_object(bundle.lua(), object_index, &label)?;
        if object
            .raw_get::<String>(type_key())
            .map_err(|error| execution_error(&label, error))?
            != "EditBox"
        {
            return Err(UiScriptError::Plan {
                message: format!("pointer-selection target {object_index} is not an EditBox"),
            });
        }
        let text = object
            .raw_get::<String>(text_key())
            .map_err(|error| execution_error(&label, error))?;
        let anchor = clamp_utf8_boundary(&text, anchor) as u32;
        let cursor = clamp_utf8_boundary(&text, cursor) as u32;
        object
            .raw_set(edit_cursor_key(), cursor)
            .and_then(|()| object.raw_set(edit_selection_start_key(), anchor))
            .and_then(|()| object.raw_set(edit_selection_end_key(), cursor))
            .and_then(|()| reset_edit_box_caret(&object))
            .map_err(|error| execution_error(&label, error))
    }

    /// Delivers committed UTF-8 text to the currently focused EditBox.
    pub(crate) fn dispatch_edit_text(
        &mut self,
        bundle: &UiBundle,
        object_index: usize,
        text: &str,
    ) -> Result<(), UiScriptError> {
        let label = format!("EditBox object {object_index}:text-input");
        let lua = bundle.lua();
        let object = self.runtime_object(lua, object_index, &label)?;
        if object
            .raw_get::<String>(type_key())
            .map_err(|error| execution_error(&label, error))?
            != "EditBox"
            || !object
                .raw_get::<bool>(edit_focused_key())
                .map_err(|error| execution_error(&label, error))?
        {
            return Err(UiScriptError::Plan {
                message: format!("text-input target {object_index} is not the focused EditBox"),
            });
        }
        let numeric = object
            .raw_get::<bool>(edit_numeric_key())
            .map_err(|error| execution_error(&label, error))?;
        for character in text.chars() {
            if character.is_control() || numeric && !character.is_ascii_digit() {
                continue;
            }
            let character = character.to_string();
            insert_edit_box_text(lua, &object, &character, true)
                .map_err(|error| execution_error(&label, error))?;
            if let Some(function) = object_script_function(lua, &object, UiScriptHandler::Char)
                .map_err(|error| execution_error(&label, error))?
            {
                call_string_object_handler(lua, &function, object.clone(), &character)
                    .map_err(|error| execution_error(&label, error))?;
            }
        }
        Ok(())
    }

    /// Delivers an in-progress input-method composition without committing it.
    pub(crate) fn dispatch_edit_composition(
        &mut self,
        bundle: &UiBundle,
        object_index: usize,
        text: &str,
    ) -> Result<(), UiScriptError> {
        let label = format!("EditBox object {object_index}:composition");
        let lua = bundle.lua();
        let object = self.runtime_object(lua, object_index, &label)?;
        let Some(function) = object_script_function(lua, &object, UiScriptHandler::CharComposition)
            .map_err(|error| execution_error(&label, error))?
        else {
            return Ok(());
        };
        call_string_object_handler(lua, &function, object, text)
            .map_err(|error| execution_error(&label, error))
    }

    /// Delivers one key transition to a focused EditBox or keyboard frame.
    pub(crate) fn dispatch_keyboard_key(
        &mut self,
        bundle: &UiBundle,
        object_index: usize,
        key: &str,
        pressed: bool,
        modifiers: UiKeyboardModifiers,
    ) -> Result<(), UiScriptError> {
        let phase = if pressed { "down" } else { "up" };
        let label = format!("UI object {object_index}:key-{phase}");
        let lua = bundle.lua();
        let object = self.runtime_object(lua, object_index, &label)?;
        let handler = if pressed {
            UiScriptHandler::KeyDown
        } else {
            UiScriptHandler::KeyUp
        };
        if let Some(function) = object_script_function(lua, &object, handler)
            .map_err(|error| execution_error(&label, error))?
        {
            call_string_object_handler(lua, &function, object.clone(), key)
                .map_err(|error| execution_error(&label, error))?;
        }
        let is_edit_box = object
            .raw_get::<String>(type_key())
            .map_err(|error| execution_error(&label, error))?
            == "EditBox";
        if pressed && is_edit_box {
            dispatch_edit_box_key(lua, &object, key, modifiers)
                .map_err(|error| execution_error(&label, error))?;
        }
        Ok(())
    }

    /// Delivers pointer handlers for non-button mouse-enabled frames.
    pub(crate) fn dispatch_frame_pointer(
        &mut self,
        bundle: &UiBundle,
        object_index: usize,
        mouse_button: &str,
        pressed: bool,
    ) -> Result<(), UiScriptError> {
        let phase = if pressed { "down" } else { "up" };
        let label = format!("UI object {object_index}:pointer-{phase}");
        let lua = bundle.lua();
        let object = self.runtime_object(lua, object_index, &label)?;
        let handler = if pressed {
            UiScriptHandler::MouseDown
        } else {
            UiScriptHandler::MouseUp
        };
        let Some(function) = object_script_function(lua, &object, handler)
            .map_err(|error| execution_error(&label, error))?
        else {
            return Ok(());
        };
        call_legacy_string_handler(lua, &function, object, mouse_button)
            .map_err(|error| execution_error(&label, error))
    }

    /// Applies a native pointer-derived value to one live Slider.
    pub(crate) fn dispatch_slider_value(
        &mut self,
        bundle: &UiBundle,
        object_index: usize,
        value: f64,
    ) -> Result<(), UiScriptError> {
        let label = format!("Slider object {object_index}:pointer-value");
        if !value.is_finite() {
            return Err(UiScriptError::Plan {
                message: format!("invalid slider value for object {object_index}"),
            });
        }
        let lua = bundle.lua();
        let object = self.runtime_object(lua, object_index, &label)?;
        if object
            .raw_get::<String>(type_key())
            .map_err(|error| execution_error(&label, error))?
            != "Slider"
        {
            return Err(UiScriptError::Plan {
                message: format!("pointer-value target {object_index} is not a Slider"),
            });
        }
        if !object
            .raw_get::<bool>(enabled_key())
            .map_err(|error| execution_error(&label, error))?
        {
            return Ok(());
        }
        set_range_value(lua, object, value).map_err(|error| execution_error(&label, error))
    }

    /// Copies the compact native state needed to present a captured Slider.
    ///
    /// Drag callbacks commonly update a ScrollFrame on every mouse event. A
    /// frame-boundary refresh reads only the captured range and the finite set
    /// of ScrollFrames; the release event still performs the complete arena
    /// snapshot and therefore reconciles any unusual callback side effects.
    pub(crate) fn refresh_slider_scroll_snapshot(
        &self,
        bundle: &UiBundle,
        live: &mut super::runtime_state::UiRuntimeObjectPlan,
        slider_index: usize,
    ) -> Result<(), UiScriptError> {
        let lua = bundle.lua();
        let registry: Table = lua
            .named_registry_value(OBJECT_REGISTRY)
            .map_err(|error| execution_error("Slider scroll refresh", error))?;
        let scroll_indices = live
            .objects()
            .iter()
            .enumerate()
            .filter_map(|(index, object)| {
                (object.kind == UiObjectKind::ScrollFrame).then_some(index)
            })
            .collect::<Vec<_>>();

        let slider: Table = registry
            .raw_get(slider_index + 1)
            .map_err(|error| execution_error("Slider scroll refresh", error))?;
        let slider_state = snapshot_slider(slider_index + 1, &slider)?;
        live.replace_slider(slider_index, slider_state);

        for object_index in scroll_indices {
            let lua_index = object_index + 1;
            let object: Table = registry
                .raw_get(lua_index)
                .map_err(|error| execution_error("Slider scroll refresh", error))?;
            let offset = (
                finite_region_number(
                    &object,
                    horizontal_scroll_key(),
                    lua_index,
                    "horizontal scroll",
                )?,
                finite_region_number(&object, vertical_scroll_key(), lua_index, "vertical scroll")?,
            );
            let range = (
                finite_region_number(
                    &object,
                    horizontal_scroll_range_key(),
                    lua_index,
                    "horizontal scroll range",
                )?,
                finite_region_number(
                    &object,
                    vertical_scroll_range_key(),
                    lua_index,
                    "vertical scroll range",
                )?,
            );
            live.replace_scroll_state(object_index, offset, range);
        }
        Ok(())
    }

    fn runtime_object(
        &self,
        lua: &Lua,
        object_index: usize,
        label: &str,
    ) -> Result<Table, UiScriptError> {
        if object_index >= self.registered_object_count() {
            return Err(UiScriptError::Plan {
                message: format!("UI object {object_index} is outside the live object arena"),
            });
        }
        let objects: Table = lua
            .named_registry_value(OBJECT_REGISTRY)
            .map_err(|error| execution_error(label, error))?;
        objects
            .raw_get(object_index + 1)
            .map_err(|error| execution_error(label, error))
    }

    fn execute_batch(
        &mut self,
        lua: &Lua,
        tree: &UiObjectTree<'_>,
        scripts: &UiScriptPlan,
        batch: UiObjectBatch,
    ) -> Result<(), UiScriptError> {
        let mut visited = 0;
        self.execute_object(lua, tree, scripts, batch, batch.root(), &mut visited)?;
        if visited != batch.node_count() {
            return Err(UiScriptError::Plan {
                message: format!(
                    "action {} reached {visited} of {} construction nodes",
                    batch.action_index(),
                    batch.node_count()
                ),
            });
        }
        Ok(())
    }

    fn execute_object(
        &mut self,
        lua: &Lua,
        tree: &UiObjectTree<'_>,
        scripts: &UiScriptPlan,
        batch: UiObjectBatch,
        node_index: usize,
        visited: &mut usize,
    ) -> Result<(), UiScriptError> {
        if !batch.node_range().contains(&node_index) {
            return Err(UiScriptError::Plan {
                message: format!(
                    "object {node_index} escaped action {} construction range",
                    batch.action_index()
                ),
            });
        }
        self.register_object(lua, tree, scripts, node_index)?;
        *visited += 1;
        let children = tree
            .nodes()
            .get(node_index)
            .ok_or_else(|| UiScriptError::Plan {
                message: format!("construction node {node_index} is outside the object arena"),
            })?
            .construction_children();
        for child in children {
            self.execute_object(lua, tree, scripts, batch, *child, visited)?;
        }
        self.execute_load_handler(lua, tree, node_index)
    }

    fn register_object(
        &mut self,
        lua: &Lua,
        tree: &UiObjectTree<'_>,
        scripts: &UiScriptPlan,
        node_index: usize,
    ) -> Result<(), UiScriptError> {
        let object = tree
            .nodes()
            .get(node_index)
            .ok_or_else(|| UiScriptError::Plan {
                message: format!("object index {node_index} is outside the arena"),
            })?;
        let dimensions =
            self.region_dimensions
                .get(node_index)
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("object {node_index} has no resolved region state"),
                })?;
        let shown =
            self.region_shown
                .get(node_index)
                .copied()
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("object {node_index} has no resolved visibility state"),
                })?;
        let frame_id = if is_frame_object(object.kind()) {
            Some(
                self.frame_ids
                    .get(node_index)
                    .copied()
                    .flatten()
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("frame object {node_index} has no resolved frame state"),
                    })?,
            )
        } else {
            None
        };
        let frame_level = if is_frame_object(object.kind()) {
            Some(
                self.frame_levels
                    .get(node_index)
                    .copied()
                    .flatten()
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("frame object {node_index} has no resolved frame level"),
                    })?,
            )
        } else {
            None
        };
        let keyboard_enabled = if is_frame_object(object.kind()) {
            Some(
                self.frame_keyboard_enabled
                    .get(node_index)
                    .copied()
                    .flatten()
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!(
                            "frame object {node_index} has no resolved keyboard-input state"
                        ),
                    })?,
            )
        } else {
            None
        };
        let mouse_enabled = if is_frame_object(object.kind()) {
            Some(
                self.frame_mouse_enabled
                    .get(node_index)
                    .copied()
                    .flatten()
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!(
                            "frame object {node_index} has no resolved mouse-input state"
                        ),
                    })?,
            )
        } else {
            None
        };
        let clamped_to_screen = if is_frame_object(object.kind()) {
            Some(
                self.frame_clamped_to_screen
                    .get(node_index)
                    .copied()
                    .flatten()
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!(
                            "frame object {node_index} has no resolved screen-clamp state"
                        ),
                    })?,
            )
        } else {
            None
        };
        let movable =
            resolved_frame_flag(&self.frame_movable, node_index, object.kind(), "movable")?;
        let resizable = resolved_frame_flag(
            &self.frame_resizable,
            node_index,
            object.kind(),
            "resizable",
        )?;
        let top_level = resolved_frame_flag(
            &self.frame_top_level,
            node_index,
            object.kind(),
            "top-level",
        )?;
        let dont_save_position = resolved_frame_flag(
            &self.frame_dont_save_position,
            node_index,
            object.kind(),
            "position-persistence",
        )?;
        let frame_strata = if is_frame_object(object.kind()) {
            Some(
                self.frame_strata
                    .get(node_index)
                    .copied()
                    .flatten()
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("frame object {node_index} has no resolved frame strata"),
                    })?,
            )
        } else {
            None
        };
        let initial_anchors =
            self.region_anchors
                .get(node_index)
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("object {node_index} has no resolved anchor state"),
                })?;
        let anchors = lua
            .create_table()
            .map_err(|error| execution_error("object registration", error))?;
        for anchor in initial_anchors {
            let record = create_anchor_record(
                lua,
                anchor.point,
                anchor.target,
                anchor.relative_point,
                anchor.offset,
            )
            .map_err(|error| execution_error("object registration", error))?;
            anchors
                .raw_set(point_index(anchor.point), record)
                .map_err(|error| execution_error("object registration", error))?;
        }
        let table = lua
            .create_table()
            .map_err(|error| execution_error("object registration", error))?;
        table
            .raw_set(name_key(), object.name())
            .and_then(|()| table.raw_set(type_key(), object_type_name(object.kind())))
            .and_then(|()| table.raw_set(role_key(), object_role_name(object.role())))
            .and_then(|()| {
                table.raw_set(
                    parent_key(),
                    object.parent().map(|parent| parent.saturating_add(1)),
                )
            })
            .and_then(|()| table.raw_set(width_key(), dimensions.0))
            .and_then(|()| table.raw_set(height_key(), dimensions.1))
            .and_then(|()| table.raw_set(index_key(), node_index + 1))
            .and_then(|()| table.raw_set(anchors_key(), anchors))
            .and_then(|()| table.raw_set(shown_key(), shown))
            .and_then(|()| {
                table.raw_set(
                    alpha_key(),
                    self.region_alpha.get(node_index).copied().ok_or_else(|| {
                        mlua::Error::runtime("object has no resolved alpha state")
                    })?,
                )
            })
            .and_then(|()| {
                table.raw_set(
                    scale_key(),
                    self.region_scale.get(node_index).copied().ok_or_else(|| {
                        mlua::Error::runtime("object has no resolved scale state")
                    })?,
                )
            })
            .and_then(|()| table.raw_set(draw_layer_key(), "ARTWORK"))
            .and_then(|()| table.raw_set(draw_sub_level_key(), 0_i16))
            .and_then(|()| {
                table.raw_set(
                    texture_color_key(),
                    lua.create_sequence_from([1.0, 1.0, 1.0, 1.0])?,
                )
            })
            .map_err(|error| execution_error("object registration", error))?;
        if is_frame_object(object.kind()) {
            let script_handlers = lua
                .create_table()
                .map_err(|error| execution_error("object registration", error))?;
            let script_node = scripts
                .node(node_index)
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("frame object {node_index} has no script state"),
                })?;
            for binding in scripts.bindings_for(script_node) {
                let value = match binding.target() {
                    UiScriptTarget::Compiled(index) => Value::Function(
                        scripts
                            .compiled_function(lua, *index)
                            .map_err(|error| execution_error("object registration", error))?,
                    ),
                    UiScriptTarget::Global(name) => Value::String(
                        lua.create_string(name)
                            .map_err(|error| execution_error("object registration", error))?,
                    ),
                };
                script_handlers
                    .raw_set(binding.handler().name(), value)
                    .map_err(|error| execution_error("object registration", error))?;
            }
            table
                .raw_set(
                    events_key(),
                    lua.create_table()
                        .map_err(|error| execution_error("object registration", error))?,
                )
                .and_then(|()| table.raw_set(all_events_key(), false))
                .and_then(|()| table.raw_set(id_key(), frame_id))
                .and_then(|()| table.raw_set(frame_level_key(), frame_level))
                .and_then(|()| table.raw_set(frame_strata_key(), frame_strata))
                .and_then(|()| table.raw_set(keyboard_enabled_key(), keyboard_enabled))
                .and_then(|()| table.raw_set(mouse_enabled_key(), mouse_enabled))
                .and_then(|()| {
                    table.raw_set(
                        mouse_wheel_enabled_key(),
                        object.kind() == UiObjectKind::ScrollFrame,
                    )
                })
                .and_then(|()| table.raw_set(frame_clamped_key(), clamped_to_screen))
                .and_then(|()| table.raw_set(frame_movable_key(), movable))
                .and_then(|()| table.raw_set(frame_resizable_key(), resizable))
                .and_then(|()| table.raw_set(frame_top_level_key(), top_level))
                .and_then(|()| table.raw_set(frame_user_placed_key(), false))
                .and_then(|()| table.raw_set(frame_dont_save_position_key(), dont_save_position))
                .and_then(|()| table.raw_set(drag_button_key(), 0_u8))
                .and_then(|()| {
                    table.raw_set(
                        frame_clamp_insets_key(),
                        lua.create_sequence_from([0.0_f64; 4])?,
                    )
                })
                .and_then(|()| {
                    table.raw_set(
                        hit_rect_insets_key(),
                        lua.create_sequence_from([0.0_f64; 4])?,
                    )
                })
                .and_then(|()| table.raw_set(frame_depth_key(), 0.0))
                .and_then(|()| table.raw_set(ignore_depth_key(), false))
                .and_then(|()| table.raw_set(attributes_key(), lua.create_table()?))
                .and_then(|()| table.raw_set(script_handlers_key(), script_handlers))
                .map_err(|error| execution_error("object registration", error))?;
        }
        if is_enabled_control(object.kind()) {
            table
                .raw_set(enabled_key(), true)
                .map_err(|error| execution_error("object registration", error))?;
        }
        if matches!(
            object.kind(),
            UiObjectKind::Button | UiObjectKind::CheckButton
        ) {
            let button = self
                .buttons
                .get(node_index)
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("button {node_index} has no initial button state"),
                })?;
            table
                .raw_set(highlight_locked_key(), false)
                .and_then(|()| table.raw_set(hovered_key(), false))
                .and_then(|()| table.raw_set(disabled_text_color_key(), Option::<Table>::None))
                .and_then(|()| table.raw_set(click_action_key(), 0x8000_0000_u64))
                .and_then(|()| table.raw_set(button_pressed_key(), false))
                .and_then(|()| table.raw_set(button_state_locked_key(), false))
                .and_then(|()| table.raw_set(drag_button_key(), 0_u8))
                .map_err(|error| execution_error("object registration", error))?;
            set_initial_font(
                lua,
                &table,
                normal_font_key(),
                button.normal_font.as_deref(),
            )
            .and_then(|()| {
                set_initial_font(
                    lua,
                    &table,
                    disabled_font_key(),
                    button.disabled_font.as_deref(),
                )
            })
            .and_then(|()| {
                set_initial_font(
                    lua,
                    &table,
                    highlight_font_key(),
                    button.highlight_font.as_deref(),
                )
            })
            .map_err(|error| execution_error("object registration", error))?;
            if let Some(reference) = &button.text_reference {
                let text = stock_text(lua, reference)
                    .map_err(|error| execution_error("object registration", error))?;
                table
                    .raw_set(text_key(), text)
                    .map_err(|error| execution_error("object registration", error))?;
            }
        }
        if object.kind() == UiObjectKind::CheckButton {
            table
                .raw_set(checked_key(), false)
                .map_err(|error| execution_error("object registration", error))?;
        }
        if object.kind() == UiObjectKind::EditBox {
            initialize_edit_box(lua, &table)
                .map_err(|error| execution_error("object registration", error))?;
            let edit = self
                .font_strings
                .get(node_index)
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("EditBox {node_index} has no initial text state"),
                })?;
            table
                .raw_set(edit_max_letters_key(), edit.edit_max_letters)
                .and_then(|()| table.raw_set(edit_password_key(), edit.edit_password))
                .and_then(|()| table.raw_set(edit_multi_line_key(), edit.edit_multiline))
                .and_then(|()| {
                    table.raw_set(
                        edit_text_insets_key(),
                        lua.create_sequence_from(edit.edit_text_insets)?,
                    )
                })
                .and_then(|()| {
                    table.raw_set(
                        edit_highlight_color_key(),
                        lua.create_sequence_from(edit.edit_highlight_color)?,
                    )
                })
                .map_err(|error| execution_error("object registration", error))?;
        }
        if object.kind() == UiObjectKind::ScrollFrame {
            table
                .raw_set(horizontal_scroll_key(), 0.0)
                .and_then(|()| table.raw_set(vertical_scroll_key(), 0.0))
                .and_then(|()| table.raw_set(horizontal_scroll_range_key(), 0.0))
                .and_then(|()| table.raw_set(vertical_scroll_range_key(), 0.0))
                .and_then(|()| table.raw_set(scroll_child_key(), Option::<Table>::None))
                .map_err(|error| execution_error("object registration", error))?;
        }
        if matches!(
            object.kind(),
            UiObjectKind::Slider | UiObjectKind::StatusBar
        ) {
            table
                .raw_set(slider_min_key(), 0.0)
                .and_then(|()| table.raw_set(slider_max_key(), 0.0))
                .and_then(|()| table.raw_set(slider_value_key(), 0.0))
                .map_err(|error| execution_error("object registration", error))?;
        }
        if object.kind() == UiObjectKind::Slider {
            table
                .raw_set(slider_step_key(), 0.0)
                .and_then(|()| table.raw_set(slider_orientation_key(), "VERTICAL"))
                .map_err(|error| execution_error("object registration", error))?;
        }
        if object.kind() == UiObjectKind::StatusBar {
            table
                .raw_set(
                    status_bar_color_key(),
                    lua.create_sequence_from([1.0, 1.0, 1.0, 1.0])
                        .map_err(|error| execution_error("object registration", error))?,
                )
                .and_then(|()| table.raw_set(status_bar_texture_key(), Option::<Table>::None))
                .map_err(|error| execution_error("object registration", error))?;
        }
        if object.kind() == UiObjectKind::GameTooltip {
            table
                .raw_set(tooltip_owner_key(), Option::<usize>::None)
                .and_then(|()| table.raw_set(tooltip_anchor_key(), "ANCHOR_NONE"))
                .and_then(|()| table.raw_set(tooltip_offset_x_key(), 0.0))
                .and_then(|()| table.raw_set(tooltip_offset_y_key(), 0.0))
                .and_then(|()| table.raw_set(tooltip_padding_key(), 0.0))
                .map_err(|error| execution_error("object registration", error))?;
        }
        if object.kind() == UiObjectKind::Minimap {
            crate::feature::initialize_minimap_state(&table)
                .map_err(|error| execution_error("object registration", error))?;
        }
        if matches!(object.kind(), UiObjectKind::Model | UiObjectKind::ModelFfx) {
            initialize_model_runtime_state(lua, &table)
                .map_err(|error| execution_error("object registration", error))?;
        }
        if matches!(
            object.kind(),
            UiObjectKind::FontString | UiObjectKind::EditBox
        ) {
            let font = self
                .font_strings
                .get(node_index)
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("font string {node_index} has no initial font state"),
                })?;
            if object.kind() == UiObjectKind::FontString {
                table
                    .raw_set(auto_text_width_key(), dimensions.0 == 0.0)
                    .and_then(|()| table.raw_set(auto_text_height_key(), dimensions.1 == 0.0))
                    .map_err(|error| execution_error("object registration", error))?;
            }
            table
                .raw_set(font_set_key(), font.assigned)
                .and_then(|()| table.raw_set(justify_h_key(), font.justify_h.as_str()))
                .and_then(|()| table.raw_set(justify_v_key(), font.justify_v.as_str()))
                .and_then(|()| table.raw_set(spacing_key(), font.spacing))
                .and_then(|()| table.raw_set(word_wrap_key(), font.word_wrap))
                .and_then(|()| table.raw_set(non_space_wrap_key(), font.non_space_wrap))
                .and_then(|()| table.raw_set(max_text_lines_key(), font.max_lines))
                .and_then(|()| {
                    table.raw_set(
                        text_color_key(),
                        lua.create_sequence_from([1.0, 1.0, 1.0, 1.0])?,
                    )
                })
                .and_then(|()| {
                    table.raw_set(
                        font_shadow_offset_key(),
                        lua.create_sequence_from([0.0_f64, 0.0])?,
                    )
                })
                .and_then(|()| {
                    table.raw_set(
                        font_shadow_color_key(),
                        lua.create_sequence_from([0.0_f64, 0.0, 0.0, 1.0])?,
                    )
                })
                .map_err(|error| execution_error("object registration", error))?;
            if object.kind() == UiObjectKind::FontString
                && let Some(reference) = &font.text_reference
            {
                let text = stock_text(lua, reference)
                    .map_err(|error| execution_error("object registration", error))?;
                table
                    .raw_set(text_key(), text)
                    .map_err(|error| execution_error("object registration", error))?;
            }
            if let Some(name) = &font.object_name {
                let global: Table = lua.globals().raw_get(name.as_str()).map_err(|error| {
                    execution_error(
                        "object registration",
                        mlua::Error::runtime(format!(
                            "font string {} refers to unavailable font {name}: {error}",
                            object.name().unwrap_or("<unnamed>")
                        )),
                    )
                })?;
                let color: Table = global
                    .raw_get(text_color_key())
                    .map_err(|error| execution_error("object registration", error))?;
                let shadow_offset: Table = global
                    .raw_get(font_shadow_offset_key())
                    .map_err(|error| execution_error("object registration", error))?;
                let shadow_color: Table = global
                    .raw_get(font_shadow_color_key())
                    .map_err(|error| execution_error("object registration", error))?;
                table
                    .raw_set(font_object_key(), global)
                    .and_then(|()| table.raw_set(text_color_key(), color))
                    .and_then(|()| table.raw_set(font_shadow_offset_key(), shadow_offset))
                    .and_then(|()| table.raw_set(font_shadow_color_key(), shadow_color))
                    .map_err(|error| execution_error("object registration", error))?;
            }
        }
        if object.kind() == UiObjectKind::Texture {
            let texture = self
                .textures
                .get(node_index)
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("texture {node_index} has no initial state"),
                })?;
            table
                .raw_set(
                    tex_coord_key(),
                    lua.create_sequence_from(texture.coords)
                        .map_err(|error| execution_error("object registration", error))?,
                )
                .and_then(|()| {
                    table.raw_set(
                        texture_color_key(),
                        lua.create_sequence_from(texture.colors.iter().flatten().copied())?,
                    )
                })
                .and_then(|()| table.raw_set(texture_file_key(), texture.file.as_deref()))
                .and_then(|()| table.raw_set(portrait_unit_key(), Option::<String>::None))
                .and_then(|()| table.raw_set(texture_solid_color_key(), Option::<Table>::None))
                .and_then(|()| table.raw_set(texture_blend_mode_key(), texture.blend_mode))
                .and_then(|()| table.raw_set(horizontal_tiling_key(), texture.horizontal_tiling))
                .and_then(|()| table.raw_set(vertical_tiling_key(), texture.vertical_tiling))
                .and_then(|()| table.raw_set(non_blocking_key(), texture.non_blocking))
                .and_then(|()| table.raw_set(desaturated_key(), false))
                .and_then(|()| table.raw_set(draw_layer_key(), texture.draw_layer))
                .and_then(|()| table.raw_set(draw_sub_level_key(), texture.draw_sub_level))
                .map_err(|error| execution_error("object registration", error))?;
        }
        let metatable_key = self
            .object_metatables
            .get(object_kind_index(object.kind()))
            .ok_or_else(|| UiScriptError::Plan {
                message: format!(
                    "{} has no script metatable",
                    object_type_name(object.kind())
                ),
            })?;
        let metatable: Table = lua
            .registry_value(metatable_key)
            .map_err(|error| execution_error("object registration", error))?;
        table
            .set_metatable(Some(metatable))
            .map_err(|error| execution_error("object registration", error))?;
        let objects: Table = lua
            .named_registry_value(OBJECT_REGISTRY)
            .map_err(|error| execution_error("object registration", error))?;
        objects
            .raw_set(node_index + 1, table.clone())
            .map_err(|error| execution_error("object registration", error))?;
        if is_frame_object(object.kind()) {
            let subscribed = object_has_script(&table, UiScriptHandler::Update)
                .map_err(|error| execution_error("object registration", error))?;
            set_update_subscription(lua, &table, subscribed)
                .map_err(|error| execution_error("object registration", error))?;
        }
        if let Some(key) = object.parent_key()
            && let Some(parent) = object.construction_parent()
        {
            let owner: Table = objects
                .raw_get(parent + 1)
                .map_err(|error| execution_error("object registration", error))?;
            owner
                .raw_set(key, table.clone())
                .map_err(|error| execution_error("object registration", error))?;
        }
        if let Some(key) = widget_region_key(object.role())
            && let Some(parent) = object.construction_parent()
        {
            let owner: Table = objects
                .raw_get(parent + 1)
                .map_err(|error| execution_error("object registration", error))?;
            owner
                .raw_set(key, table.clone())
                .map_err(|error| execution_error("object registration", error))?;
            if object.role() == UiObjectRole::ScrollChild {
                update_scroll_child_rect(&owner)
                    .map_err(|error| execution_error("object registration", error))?;
            }
            if object.role() == UiObjectRole::ButtonText
                && let Some(font) = owner
                    .raw_get::<Option<Table>>(normal_font_key())
                    .map_err(|error| execution_error("object registration", error))?
            {
                table
                    .raw_set(font_object_key(), font)
                    .and_then(|()| table.raw_set(font_set_key(), true))
                    .and_then(|()| {
                        table.raw_set(text_key(), owner.raw_get::<Option<String>>(text_key())?)
                    })
                    .map_err(|error| execution_error("object registration", error))?;
            }
        }

        if let Some(name) = object.name() {
            let globals = lua.globals();
            let existing: Value = globals
                .raw_get(name)
                .map_err(|error| execution_error("object registration", error))?;
            if matches!(existing, Value::Nil) {
                globals
                    .raw_set(name, table)
                    .map_err(|error| execution_error("object registration", error))?;
            }
        }
        register_owner_animations(
            lua,
            &self.animations,
            node_index,
            &self.animation_metatables,
        )
        .map_err(|error| execution_error("animation registration", error))?;
        self.registered_objects
            .set(self.registered_objects.get() + 1);
        Ok(())
    }

    fn execute_load_handler(
        &mut self,
        lua: &Lua,
        tree: &UiObjectTree<'_>,
        node_index: usize,
    ) -> Result<(), UiScriptError> {
        let object = tree
            .nodes()
            .get(node_index)
            .ok_or_else(|| UiScriptError::Plan {
                message: format!("OnLoad object index {node_index} is outside the arena"),
            })?;
        if !is_frame_object(object.kind()) {
            return Ok(());
        }
        let label = format!(
            "{}<{}>:OnLoad",
            object.name().unwrap_or("<unnamed>"),
            object_type_name(object.kind())
        );
        let objects: Table = lua
            .named_registry_value(OBJECT_REGISTRY)
            .map_err(|error| execution_error(&label, error))?;
        let table: Table = objects
            .raw_get(node_index + 1)
            .map_err(|error| execution_error(&label, error))?;
        let Some(function) = object_script_function(lua, &table, UiScriptHandler::Load)
            .map_err(|error| execution_error(&label, error))?
        else {
            return Ok(());
        };
        call_object_handler(lua, &function, table)
            .map_err(|error| execution_error(&label, error))?;
        self.executed_load_handlers += 1;
        Ok(())
    }
}

fn register_create_frame(lua: &Lua, dynamic_arena: DynamicArenaState) -> mlua::Result<()> {
    lua.globals().raw_set(
        "CreateFrame",
        lua.create_function(
            move |lua,
                  (kind, name, parent, template): (
                String,
                Option<String>,
                Option<Table>,
                Option<String>,
            )| {
                let descriptor = runtime_template_descriptor(lua, template.as_deref())?;
                create_dynamic_frame(
                    lua,
                    &kind,
                    name.as_deref(),
                    parent,
                    descriptor,
                    &dynamic_arena.counters(),
                )
            },
        )?,
    )
}

#[derive(Clone)]
struct DynamicArenaState {
    static_object_count: usize,
    registered_objects: Rc<Cell<usize>>,
    dynamic_objects: Rc<Cell<usize>>,
}

impl DynamicArenaState {
    fn counters(&self) -> DynamicArenaCounters<'_> {
        DynamicArenaCounters {
            static_object_count: self.static_object_count,
            registered_objects: &self.registered_objects,
            dynamic_objects: &self.dynamic_objects,
        }
    }
}

fn runtime_template_descriptor(lua: &Lua, template: Option<&str>) -> mlua::Result<Option<Table>> {
    let Some(template) = template else {
        return Ok(None);
    };
    if template.contains(',') {
        return Err(mlua::Error::runtime(
            "CreateFrame multiple-template inheritance is not implemented",
        ));
    }
    let templates: Table = lua.named_registry_value(TEMPLATE_REGISTRY)?;
    let descriptor = templates.raw_get::<Table>(template).map_err(|_| {
        mlua::Error::runtime(format!("CreateFrame template {template} is unavailable"))
    })?;
    if let Some(dependency) = descriptor.raw_get::<Option<String>>("deferred_dependency")? {
        return Err(mlua::Error::runtime(format!(
            "CreateFrame template {template} awaits template {dependency}"
        )));
    }
    Ok(Some(descriptor))
}

struct DynamicArenaCounters<'state> {
    static_object_count: usize,
    registered_objects: &'state Cell<usize>,
    dynamic_objects: &'state Cell<usize>,
}

fn create_dynamic_frame(
    lua: &Lua,
    requested_kind: &str,
    requested_name: Option<&str>,
    requested_parent: Option<Table>,
    descriptor: Option<Table>,
    counters: &DynamicArenaCounters<'_>,
) -> mlua::Result<Table> {
    let requested_kind = OBJECT_KINDS
        .iter()
        .copied()
        .map(object_type_name)
        .find(|kind| kind.eq_ignore_ascii_case(requested_kind))
        .ok_or_else(|| mlua::Error::runtime(format!("unknown frame type {requested_kind}")))?;
    let records = if let Some(descriptor) = &descriptor {
        descriptor.raw_get::<Table>("nodes")?
    } else {
        let records = lua.create_table()?;
        let record = lua.create_table()?;
        record.raw_set("kind", requested_kind)?;
        record.raw_set("role", "object")?;
        record.raw_set("root_name", true)?;
        record.raw_set("children", lua.create_table()?)?;
        record.raw_set("width", 0.0)?;
        record.raw_set("height", 0.0)?;
        record.raw_set("shown", true)?;
        record.raw_set("alpha", 1.0)?;
        record.raw_set("scale", 1.0)?;
        record.raw_set("anchors", lua.create_table()?)?;
        record.raw_set("font_assigned", false)?;
        record.raw_set("font_text_reference", Option::<String>::None)?;
        record.raw_set("font_spacing", 0.0_f64)?;
        record.raw_set("font_word_wrap", true)?;
        record.raw_set("font_non_space_wrap", false)?;
        record.raw_set("font_max_lines", 0_u32)?;
        record.raw_set("edit_max_letters", 0_u32)?;
        record.raw_set("edit_password", false)?;
        record.raw_set("edit_multiline", false)?;
        record.raw_set("edit_text_insets", lua.create_sequence_from([0.0_f64; 4])?)?;
        record.raw_set(
            "edit_highlight_color",
            lua.create_sequence_from([96.0 / 255.0, 96.0 / 255.0, 96.0 / 255.0, 1.0])?,
        )?;
        record.raw_set(
            "justify_h",
            if requested_kind == "EditBox" {
                "LEFT"
            } else {
                "CENTER"
            },
        )?;
        record.raw_set("justify_v", "MIDDLE")?;
        record.raw_set(
            "texture_coords",
            lua.create_sequence_from([0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0])?,
        )?;
        record.raw_set("texture_color", lua.create_sequence_from([1.0; 16])?)?;
        record.raw_set("texture_file", Option::<String>::None)?;
        record.raw_set("texture_blend_mode", "BLEND")?;
        record.raw_set("horizontal_tiling", false)?;
        record.raw_set("vertical_tiling", false)?;
        record.raw_set("non_blocking", false)?;
        record.raw_set("draw_layer", "ARTWORK")?;
        record.raw_set("draw_sub_level", 0_i16)?;
        records.raw_set(1, record)?;
        records
    };
    let root_local = descriptor
        .as_ref()
        .map_or(Ok(1), |descriptor| descriptor.raw_get::<usize>("root"))?;
    let root_record: Table = records.raw_get(root_local)?;
    let template_kind = root_record.raw_get::<String>("kind")?;
    if !template_kind.eq_ignore_ascii_case(requested_kind) {
        return Err(mlua::Error::runtime(format!(
            "CreateFrame type {requested_kind} does not match template type {template_kind}"
        )));
    }
    let requested_name = requested_name
        .map(|name| expand_dynamic_parent_name(lua, name, requested_parent.as_ref()))
        .transpose()?;
    let base_name = requested_name.clone().or_else(|| {
        requested_parent
            .as_ref()
            .and_then(|parent| parent.raw_get(name_key()).ok())
    });
    let count = records.raw_len();
    let mut objects = Vec::with_capacity(count);
    for local_index in 1..=count {
        let record: Table = records.raw_get(local_index)?;
        let kind = record.raw_get::<String>("kind")?;
        let name = dynamic_name(&record, requested_name.as_deref(), base_name.as_deref())?;
        let parent = if local_index == root_local {
            requested_parent.clone()
        } else if let Some(parent_index) = record.raw_get::<Option<usize>>("parent")? {
            objects.get(parent_index - 1).cloned()
        } else if let Some(parent_name) = record.raw_get::<Option<String>>("external_parent")? {
            lua.globals().raw_get::<Option<Table>>(parent_name)?
        } else {
            None
        };
        let index = counters.static_object_count + counters.dynamic_objects.get() + 1;
        let object =
            create_dynamic_object(lua, &kind, name.as_deref(), parent.as_ref(), index, &record)?;
        counters
            .dynamic_objects
            .set(counters.dynamic_objects.get() + 1);
        counters
            .registered_objects
            .set(counters.registered_objects.get() + 1);
        objects.push(object);
    }
    apply_dynamic_anchors(lua, &records, &objects)?;
    run_dynamic_load(lua, &records, &objects, root_local)?;
    objects
        .get(root_local - 1)
        .cloned()
        .ok_or_else(|| mlua::Error::runtime("CreateFrame root is outside its template"))
}

fn expand_dynamic_parent_name(
    lua: &Lua,
    source: &str,
    parent: Option<&Table>,
) -> mlua::Result<String> {
    let Some(prefix) = source.get(..7) else {
        return Ok(source.to_owned());
    };
    if !prefix.eq_ignore_ascii_case("$parent") {
        return Ok(source.to_owned());
    }

    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let mut cursor = parent.cloned();
    while let Some(candidate) = cursor {
        if let Some(name) = candidate.raw_get::<Option<String>>(name_key())?
            && !name.is_empty()
        {
            return Ok(format!("{name}{}", &source[7..]));
        }
        cursor = candidate
            .raw_get::<Option<usize>>(parent_key())?
            .map(|index| objects.raw_get::<Table>(index))
            .transpose()?;
    }
    Ok(source[7..].to_owned())
}

fn dynamic_name(
    record: &Table,
    requested_name: Option<&str>,
    base_name: Option<&str>,
) -> mlua::Result<Option<String>> {
    if record.raw_get::<Option<bool>>("root_name")?.is_some() {
        Ok(requested_name.map(str::to_owned))
    } else if let Some(suffix) = record.raw_get::<Option<String>>("root_suffix")? {
        Ok(Some(format!("{}{suffix}", base_name.unwrap_or(""))))
    } else {
        record.raw_get("absolute_name")
    }
}

fn create_dynamic_object(
    lua: &Lua,
    kind: &str,
    name: Option<&str>,
    parent: Option<&Table>,
    index: usize,
    record: &Table,
) -> mlua::Result<Table> {
    let object = lua.create_table()?;
    object.raw_set(name_key(), name)?;
    object.raw_set(type_key(), kind)?;
    object.raw_set(role_key(), record.raw_get::<String>("role")?)?;
    object.raw_set(
        parent_key(),
        parent
            .map(|parent| parent.raw_get::<usize>(index_key()))
            .transpose()?,
    )?;
    object.raw_set(index_key(), index)?;
    object.raw_set(width_key(), record.raw_get::<f64>("width")?)?;
    object.raw_set(height_key(), record.raw_get::<f64>("height")?)?;
    object.raw_set(shown_key(), record.raw_get::<bool>("shown")?)?;
    object.raw_set(alpha_key(), record.raw_get::<f64>("alpha")?)?;
    object.raw_set(scale_key(), record.raw_get::<f64>("scale")?)?;
    object.raw_set(anchors_key(), lua.create_table()?)?;
    object.raw_set(draw_layer_key(), record.raw_get::<String>("draw_layer")?)?;
    object.raw_set(
        draw_sub_level_key(),
        record.raw_get::<i16>("draw_sub_level")?,
    )?;
    object.raw_set(
        texture_color_key(),
        lua.create_sequence_from([1.0, 1.0, 1.0, 1.0])?,
    )?;
    if !matches!(kind, "Texture" | "FontString") {
        object.raw_set(events_key(), lua.create_table()?)?;
        object.raw_set(all_events_key(), false)?;
        object.raw_set(id_key(), 0)?;
        let level = parent
            .map(|parent| parent.raw_get::<i32>(frame_level_key()))
            .transpose()?
            .map_or(0, |level| level.saturating_add(1));
        object.raw_set(frame_level_key(), level)?;
        let strata = parent
            .map(|parent| parent.raw_get::<String>(frame_strata_key()))
            .transpose()?
            .unwrap_or_else(|| "MEDIUM".to_owned());
        object.raw_set(frame_strata_key(), strata)?;
        object.raw_set(keyboard_enabled_key(), false)?;
        object.raw_set(
            mouse_enabled_key(),
            matches!(kind, "Button" | "CheckButton" | "EditBox"),
        )?;
        object.raw_set(mouse_wheel_enabled_key(), kind == "ScrollFrame")?;
        object.raw_set(
            frame_clamped_key(),
            record.raw_get::<bool>("clamped_to_screen")?,
        )?;
        object.raw_set(frame_movable_key(), record.raw_get::<bool>("movable")?)?;
        object.raw_set(frame_resizable_key(), record.raw_get::<bool>("resizable")?)?;
        object.raw_set(frame_top_level_key(), record.raw_get::<bool>("top_level")?)?;
        object.raw_set(frame_user_placed_key(), false)?;
        object.raw_set(
            frame_dont_save_position_key(),
            record.raw_get::<bool>("dont_save_position")?,
        )?;
        object.raw_set(drag_button_key(), 0_u8)?;
        object.raw_set(
            frame_clamp_insets_key(),
            lua.create_sequence_from([0.0_f64; 4])?,
        )?;
        object.raw_set(
            hit_rect_insets_key(),
            lua.create_sequence_from([0.0_f64; 4])?,
        )?;
        object.raw_set(frame_depth_key(), 0.0)?;
        object.raw_set(ignore_depth_key(), false)?;
        object.raw_set(attributes_key(), lua.create_table()?)?;
        let handlers = lua.create_table()?;
        if let Some(initial) = record.raw_get::<Option<Table>>("scripts")? {
            for pair in initial.pairs::<String, Value>() {
                let (name, value) = pair?;
                handlers.raw_set(name, value)?;
            }
        }
        object.raw_set(script_handlers_key(), handlers)?;
    }
    if matches!(kind, "Button" | "CheckButton" | "Slider") {
        object.raw_set(enabled_key(), true)?;
    }
    if matches!(kind, "Button" | "CheckButton") {
        object.raw_set(highlight_locked_key(), false)?;
        object.raw_set(hovered_key(), false)?;
        object.raw_set(disabled_text_color_key(), Option::<Table>::None)?;
        object.raw_set(click_action_key(), 0x8000_0000_u64)?;
        object.raw_set(button_pressed_key(), false)?;
        object.raw_set(button_state_locked_key(), false)?;
        object.raw_set(drag_button_key(), 0_u8)?;
        set_initial_font(
            lua,
            &object,
            normal_font_key(),
            record.raw_get::<Option<String>>("normal_font")?.as_deref(),
        )?;
        set_initial_font(
            lua,
            &object,
            disabled_font_key(),
            record
                .raw_get::<Option<String>>("disabled_font")?
                .as_deref(),
        )?;
        set_initial_font(
            lua,
            &object,
            highlight_font_key(),
            record
                .raw_get::<Option<String>>("highlight_font")?
                .as_deref(),
        )?;
        if let Some(reference) = record.raw_get::<Option<String>>("button_text_reference")? {
            object.raw_set(text_key(), stock_text(lua, &reference)?)?;
        }
    }
    if kind == "CheckButton" {
        object.raw_set(checked_key(), false)?;
    }
    if kind == "EditBox" {
        initialize_edit_box(lua, &object)?;
        object.raw_set(
            edit_max_letters_key(),
            record.raw_get::<u32>("edit_max_letters")?,
        )?;
        object.raw_set(
            edit_password_key(),
            record.raw_get::<bool>("edit_password")?,
        )?;
        object.raw_set(
            edit_multi_line_key(),
            record.raw_get::<bool>("edit_multiline")?,
        )?;
        object.raw_set(
            edit_text_insets_key(),
            record.raw_get::<Table>("edit_text_insets")?,
        )?;
        object.raw_set(
            edit_highlight_color_key(),
            record.raw_get::<Table>("edit_highlight_color")?,
        )?;
    }
    if kind == "ScrollFrame" {
        object.raw_set(horizontal_scroll_key(), 0.0)?;
        object.raw_set(vertical_scroll_key(), 0.0)?;
        object.raw_set(horizontal_scroll_range_key(), 0.0)?;
        object.raw_set(vertical_scroll_range_key(), 0.0)?;
        object.raw_set(scroll_child_key(), Option::<Table>::None)?;
    }
    if matches!(kind, "Slider" | "StatusBar") {
        object.raw_set(slider_min_key(), 0.0)?;
        object.raw_set(slider_max_key(), 0.0)?;
        object.raw_set(slider_value_key(), 0.0)?;
    }
    if kind == "Slider" {
        object.raw_set(slider_step_key(), 0.0)?;
        object.raw_set(slider_orientation_key(), "VERTICAL")?;
    }
    if kind == "StatusBar" {
        object.raw_set(
            status_bar_color_key(),
            lua.create_sequence_from([1.0, 1.0, 1.0, 1.0])?,
        )?;
        object.raw_set(status_bar_texture_key(), Option::<Table>::None)?;
    }
    if kind == "GameTooltip" {
        object.raw_set(tooltip_owner_key(), Option::<usize>::None)?;
        object.raw_set(tooltip_anchor_key(), "ANCHOR_NONE")?;
        object.raw_set(tooltip_offset_x_key(), 0.0)?;
        object.raw_set(tooltip_offset_y_key(), 0.0)?;
        object.raw_set(tooltip_padding_key(), 0.0)?;
    }
    if matches!(kind, "Model" | "ModelFFX") {
        initialize_model_runtime_state(lua, &object)?;
    }
    if matches!(kind, "FontString" | "EditBox") {
        if kind == "FontString" {
            object.raw_set(
                auto_text_width_key(),
                record.raw_get::<f64>("width")? == 0.0,
            )?;
            object.raw_set(
                auto_text_height_key(),
                record.raw_get::<f64>("height")? == 0.0,
            )?;
        }
        object.raw_set(font_set_key(), record.raw_get::<bool>("font_assigned")?)?;
        object.raw_set(justify_h_key(), record.raw_get::<String>("justify_h")?)?;
        object.raw_set(justify_v_key(), record.raw_get::<String>("justify_v")?)?;
        object.raw_set(spacing_key(), record.raw_get::<f64>("font_spacing")?)?;
        object.raw_set(word_wrap_key(), record.raw_get::<bool>("font_word_wrap")?)?;
        object.raw_set(
            non_space_wrap_key(),
            record.raw_get::<bool>("font_non_space_wrap")?,
        )?;
        object.raw_set(
            max_text_lines_key(),
            record.raw_get::<u32>("font_max_lines")?,
        )?;
        object.raw_set(
            text_color_key(),
            lua.create_sequence_from([1.0, 1.0, 1.0, 1.0])?,
        )?;
        object.raw_set(
            font_shadow_offset_key(),
            lua.create_sequence_from([0.0_f64, 0.0])?,
        )?;
        object.raw_set(
            font_shadow_color_key(),
            lua.create_sequence_from([0.0_f64, 0.0, 0.0, 1.0])?,
        )?;
        if kind == "FontString"
            && let Some(reference) = record.raw_get::<Option<String>>("font_text_reference")?
        {
            object.raw_set(text_key(), stock_text(lua, &reference)?)?;
        }
        if let Some(name) = record.raw_get::<Option<String>>("font_object_name")? {
            let font: Table = lua.globals().raw_get(name.as_str())?;
            let color: Table = font.raw_get(text_color_key())?;
            let shadow_offset: Table = font.raw_get(font_shadow_offset_key())?;
            let shadow_color: Table = font.raw_get(font_shadow_color_key())?;
            object.raw_set(font_object_key(), font)?;
            object.raw_set(text_color_key(), color)?;
            object.raw_set(font_shadow_offset_key(), shadow_offset)?;
            object.raw_set(font_shadow_color_key(), shadow_color)?;
        }
    }
    if kind == "Texture" {
        object.raw_set(tex_coord_key(), record.raw_get::<Table>("texture_coords")?)?;
        object.raw_set(
            texture_color_key(),
            record.raw_get::<Table>("texture_color")?,
        )?;
        object.raw_set(
            texture_file_key(),
            record.raw_get::<Option<String>>("texture_file")?,
        )?;
        object.raw_set(portrait_unit_key(), Option::<String>::None)?;
        object.raw_set(texture_solid_color_key(), Option::<Table>::None)?;
        object.raw_set(
            texture_blend_mode_key(),
            record.raw_get::<String>("texture_blend_mode")?,
        )?;
        object.raw_set(
            horizontal_tiling_key(),
            record.raw_get::<bool>("horizontal_tiling")?,
        )?;
        object.raw_set(
            vertical_tiling_key(),
            record.raw_get::<bool>("vertical_tiling")?,
        )?;
        object.raw_set(non_blocking_key(), record.raw_get::<bool>("non_blocking")?)?;
        object.raw_set(desaturated_key(), false)?;
        object.raw_set(draw_layer_key(), record.raw_get::<String>("draw_layer")?)?;
        object.raw_set(
            draw_sub_level_key(),
            record.raw_get::<i16>("draw_sub_level")?,
        )?;
    }
    let metatables: Table = lua.named_registry_value(METATABLE_REGISTRY)?;
    object.set_metatable(Some(metatables.raw_get(kind)?))?;
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    objects.raw_set(index, object.clone())?;
    if !matches!(kind, "Texture" | "FontString") {
        let subscribed = object_has_script(&object, UiScriptHandler::Update)?;
        set_update_subscription(lua, &object, subscribed)?;
    }
    if let Some(parent) = parent
        && let Some(key) = dynamic_widget_region_key(record.raw_get::<String>("role")?.as_str())
    {
        parent.raw_set(key, object.clone())?;
        if key == button_text_key()
            && let Some(font) = parent.raw_get::<Option<Table>>(normal_font_key())?
        {
            object.raw_set(font_object_key(), font)?;
            object.raw_set(font_set_key(), true)?;
            object.raw_set(text_key(), parent.raw_get::<Option<String>>(text_key())?)?;
        }
    }
    if let Some(parent) = parent
        && let Some(parent_key) = record.raw_get::<Option<String>>("parent_key")?
    {
        parent.raw_set(parent_key, object.clone())?;
    }
    if let Some(name) = name
        && matches!(lua.globals().raw_get::<Value>(name)?, Value::Nil)
    {
        lua.globals().raw_set(name, object.clone())?;
    }
    Ok(object)
}

fn apply_dynamic_anchors(lua: &Lua, records: &Table, objects: &[Table]) -> mlua::Result<()> {
    for (local_index, object) in objects.iter().enumerate() {
        let record: Table = records.raw_get(local_index + 1)?;
        let prototypes: Table = record.raw_get("anchors")?;
        let anchors = lua.create_table()?;
        for point_index in 1..=9 {
            let Some(prototype) = prototypes.raw_get::<Option<Table>>(point_index)? else {
                continue;
            };
            let point_name = prototype.raw_get::<String>("point")?;
            let point = parse_point(&point_name)
                .ok_or_else(|| mlua::Error::runtime("runtime template has unknown anchor point"))?;
            let relative_name = prototype.raw_get::<String>("relative_point")?;
            let relative_point = parse_point(&relative_name).ok_or_else(|| {
                mlua::Error::runtime("runtime template has unknown relative anchor point")
            })?;
            let target_kind = prototype.raw_get::<String>("target_kind")?;
            let target = match target_kind.as_str() {
                "parent" => object.raw_get::<Option<usize>>(parent_key())?,
                "local" => {
                    let target = prototype.raw_get::<usize>("target")?;
                    Some(
                        objects
                            .get(target.saturating_sub(1))
                            .ok_or_else(|| {
                                mlua::Error::runtime(
                                    "runtime template anchor target is outside its instance",
                                )
                            })?
                            .raw_get::<usize>(index_key())?,
                    )
                }
                "global" => {
                    let name = prototype.raw_get::<String>("target")?;
                    let target: Table = lua.globals().raw_get(name.as_str()).map_err(|_| {
                        mlua::Error::runtime(format!(
                            "runtime template anchor target {name} is unavailable"
                        ))
                    })?;
                    Some(target.raw_get::<usize>(index_key())?)
                }
                "dynamic" => {
                    let source = prototype.raw_get::<String>("target")?;
                    let registry: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
                    let parent = object
                        .raw_get::<Option<usize>>(parent_key())?
                        .map(|index| registry.raw_get::<Table>(index))
                        .transpose()?;
                    let name = expand_dynamic_parent_name(lua, &source, parent.as_ref())?;
                    let target = lua
                        .globals()
                        .raw_get::<Option<Table>>(name.as_str())?
                        .ok_or_else(|| {
                            mlua::Error::runtime(format!(
                                "runtime template anchor target {name} is unavailable"
                            ))
                        })?;
                    Some(target.raw_get::<usize>(index_key())?)
                }
                _ => {
                    return Err(mlua::Error::runtime(
                        "runtime template has unknown anchor target kind",
                    ));
                }
            };
            let anchor = create_anchor_record(
                lua,
                point,
                target,
                relative_point,
                (
                    prototype.raw_get::<f64>("x")?,
                    prototype.raw_get::<f64>("y")?,
                ),
            )?;
            anchors.raw_set(point_index, anchor)?;
        }
        object.raw_set(anchors_key(), anchors)?;
    }
    Ok(())
}

fn run_dynamic_load(
    lua: &Lua,
    records: &Table,
    objects: &[Table],
    local: usize,
) -> mlua::Result<()> {
    let record: Table = records.raw_get(local)?;
    let children: Table = record.raw_get("children")?;
    for child in children.sequence_values::<usize>() {
        run_dynamic_load(lua, records, objects, child?)?;
    }
    let object = objects
        .get(local - 1)
        .cloned()
        .ok_or_else(|| mlua::Error::runtime("dynamic OnLoad object is outside its template"))?;
    if let Some(function) = object_script_function(lua, &object, UiScriptHandler::Load)? {
        call_object_handler(lua, &function, object)?;
    }
    Ok(())
}

/// Installs and restores build 12340's legacy global `this` around callbacks.
fn call_object_handler(lua: &Lua, function: &mlua::Function, object: Table) -> mlua::Result<()> {
    let globals = lua.globals();
    let previous = globals.raw_get::<Value>("this")?;
    globals.raw_set("this", object.clone())?;
    let result = function.call::<()>(object);
    let restore = globals.raw_set("this", previous);
    match result {
        Ok(()) => restore,
        Err(error) => {
            let _ = restore;
            Err(error)
        }
    }
}

/// Applies CheckButton's native state mutation before `OnClick`. Character
/// creation's three authored choice families are mutually exclusive; ordinary
/// check buttons toggle independently.
fn apply_check_button_click(lua: &Lua, object: &Table) -> mlua::Result<()> {
    let name = object.raw_get::<Option<String>>(name_key())?;
    let group = name.as_deref().map_or(0, character_create_choice_group);
    if group == 0 {
        let checked = object.raw_get::<bool>(checked_key())?;
        return object.raw_set(checked_key(), !checked);
    }
    let parent = object.raw_get::<Option<usize>>(parent_key())?;
    let clicked_index = object.raw_get::<usize>(index_key())?;
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    for index in 1..=objects.raw_len() {
        let candidate: Table = objects.raw_get(index)?;
        if candidate.raw_get::<String>(type_key())? != "CheckButton"
            || candidate.raw_get::<Option<usize>>(parent_key())? != parent
        {
            continue;
        }
        let candidate_name = candidate.raw_get::<Option<String>>(name_key())?;
        if candidate_name
            .as_deref()
            .is_some_and(|name| character_create_choice_group(name) == group)
        {
            candidate.raw_set(
                checked_key(),
                candidate.raw_get::<usize>(index_key())? == clicked_index,
            )?;
        }
    }
    Ok(())
}

fn character_create_choice_group(name: &str) -> u8 {
    if name.starts_with("CharacterCreateRaceButton") {
        1
    } else if name.starts_with("CharacterCreateClassButton") {
        2
    } else if matches!(
        name,
        "CharacterCreateGenderButtonMale" | "CharacterCreateGenderButtonFemale"
    ) {
        3
    } else {
        0
    }
}

/// Installs and restores build 12340's legacy global `this` around a callback
/// receiving one native string argument.
fn call_string_object_handler(
    lua: &Lua,
    function: &mlua::Function,
    object: Table,
    value: &str,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let previous = globals.raw_get::<Value>("this")?;
    globals.raw_set("this", object.clone())?;
    let result = function.call::<()>((object, value));
    let restore = globals.raw_set("this", previous);
    match result {
        Ok(()) => restore,
        Err(error) => {
            let _ = restore;
            Err(error)
        }
    }
}

/// Preserves the legacy `this` and `arg1` globals for pointer callbacks.
fn call_legacy_string_handler(
    lua: &Lua,
    function: &mlua::Function,
    object: Table,
    value: &str,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let previous_this = globals.raw_get::<Value>("this")?;
    let previous_arg1 = globals.raw_get::<Value>("arg1")?;
    globals.raw_set("this", object.clone())?;
    globals.raw_set("arg1", value)?;
    let result = function.call::<()>((object, value));
    let restore_this = globals.raw_set("this", previous_this);
    let restore_arg1 = globals.raw_set("arg1", previous_arg1);
    match result {
        Ok(()) => {
            restore_this?;
            restore_arg1
        }
        Err(error) => {
            let _ = restore_this;
            let _ = restore_arg1;
            Err(error)
        }
    }
}

fn dispatch_subscribers(
    lua: &Lua,
    object_count: usize,
    event: &str,
    payload: &[Value],
) -> mlua::Result<usize> {
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let mut dispatched = 0;
    for index in 1..=object_count {
        let object: Table = objects.raw_get(index)?;
        let all_events = object
            .raw_get::<Option<bool>>(all_events_key())?
            .unwrap_or(false);
        let subscribed = if all_events {
            true
        } else if let Some(events) = object.raw_get::<Option<Table>>(events_key())? {
            events.raw_get::<Option<bool>>(event)? == Some(true)
        } else {
            false
        };
        if !subscribed {
            continue;
        }
        let Some(function) = object_script_function(lua, &object, UiScriptHandler::Event)? else {
            continue;
        };
        call_event_handler(lua, &function, object, event, payload)?;
        dispatched += 1;
    }
    Ok(dispatched)
}

fn call_event_handler(
    lua: &Lua,
    function: &mlua::Function,
    object: Table,
    event: &str,
    payload: &[Value],
) -> mlua::Result<()> {
    let globals = lua.globals();
    let previous_this = globals.raw_get::<Value>("this")?;
    globals.raw_set("this", object.clone())?;
    let mut arguments = Vec::with_capacity(payload.len() + 2);
    arguments.push(Value::Table(object));
    arguments.push(Value::String(lua.create_string(event)?));
    arguments.extend(payload.iter().cloned());
    let result = function.call::<()>(MultiValue::from_vec(arguments));
    let restore = globals.raw_set("this", previous_this);
    match result {
        Ok(()) => restore,
        Err(error) => {
            let _ = restore;
            Err(error)
        }
    }
}

fn event_argument(lua: &Lua, argument: &UiEventArgument) -> mlua::Result<Value> {
    match argument {
        UiEventArgument::Nil => Ok(Value::Nil),
        UiEventArgument::Boolean(value) => Ok(Value::Boolean(*value)),
        UiEventArgument::Integer(value) => Ok(Value::Integer(*value)),
        UiEventArgument::Number(value) => Ok(Value::Number(*value)),
        UiEventArgument::String(value) => lua.create_string(value).map(Value::String),
    }
}

fn restore_event_globals(lua: &Lua, event: Value, arguments: Vec<Value>) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.raw_set("event", event)?;
    for (index, value) in arguments.into_iter().enumerate() {
        globals.raw_set(format!("arg{}", index + 1), value)?;
    }
    Ok(())
}

fn register_font(
    lua: &Lua,
    metatable_key: &RegistryKey,
    definition: &FontDefinition,
) -> Result<(), UiScriptError> {
    let table = lua
        .create_table()
        .map_err(|error| execution_error("font registration", error))?;
    let color = definition.color().map_or([1.0, 1.0, 1.0, 1.0], |color| {
        [
            f64::from(color.red()),
            f64::from(color.green()),
            f64::from(color.blue()),
            f64::from(color.alpha().unwrap_or(1.0)),
        ]
    });
    let shadow_offset = definition
        .shadow()
        .and_then(FontShadow::offset)
        .map_or([0.0, 0.0], |(x, y)| [f64::from(x), f64::from(y)]);
    let shadow_color =
        definition
            .shadow()
            .and_then(FontShadow::color)
            .map_or([0.0, 0.0, 0.0, 1.0], |color| {
                [
                    f64::from(color.red()),
                    f64::from(color.green()),
                    f64::from(color.blue()),
                    f64::from(color.alpha().unwrap_or(1.0)),
                ]
            });
    table
        .raw_set(name_key(), definition.name())
        .and_then(|()| table.raw_set(type_key(), "Font"))
        .and_then(|()| {
            table.raw_set(
                font_face_key(),
                definition.face().map(solarity_asset::AssetPath::as_str),
            )
        })
        .and_then(|()| table.raw_set(font_height_key(), definition.height().map(f64::from)))
        .and_then(|()| table.raw_set(font_flags_key(), font_flags(definition)))
        .and_then(|()| table.raw_set(spacing_key(), definition.spacing().map_or(0.0, f64::from)))
        .and_then(|()| table.raw_set(text_color_key(), lua.create_sequence_from(color)?))
        .and_then(|()| {
            table.raw_set(
                font_shadow_offset_key(),
                lua.create_sequence_from(shadow_offset)?,
            )
        })
        .and_then(|()| {
            table.raw_set(
                font_shadow_color_key(),
                lua.create_sequence_from(shadow_color)?,
            )
        })
        .map_err(|error| execution_error("font registration", error))?;
    let metatable: Table = lua
        .registry_value(metatable_key)
        .map_err(|error| execution_error("font registration", error))?;
    table
        .set_metatable(Some(metatable))
        .map_err(|error| execution_error("font registration", error))?;

    // FrameScript_Object registers a named object only when that global is
    // currently nil. This first-wins behavior is observable by stock Lua.
    let globals = lua.globals();
    let existing: Value = globals
        .raw_get(definition.name())
        .map_err(|error| execution_error("font registration", error))?;
    if matches!(existing, Value::Nil) {
        globals
            .raw_set(definition.name(), table)
            .map_err(|error| execution_error("font registration", error))?;
    }
    Ok(())
}

fn create_font_metatable(lua: &Lua) -> mlua::Result<Table> {
    let methods = lua.create_table()?;
    methods.raw_set(
        "GetName",
        lua.create_function(|_, object: Table| object.raw_get::<String>(name_key()))?,
    )?;
    methods.raw_set(
        "GetObjectType",
        lua.create_function(|_, _: Table| Ok("Font"))?,
    )?;
    methods.raw_set(
        "IsObjectType",
        lua.create_function(|_, (_, candidate): (Table, String)| {
            Ok(candidate.eq_ignore_ascii_case("Font").then_some(true))
        })?,
    )?;
    register_font_spacing_methods(lua, &methods)?;
    register_font_shadow_methods(lua, &methods)?;
    let metatable = lua.create_table()?;
    metatable.raw_set("__index", methods)?;
    Ok(metatable)
}

fn font_flags(definition: &FontDefinition) -> String {
    let mut flags = Vec::with_capacity(2);
    if let Some(outline) = definition.outline() {
        flags.push(match outline {
            FontOutline::Normal => "OUTLINE",
            FontOutline::Thick => "THICKOUTLINE",
        });
    }
    if definition.monochrome() == Some(true) {
        flags.push("MONOCHROME");
    }
    flags.join(", ")
}

#[allow(clippy::too_many_arguments)]
fn create_object_metatable(
    lua: &Lua,
    manifest_kind: UiManifestKind,
    kind: UiObjectKind,
    ui_extent: (f64, f64),
    assets: Option<AssetStoreHandle>,
    media_intent: Rc<RefCell<UiGlueMediaIntent>>,
    text_measurement: Option<buttons::TextMeasurement>,
    dynamic_arena: DynamicArenaState,
) -> mlua::Result<Table> {
    let methods = lua.create_table()?;
    methods.raw_set(
        "GetName",
        lua.create_function(|_, object: Table| object.raw_get::<Option<String>>(name_key()))?,
    )?;
    methods.raw_set(
        "GetObjectType",
        lua.create_function(|_, object: Table| object.raw_get::<String>(type_key()))?,
    )?;
    methods.raw_set(
        "IsObjectType",
        lua.create_function(|_, (object, candidate): (Table, String)| {
            let exact = object.raw_get::<String>(type_key())?;
            Ok(is_object_type(&exact, &candidate).then_some(true))
        })?,
    )?;
    methods.raw_set(
        "GetParent",
        lua.create_function(|lua, object: Table| {
            let Some(parent) = object.raw_get::<Option<usize>>(parent_key())? else {
                return Ok(None::<Table>);
            };
            let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
            objects.raw_get::<Option<Table>>(parent)
        })?,
    )?;
    register_region_methods(lua, &methods, kind, ui_extent)?;
    if is_frame_object(kind) {
        register_frame_event_methods(lua, &methods, manifest_kind)?;
        register_frame_backdrop_methods(lua, &methods)?;
        register_frame_visibility_methods(lua, &methods)?;
        register_frame_attribute_methods(lua, &methods)?;
        register_frame_script_methods(lua, &methods, kind)?;
        register_frame_region_factory_methods(lua, &methods, dynamic_arena.clone())?;
        buttons::register_drag_methods(lua, &methods)?;
    }
    if is_enabled_control(kind) {
        register_enabled_methods(lua, &methods, kind)?;
    }
    if matches!(kind, UiObjectKind::Button | UiObjectKind::CheckButton) {
        buttons::register_button_methods(
            lua,
            &methods,
            text_measurement.clone(),
            dynamic_arena.clone(),
        )?;
    }
    if kind == UiObjectKind::CheckButton {
        buttons::register_check_button_methods(lua, &methods)?;
    }
    if kind == UiObjectKind::EditBox {
        register_edit_box_methods(lua, &methods)?;
    }
    if kind == UiObjectKind::FontString {
        register_font_string_methods(lua, &methods, text_measurement)?;
    }
    if kind == UiObjectKind::SimpleHtml {
        register_simple_html_methods(lua, &methods)?;
    }
    if kind == UiObjectKind::Texture {
        register_texture_methods(lua, &methods)?;
    }
    if matches!(kind, UiObjectKind::Model | UiObjectKind::ModelFfx) {
        register_model_methods(lua, &methods, assets.clone())?;
    }
    if kind == UiObjectKind::MovieFrame {
        register_movie_frame_methods(lua, &methods, assets, media_intent)?;
    }
    if kind == UiObjectKind::ScrollFrame {
        register_scroll_frame_methods(lua, &methods)?;
    }
    if kind == UiObjectKind::Slider {
        register_slider_methods(lua, &methods)?;
    }
    if kind == UiObjectKind::StatusBar {
        register_status_bar_methods(lua, &methods, dynamic_arena)?;
    }
    if kind == UiObjectKind::GameTooltip {
        tooltips::register_game_tooltip_methods(lua, &methods)?;
    }
    if kind == UiObjectKind::Minimap {
        crate::feature::register_minimap_methods(lua, &methods)?;
    }
    if kind == UiObjectKind::QuestPoiFrame {
        crate::feature::register_quest_poi_methods(lua, &methods)?;
    }
    let metatable = lua.create_table()?;
    metatable.raw_set("__index", methods)?;
    Ok(metatable)
}

/// Installs the per-frame secure attribute store and wildcard lookup order.
fn register_frame_attribute_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetAttribute",
        lua.create_function(|lua, (object, name, value): (Table, String, Value)| {
            let attributes: Table = object.raw_get(attributes_key())?;
            attributes.raw_set(name.as_str(), value.clone())?;
            if let Some(function) =
                object_script_function(lua, &object, UiScriptHandler::AttributeChanged)?
            {
                call_attribute_changed_handler(lua, &function, object, name, value)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "GetAttribute",
        lua.create_function(|lua, (object, arguments): (Table, Variadic<Value>)| {
            let attributes: Table = object.raw_get(attributes_key())?;
            match arguments.as_slice() {
                [name] => {
                    let name = attribute_name(lua, name)?;
                    attributes.raw_get::<Value>(name)
                }
                [prefix, name, suffix] => {
                    let prefix = attribute_name(lua, prefix)?;
                    let name = attribute_name(lua, name)?;
                    let suffix = attribute_name(lua, suffix)?;
                    for candidate in [
                        format!("{prefix}{name}{suffix}"),
                        format!("*{name}{suffix}"),
                        format!("{prefix}{name}*"),
                        format!("*{name}*"),
                        name,
                    ] {
                        let value = attributes.raw_get::<Value>(candidate)?;
                        if !matches!(value, Value::Nil) {
                            return Ok(value);
                        }
                    }
                    Ok(Value::Nil)
                }
                _ => Err(mlua::Error::runtime(
                    "Usage: Frame:GetAttribute(name) or Frame:GetAttribute(prefix, name, suffix)",
                )),
            }
        })?,
    )
}

/// Coerces a native attribute-name argument using Lua 5.1 string semantics.
fn attribute_name(lua: &Lua, value: &Value) -> mlua::Result<String> {
    lua.coerce_string(value.clone())?
        .map(|value| value.to_string_lossy())
        .ok_or_else(|| mlua::Error::runtime("frame attribute name must be a string"))
}

/// Preserves the legacy `this` global while delivering one attribute change.
fn call_attribute_changed_handler(
    lua: &Lua,
    function: &mlua::Function,
    object: Table,
    name: String,
    value: Value,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let previous = globals.raw_get::<Value>("this")?;
    globals.raw_set("this", object.clone())?;
    let result = function.call::<()>((object, name, value));
    let restore = globals.raw_set("this", previous);
    match result {
        Ok(()) => restore,
        Err(error) => {
            let _restore_result = restore;
            Err(error)
        }
    }
}

fn register_frame_script_methods(
    lua: &Lua,
    methods: &Table,
    kind: UiObjectKind,
) -> mlua::Result<()> {
    methods.raw_set(
        "SetScript",
        lua.create_function(move |lua, (object, name, value): (Table, String, Value)| {
            let handler = handler_for(kind, &name)
                .ok_or_else(|| mlua::Error::runtime(format!("Unknown script handler: {name}")))?;
            if !matches!(value, Value::Nil | Value::Function(_)) {
                return Err(mlua::Error::runtime(format!(
                    "Usage: {}:SetScript(\"scriptType\", function)",
                    object_type_name(kind)
                )));
            }
            let handlers: Table = object.raw_get(script_handlers_key())?;
            let subscribed = !matches!(value, Value::Nil);
            handlers.raw_set(handler.name(), value)?;
            if handler == UiScriptHandler::Update {
                set_update_subscription(lua, &object, subscribed)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "GetScript",
        lua.create_function(move |lua, (object, name): (Table, String)| {
            let handler = handler_for(kind, &name)
                .ok_or_else(|| mlua::Error::runtime(format!("Unknown script handler: {name}")))?;
            object_script_function(lua, &object, handler)
        })?,
    )?;
    methods.raw_set(
        "HasScript",
        lua.create_function(move |_, (object, name): (Table, String)| {
            let handler = handler_for(kind, &name)
                .ok_or_else(|| mlua::Error::runtime(format!("Unknown script handler: {name}")))?;
            let handlers: Table = object.raw_get(script_handlers_key())?;
            Ok(!matches!(
                handlers.raw_get::<Value>(handler.name())?,
                Value::Nil
            ))
        })?,
    )
}

fn register_frame_region_factory_methods(
    lua: &Lua,
    methods: &Table,
    dynamic_arena: DynamicArenaState,
) -> mlua::Result<()> {
    let texture_arena = dynamic_arena.clone();
    methods.raw_set(
        "CreateTexture",
        lua.create_function(
            move |lua,
                  (parent, name, layer, template, sub_level): (
                Table,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<i32>,
            )| {
                create_dynamic_region(
                    lua,
                    "Texture",
                    parent,
                    name.as_deref(),
                    layer.as_deref(),
                    template.as_deref(),
                    sub_level,
                    &texture_arena.counters(),
                )
            },
        )?,
    )?;
    methods.raw_set(
        "CreateFontString",
        lua.create_function(
            move |lua,
                  (parent, name, layer, template, sub_level): (
                Table,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<i32>,
            )| {
                create_dynamic_region(
                    lua,
                    "FontString",
                    parent,
                    name.as_deref(),
                    layer.as_deref(),
                    template.as_deref(),
                    sub_level,
                    &dynamic_arena.counters(),
                )
            },
        )?,
    )
}

#[allow(clippy::too_many_arguments)]
fn create_dynamic_region(
    lua: &Lua,
    kind: &str,
    parent: Table,
    name: Option<&str>,
    layer: Option<&str>,
    template: Option<&str>,
    sub_level: Option<i32>,
    counters: &DynamicArenaCounters<'_>,
) -> mlua::Result<Table> {
    let inherited_font = if kind == "FontString" {
        template
            .and_then(|name| lua.globals().raw_get::<Option<Table>>(name).ok().flatten())
            .filter(|font| {
                font.raw_get::<String>(type_key())
                    .is_ok_and(|kind| kind == "Font")
            })
    } else {
        None
    };
    let descriptor = if inherited_font.is_some() {
        None
    } else {
        runtime_template_descriptor(lua, template)?
    };
    let object = create_dynamic_frame(lua, kind, name, Some(parent), descriptor, counters)?;
    if let Some(layer) = layer {
        let layer = parse_draw_layer_name(layer)
            .ok_or_else(|| mlua::Error::runtime("invalid draw layer"))?;
        object.raw_set(draw_layer_key(), layer)?;
    }
    if let Some(sub_level) = sub_level {
        object.raw_set(
            draw_sub_level_key(),
            sub_level.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
        )?;
    }
    if let Some(font) = inherited_font {
        let color: Table = font.raw_get(text_color_key())?;
        object.raw_set(font_object_key(), font)?;
        object.raw_set(text_color_key(), color)?;
        object.raw_set(font_set_key(), true)?;
    }
    Ok(object)
}

fn object_script_function(
    lua: &Lua,
    object: &Table,
    handler: UiScriptHandler,
) -> mlua::Result<Option<mlua::Function>> {
    let Some(handlers) = object.raw_get::<Option<Table>>(script_handlers_key())? else {
        return Ok(None);
    };
    match handlers.raw_get::<Value>(handler.name())? {
        Value::Nil => Ok(None),
        Value::Function(function) => Ok(Some(function)),
        Value::String(name) => {
            let name = name.to_str()?;
            match lua.globals().raw_get::<Value>(&name)? {
                Value::Nil => Ok(None),
                Value::Function(function) => Ok(Some(function)),
                _ => Err(mlua::Error::runtime(format!(
                    "global handler {name} is not a function"
                ))),
            }
        }
        _ => Err(mlua::Error::runtime("script handler slot is invalid")),
    }
}

fn object_has_script(object: &Table, handler: UiScriptHandler) -> mlua::Result<bool> {
    let Some(handlers) = object.raw_get::<Option<Table>>(script_handlers_key())? else {
        return Ok(false);
    };
    Ok(!matches!(
        handlers.raw_get::<Value>(handler.name())?,
        Value::Nil
    ))
}

/// Retains construction order while allowing `SetScript(..., nil)` to remove
/// a frame from the hot update path without compacting the sequence table.
fn set_update_subscription(lua: &Lua, object: &Table, subscribed: bool) -> mlua::Result<()> {
    let object_index = object.raw_get::<usize>(index_key())?;
    let members: Table = lua.named_registry_value(ON_UPDATE_MEMBERS_REGISTRY)?;
    let seen: Table = lua.named_registry_value(ON_UPDATE_SEEN_REGISTRY)?;
    if subscribed && !seen.raw_get::<bool>(object_index)? {
        let objects: Table = lua.named_registry_value(ON_UPDATE_OBJECTS_REGISTRY)?;
        objects.raw_set(objects.raw_len() + 1, object_index)?;
        seen.raw_set(object_index, true)?;
    }
    members.raw_set(object_index, subscribed)
}

/// Installs the runtime document replacement used by stock CreditsFrame.lua.
///
/// Build 12340 registers `SetText` at `0x00B2D1E0 -> 0x009750D0`;
/// `CSimpleHTML::SetText` at `0x0096D890` consumes a missing argument as an
/// empty document and marks native layout dirty.
fn register_simple_html_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetText",
        lua.create_function(|lua, (object, value): (Table, Value)| {
            let text = lua
                .coerce_string(value)?
                .map_or_else(String::new, |value| value.to_string_lossy());
            object.raw_set(text_key(), text)
        })?,
    )
}

fn register_font_string_methods(
    lua: &Lua,
    methods: &Table,
    measurement: Option<buttons::TextMeasurement>,
) -> mlua::Result<()> {
    let font_measurement = measurement.clone();
    methods.raw_set(
        "SetFontObject",
        lua.create_function(move |lua, (font_string, value): (Table, Value)| {
            let font = resolve_font_object(lua, value).ok_or_else(|| {
                mlua::Error::runtime("Usage: FontString:SetFontObject(fontObject)")
            })?;
            let color: Table = font.raw_get(text_color_key())?;
            let shadow_offset: Table = font.raw_get(font_shadow_offset_key())?;
            let shadow_color: Table = font.raw_get(font_shadow_color_key())?;
            font_string.raw_set(font_object_key(), font)?;
            font_string.raw_set(text_color_key(), color)?;
            font_string.raw_set(font_shadow_offset_key(), shadow_offset)?;
            font_string.raw_set(font_shadow_color_key(), shadow_color)?;
            font_string.raw_set(font_set_key(), true)?;
            if let Some(measurement) = &font_measurement {
                measurement.update_auto_font_string_size(&font_string)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "GetFontObject",
        lua.create_function(|_, font_string: Table| {
            font_string.raw_get::<Option<Table>>(font_object_key())
        })?,
    )?;
    methods.raw_set(
        "GetFont",
        lua.create_function(|_, font_string: Table| {
            let Some(font) = font_string.raw_get::<Option<Table>>(font_object_key())? else {
                return Ok((None::<String>, None::<f64>, None::<String>));
            };
            Ok((
                font.raw_get::<Option<String>>(font_face_key())?,
                font.raw_get::<Option<f64>>(font_height_key())?,
                font.raw_get::<Option<String>>(font_flags_key())?,
            ))
        })?,
    )?;
    let set_text_measurement = measurement.clone();
    methods.raw_set(
        "SetText",
        lua.create_function(move |lua, (font_string, value): (Table, Value)| {
            require_font_string_font(&font_string, "SetText")?;
            font_string.raw_set(text_key(), lua_text(lua, value)?)?;
            if let Some(measurement) = &set_text_measurement {
                measurement.update_auto_font_string_size(&font_string)?;
            }
            Ok(())
        })?,
    )?;
    let formatted_text_measurement = measurement.clone();
    methods.raw_set(
        "SetFormattedText",
        lua.create_function(
            move |lua, (font_string, arguments): (Table, Variadic<Value>)| {
                require_font_string_font(&font_string, "SetFormattedText")?;
                let library: Table = lua.globals().raw_get("string")?;
                let format: mlua::Function = library.raw_get("format")?;
                font_string.raw_set(text_key(), format.call::<String>(arguments)?)?;
                if let Some(measurement) = &formatted_text_measurement {
                    measurement.update_auto_font_string_size(&font_string)?;
                }
                Ok(())
            },
        )?,
    )?;
    methods.raw_set(
        "GetText",
        lua.create_function(|_, font_string: Table| {
            let text = font_string.raw_get::<Option<String>>(text_key())?;
            Ok(text.filter(|text| !text.is_empty()))
        })?,
    )?;
    methods.raw_set(
        "SetTextColor",
        lua.create_function(
            |lua, (font_string, red, green, blue, alpha): (Table, f64, f64, f64, Option<f64>)| {
                font_string.raw_set(
                    text_color_key(),
                    lua.create_sequence_from(clamped_color(red, green, blue, alpha))?,
                )
            },
        )?,
    )?;
    methods.raw_set(
        "GetTextColor",
        lua.create_function(|_, font_string: Table| {
            let color: Table = font_string.raw_get(text_color_key())?;
            Ok((
                color.raw_get::<f64>(1)?,
                color.raw_get::<f64>(2)?,
                color.raw_get::<f64>(3)?,
                color.raw_get::<f64>(4)?,
            ))
        })?,
    )?;
    let width_measurement = measurement.clone();
    methods.raw_set(
        "GetStringWidth",
        lua.create_function(move |_, font_string: Table| {
            let Some(measurement) = &width_measurement else {
                return Err(mlua::Error::runtime(
                    "FontString:GetStringWidth requires a mounted stock asset store",
                ));
            };
            measurement.font_string_width(&font_string)
        })?,
    )?;
    let height_measurement = measurement.clone();
    methods.raw_set(
        "GetStringHeight",
        lua.create_function(move |_, font_string: Table| {
            let Some(measurement) = &height_measurement else {
                return Err(mlua::Error::runtime(
                    "FontString:GetStringHeight requires a mounted stock asset store",
                ));
            };
            measurement.font_string_height(&font_string)
        })?,
    )?;
    let field_measurement = measurement.clone();
    methods.raw_set(
        "GetFieldSize",
        lua.create_function(move |_, font_string: Table| {
            let Some(measurement) = &field_measurement else {
                return Err(mlua::Error::runtime(
                    "FontString:GetFieldSize requires a mounted stock asset store",
                ));
            };
            measurement.font_string_dimensions(&font_string)
        })?,
    )?;
    let word_wrap_measurement = measurement.clone();
    methods.raw_set(
        "SetWordWrap",
        lua.create_function(move |_, (font_string, enabled): (Table, bool)| {
            font_string.raw_set(word_wrap_key(), enabled)?;
            if let Some(measurement) = &word_wrap_measurement {
                measurement.update_auto_font_string_size(&font_string)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "CanWordWrap",
        lua.create_function(|_, font_string: Table| font_string.raw_get::<bool>(word_wrap_key()))?,
    )?;
    let non_space_measurement = measurement;
    methods.raw_set(
        "SetNonSpaceWrap",
        lua.create_function(move |_, (font_string, enabled): (Table, bool)| {
            font_string.raw_set(non_space_wrap_key(), enabled)?;
            if let Some(measurement) = &non_space_measurement {
                measurement.update_auto_font_string_size(&font_string)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "CanNonSpaceWrap",
        lua.create_function(|_, font_string: Table| {
            font_string.raw_get::<bool>(non_space_wrap_key())
        })?,
    )?;
    register_font_string_justification_methods(lua, methods)?;
    register_font_spacing_methods(lua, methods)?;
    register_font_shadow_methods(lua, methods)
}

fn register_font_spacing_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetSpacing",
        lua.create_function(|_, (object, spacing): (Table, f64)| {
            if !spacing.is_finite() {
                return Err(mlua::Error::runtime("SetSpacing(): spacing must be finite"));
            }
            object.raw_set(spacing_key(), spacing)
        })?,
    )?;
    methods.raw_set(
        "GetSpacing",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(spacing_key()))?,
    )
}

fn register_font_shadow_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetShadowOffset",
        lua.create_function(|lua, (object, x, y): (Table, f64, f64)| {
            object.raw_set(font_shadow_offset_key(), lua.create_sequence_from([x, y])?)
        })?,
    )?;
    methods.raw_set(
        "GetShadowOffset",
        lua.create_function(|_, object: Table| {
            let offset: Table = object.raw_get(font_shadow_offset_key())?;
            Ok((offset.raw_get::<f64>(1)?, offset.raw_get::<f64>(2)?))
        })?,
    )?;
    methods.raw_set(
        "SetShadowColor",
        lua.create_function(
            |lua, (object, red, green, blue, alpha): (Table, f64, f64, f64, Option<f64>)| {
                object.raw_set(
                    font_shadow_color_key(),
                    lua.create_sequence_from(clamped_color(red, green, blue, alpha))?,
                )
            },
        )?,
    )?;
    methods.raw_set(
        "GetShadowColor",
        lua.create_function(|_, object: Table| {
            let color: Table = object.raw_get(font_shadow_color_key())?;
            Ok((
                color.raw_get::<f64>(1)?,
                color.raw_get::<f64>(2)?,
                color.raw_get::<f64>(3)?,
                color.raw_get::<f64>(4)?,
            ))
        })?,
    )
}

fn register_font_string_justification_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetJustifyH",
        lua.create_function(|_, (font_string, value): (Table, String)| {
            let value = stock_justify(&value).ok_or_else(|| {
                mlua::Error::runtime("Usage: FontString:SetJustifyH(\"justify\")")
            })?;
            font_string.raw_set(justify_h_key(), value)
        })?,
    )?;
    methods.raw_set(
        "GetJustifyH",
        lua.create_function(|_, font_string: Table| {
            font_string.raw_get::<String>(justify_h_key())
        })?,
    )?;
    methods.raw_set(
        "SetJustifyV",
        lua.create_function(|_, (font_string, value): (Table, String)| {
            let value = stock_justify(&value).ok_or_else(|| {
                mlua::Error::runtime("Usage: FontString:SetJustifyV(\"justify\")")
            })?;
            font_string.raw_set(justify_v_key(), value)
        })?,
    )?;
    methods.raw_set(
        "GetJustifyV",
        lua.create_function(|_, font_string: Table| {
            font_string.raw_get::<String>(justify_v_key())
        })?,
    )
}

fn stock_justify(value: &str) -> Option<&'static str> {
    if value.eq_ignore_ascii_case("LEFT") {
        Some("LEFT")
    } else if value.eq_ignore_ascii_case("CENTER") {
        Some("CENTER")
    } else if value.eq_ignore_ascii_case("RIGHT") {
        Some("RIGHT")
    } else if value.eq_ignore_ascii_case("TOP") {
        Some("TOP")
    } else if value.eq_ignore_ascii_case("MIDDLE") {
        Some("MIDDLE")
    } else if value.eq_ignore_ascii_case("BOTTOM") {
        Some("BOTTOM")
    } else {
        None
    }
}

fn require_font_string_font(font_string: &Table, method: &str) -> mlua::Result<()> {
    if font_string.raw_get::<bool>(font_set_key())? {
        return Ok(());
    }
    let name = font_string
        .raw_get::<Option<String>>(name_key())?
        .unwrap_or_else(|| "<unnamed>".to_owned());
    Err(mlua::Error::runtime(format!(
        "{name}:{method}(): Font not set"
    )))
}

fn register_texture_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetTexture",
        lua.create_function(|lua, (texture, arguments): (Table, Variadic<Value>)| {
            let first = arguments.first().cloned().unwrap_or(Value::Nil);
            if let Some(red) = lua.coerce_number(first.clone())? {
                let mut color = [red, 1.0, 1.0, 1.0];
                for (index, component) in color.iter_mut().enumerate().skip(1) {
                    if let Some(value) = arguments
                        .get(index)
                        .map(|value| lua.coerce_number(value.clone()))
                        .transpose()?
                        .flatten()
                    {
                        *component = value;
                    }
                }
                if color.iter().any(|value| !value.is_finite()) {
                    return Err(mlua::Error::runtime("SetTexture(): invalid color"));
                }
                texture.raw_set(texture_file_key(), Option::<String>::None)?;
                texture.raw_set(portrait_unit_key(), Option::<String>::None)?;
                texture.raw_set(texture_solid_color_key(), lua.create_sequence_from(color)?)?;
                return Ok(());
            }
            let value = lua
                .coerce_string(first)?
                .map(|value| value.to_string_lossy())
                .unwrap_or_default();
            let file = (!value.is_empty()).then_some(value);
            texture.raw_set(texture_file_key(), file)?;
            texture.raw_set(portrait_unit_key(), Option::<String>::None)?;
            texture.raw_set(texture_solid_color_key(), Option::<Table>::None)
        })?,
    )?;
    methods.raw_set(
        "GetTexture",
        lua.create_function(|_, texture: Table| {
            texture.raw_get::<Option<String>>(texture_file_key())
        })?,
    )?;
    methods.raw_set(
        "SetBlendMode",
        lua.create_function(|_, (texture, requested): (Table, String)| {
            let mode = if requested.eq_ignore_ascii_case("BLEND") {
                "BLEND"
            } else if requested.eq_ignore_ascii_case("ADD") {
                "ADD"
            } else {
                return Err(mlua::Error::runtime("invalid texture blend mode"));
            };
            texture.raw_set(texture_blend_mode_key(), mode)
        })?,
    )?;
    methods.raw_set(
        "GetBlendMode",
        lua.create_function(|_, texture: Table| {
            texture.raw_get::<String>(texture_blend_mode_key())
        })?,
    )?;
    register_texture_flag_methods(lua, methods)?;
    methods.raw_set(
        "SetTexCoord",
        lua.create_function(|lua, (texture, arguments): (Table, Variadic<Value>)| {
            let values = arguments
                .iter()
                .map(|value| lua_number(value).unwrap_or(0.0))
                .collect::<Vec<_>>();
            let coords = match values.as_slice() {
                [left, right, top, bottom] => [
                    *left, *top, *left, *bottom, *right, *top, *right, *bottom,
                ],
                [ul_x, ul_y, ll_x, ll_y, ur_x, ur_y, lr_x, lr_y] => {
                    [*ul_x, *ul_y, *ll_x, *ll_y, *ur_x, *ur_y, *lr_x, *lr_y]
                }
                _ => {
                    return Err(mlua::Error::runtime(
                        "Usage: Texture:SetTexCoord(minX, maxX, minY, maxY) or SetTexCoord(ULx, ULy, LLx, LLy, URx, URy, LRx, LRy)",
                    ));
                }
            };
            if coords
                .iter()
                .any(|value| *value < -10_000.0 || *value > 10_000.0)
            {
                return Err(mlua::Error::runtime("TexCoord out of range"));
            }
            texture.raw_set(tex_coord_key(), lua.create_sequence_from(coords)?)
        })?,
    )?;
    methods.raw_set(
        "GetTexCoord",
        lua.create_function(|_, texture: Table| {
            let coords: Table = texture.raw_get(tex_coord_key())?;
            coords
                .sequence_values::<f64>()
                .map(|value| value.map(Value::Number))
                .collect::<mlua::Result<Variadic<Value>>>()
        })?,
    )?;
    methods.raw_set(
        "SetVertexColor",
        lua.create_function(|lua, (texture, arguments): (Table, Variadic<Value>)| {
            let mut color = [0.0, 0.0, 0.0, 1.0];
            for (index, component) in color.iter_mut().take(3).enumerate() {
                *component = arguments
                    .get(index)
                    .map(|value| lua.coerce_number(value.clone()))
                    .transpose()?
                    .flatten()
                    .unwrap_or(0.0);
            }
            color[3] = arguments
                .get(3)
                .map(|value| lua.coerce_number(value.clone()))
                .transpose()?
                .flatten()
                .unwrap_or(1.0);
            if color.iter().any(|value| !value.is_finite()) {
                return Err(mlua::Error::runtime("invalid vertex color"));
            }
            texture.raw_set(
                texture_color_key(),
                lua.create_sequence_from(color.into_iter().cycle().take(16))?,
            )
        })?,
    )?;
    methods.raw_set(
        "GetVertexColor",
        lua.create_function(|_, texture: Table| {
            let color: Table = texture.raw_get(texture_color_key())?;
            Ok((
                color.raw_get::<f64>(1)?,
                color.raw_get::<f64>(2)?,
                color.raw_get::<f64>(3)?,
                color.raw_get::<f64>(4)?,
            ))
        })?,
    )?;
    methods.raw_set(
        "SetGradient",
        lua.create_function(|lua, (texture, arguments): (Table, Variadic<Value>)| {
            set_texture_gradient(lua, &texture, arguments.as_slice(), false)
        })?,
    )?;
    methods.raw_set(
        "SetGradientAlpha",
        lua.create_function(|lua, (texture, arguments): (Table, Variadic<Value>)| {
            set_texture_gradient(lua, &texture, arguments.as_slice(), true)
        })?,
    )
}

fn register_texture_flag_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    for (set_name, get_name, key) in [
        ("SetHorizTile", "GetHorizTile", horizontal_tiling_key()),
        ("SetVertTile", "GetVertTile", vertical_tiling_key()),
        ("SetNonBlocking", "GetNonBlocking", non_blocking_key()),
    ] {
        methods.raw_set(
            set_name,
            lua.create_function(move |_, (texture, enabled): (Table, Option<bool>)| {
                texture.raw_set(key, enabled.unwrap_or(true))
            })?,
        )?;
        methods.raw_set(
            get_name,
            lua.create_function(move |_, texture: Table| {
                Ok(texture.raw_get::<bool>(key)?.then_some(Value::Number(1.0)))
            })?,
        )?;
    }
    methods.raw_set(
        "SetDesaturated",
        lua.create_function(|_, (texture, arguments): (Table, Variadic<Value>)| {
            // Stock treats an omitted value as true, but an explicit nil as false.
            // CharacterCreate.lua relies on that distinction when it re-enables
            // race and class buttons, and on the numeric success result.
            let enabled = match arguments.first() {
                None => true,
                Some(Value::Nil | Value::Boolean(false)) => false,
                Some(_) => true,
            };
            texture.raw_set(desaturated_key(), enabled)?;
            Ok(1.0)
        })?,
    )?;
    methods.raw_set(
        "IsDesaturated",
        lua.create_function(|_, texture: Table| {
            Ok(texture
                .raw_get::<bool>(desaturated_key())?
                .then_some(Value::Number(1.0)))
        })?,
    )?;
    Ok(())
}

fn set_texture_gradient(
    lua: &Lua,
    texture: &Table,
    arguments: &[Value],
    includes_alpha: bool,
) -> mlua::Result<()> {
    let orientation = arguments
        .first()
        .and_then(|value| value.as_string())
        .map(|value| value.to_string_lossy())
        .ok_or_else(|| mlua::Error::runtime("invalid Texture gradient"))?;
    let stride = if includes_alpha { 4 } else { 3 };
    if arguments.len() != 1 + stride * 2 {
        return Err(mlua::Error::runtime("invalid Texture gradient"));
    }
    let mut minimum = [0.0, 0.0, 0.0, 1.0];
    let mut maximum = [0.0, 0.0, 0.0, 1.0];
    for channel in 0..stride {
        minimum[channel] = lua
            .coerce_number(arguments[1 + channel].clone())?
            .ok_or_else(|| mlua::Error::runtime("invalid Texture gradient"))?
            .clamp(0.0, 1.0);
        maximum[channel] = lua
            .coerce_number(arguments[1 + stride + channel].clone())?
            .ok_or_else(|| mlua::Error::runtime("invalid Texture gradient"))?
            .clamp(0.0, 1.0);
    }
    let colors = if orientation.eq_ignore_ascii_case("VERTICAL") {
        [maximum, minimum, maximum, minimum]
    } else if orientation.eq_ignore_ascii_case("HORIZONTAL") {
        [minimum, minimum, maximum, maximum]
    } else {
        return Err(mlua::Error::runtime("invalid Texture gradient"));
    };
    texture.raw_set(
        texture_color_key(),
        lua.create_sequence_from(colors.into_iter().flatten())?,
    )
}

fn initialize_model_runtime_state(lua: &Lua, model: &Table) -> mlua::Result<()> {
    model.raw_set(model_camera_key(), 0)?;
    model.raw_set(model_sequence_key(), 0_u32)?;
    model.raw_set(model_sequence_time_sequence_key(), 0_u32)?;
    model.raw_set(model_sequence_time_key(), 0_i32)?;
    model.raw_set(model_scale_key(), 1.0)?;
    model.raw_set(model_fog_color_key(), Option::<Table>::None)?;
    model.raw_set(model_fog_near_key(), 0.0)?;
    model.raw_set(model_fog_far_key(), 0.0)?;
    model.raw_set(model_glow_key(), 0.0)?;
    reset_model_lights(model)?;
    // Keep `lua` explicit here: every initialized object belongs to this exact
    // runtime, and model state must never be shared across Lua instances.
    let _ = lua;
    Ok(())
}

fn register_model_methods(
    lua: &Lua,
    methods: &Table,
    assets: Option<AssetStoreHandle>,
) -> mlua::Result<()> {
    methods.raw_set(
        "SetModel",
        lua.create_function(move |lua, (model, value): (Table, Value)| {
            let Some(value) = lua.coerce_string(value)? else {
                return Err(mlua::Error::runtime("Usage: Model:SetModel(\"file\")"));
            };
            let display = value.to_string_lossy();
            let path = AssetPath::new(&display)
                .map_err(|error| mlua::Error::runtime(format!("Invalid model file: {error}")))?;
            let Some(assets) = &assets else {
                return Err(mlua::Error::runtime(
                    "Model:SetModel requires a mounted asset store",
                ));
            };
            assets
                .borrow_mut()
                .read(&path)
                .map_err(|_| mlua::Error::runtime(format!("Invalid model file: {display}")))?;
            model.raw_set(model_file_key(), path.as_str())
        })?,
    )?;
    methods.raw_set(
        "GetModel",
        lua.create_function(|_, model: Table| model.raw_get::<Option<String>>(model_file_key()))?,
    )?;
    methods.raw_set(
        "SetCamera",
        lua.create_function(|lua, (model, value): (Table, Value)| {
            let value = lua
                .coerce_number(value)?
                .ok_or_else(|| mlua::Error::runtime("Usage: Model:SetCamera(index)"))?;
            model.raw_set(model_camera_key(), value as i32)
        })?,
    )?;
    methods.raw_set(
        "SetSequence",
        lua.create_function(|lua, (model, value): (Table, Value)| {
            let value = lua
                .coerce_number(value)?
                .ok_or_else(|| mlua::Error::runtime("Usage: Model:SetSequence(sequence)"))?;
            if !(0.0..506.0).contains(&value) {
                return Err(mlua::Error::runtime(
                    "SetSequence(sequence) exceeds valid range of 0 - 506",
                ));
            }
            model.raw_set(model_sequence_key(), value as u32)
        })?,
    )?;
    methods.raw_set(
        "SetSequenceTime",
        lua.create_function(|lua, (model, sequence, time): (Table, Value, Value)| {
            let sequence = lua.coerce_number(sequence)?.ok_or_else(|| {
                mlua::Error::runtime("Usage: Model:SetSequenceTime(sequence, time)")
            })?;
            let time = lua.coerce_number(time)?.ok_or_else(|| {
                mlua::Error::runtime("Usage: Model:SetSequenceTime(sequence, time)")
            })?;
            model.raw_set(model_sequence_time_sequence_key(), sequence as u32)?;
            model.raw_set(model_sequence_time_key(), time as i32)
        })?,
    )?;
    methods.raw_set(
        "AdvanceTime",
        lua.create_function(|_, _model: Table| {
            // WoW 3.3.5a's Model:AdvanceTime wrapper reaches a native method
            // which returns true without accepting a Lua elapsed-time value.
            // Animation time is advanced by the renderer's frame clock.
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "SetModelScale",
        lua.create_function(|lua, (model, value): (Table, Value)| {
            let value = lua
                .coerce_number(value)?
                .ok_or_else(|| mlua::Error::runtime("Usage: Model:SetModelScale(scale)"))?;
            model.raw_set(model_scale_key(), value)
        })?,
    )?;
    methods.raw_set(
        "SetFogColor",
        lua.create_function(
            |lua, (model, red, green, blue): (Table, Value, Value, Value)| {
                let color = [
                    finite_model_number(lua, red, "fog color")?,
                    finite_model_number(lua, green, "fog color")?,
                    finite_model_number(lua, blue, "fog color")?,
                ];
                model.raw_set(
                    model_fog_color_key(),
                    lua.create_sequence_from(color.into_iter().map(|value| value.clamp(0.0, 1.0)))?,
                )
            },
        )?,
    )?;
    methods.raw_set(
        "SetFogNear",
        lua.create_function(|lua, (model, value): (Table, Value)| {
            model.raw_set(
                model_fog_near_key(),
                finite_model_number(lua, value, "fog near")?,
            )
        })?,
    )?;
    methods.raw_set(
        "SetFogFar",
        lua.create_function(|lua, (model, value): (Table, Value)| {
            model.raw_set(
                model_fog_far_key(),
                finite_model_number(lua, value, "fog far")?,
            )
        })?,
    )?;
    methods.raw_set(
        "ClearFog",
        lua.create_function(|_, model: Table| {
            model.raw_set(model_fog_color_key(), Option::<Table>::None)
        })?,
    )?;
    methods.raw_set(
        "SetGlow",
        lua.create_function(|lua, (model, value): (Table, Value)| {
            model.raw_set(
                model_glow_key(),
                finite_model_number(lua, value, "model glow")?,
            )
        })?,
    )?;
    methods.raw_set(
        "ResetLights",
        lua.create_function(|_, model: Table| reset_model_lights(&model))?,
    )?;
    register_model_light_method(
        lua,
        methods,
        "AddLight",
        model_background_light_live_key(),
        model_background_light_ghost_key(),
    )?;
    register_model_light_method(
        lua,
        methods,
        "AddCharacterLight",
        model_character_light_live_key(),
        model_character_light_ghost_key(),
    )?;
    register_model_light_method(
        lua,
        methods,
        "AddPetLight",
        model_pet_light_live_key(),
        model_pet_light_ghost_key(),
    )
}

fn finite_model_number(lua: &Lua, value: Value, label: &str) -> mlua::Result<f64> {
    let value = lua
        .coerce_number(value)?
        .ok_or_else(|| mlua::Error::runtime(format!("invalid {label}")))?;
    if !value.is_finite() {
        return Err(mlua::Error::runtime(format!("non-finite {label}")));
    }
    Ok(value)
}

fn reset_model_lights(model: &Table) -> mlua::Result<()> {
    for key in [
        model_background_light_live_key(),
        model_background_light_ghost_key(),
        model_character_light_live_key(),
        model_character_light_ghost_key(),
        model_pet_light_live_key(),
        model_pet_light_ghost_key(),
    ] {
        model.raw_set(key, Option::<Table>::None)?;
    }
    Ok(())
}

fn register_model_light_method(
    lua: &Lua,
    methods: &Table,
    function: &'static str,
    live_key: LightUserData,
    ghost_key: LightUserData,
) -> mlua::Result<()> {
    methods.raw_set(
        function,
        lua.create_function(move |lua, (model, arguments): (Table, Variadic<Value>)| {
            if arguments.len() != 14 {
                return Err(mlua::Error::runtime(format!(
                    "Usage: ModelFFX:{function}(set, enabled, omni, x, y, z, ambientIntensity, ambientR, ambientG, ambientB, diffuseIntensity, diffuseR, diffuseG, diffuseB)"
                )));
            }
            let [set, enabled, omnidirectional, direction_x, direction_y, direction_z, ambient_intensity, ambient_red, ambient_green, ambient_blue, diffuse_intensity, diffuse_red, diffuse_green, diffuse_blue] =
                arguments.as_slice()
            else {
                unreachable!("light argument count was validated");
            };
            let values = [
                finite_model_number(lua, set.clone(), "model light value")?,
                finite_model_number(lua, enabled.clone(), "model light value")?,
                finite_model_number(lua, omnidirectional.clone(), "model light value")?,
                finite_model_number(lua, direction_x.clone(), "model light value")?,
                finite_model_number(lua, direction_y.clone(), "model light value")?,
                finite_model_number(lua, direction_z.clone(), "model light value")?,
                finite_model_number(lua, ambient_intensity.clone(), "model light value")?,
                finite_model_number(lua, ambient_red.clone(), "model light value")?,
                finite_model_number(lua, ambient_green.clone(), "model light value")?,
                finite_model_number(lua, ambient_blue.clone(), "model light value")?,
                finite_model_number(lua, diffuse_intensity.clone(), "model light value")?,
                finite_model_number(lua, diffuse_red.clone(), "model light value")?,
                finite_model_number(lua, diffuse_green.clone(), "model light value")?,
                finite_model_number(lua, diffuse_blue.clone(), "model light value")?,
            ];
            let key = match values[0] as i32 {
                0 if values[0] == 0.0 => live_key,
                1 if values[0] == 1.0 => ghost_key,
                _ => return Err(mlua::Error::runtime("invalid ModelFFX light set")),
            };
            if values[2] != 0.0 {
                return Err(mlua::Error::runtime(
                    "build 12340 ModelFFX only supports directional lights",
                ));
            }
            let lights = model.raw_get::<Option<Table>>(key)?.unwrap_or(lua.create_table()?);
            let count = lights.raw_len();
            if count >= 4 {
                return Err(mlua::Error::runtime(
                    "ModelFFX light set exceeds the stock four-light bound",
                ));
            }
            let enabled = values[1] != 0.0;
            let enabled_scale = if enabled { 1.0 } else { 0.0 };
            let ambient_intensity = values[6];
            let diffuse_intensity = values[10];
            let light = lua.create_sequence_from([
                values[3] * enabled_scale,
                values[4] * enabled_scale,
                values[5] * enabled_scale,
                values[7] * ambient_intensity * enabled_scale,
                values[8] * ambient_intensity * enabled_scale,
                values[9] * ambient_intensity * enabled_scale,
                values[11] * diffuse_intensity * enabled_scale,
                values[12] * diffuse_intensity * enabled_scale,
                values[13] * diffuse_intensity * enabled_scale,
            ])?;
            lights.raw_set(count + 1, light)?;
            model.raw_set(key, lights)
        })?,
    )
}

fn register_movie_frame_methods(
    lua: &Lua,
    methods: &Table,
    assets: Option<AssetStoreHandle>,
    media_intent: Rc<RefCell<UiGlueMediaIntent>>,
) -> mlua::Result<()> {
    let start_intent = media_intent.clone();
    methods.raw_set(
        "StartMovie",
        lua.create_function(
            move |lua, (movie_frame, value, volume): (Table, Value, Option<Value>)| {
                let Some(value) = lua.coerce_string(value)? else {
                    return Err(mlua::Error::runtime(
                        "Usage: MovieFrame:StartMovie(\"movie\" [, volume])",
                    ));
                };
                let display = value.to_string_lossy();
                let identity = AssetPath::new(&display).map_err(|error| {
                    mlua::Error::runtime(format!("Invalid movie file: {error}"))
                })?;
                let Some(assets) = &assets else {
                    return Err(mlua::Error::runtime(
                        "MovieFrame:StartMovie requires a mounted asset store",
                    ));
                };
                let path = match assets.borrow().cinematic_file_path(&identity) {
                    Ok(path) => path,
                    Err(solarity_asset::AssetError::AssetNotFound { .. }) => return Ok(false),
                    Err(error) => {
                        return Err(mlua::Error::runtime(format!(
                            "Invalid movie file {display}: {error}"
                        )));
                    }
                };
                let volume = volume
                    .map(|value| lua.coerce_number(value))
                    .transpose()?
                    .flatten()
                    .unwrap_or(100.0);
                if !volume.is_finite() || volume < 0.0 || volume > f64::from(u32::MAX) {
                    return Err(mlua::Error::runtime("invalid movie volume"));
                }
                let mut intent = start_intent.borrow_mut();
                let generation = intent.next_movie_generation.checked_add(1).ok_or_else(|| {
                    mlua::Error::runtime("Glue movie playback generation capacity exceeded")
                })?;
                intent.next_movie_generation = generation;
                intent.movie = Some(UiGlueMovieRequest {
                    object_index: movie_frame.raw_get::<usize>(index_key())? - 1,
                    generation,
                    path,
                    volume: volume as u32,
                    subtitles_enabled: movie_frame
                        .raw_get::<Option<bool>>(movie_subtitles_key())?
                        .unwrap_or(false),
                });
                Ok(true)
            },
        )?,
    )?;
    let stop_intent = media_intent;
    methods.raw_set(
        "StopMovie",
        lua.create_function(move |_, movie_frame: Table| {
            let object_index = movie_frame.raw_get::<usize>(index_key())? - 1;
            let mut intent = stop_intent.borrow_mut();
            if intent
                .movie
                .as_ref()
                .is_some_and(|movie| movie.object_index == object_index)
            {
                intent.movie = None;
                intent.movie_stop_completion = Some(object_index);
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "EnableSubtitles",
        lua.create_function(|_, (movie_frame, enabled): (Table, Option<bool>)| {
            movie_frame.raw_set(movie_subtitles_key(), enabled.unwrap_or(true))
        })?,
    )
}

fn resolve_font_object(lua: &Lua, value: Value) -> Option<Table> {
    let table = match value {
        Value::String(name) => lua.globals().raw_get::<Table>(name.to_str().ok()?).ok()?,
        Value::Table(table) => table,
        _ => return None,
    };
    let kind = table.raw_get::<String>(type_key()).ok()?;
    kind.eq_ignore_ascii_case("Font").then_some(table)
}

fn lua_text(lua: &Lua, value: Value) -> mlua::Result<Option<String>> {
    lua.coerce_string(value)
        .map(|value| value.map(|value| value.to_string_lossy()))
}

fn lua_bool(value: &Value, default: bool) -> bool {
    match value {
        Value::Nil => false,
        Value::Boolean(value) => *value,
        Value::Integer(value) => *value != 0,
        Value::Number(value) => *value != 0.0,
        Value::String(value) => {
            let value = value.to_string_lossy();
            let Some(first) = value.as_bytes().first().copied() else {
                return default;
            };
            match first {
                b'0' | b'F' | b'N' | b'f' | b'n' => false,
                b'1'..=b'9' | b'T' | b'Y' | b't' | b'y' => true,
                _ if value.eq_ignore_ascii_case("off")
                    || value.eq_ignore_ascii_case("disabled") =>
                {
                    false
                }
                _ if value.eq_ignore_ascii_case("on") || value.eq_ignore_ascii_case("enabled") => {
                    true
                }
                _ => default,
            }
        }
        _ => default,
    }
}

fn register_frame_visibility_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "GetID",
        lua.create_function(|_, object: Table| object.raw_get::<i32>(id_key()))?,
    )?;
    methods.raw_set(
        "SetID",
        lua.create_function(|_, (object, id): (Table, i32)| object.raw_set(id_key(), id))?,
    )?;
    methods.raw_set(
        "GetFrameLevel",
        lua.create_function(|_, object: Table| object.raw_get::<i32>(frame_level_key()))?,
    )?;
    methods.raw_set(
        "SetFrameLevel",
        lua.create_function(|lua, (object, value): (Table, Value)| {
            let value = lua
                .coerce_number(value)?
                .ok_or_else(|| mlua::Error::runtime("Usage: Frame:SetFrameLevel(level)"))?
                as i32;
            if value < 0 {
                return Err(mlua::Error::runtime(format!(
                    "Frame:SetFrameLevel(): Passed negative frame level: {value}"
                )));
            }
            set_frame_level(lua, &object, value, true)
        })?,
    )?;
    methods.raw_set(
        "Raise",
        lua.create_function(|lua, object: Table| raise_frame(lua, &object))?,
    )?;
    methods.raw_set(
        "GetFrameStrata",
        lua.create_function(|_, object: Table| object.raw_get::<String>(frame_strata_key()))?,
    )?;
    methods.raw_set(
        "SetFrameStrata",
        lua.create_function(|_, (object, requested): (Table, String)| {
            let strata = parse_frame_strata_name(&requested)
                .ok_or_else(|| mlua::Error::runtime("invalid frame strata"))?;
            object.raw_set(frame_strata_key(), strata)
        })?,
    )?;
    methods.raw_set(
        "EnableKeyboard",
        lua.create_function(|_, (object, arguments): (Table, Variadic<Value>)| {
            let enabled = arguments.first().is_none_or(|value| lua_bool(value, true));
            object.raw_set(keyboard_enabled_key(), enabled)
        })?,
    )?;
    methods.raw_set(
        "IsKeyboardEnabled",
        lua.create_function(|_, object: Table| {
            Ok(object
                .raw_get::<bool>(keyboard_enabled_key())?
                .then_some(Value::Number(1.0)))
        })?,
    )?;
    methods.raw_set(
        "EnableMouse",
        lua.create_function(|_, (object, arguments): (Table, Variadic<Value>)| {
            let enabled = arguments
                .first()
                .is_some_and(|value| lua_bool(value, false));
            object.raw_set(mouse_enabled_key(), enabled)
        })?,
    )?;
    methods.raw_set(
        "IsMouseEnabled",
        lua.create_function(|_, object: Table| {
            Ok(object
                .raw_get::<bool>(mouse_enabled_key())?
                .then_some(Value::Number(1.0)))
        })?,
    )?;
    methods.raw_set(
        "EnableMouseWheel",
        lua.create_function(|_, (object, arguments): (Table, Variadic<Value>)| {
            let enabled = arguments
                .first()
                .is_some_and(|value| lua_bool(value, false));
            object.raw_set(mouse_wheel_enabled_key(), enabled)
        })?,
    )?;
    methods.raw_set(
        "IsMouseWheelEnabled",
        lua.create_function(|_, object: Table| {
            Ok(object
                .raw_get::<bool>(mouse_wheel_enabled_key())?
                .then_some(Value::Number(1.0)))
        })?,
    )?;
    for (setter, getter, key) in [
        ("SetMovable", "IsMovable", frame_movable_key()),
        ("SetResizable", "IsResizable", frame_resizable_key()),
        ("SetToplevel", "IsToplevel", frame_top_level_key()),
        ("SetUserPlaced", "IsUserPlaced", frame_user_placed_key()),
        (
            "SetDontSavePosition",
            "GetDontSavePosition",
            frame_dont_save_position_key(),
        ),
    ] {
        methods.raw_set(
            setter,
            lua.create_function(move |_, (object, arguments): (Table, Variadic<Value>)| {
                let enabled = arguments.first().is_none_or(|value| lua_bool(value, true));
                object.raw_set(key, enabled)
            })?,
        )?;
        methods.raw_set(
            getter,
            lua.create_function(move |_, object: Table| {
                stock_optional_true(object.raw_get::<bool>(key)?)
            })?,
        )?;
    }
    methods.raw_set(
        "SetClampedToScreen",
        lua.create_function(|_, (object, enabled): (Table, Option<bool>)| {
            object.raw_set(frame_clamped_key(), enabled.unwrap_or(true))
        })?,
    )?;
    methods.raw_set(
        "IsClampedToScreen",
        lua.create_function(|_, object: Table| {
            stock_optional_true(object.raw_get::<bool>(frame_clamped_key())?)
        })?,
    )?;
    methods.raw_set(
        "SetClampRectInsets",
        lua.create_function(
            |lua, (object, left, right, top, bottom): (Table, f64, f64, f64, f64)| {
                object.raw_set(
                    frame_clamp_insets_key(),
                    lua.create_sequence_from([left, right, top, bottom])?,
                )
            },
        )?,
    )?;
    methods.raw_set(
        "GetClampRectInsets",
        lua.create_function(|_, object: Table| {
            let insets: Table = object.raw_get(frame_clamp_insets_key())?;
            Ok((
                insets.raw_get::<f64>(1)?,
                insets.raw_get::<f64>(2)?,
                insets.raw_get::<f64>(3)?,
                insets.raw_get::<f64>(4)?,
            ))
        })?,
    )?;
    methods.raw_set(
        "SetHitRectInsets",
        lua.create_function(
            |lua, (object, left, right, top, bottom): (Table, f64, f64, f64, f64)| {
                object.raw_set(
                    hit_rect_insets_key(),
                    lua.create_sequence_from([left, right, top, bottom])?,
                )
            },
        )?,
    )?;
    methods.raw_set(
        "GetHitRectInsets",
        lua.create_function(|_, object: Table| {
            let insets: Table = object.raw_get(hit_rect_insets_key())?;
            Ok((
                insets.raw_get::<f64>(1)?,
                insets.raw_get::<f64>(2)?,
                insets.raw_get::<f64>(3)?,
                insets.raw_get::<f64>(4)?,
            ))
        })?,
    )?;
    methods.raw_set(
        "SetDepth",
        lua.create_function(|_, (object, depth): (Table, f64)| {
            if !depth.is_finite() {
                return Err(mlua::Error::runtime("SetDepth(): invalid additive depth"));
            }
            object.raw_set(frame_depth_key(), depth)
        })?,
    )?;
    methods.raw_set(
        "GetDepth",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(frame_depth_key()))?,
    )?;
    methods.raw_set(
        "GetEffectiveDepth",
        lua.create_function(|lua, object: Table| effective_frame_depth(lua, object))?,
    )?;
    methods.raw_set(
        "IgnoreDepth",
        lua.create_function(|_, (object, ignored): (Table, bool)| {
            object.raw_set(ignore_depth_key(), ignored)
        })?,
    )?;
    methods.raw_set(
        "IsIgnoringDepth",
        lua.create_function(|_, object: Table| {
            Ok(object
                .raw_get::<bool>(ignore_depth_key())?
                .then_some(Value::Number(1.0)))
        })?,
    )
}

/// Raises the stock top-level owner and preserves its descendants' level offsets.
fn raise_frame(lua: &Lua, object: &Table) -> mlua::Result<()> {
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let mut raised = object.clone();
    loop {
        if raised.raw_get::<bool>(frame_top_level_key())? {
            break;
        }
        let Some(parent_index) = raised.raw_get::<Option<usize>>(parent_key())? else {
            return Ok(());
        };
        let Some(parent) = objects.raw_get::<Option<Table>>(parent_index)? else {
            return Ok(());
        };
        raised = parent;
    }

    let strata = raised.raw_get::<String>(frame_strata_key())?;
    let mut top_level = 0_i32;
    for index in 1..=objects.raw_len() {
        let Some(candidate) = objects.raw_get::<Option<Table>>(index)? else {
            continue;
        };
        if candidate
            .raw_get::<Option<String>>(frame_strata_key())?
            .is_some_and(|candidate_strata| candidate_strata == strata)
        {
            top_level = top_level.max(
                candidate
                    .raw_get::<Option<i32>>(frame_level_key())?
                    .unwrap_or(0)
                    .saturating_add(1),
            );
        }
    }
    set_frame_level(lua, &raised, top_level, true)
}

/// Applies stock's bounded level delta and optionally shifts the whole child tree.
fn set_frame_level(
    lua: &Lua,
    object: &Table,
    requested_level: i32,
    shift_children: bool,
) -> mlua::Result<()> {
    let old_level = object.raw_get::<i32>(frame_level_key())?;
    let delta = requested_level.max(0).saturating_sub(old_level).min(128);
    if delta == 0 {
        return Ok(());
    }

    object.raw_set(frame_level_key(), old_level.saturating_add(delta))?;
    if !shift_children {
        return Ok(());
    }

    let root_index = object.raw_get::<usize>(index_key())?;
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    for index in 1..=objects.raw_len() {
        let Some(candidate) = objects.raw_get::<Option<Table>>(index)? else {
            continue;
        };
        if candidate.raw_get::<usize>(index_key())? == root_index
            || candidate
                .raw_get::<Option<i32>>(frame_level_key())?
                .is_none()
            || !object_is_descendant(&objects, &candidate, root_index)?
        {
            continue;
        }
        let level = candidate.raw_get::<i32>(frame_level_key())?;
        candidate.raw_set(frame_level_key(), level.saturating_add(delta).max(0))?;
    }
    Ok(())
}

fn effective_frame_depth(lua: &Lua, mut object: Table) -> mlua::Result<f64> {
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let mut depth = 0.0;
    loop {
        depth += object.raw_get::<f64>(frame_depth_key())?;
        let Some(parent) = object.raw_get::<Option<usize>>(parent_key())? else {
            return Ok(depth);
        };
        let Some(parent) = objects.raw_get::<Option<Table>>(parent)? else {
            return Ok(depth);
        };
        // Regions have no additive frame depth and terminate the frame chain.
        if parent.raw_get::<Option<f64>>(frame_depth_key())?.is_none() {
            return Ok(depth);
        }
        object = parent;
    }
}

/// Seeds the native edit-box defaults before XML scripts can observe them.
fn initialize_edit_box(lua: &Lua, object: &Table) -> mlua::Result<()> {
    object.raw_set(text_key(), "")?;
    object.raw_set(edit_focused_key(), false)?;
    object.raw_set(edit_alt_arrow_key(), false)?;
    object.raw_set(edit_history_lines_key(), 0_u32)?;
    object.raw_set(edit_history_key(), lua.create_table()?)?;
    object.raw_set(edit_max_letters_key(), 0_u32)?;
    object.raw_set(edit_max_bytes_key(), 0_u32)?;
    object.raw_set(edit_cursor_key(), 0_u32)?;
    object.raw_set(edit_selection_start_key(), 0_u32)?;
    object.raw_set(edit_selection_end_key(), 0_u32)?;
    object.raw_set(edit_blink_speed_key(), 0.5_f64)?;
    object.raw_set(edit_caret_elapsed_key(), 0.0_f64)?;
    object.raw_set(edit_caret_visible_key(), true)?;
    object.raw_set(
        edit_highlight_color_key(),
        lua.create_sequence_from([96.0 / 255.0, 96.0 / 255.0, 96.0 / 255.0, 1.0])?,
    )?;
    object.raw_set(edit_password_key(), false)?;
    object.raw_set(edit_numeric_key(), false)?;
    object.raw_set(edit_multi_line_key(), false)?;
    object.raw_set(edit_count_invisible_key(), false)?;
    object.raw_set(edit_auto_focus_key(), true)?;
    object.raw_set(
        edit_text_insets_key(),
        lua.create_sequence_from([0.0_f64; 4])?,
    )
}

/// Registers the retained text-entry surface used by stock chat edit boxes.
fn register_edit_box_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    register_edit_box_font_methods(lua, methods)?;
    methods.raw_set(
        "SetText",
        lua.create_function(|lua, (object, value): (Table, Value)| {
            let text = lua
                .coerce_string(value)?
                .ok_or_else(|| edit_box_usage(&object, "SetText", "\"text\""))?
                .to_string_lossy();
            set_edit_box_text(lua, &object, text, false)
        })?,
    )?;
    methods.raw_set(
        "GetText",
        lua.create_function(|_, object: Table| object.raw_get::<String>(text_key()))?,
    )?;
    methods.raw_set(
        "SetNumber",
        lua.create_function(|lua, (object, number): (Table, f64)| {
            set_edit_box_text(lua, &object, number.to_string(), false)
        })?,
    )?;
    methods.raw_set(
        "GetNumber",
        lua.create_function(|_, object: Table| {
            Ok(object
                .raw_get::<String>(text_key())?
                .parse::<f64>()
                .unwrap_or(0.0))
        })?,
    )?;
    methods.raw_set(
        "GetNumLetters",
        lua.create_function(|_, object: Table| {
            Ok(object.raw_get::<String>(text_key())?.chars().count() as u32)
        })?,
    )?;
    register_edit_box_limit_methods(lua, methods)?;
    register_edit_box_focus_methods(lua, methods)?;
    register_edit_box_flag_methods(lua, methods)?;
    register_edit_box_history_methods(lua, methods)?;
    register_edit_box_cursor_methods(lua, methods)?;
    register_edit_box_text_inset_methods(lua, methods)
}

fn register_edit_box_font_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetFontObject",
        lua.create_function(|lua, (edit_box, value): (Table, Value)| {
            let font = resolve_font_object(lua, value)
                .ok_or_else(|| mlua::Error::runtime("Usage: EditBox:SetFontObject(fontObject)"))?;
            let color: Table = font.raw_get(text_color_key())?;
            let shadow_offset: Table = font.raw_get(font_shadow_offset_key())?;
            let shadow_color: Table = font.raw_get(font_shadow_color_key())?;
            edit_box.raw_set(font_object_key(), font)?;
            edit_box.raw_set(text_color_key(), color)?;
            edit_box.raw_set(font_shadow_offset_key(), shadow_offset)?;
            edit_box.raw_set(font_shadow_color_key(), shadow_color)?;
            edit_box.raw_set(font_set_key(), true)
        })?,
    )?;
    methods.raw_set(
        "GetFontObject",
        lua.create_function(|_, edit_box: Table| {
            edit_box.raw_get::<Option<Table>>(font_object_key())
        })?,
    )?;
    methods.raw_set(
        "GetFont",
        lua.create_function(|_, edit_box: Table| {
            let Some(font) = edit_box.raw_get::<Option<Table>>(font_object_key())? else {
                return Ok((None::<String>, None::<f64>, None::<String>));
            };
            Ok((
                font.raw_get::<Option<String>>(font_face_key())?,
                font.raw_get::<Option<f64>>(font_height_key())?,
                font.raw_get::<Option<String>>(font_flags_key())?,
            ))
        })?,
    )?;
    methods.raw_set(
        "SetTextColor",
        lua.create_function(
            |lua, (edit_box, red, green, blue, alpha): (Table, f64, f64, f64, Option<f64>)| {
                edit_box.raw_set(
                    text_color_key(),
                    lua.create_sequence_from(clamped_color(red, green, blue, alpha))?,
                )
            },
        )?,
    )?;
    methods.raw_set(
        "GetTextColor",
        lua.create_function(|_, edit_box: Table| {
            let color: Table = edit_box.raw_get(text_color_key())?;
            Ok((
                color.raw_get::<f64>(1)?,
                color.raw_get::<f64>(2)?,
                color.raw_get::<f64>(3)?,
                color.raw_get::<f64>(4)?,
            ))
        })?,
    )?;
    register_font_string_justification_methods(lua, methods)?;
    register_font_spacing_methods(lua, methods)?;
    register_font_shadow_methods(lua, methods)
}

fn register_edit_box_limit_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    for (setter, getter, key) in [
        ("SetMaxLetters", "GetMaxLetters", edit_max_letters_key()),
        ("SetMaxBytes", "GetMaxBytes", edit_max_bytes_key()),
    ] {
        methods.raw_set(
            setter,
            lua.create_function(move |_, (object, limit): (Table, u32)| {
                object.raw_set(key, limit)
            })?,
        )?;
        methods.raw_set(
            getter,
            lua.create_function(move |_, object: Table| object.raw_get::<u32>(key))?,
        )?;
    }
    methods.raw_set(
        "SetBlinkSpeed",
        lua.create_function(|_, (object, speed): (Table, f64)| {
            if !speed.is_finite() {
                return Err(mlua::Error::runtime("non-finite EditBox blink speed"));
            }
            object.raw_set(edit_blink_speed_key(), speed)?;
            reset_edit_box_caret(&object)
        })?,
    )?;
    methods.raw_set(
        "GetBlinkSpeed",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(edit_blink_speed_key()))?,
    )
}

fn register_edit_box_focus_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetFocus",
        lua.create_function(|lua, object: Table| set_edit_box_focus(lua, &object, true))?,
    )?;
    methods.raw_set(
        "ClearFocus",
        lua.create_function(|lua, object: Table| set_edit_box_focus(lua, &object, false))?,
    )?;
    methods.raw_set(
        "HasFocus",
        lua.create_function(|_, object: Table| {
            stock_optional_true(object.raw_get::<bool>(edit_focused_key())?)
        })?,
    )
}

fn register_edit_box_flag_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    for (setter, getter, key) in [
        ("SetAutoFocus", "IsAutoFocus", edit_auto_focus_key()),
        ("SetMultiLine", "IsMultiLine", edit_multi_line_key()),
        ("SetNumeric", "IsNumeric", edit_numeric_key()),
        ("SetPassword", "IsPassword", edit_password_key()),
        (
            "SetCountInvisibleLetters",
            "IsCountInvisibleLetters",
            edit_count_invisible_key(),
        ),
        (
            "SetAltArrowKeyMode",
            "GetAltArrowKeyMode",
            edit_alt_arrow_key(),
        ),
    ] {
        methods.raw_set(
            setter,
            lua.create_function(move |_, (object, enabled): (Table, Option<bool>)| {
                object.raw_set(key, enabled.unwrap_or(true))
            })?,
        )?;
        methods.raw_set(
            getter,
            lua.create_function(move |_, object: Table| {
                stock_optional_true(object.raw_get::<bool>(key)?)
            })?,
        )?;
    }
    methods.raw_set(
        "IsInIMECompositionMode",
        lua.create_function(|_, _: Table| Ok(Value::Nil))?,
    )?;
    methods.raw_set(
        "GetInputLanguage",
        lua.create_function(|_, _: Table| Ok("ROMAN"))?,
    )?;
    methods.raw_set(
        "ToggleInputLanguage",
        lua.create_function(|_, _: Table| Ok(()))?,
    )
}

fn register_edit_box_history_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetHistoryLines",
        lua.create_function(|_, (object, lines): (Table, u32)| {
            if lines == 0 {
                return Err(edit_box_usage(&object, "SetHistoryLines", "numLines"));
            }
            object.raw_set(edit_history_lines_key(), lines)
        })?,
    )?;
    methods.raw_set(
        "GetHistoryLines",
        lua.create_function(|_, object: Table| object.raw_get::<u32>(edit_history_lines_key()))?,
    )?;
    methods.raw_set(
        "ClearHistory",
        lua.create_function(|lua, object: Table| {
            object.raw_set(edit_history_key(), lua.create_table()?)
        })?,
    )?;
    methods.raw_set(
        "AddHistoryLine",
        lua.create_function(|lua, (object, value): (Table, Value)| {
            let text = lua
                .coerce_string(value)?
                .ok_or_else(|| edit_box_usage(&object, "AddHistoryLine", "\"text\""))?
                .to_string_lossy();
            let maximum = object.raw_get::<u32>(edit_history_lines_key())? as usize;
            if maximum == 0 {
                return Ok(());
            }
            let history: Table = object.raw_get(edit_history_key())?;
            let count = history.raw_len();
            if count >= maximum {
                for index in 1..count {
                    let next = history.raw_get::<Value>(index + 1)?;
                    history.raw_set(index, next)?;
                }
                history.raw_set(count, text)
            } else {
                history.raw_set(count + 1, text)
            }
        })?,
    )
}

fn register_edit_box_cursor_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetCursorPosition",
        lua.create_function(|_, (object, position): (Table, u32)| {
            let text = object.raw_get::<String>(text_key())?;
            object.raw_set(
                edit_cursor_key(),
                clamp_utf8_boundary(&text, position as usize) as u32,
            )?;
            reset_edit_box_caret(&object)
        })?,
    )?;
    methods.raw_set(
        "GetCursorPosition",
        lua.create_function(|_, object: Table| object.raw_get::<u32>(edit_cursor_key()))?,
    )?;
    methods.raw_set(
        "GetUTF8CursorPosition",
        lua.create_function(|_, object: Table| {
            let text = object.raw_get::<String>(text_key())?;
            let cursor = object.raw_get::<u32>(edit_cursor_key())? as usize;
            Ok(text[..clamp_utf8_boundary(&text, cursor)].chars().count() as u32)
        })?,
    )?;
    methods.raw_set(
        "HighlightText",
        lua.create_function(
            |_, (object, start, finish): (Table, Option<u32>, Option<i64>)| {
                let text = object.raw_get::<String>(text_key())?;
                let length = text.len() as u32;
                let start = clamp_utf8_boundary(&text, start.unwrap_or(0).min(length) as usize);
                let finish = clamp_utf8_boundary(
                    &text,
                    finish
                        .map_or(length, |value| u32::try_from(value).unwrap_or(length))
                        .min(length) as usize,
                );
                object.raw_set(edit_selection_start_key(), start as u32)?;
                object.raw_set(edit_selection_end_key(), finish as u32)?;
                reset_edit_box_caret(&object)
            },
        )?,
    )?;
    methods.raw_set(
        "Insert",
        lua.create_function(|lua, (object, value): (Table, Value)| {
            let inserted = lua
                .coerce_string(value)?
                .ok_or_else(|| edit_box_usage(&object, "Insert", "\"text\""))?
                .to_string_lossy();
            insert_edit_box_text(lua, &object, &inserted, false)
        })?,
    )
}

fn register_edit_box_text_inset_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetTextInsets",
        lua.create_function(
            |lua, (object, left, right, top, bottom): (Table, f64, f64, f64, f64)| {
                object.raw_set(
                    edit_text_insets_key(),
                    lua.create_sequence_from([left, right, top, bottom])?,
                )
            },
        )?,
    )?;
    methods.raw_set(
        "GetTextInsets",
        lua.create_function(|_, object: Table| {
            let insets: Table = object.raw_get(edit_text_insets_key())?;
            Ok((
                insets.raw_get::<f64>(1)?,
                insets.raw_get::<f64>(2)?,
                insets.raw_get::<f64>(3)?,
                insets.raw_get::<f64>(4)?,
            ))
        })?,
    )
}

fn set_edit_box_text(
    lua: &Lua,
    object: &Table,
    text: String,
    user_input: bool,
) -> mlua::Result<()> {
    set_edit_box_text_at_cursor(lua, object, text, usize::MAX, user_input)
}

/// Applies limits and publishes cursor/selection before `OnTextChanged` runs.
fn set_edit_box_text_at_cursor(
    lua: &Lua,
    object: &Table,
    text: String,
    cursor: usize,
    user_input: bool,
) -> mlua::Result<()> {
    let byte_limit = object.raw_get::<u32>(edit_max_bytes_key())? as usize;
    let letter_limit = object.raw_get::<u32>(edit_max_letters_key())? as usize;
    let mut text = text;
    if letter_limit != 0 {
        text = text.chars().take(letter_limit).collect();
    }
    if byte_limit != 0 && text.len() > byte_limit {
        let mut boundary = byte_limit;
        while !text.is_char_boundary(boundary) {
            boundary -= 1;
        }
        text.truncate(boundary);
    }
    let cursor = clamp_utf8_boundary(&text, cursor) as u32;
    object.raw_set(text_key(), text)?;
    object.raw_set(edit_cursor_key(), cursor)?;
    object.raw_set(edit_selection_start_key(), cursor)?;
    object.raw_set(edit_selection_end_key(), cursor)?;
    reset_edit_box_caret(object)?;
    if let Some(function) = object_script_function(lua, object, UiScriptHandler::TextChanged)? {
        call_boolean_object_handler(lua, &function, object.clone(), user_input)?;
    }
    Ok(())
}

/// Inserts one native text-input fragment at the live selection or cursor.
fn insert_edit_box_text(
    lua: &Lua,
    object: &Table,
    inserted: &str,
    user_input: bool,
) -> mlua::Result<()> {
    let text = object.raw_get::<String>(text_key())?;
    let (start, end) = edit_box_replacement_range(object, &text)?;
    let mut replacement = String::with_capacity(text.len() + inserted.len());
    replacement.push_str(&text[..start]);
    replacement.push_str(inserted);
    replacement.push_str(&text[end..]);
    set_edit_box_text_at_cursor(
        lua,
        object,
        replacement,
        start.saturating_add(inserted.len()),
        user_input,
    )
}

/// Handles the editing and specialized callback vocabulary owned by EditBox.
fn dispatch_edit_box_key(
    lua: &Lua,
    object: &Table,
    key: &str,
    modifiers: UiKeyboardModifiers,
) -> mlua::Result<()> {
    if modifiers.control() && key.eq_ignore_ascii_case("A") {
        let length = object.raw_get::<String>(text_key())?.len() as u32;
        object.raw_set(edit_selection_start_key(), 0_u32)?;
        object.raw_set(edit_selection_end_key(), length)?;
        object.raw_set(edit_cursor_key(), length)?;
        reset_edit_box_caret(object)?;
        return Ok(());
    }
    match key {
        "BACKSPACE" => delete_edit_box_text(lua, object, true)?,
        "DELETE" => delete_edit_box_text(lua, object, false)?,
        "LEFT" => move_edit_box_cursor(object, EditCursorMove::Left, modifiers.shift())?,
        "RIGHT" => move_edit_box_cursor(object, EditCursorMove::Right, modifiers.shift())?,
        "HOME" => move_edit_box_cursor(object, EditCursorMove::Home, modifiers.shift())?,
        "END" => move_edit_box_cursor(object, EditCursorMove::End, modifiers.shift())?,
        "ENTER" | "NUMPADENTER" => {
            call_optional_object_handler(lua, object, UiScriptHandler::EnterPressed)?;
        }
        "ESCAPE" => {
            call_optional_object_handler(lua, object, UiScriptHandler::EscapePressed)?;
        }
        "SPACE" => {
            call_optional_object_handler(lua, object, UiScriptHandler::SpacePressed)?;
        }
        "TAB" => {
            call_optional_object_handler(lua, object, UiScriptHandler::TabPressed)?;
        }
        _ => {}
    }
    reset_edit_box_caret(object)?;
    Ok(())
}

fn call_optional_object_handler(
    lua: &Lua,
    object: &Table,
    handler: UiScriptHandler,
) -> mlua::Result<()> {
    let Some(function) = object_script_function(lua, object, handler)? else {
        return Ok(());
    };
    call_object_handler(lua, &function, object.clone())
}

/// Deletes the selected range or one adjacent UTF-8 scalar.
fn delete_edit_box_text(lua: &Lua, object: &Table, backward: bool) -> mlua::Result<()> {
    let text = object.raw_get::<String>(text_key())?;
    let (mut start, mut end) = edit_box_replacement_range(object, &text)?;
    if start == end {
        if backward {
            start = previous_utf8_boundary(&text, start);
        } else {
            end = next_utf8_boundary(&text, end);
        }
    }
    if start == end {
        return Ok(());
    }
    let mut replacement = String::with_capacity(text.len() - (end - start));
    replacement.push_str(&text[..start]);
    replacement.push_str(&text[end..]);
    set_edit_box_text_at_cursor(lua, object, replacement, start, true)
}

fn edit_box_replacement_range(object: &Table, text: &str) -> mlua::Result<(usize, usize)> {
    let start = object.raw_get::<u32>(edit_selection_start_key())? as usize;
    let end = object.raw_get::<u32>(edit_selection_end_key())? as usize;
    if start != end {
        Ok((
            clamp_utf8_boundary(text, start.min(end)),
            clamp_utf8_boundary(text, start.max(end)),
        ))
    } else {
        let cursor = object.raw_get::<u32>(edit_cursor_key())? as usize;
        let cursor = clamp_utf8_boundary(text, cursor);
        Ok((cursor, cursor))
    }
}

#[derive(Clone, Copy)]
enum EditCursorMove {
    Left,
    Right,
    Home,
    End,
}

fn move_edit_box_cursor(
    object: &Table,
    movement: EditCursorMove,
    extend_selection: bool,
) -> mlua::Result<()> {
    let text = object.raw_get::<String>(text_key())?;
    let cursor = clamp_utf8_boundary(&text, object.raw_get::<u32>(edit_cursor_key())? as usize);
    let start = clamp_utf8_boundary(
        &text,
        object.raw_get::<u32>(edit_selection_start_key())? as usize,
    );
    let end = clamp_utf8_boundary(
        &text,
        object.raw_get::<u32>(edit_selection_end_key())? as usize,
    );
    let target = if !extend_selection && start != end {
        match movement {
            EditCursorMove::Left | EditCursorMove::Home => start.min(end),
            EditCursorMove::Right | EditCursorMove::End => start.max(end),
        }
    } else {
        match movement {
            EditCursorMove::Left => previous_utf8_boundary(&text, cursor),
            EditCursorMove::Right => next_utf8_boundary(&text, cursor),
            EditCursorMove::Home => 0,
            EditCursorMove::End => text.len(),
        }
    };
    object.raw_set(edit_cursor_key(), target as u32)?;
    if extend_selection {
        let anchor = if start == end {
            cursor
        } else if cursor == start {
            end
        } else {
            start
        };
        object.raw_set(edit_selection_start_key(), anchor as u32)?;
        object.raw_set(edit_selection_end_key(), target as u32)
    } else {
        object.raw_set(edit_selection_start_key(), target as u32)?;
        object.raw_set(edit_selection_end_key(), target as u32)
    }
}

fn previous_utf8_boundary(text: &str, cursor: usize) -> usize {
    text[..clamp_utf8_boundary(text, cursor)]
        .char_indices()
        .next_back()
        .map_or(0, |(index, _)| index)
}

fn next_utf8_boundary(text: &str, cursor: usize) -> usize {
    let cursor = clamp_utf8_boundary(text, cursor);
    text[cursor..]
        .chars()
        .next()
        .map_or(cursor, |character| cursor + character.len_utf8())
}

fn clamp_utf8_boundary(text: &str, requested: usize) -> usize {
    let mut boundary = requested.min(text.len());
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

fn set_edit_box_focus(lua: &Lua, object: &Table, focused: bool) -> mlua::Result<()> {
    let object_index = object.raw_get::<usize>(index_key())?;
    let was_focused = object.raw_get::<bool>(edit_focused_key())?;
    if !focused {
        if was_focused {
            object.raw_set(edit_focused_key(), false)?;
            reset_edit_box_caret(object)?;
            if lua.named_registry_value::<usize>(FOCUSED_EDIT_BOX_REGISTRY)? == object_index {
                lua.set_named_registry_value(FOCUSED_EDIT_BOX_REGISTRY, 0_usize)?;
            }
            call_optional_object_handler(lua, object, UiScriptHandler::EditFocusLost)?;
        }
        return Ok(());
    }
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let previous = lua.named_registry_value::<usize>(FOCUSED_EDIT_BOX_REGISTRY)?;
    if previous != 0 && previous != object_index {
        let candidate = objects.raw_get::<Table>(previous)?;
        candidate.raw_set(edit_focused_key(), false)?;
        reset_edit_box_caret(&candidate)?;
        call_optional_object_handler(lua, &candidate, UiScriptHandler::EditFocusLost)?;
    }
    if !was_focused {
        object.raw_set(edit_focused_key(), true)?;
        reset_edit_box_caret(object)?;
        lua.set_named_registry_value(FOCUSED_EDIT_BOX_REGISTRY, object_index)?;
        call_optional_object_handler(lua, object, UiScriptHandler::EditFocusGained)?;
    }
    Ok(())
}

fn reset_edit_box_caret(object: &Table) -> mlua::Result<()> {
    object.raw_set(edit_caret_elapsed_key(), 0.0_f64)?;
    object.raw_set(edit_caret_visible_key(), true)
}

/// Advances the one focused EditBox's authored half-cycle and marks only
/// visibility boundaries as retained presentation changes.
fn advance_edit_box_caret(lua: &Lua, objects: &Table, elapsed_seconds: f64) -> mlua::Result<()> {
    let object_index = lua.named_registry_value::<usize>(FOCUSED_EDIT_BOX_REGISTRY)?;
    if object_index == 0 {
        return Ok(());
    }
    let mut changed = false;
    let object = objects.raw_get::<Table>(object_index)?;
    if object.raw_get::<bool>(edit_focused_key())? {
        let interval = object.raw_get::<f64>(edit_blink_speed_key())?;
        let visible = object.raw_get::<bool>(edit_caret_visible_key())?;
        if interval <= 0.0 {
            if !visible {
                reset_edit_box_caret(&object)?;
                changed = true;
            }
        } else {
            let total = object.raw_get::<f64>(edit_caret_elapsed_key())? + elapsed_seconds;
            if total < interval {
                object.raw_set(edit_caret_elapsed_key(), total)?;
            } else {
                let boundaries = (total / interval).floor() as u64;
                object.raw_set(edit_caret_elapsed_key(), total % interval)?;
                if boundaries & 1 != 0 {
                    object.raw_set(edit_caret_visible_key(), !visible)?;
                    changed = true;
                }
            }
        }
    }
    if changed {
        mark_live_state_changed(lua)?;
    }
    Ok(())
}

fn call_boolean_object_handler(
    lua: &Lua,
    function: &mlua::Function,
    object: Table,
    value: bool,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let previous_this = globals.raw_get::<Value>("this")?;
    let previous_arg = globals.raw_get::<Value>("arg1")?;
    globals.raw_set("this", object.clone())?;
    globals.raw_set("arg1", value)?;
    let result = function.call::<()>((object, value));
    let restore_this = globals.raw_set("this", previous_this);
    let restore_arg = globals.raw_set("arg1", previous_arg);
    match result {
        Ok(()) => {
            restore_this?;
            restore_arg
        }
        Err(error) => {
            let _ = restore_this;
            let _ = restore_arg;
            Err(error)
        }
    }
}

/// Preserves legacy callback globals for one finite numeric argument.
fn call_number_object_handler(
    lua: &Lua,
    function: &mlua::Function,
    object: Table,
    value: f64,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let previous_this = globals.raw_get::<Value>("this")?;
    let previous_arg = globals.raw_get::<Value>("arg1")?;
    globals.raw_set("this", object.clone())?;
    globals.raw_set("arg1", value)?;
    let result = function.call::<()>((object, value));
    let restore_this = globals.raw_set("this", previous_this);
    let restore_arg = globals.raw_set("arg1", previous_arg);
    match result {
        Ok(()) => {
            restore_this?;
            restore_arg
        }
        Err(error) => {
            let _ = restore_this;
            let _ = restore_arg;
            Err(error)
        }
    }
}

/// Preserves legacy callback globals for two finite numeric arguments.
fn call_two_number_object_handler(
    lua: &Lua,
    function: &mlua::Function,
    object: Table,
    first: f64,
    second: f64,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let previous_this = globals.raw_get::<Value>("this")?;
    let previous_arg1 = globals.raw_get::<Value>("arg1")?;
    let previous_arg2 = globals.raw_get::<Value>("arg2")?;
    globals.raw_set("this", object.clone())?;
    globals.raw_set("arg1", first)?;
    globals.raw_set("arg2", second)?;
    let result = function.call::<()>((object, first, second));
    let restore_this = globals.raw_set("this", previous_this);
    let restore_arg1 = globals.raw_set("arg1", previous_arg1);
    let restore_arg2 = globals.raw_set("arg2", previous_arg2);
    match result {
        Ok(()) => {
            restore_this?;
            restore_arg1?;
            restore_arg2
        }
        Err(error) => {
            let _ = restore_this;
            let _ = restore_arg1;
            let _ = restore_arg2;
            Err(error)
        }
    }
}

fn stock_optional_true(value: bool) -> mlua::Result<Value> {
    Ok(if value {
        Value::Number(1.0)
    } else {
        Value::Nil
    })
}

fn edit_box_usage(object: &Table, method: &str, arguments: &str) -> mlua::Error {
    let name = object
        .raw_get::<Option<String>>(name_key())
        .ok()
        .flatten()
        .unwrap_or_else(|| "<unnamed>".to_owned());
    mlua::Error::runtime(format!("Usage: {name}:{method}({arguments})"))
}

fn register_scroll_frame_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "GetScrollChild",
        lua.create_function(|_, object: Table| {
            object.raw_get::<Option<Table>>(scroll_child_key())
        })?,
    )?;
    methods.raw_set(
        "SetScrollChild",
        lua.create_function(|lua, (object, requested): (Table, Value)| {
            set_scroll_child(lua, &object, requested)
        })?,
    )?;
    methods.raw_set(
        "GetHorizontalScroll",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(horizontal_scroll_key()))?,
    )?;
    methods.raw_set(
        "GetVerticalScroll",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(vertical_scroll_key()))?,
    )?;
    methods.raw_set(
        "GetHorizontalScrollRange",
        lua.create_function(|_, object: Table| {
            object.raw_get::<f64>(horizontal_scroll_range_key())
        })?,
    )?;
    methods.raw_set(
        "GetVerticalScrollRange",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(vertical_scroll_range_key()))?,
    )?;
    methods.raw_set(
        "SetHorizontalScroll",
        lua.create_function(|lua, (object, value): (Table, f64)| {
            let (range, _) = refresh_scroll_ranges(&object)?;
            let value = value.clamp(0.0, range);
            let previous = object.raw_get::<f64>(horizontal_scroll_key())?;
            if value == previous {
                return Ok(());
            }
            object.raw_set(horizontal_scroll_key(), value)?;
            if let Some(function) =
                object_script_function(lua, &object, UiScriptHandler::HorizontalScroll)?
            {
                call_number_object_handler(lua, &function, object, value)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "SetVerticalScroll",
        lua.create_function(|lua, (object, value): (Table, f64)| {
            let (_, range) = refresh_scroll_ranges(&object)?;
            let value = value.clamp(0.0, range);
            let previous = object.raw_get::<f64>(vertical_scroll_key())?;
            if value == previous {
                return Ok(());
            }
            object.raw_set(vertical_scroll_key(), value)?;
            if let Some(function) =
                object_script_function(lua, &object, UiScriptHandler::VerticalScroll)?
            {
                call_number_object_handler(lua, &function, object, value)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "UpdateScrollChildRect",
        lua.create_function(|_, object: Table| update_scroll_child_rect(&object))?,
    )?;
    Ok(())
}

/// Recomputes the scrollable extent after script-authored child dimensions change.
///
/// The native method marks `CSimpleScrollFrame` dirty and its layout pass
/// publishes these ranges. The retained runtime resolves the same observable
/// state eagerly because FrameXML construction runs without an intervening
/// native frame tick.
fn update_scroll_child_rect(object: &Table) -> mlua::Result<()> {
    let (horizontal_range, vertical_range) = refresh_scroll_ranges(object)?;

    let horizontal_scroll = object
        .raw_get::<f64>(horizontal_scroll_key())?
        .clamp(0.0, horizontal_range);
    let vertical_scroll = object
        .raw_get::<f64>(vertical_scroll_key())?
        .clamp(0.0, vertical_range);
    object.raw_set(horizontal_scroll_key(), horizontal_scroll)?;
    object.raw_set(vertical_scroll_key(), vertical_scroll)
}

/// Publishes the range implied by the scroll frame's current live dimensions.
fn refresh_scroll_ranges(object: &Table) -> mlua::Result<(f64, f64)> {
    let frame_width = object.raw_get::<f64>(width_key())?;
    let frame_height = object.raw_get::<f64>(height_key())?;
    let (child_width, child_height) = object
        .raw_get::<Option<Table>>(scroll_child_key())?
        .map(|child| {
            Ok::<(f64, f64), mlua::Error>((
                child.raw_get::<f64>(width_key())?,
                child.raw_get::<f64>(height_key())?,
            ))
        })
        .transpose()?
        .unwrap_or((0.0, 0.0));
    let horizontal_range = (child_width - frame_width).max(0.0);
    let vertical_range = (child_height - frame_height).max(0.0);
    object.raw_set(horizontal_scroll_range_key(), horizontal_range)?;
    object.raw_set(vertical_scroll_range_key(), vertical_range)?;
    Ok((horizontal_range, vertical_range))
}

/// Applies the stock scroll-child ownership contract recovered from
/// `CSimpleScrollFrame::SetScrollChild` in the 3.3.5a executable.
fn set_scroll_child(lua: &Lua, object: &Table, requested: Value) -> mlua::Result<()> {
    let owner_name = object
        .raw_get::<Option<String>>(name_key())?
        .unwrap_or_else(|| "<unnamed>".to_owned());
    let child = match requested {
        Value::Nil => None,
        Value::String(name) => {
            let name = name.to_str()?;
            lua.globals()
                .raw_get::<Option<Table>>(name.as_ref())?
                .ok_or_else(|| {
                    mlua::Error::runtime(format!(
                        "{owner_name}:SetScrollChild(): Couldn't find frame named '{name}'"
                    ))
                })?
                .into()
        }
        Value::Table(child) => Some(child),
        _ => {
            return Err(mlua::Error::runtime(format!(
                "{owner_name}:SetScrollChild(): Couldn't find frame named '<invalid>'"
            )));
        }
    };

    if let Some(child) = child.as_ref() {
        let child_type = child.raw_get::<Option<String>>(type_key())?;
        let Some(child_type) = child_type else {
            return Err(mlua::Error::runtime(format!(
                "{owner_name}:SetScrollChild(): Couldn't find 'this' in child object"
            )));
        };
        if !is_object_type(&child_type, "Frame") {
            return Err(mlua::Error::runtime(format!(
                "{owner_name}:SetScrollChild(): Wrong child object type, expected frame"
            )));
        }

        let requested_index = child.raw_get::<usize>(index_key())?;
        let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
        let mut cursor = Some(object.raw_get::<usize>(index_key())?);
        while let Some(index) = cursor {
            if index == requested_index {
                let child_name = child
                    .raw_get::<Option<String>>(name_key())?
                    .unwrap_or_else(|| "<unnamed>".to_owned());
                return Err(mlua::Error::runtime(format!(
                    "{owner_name}:SetScrollChild(): Would create a loop adding child {child_name}"
                )));
            }
            let candidate: Table = objects.raw_get(index)?;
            cursor = candidate.raw_get::<Option<usize>>(parent_key())?;
        }
    }

    // Native code detaches the previous scroll child before assigning and
    // parenting the replacement. This relation is therefore stronger than a
    // cached getter value and must update the live region hierarchy as well.
    if let Some(previous) = object.raw_get::<Option<Table>>(scroll_child_key())? {
        previous.raw_set(parent_key(), Option::<usize>::None)?;
    }
    if let Some(child) = child.as_ref() {
        child.raw_set(parent_key(), object.raw_get::<usize>(index_key())?)?;
    }
    object.raw_set(scroll_child_key(), child)
}

fn register_slider_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    register_range_value_methods(lua, methods)?;
    methods.raw_set(
        "GetOrientation",
        lua.create_function(|_, object: Table| object.raw_get::<String>(slider_orientation_key()))?,
    )?;
    methods.raw_set(
        "SetOrientation",
        lua.create_function(|_, (object, orientation): (Table, String)| {
            let orientation = orientation.to_ascii_uppercase();
            if !matches!(orientation.as_str(), "HORIZONTAL" | "VERTICAL") {
                return Err(mlua::Error::runtime(
                    "SetOrientation expects HORIZONTAL or VERTICAL",
                ));
            }
            object.raw_set(slider_orientation_key(), orientation)
        })?,
    )?;
    methods.raw_set(
        "GetValueStep",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(slider_step_key()))?,
    )?;
    methods.raw_set(
        "SetValueStep",
        lua.create_function(|lua, (object, step): (Table, f64)| {
            object.raw_set(slider_step_key(), step.max(0.0))?;
            let value = object.raw_get::<f64>(slider_value_key())?;
            set_range_value(lua, object, value)
        })?,
    )?;
    Ok(())
}

/// Registers the value/range contract shared by Slider and StatusBar.
fn register_range_value_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "GetValue",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(slider_value_key()))?,
    )?;
    methods.raw_set(
        "SetValue",
        lua.create_function(|lua, (object, value): (Table, f64)| {
            set_range_value(lua, object, value)
        })?,
    )?;
    methods.raw_set(
        "GetMinMaxValues",
        lua.create_function(|_, object: Table| {
            Ok((
                object.raw_get::<f64>(slider_min_key())?,
                object.raw_get::<f64>(slider_max_key())?,
            ))
        })?,
    )?;
    methods.raw_set(
        "SetMinMaxValues",
        lua.create_function(|_, (object, minimum, maximum): (Table, f64, f64)| {
            if minimum > maximum {
                return Err(mlua::Error::runtime(
                    "SetMinMaxValues minimum exceeds maximum",
                ));
            }
            object.raw_set(slider_min_key(), minimum)?;
            object.raw_set(slider_max_key(), maximum)?;
            let value = object.raw_get::<f64>(slider_value_key())?;
            object.raw_set(slider_value_key(), value.clamp(minimum, maximum))
        })?,
    )?;
    Ok(())
}

fn set_range_value(lua: &Lua, object: Table, value: f64) -> mlua::Result<()> {
    let minimum = object.raw_get::<f64>(slider_min_key())?;
    let maximum = object.raw_get::<f64>(slider_max_key())?;
    let step = object
        .raw_get::<Option<f64>>(slider_step_key())?
        .unwrap_or(0.0);
    let value = if step > 0.0 {
        (minimum + ((value - minimum) / step).round() * step).clamp(minimum, maximum)
    } else {
        value.clamp(minimum, maximum)
    };
    let previous = object.raw_get::<f64>(slider_value_key())?;
    if value == previous {
        return Ok(());
    }
    object.raw_set(slider_value_key(), value)?;
    if let Some(function) = object_script_function(lua, &object, UiScriptHandler::ValueChanged)? {
        call_number_object_handler(lua, &function, object, value)?;
    }
    Ok(())
}

/// Installs status-bar fill color and texture state on top of shared range state.
fn register_status_bar_methods(
    lua: &Lua,
    methods: &Table,
    dynamic_arena: DynamicArenaState,
) -> mlua::Result<()> {
    register_range_value_methods(lua, methods)?;
    methods.raw_set(
        "SetStatusBarColor",
        lua.create_function(
            |lua, (object, red, green, blue, alpha): (Table, f64, f64, f64, Option<f64>)| {
                object.raw_set(
                    status_bar_color_key(),
                    lua.create_sequence_from(clamped_color(red, green, blue, alpha))?,
                )
            },
        )?,
    )?;
    methods.raw_set(
        "GetStatusBarColor",
        lua.create_function(|_, object: Table| {
            let color: Table = object.raw_get(status_bar_color_key())?;
            Ok((
                color.raw_get::<f64>(1)?,
                color.raw_get::<f64>(2)?,
                color.raw_get::<f64>(3)?,
                color.raw_get::<f64>(4)?,
            ))
        })?,
    )?;
    methods.raw_set(
        "SetStatusBarTexture",
        lua.create_function(move |lua, (object, value): (Table, Value)| {
            let texture = match value {
                Value::Nil => {
                    object.raw_set(status_bar_texture_key(), Option::<Table>::None)?;
                    return Ok(());
                }
                Value::Table(texture) => {
                    if texture.raw_get::<String>(type_key())? != "Texture" {
                        return Err(mlua::Error::runtime(
                            "Usage: StatusBar:SetStatusBarTexture(\"filename\" or textureObject)",
                        ));
                    }
                    texture
                }
                Value::String(path) => {
                    let texture = match object.raw_get::<Option<Table>>(status_bar_texture_key())? {
                        Some(texture) => texture,
                        None => create_dynamic_region(
                            lua,
                            "Texture",
                            object.clone(),
                            None,
                            "ARTWORK".into(),
                            None,
                            None,
                            &dynamic_arena.counters(),
                        )?,
                    };
                    let path = path.to_string_lossy();
                    texture.raw_set(texture_file_key(), (!path.is_empty()).then_some(path))?;
                    texture.raw_set(texture_solid_color_key(), Option::<Table>::None)?;
                    texture
                }
                _ => {
                    return Err(mlua::Error::runtime(
                        "Usage: StatusBar:SetStatusBarTexture(\"filename\" or textureObject)",
                    ));
                }
            };
            object.raw_set(status_bar_texture_key(), texture)
        })?,
    )?;
    methods.raw_set(
        "GetStatusBarTexture",
        lua.create_function(|_, object: Table| {
            object.raw_get::<Option<Table>>(status_bar_texture_key())
        })?,
    )
}

fn register_enabled_methods(lua: &Lua, methods: &Table, kind: UiObjectKind) -> mlua::Result<()> {
    methods.raw_set(
        "Enable",
        lua.create_function(|lua, object: Table| {
            if !object.raw_get::<bool>(enabled_key())? {
                object.raw_set(enabled_key(), true)?;
                mark_live_state_changed(lua)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "Disable",
        lua.create_function(|lua, object: Table| {
            if object.raw_get::<bool>(enabled_key())? {
                object.raw_set(enabled_key(), false)?;
                mark_live_state_changed(lua)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "IsEnabled",
        lua.create_function(move |_, object: Table| {
            let enabled = object.raw_get::<bool>(enabled_key())?;
            if kind == UiObjectKind::Slider {
                Ok(if enabled {
                    Value::Number(1.0)
                } else {
                    Value::Nil
                })
            } else {
                Ok(Value::Number(if enabled { 1.0 } else { 0.0 }))
            }
        })?,
    )?;
    Ok(())
}

fn register_region_methods(
    lua: &Lua,
    methods: &Table,
    kind: UiObjectKind,
    ui_extent: (f64, f64),
) -> mlua::Result<()> {
    methods.raw_set(
        "SetParent",
        lua.create_function(|lua, (object, requested): (Table, Option<Table>)| {
            let object_index = object.raw_get::<usize>(index_key())?;
            let requested_index = requested
                .as_ref()
                .map(|parent| parent.raw_get::<usize>(index_key()))
                .transpose()?;
            let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
            let mut cursor = requested_index;
            while let Some(index) = cursor {
                if index == object_index {
                    return Err(mlua::Error::runtime(
                        "SetParent(): parent would create a region hierarchy cycle",
                    ));
                }
                let parent: Table = objects.raw_get(index)?;
                cursor = parent.raw_get::<Option<usize>>(parent_key())?;
            }
            object.raw_set(parent_key(), requested_index)
        })?,
    )?;
    methods.raw_set(
        "GetWidth",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(width_key()))?,
    )?;
    methods.raw_set(
        "GetHeight",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(height_key()))?,
    )?;
    methods.raw_set(
        "SetWidth",
        lua.create_function(move |_, (object, width): (Table, f64)| {
            object.raw_set(width_key(), width)?;
            if kind == UiObjectKind::FontString {
                object.raw_set(auto_text_width_key(), false)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "SetHeight",
        lua.create_function(move |_, (object, height): (Table, f64)| {
            object.raw_set(height_key(), height)?;
            if kind == UiObjectKind::FontString {
                object.raw_set(auto_text_height_key(), false)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "SetSize",
        lua.create_function(move |_, (object, width, height): (Table, f64, f64)| {
            object.raw_set(width_key(), width)?;
            object.raw_set(height_key(), height)?;
            if kind == UiObjectKind::FontString {
                object.raw_set(auto_text_width_key(), false)?;
                object.raw_set(auto_text_height_key(), false)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "GetSize",
        lua.create_function(|_, object: Table| {
            Ok((
                object.raw_get::<f64>(width_key())?,
                object.raw_get::<f64>(height_key())?,
            ))
        })?,
    )?;
    register_region_bounds_methods(lua, methods, ui_extent)?;
    methods.raw_set(
        "GetAlpha",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(alpha_key()))?,
    )?;
    methods.raw_set(
        "SetAlpha",
        lua.create_function(|lua, (object, alpha): (Table, f64)| {
            if !alpha.is_finite() {
                return Err(mlua::Error::runtime("non-finite region alpha"));
            }
            let alpha = alpha.clamp(0.0, 1.0);
            if object.raw_get::<f64>(alpha_key())? != alpha {
                object.raw_set(alpha_key(), alpha)?;
                mark_live_state_changed(lua)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "GetScale",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(scale_key()))?,
    )?;
    methods.raw_set(
        "SetDrawLayer",
        lua.create_function(
            |_, (object, requested, sub_level): (Table, String, Option<i32>)| {
                let layer = parse_draw_layer_name(&requested)
                    .ok_or_else(|| mlua::Error::runtime("invalid draw layer"))?;
                let sub_level = i16::try_from(sub_level.unwrap_or(0))
                    .map_err(|_| mlua::Error::runtime("invalid draw layer"))?;
                object.raw_set(draw_layer_key(), layer)?;
                object.raw_set(draw_sub_level_key(), sub_level)
            },
        )?,
    )?;
    methods.raw_set(
        "GetDrawLayer",
        lua.create_function(|_, object: Table| object.raw_get::<String>(draw_layer_key()))?,
    )?;
    methods.raw_set(
        "SetScale",
        lua.create_function(|_, (object, scale): (Table, f64)| {
            if scale <= 0.0 {
                return Err(mlua::Error::runtime("SetScale(): scale must be positive"));
            }
            object.raw_set(scale_key(), scale)
        })?,
    )?;
    methods.raw_set(
        "GetNumPoints",
        lua.create_function(|_, object: Table| {
            let anchors: Table = object.raw_get(anchors_key())?;
            let mut count = 0;
            for entry in anchors.pairs::<Value, Value>() {
                let _ = entry?;
                count += 1;
            }
            Ok(count)
        })?,
    )?;
    methods.raw_set(
        "GetPoint",
        lua.create_function(|lua, (object, index): (Table, Option<usize>)| {
            get_region_point(lua, &object, index.unwrap_or(1))
        })?,
    )?;
    methods.raw_set(
        "SetPoint",
        lua.create_function(
            |lua, (object, point, arguments): (Table, String, Variadic<Value>)| {
                set_region_point(lua, &object, &point, arguments.as_slice())
            },
        )?,
    )?;
    methods.raw_set(
        "SetAllPoints",
        lua.create_function(|lua, (object, relative): (Table, Option<Value>)| {
            set_all_region_points(lua, &object, relative)
        })?,
    )?;
    methods.raw_set(
        "ClearAllPoints",
        lua.create_function(|lua, object: Table| {
            object.raw_set(anchors_key(), lua.create_table()?)
        })?,
    )?;
    register_region_vertex_color_methods(lua, methods)?;
    register_region_visibility_methods(lua, methods)?;
    Ok(())
}

#[derive(Clone, Copy)]
struct LiveRegionBounds {
    left: f64,
    bottom: f64,
    right: f64,
    top: f64,
}

/// Region edge queries run during FrameXML construction, before the retained
/// arena is frozen into the render geometry plan. Resolve the same authored
/// anchors directly from the live Lua tables so mutations made by an earlier
/// statement are visible to the next statement.
fn register_region_bounds_methods(
    lua: &Lua,
    methods: &Table,
    ui_extent: (f64, f64),
) -> mlua::Result<()> {
    methods.raw_set(
        "GetLeft",
        lua.create_function(move |lua, object: Table| {
            Ok(resolve_live_region_bounds(lua, object, ui_extent)?.map(|bounds| bounds.left))
        })?,
    )?;
    methods.raw_set(
        "GetRight",
        lua.create_function(move |lua, object: Table| {
            Ok(resolve_live_region_bounds(lua, object, ui_extent)?.map(|bounds| bounds.right))
        })?,
    )?;
    methods.raw_set(
        "GetTop",
        lua.create_function(move |lua, object: Table| {
            Ok(resolve_live_region_bounds(lua, object, ui_extent)?.map(|bounds| bounds.top))
        })?,
    )?;
    methods.raw_set(
        "GetBottom",
        lua.create_function(move |lua, object: Table| {
            Ok(resolve_live_region_bounds(lua, object, ui_extent)?.map(|bounds| bounds.bottom))
        })?,
    )?;
    methods.raw_set(
        "GetCenter",
        lua.create_function(move |lua, object: Table| {
            let bounds = resolve_live_region_bounds(lua, object, ui_extent)?;
            Ok(match bounds {
                Some(bounds) => (
                    Some((bounds.left + bounds.right) * 0.5),
                    Some((bounds.bottom + bounds.top) * 0.5),
                ),
                None => (None, None),
            })
        })?,
    )?;
    methods.raw_set(
        "GetBoundsRect",
        lua.create_function(move |lua, object: Table| {
            let bounds = resolve_live_region_bounds(lua, object, ui_extent)?;
            Ok(match bounds {
                Some(bounds) => (
                    Some(bounds.left),
                    Some(bounds.bottom),
                    Some(bounds.right - bounds.left),
                    Some(bounds.top - bounds.bottom),
                ),
                None => (None, None, None, None),
            })
        })?,
    )
}

fn resolve_live_region_bounds(
    lua: &Lua,
    object: Table,
    ui_extent: (f64, f64),
) -> mlua::Result<Option<LiveRegionBounds>> {
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let mut resolved = HashMap::new();
    let mut visiting = Vec::new();
    resolve_live_region_bounds_inner(
        &objects,
        object,
        LiveRegionBounds {
            left: 0.0,
            bottom: 0.0,
            right: ui_extent.0,
            top: ui_extent.1,
        },
        &mut resolved,
        &mut visiting,
    )
}

fn resolve_live_region_bounds_inner(
    objects: &Table,
    object: Table,
    screen: LiveRegionBounds,
    resolved: &mut HashMap<usize, LiveRegionBounds>,
    visiting: &mut Vec<usize>,
) -> mlua::Result<Option<LiveRegionBounds>> {
    let index = object.raw_get::<usize>(index_key())?;
    if let Some(bounds) = resolved.get(&index).copied() {
        return Ok(Some(bounds));
    }
    if visiting.contains(&index) {
        return Ok(None);
    }
    visiting.push(index);

    let parent_index = object.raw_get::<Option<usize>>(parent_key())?;
    let mut anchors = Vec::new();
    let anchor_table: Table = object.raw_get(anchors_key())?;
    for point_index in 1..=9 {
        let Some(record) = anchor_table.raw_get::<Option<Table>>(point_index)? else {
            continue;
        };
        let Some(point) = parse_point(record.raw_get::<String>(1)?.as_str()) else {
            visiting.pop();
            return Ok(None);
        };
        let Some(relative_point) = parse_point(record.raw_get::<String>(3)?.as_str()) else {
            visiting.pop();
            return Ok(None);
        };
        anchors.push((
            point,
            record.raw_get::<Option<usize>>(2)?,
            relative_point,
            (record.raw_get::<f64>(4)?, record.raw_get::<f64>(5)?),
        ));
    }
    let role = object.raw_get::<String>(role_key())?;
    if anchors.is_empty()
        && role != object_role_name(UiObjectRole::Object)
        && role != object_role_name(UiObjectRole::ScrollChild)
        && let Some(parent) = parent_index
    {
        anchors.push((UiPoint::Center, Some(parent), UiPoint::Center, (0.0, 0.0)));
    }

    let mut x_constraints = Vec::with_capacity(anchors.len());
    let mut y_constraints = Vec::with_capacity(anchors.len());
    let mut all_x = Vec::with_capacity(anchors.len());
    let mut all_y = Vec::with_capacity(anchors.len());
    for (point, target_index, relative_point, offset) in anchors {
        let target = if let Some(target_index) = target_index {
            let Some(target) = objects.raw_get::<Option<Table>>(target_index)? else {
                visiting.pop();
                return Ok(None);
            };
            let Some(bounds) =
                resolve_live_region_bounds_inner(objects, target, screen, resolved, visiting)?
            else {
                visiting.pop();
                return Ok(None);
            };
            bounds
        } else {
            screen
        };
        let x = (
            live_point_x_factor(point),
            live_axis_coordinate(
                target.left,
                target.right,
                live_point_x_factor(relative_point),
            ) + offset.0,
        );
        let y = (
            live_point_y_factor(point),
            live_axis_coordinate(
                target.bottom,
                target.top,
                live_point_y_factor(relative_point),
            ) + offset.1,
        );
        all_x.push(x);
        all_y.push(y);
        if live_point_constrains_x(point) {
            x_constraints.push(x);
        }
        if live_point_constrains_y(point) {
            y_constraints.push(y);
        }
    }

    let parent = if let Some(parent_index) = parent_index {
        let Some(parent) = objects.raw_get::<Option<Table>>(parent_index)? else {
            visiting.pop();
            return Ok(None);
        };
        resolve_live_region_bounds_inner(objects, parent, screen, resolved, visiting)?
    } else {
        None
    };
    let fallback_left = parent.map_or(0.0, |bounds| bounds.left);
    let fallback_bottom = parent.map_or(0.0, |bounds| bounds.bottom);
    let horizontal = solve_live_axis(
        object.raw_get::<f64>(width_key())?,
        if x_constraints.is_empty() {
            &all_x
        } else {
            &x_constraints
        },
        fallback_left,
    );
    let vertical = solve_live_axis(
        object.raw_get::<f64>(height_key())?,
        if y_constraints.is_empty() {
            &all_y
        } else {
            &y_constraints
        },
        fallback_bottom,
    );
    visiting.pop();
    let (Some(horizontal), Some(vertical)) = (horizontal, vertical) else {
        return Ok(None);
    };
    let bounds = LiveRegionBounds {
        left: horizontal.0,
        bottom: vertical.0,
        right: horizontal.0 + horizontal.1,
        top: vertical.0 + vertical.1,
    };
    resolved.insert(index, bounds);
    Ok(Some(bounds))
}

fn solve_live_axis(
    authored_extent: f64,
    constraints: &[(f64, f64)],
    fallback_begin: f64,
) -> Option<(f64, f64)> {
    const AXIS_EPSILON: f64 = 0.0001;

    let extent = authored_extent.max(0.0);
    let Some(first) = constraints.first().copied() else {
        return Some((fallback_begin, extent));
    };
    let mut best_pair = None;
    let mut best_separation = 0.0;
    for (left_index, left) in constraints.iter().enumerate() {
        for right in &constraints[left_index + 1..] {
            let separation = (right.0 - left.0).abs();
            if separation > best_separation {
                best_pair = Some((*left, *right));
                best_separation = separation;
            }
        }
    }
    let (begin, extent) = if best_separation > AXIS_EPSILON {
        let (first, second) = best_pair?;
        let inferred = (second.1 - first.1) / (second.0 - first.0);
        let begin = first.1 - first.0 * inferred;
        if inferred < 0.0 {
            (begin + inferred, -inferred)
        } else {
            (begin, inferred)
        }
    } else {
        (first.1 - first.0 * extent, extent)
    };
    (begin.is_finite() && extent.is_finite()).then_some((begin, extent))
}

const fn live_point_x_factor(point: UiPoint) -> f64 {
    match point {
        UiPoint::Top | UiPoint::Center | UiPoint::Bottom => 0.5,
        UiPoint::TopRight | UiPoint::Right | UiPoint::BottomRight => 1.0,
        _ => 0.0,
    }
}

const fn live_point_y_factor(point: UiPoint) -> f64 {
    match point {
        UiPoint::Left | UiPoint::Center | UiPoint::Right => 0.5,
        UiPoint::TopLeft | UiPoint::Top | UiPoint::TopRight => 1.0,
        _ => 0.0,
    }
}

const fn live_point_constrains_x(point: UiPoint) -> bool {
    matches!(
        point,
        UiPoint::TopLeft
            | UiPoint::Left
            | UiPoint::BottomLeft
            | UiPoint::TopRight
            | UiPoint::Right
            | UiPoint::BottomRight
    )
}

const fn live_point_constrains_y(point: UiPoint) -> bool {
    matches!(
        point,
        UiPoint::TopLeft
            | UiPoint::Top
            | UiPoint::TopRight
            | UiPoint::BottomLeft
            | UiPoint::Bottom
            | UiPoint::BottomRight
    )
}

fn live_axis_coordinate(begin: f64, end: f64, factor: f64) -> f64 {
    begin + (end - begin) * factor
}

fn register_region_vertex_color_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetVertexColor",
        lua.create_function(|lua, (object, arguments): (Table, Variadic<Value>)| {
            let mut color = [0.0, 0.0, 0.0, 1.0];
            for (index, component) in color.iter_mut().take(3).enumerate() {
                *component = arguments
                    .get(index)
                    .map(|value| lua.coerce_number(value.clone()))
                    .transpose()?
                    .flatten()
                    .unwrap_or(0.0);
            }
            color[3] = arguments
                .get(3)
                .map(|value| lua.coerce_number(value.clone()))
                .transpose()?
                .flatten()
                .unwrap_or(1.0);
            if color.iter().any(|value| !value.is_finite()) {
                return Err(mlua::Error::runtime("invalid vertex color"));
            }
            let kind = object.raw_get::<String>(type_key())?;
            let component_count = if kind == "Texture" { 16 } else { 4 };
            object.raw_set(
                texture_color_key(),
                lua.create_sequence_from(color.into_iter().cycle().take(component_count))?,
            )
        })?,
    )?;
    methods.raw_set(
        "GetVertexColor",
        lua.create_function(|_, object: Table| {
            let color: Table = object.raw_get(texture_color_key())?;
            Ok((
                color.raw_get::<f64>(1)?,
                color.raw_get::<f64>(2)?,
                color.raw_get::<f64>(3)?,
                color.raw_get::<f64>(4)?,
            ))
        })?,
    )
}

fn parse_draw_layer_name(value: &str) -> Option<&'static str> {
    if value.eq_ignore_ascii_case("BACKGROUND") {
        Some("BACKGROUND")
    } else if value.eq_ignore_ascii_case("BORDER") {
        Some("BORDER")
    } else if value.eq_ignore_ascii_case("ARTWORK") {
        Some("ARTWORK")
    } else if value.eq_ignore_ascii_case("OVERLAY") {
        Some("OVERLAY")
    } else if value.eq_ignore_ascii_case("HIGHLIGHT") {
        Some("HIGHLIGHT")
    } else {
        None
    }
}

fn register_region_visibility_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "Show",
        lua.create_function(|lua, object: Table| {
            if !object.raw_get::<bool>(shown_key())? {
                set_object_shown(lua, &object, true)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "Hide",
        lua.create_function(|lua, object: Table| {
            if object.raw_get::<bool>(shown_key())? {
                set_object_shown(lua, &object, false)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "IsShown",
        lua.create_function(|_, object: Table| {
            Ok(object
                .raw_get::<bool>(shown_key())?
                .then_some(Value::Number(1.0)))
        })?,
    )?;
    methods.raw_set(
        "IsVisible",
        lua.create_function(|lua, object: Table| {
            Ok(object_is_visible(lua, object)?.then_some(Value::Number(1.0)))
        })?,
    )
}

fn set_object_shown(lua: &Lua, object: &Table, shown: bool) -> mlua::Result<()> {
    let root_index = object.raw_get::<usize>(index_key())?;
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let mut subtree = Vec::new();
    for index in 1..=objects.raw_len() {
        let Some(candidate) = objects.raw_get::<Option<Table>>(index)? else {
            continue;
        };
        if object_is_descendant(&objects, &candidate, root_index)? {
            let visible = object_is_visible(lua, candidate.clone())?;
            subtree.push((candidate, visible));
        }
    }

    object.raw_set(shown_key(), shown)?;
    mark_live_state_changed(lua)?;
    for (candidate, was_visible) in subtree {
        let is_visible = object_is_visible(lua, candidate.clone())?;
        if was_visible == is_visible || !is_script_frame_table(&candidate)? {
            continue;
        }
        let handler = if is_visible {
            UiScriptHandler::Show
        } else {
            UiScriptHandler::Hide
        };
        call_optional_object_handler(lua, &candidate, handler)?;
    }
    Ok(())
}

fn object_is_descendant(
    objects: &Table,
    candidate: &Table,
    root_index: usize,
) -> mlua::Result<bool> {
    let mut cursor = Some(candidate.raw_get::<usize>(index_key())?);
    while let Some(index) = cursor {
        if index == root_index {
            return Ok(true);
        }
        let Some(object) = objects.raw_get::<Option<Table>>(index)? else {
            return Ok(false);
        };
        cursor = object.raw_get::<Option<usize>>(parent_key())?;
    }
    Ok(false)
}

fn is_script_frame_table(object: &Table) -> mlua::Result<bool> {
    Ok(!matches!(
        object.raw_get::<String>(type_key())?.as_str(),
        "Texture" | "FontString"
    ))
}

fn object_is_visible(lua: &Lua, mut object: Table) -> mlua::Result<bool> {
    loop {
        if !object.raw_get::<bool>(shown_key())? {
            return Ok(false);
        }
        let Some(parent_index) = object.raw_get::<Option<usize>>(parent_key())? else {
            return Ok(true);
        };
        let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
        let Some(parent) = objects.raw_get::<Option<Table>>(parent_index)? else {
            return Ok(false);
        };
        object = parent;
    }
}

fn get_region_point(
    lua: &Lua,
    object: &Table,
    index: usize,
) -> mlua::Result<(String, Option<Table>, String, f64, f64)> {
    let anchors: Table = object.raw_get(anchors_key())?;
    let mut records = Vec::new();
    for point in 1..=9 {
        if let Some(record) = anchors.raw_get::<Option<Table>>(point)? {
            records.push(record);
        }
    }
    let record = records
        .get(index.saturating_sub(1))
        .ok_or_else(|| mlua::Error::runtime("GetPoint(): anchor index is out of range"))?;
    let relative = if let Some(target) = record.raw_get::<Option<usize>>(2)? {
        let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
        objects.raw_get::<Option<Table>>(target)?
    } else {
        None
    };
    Ok((
        record.raw_get(1)?,
        relative,
        record.raw_get(3)?,
        record.raw_get(4)?,
        record.raw_get(5)?,
    ))
}

fn set_all_region_points(lua: &Lua, object: &Table, relative: Option<Value>) -> mlua::Result<()> {
    let target = match relative {
        None => object.raw_get::<Option<usize>>(parent_key())?,
        Some(Value::Nil) => None,
        Some(Value::String(name)) => {
            let name = resolve_region_name(lua, object, name.to_str()?.as_ref())?;
            let table = lua.globals().raw_get::<Table>(name.as_str()).map_err(|_| {
                mlua::Error::runtime(format!(
                    "SetAllPoints(): Couldn't find region named '{name}'"
                ))
            })?;
            Some(table.raw_get::<usize>(index_key())?)
        }
        Some(Value::Table(table)) => Some(table.raw_get::<usize>(index_key())?),
        Some(_) => object.raw_get::<Option<usize>>(parent_key())?,
    };
    if target == Some(object.raw_get::<usize>(index_key())?) {
        return Err(mlua::Error::runtime(
            "SetAllPoints(): trying to anchor to itself",
        ));
    }
    let anchors = lua.create_table()?;
    let top_left =
        create_anchor_record(lua, UiPoint::TopLeft, target, UiPoint::TopLeft, (0.0, 0.0))?;
    let bottom_right = create_anchor_record(
        lua,
        UiPoint::BottomRight,
        target,
        UiPoint::BottomRight,
        (0.0, 0.0),
    )?;
    anchors.raw_set(point_index(UiPoint::TopLeft), top_left)?;
    anchors.raw_set(point_index(UiPoint::BottomRight), bottom_right)?;
    object.raw_set(anchors_key(), anchors)
}

fn set_region_point(
    lua: &Lua,
    object: &Table,
    point_name: &str,
    arguments: &[Value],
) -> mlua::Result<()> {
    let point = parse_point(point_name)
        .ok_or_else(|| mlua::Error::runtime("SetPoint(): Unknown region point"))?;
    let mut argument_index = 0;
    let mut target = object.raw_get::<Option<usize>>(parent_key())?;
    if let Some(argument) = arguments.first() {
        match argument {
            Value::String(name) => {
                let name = resolve_region_name(lua, object, name.to_str()?.as_ref())?;
                let relative: Value = lua.globals().raw_get(name.as_str())?;
                let Value::Table(relative) = relative else {
                    return Err(mlua::Error::runtime(format!(
                        "SetPoint(): Couldn't find region named '{name}'"
                    )));
                };
                target = Some(relative.raw_get::<usize>(index_key())?);
                argument_index += 1;
            }
            Value::Table(relative) => {
                target = Some(relative.raw_get::<usize>(index_key())?);
                argument_index += 1;
            }
            Value::Nil => {
                target = None;
                argument_index += 1;
            }
            _ => {}
        }
    }
    let self_index = object.raw_get::<usize>(index_key())?;
    if target == Some(self_index) {
        return Err(mlua::Error::runtime(
            "SetPoint(): trying to anchor to itself",
        ));
    }
    let mut relative_point = point;
    if let Some(Value::String(name)) = arguments.get(argument_index) {
        relative_point = parse_point(name.to_str()?.as_ref())
            .ok_or_else(|| mlua::Error::runtime("SetPoint(): Unknown region point"))?;
        argument_index += 1;
    }
    let offset = match (
        arguments.get(argument_index).and_then(lua_number),
        arguments.get(argument_index + 1).and_then(lua_number),
    ) {
        (Some(x), Some(y)) => (x, y),
        _ => (0.0, 0.0),
    };
    let record = create_anchor_record(lua, point, target, relative_point, offset)?;
    let anchors: Table = object.raw_get(anchors_key())?;
    anchors.raw_set(point_index(point), record)
}

fn resolve_region_name(lua: &Lua, object: &Table, source: &str) -> mlua::Result<String> {
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let parent = object
        .raw_get::<Option<usize>>(parent_key())?
        .map(|index| objects.raw_get::<Table>(index))
        .transpose()?;
    expand_dynamic_parent_name(lua, source, parent.as_ref())
}

fn create_anchor_record(
    lua: &Lua,
    point: UiPoint,
    target: Option<usize>,
    relative_point: UiPoint,
    offset: (f64, f64),
) -> mlua::Result<Table> {
    let record = lua.create_table()?;
    record.raw_set(1, point_name(point))?;
    record.raw_set(2, target)?;
    record.raw_set(3, point_name(relative_point))?;
    record.raw_set(4, offset.0)?;
    record.raw_set(5, offset.1)?;
    Ok(record)
}

fn lua_number(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(value) => Some(*value as f64),
        Value::Number(value) => Some(*value),
        _ => None,
    }
}

pub(super) fn parse_point(value: &str) -> Option<UiPoint> {
    if value.eq_ignore_ascii_case("TOPLEFT") {
        Some(UiPoint::TopLeft)
    } else if value.eq_ignore_ascii_case("TOP") {
        Some(UiPoint::Top)
    } else if value.eq_ignore_ascii_case("TOPRIGHT") {
        Some(UiPoint::TopRight)
    } else if value.eq_ignore_ascii_case("LEFT") {
        Some(UiPoint::Left)
    } else if value.eq_ignore_ascii_case("CENTER") {
        Some(UiPoint::Center)
    } else if value.eq_ignore_ascii_case("RIGHT") {
        Some(UiPoint::Right)
    } else if value.eq_ignore_ascii_case("BOTTOMLEFT") {
        Some(UiPoint::BottomLeft)
    } else if value.eq_ignore_ascii_case("BOTTOM") {
        Some(UiPoint::Bottom)
    } else if value.eq_ignore_ascii_case("BOTTOMRIGHT") {
        Some(UiPoint::BottomRight)
    } else {
        None
    }
}

const fn point_name(point: UiPoint) -> &'static str {
    match point {
        UiPoint::TopLeft => "TOPLEFT",
        UiPoint::Top => "TOP",
        UiPoint::TopRight => "TOPRIGHT",
        UiPoint::Left => "LEFT",
        UiPoint::Center => "CENTER",
        UiPoint::Right => "RIGHT",
        UiPoint::BottomLeft => "BOTTOMLEFT",
        UiPoint::Bottom => "BOTTOM",
        UiPoint::BottomRight => "BOTTOMRIGHT",
    }
}

const fn point_index(point: UiPoint) -> usize {
    match point {
        UiPoint::TopLeft => 1,
        UiPoint::Top => 2,
        UiPoint::TopRight => 3,
        UiPoint::Left => 4,
        UiPoint::Center => 5,
        UiPoint::Right => 6,
        UiPoint::BottomLeft => 7,
        UiPoint::Bottom => 8,
        UiPoint::BottomRight => 9,
    }
}

fn register_frame_event_methods(
    lua: &Lua,
    methods: &Table,
    manifest_kind: UiManifestKind,
) -> mlua::Result<()> {
    methods.raw_set(
        "RegisterEvent",
        lua.create_function(move |_, (object, name): (Table, String)| {
            let canonical = registered_event(manifest_kind, &name)?;
            if let Some(canonical) = canonical {
                event_table(&object)?.raw_set(canonical, true)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "UnregisterEvent",
        lua.create_function(move |_, (object, name): (Table, String)| {
            let canonical = registered_event(manifest_kind, &name)?;
            if let Some(canonical) = canonical {
                event_table(&object)?.raw_set(canonical, Value::Nil)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "RegisterAllEvents",
        lua.create_function(|_, object: Table| object.raw_set(all_events_key(), true))?,
    )?;
    methods.raw_set(
        "UnregisterAllEvents",
        lua.create_function(|lua, object: Table| {
            object.raw_set(events_key(), lua.create_table()?)?;
            object.raw_set(all_events_key(), false)
        })?,
    )?;
    methods.raw_set(
        "IsEventRegistered",
        lua.create_function(move |_, (object, name): (Table, String)| {
            let Some(canonical) = registered_event(manifest_kind, &name)? else {
                return Ok(None::<bool>);
            };
            if object.raw_get::<bool>(all_events_key())? {
                return Ok(Some(true));
            }
            event_table(&object)?.raw_get::<Option<bool>>(canonical)
        })?,
    )?;
    Ok(())
}

fn register_frame_backdrop_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetBackdropColor",
        lua.create_function(
            |lua, (object, red, green, blue, alpha): (Table, f64, f64, f64, Option<f64>)| {
                let color = clamped_color(red, green, blue, alpha);
                object.raw_set(backdrop_color_key(), lua.create_sequence_from(color)?)
            },
        )?,
    )?;
    methods.raw_set(
        "SetBackdropBorderColor",
        lua.create_function(
            |lua, (object, red, green, blue, alpha): (Table, f64, f64, f64, Option<f64>)| {
                let color = clamped_color(red, green, blue, alpha);
                object.raw_set(
                    backdrop_border_color_key(),
                    lua.create_sequence_from(color)?,
                )
            },
        )?,
    )?;
    Ok(())
}

pub(super) fn clamped_color(red: f64, green: f64, blue: f64, alpha: Option<f64>) -> [f64; 4] {
    [
        red.clamp(0.0, 1.0),
        green.clamp(0.0, 1.0),
        blue.clamp(0.0, 1.0),
        alpha.unwrap_or(1.0).clamp(0.0, 1.0),
    ]
}

fn registered_event(
    manifest_kind: UiManifestKind,
    name: &str,
) -> mlua::Result<Option<&'static str>> {
    match manifest_kind {
        UiManifestKind::Glue => Ok(canonical_glue_event(name)),
        UiManifestKind::Frame => Ok(canonical_frame_event(name)),
    }
}

fn event_table(object: &Table) -> mlua::Result<Table> {
    object.raw_get(events_key())
}

fn tree_font_strings(tree: &UiObjectTree<'_>, fonts: &FontCatalog) -> Vec<InitialFont> {
    tree.nodes()
        .iter()
        .map(|node| {
            if !matches!(
                node.kind(),
                UiObjectKind::FontString | UiObjectKind::EditBox
            ) {
                return InitialFont::default();
            }
            let mut initial = InitialFont::default();
            if node.kind() == UiObjectKind::EditBox {
                // FontString centers by default; the sibling EditBox native
                // begins left-justified unless XML or a Font overrides it.
                initial.justify_h = "LEFT".to_owned();
            }
            for layer in node.layers() {
                apply_initial_font_element(&mut initial, fonts, layer.element());
                if node.kind() == UiObjectKind::EditBox {
                    apply_initial_edit_box_element(
                        &mut initial,
                        fonts,
                        layer.document(),
                        layer.element(),
                    );
                }
            }
            initial
        })
        .collect()
}

fn apply_initial_font_element(
    initial: &mut InitialFont,
    fonts: &FontCatalog,
    element: &crate::XmlElement,
) {
    if let Some(inherits) = xml_attribute(element, "inherits") {
        for name in inherits.split(',').map(str::trim) {
            if let Some(definition) = fonts.definition(name) {
                initial.assigned = true;
                initial.object_name = Some(name.to_owned());
                apply_font_justification(initial, definition);
            }
        }
    }
    if let Some(font) = xml_attribute(element, "font") {
        initial.assigned = !font.is_empty();
        let definition = fonts.definition(font);
        initial.object_name = definition.map(|definition| definition.name().to_owned());
        if let Some(definition) = definition {
            apply_font_justification(initial, definition);
        }
    }
    if let Some(value) = xml_attribute(element, "justifyH")
        && stock_justify(value).is_some()
    {
        initial.justify_h = value.to_ascii_uppercase();
    }
    if let Some(value) = xml_attribute(element, "justifyV")
        && stock_justify(value).is_some()
    {
        initial.justify_v = value.to_ascii_uppercase();
    }
    if let Some(value) = xml_attribute(element, "spacing")
        && let Ok(spacing) = value.parse::<f64>()
        && spacing.is_finite()
    {
        initial.spacing = spacing;
    }
    if let Some(value) = xml_attribute(element, "wordwrap").and_then(stock_xml_bool) {
        initial.word_wrap = value;
    }
    if let Some(value) = xml_attribute(element, "nonspacewrap").and_then(stock_xml_bool) {
        initial.non_space_wrap = value;
    }
    if let Some(value) = xml_attribute(element, "maxLines").and_then(|value| value.parse().ok()) {
        initial.max_lines = value;
    }
    if let Some(reference) = xml_attribute(element, "text") {
        initial.text_reference = (!reference.is_empty()).then(|| reference.to_owned());
    }
}

fn apply_initial_edit_box_element(
    initial: &mut InitialFont,
    fonts: &FontCatalog,
    document: &crate::XmlDocument,
    element: &crate::XmlElement,
) {
    if let Some(value) = xml_attribute(element, "letters").and_then(|value| value.parse().ok()) {
        initial.edit_max_letters = value;
    }
    if let Some(value) = xml_attribute(element, "password").and_then(stock_xml_bool) {
        initial.edit_password = value;
    }
    if let Some(value) = xml_attribute(element, "multiLine").and_then(stock_xml_bool) {
        initial.edit_multiline = value;
    }
    for content in element.content() {
        let XmlContent::Element(index) = content else {
            continue;
        };
        let Some(child) = document.element(*index) else {
            continue;
        };
        if child.name() == "FontString" {
            apply_initial_font_element(initial, fonts, child);
        } else if child.name() == "TextInsets" {
            for content in child.content() {
                let XmlContent::Element(index) = content else {
                    continue;
                };
                let Some(inset) = document.element(*index) else {
                    continue;
                };
                if inset.name() != "AbsInset" {
                    continue;
                }
                for (slot, name) in ["left", "right", "top", "bottom"].into_iter().enumerate() {
                    if let Some(value) =
                        xml_attribute(inset, name).and_then(|value| value.parse::<f64>().ok())
                        && value.is_finite()
                    {
                        initial.edit_text_insets[slot] = value;
                    }
                }
            }
        } else if child.name() == "HighlightColor" {
            for (slot, name) in ["r", "g", "b", "a"].into_iter().enumerate() {
                if let Some(value) = xml_attribute(child, name)
                    .and_then(|value| value.parse::<f64>().ok())
                    .filter(|value| value.is_finite())
                {
                    initial.edit_highlight_color[slot] = value;
                }
            }
        }
    }
}

fn stock_xml_bool(value: &str) -> Option<bool> {
    if value == "1" || value.eq_ignore_ascii_case("true") {
        Some(true)
    } else if value == "0" || value.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

fn tree_buttons(tree: &UiObjectTree<'_>) -> Vec<InitialButton> {
    tree.nodes()
        .iter()
        .map(|node| {
            if !matches!(
                node.kind(),
                UiObjectKind::Button | UiObjectKind::CheckButton
            ) {
                return InitialButton::default();
            }
            let mut initial = InitialButton::default();
            for layer in node.layers() {
                if let Some(reference) = xml_attribute(layer.element(), "text") {
                    initial.text_reference = (!reference.is_empty()).then(|| reference.to_owned());
                }
                for content in layer.element().content() {
                    let XmlContent::Element(index) = content else {
                        continue;
                    };
                    let Some(child) = layer.document().element(*index) else {
                        continue;
                    };
                    let target = match child.name() {
                        "NormalFont" => &mut initial.normal_font,
                        "DisabledFont" => &mut initial.disabled_font,
                        "HighlightFont" => &mut initial.highlight_font,
                        _ => continue,
                    };
                    if let Some(style) = xml_attribute(child, "style") {
                        *target = (!style.is_empty()).then(|| style.to_owned());
                    }
                }
            }
            initial
        })
        .collect()
}

fn set_initial_font(
    lua: &Lua,
    button: &Table,
    key: LightUserData,
    name: Option<&str>,
) -> mlua::Result<()> {
    let Some(name) = name else {
        return Ok(());
    };
    let font = lua.globals().raw_get::<Option<Table>>(name)?;
    if let Some(font) = font {
        button.raw_set(key, font)?;
    }
    Ok(())
}

fn stock_text(lua: &Lua, reference: &str) -> mlua::Result<String> {
    let localized = lua.globals().raw_get::<Option<String>>(reference)?;
    Ok(localized
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| reference.to_owned()))
}

fn apply_font_justification(initial: &mut InitialFont, definition: &FontDefinition) {
    if let Some(spacing) = definition.spacing() {
        initial.spacing = f64::from(spacing);
    }
    if let Some(value) = definition.horizontal_justification() {
        initial.justify_h = match value {
            HorizontalJustification::Left => "LEFT",
            HorizontalJustification::Center => "CENTER",
            HorizontalJustification::Right => "RIGHT",
        }
        .to_owned();
    }
    if let Some(value) = definition.vertical_justification() {
        initial.justify_v = match value {
            VerticalJustification::Top => "TOP",
            VerticalJustification::Middle => "MIDDLE",
            VerticalJustification::Bottom => "BOTTOM",
        }
        .to_owned();
    }
}

fn tree_textures(
    tree: &UiObjectTree<'_>,
    textures: &UiTextureStatePlan,
) -> Result<Vec<InitialTexture>, UiScriptError> {
    tree.nodes()
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let initial = InitialTexture::default();
            if node.kind() != UiObjectKind::Texture {
                return Ok(initial);
            }
            textures
                .state(index)
                .map(|state| InitialTexture {
                    file: match state.file() {
                        Some(UiTextureFile::Asset(path)) => Some(path.as_str().to_owned()),
                        Some(UiTextureFile::Dynamic) | None => None,
                    },
                    coords: state.tex_coords().map(f64::from),
                    colors: state.vertex_colors().map(|color| color.map(f64::from)),
                    blend_mode: blend_mode_name(state.blend_mode()),
                    horizontal_tiling: state.horizontal_tiling(),
                    vertical_tiling: state.vertical_tiling(),
                    non_blocking: state.non_blocking(),
                    draw_layer: draw_layer_name(state.draw_layer()),
                    draw_sub_level: state.draw_sub_level(),
                })
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("texture {index} is outside the texture state plan"),
                })
        })
        .collect()
}

fn blend_mode_name(value: UiBlendMode) -> &'static str {
    match value {
        UiBlendMode::Blend => "BLEND",
        UiBlendMode::Add => "ADD",
    }
}

fn draw_layer_name(value: UiDrawLayer) -> &'static str {
    match value {
        UiDrawLayer::Background => "BACKGROUND",
        UiDrawLayer::Border => "BORDER",
        UiDrawLayer::Artwork => "ARTWORK",
        UiDrawLayer::Overlay => "OVERLAY",
        UiDrawLayer::Highlight => "HIGHLIGHT",
    }
}

fn frame_strata_name(value: UiFrameStrata) -> &'static str {
    match value {
        UiFrameStrata::Background => "BACKGROUND",
        UiFrameStrata::Low => "LOW",
        UiFrameStrata::Medium => "MEDIUM",
        UiFrameStrata::High => "HIGH",
        UiFrameStrata::Dialog => "DIALOG",
        UiFrameStrata::Fullscreen => "FULLSCREEN",
        UiFrameStrata::FullscreenDialog => "FULLSCREEN_DIALOG",
        UiFrameStrata::Tooltip => "TOOLTIP",
    }
}

fn parse_frame_strata_name(value: &str) -> Option<&'static str> {
    if value.eq_ignore_ascii_case("BACKGROUND") {
        Some("BACKGROUND")
    } else if value.eq_ignore_ascii_case("LOW") {
        Some("LOW")
    } else if value.eq_ignore_ascii_case("MEDIUM") {
        Some("MEDIUM")
    } else if value.eq_ignore_ascii_case("HIGH") {
        Some("HIGH")
    } else if value.eq_ignore_ascii_case("DIALOG") {
        Some("DIALOG")
    } else if value.eq_ignore_ascii_case("FULLSCREEN") {
        Some("FULLSCREEN")
    } else if value.eq_ignore_ascii_case("FULLSCREEN_DIALOG") {
        Some("FULLSCREEN_DIALOG")
    } else if value.eq_ignore_ascii_case("TOOLTIP") {
        Some("TOOLTIP")
    } else {
        None
    }
}

fn xml_attribute<'a>(element: &'a crate::XmlElement, name: &str) -> Option<&'a str> {
    element
        .attributes()
        .iter()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn widget_region_key(role: UiObjectRole) -> Option<LightUserData> {
    match role {
        UiObjectRole::ScrollChild => Some(scroll_child_key()),
        UiObjectRole::ButtonText => Some(button_text_key()),
        UiObjectRole::NormalTexture => Some(normal_texture_key()),
        UiObjectRole::PushedTexture => Some(pushed_texture_key()),
        UiObjectRole::DisabledTexture => Some(disabled_texture_key()),
        UiObjectRole::HighlightTexture => Some(highlight_texture_key()),
        _ => None,
    }
}

fn dynamic_widget_region_key(role: &str) -> Option<LightUserData> {
    match role {
        "scroll_child" => Some(scroll_child_key()),
        "button_text" => Some(button_text_key()),
        "normal_texture" => Some(normal_texture_key()),
        "pushed_texture" => Some(pushed_texture_key()),
        "disabled_texture" => Some(disabled_texture_key()),
        "highlight_texture" => Some(highlight_texture_key()),
        _ => None,
    }
}

fn object_type_name(kind: UiObjectKind) -> &'static str {
    match kind {
        UiObjectKind::Frame => "Frame",
        UiObjectKind::Button => "Button",
        UiObjectKind::CheckButton => "CheckButton",
        UiObjectKind::ColorSelect => "ColorSelect",
        UiObjectKind::Cooldown => "Cooldown",
        UiObjectKind::EditBox => "EditBox",
        UiObjectKind::FontString => "FontString",
        UiObjectKind::GameTooltip => "GameTooltip",
        UiObjectKind::MessageFrame => "MessageFrame",
        UiObjectKind::Minimap => "Minimap",
        UiObjectKind::QuestPoiFrame => "QuestPOIFrame",
        UiObjectKind::Model => "Model",
        UiObjectKind::ModelFfx => "ModelFFX",
        UiObjectKind::MovieFrame => "MovieFrame",
        UiObjectKind::ScrollFrame => "ScrollFrame",
        UiObjectKind::ScrollingMessageFrame => "ScrollingMessageFrame",
        UiObjectKind::SimpleHtml => "SimpleHTML",
        UiObjectKind::Slider => "Slider",
        UiObjectKind::StatusBar => "StatusBar",
        UiObjectKind::Texture => "Texture",
        UiObjectKind::WorldFrame => "WorldFrame",
    }
}

fn object_role_name(role: UiObjectRole) -> &'static str {
    match role {
        UiObjectRole::Object => "object",
        UiObjectRole::ScrollChild => "scroll_child",
        UiObjectRole::ButtonText => "button_text",
        UiObjectRole::NormalTexture => "normal_texture",
        UiObjectRole::PushedTexture => "pushed_texture",
        UiObjectRole::DisabledTexture => "disabled_texture",
        UiObjectRole::HighlightTexture => "highlight_texture",
        UiObjectRole::CheckedTexture => "checked_texture",
        UiObjectRole::DisabledCheckedTexture => "disabled_checked_texture",
        UiObjectRole::ThumbTexture => "thumb_texture",
    }
}

fn is_object_type(exact: &str, candidate: &str) -> bool {
    if exact.eq_ignore_ascii_case(candidate) {
        return true;
    }
    if candidate.eq_ignore_ascii_case("Region") {
        return true;
    }
    if candidate.eq_ignore_ascii_case("Frame") {
        return !matches!(exact, "Texture" | "FontString");
    }
    (candidate.eq_ignore_ascii_case("Button") && exact == "CheckButton")
        || (candidate.eq_ignore_ascii_case("Model") && exact == "ModelFFX")
        || (candidate.eq_ignore_ascii_case("MessageFrame") && exact == "ScrollingMessageFrame")
}

fn is_frame_object(kind: UiObjectKind) -> bool {
    !matches!(kind, UiObjectKind::Texture | UiObjectKind::FontString)
}

fn resolved_frame_flag(
    values: &[Option<bool>],
    node_index: usize,
    kind: UiObjectKind,
    label: &str,
) -> Result<Option<bool>, UiScriptError> {
    if !is_frame_object(kind) {
        return Ok(None);
    }
    values
        .get(node_index)
        .copied()
        .flatten()
        .map(Some)
        .ok_or_else(|| UiScriptError::Plan {
            message: format!("frame object {node_index} has no resolved {label} state"),
        })
}

fn is_enabled_control(kind: UiObjectKind) -> bool {
    matches!(
        kind,
        UiObjectKind::Button | UiObjectKind::CheckButton | UiObjectKind::Slider
    )
}

const fn object_kind_index(kind: UiObjectKind) -> usize {
    match kind {
        UiObjectKind::Frame => 0,
        UiObjectKind::Button => 1,
        UiObjectKind::CheckButton => 2,
        UiObjectKind::ColorSelect => 3,
        UiObjectKind::Cooldown => 4,
        UiObjectKind::EditBox => 5,
        UiObjectKind::FontString => 6,
        UiObjectKind::GameTooltip => 7,
        UiObjectKind::MessageFrame => 8,
        UiObjectKind::Minimap => 9,
        UiObjectKind::QuestPoiFrame => 10,
        UiObjectKind::Model => 11,
        UiObjectKind::ModelFfx => 12,
        UiObjectKind::MovieFrame => 13,
        UiObjectKind::ScrollFrame => 14,
        UiObjectKind::ScrollingMessageFrame => 15,
        UiObjectKind::SimpleHtml => 16,
        UiObjectKind::Slider => 17,
        UiObjectKind::StatusBar => 18,
        UiObjectKind::Texture => 19,
        UiObjectKind::WorldFrame => 20,
    }
}

pub(super) fn name_key() -> LightUserData {
    hidden_key(&NAME_TOKEN)
}

pub(super) fn type_key() -> LightUserData {
    hidden_key(&TYPE_TOKEN)
}

pub(super) fn role_key() -> LightUserData {
    hidden_key(&ROLE_TOKEN)
}

pub(super) fn parent_key() -> LightUserData {
    hidden_key(&PARENT_TOKEN)
}

fn events_key() -> LightUserData {
    hidden_key(&EVENTS_TOKEN)
}

fn all_events_key() -> LightUserData {
    hidden_key(&ALL_EVENTS_TOKEN)
}

pub(super) fn width_key() -> LightUserData {
    hidden_key(&WIDTH_TOKEN)
}

pub(super) fn height_key() -> LightUserData {
    hidden_key(&HEIGHT_TOKEN)
}

pub(super) fn backdrop_color_key() -> LightUserData {
    hidden_key(&BACKDROP_COLOR_TOKEN)
}

pub(super) fn backdrop_border_color_key() -> LightUserData {
    hidden_key(&BACKDROP_BORDER_COLOR_TOKEN)
}

pub(super) fn index_key() -> LightUserData {
    hidden_key(&INDEX_TOKEN)
}

pub(super) fn anchors_key() -> LightUserData {
    hidden_key(&ANCHORS_TOKEN)
}

pub(super) fn enabled_key() -> LightUserData {
    hidden_key(&ENABLED_TOKEN)
}

pub(super) fn horizontal_scroll_key() -> LightUserData {
    hidden_key(&HORIZONTAL_SCROLL_TOKEN)
}

pub(super) fn vertical_scroll_key() -> LightUserData {
    hidden_key(&VERTICAL_SCROLL_TOKEN)
}

pub(super) fn horizontal_scroll_range_key() -> LightUserData {
    hidden_key(&HORIZONTAL_SCROLL_RANGE_TOKEN)
}

pub(super) fn vertical_scroll_range_key() -> LightUserData {
    hidden_key(&VERTICAL_SCROLL_RANGE_TOKEN)
}

pub(super) fn scroll_child_key() -> LightUserData {
    hidden_key(&SCROLL_CHILD_TOKEN)
}

pub(super) fn edit_focused_key() -> LightUserData {
    hidden_key(&EDIT_FOCUSED_TOKEN)
}

fn edit_alt_arrow_key() -> LightUserData {
    hidden_key(&EDIT_ALT_ARROW_TOKEN)
}

fn edit_history_lines_key() -> LightUserData {
    hidden_key(&EDIT_HISTORY_LINES_TOKEN)
}

fn edit_history_key() -> LightUserData {
    hidden_key(&EDIT_HISTORY_TOKEN)
}

fn edit_max_letters_key() -> LightUserData {
    hidden_key(&EDIT_MAX_LETTERS_TOKEN)
}

fn edit_max_bytes_key() -> LightUserData {
    hidden_key(&EDIT_MAX_BYTES_TOKEN)
}

pub(super) fn edit_cursor_key() -> LightUserData {
    hidden_key(&EDIT_CURSOR_TOKEN)
}

pub(super) fn edit_selection_start_key() -> LightUserData {
    hidden_key(&EDIT_SELECTION_START_TOKEN)
}

pub(super) fn edit_selection_end_key() -> LightUserData {
    hidden_key(&EDIT_SELECTION_END_TOKEN)
}

fn edit_blink_speed_key() -> LightUserData {
    hidden_key(&EDIT_BLINK_SPEED_TOKEN)
}

pub(super) fn edit_caret_elapsed_key() -> LightUserData {
    hidden_key(&EDIT_CARET_ELAPSED_TOKEN)
}

pub(super) fn edit_caret_visible_key() -> LightUserData {
    hidden_key(&EDIT_CARET_VISIBLE_TOKEN)
}

pub(super) fn edit_password_key() -> LightUserData {
    hidden_key(&EDIT_PASSWORD_TOKEN)
}

fn edit_numeric_key() -> LightUserData {
    hidden_key(&EDIT_NUMERIC_TOKEN)
}

pub(super) fn edit_multi_line_key() -> LightUserData {
    hidden_key(&EDIT_MULTI_LINE_TOKEN)
}

fn edit_count_invisible_key() -> LightUserData {
    hidden_key(&EDIT_COUNT_INVISIBLE_TOKEN)
}

fn edit_auto_focus_key() -> LightUserData {
    hidden_key(&EDIT_AUTO_FOCUS_TOKEN)
}

pub(super) fn edit_text_insets_key() -> LightUserData {
    hidden_key(&EDIT_TEXT_INSETS_TOKEN)
}

pub(super) fn edit_highlight_color_key() -> LightUserData {
    hidden_key(&EDIT_HIGHLIGHT_COLOR_TOKEN)
}

fn frame_clamped_key() -> LightUserData {
    hidden_key(&FRAME_CLAMPED_TOKEN)
}

fn frame_clamp_insets_key() -> LightUserData {
    hidden_key(&FRAME_CLAMP_INSETS_TOKEN)
}

pub(super) fn hit_rect_insets_key() -> LightUserData {
    hidden_key(&HIT_RECT_INSETS_TOKEN)
}

pub(super) fn font_shadow_offset_key() -> LightUserData {
    hidden_key(&FONT_SHADOW_OFFSET_TOKEN)
}

pub(super) fn font_shadow_color_key() -> LightUserData {
    hidden_key(&FONT_SHADOW_COLOR_TOKEN)
}

fn frame_movable_key() -> LightUserData {
    hidden_key(&FRAME_MOVABLE_TOKEN)
}

fn frame_resizable_key() -> LightUserData {
    hidden_key(&FRAME_RESIZABLE_TOKEN)
}

fn frame_top_level_key() -> LightUserData {
    hidden_key(&FRAME_TOP_LEVEL_TOKEN)
}

fn frame_user_placed_key() -> LightUserData {
    hidden_key(&FRAME_USER_PLACED_TOKEN)
}

fn frame_dont_save_position_key() -> LightUserData {
    hidden_key(&FRAME_DONT_SAVE_POSITION_TOKEN)
}

pub(super) fn slider_min_key() -> LightUserData {
    hidden_key(&SLIDER_MIN_TOKEN)
}

pub(super) fn slider_max_key() -> LightUserData {
    hidden_key(&SLIDER_MAX_TOKEN)
}

pub(super) fn slider_value_key() -> LightUserData {
    hidden_key(&SLIDER_VALUE_TOKEN)
}

pub(super) fn slider_step_key() -> LightUserData {
    hidden_key(&SLIDER_STEP_TOKEN)
}

pub(super) fn slider_orientation_key() -> LightUserData {
    hidden_key(&SLIDER_ORIENTATION_TOKEN)
}

pub(super) fn shown_key() -> LightUserData {
    hidden_key(&SHOWN_TOKEN)
}

pub(super) fn alpha_key() -> LightUserData {
    hidden_key(&ALPHA_TOKEN)
}

pub(super) fn scale_key() -> LightUserData {
    hidden_key(&SCALE_TOKEN)
}

fn id_key() -> LightUserData {
    hidden_key(&ID_TOKEN)
}

pub(super) fn normal_font_key() -> LightUserData {
    hidden_key(&NORMAL_FONT_TOKEN)
}

pub(super) fn disabled_font_key() -> LightUserData {
    hidden_key(&DISABLED_FONT_TOKEN)
}

pub(super) fn disabled_text_color_key() -> LightUserData {
    hidden_key(&DISABLED_TEXT_COLOR_TOKEN)
}

pub(super) fn auto_text_width_key() -> LightUserData {
    hidden_key(&AUTO_TEXT_WIDTH_TOKEN)
}

pub(super) fn auto_text_height_key() -> LightUserData {
    hidden_key(&AUTO_TEXT_HEIGHT_TOKEN)
}

pub(super) fn word_wrap_key() -> LightUserData {
    hidden_key(&WORD_WRAP_TOKEN)
}

pub(super) fn non_space_wrap_key() -> LightUserData {
    hidden_key(&NON_SPACE_WRAP_TOKEN)
}

pub(super) fn max_text_lines_key() -> LightUserData {
    hidden_key(&MAX_TEXT_LINES_TOKEN)
}

pub(super) fn highlight_font_key() -> LightUserData {
    hidden_key(&HIGHLIGHT_FONT_TOKEN)
}

pub(super) fn text_key() -> LightUserData {
    hidden_key(&TEXT_TOKEN)
}

pub(super) fn highlight_locked_key() -> LightUserData {
    hidden_key(&HIGHLIGHT_LOCKED_TOKEN)
}

pub(super) fn hovered_key() -> LightUserData {
    hidden_key(&HOVERED_TOKEN)
}

pub(super) fn font_set_key() -> LightUserData {
    hidden_key(&FONT_SET_TOKEN)
}

pub(super) fn font_object_key() -> LightUserData {
    hidden_key(&FONT_OBJECT_TOKEN)
}

pub(super) fn tex_coord_key() -> LightUserData {
    hidden_key(&TEX_COORD_TOKEN)
}

pub(super) fn justify_h_key() -> LightUserData {
    hidden_key(&JUSTIFY_H_TOKEN)
}

pub(super) fn justify_v_key() -> LightUserData {
    hidden_key(&JUSTIFY_V_TOKEN)
}

pub(super) fn checked_key() -> LightUserData {
    hidden_key(&CHECKED_TOKEN)
}

pub(super) fn model_camera_key() -> LightUserData {
    hidden_key(&MODEL_CAMERA_TOKEN)
}

pub(super) fn model_sequence_key() -> LightUserData {
    hidden_key(&MODEL_SEQUENCE_TOKEN)
}

pub(super) fn model_file_key() -> LightUserData {
    hidden_key(&MODEL_FILE_TOKEN)
}

pub(super) fn click_action_key() -> LightUserData {
    hidden_key(&CLICK_ACTION_TOKEN)
}

pub(super) fn button_pressed_key() -> LightUserData {
    hidden_key(&BUTTON_PRESSED_TOKEN)
}

pub(super) fn button_state_locked_key() -> LightUserData {
    hidden_key(&BUTTON_STATE_LOCKED_TOKEN)
}

pub(super) fn drag_button_key() -> LightUserData {
    hidden_key(&DRAG_BUTTON_TOKEN)
}

pub(super) fn spacing_key() -> LightUserData {
    hidden_key(&SPACING_TOKEN)
}

pub(super) fn model_sequence_time_sequence_key() -> LightUserData {
    hidden_key(&MODEL_SEQUENCE_TIME_SEQUENCE_TOKEN)
}

pub(super) fn model_sequence_time_key() -> LightUserData {
    hidden_key(&MODEL_SEQUENCE_TIME_TOKEN)
}

pub(super) fn frame_level_key() -> LightUserData {
    hidden_key(&FRAME_LEVEL_TOKEN)
}

pub(super) fn model_scale_key() -> LightUserData {
    hidden_key(&MODEL_SCALE_TOKEN)
}

pub(super) fn model_fog_color_key() -> LightUserData {
    hidden_key(&MODEL_FOG_COLOR_TOKEN)
}

pub(super) fn model_fog_near_key() -> LightUserData {
    hidden_key(&MODEL_FOG_NEAR_TOKEN)
}

pub(super) fn model_fog_far_key() -> LightUserData {
    hidden_key(&MODEL_FOG_FAR_TOKEN)
}

pub(super) fn model_glow_key() -> LightUserData {
    hidden_key(&MODEL_GLOW_TOKEN)
}

pub(super) fn model_background_light_live_key() -> LightUserData {
    hidden_key(&MODEL_BACKGROUND_LIGHT_LIVE_TOKEN)
}

pub(super) fn model_background_light_ghost_key() -> LightUserData {
    hidden_key(&MODEL_BACKGROUND_LIGHT_GHOST_TOKEN)
}

pub(super) fn model_character_light_live_key() -> LightUserData {
    hidden_key(&MODEL_CHARACTER_LIGHT_LIVE_TOKEN)
}

pub(super) fn model_character_light_ghost_key() -> LightUserData {
    hidden_key(&MODEL_CHARACTER_LIGHT_GHOST_TOKEN)
}

pub(super) fn model_pet_light_live_key() -> LightUserData {
    hidden_key(&MODEL_PET_LIGHT_LIVE_TOKEN)
}

pub(super) fn model_pet_light_ghost_key() -> LightUserData {
    hidden_key(&MODEL_PET_LIGHT_GHOST_TOKEN)
}

pub(super) fn keyboard_enabled_key() -> LightUserData {
    hidden_key(&KEYBOARD_ENABLED_TOKEN)
}

pub(super) fn texture_color_key() -> LightUserData {
    hidden_key(&TEXTURE_COLOR_TOKEN)
}

pub(super) fn texture_file_key() -> LightUserData {
    hidden_key(&TEXTURE_FILE_TOKEN)
}

pub(super) fn portrait_unit_key() -> LightUserData {
    hidden_key(&PORTRAIT_UNIT_TOKEN)
}

pub(super) fn texture_solid_color_key() -> LightUserData {
    hidden_key(&TEXTURE_SOLID_COLOR_TOKEN)
}

pub(super) fn texture_blend_mode_key() -> LightUserData {
    hidden_key(&TEXTURE_BLEND_MODE_TOKEN)
}

pub(super) fn horizontal_tiling_key() -> LightUserData {
    hidden_key(&HORIZONTAL_TILING_TOKEN)
}

pub(super) fn vertical_tiling_key() -> LightUserData {
    hidden_key(&VERTICAL_TILING_TOKEN)
}

fn movie_subtitles_key() -> LightUserData {
    hidden_key(&MOVIE_SUBTITLES_TOKEN)
}

pub(super) fn non_blocking_key() -> LightUserData {
    hidden_key(&NON_BLOCKING_TOKEN)
}

pub(super) fn desaturated_key() -> LightUserData {
    hidden_key(&DESATURATED_TOKEN)
}

pub(super) fn draw_layer_key() -> LightUserData {
    hidden_key(&DRAW_LAYER_TOKEN)
}

pub(super) fn draw_sub_level_key() -> LightUserData {
    hidden_key(&DRAW_SUB_LEVEL_TOKEN)
}

pub(super) fn frame_strata_key() -> LightUserData {
    hidden_key(&FRAME_STRATA_TOKEN)
}

fn frame_depth_key() -> LightUserData {
    hidden_key(&FRAME_DEPTH_TOKEN)
}

fn ignore_depth_key() -> LightUserData {
    hidden_key(&IGNORE_DEPTH_TOKEN)
}

pub(super) fn mouse_enabled_key() -> LightUserData {
    hidden_key(&MOUSE_ENABLED_TOKEN)
}

pub(super) fn mouse_wheel_enabled_key() -> LightUserData {
    hidden_key(&MOUSE_WHEEL_ENABLED_TOKEN)
}

fn attributes_key() -> LightUserData {
    hidden_key(&ATTRIBUTES_TOKEN)
}

fn status_bar_color_key() -> LightUserData {
    hidden_key(&STATUS_BAR_COLOR_TOKEN)
}

fn status_bar_texture_key() -> LightUserData {
    hidden_key(&STATUS_BAR_TEXTURE_TOKEN)
}

pub(super) fn text_color_key() -> LightUserData {
    hidden_key(&TEXT_COLOR_TOKEN)
}

pub(super) fn tooltip_owner_key() -> LightUserData {
    hidden_key(&TOOLTIP_OWNER_TOKEN)
}

pub(super) fn tooltip_anchor_key() -> LightUserData {
    hidden_key(&TOOLTIP_ANCHOR_TOKEN)
}

pub(super) fn tooltip_offset_x_key() -> LightUserData {
    hidden_key(&TOOLTIP_OFFSET_X_TOKEN)
}

pub(super) fn tooltip_offset_y_key() -> LightUserData {
    hidden_key(&TOOLTIP_OFFSET_Y_TOKEN)
}

pub(super) fn tooltip_padding_key() -> LightUserData {
    hidden_key(&TOOLTIP_PADDING_TOKEN)
}

pub(super) fn font_face_key() -> LightUserData {
    hidden_key(&FONT_FACE_TOKEN)
}

pub(super) fn font_height_key() -> LightUserData {
    hidden_key(&FONT_HEIGHT_TOKEN)
}

pub(super) fn font_flags_key() -> LightUserData {
    hidden_key(&FONT_FLAGS_TOKEN)
}

fn normal_texture_key() -> LightUserData {
    hidden_key(&NORMAL_TEXTURE_TOKEN)
}

fn pushed_texture_key() -> LightUserData {
    hidden_key(&PUSHED_TEXTURE_TOKEN)
}

fn disabled_texture_key() -> LightUserData {
    hidden_key(&DISABLED_TEXTURE_TOKEN)
}

fn highlight_texture_key() -> LightUserData {
    hidden_key(&HIGHLIGHT_TEXTURE_TOKEN)
}

fn script_handlers_key() -> LightUserData {
    hidden_key(&SCRIPT_HANDLERS_TOKEN)
}

fn button_text_key() -> LightUserData {
    hidden_key(&BUTTON_TEXT_TOKEN)
}

fn live_state_generation(lua: &Lua) -> mlua::Result<u64> {
    lua.named_registry_value(LIVE_STATE_GENERATION_REGISTRY)
}

pub(crate) fn mark_live_state_changed(lua: &Lua) -> mlua::Result<()> {
    let generation = live_state_generation(lua)?;
    lua.set_named_registry_value(LIVE_STATE_GENERATION_REGISTRY, generation.wrapping_add(1))
}

fn hidden_key(token: &'static u8) -> LightUserData {
    LightUserData(std::ptr::from_ref(token).cast_mut().cast::<c_void>())
}

fn execution_error(label: impl Into<String>, error: mlua::Error) -> UiScriptError {
    UiScriptError::Execution {
        label: label.into(),
        message: error.to_string(),
    }
}
