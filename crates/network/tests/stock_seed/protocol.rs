//! External stock-compatibility tests for the stable protocol boundary.

use std::error::Error;

use solarity_network::{AddonManifestError, WorldAddon, WorldAddonManifest};

/// Add-on identity records remain ordered and reject names that break stock CStrings.
#[test]
fn addon_manifest_validates_names_and_preserves_order() -> Result<(), Box<dyn Error>> {
    let first = WorldAddon::new("Blizzard_TimeManager", true, 7, 9)?;
    let second = WorldAddon::new("DamageMeter", false, 11, 13)?;
    let manifest = WorldAddonManifest::new(vec![first, second])?;

    assert_eq!(manifest.addons()[0].name(), "Blizzard_TimeManager");
    assert!(manifest.addons()[0].is_enabled());
    assert_eq!(manifest.addons()[0].crc(), 7);
    assert_eq!(manifest.addons()[0].unknown(), 9);
    assert_eq!(manifest.addons()[1].name(), "DamageMeter");
    assert!(!manifest.addons()[1].is_enabled());
    assert!(WorldAddonManifest::empty().addons().is_empty());
    assert!(matches!(
        WorldAddon::new("broken\0addon", false, 0, 0),
        Err(AddonManifestError::InvalidName { .. })
    ));
    assert!(matches!(
        WorldAddon::new("", false, 0, 0),
        Err(AddonManifestError::InvalidName { .. })
    ));
    let repeated = WorldAddon::new("BoundedAddon", true, 0, 0)?;
    assert!(matches!(
        WorldAddonManifest::new(vec![repeated; 4_097]),
        Err(AddonManifestError::TooManyAddons { .. })
    ));
    Ok(())
}
