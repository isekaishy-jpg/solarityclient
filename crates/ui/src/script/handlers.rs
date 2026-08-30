//! Typed stock XML callbacks and shared compiled Lua functions.

use std::collections::HashMap;

use mlua::{Function, Lua, RegistryKey};

use crate::{UiObjectKind, UiObjectTree, UiScriptError, XmlContent, XmlDocument, XmlElement};

const HANDLER_COUNT: usize = UiScriptHandler::TooltipSetAchievement as usize + 1;

/// A callback slot accepted by build-12340 frame and widget implementations.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(usize)]
pub enum UiScriptHandler {
    /// Generic event dispatch with the event name and variable payload.
    Event,
    /// Object construction callback.
    Load,
    /// Region size change.
    SizeChanged,
    /// Per-frame update.
    Update,
    /// Local visibility changed to shown.
    Show,
    /// Local visibility changed to hidden.
    Hide,
    /// Pointer entered the frame.
    Enter,
    /// Pointer left the frame.
    Leave,
    /// Mouse button pressed.
    MouseDown,
    /// Mouse button released.
    MouseUp,
    /// Mouse wheel motion.
    MouseWheel,
    /// Drag operation began.
    DragStart,
    /// Drag operation ended.
    DragStop,
    /// A dragged value was received.
    ReceiveDrag,
    /// Text character input.
    Char,
    /// Keyboard key pressed.
    KeyDown,
    /// Keyboard key released.
    KeyUp,
    /// Script attribute mutation.
    AttributeChanged,
    /// Frame or button became enabled.
    Enable,
    /// Frame or button became disabled.
    Disable,
    /// Button callback before its principal click handler.
    PreClick,
    /// Principal button click callback.
    Click,
    /// Button callback after its principal click handler.
    PostClick,
    /// Button double-click callback.
    DoubleClick,
    /// Slider or status-bar value mutation.
    ValueChanged,
    /// Slider or status-bar range mutation.
    MinMaxChanged,
    /// Model presentation update.
    UpdateModel,
    /// Model animation completion.
    AnimFinished,
    /// Edit-box enter key.
    EnterPressed,
    /// Edit-box escape key.
    EscapePressed,
    /// Edit-box space key.
    SpacePressed,
    /// Edit-box tab key.
    TabPressed,
    /// Edit-box text mutation.
    TextChanged,
    /// Edit-box text replacement.
    TextSet,
    /// Edit-box cursor geometry mutation.
    CursorChanged,
    /// Input language mutation.
    InputLanguageChanged,
    /// Edit-box focus acquisition.
    EditFocusGained,
    /// Edit-box focus loss.
    EditFocusLost,
    /// Input-method composition text.
    CharComposition,
    /// Horizontal scroll offset mutation.
    HorizontalScroll,
    /// Vertical scroll offset mutation.
    VerticalScroll,
    /// Scrollable range mutation.
    ScrollRangeChanged,
    /// Color selection mutation.
    ColorSelect,
    /// Pointer entered a hyperlink.
    HyperlinkEnter,
    /// Pointer left a hyperlink.
    HyperlinkLeave,
    /// Hyperlink click.
    HyperlinkClick,
    /// Scrolling-message position mutation.
    MessageScrollChanged,
    /// Movie playback completion.
    MovieFinished,
    /// Movie subtitle became visible.
    MovieShowSubtitle,
    /// Movie subtitle became hidden.
    MovieHideSubtitle,
    /// Tooltip requested its default anchor.
    TooltipSetDefaultAnchor,
    /// Tooltip content was cleared.
    TooltipCleared,
    /// Tooltip money content was added.
    TooltipAddMoney,
    /// Tooltip unit content was selected.
    TooltipSetUnit,
    /// Tooltip item content was selected.
    TooltipSetItem,
    /// Tooltip spell content was selected.
    TooltipSetSpell,
    /// Tooltip quest content was selected.
    TooltipSetQuest,
    /// Tooltip achievement content was selected.
    TooltipSetAchievement,
}

