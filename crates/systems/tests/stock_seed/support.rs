//! Generated MPQ fixtures shared by external systems tests.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use wow_mpq::{ArchiveBuilder, ListfileOption};

/// Exact archive names required by the consolidated enUS test layout.
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

/// One file written to a generated client archive.
pub(crate) struct FixtureFile<'bytes> {
    /// MPQ-internal path.
    pub(crate) path: &'bytes str,
    /// File contents.
    pub(crate) bytes: &'bytes [u8],
}

/// An isolated complete client-data tree removed after its test.
pub(crate) struct Fixture {
    root: PathBuf,
}

impl Fixture {
    /// Builds the required archive set with supplied files in `common.MPQ`.
    pub(crate) fn new(files: &[FixtureFile<'_>]) -> Result<Self, Box<dyn Error>> {
        static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "solarity-systems-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("Data/enUS"))?;
        for archive in REQUIRED_ARCHIVES {
            let entries = if archive == "common.MPQ" {
                Some(files)
            } else {
                None
            };
            build_archive(&root.join("Data").join(archive), archive, entries)?;
        }
        Ok(Self { root })
    }

    /// Returns the generated `Data` directory.
    pub(crate) fn data_root(&self) -> PathBuf {
        self.root.join("Data")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup_result = fs::remove_dir_all(&self.root);
    }
}

/// Builds one deterministic MPQ and its generated listfile.
fn build_archive(
    path: &Path,
    archive_name: &str,
    files: Option<&[FixtureFile<'_>]>,
) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let marker = format!("fixture:{archive_name}").into_bytes();
    let mut builder = ArchiveBuilder::new()
        .listfile_option(ListfileOption::Generate)
        .add_file_data(marker, "Solarity\\FixtureMarker.txt");
    if let Some(files) = files {
        for file in files {
            builder = builder.add_file_data(file.bytes.to_vec(), file.path);
        }
    }
    builder.build(path)?;
    Ok(())
}
