//! External stock-compatibility tests for archive discovery and resolution.

use std::error::Error;
use std::fs;
use std::path::Path;

use solarity_asset::{
    ArchiveCatalog, ArchiveKind, AssetError, AssetPath, AssetStore, ClientDataRoot, Locale,
    LocalizedDocument,
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
    assert_eq!(catalog.existing_locales(), &[Locale::EnUs]);
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

/// MovieFrame's extensionless identity resolves only in the selected locale's
/// stock loose cinematic tree.
#[test]
fn cinematic_identity_resolves_locale_loose_avi() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[])?;
    fixture.write_loose_file(
        "Data/enUS/Interface/Cinematics/Logo_1024.avi",
        b"RIFF fixture",
    )?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let path = store.cinematic_file_path(&AssetPath::new("interface/cinematics/logo_1024")?)?;

    assert_eq!(
        path,
        fs::canonicalize(
            fixture
                .data_root()
                .join("enUS/Interface/Cinematics/Logo_1024.avi"),
        )?
    );
    assert_eq!(fs::read(path)?, b"RIFF fixture");
    Ok(())
}

/// Loose cinematic lookup is scoped and never becomes a general filesystem
/// fallback for missing archive assets.
#[test]
fn cinematic_lookup_rejects_other_loose_namespaces() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[])?;
    fixture.write_loose_file("Data/enUS/DBFilesClient/Forbidden.avi", b"loose")?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    assert!(matches!(
        store.cinematic_file_path(&AssetPath::new("DBFilesClient/Forbidden")?),
        Err(AssetError::InvalidAssetPath { .. })
    ));
    Ok(())
}

/// Built-in Glue documents resolve only from the selected locale root with
/// Win32 filename case folding.
#[test]
fn localized_glue_document_resolves_selected_locale_file() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[])?;
    fixture.write_loose_file("Data/enUS/EULA.HTML", b"<html>selected</html>")?;
    fixture.write_loose_file("Data/frFR/eula.html", b"<html>other locale</html>")?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    assert_eq!(
        store.read_localized_document(LocalizedDocument::Eula)?,
        Some(b"<html>selected</html>".to_vec())
    );
    assert_eq!(
        store.read_localized_document(LocalizedDocument::Termination)?,
        None
    );
    Ok(())
}

/// The localized-document API admits only names authored by stock GlueXML.
#[test]
fn localized_glue_document_name_has_no_general_loose_fallback() {
    assert!(matches!(
        "../Config.wtf".parse::<LocalizedDocument>(),
        Err(AssetError::UnsupportedLocalizedDocument { name }) if name == "../Config.wtf"
    ));
    assert!("EULA.HTML".parse::<LocalizedDocument>().is_ok());
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