impl UiScriptHandler {
    /// Returns the exact XML callback spelling.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Event => "OnEvent",
            Self::Load => "OnLoad",
            Self::SizeChanged => "OnSizeChanged",
            Self::Update => "OnUpdate",
            Self::Show => "OnShow",
            Self::Hide => "OnHide",
            Self::Enter => "OnEnter",
            Self::Leave => "OnLeave",
            Self::MouseDown => "OnMouseDown",
            Self::MouseUp => "OnMouseUp",
            Self::MouseWheel => "OnMouseWheel",
            Self::DragStart => "OnDragStart",
            Self::DragStop => "OnDragStop",
            Self::ReceiveDrag => "OnReceiveDrag",
            Self::Char => "OnChar",
            Self::KeyDown => "OnKeyDown",
            Self::KeyUp => "OnKeyUp",
            Self::AttributeChanged => "OnAttributeChanged",
            Self::Enable => "OnEnable",
            Self::Disable => "OnDisable",
            Self::PreClick => "PreClick",
            Self::Click => "OnClick",
            Self::PostClick => "PostClick",
            Self::DoubleClick => "OnDoubleClick",
            Self::ValueChanged => "OnValueChanged",
            Self::MinMaxChanged => "OnMinMaxChanged",
            Self::UpdateModel => "OnUpdateModel",
            Self::AnimFinished => "OnAnimFinished",
            Self::EnterPressed => "OnEnterPressed",
            Self::EscapePressed => "OnEscapePressed",
            Self::SpacePressed => "OnSpacePressed",
            Self::TabPressed => "OnTabPressed",
            Self::TextChanged => "OnTextChanged",
            Self::TextSet => "OnTextSet",
            Self::CursorChanged => "OnCursorChanged",
            Self::InputLanguageChanged => "OnInputLanguageChanged",
            Self::EditFocusGained => "OnEditFocusGained",
            Self::EditFocusLost => "OnEditFocusLost",
            Self::CharComposition => "OnCharComposition",
            Self::HorizontalScroll => "OnHorizontalScroll",
            Self::VerticalScroll => "OnVerticalScroll",
            Self::ScrollRangeChanged => "OnScrollRangeChanged",
            Self::ColorSelect => "OnColorSelect",
            Self::HyperlinkEnter => "OnHyperlinkEnter",
            Self::HyperlinkLeave => "OnHyperlinkLeave",
            Self::HyperlinkClick => "OnHyperlinkClick",
            Self::MessageScrollChanged => "OnMessageScrollChanged",
            Self::MovieFinished => "OnMovieFinished",
            Self::MovieShowSubtitle => "OnMovieShowSubtitle",
            Self::MovieHideSubtitle => "OnMovieHideSubtitle",
            Self::TooltipSetDefaultAnchor => "OnTooltipSetDefaultAnchor",
            Self::TooltipCleared => "OnTooltipCleared",
            Self::TooltipAddMoney => "OnTooltipAddMoney",
            Self::TooltipSetUnit => "OnTooltipSetUnit",
            Self::TooltipSetItem => "OnTooltipSetItem",
            Self::TooltipSetSpell => "OnTooltipSetSpell",
            Self::TooltipSetQuest => "OnTooltipSetQuest",
            Self::TooltipSetAchievement => "OnTooltipSetAchievement",
        }
    }

    const fn parameters(self) -> &'static str {
        match self {
            Self::Event => "self,event,...",
            Self::SizeChanged => "self,w,h",
            Self::Update => "self,elapsed",
            Self::Enter | Self::Leave => "self,motion",
            Self::MouseDown | Self::MouseUp | Self::DragStart | Self::DoubleClick => "self,button",
            Self::MouseWheel => "self,delta",
            Self::Char | Self::CharComposition => "self,text",
            Self::KeyDown | Self::KeyUp => "self,key",
            Self::AttributeChanged => "self,name,value",
            Self::PreClick | Self::Click | Self::PostClick => "self,button,down",
            Self::ValueChanged => "self,value",
            Self::MinMaxChanged => "self,min,max",
            Self::TextChanged => "self,userInput",
            Self::CursorChanged => "self,x,y,w,h",
            Self::InputLanguageChanged => "self,language",
            Self::HorizontalScroll | Self::VerticalScroll => "self,offset",
            Self::ScrollRangeChanged => "self,xrange,yrange",
            Self::ColorSelect => "self,r,g,b",
            Self::HyperlinkEnter | Self::HyperlinkLeave => "self,link,text",
            Self::HyperlinkClick => "self,link,text,button",
            Self::MovieShowSubtitle => "self,text",
            Self::TooltipAddMoney => "self,cost,maxcost",
            _ => "self",
        }
    }
}

