//! World root scaling reaches layout, input and synchronous Lua notifications.

use crate::support::{Fixture, FixtureFile};
use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
use solarity_ui::{
    AddonCatalog, FrameManager, GlueManager, UiEventArgument, UiEventPayload, UiPointerButton,
    UiScriptEnvironment,
};
use std::error::Error;

#[test]
fn world_scale_updates_anchors_hit_testing_and_nested_events() -> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let mut manager = frame(&fixture, &[])?;
    let root = (0..manager.geometry().region_count())
        .find(|index| manager.object_name(*index) == Some("UIParent"))
        .ok_or("root")?;
    let button = (0..manager.geometry().region_count())
        .find(|index| manager.object_name(*index) == Some("Action"))
        .ok_or("button")?;
    let mut previous_left = None;
    for (command, scale) in [
        ("", 0.9_f32),
        ("enable", 0.8),
        ("smaller", 0.64),
        ("disable", 0.9),
    ] {
        if !command.is_empty() {
            manager.dispatch_event(
                "PLAYER_LOGIN",
                &UiEventPayload::new([UiEventArgument::String(command.into())]),
            )?;
        }
        let root_bounds = manager
            .geometry()
            .region(root)
            .ok_or("root bounds")?
            .presentation_bounds();
        assert_eq!(
            [
                root_bounds.left(),
                root_bounds.bottom(),
                root_bounds.right(),
                root_bounds.top()
            ],
            [0., 0., 768. * 2560. / 1440., 768.]
        );
        let bounds = manager
            .geometry()
            .region(button)
            .ok_or("button bounds")?
            .presentation_bounds();
        let expected_left = root_bounds.right() - 110. * f64::from(scale);
        assert!((bounds.left() - expected_left).abs() < 0.0001);
        assert!((bounds.width() - 100. * f64::from(scale)).abs() < 0.0001);
        assert!((bounds.height() - 40. * f64::from(scale)).abs() < 0.0001);
        let center = (
            (bounds.left() + bounds.right()) / 2.,
            (bounds.bottom() + bounds.top()) / 2.,
        );
        let left = bounds.left();
        if let Some(previous) = previous_left.filter(|previous| *previous + 1. < left) {
            let outside = (previous + 1., center.1);
            assert_eq!(
                manager
                    .pointer_button(outside, UiPointerButton::Left, true)?
                    .object_index(),
                None
            );
            manager.pointer_button(outside, UiPointerButton::Left, false)?;
        }
        manager.pointer_motion(center)?;
        assert_eq!(
            manager
                .pointer_button(center, UiPointerButton::Left, true)?
                .object_index(),
            Some(button)
        );
        manager.pointer_button(center, UiPointerButton::Left, false)?;
        previous_left = Some(left);
    }
    manager.dispatch_event(
        "PLAYER_LOGIN",
        &UiEventPayload::new([UiEventArgument::String("clicked".into())]),
    )?;
    Ok(())
}

#[test]
fn world_scale_loads_profile_without_rescaling_glue() -> Result<(), Box<dyn Error>> {
    let fixture = fixture()?;
    let manager = frame(
        &fixture,
        &[
            ("useUiScale".into(), "1".into()),
            ("uiScale".into(), "0.75".into()),
        ],
    )?;
    let root = (0..manager.geometry().region_count())
        .find(|index| manager.object_name(*index) == Some("UIParent"))
        .ok_or("root")?;
    assert_eq!(
        manager
            .geometry()
            .region(root)
            .ok_or("root")?
            .effective_scale(),
        0.75
    );
    assert!(manager.take_changed_cvars().is_empty());
    let glue = GlueManager::start(
        AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?,
        (2560, 1440),
        false,
    )?;
    assert_eq!(
        glue.geometry()
            .region(0)
            .ok_or("Glue root")?
            .effective_scale(),
        1.0
    );
    Ok(())
}

fn frame(fixture: &Fixture, cvars: &[(String, String)]) -> Result<FrameManager, Box<dyn Error>> {
    let archive =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    Ok(FrameManager::start_shared(
        AssetStoreHandle::new(AssetStore::mount(archive)?),
        UiScriptEnvironment::new(2560, 1440, false)?,
        cvars,
        &AddonCatalog::default(),
    )?)
}

fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let table = |records: u32, fields: u32| {
        let mut bytes = b"WDBC".to_vec();
        for value in [records, fields, fields * 4, 1] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.resize(21 + records as usize * fields as usize * 4, 0);
        bytes
    };
    let base = table(11, 1);
    let coefficients = table(1100, 1);
    let slots = table(0, 3);
    let mut files = vec![
        FixtureFile {
            path: "DBFilesClient/gtChanceToMeleeCritBase.dbc",
            bytes: &base,
        },
        FixtureFile {
            path: "DBFilesClient/gtChanceToSpellCritBase.dbc",
            bytes: &base,
        },
        FixtureFile {
            path: "DBFilesClient/PaperDollItemFrame.dbc",
            bytes: &slots,
        },
        FixtureFile {
            path: "Interface/FrameXML/FrameXML.toc",
            bytes: b"Scale.xml\n",
        },
        FixtureFile {
            path: "Interface/FrameXML/Bindings.xml",
            bytes: b"<Bindings/>",
        },
        FixtureFile {
            path: "WTF/DefaultBindings.wtf",
            bytes: b"",
        },
        FixtureFile {
            path: "Interface/GlueXML/GlueXML.toc",
            bytes: b"Scale.xml\n",
        },
        FixtureFile {
            path: "Interface/GlueXML/Scale.xml",
            bytes: br#"<Ui><Frame name="UIParent" setAllPoints="true"><Scripts><OnLoad>
SetCVar('uiScale', 0.7); SetCVar('useUiScale', 1)
assert(self:GetScale() == 1 and GetScreenHeight() == 768)
</OnLoad></Scripts></Frame></Ui>"#,
        },
        FixtureFile {
            path: "Interface/FrameXML/Scale.xml",
            bytes: br#"<Ui>
<Frame name="UIParent" setAllPoints="true"><Scripts>
<OnLoad>
Clicks = 0; ScaleEvents = 0
self:RegisterEvent('DISPLAY_SIZE_CHANGED'); self:RegisterEvent('PLAYER_LOGIN')
</OnLoad><OnEvent>
if event == 'DISPLAY_SIZE_CHANGED' then
 ScaleEvents = ScaleEvents + 1
 CachedUse = GetCVar('useUiScale'); CachedScale = GetCVar('uiScale')
 assert(arg1 == nil)
 assert(math.abs(GetScreenHeight() * self:GetEffectiveScale() - 768) &lt; 0.0001)
 assert(math.abs(GetScreenWidth() - self:GetWidth()) &lt; 0.0001)
elseif event == 'PLAYER_LOGIN' then
 local command = arg1
 if command == 'enable' then
  SetCVar('UiScAlE', 0.8); assert(ScaleEvents == 1)
  SetCVar('useUiScale', 1); assert(ScaleEvents == 2 and CachedUse == '0')
 elseif command == 'smaller' then
  SetCVar('uiScale', 0.5); assert(ScaleEvents == 3 and CachedScale == '0.8')
 elseif command == 'disable' then
  SetCVar('useUiScale', 0); assert(ScaleEvents == 4 and CachedUse == '1')
 elseif command == 'clicked' then assert(Clicks == 4) end
 assert(event == 'PLAYER_LOGIN' and arg1 == command)
end
</OnEvent></Scripts><Frames>
<PlayerModel name="PaperDoll" hidden="true"><Scripts>
<OnLoad>self:RegisterEvent('DISPLAY_SIZE_CHANGED')</OnLoad>
<OnEvent>self:RefreshUnit()</OnEvent>
</Scripts></PlayerModel>
<Button name="Action" enableMouse="true"><Size x="100" y="40"/>
<Anchors><Anchor point="BOTTOMRIGHT"><Offset x="-10" y="20"/></Anchor></Anchors>
<Layers><Layer><Texture setAllPoints="true"><Color r="1" g="0" b="0"/></Texture></Layer></Layers>
<Scripts><OnClick>Clicks = Clicks + 1</OnClick></Scripts></Button>
</Frames></Frame></Ui>"#,
        },
    ];
    for path in [
        "DBFilesClient/gtChanceToMeleeCrit.dbc",
        "DBFilesClient/gtChanceToSpellCrit.dbc",
        "DBFilesClient/gtOCTRegenHP.dbc",
        "DBFilesClient/gtRegenHPPerSpt.dbc",
        "DBFilesClient/gtRegenMPPerSpt.dbc",
    ] {
        files.push(FixtureFile {
            path,
            bytes: &coefficients,
        });
    }
    Fixture::new(&files)
}
