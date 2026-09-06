//! External stock-compatibility tests for deterministic UI presentation.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_rendering::{
    UiRenderBlend, UiRenderSource, UiRenderTransform, UiTextureAddressMode, UiTextureResidency,
};
use solarity_ui::{GlueManager, UiBlendMode, UiFrameStrata, UiPointerButton, UiTextureSource};

use crate::support::{Fixture, FixtureFile};

#[test]
fn ui_mask_requests_deduplicate_and_promote_shared_residency() -> Result<(), Box<dyn Error>> {
    use solarity_asset::AssetPath;
    use solarity_rendering::{UiMeshPlan, UiRenderMask, UiRenderQuad};
    use solarity_ui::UiTextureAssetPlan;
    let tile = AssetPath::new("Tile.blp")?;
    let mask = AssetPath::new("Mask.blp")?;
    let quad = |source, residency| {
        UiRenderQuad::new(
            0,
            UiRenderSource::Texture(source),
            UiRenderBlend::Alpha,
            UiTextureAddressMode::Clamp,
            UiTextureAddressMode::Clamp,
            residency,
            false,
            [0.0, 0.0, 32.0, 32.0],
            [[0.0; 2]; 4],
            [[1.0; 4]; 4],
        )
    };
    let plan = UiMeshPlan::prepare(
        [64.0; 2],
        [
            quad(tile.clone(), UiTextureResidency::NonBlocking)
                .with_mask(UiRenderMask::new(mask.clone(), [0.0, 0.0, 64.0, 64.0])),
            quad(mask.clone(), UiTextureResidency::Blocking),
        ]
        .into_iter(),
    )?;
    let requests = UiTextureAssetPlan::prepare(&plan)?;
    assert_eq!(requests.requests().len(), 2);
    assert_eq!(
        requests.request_for_batch(0).map(|request| request.path()),
        Some(&tile)
    );
    let shared_mask = requests
        .mask_request_for_batch(0)
        .ok_or("missing mask request")?;
    assert_eq!(shared_mask.path(), &mask);
    assert_eq!(shared_mask.residency(), UiTextureResidency::Blocking);
    assert_eq!(Some(shared_mask), requests.request_for_batch(1));
    assert!(requests.mask_request_for_batch(1).is_none());
    Ok(())
}

/// Portrait requests survive presentation and are replaced through ordinary texture APIs.
#[test]
fn portrait_requests_follow_lua_source_changes_and_missing_units() -> Result<(), Box<dyn Error>> {
    use solarity_asset::AssetStoreHandle;
    use solarity_ui::{AddonCatalog, FrameManager, UiPlayerState, UiScriptEnvironment};
    let table = |records: u32, fields: u32| {
        let mut bytes = b"WDBC".to_vec();
        for value in [records, fields, fields * 4, 1] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.resize(21 + records as usize * fields as usize * 4, 0);
        bytes
    };
    let slots = table(0, 3);
    let crit_base = table(11, 1);
    let coefficients = table(1100, 1);
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient/gtChanceToMeleeCritBase.dbc",
            bytes: &crit_base,
        },
        FixtureFile {
            path: "DBFilesClient/gtChanceToSpellCritBase.dbc",
            bytes: &crit_base,
        },
        FixtureFile {
            path: "DBFilesClient/gtChanceToMeleeCrit.dbc",
            bytes: &coefficients,
        },
        FixtureFile {
            path: "DBFilesClient/gtChanceToSpellCrit.dbc",
            bytes: &coefficients,
        },
        FixtureFile {
            path: "DBFilesClient/gtOCTRegenHP.dbc",
            bytes: &coefficients,
        },
        FixtureFile {
            path: "DBFilesClient/gtRegenHPPerSpt.dbc",
            bytes: &coefficients,
        },
        FixtureFile {
            path: "DBFilesClient/gtRegenMPPerSpt.dbc",
            bytes: &coefficients,
        },
        FixtureFile {
            path: "DBFilesClient/PaperDollItemFrame.dbc",
            bytes: &slots,
        },
        FixtureFile {
            path: "Interface\\FrameXML\\FrameXML.toc",
            bytes: b"Portrait.xml\n",
        },
        FixtureFile {
            path: "Interface\\FrameXML\\Portrait.xml",
            bytes: br#"<Ui>
<Frame name="Owner"><Size x="64" y="64"/><Anchors><Anchor point="CENTER"/></Anchors>
<Layers><Layer><Texture name="Portrait" setAllPoints="true"/></Layer></Layers>
<Scripts><OnLoad>assert(SetPortraitTexture(Portrait, "PLAYER") == true)</OnLoad></Scripts>
</Frame></Ui>"#,
        },
        FixtureFile {
            path: "Interface\\FrameXML\\Bindings.xml",
            bytes: br#"<Bindings>
<Binding name="SOLID">Portrait:SetTexture(0.25, 0.5, 1, 0.75)</Binding>
<Binding name="FILE">Portrait:SetTexture("Interface\\Icons\\Test")</Binding>
<Binding name="PORTRAIT">assert(SetPortraitTexture(Portrait, "player") == true)</Binding>
<Binding name="MISSING">assert(SetPortraitTexture(Portrait, "missing") == false)</Binding>
</Bindings>"#,
        },
        FixtureFile {
            path: "WTF\\DefaultBindings.wtf",
            bytes: b"",
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let environment = UiScriptEnvironment::new(800, 600, false)?;
    environment
        .world_state()
        .enter_player(UiPlayerState::new(0));
    let mut manager = FrameManager::start_shared(
        AssetStoreHandle::new(AssetStore::mount(catalog)?),
        environment,
        &[],
        &AddonCatalog::default(),
    )?;
    let source = |manager: &FrameManager| {
        manager
            .render_plan()
            .mesh()
            .batches()
            .first()
            .map(|batch| batch.source().clone())
    };
    assert_eq!(
        source(&manager),
        Some(UiRenderSource::UnitPortrait("player".to_owned()))
    );
    assert!(manager.render_plan().texture_assets().requests().is_empty());
    manager.invoke_binding("SOLID", true)?;
    assert_eq!(source(&manager), Some(UiRenderSource::VertexColor));
    manager.invoke_binding("PORTRAIT", true)?;
    assert_eq!(
        source(&manager),
        Some(UiRenderSource::UnitPortrait("player".to_owned()))
    );
    manager.invoke_binding("FILE", true)?;
    assert!(
        matches!(source(&manager), Some(UiRenderSource::Texture(path)) if path.as_str() == "INTERFACE\\ICONS\\TEST.BLP")
    );
    manager.invoke_binding("MISSING", true)?;
    assert_eq!(source(&manager), None);
    manager.invoke_binding("PORTRAIT", true)?;
    assert_eq!(
        source(&manager),
        Some(UiRenderSource::UnitPortrait("player".to_owned()))
    );
    Ok(())
}

