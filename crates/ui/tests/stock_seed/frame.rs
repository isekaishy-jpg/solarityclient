//! External stock-compatibility tests for global UI object registration.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiBundle, UiInheritanceTarget, UiManifestKind, UiObjectCatalog, UiObjectError,
    UiObjectKind,
};

use crate::support::{Fixture, FixtureFile};

/// Virtual templates and live roots retain exact expanded declaration order.
#[test]
fn object_catalog_resolves_earlier_virtual_templates() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Objects.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Objects.xml",
            bytes: br#"<Ui>
  <Frame name="BaseFrame" virtual="true"/>
  <Texture name="LogoTexture" virtual="true"/>
  <Button name="LoginButton" inherits="BaseFrame" parent="GlueParent"/>
</Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;

    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let login = objects
        .definition("LoginButton")
        .ok_or("missing LoginButton")?;

    assert_eq!(objects.definitions().len(), 3);
    assert_eq!(objects.templates().len(), 2);
    assert_eq!(objects.roots().len(), 1);
    assert_eq!(login.kind(), UiObjectKind::Button);
    assert_eq!(login.parent_name(), Some("GlueParent"));
    assert_eq!(login.inherited_from(), &[UiInheritanceTarget::Object(0)]);
    assert_eq!(login.element().name(), "Button");
    Ok(())
}

/// Template inheritance never searches forward through later declarations.
#[test]
fn object_catalog_rejects_forward_template_lookup() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Objects.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Objects.xml",
            bytes: br#"<Ui>
  <Button name="LoginButton" inherits="LaterTemplate"/>
  <Button name="LaterTemplate" virtual="true"/>
</Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;

    let result = UiObjectCatalog::from_bundle(&bundle, &fonts);

    assert!(matches!(result, Err(UiObjectError::Declaration { .. })));
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
