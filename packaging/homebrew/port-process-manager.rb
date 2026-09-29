cask "port-process-manager" do
  arch arm: "aarch64", intel: "x86_64"

  version "@VERSION@"
  sha256 arm:   "@SHA256:port-process-manager-@VERSION@-aarch64.dmg@",
         intel: "@SHA256:port-process-manager-@VERSION@-x86_64.dmg@"

  url "https://github.com/greenfield-inc/port-process-manager/releases/download/v#{version}/port-process-manager-#{version}-#{arch}.dmg"
  name "Port Process Manager"
  desc "Menu bar app that shows every dev server on your machine"
  homepage "https://github.com/greenfield-inc/port-process-manager"

  depends_on macos: ">= :ventura"

  app "Port Process Manager.app"

  zap trash: [
    "~/Library/Application Support/port-process-manager",
    "~/Library/Application Support/to.greenfield.portprocessmanager",
    "~/Library/Caches/to.greenfield.portprocessmanager",
    "~/Library/WebKit/to.greenfield.portprocessmanager",
  ]
end
