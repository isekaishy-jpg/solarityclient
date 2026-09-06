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
  INITIAL_ZOOM = self:GetZoom()
  self:SetPlayerTextureHeight(40)
  self:SetPlayerTextureWidth(36)
  self:SetZoom(5)
  self:PingLocation(-.25, .75)
  TRACKING_COUNT = GetNumTrackingTypes()
  TRACKING_NAME, TRACKING_TEXTURE, TRACKING_ACTIVE, TRACKING_CATEGORY = GetTrackingInfo(2)
</OnLoad></Scripts></Minimap><Minimap name="SecondMinimap"/></Ui>"#,
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
    let minimap = environment.minimap_state();
    let cvar_observer = environment.clone();
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
            r#"assert(Minimap.GetPlayerTextureHeight == nil)
assert(Minimap.GetPlayerTextureWidth == nil)
assert(Minimap:GetZoomLevels() == 6)
assert(INITIAL_ZOOM == 3)
assert(GetCVarDefault("minimapZoom") == "3")
assert(GetCVarDefault("minimapInsideZoom") == "3")
assert(GetCVarMin("minimapZoom") == nil and GetCVarMax("minimapZoom") == nil)
assert(Minimap:GetZoom() == 5)
assert(SecondMinimap:GetZoom() == 5)
assert(GetCVar("minimapZoom") == "5")
assert(TRACKING_COUNT == 2)
assert(TRACKING_NAME == "Repair")
assert(TRACKING_TEXTURE == "Interface\\Minimap\\Tracking\\Repair")
assert(TRACKING_ACTIVE)
assert(TRACKING_CATEGORY == "area")
assert(GetTrackingTexture() == TRACKING_TEXTURE)
SetTracking(nil)
assert(GetTrackingTexture() == nil)
Minimap:SetZoom(6)
assert(Minimap:GetZoom() == 5)
Minimap:SetZoom(2.9)
assert(SecondMinimap:GetZoom() == 2)
assert(GetCVar("minimapZoom") == "2")
Minimap:SetZoom(-1)
assert(Minimap:GetZoom() == 5)
Minimap:SetZoom(-.9)
assert(Minimap:GetZoom() == 0)
Minimap:SetZoom(4294967298)
assert(Minimap:GetZoom() == 2)
Minimap:SetZoom(0/0)
assert(Minimap:GetZoom() == 0)
Minimap:SetZoom(math.huge)
assert(Minimap:GetZoom() == 0)
Minimap:SetZoom("4.8")
assert(Minimap:GetZoom() == 4)
assert(not pcall(Minimap.SetZoom, Minimap, "invalid"))
SetCVar("minimapZoom", "1")
assert(Minimap:GetZoom() == 4)
Minimap:SetZoom(4)
assert(GetCVar("minimapZoom") == "1")"#,
        )
        .exec()?;
    assert_eq!(minimap.zoom(), 4);
    assert!((minimap.radius() - 100.0).abs() < 0.001);
    let revision = minimap.revision();
    assert!(minimap.set_indoors(true));
    assert_eq!(minimap.revision(), revision + 1);
    assert!(!minimap.set_indoors(true));
    assert_eq!(minimap.revision(), revision + 1);
    assert_eq!(minimap.zoom(), 3);
    assert_eq!(minimap.radius(), 60.0);
    bundle
        .lua()
        .load(
            r#"
assert(Minimap:GetZoom() == 3 and SecondMinimap:GetZoom() == 3)
SecondMinimap:SetZoom(1)
assert(Minimap:GetZoom() == 1)
assert(GetCVar("minimapInsideZoom") == "1")
assert(GetCVar("minimapZoom") == "1")
"#,
        )
        .exec()?;
    assert_eq!(minimap.radius(), 120.0);
    assert_eq!(
        cvar_observer.cvar_value("minimapInsideZoom").as_deref(),
        Some("1")
    );
    assert!(minimap.set_indoors(false));
    assert_eq!(minimap.zoom(), 4);
    bundle
        .lua()
        .load("assert(Minimap:GetZoom() == 4 and SecondMinimap:GetZoom() == 4)")
        .exec()?;
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}

