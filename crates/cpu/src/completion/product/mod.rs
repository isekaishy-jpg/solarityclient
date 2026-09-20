//! One immutable product binds durable payload ownership to its readiness generation.

mod publisher;

pub use publisher::ProductPublisher;

use super::{CompletionPort, ReadyToken};
use crate::{ByteReservation, CpuError, CpuStorageBudget, CpuStorageClass, CpuStorageKind};
use std::sync::{Arc, OnceLock};

/// Borrowed terminal state, pinned by the product lease which supplied it.
pub enum ProductOutcome<'a, T, E> {
    /// The producer published a complete immutable value.
    Succeeded(&'a T),
    /// Domain failure is retained independently of scheduler failure propagation.
    Failed(&'a E),
    /// The unique producer left without publishing; dependents are cancelled.
    Abandoned,
}

/// The single publication cell never resets while any consumer can address it.
enum Value<T, E> {
    Published(Result<T, E>),
    Abandoned,
}

/// Readiness and payload share one lifetime; no port reset or separate payload replacement exists.
struct State<T, E> {
    port: CompletionPort,
    value: OnceLock<Value<T, E>>,
    _memory: ByteReservation,
}

/// Shared immutable output with readiness from the same owned generation.
/// Cloning pins existing storage; it neither copies T/E nor allocates a new cell.
/// A readiness token alone does not pin the payload: jobs must retain this lease.
pub struct SharedProduct<T, E> {
    state: Arc<State<T, E>>,
}

impl<T, E> Clone for SharedProduct<T, E> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

impl<T, E> SharedProduct<T, E> {
    /// Admits fixed result storage and bounded dependency slots before input transfer.
    /// Allocations nested inside T/E need their own domain reservations or leases.
    /// The unique publisher must publish a result or abandon it through drop.
    /// # Errors
    /// Reports byte/slot admission failure without creating a producer obligation.
    pub fn new(
        subscribers: usize,
        budget: &CpuStorageBudget,
        class: CpuStorageClass,
    ) -> Result<(ProductPublisher<T, E>, Self), CpuError> {
        let bytes = std::mem::size_of::<State<T, E>>()
            .checked_add(2 * std::mem::size_of::<usize>())
            .ok_or(CpuError::BatchStorage)?;
        let memory = budget.reserve(class, CpuStorageKind::Result, bytes)?;
        let port = CompletionPort::new(subscribers, budget, class)?;
        let producer = port.producer()?;
        let state = Arc::new(State {
            port,
            value: OnceLock::new(),
            _memory: memory,
        });
        Ok((
            ProductPublisher::new(Arc::clone(&state), producer),
            Self { state },
        ))
    }

    /// Scheduler readiness for exactly the immutable value held by this lease.
    #[must_use]
    pub fn readiness(&self) -> ReadyToken {
        self.state.port.readiness()
    }

    /// Borrows a durable result without blocking or acquiring a result mutex.
    /// Publication stores the payload before releasing dependent scheduler work.
    #[must_use]
    pub fn poll(&self) -> Option<ProductOutcome<'_, T, E>> {
        self.state.value.get().map(|value| match value {
            Value::Published(Ok(value)) => ProductOutcome::Succeeded(value),
            Value::Published(Err(error)) => ProductOutcome::Failed(error),
            Value::Abandoned => ProductOutcome::Abandoned,
        })
    }
}
