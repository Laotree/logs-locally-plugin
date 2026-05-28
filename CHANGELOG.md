# Changelog

All notable changes to this project are documented here.

## [0.8.1] — 2026-05-28

### Changed
- Security audit: no sensitive information found; codebase is clean

## [0.8.0] — 2026-05-28

### Added
- **Default push URL** — `llp push` now defaults to `https://llp.qingyuejiaju.cn`, no config needed
- **Anonymous push support** — `pushToken` is no longer required; relay accepts pushes without a Bearer token, using the `user` field for identity instead
- **CI Docker build workflow** — GitHub Actions automatically builds and pushes Docker images to `ghcr.io/laotree/logs-locally-plugin` on every push to `main`
- `docker-compose.yml` now pulls from `ghcr.io` instead of building locally — zero-build deployment on low-performance machines

### Changed
- `pushUrl` config field now has a default value (`https://llp.qingyuejiaju.cn`), defined in `config.rs`
- Push URL normalization: auto-prepends `http://` or `https://` when no scheme is provided

## [0.7.0] — 2026-05-28

### Added
- **Codex CLI session support** — import OpenAI Codex CLI sessions from `~/.codex/sessions/` into the same SQLite database alongside Claude Code and Pi sessions
- New config field `codexSessionsDir` (optional) — set to `~/.codex/sessions` to enable Codex imports
- `source="codex"` tag on all Codex sessions; green badge in web UI; "Codex CLI" filter pill in session browser
- Per-turn assistant messages aggregated from Codex reasoning summaries, function calls, and tool outputs

## [0.6.0] — 2026-05-28

### Added
- **`llp push`** — renders a token/session activity heatmap SVG locally and uploads it to a Cloudflare Worker or relay; enables GitHub profile chart embedding with no raw session content leaving the machine
- **Activity heatmap calendar** in the global dashboard (two contribution-style grids: sessions and tokens)
- **Multi-user relay server** (`llp relay`) with anonymous token hashing (SHA-256), per-user hourly rate limiting, and cron scheduling
- **Cloudflare Workers** backend for chart hosting (replaces Fly.io); includes `workers/` directory with a zero-build-step JS Worker
- **`docker-compose.yml`** for easy self-hosted relay deployment
- **Per-user push rate limit** (`LLP_MAX_PUSHES_PER_HOUR`, default 5) on the relay
- Auto-schedule prompt on first `llp push` — installs a daily 09:00 cron job with user confirmation
- `wrangler.toml.example` and `.dev.vars` ignored by default

### Fixed
- Frontend wired to real API endpoints (removed all mock/stub data); scroll and sidebar layout corrected (#51)
- Month labels in SVG chart skipped when too close to the previous label (#46)
- Stray backslashes in SVG raw strings in `chart.rs` (#45)
- Wrangler dependency bumped from `^3` to `^4` to resolve high-severity vulnerability (#42)

### Documentation
- Homebrew listed as recommended install option
- README updated to reflect multi-agent scope (Claude Code + Pi agent)
- Config reference table and relay self-hosting guide added

## [0.5.0] — 2026-05-27

### Added
- Global aggregate dashboard with radar chart, stat bars, and neon pie chart
- Hover tooltip on N/A score badge explaining why a session is unscored

### Fixed
- Removed unused `get_unscored_session_ids` from `db.rs`

## [0.4.0]

### Added
- Pi agent session support — browse Claude Code and Pi sessions in one UI
- `import-all` command for bulk historical imports
- `rescore` command to re-evaluate session quality scores after upgrades
- Multiple DB paths (`db_paths` config) — import writes to all, serve reads the first
- `source` column on sessions table (`claude` | `pi`)

## [0.3.0]

### Added
- Session scoring across 7 quality dimensions: security, effectivity, solidity, efficiency, planning, recovery, accuracy
- Letter grades S/A/B/C/D/F; trivial sessions marked N/A
- Sensitive data scrubbing before storage (API keys, tokens, home paths, emails)

## [0.2.1]

### Fixed
- Minor bug fixes and stability improvements

## [0.2.0]

### Added
- Web UI (`llp serve`) for browsing and searching sessions
- Dark theme, live auto-refresh (10 s polling)
- Statistics dashboard (token usage by model, score aggregates)
