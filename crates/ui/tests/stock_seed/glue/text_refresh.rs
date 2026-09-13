//! Incremental labels and edit boxes must match a fresh complete publication.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::GlueManager;

use crate::support::{Fixture, FixtureFile};

/// Hidden peers, duplicate maxima and dynamic frames all participate in Raise.
#[test]
fn frame_raise_tracks_lowered_levels_strata_changes_and_dynamic_registration()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile { path: "Interface\\GlueXML\\GlueXML.toc", bytes: b"Order.xml\n" },
        FixtureFile { path: "Interface\\GlueXML\\Order.xml", bytes: br#"<Ui>
<Frame name="Owner" toplevel="true" frameLevel="1"><Size x="100" y="100"/><Anchors><Anchor point="CENTER"/></Anchors>
<Frames><Frame name="Child" frameLevel="2"/></Frames></Frame>
<Frame name="First" hidden="true" frameLevel="5"/>
<Frame name="Second" hidden="true" frameLevel="5"/>
</Ui>"# },
    ])?;
    let manager = start(&fixture)?;
    manager
        .bundle()
        .lua()
        .load(
            r#"
Owner:Raise()
assert(Owner:GetFrameLevel() == 6 and Child:GetFrameLevel() == 7)
Owner:SetFrameLevel(1)
First:SetFrameLevel(2)
Second:SetFrameStrata("LOW")
Owner:Raise()
assert(Owner:GetFrameLevel() == 3 and Child:GetFrameLevel() == 4)
local later = CreateFrame("Frame", "Later")
later:SetFrameLevel(40)
Owner:Raise()
assert(Owner:GetFrameLevel() == 41 and Child:GetFrameLevel() == 42)
later:SetFrameStrata("HIGH")
Owner:SetFrameLevel(0)
Owner:Raise()
assert(Owner:GetFrameLevel() == 3 and Child:GetFrameLevel() == 4)
Second:SetFrameStrata("MEDIUM")
Owner:Raise()
assert(Owner:GetFrameLevel() == 6 and Child:GetFrameLevel() == 7)
"#,
        )
        .exec()?;
    Ok(())
}

/// Stock text layout is unchanged by retaining only the dirty owners' metadata.
#[test]
fn sparse_text_refresh_matches_fresh_labels_and_edit_box_selection() -> Result<(), Box<dyn Error>> {
    let fixture = text_fixture("12:34", "abc")?;
    let mut retained = start(&fixture)?;
    let atlas = retained.glyphs().identity();
    let snapshots = retained.runtime_snapshot_count();
    for (label, edit) in [("12:35", "abcdef"), ("", ""), ("1", "ab"), ("12:34", "abc")] {
        retained.bundle().lua().globals().set("LABEL", label)?;
        retained.bundle().lua().globals().set("EDIT", edit)?;
        retained.update(0.0)?;
        assert!(retained.take_callback_failure().is_none());
        let expected_fixture = text_fixture(label, edit)?;
        let expected = start(&expected_fixture)?;
        let visible = |manager: &GlueManager| {
            manager
                .glyphs()
                .quads(manager.geometry())
                .into_iter()
                .filter(|quad| quad.color()[3] > 0.0)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            visible(&retained),
            visible(&expected),
            "label={label:?}, edit={edit:?}"
        );
        assert_eq!(retained.glyphs().identity(), atlas);
        assert_eq!(retained.runtime_snapshot_count(), snapshots);
    }
    Ok(())
}

/// Independent initial publications cover growth, empty strings and caret state.
fn text_fixture(label: &str, edit: &str) -> Result<Fixture, Box<dyn Error>> {
    text_fixture_at_order(label, edit, 0, "MEDIUM")
}

