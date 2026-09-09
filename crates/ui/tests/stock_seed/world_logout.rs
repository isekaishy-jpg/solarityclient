//! 6B1930 admission, 6B08B0 responses and 6B0900 cancellation acknowledgment.

use super::*;

#[test]
fn logout_lua_admission_and_callbacks_follow_native_pending_flags()
-> Result<(), Box<dyn std::error::Error>> {
    let lua = Lua::new();
    let environment = UiScriptEnvironment::new(800, 600, false)?;
    let world = environment.world_state();
    let state = world.logout();
    register_globals(&lua, &lua.globals(), &environment)?;
    lua.load("Logout(); Quit(); ForceLogout(); CancelLogout()")
        .exec()?;
    assert_eq!(state.pending_action(), None);
    world.enter_player(crate::UiPlayerState::new(0));
    lua.load("Logout(); Quit(); Logout()").exec()?;
    assert_eq!(state.pending_action(), Some(UiLogoutAction::Request));
    state.accept_action();
    assert_eq!(state.pending_action(), None);
    assert!(!state.quits_process());
    assert_eq!(state.response(0, false), Some("PLAYER_CAMPING"));
    assert_eq!(state.response(0, true), None);
    lua.load("CancelLogout()").exec()?;
    assert_eq!(state.pending_action(), Some(UiLogoutAction::Cancel));
    state.accept_action();
    assert_eq!(state.cancel_acknowledged(), None);
    lua.load("Quit()").exec()?;
    state.accept_action();
    assert!(state.quits_process());
    assert_eq!(state.response(0, false), Some("PLAYER_QUITING"));
    assert_eq!(state.cancel_acknowledged(), Some("LOGOUT_CANCEL"));
    lua.load("Logout()").exec()?;
    assert_eq!(
        state.pending_action(),
        None,
        "native pending flag remains set during LOGOUT_CANCEL handlers"
    );
    state.finish_cancellation();
    assert_eq!(state.cancel_acknowledged(), None);
    lua.load("Quit(); ForceLogout()").exec()?;
    assert_eq!(state.pending_action(), Some(UiLogoutAction::Request));
    state.accept_action();
    assert_eq!(state.pending_action(), Some(UiLogoutAction::Force));
    state.accept_action();
    assert!(!state.quits_process());
    assert_eq!(state.response(u32::MAX, false), Some("UI_ERROR_MESSAGE"));
    lua.load("Logout()").exec()?;
    assert_eq!(
        state.pending_action(),
        None,
        "native pending flag remains set during UI_ERROR_MESSAGE handlers"
    );
    state.finish_cancellation();
    lua.load("Logout()").exec()?;
    assert_eq!(state.pending_action(), Some(UiLogoutAction::Request));
    lua.load("ForceQuit()").exec()?;
    assert_eq!(
        environment.process().borrow_mut().take(),
        Some(UiProcessAction::Quit)
    );
    Ok(())
}
