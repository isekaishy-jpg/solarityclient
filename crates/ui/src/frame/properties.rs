//! Typed stock frame ordering and interaction properties.

use solarity_asset::AssetPath;

use crate::frame::UiFrameError;
use crate::{UiObjectKind, UiObjectTree, XmlElement};

/// Stock frame strata in back-to-front order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum UiFrameStrata {
    /// Behind the ordinary interface.
    Background,
    /// Low-priority interface frames.
    Low,
    /// Ordinary interface frames.
    Medium,
    /// High-priority interface frames.
    High,
    /// Modal dialog frames.
    Dialog,
    /// Full-screen frames.
    Fullscreen,
    /// Modal full-screen dialog frames.
    FullscreenDialog,
    /// Tooltip frames above other UI strata.
    Tooltip,
}

/// Frame properties contributed by one XML inheritance layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiFrameLayer {
    strata: Option<UiFrameStrata>,
    level: Option<i32>,
    id: Option<i32>,
    top_level: Option<bool>,
    movable: Option<bool>,
    resizable: Option<bool>,
    clamped_to_screen: Option<bool>,
    keyboard_enabled: Option<bool>,
    mouse_enabled: Option<bool>,
    protected: Option<bool>,
    position_persistence_disabled: Option<bool>,
}

impl UiFrameLayer {
    /// Returns the explicitly supplied frame stratum.
    #[must_use]
    pub const fn strata(self) -> Option<UiFrameStrata> {
        self.strata
    }

    /// Returns the explicitly supplied level within the frame stratum.
    #[must_use]
    pub const fn level(self) -> Option<i32> {
        self.level
    }

    /// Returns the explicitly supplied numeric identifier.
    #[must_use]
    pub const fn id(self) -> Option<i32> {
        self.id
    }

    /// Returns the explicit top-level interaction flag.
    #[must_use]
    pub const fn top_level(self) -> Option<bool> {
        self.top_level
    }

    /// Returns the explicit movable flag.
    #[must_use]
    pub const fn movable(self) -> Option<bool> {
        self.movable
    }

    /// Returns the explicit resizable flag.
    #[must_use]
    pub const fn resizable(self) -> Option<bool> {
        self.resizable
    }

    /// Returns the explicit screen-clamping flag.
    #[must_use]
    pub const fn clamped_to_screen(self) -> Option<bool> {
        self.clamped_to_screen
    }

    /// Returns the explicit keyboard-input flag.
    #[must_use]
    pub const fn keyboard_enabled(self) -> Option<bool> {
        self.keyboard_enabled
    }

    /// Returns the explicit mouse-input flag.
    #[must_use]
    pub const fn mouse_enabled(self) -> Option<bool> {
        self.mouse_enabled
    }

    /// Returns the explicit protected-frame flag.
    #[must_use]
    pub const fn protected(self) -> Option<bool> {
        self.protected
    }

    /// Returns the explicit opt-out from saved frame positions.
    #[must_use]
    pub const fn position_persistence_disabled(self) -> Option<bool> {
        self.position_persistence_disabled
    }
}

/// Flat layer range belonging to one frame-derived object node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiFrameNode {
    first_layer: usize,
    layer_count: usize,
}

impl UiFrameNode {
    /// Returns the number of property-bearing frame layers.
    #[must_use]
    pub const fn layer_count(self) -> usize {
        self.layer_count
    }
}

/// Flat typed frame properties parallel to the complete object tree.
pub struct UiFramePlan {
    nodes: Vec<UiFrameNode>,
    layers: Vec<UiFrameLayer>,
}

/// Resolved stock frame state after parenting and XML inheritance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiFrameState {
    strata: UiFrameStrata,
    level: i32,
    id: i32,
    top_level: bool,
    movable: bool,
    resizable: bool,
    clamped_to_screen: bool,
    keyboard_enabled: bool,
    mouse_enabled: bool,
    protected: bool,
    position_persistence_disabled: bool,
}

impl UiFrameState {
    /// Returns the resolved frame stratum.
    #[must_use]
    pub const fn strata(self) -> UiFrameStrata {
        self.strata
    }

