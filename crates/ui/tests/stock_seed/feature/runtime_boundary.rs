//! Integrated regression coverage for controlled-player and world UI state.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiAccountExpansion, UiAnimationPlan, UiBattlegroundType, UiBundle, UiFramePlan,
    UiLayoutPlan, UiMailComposeState, UiManifestKind, UiObjectCatalog, UiObjectTree, UiPetAction,
    UiPlayerClassState, UiPlayerState, UiPlayerStatsState, UiPlayerVitalsState, UiPossessAction,
    UiRegionStatePlan, UiRune, UiRuneType, UiRuntimeTemplatePlan, UiScriptEnvironment,
    UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan, UiShapeshiftForm, UiTexturePlan,
    UiTextureStatePlan, UiUnitPowerType, UiWorldStateIndicator,
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
  SetMapToCurrentZone()
  assert(GetCurrentMapAreaID() == 42 and GetCurrentMapZone() == 7)
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
  assert(GetNumBattlefields() == 1)
  SetSelectedBattlefield(0)
  local battlefieldName, battlefieldDescription, maximumGroup, battlefieldCanEnter,
        battlefieldHoliday, battlefieldRandom = GetBattlefieldInfo()
  assert(battlefieldName == "Warsong Gulch" and battlefieldDescription == "Capture the flag")
  assert(maximumGroup == 10 and battlefieldCanEnter and battlefieldHoliday and not battlefieldRandom)
  local baseStat, effectiveStat, positiveStat, negativeStat = UnitStat("player", 1)
  assert(baseStat == 101 and effectiveStat == 101 and positiveStat == 7 and negativeStat == -3)
  assert(GetAttackPowerForStat(1, 20) == 20)
  assert(GetAttackPowerForStat(2, 20) == 0)
  assert(GetCritChanceFromAgility("player") == 0)
  assert(GetUnitMaxHealthModifier("player") == 0)
  assert(GetSpellCritChanceFromIntellect("player") == 0)
  assert(GetUnitHealthRegenRateFromSpirit("player") == 0)
  assert(GetUnitManaRegenRateFromSpirit("player") == 0)
  local baseArmor, effectiveArmor, armor, positiveArmor, negativeArmor = UnitArmor("player")
  assert(baseArmor == 0 and effectiveArmor == 0 and armor == 0)
  assert(positiveArmor == 0 and negativeArmor == 0)
  local attackSpeed, offhandSpeed = UnitAttackSpeed("player")
  assert(attackSpeed == 0 and offhandSpeed == nil)
  local minDamage, maxDamage, minOffhandDamage, maxOffhandDamage,
        damagePositive, damageNegative, damagePercent = UnitDamage("player")
  assert(minDamage == 0 and maxDamage == 0 and minOffhandDamage == 0)
  assert(maxOffhandDamage == 0 and damagePositive == 0 and damageNegative == 0)
  assert(damagePercent == 0)
  assert(GetCombatRating(18) == 0 and GetCombatRatingBonus(18) == 0)
  local basePower, positivePower, negativePower = UnitAttackPower("player")
  assert(basePower == 0 and positivePower == 0 and negativePower == 0)
  assert(GetComboPoints("player") == 0 and GetComboPoints("player", "target") == 0)
  local tank, healer, damage = UnitGroupRolesAssigned("player")
  assert(not tank and not healer and not damage)
  assert(UnitIsTalking("SolarityTester") == nil)
  assert(IsPVPTimerRunning() == nil and GetPVPTimer() == 0)
  local yesterdayKills, yesterdayHonor = GetPVPYesterdayStats()
  local sessionKills, sessionHonor = GetPVPSessionStats()
  local lifetimeKills, highestRank = GetPVPLifetimeStats()
  local rankName, rankNumber = GetPVPRankInfo(0)
  assert(yesterdayKills == 0 and yesterdayHonor == 0)
  assert(sessionKills == 0 and sessionHonor == 0)
  assert(lifetimeKills == 0 and highestRank == 0)
  assert(rankName == nil and rankNumber == 0)
  assert(UnitPVPRank("player") == 0 and GetPVPRankProgress() == 0)
  local honor, honorCap = GetHonorCurrency()
  local arena, arenaCap = GetArenaCurrency()
  assert(honor == 0 and honorCap == 75000)
  assert(arena == 0 and arenaCap == 10000)
  assert(GetCurrentArenaSeason() == 0 and GetPreviousArenaSeason() == 0)
  local freeSlots, bagFamily = GetContainerNumFreeSlots(0)
  assert(freeSlots == 16 and bagFamily == 0)
  assert(GetInventoryAlertStatus(1) == 0)
  assert(OffhandHasWeapon() == nil)
  local bankSlots, fullBank = GetNumBankSlots()
  assert(bankSlots == 0 and fullBank == nil)
  local timerName, timerValue, timerMaximum, timerScale, timerPaused, timerLabel =
    GetMirrorTimerInfo(1)
  assert(timerName == "UNKNOWN" and timerValue == 0 and timerMaximum == 0)
  assert(timerScale == 0 and timerPaused == 0 and timerLabel == "")
  assert(GetArmorPenetration() == 0 and GetDodgeChance() == 0)
  assert(GetSpellBonusDamage(2) == 0 and GetSpellBonusHealing() == 0)
  local expertise, offhandExpertise = GetExpertise()
  assert(expertise == 0 and offhandExpertise == 0)
  local manaRegen, castingManaRegen = GetManaRegen()
  assert(manaRegen == 0 and castingManaRegen == 0)
  assert(UnitHasMana("player") == 1 and UnitHasRelicSlot("player") == nil)
  assert(GetXPExhaustion() == nil)
  assert(select('#', GetQuestTimers()) == 0)
  assert(select('#', GetTrackedAchievements()) == 0)
  assert(HasCompletedAnyAchievement() == nil)
  assert(CalendarGetNumPendingInvites() == 0)
  assert(UnitCastingInfo("player") == nil and UnitChannelInfo("player") == nil)
  assert(GetGuildRosterShowOffline() == nil)
  SetGuildRosterShowOffline(true)
  assert(GetGuildRosterShowOffline() == 1)
  assert(GetNumVoiceSessionMembersBySessionID(1) == 0)
  local queued = { 42 }
  assert(GetLFGQueuedList(queued) == queued and next(queued) == nil)
  local leaderRole, tankRole, healerRole, damageRole = GetLFGRoles()
  assert(not leaderRole and not tankRole and not healerRole and not damageRole)
  local tankAvailable, healerAvailable, damageAvailable = GetAvailableRoles()
  assert(tankAvailable and not healerAvailable and damageAvailable)
  assert(not CanPartyLFGBackfill())
  assert(GetLFGDeserterExpiration() == nil)
  assert(GetLFGRandomCooldownExpiration() == nil)
  assert(GetNumLanguages() == 0 and GetLanguageByIndex(1) == nil)
  local restId, restName, restMultiplier = GetRestState()
  assert(restId == 2 and restName == "Normal" and restMultiplier == 1)
  assert(IsXPUserDisabled() == nil)
  RequestRaidInfo()
  GetGMTicket()
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
        .world_state()
        .enter_player(UiPlayerState::new(0));
    environment
        .world_state()
        .set_player_class(UiPlayerClassState::new("Warrior", "WARRIOR", 1));
    environment
        .world_state()
        .set_player_stats(UiPlayerStatsState::new(
            [101, 202, 303, 404, 505],
            [7, 0, 0, 0, 0],
            [-3, 0, 0, 0, 0],
        ));
    environment
        .world_state()
        .set_player_vitals(UiPlayerVitalsState::new(
            100,
            100,
            100,
            100,
            UiUnitPowerType::Mana,
        ));

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
    let group_roster = environment.group_roster_state();
    let support = environment.support_state();
    battlefield.replace_battleground_types(vec![
        UiBattlegroundType::new("Warsong Gulch", true, true, false, 2)
            .with_details("Capture the flag", 10),
    ]);
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
    assert!(group_roster.take_raid_info_request());
    assert!(!group_roster.take_raid_info_request());
    assert!(support.take_gm_ticket_request());
    assert!(!support.take_gm_ticket_request());
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