/// Saved settings initialize both modes before OnLoad; mode transitions publish
/// their selected zoom before the stock event and only runtime changes persist.
#[test]
fn minimap_scene_restores_settings_and_dispatches_mode_changes() -> Result<(), Box<dyn Error>> {
    use solarity_asset::AssetStoreHandle;
    use solarity_ui::{AddonCatalog, FrameManager};
    let dbc = |records: u32, fields: u32| {
        let mut bytes = b"WDBC".to_vec();
        for value in [records, fields, fields * 4, 1] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.resize(21 + records as usize * fields as usize * 4, 0);
        bytes
    };
    let base = dbc(11, 1);
    let coefficients = dbc(1100, 1);
    let slots = dbc(0, 3);
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient/gtChanceToMeleeCritBase.dbc",
            bytes: &base,
        },
        FixtureFile {
            path: "DBFilesClient/gtChanceToSpellCritBase.dbc",
            bytes: &base,
        },
        FixtureFile {
            path: "DBFilesClient/gtChanceToMeleeCrit.dbc",
            bytes: &coefficients,
        },
        FixtureFile {
            path: "DBFilesClient/gtChanceToSpellCrit.dbc",
            bytes: &coefficients,
        },
        FixtureFile {
            path: "DBFilesClient/gtOCTRegenHP.dbc",
            bytes: &coefficients,
        },
        FixtureFile {
            path: "DBFilesClient/gtRegenHPPerSpt.dbc",
            bytes: &coefficients,
        },
        FixtureFile {
            path: "DBFilesClient/gtRegenMPPerSpt.dbc",
            bytes: &coefficients,
        },
        FixtureFile {
            path: "DBFilesClient/PaperDollItemFrame.dbc",
            bytes: &slots,
        },
        FixtureFile {
            path: "Interface/FrameXML/FrameXML.toc",
            bytes: b"Minimap.xml\n",
        },
        FixtureFile {
            path: "Interface/FrameXML/Minimap.xml",
            bytes: br#"<Ui><Minimap name="Minimap"><Scripts>
<OnLoad>
assert(self:GetZoom() == 2)
assert(GetCVar("minimapZoom") == "2" and GetCVar("minimapInsideZoom") == "5")
self:RegisterEvent("MINIMAP_UPDATE_ZOOM")
ZOOM_EVENT_COUNT = "0"
</OnLoad>
<OnEvent>
assert(event == "MINIMAP_UPDATE_ZOOM")
ZOOM_EVENT_COUNT = tostring(tonumber(ZOOM_EVENT_COUNT) + 1)
EVENT_ZOOM = tostring(self:GetZoom())
</OnEvent></Scripts></Minimap></Ui>"#,
        },
        FixtureFile {
            path: "Interface/FrameXML/Bindings.xml",
            bytes: br#"<Bindings><Binding name="ZOOM">Minimap:SetZoom(4)</Binding></Bindings>"#,
        },
        FixtureFile {
            path: "WTF/DefaultBindings.wtf",
            bytes: b"",
        },
    ])?;
    let mut manager = FrameManager::start_shared(
        AssetStoreHandle::new(mount(&fixture)?),
        UiScriptEnvironment::new(1024, 768, false)?,
        &[
            ("minimapZoom".into(), "2".into()),
            ("minimapInsideZoom".into(), "5".into()),
        ],
        &AddonCatalog::default(),
    )?;
    assert!(manager.take_changed_cvars().is_empty());
    assert_eq!(manager.minimap_state().zoom(), 2);
    manager.set_minimap_indoors(true)?;
    assert_eq!(manager.localized_text("EVENT_ZOOM")?.as_deref(), Some("5"));
    assert_eq!(
        manager.localized_text("ZOOM_EVENT_COUNT")?.as_deref(),
        Some("1")
    );
    manager.set_minimap_indoors(true)?;
    assert_eq!(
        manager.localized_text("ZOOM_EVENT_COUNT")?.as_deref(),
        Some("1")
    );
    manager.invoke_binding("ZOOM", true)?;
    assert_eq!(manager.minimap_state().zoom(), 4);
    assert_eq!(
        manager.localized_text("ZOOM_EVENT_COUNT")?.as_deref(),
        Some("1")
    );
    assert_eq!(
        manager.take_changed_cvars(),
        [("minimapInsideZoom".into(), "4".into())]
    );
    manager.set_minimap_indoors(false)?;
    assert_eq!(manager.localized_text("EVENT_ZOOM")?.as_deref(), Some("2"));
    assert_eq!(
        manager.localized_text("ZOOM_EVENT_COUNT")?.as_deref(),
        Some("2")
    );
    assert!(manager.take_changed_cvars().is_empty());
    Ok(())
}
