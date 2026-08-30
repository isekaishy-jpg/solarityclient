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
    let mut builder = ArchiveBuilder::new()
        .listfile_option(ListfileOption::Generate)
        .add_file_data(
            format!("fixture:{archive}").into_bytes(),
            "Solarity\\RuntimeFixture.txt",
        );
    if archive == "enUS/locale-enUS.MPQ" {
        builder = builder.add_file_data(
            bootstrap_texture_blp(),
            "Interface\\Icons\\INV_Misc_QuestionMark.blp",
        );
    }
    builder.build(path)?;
    Ok(())
}

/// Builds a two-pixel BLP2/RAW3 texture for the asset-backed startup frame.
fn bootstrap_texture_blp() -> Vec<u8> {
    const HEADER_SIZE: u32 = 148;
    const PALETTE_SIZE: u32 = 256 * 4;
    const PIXEL_OFFSET: u32 = HEADER_SIZE + PALETTE_SIZE;
    const PIXEL_BYTES: u32 = 8;

    let mut bytes = Vec::with_capacity((PIXEL_OFFSET + PIXEL_BYTES) as usize);
    bytes.extend_from_slice(b"BLP2");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&[3, 8, 8, 0]);
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&PIXEL_OFFSET.to_le_bytes());
    for _unused in 1..16 {
        bytes.extend_from_slice(&0_u32.to_le_bytes());
    }
    bytes.extend_from_slice(&PIXEL_BYTES.to_le_bytes());
    for _unused in 1..16 {
        bytes.extend_from_slice(&0_u32.to_le_bytes());
    }
    bytes.resize(PIXEL_OFFSET as usize, 0);
    bytes.extend_from_slice(&0xFFFF_0000_u32.to_le_bytes());
    bytes.extend_from_slice(&0xFF00_FF00_u32.to_le_bytes());
    bytes
}
