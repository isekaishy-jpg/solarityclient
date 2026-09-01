//! Restricted HTML parsing, font selection, and wrapped block layout.

use solarity_asset::{AssetError, AssetPath, AssetStore, LocalizedDocument};
use thiserror::Error;

use crate::{
    FontCatalog, FontError, FontRasterization, FontSystem, UiLoadError, UiObjectKind, UiObjectTree,
    UiRegionStatePlan, XmlContent, XmlDocument, XmlElement,
};

/// A failure while constructing stock `CSimpleHTML` document state.
#[derive(Debug, Error)]
pub enum UiSimpleHtmlError {
    /// The selected locale document could not be resolved or read.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// The locale document could not enter the UI XML tree.
    #[error(transparent)]
    Load(#[from] UiLoadError),
    /// An archive-backed font could not be measured.
    #[error(transparent)]
    Font(#[from] FontError),
    /// A `SimpleHTML` declaration or document violated the recovered contract.
    #[error("invalid SimpleHTML state for {object}: {message}")]
    Declaration {
        /// Expanded object name or stable arena identity.
        object: String,
        /// Structural context.
        message: String,
    },
}

/// One of the four font objects retained by stock `CSimpleHTML`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiSimpleHtmlFontSlot {
    /// Paragraph and explicit line-break text.
    Normal,
    /// `<h1>` text.
    Header1,
    /// `<h2>` text.
    Header2,
    /// `<h3>` text.
    Header3,
}

impl UiSimpleHtmlFontSlot {
    const fn index(self) -> usize {
        match self {
            Self::Normal => 0,
            Self::Header1 => 1,
            Self::Header2 => 2,
            Self::Header3 => 3,
        }
    }
}

/// Horizontal alignment accepted by the restricted stock HTML parser.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiSimpleHtmlAlignment {
    /// Anchor text to the left edge.
    Left,
    /// Center text within the widget width.
    Center,
    /// Anchor text to the right edge.
    Right,
}

/// One semantic text block produced by `ParseBODY` and `ParseP`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSimpleHtmlBlock {
    font: UiSimpleHtmlFontSlot,
    alignment: UiSimpleHtmlAlignment,
    text: String,
}

impl UiSimpleHtmlBlock {
    /// Returns the stock font slot selected by the source tag.
    #[must_use]
    pub const fn font(&self) -> UiSimpleHtmlFontSlot {
        self.font
    }

    /// Returns the source paragraph alignment.
    #[must_use]
    pub const fn alignment(&self) -> UiSimpleHtmlAlignment {
        self.alignment
    }

    /// Returns collapsed paragraph text with explicit breaks retained as `|n`.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// One measured output line ready for glyph placement.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSimpleHtmlLine {
    font: UiSimpleHtmlFontSlot,
    font_object: String,
    alignment: UiSimpleHtmlAlignment,
    text: String,
    width: f32,
    top: f32,
    height: f32,
}

/// One parsed locale document before widget-specific font layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSimpleHtmlDocument {
    source: LocalizedDocument,
    blocks: Vec<UiSimpleHtmlBlock>,
}

impl UiSimpleHtmlDocument {
    /// Parses the exact restricted tag set accepted by stock `CSimpleHTML`.
    ///
    /// # Errors
    ///
    /// Returns a UTF-8 or XML error when the locale file cannot enter the
    /// retained document tree.
    pub fn parse(source: LocalizedDocument, bytes: &[u8]) -> Result<Self, UiSimpleHtmlError> {
        Ok(Self {
            source,
            blocks: parse_document(source, bytes)?,
        })
    }

    /// Returns the locale-root source identity.
    #[must_use]
    pub const fn source(&self) -> LocalizedDocument {
        self.source
    }

    /// Returns semantic blocks in source order.
    #[must_use]
    pub fn blocks(&self) -> &[UiSimpleHtmlBlock] {
        &self.blocks
    }
}

impl UiSimpleHtmlLine {
    /// Returns the source font family used by this line.
    #[must_use]
    pub const fn font(&self) -> UiSimpleHtmlFontSlot {
        self.font
    }

    /// Returns the resolved global font-object name.
    #[must_use]
    pub fn font_object(&self) -> &str {
        &self.font_object
    }

