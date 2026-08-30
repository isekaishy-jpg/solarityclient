//! Realm-list metadata authored in build-12340 client databases.

mod category;
mod configuration;

pub use category::{RealmCategoryCatalog, RealmCategoryDefinition};
pub use configuration::{RealmConfiguration, RealmConfigurationCatalog};
