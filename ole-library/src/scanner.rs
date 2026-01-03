//! Directory scanner with parallel track analysis
//!
//! Scans directories for audio files, analyzes BPM and key using multiple
//! threads, and stores results in the cache.

use crate::cache::{AnalysisCache, CachedAnalysis, CacheError};
use crate::loader::{LoadError, TrackLoader};
use crossbeam_channel::{self, Receiver, Sender};
use ole_analysis::{BeatGridAnalyzer, CamelotKey, KeyAnalyzer};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::SystemTime;

/// Progress updates during directory scanning
#[derive(Debug, Clone)]
pub enum ScanProgress {
    /// Scanning started
    Started {
        /// Total number of files to process
        total: usize,
    },
    /// Currently analyzing a file
    Analyzing {
        /// Current file number (1-indexed)
        current: usize,
        /// Total number of files
        total: usize,
        /// Path being analyzed
        path: PathBuf,
    },
    /// File was already cached (no re-analysis needed)
    Cached {
        /// Current file number (1-indexed)
        current: usize,
        /// Total number of files
        total: usize,
        /// Path that was cached
        path: PathBuf,
    },
    /// Scanning completed
    Complete {
        /// Number of files that were analyzed
        analyzed: usize,
        /// Number of files that were already cached
        cached: usize,
        /// Number of files that failed
        failed: usize,
    },
    /// Error analyzing a file
    Error {
        /// Path that failed
        path: PathBuf,
        /// Error message
        message: String,
    },
}

/// Configuration for the directory scanner
#[derive(Debug, Clone)]
pub struct ScanConfig {
    /// Directory to scan
    pub directory: PathBuf,
    /// File extensions to include
    pub extensions: Vec<String>,
    /// Maximum number of parallel analysis threads
    pub max_threads: usize,
    /// Whether to scan subdirectories recursively
    pub recursive: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            directory: PathBuf::new(),
            extensions: vec![
                "mp3".into(),
                "flac".into(),
                "wav".into(),
                "ogg".into(),
                "m4a".into(),
                "aac".into(),
            ],
            max_threads: 4,
            recursive: true,
        }
    }
}

/// Error type for scanning operations
#[derive(Debug)]
pub enum ScanError {
    /// Cache error
    Cache(CacheError),
    /// IO error
    Io(std::io::Error),
    /// Analysis error
    Analysis(String),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::Cache(e) => write!(f, "Cache error: {}", e),
            ScanError::Io(e) => write!(f, "IO error: {}", e),
            ScanError::Analysis(s) => write!(f, "Analysis error: {}", s),
        }
    }
}

impl From<CacheError> for ScanError {
    fn from(e: CacheError) -> Self {
        ScanError::Cache(e)
    }
}

impl From<std::io::Error> for ScanError {
    fn from(e: std::io::Error) -> Self {
        ScanError::Io(e)
    }
}

impl From<LoadError> for ScanError {
    fn from(e: LoadError) -> Self {
        ScanError::Analysis(e.to_string())
    }
}

/// Result of a directory scan
pub struct ScanResult {
    /// All tracks (newly analyzed + cached)
    pub tracks: Vec<CachedAnalysis>,
    /// Number of files that were analyzed
    pub analyzed_count: usize,
    /// Number of files from cache
    pub cached_count: usize,
    /// Number of files that failed
    pub failed_count: usize,
}

/// Directory scanner with parallel analysis
pub struct LibraryScanner {
    cache: Arc<Mutex<AnalysisCache>>,
}

impl LibraryScanner {
    /// Create a new scanner with the given cache
    pub fn new(cache: AnalysisCache) -> Self {
        Self {
            cache: Arc::new(Mutex::new(cache)),
        }
    }

