//! Typed stock texture properties and canonical archive paths.

use solarity_asset::AssetPath;
use thiserror::Error;

use crate::{UiObjectKind, UiObjectTree, XmlContent, XmlElement};

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
    /// Additive blending selected by `alphaMode="ADD"`.
    Add,
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
                    if let Some(layer) =
                        parse_layer(source.source_path(), source.document(), source.element())?
                    {
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
    path: &AssetPath,
    document: &crate::XmlDocument,
    element: &XmlElement,
) -> Result<Option<UiTextureLayer>, UiTextureError> {
    let file = attribute(element, "file")
        .map(|value| canonical_texture_file(path, value))
        .transpose()?;
    let blend_mode = attribute(element, "alphaMode")
        .map(|value| match value {
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
    let layer = UiTextureLayer {
        file,
        blend_mode,
        tex_coords,
    };
    let present = layer.file.is_some() || layer.blend_mode.is_some() || layer.tex_coords.is_some();
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
