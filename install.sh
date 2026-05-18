#!/usr/bin/env bash
# Psmeter 단일 파일 설치 스크립트.
#
# 사용법:
#   curl -fsSL https://raw.githubusercontent.com/newrps/psmeter/main/install.sh | sudo bash
#   curl -fsSL https://raw.githubusercontent.com/newrps/psmeter/main/install.sh | sudo bash -s -- meter.example.com
#
# 도메인 인자를 주면 Caddy 자동 reverse proxy + Let's Encrypt 인증서까지 설정한다.
# 도메인 없으면 127.0.0.1:3100 으로 바인드만. 사용자가 따로 reverse proxy 붙임.

set -euo pipefail

REPO="${PSMETER_REPO:-newrps/psmeter}"
INSTALL_DIR="${PSMETER_INSTALL_DIR:-/opt/psmeter}"
DATA_DIR="${PSMETER_DATA_DIR:-/var/lib/psmeter}"
SERVICE="psmeter"
BIN_PATH="$INSTALL_DIR/psmeter-server"
DOMAIN="${1:-}"

log()  { echo -e "\033[36m==>\033[0m $*"; }
ok()   { echo -e "\033[32m OK\033[0m $*"; }
die()  { echo -e "\033[31mERR\033[0m $*" >&2; exit 1; }

[ "$(id -u)" -eq 0 ] || die "root 권한 필요. sudo로 실행하세요."

# ---- OS/arch 감지 ----
case "$(uname -s)" in
  Linux) ;;
  *) die "Linux 전용 (현재: $(uname -s))" ;;
esac

case "$(uname -m)" in
  x86_64|amd64)        ARCH=x86_64 ;;
  aarch64|arm64)       ARCH=aarch64 ;;
  *) die "미지원 아키텍처: $(uname -m)" ;;
esac

log "OS: Linux $ARCH"

# ---- 의존성 (curl 외 추가 없음) ----
command -v curl >/dev/null || die "curl 설치 필요: apt install -y curl"

# ---- 최신 release 바이너리 URL ----
log "최신 release 조회 ($REPO)"
API_URL="https://api.github.com/repos/$REPO/releases/latest"
RESP=$(curl -fsSL "$API_URL" 2>/dev/null || echo "")
if [ -z "$RESP" ]; then
  die "release API 호출 실패. repo 이름($REPO) 또는 네트워크 확인."
fi
BIN_URL=$(printf '%s' "$RESP" | grep -oE '"browser_download_url":[[:space:]]*"[^"]+"' \
          | grep -oE 'https://[^"]+psmeter-server-linux-'$ARCH'[^"]*' \
          | head -1 || true)
[ -n "$BIN_URL" ] || die "linux-$ARCH 바이너리 release에 없음."

# ---- 다운로드 ----
mkdir -p "$INSTALL_DIR" "$DATA_DIR"
log "바이너리 다운로드: $BIN_URL"
curl -fsSL "$BIN_URL" -o "$BIN_PATH.new"
chmod +x "$BIN_PATH.new"
mv "$BIN_PATH.new" "$BIN_PATH"

# ---- 사용자 (옵션) ----
if ! id psmeter >/dev/null 2>&1; then
  useradd --system --home-dir "$DATA_DIR" --shell /usr/sbin/nologin psmeter || true
fi
chown -R psmeter:psmeter "$DATA_DIR" 2>/dev/null || true

# ---- systemd unit ----
log "systemd unit 작성"
cat > "/etc/systemd/system/${SERVICE}.service" <<EOF
[Unit]
Description=Psmeter analytics server
After=network.target

[Service]
Type=simple
User=psmeter
Group=psmeter
Environment=PSMETER_DATA_DIR=$DATA_DIR
Environment=PSMETER_PORT=3100
Environment=PSMETER_BIND=127.0.0.1
ExecStart=$BIN_PATH
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=$DATA_DIR

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable --now "$SERVICE"
sleep 1
systemctl is-active --quiet "$SERVICE" || die "psmeter 시작 실패. journalctl -u psmeter -n 50"
ok "psmeter 서비스 시작됨"

# ---- Caddy 자동 설정 (도메인 인자 있을 때) ----
if [ -n "$DOMAIN" ]; then
  log "Caddy 설치 + reverse proxy 설정: $DOMAIN"
  if ! command -v caddy >/dev/null; then
    apt-get update -qq
    apt-get install -y -qq debian-keyring debian-archive-keyring apt-transport-https curl gnupg
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
      | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
      | tee /etc/apt/sources.list.d/caddy-stable.list >/dev/null
    apt-get update -qq
    apt-get install -y -qq caddy
  fi

  CADDYFILE="/etc/caddy/Caddyfile"
  if ! grep -q "$DOMAIN" "$CADDYFILE" 2>/dev/null; then
    cat >> "$CADDYFILE" <<EOF

$DOMAIN {
    reverse_proxy 127.0.0.1:3100
}
EOF
    systemctl reload caddy || systemctl restart caddy
  fi
  ok "Caddy 활성화. 도메인이 이 서버로 DNS 잡혀있으면 자동 https 발급됨."
fi

# ---- 안내 ----
echo
echo "===================================================="
ok "psmeter 설치 완료"
echo "  - 바이너리:  $BIN_PATH"
echo "  - 데이터:    $DATA_DIR"
echo "  - 서비스:    systemctl status $SERVICE"
echo "  - 로그:      journalctl -u $SERVICE -f"
if [ -n "$DOMAIN" ]; then
  echo "  - 대시보드:  https://$DOMAIN/  ← 첫 접속에서 admin 토큰 설정"
else
  echo "  - 대시보드:  http://127.0.0.1:3100/  (reverse proxy 필요)"
  echo "    Caddy 자동 설정: install.sh 다시 실행하며 도메인 인자 주기"
  echo "    예: sudo bash install.sh meter.example.com"
fi
echo
echo "텔레그램 디스크 알림 (선택):"
echo "  sudo systemctl edit $SERVICE   # override.conf에 아래 추가:"
echo "  [Service]"
echo "  Environment=PSMETER_TELEGRAM_BOT_TOKEN=xxx"
echo "  Environment=PSMETER_TELEGRAM_CHAT_ID=yyy"
echo "  Environment=PSMETER_DISK_ALERT_PCT=80"
echo "  sudo systemctl restart $SERVICE"
echo "===================================================="
