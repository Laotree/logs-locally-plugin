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
        if let Some(score) = crate::scorer::score_session(&session, &messages) {
            db.upsert_score(&score).context("storing session score")?;
        }
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

/// Return all pi JSONL files across every project under `pi_jsonl_dir`, sorted by path.
pub fn list_all_pi_session_files(pi_jsonl_dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    if !pi_jsonl_dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for project in std::fs::read_dir(pi_jsonl_dir)
        .context("reading pi dir")?
        .filter_map(|e| e.ok())
    {
        let project_path = project.path();
        if !project_path.is_dir() {
            continue;
        }
        for session in std::fs::read_dir(&project_path)
            .context("reading pi project dir")?
            .filter_map(|e| e.ok())
        {
            let path = session.path();
            if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                entries.push(path);
            }
        }
    }
    entries.sort();
    Ok(entries)
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
        if let Some(score) = crate::scorer::score_session(&session, &messages) {
            db.upsert_score(&score).context("storing pi session score")?;
        }
    }
    Ok(imported)
}

// ─── Codex CLI session support ────────────────────────────────────────────────

/// Find the most recently modified Codex JSONL file under the date-structured sessions dir.
pub fn find_latest_codex_session(codex_sessions_dir: &Path) -> Result<Option<std::path::PathBuf>> {
    let mut latest: Option<(std::path::PathBuf, SystemTime)> = None;

    fn walk(dir: &Path, latest: &mut Option<(std::path::PathBuf, SystemTime)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, latest);
            } else if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                let modified = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                match latest {
                    Some((_, t)) if modified <= *t => {}
                    _ => *latest = Some((path, modified)),
                }
            }
        }
    }

    walk(codex_sessions_dir, &mut latest);
    Ok(latest.map(|(p, _)| p))
}

/// List all Codex JSONL files under the date-structured sessions dir, sorted by path.
pub fn list_codex_session_files(codex_sessions_dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files: Vec<std::path::PathBuf> = Vec::new();

    fn walk(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                files.push(path);
            }
        }
    }

    walk(codex_sessions_dir, &mut files);
    files.sort();
    Ok(files)
}

