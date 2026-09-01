//! Owned XML tree used by stock GlueXML and FrameXML construction.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use solarity_asset::AssetPath;

use crate::xml::UiLoadError;

/// One normalized XML attribute.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlAttribute {
    name: String,
    value: String,
}

impl XmlAttribute {
    /// Returns the qualified attribute name as written by the client data.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the XML-normalized attribute value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Ordered content owned by an XML element.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XmlContent {
    /// The arena index of a child element.
    Element(usize),
    /// Non-structural character data.
    Text(String),
}

/// One element in an owned document arena.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlElement {
    name: String,
    attributes: Vec<XmlAttribute>,
    content: Vec<XmlContent>,
}

impl XmlElement {
    /// Returns the qualified element name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns attributes in source order.
    #[must_use]
    pub fn attributes(&self) -> &[XmlAttribute] {
        &self.attributes
    }

    /// Returns child elements and text in source order.
    #[must_use]
    pub fn content(&self) -> &[XmlContent] {
        &self.content
    }
}

/// An owned, arena-backed stock UI XML document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlDocument {
    elements: Vec<XmlElement>,
    root: usize,
}

impl XmlDocument {
    /// Parses a UTF-8 GlueXML or FrameXML source file.
    ///
    /// # Errors
    ///
    /// Returns [`UiLoadError::Xml`] for malformed XML or an invalid document
    /// shape. Schema and template validation belong to later UI construction.
    pub fn parse(path: &AssetPath, source: &str) -> Result<Self, UiLoadError> {
        Self::parse_with_whitespace(path, source, false)
    }

    /// Parses inline-text markup without discarding whitespace-only runs.
    pub(crate) fn parse_preserving_text_whitespace(
        path: &AssetPath,
        source: &str,
    ) -> Result<Self, UiLoadError> {
        Self::parse_with_whitespace(path, source, true)
    }

    fn parse_with_whitespace(
        path: &AssetPath,
        source: &str,
        preserve_text_whitespace: bool,
    ) -> Result<Self, UiLoadError> {
        let mut reader = Reader::from_str(source);
        let mut elements = Vec::new();
        let mut stack = Vec::new();
        let mut root = None;

        loop {
            let event = reader.read_event().map_err(|error| UiLoadError::Xml {
                path: path.clone(),
                message: error.to_string(),
            })?;
            match event {
                Event::Start(start) => {
                    let index = push_element(path, start, &mut elements)?;
                    attach_element(path, index, &mut root, &stack, &mut elements)?;
                    stack.push(index);
                }
                Event::Empty(start) => {
                    let index = push_element(path, start, &mut elements)?;
                    attach_element(path, index, &mut root, &stack, &mut elements)?;
                }
                Event::End(_) => {
                    if stack.pop().is_none() {
                        return Err(xml_error(path, "closing element without an open element"));
                    }
                }
                Event::Text(text) => {
                    let value = text.xml10_content().into_owned();
                    if preserve_text_whitespace || !value.trim().is_empty() {
                        push_text(path, &stack, &mut elements, value)?;
                    }
                }
                Event::CData(text) => {
                    push_text(path, &stack, &mut elements, text.into_inner().into_owned())?;
                }
                Event::GeneralRef(reference) => {
                    let value = reference_text(path, &reference)?;
                    push_text(path, &stack, &mut elements, value)?;
                }
                Event::Eof => break,
                Event::Comment(_) | Event::Decl(_) | Event::DocType(_) | Event::PI(_) => {}
            }
        }

        if !stack.is_empty() {
            return Err(xml_error(path, "document ended with open elements"));
        }
        let root = root.ok_or_else(|| xml_error(path, "document has no root element"))?;
        Ok(Self { elements, root })
    }

    /// Returns the document root.
    #[must_use]
    pub fn root(&self) -> &XmlElement {
        &self.elements[self.root]
    }

    /// Returns an arena element by index.
    #[must_use]
    pub fn element(&self, index: usize) -> Option<&XmlElement> {
        self.elements.get(index)
    }

    /// Returns the number of elements in the document.
    #[must_use]
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }
}

fn push_element(
    path: &AssetPath,
    start: BytesStart<'_>,
    elements: &mut Vec<XmlElement>,
) -> Result<usize, UiLoadError> {
    let mut attributes = Vec::new();
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|error| UiLoadError::Xml {
            path: path.clone(),
            message: error.to_string(),
        })?;
        let value = attribute
            .normalized_value(XmlVersion::Explicit1_0)
            .map_err(|error| UiLoadError::Xml {
                path: path.clone(),
                message: error.to_string(),
            })?;
        attributes.push(XmlAttribute {
            name: attribute.key.into_inner().to_owned(),
            value: value.into_owned(),
        });
    }

    let index = elements.len();
    elements.push(XmlElement {
        name: start.name().as_ref().to_owned(),
        attributes,
        content: Vec::new(),
    });
    Ok(index)
}

fn attach_element(
    path: &AssetPath,
    index: usize,
    root: &mut Option<usize>,
    stack: &[usize],
    elements: &mut [XmlElement],
) -> Result<(), UiLoadError> {
    if let Some(parent) = stack.last() {
        elements[*parent].content.push(XmlContent::Element(index));
    } else if root.replace(index).is_some() {
        return Err(xml_error(path, "document has multiple root elements"));
    }
    Ok(())
}

fn push_text(
    path: &AssetPath,
    stack: &[usize],
    elements: &mut [XmlElement],
    value: String,
) -> Result<(), UiLoadError> {
    if let Some(parent) = stack.last() {
        elements[*parent].content.push(XmlContent::Text(value));
        Ok(())
    } else if value.trim().is_empty() {
        Ok(())
    } else {
        Err(xml_error(
            path,
            "character data appears outside the root element",
        ))
    }
}

fn reference_text(
    path: &AssetPath,
    reference: &quick_xml::events::BytesRef<'_>,
) -> Result<String, UiLoadError> {
    if let Some(character) = reference
        .resolve_char_ref()
        .map_err(|error| xml_error(path, error.to_string()))?
    {
        return Ok(character.to_string());
    }

    let value = match reference.as_ref() {
        "lt" => "<".to_owned(),
        "gt" => ">".to_owned(),
        "amp" => "&".to_owned(),
        "apos" => "'".to_owned(),
        "quot" => "\"".to_owned(),
        name => {
            return Err(xml_error(
                path,
                format!("unknown entity reference &{name};"),
            ));
        }
    };
    Ok(value)
}

fn xml_error(path: &AssetPath, message: impl Into<String>) -> UiLoadError {
    UiLoadError::Xml {
        path: path.clone(),
        message: message.into(),
    }
}
