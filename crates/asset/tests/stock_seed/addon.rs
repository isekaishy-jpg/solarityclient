//! External tests for stock's explicit loose/MPQ AddOn source stack.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetError, AssetPath, AssetStore, ClientDataRoot, Locale};

use crate::support::{Fixture, FixtureFile};

/// Directory identity is discovered from the loose install tree while module
/// payloads remain available from the mounted archive namespace.
#[test]
fn addon_directory_identity_can_resolve_archived_payloads() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Interface\\AddOns\\Blizzard_TimeManager\\Blizzard_TimeManager.toc",
        bytes: b"## Interface: 30300\nBlizzard_TimeManager.lua\n",
    }])?;
    fixture.write_loose_file(
        "Interface/AddOns/Blizzard_TimeManager/Blizzard_TimeManager.pub",
        b"signed marker",
    )?;
    let mut store = mount(&fixture)?;
    let toc = AssetPath::new("interface/addons/blizzard_timemanager/blizzard_timemanager.toc")?;

    assert_eq!(store.addon_names()?, ["Blizzard_TimeManager"]);
    assert!(store.contains_addon_file(&toc)?);
    assert_eq!(
        store.read_addon_file(&toc)?,
        b"## Interface: 30300\nBlizzard_TimeManager.lua\n"
    );
    Ok(())
}

/// A custom loose module overrides the same virtual AddOn file without making
/// loose lookup a general client-asset fallback.
#[test]
fn loose_addon_payload_has_scoped_precedence() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Interface\\AddOns\\Example\\Example.toc",
        bytes: b"archived",
    }])?;
    fixture.write_loose_file("Interface/AddOns/Example/Example.toc", b"loose")?;
    fixture.write_loose_file("DBFilesClient/Forbidden.dbc", b"not an addon")?;
    let mut store = mount(&fixture)?;

    let addon = AssetPath::new("Interface/AddOns/Example/Example.toc")?;
    assert_eq!(store.read_addon_file(&addon)?, b"loose");
    let forbidden = AssetPath::new("DBFilesClient/Forbidden.dbc")?;
    assert!(matches!(
        store.read_addon_file(&forbidden),
        Err(AssetError::InvalidAssetPath { .. })
    ));
    assert!(matches!(
        store.read(&forbidden),
        Err(AssetError::AssetNotFound { .. })
    ));
    Ok(())
}

/// An installation with no AddOns directory has an empty catalog rather than
/// an invented built-in module list.
#[test]
fn absent_addon_directory_is_empty() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[])?;
    let store = mount(&fixture)?;

    assert!(store.addon_names()?.is_empty());
    assert!(!fixture.install_root().join("Interface/AddOns").exists());
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    Ok(AssetStore::mount(ArchiveCatalog::discover(
        root,
        Locale::EnUs,
    )?)?)
}
