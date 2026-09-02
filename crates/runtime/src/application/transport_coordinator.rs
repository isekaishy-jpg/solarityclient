//! Referenced player-transport resource ownership during world admission.

use solarity_asset::{
    AssetPath, AssetStoreHandle, BlpTextureCache, GameObjectDisplayCatalog, M2ModelCache,
    WmoModelCache,
};
use solarity_ecs::{ActiveWorld, ObjectKind, WorldTransform};
use thiserror::Error;

use crate::application::terrain_coordinator::RuntimeTerrainError;
use crate::application::terrain_coordinator::m2_residency::ResidentM2Source;
use crate::application::terrain_coordinator::world_model_residency::ResidentWorldModelSource;

/// Failure while admitting the exact display resource owned by a transport.
#[derive(Debug, Error)]
pub enum RuntimeTransportError {
    /// M2, WMO, or authored texture residency failed strict decoding.
    #[error(transparent)]
    Resource(#[from] RuntimeTerrainError),
}

/// Resource family selected by one `GameObjectDisplayInfo.dbc` row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeTransportResourceKind {
    /// An M2, MDX, or MDL generation with its primary SKIN profile.
    M2,
    /// A root WMO with every numbered group.
    WorldModel,
}

/// Observable result of synchronizing the controlled player's transport parent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeTransportPoll {
    /// No active world or transport relationship exists.
    Idle,
    /// The movement relationship names an object that has not entered the world.
    AwaitingObject {
        /// Exact server GUID named by local-player movement.
        guid: u64,
    },
    /// The admitted object owns no supported display resource.
    NoResource {
        /// Exact admitted GameObject GUID.
        guid: u64,
    },
    /// A new M2 or WMO generation and its texture inputs became resident.
    ResourceLoaded {
        /// Exact admitted GameObject GUID.
        guid: u64,
        /// Resource family selected by the display row.
        kind: RuntimeTransportResourceKind,
    },
    /// The previously admitted resource generation remains current.
    Current {
        /// Exact admitted GameObject GUID.
        guid: u64,
        /// Resource family selected by the display row.
        kind: RuntimeTransportResourceKind,
    },
}

/// Complete CPU-side resource retained for the current transport display.
pub(in crate::application) enum ResidentTransportResource {
    /// One decoded M2/SKIN generation and its texture declarations.
    M2(ResidentM2Source),
    /// One decoded root/group WMO generation and its MOMT texture stages.
    WorldModel(ResidentWorldModelSource),
}

impl ResidentTransportResource {
    /// Returns the closed resource family represented by this generation.
    const fn kind(&self) -> RuntimeTransportResourceKind {
        match self {
            Self::M2(_) => RuntimeTransportResourceKind::M2,
            Self::WorldModel(_) => RuntimeTransportResourceKind::WorldModel,
        }
    }
}

/// One transport generation plus the latest authoritative placement inputs.
pub(in crate::application) struct ResidentTransport {
    guid: u64,
    display_id: u32,
    transform: Option<WorldTransform>,
    scale: Option<f32>,
    resource: ResidentTransportResource,
}

impl ResidentTransport {
    /// Returns the exact movement-parent GUID owning this resource.
    pub(in crate::application) const fn guid(&self) -> u64 {
        self.guid
    }

    /// Returns the display identity used to select this generation.
    pub(in crate::application) const fn display_id(&self) -> u32 {
        self.display_id
    }

    /// Returns a finite authoritative transform when movement supplied one.
    pub(in crate::application) const fn transform(&self) -> Option<WorldTransform> {
        self.transform
    }

    /// Returns the positive replicated object scale when it is materialized.
    pub(in crate::application) const fn scale(&self) -> Option<f32> {
        self.scale
    }

    /// Returns the complete decoded M2 or WMO resource generation.
    pub(in crate::application) const fn resource(&self) -> &ResidentTransportResource {
        &self.resource
    }
}

/// Owns only the resource named by the controlled player's transport relation.
pub struct RuntimeTransportPresentation {
    assets: AssetStoreHandle,
    displays: GameObjectDisplayCatalog,
    textures: BlpTextureCache,
    models: M2ModelCache,
    world_models: WmoModelCache,
    resident: Option<ResidentTransport>,
    readiness: bool,
}

impl RuntimeTransportPresentation {
    /// Creates an empty transport owner over the process-wide archive stack.
    #[must_use]
    pub fn new(assets: AssetStoreHandle, displays: GameObjectDisplayCatalog) -> Self {
        Self {
            assets,
            displays,
            textures: BlpTextureCache::new(),
            models: M2ModelCache::new(),
            world_models: WmoModelCache::new(),
            resident: None,
            readiness: true,
        }
    }

