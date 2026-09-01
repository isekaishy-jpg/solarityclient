//! External stock-compatibility tests for typed XML script handlers.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiAnimationPlan, UiBindingAssignments, UiBindingCatalog, UiBundle, UiFactionGroup,
    UiFramePlan, UiLayoutPlan, UiManifestKind, UiObjectCatalog, UiObjectTree, UiPlayerFactionState,
    UiPlayerProgressionState, UiPlayerState, UiRealmTime, UiRegionStatePlan, UiRuntimeTemplatePlan,
    UiScriptEnvironment, UiScriptError, UiScriptHandler, UiScriptPlan, UiScriptRuntime,
    UiScriptRuntimePlan, UiScriptTarget, UiTexturePlan, UiTextureStatePlan,
};

use crate::support::{Fixture, FixtureFile};

/// Script coordinates retain the stock fixed-height aspect compensation.
#[test]
fn script_environment_derives_stock_ui_extent() -> Result<(), Box<dyn Error>> {
    let standard = UiScriptEnvironment::new(1024, 768, false)?;
    let wide = UiScriptEnvironment::new(1920, 1080, false)?;

    assert_eq!(standard.logical_extent(), (1024, 768));
    assert_eq!(standard.ui_extent(), (1024.0, 768.0));
    assert!((wide.ui_extent().0 - 1_365.333_333_333_333_3).abs() < f64::EPSILON);
    assert_eq!(wide.ui_extent().1, 768.0);
    assert!(matches!(
        UiScriptEnvironment::new(0, 1080, false),
        Err(UiScriptError::Plan { .. })
    ));
    Ok(())
}

/// Later XML layers replace or clear one callback slot without recompiling templates.
#[test]
fn script_plan_applies_stock_handler_replacement() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Scripts.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Scripts.xml",
            bytes: br#"<Ui>
  <Button name="ButtonTemplate" virtual="true"><Scripts>
    <OnLoad>self.loaded = true</OnLoad>
    <OnEvent>local payload = ...</OnEvent>
  </Scripts></Button>
  <Button name="LiveButton" inherits="ButtonTemplate"><Scripts>
    <OnLoad function="NamedLoad"/>
    <OnEvent/>
    <OnUpdate>self.elapsed = elapsed</OnUpdate>
  </Scripts></Button>
</Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;
    let scripts = UiScriptPlan::from_tree(&tree, bundle.lua())?;
    let templates = UiRuntimeTemplatePlan::from_catalog(&objects, &fonts, bundle.lua())?;

    let live_index = tree.node_index("LiveButton").ok_or("missing LiveButton")?;
    let node = scripts.node(live_index).ok_or("missing script node")?;
    let bindings = scripts.bindings_for(node);
    assert_eq!(scripts.declaration_count(), 5);
    assert_eq!(scripts.function_count(), 3);
    assert_eq!(templates.templates().len(), 1);
    assert_eq!(templates.node_count(), 1);
    assert_eq!(bindings.len(), 2);
    assert!(bindings.iter().any(|binding| {
        binding.handler() == UiScriptHandler::Load
            && binding.target() == &UiScriptTarget::Global("NamedLoad".to_owned())
    }));
    assert!(bindings.iter().any(|binding| {
        binding.handler() == UiScriptHandler::Update
            && matches!(binding.target(), UiScriptTarget::Compiled(_))
    }));
    assert!(
        !bindings
            .iter()
            .any(|binding| binding.handler() == UiScriptHandler::Event)
    );
    Ok(())
}

/// A callback unsupported by the concrete stock widget does not become generic.
#[test]
fn script_plan_rejects_handler_from_another_widget() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Scripts.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Scripts.xml",
            bytes: br#"<Ui><Frame name="Root"><Scripts>
  <OnColorSelect>self.changed = true</OnColorSelect>
</Scripts></Frame></Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;

    let result = UiScriptPlan::from_tree(&tree, bundle.lua());

    assert!(matches!(result, Err(UiScriptError::Handler { .. })));
    Ok(())
}

