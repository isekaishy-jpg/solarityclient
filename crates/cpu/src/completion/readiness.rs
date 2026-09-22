//! Bounded readiness subscriptions; payload ownership stays with the domain.

use crate::storage::StorageVec;
use crate::{
    CpuError, CpuStorageBudget, CpuStorageClass, CpuStorageKind, CpuStorageReservation,
    CpuStorageWorkingSet, JobOutcome,
};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

/// Internal metadata delivery only. Implementations enqueue work and never run
/// a domain kernel inline or publish another readiness port recursively.
pub(crate) trait ReadySink: Send + Sync {
    fn signal(self: Arc<Self>, epoch: u64, input: usize, outcome: JobOutcome);
}

/// Metadata-only promotion of a checked producer epoch; propagation is queued.
pub(crate) trait PrioritySink: Send + Sync {
    fn require_urgent(self: Arc<Self>, epoch: u64);
}

/// A port never owns the producer or extends its epoch lifetime by itself.
struct PriorityOwner {
    sink: Weak<dyn PrioritySink>,
    epoch: u64,
}

/// A reserved consumer identifies one epoch of an existing scheduler owner.
struct Target {
    sink: Weak<dyn ReadySink>,
    epoch: u64,
    input: usize,
}
/// Slot identity also changes on cancellation/reuse inside one port generation.
struct Slot {
    serial: u64,
    reserved: bool,
    target: Option<Target>,
}
/// Outcome publication and subscriber registration share this one lock.
struct State {
    generation: u64,
    outcome: Option<JobOutcome>,
    slots: StorageVec<Slot>,
    bound: StorageVec<usize>,
    subscribers: usize,
    publishing: bool,
    producer_issued: bool,
    urgent: bool,
    priority_owner: Option<PriorityOwner>,
}
/// Domain callbacks are never invoked while holding readiness metadata.
struct Core {
    state: Mutex<State>,
}
impl Core {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|_| unreachable!("readiness metadata cannot panic"))
    }
    /// Publishes once, then delivers outside the lock. Reset cannot recycle slots
    /// while delivery is active. Targets only enqueue scheduler metadata.
    fn complete(&self, generation: u64, outcome: JobOutcome) -> Result<bool, CpuError> {
        let mut state = self.lock();
        if state.generation != generation {
            return Err(CpuError::StaleReadiness);
        }
        if let Some(previous) = state.outcome {
            return if previous == outcome {
                Ok(false)
            } else {
                Err(CpuError::ReadinessConflict)
            };
        }
        state.outcome = Some(outcome);
        state.publishing = true;
        let mut bound = std::mem::take(&mut state.bound);
        drop(state);
        for &index in bound.iter() {
            let target = self.lock().slots[index].target.take();
            if let Some(target) = target
                && let Some(sink) = target.sink.upgrade()
            {
                sink.signal(target.epoch, target.input, outcome);
            }
        }
        bound.clear();
        let mut state = self.lock();
        state.bound = bound;
        state.publishing = false;
        Ok(true)
    }
}

/// Reusable readiness owner for an external resource or a CPU batch. It never
/// carries the only copy of a decoded asset, GPU handle or gameplay result.
pub struct CompletionPort {
    core: Arc<Core>,
}
impl CompletionPort {
    /// Creates an active generation with an explicit maximum subscriber count.
    /// # Errors
    /// Reports metadata allocation failure before opening the generation.
    pub fn new(
        subscribers: usize,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
    ) -> Result<Self, CpuError> {
        let port = Self::empty();
        port.begin(subscribers, budget, class)?;
        Ok(port)
    }
    /// Batch registration allocates its port once, before any scene activation.
    pub(crate) fn empty() -> Self {
        Self {
            core: Arc::new(Core {
                state: Mutex::new(State {
                    generation: 0,
                    outcome: None,
                    slots: StorageVec::default(),
                    bound: StorageVec::default(),
                    subscribers: 0,
                    publishing: false,
                    producer_issued: false,
                    urgent: false,
                    priority_owner: None,
                }),
            }),
        }
    }
    /// Rebinds only after terminal publication and all subscriptions have left.
    /// # Errors
    /// Reports active subscriptions, generation exhaustion or storage failure.
    pub fn restart(
        &mut self,
        subscribers: usize,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
    ) -> Result<ReadyToken, CpuError> {
        self.begin(subscribers, budget, class)
    }
    /// Internal owners serialize activation with their own epoch admission.
    pub(crate) fn begin(
        &self,
        subscribers: usize,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
    ) -> Result<ReadyToken, CpuError> {
        let mut working_set = CpuStorageWorkingSet::default();
        self.include_storage(subscribers, budget, class, &mut working_set)?;
        let mut reservation = budget.reserve_working_set(class, working_set.bytes())?;
        self.begin_reserved(subscribers, &mut reservation)
    }

