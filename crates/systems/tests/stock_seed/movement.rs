//! External stock-compatibility tests for `systems/movement` belong here.

use std::error::Error;

use solarity_asset::{AnimationDataCatalog, ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ecs::{UnitAnimationTier, WorldMovementSpeeds, WorldMovementState};
use solarity_systems::{resolve_unit_locomotion_animation, resolve_unit_model_animation};

use crate::support::{Fixture, FixtureFile};

const SPEEDS: WorldMovementSpeeds =
    WorldMovementSpeeds::new([2.5, 7.0, 4.5, 4.72, 2.5, 7.0, 4.5, 3.0, 3.0]);

fn animation(flags: u64) -> u16 {
    resolve_unit_locomotion_animation(WorldMovementState::new(flags, SPEEDS)).animation_id()
}

/// The recovered selector preserves stock's common ground locomotion IDs.
#[test]
fn ground_locomotion_uses_stock_selector_order() {
    assert_eq!(animation(0), 0);
    assert_eq!(animation(0x0000_0000_0001), 5);
    assert_eq!(animation(0x0000_0000_0101), 4);
    assert_eq!(animation(0x0000_0000_0002), 13);
    assert_eq!(animation(0x0000_0000_1001), 40);
}

/// Swimming and flying share the base 41--45 family before tier remapping.
#[test]
fn aquatic_locomotion_preserves_direction_precedence() {
    for environment in [0x0000_0020_0000, 0x0000_0200_0000] {
        assert_eq!(animation(environment), 41);
        assert_eq!(animation(environment | 0x1), 42);
        assert_eq!(animation(environment | 0x2), 45);
        assert_eq!(animation(environment | 0x4), 43);
        assert_eq!(animation(environment | 0x8), 44);
        assert_eq!(animation(environment | 0xC), 43);
    }
}

/// Tier mapping precedes fallback traversal, then descends through tier parents.
#[test]
fn model_animation_resolution_follows_stock_tiers_and_fallbacks() -> Result<(), Box<dyn Error>> {
    let table = animation_data_fixture();
    let fixture = Fixture::new(&[FixtureFile {
        path: "DBFilesClient\\AnimationData.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let catalog = AnimationDataCatalog::load(&mut store)?;
    let run = resolve_unit_locomotion_animation(WorldMovementState::new(1, SPEEDS));

    let fly = resolve_unit_model_animation(&catalog, run, UnitAnimationTier::Fly, |id| id == 234)
        .ok_or("flying run did not resolve")?;
    assert_eq!(fly.animation_id(), 234);

    let hover =
        resolve_unit_model_animation(&catalog, run, UnitAnimationTier::Hover, |id| id == 234)
            .ok_or("hover did not descend to the flying tier")?;
    assert_eq!(hover.animation_id(), 234);

    let fly_stand =
        resolve_unit_model_animation(&catalog, run, UnitAnimationTier::Fly, |id| id == 229)
            .ok_or("flying fallback did not reach tiered Stand")?;
    assert_eq!(fly_stand.animation_id(), 229);

    let ground = resolve_unit_model_animation(&catalog, run, UnitAnimationTier::Fly, |id| id == 5)
        .ok_or("flying tier did not descend to ground")?;
    assert_eq!(ground.animation_id(), 5);
    Ok(())
}

/// Cyclic malformed fallback metadata terminates without inventing a sequence.
#[test]
fn model_animation_resolution_guards_fallback_cycles() -> Result<(), Box<dyn Error>> {
    let mut table = animation_data_fixture();
    // Rows 4 and 5 begin after the header and their preceding 32-byte rows.
    let walk_fallback_offset = 20 + 32 + 5 * 4;
    table[walk_fallback_offset..walk_fallback_offset + 4].copy_from_slice(&5_u32.to_le_bytes());
    let run_fallback_offset = 20 + 2 * 32 + 5 * 4;
    table[run_fallback_offset..run_fallback_offset + 4].copy_from_slice(&4_u32.to_le_bytes());
    let fixture = Fixture::new(&[FixtureFile {
        path: "DBFilesClient\\AnimationData.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let catalog = AnimationDataCatalog::load(&mut store)?;
    let run = resolve_unit_locomotion_animation(WorldMovementState::new(1, SPEEDS));

    assert_eq!(
        resolve_unit_model_animation(&catalog, run, UnitAnimationTier::Ground, |_| false),
        None
    );
    Ok(())
}

/// Builds the behavior subset needed to exercise ground and flying resolution.
fn animation_data_fixture() -> Vec<u8> {
    let fields: [u32; 48] = [
        // ID, name, weapon, body, flags, fallback, behavior, tier.
        0, 0, 0, 1, 3, 0, 0, 0, 4, 0, 0, 0x41, 6, 0, 4, 0, 5, 0, 0, 0x41, 6, 0, 5, 0, 229, 0, 0, 1,
        3, 229, 0, 3, 233, 0, 0, 0x41, 6, 229, 4, 3, 234, 0, 0, 0x41, 6, 229, 5, 3,
    ];
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + 1);
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&6_u32.to_le_bytes());
    bytes.extend_from_slice(&8_u32.to_le_bytes());
    bytes.extend_from_slice(&32_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.push(0);
    bytes
}
