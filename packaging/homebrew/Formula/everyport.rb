class Everyport < Formula
  desc "See every dev server on your machine, or any box you can reach"
  homepage "https://github.com/greenfield-inc/everyport"
  version "@VERSION@"
  license "MIT"

  base = "https://github.com/greenfield-inc/everyport/releases/download/v#{version}"

  on_macos do
    on_arm do
      url "#{base}/everyport-aarch64-apple-darwin"
      sha256 "@SHA256:everyport-aarch64-apple-darwin@"
    end
    on_intel do
      url "#{base}/everyport-x86_64-apple-darwin"
      sha256 "@SHA256:everyport-x86_64-apple-darwin@"
    end
  end

  on_linux do
    on_arm do
      url "#{base}/everyport-aarch64-unknown-linux-musl"
      sha256 "@SHA256:everyport-aarch64-unknown-linux-musl@"
    end
    on_intel do
      url "#{base}/everyport-x86_64-unknown-linux-musl"
      sha256 "@SHA256:everyport-x86_64-unknown-linux-musl@"
    end
  end

  def install
    bin.install Dir["everyport-*"].first => "everyport"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/everyport --version")
  end
end
