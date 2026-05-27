[![Plugin](https://img.shields.io/badge/Claude%20Code-Plugin-f5a623)](https://laotree.github.io/logs-locally-plugin/)
[![MIT License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

# Logs Locally Plugin

Store and browse Claude Code **and Pi agent** session logs in a local SQLite database with a built-in web UI and automatic session scoring.

**[Homepage](https://laotree.github.io/logs-locally-plugin/) &middot; [Installation](#installation) &middot; [GitHub](https://github.com/Laotree/logs-locally-plugin)**

![Logs Locally web UI — session list and conversation detail](docs/screenshot.png)

```
llp import       # save the latest session to SQLite
llp serve        # start the web log browser
llp import-all   # bulk import all historical sessions for a project
llp rescore      # re-score all sessions (for version upgrades)
```

## How It Works

Each time Claude Code exits, the `Stop` hook triggers `llp import`, which:

1. Finds the most recent `.jsonl` session file in `~/.claude/projects/<project>/`
2. Parses messages, models, token usage, and session metadata
3. Upserts into a local SQLite database (deduplicated by session ID)
4. Also imports the latest **Pi agent** session for the same project (if `piJsonlDir` is configured)
5. Scores each session across 7 quality dimensions (security, effectivity, solidity, efficiency, planning, recovery, accuracy)
6. Scrubs sensitive data (API keys, tokens, credentials, home paths, emails) before storage

The `serve` command starts a web UI at `http://127.0.0.1:8484` for browsing and searching sessions.

## Installation

### Option 1: Homebrew (recommended)

```bash
brew tap Laotree/tap
brew install llp
```

### Option 2: Install from source

```bash
cargo install --git https://github.com/Laotree/logs-locally-plugin
```

### Option 3: Build locally

```bash
cargo build --release
cp target/release/llp ~/.local/bin/llp
```

> Make sure `~/.local/bin` is in your `PATH`.

### Option 4: Install as a Claude Code plugin (marketplace)

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

Create `config.json` in the working directory or pass a custom path with `llp --config <path>`:

```json
{
  "db_path": "~/.local/share/logs-locally-plugin/logs.db",
  "db_paths": [
    "~/.local/share/logs-locally-plugin/logs.db",
    "~/path/to/secondary/logs.db"
  ],
  "claude_projects_dir": "~/.claude/projects",
  "piJsonlDir": "~/.pi/agent/sessions",
  "host": "127.0.0.1",
  "port": 8484
}
```

All fields are optional — defaults are shown above.

| Field | Description | Default |
|-------|-------------|---------|
| `db_path` | Primary SQLite database path | `~/.local/share/logs-locally-plugin/logs.db` |
| `db_paths` | Multiple DB paths (import writes to all, serve reads the first) | falls back to `db_path` |
| `claude_projects_dir` | Claude Code sessions directory | `~/.claude/projects` |
| `piJsonlDir` | Pi agent sessions directory (optional — omit to skip pi imports) | none |
| `host` | Web server bind address | `127.0.0.1` |
| `port` | Web server port | `8484` |

## Usage

### Import the latest session

```bash
cd /path/to/your/project
llp import
```

This auto-detects the Claude Code project from the current working directory and imports the most recent session (both Claude Code and Pi agent).

You can also import a specific JSONL file:

```bash
llp import /path/to/specific/session.jsonl
```

### Import all historical sessions

```bash
llp import-all /path/to/your/project
```

Imports every past session (Claude and Pi) for the given project directory.

### Browse logs

```bash
llp serve
# Open http://127.0.0.1:8484
```

```bash
llp serve --port 9090   # override port
```

Features:
- Session list with search and filters (by model, source, time range, keyword)
- Message detail view with thinking blocks and tool calls
- Session scoring (7 quality dimensions with letter grades S/A/B/C/D/F)
- **Pi agent session support** — browse sessions from both agents in one UI
- Live auto-refresh (10s polling)
- Statistics dashboard (token usage by model, score aggregates)
- Dark theme, Claude web-inspired design

### Re-score sessions

```bash
llp rescore
```

Re-evaluates session quality scores. Useful after upgrading from a version that didn't include session scoring. Trivial sessions (single commands, empty exchanges) are marked N/A.

### Command reference

```
Usage: llp [OPTIONS] <COMMAND>

Commands:
  import       Import the latest Claude Code or Pi session into SQLite
  serve        Start the local web server for browsing logs
  import-all   Import all existing sessions from a project
  rescore      Re-score all sessions in the database
  help         Print help

Options:
  -c, --config <FILE>  Path to config file [default: config.json]
  -h, --help           Print help
  push         Push daily aggregated activity to a remote llp server
```

## GitHub Profile Chart (Fly.io)

Embed a live token/session heatmap in your GitHub profile README — two contribution-style grids, no raw session content ever leaves your machine.

### 1. Deploy to Fly.io

```bash
fly auth login
fly apps create llp-chart          # or any name you prefer
fly volumes create llp_data --size 1 --region nrt
fly secrets set LLP_PUSH_TOKEN=$(openssl rand -hex 32)
fly deploy
```

Your chart is now live at `https://llp-chart.fly.dev/chart.svg`.

### 2. Configure locally

Add to `config.json`:

```json
{
  "pushToken": "<same secret you set above>"
}
```

### 3. Push activity data

```bash
llp push https://llp-chart.fly.dev
```

Sends only daily aggregates (`day`, `session_count`, `token_count`) — no titles, messages, or any session content.

Optionally add to your `Stop` hook to push automatically on every session end:

```json
{
  "hooks": {
    "Stop": [
      { "hooks": [{ "type": "command", "command": "llp import && llp push https://llp-chart.fly.dev" }] }
    ]
  }
}
```

### 4. Add to your GitHub profile README

```markdown
![Activity](https://llp-chart.fly.dev/chart.svg)
```

---

## Build

```bash
# Build the release binary
cargo build --release

# Install to ~/.local/bin
cp target/release/llp ~/.local/bin/llp

# Or install from source in one step
cargo install --git https://github.com/Laotree/logs-locally-plugin
```

### Makefile targets

```bash
make build    # cargo build
make release  # cargo build --release
make install  # build release + copy to ~/.local/bin + install hooks
make test     # cargo test
make serve    # cargo run -- serve
make import   # cargo run -- import
make rescore  # cargo run -- rescore
make fmt      # cargo fmt
make lint     # cargo clippy
make clean    # cargo clean
```

## Database

Sessions and messages are stored in a SQLite database at `~/.local/share/logs-locally-plugin/logs.db` (configurable). Imports can write to multiple DBs simultaneously (`db_paths` config).

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
    version TEXT,
    source TEXT NOT NULL DEFAULT 'claude'   -- 'claude' | 'pi'
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

-- Scoring table (auto-scored on import)
CREATE TABLE scores (
    session_id TEXT PRIMARY KEY REFERENCES sessions(id),
    total_score INTEGER NOT NULL,
    security INTEGER NOT NULL,
    effectivity INTEGER NOT NULL,
    solidity INTEGER NOT NULL,
    efficiency INTEGER NOT NULL,
    planning_quality INTEGER NOT NULL,
    recovery_ability INTEGER NOT NULL,
    hallucination_rate INTEGER NOT NULL,
    grade TEXT NOT NULL,            -- S/A/B/C/D/F
    scored_at TEXT NOT NULL
);
```

### Scoring dimensions

Each session is scored on 7 dimensions (max 100 total):

| Dimension | Max | What it measures |
|-----------|-----|------------------|
| Security | 15 | Dangerous commands (rm -rf, pipe-to-shell, etc.) |
| Effectivity | 15 | Completion rate, failure vs. success signals |
| Solidity | 10 | Test coverage: test execution > test references > code generation |
| Efficiency | 15 | Correction loops, token bloat, short clean sessions |
| Planning | 15 | Structured plans, numbered steps, sequential thinking |
| Recovery | 15 | Error handling, self-correction rate |
| Accuracy | 15 | User satisfaction vs. corrections and confusion |

Trivial sessions (single commands, empty exchanges) are marked N/A instead of scored.

### Security & privacy

Before storage, all content is scrubbed for sensitive data:
- API keys (Anthropic, OpenAI, GitHub, AWS)
- Bearer tokens and credentials in URLs
- Environment variable secrets (names ending in KEY/SECRET/TOKEN/PASSWORD)
- Home directory paths (`/Users/name` → `~`)
- Email addresses
