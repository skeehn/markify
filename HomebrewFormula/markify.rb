class Markify < Formula
  desc "Web data layer for AI agents — scrape, search, extract, structure"
  homepage "https://github.com/skeehn/markify"
  url "https://github.com/skeehn/markify/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "FILL_IN_AFTER_TAG_RELEASE"
  license "Apache-2.0"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--root", prefix, "--path", "server"
  end

  service do
    run [opt_bin/"markify", "server"]
    working_dir var
    keep_alive true
    environment_variables RUST_LOG: "info"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/markify --version")
  end
end
