use std::path::Path;
use tree_sitter::StreamingIterator;

#[derive(Debug, Clone)]
pub struct TsSymbol {
    pub name: String,
    pub kind: String,
    pub line: usize,
    pub signature: String,
}

pub struct TsParser {
    parser: tree_sitter::Parser,
}

impl Default for TsParser {
    fn default() -> Self {
        Self::new()
    }
}

impl TsParser {
    pub fn new() -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).ok();
        Self { parser }
    }

    fn language_for_ext(ext: &str) -> Option<tree_sitter::Language> {
        match ext {
            "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
            "py" => Some(tree_sitter_python::LANGUAGE.into()),
            "js" | "jsx" => Some(tree_sitter_javascript::LANGUAGE.into()),
            "ts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
            "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
            "go" => Some(tree_sitter_go::LANGUAGE.into()),
            _ => None,
        }
    }

    pub fn extract_symbols(&mut self, path: &Path, content: &str) -> Vec<TsSymbol> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let lang = match Self::language_for_ext(ext) {
            Some(l) => l,
            None => return Vec::new(),
        };

        let Ok(_) = self.parser.set_language(&lang) else {
            return Vec::new();
        };
        let Some(tree) = self.parser.parse(content, None) else {
            return Vec::new();
        };
        let root = tree.root_node();

        let query_str = match ext {
            "rs" => Self::rust_query(),
            "py" => Self::python_query(),
            "js" | "jsx" | "ts" | "tsx" => Self::js_query(),
            "go" => Self::go_query(),
            _ => return Vec::new(),
        };

        let Ok(query) = tree_sitter::Query::new(&lang, query_str) else {
            return Vec::new();
        };
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, root, content.as_bytes());

        let mut symbols = Vec::new();
        while let Some(m) = matches.next() {
            let mut name = String::new();
            let mut kind = String::new();

            for capture in m.captures.iter() {
                let capture_name = query.capture_names()[capture.index as usize];
                let node = capture.node;
                let text = node.utf8_text(content.as_bytes()).unwrap_or("");

                if capture_name == "name" {
                    name = text.to_string();
                    if kind.is_empty() {
                        kind = node.kind().to_string();
                    }
                }
            }

            if name.is_empty() || name.starts_with('_') {
                continue;
            }

            let sig_node = m.captures.first().map(|c| c.node);
            let signature = sig_node
                .and_then(|n| {
                    let start = n.start_position();
                    let end = n.end_position();
                    let lines: Vec<&str> = content
                        .lines()
                        .skip(start.row)
                        .take(end.row - start.row + 1)
                        .collect();
                    lines.first().map(|l| l.trim().to_string())
                })
                .unwrap_or_default();

            let line = sig_node.map(|n| n.start_position().row + 1).unwrap_or(1);

            symbols.push(TsSymbol {
                name,
                kind,
                line,
                signature,
            });
        }

        symbols
    }

    pub fn extract_imports(&mut self, path: &Path, content: &str) -> Vec<String> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let lang = match Self::language_for_ext(ext) {
            Some(l) => l,
            None => return Vec::new(),
        };

        let Ok(_) = self.parser.set_language(&lang) else {
            return Vec::new();
        };
        let Some(tree) = self.parser.parse(content, None) else {
            return Vec::new();
        };
        let root = tree.root_node();

        let query_str: &str = match ext {
            "rs" => "(use_declaration) @import",
            "py" => "(import_statement) @import\n(import_from_statement) @import",
            "js" | "jsx" | "ts" | "tsx" => "(import_statement) @import",
            "go" => "(import_declaration) @import",
            _ => return Vec::new(),
        };

        let Ok(query) = tree_sitter::Query::new(&lang, query_str) else {
            return Vec::new();
        };
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, root, content.as_bytes());

        let mut imports = Vec::new();
        while let Some(m) = matches.next() {
            for capture in m.captures.iter() {
                let text = capture.node.utf8_text(content.as_bytes()).unwrap_or("");
                imports.push(text.to_string());
            }
        }

        imports
    }

    fn rust_query() -> &'static str {
        r#"
(function_item name: (identifier) @name)
(struct_item name: (type_identifier) @name)
(enum_item name: (type_identifier) @name)
(trait_item name: (type_identifier) @name)
(impl_item type: (type_identifier) @name)
(mod_item name: (identifier) @name)
(const_item name: (identifier) @name)
(static_item name: (identifier) @name)
"#
    }

    fn python_query() -> &'static str {
        r#"
(function_definition name: (identifier) @name)
(class_definition name: (identifier) @name)
"#
    }

    fn js_query() -> &'static str {
        r#"
(function_declaration name: (identifier) @name)
(class_declaration name: (identifier) @name)
(export_statement
  declaration: (function_declaration name: (identifier) @name))
(export_statement
  declaration: (class_declaration name: (identifier) @name))
(variable_declarator
  name: (identifier) @name
  value: (arrow_function))
(variable_declarator
  name: (identifier) @name
  value: (function_expression))
(method_definition name: (property_identifier) @name)
"#
    }

    fn go_query() -> &'static str {
        r#"
(function_declaration name: (identifier) @name)
(method_declaration name: (field_identifier) @name)
(type_declaration (type_spec name: (type_identifier) @name))
"#
    }
}
