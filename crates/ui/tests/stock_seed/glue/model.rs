//! Model file and instance contracts recovered from build 12340's Lua bridge.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{GlueManager, UiEventPayload, UiModelAction};

use crate::support::{Fixture, FixtureFile};

/// Executes inside a registered callback so the normal mutation publisher runs.
fn action(manager: &mut GlueManager, source: &str) -> Result<(), Box<dyn Error>> {
    let lua = manager.bundle().lua();
    lua.globals()
        .set("MODEL_ACTION", lua.load(source).into_function()?)?;
    manager.dispatch_event("SET_GLUE_SCREEN", &UiEventPayload::default())?;
    Ok(())
}

/// Stock queues every call against its current instance, including hidden models.
#[test]
fn model_sequence_actions_preserve_order_restarts_and_native_integer_conversion()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Model.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Model.xml",
            bytes: br#"<Ui>
<Model name="Probe"><Size x="200" y="100"/><Anchors><Anchor point="CENTER"/></Anchors><Scripts>
<OnLoad>self:RegisterEvent("SET_GLUE_SCREEN")</OnLoad>
<OnEvent>if MODEL_ACTION then MODEL_ACTION() end</OnEvent>
</Scripts></Model></Ui>"#,
        },
        FixtureFile {
            path: "Solarity\\Backdrop.m2",
            bytes: b"deferred model bytes",
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1280, 720), false)?;
    action(
        &mut manager,
        r#"
Probe:SetSequence(7)
Probe:SetSequenceTime(7, 10)
Probe:SetModel("Solarity\\Backdrop.m2")
Probe:Hide()
Probe:SetSequence("7.9")
Probe:SetSequence(7)
Probe:SetSequenceTime(7, "250.9")
Probe:SetSequenceTime(7, 250.9)
Probe:SetSequenceTime(7, -250.9)
Probe:SetSequence(-0.5)
Probe:SetSequence(505.9)
Probe:SetSequence(4294967303)
Probe:SetSequence(9223372036854775808)
assert(not pcall(function() Probe:SetSequence(-1) end))
assert(not pcall(function() Probe:SetSequence(506) end))
assert(not pcall(function() Probe:SetSequence({}) end))
Probe:SetSequenceTime(-1, 2147483648)
Probe:SetSequenceTime(65536, -2147483649)
Probe:SetSequence(7)
Probe:ClearModel()
Probe:SetSequence(7)
Probe:SetModel("Solarity\\Backdrop.m2")
Probe:SetSequence(7)
"#,
    )?;
    let Some(UiModelAction::Assign {
        instance: first,
        path: Some(path),
    }) = manager.take_model_action()
    else {
        return Err("missing first assignment".into());
    };
    assert_eq!(path.as_str(), "SOLARITY\\BACKDROP.M2");
    let expected = [
        (7, 0),
        (7, 0),
        (7, 250),
        (7, 250),
        (7, -250),
        (0, 0),
        (505, 0),
        (7, 0),
        (0, 0),
        (u32::MAX, i32::MIN),
        (65_536, i32::MIN),
        (7, 0),
    ];
    for (animation_id, time_offset_ms) in expected {
        assert_eq!(
            manager.take_model_action(),
            Some(UiModelAction::Sequence {
                instance: first,
                animation_id,
                time_offset_ms
            })
        );
    }
    let Some(UiModelAction::Assign {
        instance: cleared,
        path: None,
    }) = manager.take_model_action()
    else {
        return Err("missing clear".into());
    };
    let Some(UiModelAction::Assign {
        instance: second,
        path: Some(_),
    }) = manager.take_model_action()
    else {
        return Err("missing replacement".into());
    };
    assert_eq!(first.object_index(), second.object_index());
    assert_ne!(first.generation(), cleared.generation());
    assert_ne!(cleared.generation(), second.generation());
    assert_eq!(
        manager.take_model_action(),
        Some(UiModelAction::Sequence {
            instance: second,
            animation_id: 7,
            time_offset_ms: 0
        })
    );
    assert_eq!(manager.take_model_action(), None);
    Ok(())
}

