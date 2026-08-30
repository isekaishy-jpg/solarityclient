//! UI-owned realm directory values exposed synchronously to Glue Lua.

/// Boolean realm properties returned by stock `GetRealmInfo`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UiRealmFlags {
    invalid: bool,
    offline: bool,
    locked: bool,
    player_killing_allowed: bool,
    roleplaying: bool,
}

impl UiRealmFlags {
    /// Captures the complete set of realm-list Boolean properties.
    #[must_use]
    pub const fn new(
        invalid: bool,
        offline: bool,
        locked: bool,
        player_killing_allowed: bool,
        roleplaying: bool,
    ) -> Self {
        Self {
            invalid,
            offline,
            locked,
            player_killing_allowed,
            roleplaying,
        }
    }

    /// Reports whether the client considers this realm incompatible.
    #[must_use]
    pub const fn is_invalid(self) -> bool {
        self.invalid
    }

    /// Reports whether the realm is unavailable.
    #[must_use]
    pub const fn is_offline(self) -> bool {
        self.offline
    }

    /// Reports whether the account is locked out of this realm.
    #[must_use]
    pub const fn is_locked(self) -> bool {
        self.locked
    }

    /// Reports whether the realm permits player killing.
    #[must_use]
    pub const fn player_killing_allowed(self) -> bool {
        self.player_killing_allowed
    }

    /// Reports whether the realm uses roleplaying rules.
    #[must_use]
    pub const fn roleplaying(self) -> bool {
        self.roleplaying
    }
}

/// Version tuple returned only when a realm advertises a required build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiRealmVersion {
    major: u8,
    minor: u8,
    revision: u8,
    build: u16,
    realm_type: u32,
}

impl UiRealmVersion {
    /// Captures the build and realm type carried by a versioned realm row.
    #[must_use]
    pub const fn new(major: u8, minor: u8, revision: u8, build: u16, realm_type: u32) -> Self {
        Self {
            major,
            minor,
            revision,
            build,
            realm_type,
        }
    }

    /// Returns the major client version.
    #[must_use]
    pub const fn major(self) -> u8 {
        self.major
    }

    /// Returns the minor client version.
    #[must_use]
    pub const fn minor(self) -> u8 {
        self.minor
    }

    /// Returns the patch revision.
    #[must_use]
    pub const fn revision(self) -> u8 {
        self.revision
    }

    /// Returns the required client build.
    #[must_use]
    pub const fn build(self) -> u16 {
        self.build
    }

    /// Returns the numeric realm rule-set type.
    #[must_use]
    pub const fn realm_type(self) -> u32 {
        self.realm_type
    }
}

/// One realm row ready for build-12340 Glue presentation.
#[derive(Clone, Debug, PartialEq)]
pub struct UiRealmInfo {
    id: u32,
    name: String,
    character_count: u8,
    flags: UiRealmFlags,
    load: f64,
    version: Option<UiRealmVersion>,
}

impl UiRealmInfo {
    /// Captures one realm-list row without retaining transport-specific data.
    #[must_use]
    pub fn new(
        id: u32,
        name: String,
        character_count: u8,
        flags: UiRealmFlags,
        load: f64,
        version: Option<UiRealmVersion>,
    ) -> Self {
        Self {
            id,
            name,
            character_count,
            flags,
            load,
            version,
        }
    }

    /// Returns the stable realm-list identifier.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the server-authored realm name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the account's known character count on this realm.
    #[must_use]
    pub const fn character_count(&self) -> u8 {
        self.character_count
    }

    /// Returns the realm's Boolean presentation properties.
    #[must_use]
    pub const fn flags(&self) -> UiRealmFlags {
        self.flags
    }

    /// Returns the stock-normalized population load value.
    #[must_use]
    pub const fn load(&self) -> f64 {
        self.load
    }

    /// Returns advertised version fields when the server requires a build.
    #[must_use]
    pub const fn version(&self) -> Option<UiRealmVersion> {
        self.version
    }
}

/// One DBC-authored category and its server-supplied realm rows.
#[derive(Clone, Debug, PartialEq)]
pub struct UiRealmCategory {
    id: u32,
    name: String,
    invalid_tournament: bool,
    tournament: bool,
    invalid_locale: bool,
    realms: Vec<UiRealmInfo>,
}

