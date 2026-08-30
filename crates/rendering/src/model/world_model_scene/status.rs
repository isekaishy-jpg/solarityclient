//! Strict WMO mesh-preparation failures.

use solarity_asset::AssetPath;
use thiserror::Error;

/// Failure while combining one decoded WMO generation for GPU upload.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum WorldModelMeshPlanError {
    /// Combined HD geometry cannot fit the renderer's direct `u32` indices.
    #[error("WMO {path} combined geometry exceeds u32 indexing")]
    IndexCapacity {
        /// Root WMO whose groups exceed the direct renderer ABI.
        path: AssetPath,
    },
    /// A decoded draw no longer fits its owning combined group range.
    #[error("WMO {path} group {group_index} batch {batch_index} range overflows")]
    DrawRange {
        /// Root WMO whose immutable decoded tables disagree.
        path: AssetPath,
        /// Numeric group file index.
        group_index: u32,
        /// MOBA index within the group.
        batch_index: usize,
    },
}

/// Failure while preparing one owning MODF or game-object WMO transform.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum WorldModelPlacementError {
    /// Position, rotation, scale, or the resulting matrix is invalid.
    #[error("placed WMO {path} has an invalid transform")]
    InvalidTransform {
        /// Shared root-WMO generation being placed.
        path: AssetPath,
    },
    /// A draw references a group absent from the immutable mesh plan.
    #[error("placed WMO {path} draw {draw_index} references missing group {group_index}")]
    MissingGroup {
        /// Shared root-WMO generation being placed.
        path: AssetPath,
        /// Draw position in the combined MOBA table.
        draw_index: usize,
        /// Missing numeric group index.
        group_index: u32,
    },
}
