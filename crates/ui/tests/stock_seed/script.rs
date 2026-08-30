//! External stock-compatibility tests for typed XML script handlers.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiBundle, UiFramePlan, UiLayoutPlan, UiManifestKind, UiObjectCatalog,
    UiObjectTree, UiRegionStatePlan, UiRuntimeTemplatePlan, UiScriptEnvironment, UiScriptError,
    UiScriptHandler, UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan, UiScriptTarget,
    UiTexturePlan, UiTextureStatePlan,
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
    let runtime_plan = UiScriptRuntimePlan::new(
        &tree,
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
<Button name="FontButton"><Scripts><OnLoad>
  FontLabel:SetText("Label")
  assert(FontLabel:GetText() == "Label")
  assert(FontLabel:GetFontObject() == GlueFontTest)
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
  local bare = CreateFrame("FontString", "BareLabel", self)
  assert(bare:GetJustifyH() == "CENTER" and bare:GetJustifyV() == "MIDDLE")
  assert(not pcall(function() bare:SetText("invalid") end))
  local check = CreateFrame("CheckButton", "DynamicCheck", self)
  assert(check:GetChecked() == nil)
  check:SetChecked(1)
  assert(check:GetChecked() == 1)
  check:SetChecked(0)
  assert(check:GetChecked() == nil)
  local model = CreateFrame("ModelFFX", "DynamicModel", self)
  model:SetCamera(0)
  model:SetSequence(505)
  model:SetSequenceTime(0, 250)
  model:SetModelScale(1.25)
  assert(not pcall(function() model:SetSequence(506) end))
</OnLoad></Scripts></Button>
</Ui>"#,
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
    let runtime_plan = UiScriptRuntimePlan::new(
        &tree,
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
    let runtime_plan = UiScriptRuntimePlan::new(
        &tree,
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

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
