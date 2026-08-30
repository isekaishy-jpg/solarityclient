//! Stock `<Font>` objects and load-order inheritance.

use std::collections::HashMap;

use solarity_asset::AssetPath;

use crate::font::FontError;
use crate::{UiBundle, UiResourceContent, XmlContent, XmlDocument, XmlElement};

/// Stock outline strength named by a font object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontOutline {
    /// The stock `NORMAL` outline.
    Normal,
    /// The stock `THICK` outline.
    Thick,
}

/// Horizontal string alignment explicitly selected by a font object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HorizontalJustification {
    /// Align to the left edge.
    Left,
    /// Center horizontally.
    Center,
    /// Align to the right edge.
    Right,
}

/// Vertical string alignment explicitly selected by a font object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerticalJustification {
    /// Align to the top edge.
    Top,
    /// Center vertically.
    Middle,
    /// Align to the bottom edge.
    Bottom,
}

/// RGB color with an optional explicitly supplied alpha component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontColor {
    red: f32,
    green: f32,
    blue: f32,
    alpha: Option<f32>,
}

impl FontColor {
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

    /// Returns alpha only when the XML supplied it explicitly.
    #[must_use]
    pub const fn alpha(self) -> Option<f32> {
        self.alpha
    }
}

/// Explicit shadow properties, independently inheritable by offset and color.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FontShadow {
    offset: Option<(f32, f32)>,
    color: Option<FontColor>,
}

impl FontShadow {
    /// Returns the explicit horizontal and vertical offset.
    #[must_use]
    pub const fn offset(self) -> Option<(f32, f32)> {
        self.offset
    }

    /// Returns the explicit shadow color.
    #[must_use]
    pub const fn color(self) -> Option<FontColor> {
        self.color
    }
}

/// One globally named font after ordered inheritance has been applied.
#[derive(Clone, Debug, PartialEq)]
pub struct FontDefinition {
    name: String,
    inherited_from: Vec<String>,
    face: Option<AssetPath>,
    height: Option<f32>,
    outline: Option<FontOutline>,
    monochrome: Option<bool>,
    spacing: Option<f32>,
    color: Option<FontColor>,
    shadow: Option<FontShadow>,
    horizontal: Option<HorizontalJustification>,
    vertical: Option<VerticalJustification>,
    virtual_object: Option<bool>,
}

impl FontDefinition {
    /// Returns the global font-object name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns direct parent names in XML order.
    #[must_use]
    pub fn inherited_from(&self) -> &[String] {
        &self.inherited_from
    }

    /// Returns the inherited archive font path, when supplied.
    #[must_use]
    pub const fn face(&self) -> Option<&AssetPath> {
        self.face.as_ref()
    }

    /// Returns the inherited pixel height, when supplied.
    #[must_use]
    pub const fn height(&self) -> Option<f32> {
        self.height
    }

    /// Returns the inherited outline selection, when supplied.
    #[must_use]
    pub const fn outline(&self) -> Option<FontOutline> {
        self.outline
    }

    /// Returns the inherited monochrome flag, when supplied.
    #[must_use]
    pub const fn monochrome(&self) -> Option<bool> {
        self.monochrome
    }

    /// Returns inherited additional glyph spacing, when supplied.
    #[must_use]
    pub const fn spacing(&self) -> Option<f32> {
        self.spacing
    }

    /// Returns the inherited primary color, when supplied.
    #[must_use]
    pub const fn color(&self) -> Option<FontColor> {
        self.color
    }

    /// Returns inherited shadow properties, when supplied.
    #[must_use]
    pub const fn shadow(&self) -> Option<FontShadow> {
        self.shadow
    }

    /// Returns inherited horizontal justification, when supplied.
    #[must_use]
    pub const fn horizontal_justification(&self) -> Option<HorizontalJustification> {
        self.horizontal
    }

    /// Returns inherited vertical justification, when supplied.
    #[must_use]
    pub const fn vertical_justification(&self) -> Option<VerticalJustification> {
        self.vertical
    }

    /// Returns whether XML explicitly identifies the object as virtual.
    #[must_use]
    pub const fn virtual_object(&self) -> Option<bool> {
        self.virtual_object
    }

    fn empty(name: String) -> Self {
        Self {
            name,
            inherited_from: Vec::new(),
            face: None,
            height: None,
            outline: None,
            monochrome: None,
            spacing: None,
            color: None,
            shadow: None,
            horizontal: None,
            vertical: None,
            virtual_object: None,
        }
    }

