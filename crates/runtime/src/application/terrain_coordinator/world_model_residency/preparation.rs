//! Ordered MODF and MODD service steps preserve complete scene publication.

use super::super::tile_preparation::SharedTerrainSources;
use super::super::{RuntimeTerrainError, m2_residency::ResidentM2SceneBuilder};
use super::{
    ResidentWorldModelCache, ResidentWorldModelPlacement, ResidentWorldModelScene,
    ResidentWorldModelSource, same_world_model_placement,
};
use crate::application::liquid::LiquidAssetCache;
use glam::Vec3;
use solarity_asset::{
    AssetPath, AssetStore, BlpTextureCache, DecodedTerrainTile, M2LoadDependency, M2ModelCache,
    ResourceLease, TerrainWorldModelPlacement,
};
use solarity_systems::{
    PlacedWorldModelCollision, PlacedWorldModelLiquid, WorldModelCollisionScene,
    WorldModelLiquidScene,
};
use std::{collections::HashMap, ops::ControlFlow, sync::Arc};

/// Admits the sole WDT-level MODF owner used by a global-WMO map.
pub(in super::super) fn prepare_global_world_model(
    placement: &TerrainWorldModelPlacement,
    model_cache: &mut ResidentWorldModelCache,
    m2_cache: &mut M2ModelCache,
    texture_cache: &mut BlpTextureCache,
    m2_builder: &mut ResidentM2SceneBuilder,
    liquid_assets: &mut LiquidAssetCache,
    store: &mut AssetStore,
) -> Result<
    (
        ResidentWorldModelScene,
        WorldModelCollisionScene,
        WorldModelLiquidScene,
    ),
    RuntimeTerrainError,
> {
    let mut pending = WorldModelPreparation::for_global(placement);
    while !pending.advance(
        model_cache,
        m2_cache,
        texture_cache,
        m2_builder,
        liquid_assets,
        store,
        None,
        &mut None,
    )? {}
    Ok(pending.finish())
}

/// Tile continuations pin decoded input and copy only MCRF indices. Repeated
/// chunk references never duplicate source paths or complete placement records.
enum PlacementInputs {
    Tile {
        tile: Arc<DecodedTerrainTile>,
        references: Vec<usize>,
    },
    Global(Box<TerrainWorldModelPlacement>),
}
impl PlacementInputs {
    fn len(&self) -> usize {
        match self {
            Self::Tile { references, .. } => references.len(),
            Self::Global(_) => 1,
        }
    }
    fn get(&self, index: usize) -> Option<&TerrainWorldModelPlacement> {
        match self {
            Self::Tile { tile, references } => references
                .get(index)
                .map(|&index| &tile.world_models()[index]),
            Self::Global(placement) => (index == 0).then_some(placement),
        }
    }
}

/// One selected MODF owns its ordered nested MODD continuation.
struct ActivePlacement {
    index: usize,
    source_index: usize,
    doodads: Vec<usize>,
    next: usize,
    pending: Option<M2LoadDependency>,
    model: Option<ResourceLease<solarity_asset::DecodedM2Model>>,
    materials: solarity_asset::BlpTexturePreparation,
}

/// Whole WMO membership is private until all authored placements and nested
/// resources succeed. Each turn handles one MODF admission or one MODD owner.
pub(in super::super) struct WorldModelPreparation {
    placements: PlacementInputs,
    next: usize,
    active: Option<ActivePlacement>,
    pending_root: Option<super::super::PendingWorldModel>,
    pending_source: Option<super::WorldModelSourcePreparation>,
    result: ResidentWorldModelScene,
    collision: WorldModelCollisionScene,
    liquids: WorldModelLiquidScene,
    source_indices: HashMap<AssetPath, usize>,
    placement_owners: HashMap<u32, usize>,
}

impl WorldModelPreparation {
    /// Finish only a source whose production this withdrawn scene already owns.
    /// No further MODF/MODD registration, materials or queries are constructed.
    pub(in super::super) fn retire_source_step(&mut self, store: &mut AssetStore) -> bool {
        super::super::PendingWorldModel::retire_step(&mut self.pending_root, store)
    }

    /// Pins decoded placement records in first-reference traversal order.
    pub(in super::super) fn for_tile(tile: &Arc<DecodedTerrainTile>) -> Box<Self> {
        Self::new(PlacementInputs::Tile {
            tile: Arc::clone(tile),
            references: tile
                .chunks()
                .iter()
                .flat_map(|chunk| chunk.world_model_references())
                .map(|&index| index as usize)
                .collect(),
        })
    }

    /// Global maps have one WDT-level MODF and the same nested resource rules.
    pub(in super::super) fn for_global(placement: &TerrainWorldModelPlacement) -> Box<Self> {
        Self::new(PlacementInputs::Global(Box::new(placement.clone())))
    }

    /// Owns the finite placement input while source and query products accumulate.
    fn new(placements: PlacementInputs) -> Box<Self> {
        Box::new(Self {
            placements,
            next: 0,
            active: None,
            pending_root: None,
            pending_source: None,
            result: ResidentWorldModelScene::default(),
            collision: WorldModelCollisionScene::new(),
            liquids: WorldModelLiquidScene::new(),
            source_indices: HashMap::new(),
            placement_owners: HashMap::new(),
        })
    }