    /// Scan a directory synchronously (blocking)
    ///
    /// Returns the scan result and sends progress updates through the channel.
    pub fn scan(
        &self,
        config: &ScanConfig,
        progress_tx: Option<Sender<ScanProgress>>,
    ) -> Result<ScanResult, ScanError> {
        // Collect all audio files
        let files = self.collect_files(&config.directory, &config.extensions, config.recursive);
        let total = files.len();

        if let Some(ref tx) = progress_tx {
            let _ = tx.send(ScanProgress::Started { total });
        }

        if total == 0 {
            if let Some(ref tx) = progress_tx {
                let _ = tx.send(ScanProgress::Complete {
                    analyzed: 0,
                    cached: 0,
                    failed: 0,
                });
            }
            return Ok(ScanResult {
                tracks: Vec::new(),
                analyzed_count: 0,
                cached_count: 0,
                failed_count: 0,
            });
        }

        // Partition into cached and uncached
        let (cached_files, uncached_files) = self.partition_by_cache(&files);

        // Report cached files
        for (i, (path, _)) in cached_files.iter().enumerate() {
            if let Some(ref tx) = progress_tx {
                let _ = tx.send(ScanProgress::Cached {
                    current: i + 1,
                    total,
                    path: path.clone(),
                });
            }
        }

        let cached_count = cached_files.len();
        let cached_analyses: Vec<CachedAnalysis> =
            cached_files.into_iter().map(|(_, a)| a).collect();

        // Analyze uncached files in parallel
        let (new_analyses, failed_count) = self.analyze_parallel(
            uncached_files,
            config.max_threads,
            cached_count,
            total,
            progress_tx.clone(),
        );

        // Combine results
        let mut all_tracks = cached_analyses;
        all_tracks.extend(new_analyses.iter().cloned());

        // Sort by key, then BPM
        all_tracks.sort_by(|a, b| {
            match (&a.key, &b.key) {
                (Some(ka), Some(kb)) => ka.cmp(kb).then_with(|| {
                    a.bpm
                        .unwrap_or(0.0)
                        .partial_cmp(&b.bpm.unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                }),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a
                    .bpm
                    .unwrap_or(0.0)
                    .partial_cmp(&b.bpm.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal),
            }
        });

        if let Some(ref tx) = progress_tx {
            let _ = tx.send(ScanProgress::Complete {
                analyzed: new_analyses.len(),
                cached: cached_count,
                failed: failed_count,
            });
        }

        Ok(ScanResult {
            tracks: all_tracks,
            analyzed_count: new_analyses.len(),
            cached_count,
            failed_count,
        })
    }

    /// Start an asynchronous scan
    ///
    /// Returns a receiver for progress updates and a handle to the scanning thread.
    pub fn scan_async(
        &self,
        config: ScanConfig,
    ) -> (Receiver<ScanProgress>, JoinHandle<Result<ScanResult, ScanError>>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let cache = Arc::clone(&self.cache);

        let handle = thread::spawn(move || {
            let scanner = LibraryScanner { cache };
            scanner.scan(&config, Some(tx))
        });

        (rx, handle)
    }

    /// Collect all audio files from a directory
    fn collect_files(&self, dir: &Path, extensions: &[String], recursive: bool) -> Vec<PathBuf> {
        let mut files = Vec::new();

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return files,
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
                        files.push(path);
                    }
                }
            } else if path.is_dir() && recursive {
                files.extend(self.collect_files(&path, extensions, recursive));
            }
        }

        // Sort by filename for consistent ordering
        files.sort();
        files
    }

    /// Partition files into cached (with analysis) and uncached
    fn partition_by_cache(&self, files: &[PathBuf]) -> (Vec<(PathBuf, CachedAnalysis)>, Vec<PathBuf>) {
        let mut cached = Vec::new();
        let mut uncached = Vec::new();

        let cache = self.cache.lock().unwrap();

        for path in files {
            if let Ok(meta) = std::fs::metadata(path) {
                let file_size = meta.len();
                let modified_time = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                if let Some(analysis) = cache.get(path, file_size, modified_time) {
                    cached.push((path.clone(), analysis));
                    continue;
                }
            }
            uncached.push(path.clone());
        }

        (cached, uncached)
    }

    /// Analyze files in parallel using a thread pool
    fn analyze_parallel(
        &self,
        files: Vec<PathBuf>,
        max_threads: usize,
        base_index: usize,
        total: usize,
        progress_tx: Option<Sender<ScanProgress>>,
    ) -> (Vec<CachedAnalysis>, usize) {
        if files.is_empty() {
            return (Vec::new(), 0);
        }

        let file_count = files.len();
        let thread_count = max_threads.min(file_count).max(1);

        // Shared state for work distribution
        let files = Arc::new(Mutex::new(files.into_iter().enumerate().collect::<Vec<_>>()));
        let results: Arc<Mutex<Vec<CachedAnalysis>>> = Arc::new(Mutex::new(Vec::new()));
        let failed_count = Arc::new(Mutex::new(0usize));
        let cache = Arc::clone(&self.cache);

        let mut handles = Vec::new();

        for _ in 0..thread_count {
            let files = Arc::clone(&files);
            let results = Arc::clone(&results);
            let failed_count = Arc::clone(&failed_count);
            let cache = Arc::clone(&cache);
            let progress_tx = progress_tx.clone();

            let handle = thread::spawn(move || {
                let loader = TrackLoader::new();
                let beat_analyzer = BeatGridAnalyzer::new(48000);
                let key_analyzer = KeyAnalyzer::new(48000);

                loop {
                    // Get next file to process
                    let item = {
                        let mut files = files.lock().unwrap();
                        files.pop()
                    };

                    let (idx, path) = match item {
                        Some(item) => item,
                        None => break, // No more files
                    };

                    let current = base_index + idx + 1;

                    if let Some(ref tx) = progress_tx {
                        let _ = tx.send(ScanProgress::Analyzing {
                            current,
                            total,
                            path: path.clone(),
                        });
                    }

                    match analyze_track(&loader, &beat_analyzer, &key_analyzer, &path) {
                        Ok(analysis) => {
                            // Store in cache
                            if let Ok(cache) = cache.lock() {
                                let _ = cache.store(&analysis);
                            }
                            results.lock().unwrap().push(analysis);
                        }
                        Err(e) => {
                            *failed_count.lock().unwrap() += 1;
                            if let Some(ref tx) = progress_tx {
                                let _ = tx.send(ScanProgress::Error {
                                    path: path.clone(),
                                    message: e.to_string(),
                                });
                            }
                        }
                    }
                }
            });

            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            let _ = handle.join();
        }

        // Extract results from Arc<Mutex<Vec>>
        let results = match Arc::try_unwrap(results) {
            Ok(mutex) => mutex.into_inner().unwrap(),
            Err(arc) => arc.lock().unwrap().clone(),
        };

        let failed = *failed_count.lock().unwrap();

        (results, failed)
    }

    /// Get all cached tracks
    pub fn get_all_tracks(&self) -> Result<Vec<CachedAnalysis>, CacheError> {
        self.cache.lock().unwrap().get_all_sorted()
    }
}

