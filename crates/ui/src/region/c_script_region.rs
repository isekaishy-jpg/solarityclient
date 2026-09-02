//! Typed stock region geometry stored in flat startup arenas.

use solarity_asset::AssetPath;

use crate::region::UiLayoutError;
use crate::{UiElementLayer, UiObjectTree, XmlContent, XmlDocument, XmlElement};

/// One of the nine stock anchor points.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiPoint {
    /// Upper-left corner.
    TopLeft,
    /// Upper edge center.
    Top,
    /// Upper-right corner.
    TopRight,
    /// Left edge center.
    Left,
    /// Center.
    Center,
    /// Right edge center.
    Right,
    /// Lower-left corner.
    BottomLeft,
    /// Lower edge center.
    Bottom,
    /// Lower-right corner.
    BottomRight,
}

impl UiPoint {
    pub(crate) const fn index(self) -> usize {
        self as usize
    }
}

/// Explicit absolute dimensions from one XML layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiDimensions {
    width: Option<f32>,
    height: Option<f32>,
}

impl UiDimensions {
    /// Returns the explicitly supplied width.
    #[must_use]
    pub const fn width(self) -> Option<f32> {
        self.width
    }

    /// Returns the explicitly supplied height.
    #[must_use]
    pub const fn height(self) -> Option<f32> {
        self.height
    }
}

/// One stock anchor declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct UiAnchor {
    point: UiPoint,
    relative_to: Option<String>,
    relative_point: Option<UiPoint>,
    offset: Option<(f32, f32)>,
}

impl UiAnchor {
    /// Returns the anchored point on this region.
    #[must_use]
    pub const fn point(&self) -> UiPoint {
        self.point
    }

    /// Returns the expanded relative object name when XML supplied one.
    #[must_use]
    pub fn relative_to(&self) -> Option<&str> {
        self.relative_to.as_deref()
    }

    /// Returns the explicit point on the relative region.
    #[must_use]
    pub const fn relative_point(&self) -> Option<UiPoint> {
        self.relative_point
    }

    /// Returns the explicit absolute offset.
    #[must_use]
    pub const fn offset(&self) -> Option<(f32, f32)> {
        self.offset
    }
}

/// Layout properties contributed by one XML inheritance layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiLayoutLayer {
    dimensions: Option<UiDimensions>,
    first_anchor: usize,
    anchor_count: usize,
    anchors_present: bool,
    set_all_points: Option<bool>,
    hidden: Option<bool>,
    alpha: Option<f32>,
    scale: Option<f32>,
}

impl UiLayoutLayer {
    /// Returns dimensions explicitly supplied by this layer.
    #[must_use]
    pub const fn dimensions(self) -> Option<UiDimensions> {
        self.dimensions
    }

    /// Returns whether this layer contains an `<Anchors>` element.
    #[must_use]
    pub const fn anchors_present(self) -> bool {
        self.anchors_present
    }

    /// Returns the explicit `setAllPoints` flag.
    #[must_use]
    pub const fn set_all_points(self) -> Option<bool> {
        self.set_all_points
    }

    /// Returns the explicit hidden flag.
    #[must_use]
    pub const fn hidden(self) -> Option<bool> {
        self.hidden
    }

    /// Returns explicit region alpha.
    #[must_use]
    pub const fn alpha(self) -> Option<f32> {
        self.alpha
    }

    /// Returns explicit region scale.
    #[must_use]
    pub const fn scale(self) -> Option<f32> {
        self.scale
    }
}

/// Range of typed layout layers belonging to one object node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiNodeLayout {
    first_layer: usize,
    layer_count: usize,
}

impl UiNodeLayout {
    /// Returns the number of layout-contributing XML layers.
    #[must_use]
    pub const fn layer_count(self) -> usize {
        self.layer_count
    }
}

/// Flat typed layout data parallel to a [`UiObjectTree`].
pub struct UiLayoutPlan {
    nodes: Vec<UiNodeLayout>,
    layers: Vec<UiLayoutLayer>,
    anchors: Vec<UiAnchor>,
}

