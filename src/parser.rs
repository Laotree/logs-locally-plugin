use crate::db::{Message, Session};
use crate::scrub::scrub_sensitive;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;
use std::time::SystemTime;

/// Parse a single session JSONL file and return session + messages.
pub fn parse_session_file(path: &Path) -> Result<(Session, Vec<Message>)> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading {:?}", path))?;

    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let mut session = Session {
        id: file_name.to_string(),
        title: None,
        model: None,
        created_at: String::new(),
        updated_at: String::new(),
        message_count: 0,
        token_count: 0,
        cwd: None,
        git_branch: None,
        version: None,
        source: "claude".to_string(),
    };

    let mut messages: Vec<Message> = Vec::new();
    let mut first_ts: Option<String> = None;
    let mut last_ts: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let obj: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let event_type = obj["type"].as_str().unwrap_or("");

        match event_type {
            "queue-operation" => continue,
            "user" | "assistant" | "attachment" | "tool" => {
                let timestamp = obj["timestamp"].as_str().unwrap_or("").to_string();
                if first_ts.is_none() || timestamp < *first_ts.as_ref().unwrap() {
                    first_ts = Some(timestamp.clone());
                }
                if last_ts.is_none() || timestamp > *last_ts.as_ref().unwrap() {
                    last_ts = Some(timestamp.clone());
                }

                // Track session metadata from user events
                if event_type == "user" {
                    if session.cwd.is_none() {
                        session.cwd = obj["cwd"].as_str().map(|s| scrub_sensitive(s));
                    }
                    if session.git_branch.is_none() {
                        session.git_branch = obj["gitBranch"].as_str().map(|s| s.to_string());
                    }
                    if session.version.is_none() {
                        session.version = obj["version"].as_str().map(|s| s.to_string());
                    }
                }

                // Process message content
                if let Some(msg) = obj.get("message") {
                    let role = msg["role"].as_str().unwrap_or(event_type).to_string();
                    let content = scrub_sensitive(&extract_content(msg));
                    let uuid = obj["uuid"].as_str().unwrap_or("").to_string();
                    let parent_id = obj["parentUuid"].as_str().map(|s| s.to_string());
                    let model = msg["model"].as_str().map(|s| s.to_string());

                    // Track model from assistant messages
                    if let Some(ref m) = model {
                        session.model = Some(m.clone());
                    }

                    let token_count = msg["usage"]["input_tokens"]
                        .as_i64()
                        .unwrap_or(0)
                        + msg["usage"]["output_tokens"]
                            .as_i64()
                            .unwrap_or(0);

                    let msg_obj = Message {
                        id: uuid,
                        session_id: session.id.clone(),
                        role,
                        content,
                        created_at: timestamp,
                        token_count,
                        parent_id,
                        model,
                    };
                    messages.push(msg_obj);
                }
            }
            "ai-title" => {
                if let Some(title) = obj["aiTitle"].as_str() {
                    if session.title.is_none() || session.title.as_deref() == Some("") {
                        session.title = Some(scrub_sensitive(title));
                    }
                }
            }
            _ => {}
        }
    }

    // Set timestamps
    session.created_at = first_ts.unwrap_or_else(|| iso_now());
    session.updated_at = last_ts.unwrap_or_else(|| iso_now());

    session.message_count = messages.len() as i64;
    session.token_count = messages.iter().map(|m| m.token_count).sum();

    Ok((session, messages))
}

/// Extract text content from a message value.
fn extract_content(msg: &Value) -> String {
    match &msg["content"] {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let mut parts: Vec<String> = Vec::new();
            for item in arr {
                if let Some(text) = item["text"].as_str() {
                    parts.push(text.to_string());
                } else if let Some(text) = item["thinking"].as_str() {
                    parts.push(format!("[thinking]{}[/thinking]", text));
                } else if item["type"].as_str() == Some("tool_result") {
                    if let Some(content) = item["content"].as_array() {
                        let texts: Vec<&str> = content
                            .iter()
                            .filter_map(|c| c["text"].as_str())
                            .collect();
                        if !texts.is_empty() {
                            parts.push(format!("[tool_result]\n{}[/tool_result]", texts.join("\n")));
                        }
                    } else if let Some(text) = item["content"].as_str() {
                        parts.push(format!("[tool_result]\n{}[/tool_result]", text));
                    }
                } else if let Some(name) = item["name"].as_str() {
                    if let Some(input) = item["input"].as_str() {
                        parts.push(format!("[tool: {}]\n{}[/tool]", name, input));
                    } else if let Some(input) = item["input"].as_object() {
                        parts.push(format!(
                            "[tool: {}]\n{}[/tool]",
                            name,
                            serde_json::to_string_pretty(input).unwrap_or_default()
                        ));
                    } else {
                        parts.push(format!("[tool: {}]", name));
                    }
                }
            }
            parts.join("\n\n")
        }
        _ => serde_json::to_string(&msg["content"]).unwrap_or_default(),
    }
}

