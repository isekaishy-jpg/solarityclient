//! Ordered stock UI manifest and resource loading.

use mlua::Lua;
use solarity_asset::{AssetPath, AssetStore};

use crate::xml::{UiLoadError, XmlContent, XmlDocument, XmlElement};

/// The built-in UI manifest selected for a client state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiManifestKind {
    /// Login, realm, and character-selection user interface.
    Glue,
    /// In-world user interface.
    Frame,
}

impl UiManifestKind {
    /// Returns the exact build-12340 archive path.
    #[must_use]
    pub const fn path(self) -> &'static str {
        match self {
            Self::Glue => "Interface\\GlueXML\\GlueXML.toc",
            Self::Frame => "Interface\\FrameXML\\FrameXML.toc",
        }
    }
}

/// The source language named by a UI manifest entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiManifestEntryKind {
    /// XML layout, templates, and inline handlers.
    Xml,
    /// Lua 5.1 source.
    Lua,
}

/// One source file in stock manifest order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiManifestEntry {
    path: AssetPath,
    kind: UiManifestEntryKind,
}

impl UiManifestEntry {
    /// Returns the resolved archive path.
    #[must_use]
    pub fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the source language.
    #[must_use]
    pub const fn kind(&self) -> UiManifestEntryKind {
        self.kind
    }
}

/// A parsed built-in TOC preserving source order exactly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiManifest {
    kind: UiManifestKind,
    path: AssetPath,
    entries: Vec<UiManifestEntry>,
}

impl UiManifest {
    /// Loads and parses one built-in UI manifest through the stock archive stack.
    ///
    /// # Errors
    ///
    /// Returns an asset, text-decoding, or manifest-entry error. Entries are
    /// resolved only relative to the manifest directory; no loose-file or path
    /// search fallback is used.
    pub fn load(store: &mut AssetStore, kind: UiManifestKind) -> Result<Self, UiLoadError> {
        let path = AssetPath::new(kind.path())?;
        let bytes = store.read(&path)?.into_bytes();
        let source = decode_text(&path, bytes)?;
        Self::parse(kind, path, &source)
    }

    /// Returns the selected built-in manifest role.
    #[must_use]
    pub const fn kind(&self) -> UiManifestKind {
        self.kind
    }

    /// Returns the normalized manifest archive path.
    #[must_use]
    pub fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns entries in their exact source order.
    #[must_use]
    pub fn entries(&self) -> &[UiManifestEntry] {
        &self.entries
    }

    fn parse(kind: UiManifestKind, path: AssetPath, source: &str) -> Result<Self, UiLoadError> {
        let directory = path
            .as_str()
            .rsplit_once('\\')
            .map_or("", |(directory, _)| directory);
        let mut entries = Vec::new();

        for (index, line) in source.lines().enumerate() {
            let value = line.trim();
            if value.is_empty() || value.starts_with('#') {
                continue;
            }

            let entry_kind = if value.ends_with_ignore_ascii_case(".xml") {
                UiManifestEntryKind::Xml
            } else if value.ends_with_ignore_ascii_case(".lua") {
                UiManifestEntryKind::Lua
            } else {
                return Err(manifest_entry_error(&path, index + 1, value));
            };
            let candidate = format!("{directory}\\{value}");
            let entry_path = AssetPath::new(&candidate)
                .map_err(|_| manifest_entry_error(&path, index + 1, value))?;
            entries.push(UiManifestEntry {
                path: entry_path,
                kind: entry_kind,
            });
        }

        Ok(Self {
            kind,
            path,
            entries,
        })
    }
}

/// An owned Lua 5.1 source file awaiting stock API registration and execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LuaSource {
    source: String,
}

impl LuaSource {
    /// Returns the source text exactly as decoded from the archive.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.source
    }
}

