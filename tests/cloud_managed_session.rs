//! Standalone entry for the managed-session scenarios also required by the
//! accepted consumer boundary target.
#![cfg(feature = "supabase-cloud")]

#[path = "common/cloud_managed.rs"]
mod managed;
#[path = "common/cloud_managed_scenarios.rs"]
mod scenarios;
