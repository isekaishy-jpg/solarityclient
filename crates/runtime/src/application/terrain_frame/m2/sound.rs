//! Callback handles follow the lifetime of the actual model instance.

use std::rc::{Rc, Weak};

use super::M2GpuPlacementOwner;

/// Callback policy recovered from 70C050 (game object) and 7BD5A0 (doodad).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::application) enum M2SoundKind {
    Doodad,
    GameObject,
}

impl M2SoundKind {
    pub(in crate::application) const fn fade_in_seconds(self) -> f32 {
        match self {
            Self::Doodad => 3.0,
            Self::GameObject => 1.0,
        }
    }

    pub(in crate::application) const fn fade_out_seconds(self) -> f32 {
        match self {
            Self::Doodad => 0.0,
            Self::GameObject => 1.0,
        }
    }

    pub(super) const fn for_placement(owner: M2GpuPlacementOwner) -> Option<Self> {
        match owner {
            M2GpuPlacementOwner::Static(_)
            | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. } => Some(Self::Doodad),
            M2GpuPlacementOwner::GameObject { .. } => Some(Self::GameObject),
            _ => None,
        }
    }
}

/// A weak callback handle cannot keep a removed rendering model alive.
#[derive(Clone, Debug)]
pub(in crate::application) struct M2SoundOwner {
    lifetime: Weak<M2SoundKind>,
    kind: M2SoundKind,
}

impl M2SoundOwner {
    pub(in crate::application) fn new(lifetime: &Rc<M2SoundKind>) -> Self {
        Self {
            lifetime: Rc::downgrade(lifetime),
            kind: **lifetime,
        }
    }

    pub(in crate::application) fn is_live(&self) -> bool {
        self.lifetime.strong_count() != 0
    }

    pub(in crate::application) fn same_model(&self, other: &Self) -> bool {
        self.lifetime.ptr_eq(&other.lifetime)
    }

    pub(in crate::application) const fn kind(&self) -> M2SoundKind {
        self.kind
    }
}
