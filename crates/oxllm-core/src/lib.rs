pub mod config;
pub mod error;
pub mod router;
pub mod state;
pub mod telemetry;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
