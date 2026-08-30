//! Typed stock texture properties and canonical archive paths.

use solarity_asset::AssetPath;
use thiserror::Error;

use crate::{UiDrawLayer, UiObjectKind, UiObjectTree, XmlContent, XmlElement};

/// A failure while decoding a stock texture declaration.
#[derive(Debug, Error)]
pub enum UiTextureError {
    /// A texture property or path violated observed stock invariants.
    #[error("invalid UI texture in {path}: {message}")]
    Texture {
        /// XML source containing the declaration.
        path: AssetPath,
        /// File, blend-mode, or coordinate context.
        message: String,
    },
    /// Compact texture-state resolution failed after parsing.
    #[error("could not resolve UI texture state: {message}")]
    Resolution {
        /// Arena or compact-index context.
        message: String,
    },
}

/// Explicit file state contributed by one texture XML layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiTextureFile {
    /// A canonical `.BLP` archive path.
    Asset(AssetPath),
    /// An explicit empty value awaiting runtime assignment.
    Dynamic,
}

/// Explicit stock texture blend behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiBlendMode {
    /// Ordinary source-alpha blending selected by `alphaMode="BLEND"`.
    Blend,
    /// Additive blending selected by `alphaMode="ADD"`.
    Add,
}

/// Explicit texture color components.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiTextureColor {
    red: f32,
    green: f32,
    blue: f32,
    alpha: Option<f32>,
}

impl UiTextureColor {
    /// Returns the red component.
    #[must_use]
    pub const fn red(self) -> f32 {
        self.red
    }

    /// Returns the green component.
    #[must_use]
    pub const fn green(self) -> f32 {
        self.green
    }

    /// Returns the blue component.
    #[must_use]
    pub const fn blue(self) -> f32 {
        self.blue
    }

    /// Returns the explicitly supplied alpha component.
    #[must_use]
    pub const fn alpha(self) -> Option<f32> {
        self.alpha
    }
}

/// Direction used to interpolate a stock XML texture gradient.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiGradientOrientation {
    /// Interpolate from the lower edge to the upper edge.
    Vertical,
}

/// Explicit color endpoints for one texture gradient.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiTextureGradient {
    orientation: UiGradientOrientation,
    minimum: UiTextureColor,
    maximum: UiTextureColor,
}

impl UiTextureGradient {
    /// Returns the interpolation direction.
    #[must_use]
    pub const fn orientation(self) -> UiGradientOrientation {
        self.orientation
    }

    /// Returns the minimum endpoint color.
    #[must_use]
    pub const fn minimum(self) -> UiTextureColor {
        self.minimum
    }

    /// Returns the maximum endpoint color.
    #[must_use]
    pub const fn maximum(self) -> UiTextureColor {
        self.maximum
    }
}

/// Texture coordinates explicitly supplied by one XML layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiTexCoords {
    left: Option<f32>,
    right: Option<f32>,
    top: Option<f32>,
    bottom: Option<f32>,
}

impl UiTexCoords {
    /// Returns the explicit left coordinate.
    #[must_use]
    pub const fn left(self) -> Option<f32> {
        self.left
    }

    /// Returns the explicit right coordinate.
    #[must_use]
    pub const fn right(self) -> Option<f32> {
        self.right
    }

    /// Returns the explicit top coordinate.
    #[must_use]
    pub const fn top(self) -> Option<f32> {
        self.top
    }

    /// Returns the explicit bottom coordinate.
    #[must_use]
    pub const fn bottom(self) -> Option<f32> {
        self.bottom
    }
}

/// Texture properties contributed by one XML inheritance layer.
#[derive(Clone, Debug, PartialEq)]
pub struct UiTextureLayer {
    file: Option<UiTextureFile>,
    blend_mode: Option<UiBlendMode>,
    tex_coords: Option<UiTexCoords>,
    color: Option<UiTextureColor>,
    gradient: Option<UiTextureGradient>,
    horizontal_tiling: Option<bool>,
    vertical_tiling: Option<bool>,
    non_blocking: Option<bool>,
    draw_layer: Option<UiDrawLayer>,
    draw_sub_level: Option<i16>,
}

impl UiTextureLayer {
    /// Returns the explicit canonical file or dynamic assignment marker.
    #[must_use]
    pub const fn file(&self) -> Option<&UiTextureFile> {
        self.file.as_ref()
    }

    /// Returns the explicit blend mode.
    #[must_use]
    pub const fn blend_mode(&self) -> Option<UiBlendMode> {
        self.blend_mode
    }

    /// Returns explicitly supplied texture coordinates.
    #[must_use]
    pub const fn tex_coords(&self) -> Option<UiTexCoords> {
        self.tex_coords
    }

