//! Source stepping preserves aliases, payloads and the existing diagnostic order.

use std::{error::Error, ops::ControlFlow};

use solarity_asset::{ArchiveCatalog, AssetPath, ClientDataRoot, Locale};

use super::prepare_configured_glue_textures;
use crate::test_support::{ClientFixture, bootstrap_texture_blp};

/// Mounts and each requested path have separate turns; no entry is skipped or retried.
#[test]
fn configured_texture_steps_keep_alias_reuse_and_ordered_failures() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::with_common_files(&[("fixture.blp", &bootstrap_texture_blp())])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mounts = catalog.descriptors().len();
    let namespace = catalog.namespace();
    let paths = [
        "fixture.blp",
        "missing-one.blp",
        "FIXTURE.BLP",
        "missing-two.blp",
    ]
    .map(AssetPath::new)
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    let mut operation = prepare_configured_glue_textures(
        catalog,
        paths,
        solarity_asset::AssetReadBudget::for_service(
            solarity_cpu::CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(0, 0, 1 << 20)),
            solarity_cpu::CpuService::Speculative,
        ),
    );
    for _ in 0..(1 + mounts + 4) {
        assert!(matches!(operation(), ControlFlow::Continue(())));
    }
    let result = operation()
        .break_value()
        .ok_or("missing prewarm result")??;
    assert_eq!(result.cache.len(), 1);
    assert_eq!(result.cache.entries(namespace).count(), 1);
    assert_eq!(result.failures.len(), 2);
    assert!(result.failures[0].contains("MISSING-ONE.BLP"));
    assert!(result.failures[1].contains("MISSING-TWO.BLP"));
    Ok(())
}
