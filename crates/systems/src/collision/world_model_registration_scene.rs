//! Native cross-root registration banks and terrain occlusion (`0x007C2700`).

use glam::Vec3;

use super::{
    MovementBspCacheMode, PlacedWorldModelCollision, WorldModelCollisionError,
    WorldModelRegistrationHit, WorldModelRegistrationKind,
};

/// A selected WMO group with the spatial scene's exact root identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelRegistrationCandidate<Owner> {
    owner: Owner,
    hit: WorldModelRegistrationHit,
}

impl<Owner: Copy> WorldModelRegistrationCandidate<Owner> {
    /// Returns the root identity supplied by the scene during traversal.
    #[must_use]
    pub const fn owner(self) -> Owner {
        self.owner
    }

    /// Returns the selected group, fraction, face, and interior classification.
    #[must_use]
    pub const fn hit(self) -> WorldModelRegistrationHit {
        self.hit
    }

    const fn without_face(mut self) -> Self {
        self.hit = self.hit.without_face();
        self
    }
}

/// Allocation-free accumulation in native transformed/static root banks.
pub struct WorldModelRegistrationQuery<Owner> {
    start: Vec3,
    end: Vec3,
    point: Vec3,
    primary: [Option<WorldModelRegistrationCandidate<Owner>>; 2],
    fallback: [Option<WorldModelRegistrationCandidate<Owner>>; 2],
}

impl<Owner: Copy> WorldModelRegistrationQuery<Owner> {
    /// Starts the original 1.05-fraction probe with empty result banks.
    ///
    /// # Errors
    /// Returns [`WorldModelCollisionError::NonFiniteSegment`] for invalid points.
    pub fn new(
        start: Vec3,
        end: Vec3,
        containment_point: Vec3,
    ) -> Result<Self, WorldModelCollisionError> {
        if !start.is_finite() || !end.is_finite() || !containment_point.is_finite() {
            return Err(WorldModelCollisionError::NonFiniteSegment);
        }
        Ok(Self {
            start,
            end,
            point: containment_point,
            primary: [None; 2],
            fallback: [None; 2],
        })
    }

    /// Visits one eligible, resident root in the scene's registration order.
    ///
    /// Each root sees the current maxima in its own bank. Portals may replace
    /// a slightly nearer result under the original tolerance, so results are
    /// accepted directly after the root probe. The scene resolves root residency
    /// and the native exclusion flag before calling this method.
    ///
    /// # Errors
    /// Returns [`WorldModelCollisionError`] if the placed root query fails.
    pub fn probe_root(
        &mut self,
        owner: Owner,
        kind: WorldModelRegistrationKind,
        placement: &mut PlacedWorldModelCollision,
        cache_mode: MovementBspCacheMode,
    ) -> Result<(), WorldModelCollisionError> {
        let bank = usize::from(kind == WorldModelRegistrationKind::Static);
        let maximum = [self.primary[bank], self.fallback[bank]]
            .map(|hit| hit.map_or(1.05, |hit| hit.hit.fraction()));
        let result = placement
            .probe_registration(self.start, self.end, self.point, kind, maximum, cache_mode)?;
        if let Some(hit) = result.primary {
            self.primary[bank] = Some(WorldModelRegistrationCandidate { owner, hit });
        }
        if let Some(hit) = result.fallback {
            self.fallback[bank] = Some(WorldModelRegistrationCandidate { owner, hit });
        }
        Ok(())
    }

    /// Resolves native fallback promotion and transformed-root preference.
    ///
    /// A transformed result takes precedence even when a static root is closer.
    /// Static candidates move into the preferred bank only if the transformed
    /// bank has no primary or fallback hit. A synthesized fallback copies its
    /// primary candidate with the face cleared, as native `0x007C2700` does.
    #[must_use]
    pub fn finish(mut self) -> WorldModelRegistrationSelection<Owner> {
        for bank in 0..2 {
            if self.primary[bank].is_none() {
                self.primary[bank] = self.fallback[bank];
            }
        }
        if self.primary[0].is_none() {
            self.primary[0] = self.primary[1].take();
            self.fallback[0] = self.fallback[1].take();
        }
        for bank in 0..2 {
            if self.fallback[bank].is_none() {
                self.fallback[bank] =
                    self.primary[bank].map(WorldModelRegistrationCandidate::without_face);
            }
        }
        WorldModelRegistrationSelection {
            primary: self.primary,
            fallback: self.fallback,
        }
    }
}

/// Resolved banks retained until the spatial owner applies terrain and flags.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelRegistrationSelection<Owner> {
    primary: [Option<WorldModelRegistrationCandidate<Owner>>; 2],
    fallback: [Option<WorldModelRegistrationCandidate<Owner>>; 2],
}

impl<Owner: Copy> WorldModelRegistrationSelection<Owner> {
    /// Returns preferred and secondary primary candidates in native bank order.
    #[must_use]
    pub const fn primary(self) -> [Option<WorldModelRegistrationCandidate<Owner>>; 2] {
        self.primary
    }

    /// Returns corresponding fallback candidates in native bank order.
    #[must_use]
    pub const fn fallback(self) -> [Option<WorldModelRegistrationCandidate<Owner>>; 2] {
        self.fallback
    }

    /// Returns the first surviving primary candidate used to choose interior registration.
    #[must_use]
    pub fn selected(self) -> Option<WorldModelRegistrationCandidate<Owner>> {
        self.primary[0].or(self.primary[1])
    }

    /// Applies native map-owner flag `+0x7C & 0x2000` after bank promotion.
    pub fn clear_secondary_bank(&mut self) {
        self.primary[1] = None;
        self.fallback[1] = None;
    }

    /// Removes each bank whose primary candidate is strictly below terrain.
    ///
    /// The caller supplies `0x007C28F0`'s stored terrain fraction after resolving
    /// height and rejecting terrain above the probe start. Equality preserves
    /// the WMO bank. Its fallback is removed together with its primary result.
    ///
    /// # Errors
    /// Returns [`WorldModelCollisionError::InvalidMaximumFraction`] for negative
    /// or non-finite terrain fractions.
    pub fn occlude_by_terrain(&mut self, fraction: f32) -> Result<(), WorldModelCollisionError> {
        if !fraction.is_finite() || fraction < 0.0 {
            return Err(WorldModelCollisionError::InvalidMaximumFraction);
        }
        for bank in 0..2 {
            if self.primary[bank].is_some_and(|hit| fraction < hit.hit.fraction()) {
                self.primary[bank] = None;
                self.fallback[bank] = None;
            }
        }
        Ok(())
    }
}