/// Parsed content for one ordered manifest entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiResourceContent {
    /// An owned XML document.
    Xml(XmlDocument),
    /// Lua 5.1 source compiled for validation but not yet executed.
    Lua(LuaSource),
}

/// One loaded UI source and its exact archive identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiResource {
    path: AssetPath,
    content: UiResourceContent,
}

impl UiResource {
    /// Returns the normalized archive path.
    #[must_use]
    pub fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the parsed or compiled-language content.
    #[must_use]
    pub const fn content(&self) -> &UiResourceContent {
        &self.content
    }
}

/// One operation in the fully expanded stock UI load order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiLoadAction {
    /// Construct or register one root XML object.
    XmlElement {
        /// Index into [`UiBundle::resources`].
        resource_index: usize,
        /// Index into the resource's [`XmlDocument`] arena.
        element_index: usize,
    },
    /// Execute one manifest- or XML-referenced Lua file after API registration.
    LuaResource {
        /// Index into [`UiBundle::resources`].
        resource_index: usize,
    },
    /// Execute a root-level inline `<Script>` after API registration.
    InlineLua {
        /// XML source owning the script.
        path: AssetPath,
        /// Lua 5.1 source text.
        source: String,
    },
}

/// A completely resolved built-in UI manifest and its ordered resources.
pub struct UiBundle {
    manifest: UiManifest,
    resources: Vec<UiResource>,
    actions: Vec<UiLoadAction>,
    lua: Lua,
}

impl UiBundle {
    /// Loads every source in one built-in manifest through the archive stack.
    ///
    /// Lua chunks are compiled as Lua 5.1 but deliberately not executed. Stock
    /// globals must be registered first; inventing no-op globals would hide API
    /// incompatibilities and violate the no-fallback policy.
    ///
    /// # Errors
    ///
    /// Returns the first manifest, asset, XML, or Lua compilation failure in
    /// stock load order.
    pub fn load(store: &mut AssetStore, kind: UiManifestKind) -> Result<Self, UiLoadError> {
        let manifest = UiManifest::load(store, kind)?;
        let mut loader = UiBundleLoader::new(store, manifest.entries.len());
        for entry in &manifest.entries {
            loader.load_entry(&entry.path, entry.kind)?;
        }
        let (resources, actions, lua) = loader.finish();
        Ok(Self {
            manifest,
            resources,
            actions,
            lua,
        })
    }

    /// Returns the parsed manifest.
    #[must_use]
    pub const fn manifest(&self) -> &UiManifest {
        &self.manifest
    }

    /// Returns resources in exact manifest order.
    #[must_use]
    pub fn resources(&self) -> &[UiResource] {
        &self.resources
    }

    /// Returns the expanded XML and Lua operations in exact load order.
    #[must_use]
    pub fn actions(&self) -> &[UiLoadAction] {
        &self.actions
    }

    /// Returns one loaded resource by action index.
    #[must_use]
    pub fn resource(&self, index: usize) -> Option<&UiResource> {
        self.resources.get(index)
    }

    /// Returns the owned Lua state reserved for stock API registration.
    #[must_use]
    pub const fn lua(&self) -> &Lua {
        &self.lua
    }
}

struct UiBundleLoader<'a> {
    store: &'a mut AssetStore,
    resources: Vec<UiResource>,
    actions: Vec<UiLoadAction>,
    include_stack: Vec<AssetPath>,
    lua: Lua,
}

impl<'a> UiBundleLoader<'a> {
    fn new(store: &'a mut AssetStore, manifest_entries: usize) -> Self {
        Self {
            store,
            resources: Vec::with_capacity(manifest_entries),
            actions: Vec::with_capacity(manifest_entries),
            include_stack: Vec::new(),
            lua: Lua::new(),
        }
    }

    fn load_entry(
        &mut self,
        path: &AssetPath,
        kind: UiManifestEntryKind,
    ) -> Result<(), UiLoadError> {
        match kind {
            UiManifestEntryKind::Xml => self.load_xml(path),
            UiManifestEntryKind::Lua => self.load_lua(path),
        }
    }

