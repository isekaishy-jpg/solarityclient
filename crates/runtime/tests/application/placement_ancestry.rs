//! Last-preceding attachment identity survives large scenery populations.

use super::{M2GpuPlacementOwner, ParentBinding, PlacementAncestry, UnitAnimationBehavior};
use crate::application::terrain_coordinator::m2_residency::ResidentM2Owner;
use solarity_rendering::CharacterAttachmentPoint;

/// Whole rebuilds retain the scalar selector's exact preceding-owner semantics.
#[test]
fn rebuilt_ancestry_matches_point_queries_after_removal_and_reordering() {
    let mut placements = placement_fixture(512, 24);
    let mut ancestry = PlacementAncestry::default();
    let mut parents = Vec::new();
    for step in 0..3 {
        if step == 1 {
            placements.retain(|placement| {
                !matches!(placement.owner, M2GpuPlacementOwner::CreatureMount { .. })
            });
        } else if step == 2 {
            placements.rotate_left(514);
        }
        let expected = (0..placements.len())
            .map(|index| super::placement_parent(&placements, index))
            .collect::<Vec<_>>();
        ancestry.rebuild(&placements, &mut parents);
        assert_eq!(parents, expected);
    }
}

/// Manual comparison isolates attachment lookup from live GPU and NPC variation.
#[test]
#[ignore = "manual optimized placement-lookup timing"]
fn profile_resident_placement_ancestry() {
    let placements = placement_fixture(23_000, 300);
    let mut ancestry = PlacementAncestry::default();
    let mut parents = Vec::new();
    let expected = (0..placements.len())
        .map(|index| super::placement_parent(&placements, index))
        .collect::<Vec<_>>();
    ancestry.rebuild(&placements, &mut parents);
    assert_eq!(parents, expected);
    let started = std::time::Instant::now();
    for _ in 0..20 {
        for index in 0..placements.len() {
            std::hint::black_box(super::placement_parent(
                std::hint::black_box(&placements),
                index,
            ));
        }
    }
    let scalar = started.elapsed().as_secs_f64() * 1000.0 / 20.0;
    let started = std::time::Instant::now();
    for _ in 0..20 {
        ancestry.rebuild(std::hint::black_box(&placements), &mut parents);
        std::hint::black_box(&parents);
    }
    let indexed = started.elapsed().as_secs_f64() * 1000.0 / 20.0;
    eprintln!(
        "placement ancestry: resident={} scalar_ms={scalar:.6} indexed_ms={indexed:.6}",
        placements.len()
    );
}

/// Static residency precedes native unit bodies, optional mounts and equipment.
fn placement_fixture(scenery: u32, units: u64) -> Vec<super::M2GpuPlacement> {
    let mut placements = (0..scenery)
        .map(|unique_id| {
            placement(M2GpuPlacementOwner::Static(
                ResidentM2Owner::TerrainDoodad { unique_id },
            ))
        })
        .collect::<Vec<_>>();
    for guid in 1..=units {
        if guid % 4 == 0 {
            placements.push(placement(M2GpuPlacementOwner::CreatureMount { guid }));
        }
        placements.push(placement(M2GpuPlacementOwner::CreatureBody { guid }));
        if guid % 5 == 0 {
            placements.push(placement(M2GpuPlacementOwner::CreatureBody { guid }));
        }
        placements.push(placement(M2GpuPlacementOwner::UnitItem {
            guid,
            point: CharacterAttachmentPoint::HandRight,
        }));
        placements.push(placement(M2GpuPlacementOwner::UnitItemVisual {
            guid,
            item_point: CharacterAttachmentPoint::HandRight,
            effect_point: 0,
        }));
    }
    placements
}

/// Parent selection needs placement identity, not a GPU resource or animation file.
fn placement(owner: M2GpuPlacementOwner) -> super::M2GpuPlacement {
    super::M2GpuPlacement {
        static_spatial: None,
        sound_lifetime: Default::default(),
        light_lifetime: Default::default(),
        placement_valid: true,
        scene_indoor_fog: false,
        world_model_state: None,
        source_index: 0,
        local_transform: glam::Mat4::IDENTITY,
        ground_placement: None,
        scene_registration: None,
        rider_scale: 1.0,
        transform: glam::Mat4::IDENTITY,
        orientation: solarity_rendering::M2ModelOrientation::Authored,
        glue_parent_attachment: None,
        owner,
        flags: 0,
        color: [255; 4],
        entity_lighting: Default::default(),
        opacity: 1.0,
        entity_opacity: None,
        retirement: None,
        particle_colors: None,
        playback: None,
        passenger_playback_advance: None,
        unit_animation: None,
        unit_presentation: None,
        mount_key: None,
        item_identity: None,
        particles: Vec::new(),
        ribbons: Vec::new(),
        last_effect_time_ms: 0,
        unit_effect: None,
    }
}

