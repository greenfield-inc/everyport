# Installs the everyport CLI from GitHub Releases into ~\.local\bin and adds it to your PATH.
#
#   irm https://github.com/greenfield-inc/everyport/releases/latest/download/install.ps1 | iex
#
# Environment:
#   EVERYPORT_VERSION       release to install, such as 0.2.0 (default: latest)
#   EVERYPORT_INSTALL_DIR   where to put everyport.exe (default: ~\.local\bin)
#   EVERYPORT_DOWNLOAD_URL  folder that holds the release files, for mirrors and testing (default: the GitHub release)
#   EVERYPORT_ALLOW_INSECURE set to 1 to allow a EVERYPORT_DOWNLOAD_URL that is not https://, for testing
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repo = 'https://github.com/greenfield-inc/everyport'
$installDir = if ($env:EVERYPORT_INSTALL_DIR) { $env:EVERYPORT_INSTALL_DIR } else { Join-Path $HOME '.local\bin' }

$arch = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
  'X64' { 'x86_64' }
  'Arm64' { 'aarch64' }
  default { throw "everyport install: unsupported CPU: $_" }
}
$asset = "everyport-$arch-pc-windows-msvc.exe"

$base = if ($env:EVERYPORT_DOWNLOAD_URL) { $env:EVERYPORT_DOWNLOAD_URL }
  elseif ($env:EVERYPORT_VERSION) { "$repo/releases/download/v$($env:EVERYPORT_VERSION.TrimStart('v'))" }
  else { "$repo/releases/latest/download" }
# SHA256SUMS comes from the same place as the binary, so only https protects it.
if (-not $base.StartsWith('https://') -and $env:EVERYPORT_ALLOW_INSECURE -ne '1') {
  throw "everyport install: $base is not an https:// URL. For testing, set EVERYPORT_ALLOW_INSECURE=1."
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("everyport-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  Write-Host "Downloading $asset from $base"
  $binary = Join-Path $tmp 'everyport.exe'
  Invoke-WebRequest -UseBasicParsing -Uri "$base/$asset" -OutFile $binary
  $sums = (Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS").Content
  if ($sums -is [byte[]]) { $sums = [System.Text.Encoding]::UTF8.GetString($sums) }

  $expected = $null
  foreach ($line in $sums -split "`r?`n") {
    $fields = $line.Trim() -split '\s+'
    if ($fields.Count -eq 2 -and $fields[1].TrimStart('*') -eq $asset) { $expected = $fields[0].ToLower() }
  }
  if (-not $expected) { throw "everyport install: SHA256SUMS has no entry for $asset" }
  $actual = (Get-FileHash -Algorithm SHA256 $binary).Hash.ToLower()
  if ($actual -ne $expected) { throw "everyport install: checksum mismatch for ${asset}: expected $expected, got $actual" }

  New-Item -ItemType Directory -Force -Path $installDir | Out-Null
  $target = Join-Path $installDir 'everyport.exe'
  Move-Item -Force $binary $target
  Write-Host "Installed $(& $target --version) to $target"
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

# Read the user PATH as stored, so entries like %USERPROFILE%\bin stay unexpanded.
$environment = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
try {
  $userPath = $environment.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
  if (-not (($userPath -split ';') -contains $installDir)) {
    $environment.SetValue('Path', ((@($userPath.TrimEnd(';'), $installDir) | Where-Object { $_ }) -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
    # Setting any user variable tells running apps, such as Explorer, to reload the environment.
    [Environment]::SetEnvironmentVariable('EVERYPORT_INSTALL_REFRESH', '1', 'User')
    [Environment]::SetEnvironmentVariable('EVERYPORT_INSTALL_REFRESH', $null, 'User')
    $env:Path = "$env:Path;$installDir"
    Write-Host "Added $installDir to your PATH. Open a new terminal to use everyport."
  }
} finally {
  $environment.Close()
}
