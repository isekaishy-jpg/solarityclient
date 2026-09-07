//! Native status-bar XML, script callbacks, and rendered fill behavior.

use crate::support::{Fixture, FixtureFile};
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::GlueManager;
use std::error::Error;

#[test]
fn status_bar_fill_tracks_native_range_color_axis_and_texture_replacement()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile { path: "Interface/GlueXML/GlueXML.toc", bytes: b"Bars.xml\n" },
        FixtureFile { path: "Interface/GlueXML/Bars.xml", bytes: br#"<Ui>
<StatusBar name="BarTemplate" virtual="true" minValue="10" maxValue="110" defaultValue="35" drawLayer="OVERLAY">
  <Size x="200" y="40"/><Anchors><Anchor point="BOTTOMLEFT"/></Anchors>
  <BarTexture name="$parentFill"><Color r="1" g="1" b="1"/></BarTexture>
  <BarColor g="1"/>
</StatusBar>
<StatusBar name="Health" inherits="BarTemplate"><Scripts><OnLoad>
    assert(self:GetValue() == 35 and self:GetOrientation() == 'HORIZONTAL')
    assert(self:GetStatusBarTexture() == HealthFill and HealthFill:GetDrawLayer() == 'OVERLAY')
    local r,g,b,a = self:GetStatusBarColor()
    assert(r == 0 and g == 1 and b == 0 and a == 1)
    local dynamic = CreateFrame('StatusBar', 'Dynamic', nil, 'BarTemplate')
    assert(dynamic:GetValue() == 35 and dynamic:GetStatusBarTexture() == DynamicFill)
    dynamic:SetOrientation('vertical')
    dynamic:SetRotatesTexture('true')
    assert(dynamic:GetRotatesTexture() == 1)
    local u1,v1,u2,v2,u3,v3,u4,v4 = DynamicFill:GetTexCoord()
    assert(u1 == 0 and v1 == 1 and u2 == 1 and v2 == 1 and u3 == 0 and v3 == 0 and u4 == 1 and v4 == 0)
    local blank = CreateFrame('StatusBar')
    assert(blank:GetValue() == 0 and blank:GetRotatesTexture() == nil)
    blank:SetValue(100) -- ignored until the first valid range
    assert(blank:GetValue() == 0)
    local events = {}
    blank:SetScript('OnMinMaxChanged', function(bar, minimum, maximum)
        events[#events+1] = 'range:'..minimum..':'..maximum..':'..bar:GetValue()
    end)
    blank:SetScript('OnValueChanged', function(bar, value) events[#events+1] = 'value:'..value end)
    blank:SetMinMaxValues(0, 100)
    blank:SetValue(0) -- first valid zero must dispatch
    blank:SetValue(0)
    blank:SetValue(80)
    blank:SetMinMaxValues(10, 50)
    blank:SetMinMaxValues(10, 50)
    assert(table.concat(events, ',') == 'range:0:100:0,value:0,value:80,range:10:50:80,value:50')
    assert(not pcall(blank.SetMinMaxValues, blank, 0, 1000000000000))
    assert(not pcall(blank.SetMinMaxValues, blank, -2000000000000, 0))
    blank:SetMinMaxValues(80, 20)
    local minimum, maximum = blank:GetMinMaxValues()
    assert(minimum == 20 and maximum == 20 and blank:GetValue() == 20)
    blank:SetStatusBarColor(0, 0, 0)
    local r,g,b,a = blank:GetStatusBarColor()
    assert(r == 1 and g == 1 and b == 1 and a == 1)
    blank:SetStatusBarTexture(nil, false)
    self.tick = 0
</OnLoad><OnUpdate>
    self.tick = self.tick + 1
    if self.tick == 1 then
        assert(HealthFill:GetWidth() == 50 and DynamicFill:GetHeight() == 10)
        assert(HealthFill:GetNumPoints() == 4)
        local p,relative,rp,x,y = HealthFill:GetPoint(2)
        assert(p == 'TOPRIGHT' and relative == self and rp == 'TOPRIGHT' and x == -150 and y == 0)
        self:SetValue(60) Dynamic:SetValue(110)
    elseif self.tick == 2 then self:SetValue(10) Dynamic:SetMinMaxValues(0,0)
    elseif self.tick == 3 then
        local replacement = Dynamic:CreateTexture('Replacement')
        replacement:SetTexture(1,1,1)
        self:SetStatusBarTexture(replacement, 'BORDER')
        self:SetStatusBarTexture(replacement, 'invalid') -- same pointer retains its layer
        assert(replacement:GetDrawLayer() == 'BORDER')
        assert(replacement:GetParent() == self and HealthFill == nil)
        self:SetValue(110)
        self:SetStatusBarColor(.25, .5, .75, .8)
        local r,g,b,a = self:GetStatusBarColor()
        assert(math.abs(r-64/255) &lt; .00001 and math.abs(g-128/255) &lt; .00001)
        assert(math.abs(b-191/255) &lt; .00001 and math.abs(a-204/255) &lt; .00001)
        replacement:SetVertexColor(1,0,0,1)
        assert(self:GetStatusBarColor() == 1)
    elseif self.tick == 4 then self:SetStatusBarTexture(nil)
    end
</OnUpdate></Scripts></StatusBar>
<StatusBar name="ColorBeforeTexture" minValue="0" maxValue="1" defaultValue="1">
 <Size x="10" y="10"/><BarColor r="1" g="0" b="0"/><BarTexture name="Untinted"><Color r="1" g="1" b="1"/></BarTexture>
</StatusBar>
</Ui>"# },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1024, 768), false)?;
    let index = |manager: &GlueManager, name: &str| {
        manager
            .objects()
            .iter()
            .position(|o| o.name() == Some(name))
            .ok_or("missing region")
    };
    let health = index(&manager, "HealthFill")?;
    let dynamic = index(&manager, "DynamicFill")?;
    let check = |manager: &GlueManager,
                 index,
                 width: f64,
                 height: f64,
                 shown|
     -> Result<(), Box<dyn Error>> {
        let region = manager.geometry().region(index).ok_or("missing geometry")?;
        let bounds = region.logical_bounds();
        assert!(
            (bounds.width() - width).abs() < 0.001,
            "width {} expected {width}",
            bounds.width()
        );
        assert!(
            (bounds.height() - height).abs() < 0.001,
            "height {} expected {height}",
            bounds.height()
        );
        assert_eq!(region.effectively_shown(), shown);
        assert!(bounds.left().abs() < 0.001 && bounds.bottom().abs() < 0.001);
        Ok(())
    };
    check(&manager, health, 50., 40., true)?;
    check(&manager, dynamic, 200., 10., true)?;
    let members = manager.presentation().members_in_draw_order();
    let fill = members
        .iter()
        .find(|m| m.object_index() == health)
        .ok_or("missing fill")?;
    assert_eq!(fill.vertex_colors(), [[0., 1., 0., 1.]; 4]);

    assert_eq!(
        fill.tex_coords(),
        [0., 0., 0., 1., 1., 0., 1., 1.],
        "native stretches the entire texture"
    );
    let untinted = index(&manager, "Untinted")?;
    assert_eq!(
        members
            .iter()
            .find(|m| m.object_index() == untinted)
            .ok_or("missing untinted")?
            .vertex_colors(),
        [[1.; 4]; 4]
    );
    let index_storage = manager.render_plan().mesh().indices().as_ptr();
    let snapshots = manager.runtime_snapshot_count();
    let glyph_identity = manager.glyphs().identity();
    manager.update(0.01)?;
    assert!(manager.take_callback_failure().is_none());
    assert_eq!(
        manager.render_plan().mesh().indices().as_ptr(),
        index_storage
    );
    assert_eq!(manager.glyphs().identity(), glyph_identity);
    assert_eq!(manager.runtime_snapshot_count(), snapshots);
    check(&manager, health, 100., 40., true)?;
    check(&manager, dynamic, 200., 40., true)?;
    manager.update(0.01)?;
    assert!(manager.take_callback_failure().is_none());
    check(&manager, health, 100., 40., false)?;
    check(&manager, dynamic, 200., 40., false)?;
    manager.update(0.01)?;
    assert!(manager.take_callback_failure().is_none());
    let replacement = index(&manager, "Replacement")?;
    check(&manager, replacement, 200., 40., true)?;
    assert!(
        !manager
            .geometry()
            .region(health)
            .ok_or("retired geometry")?
            .effectively_shown()
    );
    manager.update(0.01)?;
    assert!(manager.take_callback_failure().is_none());
    assert!(
        !manager
            .geometry()
            .region(replacement)
            .ok_or("cleared geometry")?
            .effectively_shown()
    );
    Ok(())
}
