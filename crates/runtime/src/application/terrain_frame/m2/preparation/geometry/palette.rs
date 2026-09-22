//! Owned render-pose overrides are independent of ordered CPU bone samples.

use super::super::super::{EffectRecords, M2ParticlePlacement, M2RibbonTrail};
use glam::Mat4;
use solarity_rendering::{M2AnimationClock, M2BonePoseOverrides, M2FingerPoseHands};

/// Retained override storage avoids allocating a separate pose task per frame.
#[derive(Default)]
pub(super) struct PaletteInput {
    pub(super) pending: bool,
    fingers: Option<(M2AnimationClock, M2FingerPoseHands)>,
    transforms: solarity_cpu::CpuBuffer<(u16, Mat4)>,
    sequences: solarity_cpu::CpuBuffer<(u16, M2AnimationClock)>,
}

impl PaletteInput {
    /// Reserves copied overrides before the placement transfers any simulation state.
    pub(super) fn prepare(
        &mut self,
        input: Option<M2BonePoseOverrides<'_>>,
        budget: &solarity_cpu::CpuStorageBudget,
        effects: Option<(
            &mut EffectRecords<M2ParticlePlacement>,
            &mut EffectRecords<M2RibbonTrail>,
        )>,
    ) -> Result<(), solarity_cpu::CpuError> {
        let (transforms, sequences) = input.as_ref().map_or((0, 0), |input| {
            (input.bone_transforms.len(), input.bone_sequences.len())
        });
        use solarity_cpu::{
            CpuStorageClass as Class, CpuStorageKind as Kind, CpuStorageWorkingSet,
        };
        let mut working_set = CpuStorageWorkingSet::default();
        working_set.include(
            self.transforms
                .reservation_bytes(budget, Class::Frame, transforms)?,
            self.transforms.replacement_credit(transforms),
        )?;
        working_set.include(
            self.sequences
                .reservation_bytes(budget, Class::Frame, sequences)?,
            self.sequences.replacement_credit(sequences),
        )?;
        if let Some((particles, ribbons)) = &effects {
            particles.include_storage(budget, &mut working_set)?;
            ribbons.include_storage(budget, &mut working_set)?;
        }
        let mut reservation = budget.reserve_working_set(Class::Frame, working_set.bytes())?;
        self.transforms
            .reserve_reserved(&mut reservation, Kind::Scratch, transforms)?;
        self.sequences
            .reserve_reserved(&mut reservation, Kind::Scratch, sequences)?;
        if let Some((particles, ribbons)) = effects {
            particles.reserve_reserved(&mut reservation)?;
            ribbons.reserve_reserved(&mut reservation)?;
        }
        self.pending = input.is_some();
        self.transforms.clear();
        self.sequences.clear();
        self.fingers = None;
        if let Some(input) = input {
            self.fingers = input.finger_pose;
            self.transforms.extend_from_slice(input.bone_transforms)?;
            self.sequences.extend_from_slice(input.bone_sequences)?;
        }
        Ok(())
    }

    pub(super) fn overrides<'a>(&'a self, orientation: &'a [bool]) -> M2BonePoseOverrides<'a> {
        M2BonePoseOverrides {
            model_oriented_billboard_bones: orientation,
            finger_pose: self.fingers,
            bone_transforms: &self.transforms,
            bone_sequences: &self.sequences,
        }
    }
}
