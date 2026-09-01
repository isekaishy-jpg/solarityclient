//! Typed backdrop declarations retained in XML inheritance order.

use solarity_asset::AssetPath;

use crate::widget::canonical_texture_asset;
use crate::{
    UiBlendMode, UiElementLayer, UiFrameError, UiObjectKind, UiObjectTree, XmlContent, XmlDocument,
    XmlElement,
};

/// Explicit archive-backed or cleared backdrop texture state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiBackdropFile {
    /// One canonical `.BLP` path in the mounted client file stack.
    Asset(AssetPath),
    /// An explicit empty path that clears an inherited texture.
    Clear,
}

/// Properties contributed by one inherited `<Backdrop>` declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct UiBackdropLayer {
    pub(crate) background: Option<UiBackdropFile>,
    pub(crate) edge: Option<UiBackdropFile>,
    pub(crate) tiled: Option<bool>,
    pub(crate) tile_size: Option<f32>,
    pub(crate) edge_size: Option<f32>,
    pub(crate) insets: Option<[f32; 4]>,
    pub(crate) color: Option<[f32; 4]>,
    pub(crate) border_color: Option<[f32; 4]>,
    pub(crate) blend_mode: Option<UiBlendMode>,
}

impl UiBackdropLayer {
    /// Returns the explicit background file state.
    #[must_use]
    pub const fn background(&self) -> Option<&UiBackdropFile> {
        self.background.as_ref()
    }

    /// Returns the explicit border-atlas file state.
    #[must_use]
    pub const fn edge(&self) -> Option<&UiBackdropFile> {
        self.edge.as_ref()
    }

    /// Returns whether this layer explicitly enables background tiling.
    #[must_use]
    pub const fn tiled(&self) -> Option<bool> {
        self.tiled
    }

    /// Returns the explicitly authored logical background tile size.
    #[must_use]
    pub const fn tile_size(&self) -> Option<f32> {
        self.tile_size
    }

    /// Returns the explicitly authored logical edge and corner size.
    #[must_use]
    pub const fn edge_size(&self) -> Option<f32> {
        self.edge_size
    }

    /// Returns left, right, top, and bottom background insets.
    #[must_use]
    pub const fn insets(&self) -> Option<[f32; 4]> {
        self.insets
    }

    /// Returns the explicit background modulation color.
    #[must_use]
    pub const fn color(&self) -> Option<[f32; 4]> {
        self.color
    }

    /// Returns the explicit border modulation color.
    #[must_use]
    pub const fn border_color(&self) -> Option<[f32; 4]> {
        self.border_color
    }

    /// Returns the explicit source-alpha or additive blend mode.
    #[must_use]
    pub const fn blend_mode(&self) -> Option<UiBlendMode> {
        self.blend_mode
    }
}

/// Flat layer range belonging to one frame object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiBackdropNode {
    first_layer: usize,
    layer_count: usize,
}

impl UiBackdropNode {
    /// Returns the number of inherited backdrop declarations.
    #[must_use]
    pub const fn layer_count(self) -> usize {
        self.layer_count
    }
}

/// Backdrop declarations parallel to the complete object tree.
pub struct UiBackdropPlan {
    nodes: Vec<UiBackdropNode>,
    layers: Vec<UiBackdropLayer>,
}

impl UiBackdropPlan {
    /// Parses native backdrop declarations without loading their images.
    ///
    /// # Errors
    ///
    /// Returns [`UiFrameError::Property`] for malformed paths, booleans,
    /// numeric values, colors, or unsupported blend modes.
    pub fn from_tree(tree: &UiObjectTree<'_>) -> Result<Self, UiFrameError> {
        let mut plan = Self {
            nodes: Vec::with_capacity(tree.nodes().len()),
            layers: Vec::new(),
        };
        for node in tree.nodes() {
            let first_layer = plan.layers.len();
            if !matches!(
                node.kind(),
                UiObjectKind::Texture | UiObjectKind::FontString
            ) {
                for source in node.layers() {
                    if let Some(backdrop) =
                        child_named(source.document(), source.element(), "Backdrop")
                    {
                        plan.layers.push(parse_backdrop(source, backdrop)?);
                    }
                }
            }
            plan.nodes.push(UiBackdropNode {
                first_layer,
                layer_count: plan.layers.len() - first_layer,
            });
        }
        Ok(plan)
    }

    /// Returns backdrop metadata for one object-arena index.
    #[must_use]
    pub fn node(&self, index: usize) -> Option<UiBackdropNode> {
        self.nodes.get(index).copied()
    }

    /// Returns inherited declarations for one frame.
    #[must_use]
    pub fn layers_for(&self, node: UiBackdropNode) -> &[UiBackdropLayer] {
        &self.layers[node.first_layer..node.first_layer + node.layer_count]
    }

