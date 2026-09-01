//! External stock-compatibility tests for group-finder lifecycle projection.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiAnimationPlan, UiBundle, UiFramePlan, UiGroupFinderProposal, UiGroupFinderRole,
    UiGroupFinderRoleCheck, UiGroupFinderServerInfo, UiGroupFinderState, UiLayoutPlan,
    UiManifestKind, UiObjectCatalog, UiObjectTree, UiRegionStatePlan, UiRuntimeTemplatePlan,
    UiScriptEnvironment, UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan, UiTexturePlan,
    UiTextureStatePlan,
};

use crate::support::{Fixture, FixtureFile};

/// The proposal, server queue, and role-check APIs share one retained lifecycle.
#[test]
fn group_finder_globals_project_complete_active_state() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\FrameXML\\FrameXML.toc",
            bytes: b"GroupFinder.xml\n",
        },
        FixtureFile {
            path: "Interface\\FrameXML\\GroupFinder.xml",
            bytes: br#"<Ui><Frame name="Root"><Scripts><OnLoad>
  EXISTS, TYPE_ID, DUNGEON_ID, DUNGEON_NAME, DUNGEON_TEXTURE, ROLE, RESPONDED,
    ENCOUNTERS, COMPLETED, MEMBERS, LEADER, HOLIDAY = GetLFGProposal()
  IN_PARTY, JOINED, QUEUED, NO_PARTIAL, ACHIEVEMENTS, COMMENT, SLOTS = GetLFGInfoServer()
  ROLE_CHECK, ROLE_SLOTS, ROLE_MEMBERS = GetLFGRoleUpdate()
  LISTED, PARTY_LFG, IN_DUNGEON, RESTRICTED =
    IsListedInLFR(), IsPartyLFG(), IsInLFGDungeon(), HasLFGRestrictions()
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
    let finder = environment.group_finder_state();
    finder.set_proposal(Some(UiGroupFinderProposal::new(
        1,
        258,
        "The Oculus",
        "Interface\\LFGFrame\\LFGIcon-Oculus",
        UiGroupFinderRole::Healer,
        true,
        4,
        1,
        5,
        false,
        true,
    )?));
    finder.set_server_info(UiGroupFinderServerInfo::new(
        true,
        true,
        true,
        true,
        2,
        "achievement run",
        3,
    ));
    finder.set_role_check(UiGroupFinderRoleCheck::new(true, 3, 5));
    finder.set_listed_in_lfr(true);
    finder.set_party_lfg(true);
    finder.set_in_lfg_dungeon(true);
    finder.set_restrictions(true);
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
    assert!(globals.get::<bool>("EXISTS")?);
    assert_eq!(globals.get::<u32>("DUNGEON_ID")?, 258);
    assert_eq!(globals.get::<String>("ROLE")?, "HEALER");
    assert_eq!(globals.get::<u32>("ENCOUNTERS")?, 4);
    assert_eq!(globals.get::<u32>("COMPLETED")?, 1);
    assert!(globals.get::<bool>("QUEUED")?);
    assert_eq!(globals.get::<String>("COMMENT")?, "achievement run");
    assert!(globals.get::<bool>("ROLE_CHECK")?);
    assert_eq!(globals.get::<u32>("ROLE_MEMBERS")?, 5);
    assert!(globals.get::<bool>("LISTED")?);
    assert!(globals.get::<bool>("PARTY_LFG")?);
    assert!(globals.get::<bool>("IN_DUNGEON")?);
    assert!(globals.get::<bool>("RESTRICTED")?);
    Ok(())
}

/// Inactive state returns the stock false sentinels without fabricated records.
#[test]
fn group_finder_initial_state_is_authoritatively_inactive() {
    let state = UiGroupFinderState::new();
    assert!(state.proposal().is_none());
    assert!(!state.server_info().queued());
    assert!(!state.role_check().in_progress());
    assert!(!state.is_listed_in_lfr());
    assert!(!state.is_party_lfg());
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
