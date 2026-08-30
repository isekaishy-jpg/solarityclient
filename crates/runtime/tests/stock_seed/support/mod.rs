//! Generated client archive layout for runtime integration tests.

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

/// A complete temporary consolidated client data profile.
pub(crate) struct ClientFixture {
    root: PathBuf,
}

impl ClientFixture {
    /// Generates all archives required by runtime startup.
    pub(crate) fn new() -> Result<Self, Box<dyn Error>> {
        static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "solarity-runtime-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("Data/enUS"))?;
        for archive in REQUIRED_ARCHIVES {
            build_archive(&root.join("Data").join(archive), archive)?;
        }
        Ok(Self { root })
    }

    /// Returns the generated data directory.
    pub(crate) fn data_root(&self) -> PathBuf {
        self.root.join("Data")
    }
}

impl Drop for ClientFixture {
    fn drop(&mut self) {
        let _cleanup_result = fs::remove_dir_all(&self.root);
    }
}

/// Creates one valid MPQ with a diagnostic marker.
fn build_archive(path: &Path, archive: &str) -> Result<(), Box<dyn Error>> {
    ArchiveBuilder::new()
        .listfile_option(ListfileOption::Generate)
        .add_file_data(
            format!("fixture:{archive}").into_bytes(),
            "Solarity\\RuntimeFixture.txt",
        )
        .build(path)?;
    Ok(())
}
