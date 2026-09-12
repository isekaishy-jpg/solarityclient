//! Transactional publication of a changed geometry dependency island.

use super::resolver::GeometryResolver;
use super::{UiRegionGeometryPlan, UiRegionGeometryRefresh, UiScreenRect, resolution_error};
use crate::UiLayoutError;
use crate::script::UiRuntimeObjectPlan;

impl UiRegionGeometryPlan {
    /// Re-solves the transitive geometry island rooted at changed objects.
    ///
    /// Parent inheritance and authored anchor targets are the only edges that
    /// can carry a region mutation to another object. Callers supply every owner
    /// whose dependencies changed. Reverse links discover only reachable regions;
    /// resolution borrows unrelated slots and commits links only after success.
    pub(crate) fn refresh_dependency_regions(
        &mut self,
        live: &UiRuntimeObjectPlan,
        root_indices: impl IntoIterator<Item = usize>,
    ) -> Result<UiRegionGeometryRefresh, UiLayoutError> {
        if live.objects().len() != self.regions.len()
            || self.presentations.len() != self.regions.len()
        {
            return Err(resolution_error(
                "live and retained geometry arenas have different sizes",
            ));
        }
        let mut roots = root_indices.into_iter().collect::<Vec<_>>();
        roots.sort_unstable();
        roots.dedup();
        let changes = self.dependencies.stage(live, &roots)?;
        let object_indices = self.dependencies.affected(&roots);
        if object_indices.is_empty() {
            return Ok(UiRegionGeometryRefresh {
                affected_objects: object_indices,
                previous_regions: Vec::new(),
                changed_objects: Vec::new(),
            });
        }
        let screen = UiScreenRect {
            left: 0.0,
            bottom: 0.0,
            right: self.ui_extent.0,
            top: self.ui_extent.1,
        };
        let mut resolver = GeometryResolver {
            live,
            screen,
            // Borrow unaffected seeds. Only this sorted dependency island owns
            // new results and cycle-detection slots during a retained refresh.
            retained: Some((self, &object_indices)),
            resolved: vec![None; object_indices.len()],
            visiting: vec![false; object_indices.len()],
        };
        for &object_index in &object_indices {
            resolver.resolve(object_index)?;
        }
        // Finish all resolution before publishing any results, including on
        // cycles or invalid references reached through a changed anchor.
        let refreshed = resolver
            .resolved
            .into_iter()
            .zip(&object_indices)
            .map(|(region, object_index)| {
                region.ok_or_else(|| {
                    resolution_error(format!(
                        "refreshed live region {object_index} remained unresolved"
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.dependencies.publish(changes);
        let mut changed_objects = Vec::new();
        let mut previous_regions = Vec::with_capacity(object_indices.len());
        for (&object_index, resolved) in object_indices.iter().zip(refreshed) {
            previous_regions.push(self.regions[object_index]);
            if self.regions[object_index] != resolved.public {
                changed_objects.push(object_index);
            }
            self.regions[object_index] = resolved.public;
            self.presentations[object_index] = resolved.presentation;
        }
        Ok(UiRegionGeometryRefresh {
            affected_objects: object_indices,
            previous_regions,
            changed_objects,
        })
    }
}
