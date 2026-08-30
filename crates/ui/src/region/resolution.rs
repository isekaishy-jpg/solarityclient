//! Resolved stock region state after XML inheritance and ownership.

use crate::{UiLayoutError, UiLayoutPlan, UiObjectTree, UiPoint};

const POINT_COUNT: usize = 9;

/// The region against which an anchor is resolved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiAnchorTarget {
    /// The unparented screen root owned by `CSimpleTop`.
    Screen,
    /// One object in the live object arena.
    Object(usize),
}

/// One final anchor after inherited declarations and `setAllPoints` apply.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiRegionAnchor {
    point: UiPoint,
    target: UiAnchorTarget,
    relative_point: UiPoint,
    offset: (f32, f32),
}

impl UiRegionAnchor {
    /// Returns the point constrained on this region.
    #[must_use]
    pub const fn point(self) -> UiPoint {
        self.point
    }

    /// Returns the screen root or live object used by the constraint.
    #[must_use]
    pub const fn target(self) -> UiAnchorTarget {
        self.target
    }

    /// Returns the point used on the anchor target.
    #[must_use]
    pub const fn relative_point(self) -> UiPoint {
        self.relative_point
    }

    /// Returns the absolute XML offset from the target point.
    #[must_use]
    pub const fn offset(self) -> (f32, f32) {
        self.offset
    }
}

/// Final startup properties for one script region.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiRegionState {
    width: f32,
    height: f32,
    first_anchor: usize,
    anchor_count: usize,
    shown: bool,
    effectively_shown: bool,
    alpha: f32,
    effective_alpha: f32,
    scale: f32,
    effective_scale: f32,
}

impl UiRegionState {
    /// Returns the resolved absolute width, or zero when XML never sets one.
    #[must_use]
    pub const fn width(self) -> f32 {
        self.width
    }

    /// Returns the resolved absolute height, or zero when XML never sets one.
    #[must_use]
    pub const fn height(self) -> f32 {
        self.height
    }

    /// Returns whether this region itself starts shown.
    #[must_use]
    pub const fn shown(self) -> bool {
        self.shown
    }

    /// Returns startup visibility after applying the ownership chain.
    #[must_use]
    pub const fn effectively_shown(self) -> bool {
        self.effectively_shown
    }

    /// Returns this region's resolved local alpha.
    #[must_use]
    pub const fn alpha(self) -> f32 {
        self.alpha
    }

    /// Returns startup alpha multiplied through the ownership chain.
    #[must_use]
    pub const fn effective_alpha(self) -> f32 {
        self.effective_alpha
    }

    /// Returns this region's resolved local scale.
    #[must_use]
    pub const fn scale(self) -> f32 {
        self.scale
    }

    /// Returns startup scale multiplied through the ownership chain.
    #[must_use]
    pub const fn effective_scale(self) -> f32 {
        self.effective_scale
    }
}

/// Flat resolved region states and final anchor constraints.
pub struct UiRegionStatePlan {
    states: Vec<UiRegionState>,
    anchors: Vec<UiRegionAnchor>,
}

