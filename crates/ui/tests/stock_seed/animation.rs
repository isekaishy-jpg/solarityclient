//! External stock-compatibility tests for retained XML animation objects.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, GlueManager, UiAnimationKind, UiAnimationLooping, UiAnimationPlan,
    UiAnimationValue, UiBundle, UiFramePlan, UiLayoutPlan, UiManifestKind, UiObjectCatalog,
    UiObjectTree, UiRegionStatePlan, UiRuntimeTemplatePlan, UiScriptEnvironment, UiScriptPlan,
    UiScriptRuntime, UiScriptRuntimePlan, UiTexturePlan, UiTextureStatePlan,
};

use crate::support::{Fixture, FixtureFile};

/// Animation groups retain their separate ownership tree and stock Lua state.
#[test]
fn animation_plan_constructs_named_primitives_before_owner_on_load() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\FrameXML\\FrameXML.toc",
            bytes: b"Animations.xml\n",
        },
        FixtureFile {
            path: "Interface\\FrameXML\\Animations.xml",
            bytes: br#"<Ui>
  <Frame name="Root">
    <Frames>
      <Frame name="$parentCallOut">
        <Animations>
          <AnimationGroup looping="BOUNCE" parentKey="pulseGroup">
            <Alpha name="$parentPulser" change="-.7" duration=".75" parentKey="pulser"/>
            <Translation offsetX="240" offsetY="-4" duration=".85" order="2"/>
            <Scripts><OnFinished>ANIMATION_FINISHED = self:GetObjectType()</OnFinished></Scripts>
          </AnimationGroup>
        </Animations>
        <Scripts><OnLoad>
          self.pulseGroup:Play()
          RootCallOutPulser:Stop()
          ANIMATION_READY = self.pulseGroup:GetLooping()
        </OnLoad></Scripts>
      </Frame>
    </Frames>
  </Frame>
</Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Frame)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;
    let animations = UiAnimationPlan::from_tree(&tree)?;

    assert_eq!(animations.groups().len(), 1);
    assert_eq!(animations.animations().len(), 2);
    let group = &animations.groups()[0];
    let owner = tree
        .node_index("RootCallOut")
        .ok_or("missing RootCallOut")?;
    assert_eq!(group.owner(), owner);
    assert_eq!(group.name(), None);
    assert_eq!(group.parent_key(), Some("pulseGroup"));
    assert_eq!(group.looping(), UiAnimationLooping::Bounce);
    let alpha = &animations.animations()[0];
    assert_eq!(alpha.name(), Some("RootCallOutPulser"));
    assert_eq!(alpha.kind(), UiAnimationKind::Alpha);
    assert_eq!(alpha.order(), 1);
    assert_eq!(alpha.value(), UiAnimationValue::Alpha { change: -0.7 });
    assert_eq!(
        animations.animations()[1].value(),
        UiAnimationValue::Translation {
            offset: (240.0, -4.0)
        }
    );

    let frames = UiFramePlan::from_tree(&tree)?.resolve(&tree)?;
    let layout = UiLayoutPlan::from_tree(&tree)?;
    let regions = UiRegionStatePlan::resolve(&tree, &layout)?;
    let scripts = UiScriptPlan::from_tree(&tree, bundle.lua())?;
    let templates = UiRuntimeTemplatePlan::from_catalog(&objects, &fonts, bundle.lua())?;
    let textures = UiTexturePlan::from_tree(&tree)?;
    let texture_states = UiTextureStatePlan::resolve(&tree, &textures)?;
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

    let lua = bundle.lua();
    assert_eq!(lua.globals().get::<String>("ANIMATION_READY")?, "BOUNCE");
    lua.load(
        r#"assert(RootCallOut.pulseGroup:GetParent() == RootCallOut)
assert(RootCallOut.pulseGroup.pulser == RootCallOutPulser)
assert(RootCallOutPulser:GetAnimationGroup() == RootCallOut.pulseGroup)
assert(RootCallOutPulser:GetChange() == -.7)
assert(RootCallOut.pulseGroup:GetDuration() == 1.6)
local alpha, translation = RootCallOut.pulseGroup:GetAnimations()
assert(alpha:GetObjectType() == "Alpha")
assert(translation:GetObjectType() == "Translation")
assert(not RootCallOutPulser:IsPlaying())
RootCallOut.pulseGroup:Finish()"#,
    )
    .exec()?;
    assert_eq!(
        lua.globals().get::<String>("ANIMATION_FINISHED")?,
        "AnimationGroup"
    );
    Ok(())
}

