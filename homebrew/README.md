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

#### Manual formula update

If the automated publish didn't fire (e.g. missing token), update the formula manually:

1. Replace the `revision` value in `llp.rb` with the new commit SHA.
2. Update `version` and `tag` if the version changed.
3. Commit and push to the `homebrew-tap` repo.

### Release workflow

```bash
git tag v0.5.0
git push origin v0.5.0       # triggers GitHub Actions → homebrew-tap dispatch
```

### Optional: Archive URL (stable release)

```ruby
url "https://github.com/Laotree/logs-locally-plugin/archive/refs/tags/v0.5.0.tar.gz"
sha256 "..."  # run `brew fetch --force-bottle-url <url>` to get the SHA
```
