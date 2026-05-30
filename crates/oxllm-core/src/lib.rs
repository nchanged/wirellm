pub mod config;
pub mod error;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
