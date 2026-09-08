//! Session-owned source-name cache and pending resurrection callbacks.

use solarity_network::{WorldPlayerName, WorldPlayerNameResponse, WorldPlayerNameResult};
use std::collections::{BTreeMap, VecDeque};

#[derive(Default)]
struct Entry {
    value: Option<WorldPlayerName>,
    callbacks: usize,
}

#[derive(Default)]
pub(super) struct RuntimePlayerNameCache {
    entries: BTreeMap<u64, Entry>,
    requests: VecDeque<u64>,
}

impl RuntimePlayerNameCache {
    pub(super) fn lookup(&self, guid: u64) -> Option<&str> {
        (guid != 0)
            .then(|| {
                self.entries
                    .get(&guid)?
                    .value
                    .as_ref()
                    .map(|v| v.name.as_str())
            })
            .flatten()
    }

    pub(super) fn request_for_offer(&mut self, guid: u64) -> Option<String> {
        if guid == 0 {
            return None;
        }
        if let Some(name) = self.lookup(guid) {
            return Some(name.to_owned());
        }
        let entry = self.entries.entry(guid).or_insert_with(|| {
            self.requests.push_back(guid);
            Entry::default()
        });
        // 67D770 is called with its deduplication byte zero for each offer.
        entry.callbacks += 1;
        None
    }

    pub(super) fn receive(&mut self, response: WorldPlayerNameResponse) -> usize {
        match response.result {
            WorldPlayerNameResult::Retry => {
                if self.entries.contains_key(&response.guid)
                    && !self.requests.contains(&response.guid)
                {
                    self.requests.push_back(response.guid);
                }
                0
            }
            WorldPlayerNameResult::Missing => {
                self.requests.retain(|guid| *guid != response.guid);
                self.entries
                    .remove(&response.guid)
                    .map_or(0, |entry| entry.callbacks)
            }
            WorldPlayerNameResult::Found(value) => {
                self.requests.retain(|guid| *guid != response.guid);
                let entry = self.entries.entry(response.guid).or_default();
                entry.value = Some(value);
                std::mem::take(&mut entry.callbacks)
            }
        }
    }

    pub(super) fn pending_request(&self) -> Option<u64> {
        self.requests.front().copied()
    }
    pub(super) fn request_admitted(&mut self) {
        self.requests.pop_front();
    }
}