    /// Batch admission includes readiness records before any scheduler buffer grows.
    pub(crate) fn include_storage(
        &self,
        subscribers: usize,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
        working_set: &mut CpuStorageWorkingSet,
    ) -> Result<(), CpuError> {
        let state = self.core.lock();
        if state.generation != 0
            && (state.outcome.is_none() || state.publishing || state.subscribers != 0)
        {
            return Err(CpuError::ReadinessActive);
        }
        state
            .generation
            .checked_add(1)
            .ok_or(CpuError::EpochExhausted)?;
        working_set.include(
            state.slots.reservation_bytes(budget, class, subscribers)?,
            state.slots.replacement_credit(subscribers),
        )?;
        working_set.include(
            state.bound.reservation_bytes(budget, class, subscribers)?,
            state.bound.replacement_credit(subscribers),
        )
    }

    /// Activates a serialized owner using its protected connected reservation.
    pub(crate) fn begin_reserved(
        &self,
        subscribers: usize,
        reservation: &mut CpuStorageReservation,
    ) -> Result<ReadyToken, CpuError> {
        let mut state = self.core.lock();
        if state.generation != 0
            && (state.outcome.is_none() || state.publishing || state.subscribers != 0)
        {
            return Err(CpuError::ReadinessActive);
        }
        let generation = state
            .generation
            .checked_add(1)
            .ok_or(CpuError::EpochExhausted)?;
        state
            .slots
            .reserve_reserved(reservation, CpuStorageKind::Metadata, subscribers)?;
        state
            .bound
            .reserve_reserved(reservation, CpuStorageKind::Metadata, subscribers)?;
        state.slots.resize_with(subscribers, || Slot {
            serial: 0,
            reserved: false,
            target: None,
        });
        state.generation = generation;
        state.outcome = None;
        state.producer_issued = false;
        state.urgent = false;
        state.priority_owner = None;
        Ok(ReadyToken {
            core: Arc::downgrade(&self.core),
            generation,
        })
    }
    /// Borrows the current generation's readiness identity.
    #[must_use]
    pub fn readiness(&self) -> ReadyToken {
        ReadyToken {
            core: Arc::downgrade(&self.core),
            generation: self.core.lock().generation,
        }
    }
    /// Issues the single owned external producer. Abandoning it cancels this
    /// resource generation, while cancelling a subscriber affects only its edge.
    /// # Errors
    /// A second producer or a terminal generation is rejected.
    pub fn producer(&self) -> Result<CompletionProducer, CpuError> {
        let mut state = self.core.lock();
        if state.producer_issued || state.outcome.is_some() {
            return Err(CpuError::ReadinessActive);
        }
        state.producer_issued = true;
        Ok(CompletionProducer {
            token: ReadyToken {
                core: Arc::downgrade(&self.core),
                generation: state.generation,
            },
            finished: false,
        })
    }
    /// CPU batch owners publish using the token captured at admission.
    pub(crate) fn complete(&self, token: &ReadyToken, outcome: JobOutcome) {
        let _published = self.core.complete(token.generation, outcome);
    }

    /// Installs a weak producer link before its readiness token becomes public.
    pub(crate) fn priority_owner(&self, sink: Weak<dyn PrioritySink>, epoch: u64) {
        self.core.lock().priority_owner = Some(PriorityOwner { sink, epoch });
    }
}
impl Drop for CompletionPort {
    fn drop(&mut self) {
        let generation = self.core.lock().generation;
        let _cancelled = self.core.complete(generation, JobOutcome::Cancelled);
    }
}

