use anyhow::{Context, Result};
use std::{env, fs, path::PathBuf};

#[derive(serde::Deserialize, Clone)]
pub struct Config {
    pub token: String,
    pub default_remote: String,
    pub default_upstream: String,
    pub submit: Submit,
}

#[derive(serde::Deserialize, Clone)]
pub struct Submit {
    /// When creating branches during submit, use this field as a prefix
    pub branch_prefix: Option<String>,

    /// When submitting branches, should the commit sha or the index of the commit
    /// in the stack be used as the branch
    pub use_indexed_branches: bool,

    /// When submitting a stack, if there is no branch associated with the current
    /// HEAD create one
    pub auto_create_branches: bool,

    /// When submitting a stack, should the summary and body of the commit message
    /// always replace the contents of the PR?
    pub authoritative_commits: bool,
}

impl Config {
    pub fn load() -> Result<Self> {
        let home = PathBuf::from(env::var("HOME").context("failed to get home dir")?);
        let config_path = home.join(".config/fel/config.toml");
        let contents = fs::read_to_string(config_path).context("failed to load config")?;
        Ok(toml::from_str(&contents)?)
    }
}
