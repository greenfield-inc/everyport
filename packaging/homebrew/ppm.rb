class Ppm < Formula
  desc "See every dev server on your machine, or any box you can reach"
  homepage "https://github.com/greenfield-inc/port-process-manager"
  version "@VERSION@"
  license "MIT"

  base = "https://github.com/greenfield-inc/port-process-manager/releases/download/v#{version}"

  on_macos do
    on_arm do
      url "#{base}/ppm-aarch64-apple-darwin"
      sha256 "@SHA256:ppm-aarch64-apple-darwin@"
    end
    on_intel do
      url "#{base}/ppm-x86_64-apple-darwin"
      sha256 "@SHA256:ppm-x86_64-apple-darwin@"
    end
  end

  on_linux do
    on_arm do
      url "#{base}/ppm-aarch64-unknown-linux-musl"
      sha256 "@SHA256:ppm-aarch64-unknown-linux-musl@"
    end
    on_intel do
      url "#{base}/ppm-x86_64-unknown-linux-musl"
      sha256 "@SHA256:ppm-x86_64-unknown-linux-musl@"
    end
  end

  def install
    bin.install Dir["ppm-*"].first => "ppm"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/ppm --version")
  end
end
