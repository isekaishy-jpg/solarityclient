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
}
