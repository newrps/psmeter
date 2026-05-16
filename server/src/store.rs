use async_trait::async_trait;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub type SqlitePool = Pool<SqliteConnectionManager>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub domain: String,
    pub name: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub site: String,
    pub kind: String,        // "pageview" | "custom" | ...
    pub path: String,
    pub referrer: Option<String>,
    pub visitor_hash: String, // 일별 회전 해시 (개인정보 보호)
    pub country: Option<String>,
    pub device: Option<String>, // "mobile" | "desktop" | "tablet"
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
}

#[async_trait]
pub trait Store: Send + Sync {
    async fn ensure_site(&self, domain: &str, name: &str) -> rusqlite::Result<()>;
    async fn list_sites(&self) -> rusqlite::Result<Vec<Site>>;
    async fn delete_site(&self, domain: &str) -> rusqlite::Result<()>;
    async fn record_event(&self, ev: &Event) -> rusqlite::Result<()>;
    async fn stats(&self, site: &str, from_ms: i64, to_ms: i64) -> rusqlite::Result<Stats>;
    async fn active_visitors(&self, site: &str, since_ms: i64) -> rusqlite::Result<i64>;
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
            ",
        )?;
        Ok(())
    }

    fn conn(&self) -> rusqlite::Result<r2d2::PooledConnection<SqliteConnectionManager>> {
        self.pool
            .get()
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
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

    async fn stats(&self, site: &str, from_ms: i64, to_ms: i64) -> rusqlite::Result<Stats> {
        let conn = self.conn()?;

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
            &conn,
            "SELECT path, COUNT(*) c FROM events
             WHERE site = ?1 AND kind='pageview' AND ts_ms BETWEEN ?2 AND ?3
             GROUP BY path ORDER BY c DESC LIMIT 10",
            site,
            from_ms,
            to_ms,
        )?;

        let top_referrers = top_n(
            &conn,
            "SELECT COALESCE(referrer,'(direct)'), COUNT(*) c FROM events
             WHERE site = ?1 AND kind='pageview' AND ts_ms BETWEEN ?2 AND ?3
             GROUP BY referrer ORDER BY c DESC LIMIT 10",
            site,
            from_ms,
            to_ms,
        )?;

        let countries = top_n(
            &conn,
            "SELECT COALESCE(country,'(unknown)'), COUNT(*) c FROM events
             WHERE site = ?1 AND kind='pageview' AND ts_ms BETWEEN ?2 AND ?3
             GROUP BY country ORDER BY c DESC LIMIT 10",
            site,
            from_ms,
            to_ms,
        )?;

        let devices = top_n(
            &conn,
            "SELECT COALESCE(device,'(unknown)'), COUNT(*) c FROM events
             WHERE site = ?1 AND kind='pageview' AND ts_ms BETWEEN ?2 AND ?3
             GROUP BY device ORDER BY c DESC LIMIT 10",
            site,
            from_ms,
            to_ms,
        )?;

        Ok(Stats {
            pageviews,
            visitors,
            top_pages,
            top_referrers,
            countries,
            devices,
        })
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
