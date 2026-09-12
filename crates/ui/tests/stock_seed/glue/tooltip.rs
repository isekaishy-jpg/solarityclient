//! First tooltip publication with real, portable glyph coverage and retained lines.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::GlueManager;

use crate::support::{Fixture, FixtureFile};

#[test]
fn first_tooltip_reveal_publishes_covered_glyphs_and_falls_back_for_new_coverage()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile { path: "Interface\\GlueXML\\GlueXML.toc", bytes: b"TooltipPublication.xml\n" },
        FixtureFile { path: "Fonts\\TooltipFixture.ttf", bytes: include_bytes!("../../fixtures/tooltip_fixture.ttf") },
        FixtureFile { path: "Interface\\GlueXML\\TooltipPublication.xml", bytes: br#"<Ui>
<Font name="TooltipFixtureFont" font="Fonts\TooltipFixture.ttf"><FontHeight><AbsValue val="16"/></FontHeight></Font>
<Frame name="Seed" hidden="true"><Layers><Layer>
  <FontString inherits="TooltipFixtureFont" text="ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"/>
</Layer></Layers></Frame>
<Button name="Owner" enableMouse="true"><Size x="100" y="40"/><Anchors><Anchor point="CENTER"/></Anchors>
<Scripts><OnEnter>
  Tip:SetOwner(self, "ANCHOR_RIGHT")
  Tip:SetMinimumWidth(180, 1)
  Tip:SetText(LABEL or "First")
  Tip:AddLine("Second")
</OnEnter><OnLeave>Tip:Hide()</OnLeave></Scripts></Button>
<GameTooltip name="Tip" hidden="true" frameStrata="TOOLTIP"><Size x="120" y="30"/>
<Layers><Layer level="BACKGROUND"><Texture name="TipBackground" setAllPoints="true"><Color r="0.1" g="0.2" b="0.3"/></Texture></Layer>
<Layer level="ARTWORK">
  <FontString name="$parentTextLeft1" inherits="TooltipFixtureFont" hidden="true"/>
  <FontString name="$parentTextRight1" inherits="TooltipFixtureFont" hidden="true"/>
  <FontString name="$parentTextLeft2" inherits="TooltipFixtureFont" hidden="true"/>
  <FontString name="$parentTextRight2" inherits="TooltipFixtureFont" hidden="true"/>
  <FontString name="$parentTextLeft3" inherits="TooltipFixtureFont" hidden="true"/>
  <FontString name="$parentTextRight3" inherits="TooltipFixtureFont" hidden="true"/>
</Layer></Layers></GameTooltip>
</Ui>"# },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1280, 720), false)?;
    let index = |name: &str| {
        manager
            .objects()
            .iter()
            .position(|object| object.name() == Some(name))
            .ok_or("missing fixture object")
    };
    let owner = index("Owner")?;
    let tip = index("Tip")?;
    let background = index("TipBackground")?;
    let first = index("TipTextLeft1")?;
    let second = index("TipTextLeft2")?;
    let owner_bounds = manager
        .geometry()
        .region(owner)
        .ok_or("owner bounds")?
        .presentation_bounds();
    let pointer = (
        (owner_bounds.left() + owner_bounds.right()) * 0.5,
        (owner_bounds.bottom() + owner_bounds.top()) * 0.5,
    );
    let atlas = manager.glyphs().identity();
    let snapshots = manager.runtime_snapshot_count();
    assert!(manager.glyphs().quads(manager.geometry()).is_empty());
    for (label, glyph_count, retained_coverage) in [
        ("First", 5, true),
        ("Changed", 7, true),
        ("\u{03a9}", 1, false),
    ] {
        manager.bundle().lua().globals().set("LABEL", label)?;
        manager.pointer_motion(pointer)?;
        assert!(manager.take_callback_failure().is_none());
        let quads = manager.glyphs().quads(manager.geometry());
        assert_eq!(
            quads
                .iter()
                .filter(|quad| quad.object_index() == first)
                .count(),
            glyph_count
        );
        assert_eq!(
            quads
                .iter()
                .filter(|quad| quad.object_index() == second)
                .count(),
            6
        );
        assert_eq!(manager.glyphs().identity() == atlas, retained_coverage);
        let mesh = manager.render_plan().mesh();
        for (object, expected) in [(first, glyph_count), (second, 6), (background, 1)] {
            let visible = mesh
                .object_indices()
                .iter()
                .enumerate()
                .filter(|(quad, index)| {
                    **index == object && mesh.vertices()[quad * 4].color()[3] > 0.0
                })
                .count();
            assert_eq!(visible, expected);
        }
        let bounds = manager
            .geometry()
            .region(tip)
            .ok_or("tooltip bounds")?
            .logical_bounds();
        assert!(bounds.width() >= 180.0);
        assert_eq!(
            manager
                .geometry()
                .region(background)
                .ok_or("background bounds")?
                .logical_bounds(),
            bounds
        );
        assert!(
            manager
                .presentation()
                .members_in_draw_order()
                .iter()
                .any(|member| member.object_index() == background && member.opacity() > 0.0)
        );
        manager.pointer_motion((-1.0, -1.0))?;
        assert!(manager.glyphs().quads(manager.geometry()).is_empty());
    }
    assert_eq!(manager.runtime_snapshot_count(), snapshots);
    Ok(())
}