/// 0x0095F990 creates an instance on every SetModel; 0x00960620 assigns null.
/// The shared resource lookup normalizes MDL/MDX and returns ErrorCube on a miss.
/// ModelFFX clearing deliberately avoids its apparent native null dereference.
#[test]
fn model_replacement_aliases_and_clear_reach_retained_presentation() -> Result<(), Box<dyn Error>> {
    for kind in ["Model", "ModelFFX"] {
        let xml = format!(
            r#"<Ui>
<Frame name="ModelOwner"><Scripts>
<OnLoad>self:RegisterEvent("SET_GLUE_SCREEN")</OnLoad>
<OnEvent>if MODEL_ACTION then MODEL_ACTION() end</OnEvent>
</Scripts><Frames>
<{kind} name="Probe"><Size x="200" y="100"/><Anchors><Anchor point="CENTER"/></Anchors></{kind}>
</Frames></Frame></Ui>"#
        );
        // Stock queues the byte read after its file-presence check. These
        // markers exercise that script boundary without a renderer/decoder.
        let fixture = Fixture::new(&[
            FixtureFile {
                path: "Interface\\GlueXML\\GlueXML.toc",
                bytes: b"Model.xml\n",
            },
            FixtureFile {
                path: "Interface\\GlueXML\\Model.xml",
                bytes: xml.as_bytes(),
            },
            FixtureFile {
                path: "Solarity\\Backdrop.m2",
                bytes: b"deferred model bytes",
            },
            FixtureFile {
                path: "Spells\\ErrorCube.m2",
                bytes: b"deferred error cube bytes",
            },
        ])?;
        let catalog =
            ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
        let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1280, 720), false)?;
        let object_index = manager
            .objects()
            .iter()
            .position(|object| object.name() == Some("Probe"))
            .ok_or("missing model widget")?;
        assert!(!manager.presentation().model_was_cleared(object_index));
        action(
            &mut manager,
            r#"
assert(Probe:GetModel() == Probe)
assert(Probe:GetModel("extra") == "extra")
Probe:SetModel("solarity/backdrop.MDX")
assert(Probe:GetModel() == "solarity\\backdrop.m2")
Probe:SetSequence(7)
Probe:SetModelScale(1.25)
Probe:SetCamera(2)
"#,
        )?;
        let initial = manager
            .presentation()
            .visible_models()
            .next()
            .ok_or("missing first model")?
            .clone();
        assert_eq!(initial.path().as_str(), "SOLARITY\\BACKDROP.M2");
        assert_eq!(initial.sequence(), 7);
        assert_eq!(initial.effective_scale(), 1.0);
        assert_eq!(initial.ui_extent()[1], 768.0);
        action(&mut manager, "ModelOwner:SetScale(0.5)")?;
        let scaled = manager
            .presentation()
            .visible_models()
            .next()
            .ok_or("lost scaled viewport")?;
        assert_eq!(scaled.effective_scale(), 0.5);
        assert_eq!(scaled.bounds().width(), 100.0);
        assert_eq!(scaled.bounds().height(), 50.0);
        assert_eq!(scaled.model_scale(), 1.25);
        action(&mut manager, "ModelOwner:SetScale(1)")?;
        action(&mut manager, r#"Probe:SetModel("Solarity\\Backdrop.mdl")"#)?;
        let replacement = manager
            .presentation()
            .visible_models()
            .next()
            .ok_or("missing replacement model")?;
        assert_eq!(replacement.path(), initial.path());
        assert_ne!(
            replacement.instance_generation(),
            initial.instance_generation()
        );
        assert_eq!(replacement.sequence(), 0);
        assert_eq!(replacement.model_scale(), 1.25);
        assert_eq!(replacement.camera(), 2);
        let generation = replacement.instance_generation();
        action(
            &mut manager,
            r#"assert(not pcall(function() Probe:SetModel({}) end))"#,
        )?;
        assert_eq!(
            manager
                .presentation()
                .visible_models()
                .next()
                .ok_or("lost model after usage error")?
                .instance_generation(),
            generation
        );

        for path in [
            "Solarity/Missing.m2",
            "Solarity/Unsupported.txt",
            "Solarity/NoExtension",
        ] {
            action(&mut manager, &format!("Probe:SetModel({path:?})"))?;
            assert_eq!(
                manager
                    .presentation()
                    .visible_models()
                    .next()
                    .ok_or("missing error cube")?
                    .path()
                    .as_str(),
                "SPELLS\\ERRORCUBE.M2"
            );
        }
        action(
            &mut manager,
            r#"Probe:ClearModel(); assert(Probe:GetModel() == Probe)"#,
        )?;
        assert_eq!(manager.presentation().visible_models().count(), 0);
        assert!(manager.presentation().model_was_cleared(object_index));
        assert!(manager.presentation().has_visible_cleared_model());
        action(&mut manager, "ModelOwner:Hide()")?;
        assert!(!manager.presentation().has_visible_cleared_model());
        action(&mut manager, "ModelOwner:Show()")?;
        assert!(manager.presentation().has_visible_cleared_model());
        action(&mut manager, "Probe:SetAlpha(0)")?;
        assert!(!manager.presentation().has_visible_cleared_model());
        action(&mut manager, "Probe:SetAlpha(1)")?;
        assert!(manager.presentation().has_visible_cleared_model());
        action(&mut manager, r#"Probe:SetModel("Solarity\\Backdrop.m2")"#)?;
        assert!(!manager.presentation().model_was_cleared(object_index));
        assert_eq!(manager.presentation().visible_models().count(), 1);
        action(
            &mut manager,
            r#"
assert(not pcall(function() Probe:SetModel("") end))
assert(Probe:GetModel() == Probe)
"#,
        )?;
        assert!(manager.presentation().has_visible_cleared_model());
        assert_eq!(manager.presentation().visible_models().count(), 0);
    }
    Ok(())
}
