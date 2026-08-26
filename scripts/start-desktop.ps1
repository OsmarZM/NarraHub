param(
  [ValidateSet('dev', 'build', 'qualification')]
  [string]$Mode = 'dev'
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$preferredNode = 'D:\DevTools\Node\node-v24.16.0-win-x64'
$preferredTarget = 'D:\DevTools\NarraHubTarget'
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
$mingwBin = Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.MSVCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin'

$pathEntries = @($preferredNode, $cargoBin, $mingwBin) | Where-Object { Test-Path -LiteralPath $_ }
$env:Path = (($pathEntries + @($env:Path)) -join ';')

$cargoTarget = if ($env:NARRAHUB_CARGO_TARGET_DIR) {
  $env:NARRAHUB_CARGO_TARGET_DIR
} elseif (Test-Path -LiteralPath (Split-Path -Parent $preferredTarget)) {
  $preferredTarget
} else {
  Join-Path $projectRoot '.native-target'
}

New-Item -ItemType Directory -Force -Path $cargoTarget | Out-Null
$env:CARGO_TARGET_DIR = $cargoTarget

$nodeVersion = (& node --version)
Write-Host '=========================================' -ForegroundColor Cyan
Write-Host " NarraHub Desktop - $Mode" -ForegroundColor Cyan
Write-Host " Node: $nodeVersion" -ForegroundColor DarkCyan
Write-Host " Cache Rust: $cargoTarget" -ForegroundColor DarkCyan
Write-Host '=========================================' -ForegroundColor Cyan

Push-Location $projectRoot
try {
  $tauriArgs = @($Mode)
  if ($Mode -eq 'build') {
    $tauriArgs += @('--config', 'src-tauri/tauri.production.conf.json')
  } elseif ($Mode -eq 'qualification') {
    $tauriArgs = @('dev', '--config', 'src-tauri/tauri.qualification.conf.json')
  }
  & npm.cmd run tauri -- @tauriArgs
  $nativeExitCode = $LASTEXITCODE
} finally {
  Pop-Location
}

exit $nativeExitCode
