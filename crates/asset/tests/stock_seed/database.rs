//! External stock-compatibility tests for WDBC bootstrap.

use std::error::Error;
use std::path::Path;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetStore, ClientDataRoot, Locale, WdbcTable,
};

use crate::support::{Fixture, FixtureFile};

/// WDBC parsing begins only after ordinary stock archive resolution completes.
#[test]
fn wdbc_bootstrap_uses_arbitrary_file_precedence() -> Result<(), Box<dyn Error>> {
    let base_table = create_wdbc(1, 2, &[1, 0], b"base\0");
    let patch_table = create_wdbc(1, 2, &[7, 11], b"patched\0");
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\Test.dbc",
            bytes: &base_table,
        },
        FixtureFile {
            archive: "patch-2.MPQ",
            path: "DBFilesClient\\Test.dbc",
            bytes: &patch_table,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let path = AssetPath::new("DBFilesClient/Test.dbc")?;

    let table = WdbcTable::load(&mut store, &path)?;

    assert_eq!(table.path(), &path);
    assert_eq!(table.source().relative_path(), Path::new("patch-2.MPQ"));
    assert_eq!(table.header().record_count(), 1);
    assert_eq!(table.header().field_count(), 2);
    assert_eq!(table.header().record_size(), 8);
    assert_eq!(table.header().string_block_size(), 8);
    assert_eq!(table.record(0), Some([7, 0, 0, 0, 11, 0, 0, 0].as_slice()));
    assert_eq!(table.record(1), None);
    assert_eq!(table.string_bytes(0), Some(b"patched".as_slice()));
    Ok(())
}

/// A truncated WDBC entry fails at the database boundary with its asset path.
#[test]
fn truncated_wdbc_is_rejected() -> Result<(), Box<dyn Error>> {
    let mut table = create_wdbc(1, 1, &[42], b"\0");
    table.truncate(table.len() - 1);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\Broken.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let path = AssetPath::new("DBFilesClient/Broken.dbc")?;

    assert!(matches!(
        WdbcTable::load(&mut store, &path),
        Err(AssetError::DatabaseDecode { path: failed, .. }) if failed == path
    ));
    Ok(())
}

/// Generates the fixed WDBC layout used by stock-era client tables.
fn create_wdbc(
    record_count: u32,
    field_count: u32,
    fields: &[u32],
    string_block: &[u8],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + string_block.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&record_count.to_le_bytes());
    bytes.extend_from_slice(&field_count.to_le_bytes());
    bytes.extend_from_slice(&(field_count * 4).to_le_bytes());
    bytes.extend_from_slice(&(string_block.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(string_block);
    bytes
}
