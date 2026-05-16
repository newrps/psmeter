# Psmeter deploy script
# Usage:
#   .\deploy.ps1                    # 빌드 + 배포
#   .\deploy.ps1 -NoBuild           # 기존 바이너리로 배포만
#   .\deploy.ps1 -NoGit             # git commit/push 건너뜀
#   .\deploy.ps1 -Message "fix X"   # 커밋 메시지 지정
#   .\deploy.ps1 -DryRun            # 미리보기

param(
  [string]$Message = "",
  [switch]$NoBuild,
  [switch]$NoGit,
  [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot

# ---- 설정 (Oracle Free Tier 인스턴스) ----
# SSH config alias 사용 — ~/.ssh/config 의 "Host psmeter" 또는 "Host pscheckwait" 등록 필요
$SSHHost    = if ($env:PSMETER_SSH_HOST) { $env:PSMETER_SSH_HOST } else { "psmeter" }
$RemotePath = "/opt/psmeter/psmeter-server-linux"
$Binary     = "$root\server\target-linux\release\psmeter-server"
$HealthUrl  = if ($env:PSMETER_HEALTH_URL) { $env:PSMETER_HEALTH_URL } else { "https://meter.zam.kr/api/health" }

function Step($msg) { Write-Host ""; Write-Host "==> $msg" -ForegroundColor Cyan }
function Skip($msg) { Write-Host "    [skip] $msg" -ForegroundColor DarkGray }
function Run($cmd) {
  if ($DryRun) { Write-Host "    [dry] $cmd" -ForegroundColor Yellow; return }
  Write-Host "    $cmd" -ForegroundColor DarkGray
  Invoke-Expression $cmd
  if ($LASTEXITCODE -ne 0) { throw "command failed: $cmd" }
}

# ---- 1. Git ----
Step "Git"
if ($NoGit) {
  Skip "git commit/push (-NoGit)"
} else {
  Set-Location $root
  $changes = git status --short
  if (-not $changes) {
    Skip "변경사항 없음"
  } else {
    Write-Host $changes
    $msg = if ($Message) { $Message } else { "Deploy at $(Get-Date -Format 'yyyy-MM-dd HH:mm')" }
    Run "git add -A"
    if ($DryRun) {
      Write-Host "    [dry] git commit -m `"$msg`""
      Write-Host "    [dry] git push"
    } else {
      git commit -m $msg
      if ($LASTEXITCODE -ne 0) { throw "git commit failed" }
      git push
      if ($LASTEXITCODE -ne 0) { throw "git push failed" }
    }
  }
}

# ---- 2a. Build dashboard ----
Step "Build dashboard (SvelteKit)"
if ($NoBuild) {
  Skip "npm run build (-NoBuild)"
} else {
  Run "npm --prefix `"$root\dashboard-sveltekit`" run build"
}

# ---- 2b. Build Rust server (Linux binary via Docker, embeds dashboard) ----
Step "Build Rust server (Linux binary via Docker)"
if ($NoBuild) {
  Skip "cargo build (-NoBuild)"
  if (-not (Test-Path $Binary)) {
    throw "바이너리 없음: $Binary — 먼저 빌드 필요"
  }
} else {
  Run "docker run --rm -v `"${root}:/work`" -w /work/server rust:1-bookworm cargo build --release --target-dir target-linux"
}

if (-not $DryRun) {
  $size = (Get-Item $Binary).Length / 1MB
  Write-Host ("    바이너리: {0} ({1:N1} MB)" -f $Binary, $size)
}

# ---- 3. Upload ----
Step "Upload → $SSHHost"
Run "scp `"$Binary`" `"${SSHHost}:/tmp/psmeter-server-new`""

# ---- 4. Restart ----
Step "Restart psmeter service"
$remoteCmd = "sudo systemctl stop psmeter && sudo mv /tmp/psmeter-server-new $RemotePath && sudo chmod +x $RemotePath && sudo systemctl start psmeter && sleep 2 && sudo systemctl is-active psmeter"
Run "ssh $SSHHost `"$remoteCmd`""

# ---- 5. Health check ----
Step "Health check"
if ($DryRun) {
  Write-Host "    [dry] GET $HealthUrl" -ForegroundColor Yellow
} else {
  try {
    $r = Invoke-WebRequest $HealthUrl -UseBasicParsing -TimeoutSec 10
    Write-Host "    HTTP $($r.StatusCode) — $($r.Content)" -ForegroundColor Green
  } catch {
    Write-Host "    헬스체크 실패: $_" -ForegroundColor Red
    throw
  }
}

Write-Host ""
Write-Host "✓ Done." -ForegroundColor Green