/// A later-client animation primitive does not silently acquire invented behavior.
#[test]
fn animation_plan_rejects_unimplemented_primitive_types() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\FrameXML\\FrameXML.toc",
            bytes: b"Animations.xml\n",
        },
        FixtureFile {
            path: "Interface\\FrameXML\\Animations.xml",
            bytes: br#"<Ui><Frame name="Root"><Animations><AnimationGroup>
  <Scale scaleX="2" scaleY="2" duration="1"/>
</AnimationGroup></Animations></Frame></Ui>"#,
        },
    ])?;
    let mut store = mount(&fixture)?;
    let bundle = UiBundle::load(&mut store, UiManifestKind::Frame)?;
    let fonts = FontCatalog::from_bundle(&bundle)?;
    let objects = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
    let tree = UiObjectTree::from_catalog(&objects, &fonts)?;

    let error = match UiAnimationPlan::from_tree(&tree) {
        Ok(_) => return Err("later-client Scale animation was accepted".into()),
        Err(error) => error,
    };

    assert!(error.to_string().contains("<Scale>"));
    Ok(())
}

/// Frame-clock animation output reaches the owner and its complete child tree.
#[test]
fn animation_groups_apply_parallel_bands_to_live_geometry() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Animations.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Animations.xml",
            bytes: br#"<Ui><Frame name="Root">
  <Size x="100" y="100"/><Anchors><Anchor point="BOTTOMLEFT" x="100" y="100"/></Anchors>
  <Animations><AnimationGroup parentKey="motion">
    <Alpha change="-.5" duration=".5" smoothing="IN"/>
    <Translation offsetX="40" offsetY="20" duration=".5" order="2"/>
    <Scripts>
      <OnLoad>self:Play()</OnLoad>
      <OnFinished>ANIMATION_FINISHED = true</OnFinished>
    </Scripts>
  </AnimationGroup></Animations>
  <Scripts><OnLoad>self:SetAlpha(.8)</OnLoad></Scripts>
  <Frames><Frame name="$parentChild"><Size x="20" y="20"/>
    <Anchors><Anchor point="CENTER"/></Anchors>
  </Frame></Frames>
</Frame>
<Frame name="Fade" alpha="0"><Size x="20" y="20"/>
  <Layers><Layer><Texture name="$parentTexture" file="Interface\Glues\Fade"/></Layer></Layers>
  <Animations><AnimationGroup parentKey="fade"><Alpha change=".5" duration="1"/>
    <Scripts><OnLoad>self:Play()</OnLoad></Scripts>
  </AnimationGroup></Animations>
</Frame></Ui>"#,
        },
    ])?;
    let mut manager = GlueManager::start(mount(&fixture)?, (1024, 768), false)?;
    let root = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("Root"))
        .ok_or("Root fixture frame is absent")?;
    let child = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("RootChild"))
        .ok_or("RootChild fixture frame is absent")?;
    let fade_texture = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("FadeTexture"))
        .ok_or("FadeTexture fixture texture is absent")?;
    let initial_member_count = manager.presentation().member_count();
    assert!(
        manager
            .presentation()
            .members_in_draw_order()
            .iter()
            .any(|member| member.object_index() == fade_texture)
    );

    assert!(manager.update(0.25)?);
    assert_eq!(manager.presentation().member_count(), initial_member_count);
    let root_geometry = manager
        .geometry()
        .region(root)
        .ok_or("Root geometry is absent")?;
    assert_close(root_geometry.effective_alpha(), 0.653_553_390_593_273_7);
    assert_bounds(
        root_geometry.presentation_bounds(),
        [100.0, 100.0, 200.0, 200.0],
    );

    assert!(manager.update(0.5)?);
    let root_geometry = manager
        .geometry()
        .region(root)
        .ok_or("Root geometry is absent")?;
    let child_geometry = manager
        .geometry()
        .region(child)
        .ok_or("RootChild geometry is absent")?;
    assert_close(root_geometry.effective_alpha(), 0.3);
    assert_bounds(
        root_geometry.presentation_bounds(),
        [120.0, 110.0, 220.0, 210.0],
    );
    assert_close(child_geometry.effective_alpha(), 0.3);
    assert_bounds(
        child_geometry.presentation_bounds(),
        [160.0, 150.0, 180.0, 170.0],
    );

    assert!(manager.update(0.25)?);
    let root_geometry = manager
        .geometry()
        .region(root)
        .ok_or("Root geometry is absent")?;
    assert_close(root_geometry.effective_alpha(), 0.8);
    assert_bounds(
        root_geometry.presentation_bounds(),
        [100.0, 100.0, 200.0, 200.0],
    );
    assert!(
        manager
            .bundle()
            .lua()
            .globals()
            .get::<bool>("ANIMATION_FINISHED")?
    );
    manager
        .bundle()
        .lua()
        .load("assert(Root.motion:IsDone())")
        .exec()?;
    Ok(())
}

fn assert_bounds(bounds: solarity_ui::UiScreenRect, expected: [f64; 4]) {
    for (actual, expected) in [bounds.left(), bounds.bottom(), bounds.right(), bounds.top()]
        .into_iter()
        .zip(expected)
    {
        assert_close(actual, expected);
    }
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 0.000_01,
        "actual={actual} expected={expected}"
    );
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
