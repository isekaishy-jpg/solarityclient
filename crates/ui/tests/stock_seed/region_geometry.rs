//! Native anchor, scale and clamp output through live Lua and retained geometry.

use std::error::Error;
use std::fmt::Write;

use mlua::{ObjectLike, Table};
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::GlueManager;

use crate::support::{Fixture, FixtureFile};

const POINTS: [&str; 9] = [
    "TOPLEFT",
    "TOP",
    "TOPRIGHT",
    "LEFT",
    "CENTER",
    "RIGHT",
    "BOTTOMLEFT",
    "BOTTOM",
    "BOTTOMRIGHT",
];

#[test]
fn cursor_position_uses_native_canvas_units_at_different_window_sizes() -> Result<(), Box<dyn Error>>
{
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface/GlueXML/GlueXML.toc",
            bytes: b"Geometry.xml\n",
        },
        FixtureFile {
            path: "Interface/GlueXML/Geometry.xml",
            bytes: b"<Ui><Frame name=\"Root\"/></Ui>",
        },
    ])?;
    let rows = include_str!("../fixtures/ui_cursor_position_native.txt")
        .lines()
        .filter(|row| !row.starts_with('#'))
        .map(|row| {
            row.split_whitespace()
                .map(str::parse::<f64>)
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    for chunk in rows.as_chunks::<4>().0 {
        let extent = (chunk[0][0] as u32, chunk[0][1] as u32);
        let catalog =
            ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
        let mut manager = GlueManager::start(AssetStore::mount(catalog)?, extent, false)?;
        for row in chunk {
            manager.pointer_motion((row[2] * 768. / row[1], row[3] * 768. / row[1]))?;
            let position: mlua::Function =
                manager.bundle().lua().globals().get("GetCursorPosition")?;
            let (x, y): (f64, f64) = position.call(())?;
            assert!(
                (x - row[4]).abs() < 0.0003 && (y - row[5]).abs() < 0.0003,
                "{extent:?}: ({x}, {y}) != ({}, {})",
                row[4],
                row[5]
            );
        }
    }
    Ok(())
}

#[test]
fn cursor_tooltips_match_native_updates_and_clamp_at_scaled_screen_edges()
-> Result<(), Box<dyn Error>> {
    let rows = include_str!("../fixtures/tooltip_cursor_native.txt")
        .lines()
        .filter(|row| !row.starts_with('#'))
        .map(|row| {
            row.split_whitespace()
                .map(str::parse::<f64>)
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut xml = String::from(
        r#"<Ui>
<Font name="TipFont"><FontHeight><AbsValue val="12"/></FontHeight></Font>
<Frame name="Root" scale="0.5"><Size x="900" y="600"/>
<Anchors><Anchor point="BOTTOMLEFT"><Offset x="80" y="40"/></Anchor></Anchors><Frames>"#,
    );
    for (index, row) in rows.iter().take(12).enumerate() {
        let kind = if row[0] == 8. {
            "ANCHOR_CURSOR"
        } else {
            "ANCHOR_CURSOR_RIGHT"
        };
        writeln!(
            xml,
            r#"<GameTooltip name="Tip{index}" scale="{}" hidden="true" frameStrata="TOOLTIP">
<Layers><Layer><FontString name="$parentTextLeft1" inherits="TipFont" hidden="true"/>
<FontString name="$parentTextRight1" inherits="TipFont" hidden="true"/></Layer></Layers>
<Scripts><OnLoad>
self:SetPoint('TOP', Root, 'TOP', 30, -20)
self:SetOwner(Root, '{kind}', {}, {})
assert(self:GetNumPoints() == 0)
self:SetText('Pointer')
self:SetSize(180, 60)
if self:GetName() == 'Tip0' then
 self:SetScript('OnUpdate', function(self)
  self:SetPoint('BOTTOM', nil, 'BOTTOMLEFT', 1, 1)
  self:SetScript('OnUpdate', nil)
 end)
end
</OnLoad></Scripts></GameTooltip>"#,
            row[1] * 2.,
            row[4],
            row[5]
        )?;
    }
    xml.push_str("</Frames></Frame></Ui>");
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface/GlueXML/GlueXML.toc",
            bytes: b"Geometry.xml\n",
        },
        FixtureFile {
            path: "Interface/GlueXML/Geometry.xml",
            bytes: xml.as_bytes(),
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    // A different physical height also exercises the pointer-to-UI conversion.
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1536, 1152), false)?;
    for chunk in rows.as_chunks::<12>().0 {
        manager.pointer_motion((chunk[0][2], chunk[0][3]))?;
        assert!(manager.update(0.01)?);
        assert!(!manager.update(0.01)?, "stationary cursor rebuilt the UI");
        for (index, row) in chunk.iter().enumerate() {
            let name = format!("Tip{index}");
            let tooltip: Table = manager.bundle().lua().globals().get(name.as_str())?;
            let (point, target, relative, x, y): (String, Option<Table>, String, f64, f64) =
                tooltip.call_method("GetPoint", ())?;
            assert_eq!(point, POINTS[row[6] as usize]);
            assert!(target.is_none());
            assert_eq!(relative, "BOTTOMLEFT");
            assert!((x - row[7]).abs() < 0.0003 && (y - row[8]).abs() < 0.0003);
            let scale = row[1];
            let width = 180. * scale;
            let height = 60. * scale;
            let left = (row[7] * scale - if row[6] == 7. { width / 2. } else { 0. })
                .clamp(0., 1024. - width);
            let bottom = (row[8] * scale).clamp(0., 768. - height);
            let object = manager
                .objects()
                .iter()
                .position(|object| object.name() == Some(name.as_str()))
                .ok_or("tooltip object")?;
            let bounds = manager
                .geometry()
                .region(object)
                .ok_or("tooltip bounds")?
                .presentation_bounds();
            for (actual, expected) in [bounds.left(), bounds.bottom(), bounds.right(), bounds.top()]
                .into_iter()
                .zip([left, bottom, left + width, bottom + height])
            {
                assert!(
                    (actual - expected).abs() < 0.0003,
                    "{name}: {actual} != {expected}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn scaled_root_keeps_screen_anchors_and_live_clamp_moves_its_children() -> Result<(), Box<dyn Error>>
{
    let fixture = Fixture::new(&[
        FixtureFile { path: "Interface/GlueXML/GlueXML.toc", bytes: b"Geometry.xml\n" },
        FixtureFile { path: "Interface/GlueXML/Geometry.xml", bytes: br#"<Ui>
<Frame name="Root" scale="0.5" setAllPoints="true"><Frames>
<Frame name="Child"><Size x="180" y="60"/>
<Anchors><Anchor point="BOTTOMLEFT" relativeTo="Root" relativePoint="BOTTOMRIGHT"><Offset x="3" y="-4"/></Anchor></Anchors>
<Layers><Layer><Texture name="Fill" setAllPoints="true"><Color r="1" g="0" b="0"/></Texture></Layer></Layers>
<Scripts><OnLoad>self:RegisterEvent('CHARACTER_LIST_UPDATE')</OnLoad><OnEvent>
 self:SetClampRectInsets(10,-20,30,-40)
 self:SetClampedToScreen(true)
 assert(self:GetEffectiveScale() == 0.5)
 assert(self:GetLeft() == 1888 and self:GetBottom() == 40)
 assert(self:GetRight() == 2068 and self:GetTop() == 100)
</OnEvent></Scripts></Frame></Frames></Frame></Ui>"# },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1024, 768), false)?;
    let bounds = |manager: &GlueManager, name: &str| -> Result<[f64; 4], Box<dyn Error>> {
        let index = manager
            .objects()
            .iter()
            .position(|object| object.name() == Some(name))
            .ok_or("missing scaled region")?;
        let bounds = manager
            .geometry()
            .region(index)
            .ok_or("geometry")?
            .presentation_bounds();
        Ok([bounds.left(), bounds.bottom(), bounds.right(), bounds.top()])
    };
    assert_eq!(bounds(&manager, "Root")?, [0., 0., 1024., 768.]);
    assert_eq!(bounds(&manager, "Child")?, [1025.5, -2., 1115.5, 28.]);
    manager.dispatch_event(
        "CHARACTER_LIST_UPDATE",
        &solarity_ui::UiEventPayload::empty(),
    )?;
    assert_eq!(bounds(&manager, "Child")?, [944., 20., 1034., 50.]);
    assert_eq!(bounds(&manager, "Fill")?, [944., 20., 1034., 50.]);
    Ok(())
}

#[test]
fn region_scale_and_clamp_match_native_edge_resolution() -> Result<(), Box<dyn Error>> {
    let mut xml = String::from("<Ui>");
    let mut expected = Vec::new();
    for (index, row) in include_str!("../fixtures/ui_region_layout_native.txt")
        .lines()
        .filter(|row| !row.starts_with('#'))
        .enumerate()
    {
        let values = row
            .split_whitespace()
            .map(str::parse::<f64>)
            .collect::<Result<Vec<_>, _>>()?;
        let scale = values[0];
        let count = values[12] as usize;
        writeln!(
            xml,
            r#"<Frame name="Target{index}"><Size x="{}" y="{}"/>
<Anchors><Anchor point="BOTTOMLEFT"><Offset x="{}" y="{}"/></Anchor></Anchors></Frame>
<Frame name="Case{index}" scale="{scale}" clampedToScreen="{}"><Size x="{}" y="{}"/><Anchors>"#,
            values[5] - values[3],
            values[6] - values[4],
            values[3],
            values[4],
            values[7] != 0.,
            values[1],
            values[2]
        )?;
        for point in values[13..13 + count * 4].as_chunks::<4>().0 {
            writeln!(
                xml,
                r#"<Anchor point="{}" relativeTo="Target{index}" relativePoint="{}"><Offset x="{}" y="{}"/></Anchor>"#,
                POINTS[point[0] as usize], POINTS[point[1] as usize], point[2], point[3]
            )?;
        }
        writeln!(
            xml,
            r#"</Anchors><Scripts><OnLoad>self:SetClampRectInsets({}, {}, {}, {})</OnLoad></Scripts></Frame>"#,
            values[8], values[9], values[10], values[11]
        )?;
        expected.push((scale, <[f64; 4]>::try_from(&values[13 + count * 4..])?));
    }
    xml.push_str("</Ui>");
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface/GlueXML/GlueXML.toc",
            bytes: b"Geometry.xml\n",
        },
        FixtureFile {
            path: "Interface/GlueXML/Geometry.xml",
            bytes: xml.as_bytes(),
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1024, 768), false)?;
    for (index, (scale, expected)) in expected.into_iter().enumerate() {
        let name = format!("Case{index}");
        let object = manager
            .objects()
            .iter()
            .position(|object| object.name() == Some(name.as_str()))
            .ok_or("native region missing")?;
        let region = manager.geometry().region(object).ok_or("region geometry")?;
        let rect = region.presentation_bounds();
        for (actual, expected) in [rect.left(), rect.bottom(), rect.right(), rect.top()]
            .into_iter()
            .zip(expected)
        {
            assert!(
                (actual - expected).abs() < 0.0003,
                "{name}: rendered {actual}, native {expected}"
            );
        }
        let table: Table = manager.bundle().lua().globals().get(name.as_str())?;
        for (method, expected) in ["GetLeft", "GetBottom", "GetRight", "GetTop"]
            .into_iter()
            .zip(expected)
        {
            let actual = table.call_method::<f64>(method, ())?;
            assert!(
                (actual * scale - expected).abs() < 0.0003,
                "{name} {method}: queried {actual} at scale {scale}, native {expected}"
            );
        }
    }
    Ok(())
}
