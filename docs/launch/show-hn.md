# Show HN draft

## Title (pick one — keep under 80 chars, no "Show HN:" overthinking)

- Show HN: Llp – Zero-config local history for Claude Code / Codex / Pi sessions
- Show HN: I kept losing my AI coding sessions, so I built a local logger

## URL

https://github.com/Laotree/logs-locally-plugin

## First comment (post immediately after submitting)

I use Claude Code and Codex CLI all day, and every session evaporates the moment
I close the terminal. The transcripts are sitting right there as `.jsonl` files in
`~/.claude/projects/`, but there's no good way to search across them or look back
at what an agent actually did last Tuesday.

llp is a single Rust binary that fixes this:

- Drops one line into your `Stop` hook, so it imports the latest session
  automatically every time an agent exits. No daemon, no cloud, no API key.
- Stores everything in a local SQLite DB and serves a searchable web UI on
  127.0.0.1 — filter by model, time, keyword, or a quality score.
- Works across Claude Code, Pi agent, and Codex CLI in one place.
- Scrubs API keys, tokens, emails, and home paths before anything is written.

Two things I'd genuinely like feedback on:

1. The "session scoring" — it grades each session across 7 dimensions (security,
   efficiency, planning, recovery, etc.). It's heuristic and opinionated, and I'm
   not sure the dimensions are the right ones. Tell me where it's wrong.
2. The optional GitHub-profile activity chart (`llp push`) renders the SVG locally
   and only uploads the final image — no session content leaves your machine. Curious
   whether people find that useful or gimmicky.

Install is `brew tap Laotree/tap && brew install llp` (or `cargo install --git ...`).
It's MIT. Happy to answer anything about the parsing, the SQLite schema, or the
privacy scrubbing.

## Posting notes

- Best windows for Show HN: weekday mornings ~8–10am US Eastern.
- Don't ask for upvotes anywhere. Reply to every comment in the first 2 hours.
- If someone asks "why not just grep the jsonl?" — the honest answer is: you can,
  but cross-session search, dedup, scoring, and the UI are the point.
