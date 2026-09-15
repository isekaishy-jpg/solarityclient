//! Subscription changes preserve independent consumer generations and shared demand.

use std::{collections::HashMap, hash::Hash};

use solarity_cpu::CpuService;

use super::{Entry, ResourceRequests, SharedResult, State, demand_index};

impl<K: Eq + Hash, C: Eq + Hash, T, E> ResourceRequests<K, C, T, E> {
    /// Registers or changes one consumer without repeating an existing producer.
    pub(in crate::application) fn request(&mut self, key: K, consumer: C, demand: CpuService) {
        let entry = self.entries.entry(key).or_insert_with(|| Entry {
            consumers: HashMap::new(),
            demand: [0; 3],
            state: State::Requested,
            trace: solarity_profiling::TraceContext::capture().fork("resource.request"),
        });
        let previous = entry.consumers.insert(consumer, demand);
        if let Some(previous) = previous {
            entry.demand[demand_index(previous)] -= 1;
        }
        entry.demand[demand_index(demand)] += 1;
        entry.update_service();
        entry.trace.link("resource.join");
    }

    /// Pins a ready result only for its registered generation. The subscription
    /// remains live until explicit cancellation/release after domain consumption.
    pub(in crate::application) fn ready(
        &self,
        key: &K,
        consumer: &C,
    ) -> Option<SharedResult<T, E>> {
        let entry = self.entries.get(key)?;
        if !entry.consumers.contains_key(consumer) {
            return None;
        }
        let State::Ready(result) = &entry.state else {
            return None;
        };
        entry.trace.link("resource.consume");
        Some(result.clone())
    }

    /// Withdraws only this consumer. A final ready value is returned for explicit
    /// retirement; running work stays owned and may satisfy later reacquisition.
    pub(in crate::application) fn cancel(
        &mut self,
        key: &K,
        consumer: &C,
    ) -> Option<SharedResult<T, E>> {
        let entry = self.entries.get_mut(key)?;
        let previous = entry.consumers.remove(consumer)?;
        entry.trace.link("resource.release");
        entry.demand[demand_index(previous)] -= 1;
        entry.update_service();
        if !entry.consumers.is_empty() || matches!(entry.state, State::Running(_)) {
            return None;
        }
        match self.entries.remove(key)?.state {
            State::Ready(result) => Some(result),
            State::Requested => None,
            State::Running(_) => unreachable!("running entries retain producer ownership"),
        }
    }
}