/// Analyze a single track for BPM and key
fn analyze_track(
    loader: &TrackLoader,
    beat_analyzer: &BeatGridAnalyzer,
    key_analyzer: &KeyAnalyzer,
    path: &Path,
) -> Result<CachedAnalysis, ScanError> {
    // Get file metadata
    let meta = std::fs::metadata(path)?;
    let file_size = meta.len();
    let modified_time = meta
        .modified()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| ScanError::Analysis(e.to_string()))?
        .as_secs();

    // Load the track
    let track = loader.load(path)?;

    // Analyze BPM (use first 30 seconds)
    let analysis_samples = track.samples.len().min(track.sample_rate as usize * 60); // 30 sec stereo
    let beat_grid = beat_analyzer.analyze(&track.samples[..analysis_samples]);
    let (bpm, bpm_confidence) = beat_grid
        .map(|g| (Some(g.bpm), Some(g.confidence)))
        .unwrap_or((None, None));

    // Analyze key (use first 30 seconds)
    let detected_key = key_analyzer.analyze(&track.samples[..analysis_samples]);
    let (key_str, key_confidence) = detected_key
        .map(|k| {
            let camelot = CamelotKey::from_musical_key(k.key);
            (Some(camelot.display()), Some(k.confidence))
        })
        .unwrap_or((None, None));

    Ok(CachedAnalysis {
        path: path.to_path_buf(),
        file_size,
        modified_time,
        duration_secs: track.metadata.duration_secs,
        bpm,
        bpm_confidence,
        key: key_str,
        key_confidence,
        title: track.metadata.title,
        artist: track.metadata.artist,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_config_default() {
        let config = ScanConfig::default();
        assert_eq!(config.max_threads, 4);
        assert!(config.recursive);
        assert!(config.extensions.contains(&"mp3".to_string()));
    }

    #[test]
    fn test_collect_files_empty_dir() {
        let cache = AnalysisCache::in_memory().unwrap();
        let scanner = LibraryScanner::new(cache);

        // Non-existent directory should return empty
        let files = scanner.collect_files(Path::new("/nonexistent"), &["mp3".into()], true);
        assert!(files.is_empty());
    }

    #[test]
    fn test_scan_empty_result() {
        let cache = AnalysisCache::in_memory().unwrap();
        let scanner = LibraryScanner::new(cache);

        let config = ScanConfig {
            directory: PathBuf::from("/nonexistent"),
            ..Default::default()
        };

        let result = scanner.scan(&config, None).unwrap();
        assert!(result.tracks.is_empty());
        assert_eq!(result.analyzed_count, 0);
        assert_eq!(result.cached_count, 0);
    }
}
