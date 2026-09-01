//! External stock-compatibility tests for retained XML animation objects.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    FontCatalog, UiAnimationKind, UiAnimationLooping, UiAnimationPlan, UiAnimationValue, UiBundle,
    UiFramePlan, UiLayoutPlan, UiManifestKind, UiObjectCatalog, UiObjectTree, UiRegionStatePlan,
    UiRuntimeTemplatePlan, UiScriptEnvironment, UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan,
    UiTexturePlan, UiTextureStatePlan,
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

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    Ok(AssetStore::mount(catalog)?)
}
