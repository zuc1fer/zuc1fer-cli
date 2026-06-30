use crate::ts_parser::TsParser;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub file: PathBuf,
    pub line: usize,
    pub signature: String,
}

#[derive(Debug, Clone)]
pub struct RepoMap {
    pub symbols: Vec<Arc<Symbol>>,
    pub score_map: HashMap<String, f64>,
    pub file_rankings: Vec<(PathBuf, f64)>,
    token_budget: usize,
    working_dir: PathBuf,
}

impl RepoMap {
    pub fn new(working_dir: PathBuf, token_budget: usize) -> Self {
        Self {
            symbols: Vec::new(),
            score_map: HashMap::new(),
            file_rankings: Vec::new(),
            token_budget,
            working_dir,
        }
    }

    pub fn build(&mut self) -> anyhow::Result<()> {
        self.scan_repo()?;
        self.build_dependency_graph();
        self.rank_files();
        Ok(())
    }

    fn scan_repo(&mut self) -> anyhow::Result<()> {
        let extensions: HashSet<&str> = [
            "rs", "py", "js", "ts", "tsx", "jsx", "go", "c", "cpp", "h", "hpp", "java", "rb",
            "swift", "kt", "scala", "cs", "php", "sh", "bash", "zsh", "fish", "toml", "yaml",
            "yml", "json", "md",
        ]
        .iter()
        .copied()
        .collect();

        self.walk_dir(&self.working_dir.clone(), &extensions)?;
        Ok(())
    }