fn iso_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Find the most recent JSONL file in the project's Claude Code session directory.
pub fn find_latest_session(claude_projects_dir: &Path, project_path: &Path) -> Result<Option<std::path::PathBuf>> {
    let project_dir_name = crate::config::Config::project_dir_name(project_path);
    let sessions_dir = claude_projects_dir.join(&project_dir_name);

    if !sessions_dir.exists() {
        return Ok(None);
    }

    let mut latest: Option<(std::path::PathBuf, SystemTime)> = None;

    for entry in std::fs::read_dir(&sessions_dir).context("reading sessions directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| SystemTime::UNIX_EPOCH);

        match &latest {
            Some((_, latest_time)) if modified <= *latest_time => {}
            _ => {
                latest = Some((path, modified));
            }
        }
    }

    Ok(latest.map(|(path, _)| path))
}

/// Import a session from a single JSONL file into the database.
/// Scores the session immediately after a successful import.
pub fn import_session(db: &crate::db::Db, jsonl_path: &Path) -> Result<bool> {
    let (session, messages) = parse_session_file(jsonl_path)?;
    let imported = db.import_session(&session, &messages)?;
    if imported {
        let score = crate::scorer::score_session(&session, &messages);
        db.upsert_score(&score).context("storing session score")?;
    }
    Ok(imported)
}

// ─── Pi agent session support ─────────────────────────────────────────────────

/// Derive the session subdirectory name that pi uses for a given project path.
/// Pi replaces `/` with `-` and wraps with `--` on both sides.
/// E.g. `/Users/foo/bar` → `--Users-foo-bar--`
fn pi_project_dir_name(project_path: &Path) -> String {
    let canonical = std::fs::canonicalize(project_path)
        .unwrap_or_else(|_| project_path.to_path_buf());
    let path_str = canonical.to_string_lossy();
    let inner = path_str.replace('/', "-");
    let inner = inner.trim_matches('-');
    format!("--{}--", inner)
}

/// Find the most recent pi JSONL session file for `project_path`.
pub fn find_latest_pi_session(pi_jsonl_dir: &Path, project_path: &Path) -> Result<Option<std::path::PathBuf>> {
    let dir_name = pi_project_dir_name(project_path);
    let sessions_dir = pi_jsonl_dir.join(&dir_name);

    if !sessions_dir.exists() {
        return Ok(None);
    }

    let mut latest: Option<(std::path::PathBuf, SystemTime)> = None;

    for entry in std::fs::read_dir(&sessions_dir).context("reading pi sessions directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        match &latest {
            Some((_, t)) if modified <= *t => {}
            _ => latest = Some((path, modified)),
        }
    }

    Ok(latest.map(|(p, _)| p))
}

/// Return all pi JSONL files for `project_path`, sorted by name (chronological).
pub fn list_pi_session_files(pi_jsonl_dir: &Path, project_path: &Path) -> Result<Vec<std::path::PathBuf>> {
    let dir_name = pi_project_dir_name(project_path);
    let sessions_dir = pi_jsonl_dir.join(&dir_name);

    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<_> = std::fs::read_dir(&sessions_dir)
        .context("reading pi sessions directory")?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("jsonl"))
        .collect();

    entries.sort();
    Ok(entries)
}

