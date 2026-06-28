use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};

pub struct CodeIndex {
    index: Index,
    reader: IndexReader,
    fields: IndexFields,
    _path: PathBuf,
}

struct IndexFields {
    path: Field,
    content: Field,
    symbols: Field,
    language: Field,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: String,
    pub score: f32,
    pub snippet: String,
}

impl CodeIndex {
    pub fn open_or_create(index_dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(index_dir).ok();

        let mut schema_builder = Schema::builder();
        let path_field = schema_builder.add_text_field("path", STRING | STORED);
        let content_field = schema_builder.add_text_field("content", TEXT);
        let symbols_field = schema_builder.add_text_field("symbols", TEXT | STORED);
        let language_field = schema_builder.add_text_field("language", STRING | STORED);
        let schema = schema_builder.build();

        let index = if index_dir.join("meta.json").exists() {
            Index::open_in_dir(index_dir)?
        } else {
            Index::create_in_dir(index_dir, schema.clone())?
        };

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        Ok(Self {
            index,
            reader,
            fields: IndexFields {
                path: path_field,
                content: content_field,
                symbols: symbols_field,
                language: language_field,
            },
            _path: index_dir.to_path_buf(),
        })
    }

    pub fn index_file(
        &self,
        file_path: &Path,
        content: &str,
        symbols: &[String],
        language: &str,
    ) -> anyhow::Result<()> {
        let mut writer: IndexWriter = self.index.writer(50_000_000)?;

        let path_str = file_path.display().to_string().replace('\\', "/");
        let symbols_str = symbols.join(" ");

        writer.delete_term(Term::from_field_text(self.fields.path, &path_str));

        writer.add_document(doc!(
            self.fields.path => path_str.clone(),
            self.fields.content => content.to_string(),
            self.fields.symbols => symbols_str,
            self.fields.language => language.to_string(),
        ))?;

        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn index_files_batch(
        &self,
        files: &[(PathBuf, String, Vec<String>, String)],
    ) -> anyhow::Result<()> {
        if files.is_empty() {
            return Ok(());
        }
        let mut writer: IndexWriter = self.index.writer(50_000_000)?;

        for (file_path, content, symbols, language) in files {
            let path_str = file_path.display().to_string().replace('\\', "/");
            let symbols_str = symbols.join(" ");

            writer.delete_term(Term::from_field_text(self.fields.path, &path_str));
            writer.add_document(doc!(
                self.fields.path => path_str.clone(),
                self.fields.content => content.clone(),
                self.fields.symbols => symbols_str,
                self.fields.language => language.clone(),
            ))?;
        }

        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn remove_file(&self, file_path: &Path) -> anyhow::Result<()> {
        let mut writer: IndexWriter = self.index.writer(50_000_000)?;
        let path_str = file_path.display().to_string().replace('\\', "/");
        writer.delete_term(Term::from_field_text(self.fields.path, &path_str));
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn search(&self, query_str: &str, limit: usize) -> anyhow::Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();

        let query_parser =
            QueryParser::for_index(&self.index, vec![self.fields.content, self.fields.symbols]);
        let query = query_parser.parse_query(query_str)?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc = searcher.doc::<TantivyDocument>(doc_address)?;

            let path = doc
                .get_first(self.fields.path)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let content_val = doc
                .get_first(self.fields.content)
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let snippet = content_val.chars().take(200).collect::<String>();

            results.push(SearchResult {
                path,
                score,
                snippet,
            });
        }

        Ok(results)
    }

    pub fn file_count(&self) -> u64 {
        self.reader.searcher().num_docs()
    }

    pub fn list_indexed_files(&self) -> Vec<String> {
        let searcher = self.reader.searcher();
        let mut files = Vec::new();
        for (segment_ord, segment_reader) in searcher.segment_readers().iter().enumerate() {
            for doc_id in 0..segment_reader.num_docs() {
                if let Ok(doc) = searcher
                    .doc::<TantivyDocument>(tantivy::DocAddress::new(segment_ord as u32, doc_id))
                {
                    if let Some(path) = doc.get_first(self.fields.path).and_then(|v| v.as_str()) {
                        files.push(path.to_string());
                    }
                }
            }
        }
        files
    }

    pub fn needs_full_reindex(&self, repo_files: &HashMap<String, u64>) -> bool {
        let current = self.list_indexed_files();
        let indexed_len = current.len() as u64;
        let repo_len = repo_files.len() as u64;

        if indexed_len == 0 {
            return true;
        }

        if repo_len == 0 {
            return false;
        }

        let diff_ratio = if repo_len > indexed_len {
            (repo_len - indexed_len) as f64 / repo_len as f64
        } else if indexed_len > repo_len {
            (indexed_len - repo_len) as f64 / indexed_len as f64
        } else {
            0.0
        };

        diff_ratio > 0.3
    }
}
