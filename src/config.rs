use anyhow::{Context, Result};
use std::{env, fs, path::PathBuf};

#[derive(serde::Deserialize)]
pub struct Config {
    pub token: String,
    pub default_remote: String,
    pub default_upstream: String,
    pub branch_prefix: Option<String>,
    pub use_indexed_branches: bool,
    pub auto_create_branches: bool,
}

impl Config {
    pub fn load() -> Result<Self> {
        let home = PathBuf::from(env::var("HOME").context("failed to get home dir")?);
        let config_path = home.join(".config/fel/config.toml");
        let contents = fs::read_to_string(config_path).context("failed to load config")?;
        Ok(toml::from_str(&contents)?)
    }
}
