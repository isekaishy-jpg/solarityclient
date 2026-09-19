//! Local appearance requests retain main-owned movement, camera and publication.

mod coordinator;
mod glue;
mod pose;
mod publication;
mod snapshot;

use super::DesiredPlayerModel;
use solarity_rendering::{CharacterGeosetPlan, CharacterTexturePlan};

/// Frozen local joins; Glue plans exist only when a selected body could transfer.
struct LocalAppearance {
    desired: DesiredPlayerModel,
    glue_plans: Option<(CharacterTexturePlan, CharacterGeosetPlan)>,
}
