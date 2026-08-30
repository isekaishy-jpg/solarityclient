//! Archive-backed build-12340 M2 model ownership.

use crate::model::m2_shared::{
    canonical_model_path, model_decode, parse_model, parse_skin, skin_path,
};
use crate::model::model_blob::ModelBlob;
use crate::{
    ArchiveDescriptor, AssetError, AssetPath, AssetStore, M2Material, M2SkinProfile, M2Texture,
    M2Vertex,
};

/// A decoded M2 and every external SKIN profile named by its build-12340 header.
#[derive(Debug)]
pub struct DecodedM2Model {
    path: AssetPath,
    source: ArchiveDescriptor,
    blob: ModelBlob,
    skins: Vec<M2SkinProfile>,
}

impl DecodedM2Model {
    /// Resolves the model and each `ModelNN.skin` companion independently.
    ///
    /// This is deliberately path-based: an HD patch supplies larger replacement
    /// files under the same names and wins through normal archive precedence.
    ///
    /// # Errors
    ///
    /// Returns archive lookup/read errors or [`AssetError::ModelDecode`] when
    /// the model is not exact build-12340 MD20 data, a required external profile
    /// is missing or malformed, or cross-file geometry references are invalid.
    pub fn load(store: &mut AssetStore, path: &AssetPath) -> Result<Self, AssetError> {
        let path = canonical_model_path(path)?;
        let read = store.read(&path)?;
        let source = read.source().clone();
        let model = parse_model(&path, read.bytes())?;
        let profile_count = model.header.num_skin_profiles.ok_or_else(|| {
            model_decode(
                &path,
                "M2 header has no external skin-profile count".to_owned(),
            )
        })?;
        if profile_count == 0 {
            return Err(model_decode(
                &path,
                "M2 header names no external skin profiles".to_owned(),
            ));
        }

        let model_vertex_count = model.vertices.len();
        let blob = ModelBlob::from_model(&path, read.bytes(), model)?;
        let mut skins = Vec::with_capacity(profile_count as usize);
        for profile in 0..profile_count {
            let profile_path = skin_path(&path, profile)?;
            let profile_read = store.read(&profile_path)?;
            let profile_source = profile_read.source().clone();
            let skin = parse_skin(&profile_path, profile_read.bytes())?;
            skins.push(M2SkinProfile::from_skin(
                profile_path,
                profile_source,
                skin,
                model_vertex_count,
            )?);
        }

        Ok(Self {
            path,
            source,
            blob,
            skins,
        })
    }

    /// Returns the exact normalized M2 archive path.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the archive selected for the M2 body.
    #[must_use]
    pub const fn source(&self) -> &ArchiveDescriptor {
        &self.source
    }

    /// Returns the model-internal name when the file carries one.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.blob.name.as_deref()
    }

    /// Returns every exact build-12340 model behavior flag.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.blob.flags
    }

    /// Reports whether stock substitutes shader combiners from the trailing table.
    #[must_use]
    pub const fn uses_texture_combiners(&self) -> bool {
        self.blob.flags & 0x8 != 0
    }

    /// Returns the exact decoded build-12340 vertices.
    #[must_use]
    pub fn vertices(&self) -> &[M2Vertex] {
        &self.blob.vertices
    }

    /// Returns every external view/LOD profile in header order.
    #[must_use]
    pub fn skins(&self) -> &[M2SkinProfile] {
        &self.skins
    }

    /// Returns texture declarations before display/customization replacement.
    #[must_use]
    pub fn textures(&self) -> &[M2Texture] {
        &self.blob.textures
    }

    /// Returns render flags and blend modes referenced by SKIN batches.
    #[must_use]
    pub fn materials(&self) -> &[M2Material] {
        &self.blob.materials
    }

    /// Returns the model bone palette selected by SKIN submesh ranges.
    #[must_use]
    pub fn bone_lookup(&self) -> &[u16] {
        &self.blob.bone_lookup
    }

    /// Returns texture indices selected by SKIN batch texture combos.
    #[must_use]
    pub fn texture_lookup(&self) -> &[u16] {
        &self.blob.texture_lookup
    }

    /// Returns stock texture-unit values parallel to texture combos.
    #[must_use]
    pub fn texture_units(&self) -> &[u16] {
        &self.blob.texture_units
    }

    /// Returns transparency-animation indices selected by SKIN batches.
    #[must_use]
    pub fn transparency_lookup(&self) -> &[u16] {
        &self.blob.transparency_lookup
    }

    /// Returns texture-animation indices selected by SKIN batches.
    #[must_use]
    pub fn texture_animation_lookup(&self) -> &[u16] {
        &self.blob.texture_animation_lookup
    }

    /// Returns the optional stock texture-combiner selectors in table order.
    #[must_use]
    pub fn texture_combiner_combos(&self) -> &[u16] {
        &self.blob.texture_combiner_combos
    }

    /// Returns the number of decoded model bones.
    #[must_use]
    pub const fn bone_count(&self) -> usize {
        self.blob.bone_count
    }

    /// Returns the number of decoded animation sequences.
    #[must_use]
    pub const fn animation_count(&self) -> usize {
        self.blob.animation_count
    }

    /// Returns the number of model texture definitions.
    #[must_use]
    pub const fn texture_count(&self) -> usize {
        self.blob.textures.len()
    }

    /// Returns the number of model render-flag records.
    #[must_use]
    pub const fn material_count(&self) -> usize {
        self.blob.materials.len()
    }
}