    /// Shared input suspension does not advance its MODD cursor or expose a partial
    /// scene. The synchronous path drives these identical stock registration steps.
    #[allow(clippy::too_many_arguments)] // Existing cache/query owners remain independent.
    pub(in super::super) fn advance(
        &mut self,
        model_cache: &mut ResidentWorldModelCache,
        m2_cache: &mut M2ModelCache,
        texture_cache: &mut BlpTextureCache,
        m2_builder: &mut ResidentM2SceneBuilder,
        liquid_assets: &mut LiquidAssetCache,
        store: &mut AssetStore,
        shared: Option<&SharedTerrainSources>,
        suspension: &mut Option<solarity_cpu::CpuTaskDependency>,
    ) -> Result<bool, RuntimeTerrainError> {
        if let Some(active) = &mut self.active {
            let placement = self
                .placements
                .get(active.index)
                .unwrap_or_else(|| unreachable!("active MODF input remains owned"));
            let source = &self.result.sources[active.source_index];
            if let Some(&doodad_index) = active.doodads.get(active.next) {
                let doodad = &source.model().doodads()[doodad_index];
                if let Some(shared) = shared {
                    if active.model.is_none() {
                        match shared.model(doodad.path(), &mut active.pending, store)? {
                            ControlFlow::Break(model) => active.model = Some(model),
                            ControlFlow::Continue(edge) => {
                                *suspension = Some(edge);
                                return Ok(false);
                            }
                        }
                    }
                    let model = active
                        .model
                        .as_ref()
                        .unwrap_or_else(|| unreachable!("MODD material inputs remain owned"));
                    match shared.materials(
                        &mut active.materials,
                        texture_cache,
                        store,
                        |textures, store| {
                            m2_builder.add_world_model_doodad_model(
                                placement,
                                doodad_index,
                                doodad,
                                model.clone(),
                                textures,
                                store,
                            )
                        },
                    )? {
                        ControlFlow::Continue(edge) => {
                            *suspension = Some(edge);
                            return Ok(false);
                        }
                        ControlFlow::Break(()) => active.model = None,
                    }
                } else {
                    m2_builder.add_world_model_doodad(
                        placement,
                        doodad_index,
                        doodad,
                        m2_cache,
                        texture_cache,
                        store,
                    )?;
                }
                active.next += 1;
                return Ok(false);
            }
            self.active = None;
        }
        let Some(placement) = self.placements.get(self.next) else {
            return Ok(true);
        };
        let index = self.next;
        if let Some(&previous) = self.placement_owners.get(&placement.unique_id()) {
            if !same_world_model_placement(
                self.placements
                    .get(previous)
                    .unwrap_or_else(|| unreachable!("registered MODF input remains owned")),
                placement,
            ) {
                return Err(RuntimeTerrainError::ConflictingWorldModelPlacement {
                    unique_id: placement.unique_id(),
                });
            }
            self.next += 1;
            return Ok(false);
        }
        let source_index = if let Some(&index) = self.source_indices.get(placement.path()) {
            index
        } else {
            let index = self.result.sources.len();
            let source = if let Some(shared) = shared {
                if self.pending_source.is_none() {
                    let model = match shared.world_model(
                        placement.path(),
                        &mut self.pending_root,
                        store,
                    )? {
                        ControlFlow::Break(model) => model,
                        ControlFlow::Continue(edge) => {
                            *suspension = edge;
                            return Ok(false);
                        }
                    };
                    self.pending_source =
                        Some(super::WorldModelSourcePreparation::new(model, model_cache)?);
                }
                let pending = self
                    .pending_source
                    .as_mut()
                    .unwrap_or_else(|| unreachable!("WMO material inputs remain owned"));
                match pending.step(shared, texture_cache, liquid_assets, store)? {
                    ControlFlow::Continue(edge) => {
                        *suspension = Some(edge);
                        return Ok(false);
                    }
                    ControlFlow::Break(source) => {
                        self.pending_source = None;
                        source
                    }
                }
            } else {
                ResidentWorldModelSource::load(
                    placement.path(),
                    model_cache,
                    texture_cache,
                    liquid_assets,
                    store,
                )?
            };
            self.result.sources.push(source);
            self.source_indices.insert(placement.path().clone(), index);
            index
        };
        self.placement_owners.insert(placement.unique_id(), index);
        self.next += 1;
        let source = &self.result.sources[source_index];
        let position = Vec3::from_array(placement.position());
        let rotation_degrees = Vec3::from_array(placement.rotation());
        self.collision.add(PlacedWorldModelCollision::prepare(
            ResourceLease::clone(source.model()),
            position,
            rotation_degrees,
            1.0,
        )?);
        self.liquids.add(PlacedWorldModelLiquid::prepare(
            ResourceLease::clone(source.model()),
            position,
            rotation_degrees,
            1.0,
        )?);
        self.result.placements.push(ResidentWorldModelPlacement {
            source_index,
            unique_id: placement.unique_id(),
            position,
            rotation_degrees,
            name_set: placement.name_set(),
        });
        let doodads = source
            .model()
            .referenced_active_doodad_indices(placement.doodad_set())?;
        self.active = Some(ActivePlacement {
            index,
            source_index,
            doodads,
            next: 0,
            pending: None,
            model: None,
            materials: Default::default(),
        });
        Ok(false)
    }

    /// Publication receives the same ordered three products only after completion.
    pub(in super::super) fn finish(
        self,
    ) -> (
        ResidentWorldModelScene,
        WorldModelCollisionScene,
        WorldModelLiquidScene,
    ) {
        debug_assert!(self.active.is_none() && self.next == self.placements.len());
        (self.result, self.collision, self.liquids)
    }
}
