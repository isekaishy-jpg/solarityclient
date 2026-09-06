//! Message history contracts through static and dynamic native widget objects.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiAnimationPlan, UiBundle, UiFramePlan, UiLayoutPlan, UiManifestKind,
    UiObjectCatalog, UiObjectTree, UiRegionStatePlan, UiRuntimeTemplatePlan, UiScriptEnvironment,
    UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan, UiTexturePlan, UiTextureStatePlan,
};

use crate::support::{Fixture, FixtureFile};

#[test]
fn scrolling_message_ring_preserves_native_order_identifiers_and_resize_reset()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface/FrameXML/FrameXML.toc",
            bytes: b"Messages.xml\n",
        },
        FixtureFile {
            path: "Interface/FrameXML/Messages.xml",
            bytes: br#"<Ui>
<Font name="MessageFont"><FontHeight><AbsValue val="14"/></FontHeight></Font>
<ScrollingMessageFrame name="MessageTemplate" virtual="true" maxLines="3">
  <FontString inherits="MessageFont" justifyH="LEFT"/>
</ScrollingMessageFrame>
<ScrollingMessageFrame name="Messages" inherits="MessageTemplate"><Scripts><OnLoad>
    assert(self:GetMaxLines() == 3 and self:GetCurrentLine() == -1)
    assert(self:GetNumMessages() == 0)
    assert(self:GetHyperlinksEnabled() == 1)
    self:SetHyperlinksEnabled(0)
    assert(self:GetHyperlinksEnabled() == nil)
    self:SetHyperlinksEnabled()
    assert(self:GetHyperlinksEnabled() == 1)
    self:SetHyperlinksEnabled(nil)
    assert(self:GetHyperlinksEnabled() == nil)
    self:SetHyperlinksEnabled('enabled')
    assert(self:GetHyperlinksEnabled() == 1)
    self:AddMessage('first', 1, .5, 0, 4, false, 77, 6)
    self:AddMessage('second', 9, false, 88, 7)
    self:AddMessage('third', 1, 1, 1, 4, false, 77, 8)
    self:AddMessage('newest', 1, 1, 1, 4, false, 99, 9)
    assert(self:GetNumMessages() == 3 and self:GetNumMessages(77) == 1)
    assert(self:GetCurrentLine() == 0 and self:GetMessageInfo(1) == 'second')
    local text, access, color, kind = self:GetMessageInfo(1, 77)
    assert(text == 'third' and access == 77 and color == 4 and kind == 8)
    assert(select('#', self:GetMessageInfo(1)) == 4)
    assert(not pcall(self.GetMessageInfo, self, 0) and not pcall(self.GetMessageInfo, self, 4))
    self:AddMessage('older', .2, .4, .6, 2, true, 66, 1)
    assert(self:GetNumMessages() == 3 and self:GetMessageInfo(1) == 'older')
    assert(self:GetMessageInfo(2) == 'third' and self:GetMessageInfo(3) == 'newest')
    self:AddMessage('', 1, 1, 1)
    assert(self:GetNumMessages() == 3)
    assert(not pcall(self.SetMaxLines, self, 0) and self:GetNumMessages() == 3)
    self:SetMaxLines(3)
    assert(self:GetNumMessages() == 0 and self:GetCurrentLine() == -1)
    self:AddMessage('after resize')
    assert(self:GetCurrentLine() == 0)
    self:Clear()
    assert(self:GetNumMessages() == 0 and self:GetMaxLines() == 3)
    local dynamic = CreateFrame('ScrollingMessageFrame', 'DynamicMessages', nil, 'MessageTemplate')
    assert(dynamic:GetMaxLines() == 3)
    dynamic:AddMessage('dynamic')
    assert(dynamic:GetNumMessages() == 1 and self:GetNumMessages() == 0)
    local native = CreateFrame('ScrollingMessageFrame')
    assert(native:GetMaxLines() == 8)
</OnLoad></Scripts></ScrollingMessageFrame>
</Ui>"#,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Frame)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let catalog = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&catalog, &fonts)?;
    assert_eq!(
        tree.nodes().len(),
        1,
        "message font descriptors are not child regions"
    );
    let regions = UiRegionStatePlan::resolve(&tree, &UiLayoutPlan::from_tree(&tree)?)?;
    let frames = UiFramePlan::from_tree(&tree)?.resolve(&tree)?;
    let scripts = UiScriptPlan::from_tree(&tree, bundle.lua())?;
    let templates = UiRuntimeTemplatePlan::from_catalog(&catalog, &fonts, bundle.lua())?;
    let textures = UiTextureStatePlan::resolve(&tree, &UiTexturePlan::from_tree(&tree)?)?;
    let animations = UiAnimationPlan::from_tree(&tree)?;
    let plan = UiScriptRuntimePlan::new(
        &tree,
        &animations,
        &frames,
        &regions,
        &templates,
        &fonts,
        &textures,
    );
    let environment = UiScriptEnvironment::new(1024, 768, false)?;
    let mut runtime = UiScriptRuntime::new(&bundle, &plan, environment)?;
    runtime.execute_all(&bundle, &tree, &scripts)?;
    Ok(())
}