/// The source selected for one final callback slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiScriptTarget {
    /// A body compiled once from an XML handler element.
    Compiled(u32),
    /// A global Lua function resolved when ordered execution reaches the object.
    Global(String),
}

/// One final callback binding after XML inheritance replacement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiScriptBinding {
    handler: UiScriptHandler,
    target: UiScriptTarget,
}

impl UiScriptBinding {
    /// Returns the typed callback slot.
    #[must_use]
    pub const fn handler(&self) -> UiScriptHandler {
        self.handler
    }

    /// Returns the compiled-body index or deferred global function name.
    #[must_use]
    pub const fn target(&self) -> &UiScriptTarget {
        &self.target
    }
}

/// Range of final callback bindings belonging to one object node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiScriptNode {
    first_binding: usize,
    binding_count: usize,
}

impl UiScriptNode {
    /// Returns the number of active callback slots on this object.
    #[must_use]
    pub const fn binding_count(self) -> usize {
        self.binding_count
    }
}

/// Flat callback bindings with deduplicated Lua registry functions.
pub struct UiScriptPlan {
    nodes: Vec<UiScriptNode>,
    bindings: Vec<UiScriptBinding>,
    functions: Vec<RegistryKey>,
    declaration_count: usize,
}

impl UiScriptPlan {
    /// Applies stock handler replacement and compiles exact callback wrappers.
    ///
    /// Identical inherited XML handler elements share one Lua function. Each
    /// object still owns an independent callback slot, so later `SetScript`
    /// mutation does not require cloning the compiled closure.
    ///
    /// # Errors
    ///
    /// Returns [`UiScriptError`] for a callback unsupported by its concrete
    /// widget, a wrapper compilation failure, or a compact-index overflow.
    pub fn from_tree(tree: &UiObjectTree<'_>, lua: &Lua) -> Result<Self, UiScriptError> {
        let mut plan = Self {
            nodes: Vec::with_capacity(tree.nodes().len()),
            bindings: Vec::new(),
            functions: Vec::new(),
            declaration_count: 0,
        };
        let mut compiled = HashMap::new();

        for object in tree.nodes() {
            let mut slots: [Option<UiScriptTarget>; HANDLER_COUNT] = std::array::from_fn(|_| None);
            for layer in object.layers() {
                let Some(scripts) = child_named(layer.document(), layer.element(), "Scripts")
                else {
                    continue;
                };
                for content in scripts.content() {
                    let XmlContent::Element(index) = content else {
                        continue;
                    };
                    let element =
                        layer
                            .document()
                            .element(*index)
                            .ok_or_else(|| UiScriptError::Plan {
                                message: "script child index is outside the XML arena".to_owned(),
                            })?;
                    plan.declaration_count += 1;
                    let handler = handler_for(object.kind(), element.name()).ok_or_else(|| {
                        UiScriptError::Handler {
                            path: layer.source_path().clone(),
                            object: object.name().unwrap_or("<unnamed>").to_owned(),
                            handler: element.name().to_owned(),
                        }
                    })?;
                    slots[handler as usize] = compile_target(
                        &mut plan.functions,
                        &mut compiled,
                        lua,
                        layer.source_path(),
                        object.name().unwrap_or("<unnamed>"),
                        handler,
                        element,
                    )?;
                }
            }

            let first_binding = plan.bindings.len();
            for (index, target) in slots.into_iter().enumerate() {
                let Some(target) = target else {
                    continue;
                };
                let handler = handler_from_index(index).ok_or_else(|| UiScriptError::Plan {
                    message: format!("handler slot {index} has no typed callback"),
                })?;
                plan.bindings.push(UiScriptBinding { handler, target });
            }
            plan.nodes.push(UiScriptNode {
                first_binding,
                binding_count: plan.bindings.len() - first_binding,
            });
        }
        Ok(plan)
    }

