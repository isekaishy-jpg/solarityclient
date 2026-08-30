//! External stock-compatibility tests for typed XML script handlers.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiBundle, UiManifestKind, UiObjectCatalog, UiObjectTree, UiScriptError,
    UiScriptHandler, UiScriptPlan, UiScriptTarget,
};

use crate::support::{Fixture, FixtureFile};

/// Later XML layers replace or clear one callback slot without recompiling templates.
#[test]
fn script_plan_applies_stock_handler_replacement() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Scripts.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Scripts.xml",
            bytes: br#"<Ui>
  <Button name="ButtonTemplate" virtual="true"><Scripts>
    <OnLoad>self.loaded = true</OnLoad>
    <OnEvent>local payload = ...</OnEvent>
  </Scripts></Button>
  <Button name="LiveButton" inherits="ButtonTemplate"><Scripts>
    <OnLoad function="NamedLoad"/>
    <OnEvent/>
    <OnUpdate>self.elapsed = elapsed</OnUpdate>
  </Scripts></Button>
</Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;
    let scripts = UiScriptPlan::from_tree(&tree, bundle.lua())?;

    let live_index = tree.node_index("LiveButton").ok_or("missing LiveButton")?;
    let node = scripts.node(live_index).ok_or("missing script node")?;
    let bindings = scripts.bindings_for(node);
    assert_eq!(scripts.declaration_count(), 5);
    assert_eq!(scripts.function_count(), 3);
    assert_eq!(bindings.len(), 2);
    assert!(bindings.iter().any(|binding| {
        binding.handler() == UiScriptHandler::Load
            && binding.target() == &UiScriptTarget::Global("NamedLoad".to_owned())
    }));
    assert!(bindings.iter().any(|binding| {
        binding.handler() == UiScriptHandler::Update
            && matches!(binding.target(), UiScriptTarget::Compiled(_))
    }));
    assert!(
        !bindings
            .iter()
            .any(|binding| binding.handler() == UiScriptHandler::Event)
    );
    Ok(())
}

/// A callback unsupported by the concrete stock widget does not become generic.
#[test]
fn script_plan_rejects_handler_from_another_widget() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Scripts.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Scripts.xml",
            bytes: br#"<Ui><Frame name="Root"><Scripts>
  <OnColorSelect>self.changed = true</OnColorSelect>
</Scripts></Frame></Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;

    let result = UiScriptPlan::from_tree(&tree, bundle.lua());

    assert!(matches!(result, Err(UiScriptError::Handler { .. })));
    Ok(())
}

/// Handler bodies compile inside their exact callback parameter list.
#[test]
fn script_plan_rejects_varargs_in_non_vararg_handler() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Scripts.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Scripts.xml",
            bytes: br#"<Ui><Frame name="Root"><Scripts>
  <OnLoad>local payload = ...</OnLoad>
</Scripts></Frame></Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;

    let result = UiScriptPlan::from_tree(&tree, bundle.lua());

    assert!(matches!(result, Err(UiScriptError::Lua { .. })));
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
