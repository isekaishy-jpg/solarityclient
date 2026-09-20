//! Stable packet ordering uses compact admitted indices, never large-record merge scratch.

use super::super::super::super::{
    M2TransparentDrawIndex, M2TransparentPass, RuntimeTerrainFrameError, compare_m2_transparent,
    scene_element_count,
};
use super::streams::FinalStreams;

impl FinalStreams {
    /// Assigns transparent scene order once while receiver lighting can still run.
    pub(super) fn order(
        &mut self,
        first_transparent_pass: M2TransparentPass,
        scratch: &mut solarity_cpu::FixedWriter<'_, usize>,
    ) -> Result<u32, RuntimeTerrainFrameError> {
        self.transparent_elements.sort_unstable_by(|left, right| {
            (left.pass != first_transparent_pass)
                .cmp(&(right.pass != first_transparent_pass))
                .then_with(|| compare_m2_transparent(&left.key, &right.key))
        });
        let first_transparent_order = scene_element_count(
            self.visible_draws.len(),
            self.particle_draws.len(),
            self.ribbon_draws.len(),
        )?;
        let water_scene_order = first_transparent_order
            .checked_add(
                self.transparent_elements
                    .iter()
                    .take_while(|element| element.pass == first_transparent_pass)
                    .count(),
            )
            .and_then(|index| u32::try_from(index).ok())
            .ok_or(solarity_rendering::VulkanError::M2DrawIndexRange)?;
        for (index, element) in self.transparent_elements.iter().enumerate() {
            let scene_order = first_transparent_order
                .checked_add(index)
                .and_then(|index| u32::try_from(index).ok())
                .ok_or(solarity_rendering::VulkanError::M2DrawIndexRange)?;
            match element.draw {
                M2TransparentDrawIndex::Mesh(draw_index) => {
                    let draw = self
                        .visible_draws
                        .get_mut(draw_index)
                        .ok_or(solarity_rendering::VulkanError::M2DrawIndexRange)?;
                    *draw = draw.with_scene_order(scene_order);
                }
                M2TransparentDrawIndex::Particle(draw_index) => {
                    let draw = self
                        .particle_draws
                        .get_mut(draw_index)
                        .ok_or(solarity_rendering::VulkanError::M2ParticleDrawIndexRange)?;
                    *draw = draw.with_scene_order(scene_order);
                }
                M2TransparentDrawIndex::Ribbon { first, count } => {
                    let draws = self
                        .ribbon_draws
                        .get_mut(first..first + count)
                        .ok_or(solarity_rendering::VulkanError::M2RibbonDrawVertexRange)?;
                    for draw in draws {
                        *draw = draw.with_scene_order(scene_order);
                    }
                }
            }
        }
        stable_scene_order(&mut self.visible_draws, scratch, |draw| draw.scene_order())?;
        stable_scene_order(&mut self.particle_draws, scratch, |draw| draw.scene_order())?;
        stable_scene_order(&mut self.ribbon_draws, scratch, |draw| draw.scene_order())?;
        Ok(water_scene_order)
    }
}

/// Original ordinal breaks equal keys. Cycle application moves each record only
/// a bounded number of times and leaves no scratch values after the scope ends.
pub(super) fn stable_scene_order<T>(
    draws: &mut [T],
    scratch: &mut solarity_cpu::FixedWriter<'_, usize>,
    key: impl Fn(&T) -> u32,
) -> Result<(), solarity_cpu::CpuError> {
    scratch.clear();
    scratch.require(draws.len())?;
    for index in 0..draws.len() {
        scratch.push(index)?;
    }
    scratch.sort_unstable_by_key(|&index| (key(&draws[index]), index));
    for start in 0..draws.len() {
        let mut current = start;
        loop {
            let next = scratch[current];
            scratch[current] = current;
            if next == start {
                break;
            }
            draws.swap(current, next);
            current = next;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../../../../../tests/application/m2_output_ordering.rs"]
mod tests;
