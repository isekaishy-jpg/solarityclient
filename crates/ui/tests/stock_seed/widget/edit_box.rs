//! External stock-compatibility tests for native EditBox input routing.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{GlueManager, UiGlueNetworkAction, UiKeyboardModifiers, UiPointerButton};

use crate::support::{Fixture, FixtureFile};

/// Focused text input preserves UTF-8 editing, legacy callback values, authored
/// Tab focus transfer, and Enter submission through `DefaultServerLogin`.
#[test]
fn focused_edit_box_routes_native_login_input() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Edit.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Edit.xml",
            bytes: br#"<Ui><Frame name="Root" enableKeyboard="true">
  <Size x="600" y="400"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Frames>
    <EditBox name="Account" letters="16">
      <Size x="200" y="40"/><Anchors><Anchor point="CENTER"><Offset x="0" y="40"/></Anchor></Anchors>
      <TextInsets><AbsInset left="12" right="5" bottom="5"/></TextInsets>
      <Scripts><OnLoad>
        self:SetText("saved")
        self:SetFocus()
      </OnLoad><OnChar>
        INPUT_LOG = (INPUT_LOG or "") .. "char:" .. text .. ";"
      </OnChar><OnTextChanged>
        INPUT_LOG = (INPUT_LOG or "") .. "text:" .. tostring(userInput) .. ";"
      </OnTextChanged><OnCharComposition>
        INPUT_LOG = (INPUT_LOG or "") .. "compose:" .. text .. ";"
      </OnCharComposition><OnEditFocusGained>
        INPUT_LOG = (INPUT_LOG or "") .. "account-gain;"
        self:HighlightText()
      </OnEditFocusGained><OnEditFocusLost>
        INPUT_LOG = (INPUT_LOG or "") .. "account-loss;"
      </OnEditFocusLost><OnTabPressed>
        Password:SetFocus()
      </OnTabPressed></Scripts>
    </EditBox>
    <EditBox name="Password" letters="16" password="true">
      <Size x="200" y="40"/><Anchors><Anchor point="CENTER"><Offset x="0" y="-40"/></Anchor></Anchors>
      <Scripts><OnEditFocusGained>
        INPUT_LOG = (INPUT_LOG or "") .. "password-gain;"
        self:HighlightText()
      </OnEditFocusGained><OnEditFocusLost>
        INPUT_LOG = (INPUT_LOG or "") .. "password-loss;"
      </OnEditFocusLost><OnEnterPressed>
        DefaultServerLogin(Account:GetText(), self:GetText())
        self:SetText("")
      </OnEnterPressed></Scripts>
    </EditBox>
  </Frames><Scripts><OnKeyDown>
    ROOT_KEY = key
  </OnKeyDown></Scripts>
</Frame></Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1280, 720), false)?;
    let account_index = object_index(&manager, "Account")?;
    let password_index = object_index(&manager, "Password")?;
    let globals = manager.bundle().lua().globals();
    globals.raw_set("INPUT_LOG", "")?;

    assert_eq!(manager.focused_edit_box(), Some(account_index));
    assert!(manager.update(0.5)?);
    assert!(!manager.update(0.25)?);
    assert!(manager.update(0.25)?);
    assert_eq!(manager.text_input("Alice")?, Some(account_index));
    let account: mlua::Table = globals.get("Account")?;
    let get_account_text: mlua::Function = account.get("GetText")?;
    let get_max_letters: mlua::Function = account.get("GetMaxLetters")?;
    let get_text_insets: mlua::Function = account.get("GetTextInsets")?;
    let get_justify_h: mlua::Function = account.get("GetJustifyH")?;
    assert_eq!(get_max_letters.call::<u32>(account.clone())?, 16);
    assert_eq!(
        get_text_insets.call::<(f64, f64, f64, f64)>(account.clone())?,
        (12.0, 5.0, 0.0, 5.0)
    );
    assert_eq!(get_account_text.call::<String>(account.clone())?, "Alice");
    assert_eq!(get_justify_h.call::<String>(account.clone())?, "LEFT");
    assert_eq!(manager.text_composition("候補")?, Some(account_index));

    assert_eq!(
        manager.keyboard_key("TAB", true, UiKeyboardModifiers::default())?,
        Some(account_index)
    );
    assert_eq!(manager.focused_edit_box(), Some(password_index));
    assert_eq!(manager.text_input("Sécret")?, Some(password_index));
    assert_eq!(
        manager.keyboard_key("BACKSPACE", true, UiKeyboardModifiers::default())?,
        Some(password_index)
    );
    let password: mlua::Table = globals.get("Password")?;
    let get_password_text: mlua::Function = password.get("GetText")?;
    let is_password: mlua::Function = password.get("IsPassword")?;
    assert_eq!(
        is_password.call::<Option<bool>>(password.clone())?,
        Some(true)
    );
    assert_eq!(get_password_text.call::<String>(password.clone())?, "Sécre");

    assert_eq!(
        manager.keyboard_key("ENTER", true, UiKeyboardModifiers::default())?,
        Some(password_index)
    );
    let UiGlueNetworkAction::Login(request) = manager
        .take_network_action()
        .ok_or("missing authored login request")?
    else {
        return Err("authored EditBox Enter did not submit login".into());
    };
    assert_eq!(request.account_name(), "Alice");
    assert_eq!(request.password_bytes(), "Sécre".as_bytes());
    assert_eq!(get_password_text.call::<String>(password.clone())?, "");
    let log = globals.get::<String>("INPUT_LOG")?;
    assert!(log.contains("text:true;char:A;"));
    assert!(log.contains("compose:候補;"));
    assert!(log.contains("account-loss;password-gain;"));

    let account_position = object_center(&manager, account_index)?;
    assert_eq!(
        manager
            .pointer_button(account_position, UiPointerButton::Left, true)?
            .object_index(),
        Some(account_index)
    );
    assert_eq!(manager.focused_edit_box(), Some(account_index));
    Ok(())
}

fn object_index(manager: &GlueManager, name: &str) -> Result<usize, Box<dyn Error>> {
    manager
        .objects()
        .iter()
        .position(|object| object.name() == Some(name))
        .ok_or_else(|| format!("missing object {name}").into())
}

fn object_center(manager: &GlueManager, object_index: usize) -> Result<(f64, f64), Box<dyn Error>> {
    let bounds = manager
        .geometry()
        .region(object_index)
        .ok_or("missing EditBox geometry")?
        .presentation_bounds();
    Ok((
        bounds.left() + bounds.width() * 0.5,
        bounds.bottom() + bounds.height() * 0.5,
    ))
}
