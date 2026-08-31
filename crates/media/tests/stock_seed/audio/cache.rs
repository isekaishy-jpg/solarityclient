//! External stock-compatibility tests for encoded sound residency.

use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale};
use solarity_media::SoundCache;

use crate::support::{Fixture, FixtureFile};

/// A replacement pack wins once and shares one encoded path owner.
#[test]
fn sound_cache_shares_selected_patch_payload() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Sound\\Weighted\\First.wav",
            bytes: b"base-wave",
        },
        FixtureFile {
            archive: "patch-2.MPQ",
            path: "Sound\\Weighted\\First.wav",
            bytes: b"replacement-wave-with-larger-payload",
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let path = AssetPath::new("Sound/Weighted/First.wav")?;
    let mut cache = SoundCache::new();

    let first = cache.load(&mut store, &path)?;
    let second = cache.load(&mut store, &path)?;
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(cache.len(), 1);
    assert_eq!(first.path(), &path);
    assert_eq!(first.source().relative_path(), Path::new("patch-2.MPQ"));
    assert_eq!(first.bytes(), b"replacement-wave-with-larger-payload");

    drop(second);
    assert_eq!(cache.collect_unused(), 0);
    drop(first);
    assert_eq!(cache.collect_unused(), 1);
    assert!(cache.is_empty());
    Ok(())
}
