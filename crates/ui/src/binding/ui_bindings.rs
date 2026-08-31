//! Ordered loading and strict decoding of stock `Bindings.xml` documents.

use std::collections::HashSet;

use mlua::Lua;
use solarity_asset::{AssetPath, AssetStore};

use crate::addon::AddonDefinition;
use crate::binding::{
    UiBindingDefinition, UiBindingDocument, UiBindingError, UiBindingPlatform,
    UiModifiedClickChord, UiModifiedClickDefinition,
};
use crate::xml::{UiLoadError, XmlContent, XmlDocument, XmlElement};

/// Archive path of the build-12340 built-in binding vocabulary.
const BUILTIN_BINDINGS_PATH: &str = "Interface\\FrameXML\\Bindings.xml";

/// Ordered built-in and AddOn binding documents.
///
/// `UIBindings.cpp` treats binding documents as a special UI input rather than
/// as ordinary `FrameXML.toc` entries. The catalog preserves document and
/// declaration order. Stock diagnoses duplicate binding names, group headers,
/// and modified-click actions against the current source, so the catalog
/// rejects them across every appended document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiBindingCatalog {
    documents: Vec<UiBindingDocument>,
    binding_names: HashSet<String>,
    binding_headers: HashSet<String>,
    modified_click_actions: HashSet<String>,
}

impl UiBindingCatalog {
    /// Loads and validates the built-in FrameXML binding document.
    ///
    /// # Errors
    ///
    /// Returns an asset, text, XML, Lua compilation, or binding-schema error.
    pub fn load_builtin(store: &mut AssetStore) -> Result<Self, UiBindingError> {
        let path = AssetPath::new(BUILTIN_BINDINGS_PATH)?;
        let document = load_document(store, path, DocumentSource::Archive)?;
        let mut catalog = Self {
            documents: Vec::new(),
            binding_names: HashSet::new(),
            binding_headers: HashSet::new(),
            modified_click_actions: HashSet::new(),
        };
        catalog.append_document(document)?;
        Ok(catalog)
    }

    /// Appends one enabled AddOn's special binding document when it exists.
    ///
    /// AddOn load order is owned by the caller because dependency sorting and
    /// per-account enablement precede binding registration in stock. The
    /// explicit AddOn file stack retains loose-first, MPQ-second resolution.
    ///
    /// Returns whether the AddOn supplied a `Bindings.xml` document.
    ///
    /// # Errors
    ///
    /// Returns an asset, text, XML, Lua compilation, or binding-schema error.
    pub fn append_addon(
        &mut self,
        store: &mut AssetStore,
        addon: &AddonDefinition,
    ) -> Result<bool, UiBindingError> {
        let path = AssetPath::new(format!("Interface\\AddOns\\{}\\Bindings.xml", addon.name()))?;
        if !store.contains_addon_file(&path)? {
            return Ok(false);
        }
        let document = load_document(store, path, DocumentSource::Addon)?;
        self.append_document(document)?;
        Ok(true)
    }

    /// Returns documents in built-in then caller-supplied AddOn load order.
    #[must_use]
    pub fn documents(&self) -> &[UiBindingDocument] {
        &self.documents
    }

    /// Iterates every command declaration without allocating a flattened list.
    pub fn bindings(&self) -> impl ExactSizeIterator<Item = &UiBindingDefinition> {
        let length = self
            .documents
            .iter()
            .map(|document| document.bindings().len())
            .sum();
        BindingIter {
            documents: self.documents.iter(),
            current: None,
            length,
        }
    }

    /// Iterates every modified-click declaration in source order.
    pub fn modified_clicks(&self) -> impl Iterator<Item = &UiModifiedClickDefinition> {
        self.documents
            .iter()
            .flat_map(UiBindingDocument::modified_clicks)
    }

    /// Validates global declaration identity before committing one document.
    fn append_document(&mut self, document: UiBindingDocument) -> Result<(), UiBindingError> {
        let mut local_names = HashSet::with_capacity(document.bindings().len());
        let mut local_headers = HashSet::new();
        let mut local_actions = HashSet::with_capacity(document.modified_clicks().len());
        for binding in document.bindings() {
            if self.binding_names.contains(binding.name()) || !local_names.insert(binding.name()) {
                return Err(schema_error(
                    document.path(),
                    format!("Binding {} is defined more than once", binding.name()),
                ));
            }
            if let Some(header) = binding.header()
                && (self.binding_headers.contains(header) || !local_headers.insert(header))
            {
                return Err(schema_error(
                    document.path(),
                    format!("Binding header {header} is defined more than once"),
                ));
            }
        }
        for modified_click in document.modified_clicks() {
            if self
                .modified_click_actions
                .contains(modified_click.action())
                || !local_actions.insert(modified_click.action())
            {
                return Err(schema_error(
                    document.path(),
                    format!(
                        "Modified click {} is defined more than once",
                        modified_click.action()
                    ),
                ));
            }
        }

        self.binding_names
            .extend(local_names.into_iter().map(str::to_owned));
        self.binding_headers
            .extend(local_headers.into_iter().map(str::to_owned));
        self.modified_click_actions
            .extend(local_actions.into_iter().map(str::to_owned));
        self.documents.push(document);
        Ok(())
    }
}

