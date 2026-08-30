//! Generated MPQ fixtures shared by external asset tests.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use wow_mpq::{ArchiveBuilder, ListfileOption};

/// Exact archive names required by the consolidated enUS test layout.
pub(crate) const REQUIRED_ARCHIVES: [&str; 10] = [
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

/// An archive file injected into a generated fixture.
pub(crate) struct FixtureFile<'a> {
    /// Archive path relative to the fixture `Data` directory.
    pub(crate) archive: &'a str,
    /// MPQ-internal path.
    pub(crate) path: &'a str,
    /// File contents.
    pub(crate) bytes: &'a [u8],
}

/// An isolated client data tree removed when its test completes.
pub(crate) struct Fixture {
    root: PathBuf,
}

impl Fixture {
    /// Builds the complete consolidated layout and any requested patch archives.
    pub(crate) fn new(files: &[FixtureFile<'_>]) -> Result<Self, Box<dyn Error>> {
        static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("solarity-asset-{}-{sequence}", std::process::id()));
        fs::create_dir_all(root.join("Data/enUS"))?;

        for archive in REQUIRED_ARCHIVES {
            build_archive(
                &root.join("Data").join(archive),
                archive,
                files.iter().filter(|file| file.archive == archive),
            )?;
        }

        for archive in unique_extra_archives(files) {
            build_archive(
                &root.join("Data").join(archive),
                archive,
                files.iter().filter(|file| file.archive == archive),
            )?;
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

/// Builds one MPQ with a marker plus all files assigned to it.
fn build_archive<'a>(
    path: &Path,
    archive_name: &str,
    files: impl Iterator<Item = &'a FixtureFile<'a>>,
) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let marker = format!("fixture:{archive_name}").into_bytes();
    let mut builder = ArchiveBuilder::new()
        .listfile_option(ListfileOption::Generate)
        .add_file_data(marker, "Solarity\\FixtureMarker.txt");
    for file in files {
        builder = builder.add_file_data(file.bytes.to_vec(), file.path);
    }
    builder.build(path)?;
    Ok(())
}

/// Returns extra archive names once, preserving their fixture declaration order.
fn unique_extra_archives<'a>(files: &'a [FixtureFile<'a>]) -> Vec<&'a str> {
    let mut archives = Vec::new();
    for file in files {
        if REQUIRED_ARCHIVES.contains(&file.archive) || archives.contains(&file.archive) {
            continue;
        }
        archives.push(file.archive);
    }
    archives
}
