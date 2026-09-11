//! Borrowed environment caster queues and one submitted cache transition.

use glam::Vec3;

use crate::{
    M2PreparedDraw, WorldEnvironmentShadowMap, WorldEnvironmentShadowState,
    WorldEnvironmentShadowUpdate, WorldModelPreparedDraw, WorldShadowProjection,
    WorldShadowProjectionError, WorldShadowQuality,
};

/// An admitted M2 packet with its three-map membership.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldEnvironmentM2Caster {
    /// Uses the main world frame's shared animated bone palette.
    pub draw: M2PreparedDraw,
    /// Near, middle, and far membership in bits zero through two; bit three adds
    /// animated scenery to the primary map at the appropriate quality.
    pub maps: u8,
}

/// One logical WMO batch, independent of its visible material's physical passes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldEnvironmentWmoCaster {
    /// Carries the model transform, geometry range, and original first texture.
    pub draw: WorldModelPreparedDraw,
    /// Near, middle, and far membership in bits zero through two; bit three
    /// includes primary scenery at the appropriate quality.
    pub maps: u8,
    /// 7AB760 alpha-tests only MOMT blend one, with threshold 224/255.
    pub blend_mode: solarity_asset::WorldModelBlendMode,
}

/// One pending environment region with its actual caster projection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldEnvironmentShadowPass {
    projection: WorldShadowProjection,
    update: WorldEnvironmentShadowUpdate,
}

impl WorldEnvironmentShadowPass {
    /// Returns the volume used for scenery admission and rasterization.
    #[must_use]
    pub const fn projection(self) -> WorldShadowProjection {
        self.projection
    }

    /// Returns the destination texture and original normalized viewport.
    #[must_use]
    pub const fn update(self) -> WorldEnvironmentShadowUpdate {
        self.update
    }
}

/// Three retained receiver maps and this frame's environment caster packets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldEnvironmentShadowFrame<'a> {
    cache_id: u64,
    quality: WorldShadowQuality,
    receivers: [WorldShadowProjection; 3],
    buffers: [usize; 3],
    passes: [Option<WorldEnvironmentShadowPass>; 3],
    m2: &'a [WorldEnvironmentM2Caster],
    wmo: &'a [WorldEnvironmentWmoCaster],
}

impl<'a> WorldEnvironmentShadowFrame<'a> {
    /// Joins the already-advanced refresh state to its pending GPU updates.
    ///
    /// Publish the state only after successful frame submission. Caster queues
    /// can be attached after constructing these volumes for spatial admission.
    ///
    /// # Errors
    /// Rejects qualities without environment maps or invalid projection inputs.
    pub fn new(
        state: &WorldEnvironmentShadowState,
        updates: [Option<WorldEnvironmentShadowUpdate>; 3],
        origin: Vec3,
        day_night_direction: Vec3,
    ) -> Result<Self, WorldShadowProjectionError> {
        let published = state.published_maps();
        let receiver = |index: usize| {
            WorldShadowProjection::environment(
                state.quality(),
                WorldEnvironmentShadowMap::ALL[index],
                published[index].0,
                origin,
                day_night_direction,
            )
        };
        let receivers = [receiver(0)?, receiver(1)?, receiver(2)?];
        let mut passes = [None; 3];
        for (index, update) in updates.into_iter().enumerate() {
            if let Some(update) = update {
                let projection = WorldShadowProjection::environment(
                    state.quality(),
                    WorldEnvironmentShadowMap::ALL[index],
                    update.center,
                    origin,
                    day_night_direction,
                )?
                .with_caster_region(update.crop)?;
                passes[index] = Some(WorldEnvironmentShadowPass { projection, update });
            }
        }
        Ok(Self {
            cache_id: state.cache_id(),
            quality: state.quality(),
            receivers,
            buffers: published.map(|map| map.1),
            passes,
            m2: &[],
            wmo: &[],
        })
    }

    /// Attaches the material-eligible scenery packets after spatial admission.
    #[must_use]
    pub const fn with_casters(
        mut self,
        m2: &'a [WorldEnvironmentM2Caster],
        wmo: &'a [WorldEnvironmentWmoCaster],
    ) -> Self {
        self.m2 = m2;
        self.wmo = wmo;
        self
    }

    /// Returns the original cached or cascaded quality.
    #[must_use]
    pub const fn quality(self) -> WorldShadowQuality {
        self.quality
    }

    pub(in crate::device) const fn cache_id(self) -> u64 {
        self.cache_id
    }

    /// Returns the receiver projections in near-to-far order.
    #[must_use]
    pub const fn receivers(self) -> [WorldShadowProjection; 3] {
        self.receivers
    }

    /// Returns the published texture in each map's pair.
    #[must_use]
    pub const fn buffers(self) -> [usize; 3] {
        self.buffers
    }

    /// Returns this submitted frame's nonempty update regions.
    #[must_use]
    pub const fn passes(self) -> [Option<WorldEnvironmentShadowPass>; 3] {
        self.passes
    }

    /// Returns admitted M2 packets sharing the frame's bone palette.
    #[must_use]
    pub const fn m2_casters(self) -> &'a [WorldEnvironmentM2Caster] {
        self.m2
    }

    /// Returns one packet per admitted logical WMO material batch.
    #[must_use]
    pub const fn wmo_casters(self) -> &'a [WorldEnvironmentWmoCaster] {
        self.wmo
    }

    /// Counts the actual near/middle/far updates followed by primary scenery.
    pub(in crate::device) fn draw_counts(self) -> [usize; 4] {
        std::array::from_fn(|index| {
            if index < 3 && self.passes[index].is_none() {
                return 0;
            }
            let mask = 1 << index;
            self.wmo
                .iter()
                .filter(|caster| caster.maps & mask != 0)
                .count()
                + self
                    .m2
                    .iter()
                    .filter(|caster| {
                        caster.maps & mask != 0 && caster.draw.shadow_material().is_some()
                    })
                    .count()
        })
    }
}