/// Selects the one permitted storage boundary for a binding document.
#[derive(Clone, Copy)]
enum DocumentSource {
    Archive,
    Addon,
}

/// Reads one document through its stock source and validates its full shape.
fn load_document(
    store: &mut AssetStore,
    path: AssetPath,
    source: DocumentSource,
) -> Result<UiBindingDocument, UiBindingError> {
    let bytes = match source {
        DocumentSource::Archive => store.read(&path)?.into_bytes(),
        DocumentSource::Addon => store.read_addon_file(&path)?,
    };
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    let source = std::str::from_utf8(bytes).map_err(|error| UiLoadError::TextEncoding {
        path: path.clone(),
        message: error.to_string(),
    })?;
    parse_document(path, source)
}

/// Decodes the exact build-12340 binding vocabulary and compiles every body.
fn parse_document(path: AssetPath, source: &str) -> Result<UiBindingDocument, UiBindingError> {
    let document = XmlDocument::parse(&path, source)?;
    let root = document.root();
    if root.name() != "Bindings" {
        return Err(schema_error(
            &path,
            format!("expected Bindings root, found {}", root.name()),
        ));
    }
    if !root.attributes().is_empty() {
        return Err(schema_error(&path, "Bindings root has attributes"));
    }

    let mut bindings = Vec::new();
    let mut modified_clicks = Vec::new();
    let lua = Lua::new();
    for content in root.content() {
        let XmlContent::Element(index) = content else {
            return Err(schema_error(&path, "Bindings root has character data"));
        };
        let element = document
            .element(*index)
            .ok_or_else(|| schema_error(&path, "binding child index is outside the XML arena"))?;
        match element.name() {
            "Binding" => bindings.push(parse_binding(&path, &lua, element)?),
            "ModifiedClick" => modified_clicks.push(parse_modified_click(&path, element)?),
            name => {
                return Err(schema_error(
                    &path,
                    format!("unsupported Bindings child {name}"),
                ));
            }
        }
    }
    Ok(UiBindingDocument::new(path, bindings, modified_clicks))
}

/// Validates one named binding and its executable Lua 5.1 body.
fn parse_binding(
    path: &AssetPath,
    lua: &Lua,
    element: &XmlElement,
) -> Result<UiBindingDefinition, UiBindingError> {
    let mut name = None;
    let mut header = None;
    let mut run_on_up = None;
    let mut hidden = None;
    let mut debug = None;
    let mut platform = None;
    for attribute in element.attributes() {
        let slot = match attribute.name() {
            "name" => &mut name,
            "header" => &mut header,
            "runOnUp" => &mut run_on_up,
            "hidden" => &mut hidden,
            "debug" => &mut debug,
            "platform" => &mut platform,
            unknown => {
                return Err(schema_error(
                    path,
                    format!("Binding has unsupported attribute {unknown}"),
                ));
            }
        };
        if slot.replace(attribute.value()).is_some() {
            return Err(schema_error(
                path,
                format!("Binding repeats attribute {}", attribute.name()),
            ));
        }
    }

    let name = required_token(path, "Binding", "name", name)?;
    let header = optional_token(path, "Binding", "header", header)?;
    let run_on_up = stock_bool(path, "Binding", "runOnUp", run_on_up)?;
    let hidden = stock_bool(path, "Binding", "hidden", hidden)?;
    let debug = stock_bool(path, "Binding", "debug", debug)?;
    let platform = match platform {
        None => None,
        Some("windows") => Some(UiBindingPlatform::Windows),
        Some("mac") => Some(UiBindingPlatform::Mac),
        Some(value) => {
            return Err(schema_error(
                path,
                format!("Binding platform has unsupported value {value:?}"),
            ));
        }
    };
    let body = direct_text(path, "Binding", element)?;
    if body.trim().is_empty() {
        return Err(schema_error(
            path,
            format!("Binding {name} has no Lua body"),
        ));
    }
    lua.load(&body)
        .set_name(format!("{}:<Binding {name}>", path.as_str()))
        .into_function()
        .map_err(|error| UiLoadError::Lua {
            path: path.clone(),
            message: error.to_string(),
        })?;

    Ok(UiBindingDefinition::new(
        name.to_owned(),
        header.map(str::to_owned),
        body,
        run_on_up,
        hidden,
        debug,
        platform,
    ))
}

