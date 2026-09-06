//! AddOn source execution through the existing FrameXML Lua and object registry.

use crate::support::{Fixture, FixtureFile};
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    AddonCatalog, FontCatalog, UiAddonLoadState, UiAnimationPlan, UiBundle, UiFramePlan,
    UiLayoutPlan, UiManifestKind, UiObjectCatalog, UiObjectTree, UiRegionStatePlan,
    UiRuntimeTemplatePlan, UiScriptEnvironment, UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan,
    UiTexturePlan, UiTextureStatePlan,
};
use std::error::Error;

#[test]
fn load_addon_preserves_nested_execution_dependencies_and_existing_objects()
-> Result<(), Box<dyn Error>> {
    let mut slots = b"WDBC".to_vec();
    for field in [0_u32, 3, 12, 1] {
        slots.extend_from_slice(&field.to_le_bytes());
    }
    slots.push(0);
    let crit_base = game_table(11);
    let coefficients = game_table(1100);
    let fixture = Fixture::new(&[
        FixtureFile { path: "DBFilesClient/gtChanceToMeleeCritBase.dbc", bytes: &crit_base },
        FixtureFile { path: "DBFilesClient/gtChanceToSpellCritBase.dbc", bytes: &crit_base },
        FixtureFile { path: "DBFilesClient/gtChanceToMeleeCrit.dbc", bytes: &coefficients },
        FixtureFile { path: "DBFilesClient/gtChanceToSpellCrit.dbc", bytes: &coefficients },
        FixtureFile { path: "DBFilesClient/gtOCTRegenHP.dbc", bytes: &coefficients },
        FixtureFile { path: "DBFilesClient/gtRegenHPPerSpt.dbc", bytes: &coefficients },
        FixtureFile { path: "DBFilesClient/gtRegenMPPerSpt.dbc", bytes: &coefficients },
        FixtureFile { path: "DBFilesClient/PaperDollItemFrame.dbc", bytes: &slots },
        FixtureFile { path: "Interface/FrameXML/FrameXML.toc", bytes: b"Base.xml\n" },
        FixtureFile { path: "Interface/FrameXML/Base.xml", bytes: br#"<Ui>
<Font name="BaseFont"><FontHeight><AbsValue val="12"/></FontHeight></Font>
<Frame name="Parent" frameLevel="4" frameStrata="HIGH"/>
<Frame name="Observer"><Scripts><OnLoad>self:RegisterEvent("ADDON_LOADED")</OnLoad><OnEvent>
  assert(this == self and event == "ADDON_LOADED" and arg1 == select(1, ...))
  local loaded, finished = IsAddOnLoaded(arg1)
  assert(loaded == 1 and finished == 1)
  EVENTS = (EVENTS or "") .. arg1 .. ","
</OnEvent></Scripts></Frame>
<Frame name="Driver"><Scripts><OnLoad>
  SetAddonVersionCheck(1)
  ORDER = ""
  local parent = Parent
  event, arg1, arg2 = "outer", "original", 19
  local ok, reason = LoadAddOn("Main")
  assert(ok == 1 and reason == nil, tostring(ok) .. ":" .. tostring(reason))
  assert(Parent == parent and Tail == nil)
  assert(event == "outer" and arg1 == "original" and arg2 == 19 and this == self)
  assert(ORDER == "optional,dependency,main-first,nested,main-last,")
  assert(EVENTS == "Optional,Dependency,Nested,Main,")
  assert(MainRoot:GetParent() == Parent and MainRoot:GetID() == 7)
  MainRoot:SetParent("Observer")
  assert(MainRoot:GetParent() == Observer)
  assert(not pcall(MainRoot.SetParent, MainRoot, "MissingParent"))
  assert(not pcall(MainRoot.SetParent, MainRoot, MainRootText))
  assert(MainRoot:GetParent() == Observer)
  MainRoot:SetParent("Parent")
  assert(MainRoot:GetFrameStrata() == "DIALOG" and MainRoot:GetFrameLevel() == 8)
  assert(MainRoot:IsMouseEnabled() and MainRoot:IsKeyboardEnabled())
  assert(MainRoot:GetWidth() == 83 and MainRoot:GetHeight() == 29)
  assert(MainRootText:GetFontObject() == AddonFont and MainRootText:GetText() == "module text")
  assert(LoadAddOn("mAiN") == 1 and ORDER == "optional,dependency,main-first,nested,main-last,")
  assert(not pcall(LoadAddOn, 0) and not pcall(LoadAddOn, 99))
  assert(select(2, LoadAddOn("Absent")) == "MISSING")
  assert(select(2, LoadAddOn("Disabled")) == "DISABLED")
  assert(select(2, LoadAddOn("BrokenDependency")) == "DEP_DISABLED")
  assert(select(2, LoadAddOn("Old")) == "INTERFACE_VERSION")
  SetAddonVersionCheck(0)
  assert(LoadAddOn("Old") == 1)
</OnLoad></Scripts></Frame>
<Frame name="Tail"/>
</Ui>"# },
        FixtureFile { path: "Interface/AddOns/Main/Main.toc", bytes: b"## Interface: 30300\n## OptionalDeps: Optional, Disabled\n## Dependencies: Dependency\n## LoadOnDemand: 1\n## SavedVariables: MainAccount\n## SavedVariablesPerCharacter: MainCharacter\nFirst.lua\nMain.xml\nLast.lua\n" },
        FixtureFile { path: "Interface/AddOns/Main/First.lua", bytes: b"error('the loose source must win')" },
        FixtureFile { path: "Interface/AddOns/Main/Last.lua", bytes: br#"local name, private = ...
assert(name == "Main" and private.token == 23)
assert(FutureTemplate == nil)
local future = CreateFrame("Frame", "FutureInstance", Parent, "FutureTemplate")
assert(future and NestedRoot)
ORDER = ORDER .. "main-last,"
"# },
        FixtureFile { path: "Interface/AddOns/Main/Main.xml", bytes: br#"<Ui>
<Font name="AddonFont" inherits="BaseFont"/>
<Frame name="SharedTemplate" virtual="true"><Size><AbsDimension x="11" y="12"/></Size></Frame>
<Frame name="MainRoot" parent="Parent" inherits="DependencyTemplate" id="7" frameLevel="8" frameStrata="DIALOG" enableMouse="true" enableKeyboard="true">
  <Size><AbsDimension x="83" y="29"/></Size>
  <Layers><Layer><FontString name="$parentText" inherits="AddonFont"/></Layer></Layers>
  <Scripts><OnLoad>MainRootText:SetText("module text")</OnLoad></Scripts>
</Frame>
<Script>assert(LoadAddOn("Nested") == 1)</Script>
<Frame name="FutureTemplate" virtual="true"/>
</Ui>"# },
        FixtureFile { path: "Interface/AddOns/Dependency/Dependency.toc", bytes: b"## Interface: 30300\nDep.lua\nDep.xml\n" },
        FixtureFile { path: "Interface/AddOns/Dependency/Dep.lua", bytes: br#"local name, private = ...
assert(name == "Dependency" and private.token == nil)
local loaded, finished = IsAddOnLoaded("Main")
assert(loaded == 1 and finished == nil and LoadAddOn("Main") == 1)
ORDER = ORDER .. "dependency,"
"# },
        FixtureFile { path: "Interface/AddOns/Dependency/Dep.xml", bytes: br#"<Ui><Frame name="DependencyTemplate" virtual="true" id="23"/></Ui>"# },
        FixtureFile { path: "Interface/AddOns/Optional/Optional.toc", bytes: b"## Interface: 30300\nOpt.lua\n" },
        FixtureFile { path: "Interface/AddOns/Optional/Opt.lua", bytes: b"ORDER = ORDER .. 'optional,'" },
        FixtureFile { path: "Interface/AddOns/Nested/Nested.toc", bytes: b"## Interface: 30300\nNested.xml\n" },
        FixtureFile { path: "Interface/AddOns/Nested/Nested.xml", bytes: br#"<Ui>
<Script>assert(MainRoot and not pcall(CreateFrame, "Frame", nil, nil, "FutureTemplate"))</Script>
<Frame name="NestedRoot" parent="MainRoot" inherits="SharedTemplate"><Scripts><OnLoad>
assert(self:GetWidth() == 11 and self:GetParent() == MainRoot)
ORDER = ORDER .. "nested,"
</OnLoad></Scripts></Frame>
</Ui>"# },
        FixtureFile { path: "Interface/AddOns/Disabled/Disabled.toc", bytes: b"## Interface: 30300\n## DefaultState: disabled\n" },
        FixtureFile { path: "Interface/AddOns/BrokenDependency/BrokenDependency.toc", bytes: b"## Interface: 30300\n## Dependencies: Disabled\n" },
        FixtureFile { path: "Interface/AddOns/Old/Old.toc", bytes: b"## Interface: 30200\n" },
    ])?;
    for name in [
        "Main",
        "Dependency",
        "Optional",
        "Nested",
        "Disabled",
        "BrokenDependency",
        "Old",
    ] {
        fixture.write_loose_file(format!("Interface/AddOns/{name}/fixture-presence"), b"")?;
    }
    fixture.write_loose_file(
        "Interface/AddOns/Main/First.lua",
        br#"local name, private = ...
assert(name == "Main")
private.token = 23
local loaded, finished = IsAddOnLoaded("Dependency")
assert(loaded == 1 and finished == 1)
ORDER = ORDER .. "main-first,"
"#,
    )?;
    let store = mount(&fixture)?;
    let (bundle, environment, state) = execute(store)?;
    assert_eq!(state.status_by_name("Main"), Some((true, true)));
    assert_eq!(
        state.status_by_name("BrokenDependency"),
        Some((false, false))
    );
    assert_eq!(
        environment.saved_variable_state().account_names(),
        ["MainAccount"]
    );
    assert_eq!(
        environment.saved_variable_state().character_names(),
        ["MainCharacter"]
    );
    assert!(bundle.lua().globals().contains_key("Tail")?);
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    Ok(AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?)
}

fn execute(
    mut store: AssetStore,
) -> Result<(UiBundle, UiScriptEnvironment, UiAddonLoadState), Box<dyn Error>> {
    let addons = AddonCatalog::discover(&mut store)?;
    let state = UiAddonLoadState::from_catalog(&addons);
    let bundle = UiBundle::load(&mut store, UiManifestKind::Frame)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let catalog = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&catalog, &fonts)?;
    let regions = UiRegionStatePlan::resolve(&tree, &UiLayoutPlan::from_tree(&tree)?)?;
    let frames = UiFramePlan::from_tree(&tree)?.resolve(&tree)?;
    let scripts = UiScriptPlan::from_tree(&tree, bundle.lua())?;
    let templates = UiRuntimeTemplatePlan::from_catalog(&catalog, &fonts, bundle.lua())?;
    let textures = UiTextureStatePlan::resolve(&tree, &UiTexturePlan::from_tree(&tree)?)?;
    let animations = UiAnimationPlan::from_tree(&tree)?;
    let plan = UiScriptRuntimePlan::new(
        &tree,
        &animations,
        &frames,
        &regions,
        &templates,
        &fonts,
        &textures,
    );
    let environment = UiScriptEnvironment::new(1024, 768, false)?
        .with_asset_store(store)
        .with_addon_load_state(state.clone());
    let mut runtime = UiScriptRuntime::new(&bundle, &plan, environment.clone())?;
    runtime.execute_all(&bundle, &tree, &scripts)?;
    Ok((bundle, environment, state))
}

fn game_table(rows: u32) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [rows, 1, 4, 1] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.resize(20 + rows as usize * 4 + 1, 0);
    bytes
}
