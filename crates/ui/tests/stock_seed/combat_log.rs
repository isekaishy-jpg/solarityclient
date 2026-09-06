//! Combat history tests exercise nonempty records through the native Lua API.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiAnimationPlan, UiBundle, UiCombatLogEntry, UiCombatLogObject, UiCombatLogSpell,
    UiEventArgument as Arg, UiEventPayload, UiFramePlan, UiLayoutPlan, UiManifestKind,
    UiObjectCatalog, UiObjectTree, UiPlayerState, UiRegionStatePlan, UiRuntimeTemplatePlan,
    UiScriptEnvironment, UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan, UiTexturePlan,
    UiTextureStatePlan,
};

use crate::support::{Fixture, FixtureFile};

#[test]
fn combat_history_filters_navigates_and_preserves_the_complete_event_tuple()
-> Result<(), Box<dyn Error>> {
    let (bundle, environment) = runtime()?;
    let history = environment.combat_log_state();
    history.append(entry("SWING_DAMAGE", 1, None, 10.0)?, 100, 300);
    history.append(entry("SPELL_DAMAGE", 2, Some("Frostbolt"), 20.0)?, 200, 300);
    history.append(entry("SPELL_HEAL", 1, Some("Heal"), 30.0)?, 300, 300);
    bundle.lua().load(r#"
        assert(CombatLogGetNumEntries() == 3)
        assert(select('#', CombatLogGetCurrentEntry()) == 0)
        assert(CombatLogSetCurrentEntry(1) == 1)
        assert(select(2, CombatLogGetCurrentEntry()) == 'SWING_DAMAGE')
        assert(CombatLogSetCurrentEntry(0) == 1)
        assert(select(2, CombatLogGetCurrentEntry()) == 'SPELL_HEAL')
        assert(CombatLogAdvanceEntry(-1) == 1)
        local timestamp, event, source, sourceName, sourceFlags, destination,
            destinationName, destinationFlags, spell, name, school, amount, overkill,
            damageSchool, resisted, blocked, absorbed, critical, glancing, crushing = CombatLogGetCurrentEntry()
        assert(timestamp == 20 and event == 'SPELL_DAMAGE')
        assert(source == '0x0000000000000002' and sourceName == 'Caster' and sourceFlags == 0x511)
        assert(destination == '0xF130000000000007' and destinationName == nil and destinationFlags == 0xa48)
        assert(spell == 116 and name == 'Frostbolt' and school == 16)
        assert(amount == 87 and overkill == -1 and damageSchool == 16)
        assert(resisted == nil and blocked == nil and absorbed == 5 and critical == 1)
        assert(glancing == nil and crushing == nil and select('#', CombatLogGetCurrentEntry()) == 20)
        assert(CombatLogAdvanceEntry(1) == 1 and CombatLogAdvanceEntry(1) == nil)
        assert(select('#', CombatLogGetCurrentEntry()) == 0 and CombatLogAdvanceEntry(-1) == nil)

        assert(not pcall(CombatLogAddFilter, nil, 1))
        assert(not pcall(CombatLogAddFilter, nil, nil, 0x300))
        assert(CombatLogGetNumEntries() == 3)
        CombatLogAddFilter('unknown, spell_damage SPELL_HEAL', 0x511, 0xa48, 'fRoStBoLt')
        assert(CombatLogGetNumEntries() == 1)
        assert(CombatLogGetNumEntries(true) == 3 and CombatLogGetNumEntries(0) == 1)
        assert(CombatLogGetNumEntries('enabled') == 3 and CombatLogGetNumEntries('off') == 1)
        assert(CombatLogGetNumEntries({}) == 1)
        assert(CombatLogSetCurrentEntry(0) == 1 and select(2, CombatLogGetCurrentEntry()) == 'SPELL_DAMAGE')
        CombatLogAddFilter('SWING_DAMAGE', '0X1trailing', nil)
        assert(CombatLogGetNumEntries() == 2)
        assert(CombatLogSetCurrentEntry(-1) == 1 and select(2, CombatLogGetCurrentEntry()) == 'SWING_DAMAGE')
        assert(CombatLogAdvanceEntry(1) == 1 and select(2, CombatLogGetCurrentEntry()) == 'SPELL_DAMAGE')
        assert(CombatLogSetCurrentEntry(3) == nil and CombatLogSetCurrentEntry(3, true) == 1)
        CombatLogResetFilter()
        assert(select(2, CombatLogGetCurrentEntry()) == 'SPELL_HEAL' and CombatLogGetNumEntries() == 3)
        CombatLogAddFilter('', nil, nil)
        assert(CombatLogGetNumEntries() == 0 and CombatLogAdvanceEntry(0) == 1)
        CombatLogResetFilter()
        CombatLogAddFilter(nil, 'invalid-guid', nil)
        assert(CombatLogGetNumEntries() == 0)
        CombatLogResetFilter()
        CombatLogAddFilter(nil, nil, nil, 116)
        assert(CombatLogGetNumEntries() == 2)
        assert(CombatLog_Object_IsA(0x511, 0x511) == 1)
        assert(CombatLog_Object_IsA(0x511, 0x512) == nil)
        assert(CombatLog_Object_IsA(0x10000, 0x10000) == 1)
        CombatLogClearEntries()
        assert(CombatLogGetNumEntries(true) == 0 and select('#', CombatLogGetCurrentEntry()) == 0)
    "#).exec()?;
    Ok(())
}

#[test]
fn retention_recycles_on_admission_and_advances_a_deleted_cursor() -> Result<(), Box<dyn Error>> {
    let (bundle, environment) = runtime()?;
    let history = environment.combat_log_state();
    history.append(entry("SWING_DAMAGE", 1, None, 1.0)?, u32::MAX - 999, 2);
    history.append(entry("SWING_DAMAGE", 1, None, 2.0)?, 0, 2);
    bundle
        .lua()
        .load("assert(CombatLogSetCurrentEntry(1) == 1)")
        .exec()?;
    // Exactly two seconds after the first entry, across the wrapping tick.
    history.append(entry("SWING_DAMAGE", 1, None, 3.0)?, 1000, 2);
    bundle
        .lua()
        .load(
            r#"
        assert(CombatLogGetNumEntries() == 2 and CombatLogGetCurrentEntry() == 2)
        assert(CombatLogGetRetentionTime() == 300)
        CombatLogSetRetentionTime('12.9')
        assert(CombatLogGetRetentionTime() == 12 and GetCVar('combatLogRetentionTime') == '12')
        SetCVar('combatLogRetentionTime', '19')
        assert(CombatLogGetRetentionTime() == 19)
        assert(not pcall(CombatLogSetRetentionTime, {}) and not pcall(CombatLogSetCurrentEntry))
        CombatLogClearEntries()
    "#,
        )
        .exec()?;
    // Clear leaves two reusable native slots; both are consumed before an
    // expired historical node needs to be recycled.
    history.append(entry("SWING_DAMAGE", 1, None, 4.0)?, 4000, 0);
    history.append(entry("SWING_DAMAGE", 1, None, 5.0)?, 5000, 0);
    bundle
        .lua()
        .load("assert(CombatLogGetNumEntries() == 2)")
        .exec()?;
    history.append(entry("SWING_DAMAGE", 1, None, 6.0)?, 6000, 0);
    bundle.lua().load("assert(CombatLogGetNumEntries() == 2 and CombatLogSetCurrentEntry(1) == 1 and CombatLogGetCurrentEntry() == 5)").exec()?;
    Ok(())
}

#[test]
fn combat_text_captures_resolved_player_identity() -> Result<(), Box<dyn Error>> {
    let (bundle, environment) = runtime()?;
    let world = environment.world_state();
    let state = environment.combat_log_state();
    world.enter_player(UiPlayerState::new(0));
    world.set_player_guid(0x1234);
    bundle
        .lua()
        .load("CombatTextSetActiveUnit('PLAYER')")
        .exec()?;
    assert_eq!(state.active_text_unit(), Some(0x1234));
    world.set_player_guid(0x5678);
    assert_eq!(state.active_text_unit(), Some(0x1234));
    bundle
        .lua()
        .load("CombatTextSetActiveUnit('player')")
        .exec()?;
    assert_eq!(state.active_text_unit(), Some(0x5678));
    world.leave_world();
    bundle
        .lua()
        .load("CombatTextSetActiveUnit('player')")
        .exec()?;
    assert_eq!(state.active_text_unit(), None);
    Ok(())
}

fn entry(
    event: &str,
    guid: u64,
    spell: Option<&str>,
    timestamp: f64,
) -> Result<UiCombatLogEntry, Box<dyn Error>> {
    Ok(UiCombatLogEntry::new(
        timestamp,
        event,
        UiCombatLogObject {
            guid,
            name: Some("Caster".to_owned()),
            flags: 0x511,
        },
        UiCombatLogObject {
            guid: 0xf130_0000_0000_0007,
            name: None,
            flags: 0xa48,
        },
        spell.map(|name| UiCombatLogSpell {
            id: 116,
            name: Some(name.to_owned()),
            school: 16,
        }),
        UiEventPayload::new([
            Arg::Integer(87),
            Arg::Integer(-1),
            Arg::Integer(16),
            Arg::Nil,
            Arg::Nil,
            Arg::Integer(5),
            Arg::Integer(1),
            Arg::Nil,
            Arg::Nil,
        ]),
    )?)
}

fn runtime() -> Result<(UiBundle, UiScriptEnvironment), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface/FrameXML/FrameXML.toc",
            bytes: b"Test.xml\n",
        },
        FixtureFile {
            path: "Interface/FrameXML/Test.xml",
            bytes: b"<Ui><Frame name=\"Root\"/></Ui>",
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
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
    let environment = UiScriptEnvironment::new(1024, 768, false)?;
    let mut runtime = UiScriptRuntime::new(&bundle, &plan, environment.clone())?;
    runtime.execute_all(&bundle, &tree, &scripts)?;
    Ok((bundle, environment))
}
