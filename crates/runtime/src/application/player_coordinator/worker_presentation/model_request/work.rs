//! Shared primary sources gate appearance work without occupying a waiting worker.

use super::super::super::{
    RuntimePlayerError, RuntimePlayerPresentation, RuntimePlayerSharedCatalogs,
};
use super::super::{AppearanceCompletion, AppearanceWorkerCache, with_worker_presentation};
use solarity_asset::{ArchiveCatalog, DecodedM2Model, M2LoadProducer, ResourceLease};
use solarity_rendering::CharacterComponentTextureLevel;
use std::ops::ControlFlow;

type SourceProgress =
    Result<ControlFlow<(), Option<solarity_cpu::CpuTaskDependency>>, RuntimePlayerError>;

/// Main selects immutable appearance inputs; the worker owns the preparation call.
pub(super) type Prepare<T> = Box<
    dyn FnOnce(
            &mut RuntimePlayerPresentation,
            ResourceLease<DecodedM2Model>,
        ) -> Result<T, RuntimePlayerError>
        + Send,
>;

/// One exclusive archive/derived-cache bank returns independently of source success.
pub(super) struct AppearanceBank {
    pub(super) catalog: ArchiveCatalog,
    pub(super) catalogs: RuntimePlayerSharedCatalogs,
    pub(super) level: CharacterComponentTextureLevel,
    pub(super) cache: AppearanceWorkerCache,
}

/// A reserved producer owns decoding; a ready lease avoids any second model lookup.
pub(super) enum ModelInput {
    Ready(ResourceLease<DecodedM2Model>),
    Producer(M2LoadProducer),
    Pending(solarity_asset::M2LoadDependency),
}

impl AppearanceBank {
    /// Source turns retain owned leases and suspend only on another producer.
    /// Derived construction executes once after every selected source is ready.
    pub(super) fn operation<T: Send + 'static>(
        self,
        model: ModelInput,
        sources: super::AppearanceSources,
        prepare: Prepare<T>,
        budget: solarity_cpu::CpuStorageBudget,
        service: solarity_cpu::CpuServiceControl,
    ) -> impl FnMut(
        &solarity_cpu::JobContext<'_>,
    ) -> solarity_cpu::CpuTaskStep<AppearanceCompletion<T>>
    + Send {
        use solarity_cpu::{CpuError, CpuTaskStep};
        let mut bank = Some(self);
        let mut input = Some(model);
        let mut primary = None;
        let mut plan = Some(sources);
        let mut sources = None;
        let mut prepare = Some(prepare);
        move |context| {
            context.diagnostic_value("appearance.sources.turn", 1);
            let current = bank
                .as_mut()
                .unwrap_or_else(|| unreachable!("appearance owns its bank"));
            // A producer was claimed before dispatch and must publish to joiners,
            // even if its original appearance has since been withdrawn.
            if context.is_cancelled() && !matches!(input, Some(ModelInput::Producer(_))) {
                return CpuTaskStep::Complete(
                    bank.take()
                        .unwrap_or_else(|| {
                            unreachable!("admitted appearance retains its phase inputs")
                        })
                        .failed(CpuError::JobCancelled.into()),
                );
            }
            let step = (|| -> SourceProgress {
                if let Some(ModelInput::Pending(dependency)) = input.as_ref() {
                    if let Some(model) = dependency.poll() {
                        primary = Some(model?);
                        input = None;
                        return Ok(ControlFlow::Continue(None));
                    }
                    return Ok(ControlFlow::Continue(Some(dependency.task_dependency()?)));
                }
                if let Err(error) = current.cache.mount(&current.catalog) {
                    if let Some(ModelInput::Producer(producer)) = input.take() {
                        let error = solarity_asset::M2LoadError::Asset(std::sync::Arc::new(error));
                        producer.fail(error.clone());
                        return Err(error.into());
                    }
                    return Err(error.into());
                }
                if let Some(model) = input.take() {
                    primary = Some(match model {
                        ModelInput::Ready(model) => model,
                        ModelInput::Producer(producer) => producer.load_admitted(
                            current.cache.store.as_mut().unwrap_or_else(|| {
                                unreachable!("admitted appearance retains its phase inputs")
                            }),
                            &solarity_asset::AssetReadBudget::for_service(
                                budget.clone(),
                                service.service(),
                            ),
                        )?,
                        ModelInput::Pending(_) => {
                            unreachable!("pending primary was serviced before archive work")
                        }
                    });
                    return Ok(ControlFlow::Continue(None));
                }
                if let Some(plan) = plan.take() {
                    sources = Some(plan.resolve(
                        &current.catalogs,
                        primary.as_ref().unwrap_or_else(|| {
                            unreachable!("admitted appearance retains its phase inputs")
                        }),
                        &budget,
                    )?);
                }
                sources
                    .as_mut()
                    .unwrap_or_else(|| unreachable!("admitted appearance retains its phase inputs"))
                    .step(
                        current.cache.store.as_mut().unwrap_or_else(|| {
                            unreachable!("admitted appearance retains its phase inputs")
                        }),
                        &budget,
                        &service,
                    )
            })();
            match step {
                Ok(ControlFlow::Continue(edge)) => {
                    edge.map_or(CpuTaskStep::Continue, CpuTaskStep::Wait)
                }
                Ok(ControlFlow::Break(())) => {
                    let completion = bank
                        .take()
                        .unwrap_or_else(|| {
                            unreachable!("admitted appearance retains its phase inputs")
                        })
                        .prepare(
                            primary.take().unwrap_or_else(|| {
                                unreachable!("admitted appearance retains its primary source")
                            }),
                            prepare.take().unwrap_or_else(|| {
                                unreachable!("admitted appearance retains its phase inputs")
                            }),
                            &solarity_asset::AssetReadBudget::for_service(
                                budget.clone(),
                                service.service(),
                            ),
                        );
                    // The final consumer has acquired its own leases before these pins leave.
                    sources = None;
                    CpuTaskStep::Complete(completion)
                }
                Err(error) => CpuTaskStep::Complete(
                    bank.take()
                        .unwrap_or_else(|| {
                            unreachable!("admitted appearance retains its phase inputs")
                        })
                        .failed(error),
                ),
            }
        }
    }

    /// Constructs derived products once, with every source still pinned by its phase.
    fn prepare<T>(
        mut self,
        model: ResourceLease<DecodedM2Model>,
        prepare: Prepare<T>,
        budget: &solarity_asset::AssetReadBudget,
    ) -> AppearanceCompletion<T> {
        let result = with_worker_presentation(
            self.catalog,
            self.catalogs,
            self.level,
            &mut self.cache,
            |presentation| {
                let assets = presentation.assets.clone();
                assets.with_read_budget(budget, || prepare(presentation, model))
            },
        );
        AppearanceCompletion {
            cache: self.cache,
            result,
        }
    }

    /// A cancelled or failed prerequisite leaves all worker-local caches owned.
    pub(super) fn failed<T>(self, error: RuntimePlayerError) -> AppearanceCompletion<T> {
        AppearanceCompletion {
            cache: self.cache,
            result: Err(error),
        }
    }
}
