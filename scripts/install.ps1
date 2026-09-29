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
# Windows PowerShell 5.1 can default to TLS 1.0, which GitHub refuses.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$repo = 'https://github.com/greenfield-inc/everyport'
$installDir = if ($env:EVERYPORT_INSTALL_DIR) { $env:EVERYPORT_INSTALL_DIR } else { Join-Path $HOME '.local\bin' }

# The OS architecture, also from 32-bit PowerShell on 64-bit Windows. Windows PowerShell 5.1
# can't be trusted with RuntimeInformation.OSArchitecture, which can come back empty.
function Get-WindowsArch {
  $cpu = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
  switch ($cpu) {
    'AMD64' { 'x86_64' }
    'ARM64' { 'aarch64' }
    default { throw "everyport install: unsupported CPU: '$cpu'" }
  }
}
$asset = "everyport-$(Get-WindowsArch)-pc-windows-msvc.exe"

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
  # Accept matches curl. Without it, GitHub answers a missing file with a large HTML page, which
  # Windows PowerShell 5.1 reports as "The connection was closed unexpectedly" instead of a 404.
  function Save-ReleaseFile($name, $path) {
    Invoke-WebRequest -UseBasicParsing -Headers @{ Accept = '*/*' } -Uri "$base/$name" -OutFile $path
  }

  $sumsPath = Join-Path $tmp 'SHA256SUMS'
  try { Save-ReleaseFile 'SHA256SUMS' $sumsPath }
  catch { throw "everyport install: no release found at $base. If Everyport hasn't had its first release yet, check $repo/releases.`n$($_.Exception.Message)" }
  Write-Host "Downloading $asset from $base"
  $binary = Join-Path $tmp 'everyport.exe'
  Save-ReleaseFile $asset $binary

  $expected = $null
  foreach ($line in Get-Content $sumsPath) {
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
