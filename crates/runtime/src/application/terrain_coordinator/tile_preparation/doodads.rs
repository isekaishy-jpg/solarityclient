//! Ordered MDDF registration and shared model dependency consumption.

use super::super::{
    RuntimeTerrainError, m2_residency::ResidentM2SceneBuilder, same_doodad_placement,
};
use solarity_asset::{AssetStore, BlpTextureCache, DecodedTerrainTile, M2ModelCache};
use std::collections::HashMap;

/// Retains MDDF table order and duplicate validation across placement boundaries.
pub(super) struct DoodadCursor {
    referenced: Vec<bool>,
    next: usize,
    placements: HashMap<u32, usize>,
    pending: Option<(usize, solarity_asset::M2LoadDependency)>,
}

impl DoodadCursor {
    /// Strict ADT decoding has already validated every MCRF index.
    pub(super) fn new(tile: &DecodedTerrainTile) -> Self {
        let mut referenced = vec![false; tile.doodads().len()];
        for reference in tile
            .chunks()
            .iter()
            .flat_map(|chunk| chunk.doodad_references())
        {
            referenced[*reference as usize] = true;
        }
        Self {
            referenced,
            next: 0,
            placements: HashMap::new(),
            pending: None,
        }
    }

    /// Adds at most one referenced placement; true means all MDDF entries finished.
    #[allow(clippy::too_many_arguments)] // Cache and continuation owners have separate lifetimes.
    pub(super) fn advance(
        &mut self,
        tile: &DecodedTerrainTile,
        builder: &mut ResidentM2SceneBuilder,
        models: &mut M2ModelCache,
        textures: &mut BlpTextureCache,
        store: &mut AssetStore,
        shared: Option<&super::SharedTerrainSources>,
        suspension: &mut Option<solarity_cpu::CpuTaskDependency>,
    ) -> Result<bool, RuntimeTerrainError> {
        if let Some((index, dependency)) = self.pending.take() {
            // The scheduler resumes only after durable source publication. The
            // enclosing terrain task handles withdrawal before calling advance.
            let model = dependency.poll().unwrap_or_else(|| {
                unreachable!("terrain source consumption follows dependency readiness")
            })?;
            builder.add_terrain_doodad_model(&tile.doodads()[index], model, textures, store)?;
            return Ok(self.next == tile.doodads().len());
        }
        while self.next < tile.doodads().len() {
            let index = self.next;
            self.next += 1;
            if !self.referenced[index] {
                continue;
            }
            let placement = &tile.doodads()[index];
            if let Some(previous) = self.placements.insert(placement.unique_id(), index) {
                if !same_doodad_placement(&tile.doodads()[previous], placement) {
                    return Err(RuntimeTerrainError::ConflictingDoodadPlacement {
                        unique_id: placement.unique_id(),
                    });
                }
            } else {
                if let Some(shared) = shared {
                    let mut pending = None;
                    let model = match shared.model(placement.path(), &mut pending, store)? {
                        std::ops::ControlFlow::Break(model) => model,
                        std::ops::ControlFlow::Continue(dependency) => {
                            *suspension = Some(dependency);
                            self.pending = Some((
                                index,
                                pending.unwrap_or_else(|| {
                                    unreachable!("a suspended source retains its typed dependency")
                                }),
                            ));
                            return Ok(false);
                        }
                    };
                    builder.add_terrain_doodad_model(placement, model, textures, store)?;
                } else {
                    builder.add_terrain_doodad(placement, models, textures, store)?;
                }
            }
            return Ok(self.next == tile.doodads().len());
        }
        Ok(true)
    }
}
