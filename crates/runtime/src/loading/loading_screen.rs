//! Stock loading-card selection and renderer-resident transition frames.

use std::collections::HashMap;
use std::sync::Arc;

use solarity_asset::{
    AssetPath, AssetStoreHandle, BlpTextureCache, LoadingScreenCatalog, LoadingScreenDefinition,
    MapCatalog,
};
use solarity_rendering::{
    BlpColorSpace, BlpTextureHandle, BlpTextureUploadRequest, UiMeshPlan, UiPreparedDraw,
    UiRenderBlend, UiRenderQuad, UiRenderSource, UiTextureAddressMode, UiTextureResidency,
    VulkanRenderer,
};
use solarity_ui::UiRenderError;

use crate::application::ApplicationError;
use crate::application::ui_frame::PreparedUiFrame;

use super::{RuntimeLoadingStage, layout::centered_aspect_fill_uv};

const LOADING_BAR_BACKGROUND: &str = "Interface\\Glues\\LoadingBar\\Loading-BarBackground.blp";
const LOADING_BAR_FILL: &str = "Interface\\Glues\\LoadingBar\\Loading-BarFill.blp";
const LOADING_BAR_BORDER: &str = "Interface\\Glues\\LoadingBar\\Loading-BarBorder.blp";

/// Map-indexed loading-card metadata retained before `MapCatalog` changes owner.
pub(crate) struct LoadingScreenDirectory {
    by_map: HashMap<u32, LoadingScreenDefinition>,
}

impl LoadingScreenDirectory {
    pub(crate) fn new(maps: &MapCatalog, screens: Option<LoadingScreenCatalog>) -> Self {
        let by_map = screens.map_or_else(HashMap::new, |screens| {
            maps.maps()
                .iter()
                .filter_map(|map| {
                    screens
                        .screen(map.loading_screen_id())
                        .cloned()
                        .map(|screen| (map.id(), screen))
                })
                .collect()
        });
        Self { by_map }
    }

    pub(crate) fn screen(&self, map_id: u32) -> Option<&LoadingScreenDefinition> {
        self.by_map.get(&map_id)
    }
}

/// Independently retained loading card presented between Glue and World.
pub(crate) struct RuntimeLoadingScreen {
    stage: RuntimeLoadingStage,
    frames: Vec<PreparedUiFrame>,
    presented: bool,
    final_frame_presented: bool,
}

struct UploadedLoadingTextures {
    handles: HashMap<AssetPath, BlpTextureHandle>,
    extents: HashMap<AssetPath, (u32, u32)>,
}

impl RuntimeLoadingScreen {
    /// Resolves stock loading art and prepares each finite progress generation.
    pub(crate) fn prepare(
        renderer: &mut VulkanRenderer,
        assets: &AssetStoreHandle,
        directory: &LoadingScreenDirectory,
        map_id: u32,
        display_extent: (u32, u32),
    ) -> Result<Self, ApplicationError> {
        let screen = directory.screen(map_id);
        let background = resolve_background(assets, screen, display_extent)?;
        let logical_extent = [
            display_extent.0 as f32 / display_extent.1 as f32 * 768.0,
            768.0,
        ];
        let bar_background = AssetPath::new(LOADING_BAR_BACKGROUND)?;
        let bar_fill = AssetPath::new(LOADING_BAR_FILL)?;
        let bar_border = AssetPath::new(LOADING_BAR_BORDER)?;
        // LoadingScreen.cpp 0x0040A990 owns the normal-card bar textures. Its
        // separate 0x0040AB70 text path exists only when
        // 0x00407E40 publishes TRIAL_LOADING_MESSAGE for a trial account.
        let mut paths = vec![bar_background, bar_fill, bar_border];
        if let Some(background) = &background {
            paths.push(background.clone());
        }
        let uploaded = upload_textures(renderer, assets, &paths)?;
        let background = background.as_ref().map(|path| {
            let extent = uploaded.extents[path];
            (path, centered_aspect_fill_uv(logical_extent, extent))
        });
        let frames = RuntimeLoadingStage::ALL
            .into_iter()
            .map(|stage| {
                let plan = loading_mesh(logical_extent, background, stage.progress())?;
                PreparedUiFrame::prepare(renderer, &plan, &uploaded.handles, None)
            })
            .collect::<Result<Vec<_>, ApplicationError>>()?;
        Ok(Self {
            stage: RuntimeLoadingStage::AwaitingWorld,
            frames,
            presented: false,
            final_frame_presented: false,
        })
    }

    /// Advances only when a later real subsystem milestone has completed.
    pub(crate) fn advance(&mut self, stage: RuntimeLoadingStage) {
        self.stage = self.stage.max(stage);
    }

    /// Presents the current loading-card generation.
    pub(crate) fn present(
        &mut self,
        renderer: &mut VulkanRenderer,
        overlay: &[UiPreparedDraw],
    ) -> Result<(), ApplicationError> {
        self.frames[self.stage.index()].present_with_overlay(renderer, overlay)?;
        self.presented = true;
        if self.stage == RuntimeLoadingStage::SceneReady {
            self.final_frame_presented = true;
        }
        Ok(())
    }

    /// Reports that a complete card was presented after all first-world inputs.
    pub(crate) fn ready_to_complete(&self) -> bool {
        self.stage == RuntimeLoadingStage::SceneReady && self.final_frame_presented
    }

