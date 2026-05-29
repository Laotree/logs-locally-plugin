class Llp < Formula
  desc "Read Claude Code session JSONL files and persist them to a local SQLite database"
  homepage "https://github.com/Laotree/logs-locally-plugin"
  version "0.8.8"
  license "MIT"

  # Downloads a prebuilt, stripped binary (~1 MB) from the GitHub Release
  # produced by .github/workflows/release-binaries.yml. No Rust toolchain or
  # from-source compile required.
  #
  # This is a reference copy. The live formula lives in Laotree/homebrew-tap,
  # where scripts/update_formula.py regenerates it per release — downloading
  # each tarball and filling in the real sha256 (the zeros below are
  # placeholders).
  on_macos do
    on_arm do
      url "https://github.com/Laotree/logs-locally-plugin/releases/download/v0.8.8/llp-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "https://github.com/Laotree/logs-locally-plugin/releases/download/v0.8.8/llp-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/Laotree/logs-locally-plugin/releases/download/v0.8.8/llp-x86_64-unknown-linux-gnu.tar.gz"
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
