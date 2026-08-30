//! External stock-compatibility tests for archive discovery and resolution.

use std::error::Error;
use std::fs;
use std::path::Path;

use solarity_asset::{
    ArchiveCatalog, ArchiveKind, AssetError, AssetPath, AssetStore, ClientDataRoot, Locale,
};

use crate::support::{Fixture, FixtureFile};

/// Stock uses a separate base band and opens the reversed patch list at 64.
#[test]
fn catalog_preserves_exact_stock_priority_order() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "patch.MPQ",
            path: "patch.txt",
            bytes: b"global-one",
        },
        FixtureFile {
            archive: "enUS/patch-enUS.MPQ",
            path: "patch.txt",
            bytes: b"locale-one",
        },
        FixtureFile {
            archive: "patch-2.MPQ",
            path: "patch.txt",
            bytes: b"global-two",
        },
        FixtureFile {
            archive: "enUS/patch-enUS-2.MPQ",
            path: "patch.txt",
            bytes: b"locale-two",
        },
        FixtureFile {
            archive: "patch-3.MPQ",
            path: "patch.txt",
            bytes: b"global-three",
        },
        FixtureFile {
            archive: "enUS/patch-enUS-3.MPQ",
            path: "patch.txt",
            bytes: b"locale-three",
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    let actual = catalog
        .descriptors()
        .iter()
        .map(|descriptor| {
            (
                descriptor
                    .relative_path()
                    .to_string_lossy()
                    .replace('\\', "/"),
                descriptor.priority().value(),
                descriptor.kind(),
            )
        })
        .collect::<Vec<_>>();
    let expected = [
        ("patch-3.MPQ", 69, ArchiveKind::Patch),
        ("patch-2.MPQ", 68, ArchiveKind::Patch),
        ("enUS/patch-enUS-3.MPQ", 67, ArchiveKind::Patch),
        ("enUS/patch-enUS-2.MPQ", 66, ArchiveKind::Patch),
        ("patch.MPQ", 65, ArchiveKind::Patch),
        ("enUS/patch-enUS.MPQ", 64, ArchiveKind::Patch),
        ("expansion.MPQ", 49, ArchiveKind::Global),
        ("lichking.MPQ", 48, ArchiveKind::Global),
        ("common.MPQ", 47, ArchiveKind::Global),
        ("common-2.MPQ", 46, ArchiveKind::Global),
        ("enUS/locale-enUS.MPQ", 45, ArchiveKind::Localized),
        ("enUS/speech-enUS.MPQ", 44, ArchiveKind::Localized),
        ("enUS/expansion-locale-enUS.MPQ", 43, ArchiveKind::Localized),
        ("enUS/lichking-locale-enUS.MPQ", 42, ArchiveKind::Localized),
        ("enUS/expansion-speech-enUS.MPQ", 41, ArchiveKind::Localized),
        ("enUS/lichking-speech-enUS.MPQ", 40, ArchiveKind::Localized),
    ]
    .map(|(path, priority, kind)| (path.to_owned(), priority, kind));

    assert_eq!(actual, expected);
    Ok(())
}

/// The highest stock patch priority wins duplicate resolution.
#[test]
fn duplicate_assets_resolve_from_the_highest_stock_priority() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Interface\\FrameXML\\FrameXML.toc",
            bytes: b"base",
        },
        FixtureFile {
            archive: "enUS/patch-enUS-3.MPQ",
            path: "Interface\\FrameXML\\FrameXML.toc",
            bytes: b"localized-patch",
        },
        FixtureFile {
            archive: "patch-3.MPQ",
            path: "Interface\\FrameXML\\FrameXML.toc",
            bytes: b"global-patch",
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let read = store.read(&AssetPath::new("interface/framexml/framexml.toc")?)?;

    assert_eq!(read.bytes(), b"global-patch");
    assert_eq!(read.source().relative_path(), Path::new("patch-3.MPQ"));
    Ok(())
}

/// Stock's unnumbered global patch outranks its localized peer after reverse-open.
#[test]
fn global_unnumbered_patch_outranks_localized_unnumbered_patch() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "patch.MPQ",
            path: "duplicate.txt",
            bytes: b"global",
        },
        FixtureFile {
            archive: "enUS/patch-enUS.MPQ",
            path: "duplicate.txt",
            bytes: b"localized",
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let read = store.read(&AssetPath::new("DUPLICATE.TXT")?)?;

    assert_eq!(read.bytes(), b"global");
    assert_eq!(read.source().relative_path(), Path::new("patch.MPQ"));
    Ok(())
}

/// A lettered HD pack replaces the same virtual file without a second namespace.
#[test]
fn lettered_hd_pack_replaces_the_same_larger_asset() -> Result<(), Box<dyn Error>> {
    let stock_bytes = vec![0x35; 32 * 1024];
    let hd_bytes = (0..2 * 1024 * 1024)
        .map(|offset| (offset % 251) as u8)
        .collect::<Vec<_>>();
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "patch-3.MPQ",
            path: "World\\Expansion02\\Doodads\\Example.blp",
            bytes: &stock_bytes,
        },
        FixtureFile {
            archive: "patch-H.MPQ",
            path: "World\\Expansion02\\Doodads\\Example.blp",
            bytes: &hd_bytes,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    let hd_descriptor = catalog
        .descriptors()
        .iter()
        .find(|descriptor| descriptor.relative_path() == Path::new("patch-H.MPQ"));
    assert!(hd_descriptor.is_some());

    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("world/expansion02/doodads/example.blp")?;
    let read = store.read(&path)?;

    assert_eq!(read.source().relative_path(), Path::new("patch-H.MPQ"));
    assert_eq!(read.bytes(), hd_bytes);
    Ok(())
}

/// Missing files fail explicitly instead of probing loose paths or other roots.
#[test]
fn missing_asset_has_no_non_stock_fallback() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let path = AssetPath::new("DBFilesClient/Missing.dbc")?;

    assert!(matches!(
        store.read(&path),
        Err(AssetError::AssetNotFound { path: missing }) if missing == path
    ));
    Ok(())
}

/// Presence probes use the mounted virtual namespace without reading payloads.
#[test]
fn asset_presence_uses_exact_archive_paths() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[FixtureFile {
        archive: "patch-H.MPQ",
        path: "Item\\TextureComponents\\HandTexture\\Glove_U.blp",
        bytes: b"large replacement payload",
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    assert!(store.contains(&AssetPath::new(
        "item/texturecomponents/handtexture/glove_u.blp"
    )?)?);
    assert!(!store.contains(&AssetPath::new(
        "item/texturecomponents/handtexture/glove_f.blp"
    )?)?);
    Ok(())
}

/// A missing required file identifies the expected consolidated archive.
#[test]
fn missing_required_archive_fails_discovery() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[])?;
    fs::remove_file(fixture.data_root().join("common-2.MPQ"))?;
    let root = ClientDataRoot::new(fixture.data_root())?;

    assert!(matches!(
        ArchiveCatalog::discover(root, Locale::EnUs),
        Err(AssetError::MissingRequiredArchive { path })
            if path.ends_with("common-2.MPQ")
    ));
    Ok(())
}

/// A present corrupt archive fails mounting rather than being silently skipped.
#[test]
fn corrupt_required_archive_fails_mount() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[])?;
    fs::write(fixture.data_root().join("common.MPQ"), b"not an MPQ")?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;

    assert!(matches!(
        AssetStore::mount(catalog),
        Err(AssetError::ArchiveOpen { path, .. }) if path.ends_with("common.MPQ")
    ));
    Ok(())
}