impl UiRealmCategory {
    /// Captures one physical category in `Cfg_Categories.dbc` order.
    #[must_use]
    pub fn new(id: u32, name: String, realms: Vec<UiRealmInfo>) -> Self {
        Self {
            id,
            name,
            invalid_tournament: false,
            tournament: false,
            invalid_locale: false,
            realms,
        }
    }

    /// Attaches stock tournament and locale eligibility results.
    #[must_use]
    pub const fn with_eligibility(
        mut self,
        invalid_tournament: bool,
        tournament: bool,
        invalid_locale: bool,
    ) -> Self {
        self.invalid_tournament = invalid_tournament;
        self.tournament = tournament;
        self.invalid_locale = invalid_locale;
        self
    }

    /// Returns the DBC category identifier.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the localized DBC category name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the server realms assigned to this category.
    #[must_use]
    pub fn realms(&self) -> &[UiRealmInfo] {
        &self.realms
    }

    /// Reports whether this tournament category is unavailable to the account.
    #[must_use]
    pub const fn is_invalid_tournament(&self) -> bool {
        self.invalid_tournament
    }

    /// Reports whether the category contains tournament realms.
    #[must_use]
    pub const fn is_tournament(&self) -> bool {
        self.tournament
    }

    /// Reports whether the category's locale differs from the client locale.
    #[must_use]
    pub const fn is_invalid_locale(&self) -> bool {
        self.invalid_locale
    }
}

/// Complete realm state queried synchronously by stock Glue Lua.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiRealmDirectory {
    categories: Vec<UiRealmCategory>,
    selected_category: usize,
    selected_realm_id: Option<u32>,
}

impl UiRealmDirectory {
    /// Captures physical DBC categories and the currently selected realm.
    #[must_use]
    pub fn new(categories: Vec<UiRealmCategory>, selected_realm_id: Option<u32>) -> Self {
        let selected_category = selected_realm_id
            .and_then(|realm_id| {
                categories
                    .iter()
                    .position(|category| category.realms.iter().any(|realm| realm.id == realm_id))
            })
            .unwrap_or(0);
        Self {
            categories,
            selected_category,
            selected_realm_id,
        }
    }

    /// Returns physical DBC categories, including categories with no realms.
    #[must_use]
    pub fn categories(&self) -> &[UiRealmCategory] {
        &self.categories
    }

    /// Returns the selected realm identifier, if one is known.
    #[must_use]
    pub const fn selected_realm_id(&self) -> Option<u32> {
        self.selected_realm_id
    }

    pub(crate) fn displayed_categories(&self) -> impl Iterator<Item = &UiRealmCategory> {
        self.categories
            .iter()
            .filter(|category| !category.realms.is_empty())
    }

    pub(crate) fn displayed_category(&self, one_based_index: u32) -> Option<&UiRealmCategory> {
        let index = usize::try_from(one_based_index.checked_sub(1)?).ok()?;
        self.displayed_categories().nth(index)
    }

    pub(crate) fn realm(
        &self,
        category_index: Option<u32>,
        realm_index: u32,
    ) -> Option<&UiRealmInfo> {
        let index = usize::try_from(realm_index.checked_sub(1)?).ok()?;
        if let Some(category_index) = category_index {
            return self.displayed_category(category_index)?.realms.get(index);
        }

        let mut remaining = index;
        for category in &self.categories {
            if remaining < category.realms.len() {
                return category.realms.get(remaining);
            }
            remaining = remaining.checked_sub(category.realms.len())?;
        }
        None
    }

    pub(crate) fn select_category(&mut self, one_based_index: u32) {
        let Some(category) = self.displayed_category(one_based_index) else {
            return;
        };
        let category_id = category.id;
        if let Some(index) = self
            .categories
            .iter()
            .position(|candidate| candidate.id == category_id)
        {
            self.selected_category = index;
        }
    }

    pub(crate) fn selected_displayed_category(&self) -> u32 {
        let Some(selected) = self.categories.get(self.selected_category) else {
            return 1;
        };
        self.displayed_categories()
            .position(|category| category.id == selected.id)
            .and_then(|index| u32::try_from(index + 1).ok())
            .unwrap_or(1)
    }
}
