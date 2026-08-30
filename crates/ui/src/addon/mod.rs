//! AddOn discovery, metadata, dependency order, saved state, and enablement.
//!
//! The stock `AddOns.cpp` boundary makes AddOn loading distinct from Lua
//! execution and XML construction. Archive/file-stack resolution is requested
//! through the asset facade so load order stays consistent with stock.

mod add_ons;