    /// Returns the explicit uniform vertex color.
    #[must_use]
    pub const fn color(&self) -> Option<UiTextureColor> {
        self.color
    }

    /// Returns the explicit vertex-color gradient.
    #[must_use]
    pub const fn gradient(&self) -> Option<UiTextureGradient> {
        self.gradient
    }

    /// Returns the explicit horizontal-tiling flag.
    #[must_use]
    pub const fn horizontal_tiling(&self) -> Option<bool> {
        self.horizontal_tiling
    }

    /// Returns the explicit vertical-tiling flag.
    #[must_use]
    pub const fn vertical_tiling(&self) -> Option<bool> {
        self.vertical_tiling
    }

    /// Returns the explicit non-blocking load flag.
    #[must_use]
    pub const fn non_blocking(&self) -> Option<bool> {
        self.non_blocking
    }

    /// Returns the enclosing stock draw band when this layer supplies one.
    #[must_use]
    pub const fn draw_layer(&self) -> Option<UiDrawLayer> {
        self.draw_layer
    }

    /// Returns the explicit ordering offset within the draw band.
    #[must_use]
    pub const fn draw_sub_level(&self) -> Option<i16> {
        self.draw_sub_level
    }
}

/// Flat layer range belonging to one texture object node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiTextureNode {
    first_layer: usize,
    layer_count: usize,
}

impl UiTextureNode {
    /// Returns the number of property-bearing texture layers.
    #[must_use]
    pub const fn layer_count(self) -> usize {
        self.layer_count
    }
}

/// Flat typed texture properties parallel to the complete object tree.
pub struct UiTexturePlan {
    nodes: Vec<UiTextureNode>,
    layers: Vec<UiTextureLayer>,
}

impl UiTexturePlan {
    /// Parses all texture objects without loading image payloads eagerly.
    ///
    /// # Errors
    ///
    /// Returns [`UiTextureError::Texture`] for unsupported extensions, blend
    /// modes, non-finite coordinates, or invalid archive paths.
    pub fn from_tree(tree: &UiObjectTree<'_>) -> Result<Self, UiTextureError> {
        let mut plan = Self {
            nodes: Vec::with_capacity(tree.nodes().len()),
            layers: Vec::new(),
        };
        for node in tree.nodes() {
            let first_layer = plan.layers.len();
            if node.kind() == UiObjectKind::Texture {
                for source in node.layers() {
                    if let Some(layer) = parse_layer(source)? {
                        plan.layers.push(layer);
                    }
                }
            }
            plan.nodes.push(UiTextureNode {
                first_layer,
                layer_count: plan.layers.len() - first_layer,
            });
        }
        Ok(plan)
    }

    /// Returns texture metadata for one object node.
    #[must_use]
    pub fn node(&self, index: usize) -> Option<UiTextureNode> {
        self.nodes.get(index).copied()
    }

    /// Returns property-bearing layers for one texture node.
    #[must_use]
    pub fn layers_for(&self, node: UiTextureNode) -> &[UiTextureLayer] {
        &self.layers[node.first_layer..node.first_layer + node.layer_count]
    }

    /// Returns the total number of retained texture layers.
    #[must_use]
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
}

fn parse_layer(
    source: &crate::UiElementLayer<'_>,
) -> Result<Option<UiTextureLayer>, UiTextureError> {
    let path = source.source_path();
    let document = source.document();
    let element = source.element();
    let file = attribute(element, "file")
        .map(|value| canonical_texture_file(path, value))
        .transpose()?;
    let blend_mode = attribute(element, "alphaMode")
        .map(|value| match value {
            "BLEND" => Ok(UiBlendMode::Blend),
            "ADD" => Ok(UiBlendMode::Add),
            _ => Err(texture_error(
                path,
                format!("unsupported alphaMode {value}"),
            )),
        })
        .transpose()?;
    let tex_coords = child_named(document, element, "TexCoords")
        .map(|coords| parse_tex_coords(path, coords))
        .transpose()?;
    let color = child_named(document, element, "Color")
        .map(|color| parse_color(path, color))
        .transpose()?;
    let gradient = child_named(document, element, "Gradient")
        .map(|gradient| parse_gradient(path, document, gradient))
        .transpose()?;
    let layer = UiTextureLayer {
        file,
        blend_mode,
        tex_coords,
        color,
        gradient,
        horizontal_tiling: parse_optional_bool(path, element, "horizTile")?,
        vertical_tiling: parse_optional_bool(path, element, "vertTile")?,
        non_blocking: parse_optional_bool(path, element, "nonBlocking")?,
        draw_layer: source.draw_layer(),
        draw_sub_level: parse_optional_integer(path, element, "subLevel")?,
    };
    let present = layer.file.is_some()
        || layer.blend_mode.is_some()
        || layer.tex_coords.is_some()
        || layer.color.is_some()
        || layer.gradient.is_some()
        || layer.horizontal_tiling.is_some()
        || layer.vertical_tiling.is_some()
        || layer.non_blocking.is_some()
        || layer.draw_layer.is_some()
        || layer.draw_sub_level.is_some();
    Ok(present.then_some(layer))
}

