//! Generated MPQ fixture shared by external UI tests.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use wow_mpq::{ArchiveBuilder, ListfileOption};

const REQUIRED_ARCHIVES: [&str; 10] = [
    "expansion.MPQ",
    "lichking.MPQ",
    "common.MPQ",
    "common-2.MPQ",
    "enUS/locale-enUS.MPQ",
    "enUS/speech-enUS.MPQ",
    "enUS/expansion-locale-enUS.MPQ",
    "enUS/lichking-locale-enUS.MPQ",
    "enUS/expansion-speech-enUS.MPQ",
    "enUS/lichking-speech-enUS.MPQ",
];

/// One file injected into the localized stock archive.
pub(crate) struct FixtureFile<'a> {
    /// MPQ-internal path.
    pub(crate) path: &'a str,
    /// Source bytes.
    pub(crate) bytes: &'a [u8],
}

/// An isolated consolidated client-data tree.
pub(crate) struct Fixture {
    root: PathBuf,
}

impl Fixture {
    /// Builds all mandatory archives and injects UI files into locale-enUS.MPQ.
    pub(crate) fn new(files: &[FixtureFile<'_>]) -> Result<Self, Box<dyn Error>> {
        static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("solarity-ui-{}-{sequence}", std::process::id()));
        fs::create_dir_all(root.join("Data/enUS"))?;

        for archive in REQUIRED_ARCHIVES {
            build_archive(&root.join("Data").join(archive), archive, files)?;
        }
        Ok(Self { root })
    }

    /// Returns the generated Data directory.
    pub(crate) fn data_root(&self) -> PathBuf {
        self.root.join("Data")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup_result = fs::remove_dir_all(&self.root);
    }
}

fn build_archive(
    path: &Path,
    archive_name: &str,
    files: &[FixtureFile<'_>],
) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let marker = format!("fixture:{archive_name}").into_bytes();
    let mut builder = ArchiveBuilder::new()
        .listfile_option(ListfileOption::Generate)
        .add_file_data(marker, "Solarity\\FixtureMarker.txt");
    if archive_name == "enUS/locale-enUS.MPQ" {
        for file in files {
            builder = builder.add_file_data(file.bytes.to_vec(), file.path);
        }
    }
    builder.build(path)?;
    Ok(())
}
