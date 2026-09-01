//! Typed stock XML animation declarations and owner-local construction order.

use std::collections::HashMap;
use std::ops::Range;

use solarity_asset::AssetPath;
use thiserror::Error;

use crate::{UiObjectTree, XmlContent, XmlDocument, XmlElement};

/// A failure while decoding the animation vocabulary used by stock FrameXML.
#[derive(Debug, Error)]
pub enum UiAnimationError {
    /// An animation declaration contains an invalid or unsupported property.
    #[error("invalid UI animation declaration in {path}: {message}")]
    Declaration {
        /// XML file containing the declaration.
        path: AssetPath,
        /// Attribute, hierarchy, or global-name context.
        message: String,
    },
}

/// Concrete animation primitives authored by the build-12340 UI bundle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiAnimationKind {
    /// Timing-only base animation.
    Animation,
    /// Additive alpha transition.
    Alpha,
    /// Additive two-dimensional translation.
    Translation,
}

impl UiAnimationKind {
    /// Returns the stock script-visible object type.
    #[must_use]
    pub const fn object_type(self) -> &'static str {
        match self {
            Self::Animation => "Animation",
            Self::Alpha => "Alpha",
            Self::Translation => "Translation",
        }
    }
}

/// Repetition behavior owned by one animation group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiAnimationLooping {
    /// Play the timeline once.
    None,
    /// Restart at the beginning after completion.
    Repeat,
    /// Alternate forward and reverse playback.
    Bounce,
}

impl UiAnimationLooping {
    /// Returns the exact Lua token used by the stock API.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Repeat => "REPEAT",
            Self::Bounce => "BOUNCE",
        }
    }
}

/// Type-specific immutable values retained from one XML primitive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiAnimationValue {
    /// Timing-only animation without an interpolated property.
    Timing,
    /// Additive alpha delta.
    Alpha {
        /// Signed value added across the interpolation interval.
        change: f64,
    },
    /// Additive screen-space offset.
    Translation {
        /// Horizontal and vertical UI-unit deltas.
        offset: (f64, f64),
    },
}

/// One animation primitive in authored group order.
#[derive(Clone, Debug, PartialEq)]
pub struct UiAnimation {
    group: usize,
    name: Option<String>,
    parent_key: Option<String>,
    kind: UiAnimationKind,
    duration: f64,
    start_delay: f64,
    end_delay: f64,
    order: u32,
    smoothing: String,
    value: UiAnimationValue,
}

impl UiAnimation {
    /// Returns the owning group arena index.
    #[must_use]
    pub const fn group(&self) -> usize {
        self.group
    }

    /// Returns the expanded global name, when authored.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the field used to publish this primitive on its group.
    #[must_use]
    pub fn parent_key(&self) -> Option<&str> {
        self.parent_key.as_deref()
    }

    /// Returns the concrete primitive kind.
    #[must_use]
    pub const fn kind(&self) -> UiAnimationKind {
        self.kind
    }

    /// Returns the authored active duration in seconds.
    #[must_use]
    pub const fn duration(&self) -> f64 {
        self.duration
    }

    /// Returns the delay before active interpolation in seconds.
    #[must_use]
    pub const fn start_delay(&self) -> f64 {
        self.start_delay
    }

    /// Returns the delay after active interpolation in seconds.
    #[must_use]
    pub const fn end_delay(&self) -> f64 {
        self.end_delay
    }

    /// Returns the sequential timeline band; equal orders run together.
    #[must_use]
    pub const fn order(&self) -> u32 {
        self.order
    }

    /// Returns the stock smoothing token.
    #[must_use]
    pub fn smoothing(&self) -> &str {
        &self.smoothing
    }

    /// Returns the primitive-specific authored value.
    #[must_use]
    pub const fn value(&self) -> UiAnimationValue {
        self.value
    }
}

/// One frame-owned animation group and its contiguous primitive range.
#[derive(Clone, Debug, PartialEq)]
pub struct UiAnimationGroup {
    owner: usize,
    name: Option<String>,
    parent_key: Option<String>,
    looping: UiAnimationLooping,
    animations: Range<usize>,
    on_load: Option<UiAnimationHandler>,
    on_finished: Option<UiAnimationHandler>,
}

