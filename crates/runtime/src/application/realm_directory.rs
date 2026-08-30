//! Projection of authenticated realm rows through client-authored metadata.

use solarity_asset::{
    AssetError, AssetStore, Locale, RealmCategoryCatalog, RealmConfigurationCatalog,
};
use solarity_network::{RealmDirectory, RealmEntry, RealmRecommendation};
use solarity_ui::{UiRealmCategory, UiRealmDirectory, UiRealmFlags, UiRealmInfo, UiRealmVersion};

/// Tournament marker stored in `Cfg_Categories.dbc::Flags`.
const CATEGORY_FLAG_TOURNAMENT: u32 = 0x1;

/// Client-authored metadata required to present server-authored realm rows.
pub(super) struct RuntimeRealmMetadata {
    locale: Locale,
    categories: RealmCategoryCatalog,
    configurations: RealmConfigurationCatalog,
}

impl RuntimeRealmMetadata {
    /// Loads both exact build-12340 tables before the asset store enters Glue.
    pub(super) fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        Ok(Self {
            locale: store.locale(),
            categories: RealmCategoryCatalog::load(store)?,
            configurations: RealmConfigurationCatalog::load(store)?,
        })
    }

    /// Builds DBC categories before any authenticated realm list is available.
    pub(super) fn empty_directory(&self) -> UiRealmDirectory {
        self.project(&RealmDirectory::default(), None, false)
    }

    /// Joins exact server categories and realm types to their DBC records.
    pub(super) fn project(
        &self,
        directory: &RealmDirectory,
        selected_realm_id: Option<u32>,
        tournament_access: bool,
    ) -> UiRealmDirectory {
        let categories = self
            .categories
            .categories()
            .map(|category| {
                let realms = directory
                    .entries()
                    .iter()
                    .filter(|realm| realm.category().id() == category.id())
                    .map(|realm| self.project_realm(realm))
                    .collect();
                let tournament = category.flags() & CATEGORY_FLAG_TOURNAMENT != 0;
                UiRealmCategory::new(category.id(), category.name().to_owned(), realms)
                    .with_eligibility(
                        tournament && !tournament_access,
                        tournament,
                        invalid_locale(category.locale_mask(), self.locale),
                    )
            })
            .collect();
        UiRealmDirectory::new(categories, selected_realm_id)
    }

    /// Finds the stock preferred-realm candidate within one visible category.
    pub(super) fn preferred_realm(
        &self,
        directory: &RealmDirectory,
        category_index: u32,
        player_killing_allowed: bool,
        roleplaying: bool,
    ) -> Option<(u32, u32)> {
        let category_id = self
            .categories
            .categories()
            .filter(|category| {
                directory
                    .entries()
                    .iter()
                    .any(|realm| realm.category().id() == category.id())
            })
            .nth(usize::try_from(category_index.checked_sub(1)?).ok()?)?
            .id();

        let mut full_match = None;
        let mut visible_index = 0_u32;
        for realm in directory
            .entries()
            .iter()
            .filter(|realm| realm.category().id() == category_id)
        {
            visible_index = visible_index.checked_add(1)?;
            if realm.is_invalid() || realm.is_offline() {
                continue;
            }
            if realm.recommendation() == RealmRecommendation::Blue {
                return Some((u32::from(realm.id()), visible_index));
            }
            let (realm_pvp, realm_rp) = self.realm_rules(realm);
            if realm_pvp != player_killing_allowed || realm_rp != roleplaying {
                continue;
            }
            if realm.recommendation() != RealmRecommendation::RedFull {
                return Some((u32::from(realm.id()), visible_index));
            }
            full_match = Some((u32::from(realm.id()), visible_index));
        }
        full_match
    }

    fn project_realm(&self, realm: &RealmEntry) -> UiRealmInfo {
        let (player_killing_allowed, roleplaying) = self.realm_rules(realm);
        let flags = UiRealmFlags::new(
            realm.is_invalid(),
            realm.is_offline(),
            realm.is_locked(),
            player_killing_allowed,
            roleplaying,
        );
        let version = realm
            .required_build()
            .map(|(major, minor, revision, build)| {
                UiRealmVersion::new(major, minor, revision, build, realm.realm_type().id())
            });
        UiRealmInfo::new(
            u32::from(realm.id()),
            realm.name().to_owned(),
            realm.character_count(),
            flags,
            realm_load(realm),
            version,
        )
    }

    pub(super) fn realm_rules(&self, realm: &RealmEntry) -> (bool, bool) {
        self.configurations
            .configurations()
            .find(|configuration| configuration.realm_type() == realm.realm_type().id())
            .map_or((false, false), |configuration| {
                (
                    configuration.player_killing_allowed(),
                    configuration.roleplaying(),
                )
            })
    }
}

/// Converts category locale masks through the build-12340 locale bit table.
const fn invalid_locale(mask: u32, locale: Locale) -> bool {
    if mask == 0 {
        return false;
    }
    let bit = match locale {
        Locale::EnUs | Locale::EnGb | Locale::EnCn | Locale::EnTw => 0,
        Locale::KoKr => 1,
        Locale::FrFr => 2,
        Locale::DeDe => 3,
        Locale::ZhCn => 4,
        Locale::ZhTw => 5,
        Locale::EsEs => 6,
        Locale::EsMx => 7,
        Locale::RuRu => 8,
    };
    mask & (1_u32 << bit) == 0
}

/// Converts protocol population and force flags to RealmList.lua's load scale.
fn realm_load(realm: &RealmEntry) -> f64 {
    match realm.recommendation() {
        RealmRecommendation::Blue => -3.0,
        RealmRecommendation::Green => -2.0,
        RealmRecommendation::RedFull => 2.0,
        RealmRecommendation::None => match realm.population() {
            population if population < 0.5 => -1.0,
            population if population < 1.0 => 0.0,
            population if population < 2.0 => 1.0,
            _ => 2.0,
        },
    }
}
