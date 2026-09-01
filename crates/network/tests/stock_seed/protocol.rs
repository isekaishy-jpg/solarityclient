//! External stock-compatibility tests for the stable protocol boundary.

use std::error::Error;

use solarity_network::{
    AddonManifestError, WorldActionButtonUpdate, WorldActionButtons, WorldAddon,
    WorldAddonManifest, WorldTimeSpeed,
};

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

/// Packed realm time exposes every build-12340 calendar field without host-time input.
#[test]
fn world_time_preserves_stock_packed_calendar() -> Result<(), Box<dyn Error>> {
    // Tuesday, December 8, 2009 at 21:37. The wire uses zero-based month,
    // month-day, and Sunday-first weekday fields.
    let packed = (9 << 24) | (11 << 20) | (7 << 14) | (2 << 11) | (21 << 6) | 37;
    let time = WorldTimeSpeed::new(packed, 1.0 / 60.0, 0)?;

    assert_eq!(time.year(), 2009);
    assert_eq!(time.month_index(), 11);
    assert_eq!(time.month_day(), 8);
    assert_eq!(time.weekday_index(), 2);
    assert_eq!(time.hour(), 21);
    assert_eq!(time.minute(), 37);
    Ok(())
}

/// Impossible packed calendar fields fail at the authoritative packet boundary.
#[test]
fn world_time_rejects_invalid_packed_calendar() {
    let invalid_month = 12 << 20;
    let invalid_weekday = 7 << 11;
    let february_thirtieth = (1 << 20) | (29 << 14);
    let non_leap_february_twenty_ninth = (1 << 24) | (1 << 20) | (28 << 14);
    let leap_february_twenty_ninth = (1 << 20) | (28 << 14);

    assert!(WorldTimeSpeed::new(invalid_month, 0.0, 0).is_err());
    assert!(WorldTimeSpeed::new(invalid_weekday, 0.0, 0).is_err());
    assert!(WorldTimeSpeed::new(february_thirtieth, 0.0, 0).is_err());
    assert!(WorldTimeSpeed::new(non_leap_february_twenty_ninth, 0.0, 0).is_err());
    assert!(WorldTimeSpeed::new(leap_february_twenty_ninth, 0.0, 0).is_ok());
}

/// Action-button packets retain all 144 packed values and the clear variant.
#[test]
fn action_button_packet_decodes_complete_images() -> Result<(), Box<dyn Error>> {
    let mut payload = vec![1];
    for slot in 0_u32..144 {
        payload.extend_from_slice(&(0x8000_0000 | slot).to_le_bytes());
    }
    let buttons = WorldActionButtons::decode(&payload)?;
    assert_eq!(buttons.update(), WorldActionButtonUpdate::Replace);
    assert_eq!(buttons.slots().len(), 144);
    assert_eq!(buttons.slot(0), Some(0x8000_0000));
    assert_eq!(buttons.slot(143), Some(0x8000_008F));
    assert_eq!(buttons.slot(144), None);

    let clear = WorldActionButtons::decode(&[2])?;
    assert_eq!(clear.update(), WorldActionButtonUpdate::Clear);
    assert!(clear.slots().iter().all(|slot| *slot == 0));
    Ok(())
}

/// Action-button framing rejects partial images, unknown states, and padded clears.
#[test]
fn action_button_packet_rejects_non_stock_framing() {
    assert!(WorldActionButtons::decode(&[]).is_err());
    assert!(WorldActionButtons::decode(&[3]).is_err());
    assert!(WorldActionButtons::decode(&[1, 0]).is_err());
    assert!(WorldActionButtons::decode(&[2, 0]).is_err());
}