impl UiRegionStatePlan {
    /// Applies inherited layout layers to the final object ownership graph.
    ///
    /// # Errors
    ///
    /// Returns [`UiLayoutError::Resolution`] for mismatched arenas, missing or
    /// self-referential anchor targets, invalid scale, and ownership cycles.
    pub fn resolve(tree: &UiObjectTree<'_>, layout: &UiLayoutPlan) -> Result<Self, UiLayoutError> {
        if layout.node_count() != tree.nodes().len() {
            return Err(resolution_error(
                "layout plan and object tree have different node counts",
            ));
        }

        let mut local = Vec::with_capacity(tree.nodes().len());
        let mut anchors = Vec::new();
        for (node_index, object) in tree.nodes().iter().enumerate() {
            let layout_node = layout.node(node_index).ok_or_else(|| {
                resolution_error(format!("object {node_index} has no layout-plan entry"))
            })?;
            let mut width = 0.0;
            let mut height = 0.0;
            let mut shown = true;
            let mut alpha = 1.0;
            let mut scale = 1.0;
            let mut points = [None; POINT_COUNT];

            for layer in layout.layers_for(layout_node) {
                if let Some(dimensions) = layer.dimensions() {
                    if let Some(value) = dimensions.width() {
                        width = value;
                    }
                    if let Some(value) = dimensions.height() {
                        height = value;
                    }
                }
                if layer.anchors_present() {
                    for anchor in layout.anchors_for(*layer) {
                        let target = match anchor.relative_to() {
                            Some(name) => {
                                let target_index = tree.node_index(name).ok_or_else(|| {
                                    resolution_error(format!(
                                        "region {} names unavailable anchor target {name}",
                                        object.name().unwrap_or("<unnamed>")
                                    ))
                                })?;
                                if target_index == node_index {
                                    return Err(resolution_error(format!(
                                        "region {} anchors to itself",
                                        object.name().unwrap_or("<unnamed>")
                                    )));
                                }
                                UiAnchorTarget::Object(target_index)
                            }
                            None => object
                                .parent()
                                .map_or(UiAnchorTarget::Screen, UiAnchorTarget::Object),
                        };
                        let point = anchor.point();
                        points[point.index()] = Some(UiRegionAnchor {
                            point,
                            target,
                            relative_point: anchor.relative_point().unwrap_or(point),
                            offset: anchor.offset().unwrap_or((0.0, 0.0)),
                        });
                    }
                } else if layer.set_all_points() == Some(true) {
                    let target = object
                        .parent()
                        .map_or(UiAnchorTarget::Screen, UiAnchorTarget::Object);
                    points = [None; POINT_COUNT];
                    points[UiPoint::TopLeft.index()] = Some(UiRegionAnchor {
                        point: UiPoint::TopLeft,
                        target,
                        relative_point: UiPoint::TopLeft,
                        offset: (0.0, 0.0),
                    });
                    points[UiPoint::BottomRight.index()] = Some(UiRegionAnchor {
                        point: UiPoint::BottomRight,
                        target,
                        relative_point: UiPoint::BottomRight,
                        offset: (0.0, 0.0),
                    });
                }
                if let Some(hidden) = layer.hidden() {
                    shown = !hidden;
                }
                if let Some(value) = layer.alpha() {
                    alpha = value;
                }
                if let Some(value) = layer.scale() {
                    if value <= 0.0 {
                        return Err(resolution_error(format!(
                            "region {} has nonpositive scale {value}",
                            object.name().unwrap_or("<unnamed>")
                        )));
                    }
                    scale = value;
                }
            }

            let first_anchor = anchors.len();
            anchors.extend(points.into_iter().flatten());
            local.push(UiRegionState {
                width,
                height,
                first_anchor,
                anchor_count: anchors.len() - first_anchor,
                shown,
                effectively_shown: shown,
                alpha,
                effective_alpha: alpha,
                scale,
                effective_scale: scale,
            });
        }

        let mut states = vec![None; local.len()];
        let mut visiting = vec![false; local.len()];
        for index in 0..local.len() {
            resolve_effective_state(tree, &local, index, &mut states, &mut visiting)?;
        }
        Ok(Self {
            states: states.into_iter().flatten().collect(),
            anchors,
        })
    }

    /// Returns the resolved state for one object-arena index.
    #[must_use]
    pub fn state(&self, node_index: usize) -> Option<UiRegionState> {
        self.states.get(node_index).copied()
    }

    /// Returns the final constraints owned by one resolved region state.
    #[must_use]
    pub fn anchors_for(&self, state: UiRegionState) -> &[UiRegionAnchor] {
        &self.anchors[state.first_anchor..state.first_anchor + state.anchor_count]
    }

    /// Returns the number of resolved region states.
    #[must_use]
    pub fn state_count(&self) -> usize {
        self.states.len()
    }

    /// Returns the number of final anchor constraints after replacement.
    #[must_use]
    pub fn anchor_count(&self) -> usize {
        self.anchors.len()
    }
}

fn resolve_effective_state(
    tree: &UiObjectTree<'_>,
    local: &[UiRegionState],
    index: usize,
    resolved: &mut [Option<UiRegionState>],
    visiting: &mut [bool],
) -> Result<UiRegionState, UiLayoutError> {
    if let Some(state) = resolved.get(index).copied().flatten() {
        return Ok(state);
    }
    let object = tree.nodes().get(index).ok_or_else(|| {
        resolution_error(format!("object parent index {index} is outside the arena"))
    })?;
    let is_visiting = visiting
        .get_mut(index)
        .ok_or_else(|| resolution_error(format!("region index {index} is outside the arena")))?;
    if std::mem::replace(is_visiting, true) {
        return Err(resolution_error(format!(
            "region ownership cycle reaches {}",
            object.name().unwrap_or("<unnamed>")
        )));
    }

    let result = (|| {
        let mut state = *local
            .get(index)
            .ok_or_else(|| resolution_error(format!("region index {index} has no local state")))?;
        if let Some(parent_index) = object.parent() {
            let parent = resolve_effective_state(tree, local, parent_index, resolved, visiting)?;
            state.effectively_shown = state.shown && parent.effectively_shown;
            state.effective_alpha = state.alpha * parent.effective_alpha;
            state.effective_scale = state.scale * parent.effective_scale;
        }
        Ok(state)
    })();
    visiting[index] = false;
    let state = result?;
    resolved[index] = Some(state);
    Ok(state)
}

fn resolution_error(message: impl Into<String>) -> UiLayoutError {
    UiLayoutError::Resolution {
        message: message.into(),
    }
}
