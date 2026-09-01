//! External stock-compatibility tests for the native minimap widget.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiAnimationPlan, UiBundle, UiFramePlan, UiLayoutPlan, UiManifestKind,
    UiObjectCatalog, UiObjectTree, UiRegionStatePlan, UiRuntimeTemplatePlan, UiScriptEnvironment,
    UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan, UiTexturePlan, UiTextureStatePlan,
    UiTrackingCategory, UiTrackingType,
};

use crate::support::{Fixture, FixtureFile};

/// Stock minimap zoom and player-arrow geometry survive XML initialization.
#[test]
fn minimap_retains_native_presentation_properties() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\FrameXML\\FrameXML.toc",
            bytes: b"Minimap.xml\n",
        },
        FixtureFile {
            path: "Interface\\FrameXML\\Minimap.xml",
            bytes: br#"<Ui><Minimap name="Minimap"><Scripts><OnLoad>
  self:SetPlayerTextureHeight(40)
  self:SetPlayerTextureWidth(36)
  self:SetZoom(5)
  self:PingLocation(-.25, .75)
  TRACKING_COUNT = GetNumTrackingTypes()
  TRACKING_NAME, TRACKING_TEXTURE, TRACKING_ACTIVE, TRACKING_CATEGORY = GetTrackingInfo(2)
</OnLoad></Scripts></Minimap></Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Frame)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;
    let animations = UiAnimationPlan::from_tree(&tree)?;
    let frames = UiFramePlan::from_tree(&tree)?.resolve(&tree)?;
    let layout = UiLayoutPlan::from_tree(&tree)?;
    let regions = UiRegionStatePlan::resolve(&tree, &layout)?;
    let scripts = UiScriptPlan::from_tree(&tree, bundle.lua())?;
    let templates = UiRuntimeTemplatePlan::from_catalog(&objects, &fonts, bundle.lua())?;
    let textures = UiTexturePlan::from_tree(&tree)?;
    let texture_states = UiTextureStatePlan::resolve(&tree, &textures)?;
    let runtime_plan = UiScriptRuntimePlan::new(
        &tree,
        &animations,
        &frames,
        &regions,
        &templates,
        &fonts,
        &texture_states,
    );
    let environment = UiScriptEnvironment::new(1024, 768, false)?;
    let tracking = environment.minimap_tracking_state();
    tracking.replace_types(vec![
        UiTrackingType::new(
            "Find Minerals",
            "Interface\\Icons\\Spell_Nature_Earthquake",
            UiTrackingCategory::Spell,
        )?,
        UiTrackingType::new(
            "Repair",
            "Interface\\Minimap\\Tracking\\Repair",
            UiTrackingCategory::Area,
        )?,
    ]);
    tracking.select(Some(2))?;
    let mut runtime = UiScriptRuntime::new(&bundle, &runtime_plan, environment)?;
    runtime.execute_all(&bundle, &tree, &scripts)?;

    bundle
        .lua()
        .load(
            r#"assert(Minimap:GetPlayerTextureHeight() == 40)
assert(Minimap:GetPlayerTextureWidth() == 36)
assert(Minimap:GetZoomLevels() == 6)
assert(Minimap:GetZoom() == 5)
assert(TRACKING_COUNT == 2)
assert(TRACKING_NAME == "Repair")
assert(TRACKING_TEXTURE == "Interface\\Minimap\\Tracking\\Repair")
assert(TRACKING_ACTIVE)
assert(TRACKING_CATEGORY == "area")
assert(GetTrackingTexture() == TRACKING_TEXTURE)
SetTracking(nil)
assert(GetTrackingTexture() == nil)
local ok = pcall(Minimap.SetZoom, Minimap, 6)
assert(not ok)"#,
        )
        .exec()?;
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