    fn inherit(&mut self, parent: &Self) {
        self.face.clone_from(&parent.face);
        self.height = parent.height;
        self.outline = parent.outline;
        self.monochrome = parent.monochrome;
        self.spacing = parent.spacing;
        self.color = parent.color;
        self.shadow = parent.shadow;
        self.horizontal = parent.horizontal;
        self.vertical = parent.vertical;
        self.virtual_object = parent.virtual_object;
    }
}

/// Globally named fonts constructed in built-in XML load order.
pub struct FontCatalog {
    definitions: Vec<FontDefinition>,
    by_name: HashMap<String, usize>,
}

impl FontCatalog {
    /// Constructs all root-level `<Font>` objects in an ordered UI bundle.
    ///
    /// # Errors
    ///
    /// Returns [`FontError::Definition`] for invalid values, duplicate names,
    /// or a parent that has not already been defined. Forward-searching for an
    /// inheritance target would change stock load-order behavior.
    pub fn from_bundle(bundle: &UiBundle) -> Result<Self, FontError> {
        let mut catalog = Self {
            definitions: Vec::new(),
            by_name: HashMap::new(),
        };
        for resource in bundle.resources() {
            let UiResourceContent::Xml(document) = resource.content() else {
                continue;
            };
            catalog.read_document(resource.path(), document)?;
        }
        Ok(catalog)
    }

    /// Returns definitions in construction order.
    #[must_use]
    pub fn definitions(&self) -> &[FontDefinition] {
        &self.definitions
    }

    /// Finds a global font by its case-sensitive XML name.
    #[must_use]
    pub fn definition(&self, name: &str) -> Option<&FontDefinition> {
        self.by_name
            .get(name)
            .and_then(|index| self.definitions.get(*index))
    }

    fn read_document(&mut self, path: &AssetPath, document: &XmlDocument) -> Result<(), FontError> {
        for content in document.root().content() {
            let XmlContent::Element(index) = content else {
                continue;
            };
            let Some(element) = document.element(*index) else {
                return Err(definition_error(
                    path,
                    "XML child index is outside the arena",
                ));
            };
            if element.name() != "Font" {
                continue;
            }
            let definition = self.parse_definition(path, document, element)?;
            if self.by_name.contains_key(definition.name()) {
                return Err(definition_error(
                    path,
                    format!("duplicate font name {}", definition.name()),
                ));
            }
            let index = self.definitions.len();
            self.by_name.insert(definition.name.clone(), index);
            self.definitions.push(definition);
        }
        Ok(())
    }

    fn parse_definition(
        &self,
        path: &AssetPath,
        document: &XmlDocument,
        element: &XmlElement,
    ) -> Result<FontDefinition, FontError> {
        let name = attribute(element, "name")
            .ok_or_else(|| definition_error(path, "root-level Font has no name"))?
            .to_owned();
        let mut definition = FontDefinition::empty(name.clone());

        if let Some(parents) = attribute(element, "inherits") {
            for parent_name in parents.split(',').map(str::trim) {
                let Some(parent) = self.definition(parent_name) else {
                    return Err(definition_error(
                        path,
                        format!("font {name} inherits unavailable font {parent_name}"),
                    ));
                };
                definition.inherit(parent);
                definition.inherited_from.push(parent_name.to_owned());
            }
        }

        if let Some(value) = attribute(element, "font") {
            definition.face = Some(
                AssetPath::new(value).map_err(|error| definition_error(path, error.to_string()))?,
            );
        }
        if let Some(value) = attribute(element, "outline") {
            definition.outline = Some(parse_outline(path, value)?);
        }
        if let Some(value) = attribute(element, "monochrome") {
            definition.monochrome = Some(parse_bool(path, "monochrome", value)?);
        }
        if let Some(value) = attribute(element, "virtual") {
            definition.virtual_object = Some(parse_bool(path, "virtual", value)?);
        }
        if let Some(value) = attribute(element, "spacing") {
            definition.spacing = Some(parse_number(path, "spacing", value)?);
        }
        if let Some(value) = attribute(element, "justifyH") {
            definition.horizontal = Some(parse_horizontal(path, value)?);
        }
        if let Some(value) = attribute(element, "justifyV") {
            definition.vertical = Some(parse_vertical(path, value)?);
        }

        for content in element.content() {
            let XmlContent::Element(index) = content else {
                continue;
            };
            let child = document
                .element(*index)
                .ok_or_else(|| definition_error(path, "XML child index is outside the arena"))?;
            match child.name() {
                "FontHeight" => {
                    definition.height = Some(parse_font_height(path, document, child)?);
                }
                "Color" => definition.color = Some(parse_color(path, child)?),
                "Shadow" => {
                    definition.shadow = Some(parse_shadow(
                        path,
                        document,
                        child,
                        definition.shadow.unwrap_or_default(),
                    )?);
                }
                _ => {}
            }
        }
        Ok(definition)
    }
}

