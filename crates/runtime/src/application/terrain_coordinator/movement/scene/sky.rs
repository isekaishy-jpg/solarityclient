//! Sky bank and MOSB ownership across the ordered camera-root traversal.

use solarity_asset::DecodedWorldModel;
use solarity_rendering::{WorldCameraError, WorldScreenWindow, WorldSkyWindow};
use solarity_systems::{WorldModelCameraSceneQuery, WorldModelExteriorPortalWindow};
use std::sync::Arc;

/// Retains the selected root so its MOSB name survives until frame consumption.
#[derive(Default)]
pub(super) struct WorldSceneSky {
    window: Option<WorldModelExteriorPortalWindow>,
    root: Option<Arc<DecodedWorldModel>>,
}

impl WorldSceneSky {
    /// 7831A0 clears both the sky bank and the root skybox pointer each frame.
    pub(super) fn begin(&mut self) {
        self.window = None;
        self.root = None;
    }

    /// 795D40 seeds full sky for 0x40140 camera groups. 79A870 discards that
    /// seed after a secondary root, so the caller omits this in dual-root frames.
    pub(super) fn seed(&mut self, root: &DecodedWorldModel, groups: [Option<usize>; 2]) {
        if groups
            .into_iter()
            .flatten()
            .any(|group| root.group_info()[group].flags() & 0x40140 != 0)
        {
            self.outdoors();
        }
    }

    /// A camera without WMO registration starts with full visibility at depth 0.
    pub(super) fn outdoors(&mut self) {
        self.window = Some(WorldModelExteriorPortalWindow {
            screen_window: [0., 0., 1., 1.],
            depth: 0.,
        });
    }

    /// 7AC060 publishes MOSB even when it is null, replacing earlier selection.
    pub(super) fn record_root(
        &mut self,
        root: &Arc<DecodedWorldModel>,
        query: &WorldModelCameraSceneQuery,
    ) {
        if query.has_skybox_request() {
            self.root = root.skybox().map(|_| Arc::clone(root));
        }
    }

    /// Only the primary root's portals survive 79A870's between-root bank reset.
    pub(super) fn record_primary(&mut self, query: &WorldModelCameraSceneQuery) {
        if let Some(incoming) = query.sky_window() {
            self.window = Some(match self.window {
                None => incoming,
                Some(previous) => WorldModelExteriorPortalWindow {
                    screen_window: std::array::from_fn(|axis| {
                        let old = previous.screen_window[axis];
                        let new = incoming.screen_window[axis];
                        if (axis < 2 && old < new) || (axis >= 2 && old > new) {
                            old
                        } else {
                            new
                        }
                    }),
                    depth: if previous.depth > incoming.depth {
                        previous.depth
                    } else {
                        incoming.depth
                    },
                },
            });
        }
    }

    /// Produces 7F09B0's visible full-viewport intersection for model preparation.
    pub(super) fn window(&self) -> Result<Option<WorldSkyWindow>, WorldCameraError> {
        let Some(window) = self.window else {
            return Ok(None);
        };
        let bounds = window.screen_window;
        // Projection may collapse a displaced polygon to an edge. Stock keeps
        // its bank for background selection but rejects its draw rectangle.
        if bounds[0] >= bounds[2] || bounds[1] >= bounds[3] {
            return Ok(None);
        }
        Ok(WorldSkyWindow::new(bounds)?.clipped(WorldScreenWindow::FULL))
    }

    /// The bank's presence selects background fog before draw-window clipping.
    pub(super) fn has_window(&self) -> bool {
        self.window.is_some()
    }

    pub(super) fn skybox(&self) -> Option<&str> {
        self.root.as_ref().and_then(|root| root.skybox())
    }
}