impl UiAnimationGroup {
    /// Returns the owning UI object arena index.
    #[must_use]
    pub const fn owner(&self) -> usize {
        self.owner
    }

    /// Returns the expanded global name, when authored.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the field used to publish this group on its owning frame.
    #[must_use]
    pub fn parent_key(&self) -> Option<&str> {
        self.parent_key.as_deref()
    }

    /// Returns the authored repetition behavior.
    #[must_use]
    pub const fn looping(&self) -> UiAnimationLooping {
        self.looping
    }

    /// Returns primitive arena indices in authored order.
    #[must_use]
    pub fn animation_range(&self) -> Range<usize> {
        self.animations.clone()
    }

    pub(crate) fn on_load(&self) -> Option<&UiAnimationHandler> {
        self.on_load.as_ref()
    }

    pub(crate) fn on_finished(&self) -> Option<&UiAnimationHandler> {
        self.on_finished.as_ref()
    }
}

/// A stock XML animation callback target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum UiAnimationHandler {
    Inline(String),
    Global(String),
}

/// Complete animation arena aligned to a [`UiObjectTree`].
#[derive(Clone, Debug, PartialEq)]
pub struct UiAnimationPlan {
    groups: Vec<UiAnimationGroup>,
    animations: Vec<UiAnimation>,
    owner_groups: Vec<Vec<usize>>,
}

impl UiAnimationPlan {
    /// Decodes every inherited and concrete `<Animations>` layer.
    ///
    /// The build-12340 bundle uses timing-only `Animation`, `Alpha`, and
    /// `Translation` primitives. The plan deliberately rejects later-client
    /// primitive types so a modified UI cannot silently enter semantics this
    /// client has not implemented.
    ///
    /// # Errors
    ///
    /// Returns [`UiAnimationError`] for malformed hierarchy, names, numbers,
    /// loop modes, smoothing modes, or duplicate global animation names.
    pub fn from_tree(tree: &UiObjectTree<'_>) -> Result<Self, UiAnimationError> {
        let mut plan = Self {
            groups: Vec::new(),
            animations: Vec::new(),
            owner_groups: vec![Vec::new(); tree.nodes().len()],
        };
        let mut names = tree
            .nodes()
            .iter()
            .filter_map(|node| node.name().map(|name| (name.to_owned(), "UI object")))
            .collect::<HashMap<_, _>>();

        for (owner, node) in tree.nodes().iter().enumerate() {
            for layer in node.layers() {
                for animations in direct_children(
                    layer.source_path(),
                    layer.document(),
                    layer.element(),
                    "Animations",
                )? {
                    for group_element in
                        direct_element_children(layer.source_path(), layer.document(), animations)?
                    {
                        if group_element.name() != "AnimationGroup" {
                            return Err(animation_error(
                                layer.source_path(),
                                format!(
                                    "unsupported element <{}> inside <Animations>",
                                    group_element.name()
                                ),
                            ));
                        }
                        plan.push_group(
                            owner,
                            node.name_context(),
                            layer.source_path(),
                            layer.document(),
                            group_element,
                            &mut names,
                        )?;
                    }
                }
            }
        }
        Ok(plan)
    }

    /// Returns groups in construction order.
    #[must_use]
    pub fn groups(&self) -> &[UiAnimationGroup] {
        &self.groups
    }

    /// Returns primitive animations in construction order.
    #[must_use]
    pub fn animations(&self) -> &[UiAnimation] {
        &self.animations
    }

    /// Returns group arena indices owned by one UI object.
    #[must_use]
    pub fn groups_for_owner(&self, owner: usize) -> &[usize] {
        self.owner_groups.get(owner).map_or(&[], Vec::as_slice)
    }