    /// Returns the total number of retained `<Backdrop>` declarations.
    #[must_use]
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
}

fn parse_backdrop(
    source: &UiElementLayer<'_>,
    element: &XmlElement,
) -> Result<UiBackdropLayer, UiFrameError> {
    let path = source.source_path();
    let document = source.document();
    let background = attribute(element, "bgFile")
        .map(|value| parse_file(path, value))
        .transpose()?;
    let edge = attribute(element, "edgeFile")
        .map(|value| parse_file(path, value))
        .transpose()?;
    let tiled = parse_optional_bool(path, element, "tile")?;
    let tile_size = child_named(document, element, "TileSize")
        .map(|value| parse_value(path, document, value, "TileSize"))
        .transpose()?;
    let edge_size = child_named(document, element, "EdgeSize")
        .map(|value| parse_value(path, document, value, "EdgeSize"))
        .transpose()?;
    let insets = child_named(document, element, "BackgroundInsets")
        .map(|value| parse_insets(path, document, value))
        .transpose()?;
    let color = child_named(document, element, "Color")
        .map(|value| parse_color(path, value))
        .transpose()?;
    let border_color = child_named(document, element, "BorderColor")
        .map(|value| parse_color(path, value))
        .transpose()?;
    let blend_mode = attribute(element, "alphaMode")
        .map(|value| match value {
            "BLEND" => Ok(UiBlendMode::Blend),
            "ADD" => Ok(UiBlendMode::Add),
            _ => Err(property_error(
                path,
                format!("unsupported Backdrop alphaMode {value}"),
            )),
        })
        .transpose()?;
    Ok(UiBackdropLayer {
        background,
        edge,
        tiled,
        tile_size,
        edge_size,
        insets,
        color,
        border_color,
        blend_mode,
    })
}

fn parse_file(path: &AssetPath, value: &str) -> Result<UiBackdropFile, UiFrameError> {
    if value.is_empty() {
        return Ok(UiBackdropFile::Clear);
    }
    canonical_texture_asset(value)
        .map(UiBackdropFile::Asset)
        .map_err(|message| property_error(path, format!("invalid Backdrop texture: {message}")))
}

fn parse_value(
    path: &AssetPath,
    document: &XmlDocument,
    element: &XmlElement,
    label: &str,
) -> Result<f32, UiFrameError> {
    let value = child_named(document, element, "AbsValue").unwrap_or(element);
    let number = attribute(value, "val")
        .ok_or_else(|| property_error(path, format!("Backdrop {label} has no val")))?
        .parse::<f32>()
        .map_err(|error| property_error(path, format!("invalid Backdrop {label}: {error}")))?;
    if !number.is_finite() || number <= 0.0 {
        return Err(property_error(
            path,
            format!("Backdrop {label} must be finite and positive"),
        ));
    }
    Ok(number)
}

fn parse_insets(
    path: &AssetPath,
    document: &XmlDocument,
    element: &XmlElement,
) -> Result<[f32; 4], UiFrameError> {
    let inset = child_named(document, element, "AbsInset").unwrap_or(element);
    Ok([
        parse_optional_number(path, inset, "left")?.unwrap_or(0.0),
        parse_optional_number(path, inset, "right")?.unwrap_or(0.0),
        parse_optional_number(path, inset, "top")?.unwrap_or(0.0),
        parse_optional_number(path, inset, "bottom")?.unwrap_or(0.0),
    ])
}

fn parse_color(path: &AssetPath, element: &XmlElement) -> Result<[f32; 4], UiFrameError> {
    Ok([
        parse_required_number(path, element, "r")?,
        parse_required_number(path, element, "g")?,
        parse_required_number(path, element, "b")?,
        parse_optional_number(path, element, "a")?.unwrap_or(1.0),
    ])
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
                Err(property_error(
                    path,
                    format!("invalid Backdrop {name} boolean {value}"),
                ))
            }
        })
        .transpose()
}

fn parse_optional_number(
    path: &AssetPath,
    element: &XmlElement,
    name: &str,
) -> Result<Option<f32>, UiFrameError> {
    attribute(element, name)
        .map(|value| {
            let number = value.parse::<f32>().map_err(|error| {
                property_error(path, format!("invalid Backdrop {name} {value}: {error}"))
            })?;
            if !number.is_finite() {
                return Err(property_error(
                    path,
                    format!("non-finite Backdrop {name} {value}"),
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
) -> Result<f32, UiFrameError> {
    parse_optional_number(path, element, name)?
        .ok_or_else(|| property_error(path, format!("Backdrop color has no {name}")))
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

fn property_error(path: &AssetPath, message: impl Into<String>) -> UiFrameError {
    UiFrameError::Property {
        path: path.clone(),
        message: message.into(),
    }
}