/// A generation identity that cannot keep a removed resource producer alive.
#[derive(Clone)]
pub struct ReadyToken {
    core: Weak<Core>,
    generation: u64,
}
impl ReadyToken {
    /// Latches consumer urgency and notifies the producer outside the port lock.
    /// Completion/reuse races need no promotion and cannot affect a new epoch.
    pub(crate) fn require_urgent(&self) {
        let Some(core) = self.core.upgrade() else {
            return;
        };
        let mut state = core.lock();
        if state.generation != self.generation || state.outcome.is_some() || state.urgent {
            return;
        }
        state.urgent = true;
        let owner = state
            .priority_owner
            .as_ref()
            .and_then(|owner| owner.sink.upgrade().map(|sink| (sink, owner.epoch)));
        drop(state);
        if let Some((sink, epoch)) = owner {
            sink.require_urgent(epoch);
        }
    }
    /// Identity comparison does not acquire resource metadata or retain its payload.
    pub(crate) fn same_generation(&self, other: &Self) -> bool {
        self.generation == other.generation && self.core.ptr_eq(&other.core)
    }
    /// Observes durable readiness; it does not consume a resource payload.
    /// # Errors
    /// Retired owners and old generations are rejected.
    pub fn outcome(&self) -> Result<Option<JobOutcome>, CpuError> {
        let core = self.core.upgrade().ok_or(CpuError::StaleReadiness)?;
        let state = core.lock();
        if state.generation != self.generation {
            return Err(CpuError::StaleReadiness);
        }
        Ok(state.outcome)
    }
    /// Reserves subscriber capacity before a batch transfers any job state.
    pub(crate) fn reserve(&self) -> Result<Subscription, CpuError> {
        let core = self.core.upgrade().ok_or(CpuError::StaleReadiness)?;
        let mut state = core.lock();
        if state.generation != self.generation {
            return Err(CpuError::StaleReadiness);
        }
        let index = state
            .slots
            .iter()
            .position(|slot| !slot.reserved)
            .ok_or(CpuError::ReadinessCapacity)?;
        let slot = &mut state.slots[index];
        let serial = slot.serial.checked_add(1).ok_or(CpuError::EpochExhausted)?;
        slot.serial = serial;
        slot.reserved = true;
        state.subscribers += 1;
        drop(state);
        Ok(Subscription {
            core,
            generation: self.generation,
            index,
            serial,
        })
    }
}

/// Single external completion owner; its generation cannot affect a recycled port.
pub struct CompletionProducer {
    token: ReadyToken,
    finished: bool,
}
impl CompletionProducer {
    /// Reports latched consumer urgency without changing the producer's execution
    /// class. External services retain their own admission and scheduling policy.
    /// # Errors
    /// A retired or recycled generation cannot report another producer's demand.
    pub fn is_urgent(&self) -> Result<bool, CpuError> {
        let core = self.token.core.upgrade().ok_or(CpuError::StaleReadiness)?;
        let state = core.lock();
        if state.generation != self.token.generation {
            return Err(CpuError::StaleReadiness);
        }
        Ok(state.urgent)
    }
    /// Publishes once. Repeating the same outcome is idempotent; conflicting or
    /// stale completion is an explicit error and cannot complete a new occupant.
    /// # Errors
    /// Reports a retired generation or a conflicting duplicate outcome.
    pub fn complete(&mut self, outcome: JobOutcome) -> Result<bool, CpuError> {
        self.finished = true;
        self.token
            .core
            .upgrade()
            .ok_or(CpuError::StaleReadiness)?
            .complete(self.token.generation, outcome)
    }
}
impl Drop for CompletionProducer {
    fn drop(&mut self) {
        if !self.finished
            && let Some(core) = self.token.core.upgrade()
        {
            let _cancelled = core.complete(self.token.generation, JobOutcome::Cancelled);
        }
    }
}

/// One reserved subscription. Core-to-port is the only nested lock order;
/// publication releases the port lock before delivering to a batch.
pub(crate) struct Subscription {
    core: Arc<Core>,
    generation: u64,
    index: usize,
    serial: u64,
}
impl Subscription {
    /// The binding command pins metadata without owning/unsubscribing its slot.
    pub(crate) fn binder(&self) -> Binding {
        Binding {
            core: Arc::clone(&self.core),
            generation: self.generation,
            index: self.index,
            serial: self.serial,
        }
    }
}

/// Owned metadata needed to bind after releasing the consumer's state lock.
pub(crate) struct Binding {
    core: Arc<Core>,
    generation: u64,
    index: usize,
    serial: u64,
}
impl Binding {
    /// Binds after input admission. Publication that raced reservation is delivered
    /// immediately outside the lock, without losing or double-counting readiness.
    pub(crate) fn bind(self, sink: Weak<dyn ReadySink>, epoch: u64, input: usize) {
        let mut state = self.core.lock();
        if state.generation != self.generation {
            return;
        }
        let outcome = state.outcome;
        let slot = &mut state.slots[self.index];
        if !slot.reserved || slot.serial != self.serial {
            return;
        }
        if let Some(outcome) = outcome {
            drop(state);
            if let Some(sink) = sink.upgrade() {
                sink.signal(epoch, input, outcome);
            }
        } else {
            slot.target = Some(Target { sink, epoch, input });
            state.bound.push(self.index);
        }
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        let mut state = self.core.lock();
        if state.generation == self.generation
            && let Some(slot) = state.slots.get_mut(self.index)
            && slot.serial == self.serial
            && slot.reserved
        {
            slot.reserved = false;
            slot.target = None;
            state.subscribers -= 1;
            state.bound.retain(|index| *index != self.index);
        }
    }
}
