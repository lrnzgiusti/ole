//! Track library for OLE - loading, metadata, and caching

mod cache;
mod loader;
mod scanner;

pub use cache::{AnalysisCache, CacheError, CachedAnalysis};
pub use loader::{LoadError, LoadedTrack, TrackLoader, TrackMetadata};
pub use scanner::{LibraryScanner, ScanConfig, ScanError, ScanProgress, ScanResult};
