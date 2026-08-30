//! External stock-compatibility tests for persistent GlueXML ownership.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{GlueManager, UiObjectKind};

use crate::support::{Fixture, FixtureFile};

/// The manager retains executable state after temporary XML plans are gone.
#[test]
fn glue_manager_owns_executed_login_ui() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Bootstrap.xml\nAfter.lua\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Bootstrap.xml",
            bytes: br#"<Ui><Frame name="GlueBootstrap"><Frames>
  <Model name="$parentModel"/>
</Frames><Scripts><OnLoad>
  GlueBootstrapModel:SetModel("Solarity\\FixtureMarker.txt")
  self.loaded = true
</OnLoad></Scripts></Frame></Ui>"#,
        },
        FixtureFile {
            path: "Interface\\GlueXML\\After.lua",
            bytes: br#"assert(GlueBootstrap.loaded)
assert(GlueBootstrapModel:GetModel() == "SOLARITY\\FIXTUREMARKER.TXT")
GLUE_READY = true"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let report = manager.report();

    assert_eq!(report.resource_count(), 2);
    assert_eq!(report.action_count(), 2);
    assert_eq!(report.object_count(), 2);
    assert_eq!(report.named_object_count(), 2);
    assert_eq!(report.frame_count(), 2);
    assert_eq!(report.region_count(), 2);
    assert_eq!(report.executed_chunk_count(), 1);
    assert_eq!(report.executed_load_handler_count(), 1);
    assert_eq!(manager.objects()[0].kind(), UiObjectKind::Frame);
    assert_eq!(manager.children(0), Some(&[1][..]));
    assert_eq!(manager.objects()[1].parent(), Some(0));
    assert!(manager.bundle().lua().globals().get::<bool>("GLUE_READY")?);
    Ok(())
}
