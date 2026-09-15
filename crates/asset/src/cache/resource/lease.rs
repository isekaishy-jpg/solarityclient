//! Consumer pins preserve payload identity while cache ownership remains separate.

use std::{
    fmt,
    ops::Deref,
    sync::{Arc, Weak},
};

use super::releases::{Releases, Ticket};

/// One shared pin covers all clones of a live consumer generation.
pub(super) struct Pin<T> {
    pub(super) value: Arc<T>,
    release: Option<(Weak<Releases>, Ticket)>,
}

impl<T> Drop for Pin<T> {
    fn drop(&mut self) {
        if let Some((owner, ticket)) = &self.release
            && let Some(owner) = owner.upgrade()
        {
            owner.notify(*ticket);
        }
    }
}

/// Pins an immutable resource without exposing an untracked owning Arc.
/// Cloning a live lease shares its pin; the final clone notifies its cache once.
pub struct ResourceLease<T> {
    pub(super) pin: Arc<Pin<T>>,
}

impl<T> ResourceLease<T> {
    /// Owns an immutable resource with no cache retention or scene activation.
    #[must_use]
    pub fn new(value: T) -> Self {
        Self {
            pin: Arc::new(Pin {
                value: Arc::new(value),
                release: None,
            }),
        }
    }

    /// The cache retains its distinct payload owner while consumers share one pin.
    pub(super) fn cached(value: Arc<T>, releases: &Arc<Releases>, ticket: Ticket) -> Self {
        Self {
            pin: Arc::new(Pin {
                value,
                release: Some((Arc::downgrade(releases), ticket)),
            }),
        }
    }

    /// Compares immutable payload generations, including release/reacquire cycles.
    #[must_use]
    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        Arc::ptr_eq(&this.pin.value, &other.pin.value)
    }

    /// Non-owning identity only; callers must keep a lease while accessing the value.
    #[must_use]
    pub fn as_ptr(this: &Self) -> *const T {
        Arc::as_ptr(&this.pin.value)
    }

    /// Observes identity and existing demand without keeping a resource consumer alive.
    #[must_use]
    pub fn downgrade(this: &Self) -> ResourceWeak<T> {
        ResourceWeak {
            value: Arc::downgrade(&this.pin.value),
            pin: Arc::downgrade(&this.pin),
        }
    }
}

impl<T> Clone for ResourceLease<T> {
    fn clone(&self) -> Self {
        Self {
            pin: Arc::clone(&self.pin),
        }
    }
}
impl<T> Deref for ResourceLease<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.pin.value
    }
}
impl<T> AsRef<T> for ResourceLease<T> {
    fn as_ref(&self) -> &T {
        self
    }
}
impl<T: fmt::Debug> fmt::Debug for ResourceLease<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

/// Weak observation can join existing consumers but cannot reactivate cache-only data.
/// New demand after the last release must go through the owning resource cache.
pub struct ResourceWeak<T> {
    value: Weak<T>,
    pin: Weak<Pin<T>>,
}
impl<T> ResourceWeak<T> {
    /// Reports whether the immutable generation still has a cache or consumer owner.
    /// This is an observation only; it neither pins nor reactivates the resource.
    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.value.strong_count() != 0
    }

    /// Upgrades only while a consumer of this pin generation remains live.
    #[must_use]
    pub fn upgrade(&self) -> Option<ResourceLease<T>> {
        self.pin.upgrade().map(|pin| ResourceLease { pin })
    }
    /// Identity remains comparable while this weak observer exists.
    #[must_use]
    pub fn as_ptr(&self) -> *const T {
        self.value.as_ptr()
    }
    /// Compares payload identity without reviving consumer demand.
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.value.ptr_eq(&other.value)
    }
}
impl<T> Default for ResourceWeak<T> {
    fn default() -> Self {
        Self {
            value: Weak::new(),
            pin: Weak::new(),
        }
    }
}
impl<T> Clone for ResourceWeak<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            pin: self.pin.clone(),
        }
    }
}
impl<T> fmt::Debug for ResourceWeak<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResourceWeak").finish_non_exhaustive()
    }
}