    fn walk_dir(&mut self, dir: &Path, extensions: &HashSet<&str>) -> anyhow::Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }

            if path.is_dir() {
                self.walk_dir(&path, extensions)?;
            } else if let Some(ext) = path.extension() {
                if extensions.contains(ext.to_string_lossy().as_ref()) {
                    self.extract_symbols(&path)?;
                }
            }
        }

        Ok(())
    }

    fn extract_symbols(&mut self, path: &Path) -> anyhow::Result<()> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Ok(()),
        };

        let mut parser = TsParser::new();
        for sym in parser.extract_symbols(path, &content) {
            self.symbols.push(Arc::new(Symbol {
                name: sym.name,
                kind: sym.kind,
                file: path.to_path_buf(),
                line: sym.line,
                signature: sym.signature,
            }));
        }

        Ok(())
    }

    fn build_dependency_graph(&mut self) {
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        let mut seen_files: HashSet<PathBuf> = HashSet::new();

        for sym in &self.symbols {
            if !seen_files.insert(sym.file.clone()) {
                continue;
            }
            let file_str = sym.file.to_string_lossy().to_string();
            let content = std::fs::read_to_string(&sym.file).unwrap_or_default();

            let imports = self.extract_imports(&sym.file, &content);
            let mut target_files: Vec<String> = imports
                .into_iter()
                .filter_map(|import_path| {
                    self.resolve_import(&sym.file, &import_path)
                        .map(|p| p.to_string_lossy().to_string())
                })
                .collect();
            target_files.sort();
            target_files.dedup();

            edges
                .entry(file_str.clone())
                .or_default()
                .extend(target_files.clone());

            for target in &target_files {
                edges.entry(target.clone()).or_default();
            }
        }

        self.score_map = self.pagerank(&edges);
    }

    fn extract_imports(&self, path: &Path, content: &str) -> Vec<String> {
        let mut parser = TsParser::new();
        let raw = parser.extract_imports(path, content);
        raw.into_iter()
            .filter_map(|imp| self.normalize_import(path, &imp))
            .collect()
    }

    fn normalize_import(&self, _from_file: &Path, import_line: &str) -> Option<String> {
        let trimmed = import_line.trim();
        if trimmed.starts_with("use ") {
            let rest = trimmed.strip_prefix("use ")?;
            if let Some(idx) = rest.find("::") {
                Some(rest[..idx].to_string())
            } else {
                Some(rest.trim_end_matches(';').to_string())
            }
        } else if trimmed.starts_with("import ") {
            let rest = trimmed.strip_prefix("import ")?;
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 2 {
                Some(parts[1].trim_matches('"').trim_matches('\'').to_string())
            } else {
                None
            }
        } else if trimmed.starts_with("from ") {
            let rest = trimmed.strip_prefix("from ")?;
            rest.split_whitespace().next().map(|s| s.to_string())
        } else if trimmed.starts_with("import (") {
            trimmed
                .trim_start_matches("import (")
                .trim_end_matches(')')
                .split_whitespace()
                .next()
                .filter(|p| p.starts_with('"'))
                .map(|p| p.trim_matches('"').to_string())
        } else {
            None
        }
    }

    fn resolve_import(&self, from_file: &Path, import: &str) -> Option<PathBuf> {
        let from_dir = from_file.parent().unwrap_or(Path::new(""));

        for candidate in &[
            from_dir.join(import),
            from_dir.join(format!("{import}.rs")),
            from_dir.join(format!("{import}.py")),
            from_dir.join(format!("{import}.ts")),
            from_dir.join(format!("{import}.js")),
            from_dir.join(format!("{import}/mod.rs")),
            from_dir.join(format!("{import}/index.ts")),
            from_dir.join(format!("{import}/index.js")),
            from_dir.join(format!("{import}/__init__.py")),
        ] {
            if candidate.exists() {
                return Some(self.working_dir.join(candidate));
            }
        }
        None
    }

    fn pagerank(&self, edges: &HashMap<String, Vec<String>>) -> HashMap<String, f64> {
        let damping: f64 = 0.85;
        let iterations = 20;

        let mut node_set: HashSet<&str> = HashSet::new();
        for (src, targets) in edges {
            node_set.insert(src.as_str());
            for t in targets {
                node_set.insert(t.as_str());
            }
        }
        let all_nodes: Vec<&str> = node_set.into_iter().collect();
        let n = all_nodes.len() as f64;
        if n == 0.0 {
            return HashMap::new();
        }
        let base = (1.0 - damping) / n;

        let mut out_degree: HashMap<&str, f64> = HashMap::new();
        let mut incoming: HashMap<&str, Vec<&str>> = HashMap::new();
        for (src, targets) in edges {
            out_degree.insert(src.as_str(), targets.len().max(1) as f64);
            for t in targets {
                incoming.entry(t.as_str()).or_default().push(src.as_str());
            }
        }

        let mut scores: HashMap<&str, f64> = all_nodes.iter().map(|nd| (*nd, 1.0 / n)).collect();

        for _ in 0..iterations {
            let mut new_scores: HashMap<&str, f64> = HashMap::with_capacity(all_nodes.len());
            for node in &all_nodes {
                let mut rank = base;
                if let Some(sources) = incoming.get(node) {
                    for src in sources {
                        let src_score = scores.get(src).copied().unwrap_or(0.0);
                        let od = out_degree.get(src).copied().unwrap_or(1.0);
                        rank += damping * src_score / od;
                    }
                }
                new_scores.insert(*node, rank);
            }
            scores = new_scores;
        }

        scores
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    }

    fn rank_files(&mut self) {
        let mut file_scores: Vec<(PathBuf, f64)> = self
            .score_map
            .iter()
            .map(|(path, score)| (PathBuf::from(path), *score))
            .collect();

        file_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        self.file_rankings = file_scores;

        let symbol_scores: HashMap<String, f64> = self
            .symbols
            .iter()
            .map(|s| {
                (
                    s.file.to_string_lossy().to_string(),
                    self.file_score(&s.file),
                )
            })
            .collect();

        self.symbols.sort_by(|a, b| {
            let score_a = symbol_scores
                .get(&a.file.to_string_lossy().to_string())
                .copied()
                .unwrap_or(0.0);
            let score_b = symbol_scores
                .get(&b.file.to_string_lossy().to_string())
                .copied()
                .unwrap_or(0.0);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    fn file_score(&self, file: &Path) -> f64 {
        let key = file.to_string_lossy().to_string();
        self.score_map.get(&key).copied().unwrap_or(0.0)
    }

    pub fn format_context(&self) -> String {
        let mut output = String::from("Repository map (top symbols by relevance):\n\n");
        let mut used_files: HashSet<String> = HashSet::new();
        let mut token_count = 0;

        for sym in &self.symbols {
            let file_key = sym.file.to_string_lossy().to_string();
            if used_files.contains(&file_key) {
                continue;
            }

            let score = self.file_score(&sym.file);
            if score < 0.01 {
                continue;
            }

            let rel_path = sym
                .file
                .strip_prefix(&self.working_dir)
                .unwrap_or(&sym.file)
                .display()
                .to_string()
                .replace('\\', "/");

            let entry = format!(
                "\n{rel_path} (relevance: {:.3})\n  {} {}: {}\n",
                score, sym.kind, sym.name, sym.signature
            );

            if token_count + entry.len() / 3 > self.token_budget {
                if token_count == 0 {
                    output.push_str(&entry);
                }
                break;
            }

            output.push_str(&entry);
            token_count += entry.len() / 3;
            used_files.insert(file_key);
        }

        if output.is_empty() {
            output.push_str("(no symbols found in repository)\n");
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_symbols_from_source() {
        let source = r#"
pub fn hello_world() -> String {
    "hello".into()
}

pub struct User {
    name: String,
}

pub enum Status {
    Active,
    Inactive,
}

pub trait Handler {
    fn handle(&self);
}

mod utils;
const MAX_SIZE: usize = 1024;
"#;
        let mut parser = TsParser::new();
        let path = Path::new("test.rs");
        let symbols = parser.extract_symbols(path, source);

        let names: Vec<String> = symbols.iter().map(|s| s.name.clone()).collect();
        assert!(
            names.contains(&"hello_world".to_string()),
            "missing hello_world fn"
        );
        assert!(names.contains(&"User".to_string()), "missing User struct");
        assert!(names.contains(&"Status".to_string()), "missing Status enum");
        assert!(
            names.contains(&"Handler".to_string()),
            "missing Handler trait"
        );
        assert!(names.contains(&"utils".to_string()), "missing utils mod");
        assert!(
            names.contains(&"MAX_SIZE".to_string()),
            "missing MAX_SIZE const"
        );
    }

    #[test]
    fn test_extract_python_symbols() {
        let source = r#"
def get_user(user_id: int) -> User:
    pass

class UserManager:
    def create(self, name):
        pass

async def fetch_data():
    pass
"#;
        let mut parser = TsParser::new();
        let path = Path::new("test.py");
        let symbols = parser.extract_symbols(path, source);

        let names: Vec<String> = symbols.iter().map(|s| s.name.clone()).collect();
        assert!(names.contains(&"get_user".to_string()), "missing get_user");
        assert!(
            names.contains(&"UserManager".to_string()),
            "missing UserManager class"
        );
        assert!(
            names.contains(&"fetch_data".to_string()),
            "missing fetch_data"
        );
    }

    #[test]
    fn test_extract_js_symbols() {
        let source = r#"
function fetchUsers() {
    return [];
}

class ApiClient {
    request() {}
}

export function init() {}

const handler = () => {};
"#;
        let mut parser = TsParser::new();
        let path = Path::new("test.js");
        let symbols = parser.extract_symbols(path, source);

        let names: Vec<String> = symbols.iter().map(|s| s.name.clone()).collect();
        assert!(
            names.contains(&"fetchUsers".to_string()),
            "missing fetchUsers"
        );
        assert!(
            names.contains(&"ApiClient".to_string()),
            "missing ApiClient"
        );
        assert!(names.contains(&"init".to_string()), "missing init export");
    }

    #[test]
    fn test_extract_go_symbols() {
        let source = r#"
func NewServer() *Server {
    return &Server{}
}

type Config struct {
    Port int
}

func (s *Server) Start() error {
    return nil
}
"#;
        let mut parser = TsParser::new();
        let path = Path::new("test.go");
        let symbols = parser.extract_symbols(path, source);

        let names: Vec<String> = symbols.iter().map(|s| s.name.clone()).collect();
        assert!(
            names.contains(&"NewServer".to_string()),
            "missing NewServer"
        );
        assert!(names.contains(&"Config".to_string()), "missing Config type");
        assert!(names.contains(&"Start".to_string()), "missing Start method");
    }

    #[test]
    fn test_regex_import_parsing() {
        let map = RepoMap::new(PathBuf::from("."), 1024);
        let imp = map.normalize_import(Path::new("lib.rs"), "use std::collections::HashMap;");
        assert_eq!(imp, Some("std".to_string()));
    }
}
