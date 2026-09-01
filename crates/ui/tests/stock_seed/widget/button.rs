//! External stock-compatibility tests for native Button state.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::GlueManager;

use crate::support::{Fixture, FixtureFile};

/// Stock rotation scripts observe the normal/pushed pair, while the optional
/// lock argument keeps an authored pushed state until explicitly released.
#[test]
fn button_state_round_trips_stock_names_and_lock() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Button.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Button.xml",
            bytes: br#"<Ui><Button name="StateButton"><Size x="80" y="30"/></Button></Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1280, 720), false)?;
    let button: mlua::Table = manager.bundle().lua().globals().get("StateButton")?;
    let get_state: mlua::Function = button.get("GetButtonState")?;
    let set_state: mlua::Function = button.get("SetButtonState")?;

    assert_eq!(get_state.call::<String>(button.clone())?, "NORMAL");
    set_state.call::<()>((button.clone(), "PUSHED", true))?;
    assert_eq!(get_state.call::<String>(button.clone())?, "PUSHED");
    set_state.call::<()>((button.clone(), "NORMAL", false))?;
    assert_eq!(get_state.call::<String>(button)?, "NORMAL");
    Ok(())
}
