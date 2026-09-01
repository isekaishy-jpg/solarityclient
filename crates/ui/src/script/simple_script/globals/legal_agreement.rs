//! Native legal-agreement globals used by stock AccountLogin Lua.

use mlua::{Lua, Table};

use super::super::cvars::UiCVarRegistry;

const AGREEMENTS: [(&str, &str, &str, &str); 5] = [
    ("EULAAccepted", "AcceptEULA", "ShowEULANotice", "readEULA"),
    ("TOSAccepted", "AcceptTOS", "ShowTOSNotice", "readTOS"),
    (
        "TerminationWithoutNoticeAccepted",
        "AcceptTerminationWithoutNotice",
        "ShowTerminationWithoutNoticeNotice",
        "readTerminationWithoutNotice",
    ),
    (
        "ScanningAccepted",
        "AcceptScanning",
        "ShowScanningNotice",
        "readScanning",
    ),
    (
        "ContestAccepted",
        "AcceptContest",
        "ShowContestNotice",
        "readContest",
    ),
];

/// Registers the exact query, accept, and first-display notice families.
pub(super) fn register_globals(
    lua: &Lua,
    globals: &Table,
    cvars: UiCVarRegistry,
) -> mlua::Result<()> {
    for (query_name, accept_name, notice_name, cvar_name) in AGREEMENTS {
        let query_state = cvars.clone();
        globals.raw_set(
            query_name,
            lua.create_function(move |_, ()| Ok(read_status(&query_state, cvar_name)? == 1))?,
        )?;

        let accept_state = cvars.clone();
        globals.raw_set(
            accept_name,
            lua.create_function(move |_, ()| {
                accept_state
                    .set(cvar_name, "1".to_owned())
                    .map_err(|error| {
                        mlua::Error::runtime(format!("could not accept {cvar_name}: {error:?}"))
                    })
            })?,
        )?;

        let notice_state = cvars.clone();
        globals.raw_set(
            notice_name,
            lua.create_function(move |_, ()| Ok(read_status(&notice_state, cvar_name)? < 0))?,
        )?;
    }
    Ok(())
}

fn read_status(cvars: &UiCVarRegistry, name: &str) -> mlua::Result<i32> {
    cvars
        .get(name)
        .ok_or_else(|| mlua::Error::runtime(format!("stock {name} CVar is not registered")))?
        .parse::<i32>()
        .map_err(|_source| mlua::Error::runtime(format!("stock {name} CVar is not an integer")))
}
