//! Typed AddOn identity, metadata, and initial enablement state.

/// Compatibility of an AddOn's declared interface with build 12340.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddonCompatibility {
    /// The AddOn explicitly targets build 12340.
    Current,
    /// The AddOn targets an earlier interface and needs the user's out-of-date
    /// AddOn override before stock will load it.
    OutOfDate,
    /// The AddOn targets a later interface that this client cannot provide.
    Newer,
    /// The TOC omitted the mandatory interface declaration.
    MissingInterface,
}

/// One discovered build-12340 AddOn module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddonDefinition {
    pub(super) name: String,
    pub(super) title: String,
    pub(super) notes: Option<String>,
    pub(super) interface: Option<u32>,
    pub(super) compatibility: AddonCompatibility,
    pub(super) dependencies: Vec<String>,
    pub(super) optional_dependencies: Vec<String>,
    pub(super) load_on_demand: bool,
    pub(super) enabled_by_default: bool,
    pub(super) signed: bool,
    pub(super) secure: bool,
    pub(super) saved_variables: Vec<String>,
    pub(super) saved_variables_per_character: Vec<String>,
    pub(super) entrypoints: Vec<String>,
}

impl AddonDefinition {
    /// Returns the exact install-directory identity used by the protocol.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the locale-selected title, falling back only to the folder name.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the locale-selected descriptive metadata.
    #[must_use]
    pub fn notes(&self) -> Option<&str> {
        self.notes.as_deref()
    }

    /// Returns the declared TOC interface number.
    #[must_use]
    pub const fn interface(&self) -> Option<u32> {
        self.interface
    }

    /// Returns the build-12340 compatibility classification.
    #[must_use]
    pub const fn compatibility(&self) -> AddonCompatibility {
        self.compatibility
    }

    /// Returns mandatory module names in declared order.
    #[must_use]
    pub fn dependencies(&self) -> &[String] {
        &self.dependencies
    }

    /// Returns optional module names in declared order.
    #[must_use]
    pub fn optional_dependencies(&self) -> &[String] {
        &self.optional_dependencies
    }

    /// Reports whether stock defers execution until explicitly requested.
    #[must_use]
    pub const fn is_load_on_demand(&self) -> bool {
        self.load_on_demand
    }

    /// Reports the TOC's initial enablement before per-account state is read.
    #[must_use]
    pub const fn is_enabled_by_default(&self) -> bool {
        self.enabled_by_default
    }

    /// Reports whether the installation provides Blizzard's `.pub` marker.
    #[must_use]
    pub const fn is_signed(&self) -> bool {
        self.signed
    }

    /// Reports whether the TOC explicitly declares secure execution.
    #[must_use]
    pub const fn is_secure(&self) -> bool {
        self.secure
    }

    /// Returns account-wide saved-variable names in declaration order.
    #[must_use]
    pub fn saved_variables(&self) -> &[String] {
        &self.saved_variables
    }

    /// Returns character-scoped saved-variable names in declaration order.
    #[must_use]
    pub fn saved_variables_per_character(&self) -> &[String] {
        &self.saved_variables_per_character
    }

    /// Returns Lua/XML payload paths in TOC execution order.
    #[must_use]
    pub fn entrypoints(&self) -> &[String] {
        &self.entrypoints
    }

    /// Reports default login enablement without an out-of-date override.
    #[must_use]
    pub const fn is_initially_enabled(&self) -> bool {
        self.enabled_by_default && matches!(self.compatibility, AddonCompatibility::Current)
    }
}
