cask "everyport" do
  arch arm: "aarch64", intel: "x86_64"

  version "@VERSION@"
  sha256 arm:   "@SHA256:everyport-@VERSION@-aarch64.dmg@",
         intel: "@SHA256:everyport-@VERSION@-x86_64.dmg@"

  url "https://github.com/greenfield-inc/everyport/releases/download/v#{version}/everyport-#{version}-#{arch}.dmg"
  name "Everyport"
  desc "Menu bar app that shows every dev server on your machine"
  homepage "https://github.com/greenfield-inc/everyport"

  depends_on macos: :ventura

  app "Everyport.app"

  zap trash: [
    "~/Library/Application Support/everyport",
    "~/Library/Application Support/dev.everyport.desktop",
    "~/Library/Caches/dev.everyport.desktop",
    "~/Library/WebKit/dev.everyport.desktop",
  ]
end
