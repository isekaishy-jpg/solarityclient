//! Admission, completion and shutdown never lend the request index to a worker.

use std::{hash::Hash, sync::Arc};

use solarity_cpu::{CpuError, CpuExecutor};

use super::{ResourceRequests, SharedResult, State};

impl<K: Eq + Hash + Clone, C, T: Send + Sync + 'static, E: From<CpuError> + Send + 'static>
    ResourceRequests<K, C, T, E>
{
    /// Reserves CPU admission before invoking a caller-owned producer factory.
    /// The factory may borrow its domain owner and transfer inputs only when
    /// called; refusal or an existing producer leaves those inputs untouched.
    pub(in crate::application) fn start<F>(
        &mut self,
        key: &K,
        cpu: &CpuExecutor,
        make_producer: impl FnOnce() -> F,
    ) -> Result<bool, CpuError>
    where
        F: FnOnce() -> Result<Arc<T>, E> + Send + 'static,
    {
        let Some(entry) = self.entries.get_mut(key) else {
            return Ok(false);
        };
        if !matches!(entry.state, State::Requested) {
            return Ok(false);
        }
        let permit = cpu.try_reserve_for(entry.service())?;
        let _trace = entry.trace.enter();
        let task = permit.submit(make_producer());
        entry.state = State::Running(task);
        self.active.push(key.clone());
        Ok(true)
    }

    /// Polls admitted tasks only. Abandoned completions return to the domain's
    /// retirement owner; failures remain shared only for currently registered consumers.
    pub(in crate::application) fn poll(&mut self) -> Vec<SharedResult<T, E>> {
        let mut retired = Vec::new();
        let mut index = 0;
        while index < self.active.len() {
            let key = &self.active[index];
            let entry = self
                .entries
                .get_mut(key)
                .unwrap_or_else(|| unreachable!("every active producer has an index entry"));
            let State::Running(task) = &entry.state else {
                unreachable!("only producers are active");
            };
            if !task.is_finished() {
                index += 1;
                continue;
            }
            let State::Running(task) = std::mem::replace(&mut entry.state, State::Requested) else {
                unreachable!("the observed producer cannot change during polling");
            };
            let result = task
                .join()
                .map_err(E::from)
                .and_then(|result| result)
                .map_err(Arc::new);
            if entry.consumers.is_empty() {
                retired.push(result);
                self.entries.remove(key);
            } else {
                entry.state = State::Ready(result);
            }
            self.active.swap_remove(index);
        }
        retired
    }

    /// Observes all remaining producer outcomes at explicit domain shutdown.
    /// No discarded handle can let archive inputs outlive this ownership boundary.
    pub(in crate::application) fn drain(&mut self) -> Vec<SharedResult<T, E>> {
        self.active.clear();
        self.entries
            .drain()
            .filter_map(|(_, entry)| match entry.state {
                State::Requested => None,
                State::Ready(result) => Some(result),
                State::Running(task) => Some(
                    task.join()
                        .map_err(E::from)
                        .and_then(|result| result)
                        .map_err(Arc::new),
                ),
            })
            .collect()
    }
}