/// File-only XML textures fill their parent until Lua changes their anchors.
#[test]
fn xml_texture_default_parent_anchors_survive_templates_and_clear_in_lua()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile { path: "Interface\\GlueXML\\GlueXML.toc", bytes: b"Anchors.xml\n" },
        FixtureFile { path: "Interface\\GlueXML\\Anchors.xml", bytes: br#"<Ui>
<Texture name="SizedTemplate" virtual="true"><Size x="17" y="19"/></Texture>
<Texture name="AnchoredTemplate" virtual="true"><Size x="23" y="29"/><Anchors><Anchor point="CENTER"/></Anchors></Texture>
<Frame name="DefaultTemplate" virtual="true"><Size x="90" y="50"/>
  <Layers><Layer><Texture name="$parentTexture" file="Interface\Minimap\UI-Minimap-Border"/></Layer></Layers>
</Frame>
<Frame name="Owner"><Size x="192" y="192"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Layers><Layer>
    <Texture name="Border" file="Interface\Minimap\UI-Minimap-Border"/>
    <Texture name="Sized" inherits="SizedTemplate"><Color r="1" g="0" b="0"/></Texture>
    <Texture name="Explicit" inherits="AnchoredTemplate"><Color r="0" g="1" b="0"/></Texture>
    <Texture name="Cleared"><Color r="0" g="0" b="1"/></Texture>
    <Texture name="Reanchored"/>
  </Layer></Layers>
  <Scripts><OnLoad>
    assert(Border:GetNumPoints() == 2 and Sized:GetNumPoints() == 2)
    local p, relative, rp, x, y = Border:GetPoint(1)
    assert(p == "TOPLEFT" and relative == self and rp == p and x == 0 and y == 0)
    assert(Border:GetPoint(2) == "BOTTOMRIGHT")
    assert(Explicit:GetNumPoints() == 1 and Explicit:GetPoint() == "CENTER")
    Cleared:ClearAllPoints()
    assert(Cleared:GetNumPoints() == 0)
    Reanchored:SetPoint("TOPLEFT", self, "TOPLEFT", 5, -7)
    assert(Reanchored:GetNumPoints() == 2) -- the other initial anchor remains
    local t = self:CreateTexture("LuaTexture")
    assert(t:GetNumPoints() == 0)
    local f = CreateFrame("Frame", "Dynamic", self, "DefaultTemplate")
    f:SetPoint("CENTER")
    assert(DynamicTexture:GetNumPoints() == 2)
    local _, parent = DynamicTexture:GetPoint()
    assert(parent == f)
    local cleared = CreateFrame("Frame", "DynamicCleared", self, "DefaultTemplate")
    DynamicClearedTexture:ClearAllPoints()
    assert(DynamicClearedTexture:GetNumPoints() == 0)
  </OnLoad></Scripts>
</Frame></Ui>"# },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (800, 600), false)?;
    let bounds = |name| {
        let index = manager
            .objects()
            .iter()
            .position(|object| object.name() == Some(name))
            .ok_or("missing object")?;
        manager
            .geometry()
            .region(index)
            .map(|region| region.logical_bounds())
            .ok_or("missing geometry")
    };
    assert_eq!(bounds("Border")?, bounds("Owner")?);
    assert_eq!(bounds("Sized")?, bounds("Owner")?);
    assert_eq!(bounds("DynamicTexture")?, bounds("Dynamic")?);
    assert_eq!(bounds("Explicit")?.width(), 23.0);
    assert_eq!(bounds("Explicit")?.height(), 29.0);
    for name in ["Cleared", "DynamicClearedTexture", "LuaTexture"] {
        assert_eq!(bounds(name)?.width(), 0.0);
        assert_eq!(bounds(name)?.height(), 0.0);
    }
    assert_eq!(bounds("Reanchored")?.width(), 187.0);
    assert_eq!(bounds("Reanchored")?.height(), 185.0);
    Ok(())
}