impl UiLayoutPlan {
    /// Parses layout-bearing properties for every object and inheritance layer.
    ///
    /// Layers with no layout properties consume no entry. Missing properties
    /// stay absent for the stock default/application stage rather than being
    /// assigned guessed values.
    ///
    /// # Errors
    ///
    /// Returns [`UiLayoutError::Layout`] for invalid booleans, finite numbers,
    /// point names, structures, or `$parent` references.
    pub fn from_tree(tree: &UiObjectTree<'_>) -> Result<Self, UiLayoutError> {
        let mut plan = Self {
            nodes: Vec::with_capacity(tree.nodes().len()),
            layers: Vec::new(),
            anchors: Vec::new(),
        };

        for (node_index, node) in tree.nodes().iter().enumerate() {
            let first_layer = plan.layers.len();
            let parent_context = node
                .parent()
                .and_then(|index| tree.nodes().get(index))
                .and_then(|parent| parent.name_context());
            for source in node.layers() {
                if let Some(layer) = parse_layer(
                    source,
                    parent_context,
                    tree.is_dynamic_root(node_index),
                    &mut plan.anchors,
                )? {
                    plan.layers.push(layer);
                }
            }
            plan.nodes.push(UiNodeLayout {
                first_layer,
                layer_count: plan.layers.len() - first_layer,
            });
        }
        Ok(plan)
    }

    /// Returns layout metadata for one object node.
    #[must_use]
    pub fn node(&self, index: usize) -> Option<UiNodeLayout> {
        self.nodes.get(index).copied()
    }

    /// Returns the number of object nodes covered by the plan.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns typed layers for one node in inheritance order.
    #[must_use]
    pub fn layers_for(&self, node: UiNodeLayout) -> &[UiLayoutLayer] {
        &self.layers[node.first_layer..node.first_layer + node.layer_count]
    }

    /// Returns anchors contributed by one layout layer.
    #[must_use]
    pub fn anchors_for(&self, layer: UiLayoutLayer) -> &[UiAnchor] {
        &self.anchors[layer.first_anchor..layer.first_anchor + layer.anchor_count]
    }

    /// Returns the total number of retained layout layers.
    #[must_use]
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Returns the total number of retained anchors.
    #[must_use]
    pub fn anchor_count(&self) -> usize {
        self.anchors.len()
    }
}

fn parse_layer(
    source: &UiElementLayer<'_>,
    parent_context: Option<&str>,
    allow_dynamic_parent: bool,
    anchors: &mut Vec<UiAnchor>,
) -> Result<Option<UiLayoutLayer>, UiLayoutError> {
    let path = source.source_path();
    let element = source.element();
    let dimensions = child_named(source.document(), element, "Size")
        .map(|size| parse_dimensions(path, source.document(), size))
        .transpose()?;
    let first_anchor = anchors.len();
    let anchors_element = child_named(source.document(), element, "Anchors");
    if let Some(anchors_element) = anchors_element {
        parse_anchors(
            path,
            source.document(),
            anchors_element,
            parent_context,
            allow_dynamic_parent,
            anchors,
        )?;
    }
    let layer = UiLayoutLayer {
        dimensions,
        first_anchor,
        anchor_count: anchors.len() - first_anchor,
        anchors_present: anchors_element.is_some(),
        set_all_points: parse_optional_bool(path, element, "setAllPoints")?,
        hidden: parse_optional_bool(path, element, "hidden")?,
        alpha: parse_optional_number(path, element, "alpha")?,
        scale: parse_optional_number(path, element, "scale")?,
    };
    let present = layer.dimensions.is_some()
        || layer.anchors_present
        || layer.set_all_points.is_some()
        || layer.hidden.is_some()
        || layer.alpha.is_some()
        || layer.scale.is_some();
    Ok(present.then_some(layer))
}

fn parse_dimensions(
    path: &AssetPath,
    document: &XmlDocument,
    element: &XmlElement,
) -> Result<UiDimensions, UiLayoutError> {
    let values = child_named(document, element, "AbsDimension").unwrap_or(element);
    let dimensions = UiDimensions {
        width: parse_optional_number(path, values, "x")?,
        height: parse_optional_number(path, values, "y")?,
    };
    if dimensions.width.is_none() && dimensions.height.is_none() {
        return Err(layout_error(
            path,
            "Size has neither direct nor AbsDimension coordinates",
        ));
    }
    Ok(dimensions)
}

fn parse_anchors(
    path: &AssetPath,
    document: &XmlDocument,
    element: &XmlElement,
    parent_context: Option<&str>,
    allow_dynamic_parent: bool,
    output: &mut Vec<UiAnchor>,
) -> Result<(), UiLayoutError> {
    for content in element.content() {
        let XmlContent::Element(index) = content else {
            continue;
        };
        let anchor = document
            .element(*index)
            .ok_or_else(|| layout_error(path, "Anchor index is outside the XML arena"))?;
        if anchor.name() != "Anchor" {
            return Err(layout_error(
                path,
                format!("unsupported Anchors child {}", anchor.name()),
            ));
        }
        let point = attribute(anchor, "point")
            .ok_or_else(|| layout_error(path, "Anchor has no point"))
            .and_then(|value| parse_point(path, value))?;
        let relative_to = attribute(anchor, "relativeTo")
            .filter(|value| !value.is_empty())
            .map(|value| expand_parent(path, value, parent_context, allow_dynamic_parent))
            .transpose()?;
        let relative_point = attribute(anchor, "relativePoint")
            .filter(|value| !value.is_empty())
            .map(|value| parse_point(path, value))
            .transpose()?;
        let offset = match child_named(document, anchor, "Offset") {
            Some(offset) => Some(parse_offset(path, document, offset)?),
            None => parse_direct_offset(path, anchor)?,
        };
        output.push(UiAnchor {
            point,
            relative_to,
            relative_point,
            offset,
        });
    }
    Ok(())
}

