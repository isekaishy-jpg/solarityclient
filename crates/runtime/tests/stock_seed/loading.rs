//! External stock-compatibility tests for ordered world-transition progress.

#[path = "../../src/loading/layout.rs"]
mod layout;
#[path = "../../src/loading/readiness.rs"]
mod readiness;

use layout::{STOCK_LOADING_ART_ASPECT, STOCK_WIDE_LOADING_ART_ASPECT, centered_aspect_fill_uv};
use readiness::{RuntimeLoadingReadiness, RuntimeLoadingStage};

/// Loading art retains its authored aspect ratio and crops around the center.
#[test]
fn loading_art_uses_stock_centered_aspect_fill_coordinates() {
    assert_eq!(
        centered_aspect_fill_uv([1_024.0, 768.0], STOCK_LOADING_ART_ASPECT),
        [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]]
    );

    let widescreen_viewport = centered_aspect_fill_uv([1_366.0, 768.0], STOCK_LOADING_ART_ASPECT);
    assert_uv_close(widescreen_viewport[0], [0.0, 0.125_183_02]);
    assert_uv_close(widescreen_viewport[3], [1.0, 0.874_816_95]);

    let standard_viewport =
        centered_aspect_fill_uv([1_024.0, 768.0], STOCK_WIDE_LOADING_ART_ASPECT);
    assert_uv_close(standard_viewport[0], [0.083_333_31, 0.0]);
    assert_uv_close(standard_viewport[3], [0.916_666_7, 1.0]);

    let wide_art_on_sixteen_nine =
        centered_aspect_fill_uv([1_920.0, 1_080.0], STOCK_WIDE_LOADING_ART_ASPECT);
    assert_uv_close(wide_art_on_sixteen_nine[0], [0.0, 0.05]);
    assert_uv_close(wide_art_on_sixteen_nine[3], [1.0, 0.95]);
}

fn assert_uv_close(actual: [f32; 2], expected: [f32; 2]) {
    assert!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(actual, expected)| (actual - expected).abs() < 0.000_01),
        "actual={actual:?} expected={expected:?}"
    );
}

/// Later subsystem readiness cannot bypass an earlier first-frame prerequisite.
#[test]
fn loading_readiness_requires_the_complete_stage_chain() {
    let readiness = RuntimeLoadingReadiness {
        world_accepted: false,
        environment_ready: true,
        player_ready: true,
        scene_ready: true,
        ui_ready: true,
        transport_resource_ready: true,
    };
    assert_eq!(readiness.stage(), RuntimeLoadingStage::AwaitingWorld);

    let readiness = RuntimeLoadingReadiness {
        world_accepted: true,
        environment_ready: false,
        player_ready: true,
        scene_ready: true,
        ui_ready: true,
        transport_resource_ready: true,
    };
    assert_eq!(readiness.stage(), RuntimeLoadingStage::WorldAccepted);

    let readiness = RuntimeLoadingReadiness {
        world_accepted: true,
        environment_ready: true,
        player_ready: false,
        scene_ready: true,
        ui_ready: true,
        transport_resource_ready: true,
    };
    assert_eq!(readiness.stage(), RuntimeLoadingStage::EnvironmentReady);

    let readiness = RuntimeLoadingReadiness {
        world_accepted: true,
        environment_ready: true,
        player_ready: true,
        scene_ready: false,
        ui_ready: true,
        transport_resource_ready: true,
    };
    assert_eq!(readiness.stage(), RuntimeLoadingStage::PlayerReady);

    let readiness = RuntimeLoadingReadiness {
        world_accepted: true,
        environment_ready: true,
        player_ready: true,
        scene_ready: true,
        ui_ready: true,
        transport_resource_ready: false,
    };
    assert_eq!(readiness.stage(), RuntimeLoadingStage::PlayerReady);

    let readiness = RuntimeLoadingReadiness {
        world_accepted: true,
        environment_ready: true,
        player_ready: true,
        scene_ready: true,
        ui_ready: false,
        transport_resource_ready: true,
    };
    assert_eq!(readiness.stage(), RuntimeLoadingStage::PlayerReady);

    let readiness = RuntimeLoadingReadiness {
        world_accepted: true,
        environment_ready: true,
        player_ready: true,
        scene_ready: true,
        ui_ready: true,
        transport_resource_ready: true,
    };
    assert_eq!(readiness.stage(), RuntimeLoadingStage::SceneReady);
}

/// Retained card generations use monotonic finite progress ending at completion.
#[test]
fn loading_stages_have_monotonic_finite_progress() {
    let progress = RuntimeLoadingStage::ALL.map(RuntimeLoadingStage::progress);
    let indices = RuntimeLoadingStage::ALL.map(RuntimeLoadingStage::index);

    assert!(progress.into_iter().all(f32::is_finite));
    assert!(progress.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(progress.last(), Some(&1.0));
    assert_eq!(indices, [0, 1, 2, 3, 4]);
}