/// XML color sources and file tints survive inheritance and dynamic templates.
#[test]
fn xml_color_sources_follow_stock_file_precedence_and_dynamic_templates()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile { path: "Interface\\GlueXML\\GlueXML.toc", bytes: b"Colors.xml\n" },
        FixtureFile { path: "Interface\\GlueXML\\Colors.xml", bytes: br#"<Ui>
<Texture name="TintedAsset" virtual="true" file="Interface\Glues\Base"><Color r="0.5" g="0.5" b="0.5" a="1"/></Texture>
<Frame name="ColorTemplate" virtual="true"><Size x="80" y="40"/>
  <Layers><Layer level="ARTWORK"><Texture name="$parentColor" setAllPoints="true"><Color r="0.25" g="0.5" b="1" a="0.75"/></Texture></Layer></Layers>
</Frame>
<Frame name="Owner"><Size x="100" y="100"/>
  <Layers><Layer level="ARTWORK">
    <Texture name="Replaced" inherits="TintedAsset" setAllPoints="true"><Color r="1" g="0" b="0" a="0.75"/></Texture>
    <Texture name="FileWins" inherits="TintedAsset" file="Interface\Glues\New" setAllPoints="true"><Color r="0" g="1" b="0" a="0.75"/></Texture>
    <Texture name="EmptyFile" inherits="TintedAsset" file="" setAllPoints="true"/>
  </Layer></Layers>
  <Scripts><OnLoad>CreateFrame("Frame", "Dynamic", nil, "ColorTemplate")</OnLoad></Scripts>
</Frame>
</Ui>"# },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (800, 600), false)?;
    let member = |name| {
        manager
            .presentation()
            .members_in_draw_order()
            .iter()
            .find(|member| manager.objects()[member.object_index()].name() == Some(name))
            .ok_or("missing texture member")
    };
    let replaced = member("Replaced")?;
    assert_eq!(
        replaced.source(),
        &UiTextureSource::SolidColor([1.0, 0.0, 0.0, 0.75])
    );
    assert_eq!(replaced.vertex_colors(), [[0.5, 0.5, 0.5, 1.0]; 4]);
    let file = member("FileWins")?;
    assert!(
        matches!(file.source(), UiTextureSource::Asset(path) if path.as_str() == "INTERFACE\\GLUES\\NEW.BLP")
    );
    assert_eq!(file.vertex_colors(), [[0.0, 1.0, 0.0, 0.75]; 4]);
    let empty = member("EmptyFile")?;
    assert!(
        matches!(empty.source(), UiTextureSource::Asset(path) if path.as_str() == "INTERFACE\\GLUES\\BASE.BLP")
    );
    let dynamic = member("DynamicColor")?;
    assert_eq!(
        dynamic.source(),
        &UiTextureSource::SolidColor([0.25, 0.5, 1.0, 0.75])
    );
    assert_eq!(dynamic.vertex_colors(), [[1.0; 4]; 4]);
    Ok(())
}

/// Authored overlays retain zero-opacity slots while their owning frame is
/// visible, including mixed widget/visibility callbacks such as race choices.
#[test]
fn hidden_texture_overlays_retain_slots_across_content_and_visibility_changes()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile { path: "Interface\\GlueXML\\GlueXML.toc", bytes: b"Overlays.xml\n" },
        FixtureFile { path: "Interface\\GlueXML\\Overlays.xml", bytes: br#"<Ui>
<CheckButton name="Choice"><Size x="100" y="100"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Layers><Layer level="OVERLAY">
    <Texture name="ChoiceOverlay" hidden="true" setAllPoints="true"><Color r="0" g="0" b="0" a="0.75"/></Texture>
    <Texture name="EmptyOverlay" hidden="true" setAllPoints="true"/>
  </Layer></Layers>
  <Scripts><OnClick>
    if ChoiceOverlay:IsShown() then ChoiceOverlay:Hide() EmptyOverlay:Hide() else ChoiceOverlay:Show() EmptyOverlay:Show() end
    self:SetChecked(ChoiceOverlay:IsShown())
  </OnClick></Scripts>
</CheckButton>
<Frame name="OtherScreen" hidden="true"><Size x="100" y="100"/>
  <Layers><Layer level="ARTWORK"><Texture name="OtherScreenTexture" setAllPoints="true"><Color r="1" g="0" b="0"/></Texture></Layer></Layers>
</Frame>
</Ui>"# },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (800, 600), false)?;
    let index = |name| {
        manager
            .objects()
            .iter()
            .position(|object| object.name() == Some(name))
            .ok_or("missing object")
    };
    let choice = index("Choice")?;
    let overlay = index("ChoiceOverlay")?;
    let other = index("OtherScreenTexture")?;
    let bounds = manager
        .geometry()
        .region(choice)
        .ok_or("missing choice bounds")?
        .presentation_bounds();
    let pointer = (
        bounds.left() + bounds.width() * 0.5,
        bounds.bottom() + bounds.height() * 0.5,
    );
    let mesh = manager.render_plan().mesh();
    assert!(mesh.contains_object(overlay));
    assert!(!mesh.contains_object(other));
    assert_eq!(mesh.batches()[0].opacity(), 0.0);
    let identity = mesh.geometry_identity();
    let vertices = mesh.vertices().to_vec();
    let indices = mesh.indices().to_vec();
    for expected in [1.0, 0.0, 1.0] {
        manager.pointer_button(pointer, UiPointerButton::Left, true)?;
        assert!(
            manager
                .pointer_button(pointer, UiPointerButton::Left, false)?
                .click_activated()
        );
        let mesh = manager.render_plan().mesh();
        assert_eq!(mesh.geometry_identity(), identity);
        assert_eq!(mesh.vertices(), vertices);
        assert_eq!(mesh.indices(), indices);
        assert_eq!(mesh.object_indices(), [overlay]);
        assert_eq!(mesh.batches()[0].opacity(), expected);
        assert_eq!(mesh.vertices()[0].color(), [0.0, 0.0, 0.0, 0.75]);
    }
    Ok(())
}

