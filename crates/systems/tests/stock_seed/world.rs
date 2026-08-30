//! External stock-compatibility tests for build-12340 world policy.

use solarity_ecs::WorldMapId;
use solarity_systems::{
    EXTENDED_WORLD_VIEW_DISTANCE_MAXIMUM, LEGACY_WORLD_VIEW_DISTANCE_MAXIMUM,
    WORLD_VIEW_DISTANCE_MINIMUM, WorldViewDistanceError, WorldViewDistanceLimit,
    WorldViewDistanceRequest, resolve_world_view_distance,
};

const ONE_GIBIBYTE: u64 = 0x4000_0000;

/// Azeroth retains the legacy ceiling even on high-memory systems.
#[test]
fn legacy_maps_require_the_hidden_override_for_extended_far_clip() {
    let ordinary = resolve_world_view_distance(WorldViewDistanceRequest::new(
        1_277.0,
        WorldMapId::new(0),
        ONE_GIBIBYTE + 1,
    ));
    let overridden = resolve_world_view_distance(
        WorldViewDistanceRequest::new(1_277.0, WorldMapId::new(0), 0).with_far_clip_override(),
    );

    assert_eq!(
        ordinary.map(|distance| (distance.value(), distance.limit())),
        Ok((
            LEGACY_WORLD_VIEW_DISTANCE_MAXIMUM,
            WorldViewDistanceLimit::Legacy,
        ))
    );
    assert_eq!(
        overridden.map(|distance| (distance.value(), distance.limit())),
        Ok((1_277.0, WorldViewDistanceLimit::Extended))
    );
}

/// Newer maps require physical memory strictly greater than one GiB.
#[test]
fn newer_maps_select_the_memory_gated_far_clip_ceiling() {
    let threshold = resolve_world_view_distance(WorldViewDistanceRequest::new(
        2_000.0,
        WorldMapId::new(571),
        ONE_GIBIBYTE,
    ));
    let above_threshold = resolve_world_view_distance(WorldViewDistanceRequest::new(
        2_000.0,
        WorldMapId::new(571),
        ONE_GIBIBYTE + 1,
    ));

    assert_eq!(
        threshold.map(|distance| distance.value()),
        Ok(LEGACY_WORLD_VIEW_DISTANCE_MAXIMUM)
    );
    assert_eq!(
        above_threshold.map(|distance| (distance.value(), distance.limit())),
        Ok((
            EXTENDED_WORLD_VIEW_DISTANCE_MAXIMUM,
            WorldViewDistanceLimit::Extended,
        ))
    );
}

/// The native world clamp supersedes the lower UI CVar range boundary.
#[test]
fn world_far_clip_uses_the_native_minimum() {
    let distance = resolve_world_view_distance(WorldViewDistanceRequest::new(
        177.0,
        WorldMapId::new(571),
        ONE_GIBIBYTE + 1,
    ));

    assert_eq!(
        distance.map(|distance| distance.value()),
        Ok(WORLD_VIEW_DISTANCE_MINIMUM)
    );
}

/// Invalid CVar state is not replaced with an invented viewing distance.
#[test]
fn non_finite_far_clip_has_no_runtime_fallback() {
    assert_eq!(
        resolve_world_view_distance(WorldViewDistanceRequest::new(
            f32::NAN,
            WorldMapId::new(571),
            ONE_GIBIBYTE + 1,
        )),
        Err(WorldViewDistanceError::NonFiniteRequested)
    );
}
