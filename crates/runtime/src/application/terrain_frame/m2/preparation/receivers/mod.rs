//! Actual mesh and effect consumers select spatial lighting and ancestor work.

mod frame;

#[cfg(test)]
#[path = "../../../../../../tests/application/m2_receiver_demand.rs"]
mod tests;

use glam::Vec3;

use super::super::RuntimeTerrainFrameError;

/// Ancestry inputs survive until all visibility and alpha decisions are known.
#[derive(Clone, Copy)]
struct ReceiverRequest {
    parent: Option<usize>,
    center: Vec3,
    fog: Option<Vec3>,
}

/// Frame-local sparse requests; no historical pose or lighting result is cached.
#[derive(Default)]
pub(in crate::application::terrain_frame::m2) struct ReceiverFrame {
    requests: Vec<Option<ReceiverRequest>>,
    touched: Vec<usize>,
    needed: Vec<bool>,
    remap: Vec<Option<u32>>,
}

impl ReceiverFrame {
    /// Every submitted packet must retain its selected receiver; losing it is
    /// an orchestration error, never permission to use a different light bank.
    fn resolved(&self, placement: u32) -> Result<u32, RuntimeTerrainFrameError> {
        self.remap
            .get(placement as usize)
            .copied()
            .flatten()
            .ok_or(RuntimeTerrainFrameError::MissingM2Receiver { placement })
    }

    /// Clears only records used by the preceding frame, including retired indices.
    pub(in crate::application::terrain_frame::m2) fn clear(&mut self) {
        for index in self.touched.drain(..) {
            self.requests[index] = None;
            self.needed[index] = false;
            self.remap[index] = None;
        }
    }

    /// Records dependency facts without running a spatial callback or making a scene.
    pub(in crate::application::terrain_frame::m2) fn record(
        &mut self,
        placement: usize,
        parent: Option<usize>,
        center: Vec3,
        fog: Option<Vec3>,
    ) -> Result<u32, RuntimeTerrainFrameError> {
        let index = u32::try_from(placement)
            .map_err(|_| solarity_rendering::VulkanError::WorldFrameCapacity)?;
        let end = self.requests.len().max(placement + 1);
        self.requests.resize(end, None);
        self.needed.resize(end, false);
        self.remap.resize(end, None);
        if self.requests[placement].is_none() {
            self.touched.push(placement);
        }
        self.requests[placement] = Some(ReceiverRequest {
            parent,
            center,
            fog,
        });
        Ok(index)
    }

    /// Fog selection follows the admitted model's liquid and portal classification.
    pub(in crate::application::terrain_frame::m2) fn set_fog(
        &mut self,
        placement: usize,
        fog: Option<Vec3>,
    ) {
        if let Some(Some(request)) = self.requests.get_mut(placement) {
            request.fog = fog;
        }
    }

    /// A rendered child requires the callback and center of every retained ancestor.
    fn require(&mut self, placement: u32) {
        let mut current = placement as usize;
        while let Some(Some(request)) = self.requests.get(current) {
            if self.needed[current] {
                break;
            }
            self.needed[current] = true;
            let Some(parent) = request.parent else { break };
            current = parent;
        }
    }
}