/// A built-in virtual template may await a concrete template from a later AddOn.
#[test]
fn runtime_template_plan_retains_later_addon_dependencies() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\FrameXML\\FrameXML.toc",
            bytes: b"Templates.xml\n",
        },
        FixtureFile {
            path: "Interface\\FrameXML\\Templates.xml",
            bytes: br#"<Ui>
  <Frame name="DeferredAlert" virtual="true"><Frames>
    <Frame name="$parentIcon" inherits="LaterAddonIconTemplate"/>
  </Frames></Frame>
  <Frame name="UIParent"/>
</Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Frame)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;
    let frames = UiFramePlan::from_tree(&tree)?.resolve(&tree)?;
    let layout = UiLayoutPlan::from_tree(&tree)?;
    let regions = UiRegionStatePlan::resolve(&tree, &layout)?;
    let templates = UiRuntimeTemplatePlan::from_catalog(&objects, &fonts, bundle.lua())?;
    let textures = UiTexturePlan::from_tree(&tree)?;
    let texture_states = UiTextureStatePlan::resolve(&tree, &textures)?;

    assert!(templates.templates().is_empty());
    assert_eq!(templates.deferred_templates().len(), 1);
    assert_eq!(templates.deferred_templates()[0].name(), "DeferredAlert");
    assert_eq!(
        templates.deferred_templates()[0].dependency(),
        "LaterAddonIconTemplate"
    );

    let animations = UiAnimationPlan::from_tree(&tree)?;
    let runtime_plan = UiScriptRuntimePlan::new(
        &tree,
        &animations,
        &frames,
        &regions,
        &templates,
        &fonts,
        &texture_states,
    );
    let _runtime = UiScriptRuntime::new(
        &bundle,
        &runtime_plan,
        UiScriptEnvironment::new(1024, 768, false)?,
    )?;
    bundle
        .lua()
        .load(
            r#"local ok, message = pcall(CreateFrame, "Frame", "Alert", UIParent, "DeferredAlert")
DEFERRED_RESULT = ok
DEFERRED_MESSAGE = tostring(message)"#,
        )
        .exec()?;

    assert!(!bundle.lua().globals().get::<bool>("DEFERRED_RESULT")?);
    assert!(
        bundle
            .lua()
            .globals()
            .get::<String>("DEFERRED_MESSAGE")?
            .contains("awaits template LaterAddonIconTemplate")
    );
    Ok(())
}

/// Handler bodies compile inside their exact callback parameter list.
#[test]
fn script_plan_rejects_varargs_in_non_vararg_handler() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Scripts.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Scripts.xml",
            bytes: br#"<Ui><Frame name="Root"><Scripts>
  <OnLoad>local payload = ...</OnLoad>
</Scripts></Frame></Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Glue)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;

    let result = UiScriptPlan::from_tree(&tree, bundle.lua());

    assert!(matches!(result, Err(UiScriptError::Lua { .. })));
    Ok(())
}

/// A virtual root resolves its stock `$parent` anchor against each instance.
#[test]
fn runtime_template_resolves_dynamic_root_parent_anchor() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\FrameXML\\FrameXML.toc",
            bytes: b"Objects.xml\nCheck.lua\n",
        },
        FixtureFile {
            path: "Interface\\FrameXML\\Objects.xml",
            bytes: br#"<Ui>
  <Button name="RosterTemplate" virtual="true"><Anchors>
    <Anchor point="BOTTOM" relativeTo="$parentUpButton" relativePoint="TOP"/>
  </Anchors></Button>
  <Frame name="Panel"><Frames><Button name="$parentUpButton"/></Frames></Frame>
</Ui>"#,
        },
        FixtureFile {
            path: "Interface\\FrameXML\\Check.lua",
            bytes: br#"local roster = CreateFrame("Button", "PanelRoster", Panel, "RosterTemplate")
assert(CreateFrame("FRAME"):GetObjectType() == "Frame")
roster:RegisterEvent("player_login")
assert(roster:IsEventRegistered("PLAYER_LOGIN"))
roster:RegisterEvent("NOT_A_STOCK_FRAME_EVENT")
assert(roster:IsEventRegistered("NOT_A_STOCK_FRAME_EVENT") == nil)
roster:UnregisterEvent("PLAYER_LOGIN")
assert(roster:IsEventRegistered("PLAYER_LOGIN") == nil)
Panel:SetDepth(1.25)
roster:SetDepth(0.75)
assert(roster:GetDepth() == 0.75 and roster:GetEffectiveDepth() == 2.0)
roster:IgnoreDepth(true)
assert(roster:IsIgnoringDepth())
roster:EnableMouse(true)
assert(roster:IsMouseEnabled())
roster:EnableMouse()
assert(roster:IsMouseEnabled() == nil)
local point, relative, relativePoint, x, y = roster:GetPoint(1)
assert(point == "BOTTOM")
assert(relative == PanelUpButton)
assert(relativePoint == "TOP" and x == 0 and y == 0)
DYNAMIC_ANCHOR_TARGET = relative:GetName()"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Frame)?;
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
    let runtime_plan = UiScriptRuntimePlan::new(
        &tree,
        &animations,
        &frames,
        &regions,
        &templates,
        &fonts,
        &texture_states,
    );
    let mut runtime = UiScriptRuntime::new(
        &bundle,
        &runtime_plan,
        UiScriptEnvironment::new(1024, 768, false)?,
    )?;

    runtime.execute_all(&bundle, &tree, &scripts)?;

    assert_eq!(
        bundle
            .lua()
            .globals()
            .get::<String>("DYNAMIC_ANCHOR_TARGET")?,
        "PanelUpButton"
    );
    Ok(())
}

