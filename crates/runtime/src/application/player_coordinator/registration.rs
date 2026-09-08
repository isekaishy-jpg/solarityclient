//! Unit render bounds used by the native liquid registration gate.

use glam::{Mat4, Vec3};
use solarity_asset::DecodedM2Model;
use solarity_ecs::{WorldObjectIdentity, WorldTransform};
use solarity_systems::{MovementCollectionError, MovementCollisionBounds};

use super::{ResidentPlayerModel, RuntimePlayerPresentation};

impl RuntimePlayerPresentation {
    pub(in crate::application) fn unit_effect_owner(
        &self,
        identity: WorldObjectIdentity,
    ) -> Option<&std::rc::Rc<super::super::unit_animation::UnitAnimationBehavior>> {
        self.unit_animations
            .get(identity.guid())
            .filter(|owner| owner.identity() == identity)
    }
    /// Borrows each CPU-resident body without tying unit effects to GPU uploads.
    pub(in crate::application) fn unit_effect_models(
        &self,
    ) -> impl Iterator<Item = (WorldObjectIdentity, &std::sync::Arc<DecodedM2Model>)> {
        self.resident
            .iter()
            .chain(&self.remote_players)
            .map(|player| (player.identity, &player.model))
            .chain(
                self.creatures_resident
                    .iter()
                    .map(|creature| (creature.key.identity, &creature.model)),
            )
    }

    /// 7370D0 selects the mount when present and transforms its recursive
    /// render box. An absent/unready model registers only the unit position.
    pub(in crate::application) fn liquid_registration_top(
        &self,
        identity: WorldObjectIdentity,
        transform: WorldTransform,
    ) -> Result<f32, MovementCollectionError> {
        let (bounds, scale) = if let Some(player) = self
            .resident
            .iter()
            .chain(&self.remote_players)
            .find(|player| player.identity == identity)
        {
            let body = player_bounds(player);
            if let Some(mount) = &player.mount {
                (with_children(&mount.model, [body]), mount.object_scale)
            } else {
                (body, player.object_scale)
            }
        } else if let Some(creature) = self
            .creatures_resident
            .iter()
            .find(|creature| creature.key.identity == identity)
        {
            (
                with_children(&creature.model, []),
                creature.key.object_scale,
            )
        } else {
            return Ok(transform.position().z);
        };
        // 780240 requires positive extent on every axis before 984860.
        if !bounds[0].cmplt(bounds[1]).all() {
            return Ok(transform.position().z);
        }
        let matrix = Mat4::from_translation(transform.position())
            * Mat4::from_rotation_z(transform.orientation())
            * Mat4::from_scale(Vec3::splat(scale));
        Ok(MovementCollisionBounds::new(bounds[0], bounds[1])?
            .transformed(matrix)?
            .maximum()
            .z)
    }
}

fn player_bounds(player: &ResidentPlayerModel) -> [Vec3; 2] {
    with_children(
        &player.model,
        player.attachments.iter().map(|attachment| {
            with_children(
                &attachment.model,
                attachment
                    .visual_effects
                    .iter()
                    .map(|effect| with_children(&effect.model, [])),
            )
        }),
    )
}

fn with_children(
    model: &DecodedM2Model,
    children: impl IntoIterator<Item = [Vec3; 2]>,
) -> [Vec3; 2] {
    let header = model.bounds();
    expand_children([header.minimum(), header.maximum()], children)
}

/// 8254F0 expands the original parent header by each child's complete extent,
/// then unions the candidates. Attachment pose and scale do not enter this box.
fn expand_children(header: [Vec3; 2], children: impl IntoIterator<Item = [Vec3; 2]>) -> [Vec3; 2] {
    let mut bounds = header;
    for child in children {
        if child[0].cmple(child[1]).any() {
            for axis in 0..3 {
                let extent = f64::from(child[1][axis]) - f64::from(child[0][axis]);
                bounds[0][axis] = bounds[0][axis].min((f64::from(header[0][axis]) - extent) as f32);
                bounds[1][axis] = bounds[1][axis].max((f64::from(header[1][axis]) + extent) as f32);
            }
        }
    }
    bounds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recursive_bounds_match_original_model_code() {
        let fixture = include_bytes!("../../../tests/fixtures/unit_render_bounds.bin");
        assert_eq!(fixture.len(), 256 * 144);
        for (case, record) in fixture.as_chunks::<144>().0.iter().enumerate() {
            let boxes: [[Vec3; 2]; 6] = std::array::from_fn(|index| {
                std::array::from_fn(|corner| {
                    Vec3::from_array(std::array::from_fn(|axis| {
                        let offset = index * 24 + corner * 12 + axis * 4;
                        f32::from_le_bytes(std::array::from_fn(|byte| record[offset + byte]))
                    }))
                })
            });
            let first = expand_children(boxes[1], [boxes[3]]);
            let second = expand_children(boxes[2], [boxes[4]]);
            let actual = expand_children(boxes[0], [first, second]);
            for (actual, expected) in actual.into_iter().zip(boxes[5]) {
                assert_eq!(
                    actual.to_array().map(f32::to_bits),
                    expected.to_array().map(f32::to_bits),
                    "native recursive bounds case {case}"
                );
            }
        }
    }
}