/// One native callback can recolor a backdrop and texture while changing another
/// texture's UVs. A later material replacement still removes the old source.
#[test]
fn mixed_content_patch_preserves_native_decorations_and_material_changes()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile { path: "Interface\\GlueXML\\GlueXML.toc", bytes: b"Mixed.xml\n" },
        FixtureFile { path: "Interface\\GlueXML\\Mixed.xml", bytes: br#"<Ui>
<Button name="Panel"><Size x="200" y="100"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Backdrop bgFile="Interface\Glues\Background"><Color r="1" g="1" b="1"/></Backdrop>
  <Layers><Layer level="ARTWORK">
    <Texture name="Tint" file="Interface\Glues\Tint"><Size x="40" y="30"/><Anchors><Anchor point="LEFT"/></Anchors></Texture>
    <Texture name="Icon" file="Interface\Glues\Icon"><Size x="40" y="30"/><Anchors><Anchor point="RIGHT"/></Anchors></Texture>
    <Texture name="Untouched" file="Interface\Glues\Untouched"><Size x="10" y="10"/><Anchors><Anchor point="BOTTOM"/></Anchors></Texture>
  </Layer></Layers>
  <Scripts><OnClick>
    if not self.changed then
      self.changed = true
      self:SetBackdropColor(0.25, 0.5, 0.75, 1)
      Tint:SetVertexColor(0.5, 0.75, 1, 0.5)
      Icon:SetTexCoord(0.25, 0.75, 0.125, 0.875)
    else
      Icon:SetTexture("Interface\\Glues\\Other")
    end
  </OnClick></Scripts>
</Button>
</Ui>"# },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (800, 600), false)?;
    let index = |name: &str| {
        manager
            .objects()
            .iter()
            .position(|object| object.name() == Some(name))
            .ok_or("missing named region")
    };
    let panel = index("Panel")?;
    let tint = index("Tint")?;
    let icon = index("Icon")?;
    let untouched = index("Untouched")?;
    let bounds = manager
        .geometry()
        .region(panel)
        .ok_or("missing panel geometry")?
        .presentation_bounds();
    let pointer = (
        bounds.left() + bounds.width() * 0.5,
        bounds.bottom() + bounds.height() * 0.5,
    );
    let mesh = manager.render_plan().mesh();
    let old_owners = mesh.object_indices().to_vec();
    let old_vertices = mesh.vertices().to_vec();
    manager.pointer_button(pointer, UiPointerButton::Left, true)?;
    assert!(
        manager
            .pointer_button(pointer, UiPointerButton::Left, false)?
            .click_activated()
    );
    let mesh = manager.render_plan().mesh();
    assert_eq!(mesh.object_indices(), old_owners);
    assert_eq!(mesh.vertices().len(), old_vertices.len());
    for (quad, &owner) in mesh.object_indices().iter().enumerate() {
        let vertices = &mesh.vertices()[quad * 4..quad * 4 + 4];
        let old = &old_vertices[quad * 4..quad * 4 + 4];
        assert!(
            vertices
                .iter()
                .zip(old)
                .all(|(new, old)| new.position() == old.position())
        );
        if owner == panel {
            assert!(
                vertices
                    .iter()
                    .all(|vertex| vertex.color() == [0.25, 0.5, 0.75, 1.0])
            );
        }
        if owner == tint {
            assert!(
                vertices
                    .iter()
                    .all(|vertex| vertex.color() == [0.5, 0.75, 1.0, 0.5])
            );
        }
        if owner == icon {
            assert_eq!(vertices[0].texture_coordinates(), [0.25, 0.125]);
            assert_eq!(vertices[3].texture_coordinates(), [0.75, 0.875]);
        }
        if owner == untouched {
            assert_eq!(vertices, old);
        }
    }
    manager.pointer_button(pointer, UiPointerButton::Left, true)?;
    assert!(
        manager
            .pointer_button(pointer, UiPointerButton::Left, false)?
            .click_activated()
    );
    let sources = manager
        .render_plan()
        .mesh()
        .sources_for_object(icon)
        .collect::<Vec<_>>();
    assert_eq!(sources.len(), 1);
    assert!(
        matches!(sources[0], UiRenderSource::Texture(path) if path.as_str() == "INTERFACE\\GLUES\\OTHER.BLP")
    );
    let requests = manager.render_plan().texture_assets().requests();
    assert!(
        requests
            .iter()
            .any(|request| request.path().as_str() == "INTERFACE\\GLUES\\OTHER.BLP")
    );
    assert!(
        !requests
            .iter()
            .any(|request| request.path().as_str() == "INTERFACE\\GLUES\\ICON.BLP")
    );
    let mesh = manager.render_plan().mesh();
    assert_eq!(mesh.object_indices(), old_owners);
    for (quad, &owner) in mesh.object_indices().iter().enumerate() {
        if owner == untouched {
            assert_eq!(
                &mesh.vertices()[quad * 4..quad * 4 + 4],
                &old_vertices[quad * 4..quad * 4 + 4]
            );
        }
    }
    Ok(())
}

/// Live Lua texture mutations feed stable stock packet ordering and membership.
#[test]
fn glue_presentation_packets_use_post_lua_texture_state() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Presentation.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Presentation.xml",
            bytes: br#"<Ui>
<Texture name="Ownerless" file="Interface\Glues\Ownerless"/>
<Frame name="LowOwner" frameStrata="LOW" frameLevel="2" alpha="0.5">
  <Size x="200" y="100"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Layers>
    <Layer level="BACKGROUND">
      <Texture name="LowFirst" file="Interface\Glues\First"><Size x="20" y="10"/></Texture>
      <Texture name="LowSecond" file="Interface\Glues\Second"><Size x="20" y="10"/></Texture>
    </Layer>
    <Layer level="ARTWORK">
      <Texture name="Mutated" file="Interface\Glues\Before"><Size x="40" y="20"/></Texture>
      <Texture name="Empty"><Size x="10" y="10"/></Texture>
      <Texture name="Transparent" file="Interface\Glues\Transparent" alpha="0"><Size x="10" y="10"/></Texture>
    </Layer>
  </Layers>
  <Scripts><OnLoad>
    Mutated:SetTexture("Interface\\Glues\\After.tga")
    Mutated:SetBlendMode("ADD")
    Mutated:SetDrawLayer("OVERLAY", -2)
    Mutated:SetTexCoord(0.25, 0.75, 0, 1)
    Mutated:SetDesaturated(true)
    Mutated:SetGradientAlpha("VERTICAL", 0.1, 0.2, 0.3, 0.4, 0.6, 0.7, 0.8, 0.9)
  </OnLoad></Scripts>