fn parse_font_height(
    path: &AssetPath,
    document: &XmlDocument,
    element: &XmlElement,
) -> Result<f32, FontError> {
    let value = child_named(document, element, "AbsValue")
        .and_then(|child| attribute(child, "val"))
        .ok_or_else(|| definition_error(path, "FontHeight has no AbsValue val"))?;
    let height = parse_number(path, "font height", value)?;
    if height <= 0.0 {
        return Err(definition_error(path, "font height must be positive"));
    }
    Ok(height)
}

fn parse_shadow(
    path: &AssetPath,
    document: &XmlDocument,
    element: &XmlElement,
    mut shadow: FontShadow,
) -> Result<FontShadow, FontError> {
    if let Some(offset) = child_named(document, element, "Offset") {
        let dimension = child_named(document, offset, "AbsDimension")
            .ok_or_else(|| definition_error(path, "Shadow Offset has no AbsDimension"))?;
        let x = parse_required_number(path, dimension, "x")?;
        let y = parse_required_number(path, dimension, "y")?;
        shadow.offset = Some((x, y));
    }
    if let Some(color) = child_named(document, element, "Color") {
        shadow.color = Some(parse_color(path, color)?);
    }
    Ok(shadow)
}

fn parse_color(path: &AssetPath, element: &XmlElement) -> Result<FontColor, FontError> {
    Ok(FontColor {
        red: parse_required_number(path, element, "r")?,
        green: parse_required_number(path, element, "g")?,
        blue: parse_required_number(path, element, "b")?,
        alpha: attribute(element, "a")
            .map(|value| parse_number(path, "color alpha", value))
            .transpose()?,
    })
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

fn parse_required_number(
    path: &AssetPath,
    element: &XmlElement,
    name: &str,
) -> Result<f32, FontError> {
    let value = attribute(element, name)
        .ok_or_else(|| definition_error(path, format!("missing numeric attribute {name}")))?;
    parse_number(path, name, value)
}

fn parse_number(path: &AssetPath, field: &str, value: &str) -> Result<f32, FontError> {
    let number = value.parse::<f32>().map_err(|error| {
        definition_error(path, format!("invalid {field} value {value}: {error}"))
    })?;
    if !number.is_finite() {
        return Err(definition_error(
            path,
            format!("non-finite {field} value {value}"),
        ));
    }
    Ok(number)
}

fn parse_bool(path: &AssetPath, field: &str, value: &str) -> Result<bool, FontError> {
    if value.eq_ignore_ascii_case("true") {
        Ok(true)
    } else if value.eq_ignore_ascii_case("false") {
        Ok(false)
    } else {
        Err(definition_error(
            path,
            format!("invalid {field} boolean {value}"),
        ))
    }
}

fn parse_outline(path: &AssetPath, value: &str) -> Result<FontOutline, FontError> {
    match value {
        "NORMAL" => Ok(FontOutline::Normal),
        "THICK" => Ok(FontOutline::Thick),
        _ => Err(definition_error(
            path,
            format!("unsupported outline value {value}"),
        )),
    }
}

fn parse_horizontal(path: &AssetPath, value: &str) -> Result<HorizontalJustification, FontError> {
    match value {
        "LEFT" => Ok(HorizontalJustification::Left),
        "CENTER" => Ok(HorizontalJustification::Center),
        "RIGHT" => Ok(HorizontalJustification::Right),
        _ => Err(definition_error(
            path,
            format!("unsupported horizontal justification {value}"),
        )),
    }
}

fn parse_vertical(path: &AssetPath, value: &str) -> Result<VerticalJustification, FontError> {
    match value {
        "TOP" => Ok(VerticalJustification::Top),
        "MIDDLE" => Ok(VerticalJustification::Middle),
        "BOTTOM" => Ok(VerticalJustification::Bottom),
        _ => Err(definition_error(
            path,
            format!("unsupported vertical justification {value}"),
        )),
    }
}

fn definition_error(path: &AssetPath, message: impl Into<String>) -> FontError {
    FontError::Definition {
        path: path.clone(),
        message: message.into(),
    }
}