    /// Returns callback metadata for one object-arena index.
    #[must_use]
    pub fn node(&self, index: usize) -> Option<UiScriptNode> {
        self.nodes.get(index).copied()
    }

    /// Returns final active callback bindings for an object.
    #[must_use]
    pub fn bindings_for(&self, node: UiScriptNode) -> &[UiScriptBinding] {
        &self.bindings[node.first_binding..node.first_binding + node.binding_count]
    }

    /// Finds one active callback on an object-arena index.
    #[must_use]
    pub fn binding(&self, node_index: usize, handler: UiScriptHandler) -> Option<&UiScriptBinding> {
        let node = self.node(node_index)?;
        self.bindings_for(node)
            .iter()
            .find(|binding| binding.handler == handler)
    }

    /// Returns the number of XML callback declarations processed.
    #[must_use]
    pub const fn declaration_count(&self) -> usize {
        self.declaration_count
    }

    /// Returns the number of active callback slots after replacement.
    #[must_use]
    pub fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    /// Returns the number of unique inline functions retained in Lua.
    #[must_use]
    pub fn function_count(&self) -> usize {
        self.functions.len()
    }

    pub(super) fn compiled_function(&self, lua: &Lua, index: u32) -> mlua::Result<Function> {
        let key = self.functions.get(index as usize).ok_or_else(|| {
            mlua::Error::runtime("compiled UI handler index is outside the arena")
        })?;
        lua.registry_value(key)
    }
}

fn compile_target(
    functions: &mut Vec<RegistryKey>,
    compiled: &mut HashMap<(UiScriptHandler, usize), u32>,
    lua: &Lua,
    path: &solarity_asset::AssetPath,
    object: &str,
    handler: UiScriptHandler,
    element: &XmlElement,
) -> Result<Option<UiScriptTarget>, UiScriptError> {
    if let Some(function) = attribute(element, "function").filter(|value| !value.is_empty()) {
        return Ok(Some(UiScriptTarget::Global(function.to_owned())));
    }
    let body = direct_text(element);
    if body.trim().is_empty() {
        return Ok(None);
    }
    let key = (handler, element as *const XmlElement as usize);
    let function_index = if let Some(index) = compiled.get(&key) {
        *index
    } else {
        let source = format!("return function({}) {body}\nend", handler.parameters());
        let label = format!("{}:*:{}", path.as_str(), handler.name());
        let function = lua
            .load(&source)
            .set_name(&label)
            .eval::<Function>()
            .map_err(|error| UiScriptError::Lua {
                path: path.clone(),
                object: object.to_owned(),
                handler: handler.name(),
                message: error.to_string(),
            })?;
        let registry_key =
            lua.create_registry_value(function)
                .map_err(|error| UiScriptError::Lua {
                    path: path.clone(),
                    object: object.to_owned(),
                    handler: handler.name(),
                    message: error.to_string(),
                })?;
        let index = u32::try_from(functions.len()).map_err(|error| UiScriptError::Plan {
            message: format!("compiled handler index exceeds u32: {error}"),
        })?;
        functions.push(registry_key);
        compiled.insert(key, index);
        index
    };
    Ok(Some(UiScriptTarget::Compiled(function_index)))
}

