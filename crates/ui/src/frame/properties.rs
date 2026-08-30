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
    attribute(element, name)
        .map(|value| {
            value.parse::<i32>().map_err(|error| {
                frame_error(path, format!("invalid {name} value {value}: {error}"))
            })
        })
        .transpose()
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