fn parse_offset(
    path: &AssetPath,
    document: &XmlDocument,
    element: &XmlElement,
) -> Result<(f32, f32), UiLayoutError> {
    let absolute = child_named(document, element, "AbsDimension").unwrap_or(element);
    Ok((
        parse_required_number(path, absolute, "x")?,
        parse_required_number(path, absolute, "y")?,
    ))
}

/// Reads the compact anchor coordinates used throughout shipped GlueXML.
///
/// Build 12340 accepts `x` and `y` directly on `<Anchor>` in addition to the
/// older nested `<Offset>` spelling. Either coordinate may be omitted and the
/// schema supplies zero for that axis.
fn parse_direct_offset(
    path: &AssetPath,
    element: &XmlElement,
) -> Result<Option<(f32, f32)>, UiLayoutError> {
    let x = parse_optional_number(path, element, "x")?;
    let y = parse_optional_number(path, element, "y")?;
    Ok(match (x, y) {
        (None, None) => None,
        (x, y) => Some((x.unwrap_or(0.0), y.unwrap_or(0.0))),
    })
}

fn parse_point(path: &AssetPath, value: &str) -> Result<UiPoint, UiLayoutError> {
    match value {
        "TOPLEFT" => Ok(UiPoint::TopLeft),
        "TOP" => Ok(UiPoint::Top),
        "TOPRIGHT" => Ok(UiPoint::TopRight),
        "LEFT" => Ok(UiPoint::Left),
        "CENTER" => Ok(UiPoint::Center),
        "RIGHT" => Ok(UiPoint::Right),
        "BOTTOMLEFT" => Ok(UiPoint::BottomLeft),
        "BOTTOM" => Ok(UiPoint::Bottom),
        "BOTTOMRIGHT" => Ok(UiPoint::BottomRight),
        _ => Err(layout_error(
            path,
            format!("unsupported anchor point {value}"),
        )),
    }
}

fn parse_optional_bool(
    path: &AssetPath,
    element: &XmlElement,
    name: &str,
) -> Result<Option<bool>, UiLayoutError> {
    attribute(element, name)
        .map(|value| {
            if value.eq_ignore_ascii_case("true") {
                Ok(true)
            } else if value.eq_ignore_ascii_case("false") {
                Ok(false)
            } else {
                Err(layout_error(
                    path,
                    format!("invalid {name} boolean {value}"),
                ))
            }
        })
        .transpose()
}

fn parse_optional_number(
    path: &AssetPath,
    element: &XmlElement,
    name: &str,
) -> Result<Option<f32>, UiLayoutError> {
    attribute(element, name)
        .map(|value| {
            let number = value.parse::<f32>().map_err(|error| {
                layout_error(path, format!("invalid {name} value {value}: {error}"))
            })?;
            if !number.is_finite() {
                return Err(layout_error(
                    path,
                    format!("non-finite {name} value {value}"),
                ));
            }
            Ok(number)
        })
        .transpose()
}

fn parse_required_number(
    path: &AssetPath,
    element: &XmlElement,
    name: &str,
) -> Result<f32, UiLayoutError> {
    parse_optional_number(path, element, name)?
        .ok_or_else(|| layout_error(path, format!("missing numeric attribute {name}")))
}

fn expand_parent(
    path: &AssetPath,
    value: &str,
    parent_context: Option<&str>,
    allow_dynamic_parent: bool,
) -> Result<String, UiLayoutError> {
    if !value.contains("$parent") {
        return Ok(value.to_owned());
    }
    if let Some(parent) = parent_context {
        return Ok(value.replace("$parent", parent));
    }
    if allow_dynamic_parent {
        return Ok(value.to_owned());
    }
    Err(layout_error(
        path,
        format!("relativeTo {value} has no named parent context"),
    ))
}

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

fn attribute<'a>(element: &'a XmlElement, name: &str) -> Option<&'a str> {
    element
        .attributes()
        .iter()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn layout_error(path: &AssetPath, message: impl Into<String>) -> UiLayoutError {
    UiLayoutError::Layout {
        path: path.clone(),
        message: message.into(),
    }
}
