//! A private root cursor yields between independently resolved numbered WMO groups.

use super::super::map_obj_group::DecodedWorldModelGroup;
use super::super::map_obj_spatial::WorldModelSpatialData;
use super::super::{
    DecodedWorldModel, WorldModelDoodad, WorldModelDoodadSet, WorldModelFog, WorldModelMaterial,
    WorldModelShader,
};
use super::group::load_group;
use super::layout::{validate_bounds, world_model_error, world_model_message};
use super::root::{
    decode_doodads, decode_fogs, decode_materials, validate_root, validate_root_chunk_layout,
};
use crate::{ArchiveDescriptor, AssetError, AssetPath, AssetStore};
use std::io::Cursor;
use wow_wmo::{ParsedWmo, parse_wmo};

/// No partial WMO escapes this cursor. Group order and final validation match MapObjRead.
pub(crate) struct WorldModelPreparation {
    path: AssetPath,
    source: ArchiveDescriptor,
    root: wow_wmo::root_parser::WmoRoot,
    bounds: [[f32; 3]; 2],
    materials: Vec<WorldModelMaterial>,
    fogs: Vec<WorldModelFog>,
    doodad_sets: Vec<WorldModelDoodadSet>,
    doodads: Vec<WorldModelDoodad>,
    groups: Vec<DecodedWorldModelGroup>,
}

impl WorldModelPreparation {
    /// Validate the root before any numbered source is requested.
    pub(crate) fn begin(store: &mut AssetStore, path: &AssetPath) -> Result<Self, AssetError> {
        let read = store.read(path)?;
        validate_root_chunk_layout(path, read.bytes())?;
        let parsed = parse_wmo(&mut Cursor::new(read.bytes()))
            .map_err(|error| world_model_error(path, error))?;
        let ParsedWmo::Root(root) = parsed else {
            return Err(world_model_message(path, "expected a WMO root file"));
        };
        validate_root(path, &root)?;
        let bounds = validate_bounds(path, root.bounding_box_min, root.bounding_box_max, "MOHD")?;
        let materials = decode_materials(path, read.bytes(), &root)?;
        let fogs = decode_fogs(path, read.bytes())?;
        let (doodad_sets, doodads) = decode_doodads(path, read.bytes(), &root)?;
        if root.flags & 0x02 == 0
            && materials
                .iter()
                .any(|m| m.shader() == WorldModelShader::Composite)
        {
            return Err(world_model_message(
                path,
                "MOMT shader 6 selects stock's null ordinary MapObj effect",
            ));
        }
        let group_count =
            usize::try_from(root.n_groups).map_err(|error| world_model_error(path, error))?;
        Ok(Self {
            path: path.clone(),
            source: read.source().clone(),
            root,
            bounds,
            materials,
            fogs,
            doodad_sets,
            doodads,
            groups: Vec::with_capacity(group_count),
        })
    }

    /// Decode at most one group. A false return means final validation can run next turn.
    pub(crate) fn advance(&mut self, store: &mut AssetStore) -> Result<bool, AssetError> {
        if self.groups.len() == self.root.n_groups as usize {
            return Ok(false);
        }
        self.groups.push(load_group(
            store,
            &self.path,
            self.groups.len() as u32,
            self.root.materials.len(),
            self.root.lights.len(),
            self.root.doodad_defs.len(),
        )?);
        Ok(true)
    }

    /// Preserve original error precedence: every group loads before cross-group fog/spatial checks.
    pub(crate) fn finish(self) -> Result<DecodedWorldModel, AssetError> {
        assert_eq!(
            self.groups.len(),
            self.root.n_groups as usize,
            "WMO publication follows every group"
        );
        for group in &self.groups {
            if group
                .fog_ids()
                .into_iter()
                .any(|index| index != 0 && usize::from(index) >= self.fogs.len())
            {
                return Err(world_model_message(
                    &self.path,
                    "MOGP fog index exceeds the MFOG table",
                ));
            }
        }
        let spatial = WorldModelSpatialData::decode(&self.path, &self.root, &self.groups)?;
        Ok(DecodedWorldModel::new(
            self.path,
            self.source,
            self.root.flags,
            self.root.ambient_color,
            self.root.wmo_id,
            self.bounds,
            spatial,
            self.materials,
            self.doodad_sets,
            self.doodads,
            self.groups,
            self.fogs,
            self.root.skybox,
        ))
    }
}

impl DecodedWorldModel {
    /// Loads a complete root and its independently resolved groups through the same staged reader.
    /// # Errors
    /// Rejects invalid root/group versions, bounds, tables, BSP references or build-era layout.
    pub fn load(store: &mut AssetStore, path: &AssetPath) -> Result<Self, AssetError> {
        let _profile = solarity_profiling::profile!("asset.world_model.map_obj_read.load");
        let mut preparation = WorldModelPreparation::begin(store, path)?;
        while preparation.advance(store)? {}
        preparation.finish()
    }
}
