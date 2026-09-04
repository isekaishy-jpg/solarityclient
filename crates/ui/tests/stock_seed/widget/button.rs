//! External stock-compatibility tests for native Button state.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::GlueManager;

use crate::support::{Fixture, FixtureFile};

/// Script Click enters the checkbox override before Button's enabled and
/// recursion guards: Wow.exe 0x00978260 -> 0x009623C0 -> 0x0096FD70.
#[test]
fn checkbox_script_click_preserves_native_toggle_and_recursion_order() -> Result<(), Box<dyn Error>>
{
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"CheckClick.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\CheckClick.xml",
            bytes: br#"<Ui>
<CheckButton name="ScriptCheck"><Scripts>
  <PreClick>CLICK_LOG = CLICK_LOG .. "pre:" .. tostring(self:GetChecked()) .. ";"</PreClick>
  <OnClick>
    CLICK_LOG = CLICK_LOG .. "click:" .. tostring(self:GetChecked()) .. ";"
    if RECURSE then self:Click() end
  </OnClick>
  <PostClick>CLICK_LOG = CLICK_LOG .. "post:" .. tostring(self:GetChecked()) .. ";"</PostClick>
</Scripts></CheckButton>
<Button name="ErrorButton"><Scripts><OnClick>
  ERROR_CALLS = ERROR_CALLS + 1
  error("fixture error")
</OnClick></Scripts></Button>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1280, 720), false)?;
    manager
        .bundle()
        .lua()
        .load(
            r#"
      CLICK_LOG = ""
      ScriptCheck:Click()
      assert(CLICK_LOG == "pre:true;click:true;post:true;")
      assert(ScriptCheck:GetChecked())
      CLICK_LOG = ""
      ScriptCheck:Disable()
      ScriptCheck:Click()
      assert(not ScriptCheck:GetChecked() and CLICK_LOG == "")
      ScriptCheck:Enable()
      RECURSE = true
      ScriptCheck:Click()
      assert(CLICK_LOG == "pre:true;click:true;post:false;")
      assert(not ScriptCheck:GetChecked())
      ERROR_CALLS = 0
      assert(not pcall(function() ErrorButton:Click() end))
      assert(not pcall(function() ErrorButton:Click() end))
      assert(ERROR_CALLS == 2)
    "#,
        )
        .exec()?;
    Ok(())
}

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
