//! Stock's FrameXML sound gate applies when Lua calls the entry API.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
use solarity_ui::{
    AddonCatalog, FrameManager, UiEventPayload, UiGlueMediaAction, UiScriptEnvironment,
};

use crate::support::{Fixture, FixtureFile};

/// 52A980 and 528010 suppress entry sounds; 4C9110 direct files bypass the gate.
#[test]
fn frame_startup_and_world_entry_suppress_sound_entries_without_delaying_them()
-> Result<(), Box<dyn Error>> {
    let base = zero_table(11, 1);
    let coefficients = zero_table(1100, 1);
    let slots = zero_table(0, 3);
    let mut files = vec![
        FixtureFile {
            path: "DBFilesClient/gtChanceToMeleeCritBase.dbc",
            bytes: &base,
        },
        FixtureFile {
            path: "DBFilesClient/gtChanceToSpellCritBase.dbc",
            bytes: &base,
        },
        FixtureFile {
            path: "DBFilesClient/PaperDollItemFrame.dbc",
            bytes: &slots,
        },
        FixtureFile {
            path: "Interface/FrameXML/FrameXML.toc",
            bytes: b"Sounds.xml\n",
        },
        FixtureFile {
            path: "Interface/FrameXML/Bindings.xml",
            bytes: b"<Bindings/>",
        },
        FixtureFile {
            path: "WTF/DefaultBindings.wtf",
            bytes: b"",
        },
        FixtureFile {
            path: "Interface/FrameXML/Sounds.xml",
            bytes: br#"<Ui><Frame name="Owner"><Scripts>
<OnLoad>
 PlaySound("startup")
 PlaySoundFile("Sound/Direct.wav")
 self:RegisterEvent("PLAYER_ENTERING_WORLD")
 self:RegisterEvent("PLAYER_LOGIN")
 self:RegisterEvent("PLAYER_CONTROL_LOST")
 self:RegisterEvent("PLAYER_CONTROL_GAINED")
</OnLoad>
<OnEvent>
 PlaySound(event)
 if event == "PLAYER_CONTROL_LOST" then error("controlled failure") end
</OnEvent>
</Scripts></Frame></Ui>"#,
        },
    ];
    for path in [
        "DBFilesClient/gtChanceToMeleeCrit.dbc",
        "DBFilesClient/gtChanceToSpellCrit.dbc",
        "DBFilesClient/gtOCTRegenHP.dbc",
        "DBFilesClient/gtRegenHPPerSpt.dbc",
        "DBFilesClient/gtRegenMPPerSpt.dbc",
    ] {
        files.push(FixtureFile {
            path,
            bytes: &coefficients,
        });
    }
    let fixture = Fixture::new(&files)?;
    let archive =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = FrameManager::start_shared(
        AssetStoreHandle::new(AssetStore::mount(archive)?),
        UiScriptEnvironment::new(800, 600, false)?,
        &[],
        &AddonCatalog::default(),
    )?;
    assert_eq!(
        manager.take_media_action(),
        Some(UiGlueMediaAction::PlaySoundFile("Sound/Direct.wav".into()))
    );
    assert_eq!(manager.take_media_action(), None);
    manager.dispatch_event("PLAYER_ENTERING_WORLD", &UiEventPayload::empty())?;
    manager.with_suppressed_sound_entries(|manager| {
        manager.with_suppressed_sound_entries(|manager| {
            manager.dispatch_event("PLAYER_LOGIN", &UiEventPayload::empty())
        })
    })?;
    assert_eq!(manager.take_media_action(), None);
    assert!(
        manager
            .with_suppressed_sound_entries(|manager| {
                manager.dispatch_event("PLAYER_CONTROL_LOST", &UiEventPayload::empty())
            })
            .is_err()
    );
    manager.dispatch_event("PLAYER_CONTROL_GAINED", &UiEventPayload::empty())?;
    assert_eq!(
        manager.take_media_action(),
        Some(UiGlueMediaAction::PlaySound("PLAYER_CONTROL_GAINED".into()))
    );
    assert_eq!(manager.take_media_action(), None);
    Ok(())
}

/// Supplies deterministic character coefficient tables without stock archives.
fn zero_table(records: u32, fields: u32) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [records, fields, fields * 4, 1] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.resize(21 + records as usize * fields as usize * 4, 0);
    bytes
}
