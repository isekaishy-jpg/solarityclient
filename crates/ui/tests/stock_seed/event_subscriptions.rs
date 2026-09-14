//! Subscription mutation keeps the original bounded, creation-ordered traversal.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{GlueManager, UiEventPayload};

use crate::support::{Fixture, FixtureFile};

/// Exercises both explicit and all-event membership among unrelated regions.
#[test]
fn indexed_events_observe_later_mutations_without_revisiting_earlier_or_new_owners()
-> Result<(), Box<dyn Error>> {
    let mut xml = String::from(
        r#"<Ui>
<Frame name="Earlier"><Scripts><OnEvent>ORDER=ORDER..'Earlier;'</OnEvent></Scripts></Frame>
<Frame name="First"><Scripts>
<OnLoad>ORDER=''; self:RegisterEvent('character_list_update'); self:RegisterEvent('CHARACTER_LIST_UPDATE')</OnLoad>
<OnEvent>
 ORDER=ORDER..'First;'
 Earlier:RegisterEvent('CHARACTER_LIST_UPDATE')
 Later:RegisterEvent('CHARACTER_LIST_UPDATE')
 Removed:UnregisterAllEvents()
 All:UnregisterEvent('CHARACTER_LIST_UPDATE')
 local new = CreateFrame('Frame', 'New')
 new:RegisterEvent('CHARACTER_LIST_UPDATE')
 new:SetScript('OnEvent', function() ORDER=ORDER..'New;' end)
 self:UnregisterAllEvents()
</OnEvent></Scripts></Frame>"#,
    );
    for index in 0..2048 {
        xml.push_str(&format!("<Frame name=\"Unrelated{index}\"/>"));
    }
    xml.push_str(r#"
<Frame name="Later"><Scripts><OnEvent>ORDER=ORDER..'Later;'</OnEvent></Scripts></Frame>
<Frame name="Removed"><Scripts><OnLoad>self:RegisterAllEvents()</OnLoad><OnEvent>
 if event=='CHARACTER_LIST_UPDATE' then error('removed subscriber') end
</OnEvent></Scripts></Frame>
<Frame name="All"><Scripts><OnLoad>self:RegisterEvent('CHARACTER_LIST_UPDATE');self:RegisterAllEvents()</OnLoad><OnEvent>
 if event=='CHARACTER_LIST_UPDATE' then ORDER=ORDER..'All;' end
</OnEvent></Scripts></Frame></Ui>"#);
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface/GlueXML/GlueXML.toc",
            bytes: b"Events.xml\n",
        },
        FixtureFile {
            path: "Interface/GlueXML/Events.xml",
            bytes: xml.as_bytes(),
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (800, 600), false)?;
    let report = manager.dispatch_event("CHARACTER_LIST_UPDATE", &UiEventPayload::empty())?;
    assert_eq!(report.subscriber_count(), 3);
    assert_eq!(
        manager.bundle().lua().globals().get::<String>("ORDER")?,
        "First;Later;All;"
    );
    manager.bundle().lua().globals().set("ORDER", "")?;
    let report = manager.dispatch_event("CHARACTER_LIST_UPDATE", &UiEventPayload::empty())?;
    assert_eq!(report.subscriber_count(), 4);
    assert_eq!(
        manager.bundle().lua().globals().get::<String>("ORDER")?,
        "Earlier;Later;All;New;"
    );
    manager.bundle().lua().load("Earlier:UnregisterAllEvents(); Later:UnregisterAllEvents(); All:UnregisterAllEvents(); New:UnregisterAllEvents()").exec()?;
    assert_eq!(
        manager
            .dispatch_event("CHARACTER_LIST_UPDATE", &UiEventPayload::empty())?
            .subscriber_count(),
        0
    );
    Ok(())
}
