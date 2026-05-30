use thiserror::Error;

#[derive(Debug, Error)]
pub enum OxllmError {
    #[error("Configuration loading failed: {0}")]
    ConfigLoad(String),

    #[error("TOML parsing failed: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("Required environment variable is missing: {0}")]
    EnvVarMissing(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Upstream network request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("No available or healthy provider found for virtual model: {0}")]
    NoAvailableProvider(String),

    #[error("Invalid or unmapped virtual model requested: {0}")]
    InvalidModel(String),

    #[error("Internal proxy routing error: {0}")]
    Routing(String),
}

pub type Result<T> = std::result::Result<T, OxllmError>;