    /// Synchronizes stock's loading-card transport resource dependency.
    ///
    /// Ghidra `0x00409800` blocks while the movement-parent object is absent.
    /// Once that object exists, an absent display owner is non-blocking; a
    /// present M2/WMO owner blocks only until its resource request completes.
    /// This implementation performs the equivalent strict request
    /// synchronously and retains its completed generation for GPU publication.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeTransportError`] when a referenced stock M2, WMO, SKIN,
    /// or authored texture cannot be strictly loaded.
    pub fn synchronize(
        &mut self,
        world: Option<&ActiveWorld>,
    ) -> Result<RuntimeTransportPoll, RuntimeTransportError> {
        let Some(world) = world else {
            self.clear();
            self.readiness = true;
            return Ok(RuntimeTransportPoll::Idle);
        };
        let Some(guid) = world.local_player_transport_guid() else {
            self.clear();
            self.readiness = true;
            return Ok(RuntimeTransportPoll::Idle);
        };
        if world.object_kind(guid) != Some(ObjectKind::GameObject) {
            self.clear();
            self.readiness = false;
            return Ok(RuntimeTransportPoll::AwaitingObject { guid });
        }

        let Some(presentation) = world
            .game_object_presentation(guid)
            .filter(|presentation| presentation.display_id() != 0)
        else {
            self.clear();
            self.readiness = true;
            return Ok(RuntimeTransportPoll::NoResource { guid });
        };
        let display_id = presentation.display_id();
        let Some(display) = self.displays.display(display_id) else {
            self.clear();
            self.readiness = true;
            return Ok(RuntimeTransportPoll::NoResource { guid });
        };
        let path = display.asset_path();
        let kind = resource_kind(path.as_str());
        let Some(kind) = kind else {
            self.clear();
            self.readiness = true;
            return Ok(RuntimeTransportPoll::NoResource { guid });
        };
        let transform = world.object_transform(guid).filter(valid_transform);
        let scale = world
            .object_presentation(guid)
            .map(solarity_ecs::ObjectPresentation::scale)
            .filter(|scale| scale.is_finite() && *scale > 0.0);
        if self.resident.as_ref().is_some_and(|resident| {
            resident.guid == guid
                && resident.display_id == display_id
                && resident.resource.kind() == kind
        }) {
            if let Some(resident) = self.resident.as_mut() {
                resident.transform = transform;
                resident.scale = scale;
            }
            self.readiness = true;
            return Ok(RuntimeTransportPoll::Current { guid, kind });
        }

        self.readiness = false;
        let resource = match kind {
            RuntimeTransportResourceKind::M2 => {
                ResidentTransportResource::M2(ResidentM2Source::load(
                    path,
                    &mut self.models,
                    &mut self.textures,
                    &mut self.assets.borrow_mut(),
                )?)
            }
            RuntimeTransportResourceKind::WorldModel => {
                ResidentTransportResource::WorldModel(ResidentWorldModelSource::load(
                    path,
                    &mut self.world_models,
                    &mut self.textures,
                    &mut self.assets.borrow_mut(),
                )?)
            }
        };
        self.resident = Some(ResidentTransport {
            guid,
            display_id,
            transform,
            scale,
            resource,
        });
        self.textures.collect_unused();
        self.models.collect_unused();
        self.world_models.collect_unused();
        self.readiness = true;
        Ok(RuntimeTransportPoll::ResourceLoaded { guid, kind })
    }

    /// Reports whether stock's referenced-object/resource gate is satisfied.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.readiness
    }

    /// Returns the currently retained resource family for diagnostics.
    #[must_use]
    pub fn resident_resource_kind(&self) -> Option<RuntimeTransportResourceKind> {
        self.resident
            .as_ref()
            .map(|resident| resident.resource.kind())
    }

    /// Returns the exact currently retained transport GUID.
    #[must_use]
    pub fn resident_guid(&self) -> Option<u64> {
        self.resident.as_ref().map(ResidentTransport::guid)
    }

    /// Returns the exact display row retained by the current generation.
    #[must_use]
    pub fn resident_display_id(&self) -> Option<u32> {
        self.resident.as_ref().map(ResidentTransport::display_id)
    }

    /// Returns the latest finite authoritative placement, when materialized.
    #[must_use]
    pub fn resident_transform(&self) -> Option<WorldTransform> {
        self.resident
            .as_ref()
            .and_then(ResidentTransport::transform)
    }

    /// Returns the latest positive replicated scale, when materialized.
    #[must_use]
    pub fn resident_scale(&self) -> Option<f32> {
        self.resident.as_ref().and_then(ResidentTransport::scale)
    }

    /// Returns the canonical decoded M2 or root-WMO archive identity.
    #[must_use]
    pub fn resident_asset_path(&self) -> Option<&AssetPath> {
        self.resident
            .as_ref()
            .map(ResidentTransport::resource)
            .map(|resource| match resource {
                ResidentTransportResource::M2(source) => source.model().path(),
                ResidentTransportResource::WorldModel(source) => source.model().path(),
            })
    }

    /// Releases the active-world generation during ordered client shutdown.
    pub fn disconnect(&mut self) {
        self.clear();
        self.readiness = true;
    }

    /// Releases the transport generation and cache-only resource references.
    fn clear(&mut self) {
        self.resident = None;
        self.textures.collect_unused();
        self.models.collect_unused();
        self.world_models.collect_unused();
    }
}

/// Classifies only the resource suffixes accepted by stock display owners.
fn resource_kind(path: &str) -> Option<RuntimeTransportResourceKind> {
    if path.ends_with(".WMO") {
        return Some(RuntimeTransportResourceKind::WorldModel);
    }
    if path.ends_with(".M2") || path.ends_with(".MDX") || path.ends_with(".MDL") {
        return Some(RuntimeTransportResourceKind::M2);
    }
    None
}

/// Rejects partial/nonfinite movement without withholding resource readiness.
fn valid_transform(transform: &WorldTransform) -> bool {
    transform.position().is_finite() && transform.orientation().is_finite()
}
