use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct SessionStore {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub id: String,
    pub model: String,
    pub working_dir: String,
    pub message_count: usize,
    pub total_tokens: u64,
    pub created_at: String,
    pub updated_at: String,
}

impl SessionStore {
    pub fn new(db_path: &PathBuf) -> anyhow::Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                model TEXT NOT NULL,
                working_dir TEXT NOT NULL,
                messages_json TEXT NOT NULL DEFAULT '[]',
                total_tokens INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn save(
        &self,
        id: &str,
        model: &str,
        working_dir: &str,
        messages: &[crate::session::SessionMessage],
        total_tokens: u64,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        let messages_json = serde_json::to_string(messages)?;
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO sessions (id, model, working_dir, messages_json, total_tokens, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET
                messages_json = excluded.messages_json,
                total_tokens = excluded.total_tokens,
                model = excluded.model,
                updated_at = excluded.updated_at",
            params![id, model, working_dir, messages_json, total_tokens, now],
        )?;

        Ok(())
    }

    pub fn load(&self, id: &str) -> anyhow::Result<Option<crate::session::Session>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, model, working_dir, messages_json, total_tokens, created_at, updated_at
             FROM sessions WHERE id = ?1",
        )?;

        let row = stmt.query_row(params![id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, u64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        });

        match row {
            Ok((id, model, working_dir, messages_json, total_tokens, created_at, updated_at)) => {
                let messages: Vec<crate::session::SessionMessage> =
                    serde_json::from_str(&messages_json)?;
                Ok(Some(crate::session::Session {
                    id,
                    working_dir,
                    model,
                    messages,
                    total_tokens,
                    created_at,
                    updated_at,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn list(&self) -> anyhow::Result<Vec<SessionMeta>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, model, working_dir, total_tokens, created_at, updated_at,
                    length(messages_json) - length(replace(messages_json, '\"role\"', '')) as msg_count
             FROM sessions ORDER BY updated_at DESC LIMIT 50",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(SessionMeta {
                id: row.get(0)?,
                model: row.get(1)?,
                working_dir: row.get(2)?,
                total_tokens: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                message_count: (row.get::<_, usize>(6)? / 5).min(999),
            })
        })?;

        let mut metas = Vec::new();
        for row in rows {
            metas.push(row?);
        }
        Ok(metas)
    }

    pub fn delete(&self, id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(())
    }
}
