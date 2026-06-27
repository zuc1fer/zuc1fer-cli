pub mod agent;
pub mod config;
pub mod mcp_bridge;
pub mod mcp_tool;
pub mod repomap;
pub mod session;
pub mod session_store;

use anyhow::Context;
use std::path::PathBuf;

pub fn default_config_dir() -> anyhow::Result<PathBuf> {
    let dir = dirs::home_dir()
        .context("Could not find home directory")?
        .join(".config")
        .join("zuc1fer");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

pub fn default_data_dir() -> anyhow::Result<PathBuf> {
    let dir = default_config_dir()?;
    Ok(dir)
}
