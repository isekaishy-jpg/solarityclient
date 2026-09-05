//! Archive-backed build-12340 M2 model ownership.

use crate::model::M2AnimationSet;
use crate::model::lookups::M2LookupTables;
use crate::model::m2_shared::{canonical_model_path, model_decode, skin_path};
use crate::model::model_blob::{ModelBlob, ModelBodyHeader};
use crate::{
    ArchiveDescriptor, AssetError, AssetPath, AssetStore, M2Attachment, M2Material, M2ModelBounds,
    M2SkinProfile, M2Texture, M2Vertex,
};

/// A decoded M2 and the external SKIN profiles requested by its load boundary.
#[derive(Debug)]
pub struct DecodedM2Model {
    path: AssetPath,
    source: ArchiveDescriptor,
    blob: ModelBlob,
    animations: M2AnimationSet,
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
        Self::load_profiles(store, path, SkinProfileLoad::All)
    }

    /// Resolves only build 12340's highest-capability `00.skin` companion.
    ///
    /// The stock runtime selects one external view when the shared M2 enters
    /// memory. Vulkan 1.3 satisfies that highest-capability branch, so runtime
    /// caches should not read the other, potentially HD-sized, profiles.
    /// [`Self::load`] remains the exhaustive validation/tooling boundary.
    ///
    /// # Errors
    ///
    /// Returns the same strict M2 and SKIN failures as [`Self::load`], scoped
    /// to the required primary external profile.
    pub fn load_primary_profile(
        store: &mut AssetStore,
        path: &AssetPath,
    ) -> Result<Self, AssetError> {
        Self::load_profiles(store, path, SkinProfileLoad::Primary)
    }

    /// Shares body/animation decoding while varying only external view demand.
    fn load_profiles(
        store: &mut AssetStore,
        path: &AssetPath,
        profile_load: SkinProfileLoad,
    ) -> Result<Self, AssetError> {
        let path = canonical_model_path(path)?;
        let read = store.read(&path)?;
        let source = read.source().clone();
        let model_bytes = read.into_bytes();
        let header = ModelBodyHeader::decode(&path, &model_bytes)?;
        let profile_count = header.skin_profile_count();
        if profile_count == 0 {
            return Err(model_decode(
                &path,
                "M2 header names no external skin profiles".to_owned(),
            ));
        }

        let model_vertex_count = header.vertex_count();
        let animations = M2AnimationSet::load(store, &path, &model_bytes)?;
        let lookups = M2LookupTables::decode(
            &path,
            &model_bytes,
            animations.bones().len(),
            header.texture_count(),
            animations.texture_weights().len(),
            animations.texture_transforms().len(),
        )?;
        let blob = ModelBlob::decode(&path, &model_bytes, header, lookups)?;
        let loaded_profile_count = match profile_load {
            SkinProfileLoad::All => profile_count,
            SkinProfileLoad::Primary => 1,
        };
        let mut skins = Vec::with_capacity(loaded_profile_count as usize);
        for profile in 0..loaded_profile_count {
            let profile_path = skin_path(&path, profile)?;
            let profile_read = store.read(&profile_path)?;
            let profile_source = profile_read.source().clone();
            skins.push(M2SkinProfile::decode(
                profile_path,
                profile_source,
                profile_read.bytes(),
                model_vertex_count,
            )?);
        }
        validate_material_animation_references(&path, &blob, &animations, &skins)?;

        Ok(Self {
            path,
            source,
            blob,
            animations,
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

    /// Returns the authored external SKIN/view count even when runtime loading
    /// deliberately retains only the highest-capability primary profile.
    #[must_use]
    pub const fn skin_profile_count(&self) -> u32 {
        self.blob.skin_profile_count
    }

    /// Returns the authored model-space render bounds.
    #[must_use]
    pub const fn bounds(&self) -> M2ModelBounds {
        self.blob.bounds
    }

    /// Returns header +0xBC even when no dedicated collision faces are authored.
    ///
    /// GameObject model admission and spatial registration use these bounds
    /// independently of the collision mesh and header +0xA0 render bounds.
    #[must_use]
    pub const fn collision_bounds(&self) -> M2ModelBounds {
        self.blob.collision_bounds
    }

    /// Resolves an authored attachment through the M2 attachment lookup table.
    ///
    /// The lookup is authoritative. Missing slots and `0xFFFF` entries return
    /// `None`; this boundary does not search the attachment array as a fallback.
    #[must_use]
    pub fn attachment(&self, id: u32) -> Option<&M2Attachment> {
        let slot = usize::try_from(id).ok()?;
        let index = *self.animations.attachment_lookup().get(slot)?;
        (index != u16::MAX)
            .then(|| self.animations.attachments().get(usize::from(index)))
            .flatten()
    }

    /// Returns every authored attachment record in file order.
    #[must_use]
    pub fn attachments(&self) -> &[M2Attachment] {
        self.animations.attachments()
    }

    /// Returns the raw attachment-to-record lookup table.
    #[must_use]
    pub fn attachment_lookup(&self) -> &[u16] {
        self.animations.attachment_lookup()
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

    /// Returns the dedicated unanimated collision mesh when authored.
    #[must_use]
    pub const fn collision_mesh(&self) -> Option<&crate::M2CollisionMesh> {
        self.blob.collision.as_ref()
    }

    /// Returns loaded external view/LOD profiles in header order.
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

    /// Returns texture-kind slots mapped to replaceable texture declarations.
    #[must_use]
    pub fn replaceable_texture_lookup(&self) -> &[u16] {
        &self.blob.replaceable_texture_lookup
    }

    /// Returns texture indices selected by SKIN batch texture combos.
    #[must_use]
    pub fn texture_lookup(&self) -> &[u16] {
        &self.blob.texture_lookup
    }

    /// Returns signed stock texture-coordinate selectors parallel to combos.
    #[must_use]
    pub fn texture_coordinate_lookup(&self) -> &[i16] {
        &self.blob.texture_coordinate_lookup
    }

    /// Returns texture-weight indices selected by SKIN batches.
    #[must_use]
    pub fn texture_weight_lookup(&self) -> &[u16] {
        &self.blob.transparency_lookup
    }

    /// Returns texture-transform indices selected by SKIN batches.
    #[must_use]
    pub fn texture_transform_lookup(&self) -> &[u16] {
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
        self.animations.bone_count()
    }

    /// Returns the number of decoded animation sequences.
    #[must_use]
    pub fn animation_count(&self) -> usize {
        self.animations.sequences().len()
    }

    /// Returns decoded sequence metadata and nested bone animation channels.
    #[must_use]
    pub const fn animations(&self) -> &M2AnimationSet {
        &self.animations
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

/// External SKIN demand for exhaustive tooling versus the stock live runtime.
#[derive(Clone, Copy)]
enum SkinProfileLoad {
    All,
    Primary,
}

/// Proves every SKIN animation selector against the decoded M2 tables.
fn validate_material_animation_references(
    path: &AssetPath,
    blob: &ModelBlob,
    animations: &M2AnimationSet,
    skins: &[M2SkinProfile],
) -> Result<(), AssetError> {
    validate_lookup(
        path,
        "texture-weight lookup",
        &blob.transparency_lookup,
        animations.texture_weights().len(),
    )?;
    validate_lookup(
        path,
        "texture-transform lookup",
        &blob.texture_animation_lookup,
        animations.texture_transforms().len(),
    )?;
    for skin in skins {
        for (batch_index, batch) in skin.batches().iter().enumerate() {
            if batch.color_index != u16::MAX
                && usize::from(batch.color_index) >= animations.colors().len()
            {
                return Err(model_decode(
                    skin.path(),
                    format!("batch {batch_index} color {} is missing", batch.color_index),
                ));
            }
            validate_combo_span(
                skin.path(),
                batch_index,
                "texture-weight",
                batch.texture_weight_combo_index,
                1,
                &blob.transparency_lookup,
            )?;
            validate_combo_span(
                skin.path(),
                batch_index,
                "texture-transform",
                batch.texture_transform_combo_index,
                batch.texture_count,
                &blob.texture_animation_lookup,
            )?;
        }
    }
    for (ribbon_index, ribbon) in animations.ribbons().iter().enumerate() {
        if ribbon.texture_indices().len() != ribbon.material_indices().len() {
            return Err(model_decode(
                path,
                format!(
                    "ribbon {ribbon_index} has {} textures for {} material passes",
                    ribbon.texture_indices().len(),
                    ribbon.material_indices().len()
                ),
            ));
        }
        for texture_index in ribbon.texture_indices() {
            if usize::from(*texture_index) >= blob.textures.len() {
                return Err(model_decode(
                    path,
                    format!("ribbon {ribbon_index} references missing texture {texture_index}"),
                ));
            }
        }
        for material_index in ribbon.material_indices() {
            if usize::from(*material_index) >= blob.materials.len() {
                return Err(model_decode(
                    path,
                    format!("ribbon {ribbon_index} references missing material {material_index}"),
                ));
            }
        }
        let color_index = ribbon.color_index();
        if color_index >= 0 && color_index as usize >= animations.colors().len() {
            return Err(model_decode(
                path,
                format!("ribbon {ribbon_index} references missing color {color_index}"),
            ));
        }
        let transform_lookup = ribbon.texture_transform_lookup_index();
        if transform_lookup >= 0 && transform_lookup as usize >= blob.texture_animation_lookup.len()
        {
            return Err(model_decode(
                path,
                format!(
                    "ribbon {ribbon_index} references missing texture-transform lookup {transform_lookup}"
                ),
            ));
        }
    }
    for (particle_index, particle) in animations.particles().iter().enumerate() {
        for texture_index in particle.texture_indices().into_iter().flatten() {
            if usize::from(texture_index) >= blob.textures.len() {
                return Err(model_decode(
                    path,
                    format!("particle {particle_index} references missing texture {texture_index}"),
                ));
            }
        }
    }
    Ok(())
}

fn validate_lookup(
    path: &AssetPath,
    field: &str,
    lookup: &[u16],
    target_count: usize,
) -> Result<(), AssetError> {
    for (index, value) in lookup.iter().copied().enumerate() {
        if value != u16::MAX && usize::from(value) >= target_count {
            return Err(model_decode(
                path,
                format!("{field} {index} references missing entry {value}"),
            ));
        }
    }
    Ok(())
}

fn validate_combo_span(
    path: &AssetPath,
    batch_index: usize,
    field: &str,
    first: u16,
    count: u16,
    lookup: &[u16],
) -> Result<(), AssetError> {
    // Stock item M2s commonly retain a zero combo word while omitting the
    // entire optional lookup table. In that representation the material owns
    // no animation selector and consumes the identity value.
    if lookup.is_empty() || first == u16::MAX {
        return Ok(());
    }
    let start = usize::from(first);
    let end = start.checked_add(usize::from(count)).ok_or_else(|| {
        model_decode(
            path,
            format!("batch {batch_index} {field} combo span overflows"),
        )
    })?;
    if end > lookup.len() {
        return Err(model_decode(
            path,
            format!("batch {batch_index} {field} combo span is missing"),
        ));
    }
    Ok(())
}