    /// Returns the resolved level within the frame stratum.
    #[must_use]
    pub const fn level(self) -> i32 {
        self.level
    }

    /// Returns the resolved numeric identifier.
    #[must_use]
    pub const fn id(self) -> i32 {
        self.id
    }

    /// Returns whether the frame has top-level interaction behavior.
    #[must_use]
    pub const fn top_level(self) -> bool {
        self.top_level
    }

    /// Returns whether the frame is movable.
    #[must_use]
    pub const fn movable(self) -> bool {
        self.movable
    }

    /// Returns whether the frame is resizable.
    #[must_use]
    pub const fn resizable(self) -> bool {
        self.resizable
    }

    /// Returns whether the frame is clamped to the screen.
    #[must_use]
    pub const fn clamped_to_screen(self) -> bool {
        self.clamped_to_screen
    }

    /// Returns whether keyboard input is enabled.
    #[must_use]
    pub const fn keyboard_enabled(self) -> bool {
        self.keyboard_enabled
    }

    /// Returns whether mouse input is enabled.
    #[must_use]
    pub const fn mouse_enabled(self) -> bool {
        self.mouse_enabled
    }

    /// Returns whether protected-frame restrictions apply.
    #[must_use]
    pub const fn protected(self) -> bool {
        self.protected
    }

    /// Returns whether saved frame positions are disabled.
    #[must_use]
    pub const fn position_persistence_disabled(self) -> bool {
        self.position_persistence_disabled
    }

    fn unparented(kind: UiObjectKind) -> Self {
        Self {
            strata: UiFrameStrata::Medium,
            level: 0,
            id: 0,
            top_level: false,
            movable: false,
            resizable: false,
            clamped_to_screen: false,
            keyboard_enabled: false,
            mouse_enabled: matches!(kind, UiObjectKind::Button | UiObjectKind::CheckButton),
            protected: false,
            position_persistence_disabled: false,
        }
    }

    fn child_of(parent: Self, kind: UiObjectKind) -> Result<Self, UiFrameError> {
        let level = parent
            .level
            .checked_add(1)
            .ok_or_else(|| UiFrameError::Resolution {
                message: "parent frame level cannot be incremented".to_owned(),
            })?;
        Ok(Self {
            strata: parent.strata,
            level,
            ..Self::unparented(kind)
        })
    }

    fn apply(&mut self, layer: UiFrameLayer) {
        if let Some(value) = layer.strata {
            self.strata = value;
        }
        if let Some(value) = layer.level {
            self.level = value;
        }
        if let Some(value) = layer.id {
            self.id = value;
        }
        if let Some(value) = layer.top_level {
            self.top_level = value;
        }
        if let Some(value) = layer.movable {
            self.movable = value;
        }
        if let Some(value) = layer.resizable {
            self.resizable = value;
        }
        if let Some(value) = layer.clamped_to_screen {
            self.clamped_to_screen = value;
        }
        if let Some(value) = layer.keyboard_enabled {
            self.keyboard_enabled = value;
        }
        if let Some(value) = layer.mouse_enabled {
            self.mouse_enabled = value;
        }
        if let Some(value) = layer.protected {
            self.protected = value;
        }
        if let Some(value) = layer.position_persistence_disabled {
            self.position_persistence_disabled = value;
        }
    }
}

/// Compact resolved frame states indexed indirectly from object nodes.
pub struct UiFrameStatePlan {
    node_states: Vec<Option<u32>>,
    states: Vec<UiFrameState>,
}

impl UiFrameStatePlan {
    /// Returns resolved state for a frame-derived object node.
    #[must_use]
    pub fn state(&self, node_index: usize) -> Option<UiFrameState> {
        let state_index = self.node_states.get(node_index)?.as_ref()?;
        self.states.get(*state_index as usize).copied()
    }

    /// Returns the number of resolved frame-derived objects.
    #[must_use]
    pub fn state_count(&self) -> usize {
        self.states.len()
    }
}

