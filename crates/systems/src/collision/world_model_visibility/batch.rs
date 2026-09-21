//! Native resident MOBA selection across a group's ordered stored frusta.

use super::WorldSceneFrustum;
use crate::collision::MovementCollisionBounds;

/// Retains first-accepted batch order and reusable per-group visibility state.
#[derive(Default)]
pub struct WorldModelBatchVisibilityQuery {
    selected: Vec<usize>,
    visible: Vec<bool>,
}

impl WorldModelBatchVisibilityQuery {
    /// Selects resident drawable batches from root-local bounds and clip regions.
    ///
    /// 7ABF50 replays group frusta in callback order. 7AC6A0 clears the MOBA
    /// high-nibble markers on the first frustum, then 7A7630 tests each unseen
    /// batch. A later frustum appends newly accepted batches in authored order;
    /// sorting the final indices or merging windows changes submission order.
    /// Each call starts a new group/frame. Authored batch flags remain immutable.
    /// Callers supply the signed-i16 MOBA bounds widened to f32 without a world
    /// transform, and frusta transformed into that same root-local space.
    pub fn query(
        &mut self,
        bounds: &[MovementCollisionBounds],
        frusta: &[WorldSceneFrustum],
    ) -> &[usize] {
        self.selected.clear();
        self.visible.resize(bounds.len(), false);
        self.visible.fill(false);
        select(bounds, frusta, &mut self.visible, |index| {
            self.selected.push(index);
            Ok::<_, std::convert::Infallible>(())
        })
        .unwrap_or_else(|never| match never {});
        &self.selected
    }
    /// Selects into preadmitted worker storage without allocating in the kernel.
    /// # Errors
    /// Rejects insufficient visibility or selected-index capacity.
    pub fn query_admitted<'a>(
        bounds: &[MovementCollisionBounds],
        frusta: &[WorldSceneFrustum],
        visible: &mut [bool],
        selected: &'a mut solarity_cpu::CpuBuffer<usize>,
    ) -> Result<&'a [usize], solarity_cpu::CpuError> {
        if visible.len() < bounds.len() {
            return Err(solarity_cpu::CpuError::OutputCapacity {
                requested: bounds.len(),
                available: visible.len(),
            });
        }
        selected.clear();
        visible.fill(false);
        select(bounds, frusta, visible, |index| selected.push(index))?;
        Ok(selected)
    }
}

fn select<E>(
    bounds: &[MovementCollisionBounds],
    frusta: &[WorldSceneFrustum],
    visible: &mut [bool],
    mut append: impl FnMut(usize) -> Result<(), E>,
) -> Result<(), E> {
    for frustum in frusta {
        for (index, bounds) in bounds.iter().copied().enumerate() {
            if !visible[index] && frustum.intersects_bounds(bounds) {
                visible[index] = true;
                append(index)?;
            }
        }
    }
    Ok(())
}
