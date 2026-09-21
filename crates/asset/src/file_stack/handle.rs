//! Cloneable main-thread ownership for the single mounted archive stack.

use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

use super::AssetStore;

/// Shared access to one mounted file stack without remounting its MPQs.
///
/// Asset decoding and Glue Lua both run on the client main thread. This handle
/// deliberately uses main-thread reference counting; archive reads are not
/// silently moved onto worker threads whose ordering could differ from stock.
#[derive(Clone)]
pub struct AssetStoreHandle {
    store: Rc<RefCell<AssetStore>>,
}

impl AssetStoreHandle {
    /// Takes ownership of the one mounted process-wide asset store.
    #[must_use]
    pub fn new(store: AssetStore) -> Self {
        Self {
            store: Rc::new(RefCell::new(store)),
        }
    }

    /// Borrows the mounted store for an immutable main-thread operation.
    #[must_use]
    pub fn borrow(&self) -> Ref<'_, AssetStore> {
        self.store.borrow()
    }

    /// Borrows the mounted store for ordered reads and cache mutation.
    #[must_use]
    pub fn borrow_mut(&self) -> RefMut<'_, AssetStore> {
        self.store.borrow_mut()
    }

    /// Applies source admission while a consumer uses cloned handles for its reads.
    /// The policy guard holds no reader borrow across the operation. Nested calls
    /// restore the previous policy on completion or unwind; returned bytes retain
    /// their own reservations independently of this handle.
    /// # Panics
    /// Panics if a store borrow is held at entry or escapes the operation's scope.
    pub fn with_read_budget<T>(
        &self,
        budget: &crate::AssetReadBudget,
        operation: impl FnOnce() -> T,
    ) -> T {
        let previous = self.store.borrow_mut().read_budget.replace(budget.clone());
        let _scope = ReadScope {
            handle: self,
            previous,
        };
        operation()
    }

    /// Recovers the mounted store when this is its sole remaining handle.
    ///
    /// The original handle is returned unchanged when another main-thread
    /// owner still exists, so a caller never loses access on failure.
    pub fn try_into_store(self) -> Result<AssetStore, Self> {
        Rc::try_unwrap(self.store)
            .map(RefCell::into_inner)
            .map_err(|store| Self { store })
    }
}

/// Restores policy after every operation-local RefCell borrow has unwound.
struct ReadScope<'a> {
    handle: &'a AssetStoreHandle,
    previous: Option<crate::AssetReadBudget>,
}
impl Drop for ReadScope<'_> {
    fn drop(&mut self) {
        self.handle.store.borrow_mut().read_budget = self.previous.take();
    }
}
