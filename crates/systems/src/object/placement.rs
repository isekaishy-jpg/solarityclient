//! GameObject passenger quaternions and shared render/collision placement.

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use solarity_ecs::{ActiveWorld, GameObjectAnimatedPose, ObjectKind, WorldObjectIdentity};
use thiserror::Error;

use super::game_object_transport_pose;

/// Decodes build 12340's signed 22/21/21-bit local quaternion (`0x00982340`).
///
/// The positive W hemisphere is implicit. Stock emits zero W when the squared
/// XYZ length differs from one by less than 2^-20; it does not normalize XYZ.
/// Invalid packed combinations outside that tolerance retain a NaN W, which
/// [`GameObjectPlacement`] rejects before rendering or collision admission.
#[must_use]
pub fn unpack_game_object_rotation(packed: u64) -> [f32; 4] {
    let x = ((packed as i64) >> 42) as f64 / 2_097_152.0;
    let y = (((packed << 22) as i64) >> 43) as f64 / 1_048_576.0;
    let z = (((packed << 43) as i64) >> 43) as f64 / 1_048_576.0;
    let squared = (y * y + x * x) + z * z;
    let w = if (squared - 1.0).abs() < 1.0 / 1_048_576.0 {
        0.0
    } else {
        (1.0 - squared).sqrt()
    };
    [x as f32, y as f32, z as f32, w as f32]
}

/// Validated GameObject placement shared by model presentation and collision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameObjectPlacement {
    matrix: Mat4,
    rotation: [f32; 4],
}

impl GameObjectPlacement {
    /// Builds the native quaternion matrix followed by the object's own scale.
    ///
    /// # Errors
    ///
    /// Returns [`GameObjectPlacementError::InvalidTransform`] for non-finite,
    /// non-positive, or singular placement inputs.
    pub fn new(
        position: Vec3,
        rotation: [f32; 4],
        scale: f32,
    ) -> Result<Self, GameObjectPlacementError> {
        if !position.is_finite()
            || !rotation.iter().all(|v| v.is_finite())
            || !scale.is_finite()
            || scale <= 0.0
        {
            return Err(GameObjectPlacementError::InvalidTransform);
        }
        let mut matrix = quaternion_matrix(rotation);
        // 0x004C1BF0 scales the three local basis rows, leaving translation.
        for column in 0..3 {
            for row in 0..3 {
                matrix[column * 4 + row] *= scale;
            }
        }
        matrix[12..15].copy_from_slice(&position.to_array());
        let matrix = Mat4::from_cols_array(&matrix);
        let determinant = matrix.determinant();
        if !matrix.is_finite() || !determinant.is_finite() || determinant == 0.0 {
            return Err(GameObjectPlacementError::InvalidTransform);
        }
        Ok(Self { matrix, rotation })
    }

    /// Returns the complete local-to-world matrix in the server Z-up basis.
    #[must_use]
    pub const fn matrix(self) -> Mat4 {
        self.matrix
    }

    /// Returns the composed world quaternion without renormalization.
    #[must_use]
    pub const fn rotation(self) -> [f32; 4] {
        self.rotation
    }

    /// Admits the transport's full matrix, retaining its separately packed rotation.
    fn animated(pose: GameObjectAnimatedPose) -> Result<Self, GameObjectPlacementError> {
        // 7134A0 sends this matrix directly to 77FDD0. The map-model placement
        // does not apply OBJECT_FIELD_SCALE_X a second time (7B5870/7B67B0).
        let matrix = pose.matrix();
        let rotation = unpack_game_object_rotation(pose.packed_rotation());
        if !rotation.into_iter().all(f32::is_finite) {
            return Err(GameObjectPlacementError::InvalidTransform);
        }
        let determinant = matrix.determinant();
        if !matrix.is_finite() || !determinant.is_finite() || determinant == 0.0 {
            return Err(GameObjectPlacementError::InvalidTransform);
        }
        Ok(Self { matrix, rotation })
    }
}

/// Placement inputs that cannot yet produce a complete GameObject matrix.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum GameObjectPlacementError {
    /// A referenced object has not entered the visible GUID registry.
    #[error("GameObject placement references missing object {guid:#018X}")]
    MissingObject {
        /// Missing owner or parent GUID.
        guid: u64,
    },
    /// A GameObject or parent has not supplied its position and scale.
    #[error("GameObject {guid:#018X} has incomplete placement inputs")]
    MissingPlacement {
        /// Owner whose replicated inputs are incomplete.
        guid: u64,
    },
    /// The referenced parent requires a different domain's placement provider.
    #[error("GameObject placement requires an unsupported {kind:?} parent {guid:#018X}")]
    UnsupportedOwner {
        /// Referenced owner GUID.
        guid: u64,
        /// Stock object category requiring its own placement provider.
        kind: ObjectKind,
    },
    /// The replicated passenger chain contains a cycle.
    #[error("GameObject passenger chain repeats {guid:#018X}")]
    PassengerCycle {
        /// First repeated GUID.
        guid: u64,
    },
    /// The final matrix would be non-finite or singular.
    #[error("GameObject placement is non-finite, non-positive, or singular")]
    InvalidTransform,
}

