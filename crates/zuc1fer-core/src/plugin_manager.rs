use crate::plugin_tool::PluginTool;
use serde::Deserialize;
use std::sync::Arc;
use zuc1fer_tools::ToolRegistry;

#[derive(Debug, Deserialize)]
struct PluginManifest {
    plugin: PluginMeta,
    #[serde(default)]
    tools: Vec<ToolDefToml>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PluginMeta {
    name: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ToolDefToml {
    name: String,
    description: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
}

fn default_timeout() -> u64 {
    60_000
}

pub fn discover_plugins(registry: &mut ToolRegistry) -> anyhow::Result<Vec<String>> {
    let plugin_dir = crate::default_config_dir()?.join("plugins");
    if !plugin_dir.exists() {
        return Ok(Vec::new());
    }

    let mut loaded = Vec::new();

    let entries = match std::fs::read_dir(&plugin_dir) {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("plugin.toml");
        if !manifest_path.exists() {
            continue;
        }

        let content = match std::fs::read_to_string(&manifest_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "Failed to read plugin manifest {}: {e}",
                    manifest_path.display()
                );
                continue;
            }
        };

        let manifest: PluginManifest = match toml::from_str(&content) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Invalid plugin manifest {}: {e}", manifest_path.display());
                continue;
            }
        };

        let plugin_name = manifest.plugin.name.clone();

        for tool_def in &manifest.tools {
            let tool = PluginTool::from_config(
                tool_def.name.clone(),
                tool_def.description.clone(),
                tool_def.command.clone(),
                tool_def.args.clone(),
                tool_def.timeout_ms,
            );
            registry.register(Arc::new(tool));
        }

        tracing::info!(
            "Plugin loaded: {} ({} tools) from {}",
            plugin_name,
            manifest.tools.len(),
            path.display()
        );
        loaded.push(plugin_name);
    }

    Ok(loaded)
}
