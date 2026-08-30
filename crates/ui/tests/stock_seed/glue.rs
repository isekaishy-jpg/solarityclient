//! External stock-compatibility tests for persistent GlueXML ownership.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{GlueError, GlueManager, UiLayoutError, UiObjectKind};

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

/// Screen rectangles use post-OnLoad dimensions, anchors, and dynamic frames.
#[test]
fn glue_manager_resolves_live_startup_geometry() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Geometry.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Geometry.xml",
            bytes: br#"<Ui>
<Frame name="DynamicTemplate" virtual="true" alpha="0.5" scale="0.5">
  <Size x="60" y="30"/>
  <Anchors><Anchor point="CENTER"/></Anchors>
</Frame>
<Frame name="Root"><Frames>
  <Frame name="$parentStretch">
    <Size x="7" y="9"/>
    <Anchors>
      <Anchor point="TOPLEFT" relativeTo="$parent" relativePoint="TOPLEFT">
        <Offset x="5" y="-6"/>
      </Anchor>
      <Anchor point="BOTTOMRIGHT" relativeTo="$parent" relativePoint="BOTTOMRIGHT">
        <Offset x="-7" y="8"/>
      </Anchor>
    </Anchors>
  </Frame>
</Frames><Scripts><OnLoad>
  self:SetSize(400, 200)
  self:SetPoint("CENTER", nil, "CENTER")
  self:SetAlpha(0.8)
  self:SetScale(0.5)
  local dynamic = CreateFrame("Frame", "Dynamic", self)
  dynamic:SetSize(120, 40)
  dynamic:SetPoint("TOPLEFT", self, "TOPLEFT", 10, -20)
  CreateFrame("Frame", "Templated", self, "DynamicTemplate")
</OnLoad></Scripts></Frame></Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;

    assert_eq!(manager.report().object_count(), 4);
    assert_eq!(manager.report().region_count(), 4);
    assert_eq!(manager.objects()[2].name(), Some("Dynamic"));
    assert_eq!(manager.objects()[3].name(), Some("Templated"));
    assert_eq!(manager.children(0), Some(&[1, 2, 3][..]));

    let root = manager
        .geometry()
        .region(0)
        .ok_or("missing root geometry")?;
    let stretch = manager
        .geometry()
        .region(1)
        .ok_or("missing stretch geometry")?;
    let dynamic = manager
        .geometry()
        .region(2)
        .ok_or("missing dynamic geometry")?;
    let templated = manager
        .geometry()
        .region(3)
        .ok_or("missing templated geometry")?;
    assert_close(root.logical_bounds().width(), 400.0);
    assert_close(root.logical_bounds().height(), 200.0);
    assert_close(root.logical_bounds().left(), 482.666_666_666_7);
    assert_close(root.logical_bounds().bottom(), 284.0);
    assert_close(root.presentation_bounds().width(), 200.0);
    assert_close(root.effective_alpha(), 0.8);
    assert_close(root.effective_scale(), 0.5);
    assert_close(
        stretch.logical_bounds().left(),
        root.logical_bounds().left() + 5.0,
    );
    assert_close(
        stretch.logical_bounds().right(),
        root.logical_bounds().right() - 7.0,
    );
    assert_close(
        stretch.logical_bounds().top(),
        root.logical_bounds().top() - 6.0,
    );
    assert_close(
        stretch.logical_bounds().bottom(),
        root.logical_bounds().bottom() + 8.0,
    );
    assert_close(
        dynamic.logical_bounds().left(),
        root.logical_bounds().left() + 10.0,
    );
    assert_close(
        dynamic.logical_bounds().top(),
        root.logical_bounds().top() - 20.0,
    );
    assert_close(dynamic.logical_bounds().width(), 120.0);
    assert_close(dynamic.logical_bounds().height(), 40.0);
    assert_close(dynamic.presentation_bounds().width(), 60.0);
    assert_close(templated.logical_bounds().width(), 60.0);
    assert_close(templated.logical_bounds().height(), 30.0);
    assert_close(
        (templated.logical_bounds().left() + templated.logical_bounds().right()) * 0.5,
        (root.logical_bounds().left() + root.logical_bounds().right()) * 0.5,
    );
    assert_close(templated.presentation_bounds().width(), 15.0);
    assert_close(templated.effective_alpha(), 0.4);
    assert_close(templated.effective_scale(), 0.25);
    Ok(())
}

/// Mutually dependent live anchors fail instead of receiving guessed bounds.
#[test]
fn glue_manager_rejects_live_anchor_cycles() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Cycle.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Cycle.xml",
            bytes: br#"<Ui><Frame name="Root"><Frames>
  <Frame name="First"><Size x="10" y="10"/></Frame>
  <Frame name="Second"><Size x="10" y="10"/></Frame>
</Frames><Scripts><OnLoad>
  First:SetPoint("CENTER", Second, "CENTER")
  Second:SetPoint("CENTER", First, "CENTER")
</OnLoad></Scripts></Frame></Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let result = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false);

    assert!(matches!(
        result,
        Err(GlueError::Layout(UiLayoutError::Resolution { .. }))
    ));
    Ok(())
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 0.000_001,
        "{actual} != {expected}"
    );
}
