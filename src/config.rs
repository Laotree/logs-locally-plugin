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
    /// One or more SQLite database paths to write to on import.
    /// The first path is also used for `serve`. When unset, falls back to `db_path`.
    #[serde(default)]
    pub db_paths: Vec<PathBuf>,

    /// Legacy single-path field. Used when `db_paths` is absent.
    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,

    /// Claude Code projects directory
    #[serde(default = "default_claude_dir")]
    pub claude_projects_dir: PathBuf,

    /// Pi agent sessions directory (optional).
    /// Set to `~/.pi/agent/sessions` (or a custom path) to also import pi sessions.
    #[serde(rename = "piJsonlDir")]
    pub pi_jsonl_dir: Option<PathBuf>,

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
    pub fn load(path: Option<&std::path::Path>) -> Self {
        let path = path.unwrap_or_else(|| std::path::Path::new("config.json"));
        match std::fs::read_to_string(path) {
            Ok(content) => {
                match serde_json::from_str::<Config>(&content) {
                    Ok(mut config) => {
                        config.db_path = expand_tilde(&config.db_path.to_string_lossy());
                        config.claude_projects_dir = expand_tilde(&config.claude_projects_dir.to_string_lossy());
                        config.db_paths = config
                            .db_paths
                            .into_iter()
                            .map(|p| expand_tilde(&p.to_string_lossy()))
                            .collect();
                        config.pi_jsonl_dir = config.pi_jsonl_dir
                            .map(|p| expand_tilde(&p.to_string_lossy()));
                        config
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to parse {:?}, using defaults: {}", path, e);
                        Config::default()
                    }
                }
            }
            Err(_) => {
                // File not found — use defaults silently
                Config::default()
            }
        }
    }

    fn default() -> Self {
        let mut c = Config {
            db_paths: Vec::new(),
            db_path: default_db_path(),
            claude_projects_dir: default_claude_dir(),
            pi_jsonl_dir: None,
            host: default_host(),
            port: default_port(),
        };
        c.db_path = expand_tilde(&c.db_path.to_string_lossy());
        c.claude_projects_dir = expand_tilde(&c.claude_projects_dir.to_string_lossy());
        c
    }

    /// Returns all DB paths to write to on import.
    /// Uses `db_paths` when set; falls back to the single `db_path`.
    pub fn effective_db_paths(&self) -> Vec<&PathBuf> {
        if self.db_paths.is_empty() {
            vec![&self.db_path]
        } else {
            self.db_paths.iter().collect()
        }
    }

    /// Returns the primary DB path (first in list), used by `serve`.
    pub fn primary_db_path(&self) -> &PathBuf {
        self.db_paths.first().unwrap_or(&self.db_path)
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
