//! External stock-compatibility tests for ordered world-transition progress.

#[path = "../../src/loading/readiness.rs"]
mod readiness;

use readiness::{RuntimeLoadingReadiness, RuntimeLoadingStage};

/// Later subsystem readiness cannot bypass an earlier first-frame prerequisite.
#[test]
fn loading_readiness_requires_the_complete_stage_chain() {
    let readiness = RuntimeLoadingReadiness {
        world_accepted: false,
        environment_ready: true,
        player_ready: true,
        scene_ready: true,
        transport_admitted: true,
    };
    assert_eq!(readiness.stage(), RuntimeLoadingStage::AwaitingWorld);

    let readiness = RuntimeLoadingReadiness {
        world_accepted: true,
        environment_ready: false,
        player_ready: true,
        scene_ready: true,
        transport_admitted: true,
    };
    assert_eq!(readiness.stage(), RuntimeLoadingStage::WorldAccepted);

    let readiness = RuntimeLoadingReadiness {
        world_accepted: true,
        environment_ready: true,
        player_ready: false,
        scene_ready: true,
        transport_admitted: true,
    };
    assert_eq!(readiness.stage(), RuntimeLoadingStage::EnvironmentReady);

    let readiness = RuntimeLoadingReadiness {
        world_accepted: true,
        environment_ready: true,
        player_ready: true,
        scene_ready: false,
        transport_admitted: true,
    };
    assert_eq!(readiness.stage(), RuntimeLoadingStage::PlayerReady);

    let readiness = RuntimeLoadingReadiness {
        world_accepted: true,
        environment_ready: true,
        player_ready: true,
        scene_ready: true,
        transport_admitted: false,
    };
    assert_eq!(readiness.stage(), RuntimeLoadingStage::PlayerReady);

    let readiness = RuntimeLoadingReadiness {
        world_accepted: true,
        environment_ready: true,
        player_ready: true,
        scene_ready: true,
        transport_admitted: true,
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
