//! External stock-compatibility tests for typed region layout layers.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiAnchorTarget, UiBundle, UiDrawLayer, UiLayoutError, UiLayoutPlan,
    UiManifestKind, UiObjectCatalog, UiObjectError, UiObjectTree, UiPoint, UiRegionStatePlan,
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

/// Shipped GlueXML commonly writes offsets on the Anchor itself.
#[test]
fn layout_plan_preserves_compact_anchor_offsets() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Layout.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Layout.xml",
            bytes: br#"<Ui><Frame name="Root" setAllPoints="true"><Frames>
  <Frame name="Compact"><Size x="20" y="10"/><Anchors>
    <Anchor point="TOPLEFT" relativeTo="Root" x="9" y="-5"/>
  </Anchors></Frame>
  <Frame name="SingleAxis"><Size x="20" y="10"/><Anchors>
    <Anchor point="BOTTOM" relativeTo="Root" y="12"/>
  </Anchors></Frame>
</Frames></Frame></Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;
    let layout = UiLayoutPlan::from_tree(&tree)?;

    let compact = tree.node_index("Compact").ok_or("missing Compact")?;
    let compact_layer = layout
        .layers_for(layout.node(compact).ok_or("missing Compact layout")?)
        .last()
        .ok_or("missing Compact layout layer")?;
    assert_eq!(
        layout.anchors_for(*compact_layer)[0].offset(),
        Some((9.0, -5.0))
    );

    let single_axis = tree.node_index("SingleAxis").ok_or("missing SingleAxis")?;
    let single_axis_layer = layout
        .layers_for(
            layout
                .node(single_axis)
                .ok_or("missing SingleAxis layout")?,
        )
        .last()
        .ok_or("missing SingleAxis layout layer")?;
    assert_eq!(
        layout.anchors_for(*single_axis_layer)[0].offset(),
        Some((0.0, 12.0))
    );
    Ok(())
}

/// Startup resolution replaces anchors by point and inherits effective state.
#[test]
fn region_state_resolves_stock_layout_application_order() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Layout.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Layout.xml",
            bytes: br#"<Ui>
  <Button name="ButtonTemplate" virtual="true">
    <Size x="100" y="20"/>
    <Anchors><Anchor point="TOPLEFT"><Offset x="1" y="2"/></Anchor></Anchors>
  </Button>
  <Frame name="Root" setAllPoints="true" hidden="true" alpha="0.5" scale="0.5">
    <Frames>
      <Button name="$parentChild" inherits="ButtonTemplate" hidden="false" alpha="0.5" scale="0.5">
        <Size x="120"/>
        <Anchors>
          <Anchor point="TOPLEFT" relativeTo="Root"><Offset x="5" y="6"/></Anchor>
          <Anchor point="BOTTOMRIGHT" relativeTo="Root"/>
        </Anchors>
      </Button>
      <Frame name="$parentAnchorWins" setAllPoints="true">
        <Anchors><Anchor point="CENTER" relativeTo="" relativePoint=""/></Anchors>
      </Frame>
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
    let layout = UiLayoutPlan::from_tree(&tree)?;
    let states = UiRegionStatePlan::resolve(&tree, &layout)?;

    let root_index = tree.node_index("Root").ok_or("missing Root")?;
    let root = states.state(root_index).ok_or("missing Root state")?;
    assert!(!root.shown());
    assert_eq!(states.anchors_for(root).len(), 2);
    assert!(
        states
            .anchors_for(root)
            .iter()
            .all(|anchor| anchor.target() == UiAnchorTarget::Screen)
    );

    let child_index = tree.node_index("RootChild").ok_or("missing child")?;
    let child = states.state(child_index).ok_or("missing child state")?;
    assert_eq!((child.width(), child.height()), (120.0, 20.0));
    assert!(child.shown());
    assert!(!child.effectively_shown());
    assert_eq!((child.alpha(), child.effective_alpha()), (0.5, 0.25));
    assert_eq!((child.scale(), child.effective_scale()), (0.5, 0.25));
    let child_anchors = states.anchors_for(child);
    assert_eq!(child_anchors.len(), 2);
    assert_eq!(child_anchors[0].point(), UiPoint::TopLeft);
    assert_eq!(
        child_anchors[0].target(),
        UiAnchorTarget::Object(root_index)
    );
    assert_eq!(child_anchors[0].relative_point(), UiPoint::TopLeft);
    assert_eq!(child_anchors[0].offset(), (5.0, 6.0));
    assert_eq!(child_anchors[1].point(), UiPoint::BottomRight);

    let anchor_wins_index = tree
        .node_index("RootAnchorWins")
        .ok_or("missing anchor-wins frame")?;
    let anchor_wins = states
        .state(anchor_wins_index)
        .ok_or("missing anchor-wins state")?;
    let anchors = states.anchors_for(anchor_wins);
    assert_eq!(anchors.len(), 1);
    assert_eq!(anchors[0].point(), UiPoint::Center);
    assert_eq!(anchors[0].target(), UiAnchorTarget::Object(root_index));
    assert_eq!(anchors[0].relative_point(), UiPoint::Center);
    Ok(())
}

/// Explicit anchor names must resolve exactly; no alternate owner is guessed.
#[test]
fn region_state_rejects_missing_anchor_target() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Layout.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Layout.xml",
            bytes: br#"<Ui><Frame name="Root"><Anchors>
  <Anchor point="CENTER" relativeTo="Unavailable"/>
</Anchors></Frame></Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;
    let layout = UiLayoutPlan::from_tree(&tree)?;

    let result = UiRegionStatePlan::resolve(&tree, &layout);

    assert!(matches!(result, Err(UiLayoutError::Resolution { .. })));
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
