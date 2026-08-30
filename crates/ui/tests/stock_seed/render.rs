//! External stock-compatibility tests for deterministic UI presentation.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_rendering::{UiRenderBlend, UiRenderSource, UiTextureAddressMode, UiTextureResidency};
use solarity_ui::{GlueManager, UiBlendMode, UiFrameStrata, UiTextureSource};

use crate::support::{Fixture, FixtureFile};

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

    assert_eq!(presentation.member_count(), 5);
    assert_eq!(presentation.packets().len(), 4);

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

    let mutated = &presentation.members(1).ok_or("missing mutated packet")?[0];
    assert_eq!(
        manager.objects()[mutated.object_index()].name(),
        Some("Mutated")
    );
    assert_eq!(presentation.packets()[1].key().draw_rank(), 30);
    assert_eq!(presentation.packets()[1].key().draw_sub_level(), -2);
    assert_eq!(mutated.blend_mode(), UiBlendMode::Add);
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
    assert!((mutated.vertex_colors()[0][3] - 0.45).abs() < 0.000_01);
    assert!((mutated.vertex_colors()[1][3] - 0.2).abs() < 0.000_01);

    let disabled = &presentation.members(2).ok_or("missing disabled packet")?[0];
    assert_eq!(
        manager.objects()[disabled.object_index()].name(),
        Some("HighButtonDisabled")
    );
    assert_eq!(presentation.packets()[2].key().draw_rank(), 21);

    let solid = &presentation.members(3).ok_or("missing solid packet")?[0];
    assert_eq!(
        manager.objects()[solid.object_index()].name(),
        Some("Solid")
    );
    assert_eq!(
        solid.source(),
        &UiTextureSource::SolidColor([0.2, 0.4, 0.6, 0.8])
    );
    assert_eq!(
        presentation.packets()[3].key().strata(),
        UiFrameStrata::High
    );
    assert_eq!(presentation.packets()[3].key().draw_rank(), 40);

    let mesh = manager.render_plan().mesh();
    assert_eq!(mesh.logical_extent(), [1_365.333_4, 768.0]);
    assert_eq!(mesh.vertices().len(), 20);
    assert_eq!(mesh.indices().len(), 30);
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
    assert!(matches!(mutated_batch.source(), UiRenderSource::Texture(_)));
    let solid_quad = mesh.object_indices().len() - 1;
    let solid_vertex = mesh.vertices()[solid_quad * 4];
    assert_eq!(solid_vertex.color(), [0.2, 0.4, 0.6, 0.8]);
    let texture_assets = manager.render_plan().texture_assets();
    assert_eq!(texture_assets.requests().len(), 4);
    assert_eq!(
        texture_assets.request_for_batch(mesh.batches().len() - 1),
        None,
        "the final solid-color batch has no invented image request"
    );
    Ok(())
}
