//! Explicit locale-loose resolution for built-in Glue HTML documents.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::str::FromStr;

use crate::archive::AssetError;
use crate::file_stack::AssetStore;

/// A locale-loose document referenced by build-12340 GlueXML.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalizedDocument {
    /// Terms of Use.
    Tos,
    /// End User License Agreement.
    Eula,
    /// Termination Without Notice agreement.
    Termination,
    /// System-scanning agreement.
    Scan,
    /// Contest agreement.
    Contest,
    /// Hardware survey copy.
    Survey,
    /// Login connection-help copy.
    ConnectionHelp,
    /// Base-game credits.
    Credits,
    /// The Burning Crusade credits.
    CreditsBurningCrusade,
    /// Wrath of the Lich King credits.
    CreditsWrath,
}

impl LocalizedDocument {
    /// Returns the exact locale-root filename authored by stock GlueXML.
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Tos => "tos.html",
            Self::Eula => "eula.html",
            Self::Termination => "termination.html",
            Self::Scan => "scan.html",
            Self::Contest => "contest.html",
            Self::Survey => "survey.html",
            Self::ConnectionHelp => "connection-help.html",
            Self::Credits => "credits.html",
            Self::CreditsBurningCrusade => "credits_BC.html",
            Self::CreditsWrath => "credits_LK.html",
        }
    }
}

impl FromStr for LocalizedDocument {
    type Err = AssetError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.eq_ignore_ascii_case("tos.html") {
            Ok(Self::Tos)
        } else if value.eq_ignore_ascii_case("eula.html") {
            Ok(Self::Eula)
        } else if value.eq_ignore_ascii_case("termination.html") {
            Ok(Self::Termination)
        } else if value.eq_ignore_ascii_case("scan.html") {
            Ok(Self::Scan)
        } else if value.eq_ignore_ascii_case("contest.html") {
            Ok(Self::Contest)
        } else if value.eq_ignore_ascii_case("survey.html") {
            Ok(Self::Survey)
        } else if value.eq_ignore_ascii_case("connection-help.html") {
            Ok(Self::ConnectionHelp)
        } else if value.eq_ignore_ascii_case("credits.html") {
            Ok(Self::Credits)
        } else if value.eq_ignore_ascii_case("credits_BC.html") {
            Ok(Self::CreditsBurningCrusade)
        } else if value.eq_ignore_ascii_case("credits_LK.html") {
            Ok(Self::CreditsWrath)
        } else {
            Err(AssetError::UnsupportedLocalizedDocument {
                name: value.to_owned(),
            })
        }
    }
}

impl AssetStore {
    /// Reads one present built-in document below the selected locale root.
    ///
    /// Absence returns `None` because several documents are not shipped for
    /// regions whose corresponding Glue agreement flags are disabled. The
    /// method never probes another locale or the general loose filesystem.
    ///
    /// # Errors
    ///
    /// Returns a scoped filesystem error when the selected locale tree cannot
    /// be inspected or a discovered document cannot be read.
    pub fn read_localized_document(
        &self,
        document: LocalizedDocument,
    ) -> Result<Option<Vec<u8>>, AssetError> {
        let locale_root = self.data_root.as_path().join(self.locale().as_str());
        let Some(path) = find_file(&locale_root, document.file_name())? else {
            return Ok(None);
        };
        fs::read(&path)
            .map(Some)
            .map_err(|source| AssetError::LocalizedDocumentRead {
                path,
                message: source.to_string(),
            })
    }
}

/// Reproduces Win32 case folding for one direct locale-root file.
fn find_file(root: &Path, expected: &str) -> Result<Option<std::path::PathBuf>, AssetError> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(source) if source.kind() == ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(AssetError::LocalizedDocumentLookup {
                path: root.to_path_buf(),
                message: source.to_string(),
            });
        }
    };
    for entry in entries {
        let entry = entry.map_err(|source| AssetError::LocalizedDocumentLookup {
            path: root.to_path_buf(),
            message: source.to_string(),
        })?;
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(expected)
            && entry
                .file_type()
                .map_err(|source| AssetError::LocalizedDocumentLookup {
                    path: entry.path(),
                    message: source.to_string(),
                })?
                .is_file()
        {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}
