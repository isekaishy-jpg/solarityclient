//! Typed access to client database tables and data stores.
//!
//! This boundary follows the stock `DBClient.cpp`, `DBCache.cpp`, and
//! `WDataStore.cpp` family. Table schemas and validation belong here; game
//! behavior that consumes a table belongs to its owning domain crate.

mod c_data_store;
mod db_cache;
mod db_cache_instances;
mod db_client;
mod w_data_store;
mod wow_client_db;

pub use wow_client_db::{WdbcHeader, WdbcTable};
