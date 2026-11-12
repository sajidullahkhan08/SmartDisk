use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

use crate::core::errors::StorageError;
use crate::core::models::{ScanResult, ScanStatus};
use crate::core::util::now_unix;
use crate::storage::settings::AppSettings;

#[derive(Clone, Debug)]
pub struct ScanHistoryEntry {
    pub id: i64,
    pub path: String,
    pub started_at: i64,
    pub finished_at: i64,
    pub total_size: u64,
    pub files: u64,
    pub dirs: u64,
    pub errors: u64,
    pub status: String,
    pub elapsed_ms: u64,
    pub volume_label: String,
}

pub struct Database {
    conn: Connection,
    path: PathBuf,
}

pub fn default_data_dir() -> Result<PathBuf, StorageError> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)))
        .ok_or_else(|| StorageError::DataDir("APPDATA is not defined".into()))?;
    let dir = base.join("SmartDisk");
    std::fs::create_dir_all(&dir)
        .map_err(|e| StorageError::DataDir(format!("{}: {e}", dir.display())))?;
    Ok(dir)
}

pub fn default_database_path() -> Result<PathBuf, StorageError> {
    Ok(default_data_dir()?.join("smartdisk.db"))
}

impl Database {
    pub fn open_default() -> Result<Self, StorageError> {
        Self::open(default_database_path()?)
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| StorageError::DataDir(format!("{}: {e}", parent.display())))?;
        }
        let conn = Connection::open(&path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS settings (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS scan_history (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 path TEXT NOT NULL,
                 started_at INTEGER NOT NULL,
                 finished_at INTEGER NOT NULL,
                 total_size INTEGER NOT NULL,
                 files INTEGER NOT NULL,
                 dirs INTEGER NOT NULL,
                 errors INTEGER NOT NULL,
                 status TEXT NOT NULL,
                 elapsed_ms INTEGER NOT NULL,
                 volume_label TEXT NOT NULL DEFAULT '',
                 volume_filesystem TEXT NOT NULL DEFAULT '',
                 volume_total INTEGER NOT NULL DEFAULT 0,
                 volume_free INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS idx_scan_history_started
                 ON scan_history(started_at DESC);
             CREATE TABLE IF NOT EXISTS recent_locations (
                 path TEXT PRIMARY KEY,
                 last_used INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_recent_used
                 ON recent_locations(last_used DESC);",
        )?;
        Ok(Self { conn, path })
    }

    pub fn in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE scan_history (
                 id INTEGER PRIMARY KEY AUTOINCREMENT, path TEXT NOT NULL,
                 started_at INTEGER NOT NULL, finished_at INTEGER NOT NULL,
                 total_size INTEGER NOT NULL, files INTEGER NOT NULL,
                 dirs INTEGER NOT NULL, errors INTEGER NOT NULL,
                 status TEXT NOT NULL, elapsed_ms INTEGER NOT NULL,
                 volume_label TEXT NOT NULL DEFAULT '',
                 volume_filesystem TEXT NOT NULL DEFAULT '',
                 volume_total INTEGER NOT NULL DEFAULT 0,
                 volume_free INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE recent_locations (path TEXT PRIMARY KEY, last_used INTEGER NOT NULL);",
        )?;
        Ok(Self {
            conn,
            path: PathBuf::from(":memory:"),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn size_bytes(&self) -> u64 {
        std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0)
    }

    pub fn load_settings(&self) -> AppSettings {
        self.conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'app'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str::<AppSettings>(&json).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), StorageError> {
        let json = serde_json::to_string(settings)?;
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES('app', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    }

    pub fn record_scan(&self, result: &ScanResult) -> Result<i64, StorageError> {
        let volume = result.volume.clone().unwrap_or_default();
        self.conn.execute(
            "INSERT INTO scan_history
             (path, started_at, finished_at, total_size, files, dirs, errors, status,
              elapsed_ms, volume_label, volume_filesystem, volume_total, volume_free)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                result.root_path,
                result.started_at,
                result.finished_at,
                u64_to_i64(result.stats.total_size),
                u64_to_i64(result.stats.files),
                u64_to_i64(result.stats.dirs),
                u64_to_i64(result.stats.errors),
                result.status.key(),
                u64_to_i64(result.stats.elapsed_ms),
                volume.label,
                volume.filesystem,
                u64_to_i64(volume.total),
                u64_to_i64(volume.free),
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.prune_history(200)?;
        Ok(id)
    }

    fn prune_history(&self, max_entries: usize) -> Result<(), StorageError> {
        self.conn.execute(
            "DELETE FROM scan_history WHERE id NOT IN
             (SELECT id FROM scan_history ORDER BY started_at DESC LIMIT ?1)",
            params![max_entries as i64],
        )?;
        Ok(())
    }

    pub fn history(&self, limit: usize) -> Result<Vec<ScanHistoryEntry>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, started_at, finished_at, total_size, files, dirs, errors,
                    status, elapsed_ms, volume_label
             FROM scan_history ORDER BY started_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(ScanHistoryEntry {
                id: row.get(0)?,
                path: row.get(1)?,
                started_at: row.get(2)?,
                finished_at: row.get(3)?,
                total_size: row.get::<_, i64>(4)?.max(0) as u64,
                files: row.get::<_, i64>(5)?.max(0) as u64,
                dirs: row.get::<_, i64>(6)?.max(0) as u64,
                errors: row.get::<_, i64>(7)?.max(0) as u64,
                status: row.get(8)?,
                elapsed_ms: row.get::<_, i64>(9)?.max(0) as u64,
                volume_label: row.get(10)?,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn clear_history(&self) -> Result<(), StorageError> {
        self.conn.execute("DELETE FROM scan_history", [])?;
        Ok(())
    }

    pub fn add_recent_location(&self, path: &str) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO recent_locations(path, last_used) VALUES(?1, ?2)
             ON CONFLICT(path) DO UPDATE SET last_used = excluded.last_used",
            params![path, now_unix()],
        )?;
        self.conn.execute(
            "DELETE FROM recent_locations WHERE path NOT IN
             (SELECT path FROM recent_locations ORDER BY last_used DESC LIMIT 12)",
            [],
        )?;
        Ok(())
    }

    pub fn recent_locations(&self, limit: usize) -> Result<Vec<String>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT path FROM recent_locations ORDER BY last_used DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| row.get::<_, String>(0))?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn clear_all_scan_data(&self) -> Result<(), StorageError> {
        self.conn.execute("DELETE FROM scan_history", [])?;
        self.conn.execute("DELETE FROM recent_locations", [])?;
        Ok(())
    }

    /// Best-effort vacuum. Failure (for example while another connection is
    /// active) is reported to the caller rather than ignored silently.
    pub fn vacuum(&self) -> Result<(), StorageError> {
        self.conn.execute_batch("VACUUM;")?;
        Ok(())
    }
}

fn u64_to_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

/// Outcome counts used by the history view.
pub fn status_label(key: &str) -> &'static str {
    ScanStatus::from_key(key).label()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{ScanStats, ScanStatus, VolumeInfo};

    fn sample_result() -> ScanResult {
        ScanResult {
            nodes: Vec::new(),
            root: 0,
            root_path: "C:\\Test".into(),
            status: ScanStatus::Complete,
            stats: ScanStats {
                total_size: 1234,
                files: 5,
                dirs: 2,
                errors: 0,
                elapsed_ms: 42,
                ..Default::default()
            },
            errors: Vec::new(),
            warnings: Vec::new(),
            started_at: 100,
            finished_at: 200,
            volume: Some(VolumeInfo {
                label: "Test".into(),
                filesystem: "NTFS".into(),
                total: 1000,
                free: 400,
                used: 600,
                ..Default::default()
            }),
        }
    }

    #[test]
    fn settings_round_trip() {
        let db = Database::in_memory().unwrap();
        let mut settings = db.load_settings();
        assert_eq!(settings.theme, "dark");
        settings.theme = "light".into();
        settings.exclusions.push("C:\\Skip".into());
        db.save_settings(&settings).unwrap();
        let loaded = db.load_settings();
        assert_eq!(loaded.theme, "light");
        assert_eq!(loaded.exclusions, vec!["C:\\Skip"]);
    }

    #[test]
    fn history_round_trip() {
        let db = Database::in_memory().unwrap();
        db.record_scan(&sample_result()).unwrap();
        let entries = db.history(10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "C:\\Test");
        assert_eq!(entries[0].total_size, 1234);
        assert_eq!(entries[0].status, "complete");
        assert_eq!(entries[0].volume_label, "Test");
        db.clear_history().unwrap();
        assert!(db.history(10).unwrap().is_empty());
    }

    #[test]
    fn recent_locations_are_updated() {
        let db = Database::in_memory().unwrap();
        db.add_recent_location("C:\\A").unwrap();
        db.add_recent_location("C:\\B").unwrap();
        db.add_recent_location("C:\\A").unwrap();
        let recent = db.recent_locations(10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0], "C:\\A");
    }
}