/// Validates one special modified-click action and its stock default token.
fn parse_modified_click(
    path: &AssetPath,
    element: &XmlElement,
) -> Result<UiModifiedClickDefinition, UiBindingError> {
    let mut action = None;
    let mut default = None;
    for attribute in element.attributes() {
        let slot = match attribute.name() {
            "action" => &mut action,
            "default" => &mut default,
            unknown => {
                return Err(schema_error(
                    path,
                    format!("ModifiedClick has unsupported attribute {unknown}"),
                ));
            }
        };
        if slot.replace(attribute.value()).is_some() {
            return Err(schema_error(
                path,
                format!("ModifiedClick repeats attribute {}", attribute.name()),
            ));
        }
    }
    if !element.content().is_empty() {
        return Err(schema_error(path, "ModifiedClick has child content"));
    }
    let action = required_token(path, "ModifiedClick", "action", action)?;
    let default = required_token(path, "ModifiedClick", "default", default)?;
    let default = UiModifiedClickChord::parse(default)
        .map_err(|message| schema_error(path, format!("ModifiedClick default {message}")))?;
    Ok(UiModifiedClickDefinition::new(action.to_owned(), default))
}

/// Concatenates direct character data and rejects nested binding markup.
fn direct_text(
    path: &AssetPath,
    element_name: &str,
    element: &XmlElement,
) -> Result<String, UiBindingError> {
    let mut source = String::new();
    for content in element.content() {
        match content {
            XmlContent::Text(text) => source.push_str(text),
            XmlContent::Element(_) => {
                return Err(schema_error(
                    path,
                    format!("{element_name} has nested elements"),
                ));
            }
        }
    }
    Ok(source)
}

/// Requires a nonempty declaration token without rewriting authored identity.
fn required_token<'a>(
    path: &AssetPath,
    element: &str,
    attribute: &str,
    value: Option<&'a str>,
) -> Result<&'a str, UiBindingError> {
    let value = value.ok_or_else(|| {
        schema_error(
            path,
            format!("{element} has no required {attribute} attribute"),
        )
    })?;
    if value.trim().is_empty() {
        return Err(schema_error(
            path,
            format!("{element} has empty {attribute} attribute"),
        ));
    }
    Ok(value)
}

/// Validates an optional nonempty declaration token.
fn optional_token<'a>(
    path: &AssetPath,
    element: &str,
    attribute: &str,
    value: Option<&'a str>,
) -> Result<Option<&'a str>, UiBindingError> {
    value
        .map(|value| required_token(path, element, attribute, Some(value)))
        .transpose()
}

/// Decodes the lowercase XML booleans used by build-12340 binding documents.
fn stock_bool(
    path: &AssetPath,
    element: &str,
    attribute: &str,
    value: Option<&str>,
) -> Result<bool, UiBindingError> {
    match value {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        Some(value) => Err(schema_error(
            path,
            format!("{element} {attribute} has invalid value {value:?}"),
        )),
    }
}

/// Adds stable source-path context to a binding-shape failure.
fn schema_error(path: &AssetPath, message: impl Into<String>) -> UiBindingError {
    UiBindingError::Schema {
        path: path.clone(),
        message: message.into(),
    }
}

/// Nonallocating flattened iterator with an exact remaining length.
struct BindingIter<'a> {
    documents: std::slice::Iter<'a, UiBindingDocument>,
    current: Option<std::slice::Iter<'a, UiBindingDefinition>>,
    length: usize,
}

impl<'a> Iterator for BindingIter<'a> {
    type Item = &'a UiBindingDefinition;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(binding) = self.current.as_mut().and_then(Iterator::next) {
                self.length -= 1;
                return Some(binding);
            }
            self.current = Some(self.documents.next()?.bindings().iter());
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.length, Some(self.length))
    }
}

impl ExactSizeIterator for BindingIter<'_> {
    fn len(&self) -> usize {
        self.length
    }
}

impl std::iter::FusedIterator for BindingIter<'_> {}