    fn push_group(
        &mut self,
        owner: usize,
        owner_context: Option<&str>,
        path: &AssetPath,
        document: &XmlDocument,
        element: &XmlElement,
        names: &mut HashMap<String, &'static str>,
    ) -> Result<(), UiAnimationError> {
        let name = expanded_name(path, element, owner_context)?;
        register_name(path, name.as_deref(), "animation group", names)?;
        let name_context = name.as_deref().or(owner_context);
        let looping = parse_looping(path, attribute(element, "looping"))?;
        let first_animation = self.animations.len();
        let group_index = self.groups.len();
        let mut on_load = None;
        let mut on_finished = None;

        for child in direct_element_children(path, document, element)? {
            match child.name() {
                "Animation" | "Alpha" | "Translation" => {
                    let animation = parse_animation(path, child, group_index, name_context, names)?;
                    self.animations.push(animation);
                }
                "Scripts" => {
                    (on_load, on_finished) = parse_group_scripts(path, document, child)?;
                }
                name => {
                    return Err(animation_error(
                        path,
                        format!("unsupported animation-group child <{name}>"),
                    ));
                }
            }
        }

        let group = UiAnimationGroup {
            owner,
            name,
            parent_key: nonempty_attribute(element, "parentKey").map(str::to_owned),
            looping,
            animations: first_animation..self.animations.len(),
            on_load,
            on_finished,
        };
        self.groups.push(group);
        self.owner_groups[owner].push(group_index);
        Ok(())
    }
}

fn parse_animation(
    path: &AssetPath,
    element: &XmlElement,
    group: usize,
    name_context: Option<&str>,
    names: &mut HashMap<String, &'static str>,
) -> Result<UiAnimation, UiAnimationError> {
    let name = expanded_name(path, element, name_context)?;
    register_name(path, name.as_deref(), "animation", names)?;
    let kind = match element.name() {
        "Animation" => UiAnimationKind::Animation,
        "Alpha" => UiAnimationKind::Alpha,
        "Translation" => UiAnimationKind::Translation,
        name => {
            return Err(animation_error(
                path,
                format!("unsupported animation primitive <{name}>"),
            ));
        }
    };
    let value = match kind {
        UiAnimationKind::Animation => UiAnimationValue::Timing,
        UiAnimationKind::Alpha => UiAnimationValue::Alpha {
            change: parse_number(path, element, "change", 0.0, false)?,
        },
        UiAnimationKind::Translation => UiAnimationValue::Translation {
            offset: (
                parse_number(path, element, "offsetX", 0.0, false)?,
                parse_number(path, element, "offsetY", 0.0, false)?,
            ),
        },
    };
    let smoothing = attribute(element, "smoothing").unwrap_or("NONE");
    if !matches!(smoothing, "NONE" | "IN" | "OUT" | "IN_OUT") {
        return Err(animation_error(
            path,
            format!("unsupported smoothing mode {smoothing}"),
        ));
    }
    let order = match attribute(element, "order") {
        Some(value) => value.parse::<u32>().map_err(|_| {
            animation_error(
                path,
                format!("invalid order {value} on <{}>", element.name()),
            )
        })?,
        None => 1,
    };
    if order == 0 {
        return Err(animation_error(path, "animation order must be positive"));
    }

    Ok(UiAnimation {
        group,
        name,
        parent_key: nonempty_attribute(element, "parentKey").map(str::to_owned),
        kind,
        duration: parse_number(path, element, "duration", 0.0, true)?,
        start_delay: parse_number(path, element, "startDelay", 0.0, true)?,
        end_delay: parse_number(path, element, "endDelay", 0.0, true)?,
        order,
        smoothing: smoothing.to_owned(),
        value,
    })
}

fn parse_group_scripts(
    path: &AssetPath,
    document: &XmlDocument,
    scripts: &XmlElement,
) -> Result<(Option<UiAnimationHandler>, Option<UiAnimationHandler>), UiAnimationError> {
    let mut on_load = None;
    let mut on_finished = None;
    for handler in direct_element_children(path, document, scripts)? {
        let target = if let Some(function) = nonempty_attribute(handler, "function") {
            UiAnimationHandler::Global(function.to_owned())
        } else {
            let source = handler
                .content()
                .iter()
                .filter_map(|content| match content {
                    XmlContent::Text(text) => Some(text.as_str()),
                    XmlContent::Element(_) => None,
                })
                .collect::<String>();
            UiAnimationHandler::Inline(source)
        };
        match handler.name() {
            "OnLoad" => on_load = Some(target),
            "OnFinished" => on_finished = Some(target),
            name => {
                return Err(animation_error(
                    path,
                    format!("unsupported animation-group handler <{name}>"),
                ));
            }
        }
    }
    Ok((on_load, on_finished))
}

