//! Archive-backed top-left frame-rate overlay.

use std::collections::HashMap;
use std::time::Instant;

use solarity_asset::{AssetPath, AssetStoreHandle};
use solarity_rendering::{UiPreparedDraw, VulkanRenderer};
use solarity_ui::{FontRasterization, UiGlyphAtlasPlan, UiNativeTextStyle};

use crate::FrameRateCounter;
use crate::application::ApplicationError;
use crate::application::ui_frame::PreparedUiFrame;

const FONT_PATH: &str = "Fonts\\FRIZQT__.TTF";
const GLYPH_REPERTOIRE: &str = "-0123456789. FPS";
const UI_HEIGHT: f32 = 768.0;

/// Renderer-resident developer FPS overlay matching SolCL's default style.
pub(super) struct RuntimeFpsOverlay {
    atlas: UiGlyphAtlasPlan,
    style: UiNativeTextStyle,
    frame: PreparedUiFrame,
    counter: FrameRateCounter,
    logical_extent: [f32; 2],
    display_height: u32,
    text: String,
}

impl RuntimeFpsOverlay {
    pub(super) fn prepare(
        renderer: &mut VulkanRenderer,
        assets: &AssetStoreHandle,
        pixel_extent: (u32, u32),
    ) -> Result<Option<Self>, ApplicationError> {
        let logical_extent = overlay_extent(pixel_extent);
        let face = AssetPath::new(FONT_PATH)?;
        if !assets.borrow().contains(&face)? {
            tracing::warn!(font = %face, "FPS overlay font is absent from the mounted client data");
            return Ok(None);
        }
        let style = UiNativeTextStyle::new(face, 16.0, FontRasterization::Antialiased)
            .with_color([0.35, 1.0, 0.35, 1.0])
            .with_shadow([0.0, 0.0, 0.0, 1.0], [1.0, 1.0])
            .with_outline([0.0, 0.0, 0.0, 1.0], 1.0);
        let mut store = assets.borrow_mut();
        let atlas = UiGlyphAtlasPlan::from_native_text(
            &style,
            GLYPH_REPERTOIRE,
            &mut store,
            pixel_extent.1,
        )?;
        drop(store);
        let glyph_texture =
            renderer.upload_ui_glyph_texture(atlas.identity(), atlas.extent(), atlas.rgba8())?;
        let text = "-- FPS".to_owned();
        let plan = atlas.native_text_mesh(
            &text,
            &style,
            logical_extent,
            [12.0, 10.0],
            24.0,
            pixel_extent.1,
        )?;
        let frame = PreparedUiFrame::prepare(
            renderer,
            &plan,
            &HashMap::new(),
            Some((atlas.identity(), glyph_texture)),
        )?;
        Ok(Some(Self {
            atlas,
            style,
            frame,
            counter: FrameRateCounter::new(),
            logical_extent,
            display_height: pixel_extent.1,
            text,
        }))
    }

    pub(super) fn record_presented(
        &mut self,
        renderer: &mut VulkanRenderer,
        completed_at: Instant,
    ) -> Result<(), ApplicationError> {
        let Some(fps) = self.counter.record(completed_at) else {
            return Ok(());
        };
        let text = format!("{fps:.1} FPS");
        if self.text == text {
            return Ok(());
        }
        let plan = self.atlas.native_text_mesh(
            &text,
            &self.style,
            self.logical_extent,
            [12.0, 10.0],
            24.0,
            self.display_height,
        )?;
        self.frame.replace_mesh(renderer, &plan)?;
        self.text = text;
        Ok(())
    }

    pub(super) const fn logical_extent(&self) -> [f32; 2] {
        self.logical_extent
    }

    pub(super) fn draws(&self) -> &[UiPreparedDraw] {
        self.frame.draws()
    }
}

pub(super) fn overlay_extent(pixel_extent: (u32, u32)) -> [f32; 2] {
    [
        pixel_extent.0 as f32 / pixel_extent.1 as f32 * UI_HEIGHT,
        UI_HEIGHT,
    ]
}
