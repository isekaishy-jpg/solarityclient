//! Authored WMO group selection and portal topology, independent of presentation.

use crate::{AssetError, AssetPath};

use super::DecodedWorldModelGroup;

/// One root MOGI entry; its flags and bounds are distinct from loaded MOGP data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelGroupInfo {
    flags: u32,
    bounds: [[f32; 3]; 2],
}

impl WorldModelGroupInfo {
    /// Returns MOGI flags after 7D7470 clears 0x40000 for an empty MOSB.
    #[must_use]
    pub const fn flags(self) -> u32 {
        self.flags
    }

    /// Returns the root-local MOGI box.
    #[must_use]
    pub const fn bounds(self) -> [[f32; 3]; 2] {
        self.bounds
    }
}

/// One MOPT polygon range and authored plane, without normal normalization.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelPortal {
    vertex_start: u16,
    vertex_count: u16,
    normal: [f32; 3],
    distance: f32,
}

impl WorldModelPortal {
    /// Returns the first vertex in the root MOPV table.
    #[must_use]
    pub const fn vertex_start(self) -> u16 {
        self.vertex_start
    }

    /// Returns the number of polygon vertices in authored winding order.
    #[must_use]
    pub const fn vertex_count(self) -> u16 {
        self.vertex_count
    }

    /// Returns the authored plane normal.
    #[must_use]
    pub const fn normal(self) -> [f32; 3] {
        self.normal
    }

    /// Returns D in the native plane equation `dot(normal, point) + D = 0`.
    #[must_use]
    pub const fn distance(self) -> f32 {
        self.distance
    }
}

/// One MOPR edge from a group's portal range to an adjacent group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldModelPortalReference {
    portal_index: u16,
    group_index: u16,
    side: i16,
    padding: u16,
}

impl WorldModelPortalReference {
    /// Returns the referenced MOPT entry.
    #[must_use]
    pub const fn portal_index(self) -> u16 {
        self.portal_index
    }

    /// Returns the adjacent root group index.
    #[must_use]
    pub const fn group_index(self) -> u16 {
        self.group_index
    }

    /// Returns the signed side used when choosing a portal's source group.
    #[must_use]
    pub const fn side(self) -> i16 {
        self.side
    }

    /// Returns the authored padding word without interpreting it as flags.
    #[must_use]
    pub const fn padding(self) -> u16 {
        self.padding
    }
}

pub(super) struct WorldModelSpatialData {
    pub groups: Vec<WorldModelGroupInfo>,
    pub vertices: Vec<[f32; 3]>,
    pub portals: Vec<WorldModelPortal>,
    pub references: Vec<WorldModelPortalReference>,
    pub convex_volume_planes: Vec<[f32; 4]>,
}

impl WorldModelSpatialData {
    pub(super) fn decode(
        path: &AssetPath,
        root: &wow_wmo::root_parser::WmoRoot,
        groups: &[DecodedWorldModelGroup],
    ) -> Result<Self, AssetError> {
        let invalid = |message: &str| AssetError::WorldModelDecode {
            path: path.clone(),
            message: message.to_owned(),
        };
        // Build 12340's 0x007D7470 stores MCVP's byte count divided by 16.
        // The dependency labels this chunk Cataclysm+, but its four-float
        // records are already present in stock Wrath transport roots.
        let convex_volume_planes = root
            .convex_volume_planes
            .iter()
            .map(|entry| entry.plane)
            .collect::<Vec<_>>();
        if convex_volume_planes
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
        {
            return Err(invalid("MCVP contains a non-finite plane"));
        }
        let vertices = root
            .portal_vertices
            .iter()
            .map(|v| [v.x, v.y, v.z])
            .collect::<Vec<_>>();
        if vertices.iter().flatten().any(|value| !value.is_finite()) {
            return Err(invalid("MOPV contains a non-finite vertex"));
        }
        let mut portals = Vec::with_capacity(root.portals.len());
        for portal in &root.portals {
            let end = usize::from(portal.start_vertex) + usize::from(portal.n_vertices);
            if end > vertices.len() {
                return Err(invalid("MOPT vertex range exceeds MOPV"));
            }
            let normal = [portal.normal.x, portal.normal.y, portal.normal.z];
            if normal
                .into_iter()
                .chain([portal.distance])
                .any(|value| !value.is_finite())
            {
                return Err(invalid("MOPT contains a non-finite plane"));
            }
            portals.push(WorldModelPortal {
                vertex_start: portal.start_vertex,
                vertex_count: portal.n_vertices,
                normal,
                distance: portal.distance,
            });
        }
        let mut references = Vec::with_capacity(root.portal_refs.len());
        for reference in &root.portal_refs {
            if usize::from(reference.portal_index) >= portals.len() {
                return Err(invalid("MOPR references a portal outside MOPT"));
            }
            if usize::from(reference.group_index) >= groups.len() {
                return Err(invalid("MOPR references a group outside MOGI"));
            }
            references.push(WorldModelPortalReference {
                portal_index: reference.portal_index,
                group_index: reference.group_index,
                side: reference.side,
                padding: reference.padding,
            });
        }
        for group in groups {
            let end = usize::from(group.portal_reference_start())
                + usize::from(group.portal_reference_count());
            if end > references.len() {
                return Err(AssetError::WorldModelDecode {
                    path: group.path().clone(),
                    message: "MOGP portal range exceeds root MOPR".to_owned(),
                });
            }
        }
        Ok(Self {
            groups: root
                .group_info
                .iter()
                .map(|group| WorldModelGroupInfo {
                    // 7D7470 disables sky-only portals and registration seeds
                    // when MOSB begins with NUL. Loaded MOGP flags stay intact.
                    flags: if root.skybox.is_some() {
                        group.flags
                    } else {
                        group.flags & !0x40000
                    },
                    bounds: [group.bounding_box_min, group.bounding_box_max],
                })
                .collect(),
            vertices,
            portals,
            references,
            convex_volume_planes,
        })
    }
}
