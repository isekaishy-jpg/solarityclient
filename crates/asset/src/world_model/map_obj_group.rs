//! Owned WMO group geometry and BSP collision records.

use crate::{ArchiveDescriptor, AssetPath};

/// One decoded WMO group file in root group-index order.
pub struct DecodedWorldModelGroup {
    index: u32,
    path: AssetPath,
    source: ArchiveDescriptor,
    flags: u32,
    bounds: [[f32; 3]; 2],
    liquid_type: u32,
    vertices: Vec<[f32; 3]>,
    indices: Vec<u16>,
    polygons: Vec<WorldModelPolygon>,
    bsp_nodes: Vec<WorldModelBspNode>,
    bsp_faces: Vec<u16>,
}

impl DecodedWorldModelGroup {
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn new(
        index: u32,
        path: AssetPath,
        source: ArchiveDescriptor,
        flags: u32,
        bounds: [[f32; 3]; 2],
        liquid_type: u32,
        vertices: Vec<[f32; 3]>,
        indices: Vec<u16>,
        polygons: Vec<WorldModelPolygon>,
        bsp_nodes: Vec<WorldModelBspNode>,
        bsp_faces: Vec<u16>,
    ) -> Self {
        Self {
            index,
            path,
            source,
            flags,
            bounds,
            liquid_type,
            vertices,
            indices,
            polygons,
            bsp_nodes,
            bsp_faces,
        }
    }

    /// Returns the zero-based root group index.
    #[must_use]
    pub const fn index(&self) -> u32 {
        self.index
    }

    /// Returns this group's derived archive path.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the archive selected independently for this group file.
    #[must_use]
    pub const fn source(&self) -> &ArchiveDescriptor {
        &self.source
    }

    /// Returns raw MOGP group flags.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns group-local lower and upper authored bounds.
    #[must_use]
    pub const fn bounds(&self) -> [[f32; 3]; 2] {
        self.bounds
    }

    /// Returns the MOGP liquid identifier used by MLIQ resolution.
    #[must_use]
    pub const fn liquid_type(&self) -> u32 {
        self.liquid_type
    }

    /// Returns group-local MOVT positions.
    #[must_use]
    pub fn vertices(&self) -> &[[f32; 3]] {
        &self.vertices
    }

    /// Returns the direct triangle-list MOVI stream.
    #[must_use]
    pub fn indices(&self) -> &[u16] {
        &self.indices
    }

    /// Returns MOPY entries parallel to MOVI triangles.
    #[must_use]
    pub fn polygons(&self) -> &[WorldModelPolygon] {
        &self.polygons
    }

    /// Returns the stock MOBN BSP nodes.
    #[must_use]
    pub fn bsp_nodes(&self) -> &[WorldModelBspNode] {
        &self.bsp_nodes
    }

    /// Returns MOBR polygon indices referenced by BSP leaf ranges.
    #[must_use]
    pub fn bsp_faces(&self) -> &[u16] {
        &self.bsp_faces
    }
}

/// One MOPY material/behavior record parallel to a triangle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldModelPolygon {
    flags: u8,
    material_id: u8,
}

impl WorldModelPolygon {
    pub(super) const fn new(flags: u8, material_id: u8) -> Self {
        Self { flags, material_id }
    }

    /// Returns the raw MOPY behavior flags.
    #[must_use]
    pub const fn flags(self) -> u8 {
        self.flags
    }

    /// Returns the MOMT slot or `0xff` for a non-rendered polygon.
    #[must_use]
    pub const fn material_id(self) -> u8 {
        self.material_id
    }

    /// Returns whether stock submits this polygon to rendering.
    #[must_use]
    pub const fn is_renderable(self) -> bool {
        self.material_id != 0xff && self.flags & 0x20 != 0 && self.flags & 0x04 == 0
    }

    /// Returns whether stock admits this polygon to ordinary world collision.
    #[must_use]
    pub const fn is_collidable(self) -> bool {
        self.flags & 0x08 != 0 || self.is_renderable()
    }

    /// Returns whether the polygon admits camera collision.
    #[must_use]
    pub const fn is_camera_collidable(self) -> bool {
        // MOPY 0x02 is build 12340's F_NOCAMCOLLIDE flag.
        self.is_collidable() && self.flags & 0x02 == 0
    }
}

/// One MOBN axial BSP node with signed child references.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelBspNode {
    flags: u16,
    negative_child: i16,
    positive_child: i16,
    face_count: u16,
    first_face: u32,
    plane_distance: f32,
}

impl WorldModelBspNode {
    pub(super) const fn new(
        flags: u16,
        negative_child: i16,
        positive_child: i16,
        face_count: u16,
        first_face: u32,
        plane_distance: f32,
    ) -> Self {
        Self {
            flags,
            negative_child,
            positive_child,
            face_count,
            first_face,
            plane_distance,
        }
    }

    /// Returns the raw MOBN flags including the axial plane selector.
    #[must_use]
    pub const fn flags(self) -> u16 {
        self.flags
    }

    /// Returns the negative-side child or stock's negative leaf sentinel.
    #[must_use]
    pub const fn negative_child(self) -> i16 {
        self.negative_child
    }

    /// Returns the positive-side child or stock's negative leaf sentinel.
    #[must_use]
    pub const fn positive_child(self) -> i16 {
        self.positive_child
    }

    /// Returns the number of MOBR entries owned by this node.
    #[must_use]
    pub const fn face_count(self) -> u16 {
        self.face_count
    }

    /// Returns the first MOBR entry owned by this node.
    #[must_use]
    pub const fn first_face(self) -> u32 {
        self.first_face
    }

    /// Returns the group-local axial split distance.
    #[must_use]
    pub const fn plane_distance(self) -> f32 {
        self.plane_distance
    }
}
