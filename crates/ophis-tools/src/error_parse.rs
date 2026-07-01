use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedError {
    pub file: Option<String>,
    pub line: Option<u64>,
    pub column: Option<u64>,
    pub message: String,
    pub error_type: String,
}

pub fn extract_errors(output: &str) -> Vec<ParsedError> {
    let mut errors = Vec::new();
    errors.extend(parse_rust_errors(output));
    errors.extend(parse_python_tracebacks(output));
    errors.extend(parse_test_failures(output));
    errors
}

fn parse_rust_errors(output: &str) -> Vec<ParsedError> {
    let mut errors = Vec::new();
    let mut lines = output.lines().peekable();

    while let Some(line) = lines.next() {
        if !line.starts_with("error[")
            && !line.starts_with("error: ")
            && !line.starts_with("error:")
        {
            continue;
        }

        let message = if line.starts_with("error[") {
            line.to_string()
        } else {
            line.to_string()
        };

        let mut error = ParsedError {
            file: None,
            line: None,
            column: None,
            message: message.clone(),
            error_type: "rustc".into(),
        };

        if let Some(next) = lines.peek() {
            if let Some(pos) = next.trim().strip_prefix("--> ") {
                if let Some((rest, col_str)) = pos.rsplit_once(':') {
                    if let Some((file_str, line_str)) = rest.rsplit_once(':') {
                        error.file = Some(file_str.trim().to_string());
                        error.line = line_str.parse().ok();
                        error.column = col_str.parse().ok();
                    } else {
                        error.file = Some(pos.trim().to_string());
                    }
                } else {
                    error.file = Some(pos.trim().to_string());
                }
            }
        }

        errors.push(error);
    }
    errors
}

fn parse_python_tracebacks(output: &str) -> Vec<ParsedError> {
    let mut errors = Vec::new();
    let mut lines = output.lines().peekable();
    let mut in_traceback = false;

    while let Some(line) = lines.next() {
        let trimmed = line.trim();

        if trimmed.starts_with("Traceback (most recent call last):") {
            in_traceback = true;
            continue;
        }

        if in_traceback {
            if let Some(rest) = trimmed.strip_prefix("File \"") {
                if let Some((file, line_rest)) = rest.split_once("\", line ") {
                    if let Some((line_str, _)) = line_rest.split_once(',') {
                        let line_no: u64 = line_str.trim().parse().unwrap_or(0);

                        let next_line = lines.next();
                        let msg = next_line.map(|l| l.trim().to_string()).unwrap_or_default();

                        errors.push(ParsedError {
                            file: Some(file.to_string()),
                            line: Some(line_no),
                            column: None,
                            message: msg,
                            error_type: "python".into(),
                        });
                    }
                }
            }

            if trimmed.starts_with("Error:")
                || trimmed.starts_with("SyntaxError:")
                || trimmed.starts_with("TypeError:")
                || trimmed.starts_with("ValueError:")
                || trimmed.starts_with("KeyError:")
                || trimmed.starts_with("ImportError:")
                || trimmed.starts_with("ModuleNotFoundError:")
                || trimmed.starts_with("IndexError:")
                || trimmed.starts_with("AttributeError:")
                || trimmed.starts_with("NameError:")
                || trimmed.starts_with("ZeroDivisionError:")
            {
                let msg = trimmed.to_string();
                if !errors.iter().any(|e| e.message == msg) {
                    errors.push(ParsedError {
                        file: None,
                        line: None,
                        column: None,
                        message: msg,
                        error_type: "python".into(),
                    });
                }
            }

            if trimmed.is_empty() && line.trim().is_empty() {
                in_traceback = false;
            }
        }
    }
    errors
}

fn parse_test_failures(output: &str) -> Vec<ParsedError> {
    let mut errors = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.contains("FAILED") || trimmed.starts_with("test ") && trimmed.contains("FAILED")
        {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            let test_name = parts
                .iter()
                .find(|p| p.starts_with("test_") || p.starts_with("tests::"))
                .cloned()
                .unwrap_or(trimmed);

            errors.push(ParsedError {
                file: None,
                line: None,
                column: None,
                message: format!("Test FAILED: {test_name}"),
                error_type: "test".into(),
            });
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_compiler_error() {
        let output = "error[E0308]: mismatched types
 --> src/main.rs:42:5
  |
42 |     let x: i32 = \"hello\";
  |     ^^^^^^^^^^^^^^^^^^^ expected `i32`, found `&str`
";
        let errors = extract_errors(output);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, "rustc");
        assert_eq!(errors[0].file.as_deref(), Some("src/main.rs"));
        assert_eq!(errors[0].line, Some(42));
        assert_eq!(errors[0].column, Some(5));
        assert!(errors[0].message.contains("E0308"));
    }

    #[test]
    fn test_rust_plain_error() {
        let output = "error: could not compile `foo` due to previous error\n";
        let errors = extract_errors(output);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, "rustc");
        assert!(errors[0].file.is_none());
    }

    #[test]
    fn test_python_traceback() {
        let output = "Traceback (most recent call last):
  File \"test.py\", line 10, in <module>
    main()
  File \"test.py\", line 5, in main
    return 1 / 0
ZeroDivisionError: division by zero
";
        let errors = extract_errors(output);
        assert!(errors.len() >= 2);
        assert!(errors.iter().any(|e| e.error_type == "python"));
    }

    #[test]
    fn test_test_failure() {
        let output = "
test tests::test_foo ... FAILED
test tests::test_bar ... ok
";
        let errors = extract_errors(output);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, "test");
        assert!(errors[0].message.contains("test_foo"));
    }

    #[test]
    fn test_no_errors() {
        let errors = extract_errors("hello world\nall good\n");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_multiple_rust_errors() {
        let output = "error[E0308]: mismatched types
 --> src/main.rs:42:5
error[E0599]: no method named `foo` found
 --> src/lib.rs:10:15
";
        let errors = extract_errors(output);
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].file.as_deref(), Some("src/main.rs"));
        assert_eq!(errors[1].file.as_deref(), Some("src/lib.rs"));
    }
}
