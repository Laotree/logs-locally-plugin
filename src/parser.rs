use crate::db::{Message, Session};
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
                        session.cwd = obj["cwd"].as_str().map(|s| s.to_string());
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
                    let content = extract_content(msg);
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
                        session.title = Some(title.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    // Set timestamps
    session.created_at = first_ts.unwrap_or_else(|| iso_now());
    session.updated_at = last_ts.unwrap_or_else(|| iso_now());

    // Count user messages (excluding attachments)
    let user_msg_count = messages
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .count() as i64;

    session.message_count = user_msg_count;
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
    // Approximate ISO 8601 without chrono dependency on formatting
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Simple ISO format
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Approximate year from days since epoch (rough)
    let year = 1970 + (days as f64 / 365.25) as u64;
    let remaining_days = days as f64 % 365.25;
    let month = 1 + (remaining_days / 30.44) as u64;
    let day = 1 + (remaining_days % 30.44) as u64;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year,
        month.min(12),
        day.min(28),
        hours,
        minutes,
        seconds
    )
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
pub fn import_session(db: &crate::db::Db, jsonl_path: &Path) -> Result<bool> {
    let file_name = jsonl_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    if db.session_exists(file_name)? {
        return Ok(false); // already imported
    }

    let (session, messages) = parse_session_file(jsonl_path)?;
    db.upsert_session(&session)?;
    for msg in &messages {
        db.upsert_message(msg)?;
    }

    Ok(true)
}
