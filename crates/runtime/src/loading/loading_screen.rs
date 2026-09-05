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

use super::{
    RuntimeLoadingReadiness, RuntimeLoadingStage,
    layout::{STOCK_LOADING_ART_ASPECT, STOCK_WIDE_LOADING_ART_ASPECT, centered_aspect_fit_bounds},
};

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
    plans: Vec<UiMeshPlan>,
    frame: PreparedUiFrame,
    presented: bool,
    final_frame_presented: bool,
}

struct UploadedLoadingTextures {
    handles: HashMap<AssetPath, BlpTextureHandle>,
}

struct ResolvedLoadingBackground {
    path: AssetPath,
    authored_aspect: f32,
}

impl RuntimeLoadingScreen {
    /// Resolves stock loading art and prepares each finite progress generation.
    pub(crate) fn prepare(
        renderer: &mut VulkanRenderer,
        assets: &AssetStoreHandle,
        textures: &mut BlpTextureCache,
        directory: &LoadingScreenDirectory,
        map_id: Option<u32>,
        display_extent: (u32, u32),
    ) -> Result<Self, ApplicationError> {
        let screen = map_id.and_then(|map_id| directory.screen(map_id));
        let background = resolve_background(assets, screen, display_extent)?;
        let logical_extent = [
            display_extent.0 as f32 / display_extent.1 as f32 * 768.0,
            768.0,
        ];
        let bar_fill = AssetPath::new(LOADING_BAR_FILL)?;
        let bar_border = AssetPath::new(LOADING_BAR_BORDER)?;
        // LoadingScreen.cpp 0x0040A990 owns exactly the fill and border textures
        // from the two records at 0x009E2DFC. Its
        // separate 0x0040AB70 text path exists only when
        // 0x00407E40 publishes TRIAL_LOADING_MESSAGE for a trial account.
        let mut paths = vec![bar_fill, bar_border];
        if let Some(background) = &background {
            paths.push(background.path.clone());
        }
        let uploaded = upload_textures(renderer, assets, textures, &paths)?;
        let card_bounds = centered_aspect_fit_bounds(
            logical_extent,
            background
                .as_ref()
                .map_or(STOCK_LOADING_ART_ASPECT, |background| {
                    background.authored_aspect
                }),
        );
        let background = background.as_ref().map(|background| &background.path);
        let plans = RuntimeLoadingStage::ALL
            .into_iter()
            .map(|stage| loading_mesh(logical_extent, card_bounds, background, stage.progress()))
            .collect::<Result<Vec<_>, ApplicationError>>()?;
        let frame = PreparedUiFrame::prepare(
            renderer,
            &plans[RuntimeLoadingStage::AwaitingWorld.index()],
            &uploaded.handles,
            None,
        )?;
        Ok(Self {
            stage: RuntimeLoadingStage::AwaitingWorld,
            plans,
            frame,
            presented: false,
            final_frame_presented: false,
        })
    }

    /// Advances only when a later real subsystem milestone has completed.
    ///
    /// The complete readiness image is logged once per stage transition. This
    /// keeps a retained card diagnosable without emitting one record per frame
    /// or weakening any of stock's first-world prerequisites.
    pub(crate) fn advance(&mut self, readiness: RuntimeLoadingReadiness) {
        let stage = readiness.stage();
        let previous = self.stage;
        self.stage = previous.max(stage);
        if self.stage == previous {
            return;
        }
        tracing::info!(
            previous_stage = ?previous,
            stage = ?self.stage,
            world_accepted = readiness.world_accepted,
            environment_ready = readiness.environment_ready,
            player_ready = readiness.player_ready,
            scene_ready = readiness.scene_ready,
            ui_ready = readiness.ui_ready,
            transport_resource_ready = readiness.transport_resource_ready,
            "loading card advanced to a completed first-world milestone"
        );
    }

    /// Presents the current loading-card generation.
    pub(crate) fn present(
        &mut self,
        renderer: &mut VulkanRenderer,
        overlay: &[UiPreparedDraw],
    ) -> Result<(), ApplicationError> {
        self.frame
            .replace_mesh(renderer, &self.plans[self.stage.index()])?;
        self.frame.present_with_overlay(renderer, overlay)?;
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
) -> Result<Option<ResolvedLoadingBackground>, ApplicationError> {
    let Some(screen) = screen else {
        return Ok(None);
    };
    let aspect = logical_extent.0 as f32 / logical_extent.1 as f32;
    if screen.has_widescreen()
        && aspect > 4.0 / 3.0
        && let Some(wide) = screen.widescreen_texture()?
        && assets.borrow().contains(&wide)?
    {
        return Ok(Some(ResolvedLoadingBackground {
            path: wide,
            authored_aspect: STOCK_WIDE_LOADING_ART_ASPECT,
        }));
    }
    if assets.borrow().contains(screen.texture())? {
        return Ok(Some(ResolvedLoadingBackground {
            path: screen.texture().clone(),
            authored_aspect: STOCK_LOADING_ART_ASPECT,
        }));
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
    cache: &mut BlpTextureCache,
    paths: &[AssetPath],
) -> Result<UploadedLoadingTextures, ApplicationError> {
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
    Ok(UploadedLoadingTextures {
        handles: paths.iter().cloned().zip(handles).collect(),
    })
}

/// Maps the complete native card viewport into the renderer's display coordinates.
/// The bar records at `0x009E2DFC` share that viewport with the uncropped image.
fn loading_mesh(
    logical_extent: [f32; 2],
    card_bounds: [f32; 4],
    background: Option<&AssetPath>,
    progress: f32,
) -> Result<UiMeshPlan, ApplicationError> {
    let [left, bottom, right, top] = card_bounds;
    let width = right - left;
    let height = top - bottom;
    let border_width = width * 0.600;
    let inner_width = width * 0.525;
    let border_height = height * 0.050;
    let inner_height = height * 0.025;
    let center_y = bottom + height * 0.075;
    let inner_x = left + (width - inner_width) * 0.5;
    let inner_y = center_y - inner_height * 0.5;
    let fill_width = progress.clamp(0.0, 1.0) * inner_width;
    let mut quads = vec![solid_quad(
        0,
        [0.0, 0.0, logical_extent[0], logical_extent[1]],
        [0.0, 0.0, 0.0, 1.0],
    )];
    if let Some(path) = background {
        quads.push(texture_quad(1, path.clone(), card_bounds));
    }
    quads.extend([
        texture_quad(
            2,
            AssetPath::new(LOADING_BAR_FILL)?,
            [
                inner_x,
                inner_y,
                inner_x + fill_width,
                inner_y + inner_height,
            ],
        ),
        texture_quad(
            3,
            AssetPath::new(LOADING_BAR_BORDER)?,
            [
                left + (width - border_width) * 0.5,
                center_y - border_height * 0.5,
                left + (width + border_width) * 0.5,
                center_y + border_height * 0.5,
            ],
        ),
    ]);
    UiMeshPlan::prepare(logical_extent, quads.into_iter())
        .map_err(UiRenderError::from)
        .map_err(ApplicationError::from)
}

/// Preserves the full stock UV range for both loading art and progress textures.
fn texture_quad(object_index: usize, path: AssetPath, bounds: [f32; 4]) -> UiRenderQuad {
    UiRenderQuad::new(
        object_index,
        UiRenderSource::Texture(path),
        UiRenderBlend::Alpha,
        UiTextureAddressMode::Clamp,
        UiTextureAddressMode::Clamp,
        UiTextureResidency::Blocking,
        false,
        bounds,
        [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]],
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
