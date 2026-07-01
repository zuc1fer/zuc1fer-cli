use crate::error_parse::ParsedError;

pub struct Alternative {
    pub label: String,
    pub command: String,
}

pub fn generate_alternatives(cmd: &str, errors: &[ParsedError]) -> Vec<Alternative> {
    let mut alts = Vec::new();

    let trimmed = cmd.trim();

    if is_rust_build_cmd(trimmed) {
        alts.push(Alternative {
            label: "cargo check".into(),
            command: trimmed
                .replace("cargo build", "cargo check")
                .replace("cargo run", "cargo check"),
        });

        for err in errors {
            if is_resolve_error(err) {
                alts.push(Alternative {
                    label: "cargo update".into(),
                    command: format!("cd /d {} && cargo update", extract_workspace_dir(trimmed)),
                });
                break;
            }
        }
    }

    if trimmed.contains("cargo test") {
        alts.push(Alternative {
            label: "cargo build first".into(),
            command: trimmed.replace("cargo test", "cargo build"),
        });
    }

    alts
}

fn is_rust_build_cmd(cmd: &str) -> bool {
    cmd.contains("cargo build")
        || cmd.contains("cargo check")
        || cmd.contains("cargo run")
        || cmd.contains("cargo test")
}

fn is_resolve_error(err: &ParsedError) -> bool {
    err.error_type == "rustc"
        && (err.message.contains("E0432") || err.message.contains("E0433")
            || err.message.contains("E0463") || err.message.contains("E0277")
            || err.message.contains("unresolved"))
}

fn extract_workspace_dir(cmd: &str) -> String {
    if let Some(pos) = cmd.find("cd /d ") {
        let rest = &cmd[pos + 6..];
        if let Some(end) = rest.find(" && ") {
            return rest[..end].trim().to_string();
        }
    }
    ".".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cargo_build_gets_check_alternative() {
        let alts = generate_alternatives("cd /tmp && cargo build", &[]);
        assert!(alts.iter().any(|a| a.command.contains("cargo check")));
    }

    #[test]
    fn test_no_alts_for_non_rust() {
        let alts = generate_alternatives("echo hello", &[]);
        assert!(alts.is_empty());
    }

    #[test]
    fn test_resolve_error_adds_cargo_update() {
        let err = ParsedError {
            file: Some("src/main.rs".into()),
            line: Some(1),
            column: Some(1),
            message: "error[E0433]: failed to resolve".into(),
            error_type: "rustc".into(),
        };
        let alts = generate_alternatives("cd /tmp && cargo build", &[err]);
        assert!(alts.iter().any(|a| a.label == "cargo update"));
    }
}
