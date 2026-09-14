//! Event transactions retain Lua ordering and live geometry before one publication.

#[path = "presentation_transaction/stock.rs"]
mod stock;

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{GlueManager, UiEventArgument, UiEventPayload};

use crate::support::{Fixture, FixtureFile};

/// Creates topology and then queries/modifies it from later event handlers.
fn fixture() -> Result<Fixture, Box<dyn Error>> {
    Fixture::new(&[
        FixtureFile {
            path: "Interface/GlueXML/GlueXML.toc",
            bytes: b"Transaction.xml\n",
        },
        FixtureFile {
            path: "Interface/GlueXML/Transaction.xml",
            bytes: br#"<Ui>
<Frame name="Owner"><Size x="200" y="100"/><Anchors><Anchor point="CENTER"/></Anchors><Scripts>
<OnLoad>self:RegisterEvent("SET_GLUE_SCREEN")</OnLoad>
<OnEvent>
 if arg1 == "first" then
  Child = CreateFrame("Frame", "CreatedChild", self)
  Child:SetWidth(80); Child:SetHeight(40); Child:SetPoint("CENTER", self, "CENTER", 0, 0)
  Scroller = CreateFrame("ScrollFrame", "DeferredScroll", self)
  Scroller:SetWidth(50); Scroller:SetHeight(20); Scroller:SetPoint("CENTER")
  Content = CreateFrame("Frame", "DeferredContent", Scroller)
  Content:SetWidth(50); Content:SetHeight(0)
  Scroller:SetScript("OnScrollRangeChanged", function(self, x, y) ScrollRangeSeen = y end)
  Scroller:SetScrollChild(Content)
  Content:SetHeight(100)
 elseif arg1 == "second" then
  assert(Child:GetWidth() == 80, "created child width")
  assert(ScrollRangeSeen == Scroller:GetVerticalScrollRange() and ScrollRangeSeen > 0,
    "range callback=" .. tostring(ScrollRangeSeen) .. " queried=" .. tostring(Scroller:GetVerticalScrollRange()))
  assert(Child:GetRight() - Child:GetLeft() == 80)
  Child:SetWidth(120); Child:SetScale(0.75)
 elseif arg1 == "third" then
  assert(Child:GetWidth() == 120)
  self:SetWidth(Child:GetWidth() * 2)
  Child:SetParent(nil)
 end
</OnEvent></Scripts></Frame></Ui>"#,
        },
    ])
}

/// The same ordered callbacks produce identical geometry with one broad snapshot.
#[test]
fn deferred_events_publish_once_and_preserve_intermediate_lua_geometry()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let mount = || -> Result<GlueManager, Box<dyn Error>> {
        let catalog =
            ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
        Ok(GlueManager::start(
            AssetStore::mount(catalog)?,
            (800, 600),
            false,
        )?)
    };
    let mut sequential = mount()?;
    let mut batched = mount()?;
    let first = batched.runtime_snapshot_count();
    for event in ["first", "second", "third"] {
        sequential.dispatch_event("SET_GLUE_SCREEN", &payload(event))?;
    }
    batched.with_deferred_presentation(|manager| -> Result<(), Box<dyn Error>> {
        manager.dispatch_event("SET_GLUE_SCREEN", &payload("first"))?;
        manager.with_deferred_presentation(|manager| {
            manager.dispatch_event("SET_GLUE_SCREEN", &payload("second"))
        })??;
        assert_eq!(manager.runtime_snapshot_count(), first);
        manager.dispatch_event("SET_GLUE_SCREEN", &payload("third"))?;
        Ok(())
    })??;
    assert_eq!(batched.runtime_snapshot_count(), first + 1);
    assert!(sequential.runtime_snapshot_count() > batched.runtime_snapshot_count());
    assert_eq!(sequential.objects().len(), batched.objects().len());
    for index in 0..sequential.objects().len() {
        assert_eq!(
            sequential.geometry().region(index),
            batched.geometry().region(index)
        );
    }
    Ok(())
}

/// An unwinding publisher restores nesting and does not strand pending topology.
#[test]
fn deferred_event_scope_recovers_after_unwind() -> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (800, 600), false)?;
    let first = manager.runtime_snapshot_count();
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _result = manager.with_deferred_presentation(|manager| {
            assert!(
                manager
                    .dispatch_event("SET_GLUE_SCREEN", &payload("first"))
                    .is_ok()
            );
            panic!("synthetic event publisher failure")
        });
    }));
    assert!(failed.is_err());
    manager.dispatch_event("SET_GLUE_SCREEN", &payload("second"))?;
    assert!(manager.runtime_snapshot_count() > first);
    assert!(
        manager
            .objects()
            .iter()
            .any(|object| object.name() == Some("CreatedChild"))
    );
    Ok(())
}

/// Supplies the event's authored string argument.
fn payload(value: &str) -> UiEventPayload {
    UiEventPayload::new([UiEventArgument::String(value.into())])
}