/// XML construction and Lua files share one manifest-ordered execution cursor.
#[test]
fn script_runtime_executes_stock_bootstrap_order() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Objects.xml\nBetween.lua\nLater.xml\nAfter.lua\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Objects.xml",
            bytes: br#"<Ui>
<Button name="DynamicTemplate" virtual="true"><Frames><Button name="$parentLabel"><Scripts><OnLoad>
  LOAD_ORDER = LOAD_ORDER .. self:GetName() .. ";"
  assert(self:GetParent():GetName() == "Dynamic")
</OnLoad></Scripts></Button></Frames><Scripts><OnLoad>
  LOAD_ORDER = LOAD_ORDER .. self:GetName() .. ";"
  assert(self:HasScript("OnUpdate"))
  assert(type(self:GetScript("OnUpdate")) == "function")
</OnLoad><OnUpdate>self.elapsed = elapsed</OnUpdate></Scripts></Button>
<Frame name="First"><Frames>
  <Button name="$parentChild"><Size x="40" y="20"/><Scripts><OnLoad>
    LOAD_ORDER = (LOAD_ORDER or "") .. self:GetName() .. ";"
    assert(self:GetObjectType() == "Button")
    assert(self:IsObjectType("Button"))
    assert(self:IsObjectType("Frame"))
    assert(self:IsObjectType("Slider") == nil)
    assert(self:GetParent() == First)
    assert(self:GetWidth() == 40 and self:GetHeight() == 20)
    self:SetSize(50, 25)
    assert(self:GetWidth() == 50 and self:GetHeight() == 25)
    self:SetPoint("TOPLEFT", First, "BOTTOMLEFT", 2, -3)
    self:Disable()
    assert(self:IsEnabled() == 0)
    self:Enable()
    assert(Later == nil)
  </OnLoad></Scripts></Button>
</Frames><Scripts><OnLoad>
  LOAD_ORDER = LOAD_ORDER .. self:GetName() .. ";"
  assert(GetScreenHeight() == 768)
  assert(bit.tobit(4294967295) == -1)
  assert(bit.bnot(0) == -1)
  assert(bit.band(0xff, 0x0f) == 0x0f)
  assert(bit.bor(0xf0, 0x0f) == 0xff)
  assert(bit.bxor(0xff, 0x0f) == 0xf0)
  assert(bit.lshift(1, 31) == -2147483648)
  assert(bit.rshift(0x80000000, 31) == 1)
  assert(bit.arshift(0x80000000, 31) == -1)
  assert(bit.rol(1, 1) == 2 and bit.ror(2, 1) == 1)
  local lagTypes = {}
  RegisterStaticConstants(lagTypes)
  assert(lagTypes.Loot == 1 and lagTypes.AuctionHouse == 2)
  assert(lagTypes.Mail == 3 and lagTypes.Chat == 4)
  assert(lagTypes.Movement == 5 and lagTypes.Spell == 6)
  local qualityRed, qualityGreen, qualityBlue, qualityHex = GetItemQualityColor(4)
  assert(qualityRed == 0.64 and qualityGreen == 0.21 and qualityBlue == 0.93)
  assert(qualityHex == "ffa335ee")
  assert(({GetItemQualityColor(-1)})[4] == "ffffffff")
  assert(issecure())
  local secureValue, secureExtra = securecall(function(value) return value, "secure", 4 end, 3)
  assert(secureValue == 3 and secureExtra == "secure")
  assert(abs(GetScreenWidth() - 1365.3333333333) &lt; 0.001)
  assert(GetNumCharacters() == 0)
  assert(GetSavedAccountName() == "")
  SetSavedAccountName("tester")
  assert(GetSavedAccountName() == "tester")
  SetSavedAccountName("")
  assert(GetSavedAccountList() == "")
  assert(GetCVar("showToolsUI") == "-1")
  assert(GetCVarDefault("showToolsUI") == "-1")
  assert(GetCVarDefault("gxVSync") == "1")
  assert(GetCVarMin("farclip") == 177 and GetCVarMax("farclip") == 1277)
  assert(GetCVarBool("gxVSync"))
  assert(GetCVar("rotateMinimap") == "0")
  assert(GetCVarDefault("cameraSmoothStyle") == "4")
  assert(GetCVar("conversationMode") == "popout")
  assert(GetCVarMin("rotateMinimap") == 0 and GetCVarMax("rotateMinimap") == 1)
  assert(GetTerrainMip() == 1)
  SetTerrainMip(0)
  assert(GetCVar("shadowLevel") == "1")
  assert(GetGamma() == 0)
  SetGamma(0.25)
  assert(GetCVar("gamma") == "1.25")
  local anisotropic, _, _, _, _, maximumAnisotropy = GetVideoCaps()
  assert(anisotropic == false and maximumAnisotropy == 1)
  SetCVar("showToolsUI", 1)
  assert(GetCVar("SHOWTOOLSUI") == "1")
  assert(not pcall(function() GetCVar("notRegistered") end))
  assert(format("%s:%d", "screen", 7) == "screen:7")
  local values = { retained = true }
  assert(wipe(values) == values and next(values) == nil)
  local handler = function() end
  seterrorhandler(handler)
  assert(geterrorhandler() == handler)
  self:RegisterEvent("set_glue_screen")
  assert(self:IsEventRegistered("SET_GLUE_SCREEN"))
  self:RegisterEvent("NOT_A_STOCK_EVENT")
  assert(self:IsEventRegistered("NOT_A_STOCK_EVENT") == nil)
  assert(self:HasScript("OnLoad"))
  assert(type(self:GetScript("OnLoad")) == "function")
  local show = function(frame) frame.wasShown = true end
  self:SetScript("OnShow", show)
  assert(self:HasScript("OnShow") and self:GetScript("OnShow") == show)
  self:SetScript("OnShow", nil)
  assert(not self:HasScript("OnShow") and self:GetScript("OnShow") == nil)
  assert(not pcall(function() self:SetScript("NotAHandler", show) end))
  assert(not pcall(function() self:SetScript("OnShow", "not a function") end))
