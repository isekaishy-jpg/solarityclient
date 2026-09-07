//! One borrowed movement context and its per-sweep world collection cache.

use glam::Vec3;
use solarity_ecs::{ActiveWorld, WorldObjectIdentity};
use solarity_systems::{
    MovementBspCacheMode, MovementCollisionBounds, MovementCollisionTriangle,
    MovementCollisionVolume, MovementGeometry, MovementIntervalRequest, MovementTransportFrame,
};

use super::{
    RuntimeMovementOwner, RuntimeMovementQuery, RuntimeStaticMovementError,
    RuntimeStaticMovementResidency, RuntimeTerrainCoordinator,
};
use crate::application::RuntimeGameObjectPresentation;

/// Geometry retained only while its world, object generations, and flags are borrowed.
///
/// Begin each interval with `collect_interval`, then call `prepare_sweep` before
/// each ground/step/fall probe. A hit reuses ordered candidates; a miss recollects
/// the union required by stock. Pending or failed collection exposes no faces.
/// Movement response and partial-state handling remain the solver's responsibility.
/// A supplied passenger frame converts collection bounds to world space and
/// collected faces back to solver space. Liquid/WDL policy is owned separately.
pub struct RuntimeMovementGeometry<'a> {
    terrain: &'a mut RuntimeTerrainCoordinator,
    world: &'a ActiveWorld,
    objects: &'a RuntimeGameObjectPresentation,
    flags: u32,
    cache: MovementBspCacheMode,
    output: &'a mut RuntimeMovementQuery,
    bounds: Option<MovementCollisionBounds>,
    frame: Option<MovementTransportFrame>,
    failure: Option<RuntimeMovementGeometryFailure>,
}

/// Retained cause of an unavailable probe reported by the movement solver.
#[derive(Debug)]
pub enum RuntimeMovementGeometryFailure {
    /// Required world geometry has not completed residency.
    Pending(RuntimeStaticMovementResidency),
    /// Invalid input, geometry, or a stale admitted reference prevented collection.
    Invalid(RuntimeStaticMovementError),
}

impl<'a> RuntimeMovementGeometry<'a> {
    /// Resolves the current world generation before a passenger link is admitted.
    pub(in crate::application) fn passenger_identity(
        &self,
        guid: u64,
    ) -> Option<WorldObjectIdentity> {
        self.world.object_identity(guid)
    }

    /// Reads the exact parent's matrix; a retired generation cannot be reused.
    pub(in crate::application) fn passenger_frame(
        &self,
        identity: WorldObjectIdentity,
    ) -> Result<Option<MovementTransportFrame>, solarity_systems::GameObjectPlacementError> {
        self.objects.object_movement_frame(identity)
    }

    /// Reads virtual +0xEC independently of an already attached parent's retention.
    pub(in crate::application) fn can_board(&self, identity: WorldObjectIdentity) -> bool {
        self.objects.object_can_board(identity)
    }

    /// Reads the behavior's published phase rather than substituting the client clock.
    pub(in crate::application) fn passenger_time_ms(
        &self,
        identity: WorldObjectIdentity,
    ) -> Option<u32> {
        self.objects.object_passenger_time_ms(identity)
    }

    /// Evaluates virtual +0xF0 only for a zero reported contact GUID.
    pub(in crate::application) fn retains_passenger(
        &self,
        identity: WorldObjectIdentity,
        position: Vec3,
    ) -> Result<bool, solarity_systems::MovementCollectionError> {
        self.objects.object_retains_passenger(identity, position)
    }

    /// Borrows one fixed context and invalidates any prior query's candidates.
    pub fn new(
        terrain: &'a mut RuntimeTerrainCoordinator,
        world: &'a ActiveWorld,
        objects: &'a RuntimeGameObjectPresentation,
        flags: u32,
        cache: MovementBspCacheMode,
        output: &'a mut RuntimeMovementQuery,
    ) -> Self {
        output.clear();
        Self {
            terrain,
            world,
            objects,
            flags,
            cache,
            output,
            bounds: None,
            frame: None,
            failure: None,
        }
    }

    /// Freezes a parent's matrix for this context's passenger-space probes.
    /// Changing coordinate systems invalidates both candidates and world coverage.
    pub fn set_transport_frame(&mut self, frame: Option<MovementTransportFrame>) {
        self.frame = frame;
        self.bounds = None;
        self.failure = None;
        self.output.clear();
    }

