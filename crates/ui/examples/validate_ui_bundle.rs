//! Validates one built-in UI bundle against an installed stock client.

use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;

use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, FontRasterization, FontSystem, UiBundle, UiLayoutPlan, UiManifestKind,
    UiObjectCatalog, UiObjectTree, UiResourceContent,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os();
    let _executable = arguments.next();
    let data_root = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| argument_error("missing client Data directory"))?;
    let locale = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| argument_error("missing ASCII locale such as enUS"))?
        .parse::<Locale>()?;
    let kind = match arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .as_deref()
    {
        Some("glue") => UiManifestKind::Glue,
        Some("frame") => UiManifestKind::Frame,
        _ => return Err(argument_error("bundle must be glue or frame").into()),
    };
    if arguments.next().is_some() {
        return Err(argument_error("unexpected extra arguments").into());
    }

    let root = ClientDataRoot::new(data_root)?;
    let catalog = ArchiveCatalog::discover(root, locale)?;
    let archive_count = catalog.descriptors().len();
    let mut store = AssetStore::mount(catalog)?;
    let bundle = UiBundle::load(&mut store, kind)?;
    let xml_count = bundle
        .resources()
        .iter()
        .filter(|resource| matches!(resource.content(), UiResourceContent::Xml(_)))
        .count();
    let lua_count = bundle.resources().len() - xml_count;
    let font_catalog = FontCatalog::from_bundle(&bundle)?;
    let object_catalog = UiObjectCatalog::from_bundle(&bundle, &font_catalog)?;
    let object_tree = UiObjectTree::from_catalog(&object_catalog, &font_catalog)?;
    let layout_plan = UiLayoutPlan::from_tree(&object_tree)?;
    let named_object_count = object_tree
        .nodes()
        .iter()
        .filter(|node| node.name().is_some())
        .count();
    let font_path = AssetPath::new("Fonts\\FRIZQT__.TTF")?;
    let mut fonts = FontSystem::new()?;
    let glyph = fonts.rasterize(
        &mut store,
        &font_path,
        16,
        'A',
        FontRasterization::Antialiased,
    )?;

    println!(
        "validated {:?}: {archive_count} archives, {} resources ({xml_count} XML, {lua_count} Lua), {} ordered actions, {} fonts, {} templates, {} live roots, {} instantiated objects ({named_object_count} named, {} top-level), {} layout layers and {} anchors, FRIZQT__ 'A' {}x{}",
        bundle.manifest().kind(),
        bundle.resources().len(),
        bundle.actions().len(),
        font_catalog.definitions().len(),
        object_catalog.templates().len(),
        object_catalog.roots().len(),
        object_tree.nodes().len(),
        object_tree.top_level().len(),
        layout_plan.layer_count(),
        layout_plan.anchor_count(),
        glyph.width(),
        glyph.height()
    );
    Ok(())
}

fn argument_error(message: &str) -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        format!("{message}; usage: validate_ui_bundle <Data> <locale> <glue|frame>"),
    )
}
