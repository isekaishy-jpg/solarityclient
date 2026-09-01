//! External stock-compatibility tests for AddOn catalog construction.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{AddonCatalog, AddonCompatibility, STANDARD_ADDON_CRC, UiAddonLoadState};

use crate::support::{Fixture, FixtureFile};

/// TOC metadata is locale-selected while dependencies and entrypoints retain
/// their declaration order.
#[test]
fn catalog_parses_stock_metadata_and_signature_identity() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[FixtureFile {
        path: "Interface\\AddOns\\Blizzard_Example\\Blizzard_Example.toc",
        bytes: br#"
## Interface: 30300
## Title: Example
## Title-enUS: Localized Example
## Notes: Stock module notes
## Dependencies: Blizzard_Dependency, Blizzard_Shared
## OptionalDeps: Blizzard_Optional
## LoadOnDemand: 1
## Secure: 1
## SavedVariables: ExampleAccount, ExampleShared
## SavedVariablesPerCharacter: ExampleCharacter
Example.xml
Source/Example.lua
"#,
    }])?;
    fixture.write_loose_file(
        "Interface/AddOns/Blizzard_Example/Blizzard_Example.pub",
        b"signature marker",
    )?;
    let mut assets = mount(&fixture)?;

    let catalog = AddonCatalog::discover(&mut assets)?;
    let addon = &catalog.addons()[0];
    assert_eq!(addon.name(), "Blizzard_Example");
    assert_eq!(addon.title(), "Localized Example");
    assert_eq!(addon.notes(), Some("Stock module notes"));
    assert_eq!(addon.interface(), Some(30_300));
    assert_eq!(addon.compatibility(), AddonCompatibility::Current);
    assert_eq!(
        addon.dependencies(),
        ["Blizzard_Dependency", "Blizzard_Shared"]
    );
    assert_eq!(addon.optional_dependencies(), ["Blizzard_Optional"]);
    assert!(addon.is_load_on_demand());
    assert!(addon.is_enabled_by_default());
    assert!(addon.is_initially_enabled());
    assert!(addon.is_signed());
    assert!(addon.is_secure());
    assert_eq!(addon.saved_variables(), ["ExampleAccount", "ExampleShared"]);
    assert_eq!(addon.saved_variables_per_character(), ["ExampleCharacter"]);
    assert_eq!(addon.entrypoints(), ["Example.xml", "Source\\Example.lua"]);
    let load_state = UiAddonLoadState::from_catalog(&catalog);
    assert_eq!(load_state.status_by_index(1), Some((false, false)));
    assert_eq!(
        load_state.status_by_name("blizzard_example"),
        Some((false, false))
    );
    assert!(load_state.set_status("Blizzard_Example", true, true));
    assert_eq!(load_state.status_by_index(1), Some((true, true)));
    assert!(!load_state.set_status("Missing", true, true));
    assert_eq!(STANDARD_ADDON_CRC, 0x4C1C_776D);
    Ok(())
}

/// Compatibility and DefaultState independently control initial enablement;
/// unrelated loose directories without a matching TOC are not catalog entries.
#[test]
fn catalog_classifies_interface_and_ignores_orphans() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[])?;
    fixture.write_loose_file(
        "Interface/AddOns/Older/Older.toc",
        b"## Interface: 30200\nOlder.lua\n",
    )?;
    fixture.write_loose_file(
        "Interface/AddOns/Disabled/Disabled.toc",
        b"## Interface: 30300\n## DefaultState: disabled\nDisabled.lua\n",
    )?;
    fixture.write_loose_file("Interface/AddOns/Orphan/Orphan.pub", b"marker")?;
    let mut assets = mount(&fixture)?;

    let catalog = AddonCatalog::discover(&mut assets)?;
    assert_eq!(catalog.addons().len(), 2);
    let disabled = &catalog.addons()[0];
    assert_eq!(disabled.name(), "Disabled");
    assert_eq!(disabled.compatibility(), AddonCompatibility::Current);
    assert!(!disabled.is_initially_enabled());
    let older = &catalog.addons()[1];
    assert_eq!(older.name(), "Older");
    assert_eq!(older.compatibility(), AddonCompatibility::OutOfDate);
    assert!(!older.is_initially_enabled());
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let data_root = ClientDataRoot::new(fixture.data_root())?;
    Ok(AssetStore::mount(ArchiveCatalog::discover(
        data_root,
        Locale::EnUs,
    )?)?)
}