    /// Returns the line alignment within the owning widget.
    #[must_use]
    pub const fn alignment(&self) -> UiSimpleHtmlAlignment {
        self.alignment
    }

    /// Returns the line text after width wrapping.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the measured width in stock UI units.
    #[must_use]
    pub const fn width(&self) -> f32 {
        self.width
    }

    /// Returns the top edge below the widget's top edge in UI units.
    #[must_use]
    pub const fn top(&self) -> f32 {
        self.top
    }

    /// Returns the selected font line height in UI units.
    #[must_use]
    pub const fn height(&self) -> f32 {
        self.height
    }
}

/// One static `SimpleHTML` object after locale load and line wrapping.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSimpleHtmlNode {
    source: Option<LocalizedDocument>,
    document: Option<UiSimpleHtmlDocument>,
    lines: Vec<UiSimpleHtmlLine>,
    content_height: f32,
    clip_object: Option<usize>,
}

impl UiSimpleHtmlNode {
    /// Returns the locale-root file selected by the XML `file` attribute.
    #[must_use]
    pub const fn source(&self) -> Option<LocalizedDocument> {
        self.source
    }

    /// Returns semantic blocks in source order.
    #[must_use]
    pub fn blocks(&self) -> &[UiSimpleHtmlBlock] {
        self.document
            .as_ref()
            .map_or(&[], UiSimpleHtmlDocument::blocks)
    }

    /// Returns measured lines in top-to-bottom order.
    #[must_use]
    pub fn lines(&self) -> &[UiSimpleHtmlLine] {
        &self.lines
    }

    /// Returns the complete internal text height in stock UI units.
    #[must_use]
    pub const fn content_height(&self) -> f32 {
        self.content_height
    }

    /// Returns the nearest owning scroll-frame viewport, when present.
    #[must_use]
    pub const fn clip_object(&self) -> Option<usize> {
        self.clip_object
    }
}

/// Arena-aligned static state for every authored `SimpleHTML` object.
#[derive(Clone, Debug)]
pub struct UiSimpleHtmlPlan {
    nodes: Vec<Option<UiSimpleHtmlNode>>,
}

impl UiSimpleHtmlPlan {
    pub(crate) fn empty(object_count: usize) -> Self {
        Self {
            nodes: vec![None; object_count],
        }
    }

    pub(crate) fn requires_asset_store(tree: &UiObjectTree<'_>) -> bool {
        tree.nodes().iter().any(|object| {
            object.kind() == UiObjectKind::SimpleHtml
                && object.layers().iter().any(|layer| {
                    attribute(layer.element(), "file").is_some_and(|name| !name.is_empty())
                })
        })
    }

    /// Loads locale documents and reproduces stock paragraph wrapping.
    ///
    /// Documents are read only through the selected locale root. Header font
    /// slots fall back to the normal slot exactly as `CSimpleHTML::AddText`
    /// does when a requested header font was not supplied.
    ///
    /// # Errors
    ///
    /// Returns the first locale read, UTF-8/XML parse, font, or declaration
    /// error. No operating-system font or alternate locale is substituted.
    pub fn resolve(
        tree: &UiObjectTree<'_>,
        regions: &UiRegionStatePlan,
        fonts: &FontCatalog,
        assets: &mut AssetStore,
        logical_height: u32,
    ) -> Result<Self, UiSimpleHtmlError> {
        let pixels_per_ui_unit = logical_height as f64 / 768.0;
        let mut font_system = FontSystem::new()?;
        let mut nodes = Vec::with_capacity(tree.nodes().len());
        for (index, object) in tree.nodes().iter().enumerate() {
            if object.kind() != UiObjectKind::SimpleHtml {
                nodes.push(None);
                continue;
            }
            let label = object
                .name()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("object {index}"));
            let styles = font_styles(object, fonts, &label)?;
            let source = object
                .layers()
                .iter()
                .filter_map(|layer| attribute(layer.element(), "file"))
                .next_back()
                .filter(|name| !name.is_empty())
                .map(str::parse)
                .transpose()?;
            let document = match source {
                Some(document) => match assets.read_localized_document(document)? {
                    Some(bytes) => Some(UiSimpleHtmlDocument::parse(document, &bytes)?),
                    None => None,
                },
                None => None,
            };
            let width = regions
                .state(index)
                .map(|state| state.width())
                .ok_or_else(|| declaration(&label, "object has no resolved region width"))?;
            let (lines, content_height) = wrap_blocks(
                document.as_ref().map_or(&[], UiSimpleHtmlDocument::blocks),
                &styles,
                fonts,
                f64::from(width),
                pixels_per_ui_unit,
                assets,
                &mut font_system,
                &label,
            )?;
            nodes.push(Some(UiSimpleHtmlNode {
                source,
                document,
                lines,
                content_height: content_height as f32,
                clip_object: nearest_scroll_frame(tree, object.parent()),
            }));
        }
        Ok(Self { nodes })
    }

    /// Returns the state aligned with one object-arena index.
    #[must_use]
    pub fn node(&self, object_index: usize) -> Option<&UiSimpleHtmlNode> {
        self.nodes.get(object_index).and_then(Option::as_ref)
    }
}

