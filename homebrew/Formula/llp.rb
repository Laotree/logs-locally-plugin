class Llp < Formula
  desc "Read Claude Code session JSONL files and persist them to a local SQLite database"
  homepage "https://github.com/Laotree/logs-locally-plugin"
  url "https://github.com/Laotree/logs-locally-plugin.git",
      tag: "v0.2.1",
      revision: "0f149468eba076e9806f0192fc622534e6282dda"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/llp --version")
  end
end
