//! Native minimap slots, asynchronous archive residency, and retained GPU draws.

#[cfg(test)]
#[path = "../../../tests/application/minimap.rs"]
mod tests;

use std::collections::{HashMap, HashSet};

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetStore, BlpTextureSource, MinimapTextureCatalog,
    TerrainMap,
};
use solarity_cpu::{CpuError, CpuExecutor, CpuTask};
use solarity_ecs::WorldTransform;
use solarity_rendering::{
    BlpColorSpace, BlpTextureHandle, BlpTextureUploadRequest, MinimapView, UiMeshPlan,
    UiPreparedDraw, UiRenderSource, VulkanRenderer,
};
use solarity_ui::{FrameManager, UiMinimapPresentation};

use super::{ApplicationError, RuntimeUiFrame, RuntimeWorldUiError};
use crate::application::ui_frame::PreparedUiFrame;

struct TextureCompletion {
    store: AssetStore,
    sources: Vec<(AssetPath, Result<BlpTextureSource, AssetError>)>,
}

struct MinimapSlot {
    object_index: usize,
    insertion: usize,
    combined_offset: usize,
    combined_count: usize,
    widget: UiMinimapPresentation,
    translation: [f32; 2],
    clip: Option<[f32; 4]>,
    opacity: f32,
    frame: Option<PreparedUiFrame>,
    active: bool,
    needs_update: bool,
}

impl MinimapSlot {
    fn draws(&self) -> &[UiPreparedDraw] {
        if self.active {
            self.frame.as_ref().map_or(&[], PreparedUiFrame::draws)
        } else {
            &[]
        }
    }
}

pub(super) struct RuntimeMinimapScene {
    catalog: MinimapTextureCatalog,
    archive_catalog: ArchiveCatalog,
    worker_store: Option<AssetStore>,
    pending: Option<CpuTask<Result<TextureCompletion, AssetError>>>,
    request_deferred: bool,
    textures: HashMap<AssetPath, BlpTextureHandle>,
    failed: HashSet<AssetPath>,
    default_mask: AssetPath,
    corpse_icon: AssetPath,
    corpse_arrow: AssetPath,
    slots: Vec<MinimapSlot>,
    /// Hidden frames can disappear from a full UI topology rebuild. Their GPU
    /// storage remains owned by the same stable Lua object on its next reveal.
    dormant_frames: HashMap<usize, PreparedUiFrame>,
    combined: Vec<UiPreparedDraw>,
    ui_revision: Option<u64>,
    scene_revision: Option<u64>,
    world: Option<WorldTransform>,
    map_id: Option<u32>,
    rotate: bool,
}

impl RuntimeMinimapScene {
    pub(super) fn ready(&self) -> bool {
        self.ui_revision.is_some()
            && self.scene_revision.is_some()
            && self.pending.is_none()
            && !self.request_deferred
    }
    pub(super) fn new(
        store: &mut AssetStore,
        archive_catalog: ArchiveCatalog,
    ) -> Result<Self, AssetError> {
        Ok(Self {
            catalog: MinimapTextureCatalog::load(store)?,
            archive_catalog,
            worker_store: None,
            pending: None,
            request_deferred: false,
            textures: HashMap::new(),
            failed: HashSet::new(),
            default_mask: AssetPath::new("Textures\\MinimapMask.blp")?,
            corpse_icon: AssetPath::new("Interface/Minimap/ObjectIcons.blp")?,
            corpse_arrow: AssetPath::new("Interface/Minimap/Rotating-MinimapCorpseArrow.blp")?,
            slots: Vec::new(),
            dormant_frames: HashMap::new(),
            combined: Vec::new(),
            ui_revision: None,
            scene_revision: None,
            world: None,
            map_id: None,
            rotate: false,
        })
    }

