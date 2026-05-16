use async_trait::async_trait;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub type SqlitePool = Pool<SqliteConnectionManager>;

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub domain: String,
    pub name: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub site: String,
    pub kind: String,
    pub path: String,
    pub referrer: Option<String>,
    pub visitor_hash: String,
    pub country: Option<String>,
    pub device: Option<String>,
    pub browser: Option<String>,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub pageviews: i64,
    pub visitors: i64,
    pub top_pages: Vec<(String, i64)>,
    pub top_referrers: Vec<(String, i64)>,
    pub countries: Vec<(String, i64)>,
    pub devices: Vec<(String, i64)>,
    pub source: String, // "raw" | "hourly" | "daily" — 디버깅/관측용
}

#[async_trait]
pub trait Store: Send + Sync {
    async fn ensure_site(&self, domain: &str, name: &str) -> rusqlite::Result<()>;
    async fn list_sites(&self) -> rusqlite::Result<Vec<Site>>;
    async fn delete_site(&self, domain: &str) -> rusqlite::Result<()>;
    async fn record_event(&self, ev: &Event) -> rusqlite::Result<()>;
    async fn stats(&self, site: &str, from_ms: i64, to_ms: i64) -> rusqlite::Result<Stats>;
    async fn active_visitors(&self, site: &str, since_ms: i64) -> rusqlite::Result<i64>;
    async fn rollup_hourly(&self) -> rusqlite::Result<usize>;
    async fn rollup_daily(&self) -> rusqlite::Result<usize>;
}

pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let manager = SqliteConnectionManager::file(path)
            .with_init(|c| {
                c.execute_batch(
                    "PRAGMA journal_mode = WAL;
                     PRAGMA synchronous = NORMAL;
                     PRAGMA cache_size = -64000;
                     PRAGMA temp_store = MEMORY;
                     PRAGMA foreign_keys = ON;",
                )
            });
        let pool = Pool::builder()
            .max_size(8)
            .build(manager)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

        let store = Self { pool };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> rusqlite::Result<()> {
        let conn = self.conn()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sites (
                domain      TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                created_at  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS events (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                site          TEXT NOT NULL,
                kind          TEXT NOT NULL,
                path          TEXT NOT NULL,
                referrer      TEXT,
                visitor_hash  TEXT NOT NULL,
                country       TEXT,
                device        TEXT,
                browser       TEXT,
                ts_ms         INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_events_site_ts ON events(site, ts_ms);
            CREATE INDEX IF NOT EXISTS idx_events_site_path ON events(site, path);
            CREATE INDEX IF NOT EXISTS idx_events_site_visitor ON events(site, visitor_hash);

            CREATE TABLE IF NOT EXISTS events_hourly (
                site       TEXT NOT NULL,
                ts_hour    INTEGER NOT NULL,
                path       TEXT NOT NULL,
                referrer   TEXT NOT NULL,
                country    TEXT NOT NULL,
                device     TEXT NOT NULL,
                pageviews  INTEGER NOT NULL,
                PRIMARY KEY (site, ts_hour, path, referrer, country, device)
            );
            CREATE INDEX IF NOT EXISTS idx_eh_site_ts ON events_hourly(site, ts_hour);

            CREATE TABLE IF NOT EXISTS visitors_hourly (
                site         TEXT NOT NULL,
                ts_hour      INTEGER NOT NULL,
                visitor_hash TEXT NOT NULL,
                PRIMARY KEY (site, ts_hour, visitor_hash)
            );

            CREATE TABLE IF NOT EXISTS events_daily (
                site       TEXT NOT NULL,
                ts_day     INTEGER NOT NULL,
                path       TEXT NOT NULL,
                referrer   TEXT NOT NULL,
                country    TEXT NOT NULL,
                device     TEXT NOT NULL,
                pageviews  INTEGER NOT NULL,
                PRIMARY KEY (site, ts_day, path, referrer, country, device)
            );
            CREATE INDEX IF NOT EXISTS idx_ed_site_ts ON events_daily(site, ts_day);

            CREATE TABLE IF NOT EXISTS visitors_daily (
                site         TEXT NOT NULL,
                ts_day       INTEGER NOT NULL,
                visitor_hash TEXT NOT NULL,
                PRIMARY KEY (site, ts_day, visitor_hash)
            );

            CREATE TABLE IF NOT EXISTS rollup_state (
                name       TEXT PRIMARY KEY,
                last_ts_ms INTEGER NOT NULL DEFAULT 0
            );
            ",
        )?;
        Ok(())
    }

    fn conn(&self) -> rusqlite::Result<r2d2::PooledConnection<SqliteConnectionManager>> {
        self.pool
            .get()
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
    }

    fn stats_raw(
        &self,
        conn: &rusqlite::Connection,
        site: &str,
        from_ms: i64,
        to_ms: i64,
    ) -> rusqlite::Result<Stats> {
        let pageviews: i64 = conn.query_row(
            "SELECT COUNT(*) FROM events
             WHERE site = ?1 AND kind = 'pageview' AND ts_ms BETWEEN ?2 AND ?3",
            params![site, from_ms, to_ms],
            |r| r.get(0),
        )?;

        let visitors: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT visitor_hash) FROM events
             WHERE site = ?1 AND ts_ms BETWEEN ?2 AND ?3",
            params![site, from_ms, to_ms],
            |r| r.get(0),
        )?;

        let top_pages = top_n(
            conn,
            "SELECT path, COUNT(*) c FROM events
             WHERE site = ?1 AND kind='pageview' AND ts_ms BETWEEN ?2 AND ?3
             GROUP BY path ORDER BY c DESC LIMIT 10",
            site, from_ms, to_ms,
        )?;

        let top_referrers = top_n(
            conn,
            "SELECT COALESCE(referrer,'(direct)'), COUNT(*) c FROM events
             WHERE site = ?1 AND kind='pageview' AND ts_ms BETWEEN ?2 AND ?3
             GROUP BY referrer ORDER BY c DESC LIMIT 10",
            site, from_ms, to_ms,
        )?;

        let countries = top_n(
            conn,
            "SELECT COALESCE(country,'(unknown)'), COUNT(*) c FROM events
             WHERE site = ?1 AND kind='pageview' AND ts_ms BETWEEN ?2 AND ?3
             GROUP BY country ORDER BY c DESC LIMIT 10",
            site, from_ms, to_ms,
        )?;

        let devices = top_n(
            conn,
            "SELECT COALESCE(device,'(unknown)'), COUNT(*) c FROM events
             WHERE site = ?1 AND kind='pageview' AND ts_ms BETWEEN ?2 AND ?3
             GROUP BY device ORDER BY c DESC LIMIT 10",
            site, from_ms, to_ms,
        )?;

        Ok(Stats {
            pageviews,
            visitors,
            top_pages,
            top_referrers,
            countries,
            devices,
            source: "raw".to_string(),
        })
    }

    // 사전집계 테이블 + raw 보충으로 stats 계산.
    // cutoff = 마지막 롤업 ts_ms. 그보다 앞은 agg에서, 그보다 뒤는 raw에서 가져와 합산.
    // 이렇게 해야 오늘/지금 시각 데이터가 락업 시점에 누락되지 않는다.
    fn stats_combined(
        &self,
        conn: &rusqlite::Connection,
        site: &str,
        from_ms: i64,
        to_ms: i64,
        cutoff_ms: i64,
        events_table: &str,
        visitors_table: &str,
        ts_col: &str,
        source: &str,
    ) -> rusqlite::Result<Stats> {
        // agg 범위: [from, min(to, cutoff))   (cutoff는 다음 버킷 시작 = 미rollup 경계)
        // raw 범위: [max(from, cutoff), to]
        let agg_to = cutoff_ms.min(to_ms);
        let raw_from = cutoff_ms.max(from_ms);
        // BETWEEN은 닫힌 구간. agg는 (cutoff 미만)만 봐야 하므로 agg_to - 1 사용.
        let agg_to_inc = agg_to - 1;

        let pageviews: i64 = conn.query_row(
            &format!(
                "SELECT
                   COALESCE((SELECT SUM(pageviews) FROM {events_table}
                             WHERE site=?1 AND {ts_col} BETWEEN ?2 AND ?3), 0)
                 + COALESCE((SELECT COUNT(*) FROM events
                             WHERE site=?1 AND kind='pageview'
                             AND ts_ms BETWEEN ?4 AND ?5), 0)"
            ),
            params![site, from_ms, agg_to_inc, raw_from, to_ms],
            |r| r.get(0),
        )?;

        let visitors: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(DISTINCT visitor_hash) FROM (
                    SELECT visitor_hash FROM {visitors_table}
                    WHERE site=?1 AND {ts_col} BETWEEN ?2 AND ?3
                    UNION ALL
                    SELECT visitor_hash FROM events
                    WHERE site=?1 AND ts_ms BETWEEN ?4 AND ?5
                 )"
            ),
            params![site, from_ms, agg_to_inc, raw_from, to_ms],
            |r| r.get(0),
        )?;

        let top_pages = top_n_combined(
            conn, events_table, ts_col, "path", "kind='pageview'",
            site, from_ms, agg_to_inc, raw_from, to_ms,
        )?;
        let top_referrers = top_n_combined(
            conn, events_table, ts_col, "referrer", "kind='pageview'",
            site, from_ms, agg_to_inc, raw_from, to_ms,
        )?;
        let countries = top_n_combined(
            conn, events_table, ts_col, "country", "kind='pageview'",
            site, from_ms, agg_to_inc, raw_from, to_ms,
        )?;
        let devices = top_n_combined(
            conn, events_table, ts_col, "device", "kind='pageview'",
            site, from_ms, agg_to_inc, raw_from, to_ms,
        )?;

        Ok(Stats {
            pageviews,
            visitors,
            top_pages,
            top_referrers,
            countries,
            devices,
            source: source.to_string(),
        })
    }

    fn last_rollup_ts(&self, conn: &rusqlite::Connection, name: &str) -> i64 {
        conn.query_row(
            "SELECT last_ts_ms FROM rollup_state WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }
}

#[async_trait]
impl Store for SqliteStore {
    async fn ensure_site(&self, domain: &str, name: &str) -> rusqlite::Result<()> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT OR IGNORE INTO sites (domain, name, created_at) VALUES (?1, ?2, ?3)",
            params![domain, name, now],
        )?;
        Ok(())
    }

    async fn list_sites(&self) -> rusqlite::Result<Vec<Site>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT domain, name, created_at FROM sites ORDER BY created_at DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Site {
                    domain: r.get(0)?,
                    name: r.get(1)?,
                    created_at: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    async fn delete_site(&self, domain: &str) -> rusqlite::Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM sites WHERE domain = ?1", params![domain])?;
        conn.execute("DELETE FROM events WHERE site = ?1", params![domain])?;
        conn.execute("DELETE FROM events_hourly WHERE site = ?1", params![domain])?;
        conn.execute("DELETE FROM visitors_hourly WHERE site = ?1", params![domain])?;
        conn.execute("DELETE FROM events_daily WHERE site = ?1", params![domain])?;
        conn.execute("DELETE FROM visitors_daily WHERE site = ?1", params![domain])?;
        Ok(())
    }

    async fn record_event(&self, ev: &Event) -> rusqlite::Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO events
                (site, kind, path, referrer, visitor_hash, country, device, browser, ts_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                ev.site,
                ev.kind,
                ev.path,
                ev.referrer,
                ev.visitor_hash,
                ev.country,
                ev.device,
                ev.browser,
                ev.ts_ms,
            ],
        )?;
        Ok(())
    }

    // 시간 범위에 따라 자동 라우팅:
    //   > 7일 → daily, > 1일 → hourly, 그 외 → raw
    // 사전집계 테이블이 비어 있어도 안전 (SUM=0 반환). 첫 롤업이 돌기 전엔 raw가 더 정확하지만
    // 그 시점엔 raw도 빠르니 무관.
    async fn stats(&self, site: &str, from_ms: i64, to_ms: i64) -> rusqlite::Result<Stats> {
        let conn = self.conn()?;
        let range = to_ms - from_ms;
        if range > 7 * DAY_MS {
            let cutoff = self.last_rollup_ts(&conn, "daily");
            let from_d = floor_to_bucket(from_ms, DAY_MS);
            self.stats_combined(&conn, site, from_d, to_ms, cutoff,
                "events_daily", "visitors_daily", "ts_day", "daily")
        } else if range > DAY_MS {
            let cutoff = self.last_rollup_ts(&conn, "hourly");
            let from_h = floor_to_bucket(from_ms, HOUR_MS);
            self.stats_combined(&conn, site, from_h, to_ms, cutoff,
                "events_hourly", "visitors_hourly", "ts_hour", "hourly")
        } else {
            self.stats_raw(&conn, site, from_ms, to_ms)
        }
    }

    async fn active_visitors(&self, site: &str, since_ms: i64) -> rusqlite::Result<i64> {
        let conn = self.conn()?;
        let n: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT visitor_hash) FROM events
             WHERE site = ?1 AND ts_ms >= ?2",
            params![site, since_ms],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    // 마지막 hourly_rollup ts_ms 이후 ~ 직전 시간 시작 전까지의 raw events를
    // events_hourly/visitors_hourly로 집계 (idempotent, ON CONFLICT 누적).
    // 반환: 처리한 raw events 개수.
    async fn rollup_hourly(&self) -> rusqlite::Result<usize> {
        let mut conn = self.conn()?;
        let now = chrono::Utc::now().timestamp_millis();
        let cur_hour = floor_to_bucket(now, HOUR_MS);

        let last_ts: i64 = conn
            .query_row(
                "SELECT last_ts_ms FROM rollup_state WHERE name='hourly'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        if last_ts >= cur_hour {
            return Ok(0);
        }

        let tx = conn.transaction()?;
        // events_hourly: (path, referrer, country, device)별 pageviews 누적
        tx.execute(
            "INSERT INTO events_hourly (site, ts_hour, path, referrer, country, device, pageviews)
             SELECT
                site,
                (ts_ms / ?1) * ?1 AS ts_hour,
                path,
                COALESCE(referrer, '(direct)'),
                COALESCE(country, '(unknown)'),
                COALESCE(device, '(unknown)'),
                COUNT(*)
             FROM events
             WHERE kind='pageview' AND ts_ms >= ?2 AND ts_ms < ?3
             GROUP BY site, ts_hour, path,
                      COALESCE(referrer, '(direct)'),
                      COALESCE(country, '(unknown)'),
                      COALESCE(device, '(unknown)')
             ON CONFLICT(site, ts_hour, path, referrer, country, device)
             DO UPDATE SET pageviews = pageviews + EXCLUDED.pageviews",
            params![HOUR_MS, last_ts, cur_hour],
        )?;

        // visitors_hourly: 시간 단위로 dedupe된 visitor_hash
        // (모든 kind 포함 — pageview/custom 누구든 활동이 visit으로 간주)
        let visitors_inserted = tx.execute(
            "INSERT OR IGNORE INTO visitors_hourly (site, ts_hour, visitor_hash)
             SELECT
                site,
                (ts_ms / ?1) * ?1 AS ts_hour,
                visitor_hash
             FROM events
             WHERE ts_ms >= ?2 AND ts_ms < ?3",
            params![HOUR_MS, last_ts, cur_hour],
        )?;

        tx.execute(
            "INSERT INTO rollup_state (name, last_ts_ms) VALUES ('hourly', ?1)
             ON CONFLICT(name) DO UPDATE SET last_ts_ms = excluded.last_ts_ms",
            params![cur_hour],
        )?;
        tx.commit()?;

        Ok(visitors_inserted)
    }

    async fn rollup_daily(&self) -> rusqlite::Result<usize> {
        let mut conn = self.conn()?;
        let now = chrono::Utc::now().timestamp_millis();
        let cur_day = floor_to_bucket(now, DAY_MS);

        let last_ts: i64 = conn
            .query_row(
                "SELECT last_ts_ms FROM rollup_state WHERE name='daily'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        if last_ts >= cur_day {
            return Ok(0);
        }

        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO events_daily (site, ts_day, path, referrer, country, device, pageviews)
             SELECT
                site,
                (ts_ms / ?1) * ?1 AS ts_day,
                path,
                COALESCE(referrer, '(direct)'),
                COALESCE(country, '(unknown)'),
                COALESCE(device, '(unknown)'),
                COUNT(*)
             FROM events
             WHERE kind='pageview' AND ts_ms >= ?2 AND ts_ms < ?3
             GROUP BY site, ts_day, path,
                      COALESCE(referrer, '(direct)'),
                      COALESCE(country, '(unknown)'),
                      COALESCE(device, '(unknown)')
             ON CONFLICT(site, ts_day, path, referrer, country, device)
             DO UPDATE SET pageviews = pageviews + EXCLUDED.pageviews",
            params![DAY_MS, last_ts, cur_day],
        )?;

        let visitors_inserted = tx.execute(
            "INSERT OR IGNORE INTO visitors_daily (site, ts_day, visitor_hash)
             SELECT
                site,
                (ts_ms / ?1) * ?1 AS ts_day,
                visitor_hash
             FROM events
             WHERE ts_ms >= ?2 AND ts_ms < ?3",
            params![DAY_MS, last_ts, cur_day],
        )?;

        tx.execute(
            "INSERT INTO rollup_state (name, last_ts_ms) VALUES ('daily', ?1)
             ON CONFLICT(name) DO UPDATE SET last_ts_ms = excluded.last_ts_ms",
            params![cur_day],
        )?;
        tx.commit()?;

        Ok(visitors_inserted)
    }
}

