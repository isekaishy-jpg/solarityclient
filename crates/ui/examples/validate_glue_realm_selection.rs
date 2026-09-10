//! Exercises realm buttons with original GlueXML and no preloaded characters.

use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
use solarity_ui::{
    AddonCatalog, GlueInitialScreen, GlueManager, UiEventArgument, UiEventPayload,
    UiGlueNetworkAction, UiPointerButton, UiRealmCategory, UiRealmDirectory, UiRealmFlags,
    UiRealmInfo,
};
use std::{cell::RefCell, error::Error, rc::Rc};

fn main() -> Result<(), Box<dyn Error>> {
    let root = std::env::args().nth(1).ok_or("missing Data root")?;
    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(root)?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let addons = AddonCatalog::discover(&mut store)?;
    let mut manager = GlueManager::start_shared_with_profile_and_random(
        AssetStoreHandle::new(store),
        (1280, 720),
        false,
        GlueInitialScreen::Login,
        &[
            "readEULA",
            "readTOS",
            "readTerminationWithoutNotice",
            "readScanning",
            "readContest",
        ]
        .map(|name| (name.to_owned(), "1".to_owned())),
        &addons,
        Rc::new(RefCell::new(solarity_cpu::BlizzardRand::new(1))),
    )?;
    for _ in 0..120 {
        manager.update(1.0 / 60.0)?;
    }
    manager.set_realm_directory(UiRealmDirectory::new(
        ["Development", "Normal"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                UiRealmCategory::new(
                    index as u32 + 1,
                    name.to_owned(),
                    vec![UiRealmInfo::new(
                        index as u32 + 1,
                        name.to_owned(),
                        0,
                        UiRealmFlags::default(),
                        0.,
                        None,
                    )],
                )
            })
            .collect(),
        None,
    ));
    manager.dispatch_event(
        "GET_PREFERRED_REALM_INFO",
        &UiEventPayload::new([UiEventArgument::Integer(2)]),
    )?;
    for _ in 0..120 {
        manager.update(1.0 / 60.0)?;
    }
    assert_eq!(manager.current_screen(), "realmwizard");
    click(&mut manager, "RealmWizardLocationButton1")?;
    click(&mut manager, "RealmWizardSuggest")?;
    assert!(take_actions(&manager).iter().any(|action| matches!(
        action,
        UiGlueNetworkAction::SetPreferredRealmInfo {
            category_index: 1,
            player_killing_allowed: false,
            roleplaying: false
        }
    )));
    // The runtime publishes this native event for a matching category candidate.
    // No directory fixture supplies a fallback race: production DBC state must do it.
    manager.dispatch_event(
        "SUGGEST_REALM",
        &UiEventPayload::new([UiEventArgument::Integer(1), UiEventArgument::Integer(1)]),
    )?;
    for _ in 0..120 {
        manager.update(1.0 / 60.0)?;
    }
    assert_eq!(manager.current_screen(), "charselect");
    assert!(
        take_actions(&manager)
            .iter()
            .any(|action| matches!(action, UiGlueNetworkAction::ChangeRealm { realm_id: 1 }))
    );
    let background: String = manager
        .bundle()
        .lua()
        .load("return GetSelectBackgroundModel(0)")
        .eval()?;
    assert_eq!(background, "Orc");
    click(&mut manager, "CharSelectChangeRealmButton")?;
    assert!(take_actions(&manager).iter().any(|action| matches!(
        action,
        UiGlueNetworkAction::RequestRealmList {
            show_progress_dialog: true,
            ..
        }
    )));
    // A nonmatching wizard preference opens the list instead of suggesting a realm.
    manager.dispatch_event(
        "SET_GLUE_SCREEN",
        &UiEventPayload::new([UiEventArgument::String("realmwizard".to_owned())]),
    )?;
    for _ in 0..120 {
        manager.update(1.0 / 60.0)?;
    }
    click(&mut manager, "RealmWizardLocationButton1")?;
    click(&mut manager, "RealmWizardGameTypeButton2")?;
    click(&mut manager, "RealmWizardSuggest")?;
    assert!(take_actions(&manager).iter().any(|action| matches!(
        action,
        UiGlueNetworkAction::SetPreferredRealmInfo {
            category_index: 1,
            player_killing_allowed: true,
            roleplaying: false
        }
    )));
    manager.dispatch_event("OPEN_REALM_LIST", &UiEventPayload::empty())?;
    for _ in 0..120 {
        manager.update(1.0 / 60.0)?;
    }
    assert!(
        manager
            .bundle()
            .lua()
            .load("return RealmList:IsShown() ~= nil and RealmList:IsShown() ~= false")
            .eval::<bool>()?
    );
    println!(
        "validated Suggest Realm match/no-match callbacks, default race before character enumeration, and Change Realm request"
    );
    Ok(())
}

fn take_actions(manager: &GlueManager) -> Vec<UiGlueNetworkAction> {
    let mut actions = Vec::new();
    while let Some(action) = manager.take_network_action() {
        actions.push(action);
    }
    actions
}

fn click(manager: &mut GlueManager, name: &str) -> Result<(), Box<dyn Error>> {
    let index = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some(name))
        .ok_or("button missing")?;
    let region = manager
        .geometry()
        .region(index)
        .ok_or("button geometry missing")?;
    assert!(
        region.effectively_shown() && region.effective_alpha() > 0.0,
        "{name} hidden"
    );
    let bounds = region.presentation_bounds();
    let position = (
        (bounds.left() + bounds.right()) / 2.,
        (bounds.bottom() + bounds.top()) / 2.,
    );
    manager.pointer_motion(position)?;
    manager.pointer_button(position, UiPointerButton::Left, true)?;
    let release = manager.pointer_button(position, UiPointerButton::Left, false)?;
    assert_eq!(release.object_index(), Some(index), "{name}");
    assert!(release.click_activated(), "{name}");
    Ok(())
}