/// Reuses passenger-chain scratch when resolving successive world snapshots.
#[derive(Default)]
pub struct GameObjectPlacementResolver {
    chain: Vec<u64>,
    placements: HashMap<WorldObjectIdentity, CachedPlacement>,
}

#[derive(Eq, PartialEq)]
struct PlacementInputs {
    packed_rotation: u64,
    local_position: [u32; 3],
    scale: u32,
    parent: Option<([u32; 16], [u32; 4])>,
    animated_matrix: Option<[u32; 16]>,
}

struct CachedPlacement {
    inputs: PlacementInputs,
    placement: GameObjectPlacement,
}

impl GameObjectPlacementResolver {
    /// Resolves the current replicated placement through its GameObject parents.
    ///
    /// Parent motion affects passenger position through the parent's full
    /// matrix and rotation through `0x004F4320`, with each object's own scale.
    /// Behavior-owned transport matrices supersede the replicated root pose.
    ///
    /// # Errors
    ///
    /// Returns [`GameObjectPlacementError`] for missing, invalid, cyclic, or
    /// unsupported owners. It never substitutes an identity parent matrix.
    pub fn resolve(
        &mut self,
        world: &ActiveWorld,
        guid: u64,
    ) -> Result<GameObjectPlacement, GameObjectPlacementError> {
        self.resolve_with_scale(world, guid, None)
    }

    /// Constructs 7110B0/783500's initial map handle from world position and yaw.
    /// The constructor ignores the object's scale and quaternion tilt. Later
    /// route samples replace this matrix directly through 77FDD0.
    ///
    /// # Errors
    /// Returns the same dependency and transform errors as [`Self::resolve`].
    pub fn resolve_map_model_initial(
        &mut self,
        world: &ActiveWorld,
        guid: u64,
    ) -> Result<GameObjectPlacement, GameObjectPlacementError> {
        let placement = self.resolve_with_scale(world, guid, Some(1.0))?;
        let mut yaw = None;
        // 70C310 first extracts the 4F45B0 composed quaternion's angle, then
        // 4F42A0 separately adds the parent's virtual facing and wraps it.
        for &owner in self.chain.iter().rev() {
            let identity = world
                .object_identity(owner)
                .ok_or(GameObjectPlacementError::MissingObject { guid: owner })?;
            let rotation = self
                .placements
                .get(&identity)
                .ok_or(GameObjectPlacementError::MissingPlacement { guid: owner })?
                .placement
                .rotation();
            let local = quaternion_facing(rotation);
            yaw = Some(yaw.map_or(local, |parent: f32| {
                let sum = f64::from((f64::from(parent) + f64::from(local)) as f32);
                let period = f64::from(std::f32::consts::TAU);
                let remainder = sum % period;
                (if remainder < 0.0 {
                    remainder + period
                } else {
                    remainder
                }) as f32
            }));
        }
        GameObjectPlacement::animated(game_object_transport_pose(
            placement.matrix.w_axis.truncate(),
            yaw.ok_or(GameObjectPlacementError::MissingPlacement { guid })?,
            0.0,
            0.0,
        )?)
    }

