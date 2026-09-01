//! Exercises real locale-backed first-run agreements and later-run bypass.

use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;

use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
use solarity_ui::{
    GlueInitialScreen, GlueManager, UiGlueNetworkAction, UiKeyboardModifiers, UiPointerButton,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os();
    let _executable = arguments.next();
    let data_root = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(usage_error)?;
    let locale = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage_error)?
        .parse::<Locale>()?;
    if arguments.next().is_some() {
        return Err(usage_error().into());
    }

    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(data_root.clone())?, locale)?;
    let mut manager = GlueManager::start_shared_with_initial_screen_and_cvars(
        AssetStoreHandle::new(AssetStore::mount(catalog)?),
        (1280, 720),
        false,
        GlueInitialScreen::Login,
        &[
            ("readEULA".to_owned(), "-1".to_owned()),
            ("readTOS".to_owned(), "-1".to_owned()),
            ("readTerminationWithoutNotice".to_owned(), "1".to_owned()),
            ("readScanning".to_owned(), "1".to_owned()),
            ("readContest".to_owned(), "1".to_owned()),
        ],
    )?;

    accept_notice(&mut manager, "EULAScrollFrame", "TOSAccept", "readEULA")?;
    accept_notice(&mut manager, "TOSScrollFrame", "TOSAccept", "readTOS")?;
    let changes = manager.take_changed_cvars();
    if manager.current_screen() != "login"
        || !contains_change(&changes, "readEULA", "1")
        || !contains_change(&changes, "readTOS", "1")
    {
        return Err(invalid_data(format!(
            "first-run agreements did not reach login with persistent acceptance: screen={}, changes={changes:?}",
            manager.current_screen()
        ))
        .into());
    }
    drop(manager);

    let accepted_cvars = [
        ("readEULA".to_owned(), "1".to_owned()),
        ("readTOS".to_owned(), "1".to_owned()),
        ("readTerminationWithoutNotice".to_owned(), "1".to_owned()),
        ("readScanning".to_owned(), "1".to_owned()),
        ("readContest".to_owned(), "1".to_owned()),
    ];
    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(data_root)?, locale)?;
    let mut later = GlueManager::start_shared_with_initial_screen_and_cvars(
        AssetStoreHandle::new(AssetStore::mount(catalog)?),
        (1280, 720),
        false,
        GlueInitialScreen::Login,
        &accepted_cvars,
    )?;
    if later.current_screen() != "login"
        || !object_is_shown(&later, "AccountLoginUI")?
        || object_is_shown(&later, "TOSFrame")?
        || later.media_intent().movie().is_some()
    {
        return Err(invalid_data(
            "accepted later-run state did not bypass MOV and legal notices".to_owned(),
        )
        .into());
    }
    validate_login_input(&mut later)?;
    println!(
        "validated first-run agreements, later-run bypass, and authored login input: changes={changes:?}"
    );
    Ok(())
}

fn validate_login_input(manager: &mut GlueManager) -> Result<(), Box<dyn Error>> {
    let account_index = object_index(manager, "AccountLoginAccountEdit")?;
    let password_index = object_index(manager, "AccountLoginPasswordEdit")?;
    if manager.focused_edit_box() != Some(account_index) {
        return Err(
            invalid_data("stock login did not focus the account EditBox".to_owned()).into(),
        );
    }
    manager.text_input("VALIDATION_ACCOUNT")?;
    if manager.keyboard_key("TAB", true, UiKeyboardModifiers::default())? != Some(account_index)
        || manager.focused_edit_box() != Some(password_index)
    {
        return Err(
            invalid_data("stock login Tab did not focus the password EditBox".to_owned()).into(),
        );
    }
    manager.text_input("validation-secret")?;
    manager.keyboard_key("ENTER", true, UiKeyboardModifiers::default())?;
    let Some(UiGlueNetworkAction::Login(request)) = manager.take_network_action() else {
        return Err(
            invalid_data("stock login Enter did not emit a login request".to_owned()).into(),
        );
    };
    if request.account_name() != "VALIDATION_ACCOUNT"
        || request.password_bytes() != b"validation-secret"
    {
        return Err(invalid_data(
            "stock login request did not retain entered credentials".to_owned(),
        )
        .into());
    }
    let globals = manager.bundle().lua().globals();
    let password = globals.get::<mlua::Table>("AccountLoginPasswordEdit")?;
    let get_text = password.get::<mlua::Function>("GetText")?;
    if !get_text.call::<String>(password)?.is_empty() {
        return Err(
            invalid_data("stock login did not clear submitted password text".to_owned()).into(),
        );
    }
    Ok(())
}

