//! Track library for OLE - loading, metadata, and caching

mod cache;
mod config;
mod loader;
mod scanner;

pub use cache::{AnalysisCache, CacheError, CachedAnalysis};
pub use config::Config;
pub use loader::{LoadError, LoadedTrack, TrackLoader, TrackMetadata};
pub use scanner::{LibraryScanner, ScanConfig, ScanError, ScanProgress, ScanResult};
