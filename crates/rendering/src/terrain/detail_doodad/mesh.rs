//! Native detail mesh expansion, terrain alignment, and four texture buckets.

use glam::{Mat3, Vec3};
use solarity_asset::{AssetPath, DecodedM2Model, GroundEffectCatalog};

use super::placement::{
    GroundDetailDensity, GroundDetailError, GroundDetailPlacement, TerrainDetailChunk,
};

/// The first SKIN and first texture used directly by DetailDoodad.cpp.
pub struct GroundDetailModel {
    texture: AssetPath,
    vertices: Vec<([f32; 3], [f32; 2])>,
    indices: Vec<u16>,
}

impl GroundDetailModel {
    /// Resolves native 7B1B10/7B1B50's first texture and direct SKIN lookup.
    ///
    /// # Errors
    /// Rejects missing first-profile or texture-filename inputs.
    pub fn prepare(model: &DecodedM2Model) -> Result<Self, GroundDetailError> {
        let texture = model
            .textures()
            .first()
            .and_then(|texture| texture.filename())
            .ok_or_else(|| GroundDetailError::ModelInput(model.path().clone()))?
            .clone();
        let skin = model
            .skins()
            .first()
            .ok_or_else(|| GroundDetailError::ModelInput(model.path().clone()))?;
        let vertices = skin
            .vertex_lookup()
            .iter()
            .map(|index| {
                let vertex = model.vertices()[usize::from(*index)];
                (
                    vertex.position().to_array(),
                    vertex.texture_coordinates()[0].to_array(),
                )
            })
            .collect();
        Ok(Self {
            texture,
            vertices,
            indices: skin.triangle_lookup().to_vec(),
        })
    }

    /// Returns the single texture used by every instance of this detail model.
    #[must_use]
    pub const fn texture(&self) -> &AssetPath {
        &self.texture
    }
}

/// The native 36-byte vertex contains terrain normals and authored shadow alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundDetailVertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [u8; 4],
    coordinates: [f32; 2],
}

impl GroundDetailVertex {
    /// Returns the position relative to the chunk's shared origin.
    #[must_use]
    pub const fn position(self) -> [f32; 3] {
        self.position
    }
    /// Returns the unrotated geometric terrain normal used for lighting.
    #[must_use]
    pub const fn normal(self) -> [f32; 3] {
        self.normal
    }
    /// Returns RGBA terrain tint and authored shadow visibility.
    #[must_use]
    pub const fn color(self) -> [u8; 4] {
        self.color
    }
    /// Returns the first authored model texture coordinate.
    #[must_use]
    pub const fn coordinates(self) -> [f32; 2] {
        self.coordinates
    }