</OnLoad></Scripts></Frame></Ui>"#,
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Between.lua",
            bytes: br#"assert(First ~= nil)
assert(FirstChild ~= nil)
assert(Later == nil)
BETWEEN = First:GetName()
Later = "reserved"
local dynamic = CreateFrame("Button", "Dynamic", First, "DynamicTemplate")
assert(dynamic:GetParent() == First)
assert(DynamicLabel:GetParent() == dynamic)
function NamedLoad(self)
  LOAD_ORDER = LOAD_ORDER .. self:GetName() .. ";"
  assert(self:GetObjectType() == "CheckButton")
  assert(self:IsObjectType("Button"))
end"#,
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Later.xml",
            bytes: br#"<Ui><CheckButton name="Later"><Scripts>
  <OnLoad function="NamedLoad"/>
</Scripts></CheckButton></Ui>"#,
        },
        FixtureFile {
            path: "Interface\\GlueXML\\After.lua",
            bytes: br#"assert(Later == "reserved")
RESULT = BETWEEN .. ":" .. LOAD_ORDER"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
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
    let environment = UiScriptEnvironment::new(1920, 1080, false)?;
    let animations = UiAnimationPlan::from_tree(&tree)?;
    let runtime_plan = UiScriptRuntimePlan::new(
        &tree,
        &animations,
        &frames,
        &regions,
        &templates,
        &fonts,
        &texture_states,
    );
    let mut runtime = UiScriptRuntime::new(&bundle, &runtime_plan, environment)?;

    assert!(
        bundle
            .lua()
            .globals()
            .get::<Option<mlua::Table>>("First")?
            .is_none()
    );
    assert!(runtime.execute_next(&bundle, &tree, &scripts)?);
    assert_eq!(runtime.next_action(), 1);
    assert_eq!(runtime.registered_object_count(), 0);
    assert!(runtime.execute_next(&bundle, &tree, &scripts)?);
    assert_eq!(runtime.next_action(), 2);
    assert_eq!(runtime.registered_object_count(), 2);
    assert_eq!(runtime.executed_load_handler_count(), 2);
    assert!(
        bundle
            .lua()
            .globals()
            .get::<Option<mlua::Table>>("First")?
            .is_some()
    );
    assert!(
        bundle
            .lua()
            .globals()
            .get::<Option<mlua::Table>>("Later")?
            .is_none()
    );

    runtime.execute_all(&bundle, &tree, &scripts)?;

    assert_eq!(runtime.next_action(), bundle.actions().len());
    assert_eq!(runtime.registered_object_count(), 5);
    assert_eq!(runtime.executed_chunk_count(), 2);
    assert_eq!(runtime.executed_load_handler_count(), 3);
    assert_eq!(
        bundle.lua().globals().get::<String>("RESULT")?,
        "First:FirstChild;First;DynamicLabel;Dynamic;Later;"
    );
    Ok(())
}

