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
    liquid: Option<WorldModelLiquid>,
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
        liquid: Option<WorldModelLiquid>,
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
            liquid,
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

    /// Returns the exact authored MLIQ grid when this group carries one.
    #[must_use]
    pub const fn liquid(&self) -> Option<&WorldModelLiquid> {
        self.liquid.as_ref()
    }

    /// Resolves one build-12340 `LiquidType.dbc` identifier.
    ///
    /// Legacy roots overload the low tile nibble when MOGP liquid type is 15;
    /// newer roots identify liquid types directly through MOHD flag `0x4`.
    #[must_use]
    pub const fn resolve_liquid_type(&self, root_flags: u16, tile: Option<u8>) -> u32 {
        let value = self.liquid_type;
        if value == 0 {
            return 0;
        }
        if root_flags & 0x4 != 0 {
            return if value < 21 {
                local_liquid_type(value - 1, self.flags)
            } else {
                value
            };
        }
        if value == 15
            && let Some(tile) = tile
        {
            let legacy = (tile & 0x0f) as u32;
            return if legacy == 1 {
                14
            } else if legacy == 2 {
                19
            } else if legacy == 3 {
                20
            } else if legacy >= 4 {
                13
            } else {
                0
            };
        }
        if value < 20 {
            local_liquid_type(value, self.flags)
        } else {
            value + 1
        }
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

const fn local_liquid_type(value: u32, group_flags: u32) -> u32 {
    match value & 3 {
        0 if group_flags & 0x8_0000 != 0 => 14,
        0 => 13,
        1 => 14,
        2 => 19,
        _ => 20,
    }
}

/// One exact WotLK MLIQ grid in WMO group-local coordinates.
pub struct WorldModelLiquid {
    vertex_width: u32,
    vertex_height: u32,
    tile_width: u32,
    tile_height: u32,
    corner: [f32; 3],
    material_id: u16,
    vertices: Vec<WorldModelLiquidVertex>,
    tiles: Vec<u8>,
}

impl WorldModelLiquid {
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn new(
        vertex_width: u32,
        vertex_height: u32,
        tile_width: u32,
        tile_height: u32,
        corner: [f32; 3],
        material_id: u16,
        vertices: Vec<WorldModelLiquidVertex>,
        tiles: Vec<u8>,
    ) -> Self {
        Self {
            vertex_width,
            vertex_height,
            tile_width,
            tile_height,
            corner,
            material_id,
            vertices,
            tiles,
        }
    }

    /// Returns the number of stored vertices along local X.
    #[must_use]
    pub const fn vertex_width(&self) -> u32 {
        self.vertex_width
    }

    /// Returns the number of stored vertices along local Y.
    #[must_use]
    pub const fn vertex_height(&self) -> u32 {
        self.vertex_height
    }

    /// Returns the number of authored tiles along local X.
    #[must_use]
    pub const fn tile_width(&self) -> u32 {
        self.tile_width
    }

    /// Returns the number of authored tiles along local Y.
    #[must_use]
    pub const fn tile_height(&self) -> u32 {
        self.tile_height
    }

    /// Returns the group-local lower grid corner.
    #[must_use]
    pub const fn corner(&self) -> [f32; 3] {
        self.corner
    }

    /// Returns the MOMT slot used for liquid diffuse color.
    #[must_use]
    pub const fn material_id(&self) -> u16 {
        self.material_id
    }

    /// Returns column-major MLIQ vertices indexed as `x * height + y`.
    #[must_use]
    pub fn vertices(&self) -> &[WorldModelLiquidVertex] {
        &self.vertices
    }

    /// Returns column-major tile bytes indexed as `x * height + y`.
    #[must_use]
    pub fn tiles(&self) -> &[u8] {
        &self.tiles
    }
}

/// One overloaded eight-byte WotLK MLIQ vertex record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelLiquidVertex {
    flow_one: u8,
    flow_two: u8,
    flow_one_percent: u8,
    texture_s: i16,
    texture_t: i16,
    height: f32,
}

impl WorldModelLiquidVertex {
    pub(super) const fn new(overloaded: [u8; 4], height: f32) -> Self {
        Self {
            flow_one: overloaded[0],
            flow_two: overloaded[1],
            flow_one_percent: overloaded[2],
            texture_s: i16::from_le_bytes([overloaded[0], overloaded[1]]),
            texture_t: i16::from_le_bytes([overloaded[2], overloaded[3]]),
            height,
        }
    }

    /// Returns water's first flow/depth-table coordinate byte.
    #[must_use]
    pub const fn flow_one(self) -> u8 {
        self.flow_one
    }

    /// Returns water's second flow byte.
    #[must_use]
    pub const fn flow_two(self) -> u8 {
        self.flow_two
    }

    /// Returns water's first-flow blend percentage byte.
    #[must_use]
    pub const fn flow_one_percent(self) -> u8 {
        self.flow_one_percent
    }

    /// Returns magma/slime's signed fixed-point S coordinate.
    #[must_use]
    pub const fn texture_s(self) -> i16 {
        self.texture_s
    }

    /// Returns magma/slime's signed fixed-point T coordinate.
    #[must_use]
    pub const fn texture_t(self) -> i16 {
        self.texture_t
    }

    /// Returns the group-local liquid surface height.
    #[must_use]
    pub const fn height(self) -> f32 {
        self.height
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
