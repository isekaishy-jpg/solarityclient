//! External stock-compatibility tests for typed region layout layers.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiBundle, UiLayoutPlan, UiManifestKind, UiObjectCatalog, UiObjectTree, UiPoint,
};

use crate::support::{Fixture, FixtureFile};

/// Absolute wrapper and direct dimensions remain ordered without guessed defaults.
#[test]
fn layout_plan_preserves_inherited_geometry_layers() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Layout.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Layout.xml",
            bytes: br#"<Ui>
  <Button name="ButtonTemplate" virtual="true">
    <Size><AbsDimension x="100" y="20"/></Size>
    <Anchors><Anchor point="CENTER"/></Anchors>
  </Button>
  <Frame name="Root">
    <Frames>
      <Button name="$parentChild" inherits="ButtonTemplate" hidden="true">
        <Size x="120" y="24"/>
        <Anchors>
          <Anchor point="TOPLEFT" relativeTo="$parent" relativePoint="BOTTOMLEFT">
            <Offset x="5" y="-3"/>
          </Anchor>
        </Anchors>
      </Button>
    </Frames>
  </Frame>
</Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;

    let plan = UiLayoutPlan::from_tree(&tree)?;
    let child_index = tree
        .nodes()
        .iter()
        .position(|node| node.name() == Some("RootChild"))
        .ok_or("missing RootChild")?;
    let node = plan.node(child_index).ok_or("missing child layout")?;
    let layers = plan.layers_for(node);

    assert_eq!(layers.len(), 2);
    assert_eq!(
        layers[0].dimensions().and_then(|size| size.width()),
        Some(100.0)
    );
    assert_eq!(
        layers[1].dimensions().and_then(|size| size.height()),
        Some(24.0)
    );
    assert_eq!(layers[1].hidden(), Some(true));
    assert_eq!(plan.anchors_for(layers[0])[0].point(), UiPoint::Center);
    let concrete_anchor = &plan.anchors_for(layers[1])[0];
    assert_eq!(concrete_anchor.relative_to(), Some("Root"));
    assert_eq!(concrete_anchor.relative_point(), Some(UiPoint::BottomLeft));
    assert_eq!(concrete_anchor.offset(), Some((5.0, -3.0)));
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
