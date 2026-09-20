//! Unique producer obligation publishes payload before dependent readiness.

use super::{State, Value};
use crate::{CompletionProducer, JobOutcome};
use std::sync::Arc;

/// Dropping the unique publisher makes abandonment terminal and releases dependents.
/// It does not destroy values still pinned by a consumer or transfer input ownership.
pub struct ProductPublisher<T, E> {
    state: Arc<State<T, E>>,
    producer: Option<CompletionProducer>,
}

impl<T, E> ProductPublisher<T, E> {
    /// Called only after both cell and readiness metadata have been admitted.
    pub(super) fn new(state: Arc<State<T, E>>, producer: CompletionProducer) -> Self {
        Self {
            state,
            producer: Some(producer),
        }
    }

    /// Consumes the producer, retaining the exact domain result for every reader.
    /// Readiness outcome is derived here; callers cannot signal success for an error.
    pub fn publish(mut self, result: Result<T, E>) {
        self.complete(Value::Published(result));
    }

    /// There is one producer and no reset API, so conflicting publication is an internal defect.
    fn complete(&mut self, value: Value<T, E>) {
        let Some(mut producer) = self.producer.take() else {
            unreachable!("a product publisher completes exactly once");
        };
        let outcome = match &value {
            Value::Published(Ok(_)) => JobOutcome::Succeeded,
            Value::Published(Err(_)) => JobOutcome::Failed,
            Value::Abandoned => JobOutcome::Cancelled,
        };
        if self.state.value.set(value).is_err() {
            unreachable!("the unique product publisher owns the empty result cell");
        }
        producer.complete(outcome).unwrap_or_else(|_| {
            unreachable!("the product owns its original readiness generation through publication")
        });
    }
}

impl<T, E> Drop for ProductPublisher<T, E> {
    fn drop(&mut self) {
        if self.producer.is_some() {
            self.complete(Value::Abandoned);
        }
    }
}
