# Installs the Port Process Manager desktop app and the ppm CLI from GitHub Releases, for the current user.
#
#   irm https://greenfield-inc.github.io/port-process-manager/install.ps1 | iex
#   & ([scriptblock]::Create((irm https://greenfield-inc.github.io/port-process-manager/install.ps1))) -NoOpen
#
# The app installs quietly into %LOCALAPPDATA%\Port Process Manager with no admin
# prompt, and ppm goes into ~\.local\bin, which is added to your PATH. A download
# through PowerShell has no Mark of the Web, so SmartScreen doesn't ask.
#
# Options:
#   -Cli      install only the ppm CLI
#   -NoOpen   don't open the app afterwards
#
# Environment:
#   PPM_VERSION       release to install, such as 0.2.0 (default: latest)
#   PPM_APP_DIR       where the app goes (default: %LOCALAPPDATA%\Port Process Manager)
#   PPM_INSTALL_DIR   where ppm.exe goes (default: ~\.local\bin)
#   PPM_DOWNLOAD_URL  folder that holds the release files, for mirrors and testing (default: the GitHub release)
#   PPM_ALLOW_INSECURE set to 1 to allow a PPM_DOWNLOAD_URL that is not https://, for testing
param([switch]$Cli, [switch]$NoOpen)

# A child scope keeps these settings out of the caller's session when run through iex.
& {
  $ErrorActionPreference = 'Stop'
  $ProgressPreference = 'SilentlyContinue'
  # Windows PowerShell 5.1 can default to TLS 1.0, which GitHub refuses.
  [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

  $repo = 'https://github.com/greenfield-inc/port-process-manager'
  $base = if ($env:PPM_DOWNLOAD_URL) { $env:PPM_DOWNLOAD_URL }
    elseif ($env:PPM_VERSION) { "$repo/releases/download/v$($env:PPM_VERSION.TrimStart('v'))" }
    else { "$repo/releases/latest/download" }
  # SHA256SUMS comes from the same place as the files, so only https protects them.
  if (-not $base.StartsWith('https://') -and $env:PPM_ALLOW_INSECURE -ne '1') {
    throw "Port Process Manager install: $base is not an https:// URL. For testing, set PPM_ALLOW_INSECURE=1."
  }

  $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("ppm-" + [guid]::NewGuid())
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
      if (-not $sums[$name]) { throw "Port Process Manager install: SHA256SUMS has no entry for $name" }
      $actual = (Get-FileHash -Algorithm SHA256 $path).Hash.ToLower()
      if ($actual -ne $sums[$name]) { throw "Port Process Manager install: checksum mismatch for ${name}: expected $($sums[$name]), got $actual" }
      $path
    }

    $setup = $null
    if (-not $Cli) {
      $arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') { 'aarch64' } else { 'x86_64' }
      $installers = @($sums.Keys | Where-Object { $_ -like 'port-process-manager-*-setup.exe' })
      $name = $installers | Where-Object { $_ -like "*-$arch-setup.exe" } | Select-Object -First 1
      if (-not $name -and $arch -eq 'aarch64') {
        # Windows 11 on Arm runs x64 apps through emulation. ppm itself is native.
        $name = $installers | Where-Object { $_ -like '*-x86_64-setup.exe' } | Select-Object -First 1
        if ($name) { Write-Host 'There is no Arm64 build of the app yet, so this installs the x64 app, which Windows runs through emulation.' }
      }
      if (-not $name) { throw 'Port Process Manager install: this release has no Windows app installer' }
      # Verify everything before installing anything.
      $setup = Get-ReleaseFile $name
    }

    $cliScript = Get-Content -Raw (Get-ReleaseFile 'install.ps1')
    $previousUrl = $env:PPM_DOWNLOAD_URL
    $env:PPM_DOWNLOAD_URL = $base
    try { & ([scriptblock]::Create($cliScript)) } finally { $env:PPM_DOWNLOAD_URL = $previousUrl }

    if ($setup) {
      # The installer is per-user (no admin prompt). In silent mode it closes a running copy first.
      # /D must come last and unquoted. Without it, the installer uses the default folder.
      $setupArgs = @('/S')
      if ($env:PPM_APP_DIR) { $setupArgs += "/D=$env:PPM_APP_DIR" }
      $appDir = if ($env:PPM_APP_DIR) { $env:PPM_APP_DIR } else { Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Port Process Manager' }
      Write-Host 'Installing Port Process Manager'
      $process = Start-Process -FilePath $setup -ArgumentList $setupArgs -Wait -PassThru
      if ($process.ExitCode -ne 0) { throw "Port Process Manager install: the installer exited with code $($process.ExitCode)" }
      $exe = Join-Path $appDir 'ppm-desktop.exe'
      if (-not (Test-Path $exe)) { throw "Port Process Manager install: the installer finished, but $exe is missing" }
      Write-Host "Installed Port Process Manager to $appDir"
      if (-not $NoOpen) { Start-Process -FilePath $exe }
    }
  } finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
  }
}
