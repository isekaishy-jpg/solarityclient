//! Shared native-order placement fixtures without GPU or archive dependencies.

use crate::application::terrain_coordinator::m2_residency::ResidentM2Owner;
use crate::application::terrain_frame::m2::{M2GpuPlacement, M2GpuPlacementOwner};
use solarity_rendering::CharacterAttachmentPoint;

/// Static residency precedes native unit bodies, optional mounts and equipment.
pub(super) fn placement_fixture(scenery: u32, units: u64) -> Vec<M2GpuPlacement> {
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
pub(super) fn placement(owner: M2GpuPlacementOwner) -> M2GpuPlacement {
    M2GpuPlacement {
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
        particles: Default::default(),
        ribbons: Default::default(),
        last_effect_time_ms: 0,
        unit_effect: None,
    }
}
