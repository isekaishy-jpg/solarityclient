//! Native map-handle containment used when an active passenger loses contact.

use glam::Vec3;

use crate::collision::MovementCollisionBounds;

/// Borrowed retention geometry from `0x0070B360`/`0x0077FFB0`.
/// Coordinates are already in the passenger's parent space. This predicate
/// retains an existing parent; it does not grant permission to board one.
#[derive(Clone, Copy, Debug)]
pub enum MovementTransportVolume<'a> {
    /// No map handle, or a handle with neither the WMO nor M2 family bit.
    Unbounded,
    /// Authored M2 collision bounds; absent while its model is missing or unready.
    Model(Option<MovementCollisionBounds>),
    /// A WMO requires loaded groups before testing its authored MCVP planes.
    WorldModel {
        /// Number of loaded groups, independent of the plane count.
        loaded_groups: usize,
        /// Authored local-space plane coefficients, in their original order.
        planes: &'a [[f32; 4]],
    },
}

impl MovementTransportVolume<'_> {
    /// Tests native inclusive half-spaces, including the M2 upper-Z padding.
    #[must_use]
    pub fn contains(self, position: Vec3) -> bool {
        match self {
            Self::Unbounded => true,
            Self::Model(None) => false,
            Self::Model(Some(bounds)) => {
                let minimum = bounds.minimum();
                let maximum = bounds.maximum();
                // 77FFB0 stores this plane's offset once, after both x87
                // subtractions; adding a rounded height to Z changes the edge.
                let top = ((-f64::from(maximum.z) - f64::from(f32::from_bits(0x3fd1_f908)))
                    - f64::from(f32::from_bits(0x3c63_8e39))) as f32;
                [
                    [1., 0., 0., -maximum.x],
                    [0., 1., 0., -maximum.y],
                    [0., 0., 1., top],
                    [-1., 0., 0., minimum.x],
                    [0., -1., 0., minimum.y],
                    [0., 0., -1., minimum.z],
                ]
                .into_iter()
                .all(|plane| !outside(plane, position))
            }
            Self::WorldModel {
                loaded_groups,
                planes,
            } => loaded_groups != 0 && planes.iter().all(|&plane| !outside(plane, position)),
        }
    }
}

/// Both native loops add Y, Z, X, then D without an intermediate float spill.
fn outside(plane: [f32; 4], position: Vec3) -> bool {
    let [a, b, c, d] = plane.map(f64::from);
    ((b * f64::from(position.y) + c * f64::from(position.z)) + a * f64::from(position.x)) + d > 0.
}
