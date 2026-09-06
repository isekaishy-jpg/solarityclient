//! Native GameTooltip line lifetime through the production XML/Lua runtime.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiAnimationPlan, UiBundle, UiFramePlan, UiLayoutPlan, UiManifestKind,
    UiObjectCatalog, UiObjectTree, UiRegionStatePlan, UiRuntimeTemplatePlan, UiScriptEnvironment,
    UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan, UiTexturePlan, UiTextureStatePlan,
};

use crate::support::{Fixture, FixtureFile};

/// 0x0061FEC0 retains a spare font pair; 0x0061C620 clears before firing the callback;
/// 0x0061CFF0 requires an owner and content, including SetText's implicit Show.
#[test]
fn tooltip_retains_lines_and_obeys_owner_visibility_lifetime() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Tooltip.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Tooltip.xml",
            // The font has no face: this fixture tests line/owner lifetime. Installed
            // FrameXML replay separately exercises measurement with the stock face.
            bytes: br#"<Ui>
<Font name="TooltipFont"><FontHeight><AbsValue val="12"/></FontHeight></Font>
<Frame name="Owner"/>
<GameTooltip name="TipTemplate" virtual="true" hidden="true"><Layers><Layer>
  <FontString name="$parentTextLeft1" inherits="TooltipFont" hidden="true"/>
  <FontString name="$parentTextRight1" inherits="TooltipFont" hidden="true"/>
</Layer></Layers></GameTooltip>
<GameTooltip name="Tip" hidden="true"><Layers><Layer level="ARTWORK">
  <FontString name="$parentTextLeft1" inherits="TooltipFont" hidden="true"/>
  <FontString name="$parentTextRight1" inherits="TooltipFont" hidden="true"/>
</Layer></Layers><Scripts><OnTooltipCleared>
  assert(this == self)
  assert(self:NumLines() == 0)
  assert(not TipTextLeft1:IsShown() and not TipTextRight1:IsShown())
  CLEARED = (CLEARED or 0) + 1