impl UiFramePlan {
    /// Parses ordering and common interaction properties for frame-derived objects.
    ///
    /// # Errors
    ///
    /// Returns [`UiFrameError::Property`] for unknown strata or malformed
    /// integer and boolean attributes.
    pub fn from_tree(tree: &UiObjectTree<'_>) -> Result<Self, UiFrameError> {
        let mut plan = Self {
            nodes: Vec::with_capacity(tree.nodes().len()),
            layers: Vec::new(),
        };
        for node in tree.nodes() {
            let first_layer = plan.layers.len();
            if is_frame_kind(node.kind()) {
                for source in node.layers() {
                    if let Some(layer) = parse_layer(source.source_path(), source.element())? {
                        plan.layers.push(layer);
                    }
                }
            }
            plan.nodes.push(UiFrameNode {
                first_layer,
                layer_count: plan.layers.len() - first_layer,
            });
        }
        Ok(plan)
    }

    /// Returns frame metadata for one object node.
    #[must_use]
    pub fn node(&self, index: usize) -> Option<UiFrameNode> {
        self.nodes.get(index).copied()
    }

    /// Returns property-bearing layers for one frame node.
    #[must_use]
    pub fn layers_for(&self, node: UiFrameNode) -> &[UiFrameLayer] {
        &self.layers[node.first_layer..node.first_layer + node.layer_count]
    }

    /// Returns the total number of retained frame-property layers.
    #[must_use]
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Resolves stock parenting defaults followed by ordered XML overrides.
    ///
    /// # Errors
    ///
    /// Returns [`UiFrameError::Resolution`] for parent cycles, non-frame
    /// parents, index mismatches, or a frame-level overflow.
    pub fn resolve(&self, tree: &UiObjectTree<'_>) -> Result<UiFrameStatePlan, UiFrameError> {
        if self.nodes.len() != tree.nodes().len() {
            return Err(UiFrameError::Resolution {
                message: "frame plan and object tree have different node counts".to_owned(),
            });
        }
        let mut resolved = vec![None; tree.nodes().len()];
        let mut visiting = vec![false; tree.nodes().len()];
        for index in 0..tree.nodes().len() {
            let _state = self.resolve_node(tree, index, &mut resolved, &mut visiting)?;
        }

        let mut node_states = vec![None; resolved.len()];
        let frame_count = resolved.iter().flatten().count();
        let mut states = Vec::with_capacity(frame_count);
        for (node_index, state) in resolved.into_iter().enumerate() {
            let Some(state) = state else {
                continue;
            };
            let state_index =
                u32::try_from(states.len()).map_err(|error| UiFrameError::Resolution {
                    message: format!("frame state index exceeds u32: {error}"),
                })?;
            states.push(state);
            node_states[node_index] = Some(state_index);
        }
        Ok(UiFrameStatePlan {
            node_states,
            states,
        })
    }

    fn resolve_node(
        &self,
        tree: &UiObjectTree<'_>,
        index: usize,
        resolved: &mut [Option<UiFrameState>],
        visiting: &mut [bool],
    ) -> Result<Option<UiFrameState>, UiFrameError> {
        if let Some(state) = resolved[index] {
            return Ok(Some(state));
        }
        let object = &tree.nodes()[index];
        if !is_frame_kind(object.kind()) {
            return Ok(None);
        }
        if std::mem::replace(&mut visiting[index], true) {
            return Err(UiFrameError::Resolution {
                message: format!(
                    "frame parent cycle reaches {}",
                    object.name().unwrap_or("<unnamed>")
                ),
            });
        }
        let result = (|| {
            let mut state = if let Some(parent_index) = object.parent() {
                let parent = self
                    .resolve_node(tree, parent_index, resolved, visiting)?
                    .ok_or_else(|| UiFrameError::Resolution {
                        message: format!(
                            "frame {} has a non-frame parent",
                            object.name().unwrap_or("<unnamed>")
                        ),
                    })?;
                UiFrameState::child_of(parent, object.kind())?
            } else {
                UiFrameState::unparented(object.kind())
            };
            let node = self.nodes[index];
            for layer in self.layers_for(node) {
                state.apply(*layer);
            }
            Ok(state)
        })();
        visiting[index] = false;
        let state = result?;
        resolved[index] = Some(state);
        Ok(Some(state))
    }
}

