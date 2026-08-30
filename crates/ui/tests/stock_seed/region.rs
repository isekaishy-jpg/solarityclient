//! External stock-compatibility tests for typed region layout layers.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiBundle, UiDrawLayer, UiLayoutPlan, UiManifestKind, UiObjectCatalog,
    UiObjectError, UiObjectTree, UiPoint,
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

/// Layer wrappers retain stock draw order, schema defaults, and one shipped typo.
#[test]
fn object_layers_preserve_stock_draw_bands() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Layers.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Layers.xml",
            bytes: br#"<Ui><Frame name="Root"><Layers>
  <Layer level="BACKGROUND"><Texture name="$parentBackground"/></Layer>
  <Layer><FontString name="$parentLabel"/></Layer>
  <Layer level="OVERLAY`"><Texture name="$parentGlow"/></Layer>
</Layers></Frame></Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;

    assert_eq!(
        tree.node("RootBackground").and_then(last_draw_layer),
        Some(UiDrawLayer::Background)
    );
    assert_eq!(
        tree.node("RootLabel").and_then(last_draw_layer),
        Some(UiDrawLayer::Artwork)
    );
    assert_eq!(
        tree.node("RootGlow").and_then(last_draw_layer),
        Some(UiDrawLayer::Overlay)
    );
    Ok(())
}

/// Unknown draw bands fail instead of being folded into an arbitrary level.
#[test]
fn object_tree_rejects_unknown_draw_band() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Layers.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Layers.xml",
            bytes: br#"<Ui><Frame name="Root"><Layers><Layer level="FRONT">
  <Texture name="$parentInvalid"/>
</Layer></Layers></Frame></Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;

    let result = UiObjectTree::from_catalog(&objects, &fonts);

    assert!(matches!(result, Err(UiObjectError::Declaration { .. })));
    Ok(())
}

fn last_draw_layer(node: &solarity_ui::UiObjectNode<'_>) -> Option<UiDrawLayer> {
    node.layers()
        .iter()
        .filter_map(|layer| layer.draw_layer())
        .next_back()
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
