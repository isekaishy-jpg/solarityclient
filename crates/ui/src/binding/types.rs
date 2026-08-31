//! Immutable declarations retained from one stock binding document.

use solarity_asset::AssetPath;

use crate::binding::UiModifiedClickChord;

/// Client platform selected by a binding declaration's exact gate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiBindingPlatform {
    /// The Win32 client token embedded by build 12340.
    Windows,
    /// The stock Mac client token used by platform-specific media commands.
    Mac,
}

/// One Lua command declared by a `<Binding>` element.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiBindingDefinition {
    name: String,
    header: Option<String>,
    body: String,
    run_on_up: bool,
    hidden: bool,
    debug: bool,
    platform: Option<UiBindingPlatform>,
}

impl UiBindingDefinition {
    /// Constructs a declaration after XML and Lua validation.
    pub(super) fn new(
        name: String,
        header: Option<String>,
        body: String,
        run_on_up: bool,
        hidden: bool,
        debug: bool,
        platform: Option<UiBindingPlatform>,
    ) -> Self {
        Self {
            name,
            header,
            body,
            run_on_up,
            hidden,
            debug,
            platform,
        }
    }

    /// Returns the global command identity used by saved key assignments.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the optional localized-header token that starts a UI group.
    #[must_use]
    pub fn header(&self) -> Option<&str> {
        self.header.as_deref()
    }

    /// Returns the authored Lua 5.1 body without normalizing whitespace.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Reports whether stock invokes the body for both down and up transitions.
    #[must_use]
    pub const fn runs_on_up(&self) -> bool {
        self.run_on_up
    }

    /// Reports whether the command is omitted from ordinary binding UI lists.
    #[must_use]
    pub const fn is_hidden(&self) -> bool {
        self.hidden
    }

    /// Reports whether the command belongs to the debug-only vocabulary.
    #[must_use]
    pub const fn is_debug(&self) -> bool {
        self.debug
    }

    /// Returns the exact client-platform gate, or no gate for shared commands.
    #[must_use]
    pub const fn platform(&self) -> Option<UiBindingPlatform> {
        self.platform
    }

    /// Reports whether stock admits the command on the selected client platform.
    #[must_use]
    pub fn is_available_on(&self, platform: UiBindingPlatform) -> bool {
        self.platform.is_none() || self.platform == Some(platform)
    }
}

/// One default modifier/button chord declared by `<ModifiedClick>`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiModifiedClickDefinition {
    action: String,
    default: UiModifiedClickChord,
}

impl UiModifiedClickDefinition {
    /// Constructs a declaration after required-token validation.
    pub(super) fn new(action: String, default: UiModifiedClickChord) -> Self {
        Self { action, default }
    }

    /// Returns the global modified-click action identity.
    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }

    /// Returns the exact stock default chord token.
    #[must_use]
    pub fn default(&self) -> &str {
        self.default.as_str()
    }

    /// Returns the validated stock default chord.
    #[must_use]
    pub const fn default_chord(&self) -> &UiModifiedClickChord {
        &self.default
    }
}

/// One validated built-in or AddOn binding source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiBindingDocument {
    path: AssetPath,
    bindings: Vec<UiBindingDefinition>,
    modified_clicks: Vec<UiModifiedClickDefinition>,
}

impl UiBindingDocument {
    /// Constructs a source-order document after full declaration validation.
    pub(super) fn new(
        path: AssetPath,
        bindings: Vec<UiBindingDefinition>,
        modified_clicks: Vec<UiModifiedClickDefinition>,
    ) -> Self {
        Self {
            path,
            bindings,
            modified_clicks,
        }
    }

    /// Returns the exact archive-relative source identity.
    #[must_use]
    pub fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns command declarations in authored order.
    #[must_use]
    pub fn bindings(&self) -> &[UiBindingDefinition] {
        &self.bindings
    }

    /// Returns modified-click defaults in authored order.
    #[must_use]
    pub fn modified_clicks(&self) -> &[UiModifiedClickDefinition] {
        &self.modified_clicks
    }
}
