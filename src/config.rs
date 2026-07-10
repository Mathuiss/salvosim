use crate::{error::ConfigLoadError, models::ScenarioConfig};
use std::fs;

/// Reads a TOML file from the given path and parses it into the ScenarioConfig struct in memory.
pub fn load_scenario(file_path: &str) -> Result<ScenarioConfig, ConfigLoadError> {
    // Read the raw text from the file
    let file_contents = fs::read_to_string(file_path)?;

    // Parse the TOML string into our Rust data structures
    let config: ScenarioConfig = toml::from_str(&file_contents)?;

    Ok(config)
}