fn direct_children<'a>(
    path: &AssetPath,
    document: &'a XmlDocument,
    element: &XmlElement,
    name: &str,
) -> Result<Vec<&'a XmlElement>, UiAnimationError> {
    Ok(direct_element_children(path, document, element)?
        .into_iter()
        .filter(|child| child.name() == name)
        .collect())
}

fn direct_element_children<'a>(
    path: &AssetPath,
    document: &'a XmlDocument,
    element: &XmlElement,
) -> Result<Vec<&'a XmlElement>, UiAnimationError> {
    element
        .content()
        .iter()
        .filter_map(|content| match content {
            XmlContent::Element(index) => Some(*index),
            XmlContent::Text(_) => None,
        })
        .map(|index| {
            document.element(index).ok_or_else(|| {
                animation_error(path, "animation child index is outside the XML arena")
            })
        })
        .collect()
}

fn expanded_name(
    path: &AssetPath,
    element: &XmlElement,
    parent: Option<&str>,
) -> Result<Option<String>, UiAnimationError> {
    let Some(name) = nonempty_attribute(element, "name") else {
        return Ok(None);
    };
    if !name.contains("$parent") {
        return Ok(Some(name.to_owned()));
    }
    let parent = parent.ok_or_else(|| {
        animation_error(
            path,
            format!("animation name {name} has no named parent context"),
        )
    })?;
    Ok(Some(name.replace("$parent", parent)))
}

fn register_name(
    path: &AssetPath,
    name: Option<&str>,
    kind: &'static str,
    names: &mut HashMap<String, &'static str>,
) -> Result<(), UiAnimationError> {
    let Some(name) = name else {
        return Ok(());
    };
    if let Some(previous) = names.insert(name.to_owned(), kind) {
        return Err(animation_error(
            path,
            format!("{kind} name {name} collides with an earlier {previous}"),
        ));
    }
    Ok(())
}

fn parse_looping(
    path: &AssetPath,
    value: Option<&str>,
) -> Result<UiAnimationLooping, UiAnimationError> {
    match value.unwrap_or("NONE") {
        "NONE" => Ok(UiAnimationLooping::None),
        "REPEAT" => Ok(UiAnimationLooping::Repeat),
        "BOUNCE" => Ok(UiAnimationLooping::Bounce),
        value => Err(animation_error(
            path,
            format!("unsupported animation looping mode {value}"),
        )),
    }
}

fn parse_number(
    path: &AssetPath,
    element: &XmlElement,
    name: &str,
    default: f64,
    nonnegative: bool,
) -> Result<f64, UiAnimationError> {
    let Some(authored) = attribute(element, name) else {
        return Ok(default);
    };
    let value = authored.parse::<f64>().map_err(|_| {
        animation_error(
            path,
            format!("invalid {name} value {authored} on <{}>", element.name()),
        )
    })?;
    if !value.is_finite() || nonnegative && value < 0.0 {
        return Err(animation_error(
            path,
            format!("invalid {name} value {authored} on <{}>", element.name()),
        ));
    }
    Ok(value)
}

fn nonempty_attribute<'a>(element: &'a XmlElement, name: &str) -> Option<&'a str> {
    attribute(element, name).filter(|value| !value.is_empty())
}

fn attribute<'a>(element: &'a XmlElement, name: &str) -> Option<&'a str> {
    element
        .attributes()
        .iter()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn animation_error(path: &AssetPath, message: impl Into<String>) -> UiAnimationError {
    UiAnimationError::Declaration {
        path: path.clone(),
        message: message.into(),
    }
}