</Frame>
<Button name="HighButton" frameStrata="HIGH" frameLevel="1">
  <Size x="80" y="30"/><Anchors><Anchor point="CENTER"/></Anchors>
  <NormalTexture name="$parentNormal" file="Interface\Glues\Normal"/>
  <DisabledTexture name="$parentDisabled" file="Interface\Glues\Disabled"/>
  <Scripts><OnLoad>self:Disable()</OnLoad></Scripts>
</Button>
<Frame name="SolidOwner" frameStrata="HIGH" frameLevel="2"><Layers><Layer level="HIGHLIGHT">
  <Texture name="Solid"><Size x="12" y="8"/></Texture>
</Layer></Layers><Scripts><OnLoad>Solid:SetTexture(0.2, 0.4, 0.6, 0.8)</OnLoad></Scripts></Frame>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let presentation = manager.presentation();

    assert_eq!(presentation.member_count(), 7);
    assert_eq!(presentation.packets().len(), 6);

    let low_background = presentation.packets()[0];
    assert_eq!(low_background.key().strata(), UiFrameStrata::Low);
    assert_eq!(low_background.key().frame_level(), 2);
    assert_eq!(low_background.key().draw_rank(), 0);
    assert_eq!(low_background.member_count(), 2);
    let names = presentation
        .members(0)
        .ok_or("missing low background packet")?
        .iter()
        .map(|member| manager.objects()[member.object_index()].name())
        .collect::<Vec<_>>();
    assert_eq!(names, [Some("LowFirst"), Some("LowSecond")]);

    let transparent = &presentation
        .members(1)
        .ok_or("missing transparent packet")?[0];
    assert_eq!(
        manager.objects()[transparent.object_index()].name(),
        Some("Transparent")
    );
    assert_eq!(transparent.opacity(), 0.0);
    let mutated = &presentation.members(2).ok_or("missing mutated packet")?[0];
    assert_eq!(
        manager.objects()[mutated.object_index()].name(),
        Some("Mutated")
    );
    assert_eq!(presentation.packets()[2].key().draw_rank(), 30);
    assert_eq!(presentation.packets()[2].key().draw_sub_level(), -2);
    assert_eq!(mutated.blend_mode(), UiBlendMode::Add);
    assert!(mutated.desaturated());
    assert_eq!(
        mutated.source(),
        &UiTextureSource::Asset(solarity_asset::AssetPath::new(
            "Interface\\Glues\\After.blp"
        )?)
    );
    assert_eq!(
        mutated.tex_coords(),
        [0.25, 0.0, 0.25, 1.0, 0.75, 0.0, 0.75, 1.0]
    );
    assert!((mutated.vertex_colors()[0][3] - 0.9).abs() < 0.000_01);
    assert!((mutated.vertex_colors()[1][3] - 0.4).abs() < 0.000_01);
    assert!((mutated.opacity() - 0.5).abs() < 0.000_01);

    let normal = &presentation.members(3).ok_or("missing normal packet")?[0];
    assert_eq!(
        manager.objects()[normal.object_index()].name(),
        Some("HighButtonNormal")
    );
    assert_eq!(normal.opacity(), 0.0);

    let disabled = &presentation.members(4).ok_or("missing disabled packet")?[0];
    assert_eq!(
        manager.objects()[disabled.object_index()].name(),
        Some("HighButtonDisabled")
    );
    assert_eq!(presentation.packets()[4].key().draw_rank(), 21);
    assert_eq!(disabled.bounds().width(), 80.0);
    assert_eq!(disabled.bounds().height(), 30.0);

    let solid = &presentation.members(5).ok_or("missing solid packet")?[0];
    assert_eq!(
        manager.objects()[solid.object_index()].name(),
        Some("Solid")
    );
    assert_eq!(
        solid.source(),
        &UiTextureSource::SolidColor([0.2, 0.4, 0.6, 0.8])
    );
    assert_eq!(
        presentation.packets()[5].key().strata(),
        UiFrameStrata::High
    );
    assert_eq!(presentation.packets()[5].key().draw_rank(), 40);

    let mesh = manager.render_plan().mesh();
    assert_eq!(mesh.logical_extent(), [1_365.333_4, 768.0]);
    assert_eq!(mesh.vertices().len(), 28);
    // Every contiguous batch reuses the same canonical zero-based quad-index
    // prefix through baseVertex instead of retaining absolute indices per quad.
    assert_eq!(mesh.indices().len(), 6);
    assert_eq!(mesh.object_indices().len(), presentation.member_count());
    let mutated_path = solarity_asset::AssetPath::new("Interface\\Glues\\After.blp")?;
    let mutated_batch = mesh
        .batches()
        .iter()
        .find(|batch| batch.source() == &UiRenderSource::Texture(mutated_path.clone()))
        .ok_or("missing mutated texture render batch")?;
    assert_eq!(mutated_batch.blend(), UiRenderBlend::Additive);
    assert_eq!(
        mutated_batch.horizontal_address(),
        UiTextureAddressMode::Clamp
    );
    assert_eq!(
        mutated_batch.vertical_address(),
        UiTextureAddressMode::Clamp
    );
    assert_eq!(mutated_batch.residency(), UiTextureResidency::Blocking);
    assert!(mutated_batch.desaturated());
    assert!(matches!(mutated_batch.source(), UiRenderSource::Texture(_)));
    let solid_quad = mesh.object_indices().len() - 1;
    let solid_vertex = mesh.vertices()[solid_quad * 4];
    assert_eq!(solid_vertex.color(), [0.2, 0.4, 0.6, 0.8]);
    let texture_assets = manager.render_plan().texture_assets();
    assert_eq!(texture_assets.requests().len(), 6);
    assert_eq!(
        texture_assets.request_for_batch(mesh.batches().len() - 1),
        None,
        "the final solid-color batch has no invented image request"
    );
    Ok(())
}