/// Global font objects appear at their XML action and remain typed button state.
#[test]
fn script_runtime_registers_ordered_font_objects() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\FrameXML\\Bindings.xml",
            bytes: br#"<Bindings><Binding name="MOVEFORWARD">MoveForwardStart()</Binding><ModifiedClick action="SELFCAST" default="ALT"/></Bindings>"#,
        },
        FixtureFile {
            path: "WTF\\DefaultBindings.wtf",
            bytes: b"bind W MOVEFORWARD\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Fonts.xml\nButton.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Fonts.xml",
            bytes: br#"<Ui><Font name="GlueFontTest" font="Fonts\FRIZQT__.TTF" justifyH="RIGHT" justifyV="BOTTOM">
  <FontHeight><AbsValue val="12"/></FontHeight>
</Font></Ui>"#,
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Button.xml",
            bytes: br#"<Ui>
<FontString name="FontLabel" inherits="GlueFontTest"/>
<Texture name="CoordinateTexture"><TexCoords left="0.25" right="0.75" top="0.5" bottom="0.875"/></Texture>
<Texture name="GradientTexture"><Gradient orientation="VERTICAL"><MinColor r="0.1" g="0.2" b="0.3" a="0.4"/><MaxColor r="0.6" g="0.7" b="0.8" a="0.9"/></Gradient></Texture>
<Frame name="OwnedTemplate" virtual="true"><Frames><Frame name="$parentOwned" parentKey="owned"/></Frames></Frame>
<Button name="FontButton"><Frames><Frame name="$parentOwned" parentKey="owned"/></Frames><Scripts><OnLoad>
  assert(self.owned == FontButtonOwned)
  local dynamicOwned = CreateFrame("Frame", "DynamicOwned", self, "OwnedTemplate")
  assert(dynamicOwned.owned == DynamicOwnedOwned)
  local reparented = CreateFrame("Frame", "Reparented", self)
  local newParent = CreateFrame("Frame", "NewParent", self)
  reparented:SetParent(newParent)
  assert(reparented:GetParent() == newParent)
  assert(not pcall(function() newParent:SetParent(reparented) end))
  reparented:SetParent(nil)
  assert(reparented:GetParent() == nil)
  assert(GetModifiedClick("SELFCAST") == "ALT")
  assert(not BNFeaturesEnabled() and not BNConnected() and not BNFeaturesEnabledAndConnected())
  FontLabel:SetText("Label")
  assert(FontLabel:GetText() == "Label")
  FontLabel:SetTextColor(0.25, 0.5, 0.75, 0.8)
  local tr, tg, tb, ta = FontLabel:GetTextColor()
  assert(tr == 0.25 and tg == 0.5 and tb == 0.75 and ta == 0.8)
  assert(FontLabel:GetFontObject() == GlueFontTest)
  local face, height, flags = FontLabel:GetFont()
  assert(face == "FONTS\\FRIZQT__.TTF" and height == 12 and flags == "")
  assert(FontLabel:GetJustifyH() == "RIGHT")
  assert(FontLabel:GetJustifyV() == "BOTTOM")
  FontLabel:SetJustifyH("left")
  FontLabel:SetJustifyV("middle")
  assert(FontLabel:GetJustifyH() == "LEFT")
  assert(FontLabel:GetJustifyV() == "MIDDLE")
  local ulX, ulY, llX, llY, urX, urY, lrX, lrY = CoordinateTexture:GetTexCoord()
  assert(ulX == 0.25 and ulY == 0.5 and llX == 0.25 and llY == 0.875)
  assert(urX == 0.75 and urY == 0.5 and lrX == 0.75 and lrY == 0.875)
  CoordinateTexture:SetTexCoord(0.125, 0.625, 0, 1)
  ulX, ulY, llX, llY, urX, urY, lrX, lrY = CoordinateTexture:GetTexCoord()
  assert(ulX == 0.125 and ulY == 0 and llX == 0.125 and llY == 1)
  assert(urX == 0.625 and urY == 0 and lrX == 0.625 and lrY == 1)
  local r, g, b, a = GradientTexture:GetVertexColor()
  assert(math.abs(r - 0.6) &lt; 0.00001 and math.abs(g - 0.7) &lt; 0.00001)
  assert(math.abs(b - 0.8) &lt; 0.00001 and math.abs(a - 0.9) &lt; 0.00001)
  GradientTexture:SetVertexColor(0.25, 0.5, 0.75, 1)
  r, g, b, a = GradientTexture:GetVertexColor()
  assert(r == 0.25 and g == 0.5 and b == 0.75 and a == 1)
  assert(GradientTexture:GetTexture() == nil)
  GradientTexture:SetTexture(0.2, 0.4, 0.6, 0.8)
  assert(GradientTexture:GetTexture() == nil)
  GradientTexture:SetTexture("Interface\\Glues\\TestTexture.tga")
  assert(GradientTexture:GetTexture() == "Interface\\Glues\\TestTexture.tga")
  GradientTexture:SetBlendMode("add")
  assert(GradientTexture:GetBlendMode() == "ADD")
  GradientTexture:SetDrawLayer("overlay", -2)
  assert(GradientTexture:GetDrawLayer() == "OVERLAY")
  GradientTexture:SetHorizTile(true)
  GradientTexture:SetVertTile(true)
  GradientTexture:SetNonBlocking(true)
  assert(not GradientTexture:IsDesaturated())
  GradientTexture:SetDesaturated(true)
  assert(GradientTexture:GetHorizTile() == 1)
  assert(GradientTexture:GetVertTile() == 1)
  assert(GradientTexture:GetNonBlocking() == 1)
  assert(GradientTexture:IsDesaturated() == 1)
  GradientTexture:SetGradientAlpha("HORIZONTAL", 0.1, 0.2, 0.3, 0.4, 0.6, 0.7, 0.8, 0.9)
  r, g, b, a = GradientTexture:GetVertexColor()
  assert(math.abs(r - 0.1) &lt; 0.00001 and math.abs(a - 0.4) &lt; 0.00001)
  assert(GlueFontTest:GetName() == "GlueFontTest")
  assert(GlueFontTest:GetObjectType() == "Font")
  assert(GlueFontTest:IsObjectType("Font"))
  self:SetNormalFontObject(GlueFontTest)
  self:SetDisabledFontObject("GlueFontTest")
  self:SetHighlightFontObject(GlueFontTest)
  assert(self:GetNormalFontObject() == GlueFontTest)
  assert(self:GetDisabledFontObject() == GlueFontTest)
  assert(self:GetHighlightFontObject() == GlueFontTest)
  self:SetText("Player")
  assert(self:GetText() == "Player")
  self:SetFormattedText("%s %d", "Player", 2)
  assert(self:GetText() == "Player 2")
  self:SetText("")
  assert(self:GetText() == nil)
  self:LockHighlight()
  self:UnlockHighlight()
  self:RegisterForClicks("LeftButtonDown", "LeftButtonUp")
  local childLabel = self:CreateFontString("$parentDynamicLabel", "OVERLAY", "GlueFontTest", 3)
  assert(childLabel:GetName() == "FontButtonDynamicLabel")
  assert(childLabel:GetParent() == self and childLabel:GetFontObject() == GlueFontTest)
  assert(childLabel:GetDrawLayer() == "OVERLAY")
  self:SetFontString(childLabel)
  assert(self:GetFontString() == childLabel)
  local childTexture = self:CreateTexture("$parentDynamicTexture", "BACKGROUND")
  assert(childTexture:GetName() == "FontButtonDynamicTexture")
  assert(childTexture:GetParent() == self and childTexture:GetDrawLayer() == "BACKGROUND")
  childTexture:SetParent(newParent)
  assert(childTexture:GetParent() == newParent)
  assert(self:GetNormalTexture() == nil)
  self:SetNormalTexture("Interface\\Buttons\\UI-DialogBox-Button-Up")
  local normalTexture = self:GetNormalTexture()
  assert(normalTexture:GetParent() == self)
  assert(normalTexture:GetTexture() == "Interface\\Buttons\\UI-DialogBox-Button-Up")
  self:SetPushedTexture(childTexture)
  assert(self:GetPushedTexture() == childTexture)
  self:SetDisabledTexture(nil)
  assert(self:GetDisabledTexture() == nil)
  assert(not pcall(function() self:SetHighlightTexture(self) end))
  self:SetAttribute("plain", 17)
  assert(self:GetAttribute("plain") == 17)
  assert(ATTRIBUTE_CHANGED_NAME == "plain" and ATTRIBUTE_CHANGED_VALUE == 17)
  self:SetAttribute("*type1", "wild-suffix")
  self:SetAttribute("shift-type*", "prefix-wild")
  assert(self:GetAttribute("shift-", "type", "1") == "wild-suffix")
  self:SetAttribute("shift-type1", "exact")
  assert(self:GetAttribute("shift-", "type", "1") == "exact")
  local attributeTable = { marker = 9 }
  self:SetAttribute("table", attributeTable)
  assert(self:GetAttribute("table") == attributeTable)
  self:SetAttribute("plain", nil)
  assert(self:GetAttribute("plain") == nil)
  local bare = CreateFrame("FontString", "BareLabel", self)
  assert(bare:GetJustifyH() == "CENTER" and bare:GetJustifyV() == "MIDDLE")
  assert(not pcall(function() bare:SetText("invalid") end))
  local check = CreateFrame("CheckButton", "DynamicCheck", self)
  assert(check:GetChecked() == nil)
  check:SetChecked(1)
  assert(check:GetChecked() == 1)
  check:SetChecked(0)
  assert(check:GetChecked() == nil)
  local status = CreateFrame("StatusBar", "DynamicStatus", self)
  status:SetMinMaxValues(0, 100)
  status:SetValue(125)
  assert(status:GetValue() == 100)
  local minimum, maximum = status:GetMinMaxValues()
  assert(minimum == 0 and maximum == 100)
  status:SetStatusBarColor(0.25, 0.5, 0.75, 0.8)
  local sr, sg, sb, sa = status:GetStatusBarColor()
  assert(sr == 0.25 and sg == 0.5 and sb == 0.75 and sa == 0.8)
  status:SetStatusBarTexture("Interface\\TargetingFrame\\UI-StatusBar")
  assert(status:GetStatusBarTexture():GetTexture() == "Interface\\TargetingFrame\\UI-StatusBar")
  local tooltip = CreateFrame("GameTooltip", "DynamicTooltip", self)
  assert(tooltip:GetOwner() == nil and not tooltip:IsOwned(self))
  tooltip:SetOwner(self, "ANCHOR_RIGHT", 3, -4)
  assert(tooltip:GetOwner() == self and tooltip:IsOwned(self) == 1)
  tooltip:Hide()
  assert(tooltip:GetOwner() == nil and not tooltip:IsOwned(self))
  assert(not pcall(function() tooltip:SetOwner(self, "INVALID_ANCHOR") end))
  local model = CreateFrame("ModelFFX", "DynamicModel", self)
  model:SetCamera(0)
  model:SetSequence(505)
  model:SetSequenceTime(0, 250)
  model:SetModelScale(1.25)
  assert(not pcall(function() model:SetSequence(506) end))
</OnLoad>
<OnAttributeChanged>
  ATTRIBUTE_CHANGED_NAME = name
  ATTRIBUTE_CHANGED_VALUE = value
</OnAttributeChanged></Scripts></Button>
</Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let binding_catalog = UiBindingCatalog::load_builtin(&mut store)?;
    let bindings = UiBindingAssignments::load_defaults(&mut store, &binding_catalog)?;
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
    let environment =
        UiScriptEnvironment::new(1920, 1080, false)?.with_binding_assignments(bindings);
    let animations = UiAnimationPlan::from_tree(&tree)?;
    let runtime_plan = UiScriptRuntimePlan::new(
        &tree,
        &animations,
        &frames,
        &regions,
        &templates,
        &fonts,
        &texture_states,
    );
    let mut runtime = UiScriptRuntime::new(&bundle, &runtime_plan, environment)?;

    assert!(
        bundle
            .lua()
            .globals()
            .get::<Option<mlua::Table>>("GlueFontTest")?
            .is_none()
    );
    assert!(runtime.execute_next(&bundle, &tree, &scripts)?);
    assert!(
        bundle
            .lua()
            .globals()
            .get::<Option<mlua::Table>>("GlueFontTest")?
            .is_some()
    );
    runtime.execute_all(&bundle, &tree, &scripts)?;

    assert_eq!(runtime.executed_load_handler_count(), 1);
    Ok(())
}

