# Homebrew Tap for logs-locally-plugin

This directory contains the Homebrew formula for [logs-locally-plugin](https://github.com/Laotree/logs-locally-plugin).

## Setup

To make this plugin installable via Homebrew, create a tap repository:

### 1. Create the tap repo on GitHub

Create a new repository named `homebrew-tap` under the `Laotree` GitHub organization (or your personal account). The name must match the pattern `homebrew-<tapname>`.

### 2. Add the formula

Copy `Formula/llp.rb` into the tap repo:

```
homebrew-tap/
  Formula/
    llp.rb
```

Push to GitHub.

### 3. Install

Users can now install with:

```bash
brew tap Laotree/tap
brew install llp
```

Or in one step:

```bash
brew install Laotree/tap/llp
```

### Updating

This repository uses automated publishing via GitHub Actions. When a `v*` tag is pushed to `logs-locally-plugin`, the [publish-homebrew-tap](../.github/workflows/publish-homebrew-tap.yml) workflow sends a `repository_dispatch` event to `Laotree/homebrew-tap`, which updates the formula automatically.

The formula downloads a prebuilt, stripped binary (~1 MB) from the GitHub
Release — no Rust toolchain or from-source compile is needed on the user's
machine. The release artifacts are built by
[`release-binaries.yml`](../.github/workflows/release-binaries.yml), and the
per-platform `sha256` values are forwarded to the tap by the
[`publish-homebrew-tap`](../.github/workflows/publish-homebrew-tap.yml)
workflow.

#### Manual formula update

If the automated publish didn't fire (e.g. missing token), update the formula manually:

1. Bump `version` in `llp.rb`.
2. Update each `url` to the new tag and replace each `sha256` with the value
   from the matching `llp-<target>.tar.gz.sha256` asset on the release.
3. Commit and push to the `homebrew-tap` repo.

### Release workflow

```bash
git tag v0.8.7
git push origin v0.8.7   # builds release binaries → dispatches to homebrew-tap
```
