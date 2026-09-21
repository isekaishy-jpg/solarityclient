//! Transfer compiled callback code without sharing a Lua state across threads.

use crate::UiScriptError;
use mlua::{Function, Lua, RegistryKey, chunk::ChunkMode};
use std::{collections::HashMap, sync::Arc};

pub(crate) type PreparedFunctions = Vec<Arc<Vec<u8>>>;

#[derive(Default)]
pub(crate) struct ExportFunctions(HashMap<usize, (Function, Arc<Vec<u8>>)>);

impl ExportFunctions {
    pub(crate) fn export(
        &mut self,
        lua: &Lua,
        keys: Vec<RegistryKey>,
    ) -> Result<PreparedFunctions, UiScriptError> {
        keys.into_iter()
            .map(|key| {
                let function: Function = lua.registry_value(&key).map_err(transfer_error)?;
                Ok(self
                    .0
                    .entry(function.to_pointer() as usize)
                    .or_insert_with(|| (function.clone(), Arc::new(function.dump(false))))
                    .1
                    .clone())
            })
            .collect()
    }
}

#[derive(Default)]
pub(crate) struct BindFunctions(HashMap<usize, (Arc<Vec<u8>>, Function)>);

impl BindFunctions {
    pub(crate) fn bind(
        &mut self,
        lua: &Lua,
        functions: PreparedFunctions,
    ) -> Result<Vec<RegistryKey>, UiScriptError> {
        functions
            .iter()
            .map(|code| {
                let identity = Arc::as_ptr(code) as usize;
                let function = match self.0.entry(identity) {
                    std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::hash_map::Entry::Vacant(entry) => entry.insert((
                        code.clone(),
                        lua.load(code.as_slice())
                            .set_mode(ChunkMode::Binary)
                            .into_function()
                            .map_err(transfer_error)?,
                    )),
                };
                lua.create_registry_value(function.1.clone())
                    .map_err(transfer_error)
            })
            .collect()
    }
}

fn transfer_error(error: mlua::Error) -> UiScriptError {
    UiScriptError::Plan {
        message: format!("compiled callback transfer: {error}"),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/prepared_callbacks.rs"]
mod tests;
