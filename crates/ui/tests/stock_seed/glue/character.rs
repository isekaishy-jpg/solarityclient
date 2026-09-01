//! External stock-compatibility tests for character-selection Glue state.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{GlueManager, UiCharacterDirectory, UiCharacterInfo, UiGlueNetworkAction};

use crate::support::{Fixture, FixtureFile};

/// Character globals retain server order, stock return arity, and selected-ID
/// actions without exposing protocol types to the UI crate.
#[test]
fn glue_manager_bridges_character_selection_globals() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Character.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Character.xml",
            bytes: br#"<Ui>
  <ModelFFX name="CharacterSelect">
    <Scripts><OnLoad>SetCharSelectModelFrame("CharacterSelect")</OnLoad></Scripts>
  </ModelFFX>
  <ModelFFX name="CharacterCreate">
    <Scripts><OnLoad>SetCharCustomizeFrame("CharacterCreate")</OnLoad></Scripts>
  </ModelFFX>
</Ui>"#,
        },
        FixtureFile {
            path: "Interface\\Glues\\Models\\UI_Human\\UI_Human.m2",
            bytes: b"select model fixture",
        },
        FixtureFile {
            path: "Interface\\Glues\\Models\\UI_Dwarf\\UI_Dwarf.m2",
            bytes: b"customize model fixture",
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    manager.set_character_directory(UiCharacterDirectory::new(vec![
        UiCharacterInfo::new(
            100,
            "First".to_owned(),
            "Human".to_owned(),
            "Human".to_owned(),
            "Mage".to_owned(),
            8,
            80,
            Some("Dalaran".to_owned()),
            3,
            0x2000,
            1,
        ),
        UiCharacterInfo::new(
            200,
            "Second".to_owned(),
            "Orc".to_owned(),
            "Orc".to_owned(),
            "Death Knight".to_owned(),
            6,
            55,
            None,
            2,
            0,
            0x0010_0000,
        ),
    ]));
    let globals = manager.bundle().lua().globals();

    assert_eq!(
        globals
            .get::<mlua::Function>("GetNumCharacters")?
            .call::<usize>(())?,
        2
    );
    let id_from_index = globals.get::<mlua::Function>("GetCharIDFromIndex")?;
    assert_eq!(id_from_index.call::<u64>(1_u32)?, 100);
    let index_from_id = globals.get::<mlua::Function>("GetIndexFromCharID")?;
    assert_eq!(index_from_id.call::<u32>(200_u64)?, 2);
    let info = globals
        .get::<mlua::Function>("GetCharacterInfo")?
        .call::<mlua::MultiValue>(1_u32)?;
    assert_eq!(info.len(), 11);
    assert_eq!(
        info[0].as_string().map(|value| value.to_string_lossy()),
        Some("First".to_owned())
    );
    assert_eq!(
        info[1].as_string().map(|value| value.to_string_lossy()),
        Some("Human".to_owned())
    );
    assert_eq!(
        info[2].as_string().map(|value| value.to_string_lossy()),
        Some("Mage".to_owned())
    );
    assert_eq!(
        info[4].as_string().map(|value| value.to_string_lossy()),
        Some("Dalaran".to_owned())
    );
    assert_eq!(info[5].as_integer(), Some(3));
    assert_eq!(info[6].as_boolean(), Some(true));
    assert_eq!(info[7].as_boolean(), Some(true));
    let background = globals.get::<mlua::Function>("GetSelectBackgroundModel")?;
    assert_eq!(background.call::<String>(2_u32)?, "DEATHKNIGHT");
    globals
        .get::<mlua::Function>("SetCharSelectBackground")?
        .call::<()>("Interface\\Glues\\Models\\UI_Human\\UI_Human.m2")?;
    globals
        .get::<mlua::Function>("SetCharCustomizeBackground")?
        .call::<()>("Interface\\Glues\\Models\\UI_Dwarf\\UI_Dwarf.m2")?;
    for (frame_name, expected_path) in [
        (
            "CharacterSelect",
            "INTERFACE\\GLUES\\MODELS\\UI_HUMAN\\UI_HUMAN.M2",
        ),
        (
            "CharacterCreate",
            "INTERFACE\\GLUES\\MODELS\\UI_DWARF\\UI_DWARF.M2",
        ),
    ] {
        let frame = globals.get::<mlua::Table>(frame_name)?;
        assert_eq!(
            frame
                .get::<mlua::Function>("GetModel")?
                .call::<String>(frame)?,
            expected_path
        );
    }

    globals
        .get::<mlua::Function>("ReadyForAccountDataTimes")?
        .call::<()>(())?;
    globals
        .get::<mlua::Function>("GetCharacterListUpdate")?
        .call::<()>(())?;
    globals
        .get::<mlua::Function>("RequestRealmSplitInfo")?
        .call::<()>(())?;
    assert!(matches!(
        manager.take_network_action(),
        Some(UiGlueNetworkAction::ReadyForAccountDataTimes)
    ));
    assert!(matches!(
        manager.take_network_action(),
        Some(UiGlueNetworkAction::RequestCharacterListUpdate)
    ));
    assert!(matches!(
        manager.take_network_action(),
        Some(UiGlueNetworkAction::RequestRealmSplitInfo)
    ));

    globals
        .get::<mlua::Function>("SelectCharacter")?
        .call::<()>(2_u32)?;
    globals
        .get::<mlua::Function>("EnterWorld")?
        .call::<()>(())?;
    assert!(matches!(
        manager.take_network_action(),
        Some(UiGlueNetworkAction::SelectCharacter { guid: 200 })
    ));
    assert!(matches!(
        manager.take_network_action(),
        Some(UiGlueNetworkAction::EnterWorld { guid: 200 })
    ));
    Ok(())
}
