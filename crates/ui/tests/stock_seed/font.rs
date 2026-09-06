//! External stock-compatibility tests for the archive-backed font boundary.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetError, AssetPath, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, FontError, FontOutline, FontRasterization, FontSystem, HorizontalJustification,
    UiBundle, UiManifestKind,
};

use crate::support::{Fixture, FixtureFile};

/// Invalid archive bytes fail at the selected face instead of reaching an OS font.
#[test]
fn invalid_stock_font_has_no_system_fallback() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[FixtureFile {
        path: "Fonts\\Broken.ttf",
        bytes: b"not a TrueType font",
    }])?;
    let mut store = mount(&fixture)?;
    let path = AssetPath::new("Fonts/Broken.ttf")?;
    let mut fonts = FontSystem::new()?;

    let result = fonts.rasterize(&mut store, &path, 16, 'A', FontRasterization::Antialiased);

    assert!(matches!(result, Err(FontError::Face { path: failed, .. }) if failed == path));
    assert_eq!(fonts.loaded_face_count(), 0);
    Ok(())
}

/// A missing stock font remains an asset error with no platform lookup path.
#[test]
fn missing_stock_font_remains_missing() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[])?;
    let mut store = mount(&fixture)?;
    let path = AssetPath::new("Fonts/Missing.ttf")?;
    let mut fonts = FontSystem::new()?;

    let result = fonts.rasterize(&mut store, &path, 16, 'A', FontRasterization::Antialiased);

    assert!(matches!(
        result,
        Err(FontError::Asset(AssetError::AssetNotFound { path: failed })) if failed == path
    ));
    Ok(())
}

/// Root font objects inherit only from definitions already constructed in load order.
#[test]
fn font_catalog_applies_ordered_stock_inheritance() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Fonts.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Fonts.xml",
            bytes: br#"<Ui>
  <Font name="Base" font="Fonts\FRIZQT__.TTF" monochrome="false" virtual="true">
    <Shadow><Offset><AbsDimension x="1" y="-1"/></Offset><Color r="0" g="0" b="0"/></Shadow>
    <FontHeight><AbsValue val="12"/></FontHeight>
    <Color r="1" g="0.82" b="0"/>
  </Font>
  <Font name="Child" inherits="Base" outline="THICK" spacing="1.5" justifyH="LEFT">
    <Color r="1" g="1" b="1" a="0.75"/>
  </Font>
</Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;

    let catalog = FontCatalog::from_bundle(&bundle)?;
    let child = catalog.definition("Child").ok_or("missing Child font")?;

    assert_eq!(catalog.definitions().len(), 2);
    assert_eq!(child.inherited_from(), &["Base"]);
    assert_eq!(
        child.face().map(AssetPath::as_str),
        Some("FONTS\\FRIZQT__.TTF")
    );
    assert_eq!(child.height(), Some(12.0));
    assert_eq!(child.outline(), Some(FontOutline::Thick));
    assert_eq!(child.spacing(), Some(1.5));
    assert_eq!(
        child.horizontal_justification(),
        Some(HorizontalJustification::Left)
    );
    assert_eq!(child.color().and_then(|color| color.alpha()), Some(0.75));
    assert_eq!(
        child.shadow().and_then(|shadow| shadow.offset()),
        Some((1.0, -1.0))
    );
    Ok(())
}

/// Inheritance never searches later files for an unavailable parent.
#[test]
fn font_catalog_rejects_forward_inheritance() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Fonts.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Fonts.xml",
            bytes: br#"<Ui><Font name="Child" inherits="Later"/><Font name="Later"/></Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;

    let result = FontCatalog::from_bundle(&bundle);

    assert!(matches!(result, Err(FontError::Definition { .. })));
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}

/// Zero dimensions restore the native automatic extent before the next update.
/// A face-less font uses the native one-pixel minimum; the stock FrameXML
/// lifecycle validator additionally covers actual General/Combat Log glyphs.
#[test]
fn font_string_zero_dimensions_restore_immediate_automatic_extents() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"AutoSize.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\AutoSize.xml",
            bytes: br#"<Ui>
<Font name="SizeFont" virtual="true"><FontHeight><AbsValue val="12"/></FontHeight></Font>
<Frame name="Owner"><Size x="200" y="100"/><Anchors><Anchor point="CENTER"/></Anchors>
<Layers><Layer><FontString name="Label" inherits="SizeFont" text="General">
<Size x="50" y="8"/><Anchors><Anchor point="LEFT"/></Anchors>
</FontString></Layer></Layers></Frame>
</Ui>"#,
        },
    ])?;
    let mut manager = solarity_ui::GlueManager::start(mount(&fixture)?, (1024, 768), false)?;
    manager
        .bundle()
        .lua()
        .load(
            r#"
        assert(Label:GetWidth() == 50 and Label:GetHeight() == 8)
        Label:SetWidth(0)
        assert(Label:GetWidth() == 1 and Label:GetHeight() == 8)
        Label:SetHeight(0)
        assert(Label:GetHeight() == 1)
        Label:SetSize(50, 8)
        assert(Label:GetWidth() == 50 and Label:GetHeight() == 8)
        Label:SetSize(0, 0)
        assert(Label:GetWidth() == 1 and Label:GetHeight() == 1)
        Label:SetText("")
        assert(Label:GetWidth() == 0 and Label:GetHeight() == 0)
        Label:SetText("Combat Log")
        assert(Label:GetWidth() == 1 and Label:GetHeight() == 1)
        Owner:SetSize(0, 0)
        assert(Owner:GetWidth() == 0 and Owner:GetHeight() == 0)
    "#,
        )
        .exec()?;
    manager.update(0.01)?;
    manager
        .bundle()
        .lua()
        .load("assert(Label:GetWidth() == 1 and Label:GetHeight() == 1); Label:SetWidth(0); Label:SetHeight(0); Label:SetSize(0, 0)")
        .exec()?;
    assert!(
        !manager.update(0.01)?,
        "unchanged automatic extents rebuilt presentation"
    );
    Ok(())
}
