//! Simple configuration persistence for OLE
//!
//! Stores user preferences like last scanned folder.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Application configuration
#[derive(Debug, Default)]
pub struct Config {
    /// Last folder that was scanned for tracks
    pub last_scan_folder: Option<PathBuf>,
}

impl Config {
    /// Load config from the default location
    ///
    /// Returns default config if file doesn't exist or can't be parsed.
    pub fn load() -> Self {
        let path = Self::config_path();
        Self::load_from(&path).unwrap_or_default()
    }

    /// Load config from a specific path
    pub fn load_from(path: &Path) -> io::Result<Self> {
        let content = fs::read_to_string(path)?;
        Ok(Self::parse(&content))
    }

    /// Save config to the default location
    pub fn save(&self) -> io::Result<()> {
        let path = Self::config_path();
        self.save_to(&path)
    }

    /// Save config to a specific path
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = self.serialize();
        fs::write(path, content)
    }

    /// Get the default config file path
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ole")
            .join("config.txt")
    }

    /// Parse config from simple key=value format
    fn parse(content: &str) -> Self {
        let mut config = Self::default();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "last_scan_folder" => {
                        if !value.is_empty() {
                            config.last_scan_folder = Some(PathBuf::from(value));
                        }
                    }
                    _ => {} // Ignore unknown keys
                }
            }
        }

        config
    }

    /// Serialize config to simple key=value format
    fn serialize(&self) -> String {
        let mut lines = Vec::new();
        lines.push("# OLE Configuration".to_string());

        if let Some(ref folder) = self.last_scan_folder {
            lines.push(format!("last_scan_folder={}", folder.display()));
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        let config = Config::parse("");
        assert!(config.last_scan_folder.is_none());
    }

    #[test]
    fn test_parse_with_folder() {
        let config = Config::parse("last_scan_folder=/home/user/music");
        assert_eq!(
            config.last_scan_folder,
            Some(PathBuf::from("/home/user/music"))
        );
    }

    #[test]
    fn test_parse_with_comments() {
        let content = "# Comment\nlast_scan_folder=/music\n# Another comment";
        let config = Config::parse(content);
        assert_eq!(config.last_scan_folder, Some(PathBuf::from("/music")));
    }

    #[test]
    fn test_serialize_roundtrip() {
        let mut config = Config::default();
        config.last_scan_folder = Some(PathBuf::from("/test/path"));

        let serialized = config.serialize();
        let parsed = Config::parse(&serialized);

        assert_eq!(parsed.last_scan_folder, config.last_scan_folder);
    }
}