/// Missing mounts cannot bind to later owners, and newer bodies replace older ones.
#[test]
fn preceding_owner_order_preserves_missing_mounts_and_duplicate_bodies() {
    let mut index = PlacementAncestry::default();
    let mount = M2GpuPlacementOwner::CreatureMount { guid: 7 };
    assert_eq!(index.parent(ParentBinding::Owner(mount)), None);
    for unique_id in 0..50_000 {
        index.insert(
            M2GpuPlacementOwner::Static(ResidentM2Owner::TerrainDoodad { unique_id }),
            None,
            unique_id as usize,
        );
    }
    assert_eq!(index.parent(ParentBinding::Owner(mount)), None);
    index.insert(mount, None, 50_000);
    assert_eq!(index.parent(ParentBinding::Owner(mount)), Some(50_000));
    assert_eq!(
        index.parent(ParentBinding::Owner(M2GpuPlacementOwner::PlayerMount {
            guid: 7
        })),
        None
    );
    index.insert(M2GpuPlacementOwner::CreatureBody { guid: 7 }, None, 50_001);
    index.insert(
        M2GpuPlacementOwner::RemotePlayerBody { guid: 7 },
        None,
        50_002,
    );
    assert_eq!(index.parent(ParentBinding::Body(7)), Some(50_002));
    index.insert(M2GpuPlacementOwner::CreatureBody { guid: 8 }, None, 50_003);
    assert_eq!(index.parent(ParentBinding::Body(7)), Some(50_002));
    index.insert(mount, None, 50_004);
    assert_eq!(index.parent(ParentBinding::Owner(mount)), Some(50_004));
}

/// Item identity and the latest Glue model use separate attachment domains.
#[test]
fn equipment_and_glue_attachments_keep_distinct_parent_rules() {
    let mut index = PlacementAncestry::default();
    let right = M2GpuPlacementOwner::UnitItem {
        guid: 7,
        point: CharacterAttachmentPoint::HandRight,
    };
    let left = M2GpuPlacementOwner::UnitItem {
        guid: 7,
        point: CharacterAttachmentPoint::HandLeft,
    };
    index.insert(right, None, 0);
    index.insert(left, None, 1);
    assert_eq!(index.parent(ParentBinding::Owner(right)), Some(0));
    assert_eq!(index.parent(ParentBinding::Owner(left)), Some(1));
    index.insert(M2GpuPlacementOwner::GlueModel { object_index: 15 }, None, 2);
    index.insert(M2GpuPlacementOwner::GluePet, None, 3);
    assert_eq!(index.parent(ParentBinding::Glue), Some(2));
    index.insert(M2GpuPlacementOwner::GlueModel { object_index: 20 }, None, 4);
    assert_eq!(index.parent(ParentBinding::Glue), Some(4));
}

/// Reused gameplay identifiers cannot replace an effect's allocation identity.
#[test]
fn effect_parent_uses_animation_allocation_identity_instead_of_guid() {
    let mut index = PlacementAncestry::default();
    let first = 1_u8;
    let replacement = 2_u8;
    // Only addresses are compared; these deliberately are not dereferenceable
    // animation objects, matching the lookup's non-owning identity contract.
    let first = std::ptr::from_ref(&first).cast::<UnitAnimationBehavior>();
    let replacement = std::ptr::from_ref(&replacement).cast::<UnitAnimationBehavior>();
    let owner = M2GpuPlacementOwner::CreatureBody { guid: 7 };
    index.insert(owner, Some(first), 0);
    index.insert(owner, Some(replacement), 1);
    assert_eq!(index.parent(ParentBinding::Animation(first)), Some(0));
    assert_eq!(index.parent(ParentBinding::Animation(replacement)), Some(1));
    index.insert(
        M2GpuPlacementOwner::CreatureMount { guid: 7 },
        Some(first),
        2,
    );
    assert_eq!(index.parent(ParentBinding::Animation(first)), Some(2));
}
