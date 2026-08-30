//! Ordered stock UI manifest and resource loading.

use mlua::Lua;
use solarity_asset::{AssetPath, AssetStore};

use crate::xml::{UiLoadError, XmlDocument};

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

/// A completely resolved built-in UI manifest and its ordered resources.
pub struct UiBundle {
    manifest: UiManifest,
    resources: Vec<UiResource>,
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
        let lua = Lua::new();
        let mut resources = Vec::with_capacity(manifest.entries.len());

        for entry in &manifest.entries {
            let bytes = store.read(&entry.path)?.into_bytes();
            let source = decode_text(&entry.path, bytes)?;
            let content = match entry.kind {
                UiManifestEntryKind::Xml => {
                    UiResourceContent::Xml(XmlDocument::parse(&entry.path, &source)?)
                }
                UiManifestEntryKind::Lua => {
                    lua.load(&source)
                        .set_name(entry.path.as_str())
                        .into_function()
                        .map_err(|error| UiLoadError::Lua {
                            path: entry.path.clone(),
                            message: error.to_string(),
                        })?;
                    UiResourceContent::Lua(LuaSource { source })
                }
            };
            resources.push(UiResource {
                path: entry.path.clone(),
                content,
            });
        }

        Ok(Self {
            manifest,
            resources,
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

    /// Returns the owned Lua state reserved for stock API registration.
    #[must_use]
    pub const fn lua(&self) -> &Lua {
        &self.lua
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
