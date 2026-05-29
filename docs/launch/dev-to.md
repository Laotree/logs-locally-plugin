---
title: I kept losing my AI coding sessions, so I built a zero-config local logger
published: false
description: llp saves every Claude Code, Codex CLI, and Pi agent session to a local SQLite DB with a searchable web UI — automatically, after every session.
tags: ai, rust, cli, productivity
cover_image: https://laotree.github.io/logs-locally-plugin/docs/demo.gif
---

> Replace `published: false` with `true` when you're ready. Swap the cover image for a
> hosted GIF/PNG URL if dev.to doesn't pick up the relative path.

## The problem

I run Claude Code and Codex CLI all day. Every one of those conversations — the plan,
the dead ends, the actual diff that worked — disappears the second I close the terminal.

The data isn't even gone. It's sitting in `~/.claude/projects/<project>/*.jsonl`. But
there's no way to search across sessions, no way to ask "what did the agent try last
week," and no way to see at a glance whether a session went well or off the rails.

## The solution

**llp** — a single Rust binary that imports those session files into a local SQLite
database and serves a searchable web UI. Zero config, no daemon, no cloud, no API key.

![llp demo](https://laotree.github.io/logs-locally-plugin/docs/demo.gif)

## 30-second setup

```bash
# Install (macOS / Linux)
brew tap Laotree/tap && brew install llp

# Save your latest session right now
llp import

# Browse everything at http://127.0.0.1:8484
llp serve
```

Then add one line to `~/.claude/settings.json` so it runs automatically on every exit:

```json
{
  "hooks": {
    "Stop": [{ "hooks": [{ "type": "command", "command": "llp import" }] }]
  }
}
```

That's it. Every session from now on gets saved the moment the agent stops.

## What you actually get

- **Full history** of every Claude Code, Pi agent, and Codex CLI session, in SQLite.
- **Searchable web UI** — filter by model, time, keyword, or quality score.
- **Session scoring** — each session graded across 7 dimensions (security, efficiency,
  planning, recovery, accuracy…) with letter grades.
- **Privacy-first** — API keys, tokens, emails, and home paths are scrubbed before
  storage. Nothing leaves your machine.
- **Optional GitHub profile chart** — a live activity heatmap rendered locally; only
  the final SVG is uploaded, never session content.

## Why "plugin" if it's just a binary?

Fair question. The name reflects that it hooks into Claude Code's `Stop` hook — but
there's no marketplace or plugin runtime involved. It's a plain CLI you install with
Homebrew or `cargo install`, and the hook is a one-line command.

## A note on the scoring

The quality scoring is heuristic and deliberately opinionated — it rewards structured
planning, test execution, and clean recovery from errors, and penalizes dangerous
commands and correction loops. I'd love feedback on whether the dimensions are the
right ones.

## Try it

- GitHub: https://github.com/Laotree/logs-locally-plugin
- It's MIT licensed. Issues and PRs welcome.

If you've ever scrolled back up a dead terminal trying to remember what an agent did,
this is for you.

## Posting notes (delete before publishing)

- dev.to lets you import from a canonical URL — set `canonical_url` if you cross-post.
- The cover_image relative path may not resolve on dev.to; upload the GIF or use the
  full https URL once GitHub Pages serves it.
