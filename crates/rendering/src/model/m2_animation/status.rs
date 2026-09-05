//! Stable failures from M2 bone-pose composition.

use thiserror::Error;

/// A decoded animation set cannot produce the requested model pose.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum M2BonePoseError {
    /// The caller selected a sequence index outside the model catalog.
    #[error("M2 animation sequence {requested} is unavailable; model has {available} sequences")]
    SequenceIndex {
        /// Requested zero-based sequence index.
        requested: usize,
        /// Number of decoded sequence records.
        available: usize,
    },
    /// Stock disabled an external or aliased sequence whose payload is absent.
    #[error("M2 animation sequence {sequence} has no available payload")]
    SequenceUnavailable {
        /// Requested zero-based sequence index.
        sequence: usize,
    },
    /// NaN or infinity cannot participate in deterministic animation clocks.
    #[error("M2 animation clock contains a non-finite time")]
    NonFiniteTime,
    /// A previous pose can contribute only a finite unit-interval weight.
    #[error("M2 sequence blend weight must be finite and between zero and one")]
    InvalidBlendWeight,
    /// Billboard composition requires the active camera/view basis.
    #[error("M2 billboard bone {bone} requires a camera-relative pose path")]
    BillboardViewRequired {
        /// Zero-based bone index carrying one of stock's billboard flags.
        bone: usize,
    },
    /// The model-to-view transform cannot be inverted for billboard recovery.
    #[error("M2 billboard pose requires a finite, invertible model-view transform")]
    InvalidModelView,
    /// An attachment from another model references a missing palette entry.
    #[error("M2 attachment bone {requested} is unavailable; pose has {available} bones")]
    AttachmentBoneIndex {
        /// Attachment bone index.
        requested: u16,
        /// Number of transforms in the composed pose.
        available: usize,
    },
    /// A particle declaration from another model references a missing palette entry.
    #[error("M2 particle bone {requested} is unavailable; pose has {available} bones")]
    ParticleBoneIndex {
        /// Emitter bone index.
        requested: u16,
        /// Number of transforms in the composed pose.
        available: usize,
    },
    /// A decoded light from another model references a missing palette entry.
    #[error("M2 light bone {requested} is unavailable; pose has {available} bones")]
    LightBoneIndex {
        /// Light bone index.
        requested: u16,
        /// Number of transforms in the composed pose.
        available: usize,
    },
}

/// A model/mesh pair cannot produce one animated material snapshot.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum M2MaterialPoseError {
    /// Bone and material animation share the same sequence validity rules.
    #[error(transparent)]
    Animation(#[from] M2BonePoseError),
    /// The mesh plan was prepared from another decoded model identity.
    #[error("M2 material pose model does not match its mesh plan")]
    ModelMismatch,
    /// The requested SKIN material batch is absent.
    #[error("M2 material draw {requested} is unavailable; plan has {available} draws")]
    DrawIndex {
        /// Requested zero-based draw slot.
        requested: usize,
        /// Number of draws retained by the plan.
        available: usize,
    },
    /// A validated SKIN color selector is unexpectedly absent.
    #[error("M2 color {requested} is unavailable; model has {available} colors")]
    ColorIndex {
        /// Authored color-animation slot.
        requested: u16,
        /// Number of decoded color animations.
        available: usize,
    },
    /// A texture-weight combo starts outside the model lookup.
    #[error("M2 texture-weight lookup {requested} is unavailable; model has {available} entries")]
    TextureWeightLookup {
        /// Authored lookup-table slot.
        requested: usize,
        /// Number of decoded lookup entries.
        available: usize,
    },
    /// A texture-weight lookup references an absent animation.
    #[error("M2 texture weight {requested} is unavailable; model has {available} weights")]
    TextureWeightIndex {
        /// Authored texture-weight slot.
        requested: u16,
        /// Number of decoded texture weights.
        available: usize,
    },
    /// A texture-transform combo starts outside the model lookup.
    #[error(
        "M2 texture-transform lookup {requested} is unavailable; model has {available} entries"
    )]
    TextureTransformLookup {
        /// Authored lookup-table slot.
        requested: usize,
        /// Number of decoded lookup entries.
        available: usize,
    },
    /// Build 12340 has no material path beyond two texture stages.
    #[error("M2 material requests unsupported texture-stage count {requested}")]
    TextureStageCount {
        /// Authored stage count outside the fixed one/two-stage domain.
        requested: u16,
    },
    /// A texture-transform lookup references an absent animation.
    #[error("M2 texture transform {requested} is unavailable; model has {available} transforms")]
    TextureTransformIndex {
        /// Authored texture-transform slot.
        requested: u16,
        /// Number of decoded texture transforms.
        available: usize,
    },
}
