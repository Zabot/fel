use anyhow::{Context, Result};
use std::fs;

#[derive(serde::Deserialize)]
pub struct Config {
    pub token: String,
    pub default_remote: String,
    pub default_upstream: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        let contents = fs::read_to_string("fel.toml").context("failed to load config")?;
        Ok(toml::from_str(&contents)?)
    }
}
