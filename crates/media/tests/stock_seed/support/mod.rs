//! Generated stock-layout MPQ fixtures shared by external media tests.

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

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

/// One file assigned to a generated archive.
pub(crate) struct FixtureFile<'a> {
    pub(crate) archive: &'a str,
    pub(crate) path: &'a str,
    pub(crate) bytes: &'a [u8],
}

/// Isolated consolidated client data removed after its test.
pub(crate) struct Fixture {
    root: PathBuf,
}

impl Fixture {
    /// Creates the required stock archive set and requested patch archives.
    pub(crate) fn new(files: &[FixtureFile<'_>]) -> Result<Self, Box<dyn Error>> {
        static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("solarity-media-{}-{sequence}", std::process::id()));
        fs::create_dir_all(root.join("Data/enUS"))?;

        for archive in REQUIRED_ARCHIVES {
            build_archive(
                &root.join("Data").join(archive),
                archive,
                files.iter().filter(|file| file.archive == archive),
            )?;
        }
        let mut extra_archives = Vec::new();
        for file in files {
            if REQUIRED_ARCHIVES.contains(&file.archive) || extra_archives.contains(&file.archive) {
                continue;
            }
            extra_archives.push(file.archive);
        }
        for archive in extra_archives {
            build_archive(
                &root.join("Data").join(archive),
                archive,
                files.iter().filter(|file| file.archive == archive),
            )?;
        }
        Ok(Self { root })
    }

    /// Returns the generated client `Data` directory.
    pub(crate) fn data_root(&self) -> PathBuf {
        self.root.join("Data")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup_result = fs::remove_dir_all(&self.root);
    }
}

/// Writes one MPQ with a listfile and every assigned fixture payload.
fn build_archive<'a>(
    path: &std::path::Path,
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

/// Creates one exact build-12340 `SoundEntries.dbc` row.
pub(crate) fn sound_entries_fixture(id: u32, files: [(&str, u32); 3], directory: &str) -> Vec<u8> {
    let mut strings = vec![0];
    let name = append_string(&mut strings, "WeightedSound");
    let file_offsets = files.map(|(file, _weight)| append_string(&mut strings, file));
    let directory = append_string(&mut strings, directory);
    let mut fields = vec![
        id,
        4,
        name,
        file_offsets[0],
        file_offsets[1],
        file_offsets[2],
    ];
    fields.extend([0; 7]);
    fields.extend(files.map(|(_file, weight)| weight));
    fields.extend([0; 7]);
    fields.extend([
        directory,
        1.0_f32.to_bits(),
        0,
        4.0_f32.to_bits(),
        40.0_f32.to_bits(),
        0,
        0,
    ]);
    create_wdbc(1, 30, &fields, &strings)
}

/// Serializes one fixed-field WDBC table.
fn create_wdbc(record_count: u32, field_count: u32, fields: &[u32], strings: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + strings.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&record_count.to_le_bytes());
    bytes.extend_from_slice(&field_count.to_le_bytes());
    bytes.extend_from_slice(&(field_count * 4).to_le_bytes());
    bytes.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(strings);
    bytes
}

/// Adds one NUL-terminated fixture string and returns its table offset.
fn append_string(strings: &mut Vec<u8>, value: &str) -> u32 {
    let offset = strings.len() as u32;
    strings.extend_from_slice(value.as_bytes());
    strings.push(0);
    offset
}

/// Serializes SDL_mixer initialization inside this integration-test process.
pub(crate) fn sdl_test_lock() -> MutexGuard<'static, ()> {
    static SDL_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    match SDL_TEST_LOCK.get_or_init(|| Mutex::new(())).lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Builds a mono signed-16 PCM RIFF/WAVE fixture.
pub(crate) fn pcm_wav(sample_rate_hz: u32, samples: &[i16]) -> Result<Vec<u8>, Box<dyn Error>> {
    const FORMAT_CHUNK_SIZE: u32 = 16;
    const PCM_FORMAT: u16 = 1;
    const CHANNEL_COUNT: u16 = 1;
    const BITS_PER_SAMPLE: u16 = 16;

    let data_byte_count = u32::try_from(samples.len())?
        .checked_mul(size_of::<i16>() as u32)
        .ok_or("WAV fixture byte count overflows")?;
    let mut bytes = Vec::with_capacity(44 + data_byte_count as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_byte_count).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&FORMAT_CHUNK_SIZE.to_le_bytes());
    bytes.extend_from_slice(&PCM_FORMAT.to_le_bytes());
    bytes.extend_from_slice(&CHANNEL_COUNT.to_le_bytes());
    bytes.extend_from_slice(&sample_rate_hz.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate_hz * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_byte_count.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(bytes)
}