fn contains_change(changes: &[(String, String)], name: &str, value: &str) -> bool {
    changes
        .iter()
        .any(|(changed_name, changed_value)| changed_name == name && changed_value == value)
}

fn object_is_shown(manager: &GlueManager, name: &str) -> Result<bool, IoError> {
    let index = object_index(manager, name)?;
    Ok(manager
        .geometry()
        .region(index)
        .is_some_and(solarity_ui::UiRegionGeometry::effectively_shown))
}

fn accept_notice(
    manager: &mut GlueManager,
    scroll_name: &str,
    accept_name: &str,
    cvar: &str,
) -> Result<(), Box<dyn Error>> {
    let scroll_index = object_index(manager, scroll_name)?;
    let position = object_center(manager, scroll_index)?;
    let initial = manager
        .scroll_frames()
        .state(scroll_index)
        .ok_or_else(|| invalid_data(format!("{scroll_name} has no live scroll state")))?;
    println!(
        "{scroll_name}: offset={}, range={}",
        initial.offset().1,
        initial.range().1
    );
    if initial.offset().1 < initial.range().1 {
        if manager.pointer_wheel(position, -1.0)? != Some(scroll_index) {
            return Err(
                invalid_data(format!("{scroll_name} did not retain wheel targeting")).into(),
            );
        }
        let advanced = manager
            .scroll_frames()
            .state(scroll_index)
            .ok_or_else(|| invalid_data(format!("{scroll_name} disappeared after scrolling")))?;
        let step = advanced.offset().1 - initial.offset().1;
        if step <= 0.0 {
            return Err(invalid_data(format!(
                "{scroll_name} authored wheel callback did not advance"
            ))
            .into());
        }
        let remaining = ((advanced.range().1 - advanced.offset().1) / step)
            .ceil()
            .max(0.0) as usize;
        for _ in 0..remaining {
            if manager.pointer_wheel(position, -1.0)? != Some(scroll_index) {
                return Err(invalid_data(format!("{scroll_name} lost wheel targeting")).into());
            }
        }
    }
    let state = manager
        .scroll_frames()
        .state(scroll_index)
        .ok_or_else(|| invalid_data(format!("{scroll_name} disappeared after scrolling")))?;
    if state.offset().1 < state.range().1 {
        return Err(invalid_data(format!("{scroll_name} did not reach its authored range")).into());
    }

    let accept_index = object_index(manager, accept_name)?;
    let accept_position = object_center(manager, accept_index)?;
    let enabled = {
        let globals = manager.bundle().lua().globals();
        let button = globals.get::<mlua::Table>(accept_name)?;
        let is_enabled = button.get::<mlua::Function>("IsEnabled")?;
        is_enabled.call::<bool>(button)?
    };
    println!(
        "{accept_name}: enabled={enabled}, shown={}",
        manager
            .geometry()
            .region(accept_index)
            .is_some_and(solarity_ui::UiRegionGeometry::effectively_shown)
    );
    let down = manager.pointer_button(accept_position, UiPointerButton::Left, true)?;
    let up = manager.pointer_button(accept_position, UiPointerButton::Left, false)?;
    if down.object_index() != Some(accept_index)
        || up.object_index() != Some(accept_index)
        || !up.click_activated()
    {
        return Err(invalid_data(format!(
            "{accept_name} did not receive captured activation: down={down:?}, up={up:?}"
        ))
        .into());
    }
    if manager.cvar_value(cvar).as_deref() != Some("1") {
        return Err(invalid_data(format!("{accept_name} did not persist {cvar}")).into());
    }
    Ok(())
}

fn object_index(manager: &GlueManager, name: &str) -> Result<usize, IoError> {
    manager
        .objects()
        .iter()
        .position(|object| object.name() == Some(name))
        .ok_or_else(|| invalid_data(format!("Glue object {name} is unavailable")))
}

fn object_center(manager: &GlueManager, object_index: usize) -> Result<(f64, f64), IoError> {
    let bounds = manager
        .geometry()
        .region(object_index)
        .ok_or_else(|| invalid_data(format!("Glue object {object_index} has no geometry")))?
        .presentation_bounds();
    Ok((
        bounds.left() + bounds.width() * 0.5,
        bounds.bottom() + bounds.height() * 0.5,
    ))
}

fn invalid_data(message: String) -> IoError {
    IoError::new(ErrorKind::InvalidData, message)
}

fn usage_error() -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        "usage: validate_glue_interaction <Data> <locale>",
    )
}
