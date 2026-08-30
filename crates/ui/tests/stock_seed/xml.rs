//! External stock-compatibility tests for ordered built-in UI content.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{UiBundle, UiLoadError, UiManifestEntryKind, UiManifestKind, UiResourceContent};

use crate::support::{Fixture, FixtureFile};

const GLUE_TOC: &[u8] = br#"## Interface: 30300
# Stock comments and metadata do not enter the source list.
GlueStrings.lua
Layouts\Root.xml
"#;

const GLUE_LUA: &[u8] = br#"local marker = 7
if marker ~= 7 then error("must not run") end
"#;

const GLUE_XML: &[u8] = br#"<?xml version="1.0"?>
<Ui xmlns="http://www.blizzard.com/wow/ui/">
  <Frame name="RootFrame">
    <Script><![CDATA[local mask = 1 & 2]]></Script>
  </Frame>
</Ui>
"#;

/// TOC order is authoritative, paths are manifest-relative, and Lua is only compiled.
#[test]
fn glue_bundle_preserves_stock_manifest_order() -> Result<(), Box<dyn Error>> {
    let fixture = ui_fixture(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: GLUE_TOC,
        },
        FixtureFile {
            path: "Interface\\GlueXML\\GlueStrings.lua",
            bytes: GLUE_LUA,
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Layouts\\Root.xml",
            bytes: GLUE_XML,
        },
    ])?;
    let mut store = mount(&fixture)?;

    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;

    assert_eq!(bundle.manifest().entries().len(), 2);
    assert_eq!(
        bundle.manifest().entries()[0].kind(),
        UiManifestEntryKind::Lua
    );
    assert_eq!(
        bundle.manifest().entries()[0].path().as_str(),
        "INTERFACE\\GLUEXML\\GLUESTRINGS.LUA"
    );
    assert_eq!(
        bundle.manifest().entries()[1].path().as_str(),
        "INTERFACE\\GLUEXML\\LAYOUTS\\ROOT.XML"
    );

    assert!(matches!(
        bundle.resources()[0].content(),
        UiResourceContent::Lua(source) if source.as_str() == String::from_utf8_lossy(GLUE_LUA)
    ));
    let UiResourceContent::Xml(document) = bundle.resources()[1].content() else {
        return Err("second resource was not XML".into());
    };
    assert_eq!(document.root().name(), "Ui");
    assert_eq!(document.element_count(), 3);
    assert_eq!(document.root().attributes()[0].name(), "xmlns");
    assert_eq!(
        document.root().attributes()[0].value(),
        "http://www.blizzard.com/wow/ui/"
    );
    Ok(())
}

/// Unsupported entries fail where stock data names them instead of being searched elsewhere.
#[test]
fn manifest_rejects_non_source_entries() -> Result<(), Box<dyn Error>> {
    let fixture = ui_fixture(&[FixtureFile {
        path: "Interface\\GlueXML\\GlueXML.toc",
        bytes: b"## Interface: 30300\nGlueStrings.lua\nTextures\\Logo.blp\n",
    }])?;
    let mut store = mount(&fixture)?;

    let result = UiBundle::load(&mut store, UiManifestKind::Glue);

    assert!(matches!(
        result,
        Err(UiLoadError::ManifestEntry { line: 3, value, .. })
            if value == "Textures\\Logo.blp"
    ));
    Ok(())
}

/// Lua syntax is validated before the stock API registration stage.
#[test]
fn malformed_lua_fails_during_bundle_load() -> Result<(), Box<dyn Error>> {
    let fixture = ui_fixture(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Broken.lua\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Broken.lua",
            bytes: b"local = broken",
        },
    ])?;
    let mut store = mount(&fixture)?;

    let result = UiBundle::load(&mut store, UiManifestKind::Glue);

    assert!(matches!(result, Err(UiLoadError::Lua { .. })));
    Ok(())
}

fn ui_fixture(files: &[FixtureFile<'_>]) -> Result<Fixture, Box<dyn Error>> {
    Fixture::new(files)
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