fn canonical_texture_file(path: &AssetPath, value: &str) -> Result<UiTextureFile, UiTextureError> {
    if value.is_empty() {
        return Ok(UiTextureFile::Dynamic);
    }
    let lower = value.to_ascii_lowercase();
    let canonical = if lower.ends_with(".tga") {
        format!("{}.blp", &value[..value.len() - 4])
    } else if lower.ends_with(".blp") {
        value.to_owned()
    } else if value
        .rsplit(['\\', '/'])
        .next()
        .is_some_and(|component| !component.contains('.'))
    {
        format!("{value}.blp")
    } else {
        return Err(texture_error(
            path,
            format!("unsupported texture extension in {value}"),
        ));
    };
    AssetPath::new(canonical)
        .map(UiTextureFile::Asset)
        .map_err(|error| texture_error(path, error.to_string()))
}

fn parse_tex_coords(path: &AssetPath, element: &XmlElement) -> Result<UiTexCoords, UiTextureError> {
    Ok(UiTexCoords {
        left: parse_optional_number(path, element, "left")?,
        right: parse_optional_number(path, element, "right")?,
        top: parse_optional_number(path, element, "top")?,
        bottom: parse_optional_number(path, element, "bottom")?,
    })
}

fn parse_color(path: &AssetPath, element: &XmlElement) -> Result<UiTextureColor, UiTextureError> {
    Ok(UiTextureColor {
        red: parse_required_number(path, element, "r")?,
        green: parse_required_number(path, element, "g")?,
        blue: parse_required_number(path, element, "b")?,
        alpha: parse_optional_number(path, element, "a")?,
    })
}

fn parse_gradient(
    path: &AssetPath,
    document: &crate::XmlDocument,
    element: &XmlElement,
) -> Result<UiTextureGradient, UiTextureError> {
    let orientation = match attribute(element, "orientation") {
        Some("VERTICAL") => UiGradientOrientation::Vertical,
        Some(value) => {
            return Err(texture_error(
                path,
                format!("unsupported gradient orientation {value}"),
            ));
        }
        None => return Err(texture_error(path, "Gradient has no orientation")),
    };
    let minimum = child_named(document, element, "MinColor")
        .ok_or_else(|| texture_error(path, "Gradient has no MinColor"))
        .and_then(|color| parse_color(path, color))?;
    let maximum = child_named(document, element, "MaxColor")
        .ok_or_else(|| texture_error(path, "Gradient has no MaxColor"))
        .and_then(|color| parse_color(path, color))?;
    Ok(UiTextureGradient {
        orientation,
        minimum,
        maximum,
    })
}

fn parse_optional_bool(
    path: &AssetPath,
    element: &XmlElement,
    name: &str,
) -> Result<Option<bool>, UiTextureError> {
    attribute(element, name)
        .map(|value| {
            if value.eq_ignore_ascii_case("true") {
                Ok(true)
            } else if value.eq_ignore_ascii_case("false") {
                Ok(false)
            } else {
                Err(texture_error(
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
) -> Result<Option<f32>, UiTextureError> {
    attribute(element, name)
        .map(|value| {
            let number = value.parse::<f32>().map_err(|error| {
                texture_error(path, format!("invalid {name} value {value}: {error}"))
            })?;
            if !number.is_finite() {
                return Err(texture_error(
                    path,
                    format!("non-finite {name} value {value}"),
                ));
            }
            Ok(number)
        })
        .transpose()
}

fn parse_optional_integer(
    path: &AssetPath,
    element: &XmlElement,
    name: &str,
) -> Result<Option<i16>, UiTextureError> {
    attribute(element, name)
        .map(|value| {
            value.parse::<i16>().map_err(|error| {
                texture_error(path, format!("invalid {name} value {value}: {error}"))
            })
        })
        .transpose()
}

fn parse_required_number(
    path: &AssetPath,
    element: &XmlElement,
    name: &str,
) -> Result<f32, UiTextureError> {
    parse_optional_number(path, element, name)?
        .ok_or_else(|| texture_error(path, format!("missing numeric attribute {name}")))
}

fn child_named<'a>(
    document: &'a crate::XmlDocument,
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

fn texture_error(path: &AssetPath, message: impl Into<String>) -> UiTextureError {
    UiTextureError::Texture {
        path: path.clone(),
        message: message.into(),
    }
}
