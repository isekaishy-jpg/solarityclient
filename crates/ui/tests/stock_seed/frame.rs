//! External stock-compatibility tests for global UI object registration.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiBundle, UiFrameError, UiFramePlan, UiFrameStrata, UiInheritanceTarget,
    UiManifestKind, UiObjectCatalog, UiObjectError, UiObjectKind, UiObjectTree,
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
  <Frame name="BaseFrame" virtual="true"><Frames><Frame name="$parentChild"/></Frames></Frame>
  <Texture name="LogoTexture" virtual="true"/>
  <Frame name="GlueParent"/>
  <Button name="LoginButton" inherits="BaseFrame" parent="GlueParent"/>
</Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;

    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;
    let login = objects
        .definition("LoginButton")
        .ok_or("missing LoginButton")?;

    assert_eq!(objects.definitions().len(), 4);
    assert_eq!(objects.templates().len(), 2);
    assert_eq!(objects.roots().len(), 2);
    assert_eq!(login.kind(), UiObjectKind::Button);
    assert_eq!(login.parent_name(), Some("GlueParent"));
    assert_eq!(login.inherited_from(), &[UiInheritanceTarget::Object(0)]);
    assert_eq!(login.element().name(), "Button");
    let login_node = tree.node("LoginButton").ok_or("missing live LoginButton")?;
    let child_node = tree
        .node("LoginButtonChild")
        .ok_or("missing inherited LoginButtonChild")?;
    assert_eq!(tree.top_level().len(), 1);
    assert_eq!(login_node.parent(), Some(0));
    assert_eq!(child_node.parent(), Some(1));
    assert_eq!(login_node.layers().len(), 2);
    Ok(())
}

/// Frame ordering and interaction flags retain exact inheritance-layer order.
#[test]
fn frame_plan_preserves_stock_properties() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Frames.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Frames.xml",
            bytes: br#"<Ui>
  <Frame name="Base" virtual="true" frameStrata="LOW" frameLevel="3" id="7"
         toplevel="true" movable="true" resizable="true" clampedToScreen="true"
         enableKeyboard="true" enableMouse="false" protected="true"
         dontSavePosition="true"/>
  <Button name="Login" inherits="Base" frameStrata="DIALOG" frameLevel="11"
          enableMouse="true"/>
</Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;
    let plan = UiFramePlan::from_tree(&tree)?;
    let index = tree
        .nodes()
        .iter()
        .position(|node| node.name() == Some("Login"))
        .ok_or("missing Login")?;
    let node = plan.node(index).ok_or("missing Login frame node")?;
    let layers = plan.layers_for(node);

    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0].strata(), Some(UiFrameStrata::Low));
    assert_eq!(layers[0].level(), Some(3));
    assert_eq!(layers[0].id(), Some(7));
    assert_eq!(layers[0].top_level(), Some(true));
    assert_eq!(layers[0].movable(), Some(true));
    assert_eq!(layers[0].resizable(), Some(true));
    assert_eq!(layers[0].clamped_to_screen(), Some(true));
    assert_eq!(layers[0].keyboard_enabled(), Some(true));
    assert_eq!(layers[0].mouse_enabled(), Some(false));
    assert_eq!(layers[0].protected(), Some(true));
    assert_eq!(layers[0].position_persistence_disabled(), Some(true));
    assert_eq!(layers[1].strata(), Some(UiFrameStrata::Dialog));
    assert_eq!(layers[1].level(), Some(11));
    assert_eq!(layers[1].mouse_enabled(), Some(true));
    let states = plan.resolve(&tree)?;
    let state = states.state(index).ok_or("missing Login frame state")?;
    assert_eq!(state.strata(), UiFrameStrata::Dialog);
    assert_eq!(state.level(), 11);
    assert_eq!(state.id(), 7);
    assert!(state.top_level());
    assert!(state.movable());
    assert!(state.resizable());
    assert!(state.clamped_to_screen());
    assert!(state.keyboard_enabled());
    assert!(state.mouse_enabled());
    assert!(state.protected());
    assert!(state.position_persistence_disabled());
    Ok(())
}

/// Parenting establishes stratum and level before XML overrides are applied.
#[test]
fn frame_state_resolves_deferred_parent_order() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Frames.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Frames.xml",
            bytes: br#"<Ui>
  <Frame name="Child" parent="LaterParent"/>
  <Frame name="LaterParent" frameStrata="HIGH" frameLevel="20"/>
  <Frame name="Unparented"/>
</Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;
    let plan = UiFramePlan::from_tree(&tree)?;
    let states = plan.resolve(&tree)?;
    let child_index = tree
        .nodes()
        .iter()
        .position(|node| node.name() == Some("Child"))
        .ok_or("missing Child")?;
    let unparented_index = tree
        .nodes()
        .iter()
        .position(|node| node.name() == Some("Unparented"))
        .ok_or("missing Unparented")?;
    let child = states.state(child_index).ok_or("missing Child state")?;
    let unparented = states
        .state(unparented_index)
        .ok_or("missing Unparented state")?;

    assert_eq!(child.strata(), UiFrameStrata::High);
    assert_eq!(child.level(), 21);
    assert_eq!(unparented.strata(), UiFrameStrata::Medium);
    assert_eq!(unparented.level(), 0);
    assert!(!unparented.mouse_enabled());
    Ok(())
}

/// Unknown frame strata fail without being assigned a nearby render band.
#[test]
fn frame_plan_rejects_unknown_strata() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Frames.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Frames.xml",
            bytes: br#"<Ui><Frame name="Root" frameStrata="FRONT"/></Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;

    let result = UiFramePlan::from_tree(&tree);

    assert!(matches!(result, Err(UiFrameError::Property { .. })));
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

/// Distinct children may reuse a global name; the later registration shadows it.
#[test]
fn nested_global_name_shadowing_preserves_both_instances() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Objects.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Objects.xml",
            bytes: br#"<Ui>
  <Frame name="First"><Layers><Layer><FontString name="Shared"/></Layer></Layers></Frame>
  <Frame name="Second"><Layers><Layer><FontString name="Shared"/></Layer></Layers></Frame>
</Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;

    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;

    assert_eq!(tree.nodes().len(), 4);
    assert_eq!(tree.nodes()[0].children(), &[1]);
    assert_eq!(tree.nodes()[2].children(), &[3]);
    assert_eq!(tree.node("Shared").and_then(|node| node.parent()), Some(2));
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
