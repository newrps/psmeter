# Psmeter 📊

스크립트 한 줄로 붙이는 **셀프호스팅 분석 서비스**.
GA 대체 — 쿠키 없음, 실시간, 가벼움, 단일 바이너리.

```
[브라우저]
   ↓ <script src=".../tracker/psmeter.js" data-site="example.com">
[psmeter 서버 🦀] ─→ [SQLite 임베드] (data/psmeter.db)
   ↓
[관리자 대시보드 SvelteKit] (서버에 임베드)
```

## 차별점 (vs Google Analytics)

| 항목 | GA | psmeter |
|---|---|---|
| **JS 크기** | 30~50 KB | **<1 KB** |
| **AdBlocker** | ~30% 차단됨 | 1st-party 도메인 → 거의 100% 잡힘 |
| **실시간** | 24h 지연 | **즉시** (3초 폴링/WS) |
| **쿠키** | 사용 | **없음** (해시 + 일별 회전) |
| **데이터 소유** | Google | 본인 SQLite 파일 |
| **설치** | 코드 박기 | **단일 바이너리 + 5분** |

## 차별점 (vs 다른 셀프호스팅 도구)

| 도구 | DB | 설치 진입장벽 |
|---|---|---|
| Plausible (OSS) | PostgreSQL + ClickHouse | ⭐⭐ (둘 다 깔아야) |
| Umami | PostgreSQL/MySQL | ⭐⭐⭐ docker-compose |
| GoatCounter | SQLite | ⭐⭐⭐⭐⭐ |
| **psmeter** | **SQLite 임베드** | **⭐⭐⭐⭐⭐ 바이너리 1개** |

## 빠른 시작

### Windows
1. `dist/psmeter-server.exe` 더블클릭 (또는 빌드: `cd server && cargo build --release`)
2. 브라우저로 `http://localhost:3100` 접속
3. 첫 화면에서 관리자 토큰 설정 → 사이트 등록 → 설치 코드 복사

### Linux
```bash
chmod +x psmeter-server-linux
PSMETER_PORT=3100 ./psmeter-server-linux
```

### 임베드 (사이트에 박기)

```html
<script src="https://your-psmeter.com/tracker/psmeter.js"
        data-server="https://your-psmeter.com"
        data-site="example.com" async></script>
```

자동으로 추적:
- 페이지뷰 (SPA pushState 포함)
- 디바이스 (mobile/tablet/desktop)
- 브라우저 (Chrome/Firefox/Safari/Edge)
- Referrer
- Unique visitor (IP+UA+일자 SHA-256 → 일별 회전)

커스텀 이벤트:
```js
window.psmeter('signup_click');
window.psmeter('purchase');
```

## 환경변수

| 변수 | 기본값 | 설명 |
|---|---|---|
| `PSMETER_PORT` | `3100` | 리스닝 포트 |
| `PSMETER_BIND` | `0.0.0.0` | 바인딩 주소 (운영: `127.0.0.1`) |
| `PSMETER_DATA_DIR` | `./data` | SQLite + config 저장 경로 |
| `PSMETER_ADMIN_TOKEN` | (config.json) | 환경변수로 토큰 강제 지정 |

## API

### 공개 (tracker가 사용)
| Method | Path | 설명 |
|---|---|---|
| `POST` | `/api/track` | 이벤트 수집 |
| `GET`  | `/api/health` | 헬스체크 |
| `GET`  | `/tracker/psmeter.js` | 트래커 JS |

Body (POST /api/track):
```json
{ "site": "example.com", "kind": "pageview", "path": "/about", "referrer": "https://google.com" }
```

### 관리자 (Bearer 토큰)
| Method | Path | 설명 |
|---|---|---|
| `GET`    | `/api/admin/sites` | 사이트 목록 |
| `POST`   | `/api/admin/sites` | 사이트 추가 |
| `DELETE` | `/api/admin/sites/:domain` | 사이트 삭제 |
| `GET`    | `/api/admin/stats?site=&from=&to=` | 통계 (최근 24h 기본) |
| `GET`    | `/api/admin/live?site=` | 지금 보고있는 사람 수 |
| `GET`    | `/api/admin/ws?token=` | 실시간 사이트별 active visitors |

## 폴더 구조

```
psmeter/
├── server/                Rust(axum) 백엔드
│   ├── src/main.rs        라우터 + 핸들러 + 임베드 자산
│   └── src/store.rs       Store trait + SqliteStore
├── dashboard-sveltekit/   관리자 대시보드 SPA
├── tracker/psmeter.js     1KB 임베드 JS
├── demo/                  데모 페이지
├── dist/                  바이너리
├── deploy.ps1             빌드/배포 한방 스크립트
└── README.md
```

## 데이터 모델 (SQLite)

```sql
CREATE TABLE sites (
  domain     TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE events (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  site         TEXT NOT NULL,
  kind         TEXT NOT NULL,
  path         TEXT NOT NULL,
  referrer     TEXT,
  visitor_hash TEXT NOT NULL,
  country      TEXT,
  device       TEXT,
  browser      TEXT,
  ts_ms        INTEGER NOT NULL
);
```

WAL 모드 + 인덱스 → 단일 코어로 초당 10만 INSERT 가능.

## 확장성

- **시작 ~ 일 1천만 이벤트**: SQLite (현재)
- **일 1억+**: ClickHouse 마이그 (Store trait 의 새 impl)
- 코드의 SQL 부분만 바꾸면 됨 — 인터페이스는 안 바뀜

## 빌드

Rust 1.83+ 필요.

```bash
# Dashboard 먼저
cd dashboard-sveltekit && npm install && npm run build

# 그 다음 Rust 서버 (dashboard build 를 임베드)
cd ../server && cargo build --release
```

**한 방에**: `.\deploy.ps1` 실행 (Oracle/NAS 서버에 배포까지)

## 라이선스

MIT
