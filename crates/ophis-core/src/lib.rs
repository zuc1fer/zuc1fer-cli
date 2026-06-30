pub mod agent;
pub mod code_index;
pub mod config;
pub mod indexer;
pub mod lsp_client;
pub mod lsp_tool;
pub mod mcp_bridge;
pub mod mcp_tool;
pub mod plugin_manager;
pub mod plugin_tool;
pub mod repomap;
pub mod semantic_tool;
pub mod session;
pub mod session_store;
pub mod ts_parser;

use anyhow::Context;
use std::path::PathBuf;

pub fn default_config_dir() -> anyhow::Result<PathBuf> {
    let dir = dirs::home_dir()
        .context("Could not find home directory")?
        .join(".config")
        .join("OPHIS");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

pub fn default_data_dir() -> anyhow::Result<PathBuf> {
    let dir = default_config_dir()?;
    Ok(dir)
}