/// Parse a single pi agent JSONL session file into a `Session` + messages.
pub fn parse_pi_session_file(path: &Path) -> Result<(Session, Vec<Message>)> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading {:?}", path))?;

    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");

    let mut session = Session {
        id: file_stem.to_string(),
        title: None,
        model: None,
        created_at: String::new(),
        updated_at: String::new(),
        message_count: 0,
        token_count: 0,
        cwd: None,
        git_branch: None,
        version: None,
        source: "pi".to_string(),
    };

    let mut messages: Vec<Message> = Vec::new();
    let mut first_ts: Option<String> = None;
    let mut last_ts: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let obj: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        match obj["type"].as_str().unwrap_or("") {
            "session" => {
                if let Some(id) = obj["id"].as_str() {
                    session.id = id.to_string();
                }
                if let Some(ts) = obj["timestamp"].as_str() {
                    first_ts = Some(ts.to_string());
                }
                session.cwd = obj["cwd"].as_str().map(|s| scrub_sensitive(s));
            }
            "model_change" => {
                if let Some(model) = obj["modelId"].as_str() {
                    session.model = Some(model.to_string());
                }
            }
            "message" => {
                let msg = &obj["message"];
                let role = msg["role"].as_str().unwrap_or("").to_string();

                // Skip tool results — their content is context, not a conversation turn.
                if role == "toolResult" {
                    continue;
                }

                let ts = obj["timestamp"].as_str().unwrap_or("").to_string();
                if last_ts.is_none() || ts > *last_ts.as_ref().unwrap() {
                    last_ts = Some(ts.clone());
                }

                let id = obj["id"].as_str().unwrap_or("").to_string();
                let parent_id = obj["parentId"].as_str().map(|s| s.to_string());
                let token_count = msg["usage"]["totalTokens"].as_i64().unwrap_or(0);
                let model = msg["model"].as_str().map(|s| s.to_string());
                let content = scrub_sensitive(&extract_pi_content(msg));

                // Title from first user message.
                if role == "user" && session.title.is_none() {
                    let first_line = content.lines().next().unwrap_or("").trim();
                    if !first_line.is_empty() {
                        let title = if first_line.chars().count() > 60 {
                            let truncated: String = first_line.chars().take(60).collect();
                            format!("{}…", truncated)
                        } else {
                            first_line.to_string()
                        };
                        session.title = Some(title);
                    }
                }

                messages.push(Message {
                    id,
                    session_id: session.id.clone(),
                    role,
                    content,
                    created_at: ts,
                    token_count,
                    parent_id,
                    model,
                });
            }
            _ => {}
        }
    }

    session.created_at = first_ts.unwrap_or_else(iso_now);
    session.updated_at = last_ts.unwrap_or_else(iso_now);
    session.message_count = messages.len() as i64;
    session.token_count = messages.iter().map(|m| m.token_count).sum();

    Ok((session, messages))
}

/// Extract displayable content from a pi message object.
fn extract_pi_content(msg: &Value) -> String {
    match &msg["content"] {
        Value::Array(arr) => {
            let mut parts: Vec<String> = Vec::new();
            for item in arr {
                match item["type"].as_str().unwrap_or("") {
                    "text" => {
                        if let Some(text) = item["text"].as_str() {
                            parts.push(text.to_string());
                        }
                    }
                    "thinking" => {
                        if let Some(text) = item["thinking"].as_str() {
                            parts.push(format!("[thinking]{}[/thinking]", text));
                        }
                    }
                    "toolCall" => {
                        let name = item["name"].as_str().unwrap_or("unknown");
                        if let Some(args) = item["arguments"].as_object() {
                            parts.push(format!(
                                "[tool: {}]\n{}[/tool]",
                                name,
                                serde_json::to_string_pretty(args).unwrap_or_default()
                            ));
                        } else {
                            parts.push(format!("[tool: {}]", name));
                        }
                    }
                    _ => {}
                }
            }
            parts.join("\n\n")
        }
        Value::String(s) => s.clone(),
        _ => String::new(),
    }
}

/// Import a pi session file into the database, scoring it on success.
pub fn import_pi_session(db: &crate::db::Db, jsonl_path: &Path) -> Result<bool> {
    let (session, messages) = parse_pi_session_file(jsonl_path)?;
    let imported = db.import_session(&session, &messages)?;
    if imported {
        let score = crate::scorer::score_session(&session, &messages);
        db.upsert_score(&score).context("storing pi session score")?;
    }
    Ok(imported)
}