/// Finds the viewport that clips this retained scroll child.
fn nearest_scroll_frame(tree: &UiObjectTree<'_>, mut parent: Option<usize>) -> Option<usize> {
    while let Some(index) = parent {
        let object = tree.nodes().get(index)?;
        if object.kind() == UiObjectKind::ScrollFrame {
            return Some(index);
        }
        parent = object.parent();
    }
    None
}

#[derive(Clone, Debug)]
struct HtmlFontStyle {
    object: String,
    spacing: f64,
}

fn font_styles(
    object: &crate::UiObjectNode<'_>,
    fonts: &FontCatalog,
    label: &str,
) -> Result<[Option<HtmlFontStyle>; 4], UiSimpleHtmlError> {
    let mut styles = [None, None, None, None];
    for layer in object.layers() {
        if let Some(font) = attribute(layer.element(), "font") {
            let shared = style(font, attribute(layer.element(), "spacing"), fonts, label)?;
            styles.fill(shared);
        }
        for content in layer.element().content() {
            let XmlContent::Element(index) = content else {
                continue;
            };
            let Some(child) = layer.document().element(*index) else {
                continue;
            };
            let slot = if child.name().eq_ignore_ascii_case("FontString") {
                Some(UiSimpleHtmlFontSlot::Normal)
            } else if child.name().eq_ignore_ascii_case("FontStringHeader1") {
                Some(UiSimpleHtmlFontSlot::Header1)
            } else if child.name().eq_ignore_ascii_case("FontStringHeader2") {
                Some(UiSimpleHtmlFontSlot::Header2)
            } else if child.name().eq_ignore_ascii_case("FontStringHeader3") {
                Some(UiSimpleHtmlFontSlot::Header3)
            } else {
                None
            };
            let Some(slot) = slot else {
                continue;
            };
            let inherited = attribute(child, "inherits").and_then(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .rfind(|name| !name.is_empty())
            });
            let explicit = attribute(child, "font").filter(|name| !name.is_empty());
            if let Some(name) = explicit.or(inherited) {
                styles[slot.index()] = style(name, attribute(child, "spacing"), fonts, label)?;
            }
        }
    }
    if styles[0].is_none() {
        return Err(declaration(
            label,
            "normal FontString slot has no stock font",
        ));
    }
    Ok(styles)
}

fn style(
    name: &str,
    spacing: Option<&str>,
    fonts: &FontCatalog,
    label: &str,
) -> Result<Option<HtmlFontStyle>, UiSimpleHtmlError> {
    let definition = fonts
        .definition(name)
        .ok_or_else(|| declaration(label, format!("font object {name} is unavailable")))?;
    let spacing = match spacing {
        Some(value) => value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| declaration(label, format!("invalid font spacing {value}")))?,
        None => f64::from(definition.spacing().unwrap_or(0.0)),
    };
    Ok(Some(HtmlFontStyle {
        object: name.to_owned(),
        spacing,
    }))
}

