//! External stock-compatibility tests for stock binding declarations.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{AddonCatalog, UiBindingCatalog, UiBindingError};

use crate::support::{Fixture, FixtureFile};

const BUILTIN_PATH: &str = "Interface\\FrameXML\\Bindings.xml";

/// Binding declarations retain source order, flags, Lua bodies, and click defaults.
#[test]
fn builtin_catalog_decodes_stock_binding_vocabulary() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[FixtureFile {
        path: BUILTIN_PATH,
        bytes: br#"<Bindings>
  <Binding name="MOVEFORWARD" runOnUp="true" header="MOVEMENT">
    if keystate == "down" then MoveForwardStart() else MoveForwardStop() end
  </Binding>
  <Binding name="TOGGLESTATS" hidden="true" debug="true">
    ToggleStats()
  </Binding>
  <Binding name="ITUNES_PLAYPAUSE" platform="mac">
    MusicPlayer_PlayPause()
  </Binding>
  <ModifiedClick action="SELFCAST" default="ALT"/>
  <ModifiedClick action="CHATLINK" default="SHIFT-BUTTON1"/>
</Bindings>"#,
    }])?;
    let mut assets = mount(&fixture)?;

    let catalog = UiBindingCatalog::load_builtin(&mut assets)?;

    assert_eq!(catalog.documents().len(), 1);
    assert_eq!(
        catalog.documents()[0].path().as_str(),
        BUILTIN_PATH.to_ascii_uppercase()
    );
    assert_eq!(catalog.bindings().len(), 3);
    let bindings = catalog.bindings().collect::<Vec<_>>();
    assert_eq!(bindings[0].name(), "MOVEFORWARD");
    assert_eq!(bindings[0].header(), Some("MOVEMENT"));
    assert!(bindings[0].body().contains("MoveForwardStart"));
    assert!(bindings[0].runs_on_up());
    assert!(!bindings[0].is_hidden());
    assert!(bindings[1].is_hidden());
    assert!(bindings[1].is_debug());
    assert!(bindings[2].is_mac_only());
    let clicks = catalog.modified_clicks().collect::<Vec<_>>();
    assert_eq!(clicks.len(), 2);
    assert_eq!(clicks[0].action(), "SELFCAST");
    assert_eq!(clicks[0].default(), "ALT");
    assert_eq!(clicks[1].default(), "SHIFT-BUTTON1");
    Ok(())
}

/// AddOn binding discovery uses the explicit loose-first source stack and caller order.
#[test]
fn addon_catalog_appends_special_loose_binding_document() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: BUILTIN_PATH,
            bytes: br#"<Bindings><Binding name="BUILTIN">Builtin()</Binding></Bindings>"#,
        },
        FixtureFile {
            path: "Interface\\AddOns\\Example\\Example.toc",
            bytes: b"## Interface: 30300\nExample.lua\n",
        },
        FixtureFile {
            path: "Interface\\AddOns\\Example\\Bindings.xml",
            bytes: br#"<Bindings><Binding name="ARCHIVE">ArchiveBody()</Binding></Bindings>"#,
        },
    ])?;
    fixture.write_loose_file("Interface/AddOns/Example/Example.pub", b"signature")?;
    fixture.write_loose_file(
        "Interface/AddOns/Example/Bindings.xml",
        br#"<Bindings>
  <Binding name="ADDON_ACTION" runOnUp="true">AddonBody(keystate)</Binding>
</Bindings>"#,
    )?;
    let mut assets = mount(&fixture)?;
    let addons = AddonCatalog::discover(&mut assets)?;
    let mut bindings = UiBindingCatalog::load_builtin(&mut assets)?;

    assert!(bindings.append_addon(&mut assets, &addons.addons()[0])?);

    assert_eq!(bindings.documents().len(), 2);
    assert_eq!(
        bindings.documents()[1].path().as_str(),
        "INTERFACE\\ADDONS\\EXAMPLE\\BINDINGS.XML"
    );
    let names = bindings
        .bindings()
        .map(|binding| binding.name())
        .collect::<Vec<_>>();
    assert_eq!(names, ["BUILTIN", "ADDON_ACTION"]);
    Ok(())
}

