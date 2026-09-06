//! Real archive movement callbacks reach the software mixer through live routing.

use super::super::{RuntimeSoundLoader, SoundPolicy};
use super::*;
use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
use solarity_cpu::{CpuExecutor, CpuPoolConfig};
use solarity_ecs::{
    UnitAnimationTier, UnitPresentation, UnitSheathState, WorldBootstrap, WorldMapId,
    WorldMovementContext, WorldMovementSpeeds, WorldMovementState,
};
use solarity_media::{
    AdvancedSoundService, OwnedSoundEngine, SoundCategory, SoundOutputTarget,
    SoundSoftwareChannelCount,
};
use solarity_rendering::WorldCamera;
use std::cell::Cell;
use std::error::Error;
use std::num::NonZeroUsize;
use std::time::Instant;

struct Cvars {
    footsteps: Cell<bool>,
    armor: Cell<bool>,
}

impl SoundCvarSource for Cvars {
    fn sound_cvar(&self, name: &str) -> Option<String> {
        Some(match name {
            "FootstepSounds" => u8::from(self.footsteps.get()).to_string(),
            "Sound_EnableArmorFoleySoundForSelf" | "Sound_EnableArmorFoleySoundForOthers" => {
                u8::from(self.armor.get()).to_string()
            }
            "Sound_MaxCacheableSizeInBytes" => "2097152".into(),
            "Sound_MaxCacheSizeInBytes" => "16777216".into(),
            _ => "1".into(),
        })
    }
}

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with the user's 3.3.5a archives"]
fn stock_movement_callbacks_produce_audio_and_obey_live_admission() -> Result<(), Box<dyn Error>> {
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(root)?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog.clone())?;
    let creatures = CreatureCatalog::load(&mut store)?;
    let races = solarity_asset::CharacterRaceCatalog::load(&mut store)?;
    let display_id = races.race(10).ok_or("Blood Elf race")?.male_display_id();
    let items = ItemDefinitionCatalog::load(&mut store)?;
    let movement_sounds = MovementSoundCatalog::load(&mut store)?;
    assert_eq!(movement_sounds.footstep(8, 1, false), 15168);
    assert_eq!(movement_sounds.footstep(8, 1, true), 1063);
    assert_eq!(
        movement_sounds.footstep(8, u32::MAX, false),
        movement_sounds.footstep(8, 0, false)
    );
    assert_eq!(movement_sounds.armor(1), 0);
    let entries = solarity_asset::SoundEntryCatalog::load(&mut store)?;
    for (material, id, name) in [
        (5, 1005, "FoleySoundChain"),
        (6, 1004, "FoleySoundPlate"),
        (8, 1003, "FoleySoundLeather"),
    ] {
        assert_eq!(movement_sounds.armor(material), id);
        let entry = entries.entry(id).ok_or("material foley sound entry")?;
        assert_eq!(entry.internal_name(), name);
        assert!(entry.assets().iter().all(|asset| {
            asset
                .path()
                .as_str()
                .starts_with("SOUND\\ITEM\\FOLEYSOUNDS\\")
        }));
    }
    let cvars = Cvars {
        footsteps: Cell::new(true),
        armor: Cell::new(false),
    };
    let engine = OwnedSoundEngine::load(
        &mut store,
        SoundOutputTarget::Memory,
        SoundSoftwareChannelCount::new(64),
        SoundPolicy::read(&cvars)?.settings,
    )?;
    let mut sound = RuntimeSoundCoordinator {
        assets: AssetStoreHandle::new(store),
        engine,
        loader: RuntimeSoundLoader::new(catalog),
        advanced: AdvancedSoundService::new(),
        glue_music: None,
        glue_ambience: None,
        resident_tile: None,
        staged_emitters: None,
        last_update: Instant::now(),
        movement_sounds,
        movement_events: Default::default(),
        movement_loads: Vec::new(),
        movement_voices: Vec::new(),
    };
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(NonZeroUsize::MIN, NonZeroUsize::MIN))?;
    let display = creatures
        .display(display_id)
        .ok_or("Blood Elf male display")?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(530),
        1,
        "Audio",
        Vec3::ZERO,
        0.,
    ));
    let player = world.local_player();
    world
        .storage_mut()
        .add_component(player, (ObjectKind::Player,));
    let chest = (1..60_000)
        .filter_map(|entry| items.item(entry))
        .find(|item| {
            matches!(
                item.inventory_type(),
                solarity_asset::InventoryType::Chest | solarity_asset::InventoryType::Robe
            ) && sound.movement_sounds.armor(item.material_id() as u32) != 0
        })
        .ok_or("authored chest armor foley")?;
    let mut equipment = PlayerEquipment::default().items();
    equipment[PlayerEquipmentSlot::Chest.index()] =
        solarity_ecs::VisibleEquipmentItem::new(chest.id(), 0);
    world
        .storage_mut()
        .add_component(player, (PlayerEquipment::new(equipment),));
    world.storage_mut().add_component(
        player,
        (UnitPresentation::new(
            display.id(),
            display.id(),
            0,
            0,
            UnitAnimationTier::Ground,
            UnitSheathState::Unarmed,
        ),),
    );
    let movement = WorldMovementState::new(
        0,
        WorldMovementSpeeds::new([2.5, 7., 4.5, 4.72, 2.5, 7., 4.5, 3., 3.]),
        WorldMovementContext::default(),
    );
    world.update_movement(1, movement)?;
    let camera = WorldCamera::stock(Vec3::new(-5., 0., 2.), Vec3::Z, Vec3::Z, 100.).frame(1.)?;
    let mut random = BlizzardRand::new(1);
    let source = UnitSoundContext {
        world: &world,
        creatures: &creatures,
        items: &items,
        cvars: &cvars,
    }
    .source(1, &sound.movement_sounds)
    .ok_or("unit sound source")?;
    let sounds = source.sounds.ok_or("creature sound data")?;
    // The ordinary Blood Elf row authors no jump/land vocal. Silence remains
    // intentional; creatures/vehicles with those fields exercise the cue path.
    assert_eq!(sounds.jump(), 0);
    assert_eq!(sounds.land(), 0);
    let vocal_display = creatures
        .displays()
        .iter()
        .find(|display| {
            sound
                .movement_sounds
                .creature(display.sound_id())
                .or_else(|| {
                    creatures
                        .model(display.model_id())
                        .and_then(|model| sound.movement_sounds.creature(model.sound_id()))
                })
                .is_some_and(|sounds| sounds.jump() != 0 && sounds.land() != 0)
        })
        .ok_or("authored jump/land display")?
        .id();
    let event = RuntimeM2Event::new(*b"$FSD", 0, Vec3::ZERO, Some(1));
    for (label, notification, footstep, wet, armor) in [
        (
            "jump",
            Some(UnitMovementAnimationEventKind::Jump),
            false,
            false,
            false,
        ),
        (
            "land",
            Some(UnitMovementAnimationEventKind::Land {
                previous_flags: 0x1000,
                forced: true,
                slow: true,
            }),
            false,
            false,
            false,
        ),
        ("dry footstep", None, true, false, false),
        ("wet footstep", None, true, true, false),
        ("armor foley", None, true, false, true),
    ] {
        cvars.footsteps.set(!armor);
        cvars.armor.set(armor);
        let selected_display = if footstep {
            display.id()
        } else {
            vocal_display
        };
        world.storage_mut().add_component(
            player,
            (UnitPresentation::new(
                selected_display,
                selected_display,
                0,
                0,
                UnitAnimationTier::Ground,
                UnitSheathState::Unarmed,
            ),),
        );
        sound
            .engine
            .with_engine_mut(|engine| engine.stop_category(SoundCategory::Sfx))?;
        if let Some(kind) = notification {
            sound.notify_unit_movement(UnitMovementAnimationEvent {
                identity: world.object_identity(1).ok_or("unit identity")?,
                movement,
                stand: 0,
                kind,
            });
        }
        let events = if footstep {
            std::slice::from_ref(&event)
        } else {
            &[]
        };
        sound.play_unit_events(
            events,
            camera,
            UnitSoundContext {
                world: &world,
                creatures: &creatures,
                items: &items,
                cvars: &cvars,
            },
            |_, _, _| Ok((0, wet)),
            &mut random,
        )?;
        assert_eq!(
            sound
                .engine
                .with_engine(|engine| engine.active_voice_count()),
            0,
            "{label}: payload played synchronously"
        );
        finish_loads(&mut sound, &cpu)?;
        assert_eq!(
            sound
                .engine
                .with_engine(|engine| engine.active_voice_count()),
            1,
            "{label}"
        );
        let mut audible = false;
        for _ in 0..32 {
            let mut samples = [0_u8; 4096];
            sound
                .engine
                .with_engine(|engine| engine.generate(&mut samples))?;
            audible |= samples.iter().any(|sample| *sample != 0);
        }
        assert!(audible, "{label}: mixer remained silent");
        assert!(sound.movement_events.is_empty());
        println!("{label}: decoded and mixed original archive audio");
    }
    sound
        .engine
        .with_engine_mut(|engine| engine.stop_category(SoundCategory::Sfx))?;
    cvars.armor.set(false);
    cvars.footsteps.set(false);
    sound.play_unit_events(
        &[event],
        camera,
        UnitSoundContext {
            world: &world,
            creatures: &creatures,
            items: &items,
            cvars: &cvars,
        },
        |_, _, _| {
            Err(RuntimeSoundError::MissingCVar {
                name: "disabled surface must not be queried",
            })
        },
        &mut random,
    )?;
    assert_eq!(
        sound
            .engine
            .with_engine(|engine| engine.active_voice_count()),
        0
    );
    cvars.footsteps.set(true);
    for (field, flag) in [(74, 0x0002_0000), (150, 0x10)] {
        world.update_fields(1, [(field, flag)])?;
        sound.play_unit_events(
            &[event],
            camera,
            UnitSoundContext {
                world: &world,
                creatures: &creatures,
                items: &items,
                cvars: &cvars,
            },
            |_, _, _| {
                Err(RuntimeSoundError::MissingCVar {
                    name: "suppressed unit must not query surface",
                })
            },
            &mut random,
        )?;
        assert_eq!(
            sound
                .engine
                .with_engine(|engine| engine.active_voice_count()),
            0
        );
        world.update_fields(1, [(field, 0)])?;
    }
    sound.play_unit_events(
        &[event; 20],
        camera,
        UnitSoundContext {
            world: &world,
            creatures: &creatures,
            items: &items,
            cvars: &cvars,
        },
        |_, _, _| Ok((0, false)),
        &mut random,
    )?;
    assert!(sound.movement_loads.len() <= 4);
    finish_loads(&mut sound, &cpu)?;
    assert!(
        sound
            .engine
            .with_engine(|engine| engine.active_voice_count())
            <= 4
    );
    assert_eq!(
        UnitSoundOptions {
            local: true,
            footstep: true
        }
        .request(1)
        .channel()
        .value(),
        17
    );
    assert_eq!(
        UnitSoundOptions {
            local: false,
            footstep: true
        }
        .request(1)
        .channel()
        .value(),
        13
    );
    sound.disconnect()?;
    assert_eq!(
        sound
            .engine
            .with_engine(|engine| engine.active_voice_count()),
        0
    );
    sound.play_unit_events(
        &[event],
        camera,
        UnitSoundContext {
            world: &world,
            creatures: &creatures,
            items: &items,
            cvars: &cvars,
        },
        |_, _, _| Ok((0, false)),
        &mut random,
    )?;
    assert_eq!(sound.movement_loads.len(), 1);
    sound.disconnect()?;
    assert!(sound.movement_loads.is_empty());
    sound.shutdown()?;
    cpu.shutdown()?;
    Ok(())
}

fn finish_loads(
    sound: &mut RuntimeSoundCoordinator,
    cpu: &CpuExecutor,
) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + std::time::Duration::from_secs(20);
    while !sound.movement_loads.is_empty() {
        sound.poll_loads(cpu)?;
        if Instant::now() > deadline {
            return Err("movement sound worker timeout".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    Ok(())
}