/// Parse a single Codex CLI JSONL session file into a `Session` + messages.
pub fn parse_codex_session_file(path: &Path) -> Result<(Session, Vec<Message>)> {
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
        source: "codex".to_string(),
    };

    let mut messages: Vec<Message> = Vec::new();
    let mut first_ts: Option<String> = None;
    let mut last_ts: Option<String> = None;

    // Per-turn state for aggregating assistant content
    let mut current_turn_id: Option<String> = None;
    let mut assistant_parts: Vec<String> = Vec::new();
    let mut user_msg_id: Option<String> = None;
    let mut turn_ts: String = String::new();

    let flush_assistant = |turn_id: &str,
                           parts: &mut Vec<String>,
                           user_id: &Option<String>,
                           ts: &str,
                           session_id: &str,
                           messages: &mut Vec<Message>| {
        if parts.is_empty() {
            return;
        }
        let content = parts.join("\n\n");
        parts.clear();
        messages.push(Message {
            id: format!("{}-assistant", turn_id),
            session_id: session_id.to_string(),
            role: "assistant".to_string(),
            content,
            created_at: ts.to_string(),
            token_count: 0,
            parent_id: user_id.clone(),
            model: None,
        });
    };

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let obj: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let ts = obj["timestamp"].as_str().unwrap_or("").to_string();
        if !ts.is_empty() {
            if first_ts.is_none() {
                first_ts = Some(ts.clone());
            }
            if last_ts.is_none() || ts > *last_ts.as_ref().unwrap() {
                last_ts = Some(ts.clone());
            }
        }

        match obj["type"].as_str().unwrap_or("") {
            "session_meta" => {
                let payload = &obj["payload"];
                if let Some(id) = payload["id"].as_str() {
                    session.id = id.to_string();
                }
                if let Some(created) = payload["timestamp"].as_str() {
                    first_ts = Some(created.to_string());
                }
                session.cwd = payload["cwd"].as_str().map(|s| scrub_sensitive(s));
                session.git_branch = payload["git"]["branch"].as_str().map(|s| s.to_string());
                session.version = payload["cli_version"].as_str().map(|s| s.to_string());
                if let Some(provider) = payload["model_provider"].as_str() {
                    session.model = Some(provider.to_string());
                }
            }
            "event_msg" => {
                let payload = &obj["payload"];
                match payload["type"].as_str().unwrap_or("") {
                    "task_started" => {
                        // Flush previous turn's assistant content before starting new turn
                        if let Some(ref tid) = current_turn_id.clone() {
                            flush_assistant(tid, &mut assistant_parts, &user_msg_id, &turn_ts, &session.id, &mut messages);
                        }
                        current_turn_id = payload["turn_id"].as_str().map(|s| s.to_string());
                        user_msg_id = None;
                        turn_ts = ts.clone();
                    }
                    "user_message" => {
                        let text = payload["message"].as_str().unwrap_or("").to_string();
                        if text.is_empty() {
                            continue;
                        }
                        let msg_id = current_turn_id
                            .as_deref()
                            .map(|t| format!("{}-user", t))
                            .unwrap_or_else(|| format!("{}-user", file_stem));

                        // Title from first user message
                        if session.title.is_none() {
                            let first_line = text.lines().next().unwrap_or("").trim();
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

                        user_msg_id = Some(msg_id.clone());
                        messages.push(Message {
                            id: msg_id,
                            session_id: session.id.clone(),
                            role: "user".to_string(),
                            content: scrub_sensitive(&text),
                            created_at: ts.clone(),
                            token_count: 0,
                            parent_id: None,
                            model: None,
                        });
                    }
                    "token_count" => {
                        let total = payload["info"]["total_token_usage"]["total_tokens"]
                            .as_i64()
                            .unwrap_or(0);
                        if total > session.token_count {
                            session.token_count = total;
                        }
                    }
                    "task_complete" => {
                        if let Some(ref tid) = current_turn_id.clone() {
                            flush_assistant(tid, &mut assistant_parts, &user_msg_id, &ts, &session.id, &mut messages);
                        }
                        current_turn_id = None;
                    }
                    _ => {}
                }
            }
            "response_item" => {
                let payload = &obj["payload"];
                match payload["type"].as_str().unwrap_or("") {
                    "reasoning" => {
                        // Aggregate reasoning summary text into assistant content
                        if let Some(summaries) = payload["summary"].as_array() {
                            let text: Vec<&str> = summaries
                                .iter()
                                .filter_map(|s| s["text"].as_str())
                                .collect();
                            if !text.is_empty() {
                                assistant_parts.push(format!("[thinking]{}\n[/thinking]", text.join(" ")));
                            }
                        }
                    }
                    "function_call" => {
                        let name = payload["name"].as_str().unwrap_or("unknown");
                        if let Ok(args) = serde_json::from_str::<Value>(
                            payload["arguments"].as_str().unwrap_or("{}"),
                        ) {
                            assistant_parts.push(format!(
                                "[tool: {}]\n{}[/tool]",
                                name,
                                serde_json::to_string_pretty(&args).unwrap_or_default()
                            ));
                        } else {
                            assistant_parts.push(format!("[tool: {}]", name));
                        }
                    }
                    "function_call_output" => {
                        let output = payload["output"].as_str().unwrap_or("").trim();
                        if !output.is_empty() {
                            assistant_parts.push(format!("[tool_result]\n{}[/tool_result]", output));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // Flush any remaining assistant content from the last turn
    if let Some(ref tid) = current_turn_id.clone() {
        flush_assistant(tid, &mut assistant_parts, &user_msg_id, &last_ts.as_deref().unwrap_or(""), &session.id, &mut messages);
    }

    session.created_at = first_ts.unwrap_or_else(iso_now);
    session.updated_at = last_ts.unwrap_or_else(iso_now);
    session.message_count = messages.len() as i64;

    Ok((session, messages))
}

/// Import a Codex session file into the database, scoring it on success.
pub fn import_codex_session(db: &crate::db::Db, jsonl_path: &Path) -> Result<bool> {
    let (session, messages) = parse_codex_session_file(jsonl_path)?;
    let imported = db.import_session(&session, &messages)?;
    if imported {
        if let Some(score) = crate::scorer::score_session(&session, &messages) {
            db.upsert_score(&score).context("storing codex session score")?;
        }
    }
    Ok(imported)
}

// ─── opencode session support ─────────────────────────────────────────────────
//
// opencode (v1.x) splits a session across three storage subdirectories:
//   storage/session/<projectID>/<sessionID>.json   — session info (title, directory, times)
//   storage/message/<sessionID>/<messageID>.json   — message metadata (role, times, tokens)
//   storage/part/<messageID>/<partID>.json         — content parts (text, reasoning, tool)
// Timestamps are epoch milliseconds.

fn epoch_ms_to_iso(ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.to_rfc3339())
}

/// Find the most recently modified opencode session info JSON across all projects.
pub fn find_latest_opencode_session(storage_dir: &Path) -> Result<Option<std::path::PathBuf>> {
    let mut latest: Option<(std::path::PathBuf, SystemTime)> = None;
    for path in list_opencode_session_files(storage_dir)? {
        let modified = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        match &latest {
            Some((_, t)) if modified <= *t => {}
            _ => latest = Some((path, modified)),
        }
    }
    Ok(latest.map(|(p, _)| p))
}

/// List all opencode session info JSONs across every project, sorted by path.
pub fn list_opencode_session_files(storage_dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let sessions_dir = storage_dir.join("session");
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for project in std::fs::read_dir(&sessions_dir)
        .context("reading opencode session dir")?
        .filter_map(|e| e.ok())
    {
        let project_path = project.path();
        if !project_path.is_dir() {
            continue;
        }
        for session in std::fs::read_dir(&project_path)
            .context("reading opencode project session dir")?
            .filter_map(|e| e.ok())
        {
            let path = session.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Parse an opencode session from its session info JSON into a `Session` + messages.
/// `info_path` is `<storage>/session/<projectID>/<sessionID>.json`; messages and
/// parts are read from the sibling `message/` and `part/` directories.
pub fn parse_opencode_session_file(info_path: &Path) -> Result<(Session, Vec<Message>)> {
    let info: Value = serde_json::from_str(
        &std::fs::read_to_string(info_path).with_context(|| format!("reading {:?}", info_path))?,
    )
    .with_context(|| format!("parsing {:?}", info_path))?;

    let storage_dir = info_path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .context("resolving opencode storage dir from session path")?;

    let session_id = info["id"]
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| info_path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string());

    let mut session = Session {
        id: session_id.clone(),
        title: info["title"].as_str().map(|s| scrub_sensitive(s)),
        model: None,
        created_at: info["time"]["created"]
            .as_i64()
            .and_then(epoch_ms_to_iso)
            .unwrap_or_else(iso_now),
        updated_at: info["time"]["updated"]
            .as_i64()
            .and_then(epoch_ms_to_iso)
            .unwrap_or_else(iso_now),
        message_count: 0,
        token_count: 0,
        cwd: info["directory"].as_str().map(|s| scrub_sensitive(s)),
        git_branch: None,
        version: info["version"].as_str().map(|s| s.to_string()),
        source: "opencode".to_string(),
    };

    let mut messages: Vec<Message> = Vec::new();
    let messages_dir = storage_dir.join("message").join(&session_id);

    let mut msg_files: Vec<std::path::PathBuf> = match std::fs::read_dir(&messages_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
            .collect(),
        Err(_) => Vec::new(),
    };
    // Message IDs are monotonic, so name order is chronological.
    msg_files.sort();

    for msg_path in &msg_files {
        let Ok(content) = std::fs::read_to_string(msg_path) else { continue };
        let Ok(msg) = serde_json::from_str::<Value>(&content) else { continue };

        let id = msg["id"].as_str().unwrap_or("").to_string();
        if id.is_empty() {
            continue;
        }
        let role = msg["role"].as_str().unwrap_or("").to_string();
        let created_at = msg["time"]["created"]
            .as_i64()
            .and_then(epoch_ms_to_iso)
            .unwrap_or_default();
        let parent_id = msg["parentID"].as_str().map(|s| s.to_string());
        let model = msg["modelID"].as_str().map(|s| s.to_string());
        let token_count = msg["tokens"]["input"].as_i64().unwrap_or(0)
            + msg["tokens"]["output"].as_i64().unwrap_or(0);

        if role == "assistant" {
            if let Some(ref m) = model {
                session.model = Some(m.clone());
            }
        }

        let content = scrub_sensitive(&read_opencode_parts(storage_dir, &id));

        messages.push(Message {
            id,
            session_id: session.id.clone(),
            role,
            content,
            created_at,
            token_count,
            parent_id,
            model,
        });
    }

    // Title fallback: first line of the first user message.
    if session.title.as_deref().is_none_or(|t| t.is_empty()) {
        if let Some(user_msg) = messages.iter().find(|m| m.role == "user") {
            let first_line = user_msg.content.lines().next().unwrap_or("").trim();
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
    }

    session.message_count = messages.len() as i64;
    session.token_count = messages.iter().map(|m| m.token_count).sum();

    Ok((session, messages))
}

/// Read and join the content parts of an opencode message.
fn read_opencode_parts(storage_dir: &Path, message_id: &str) -> String {
    let parts_dir = storage_dir.join("part").join(message_id);
    let mut part_files: Vec<std::path::PathBuf> = match std::fs::read_dir(&parts_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
            .collect(),
        Err(_) => return String::new(),
    };
    part_files.sort();

    let mut parts: Vec<String> = Vec::new();
    for part_path in &part_files {
        let Ok(content) = std::fs::read_to_string(part_path) else { continue };
        let Ok(part) = serde_json::from_str::<Value>(&content) else { continue };

        match part["type"].as_str().unwrap_or("") {
            "text" => {
                let text = part["text"].as_str().unwrap_or("").trim();
                if !text.is_empty() {
                    parts.push(text.to_string());
                }
            }
            "reasoning" => {
                let text = part["text"].as_str().unwrap_or("").trim();
                if !text.is_empty() {
                    parts.push(format!("[thinking]{}[/thinking]", text));
                }
            }
            "tool" => {
                let name = part["tool"].as_str().unwrap_or("unknown");
                if let Some(input) = part["state"]["input"].as_object() {
                    parts.push(format!(
                        "[tool: {}]\n{}[/tool]",
                        name,
                        serde_json::to_string_pretty(input).unwrap_or_default()
                    ));
                } else {
                    parts.push(format!("[tool: {}]", name));
                }
                let output = part["state"]["output"].as_str().unwrap_or("").trim();
                if !output.is_empty() {
                    parts.push(format!("[tool_result]\n{}[/tool_result]", output));
                }
            }
            _ => {}
        }
    }
    parts.join("\n\n")
}

/// Import an opencode session into the database, scoring it on success.
pub fn import_opencode_session(db: &crate::db::Db, info_path: &Path) -> Result<bool> {
    let (session, messages) = parse_opencode_session_file(info_path)?;
    let imported = db.import_session(&session, &messages)?;
    if imported {
        if let Some(score) = crate::scorer::score_session(&session, &messages) {
            db.upsert_score(&score).context("storing opencode session score")?;
        }
    }
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal opencode storage tree in a unique temp dir.
    fn write_opencode_fixture() -> std::path::PathBuf {
        let storage = std::env::temp_dir().join(format!(
            "llp-opencode-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let sid = "ses_test0001";
        let session_dir = storage.join("session").join("proj1");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(
            session_dir.join(format!("{}.json", sid)),
            r#"{
              "id": "ses_test0001",
              "version": "1.14.50",
              "projectID": "proj1",
              "directory": "/tmp/myproject",
              "title": "Fix the build",
              "time": { "created": 1770362255662, "updated": 1770362387759 }
            }"#,
        )
        .unwrap();

        let msg_dir = storage.join("message").join(sid);
        std::fs::create_dir_all(&msg_dir).unwrap();
        std::fs::write(
            msg_dir.join("msg_a.json"),
            r#"{
              "id": "msg_a",
              "sessionID": "ses_test0001",
              "role": "user",
              "time": { "created": 1770362255676 }
            }"#,
        )
        .unwrap();
        std::fs::write(
            msg_dir.join("msg_b.json"),
            r#"{
              "id": "msg_b",
              "sessionID": "ses_test0001",
              "role": "assistant",
              "parentID": "msg_a",
              "modelID": "big-pickle",
              "providerID": "opencode",
              "time": { "created": 1770362255688, "completed": 1770362260790 },
              "tokens": { "input": 100, "output": 50, "reasoning": 1, "cache": { "read": 0, "write": 0 } }
            }"#,
        )
        .unwrap();

        let part_a = storage.join("part").join("msg_a");
        std::fs::create_dir_all(&part_a).unwrap();
        std::fs::write(
            part_a.join("prt_1.json"),
            r#"{ "id": "prt_1", "messageID": "msg_a", "type": "text", "text": "please fix the build" }"#,
        )
        .unwrap();

        let part_b = storage.join("part").join("msg_b");
        std::fs::create_dir_all(&part_b).unwrap();
        std::fs::write(
            part_b.join("prt_2.json"),
            r#"{ "id": "prt_2", "messageID": "msg_b", "type": "step-start", "snapshot": "abc" }"#,
        )
        .unwrap();
        std::fs::write(
            part_b.join("prt_3.json"),
            r#"{ "id": "prt_3", "messageID": "msg_b", "type": "reasoning", "text": "look at the Makefile" }"#,
        )
        .unwrap();
        std::fs::write(
            part_b.join("prt_4.json"),
            r#"{ "id": "prt_4", "messageID": "msg_b", "type": "tool", "callID": "c1", "tool": "bash",
                 "state": { "status": "completed", "input": { "command": "make" }, "output": "ok" } }"#,
        )
        .unwrap();
        std::fs::write(
            part_b.join("prt_5.json"),
            r#"{ "id": "prt_5", "messageID": "msg_b", "type": "text", "text": "Done, the build passes." }"#,
        )
        .unwrap();

        storage
    }

    #[test]
    fn parses_opencode_session() {
        let storage = write_opencode_fixture();
        let info_path = storage.join("session").join("proj1").join("ses_test0001.json");

        let (session, messages) = parse_opencode_session_file(&info_path).unwrap();

        assert_eq!(session.id, "ses_test0001");
        assert_eq!(session.source, "opencode");
        assert_eq!(session.title.as_deref(), Some("Fix the build"));
        assert_eq!(session.cwd.as_deref(), Some("/tmp/myproject"));
        assert_eq!(session.version.as_deref(), Some("1.14.50"));
        assert_eq!(session.model.as_deref(), Some("big-pickle"));
        assert_eq!(session.message_count, 2);
        assert_eq!(session.token_count, 150);
        assert!(session.created_at.starts_with("2026-"));

        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "please fix the build");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].parent_id.as_deref(), Some("msg_a"));
        assert_eq!(messages[1].token_count, 150);
        assert!(messages[1].content.contains("[thinking]look at the Makefile[/thinking]"));
        assert!(messages[1].content.contains("[tool: bash]"));
        assert!(messages[1].content.contains("[tool_result]\nok[/tool_result]"));
        assert!(messages[1].content.contains("Done, the build passes."));
        assert!(!messages[1].content.contains("step-start"));

        std::fs::remove_dir_all(&storage).ok();
    }

    #[test]
    fn lists_and_finds_opencode_sessions() {
        let storage = write_opencode_fixture();

        let files = list_opencode_session_files(&storage).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("ses_test0001.json"));

        let latest = find_latest_opencode_session(&storage).unwrap();
        assert_eq!(latest, Some(files[0].clone()));

        // A missing storage dir yields empty results, not an error.
        let missing = storage.join("does-not-exist");
        assert!(list_opencode_session_files(&missing).unwrap().is_empty());
        assert!(find_latest_opencode_session(&missing).unwrap().is_none());

        std::fs::remove_dir_all(&storage).ok();
    }
}
