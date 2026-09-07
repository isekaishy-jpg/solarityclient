//! Session-wide template queries and lifetime-owned cache callbacks.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::{Rc, Weak};

use solarity_network::{GameObjectQueryResponse, GameObjectTemplate};

/// One object's callback registration. Dropping it cancels delivery, as in
/// native `0x00712B80`; the outstanding network query remains session-owned.
pub(in crate::application) struct GameObjectTemplateBinding {
    result: Rc<RefCell<Option<Rc<GameObjectTemplate>>>>,
}

impl GameObjectTemplateBinding {
    /// Shares a completed template on demand, without cloning its property data.
    pub(in crate::application) fn template(&self) -> Option<Rc<GameObjectTemplate>> {
        self.result.borrow().as_ref().map(Rc::clone)
    }
}

/// Weak delivery targets cannot retain removed GameObjects or cross GUID reuse.
type TemplateCallback = Weak<RefCell<Option<Rc<GameObjectTemplate>>>>;

/// Native `0x0067BD40` coalesces requests by entry, while `0x0067BEE0`
/// removes missing entries instead of keeping a negative cache or retrying.
enum CachedTemplate {
    Pending(Vec<TemplateCallback>),
    Ready(Rc<GameObjectTemplate>),
}

/// Cache ownership outlives map replacement and ends with the world connection.
pub(in crate::application) struct GameObjectTemplateCache {
    entries: BTreeMap<u32, CachedTemplate>,
    /// Requests wait here only until the bounded encrypted writer admits them.
    requests: VecDeque<(u32, u64)>,
}

impl GameObjectTemplateCache {
    pub(in crate::application) const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            requests: VecDeque::new(),
        }
    }

    /// Registers exactly once for a resident object's template admission.
    pub(in crate::application) fn bind(
        &mut self,
        entry: u32,
        guid: u64,
    ) -> GameObjectTemplateBinding {
        let result = Rc::new(RefCell::new(None));
        if entry != 0 {
            match self.entries.entry(entry).or_insert_with(|| {
                self.requests.push_back((entry, guid));
                CachedTemplate::Pending(Vec::new())
            }) {
                CachedTemplate::Pending(callbacks) => {
                    callbacks.retain(|callback| callback.strong_count() != 0);
                    callbacks.push(Rc::downgrade(&result));
                }
                CachedTemplate::Ready(template) => *result.borrow_mut() = Some(Rc::clone(template)),
            }
        }
        GameObjectTemplateBinding { result }
    }

    /// Delivers only to surviving registrations. Missing replies complete those
    /// registrations empty; a later new admission may issue a new native query.
    pub(in crate::application) fn receive(&mut self, response: GameObjectQueryResponse) {
        let entry = response.entry();
        let previous = self.entries.remove(&entry);
        let template = match response {
            GameObjectQueryResponse::Found(template) => Some(Rc::from(template)),
            GameObjectQueryResponse::Missing(_) => None,
        };
        if let Some(CachedTemplate::Pending(callbacks)) = previous {
            for callback in callbacks {
                if let Some(target) = callback.upgrade() {
                    *target.borrow_mut() = template.as_ref().map(Rc::clone);
                }
            }
        }
        if let Some(template) = template {
            self.entries.insert(entry, CachedTemplate::Ready(template));
        }
    }

    pub(in crate::application) fn pending_request(&self) -> Option<(u32, u64)> {
        self.requests.front().copied()
    }

    /// Retires only the request successfully admitted to the encrypted writer.
    pub(in crate::application) fn request_admitted(&mut self) {
        self.requests.pop_front();
    }

    pub(in crate::application) fn clear(&mut self) {
        self.entries.clear();
        self.requests.clear();
    }
}
