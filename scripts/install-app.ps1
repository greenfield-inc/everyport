# Installs the Everyport desktop app and the everyport CLI from GitHub Releases, for the current user.
#
#   irm https://everyport.dev/install.ps1 | iex
#   & ([scriptblock]::Create((irm https://everyport.dev/install.ps1))) -NoOpen
#
# The app installs quietly into %LOCALAPPDATA%\Everyport with no admin
# prompt, and everyport goes into ~\.local\bin, which is added to your PATH. A download
# through PowerShell has no Mark of the Web, so SmartScreen doesn't ask.
#
# Options:
#   -Cli      install only the everyport CLI
#   -NoOpen   don't open the app afterwards
#
# Environment:
#   EVERYPORT_VERSION       release to install, such as 0.2.0 (default: latest)
#   EVERYPORT_APP_DIR       where the app goes (default: %LOCALAPPDATA%\Everyport)
#   EVERYPORT_INSTALL_DIR   where everyport.exe goes (default: ~\.local\bin)
#   EVERYPORT_DOWNLOAD_URL  folder that holds the release files, for mirrors and testing (default: the GitHub release)
#   EVERYPORT_ALLOW_INSECURE set to 1 to allow a EVERYPORT_DOWNLOAD_URL that is not https://, for testing
param([switch]$Cli, [switch]$NoOpen)

# A child scope keeps these settings out of the caller's session when run through iex.
& {
  $ErrorActionPreference = 'Stop'
  $ProgressPreference = 'SilentlyContinue'
  # Windows PowerShell 5.1 can default to TLS 1.0, which GitHub refuses.
  [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

  $repo = 'https://github.com/greenfield-inc/everyport'
  $base = if ($env:EVERYPORT_DOWNLOAD_URL) { $env:EVERYPORT_DOWNLOAD_URL }
    elseif ($env:EVERYPORT_VERSION) { "$repo/releases/download/v$($env:EVERYPORT_VERSION.TrimStart('v'))" }
    else { "$repo/releases/latest/download" }
  # SHA256SUMS comes from the same place as the files, so only https protects them.
  if (-not $base.StartsWith('https://') -and $env:EVERYPORT_ALLOW_INSECURE -ne '1') {
    throw "Everyport install: $base is not an https:// URL. For testing, set EVERYPORT_ALLOW_INSECURE=1."
  }

  $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("everyport-" + [guid]::NewGuid())
  New-Item -ItemType Directory -Path $tmp | Out-Null
  try {
    $sums = @{}
    $text = (Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS").Content
    if ($text -is [byte[]]) { $text = [System.Text.Encoding]::UTF8.GetString($text) }
    foreach ($line in $text -split "`r?`n") {
      $fields = $line.Trim() -split '\s+'
      if ($fields.Count -eq 2) { $sums[$fields[1].TrimStart('*')] = $fields[0].ToLower() }
    }

    # Downloads a release file and checks it against SHA256SUMS.
    function Get-ReleaseFile($name) {
      Write-Host "Downloading $name"
      $path = Join-Path $tmp $name
      Invoke-WebRequest -UseBasicParsing -Uri "$base/$name" -OutFile $path
      if (-not $sums[$name]) { throw "Everyport install: SHA256SUMS has no entry for $name" }
      $actual = (Get-FileHash -Algorithm SHA256 $path).Hash.ToLower()
      if ($actual -ne $sums[$name]) { throw "Everyport install: checksum mismatch for ${name}: expected $($sums[$name]), got $actual" }
      $path
    }

    $setup = $null
    if (-not $Cli) {
      $arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') { 'aarch64' } else { 'x86_64' }
      $installers = @($sums.Keys | Where-Object { $_ -like 'everyport-*-setup.exe' })
      $name = $installers | Where-Object { $_ -like "*-$arch-setup.exe" } | Select-Object -First 1
      if (-not $name -and $arch -eq 'aarch64') {
        # Windows 11 on Arm runs x64 apps through emulation. everyport itself is native.
        $name = $installers | Where-Object { $_ -like '*-x86_64-setup.exe' } | Select-Object -First 1
        if ($name) { Write-Host 'There is no Arm64 build of the app yet, so this installs the x64 app, which Windows runs through emulation.' }
      }
      if (-not $name) { throw 'Everyport install: this release has no Windows app installer' }
      # Verify everything before installing anything.
      $setup = Get-ReleaseFile $name
    }

    $cliScript = Get-Content -Raw (Get-ReleaseFile 'install.ps1')
    $previousUrl = $env:EVERYPORT_DOWNLOAD_URL
    $env:EVERYPORT_DOWNLOAD_URL = $base
    try { & ([scriptblock]::Create($cliScript)) } finally { $env:EVERYPORT_DOWNLOAD_URL = $previousUrl }

    if ($setup) {
      # The installer is per-user (no admin prompt). In silent mode it closes a running copy first.
      # /D must come last and unquoted. Without it, the installer uses the default folder.
      $setupArgs = @('/S')
      if ($env:EVERYPORT_APP_DIR) { $setupArgs += "/D=$env:EVERYPORT_APP_DIR" }
      $appDir = if ($env:EVERYPORT_APP_DIR) { $env:EVERYPORT_APP_DIR } else { Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Everyport' }
      Write-Host 'Installing Everyport'
      $process = Start-Process -FilePath $setup -ArgumentList $setupArgs -Wait -PassThru
      if ($process.ExitCode -ne 0) { throw "Everyport install: the installer exited with code $($process.ExitCode)" }
      $exe = Join-Path $appDir 'everyport-desktop.exe'
      if (-not (Test-Path $exe)) { throw "Everyport install: the installer finished, but $exe is missing" }
      Write-Host "Installed Everyport to $appDir"
      if (-not $NoOpen) { Start-Process -FilePath $exe }
    }
  } finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
  }
}