/// A missing API remains an execution error at its manifest action.
#[test]
fn script_runtime_does_not_advance_past_execution_error() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Missing.lua\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Missing.lua",
            bytes: b"MissingStockApi()\n",
        },
    ])?;
    let mut store = mount(&fixture)?;
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
    let environment = UiScriptEnvironment::new(1920, 1080, false)?;
    let animations = UiAnimationPlan::from_tree(&tree)?;
    let runtime_plan = UiScriptRuntimePlan::new(
        &tree,
        &animations,
        &frames,
        &regions,
        &templates,
        &fonts,
        &texture_states,
    );
    let mut runtime = UiScriptRuntime::new(&bundle, &runtime_plan, environment)?;

    let result = runtime.execute_next(&bundle, &tree, &scripts);

    assert!(matches!(result, Err(UiScriptError::Execution { .. })));
    assert_eq!(runtime.next_action(), 0);
    assert_eq!(runtime.executed_chunk_count(), 0);
    Ok(())
}

/// Player globals observe live authoritative state without offline fallbacks.
#[test]
fn frame_runtime_reads_live_player_state() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\FrameXML\\FrameXML.toc",
            bytes: b"Money.xml\n",
        },
        FixtureFile {
            path: "Interface\\FrameXML\\Money.xml",
            bytes: br#"<Ui><Frame name="MoneyProbe"><Scripts>
  <OnLoad>
    INITIAL_MONEY = GetMoney()
    INITIAL_XP = UnitXP("player")
    INITIAL_XP_MAX = UnitXPMax("player")
    INITIAL_FACTION, INITIAL_FACTION_NAME = UnitFactionGroup("player")
    INITIAL_REALM_HOUR, INITIAL_REALM_MINUTE = GetGameTime()
  </OnLoad>
</Scripts></Frame></Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Frame)?;
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
    let environment = UiScriptEnvironment::new(1920, 1080, false)?;
    let world = environment.world_state();
    let action_bar = environment.action_bar_state();
    world.enter_player(UiPlayerState::new(12_345_678));
    world.set_player_progression(UiPlayerProgressionState::new(123_456, 1_000_000));
    world.set_player_faction(UiPlayerFactionState::new(UiFactionGroup::Horde, "Horde"));
    world.set_realm_time(UiRealmTime::new(21, 37)?);
    let animations = UiAnimationPlan::from_tree(&tree)?;
    let runtime_plan = UiScriptRuntimePlan::new(
        &tree,
        &animations,
        &frames,
        &regions,
        &templates,
        &fonts,
        &texture_states,
    );
    let mut runtime = UiScriptRuntime::new(&bundle, &runtime_plan, environment)?;

    runtime.execute_all(&bundle, &tree, &scripts)?;
    assert_eq!(
        bundle.lua().globals().get::<f64>("INITIAL_MONEY")?,
        12_345_678.0
    );
    assert_eq!(bundle.lua().globals().get::<f64>("INITIAL_XP")?, 123_456.0);
    assert_eq!(
        bundle.lua().globals().get::<f64>("INITIAL_XP_MAX")?,
        1_000_000.0
    );
    assert_eq!(
        bundle.lua().globals().get::<String>("INITIAL_FACTION")?,
        "Horde"
    );
    assert_eq!(
        bundle
            .lua()
            .globals()
            .get::<String>("INITIAL_FACTION_NAME")?,
        "Horde"
    );
    assert_eq!(
        (
            bundle.lua().globals().get::<u8>("INITIAL_REALM_HOUR")?,
            bundle.lua().globals().get::<u8>("INITIAL_REALM_MINUTE")?
        ),
        (21, 37)
    );
    assert_eq!(
        bundle
            .lua()
            .load("return select('#', UnitFactionGroup('target'))")
            .eval::<u8>()?,
        0
    );
    assert_eq!(
        bundle
            .lua()
            .load("return GetActionBarPage()")
            .eval::<u8>()?,
        1
    );
    bundle.lua().load("ChangeActionBarPage(6)").exec()?;
    assert_eq!(action_bar.page(), 6);
    let rejected = bundle
        .lua()
        .load("return pcall(ChangeActionBarPage, 7)")
        .eval::<bool>()?;
    assert!(!rejected);

    world.enter_player(UiPlayerState::new(u32::MAX));
    world.set_player_progression(UiPlayerProgressionState::new(u32::MAX, 0));
    world.set_cursor_money_copper(234);
    world.set_player_trade_money_copper(567);
    world.set_realm_time(UiRealmTime::new(3, 5)?);
    assert_eq!(
        bundle.lua().load("return GetMoney()").eval::<f64>()?,
        f64::from(u32::MAX)
    );
    assert_eq!(
        bundle
            .lua()
            .load("return GetCursorMoney(), GetPlayerTradeMoney()")
            .eval::<(f64, f64)>()?,
        (234.0, 567.0)
    );
    assert_eq!(
        bundle
            .lua()
            .load("return GetGameTime()")
            .eval::<(u8, u8)>()?,
        (3, 5)
    );
    assert_eq!(
        bundle
            .lua()
            .load("return UnitXP('player'), UnitXPMax('player'), UnitXP('target')")
            .eval::<(f64, f64, f64)>()?,
        (f64::from(u32::MAX), 0.0, 0.0)
    );

    world.leave_world();
    let (available, message) = bundle
        .lua()
        .load("local ok, value = pcall(GetMoney); return ok, tostring(value)")
        .eval::<(bool, String)>()?;
    assert!(!available);
    assert!(message.contains("authoritative active-player state"));
    let (available, message) = bundle
        .lua()
        .load("local ok, value = pcall(UnitXP, 'player'); return ok, tostring(value)")
        .eval::<(bool, String)>()?;
    assert!(!available);
    assert!(message.contains("authoritative local-player progression"));
    let (available, message) = bundle
        .lua()
        .load("local ok, value = pcall(GetGameTime); return ok, tostring(value)")
        .eval::<(bool, String)>()?;
    assert!(!available);
    assert!(message.contains("authoritative realm time"));
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