    /// Collects the initial interval region, including its private probe reach.
    ///
    /// # Errors
    /// Invalid inputs, geometry, or stale references invalidate this cache.
    pub fn collect_interval(
        &mut self,
        request: MovementIntervalRequest,
    ) -> Result<RuntimeStaticMovementResidency, RuntimeStaticMovementError> {
        self.bounds = None;
        self.failure = None;
        self.output.clear();
        let bounds = match self.frame {
            Some(frame) => request.collection_bounds_in_frame(frame)?,
            None => request.collection_bounds()?,
        };
        let residency = self.terrain.collect_movement(
            self.world,
            self.objects,
            bounds.query(),
            self.flags,
            self.cache,
            self.output,
        )?;
        if residency == RuntimeStaticMovementResidency::Ready {
            self.localize_candidates()?;
            self.output.set_interval_bounds(bounds);
            self.bounds = Some(bounds.query());
        }
        Ok(residency)
    }

    /// Ensures a sweep endpoint is covered before the solver consumes candidates.
    ///
    /// Only `Ready` permits narrow-phase use. A pending or failed refresh clears
    /// both faces and coverage; callers must collect a new interval before retrying.
    /// The returned region is published only after the entire query succeeds.
    ///
    /// # Errors
    /// Returns an error before initial complete collection, after invalidation,
    /// or for invalid sweep bounds, geometry, or stale references.
    pub fn prepare_sweep(
        &mut self,
        volume: &MovementCollisionVolume,
        direction: Vec3,
        distance: f32,
    ) -> Result<RuntimeStaticMovementResidency, RuntimeStaticMovementError> {
        let cached = self
            .bounds
            .ok_or(RuntimeStaticMovementError::InvalidReference)?;
        let refresh = match self.frame {
            Some(frame) => volume.sweep_refresh_bounds_in_frame(direction, distance, cached, frame),
            None => volume.sweep_refresh_bounds(direction, distance, cached),
        };
        let refresh = match refresh {
            Ok(refresh) => refresh,
            Err(error) => {
                self.bounds = None;
                self.output.clear();
                return Err(error.into());
            }
        };
        let Some(refresh) = refresh else {
            return Ok(RuntimeStaticMovementResidency::Ready);
        };
        self.bounds = None;
        let residency = self.terrain.collect_movement(
            self.world,
            self.objects,
            refresh,
            self.flags,
            self.cache,
            self.output,
        )?;
        if residency == RuntimeStaticMovementResidency::Ready {
            self.localize_candidates()?;
            self.bounds = Some(refresh);
        }
        Ok(residency)
    }

    /// Native collection converts complete world faces once, preserving owner order.
    fn localize_candidates(&mut self) -> Result<(), RuntimeStaticMovementError> {
        if let Some(frame) = self.frame {
            self.output.localize(frame)?;
        }
        Ok(())
    }

    /// Returns the last complete region within this borrowed context.
    #[must_use]
    pub const fn cached_bounds(&self) -> Option<MovementCollisionBounds> {
        self.bounds
    }

    /// Returns the last complete query's candidates in native append order.
    #[must_use]
    pub fn triangles(&self) -> &[MovementCollisionTriangle] {
        self.output.triangles()
    }

    /// Copies a selected face's owner before later probes can reorder candidates.
    #[must_use]
    pub fn owner(&self, triangle: usize) -> Option<RuntimeMovementOwner> {
        self.output.owner(triangle)
    }

    /// Returns the first unavailable probe's cause from `MovementGeometry` use.
    /// A new interval collection clears it. Direct `prepare_sweep` calls return
    /// their result to the caller instead of retaining it here.
    #[must_use]
    pub const fn failure(&self) -> Option<&RuntimeMovementGeometryFailure> {
        self.failure.as_ref()
    }

    /// Consumes this interval's failure after the solver returns. Coverage stays
    /// invalid until the caller begins a new interval.
    pub fn take_failure(&mut self) -> Option<RuntimeMovementGeometryFailure> {
        self.failure.take()
    }
}

impl MovementGeometry for RuntimeMovementGeometry<'_> {
    type TriangleIdentity = RuntimeMovementOwner;

    fn prepare_sweep(
        &mut self,
        volume: &MovementCollisionVolume,
        direction: Vec3,
        distance: f32,
    ) -> bool {
        if self.failure.is_some() {
            return false;
        }
        match self.prepare_sweep(volume, direction, distance) {
            Ok(RuntimeStaticMovementResidency::Ready) => true,
            Ok(pending) => {
                self.failure = Some(RuntimeMovementGeometryFailure::Pending(pending));
                false
            }
            Err(error) => {
                self.failure = Some(RuntimeMovementGeometryFailure::Invalid(error));
                false
            }
        }
    }

    fn triangles(&self) -> &[MovementCollisionTriangle] {
        self.triangles()
    }

    fn triangle_identity(&self, triangle: usize) -> Option<Self::TriangleIdentity> {
        self.owner(triangle)
    }
}