fn floor_to_bucket(ts_ms: i64, bucket: i64) -> i64 {
    (ts_ms / bucket) * bucket
}

fn top_n(
    conn: &rusqlite::Connection,
    sql: &str,
    site: &str,
    from: i64,
    to: i64,
) -> rusqlite::Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(params![site, from, to], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// 사전집계 + raw 보충 union 결과를 dimension별로 합산한 top10.
// raw 쪽은 NULL을 '(direct)'/'(unknown)'로 정규화 (집계 테이블과 일치시키기 위해).
fn top_n_combined(
    conn: &rusqlite::Connection,
    events_table: &str,
    ts_col: &str,
    dim: &str,           // "path" | "referrer" | "country" | "device"
    where_extra: &str,   // "kind='pageview'" (raw 쪽 필터만 적용)
    site: &str,
    agg_from: i64,
    agg_to_inc: i64,
    raw_from: i64,
    raw_to: i64,
) -> rusqlite::Result<Vec<(String, i64)>> {
    let raw_expr = match dim {
        "referrer" => "COALESCE(referrer,'(direct)')".to_string(),
        "country"  => "COALESCE(country,'(unknown)')".to_string(),
        "device"   => "COALESCE(device,'(unknown)')".to_string(),
        other      => other.to_string(),
    };
    let sql = format!(
        "SELECT k, SUM(c) AS total FROM (
            SELECT {dim} AS k, pageviews AS c FROM {events_table}
            WHERE site = ?1 AND {ts_col} BETWEEN ?2 AND ?3
            UNION ALL
            SELECT {raw_expr} AS k, COUNT(*) AS c FROM events
            WHERE site = ?1 AND {where_extra}
                  AND ts_ms BETWEEN ?4 AND ?5
            GROUP BY {raw_expr}
         )
         GROUP BY k ORDER BY total DESC LIMIT 10"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(
            params![site, agg_from, agg_to_inc, raw_from, raw_to],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
