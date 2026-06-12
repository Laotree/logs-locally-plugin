# Changelog

All notable changes to this project are documented here.

## [0.10.1] — 2026-06-12

### Changed
- **Web UI: tech-themed visual overhaul** (#83). Dark-first deep-space palette with neon cyan/violet accents and a refreshed cool light palette; layered blueprint-grid + ambient-glow backdrop; glassmorphism (translucent + backdrop-blur) sidebar, top bar, panels and tables; Oxanium display font for headings, KPI values and grade letters (replaces the Newsreader serif); glow effects on active nav, KPI cards, badges, focus rings and the status pulse dot; view entrance animations and heatmap cell hover zoom. The tweaks panel and gear button are now theme-aware (previously hardcoded light glass), and the forest/plum/crimson/mono accent variants were retuned for both themes. All functionality is unchanged.

### Fixed
- Web UI: added the missing `llp-spin` keyframes — the loading spinner now actually rotates.

## [0.10.0] — 2026-06-12

### Added
- **opencode session support.** New optional config field `opencodeStorageDir` (e.g. `~/.local/share/opencode/storage`). When set, `llp import` also imports the latest opencode session and `llp import-all` imports every opencode session across all projects. Verified against opencode v1.14.x storage (session info under `storage/session/<projectID>/`, message metadata under `storage/message/<sessionID>/`, content parts under `storage/part/<messageID>/`). Sessions are stored with `source = 'opencode'` and include title, working directory, model, token counts, thinking blocks, and tool calls/results.
- Web UI: opencode source badge and an "opencode" option in the Sessions source filter.

### Changed
- Docs (README + homepage) updated to cover all four supported sources (Claude Code, Pi agent, Codex CLI, opencode); fixed the duplicated step numbering in the README "How It Works" list.

## [0.9.1] — 2026-06-04

### Changed
- **Web UI: increased information density.** Padding and gaps reduced ~25% throughout (topbar, KPI cards, panels, session table rows, session detail view), fitting more content into each viewport without scrolling. All sub-12 px label fonts bumped 1–2 px for legibility (nav sections, table headers, badge labels, meta text).
- **Sidebar version tag is now dynamic.** `/api/stats` exposes `CARGO_PKG_VERSION`; the sidebar reads `stats.version` instead of the previously hardcoded `v0.5.0`.

## [0.9.0] — 2026-05-29

### Removed
- **Cloud activity-chart sharing feature** (`llp push`, `llp relay`, the bundled Cloudflare Worker in `workers/`, and the `/api/push` + `/chart.svg` server endpoints). This was the only part of the project that pulled in a Node.js toolchain (`wrangler`) and the heavyweight `reqwest` HTTP/TLS stack.
  - Dropped dependencies `reqwest`, `sha2`, and `hex`, shrinking the release binary.
  - Removed config fields `pushToken`, `pushUrl`, `pushUser`, and `dataDir` (silently ignored if still present in an existing `config.json`).
  - The local web UI is unchanged: the activity heatmap on the dashboard still renders, served from the local SQLite DB via `/api/activity`.
  - Docker image is now serve-only (relay mode and `docker-compose.yml` removed).

## [0.8.9] — 2026-05-29

### Fixed
- `release-binaries.yml` could not upload the prebuilt tarballs (HTTP 403 — the job's `GITHUB_TOKEN` lacked `contents: write`); added the permission. (v0.8.8 built but never published a release.)

## [0.8.8] — 2026-05-29

### Changed
- **Homebrew now installs a prebuilt binary instead of building from source.** The formula downloads a stripped ~1 MB binary from the GitHub Release — no Rust toolchain or from-source compile required. New `release-binaries.yml` workflow builds per-platform tarballs (macOS arm64/x86_64, Linux x86_64) on `v*` tags; `publish-homebrew-tap.yml` waits for those assets and the tap's generator downloads and hashes them.
- Release binaries are smaller: `[profile.release]` size optimizations (`opt-level = "z"`, `lto`, `codegen-units = 1`, `strip`, `panic = "abort"`) and trimmed `tokio` features cut the binary from ~8.9 MB to ~3.1 MB.

### Fixed
- Homebrew formula smoke test used the nonexistent `llp --version`; corrected to `llp version`.

## [0.8.7] — 2026-05-29

### Added
- Grade filter on the Sessions page — filter by S / A / B / C / D / F

### Changed
- Filter bar now stacks vertically so all filters are always visible
- Model filter options are loaded from `/api/stats` (stable across pages) instead of being derived from the current page's sessions

## [0.8.6] — 2026-05-28

### Fixed
- `llp push`, `llp relay`, `llp version` commands now included in published binary (v0.8.5 tag was cut before these were added)

## [0.8.5] — 2026-05-28

### Added
- `llp version` command — prints the current version and exits

### Changed
- Removed marketplace install instructions from README (not yet available)

## [0.8.4] — 2026-05-28

### Fixed
- `llp import-all` without arguments now imports all projects (previously required explicit args)

## [0.8.3] — 2026-05-28

### Changed
- README restructured: problem → solution → copy-paste setup for faster onboarding
- README title and opening now correctly reflect multi-agent scope (Claude Code, Pi agent, Codex CLI)
- Added activity heatmap live SVG preview to README
- GitHub repository topics updated: added `sqlite` and `llm`

## [0.8.2] — 2026-05-28

### Fixed
- `llp push` no longer prompts to schedule a cron job if one is already installed
- `llp push` now shows how long to wait when rate limited (e.g. "try again in 47 minute(s)") instead of a bare HTTP 429 error

### Meta
- Added `license`, `repository`, and `readme` fields to `Cargo.toml`; first release published to [crates.io](https://crates.io/crates/logs-locally-plugin)

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
