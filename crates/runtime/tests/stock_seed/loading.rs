//! External stock-compatibility tests for ordered world-transition progress.

#[path = "../../src/loading/layout.rs"]
mod layout;
#[path = "../../src/loading/readiness.rs"]
mod readiness;

use layout::{STOCK_LOADING_ART_ASPECT, STOCK_WIDE_LOADING_ART_ASPECT, centered_aspect_fit_bounds};
use readiness::{RuntimeLoadingReadiness, RuntimeLoadingStage};

/// 0x0040A270 fits the graphics viewport; it does not crop the artwork's UVs.
#[test]
fn loading_card_fits_stock_viewport_with_black_margins() {
    assert_eq!(
        centered_aspect_fit_bounds([1_024.0, 768.0], STOCK_LOADING_ART_ASPECT),
        [0.0, 0.0, 1_024.0, 768.0]
    );
    assert_bounds_close(
        centered_aspect_fit_bounds([1_366.0, 768.0], STOCK_LOADING_ART_ASPECT),
        [171.0, 0.0, 1_195.0, 768.0],
    );
    assert_bounds_close(
        centered_aspect_fit_bounds([1_024.0, 768.0], STOCK_WIDE_LOADING_ART_ASPECT),
        [0.0, 64.0, 1_024.0, 704.0],
    );
    assert_bounds_close(
        centered_aspect_fit_bounds([1_920.0, 1_080.0], STOCK_WIDE_LOADING_ART_ASPECT),
        [96.0, 0.0, 1_824.0, 1_080.0],
    );
    // The reported test session used this actual fullscreen drawable extent.
    assert_bounds_close(
        centered_aspect_fit_bounds([2_560.0, 1_440.0], STOCK_WIDE_LOADING_ART_ASPECT),
        [128.0, 0.0, 2_432.0, 1_440.0],
    );
}

fn assert_bounds_close(actual: [f32; 4], expected: [f32; 4]) {
    assert!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(actual, expected)| (actual - expected).abs() < 0.001),
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
