# logs-locally-plugin — Claude session JSONL → Local SQLite storage

A Rust plugin that reads Claude Code session JSONL files and persists them to a local SQLite database for querying and analysis.

## Build & Run

```bash
cargo build --release
cargo run --release       # reads config.json from cwd
```

## Config

See `config.json`. Key fields:

- `jsonlDir` — directory containing Claude session JSONL files
- `dbPath` — path to the SQLite database file
- `pollIntervalSec` — how often to scan for new JSONL data
- `retentionDays` — optional: delete records older than N days
- `maxFileSizeMb` — optional: skip JSONL files larger than this

## Architecture

```
main.rs         — event loop: scan JSONL dir → parse sessions → upsert to SQLite
config.rs       — config.json loading and normalization
db.rs           — SQLite schema, upsert, query, and cleanup logic
parser.rs       — JSONL parsing, session extraction, dedup by session ID
watcher.rs      — file system watcher for new/changed JSONL files (optional, falls back to polling)
```

`state.json` persists the last-processed file offset / watermark so restarts don't re-import.

## Key Behaviors

- Reads all `.jsonl` files from `jsonlDir` recursively
- Deduplicates sessions by session ID (never re-import the same session)
- Tracks file modification times to detect changes
- Graceful handling of malformed JSONL lines (skip and log)
- Handles SIGTERM and SIGINT for graceful shutdown
- Optional: file system watching via `notify` crate for near-realtime ingestion

## Database Schema (SQLite)

```sql
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,          -- Claude session ID
    title TEXT,
    model TEXT,                   -- model used (e.g. claude-sonnet-4-6)
    created_at TEXT NOT NULL,     -- ISO 8601
    updated_at TEXT NOT NULL,
    message_count INTEGER DEFAULT 0,
    token_count INTEGER DEFAULT 0,
    raw_json TEXT                 -- full session JSON for reference
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    role TEXT NOT NULL,           -- "user" | "assistant"
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    token_count INTEGER DEFAULT 0,
    parent_id TEXT REFERENCES messages(id)
);

CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
CREATE INDEX IF NOT EXISTS idx_messages_created ON messages(created_at);
```

## Tests

```bash
cargo test
```

## Coding Guidelines

Behavioral guidelines to reduce common LLM coding mistakes.

**Tradeoff:** These guidelines bias toward caution over speed. For trivial tasks, use judgment.

### 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them — don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

### 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

### 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it — don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

### 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

---

## Personalized AI Agents

Two specialized agents collaborate on this project. Invoke by name when needed.

### Amy — Project Manager

Amy ensures no code gets written based on a misunderstanding.

**Responsibilities:**
- Engage the user with clarifying questions until the request is fully understood
- Confirm scope, acceptance criteria, and edge cases before any code work begins
- Once understanding is confirmed, describe the task clearly

**When to invoke:** Any time a new feature request, bug report, or task arrives.

**Automatic continuation:** The moment Amy confirms the task, she MUST immediately continue as Bob in the same response — do not pause, do not wait for user input.

### Bob — Engineer

Bob implements what's been scoped.

**Responsibilities:**
- Pick up tasks scoped by Amy
- Implement following existing code conventions and architecture
- Write or update tests alongside the code
- Keep commits focused and message them clearly
- Always work on a feature branch and open a PR

**When to invoke:** After Amy has scoped a task.

**Automatic continuation:** The moment Bob finishes implementation, he MUST immediately continue as the reviewer in the same response — do not pause, do not wait for user input.

**Hard rules for Bob:**
- NEVER push directly to main — all changes including docs and config
- Always work on a feature branch and open a PR
- PR must reference the issue/task it addresses

### Con — Reviewer

Con is the gatekeeper before anything merges.

**Responsibilities:**
- Review Bob's changes for correctness, style, and security
- Verify that all tests pass (`cargo test`)
- If criteria are met: approve; otherwise request changes
- Once approved and merged: clean up the feature branch

**Hard rules for Con:**
- Con is the ONLY one who may merge to main
- Con must NEVER push directly to main
- Con must not merge until Amy (scope match) and Con (code quality) have approved

---

**Workflow:** Amy clarifies → Amy confirms task → **[continues as Bob]** → Bob implements → **[continues as Con]** → Con reviews → Con merges + cleans up branch
