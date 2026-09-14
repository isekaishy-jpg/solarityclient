//! Cancellation releases real partial Lua owners and their native sound guard.

// The shared archive fixture also serves loose-file integration tests.
#[allow(dead_code)]
#[path = "../stock_seed/support/mod.rs"]
mod support;

use crate::{AddonCatalog, FrameManager, FrameUiSources, UiGlueMediaAction, UiScriptEnvironment};
use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
use std::{error::Error, time::Duration};
use support::{Fixture, FixtureFile};

/// An actual pending FrameXML task restores admission when cancelled before load.
#[test]
fn cancelling_frame_construction_releases_assets_and_sound_admission() -> Result<(), Box<dyn Error>>
{
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface/FrameXML/Bindings.xml",
            bytes: b"<Bindings/>",
        },
        FixtureFile {
            path: "WTF/DefaultBindings.wtf",
            bytes: b"",
        },
        FixtureFile {
            path: "Interface/FrameXML/FrameXML.toc",
            bytes: b"",
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let assets = AssetStoreHandle::new(AssetStore::mount(catalog)?);
    let sources = FrameUiSources::load(&mut assets.borrow_mut())?;
    let environment = UiScriptEnvironment::new(800, 600, false)?;
    let media = environment.media_intent();
    let mut startup = FrameManager::begin_with_sources(
        assets.clone(),
        environment,
        &[],
        &AddonCatalog::default(),
        sources,
    );
    assert!(startup.advance(Duration::ZERO).is_pending());
    // No store borrow survives a cooperative checkpoint.
    drop(assets.borrow_mut());
    media.borrow_mut().play_sound_entry("during".into());
    assert_eq!(media.borrow_mut().take_action(), None);
    drop(startup);
    media.borrow_mut().play_sound_entry("after".into());
    assert_eq!(
        media.borrow_mut().take_action(),
        Some(UiGlueMediaAction::PlaySound("after".into()))
    );
    Ok(())
}

/// Exercises callback-time geometry across suspended entry steps and cancellation.
#[test]
fn entry_publication_preserves_order_geometry_and_sound_scope() -> Result<(), Box<dyn Error>> {
    let base = zero_table(11, 1);
    let coefficients = zero_table(1100, 1);
    let slots = zero_table(0, 3);
    let mut files = vec![
        FixtureFile {
            path: "DBFilesClient/gtChanceToMeleeCritBase.dbc",
            bytes: &base,
        },
        FixtureFile {
            path: "DBFilesClient/gtChanceToSpellCritBase.dbc",
            bytes: &base,
        },
        FixtureFile {
            path: "DBFilesClient/PaperDollItemFrame.dbc",
            bytes: &slots,
        },
        FixtureFile {
            path: "Interface/FrameXML/Bindings.xml",
            bytes: b"<Bindings/>",
        },
        FixtureFile {
            path: "WTF/DefaultBindings.wtf",
            bytes: b"",
        },
        FixtureFile {
            path: "Interface/FrameXML/FrameXML.toc",
            bytes: b"Entry.xml\n",
        },
        FixtureFile {
            path: "Interface/FrameXML/Entry.xml",
            bytes: br#"<Ui><Frame name="Owner"><Size x="200" y="100"/>
<Anchors><Anchor point="CENTER"/></Anchors><Scripts>
<OnLoad>self:RegisterEvent("PLAYER_LOGIN"); self:RegisterEvent("PLAYER_ENTERING_WORLD")</OnLoad>
<OnEvent>
 if event == "PLAYER_LOGIN" then
  Child = CreateFrame("Frame", "EntryChild", self)
  Child:SetWidth(80); Child:SetHeight(40); Child:SetPoint("CENTER", self, "CENTER", 0, 0)
  Scroller = CreateFrame("ScrollFrame", "EntryScroll", self)
  Scroller:SetWidth(50); Scroller:SetHeight(20); Scroller:SetPoint("CENTER")
  Content = CreateFrame("Frame", "EntryContent", Scroller)
  Content:SetWidth(50); Content:SetHeight(0)
  Scroller:SetScript("OnScrollRangeChanged", function(self, x, y) RangeSeen = y end)
  Scroller:SetScrollChild(Content); Content:SetHeight(100)
 else
  assert(Child:GetWidth() == 80, "entry event order")
  assert(Child:GetRight() - Child:GetLeft() == 80, "entry child geometry")
  assert(RangeSeen == Scroller:GetVerticalScrollRange() and RangeSeen > 0, "entry range callback")
  Child:SetWidth(120)
 end
 PlaySound("entry")
</OnEvent></Scripts></Frame></Ui>"#,
        },
    ];
    for path in [
        "DBFilesClient/gtChanceToMeleeCrit.dbc",
        "DBFilesClient/gtChanceToSpellCrit.dbc",
        "DBFilesClient/gtOCTRegenHP.dbc",
        "DBFilesClient/gtRegenHPPerSpt.dbc",
        "DBFilesClient/gtRegenMPPerSpt.dbc",
    ] {
        files.push(FixtureFile {
            path,
            bytes: &coefficients,
        });
    }
    let fixture = Fixture::new(&files)?;
    for cancel in [false, true] {
        let catalog =
            ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
        let assets = AssetStoreHandle::new(AssetStore::mount(catalog)?);
        let environment = UiScriptEnvironment::new(800, 600, false)?;
        let media = environment.media_intent();
        let manager =
            FrameManager::start_shared(assets.clone(), environment, &[], &AddonCatalog::default())?;
        let callbacks = std::rc::Rc::new(std::cell::Cell::new(0));
        let steps = callbacks.clone();
        let mut events = ["PLAYER_LOGIN", "PLAYER_ENTERING_WORLD"].into_iter();
        let mut publication = manager.begin_suppressed_publication(move |manager| {
            let Some(event) = events.next() else {
                return std::ops::ControlFlow::Break(());
            };
            assert!(
                manager
                    .dispatch_event(event, &crate::UiEventPayload::empty())
                    .is_ok(),
                "ordered entry callback"
            );
            steps.set(steps.get() + 1);
            std::ops::ControlFlow::Continue(())
        });
        assert!(publication.advance(Duration::ZERO).is_pending());
        assert_eq!(callbacks.get(), 1);
        assert_eq!(media.borrow_mut().take_action(), None);
        drop(assets.borrow_mut());
        if cancel {
            drop(publication);
        } else {
            let mut polls = 0;
            loop {
                polls += 1;
                assert!(polls < 100, "finite entry publication");
                if let std::task::Poll::Ready(result) = publication.advance(Duration::ZERO) {
                    let (manager, ()) = result?;
                    assert_eq!(callbacks.get(), 2);
                    let child = (0..manager.geometry().region_count())
                        .find(|index| manager.object_name(*index) == Some("EntryChild"))
                        .ok_or("published entry child")?;
                    let bounds = manager
                        .geometry()
                        .region(child)
                        .ok_or("child geometry")?
                        .logical_bounds();
                    assert_eq!(bounds.right() - bounds.left(), 120.0);
                    break;
                }
            }
        }
        assert_eq!(media.borrow_mut().take_action(), None);
        media.borrow_mut().play_sound_entry("after".into());
        assert_eq!(
            media.borrow_mut().take_action(),
            Some(UiGlueMediaAction::PlaySound("after".into()))
        );
    }
    Ok(())
}

/// Supplies deterministic native character tables for a minimal world UI.
fn zero_table(records: u32, fields: u32) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [records, fields, fields * 4, 1] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.resize(21 + records as usize * fields as usize * 4, 0);
    bytes
}