fn handler_for(kind: UiObjectKind, name: &str) -> Option<UiScriptHandler> {
    let specific = match kind {
        UiObjectKind::Button | UiObjectKind::CheckButton => match_name(
            name,
            &[
                UiScriptHandler::PreClick,
                UiScriptHandler::Click,
                UiScriptHandler::PostClick,
                UiScriptHandler::DoubleClick,
            ],
        ),
        UiObjectKind::ColorSelect => match_name(name, &[UiScriptHandler::ColorSelect]),
        UiObjectKind::EditBox => match_name(
            name,
            &[
                UiScriptHandler::EnterPressed,
                UiScriptHandler::EscapePressed,
                UiScriptHandler::SpacePressed,
                UiScriptHandler::TabPressed,
                UiScriptHandler::TextChanged,
                UiScriptHandler::TextSet,
                UiScriptHandler::CursorChanged,
                UiScriptHandler::InputLanguageChanged,
                UiScriptHandler::EditFocusGained,
                UiScriptHandler::EditFocusLost,
                UiScriptHandler::CharComposition,
            ],
        ),
        UiObjectKind::Model | UiObjectKind::ModelFfx => match_name(
            name,
            &[UiScriptHandler::UpdateModel, UiScriptHandler::AnimFinished],
        ),
        UiObjectKind::ScrollFrame => match_name(
            name,
            &[
                UiScriptHandler::HorizontalScroll,
                UiScriptHandler::VerticalScroll,
                UiScriptHandler::ScrollRangeChanged,
            ],
        ),
        UiObjectKind::Slider | UiObjectKind::StatusBar => match_name(
            name,
            &[
                UiScriptHandler::ValueChanged,
                UiScriptHandler::MinMaxChanged,
            ],
        ),
        UiObjectKind::SimpleHtml | UiObjectKind::ScrollingMessageFrame => match_name(
            name,
            &[
                UiScriptHandler::HyperlinkEnter,
                UiScriptHandler::HyperlinkLeave,
                UiScriptHandler::HyperlinkClick,
                UiScriptHandler::MessageScrollChanged,
            ],
        ),
        UiObjectKind::MovieFrame => match_name(
            name,
            &[
                UiScriptHandler::MovieFinished,
                UiScriptHandler::MovieShowSubtitle,
                UiScriptHandler::MovieHideSubtitle,
            ],
        ),
        UiObjectKind::GameTooltip => match_name(
            name,
            &[
                UiScriptHandler::TooltipSetDefaultAnchor,
                UiScriptHandler::TooltipCleared,
                UiScriptHandler::TooltipAddMoney,
                UiScriptHandler::TooltipSetUnit,
                UiScriptHandler::TooltipSetItem,
                UiScriptHandler::TooltipSetSpell,
                UiScriptHandler::TooltipSetQuest,
                UiScriptHandler::TooltipSetAchievement,
            ],
        ),
        _ => None,
    };
    if specific.is_some() || matches!(kind, UiObjectKind::FontString | UiObjectKind::Texture) {
        return specific;
    }
    match_name(
        name,
        &[
            UiScriptHandler::Event,
            UiScriptHandler::Load,
            UiScriptHandler::SizeChanged,
            UiScriptHandler::Update,
            UiScriptHandler::Show,
            UiScriptHandler::Hide,
            UiScriptHandler::Enter,
            UiScriptHandler::Leave,
            UiScriptHandler::MouseDown,
            UiScriptHandler::MouseUp,
            UiScriptHandler::MouseWheel,
            UiScriptHandler::DragStart,
            UiScriptHandler::DragStop,
            UiScriptHandler::ReceiveDrag,
            UiScriptHandler::Char,
            UiScriptHandler::KeyDown,
            UiScriptHandler::KeyUp,
            UiScriptHandler::AttributeChanged,
            UiScriptHandler::Enable,
            UiScriptHandler::Disable,
        ],
    )
}

