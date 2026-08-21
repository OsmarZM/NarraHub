param(
  [ValidateRange(1024, 65535)]
  [int]$Port = 8787
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$preferredCloudflared = 'D:\DevTools\Cloudflared\cloudflared.exe'
$cloudflaredCommand = Get-Command cloudflared.exe -ErrorAction SilentlyContinue
$cloudflaredPath = if (Test-Path -LiteralPath $preferredCloudflared) {
  $preferredCloudflared
} elseif ($cloudflaredCommand) {
  $cloudflaredCommand.Source
} else {
  throw 'cloudflared não foi encontrado. Instale-o em D:\DevTools\Cloudflared ou adicione cloudflared.exe ao PATH.'
}

$runtimeRoot = if (Test-Path -LiteralPath 'D:\') { 'D:\DevTools\NarraHubRuntime' } else { Join-Path $projectRoot '.runtime' }
New-Item -ItemType Directory -Force -Path $runtimeRoot | Out-Null
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$tunnelLog = Join-Path $runtimeRoot "quick-tunnel-$stamp.log"
$tunnel = $null

try {
  $tunnel = Start-Process -FilePath $cloudflaredPath -ArgumentList @(
    'tunnel', '--url', "http://127.0.0.1:$Port", '--no-autoupdate'
  ) -RedirectStandardError $tunnelLog -PassThru -WindowStyle Hidden

  $deadline = (Get-Date).AddSeconds(35)
  $publicUrl = $null
  while ((Get-Date) -lt $deadline -and -not $publicUrl) {
    if ($tunnel.HasExited) { throw "O Quick Tunnel encerrou antes de publicar a URL. Consulte $tunnelLog" }
    if (Test-Path -LiteralPath $tunnelLog) {
      $contents = [string](Get-Content -LiteralPath $tunnelLog -Raw -ErrorAction SilentlyContinue)
      $match = [regex]::Match($contents, 'https://[a-z0-9-]+\.trycloudflare\.com')
      if ($match.Success) { $publicUrl = $match.Value }
    }
    if (-not $publicUrl) { Start-Sleep -Milliseconds 500 }
  }
  if (-not $publicUrl) { throw "O Quick Tunnel não retornou uma URL em 35 segundos. Consulte $tunnelLog" }

  $env:NARRAHUB_SHARE_PORT = [string]$Port
  $env:NARRAHUB_SHARE_PUBLIC_URL = $publicUrl
  Write-Host "NarraHub Share temporário: $publicUrl"
  Write-Host 'Mantenha este terminal aberto. A URL muda quando o processo for reiniciado.'
  & node (Join-Path $projectRoot 'services\share-api\src\server.mjs')
} finally {
  if ($tunnel -and -not $tunnel.HasExited) { Stop-Process -Id $tunnel.Id -Force }
}
