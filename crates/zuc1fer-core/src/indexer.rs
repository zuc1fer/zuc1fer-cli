use crate::code_index::CodeIndex;
use crate::ts_parser::TsParser;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct Indexer {
    index: Arc<CodeIndex>,
    working_dir: PathBuf,
}

impl Indexer {
    pub fn new(index: Arc<CodeIndex>, working_dir: PathBuf) -> Self {
        Self { index, working_dir }
    }

    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::task::spawn_blocking(move || {
            self.run_watcher();
        })
    }

    pub fn build_full_index(index: &Arc<CodeIndex>, working_dir: &PathBuf) -> anyhow::Result<()> {
        let extensions: HashSet<&str> = [
            "rs", "py", "js", "ts", "tsx", "jsx", "go", "c", "cpp", "h", "hpp", "java", "rb",
            "swift", "kt", "scala", "cs", "php", "sh", "bash", "toml", "yaml", "yml", "json", "md",
        ]
        .iter()
        .copied()
        .collect();

        let files = collect_source_files(working_dir, &extensions);
        let mut batch = Vec::new();

        for file_path in &files {
            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let mut parser = TsParser::new();
            let symbols: Vec<String> = parser
                .extract_symbols(file_path, &content)
                .into_iter()
                .map(|s| s.name)
                .collect();

            batch.push((file_path.clone(), content, symbols, ext.to_string()));

            if batch.len() >= 50 {
                index.index_files_batch(&batch)?;
                batch.clear();
            }
        }

        if !batch.is_empty() {
            index.index_files_batch(&batch)?;
        }

        tracing::info!("Tantivy index built: {} files", files.len());
        Ok(())
    }

    fn run_watcher(self) {
        let (tx, mut rx) = mpsc::unbounded_channel();

        let mut watcher =
            match notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!("Failed to create file watcher: {e}");
                    return;
                }
            };

        if watcher
            .watch(&self.working_dir, RecursiveMode::Recursive)
            .is_err()
        {
            tracing::warn!("Failed to watch directory");
            return;
        }

        tracing::info!("File watcher started for: {}", self.working_dir.display());

        let mut pending: HashSet<PathBuf> = HashSet::new();
        let debounce = Duration::from_secs(2);

        loop {
            let event = match rx.blocking_recv() {
                Some(e) => e,
                None => break,
            };

            let is_relevant = matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
            );

            if !is_relevant {
                continue;
            }

            for path in &event.paths {
                let path_str = path.to_string_lossy();
                if path_str.contains(".zuc1fer")
                    || path_str.contains(".git")
                    || path_str.contains("target")
                {
                    continue;
                }
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if name.starts_with('.') || name == "node_modules" || ext.is_empty() {
                    continue;
                }

                if matches!(event.kind, EventKind::Remove(_)) {
                    if let Err(e) = self.index.remove_file(path) {
                        tracing::warn!("Failed to remove {} from index: {e}", path.display());
                    }
                } else {
                    pending.insert(path.clone());
                }
            }

            while let Ok(event) = rx.try_recv() {
                for path in &event.paths {
                    if matches!(event.kind, EventKind::Remove(_)) {
                        let _ = self.index.remove_file(path);
                    } else {
                        pending.insert(path.clone());
                    }
                }
            }

            if !pending.is_empty() {
                std::thread::sleep(debounce);

                while let Ok(event) = rx.try_recv() {
                    for path in &event.paths {
                        if matches!(event.kind, EventKind::Remove(_)) {
                            let _ = self.index.remove_file(path);
                        } else {
                            pending.insert(path.clone());
                        }
                    }
                }

                let to_index: Vec<PathBuf> = pending.drain().collect();
                for file_path in &to_index {
                    let content = match std::fs::read_to_string(file_path) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let mut parser = TsParser::new();
                    let symbols: Vec<String> = parser
                        .extract_symbols(file_path, &content)
                        .into_iter()
                        .map(|s| s.name)
                        .collect();

                    if let Err(e) = self.index.index_file(file_path, &content, &symbols, ext) {
                        tracing::warn!("Failed to index {}: {e}", file_path.display());
                    }
                }
            }
        }
    }
}

fn collect_source_files(dir: &PathBuf, extensions: &HashSet<&str>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(dir, dir, extensions, &mut files);
    files
}

fn collect_files_recursive(
    base: &PathBuf,
    dir: &PathBuf,
    extensions: &HashSet<&str>,
    files: &mut Vec<PathBuf>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }

        if path.is_dir() {
            collect_files_recursive(base, &path, extensions, files);
        } else if let Some(ext) = path.extension() {
            if extensions.contains(ext.to_string_lossy().as_ref()) {
                files.push(path);
            }
        }
    }
}