fn is_frame_kind(kind: UiObjectKind) -> bool {
    !matches!(kind, UiObjectKind::FontString | UiObjectKind::Texture)
}

fn parse_layer(
    path: &AssetPath,
    element: &XmlElement,
) -> Result<Option<UiFrameLayer>, UiFrameError> {
    let layer = UiFrameLayer {
        strata: attribute(element, "frameStrata")
            .map(|value| parse_strata(path, value))
            .transpose()?,
        level: parse_optional_integer(path, element, "frameLevel")?,
        id: parse_optional_integer(path, element, "id")?,
        top_level: parse_optional_bool(path, element, "toplevel")?,
        movable: parse_optional_bool(path, element, "movable")?,
        resizable: parse_optional_bool(path, element, "resizable")?,
        clamped_to_screen: parse_optional_bool(path, element, "clampedToScreen")?,
        keyboard_enabled: parse_optional_bool(path, element, "enableKeyboard")?,
        mouse_enabled: parse_optional_bool(path, element, "enableMouse")?,
        protected: parse_optional_bool(path, element, "protected")?,
        position_persistence_disabled: parse_optional_bool(path, element, "dontSavePosition")?,
    };
    let present = layer.strata.is_some()
        || layer.level.is_some()
        || layer.id.is_some()
        || layer.top_level.is_some()
        || layer.movable.is_some()
        || layer.resizable.is_some()
        || layer.clamped_to_screen.is_some()
        || layer.keyboard_enabled.is_some()
        || layer.mouse_enabled.is_some()
        || layer.protected.is_some()
        || layer.position_persistence_disabled.is_some();
    Ok(present.then_some(layer))
}

fn parse_strata(path: &AssetPath, value: &str) -> Result<UiFrameStrata, UiFrameError> {
    match value {
        "BACKGROUND" => Ok(UiFrameStrata::Background),
        "LOW" => Ok(UiFrameStrata::Low),
        "MEDIUM" => Ok(UiFrameStrata::Medium),
        "HIGH" => Ok(UiFrameStrata::High),
        "DIALOG" => Ok(UiFrameStrata::Dialog),
        "FULLSCREEN" => Ok(UiFrameStrata::Fullscreen),
        "FULLSCREEN_DIALOG" => Ok(UiFrameStrata::FullscreenDialog),
        "TOOLTIP" => Ok(UiFrameStrata::Tooltip),
        _ => Err(frame_error(
            path,
            format!("unsupported frameStrata {value}"),
        )),
    }
}

fn parse_optional_integer(
    path: &AssetPath,
    element: &XmlElement,
    name: &str,
) -> Result<Option<i32>, UiFrameError> {
    let parsed = attribute(element, name)
        .map(|value| {
            value.parse::<i32>().map_err(|error| {
                frame_error(path, format!("invalid {name} value {value}: {error}"))
            })
        })
        .transpose()?;
    if name == "frameLevel" && parsed.is_some_and(|value| value <= 0) {
        return Err(frame_error(path, "frameLevel must be greater than zero"));
    }
    if name == "id" && parsed.is_some_and(|value| value < 0) {
        return Err(frame_error(path, "id must be nonnegative"));
    }
    Ok(parsed)
}

fn parse_optional_bool(
    path: &AssetPath,
    element: &XmlElement,
    name: &str,
) -> Result<Option<bool>, UiFrameError> {
    attribute(element, name)
        .map(|value| {
            if value.eq_ignore_ascii_case("true") {
                Ok(true)
            } else if value.eq_ignore_ascii_case("false") {
                Ok(false)
            } else {
                Err(frame_error(path, format!("invalid {name} boolean {value}")))
            }
        })
        .transpose()
}

fn attribute<'a>(element: &'a XmlElement, name: &str) -> Option<&'a str> {
    element
        .attributes()
        .iter()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn frame_error(path: &AssetPath, message: impl Into<String>) -> UiFrameError {
    UiFrameError::Property {
        path: path.clone(),
        message: message.into(),
    }
}
