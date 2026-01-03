//! SQLite cache for track analysis results
//!
//! Stores BPM, key, and metadata analysis to avoid re-analyzing unchanged files.

use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur during cache operations
#[derive(Error, Debug)]
pub enum CacheError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Cached analysis result for a track
#[derive(Debug, Clone)]
pub struct CachedAnalysis {
    /// Path to the audio file
    pub path: PathBuf,
    /// File size in bytes (for cache invalidation)
    pub file_size: u64,
    /// File modification time as Unix timestamp (for cache invalidation)
    pub modified_time: u64,
    /// Track duration in seconds
    pub duration_secs: f64,
    /// Detected BPM (None if detection failed)
    pub bpm: Option<f32>,
    /// BPM detection confidence (0.0-1.0)
    pub bpm_confidence: Option<f32>,
    /// Detected key in Camelot notation (e.g., "8A", "12B")
    pub key: Option<String>,
    /// Key detection confidence (0.0-1.0)
    pub key_confidence: Option<f32>,
    /// Track title
    pub title: String,
    /// Track artist
    pub artist: String,
}

/// Analysis cache backed by SQLite
pub struct AnalysisCache {
    conn: Connection,
}

impl AnalysisCache {
    /// SQL schema for the tracks table
    const SCHEMA: &'static str = r#"
        CREATE TABLE IF NOT EXISTS tracks (
            id INTEGER PRIMARY KEY,
            path TEXT UNIQUE NOT NULL,
            file_size INTEGER NOT NULL,
            modified_time INTEGER NOT NULL,
            duration_secs REAL NOT NULL,
            bpm REAL,
            bpm_confidence REAL,
            key TEXT,
            key_confidence REAL,
            title TEXT NOT NULL,
            artist TEXT NOT NULL,
            analyzed_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_path ON tracks(path);
        CREATE INDEX IF NOT EXISTS idx_key ON tracks(key);
        CREATE INDEX IF NOT EXISTS idx_bpm ON tracks(bpm);
    "#;

    /// Open or create a cache database at the given path
    pub fn open(db_path: &Path) -> Result<Self, CacheError> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;
        conn.execute_batch(Self::SCHEMA)?;
        Ok(Self { conn })
    }