    fn load_lua(&mut self, path: &AssetPath) -> Result<(), UiLoadError> {
        let bytes = self.store.read(path)?.into_bytes();
        let source = decode_text(path, bytes)?;
        compile_lua(&self.lua, path, path.as_str(), &source)?;
        let resource_index = self.resources.len();
        self.resources.push(UiResource {
            path: path.clone(),
            content: UiResourceContent::Lua(LuaSource { source }),
        });
        self.actions
            .push(UiLoadAction::LuaResource { resource_index });
        Ok(())
    }

    fn load_xml(&mut self, path: &AssetPath) -> Result<(), UiLoadError> {
        if self.include_stack.contains(path) {
            return Err(UiLoadError::Directive {
                path: path.clone(),
                message: "recursive XML include".to_owned(),
            });
        }

        let bytes = self.store.read(path)?.into_bytes();
        let source = decode_text(path, bytes)?;
        let document = XmlDocument::parse(path, &source)?;
        let directives = analyze_root(path, &document)?;
        let resource_index = self.resources.len();
        self.resources.push(UiResource {
            path: path.clone(),
            content: UiResourceContent::Xml(document),
        });

        self.include_stack.push(path.clone());
        let result = self.apply_directives(path, resource_index, directives);
        let removed = self.include_stack.pop();
        debug_assert_eq!(removed.as_ref(), Some(path));
        result
    }

