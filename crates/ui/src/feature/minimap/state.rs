//! Scene-owned minimap zoom, shared by every Minimap frame.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use solarity_asset::AssetPath;

#[derive(Debug)]
struct MinimapState {
    zoom: Cell<[u32; 2]>,
    indoors: Cell<bool>,
    revision: Cell<u64>,
    mask: RefCell<Option<AssetPath>>,
}

/// Native minimap scene controls, independent of any particular Lua widget.
#[derive(Clone, Debug)]
pub struct UiMinimapState(Rc<MinimapState>);

impl Default for UiMinimapState {
    fn default() -> Self {
        Self(Rc::new(MinimapState {
            zoom: Cell::new([3, 3]),
            indoors: Cell::new(false),
            revision: Cell::new(0),
            mask: RefCell::new(None),
        }))
    }
}

impl UiMinimapState {
    /// Number of stock indoor and outdoor zoom levels.
    pub const ZOOM_LEVELS: u32 = 6;

    /// Returns the selected indoor or outdoor zoom index.
    #[must_use]
    pub fn zoom(&self) -> u32 {
        self.0.zoom.get()[usize::from(self.indoors())]
    }

    /// Whether the scene has selected indoor WMO minimap tiles.
    #[must_use]
    pub fn indoors(&self) -> bool {
        self.0.indoors.get()
    }

    /// Changes when the scene's zoom, selected mode, or mask changes.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.0.revision.get()
    }

    /// Shared custom mask; absence selects stock Textures/MinimapMask.blp.
    #[must_use]
    pub fn mask(&self) -> Option<AssetPath> {
        self.0.mask.borrow().clone()
    }

    pub(crate) fn set_mask(&self, path: AssetPath) {
        let mut mask = self.0.mask.borrow_mut();
        if mask.as_ref() != Some(&path) {
            *mask = Some(path);
            self.bump_revision();
        }
    }

    /// Returns the visible radius in world yards at the selected zoom.
    #[must_use]
    pub fn radius(&self) -> f32 {
        let zoom = self.zoom() as usize;
        if self.indoors() {
            [150.0, 120.0, 90.0, 60.0, 40.0, 25.0][zoom]
        } else {
            [14.0, 12.0, 10.0, 8.0, 6.0, 4.0][zoom] * 0.5 * 33.333_332
        }
    }

    /// Selects the mode recovered from world geometry. Returns whether callers
    /// must dispatch the stock `MINIMAP_UPDATE_ZOOM` transition event.
    pub fn set_indoors(&self, indoors: bool) -> bool {
        if self.0.indoors.replace(indoors) == indoors {
            return false;
        }
        self.bump_revision();
        true
    }

    pub(crate) fn zoom_cvar(&self) -> &'static str {
        if self.indoors() {
            "minimapInsideZoom"
        } else {
            "minimapZoom"
        }
    }

    pub(crate) fn set_zoom(&self, zoom: u32) {
        let mut levels = self.0.zoom.get();
        let selected = &mut levels[usize::from(self.indoors())];
        let zoom = zoom.min(Self::ZOOM_LEVELS - 1);
        if *selected != zoom {
            *selected = zoom;
            self.0.zoom.set(levels);
            self.bump_revision();
        }
    }

    /// Native scene startup copies both saved CVars once. Later direct CVar
    /// writes have no callback into the active scene in build 12340.
    pub(crate) fn restore_zoom(&self, outdoor: u32, indoor: u32) {
        let levels = [outdoor.min(5), indoor.min(5)];
        if self.0.zoom.replace(levels) != levels {
            self.bump_revision();
        }
    }

    fn bump_revision(&self) {
        self.0.revision.set(self.0.revision.get().wrapping_add(1));
    }
}
