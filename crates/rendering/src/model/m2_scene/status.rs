//! Stable failures while translating decoded M2 data into renderer geometry.

use solarity_asset::AssetPath;
use thiserror::Error;

/// A malformed or unavailable M2 view needed for renderer preparation.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum M2MeshPlanError {
    /// The caller selected a profile not named by the M2 header.
    #[error("M2 model {path} has no skin profile {profile_index}")]
    MissingProfile {
        /// Model whose profile was requested.
        path: AssetPath,
        /// Explicit zero-based profile index.
        profile_index: usize,
    },
    /// A profile triangle could not map through its vertex lookup.
    #[error(
        "M2 skin {path} triangle entry {triangle_index} references missing profile vertex {profile_vertex}"
    )]
    MissingProfileVertex {
        /// External SKIN path containing the invalid lookup.
        path: AssetPath,
        /// Position within the triangle lookup.
        triangle_index: usize,
        /// Referenced profile-local vertex index.
        profile_vertex: u16,
    },
    /// A used SKIN palette entry exceeded the model bone lookup table.
    #[error(
        "M2 skin {path} profile vertex {profile_vertex} influence {influence} references missing bone lookup {lookup_index}"
    )]
    MissingBoneLookup {
        /// External SKIN path containing the palette byte.
        path: AssetPath,
        /// Profile-local vertex carrying the palette reference.
        profile_vertex: usize,
        /// Zero-based influence within the vertex.
        influence: usize,
        /// Computed entry in the model bone lookup table.
        lookup_index: usize,
    },
    /// A material batch selected an absent submesh.
    #[error("M2 skin {path} batch {batch_index} references missing submesh {submesh_index}")]
    MissingSubmesh {
        /// External SKIN path containing the batch.
        path: AssetPath,
        /// Zero-based material batch index.
        batch_index: usize,
        /// Referenced submesh index.
        submesh_index: u16,
    },
    /// A material batch selected an absent model material.
    #[error("M2 skin {path} batch {batch_index} references missing material {material_index}")]
    MissingMaterial {
        /// External SKIN path containing the batch.
        path: AssetPath,
        /// Zero-based material batch index.
        batch_index: usize,
        /// Referenced model material index.
        material_index: u16,
    },
    /// A batch texture stage exceeded the model texture lookup.
    #[error(
        "M2 skin {path} batch {batch_index} texture stage {stage} has no texture combo {combo_index}"
    )]
    MissingTextureCombo {
        /// External SKIN path containing the batch.
        path: AssetPath,
        /// Zero-based material batch index.
        batch_index: usize,
        /// Zero-based texture stage within the batch.
        stage: u16,
        /// Computed model texture-combo index.
        combo_index: usize,
    },
    /// A texture lookup selected an absent M2 texture declaration.
    #[error(
        "M2 skin {path} batch {batch_index} texture stage {stage} references missing texture {texture_index}"
    )]
    MissingTexture {
        /// External SKIN path containing the batch.
        path: AssetPath,
        /// Zero-based material batch index.
        batch_index: usize,
        /// Zero-based texture stage within the batch.
        stage: u16,
        /// Referenced M2 texture declaration index.
        texture_index: u16,
    },
    /// A batch texture stage exceeded the texture-coordinate lookup.
    #[error(
        "M2 skin {path} batch {batch_index} texture stage {stage} has no coordinate combo {combo_index}"
    )]
    MissingCoordinateCombo {
        /// External SKIN path containing the batch.
        path: AssetPath,
        /// Zero-based material batch index.
        batch_index: usize,
        /// Zero-based texture stage within the batch.
        stage: u16,
        /// Computed texture-coordinate combo index.
        combo_index: usize,
    },
}
