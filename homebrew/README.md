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

When the plugin gets a new release, update the formula:

1. Replace the `revision` value in `llp.rb` with the new commit SHA (or use `url` pointing to a release archive).
2. Update `version` if the version changed.
3. Commit and push to the `homebrew-tap` repo.

### Optional: Stable release

For a stable release (no `--head` flag needed), tag the `logs-locally-plugin` repo:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Then update the formula's `url`:

```ruby
url "https://github.com/Laotree/logs-locally-plugin.git", tag: "v0.1.0"
```

And you can optionally switch to an archive URL:

```ruby
url "https://github.com/Laotree/logs-locally-plugin/archive/refs/tags/v0.1.0.tar.gz"
sha256 "..."  # run `brew fetch --force-bottle-url <url>` to get the SHA
```