    /// Initial map handles replace only the requested owner's scale; parent
    /// matrices retain their native scale when transforming the local position.
    fn resolve_with_scale(
        &mut self,
        world: &ActiveWorld,
        guid: u64,
        owner_scale: Option<f32>,
    ) -> Result<GameObjectPlacement, GameObjectPlacementError> {
        self.chain.clear();
        let mut current = guid;
        loop {
            if self.chain.contains(&current) {
                return Err(GameObjectPlacementError::PassengerCycle { guid: current });
            }
            let kind = world
                .object_kind(current)
                .ok_or(GameObjectPlacementError::MissingObject { guid: current })?;
            if kind != ObjectKind::GameObject {
                return Err(GameObjectPlacementError::UnsupportedOwner {
                    guid: current,
                    kind,
                });
            }
            self.chain.push(current);
            match world
                .game_object_movement(current)
                .and_then(|value| value.transport())
            {
                Some(transport) if transport.guid != 0 => current = transport.guid,
                _ => break,
            }
        }
        let mut parent: Option<GameObjectPlacement> = None;
        for &owner in self.chain.iter().rev() {
            let movement = world.game_object_movement(owner).unwrap_or_default();
            let animated = world.game_object_animated_pose(owner);
            let local_position = if let Some(pose) = animated {
                pose.matrix().w_axis.truncate()
            } else if parent.is_some() {
                movement
                    .transport()
                    .ok_or(GameObjectPlacementError::MissingPlacement { guid: owner })?
                    .position
            } else {
                world
                    .object_transform(owner)
                    .ok_or(GameObjectPlacementError::MissingPlacement { guid: owner })?
                    .position()
            };
            let scale = if animated.is_some() {
                1.0
            } else if let Some(scale) = owner_scale.filter(|_| owner == guid) {
                scale
            } else {
                world
                    .object_presentation(owner)
                    .ok_or(GameObjectPlacementError::MissingPlacement { guid: owner })?
                    .scale()
            };
            let identity = world
                .object_identity(owner)
                .ok_or(GameObjectPlacementError::MissingObject { guid: owner })?;
            let inputs = PlacementInputs {
                packed_rotation: animated.map_or(
                    movement.packed_rotation(),
                    GameObjectAnimatedPose::packed_rotation,
                ),
                local_position: local_position.to_array().map(f32::to_bits),
                scale: scale.to_bits(),
                parent: parent.map(|parent| {
                    (
                        parent.matrix.to_cols_array().map(f32::to_bits),
                        parent.rotation.map(f32::to_bits),
                    )
                }),
                animated_matrix: animated
                    .map(|pose| pose.matrix().to_cols_array().map(f32::to_bits)),
            };
            if let Some(cached) = self.placements.get(&identity)
                && cached.inputs == inputs
            {
                parent = Some(cached.placement);
                continue;
            }
            let local = unpack_game_object_rotation(movement.packed_rotation());
            let (position, rotation) = if let Some(parent) = parent {
                (
                    transform_point(parent.matrix, local_position),
                    compose_rotation(local, parent.rotation),
                )
            } else {
                (local_position, local)
            };
            let placement = if let Some(pose) = animated {
                GameObjectPlacement::animated(pose)?
            } else {
                GameObjectPlacement::new(position, rotation, scale)?
            };
            self.placements
                .insert(identity, CachedPlacement { inputs, placement });
            parent = Some(placement);
        }
        parent.ok_or(GameObjectPlacementError::MissingPlacement { guid })
    }

    /// Releases cached matrices for lifetimes that left this active world.
    pub fn retain_world(&mut self, world: &ActiveWorld) {
        self.placements
            .retain(|identity, _| world.object_identity(identity.guid()) == Some(*identity));
    }

    /// Retires cached placements while retaining scratch allocations for reuse.
    pub fn clear(&mut self) {
        self.chain.clear();
        self.placements.clear();
    }
}

/// 4F4630's epsilon branches preserve positive 3pi/2 at the negative Y axis.
fn quaternion_facing(rotation: [f32; 4]) -> f32 {
    let [x, y, z, w] = rotation.map(f64::from);
    let cosine = 1.0 - (y * y + z * z) * 2.0;
    let sine = (y * x + w * z) * 2.0;
    let epsilon = 1.0 / 4_194_304.0;
    let pi = f64::from(std::f32::consts::PI);
    if cosine.abs() < epsilon {
        return (if sine < 0.0 { 1.5 * pi } else { 0.5 * pi }) as f32;
    }
    if sine.abs() >= epsilon {
        return sine.atan2(cosine) as f32;
    }
    if cosine <= 0.0 { pi as f32 } else { 0.0 }
}

/// Native 0x004C1C40 preserves three spilled f32 products before later sums.
fn quaternion_matrix(rotation: [f32; 4]) -> [f32; 16] {
    let [x, y, z, w] = rotation.map(f64::from);
    let xx = x * x * 2.0;
    let xy = x * y * 2.0;
    let xz = f64::from((x * z * 2.0) as f32);
    let yy = y * y * 2.0;
    let yz = f64::from((y * z * 2.0) as f32);
    let zz = z * z * 2.0;
    let wx = w * x * 2.0;
    let wy = w * y * 2.0;
    let wz = w * z * 2.0;
    [
        (1.0 - (zz + yy)) as f32,
        (xy + wz) as f32,
        (xz - wy) as f32,
        0.0,
        (xy - wz) as f32,
        (1.0 - (f64::from(zz as f32) + xx)) as f32,
        (yz + wx) as f32,
        0.0,
        (xz + wy) as f32,
        (yz - wx) as f32,
        (1.0 - (yy + xx)) as f32,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

/// The multiplication order and store boundaries from 0x004F4320.
fn compose_rotation(local: [f32; 4], parent: [f32; 4]) -> [f32; 4] {
    let [x, y, z, w] = local.map(f64::from);
    let [px, py, pz, pw] = parent.map(f64::from);
    [
        ((pw * x + pz * y + w * px) - z * py) as f32,
        ((z * px + w * py + pw * y) - pz * x) as f32,
        ((x * py + pw * z + pz * w) - px * y) as f32,
        (((pw * w - x * px) - y * py) - pz * z) as f32,
    ]
}

fn transform_point(matrix: Mat4, point: Vec3) -> Vec3 {
    let m = matrix.to_cols_array().map(f64::from);
    let [x, y, z] = point.to_array().map(f64::from);
    Vec3::from_array(std::array::from_fn(|row| {
        (m[12 + row] + (z * m[8 + row] + y * m[4 + row] + x * m[row])) as f32
    }))
}