</OnTooltipCleared><OnShow>SHOWN = (SHOWN or 0) + 1</OnShow>
<OnHide>HIDDEN = (HIDDEN or 0) + 1</OnHide></Scripts></GameTooltip>
<Frame name="Check"><Scripts><OnLoad>
  assert(Tip:NumLines() == 0)
  Tip:ClearLines()
  assert(CLEARED == nil)
  Tip:SetOwner(Owner, "ANCHOR_RIGHT", 3, -4)
  assert(not Tip:IsShown())
  Tip:AddLine("First")
  assert(Tip:NumLines() == 1 and not Tip:IsShown())
  assert(TipTextLeft1:GetText() == "First" and TipTextLeft1:IsShown())
  assert(TipTextLeft2 and TipTextRight2 and not TipTextLeft2:IsShown())
  assert(TipTextLeft2:GetFontObject() == TooltipFont)
  Tip:AddDoubleLine("Second", "Value", 1, 0, 0, 0, 1, 0)
  assert(Tip:NumLines() == 2 and TipTextLeft3 and TipTextRight3)
  assert(TipTextRight2:GetText() == "Value" and TipTextRight2:IsShown())
  Tip:AddLine(nil)
  Tip:AddDoubleLine("", "")
  assert(Tip:NumLines() == 2)
  Tip:Show()
  assert(Tip:IsShown() and SHOWN == 1)
  local point, relative, relativePoint, x, y = Tip:GetPoint()
  assert(point == "BOTTOMLEFT" and relative == Owner and relativePoint == "TOPRIGHT")
  assert(x == 3 and y == -4)
  Tip:Show()
  assert(SHOWN == 1)
  local retained = TipTextLeft2
  Tip:SetText(17, 0, 0, 1, 0.5)
  assert(Tip:NumLines() == 1 and Tip:IsShown() and CLEARED == 1)
  assert(TipTextLeft1:GetText() == "17")
  assert(TipTextLeft2 == retained and not retained:IsShown())
  Tip:SetAlpha(0.25)
  Tip:Show()
  assert(Tip:GetAlpha() == 1)
  Tip:Hide()
  assert(HIDDEN == 1 and CLEARED == 2)
  assert(Tip:GetOwner() == nil and Tip:NumLines() == 0)
  Tip:AddLine("Ownerless")
  Tip:Show()
  assert(not Tip:IsShown() and Tip:NumLines() == 0 and CLEARED == 3)
  Tip:SetOwner(Owner)
  Tip:SetText("Replacement")
  Tip:SetOwner(Owner, "ANCHOR_NONE")
  assert(not Tip:IsShown() and Tip:NumLines() == 0 and Tip:GetNumPoints() == 0)
  Tip:SetText("Again")
  assert(not pcall(function() Tip:SetOwner(TipTextLeft1) end))
  assert(Tip:GetOwner() == nil and not Tip:IsShown() and Tip:NumLines() == 0)
  local bare = CreateFrame("GameTooltip", "BareTip", Owner)
  bare:SetOwner(Owner)
  bare:SetText("No registered font strings")
  assert(bare:NumLines() == 0 and not bare:IsShown())
  local l = bare:CreateFontString(nil, "ARTWORK", "TooltipFont")
  local r = bare:CreateFontString(nil, "ARTWORK", "TooltipFont")
  bare:AddFontStrings(l, r)
  bare:SetOwner(Owner)
  bare:SetText("Registered")
  assert(bare:NumLines() == 1 and bare:IsShown() and l:GetText() == "Registered")
  Tip:SetMinimumWidth(120, 0)
  local width, fixed = Tip:GetMinimumWidth()
  assert(width == 120 and fixed == nil)
  Tip:ClearLines()
  assert(Tip:GetMinimumWidth() == 0)
  Tip:SetMinimumWidth(160, 1)
  Tip:ClearLines()
  width, fixed = Tip:GetMinimumWidth()
  assert(width == 160 and fixed == 1)
  Tip:SetPadding(7)
  assert(Tip:GetPadding() == 7)
  local dynamic = CreateFrame("GameTooltip", "InheritedTip", Owner, "TipTemplate")
  dynamic:SetOwner(Owner)
  dynamic:SetText("Template")
  assert(dynamic:NumLines() == 1 and InheritedTipTextLeft1:GetText() == "Template")
  local manual = CreateFrame("GameTooltip", "ManualTip", Owner)
  manual:CreateFontString("$parentTextLeft1", "ARTWORK", "TooltipFont")
  manual:CreateFontString("$parentTextRight1", "ARTWORK", "TooltipFont")
  manual:SetOwner(Owner)
  manual:SetText("Names alone are not XML registration")
  assert(manual:NumLines() == 0)
</OnLoad></Scripts></Frame>
</Ui>"#,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;
    let layout = UiLayoutPlan::from_tree(&tree)?;
    let regions = UiRegionStatePlan::resolve(&tree, &layout)?;
    let frames = UiFramePlan::from_tree(&tree)?.resolve(&tree)?;
    let scripts = UiScriptPlan::from_tree(&tree, bundle.lua())?;
    let templates = UiRuntimeTemplatePlan::from_catalog(&objects, &fonts, bundle.lua())?;
    let textures = UiTexturePlan::from_tree(&tree)?;
    let texture_states = UiTextureStatePlan::resolve(&tree, &textures)?;
    let animations = UiAnimationPlan::from_tree(&tree)?;
    let plan = UiScriptRuntimePlan::new(
        &tree,
        &animations,
        &frames,
        &regions,
        &templates,
        &fonts,
        &texture_states,
    );
    let environment = UiScriptEnvironment::new(1024, 768, false)?.with_asset_store(store);
    let mut runtime = UiScriptRuntime::new(&bundle, &plan, environment)?;
    runtime.execute_all(&bundle, &tree, &scripts)?;
    Ok(())
}