/// Alpha-only Glue updates patch retained draw state without rebuilding UI bytes.
#[test]
fn glue_alpha_updates_retain_the_existing_ui_mesh() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Fade.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Fade.xml",
            bytes: br#"<Ui><Frame name="Fading" alpha="1">
  <Size x="100" y="50"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Layers><Layer level="ARTWORK">
    <Texture name="FadingTexture" file="Interface\Glues\Fade" setAllPoints="true"/>
  </Layer></Layers>
  <Animations><AnimationGroup><Translation offsetX="20" offsetY="8" duration="1"/>
    <Scripts><OnLoad>self:Play()</OnLoad></Scripts>
  </AnimationGroup></Animations>
  <Scripts><OnUpdate>self:SetAlpha(self:GetAlpha() - elapsed)</OnUpdate></Scripts>
</Frame></Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1280, 720), false)?;
    let identity = manager.render_plan().mesh().geometry_identity();
    let vertex_bytes = manager.render_plan().mesh().vertex_bytes().to_vec();
    let index_bytes = manager.render_plan().mesh().index_bytes().to_vec();

    assert!(manager.update(0.25)?);

    let mesh = manager.render_plan().mesh();
    assert_eq!(mesh.geometry_identity(), identity);
    assert_eq!(mesh.vertex_bytes(), vertex_bytes);
    assert_eq!(mesh.index_bytes(), index_bytes);
    assert_eq!(mesh.batches().len(), 1);
    assert!((mesh.batches()[0].opacity() - 0.75).abs() < 0.000_01);
    assert_eq!(mesh.batches()[0].translation(), [5.0, 2.0]);
    Ok(())
}

/// Button skins follow stock's mutually exclusive pushed, disabled, and
/// checked role predicates, including the missing-disabled-skin fallback.
#[test]
fn glue_presentation_selects_stock_button_state_textures() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Buttons.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Buttons.xml",
            bytes: br#"<Ui>
<CheckButton name="ButtonTemplate" virtual="true"><Size x="20" y="20"/>
  <NormalTexture name="$parentNormal" file="Interface\Glues\Normal"/>
  <PushedTexture name="$parentPushed" file="Interface\Glues\Pushed"/>
  <DisabledTexture name="$parentDisabled" file="Interface\Glues\Disabled"/>
  <CheckedTexture name="$parentChecked" file="Interface\Glues\Checked"/>
  <DisabledCheckedTexture name="$parentDisabledChecked" file="Interface\Glues\DisabledChecked"/>
</CheckButton>
<CheckButton name="NormalButton" inherits="ButtonTemplate"/>
<CheckButton name="PushedButton" inherits="ButtonTemplate">
  <Scripts><OnLoad>self:SetButtonState("PUSHED")</OnLoad></Scripts>
</CheckButton>
<CheckButton name="DisabledButton" inherits="ButtonTemplate">
  <Scripts><OnLoad>self:Disable()</OnLoad></Scripts>
</CheckButton>
<CheckButton name="FallbackButton"><Size x="20" y="20"/>
  <NormalTexture name="$parentNormal" file="Interface\Glues\Normal"/>
  <Scripts><OnLoad>self:Disable()</OnLoad></Scripts>
</CheckButton>
<CheckButton name="CheckedButton" inherits="ButtonTemplate">
  <Scripts><OnLoad>self:SetChecked(1)</OnLoad></Scripts>
</CheckButton>
<CheckButton name="DisabledCheckedButton" inherits="ButtonTemplate">
  <Scripts><OnLoad>self:SetChecked(1) self:Disable()</OnLoad></Scripts>
</CheckButton>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let opacities = manager
        .presentation()
        .members_in_draw_order()
        .iter()
        .filter_map(|member| {
            manager.objects()[member.object_index()]
                .name()
                .map(|name| (name, member.opacity()))
        })
        .collect::<std::collections::BTreeMap<_, _>>();

    let is_active = |name| opacities.get(name).is_some_and(|opacity| *opacity > 0.0);
    assert!(is_active("NormalButtonNormal"));
    assert!(!is_active("NormalButtonPushed"));
    assert!(is_active("PushedButtonPushed"));
    assert!(!is_active("PushedButtonNormal"));
    assert!(is_active("DisabledButtonDisabled"));
    assert!(!is_active("DisabledButtonNormal"));
    assert!(is_active("FallbackButtonNormal"));
    assert!(is_active("CheckedButtonNormal"));
    assert!(is_active("CheckedButtonChecked"));
    assert!(is_active("DisabledCheckedButtonDisabledChecked"));
    assert!(!is_active("DisabledCheckedButtonDisabled"));
    assert!(!is_active("DisabledCheckedButtonNormal"));
    Ok(())
}

/// Visible ModelFFX state crosses the retained Lua-to-render boundary exactly.
#[test]
fn glue_presentation_retains_stock_model_ffx_state() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"ModelPresentation.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\ModelPresentation.xml",
            bytes: br#"<Ui>
<ModelFFX name="AccountLogin" frameStrata="BACKGROUND" frameLevel="2" alpha="0.75">
  <Size x="800" y="600"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Scripts><OnLoad>
    self:SetModel("Interface\\Glues\\Models\\UI_MainMenu_Northrend\\UI_MainMenu_Northrend.m2")
    self:SetCamera(1)
    self:SetSequence(7)
    self:SetSequenceTime(7, 1250)
    self:SetModelScale(1.5)
  </OnLoad></Scripts>