    /// Serializes the dedicated vertex ABI without native Rust layout assumptions.
    fn append_bytes(self, bytes: &mut Vec<u8>) {
        for value in self.position.into_iter().chain(self.normal) {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(self.color);
        for value in self.coordinates {
            bytes.extend(value.to_le_bytes());
        }
    }
}

/// One native texture bucket's range within the combined chunk index bank.
pub struct GroundDetailBatch {
    texture: AssetPath,
    first_index: u32,
    index_count: u32,
}

impl GroundDetailBatch {
    /// Returns the exact texture shared by the bucket's selected placements.
    #[must_use]
    pub const fn texture(&self) -> &AssetPath {
        &self.texture
    }
    /// Returns the combined first index and index count.
    #[must_use]
    pub const fn index_range(&self) -> [u32; 2] {
        [self.first_index, self.index_count]
    }
}

/// Immutable, texture-grouped detail geometry for one density/chunk generation.
pub struct GroundDetailMeshPlan {
    origin: [f32; 3],
    vertices: Vec<GroundDetailVertex>,
    indices: Vec<u16>,
    batches: Vec<GroundDetailBatch>,
}

impl GroundDetailMeshPlan {
    /// Reproduces native 7B31E0's strict capacity tests and 7B1B50's transforms.
    ///
    /// The caller provides the resident first-profile model for every placement.
    /// # Errors
    /// Rejects absent models or geometry exceeding the native 16-bit index bank.
    pub fn prepare<'a>(
        chunk: &TerrainDetailChunk,
        catalog: &GroundEffectCatalog,
        density: GroundDetailDensity,
        mut resolve: impl FnMut(u32) -> Option<&'a GroundDetailModel>,
    ) -> Result<Self, GroundDetailError> {
        let capacity = (usize::from(density.cells()) * 64).min(4096);
        let mut buckets = Vec::<Bucket>::with_capacity(4);
        for &placement in chunk.placements() {
            let model = resolve(placement.model())
                .ok_or(GroundDetailError::MissingDoodad(placement.model()))?;
            let matching = buckets.iter().position(|bucket| {
                bucket.texture == model.texture
                    && bucket.vertex_count + model.vertices.len() < capacity
                    && bucket.index_count + model.indices.len() < capacity
            });
            let index = if let Some(index) = matching {
                index
            } else {
                if buckets.len() == 4 {
                    continue;
                }
                buckets.push(Bucket {
                    texture: model.texture.clone(),
                    placements: Vec::new(),
                    vertex_count: 0,
                    index_count: 0,
                });
                buckets.len() - 1
            };
            let bucket = &mut buckets[index];
            bucket.vertex_count += model.vertices.len();
            bucket.index_count += model.indices.len();
            bucket.placements.push((placement, model));
        }
        let mut result = Self {
            origin: chunk.origin(),
            vertices: Vec::new(),
            indices: Vec::new(),
            batches: Vec::with_capacity(buckets.len()),
        };
        for bucket in buckets {
            let first_index = result.indices.len() as u32;
            let mut transforms = FaceTransforms::default();
            for (placement, model) in bucket.placements {
                let flags = catalog
                    .doodad(placement.model())
                    .ok_or(GroundDetailError::MissingDoodad(placement.model()))?
                    .flags();
                let transform = transforms.select(placement, flags);
                let first_vertex = u16::try_from(result.vertices.len())
                    .map_err(|_| GroundDetailError::MeshCapacity)?;
                if result.vertices.len() + model.vertices.len() > usize::from(u16::MAX) {
                    return Err(GroundDetailError::MeshCapacity);
                }
                result
                    .vertices
                    .extend(model.vertices.iter().map(|&(position, coordinates)| {
                        GroundDetailVertex {
                            position: ((transform * Vec3::from_array(position))
                                * placement.scale()
                                + Vec3::from_array(placement.position()))
                            .to_array(),
                            normal: placement.normal(),
                            color: placement.color(),
                            coordinates,
                        }
                    }));
                result
                    .indices
                    .extend(model.indices.iter().map(|index| first_vertex + index));
            }
            result.batches.push(GroundDetailBatch {
                texture: bucket.texture,
                first_index,
                index_count: result.indices.len() as u32 - first_index,
            });
        }
        Ok(result)
    }

    /// Returns the common world-space origin for chunk-local vertices.
    #[must_use]
    pub const fn origin(&self) -> [f32; 3] {
        self.origin
    }
    /// Returns vertices in native texture-bucket order.
    #[must_use]
    pub fn vertices(&self) -> &[GroundDetailVertex] {
        &self.vertices
    }
    /// Returns complete triangle indices into the combined vertex bank.
    #[must_use]
    pub fn indices(&self) -> &[u16] {
        &self.indices
    }
    /// Returns at most four accepted native texture buckets.
    #[must_use]
    pub fn batches(&self) -> &[GroundDetailBatch] {
        &self.batches
    }
    /// Serializes immutable vertex and index arrays for one upload.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(self.vertices.len() * 36 + self.indices.len() * 2);
        for vertex in &self.vertices {
            vertex.append_bytes(&mut result);
        }
        for index in &self.indices {
            result.extend(index.to_le_bytes());
        }
        result
    }
}

/// Preserves insertion order and the native per-texture capacity counters.
struct Bucket<'a> {
    texture: AssetPath,
    placements: Vec<(GroundDetailPlacement, &'a GroundDetailModel)>,
    vertex_count: usize,
    index_count: usize,
}

/// Native aligned instances reuse the first rotation on each consecutive cell face.
#[derive(Default)]
struct FaceTransforms {
    cell: Option<u16>,
    faces: [Option<Mat3>; 4],
}

impl FaceTransforms {
    /// Selects either ordinary Z rotation or the native cached slope basis.
    fn select(&mut self, placement: GroundDetailPlacement, flags: u32) -> Mat3 {
        let rotation = Mat3::from_rotation_z(placement.angle());
        if flags & 1 == 0 {
            return rotation;
        }
        let cell = placement.face() & 0xfc;
        if self.cell != Some(cell) {
            self.cell = Some(cell);
            self.faces = [None; 4];
        }
        let face = usize::from(placement.face() & 3);
        *self.faces[face].get_or_insert_with(|| {
            let normal = Vec3::from_array(placement.normal());
            let tangent = Vec3::new(0.0, normal.z, -normal.y).normalize();
            let basis = Mat3::from_cols(normal.cross(tangent), tangent, normal);
            basis * rotation
        })
    }
}
