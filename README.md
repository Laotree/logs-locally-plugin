# Logs Locally Plugin

Store and browse Claude Code session logs in a local SQLite database with a built-in web UI.

```
llp import       # save the latest session to SQLite
llp serve        # start the web log browser
llp import-all   # bulk import all historical sessions for a project
```

## How It Works

Each time Claude Code exits, the `Stop` hook triggers `llp import`, which:

1. Finds the most recent `.jsonl` session file in `~/.claude/projects/<project>/`
2. Parses messages, models, token usage, and session metadata
3. Upserts into a local SQLite database (deduplicated by session ID)

The `serve` command starts a web UI at `http://127.0.0.1:8484` for browsing and searching sessions.

## Installation

### Option 1: Install from source

```bash
cargo install --git https://github.com/raypar/logs-locally-plugin
```

### Option 2: Build locally

```bash
cargo build --release
cp target/release/llp ~/.local/bin/llp
```

> Make sure `~/.local/bin` is in your `PATH`.

### Option 3: Install as a Claude Code plugin (marketplace)

Once published to the community marketplace:

```
/plugin install logs-locally-plugin@claude-community
```

The plugin provides the `Stop` hook and a `/logs-locally-plugin:serve` skill automatically. After installing, run:

```
/plugin enable logs-locally-plugin@claude-community
/reload-plugins
```

## Configuration

### Claude Code Hook (auto-import on exit)

```bash
mkdir -p ~/.claude
```

Add to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "llp import"
          }
        ]
      }
    ]
  }
}
```

### Optional: config.json

Create `config.json` in the working directory or wherever you run `llp`:

```json
{
  "db_path": "~/.local/share/logs-locally-plugin/logs.db",
  "claude_projects_dir": "~/.claude/projects",
  "host": "127.0.0.1",
  "port": 8484
}
```

All fields are optional — defaults are shown above.

## Usage

### Import the latest session

```bash
cd /path/to/your/project
llp import
```

This auto-detects the Claude Code project from the current working directory and imports the most recent session.

### Import all historical sessions

```bash
llp import-all /path/to/your/project
```

### Browse logs

```bash
llp serve
# Open http://127.0.0.1:8484
```

Features:
- Session list with search and filters (by model, time range, keyword)
- Message detail view with thinking blocks and tool calls
- Live auto-refresh (10s polling)
- Dark theme, Claude web-inspired design

### Command reference

```
Usage: llp [OPTIONS] <COMMAND>

Commands:
  import       Import the latest Claude Code session into SQLite
  serve        Start the local web server for browsing logs
  import-all   Import all existing sessions from a project
  help         Print help
```

## Build

```bash
cargo build --release
```

## Database

Sessions and messages are stored in a SQLite database at `~/.local/share/logs-locally-plugin/logs.db` (configurable).

```sql
-- Sessions table
CREATE TABLE sessions (
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

-- Messages table
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    token_count INTEGER DEFAULT 0,
    parent_id TEXT,
    model TEXT
);
```