    fn apply_directives(
        &mut self,
        owner: &AssetPath,
        resource_index: usize,
        directives: Vec<RootDirective>,
    ) -> Result<(), UiLoadError> {
        for directive in directives {
            match directive {
                RootDirective::Element {
                    element_index,
                    handlers,
                } => {
                    for (label, source) in handlers {
                        compile_lua(&self.lua, owner, &label, &source)?;
                    }
                    self.actions.push(UiLoadAction::XmlElement {
                        resource_index,
                        element_index,
                    });
                }
                RootDirective::Include(file) => {
                    let path = resolve_directive_path(owner, &file, ".xml")?;
                    self.load_xml(&path)?;
                }
                RootDirective::Script { file, source } => {
                    if let Some(file) = file {
                        let path = resolve_directive_path(owner, &file, ".lua")?;
                        self.load_lua(&path)?;
                    }
                    if let Some(source) = source {
                        compile_lua(&self.lua, owner, owner.as_str(), &source)?;
                        self.actions.push(UiLoadAction::InlineLua {
                            path: owner.clone(),
                            source,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    fn finish(self) -> (Vec<UiResource>, Vec<UiLoadAction>, Lua) {
        (self.resources, self.actions, self.lua)
    }
}

enum RootDirective {
    Element {
        element_index: usize,
        handlers: Vec<(String, String)>,
    },
    Include(String),
    Script {
        file: Option<String>,
        source: Option<String>,
    },
}

fn analyze_root(
    path: &AssetPath,
    document: &XmlDocument,
) -> Result<Vec<RootDirective>, UiLoadError> {
    let mut directives = Vec::new();
    for content in document.root().content() {
        let XmlContent::Element(element_index) = content else {
            continue;
        };
        let element = document
            .element(*element_index)
            .ok_or_else(|| directive_error(path, "XML root child index is outside the arena"))?;
        match element.name() {
            "Include" => {
                let file = attribute(element, "file")
                    .ok_or_else(|| directive_error(path, "Include has no file attribute"))?;
                directives.push(RootDirective::Include(file.to_owned()));
            }
            "Script" => {
                let file = attribute(element, "file").map(str::to_owned);
                let source = direct_text(element).filter(|source| !source.trim().is_empty());
                if file.is_none() && source.is_none() {
                    return Err(directive_error(path, "Script has neither file nor source"));
                }
                directives.push(RootDirective::Script { file, source });
            }
            _ => {
                let mut handlers = Vec::new();
                collect_handlers(path, document, *element_index, &mut handlers)?;
                directives.push(RootDirective::Element {
                    element_index: *element_index,
                    handlers,
                });
            }
        }
    }
    Ok(directives)
}

fn collect_handlers(
    path: &AssetPath,
    document: &XmlDocument,
    element_index: usize,
    handlers: &mut Vec<(String, String)>,
) -> Result<(), UiLoadError> {
    let element = document
        .element(element_index)
        .ok_or_else(|| directive_error(path, "XML child index is outside the arena"))?;
    if element.name().starts_with("On") {
        let source = direct_text(element).unwrap_or_default();
        if !source.trim().is_empty() {
            handlers.push((format!("{}:<{}>", path.as_str(), element.name()), source));
        }
    }
    for content in element.content() {
        if let XmlContent::Element(child) = content {
            collect_handlers(path, document, *child, handlers)?;
        }
    }
    Ok(())
}

fn direct_text(element: &XmlElement) -> Option<String> {
    let mut source = String::new();
    for content in element.content() {
        if let XmlContent::Text(text) = content {
            source.push_str(text);
        }
    }
    (!source.is_empty()).then_some(source)
}

fn attribute<'a>(element: &'a XmlElement, name: &str) -> Option<&'a str> {
    element
        .attributes()
        .iter()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn resolve_directive_path(
    owner: &AssetPath,
    file: &str,
    extension: &str,
) -> Result<AssetPath, UiLoadError> {
    if !file.ends_with_ignore_ascii_case(extension) {
        return Err(directive_error(
            owner,
            format!("directive path {file} must end in {extension}"),
        ));
    }
    let directory = owner
        .as_str()
        .rsplit_once('\\')
        .map_or("", |(directory, _)| directory);
    let mut components = directory.split('\\').collect::<Vec<_>>();
    for component in file.split(['\\', '/']) {
        match component {
            "" | "." => {
                return Err(directive_error(
                    owner,
                    format!("directive path {file} contains an empty or current component"),
                ));
            }
            ".." => {
                if components.pop().is_none() {
                    return Err(directive_error(
                        owner,
                        format!("directive path {file} escapes the archive root"),
                    ));
                }
            }
            value => components.push(value),
        }
    }
    AssetPath::new(components.join("\\")).map_err(|error| directive_error(owner, error.to_string()))
}

fn compile_lua(lua: &Lua, path: &AssetPath, label: &str, source: &str) -> Result<(), UiLoadError> {
    lua.load(source)
        .set_name(label)
        .into_function()
        .map_err(|error| UiLoadError::Lua {
            path: path.clone(),
            message: error.to_string(),
        })?;
    Ok(())
}

fn directive_error(path: &AssetPath, message: impl Into<String>) -> UiLoadError {
    UiLoadError::Directive {
        path: path.clone(),
        message: message.into(),
    }
}

fn decode_text(path: &AssetPath, bytes: Vec<u8>) -> Result<String, UiLoadError> {
    String::from_utf8(bytes).map_err(|error| UiLoadError::TextEncoding {
        path: path.clone(),
        message: error.to_string(),
    })
}

fn manifest_entry_error(path: &AssetPath, line: usize, value: &str) -> UiLoadError {
    UiLoadError::ManifestEntry {
        path: path.clone(),
        line,
        value: value.to_owned(),
    }
}

trait EndsWithIgnoreAsciiCase {
    fn ends_with_ignore_ascii_case(&self, suffix: &str) -> bool;
}

impl EndsWithIgnoreAsciiCase for str {
    fn ends_with_ignore_ascii_case(&self, suffix: &str) -> bool {
        self.get(self.len().saturating_sub(suffix.len())..)
            .is_some_and(|ending| ending.eq_ignore_ascii_case(suffix))
    }
}
