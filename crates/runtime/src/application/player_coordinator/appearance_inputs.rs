//! Retained input identities for expensive character appearance composition.

use solarity_ecs::{
    ActiveWorld, ObjectFields, ObjectPresentation, PlayerAppearance, PlayerEquipment, UnitIdentity,
    UnitSheathState, WorldObjectIdentity,
};

/// Matches every mutable input to model, body-scale, equipment and atlas planning.
/// Motion, stand state and animation tier remain on their independent live path.
/// The coordinator owns immutable DBC catalogs for its entire lifetime.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct RemoteAppearanceInputs {
    pub(super) identity: WorldObjectIdentity,
    pub(super) object: ObjectPresentation,
    pub(super) unit: UnitIdentity,
    pub(super) appearance: PlayerAppearance,
    pub(super) equipment: PlayerEquipment,
    pub(super) displays: [u32; 3],
    pub(super) sheath: UnitSheathState,
    pub(super) body_fields: [u32; 3],
}

impl RemoteAppearanceInputs {
    /// Missing projections never become a cache hit; the ordinary resolver keeps
    /// ownership of its established absence and error behavior.
    pub(super) fn read(world: &ActiveWorld, guid: u64) -> Option<Self> {
        let identity = world.object_identity(guid)?;
        let entity = world.entity_by_guid(guid)?;
        let presentation = world.unit_presentation(guid)?;
        let storage = world.storage();
        let fields = storage.get::<&ObjectFields>(entity).ok()?;
        Some(Self {
            identity,
            object: world.object_presentation(guid)?,
            unit: **storage.get::<&UnitIdentity>(entity).ok()?,
            appearance: **storage.get::<&PlayerAppearance>(entity).ok()?,
            equipment: **storage.get::<&PlayerEquipment>(entity).ok()?,
            displays: [
                presentation.display_id(),
                presentation.native_display_id(),
                presentation.mount_display_id(),
            ],
            sheath: presentation.sheath_state(),
            body_fields: [fields.get(67), fields.get(54), fields.get(75)],
        })
    }
}

/// NPC model and weapon dependencies, including native reconciliation state.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct CreatureAppearanceInputs {
    identity: WorldObjectIdentity,
    object: ObjectPresentation,
    presentation: solarity_ecs::UnitPresentation,
    fields: [u32; 3],
    virtual_items: solarity_ecs::UnitVirtualItems,
    flags: solarity_ecs::UnitFlags,
    has_attack_target: bool,
    template: Option<(u32, u32)>,
    body_definition: Option<u32>,
    previous_weapon: Option<(solarity_rendering::NpcWeaponState, u8)>,
}

impl CreatureAppearanceInputs {
    /// Captures all mutable providers used by the native body and weapon joins.
    pub(super) fn read(
        world: &ActiveWorld,
        guid: u64,
        template: Option<(u32, u32)>,
        body_definition: Option<u32>,
        previous_weapon: Option<(solarity_rendering::NpcWeaponState, u8)>,
    ) -> Option<Self> {
        let entity = world.entity_by_guid(guid)?;
        let fields = world.storage().get::<&ObjectFields>(entity).ok()?;
        Some(Self {
            identity: world.object_identity(guid)?,
            object: world.object_presentation(guid)?,
            presentation: world.unit_presentation(guid)?,
            fields: [fields.get(67), fields.get(54), fields.get(75)],
            virtual_items: world.unit_virtual_items(guid).unwrap_or_default(),
            flags: world.unit_flags(guid).unwrap_or_default(),
            has_attack_target: world.unit_attack_target(guid) != 0,
            template,
            body_definition,
            previous_weapon,
        })
    }
}
