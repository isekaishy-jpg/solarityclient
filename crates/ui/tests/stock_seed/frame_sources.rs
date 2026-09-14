//! Worker preparation validates stock source order without executing Lua.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::FrameUiSources;

use crate::support::{Fixture, FixtureFile};

/// A valid source that fails if executed must still prepare on a foreign thread.
#[test]
fn frame_sources_cross_worker_boundary_without_executing_chunks() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(b"error('must only run with the live world environment')")?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let result = std::thread::spawn(move || FrameUiSources::load(&mut AssetStore::mount(catalog)?))
        .join()
        .map_err(|_| "source worker panicked")?;
    let _sources = result?;
    Ok(())
}

/// Moving archive validation must retain the first Lua syntax error and path.
#[test]
fn frame_sources_reject_invalid_lua_before_live_admission() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(b"local =")?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let Err(error) = FrameUiSources::load(&mut AssetStore::mount(catalog)?) else {
        return Err("invalid Lua was accepted".into());
    };
    let solarity_ui::GlueError::Load(solarity_ui::UiLoadError::Lua { path, .. }) = error else {
        return Err("wrong validation error".into());
    };
    assert_eq!(path.as_str(), "INTERFACE\\FRAMEXML\\FIRST.LUA");
    Ok(())
}

/// Both paths exercise XML Include expansion before the referenced Lua file.
fn fixture(lua: &[u8]) -> Result<Fixture, Box<dyn Error>> {
    Fixture::new(&[
        FixtureFile {
            path: "Interface/FrameXML/Bindings.xml",
            bytes: b"<Bindings/>",
        },
        FixtureFile {
            path: "WTF/DefaultBindings.wtf",
            bytes: b"",
        },
        FixtureFile {
            path: "Interface/FrameXML/FrameXML.toc",
            bytes: b"Root.xml\n",
        },
        FixtureFile {
            path: "Interface/FrameXML/Root.xml",
            bytes: b"<Ui><Include file=\"Nested.xml\"/></Ui>",
        },
        FixtureFile {
            path: "Interface/FrameXML/Nested.xml",
            bytes: b"<Ui><Script file=\"First.lua\"/></Ui>",
        },
        FixtureFile {
            path: "Interface/FrameXML/First.lua",
            bytes: lua,
        },
    ])
}
