class Llp < Formula
  desc "Read Claude Code session JSONL files and persist them to a local SQLite database"
  homepage "https://github.com/Laotree/logs-locally-plugin"
  version "0.8.7"
  license "MIT"

  # Downloads a prebuilt, stripped binary (~1 MB) from the GitHub Release
  # produced by .github/workflows/release-binaries.yml. No Rust toolchain or
  # from-source compile required.
  #
  # The sha256 values below are injected per release by the publish-homebrew-tap
  # workflow from the `llp-<target>.tar.gz.sha256` files attached to the release.
  on_macos do
    on_arm do
      url "https://github.com/Laotree/logs-locally-plugin/releases/download/v0.8.7/llp-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "https://github.com/Laotree/logs-locally-plugin/releases/download/v0.8.7/llp-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/Laotree/logs-locally-plugin/releases/download/v0.8.7/llp-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    bin.install "llp"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/llp version")
  end
end