fn parse_document(
    source: LocalizedDocument,
    bytes: &[u8],
) -> Result<Vec<UiSimpleHtmlBlock>, UiSimpleHtmlError> {
    let path = AssetPath::new(source.file_name())?;
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|error| UiLoadError::TextEncoding {
        path: path.clone(),
        message: error.to_string(),
    })?;
    let document = XmlDocument::parse_preserving_text_whitespace(&path, text)?;
    if !document.root().name().eq_ignore_ascii_case("HTML") {
        return Err(UiLoadError::Xml {
            path,
            message: "restricted HTML document root is not HTML".to_owned(),
        }
        .into());
    }
    let body = child(&document, document.root(), "BODY").ok_or_else(|| UiLoadError::Xml {
        path: path.clone(),
        message: "restricted HTML document has no BODY".to_owned(),
    })?;
    let mut blocks = Vec::new();
    for content in body.content() {
        let XmlContent::Element(index) = content else {
            continue;
        };
        let Some(element) = document.element(*index) else {
            continue;
        };
        let font = if element.name().eq_ignore_ascii_case("P") {
            Some(UiSimpleHtmlFontSlot::Normal)
        } else if element.name().eq_ignore_ascii_case("H1") {
            Some(UiSimpleHtmlFontSlot::Header1)
        } else if element.name().eq_ignore_ascii_case("H2") {
            Some(UiSimpleHtmlFontSlot::Header2)
        } else if element.name().eq_ignore_ascii_case("H3") {
            Some(UiSimpleHtmlFontSlot::Header3)
        } else {
            None
        };
        if element.name().eq_ignore_ascii_case("BR") {
            blocks.push(UiSimpleHtmlBlock {
                font: UiSimpleHtmlFontSlot::Normal,
                alignment: UiSimpleHtmlAlignment::Left,
                text: "|n".to_owned(),
            });
        } else if let Some(font) = font {
            blocks.push(UiSimpleHtmlBlock {
                font,
                alignment: alignment(attribute(element, "align")),
                text: paragraph_text(&document, element),
            });
        }
    }
    Ok(blocks)
}

fn paragraph_text(document: &XmlDocument, element: &XmlElement) -> String {
    let mut raw = String::new();
    for content in element.content() {
        match content {
            XmlContent::Text(text) => raw.push_str(text),
            XmlContent::Element(index) => {
                if document
                    .element(*index)
                    .is_some_and(|child| child.name().eq_ignore_ascii_case("BR"))
                {
                    raw.push_str("|n");
                }
            }
        }
    }
    collapse_whitespace(&raw)
}

fn collapse_whitespace(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut whitespace = false;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '|' && chars.peek() == Some(&'n') {
            chars.next();
            while output.ends_with(' ') {
                output.pop();
            }
            output.push_str("|n");
            whitespace = false;
        } else if character.is_whitespace() {
            whitespace = !output.is_empty() && !output.ends_with("|n");
        } else {
            if whitespace {
                output.push(' ');
            }
            output.push(character);
            whitespace = false;
        }
    }
    while output.ends_with(' ') {
        output.pop();
    }
    output
}

