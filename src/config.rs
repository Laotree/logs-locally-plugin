use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// Path to the SQLite database file
    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,

    /// Claude Code projects directory
    #[serde(default = "default_claude_dir")]
    pub claude_projects_dir: PathBuf,

    /// Host for the web server
    #[serde(default = "default_host")]
    pub host: String,

    /// Port for the web server
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_db_path() -> PathBuf {
    dirs_home_dir().join(".local").join("share").join("logs-locally-plugin").join("logs.db")
}

fn default_claude_dir() -> PathBuf {
    dirs_home_dir().join(".claude").join("projects")
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8484
}

fn dirs_home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

impl Config {
    pub fn load(path: Option<&std::path::Path>) -> Result<Self> {
        let path = path.unwrap_or_else(|| std::path::Path::new("config.json"));
        let content =
            std::fs::read_to_string(path).with_context(|| format!("reading {:?}", path))?;
        let mut config: Config =
            serde_json::from_str(&content).with_context(|| format!("parsing {:?}", path))?;
        // Expand ~ in paths
        config.db_path = expand_tilde(&config.db_path.to_string_lossy());
        config.claude_projects_dir = expand_tilde(&config.claude_projects_dir.to_string_lossy());
        Ok(config)
    }

    /// Get the project directory name, matching Claude Code's naming convention.
    pub fn project_dir_name(project_path: &std::path::Path) -> String {
        let canonical = std::fs::canonicalize(project_path)
            .unwrap_or_else(|_| project_path.to_path_buf());
        let path_str = canonical.to_string_lossy();
        let sanitized = path_str
            .replace('/', "-")
            .replace(':', "")
            .replace('\\', "-");
        sanitized
    }
}