    pub(super) fn draws<'a>(&'a self, ui: &'a RuntimeUiFrame) -> &'a [UiPreparedDraw] {
        if self.ui_revision.is_some() {
            &self.combined
        } else {
            ui.draws()
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn synchronize(
        &mut self,
        renderer: &mut VulkanRenderer,
        cpu: &CpuExecutor,
        manager: &FrameManager,
        ui: &RuntimeUiFrame,
        ui_revision: u64,
        map: Option<&TerrainMap>,
        world: Option<WorldTransform>,
    ) -> Result<(), ApplicationError> {
        let uploaded = self.poll_textures(renderer)?;
        let ui_changed = self.ui_revision != Some(ui_revision);
        if ui_changed {
            let mut previous = std::mem::take(&mut self.slots);
            for (batch_index, batch) in manager.render_plan().mesh().batches().iter().enumerate() {
                let UiRenderSource::Minimap(object_index) = batch.source() else {
                    continue;
                };
                let Some(widget) = manager.minimap_presentation(*object_index) else {
                    continue;
                };
                if let Some(index) = previous
                    .iter()
                    .position(|slot| slot.object_index == *object_index)
                {
                    let mut slot = previous.swap_remove(index);
                    slot.insertion = ui.draw_insertion_index(batch_index);
                    slot.needs_update |= slot.widget != widget
                        || slot.translation != batch.translation()
                        || slot.clip != batch.clip()
                        || slot.opacity != batch.opacity();
                    slot.widget = widget;
                    slot.translation = batch.translation();
                    slot.clip = batch.clip();
                    slot.opacity = batch.opacity();
                    self.slots.push(slot);
                    continue;
                }
                let frame = self.dormant_frames.remove(object_index);
                self.slots.push(MinimapSlot {
                    object_index: *object_index,
                    insertion: ui.draw_insertion_index(batch_index),
                    combined_offset: 0,
                    combined_count: 0,
                    widget,
                    translation: batch.translation(),
                    clip: batch.clip(),
                    opacity: batch.opacity(),
                    frame,
                    active: false,
                    needs_update: true,
                });
            }
            for slot in previous {
                if let Some(frame) = slot.frame {
                    self.dormant_frames.insert(slot.object_index, frame);
                }
            }
        }
        let state = manager.minimap_state();
        let rotate = manager
            .cvar_number("rotateMinimap")
            .is_some_and(|value| value != 0.0);
        let scene_changed = uploaded
            || self.world != world
            || self.map_id != map.map(TerrainMap::map_id)
            || self.scene_revision != Some(state.revision())
            || self.rotate != rotate;
        let changed = ui_changed || scene_changed;
        if !changed && !self.request_deferred {
            return Ok(());
        }
        // Retrying a saturated executor still requires discovering the current
        // missing images, even if movement and Lua state did not change.
        let mask = state.mask().unwrap_or_else(|| self.default_mask.clone());
        let mut requested = Vec::new();
        let mut topology_changed = ui_changed;
        for slot in &mut self.slots {
            let slot_changed = scene_changed || slot.needs_update;
            if !slot_changed && !self.request_deferred {
                continue;
            }
            let mut quads = Vec::with_capacity(6);
            if let (Some(map), Some(world)) = (map, world)
                && slot.opacity > 0.0
            {
                let bounds = slot.widget.bounds();
                let [dx, dy] = slot.translation;
                let bounds = [
                    bounds.left() as f32 + dx,
                    bounds.bottom() as f32 + dy,
                    bounds.right() as f32 + dx,
                    bounds.top() as f32 + dy,
                ];
                if bounds[0] < bounds[2] && bounds[1] < bounds[3] {
                    let position = world.position();
                    let facing = world.orientation();
                    let view = MinimapView::new(
                        [position.x, position.y],
                        state.radius(),
                        if rotate { facing } else { 0.0 },
                        bounds,
                    )
                    .map_err(RuntimeWorldUiError::from)?;
                    let clip = slot.clip.map_or(bounds, |clip| {
                        [
                            bounds[0].max(clip[0]),
                            bounds[1].max(clip[1]),
                            bounds[2].min(clip[2]),
                            bounds[3].min(clip[3]),
                        ]
                    });
                    if clip[0] < clip[2] && clip[1] < clip[3] {
                        for tile in view.terrain_tiles() {
                            if let Some(path) =
                                self.catalog.terrain_texture(map.directory(), tile)?
                            {
                                requested.push(path.clone());
                                requested.push(mask.clone());
                                if self.textures.contains_key(path)
                                    && self.textures.contains_key(&mask)
                                {
                                    quads.push(
                                        view.terrain_quad(
                                            slot.object_index,
                                            tile,
                                            path.clone(),
                                            mask.clone(),
                                        )
                                        .with_opacity(slot.opacity)
                                        .with_clip(clip),
                                    );
                                }
                            }
                        }
                        if let Some(corpse) = view.corpse_quad(
                            slot.object_index,
                            state.corpse(),
                            slot.widget.effective_scale(),
                            self.corpse_icon.clone(),
                            self.corpse_arrow.clone(),
                        ) && let UiRenderSource::Texture(path) = corpse.source()
                        {
                            requested.push(path.clone());
                            if self.textures.contains_key(path) {
                                quads.push(corpse.with_opacity(slot.opacity).with_clip(clip));
                            }
                        }
                        let player = slot.widget.player_texture();
                        if slot
                            .widget
                            .player_size()
                            .into_iter()
                            .all(|value| value > 0.0)
                        {
                            requested.push(player.clone());
                            if self.textures.contains_key(player) {
                                quads.push(
                                    view.player_quad(
                                        slot.object_index,
                                        player.clone(),
                                        slot.widget.player_size(),
                                        facing,
                                    )
                                    .with_opacity(slot.opacity)
                                    .with_clip(clip),
                                );
                            }
                        }
                    }
                }
            }
            if slot_changed {
                slot.active = !quads.is_empty();
                if slot.active {
                    let plan = UiMeshPlan::prepare(ui.logical_extent(), quads.into_iter())
                        .map_err(solarity_ui::UiRenderError::from)?;
                    match &mut slot.frame {
                        Some(frame) => {
                            if !frame.try_replace_compatible_mesh(renderer, &plan)? {
                                *frame = PreparedUiFrame::prepare_reusing_mesh(
                                    renderer,
                                    frame.mesh(),
                                    &plan,
                                    &self.textures,
                                    None,
                                )?;
                            }
                        }
                        None => {
                            slot.frame = Some(PreparedUiFrame::prepare(
                                renderer,
                                &plan,
                                &self.textures,
                                None,
                            )?)
                        }
                    }
                }
                topology_changed |= slot.draws().len() != slot.combined_count;
                slot.needs_update = false;
            }
        }
        self.request_textures(cpu, requested)?;
        if topology_changed {
            self.combined.clear();
            let mut first = 0;
            for slot in &mut self.slots {
                self.combined
                    .extend_from_slice(&ui.draws()[first..slot.insertion]);
                slot.combined_offset = self.combined.len();
                self.combined.extend_from_slice(slot.draws());
                slot.combined_count = self.combined.len() - slot.combined_offset;
                first = slot.insertion;
            }
            self.combined.extend_from_slice(&ui.draws()[first..]);
        } else if changed {
            for slot in &self.slots {
                self.combined[slot.combined_offset..slot.combined_offset + slot.combined_count]
                    .copy_from_slice(slot.draws());
            }
        }
        self.ui_revision = Some(ui_revision);
        self.scene_revision = Some(state.revision());
        self.world = world;
        self.map_id = map.map(TerrainMap::map_id);
        self.rotate = rotate;
        Ok(())
    }

    fn request_textures(
        &mut self,
        cpu: &CpuExecutor,
        mut paths: Vec<AssetPath>,
    ) -> Result<(), ApplicationError> {
        if self.pending.is_some() {
            return Ok(());
        }
        self.request_deferred = false;
        paths.retain(|path| !self.textures.contains_key(path) && !self.failed.contains(path));
        let mut unique = HashSet::new();
        paths.retain(|path| unique.insert(path.clone()));
        if paths.is_empty() {
            return Ok(());
        }
        let catalog = self.archive_catalog.clone();
        let worker = self.worker_store.take();
        self.pending = match cpu.try_submit(move || {
            let mut store = match worker {
                Some(store) => store,
                None => AssetStore::mount(catalog)?,
            };
            let sources = paths
                .into_iter()
                .map(|path| {
                    let source = BlpTextureSource::load(&mut store, &path);
                    (path, source)
                })
                .collect();
            Ok(TextureCompletion { store, sources })
        }) {
            Ok(task) => Some(task),
            Err(CpuError::AtCapacity { .. }) => {
                self.request_deferred = true;
                None
            }
            Err(error) => return Err(error.into()),
        };
        Ok(())
    }

    fn poll_textures(&mut self, renderer: &mut VulkanRenderer) -> Result<bool, ApplicationError> {
        if self.pending.as_ref().is_none_or(|task| !task.is_finished()) {
            return Ok(false);
        }
        let Some(task) = self.pending.take() else {
            return Ok(false);
        };
        self.scene_revision = None;
        let completion = task.join()??;
        self.worker_store = Some(completion.store);
        let mut sources = Vec::new();
        for (path, source) in completion.sources {
            match source {
                Ok(source) => sources.push((path, source)),
                Err(error) => {
                    self.failed.insert(path);
                    if !matches!(error, AssetError::AssetNotFound { .. }) {
                        return Err(error.into());
                    }
                }
            }
        }
        if sources.is_empty() {
            return Ok(true);
        }
        let uploads = sources
            .iter()
            .map(|(_, source)| BlpTextureUploadRequest::new(source, BlpColorSpace::Linear))
            .collect::<Vec<_>>();
        let handles = renderer.upload_blp_textures(&uploads)?;
        for ((path, _), handle) in sources.into_iter().zip(handles) {
            self.textures.insert(path, handle);
        }
        Ok(true)
    }
}