#[allow(clippy::too_many_arguments)]
fn wrap_blocks(
    blocks: &[UiSimpleHtmlBlock],
    styles: &[Option<HtmlFontStyle>; 4],
    fonts: &FontCatalog,
    width: f64,
    pixels_per_ui_unit: f64,
    assets: &mut AssetStore,
    font_system: &mut FontSystem,
    label: &str,
) -> Result<(Vec<UiSimpleHtmlLine>, f64), UiSimpleHtmlError> {
    if !width.is_finite() || width <= 0.0 {
        return Err(declaration(label, "content width must be positive"));
    }
    let mut lines = Vec::new();
    let mut cursor = 0.0_f64;
    for (block_index, block) in blocks.iter().enumerate() {
        let style = styles[block.font.index()]
            .as_ref()
            .or(styles[0].as_ref())
            .ok_or_else(|| declaration(label, "normal font fallback is unavailable"))?;
        let definition = fonts.definition(&style.object).ok_or_else(|| {
            declaration(
                label,
                format!("font object {} is unavailable", style.object),
            )
        })?;
        let face = definition
            .face()
            .ok_or_else(|| declaration(label, format!("font {} has no face", style.object)))?;
        let height =
            f64::from(definition.height().ok_or_else(|| {
                declaration(label, format!("font {} has no height", style.object))
            })?);
        let pixel_height = (height * pixels_per_ui_unit).round().max(1.0) as u32;
        let rasterization = if definition.monochrome().unwrap_or(false) {
            FontRasterization::Monochrome
        } else {
            FontRasterization::Antialiased
        };
        let wrapped = wrap_text(
            &block.text,
            width,
            style.spacing,
            pixels_per_ui_unit,
            assets,
            font_system,
            face,
            pixel_height,
            rasterization,
        )?;
        let wrapped_count = wrapped.len();
        for (line_index, (text, measured_width)) in wrapped.into_iter().enumerate() {
            lines.push(UiSimpleHtmlLine {
                font: block.font,
                font_object: style.object.clone(),
                alignment: block.alignment,
                text,
                width: measured_width as f32,
                top: cursor as f32,
                height: height as f32,
            });
            cursor += height;
            if line_index + 1 < wrapped_count {
                cursor += style.spacing;
            }
        }
        if block_index + 1 < blocks.len() {
            cursor += style.spacing;
        }
    }
    Ok((lines, cursor.max(0.0)))
}

#[allow(clippy::too_many_arguments)]
fn wrap_text(
    text: &str,
    maximum_width: f64,
    spacing: f64,
    pixels_per_ui_unit: f64,
    assets: &mut AssetStore,
    system: &mut FontSystem,
    face: &AssetPath,
    pixel_height: u32,
    rasterization: FontRasterization,
) -> Result<Vec<(String, f64)>, UiSimpleHtmlError> {
    let mut output = Vec::new();
    for explicit in text.split("|n") {
        if explicit.is_empty() {
            output.push((String::new(), 0.0));
            continue;
        }
        let mut line = String::new();
        for word in explicit.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            let candidate_width = measure(
                &candidate,
                spacing,
                pixels_per_ui_unit,
                assets,
                system,
                face,
                pixel_height,
                rasterization,
            )?;
            if !line.is_empty() && candidate_width > maximum_width {
                let width = measure(
                    &line,
                    spacing,
                    pixels_per_ui_unit,
                    assets,
                    system,
                    face,
                    pixel_height,
                    rasterization,
                )?;
                output.push((std::mem::take(&mut line), width));
                line.push_str(word);
            } else {
                line = candidate;
            }
        }
        let width = measure(
            &line,
            spacing,
            pixels_per_ui_unit,
            assets,
            system,
            face,
            pixel_height,
            rasterization,
        )?;
        output.push((line, width));
    }
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn measure(
    text: &str,
    spacing: f64,
    pixels_per_ui_unit: f64,
    assets: &mut AssetStore,
    system: &mut FontSystem,
    face: &AssetPath,
    pixel_height: u32,
    rasterization: FontRasterization,
) -> Result<f64, UiSimpleHtmlError> {
    let width = system.measure_line_width_26_6(assets, face, pixel_height, text, rasterization)?
        as f64
        / 64.0
        / pixels_per_ui_unit;
    Ok(width + text.chars().count().saturating_sub(1) as f64 * spacing)
}

fn child<'a>(
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
            .filter(|child| child.name().eq_ignore_ascii_case(name))
    })
}

fn alignment(value: Option<&str>) -> UiSimpleHtmlAlignment {
    match value {
        Some(value) if value.eq_ignore_ascii_case("CENTER") => UiSimpleHtmlAlignment::Center,
        Some(value) if value.eq_ignore_ascii_case("RIGHT") => UiSimpleHtmlAlignment::Right,
        _ => UiSimpleHtmlAlignment::Left,
    }
}

fn attribute<'a>(element: &'a XmlElement, name: &str) -> Option<&'a str> {
    element
        .attributes()
        .iter()
        .find(|attribute| attribute.name().eq_ignore_ascii_case(name))
        .map(|attribute| attribute.value())
}

fn declaration(object: &str, message: impl Into<String>) -> UiSimpleHtmlError {
    UiSimpleHtmlError::Declaration {
        object: object.to_owned(),
        message: message.into(),
    }
}
