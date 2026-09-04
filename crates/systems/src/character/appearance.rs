//! Joins authoritative ECS unit state to stock model appearance catalogs.

use solarity_asset::{
    AppearanceError, CharacterAppearanceCatalog, CharacterCustomization, CharacterModelAppearance,
    CreatureCatalog, CreatureModelAppearance,
};
use solarity_ecs::{
    ActiveWorld, ObjectKind, ObjectPresentation, PlayerAppearance, UnitIdentity, UnitPresentation,
};
use thiserror::Error;

/// A visible unit resolved into body, customization, and optional mount models.
pub struct UnitModelAppearance<'catalog> {
    guid: u64,
    object_scale: f32,
    native_display_id: u32,
    body: CreatureModelAppearance<'catalog>,
    character: Option<CharacterModelAppearance<'catalog>>,
    player_class_id: Option<u8>,
    mount: Option<CreatureModelAppearance<'catalog>>,
}

impl UnitModelAppearance<'_> {
    /// Returns the authoritative server GUID whose state produced this plan.
    #[must_use]
    pub const fn guid(&self) -> u64 {
        self.guid
    }

    /// Returns `OBJECT_FIELD_SCALE_X` without applying a guessed default.
    #[must_use]
    pub const fn object_scale(&self) -> f32 {
        self.object_scale
    }

    /// Returns the unmorphed display ID retained for stock model transitions.
    #[must_use]
    pub const fn native_display_id(&self) -> u32 {
        self.native_display_id
    }

    /// Returns the model selected by the active unit display ID.
    #[must_use]
    pub const fn body(&self) -> &CreatureModelAppearance<'_> {
        &self.body
    }

    /// Returns player texture/geoset composition for player objects only.
    #[must_use]
    pub const fn character(&self) -> Option<&CharacterModelAppearance<'_>> {
        self.character.as_ref()
    }

    /// Returns the authoritative player class used by character geoset rules.
    #[must_use]
    pub const fn player_class_id(&self) -> Option<u8> {
        self.player_class_id
    }

    /// Returns the active mount model when `UNIT_FIELD_MOUNTDISPLAYID` is nonzero.
    #[must_use]
    pub const fn mount(&self) -> Option<&CreatureModelAppearance<'_>> {
        self.mount.as_ref()
    }
}

/// A missing ECS component or DBC reference needed for unit presentation.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum UnitModelAppearanceError {
    /// The GUID is absent from the active-world registry.
    #[error("model appearance references unknown object {guid:#018X}")]
    UnknownObject {
        /// Unresolved server GUID.
        guid: u64,
    },
    /// The indexed object has not received its create-time category.
    #[error("model appearance references untyped object {guid:#018X}")]
    MissingObjectKind {
        /// Server GUID with incomplete create state.
        guid: u64,
    },
    /// Only stock unit and player categories have unit model fields.
    #[error("model appearance references non-unit object {guid:#018X}")]
    NotUnit {
        /// Server GUID with another object category.
        guid: u64,
    },
    /// Common object presentation state is missing.
    #[error("unit {guid:#018X} has no object presentation")]
    MissingObjectPresentation {
        /// Server GUID with incomplete projected fields.
        guid: u64,
    },
    /// Unit display fields have not been projected.
    #[error("unit {guid:#018X} has no unit presentation")]
    MissingUnitPresentation {
        /// Server GUID with incomplete projected fields.
        guid: u64,
    },
    /// A player lacks its projected race and gender bytes.
    #[error("player {guid:#018X} has no unit identity")]
    MissingUnitIdentity {
        /// Player server GUID.
        guid: u64,
    },
    /// A player lacks its five customization bytes.
    #[error("player {guid:#018X} has no customization")]
    MissingPlayerAppearance {
        /// Player server GUID.
        guid: u64,
    },
    /// A projected DBC identifier or customization key does not resolve.
    #[error(transparent)]
    Asset(#[from] AppearanceError),
}

/// Resolves one visible unit without storing asset or renderer types in ECS.
///
/// The active display selects the body. Player objects additionally use their
/// race, gender, and five customization bytes. A nonzero mount display is
/// resolved independently through the same creature catalog, preserving patch
/// precedence for every model and texture path reached later.
///
/// # Errors
///
/// Returns [`UnitModelAppearanceError`] when the GUID is absent, required
/// projected components are missing, the object is not a unit/player, or any
/// required stock appearance reference fails to resolve.
pub fn resolve_unit_model<'catalog>(
    world: &ActiveWorld,
    guid: u64,
    creatures: &'catalog CreatureCatalog,
    characters: &'catalog CharacterAppearanceCatalog,
) -> Result<UnitModelAppearance<'catalog>, UnitModelAppearanceError> {
    let entity = world
        .entity_by_guid(guid)
        .ok_or(UnitModelAppearanceError::UnknownObject { guid })?;
    let kind = world
        .storage()
        .get::<&ObjectKind>(entity)
        .map(|kind| **kind)
        .map_err(|_| UnitModelAppearanceError::MissingObjectKind { guid })?;
    if !matches!(kind, ObjectKind::Unit | ObjectKind::Player) {
        return Err(UnitModelAppearanceError::NotUnit { guid });
    }
    let object = world
        .storage()
        .get::<&ObjectPresentation>(entity)
        .map(|object| **object)
        .map_err(|_| UnitModelAppearanceError::MissingObjectPresentation { guid })?;
    let presentation = world
        .storage()
        .get::<&UnitPresentation>(entity)
        .map(|presentation| **presentation)
        .map_err(|_| UnitModelAppearanceError::MissingUnitPresentation { guid })?;

    let body = creatures.resolve_model(presentation.display_id())?;
    let mount = if presentation.mount_display_id() == 0 {
        None
    } else {
        Some(creatures.resolve_model(presentation.mount_display_id())?)
    };
    let (character, player_class_id) = if kind == ObjectKind::Player {
        let identity = world
            .storage()
            .get::<&UnitIdentity>(entity)
            .map(|identity| **identity)
            .map_err(|_| UnitModelAppearanceError::MissingUnitIdentity { guid })?;
        let appearance = world
            .storage()
            .get::<&PlayerAppearance>(entity)
            .map(|appearance| **appearance)
            .map_err(|_| UnitModelAppearanceError::MissingPlayerAppearance { guid })?;
        let customization = CharacterCustomization::new(
            appearance.skin_id(),
            appearance.face_id(),
            appearance.hair_style_id(),
            appearance.hair_color_id(),
            appearance.facial_hair_style_id(),
        );
        (
            Some(characters.resolve_player_for_class(
                u32::from(identity.race_id()),
                u32::from(identity.gender_id()),
                identity.class_id(),
                customization,
            )?),
            Some(identity.class_id()),
        )
    } else {
        (None, None)
    };

    Ok(UnitModelAppearance {
        guid,
        object_scale: object.scale(),
        native_display_id: presentation.native_display_id(),
        body,
        character,
        player_class_id,
        mount,
    })
}
