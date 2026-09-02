//! Integrated regression coverage for controlled-player and world UI state.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiAccountExpansion, UiAnimationPlan, UiBattlegroundType, UiBundle, UiFramePlan,
    UiLayoutPlan, UiMailComposeState, UiManifestKind, UiObjectCatalog, UiObjectTree, UiPetAction,
    UiPossessAction, UiRegionStatePlan, UiRune, UiRuneType, UiRuntimeTemplatePlan,
    UiScriptEnvironment, UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan, UiShapeshiftForm,
    UiTexturePlan, UiTextureStatePlan, UiWorldStateIndicator,
};

use crate::support::{Fixture, FixtureFile};

/// Native feature families expose one coherent live environment to FrameXML.
#[test]
fn frame_globals_read_the_shared_runtime_boundary() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\FrameXML\\FrameXML.toc",
            bytes: b"State.xml\n",
        },
        FixtureFile {
            path: "Interface\\FrameXML\\State.xml",
            bytes: br#"<Ui>
<Font name="RuntimeFont" font="Fonts\FRIZQT__.TTF"><FontHeight><AbsValue val="12"/></FontHeight></Font>
<Button name="RuntimeButtonTemplate" virtual="true">
  <Scripts><OnClick>CLICK_BUTTON = button; CLICK_DOWN = down</OnClick></Scripts>
  <ButtonText name="$parentText"/><NormalFont style="RuntimeFont"/>
</Button>
<Frame name="Root"><Scripts><OnLoad>
  assert(GetAccountExpansionLevel() == 2 and GetExpansionLevel() == 2)
  local haveTotem, name, startTime, duration, icon = GetTotemInfo(1)
  assert(not haveTotem and name == "" and startTime == 0 and duration == 0 and icon == nil)
  assert(GetMouseFocus() == nil)
  assert(GetSendMailPrice() == 60 and GetTabardCreationCost() == 100000)
  assert(PetHasActionBar() == 1)
  local petName, petSubtext, petTexture, petToken, petActive = GetPetActionInfo(1)
  assert(petName == "Growl" and petSubtext == nil and petTexture == "Pet-Growl")
  assert(petToken == nil and petActive == nil and GetPetActionSlotUsable(1) == 1)
  assert(GetNumShapeshiftForms() == 1)
  local formTexture, formName, formActive, formCastable = GetShapeshiftFormInfo(1)
  assert(formTexture == "Bear-Texture" and formName == "Bear Form")
  assert(formActive == nil and formCastable == 1)
  assert(IsPossessBarVisible() == 1)
  local possessTexture, possessName, possessEnabled = GetPossessInfo(1)
  assert(possessTexture == "Cancel-Texture" and possessName == "Dismiss" and possessEnabled == 1)
  assert(GetRuneType(1) == 1)
  local runeStart, runeDuration, runeReady = GetRuneCooldown(1)
  assert(runeStart == 0 and runeDuration == 0 and runeReady)
  assert(GetMapInfo() == "TestMap")
  assert(GetCurrentMapContinent() == 1 and GetCurrentMapZone() == 7)
  assert(GetCurrentMapAreaID() == 42 and GetCurrentMapDungeonLevel() == 2)
  assert(GetNumDungeonMapLevels() == 3 and DungeonUsesTerrainMap() == 1)
  assert(IsZoomOutAvailable() == 1 and not HasDebugZoneMap())
  assert(GetNumWorldStateUI() == 1)
  local uiType, uiState, uiText, uiIcon, dynamicIcon, tooltip,
        dynamicTooltip, extendedUi, state1, state2, state3 = GetWorldStateUIInfo(1)
  assert(uiType == 1 and uiState == 2 and uiText == "Wintergrasp")
  assert(uiIcon == "Static" and dynamicIcon == "Dynamic")
  assert(tooltip == "Objective" and dynamicTooltip == "Contested")
  assert(extendedUi == "CAPTUREPOINT" and state1 == 3 and state2 == 4 and state3 == 5)
  assert(GetNumBattlegroundTypes() == 1)
  local bgName, canEnter, holiday, random, bgId = GetBattlegroundInfo(1)
  assert(bgName == "Warsong Gulch" and canEnter and holiday and not random and bgId == 2)
  local tab = CreateFrame("Button", "DynamicTab", self, "RuntimeButtonTemplate")
  assert(DynamicTabText:GetFontObject() == RuntimeFont)
  DynamicTabText:SetText("Tab")
  tab:SetText("Tab")
  local tabTextHeight = tab:GetTextHeight()
  assert(tabTextHeight == 12, tostring(tabTextHeight))
  tab:Click()
  assert(CLICK_BUTTON == "LeftButton" and CLICK_DOWN == false, tostring(CLICK_BUTTON)..":"..tostring(CLICK_DOWN))
  local tooltipFrame = CreateFrame("GameTooltip", "RuntimeTooltip", self)
  tooltipFrame:SetPadding(16)
  local questPoi = CreateFrame("QuestPOIFrame", "RuntimeQuestPoi", self)
  assert(questPoi:GetObjectType() == "QuestPOIFrame" and questPoi:IsObjectType("Frame"))
  questPoi:SetFillTexture("Fill")
  questPoi:SetBorderTexture("Border")
  questPoi:SetFillAlpha(128)
  questPoi:SetBorderAlpha(192)
  questPoi:SetBorderScalar(1)
  questPoi:DrawQuestBlob(42, true)
  assert(questPoi:GetNumTooltips() == 0 and questPoi:GetTooltipIndex(1) == nil)
</OnLoad></Scripts></Frame></Ui>"#,
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
    let environment = UiScriptEnvironment::new(1280, 720, false)?;

    environment
        .account_state()
        .set_expansion(UiAccountExpansion::WrathOfTheLichKing);
    let mail: UiMailComposeState = environment.mail_compose_state();
    mail.set_postage(60);
    environment.tabard_state().set_session(true, 100_000);
    let mut pet_slots = std::array::from_fn(|_| None);
    pet_slots[0] = Some(UiPetAction::new("Growl", "Pet-Growl"));
    environment.pet_action_state().replace(true, pet_slots);
    environment
        .stance_state()
        .replace_forms(vec![UiShapeshiftForm::new("Bear-Texture", "Bear Form")]);
    environment.stance_state().set_possess_bar_visible(true);
    environment
        .stance_state()
        .replace_possess_actions(vec![UiPossessAction::new("Cancel-Texture", "Dismiss")]);
    let mut runes = [None; 6];
    runes[0] = Some(UiRune::ready(UiRuneType::Blood));
    environment.rune_state().replace(runes);
    environment
        .world_map_state()
        .set_selection(Some("TestMap".to_owned()), Some(512), 1, 2, true);
    environment.world_map_state().set_location(7, 42, 3);
    environment.world_map_state().set_zoom_out_available(true);
    environment.world_state_ui_state().replace(vec![
        UiWorldStateIndicator::new(1, 2, "Wintergrasp")
            .with_icons("Static", "Dynamic")
            .with_tooltips("Objective", "Contested")
            .with_extended_ui("CAPTUREPOINT", [3, 4, 5]),
    ]);
    let battlefield = environment.battlefield_state();
    battlefield.replace_battleground_types(vec![UiBattlegroundType::new(
        "Warsong Gulch",
        true,
        true,
        false,
        2,
    )]);
    assert_eq!(battlefield.battleground_type_count(), 1);

    let runtime_plan = UiScriptRuntimePlan::new(
        &tree,
        &animations,
        &frames,
        &regions,
        &templates,
        &fonts,
        &texture_states,
    );
    let mut runtime = UiScriptRuntime::new(&bundle, &runtime_plan, environment)?;
    let battleground_count: mlua::Function =
        bundle.lua().globals().get("GetNumBattlegroundTypes")?;
    assert_eq!(battleground_count.call::<usize>(())?, 1);
    runtime.execute_all(&bundle, &tree, &scripts)?;
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
