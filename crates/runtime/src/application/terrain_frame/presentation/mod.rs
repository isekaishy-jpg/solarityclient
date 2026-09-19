//! Main-affinity world preparation retains explicit stock ordering boundaries.

mod surfaces;
mod world;

/// These operations have frozen scene inputs and may run while model jobs are
/// pending. Receivers and fog publication remain behind both operations.
#[derive(Clone, Copy)]
pub(super) enum MainPreparationStep {
    GroundDetail,
    WorldModels,
}

impl MainPreparationStep {
    /// Preserve the original main preparation and failure precedence.
    pub(super) const ORDERED: [Self; 2] = [Self::GroundDetail, Self::WorldModels];
}
