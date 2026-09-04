//! External stock-compatibility tests for battlefield queue projection.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiAnimationPlan, UiBattlefieldQueueState, UiBattlefieldQueueStatus,
    UiBattlefieldSlot, UiBundle, UiFramePlan, UiLayoutPlan, UiManifestKind, UiObjectCatalog,
    UiObjectTree, UiRegionStatePlan, UiRuntimeTemplatePlan, UiScriptEnvironment, UiScriptPlan,
    UiScriptRuntime, UiScriptRuntimePlan, UiTexturePlan, UiTextureStatePlan, UiWorldPvpQueueSlot,
};

use crate::support::{Fixture, FixtureFile};

/// Queue slots preserve the complete seven-value FrameXML contract.
#[test]
fn battlefield_status_reads_authoritative_two_slot_state() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\FrameXML\\FrameXML.toc",
            bytes: b"Battlefield.xml\n",
        },
        FixtureFile {
            path: "Interface\\FrameXML\\Battlefield.xml",
            bytes: br#"<Ui><Frame name="Root"><Scripts><OnLoad>
  STATUS1, MAP1, INSTANCE1, MIN1, MAX1, TEAM1, RATED1 = GetBattlefieldStatus(1)
  STATUS2, MAP2, INSTANCE2, MIN2, MAX2, TEAM2, RATED2 = GetBattlefieldStatus(2)
  WORLD_STATUS, WORLD_MAP, WORLD_ID, WORLD_EXPIRES = GetWorldPVPQueueStatus(1)
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
    let environment = UiScriptEnvironment::new(1024, 768, false)?;
    let battlefield = environment.battlefield_state();
    battlefield.set_slot(
        1,
        UiBattlefieldSlot::active(
            UiBattlefieldQueueStatus::Confirm,
            "Nagrand Arena",
            17,
            (70, 80),
            3,
            true,
        )?,
    )?;
    environment
        .battlefield_state()
        .set_world_pvp_slot(UiWorldPvpQueueSlot::active(
            UiBattlefieldQueueStatus::Queued,
            "Wintergrasp",
            42,
            30_000,
        )?);
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
    runtime.execute_all(&bundle, &tree, &scripts)?;

    let globals = bundle.lua().globals();
    let is_active_arena = globals.get::<mlua::Function>("IsActiveBattlefieldArena")?;
    assert!(!is_active_arena.call::<bool>(())?);
    battlefield.set_slot(
        1,
        UiBattlefieldSlot::active(
            UiBattlefieldQueueStatus::Active,
            "Nagrand Arena",
            17,
            (70, 80),
            3,
            true,
        )?,
    )?;
    assert!(is_active_arena.call::<bool>(())?);
    assert_eq!(globals.get::<String>("STATUS1")?, "confirm");
    assert_eq!(globals.get::<String>("MAP1")?, "Nagrand Arena");
    assert_eq!(globals.get::<u32>("INSTANCE1")?, 17);
    assert_eq!(globals.get::<u8>("MIN1")?, 70);
    assert_eq!(globals.get::<u8>("MAX1")?, 80);
    assert_eq!(globals.get::<u8>("TEAM1")?, 3);
    assert!(globals.get::<bool>("RATED1")?);
    assert_eq!(globals.get::<String>("STATUS2")?, "none");
    assert!(globals.get::<Option<String>>("MAP2")?.is_none());
    assert_eq!(globals.get::<u32>("INSTANCE2")?, 0);
    assert!(!globals.get::<bool>("RATED2")?);
    assert_eq!(globals.get::<String>("WORLD_STATUS")?, "queued");
    assert_eq!(globals.get::<String>("WORLD_MAP")?, "Wintergrasp");
    assert_eq!(globals.get::<u32>("WORLD_ID")?, 42);
    assert_eq!(globals.get::<u32>("WORLD_EXPIRES")?, 30_000);
    Ok(())
}

/// The protocol-defined slot count and arena team vocabulary are closed.
#[test]
fn battlefield_state_rejects_non_stock_indices_and_team_sizes() {
    let state = UiBattlefieldQueueState::new();
    assert!(state.slot(0).is_err());
    assert!(state.slot(3).is_err());
    assert!(
        UiBattlefieldSlot::active(
            UiBattlefieldQueueStatus::Queued,
            "Arena",
            0,
            (1, 80),
            4,
            false,
        )
        .is_err()
    );
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
