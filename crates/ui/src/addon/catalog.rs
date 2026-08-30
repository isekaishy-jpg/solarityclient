//! AddOn discovery over the explicit loose/MPQ asset boundary.

use solarity_asset::{AssetError, AssetPath, AssetStore};

use super::{AddonCatalogError, AddonDefinition};
use crate::addon::toc::ParsedToc;

/// TOC interface number shipped by client build 12340.
pub const STOCK_INTERFACE_VERSION: u32 = 30_300;

/// CRC sent by stock for AddOns carrying Blizzard's `.pub` signature marker.
pub const STANDARD_ADDON_CRC: u32 = 0x4C1C_776D;

/// Ordered AddOn identities and parsed TOC metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AddonCatalog {
    addons: Vec<AddonDefinition>,
}

impl AddonCatalog {
    /// Discovers direct AddOn folders and resolves each matching TOC through
    /// stock's loose-first, MPQ-second AddOn source stack.
    ///
    /// Directories without a same-named TOC are not AddOns and are ignored.
    ///
    /// # Errors
    ///
    /// Returns a catalog error when permitted storage cannot be queried or a
    /// present TOC has invalid build-12340 metadata.
    pub fn discover(store: &mut AssetStore) -> Result<Self, AddonCatalogError> {
        let locale = store.locale();
        let mut addons = Vec::new();
        for name in store.addon_names()? {
            let toc_path = addon_path(&name, "toc")?;
            let bytes = match store.read_addon_file(&toc_path) {
                Ok(bytes) => bytes,
                Err(AssetError::AssetNotFound { .. }) => continue,
                Err(error) => return Err(error.into()),
            };
            let pub_path = addon_path(&name, "pub")?;
            let signed = store.contains_addon_file(&pub_path)?;
            addons.push(ParsedToc::parse(&name, &bytes)?.finish(name, locale, signed)?);
        }
        Ok(Self { addons })
    }

    /// Returns deterministic folder order used by UI enumeration and auth.
    #[must_use]
    pub fn addons(&self) -> &[AddonDefinition] {
        &self.addons
    }
}

fn addon_path(name: &str, extension: &str) -> Result<AssetPath, AssetError> {
    AssetPath::new(format!("Interface\\AddOns\\{name}\\{name}.{extension}"))
}