/// Frame order updates must move descendant glyph packets as well as textures.
#[test]
fn frame_order_journal_preserves_descendant_glyph_order() -> Result<(), Box<dyn Error>> {
    let fixture = text_fixture_at_order("12:34", "abc", 0, "MEDIUM")?;
    let mut retained = start(&fixture)?;
    let snapshots = retained.runtime_snapshot_count();
    for (level, strata) in [(5, "HIGH"), (1, "BACKGROUND"), (8, "LOW")] {
        retained.bundle().lua().globals().set("LEVEL", level)?;
        retained.bundle().lua().globals().set("STRATA", strata)?;
        retained.update(0.0)?;
        assert!(retained.take_callback_failure().is_none());
        let fixture = text_fixture_at_order("12:34", "abc", level, strata)?;
        let expected = start(&fixture)?;
        assert_eq!(
            retained.glyphs().quads(retained.geometry()),
            expected.glyphs().quads(expected.geometry())
        );
        assert_eq!(
            retained.render_plan().mesh().object_indices(),
            expected.render_plan().mesh().object_indices()
        );
        assert_eq!(
            retained.render_plan().mesh().vertices(),
            expected.render_plan().mesh().vertices()
        );
        assert_eq!(retained.runtime_snapshot_count(), snapshots);
    }
    Ok(())
}

/// Initial load and OnUpdate supply equivalent content/order through real Lua APIs.
fn text_fixture_at_order(
    label: &str,
    edit: &str,
    level: i32,
    strata: &str,
) -> Result<Fixture, Box<dyn Error>> {
    let mut xml = format!(
        r#"<Ui>
<Font name="FixtureFont" font="Fonts\TooltipFixture.ttf"><FontHeight><AbsValue val="16"/></FontHeight></Font>
<Frame name="Root"><Size x="600" y="500"/><Anchors><Anchor point="CENTER"/></Anchors>
<Layers><Layer>
<FontString name="Coverage" inherits="FixtureFont" text="0123456789:abcdef" hidden="true"/>
<FontString name="Ticker" inherits="FixtureFont" text="{label}"><Size x="120" y="24"/><Anchors><Anchor point="TOPLEFT"/></Anchors></FontString>
<FontString name="Unchanged" inherits="FixtureFont" text="abcdef"><Size x="120" y="24"/><Anchors><Anchor point="BOTTOMRIGHT"/></Anchors></FontString>
</Layer></Layers>
<Frames><EditBox name="Entry" autoFocus="false"><Size x="150" y="30"/><Anchors><Anchor point="CENTER"/></Anchors>
<Scripts><OnLoad>self:SetFontObject(FixtureFont); self:SetText("{edit}"); self:SetCursorPosition(1); self:HighlightText(0, 2)</OnLoad></Scripts>
</EditBox></Frames>
<Scripts><OnLoad>self:SetFrameLevel({level}); self:SetFrameStrata("{strata}")</OnLoad><OnUpdate>if LABEL ~= nil then
Ticker:SetText(LABEL); Entry:SetText(EDIT); Entry:SetCursorPosition(1); Entry:HighlightText(0, 2); LABEL = nil
end
if LEVEL ~= nil then self:SetFrameLevel(LEVEL); LEVEL = nil end
if STRATA ~= nil then self:SetFrameStrata(STRATA); STRATA = nil end
</OnUpdate></Scripts></Frame>"#
    );
    // Loaded but unrelated objects must not affect the requested text result.
    for index in 0..128 {
        xml.push_str(&format!(
            r#"<Frame name="Unrelated{index}" hidden="true"/>"#
        ));
    }
    xml.push_str("</Ui>");
    Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"TextRefresh.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\TextRefresh.xml",
            bytes: xml.as_bytes(),
        },
        FixtureFile {
            path: "Fonts\\TooltipFixture.ttf",
            bytes: include_bytes!("../../fixtures/tooltip_fixture.ttf"),
        },
    ])
}

/// Mounts the portable fixture through the ordinary retained presentation owner.
fn start(fixture: &Fixture) -> Result<GlueManager, Box<dyn Error>> {
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    Ok(GlueManager::start(
        AssetStore::mount(catalog)?,
        (1280, 720),
        false,
    )?)
}
