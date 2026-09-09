//! Synchronous Lua logout admission and server-owned camping notifications.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use mlua::{Lua, Table, Value, Variadic};

use crate::{UiProcessAction, UiScriptEnvironment};

/// One admitted empty logout packet, retained until the world writer accepts it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiLogoutAction {
    /// Normal CMSG_LOGOUT_REQUEST.
    Request,
    /// CMSG_LOGOUT_CANCEL, after clearing local pending state.
    Cancel,
    /// CMSG_PLAYER_LOGOUT, bypassing the pending guard.
    Force,
}

/// The pending and quit flags used by native 6B1930 and its response callbacks.
#[derive(Clone, Debug, Default)]
pub struct UiLogoutState(Rc<RefCell<LogoutState>>);

/// Shared with Lua so multiple calls in one handler observe prior admissions.
#[derive(Debug, Default)]
struct LogoutState {
    pending: bool,
    quit: bool,
    actions: VecDeque<UiLogoutAction>,
}

impl UiLogoutState {
    /// Drops flags and unsent requests with their owning world.
    pub(crate) fn clear(&self) {
        *self.0.borrow_mut() = LogoutState::default();
    }
    /// 6B1930 rejects repeat requests, but its forced path bypasses that guard.
    fn request(&self, quit: bool, force: bool) {
        let mut state = self.0.borrow_mut();
        if state.pending && !force {
            return;
        }
        state.quit = quit;
        if !force {
            state.pending = true;
        }
        state.actions.push_back(if force {
            UiLogoutAction::Force
        } else {
            UiLogoutAction::Request
        });
    }

    /// 6B18C0 clears pending immediately, before the cancellation acknowledgment.
    fn cancel(&self) {
        let mut state = self.0.borrow_mut();
        state.actions.push_back(UiLogoutAction::Cancel);
        state.pending = false;
    }

    /// Returns the oldest request without losing it to writer backpressure.
    pub fn pending_action(&self) -> Option<UiLogoutAction> {
        self.0.borrow().actions.front().copied()
    }

    /// Acknowledges a request accepted by the world writer.
    pub fn accept_action(&self) {
        self.0.borrow_mut().actions.pop_front();
    }

    /// 6B08B0 emits camping only for an accepted, noninstant response.
    pub fn response(&self, reason: u32, instant: bool) -> Option<&'static str> {
        let state = self.0.borrow();
        if reason != 0 {
            return Some("UI_ERROR_MESSAGE");
        }
        if instant {
            return None;
        }
        Some(if state.quit {
            "PLAYER_QUITING"
        } else {
            "PLAYER_CAMPING"
        })
    }

    /// 6B0900 only emits cancellation while the native pending bit remains set.
    pub fn cancel_acknowledged(&self) -> Option<&'static str> {
        self.0.borrow().pending.then_some("LOGOUT_CANCEL")
    }

    /// Clears pending after the failure/cancellation event's Lua handlers return.
    pub fn finish_cancellation(&self) {
        self.0.borrow_mut().pending = false;
    }

    /// Captures 6B2180's quit destination before world state is retired.
    pub fn quits_process(&self) -> bool {
        self.0.borrow().quit
    }
}

/// Installs world Logout/Quit and their force/cancel counterparts (510430..51AC90).
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    environment: &UiScriptEnvironment,
) -> mlua::Result<()> {
    for (name, quit, force) in [
        ("Logout", false, false),
        ("Quit", true, false),
        ("ForceLogout", false, true),
    ] {
        let world = environment.world_state();
        globals.raw_set(
            name,
            lua.create_function(move |_, _: Variadic<Value>| {
                if world.player().is_some() {
                    world.logout().request(quit, force);
                }
                Ok(())
            })?,
        )?;
    }
    let world = environment.world_state();
    globals.raw_set(
        "CancelLogout",
        lua.create_function(move |_, _: Variadic<Value>| {
            if world.player().is_some() {
                world.logout().cancel();
            }
            Ok(())
        })?,
    )?;
    let process = environment.process();
    globals.raw_set(
        "ForceQuit",
        lua.create_function(move |_, _: Variadic<Value>| {
            process.borrow_mut().push(UiProcessAction::Quit);
            Ok(())
        })?,
    )
}

#[cfg(test)]
#[path = "../../tests/stock_seed/world_logout.rs"]
mod tests;
