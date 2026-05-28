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
