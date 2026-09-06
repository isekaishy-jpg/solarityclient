//! Complete default-set WMO resources owned by replicated GameObjects.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use glam::Mat4;
use solarity_asset::{
    AssetPath, AssetStore, BlpTextureCache, DecodedWorldModel, M2ModelCache, WmoModelCache,
};

use crate::application::model_playback::M2Playback;
use crate::application::terrain_coordinator::RuntimeTerrainError;
use crate::application::terrain_coordinator::m2_residency::{
    ResidentM2Source, world_model_doodad_transform,
};
use crate::application::terrain_coordinator::world_model_residency::ResidentWorldModelSource;
use crate::application::terrain_frame::RuntimeTerrainFrameError;
use crate::random::CrtRand;

pub(in crate::application) struct GameObjectWorldModelDoodad {
    pub index: usize,
    pub source: Arc<ResidentM2Source>,
    pub local_transform: Mat4,
}

/// Root materials and the active MODD generation finish on the same worker.
pub(in crate::application) struct GameObjectWorldModelSource {
    root: ResidentWorldModelSource,
    doodads: Vec<GameObjectWorldModelDoodad>,
}

impl GameObjectWorldModelSource {
    pub(super) fn load(
        path: &AssetPath,
        roots: &mut WmoModelCache,
        models: &mut M2ModelCache,
        textures: &mut BlpTextureCache,
        store: &mut AssetStore,
    ) -> Result<Self, RuntimeTerrainError> {
        let root = ResidentWorldModelSource::load(path, roots, textures, store)?;
        let mut sources = HashMap::<AssetPath, Arc<ResidentM2Source>>::new();
        let mut doodads = Vec::new();
        for index in root.model().referenced_active_doodad_indices(0)? {
            let doodad = &root.model().doodads()[index];
            let model = models.load(store, doodad.path())?;
            let source = if let Some(source) = sources.get(model.path()) {
                Arc::clone(source)
            } else {
                let source = Arc::new(ResidentM2Source::load(
                    doodad.path(),
                    models,
                    textures,
                    store,
                )?);
                sources.insert(model.path().clone(), Arc::clone(&source));
                source
            };
            doodads.push(GameObjectWorldModelDoodad {
                index,
                source,
                local_transform: world_model_doodad_transform(doodad)?,
            });
        }
        Ok(Self { root, doodads })
    }

    pub(in crate::application) const fn root(&self) -> &ResidentWorldModelSource {
        &self.root
    }

    pub(in crate::application) fn model(&self) -> &Arc<DecodedWorldModel> {
        self.root.model()
    }

    pub(in crate::application) fn doodads(&self) -> &[GameObjectWorldModelDoodad] {
        &self.doodads
    }
}

/// One replicated root's independently started MODD timers, separate from GPU lifetime.
pub(in crate::application) struct GameObjectWorldModelState {
    display_id: u32,
    model: Arc<DecodedWorldModel>,
    playback: HashMap<usize, Option<Rc<RefCell<M2Playback>>>>,
}

impl GameObjectWorldModelState {
    pub(super) fn new(
        source: &GameObjectWorldModelSource,
        display_id: u32,
        scene_time_ms: u32,
        random: &mut CrtRand,
    ) -> Result<Self, RuntimeTerrainFrameError> {
        let mut playback = HashMap::new();
        for doodad in source.doodads() {
            playback.insert(
                doodad.index,
                M2Playback::new_at(doodad.source.model(), 0, scene_time_ms as f32, random)?
                    .map(|playback| Rc::new(RefCell::new(playback))),
            );
        }
        Ok(Self {
            display_id,
            model: Arc::clone(source.model()),
            playback,
        })
    }

    pub(super) fn matches(&self, source: &GameObjectWorldModelSource, display_id: u32) -> bool {
        self.display_id == display_id && Arc::ptr_eq(&self.model, source.model())
    }

    pub(in crate::application) fn playback(&self, index: usize) -> Option<Rc<RefCell<M2Playback>>> {
        self.playback.get(&index).and_then(Clone::clone)
    }
}
