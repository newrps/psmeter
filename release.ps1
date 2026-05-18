# Psmeter GitHub Release 자동 빌드 + 업로드.
#
# 사용법:
#   .\release.ps1 -Tag v0.2.0
#   .\release.ps1 -Tag v0.2.0 -NoArm       # x86_64만
#   .\release.ps1 -Tag v0.2.0 -Notes "릴리즈 노트"
#
# 사전조건:
#   - gh CLI 설치 + 로그인 (gh auth status)
#   - docker (multi-arch: docker buildx 또는 qemu)
#   - dashboard-sveltekit/build/ 이미 빌드되어 있어야 (deploy.ps1 또는 npm run build)

param(
  [Parameter(Mandatory=$true)]
  [string]$Tag,
  [string]$Notes = "",
  [switch]$NoArm,
  [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot

function Step($msg) { Write-Host ""; Write-Host "==> $msg" -ForegroundColor Cyan }
function Run($cmd) {
  if ($DryRun) { Write-Host "  [dry] $cmd" -ForegroundColor Yellow; return }
  Write-Host "  $cmd" -ForegroundColor DarkGray
  Invoke-Expression $cmd
  if ($LASTEXITCODE -ne 0) { throw "command failed: $cmd" }
}

# ---- 사전 체크 ----
Step "사전 체크"
if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { throw "gh CLI 필요" }
if (-not (Get-Command docker -ErrorAction SilentlyContinue)) { throw "docker 필요" }
if (-not (Test-Path "$root\dashboard-sveltekit\build\index.html")) {
  Write-Host "  dashboard build 누락. npm run build 실행" -ForegroundColor Yellow
  Run "npm --prefix `"$root\dashboard-sveltekit`" run build"
}

# ---- x86_64 빌드 ----
Step "x86_64 빌드"
Run "docker run --rm -v `"${root}:/work`" -w /work/server rust:1-bookworm cargo build --release --target-dir target-linux"
$x86 = "$root\server\target-linux\release\psmeter-server"
if (-not (Test-Path $x86)) { throw "x86_64 바이너리 생성 안됨: $x86" }
$x86Final = "$root\server\target-linux\release\psmeter-server-linux-x86_64"
Copy-Item $x86 $x86Final -Force

# ---- ARM64 빌드 (qemu) ----
$arm64Final = "$root\server\target-linux-arm64\release\psmeter-server-linux-aarch64"
if ($NoArm) {
  Write-Host "  [skip] ARM 빌드 (-NoArm)" -ForegroundColor DarkGray
} else {
  Step "aarch64 빌드 (qemu emulation, 5~15분 소요)"
  Run "docker run --rm --platform linux/arm64 -v `"${root}:/work`" -w /work/server rust:1-bookworm cargo build --release --target-dir target-linux-arm64"
  $arm64 = "$root\server\target-linux-arm64\release\psmeter-server"
  if (-not (Test-Path $arm64)) { throw "ARM 바이너리 생성 안됨: $arm64" }
  Copy-Item $arm64 $arm64Final -Force
}

# ---- gh release ----
Step "GitHub Release 생성: $Tag"
$noteFile = "$env:TEMP\psmeter-release-notes.txt"
if ($Notes) {
  Set-Content $noteFile $Notes -Encoding UTF8
} else {
  $log = git log --oneline -10
  "Auto-generated release.`n`nRecent commits:`n$log" | Set-Content $noteFile -Encoding UTF8
}

$assets = @($x86Final)
if (-not $NoArm) { $assets += $arm64Final }
$assetsStr = ($assets | ForEach-Object { "`"$_`"" }) -join ' '

Run "gh release create $Tag --title `"$Tag`" --notes-file `"$noteFile`" $assetsStr"

Write-Host ""
Write-Host "✓ Release $Tag 완료" -ForegroundColor Green
Write-Host "  설치 한줄: " -NoNewline
Write-Host "curl -fsSL https://raw.githubusercontent.com/newrps/psmeter/main/install.sh | sudo bash" -ForegroundColor Cyan