fn match_name(name: &str, handlers: &[UiScriptHandler]) -> Option<UiScriptHandler> {
    handlers
        .iter()
        .copied()
        .find(|handler| name.eq_ignore_ascii_case(handler.name()))
}

fn handler_from_index(index: usize) -> Option<UiScriptHandler> {
    ALL_HANDLERS.get(index).copied()
}

const ALL_HANDLERS: [UiScriptHandler; HANDLER_COUNT] = [
    UiScriptHandler::Event,
    UiScriptHandler::Load,
    UiScriptHandler::SizeChanged,
    UiScriptHandler::Update,
    UiScriptHandler::Show,
    UiScriptHandler::Hide,
    UiScriptHandler::Enter,
    UiScriptHandler::Leave,
    UiScriptHandler::MouseDown,
    UiScriptHandler::MouseUp,
    UiScriptHandler::MouseWheel,
    UiScriptHandler::DragStart,
    UiScriptHandler::DragStop,
    UiScriptHandler::ReceiveDrag,
    UiScriptHandler::Char,
    UiScriptHandler::KeyDown,
    UiScriptHandler::KeyUp,
    UiScriptHandler::AttributeChanged,
    UiScriptHandler::Enable,
    UiScriptHandler::Disable,
    UiScriptHandler::PreClick,
    UiScriptHandler::Click,
    UiScriptHandler::PostClick,
    UiScriptHandler::DoubleClick,
    UiScriptHandler::ValueChanged,
    UiScriptHandler::MinMaxChanged,
    UiScriptHandler::UpdateModel,
    UiScriptHandler::AnimFinished,
    UiScriptHandler::EnterPressed,
    UiScriptHandler::EscapePressed,
    UiScriptHandler::SpacePressed,
    UiScriptHandler::TabPressed,
    UiScriptHandler::TextChanged,
    UiScriptHandler::TextSet,
    UiScriptHandler::CursorChanged,
    UiScriptHandler::InputLanguageChanged,
    UiScriptHandler::EditFocusGained,
    UiScriptHandler::EditFocusLost,
    UiScriptHandler::CharComposition,
    UiScriptHandler::HorizontalScroll,
    UiScriptHandler::VerticalScroll,
    UiScriptHandler::ScrollRangeChanged,
    UiScriptHandler::ColorSelect,
    UiScriptHandler::HyperlinkEnter,
    UiScriptHandler::HyperlinkLeave,
    UiScriptHandler::HyperlinkClick,
    UiScriptHandler::MessageScrollChanged,
    UiScriptHandler::MovieFinished,
    UiScriptHandler::MovieShowSubtitle,
    UiScriptHandler::MovieHideSubtitle,
    UiScriptHandler::TooltipSetDefaultAnchor,
    UiScriptHandler::TooltipCleared,
    UiScriptHandler::TooltipAddMoney,
    UiScriptHandler::TooltipSetUnit,
    UiScriptHandler::TooltipSetItem,
    UiScriptHandler::TooltipSetSpell,
    UiScriptHandler::TooltipSetQuest,
    UiScriptHandler::TooltipSetAchievement,
];

fn child_named<'a>(
    document: &'a XmlDocument,
    element: &XmlElement,
    name: &str,
) -> Option<&'a XmlElement> {
    element.content().iter().find_map(|content| {
        let XmlContent::Element(index) = content else {
            return None;
        };
        document
            .element(*index)
            .filter(|child| child.name() == name)
    })
}

fn direct_text(element: &XmlElement) -> String {
    let mut source = String::new();
    for content in element.content() {
        if let XmlContent::Text(text) = content {
            source.push_str(text);
        }
    }
    source
}

fn attribute<'a>(element: &'a XmlElement, name: &str) -> Option<&'a str> {
    element
        .attributes()
        .iter()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}
