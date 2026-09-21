//! Callback aliases and owner globals survive retirement of the compiling Lua state.
use super::*;

#[test]
fn callback_transfer_preserves_aliases_globals_and_debug_source()
-> Result<(), Box<dyn std::error::Error>> {
    let prepared = std::thread::spawn(|| -> Result<_, UiScriptError> {
        let lua = Lua::new();
        let function: Function = lua.load("return function(self) HOST_CHECK(); self.value = OWNER_VALUE; return self.value end")
            .set_name("fixture.xml:*:OnLoad").eval().map_err(transfer_error)?;
        let mut bank = ExportFunctions::default();
        let first = bank.export(&lua, vec![lua.create_registry_value(function.clone()).map_err(transfer_error)?])?;
        let second = bank.export(&lua, vec![lua.create_registry_value(function).map_err(transfer_error)?])?;
        Ok((first, second))
    }).join().map_err(|_| "compiler panicked")??;
    let lua = Lua::new();
    let owner = std::thread::current().id();
    lua.globals().set("OWNER_VALUE", 37)?;
    lua.globals().set(
        "HOST_CHECK",
        lua.create_function(move |_, ()| {
            assert_eq!(std::thread::current().id(), owner);
            Ok(())
        })?,
    )?;
    let mut bank = BindFunctions::default();
    let first = bank.bind(&lua, prepared.0)?;
    let second = bank.bind(&lua, prepared.1)?;
    let first: Function = lua.registry_value(&first[0])?;
    let second: Function = lua.registry_value(&second[0])?;
    drop(bank);
    assert_eq!(first.to_pointer(), second.to_pointer());
    let object = lua.create_table()?;
    assert_eq!(first.call::<u32>(object.clone())?, 37);
    assert_eq!(object.get::<u32>("value")?, 37);
    let error = first
        .call::<()>(())
        .err()
        .ok_or("nil self must fail")?
        .to_string();
    assert!(error.contains("fixture.xml:*:OnLoad"), "{error}");
    Ok(())
}