    /// Open an in-memory database (for testing)
    #[cfg(test)]
    pub fn in_memory() -> Result<Self, CacheError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(Self::SCHEMA)?;
        Ok(Self { conn })
    }

    /// Get cached analysis if the file hasn't changed
    ///
    /// Returns None if:
    /// - The file is not in the cache
    /// - The file size has changed
    /// - The modification time has changed
    pub fn get(&self, path: &Path, file_size: u64, modified_time: u64) -> Option<CachedAnalysis> {
        self.conn
            .query_row(
                "SELECT path, file_size, modified_time, duration_secs, bpm, bpm_confidence,
                        key, key_confidence, title, artist
                 FROM tracks
                 WHERE path = ?1 AND file_size = ?2 AND modified_time = ?3",
                params![path.to_string_lossy().to_string(), file_size, modified_time],
                |row| {
                    Ok(CachedAnalysis {
                        path: PathBuf::from(row.get::<_, String>(0)?),
                        file_size: row.get(1)?,
                        modified_time: row.get(2)?,
                        duration_secs: row.get(3)?,
                        bpm: row.get(4)?,
                        bpm_confidence: row.get(5)?,
                        key: row.get(6)?,
                        key_confidence: row.get(7)?,
                        title: row.get(8)?,
                        artist: row.get(9)?,
                    })
                },
            )
            .ok()
    }

    /// Store analysis result in the cache
    ///
    /// If the path already exists, it will be updated.
    pub fn store(&self, analysis: &CachedAnalysis) -> Result<(), CacheError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.conn.execute(
            r#"INSERT OR REPLACE INTO tracks
               (path, file_size, modified_time, duration_secs,
                bpm, bpm_confidence, key, key_confidence,
                title, artist, analyzed_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#,
            params![
                analysis.path.to_string_lossy().to_string(),
                analysis.file_size,
                analysis.modified_time,
                analysis.duration_secs,
                analysis.bpm,
                analysis.bpm_confidence,
                analysis.key,
                analysis.key_confidence,
                analysis.title,
                analysis.artist,
                now,
            ],
        )?;
        Ok(())
    }

    /// Get all cached tracks, sorted by key then BPM
    pub fn get_all_sorted(&self) -> Result<Vec<CachedAnalysis>, CacheError> {
        let mut stmt = self.conn.prepare(
            "SELECT path, file_size, modified_time, duration_secs, bpm, bpm_confidence,
                    key, key_confidence, title, artist
             FROM tracks
             ORDER BY
                 CASE WHEN key IS NULL THEN 1 ELSE 0 END,  -- NULLs last
                 key ASC,
                 CASE WHEN bpm IS NULL THEN 1 ELSE 0 END,  -- NULLs last
                 bpm ASC",
        )?;

        let tracks = stmt
            .query_map([], |row| {
                Ok(CachedAnalysis {
                    path: PathBuf::from(row.get::<_, String>(0)?),
                    file_size: row.get(1)?,
                    modified_time: row.get(2)?,
                    duration_secs: row.get(3)?,
                    bpm: row.get(4)?,
                    bpm_confidence: row.get(5)?,
                    key: row.get(6)?,
                    key_confidence: row.get(7)?,
                    title: row.get(8)?,
                    artist: row.get(9)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tracks)
    }

    /// Get all tracks with a specific key
    pub fn get_by_key(&self, key: &str) -> Result<Vec<CachedAnalysis>, CacheError> {
        let mut stmt = self.conn.prepare(
            "SELECT path, file_size, modified_time, duration_secs, bpm, bpm_confidence,
                    key, key_confidence, title, artist
             FROM tracks
             WHERE key = ?1
             ORDER BY bpm ASC",
        )?;

        let tracks = stmt
            .query_map([key], |row| {
                Ok(CachedAnalysis {
                    path: PathBuf::from(row.get::<_, String>(0)?),
                    file_size: row.get(1)?,
                    modified_time: row.get(2)?,
                    duration_secs: row.get(3)?,
                    bpm: row.get(4)?,
                    bpm_confidence: row.get(5)?,
                    key: row.get(6)?,
                    key_confidence: row.get(7)?,
                    title: row.get(8)?,
                    artist: row.get(9)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tracks)
    }

    /// Get the number of cached tracks
    pub fn count(&self) -> Result<usize, CacheError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Remove a track from the cache
    pub fn remove(&self, path: &Path) -> Result<bool, CacheError> {
        let affected = self.conn.execute(
            "DELETE FROM tracks WHERE path = ?1",
            [path.to_string_lossy().to_string()],
        )?;
        Ok(affected > 0)
    }

    /// Clear all cached data
    pub fn clear(&self) -> Result<(), CacheError> {
        self.conn.execute("DELETE FROM tracks", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_analysis() -> CachedAnalysis {
        CachedAnalysis {
            path: PathBuf::from("/test/track.mp3"),
            file_size: 1024000,
            modified_time: 1700000000,
            duration_secs: 180.5,
            bpm: Some(128.0),
            bpm_confidence: Some(0.95),
            key: Some("8A".to_string()),
            key_confidence: Some(0.87),
            title: "Test Track".to_string(),
            artist: "Test Artist".to_string(),
        }
    }

    #[test]
    fn test_store_and_get() {
        let cache = AnalysisCache::in_memory().unwrap();
        let analysis = test_analysis();

        cache.store(&analysis).unwrap();

        let retrieved = cache.get(&analysis.path, analysis.file_size, analysis.modified_time);
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.title, "Test Track");
        assert_eq!(retrieved.bpm, Some(128.0));
        assert_eq!(retrieved.key, Some("8A".to_string()));
    }

    #[test]
    fn test_cache_invalidation_file_size() {
        let cache = AnalysisCache::in_memory().unwrap();
        let analysis = test_analysis();

        cache.store(&analysis).unwrap();

        // Different file size should not match
        let retrieved = cache.get(&analysis.path, 999999, analysis.modified_time);
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_cache_invalidation_modified_time() {
        let cache = AnalysisCache::in_memory().unwrap();
        let analysis = test_analysis();

        cache.store(&analysis).unwrap();

        // Different modification time should not match
        let retrieved = cache.get(&analysis.path, analysis.file_size, 1800000000);
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_get_all_sorted() {
        let cache = AnalysisCache::in_memory().unwrap();

        // Insert tracks with different keys/BPMs
        let mut a1 = test_analysis();
        a1.path = PathBuf::from("/test/track1.mp3");
        a1.key = Some("8A".to_string());
        a1.bpm = Some(130.0);
        cache.store(&a1).unwrap();

        let mut a2 = test_analysis();
        a2.path = PathBuf::from("/test/track2.mp3");
        a2.key = Some("8A".to_string());
        a2.bpm = Some(125.0);
        cache.store(&a2).unwrap();

        let mut a3 = test_analysis();
        a3.path = PathBuf::from("/test/track3.mp3");
        a3.key = Some("7A".to_string());
        a3.bpm = Some(128.0);
        cache.store(&a3).unwrap();

        let all = cache.get_all_sorted().unwrap();
        assert_eq!(all.len(), 3);

        // Should be sorted by key, then BPM
        assert_eq!(all[0].key, Some("7A".to_string())); // 7A first
        assert_eq!(all[1].key, Some("8A".to_string())); // 8A at 125 BPM
        assert_eq!(all[1].bpm, Some(125.0));
        assert_eq!(all[2].key, Some("8A".to_string())); // 8A at 130 BPM
        assert_eq!(all[2].bpm, Some(130.0));
    }

    #[test]
    fn test_get_by_key() {
        let cache = AnalysisCache::in_memory().unwrap();

        let mut a1 = test_analysis();
        a1.path = PathBuf::from("/test/track1.mp3");
        a1.key = Some("8A".to_string());
        cache.store(&a1).unwrap();

        let mut a2 = test_analysis();
        a2.path = PathBuf::from("/test/track2.mp3");
        a2.key = Some("9A".to_string());
        cache.store(&a2).unwrap();

        let filtered = cache.get_by_key("8A").unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].key, Some("8A".to_string()));
    }

    #[test]
    fn test_count() {
        let cache = AnalysisCache::in_memory().unwrap();

        assert_eq!(cache.count().unwrap(), 0);

        cache.store(&test_analysis()).unwrap();
        assert_eq!(cache.count().unwrap(), 1);
    }

    #[test]
    fn test_remove() {
        let cache = AnalysisCache::in_memory().unwrap();
        let analysis = test_analysis();

        cache.store(&analysis).unwrap();
        assert_eq!(cache.count().unwrap(), 1);

        let removed = cache.remove(&analysis.path).unwrap();
        assert!(removed);
        assert_eq!(cache.count().unwrap(), 0);
    }

    #[test]
    fn test_clear() {
        let cache = AnalysisCache::in_memory().unwrap();

        cache.store(&test_analysis()).unwrap();

        let mut a2 = test_analysis();
        a2.path = PathBuf::from("/test/track2.mp3");
        cache.store(&a2).unwrap();

        assert_eq!(cache.count().unwrap(), 2);

        cache.clear().unwrap();
        assert_eq!(cache.count().unwrap(), 0);
    }

    #[test]
    fn test_update_existing() {
        let cache = AnalysisCache::in_memory().unwrap();
        let mut analysis = test_analysis();

        cache.store(&analysis).unwrap();

        // Update the same path with new data
        analysis.bpm = Some(140.0);
        analysis.title = "Updated Title".to_string();
        cache.store(&analysis).unwrap();

        // Should still be only 1 track
        assert_eq!(cache.count().unwrap(), 1);

        // Should have updated data
        let retrieved = cache.get(&analysis.path, analysis.file_size, analysis.modified_time);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().bpm, Some(140.0));
    }
}
