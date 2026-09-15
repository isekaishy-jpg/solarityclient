//! Real archive operations yield before domain work and retain one owner across turns.

use std::{
    error::Error,
    ops::ControlFlow,
    sync::atomic::{AtomicUsize, Ordering},
};

use solarity_asset::{ArchiveCatalog, AssetError, AssetPath, ClientDataRoot, Locale};

use super::prepare_archive;
use crate::test_support::ClientFixture;

/// Required callers cannot run against a partial stack; later domain turns reuse it.
#[test]
fn archive_preparation_yields_between_opens_and_before_domain_steps() -> Result<(), Box<dyn Error>>
{
    let fixture = ClientFixture::with_common_files(&[("test.txt", b"payload")])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let count = catalog.descriptors().len();
    let namespace = catalog.namespace();
    let calls = AtomicUsize::new(0);
    let path = AssetPath::new("test.txt")?;
    let mut identity = None;
    let mut operation = prepare_archive(catalog, |store| {
        assert_eq!(store.namespace(), namespace);
        assert_eq!(*identity.get_or_insert(store.identity()), store.identity());
        let call = calls.fetch_add(1, Ordering::Relaxed);
        if call < 2 {
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break(store.read(&path).map(|read| read.into_bytes()))
        }
    });
    for _ in 0..=count {
        assert!(matches!(operation(), ControlFlow::Continue(())));
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }
    assert!(matches!(operation(), ControlFlow::Continue(())));
    assert!(matches!(operation(), ControlFlow::Continue(())));
    assert_eq!(
        operation()
            .break_value()
            .ok_or("missing terminal result")??,
        b"payload"
    );
    assert_eq!(calls.load(Ordering::Relaxed), 3);
    Ok(())
}

/// Opening failure never invokes the consumer or hides the selected archive error.
#[test]
fn failed_archive_preparation_skips_the_domain_operation() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let first = catalog.descriptors()[0].path().to_path_buf();
    std::fs::write(&first, b"invalid MPQ")?;
    let mut operation = prepare_archive(catalog, |_| -> ControlFlow<Result<(), AssetError>> {
        panic!("a failed mount must not enter domain preparation")
    });
    assert!(matches!(operation(), ControlFlow::Continue(())));
    assert!(
        matches!(operation(), ControlFlow::Break(Err(AssetError::ArchiveOpen { path, .. })) if path == first)
    );
    Ok(())
}
