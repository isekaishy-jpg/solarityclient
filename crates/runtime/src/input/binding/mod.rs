//! Stock key-chord serialization and assignment transition routing.

mod router;
mod stock_key;
mod types;

pub use router::InputBindingRouter;
pub(crate) use stock_key::keyboard_name;
pub use types::{InputBindingInvocation, InputBindingPhase};
