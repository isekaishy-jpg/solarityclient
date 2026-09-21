//! Nested appearance sources join the same namespace authority as primary models.

use super::super::super::{
    CharacterAttachmentPlan, CharacterItemVisualPlan, CreatureModelKey, ResidentGlueCharacterKey,
    RuntimePlayerError, RuntimePlayerSharedCatalogs,
};
use solarity_asset::{
    AssetPath, AssetResourceKey, AssetStore, DecodedM2Model, M2Load, M2LoadDependency,
    ResourceLease,
};
use solarity_cpu::{
    CpuBuffer, CpuServiceControl, CpuStorageBudget, CpuStorageClass, CpuStorageKind,
    CpuTaskDependency,
};
use std::ops::ControlFlow;

/// Frozen domain inputs; resolving source demand never creates a live instance.
pub(in crate::application::player_coordinator) enum AppearanceSources {
    Player {
        attachments: CharacterAttachmentPlan,
        mount: Option<AssetPath>,
    },
    Creature {
        key: CreatureModelKey,
        definitions: [Option<solarity_asset::ItemDefinition>; 3],
    },
    Glue(ResidentGlueCharacterKey),
}

impl AppearanceSources {
    pub(super) fn resolve(
        self,
        catalogs: &RuntimePlayerSharedCatalogs,
        primary: &DecodedM2Model,
        budget: &CpuStorageBudget,
    ) -> Result<SourceWork, RuntimePlayerError> {
        let (attachments, extra) = match self {
            Self::Player { attachments, mount } => (attachments, mount),
            Self::Creature { key, definitions } => (
                catalogs.creature_attachment_plan(&key, &definitions, primary)?,
                key.mount_key.map(|key| key.path),
            ),
            Self::Glue(key) => {
                let attachments = catalogs.glue_attachment_plan(&key)?;
                let pet = match key {
                    ResidentGlueCharacterKey::Selection(preview)
                        if preview.pet().display_id() != 0 =>
                    {
                        Some(
                            catalogs
                                .creatures
                                .resolve_model(preview.pet().display_id())?
                                .model_path()
                                .clone(),
                        )
                    }
                    _ => None,
                };
                (attachments, pet)
            }
        };
        let mut entries = CpuBuffer::default();
        for attachment in attachments.attachments() {
            let visual = CharacterItemVisualPlan::resolve(attachment, &catalogs.item_visuals);
            entries.reserve(
                budget,
                CpuStorageClass::Required,
                CpuStorageKind::Metadata,
                entries.len() + 1 + visual.effects().len(),
            )?;
            let parent = entries.len();
            entries.push(SourceEntry {
                path: attachment.model().clone(),
                parent: None,
            })?;
            for effect in visual.effects() {
                entries.push(SourceEntry {
                    path: effect.model().clone(),
                    parent: Some((parent, effect.attachment_id())),
                })?;
            }
        }
        if let Some(path) = extra {
            entries.reserve(
                budget,
                CpuStorageClass::Required,
                CpuStorageKind::Metadata,
                entries.len() + 1,
            )?;
            entries.push(SourceEntry { path, parent: None })?;
        }
        let mut models = CpuBuffer::default();
        models.reserve(
            budget,
            CpuStorageClass::Required,
            CpuStorageKind::Result,
            entries.len(),
        )?;
        Ok(SourceWork {
            entries,
            models,
            pending: None,
        })
    }
}

struct SourceEntry {
    path: AssetPath,
    parent: Option<(usize, u32)>,
}

/// Ready leases pin every nested input through the final derived preparation call.
pub(super) struct SourceWork {
    entries: CpuBuffer<SourceEntry>,
    models: CpuBuffer<Option<ResourceLease<DecodedM2Model>>>,
    pending: Option<M2LoadDependency>,
}

impl SourceWork {
    /// Resolves one source per turn. Missing attachment links never request an effect.
    pub(super) fn step(
        &mut self,
        store: &mut AssetStore,
        budget: &CpuStorageBudget,
        service: &CpuServiceControl,
    ) -> Result<ControlFlow<(), Option<CpuTaskDependency>>, RuntimePlayerError> {
        let Some(entry) = self.entries.get(self.models.len()) else {
            return Ok(ControlFlow::Break(()));
        };
        if let Some((parent, attachment)) = entry.parent
            && self.models[parent]
                .as_ref()
                .is_none_or(|model| model.attachment(attachment).is_none())
        {
            self.models.push(None)?;
            return Ok(ControlFlow::Continue(None));
        }
        let model = if let Some(dependency) = self.pending.take() {
            dependency
                .poll()
                .unwrap_or_else(|| unreachable!("appearance resumes after source readiness"))?
        } else {
            let key = AssetResourceKey::new(store.namespace(), entry.path.clone());
            match store
                .model_cache_service()
                .request_for(&key, service.service())?
            {
                M2Load::Ready(model) => model,
                // This producer completes in the current indivisible bulk turn.
                // Binding its short-lived demand to the whole appearance would
                // demote the remaining work when this source's subscribers leave.
                M2Load::Producer(producer) => producer.load(store)?,
                M2Load::Pending(request) => {
                    let dependency = request.dependency(budget, CpuStorageClass::Required)?;
                    let edge = dependency.task_dependency()?;
                    self.pending = Some(dependency);
                    return Ok(ControlFlow::Continue(Some(edge)));
                }
            }
        };
        self.models.push(Some(model))?;
        Ok(ControlFlow::Continue(None))
    }
}

#[cfg(test)]
#[path = "../../../../../tests/application/appearance_sources.rs"]
mod tests;