/// An AddOn without the special document is ordinary absence, not an archive error.
#[test]
fn addon_without_bindings_document_is_not_fabricated() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[FixtureFile {
        path: BUILTIN_PATH,
        bytes: br#"<Bindings><Binding name="BUILTIN">Builtin()</Binding></Bindings>"#,
    }])?;
    fixture.write_loose_file(
        "Interface/AddOns/NoBindings/NoBindings.toc",
        b"## Interface: 30300\nSource.lua\n",
    )?;
    let mut assets = mount(&fixture)?;
    let addons = AddonCatalog::discover(&mut assets)?;
    let mut bindings = UiBindingCatalog::load_builtin(&mut assets)?;

    assert!(!bindings.append_addon(&mut assets, &addons.addons()[0])?);
    assert_eq!(bindings.documents().len(), 1);
    Ok(())
}

/// Unknown elements and attributes do not receive permissive compatibility handling.
#[test]
fn binding_catalog_rejects_non_stock_document_shapes() -> Result<(), Box<dyn Error>> {
    let invalid_sources: [&[u8]; 4] = [
        br#"<Ui><Binding name="ACTION">Body()</Binding></Ui>"#,
        br#"<Bindings><Unknown/></Bindings>"#,
        br#"<Bindings><Binding name="ACTION" runOnUp="1">Body()</Binding></Bindings>"#,
        br#"<Bindings><Binding name="ACTION" platform="windows">Body()</Binding></Bindings>"#,
    ];

    for source in invalid_sources {
        let fixture = Fixture::new(&[FixtureFile {
            path: BUILTIN_PATH,
            bytes: source,
        }])?;
        let mut assets = mount(&fixture)?;
        assert!(matches!(
            UiBindingCatalog::load_builtin(&mut assets),
            Err(UiBindingError::Schema { .. })
        ));
    }
    Ok(())
}

/// Binding Lua is compiled at catalog construction instead of failing on first input.
#[test]
fn binding_catalog_rejects_malformed_lua_body() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[FixtureFile {
        path: BUILTIN_PATH,
        bytes: br#"<Bindings><Binding name="BROKEN">local = value</Binding></Bindings>"#,
    }])?;
    let mut assets = mount(&fixture)?;

    assert!(matches!(
        UiBindingCatalog::load_builtin(&mut assets),
        Err(UiBindingError::Load(_))
    ));
    Ok(())
}

/// Stock reports duplicate command, header, and modified-click identities.
#[test]
fn binding_catalog_rejects_duplicate_global_identities() -> Result<(), Box<dyn Error>> {
    let duplicate_sources: [&[u8]; 3] = [
        br#"<Bindings>
  <Binding name="DUPLICATE">First()</Binding>
  <Binding name="DUPLICATE">Second()</Binding>
</Bindings>"#,
        br#"<Bindings>
  <Binding name="FIRST" header="GROUP">First()</Binding>
  <Binding name="SECOND" header="GROUP">Second()</Binding>
</Bindings>"#,
        br#"<Bindings>
  <ModifiedClick action="SELFCAST" default="ALT"/>
  <ModifiedClick action="SELFCAST" default="CTRL"/>
</Bindings>"#,
    ];

    for source in duplicate_sources {
        let fixture = Fixture::new(&[FixtureFile {
            path: BUILTIN_PATH,
            bytes: source,
        }])?;
        let mut assets = mount(&fixture)?;
        assert!(matches!(
            UiBindingCatalog::load_builtin(&mut assets),
            Err(UiBindingError::Schema { .. })
        ));
    }
    Ok(())
}

/// Duplicate checks span built-in and AddOn documents without partial append.
#[test]
fn addon_cannot_replace_an_existing_binding_definition() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[FixtureFile {
        path: BUILTIN_PATH,
        bytes: br#"<Bindings><Binding name="SHARED">Builtin()</Binding></Bindings>"#,
    }])?;
    fixture.write_loose_file(
        "Interface/AddOns/Example/Example.toc",
        b"## Interface: 30300\nExample.lua\n",
    )?;
    fixture.write_loose_file(
        "Interface/AddOns/Example/Bindings.xml",
        br#"<Bindings><Binding name="SHARED">Addon()</Binding></Bindings>"#,
    )?;
    let mut assets = mount(&fixture)?;
    let addons = AddonCatalog::discover(&mut assets)?;
    let mut bindings = UiBindingCatalog::load_builtin(&mut assets)?;

    assert!(matches!(
        bindings.append_addon(&mut assets, &addons.addons()[0]),
        Err(UiBindingError::Schema { .. })
    ));
    assert_eq!(bindings.documents().len(), 1);
    assert_eq!(bindings.bindings().len(), 1);
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let data_root = ClientDataRoot::new(fixture.data_root())?;
    Ok(AssetStore::mount(ArchiveCatalog::discover(
        data_root,
        Locale::EnUs,
    )?)?)
}
