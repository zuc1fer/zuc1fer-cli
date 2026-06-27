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

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            let sym = match ext {
                "rs" => self.parse_rust_symbol(trimmed),
                "py" => self.parse_python_symbol(trimmed),
                "js" | "ts" | "jsx" | "tsx" => self.parse_js_symbol(trimmed),
                "go" => self.parse_go_symbol(trimmed),
                "c" | "cpp" | "h" | "hpp" => self.parse_c_symbol(trimmed),
                "java" | "kt" | "scala" => self.parse_java_symbol(trimmed),
                _ => None,
            };

            if let Some((name, kind, sig)) = sym {
                self.symbols.push(Arc::new(Symbol {
                    name,
                    kind,
                    file: path.to_path_buf(),
                    line: idx + 1,
                    signature: sig,
                }));
            }
        }

        Ok(())
    }

    fn parse_rust_symbol(&self, line: &str) -> Option<(String, String, String)> {
        let re = regex::Regex::new(
            r"^\s*(?:pub(?:\s*\(\s*(?:crate|super|self)\s*\))?\s+)?(fn|struct|enum|trait|impl|mod|type|const|static)\s+(\w+)"
        ).ok()?;
        let caps = re.captures(line)?;
        let kind = caps.get(1)?.as_str().to_string();
        let name = caps.get(2)?.as_str().to_string();

        if name.chars().next()?.is_uppercase() || kind == "fn" || kind == "mod" {
            let sig = line.trim().to_string();
            Some((name, kind, sig))
        } else {
            None
        }
    }

    fn parse_python_symbol(&self, line: &str) -> Option<(String, String, String)> {
        let re = regex::Regex::new(r"^\s*(def|class|async def)\s+(\w+)").ok()?;
        let caps = re.captures(line)?;
        let kind = caps.get(1)?.as_str().to_string();
        let name = caps.get(2)?.as_str().to_string();
        Some((name, kind, line.trim().to_string()))
    }

    fn parse_js_symbol(&self, line: &str) -> Option<(String, String, String)> {
        let re = regex::Regex::new(
            r"^\s*(?:export\s+(?:default\s+)?)?(?:async\s+)?(?:function|class|const|let|var)\s+(\w+)"
        ).ok()?;
        let caps = re.captures(line)?;
        let name = caps.get(1)?.as_str().to_string();

        let kind = if line.contains("function") || line.contains("=>") {
            "function"
        } else if line.contains("class") {
            "class"
        } else {
            "const"
        };

        Some((name, kind.to_string(), line.trim().to_string()))
    }

    fn parse_go_symbol(&self, line: &str) -> Option<(String, String, String)> {
        let re = regex::Regex::new(r"^\s*(?:func|type)\s+(\w+)").ok()?;
        let caps = re.captures(line)?;
        let name = caps.get(1)?.as_str().to_string();
        let kind = if line.contains("func") {
            "func"
        } else {
            "type"
        };
        Some((name, kind.to_string(), line.trim().to_string()))
    }

    fn parse_c_symbol(&self, line: &str) -> Option<(String, String, String)> {
        let re =
            regex::Regex::new(r"^\s*(?:[\w:]+\s+)+(\w+)\s*\([^)]*\)\s*(?:const\s*)?\{?").ok()?;
        let caps = re.captures(line)?;
        let name = caps.get(1)?.as_str().to_string();
        if name == "if" || name == "while" || name == "for" || name == "switch" {
            return None;
        }
        Some((name, "func".into(), line.trim().to_string()))
    }

    fn parse_java_symbol(&self, line: &str) -> Option<(String, String, String)> {
        let re = regex::Regex::new(
            r"^\s*(?:public|private|protected)?\s*(?:static|abstract|final)?\s*(?:class|interface|enum)\s+(\w+)"
        ).ok()?;
        let caps = re.captures(line)?;
        let name = caps.get(1)?.as_str().to_string();
        let kind = if line.contains("class") {
            "class"
        } else if line.contains("interface") {
            "interface"
        } else {
            "enum"
        };
        Some((name, kind.to_string(), line.trim().to_string()))
    }

    fn build_dependency_graph(&mut self) {
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();

        for sym in &self.symbols {
            let file_str = sym.file.to_string_lossy().to_string();
            let content = std::fs::read_to_string(&sym.file).unwrap_or_default();

            let imports = self.extract_imports(&content, &sym.file);
            let target_files: Vec<String> = imports
                .into_iter()
                .filter_map(|import_path| {
                    self.resolve_import(&sym.file, &import_path)
                        .map(|p| p.to_string_lossy().to_string())
                })
                .collect();

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

    fn extract_imports(&self, content: &str, _file: &Path) -> Vec<String> {
        let mut imports = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(path) = self.parse_import_line(trimmed) {
                imports.push(path);
            }
        }
        imports
    }

    fn parse_import_line(&self, line: &str) -> Option<String> {
        if line.starts_with("use ") {
            let rest = line.strip_prefix("use ")?;
            if let Some(idx) = rest.find("::") {
                Some(rest[..idx].to_string())
            } else {
                Some(rest.trim_end_matches(';').to_string())
            }
        } else if line.starts_with("import ") {
            let rest = line.strip_prefix("import ")?;
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 2 {
                Some(parts[1].trim_matches('"').trim_matches('\'').to_string())
            } else {
                None
            }
        } else if line.starts_with("from ") {
            let rest = line.strip_prefix("from ")?;
            rest.split_whitespace().next().map(|s| s.to_string())
        } else if line.starts_with("require(") {
            let rest = line.strip_prefix("require(")?;
            rest.trim_end_matches(");")
                .trim_matches('"')
                .trim_matches('\'')
                .split('/')
                .next()
                .map(|s| s.to_string())
        } else {
            None
        }
    }

    fn resolve_import(&self, from_file: &Path, import: &str) -> Option<PathBuf> {
        let from_dir = from_file.parent().unwrap_or(Path::new(""));

        for candidate in &[
            from_dir.join(&import),
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
        let mut scores: HashMap<String, f64> = HashMap::new();

        let all_nodes: Vec<&String> = edges
            .keys()
            .chain(edges.values().flat_map(|v| v.iter()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        let n = all_nodes.len() as f64;
        let base = (1.0 - damping) / n;

        for node in &all_nodes {
            scores.insert((*node).clone(), 1.0 / n);
        }

        for _ in 0..iterations {
            let mut new_scores: HashMap<String, f64> = HashMap::new();

            for node in &all_nodes {
                let mut rank = base;
                for (source, targets) in edges {
                    if targets.contains(node) {
                        let source_score = scores.get(source).copied().unwrap_or(0.0);
                        let out_degree = targets.len().max(1) as f64;
                        rank += damping * source_score / out_degree;
                    }
                }
                new_scores.insert((*node).clone(), rank);
            }

            scores = new_scores;
        }

        scores
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
                .to_string();

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
    fn test_parse_rust_fn() {
        let map = RepoMap::new(PathBuf::from("."), 1024);
        let result = map.parse_rust_symbol("pub fn hello_world() -> String {");
        assert!(result.is_some());
        let (name, kind, _) = result.unwrap();
        assert_eq!(name, "hello_world");
        assert_eq!(kind, "fn");
    }

    #[test]
    fn test_parse_python_def() {
        let map = RepoMap::new(PathBuf::from("."), 1024);
        let result = map.parse_python_symbol("def get_user(user_id: int) -> User:");
        assert!(result.is_some());
        let (name, kind, _) = result.unwrap();
        assert_eq!(name, "get_user");
        assert_eq!(kind, "def");
    }

    #[test]
    fn test_parse_rust_struct() {
        let map = RepoMap::new(PathBuf::from("."), 1024);
        let result = map.parse_rust_symbol("pub struct User {");
        assert!(result.is_some());
        let (name, kind, _) = result.unwrap();
        assert_eq!(name, "User");
        assert_eq!(kind, "struct");
    }

    #[test]
    fn test_parse_js_export() {
        let map = RepoMap::new(PathBuf::from("."), 1024);
        let result = map.parse_js_symbol("export function fetchUsers() {");
        assert!(result.is_some());
        let (name, kind, _) = result.unwrap();
        assert_eq!(name, "fetchUsers");
    }
}
