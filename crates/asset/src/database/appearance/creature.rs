//! Creature display, model, and optional NPC appearance resolution.

use crate::archive::AssetPath;
use crate::database::creature::{
    CreatureCatalog, CreatureDisplayInfo, CreatureDisplayInfoExtra, CreatureModelData,
};
use crate::model::M2TextureKind;

use super::AppearanceError;

/// One display ID joined to its M2 metadata and stock texture replacements.
pub struct CreatureModelAppearance<'catalog> {
    display: &'catalog CreatureDisplayInfo,
    model: &'catalog CreatureModelData,
    model_path: &'catalog AssetPath,
    extra: Option<&'catalog CreatureDisplayInfoExtra>,
    baked_texture: Option<AssetPath>,
    monster_textures: [Option<AssetPath>; 3],
}

impl CreatureModelAppearance<'_> {
    /// Returns the display row that selected this appearance.
    #[must_use]
    pub const fn display(&self) -> &CreatureDisplayInfo {
        self.display
    }

    /// Returns the model metadata row selected by the display.
    #[must_use]
    pub const fn model(&self) -> &CreatureModelData {
        self.model
    }

    /// Returns the optional extended player-like NPC appearance.
    #[must_use]
    pub const fn extra(&self) -> Option<&CreatureDisplayInfoExtra> {
        self.extra
    }

    /// Returns the canonical baked atlas for a player-model NPC.
    #[must_use]
    pub const fn baked_texture(&self) -> Option<&AssetPath> {
        self.baked_texture.as_ref()
    }

    /// Returns the required normalized M2 path.
    #[must_use]
    pub const fn model_path(&self) -> &AssetPath {
        self.model_path
    }

    /// Returns one creature texture replacement by its M2 semantic category.
    #[must_use]
    pub fn texture_for(&self, kind: M2TextureKind) -> Option<&AssetPath> {
        let index = match kind {
            M2TextureKind::Monster1 => 0,
            M2TextureKind::Monster2 => 1,
            M2TextureKind::Monster3 => 2,
            M2TextureKind::Hardcoded
            | M2TextureKind::Body
            | M2TextureKind::Item
            | M2TextureKind::WeaponArmorBasic
            | M2TextureKind::WeaponBlade
            | M2TextureKind::WeaponHandle
            | M2TextureKind::Environment
            | M2TextureKind::Hair
            | M2TextureKind::SkinExtra
            | M2TextureKind::UiSkin
            | M2TextureKind::TaurenMane
            | M2TextureKind::ItemIcon => return None,
        };
        self.monster_textures[index].as_ref()
    }

    /// Returns all three creature texture paths in replacement-type order.
    #[must_use]
    pub fn monster_textures(&self) -> [Option<&AssetPath>; 3] {
        self.monster_textures.each_ref().map(Option::as_ref)
    }
}

impl CreatureCatalog {
    /// Joins one display to its required model and optional extended appearance.
    ///
    /// Stock `ReplaceMonsterSkin` replaces the M2 filename component with each
    /// authored texture variation and binds the results to replacement types
    /// 11 through 13. Its subsequent `TextureCreate` call appends `.blp` when
    /// the DBC value has no extension; it does not search another folder.
    ///
    /// # Errors
    ///
    /// Returns [`AppearanceError`] when a nonzero DBC reference is absent or
    /// the selected model row has no path.
    pub fn resolve_model(
        &self,
        display_id: u32,
    ) -> Result<CreatureModelAppearance<'_>, AppearanceError> {
        let display = self
            .display(display_id)
            .ok_or(AppearanceError::MissingCreatureDisplay { display_id })?;
        let model_id = display.model_id();
        let model = self
            .model(model_id)
            .ok_or(AppearanceError::MissingCreatureModel {
                display_id,
                model_id,
            })?;
        let model_path = model
            .model_path()
            .ok_or(AppearanceError::MissingCreatureModelPath {
                display_id,
                model_id,
            })?;
        let extra_id = display.extended_display_info_id();
        let extra = if extra_id == 0 {
            None
        } else {
            Some(self.display_extra(extra_id).ok_or(
                AppearanceError::MissingCreatureDisplayExtra {
                    display_id,
                    extra_id,
                },
            )?)
        };
        let monster_textures = monster_texture_paths(model_path, display.texture_variations());
        let baked_texture = extra
            .map(|extra| {
                npc_baked_texture_path(extra.baked_texture_name()).map_err(|_error| {
                    AppearanceError::InvalidNpcBakedTexture {
                        display_id,
                        extra_id: extra.id(),
                    }
                })
            })
            .transpose()?
            .flatten();
        Ok(CreatureModelAppearance {
            display,
            model,
            model_path,
            extra,
            baked_texture,
            monster_textures,
        })
    }
}

/// Expands the filename-only NPC atlas values used by build 12340.
fn npc_baked_texture_path(name: &str) -> Result<Option<AssetPath>, crate::AssetError> {
    if name.is_empty() {
        return Ok(None);
    }
    let has_directory = name.contains(['\\', '/']);
    let has_extension = name
        .rsplit(['\\', '/'])
        .next()
        .is_some_and(|component| component.contains('.'));
    let prefix = if has_directory {
        ""
    } else {
        "Textures\\BakedNpcTextures\\"
    };
    let suffix = if has_extension { "" } else { ".blp" };
    AssetPath::new(format!("{prefix}{name}{suffix}")).map(Some)
}

/// Replaces only the final model-path component, exactly as stock does.
fn monster_texture_paths(model_path: &AssetPath, names: [&str; 3]) -> [Option<AssetPath>; 3] {
    let prefix_end = model_path
        .as_str()
        .rfind('\\')
        .map_or(0, |separator| separator + 1);
    let prefix = &model_path.as_str()[..prefix_end];
    names.map(|name| {
        if name.is_empty() {
            return None;
        }
        // Both inputs were validated as archive-relative ASCII paths while
        // decoding their tables, so their stock concatenation remains valid.
        let suffix = if name
            .rsplit(['\\', '/'])
            .next()
            .is_some_and(|part| part.contains('.'))
        {
            ""
        } else {
            ".blp"
        };
        let path = match AssetPath::new(format!("{prefix}{name}{suffix}")) {
            Ok(path) => path,
            Err(_) => unreachable!("two validated archive path fragments formed an invalid path"),
        };
        Some(path)
    })
}