    /// Reports whether the transition surface has owned at least one present.
    pub(crate) const fn has_presented(&self) -> bool {
        self.presented
    }
}

fn resolve_background(
    assets: &AssetStoreHandle,
    screen: Option<&LoadingScreenDefinition>,
    logical_extent: (u32, u32),
) -> Result<Option<AssetPath>, ApplicationError> {
    let Some(screen) = screen else {
        return Ok(None);
    };
    let aspect = logical_extent.0 as f32 / logical_extent.1 as f32;
    if screen.has_widescreen()
        && aspect > 4.0 / 3.0
        && let Some(wide) = screen.widescreen_texture()?
        && assets.borrow().contains(&wide)?
    {
        return Ok(Some(wide));
    }
    if assets.borrow().contains(screen.texture())? {
        return Ok(Some(screen.texture().clone()));
    }
    tracing::warn!(
        loading_screen_id = screen.id(),
        texture = %screen.texture(),
        "selected map loading art is absent; retaining the stock generic card"
    );
    Ok(None)
}

fn upload_textures(
    renderer: &mut VulkanRenderer,
    assets: &AssetStoreHandle,
    paths: &[AssetPath],
) -> Result<UploadedLoadingTextures, ApplicationError> {
    let mut cache = BlpTextureCache::new();
    let mut store = assets.borrow_mut();
    let sources = paths
        .iter()
        .map(|path| cache.load(&mut store, path))
        .collect::<Result<Vec<Arc<_>>, _>>()?;
    drop(store);
    let requests = sources
        .iter()
        .map(|source| BlpTextureUploadRequest::new(source, BlpColorSpace::Linear))
        .collect::<Vec<_>>();
    let handles = renderer.upload_blp_textures(&requests)?;
    let extents = paths
        .iter()
        .cloned()
        .zip(
            sources
                .iter()
                .map(|source| (source.width(), source.height())),
        )
        .collect();
    Ok(UploadedLoadingTextures {
        handles: paths.iter().cloned().zip(handles).collect(),
        extents,
    })
}

fn loading_mesh(
    logical_extent: [f32; 2],
    background: Option<(&AssetPath, [[f32; 2]; 4])>,
    progress: f32,
) -> Result<UiMeshPlan, ApplicationError> {
    let width = logical_extent[0];
    let height = logical_extent[1];
    let border_width = width * 0.600;
    let inner_width = width * 0.525;
    let border_height = height * 0.050;
    let inner_height = height * 0.025;
    let center_y = height * 0.075;
    let inner_x = (width - inner_width) * 0.5;
    let inner_y = center_y - inner_height * 0.5;
    let fill_width = 1.0 + progress.clamp(0.0, 1.0) * (inner_width - 1.0);
    let mut quads = vec![solid_quad(
        0,
        [0.0, 0.0, width, height],
        [0.015, 0.015, 0.02, 1.0],
    )];
    if let Some((path, texture_coordinates)) = background {
        quads.push(texture_quad_with_coordinates(
            1,
            path.clone(),
            [0.0, 0.0, width, height],
            texture_coordinates,
        ));
    }
    quads.extend([
        texture_quad(
            2,
            AssetPath::new(LOADING_BAR_BACKGROUND)?,
            [
                inner_x,
                inner_y,
                inner_x + inner_width,
                inner_y + inner_height,
            ],
        ),
        texture_quad(
            3,
            AssetPath::new(LOADING_BAR_FILL)?,
            [
                inner_x + 1.0,
                inner_y,
                inner_x + 1.0 + fill_width,
                inner_y + inner_height,
            ],
        ),
        texture_quad(
            4,
            AssetPath::new(LOADING_BAR_BORDER)?,
            [
                (width - border_width) * 0.5,
                center_y - border_height * 0.5,
                (width + border_width) * 0.5,
                center_y + border_height * 0.5,
            ],
        ),
    ]);
    UiMeshPlan::prepare([width, height], quads.into_iter())
        .map_err(UiRenderError::from)
        .map_err(ApplicationError::from)
}

fn texture_quad(object_index: usize, path: AssetPath, bounds: [f32; 4]) -> UiRenderQuad {
    texture_quad_with_coordinates(
        object_index,
        path,
        bounds,
        [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]],
    )
}

fn texture_quad_with_coordinates(
    object_index: usize,
    path: AssetPath,
    bounds: [f32; 4],
    texture_coordinates: [[f32; 2]; 4],
) -> UiRenderQuad {
    UiRenderQuad::new(
        object_index,
        UiRenderSource::Texture(path),
        UiRenderBlend::Alpha,
        UiTextureAddressMode::Clamp,
        UiTextureAddressMode::Clamp,
        UiTextureResidency::Blocking,
        false,
        bounds,
        texture_coordinates,
        [[1.0; 4]; 4],
    )
}

fn solid_quad(object_index: usize, bounds: [f32; 4], color: [f32; 4]) -> UiRenderQuad {
    UiRenderQuad::new(
        object_index,
        UiRenderSource::VertexColor,
        UiRenderBlend::Alpha,
        UiTextureAddressMode::Clamp,
        UiTextureAddressMode::Clamp,
        UiTextureResidency::Blocking,
        false,
        bounds,
        [[0.0; 2]; 4],
        [color; 4],
    )
}
