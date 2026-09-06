//! One borrowed movement context and its per-sweep world collection cache.

use glam::Vec3;
use solarity_ecs::ActiveWorld;
use solarity_systems::{
    MovementBspCacheMode, MovementCollisionBounds, MovementCollisionTriangle,
    MovementCollisionVolume, MovementIntervalRequest,
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
/// This adapter accepts world-space probes; it does not resolve passenger space,
/// liquid/WDL policy, or specialized GameObject behavior.
pub struct RuntimeMovementGeometry<'a> {
    terrain: &'a mut RuntimeTerrainCoordinator,
    world: &'a ActiveWorld,
    objects: &'a RuntimeGameObjectPresentation,
    flags: u32,
    cache: MovementBspCacheMode,
    output: &'a mut RuntimeMovementQuery,
    bounds: Option<MovementCollisionBounds>,
}

impl<'a> RuntimeMovementGeometry<'a> {
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
        }
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
        let residency = self.terrain.collect_movement_interval(
            self.world,
            self.objects,
            request,
            self.flags,
            self.cache,
            self.output,
        )?;
        self.bounds = self.output.interval_bounds().map(|bounds| bounds.query());
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
        let refresh = match volume.sweep_refresh_bounds(direction, distance, cached) {
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
            self.bounds = Some(refresh);
        }
        Ok(residency)
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
}
