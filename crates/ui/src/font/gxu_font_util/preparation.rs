//! Reusable typed native UI loans are checked out only after CPU task admission.

use super::{
    FontSystem, Owner,
    work::{Preparation, Request},
};
use crate::FontError;
use solarity_asset::{AssetError, AssetReadBudget, AssetStorageMap, AssetStore};
use solarity_cpu::{
    ByteReservation, CpuService, CpuStorageBudget, CpuStorageClass, CpuStorageKind,
};
use std::{
    any::{Any, TypeId},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

/// One warm slot per state/output shape, shared by font-system clones.
/// The table and slots own metadata admission; no weak slot references escape.
pub(super) struct PreparationPool {
    slots: AssetStorageMap<TypeId, Arc<dyn Any + Send + Sync>>,
}
impl Default for PreparationPool {
    fn default() -> Self {
        Self {
            slots: AssetStorageMap::metadata(),
        }
    }
}
impl PreparationPool {
    fn slot<T: Default + Send + 'static, R: Send + 'static>(
        &mut self,
        budget: Option<&CpuStorageBudget>,
    ) -> Result<Arc<Slot<T, R>>, FontError> {
        let key = TypeId::of::<(T, R)>();
        if let Some(slot) = self.slots.get(&key) {
            return Ok(Arc::clone(slot)
                .downcast::<Slot<T, R>>()
                .unwrap_or_else(|_| unreachable!("typed preparation slot key")));
        }
        let memory = reserve(budget, size_of::<Slot<T, R>>() + 2 * size_of::<usize>())?;
        let policy = budget
            .cloned()
            .map(|budget| AssetReadBudget::for_service(budget, CpuService::Required));
        self.slots.reserve(policy.as_ref(), self.slots.len() + 1)?;
        let slot = Arc::new(Slot {
            state: Mutex::new((T::default(), None)),
            busy: AtomicBool::new(false),
            _memory: memory,
        });
        self.slots.insert(policy.as_ref(), key, slot.clone())?;
        Ok(slot)
    }
}

struct Slot<T, R> {
    state: Mutex<(T, Option<R>)>,
    busy: AtomicBool,
    _memory: Option<ByteReservation>,
}

fn reserve(
    budget: Option<&CpuStorageBudget>,
    bytes: usize,
) -> Result<Option<ByteReservation>, FontError> {
    budget
        .map(|budget| budget.reserve(CpuStorageClass::Required, CpuStorageKind::Metadata, bytes))
        .transpose()
        .map_err(AssetError::from)
        .map_err(Into::into)
}

impl FontSystem {
    /// The host admits before checkout and joins before the loan restores main ownership.
    /// Partial state is reclaimed on native failure, cancellation, refusal and unwind.
    pub(crate) fn prepare<T, R, F>(
        &self,
        assets: &mut AssetStore,
        state: &mut T,
        operation: F,
    ) -> Result<R, FontError>
    where
        T: Default + Send + 'static,
        R: Send + 'static,
        F: FnOnce(&mut T, &mut FontSystem, &mut AssetStore) -> R + Send + 'static,
    {
        let Owner::Worker(worker) = &self.owner else {
            return Ok(operation(state, &mut self.clone(), assets));
        };
        let mut target = Some(state);
        let mut operation = Some(operation);
        let mut loan = None;
        worker.execute_prepared(assets.namespace(), &mut || {
            let budget = worker.executor.storage();
            let memory = reserve(budget, size_of::<Operation<T, R, F>>())?;
            let slot = worker.preparations.borrow_mut().slot::<T, R>(budget)?;
            let target = target
                .take()
                .unwrap_or_else(|| unreachable!("host prepares at most once"));
            loan = Some(Loan::checkout(slot.clone(), target)?);
            let operation = operation
                .take()
                .unwrap_or_else(|| unreachable!("one prepared operation"));
            Ok(Request::Preparation(Box::new(Operation {
                slot,
                operation,
                _memory: memory,
            })))
        })?;
        let result = loan.as_ref().and_then(|loan| {
            loan.shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .1
                .take()
        });
        result.ok_or_else(|| FontError::Execution {
            message: "UI preparation completed without its typed output".into(),
        })
    }
}

/// Named capture layout lets admission charge the actual boxed operation size.
struct Operation<T, R, F> {
    slot: Arc<Slot<T, R>>,
    operation: F,
    _memory: Option<ByteReservation>,
}
impl<T, R, F> Preparation for Operation<T, R, F>
where
    T: Send,
    R: Send,
    F: FnOnce(&mut T, &mut FontSystem, &mut AssetStore) -> R + Send,
{
    fn run(self: Box<Self>, fonts: &mut FontSystem, assets: &mut AssetStore) {
        let Self {
            slot,
            operation,
            _memory,
        } = *self;
        let mut state = slot
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.1 = Some(operation(&mut state.0, fonts, assets));
    }
}

struct Loan<'a, T, R> {
    shared: Arc<Slot<T, R>>,
    target: &'a mut T,
}
impl<'a, T, R> Loan<'a, T, R> {
    fn checkout(shared: Arc<Slot<T, R>>, target: &'a mut T) -> Result<Self, FontError> {
        if shared
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(FontError::Execution {
                message: "UI preparation state is already borrowed".into(),
            });
        }
        {
            let mut state = shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            std::mem::swap(target, &mut state.0);
        }
        Ok(Self { shared, target })
    }
}
impl<T, R> Drop for Loan<'_, T, R> {
    fn drop(&mut self) {
        let output = {
            let mut shared = self
                .shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            std::mem::swap(self.target, &mut shared.0);
            shared.1.take()
        };
        // A host failure can leave a completed result; dispose it outside the loan lock.
        self.shared.busy.store(false, Ordering::Release);
        drop(output);
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/preparation_storage.rs"]
mod tests;
