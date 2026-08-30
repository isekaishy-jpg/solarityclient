//! External stock-compatibility tests for validated archive vocabulary.

use std::error::Error;

use solarity_asset::{AssetError, AssetPath, AssetPathViolation, Locale};

/// Build 12340 probes these locale tokens in this exact order.
#[test]
fn stock_build_12340_locale_table_is_complete() -> Result<(), Box<dyn Error>> {
    let locales = [
        "deDE", "enGB", "enUS", "esES", "frFR", "koKR", "zhCN", "zhTW", "enCN", "enTW", "esMX",
        "ruRU",
    ];

    for locale in locales {
        assert_eq!(locale.parse::<Locale>()?.as_str(), locale);
    }
    assert!(matches!(
        "itIT".parse::<Locale>(),
        Err(AssetError::UnsupportedLocale { .. })
    ));
    Ok(())
}

/// MPQ hashes are case-insensitive and use backslash separators.
#[test]
fn asset_paths_normalize_once_at_the_boundary() -> Result<(), Box<dyn Error>> {
    let path = AssetPath::new("Interface/FrameXML/FrameXML.toc")?;

    assert_eq!(path.as_str(), "INTERFACE\\FRAMEXML\\FRAMEXML.TOC");
    assert_eq!(path, AssetPath::new("interface\\framexml\\framexml.toc")?);
    Ok(())
}

/// Filesystem and traversal paths never reach the MPQ dependency.
#[test]
fn asset_paths_reject_non_archive_names() {
    let cases = [
        ("", AssetPathViolation::Empty),
        ("C:\\Wow\\Data", AssetPathViolation::NotArchiveRelative),
        ("/absolute", AssetPathViolation::NotArchiveRelative),
        ("Interface/../GlueXML", AssetPathViolation::InvalidComponent),
        ("Interface//GlueXML", AssetPathViolation::InvalidComponent),
        ("Interface/é", AssetPathViolation::NonAscii),
    ];

    for (path, expected) in cases {
        assert!(matches!(
            AssetPath::new(path),
            Err(AssetError::InvalidAssetPath {
                violation,
                ..
            }) if violation == expected
        ));
    }
}
