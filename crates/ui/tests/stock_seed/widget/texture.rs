//! External stock-compatibility tests for typed texture declarations.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiBlendMode, UiBundle, UiManifestKind, UiObjectCatalog, UiObjectRole,
    UiObjectTree, UiTextureError, UiTextureFile, UiTexturePlan,
};

use crate::support::{Fixture, FixtureFile};

/// Extensionless and legacy TGA names select their stock BLP archive assets.
#[test]
fn texture_plan_canonicalizes_stock_file_names() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Textures.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Textures.xml",
            bytes: br#"<Ui>
  <Frame name="Root">
    <Layers><Layer>
      <Texture name="$parentLogo" file="Interface\Glues\Logo" alphaMode="ADD">
        <TexCoords left="0.1" right="0.9"/>
      </Texture>
      <Texture name="$parentDynamic" file=""/>
    </Layer></Layers>
    <Frames>
      <Button name="$parentButton">
        <PushedTexture file="Interface\Buttons\Push.tga"/>
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
    let plan = UiTexturePlan::from_tree(&tree)?;

    let logo_index = tree
        .nodes()
        .iter()
        .position(|node| node.name() == Some("RootLogo"))
        .ok_or("missing RootLogo")?;
    let logo_node = plan.node(logo_index).ok_or("missing logo plan node")?;
    let logo = plan
        .layers_for(logo_node)
        .first()
        .ok_or("missing logo layer")?;
    assert_eq!(
        logo.file(),
        Some(&UiTextureFile::Asset(AssetPath::new(
            "Interface\\Glues\\Logo.blp"
        )?))
    );
    assert_eq!(logo.blend_mode(), Some(UiBlendMode::Add));
    let coords = logo.tex_coords().ok_or("missing texture coordinates")?;
    assert_eq!(coords.left(), Some(0.1));
    assert_eq!(coords.right(), Some(0.9));
    assert_eq!(coords.top(), None);
    assert_eq!(coords.bottom(), None);

    let dynamic_index = tree
        .nodes()
        .iter()
        .position(|node| node.name() == Some("RootDynamic"))
        .ok_or("missing RootDynamic")?;
    let dynamic_node = plan
        .node(dynamic_index)
        .ok_or("missing dynamic plan node")?;
    assert_eq!(
        plan.layers_for(dynamic_node)[0].file(),
        Some(&UiTextureFile::Dynamic)
    );

    let pushed_index = tree
        .nodes()
        .iter()
        .position(|node| node.role() == UiObjectRole::PushedTexture)
        .ok_or("missing pushed texture")?;
    let pushed_node = plan.node(pushed_index).ok_or("missing pushed plan node")?;
    assert_eq!(
        plan.layers_for(pushed_node)[0].file(),
        Some(&UiTextureFile::Asset(AssetPath::new(
            "Interface\\Buttons\\Push.blp"
        )?))
    );
    Ok(())
}

/// Unobserved image extensions fail rather than creating a lookup fallback.
#[test]
fn texture_plan_rejects_non_stock_extension() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Textures.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Textures.xml",
            bytes: br#"<Ui><Texture name="Logo" file="Interface\Glues\Logo.png"/></Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;

    let result = UiTexturePlan::from_tree(&tree);

    assert!(matches!(result, Err(UiTextureError::Texture { .. })));
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
