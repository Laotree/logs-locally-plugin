use crate::scorer::Score;
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub id: String,
    pub title: Option<String>,
    pub model: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
    pub token_count: i64,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
    pub token_count: i64,
    pub parent_id: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub total_sessions: i64,
    pub total_messages: i64,
    pub total_tokens: i64,
    pub models: Vec<ModelStat>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelStat {
    pub model: String,
    pub session_count: i64,
    pub message_count: i64,
    pub token_count: i64,
}

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating db directory {:?}", parent))?;
        }
        let conn = Connection::open(path).context("opening SQLite database")?;
        let db = Db {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                title TEXT,
                model TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                message_count INTEGER DEFAULT 0,
                token_count INTEGER DEFAULT 0,
                cwd TEXT,
                git_branch TEXT,
                version TEXT
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL REFERENCES sessions(id),
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                token_count INTEGER DEFAULT 0,
                parent_id TEXT,
                model TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
            CREATE INDEX IF NOT EXISTS idx_messages_created ON messages(created_at);
            CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at);

            CREATE TABLE IF NOT EXISTS scores (
                session_id TEXT PRIMARY KEY REFERENCES sessions(id),
                total_score INTEGER NOT NULL,
                security INTEGER NOT NULL,
                effectivity INTEGER NOT NULL,
                solidity INTEGER NOT NULL,
                efficiency INTEGER NOT NULL,
                planning_quality INTEGER NOT NULL,
                recovery_ability INTEGER NOT NULL,
                hallucination_rate INTEGER NOT NULL,
                grade TEXT NOT NULL,
                scored_at TEXT NOT NULL
            );
            ",
        )
        .context("running migrations")?;
        Ok(())
    }

    fn upsert_session_inner(conn: &Connection, session: &Session) -> Result<()> {
        conn.execute(
            "INSERT INTO sessions (id, title, model, created_at, updated_at, message_count, token_count, cwd, git_branch, version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                title = COALESCE(?2, title),
                model = COALESCE(?3, model),
                updated_at = MAX(updated_at, ?5),
                message_count = ?6,
                token_count = ?7,
                cwd = COALESCE(?8, cwd),
                git_branch = COALESCE(?9, git_branch),
                version = COALESCE(?10, version)",
            params![
                session.id,
                session.title,
                session.model,
                session.created_at,
                session.updated_at,
                session.message_count,
                session.token_count,
                session.cwd,
                session.git_branch,
                session.version,
            ],
        )
        .context("upserting session")?;
        Ok(())
    }

    fn upsert_message_inner(conn: &Connection, msg: &Message) -> Result<()> {
        conn.execute(
            "INSERT INTO messages (id, session_id, role, content, created_at, token_count, parent_id, model)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO NOTHING",
            params![
                msg.id,
                msg.session_id,
                msg.role,
                msg.content,
                msg.created_at,
                msg.token_count,
                msg.parent_id,
                msg.model,
            ],
        )
        .context("upserting message")?;
        Ok(())
    }

    /// Import a session and its messages in a single transaction.
    /// Returns Ok(true) if imported, Ok(false) if already exists.
    /// On failure, everything is rolled back — no partial imports.
    pub fn import_session(&self, session: &Session, messages: &[Message]) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().context("starting transaction")?;

        let exists: bool = tx
            .query_row(
                "SELECT COUNT(*) > 0 FROM sessions WHERE id = ?1",
                params![session.id],
                |row| row.get(0),
            )
            .context("checking session existence")?;

        if exists {
            return Ok(false);
        }

        Db::upsert_session_inner(&tx, session)?;
        for msg in messages {
            Db::upsert_message_inner(&tx, msg)?;
        }

        tx.commit().context("committing transaction")?;
        Ok(true)
    }

    pub fn upsert_score(&self, score: &Score) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO scores (session_id, total_score, security, effectivity, solidity, efficiency, planning_quality, recovery_ability, hallucination_rate, grade, scored_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(session_id) DO UPDATE SET
                total_score = ?2, security = ?3, effectivity = ?4, solidity = ?5,
                efficiency = ?6, planning_quality = ?7, recovery_ability = ?8,
                hallucination_rate = ?9, grade = ?10, scored_at = ?11",
            params![
                score.session_id,
                score.total_score,
                score.security,
                score.effectivity,
                score.solidity,
                score.efficiency,
                score.planning_quality,
                score.recovery_ability,
                score.hallucination_rate,
                score.grade,
                score.scored_at,
            ],
        )
        .context("upserting score")?;
        Ok(())
    }

    pub fn get_score(&self, session_id: &str) -> Result<Option<Score>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT session_id, total_score, security, effectivity, solidity, efficiency,
                        planning_quality, recovery_ability, hallucination_rate, grade, scored_at
                 FROM scores WHERE session_id = ?1",
            )
            .context("preparing get score query")?;

        let mut rows = stmt
            .query_map(params![session_id], |row| {
                Ok(Score {
                    session_id: row.get(0)?,
                    total_score: row.get(1)?,
                    security: row.get(2)?,
                    effectivity: row.get(3)?,
                    solidity: row.get(4)?,
                    efficiency: row.get(5)?,
                    planning_quality: row.get(6)?,
                    recovery_ability: row.get(7)?,
                    hallucination_rate: row.get(8)?,
                    grade: row.get(9)?,
                    scored_at: row.get(10)?,
                })
            })
            .context("querying score")?;

        match rows.next() {
            Some(Ok(score)) => Ok(Some(score)),
            _ => Ok(None),
        }
    }

    /// Fetch scores for a batch of session IDs in one query.
    pub fn get_scores_for_sessions(
        &self,
        session_ids: &[String],
    ) -> Result<HashMap<String, Score>> {
        if session_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = self.conn.lock().unwrap();
        let placeholders = session_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT session_id, total_score, security, effectivity, solidity, efficiency,
                    planning_quality, recovery_ability, hallucination_rate, grade, scored_at
             FROM scores WHERE session_id IN ({})",
            placeholders
        );
        let mut stmt = conn.prepare(&sql).context("preparing batch scores query")?;
        let param_values: Vec<Box<dyn rusqlite::types::ToSql>> = session_ids
            .iter()
            .map(|s| Box::new(s.clone()) as Box<dyn rusqlite::types::ToSql>)
            .collect();
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let scores = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(Score {
                    session_id: row.get(0)?,
                    total_score: row.get(1)?,
                    security: row.get(2)?,
                    effectivity: row.get(3)?,
                    solidity: row.get(4)?,
                    efficiency: row.get(5)?,
                    planning_quality: row.get(6)?,
                    recovery_ability: row.get(7)?,
                    hallucination_rate: row.get(8)?,
                    grade: row.get(9)?,
                    scored_at: row.get(10)?,
                })
            })
            .context("querying batch scores")?
            .collect::<Result<Vec<_>, _>>()
            .context("collecting batch scores")?;

        Ok(scores.into_iter().map(|s| (s.session_id.clone(), s)).collect())
    }

    pub fn list_sessions(
        &self,
        model_filter: Option<&str>,
        since: Option<&str>,
        keyword: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut sql = String::from(
            "SELECT s.id, s.title, s.model, s.created_at, s.updated_at, s.message_count, s.token_count, s.cwd, s.git_branch, s.version FROM sessions s WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(model) = model_filter {
            sql.push_str(" AND s.model = ?");
            param_values.push(Box::new(model.to_string()));
        }
        if let Some(since) = since {
            sql.push_str(" AND s.updated_at >= ?");
            param_values.push(Box::new(since.to_string()));
        }
        if let Some(keyword) = keyword {
            sql.push_str(" AND (s.id IN (SELECT DISTINCT session_id FROM messages WHERE content LIKE ?) OR s.title LIKE ? OR s.id LIKE ?)");
            let pattern = format!("%{}%", keyword);
            param_values.push(Box::new(pattern.clone()));
            param_values.push(Box::new(pattern.clone()));
            param_values.push(Box::new(pattern));
        }

        sql.push_str(" ORDER BY updated_at DESC LIMIT ? OFFSET ?");
        param_values.push(Box::new(limit));
        param_values.push(Box::new(offset));

        let mut stmt = conn.prepare(&sql).context("preparing list sessions query")?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let sessions = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(Session {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    model: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    message_count: row.get(5)?,
                    token_count: row.get(6)?,
                    cwd: row.get(7)?,
                    git_branch: row.get(8)?,
                    version: row.get(9)?,
                })
            })
            .context("querying sessions")?
            .collect::<Result<Vec<_>, _>>()
            .context("collecting sessions")?;

        Ok(sessions)
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, title, model, created_at, updated_at, message_count, token_count, cwd, git_branch, version
                 FROM sessions WHERE id = ?1",
            )
            .context("preparing get session query")?;

        let mut rows = stmt
            .query_map(params![id], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    model: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    message_count: row.get(5)?,
                    token_count: row.get(6)?,
                    cwd: row.get(7)?,
                    git_branch: row.get(8)?,
                    version: row.get(9)?,
                })
            })
            .context("querying session")?;

        match rows.next() {
            Some(Ok(session)) => Ok(Some(session)),
            _ => Ok(None),
        }
    }

    pub fn get_messages(&self, session_id: &str) -> Result<Vec<Message>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, session_id, role, content, created_at, token_count, parent_id, model
                 FROM messages WHERE session_id = ?1
                 ORDER BY created_at ASC",
            )
            .context("preparing get messages query")?;

        let messages = stmt
            .query_map(params![session_id], |row| {
                Ok(Message {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    created_at: row.get(4)?,
                    token_count: row.get(5)?,
                    parent_id: row.get(6)?,
                    model: row.get(7)?,
                })
            })
            .context("querying messages")?
            .collect::<Result<Vec<_>, _>>()
            .context("collecting messages")?;

        Ok(messages)
    }

    pub fn get_stats(&self) -> Result<Stats> {
        let conn = self.conn.lock().unwrap();

        let (total_sessions, total_messages, total_tokens): (i64, i64, i64) = conn
            .query_row(
                "SELECT COUNT(*) as sessions, COALESCE(SUM(message_count),0) as msgs, COALESCE(SUM(token_count),0) as tokens FROM sessions",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .context("querying stats")?;

        let mut stmt = conn
            .prepare(
                "SELECT COALESCE(model, 'unknown') as model,
                        COUNT(*) as session_count,
                        COALESCE(SUM(message_count),0) as msg_count,
                        COALESCE(SUM(token_count),0) as token_count
                 FROM sessions
                 GROUP BY model
                 ORDER BY token_count DESC",
            )
            .context("preparing model stats query")?;

        let models = stmt
            .query_map([], |row| {
                Ok(ModelStat {
                    model: row.get(0)?,
                    session_count: row.get(1)?,
                    message_count: row.get(2)?,
                    token_count: row.get(3)?,
                })
            })
            .context("querying model stats")?
            .collect::<Result<Vec<_>, _>>()
            .context("collecting model stats")?;

        Ok(Stats {
            total_sessions,
            total_messages,
            total_tokens,
            models,
        })
    }
}