</ModelFFX>
</Ui>"#,
        },
        FixtureFile {
            path: "Interface\\Glues\\Models\\UI_MainMenu_Northrend\\UI_MainMenu_Northrend.m2",
            bytes: b"fixture model marker",
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1600, 900), false)?;

    let models = manager.presentation().models();
    assert_eq!(models.len(), 1);
    let model = &models[0];
    assert_eq!(
        manager.objects()[model.object_index()].name(),
        Some("AccountLogin")
    );
    assert_eq!(
        model.path().as_str(),
        "INTERFACE\\GLUES\\MODELS\\UI_MAINMENU_NORTHREND\\UI_MAINMENU_NORTHREND.M2"
    );
    assert_eq!(model.camera(), 1);
    assert_eq!(model.sequence(), 7);
    assert_eq!(model.sequence_time_sequence(), 7);
    assert_eq!(model.sequence_time_ms(), 1250);
    assert_eq!(model.model_scale(), 1.5);
    assert!((model.bounds().width() - 800.0).abs() < 0.000_01);
    assert!((model.bounds().height() - 600.0).abs() < 0.000_01);
    assert_eq!(model.alpha(), 0.75);
    assert_eq!(model.strata(), UiFrameStrata::Background);
    assert_eq!(model.frame_level(), 2);
    Ok(())
}

/// A model assigned by stock `OnLoad` remains discoverable while another Glue
/// screen hides it from the render presentation.
#[test]
fn glue_retains_hidden_model_source_for_prewarm() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"HiddenModel.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\HiddenModel.xml",
            bytes: br#"<Ui>
<ModelFFX name="AccountLogin" hidden="true">
  <Size x="800" y="600"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Scripts><OnLoad>
    self:SetModel("Interface\\Glues\\Models\\UI_MainMenu_Northrend\\UI_MainMenu_Northrend.m2")
  </OnLoad></Scripts>
</ModelFFX>
</Ui>"#,
        },
        FixtureFile {
            path: "Interface\\Glues\\Models\\UI_MainMenu_Northrend\\UI_MainMenu_Northrend.m2",
            bytes: b"fixture model marker",
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1600, 900), false)?;

    assert!(manager.presentation().models().is_empty());
    let (path, light_count) = manager
        .configured_model_source("AccountLogin")?
        .ok_or("missing hidden AccountLogin model source")?;
    assert_eq!(
        path.as_str(),
        "INTERFACE\\GLUES\\MODELS\\UI_MAINMENU_NORTHREND\\UI_MAINMENU_NORTHREND.M2"
    );
    assert_eq!(light_count, 0);
    let configured = manager
        .configured_model_presentation("AccountLogin")?
        .ok_or("missing hidden AccountLogin model presentation")?;
    assert_eq!(configured.object_index(), 0);
    assert_eq!(configured.camera(), 0);
    assert_eq!(configured.sequence(), 0);
    assert!((configured.bounds().width() - 800.0).abs() < 0.000_01);
    assert!((configured.bounds().height() - 600.0).abs() < 0.000_01);
    assert_eq!(manager.configured_model_source("MissingModel")?, None);
    Ok(())
}

/// Native backdrops preserve inherited XML state and the eight-slice edge atlas.
#[test]
fn glue_presentation_builds_stock_native_backdrop_quads() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Backdrop.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Backdrop.xml",
            bytes: br#"<Ui>
<Frame name="BackdropTemplate" virtual="true">
  <Backdrop bgFile="Interface\Tooltips\UI-Tooltip-Background"
            edgeFile="Interface\Tooltips\UI-Tooltip-Border"
            tile="true" alphaMode="BLEND">
    <TileSize><AbsValue val="16"/></TileSize>
    <EdgeSize><AbsValue val="16"/></EdgeSize>
    <BackgroundInsets><AbsInset left="4" right="4" top="4" bottom="4"/></BackgroundInsets>
    <Color r="0.1" g="0.2" b="0.3" a="0.4"/>
    <BorderColor r="0.4" g="0.5" b="0.6" a="0.7"/>
  </Backdrop>
</Frame>
<Frame name="Wrapper" hidden="true">
  <Size x="80" y="48"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Frames>
    <Frame name="Panel" inherits="BackdropTemplate" setAllPoints="true"
           frameStrata="MEDIUM" frameLevel="3" alpha="0.5">
      <Scripts><OnShow>
        self:SetBackdropColor(0.2, 0.3, 0.4, 0.8)
        self:SetBackdropBorderColor(0.6, 0.7, 0.8, 0.6)
      </OnShow></Scripts>
    </Frame>
  </Frames>
  <Scripts><OnLoad>self:Show()</OnLoad></Scripts>
</Frame>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1600, 900), false)?;

    assert_eq!(manager.backdrops().state_count(), 1);
    let panel_index = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("Panel"))
        .ok_or("missing Panel")?;
    let backdrop = manager
        .backdrops()
        .state(panel_index)
        .ok_or("missing Panel backdrop")?;
    assert_eq!(backdrop.insets(), [4.0, 4.0, 4.0, 4.0]);
    assert_eq!(backdrop.tile_size(), 16.0);
    assert_eq!(backdrop.edge_size(), 16.0);

    let presentation = manager.presentation();
    assert_eq!(presentation.member_count(), 13);
    assert_eq!(presentation.packets().len(), 2);
    assert_eq!(presentation.packets()[0].key().draw_rank(), 0);
    assert_eq!(presentation.packets()[0].member_count(), 1);
    assert_eq!(presentation.packets()[1].key().draw_rank(), 10);
    assert_eq!(presentation.packets()[1].member_count(), 12);

    let background = &presentation
        .members(0)
        .ok_or("missing backdrop background")?[0];
    assert_eq!(background.object_index(), panel_index);
    assert_eq!(background.bounds().width(), 72.0);
    assert_eq!(background.bounds().height(), 40.0);
    assert_eq!(
        background.tex_coords(),
        [0.0, 0.0, 0.0, 2.5, 4.5, 0.0, 4.5, 2.5]
    );
    assert!(background.horizontal_tiling());
    assert!(background.vertical_tiling());
    assert_eq!(background.vertex_colors()[0], [0.2, 0.3, 0.4, 0.8]);
    assert_eq!(background.opacity(), 0.5);

    let border = presentation.members(1).ok_or("missing backdrop border")?;
    assert_eq!(
        border[0].tex_coords(),
        [0.5, 0.0, 0.5, 1.0, 0.625, 0.0, 0.625, 1.0]
    );
    assert_eq!(border[0].vertex_colors()[0], [0.6, 0.7, 0.8, 0.6]);
    assert_eq!(border[0].opacity(), 0.5);
    assert!(border.iter().all(|quad| quad.object_index() == panel_index));
    assert!(border.iter().all(|quad| !quad.horizontal_tiling()));
    assert_eq!(manager.render_plan().mesh().vertices().len(), 52);
    assert_eq!(manager.render_plan().texture_assets().requests().len(), 2);
    Ok(())
}

