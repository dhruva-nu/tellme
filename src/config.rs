//! Persistent configuration stored at `.tellme/config.json` (#16).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// How command output is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
#[value(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Human-readable text (the default).
    #[default]
    Text,
    /// Machine-readable JSON.
    Json,
    /// Graphviz DOT (only meaningful for `flow --graph`).
    Dot,
}

/// On-disk configuration. Serialized as pretty JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Editor to launch for the decision editor; falls back to `$EDITOR` when `None`.
    #[serde(default)]
    pub editor: Option<String>,
    /// Default output format.
    #[serde(default)]
    pub format: OutputFormat,
    /// Source language for analysis (v1 is Python-only).
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_language() -> String {
    "python".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            editor: None,
            format: OutputFormat::default(),
            language: default_language(),
        }
    }
}

impl Config {
    /// Load config from `path`, or return defaults if the file is absent.
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s)
                .map_err(|e| Error::Config(format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(Error::Io(e)),
        }
    }

    /// Write config to `path` as pretty JSON.
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| Error::Config(e.to_string()))?;
        fs::write(path, json + "\n")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_python_text() {
        let c = Config::default();
        assert_eq!(c.format, OutputFormat::Text);
        assert_eq!(c.language, "python");
        assert!(c.editor.is_none());
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let c = Config {
            editor: Some("nvim".into()),
            format: OutputFormat::Json,
            ..Config::default()
        };
        c.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert_eq!(c, loaded);
    }

    #[test]
    fn missing_file_yields_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = Config::load(&dir.path().join("nope.json")).unwrap();
        assert_eq!(loaded, Config::default());
    }
}
