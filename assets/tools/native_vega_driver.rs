//! Accepted finite native-proof driver. Vega inference runs in the candidate's
//! Rust Candle/Metal worker. This helper cannot decide readiness or prove parity.
#[allow(dead_code)]
#[path = "integration_custody.rs"]
mod hashing;
#[allow(dead_code)]
#[path = "native_vega_model.rs"]
mod model;

include!("native_vega_driver_core.rs");
