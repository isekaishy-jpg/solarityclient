//! Stable failures while reproducing stock M2 shader selection.

use solarity_asset::AssetPath;
use thiserror::Error;

/// A batch that cannot name one of build-12340's stock M2 effects.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum M2ShaderPlanError {
    /// Stock's M2 shader path has fixed storage for one or two texture stages.
    #[error("M2 model {path} batch has unsupported texture count {texture_count}")]
    TextureCount {
        /// Model containing the malformed batch.
        path: AssetPath,
        /// Batch texture-stage count outside the stock range.
        texture_count: u16,
    },
    /// A model using flag `0x8` selected beyond its combiner table.
    #[error("M2 model {path} batch texture stage {stage} has no combiner entry {combo_index}")]
    MissingCombiner {
        /// Model containing the malformed batch.
        path: AssetPath,
        /// Zero-based texture stage within the batch.
        stage: u16,
        /// Computed table index.
        combo_index: usize,
    },
    /// The specialized high-bit selector is not one stock build 12340 maps.
    #[error("M2 model {path} uses unsupported specialized shader {shader_id:#06x}")]
    SpecializedShader {
        /// Model containing the unsupported selector.
        path: AssetPath,
        /// Exact post-substitution SKIN shader word.
        shader_id: u16,
    },
    /// The executable's observed simple-effect retry could not form an effect.
    #[error("M2 model {path} could not construct stock shader fallback 0x0011")]
    StockFallback {
        /// Model whose otherwise supported batch reached the retry.
        path: AssetPath,
    },
}