/// ScrollFrame translates and clips every region beneath its assigned child
/// while leaving sibling scrollbar chrome in the ScrollFrame's own space.
#[test]
fn glue_render_plan_retains_scrolled_textures_without_moving_chrome() -> Result<(), Box<dyn Error>>
{
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"TextureScroll.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\TextureScroll.xml",
            bytes: br#"<Ui>
<ScrollFrame name="Viewport" frameStrata="DIALOG" frameLevel="3">
  <Size x="100" y="50"/><Anchors><Anchor point="CENTER"/></Anchors>
  <ScrollChild><Frame name="Content"><Size x="100" y="100"/>
    <Layers><Layer level="ARTWORK">
      <Texture name="ContentTexture" file="Interface\Glues\Content" setAllPoints="true"/>
      <Texture name="OffscreenTexture" file="Interface\Glues\Offscreen">
        <Size x="10" y="10"/><Anchors><Anchor point="BOTTOMLEFT" relativeTo="Content" relativePoint="TOPLEFT">
          <Offset><AbsDimension x="0" y="100"/></Offset>
        </Anchor></Anchors>
      </Texture>
    </Layer></Layers>
  </Frame></ScrollChild>
  <Frames><Frame name="Chrome"><Size x="20" y="20"/>
    <Anchors><Anchor point="TOP" relativeTo="Viewport" relativePoint="TOP">
      <Offset><AbsDimension x="0" y="20"/></Offset>
    </Anchor></Anchors>
    <Layers><Layer level="OVERLAY">
      <Texture name="ChromeTexture" file="Interface\Glues\Chrome" setAllPoints="true"/>
    </Layer></Layers>
  </Frame></Frames>
  <Scripts><OnLoad>
    ContentTexture:SetGradientAlpha("VERTICAL", 0, 0, 1, 1, 1, 0, 0, 1)
    self:SetVerticalScroll(25)
  </OnLoad></Scripts>
</ScrollFrame>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let content_index = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("ContentTexture"))
        .ok_or("missing scrolled content texture")?;
    let chrome_index = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("ChromeTexture"))
        .ok_or("missing ScrollFrame chrome texture")?;
    let offscreen_index = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("OffscreenTexture"))
        .ok_or("missing offscreen texture")?;
    let viewport_index = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("Viewport"))
        .ok_or("missing viewport")?;
    let mesh = manager.render_plan().mesh();
    let content_quad = mesh
        .object_indices()
        .iter()
        .position(|index| *index == content_index)
        .ok_or("scrolled texture did not render")?;
    let content = &mesh.vertices()[content_quad * 4..content_quad * 4 + 4];
    let positions = content
        .iter()
        .map(|vertex| vertex.position())
        .collect::<Vec<_>>();
    let expected_left = 1920.0 / 1080.0 * 384.0 - 50.0;
    assert!(positions.iter().all(|position| {
        (position[0] - expected_left).abs() < 0.000_1
            || (position[0] - (expected_left + 100.0)).abs() < 0.000_1
    }));
    assert!(positions.iter().all(|position| {
        (position[1] - 309.0).abs() < 0.000_1 || (position[1] - 409.0).abs() < 0.000_1
    }));
    assert_eq!(content[0].texture_coordinates(), [0.0, 0.0]);
    assert_eq!(content[1].texture_coordinates(), [0.0, 1.0]);
    assert_eq!(content[2].texture_coordinates(), [1.0, 0.0]);
    assert_eq!(content[3].texture_coordinates(), [1.0, 1.0]);
    let content_batch = mesh
        .batches()
        .iter()
        .find(|batch| {
            let first = batch.first_quad() as usize;
            first <= content_quad && content_quad < first + batch.quad_count() as usize
        })
        .ok_or("missing retained content batch")?;
    assert_eq!(
        content_batch.transform(),
        Some(UiRenderTransform::ScrollFrame(viewport_index))
    );
    assert_eq!(content_batch.translation(), [0.0, 25.0]);
    assert_eq!(
        content_batch.clip(),
        Some([expected_left, 359.0, expected_left + 100.0, 409.0])
    );

    let chrome_quad = mesh
        .object_indices()
        .iter()
        .position(|index| *index == chrome_index)
        .ok_or("ScrollFrame chrome did not render")?;
    let chrome = &mesh.vertices()[chrome_quad * 4..chrome_quad * 4 + 4];
    assert!(
        chrome
            .iter()
            .any(|vertex| (vertex.position()[1] - 429.0).abs() < 0.000_1)
    );
    let offscreen_quad = mesh
        .object_indices()
        .iter()
        .position(|index| *index == offscreen_index)
        .ok_or("fully clipped texture lost its retained mesh slot")?;
    let offscreen = &mesh.vertices()[offscreen_quad * 4..offscreen_quad * 4 + 4];
    assert!(offscreen.iter().any(|vertex| {
        vertex.position()[0] != offscreen[0].position()[0]
            || vertex.position()[1] != offscreen[0].position()[1]
    }));
    assert!(
        manager
            .render_plan()
            .texture_assets()
            .requests()
            .iter()
            .any(|request| request.path().as_str() == "INTERFACE\\GLUES\\OFFSCREEN.BLP")
    );
    Ok(())
}
