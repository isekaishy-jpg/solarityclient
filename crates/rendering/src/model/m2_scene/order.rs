//! Build-12340 transparent M2 scene-element ordering.

use std::cmp::Ordering;

use glam::{Mat4, Vec3};

/// Squared-length normalization guard at build-12340 address `0x009EA27C`.
const STOCK_SORT_DIRECTION_EPSILON: f32 = 2.384_185_8e-7;

/// The two translucent M2 queues submitted after stock's grouped opaque pass.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum M2TransparentPass {
    /// Ordinary translucent meshes, ribbons, and particles.
    One,
    /// Late translucent work selected by stock's special routing rules.
    Two,
}

impl M2TransparentPass {
    /// Selects the queue used by one authored particle emitter.
    ///
    /// Build 12340 maps source bit `0x2000` to runtime bit `0x40000` in
    /// `0x00832EA0`. Its sole reader at `0x00821A20` routes the particle item
    /// to pass two; it does not project the particle onto world geometry.
    #[must_use]
    pub const fn for_particle_flags(flags: u32) -> Self {
        if flags & 0x0000_2000 != 0 {
            Self::Two
        } else {
            Self::One
        }
    }
}

/// Runtime-alpha classification applied before build 12340 queues an M2 item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M2ElementAlphaState {
    /// Alpha below the scene admission epsilon produces no item or draw.
    Hidden,
    /// Fully opaque runtime alpha retains the authored material pass.
    Authored,
    /// Intermediate runtime alpha enters translucent scene handling.
    Translucent,
}

impl M2ElementAlphaState {
    /// Classifies the final color/texture-weight/instance alpha product.
    ///
    /// Build 12340 `0x00821A20` omits values below `0.0001` and promotes
    /// values below `0.99999` into translucent handling. Equality remains on
    /// the higher branch at both comparisons.
    #[must_use]
    pub const fn classify(element_alpha: f32) -> Self {
        if element_alpha < 0.000_1 {
            Self::Hidden
        } else if element_alpha < 0.999_99 {
            Self::Translucent
        } else {
            Self::Authored
        }
    }
}

/// Stable producer identity shared by ordinary particles and ribbons.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct M2EffectOrder {
    priority_plane: i16,
    producer_order: u32,
}

impl M2EffectOrder {
    /// Captures the authored plane and placement-local production order.
    #[must_use]
    pub const fn new(priority_plane: i16, producer_order: u32) -> Self {
        Self {
            priority_plane,
            producer_order,
        }
    }

    /// Returns the authored plane used for mesh/effect interleaving.
    #[must_use]
    pub const fn priority_plane(self) -> i16 {
        self.priority_plane
    }

    /// Returns stable production order after plane and blend comparisons tie.
    #[must_use]
    pub const fn producer_order(self) -> u32 {
        self.producer_order
    }
}

/// The stock fields shared by transparent mesh elements in passes one and two.
///
/// Meshes, ribbons, particles, and callbacks ultimately share this prefix. The
/// producer-specific suffix belongs to the eventual unified world scene queue;
/// this key closes the mesh-only boundary without inventing a stable FIFO tie.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2TransparentSortKey {
    primary_distance: f32,
    alternate_copy: bool,
    priority_plane: i16,
    secondary_distance: f32,
    instance_identity: usize,
    material_layer: u16,
    element_type: u8,
    producer_order: u32,
}

impl M2TransparentSortKey {
    /// Captures the exact common comparator fields of one transparent mesh.
    ///
    /// Distances may be negative for the stock radius-adjusted branch behind
    /// the view origin, but decoded and sampled inputs guarantee finiteness.
    #[must_use]
    pub const fn new(
        primary_distance: f32,
        alternate_copy: bool,
        priority_plane: i16,
        secondary_distance: f32,
        instance_identity: usize,
        material_layer: u16,
    ) -> Self {
        Self {
            primary_distance,
            alternate_copy,
            priority_plane,
            secondary_distance,
            instance_identity,
            material_layer,
            element_type: 0,
            producer_order: 0,
        }
    }

    /// Adds the stock scene-element discriminator and its producer-local tie.
    ///
    /// Build 12340 assigns mesh types `0..=2`, ribbons type `3`, and particles
    /// type `4` before sorting the common transparent queue at `0x00821A20`.
    #[must_use]
    pub const fn with_scene_element(mut self, element_type: u8, producer_order: u32) -> Self {
        self.element_type = element_type;
        self.producer_order = producer_order;
        self
    }
}

/// Replays the common prefix of build 12340's pass-one/pass-two comparator.
///
/// The executable calls an in-place heapsort, so equality deliberately remains
/// equality; callers must use an unstable sort and must not promise FIFO order.
#[must_use]
pub fn compare_m2_transparent(
    left: &M2TransparentSortKey,
    right: &M2TransparentSortKey,
) -> Ordering {
    right
        .primary_distance
        .partial_cmp(&left.primary_distance)
        .unwrap_or(Ordering::Equal)
        .then_with(|| right.alternate_copy.cmp(&left.alternate_copy))
        .then_with(|| left.priority_plane.cmp(&right.priority_plane))
        .then_with(|| {
            right
                .secondary_distance
                .partial_cmp(&left.secondary_distance)
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| left.instance_identity.cmp(&right.instance_identity))
        .then_with(|| left.element_type.cmp(&right.element_type))
        .then_with(|| {
            if left.element_type < 3 && right.element_type < 3 {
                left.material_layer.cmp(&right.material_layer)
            } else {
                Ordering::Equal
            }
        })
        .then_with(|| left.producer_order.cmp(&right.producer_order))
}

/// Computes the animated section-center distance used by M2 mesh elements.
///
/// Flags `0x1` and `0x2` select the near and far edge of the transformed sort
/// sphere. Those two branches retain the view-space Z sign after squaring.
#[must_use]
pub fn m2_section_distance_key(
    sort_center: Vec3,
    sort_radius: f32,
    batch_flags: u8,
    view_transform: Mat4,
) -> f32 {
    let mut center = view_transform.transform_point3(sort_center);
    if batch_flags & 0x3 != 0 {
        let distance_squared = center.length_squared();
        if distance_squared > STOCK_SORT_DIRECTION_EPSILON {
            let radius = sort_radius * view_transform.x_axis.truncate().length();
            let radius_offset = center * (radius / distance_squared.sqrt());
            if batch_flags & 0x1 != 0 {
                center -= radius_offset;
            } else {
                center += radius_offset;
            }
        }
        let distance_squared = center.length_squared();
        return if center.z < 0.0 {
            -distance_squared
        } else {
            distance_squared
        };
    }
    center.length_squared()
}

/// Computes the transformed model-origin key retained by multi-view M2s.
///
/// Build 12340 stores this squared distance on each instance and uses it as
/// the primary transparent key when the shared-model bit is inherited.
#[must_use]
pub fn m2_model_distance_key(model_view: Mat4) -> f32 {
    model_view.transform_point3(Vec3::ZERO).length_squared()
}
