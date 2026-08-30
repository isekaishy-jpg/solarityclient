//! Item identity, ownership, inventory placement, and client-visible stats.
//!
//! This component boundary follows `Item_C.cpp`, `ItemStats.cpp`, and
//! `ItemName.cpp`. UI presentation and protocol decoding remain consumers of
//! the canonical item state stored here.

mod bag_c;
mod item_c;
mod item_name;
mod item_stats;
