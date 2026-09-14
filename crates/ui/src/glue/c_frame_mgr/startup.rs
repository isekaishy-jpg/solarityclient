//! Archive preparation and live FrameXML ownership admission.

use std::collections::HashMap;

use solarity_asset::{AssetStore, AssetStoreHandle};

use super::FrameManager;
use crate::glue::{GlueError, GlueManager};
use crate::xml::UiSourceImage;
use crate::{
    AddonCatalog, UiBindingAssignments, UiBindingCatalog, UiBundle, UiManifestKind,
    UiScriptEnvironment,
};

/// Owned built-in declarations that can be prepared before world facts are ready.
/// No Lua state, callbacks, live world handles or GPU resources cross threads.
pub struct FrameUiSources {
    catalog: UiBindingCatalog,
    bindings: UiBindingAssignments,
    declarations: UiSourceImage,
}

impl FrameUiSources {
    /// Loads the exact binding image and expanded FrameXML source order.
    /// Lua is compiled for validation on this thread and is never executed.
    ///
    /// # Errors
    /// Returns the first binding, archive, XML or Lua validation failure.
    pub fn load(assets: &mut AssetStore) -> Result<Self, GlueError> {
        let _profile = solarity_profiling::profile!("ui.frame.source_preparation");
        let catalog = UiBindingCatalog::load_builtin(assets)?;
        let bindings = UiBindingAssignments::load_defaults(assets, &catalog)?;
        let (declarations, _validator) = UiSourceImage::load(assets, UiManifestKind::Frame)?;
        Ok(Self {
            catalog,
            bindings,
            declarations,
        })
    }
}

impl FrameManager {
    /// Loads, executes, and retains the stock FrameXML manifest.
    ///
    /// The supplied environment must already contain the selected character's
    /// synchronous world facts. This owner attaches the shared asset stack,
    /// profile CVars, AddOn catalog, and stock default binding image before Lua
    /// receives control.
    ///
    /// # Errors
    ///
    /// Returns [`GlueError`] at the first archive, XML, layout, object, font,
    /// texture, or Lua compatibility boundary.
    pub fn start_shared(
        assets: AssetStoreHandle,
        environment: UiScriptEnvironment,
        cvar_values: &[(String, String)],
        addon_catalog: &AddonCatalog,
    ) -> Result<Self, GlueError> {
        let (catalog, bindings, bundle) = {
            let mut store = assets.borrow_mut();
            let catalog = UiBindingCatalog::load_builtin(&mut store)?;
            let bindings = UiBindingAssignments::load_defaults(&mut store, &catalog)?;
            let bundle = UiBundle::load(&mut store, UiManifestKind::Frame)?;
            (catalog, bindings, bundle)
        };
        Self::start_with_bundle(
            assets,
            environment,
            cvar_values,
            addon_catalog,
            catalog,
            bindings,
            bundle,
        )
    }

    /// Constructs FrameXML from previously validated archive sources.
    /// Source preparation executes no Lua and may run on a bounded CPU worker;
    /// this method keeps the live environment and all callbacks on the caller.
    ///
    /// # Errors
    /// Returns the same construction and execution errors as [`Self::start_shared`].
    pub fn start_with_sources(
        assets: AssetStoreHandle,
        environment: UiScriptEnvironment,
        cvar_values: &[(String, String)],
        addon_catalog: &AddonCatalog,
        sources: FrameUiSources,
    ) -> Result<Self, GlueError> {
        let FrameUiSources {
            catalog,
            bindings,
            declarations,
        } = sources;
        Self::start_with_bundle(
            assets,
            environment,
            cvar_values,
            addon_catalog,
            catalog,
            bindings,
            UiBundle::from_prepared_sources(declarations),
        )
    }

    /// Attaches live bindings and keeps every authored callback inside stock's
    /// sound-admission scope, regardless of where sources were validated.
    fn start_with_bundle(
        assets: AssetStoreHandle,
        environment: UiScriptEnvironment,
        cvar_values: &[(String, String)],
        addon_catalog: &AddonCatalog,
        catalog: UiBindingCatalog,
        bindings: UiBindingAssignments,
        bundle: UiBundle,
    ) -> Result<Self, GlueError> {
        // 52A980 brackets the complete FrameXML execution with 4CFB80/4CFB90.
        let _sound_admission = crate::script::UiSoundSuppression::new(environment.media_intent());
        let environment = environment
            .with_shared_asset_store(assets.clone())
            .with_cvar_values(cvar_values)
            .with_addon_load_state(crate::UiAddonLoadState::from_catalog(addon_catalog))
            .with_binding_assignments(bindings);
        let binding_assignments =
            environment
                .binding_assignments()
                .ok_or_else(|| crate::UiScriptError::Execution {
                    label: "FrameXML bindings".to_owned(),
                    message: "missing attached binding assignments".to_owned(),
                })?;
        let movement_input = environment.movement_input();
        let tutorials = environment.world_state().tutorials();
        let world = environment.world_state();
        let combat_log = environment.combat_log_state();
        let media_intent = environment.media_intent();
        let owner = GlueManager::start_shared_frame(assets, environment, bundle)?;
        let mut binding_functions = HashMap::new();
        for binding in catalog.bindings() {
            let function = owner
                .bundle()
                .lua()
                .load(format!(
                    "return function(keystate, pressure, angle, precision)\n{}\nend",
                    binding.body()
                ))
                .set_name(binding.name())
                .eval::<mlua::Function>()
                .map_err(|error| crate::UiScriptError::Execution {
                    label: binding.name().to_owned(),
                    message: error.to_string(),
                })?;
            binding_functions.insert(binding.name().to_owned(), function);
        }
        Ok(Self {
            owner,
            binding_catalog: catalog,
            binding_assignments,
            binding_functions,
            movement_input,
            tutorials,
            world,
            combat_log,
            media_intent,
        })
    }
}
